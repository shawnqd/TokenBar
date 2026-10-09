//! Offline Antigravity (agy) conversation-history reader.
//!
//! Port of upstream CodexBar's `AntigravityLocalReader` /
//! `AntigravityProtoReader` (steipete/CodexBar, Antigravity provider):
//!
//! - Conversation SQLite databases live under
//!   `~/.gemini/antigravity-cli/conversations`, `~/.gemini/antigravity` and
//!   `~/.gemini/antigravity/conversations` (`GEMINI_CLI_HOME` relocates the
//!   `~/.gemini` base exactly like the CLI itself).
//! - Every `gen_metadata` row is a protobuf generation record carrying the
//!   token counts (`newInput`, `cacheRead`, `reasoning`, `output`), the model
//!   id, a response id and the step UUID. Current builds do not embed the
//!   generation timestamp, so it is recovered from the `steps` table metadata
//!   matched by step UUID occurrence order and bot identity.
//! - Nothing is inferred: rows that fail validation are skipped, unresolved
//!   generations are withheld, and both outcomes mark the scan partial.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rusqlite::Connection;

use crate::core::ProviderError;

/// Safety bounds for one scan pass. Real installs hold tens of databases with
/// a few thousand generations each; the caps only stop pathological trees.
const MAX_DATABASES: usize = 200;
const MAX_ROWS_PER_DATABASE: usize = 20_000;
const MAX_BLOB_BYTES: usize = 4 * 1024 * 1024;
const MAX_EVENTS: usize = 200_000;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_millis(250);

/// One completed generation with token usage, dated in Unix milliseconds.
#[derive(Debug, Clone, PartialEq)]
pub struct GenerationEvent {
    pub timestamp_ms: i64,
    /// Model id exactly as recorded (`gemini-3.8-flash`, `claude-opus-4-6-thinking`, ...).
    pub model: Option<String>,
    /// Fresh (non-cached) input tokens.
    pub new_input: u64,
    /// Cached input tokens read.
    pub cache_read: u64,
    /// Generated output tokens (reasoning excluded).
    pub output: u64,
    /// Thinking/reasoning tokens, billed as output.
    pub reasoning: u64,
    /// Stable provider response id, when the local record contains one.
    /// Used only for conservative duplicate suppression across databases.
    pub response_id: Option<String>,
}

impl GenerationEvent {
    /// Total billed output: generated tokens plus reasoning, the same sum the
    /// upstream reader prices.
    pub fn billed_output(&self) -> u64 {
        self.output.saturating_add(self.reasoning)
    }

