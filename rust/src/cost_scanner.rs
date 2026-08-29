//! Local cost-usage scanner for Codex, Claude and Grok
//!
//! Scans local JSONL log files to aggregate token usage and calculate costs.
//!
//! Note: this path always full-walks session files. The disk-cache /
//! [`crate::core::CostScanOptions`] debounce API in `jsonl_scanner` is not
//! wired here yet (upstream #2089); app-level TTL still owns refresh pacing.

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

#[cfg(test)]
use crate::codex_costs::scan_codex_file_cost;
use crate::codex_costs::{
    add_codex_records_to_summary, codex_period_start, codex_scan_dates,
    scan_codex_file_cost_for_range,
};
use crate::codex_sessions::{codex_sessions_dir_candidates, default_wsl_roots};
use crate::core::{CostUsageDayRange, CostUsagePricing, JsonlScanner};
use crate::settings::Settings;

/// Cost summary from scanning local logs
#[derive(Debug, Clone, Default)]
pub struct CostSummary {
    /// Total cost in USD for the period
    pub total_cost_usd: f64,
    /// Total input tokens
    pub input_tokens: u64,
    /// Total output tokens
    pub output_tokens: u64,
    /// Total cached input tokens
    pub cached_tokens: u64,
    /// Number of sessions/conversations scanned
    pub sessions_count: u32,
    /// Cost breakdown by model
    pub by_model: HashMap<String, f64>,
    /// Token breakdown by model
    pub by_model_tokens: HashMap<String, ModelTokenCounts>,
    /// Codex cost split by speed/tier when local logs expose it.
    pub by_speed: HashMap<String, f64>,
    /// Codex token split by speed/tier when local logs expose it.
    pub by_speed_tokens: HashMap<String, ModelTokenCounts>,
    /// Model IDs that were priced with fallback rates because no canonical rate is available.
    pub unknown_models: HashSet<String>,
    /// Period start date
    pub period_start: Option<NaiveDate>,
    /// Period end date
    pub period_end: Option<NaiveDate>,
}
/// Per-model token counts
#[derive(Debug, Clone, Default)]
pub struct ModelTokenCounts {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
}

impl ModelTokenCounts {
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

impl CostSummary {
    pub fn format_total(&self) -> String {
        format!("${:.2}", self.total_cost_usd)
    }
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

/// Fallback Claude model used when a scanned model isn't in the canonical
/// pricing table (unknown or retired IDs). Prices as Sonnet 4.6.
const FALLBACK_CLAUDE_MODEL: &str = "claude-sonnet-4-6";

/// Claude cost calculation for the usage scanner.
///
/// Per-token rates come from the canonical `CostUsagePricing::claude_cost_usd`
/// table (the single source of truth for Claude pricing). The only
/// scanner-specific piece is the one-hour cache-write premium, which the
/// canonical cost function doesn't model: one-hour cache writes bill at 2x the
/// input rate.
struct ClaudePricing;

impl ClaudePricing {
    fn cost_usd_with_cache_ttl(
        model: &str,
        input: u64,
        cache_create: u64,
        cache_create_1h: u64,
        cache_read: u64,
        output: u64,
    ) -> f64 {
        let cache_create_1h = cache_create_1h.min(cache_create);
        let cache_create_5m = cache_create.saturating_sub(cache_create_1h);

        // Standard buckets (input, cache-read, 5-minute cache-write, output),
        // including any long-context tiering, come from the canonical table.
        // Unknown/retired models fall back to Sonnet pricing.
        let clamp = |v: u64| v.min(i32::MAX as u64) as i32;
        let base = CostUsagePricing::claude_cost_usd(
            model,
            clamp(input),
            clamp(cache_read),
            clamp(cache_create_5m),
            clamp(output),
        )
        .or_else(|| {
            CostUsagePricing::claude_cost_usd(
                FALLBACK_CLAUDE_MODEL,
                clamp(input),
                clamp(cache_read),
                clamp(cache_create_5m),
                clamp(output),
            )
        })
        .unwrap_or(0.0);

        // Scanner-specific: one-hour cache writes bill at 2x the input rate.
        let input_rate = CostUsagePricing::claude_input_cost_per_token(model)
            .or_else(|| CostUsagePricing::claude_input_cost_per_token(FALLBACK_CLAUDE_MODEL))
            .unwrap_or(0.0);

        base + (cache_create_1h as f64) * input_rate * 2.0
    }
}

/// JSONL event structures for Codex
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CodexEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    event_msg: Option<CodexEventMsg>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct CodexEventMsg {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    input_tokens: Option<u64>,
    cached_input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

/// JSONL event structures for Claude transcripts. Unknown fields are
/// ignored, so lines that are not assistant usage events still parse.
#[derive(Debug, Deserialize)]
struct ClaudeEvent {
    #[serde(rename = "type")]
    event_type: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "requestId", alias = "request_id")]
    request_id: Option<String>,
    message: Option<ClaudeMessage>,
}

impl ClaudeEvent {
    fn parsed_timestamp(&self) -> Option<DateTime<Utc>> {
        let timestamp = self.timestamp.as_deref()?;
        DateTime::parse_from_rfc3339(timestamp)
            .ok()
            .map(|ts| ts.with_timezone(&Utc))
    }
}

#[derive(Debug, Deserialize)]
struct ClaudeMessage {
    id: Option<String>,
    model: Option<String>,
    usage: Option<ClaudeUsage>,
}

#[derive(Debug, Deserialize)]
struct ClaudeUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
    cache_creation: Option<ClaudeCacheCreation>,
}

impl ClaudeUsage {
    /// One-hour cache-write tokens, clamped to the total cache-write count.
    fn one_hour_cache_creation_tokens(&self, total: u64) -> u64 {
        self.cache_creation
            .as_ref()
            .and_then(|cache_creation| cache_creation.ephemeral_1h_input_tokens)
            .unwrap_or(0)
            .min(total)
    }
}

