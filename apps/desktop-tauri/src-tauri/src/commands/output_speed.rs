use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

const MAX_TAIL_BYTES: u64 = 4 * 1024 * 1024;
const MAX_RECENT_SAMPLES: usize = 20;
const MAX_REASONABLE_TOKENS_PER_SECOND: f64 = 1000.0;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpeedSample {
    pub tokens_per_second: f64,
    pub output_tokens: u64,
    pub duration_ms: u64,
    pub completed_at_ms: i64,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderOutputSpeed {
    pub provider_id: &'static str,
    pub status: &'static str,
    pub tokens_per_second: Option<f64>,
    pub output_tokens: Option<u64>,
    pub updated_at_ms: Option<i64>,
    pub approximate: bool,
    pub recent_samples: Vec<OutputSpeedSample>,
}

impl ProviderOutputSpeed {
    fn unavailable(provider_id: &'static str) -> Self {
        Self {
            provider_id,
            status: "unavailable",
            tokens_per_second: None,
            output_tokens: None,
            updated_at_ms: None,
            approximate: true,
            recent_samples: Vec::new(),
        }
    }
}

fn push_recent_sample(samples: &mut Vec<OutputSpeedSample>, sample: OutputSpeedSample) {
    samples.push(sample);
    if samples.len() > MAX_RECENT_SAMPLES {
        samples.remove(0);
    }
}

fn output_speed_sample(
    tokens: u64,
    started_at_ms: i64,
    completed_at_ms: i64,
    model: Option<String>,
) -> Option<OutputSpeedSample> {
    let elapsed_ms = completed_at_ms.saturating_sub(started_at_ms);
    output_speed_sample_for_duration(tokens, elapsed_ms, completed_at_ms, model)
}

/// Build a sample from a duration that is already known.
///
/// Codex and Claude only let us bracket a response between two log lines, so
/// they subtract timestamps. Grok records the generation duration itself
/// (`usage.apiDurationMs`), which is strictly better — it excludes the time the
/// agent spent running tools between model calls — so that path passes the
/// measured duration straight through instead of re-deriving a worse one.
fn output_speed_sample_for_duration(
    tokens: u64,
    elapsed_ms: i64,
    completed_at_ms: i64,
    model: Option<String>,
) -> Option<OutputSpeedSample> {
    if tokens == 0 || elapsed_ms < 250 {
        return None;
    }
    let tokens_per_second = tokens as f64 * 1000.0 / elapsed_ms as f64;
    if !tokens_per_second.is_finite() || tokens_per_second > MAX_REASONABLE_TOKENS_PER_SECOND {
        return None;
    }
    Some(OutputSpeedSample {
        tokens_per_second,
        output_tokens: tokens,
        duration_ms: elapsed_ms as u64,
        completed_at_ms,
        model,
    })
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputSpeedSnapshot {
    pub codex: ProviderOutputSpeed,
    pub claude: ProviderOutputSpeed,
    pub grok: ProviderOutputSpeed,
    pub kimi: ProviderOutputSpeed,
    pub opencode: ProviderOutputSpeed,
    pub opencodego: ProviderOutputSpeed,
}

/// One resolved snapshot, kept briefly so repeated callers do not each pay
/// the session-file scan.
///
/// Building a snapshot walks Codex/Claude/Grok session roots and reads file
/// tails — real disk I/O. It is requested from the tray icon/tooltip/strip
/// update after every provider refresh *and* by the frontend speed hook, so
/// an uncached read ran several times per refresh round. The frontend
/// scheduler gates its own reads behind a 15s TTL; this short backend TTL
/// only collapses the burst of reads within one refresh/update cycle.
static SNAPSHOT_CACHE: std::sync::Mutex<Option<(std::time::Instant, OutputSpeedSnapshot)>> =
    std::sync::Mutex::new(None);
const SNAPSHOT_TTL: std::time::Duration = std::time::Duration::from_secs(3);

#[tauri::command]
pub fn get_output_speed_snapshot() -> OutputSpeedSnapshot {
    if let Ok(guard) = SNAPSHOT_CACHE.lock()
        && let Some((built_at, snapshot)) = guard.as_ref()
        && built_at.elapsed() < SNAPSHOT_TTL
    {
        return snapshot.clone();
    }

    let settings = codexbar::settings::Settings::load();
    let snapshot = OutputSpeedSnapshot {
        codex: resolve_codex_speed(&settings.codex_custom_sessions_dirs),
        claude: resolve_claude_speed(),
        grok: resolve_grok_speed(),
        kimi: resolve_kimi_speed(),
        opencode: resolve_opencode_speed(),
        opencodego: resolve_opencodego_speed(),
    };

    if let Ok(mut guard) = SNAPSHOT_CACHE.lock() {
        *guard = Some((std::time::Instant::now(), snapshot.clone()));
    }
    snapshot
}

fn resolve_codex_speed(custom_dirs: &[String]) -> ProviderOutputSpeed {
    let roots = codex_session_roots(custom_dirs);
    let files = recent_matching_files_in_roots(&roots, &|_| true, 5);
    for path in files {
        if let Ok(text) = read_tail(&path) {
            let speed = parse_codex_tail(&text);
            if speed.status != "unavailable" || speed.tokens_per_second.is_some() {
                return speed;
            }
        }
    }
    ProviderOutputSpeed::unavailable("codex")
}

fn resolve_claude_speed() -> ProviderOutputSpeed {
    let roots = claude_project_roots();
    let files = recent_matching_files_in_roots(&roots, &|_| true, 5);
    for path in files {
        if let Ok(text) = read_tail(&path) {
            let speed = parse_claude_tail(&text);
            if speed.status != "unavailable" || speed.tokens_per_second.is_some() {
                return speed;
            }
        }
    }
    ProviderOutputSpeed::unavailable("claude")
}

fn resolve_grok_speed() -> ProviderOutputSpeed {
    let roots = grok_roots();
    let files = recent_matching_files_in_roots(
        &roots,
        &|path| path.file_name().is_some_and(|name| name == "updates.jsonl"),
        5,
    );
    for path in files {
        if let Ok(text) = read_tail(&path) {
            let speed = parse_grok_tail(&text);
            if speed.status != "unavailable" || speed.tokens_per_second.is_some() {
                return speed;
            }
        }
    }
    ProviderOutputSpeed::unavailable("grok")
}

/// Kimi speed comes from the Kimi Code wire logs: each turn's `usage.record`
/// row is preceded by the `llm.request` row that started the model call, so
/// the response duration is the time between them. Kimi CLI rows carry running
/// totals without any request marker, so they cannot yield a duration and are
/// skipped here — a CLI-only install simply reports unavailable.
fn resolve_kimi_speed() -> ProviderOutputSpeed {
    let files = recent_matching_files_in_roots(
        &kimi_wire_roots(),
        &|path| path.file_name().is_some_and(|name| name == "wire.jsonl"),
        5,
    );
    for path in files {
        if let Ok(text) = read_tail(&path) {
            let speed = parse_kimi_tail(&text);
            if speed.status != "unavailable" || speed.tokens_per_second.is_some() {
                return speed;
            }
        }
    }
    ProviderOutputSpeed::unavailable("kimi")
}

/// OpenCode-family speed comes from the local `opencode.db` first: every
/// completed assistant message carries `time.created` / `time.completed` and
/// `tokens.output`, so the wall-clock duration of each response is known. The
/// database is the storage modern installs actually write (the legacy
/// per-message JSON tree was abandoned), which is why a capability-gated
/// reader that only walked that tree reported unavailable forever. The
/// duration includes any tool time inside the turn, so the rate is a lower
/// bound and stays flagged approximate. The legacy tree remains a fallback
/// for installs that still use it; with neither source the provider reports
/// unavailable rather than guessing.
fn resolve_opencode_speed() -> ProviderOutputSpeed {
    opencode_db_speed("opencode", "opencode")
        .or_else(resolve_opencode_legacy_speed)
        .unwrap_or_else(|| ProviderOutputSpeed::unavailable("opencode"))
}

/// The OpenCode Go subscription shares the database but stores its rows under
/// `providerID = "opencode-go"`; the same message timing applies there.
fn resolve_opencodego_speed() -> ProviderOutputSpeed {
    opencode_db_speed("opencodego", "opencode-go")
        .unwrap_or_else(|| ProviderOutputSpeed::unavailable("opencodego"))
}

fn opencode_db_speed(
    provider_id: &'static str,
    provider_filter: &str,
) -> Option<ProviderOutputSpeed> {
    let rows = codexbar::providers::opencodego::recent_message_speeds(
        provider_filter,
        MAX_RECENT_SAMPLES * 2,
    );
    opencode_speed_from_rows(provider_id, &rows)
}

/// Build a speed readout from newest-first database rows. Split from the SQL
/// read so tests can exercise the derivation without a live database.
fn opencode_speed_from_rows(
    provider_id: &'static str,
    rows: &[codexbar::providers::opencodego::MessageSpeed],
) -> Option<ProviderOutputSpeed> {
    let mut recent_samples = Vec::new();
    // Rows arrive newest-first; walk them oldest-first so the ring keeps the
    // newest samples and `recent_samples` reads chronologically (the order
    // every other parser produces).
    for row in rows.iter().rev() {
        let Some(sample) = output_speed_sample_for_duration(
            row.output_tokens,
            row.completed_ms.saturating_sub(row.created_ms),
            row.completed_ms,
            row.model.clone(),
        ) else {
            continue;
        };
        push_recent_sample(&mut recent_samples, sample);
    }
    if recent_samples.is_empty() {
        return None;
    }
    let latest = recent_samples.last().expect("non-empty samples");
    Some(ProviderOutputSpeed {
        provider_id,
        status: "recent",
        tokens_per_second: Some(latest.tokens_per_second),
        output_tokens: Some(latest.output_tokens),
        updated_at_ms: Some(latest.completed_at_ms),
        approximate: true,
        recent_samples,
    })
}

/// Legacy OpenCode per-message JSON storage, kept for installs whose data
/// still lives in `storage/message/msg_*.json` instead of the database.
fn resolve_opencode_legacy_speed() -> Option<ProviderOutputSpeed> {
    let files = recent_matching_files_with_extensions(
        &opencode_message_roots(),
        &|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("msg_"))
        },
        MAX_RECENT_SAMPLES,
        &["json"],
    );
    let mut recent_samples = Vec::new();
    for path in files {
        if let Ok(text) = fs::read_to_string(&path)
            && let Some(sample) = parse_opencode_message(&text)
        {
            push_recent_sample(&mut recent_samples, sample);
        }
    }
    if recent_samples.is_empty() {
        return None;
    }
    // Files were visited newest-first; reverse so the ring ends on the newest
    // message and the headline rate is the most recent completed response.
    recent_samples.reverse();
    let latest = recent_samples.last().expect("non-empty samples");
    Some(ProviderOutputSpeed {
        provider_id: "opencode",
        status: "recent",
        tokens_per_second: Some(latest.tokens_per_second),
        output_tokens: Some(latest.output_tokens),
        updated_at_ms: Some(latest.completed_at_ms),
        approximate: true,
        recent_samples,
    })
}