    pub fn total_tokens(&self) -> u64 {
        self.new_input
            .saturating_add(self.cache_read)
            .saturating_add(self.billed_output())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanCoverage {
    /// Every discovered database was read and every generation found its date.
    Complete,
    /// Some database, row or timestamp could not be read; the events found are valid.
    Partial,
    /// No Antigravity local database exists at all.
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct HistoryScan {
    pub events: Vec<GenerationEvent>,
    pub coverage: ScanCoverage,
}

impl HistoryScan {
    fn unavailable() -> Self {
        Self {
            events: Vec::new(),
            coverage: ScanCoverage::Unavailable,
        }
    }
}

/// Base directories that may hold Antigravity conversation databases.
///
/// `GEMINI_CLI_HOME` relocates the whole Gemini home exactly like the CLI: it
/// replaces the default `~/.gemini` instead of adding to it.
fn gemini_home_roots() -> Vec<PathBuf> {
    let base = std::env::var("GEMINI_CLI_HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".gemini")));
    let Some(base) = base else {
        return Vec::new();
    };
    [
        "antigravity-cli/conversations",
        "antigravity",
        "antigravity/conversations",
    ]
    .iter()
    .map(|relative| base.join(relative))
    .collect()
}

/// Existing `*.db` conversation databases across the known roots, without the
/// summaries database (no `gen_metadata` there).
fn discover_candidate_databases() -> (Vec<PathBuf>, bool) {
    let mut out: Vec<PathBuf> = Vec::new();
    for root in gemini_home_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().is_none_or(|ext| ext != "db") {
                continue;
            }
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if name.starts_with("conversation_summaries") {
                continue;
            }
            out.push(path);
        }
    }
    out.sort();
    let truncated = out.len() > MAX_DATABASES;
    out.truncate(MAX_DATABASES);
    (out, truncated)
}

pub fn candidate_databases() -> Vec<PathBuf> {
    discover_candidate_databases().0
}

/// One parsed history serves every window (30d/7d/1d) and every repeated read
/// inside this window; a full parse walks tens of databases and can take
/// seconds under SQLite contention. The TTL matches the local-usage cache so a
/// chart read never rescans four times per cycle.
const SCAN_MEMO_TTL: Duration = Duration::from_secs(60);

struct ScanMemoEntry {
    /// The discovered database list the scan was built from, so a relocated
    /// root (or a test fixture) never receives another root's history.
    databases: Vec<PathBuf>,
    truncated: bool,
    built_at: Instant,
    scan: Arc<HistoryScan>,
}

fn scan_memo() -> &'static Mutex<Option<ScanMemoEntry>> {
    static MEMO: OnceLock<Mutex<Option<ScanMemoEntry>>> = OnceLock::new();
    MEMO.get_or_init(|| Mutex::new(None))
}

/// Scan every reachable Antigravity conversation database.
///
/// Results are memoized for [`SCAN_MEMO_TTL`]; a cancelled scan is never
/// memoized. The memo is keyed by the discovered database list, so tests and
/// relocated installs stay isolated.
pub fn scan(cancel: Option<&AtomicBool>) -> HistoryScan {
    let (databases, truncated) = discover_candidate_databases();
    if databases.is_empty() {
        return HistoryScan::unavailable();
    }

    if let Ok(guard) = scan_memo().lock()
        && let Some(entry) = guard.as_ref()
        && entry.databases == databases
        && entry.truncated == truncated
        && entry.built_at.elapsed() < SCAN_MEMO_TTL
    {
        return (*entry.scan).clone();
    }

    let scan = scan_databases(&databases, cancel, truncated);

    if !is_cancelled(cancel)
        && let Ok(mut guard) = scan_memo().lock()
    {
        *guard = Some(ScanMemoEntry {
            databases,
            truncated,
            built_at: Instant::now(),
            scan: Arc::new(scan.clone()),
        });
    }

    scan
}

fn scan_databases(
    databases: &[PathBuf],
    cancel: Option<&AtomicBool>,
    truncated: bool,
) -> HistoryScan {
    let mut events = Vec::new();
    let mut coverage = if truncated {
        ScanCoverage::Partial
    } else {
        ScanCoverage::Complete
    };
    for database in databases {
        if is_cancelled(cancel) {
            coverage = ScanCoverage::Partial;
            break;
        }
        match read_database(database, cancel) {
            Ok(result) => {
                if !result.complete {
                    coverage = ScanCoverage::Partial;
                }
                events.extend(result.events);
            }
            Err(_) => coverage = ScanCoverage::Partial,
        }
        if events.len() > MAX_EVENTS {
            coverage = ScanCoverage::Partial;
            break;
        }
    }

    // A response id is stable across copied conversation databases. Keep the
    // first identical payload, drop exact replays, and withhold conflicts
    // while marking coverage partial rather than double-counting either copy.
    let mut seen_responses: HashMap<String, GenerationEvent> = HashMap::new();
    let mut deduped = Vec::with_capacity(events.len());
    for event in events {
        let Some(response_id) = event.response_id.as_deref() else {
            deduped.push(event);
            continue;
        };
        if let Some(previous) = seen_responses.get(response_id) {
            if previous != &event {
                coverage = ScanCoverage::Partial;
            }
            continue;
        }
        seen_responses.insert(response_id.to_string(), event.clone());
        deduped.push(event);
    }

    HistoryScan {
        events: deduped,
        coverage,
    }
}

fn is_cancelled(cancel: Option<&AtomicBool>) -> bool {
    cancel.is_some_and(|flag| flag.load(Ordering::Relaxed))
}

struct DatabaseResult {
    events: Vec<GenerationEvent>,
    complete: bool,
}

/// Read one conversation database. A database without `gen_metadata` is a
/// foreign file in a shared root, not an error: it contributes nothing and
/// does not degrade coverage (mirrors the upstream schema inspection).
fn read_database(
    path: &Path,
    cancel: Option<&AtomicBool>,
) -> Result<DatabaseResult, ProviderError> {
    let conn = crate::core::open_readonly_sqlite_connection(path, SQLITE_BUSY_TIMEOUT).map_err(
        |error| ProviderError::Other(format!("SQLite error reading Antigravity history: {error}")),
    )?;
    read_database_from_connection(&conn, cancel)
}

#[derive(Debug, Clone, Default)]
struct UsageDraft {
    new_input: u64,
    cache_read: u64,
    output: u64,
    reasoning: u64,
    response_id: Option<String>,
    bot_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct TurnDraft {
    usage: Option<UsageDraft>,
    embedded_timestamp_ms: Option<i64>,
    model: Option<String>,
    step_uuid: Option<String>,
}

#[derive(Debug, Clone)]
struct Occurrence {
    row: i64,
    timestamp_ms: Option<i64>,
    bot_id: Option<String>,
}

#[derive(Debug, Clone)]
struct StepOccurrence {
    row: i64,
    timestamp_ms: Option<i64>,
    bot_id: Option<String>,
}

struct Pending {
    row: i64,
    step_uuid: String,
    bot_id: Option<String>,
    turn: TurnDraft,
}

fn read_database_from_connection(
    conn: &Connection,
    cancel: Option<&AtomicBool>,
) -> Result<DatabaseResult, ProviderError> {
    if !has_table(conn, "gen_metadata") {
        return Ok(DatabaseResult {
            events: Vec::new(),
            complete: true,
        });
    }

    let mut complete = true;
    let mut parsed: Vec<(i64, TurnDraft)> = Vec::new();
    let mut occurrences: HashMap<String, Vec<Occurrence>> = HashMap::new();
    let mut bot_id_uses: HashMap<String, u32> = HashMap::new();

    {
        let mut statement = conn
            .prepare("SELECT idx, data FROM gen_metadata LIMIT ?1")
            .map_err(sqlite_error)?;
        let mut rows = statement
            .query([(MAX_ROWS_PER_DATABASE + 1) as i64])
            .map_err(sqlite_error)?;
        let mut seen = 0usize;
        while let Some(row) = rows.next().map_err(sqlite_error)? {
            if is_cancelled(cancel) {
                complete = false;
                break;
            }
            seen += 1;
            if seen > MAX_ROWS_PER_DATABASE {
                complete = false;
                break;
            }
            let idx = row.get::<_, Option<i64>>(0).ok().flatten();
            let data = row.get::<_, Option<Vec<u8>>>(1).ok().flatten();
            let (Some(idx), Some(data)) = (idx, data) else {
                complete = false;
                continue;
            };
            if idx < 0 || data.len() > MAX_BLOB_BYTES {
                complete = false;
                continue;
            }
            let Some(turn) = parse_turn(&data) else {
                // Rows that are not generation records are invalid evidence.
                complete = false;
                continue;
            };
            if let Some(bot) = turn
                .usage
                .as_ref()
                .and_then(|usage| usage.bot_id.as_deref())
            {
                *bot_id_uses.entry(bot.to_string()).or_insert(0) += 1;
            }
            if let Some(uuid) = turn.step_uuid.as_ref() {
                occurrences
                    .entry(uuid.clone())
                    .or_default()
                    .push(Occurrence {
                        row: idx,
                        timestamp_ms: turn.embedded_timestamp_ms,
                        bot_id: turn.usage.as_ref().and_then(|usage| usage.bot_id.clone()),
                    });
            }
            parsed.push((idx, turn));
        }
    }

    let mut raw_events: Vec<(i64, TurnDraft, i64)> = Vec::new();
    let mut pending: Vec<Pending> = Vec::new();
    for (row, turn) in &parsed {
        let Some(usage) = turn.usage.as_ref() else {
            continue;
        };
        if turn.embedded_timestamp_ms.is_none() && turn.step_uuid.is_some() {
            pending.push(Pending {
                row: *row,
                step_uuid: turn.step_uuid.clone().unwrap_or_default(),
                bot_id: usage.bot_id.clone(),
                turn: turn.clone(),
            });
            continue;
        }
        if let Some(timestamp_ms) = turn.embedded_timestamp_ms {
            raw_events.push((*row, turn.clone(), timestamp_ms));
        }
    }

    if !pending.is_empty() && has_table(conn, "steps") {
        let step_scan = read_step_timestamps(conn, &occurrences, cancel)?;
        if !step_scan.complete {
            complete = false;
        }
        let resolved = resolve_step_timestamps(
            &step_scan.timestamps,
            &occurrences,
            &step_scan.ambiguous_bot_ids,
            step_scan.saw_unidentified_rows,
        );
        if !append_recovered_events(
            &mut raw_events,
            &pending,
            &occurrences,
            &resolved,
            &step_scan.exact_by_bot,
            &step_scan.ambiguous_bot_ids,
            &bot_id_uses,
        ) {
            complete = false;
        }
    } else if !pending.is_empty() {
        complete = false;
    }

    raw_events.sort_by_key(|(row, _, _)| *row);

    // Copied rows replayed under multiple filenames share a response id; keep
    // the first, drop identical replays, and treat conflicting payloads as
    // contradicted evidence instead of publishing either version.
    let mut seen_responses: HashMap<String, GenerationEvent> = HashMap::new();
    let mut events: Vec<GenerationEvent> = Vec::new();
    for (_, turn, timestamp_ms) in &raw_events {
        let Some(usage) = turn.usage.as_ref() else {
            continue;
        };
        let Some(event) = event_from(turn, *timestamp_ms) else {
            continue;
        };
        if let Some(response) = usage.response_id.as_deref() {
            if let Some(prior) = seen_responses.get(response) {
                if prior != &event {
                    complete = false;
                }
                continue;
            }
            seen_responses.insert(response.to_string(), event.clone());
        }
        events.push(event);
    }

    Ok(DatabaseResult { events, complete })
}

fn event_from(turn: &TurnDraft, timestamp_ms: i64) -> Option<GenerationEvent> {
    let usage = turn.usage.as_ref()?;
    Some(GenerationEvent {
        timestamp_ms,
        model: turn.model.clone(),
        new_input: usage.new_input,
        cache_read: usage.cache_read,
        output: usage.output,
        reasoning: usage.reasoning,
        response_id: usage.response_id.clone(),
    })
}

struct StepTimestampScan {
    timestamps: HashMap<String, Vec<StepOccurrence>>,
    exact_by_bot: HashMap<String, (String, i64)>,
    ambiguous_bot_ids: HashSet<String>,
    saw_unidentified_rows: bool,
    complete: bool,
}

fn read_step_timestamps(
    conn: &Connection,
    needed: &HashMap<String, Vec<Occurrence>>,
    cancel: Option<&AtomicBool>,
) -> Result<StepTimestampScan, ProviderError> {
    let mut timestamps: HashMap<String, Vec<StepOccurrence>> = HashMap::new();
    let mut exact_by_bot: HashMap<String, (String, i64)> = HashMap::new();
    let mut ambiguous_bot_ids: HashSet<String> = HashSet::new();
    let mut saw_unidentified_rows = false;
    let mut complete = true;

    let mut statement = conn
        .prepare("SELECT idx, metadata FROM steps")
        .map_err(sqlite_error)?;
    let mut rows = statement.query([]).map_err(sqlite_error)?;
    while let Some(row) = rows.next().map_err(sqlite_error)? {
        if is_cancelled(cancel) {
            complete = false;
            break;
        }
        let idx = row.get::<_, Option<i64>>(0).ok().flatten();
        let metadata = row.get::<_, Option<Vec<u8>>>(1).ok().flatten();
        let (Some(idx), Some(metadata)) = (idx, metadata) else {
            complete = false;
            continue;
        };
        if idx < 0 || metadata.len() > MAX_BLOB_BYTES {
            complete = false;
            continue;
        }
        let Some(parsed) = parse_step_metadata(&metadata) else {
            complete = false;
            continue;
        };
        if let Some(bot_id) = parsed.bot_id.as_deref() {
            record_exact_bot(
                bot_id,
                parsed.step_uuid.as_deref(),
                parsed.timestamp_ms,
                &mut exact_by_bot,
                &mut ambiguous_bot_ids,
            );
        }
        let Some(step_uuid) = parsed.step_uuid else {
            saw_unidentified_rows = true;
            continue;
        };
        if needed.contains_key(&step_uuid) {
            timestamps
                .entry(step_uuid)
                .or_default()
                .push(StepOccurrence {
                    row: idx,
                    timestamp_ms: parsed.timestamp_ms,
                    bot_id: parsed.bot_id,
                });
        }
    }

    Ok(StepTimestampScan {
        timestamps,
        exact_by_bot,
        ambiguous_bot_ids,
        saw_unidentified_rows,
        complete,
    })
}

fn record_exact_bot(
    bot_id: &str,
    step_uuid: Option<&str>,
    timestamp_ms: Option<i64>,
    exact: &mut HashMap<String, (String, i64)>,
    ambiguous: &mut HashSet<String>,
) {
    if ambiguous.contains(bot_id) {
        return;
    }
    let (Some(step_uuid), Some(timestamp_ms)) = (step_uuid, timestamp_ms) else {
        exact.remove(bot_id);
        ambiguous.insert(bot_id.to_string());
        return;
    };
    match exact.get(bot_id) {
        Some((existing_uuid, existing_ms))
            if existing_ms != &timestamp_ms || existing_uuid != step_uuid =>
        {
            exact.remove(bot_id);
            ambiguous.insert(bot_id.to_string());
        }
        Some(_) => {}
        None => {
            exact.insert(bot_id.to_string(), (step_uuid.to_string(), timestamp_ms));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_step_timestamps(
    step_timestamps: &HashMap<String, Vec<StepOccurrence>>,
    needed: &HashMap<String, Vec<Occurrence>>,
    ambiguous_bot_ids: &HashSet<String>,
    unidentified_rows_present: bool,
) -> HashMap<String, Vec<i64>> {
    let mut resolved: HashMap<String, Vec<i64>> = HashMap::new();
    for (step_uuid, timestamps) in step_timestamps {
        let Some(occurrences) = needed.get(step_uuid) else {
            continue;
        };
        let mut sorted_occurrences: Vec<&Occurrence> = occurrences.iter().collect();
        sorted_occurrences.sort_by_key(|occurrence| occurrence.row);
        let unique_rows: HashSet<i64> = sorted_occurrences.iter().map(|item| item.row).collect();
        if unique_rows.len() != sorted_occurrences.len() {
            continue;
        }
        let needed_count = sorted_occurrences.len();

        let mut sorted: Vec<&StepOccurrence> = timestamps.iter().collect();
        sorted.sort_by_key(|occurrence| occurrence.row);
        if sorted.windows(2).any(|pair| pair[0].row == pair[1].row) {
            continue;
        }
        let ordered: Vec<Option<i64>> = sorted
            .iter()
            .map(|step| {
                if step
                    .bot_id
                    .as_ref()
                    .is_some_and(|bot| ambiguous_bot_ids.contains(bot))
                {
                    None
                } else {
                    step.timestamp_ms
                }
            })
            .collect();

        let selected: Vec<i64> = if ordered.len() == 1
            && (needed_count == 1 || !unidentified_rows_present)
            && ordered[0].is_some()
        {
            vec![ordered[0].expect("checked"); needed_count]
        } else {
            if ordered.len() < needed_count {
                continue;
            }
            let candidates = &ordered[..needed_count];
            if candidates.iter().any(Option::is_none) {
                continue;
            }
            candidates
                .iter()
                .map(|value| value.expect("checked"))
                .collect()
        };

        if !sorted_occurrences
            .iter()
            .zip(selected.iter())
            .all(|(occurrence, timestamp)| {
                occurrence.timestamp_ms.is_none() || occurrence.timestamp_ms == Some(*timestamp)
            })
        {
            continue;
        }
        resolved.insert(step_uuid.clone(), selected);
    }
    resolved
}

fn append_recovered_events(
    raw_events: &mut Vec<(i64, TurnDraft, i64)>,
    pending: &[Pending],
    occurrences: &HashMap<String, Vec<Occurrence>>,
    resolved: &HashMap<String, Vec<i64>>,
    exact_by_bot: &HashMap<String, (String, i64)>,
    ambiguous_bot_ids: &HashSet<String>,
    bot_id_uses: &HashMap<String, u32>,
) -> bool {
    let mut offsets: HashMap<&str, HashMap<i64, usize>> = HashMap::new();
    for (step_uuid, items) in occurrences {
        let mut rows: Vec<i64> = items.iter().map(|item| item.row).collect();
        rows.sort_unstable();
        if rows.windows(2).any(|pair| pair[0] == pair[1]) {
            continue;
        }
        offsets.insert(
            step_uuid.as_str(),
            rows.into_iter()
                .enumerate()
                .map(|(index, row)| (row, index))
                .collect(),
        );
    }

    let mut sorted_pending: Vec<&Pending> = pending.iter().collect();
    sorted_pending.sort_by_key(|item| item.row);
    let mut recovered = 0usize;
    for item in sorted_pending {
        if let Some(bot_id) = item.bot_id.as_deref() {
            if ambiguous_bot_ids.contains(bot_id) {
                continue;
            }
            if let Some((exact_uuid, _)) = exact_by_bot.get(bot_id) {
                if exact_uuid != &item.step_uuid {
                    continue;
                }
            }
            if bot_id_uses.get(bot_id).copied() == Some(1)
                && let Some((_, timestamp_ms)) = exact_by_bot.get(bot_id)
            {
                raw_events.push((item.row, item.turn.clone(), *timestamp_ms));
                recovered += 1;
                continue;
            }
        }
        let Some(timestamps) = resolved.get(&item.step_uuid) else {
            continue;
        };
        let Some(offset) = offsets
            .get(item.step_uuid.as_str())
            .and_then(|map| map.get(&item.row))
            .copied()
        else {
            continue;
        };
        let Some(timestamp_ms) = timestamps.get(offset).copied() else {
            continue;
        };
        raw_events.push((item.row, item.turn.clone(), timestamp_ms));
        recovered += 1;
    }
    recovered == pending.len()
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

fn sqlite_error(error: rusqlite::Error) -> ProviderError {
    ProviderError::Other(format!("SQLite error reading Antigravity history: {error}"))
}

enum FieldBody<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
    Fixed,
}

struct ProtoReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    malformed: bool,
}

impl<'a> ProtoReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            offset: 0,
            malformed: false,
        }
    }