/// TTL breakdown of cache writes reported by the API.
#[derive(Debug, Deserialize)]
struct ClaudeCacheCreation {
    ephemeral_1h_input_tokens: Option<u64>,
}

#[derive(Debug)]
struct ClaudeUsageRecord {
    model: String,
    timestamp: Option<DateTime<Utc>>,
    dedup_key: Option<String>,
    input: u64,
    output: u64,
    cache_create: u64,
    cache_read: u64,
    cost: f64,
}

/// `costUsdTicks` is USD scaled by 1e9.
///
/// The field states no unit beyond its name, so the scale is pinned by
/// magnitude against real logs: one observed turn reports 12_971_000_000 ticks
/// for 2.7M tokens on `grok-4.5-build`. At 1e9 that is $12.97, the right order
/// for that many tokens; at 1e6 it would be $12,971 for a single turn, which no
/// pricing makes sense of. Isolated here so a correction is one constant, not a
/// hunt through the scanner.
const GROK_COST_TICKS_PER_USD: f64 = 1e9;

/// One `session/update` line of a Grok `updates.jsonl`.
#[derive(Debug, Deserialize)]
struct GrokUpdateLine {
    /// Unix seconds. The turn is bucketed by this, not by the file's mtime.
    timestamp: Option<i64>,
    params: Option<GrokUpdateParams>,
}

#[derive(Debug, Deserialize)]
struct GrokUpdateParams {
    update: Option<GrokUpdate>,
    #[serde(rename = "_meta")]
    meta: Option<GrokUpdateMeta>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GrokUpdate {
    session_update: Option<String>,
    usage: Option<GrokUsage>,
}

#[derive(Debug, Deserialize)]
struct GrokUpdateMeta {
    #[serde(rename = "eventId")]
    event_id: Option<String>,
}

/// Token and cost totals for a turn, or for one model within a turn.
///
/// `cached_read_tokens` is a SUBSET of `input_tokens`, unlike Claude's cache
/// fields which sit alongside its input count: the logs satisfy
/// `inputTokens + outputTokens == totalTokens` exactly while
/// `cachedReadTokens < inputTokens`. Adding it to the input total would
/// double-count every cached read, which on these logs is the majority of all
/// input.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GrokUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cached_read_tokens: Option<u64>,
    cost_usd_ticks: Option<f64>,
    model_usage: Option<HashMap<String, GrokUsage>>,
}

impl GrokUsage {
    fn input(&self) -> u64 {
        self.input_tokens.unwrap_or(0)
    }
    fn output(&self) -> u64 {
        self.output_tokens.unwrap_or(0)
    }
    fn cached(&self) -> u64 {
        self.cached_read_tokens.unwrap_or(0)
    }
    fn cost_usd(&self) -> f64 {
        self.cost_usd_ticks.unwrap_or(0.0) / GROK_COST_TICKS_PER_USD
    }
}

/// Cost usage scanner
pub struct CostScanner {
    days: u32,
}

impl CostScanner {
    /// Create a new scanner for the last N days
    pub fn new(days: u32) -> Self {
        Self { days }
    }

    /// Scan Codex local logs
    pub fn scan_codex(&self) -> CostSummary {
        self.scan_codex_with_cancel(None)
    }

    /// Scan Codex local logs, stopping early when the caller cancels the scan.
    pub fn scan_codex_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        let mut summary = CostSummary::default();
        let today = Local::now().date_naive();
        let start_date = codex_period_start(today, self.days);
        let range = CostUsageDayRange::new(start_date, today);

        summary.period_start = Some(start_date);
        summary.period_end = Some(today);

        for sessions_dir in self.get_codex_sessions_dirs() {
            if is_cancelled(cancel) {
                break;
            }
            if sessions_dir.exists() {
                self.scan_codex_sessions_dir(&sessions_dir, &range, &mut summary, cancel);
            }
        }

