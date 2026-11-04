//! OpenCode provider implementation
//!
//! Fetches usage data from OpenCode (opencode.ai)
//! Uses browser cookies for authentication

mod console_api;
pub mod scraper;

// Re-exports for advanced scraping
#[allow(unused_imports)]
pub use scraper::{OpenCodeError, OpenCodeUsageFetcher, OpenCodeUsageSnapshot};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;
use uuid::Uuid;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

const BASE_URL: &str = "https://opencode.ai";
const SERVER_URL: &str = "https://opencode.ai/_server";
/// Console calls (`org list`, `go/status`) get their own budget so the legacy
/// `_server` fallback still has time left in the same refresh. Matches the Go
/// console chain's 10s console timeout.
const CONSOLE_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const MAX_RESET_SECONDS: i64 = 366 * 24 * 60 * 60;
const WORKSPACES_SERVER_ID: &str =
    "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
const SUBSCRIPTION_SERVER_ID: &str =
    "7abeebee372f304e050aaaf92be863f4a86490e382f8c79db68fd94040d691b4";

/// OpenCode provider
pub struct OpenCodeProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::OpenCode,
                display_name: "OpenCode",
                session_label: "5-hour",
                weekly_label: "Weekly",
                supports_opus: false,
                supports_credits: false,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://opencode.ai"),
                status_page_url: None,
            },
            client: crate::core::credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    /// Fetch usage with cookie header.
    ///
    /// `workspace_id` is the workspace pinned in settings. When it is usable the
    /// legacy chain scopes to it directly; only an absent or blank pin falls back
    /// to discovering the cookie's first workspace, which may not be the
    /// workspace the console chain just reported. A non-empty pin that cannot be
    /// normalized is an error rather than a silent discovery fallback.
    async fn fetch_with_cookies(
        &self,
        cookie_header: &str,
        workspace_id: Option<&str>,
    ) -> Result<UsageSnapshot, ProviderError> {
        self.fetch_with_cookies_at(SERVER_URL, cookie_header, workspace_id)
            .await
    }

    /// `fetch_with_cookies` against an explicit `_server` base, so tests can
    /// point the legacy chain at a local mock without reaching opencode.ai.
    async fn fetch_with_cookies_at(
        &self,
        server_url: &str,
        cookie_header: &str,
        workspace_id: Option<&str>,
    ) -> Result<UsageSnapshot, ProviderError> {
        let workspace_id = match Self::pinned_workspace(workspace_id)? {
            Some(workspace_id) => workspace_id,
            None => {
                self.fetch_workspace_id_at(server_url, cookie_header)
                    .await?
            }
        };

        // Then fetch subscription info
        let subscription = self
            .fetch_subscription_at(server_url, &workspace_id, cookie_header)
            .await?;

        // Parse the response
        self.parse_subscription(&subscription)
    }

    /// Normalized workspace pinned in settings.
    ///
    /// `None` means "absent or blank": both chains then discover the cookie's
    /// workspace. A non-empty pin that cannot be normalized is an error — the
    /// user asked for a specific workspace, so neither the console nor the legacy
    /// chain may quietly report a different one found by discovery.
    fn pinned_workspace(workspace_id: Option<&str>) -> Result<Option<String>, ProviderError> {
        let raw = workspace_id.map(str::trim).filter(|raw| !raw.is_empty());
        match raw {
            None => Ok(None),
            Some(raw) => console_api::normalize_workspace_id(raw)
                .map(Some)
                .ok_or_else(|| ProviderError::Parse("Invalid OpenCode workspace ID".to_string())),
        }
    }

    /// Fetch Zen Go usage from the console JSON API (first-party endpoints).
    ///
    /// The workspace comes from settings when the user pinned one, otherwise
    /// from the cookie's org list — the console scopes `go/status` by
    /// `x-org-id`, so the id must be resolved first.
    async fn fetch_console(
        &self,
        cookie_header: &str,
        pinned: Option<&str>,
    ) -> Result<UsageSnapshot, ProviderError> {
        let workspace_id = match pinned {
            Some(workspace_id) => workspace_id.to_string(),
            None => {
                console_api::fetch_workspace_id(
                    &self.client,
                    console_api::DEFAULT_ORIGIN,
                    cookie_header,
                    CONSOLE_REQUEST_TIMEOUT,
                )
                .await?
            }
        };

        console_api::fetch_usage(
            &self.client,
            console_api::DEFAULT_ORIGIN,
            &workspace_id,
            cookie_header,
            CONSOLE_REQUEST_TIMEOUT,
        )
        .await
        .map(|usage| usage.into_snapshot())
    }

    /// Fetch workspace ID from server
    async fn fetch_workspace_id_at(
        &self,
        server_url: &str,
        cookie_header: &str,
    ) -> Result<String, ProviderError> {
        let url = format!("{}?id={}", server_url, WORKSPACES_SERVER_ID);

        let response = self
            .client
            .get(&url)
            .header("Cookie", cookie_header)
            .header("X-Server-Id", WORKSPACES_SERVER_ID)
            .header("X-Server-Instance", format!("server-fn:{}", Uuid::new_v4()))
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .header("Origin", BASE_URL)
            .header("Referer", BASE_URL)
            .header(
                "Accept",
                "text/javascript, application/json;q=0.9, */*;q=0.8",
            )
            .send()
            .await?;

        // Only 401 means "this cookie is not signed in". A 403 from the legacy
        // endpoint is usually a WAF/proxy/workspace-permission refusal, so it
        // must not be reported as an expired login (the console chain classifies
        // 403 the same way).
        let status = response.status();
        if status.as_u16() == 401 {
            return Err(ProviderError::AuthRequired);
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "OpenCode API returned {status}"
            )));
        }

        let text = response.text().await?;

        // Check for sign-out indicators
        if self.looks_signed_out(&text) {
            return Err(ProviderError::AuthRequired);
        }

        // Parse workspace IDs
        let ids = self.parse_workspace_ids(&text);
        if ids.is_empty() {
            return Err(ProviderError::Parse("No workspace ID found".to_string()));
        }

        Ok(ids[0].clone())
    }

    /// Fetch subscription info for a workspace
    async fn fetch_subscription_at(
        &self,
        server_url: &str,
        workspace_id: &str,
        cookie_header: &str,
    ) -> Result<String, ProviderError> {
        let referer = format!("https://opencode.ai/workspace/{}/billing", workspace_id);
        let args = serde_json::json!([workspace_id]);
        let encoded_args = Self::url_encode(&args.to_string());
        let url = format!(
            "{}?id={}&args={}",
            server_url, SUBSCRIPTION_SERVER_ID, encoded_args
        );

        let response = self
            .client
            .get(&url)
            .header("Cookie", cookie_header)
            .header("X-Server-Id", SUBSCRIPTION_SERVER_ID)
            .header("X-Server-Instance", format!("server-fn:{}", Uuid::new_v4()))
            .header(
                "User-Agent",
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
            )
            .header("Origin", BASE_URL)
            .header("Referer", referer)
            .header(
                "Accept",
                "text/javascript, application/json;q=0.9, */*;q=0.8",
            )
            .send()
            .await?;

        let status = response.status();
        if status.as_u16() == 401 {
            return Err(ProviderError::AuthRequired);
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "OpenCode subscription API returned {status}"
            )));
        }

        let text = response.text().await?;

        if self.looks_signed_out(&text) {
            return Err(ProviderError::AuthRequired);
        }

        Ok(text)
    }

    /// Parse subscription response into UsageSnapshot
    fn parse_subscription(&self, text: &str) -> Result<UsageSnapshot, ProviderError> {
        let now = Utc::now();

        // Try to parse as JSON
        if let Ok(json) = serde_json::from_str::<Value>(text)
            && let Some(snapshot) = self.parse_usage_json(&json, now)
        {
            return Ok(snapshot);
        }

        // Fall back to regex-based parsing. Require at least one window —
        // plans may omit rolling or weekly without being unparseable.
        let rolling = self.extract_usage_regex(text, "rollingUsage").ok();
        let weekly = self.extract_usage_regex(text, "weeklyUsage").ok();
        self.snapshot_from_windows(rolling, weekly, now, self.extract_renewal_regex(text))
            .ok_or_else(|| ProviderError::Parse("Missing usage percent (rolling or weekly)".into()))
    }

    /// Build a snapshot from optional rolling/weekly windows.
    /// Rolling is preferred as primary; weekly-only becomes primary.
    fn snapshot_from_windows(
        &self,
        rolling: Option<(f64, i64)>,
        weekly: Option<(f64, i64)>,
        now: DateTime<Utc>,
        renews_at: Option<DateTime<Utc>>,
    ) -> Option<UsageSnapshot> {
        let primary = match (rolling, weekly) {
            (Some(r), _) => RateWindow::with_details(
                r.0,
                Some(300), // 5 hours rolling
                Some(now + chrono::Duration::seconds(r.1)),
                None,
            ),
            (None, Some(w)) => RateWindow::with_details(
                w.0,
                Some(10080), // weekly used as sole window
                Some(now + chrono::Duration::seconds(w.1)),
                None,
            ),
            (None, None) => return None,
        };

        let mut usage = UsageSnapshot::new(primary).with_login_method("OpenCode");
        if rolling.is_some()
            && let Some(w) = weekly
        {
            usage = usage.with_secondary(RateWindow::with_details(
                w.0,
                Some(10080),
                Some(now + chrono::Duration::seconds(w.1)),
                None,
            ));
        }
        if let Some(renews_at) = renews_at {
            usage = usage.with_extra_rate_window(
                "renewal",
                "Renews",
                RateWindow::with_details(0.0, None, Some(renews_at), None),
            );
        }
        Some(usage)
    }

    /// Parse usage from JSON response
    fn parse_usage_json(&self, json: &Value, now: DateTime<Utc>) -> Option<UsageSnapshot> {
        let renews_at = self.find_datetime(json, &["renewAt", "renew_at"]);

        let rolling = self.find_usage_window(json, &["rollingUsage", "rolling", "rolling_usage"]);
        let weekly = self.find_usage_window(json, &["weeklyUsage", "weekly", "weekly_usage"]);

        self.snapshot_from_windows(rolling, weekly, now, renews_at)
    }

    /// Find usage window in JSON by keys
    fn find_usage_window(&self, json: &Value, keys: &[&str]) -> Option<(f64, i64)> {
        for key in keys {
            if let Some(obj) = json.get(key)
                && let Some(window) = self.parse_window(obj)
            {
                return Some(window);
            }
        }

        // Try nested search
        if let Some(obj) = json.as_object() {
            for (_, value) in obj {
                if let Some(window) = self.find_usage_window(value, keys) {
                    return Some(window);
                }
            }
        }

        None
    }

    /// Parse a usage window object
    fn parse_window(&self, obj: &Value) -> Option<(f64, i64)> {
        let percent = Self::window_percent(obj)?;
        let reset_sec = Self::window_reset_seconds(obj).unwrap_or(0);
        Some((percent.clamp(0.0, 100.0), reset_sec.max(0)))
    }

    fn window_percent(obj: &Value) -> Option<f64> {
        let percent_keys = [
            "usagePercent",
            "usedPercent",
            "percentUsed",
            "percent",
            "usage_percent",
            "used_percent",
            "utilization",
            "utilizationPercent",
            "utilization_percent",
            "usage",
        ];

        Self::first_f64(obj, &percent_keys)
            // usagePercent is already scaled 0–100; sub-integer values like 0.1
            // are real sub-1% readings (e.g. Go plan), not 0–1 fractions.
            .or_else(|| Self::percent_from_used_limit(obj))
    }

    fn percent_from_used_limit(obj: &Value) -> Option<f64> {
        let used = obj
            .get("used")
            .or(obj.get("usage"))
            .and_then(|v| v.as_f64());
        let limit = obj
            .get("limit")
            .or(obj.get("total"))
            .and_then(|v| v.as_f64());
        match (used, limit) {
            (Some(used), Some(limit)) if limit > 0.0 => Some((used / limit) * 100.0),
            _ => None,
        }
    }

    fn window_reset_seconds(obj: &Value) -> Option<i64> {
        let reset_in_keys = [
            "resetInSec",
            "resetInSeconds",
            "resetSeconds",
            "reset_sec",
            "reset_in_sec",
            "resetsInSec",
            "resetsInSeconds",
            "resetIn",
            "resetSec",
        ];
        let reset_at_keys = [
            "resetAt",
            "resetsAt",
            "reset_at",
            "resets_at",
            "nextReset",
            "next_reset",
            "renewAt",
            "renew_at",
        ];

        Self::first_i64(obj, &reset_in_keys)
            .or_else(|| Self::reset_at_to_seconds(obj, &reset_at_keys))
    }

    fn reset_at_to_seconds(obj: &Value, keys: &[&str]) -> Option<i64> {
        let reset_at = Self::first_i64(obj, keys)?;
        let now = chrono::Utc::now().timestamp();
        reset_at
            .checked_sub(now)
            .filter(|seconds| (0..=MAX_RESET_SECONDS).contains(seconds))
    }

    fn find_datetime(&self, json: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
        for key in keys {
            if let Some(value) = json.get(key)
                && let Some(parsed) = Self::date_from_value(value)
            {
                return Some(parsed);
            }
        }

        if let Some(obj) = json.as_object() {
            for value in obj.values() {
                if let Some(parsed) = self.find_datetime(value, keys) {
                    return Some(parsed);
                }
            }
        }
        None
    }

    fn first_f64(obj: &Value, keys: &[&str]) -> Option<f64> {
        keys.iter().find_map(|key| obj.get(*key)?.as_f64())
    }

    fn first_i64(obj: &Value, keys: &[&str]) -> Option<i64> {
        keys.iter().find_map(|key| obj.get(*key)?.as_i64())
    }

    fn date_from_value(value: &Value) -> Option<DateTime<Utc>> {
        if let Some(number) = value.as_i64() {
            return Self::date_from_timestamp(number as f64);
        }
        if let Some(number) = value.as_f64() {
            return Self::date_from_timestamp(number);
        }
        let text = value.as_str()?.trim();
        if text.is_empty() {
            return None;
        }
        if let Ok(number) = text.parse::<f64>() {
            return Self::date_from_timestamp(number);
        }
        DateTime::parse_from_rfc3339(text)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    fn date_from_timestamp(number: f64) -> Option<DateTime<Utc>> {
        if !number.is_finite() || number <= 0.0 {
            return None;
        }
        let seconds = if number > 10_000_000_000.0 {
            number / 1000.0
        } else {
            number
        };
        DateTime::<Utc>::from_timestamp(seconds as i64, 0)
    }

    fn extract_renewal_regex(&self, text: &str) -> Option<DateTime<Utc>> {
        let re = regex_lite::Regex::new(
            r#"(?:"renewAt"|"renew_at"|renewAt|renew_at)\s*[:=]\s*"?([^",}\s]+)"?"#,
        )
        .ok()?;
        let raw = re.captures(text)?.get(1)?.as_str();
        Self::date_from_value(&Value::String(raw.to_string()))
    }

    /// Extract usage via regex patterns (JS-object and JSON-ish payloads).
    fn extract_usage_regex(&self, text: &str, prefix: &str) -> Result<(f64, i64), ProviderError> {
        // Allow optional quotes around the window key and percent field names,
        // and accept the same percent aliases used by `window_percent`.
        let percent_pattern = format!(
            r#"{prefix}[^}}]{{0,500}}?(?:"usagePercent"|usagePercent|"usedPercent"|usedPercent|"percent"|percent)\s*[:=]\s*"?([0-9]+(?:\.[0-9]+)?)"?"#
        );
        let reset_pattern = format!(
            r#"{prefix}[^}}]{{0,500}}?(?:"resetInSec"|resetInSec|"resetInSeconds"|resetInSeconds)\s*[:=]\s*"?([0-9]+)"?"#
        );

        let percent = self
            .extract_number(&percent_pattern, text)
            .ok_or_else(|| ProviderError::Parse(format!("Missing {} percent", prefix)))?;

        let reset = self
            .extract_number(&reset_pattern, text)
            .map(|n| n as i64)
            .unwrap_or(0);

        Ok((percent, reset))
    }

    /// Extract a number using regex
    fn extract_number(&self, pattern: &str, text: &str) -> Option<f64> {
        let re = regex_lite::Regex::new(pattern).ok()?;
        let caps = re.captures(text)?;
        caps.get(1)?.as_str().parse().ok()
    }

    /// Parse workspace IDs from response
    fn parse_workspace_ids(&self, text: &str) -> Vec<String> {
        let pattern = r#"id\s*:\s*"(wrk_[^"]+)""#;
        let re = match regex_lite::Regex::new(pattern) {
            Ok(r) => r,
            Err(_) => return vec![],
        };

        re.captures_iter(text)
            .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Check if response indicates user is signed out
    fn looks_signed_out(&self, text: &str) -> bool {
        let lower = text.to_lowercase();
        lower.contains("login") || lower.contains("sign in") || lower.contains("auth/authorize")
    }

    /// URL encode a string for query parameters
    fn url_encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                    result.push(c);
                }
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}