fn codex_session_roots(custom_dirs: &[String]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("CODEX_HOME") {
        let home = home.trim();
        if !home.is_empty() {
            roots.push(normalize_sessions_root(PathBuf::from(home)));
        }
    }
    if let Some(home) = user_home_dir() {
        roots.push(home.join(".codex").join("sessions"));
    }
    roots.extend(
        custom_dirs
            .iter()
            .filter(|path| !path.trim().is_empty())
            .map(|path| normalize_sessions_root(PathBuf::from(path.trim()))),
    );
    roots
}

fn normalize_sessions_root(path: PathBuf) -> PathBuf {
    if path.file_name().is_some_and(|name| name == "sessions") {
        path
    } else {
        path.join("sessions")
    }
}

fn claude_project_roots() -> Vec<PathBuf> {
    user_home_dir()
        .map(|home| vec![home.join(".claude").join("projects")])
        .unwrap_or_default()
}

fn grok_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(home) = std::env::var("GROK_HOME") {
        let home = home.trim();
        if !home.is_empty() {
            roots.push(normalize_grok_sessions_root(PathBuf::from(home)));
        }
    }
    if let Some(home) = user_home_dir() {
        roots.push(home.join(".grok").join("sessions"));
    }
    roots
}

fn normalize_grok_sessions_root(path: PathBuf) -> PathBuf {
    if path.file_name().is_some_and(|name| name == "sessions") {
        path
    } else {
        path.join("sessions")
    }
}

