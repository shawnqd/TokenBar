use async_trait::async_trait;
use reqwest::{Client, Url};
use serde_json::Value;

use crate::core::{
    CostSnapshot, FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId,
    ProviderMetadata, RateWindow, SourceMode, UsageSnapshot,
};

const CREDENTIAL_TARGET: &str = "codexbar-devin";
// Web-session tokens (auth1_) are rejected on the API host but work on the
// web host, and vice versa for service keys, so every candidate is tried.
const BASE_URLS: [&str; 2] = ["https://api.devin.ai", "https://app.devin.ai/api"];
const MISSING_ORGANIZATION_DETAIL: &str = "No organizations found for auth1 user";
const MISSING_ORGANIZATION_MESSAGE: &str = "Devin organization context is missing. Set the organization in provider extras or DEVIN_ORG, then refresh.";

pub struct DevinProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl DevinProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::Devin,
                display_name: "Devin",
                session_label: "Daily",
                weekly_label: "Weekly",
                supports_opus: false,
                supports_credits: true,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://app.devin.ai/settings/billing"),
                status_page_url: None,
            },
            client: crate::core::credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }
}

impl Default for DevinProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for DevinProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Devin
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        match ctx.source_mode {
            SourceMode::Auto | SourceMode::OAuth => {
                let token = crate::providers::resolve_api_key(
                    ctx.api_key.as_deref(),
                    CREDENTIAL_TARGET,
                    &["DEVIN_BEARER_TOKEN", "DEVIN_API_KEY"],
                )?;
                let env_org = std::env::var("DEVIN_ORG").ok();
                let org = ctx
                    .workspace_id
                    .as_deref()
                    .or(env_org.as_deref())
                    .ok_or_else(|| {
                        ProviderError::NotInstalled(
                            "Devin organization not found. Set it in provider extras or DEVIN_ORG."
                                .into(),
                        )
                    })?;
                let org = normalized_org(org);
                fetch_quota(&self.client, &token, &org, devin_urls(&org)?).await
            }
            SourceMode::Web | SourceMode::Cli => {
                Err(ProviderError::UnsupportedSource(ctx.source_mode))
            }
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::OAuth]
    }
}

async fn fetch_quota(
    client: &Client,
    token: &str,
    org: &str,
    urls: Vec<Url>,
) -> Result<ProviderFetchResult, ProviderError> {
    let mut last_non_auth_error: Option<ProviderError> = None;
    let mut auth_failures = 0usize;
    for url in urls {
        let response = match client
            .get(url)
            .bearer_auth(token)
            // Auth1 sessions resolve their organization context from this
            // header; without it the gateway answers 401 "No organizations
            // found for auth1 user" even for a valid session token.
            .header("x-cog-org-id", org)
            .header("Accept", "application/json")
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                last_non_auth_error = Some(ProviderError::Network(error));
                continue;
            }
        };
        let status = response.status();
        let body = match response.bytes().await {
            Ok(body) => body,
            Err(error) => {
                last_non_auth_error = Some(ProviderError::Network(error));
                continue;
            }
        };
        if status.is_success() {
            let value: Value = match serde_json::from_slice(&body) {
                Ok(value) => value,
                Err(error) => {
                    last_non_auth_error = Some(ProviderError::Parse(format!(
                        "Failed to parse Devin quota: {error}"
                    )));
                    continue;
                }
            };
            return fetch_result_from_quota(&value, org);
        }
        if let Some(error) = auth_response_error(status, &body) {
            if matches!(error, ProviderError::AuthRequired) {
                auth_failures += 1;
            } else {
                last_non_auth_error = Some(error);
            }
        } else {
            last_non_auth_error = Some(ProviderError::Other(format!(
                "Devin quota returned status {}",
                status
            )));
        }
    }
    // Every candidate rejecting auth means the credential itself is stale;
    // a mixed result (one host 401, the other failing differently) reports
    // the more specific non-auth failure instead of masking it.
    match last_non_auth_error {
        Some(error) => Err(error),
        None => Err(ProviderError::AuthRequired),
    }
}

