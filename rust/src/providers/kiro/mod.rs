//! Kiro provider implementation
//!
//! Fetches usage data from Kiro (Amazon's AI coding assistant)
//! Uses kiro-cli for authentication and usage fetching

pub mod version;

// Re-exports for version compatibility checking
#[allow(unused_imports)]
pub use version::{
    KiroVersion, detect_version, find_kiro_cli, get_version, is_compatible, is_installed,
};

use async_trait::async_trait;
use chrono::Datelike;
use regex_lite::Regex;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

/// Kiro provider (AWS AI assistant)
pub struct KiroProvider {
    metadata: ProviderMetadata,
}

struct KiroCliUsage {
    plan_name: String,
    matched_new_format: bool,
    is_managed_plan: bool,
    reset_date: Option<chrono::DateTime<chrono::Utc>>,
    credits_percent: f64,
    matched_percent: bool,
    matched_credits: bool,
    bonus_window: Option<RateWindow>,
    overages_enabled: bool,
    overage_credits_used: Option<f64>,
    estimated_overage_cost: Option<f64>,
}

/// `kiro-cli chat --no-interactive <prompt>` is a real model turn, so the
/// `/usage` probe must be pinned to the v2 agent engine. With the default v3
/// engine (Kiro upgrades automatically) the CLI treats `/usage` as a normal
/// prompt, answers with prose and bills one model turn on every refresh.
const AGENT_ENGINE_V2: &str = "--agent-engine=v2";
const AGENT_ENGINE_V1: &str = "--agent-engine=v1";
const LEGACY_UI: &str = "--legacy-ui";

/// Hard cap for a single probe spawn so a wedged CLI cannot hang a refresh.
const USAGE_TIMEOUT: Duration = Duration::from_secs(15);

/// Raw result of one `/usage` probe spawn.
struct KiroCliAttempt {
    stdout: String,
    stderr: String,
    success: bool,
}

impl KiroCliAttempt {
    /// CLI diagnostics use whichever stream is non-empty (stderr first).
    fn text(&self) -> &str {
        if self.stdout.trim().is_empty() {
            self.stderr.as_str()
        } else {
            self.stdout.as_str()
        }
    }
}

