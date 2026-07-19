//! Volcengine Ark subscription-plan providers.
//!
//! Ark exposes Coding Plan and Agent Plan as separate quota surfaces.  Keep
//! them separate from the existing Doubao probe so the UI can show the three
//! account types independently.

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot, credentialed_http_client_builder,
};

use super::doubao::{
    DOUBAO_CODING_PLAN_URL, DoubaoCodingPlanCredentials, coding_plan_snapshot,
    decode_coding_plan_usage, sign_volcengine_request_for_url,
};

const AGENT_PLAN_URL: &str =
    "https://ark.cn-beijing.volces.com/?Action=GetAFPUsage&Version=2024-01-01";

const DASHBOARD_URL: &str = "https://console.volcengine.com/ark/region:ark+cn-beijing/usage";

/// Dedicated Coding Plan provider.  The existing `Doubao` provider remains
/// the one-token Ark API probe; this provider reads the subscription quota
/// endpoint and therefore has its own card and credentials.
pub struct ArkCodingPlanProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl ArkCodingPlanProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::ArkCodingPlan,
                display_name: "Volcengine Ark Coding Plan",
                session_label: "5-hour Requests",
                weekly_label: "Weekly Requests",
                supports_opus: false,
                supports_credits: false,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some(DASHBOARD_URL),
                status_page_url: None,
            },
            client: credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    async fn fetch_usage_for_credentials(
        &self,
        credentials: &DoubaoCodingPlanCredentials,
    ) -> Result<UsageSnapshot, ProviderError> {
        let body = Vec::new();
        let signed = sign_volcengine_request_for_url(
            DOUBAO_CODING_PLAN_URL,
            credentials,
            &body,
            Utc::now(),
            "application/x-www-form-urlencoded; charset=utf-8",
        )?;
        let response = self
            .client
            .post(DOUBAO_CODING_PLAN_URL)
            .header("Accept", "application/json")
            .header("Content-Type", signed.content_type)
            .header("Host", signed.host)
            .header("X-Date", signed.timestamp)
            .header("X-Content-Sha256", signed.payload_hash)
            .header("Authorization", signed.authorization)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::AuthRequired);
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "Ark Coding Plan API returned {status}: {}",
                sanitized_body(&String::from_utf8_lossy(&bytes))
            )));
        }

        Ok(coding_plan_snapshot(decode_coding_plan_usage(&bytes)?))
    }
}

impl Default for ArkCodingPlanProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for ArkCodingPlanProvider {
    fn id(&self) -> ProviderId {
        ProviderId::ArkCodingPlan
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        match ctx.source_mode {
            SourceMode::Auto | SourceMode::OAuth => {
                let credentials = ctx
                    .api_key
                    .as_deref()
                    .and_then(DoubaoCodingPlanCredentials::parse)
                    .or_else(DoubaoCodingPlanCredentials::from_env)
                    .ok_or_else(|| {
                        ProviderError::NotInstalled(
                            "Access Key credentials not found. Paste access_key|secret_key|region in Preferences or set VOLCENGINE_ACCESS_KEY_ID and VOLCENGINE_SECRET_ACCESS_KEY.".into(),
                        )
                    })?;
                Ok(ProviderFetchResult::new(
                    self.fetch_usage_for_credentials(&credentials).await?,
                    "coding-plan",
                ))
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

/// Agent Plan provider backed by the official AFP quota endpoint.
pub struct ArkAgentPlanProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl ArkAgentPlanProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::ArkAgentPlan,
                display_name: "Volcengine Ark Agent Plan",
                session_label: "5-hour AFP",
                weekly_label: "Daily AFP",
                supports_opus: false,
                supports_credits: false,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some(DASHBOARD_URL),
                status_page_url: None,
            },
            client: credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    async fn fetch_usage_for_credentials(
        &self,
        credentials: &DoubaoCodingPlanCredentials,
    ) -> Result<UsageSnapshot, ProviderError> {
        let body = b"{}".to_vec();
        let signed = sign_volcengine_request_for_url(
            AGENT_PLAN_URL,
            credentials,
            &body,
            Utc::now(),
            "application/json; charset=UTF-8",
        )?;
        let response = self
            .client
            .post(AGENT_PLAN_URL)
            .header("Accept", "application/json")
            .header("Content-Type", signed.content_type)
            .header("Host", signed.host)
            .header("X-Date", signed.timestamp)
            .header("X-Content-Sha256", signed.payload_hash)
            .header("Authorization", signed.authorization)
            .body(body)
            .send()
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::AuthRequired);
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "Ark Agent Plan API returned {status}: {}",
                sanitized_body(&String::from_utf8_lossy(&bytes))
            )));
        }

        let response: AgentPlanUsageResponse = serde_json::from_slice(&bytes).map_err(|error| {
            ProviderError::Parse(format!("Failed to parse Ark Agent Plan usage: {error}"))
        })?;
        Ok(agent_plan_snapshot(response.result))
    }
}

