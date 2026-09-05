//! Codex API client for fetching usage information
//!
//! Uses OAuth tokens stored by the Codex CLI in ~/.codex/auth.json

use crate::core::{CostSnapshot, NamedRateWindow, ProviderError, RateWindow, UsageSnapshot};
use chrono::{DateTime, TimeZone, Utc};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api";
const USAGE_PATH: &str = "/wham/usage";
const RESET_CREDITS_PATH: &str = "/wham/rate-limit-reset-credits";
const CREDENTIAL_CACHE_TTL: Duration = Duration::from_secs(5);
/// Consecutive agreeing reset-credit observations required before a missing
/// value may be backfilled (UP-W-020 double-sample evidence).
const RESET_CREDIT_DOUBLE_SAMPLE: u8 = 2;

static CREDENTIAL_CACHE: OnceLock<Mutex<Option<CachedCodexCredentials>>> = OnceLock::new();

/// Last confirmed reset-credit inventory, used to backfill a temporarily
/// missing `/wham/rate-limit-reset-credits` answer (UP-W-020).
///
/// The provider is single-account today; if multi-account lands (UP-W-011) this
/// must become per-account like the credential cache.
static RESET_CREDIT_EVIDENCE: OnceLock<Mutex<Option<ResetCreditEvidence>>> = OnceLock::new();

/// Double-sample evidence for one reset-credit count.
///
/// A single observation is not trusted: the endpoint is a supplemental fetch
/// that can return stale or partial data, and resurrecting a one-off count
/// would show the user credits they no longer have. Only two consecutive
/// agreeing samples qualify, and the evidence is consumed by the backfill
/// (once-only), so a permanently failing endpoint never keeps showing an old
/// count forever.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResetCreditEvidence {
    available_count: u32,
    /// Consecutive agreeing samples observed so far (capped at 2).
    samples: u8,
    /// Whether this evidence has already been backfilled once.
    backfilled: bool,
}

impl ResetCreditEvidence {
    fn new(available_count: u32) -> Self {
        Self {
            available_count,
            samples: 1,
            backfilled: false,
        }
    }

    /// Record a fresh real observation. A differing count restarts the run; an
    /// agreeing one advances it. A real answer (even 0) supersedes any earlier
    /// value and re-arms the evidence for a future backfill.
    fn record(&mut self, available_count: u32) {
        if self.available_count == available_count {
            self.samples = self.samples.saturating_add(1).min(RESET_CREDIT_DOUBLE_SAMPLE);
            self.backfilled = false;
        } else {
            *self = Self::new(available_count);
        }
    }

    /// Once-only backfill from double-sample evidence.
    ///
    /// Returns the count to backfill exactly once per evidence: the first call
    /// marks the evidence consumed, so a second call (e.g. another refresh
    /// still missing the endpoint) returns `None` instead of resurrecting the
    /// same count again.
    fn try_backfill(&mut self) -> Option<u32> {
        if self.samples >= RESET_CREDIT_DOUBLE_SAMPLE && !self.backfilled {
            self.backfilled = true;
            return Some(self.available_count);
        }
        None
    }
}

/// Record a successful reset-credit observation into the shared evidence.
fn record_reset_credit_sample(available_count: u32) {
    if let Ok(mut guard) = RESET_CREDIT_EVIDENCE
        .get_or_init(|| Mutex::new(None))
        .lock()
    {
        match guard.as_mut() {
            Some(evidence) => evidence.record(available_count),
            None => *guard = Some(ResetCreditEvidence::new(available_count)),
        }
    }
}

/// Attempt the once-only reset-credit backfill; returns the count to show.
fn try_backfill_reset_credit() -> Option<u32> {
    let Some(mut guard) = RESET_CREDIT_EVIDENCE.get_or_init(|| Mutex::new(None)).lock().ok() else {
        return None;
    };
    guard.as_mut().and_then(ResetCreditEvidence::try_backfill)
}

/// Codex API client
pub struct CodexApi {
    client: reqwest::Client,
    home_dir: PathBuf,
}