impl KiroProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::Kiro,
                display_name: "Kiro",
                session_label: "Session",
                weekly_label: "Monthly",
                supports_opus: false,
                supports_credits: true,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://kiro.dev/account"),
                status_page_url: Some("https://health.aws.amazon.com"),
            },
        }
    }

    /// Get Kiro config directory
    fn get_kiro_config_path() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            dirs::config_dir().map(|p| p.join("Kiro"))
        }
        #[cfg(not(target_os = "windows"))]
        {
            dirs::home_dir().map(|p| p.join(".kiro"))
        }
    }

    /// Find Kiro CLI binary
    fn which_kiro() -> Option<PathBuf> {
        version::find_kiro_cli()
    }

    /// Check if user is logged in by running `kiro-cli whoami`
    async fn ensure_logged_in(&self) -> Result<(), ProviderError> {
        let cli_path = Self::which_kiro().ok_or_else(|| {
            ProviderError::NotInstalled(
                "kiro-cli not found. Install from https://kiro.dev".to_string(),
            )
        })?;

        #[cfg(windows)]
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = Command::new(&cli_path);
        // `whoami` is a plain auth probe: it must not inherit a stdin that may
        // never close, and it must not outlive the probe on timeout.
        cmd.arg("whoami")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let output = timeout(USAGE_TIMEOUT, cmd.output())
            .await
            .map_err(|_| ProviderError::Timeout)?
            .map_err(|e| ProviderError::Other(format!("Failed to run kiro-cli: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_lowercase();
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        let combined = format!("{} {}", stdout, stderr);

        if combined.contains("not logged in") || combined.contains("login required") {
            return Err(ProviderError::AuthRequired);
        }

        if !output.status.success() {
            return Err(ProviderError::Other(format!(
                "kiro-cli whoami failed with status {}",
                output.status.code().unwrap_or(-1)
            )));
        }

        Ok(())
    }

    /// Fetch usage via kiro-cli
    async fn fetch_via_cli(&self) -> Result<UsageSnapshot, ProviderError> {
        // First ensure we're logged in
        self.ensure_logged_in().await?;

        let cli_path = Self::which_kiro()
            .ok_or_else(|| ProviderError::NotInstalled("kiro-cli not found".to_string()))?;

        let attempt = Self::run_usage_command(&cli_path, &Self::usage_args(Some("v2"))).await?;

        // Some CLI builds reject the pin itself; fall back to the unpinned probe
        // only when the CLI said so explicitly.
        let attempt =
            if Self::should_retry_without_engine(&attempt.stdout, &attempt.stderr, attempt.success)
            {
                Self::run_usage_command(&cli_path, &Self::usage_args(None)).await?
            } else {
                attempt
            };

        // A plan-only v2 summary carries no quota numbers; the legacy UI is the
        // only shape that still prints them. Probe it once and accept whatever
        // it returns (at most three spawns, never a loop).
        let attempt = if Self::is_v2_plan_only_summary(attempt.text()) {
            Self::run_usage_command(&cli_path, &Self::usage_args(Some("v1"))).await?
        } else {
            attempt
        };

        self.parse_cli_attempt(&attempt)
    }

    /// Arguments for one `/usage` probe, in upstream order.
    fn usage_args(engine: Option<&str>) -> Vec<&'static str> {
        let mut args = vec!["chat", "--no-interactive"];
        match engine {
            Some("v2") => args.push(AGENT_ENGINE_V2),
            Some("v1") => {
                args.push(AGENT_ENGINE_V1);
                args.push(LEGACY_UI);
            }
            _ => {}
        }
        args.push("/usage");
        args
    }

    /// True only when the CLI explicitly rejected the engine pin, so a retry
    /// without it can succeed. Generic failures, auth errors and v3 prose are
    /// not grounds to probe again (each spawn can bill a model turn).
    fn should_retry_without_engine(stdout: &str, stderr: &str, success: bool) -> bool {
        if success {
            return false;
        }

        let combined = format!("{stdout}\n{stderr}").to_lowercase();
        let rejected_flag = [
            "error: unexpected argument",
            "unrecognized option",
            "unknown option",
        ]
        .iter()
        .any(|marker| combined.contains(marker));

        (rejected_flag && combined.contains("agent-engine"))
            || combined.contains("--legacy-ui cannot be used with --agent-engine=v2")
    }

    /// A v2-only summary: `Plan: ... | N usage breakdowns` with no per-window
    /// numbers. ANSI is stripped before matching.
    fn is_v2_plan_only_summary(text: &str) -> bool {
        let stripped = Self::strip_ansi(text);
        let trimmed = stripped.trim();
        if trimmed.is_empty() {
            return false;
        }

        let has_plan_summary = Regex::new(r"^Plan:.*\|[ \t]*\d+[ \t]+usage breakdowns?$")
            .ok()
            .is_some_and(|re| trimmed.lines().any(|line| re.is_match(line.trim())));
        if !has_plan_summary {
            return false;
        }

        let has_percent_bar = Regex::new(r"█+\s*\d+(?:\.\d+)?%")
            .ok()
            .is_some_and(|re| re.is_match(&stripped));
        let has_covered_usage = Regex::new(r"\(\s*\d+\.?\d*\s+of\s+\d+\s+covered")
            .ok()
            .is_some_and(|re| re.is_match(&stripped));

        !has_percent_bar && !has_covered_usage
    }

    /// Run one probe spawn with a hard timeout and no inherited stdin.
    ///
    /// `kill_on_drop` is required: on timeout the `output()` future is dropped,
    /// and without it the child would keep running (and keep billing a model
    /// turn for `chat --no-interactive`) after we already reported a timeout.
    async fn run_usage_command(
        cli_path: &Path,
        args: &[&str],
    ) -> Result<KiroCliAttempt, ProviderError> {
        #[cfg(windows)]
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let mut cmd = Command::new(cli_path);
        cmd.args(args)
            .env("TERM", "xterm-256color")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);

        let output = timeout(USAGE_TIMEOUT, cmd.output())
            .await
            .map_err(|_| ProviderError::Timeout)?
            .map_err(|e| ProviderError::Other(format!("Failed to run kiro-cli: {}", e)))?;

        Ok(KiroCliAttempt {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            success: output.status.success(),
        })
    }

    /// Classify auth errors, then parse whatever the accepted probe printed.
    fn parse_cli_attempt(&self, attempt: &KiroCliAttempt) -> Result<UsageSnapshot, ProviderError> {
        let combined = attempt.text();

        // Check for login errors
        let lowered = combined.to_lowercase();
        if lowered.contains("not logged in")
            || lowered.contains("login required")
            || lowered.contains("failed to initialize auth portal")
            || lowered.contains("kiro-cli login")
            || lowered.contains("oauth error")
        {
            return Err(ProviderError::AuthRequired);
        }

        self.parse_cli_output(combined)
    }

    /// Parse CLI output to extract usage information
    fn parse_cli_output(&self, output: &str) -> Result<UsageSnapshot, ProviderError> {
        let stripped = Self::strip_ansi(output);
        let trimmed = stripped.trim();
        Self::validate_cli_output(trimmed, &stripped)?;
        let lowered = stripped.to_lowercase();
        let parsed = Self::parse_usage_fields(&stripped, &lowered);

        if let Some(usage) = Self::usage_without_metrics(&parsed) {
            return Ok(usage);
        }

        let mut usage = Self::usage_with_metrics(&parsed);
        usage = Self::apply_overage_windows(usage, &parsed);

        if let Some(bonus) = parsed.bonus_window {
            usage = usage.with_secondary(bonus);
        }

        Ok(usage)
    }

    fn validate_cli_output(trimmed: &str, stripped: &str) -> Result<(), ProviderError> {
        if trimmed.is_empty() {
            return Err(ProviderError::Parse(
                "Empty output from kiro-cli".to_string(),
            ));
        }

        if stripped
            .to_lowercase()
            .contains("could not retrieve usage information")
        {
            return Err(ProviderError::Parse(
                "Kiro CLI could not retrieve usage information".to_string(),
            ));
        }

        Ok(())
    }

    fn usage_without_metrics(parsed: &KiroCliUsage) -> Option<UsageSnapshot> {
        if parsed.matched_percent || parsed.matched_credits {
            return None;
        }

        let method = if parsed.matched_new_format || parsed.plan_name != "Kiro" {
            parsed.plan_name.as_str()
        } else {
            "Kiro (installed)"
        };

        Some(
            UsageSnapshot::new(RateWindow::informational("Kiro usage details unavailable"))
                .with_login_method(method),
        )
    }

    fn usage_with_metrics(parsed: &KiroCliUsage) -> UsageSnapshot {
        let primary =
            RateWindow::with_details(parsed.credits_percent, None, parsed.reset_date, None);
        UsageSnapshot::new(primary).with_login_method(&parsed.plan_name)
    }

    fn parse_usage_fields(stripped: &str, lowered: &str) -> KiroCliUsage {
        let (plan_name, matched_new_format) = Self::parse_plan_name(stripped);
        let (credits_percent, matched_percent, matched_credits) =
            Self::parse_credit_usage(stripped);
        let (overages_enabled, overage_credits_used, estimated_overage_cost) =
            Self::parse_overages(stripped);

        KiroCliUsage {
            plan_name,
            matched_new_format,
            is_managed_plan: lowered.contains("managed by admin")
                || lowered.contains("managed by organization"),
            reset_date: Self::capture_text(stripped, r"resets on (\d{4}-\d{2}-\d{2}|\d{2}/\d{2})")
                .and_then(|date| Self::parse_reset_date_at(&date, chrono::Utc::now())),
            credits_percent,
            matched_percent,
            matched_credits,
            bonus_window: Self::parse_bonus_window(stripped),
            overages_enabled,
            overage_credits_used,
            estimated_overage_cost,
        }
    }

    fn parse_plan_name(stripped: &str) -> (String, bool) {
        if let Some(plan_line) = Self::capture_text(stripped, r"Plan:\s*(.+)")
            && let Some(first_line) = plan_line.lines().next()
        {
            return (first_line.trim().to_string(), true);
        }

        let legacy = Self::capture_text(stripped, r"\|\s*(KIRO\s+\w+)")
            .unwrap_or_else(|| "Kiro".to_string());
        (legacy, false)
    }

    fn parse_credit_usage(stripped: &str) -> (f64, bool, bool) {
        if let Some(percent) = Self::capture_number(stripped, r"█+\s*(\d+)%") {
            return (percent, true, false);
        }

        let Some((used, total)) =
            Self::capture_number_pair(stripped, r"\((\d+\.?\d*)\s+of\s+(\d+)\s+covered")
        else {
            return (0.0, false, false);
        };

        let percent = if total > 0.0 {
            (used / total) * 100.0
        } else {
            0.0
        };
        (percent, false, true)
    }

    fn parse_bonus_window(stripped: &str) -> Option<RateWindow> {
        let (used, total) =
            Self::capture_number_pair(stripped, r"Bonus credits:\s*(\d+\.?\d*)/(\d+)")?;
        if total <= 0.0 {
            return None;
        }

        let expiry_desc = Self::capture_text(stripped, r"expires in (\d+) days?")
            .map(|days| format!("expires in {days}d"));
        Some(RateWindow::with_details(
            (used / total) * 100.0,
            None,
            None,
            expiry_desc,
        ))
    }

    fn parse_overages(stripped: &str) -> (bool, Option<f64>, Option<f64>) {
        let enabled = Self::capture_text(stripped, r"(?i)Overages:\s*([^\n]+)")
            .is_some_and(|value| value.to_lowercase().starts_with("enabled"));
        let credits_used = Self::capture_number(stripped, r"(?i)Credits used:\s*(\d+\.?\d*)");
        let estimated_cost =
            Self::capture_number(stripped, r"(?i)Est\.\s*cost:\s*\$?(\d+\.?\d*)\s*USD");
        (enabled, credits_used, estimated_cost)
    }

    fn apply_overage_windows(mut usage: UsageSnapshot, parsed: &KiroCliUsage) -> UsageSnapshot {
        if !parsed.overages_enabled {
            return usage;
        }

        if let Some(credits) = parsed.overage_credits_used {
            usage = usage.with_extra_rate_window(
                "kiro-overage-credits",
                "Overage usage",
                RateWindow::informational(format!("{credits:.2} credits")),
            );
        }
        if let Some(cost) = parsed.estimated_overage_cost {
            usage = usage.with_extra_rate_window(
                "kiro-overage-cost",
                "Overage cost",
                RateWindow::informational(format!("${cost:.2} USD")),
            );
        }

        usage
    }

    fn capture_text(text: &str, pattern: &str) -> Option<String> {
        Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(text))
            .and_then(|caps| caps.get(1).map(|m| m.as_str().trim().to_string()))
    }

    fn capture_number(text: &str, pattern: &str) -> Option<f64> {
        Self::capture_text(text, pattern)?.parse().ok()
    }

    fn capture_number_pair(text: &str, pattern: &str) -> Option<(f64, f64)> {
        Regex::new(pattern)
            .ok()
            .and_then(|re| re.captures(text))
            .and_then(|caps| {
                Some((
                    caps.get(1)?.as_str().parse().ok()?,
                    caps.get(2)?.as_str().parse().ok()?,
                ))
            })
    }

    /// Strip ANSI escape sequences from text
    fn strip_ansi(text: &str) -> String {
        // Simple ANSI stripping - remove escape sequences
        let mut result = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\x1B' {
                // Skip escape sequence
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                    // Skip until we hit a letter
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                } else if chars.peek() == Some(&']') {
                    // OSC sequence - skip until BEL or ST
                    for next in chars.by_ref() {
                        if next == '\x07' || next == '\\' {
                            break;
                        }
                    }
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// Parse a reset date, either `YYYY-MM-DD` (v2 builds, read as UTC
    /// midnight on that day) or the legacy `MM/DD` form, which rolls into next
    /// year when the current year's date is already past.
    fn parse_reset_date_at(
        date_str: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        let trimmed = date_str.trim();

        if let Some(date) = Self::parse_iso_date(trimmed) {
            let datetime = date.and_hms_opt(0, 0, 0)?;
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                datetime,
                chrono::Utc,
            ));
        }

        let parts: Vec<&str> = trimmed.split('/').collect();
        if parts.len() != 2 {
            return None;
        }

        let month: u32 = parts[0].parse().ok()?;
        let day: u32 = parts[1].parse().ok()?;

        let current_year = now.year();

        // Try current year first
        if let Some(date) = chrono::NaiveDate::from_ymd_opt(current_year, month, day) {
            let datetime = date.and_hms_opt(0, 0, 0)?;
            let utc =
                chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(datetime, chrono::Utc);
            if utc > now {
                return Some(utc);
            }
        }

        // If in the past, use next year
        if let Some(date) = chrono::NaiveDate::from_ymd_opt(current_year + 1, month, day) {
            let datetime = date.and_hms_opt(0, 0, 0)?;
            return Some(chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
                datetime,
                chrono::Utc,
            ));
        }

        None
    }

    fn parse_iso_date(value: &str) -> Option<chrono::NaiveDate> {
        let parts: Vec<&str> = value.split('-').collect();
        if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
            return None;
        }

        let year: i32 = parts[0].parse().ok()?;
        let month: u32 = parts[1].parse().ok()?;
        let day: u32 = parts[2].parse().ok()?;
        chrono::NaiveDate::from_ymd_opt(year, month, day)
    }
}

