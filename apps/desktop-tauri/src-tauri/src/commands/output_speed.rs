use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

const MAX_TAIL_BYTES: u64 = 1024 * 1024;
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
}

#[tauri::command]
pub fn get_output_speed_snapshot() -> OutputSpeedSnapshot {
    let settings = codexbar::settings::Settings::load();
    OutputSpeedSnapshot {
        codex: newest_codex_session(&settings.codex_custom_sessions_dirs)
            .and_then(|path| read_tail(&path).ok())
            .map(|text| parse_codex_tail(&text))
            .unwrap_or_else(|| ProviderOutputSpeed::unavailable("codex")),
        claude: newest_file_in_roots(&claude_project_roots())
            .and_then(|path| read_tail(&path).ok())
            .map(|text| parse_claude_tail(&text))
            .unwrap_or_else(|| ProviderOutputSpeed::unavailable("claude")),
    }
}

fn newest_codex_session(custom_dirs: &[String]) -> Option<PathBuf> {
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
    newest_file_in_roots(&roots)
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

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

fn newest_file_in_roots(roots: &[PathBuf]) -> Option<PathBuf> {
    let mut newest: Option<(SystemTime, PathBuf)> = None;
    for root in roots {
        visit_jsonl_files(root, &mut |path, modified| {
            if newest
                .as_ref()
                .is_none_or(|(current, _)| modified > *current)
            {
                newest = Some((modified, path.to_path_buf()));
            }
        });
    }
    newest.map(|(_, path)| path)
}

fn visit_jsonl_files(root: &Path, visit: &mut impl FnMut(&Path, SystemTime)) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_dir() {
            visit_jsonl_files(&path, visit);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "jsonl")
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
}