impl Default for ArkAgentPlanProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for ArkAgentPlanProvider {
    fn id(&self) -> ProviderId {
        ProviderId::ArkAgentPlan
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        match ctx.source_mode {
            SourceMode::Auto | SourceMode::OAuth => {
                let credentials = ctx
                    .api_key
                    .as_deref()
                    .and_then(DoubaoCodingPlanCredentials::parse)
                    .or_else(DoubaoCodingPlanCredentials::from_env)
                    .ok_or_else(|| {
                        ProviderError::NotInstalled(
                            "Access Key credentials not found. Paste access_key|secret_key|region in Preferences or set VOLCENGINE_ACCESS_KEY_ID and VOLCENGINE_SECRET_ACCESS_KEY.".into(),
                        )
                    })?;
                Ok(ProviderFetchResult::new(
                    self.fetch_usage_for_credentials(&credentials).await?,
                    "agent-plan",
                ))
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

#[derive(Debug, Deserialize)]
struct AgentPlanUsageResponse {
    #[serde(rename = "Result")]
    result: AgentPlanResult,
}

#[derive(Debug, Deserialize)]
struct AgentPlanResult {
    #[serde(rename = "PlanType")]
    plan_type: Option<String>,
    #[serde(rename = "AFPFiveHour")]
    five_hour: Option<AgentPlanWindow>,
    #[serde(rename = "AFPDaily")]
    daily: Option<AgentPlanWindow>,
    #[serde(rename = "AFPWeekly")]
    weekly: Option<AgentPlanWindow>,
    #[serde(rename = "AFPMonthly")]
    monthly: Option<AgentPlanWindow>,
}

#[derive(Debug, Deserialize)]
struct AgentPlanWindow {
    #[serde(rename = "Quota")]
    quota: f64,
    #[serde(rename = "Used")]
    used: f64,
    #[serde(rename = "ResetTime")]
    reset_time: Option<f64>,
}

fn agent_plan_snapshot(usage: AgentPlanResult) -> UsageSnapshot {
    let primary = agent_plan_window(usage.five_hour, 5 * 60).unwrap_or_default();
    let mut snapshot = UsageSnapshot::new(primary);
    if let Some(daily) = agent_plan_window(usage.daily, 24 * 60) {
        snapshot = snapshot.with_secondary(daily);
    }
    if let Some(weekly) = agent_plan_window(usage.weekly, 7 * 24 * 60) {
        snapshot = snapshot.with_tertiary(weekly);
    }
    if let Some(monthly) = agent_plan_window(usage.monthly, 30 * 24 * 60) {
        snapshot = snapshot.with_extra_rate_window("monthly", "Monthly AFP", monthly);
    }
    if let Some(plan_type) = usage.plan_type.filter(|value| !value.trim().is_empty()) {
        snapshot = snapshot.with_login_method(format!("Agent Plan {plan_type}"));
    }
    snapshot
}

fn agent_plan_window(window: Option<AgentPlanWindow>, minutes: u32) -> Option<RateWindow> {
    let window = window?;
    let used = if window.used.is_finite() {
        window.used.max(0.0)
    } else {
        0.0
    };
    let quota = if window.quota.is_finite() {
        window.quota.max(0.0)
    } else {
        0.0
    };
    let used_percent = if quota > 0.0 {
        used / quota * 100.0
    } else {
        0.0
    };
    let reset = window.reset_time.and_then(|timestamp| {
        if timestamp.is_finite() && timestamp > 0.0 {
            Utc.timestamp_millis_opt(timestamp as i64).single()
        } else {
            None
        }
    });
    Some(RateWindow::with_details(
        used_percent,
        Some(minutes),
        reset,
        Some(format!("{used:.2}/{quota:.2} AFP")),
    ))
}

fn sanitized_body(body: &str) -> String {
    let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > 200 {
        let preview = collapsed.chars().take(200).collect::<String>();
        format!("{preview}... [truncated]")
    } else if collapsed.is_empty() {
        "empty body".to_string()
    } else {
        collapsed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_agent_plan_windows_and_plan_type() {
        let body = br#"{
          "Result": {
            "PlanType": "Medium",
            "AFPFiveHour": {"Quota": 50.0, "Used": 12.5, "ResetTime": 1783040400000},
            "AFPDaily": {"Quota": 100.0, "Used": 22.5, "ResetTime": 1783040400000},
            "AFPWeekly": {"Quota": 500.0, "Used": 150.0, "ResetTime": 1783641600000},
            "AFPMonthly": {"Quota": 2000.0, "Used": 850.5, "ResetTime": 1785628800000}
          }
        }"#;
        let response: AgentPlanUsageResponse = serde_json::from_slice(body).expect("usage");
        let snapshot = agent_plan_snapshot(response.result);
        assert!((snapshot.primary.used_percent - 25.0).abs() < f64::EPSILON);
        assert!((snapshot.secondary.unwrap().used_percent - 22.5).abs() < f64::EPSILON);
        assert!((snapshot.tertiary.unwrap().used_percent - 30.0).abs() < f64::EPSILON);
        assert_eq!(snapshot.extra_rate_windows.len(), 1);
        assert_eq!(snapshot.login_method.as_deref(), Some("Agent Plan Medium"));
    }

    #[test]
    fn agent_plan_signer_uses_ark_host_and_json_content_type() {
        let credentials = DoubaoCodingPlanCredentials {
            access_key_id: "ak-test".to_string(),
            secret_access_key: "sk-test".to_string(),
            region: "cn-beijing".to_string(),
        };
        let signed = sign_volcengine_request_for_url(
            AGENT_PLAN_URL,
            &credentials,
            b"{}",
            Utc.timestamp_opt(1_783_036_800, 0).single().expect("time"),
            "application/json; charset=UTF-8",
        )
        .expect("signed request");
        assert_eq!(signed.host, "ark.cn-beijing.volces.com");
        assert_eq!(signed.content_type, "application/json; charset=UTF-8");
        assert!(signed.authorization.contains("Credential=ak-test/"));
    }
}
