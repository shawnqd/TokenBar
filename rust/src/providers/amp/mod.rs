//! Amp provider implementation
//!
//! Amp is Sourcegraph's AI coding assistant
//! Fetches usage data from Amp's local config or API

use async_trait::async_trait;
use std::path::PathBuf;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

mod cli;
mod subscription;
use subscription::usage_snapshot_from_amp_display_text;

/// Amp provider (Sourcegraph)
pub struct AmpProvider {
    metadata: ProviderMetadata,
}

impl AmpProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::Amp,
                display_name: "Amp",
                session_label: "Usage",
                weekly_label: "Monthly",
                supports_opus: false,
                supports_credits: true,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://ampcode.com/settings/usage"),
                status_page_url: Some("https://sourcegraphstatus.com"),
            },
        }
    }

    /// Get Amp config directory
    fn get_amp_config_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            dirs::config_dir().map(|p| p.join("amp"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            dirs::home_dir().map(|p| p.join(".amp"))
        }
    }

    /// Get Sourcegraph/Cody config directory (Amp might use this)
    fn get_cody_config_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            dirs::config_dir().map(|p| p.join("sourcegraph-cody"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            dirs::home_dir().map(|p| p.join(".sourcegraph"))
        }
    }

    /// Read Amp/Sourcegraph access token
    async fn read_access_token(&self, ctx: &FetchContext) -> Result<String, ProviderError> {
        if let Some(token) = access_token_from_context(ctx) {
            return Ok(token);
        }

        if let Some(token) = access_token_from_environment() {
            return Ok(token);
        }

        if let Some(token) = Self::read_local_config_token().await {
            return Ok(token);
        }

        Err(ProviderError::AuthRequired)
    }

    async fn read_local_config_token() -> Option<String> {
        let amp_token = read_access_token_config(Self::get_amp_config_path()).await;
        if amp_token.is_some() {
            return amp_token;
        }

        read_access_token_config(Self::get_cody_config_path()).await
    }

    /// Fetch usage via Sourcegraph API
    async fn fetch_via_web(&self, ctx: &FetchContext) -> Result<UsageSnapshot, ProviderError> {
        let token = self.read_access_token(ctx).await?;

        let client = crate::core::credentialed_http_client_builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        // Sourcegraph Cody usage API
        let resp = client
            .get("https://sourcegraph.com/.api/cody/current-user/usage")
            .header("Authorization", format!("token {}", token))
            .send()
            .await?;

        if !resp.status().is_success() {
            return Err(ProviderError::AuthRequired);
        }

        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        self.parse_usage_response(&json)
    }

    fn parse_usage_response(
        &self,
        json: &serde_json::Value,
    ) -> Result<UsageSnapshot, ProviderError> {
        // Prefer the provider's own display text (Amp Free / subscription
        // windows) over the numeric completions fields.
        if let Some(display_text) = json
            .get("displayText")
            .or_else(|| json.get("display_text"))
            .or_else(|| {
                json.get("result")
                    .and_then(|result| result.get("displayText"))
            })
            .and_then(serde_json::Value::as_str)
            && let Some(usage) =
                usage_snapshot_from_amp_display_text(display_text, chrono::Utc::now())
        {
            return Ok(usage);
        }

        // Parse Sourcegraph/Amp usage response
        let used = json
            .get("completionsUsed")
            .or_else(|| json.get("used"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        let limit = json
            .get("completionsLimit")
            .or_else(|| json.get("limit"))
            .and_then(|v| v.as_f64())
            .unwrap_or(500.0);

        let used_percent = if limit > 0.0 {
            (used / limit) * 100.0
        } else {
            0.0
        };

        let plan = json
            .get("plan")
            .or_else(|| json.get("tier"))
            .and_then(|v| v.as_str())
            .unwrap_or("Pro");

        let reset_time = json
            .get("resetAt")
            .or_else(|| json.get("periodEnd"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let primary_window = RateWindow::with_details(used_percent, None, None, reset_time);
        let usage = UsageSnapshot::new(primary_window).with_login_method(plan);

        Ok(usage)
    }

    /// Probe for an Amp installation. There is no fake-success path: a
    /// configured-but-unreadable Amp yields an error, never a 0% quota.
    async fn probe_cli(&self) -> Result<UsageSnapshot, ProviderError> {
        let configured = Self::get_amp_config_path()
            .map(|p| p.join("config.json").exists())
            .unwrap_or(false)
            || Self::get_cody_config_path()
                .map(|p| p.join("config.json").exists())
                .unwrap_or(false)
            || access_token_from_environment().is_some();

        if configured {
            cli::fetch_usage().await
        } else {
            Err(ProviderError::NotInstalled(
                "Amp not configured. Set SRC_ACCESS_TOKEN, run `amp` to sign in, or configure Amp."
                    .to_string(),
            ))
        }
    }
}

fn access_token_from_context(ctx: &FetchContext) -> Option<String> {
    ctx.api_key
        .as_deref()
        .filter(|api_key| !api_key.is_empty())
        .map(str::to_string)
}

fn access_token_from_environment() -> Option<String> {
    std::env::var("SRC_ACCESS_TOKEN")
        .ok()
        .or_else(|| std::env::var("AMP_ACCESS_TOKEN").ok())
}

async fn read_access_token_config(config_dir: Option<PathBuf>) -> Option<String> {
    let config_file = config_dir?.join("config.json");
    if !config_file.exists() {
        return None;
    }

    let content = tokio::fs::read_to_string(config_file).await.ok()?;
    let json = serde_json::from_str::<serde_json::Value>(&content).ok()?;
    json.get("accessToken")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

impl Default for AmpProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for AmpProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Amp
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        tracing::debug!("Fetching Amp usage");

        match ctx.source_mode {
            SourceMode::Auto => {
                // CLI-first like upstream: `amp usage` is authoritative for
                // signed-in users; the Cody web endpoint is the fallback.
                if let Ok(usage) = cli::fetch_usage().await {
                    return Ok(ProviderFetchResult::new(usage, "cli"));
                }
                let usage = self.fetch_via_web(ctx).await?;
                Ok(ProviderFetchResult::new(usage, "web"))
            }
            SourceMode::Web => {
                let usage = self.fetch_via_web(ctx).await?;
                Ok(ProviderFetchResult::new(usage, "web"))
            }
            SourceMode::Cli => {
                let usage = self.probe_cli().await?;
                Ok(ProviderFetchResult::new(usage, "cli"))
            }
            SourceMode::OAuth => Err(ProviderError::UnsupportedSource(SourceMode::OAuth)),
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Web, SourceMode::Cli]
    }

    fn supports_web(&self) -> bool {
        true
    }

    fn supports_cli(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::subscription::{
        parse_amp_free_percent_remaining, parse_amp_subscription_usage,
        usage_snapshot_from_amp_display_text,
    };
    use super::*;
    use chrono::{TimeZone, Utc};

    #[test]
    fn dashboard_points_to_current_usage_page() {
        assert_eq!(
            AmpProvider::new().metadata().dashboard_url,
            Some("https://ampcode.com/settings/usage")
        );
    }

    #[test]
    fn parses_amp_free_percent_remaining_today() {
        let text = "Signed in as user@example.com\nAmp Free: 72% remaining today\n";
        assert_eq!(parse_amp_free_percent_remaining(text), Some(28.0));
    }

    #[test]
    fn parses_amp_free_percent_resets_daily() {
        let text = "Amp Free: 100% remaining (resets daily)";
        assert_eq!(parse_amp_free_percent_remaining(text), Some(0.0));
    }

    #[test]
    fn parses_bold_amp_free_and_current_subscription_labels() {
        let now = Utc.with_ymd_and_hms(2026, 8, 24, 12, 0, 0).unwrap();
        assert_eq!(
            parse_amp_free_percent_remaining("**Amp Free:** 0% remaining today (resets daily)"),
            Some(100.0)
        );

        let sub = parse_amp_subscription_usage(
            "**Amp Megawatt Subscription:** 68% other usage and 97% orb usage remaining - resets upon renewal in 5 days",
            now,
        )
        .expect("bold subscription");
        assert_eq!(sub.plan, "Megawatt");
        assert!((sub.other_used_percent() - 32.0).abs() < f64::EPSILON);
        assert!((sub.orb_used_percent().unwrap() - 3.0).abs() < f64::EPSILON);
        assert_eq!(sub.resets_at(), Some(now + chrono::Duration::days(5)));
    }

    #[test]
    fn ignores_dollar_remaining_form() {
        let text = "Amp Free: $4.20 / $10 remaining (replenishes +$1 / hour)";
        assert_eq!(parse_amp_free_percent_remaining(text), None);
    }

    #[test]
    fn parses_megawatt_subscription_dual_windows() {
        let now = Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap();
        let text = "Signed in as user@example.com (Acme)\n\
Subscription Megawatt: 42% other usage and 88% orb usage remaining - resets upon renewal in 12 days\n";
        let snapshot = usage_snapshot_from_amp_display_text(text, now).expect("snapshot");
        assert!((snapshot.primary.used_percent - 58.0).abs() < f64::EPSILON);
        assert_eq!(
            snapshot.primary.reset_description.as_deref(),
            Some("renews in 12 days")
        );
        let secondary = snapshot.secondary.expect("orb secondary");
        assert!((secondary.used_percent - 12.0).abs() < f64::EPSILON);
        assert_eq!(snapshot.login_method.as_deref(), Some("Megawatt"));
    }

    #[test]
    fn gigawatt_monthly_renewal_advances_calendar_months() {
        let now = Utc.with_ymd_and_hms(2026, 8, 17, 12, 0, 0).unwrap();
        let text = "Subscription Gigawatt: 10% other usage and 95% orb usage remaining - resets upon renewal in 2 months";
        let sub = parse_amp_subscription_usage(text, now).expect("subscription");
        assert_eq!(sub.plan, "Gigawatt");
        assert_eq!(sub.reset_description, "renews in 2 months");
        assert_eq!(
            sub.resets_at(),
            Some(Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap())
        );
    }

    #[test]
    fn free_tier_resets_at_8pm_new_york() {
        // 2026-08-17 18:00 UTC = 14:00 EDT → same-day 20:00 EDT = 00:00 UTC Aug 18.
        let now = Utc.with_ymd_and_hms(2026, 8, 17, 18, 0, 0).unwrap();
        let snapshot =
            usage_snapshot_from_amp_display_text("Amp Free: 72% remaining (resets daily)", now)
                .expect("snapshot");
        assert_eq!(
            snapshot.primary.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 18, 0, 0, 0).unwrap())
        );

        // 2026-08-18 00:30 UTC = 20:30 EDT Aug 17 (after the boundary) → the
        // next reset is Aug 18 20:00 EDT = Aug 19 00:00 UTC.
        let later = Utc.with_ymd_and_hms(2026, 8, 18, 0, 30, 0).unwrap();
        let snapshot =
            usage_snapshot_from_amp_display_text("Amp Free: 72% remaining (resets daily)", later)
                .expect("snapshot");
        assert_eq!(
            snapshot.primary.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 8, 19, 0, 0, 0).unwrap())
        );
    }

    // Unrecognized CLI/web output must error, never render a fabricated 0%.
    #[test]
    fn unrecognized_usage_output_is_a_parse_error() {
        let err = cli::usage_from_amp_cli_output("hello world", chrono::Utc::now())
            .expect_err("unrecognized output");
        assert!(matches!(err, ProviderError::Parse(_)));
    }
}