/// Kimi Code first (its wire logs carry request→record durations), then the
/// Kimi CLI root. `KIMI_CODE_HOME` / `KIMI_HOME` mirror the cost scanner's
/// root resolution so both features see the same relocated installs.
fn kimi_wire_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = user_home_dir();
    if let Ok(value) = std::env::var("KIMI_CODE_HOME") {
        let value = value.trim();
        if !value.is_empty() {
            roots.push(PathBuf::from(value).join("sessions"));
        }
    } else if let Some(home) = home.as_ref() {
        roots.push(home.join(".kimi-code").join("sessions"));
    }
    if let Ok(value) = std::env::var("KIMI_HOME") {
        let value = value.trim();
        if !value.is_empty() {
            roots.push(PathBuf::from(value).join("sessions"));
        }
    } else if let Some(home) = home.as_ref() {
        roots.push(home.join(".kimi").join("sessions"));
    }
    roots
}

/// Legacy OpenCode per-message JSON storage. The XDG data convention resolves
/// to `%USERPROFILE%\.local\share` on Windows, and the desktop app also uses
/// `%LOCALAPPDATA%\opencode`; both are probed.
fn opencode_message_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(value) = std::env::var("XDG_DATA_HOME") {
        let value = value.trim();
        if !value.is_empty() {
            roots.push(
                PathBuf::from(value)
                    .join("opencode")
                    .join("storage")
                    .join("message"),
            );
        }
    }
    if let Some(home) = user_home_dir() {
        roots.push(
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("storage")
                .join("message"),
        );
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        roots.push(
            PathBuf::from(local)
                .join("opencode")
                .join("storage")
                .join("message"),
        );
    }
    roots
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn recent_matching_files_in_roots(
    roots: &[PathBuf],
    accept: &dyn Fn(&Path) -> bool,
    limit: usize,
) -> Vec<PathBuf> {
    recent_matching_files_with_extensions(roots, accept, limit, &["jsonl"])
}

/// Extension-parameterized variant used by the OpenCode resolver, whose legacy
/// storage tree is one `.json` document per message rather than JSONL.
fn recent_matching_files_with_extensions(
    roots: &[PathBuf],
    accept: &dyn Fn(&Path) -> bool,
    limit: usize,
    extensions: &[&str],
) -> Vec<PathBuf> {
    let mut candidates: Vec<(SystemTime, PathBuf)> = Vec::new();
    for root in roots {
        visit_files_with_extensions(root, extensions, &mut |path, modified| {
            if !accept(path) {
                return;
            }
            candidates.push((modified, path.to_path_buf()));
        });
    }
    candidates.sort_by(|(a, _), (b, _)| b.cmp(a));
    candidates
        .into_iter()
        .take(limit)
        .map(|(_, path)| path)
        .collect()
}