    fn varint(&mut self) -> Option<u64> {
        let mut result: u64 = 0;
        for index in 0..10 {
            let byte = *self.bytes.get(self.offset)?;
            self.offset += 1;
            if index == 9 && byte > 1 {
                self.malformed = true;
                return None;
            }
            result |= u64::from(byte & 0x7f) << (index * 7);
            if byte & 0x80 == 0 {
                return Some(result);
            }
        }
        self.malformed = true;
        None
    }

    fn next_field(&mut self) -> Option<(u32, FieldBody<'a>)> {
        if self.offset >= self.bytes.len() {
            return None;
        }
        let tag = self.varint()?;
        let number = (tag >> 3) as u32;
        if number == 0 {
            self.malformed = true;
            return None;
        }
        let wire = (tag & 7) as u8;
        match wire {
            0 => Some((number, FieldBody::Varint(self.varint()?))),
            1 => {
                self.skip(8)?;
                Some((number, FieldBody::Fixed))
            }
            2 => {
                let length = usize::try_from(self.varint()?).ok()?;
                if length > self.bytes.len() - self.offset {
                    self.malformed = true;
                    return None;
                }
                let end = self.offset + length;
                let data = &self.bytes[self.offset..end];
                self.offset = end;
                Some((number, FieldBody::Bytes(data)))
            }
            5 => {
                self.skip(4)?;
                Some((number, FieldBody::Fixed))
            }
            _ => {
                self.malformed = true;
                None
            }
        }
    }

