//! OpenCode console (opencode.ai/console) web API adapter.
//!
//! The Zen console serves first-party JSON behind the browser session cookie:
//!
//! * `GET /console/api/orgs` — workspaces the session can read
//! * `GET /console/api/go/status` — Zen Go quota meters (`x-org-id` scope)
//! * `GET /console/api/billing/status` — billing mode and prepaid balance
//!
//! This module is self-contained on purpose: the caller supplies the origin (so
//! tests can point at a local mock server), the cookie header and the timeout.
//! No cookie, environment or browser lookup happens here, and response bodies
//! are never echoed into error messages.

use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde_json::Value;

use crate::core::{ProviderError, RateWindow, UsageSnapshot};

/// Production console origin.
pub(super) const DEFAULT_ORIGIN: &str = "https://opencode.ai";

/// A workspace without an active Zen subscription has no meters at all. The
/// caller treats this as "no console data" and keeps its other sources.
pub(super) const NO_SUBSCRIPTION_MESSAGE: &str =
    "OpenCode console workspace has no Zen subscription";

const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
     (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

const ORG_LIST_PATH: &str = "/console/api/orgs";
const GO_STATUS_PATH: &str = "/console/api/go/status";
const BILLING_STATUS_PATH: &str = "/console/api/billing/status";

/// Longest workspace identifier accepted from settings or a console URL.
const MAX_WORKSPACE_ID_LEN: usize = 128;

const MICRO_CENTS_PER_UNIT: f64 = 100_000_000.0;

/// Quota window decoded from `access.meters.*`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ConsoleWindow {
    /// Used share of the window, already clamped to 0..=100.
    pub used_percent: f64,
    /// Absolute reset time when the console reports one.
    pub resets_at: Option<DateTime<Utc>>,
}

/// Zen Go quota as reported by `GET /console/api/go/status`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ConsoleUsage {
    pub five_hour: ConsoleWindow,
    pub week: Option<ConsoleWindow>,
    pub month: Option<ConsoleWindow>,
    /// Subscription renewal timestamp (`access.endsAt`).
    pub renews_at: Option<DateTime<Utc>>,
}

impl ConsoleUsage {
    /// Convert into the shared snapshot model. The 5-hour lane is always the
    /// primary window because Zen Go meters it as the session limit.
    pub(super) fn into_snapshot(self) -> UsageSnapshot {
        let mut usage = UsageSnapshot::new(RateWindow::with_details(
            self.five_hour.used_percent,
            Some(300),
            self.five_hour.resets_at,
            None,
        ))
        .with_login_method("OpenCode");

        if let Some(week) = self.week {
            usage = usage.with_secondary(RateWindow::with_details(
                week.used_percent,
                Some(10080),
                week.resets_at,
                None,
            ));
        }
        if let Some(month) = self.month {
            usage = usage.with_tertiary(RateWindow::with_details(
                month.used_percent,
                Some(43200),
                month.resets_at,
                None,
            ));
        }
        if let Some(renews_at) = self.renews_at {
            usage = usage.with_extra_rate_window(
                "renewal",
                "Renews",
                RateWindow::with_details(0.0, None, Some(renews_at), None),
            );
        }
        usage
    }
}

/// True when `error` is this module's "workspace has no subscription" marker.
pub(super) fn is_no_subscription(error: &ProviderError) -> bool {
    matches!(error, ProviderError::Other(message) if message == NO_SUBSCRIPTION_MESSAGE)
}

// ── Workspace identifiers ────────────────────────────────────────────

/// Accept a bare `wrk_`/`org_` identifier or an `opencode.ai` console URL.
///
/// Settings accept whatever the user pastes, which is often the address bar
/// value (`https://opencode.ai/console/wrk_…/billing`) rather than the bare ID.
pub(super) fn normalize_workspace_id(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        return workspace_id_from_url(trimmed);
    }
    workspace_id_from_value(trimmed)
}

