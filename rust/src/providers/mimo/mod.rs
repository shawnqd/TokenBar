//! Xiaomi MiMo provider implementation.
//!
//! Uses browser cookies to read Xiaomi MiMo Token Plan usage.
//!
//! Display contract (upstream b585d48 + token-monitor parity):
//! - primary = the `month_total_token` plan quota (calendar-month window);
//! - secondary = the shared console wallet as a balance-carrier description,
//!   which the frontend renders as the balance block instead of a quota bar;
//! - plan state is classified explicitly (active / expired / no plan) and a
//!   missing plan or missing usage row stays informational — a fabricated 0%
//!   must never read as untouched quota.

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

pub(crate) const MIMO_API_BASE: &str = "https://platform.xiaomimimo.com/api/v1";

/// Description the frontend (`providerBalance.ts`) matches to detect "no
/// Token Plan" and switch the card to balance-only mode. Keep byte-identical.
const MIMO_NO_PLAN_MARKER: &str = "No active MiMo Token Plan";

/// The console serves Beijing-wall-clock timestamps and the requests pin the
/// account timezone with the `x-timeZone` header.
const MIMO_REQUEST_TIMEZONE_OFFSET_HOURS: i32 = 8;

pub struct MiMoProvider {
    metadata: ProviderMetadata,
    client: Client,
    api_base: String,
}

