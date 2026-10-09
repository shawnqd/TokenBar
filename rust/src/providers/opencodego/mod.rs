//! OpenCode Go provider implementation
//!
//! Separate workspace surface that shares the `opencode.ai` cookie domain with
//! the OpenCode provider. Auto mode uses the upstream local SQLite estimate →
//! authenticated usage API → web Cookie ladder; the web parser remains the
//! defensive SSR-aware implementation used by TokenBar.

pub(crate) mod local;
pub use local::{MessageSpeed, recent_message_speeds};
mod usage_api;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;
use std::time::Duration;
use uuid::Uuid;

use crate::core::{
    CostSnapshot, FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId,
    ProviderMetadata, RateWindow, SourceMode, UsageSnapshot,
};

const BASE_URL: &str = "https://opencode.ai";
const SERVER_URL: &str = "https://opencode.ai/_server";
const WORKSPACES_SERVER_ID: &str =
    "def39973159c7f0483d8793a822b8dbb10d067e12c65455fcb4608459ba0234f";
const BILLING_SERVER_ID: &str = "c83b78a614689c38ebee981f9b39a8b377716db85c1fd7dbab604adc02d3313d";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

pub(crate) const LOCAL_ESTIMATE_SOURCE_LABEL: &str = "local estimate";
const ZEN_BALANCE_TIMEOUT: Duration = Duration::from_secs(5);
const ZEN_BALANCE_START_DELAY: Duration = Duration::from_millis(25);
const ZEN_BALANCE_JOIN_GRACE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
struct WindowCandidate {
    used_percent: f64,
    reset_in_sec: i64,
    /// OpenCode marks depleted windows `status: "rate-limited"`.
    status_ok: bool,
    /// True when the object carried an explicit used/usagePercent field.
    has_usage_key: bool,
}