    fn skip(&mut self, count: usize) -> Option<()> {
        if self.bytes.len() - self.offset < count {
            self.malformed = true;
            return None;
        }
        self.offset += count;
        Some(())
    }
}

fn field_counter(body: FieldBody<'_>) -> Option<u64> {
    match body {
        FieldBody::Varint(value) => Some(value),
        _ => None,
    }
}

fn field_text(body: FieldBody<'_>) -> Option<String> {
    match body {
        FieldBody::Bytes(bytes) => std::str::from_utf8(bytes).ok().and_then(|text| {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }),
        _ => None,
    }
}

fn parse_timestamp(bytes: &[u8]) -> Option<i64> {
    let mut seconds: Option<u64> = None;
    let mut nanos: u64 = 0;
    let mut reader = ProtoReader::new(bytes);
    while let Some((number, body)) = reader.next_field() {
        match number {
            1 => {
                let value = field_counter(body)?;
                if value == 0 || value > 253_402_300_799 {
                    return None;
                }
                seconds = Some(value);
            }
            2 => {
                let value = field_counter(body)?;
                if value > 999_999_999 {
                    return None;
                }
                nanos = value;
            }
            _ => {}
        }
    }
    if reader.malformed {
        return None;
    }
    let seconds = seconds?;
    Some((seconds as i64) * 1000 + (nanos / 1_000_000) as i64)
}

fn parse_usage(bytes: &[u8]) -> Option<UsageDraft> {
    let mut usage = UsageDraft::default();
    let mut reader = ProtoReader::new(bytes);
    while let Some((number, body)) = reader.next_field() {
        match number {
            2 => usage.new_input = field_counter(body)?,
            5 => usage.cache_read = field_counter(body)?,
            9 => usage.reasoning = field_counter(body)?,
            10 => usage.output = field_counter(body)?,
            11 => usage.response_id = field_text(body),
            7 => usage.bot_id = field_text(body),
            _ => {}
        }
    }
    if reader.malformed {
        return None;
    }
    Some(usage)
}

fn parse_generation_timestamp(bytes: &[u8]) -> Option<i64> {
    let mut reader = ProtoReader::new(bytes);
    let mut timestamp = None;
    while let Some((number, body)) = reader.next_field() {
        if number != 4 {
            continue;
        }
        let FieldBody::Bytes(inner) = body else {
            return None;
        };
        timestamp = parse_timestamp(inner);
    }
    if reader.malformed {
        return None;
    }
    timestamp
}

fn parse_chat(bytes: &[u8], turn: &mut TurnDraft) -> Option<()> {
    let mut reader = ProtoReader::new(bytes);
    while let Some((number, body)) = reader.next_field() {
        match number {
            4 => {
                turn.usage = Some(parse_usage(match body {
                    FieldBody::Bytes(inner) => inner,
                    _ => return None,
                })?)
            }
            9 => {
                if let FieldBody::Bytes(inner) = body {
                    turn.embedded_timestamp_ms = parse_generation_timestamp(inner);
                }
            }
            19 => turn.model = field_text(body),
            _ => {}
        }
    }
    if reader.malformed { None } else { Some(()) }
}

/// Decode one `gen_metadata` generation record.
fn parse_turn(bytes: &[u8]) -> Option<TurnDraft> {
    let mut turn = TurnDraft::default();
    let mut found_chat = false;
    let mut reader = ProtoReader::new(bytes);
    while let Some((number, body)) = reader.next_field() {
        match number {
            1 => {
                found_chat = true;
                let FieldBody::Bytes(chat) = body else {
                    return None;
                };
                parse_chat(chat, &mut turn)?;
            }
            4 => turn.step_uuid = field_text(body),
            _ => {}
        }
    }
    if reader.malformed || !found_chat {
        return None;
    }
    Some(turn)
}

#[derive(Debug, Default)]
struct StepMetadata {
    timestamp_ms: Option<i64>,
    step_uuid: Option<String>,
    bot_id: Option<String>,
}

/// Decode one `steps.metadata` record.
fn parse_step_metadata(bytes: &[u8]) -> Option<StepMetadata> {
    let mut meta = StepMetadata::default();
    let mut reader = ProtoReader::new(bytes);
    while let Some((number, body)) = reader.next_field() {
        match number {
            1 => {
                if let FieldBody::Bytes(inner) = body {
                    meta.timestamp_ms = parse_timestamp(inner);
                }
            }
            9 => {
                if let FieldBody::Bytes(inner) = body
                    && let Some(bot_id) = find_bot_id(inner)
                {
                    meta.bot_id = Some(bot_id);
                }
            }
            12 => meta.step_uuid = field_text(body),
            _ => {}
        }
    }
    if reader.malformed {
        return None;
    }
    Some(meta)
}

fn find_bot_id(bytes: &[u8]) -> Option<String> {
    let mut reader = ProtoReader::new(bytes);
    while let Some((number, body)) = reader.next_field() {
        if number == 7 {
            return field_text(body);
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    fn varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                return out;
            }
        }
    }