#[derive(Debug, Deserialize)]
struct TokenPlanDetailResponse {
    code: i64,
    data: Option<TokenPlanDetailData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenPlanDetailData {
    plan_code: Option<String>,
    current_period_end: Option<String>,
    #[serde(default)]
    expired: bool,
    /// Optional status fields token-monitor also accepts; the console does
    /// not always ship `expired`/period data on every response shape.
    #[serde(default, alias = "plan_status")]
    plan_status: Option<String>,
    #[serde(default, alias = "subscriptionStatus", alias = "subscription_status")]
    subscription_status: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default, alias = "isActive")]
    is_active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct TokenPlanUsageResponse {
    code: i64,
    data: Option<TokenPlanUsageData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenPlanUsageData {
    month_usage: Option<TokenPlanMonthUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenPlanMonthUsage {
    #[serde(default)]
    items: Vec<TokenPlanUsageItem>,
    /// Some shapes report the month totals on the month object itself.
    #[serde(default)]
    used: Option<i64>,
    #[serde(default)]
    limit: Option<i64>,
    #[serde(default)]
    percent: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct TokenPlanUsageItem {
    #[serde(default)]
    name: Option<String>,
    used: i64,
    limit: i64,
    percent: f64,
}

/// The control-console balance is shared by Token Plan and pay-as-you-go
/// usage. Keep it on the Cookie-backed MiMo provider so the application needs
/// only one MiMo card and one login session.
#[derive(Debug, Deserialize)]
struct BalanceResponse {
    code: i64,
    data: Option<BalanceData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BalanceData {
    balance: NumericValue,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    cash_balance: Option<NumericValue>,
    #[serde(default)]
    gift_balance: Option<NumericValue>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NumericValue {
    Number(f64),
    String(String),
}

impl NumericValue {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::String(value) => value.trim().parse::<f64>().ok(),
        }
    }
}

impl MiMoProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::MiMo,
                display_name: "MiMo",
                session_label: "Tokens",
                weekly_label: "Plan",
                supports_opus: false,
                supports_credits: true,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://platform.xiaomimimo.com/#/console/balance"),
                status_page_url: None,
            },
            client: crate::core::credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
            api_base: MIMO_API_BASE.to_string(),
        }
    }

    #[cfg(test)]
    fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into();
        self
    }

    async fn fetch_usage_with_browser_getter<F>(
        &self,
        ctx: &FetchContext,
        get_browser_cookie: F,
    ) -> Result<ProviderFetchResult, ProviderError>
    where
        F: FnOnce() -> Result<String, ProviderError>,
    {
        match ctx.source_mode {
            SourceMode::Auto | SourceMode::Web => {}
            SourceMode::OAuth | SourceMode::Cli => {
                return Err(ProviderError::UnsupportedSource(ctx.source_mode));
            }
        }

        // Explicit manual mode (or token-override mode where allow_browser_cookie is false):
        // must use only the saved/imported/token Cookie and must not probe browser storage.
        if !ctx.allow_browser_cookie {
            let cookie = ctx
                .manual_cookie_header
                .as_deref()
                .or(ctx.fallback_cookie_header.as_deref())
                .ok_or(ProviderError::NoCookies)?;
            let snapshot = self.fetch_web(cookie).await?;
            return Ok(ProviderFetchResult::new(snapshot, "web"));
        }

        // Auto mode (browser-first):
        // Try the current browser Cookie first.
        let fallback_cookie = ctx
            .fallback_cookie_header
            .as_deref()
            .or(ctx.manual_cookie_header.as_deref());

        let browser_result = get_browser_cookie();
        let browser_cookie = match &browser_result {
            Ok(raw) => {
                if normalize_cookie_header(raw).is_some() {
                    Some(raw.as_str())
                } else {
                    None
                }
            }
            Err(_) => None,
        };

        if let Some(cookie) = browser_cookie {
            match self.fetch_web(cookie).await {
                Ok(snapshot) => {
                    // Browser succeeds: never replace it with stale saved Cookie.
                    return Ok(ProviderFetchResult::new(snapshot, "web"));
                }
                Err(ProviderError::AuthRequired) => {
                    // Only an explicit auth rejection indicates that this browser
                    // session is stale. Do not silently switch accounts after a
                    // transient server/network failure.
                    if let Some(saved) = fallback_cookie {
                        let snapshot = self.fetch_web(saved).await?;
                        return Ok(ProviderFetchResult::new(snapshot, "web"));
                    }
                    return Err(ProviderError::AuthRequired);
                }
                Err(browser_err) => return Err(browser_err),
            }
        } else {
            // Browser acquisition or validation failed (absent or unusable browser session).
            // Fall back to saved/imported Cookie if available. When that fallback is also
            // auth-rejected, keep the browser-read cause visible: a locked database or an
            // unreadable profile is the actionable condition, and a bare "authentication
            // required" hides it from the card.
            let browser_err = browser_result.err().unwrap_or(ProviderError::NoCookies);
            let Some(saved) = fallback_cookie else {
                return Err(browser_err);
            };
            match self.fetch_web(saved).await {
                Ok(snapshot) => Ok(ProviderFetchResult::new(snapshot, "web")),
                Err(ProviderError::AuthRequired) => {
                    if matches!(browser_err, ProviderError::NoCookies) {
                        // No browser session exists for this domain at all; the
                        // plain auth category is accurate and actionable.
                        return Err(ProviderError::AuthRequired);
                    }
                    Err(ProviderError::AuthUnavailable(format!(
                        "the browser session could not be read ({browser_err}); \
                         the saved MiMo cookie was also rejected. \
                         Close the browser or import a fresh cookie, then refresh."
                    )))
                }
                Err(other) => Err(other),
            }
        }
    }

    async fn fetch_web(&self, cookie_header: &str) -> Result<UsageSnapshot, ProviderError> {
        let cookie = normalize_cookie_header(cookie_header).ok_or(ProviderError::NoCookies)?;
        let detail: Option<TokenPlanDetailResponse> =
            match self.get_json("tokenPlan/detail", &cookie).await {
                Ok(response) => Some(response),
                Err(ProviderError::AuthRequired) => return Err(ProviderError::AuthRequired),
                Err(_) => None,
            };
        let usage: Option<TokenPlanUsageResponse> =
            match self.get_json("tokenPlan/usage", &cookie).await {
                Ok(response) => Some(response),
                Err(ProviderError::AuthRequired) => return Err(ProviderError::AuthRequired),
                Err(_) => None,
            };
        let balance = match self.get_json::<BalanceResponse>("balance", &cookie).await {
            Ok(response) => balance_description_from_response(response),
            Err(ProviderError::AuthRequired) => return Err(ProviderError::AuthRequired),
            // Token Plan usage remains useful if the balance endpoint has a
            // temporary server-side issue, so do not fail the whole card.
            Err(_) => None,
        };
        Ok(snapshot_from_plan(detail, usage, balance))
    }

    async fn get_json<T: for<'de> Deserialize<'de>>(
        &self,
        path: &str,
        cookie: &str,
    ) -> Result<T, ProviderError> {
        let response = self
            .client
            .get(format!("{}/{path}", self.api_base))
            .header("Cookie", cookie)
            .header("Accept", "application/json, text/plain, */*")
            .header("Origin", "https://platform.xiaomimimo.com")
            .header(
                "Referer",
                "https://platform.xiaomimimo.com/#/console/balance",
            )
            .header("x-timeZone", "UTC+08:00")
            .send()
            .await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED
            || response.status() == reqwest::StatusCode::FORBIDDEN
        {
            return Err(ProviderError::AuthRequired);
        }
        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "MiMo API returned status {}",
                response.status()
            )));
        }
        response
            .json::<T>()
            .await
            .map_err(|e| ProviderError::Parse(format!("Failed to parse MiMo response: {e}")))
    }
}