impl Default for OpenCodeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OpenCodeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenCode
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        tracing::debug!("Fetching OpenCode usage");

        match ctx.source_mode {
            SourceMode::Auto | SourceMode::Web => {
                let cookie_header = match ctx.manual_cookie_header.as_deref() {
                    Some(cookie_header) => cookie_header.to_string(),
                    None => {
                        // An explicit manual cookie source with nothing saved,
                        // or a disabled cookie source, must not silently borrow
                        // the signed-in browser identity.
                        if !ctx.allow_browser_cookie {
                            return Err(ProviderError::NoCookies);
                        }
                        match crate::providers::browser_cookie_header(&["opencode.ai"]) {
                            Ok(cookie_header) => cookie_header,
                            Err(ProviderError::NoCookies) => {
                                return Err(ProviderError::AuthRequired);
                            }
                            Err(e) => return Err(e),
                        }
                    }
                };

                // Newer deployments serve the console JSON API; older ones only
                // expose the `_server` SSR endpoints. The legacy chain stays as
                // the fallback so both deployment shapes keep working, and both
                // chains share one pinned workspace so they cannot disagree. An
                // invalid non-empty pin fails here, before either chain can fall
                // back to discovery.
                let pinned = Self::pinned_workspace(ctx.workspace_id.as_deref())?;
                let console_error =
                    match self.fetch_console(&cookie_header, pinned.as_deref()).await {
                        Ok(usage) => return Ok(ProviderFetchResult::new(usage, "console")),
                        Err(error) => error,
                    };

                match self
                    .fetch_with_cookies(&cookie_header, pinned.as_deref())
                    .await
                {
                    Ok(usage) => Ok(ProviderFetchResult::new(usage, "web")),
                    // The console error explains why the new chain was
                    // unavailable (expired session, no workspace, no Zen
                    // subscription), so it is the more actionable report.
                    Err(legacy_error) => Err(merge_console_error(console_error, legacy_error)),
                }
            }
            SourceMode::Cli => Err(ProviderError::UnsupportedSource(SourceMode::Cli)),
            SourceMode::OAuth => Err(ProviderError::UnsupportedSource(SourceMode::OAuth)),
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Web]
    }

    fn supports_web(&self) -> bool {
        true
    }

    fn supports_cli(&self) -> bool {
        false
    }
}