fn workspace_id_from_url(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "opencode.ai" && host != "www.opencode.ai" {
        return None;
    }
    let segments: Vec<&str> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();
    segments.windows(2).find_map(|pair| {
        matches!(pair[0], "console" | "workspace").then(|| workspace_id_from_value(pair[1]))?
    })
}

fn workspace_id_from_value(value: &str) -> Option<String> {
    if value.len() > MAX_WORKSPACE_ID_LEN {
        return None;
    }
    let rest = value
        .strip_prefix("wrk_")
        .or_else(|| value.strip_prefix("org_"))?;
    if rest.is_empty() {
        return None;
    }
    if !rest
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return None;
    }
    Some(value.to_string())
}

/// First valid workspace identifier in an org-list payload.
fn find_workspace_id(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => normalize_workspace_id(text),
        Value::Array(items) => items.iter().find_map(find_workspace_id),
        Value::Object(map) => {
            const ID_KEYS: [&str; 5] = ["id", "orgId", "org_id", "workspaceId", "workspace_id"];
            for key in ID_KEYS {
                if let Some(found) = map.get(key).and_then(find_workspace_id) {
                    return Some(found);
                }
            }
            map.values().find_map(find_workspace_id)
        }
        _ => None,
    }
}

// ── HTTP ─────────────────────────────────────────────────────────────