fn visit_jsonl_files(root: &Path, visit: &mut impl FnMut(&Path, SystemTime)) {
    visit_files_with_extensions(root, &["jsonl"], visit);
}

fn visit_files_with_extensions(
    root: &Path,
    extensions: &[&str],
    visit: &mut impl FnMut(&Path, SystemTime),
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            visit_files_with_extensions(&path, extensions, visit);
        } else if path
            .extension()
            .is_some_and(|extension| extensions.contains(&extension.to_string_lossy().as_ref()))
        {
            visit(&path, metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH));
        }
    }
}

fn read_tail(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(MAX_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut bytes)?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        if let Some(newline) = text.find('\n') {
            text.drain(..=newline);
        } else {
            text.clear();
        }
    }
    Ok(text)
}

fn timestamp_ms(value: &Value) -> Option<i64> {
    let raw = value.get("timestamp")?.as_str()?;
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|timestamp| timestamp.with_timezone(&Utc).timestamp_millis())
}

fn parse_codex_tail(text: &str) -> ProviderOutputSpeed {
    let mut response_started_at = None;
    let mut pending_response_start = None;
    let mut awaiting_response = false;
    let mut recent_rate = None;
    let mut recent_tokens = None;
    let mut recent_at = None;
    let mut recent_samples = Vec::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(at) = timestamp_ms(&value) else {
            continue;
        };
        let record_type = value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let payload = value.get("payload").unwrap_or(&Value::Null);
        let subtype = payload
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();

        match (record_type, subtype) {
            ("event_msg", "task_started" | "user_message") => {
                response_started_at = Some(at);
                awaiting_response = true;
            }
            ("response_item", "function_call_output" | "custom_tool_call_output") => {
                // Codex persists the usage record for the model call that
                // requested this tool immediately after the tool output. Keep
                // this timestamp pending until that usage has been measured;
                // it then becomes the start of the next model call.
                pending_response_start = Some(at);
            }
            ("response_item", "message")
                if payload.get("role").and_then(Value::as_str) == Some("user") =>
            {
                response_started_at = Some(at);
                awaiting_response = true;
            }
            ("response_item", "function_call" | "custom_tool_call") => {
                awaiting_response = false;
            }
            ("event_msg", "token_count") => {
                let output_tokens = payload
                    .pointer("/info/last_token_usage/output_tokens")
                    .and_then(Value::as_u64);
                if let (Some(tokens), Some(started_at)) = (output_tokens, response_started_at) {
                    if let Some(sample) = output_speed_sample(tokens, started_at, at, None) {
                        recent_rate = Some(sample.tokens_per_second);
                        recent_tokens = Some(tokens);
                        recent_at = Some(at);
                        push_recent_sample(&mut recent_samples, sample);
                    }
                }
                if let Some(next_start) = pending_response_start.take() {
                    response_started_at = Some(next_start);
                    awaiting_response = true;
                } else {
                    awaiting_response = false;
                }
            }
            ("event_msg", "task_complete") => awaiting_response = false,
            _ => {}
        }
    }

    ProviderOutputSpeed {
        provider_id: "codex",
        status: if awaiting_response {
            "generating"
        } else if recent_rate.is_some() {
            "recent"
        } else {
            "unavailable"
        },
        tokens_per_second: recent_rate,
        output_tokens: recent_tokens,
        updated_at_ms: recent_at,
        approximate: true,
        recent_samples,
    }
}

fn parse_claude_tail(text: &str) -> ProviderOutputSpeed {
    let mut response_started_at = None;
    let mut awaiting_response = false;
    let mut recent_rate = None;
    let mut recent_tokens = None;
    let mut recent_at = None;
    let mut recent_samples = Vec::new();
    let mut seen_message_ids = HashSet::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(at) = timestamp_ms(&value) else {
            continue;
        };
        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "user" => {
                response_started_at = Some(at);
                awaiting_response = true;
            }
            "assistant" => {
                let message = value.get("message").unwrap_or(&Value::Null);
                let output_tokens = message
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64);
                let completed = message
                    .get("stop_reason")
                    .is_some_and(|reason| !reason.is_null());
                let message_id = message
                    .get("id")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| value.get("uuid").and_then(Value::as_str).map(str::to_owned))
                    .unwrap_or_else(|| format!("{at}:{}", output_tokens.unwrap_or_default()));
                if completed && seen_message_ids.insert(message_id) {
                    if let (Some(tokens), Some(started_at)) = (output_tokens, response_started_at) {
                        let model = message
                            .get("model")
                            .and_then(Value::as_str)
                            .map(str::to_owned);
                        if let Some(sample) = output_speed_sample(tokens, started_at, at, model) {
                            recent_rate = Some(sample.tokens_per_second);
                            recent_tokens = Some(tokens);
                            recent_at = Some(at);
                            push_recent_sample(&mut recent_samples, sample);
                        }
                    }
                    awaiting_response = false;
                }
            }
            _ => {}
        }
    }

    ProviderOutputSpeed {
        provider_id: "claude",
        status: if awaiting_response {
            "generating"
        } else if recent_rate.is_some() {
            "recent"
        } else {
            "unavailable"
        },
        tokens_per_second: recent_rate,
        output_tokens: recent_tokens,
        updated_at_ms: recent_at,
        approximate: true,
        recent_samples,
    }
}