        summary
    }

    /// Scan Claude local logs
    pub fn scan_claude(&self) -> CostSummary {
        self.scan_claude_with_cancel(None)
    }

    /// Scan Claude local logs, stopping early when the caller cancels the scan.
    pub fn scan_claude_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        let projects_dir = self.get_claude_projects_dir();
        if !projects_dir.exists() {
            return CostSummary::default();
        }

        let mut summary = CostSummary::default();
        let today = Utc::now().date_naive();
        let start_date = today - Duration::days(self.days as i64);
        let cutoff = Utc::now() - Duration::days(self.days as i64);

        summary.period_start = Some(start_date);
        summary.period_end = Some(today);

        // Walk through projects directory, de-duplicating usage records
        // that appear across multiple files.
        let mut seen = HashSet::new();
        let mut handle_file = |path: &Path| {
            let counted =
                for_each_claude_usage_record(path, &cutoff, &mut seen, cancel, |record| {
                    add_claude_record_to_summary(&mut summary, record);
                });
            if counted > 0 {
                summary.sessions_count += 1;
            }
        };
        self.walk_claude_files(&projects_dir, &cutoff, cancel, &mut handle_file);

        summary
    }

    /// Scan Grok local logs
    pub fn scan_grok(&self) -> CostSummary {
        self.scan_grok_with_cancel(None)
    }

    /// Scan Grok local logs, stopping early when the caller cancels the scan.
    ///
    /// The Grok CLI records one `turn_completed` event per assistant turn in
    /// each session's `updates.jsonl`, carrying token counts, a per-model
    /// breakdown and a cost. Those turn totals are **not** cumulative — that was
    /// checked against real logs before this was written, because summing
    /// cumulative snapshots would have inflated every figure. In three
    /// multi-turn sessions, zero had non-decreasing totals.
    pub fn scan_grok_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        let sessions_dir = self.get_grok_sessions_dir();
        if !sessions_dir.exists() {
            return CostSummary::default();
        }

        let mut summary = CostSummary::default();
        let today = Utc::now().date_naive();
        summary.period_start = Some(today - Duration::days(self.days as i64));
        summary.period_end = Some(today);
        let cutoff = Utc::now() - Duration::days(self.days as i64);

        // One event can be replayed into a resumed session's log, so the same
        // turn must not be counted twice.
        let mut seen = HashSet::new();
        let mut handle_file = |path: &Path| {
            let counted = for_each_grok_turn(path, &cutoff, &mut seen, cancel, |turn| {
                add_grok_turn_to_summary(&mut summary, turn);
            });
            if counted > 0 {
                summary.sessions_count += 1;
            }
        };
        self.walk_grok_files(&sessions_dir, &cutoff, cancel, &mut handle_file);

        summary
    }

    /// Root of the Grok CLI's per-session logs.
    ///
    /// `GROK_HOME` is honoured the same way [`crate::providers::GrokProvider`]
    /// honours it for `auth.json`, so a relocated CLI stays readable by both.
    fn get_grok_sessions_dir(&self) -> PathBuf {
        if let Ok(grok_home) = std::env::var("GROK_HOME") {
            let trimmed = grok_home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("sessions");
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".grok")
            .join("sessions")
    }

    /// Walk the session tree for the one file per session that carries usage.
    ///
    /// A session directory holds a dozen files — chat history, prompts, rewind
    /// points, a summary — and only `updates.jsonl` has the turn events. Reading
    /// the rest would be pure I/O for nothing; `chat_history.jsonl` alone is
    /// often the largest file in the directory.
    fn walk_grok_files<F>(
        &self,
        dir: &Path,
        cutoff: &DateTime<Utc>,
        cancel: Option<&AtomicBool>,
        on_file: &mut F,
    ) where
        F: FnMut(&Path),
    {
        if is_cancelled(cancel) {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            if is_cancelled(cancel) {
                break;
            }
            let path = entry.path();
            if path.is_dir() {
                self.walk_grok_files(&path, cutoff, cancel, on_file);
            } else if path.file_name().is_some_and(|name| name == "updates.jsonl")
                && let Ok(metadata) = fs::metadata(&path)
                && let Ok(modified) = metadata.modified()
            {
                let modified_dt: DateTime<Utc> = modified.into();
                if modified_dt >= *cutoff {
                    on_file(&path);
                }
            }
        }
    }

    fn get_codex_sessions_dirs(&self) -> Vec<PathBuf> {
        let settings = Settings::load();
        let codex_home = std::env::var("CODEX_HOME").ok();
        codex_sessions_dir_candidates(
            dirs::home_dir(),
            codex_home,
            &settings.codex_custom_sessions_dirs,
            &default_wsl_roots(),
        )
    }

    fn scan_codex_sessions_dir(
        &self,
        sessions_dir: &Path,
        _range: &CostUsageDayRange,
        summary: &mut CostSummary,
        cancel: Option<&AtomicBool>,
    ) {
        // Codex appends to *resumed* sessions, so a file created weeks ago (and
        // filed under its original start-date folder, e.g. sessions/2026/05/21/)
        // can still hold token records from today. Iterating only the folders
        // whose date falls inside the window would miss all of those. Instead
        // walk every session file and parse any modified recently enough to
        // possibly contain in-range records; `parse_codex_file` then filters the
        // individual records back to the exact day range by their timestamp.
        let today = Local::now().date_naive();
        let start_date = codex_period_start(today, self.days);
        // One extra day of slack absorbs UTC-vs-local and clock skew; a file
        // untouched since before this can't hold a record inside the window.
        let mtime_cutoff = start_date - Duration::days(1);
        self.walk_codex_files(sessions_dir, mtime_cutoff, summary, cancel);
    }

    fn walk_codex_files(
        &self,
        dir: &Path,
        mtime_cutoff: NaiveDate,
        summary: &mut CostSummary,
        cancel: Option<&AtomicBool>,
    ) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if is_cancelled(cancel) {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                self.walk_codex_files(&path, mtime_cutoff, summary, cancel);
            } else if path.extension().is_some_and(|e| e == "jsonl") {
                // Skip files untouched since before the window — their last
                // write predates the earliest record we'd keep.
                let recent = entry
                    .metadata()
                    .and_then(|meta| meta.modified())
                    .map(|modified| {
                        DateTime::<Utc>::from(modified)
                            .with_timezone(&Local)
                            .date_naive()
                            >= mtime_cutoff
                    })
                    .unwrap_or(true);
                if recent {
                    self.parse_codex_file(&path, summary, cancel);
                }
            }
        }
    }

    fn get_claude_projects_dir(&self) -> PathBuf {
        if let Ok(claude_config) = std::env::var("CLAUDE_CONFIG_DIR") {
            let trimmed = claude_config.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("projects");
            }
        }

        // Try ~/.claude/projects first
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let claude_dir = home.join(".claude").join("projects");
        if claude_dir.exists() {
            return claude_dir;
        }

        // Fallback to ~/.config/claude/projects
        home.join(".config").join("claude").join("projects")
    }

    fn parse_codex_file(
        &self,
        path: &Path,
        summary: &mut CostSummary,
        cancel: Option<&AtomicBool>,
    ) {
        if is_cancelled(cancel) {
            return;
        }
        let today = Local::now().date_naive();
        let start_date = codex_period_start(today, self.days);
        let range = CostUsageDayRange::new(start_date, today);
        let parse_result = match JsonlScanner::parse_codex_file(path, &range, 0, None, None) {
            Ok(result) => result,
            Err(_) => return,
        };

        let (session_cost, has_tokens) =
            add_codex_records_to_summary(summary, &parse_result.records, &range);

        if has_tokens {
            summary.total_cost_usd += session_cost;
            summary.sessions_count += 1;
        }
    }

    fn walk_claude_files<F>(
        &self,
        dir: &Path,
        cutoff: &DateTime<Utc>,
        cancel: Option<&AtomicBool>,
        on_file: &mut F,
    ) where
        F: FnMut(&Path),
    {
        if is_cancelled(cancel) {
            return;
        }
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            if is_cancelled(cancel) {
                break;
            }
            let path = entry.path();
            if path.is_dir() {
                self.walk_claude_files(&path, cutoff, cancel, on_file);
            } else if path.extension().is_some_and(|e| e == "jsonl") {
                // Check file modification time
                if let Ok(metadata) = fs::metadata(&path)
                    && let Ok(modified) = metadata.modified()
                {
                    let modified_dt: DateTime<Utc> = modified.into();
                    if modified_dt >= *cutoff {
                        on_file(&path);
                    }
                }
            }
        }
    }
}