pub struct OpenCodeGoProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl OpenCodeGoProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::OpenCodeGo,
                display_name: "OpenCode Go",
                session_label: "Rolling",
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

    fn workspace_id_from_context(workspace_id: Option<&str>) -> Option<&str> {
        workspace_id.filter(|id| !id.is_empty())
    }

    async fn fetch_workspace_id(&self, cookie_header: &str) -> Result<String, ProviderError> {
        Self::fetch_workspace_id_with_client(&self.client, cookie_header).await
    }

    async fn fetch_workspace_id_with_client(
        client: &Client,
        cookie_header: &str,
    ) -> Result<String, ProviderError> {
        let url = format!("{}?id={}", SERVER_URL, WORKSPACES_SERVER_ID);
        let response = client
            .get(&url)
            .header("Cookie", cookie_header)
            .header("X-Server-Id", WORKSPACES_SERVER_ID)
            .header("X-Server-Instance", format!("server-fn:{}", Uuid::new_v4()))
            .header("User-Agent", USER_AGENT)
            .header("Origin", BASE_URL)
            .header("Referer", BASE_URL)
            .header(
                "Accept",
                "text/javascript, application/json;q=0.9, */*;q=0.8",
            )
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(ProviderError::AuthRequired);
            }
            return Err(ProviderError::Other(format!(
                "OpenCode workspace API returned {}",
                status
            )));
        }

        let text = response.text().await?;
        if Self::looks_signed_out(&text) {
            return Err(ProviderError::AuthRequired);
        }

        let ids = Self::parse_workspace_ids(&text);
        ids.into_iter()
            .next()
            .ok_or_else(|| ProviderError::Parse("No workspace ID found".to_string()))
    }

    async fn fetch_usage_page(
        &self,
        workspace_id: &str,
        cookie_header: &str,
    ) -> Result<String, ProviderError> {
        let url = format!("{}/workspace/{}/go", BASE_URL, workspace_id);
        let response = self
            .client
            .get(&url)
            .header("Cookie", cookie_header)
            .header("User-Agent", USER_AGENT)
            .header("Referer", BASE_URL)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            )
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            if status.as_u16() == 401 || status.as_u16() == 403 {
                return Err(ProviderError::AuthRequired);
            }
            return Err(ProviderError::Other(format!(
                "OpenCode Go usage page returned {}",
                status
            )));
        }

        let text = response.text().await?;
        if Self::looks_signed_out(&text) {
            return Err(ProviderError::AuthRequired);
        }
        Ok(text)
    }

    fn parse_usage_text(text: &str) -> Result<UsageSnapshot, ProviderError> {
        let now = Utc::now();

        // Do not use short aliases like "rolling" / "weekly": they false-match
        // unrelated `percent: 100` blobs earlier in the HTML payload.
        let rolling = Self::extract_window(text, &["rollingUsage", "rolling_usage"])
            .ok_or_else(|| ProviderError::Parse("Missing rolling usage window".to_string()))?;
        let weekly = Self::extract_window(text, &["weeklyUsage", "weekly_usage"]);
        let monthly = Self::extract_window(text, &["monthlyUsage", "monthly_usage"]);

        let primary = RateWindow::with_details(
            rolling.0,
            Some(300),
            Some(now + chrono::Duration::seconds(rolling.1)),
            None,
        );
        let mut snap = UsageSnapshot::new(primary).with_login_method("OpenCode Go");

        if let Some((pct, reset)) = weekly {
            snap = snap.with_secondary(RateWindow::with_details(
                pct,
                Some(10080),
                Some(now + chrono::Duration::seconds(reset)),
                None,
            ));
        }

        if let Some((pct, reset)) = monthly {
            snap = snap.with_tertiary(RateWindow::with_details(
                pct,
                Some(43200),
                Some(now + chrono::Duration::seconds(reset)),
                None,
            ));
        }

        if let Some(renews_at) = Self::extract_renewal(text) {
            snap = snap.with_extra_rate_window(
                "renewal",
                "Renews",
                RateWindow::with_details(0.0, None, Some(renews_at), None),
            );
        }

        Ok(snap)
    }

    /// Extract `(used_percent, resetInSec)` for a usage block by name.
    ///
    /// OpenCode Go's console (`analyzeRollingUsage`) emits
    /// `{ usagePercent, resetInSec, status }` where `usagePercent` is **used**
    /// (progress bar width on opencode.ai). The page HTML often embeds more
    /// than one object with the same name (SSR flight data + UI props + stale
    /// rate-limited shells). Taking the first regex hit is what made a real
    /// ~1% used window render as 100% used / 0% remaining / 已用尽.
    fn extract_window(text: &str, names: &[&str]) -> Option<(f64, i64)> {
        let mut candidates: Vec<WindowCandidate> = Vec::new();
        for name in names {
            candidates.extend(Self::collect_window_candidates(text, name));
        }
        Self::pick_window_candidate(candidates)
    }

    fn collect_window_candidates(text: &str, block: &str) -> Vec<WindowCandidate> {
        let mut out = Vec::new();
        let mut search_from = 0;
        while search_from < text.len() {
            let Some(rel) = text[search_from..].find(block) else {
                break;
            };
            let abs = search_from + rel;
            let after_name = abs + block.len();
            // Skip matches inside longer identifiers (e.g. foo_rollingUsage).
            if abs > 0 {
                let prev = text.as_bytes()[abs - 1];
                if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'"' {
                    // Allow a leading quote: "rollingUsage"
                    if prev != b'"' {
                        search_from = after_name;
                        continue;
                    }
                }
            }
            // Skip trailing identifier chars: rollingUsageX
            if after_name < text.len() {
                let next = text.as_bytes()[after_name];
                if next.is_ascii_alphanumeric() || next == b'_' {
                    search_from = after_name;
                    continue;
                }
            }
            let Some(brace_rel) = text[after_name..].find('{') else {
                search_from = after_name;
                continue;
            };
            // Object must start soon after the key (not hundreds of chars later).
            let between = text[after_name..after_name + brace_rel].trim();
            let ok_sep = between.is_empty()
                || between == ":"
                || between == "="
                || between.starts_with(':')
                || between.starts_with('=')
                || between == "\":"
                || between.ends_with(':')
                || between.ends_with('=');
            if !ok_sep || brace_rel > 24 {
                search_from = after_name;
                continue;
            }
            let from_brace = &text[after_name + brace_rel..];
            if let Some(obj) = Self::slice_following_object(from_brace) {
                if let Some(c) = Self::parse_window_object(obj) {
                    out.push(c);
                }
                search_from = after_name + brace_rel + obj.len();
            } else {
                search_from = after_name + brace_rel + 1;
            }
        }

        if out.is_empty() {
            if let Some(c) = Self::legacy_single_candidate(text, block) {
                out.push(c);
            }
        }
        out
    }

    fn legacy_single_candidate(text: &str, block: &str) -> Option<WindowCandidate> {
        const USED_KEYS: &[&str] = &[
            "usagePercent",
            "usedPercent",
            "percentUsed",
            "usage_percent",
            "used_percent",
        ];
        const REMAINING_KEYS: &[&str] = &[
            "remainingPercent",
            "remaining_percent",
            "percentRemaining",
            "percent_remaining",
        ];
        const RESET_KEYS: &[&str] = &[
            "resetInSec",
            "resetInSeconds",
            "resetSeconds",
            "resetSec",
            "reset_in_sec",
            "reset_sec",
        ];

        let reset = RESET_KEYS
            .iter()
            .find_map(|key| Self::extract_block_number(text, block, key))
            .map(|n| n as i64)
            .unwrap_or(0)
            .max(0);
        if let Some(raw) = USED_KEYS
            .iter()
            .find_map(|key| Self::extract_block_number(text, block, key))
        {
            return Some(WindowCandidate {
                used_percent: Self::normalize_percent(raw),
                reset_in_sec: reset,
                status_ok: true,
                has_usage_key: true,
            });
        }
        if let Some(raw) = REMAINING_KEYS
            .iter()
            .find_map(|key| Self::extract_block_number(text, block, key))
        {
            let remaining = Self::normalize_percent(raw);
            return Some(WindowCandidate {
                used_percent: (100.0 - remaining).clamp(0.0, 100.0),
                reset_in_sec: reset,
                status_ok: true,
                has_usage_key: false,
            });
        }
        None
    }

    fn pick_window_candidate(mut candidates: Vec<WindowCandidate>) -> Option<(f64, i64)> {
        if candidates.is_empty() {
            return None;
        }
        // Prefer real usage objects over rate-limited/template shells:
        // 1) has usagePercent/used key
        // 2) status ok (not rate-limited)
        // 3) lower used percent (a 1% live window beats a 100% shell)
        // 4) later occurrence (stable tie-break)
        candidates.sort_by(|a, b| {
            b.has_usage_key
                .cmp(&a.has_usage_key)
                .then(b.status_ok.cmp(&a.status_ok))
                .then(
                    a.used_percent
                        .partial_cmp(&b.used_percent)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
        });
        let best = candidates.into_iter().next()?;
        tracing::info!(
            used = best.used_percent,
            reset = best.reset_in_sec,
            status_ok = best.status_ok,
            "opencodego: picked usage window"
        );
        Some((best.used_percent, best.reset_in_sec))
    }

    /// Brace-balanced `{...}` starting at the first `{` in `text`.
    fn slice_following_object(text: &str) -> Option<&str> {
        let start = text.find('{')?;
        let bytes = text.as_bytes();
        let mut depth = 0i32;
        let mut in_str = false;
        let mut escape = false;
        for (i, &b) in bytes.iter().enumerate().skip(start) {
            if in_str {
                if escape {
                    escape = false;
                } else if b == b'\\' {
                    escape = true;
                } else if b == b'"' {
                    in_str = false;
                }
                continue;
            }
            match b {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(&text[start..=i]);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn parse_window_object(obj: &str) -> Option<WindowCandidate> {
        // Accept both JSON and loose JS object literals.
        let used = Self::object_number(
            obj,
            &[
                "usagePercent",
                "usedPercent",
                "percentUsed",
                "usage_percent",
                "used_percent",
            ],
        );
        let remaining = Self::object_number(
            obj,
            &[
                "remainingPercent",
                "remaining_percent",
                "percentRemaining",
                "percent_remaining",
            ],
        );
        let reset = Self::object_number(
            obj,
            &[
                "resetInSec",
                "resetInSeconds",
                "resetSeconds",
                "resetSec",
                "reset_in_sec",
                "reset_sec",
            ],
        )
        .map(|n| n as i64)
        .unwrap_or(0)
        .max(0);

        let status_ok = !obj.to_ascii_lowercase().contains("rate-limited");

        let (used_percent, has_usage_key) = if let Some(raw) = used {
            (Self::normalize_percent(raw), true)
        } else if let Some(raw) = remaining {
            let rem = Self::normalize_percent(raw);
            ((100.0 - rem).clamp(0.0, 100.0), false)
        } else if let Some(ratio) = Self::object_used_limit_ratio(obj) {
            (ratio.clamp(0.0, 100.0), true)
        } else {
            return None;
        };

        Some(WindowCandidate {
            used_percent,
            reset_in_sec: reset,
            status_ok,
            has_usage_key,
        })
    }

    fn object_number(obj: &str, keys: &[&str]) -> Option<f64> {
        for key in keys {
            // Key as whole word: no leading letter/underscore so we never match
            // inside remaining_percent when looking for a bare percent key.
            let pattern =
                format!(r#"(?i)(?:^|[{{,\s])"?{key}"?\s*[:=]\s*"?([0-9]+(?:\.[0-9]+)?)"?"#);
            if let Some(v) = Self::extract_number(&pattern, obj) {
                return Some(v);
            }
        }
        None
    }

    fn object_used_limit_ratio(obj: &str) -> Option<f64> {
        let used = Self::object_number(obj, &["used", "usage"])?;
        let limit = Self::object_number(obj, &["limit", "total"])?;
        if limit <= 0.0 {
            return None;
        }
        // Microcent-scale values are huge; ratio still works.
        Some((used / limit) * 100.0)
    }

    /// Normalize a scraped percent into 0..=100 **used**.
    ///
    /// OpenCode's console emits `usagePercent` as a percentage that is already
    /// scaled 0–100 (e.g. `0.1` = 0.1% used, `1` = 1%, `42.5` = 42.5%).
    /// The Go plan uses sub-integer precision (quotas precise to 0.1%), so a
    /// heuristic that treated `< 1.0` as a 0–1 fraction would wrongly scale
    /// `0.1` into `10%` and `0.5` into `50%`. Just clamp.
    fn normalize_percent(raw: f64) -> f64 {
        raw.clamp(0.0, 100.0)
    }

    /// Number for `blockName ... key: value` inside one object-ish span.
    fn extract_block_number(text: &str, block: &str, key: &str) -> Option<f64> {
        let pattern = format!(
            r#"{block}[^}}]{{0,400}}?(?:^|[{{,\s])"?{key}"?\s*[:=]\s*"?([0-9]+(?:\.[0-9]+)?)"?"#
        );
        Self::extract_number(&pattern, text)
    }

    fn extract_number(pattern: &str, text: &str) -> Option<f64> {
        let re = regex_lite::Regex::new(pattern).ok()?;
        re.captures(text)?.get(1)?.as_str().parse().ok()
    }

    fn extract_renewal(text: &str) -> Option<DateTime<Utc>> {
        let re = regex_lite::Regex::new(
            r#"(?:"renewAt"|"renew_at"|renewAt|renew_at)\s*[:=]\s*"?([^",}\s]+)"?"#,
        )
        .ok()?;
        let raw = re.captures(text)?.get(1)?.as_str();
        Self::date_from_text(raw)
    }

    fn date_from_text(raw: &str) -> Option<DateTime<Utc>> {
        let text = raw.trim();
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

    fn parse_workspace_ids(text: &str) -> Vec<String> {
        let pattern = r#"(wrk_[A-Za-z0-9_-]+)"#;
        let re = match regex_lite::Regex::new(pattern) {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        let mut seen = Vec::new();
        for caps in re.captures_iter(text) {
            if let Some(m) = caps.get(1) {
                let s = m.as_str().to_string();
                if !seen.contains(&s) {
                    seen.push(s);
                }
            }
        }
        seen
    }

    fn looks_signed_out(text: &str) -> bool {
        let lower = text.to_lowercase();
        lower.contains("auth/authorize")
            || lower.contains("\"signin\"")
            || lower.contains("please sign in")
    }

    fn parse_zen_balance(text: &str) -> Option<f64> {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text)
            && let Some(value) = Self::find_balance_value(&json)
        {
            return Some(value);
        }
        let patterns = [
            r#"(?i)(?:current\s+balance|zen\s+balance|現在の残高)[^$]{0,80}\$\s*([0-9][0-9,]*(?:\.[0-9]+)?)"#,
            r#"(?i)(?:balance|残高)[\s\S]{0,120}?\$\s*([0-9][0-9,]*(?:\.[0-9]+)?)"#,
        ];
        patterns.iter().find_map(|pattern| {
            let re = regex_lite::Regex::new(pattern).ok()?;
            let raw = re.captures(text)?.get(1)?.as_str().replace(',', "");
            raw.parse::<f64>().ok()
        })
    }

    fn find_balance_value(value: &serde_json::Value) -> Option<f64> {
        match value {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    let normalized: String = key
                        .to_lowercase()
                        .chars()
                        .filter(|c| c.is_ascii_alphanumeric())
                        .collect();
                    if matches!(
                        normalized.as_str(),
                        "zenbalance"
                            | "zencurrentbalance"
                            | "currentbalance"
                            | "currentbalanceusd"
                            | "balanceusd"
                            | "usdbalance"
                    ) {
                        if let Some(number) = value.as_f64() {
                            return Some(number);
                        }
                        if let Some(text) = value.as_str()
                            && let Ok(number) = text.trim().replace(',', "").parse()
                        {
                            return Some(number);
                        }
                    }
                    if let Some(found) = Self::find_balance_value(value) {
                        return Some(found);
                    }
                }
                None
            }
            serde_json::Value::Array(items) => items.iter().find_map(Self::find_balance_value),
            _ => None,
        }
    }

    async fn fetch_with_cookies(
        &self,
        ctx: &FetchContext,
        cookie_header: &str,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let workspace_id = match Self::workspace_id_from_context(ctx.workspace_id.as_deref()) {
            Some(workspace_id) => workspace_id.to_string(),
            None => self.fetch_workspace_id(cookie_header).await?,
        };
        let balance_task = ctx.include_credits.then(|| {
            self.spawn_zen_balance_task(cookie_header, Some(&workspace_id), ctx.web_timeout)
        });
        let page = match self.fetch_usage_page(&workspace_id, cookie_header).await {
            Ok(page) => page,
            Err(error) => {
                if let Some((task, _)) = balance_task {
                    task.abort();
                }
                return Err(error);
            }
        };
        let usage = match Self::parse_usage_text(&page) {
            Ok(usage) => usage,
            Err(error) => {
                if let Some((task, _)) = balance_task {
                    task.abort();
                }
                return Err(error);
            }
        };
        let balance = match Self::parse_zen_balance(&page) {
            Some(balance) => {
                if let Some((task, _)) = balance_task {
                    task.abort();
                }
                Some(balance)
            }
            None => match balance_task {
                Some((task, started_at)) => {
                    Self::join_zen_balance(
                        task,
                        started_at,
                        ctx.requires_optional_usage_completeness,
                    )
                    .await
                }
                None => None,
            },
        };
        Ok(Self::with_zen_balance(usage, "web", balance))
    }

    async fn fetch_page_text(
        client: &Client,
        url: &str,
        cookie_header: &str,
        timeout: Option<Duration>,
        label: &str,
    ) -> Result<String, ProviderError> {
        let mut request = client
            .get(url)
            .header("Cookie", cookie_header)
            .header("User-Agent", USER_AGENT)
            .header("Origin", BASE_URL)
            .header("Referer", BASE_URL)
            .header(
                "Accept",
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            );
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await?;
        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(ProviderError::AuthRequired);
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "OpenCode Go {label} returned {status}"
            )));
        }
        let text = response.text().await?;
        if Self::looks_signed_out(&text) {
            return Err(ProviderError::AuthRequired);
        }
        Ok(text)
    }

    async fn fetch_server_text(
        client: &Client,
        server_id: &str,
        args: Option<&str>,
        referer: &str,
        cookie_header: &str,
        timeout: Option<Duration>,
    ) -> Result<String, ProviderError> {
        let mut url = reqwest::Url::parse(SERVER_URL)
            .map_err(|e| ProviderError::Parse(format!("Invalid OpenCode server URL: {e}")))?;
        url.query_pairs_mut().append_pair("id", server_id);
        if let Some(args) = args {
            url.query_pairs_mut().append_pair("args", args);
        }
        let mut request = client
            .get(url)
            .header("Cookie", cookie_header)
            .header("X-Server-Id", server_id)
            .header("X-Server-Instance", format!("server-fn:{}", Uuid::new_v4()))
            .header("User-Agent", USER_AGENT)
            .header("Origin", BASE_URL)
            .header("Referer", referer)
            .header(
                "Accept",
                "text/javascript, application/json;q=0.9, */*;q=0.8",
            );
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await?;
        let status = response.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(ProviderError::AuthRequired);
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "OpenCode Go server returned {status}"
            )));
        }
        let text = response.text().await?;
        if Self::looks_signed_out(&text) {
            return Err(ProviderError::AuthRequired);
        }
        Ok(text)
    }

    async fn fetch_zen_balance(
        client: &Client,
        workspace_id: &str,
        cookie_header: &str,
        timeout: Duration,
    ) -> Option<f64> {
        let request_timeout = timeout.min(ZEN_BALANCE_TIMEOUT);
        let referer = format!("{BASE_URL}/workspace/{workspace_id}/billing");
        let page = Self::fetch_page_text(
            client,
            &referer,
            cookie_header,
            Some(request_timeout),
            "Zen dashboard page",
        )
        .await
        .ok()?;
        if let Some(balance) = Self::parse_zen_balance(&page) {
            return Some(balance);
        }

        let args = serde_json::json!([workspace_id]).to_string();
        let billing = Self::fetch_server_text(
            client,
            BILLING_SERVER_ID,
            Some(&args),
            &referer,
            cookie_header,
            Some(request_timeout),
        )
        .await
        .ok()?;
        parse_billing_server_balance(&billing)
    }

    fn spawn_zen_balance_task(
        &self,
        cookie_header: &str,
        workspace_id_override: Option<&str>,
        web_timeout: u64,
    ) -> (tokio::task::JoinHandle<Option<f64>>, std::time::Instant) {
        let client = self.client.clone();
        let cookie_header = cookie_header.to_string();
        let workspace_id_override = workspace_id_override.map(str::to_string);
        let timeout = Duration::from_secs(web_timeout.max(1));
        let started_at = std::time::Instant::now();
        let task = tokio::spawn(async move {
            tokio::time::sleep(ZEN_BALANCE_START_DELAY).await;
            let workspace_id = match workspace_id_override {
                Some(id) => id,
                None => Self::fetch_workspace_id_with_client(&client, &cookie_header)
                    .await
                    .ok()?,
            };
            Self::fetch_zen_balance(&client, &workspace_id, &cookie_header, timeout).await
        });
        (task, started_at)
    }

    async fn join_zen_balance(
        mut task: tokio::task::JoinHandle<Option<f64>>,
        started_at: std::time::Instant,
        requires_optional_usage_completeness: bool,
    ) -> Option<f64> {
        let budget = zen_balance_join_budget(started_at, requires_optional_usage_completeness);
        match tokio::time::timeout(budget, &mut task).await {
            Ok(Ok(balance)) => balance,
            Ok(Err(_)) | Err(_) => {
                task.abort();
                None
            }
        }
    }

    fn with_zen_balance(
        mut usage: UsageSnapshot,
        source: &str,
        balance: Option<f64>,
    ) -> ProviderFetchResult {
        let Some(balance) = balance else {
            return ProviderFetchResult::new(usage, source);
        };
        usage = usage.with_extra_rate_window(
            "zen-balance",
            "Zen balance",
            RateWindow::with_details(0.0, None, None, Some(format!("${balance:.2}"))),
        );
        ProviderFetchResult::new(usage, source).with_cost(CostSnapshot::new(
            balance,
            "USD",
            "Zen balance",
        ))
    }

    async fn fetch_local_with_balance(
        &self,
        ctx: &FetchContext,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let snap = local::fetch_local_usage(Utc::now())?;
        let mut result = snap.to_fetch_result();
        if let Some(api_key) = usage_api::resolve_api_key(ctx)
            && let Ok(api_result) = usage_api::fetch(&self.client, ctx, &api_key, "local+api").await
        {
            result = api_result;
        }
        Ok(self.include_zen_balance(ctx, result).await)
    }

    async fn fetch_local_estimate_with_balance(
        &self,
        ctx: &FetchContext,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let snap = local::fetch_local_usage(Utc::now())?;
        Ok(self.include_zen_balance(ctx, snap.to_fetch_result()).await)
    }

    async fn include_zen_balance(
        &self,
        ctx: &FetchContext,
        mut result: ProviderFetchResult,
    ) -> ProviderFetchResult {
        if !ctx.include_credits {
            return result;
        }
        let cookie_header = match ctx.manual_cookie_header.clone() {
            Some(header) => Some(header),
            None if ctx.allow_browser_cookie => {
                crate::providers::browser_cookie_header(&["opencode.ai"]).ok()
            }
            None => None,
        };
        let Some(cookie_header) = cookie_header else {
            return result;
        };
        let (task, started_at) = self.spawn_zen_balance_task(
            &cookie_header,
            ctx.workspace_id.as_deref(),
            ctx.web_timeout,
        );
        if let Some(balance) =
            Self::join_zen_balance(task, started_at, ctx.requires_optional_usage_completeness).await
        {
            let source = result.source_label.clone();
            result = Self::with_zen_balance(result.usage, &source, Some(balance));
        }
        result
    }

    fn auto_prefers_web_first(ctx: &FetchContext) -> bool {
        ctx.workspace_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty())
            || ctx.auto_prefer_web
    }

    fn web_error_allows_local_fallback(ctx: &FetchContext, error: &ProviderError) -> bool {
        if matches!(
            error,
            ProviderError::AuthRequired | ProviderError::NoCookies
        ) && (ctx.manual_cookie_header.is_some() || ctx.auto_prefer_web)
        {
            return false;
        }
        matches!(
            error,
            ProviderError::AuthRequired
                | ProviderError::NoCookies
                | ProviderError::Timeout
                | ProviderError::Network(_)
                | ProviderError::Parse(_)
                | ProviderError::Other(_)
        )
    }

    async fn fetch_web(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        if let Some(cookie_header) = &ctx.manual_cookie_header {
            return self.fetch_with_cookies(ctx, cookie_header).await;
        }

        if !ctx.allow_browser_cookie {
            return Err(ProviderError::AuthRequired);
        }

        match crate::providers::browser_cookie_header(&["opencode.ai"]) {
            Ok(cookie_header) => self.fetch_with_cookies(ctx, &cookie_header).await,
            Err(ProviderError::NoCookies) => Err(ProviderError::AuthRequired),
            Err(error) => Err(error),
        }
    }
}

