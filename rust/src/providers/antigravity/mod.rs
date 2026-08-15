//! Antigravity provider implementation
//!
//! Fetches usage data from Antigravity's local language server probe
//! Uses Windows process detection to find CSRF token

use async_trait::async_trait;
use regex_lite::Regex;
use serde::Deserialize;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::OnceLock;

use crate::core::{
    FetchContext, NamedRateWindow, Provider, ProviderError, ProviderFetchResult, ProviderId,
    ProviderMetadata, RateWindow, SourceMode, UsageSnapshot,
};

const NOT_RUNNING_MESSAGE: &str =
    "Antigravity language server not running. Start Google Antigravity and sign in, then retry.";

/// Antigravity provider
pub struct AntigravityProvider {
    metadata: ProviderMetadata,
}

/// Return a regex that matches `--<flag> <value>` or `--<flag>=<value>`.
fn flag_re(flag: &str) -> Regex {
    Regex::new(&format!(r"--{f}(?:\s+|\s*=\s*)(\S+)", f = flag)).expect("valid flag pattern")
}

impl AntigravityProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::Antigravity,
                display_name: "Antigravity",
                // Antigravity 2.x groups quota by model family, not by a single
                // model: the Gemini pool and the Claude/GPT pool. The labels
                // follow the official Models settings screen (Gemini Models /
                // Claude and GPT models) instead of single-model names.
                session_label: "Gemini",
                weekly_label: "Claude/GPT",
                supports_opus: true,
                supports_credits: false,
                default_enabled: false,
                is_primary: false,
                dashboard_url: None,
                status_page_url: None,
            },
        }
    }

    /// Detect running Antigravity language server and extract connection info
    fn detect_process_info() -> Result<ProcessInfo, ProviderError> {
        // Use PowerShell to get process command lines
        #[cfg(windows)]
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = Command::new("powershell.exe");
        cmd.args([
                "-ExecutionPolicy", "Bypass",
                "-Command",
                "Get-CimInstance Win32_Process | Where-Object { $_.Name -like '*language_server_windows*' -or $_.Name -like 'language_server.exe' } | ForEach-Object { \"$($_.ProcessId)`t$($_.CommandLine)\" }"
            ]);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let output = cmd
            .output()
            .map_err(|e| ProviderError::Other(format!("Failed to run PowerShell: {}", e)))?;

        if !output.status.success() {
            return Err(ProviderError::NotInstalled(
                "Failed to detect Antigravity process".to_string(),
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        Self::parse_process_info(&stdout)
            .ok_or_else(|| ProviderError::NotInstalled(NOT_RUNNING_MESSAGE.to_string()))
    }

    fn parse_process_info(stdout: &str) -> Option<ProcessInfo> {
        // Shared argument parser: handles `--flag value` and `--flag=value` forms
        let csrf_re = flag_re("csrf_token");
        let ext_csrf_re = flag_re("extension_server_csrf_token");
        let port_re = flag_re("extension_server_port");
        let https_port_re = flag_re("https_server_port");

        for line in stdout.lines() {
            if line.contains("--csrf_token") {
                // Line is "<pid>\t<command line>"; split off the PID prefix we added so the
                // PID can be used to enumerate the process's real listening ports below.
                let (pid, line) = match line.split_once('\t') {
                    Some((p, rest)) => (p.trim().parse::<u32>().ok(), rest),
                    None => (None, line),
                };

                let csrf_token = csrf_re
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string());

                let ext_csrf_token = ext_csrf_re
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .map(|m| m.as_str().to_string());

                let port = port_re
                    .captures(line)
                    .and_then(|c| c.get(1))
                    .and_then(|m| m.as_str().parse::<u16>().ok())
                    .or_else(|| {
                        https_port_re
                            .captures(line)
                            .and_then(|c| c.get(1))
                            .and_then(|m| m.as_str().parse::<u16>().ok())
                    });

                if let Some(token) = csrf_token {
                    return Some(ProcessInfo {
                        csrf_token: token,
                        extension_server_csrf_token: ext_csrf_token,
                        extension_port: port,
                        pid,
                    });
                }
            }
        }

        None
    }

    /// Find the actual API port by probing the language server's candidate ports.
    async fn find_api_port(
        extension_port: Option<u16>,
        pid: Option<u32>,
    ) -> Result<u16, ProviderError> {
        // The language server binds a RANDOM localhost port at startup; --extension_server_port
        // is only a reference point (and belongs to a separate HTTP extension server), so the
        // real gRPC/Connect API port is not guaranteed to be within a small window above it.
        // Mirror the macOS/Linux probe (which uses `lsof`) by enumerating the language-server
        // process's own listening ports first, then fall back to a heuristic window above the
        // extension port and a few historically-seen ports.
        //
        // SECURITY: TLS verification is disabled because the local language server uses a
        // self-signed certificate. This is scoped to 127.0.0.1 only; we confirm a port by
        // checking that it answers the expected gRPC endpoint.
        let client = crate::core::credentialed_http_client_builder()
            .timeout(std::time::Duration::from_secs(2))
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        // Ordered candidate ports: the process's real listening ports first (Windows
        // equivalent of `lsof`), then the heuristic window above the extension port, then a
        // few known ports as a last resort.
        let mut candidates: Vec<u16> = Vec::new();
        if let Some(pid) = pid {
            candidates.extend(Self::listening_ports_for_pid(pid));
        }
        if let Some(ep) = extension_port.filter(|&p| p > 0) {
            candidates.extend((0..20u16).map(|offset| ep.saturating_add(offset)));
        }
        candidates.extend([53835, 53836, 53837, 53838, 53845, 53849]);

        let mut probed: Vec<u16> = Vec::new();
        for port in candidates {
            if probed.contains(&port) {
                continue; // probe each port at most once
            }
            probed.push(port);
            if Self::probe_api_port(&client, port).await {
                return Ok(port);
            }
        }

        Err(ProviderError::Other(
            "Could not find Antigravity API port".to_string(),
        ))
    }

    /// Probe a single candidate port. Returns true if it answers the language server's
    /// gRPC endpoint (HTTP 200 or 401).
    async fn probe_api_port(client: &reqwest::Client, port: u16) -> bool {
        let url = format!(
            "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUnleashData",
            port
        );
        match client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .body("{}")
            .send()
            .await
        {
            Ok(resp) => {
                let code = resp.status().as_u16();
                code == 200 || code == 401
            }
            Err(_) => false,
        }
    }

    /// Enumerate the TCP ports a given PID is listening on (Windows `lsof` equivalent).
    /// On Windows this uses `Get-NetTCPConnection`; it returns an empty list on any failure
    /// so the caller deterministically falls back to the heuristic candidate ports.
    #[cfg(windows)]
    fn listening_ports_for_pid(pid: u32) -> Vec<u16> {
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = Command::new("powershell.exe");
        cmd.args([
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &format!(
                "Get-NetTCPConnection -OwningProcess {pid} -State Listen \
                 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty LocalPort"
            ),
        ]);
        cmd.creation_flags(CREATE_NO_WINDOW);

        let Ok(output) = cmd.output() else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut ports: Vec<u16> = stdout
            .lines()
            .filter_map(|l| l.trim().parse::<u16>().ok())
            .collect();
        ports.sort_unstable();
        ports.dedup();
        ports
    }

    /// Non-Windows platforms have no `Get-NetTCPConnection`; return an empty list by design so
    /// the caller falls back to the heuristic candidate ports.
    #[cfg(not(windows))]
    fn listening_ports_for_pid(_pid: u32) -> Vec<u16> {
        Vec::new()
    }

    /// Fetch user status from Antigravity API
    async fn fetch_user_status(
        &self,
        process_info: &ProcessInfo,
        api_port: u16,
    ) -> Result<UsageSnapshot, ProviderError> {
        // SECURITY: TLS verification disabled for local language server (see find_api_port)
        let client = crate::core::credentialed_http_client_builder()
            .timeout(std::time::Duration::from_secs(8))
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let url = format!(
            "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/GetUserStatus",
            api_port
        );

        let body = serde_json::json!({
            "metadata": {
                "ideName": "antigravity",
                "extensionName": "antigravity",
                "ideVersion": "unknown",
                "locale": "en"
            }
        });

        // Use extension server CSRF token if available, otherwise fall back to language server token
        let csrf_token = process_info
            .extension_server_csrf_token
            .as_deref()
            .unwrap_or(&process_info.csrf_token);

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("X-Codeium-Csrf-Token", csrf_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Other(format!("API request failed: {}", e)))?;

        if !resp.status().is_success() {
            // Retry with language server CSRF token if extension server token failed
            if process_info.extension_server_csrf_token.is_some() {
                let retry_resp = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("Connect-Protocol-Version", "1")
                    .header("X-Codeium-Csrf-Token", &process_info.csrf_token)
                    .json(&body)
                    .send()
                    .await;

                if let Ok(retry) = retry_resp
                    && retry.status().is_success()
                {
                    let json: UserStatusResponse = retry
                        .json()
                        .await
                        .map_err(|e| ProviderError::Parse(e.to_string()))?;
                    return self.parse_user_status(json);
                }
            }

            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "API error {}: {}",
                status, text
            )));
        }

        let json: UserStatusResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("Failed to parse response: {}", e)))?;

        self.parse_user_status(json)
    }

    /// Fetch the preferred `RetrieveUserQuotaSummary` probe (Option B).
    ///
    /// The summary is the same payload Antigravity's own Models settings screen
    /// renders: two groups (Gemini Models / Claude and GPT models) each with a
    /// weekly and a five-hour bucket. Older IDE language servers answer 404 for
    /// this endpoint; the caller falls back to `GetUserStatus` then.
    async fn fetch_quota_summary(
        &self,
        process_info: &ProcessInfo,
        api_port: u16,
    ) -> Result<UsageSnapshot, ProviderError> {
        // SECURITY: TLS verification disabled for local language server (see find_api_port)
        let client = crate::core::credentialed_http_client_builder()
            .timeout(std::time::Duration::from_secs(5))
            .danger_accept_invalid_certs(true)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let url = format!(
            "https://127.0.0.1:{}/exa.language_server_pb.LanguageServerService/RetrieveUserQuotaSummary",
            api_port
        );

        // Mirrors the upstream macOS probe: the summary endpoint takes a
        // force-refresh flag rather than the GetUserStatus metadata envelope.
        let body = serde_json::json!({ "forceRefresh": true });

        let csrf_token = process_info
            .extension_server_csrf_token
            .as_deref()
            .unwrap_or(&process_info.csrf_token);

        let resp = client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Connect-Protocol-Version", "1")
            .header("X-Codeium-Csrf-Token", csrf_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Other(format!("API request failed: {}", e)))?;

        if !resp.status().is_success() {
            // Retry with language server CSRF token if extension server token failed
            if process_info.extension_server_csrf_token.is_some() {
                let retry_resp = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("Connect-Protocol-Version", "1")
                    .header("X-Codeium-Csrf-Token", &process_info.csrf_token)
                    .json(&body)
                    .send()
                    .await;

                if let Ok(retry) = retry_resp
                    && retry.status().is_success()
                {
                    let json: QuotaSummaryResponse = retry
                        .json()
                        .await
                        .map_err(|e| ProviderError::Parse(e.to_string()))?;
                    return self.parse_quota_summary(json);
                }
            }

            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(ProviderError::Other(format!(
                "API error {}: {}",
                status, text
            )));
        }

        let json: QuotaSummaryResponse = resp
            .json()
            .await
            .map_err(|e| ProviderError::Other(format!("Failed to parse response: {}", e)))?;

        self.parse_quota_summary(json)
    }

    fn parse_user_status(
        &self,
        response: UserStatusResponse,
    ) -> Result<UsageSnapshot, ProviderError> {
        let user_status = response
            .user_status
            .ok_or_else(|| ProviderError::Other("Missing userStatus".to_string()))?;

        let model_configs = user_status
            .cascade_model_config_data
            .and_then(|d| d.client_model_configs)
            .unwrap_or_default();

        let mut quota_configs = model_configs
            .iter()
            .filter(|config| config.quota_info.is_some())
            .filter(|config| !model_label(config).is_empty())
            .collect::<Vec<_>>();
        quota_configs.sort_by(|a, b| compare_model_configs(a, b));

        // Family pools. Antigravity reports every model the plan covers, so the
        // same pool can contain many raw rows (Gemini 3.1 Pro High/Low, Claude
        // 3.5/4, ...). Each pool is summarized by its most restricted KNOWN
        // remaining row; image/lite/autocomplete/internal rows are excluded
        // from the pool summaries (they are not selectable text models).
        let gemini_pool = quota_configs
            .iter()
            .copied()
            .filter(|config| is_gemini_family(model_label(config)))
            .filter(|config| !is_noisy_config(config))
            .collect::<Vec<_>>();
        let claude_gpt_pool = quota_configs
            .iter()
            .copied()
            .filter(|config| is_claude_gpt_family(model_label(config)))
            .filter(|config| !is_noisy_config(config))
            .collect::<Vec<_>>();

        let gemini_representative = best_pool_representative(&gemini_pool);
        let claude_gpt_representative = best_pool_representative(&claude_gpt_pool);

        // Gemini is the primary family summary; Claude/GPT is secondary. When
        // Gemini has no known row at all, the most restricted known Claude/GPT
        // row takes the primary slot so the tray still has a real reading.
        // `fallback_representative` covers unknown-family selectable text rows
        // (the upstream local fallback).
        let primary = gemini_representative
            .or(claude_gpt_representative)
            .or_else(|| fallback_representative(&quota_configs))
            .and_then(|config| config.quota_info.as_ref())
            .map(rate_window_from_known_quota)
            .unwrap_or_else(|| RateWindow::informational(LIMITS_UNAVAILABLE.to_string()));

        let secondary = if gemini_representative.is_some() {
            claude_gpt_representative
                .and_then(|config| config.quota_info.as_ref())
                .map(rate_window_from_known_quota)
        } else {
            None
        };

        let mut snapshot = UsageSnapshot::new(primary);
        if let Some(sec) = secondary {
            snapshot = snapshot.with_secondary(sec);
        }

        // A family with no known remaining row still surfaces reset metadata as
        // an unavailable window (never as a fabricated 100% bar).
        for (pool, pool_id, pool_title, represented) in [
            (
                gemini_pool,
                "antigravity-pool-gemini",
                "Gemini",
                gemini_representative.is_some(),
            ),
            (
                claude_gpt_pool,
                "antigravity-pool-claude-gpt",
                "Claude/GPT",
                claude_gpt_representative.is_some(),
            ),
        ] {
            if represented {
                continue;
            }
            let Some(reset_only) = pool.iter().find(|config| {
                let quota = config.quota_info.as_ref().unwrap();
                quota.remaining_fraction.is_none()
                    && (quota.reset_time.is_some() || quota.reset_description.is_some())
            }) else {
                continue;
            };
            let quota = reset_only.quota_info.as_ref().unwrap();
            let named = NamedRateWindow::new(pool_id, pool_title, unavailable_window(quota))
                .with_usage_known(false);
            snapshot = snapshot.with_named_rate_window(named);
        }

        // Distinct extras: rows that do NOT represent a family pool (noisy
        // models such as image/lite/autocomplete, or unknown-family text rows)
        // are only kept when they are genuinely distinct — actually consumed
        // (remaining < 99.9%) or reset-only with unknown usage. Raw model rows
        // that merely duplicate a family representative are no longer appended.
        let fallback_used_as_primary = if gemini_representative.is_none()
            && claude_gpt_representative.is_none()
        {
            fallback_representative(&quota_configs)
        } else {
            None
        };

        for config in &quota_configs {
            if is_summary_candidate(config) {
                continue; // represented by a family pool
            }
            if let Some(fallback) = fallback_used_as_primary {
                if std::ptr::eq(*config, fallback) {
                    continue; // already surfaced as the primary slot
                }
            }
            let Some(quota) = &config.quota_info else {
                continue;
            };
            let known = quota.remaining_fraction.is_some();
            let consumed = known && quota.remaining_fraction.unwrap() < 0.999;
            let reset_only = !known
                && (quota.reset_time.is_some() || quota.reset_description.is_some());
            if !consumed && !reset_only {
                continue;
            }
            let title = clean_model_label(model_label(config));
            if title.is_empty() {
                continue;
            }
            let window = if known {
                rate_window_from_known_quota(quota)
            } else {
                unavailable_window(quota)
            };
            let named = NamedRateWindow::new(model_window_id(config), title, window)
                .with_usage_known(known);
            snapshot = snapshot.with_named_rate_window(named);
        }

        // Add plan info
        let plan_name = user_status
            .plan_status
            .and_then(|ps| ps.plan_info)
            .and_then(|pi| pi.plan_display_name.or(pi.plan_name));

        if let Some(plan) = plan_name {
            snapshot = snapshot.with_login_method(&plan);
        }

        Ok(snapshot)
    }

    /// Parse a `RetrieveUserQuotaSummary` payload into the four named windows
    /// (Gemini/Claude-GPT × five-hour/weekly) shown by the official Models
    /// settings screen. The detailed UI renders the named windows themselves;
    /// the compact/tray reading is the most restricted known bucket (resolved
    /// by the bridge, which sees the same named windows).
    fn parse_quota_summary(
        &self,
        response: QuotaSummaryResponse,
    ) -> Result<UsageSnapshot, ProviderError> {
        // Mirror the upstream `AntigravityQuotaSummaryParser.swift`
        // `invalidCode` check: an HTTP 200 response can still carry a
        // non-success `code`; reject that summary so the caller falls back to
        // GetUserStatus instead of trusting a partial error payload.
        if let Some(code) = quota_summary_invalid_code(&response) {
            return Err(ProviderError::Other(format!(
                "Quota summary rejected: invalid response code {}",
                code
            )));
        }

        let payload = response
            .payload()
            .ok_or_else(|| ProviderError::Other("Missing quota summary".to_string()))?;

        // Build one named window per group bucket, keeping the upstream sort
        // (Gemini before Claude/GPT, five-hour before weekly).
        let mut natural_order = 0usize;
        let mut named: Vec<(usize, NamedRateWindow)> = Vec::new();
        let mut groups = payload.groups.into_iter().enumerate().collect::<Vec<_>>();
        groups.sort_by(|a, b| {
            group_sort_rank(&a.1)
                .cmp(&group_sort_rank(&b.1))
                .then_with(|| a.0.cmp(&b.0))
        });
        for (_group_index, group) in groups {
            let group_title = group_display_title(&group);
            let mut buckets = group.buckets.into_iter().enumerate().collect::<Vec<_>>();
            buckets.sort_by(|a, b| {
                bucket_sort_rank(&a.1)
                    .cmp(&bucket_sort_rank(&b.1))
                    .then_with(|| a.0.cmp(&b.0))
            });
            for (_bucket_index, bucket) in buckets {
                let Some(bucket_id) = bucket
                    .bucket_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                else {
                    continue;
                };
                let bucket_title = bucket_display_title(&bucket);
                let remaining_fraction = bucket.remaining_fraction.or_else(|| {
                    bucket.remaining.as_ref().and_then(|r| r.remaining_fraction)
                });
                let usage_known = !bucket.disabled && remaining_fraction.is_some();
                let window_minutes = bucket_window_minutes(&bucket);
                let resets_at = bucket.reset_time.as_deref().and_then(parse_reset_time);
                let reset_description = bucket.description.clone().or_else(|| {
                    // A reset time that did not parse as a timestamp keeps its
                    // raw text so the user still sees the provider's wording.
                    if resets_at.is_none() {
                        bucket.reset_time.clone()
                    } else {
                        None
                    }
                });
                let window = if usage_known {
                    let remaining = remaining_fraction.unwrap_or(0.0).clamp(0.0, 1.0);
                    RateWindow::with_details(
                        (1.0 - remaining) * 100.0,
                        window_minutes,
                        resets_at,
                        reset_description,
                    )
                } else {
                    let description =
                        reset_description.unwrap_or_else(|| LIMITS_UNAVAILABLE.to_string());
                    RateWindow::informational(description)
                };
                let id = format!("antigravity-quota-summary-{}", slugify(bucket_id));
                let title = format!("{} {}", group_title, bucket_title);
                let named_window =
                    NamedRateWindow::new(id, title, window).with_usage_known(usage_known);
                named.push((natural_order, named_window));
                natural_order += 1;
            }
        }

        if named.is_empty() {
            return Err(ProviderError::Other(
                "Quota summary has no usable quota buckets".to_string(),
            ));
        }
        if !named.iter().any(|(_, w)| w.usage_known) {
            return Err(ProviderError::Other(
                "Quota summary has no usable quota buckets".to_string(),
            ));
        }

        // Most restricted bucket first so compact/tray (which draw the first
        // window) show the tightest constraint; unknown windows sort last.
        named.sort_by(|a, b| {
            b.1.usage_known
                .cmp(&a.1.usage_known)
                .then_with(|| {
                    b.1.window
                        .used_percent
                        .partial_cmp(&a.1.window.used_percent)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.0.cmp(&b.0))
        });

        // The primary slot is a placeholder that the detailed UI skips; the
        // real buckets live in `extra_rate_windows`, so no window is ever
        // rendered twice. The native tray resolves the most restricted known
        // bucket from the same named windows (see `tray_bridge.rs`).
        let mut snapshot = UsageSnapshot::new(RateWindow::informational(""));
        for (_, named_window) in named {
            snapshot = snapshot.with_named_rate_window(named_window);
        }
        Ok(snapshot)
    }
}