fn merge_console_error(console: ProviderError, legacy: ProviderError) -> ProviderError {
    match (console, legacy) {
        (console @ ProviderError::AuthRequired, _) => console,
        (_, legacy @ ProviderError::AuthRequired) => legacy,
        (console, _) => console,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_renewal_window() {
        let provider = OpenCodeProvider::new();
        let now = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let payload = serde_json::json!({
            "rollingUsage": { "usagePercent": 10, "resetInSec": 600 },
            "weeklyUsage": { "usagePercent": 50, "resetInSec": 3600 },
            "renewAt": "2026-06-01T12:00:00Z"
        });

        let snap = provider.parse_usage_json(&payload, now).expect("snapshot");
        let renewal = snap
            .extra_rate_windows
            .iter()
            .find(|window| window.id == "renewal")
            .expect("renewal window");
        assert_eq!(renewal.title, "Renews");
        assert_eq!(
            renewal.window.resets_at.unwrap().to_rfc3339(),
            "2026-06-01T12:00:00+00:00"
        );
    }

    #[test]
    fn ignores_out_of_range_reset_timestamps() {
        let payload = serde_json::json!({ "resetAt": i64::MAX });

        assert_eq!(OpenCodeProvider::window_reset_seconds(&payload), None);
    }

    #[test]
    fn parses_weekly_only_json_without_rolling() {
        let provider = OpenCodeProvider::new();
        let now = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let payload = serde_json::json!({
            "weeklyUsage": { "usagePercent": 76.0, "resetInSec": 86400 }
        });

        let snap = provider.parse_usage_json(&payload, now).expect("snapshot");
        assert!((snap.primary.used_percent - 76.0).abs() < f64::EPSILON);
        assert!(snap.secondary.is_none());
    }

    #[test]
    fn parses_rolling_only_json_without_weekly() {
        let provider = OpenCodeProvider::new();
        let now = DateTime::<Utc>::from_timestamp(1_700_000_000, 0).unwrap();
        let payload = serde_json::json!({
            "rollingUsage": { "usagePercent": 12.5, "resetInSec": 600 }
        });

        let snap = provider.parse_usage_json(&payload, now).expect("snapshot");
        assert!((snap.primary.used_percent - 12.5).abs() < f64::EPSILON);
        assert!(snap.secondary.is_none());
    }

    #[test]
    fn regex_accepts_json_quoted_usage_percent() {
        let provider = OpenCodeProvider::new();
        let text = r#"{"rollingUsage":{"usagePercent":33.0,"resetInSec":120},"weeklyUsage":{"usagePercent":80}}"#;
        let snap = provider.parse_subscription(text).expect("snapshot");
        assert!((snap.primary.used_percent - 33.0).abs() < f64::EPSILON);
        assert!((snap.secondary.as_ref().unwrap().used_percent - 80.0).abs() < f64::EPSILON);
    }

    #[test]
    fn regex_weekly_only_does_not_error_on_missing_rolling() {
        let provider = OpenCodeProvider::new();
        // Not valid JSON for serde (trailing noise) so we exercise regex path.
        let text = r#"weeklyUsage: { usagePercent: 55, resetInSec: 99 } not-json"#;
        let snap = provider.parse_subscription(text).expect("snapshot");
        assert!((snap.primary.used_percent - 55.0).abs() < f64::EPSILON);
    }

    #[test]
    fn normalizer_accepts_settings_ids_and_console_urls() {
        // The pinned-workspace shortcut reuses the console normalizer, so a
        // pasted console URL must resolve to the same bare id.
        assert_eq!(
            console_api::normalize_workspace_id("https://opencode.ai/console/wrk_abc123/billing")
                .as_deref(),
            Some("wrk_abc123")
        );
        assert_eq!(
            console_api::normalize_workspace_id(" org_team-1 ").as_deref(),
            Some("org_team-1")
        );
        assert_eq!(console_api::normalize_workspace_id("zen.design"), None);
    }

    #[test]
    fn pinned_workspace_short_circuits_discovery() {
        // A usable pin is authoritative for both chains: no org-list request is
        // made and the console URL form is reduced to its bare id.
        assert_eq!(
            OpenCodeProvider::pinned_workspace(Some(
                "https://opencode.ai/console/wrk_pinned/billing"
            ))
            .expect("usable pin")
            .as_deref(),
            Some("wrk_pinned")
        );
        assert_eq!(
            OpenCodeProvider::pinned_workspace(Some(" org_team_1 "))
                .expect("usable pin")
                .as_deref(),
            Some("org_team_1")
        );
        // No pin (or a blank one) keeps the discovery fallback available.
        assert_eq!(
            OpenCodeProvider::pinned_workspace(None).expect("no pin"),
            None
        );
        assert_eq!(
            OpenCodeProvider::pinned_workspace(Some("  ")).expect("blank pin"),
            None
        );
        // An explicitly set pin that cannot be normalized must not fall back to
        // discovery behind the user's back.
        match OpenCodeProvider::pinned_workspace(Some("zen.design")) {
            Err(ProviderError::Parse(message)) => {
                assert_eq!(message, "Invalid OpenCode workspace ID")
            }
            other => panic!("invalid pin must be rejected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn invalid_pin_is_rejected_before_any_chain_or_discovery() {
        let provider = OpenCodeProvider::new();
        let ctx = FetchContext {
            source_mode: SourceMode::Web,
            manual_cookie_header: Some("session=abc".to_string()),
            workspace_id: Some("not-a-workspace".to_string()),
            ..FetchContext::default()
        };

        // No mock server: reaching console or legacy discovery would mean the
        // invalid pin was silently ignored, and the returned error would not be
        // `Parse`.
        let error = provider
            .fetch_usage(&ctx)
            .await
            .expect_err("invalid pin must not fall back to discovery");

        match error {
            ProviderError::Parse(message) => assert_eq!(message, "Invalid OpenCode workspace ID"),
            other => panic!("expected Parse, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn legacy_chain_uses_the_pinned_workspace_instead_of_discovering_one() {
        let mut server = mockito::Server::new_async().await;
        // Only the billing call is mocked. Discovery would use the workspaces
        // server id, so a discovery attempt has no mock and fails the request.
        let subscription = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Regex("wrk_pinned".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"rollingUsage":{"usagePercent":42,"resetInSec":600}}"#)
            .expect(1)
            .create_async()
            .await;

        let provider = OpenCodeProvider::new();
        let snapshot = provider
            .fetch_with_cookies_at(
                &server.url(),
                "session=abc",
                Some("https://opencode.ai/console/wrk_pinned/billing"),
            )
            .await
            .expect("pinned workspace should be used without discovery");

        assert!(
            (snapshot.primary.used_percent - 42.0).abs() < f64::EPSILON,
            "parsed the pinned workspace subscription: {}",
            snapshot.primary.used_percent
        );
        subscription.assert_async().await;
    }

    #[tokio::test]
    async fn legacy_workspace_403_is_not_reported_as_a_signed_out_session() {
        let mut server = mockito::Server::new_async().await;
        // Discovery answers 403 (WAF/proxy style). That is a diagnostic, not an
        // expired cookie, so the caller must not be told to sign in again.
        let workspace = server
            .mock("GET", mockito::Matcher::Any)
            .with_status(403)
            .with_body("blocked")
            .create_async()
            .await;

        let provider = OpenCodeProvider::new();
        let error = provider
            .fetch_with_cookies_at(&server.url(), "session=abc", None)
            .await
            .expect_err("403 must not succeed");

        assert!(
            matches!(error, ProviderError::Other(_)),
            "403 should stay a diagnostic, got {error:?}"
        );
        if let ProviderError::Other(message) = &error {
            assert!(
                message.contains("403"),
                "message keeps the status: {message}"
            );
            assert!(
                !message.contains("blocked"),
                "response body must not leak into the error: {message}"
            );
        }
        workspace.assert_async().await;
    }

    #[tokio::test]
    async fn legacy_subscription_403_is_not_reported_as_a_signed_out_session() {
        let mut server = mockito::Server::new_async().await;
        let subscription = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Regex("wrk_pinned".to_string()))
            .with_status(403)
            .with_body("blocked")
            .create_async()
            .await;

        let provider = OpenCodeProvider::new();
        let error = provider
            .fetch_with_cookies_at(&server.url(), "session=abc", Some("wrk_pinned"))
            .await
            .expect_err("403 must not succeed");

        assert!(
            matches!(error, ProviderError::Other(_)),
            "subscription 403 should stay a diagnostic, got {error:?}"
        );
        subscription.assert_async().await;
    }

    #[tokio::test]
    async fn legacy_401_still_requires_auth() {
        let mut server = mockito::Server::new_async().await;
        let subscription = server
            .mock("GET", mockito::Matcher::Any)
            .match_query(mockito::Matcher::Regex("wrk_pinned".to_string()))
            .with_status(401)
            .create_async()
            .await;

        let provider = OpenCodeProvider::new();
        let error = provider
            .fetch_with_cookies_at(&server.url(), "session=abc", Some("wrk_pinned"))
            .await
            .expect_err("401 must not succeed");

        assert!(
            matches!(error, ProviderError::AuthRequired),
            "401 must still map to AuthRequired, got {error:?}"
        );
        subscription.assert_async().await;
    }

    #[test]
    fn console_requests_keep_their_own_timeout_budget() {
        // The console chain must not consume the legacy chain's budget: 10s per
        // console call, matching the Go console constant, not `web_timeout`.
        assert_eq!(CONSOLE_REQUEST_TIMEOUT, std::time::Duration::from_secs(10));
    }

    #[tokio::test]
    async fn manual_or_off_cookie_source_never_borrows_the_browser_cookie() {
        let provider = OpenCodeProvider::new();
        let ctx = FetchContext {
            source_mode: SourceMode::Web,
            manual_cookie_header: None,
            allow_browser_cookie: false,
            ..FetchContext::default()
        };

        let error = provider
            .fetch_usage(&ctx)
            .await
            .expect_err("browser identity must not be borrowed");

        assert!(matches!(error, ProviderError::NoCookies), "got {error:?}");
    }
}
