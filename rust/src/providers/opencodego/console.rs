//! OpenCode Console (console web app) usage chain.
//!
//! The migrated Console exposes JSON endpoints for organization discovery, Go
//! subscription status and prepaid billing under `/console/api`. They are the
//! current source for the web Cookie ladder; the legacy `/_server` functions
//! and the SSR workspace page remain as fallbacks in `mod.rs`.
//!
//! Every entry point takes `origin` so tests can point the chain at a local
//! mock server; production callers pass `https://opencode.ai`.

use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use serde_json::Value;
use std::time::Duration;

use crate::core::{ProviderError, RateWindow, UsageSnapshot};

use super::USER_AGENT;

const CONSOLE_WORKSPACES_PATH: &str = "/console/api/orgs";
const CONSOLE_GO_STATUS_PATH: &str = "/console/api/go/status";
const CONSOLE_BILLING_STATUS_PATH: &str = "/console/api/billing/status";
const CONSOLE_WORKSPACE_HEADER: &str = "x-org-id";
const BILLING_SCALE: f64 = 100_000_000.0;

/// Console Go status outcome: an authoritative snapshot, or a signed-in
/// account without a Go subscription.
#[derive(Debug)]
pub(super) enum ConsoleUsage {
    Snapshot(Box<UsageSnapshot>),
    NoSubscription,
}

/// Normalize a saved workspace/org scope: a bare `wrk_`/`org_` id or a Console
/// URL that carries one.
pub(super) fn normalize_workspace_id(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if is_workspace_id(raw) {
        return Some(raw.to_string());
    }
    let url = reqwest::Url::parse(raw).ok()?;
    if url.scheme() != "https" || url.host_str() != Some("opencode.ai") {
        return None;
    }
    let segments: Vec<_> = url.path_segments()?.collect();
    let candidate = segments
        .windows(2)
        .find_map(|parts| matches!(parts[0], "console" | "workspace").then_some(parts[1]))?;
    is_workspace_id(candidate).then(|| candidate.to_string())
}