/// `GET /console/api/orgs` — resolve the workspace the cookie can read.
pub(super) async fn fetch_workspace_id(
    client: &Client,
    origin: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<String, ProviderError> {
    let body = console_get(
        client,
        origin,
        ORG_LIST_PATH,
        None,
        cookie_header,
        timeout,
        "org list",
    )
    .await?;
    let value = parse_json(&body, "org list")?;
    find_workspace_id(&value)
        .ok_or_else(|| ProviderError::Parse("No OpenCode workspace ID found".to_string()))
}

/// `GET /console/api/go/status` — Zen Go quota meters for one workspace.
pub(super) async fn fetch_usage(
    client: &Client,
    origin: &str,
    workspace_id: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<ConsoleUsage, ProviderError> {
    let body = console_get(
        client,
        origin,
        GO_STATUS_PATH,
        Some(workspace_id),
        cookie_header,
        timeout,
        "status",
    )
    .await?;
    parse_go_status(&parse_json(&body, "status")?)
}

/// `GET /console/api/billing/status` — prepaid balance in account currency.
///
/// Only prepaid pay-as-you-go accounts expose a spendable balance; seat and
/// invoiceable plans legitimately have none. Wired best-effort by the caller.
#[allow(dead_code)]
pub(super) async fn fetch_balance(
    client: &Client,
    origin: &str,
    workspace_id: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<Option<f64>, ProviderError> {
    let body = console_get(
        client,
        origin,
        BILLING_STATUS_PATH,
        Some(workspace_id),
        cookie_header,
        timeout,
        "billing status",
    )
    .await?;
    parse_billing_status(&parse_json(&body, "billing status")?)
}

async fn console_get(
    client: &Client,
    origin: &str,
    path: &str,
    org_id: Option<&str>,
    cookie_header: &str,
    timeout: Duration,
    label: &str,
) -> Result<String, ProviderError> {
    let mut request = client
        .get(format!("{}{path}", origin.trim_end_matches('/')))
        .timeout(timeout)
        .header("Cookie", cookie_header)
        .header("Accept", "application/json")
        .header("User-Agent", BROWSER_USER_AGENT);
    if let Some(org_id) = org_id {
        request = request.header("x-org-id", org_id);
    }

    let response = request.send().await?;
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED {
        return Err(ProviderError::AuthRequired);
    }
    if !status.is_success() {
        // Status only: console error bodies can echo account data.
        return Err(ProviderError::Other(format!(
            "OpenCode Console {label} returned {status}"
        )));
    }
    Ok(response.text().await?)
}

fn parse_json(body: &str, label: &str) -> Result<Value, ProviderError> {
    serde_json::from_str(body)
        .map_err(|e| ProviderError::Parse(format!("Invalid OpenCode Console {label} JSON: {e}")))
}

// ── Parsing ──────────────────────────────────────────────────────────

fn parse_go_status(value: &Value) -> Result<ConsoleUsage, ProviderError> {
    let Some(access) = value.get("access").filter(|access| !access.is_null()) else {
        return Err(ProviderError::Other(NO_SUBSCRIPTION_MESSAGE.to_string()));
    };

    let renews_at = access.get("endsAt").and_then(date_from_value);
    let meters = access
        .get("meters")
        .filter(|meters| !meters.is_null())
        .ok_or_else(|| parse_error("access.meters"))?;

    let five_hour = required_window(meters.get("fiveHour"), "access.meters.fiveHour")?;
    let week = optional_window(meters, &["week", "weekly", "weekUsage"])?;
    let month =
        optional_window(meters, &["month", "monthly", "monthUsage"])?.map(|window| ConsoleWindow {
            // Monthly meters do not always carry their own reset stamp; the
            // workspace renewal date is the same boundary for most plans.
            resets_at: window.resets_at.or(renews_at),
            ..window
        });

    Ok(ConsoleUsage {
        five_hour,
        week,
        month,
        renews_at,
    })
}

fn optional_window(meters: &Value, keys: &[&str]) -> Result<Option<ConsoleWindow>, ProviderError> {
    for key in keys {
        if let Some(window) = meters.get(*key).filter(|window| !window.is_null()) {
            return required_window(Some(window), key).map(Some);
        }
    }
    Ok(None)
}

fn required_window(window: Option<&Value>, label: &str) -> Result<ConsoleWindow, ProviderError> {
    let window = window
        .filter(|window| !window.is_null())
        .ok_or_else(|| parse_error(label))?;
    let used = window
        .get("usedMicroCents")
        .and_then(micro_cents)
        .ok_or_else(|| parse_error(label))?;
    let limit = window
        .get("limitMicroCents")
        .and_then(micro_cents)
        .ok_or_else(|| parse_error(label))?;
    if !limit.is_finite() || limit <= 0.0 {
        return Err(parse_error(label));
    }

    // Keep whole percentages intact (0 / 0.5 / 100 stay themselves): the meter
    // pair is already an absolute used/limit ratio, not a 0..=1 fraction.
    let used_percent = (used / limit * 100.0).clamp(0.0, 100.0);
    Ok(ConsoleWindow {
        used_percent,
        resets_at: window_reset_at(window),
    })
}

fn window_reset_at(window: &Value) -> Option<DateTime<Utc>> {
    const RESET_KEYS: [&str; 7] = [
        "resetsAt",
        "resetAt",
        "resets_at",
        "reset_at",
        "endsAt",
        "ends_at",
        "windowEndsAt",
    ];
    RESET_KEYS
        .iter()
        .find_map(|key| window.get(*key).and_then(date_from_value))
}

fn parse_billing_status(value: &Value) -> Result<Option<f64>, ProviderError> {
    let billing_mode = find_string(value, &["billingMode", "billing_mode"])
        .ok_or_else(|| parse_error("billingMode"))?;
    let mode = find_string(value, &["mode"]).ok_or_else(|| parse_error("mode"))?;
    let billing_mode = billing_mode.trim().to_ascii_lowercase();
    let mode = mode.trim().to_ascii_lowercase();

    if !matches!(
        billing_mode.as_str(),
        "prepaid" | "legacy" | "seat" | "credit"
    ) {
        return Err(parse_error("billingMode"));
    }
    if !matches!(mode.as_str(), "pay-as-you-go" | "invoiceable") {
        return Err(parse_error("mode"));
    }
    // Only prepaid pay-as-you-go money is a spendable account balance. Legacy,
    // seat and credit plans report a balance field that is not a wallet.
    if billing_mode != "prepaid" || mode != "pay-as-you-go" {
        return Ok(None);
    }

    let micros = find_value(value, &["balanceMicroCents", "balance_micro_cents"])
        .and_then(micro_cents)
        .ok_or_else(|| parse_error("balanceMicroCents"))?;
    Ok(Some(micros / MICRO_CENTS_PER_UNIT))
}

/// Parse a micro-cent amount that may be a JSON number or a numeric string.
///
/// Negative values are valid: a prepaid wallet can go into arrears before the
/// console blocks it.
fn micro_cents(value: &Value) -> Option<f64> {
    let parsed = match value {
        Value::Number(_) => value.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    parsed.is_finite().then_some(parsed)
}

/// Find the first string value for any of `keys`, preferring the current object.
fn find_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    find_value(value, keys).and_then(Value::as_str)
}

fn find_value<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(found) = map.get(*key).filter(|found| !found.is_null()) {
                    return Some(found);
                }
            }
            map.values().find_map(|nested| find_value(nested, keys))
        }
        Value::Array(items) => items.iter().find_map(|nested| find_value(nested, keys)),
        _ => None,
    }
}

