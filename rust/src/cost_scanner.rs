//! Local cost-usage scanner for Codex, Claude, Grok, Gemini CLI, Copilot CLI,
//! Kimi, Amp and OpenCode-family local databases.
//!
//! Scans local log files to aggregate token usage and calculate costs. Each
//! source only contributes what its logs actually record: Grok and Amp carry
//! provider-side cost figures, OpenCode-family rows carry OpenCode's own
//! computed cost, and the Gemini/Copilot/Kimi logs are token-only: those are
//! priced from canonical model rates when the model is known and a rate
//! resolves, and stay explicitly unpriced (marked partial) otherwise rather
//! than inventing dollar values.
//!
//! Codex file parsing goes through the incremental per-file cache
//! ([`crate::scan_cache`]): unchanged session files are never re-read.

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
    apply_codex_turns_to_summary, codex_period_start, codex_turn_cost_at_date,
};
use crate::codex_sessions::{codex_sessions_dir_candidates, default_wsl_roots};
use crate::core::{CostUsageDayRange, CostUsagePricing, JsonlScanner};
use crate::scan_cache::{self, CachedTurn, CodexFileCache};
use crate::settings::Settings;

/// Per-file turn stream (thread key + creation + ordered turns), served from
/// the incremental cache when the file is unchanged. Aggregated buckets cannot
/// be used here: fork-lineage deduplication aligns per-turn sequences.
pub(crate) fn codex_file_turns(path: &Path) -> Option<CodexFileCache> {
    let (mtime, len) = scan_cache::stamp(path)?;
    if let Some(cached) = scan_cache::lookup(path, mtime, len) {
        return Some(cached);
    }

    // Whole-file range: the baseline for cumulative-total deltas must reach
    // back to the first line, or resumed-session attribution drifts.
    let wide = CostUsageDayRange::new(
        NaiveDate::from_ymd_opt(2000, 1, 1)?,
        NaiveDate::from_ymd_opt(2100, 1, 1)?,
    );
    let parse_result = JsonlScanner::parse_codex_file(path, &wide, 0, None, None).ok()?;

    let turns: Vec<CachedTurn> = parse_result
        .records
        .into_iter()
        .map(|record| {
            let input = record.input;
            let cached = record.cached.min(input);
            CachedTurn {
                day: record.day_key,
                model: record.model,
                input,
                cached,
                output: record.output,
            }
        })
        .collect();
    let entry = CodexFileCache {
        mtime,
        len,
        thread: parse_result.thread,
        created: parse_result.created,
        turns,
    };
    scan_cache::store(entry.clone(), path);
    Some(entry)
}

/// Greedy first-appearance merge of one thread's fork views, in creation
/// order. Every fork file re-emits the thread's whole ancestor history
/// (re-stamped at the fork instant) before its own turns, so aligning each
/// file's sequence against the already-accounted reference — and counting
/// only the turns past the aligned prefix — bills every real turn exactly
/// once. Model identity is part of the match; equal token triples from a
/// different model are not silently treated as replay.
pub(crate) fn dedup_thread_turns(mut files: Vec<CodexFileCache>) -> Vec<CachedTurn> {
    files.sort_by(|a, b| a.created.cmp(&b.created));
    let mut reference: Vec<CachedTurn> = Vec::new();
    let mut unique: Vec<CachedTurn> = Vec::new();
    for file in files {
        let mut aligned = 0usize;
        for turn in &file.turns {
            let seen = aligned < reference.len()
                && reference[aligned].model == turn.model
                && reference[aligned].input == turn.input
                && reference[aligned].cached == turn.cached
                && reference[aligned].output == turn.output;
            if seen {
                aligned += 1;
                continue;
            }
            unique.push(turn.clone());
            reference.push(turn.clone());
        }
    }
    unique
}

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
    /// Whether every model in the summary resolved through a canonical rate.
    /// A partial result remains useful as a breakdown, but its dollar total is
    /// explicitly an estimate rather than a complete billing reconstruction.
    pub model_pricing_completeness: ModelPricingCompleteness,
    /// Whether the requested local history window was fully inspected.
    pub history_coverage_established: bool,
    /// A completed scan with no contributing rows is a known zero. A cancelled
    /// or deadline-truncated scan must never manufacture zero.
    pub known_zero: bool,
    /// Period start date
    pub period_start: Option<NaiveDate>,
    /// Period end date
    pub period_end: Option<NaiveDate>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ModelPricingCompleteness {
    #[default]
    Complete,
    Partial {
        unpriced_models: Vec<String>,
    },
}

impl ModelPricingCompleteness {
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }

    pub fn unpriced_models(&self) -> &[String] {
        match self {
            Self::Complete => &[],
            Self::Partial { unpriced_models } => unpriced_models,
        }
    }

    pub(crate) fn mark_partial(&mut self, model: &str) {
        match self {
            Self::Complete => {
                *self = Self::Partial {
                    unpriced_models: vec![model.to_string()],
                };
            }
            Self::Partial { unpriced_models } => {
                if !unpriced_models.iter().any(|item| item == model) {
                    unpriced_models.push(model.to_string());
                }
            }
        }
    }
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

fn codex_path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase()
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
    /// Explicit models.dev cache root used by tests so pricing assertions do
    /// not depend on the machine's downloaded catalog. Production scans leave
    /// this `None` and read the real cache.
    pricing_cache_root: Option<PathBuf>,
}

impl CostScanner {
    /// Create a new scanner for the last N days
    pub fn new(days: u32) -> Self {
        Self {
            days,
            pricing_cache_root: None,
        }
    }

    #[cfg(test)]
    fn with_pricing_cache_root(mut self, root: PathBuf) -> Self {
        self.pricing_cache_root = Some(root);
        self
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

        let (turns, coverage_established) =
            self.collect_unique_codex_turns_with_status(cancel, None);
        let (total_cost, turn_count) = apply_codex_turns_to_summary(&mut summary, &turns, &range);
        summary.total_cost_usd += total_cost;
        // One session == one deduplicated turn of real consumption.
        summary.sessions_count = turn_count;
        summary.history_coverage_established = coverage_established;
        summary.known_zero = coverage_established && turn_count == 0;

        // Turns collected during this walk are flushed with the cache file so
        // the next scan (any caller) re-reads only files changed since.
        scan_cache::persist();
        summary
    }

    /// Collect the thread-deduplicated Codex turn stream. See
    /// [`dedup_thread_turns`] for the lineage-deduplication semantics.
    pub(crate) fn collect_unique_codex_turns(
        &self,
        cancel: Option<&AtomicBool>,
        deadline: Option<Instant>,
    ) -> Vec<CachedTurn> {
        self.collect_unique_codex_turns_with_status(cancel, deadline)
            .0
    }

    pub(crate) fn collect_unique_codex_turns_with_status(
        &self,
        cancel: Option<&AtomicBool>,
        deadline: Option<Instant>,
    ) -> (Vec<CachedTurn>, bool) {
        let stopped =
            || is_cancelled(cancel) || deadline.is_some_and(|limit| Instant::now() >= limit);
        let mut candidates: Vec<(PathBuf, bool)> = Vec::new();
        let mut source_available = false;
        let today = Local::now().date_naive();
        let mtime_cutoff = codex_period_start(today, self.days) - Duration::days(1);
        for sessions_dir in self.get_codex_sessions_dirs() {
            if stopped() {
                break;
            }
            if !sessions_dir.exists() {
                continue;
            }
            source_available = true;
            self.for_each_codex_file(
                &sessions_dir,
                mtime_cutoff,
                cancel,
                deadline,
                &mut |path, recent| candidates.push((path.to_path_buf(), recent)),
            );
        }

        // A recent fork may replay an ancestor whose file mtime is old. Read
        // only the metadata prefix for recent files first, then include old
        // files sharing one of those lineage keys. This preserves the fast
        // path for unrelated history without allowing the mtime filter to
        // defeat replay deduplication.
        let mut recent_threads = HashSet::new();
        let mut identities: HashMap<String, Option<String>> = HashMap::new();
        for (path, recent) in &candidates {
            if !*recent {
                continue;
            }
            let thread = JsonlScanner::read_codex_session_identity(path)
                .ok()
                .and_then(|(thread, _)| thread)
                .filter(|thread| !thread.trim().is_empty());
            if let Some(thread) = &thread {
                recent_threads.insert(thread.clone());
            }
            identities.insert(codex_path_key(path), thread);
        }

        let mut selected_paths = Vec::new();
        let mut selected_keys = HashSet::new();
        for (path, recent) in candidates {
            if stopped() {
                break;
            }
            let key = codex_path_key(&path);
            let thread = identities.get(&key).cloned().flatten().or_else(|| {
                JsonlScanner::read_codex_session_identity(&path)
                    .ok()
                    .and_then(|(thread, _)| thread)
                    .filter(|thread| !thread.trim().is_empty())
            });
            let is_lineage_ancestor = thread
                .as_ref()
                .is_some_and(|thread| recent_threads.contains(thread));
            if (recent || is_lineage_ancestor) && selected_keys.insert(key) {
                selected_paths.push(path);
            }
        }

        let mut files: Vec<CodexFileCache> = Vec::new();
        for path in selected_paths {
            if stopped() {
                break;
            }
            if let Some(mut entry) = codex_file_turns(&path) {
                // A missing/legacy identity must never place unrelated files
                // in one shared empty dedup group. The path is a stable local
                // fallback key for this scan and its persisted cache entry.
                if entry
                    .thread
                    .as_deref()
                    .is_none_or(|thread| thread.trim().is_empty())
                {
                    entry.thread = Some(format!("file:{key}", key = codex_path_key(&path)));
                }
                files.push(entry);
            }
        }

        let mut per_thread: std::collections::HashMap<String, Vec<CodexFileCache>> =
            std::collections::HashMap::new();
        for file in files {
            let key = file
                .thread
                .clone()
                .unwrap_or_else(|| "file:unknown".to_string());
            per_thread.entry(key).or_default().push(file);
        }
        let mut unique: Vec<CachedTurn> = Vec::new();
        for (_, thread_files) in &mut per_thread {
            unique.extend(dedup_thread_turns(std::mem::take(thread_files)));
        }
        // An empty result is only a confirmed zero when at least one source
        // root was present and the walk reached its end. Missing roots are a
        // configuration/installation state, not evidence of no usage.
        (unique, source_available && !stopped())
    }

    /// Walk one sessions root and report every jsonl file with whether its
    /// modification date is recent enough for the requested window. The
    /// caller uses the cheap metadata pass to discover old fork ancestors.
    fn for_each_codex_file(
        &self,
        sessions_dir: &Path,
        mtime_cutoff: NaiveDate,
        cancel: Option<&AtomicBool>,
        deadline: Option<Instant>,
        on_file: &mut dyn FnMut(&Path, bool),
    ) {
        let Ok(entries) = fs::read_dir(sessions_dir) else {
            return;
        };
        for entry in entries.flatten() {
            if is_cancelled(cancel) || deadline.is_some_and(|limit| Instant::now() >= limit) {
                break;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                self.for_each_codex_file(&path, mtime_cutoff, cancel, deadline, on_file);
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
                on_file(&path, recent);
            }
        }
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

        summary.history_coverage_established = !is_cancelled(cancel);
        summary.known_zero = summary.history_coverage_established && summary.sessions_count == 0;
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
            let counted =
                for_each_grok_turn(path, &cutoff, &mut seen, cancel, |_timestamp, turn| {
                    add_grok_turn_to_summary(&mut summary, turn);
                });
            if counted > 0 {
                summary.sessions_count += 1;
            }
        };
        self.walk_grok_files(&sessions_dir, &cutoff, cancel, &mut handle_file);

        summary.history_coverage_established = !is_cancelled(cancel);
        summary.known_zero = summary.history_coverage_established && summary.sessions_count == 0;
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