fn zen_balance_join_budget(
    started_at: std::time::Instant,
    requires_optional_usage_completeness: bool,
) -> Duration {
    if !requires_optional_usage_completeness {
        return ZEN_BALANCE_JOIN_GRACE;
    }
    ZEN_BALANCE_TIMEOUT.saturating_sub(started_at.elapsed())
}

/// The billing server function reports raw 1e-8 USD units and only becomes a
/// balance when the response carries a non-empty customer marker.
fn parse_billing_server_balance(text: &str) -> Option<f64> {
    const BILLING_SCALE: f64 = 100_000_000.0;
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(raw) = find_raw_billing_balance(&json)
    {
        return Some(raw / BILLING_SCALE);
    }
    let customer_re = regex_lite::Regex::new(
        r#"(?:"customerID"|customerID)\s*:\s*(?:\$R\[\d+\]\s*=\s*)?"[^"]+""#,
    )
    .ok()?;
    customer_re.find(text)?;
    let balance_re = regex_lite::Regex::new(
        r#"(?:"balance"|balance)\s*:\s*(?:\$R\[\d+\]\s*=\s*)?(-?[0-9]+(?:\.[0-9]+)?)"#,
    )
    .ok()?;
    let raw: f64 = balance_re
        .captures(text)?
        .get(1)?
        .as_str()
        .replace(',', "")
        .parse()
        .ok()?;
    Some(raw / BILLING_SCALE)
}