/// Read the Grok CLI's per-session `updates.jsonl`.
///
/// # Why this provider is not like the other two
///
/// Codex and Claude log a token count and leave the timing to be inferred from
/// surrounding records, so both parsers bracket a response between a user line
/// and a usage line and divide. Grok instead emits one `turn_completed` update
/// carrying a complete `usage` object:
///
/// ```text
/// "usage":{"inputTokens":237473,"outputTokens":2764,"totalTokens":240237,
///          "reasoningTokens":2155,"modelCalls":10,"apiDurationMs":104937,...}
/// ```
///
/// `totalTokens == inputTokens + outputTokens` in every record observed, so
/// `outputTokens` is the whole generated count (`reasoningTokens` is a subset of
/// it, not an addition) and is directly comparable to Codex's
/// `last_token_usage.output_tokens` and Claude's `usage.output_tokens`.
///
/// `apiDurationMs` is the summed model time across the turn's `modelCalls`. Wall
/// clock would also count the seconds the agent spent running tools between
/// those calls, which is not generation and would understate the rate on
/// tool-heavy turns — so the reported duration is used as-is.
fn parse_grok_tail(text: &str) -> ProviderOutputSpeed {
    let mut awaiting_response = false;
    let mut recent_rate = None;
    let mut recent_tokens = None;
    let mut recent_at = None;
    let mut recent_samples = Vec::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(update) = value.pointer("/params/update") else {
            continue;
        };
        match update
            .get("sessionUpdate")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "user_message_chunk" => awaiting_response = true,
            "turn_completed" => {
                awaiting_response = false;
                let Some(usage) = update.get("usage") else {
                    continue;
                };
                // `agentTimestampMs` is the precise emission time; the sibling
                // `timestamp` is only whole seconds, so it is the fallback.
                let at = value
                    .pointer("/_meta/agentTimestampMs")
                    .and_then(Value::as_i64)
                    .or_else(|| {
                        value
                            .get("timestamp")
                            .and_then(Value::as_i64)
                            .map(|seconds| seconds * 1000)
                    });
                let tokens = usage.get("outputTokens").and_then(Value::as_u64);
                let duration = usage.get("apiDurationMs").and_then(Value::as_i64);
                let (Some(tokens), Some(duration), Some(at)) = (tokens, duration, at) else {
                    continue;
                };
                // `modelUsage` is keyed by model id; a turn can span more than
                // one, so name a model only when the attribution is unambiguous.
                let model = usage
                    .get("modelUsage")
                    .and_then(Value::as_object)
                    .filter(|models| models.len() == 1)
                    .and_then(|models| models.keys().next().cloned());
                if let Some(sample) = output_speed_sample_for_duration(tokens, duration, at, model)
                {
                    recent_rate = Some(sample.tokens_per_second);
                    recent_tokens = Some(tokens);
                    recent_at = Some(at);
                    push_recent_sample(&mut recent_samples, sample);
                }
            }
            _ => {}
        }
    }

    ProviderOutputSpeed {
        provider_id: "grok",
        status: if awaiting_response {
            "generating"
        } else if recent_rate.is_some() {
            "recent"
        } else {
            "unavailable"
        },
        tokens_per_second: recent_rate,
        output_tokens: recent_tokens,
        updated_at_ms: recent_at,
        approximate: true,
        recent_samples,
    }
}

/// Kimi model ids route through a `kimi-code/` prefix and a symbolic
/// environment placeholder; neither is a real model name worth displaying.
fn normalize_kimi_speed_model(raw: Option<&str>) -> Option<String> {
    let model = raw?.trim();
    if model.is_empty() || model == "__kimi_env_model__" {
        return None;
    }
    let stripped = model.strip_prefix("kimi-code/").unwrap_or(model).trim();
    (!stripped.is_empty()).then(|| stripped.to_string())
}

