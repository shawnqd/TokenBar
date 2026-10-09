//! Signed-in installed `agy` CLI fallback for the Antigravity provider.
//!
//! Antigravity's Local API source needs the desktop language server to be
//! running. This module reads the same quota summary through the installed
//! `agy` CLI (`agy -p /usage --output-format json --print-timeout 90s`),
//! which keeps working while the desktop app is closed and uses the CLI's own
//! signed-in session.
//!
//! Robustness and security rules:
//! - the child runs in a private temporary working directory with
//!   `ANTIGRAVITY_OAUTH_CREDENTIALS_JSON` removed, so it can only use its own
//!   persisted sign-in;
//! - stdin and stderr are null; stdout is read with a hard byte bound;
//! - the whole child is bounded by a wall-clock timeout and `kill_on_drop`,
//!   so a hung CLI never leaves an orphan behind;
//! - raw CLI output, messages and credential values are never logged or
//!   embedded in errors; only sanitized status/command names and version
//!   numbers ever reach the caller.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;

use regex_lite::Regex;
use serde::Deserialize;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::timeout;

use crate::core::{ProviderError, ProviderFetchResult, UsageSnapshot};

use super::{QuotaSummaryGroupPayload, QuotaSummaryPayload, quota_summary_from_payload};

/// Minimum `agy` version whose JSON envelope carries the structured `/usage`
/// report. Older CLIs answer the prompt as plain text instead.
pub(crate) const MIN_AGY_VERSION: (u64, u64, u64) = (1, 1, 11);

/// Explicit binary overrides, checked in order. An override that does not
/// point at a file fails honestly instead of silently using another binary.
const AGY_ENV_OVERRIDES: [&str; 2] = ["ANTIGRAVITY_AGY_PATH", "AGY_PATH"];

/// Credential env injected by some hosts; the CLI must use its own saved
/// session, never an inherited raw credential blob.
const AGY_ENV_TO_SCRUB: &str = "ANTIGRAVITY_OAUTH_CREDENTIALS_JSON";

/// The CLI's own print timeout is 90s; the parent waits a little longer
/// before killing the child.
pub(super) const USAGE_TIMEOUT: Duration = Duration::from_secs(120);
/// Bound for the lightweight `--version` probe.
pub(super) const VERSION_TIMEOUT: Duration = Duration::from_secs(15);
/// A structured usage report is a few KiB; anything larger is not trusted.
const MAX_STDOUT_BYTES: usize = 4 * 1024 * 1024;

const NOT_INSTALLED_MESSAGE: &str = "agy CLI not found. Install the Antigravity CLI and sign in with `agy`, or keep Google Antigravity running.";

/// Resolve the `agy` binary: explicit override, then `PATH`, then standard
/// Windows install locations (including Codex-managed CLI installs).
pub(crate) fn resolve_agy_binary() -> Option<PathBuf> {
    let override_path = AGY_ENV_OVERRIDES
        .iter()
        .filter_map(|name| std::env::var_os(name))
        .map(PathBuf::from)
        .find(|path| !path.as_os_str().is_empty());
    resolve_agy_binary_with(override_path.as_deref())
}

fn resolve_agy_binary_with(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = override_path {
        // An explicit override is authoritative: when it is not a file we do
        // not fall back to a different agy, so a misconfigured override cannot
        // silently read another account.
        return path.is_file().then(|| path.to_path_buf());
    }
    if let Ok(path) = which::which("agy") {
        return Some(path);
    }
    standard_agy_locations()
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
}