/// Stream the de-duplicated, in-window usage records from one transcript
/// file into `on_record`. Both the summary scan and the daily-history scan
/// consume this single reader, so Claude log semantics live in one place.
/// Returns the number of records consumed, so callers can tell whether the
/// file contributed anything.
fn for_each_claude_usage_record<F>(
    path: &Path,
    cutoff: &DateTime<Utc>,
    seen: &mut HashSet<String>,
    cancel: Option<&AtomicBool>,
    mut on_record: F,
) -> usize
where
    F: FnMut(&ClaudeUsageRecord),
{
    let Ok(file) = File::open(path) else {
        return 0;
    };

    let mut counted = 0;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if is_cancelled(cancel) {
            break;
        }
        if let Ok(event) = serde_json::from_str::<ClaudeEvent>(&line)
            && let Some(record) = claude_usage_record_from_event(&event)
            && should_count_claude_record(&record, cutoff, seen)
        {
            counted += 1;
            on_record(&record);
        }
    }
    counted
}

fn claude_usage_record_from_event(event: &ClaudeEvent) -> Option<ClaudeUsageRecord> {
    if event.event_type.as_deref() != Some("assistant") {
        return None;
    }

    let message = event.message.as_ref()?;
    let usage = message.usage.as_ref()?;
    let model = message.model.as_deref().unwrap_or("claude-3-5-sonnet");

    let input = usage.input_tokens.unwrap_or(0);
    let output = usage.output_tokens.unwrap_or(0);
    let cache_create = usage.cache_creation_input_tokens.unwrap_or(0);
    let cache_read = usage.cache_read_input_tokens.unwrap_or(0);

    if input == 0 && output == 0 && cache_create == 0 && cache_read == 0 {
        return None;
    }

    let cache_create_1h = usage.one_hour_cache_creation_tokens(cache_create);
    let cost = ClaudePricing::cost_usd_with_cache_ttl(
        model,
        input,
        cache_create,
        cache_create_1h,
        cache_read,
        output,
    );

    Some(ClaudeUsageRecord {
        model: model.to_string(),
        timestamp: event.parsed_timestamp(),
        dedup_key: claude_usage_dedup_key(message.id.as_deref(), event.request_id.as_deref()),
        input,
        output,
        cache_create,
        cache_read,
        cost,
    })
}

fn claude_usage_dedup_key(message_id: Option<&str>, request_id: Option<&str>) -> Option<String> {
    match (message_id, request_id) {
        (Some(message_id), Some(request_id)) => Some(format!("{message_id}:{request_id}")),
        (Some(message_id), None) => Some(format!("message:{message_id}")),
        (None, Some(request_id)) => Some(format!("request:{request_id}")),
        (None, None) => None,
    }
}

fn should_count_claude_record(
    record: &ClaudeUsageRecord,
    cutoff: &DateTime<Utc>,
    seen: &mut HashSet<String>,
) -> bool {
    if let Some(timestamp) = record.timestamp
        && timestamp < *cutoff
    {
        return false;
    }

    if let Some(key) = &record.dedup_key
        && !seen.insert(key.clone())
    {
        return false;
    }

    true
}

fn add_claude_record_to_summary(summary: &mut CostSummary, record: &ClaudeUsageRecord) {
    if CostUsagePricing::claude_cost_usd(&record.model, 0, 0, 0, 0).is_none() {
        summary.unknown_models.insert(record.model.clone());
    }

    summary.input_tokens += record.input;
    summary.output_tokens += record.output;
    summary.cached_tokens += record.cache_create + record.cache_read;
    summary.total_cost_usd += record.cost;

    *summary.by_model.entry(record.model.clone()).or_insert(0.0) += record.cost;

    let model_tokens = summary
        .by_model_tokens
        .entry(record.model.clone())
        .or_default();
    model_tokens.input_tokens += record.input;
    model_tokens.output_tokens += record.output;
    model_tokens.cached_tokens += record.cache_create + record.cache_read;
}

/// Add one usage record to the per-day cost buckets, keyed by the record's
/// own timestamp in the local timezone. Records outside the initialized
/// date range (or without a timestamp) are ignored.
fn add_claude_record_to_daily_costs(
    daily_costs: &mut HashMap<String, f64>,
    record: &ClaudeUsageRecord,
) {
    let Some(timestamp) = record.timestamp else {
        return;
    };
    let date_str = timestamp
        .with_timezone(&Local)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    if let Some(cost) = daily_costs.get_mut(&date_str) {
        *cost += record.cost;
    }
}