    fn tag(field: u32, wire: u8) -> Vec<u8> {
        varint(((field as u64) << 3) | u64::from(wire))
    }

    fn varint_field(field: u32, value: u64) -> Vec<u8> {
        let mut out = tag(field, 0);
        out.extend(varint(value));
        out
    }

    fn bytes_field(field: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = tag(field, 2);
        out.extend(varint(payload.len() as u64));
        out.extend_from_slice(payload);
        out
    }

    fn timestamp_message(timestamp_ms: i64) -> Vec<u8> {
        let seconds = (timestamp_ms / 1000) as u64;
        let nanos = ((timestamp_ms % 1000).max(0) * 1_000_000) as u64;
        let mut out = varint_field(1, seconds);
        if nanos > 0 {
            out.extend(varint_field(2, nanos));
        }
        out
    }

    /// Encode one synthetic `gen_metadata` generation record.
    #[allow(clippy::too_many_arguments)]
    pub fn turn_blob(
        model: &str,
        step_uuid: &str,
        response_id: &str,
        bot_id: &str,
        new_input: u64,
        cache_read: u64,
        reasoning: u64,
        output: u64,
        embedded_timestamp_ms: Option<i64>,
    ) -> Vec<u8> {
        let mut usage = varint_field(2, new_input);
        usage.extend(varint_field(5, cache_read));
        usage.extend(varint_field(9, reasoning));
        usage.extend(varint_field(10, output));
        usage.extend(bytes_field(11, response_id.as_bytes()));
        usage.extend(bytes_field(7, bot_id.as_bytes()));

        let mut chat = bytes_field(4, &usage);
        if let Some(timestamp_ms) = embedded_timestamp_ms {
            chat.extend(bytes_field(
                9,
                &bytes_field(4, &timestamp_message(timestamp_ms)),
            ));
        }
        chat.extend(bytes_field(19, model.as_bytes()));

        let mut blob = bytes_field(1, &chat);
        blob.extend(bytes_field(4, step_uuid.as_bytes()));
        blob
    }