fn is_workspace_id(value: &str) -> bool {
    let Some(suffix) = value
        .strip_prefix("wrk_")
        .or_else(|| value.strip_prefix("org_"))
    else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// `GET /console/api/orgs` — organization discovery. This is the only Console
/// call that must not send `x-org-id`.
pub(super) async fn fetch_workspace_id(
    client: &Client,
    origin: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<String, ProviderError> {
    let text = fetch_text(
        client,
        &format!("{origin}{CONSOLE_WORKSPACES_PATH}"),
        None,
        cookie_header,
        timeout,
        "workspace list",
    )
    .await?;
    parse_workspace_ids(&text)
        .into_iter()
        .next()
        .ok_or_else(|| missing("workspace ID"))
}

/// `GET /console/api/go/status` — authoritative Go meter usage.
pub(super) async fn fetch_usage(
    client: &Client,
    origin: &str,
    workspace_id: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<ConsoleUsage, ProviderError> {
    let text = fetch_text(
        client,
        &format!("{origin}{CONSOLE_GO_STATUS_PATH}"),
        Some(workspace_id),
        cookie_header,
        timeout,
        "Go status",
    )
    .await?;
    parse_usage(&text)
}

/// `GET /console/api/billing/status` — prepaid Zen balance, when the account
/// has one that can be read as USD.
pub(super) async fn fetch_balance(
    client: &Client,
    origin: &str,
    workspace_id: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<Option<f64>, ProviderError> {
    let text = fetch_text(
        client,
        &format!("{origin}{CONSOLE_BILLING_STATUS_PATH}"),
        Some(workspace_id),
        cookie_header,
        timeout,
        "billing status",
    )
    .await?;
    parse_balance(&text)
}

/// GET one Console endpoint. Only the workspace-discovery call omits the
/// `x-org-id` header. A 401 is invalid credentials; any other non-2xx is an
/// API error, because a Console scope or permission failure is not expired
/// auth. Response bodies never reach the error message.
async fn fetch_text(
    client: &Client,
    url: &str,
    workspace_id: Option<&str>,
    cookie_header: &str,
    timeout: Duration,
    what: &str,
) -> Result<String, ProviderError> {
    let mut request = client
        .get(url)
        .timeout(timeout)
        .header("Cookie", cookie_header)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json");
    if let Some(workspace_id) = workspace_id {
        request = request.header(CONSOLE_WORKSPACE_HEADER, workspace_id);
    }
    let response = request.send().await?;
    let status = response.status();
    if status.as_u16() == 401 {
        return Err(ProviderError::AuthRequired);
    }
    if !status.is_success() {
        return Err(ProviderError::Other(format!(
            "OpenCode Console {what} returned {status}"
        )));
    }
    response.text().await.map_err(ProviderError::Network)
}

fn parse_workspace_ids(text: &str) -> Vec<String> {
    let Ok(Value::Array(rows)) = serde_json::from_str(text) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| row.get("id")?.as_str())
        .filter(|id| is_workspace_id(id))
        .map(str::to_string)
        .collect()
}

fn parse_usage(text: &str) -> Result<ConsoleUsage, ProviderError> {
    let root: Value = serde_json::from_str(text).map_err(|_| invalid("usage payload"))?;
    if root.is_null() || root.get("access").is_some_and(Value::is_null) {
        return Ok(ConsoleUsage::NoSubscription);
    }
    let access = root
        .get("access")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("usage payload"))?;
    let meters = access
        .get("meters")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("usage payload"))?;
    let rolling = meters
        .get("fiveHour")
        .ok_or_else(|| invalid("usage payload"))?;
    let ends_at = access.get("endsAt").and_then(date_value);
    let primary = meter_window(rolling, Some(300), None)?;
    let mut snapshot = UsageSnapshot::new(primary).with_login_method("OpenCode Go");

    if let Some(weekly) = meters.get("week").filter(|value| !value.is_null()) {
        snapshot = snapshot.with_secondary(meter_window(weekly, Some(10_080), None)?);
    }
    if let Some(monthly) = meters.get("month").filter(|value| !value.is_null()) {
        let resets_at = monthly.get("resetsAt").and_then(date_value).or(ends_at);
        let minutes = RateWindow::monthly_window_minutes(resets_at).or(Some(43_200));
        snapshot = snapshot.with_tertiary(meter_window(monthly, minutes, resets_at)?);
    }
    if let Some(ends_at) = ends_at {
        snapshot = snapshot.with_extra_rate_window(
            "renewal",
            "Renews",
            RateWindow::with_details(0.0, None, Some(ends_at), None),
        );
    }
    Ok(ConsoleUsage::Snapshot(Box::new(snapshot)))
}

fn meter_window(
    meter: &Value,
    window_minutes: Option<u32>,
    reset_override: Option<DateTime<Utc>>,
) -> Result<RateWindow, ProviderError> {
    let object = meter.as_object().ok_or_else(|| invalid("usage meter"))?;
    let used =
        numeric_value(object.get("usedMicroCents")).ok_or_else(|| missing("usage amount"))?;
    // A zero/absent limit cannot produce a percentage; never invent 0% or 100%.
    let limit = numeric_value(object.get("limitMicroCents"))
        .filter(|value| *value > 0.0)
        .ok_or_else(|| missing("usage limit"))?;
    let resets_at = reset_override.or_else(|| object.get("resetsAt").and_then(date_value));
    Ok(RateWindow::with_details(
        ((used / limit) * 100.0).clamp(0.0, 100.0),
        window_minutes,
        resets_at,
        None,
    ))
}

fn parse_balance(text: &str) -> Result<Option<f64>, ProviderError> {
    let root: Value = serde_json::from_str(text).map_err(|_| invalid("billing payload"))?;
    let billing_mode = root
        .get("billingMode")
        .and_then(Value::as_str)
        .filter(|mode| matches!(*mode, "prepaid" | "legacy" | "seat" | "credit"))
        .ok_or_else(|| invalid("billing payload"))?;
    let mode = root
        .get("mode")
        .and_then(Value::as_str)
        .filter(|mode| matches!(*mode, "pay-as-you-go" | "invoiceable"))
        .ok_or_else(|| invalid("billing payload"))?;
    // Only a prepaid pay-as-you-go account reports a spendable USD balance.
    if billing_mode != "prepaid" || mode != "pay-as-you-go" {
        return Ok(None);
    }
    let raw = root
        .get("balanceMicroCents")
        .and_then(Value::as_str)
        .ok_or_else(|| missing("balance"))?;
    let digits = raw.strip_prefix('-').unwrap_or(raw);
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid("balance"));
    }
    let balance = raw.parse::<f64>().map_err(|_| invalid("balance"))?;
    if !balance.is_finite() {
        return Err(invalid("balance"));
    }
    Ok(Some(balance / BILLING_SCALE))
}

fn invalid(what: &str) -> ProviderError {
    ProviderError::Parse(format!("Invalid OpenCode Console {what}"))
}