    /// Recursively visit files whose mtime is inside the window and let each
    /// provider's closure apply its own name/extension filter. Mirrors the
    /// grok/claude walkers but format-agnostic: the JSON-doc sources (Gemini
    /// chats, Amp threads), the Copilot session-state tree and the Kimi wire
    /// logs all share this traversal. A file rewritten in the window carries
    /// every message it contains, so the mtime bound never drops in-window
    /// rows — the per-record timestamp filters below do the precise cut.
    fn walk_source_files<F>(
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
                self.walk_source_files(&path, cutoff, cancel, on_file);
            } else if let Ok(metadata) = fs::metadata(&path)
                && let Ok(modified) = metadata.modified()
            {
                let modified_dt: DateTime<Utc> = modified.into();
                if modified_dt >= *cutoff {
                    on_file(&path);
                }
            }
        }
    }

    fn finish_summary(
        mut summary: CostSummary,
        sessions: u32,
        cancel: Option<&AtomicBool>,
    ) -> CostSummary {
        summary.sessions_count = sessions;
        summary.history_coverage_established = !is_cancelled(cancel);
        summary.known_zero = summary.history_coverage_established && sessions == 0;
        summary
    }

    fn cutoff_for(&self) -> DateTime<Utc> {
        Utc::now() - Duration::days(self.days as i64)
    }

    fn period_bounds(&self, summary: &mut CostSummary) {
        let today = Utc::now().date_naive();
        summary.period_start = Some(today - Duration::days(self.days as i64));
        summary.period_end = Some(today);
    }

    /// Root of the Gemini CLI's per-project chat documents.
    ///
    /// `GEMINI_CLI_HOME` is honoured the same way the CLI itself relocates its
    /// data, so a relocated install stays readable. `~/.gemini/tmp` holds one
    /// JSON chat document (plus legacy `session-*.json` files) per project.
    fn get_gemini_tmp_dir(&self) -> PathBuf {
        if let Ok(home) = std::env::var("GEMINI_CLI_HOME") {
            let trimmed = home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("tmp");
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".gemini")
            .join("tmp")
    }

    /// Root of the Copilot CLI's per-session event logs.
    ///
    /// `COPILOT_HOME` relocates the CLI's data root. Only
    /// `session-state/*/events.jsonl` is read here: it is always written and
    /// carries cumulative per-model usage snapshots at session shutdown.
    fn get_copilot_session_state_dir(&self) -> PathBuf {
        if let Ok(home) = std::env::var("COPILOT_HOME") {
            let trimmed = home.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed).join("session-state");
            }
        }
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".copilot")
            .join("session-state")
    }

    /// Roots of the Kimi CLI and Kimi Code wire logs.
    ///
    /// Both dialects live in `wire.jsonl` files under `sessions/`:
    /// `~/.kimi/sessions/...` (Kimi CLI) and `~/.kimi-code/sessions/...`
    /// (Kimi Code, whose `KIMI_CODE_HOME` override is honoured verbatim).
    /// `KIMI_HOME` mirrors that override for the CLI root so relocated installs
    /// and tests stay readable without touching the real home directory.
    fn get_kimi_session_roots(&self) -> Vec<PathBuf> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let kimi_home = std::env::var("KIMI_HOME")
            .ok()
            .map(|value| PathBuf::from(value.trim().to_string()))
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| home.join(".kimi"));
        let kimi_code_home = std::env::var("KIMI_CODE_HOME")
            .ok()
            .map(|value| PathBuf::from(value.trim().to_string()))
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| home.join(".kimi-code"));
        vec![kimi_home.join("sessions"), kimi_code_home.join("sessions")]
    }

    /// Root of Amp's local thread ledgers (`amp/threads/T-*.json`).
    ///
    /// Amp follows the XDG data convention, which on Windows resolves to
    /// `%USERPROFILE%\.local\share`; `XDG_DATA_HOME` overrides it.
    fn get_amp_threads_dir(&self) -> PathBuf {
        let base = std::env::var("XDG_DATA_HOME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".local").join("share")));
        base.unwrap_or_else(|| PathBuf::from("."))
            .join("amp")
            .join("threads")
    }

    /// Scan Gemini CLI chat documents (`~/.gemini/tmp/**`).
    ///
    /// Chat documents carry one token object per message with the input count
    /// **inclusive** of cached reads (Gemini API convention), so the cached
    /// portion is subtracted before summing — the same correction tokscale
    /// applies. The logs carry no cost figures, so the summary stays
    /// explicitly unpriced.
    pub fn scan_gemini_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        let root = self.get_gemini_tmp_dir();
        let mut summary = CostSummary::default();
        if !root.exists() {
            return summary;
        }
        self.period_bounds(&mut summary);
        let cutoff = self.cutoff_for();

        let mut seen = HashSet::new();
        let mut sessions = 0u32;
        let mut handle_file = |path: &Path| {
            let counted = for_each_gemini_entry(path, &cutoff, &mut seen, cancel, |entry| {
                add_gemini_entry_to_summary(
                    &mut summary,
                    entry,
                    self.pricing_cache_root.as_deref(),
                );
            });
            if counted > 0 {
                sessions += 1;
            }
        };
        self.walk_source_files(&root, &cutoff, cancel, &mut handle_file);
        Self::finish_summary(summary, sessions, cancel)
    }

    pub fn scan_gemini(&self) -> CostSummary {
        self.scan_gemini_with_cancel(None)
    }

    /// Scan Antigravity's local conversation databases (offline history).
    ///
    /// Port of upstream CodexBar's Antigravity local reader: every conversation
    /// SQLite database stores one protobuf generation record per completed
    /// model call with token counts and the model id, and generation dates are
    /// recovered from the `steps` table. Antigravity records no per-response
    /// duration, so this source contributes usage and cost only.
    pub fn scan_antigravity_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        use crate::providers::antigravity::local_history::{ScanCoverage, scan};

        let mut summary = CostSummary::default();
        self.period_bounds(&mut summary);
        let scan = scan(cancel);
        if scan.coverage == ScanCoverage::Unavailable {
            // No local Antigravity install — not a known zero.
            return summary;
        }
        let cutoff_ms = self.cutoff_for().timestamp_millis();
        let mut sessions = 0u32;
        for event in &scan.events {
            if event.timestamp_ms < cutoff_ms {
                continue;
            }
            add_antigravity_event_to_summary(
                &mut summary,
                event,
                self.pricing_cache_root.as_deref(),
            );
            sessions = sessions.saturating_add(1);
        }
        summary.sessions_count = sessions;
        summary.history_coverage_established =
            scan.coverage == ScanCoverage::Complete && !is_cancelled(cancel);
        summary.known_zero = summary.history_coverage_established && sessions == 0;
        summary
    }

    pub fn scan_antigravity(&self) -> CostSummary {
        self.scan_antigravity_with_cancel(None)
    }

    /// Scan Copilot CLI session-state logs (`~/.copilot/session-state/**`).
    ///
    /// Each `events.jsonl` ends with `session.shutdown` snapshots whose
    /// `modelMetrics` are **cumulative** per session and model, so consecutive
    /// snapshots are differenced inside one file — the same handling tokscale
    /// and ccusage apply. First snapshot contributes its full value. Token
    /// logs carry no cost figures, so the summary stays explicitly unpriced.
    pub fn scan_copilot_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        let root = self.get_copilot_session_state_dir();
        let mut summary = CostSummary::default();
        if !root.exists() {
            return summary;
        }
        self.period_bounds(&mut summary);
        let cutoff = self.cutoff_for();

        let mut sessions = 0u32;
        let mut handle_file = |path: &Path| {
            let is_events_log = path.file_name().is_some_and(|name| name == "events.jsonl");
            if !is_events_log {
                return;
            }
            let counted = for_each_copilot_shutdown(path, &cutoff, cancel, |deltas, _timestamp| {
                add_copilot_deltas_to_summary(
                    &mut summary,
                    &deltas,
                    self.pricing_cache_root.as_deref(),
                );
            });
            if counted > 0 {
                sessions += 1;
            }
        };
        self.walk_source_files(&root, &cutoff, cancel, &mut handle_file);
        Self::finish_summary(summary, sessions, cancel)
    }

    pub fn scan_copilot(&self) -> CostSummary {
        self.scan_copilot_with_cancel(None)
    }

    /// Scan Kimi CLI and Kimi Code wire logs (`wire.jsonl`).
    ///
    /// The two dialects differ: the CLI emits per-message `StatusUpdate`
    /// rows whose `token_usage` is a running total (later rows replace
    /// earlier ones per `message_id`), while Kimi Code emits turn-scoped
    /// `usage.record` rows that are per-turn deltas. The logs carry no cost
    /// figures, so the summary stays explicitly unpriced.
    pub fn scan_kimi_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        let mut summary = CostSummary::default();
        self.period_bounds(&mut summary);
        let cutoff = self.cutoff_for();

        let mut sessions = 0u32;
        for root in self.get_kimi_session_roots() {
            if !root.exists() {
                continue;
            }
            let mut handle_file = |path: &Path| {
                let is_wire_log = path.file_name().is_some_and(|name| name == "wire.jsonl");
                if !is_wire_log {
                    return;
                }
                let counted = for_each_kimi_entry(path, &cutoff, cancel, |entry| {
                    add_kimi_entry_to_summary(
                        &mut summary,
                        entry,
                        self.pricing_cache_root.as_deref(),
                    );
                });
                if counted > 0 {
                    sessions += 1;
                }
            };
            self.walk_source_files(&root, &cutoff, cancel, &mut handle_file);
        }
        Self::finish_summary(summary, sessions, cancel)
    }

    pub fn scan_kimi(&self) -> CostSummary {
        self.scan_kimi_with_cancel(None)
    }

    /// Scan Amp thread ledgers (`~/.local/share/amp/threads/T-*.json`).
    ///
    /// The usage ledger is Amp's own billing record: each event carries token
    /// counts plus a `credits` figure in Amp's USD credit unit, which is
    /// treated as provider-reported cost. Newer Amp builds moved usage
    /// accounting server-side and may not write these files anymore — an
    /// absent directory simply yields an empty scan, never a fabricated zero.
    pub fn scan_amp_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        let root = self.get_amp_threads_dir();
        let mut summary = CostSummary::default();
        if !root.exists() {
            return summary;
        }
        self.period_bounds(&mut summary);
        let cutoff = self.cutoff_for();

        let mut sessions = 0u32;
        let mut handle_file = |path: &Path| {
            let is_thread = path.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.starts_with("T-") && name.ends_with(".json")
            });
            if !is_thread {
                return;
            }
            let counted = for_each_amp_event(path, &cutoff, cancel, |event| {
                add_amp_event_to_summary(&mut summary, event);
            });
            if counted > 0 {
                sessions += 1;
            }
        };
        self.walk_source_files(&root, &cutoff, cancel, &mut handle_file);
        Self::finish_summary(summary, sessions, cancel)
    }

    pub fn scan_amp(&self) -> CostSummary {
        self.scan_amp_with_cancel(None)
    }

    /// OpenCode-family local database scan for one Zen subscription.
    ///
    /// `opencode-go` reconstructs the Go plan windows; the plain `opencode`
    /// Zen provider reads the same database filtered to its own rows. Costs
    /// are the values OpenCode itself computed and stored (`$.cost`), never
    /// derived here.
    pub fn scan_opencodego_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        self.scan_opencode_local(cancel, "opencode-go")
    }

    pub fn scan_opencodego(&self) -> CostSummary {
        self.scan_opencodego_with_cancel(None)
    }

    pub fn scan_opencode_with_cancel(&self, cancel: Option<&AtomicBool>) -> CostSummary {
        self.scan_opencode_local(cancel, "opencode")
    }

    pub fn scan_opencode(&self) -> CostSummary {
        self.scan_opencode_with_cancel(None)
    }

    fn scan_opencode_local(
        &self,
        cancel: Option<&AtomicBool>,
        provider_filter: &str,
    ) -> CostSummary {
        // The database read is one bounded SQL query with no streaming cancel
        // point; a cancellation is checked before the read starts.
        if is_cancelled(cancel) {
            return CostSummary::default();
        }
        let now = Utc::now();
        let mut summary = CostSummary::default();
        self.period_bounds(&mut summary);

        let Some(scan) = crate::providers::opencodego::local::model_cost_summary_scan_for(
            provider_filter,
            now,
            self.days,
        ) else {
            // No local install detected — not a known zero, so the chart layer
            // keeps the recent-usage slot absent instead of showing 0.
            return summary;
        };

        summary.total_cost_usd = scan.total_cost_usd;
        summary.by_model = scan.by_model;
        summary.input_tokens = scan.tokens.input;
        summary.output_tokens = scan.tokens.output;
        summary.cached_tokens = scan.tokens.cached;
        for (model, tokens) in scan.by_model_tokens {
            summary.by_model_tokens.insert(
                model,
                ModelTokenCounts {
                    input_tokens: tokens.input,
                    output_tokens: tokens.output,
                    cached_tokens: tokens.cached,
                },
            );
        }
        // Upstream parity: one session-like unit per provider invocation.
        summary.sessions_count = scan.request_count;
        summary.history_coverage_established = true;
        summary.known_zero = scan.request_count == 0 && scan.total_cost_usd == 0.0;
        summary
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
    F: FnMut(Option<i64>, &GrokUsage),
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
        let Some(params) = parsed.params else {
            continue;
        };
        let Some(update) = params.update else {
            continue;
        };
        if update.session_update.as_deref() != Some("turn_completed") {
            continue;
        }
        let Some(usage) = update.usage else { continue };

        let timestamp = parsed.timestamp.or_else(|| {
            fs::metadata(path)
                .ok()
                .and_then(|m| m.modified().ok())
                .map(|t| {
                    let dt: DateTime<Utc> = t.into();
                    dt.timestamp()
                })
        });

        if let Some(ts) = timestamp
            && let Some(recorded) = if ts > 100_000_000_000 {
                DateTime::from_timestamp_millis(ts)
            } else {
                DateTime::from_timestamp(ts, 0)
            }
            && recorded < *cutoff
        {
            continue;
        }

        if let Some(event_id) = params.meta.and_then(|meta| meta.event_id)
            && !seen.insert(event_id)
        {
            continue;
        }

        on_turn(timestamp, &usage);
        counted += 1;
    }

    counted
}