    /// Encode one synthetic `steps.metadata` record.
    pub fn step_blob(step_uuid: &str, timestamp_ms: i64, bot_id: &str) -> Vec<u8> {
        let mut blob = bytes_field(1, &timestamp_message(timestamp_ms));
        blob.extend(bytes_field(9, &bytes_field(7, bot_id.as_bytes())));
        blob.extend(bytes_field(12, step_uuid.as_bytes()));
        blob
    }

    /// Create a conversation database with the stored columns the reader queries.
    pub fn create_database(path: &Path, turns: &[Vec<u8>], steps: &[Vec<u8>]) {
        let conn = Connection::open(path).expect("open fixture db");
        conn.execute_batch(
            "CREATE TABLE gen_metadata (idx INTEGER PRIMARY KEY, data BLOB, size INTEGER);
             CREATE TABLE steps (idx INTEGER PRIMARY KEY, metadata BLOB);",
        )
        .expect("create fixture schema");
        for (index, blob) in turns.iter().enumerate() {
            conn.execute(
                "INSERT INTO gen_metadata (idx, data, size) VALUES (?1, ?2, ?3)",
                rusqlite::params![index as i64, blob, blob.len() as i64],
            )
            .expect("insert turn");
        }
        for (index, blob) in steps.iter().enumerate() {
            conn.execute(
                "INSERT INTO steps (idx, metadata) VALUES (?1, ?2)",
                rusqlite::params![index as i64, blob],
            )
            .expect("insert step");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{create_database, step_blob, turn_blob};
    use super::*;

    #[test]
    fn parse_turn_decodes_usage_model_and_step() {
        let blob = turn_blob(
            "gemini-3.8-flash",
            "step-a",
            "resp-a",
            "bot-a",
            100,
            20,
            7,
            30,
            Some(1_760_000_000_000),
        );
        let turn = parse_turn(&blob).expect("turn");
        let usage = turn.usage.expect("usage");
        assert_eq!(usage.new_input, 100);
        assert_eq!(usage.cache_read, 20);
        assert_eq!(usage.reasoning, 7);
        assert_eq!(usage.output, 30);
        assert_eq!(usage.response_id.as_deref(), Some("resp-a"));
        assert_eq!(usage.bot_id.as_deref(), Some("bot-a"));
        assert_eq!(turn.model.as_deref(), Some("gemini-3.8-flash"));
        assert_eq!(turn.step_uuid.as_deref(), Some("step-a"));
        assert_eq!(turn.embedded_timestamp_ms, Some(1_760_000_000_000));
    }

    #[test]
    fn malformed_or_foreign_blobs_are_rejected() {
        assert!(parse_turn(&[]).is_none());
        assert!(parse_turn(&[0xff, 0xff, 0xff, 0xff]).is_none());
        // A well-formed blob without a chat field is not a generation record
        // (field 4 wire 2: tag 0x22, length 4, "step").
        assert!(parse_turn(&[0x22, 0x04, b's', b't', b'e', b'p']).is_none());
    }

    #[test]
    fn read_database_recovers_timestamps_positionally_and_dedups_responses() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = dir.path().join("conv.db");
        let first = turn_blob(
            "m",
            "step-1",
            "resp-1",
            "bot-q",
            10,
            0,
            0,
            5,
            Some(1_000_000),
        );
        let second = turn_blob("m", "step-1", "resp-2", "bot-q", 20, 0, 0, 6, None);
        // Replayed copy: same response id, payload and date under another step.
        let replayed = turn_blob(
            "m",
            "step-2",
            "resp-1",
            "bot-q",
            10,
            0,
            0,
            5,
            Some(1_000_000),
        );
        let steps = vec![
            step_blob("step-1", 1_000_000, "bot-x"),
            step_blob("step-1", 2_000_000, "bot-y"),
            step_blob("step-2", 9_999_999, "bot-z"),
        ];
        create_database(&db, &[first, second, replayed], &steps);

        let result = read_database(&db, None).expect("scan");
        assert!(result.complete);
        assert_eq!(result.events.len(), 2, "{:?}", result.events);
        assert_eq!(result.events[0].timestamp_ms, 1_000_000);
        assert_eq!(result.events[1].timestamp_ms, 2_000_000);
        assert!(
            result
                .events
                .iter()
                .all(|event| event.model.as_deref() == Some("m"))
        );
    }

    #[test]
    fn scan_deduplicates_the_same_response_across_databases() {
        let dir = tempfile::tempdir().expect("temp dir");
        let first_db = dir.path().join("first.db");
        let second_db = dir.path().join("second.db");
        let turn = turn_blob(
            "m",
            "step-1",
            "same-response",
            "bot-a",
            10,
            0,
            0,
            5,
            Some(1_000_000),
        );
        let step = step_blob("step-1", 1_000_000, "bot-a");
        create_database(
            &first_db,
            std::slice::from_ref(&turn),
            std::slice::from_ref(&step),
        );
        create_database(&second_db, &[turn], &[step]);

        let result = scan_databases(&[first_db, second_db], None, false);
        assert_eq!(result.coverage, ScanCoverage::Complete);
        assert_eq!(result.events.len(), 1, "cross-db replay must count once");
        assert_eq!(
            result.events[0].response_id.as_deref(),
            Some("same-response")
        );
    }

    #[test]
    fn database_cap_marks_history_partial() {
        let result = scan_databases(&[], None, true);
        assert_eq!(result.coverage, ScanCoverage::Partial);
    }

    #[test]
    fn foreign_database_without_gen_metadata_is_ignored() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db = dir.path().join("foreign.db");
        let conn = Connection::open(&db).expect("open");
        conn.execute_batch("CREATE TABLE other (id INTEGER PRIMARY KEY);")
            .expect("schema");
        drop(conn);
        let result = read_database(&db, None).expect("scan");
        assert!(result.complete);
        assert!(result.events.is_empty());
    }
}