fn devin_urls(org: &str) -> Result<Vec<Url>, ProviderError> {
    BASE_URLS
        .iter()
        .map(|base| Url::parse(&format!("{base}/{org}/billing/quota/usage")))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ProviderError::Other(format!("Invalid Devin quota URL: {e}")))
}

fn auth_response_error(status: reqwest::StatusCode, body: &[u8]) -> Option<ProviderError> {
    if status != reqwest::StatusCode::UNAUTHORIZED && status != reqwest::StatusCode::FORBIDDEN {
        return None;
    }

    let is_missing_organization = serde_json::from_slice::<Value>(body)
        .ok()
        .is_some_and(|value| {
            value.get("detail").and_then(Value::as_str) == Some(MISSING_ORGANIZATION_DETAIL)
        });
    if is_missing_organization {
        Some(ProviderError::Other(
            MISSING_ORGANIZATION_MESSAGE.to_string(),
        ))
    } else {
        Some(ProviderError::AuthRequired)
    }
}

fn normalized_org(raw: &str) -> String {
    // Both hosts serve the quota at /{org}/billing/quota/usage with the bare
    // organization id (org_...); a prefixed path 404s server-side.
    let trimmed = raw.trim().trim_matches('/');
    trimmed
        .strip_prefix("organizations/")
        .or_else(|| trimmed.strip_prefix("org/"))
        .unwrap_or(trimmed)
        .to_string()
}

fn snapshot_from_quota(value: &Value, org: &str) -> Result<UsageSnapshot, ProviderError> {
    let daily = percent(value, &["daily_percentage", "dailyPercentage"])
        .or_else(|| percent(value, &["used_percent", "usedPercent"]))
        .ok_or_else(|| {
            ProviderError::Parse("Devin quota response missing daily usage".to_string())
        })?;
    let mut snapshot =
        UsageSnapshot::new(RateWindow::new(daily)).with_organization(org.to_string());
    if let Some(weekly) = percent(value, &["weekly_percentage", "weeklyPercentage"]) {
        snapshot = snapshot.with_secondary(RateWindow::new(weekly));
    }
    Ok(snapshot)
}

fn fetch_result_from_quota(value: &Value, org: &str) -> Result<ProviderFetchResult, ProviderError> {
    let mut result = ProviderFetchResult::new(snapshot_from_quota(value, org)?, "api");
    if let Some(balance) = extra_usage_balance(value) {
        result = result.with_cost(CostSnapshot::new(balance, "USD", "Extra usage balance"));
    }
    Ok(result)
}

fn percent(value: &Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(v) = value.get(*key).and_then(Value::as_f64) {
            return Some(v.clamp(0.0, 100.0));
        }
    }
    // Remaining-form fields invert into used percent (token-monitor parity).
    if let Some(remaining) = ["remaining_percentage", "remainingPercentage"]
        .iter()
        .find_map(|k| value.get(*k).and_then(Value::as_f64))
        .filter(|value| value.is_finite())
    {
        return Some((100.0 - remaining).clamp(0.0, 100.0));
    }
    let used = ["used", "usage", "used_count", "usedCount", "consumed"]
        .iter()
        .find_map(|k| value.get(*k).and_then(Value::as_f64));
    let limit = ["limit", "quota", "total", "max", "available"]
        .iter()
        .find_map(|k| value.get(*k).and_then(Value::as_f64));
    match (used, limit) {
        (Some(used), Some(limit)) if limit > 0.0 => Some((used / limit * 100.0).clamp(0.0, 100.0)),
        _ => None,
    }
}