fn add_grok_turn_to_daily_costs(
    daily_costs: &mut HashMap<String, f64>,
    timestamp: Option<i64>,
    turn: &GrokUsage,
) {
    let Some(ts) = timestamp else {
        return;
    };
    let Some(recorded) = (if ts > 100_000_000_000 {
        DateTime::from_timestamp_millis(ts)
    } else {
        DateTime::from_timestamp(ts, 0)
    }) else {
        return;
    };
    let date_str = recorded
        .with_timezone(&Local)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    if let Some(cost) = daily_costs.get_mut(&date_str) {
        *cost += turn.cost_usd();
    }
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

/// Mark a model as unpriced in a token-only summary.
///
/// Gemini/Copilot/Kimi logs carry no cost figures of their own; when no
/// canonical rate (embedded table or models.dev catalog) resolves for the
/// model, it lands in `model_pricing_completeness` as partial instead of
/// contributing a made-up dollar value.
fn mark_model_unpriced(summary: &mut CostSummary, model: &str) {
    summary.model_pricing_completeness.mark_partial(model);
}

/// Provider candidates tried against the models.dev catalog for each
/// token-only source. Canonical embedded tables always win in
/// `generic_model_cost_usd_with_root`; these only cover models those tables
/// do not know (Gemini direct models, Kimi/Moonshot models).
const GEMINI_PRICING_PROVIDERS: &[&str] = &["google", "gemini"];
const COPILOT_PRICING_PROVIDERS: &[&str] = &["github-copilot", "copilot"];
const KIMI_PRICING_PROVIDERS: &[&str] = &["moonshotai", "moonshot"];

/// Placeholder used when tokens exist but the log never named a model, so the
/// summary can still report that its pricing is incomplete.
const UNKNOWN_MODEL_LABEL: &str = "unknown";

fn price_tokens_with_candidates(
    provider_candidates: &[&str],
    model: &str,
    input: u64,
    cached: u64,
    output: u64,
    pricing_cache_root: Option<&Path>,
) -> Option<f64> {
    CostUsagePricing::generic_model_cost_usd_with_root(
        provider_candidates,
        model,
        input,
        cached,
        output,
        pricing_cache_root,
    )
}

/// Antigravity routes Gemini-family turns through Google's catalog; the shared
/// generic pricing also resolves the Claude/GPT families its logs record.
const ANTIGRAVITY_PRICING_PROVIDERS: &[&str] = &["google", "gemini"];

/// Routing variants Antigravity records that bill at the base model's public
/// price (mirrors upstream `AntigravityLocalReader.routingVariantSuffixes`).
const ANTIGRAVITY_ROUTING_SUFFIXES: [&str; 3] = ["-tiered", "-low", "-thinking"];

/// Antigravity records Gemini 3.1 Pro under product aliases and effort tiers
/// while the catalog lists only `gemini-3.1-pro-preview`; the safety-routed
/// 3.7 Flash id maps to ordinary 3.7 Flash (upstream alias table).
fn antigravity_pricing_alias(model: &str) -> Option<&'static str> {
    match model.to_ascii_lowercase().as_str() {
        "gemini-pro-default"
        | "gemini-pro-agent"
        | "gemini-3.1-pro"
        | "gemini-3.1-pro-high"
        | "gemini-3.1-pro-low" => Some("gemini-3.1-pro-preview"),
        "gemini-3.7-flash-safety-le" => Some("gemini-3.7-flash"),
        _ => None,
    }
}

/// Price one Antigravity generation. The exact recorded model id wins so an
/// explicitly catalogued variant keeps its own price; otherwise a known
/// routing-variant suffix or product alias falls back to the base model.
fn antigravity_cost_usd(
    model: &str,
    input: u64,
    cached: u64,
    output: u64,
    pricing_cache_root: Option<&Path>,
) -> Option<f64> {
    let price = |candidate: &str| {
        price_tokens_with_candidates(
            ANTIGRAVITY_PRICING_PROVIDERS,
            candidate,
            input,
            cached,
            output,
            pricing_cache_root,
        )
    };
    if let Some(alias) = antigravity_pricing_alias(model)
        && let Some(cost) = price(alias)
    {
        return Some(cost);
    }
    if let Some(cost) = price(model) {
        return Some(cost);
    }
    let lower = model.to_ascii_lowercase();
    for suffix in ANTIGRAVITY_ROUTING_SUFFIXES {
        if let Some(stripped) = lower.strip_suffix(suffix) {
            let base = &model[..stripped.len()];
            let candidate = antigravity_pricing_alias(base).unwrap_or(base);
            if let Some(cost) = price(candidate) {
                return Some(cost);
            }
        }
    }
    None
}

/// Accept RFC3339 strings, epoch seconds, or epoch milliseconds — the local
/// formats touched here use all three shapes across tools.
fn parse_flexible_timestamp(value: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    match value? {
        serde_json::Value::Number(number) => {
            let as_f64 = number.as_f64()?;
            let secs = if as_f64 > 100_000_000_000.0 {
                as_f64 / 1000.0
            } else {
                as_f64
            };
            DateTime::<Utc>::from_timestamp(secs as i64, 0)
        }
        serde_json::Value::String(text) => {
            let trimmed = text.trim();
            if let Ok(dt) = DateTime::parse_from_rfc3339(trimmed) {
                return Some(dt.with_timezone(&Utc));
            }
            let as_f64 = trimmed.parse::<f64>().ok()?;
            let secs = if as_f64 > 100_000_000_000.0 {
                as_f64 / 1000.0
            } else {
                as_f64
            };
            DateTime::<Utc>::from_timestamp(secs as i64, 0)
        }
        _ => None,
    }
}

fn file_mtime(path: &Path) -> Option<DateTime<Utc>> {
    fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok())
        .map(|modified| modified.into())
}

// ── Gemini CLI chat documents ───────────────────────────────────────────────

/// Token counts of one Gemini chat message. Every numeric shape the CLI has
/// shipped is accepted through aliases; `input`/`promptTokenCount` is
/// cache-inclusive and the caller nets the cached portion out.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeminiTokenCounts {
    #[serde(
        alias = "input",
        alias = "prompt",
        alias = "input_tokens",
        alias = "prompt_tokens",
        alias = "promptTokenCount"
    )]
    input: Option<u64>,
    #[serde(
        alias = "output",
        alias = "candidates",
        alias = "output_tokens",
        alias = "completion_tokens",
        alias = "candidatesTokenCount"
    )]
    output: Option<u64>,
    #[serde(
        alias = "cached",
        alias = "cached_tokens",
        alias = "cachedContentTokenCount"
    )]
    cached: Option<u64>,
    #[serde(
        alias = "thoughts",
        alias = "reasoning",
        alias = "thoughts_tokens",
        alias = "thoughtsTokenCount"
    )]
    thoughts: Option<u64>,
}

impl GeminiTokenCounts {
    fn is_empty(&self) -> bool {
        self.input.is_none()
            && self.output.is_none()
            && self.cached.is_none()
            && self.thoughts.is_none()
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiChatMessage {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    timestamp: Option<serde_json::Value>,
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    tokens: Option<GeminiTokenCounts>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiChatDoc {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    messages: Vec<GeminiChatMessage>,
}

/// One headless (`gemini -p`) JSONL run: `init` names the model, `result`
/// carries the run's token stats under the same alias set.
#[derive(Debug, Deserialize)]
struct GeminiHeadlessLine {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    stats: Option<GeminiTokenCounts>,
    #[serde(default)]
    timestamp: Option<serde_json::Value>,
}

/// Token totals of one counted Gemini message, with the cached portion
/// already subtracted from the input count.
struct GeminiUsageEntry {
    model: Option<String>,
    input: u64,
    output: u64,
    cached: u64,
    /// Message time (or file mtime fallback), used for per-day cost buckets.
    timestamp: Option<DateTime<Utc>>,
}

impl GeminiUsageEntry {
    fn from_counts(
        model: Option<String>,
        counts: &GeminiTokenCounts,
        timestamp: Option<DateTime<Utc>>,
    ) -> Self {
        let cached = counts.cached.unwrap_or(0);
        let input = counts.input.unwrap_or(0).saturating_sub(cached);
        // Thought tokens are generated output in the Gemini API contract, so
        // they count toward output; tool tokens ride the prompt side and are
        // left out rather than guessed.
        let output = counts
            .output
            .unwrap_or(0)
            .saturating_add(counts.thoughts.unwrap_or(0));
        Self {
            model,
            input,
            output,
            cached,
            timestamp,
        }
    }
}

/// Stream one Gemini chat document's in-window messages (or one headless
/// JSONL run's result line) into `on_entry`, returning how many were counted.
fn for_each_gemini_entry<F>(
    path: &Path,
    cutoff: &DateTime<Utc>,
    seen: &mut HashSet<String>,
    cancel: Option<&AtomicBool>,
    mut on_entry: F,
) -> u32
where
    F: FnMut(&GeminiUsageEntry),
{
    let Ok(text) = fs::read_to_string(path) else {
        return 0;
    };

    // Chat documents are single JSON objects with a `messages` array. The
    // struct defaults make almost any object decode, so only treat it as a
    // document when messages actually exist — otherwise fall through to the
    // headless JSONL reader.
    if let Ok(doc) = serde_json::from_str::<GeminiChatDoc>(&text)
        && !doc.messages.is_empty()
    {
        let mtime = file_mtime(path);
        let mut counted = 0;
        for message in &doc.messages {
            if is_cancelled(cancel) {
                break;
            }
            let Some(tokens) = message.tokens.as_ref() else {
                continue;
            };
            if tokens.is_empty() {
                continue;
            }
            if message.kind.as_deref() == Some("user") {
                continue;
            }
            let timestamp = parse_flexible_timestamp(message.timestamp.as_ref()).or(mtime);
            if let Some(recorded) = timestamp
                && recorded < *cutoff
            {
                continue;
            }
            // One message can be replayed into both the chats/ layout and a
            // legacy session-*.json file, so identical session+message ids
            // must only count once.
            let dedup_key = format!(
                "{}:{}",
                doc.session_id.clone().unwrap_or_default(),
                message.id.clone().unwrap_or_default()
            );
            if !message.id.is_none() && !seen.insert(dedup_key) {
                continue;
            }
            on_entry(&GeminiUsageEntry::from_counts(
                message
                    .model
                    .clone()
                    .filter(|model| !model.trim().is_empty()),
                tokens,
                timestamp,
            ));
            counted += 1;
        }
        return counted;
    }

    // Headless JSONL: `init` names the model, `result` carries the stats.
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => return 0,
    };
    let mtime = file_mtime(path);
    let mut last_init_model: Option<String> = None;
    let mut counted = 0;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if is_cancelled(cancel) {
            break;
        }
        if !line.contains("\"result\"") && !line.contains("\"init\"") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<GeminiHeadlessLine>(&line) else {
            continue;
        };
        match parsed.kind.as_deref() {
            Some("init") => {
                last_init_model = parsed.model.filter(|model| !model.trim().is_empty());
            }
            Some("result") => {
                let Some(stats) = parsed.stats.as_ref() else {
                    continue;
                };
                if stats.is_empty() {
                    continue;
                }
                let timestamp = parse_flexible_timestamp(parsed.timestamp.as_ref()).or(mtime);
                if let Some(recorded) = timestamp
                    && recorded < *cutoff
                {
                    continue;
                }
                on_entry(&GeminiUsageEntry::from_counts(
                    last_init_model.clone(),
                    stats,
                    timestamp,
                ));
                counted += 1;
            }
            _ => {}
        }
    }
    counted
}

fn add_gemini_entry_to_summary(
    summary: &mut CostSummary,
    entry: &GeminiUsageEntry,
    pricing_cache_root: Option<&Path>,
) {
    summary.input_tokens += entry.input;
    summary.output_tokens += entry.output;
    summary.cached_tokens += entry.cached;
    match entry.model.as_deref() {
        Some(model) => {
            let tokens = summary
                .by_model_tokens
                .entry(model.to_string())
                .or_default();
            tokens.input_tokens += entry.input;
            tokens.output_tokens += entry.output;
            tokens.cached_tokens += entry.cached;
            match price_tokens_with_candidates(
                GEMINI_PRICING_PROVIDERS,
                model,
                entry.input,
                entry.cached,
                entry.output,
                pricing_cache_root,
            ) {
                Some(cost) => {
                    summary.total_cost_usd += cost;
                    *summary.by_model.entry(model.to_string()).or_insert(0.0) += cost;
                }
                None => mark_model_unpriced(summary, model),
            }
        }
        None if entry.input + entry.output + entry.cached > 0 => {
            mark_model_unpriced(summary, UNKNOWN_MODEL_LABEL);
        }
        None => {}
    }
}

fn add_antigravity_event_to_summary(
    summary: &mut CostSummary,
    event: &crate::providers::antigravity::local_history::GenerationEvent,
    pricing_cache_root: Option<&Path>,
) {
    summary.input_tokens += event.new_input;
    summary.output_tokens += event.billed_output();
    summary.cached_tokens += event.cache_read;
    match event.model.as_deref() {
        Some(model) => {
            let tokens = summary
                .by_model_tokens
                .entry(model.to_string())
                .or_default();
            tokens.input_tokens += event.new_input;
            tokens.output_tokens += event.billed_output();
            tokens.cached_tokens += event.cache_read;
            match antigravity_cost_usd(
                model,
                event.new_input,
                event.cache_read,
                event.billed_output(),
                pricing_cache_root,
            ) {
                Some(cost) => {
                    summary.total_cost_usd += cost;
                    *summary.by_model.entry(model.to_string()).or_insert(0.0) += cost;
                }
                None => mark_model_unpriced(summary, model),
            }
        }
        None if event.total_tokens() > 0 => {
            mark_model_unpriced(summary, UNKNOWN_MODEL_LABEL);
        }
        None => {}
    }
}

fn add_antigravity_event_to_daily_costs(
    daily_costs: &mut HashMap<String, f64>,
    event: &crate::providers::antigravity::local_history::GenerationEvent,
    pricing_cache_root: Option<&Path>,
) {
    let Some(model) = event.model.as_deref() else {
        return;
    };
    let Some(cost) = antigravity_cost_usd(
        model,
        event.new_input,
        event.cache_read,
        event.billed_output(),
        pricing_cache_root,
    ) else {
        return;
    };
    let Some(timestamp) = DateTime::<Utc>::from_timestamp_millis(event.timestamp_ms) else {
        return;
    };
    if let Some(bucket) = daily_costs.get_mut(&daily_cost_day_key(timestamp)) {
        *bucket += cost;
    }
}

// ── Copilot CLI session-state snapshots ─────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopilotUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_read_tokens: Option<u64>,
    #[serde(default)]
    cache_write_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CopilotModelMetrics {
    #[serde(default)]
    usage: Option<CopilotUsage>,
}

/// Per-model token deltas after differencing one shutdown snapshot against the
/// previous one. `input` is net of both cache buckets (the log's inputTokens
/// is inclusive), `cached` is the separate cache traffic.
#[derive(Debug, Default, Clone)]
struct CopilotModelDelta {
    input: u64,
    output: u64,
    cached: u64,
}

/// Stream one `events.jsonl`'s `session.shutdown` snapshots into `on_event` as
/// per-model deltas, returning how many snapshots were counted.
fn for_each_copilot_shutdown<F>(
    path: &Path,
    cutoff: &DateTime<Utc>,
    cancel: Option<&AtomicBool>,
    mut on_event: F,
) -> u32
where
    F: FnMut(&HashMap<String, CopilotModelDelta>, Option<DateTime<Utc>>),
{
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let mtime = file_mtime(path);
    let mut previous: HashMap<String, CopilotUsage> = HashMap::new();
    let mut counted = 0;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if is_cancelled(cancel) {
            break;
        }
        if !line.contains("session.shutdown") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|kind| kind.as_str()) != Some("session.shutdown") {
            continue;
        }
        let Some(metrics) = value
            .get("data")
            .and_then(|data| data.get("modelMetrics"))
            .and_then(|metrics| {
                serde_json::from_value::<HashMap<String, CopilotModelMetrics>>(metrics.clone()).ok()
            })
        else {
            continue;
        };
        let timestamp = parse_flexible_timestamp(value.get("timestamp")).or(mtime);
        if let Some(recorded) = timestamp
            && recorded < *cutoff
        {
            continue;
        }