/// Kimi's wire logs have carried epoch milliseconds, epoch seconds and
/// numeric strings across versions; mirror the cost scanner's discriminator
/// (>1e11 is already milliseconds, anything smaller is seconds) instead of
/// trusting one shape and silently dropping every difference.
fn kimi_time_ms(value: &Value) -> Option<i64> {
    let raw = match value.get("time")? {
        Value::Number(number) => number.as_f64()?,
        Value::String(text) => text.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    if !raw.is_finite() || raw <= 0.0 {
        return None;
    }
    Some(if raw > 100_000_000_000.0 {
        raw as i64
    } else {
        (raw * 1000.0) as i64
    })
}

/// Parse a Kimi wire-log tail. Only the Kimi Code dialect can produce a rate:
/// each turn-scoped `usage.record` is paired with the `llm.request` row that
/// started the model call, giving the response's real duration. Kimi CLI rows
/// carry running totals without a request marker, so they yield nothing here.
fn parse_kimi_tail(text: &str) -> ProviderOutputSpeed {
    let mut pending_request: Option<(i64, Option<String>)> = None;
    let mut recent_rate = None;
    let mut recent_tokens = None;
    let mut recent_at = None;
    let mut recent_samples = Vec::new();

    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match value.get("type").and_then(Value::as_str) {
            Some("llm.request") => {
                if let Some(at) = kimi_time_ms(&value) {
                    let model =
                        normalize_kimi_speed_model(value.get("model").and_then(Value::as_str));
                    pending_request = Some((at, model));
                }
            }
            Some("usage.record") => {
                if value
                    .get("usageScope")
                    .and_then(Value::as_str)
                    .is_some_and(|scope| scope != "turn")
                {
                    continue;
                }
                let Some(at) = kimi_time_ms(&value) else {
                    continue;
                };
                let Some(tokens) = value.pointer("/usage/output").and_then(Value::as_u64) else {
                    continue;
                };
                let model = normalize_kimi_speed_model(value.get("model").and_then(Value::as_str))
                    .or_else(|| {
                        pending_request
                            .as_ref()
                            .and_then(|(_, model)| model.clone())
                    });
                if let Some((started_at, pending_model)) = pending_request.as_ref()
                    && let Some(sample) = output_speed_sample_for_duration(
                        tokens,
                        at.saturating_sub(*started_at),
                        at,
                        model.clone().or_else(|| pending_model.clone()),
                    )
                {
                    recent_rate = Some(sample.tokens_per_second);
                    recent_tokens = Some(tokens);
                    recent_at = Some(at);
                    push_recent_sample(&mut recent_samples, sample);
                }
            }
            _ => {}
        }
    }

    ProviderOutputSpeed {
        provider_id: "kimi",
        status: if recent_rate.is_some() {
            "recent"
        } else {
            "unavailable"
        },
        tokens_per_second: recent_rate,
        output_tokens: recent_tokens,
        updated_at_ms: recent_at,
        approximate: true,
        recent_samples,
    }
}