#[cfg(windows)]
fn standard_agy_locations() -> Vec<Option<PathBuf>> {
    let mut candidates = Vec::new();
    if let Some(local) = dirs::data_local_dir() {
        candidates.push(Some(
            local.join("Programs").join("Antigravity").join("agy.exe"),
        ));
        candidates.push(Some(local.join("Programs").join("agy").join("agy.exe")));
        candidates.push(Some(
            local
                .join("Programs")
                .join("Antigravity")
                .join("resources")
                .join("bin")
                .join("agy.exe"),
        ));
        candidates.push(Some(local.join("Antigravity").join("bin").join("agy.exe")));
        candidates.push(Some(local.join("agy").join("bin").join("agy.exe")));
        // npm global shims land under %LOCALAPPDATA%\npm in this shell.
        candidates.push(Some(local.join("npm").join("agy.cmd")));
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(Some(home.join(".codex").join("bin").join("agy.exe")));
        candidates.push(Some(home.join(".antigravity").join("bin").join("agy.exe")));
        candidates.push(Some(
            home.join("AppData")
                .join("Roaming")
                .join("npm")
                .join("agy.cmd"),
        ));
    }
    candidates
}

#[cfg(not(windows))]
fn standard_agy_locations() -> Vec<Option<PathBuf>> {
    // The Windows product line resolves agy through the override or PATH.
    Vec::new()
}

/// Fetch the quota snapshot through the installed, signed-in `agy` CLI.
pub(crate) async fn fetch_cli_usage() -> Result<ProviderFetchResult, ProviderError> {
    let binary = resolve_agy_binary()
        .ok_or_else(|| ProviderError::NotInstalled(NOT_INSTALLED_MESSAGE.to_string()))?;
    ensure_supported_agy_version(&binary).await?;
    let output = run_agy(
        &binary,
        &[
            "-p",
            "/usage",
            "--output-format",
            "json",
            "--print-timeout",
            "90s",
        ],
        USAGE_TIMEOUT,
    )
    .await?;
    let snapshot = parse_cli_usage_report(&output)?;
    Ok(ProviderFetchResult::new(snapshot, "cli"))
}

/// A completed bounded `agy` invocation. `stdout` is never logged or shown.
struct AgyOutput {
    stdout: Vec<u8>,
    exit_code: Option<i32>,
    success: bool,
}

/// Run `agy` with a private temporary working directory, a scrubbed
/// environment, null stdin/stderr, bounded stdout and a wall-clock timeout.
async fn run_agy(binary: &Path, args: &[&str], wall: Duration) -> Result<AgyOutput, ProviderError> {
    let workdir = PrivateTempWorkdir::create()?;

    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(workdir.path())
        .env_remove(AGY_ENV_TO_SCRUB)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = command
        .spawn()
        .map_err(|_| ProviderError::Other("Failed to launch the agy CLI".to_string()))?;
    let stdout = child.stdout.take();

    let run = async {
        let (read_result, status_result) = tokio::join!(read_bounded_stdout(stdout), child.wait());
        let stdout = read_result?;
        let status = status_result.map_err(|_| {
            ProviderError::Other("Failed while waiting for the agy CLI".to_string())
        })?;
        Ok(AgyOutput {
            stdout,
            exit_code: status.code(),
            success: status.success(),
        })
    };

    // On timeout the future (and the child with it) is dropped; `kill_on_drop`
    // terminates the process so no orphan keeps running.
    timeout(wall, run)
        .await
        .map_err(|_| ProviderError::Timeout)?
}

/// Drain the child's stdout, refusing to buffer more than `MAX_STDOUT_BYTES`.
async fn read_bounded_stdout(
    stdout: Option<tokio::process::ChildStdout>,
) -> Result<Vec<u8>, ProviderError> {
    let Some(mut stdout) = stdout else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    // Heap-allocated chunk on purpose: a fixed `[u8; 64 * 1024]` array lives
    // inside this async fn's state machine, and the debug-build `join!` +
    // `timeout` layers duplicate that state enough to overflow the Windows
    // CLI main thread's ~1 MiB stack (0xC00000FD). A `Vec` keeps only a
    // pointer in the future state.
    let mut chunk = vec![0u8; 8 * 1024];
    loop {
        let read = stdout
            .read(&mut chunk)
            .await
            .map_err(|_| ProviderError::Other("Failed to read agy CLI output".to_string()))?;
        if read == 0 {
            return Ok(out);
        }
        if out.len() + read > MAX_STDOUT_BYTES {
            return Err(ProviderError::Other(
                "agy CLI output exceeded the size bound".to_string(),
            ));
        }
        out.extend_from_slice(&chunk[..read]);
    }
}