impl Default for AntigravityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for AntigravityProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Antigravity
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, _ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        tracing::debug!("Fetching Antigravity usage via local probe");

        let process_info = match Self::detect_process_info() {
            Ok(info) => info,
            Err(e) => {
                tracing::warn!("Antigravity probe failed: {}", e);
                return Err(e);
            }
        };
        let api_port = match Self::find_api_port(process_info.extension_port, process_info.pid).await
        {
            Ok(port) => port,
            Err(e) => {
                tracing::warn!("Antigravity probe failed: {}", e);
                return Err(e);
            }
        };

        // Preferred source: RetrieveUserQuotaSummary (the two-group payload the
        // official Models screen shows). Old IDE servers answer 404; fall back
        // to GetUserStatus and aggregate by family there.
        match self.fetch_quota_summary(&process_info, api_port).await {
            Ok(usage) => {
                tracing::debug!("Antigravity quota summary accepted");
                return Ok(ProviderFetchResult::new(usage, "local"));
            }
            Err(summary_err) => {
                tracing::debug!(
                    "Antigravity quota summary unavailable; falling back to user status: {}",
                    summary_err
                );
            }
        }

        match self.fetch_user_status(&process_info, api_port).await {
            Ok(usage) => Ok(ProviderFetchResult::new(usage, "local")),
            Err(e) => {
                tracing::warn!("Antigravity probe failed: {}", e);
                Err(e)
            }
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Cli]
    }

    fn supports_cli(&self) -> bool {
        true
    }
}