        let mut deltas: HashMap<String, CopilotModelDelta> = HashMap::new();
        let mut any = false;
        for (model, metrics) in metrics {
            let Some(usage) = metrics.usage else {
                continue;
            };
            let previous_usage = previous.get(&model);
            let delta_of = |current: Option<u64>, earlier: Option<u64>| {
                current.unwrap_or(0).saturating_sub(earlier.unwrap_or(0))
            };
            let cache_read = usage.cache_read_tokens.unwrap_or(0);
            let cache_write = usage.cache_write_tokens.unwrap_or(0);
            let gross_input = usage.input_tokens.unwrap_or(0);
            let (prev_read, prev_write, prev_gross) = previous_usage
                .map(|prev| {
                    (
                        prev.cache_read_tokens.unwrap_or(0),
                        prev.cache_write_tokens.unwrap_or(0),
                        prev.input_tokens.unwrap_or(0),
                    )
                })
                .unwrap_or((0, 0, 0));
            let delta = CopilotModelDelta {
                input: gross_input
                    .saturating_sub(cache_read)
                    .saturating_sub(cache_write)
                    .saturating_sub(
                        prev_gross
                            .saturating_sub(prev_read)
                            .saturating_sub(prev_write),
                    ),
                output: delta_of(
                    usage.output_tokens,
                    previous_usage.and_then(|prev| prev.output_tokens),
                ),
                cached: (cache_read + cache_write).saturating_sub(prev_read + prev_write),
            };
            previous.insert(model.clone(), usage);
            if delta.input > 0 || delta.output > 0 || delta.cached > 0 {
                any = true;
                deltas.insert(model, delta);
            }
        }
        if any {
            on_event(&deltas, timestamp);
            counted += 1;
        }
    }

    counted
}

fn add_copilot_deltas_to_summary(
    summary: &mut CostSummary,
    deltas: &HashMap<String, CopilotModelDelta>,
    pricing_cache_root: Option<&Path>,
) {
    for (model, delta) in deltas {
        summary.input_tokens += delta.input;
        summary.output_tokens += delta.output;
        summary.cached_tokens += delta.cached;
        let tokens = summary.by_model_tokens.entry(model.clone()).or_default();
        tokens.input_tokens += delta.input;
        tokens.output_tokens += delta.output;
        tokens.cached_tokens += delta.cached;
        match price_tokens_with_candidates(
            COPILOT_PRICING_PROVIDERS,
            model,
            delta.input,
            delta.cached,
            delta.output,
            pricing_cache_root,
        ) {
            Some(cost) => {
                summary.total_cost_usd += cost;
                *summary.by_model.entry(model.clone()).or_insert(0.0) += cost;
            }
            None => mark_model_unpriced(summary, model),
        }
    }
}

// ── Kimi CLI / Kimi Code wire logs ──────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
struct KimiTokenUsage {
    #[serde(default, alias = "inputOther")]
    input_other: Option<u64>,
    #[serde(default)]
    output: Option<u64>,
    #[serde(default, alias = "inputCacheRead")]
    input_cache_read: Option<u64>,
    #[serde(default, alias = "inputCacheCreation")]
    input_cache_creation: Option<u64>,
}

/// One counted Kimi usage record. `input_other` is the non-cached portion;
/// cache reads and creations sit alongside it (Claude-style).
struct KimiUsageEntry {
    model: Option<String>,
    input_other: u64,
    output: u64,
    cache_read: u64,
    cache_creation: u64,
    timestamp: Option<DateTime<Utc>>,
}

const KIMI_SYMBOLIC_MODEL: &str = "__kimi_env_model__";
const KIMI_CODE_MODEL_PREFIX: &str = "kimi-code/";

fn normalize_kimi_model(raw: Option<&str>) -> Option<String> {
    let model = raw?.trim();
    if model.is_empty() || model == KIMI_SYMBOLIC_MODEL {
        return None;
    }
    let model = model.strip_prefix(KIMI_CODE_MODEL_PREFIX).unwrap_or(model);
    let model = model.trim();
    (!model.is_empty()).then(|| model.to_string())
}