impl CodexApi {
    pub fn new() -> Self {
        // Build client with proper TLS settings
        let client = crate::core::credentialed_http_client_builder()
            .use_rustls_tls()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            client,
            home_dir: dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
        }
    }

    /// Fetch usage information from Codex API
    /// Returns (UsageSnapshot, optional CostSnapshot)
    pub async fn fetch_usage(
        &self,
    ) -> Result<(UsageSnapshot, Option<CostSnapshot>), ProviderError> {
        // Load credentials
        let creds = self.load_credentials()?;

        // Build request URL
        let base_url = self.resolve_base_url();
        let url = format!("{}{}", base_url, USAGE_PATH);

        // Build request
        let mut request = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", creds.access_token))
            .header("User-Agent", "CodexBar")
            .header("Accept", "application/json")
            .timeout(std::time::Duration::from_secs(30));

        if let Some(account_id) = &creds.account_id
            && !account_id.is_empty()
        {
            request = request.header("ChatGPT-Account-Id", account_id);
        }

        let response = request.send().await?;

        if response.status() == 401 || response.status() == 403 {
            return Err(ProviderError::AuthRequired);
        }

        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "Codex API returned {}",
                response.status()
            )));
        }

        // Parse as raw JSON first for flexibility
        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| ProviderError::Parse(e.to_string()))?;

        let (mut usage, cost) = self.build_result_from_json(&json)?;

        // Reset credits are supplemental inventory, not a quota cycle: a
        // count of zero means the user has none, and the count itself is shown
        // as an informational row ("N reset credits available"), never as a
        // percentage. UP-W-015: the row must not read as a real 0% window.
        match self.fetch_rate_limit_reset_credits(&creds, &base_url).await {
            Ok(reset_credits) => {
                let available_count = reset_credits.available_count;
                // A real answer — including zero — supersedes earlier evidence.
                record_reset_credit_sample(available_count);
                if available_count > 0 {
                    usage = usage.with_named_rate_window(reset_credits_named(
                        available_count,
                        reset_credits.available_expiries(),
                    ));
                }
            }
            Err(error) => {
                // UP-W-020: the endpoint is temporarily missing. Backfill the
                // last double-confirmed count, but only once per evidence.
                tracing::debug!(
                    "Codex reset credits temporarily unavailable ({error}); considering double-sample backfill"
                );
                if let Some(available_count) = try_backfill_reset_credit().filter(|&count| count > 0) {
                    usage = usage.with_named_rate_window(reset_credits_named(
                        available_count,
                        Vec::new(),
                    ));
                }
            }
        }
        Ok((usage, cost))
    }

    async fn fetch_rate_limit_reset_credits(
        &self,
        creds: &CodexCredentials,
        base_url: &str,
    ) -> Result<ResetCredits, ProviderError> {
        let mut request = self
            .client
            .get(format!("{}{}", base_url, RESET_CREDITS_PATH))
            .header("Authorization", format!("Bearer {}", creds.access_token))
            .header("User-Agent", "CodexBar")
            .header("Accept", "application/json");
        if let Some(account_id) = &creds.account_id
            && !account_id.is_empty()
        {
            request = request.header("ChatGPT-Account-Id", account_id);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "Codex reset credits returned {}",
                response.status()
            )));
        }
        decode_reset_credits(&response.bytes().await?)
    }

    fn load_credentials(&self) -> Result<CodexCredentials, ProviderError> {
        let auth_path = self.get_auth_path();

        if !auth_path.exists() {
            return Err(ProviderError::NotInstalled(
                "Codex auth.json not found. Run `codex login` in a terminal to sign in."
                    .to_string(),
            ));
        }

        let modified = std::fs::metadata(&auth_path)
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        if let Some(cached) = Self::cached_credentials(&auth_path, modified) {
            return Ok(cached);
        }

        let content = std::fs::read_to_string(&auth_path).map_err(|e| {
            ProviderError::Other(format!("Failed to read Codex credentials: {}", e))
        })?;

        let credentials = Self::parse_credentials_json(&content)?;
        Self::store_cached_credentials(auth_path, modified, credentials.clone());
        Ok(credentials)
    }

    fn parse_credentials_json(content: &str) -> Result<CodexCredentials, ProviderError> {
        let json: serde_json::Value = serde_json::from_str(content)
            .map_err(|e| ProviderError::Parse(format!("Invalid Codex credentials JSON: {}", e)))?;

        // Check for OPENAI_API_KEY first
        if let Some(api_key) = json.get("OPENAI_API_KEY").and_then(|v| v.as_str()) {
            let trimmed = api_key.trim();
            if !trimmed.is_empty() {
                return Ok(CodexCredentials {
                    access_token: trimmed.to_string(),
                    account_id: None,
                });
            }
        }

        // Otherwise, look for tokens object
        let tokens = json.get("tokens").ok_or_else(|| {
            ProviderError::Parse("Codex auth.json exists but contains no tokens.".to_string())
        })?;

        let access_token = tokens
            .get("access_token")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ProviderError::Parse("Missing access_token in Codex credentials".to_string())
            })?
            .to_string();

        let account_id = tokens
            .get("account_id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        Ok(CodexCredentials {
            access_token,
            account_id,
        })
    }

    fn credential_cache() -> &'static Mutex<Option<CachedCodexCredentials>> {
        CREDENTIAL_CACHE.get_or_init(|| Mutex::new(None))
    }

    fn cached_credentials(
        path: &std::path::Path,
        modified: Option<SystemTime>,
    ) -> Option<CodexCredentials> {
        let guard = Self::credential_cache().lock().ok()?;
        let cached = guard.as_ref()?;
        if cached.path == path
            && cached.modified == modified
            && cached.loaded_at.elapsed() <= CREDENTIAL_CACHE_TTL
        {
            return Some(cached.credentials.clone());
        }
        None
    }

    fn store_cached_credentials(
        path: PathBuf,
        modified: Option<SystemTime>,
        credentials: CodexCredentials,
    ) {
        if let Ok(mut guard) = Self::credential_cache().lock() {
            *guard = Some(CachedCodexCredentials {
                path,
                modified,
                loaded_at: Instant::now(),
                credentials,
            });
        }
    }

    fn get_auth_path(&self) -> PathBuf {
        // Check CODEX_HOME env var
        if let Ok(codex_home) = std::env::var("CODEX_HOME") {
            let trimmed = codex_home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("auth.json");
            }
        }

        self.home_dir.join(".codex").join("auth.json")
    }

    fn resolve_base_url(&self) -> String {
        // Check CODEX_HOME for config.toml
        let config_path = if let Ok(codex_home) = std::env::var("CODEX_HOME") {
            let trimmed = codex_home.trim();
            if !trimmed.is_empty() {
                PathBuf::from(trimmed).join("config.toml")
            } else {
                self.home_dir.join(".codex").join("config.toml")
            }
        } else {
            self.home_dir.join(".codex").join("config.toml")
        };

        if let Ok(content) = std::fs::read_to_string(&config_path)
            && let Some(base_url) = parse_chatgpt_base_url(&content)
        {
            let normalized = normalize_base_url(&base_url);
            // Only allow HTTPS URLs for custom base URLs to prevent token exfiltration
            if normalized.starts_with("https://")
                || normalized.starts_with("http://127.0.0.1")
                || normalized.starts_with("http://localhost")
            {
                return normalized;
            }
            tracing::warn!(
                "Ignoring insecure custom chatgpt_base_url (must be HTTPS): {}",
                normalized
            );
        }

        DEFAULT_BASE_URL.to_string()
    }

    fn build_result_from_json(
        &self,
        json: &serde_json::Value,
    ) -> Result<(UsageSnapshot, Option<CostSnapshot>), ProviderError> {
        // Extract plan type
        let plan_type = json
            .get("plan_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Extract rate limit info - handle multiple possible structures
        let (primary, secondary, code_review) = self.extract_rate_limits(json);

        // Build login method string
        let login_method = plan_type.as_ref().map(|pt| match pt.as_str() {
            "guest" => "Guest".to_string(),
            "free" => "ChatGPT Free".to_string(),
            "go" => "Codex Go".to_string(),
            "plus" => "ChatGPT Plus".to_string(),
            "pro" | "pro_lite" | "prolite" | "pro-lite" => {
                if pt == "pro" {
                    "ChatGPT Pro".to_string()
                } else {
                    "Pro Lite".to_string()
                }
            }
            "team" => "ChatGPT Team".to_string(),
            "business" => "ChatGPT Business".to_string(),
            "enterprise" => "ChatGPT Enterprise".to_string(),
            "education" | "edu" => "ChatGPT Education".to_string(),
            "free_workspace" | "freeWorkspace" => "Free Workspace".to_string(),
            "quorum" => "Codex Quorum".to_string(),
            "k12" => "Codex K12".to_string(),
            other => format!("ChatGPT {}", capitalize(other)),
        });

        let mut usage = UsageSnapshot::new(primary);
        if let Some(sec) = secondary {
            usage = usage.with_secondary(sec);
        }
        if let Some(cr) = code_review {
            usage = usage.with_model_specific(cr);
        }
        for extra in self.extract_additional_rate_limits(json) {
            usage.extra_rate_windows.push(extra);
        }
        if let Some(method) = login_method {
            usage = usage.with_login_method(method);
        }

        // Extract credits if present
        let cost = self.extract_credits(json);

        Ok((usage, cost))
    }

    fn extract_rate_limits(
        &self,
        json: &serde_json::Value,
    ) -> (RateWindow, Option<RateWindow>, Option<RateWindow>) {
        // Try rate_limit object
        if let Some(rate_limit) = json.get("rate_limit") {
            let primary_opt = rate_limit
                .get("primary_window")
                .and_then(|w| self.parse_window_if_present(w));

            let secondary_opt = rate_limit
                .get("secondary_window")
                .and_then(|w| self.parse_window_if_present(w));

            let code_review = rate_limit
                .get("code_review_window")
                .and_then(|w| self.parse_window_if_present(w));

            // If primary is missing or a placeholder, promote secondary to
            // primary (weekly-only plans) so the weekly quota is still
            // recognised without a 5-hour window (UP-W-015). When nothing
            // real remains, the slot carries an informational placeholder
            // instead of a fabricated 0% window.
            let (primary, secondary) = match (primary_opt, secondary_opt) {
                (Some(p), s) => (p, s),
                (None, Some(s)) => (s, None),
                (None, None) => (no_active_session_window(), None),
            };

            return (primary, secondary, code_review);
        }

        // Try rate_limits array
        if let Some(rate_limits) = json.get("rate_limits").and_then(|v| v.as_array())
            && let Some(first) = rate_limits.first()
        {
            let primary = self.parse_window_if_present(first).unwrap_or_else(no_active_session_window);
            let secondary = rate_limits
                .get(1)
                .and_then(|w| self.parse_window_if_present(w));
            let code_review = rate_limits
                .get(2)
                .and_then(|w| self.parse_window_if_present(w));
            return (primary, secondary, code_review);
        }

        // Try direct fields
        let used_percent = json
            .get("used_percent")
            .or_else(|| json.get("usage_percent"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        (RateWindow::new(used_percent), None, None)
    }

    fn parse_window(&self, window: &serde_json::Value) -> RateWindow {
        let used_percent = window
            .get("used_percent")
            .or_else(|| window.get("usage_percent"))
            .and_then(json_f64)
            .unwrap_or(0.0);

        let window_minutes = window
            .get("limit_window_seconds")
            .and_then(|v| v.as_i64())
            .map(|s| (s / 60) as u32);

        let reset_at = window
            .get("reset_at")
            .and_then(|v| v.as_i64())
            .and_then(|ts| Utc.timestamp_opt(ts, 0).single());

        RateWindow::with_details(
            used_percent,
            window_minutes,
            reset_at,
            format_reset_countdown(reset_at),
        )
    }

    /// Parse a window only when it is real data — a `null` or an empty
    /// placeholder object (`{}`) is an API omission, not a 0% quota, and must
    /// not become a fake window (UP-W-015).
    fn parse_window_if_present(&self, window: &serde_json::Value) -> Option<RateWindow> {
        (!window.is_null() && !is_placeholder_window(window)).then(|| self.parse_window(window))
    }

    fn extract_additional_rate_limits(&self, json: &serde_json::Value) -> Vec<NamedRateWindow> {
        json.get("additional_rate_limits")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|entry| self.parse_additional_rate_limit(entry))
            .collect()
    }

    fn parse_additional_rate_limit(&self, entry: &serde_json::Value) -> Option<NamedRateWindow> {
        let metered_feature = entry
            .get("metered_feature")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let limit_name = entry
            .get("limit_name")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty());

        let rate_limit = entry.get("rate_limit").unwrap_or(entry);
        let primary = rate_limit.get("primary_window");
        let secondary = rate_limit.get("secondary_window");
        let window = primary.or(secondary)?;
        if is_placeholder_window(window) {
            return None;
        }

        let parsed = self.parse_window(window);
        let feature = metered_feature.unwrap_or_default();
        let limit = limit_name.unwrap_or_default();
        let is_spark = feature.eq_ignore_ascii_case("codex_spark")
            || feature.eq_ignore_ascii_case("spark")
            || limit.to_ascii_lowercase().contains("spark");

        if is_spark {
            let is_weekly = secondary.is_some() && primary.is_none()
                || parsed
                    .window_minutes
                    .is_some_and(|mins| mins >= 7 * 24 * 60);
            let (id, title) = if is_weekly {
                ("codex-spark-weekly", "Codex Spark Weekly")
            } else {
                ("codex-spark", "Codex Spark 5-hour")
            };
            return Some(NamedRateWindow::new(id, title, parsed));
        }

        let label = limit_name.or(metered_feature)?;
        let slug = slugify(label);
        if slug.is_empty() {
            return None;
        }

        Some(NamedRateWindow::new(
            format!("codex-{slug}"),
            titleize_limit_label(label),
            parsed,
        ))
    }

    fn extract_credits(&self, json: &serde_json::Value) -> Option<CostSnapshot> {
        let credits = json.get("credits")?;

        let has_credits = credits
            .get("has_credits")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !has_credits {
            return None;
        }

        let unlimited = credits
            .get("unlimited")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if unlimited {
            return None;
        }

        let balance = credits
            .get("balance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        Some(CostSnapshot::new(balance, "USD", "Credits"))
    }

    fn build_result(
        &self,
        response: UsageResponse,
    ) -> Result<(UsageSnapshot, Option<CostSnapshot>), ProviderError> {
        // Extract primary rate window
        let primary = if let Some(ref rate_limit) = response.rate_limit {
            if let Some(ref primary_window) = rate_limit.primary_window {
                let reset_at = timestamp_to_datetime(primary_window.reset_at);
                RateWindow::with_details(
                    primary_window.used_percent as f64,
                    primary_window.limit_window_seconds.map(|s| (s / 60) as u32),
                    reset_at,
                    format_reset_countdown(reset_at),
                )
            } else {
                RateWindow::new(0.0)
            }
        } else {
            RateWindow::new(0.0)
        };

        // Extract secondary rate window
        let secondary = response
            .rate_limit
            .as_ref()
            .and_then(|rl| rl.secondary_window.as_ref())
            .map(|window| {
                let reset_at = timestamp_to_datetime(window.reset_at);
                RateWindow::with_details(
                    window.used_percent as f64,
                    window.limit_window_seconds.map(|s| (s / 60) as u32),
                    reset_at,
                    format_reset_countdown(reset_at),
                )
            });

        // Extract code review rate window
        let code_review = response
            .rate_limit
            .as_ref()
            .and_then(|rl| rl.code_review_window.as_ref())
            .map(|window| {
                let reset_at = timestamp_to_datetime(window.reset_at);
                RateWindow::with_details(
                    window.used_percent as f64,
                    window.limit_window_seconds.map(|s| (s / 60) as u32),
                    reset_at,
                    format_reset_countdown(reset_at),
                )
            });

        // Build usage snapshot
        let login_method = response.plan_type.as_ref().map(|pt| match pt.as_str() {
            "guest" => "Guest".to_string(),
            "free" => "ChatGPT Free".to_string(),
            "go" => "ChatGPT Go".to_string(),
            "plus" => "ChatGPT Plus".to_string(),
            "pro" => "ChatGPT Pro".to_string(),
            "team" => "ChatGPT Team".to_string(),
            "business" => "ChatGPT Business".to_string(),
            "enterprise" => "ChatGPT Enterprise".to_string(),
            "education" | "edu" => "ChatGPT Education".to_string(),
            other => format!("ChatGPT {}", capitalize(other)),
        });

        let mut usage = UsageSnapshot::new(primary);
        if let Some(sec) = secondary {
            usage = usage.with_secondary(sec);
        }
        if let Some(cr) = code_review {
            usage = usage.with_model_specific(cr);
        }
        if let Some(method) = login_method {
            usage = usage.with_login_method(method);
        }

        // Build cost snapshot if credits are present
        let credit_limit = response.individual_limit.as_ref().or_else(|| {
            response
                .rate_limit
                .as_ref()
                .and_then(|rate_limit| rate_limit.individual_limit.as_ref())
        });
        let cost = response.credits.as_ref().and_then(|credits| {
            if credits.has_credits() {
                let balance = credits.balance.unwrap_or(0.0);
                if credits.unlimited() {
                    None // Unlimited credits, no need to show
                } else if let Some(limit) =
                    credit_limit.and_then(|limit| limit.to_cost_snapshot(balance))
                {
                    Some(limit)
                } else {
                    Some(CostSnapshot::new(balance, "USD", "Credits"))
                }
            } else {
                None
            }
        });

        Ok((usage, cost))
    }
}

impl Default for CodexApi {
    fn default() -> Self {
        Self::new()
    }
}

// --- Data structures ---

#[derive(Clone)]
struct CodexCredentials {
    access_token: String,
    account_id: Option<String>,
}

struct CachedCodexCredentials {
    path: PathBuf,
    modified: Option<SystemTime>,
    loaded_at: Instant,
    credentials: CodexCredentials,
}

#[derive(Debug, Deserialize)]
struct UsageResponse {
    plan_type: Option<String>,
    rate_limit: Option<RateLimitDetails>,
    credits: Option<CreditDetails>,
    #[serde(default, alias = "individualLimit")]
    individual_limit: Option<SpendControlLimitSnapshot>,
}

#[derive(Debug, Deserialize)]
struct RateLimitDetails {
    primary_window: Option<WindowSnapshot>,
    secondary_window: Option<WindowSnapshot>,
    code_review_window: Option<WindowSnapshot>,
    #[serde(default, alias = "individualLimit")]
    individual_limit: Option<SpendControlLimitSnapshot>,
}

#[derive(Debug, Deserialize)]
struct WindowSnapshot {
    used_percent: i32,
    reset_at: Option<i64>,
    limit_window_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreditDetails {
    has_credits: Option<bool>,
    unlimited: Option<bool>,
    balance: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct SpendControlLimitSnapshot {
    limit: Option<f64>,
    used: Option<f64>,
    #[serde(default, alias = "remainingPercent")]
    remaining_percent: Option<f64>,
    #[serde(default, alias = "resetsAt")]
    resets_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ResetCreditEntry {
    #[serde(default)]
    status: Option<String>,
    #[serde(default, alias = "expiresAt")]
    expires_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ResetCredits {
    #[serde(default)]
    credits: Vec<ResetCreditEntry>,
    #[serde(default)]
    available_count: u32,
}

impl ResetCredits {
    fn available_expiries(&self) -> Vec<DateTime<Utc>> {
        let mut out: Vec<DateTime<Utc>> = self
            .credits
            .iter()
            .filter(|credit| credit.is_available())
            .filter_map(|credit| parse_credit_expiry(credit.expires_at.as_deref()))
            .collect();
        out.sort();
        out
    }
}

impl ResetCreditEntry {
    fn is_available(&self) -> bool {
        match self.status.as_deref().map(str::trim) {
            None | Some("") => true,
            Some(status) => status.eq_ignore_ascii_case("available"),
        }
    }
}

/// The supplemental "N reset credits available" row.
///
/// Informational by design (UP-W-015): it carries inventory, not a percentage
/// quota, so no surface may render it as a real "0% used" window. `used_percent`
/// stays 0 and the description carries the count; the bridge flags the row
/// `is_informational` and quota windows everywhere skip it.
fn reset_credits_rate_window(available_count: u32) -> RateWindow {
    let description = format!(
        "{} reset credit{} available",
        available_count,
        if available_count == 1 { "" } else { "s" }
    );
    RateWindow::informational(description)
}

fn reset_credits_named(available_count: u32, expiries: Vec<DateTime<Utc>>) -> NamedRateWindow {
    NamedRateWindow::new(
        "reset-credits",
        "Reset credits",
        reset_credits_rate_window(available_count),
    )
    .with_inventory_expires_at(expiries)
}

fn parse_credit_expiry(raw: Option<&str>) -> Option<DateTime<Utc>> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(raw) {
        return Some(parsed.with_timezone(&Utc));
    }
    if let Ok(n) = raw.parse::<i64>() {
        return timestamp_to_datetime(Some(n));
    }
    None
}

/// Informational placeholder for a plan that has no active 5-hour session.
///
/// The window keeps a session length so cycle classification still recognises
/// the slot, while `is_informational` stops any surface from reading the 0% as
/// a real quota (UP-W-015: weekly-only plans must not show a fake 5-hour row).
fn no_active_session_window() -> RateWindow {
    let mut window = RateWindow::with_details(0.0, Some(5 * 60), None, Some("No active 5h session".to_string()));
    window.is_informational = true;
    window
}

fn decode_reset_credits(data: &[u8]) -> Result<ResetCredits, ProviderError> {
    serde_json::from_slice(data)
        .map_err(|e| ProviderError::Parse(format!("Failed to parse Codex reset credits: {e}")))
}

impl CreditDetails {
    // Helper to safely check has_credits
    fn has_credits(&self) -> bool {
        self.has_credits.unwrap_or(false)
    }

    fn unlimited(&self) -> bool {
        self.unlimited.unwrap_or(false)
    }
}

impl SpendControlLimitSnapshot {
    fn to_cost_snapshot(&self, balance: f64) -> Option<CostSnapshot> {
        let limit = self
            .limit
            .filter(|limit| limit.is_finite() && *limit >= 0.0)?;
        let used = self
            .used
            .filter(|used| used.is_finite() && *used >= 0.0)
            .or_else(|| {
                self.remaining_percent
                    .filter(|pct| pct.is_finite() && *pct >= 0.0)
                    .map(|remaining| limit * (1.0 - (remaining / 100.0)))
            })
            .unwrap_or_else(|| (limit - balance).max(0.0));
        let mut cost =
            CostSnapshot::new(used.clamp(0.0, limit), "USD", "Monthly credits").with_limit(limit);
        if let Some(resets_at) = timestamp_to_datetime(self.resets_at) {
            cost = cost.with_resets_at(resets_at);
        }
        Some(cost)
    }
}

// --- Helper functions ---

fn timestamp_to_datetime(timestamp: Option<i64>) -> Option<DateTime<Utc>> {
    timestamp.and_then(|ts| Utc.timestamp_opt(ts, 0).single())
}

fn json_f64(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|value| value as f64))
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
}

fn is_placeholder_window(window: &serde_json::Value) -> bool {
    let has_usage = window
        .get("used_percent")
        .or_else(|| window.get("usage_percent"))
        .and_then(json_f64)
        .is_some();
    let has_duration = window
        .get("limit_window_seconds")
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse::<i64>().ok()))
        .is_some();
    let has_reset = window.get("reset_at").is_some();

    !has_usage && !has_duration && !has_reset
}