pub(crate) fn normalize_cookie_header(raw: &str) -> Option<String> {
    // Tolerate a pasted request header ("Cookie: a=b; c=d") the same way the
    // ollama normalizer does — console copy flows usually include the prefix.
    let raw = raw.trim();
    let raw = raw
        .get(.."cookie:".len())
        .filter(|prefix| prefix.eq_ignore_ascii_case("cookie:"))
        .map(|_| raw["cookie:".len()..].trim())
        .unwrap_or(raw);
    let known = [
        "api-platform_serviceToken",
        "userId",
        "api-platform_ph",
        "api-platform_slh",
    ];
    let required = ["api-platform_serviceToken", "userId"];
    let mut pairs = Vec::new();
    for chunk in raw.trim().split(';') {
        let Some((name, value)) = chunk.trim().split_once('=') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if known.contains(&name) && !value.is_empty() {
            pairs.push((name.to_string(), value.to_string()));
        }
    }
    if required
        .iter()
        .all(|required| pairs.iter().any(|(name, _)| name == required))
    {
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        Some(
            pairs
                .into_iter()
                .map(|(name, value)| format!("{name}={value}"))
                .collect::<Vec<_>>()
                .join("; "),
        )
    } else {
        None
    }
}

/// Plan-state classification (token-monitor parity). `detail` fetch failures
/// or non-zero envelopes degrade to an unknown state, never to "no plan".
#[derive(Debug, Clone)]
struct MimoPlanState {
    label: Option<String>,
    resets_at: Option<DateTime<Utc>>,
    known_no_plan: bool,
    active: bool,
    expired: bool,
}

const MIMO_NO_PLAN_CODES: [&str; 5] = [
    "default",
    "none",
    "no_plan",
    "not_subscribed",
    "unsubscribed",
];
const MIMO_ACTIVE_STATUSES: [&str; 2] = ["active", "subscribed"];
const MIMO_EXPIRED_STATUSES: [&str; 2] = ["expired", "ended"];

fn normalize_plan_value(value: &str) -> String {
    value.trim().to_lowercase().replace([' ', '-'], "_")
}

fn classify_plan(detail: Option<TokenPlanDetailData>, now: DateTime<Utc>) -> MimoPlanState {
    let Some(data) = detail else {
        return MimoPlanState {
            label: None,
            resets_at: None,
            known_no_plan: false,
            active: false,
            expired: false,
        };
    };

    let label = data
        .plan_code
        .clone()
        .map(|plan| plan.trim().to_string())
        .filter(|plan| !plan.is_empty());
    let normalized_label = label.as_deref().map(normalize_plan_value);
    let raw_status = [data.plan_status, data.subscription_status, data.state]
        .into_iter()
        .flatten()
        .map(|status| normalize_plan_value(&status))
        .find(|status| !status.is_empty());
    let resets_at = data.current_period_end.as_deref().and_then(parse_mimo_date);
    let has_future_period = resets_at.is_some_and(|end| end > now);
    let has_expired_period = resets_at.is_some_and(|end| end <= now);

    let is_known_no_plan = normalized_label
        .as_deref()
        .is_some_and(|value| MIMO_NO_PLAN_CODES.contains(&value))
        || raw_status
            .as_deref()
            .is_some_and(|value| MIMO_NO_PLAN_CODES.contains(&value));
    let has_real_plan_identity = normalized_label
        .as_deref()
        .is_some_and(|value| !value.is_empty() && !MIMO_NO_PLAN_CODES.contains(&value));
    let explicit_active = raw_status
        .as_deref()
        .is_some_and(|value| MIMO_ACTIVE_STATUSES.contains(&value))
        || data.active == Some(true)
        || data.is_active == Some(true);
    let explicit_expired = raw_status
        .as_deref()
        .is_some_and(|value| MIMO_EXPIRED_STATUSES.contains(&value))
        || data.expired;
    let expired =
        !is_known_no_plan && (explicit_expired || (has_real_plan_identity && has_expired_period));
    let has_explicit_status =
        raw_status.is_some() || data.active.is_some() || data.is_active.is_some();
    let active = !is_known_no_plan
        && !expired
        && (explicit_active
            || (!has_explicit_status && has_real_plan_identity && has_future_period));

    MimoPlanState {
        label: if is_known_no_plan { None } else { label },
        resets_at,
        known_no_plan: is_known_no_plan,
        active,
        expired,
    }
}