fn missing(what: &str) -> ProviderError {
    ProviderError::Parse(format!("Missing OpenCode Console {what}"))
}

fn numeric_value(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(number) => number.as_f64().filter(|value| value.is_finite()),
        Value::String(text) => text.parse::<f64>().ok().filter(|value| value.is_finite()),
        _ => None,
    }
}

/// Console timestamps are RFC3339 strings or epoch seconds/milliseconds.
fn date_value(value: &Value) -> Option<DateTime<Utc>> {
    if let Some(number) = numeric_value(Some(value)) {
        let seconds = if number > 1_000_000_000_000.0 {
            number / 1000.0
        } else {
            number
        };
        let whole = seconds.trunc() as i64;
        let nanos = ((seconds.fract()) * 1_000_000_000.0).round() as u32;
        return Utc.timestamp_opt(whole, nanos).single();
    }
    DateTime::parse_from_rfc3339(value.as_str()?)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::{Matcher, Server};

    const COOKIE: &str = "session=abc";

    fn usage_json(five_hour_reset: &str) -> String {
        format!(
            r#"{{"access":{{"endsAt":"2026-10-19T00:00:00Z","meters":{{"fiveHour":{{"resetsAt":{five_hour_reset},"limitMicroCents":"1200000000","usedMicroCents":"300000000"}},"week":{{"resetsAt":"2026-09-21T00:00:00Z","limitMicroCents":"3000000000","usedMicroCents":"1200000000"}},"month":{{"limitMicroCents":"6000000000","usedMicroCents":"600000000"}}}}}}}}"#
        )
    }

    fn snapshot(usage: ConsoleUsage) -> Box<UsageSnapshot> {
        match usage {
            ConsoleUsage::Snapshot(snapshot) => snapshot,
            ConsoleUsage::NoSubscription => panic!("expected a usage snapshot"),
        }
    }

    #[test]
    fn usage_meters_map_to_percent_of_their_limits() {
        let snapshot = snapshot(parse_usage(&usage_json("null")).unwrap());
        assert!((snapshot.primary.used_percent - 25.0).abs() < 0.001);
        assert_eq!(snapshot.primary.window_minutes, Some(300));
        assert_eq!(snapshot.primary.resets_at, None);
        assert!((snapshot.secondary.as_ref().unwrap().used_percent - 40.0).abs() < 0.001);
        let monthly = snapshot.tertiary.as_ref().unwrap();
        assert!((monthly.used_percent - 10.0).abs() < 0.001);
        assert_eq!(
            monthly.resets_at.unwrap().to_rfc3339(),
            "2026-10-19T00:00:00+00:00",
            "month reset falls back to access.endsAt"
        );
        let renewal = snapshot
            .extra_rate_windows
            .iter()
            .find(|window| window.id == "renewal")
            .expect("renewal window");
        assert_eq!(renewal.title, "Renews");
        assert_eq!(
            renewal.window.resets_at.unwrap().to_rfc3339(),
            "2026-10-19T00:00:00+00:00"
        );
    }

    #[test]
    fn meter_percentages_keep_zero_half_and_full_bounds() {
        for (used, expected) in [("0", 0.0), ("5000", 0.5), ("1000000", 100.0)] {
            let text = format!(
                r#"{{"access":{{"meters":{{"fiveHour":{{"limitMicroCents":"1000000","usedMicroCents":"{used}"}}}}}}}}"#
            );
            let snapshot = snapshot(parse_usage(&text).unwrap());
            assert!(
                (snapshot.primary.used_percent - expected).abs() < 0.001,
                "used={used} expected {expected}%, got {}",
                snapshot.primary.used_percent
            );
        }
    }

    #[test]
    fn no_subscription_is_distinct_from_malformed_payloads() {
        assert!(matches!(
            parse_usage("null").unwrap(),
            ConsoleUsage::NoSubscription
        ));
        assert!(matches!(
            parse_usage(r#"{"access":null}"#).unwrap(),
            ConsoleUsage::NoSubscription
        ));

        let malformed = [
            "not json",
            r#"{"access":5}"#,
            r#"{"access":{}}"#,
            r#"{"access":{"meters":{}}}"#,
            r#"{"access":{"meters":{"fiveHour":null}}}"#,
            r#"{"access":{"meters":{"fiveHour":5}}}"#,
            r#"{"access":{"meters":{"fiveHour":{}}}}"#,
            r#"{"access":{"meters":{"fiveHour":{"usedMicroCents":"1"}}}}"#,
            r#"{"access":{"meters":{"fiveHour":{"usedMicroCents":"1","limitMicroCents":"0"}}}}"#,
            r#"{"access":{"meters":{"fiveHour":{"usedMicroCents":"1","limitMicroCents":"-5"}}}}"#,
        ];
        for text in malformed {
            assert!(
                matches!(parse_usage(text), Err(ProviderError::Parse(_))),
                "{text} must be a parse error"
            );
        }
    }

    #[test]
    fn optional_windows_may_be_absent() {
        let text =
            r#"{"access":{"meters":{"fiveHour":{"limitMicroCents":"100","usedMicroCents":"50"}}}}"#;
        let snapshot = snapshot(parse_usage(text).unwrap());
        assert!((snapshot.primary.used_percent - 50.0).abs() < 0.001);
        assert!(snapshot.secondary.is_none());
        assert!(snapshot.tertiary.is_none());
        assert!(snapshot.extra_rate_windows.is_empty());
    }

    #[test]
    fn null_optional_meters_read_as_absent() {
        // The console sends explicit nulls for windows the plan has no meter
        // for; they must not become 0% rows.
        let text = r#"{"access":{"endsAt":"2026-10-19T00:00:00Z","meters":{"fiveHour":{"limitMicroCents":"100","usedMicroCents":"25"},"week":null,"month":null}}}"#;
        let snapshot = snapshot(parse_usage(text).unwrap());
        assert!((snapshot.primary.used_percent - 25.0).abs() < 0.001);
        assert!(snapshot.secondary.is_none(), "week:null is not a window");
        assert!(snapshot.tertiary.is_none(), "month:null is not a window");
        assert!(
            snapshot
                .extra_rate_windows
                .iter()
                .any(|window| window.id == "renewal")
        );
    }

    #[test]
    fn resets_accept_rfc3339_and_epoch_numbers() {
        let rfc = snapshot(parse_usage(&usage_json(r#""2026-09-20T04:00:00Z""#)).unwrap());
        assert_eq!(
            rfc.primary.resets_at.unwrap().to_rfc3339(),
            "2026-09-20T04:00:00+00:00"
        );

        let seconds = snapshot(parse_usage(&usage_json("1790000000")).unwrap());
        assert_eq!(
            seconds.primary.resets_at.unwrap().timestamp(),
            1_790_000_000
        );

        let millis = snapshot(parse_usage(&usage_json("1790000000000")).unwrap());
        assert_eq!(millis.primary.resets_at.unwrap().timestamp(), 1_790_000_000);
    }

    #[test]
    fn only_prepaid_pay_as_you_go_balances_are_values() {
        let payload =
            r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"2786781005"}"#;
        let balance = parse_balance(payload).unwrap().unwrap();
        assert!((balance - 27.867_810_05).abs() < 0.000_001);

        // A negative balance is still a real balance.
        assert_eq!(
            parse_balance(
                r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"-100000000"}"#
            )
            .unwrap(),
            Some(-1.0)
        );

        for text in [
            r#"{"billingMode":"seat","mode":"pay-as-you-go","balanceMicroCents":"100"}"#,
            r#"{"billingMode":"credit","mode":"pay-as-you-go","balanceMicroCents":"100"}"#,
            r#"{"billingMode":"legacy","mode":"invoiceable","balanceMicroCents":"100"}"#,
            r#"{"billingMode":"prepaid","mode":"invoiceable","balanceMicroCents":"100"}"#,
        ] {
            assert_eq!(parse_balance(text).unwrap(), None, "{text}");
        }

        for text in [
            "not json",
            r#"{"billingMode":"other","mode":"pay-as-you-go"}"#,
            r#"{"billingMode":"prepaid"}"#,
            r#"{"billingMode":"prepaid","mode":"pay-as-you-go"}"#,
            r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"12a"}"#,
            r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"1.5"}"#,
        ] {
            assert!(
                matches!(parse_balance(text), Err(ProviderError::Parse(_))),
                "{text} must be a parse error"
            );
        }
    }

    #[test]
    fn workspace_ids_and_console_urls_normalize() {
        assert_eq!(
            parse_workspace_ids(r#"[{"id":"wrk_ONE"},{"id":"org_TWO"},{"id":"acc_BAD"}]"#),
            vec!["wrk_ONE", "org_TWO"]
        );
        assert_eq!(
            parse_workspace_ids(r#"{"id":"wrk_ONE"}"#),
            Vec::<String>::new()
        );
        assert_eq!(
            normalize_workspace_id(Some("wrk_ONE")).as_deref(),
            Some("wrk_ONE")
        );
        assert_eq!(
            normalize_workspace_id(Some("https://opencode.ai/console/org_TWO/go")).as_deref(),
            Some("org_TWO")
        );
        assert_eq!(
            normalize_workspace_id(Some("https://opencode.ai/workspace/wrk_ONE/go")).as_deref(),
            Some("wrk_ONE")
        );
        for rejected in [
            "https://example.com/console/org_TWO/go",
            "http://opencode.ai/console/org_TWO/go",
            "https://opencode.ai/console/acc_BAD/go",
            "wrk_",
            "organization",
            "",
        ] {
            assert_eq!(normalize_workspace_id(Some(rejected)), None, "{rejected}");
        }
        assert_eq!(normalize_workspace_id(None), None);
    }

    #[tokio::test]
    async fn console_chain_sends_x_org_id_only_after_discovery() {
        let mut server = Server::new_async().await;
        let orgs = server
            .mock("GET", CONSOLE_WORKSPACES_PATH)
            .match_header("cookie", COOKIE)
            .match_header("accept", "application/json")
            .match_header(CONSOLE_WORKSPACE_HEADER, Matcher::Missing)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"[{"id":"wrk_ONE"}]"#)
            .create_async()
            .await;
        let status = server
            .mock("GET", CONSOLE_GO_STATUS_PATH)
            .match_header("cookie", COOKIE)
            .match_header(CONSOLE_WORKSPACE_HEADER, "wrk_ONE")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(usage_json(r#""2026-09-20T04:00:00Z""#))
            .create_async()
            .await;
        let billing = server
            .mock("GET", CONSOLE_BILLING_STATUS_PATH)
            .match_header("cookie", COOKIE)
            .match_header(CONSOLE_WORKSPACE_HEADER, "wrk_ONE")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"billingMode":"prepaid","mode":"pay-as-you-go","balanceMicroCents":"2786781005"}"#)
            .create_async()
            .await;

        let client = Client::new();
        let origin = server.url();
        let timeout = Duration::from_secs(5);
        let workspace_id = fetch_workspace_id(&client, &origin, COOKIE, timeout)
            .await
            .expect("workspace discovery");
        assert_eq!(workspace_id, "wrk_ONE");

        let snapshot = snapshot(
            fetch_usage(&client, &origin, &workspace_id, COOKIE, timeout)
                .await
                .expect("go status"),
        );
        assert!((snapshot.primary.used_percent - 25.0).abs() < 0.001);

        let balance = fetch_balance(&client, &origin, &workspace_id, COOKIE, timeout)
            .await
            .expect("billing status");
        assert!((balance.unwrap() - 27.867_810_05).abs() < 0.000_001);

        orgs.assert_async().await;
        status.assert_async().await;
        billing.assert_async().await;
    }

    async fn status_error(status: usize) -> ProviderError {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", CONSOLE_GO_STATUS_PATH)
            .with_status(status)
            .with_body("secret response body")
            .create_async()
            .await;
        let error = fetch_usage(
            &Client::new(),
            &server.url(),
            "wrk_ONE",
            COOKIE,
            Duration::from_secs(5),
        )
        .await
        .expect_err("non-2xx must fail");
        mock.assert_async().await;
        error
    }

    #[tokio::test]
    async fn console_status_errors_are_classified_without_bodies() {
        assert!(matches!(
            status_error(401).await,
            ProviderError::AuthRequired
        ));
        for (status, label) in [(403, "403"), (429, "429"), (500, "500")] {
            match status_error(status).await {
                ProviderError::Other(message) => {
                    assert!(
                        message.contains(label),
                        "status {status} should be reported, got {message}"
                    );
                    assert!(
                        !message.contains("secret response body"),
                        "response bodies must not leak: {message}"
                    );
                }
                error => panic!("status {status} should be Other, got {error:?}"),
            }
        }
    }

    #[tokio::test]
    async fn workspace_discovery_without_ids_is_a_parse_error() {
        let mut server = Server::new_async().await;
        let mock = server
            .mock("GET", CONSOLE_WORKSPACES_PATH)
            .with_status(200)
            .with_body(r#"[]"#)
            .create_async()
            .await;
        let error = fetch_workspace_id(
            &Client::new(),
            &server.url(),
            COOKIE,
            Duration::from_secs(5),
        )
        .await
        .expect_err("empty org list must not resolve a workspace");
        mock.assert_async().await;
        assert!(matches!(error, ProviderError::Parse(_)));
    }
}