impl Default for KiroProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for KiroProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Kiro
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        tracing::debug!("Fetching Kiro usage");

        match ctx.source_mode {
            SourceMode::Auto | SourceMode::Cli => {
                let usage = self.fetch_via_cli().await?;
                Ok(ProviderFetchResult::new(usage, "cli"))
            }
            SourceMode::Web => {
                // Kiro doesn't have a direct web API, use CLI
                let usage = self.fetch_via_cli().await?;
                Ok(ProviderFetchResult::new(usage, "cli"))
            }
            SourceMode::OAuth => Err(ProviderError::UnsupportedSource(SourceMode::OAuth)),
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Cli]
    }

    fn supports_web(&self) -> bool {
        false
    }

    fn supports_cli(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn provider() -> KiroProvider {
        KiroProvider::new()
    }

    fn utc(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, hh, mm, 0).unwrap()
    }

    fn extra_window<'a>(usage: &'a UsageSnapshot, id: &str) -> &'a RateWindow {
        &usage
            .extra_rate_windows
            .iter()
            .find(|extra| extra.id == id)
            .unwrap_or_else(|| panic!("missing extra rate window {id}"))
            .window
    }

    #[test]
    fn parse_cli_output_reads_v2_rounded_credits_and_iso_reset() {
        // v2 shape (kiro-cli 2.37): covered credits instead of a bar, ISO reset.
        let output = "Estimated Usage | resets on 2026-12-01 | KIRO STUDENT\n\
                      Credits (16.27 of 1000 covered in plan), 1.6%\n";

        let usage = provider()
            .parse_cli_output(output)
            .expect("v2 output parses");

        assert!(!usage.primary.is_informational);
        assert!(
            (usage.primary.used_percent - 1.627).abs() < 1e-6,
            "expected 16.27/1000 as a percentage, got {}",
            usage.primary.used_percent
        );
        assert_eq!(format!("{:.1}", usage.primary.used_percent), "1.6");
        assert_eq!(usage.primary.resets_at, Some(utc(2026, 12, 1, 0, 0)));
        assert_eq!(usage.login_method.as_deref(), Some("KIRO STUDENT"));
        assert!(usage.secondary.is_none());
    }

    #[test]
    fn parse_cli_output_reads_legacy_plan_and_percent_bar() {
        let output = "Plan: Q Developer Pro\n████ 42%\n";

        let usage = provider()
            .parse_cli_output(output)
            .expect("legacy output parses");

        assert!(!usage.primary.is_informational);
        assert!((usage.primary.used_percent - 42.0).abs() < 1e-6);
        assert_eq!(usage.login_method.as_deref(), Some("Q Developer Pro"));
    }

    #[test]
    fn parse_cli_output_keeps_bonus_and_overage_rows_informational() {
        let output = "Plan: Kiro Pro\n\
                      ██████ 42%\n\
                      Bonus credits: 5/10 (expires in 3 days)\n\
                      Overages: enabled\n\
                      Credits used: 2.50\n\
                      Est. cost: $1.25 USD\n";

        let usage = provider()
            .parse_cli_output(output)
            .expect("overage output parses");

        assert!((usage.primary.used_percent - 42.0).abs() < 1e-6);

        let bonus = usage.secondary.as_ref().expect("bonus window");
        assert!((bonus.used_percent - 50.0).abs() < 1e-6);
        assert_eq!(bonus.reset_description.as_deref(), Some("expires in 3d"));

        let credits = extra_window(&usage, "kiro-overage-credits");
        assert!(credits.is_informational, "overage credits is not a quota");
        assert_eq!(credits.used_percent, 0.0);
        assert_eq!(credits.reset_description.as_deref(), Some("2.50 credits"));

        let cost = extra_window(&usage, "kiro-overage-cost");
        assert!(cost.is_informational, "overage cost is not a quota");
        assert_eq!(cost.used_percent, 0.0);
        assert_eq!(cost.reset_description.as_deref(), Some("$1.25 USD"));
    }

    #[test]
    fn parse_cli_output_marks_plan_without_metrics_as_informational() {
        let output = "Plan: Managed by admin\nThis account is managed by your organization.\n";

        let usage = provider()
            .parse_cli_output(output)
            .expect("managed plan output parses");

        assert!(
            usage.primary.is_informational,
            "a plan without quota numbers must not report a 0% quota"
        );
        assert_eq!(usage.primary.used_percent, 0.0);
        assert!(!usage.primary.is_exhausted());
        assert_eq!(
            usage.primary.reset_description.as_deref(),
            Some("Kiro usage details unavailable")
        );
        assert_eq!(usage.login_method.as_deref(), Some("Managed by admin"));
    }

    #[test]
    fn parse_cli_output_rejects_empty_and_unavailable_output() {
        assert!(matches!(
            provider().parse_cli_output("   \n\t "),
            Err(ProviderError::Parse(_))
        ));
        assert!(matches!(
            provider().parse_cli_output("Could not retrieve usage information"),
            Err(ProviderError::Parse(_))
        ));
    }

    #[test]
    fn parse_reset_date_at_rolls_month_day_into_next_year_when_past() {
        // Future month/day in the current year.
        assert_eq!(
            KiroProvider::parse_reset_date_at("12/01", utc(2026, 11, 1, 12, 0)),
            Some(utc(2026, 12, 1, 0, 0))
        );

        // Already past this year -> next year.
        assert_eq!(
            KiroProvider::parse_reset_date_at("12/01", utc(2026, 12, 15, 9, 0)),
            Some(utc(2027, 12, 1, 0, 0))
        );
    }

    #[test]
    fn parse_reset_date_at_reads_iso_dates_at_utc_midnight() {
        assert_eq!(
            KiroProvider::parse_reset_date_at("2026-12-01", utc(2026, 11, 2, 8, 0)),
            Some(utc(2026, 12, 1, 0, 0))
        );

        // A stale ISO date is reported as written, without the MM/DD roll.
        assert_eq!(
            KiroProvider::parse_reset_date_at("2026-12-01", utc(2027, 6, 1, 8, 0)),
            Some(utc(2026, 12, 1, 0, 0))
        );
    }

    #[test]
    fn parse_reset_date_at_rejects_unparsable_dates() {
        let now = utc(2026, 11, 2, 8, 0);

        assert_eq!(KiroProvider::parse_reset_date_at("", now), None);
        assert_eq!(KiroProvider::parse_reset_date_at("not-a-date", now), None);
        assert_eq!(KiroProvider::parse_reset_date_at("13/45", now), None);
        assert_eq!(KiroProvider::parse_reset_date_at("2026-13-01", now), None);
        assert_eq!(KiroProvider::parse_reset_date_at("12/01/2026", now), None);
    }

    #[test]
    fn usage_args_pin_the_engine_and_keep_upstream_order() {
        assert_eq!(
            KiroProvider::usage_args(Some("v2")),
            vec!["chat", "--no-interactive", "--agent-engine=v2", "/usage"]
        );
        assert_eq!(
            KiroProvider::usage_args(Some("v1")),
            vec![
                "chat",
                "--no-interactive",
                "--agent-engine=v1",
                "--legacy-ui",
                "/usage"
            ]
        );
        assert_eq!(
            KiroProvider::usage_args(None),
            vec!["chat", "--no-interactive", "/usage"]
        );
    }

    #[test]
    fn should_retry_without_engine_only_on_explicit_flag_rejection() {
        // Explicit "unknown flag for --agent-engine" from the CLI.
        assert!(KiroProvider::should_retry_without_engine(
            "",
            "error: unexpected argument '--agent-engine' found\n\nUsage: kiro-cli chat [OPTIONS]\n",
            false
        ));
        assert!(KiroProvider::should_retry_without_engine(
            "",
            "error: unrecognized option '--agent-engine=v2'\n",
            false
        ));
        assert!(KiroProvider::should_retry_without_engine(
            "",
            "error: the argument '--legacy-ui cannot be used with --agent-engine=v2'\n",
            false
        ));

        // Same diagnostics are irrelevant when the probe succeeded.
        assert!(!KiroProvider::should_retry_without_engine(
            "",
            "error: unexpected argument '--agent-engine' found\n",
            true
        ));

        // Unrelated failures must not spawn another (billable) probe.
        assert!(!KiroProvider::should_retry_without_engine(
            "",
            "error: failed to connect to the usage service\n",
            false
        ));

        // v3 prose answer: the prompt was accepted, nothing to retry.
        assert!(!KiroProvider::should_retry_without_engine(
            "Here is what I found about your Kiro usage: you have plenty of credits left.\n",
            "",
            true
        ));
        assert!(!KiroProvider::should_retry_without_engine(
            "I cannot run /usage from here, but here is a summary.\n",
            "",
            false
        ));
    }

    #[test]
    fn is_v2_plan_only_summary_needs_the_plan_line_and_no_numbers() {
        assert!(KiroProvider::is_v2_plan_only_summary(
            "Plan: Q Developer Pro | 4 usage breakdowns\n"
        ));
        // ANSI colouring is stripped before matching.
        assert!(KiroProvider::is_v2_plan_only_summary(
            "\u{1b}[1mPlan: Q Developer Pro | 4 usage breakdowns\u{1b}[0m\n"
        ));

        // A percent bar means real numbers are present.
        assert!(!KiroProvider::is_v2_plan_only_summary(
            "Plan: Q Developer Pro | 4 usage breakdowns\n████ 42%\n"
        ));
        // Covered credits mean real numbers are present.
        assert!(!KiroProvider::is_v2_plan_only_summary(
            "Plan: Q Developer Pro | 4 usage breakdowns\nCredits (16.27 of 1000 covered in plan)\n"
        ));

        assert!(!KiroProvider::is_v2_plan_only_summary(""));
        assert!(!KiroProvider::is_v2_plan_only_summary(
            "Sure, here is a summary of your usage.\n"
        ));
    }

    #[test]
    fn plan_only_summary_is_not_printed_as_a_zero_quota() {
        // The intermediate shape must still parse into an informational primary
        // rather than a 0% quota, so a failed v1 fallback never fakes headroom.
        let usage = provider()
            .parse_cli_output("Plan: Q Developer Pro | 4 usage breakdowns\n")
            .expect("plan-only summary parses");

        assert!(usage.primary.is_informational);
        assert_eq!(usage.primary.used_percent, 0.0);
    }
}