/// Parse one legacy OpenCode message document into a speed sample.
///
/// The wall-clock `completed − created` span includes any tool time inside
/// the turn, so the rate is a lower bound on raw generation speed.
fn parse_opencode_message(text: &str) -> Option<OutputSpeedSample> {
    let value = serde_json::from_str::<Value>(text).ok()?;
    if value.get("providerID").and_then(Value::as_str) != Some("opencode") {
        return None;
    }
    if value.get("role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let output_tokens = value.pointer("/tokens/output").and_then(Value::as_u64)?;
    let created_at = value.pointer("/time/created").and_then(Value::as_f64)? as i64;
    let completed_at = value.pointer("/time/completed").and_then(Value::as_f64)? as i64;
    let model = value
        .get("modelID")
        .and_then(Value::as_str)
        .map(str::to_string);
    output_speed_sample_for_duration(
        output_tokens,
        completed_at.saturating_sub(created_at),
        completed_at,
        model,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_reports_generation_and_keeps_last_completed_rate() {
        let text = r#"
{"timestamp":"2026-07-16T12:00:00.000Z","type":"event_msg","payload":{"type":"task_started"}}
{"timestamp":"2026-07-16T12:00:03.500Z","type":"response_item","payload":{"type":"custom_tool_call"}}
{"timestamp":"2026-07-16T12:00:04.000Z","type":"response_item","payload":{"type":"custom_tool_call_output"}}
{"timestamp":"2026-07-16T12:00:04.000Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"output_tokens":120}}}}
"#;
        let result = parse_codex_tail(text);
        assert_eq!(result.status, "generating");
        assert_eq!(result.output_tokens, Some(120));
        assert_eq!(result.tokens_per_second, Some(30.0));
    }

    #[test]
    fn claude_reports_only_the_latest_completed_response() {
        let text = r#"
{"timestamp":"2026-07-16T12:00:00.000Z","type":"user","message":{"role":"user"}}
{"timestamp":"2026-07-16T12:00:05.000Z","type":"assistant","message":{"id":"message-1","role":"assistant","stop_reason":"end_turn","usage":{"output_tokens":100}}}
{"timestamp":"2026-07-16T12:01:00.000Z","type":"user","message":{"role":"user"}}
{"timestamp":"2026-07-16T12:01:02.000Z","type":"assistant","message":{"id":"message-2","role":"assistant","stop_reason":"end_turn","usage":{"output_tokens":80}}}
"#;
        let result = parse_claude_tail(text);
        assert_eq!(result.status, "recent");
        assert_eq!(result.output_tokens, Some(80));
        assert_eq!(result.tokens_per_second, Some(40.0));
        assert_eq!(result.recent_samples.len(), 2);
    }

    #[test]
    fn claude_deduplicates_replayed_usage_after_tool_results() {
        let text = r#"
{"timestamp":"2026-07-16T12:00:00.000Z","type":"user","message":{"role":"user","content":[{"type":"text","text":"request"}]}}
{"timestamp":"2026-07-16T12:01:00.000Z","type":"assistant","message":{"id":"message-1","model":"claude-fable-5","role":"assistant","stop_reason":"tool_use","usage":{"output_tokens":3000}}}
{"timestamp":"2026-07-16T12:01:00.100Z","type":"user","message":{"role":"user","content":[{"type":"tool_result"}]}}
{"timestamp":"2026-07-16T12:01:01.000Z","type":"assistant","message":{"id":"message-1","model":"claude-fable-5","role":"assistant","stop_reason":"tool_use","usage":{"output_tokens":3000}}}
{"timestamp":"2026-07-16T12:01:20.100Z","type":"assistant","message":{"id":"message-2","model":"claude-fable-5","role":"assistant","stop_reason":"end_turn","usage":{"output_tokens":800}}}
"#;
        let result = parse_claude_tail(text);
        assert_eq!(result.recent_samples.len(), 2);
        assert_eq!(result.recent_samples[0].tokens_per_second, 50.0);
        assert_eq!(result.recent_samples[1].tokens_per_second, 40.0);
        assert_eq!(result.tokens_per_second, Some(40.0));
    }

    /// Shapes taken from a real `~/.grok/sessions/<cwd>/<id>/updates.jsonl`.
    #[test]
    fn grok_measures_generation_against_reported_api_duration() {
        let text = r#"
{"timestamp":1784815000,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"user_message_chunk"}},"_meta":{"agentTimestampMs":1784815000000}}
{"timestamp":1784815127,"method":"_x.ai/session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"turn_completed","prompt_id":"p1","stop_reason":"end_turn","usage":{"inputTokens":237473,"outputTokens":2000,"totalTokens":239473,"reasoningTokens":1500,"modelCalls":10,"apiDurationMs":100000,"modelUsage":{"grok-4.5":{"outputTokens":2000}}}}},"_meta":{"agentTimestampMs":1784815127000}}
"#;
        let result = parse_grok_tail(text);
        assert_eq!(result.status, "recent");
        assert_eq!(result.output_tokens, Some(2000));
        assert_eq!(result.tokens_per_second, Some(20.0));
        assert_eq!(result.updated_at_ms, Some(1784815127000));
        assert_eq!(
            result.recent_samples[0].model.as_deref(),
            Some("grok-4.5"),
            "a single-model turn should name its model"
        );
        assert_eq!(
            result.recent_samples[0].duration_ms, 100_000,
            "the turn's own apiDurationMs is the duration, not the wall clock \
             between the prompt and the completion"
        );
    }

    /// A prompt with no `turn_completed` after it is still being answered.
    #[test]
    fn grok_reports_generation_while_a_turn_is_open() {
        let text = r#"
{"timestamp":1784815127,"method":"_x.ai/session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"turn_completed","stop_reason":"end_turn","usage":{"outputTokens":500,"apiDurationMs":25000,"modelUsage":{"grok-4.5":{}}}}},"_meta":{"agentTimestampMs":1784815127000}}
{"timestamp":1784815200,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"user_message_chunk"}},"_meta":{"agentTimestampMs":1784815200000}}
{"timestamp":1784815205,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call"}},"_meta":{"agentTimestampMs":1784815205000}}
"#;
        let result = parse_grok_tail(text);
        assert_eq!(result.status, "generating");
        assert_eq!(
            result.tokens_per_second,
            Some(20.0),
            "the last completed turn's rate stays on screen while the next runs"
        );
    }

    /// Never invent a measurement: a turn that logged no usage yields nothing.
    #[test]
    fn grok_without_usage_reports_unavailable() {
        let text = r#"
{"timestamp":1784815127,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"turn_completed","stop_reason":"end_turn"}},"_meta":{"agentTimestampMs":1784815127000}}
{"timestamp":1784815128,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"tool_call_update"}},"_meta":{"agentTimestampMs":1784815128000}}
"#;
        let result = parse_grok_tail(text);
        assert_eq!(result.status, "unavailable");
        assert_eq!(result.tokens_per_second, None);
        assert!(result.recent_samples.is_empty());
    }

    /// Grok's own accounting: `totalTokens` is input + output, and reasoning is
    /// a subset of output. If that ever changes the rate would silently double,
    /// so the assumption is pinned here.
    #[test]
    fn grok_output_tokens_is_the_whole_generated_count() {
        let text = r#"
{"timestamp":1784815127,"method":"session/update","params":{"sessionId":"s1","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":30266,"outputTokens":532,"totalTokens":30798,"reasoningTokens":201,"apiDurationMs":26600}}},"_meta":{"agentTimestampMs":1784815127000}}
"#;
        let result = parse_grok_tail(text);
        assert_eq!(result.output_tokens, Some(532));
        assert_eq!(result.tokens_per_second, Some(20.0));
        assert_eq!(
            result.recent_samples[0].model, None,
            "a turn with no modelUsage must not guess a model name"
        );
    }

    #[test]
    fn kimi_pairs_turn_usage_with_its_request_for_a_real_duration() {
        let text = r#"
{"type":"llm.request","model":"kimi-code/kimi-for-coding","time":1760000000000}
{"type":"usage.record","usageScope":"turn","model":"__kimi_env_model__","usage":{"inputOther":500,"output":200},"time":1760000002000}
{"type":"usage.record","usageScope":"session","usage":{"inputOther":9999,"output":9999},"time":1760000002100}
"#;
        let result = parse_kimi_tail(text);
        assert_eq!(result.status, "recent");
        assert_eq!(result.output_tokens, Some(200));
        assert_eq!(result.tokens_per_second, Some(100.0), "200 tokens / 2s");
        assert_eq!(
            result.recent_samples[0].model.as_deref(),
            Some("kimi-for-coding"),
            "the request row names the model; the symbolic env model does not"
        );
    }

    #[test]
    fn kimi_without_a_request_marker_yields_no_rate() {
        let text = r#"
{"timestamp":1000.0,"message":{"type":"StatusUpdate","payload":{"message_id":"m1","token_usage":{"input_other":100,"output":10}}}}
"#;
        let result = parse_kimi_tail(text);
        assert_eq!(result.status, "unavailable");
        assert_eq!(result.tokens_per_second, None);
    }

    #[test]
    fn opencode_message_duration_comes_from_created_and_completed_times() {
        let text = r#"{"id":"msg_1","role":"assistant","providerID":"opencode","modelID":"claude-sonnet-4-6","tokens":{"input":100,"output":150,"cache":{"read":0,"write":0}},"time":{"created":1000.0,"completed":2500.0}}"#;
        let sample = parse_opencode_message(text).expect("sample");
        assert_eq!(sample.output_tokens, 150);
        assert_eq!(sample.duration_ms, 1500);
        assert!((sample.tokens_per_second - 100.0).abs() < 1e-9);
        assert_eq!(sample.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn opencode_non_assistant_or_foreign_provider_rows_are_rejected() {
        let user_row = r#"{"role":"user","providerID":"opencode","tokens":{"output":10},"time":{"created":0,"completed":1000}}"#;
        assert!(parse_opencode_message(user_row).is_none());
        let other_provider = r#"{"role":"assistant","providerID":"opencode-go","tokens":{"output":10},"time":{"created":0,"completed":1000}}"#;
        assert!(parse_opencode_message(other_provider).is_none());
        let no_duration = r#"{"role":"assistant","providerID":"opencode","tokens":{"output":10},"time":{"created":1000.0}}"#;
        assert!(parse_opencode_message(no_duration).is_none());
    }

    #[test]
    fn opencode_db_rows_build_a_chronological_speed_snapshot() {
        use codexbar::providers::opencodego::MessageSpeed;
        // Rows arrive newest-first, as the SQL orders them.
        let rows = vec![
            MessageSpeed {
                created_ms: 20_000,
                completed_ms: 21_000,
                output_tokens: 200,
                model: Some("m-new".to_string()),
            },
            MessageSpeed {
                created_ms: 10_000,
                completed_ms: 12_000,
                output_tokens: 100,
                model: Some("m-old".to_string()),
            },
        ];
        let speed = opencode_speed_from_rows("opencode", &rows).expect("samples");
        assert_eq!(speed.status, "recent");
        assert_eq!(speed.output_tokens, Some(200));
        assert!((speed.tokens_per_second.unwrap() - 200.0).abs() < 1e-9);
        assert_eq!(speed.recent_samples.len(), 2);
        assert_eq!(speed.recent_samples[0].model.as_deref(), Some("m-old"));
        assert_eq!(speed.recent_samples[1].model.as_deref(), Some("m-new"));
        assert_eq!(speed.recent_samples[1].duration_ms, 1_000);

        // No usable row means no reading, never a fabricated zero.
        assert!(opencode_speed_from_rows("opencode", &[]).is_none());
        assert!(
            opencode_speed_from_rows(
                "opencode",
                &[MessageSpeed {
                    created_ms: 1_000,
                    completed_ms: 1_000,
                    output_tokens: 0,
                    model: None,
                }],
            )
            .is_none()
        );
    }

    #[test]
    fn kimi_time_accepts_epoch_seconds_as_well_as_milliseconds() {
        let text = r#"
{"type":"llm.request","model":"kimi-for-coding","time":1760000000}
{"type":"usage.record","usageScope":"turn","model":"kimi-for-coding","usage":{"output":200},"time":1760000002}
"#;
        let result = parse_kimi_tail(text);
        assert_eq!(result.status, "recent");
        assert_eq!(result.output_tokens, Some(200));
        assert!(
            (result.tokens_per_second.unwrap() - 100.0).abs() < 1e-9,
            "200 tokens / 2s: {result:?}"
        );
    }
}