/// RFC3339 strings and second/millisecond epoch numbers are both accepted.
fn date_from_value(value: &Value) -> Option<DateTime<Utc>> {
    match value {
        Value::Number(_) => value.as_f64().and_then(date_from_epoch),
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return None;
            }
            if let Ok(number) = text.parse::<f64>() {
                return date_from_epoch(number);
            }
            DateTime::parse_from_rfc3339(text)
                .ok()
                .map(|date| date.with_timezone(&Utc))
        }
        _ => None,
    }
}

fn date_from_epoch(number: f64) -> Option<DateTime<Utc>> {
    if !number.is_finite() || number <= 0.0 {
        return None;
    }
    // Anything past 2286 in seconds is really a millisecond stamp.
    let seconds = if number > 10_000_000_000.0 {
        number / 1000.0
    } else {
        number
    };
    DateTime::<Utc>::from_timestamp(seconds as i64, 0)
}

fn parse_error(field: &str) -> ProviderError {
    ProviderError::Parse(format!("Invalid OpenCode Console {field}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;

    fn fixture() -> Value {
        serde_json::json!({
            "access": {
                "endsAt": "2026-12-01T00:00:00Z",
                "meters": {
                    "fiveHour": {
                        "usedMicroCents": "10000000",
                        "limitMicroCents": 40000000,
                        "resetsAt": "2026-11-02T18:00:00Z"
                    },
                    "week": {
                        "usedMicroCents": 2500000,
                        "limitMicroCents": 25000000
                    }
                }
            }
        })
    }

    #[test]
    fn normalizes_workspace_ids_and_console_urls() {
        for (raw, expected) in [
            ("wrk_abc123", "wrk_abc123"),
            ("org_abc-123", "org_abc-123"),
            ("  wrk_abc123  ", "wrk_abc123"),
            (
                "https://opencode.ai/console/wrk_abc123/billing",
                "wrk_abc123",
            ),
            ("https://opencode.ai/workspace/org_abc-123", "org_abc-123"),
        ] {
            assert_eq!(
                normalize_workspace_id(raw).as_deref(),
                Some(expected),
                "raw={raw}"
            );
        }

        for raw in [
            "",
            "wrk_",
            "abc123",
            "https://evil.test/console/wrk_abc123",
            "http://opencode.ai/console/wrk_abc123",
            "https://opencode.ai/console/",
            "https://opencode.ai/console/wrk_bad/../x",
            "wrk_bad id",
            "https://opencode.ai.evil.test/console/wrk_abc123",
        ] {
            assert_eq!(normalize_workspace_id(raw), None, "raw={raw}");
        }
    }

    #[test]
    fn finds_first_workspace_id_in_org_list() {
        let payload = serde_json::json!({
            "orgs": [
                { "id": "invalid" },
                { "id": "org_second", "slug": "acme" },
                { "id": "wrk_third" }
            ]
        });
        assert_eq!(find_workspace_id(&payload).as_deref(), Some("org_second"));
        assert_eq!(find_workspace_id(&serde_json::json!({ "orgs": [] })), None);
        assert_eq!(
            find_workspace_id(&serde_json::json!(r#"{"data":[{"id":"wrk_only"}]}"#)),
            None,
            "a JSON string is not an org list"
        );
    }

    #[test]
    fn computes_micro_cent_percentages() {
        let usage = parse_go_status(&fixture()).expect("usage");
        assert!((usage.five_hour.used_percent - 25.0).abs() < 1e-9);
        assert_eq!(
            usage.five_hour.resets_at.map(|at| at.to_rfc3339()),
            Some("2026-11-02T18:00:00+00:00".to_string())
        );
        let week = usage.week.expect("week window");
        assert!((week.used_percent - 10.0).abs() < 1e-9);
        assert!(week.resets_at.is_none());
        assert!(usage.month.is_none());
    }

    #[test]
    fn keeps_whole_and_fractional_percentages_unscaled() {
        for (used, limit, expected) in [
            (0.0, 40.0, 0.0),
            (0.2, 40.0, 0.5),
            (40.0, 40.0, 100.0),
            (80.0, 40.0, 100.0),
        ] {
            let payload = serde_json::json!({
                "access": {
                    "meters": {
                        "fiveHour": {
                            "usedMicroCents": used,
                            "limitMicroCents": limit
                        }
                    }
                }
            });
            let usage = parse_go_status(&payload).expect("usage");
            assert!(
                (usage.five_hour.used_percent - expected).abs() < 1e-9,
                "used={used} limit={limit} got {}",
                usage.five_hour.used_percent
            );
        }
    }

    #[test]
    fn null_access_means_no_subscription() {
        let error = parse_go_status(&serde_json::json!({ "access": null })).unwrap_err();
        assert!(is_no_subscription(&error), "got {error:?}");
        let missing = parse_go_status(&serde_json::json!({})).unwrap_err();
        assert!(is_no_subscription(&missing));
    }

    #[test]
    fn rejects_missing_meters_used_or_limit() {
        for payload in [
            serde_json::json!({ "access": {} }),
            serde_json::json!({ "access": { "meters": {} } }),
            serde_json::json!({ "access": { "meters": { "fiveHour": {} } } }),
            serde_json::json!({
                "access": { "meters": { "fiveHour": { "limitMicroCents": 1000 } } }
            }),
            serde_json::json!({
                "access": { "meters": { "fiveHour": { "usedMicroCents": 10 } } }
            }),
            serde_json::json!({
                "access": {
                    "meters": { "fiveHour": { "usedMicroCents": 10, "limitMicroCents": 0 } }
                }
            }),
            serde_json::json!({
                "access": {
                    "meters": { "fiveHour": { "usedMicroCents": "nope", "limitMicroCents": 10 } }
                }
            }),
        ] {
            let error = parse_go_status(&payload).unwrap_err();
            assert!(matches!(error, ProviderError::Parse(_)), "got {error:?}");
        }
    }

    #[test]
    fn month_reset_falls_back_to_access_ends_at() {
        let mut payload = fixture();
        payload["access"]["meters"]["month"] = serde_json::json!({
            "usedMicroCents": 5,
            "limitMicroCents": 50
        });
        let usage = parse_go_status(&payload).expect("usage");
        let month = usage.month.expect("month window");
        assert!((month.used_percent - 10.0).abs() < 1e-9);
        assert_eq!(
            month.resets_at.map(|at| at.to_rfc3339()),
            Some("2026-12-01T00:00:00+00:00".to_string())
        );
        assert_eq!(
            usage.renews_at.map(|at| at.to_rfc3339()),
            Some("2026-12-01T00:00:00+00:00".to_string())
        );
    }

    #[test]
    fn accepts_rfc3339_and_epoch_reset_stamps() {
        let mut payload = fixture();
        payload["access"]["meters"]["week"]["resetsAt"] = serde_json::json!(1_796_000_000i64);
        let usage = parse_go_status(&payload).expect("usage");
        assert_eq!(
            usage
                .week
                .and_then(|week| week.resets_at)
                .map(|at| at.timestamp()),
            Some(1_796_000_000)
        );

        payload["access"]["meters"]["week"]["resetsAt"] = serde_json::json!(1_796_000_000_000i64);
        let usage = parse_go_status(&payload).expect("usage");
        assert_eq!(
            usage
                .week
                .and_then(|week| week.resets_at)
                .map(|at| at.timestamp()),
            Some(1_796_000_000)
        );

        payload["access"]["meters"]["week"]["resetsAt"] =
            serde_json::json!("2026-11-09T00:00:00.000Z");
        let usage = parse_go_status(&payload).expect("usage");
        assert_eq!(
            usage
                .week
                .and_then(|week| week.resets_at)
                .map(|at| at.to_rfc3339()),
            Some("2026-11-09T00:00:00+00:00".to_string())
        );
    }

    #[test]
    fn snapshot_keeps_primary_week_month_and_renewal() {
        let mut payload = fixture();
        payload["access"]["meters"]["month"] = serde_json::json!({
            "usedMicroCents": "30000000",
            "limitMicroCents": "60000000",
            "resetsAt": "2026-12-01T00:00:00Z"
        });
        let snapshot = parse_go_status(&payload).expect("usage").into_snapshot();
        assert!((snapshot.primary.used_percent - 25.0).abs() < 1e-9);
        assert!((snapshot.secondary.as_ref().unwrap().used_percent - 10.0).abs() < 1e-9);
        assert!((snapshot.tertiary.as_ref().unwrap().used_percent - 50.0).abs() < 1e-9);
        assert!(snapshot.extra_rate_windows.iter().any(|window| {
            window.id == "renewal" && window.title == "Renews" && window.window.resets_at.is_some()
        }));
    }

    #[test]
    fn parses_prepaid_balance_in_dollars() {
        let balance = parse_billing_status(&serde_json::json!({
            "billingMode": "prepaid",
            "mode": "pay-as-you-go",
            "balanceMicroCents": "2786781005"
        }))
        .expect("billing");
        assert!((balance.expect("balance") - 27.86781005).abs() < 1e-9);
    }

    #[test]
    fn negative_prepaid_balance_is_kept() {
        let balance = parse_billing_status(&serde_json::json!({
            "billingMode": "prepaid",
            "mode": "pay-as-you-go",
            "balanceMicroCents": -150_000_000i64
        }))
        .expect("billing");
        assert!((balance.expect("balance") + 1.5).abs() < 1e-9);
    }

    #[test]
    fn non_prepaid_plans_have_no_balance() {
        for (billing_mode, mode) in [
            ("seat", "pay-as-you-go"),
            ("legacy", "pay-as-you-go"),
            ("credit", "pay-as-you-go"),
            ("prepaid", "invoiceable"),
        ] {
            let balance = parse_billing_status(&serde_json::json!({
                "billingMode": billing_mode,
                "mode": mode,
                "balanceMicroCents": "100"
            }))
            .expect("billing");
            assert!(balance.is_none(), "{billing_mode}/{mode}");
        }
    }

    #[test]
    fn unknown_billing_shape_is_a_parse_error() {
        for payload in [
            serde_json::json!({ "mode": "pay-as-you-go" }),
            serde_json::json!({ "billingMode": "prepaid" }),
            serde_json::json!({ "billingMode": "weird", "mode": "pay-as-you-go" }),
            serde_json::json!({ "billingMode": "prepaid", "mode": "weird" }),
            serde_json::json!({
                "billingMode": "prepaid",
                "mode": "pay-as-you-go",
                "balanceMicroCents": "not-a-number"
            }),
        ] {
            let error = parse_billing_status(&payload).unwrap_err();
            assert!(matches!(error, ProviderError::Parse(_)), "got {error:?}");
        }
    }

    #[tokio::test]
    async fn org_list_reads_first_workspace_without_org_header() {
        let mut server = mockito::Server::new_async().await;
        let missing_org = server
            .mock("GET", ORG_LIST_PATH)
            .match_header("cookie", "session=test")
            .match_header("x-org-id", Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"orgs":[{"id":"wrk_first"},{"id":"wrk_second"}]}"#)
            .create_async()
            .await;

        let client = Client::new();
        let workspace_id = fetch_workspace_id(
            &client,
            &server.url(),
            "session=test",
            Duration::from_secs(5),
        )
        .await
        .expect("workspace id");

        assert_eq!(workspace_id, "wrk_first");
        missing_org.assert_async().await;
    }

    #[tokio::test]
    async fn go_status_and_billing_are_scoped_by_org_header() {
        let mut server = mockito::Server::new_async().await;
        let status = server
            .mock("GET", GO_STATUS_PATH)
            .match_header("cookie", "session=test")
            .match_header("x-org-id", "wrk_abc")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(fixture().to_string())
            .create_async()
            .await;
        let billing = server
            .mock("GET", BILLING_STATUS_PATH)
            .match_header("cookie", "session=test")
            .match_header("x-org-id", "wrk_abc")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"2786781005"}"#,
            )
            .create_async()
            .await;

        let client = Client::new();
        let usage = fetch_usage(
            &client,
            &server.url(),
            "wrk_abc",
            "session=test",
            Duration::from_secs(5),
        )
        .await
        .expect("usage");
        assert!((usage.five_hour.used_percent - 25.0).abs() < 1e-9);

        let balance = fetch_balance(
            &client,
            &server.url(),
            "wrk_abc",
            "session=test",
            Duration::from_secs(5),
        )
        .await
        .expect("billing");
        assert!((balance.expect("balance") - 27.86781005).abs() < 1e-9);

        status.assert_async().await;
        billing.assert_async().await;
    }

    #[tokio::test]
    async fn unauthorized_is_auth_required_and_other_statuses_are_other() {
        let mut server = mockito::Server::new_async().await;
        let unauthorized = server
            .mock("GET", ORG_LIST_PATH)
            .with_status(401)
            .with_body("secret body must not leak")
            .create_async()
            .await;
        let client = Client::new();
        let error = fetch_workspace_id(
            &client,
            &server.url(),
            "session=test",
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ProviderError::AuthRequired));
        unauthorized.assert_async().await;

        for status_code in [403u16, 429] {
            let _mock = server
                .mock("GET", GO_STATUS_PATH)
                .with_status(status_code.into())
                .with_body("secret body must not leak")
                .create_async()
                .await;
            let error = fetch_usage(
                &client,
                &server.url(),
                "wrk_abc",
                "session=test",
                Duration::from_secs(5),
            )
            .await
            .unwrap_err();
            match error {
                ProviderError::Other(message) => {
                    assert!(
                        message.contains("OpenCode Console status returned"),
                        "message={message}"
                    );
                    assert!(!message.contains("secret"), "body leaked: {message}");
                }
                other => panic!("expected Other, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn empty_org_list_is_a_parse_error() {
        let mut server = mockito::Server::new_async().await;
        let _mock = server
            .mock("GET", ORG_LIST_PATH)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"orgs":[]}"#)
            .create_async()
            .await;
        let client = Client::new();
        let error = fetch_workspace_id(
            &client,
            &server.url(),
            "session=test",
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, ProviderError::Parse(_)), "got {error:?}");
    }
}