struct ProcessInfo {
    csrf_token: String,
    extension_server_csrf_token: Option<String>,
    extension_port: Option<u16>,
    pid: Option<u32>,
}

// API Response types

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatusResponse {
    user_status: Option<UserStatus>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UserStatus {
    #[allow(dead_code)]
    email: Option<String>,
    plan_status: Option<PlanStatus>,
    cascade_model_config_data: Option<ModelConfigData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanStatus {
    plan_info: Option<PlanInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanInfo {
    plan_name: Option<String>,
    plan_display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelConfigData {
    client_model_configs: Option<Vec<ModelConfig>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelConfig {
    #[serde(default)]
    label: String,
    #[serde(default)]
    model_id: Option<String>,
    #[serde(default)]
    id: Option<String>,
    quota_info: Option<QuotaInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaInfo {
    remaining_fraction: Option<f64>,
    reset_time: Option<String>,
    reset_description: Option<String>,
}

// ── RetrieveUserQuotaSummary response types ────────────────────────────
// Mirrors the upstream macOS probe parser
// (`AntigravityQuotaSummaryParser.swift`): the payload can arrive under
// `response`, `summary`, or directly at the root as `description` + `groups`.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryResponse {
    /// Connect response code; validated like the upstream parser's
    /// `invalidCode` (int `0`, or the strings `ok` / `success` / `0` are OK).
    code: Option<serde_json::Value>,
    #[allow(dead_code)]
    message: Option<String>,
    #[serde(default)]
    response: Option<QuotaSummaryPayload>,
    #[serde(default)]
    summary: Option<QuotaSummaryPayload>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    groups: Vec<QuotaSummaryGroupPayload>,
}

impl QuotaSummaryResponse {
    /// The effective payload: `response`, then `summary`, then a root-level
    /// `groups` array wrapped as a payload (upstream `rootPayload`).
    fn payload(&self) -> Option<QuotaSummaryPayload> {
        self.response
            .clone()
            .or_else(|| self.summary.clone())
            .or_else(|| {
                if self.groups.is_empty() {
                    None
                } else {
                    Some(QuotaSummaryPayload {
                        description: self.description.clone(),
                        groups: self.groups.clone(),
                    })
                }
            })
    }
}

/// Mirror of the upstream `AntigravityStatusProbe.invalidCode` helper
/// (`AntigravityQuotaSummaryParser.swift`): a `code` field that is not
/// success (int `0`, or the strings `ok` / `success` / `0`) invalidates the
/// whole summary. An absent (or JSON-null `Option`) `code` is OK.
fn quota_summary_invalid_code(response: &QuotaSummaryResponse) -> Option<String> {
    let code = response.code.as_ref()?;
    let is_ok = match code {
        serde_json::Value::Number(n) => n.as_i64().is_some_and(|value| value == 0),
        serde_json::Value::String(s) => {
            let lower = s.to_lowercase();
            lower == "ok" || lower == "success" || lower == "0"
        }
        // Bools/objects/arrays/floats do not decode as an upstream `CodeValue`
        // (int or string); upstream fails decoding them, so reject too.
        _ => false,
    };
    if is_ok {
        None
    } else {
        Some(match code {
            serde_json::Value::Number(n) => n.to_string(),
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        })
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryPayload {
    #[allow(dead_code)]
    description: Option<String>,
    #[serde(default)]
    groups: Vec<QuotaSummaryGroupPayload>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryGroupPayload {
    display_name: Option<String>,
    #[allow(dead_code)]
    description: Option<String>,
    #[serde(default)]
    buckets: Vec<QuotaSummaryBucketPayload>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaSummaryBucketPayload {
    bucket_id: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    disabled: bool,
    remaining_fraction: Option<f64>,
    remaining: Option<QuotaSummaryRemainingPayload>,
    reset_time: Option<String>,
}

/// The `remaining` oneof: either a plain `remainingFraction` or a protobuf
/// oneof shape `{"case": "remainingFraction", "value": 0.65}`.
#[derive(Debug, Clone)]
struct QuotaSummaryRemainingPayload {
    remaining_fraction: Option<f64>,
}

impl<'de> Deserialize<'de> for QuotaSummaryRemainingPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            remaining_fraction: Option<f64>,
            #[serde(rename = "case")]
            oneof_case: Option<String>,
            value: Option<f64>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let remaining_fraction = raw
            .remaining_fraction
            .or_else(|| {
                if raw.oneof_case.as_deref() == Some("remainingFraction") {
                    raw.value
                } else {
                    None
                }
            });
        Ok(Self { remaining_fraction })
    }
}

// ── Model-family classification ──────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum ModelFamily {
    Claude,
    ClaudeThinking,
    Gpt,
    GeminiPro,
    GeminiFlash,
    Other,
}

fn classify_model(label: &str) -> ModelFamily {
    let lower = label.to_lowercase();
    if lower.contains("claude") {
        if lower.contains("thinking") {
            ModelFamily::ClaudeThinking
        } else {
            ModelFamily::Claude
        }
    } else if lower.contains("gpt") || lower.contains("openai") {
        ModelFamily::Gpt
    } else if lower.contains("gemini") && lower.contains("pro") {
        ModelFamily::GeminiPro
    } else if lower.contains("gemini") && lower.contains("flash") {
        ModelFamily::GeminiFlash
    } else if lower.contains("pro") && !is_noisy_summary_model(&lower) {
        ModelFamily::GeminiPro
    } else if lower.contains("flash") {
        ModelFamily::GeminiFlash
    } else {
        ModelFamily::Other
    }
}

fn is_gemini_family(label: &str) -> bool {
    matches!(
        classify_model(label),
        ModelFamily::GeminiPro | ModelFamily::GeminiFlash
    )
}

fn is_claude_gpt_family(label: &str) -> bool {
    matches!(
        classify_model(label),
        ModelFamily::Claude | ModelFamily::ClaudeThinking | ModelFamily::Gpt
    )
}

/// A row that belongs to a family pool and is a selectable text model. Pool
/// representatives are drawn from summary candidates; everything else is only
/// eligible as a distinct extra (consumed or reset-only).
fn is_summary_candidate(config: &ModelConfig) -> bool {
    let label = model_label(config);
    classify_model(label) != ModelFamily::Other && !is_noisy_config(config)
}

fn is_noisy_config(config: &ModelConfig) -> bool {
    let label = model_label(config);
    if is_noisy_summary_model(label) {
        return true;
    }
    // Upstream treats `tab_`-prefixed ids as autocomplete lanes.
    config
        .model_id
        .as_deref()
        .or(config.id.as_deref())
        .is_some_and(|id| id.trim_start().to_ascii_lowercase().starts_with("tab_"))
}

/// Most restricted known row of a pool: lowest remaining fraction; ties go to
/// the earlier reset time, then the label.
fn best_pool_representative<'a>(pool: &[&'a ModelConfig]) -> Option<&'a ModelConfig> {
    pool.iter()
        .copied()
        .filter(|config| {
            config
                .quota_info
                .as_ref()
                .is_some_and(|q| q.remaining_fraction.is_some())
        })
        .min_by(|a, b| {
            let a_quota = a.quota_info.as_ref().unwrap();
            let b_quota = b.quota_info.as_ref().unwrap();
            a_quota
                .remaining_fraction
                .unwrap()
                .partial_cmp(&b_quota.remaining_fraction.unwrap())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| reset_time_cmp(a_quota, b_quota))
                .then_with(|| {
                    model_label(a)
                        .to_lowercase()
                        .cmp(&model_label(b).to_lowercase())
                })
        })
}

/// Upstream local fallback: an unknown-family selectable text row with a known
/// remaining fraction, choosing the most restricted one.
fn fallback_representative<'a>(all: &[&'a ModelConfig]) -> Option<&'a ModelConfig> {
    all.iter()
        .copied()
        .filter(|config| classify_model(model_label(config)) == ModelFamily::Other)
        .filter(|config| !is_noisy_config(config))
        .filter(|config| {
            config
                .quota_info
                .as_ref()
                .is_some_and(|q| q.remaining_fraction.is_some())
        })
        .min_by(|a, b| {
            let a_quota = a.quota_info.as_ref().unwrap();
            let b_quota = b.quota_info.as_ref().unwrap();
            a_quota
                .remaining_fraction
                .unwrap()
                .partial_cmp(&b_quota.remaining_fraction.unwrap())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    model_label(a)
                        .to_lowercase()
                        .cmp(&model_label(b).to_lowercase())
                })
        })
}