/// token-monitor parity: only the `month_total_token` item is the plan-wide
/// quota; other items are per-model sub-quotas. When the API ships items but
/// no total row, usage is unknown — never substitute the first item. When no
/// items exist at all, some shapes carry the totals on the month object.
fn select_plan_usage(month: TokenPlanMonthUsage) -> Option<TokenPlanUsageItem> {
    if !month.items.is_empty() {
        return month.items.into_iter().find(|item| {
            item.name
                .as_deref()
                .map(|name| name.trim().eq_ignore_ascii_case("month_total_token"))
                .unwrap_or(false)
        });
    }
    let (Some(used), Some(limit)) = (month.used, month.limit) else {
        return None;
    };
    let percent = month
        .percent
        .filter(|value| value.is_finite())
        .unwrap_or_else(|| {
            if limit > 0 {
                (used as f64 / limit as f64 * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            }
        });
    Some(TokenPlanUsageItem {
        name: None,
        used,
        limit,
        percent,
    })
}

fn snapshot_from_plan(
    detail: Option<TokenPlanDetailResponse>,
    usage: Option<TokenPlanUsageResponse>,
    balance: Option<String>,
) -> UsageSnapshot {
    let detail_data =
        detail.and_then(|response| (response.code == 0).then_some(response.data).flatten());
    let plan = classify_plan(detail_data, Utc::now());
    let usage_item = usage
        .and_then(|response| (response.code == 0).then_some(response.data).flatten())
        .and_then(|data| data.month_usage)
        .and_then(select_plan_usage);

    let usable_usage = usage_item.filter(|item| item.limit > 0);
    let has_expired_plan = plan.expired && (plan.label.is_some() || usable_usage.is_some());

    let primary = if let Some(item) = usable_usage.filter(|_| !plan.expired && !plan.known_no_plan)
    {
        RateWindow::with_details(
            item.percent,
            RateWindow::monthly_window_minutes(plan.resets_at),
            plan.resets_at,
            Some(format!("{}/{} tokens", item.used, item.limit)),
        )
    } else if has_expired_plan {
        let description = match plan.resets_at {
            Some(end) => format!("MiMo Token Plan expired ({})", end.format("%Y-%m-%d")),
            None => "MiMo Token Plan expired".to_string(),
        };
        RateWindow::informational(description)
    } else if plan.known_no_plan {
        RateWindow::informational(MIMO_NO_PLAN_MARKER)
    } else {
        RateWindow::informational("No MiMo token usage reported")
    };

    let mut snapshot = UsageSnapshot::new(primary);
    if let Some(description) = balance {
        snapshot.secondary = Some(RateWindow::with_details(0.0, None, None, Some(description)));
    }
    match plan.label {
        Some(plan_name) => {
            let label = plan
                .expired
                .then(|| format!("{plan_name} (expired)"))
                .unwrap_or(plan_name);
            snapshot = snapshot.with_login_method(label);
        }
        None => {
            snapshot = snapshot.with_login_method("MiMo Token Plan");
        }
    }
    snapshot
}

fn balance_description_from_response(response: BalanceResponse) -> Option<String> {
    let data = (response.code == 0).then_some(response.data).flatten()?;
    let balance = data.balance.as_f64()?;
    let cash_balance = data
        .cash_balance
        .as_ref()
        .and_then(NumericValue::as_f64)
        .map(|value| value.to_string());
    let gift_balance = data
        .gift_balance
        .as_ref()
        .and_then(NumericValue::as_f64)
        .map(|value| value.to_string());

    Some(balance_description(
        balance,
        &data.currency,
        cash_balance.as_deref(),
        gift_balance.as_deref(),
    ))
}

fn parse_mimo_date(value: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok()?;
    // The console serves Beijing-wall-clock timestamps; the request pins the
    // account timezone with `x-timeZone: UTC+08:00`, so parse against that
    // offset instead of silently treating the value as UTC.
    let offset = FixedOffset::east_opt(MIMO_REQUEST_TIMEZONE_OFFSET_HOURS * 3600)?;
    naive
        .and_local_timezone(offset)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

fn parse_decimal(value: Option<&str>) -> Option<f64> {
    value?.trim().parse().ok()
}

pub(crate) fn balance_description(
    balance: f64,
    currency: &str,
    cash_balance: Option<&str>,
    gift_balance: Option<&str>,
) -> String {
    let currency = currency.trim();
    let total = format!("{balance:.2} {currency} balance");
    let Some(cash) = parse_decimal(cash_balance) else {
        return total;
    };
    let Some(gift) = parse_decimal(gift_balance) else {
        return total;
    };
    format!("{total} (Paid: {cash:.2} {currency} / Granted: {gift:.2} {currency})")
}

impl Default for MiMoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for MiMoProvider {
    fn id(&self) -> ProviderId {
        ProviderId::MiMo
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        self.fetch_usage_with_browser_getter(ctx, || {
            crate::providers::browser_cookie_header(&["platform.xiaomimimo.com"])
        })
        .await
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Web]
    }

    fn supports_web(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn detail_response(json: &str) -> Option<TokenPlanDetailResponse> {
        Some(serde_json::from_str(json).expect("detail payload"))
    }

    fn usage_response(json: &str) -> Option<TokenPlanUsageResponse> {
        Some(serde_json::from_str(json).expect("usage payload"))
    }

    fn balance_response(json: &str) -> Option<String> {
        let response: BalanceResponse = serde_json::from_str(json).expect("balance payload");
        balance_description_from_response(response)
    }

    #[test]
    fn mimo_cookie_requires_service_token_and_user_id() {
        assert!(normalize_cookie_header("api-platform_serviceToken=abc; userId=42").is_some());
        assert!(normalize_cookie_header("api-platform_serviceToken=abc").is_none());
    }

    // Pasted request headers from the console copy flow carry a "Cookie:"
    // prefix; it must not break the required-pair check.
    #[test]
    fn mimo_cookie_tolerates_request_header_prefix() {
        assert!(
            normalize_cookie_header("Cookie: api-platform_serviceToken=abc; userId=42").is_some()
        );
        assert!(
            normalize_cookie_header(
                "cookie: api-platform_serviceToken=abc; userId=42; api-platform_ph=x"
            )
            .is_some()
        );
        assert!(normalize_cookie_header("Cookie:   ").is_none());
    }

    #[test]
    fn mimo_balance_description_includes_paid_and_granted_components() {
        assert_eq!(
            balance_description(12.5, "CNY", Some("8.25"), Some("4.25")),
            "12.50 CNY balance (Paid: 8.25 CNY / Granted: 4.25 CNY)"
        );
        assert_eq!(
            balance_description(12.5, "CNY", Some("8.25"), None),
            "12.50 CNY balance"
        );
    }

    #[test]
    fn mimo_console_balance_is_attached_as_the_secondary_balance_window() {
        let snapshot = snapshot_from_plan(
            None,
            None,
            balance_response(
                r#"{
                    "code": 0,
                    "data": {
                        "balance": 34.46,
                        "currency": "CNY",
                        "cashBalance": "34.46",
                        "giftBalance": 0
                    }
                }"#,
            ),
        );

        assert_eq!(
            snapshot
                .secondary
                .and_then(|window| window.reset_description),
            Some("34.46 CNY balance (Paid: 34.46 CNY / Granted: 0.00 CNY)".into())
        );
        // Unknown plan state must stay informational, not a fake 0% quota.
        assert!(snapshot.primary.is_informational);
    }

    // Only the `month_total_token` row is the plan quota; per-model items
    // (first item here) must never substitute for it.
    #[test]
    fn usage_selects_the_month_total_token_item() {
        let snapshot = snapshot_from_plan(
            detail_response(
                r#"{"code": 0, "data": {"planCode": "mimo_pro", "currentPeriodEnd": "2026-10-14 23:59:59"}}"#,
            ),
            usage_response(
                r#"{"code": 0, "data": {"monthUsage": {"items": [
                    {"name": "vision_tokens", "used": 10, "limit": 100, "percent": 10.0},
                    {"name": "month_total_token", "used": 300, "limit": 1000, "percent": 30.0}
                ]}}}"#,
            ),
            None,
        );

        assert!(!snapshot.primary.is_informational);
        assert!((snapshot.primary.used_percent - 30.0).abs() < 0.001);
        assert_eq!(
            snapshot.primary.reset_description.as_deref(),
            Some("300/1000 tokens")
        );
        assert_eq!(snapshot.login_method.as_deref(), Some("mimo_pro"));
    }

    // Items without a total row mean unknown usage; taking items[0] would
    // render a per-model sub-quota as the whole plan.
    #[test]
    fn usage_without_total_item_is_unknown_not_first_item() {
        let snapshot = snapshot_from_plan(
            detail_response(
                r#"{"code": 0, "data": {"planCode": "mimo_pro", "currentPeriodEnd": "2026-10-14 23:59:59"}}"#,
            ),
            usage_response(
                r#"{"code": 0, "data": {"monthUsage": {"items": [
                    {"name": "vision_tokens", "used": 10, "limit": 100, "percent": 10.0}
                ]}}}"#,
            ),
            None,
        );

        assert!(snapshot.primary.is_informational);
        assert!(!snapshot.any_exhausted());
    }

    #[test]
    fn usage_falls_back_to_month_level_totals_without_items() {
        let snapshot = snapshot_from_plan(
            None,
            usage_response(
                r#"{"code": 0, "data": {"monthUsage": {"used": 250, "limit": 1000, "percent": 25.0}}}"#,
            ),
            None,
        );

        assert!(!snapshot.primary.is_informational);
        assert!((snapshot.primary.used_percent - 25.0).abs() < 0.001);
    }

    // An active plan renders a real quota window with a calendar-month
    // length and a +08:00-parsed reset instant.
    #[test]
    fn active_plan_uses_calendar_month_window_and_utc8_reset() {
        let snapshot = snapshot_from_plan(
            detail_response(
                r#"{"code": 0, "data": {"planCode": "mimo_pro", "currentPeriodEnd": "2026-10-14 23:59:59"}}"#,
            ),
            usage_response(
                r#"{"code": 0, "data": {"monthUsage": {"items": [
                    {"name": "month_total_token", "used": 300, "limit": 1000, "percent": 30.0}
                ]}}}"#,
            ),
            None,
        );

        assert!(!snapshot.primary.is_informational);
        // 2026-10-14 23:59:59 +08:00 == 2026-10-14 15:59:59 UTC.
        assert_eq!(
            snapshot.primary.resets_at,
            Utc.with_ymd_and_hms(2026, 10, 14, 15, 59, 59).single()
        );
        // Sep 14 → Oct 14 (UTC) spans exactly 30 calendar days; the window
        // derives from the reset instant, not a fixed 30-day bucket.
        assert_eq!(snapshot.primary.window_minutes, Some(30 * 24 * 60));
    }

    #[test]
    fn no_plan_marker_stays_informational_and_byte_identical() {
        let snapshot = snapshot_from_plan(
            detail_response(r#"{"code": 0, "data": {"planCode": "no_plan"}}"#),
            None,
            balance_response(r#"{"code": 0, "data": {"balance": 5, "currency": "CNY"}}"#),
        );

        assert!(snapshot.primary.is_informational);
        assert_eq!(
            snapshot.primary.reset_description.as_deref(),
            Some("No active MiMo Token Plan")
        );
        assert!(snapshot.secondary.is_some());
        assert!(!snapshot.any_exhausted());
    }

    #[test]
    fn expired_flag_renders_informational_expiry_with_plan_label() {
        let snapshot = snapshot_from_plan(
            detail_response(
                r#"{"code": 0, "data": {"planCode": "mimo_pro", "expired": true, "currentPeriodEnd": "2026-09-01 00:00:00"}}"#,
            ),
            usage_response(
                r#"{"code": 0, "data": {"monthUsage": {"items": [
                    {"name": "month_total_token", "used": 300, "limit": 1000, "percent": 30.0}
                ]}}}"#,
            ),
            None,
        );

        assert!(snapshot.primary.is_informational);
        assert!(
            snapshot
                .primary
                .reset_description
                .as_deref()
                .is_some_and(|text| text.starts_with("MiMo Token Plan expired"))
        );
        assert_eq!(snapshot.login_method.as_deref(), Some("mimo_pro (expired)"));
    }

    #[test]
    fn past_period_end_marks_a_real_plan_expired() {
        let snapshot = snapshot_from_plan(
            detail_response(
                r#"{"code": 0, "data": {"planCode": "mimo_pro", "currentPeriodEnd": "2020-01-01 00:00:00"}}"#,
            ),
            None,
            None,
        );

        assert!(snapshot.primary.is_informational);
        assert!(
            snapshot
                .primary
                .reset_description
                .as_deref()
                .is_some_and(|text| text.starts_with("MiMo Token Plan expired"))
        );
    }

    #[test]
    fn explicit_active_status_renders_usage_without_future_period() {
        let snapshot = snapshot_from_plan(
            detail_response(
                r#"{"code": 0, "data": {"planCode": "mimo_pro", "planStatus": "active"}}"#,
            ),
            usage_response(
                r#"{"code": 0, "data": {"monthUsage": {"items": [
                    {"name": "month_total_token", "used": 100, "limit": 1000, "percent": 10.0}
                ]}}}"#,
            ),
            None,
        );

        assert!(!snapshot.primary.is_informational);
        assert!((snapshot.primary.used_percent - 10.0).abs() < 0.001);
    }

    async fn setup_success_mocks(
        server: &mut mockito::ServerGuard,
        cookie: &str,
        plan_code: &str,
    ) -> Vec<mockito::Mock> {
        let m1 = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", cookie)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(format!(
                r#"{{"code":0,"data":{{"planCode":"{plan_code}","planStatus":"active"}}}}"#
            ))
            .create_async()
            .await;
        let m2 = server
            .mock("GET", "/tokenPlan/usage")
            .match_header("cookie", cookie)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{"code":0,"data":{"monthUsage":{"items":[{"name":"month_total_token","used":100,"limit":1000,"percent":10.0}]}}}"#,
            )
            .create_async()
            .await;
        let m3 = server
            .mock("GET", "/balance")
            .match_header("cookie", cookie)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":0,"data":{"balance":50.0,"currency":"CNY"}}"#)
            .create_async()
            .await;
        vec![m1, m2, m3]
    }

    #[tokio::test]
    async fn browser_auth_rejection_falls_back_to_saved_cookie() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let browser_cookie = "api-platform_serviceToken=browser_tok; userId=browser_user";
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";

        let _m_auth = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", browser_cookie)
            .with_status(401)
            .create_async()
            .await;
        let _mocks = setup_success_mocks(&mut server, saved_cookie, "saved_plan").await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };

        let result = provider
            .fetch_usage_with_browser_getter(&ctx, || Ok(browser_cookie.to_string()))
            .await
            .expect("fallback to saved cookie should succeed when browser cookie gets 401");

        assert_eq!(result.usage.login_method.as_deref(), Some("saved_plan"));
        _m_auth.assert_async().await;
    }

    #[tokio::test]
    async fn browser_server_error_does_not_silently_switch_to_saved_cookie() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let browser_cookie = "api-platform_serviceToken=browser_tok; userId=browser_user";
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";

        let browser_detail = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", browser_cookie)
            .with_status(500)
            .create_async()
            .await;
        let browser_usage = server
            .mock("GET", "/tokenPlan/usage")
            .match_header("cookie", browser_cookie)
            .with_status(500)
            .create_async()
            .await;
        let browser_balance = server
            .mock("GET", "/balance")
            .match_header("cookie", browser_cookie)
            .with_status(500)
            .create_async()
            .await;
        let saved_detail = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", saved_cookie)
            .expect(0)
            .create_async()
            .await;
        let saved_usage = server
            .mock("GET", "/tokenPlan/usage")
            .match_header("cookie", saved_cookie)
            .expect(0)
            .create_async()
            .await;
        let saved_balance = server
            .mock("GET", "/balance")
            .match_header("cookie", saved_cookie)
            .expect(0)
            .create_async()
            .await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };
        let result = provider
            .fetch_usage_with_browser_getter(&ctx, || Ok(browser_cookie.to_string()))
            .await
            .expect("non-auth endpoint failures must not switch to a different saved account");

        assert!(result.usage.primary.is_informational);
        browser_detail.assert_async().await;
        browser_usage.assert_async().await;
        browser_balance.assert_async().await;
        saved_detail.assert_async().await;
        saved_usage.assert_async().await;
        saved_balance.assert_async().await;
    }

    #[tokio::test]
    async fn absent_browser_session_falls_back_to_saved_cookie() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";
        let _mocks = setup_success_mocks(&mut server, saved_cookie, "saved_plan").await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };

        let result = provider
            .fetch_usage_with_browser_getter(&ctx, || Err(ProviderError::NoCookies))
            .await
            .expect("absent browser cookie should fall back to saved cookie");

        assert_eq!(result.usage.login_method.as_deref(), Some("saved_plan"));
    }

    #[tokio::test]
    async fn unusable_browser_session_fails_validation_and_falls_back_to_saved_cookie() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";
        let _mocks = setup_success_mocks(&mut server, saved_cookie, "saved_plan").await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };

        let result = provider
            .fetch_usage_with_browser_getter(&ctx, || Ok("some_unusable_token=123".to_string()))
            .await
            .expect(
                "unusable browser cookie without required keys should fall back to saved cookie",
            );

        assert_eq!(result.usage.login_method.as_deref(), Some("saved_plan"));
    }

    #[tokio::test]
    async fn successful_browser_result_takes_precedence_over_saved_cookie() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let browser_cookie = "api-platform_serviceToken=browser_tok; userId=browser_user";
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";

        let _b_mocks = setup_success_mocks(&mut server, browser_cookie, "browser_plan").await;
        let s_mock = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", saved_cookie)
            .expect(0)
            .create_async()
            .await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };

        let result = provider
            .fetch_usage_with_browser_getter(&ctx, || Ok(browser_cookie.to_string()))
            .await
            .expect("browser fetch should succeed");

        assert_eq!(result.usage.login_method.as_deref(), Some("browser_plan"));
        s_mock.assert_async().await;
    }

    #[tokio::test]
    async fn manual_mode_uses_saved_cookie_and_never_probes_browser() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";
        let _mocks = setup_success_mocks(&mut server, saved_cookie, "saved_plan").await;

        let ctx = FetchContext {
            manual_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: false,
            ..FetchContext::default()
        };

        let result = provider
            .fetch_usage_with_browser_getter(&ctx, || {
                panic!("browser getter must never be probed in manual mode")
            })
            .await
            .expect("manual cookie fetch should succeed");

        assert_eq!(result.usage.login_method.as_deref(), Some("saved_plan"));
    }

    #[tokio::test]
    async fn manual_mode_without_saved_cookie_returns_nocookies_without_probing_browser() {
        let provider = MiMoProvider::new();
        let ctx = FetchContext {
            manual_cookie_header: None,
            fallback_cookie_header: None,
            allow_browser_cookie: false,
            ..FetchContext::default()
        };

        let err = provider
            .fetch_usage_with_browser_getter(&ctx, || {
                panic!("browser getter must never be probed when allow_browser_cookie is false")
            })
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::NoCookies));
    }

    #[tokio::test]
    async fn both_browser_and_saved_failing_auth_returns_auth_required() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let browser_cookie = "api-platform_serviceToken=browser_tok; userId=browser_user";
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";

        let _m1 = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", browser_cookie)
            .with_status(401)
            .create_async()
            .await;
        let _m2 = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", saved_cookie)
            .with_status(401)
            .create_async()
            .await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };

        let err = provider
            .fetch_usage_with_browser_getter(&ctx, || Ok(browser_cookie.to_string()))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::AuthRequired));
    }

    // A locked browser database (Chrome active profile) plus a stale saved
    // cookie must surface the lock as the actionable cause — a bare
    // "authentication required" sends the user to re-login instead of
    // closing the browser or re-importing.
    #[tokio::test]
    async fn locked_browser_with_stale_saved_cookie_surfaces_lock_reason() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";

        let _m1 = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", saved_cookie)
            .with_status(401)
            .create_async()
            .await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };

        let err = provider
            .fetch_usage_with_browser_getter(&ctx, || {
                Err(ProviderError::Other(
                    "the running browser has the cookie database locked; \
                     close the browser or import a fresh cookie, then refresh."
                        .to_string(),
                ))
            })
            .await
            .unwrap_err();

        match err {
            ProviderError::AuthUnavailable(message) => {
                assert!(message.contains("locked"), "message: {message}");
                assert!(message.contains("saved MiMo cookie"), "message: {message}");
            }
            other => panic!("expected AuthUnavailable, got {other}"),
        }
    }

    // No browser session for the domain at all (plain extraction miss) keeps
    // the plain auth category: the remedy is authentication, not the lock.
    #[tokio::test]
    async fn missing_browser_session_with_stale_saved_cookie_stays_auth_required() {
        let mut server = mockito::Server::new_async().await;
        let provider = MiMoProvider::new().with_api_base(server.url());
        let saved_cookie = "api-platform_serviceToken=saved_tok; userId=saved_user";

        let _m1 = server
            .mock("GET", "/tokenPlan/detail")
            .match_header("cookie", saved_cookie)
            .with_status(401)
            .create_async()
            .await;

        let ctx = FetchContext {
            fallback_cookie_header: Some(saved_cookie.to_string()),
            allow_browser_cookie: true,
            ..FetchContext::default()
        };

        let err = provider
            .fetch_usage_with_browser_getter(&ctx, || Err(ProviderError::NoCookies))
            .await
            .unwrap_err();

        assert!(matches!(err, ProviderError::AuthRequired));
    }
}