fn extra_usage_balance(value: &Value) -> Option<f64> {
    let dollars = [
        "overage_balance",
        "overageBalance",
        "extra_usage_balance",
        "extraUsageBalance",
    ]
    .iter()
    .find_map(|key| value.get(*key).and_then(Value::as_f64))
    .filter(|value| value.is_finite() && *value >= 0.0);
    dollars.or_else(|| {
        ["overage_balance_cents", "overageBalanceCents"]
            .iter()
            .find_map(|key| value.get(*key).and_then(Value::as_f64))
            .filter(|value| value.is_finite() && *value >= 0.0)
            .map(|cents| cents / 100.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn org_normalizes_to_bare_id_for_host_paths() {
        assert_eq!(normalized_org("org_abc123"), "org_abc123");
        assert_eq!(normalized_org("org/org_abc123"), "org_abc123");
        assert_eq!(normalized_org("organizations/org_abc123/"), "org_abc123");
    }

    // Percent values are already 0–100 scaled; sub-integer values (e.g. 0.1%
    // or 0.25%) must not be amplified. Divergence from upstream (which scales
    // <1.0 ×100) is a documented fork decision pending live-account evidence.
    #[test]
    fn parses_fractional_sub_integer_percent() {
        let snapshot =
            snapshot_from_quota(&serde_json::json!({"daily_percentage":0.25}), "org_abc").unwrap();
        assert_eq!(snapshot.primary.used_percent, 0.25);
    }

    #[test]
    fn parses_exact_one_as_one_percent() {
        let snapshot =
            snapshot_from_quota(&serde_json::json!({"daily_percentage":1.0}), "org_abc").unwrap();
        assert_eq!(snapshot.primary.used_percent, 1.0);
    }

    #[test]
    fn quota_urls_cover_both_hosts() {
        let urls = devin_urls("org_abc").unwrap();
        assert_eq!(urls.len(), 2);
        assert_eq!(
            urls[0].as_str(),
            "https://api.devin.ai/org_abc/billing/quota/usage"
        );
        assert_eq!(
            urls[1].as_str(),
            "https://app.devin.ai/api/org_abc/billing/quota/usage"
        );
    }

    #[test]
    fn missing_daily_usage_is_a_parse_error_not_zero_percent() {
        let err =
            snapshot_from_quota(&serde_json::json!({}), "org_abc").expect_err("no daily usage");
        assert!(matches!(err, ProviderError::Parse(_)));
    }

    #[test]
    fn parses_remaining_percentage_as_used() {
        let snapshot = snapshot_from_quota(
            &serde_json::json!({"remaining_percentage": 75.0}),
            "org_abc",
        )
        .unwrap();
        assert!((snapshot.primary.used_percent - 25.0).abs() < 0.001);
    }

    #[test]
    fn parses_extra_usage_balance() {
        let result = fetch_result_from_quota(
            &serde_json::json!({"daily_percentage": 0.2, "overage_balance": 12.34}),
            "org_abc",
        )
        .unwrap();

        let cost = result.cost.unwrap();
        assert_eq!(cost.used, 12.34);
        assert_eq!(cost.period, "Extra usage balance");
    }

    #[test]
    fn parses_extra_usage_balance_cents() {
        let result = fetch_result_from_quota(
            &serde_json::json!({"daily_percentage": 0.2, "overage_balance_cents": 7087}),
            "org_abc",
        )
        .unwrap();

        assert_eq!(result.cost.unwrap().used, 70.87);
    }

    #[test]
    fn missing_organization_401_maps_to_actionable_message() {
        let body = serde_json::to_vec(&serde_json::json!({
            "detail": "No organizations found for auth1 user"
        }))
        .unwrap();
        let error = auth_response_error(reqwest::StatusCode::UNAUTHORIZED, &body).unwrap();
        assert!(
            matches!(error, ProviderError::Other(ref message) if message.contains("organization context is missing"))
        );
    }

    #[test]
    fn plain_401_maps_to_auth_required() {
        let body = serde_json::to_vec(&serde_json::json!({"detail": "invalid token"})).unwrap();
        let error = auth_response_error(reqwest::StatusCode::UNAUTHORIZED, &body).unwrap();
        assert!(matches!(error, ProviderError::AuthRequired));
    }
}