/// Stream the de-duplicated, in-window `turn_completed` events of one Grok
/// `updates.jsonl` into `on_turn`, returning how many were counted.
fn for_each_grok_turn<F>(
    path: &Path,
    cutoff: &DateTime<Utc>,
    seen: &mut HashSet<String>,
    cancel: Option<&AtomicBool>,
    mut on_turn: F,
) -> u32
where
    F: FnMut(&GrokUsage),
{
    let Ok(file) = File::open(path) else {
        return 0;
    };

    let mut counted = 0;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if is_cancelled(cancel) {
            break;
        }
        // Every other line in this file is a tool call, a diff or a screenshot
        // payload — some of them very large. Rejecting on the raw bytes avoids
        // paying serde for lines that cannot match.
        if !line.contains("turn_completed") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<GrokUpdateLine>(&line) else {
            continue;
        };
        let Some(params) = parsed.params else { continue };
        let Some(update) = params.update else { continue };
        if update.session_update.as_deref() != Some("turn_completed") {
            continue;
        }
        let Some(usage) = update.usage else { continue };

        if let Some(timestamp) = parsed.timestamp
            && let Some(recorded) = DateTime::from_timestamp(timestamp, 0)
            && recorded < *cutoff
        {
            continue;
        }

        if let Some(event_id) = params.meta.and_then(|meta| meta.event_id)
            && !seen.insert(event_id)
        {
            continue;
        }

        on_turn(&usage);
        counted += 1;
    }

    counted
}

fn add_grok_turn_to_summary(summary: &mut CostSummary, turn: &GrokUsage) {
    summary.input_tokens += turn.input();
    summary.output_tokens += turn.output();
    summary.cached_tokens += turn.cached();
    summary.total_cost_usd += turn.cost_usd();

    // The turn names its own models, so no model table has to be kept in step
    // with whatever x.ai ships next. A turn with no breakdown still contributes
    // its totals above; it simply cannot say which model earned them, and
    // inventing a name for it would be worse than leaving it unattributed.
    for (model, model_usage) in turn.model_usage.iter().flatten() {
        *summary.by_model.entry(model.clone()).or_insert(0.0) += model_usage.cost_usd();
        let tokens = summary.by_model_tokens.entry(model.clone()).or_default();
        tokens.input_tokens += model_usage.input();
        tokens.output_tokens += model_usage.output();
        tokens.cached_tokens += model_usage.cached();
    }
}

/// Check if any cost usage sources are available
#[allow(dead_code)]
pub fn has_cost_usage_sources() -> bool {
    let scanner = CostScanner::new(1);
    scanner
        .get_codex_sessions_dirs()
        .iter()
        .any(|dir| dir.exists())
        || scanner.get_claude_projects_dir().exists()
}

/// Get daily cost history for the last N days.
/// Returns Vec of (date_string, cost_usd) sorted by date.
pub fn get_daily_cost_history(provider: &str, days: u32) -> Vec<(String, f64)> {
    get_daily_cost_history_with_budget(provider, days, None, None).0
}