fn reset_time_cmp(a: &QuotaInfo, b: &QuotaInfo) -> std::cmp::Ordering {
    let a_time = a.reset_time.as_deref().and_then(parse_reset_time);
    let b_time = b.reset_time.as_deref().and_then(parse_reset_time);
    match (a_time, b_time) {
        (Some(a_time), Some(b_time)) => a_time.cmp(&b_time),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn compare_model_configs(a: &ModelConfig, b: &ModelConfig) -> std::cmp::Ordering {
    let a_label = model_label(a);
    let b_label = model_label(b);
    family_rank(classify_model(a_label))
        .cmp(&family_rank(classify_model(b_label)))
        .then_with(|| parse_model_version(b_label).cmp(&parse_model_version(a_label)))
        .then_with(|| tier_rank(a_label).cmp(&tier_rank(b_label)))
        .then_with(|| clean_model_label(a_label).cmp(&clean_model_label(b_label)))
}

fn family_rank(family: ModelFamily) -> u8 {
    match family {
        ModelFamily::Claude => 0,
        ModelFamily::Gpt => 1,
        ModelFamily::GeminiPro => 2,
        ModelFamily::GeminiFlash => 3,
        ModelFamily::ClaudeThinking => 4,
        ModelFamily::Other => 5,
    }
}

fn tier_rank(label: &str) -> u8 {
    let lower = label.to_lowercase();
    if lower.contains("high") {
        0
    } else if lower.contains("medium") {
        1
    } else if lower.contains("low") {
        2
    } else {
        3
    }
}

fn parse_model_version(label: &str) -> (u16, u16) {
    static VERSION_RE: OnceLock<Regex> = OnceLock::new();
    let regex =
        VERSION_RE.get_or_init(|| Regex::new(r"(?i)(\d+)(?:[.-](\d+))?").expect("valid regex"));
    let Some(caps) = regex.captures(label) else {
        return (0, 0);
    };
    let major = caps
        .get(1)
        .and_then(|m| m.as_str().parse::<u16>().ok())
        .unwrap_or(0);
    let minor = caps
        .get(2)
        .and_then(|m| m.as_str().parse::<u16>().ok())
        .unwrap_or(0);
    (major, minor)
}

fn is_noisy_summary_model(label: &str) -> bool {
    let lower = label.to_lowercase();
    lower.contains("image")
        || lower.contains("lite")
        || lower.contains("autocomplete")
        || lower.contains("completion")
        || lower.contains("internal")
}

fn model_label(config: &ModelConfig) -> &str {
    if !config.label.trim().is_empty() {
        &config.label
    } else if let Some(model_id) = config.model_id.as_deref() {
        model_id
    } else {
        config.id.as_deref().unwrap_or_default()
    }
}

fn model_window_id(config: &ModelConfig) -> String {
    let raw = config
        .model_id
        .as_deref()
        .or(config.id.as_deref())
        .unwrap_or_else(|| model_label(config));
    format!("model-{}", slugify(raw))
}

fn slugify(raw: &str) -> String {
    let slug = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

/// A known-fraction quota row becomes a real rate window. The reset time is
/// parsed so the bridge can show a countdown; when it does not parse, the raw
/// text is kept as the reset description so nothing is lost.
fn rate_window_from_known_quota(quota: &QuotaInfo) -> RateWindow {
    let remaining = quota.remaining_fraction.unwrap_or(0.0).clamp(0.0, 1.0);
    let used_percent = (1.0 - remaining) * 100.0;
    let resets_at = quota.reset_time.as_deref().and_then(parse_reset_time);
    let reset_description = quota.reset_description.clone().or_else(|| {
        if resets_at.is_none() {
            quota.reset_time.clone()
        } else {
            None
        }
    });
    RateWindow::with_details(used_percent, None, resets_at, reset_description)
}

/// A row/bucket with no known remaining fraction is informational: it carries
/// the provider's reset wording (or an explicit unavailable marker) and must
/// never render as a fabricated 100%-remaining bar.
fn unavailable_window(quota: &QuotaInfo) -> RateWindow {
    let description = quota
        .reset_description
        .clone()
        .or_else(|| quota.reset_time.clone())
        .unwrap_or_else(|| LIMITS_UNAVAILABLE.to_string());
    RateWindow::informational(description)
}

fn parse_reset_time(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
}

fn clean_model_label(label: &str) -> String {
    let mut out = label.trim().replace('_', " ");
    while out.contains("  ") {
        out = out.replace("  ", " ");
    }
    out
}

// ── Quota summary bucket/group classification ────────────────────────

const LIMITS_UNAVAILABLE: &str = "Limits not available";

fn group_display_title(group: &QuotaSummaryGroupPayload) -> String {
    let title = group.display_name.as_deref().unwrap_or_default().trim();
    let lower = title.to_lowercase();
    if lower.contains("gemini") {
        "Gemini".to_string()
    } else if lower.contains("claude") || lower.contains("gpt") {
        "Claude/GPT".to_string()
    } else if title.is_empty() {
        "Quota".to_string()
    } else {
        title.to_string()
    }
}

fn bucket_display_title(bucket: &QuotaSummaryBucketPayload) -> String {
    match quota_bucket_kind(bucket) {
        QuotaBucketKind::Session => "5-hour".to_string(),
        QuotaBucketKind::Weekly => "weekly".to_string(),
        QuotaBucketKind::Other => bucket
            .display_name
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn bucket_window_minutes(bucket: &QuotaSummaryBucketPayload) -> Option<u32> {
    match quota_bucket_kind(bucket) {
        QuotaBucketKind::Session => Some(300),
        QuotaBucketKind::Weekly => Some(7 * 24 * 60),
        QuotaBucketKind::Other => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum QuotaBucketKind {
    Session,
    Weekly,
    Other,
}

fn quota_bucket_kind(bucket: &QuotaSummaryBucketPayload) -> QuotaBucketKind {
    let candidates = quota_cadence_candidates(bucket);
    if ["session", "5h", "5-hour", "five-hour"]
        .iter()
        .any(|alias| candidates.contains(*alias))
    {
        QuotaBucketKind::Session
    } else if candidates.contains("weekly") {
        QuotaBucketKind::Weekly
    } else {
        QuotaBucketKind::Other
    }
}

const SESSION_CADENCE_ALIASES: [&str; 5] = ["session", "5h", "5-hour", "five hour", "five-hour"];

/// Normalized cadence tokens drawn from the bucket id and display name.
///
/// The real payloads name their buckets with a `-limit` suffix
/// (`gemini-models-five-hour-limit`) and their display names with spaces
/// (`Five Hour Limit Remaining`), so both separators are unified and the alias
/// is matched either as an exact token, a `-<alias>` suffix, or an embedded
/// word. This is deliberately a little more permissive than the upstream
/// suffix-only matcher, because a cadence hint only renames the window — the
/// numbers come from `remainingFraction` regardless.
fn quota_cadence_candidates(bucket: &QuotaSummaryBucketPayload) -> std::collections::HashSet<String> {
    let mut candidates = std::collections::HashSet::new();
    for raw in [bucket.bucket_id.as_deref(), bucket.display_name.as_deref()] {
        let Some(raw) = raw else { continue };
        let normalized = raw.trim().to_lowercase().replace('_', "-");
        if normalized.is_empty() {
            continue;
        }
        let dashed = normalized.replace(' ', "-");
        for candidate in [normalized, dashed] {
            if candidate.is_empty() {
                continue;
            }
            candidates.insert(candidate.clone());
            for alias in SESSION_CADENCE_ALIASES {
                let alias = alias.replace(' ', "-");
                if candidate == alias
                    || candidate.ends_with(&format!("-{alias}"))
                    || candidate.contains(&alias)
                {
                    candidates.insert(alias);
                }
            }
            if candidate.contains("weekly") {
                candidates.insert("weekly".to_string());
            }
        }
    }
    candidates
}

fn group_sort_rank(group: &QuotaSummaryGroupPayload) -> u8 {
    let title = group
        .display_name
        .as_deref()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    if title.contains("gemini") {
        0
    } else if title.contains("claude") || title.contains("gpt") {
        1
    } else {
        2
    }
}

fn bucket_sort_rank(bucket: &QuotaSummaryBucketPayload) -> u8 {
    match quota_bucket_kind(bucket) {
        QuotaBucketKind::Session => 0,
        QuotaBucketKind::Weekly => 1,
        QuotaBucketKind::Other => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_model_families() {
        assert_eq!(classify_model("Claude 3.5 Sonnet"), ModelFamily::Claude);
        assert_eq!(classify_model("claude-4-opus"), ModelFamily::Claude);
        assert_eq!(
            classify_model("Claude Thinking"),
            ModelFamily::ClaudeThinking
        );
        assert_eq!(
            classify_model("claude-3.5-sonnet-thinking"),
            ModelFamily::ClaudeThinking
        );
        assert_eq!(classify_model("Gemini 2.5 Pro Low"), ModelFamily::GeminiPro);
        assert_eq!(classify_model("gemini-pro-low"), ModelFamily::GeminiPro);
        assert_eq!(classify_model("Pro Low Latency"), ModelFamily::GeminiPro);
        assert_eq!(classify_model("Gemini 2.5 Flash"), ModelFamily::GeminiFlash);
        assert_eq!(classify_model("gemini-flash"), ModelFamily::GeminiFlash);
        assert_eq!(classify_model("Flash Model"), ModelFamily::GeminiFlash);
        // GPT/OpenAI rows join the Claude/GPT pool, not "Other".
        assert_eq!(classify_model("GPT-4o"), ModelFamily::Gpt);
        assert_eq!(classify_model("openai-gpt-5"), ModelFamily::Gpt);
        assert_eq!(classify_model("unknown-model"), ModelFamily::Other);
    }

    #[test]
    fn empty_label_never_drives_a_window() {
        let resp = make_response(vec![("", 0.5)]);
        let snapshot = AntigravityProvider::new().parse_user_status(resp).unwrap();
        assert!(snapshot.extra_rate_windows.is_empty());
    }

    #[test]
    fn parses_current_language_server_process() {
        let output = r"4242	C:\Users\test\AppData\Local\Programs\Antigravity\resources\bin\language_server.exe --csrf_token 11111111-2222-3333-4444-555555555555 --extension_server_port 54123";

        let process = AntigravityProvider::parse_process_info(output).expect("process info");

        assert_eq!(process.pid, Some(4242));
        assert_eq!(process.extension_port, Some(54123));
        assert_eq!(process.csrf_token, "11111111-2222-3333-4444-555555555555");
    }

    #[test]
    fn parses_language_server_without_extension_server_port() {
        let output = "34564\tC:\\Users\\test\\AppData\\Local\\Programs\\Antigravity\\resources\\bin\\language_server.exe --standalone --override_ide_name antigravity --subclient_type hub --override_ide_version 2.0.11 --https_server_port 0 --csrf_token 68dda2fb-6b26-40c0-aeef-b9a628615714 --app_data_dir antigravity";

        let process = AntigravityProvider::parse_process_info(output)
            .expect("process info should be detected");

        assert_eq!(process.pid, Some(34564));
        assert_eq!(process.extension_port, Some(0));
        assert_eq!(process.csrf_token, "68dda2fb-6b26-40c0-aeef-b9a628615714");
    }

    #[test]
    fn parses_language_server_without_any_port_arg() {
        let output = "34564\tC:\\Users\\test\\AppData\\Local\\Programs\\Antigravity\\resources\\bin\\language_server.exe --standalone --csrf_token aabbccdd-1122-3344-5566-778899001122 --app_data_dir antigravity";

        let process = AntigravityProvider::parse_process_info(output)
            .expect("process info should be detected");

        assert_eq!(process.pid, Some(34564));
        assert_eq!(process.extension_port, None);
        assert_eq!(process.csrf_token, "aabbccdd-1122-3344-5566-778899001122");
    }

    #[test]
    fn parses_equals_form_args() {
        let output = "34564\tC:\\Users\\test\\AppData\\Local\\Programs\\Antigravity\\resources\\bin\\language_server.exe --csrf_token=68dda2fb-6b26-40c0-aeef-b9a628615714 --https_server_port=61999";

        let process = AntigravityProvider::parse_process_info(output)
            .expect("process info should be detected");

        assert_eq!(process.pid, Some(34564));
        assert_eq!(process.extension_port, Some(61999));
        assert_eq!(process.csrf_token, "68dda2fb-6b26-40c0-aeef-b9a628615714");
    }

    fn make_config(
        label: &str,
        remaining: Option<f64>,
        reset_time: Option<&str>,
    ) -> serde_json::Value {
        let mut quota = serde_json::Map::new();
        if let Some(remaining) = remaining {
            quota.insert("remainingFraction".to_string(), serde_json::json!(remaining));
        }
        if let Some(reset_time) = reset_time {
            quota.insert("resetTime".to_string(), serde_json::json!(reset_time));
        }
        serde_json::json!({
            "label": label,
            "modelId": label.to_lowercase().replace(' ', "-"),
            "quotaInfo": serde_json::Value::Object(quota),
        })
    }

    fn make_response(models: Vec<(&str, f64)>) -> UserStatusResponse {
        let json = serde_json::json!({
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelConfigs": models.iter().map(|(label, remaining)| {
                        make_config(label, Some(*remaining), None)
                    }).collect::<Vec<_>>()
                }
            }
        });
        serde_json::from_value(json).unwrap()
    }

    fn make_response_with_configs(configs: Vec<serde_json::Value>) -> UserStatusResponse {
        let json = serde_json::json!({
            "userStatus": {
                "cascadeModelConfigData": {
                    "clientModelConfigs": configs
                }
            }
        });
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn test_parse_user_status_standard_family_aggregation() {
        let resp = make_response(vec![
            ("Claude 3.5 Sonnet", 0.8),
            ("Gemini 2.5 Pro Low", 0.5),
            ("Gemini 2.5 Flash", 0.9),
        ]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        // Gemini pool is primary (most restricted known: 0.5 remaining → 50%).
        assert!((snap.primary.used_percent - 50.0).abs() < 0.1);
        // Claude/GPT pool is secondary (0.8 remaining → 20%).
        let sec = snap.secondary.unwrap();
        assert!((sec.used_percent - 20.0).abs() < 0.1);
        // Raw model rows are no longer appended as duplicate extras.
        assert_eq!(snap.extra_rate_windows.len(), 0);
        assert!(snap.model_specific.is_none());
    }

    #[test]
    fn test_parse_user_status_thinking_joins_claude_pool() {
        let resp = make_response(vec![
            ("Claude Thinking", 0.6),
            ("Claude 3.5 Sonnet", 0.7),
            ("Gemini 2.5 Flash", 0.5),
        ]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        // Gemini pool: Flash 0.5 → 50% used (primary).
        assert!((snap.primary.used_percent - 50.0).abs() < 0.1);
        // Claude pool picks the lowest remaining known: Thinking 0.6 → 40%.
        let sec = snap.secondary.unwrap();
        assert!((sec.used_percent - 40.0).abs() < 0.1);
        assert_eq!(snap.extra_rate_windows.len(), 0);
    }

    #[test]
    fn test_parse_user_status_gpt_joins_claude_gpt_pool() {
        let resp = make_response(vec![("GPT-4o", 0.4), ("Mistral Large", 0.6)]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        // No Gemini rows: the Claude/GPT representative takes primary.
        assert!((snap.primary.used_percent - 60.0).abs() < 0.1);
        assert!(snap.secondary.is_none());
        // Mistral Large is an unknown-family selectable text row with known
        // usage; it is a distinct consumed extra, not a raw duplicate.
        assert_eq!(snap.extra_rate_windows.len(), 1);
        assert_eq!(snap.extra_rate_windows[0].title, "Mistral Large");
        assert!((snap.extra_rate_windows[0].window.used_percent - 40.0).abs() < 0.1);
    }

    #[test]
    fn test_noisy_rows_do_not_drive_pool_summaries_but_stay_distinct() {
        let resp = make_response(vec![
            ("Gemini 2.5 Flash Image", 0.01),
            ("Gemini 2.5 Pro Lite", 0.02),
            ("Gemini autocomplete internal", 0.03),
            ("Claude 4 Sonnet", 0.8),
            ("Gemini 2.5 Pro Low", 0.6),
            ("Gemini 2.5 Flash", 0.7),
        ]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        // Gemini pool ignores image/lite/autocomplete rows: Pro Low 0.6 → 40%.
        assert!((snap.primary.used_percent - 40.0).abs() < 0.1);
        // Claude pool: Sonnet 0.8 → 20%.
        assert!((snap.secondary.unwrap().used_percent - 20.0).abs() < 0.1);
        // Noisy rows that are actually consumed remain as distinct extras.
        let titles: Vec<&str> = snap
            .extra_rate_windows
            .iter()
            .map(|w| w.title.as_str())
            .collect();
        assert!(titles.contains(&"Gemini 2.5 Flash Image"));
        assert!(titles.contains(&"Gemini 2.5 Pro Lite"));
        assert!(titles.contains(&"Gemini autocomplete internal"));
        // No summary-candidate raw rows duplicated.
        assert!(!titles.contains(&"Gemini 2.5 Flash"));
    }

    #[test]
    fn test_unconsumed_noisy_rows_are_not_extras() {
        // Image/lite rows at 100% remaining are noise, not data: they must not
        // appear as distinct extras at all.
        let resp = make_response(vec![
            ("Gemini 2.5 Flash Image", 0.9999),
            ("Gemini autocomplete internal", 1.0),
            ("Claude 4 Sonnet", 0.8),
            ("Gemini 2.5 Pro Low", 0.6),
        ]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        assert_eq!(snap.extra_rate_windows.len(), 0);
        assert!((snap.primary.used_percent - 40.0).abs() < 0.1);
    }

    #[test]
    fn test_reset_only_unknown_row_is_unavailable_not_full() {
        let resp = make_response_with_configs(vec![
            make_config("Claude 3.5 Sonnet", Some(0.8), Some("2026-08-15T12:00:00Z")),
            make_config("Gemini 2.5 Pro Low", None, Some("2026-08-15T12:00:00Z")),
            make_config("Gemini 2.5 Flash", None, None),
        ]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        // No known Gemini row: the pool is surfaced as a reset-only window.
        let gemini_extra = snap
            .extra_rate_windows
            .iter()
            .find(|w| w.id == "antigravity-pool-gemini")
            .expect("reset-only gemini pool window");
        assert!(!gemini_extra.usage_known);
        assert!(gemini_extra.window.is_informational);
        assert!((gemini_extra.window.used_percent - 0.0).abs() < 0.1);
        // The unknown row is never converted into a 100% remaining bar.
        assert!((snap.primary.used_percent - 20.0).abs() < 0.1);
        // A row with neither fraction nor reset is dropped entirely.
        assert!(
            !snap
                .extra_rate_windows
                .iter()
                .any(|w| w.title == "Gemini 2.5 Flash")
        );
    }

    #[test]
    fn test_all_100_payload_produces_no_fabricated_usage() {
        let resp = make_response(vec![
            ("Claude 3.5 Sonnet", 1.0),
            ("Gemini 2.5 Pro Low", 1.0),
            ("Gemini 2.5 Flash", 1.0),
        ]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        assert!((snap.primary.used_percent - 0.0).abs() < 0.1);
        assert!((snap.secondary.unwrap().used_percent - 0.0).abs() < 0.1);
        assert_eq!(snap.extra_rate_windows.len(), 0);
    }

    #[test]
    fn test_unknown_family_selectable_text_is_fallback_primary() {
        let resp = make_response(vec![("SomeText Model", 0.3), ("Mistral Large", 0.6)]);
        let provider = AntigravityProvider::new();
        let snap = provider.parse_user_status(resp).unwrap();

        // No Gemini or Claude/GPT rows: the most restricted unknown-family
        // selectable text row becomes primary (0.3 remaining → 70% used).
        assert!((snap.primary.used_percent - 70.0).abs() < 0.1);
        assert!(snap.secondary.is_none());
        // The fallback primary is not duplicated as an extra.
        assert!(
            !snap
                .extra_rate_windows
                .iter()
                .any(|w| w.title == "SomeText Model")
        );
        // Mistral Large is still a distinct consumed extra.
        assert!(
            snap.extra_rate_windows
                .iter()
                .any(|w| w.title == "Mistral Large")
        );
    }

    // ── Quota summary path ────────────────────────────────────────────

    fn make_summary_bucket(
        bucket_id: &str,
        display_name: &str,
        remaining_fraction: Option<f64>,
        reset_time: Option<&str>,
        disabled: bool,
    ) -> serde_json::Value {
        let mut bucket = serde_json::json!({
            "bucketId": bucket_id,
            "displayName": display_name,
            "description": "resets soon",
            "disabled": disabled,
        });
        if let Some(remaining_fraction) = remaining_fraction {
            bucket["remainingFraction"] = serde_json::json!(remaining_fraction);
        }
        if let Some(reset_time) = reset_time {
            bucket["resetTime"] = serde_json::json!(reset_time);
        }
        bucket
    }

    fn make_summary_response(
        groups: Vec<(&str, Vec<serde_json::Value>)>,
        envelope: &str,
    ) -> QuotaSummaryResponse {
        let groups_json = groups
            .iter()
            .map(|(display_name, buckets)| {
                serde_json::json!({
                    "displayName": display_name,
                    "description": "group description",
                    "buckets": buckets,
                })
            })
            .collect::<Vec<_>>();
        let mut root = serde_json::Map::new();
        match envelope {
            "response" => {
                root.insert(
                    "response".to_string(),
                    serde_json::json!({ "description": "summary", "groups": groups_json }),
                );
            }
            "summary" => {
                root.insert(
                    "summary".to_string(),
                    serde_json::json!({ "description": "summary", "groups": groups_json }),
                );
            }
            _ => {
                root.insert("description".to_string(), serde_json::json!("summary"));
                root.insert("groups".to_string(), serde_json::json!(groups_json));
            }
        }
        serde_json::from_value(serde_json::Value::Object(root)).unwrap()
    }

    fn four_window_summary() -> QuotaSummaryResponse {
        make_summary_response(
            vec![
                (
                    "Gemini Models",
                    vec![
                        make_summary_bucket(
                            "gemini-models-five-hour-limit",
                            "Five Hour Limit Remaining",
                            Some(0.8),
                            Some("2026-08-14T18:00:00Z"),
                            false,
                        ),
                        make_summary_bucket(
                            "gemini-models-weekly-limit",
                            "Weekly Limit Remaining",
                            Some(0.5),
                            Some("2026-08-17T00:00:00Z"),
                            false,
                        ),
                    ],
                ),
                (
                    "Claude and GPT models",
                    vec![
                        make_summary_bucket(
                            "claude-gpt-models-five-hour-limit",
                            "Five Hour Limit Remaining",
                            Some(0.9),
                            Some("2026-08-14T19:00:00Z"),
                            false,
                        ),
                        make_summary_bucket(
                            "claude-gpt-models-weekly-limit",
                            "Weekly Limit Remaining",
                            Some(0.25),
                            Some("2026-08-16T00:00:00Z"),
                            false,
                        ),
                    ],
                ),
            ],
            "response",
        )
    }

    #[test]
    fn test_parse_quota_summary_four_named_windows() {
        let provider = AntigravityProvider::new();
        let snap = provider.parse_quota_summary(four_window_summary()).unwrap();

        assert_eq!(snap.extra_rate_windows.len(), 4);
        let titles: Vec<&str> = snap
            .extra_rate_windows
            .iter()
            .map(|w| w.title.as_str())
            .collect();
        assert!(titles.contains(&"Gemini 5-hour"));
        assert!(titles.contains(&"Gemini weekly"));
        assert!(titles.contains(&"Claude/GPT 5-hour"));
        assert!(titles.contains(&"Claude/GPT weekly"));

        // Most restricted bucket first: Claude/GPT weekly (0.25 → 75% used).
        assert_eq!(snap.extra_rate_windows[0].title, "Claude/GPT weekly");
        assert!((snap.extra_rate_windows[0].window.used_percent - 75.0).abs() < 0.1);

        // Cadence metadata is attached so the UI can name the cycle.
        let gemini_weekly = snap
            .extra_rate_windows
            .iter()
            .find(|w| w.title == "Gemini weekly")
            .unwrap();
        assert_eq!(gemini_weekly.window.window_minutes, Some(7 * 24 * 60));
        assert_eq!(
            gemini_weekly.window.resets_at,
            parse_reset_time("2026-08-17T00:00:00Z")
        );
        let gemini_session = snap
            .extra_rate_windows
            .iter()
            .find(|w| w.title == "Gemini 5-hour")
            .unwrap();
        assert_eq!(gemini_session.window.window_minutes, Some(300));

        // Placeholder primary is skipped by the detailed UI (no duplicate rows).
        assert!(snap.primary.is_informational);
        assert!((snap.primary.used_percent - 0.0).abs() < 0.1);
        assert!(snap.secondary.is_none());
    }

    #[test]
    fn test_parse_quota_summary_unknown_bucket_is_unavailable() {
        let response = make_summary_response(
            vec![(
                "Gemini Models",
                vec![
                    make_summary_bucket(
                        "gemini-models-five-hour-limit",
                        "Five Hour Limit Remaining",
                        None,
                        Some("2026-08-14T18:00:00Z"),
                        false,
                    ),
                    make_summary_bucket(
                        "gemini-models-weekly-limit",
                        "Weekly Limit Remaining",
                        Some(0.5),
                        Some("2026-08-17T00:00:00Z"),
                        false,
                    ),
                ],
            )],
            "response",
        );
        let provider = AntigravityProvider::new();
        let snap = provider.parse_quota_summary(response).unwrap();

        let unknown = snap
            .extra_rate_windows
            .iter()
            .find(|w| w.title == "Gemini 5-hour")
            .unwrap();
        assert!(!unknown.usage_known);
        assert!(unknown.window.is_informational);
        // Never a fabricated 100% remaining bar.
        assert!((unknown.window.used_percent - 0.0).abs() < 0.1);
        assert!(unknown.window.reset_description.is_some());
    }

    #[test]
    fn test_parse_quota_summary_disabled_bucket_is_unavailable() {
        let response = make_summary_response(
            vec![(
                "Gemini Models",
                vec![
                    make_summary_bucket(
                        "gemini-models-five-hour-limit",
                        "Five Hour Limit Remaining",
                        Some(0.8),
                        Some("2026-08-14T18:00:00Z"),
                        true,
                    ),
                    make_summary_bucket(
                        "gemini-models-weekly-limit",
                        "Weekly Limit Remaining",
                        Some(0.5),
                        Some("2026-08-17T00:00:00Z"),
                        false,
                    ),
                ],
            )],
            "response",
        );
        let provider = AntigravityProvider::new();
        let snap = provider.parse_quota_summary(response).unwrap();

        let disabled = snap
            .extra_rate_windows
            .iter()
            .find(|w| w.title == "Gemini 5-hour")
            .unwrap();
        assert!(!disabled.usage_known);
        assert!(disabled.window.is_informational);
    }

    #[test]
    fn test_parse_quota_summary_all_unknown_is_rejected() {
        let response = make_summary_response(
            vec![(
                "Gemini Models",
                vec![make_summary_bucket(
                    "gemini-models-weekly-limit",
                    "Weekly Limit Remaining",
                    None,
                    None,
                    false,
                )],
            )],
            "response",
        );
        let result = AntigravityProvider::new().parse_quota_summary(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_quota_summary_empty_is_rejected() {
        let response = make_summary_response(vec![], "response");
        let result = AntigravityProvider::new().parse_quota_summary(response);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_quota_summary_supports_all_envelopes() {
        let provider = AntigravityProvider::new();
        for envelope in ["response", "summary", "root"] {
            let response = make_summary_response(
                vec![(
                    "Gemini Models",
                    vec![make_summary_bucket(
                        "gemini-models-weekly-limit",
                        "Weekly Limit Remaining",
                        Some(0.5),
                        None,
                        false,
                    )],
                )],
                envelope,
            );
            let snap = provider.parse_quota_summary(response).unwrap();
            assert_eq!(snap.extra_rate_windows.len(), 1, "envelope {envelope}");
            assert_eq!(snap.extra_rate_windows[0].title, "Gemini weekly");
        }
    }

    #[test]
    fn test_parse_quota_summary_rejects_non_ok_code() {
        // An HTTP 200 payload that carries a non-success `code` must be
        // treated as invalid (mirrors upstream `invalidCode`), so the caller
        // falls back to GetUserStatus.
        let groups = vec![serde_json::json!({
            "displayName": "Gemini Models",
            "buckets": [{
                "bucketId": "gemini-models-weekly-limit",
                "displayName": "Weekly Limit Remaining",
                "remainingFraction": 0.5,
            }]
        })];
        let bad_codes = [
            serde_json::json!(1),
            serde_json::json!(-1),
            serde_json::json!("error"),
            serde_json::json!("unauthorized"),
            serde_json::json!(true),
            serde_json::json!(1.5),
            serde_json::json!({"nested": 1}),
        ];
        for code in bad_codes {
            let json = serde_json::json!({
                "code": code,
                "message": "quota check failed",
                "response": { "description": "summary", "groups": groups },
            });
            let response: QuotaSummaryResponse = serde_json::from_value(json).unwrap();
            let result = AntigravityProvider::new().parse_quota_summary(response);
            assert!(result.is_err(), "expected rejection for code {code}");
        }
    }

    #[test]
    fn test_parse_quota_summary_accepts_ok_codes() {
        // `0`, `ok`, `success` and `0`-string codes (and an absent code, as
        // older servers omit it) all keep the summary usable.
        let groups = vec![serde_json::json!({
            "displayName": "Gemini Models",
            "buckets": [{
                "bucketId": "gemini-models-weekly-limit",
                "displayName": "Weekly Limit Remaining",
                "remainingFraction": 0.5,
            }]
        })];
        let ok_codes = [
            serde_json::json!(0),
            serde_json::json!("ok"),
            serde_json::json!("OK"),
            serde_json::json!("success"),
            serde_json::json!("0"),
        ];
        for code in ok_codes {
            let json = serde_json::json!({
                "code": code,
                "response": { "description": "summary", "groups": groups },
            });
            let response: QuotaSummaryResponse = serde_json::from_value(json).unwrap();
            let snap = AntigravityProvider::new().parse_quota_summary(response).unwrap();
            assert_eq!(snap.extra_rate_windows.len(), 1, "code {code}");
        }
        let json = serde_json::json!({
            "response": { "description": "summary", "groups": groups },
        });
        let response: QuotaSummaryResponse = serde_json::from_value(json).unwrap();
        let snap = AntigravityProvider::new().parse_quota_summary(response).unwrap();
        assert_eq!(snap.extra_rate_windows.len(), 1);
    }

    #[test]
    fn test_parse_quota_summary_remaining_oneof_case() {
        let json = serde_json::json!({
            "response": {
                "groups": [
                    {
                        "displayName": "Gemini Models",
                        "buckets": [
                            {
                                "bucketId": "gemini-models-weekly-limit",
                                "displayName": "Weekly Limit Remaining",
                                "remaining": { "case": "remainingFraction", "value": 0.65 },
                                "resetTime": "2026-08-17T00:00:00Z"
                            }
                        ]
                    }
                ]
            }
        });
        let response: QuotaSummaryResponse = serde_json::from_value(json).unwrap();
        let snap = AntigravityProvider::new().parse_quota_summary(response).unwrap();

        assert_eq!(snap.extra_rate_windows.len(), 1);
        // 0.65 remaining → 35% used.
        assert!((snap.extra_rate_windows[0].window.used_percent - 35.0).abs() < 0.1);
        assert!(snap.extra_rate_windows[0].usage_known);
    }

    #[test]
    fn test_quota_bucket_kind_aliases() {
        use serde_json::json;
        let session_ids = [
            "gemini-models-five-hour-limit",
            "gemini-models-5h",
            "session-limit",
            "five-hour",
        ];
        for bucket_id in session_ids {
            let bucket: QuotaSummaryBucketPayload = serde_json::from_value(json!({
                "bucketId": bucket_id,
                "displayName": "Weekly Limit Remaining",
            }))
            .unwrap();
            assert_eq!(
                quota_bucket_kind(&bucket),
                QuotaBucketKind::Session,
                "{bucket_id}"
            );
        }

        let weekly: QuotaSummaryBucketPayload = serde_json::from_value(json!({
            "bucketId": "gemini-models-weekly-limit",
            "displayName": "Weekly Limit Remaining",
        }))
        .unwrap();
        assert_eq!(quota_bucket_kind(&weekly), QuotaBucketKind::Weekly);
    }

    #[test]
    fn test_group_display_titles_normalize_families() {
        use serde_json::json;
        let gemini: QuotaSummaryGroupPayload = serde_json::from_value(json!({
            "displayName": "Gemini Models",
            "buckets": [],
        }))
        .unwrap();
        assert_eq!(group_display_title(&gemini), "Gemini");

        let claude: QuotaSummaryGroupPayload = serde_json::from_value(json!({
            "displayName": "Claude and GPT models",
            "buckets": [],
        }))
        .unwrap();
        assert_eq!(group_display_title(&claude), "Claude/GPT");
    }

    #[test]
    fn not_running_error_tells_user_how_to_start() {
        let error = ProviderError::NotInstalled(NOT_RUNNING_MESSAGE.to_string()).to_string();

        assert!(error.contains("Start Google Antigravity and sign in"));
    }
}