/// Stream one `wire.jsonl`'s usage rows into `on_entry`, returning how many
/// were counted. Both dialects are handled in one pass: CLI rows carry a
/// `message.payload.token_usage` running total keyed by `message_id` (later
/// rows replace earlier ones, so finals are emitted at end of file), while
/// Kimi Code rows are turn-scoped `usage.record` deltas emitted directly.
fn for_each_kimi_entry<F>(
    path: &Path,
    cutoff: &DateTime<Utc>,
    cancel: Option<&AtomicBool>,
    mut on_entry: F,
) -> u32
where
    F: FnMut(&KimiUsageEntry),
{
    let Ok(file) = File::open(path) else {
        return 0;
    };
    let mtime = file_mtime(path);
    let mut counted = 0u32;

    // CLI dialect: final token_usage per message id.
    let mut cli_finals: HashMap<String, (KimiTokenUsage, Option<DateTime<Utc>>)> = HashMap::new();
    // Code dialect: model named by the preceding `llm.request` row.
    let mut last_request_model: Option<String> = None;

    for line in BufReader::new(file).lines().map_while(Result::ok) {
        if is_cancelled(cancel) {
            break;
        }
        if !line.contains("token_usage")
            && !line.contains("usage.record")
            && !line.contains("llm.request")
        {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if let Some(message) = value.get("message") {
            // Kimi CLI dialect.
            let Some(payload) = message.get("payload") else {
                continue;
            };
            let Ok(usage) = serde_json::from_value::<KimiTokenUsage>(
                payload.get("token_usage").cloned().unwrap_or_default(),
            ) else {
                continue;
            };
            let timestamp = parse_flexible_timestamp(value.get("timestamp")).or(mtime);
            let message_id = payload
                .get("message_id")
                .or_else(|| payload.get("messageId"))
                .and_then(|id| id.as_str())
                .unwrap_or_default()
                .to_string();
            let entry_key = if message_id.is_empty() {
                format!("line-{}", counted as usize + cli_finals.len())
            } else {
                message_id
            };
            cli_finals.insert(entry_key, (usage, timestamp));
            continue;
        }

        if value.get("type").and_then(|kind| kind.as_str()) == Some("llm.request") {
            // The concrete model arrives on the request row; turn records may
            // only carry the symbolic env model.
            if let Some(model) =
                normalize_kimi_model(value.get("model").and_then(|model| model.as_str()))
            {
                last_request_model = Some(model);
            }
            continue;
        }

        if value.get("type").and_then(|kind| kind.as_str()) == Some("usage.record") {
            // Kimi Code dialect: turn-scoped records.
            let scope = value.get("usageScope").and_then(|scope| scope.as_str());
            if scope.is_some_and(|scope| scope != "turn") {
                // Session-scoped records are cumulative bookkeeping, not spend.
                if let Some(model) = value.get("model").and_then(|model| model.as_str()) {
                    if let Some(model) = normalize_kimi_model(Some(model)) {
                        last_request_model = Some(model);
                    }
                }
                continue;
            }
            let Ok(usage) = serde_json::from_value::<KimiTokenUsage>(
                value.get("usage").cloned().unwrap_or_default(),
            ) else {
                continue;
            };
            if usage.output.is_none()
                && usage.input_other.is_none()
                && usage.input_cache_read.is_none()
                && usage.input_cache_creation.is_none()
            {
                continue;
            }
            let timestamp = parse_flexible_timestamp(value.get("time")).or(mtime);
            if let Some(recorded) = timestamp
                && recorded < *cutoff
            {
                continue;
            }
            let model = normalize_kimi_model(value.get("model").and_then(|model| model.as_str()))
                .or_else(|| last_request_model.clone());
            on_entry(&KimiUsageEntry {
                model,
                input_other: usage.input_other.unwrap_or(0),
                output: usage.output.unwrap_or(0),
                cache_read: usage.input_cache_read.unwrap_or(0),
                cache_creation: usage.input_cache_creation.unwrap_or(0),
                timestamp,
            });
            counted += 1;
        }
    }

    // Emit the CLI finals whose last update landed inside the window.
    for (usage, timestamp) in cli_finals.into_values() {
        if let Some(recorded) = timestamp
            && recorded < *cutoff
        {
            continue;
        }
        if usage.output.is_none()
            && usage.input_other.is_none()
            && usage.input_cache_read.is_none()
            && usage.input_cache_creation.is_none()
        {
            continue;
        }
        on_entry(&KimiUsageEntry {
            model: None,
            input_other: usage.input_other.unwrap_or(0),
            output: usage.output.unwrap_or(0),
            cache_read: usage.input_cache_read.unwrap_or(0),
            cache_creation: usage.input_cache_creation.unwrap_or(0),
            timestamp,
        });
        counted += 1;
    }

    counted
}

fn add_kimi_entry_to_summary(
    summary: &mut CostSummary,
    entry: &KimiUsageEntry,
    pricing_cache_root: Option<&Path>,
) {
    summary.input_tokens += entry.input_other;
    summary.output_tokens += entry.output;
    summary.cached_tokens += entry.cache_read + entry.cache_creation;
    match entry.model.as_deref() {
        Some(model) => {
            let tokens = summary
                .by_model_tokens
                .entry(model.to_string())
                .or_default();
            tokens.input_tokens += entry.input_other;
            tokens.output_tokens += entry.output;
            tokens.cached_tokens += entry.cache_read + entry.cache_creation;
            match price_tokens_with_candidates(
                KIMI_PRICING_PROVIDERS,
                model,
                entry.input_other,
                entry.cache_read + entry.cache_creation,
                entry.output,
                pricing_cache_root,
            ) {
                Some(cost) => {
                    summary.total_cost_usd += cost;
                    *summary.by_model.entry(model.to_string()).or_insert(0.0) += cost;
                }
                None => mark_model_unpriced(summary, model),
            }
        }
        // The Kimi CLI dialect names no model, so its spend cannot be priced
        // from a catalog entry; say so instead of reporting complete pricing.
        None if entry.input_other + entry.output + entry.cache_read + entry.cache_creation > 0 => {
            mark_model_unpriced(summary, UNKNOWN_MODEL_LABEL);
        }
        None => {}
    }
}

// ── Amp thread ledgers ──────────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AmpTokens {
    #[serde(default)]
    input: Option<u64>,
    #[serde(default)]
    output: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AmpLedgerEvent {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    credits: Option<f64>,
    #[serde(default)]
    tokens: Option<AmpTokens>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AmpThread {
    #[serde(default)]
    created: Option<i64>,
    #[serde(default, rename = "usageLedger")]
    usage_ledger: Option<AmpLedger>,
}

#[derive(Debug, Deserialize)]
struct AmpLedger {
    #[serde(default)]
    events: Vec<AmpLedgerEvent>,
}

/// One counted Amp ledger event. `credits` is Amp's own USD-unit billing
/// figure and is treated as provider-reported cost, not an estimate.
struct AmpUsageEvent<'a> {
    model: Option<&'a str>,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_creation: u64,
    credits: f64,
    timestamp: Option<DateTime<Utc>>,
}

/// Stream one thread's ledger events into `on_event`, returning how many were
/// counted. Events without their own timestamp anchor to the thread creation
/// time, then the file mtime — the same fallback order tokscale uses.
fn for_each_amp_event<F>(
    path: &Path,
    cutoff: &DateTime<Utc>,
    cancel: Option<&AtomicBool>,
    mut on_event: F,
) -> u32
where
    F: FnMut(&AmpUsageEvent),
{
    let Ok(text) = fs::read_to_string(path) else {
        return 0;
    };
    let Ok(thread) = serde_json::from_str::<AmpThread>(&text) else {
        return 0;
    };
    let Some(ledger) = thread.usage_ledger else {
        return 0;
    };
    let mtime = file_mtime(path);
    let thread_created = thread
        .created
        .and_then(|ms| DateTime::<Utc>::from_timestamp_millis(ms));
    let mut counted = 0;

    for event in &ledger.events {
        if is_cancelled(cancel) {
            break;
        }
        let tokens = event.tokens.as_ref();
        let timestamp = event
            .timestamp
            .as_deref()
            .and_then(|text| DateTime::parse_from_rfc3339(text.trim()).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .or(thread_created)
            .or(mtime);
        if let Some(recorded) = timestamp
            && recorded < *cutoff
        {
            continue;
        }
        let input = tokens.and_then(|tokens| tokens.input).unwrap_or(0);
        let output = tokens.and_then(|tokens| tokens.output).unwrap_or(0);
        let cache_read = tokens
            .and_then(|tokens| tokens.cache_read_input_tokens)
            .unwrap_or(0);
        let cache_creation = tokens
            .and_then(|tokens| tokens.cache_creation_input_tokens)
            .unwrap_or(0);
        if input == 0
            && output == 0
            && cache_read == 0
            && cache_creation == 0
            && event.credits.unwrap_or(0.0) == 0.0
        {
            continue;
        }
        on_event(&AmpUsageEvent {
            model: event
                .model
                .as_deref()
                .filter(|model| !model.trim().is_empty()),
            input,
            output,
            cache_read,
            cache_creation,
            credits: event.credits.unwrap_or(0.0).max(0.0),
            timestamp,
        });
        counted += 1;
    }

    counted
}

fn add_amp_event_to_summary(summary: &mut CostSummary, event: &AmpUsageEvent) {
    summary.total_cost_usd += event.credits;
    summary.input_tokens += event.input;
    summary.output_tokens += event.output;
    summary.cached_tokens += event.cache_read + event.cache_creation;
    let tokens_spent = event.input + event.output + event.cache_read + event.cache_creation;
    if let Some(model) = event.model {
        *summary.by_model.entry(model.to_string()).or_insert(0.0) += event.credits;
        let tokens = summary
            .by_model_tokens
            .entry(model.to_string())
            .or_default();
        tokens.input_tokens += event.input;
        tokens.output_tokens += event.output;
        tokens.cached_tokens += event.cache_read + event.cache_creation;
        // Amp's ledger normally carries the provider-reported `credits`; a
        // token-bearing event without one is unpriced, not free.
        if event.credits <= 0.0 && tokens_spent > 0 {
            mark_model_unpriced(summary, model);
        }
    } else if event.credits <= 0.0 && tokens_spent > 0 {
        mark_model_unpriced(summary, UNKNOWN_MODEL_LABEL);
    }
}

fn add_amp_event_to_daily_costs(daily_costs: &mut HashMap<String, f64>, event: &AmpUsageEvent) {
    let Some(timestamp) = event.timestamp else {
        return;
    };
    let date_str = timestamp
        .with_timezone(&Local)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    if let Some(cost) = daily_costs.get_mut(&date_str) {
        *cost += event.credits;
    }
}

fn daily_cost_day_key(timestamp: DateTime<Utc>) -> String {
    timestamp
        .with_timezone(&Local)
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn add_gemini_entry_to_daily_costs(
    daily_costs: &mut HashMap<String, f64>,
    entry: &GeminiUsageEntry,
    pricing_cache_root: Option<&Path>,
) {
    let (Some(timestamp), Some(model)) = (entry.timestamp, entry.model.as_deref()) else {
        return;
    };
    let Some(cost) = price_tokens_with_candidates(
        GEMINI_PRICING_PROVIDERS,
        model,
        entry.input,
        entry.cached,
        entry.output,
        pricing_cache_root,
    ) else {
        return;
    };
    if let Some(bucket) = daily_costs.get_mut(&daily_cost_day_key(timestamp)) {
        *bucket += cost;
    }
}

fn add_copilot_deltas_to_daily_costs(
    daily_costs: &mut HashMap<String, f64>,
    deltas: &HashMap<String, CopilotModelDelta>,
    timestamp: Option<DateTime<Utc>>,
    pricing_cache_root: Option<&Path>,
) {
    let Some(timestamp) = timestamp else {
        return;
    };
    let mut total = 0.0;
    for (model, delta) in deltas {
        if let Some(cost) = price_tokens_with_candidates(
            COPILOT_PRICING_PROVIDERS,
            model,
            delta.input,
            delta.cached,
            delta.output,
            pricing_cache_root,
        ) {
            total += cost;
        }
    }
    if let Some(bucket) = daily_costs.get_mut(&daily_cost_day_key(timestamp)) {
        *bucket += total;
    }
}

fn add_kimi_entry_to_daily_costs(
    daily_costs: &mut HashMap<String, f64>,
    entry: &KimiUsageEntry,
    pricing_cache_root: Option<&Path>,
) {
    let (Some(timestamp), Some(model)) = (entry.timestamp, entry.model.as_deref()) else {
        return;
    };
    let Some(cost) = price_tokens_with_candidates(
        KIMI_PRICING_PROVIDERS,
        model,
        entry.input_other,
        entry.cache_read + entry.cache_creation,
        entry.output,
        pricing_cache_root,
    ) else {
        return;
    };
    if let Some(bucket) = daily_costs.get_mut(&daily_cost_day_key(timestamp)) {
        *bucket += cost;
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
    get_daily_cost_history_with_budget_and_pricing_root(provider, days, cancel, deadline, None)
}

/// `get_daily_cost_history_with_budget` with an explicit pricing cache root so
/// tests can exercise priced per-day buckets without touching the machine's
/// real models.dev cache. Production callers use the wrapper above.
fn get_daily_cost_history_with_budget_and_pricing_root(
    provider: &str,
    days: u32,
    cancel: Option<&AtomicBool>,
    deadline: Option<Instant>,
    pricing_cache_root: Option<&Path>,
) -> (Vec<(String, f64)>, bool) {
    if days == 0 {
        return (Vec::new(), false);
    }
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
            // One lineage-deduplicated pass over the turn stream, then bucket
            // by each turn's own day — the fork/replay duplication that made
            // per-day re-walks both slow and inflated is removed at the
            // source (see collect_unique_codex_turns).
            let window = CostUsageDayRange::new(today - Duration::days((days - 1).into()), today);
            let turns = scanner.collect_unique_codex_turns(cancel, deadline);
            for turn in &turns {
                if !CostUsageDayRange::is_in_range(&turn.day, &window.since_key, &window.until_key)
                {
                    continue;
                }
                if let Some(cost) = codex_turn_cost_at_date(
                    &turn.model,
                    turn.input,
                    turn.cached,
                    turn.output,
                    CostUsageDayRange::parse_day_key(&turn.day),
                ) {
                    *daily_costs.entry(turn.day.clone()).or_insert(0.0) += cost;
                }
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
        "grok" => {
            let sessions_dir = scanner.get_grok_sessions_dir();
            if sessions_dir.exists() {
                let cutoff = Utc::now() - Duration::days(days as i64);
                let mut seen = HashSet::new();
                let mut handle_file = |path: &Path| {
                    for_each_grok_turn(path, &cutoff, &mut seen, cancel, |timestamp, turn| {
                        add_grok_turn_to_daily_costs(&mut daily_costs, timestamp, turn);
                    });
                };
                scanner.walk_grok_files(&sessions_dir, &cutoff, cancel, &mut handle_file);
            }
        }
        "gemini" => {
            let root = scanner.get_gemini_tmp_dir();
            if root.exists() {
                let cutoff = Utc::now() - Duration::days(days as i64);
                let mut seen = HashSet::new();
                let mut handle_file = |path: &Path| {
                    for_each_gemini_entry(path, &cutoff, &mut seen, cancel, |entry| {
                        add_gemini_entry_to_daily_costs(
                            &mut daily_costs,
                            entry,
                            pricing_cache_root,
                        );
                    });
                };
                scanner.walk_source_files(&root, &cutoff, cancel, &mut handle_file);
            }
        }
        "copilot" => {
            let root = scanner.get_copilot_session_state_dir();
            if root.exists() {
                let cutoff = Utc::now() - Duration::days(days as i64);
                let mut handle_file = |path: &Path| {
                    let is_events_log = path.file_name().is_some_and(|name| name == "events.jsonl");
                    if !is_events_log {
                        return;
                    }
                    for_each_copilot_shutdown(path, &cutoff, cancel, |deltas, timestamp| {
                        add_copilot_deltas_to_daily_costs(
                            &mut daily_costs,
                            &deltas,
                            timestamp,
                            pricing_cache_root,
                        );
                    });
                };
                scanner.walk_source_files(&root, &cutoff, cancel, &mut handle_file);
            }
        }
        "kimi" => {
            for root in scanner.get_kimi_session_roots() {
                if !root.exists() {
                    continue;
                }
                let cutoff = Utc::now() - Duration::days(days as i64);
                let mut handle_file = |path: &Path| {
                    let is_wire_log = path.file_name().is_some_and(|name| name == "wire.jsonl");
                    if !is_wire_log {
                        return;
                    }
                    for_each_kimi_entry(path, &cutoff, cancel, |entry| {
                        add_kimi_entry_to_daily_costs(&mut daily_costs, entry, pricing_cache_root);
                    });
                };
                scanner.walk_source_files(&root, &cutoff, cancel, &mut handle_file);
            }
        }
        "antigravity" => {
            use crate::providers::antigravity::local_history::ScanCoverage;

            let scan = crate::providers::antigravity::local_history::scan(cancel);
            if scan.coverage != ScanCoverage::Unavailable {
                let cutoff_ms = (Utc::now() - Duration::days(days as i64)).timestamp_millis();
                for event in &scan.events {
                    if event.timestamp_ms < cutoff_ms {
                        continue;
                    }
                    add_antigravity_event_to_daily_costs(
                        &mut daily_costs,
                        event,
                        pricing_cache_root,
                    );
                }
            }
        }
        "amp" => {
            let threads_dir = scanner.get_amp_threads_dir();
            if threads_dir.exists() {
                let cutoff = Utc::now() - Duration::days(days as i64);
                let mut handle_file = |path: &Path| {
                    for_each_amp_event(path, &cutoff, cancel, |event| {
                        add_amp_event_to_daily_costs(&mut daily_costs, event);
                    });
                };
                scanner.walk_source_files(&threads_dir, &cutoff, cancel, &mut handle_file);
            }
        }
        "opencode" | "opencodego" => {
            // One bounded SQL read per provider; the rows already carry
            // OpenCode's own per-message cost, so no per-day re-derivation.
            let filter = if provider == "opencodego" {
                "opencode-go"
            } else {
                "opencode"
            };
            for (day, cost) in
                crate::providers::opencodego::local::daily_cost_series_for(filter, Utc::now(), days)
            {
                *daily_costs.entry(day).or_insert(0.0) += cost;
            }
        }
        _ => {}
    }

    // Codex file buckets parsed during the day walk are flushed with the
    // cache so subsequent history/summary scans skip unchanged files.
    if provider == "codex" {
        scan_cache::persist();
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
    use std::sync::Mutex;

    /// The new scanners locate their roots through process-global env vars
    /// (GEMINI_CLI_HOME, COPILOT_HOME, KIMI_HOME, KIMI_CODE_HOME,
    /// XDG_DATA_HOME). Tests that redirect them must serialize, or a parallel
    /// test's fixture directory leaks into another test's summary.
    static ENV_SCAN_LOCK: Mutex<()> = Mutex::new(());

    /// Write a synthetic models.dev cache artifact under `root` so pricing
    /// assertions are deterministic instead of depending on the machine's
    /// downloaded catalog.
    fn write_pricing_cache(root: &Path, providers: &str) {
        let dir = root.join("model-pricing");
        std::fs::create_dir_all(&dir).expect("pricing cache dir");
        let fetched_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or(0);
        let artifact = format!(
            r#"{{"version":1,"fetched_at_unix_ms":{fetched_at},"catalog":{{"providers":{providers}}}}}"#
        );
        std::fs::write(dir.join("models-dev-v1.json"), artifact).expect("pricing cache");
    }

    #[test]
    fn dedup_thread_turns_counts_fork_replay_once() {
        // Codex Desktop 分叉:新 rollout 文件重放整段祖先历史(day 被改写为
        // 分叉时刻)再追加自己的新 turn。去重后每个真实 turn 恰好一次。
        let root = CodexFileCache {
            mtime: 1,
            len: 1,
            thread: Some("root".to_string()),
            created: Some("2026-08-12T00:00:00Z".to_string()),
            turns: vec![
                CachedTurn {
                    day: "2026-08-12".to_string(),
                    model: "gpt-5.6-luna".to_string(),
                    input: 100,
                    cached: 50,
                    output: 10,
                },
                CachedTurn {
                    day: "2026-08-20".to_string(),
                    model: "gpt-5.6-luna".to_string(),
                    input: 200,
                    cached: 80,
                    output: 20,
                },
            ],
        };
        let fork = CodexFileCache {
            mtime: 2,
            len: 2,
            thread: Some("root".to_string()),
            created: Some("2026-08-29T00:00:00Z".to_string()),
            turns: vec![
                // 重放:值与 root 相同,day 已改写
                CachedTurn {
                    day: "2026-08-29".to_string(),
                    model: "gpt-5.6-luna".to_string(),
                    input: 100,
                    cached: 50,
                    output: 10,
                },
                CachedTurn {
                    day: "2026-08-29".to_string(),
                    model: "gpt-5.6-luna".to_string(),
                    input: 200,
                    cached: 80,
                    output: 20,
                },
                // 分叉后的真实新消费
                CachedTurn {
                    day: "2026-08-29".to_string(),
                    model: "gpt-5.6-luna".to_string(),
                    input: 30,
                    cached: 0,
                    output: 3,
                },
            ],
        };

        let unique = dedup_thread_turns(vec![fork, root]);
        assert_eq!(unique.len(), 3, "replayed history must be counted once");
        // root 的原始 day 保留(先创建先记账)
        assert_eq!(unique[0].day, "2026-08-12");
        assert_eq!(unique[1].day, "2026-08-20");
        // 分叉后的新 turn 保留自己的 day
        assert_eq!(unique[2].day, "2026-08-29");
        assert_eq!(unique[2].input, 30);
    }

    #[test]
    fn dedup_does_not_merge_equal_tokens_from_a_different_model() {
        let first = CodexFileCache {
            mtime: 1,
            len: 1,
            thread: Some("root".to_string()),
            created: Some("2026-08-12T00:00:00Z".to_string()),
            turns: vec![CachedTurn {
                day: "2026-08-12".to_string(),
                model: "gpt-5.6-sol".to_string(),
                input: 100,
                cached: 50,
                output: 10,
            }],
        };
        let second = CodexFileCache {
            mtime: 2,
            len: 2,
            thread: Some("root".to_string()),
            created: Some("2026-08-13T00:00:00Z".to_string()),
            turns: vec![CachedTurn {
                day: "2026-08-13".to_string(),
                model: "gpt-5.6-luna".to_string(),
                input: 100,
                cached: 50,
                output: 10,
            }],
        };

        let unique = dedup_thread_turns(vec![first, second]);
        assert_eq!(unique.len(), 2);
    }

    #[test]
    fn daily_history_zero_days_is_empty_and_not_claimed_complete() {
        let (history, complete) = get_daily_cost_history_with_budget("codex", 0, None, None);
        assert!(history.is_empty());
        assert!(!complete);
    }

    #[test]
    fn codex_turns_apply_parses_and_prices_one_file() {
        let today = Local::now().date_naive();
        let ts = chrono::Utc::now()
            .format("%Y-%m-%dT%H:%M:%S%.3fZ")
            .to_string();
        let mut file = tempfile::NamedTempFile::new().expect("temp file");
        writeln!(
            file,
            r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"model":"gpt-5","total_token_usage":{{"input_tokens":125,"cached_input_tokens":30,"output_tokens":15}}}}}}}}"#
        )
        .unwrap();

        let entry = codex_file_turns(file.path()).expect("turns");
        assert_eq!(entry.turns.len(), 1);

        let range = CostUsageDayRange::new(codex_period_start(today, 30), today);
        let mut summary = CostSummary::default();
        let (cost, turn_count) = apply_codex_turns_to_summary(&mut summary, &entry.turns, &range);

        assert_eq!(turn_count, 1);
        assert_eq!(
            summary.sessions_count, 0,
            "sessions belongs to scan_codex, not apply"
        );
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
        assert!(cost > 0.0);
        assert!(scan_codex_file_cost(file.path()) > 0.0);
    }

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

        let entry = codex_file_turns(&path).expect("turns");
        let today = Local::now().date_naive();
        let range = CostUsageDayRange::new(codex_period_start(today, 30), today);
        let mut summary = CostSummary::default();
        let (total_cost, turn_count) =
            apply_codex_turns_to_summary(&mut summary, &entry.turns, &range);
        summary.total_cost_usd += total_cost;
        summary.sessions_count = turn_count;

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
            summary
                .by_model_tokens
                .get("grok-4.5-build")
                .map(ModelTokenCounts::total),
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
        let counted = for_each_grok_turn(&path, &cutoff, &mut seen, None, |_, turn| {
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
        let counted = for_each_grok_turn(&path, &cutoff, &mut seen, None, |_, _| {});

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
        let counted = for_each_grok_turn(&path, &cutoff, &mut seen, None, |_, _| {});

        assert_eq!(counted, 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_daily_cost_history_includes_grok_turns() {
        let dir = std::env::temp_dir().join(format!("grok-hist-{}", std::process::id()));
        let session_dir = dir.join("sessions").join("test-session");
        std::fs::create_dir_all(&session_dir).expect("temp dir");
        let path = session_dir.join("updates.jsonl");

        let today_dt = chrono::Local::now();
        let today_ts = today_dt.timestamp();
        let today_date_str = today_dt.date_naive().format("%Y-%m-%d").to_string();
        let turn_line = GROK_TURN.replace("1785573295", &today_ts.to_string());
        std::fs::write(&path, format!("{turn_line}\n")).expect("write");

        unsafe {
            std::env::set_var("GROK_HOME", &dir);
        }

        let (history, stopped) = get_daily_cost_history_with_budget("grok", 7, None, None);
        assert!(!stopped);
        let today_entry = history.iter().find(|(date, _)| date == &today_date_str);
        assert!(
            today_entry.is_some(),
            "today entry must exist in grok history"
        );
        assert!(today_entry.unwrap().1 > 0.0, "today cost must be > 0");

        unsafe {
            std::env::remove_var("GROK_HOME");
        }
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

    // ── Gemini CLI chat documents ───────────────────────────────────────────

    /// A chat document whose messages were all written "now" so they survive
    /// any realistic cutoff.
    fn gemini_chat_doc(session_id: &str, msg_id: &str, tokens: &str) -> String {
        format!(
            r#"{{"sessionId":"{session_id}","projectHash":"hash","messages":[{{"id":"{msg_id}","timestamp":"{now}","type":"gemini","model":"gemini-2.5-pro","tokens":{tokens}}}]}}"#,
            now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        )
    }

    #[test]
    fn gemini_chat_doc_nets_cached_tokens_from_input() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // promptTokenCount is cache-inclusive: 1000 prompt − 700 cached = 300
        // net input, while the cached count lands in its own bucket.
        let doc = gemini_chat_doc(
            "s1",
            "m1",
            r#"{"promptTokenCount":1000,"candidatesTokenCount":80,"cachedContentTokenCount":700,"thoughtsTokenCount":20}"#,
        );
        let dir = std::env::temp_dir().join(format!("gemini-scan-{}", std::process::id()));
        let chats = dir.join("tmp").join("hash").join("chats");
        std::fs::create_dir_all(&chats).expect("temp dir");
        std::fs::write(chats.join("chat.json"), doc).expect("write");

        unsafe {
            std::env::set_var("GEMINI_CLI_HOME", &dir);
        }
        let summary = CostScanner::new(30)
            .with_pricing_cache_root(dir.join("empty-pricing"))
            .scan_gemini();
        unsafe {
            std::env::remove_var("GEMINI_CLI_HOME");
        }

        assert_eq!(summary.sessions_count, 1);
        assert_eq!(summary.input_tokens, 300, "input must be net of cached");
        assert_eq!(summary.output_tokens, 100, "thoughts count as output");
        assert_eq!(summary.cached_tokens, 700);
        assert!(
            summary.total_cost_usd == 0.0,
            "no catalog rate resolves from an empty cache root"
        );
        assert!(
            !summary.model_pricing_completeness.is_complete(),
            "token-only sources stay explicitly unpriced"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gemini_counts_a_message_once_across_legacy_and_chats_layouts() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let doc = gemini_chat_doc("s1", "m1", r#"{"input":50,"output":10}"#);
        let dir = std::env::temp_dir().join(format!("gemini-dedup-{}", std::process::id()));
        let chats = dir.join("tmp").join("hash").join("chats");
        let legacy = dir.join("tmp").join("legacy");
        std::fs::create_dir_all(&chats).expect("temp dir");
        std::fs::create_dir_all(&legacy).expect("temp dir");
        std::fs::write(chats.join("chat.json"), &doc).expect("write");
        std::fs::write(legacy.join("session-1.json"), &doc).expect("write");

        unsafe {
            std::env::set_var("GEMINI_CLI_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_gemini();
        unsafe {
            std::env::remove_var("GEMINI_CLI_HOME");
        }

        assert_eq!(summary.input_tokens, 50, "replayed message counts once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gemini_headless_result_line_is_counted_with_init_model() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now_ts = chrono::Utc::now().timestamp();
        let lines = format!(
            concat!(
                r#"{{"type":"init","model":"gemini-2.5-flash","session_id":"h1"}}"#,
                "\n",
                r#"{{"type":"result","stats":{{"input_tokens":120,"output_tokens":30}},"timestamp":{now_ts}}}"#
            ),
            now_ts = now_ts,
        );
        let dir = std::env::temp_dir().join(format!("gemini-headless-{}", std::process::id()));
        let chats = dir.join("tmp").join("hash").join("chats");
        std::fs::create_dir_all(&chats).expect("temp dir");
        std::fs::write(chats.join("run.jsonl"), lines).expect("write");

        unsafe {
            std::env::set_var("GEMINI_CLI_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_gemini();
        unsafe {
            std::env::remove_var("GEMINI_CLI_HOME");
        }

        assert_eq!(summary.input_tokens, 120);
        assert_eq!(summary.output_tokens, 30);
        assert!(
            summary.by_model_tokens.contains_key("gemini-2.5-flash"),
            "init-declared model must attribute the run: {:?}",
            summary.by_model_tokens
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Copilot CLI session-state snapshots ─────────────────────────────────

    #[test]
    fn copilot_shutdown_snapshots_are_differenced_not_summed() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // Cumulative snapshots: shutdown #1 totals 100 in / 20 out; shutdown #2
        // totals 150 in / 45 out. Only the deltas (100+50 in, 20+25 out) are
        // real spend; summing the snapshots would double the first window.
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let events = format!(
            concat!(
                r#"{{"type":"session.startup","data":{{}},"timestamp":"{now}"}}"#,
                "\n",
                r#"{{"type":"session.shutdown","id":"e1","timestamp":"{now}","data":{{"modelMetrics":{{"gpt-5":{{"usage":{{"inputTokens":100,"outputTokens":20,"cacheReadTokens":0,"cacheWriteTokens":0}}}}}}}}}}"#,
                "\n",
                r#"{{"type":"session.shutdown","id":"e2","timestamp":"{now}","data":{{"modelMetrics":{{"gpt-5":{{"usage":{{"inputTokens":150,"outputTokens":45,"cacheReadTokens":10,"cacheWriteTokens":0}}}}}}}}}}"#
            ),
            now = now,
        );
        let dir = std::env::temp_dir().join(format!("copilot-scan-{}", std::process::id()));
        let session = dir.join("session-state").join("sess-1");
        std::fs::create_dir_all(&session).expect("temp dir");
        std::fs::write(session.join("events.jsonl"), events).expect("write");

        unsafe {
            std::env::set_var("COPILOT_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_copilot();
        unsafe {
            std::env::remove_var("COPILOT_HOME");
        }

        assert_eq!(summary.sessions_count, 1);
        assert_eq!(summary.input_tokens, 140, "net of the 10 cached read delta");
        assert_eq!(summary.output_tokens, 45);
        assert_eq!(summary.cached_tokens, 10);
        // gpt-5 resolves through the canonical Codex table: 140 input at
        // $1.25/M, 10 cached at $0.125/M and 45 output at $10/M.
        let expected = 140.0 * 1.25e-6 + 10.0 * 1.25e-7 + 45.0 * 1e-5;
        assert!(
            (summary.total_cost_usd - expected).abs() < 1e-12,
            "expected {expected}, got {}",
            summary.total_cost_usd
        );
        assert!(
            summary.model_pricing_completeness.is_complete(),
            "embedded rates price every model in this fixture"
        );
        assert!((summary.by_model["gpt-5"] - expected).abs() < 1e-12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn copilot_ignores_non_event_files_and_incomplete_snapshots() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("copilot-skip-{}", std::process::id()));
        let session = dir.join("session-state").join("sess-1");
        std::fs::create_dir_all(&session).expect("temp dir");
        std::fs::write(
            session.join("events.jsonl"),
            r#"{"type":"session.shutdown","id":"e1","data":{"modelMetrics":{}}}"#,
        )
        .expect("write");
        std::fs::write(session.join("other.jsonl"), "not events").expect("write");

        unsafe {
            std::env::set_var("COPILOT_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_copilot();
        unsafe {
            std::env::remove_var("COPILOT_HOME");
        }

        assert_eq!(summary.sessions_count, 0);
        assert!(summary.known_zero, "an inspected empty log is a known zero");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Kimi wire logs ──────────────────────────────────────────────────────

    #[test]
    fn kimi_cli_running_totals_use_the_last_update_per_message() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        // StatusUpdate rows for one message carry running totals; only the
        // final row's counts are real spend.
        let now = chrono::Utc::now().timestamp() as f64;
        let lines = format!(
            concat!(
                r#"{{"timestamp":{t},"message":{{"type":"StatusUpdate","payload":{{"message_id":"m1","token_usage":{{"input_other":100,"output":10,"input_cache_read":5,"input_cache_creation":1}}}}}}}}"#,
                "\n",
                r#"{{"timestamp":{t},"message":{{"type":"StatusUpdate","payload":{{"message_id":"m1","token_usage":{{"input_other":200,"output":25,"input_cache_read":8,"input_cache_creation":2}}}}}}}}"#
            ),
            t = now,
        );
        let dir = std::env::temp_dir().join(format!("kimi-cli-{}", std::process::id()));
        let sessions = dir.join("sessions").join("g").join("s");
        std::fs::create_dir_all(&sessions).expect("temp dir");
        std::fs::write(sessions.join("wire.jsonl"), lines).expect("write");

        unsafe {
            std::env::set_var("KIMI_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_kimi();
        unsafe {
            std::env::remove_var("KIMI_HOME");
        }

        assert_eq!(summary.input_tokens, 200, "final running total wins");
        assert_eq!(summary.output_tokens, 25);
        assert_eq!(summary.cached_tokens, 10, "cache read + creation");
        assert!(
            !summary.model_pricing_completeness.is_complete(),
            "the CLI dialect names no model, so its tokens cannot be priced"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kimi_code_turn_records_count_directly_and_pair_models() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now_ms = chrono::Utc::now().timestamp_millis();
        let lines = format!(
            concat!(
                r#"{{"type":"llm.request","model":"kimi-code/kimi-for-coding","time":{t}}}"#,
                "\n",
                r#"{{"type":"usage.record","model":"__kimi_env_model__","usageScope":"turn","usage":{{"inputOther":300,"output":40,"inputCacheRead":12,"inputCacheCreation":3}},"time":{t}}}"#,
                "\n",
                r#"{{"type":"usage.record","usageScope":"session","usage":{{"inputOther":9999,"output":9999}},"time":{t}}}"#
            ),
            t = now_ms,
        );
        let dir = std::env::temp_dir().join(format!("kimi-code-{}", std::process::id()));
        let sessions = dir
            .join("sessions")
            .join("ws")
            .join("s")
            .join("agents")
            .join("a");
        std::fs::create_dir_all(&sessions).expect("temp dir");
        std::fs::write(sessions.join("wire.jsonl"), lines).expect("write");

        unsafe {
            std::env::set_var("KIMI_CODE_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_kimi();
        unsafe {
            std::env::remove_var("KIMI_CODE_HOME");
        }

        assert_eq!(
            summary.input_tokens, 300,
            "session-scope bookkeeping excluded"
        );
        assert_eq!(summary.output_tokens, 40);
        assert_eq!(summary.cached_tokens, 15);
        assert!(
            summary.by_model_tokens.contains_key("kimi-for-coding"),
            "request model attributed after stripping the route prefix: {:?}",
            summary.by_model_tokens
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Amp thread ledgers ──────────────────────────────────────────────────

    #[test]
    fn amp_ledger_credits_count_as_provider_reported_cost() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now_ms = chrono::Utc::now().timestamp_millis();
        let thread = format!(
            r#"{{"id":"T-1","created":{now_ms},"usageLedger":{{"events":[{{"model":"claude-sonnet-4-6","credits":0.42,"tokens":{{"input":900,"output":70,"cacheReadInputTokens":500,"cacheCreationInputTokens":30}}}}]}}}}"#
        );
        let dir = std::env::temp_dir().join(format!("amp-scan-{}", std::process::id()));
        let threads = dir.join("amp").join("threads");
        std::fs::create_dir_all(&threads).expect("temp dir");
        std::fs::write(threads.join("T-1.json"), thread).expect("write");

        unsafe {
            std::env::set_var("XDG_DATA_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_amp();
        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }

        assert_eq!(summary.sessions_count, 1);
        assert!(
            (summary.total_cost_usd - 0.42).abs() < 1e-9,
            "credits are cost"
        );
        assert_eq!(summary.input_tokens, 900);
        assert_eq!(summary.output_tokens, 70);
        assert_eq!(summary.cached_tokens, 530);
        assert!((summary.by_model["claude-sonnet-4-6"] - 0.42).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_daily_cost_history_buckets_amp_credits_by_day() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("amp-hist-{}", std::process::id()));
        let threads = dir.join("amp").join("threads");
        std::fs::create_dir_all(&threads).expect("temp dir");
        let now_ms = chrono::Local::now().timestamp_millis();
        std::fs::write(
            threads.join("T-2.json"),
            format!(
                r#"{{"id":"T-2","created":{now_ms},"usageLedger":{{"events":[{{"model":"m","credits":1.25,"tokens":{{"input":10,"output":5}}}}]}}}}"#
            ),
        )
        .expect("write");

        unsafe {
            std::env::set_var("XDG_DATA_HOME", &dir);
        }
        let (history, stopped) = get_daily_cost_history_with_budget("amp", 7, None, None);
        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }

        assert!(!stopped);
        let today_str = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        let today = history.iter().find(|(date, _)| date == &today_str);
        assert!(
            today.is_some(),
            "amp credits must land on today: {history:?}"
        );
        assert!((today.unwrap().1 - 1.25).abs() < 1e-9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── OpenCode-family local database ──────────────────────────────────────

    #[test]
    fn opencode_local_scan_filters_rows_by_provider_and_carries_tokens() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("opencode");
        std::fs::create_dir_all(&base).expect("temp dir");
        let db = base.join("opencode.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);",
        )
        .unwrap();
        let now_ms = chrono::Utc::now().timestamp_millis();
        for (id, provider, cost, tokens) in [
            ("go1", "opencode-go", 3.0, (100i64, 20i64, 5i64, 1i64)),
            ("zen1", "opencode", 2.5, (200, 30, 7, 2)),
        ] {
            let data = format!(
                r#"{{"providerID":"{provider}","role":"assistant","cost":{cost},"modelID":"m-{id}","tokens":{{"input":{in_t},"output":{out_t},"cache":{{"read":{cr},"write":{cw}}}}},"time":{{"created":{now_ms}}}}}"#,
                in_t = tokens.0,
                out_t = tokens.1,
                cr = tokens.2,
                cw = tokens.3,
            );
            conn.execute(
                "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, data, now_ms],
            )
            .unwrap();
        }
        drop(conn);

        unsafe {
            std::env::set_var("XDG_DATA_HOME", dir.path());
        }
        let go_summary = CostScanner::new(30).scan_opencodego();
        let zen_summary = CostScanner::new(30).scan_opencode();
        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }

        assert!(
            (go_summary.total_cost_usd - 3.0).abs() < 1e-9,
            "{go_summary:?}"
        );
        assert_eq!(go_summary.input_tokens, 100);
        assert!(
            (zen_summary.total_cost_usd - 2.5).abs() < 1e-9,
            "{zen_summary:?}"
        );
        assert_eq!(zen_summary.output_tokens, 30);
        assert_eq!(zen_summary.cached_tokens, 9);
        let _ = dir.close();
    }

    // ── Token-only sources priced from the models.dev catalog ───────────────

    #[test]
    fn gemini_models_price_from_the_catalog_and_unknown_models_stay_partial() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("gemini-price-{}", std::process::id()));
        let chats = dir.join("tmp").join("hash").join("chats");
        std::fs::create_dir_all(&chats).expect("temp dir");
        let now = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false);
        let doc = format!(
            concat!(
                r#"{{"sessionId":"s1","messages":["#,
                r#"{{"id":"m1","timestamp":"{now}","type":"gemini","model":"gemini-test","tokens":{{"promptTokenCount":1000,"candidatesTokenCount":500}}}},"#,
                r#"{{"id":"m2","timestamp":"{now}","type":"gemini","model":"gemini-mystery","tokens":{{"promptTokenCount":200,"candidatesTokenCount":50}}}}"#,
                r#"]}}"#
            ),
            now = now,
        );
        std::fs::write(chats.join("chat.json"), doc).expect("write");
        let pricing_root = dir.join("pricing");
        write_pricing_cache(
            &pricing_root,
            r#"{"google":{"id":"google","models":{"gemini-test":{"id":"gemini-test","cost":{"input":1.0,"output":2.0}}}}}"#,
        );

        unsafe {
            std::env::set_var("GEMINI_CLI_HOME", &dir);
        }
        let summary = CostScanner::new(30)
            .with_pricing_cache_root(pricing_root)
            .scan_gemini();
        unsafe {
            std::env::remove_var("GEMINI_CLI_HOME");
        }

        // gemini-test: 1000 input @ $1/M + 500 output @ $2/M = $0.002.
        assert!(
            (summary.total_cost_usd - 0.002).abs() < 1e-12,
            "{summary:?}"
        );
        assert!((summary.by_model["gemini-test"] - 0.002).abs() < 1e-12);
        assert!(
            summary
                .model_pricing_completeness
                .unpriced_models()
                .iter()
                .any(|model| model == "gemini-mystery"),
            "the unpriced model must be named: {:?}",
            summary.model_pricing_completeness
        );
        assert!(!summary.model_pricing_completeness.is_complete());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn kimi_code_models_price_from_the_catalog() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now_ms = chrono::Utc::now().timestamp_millis();
        let lines = format!(
            concat!(
                r#"{{"type":"llm.request","model":"kimi-code/kimi-for-coding","time":{t}}}"#,
                "\n",
                r#"{{"type":"usage.record","model":"__kimi_env_model__","usageScope":"turn","usage":{{"inputOther":300,"output":40,"inputCacheRead":12,"inputCacheCreation":3}},"time":{t}}}"#
            ),
            t = now_ms,
        );
        let dir = std::env::temp_dir().join(format!("kimi-price-{}", std::process::id()));
        let sessions = dir.join("sessions").join("ws").join("s");
        std::fs::create_dir_all(&sessions).expect("temp dir");
        std::fs::write(sessions.join("wire.jsonl"), lines).expect("write");
        let pricing_root = dir.join("pricing");
        write_pricing_cache(
            &pricing_root,
            r#"{"moonshotai":{"id":"moonshotai","models":{"kimi-for-coding":{"id":"kimi-for-coding","cost":{"input":4.0,"output":16.0,"cache_read":0.4}}}}}"#,
        );

        unsafe {
            std::env::set_var("KIMI_CODE_HOME", &dir);
        }
        let summary = CostScanner::new(30)
            .with_pricing_cache_root(pricing_root)
            .scan_kimi();
        unsafe {
            std::env::remove_var("KIMI_CODE_HOME");
        }

        // 300 input @ $4/M + 15 cached @ $0.4/M + 40 output @ $16/M.
        let expected = 300.0 * 4e-6 + 15.0 * 0.4e-6 + 40.0 * 16e-6;
        assert!(
            (summary.total_cost_usd - expected).abs() < 1e-12,
            "expected {expected}, got {summary:?}"
        );
        assert!(summary.model_pricing_completeness.is_complete());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn amp_events_without_credits_are_marked_unpriced_and_cache_rows_survive() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now_ms = chrono::Utc::now().timestamp_millis();
        let thread = format!(
            r#"{{"id":"T-3","created":{now_ms},"usageLedger":{{"events":[{{"model":"mystery","tokens":{{"cacheReadInputTokens":50,"cacheCreationInputTokens":10}}}}]}}}}"#
        );
        let dir = std::env::temp_dir().join(format!("amp-unpriced-{}", std::process::id()));
        let threads = dir.join("amp").join("threads");
        std::fs::create_dir_all(&threads).expect("temp dir");
        std::fs::write(threads.join("T-3.json"), thread).expect("write");

        unsafe {
            std::env::set_var("XDG_DATA_HOME", &dir);
        }
        let summary = CostScanner::new(30).scan_amp();
        unsafe {
            std::env::remove_var("XDG_DATA_HOME");
        }

        assert_eq!(summary.sessions_count, 1);
        assert_eq!(summary.cached_tokens, 60, "cache-only rows are kept");
        assert_eq!(summary.total_cost_usd, 0.0);
        assert!(
            !summary.model_pricing_completeness.is_complete(),
            "a token-bearing event without credits is unpriced, not free"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_daily_cost_history_prices_gemini_by_message_day() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("gemini-hist-{}", std::process::id()));
        let chats = dir.join("tmp").join("hash").join("chats");
        std::fs::create_dir_all(&chats).expect("temp dir");
        let now = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false);
        let doc = format!(
            r#"{{"sessionId":"s1","messages":[{{"id":"m1","timestamp":"{now}","type":"gemini","model":"gemini-test","tokens":{{"promptTokenCount":1000,"candidatesTokenCount":500}}}}]}}"#,
            now = now,
        );
        std::fs::write(chats.join("chat.json"), doc).expect("write");
        let pricing_root = dir.join("pricing");
        write_pricing_cache(
            &pricing_root,
            r#"{"google":{"id":"google","models":{"gemini-test":{"id":"gemini-test","cost":{"input":1.0,"output":2.0}}}}}"#,
        );

        unsafe {
            std::env::set_var("GEMINI_CLI_HOME", &dir);
        }
        let (history, stopped) = get_daily_cost_history_with_budget_and_pricing_root(
            "gemini",
            7,
            None,
            None,
            Some(&pricing_root),
        );
        unsafe {
            std::env::remove_var("GEMINI_CLI_HOME");
        }

        assert!(!stopped);
        let today_str = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        let today = history
            .iter()
            .find(|(date, _)| date == &today_str)
            .expect("today bucket");
        assert!((today.1 - 0.002).abs() < 1e-12, "{history:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_daily_cost_history_prices_copilot_shutdown_deltas_by_day() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("copilot-hist-{}", std::process::id()));
        let session = dir.join("session-state").join("sess-1");
        std::fs::create_dir_all(&session).expect("temp dir");
        let now = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, false);
        let events = format!(
            r#"{{"type":"session.shutdown","id":"e1","timestamp":"{now}","data":{{"modelMetrics":{{"gpt-5":{{"usage":{{"inputTokens":100,"outputTokens":20,"cacheReadTokens":0,"cacheWriteTokens":0}}}}}}}}}}"#,
            now = now,
        );
        std::fs::write(session.join("events.jsonl"), events).expect("write");

        unsafe {
            std::env::set_var("COPILOT_HOME", &dir);
        }
        let (history, stopped) = get_daily_cost_history_with_budget("copilot", 7, None, None);
        unsafe {
            std::env::remove_var("COPILOT_HOME");
        }

        assert!(!stopped);
        let today_str = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        // gpt-5: 100 input @ $1.25/M + 20 output @ $10/M.
        let expected = 100.0 * 1.25e-6 + 20.0 * 1e-5;
        let today = history
            .iter()
            .find(|(date, _)| date == &today_str)
            .expect("today bucket");
        assert!((today.1 - expected).abs() < 1e-12, "{history:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_daily_cost_history_prices_kimi_code_records_by_day() {
        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("kimi-hist-{}", std::process::id()));
        let sessions = dir.join("sessions").join("ws").join("s");
        std::fs::create_dir_all(&sessions).expect("temp dir");
        let now_ms = chrono::Local::now().timestamp_millis();
        let lines = format!(
            concat!(
                r#"{{"type":"llm.request","model":"kimi-code/kimi-for-coding","time":{t}}}"#,
                "\n",
                r#"{{"type":"usage.record","model":"__kimi_env_model__","usageScope":"turn","usage":{{"inputOther":100,"output":10}},"time":{t}}}"#
            ),
            t = now_ms,
        );
        std::fs::write(sessions.join("wire.jsonl"), lines).expect("write");
        let pricing_root = dir.join("pricing");
        write_pricing_cache(
            &pricing_root,
            r#"{"moonshotai":{"id":"moonshotai","models":{"kimi-for-coding":{"id":"kimi-for-coding","cost":{"input":4.0,"output":16.0}}}}}"#,
        );

        unsafe {
            std::env::set_var("KIMI_CODE_HOME", &dir);
        }
        let (history, stopped) = get_daily_cost_history_with_budget_and_pricing_root(
            "kimi",
            7,
            None,
            None,
            Some(&pricing_root),
        );
        unsafe {
            std::env::remove_var("KIMI_CODE_HOME");
        }

        assert!(!stopped);
        let today_str = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        let expected = 100.0 * 4e-6 + 10.0 * 16e-6;
        let today = history
            .iter()
            .find(|(date, _)| date == &today_str)
            .expect("today bucket");
        assert!((today.1 - expected).abs() < 1e-12, "{history:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Antigravity conversation databases ──────────────────────────────────

    fn write_antigravity_database(dir: &Path, turns: &[Vec<u8>], steps: &[Vec<u8>]) -> PathBuf {
        let base = dir.join("antigravity-cli").join("conversations");
        std::fs::create_dir_all(&base).expect("temp dir");
        let db = base.join("conv-1.db");
        crate::providers::antigravity::local_history::test_support::create_database(
            &db, turns, steps,
        );
        db
    }

    #[test]
    fn antigravity_scan_prices_known_models_and_marks_unknown_partial() {
        use crate::providers::antigravity::local_history::test_support::{step_blob, turn_blob};

        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("antigravity-scan-{}", std::process::id()));
        let now_ms = chrono::Utc::now().timestamp_millis();
        let turns = vec![
            turn_blob(
                "gemini-test",
                "step-1",
                "resp-1",
                "bot-1",
                1000,
                0,
                0,
                500,
                None,
            ),
            turn_blob(
                "gemini-mystery",
                "step-2",
                "resp-2",
                "bot-2",
                100,
                0,
                0,
                50,
                None,
            ),
        ];
        let steps = vec![
            step_blob("step-1", now_ms, "bot-1"),
            step_blob("step-2", now_ms, "bot-2"),
        ];
        let _db = write_antigravity_database(&dir, &turns, &steps);

        let pricing_root = dir.join("pricing");
        write_pricing_cache(
            &pricing_root,
            r#"{"google":{"id":"google","models":{"gemini-test":{"id":"gemini-test","cost":{"input":1.0,"output":2.0}}}}}"#,
        );

        unsafe {
            std::env::set_var("GEMINI_CLI_HOME", &dir);
        }
        let summary = CostScanner::new(30)
            .with_pricing_cache_root(pricing_root.clone())
            .scan_antigravity();
        unsafe {
            std::env::remove_var("GEMINI_CLI_HOME");
        }

        assert_eq!(summary.sessions_count, 2);
        assert_eq!(summary.input_tokens, 1100);
        assert_eq!(summary.output_tokens, 550);
        // gemini-test: 1000 input @ $1/M + 500 output @ $2/M = $0.002.
        assert!(
            (summary.total_cost_usd - 0.002).abs() < 1e-12,
            "{summary:?}"
        );
        assert!(summary.history_coverage_established);
        assert!(
            summary
                .model_pricing_completeness
                .unpriced_models()
                .iter()
                .any(|model| model == "gemini-mystery"),
            "the unpriced model must be named: {:?}",
            summary.model_pricing_completeness
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn get_daily_cost_history_prices_antigravity_generations_by_day() {
        use crate::providers::antigravity::local_history::test_support::{step_blob, turn_blob};

        let env_guard = ENV_SCAN_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let dir = std::env::temp_dir().join(format!("antigravity-hist-{}", std::process::id()));
        let now_ms = chrono::Local::now().timestamp_millis();
        let turns = vec![turn_blob(
            "gemini-test",
            "step-1",
            "resp-1",
            "bot-1",
            1000,
            0,
            0,
            500,
            None,
        )];
        let steps = vec![step_blob("step-1", now_ms, "bot-1")];
        let _db = write_antigravity_database(&dir, &turns, &steps);

        let pricing_root = dir.join("pricing");
        write_pricing_cache(
            &pricing_root,
            r#"{"google":{"id":"google","models":{"gemini-test":{"id":"gemini-test","cost":{"input":1.0,"output":2.0}}}}}"#,
        );

        unsafe {
            std::env::set_var("GEMINI_CLI_HOME", &dir);
        }
        let (history, stopped) = get_daily_cost_history_with_budget_and_pricing_root(
            "antigravity",
            7,
            None,
            None,
            Some(&pricing_root),
        );
        unsafe {
            std::env::remove_var("GEMINI_CLI_HOME");
        }

        assert!(!stopped);
        let today_str = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        let today = history
            .iter()
            .find(|(date, _)| date == &today_str)
            .expect("today bucket");
        assert!((today.1 - 0.002).abs() < 1e-12, "{history:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