/// Cancellation/deadline-aware daily history used by the desktop chart path.
///
/// The old helper had no cancellation boundary and could walk a user's entire
/// local transcript tree while a chart command (or its unit test) waited.  Keep
/// the legacy API above for callers that intentionally want an unbounded scan,
/// but give interactive callers a bounded path.  The boolean reports whether
/// the scan stopped before visiting the complete requested window.
pub fn get_daily_cost_history_with_budget(
    provider: &str,
    days: u32,
    cancel: Option<&AtomicBool>,
    deadline: Option<Instant>,
) -> (Vec<(String, f64)>, bool) {
    let stopped = || is_cancelled(cancel) || deadline.is_some_and(|limit| Instant::now() >= limit);
    let scanner = CostScanner::new(days);
    let today = Local::now().date_naive();
    let mut daily_costs: HashMap<String, f64> = HashMap::new();

    // Initialize all days with 0
    for days_ago in 0..days {
        let date = today - Duration::days(days_ago as i64);
        let date_str = date.format("%Y-%m-%d").to_string();
        daily_costs.insert(date_str, 0.0);
    }

    match provider {
        "codex" => {
            // Scan Codex logs by day across Windows and WSL session roots.
            let sessions_dirs = scanner.get_codex_sessions_dirs();
            for days_ago in 0..days {
                if stopped() {
                    break;
                }
                let date = today - Duration::days(days_ago as i64);
                let date_str = date.format("%Y-%m-%d").to_string();
                let range = CostUsageDayRange::new(date, date);
                let mut day_cost = 0.0;

                for sessions_dir in sessions_dirs.iter().filter(|dir| dir.exists()) {
                    if stopped() {
                        break;
                    }
                    for scan_date in codex_scan_dates(&range) {
                        if stopped() {
                            break;
                        }
                        let year = scan_date.format("%Y").to_string();
                        let month = scan_date.format("%m").to_string();
                        let day = scan_date.format("%d").to_string();
                        let day_dir = sessions_dir.join(&year).join(&month).join(&day);
                        if !day_dir.exists() {
                            continue;
                        }
                        if let Ok(entries) = fs::read_dir(&day_dir) {
                            for entry in entries.flatten() {
                                if stopped() {
                                    break;
                                }
                                let path = entry.path();
                                if path.extension().is_some_and(|e| e == "jsonl") {
                                    day_cost += scan_codex_file_cost_for_range(&path, &range);
                                }
                            }
                        }
                    }
                }
                daily_costs.insert(date_str, day_cost);
            }
        }
        "claude" => {
            // Real per-day breakdown: walk the project logs once,
            // de-duplicating records across files.
            let projects_dir = scanner.get_claude_projects_dir();
            if projects_dir.exists() {
                let cutoff = Utc::now() - Duration::days(days as i64);
                let mut seen = HashSet::new();
                let mut handle_file = |path: &Path| {
                    for_each_claude_usage_record(path, &cutoff, &mut seen, cancel, |record| {
                        add_claude_record_to_daily_costs(&mut daily_costs, record);
                    });
                };
                scanner.walk_claude_files(&projects_dir, &cutoff, cancel, &mut handle_file);
            }
        }
        _ => {}
    }

    // Convert to sorted vector
    let mut result: Vec<(String, f64)> = daily_costs.into_iter().collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    (result, stopped())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_unknown_model_falls_back_to_sonnet() {
        // Unknown/retired Claude IDs fall back to Sonnet 4.6 base pricing
        // ($3/1M input, $15/1M output). 100k tokens stay under the 200k tier.
        let cost =
            ClaudePricing::cost_usd_with_cache_ttl("claude-3-5-sonnet", 100_000, 0, 0, 0, 100_000);
        // 100k * $3/M + 100k * $15/M = 0.30 + 1.50 = 1.80
        assert!((cost - 1.80).abs() < 0.001);
    }

    #[test]
    fn records_unknown_claude_model_while_using_fallback_cost() {
        let event: ClaudeEvent = serde_json::from_str(
            r#"{"type":"assistant","timestamp":"2026-01-15T10:00:00Z","requestId":"req_unknown","message":{"id":"msg_unknown","model":"claude-retired-unknown","usage":{"input_tokens":100000,"output_tokens":100000}}}"#,
        )
        .unwrap();
        let record = claude_usage_record_from_event(&event).expect("usage record");
        let mut summary = CostSummary::default();

        add_claude_record_to_summary(&mut summary, &record);

        assert!(summary.total_cost_usd > 0.0);
        assert!(summary.unknown_models.contains("claude-retired-unknown"));
    }

    #[test]
    fn test_claude_fable_5_pricing() {
        let cost = ClaudePricing::cost_usd_with_cache_ttl("claude-fable-5", 100, 10, 0, 20, 5);
        let expected = (100.0 / 1_000_000.0) * 10.00
            + (10.0 / 1_000_000.0) * 12.50
            + (20.0 / 1_000_000.0) * 1.00
            + (5.0 / 1_000_000.0) * 50.00;
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_claude_one_hour_cache_write_pricing() {
        let cost = ClaudePricing::cost_usd_with_cache_ttl("claude-fable-5", 100, 30, 20, 20, 5);
        let expected = (100.0 / 1_000_000.0) * 10.00
            + (10.0 / 1_000_000.0) * 12.50
            + (20.0 / 1_000_000.0) * 20.00
            + (20.0 / 1_000_000.0) * 1.00
            + (5.0 / 1_000_000.0) * 50.00;
        assert!((cost - expected).abs() < f64::EPSILON);
    }

    #[test]
    fn test_claude_sonnet_46_honors_200k_tier() {
        // Delegating to the canonical table means the scanner now honors the
        // 200k long-context tier: 200k @ $3/M + 40k @ $6/M = 0.60 + 0.24 = 0.84
        // (the scanner's old inline table applied a flat $3/M = 0.72).
        let cost = ClaudePricing::cost_usd_with_cache_ttl("claude-sonnet-4-6", 240_000, 0, 0, 0, 0);
        assert!((cost - 0.84).abs() < 0.001);
    }

    #[test]
    fn test_current_gen_opus_uses_5_25_pricing() {
        // Opus 4.5/4.6/4.7/4.8 bill at $5/1M input + $25/1M output = $30 total.
        // Delegation regression guard: opus-4-8 in particular must resolve
        // through the canonical table (it was missing there before this fix).
        for model in [
            "claude-opus-4-5",
            "claude-opus-4-6",
            "claude-opus-4-7",
            "claude-opus-4-8",
        ] {
            let cost = ClaudePricing::cost_usd_with_cache_ttl(model, 1_000_000, 0, 0, 0, 1_000_000);
            assert!(
                (cost - 30.00).abs() < 0.001,
                "{model} should bill $30 ($5 in + $25 out), got {cost}"
            );
        }
    }

    #[test]
    fn test_legacy_opus_keeps_legacy_pricing() {
        // Legacy Opus 4.0 / 4.1 remain at $15/1M input + $75/1M output = $90 in
        // the canonical table. (Retired IDs absent from the table — e.g. Opus 3
        // `claude-3-opus-...` — fall back to Sonnet instead; they are outside
        // any realistic 30-day scan window.)
        for model in ["claude-opus-4-20250514", "claude-opus-4-1"] {
            let cost = ClaudePricing::cost_usd_with_cache_ttl(model, 1_000_000, 0, 0, 0, 1_000_000);
            assert!(
                (cost - 90.00).abs() < 0.001,
                "{model} should bill $90 ($15 in + $75 out), got {cost}"
            );
        }
    }

    #[test]
    fn test_haiku_45_uses_current_pricing() {
        // Haiku 4.5 bills at $1/1M input + $5/1M output = $6 via the canonical
        // table (previously the scanner under-priced it at the Haiku 3 rate).
        let cost = ClaudePricing::cost_usd_with_cache_ttl(
            "claude-haiku-4-5",
            1_000_000,
            0,
            0,
            0,
            1_000_000,
        );
        assert!(
            (cost - 6.00).abs() < 0.001,
            "haiku-4-5 should bill $6 ($1 in + $5 out), got {cost}"
        );
    }

    #[test]
    fn parses_current_codex_payload_token_count_events() {
        let path = std::env::temp_dir().join(format!(
            "codexbar-current-codex-token-count-{}.jsonl",
            std::process::id()
        ));
        // Use a recent timestamp so the event stays inside the scanner's
        // 30-day window no matter when the test runs. A hardcoded date
        // silently ages out of the window and makes this test fail with 0
        // sessions once it is more than 30 days in the past.
        let recent = (Utc::now() - Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let mut file = File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"model":"gpt-5","total_token_usage":{{"input_tokens":125,"cached_input_tokens":30,"output_tokens":15}}}}}}}}"#,
            ts = recent
        )
        .unwrap();
        drop(file);

        let scanner = CostScanner::new(30);
        let mut summary = CostSummary::default();
        scanner.parse_codex_file(&path, &mut summary, None);

        assert_eq!(summary.sessions_count, 1);
        assert_eq!(summary.input_tokens, 125);
        assert_eq!(summary.cached_tokens, 30);
        assert_eq!(summary.output_tokens, 15);
        assert_eq!(
            summary
                .by_model_tokens
                .get("gpt-5")
                .map(ModelTokenCounts::total),
            Some(140)
        );
        assert!(scan_codex_file_cost(&path) > 0.0);
        let _ = std::fs::remove_file(&path);
    }

    /// A verbatim `turn_completed` line, trimmed only of a long tool payload.
    /// Shape and field names come from a real `~/.grok/sessions/**/updates.jsonl`.
    const GROK_TURN: &str = r#"{"timestamp":1785573295,"method":"_x.ai/session/update","params":{"sessionId":"019fbc54","update":{"sessionUpdate":"turn_completed","stop_reason":"end_turn","usage":{"inputTokens":2672891,"outputTokens":51097,"totalTokens":2723988,"cachedReadTokens":2561920,"modelCalls":35,"costUsdTicks":12971000000,"modelUsage":{"grok-4.5-build":{"inputTokens":2672891,"outputTokens":51097,"cachedReadTokens":2561920,"costUsdTicks":12971000000}}}},"_meta":{"eventId":"019fbc54-2176"}}}"#;

    fn grok_turn_usage(line: &str) -> GrokUsage {
        serde_json::from_str::<GrokUpdateLine>(line)
            .expect("parses")
            .params
            .and_then(|params| params.update)
            .and_then(|update| update.usage)
            .expect("carries usage")
    }

    #[test]
    fn reads_grok_turn_totals_and_per_model_breakdown() {
        let mut summary = CostSummary::default();
        add_grok_turn_to_summary(&mut summary, &grok_turn_usage(GROK_TURN));

        assert_eq!(summary.input_tokens, 2_672_891);
        assert_eq!(summary.output_tokens, 51_097);
        // A subset of the input count, not an addition to it: the log's own
        // `totalTokens` is exactly input + output.
        assert_eq!(summary.cached_tokens, 2_561_920);
        assert_eq!(
            summary.input_tokens + summary.output_tokens,
            2_723_988,
            "must match the totalTokens the log itself reports"
        );
        // 12_971_000_000 ticks at 1e9 ticks per USD.
        assert!((summary.total_cost_usd - 12.971).abs() < 1e-9);
        assert_eq!(
            summary.by_model_tokens.get("grok-4.5-build").map(ModelTokenCounts::total),
            Some(2_723_988)
        );
    }

    /// The turn totals are per-turn, not a running session total. Verified
    /// against real logs before the scanner was written: summing cumulative
    /// snapshots would have multiplied every reported figure.
    #[test]
    fn sums_grok_turns_rather_than_taking_the_last() {
        let mut summary = CostSummary::default();
        let usage = grok_turn_usage(GROK_TURN);
        add_grok_turn_to_summary(&mut summary, &usage);
        add_grok_turn_to_summary(&mut summary, &usage);
        assert_eq!(summary.output_tokens, 51_097 * 2);
    }

    #[test]
    fn counts_a_grok_turn_once_across_replayed_logs() {
        let dir = std::env::temp_dir().join(format!("grok-scan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("updates.jsonl");
        // The same event id twice, as a resumed session's log replays it.
        std::fs::write(&path, format!("{GROK_TURN}\n{GROK_TURN}\n")).expect("write");

        let cutoff = DateTime::from_timestamp(0, 0).expect("epoch");
        let mut seen = HashSet::new();
        let mut summary = CostSummary::default();
        let counted = for_each_grok_turn(&path, &cutoff, &mut seen, None, |turn| {
            add_grok_turn_to_summary(&mut summary, turn);
        });

        assert_eq!(counted, 1, "the replayed copy must not be counted again");
        assert_eq!(summary.output_tokens, 51_097);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_grok_turns_older_than_the_window() {
        let dir = std::env::temp_dir().join(format!("grok-window-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("updates.jsonl");
        std::fs::write(&path, format!("{GROK_TURN}\n")).expect("write");

        // The record's timestamp is fixed, so a cutoff after it must exclude it.
        let cutoff = DateTime::from_timestamp(1_785_573_296, 0).expect("valid");
        let mut seen = HashSet::new();
        let counted = for_each_grok_turn(&path, &cutoff, &mut seen, None, |_| {});

        assert_eq!(counted, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Non-usage lines share the file and must be skipped without being parsed
    /// as turns — `chat_history` payloads in particular can be megabytes.
    #[test]
    fn skips_grok_lines_that_are_not_completed_turns() {
        let dir = std::env::temp_dir().join(format!("grok-other-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("updates.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"timestamp":1785573109,"params":{"update":{"sessionUpdate":"tool_call_update","status":"completed"}}}"#,
                "\n",
                "not json at all\n",
            ),
        )
        .expect("write");

        let cutoff = DateTime::from_timestamp(0, 0).expect("epoch");
        let mut seen = HashSet::new();
        let counted = for_each_grok_turn(&path, &cutoff, &mut seen, None, |_| {});

        assert_eq!(counted, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn derives_claude_dedup_key_from_message_and_request_ids() {
        assert_eq!(
            claude_usage_dedup_key(Some("msg_1"), Some("req_1")).as_deref(),
            Some("msg_1:req_1")
        );
        assert_eq!(
            claude_usage_dedup_key(Some("msg_1"), None).as_deref(),
            Some("message:msg_1")
        );
        assert_eq!(
            claude_usage_dedup_key(None, Some("req_1")).as_deref(),
            Some("request:req_1")
        );
        assert_eq!(claude_usage_dedup_key(None, None), None);
    }

    #[test]
    fn counts_claude_usage_once_across_duplicate_records() {
        // The same API response can be replayed into several transcript files
        // (session resume, sidechains); it must only be counted once.
        let event: ClaudeEvent = serde_json::from_str(
            r#"{"type":"assistant","timestamp":"2026-01-15T10:00:00Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-sonnet-4-6","usage":{"input_tokens":100,"output_tokens":50,"cache_creation_input_tokens":10,"cache_read_input_tokens":20}}}"#,
        )
        .unwrap();

        let record = claude_usage_record_from_event(&event).expect("usage record");
        assert_eq!(record.model, "claude-sonnet-4-6");
        assert_eq!(record.input, 100);
        assert_eq!(record.output, 50);
        assert_eq!(record.cache_create, 10);
        assert_eq!(record.cache_read, 20);
        assert!(record.cost > 0.0);

        let cutoff = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut seen = HashSet::new();
        assert!(should_count_claude_record(&record, &cutoff, &mut seen));
        assert!(!should_count_claude_record(&record, &cutoff, &mut seen));
    }

    #[test]
    fn rejects_claude_records_before_cutoff() {
        let event: ClaudeEvent = serde_json::from_str(
            r#"{"type":"assistant","timestamp":"2025-12-01T10:00:00Z","requestId":"req_old","message":{"id":"msg_old","model":"claude-sonnet-4-6","usage":{"input_tokens":1,"output_tokens":1}}}"#,
        )
        .unwrap();
        let record = claude_usage_record_from_event(&event).expect("usage record");
        let cutoff = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut seen = HashSet::new();
        assert!(!should_count_claude_record(&record, &cutoff, &mut seen));
    }

    #[test]
    fn ignores_claude_events_without_countable_usage() {
        // Non-assistant events carry no billable usage.
        let event: ClaudeEvent =
            serde_json::from_str(r#"{"type":"user","message":{"usage":{"input_tokens":5}}}"#)
                .unwrap();
        assert!(claude_usage_record_from_event(&event).is_none());

        // Zero-token usage blocks (e.g. synthetic messages) are not sessions.
        let event: ClaudeEvent = serde_json::from_str(
            r#"{"type":"assistant","message":{"id":"msg_zero","model":"claude-sonnet-4-6","usage":{"input_tokens":0,"output_tokens":0}}}"#,
        )
        .unwrap();
        assert!(claude_usage_record_from_event(&event).is_none());
    }

    fn claude_transcript_line(
        timestamp: &str,
        request_key: &str,
        request_id: &str,
        message_id: &str,
    ) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{timestamp}","{request_key}":"{request_id}","message":{{"id":"{message_id}","model":"claude-sonnet-4-6","usage":{{"input_tokens":1000,"output_tokens":500}}}}}}"#
        )
    }

    #[test]
    fn daily_history_dedups_across_files_and_buckets_by_local_day() {
        // End-to-end regression for the daily buckets: two transcript files,
        // two different days, plus a replay of the day-one record in the
        // second file (snake_case request_id, as another writer would emit).
        let dir = std::env::temp_dir();
        let file_a = dir.join(format!(
            "codexbar-claude-daily-a-{}.jsonl",
            std::process::id()
        ));
        let file_b = dir.join(format!(
            "codexbar-claude-daily-b-{}.jsonl",
            std::process::id()
        ));

        // >24h apart guarantees two distinct local calendar days.
        let day_one = Utc::now() - Duration::hours(30);
        let day_two = Utc::now() - Duration::hours(2);
        let ts_one = day_one.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let ts_two = day_two.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

        std::fs::write(
            &file_a,
            format!(
                "{}\n{}\n",
                claude_transcript_line(&ts_one, "requestId", "req_1", "msg_1"),
                claude_transcript_line(&ts_two, "requestId", "req_2", "msg_2"),
            ),
        )
        .unwrap();
        std::fs::write(
            &file_b,
            format!(
                "{}\n",
                claude_transcript_line(&ts_one, "request_id", "req_1", "msg_1"),
            ),
        )
        .unwrap();

        let day_key = |ts: &DateTime<Utc>| {
            ts.with_timezone(&Local)
                .date_naive()
                .format("%Y-%m-%d")
                .to_string()
        };
        let mut daily_costs = HashMap::new();
        daily_costs.insert(day_key(&day_one), 0.0);
        daily_costs.insert(day_key(&day_two), 0.0);

        let cutoff = Utc::now() - Duration::days(30);
        let mut seen = HashSet::new();
        for path in [&file_a, &file_b] {
            for_each_claude_usage_record(path, &cutoff, &mut seen, None, |record| {
                add_claude_record_to_daily_costs(&mut daily_costs, record);
            });
        }

        let day_one_cost = daily_costs[&day_key(&day_one)];
        let day_two_cost = daily_costs[&day_key(&day_two)];
        assert!(day_one_cost > 0.0, "day one should carry real cost");
        // Identical usage on both days: equal buckets proves the file-b
        // replay was de-duplicated (a leak would double day one).
        assert!(
            (day_one_cost - day_two_cost).abs() < f64::EPSILON,
            "each day should hold exactly one record's cost, got {day_one_cost} vs {day_two_cost}"
        );

        let _ = std::fs::remove_file(&file_a);
        let _ = std::fs::remove_file(&file_b);
    }
}