fn slugify(label: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;

    for ch in label.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash && !slug.is_empty() {
            slug.push('-');
            previous_dash = true;
        }
    }

    while slug.ends_with('-') {
        slug.pop();
    }
    slug
}

fn titleize_limit_label(label: &str) -> String {
    label
        .split(['_', '-', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first
                    .to_uppercase()
                    .chain(chars.flat_map(char::to_lowercase))
                    .collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn format_reset_countdown(reset_at: Option<DateTime<Utc>>) -> Option<String> {
    let dt = reset_at?;
    let now = Utc::now();
    if dt <= now {
        return Some("now".to_string());
    }
    let diff = dt - now;
    let total_mins = diff.num_minutes();
    let hours = diff.num_hours();
    let mins = total_mins % 60;
    if hours >= 24 {
        let days = hours / 24;
        let rem_h = hours % 24;
        if rem_h == 0 {
            Some(format!("{}d", days))
        } else {
            Some(format!("{}d {}h", days, rem_h))
        }
    } else if hours > 0 {
        if mins == 0 {
            Some(format!("{}h", hours))
        } else {
            Some(format!("{}h {}m", hours, mins))
        }
    } else {
        Some(format!("{}m", mins))
    }
}

fn parse_chatgpt_base_url(config_content: &str) -> Option<String> {
    for line in config_content.lines() {
        // Skip comments
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        // Look for chatgpt_base_url = "..."
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if key == "chatgpt_base_url" {
                let mut value = value.trim();
                // Remove quotes
                if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    value = &value[1..value.len() - 1];
                }
                return Some(value.trim().to_string());
            }
        }
    }
    None
}

fn normalize_base_url(url: &str) -> String {
    let mut trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        return DEFAULT_BASE_URL.to_string();
    }

    // Remove trailing slashes
    while trimmed.ends_with('/') {
        trimmed.pop();
    }

    // Add /backend-api if needed
    if (trimmed.starts_with("https://chatgpt.com")
        || trimmed.starts_with("https://chat.openai.com"))
        && !trimmed.contains("/backend-api")
    {
        trimmed.push_str("/backend-api");
    }

    trimmed
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_codex_credentials_without_retaining_refresh_token() {
        let credentials = CodexApi::parse_credentials_json(
            r#"{
                "tokens": {
                    "access_token": "access",
                    "refresh_token": "refresh",
                    "account_id": "acct_123"
                }
            }"#,
        )
        .expect("credentials");

        assert_eq!(credentials.access_token, "access");
        assert_eq!(credentials.account_id.as_deref(), Some("acct_123"));
    }

    #[test]
    fn decodes_reset_credits() {
        let credits = decode_reset_credits(br#"{"available_count":2,"credits":[{"id":"a"}]}"#)
            .expect("reset credits");
        assert_eq!(credits.available_count, 2);
        assert_eq!(credits.credits.len(), 1);
    }

    #[test]
    fn reset_credit_expiries_are_soonest_first_and_skip_ids() {
        let credits = decode_reset_credits(
            br#"{
                "available_count": 2,
                "credits": [
                    {
                        "id": "RateLimitResetCredit_secret",
                        "status": "available",
                        "title": "Full reset (Weekly + 5 hr)",
                        "expires_at": "2026-07-18T02:39:26Z"
                    },
                    {
                        "id": "RateLimitResetCredit_other",
                        "status": "used",
                        "expires_at": "2026-07-01T00:00:00Z"
                    },
                    {
                        "status": "available",
                        "expires_at": "2026-07-12T01:33:14Z"
                    }
                ]
            }"#,
        )
        .expect("reset credits");
        let expiries = credits.available_expiries();
        assert_eq!(expiries.len(), 2);
        assert_eq!(expiries[0], DateTime::parse_from_rfc3339("2026-07-12T01:33:14Z").unwrap().with_timezone(&Utc));
        assert_eq!(expiries[1], DateTime::parse_from_rfc3339("2026-07-18T02:39:26Z").unwrap().with_timezone(&Utc));
        let named = reset_credits_named(credits.available_count, expiries);
        let encoded = serde_json::to_string(&named).expect("named");
        assert!(!encoded.contains("RateLimitResetCredit"));
        assert_eq!(named.inventory_expires_at.len(), 2);
        assert_eq!(named.window.resets_at, named.inventory_expires_at.first().copied());
    }

    #[test]
    fn maps_codex_spark_additional_rate_limits() {
        let api = CodexApi::new();
        let (usage, _) = api
            .build_result_from_json(&json!({
                "plan_type": "pro",
                "rate_limit": {
                    "primary_window": { "used_percent": 20, "limit_window_seconds": 18000 },
                    "secondary_window": { "used_percent": 40, "limit_window_seconds": 604800 }
                },
                "additional_rate_limits": [
                    {
                        "limit_name": "Codex Spark",
                        "metered_feature": "codex_spark",
                        "rate_limit": {
                            "primary_window": { "used_percent": "17", "limit_window_seconds": 18000 }
                        }
                    },
                    {
                        "limit_name": "Codex Spark Weekly",
                        "metered_feature": "codex_spark",
                        "rate_limit": {
                            "secondary_window": { "used_percent": 62, "limit_window_seconds": 604800 }
                        }
                    }
                ]
            }))
            .expect("codex usage");

        assert_eq!(usage.extra_rate_windows.len(), 2);
        assert_eq!(usage.extra_rate_windows[0].id, "codex-spark");
        assert_eq!(usage.extra_rate_windows[0].title, "Codex Spark 5-hour");
        assert_eq!(usage.extra_rate_windows[0].window.used_percent, 17.0);
        assert_eq!(usage.extra_rate_windows[1].id, "codex-spark-weekly");
        assert_eq!(usage.extra_rate_windows[1].title, "Codex Spark Weekly");
        assert_eq!(usage.extra_rate_windows[1].window.used_percent, 62.0);
    }

    #[test]
    fn ignores_placeholder_additional_rate_limits() {
        let api = CodexApi::new();
        let (usage, _) = api
            .build_result_from_json(&json!({
                "rate_limit": {
                    "primary_window": { "used_percent": 0, "limit_window_seconds": 18000 }
                },
                "additional_rate_limits": [
                    {
                        "limit_name": "placeholder",
                        "metered_feature": "placeholder",
                        "rate_limit": { "primary_window": {} }
                    }
                ]
            }))
            .expect("codex usage");

        assert!(usage.extra_rate_windows.is_empty());
    }

    #[test]
    fn maps_top_level_individual_credit_limit_to_cost_snapshot() {
        let api = CodexApi::new();
        let (_, cost) = api
            .build_result(UsageResponse {
                plan_type: None,
                rate_limit: None,
                credits: Some(CreditDetails {
                    has_credits: Some(true),
                    unlimited: Some(false),
                    balance: Some(7.5),
                }),
                individual_limit: Some(SpendControlLimitSnapshot {
                    limit: Some(20.0),
                    used: Some(12.5),
                    remaining_percent: None,
                    resets_at: Some(1783036800),
                }),
            })
            .expect("codex result");
        let cost = cost.expect("cost");
        assert_eq!(cost.used, 12.5);
        assert_eq!(cost.limit, Some(20.0));
        assert!(cost.resets_at.is_some());
    }

    #[test]
    fn maps_nested_individual_credit_limit_to_cost_snapshot() {
        let api = CodexApi::new();
        let (_, cost) = api
            .build_result(UsageResponse {
                plan_type: None,
                rate_limit: Some(RateLimitDetails {
                    primary_window: None,
                    secondary_window: None,
                    code_review_window: None,
                    individual_limit: Some(SpendControlLimitSnapshot {
                        limit: Some(100.0),
                        used: None,
                        remaining_percent: Some(60.0),
                        resets_at: None,
                    }),
                }),
                credits: Some(CreditDetails {
                    has_credits: Some(true),
                    unlimited: Some(false),
                    balance: Some(60.0),
                }),
                individual_limit: None,
            })
            .expect("codex result");
        let cost = cost.expect("cost");
        assert_eq!(cost.used, 40.0);
        assert_eq!(cost.limit, Some(100.0));
    }

    // ── UP-W-015: placeholder/informational window recognition ────────────

    #[test]
    fn placeholder_primary_is_skipped_and_weekly_promoted() {
        // A `{}` placeholder primary must not read as a real 0% window; the
        // real weekly window takes the primary slot (Codex weekly-only plans
        // without a 5-hour window).
        let api = CodexApi::new();
        let (usage, _) = api
            .build_result_from_json(&json!({
                "rate_limit": {
                    "primary_window": {},
                    "secondary_window": {
                        "used_percent": 25,
                        "limit_window_seconds": 604800,
                        "reset_at": 1783036800
                    }
                }
            }))
            .expect("codex usage");

        assert!(!usage.primary.is_informational);
        assert_eq!(usage.primary.used_percent, 25.0);
        assert_eq!(usage.primary.window_minutes, Some(10080));
        assert!(usage.secondary.is_none());
    }

    #[test]
    fn null_primary_window_is_skipped() {
        // A JSON `null` window is an API omission, not a 0% quota.
        let api = CodexApi::new();
        let (usage, _) = api
            .build_result_from_json(&json!({
                "rate_limit": {
                    "primary_window": null,
                    "secondary_window": {
                        "used_percent": 25,
                        "limit_window_seconds": 604800,
                        "reset_at": 1783036800
                    }
                }
            }))
            .expect("codex usage");

        assert!(!usage.primary.is_informational);
        assert_eq!(usage.primary.window_minutes, Some(10080));
        assert!(usage.secondary.is_none());
    }

    #[test]
    fn missing_all_windows_yields_informational_placeholder() {
        // No real windows at all: the slot must not fabricate a 0% quota.
        let api = CodexApi::new();
        let (usage, _) = api
            .build_result_from_json(&json!({
                "rate_limit": {}
            }))
            .expect("codex usage");

        assert!(usage.primary.is_informational);
        assert_eq!(
            usage.primary.reset_description.as_deref(),
            Some("No active 5h session")
        );
    }

    #[test]
    fn placeholder_first_array_window_is_skipped() {
        // The `rate_limits` array can lead with an empty placeholder; it must
        // not become a fake primary quota. Positional mapping is preserved:
        // the placeholder yields an informational session slot and the real
        // weekly window stays in `secondary`, where window-by-kind surfaces
        // find it (UP-W-015).
        let api = CodexApi::new();
        let (usage, _) = api
            .build_result_from_json(&json!({
                "rate_limits": [
                    {},
                    { "used_percent": 25, "limit_window_seconds": 604800, "reset_at": 1783036800 }
                ]
            }))
            .expect("codex usage");

        assert!(usage.primary.is_informational);
        assert_eq!(
            usage.primary.reset_description.as_deref(),
            Some("No active 5h session")
        );
        let weekly = usage.secondary.expect("weekly window");
        assert!(!weekly.is_informational);
        assert_eq!(weekly.used_percent, 25.0);
        assert_eq!(weekly.window_minutes, Some(10080));
    }

    #[test]
    fn reset_credits_window_is_informational_not_a_quota() {
        let window = reset_credits_rate_window(2);
        assert!(window.is_informational);
        assert_eq!(window.used_percent, 0.0);
        assert_eq!(
            window.reset_description.as_deref(),
            Some("2 reset credits available")
        );

        let single = reset_credits_rate_window(1);
        assert_eq!(
            single.reset_description.as_deref(),
            Some("1 reset credit available")
        );
    }

    // ── UP-W-020: double-sample reset-credit backfill ─────────────────────

    #[test]
    fn backfill_requires_double_sample_evidence() {
        let mut evidence = ResetCreditEvidence::new(2);
        // One observation is not enough: a single sample could be stale.
        assert_eq!(evidence.try_backfill(), None);
        // Two consecutive agreeing observations confirm the count.
        evidence.record(2);
        assert_eq!(evidence.try_backfill(), Some(2));
    }

    #[test]
    fn backfill_resets_on_differing_count() {
        let mut evidence = ResetCreditEvidence::new(2);
        evidence.record(2);
        assert_eq!(evidence.try_backfill(), Some(2));
        // A new real answer with a different count restarts the run; the old
        // evidence is not trusted for the new count.
        evidence.record(3);
        assert_eq!(evidence.try_backfill(), None);
        evidence.record(3);
        assert_eq!(evidence.try_backfill(), Some(3));
    }

    #[test]
    fn backfill_runs_only_once_per_evidence() {
        // UP-W-020 idempotency: the same evidence is backfilled exactly once.
        let mut evidence = ResetCreditEvidence::new(2);
        evidence.record(2);
        assert_eq!(evidence.try_backfill(), Some(2));
        assert_eq!(evidence.try_backfill(), None);
        assert_eq!(evidence.try_backfill(), None);
    }

    #[test]
    fn real_zero_supersedes_positive_evidence() {
        // A real "0 available" answer must never be overwritten by a stale
        // positive backfill.
        let mut evidence = ResetCreditEvidence::new(2);
        evidence.record(2);
        assert_eq!(evidence.try_backfill(), Some(2));
        evidence.record(0);
        assert_eq!(evidence.try_backfill(), None);
    }

    #[test]
    fn fresh_observation_rearms_evidence() {
        // After a backfill, a fresh real observation re-arms the evidence so a
        // later outage can backfill once more; the consumed state is not
        // carried into the new sample run.
        let mut evidence = ResetCreditEvidence::new(2);
        evidence.record(2);
        assert_eq!(evidence.try_backfill(), Some(2));
        evidence.record(2);
        assert_eq!(evidence.try_backfill(), Some(2));
        assert_eq!(evidence.try_backfill(), None);
    }
}