/// A random per-invocation directory under `%TEMP%`, removed on drop. The
/// random name keeps concurrent or stale invocations from sharing a cwd.
struct PrivateTempWorkdir {
    path: PathBuf,
}

impl PrivateTempWorkdir {
    fn create() -> Result<Self, ProviderError> {
        let path = std::env::temp_dir().join(format!("codexbar-agy-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).map_err(|_| {
            ProviderError::Other("Failed to create the agy CLI work directory".to_string())
        })?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateTempWorkdir {
    fn drop(&mut self) {
        // Best effort: the OS cleans %TEMP%, and a killed child can still
        // hold the directory handle on Windows.
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Probe `agy --version` and reject versions whose JSON envelope predates the
/// structured `/usage` report. A failed or unparseable probe is not fatal on
/// its own: the usage command itself still fails honestly if unsupported.
async fn ensure_supported_agy_version(binary: &Path) -> Result<(), ProviderError> {
    let Ok(output) = run_agy(binary, &["--version"], VERSION_TIMEOUT).await else {
        tracing::debug!("Antigravity agy version probe did not complete");
        return Ok(());
    };
    if !output.success {
        return Ok(());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    match parse_agy_version(&text) {
        Some(version) if !agy_version_supported(version) => Err(ProviderError::Other(format!(
            "agy {}.{}.{} does not support structured quota reports; update the Antigravity CLI to {}.{}.{} or newer",
            version.0,
            version.1,
            version.2,
            MIN_AGY_VERSION.0,
            MIN_AGY_VERSION.1,
            MIN_AGY_VERSION.2
        ))),
        _ => Ok(()),
    }
}

/// Parse the first `major.minor.patch` version from `agy --version` output.
pub(crate) fn parse_agy_version(output: &str) -> Option<(u64, u64, u64)> {
    static VERSION_RE: OnceLock<Regex> = OnceLock::new();
    let regex = VERSION_RE
        .get_or_init(|| Regex::new(r"(\d+)\.(\d+)\.(\d+)").expect("valid agy version pattern"));
    let caps = regex.captures(output)?;
    Some((
        caps.get(1)?.as_str().parse().ok()?,
        caps.get(2)?.as_str().parse().ok()?,
        caps.get(3)?.as_str().parse().ok()?,
    ))
}

fn agy_version_supported(version: (u64, u64, u64)) -> bool {
    version >= MIN_AGY_VERSION
}

/// Parse a completed `agy -p /usage --output-format json` run into the same
/// named quota windows the Local API source produces. Rejects empty and
/// entirely-unknown payloads, and never treats a non-success run as quota.
fn parse_cli_usage_report(output: &AgyOutput) -> Result<UsageSnapshot, ProviderError> {
    let Some(json) = extract_json_object(&output.stdout) else {
        return Err(if output.success {
            ProviderError::Parse("agy CLI returned no JSON usage report".to_string())
        } else {
            ProviderError::Other(format!(
                "agy CLI exited with status {} before returning a usage report",
                output
                    .exit_code
                    .map_or_else(|| "unknown".to_string(), |code| code.to_string())
            ))
        });
    };

    let envelope: AgyUsageEnvelope = serde_json::from_slice(json).map_err(|_| {
        ProviderError::Parse("agy CLI returned an unreadable usage report".to_string())
    })?;

    // A non-zero exit is never a success, even if the body looks successful.
    if !output.success {
        return Err(classify_envelope_failure(&envelope, output.exit_code));
    }

    match envelope.effective_status().as_deref() {
        Some(status) if status.eq_ignore_ascii_case("SUCCESS") => {}
        Some(_) => return Err(classify_envelope_failure(&envelope, None)),
        None => {
            return Err(ProviderError::Parse(
                "agy CLI usage report does not report a status".to_string(),
            ));
        }
    }

    match envelope.command_name().as_deref() {
        Some(name) if name.trim_start_matches('/').eq_ignore_ascii_case("usage") => {}
        Some(name) => {
            // A plain prompt run (SUCCESS with command `prompt`) means
            // `/usage` was not executed as a command; never trust it as quota.
            return Err(ProviderError::Other(format!(
                "agy did not execute /usage as a command (reported command '{}')",
                sanitize_metadata(name)
            )));
        }
        None => {
            return Err(ProviderError::Parse(
                "agy CLI usage report does not name its command".to_string(),
            ));
        }
    }

    let payload = envelope.into_payload().ok_or_else(|| {
        ProviderError::Other("agy CLI /usage report contains no quota groups".to_string())
    })?;
    // Reuses the local quota-summary converter so the CLI keeps the Gemini and
    // Claude/GPT pools and the same named windows as the Local API source.
    // Empty and all-unknown payloads are rejected by the converter.
    quota_summary_from_payload(payload)
}

#[derive(Debug, Deserialize)]
struct AgyUsageEnvelope {
    status: Option<Value>,
    command: Option<Value>,
    data: Option<Value>,
    result: Option<Value>,
    response: Option<Value>,
    summary: Option<Value>,
    payload: Option<Value>,
    description: Option<String>,
    #[serde(default)]
    groups: Vec<QuotaSummaryGroupPayload>,
    message: Option<String>,
    error: Option<String>,
}

impl AgyUsageEnvelope {
    /// Report status: the envelope's own status, else the command object's.
    fn effective_status(&self) -> Option<String> {
        self.status
            .as_ref()
            .and_then(status_text)
            .or_else(|| match self.command.as_ref() {
                Some(Value::Object(map)) => map.get("status").and_then(status_text),
                _ => None,
            })
    }

    /// Command name: a plain string command or an object's `name`/`command`.
    fn command_name(&self) -> Option<String> {
        match self.command.as_ref()? {
            Value::String(name) => non_empty(name),
            Value::Object(map) => map
                .get("name")
                .or_else(|| map.get("command"))
                .and_then(Value::as_str)
                .and_then(non_empty),
            _ => None,
        }
    }

    /// Lowercased message/error text, used only for failure classification.
    /// The text itself is never logged or shown to the user.
    fn failure_text(&self) -> String {
        let mut parts = Vec::new();
        if let Some(command) = self.command.as_ref() {
            for key in ["message", "error"] {
                if let Some(text) = command.get(key).and_then(Value::as_str) {
                    parts.push(text);
                }
            }
        }
        if let Some(text) = self.message.as_deref() {
            parts.push(text);
        }
        if let Some(text) = self.error.as_deref() {
            parts.push(text);
        }
        parts.join(" ").to_lowercase()
    }

    /// The quota payload wherever the envelope carries it: nested in the
    /// command, in a top-level container, or as root-level groups.
    fn into_payload(self) -> Option<QuotaSummaryPayload> {
        if let Some(payload) = self.command.as_ref().and_then(payload_from_value) {
            return Some(payload);
        }
        for candidate in [
            self.data.as_ref(),
            self.result.as_ref(),
            self.response.as_ref(),
            self.summary.as_ref(),
            self.payload.as_ref(),
        ] {
            if let Some(payload) = candidate.and_then(payload_from_value) {
                return Some(payload);
            }
        }
        if self.groups.is_empty() {
            None
        } else {
            Some(QuotaSummaryPayload {
                description: self.description,
                groups: self.groups,
            })
        }
    }
}

fn classify_envelope_failure(envelope: &AgyUsageEnvelope, exit_code: Option<i32>) -> ProviderError {
    let status = envelope.effective_status().unwrap_or_default();
    let haystack = format!("{} {}", status, envelope.failure_text()).to_lowercase();
    if looks_like_auth_failure(&haystack) {
        return ProviderError::AuthRequired;
    }
    let status = sanitize_metadata(&status);
    match exit_code {
        Some(code) => ProviderError::Other(format!(
            "agy CLI usage report failed with status '{status}' (exit code {code})"
        )),
        None => ProviderError::Other(format!(
            "agy CLI usage report failed with status '{status}'"
        )),
    }
}

fn looks_like_auth_failure(haystack: &str) -> bool {
    [
        "auth",
        "login",
        "log in",
        "sign in",
        "signin",
        "signed out",
        "credential",
        "unauthor",
        "401",
        "403",
        "permission",
    ]
    .iter()
    .any(|needle| haystack.contains(needle))
}

/// Keep error metadata short and character-safe. Raw CLI output is never
/// echoed; only a bounded, sanitized status/command label is.
fn sanitize_metadata(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '_' | '-' | '.'))
        .take(64)
        .collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned.to_string()
    }
}

fn status_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => non_empty(text),
        Value::Object(map) => map
            .get("status")
            .and_then(Value::as_str)
            .and_then(non_empty),
        _ => None,
    }
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extract a quota payload from any JSON value: the payload object itself,
/// a nested container, or a JSON string holding another report (some CLI
/// envelopes wrap the structured report in a text field).
fn payload_from_value(value: &Value) -> Option<QuotaSummaryPayload> {
    match value {
        Value::String(text) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|nested| payload_from_value(&nested)),
        Value::Object(_) => {
            if let Ok(payload) = serde_json::from_value::<QuotaSummaryPayload>(value.clone()) {
                if !payload.groups.is_empty() {
                    return Some(payload);
                }
            }
            for key in [
                "payload",
                "data",
                "result",
                "response",
                "summary",
                "quota",
                "quotas",
                "quota_summary",
                "quotaSummary",
                "usage",
                "usage_summary",
                "output",
            ] {
                if let Some(nested) = value.get(key) {
                    if let Some(payload) = payload_from_value(nested) {
                        return Some(payload);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

/// Widest `{...}` span in the output, tolerating warnings printed around the
/// JSON envelope without logging any of it.
fn extract_json_object(bytes: &[u8]) -> Option<&[u8]> {
    let start = bytes.iter().position(|byte| *byte == b'{')?;
    let end = bytes.iter().rposition(|byte| *byte == b'}')?;
    (end > start).then(|| &bytes[start..=end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn success_output(value: Value) -> AgyOutput {
        AgyOutput {
            stdout: serde_json::to_vec(&value).expect("serialize fixture"),
            exit_code: Some(0),
            success: true,
        }
    }

    fn snake_gemini_bucket(remaining: f64) -> Value {
        json!({
            "bucket_id": "gemini-models-five-hour-limit",
            "display_name": "Five Hour Limit Remaining",
            "remaining_fraction": remaining,
            "reset_time": "2026-08-14T18:00:00Z",
        })
    }

    fn snake_claude_bucket(remaining: f64) -> Value {
        json!({
            "bucket_id": "claude-gpt-models-weekly-limit",
            "display_name": "Weekly Limit Remaining",
            "remaining_fraction": remaining,
        })
    }

    #[test]
    fn parses_snake_case_command_payload_with_both_families() {
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": {
                "name": "usage",
                "data": {
                    "groups": [
                        { "display_name": "Gemini Models", "buckets": [snake_gemini_bucket(0.8)] },
                        { "display_name": "Claude and GPT models", "buckets": [snake_claude_bucket(0.4)] },
                    ]
                }
            }
        }));

        let snap = parse_cli_usage_report(&output).unwrap();
        let titles: Vec<&str> = snap
            .extra_rate_windows
            .iter()
            .map(|window| window.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Gemini 5-hour", "Claude/GPT weekly"]);
        assert!(
            snap.extra_rate_windows
                .iter()
                .all(|window| window.usage_known)
        );
        assert!((snap.extra_rate_windows[0].window.used_percent - 20.0).abs() < 0.01);
        assert!((snap.extra_rate_windows[1].window.used_percent - 60.0).abs() < 0.01);
    }

    #[test]
    fn parses_camel_case_root_level_groups() {
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": { "name": "usage" },
            "description": "usage",
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "bucketId": "gemini-models-weekly-limit",
                            "displayName": "Weekly Limit Remaining",
                            "remainingFraction": 0.5,
                        }
                    ]
                }
            ]
        }));

        let snap = parse_cli_usage_report(&output).unwrap();
        assert_eq!(snap.extra_rate_windows.len(), 1);
        assert_eq!(snap.extra_rate_windows[0].title, "Gemini weekly");
    }

    #[test]
    fn parses_string_embedded_report() {
        let inner = json!({
            "groups": [
                {
                    "displayName": "Gemini Models",
                    "buckets": [
                        {
                            "bucketId": "gemini-models-five-hour-limit",
                            "displayName": "Five Hour Limit Remaining",
                            "remainingFraction": 0.25,
                        }
                    ]
                }
            ]
        });
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": { "name": "usage" },
            "response": inner.to_string(),
        }));

        let snap = parse_cli_usage_report(&output).unwrap();
        assert_eq!(snap.extra_rate_windows.len(), 1);
        assert!((snap.extra_rate_windows[0].window.used_percent - 75.0).abs() < 0.01);
    }

    #[test]
    fn accepts_command_level_status() {
        let output = success_output(json!({
            "command": {
                "name": "usage",
                "status": "SUCCESS",
                "data": {
                    "groups": [
                        { "display_name": "Gemini Models", "buckets": [snake_gemini_bucket(0.6)] }
                    ]
                }
            }
        }));

        let snap = parse_cli_usage_report(&output).unwrap();
        assert_eq!(snap.extra_rate_windows.len(), 1);
    }

    #[test]
    fn accepts_slash_usage_command_and_nested_quota_summary() {
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": { "name": "/usage" },
            "result": {
                "quota_summary": {
                    "groups": [
                        { "display_name": "Gemini Models", "buckets": [snake_gemini_bucket(0.7)] }
                    ]
                }
            }
        }));

        let snap = parse_cli_usage_report(&output).unwrap();
        assert_eq!(snap.extra_rate_windows.len(), 1);
        assert_eq!(snap.extra_rate_windows[0].title, "Gemini 5-hour");
    }

    #[test]
    fn parses_upstream_name_id_schema_for_both_families() {
        // Exact upstream CLI shape: `groups[].name` with `buckets[].id` and
        // `buckets[].name` under `status SUCCESS` / `command.name usage` /
        // `command.data`.
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": {
                "name": "usage",
                "data": {
                    "groups": [
                        {
                            "name": "Gemini Models",
                            "buckets": [
                                {
                                    "id": "gemini-5h",
                                    "name": "Five Hour Limit Remaining",
                                    "remaining_fraction": 0.6,
                                    "reset_time": "2026-09-20T12:34:56Z"
                                }
                            ]
                        },
                        {
                            "name": "Claude and GPT models",
                            "buckets": [
                                {
                                    "id": "claude-gpt-weekly",
                                    "name": "Weekly Limit Remaining",
                                    "remaining_fraction": 0.25,
                                    "reset_time": "2026-09-21T12:34:56Z"
                                }
                            ]
                        }
                    ]
                }
            }
        }));

        let snap = parse_cli_usage_report(&output).unwrap();
        let titles: Vec<&str> = snap
            .extra_rate_windows
            .iter()
            .map(|window| window.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Gemini 5-hour", "Claude/GPT weekly"]);
        assert!(
            snap.extra_rate_windows
                .iter()
                .all(|window| window.usage_known)
        );
        assert!((snap.extra_rate_windows[0].window.used_percent - 40.0).abs() < 0.01);
        assert!((snap.extra_rate_windows[1].window.used_percent - 75.0).abs() < 0.01);
        assert_eq!(snap.extra_rate_windows[0].window.window_minutes, Some(300));
        assert_eq!(
            snap.extra_rate_windows[1].window.resets_at,
            super::super::parse_reset_time("2026-09-21T12:34:56Z")
        );
    }

    #[test]
    fn rejects_non_success_status_without_leaking_the_message() {
        let output = success_output(json!({
            "status": "ERROR",
            "command": { "name": "usage" },
            "message": "quota service temporarily unreachable for user@example.com",
            "data": {
                "groups": [
                    { "display_name": "Gemini Models", "buckets": [snake_gemini_bucket(0.8)] }
                ]
            }
        }));

        let error = parse_cli_usage_report(&output).unwrap_err();
        let message = error.to_string();
        assert!(matches!(error, ProviderError::Other(_)));
        assert!(message.contains("ERROR"));
        assert!(!message.contains("unreachable"));
        assert!(!message.contains("user@example.com"));
    }

    #[test]
    fn maps_signed_out_status_to_auth_required() {
        let output = success_output(json!({
            "status": "ERROR",
            "command": { "name": "usage" },
            "message": "Not signed in. Run `agy login` to continue.",
        }));

        assert!(matches!(
            parse_cli_usage_report(&output).unwrap_err(),
            ProviderError::AuthRequired
        ));
    }

    #[test]
    fn rejects_prompt_instead_of_usage_command() {
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": { "name": "prompt" },
            "response": "I can help with that.",
        }));

        let error = parse_cli_usage_report(&output).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("/usage"));
        assert!(!message.contains("I can help"));
    }

    #[test]
    fn rejects_missing_status() {
        let output = success_output(json!({
            "command": { "name": "usage" },
            "data": { "groups": [
                { "display_name": "Gemini Models", "buckets": [snake_gemini_bucket(0.8)] }
            ] }
        }));

        assert!(matches!(
            parse_cli_usage_report(&output).unwrap_err(),
            ProviderError::Parse(_)
        ));
    }

    #[test]
    fn rejects_missing_command_name() {
        let output = success_output(json!({
            "status": "SUCCESS",
            "data": { "groups": [
                { "display_name": "Gemini Models", "buckets": [snake_gemini_bucket(0.8)] }
            ] }
        }));

        assert!(matches!(
            parse_cli_usage_report(&output).unwrap_err(),
            ProviderError::Parse(_)
        ));
    }

    #[test]
    fn rejects_empty_groups() {
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": { "name": "usage" },
            "data": { "groups": [] }
        }));

        assert!(matches!(
            parse_cli_usage_report(&output).unwrap_err(),
            ProviderError::Other(_)
        ));
    }

    #[test]
    fn rejects_unknown_only_buckets() {
        let output = success_output(json!({
            "status": "SUCCESS",
            "command": { "name": "usage" },
            "data": { "groups": [
                {
                    "display_name": "Gemini Models",
                    "buckets": [
                        {
                            "bucket_id": "gemini-models-five-hour-limit",
                            "display_name": "Five Hour Limit Remaining",
                            "reset_time": "2026-08-14T18:00:00Z",
                        }
                    ]
                }
            ] }
        }));

        assert!(parse_cli_usage_report(&output).is_err());
    }

    #[test]
    fn rejects_malformed_output() {
        let output = AgyOutput {
            stdout: b"no json here".to_vec(),
            exit_code: Some(0),
            success: true,
        };

        assert!(matches!(
            parse_cli_usage_report(&output).unwrap_err(),
            ProviderError::Parse(_)
        ));
    }

    #[test]
    fn rejects_non_zero_exit_even_with_a_success_body() {
        let mut output = success_output(json!({
            "status": "SUCCESS",
            "command": { "name": "usage" },
            "data": { "groups": [
                { "display_name": "Gemini Models", "buckets": [snake_gemini_bucket(0.8)] }
            ] }
        }));
        output.success = false;
        output.exit_code = Some(1);

        let error = parse_cli_usage_report(&output).unwrap_err();
        assert!(error.to_string().contains("exit code 1"));
    }

    #[test]
    fn extracts_json_object_from_noisy_stdout() {
        let raw = b"warning: something\n{\"status\":\"SUCCESS\"}\ndone";
        assert_eq!(
            extract_json_object(raw),
            Some(br#"{"status":"SUCCESS"}"#.as_slice())
        );
        assert_eq!(extract_json_object(b"no braces"), None);
        assert_eq!(extract_json_object(b"}{"), None);
    }

    #[test]
    fn parses_and_gates_agy_versions() {
        assert_eq!(parse_agy_version("agy 1.1.11"), Some((1, 1, 11)));
        assert_eq!(parse_agy_version("1.2.0\n"), Some((1, 2, 0)));
        assert_eq!(parse_agy_version("v2.0.1 (build 5)"), Some((2, 0, 1)));
        assert_eq!(parse_agy_version("no version here"), None);

        assert!(agy_version_supported((1, 1, 11)));
        assert!(agy_version_supported((1, 2, 0)));
        assert!(!agy_version_supported((1, 1, 10)));
        assert!(!agy_version_supported((0, 9, 99)));
    }

    #[test]
    fn override_binary_must_be_a_real_file() {
        let file = tempfile::NamedTempFile::new().expect("temp file");
        assert_eq!(
            resolve_agy_binary_with(Some(file.path())),
            Some(file.path().to_path_buf())
        );

        // A missing or non-file override fails honestly; it must not fall
        // back to an unrelated agy discovered on PATH.
        let missing = std::env::temp_dir().join("codexbar-agy-definitely-missing.exe");
        assert_eq!(resolve_agy_binary_with(Some(&missing)), None);
        assert_eq!(resolve_agy_binary_with(Some(&std::env::temp_dir())), None);
    }

    #[cfg(windows)]
    #[test]
    fn standard_locations_include_codex_managed_cli_install() {
        let expected = dirs::home_dir()
            .expect("Windows home directory")
            .join(".codex")
            .join("bin")
            .join("agy.exe");
        assert!(
            standard_agy_locations()
                .into_iter()
                .flatten()
                .any(|path| path == expected),
            "standard AGY discovery must include %USERPROFILE%\\.codex\\bin\\agy.exe"
        );
    }

    /// Regression: the subprocess route must return for any existing but
    /// non-quota binary instead of exhausting the process stack. `where.exe`
    /// is a benign Windows executable that exits quickly without touching
    /// accounts. The probe runs on a 1 MiB thread to mirror the Windows CLI
    /// main thread's stack budget, where the debug-build overflow surfaced.
    #[cfg(windows)]
    #[test]
    fn benign_child_command_returns_without_stack_overflow() {
        let binary = PathBuf::from(r"C:\Windows\System32\where.exe");
        if !binary.is_file() {
            return;
        }

        let output = std::thread::Builder::new()
            .name("antigravity-benign-cli".to_string())
            .stack_size(1024 * 1024)
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("probe runtime");
                runtime.block_on(async move {
                    // Mirror `fetch_cli_usage`: the version probe first, then
                    // the structured usage command, on one thread.
                    let version = run_agy(&binary, &["--version"], Duration::from_secs(10)).await;
                    assert!(version.is_ok(), "version probe must return");
                    run_agy(
                        &binary,
                        &[
                            "-p",
                            "/usage",
                            "--output-format",
                            "json",
                            "--print-timeout",
                            "90s",
                        ],
                        Duration::from_secs(30),
                    )
                    .await
                })
            })
            .expect("spawn probe thread")
            .join()
            .expect("benign child path must return instead of overflowing")
            .expect("benign child should complete");

        // The benign binary is not an agy report: the parser must fail
        // honestly instead of inventing quota.
        assert!(parse_cli_usage_report(&output).is_err());
    }
}