fn find_raw_billing_balance(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Object(map) => {
            if let Some(balance) = map.get("balance") {
                let customer_ok = map
                    .get("customerID")
                    .and_then(|value| value.as_str())
                    .is_some_and(|id| !id.is_empty());
                if !customer_ok {
                    return None;
                }
                return billing_numeric_value(balance);
            }
            map.values().find_map(find_raw_billing_balance)
        }
        serde_json::Value::Array(items) => items.iter().find_map(find_raw_billing_balance),
        _ => None,
    }
}

fn billing_numeric_value(value: &serde_json::Value) -> Option<f64> {
    match value {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.trim().replace(',', "").parse().ok(),
        _ => None,
    }
}

impl Default for OpenCodeGoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for OpenCodeGoProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenCodeGo
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        tracing::debug!("Fetching OpenCode Go usage");

        match ctx.source_mode {
            SourceMode::Auto => {
                let mut api_error = None;
                if let Some(api_key) = usage_api::resolve_api_key(ctx) {
                    match usage_api::fetch(&self.client, ctx, &api_key, "api").await {
                        Ok(result) => {
                            return Ok(self.include_zen_balance(ctx, result).await);
                        }
                        Err(error) => {
                            if usage_api::is_entitlement_error(&error) {
                                return Err(error);
                            }
                            tracing::debug!(
                                "OpenCode Go usage API failed; trying web/local fallback: {error}"
                            );
                            api_error = Some(error);
                        }
                    }
                }

                // In Auto, a resolved API key always gets the first attempt:
                // a saved web cookie/workspace is only a fallback preference.
                // This avoids a stale or structurally changed console page
                // preventing a working authoritative Go usage API from being
                // queried. When the API is unavailable, match Token Monitor's
                // API -> web -> local-estimate order.
                let web_first = api_error.is_some() || Self::auto_prefers_web_first(ctx);
                let mut web_error = None;
                if web_first {
                    match self.fetch_web(ctx).await {
                        Ok(result) => return Ok(result),
                        Err(error) if Self::web_error_allows_local_fallback(ctx, &error) => {
                            tracing::debug!(
                                "OpenCode Go fallback web fetch failed; trying local estimate: {error}"
                            );
                            web_error = Some(error);
                        }
                        Err(error) => return Err(api_error.unwrap_or(error)),
                    }
                }

                match self.fetch_local_estimate_with_balance(ctx).await {
                    Ok(result) => return Ok(result),
                    Err(error) => {
                        tracing::debug!("OpenCode Go local estimate failed: {error}");
                    }
                }

                if !web_first {
                    match self.fetch_web(ctx).await {
                        Ok(result) => return Ok(result),
                        Err(error) => return Err(api_error.or(web_error).unwrap_or(error)),
                    }
                }

                Err(api_error
                    .or(web_error)
                    .unwrap_or(ProviderError::AuthRequired))
            }
            SourceMode::Web => self.fetch_web(ctx).await,
            SourceMode::Cli => self.fetch_local_with_balance(ctx).await,
            SourceMode::OAuth => {
                let api_key = usage_api::resolve_api_key(ctx).ok_or_else(|| {
                    ProviderError::NotInstalled(
                        "Missing OpenCode Go API key. Add one in Settings or set OPENCODE_API_KEY."
                            .to_string(),
                    )
                })?;
                usage_api::fetch(&self.client, ctx, &api_key, "api").await
            }
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
    use super::*;

    #[test]
    fn parses_workspace_ids() {
        let text = r#"{ id: "wrk_abc123", name: "x" } { id: "wrk_def456" }"#;
        let ids = OpenCodeGoProvider::parse_workspace_ids(text);
        assert_eq!(
            ids,
            vec!["wrk_abc123".to_string(), "wrk_def456".to_string()]
        );
    }

    #[test]
    fn uses_context_workspace_id_before_discovery() {
        assert_eq!(
            OpenCodeGoProvider::workspace_id_from_context(Some("wrk_override")),
            Some("wrk_override")
        );
        assert_eq!(
            OpenCodeGoProvider::workspace_id_from_context(Some("")),
            None
        );
    }

    #[test]
    fn parses_usage_blocks() {
        let text = r#"
            rollingUsage: { usagePercent: 42.5, resetInSec: 3600 }
            weeklyUsage: { usagePercent: 0.13, resetInSec: 86400 }
            monthlyUsage: { usagePercent: 7, resetInSec: 2592000 }
        "#;
        let snap = OpenCodeGoProvider::parse_usage_text(text).unwrap();
        assert!((snap.primary.used_percent - 42.5).abs() < 0.001);
        let secondary = snap.secondary.expect("weekly");
        // 0.13 is already a percentage (Go plan sub-integer precision), not a fraction.
        assert!((secondary.used_percent - 0.13).abs() < 0.001);
        let tertiary = snap.tertiary.expect("monthly");
        assert!((tertiary.used_percent - 7.0).abs() < 0.001);
    }

    #[test]
    fn remaining_percent_is_not_treated_as_used() {
        // Regression: bare `percent` matched inside `remaining_percent`, so a
        // nearly-full remaining window looked exhausted in the settings UI.
        let text = r#"
            rollingUsage: { remaining_percent: 99, resetInSec: 3600 }
            weeklyUsage: { remainingPercent: 100, resetInSec: 86400 }
            monthlyUsage: { remaining_percent: 0.5, resetInSec: 2592000 }
        "#;
        let snap = OpenCodeGoProvider::parse_usage_text(text).unwrap();
        // 99% remaining → 1% used
        assert!((snap.primary.used_percent - 1.0).abs() < 0.001);
        assert!(!snap.primary.is_exhausted());
        // 100% remaining → 0% used
        let weekly = snap.secondary.expect("weekly");
        assert!(weekly.used_percent.abs() < 0.001);
        // remaining_percent: 0.5 is already a percentage → 0.5% remaining → 99.5% used
        let monthly = snap.tertiary.expect("monthly");
        assert!((monthly.used_percent - 99.5).abs() < 0.001);
    }

    #[test]
    fn one_percent_used_is_not_scaled_to_one_hundred() {
        // OpenCode emits Math.floor percents: 1 means one percent, not 100%.
        let text = r#"
            rollingUsage: { usagePercent: 1, resetInSec: 4907, status: "ok" }
            weeklyUsage: { usagePercent: 0, resetInSec: 86400, status: "ok" }
        "#;
        let snap = OpenCodeGoProvider::parse_usage_text(text).unwrap();
        assert!(
            (snap.primary.used_percent - 1.0).abs() < 0.001,
            "got {}",
            snap.primary.used_percent
        );
        assert!(!snap.primary.is_exhausted());
        assert!((snap.primary.remaining_percent() - 99.0).abs() < 0.001);
    }

    #[test]
    fn prefers_usage_percent_when_both_present() {
        let text = r#"
            rollingUsage: { remaining_percent: 99, usagePercent: 1.5, resetInSec: 120 }
        "#;
        let snap = OpenCodeGoProvider::parse_usage_text(text).unwrap();
        assert!((snap.primary.used_percent - 1.5).abs() < 0.001);
    }

    #[test]
    fn prefers_live_ok_window_over_rate_limited_shell() {
        // SSR pages often embed a rate-limited template (100%) before the live
        // subscription object (~1% used). First-match regexes pick the shell.
        let text = r#"
            rollingUsage: { usagePercent: 100, resetInSec: 999, status: "rate-limited" }
            weeklyUsage: { usagePercent: 0, resetInSec: 86400, status: "ok" }
            rollingUsage: { usagePercent: 1, resetInSec: 4907, status: "ok" }
            monthlyUsage: { usagePercent: 0, resetInSec: 2592000, status: "ok" }
        "#;
        let cands = OpenCodeGoProvider::collect_window_candidates(text, "rollingUsage");
        assert!(
            cands.len() >= 2,
            "expected both rollingUsage objects, got {cands:?}"
        );
        let snap = OpenCodeGoProvider::parse_usage_text(text).unwrap();
        assert!(
            (snap.primary.used_percent - 1.0).abs() < 0.001,
            "got {} from candidates {cands:?}",
            snap.primary.used_percent
        );
        assert!(!snap.primary.is_exhausted());
        assert!((snap.secondary.as_ref().unwrap().used_percent).abs() < 0.001);
    }

    #[test]
    fn parses_renewal_window() {
        let text = r#"
            rollingUsage: { usagePercent: 42.5, resetInSec: 3600 }
            weeklyUsage: { usagePercent: 50, resetInSec: 86400 }
            renewAt: "2026-06-01T12:00:00Z"
        "#;
        let snap = OpenCodeGoProvider::parse_usage_text(text).unwrap();
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
    fn billing_balance_requires_customer_marker_and_scales_raw_units() {
        assert_eq!(
            parse_billing_server_balance(r#"{"balance":3373000000,"customerID":"cus_go"}"#),
            Some(33.73)
        );
        assert_eq!(
            parse_billing_server_balance(r#"{"balance":3373000000}"#),
            None
        );
        assert_eq!(
            parse_billing_server_balance(r#"{"balance":true,"customerID":"cus_go"}"#),
            None
        );
    }

    #[test]
    fn billing_balance_parses_rsc_stream_fallback() {
        let text = r#"1:{"customerID":"cus_go","balance":1500000000}"#;
        assert_eq!(parse_billing_server_balance(text), Some(15.0));
    }
}
