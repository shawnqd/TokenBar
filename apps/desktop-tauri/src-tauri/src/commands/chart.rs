//! Provider chart data commands and DTOs.
//!
//! Cost history comes from the shared JSONL cost scanner and is available for
//! every provider. Credits history + usage breakdowns currently only apply to
//! the Codex / OpenAI dashboard cache and require an `account_email` to scope
//! reads to the right cached bundle.

use codexbar::core::OpenAIDashboardCacheStore;
use codexbar::cost_scanner::{CostScanner, CostSummary, get_daily_cost_history_with_budget};
use codexbar::locale::{self, LocaleKey};
use codexbar::settings::Settings;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

const LOCAL_USAGE_TTL: Duration = Duration::from_secs(30);
const PROVIDER_CHART_TTL: Duration = Duration::from_secs(5 * 60);
/// Interactive chart reads must not wait on an unbounded local transcript
/// walk. The background prewarm path remains uncapped, while an on-demand
/// chart returns a bounded/partial bundle and lets the next TTL pass retry.
const INTERACTIVE_CHART_SCAN_BUDGET: Duration = Duration::from_secs(3);
// v3: ProviderLocalUsageSummary gained per-period top-model fields; discard v2
// caches so the new fields are recomputed instead of loading as null.
const PROVIDER_CHART_CACHE_VERSION: u8 = 3;

/// A single (date, value) point for cost or credits history charts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCostPoint {
    pub date: String,
    pub value: f64,
}

/// A single service's usage within a day for the stacked usage breakdown chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceUsagePoint {
    pub service: String,
    pub credits_used: f64,
}

/// One day's stacked usage breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyUsageBreakdown {
    pub day: String,
    pub services: Vec<ServiceUsagePoint>,
    pub total_credits_used: f64,
}

/// Real local usage summary from Codex / Claude log files.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLocalUsageSummary {
    pub today_cost: Option<f64>,
    pub today_tokens: Option<u64>,
    pub seven_day_cost: Option<f64>,
    pub seven_day_tokens: Option<u64>,
    pub thirty_day_cost: Option<f64>,
    pub thirty_day_tokens: Option<u64>,
    /// Top model per window, so the panel's "top model" line matches whichever
    /// period the user selected instead of always reflecting 30-day totals.
    pub today_top_model: Option<String>,
    pub seven_day_top_model: Option<String>,
    pub thirty_day_top_model: Option<String>,
    pub estimate_note: String,
}

/// Full chart data bundle for one provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderChartData {
    pub provider_id: String,
    pub cost_history: Vec<DailyCostPoint>,
    pub credits_history: Vec<DailyCostPoint>,
    pub usage_breakdown: Vec<DailyUsageBreakdown>,
    pub local_usage: Option<ProviderLocalUsageSummary>,
}

#[tauri::command]
pub async fn get_provider_chart_data(
    provider_id: String,
    account_email: Option<String>,
) -> ProviderChartData {
    if let Some(cached) = cached_provider_chart_data(&provider_id, account_email.as_deref()) {
        return cached;
    }

    let fallback_provider_id = provider_id.clone();
    let cancel = register_chart_scan(&provider_id);
    tauri::async_runtime::spawn_blocking(move || {
        let (data, scan_stopped) = build_provider_chart_data_with_cancel(
            provider_id,
            account_email.clone(),
            Some(cancel.clone()),
        );
        cache_provider_chart_data_if_complete(
            &data,
            account_email.as_deref(),
            scan_stopped,
            cancel.load(Ordering::Relaxed),
        );
        data
    })
    .await
    .unwrap_or_else(|err| {
        tracing::warn!("Provider chart data worker failed: {}", err);
        ProviderChartData::empty(fallback_provider_id)
    })
}

/// Start the expensive Codex / Claude log aggregation after the shell is up,
/// so opening the tray normally hits a warm cache instead of beginning a scan.
pub(crate) fn prewarm_provider_chart_data() {
    tauri::async_runtime::spawn_blocking(move || {
        for provider_id in ["codex", "claude", "grok"] {
            let (data, scan_stopped) =
                build_provider_chart_data_with_cancel(provider_id.to_string(), None, None);
            cache_provider_chart_data_if_complete(&data, None, scan_stopped, false);
        }
    });
}

/// Restore aggregate-only data from the previous run. This keeps the first
/// tray open fast even before the background refresh has finished.
pub(crate) fn restore_provider_chart_cache() {
    let Some(path) = persisted_provider_chart_cache_path() else {
        return;
    };
    let Ok(bytes) = fs::read(path) else {
        return;
    };
    let Ok(persisted) = serde_json::from_slice::<PersistedProviderChartCache>(&bytes) else {
        return;
    };
    if persisted.version != PROVIDER_CHART_CACHE_VERSION {
        return;
    }

    for data in persisted.entries.into_values() {
        cache_provider_chart_data_in_memory(&data, None);
    }
}

#[tauri::command]
pub async fn get_provider_local_usage_summary(
    provider_id: String,
) -> Option<ProviderLocalUsageSummary> {
    let failure_provider_id = provider_id.clone();
    tauri::async_runtime::spawn_blocking(move || load_provider_local_usage_summary(&provider_id))
        .await
        .unwrap_or_else(|err| {
            tracing::warn!("Provider local usage worker failed: {}", err);
            record_local_usage_fetch_failure(&failure_provider_id, CostFetchFailure::Failed);
            None
        })
}

#[cfg(test)]
pub(crate) fn build_provider_chart_data_without_local_io(
    provider_id: String,
    account_email: Option<String>,
) -> ProviderChartData {
    // Account-scoping tests must not walk the user's live transcript tree.
    // Production requests use `build_provider_chart_data_with_cancel`, while
    // this fixture deliberately exercises only the dashboard-cache portion.
    let (credits_history, usage_breakdown) =
        load_openai_dashboard_chart_data(&provider_id, account_email.as_deref());
    ProviderChartData {
        provider_id,
        cost_history: Vec::new(),
        credits_history,
        usage_breakdown,
        local_usage: None,
    }
}

fn build_provider_chart_data_with_cancel(
    provider_id: String,
    account_email: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) -> (ProviderChartData, bool) {
    // Keep one year available so the UI's 7 day / 30 day / quarter / year
    // switch changes the actual data window instead of only relabelling it.
    let deadline = cancel
        .as_ref()
        .map(|_| Instant::now() + INTERACTIVE_CHART_SCAN_BUDGET);
    let (raw_cost, cost_scan_stopped) =
        get_daily_cost_history_with_budget(&provider_id, 365, cancel.as_deref(), deadline);
    let cost_history: Vec<DailyCostPoint> = raw_cost
        .into_iter()
        .map(|(date, value)| DailyCostPoint { date, value })
        .collect();

    let (credits_history, usage_breakdown) =
        load_openai_dashboard_chart_data(&provider_id, account_email.as_deref());
    // A bounded read is preferable to freezing the Settings/Tray detail
    // surface. Keep the result honest: do not present a partial history as
    // complete. A timeout only says that this chart request stopped early; it
    // is not evidence that the provider has no local usage. In particular, do
    // not write a fresh `None` into the shared local-usage cache here: the
    // dedicated summary command and the background enrichment pass must still
    // be able to complete the authoritative scan and publish the real value.
    let local_usage = if cost_scan_stopped
        || cancel
            .as_deref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        None
    } else {
        load_local_usage_summary_cached(&provider_id, cancel.as_deref())
    };

    (
        ProviderChartData {
            provider_id,
            cost_history,
            credits_history,
            usage_breakdown,
            local_usage,
        },
        cost_scan_stopped,
    )
}

impl ProviderChartData {
    fn empty(provider_id: String) -> Self {
        Self {
            provider_id,
            cost_history: Vec::new(),
            credits_history: Vec::new(),
            usage_breakdown: Vec::new(),
            local_usage: None,
        }
    }
}

struct CachedProviderChartData {
    loaded_at: Instant,
    data: ProviderChartData,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedProviderChartCache {
    version: u8,
    entries: HashMap<String, ProviderChartData>,
}

fn provider_chart_cache() -> &'static Mutex<HashMap<String, CachedProviderChartData>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedProviderChartData>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

#[cfg(not(test))]
fn provider_chart_persistence_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn provider_chart_cache_key(provider_id: &str, account_email: Option<&str>) -> String {
    format!(
        "{}:{}",
        provider_id.to_ascii_lowercase(),
        account_email
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase(),
    )
}

fn cached_provider_chart_data(
    provider_id: &str,
    account_email: Option<&str>,
) -> Option<ProviderChartData> {
    let exact_key = provider_chart_cache_key(provider_id, account_email);
    let base_key = provider_chart_cache_key(provider_id, None);
    let mut data = {
        let guard = provider_chart_cache().lock().ok()?;
        let entry = guard
            .get(&exact_key)
            .or_else(|| account_email.and_then(|_| guard.get(&base_key)))?;
        if entry.loaded_at.elapsed() > PROVIDER_CHART_TTL {
            return None;
        }
        entry.data.clone()
    };

    // The expensive local-log part is account-independent. A startup prewarm
    // therefore uses the provider-only key; when the UI later supplies an
    // account, only overlay the cheap account-scoped dashboard cache data.
    if exact_key != base_key {
        let (credits_history, usage_breakdown) =
            load_openai_dashboard_chart_data(provider_id, account_email);
        data.credits_history = credits_history;
        data.usage_breakdown = usage_breakdown;
        cache_provider_chart_data(&data, account_email);
    }
    Some(data)
}

fn cache_provider_chart_data(data: &ProviderChartData, account_email: Option<&str>) {
    cache_provider_chart_data_in_memory(data, account_email);
    #[cfg(not(test))]
    if account_email.is_none() {
        persist_provider_chart_data(data);
    }
}

fn cache_provider_chart_data_if_complete(
    data: &ProviderChartData,
    account_email: Option<&str>,
    scan_stopped: bool,
    cancelled: bool,
) {
    // A deadline-truncated chart is a useful best-effort response for the
    // current caller, but it must not replace a previously valid bundle in
    // either the process cache or the persisted snapshot. Otherwise the next
    // five-minute read would treat a timeout's zero-filled history as truth.
    if scan_stopped || cancelled {
        return;
    }
    cache_provider_chart_data(data, account_email);
}

fn cache_provider_chart_data_in_memory(data: &ProviderChartData, account_email: Option<&str>) {
    let key = provider_chart_cache_key(&data.provider_id, account_email);
    if let Ok(mut guard) = provider_chart_cache().lock() {
        guard.insert(
            key,
            CachedProviderChartData {
                loaded_at: Instant::now(),
                data: data.clone(),
            },
        );
    }
}

fn persisted_provider_chart_cache_path() -> Option<PathBuf> {
    Settings::settings_path()?
        .parent()
        .map(|parent| parent.join("provider-chart-cache.json"))
}

#[cfg(not(test))]
fn persist_provider_chart_data(data: &ProviderChartData) {
    let Ok(_write_guard) = provider_chart_persistence_lock().lock() else {
        return;
    };
    let Some(path) = persisted_provider_chart_cache_path() else {
        return;
    };
    let mut persisted = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PersistedProviderChartCache>(&bytes).ok())
        .filter(|cache| cache.version == PROVIDER_CHART_CACHE_VERSION)
        .unwrap_or_else(|| PersistedProviderChartCache {
            version: PROVIDER_CHART_CACHE_VERSION,
            entries: HashMap::new(),
        });
    persisted
        .entries
        .insert(data.provider_id.to_ascii_lowercase(), data.clone());

    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(&persisted) else {
        return;
    };
    let temp_path = path.with_extension("json.tmp");
    if fs::write(&temp_path, bytes).is_err() {
        return;
    }
    // Windows does not replace an existing destination with rename. Removing
    // the old complete snapshot first still avoids exposing a half-written
    // JSON file; at worst startup sees no cache and rebuilds it.
    let _ = fs::remove_file(&path);
    if fs::rename(&temp_path, &path).is_err() {
        let _ = fs::remove_file(temp_path);
    }
}

fn active_chart_scans() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static ACTIVE: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    ACTIVE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_chart_scan(provider_id: &str) -> Arc<AtomicBool> {
    let next = Arc::new(AtomicBool::new(false));
    if let Ok(mut active) = active_chart_scans().lock()
        && let Some(previous) = active.insert(provider_id.to_string(), next.clone())
    {
        previous.store(true, Ordering::Relaxed);
    }
    next
}

fn load_local_usage_summary(
    provider_id: &str,
    cancel: Option<&AtomicBool>,
) -> Option<ProviderLocalUsageSummary> {
    let thirty_day = scan_local_cost(provider_id, 30, cancel)?;
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }
    let seven_day = scan_local_cost(provider_id, 7, cancel).unwrap_or_default();
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }
    let today = scan_local_cost(provider_id, 1, cancel).unwrap_or_default();

    let thirty_day_tokens = total_tokens(&thirty_day);
    let seven_day_tokens = total_tokens(&seven_day);
    let today_tokens = total_tokens(&today);
    let has_usage =
        thirty_day.sessions_count > 0 || thirty_day.total_cost_usd > 0.0 || thirty_day_tokens > 0;
    if !has_usage {
        return None;
    }

    let lang = locale::current_language();
    Some(ProviderLocalUsageSummary {
        today_cost: non_zero_f64(today.total_cost_usd),
        today_tokens: non_zero_u64(today_tokens),
        seven_day_cost: non_zero_f64(seven_day.total_cost_usd),
        seven_day_tokens: non_zero_u64(seven_day_tokens),
        thirty_day_cost: non_zero_f64(thirty_day.total_cost_usd),
        thirty_day_tokens: non_zero_u64(thirty_day_tokens),
        today_top_model: top_model(&today),
        seven_day_top_model: top_model(&seven_day),
        thirty_day_top_model: top_model(&thirty_day),
        estimate_note: localized_estimate_note(provider_id, lang),
    })
}

pub(crate) fn load_provider_local_usage_summary(
    provider_id: &str,
) -> Option<ProviderLocalUsageSummary> {
    load_local_usage_summary_cached(provider_id, None)
}

struct CachedLocalUsage {
    loaded_at: Instant,
    summary: Option<ProviderLocalUsageSummary>,
}

fn local_usage_cache() -> &'static Mutex<HashMap<String, CachedLocalUsage>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CachedLocalUsage>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn clear_provider_local_usage_cache() {
    if let Ok(mut guard) = local_usage_cache().lock() {
        guard.clear();
    }
    if let Ok(mut guard) = provider_chart_cache().lock() {
        guard.clear();
    }
    #[cfg(not(test))]
    if let Some(path) = persisted_provider_chart_cache_path() {
        let _ = fs::remove_file(path);
    }
}

#[tauri::command]
pub fn clear_provider_local_usage_cache_command() {
    clear_provider_local_usage_cache();
}

/// Why a local-usage enrichment pass could not produce data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum CostFetchFailure {
    /// The scan failed outright (no readable logs, worker error). The next
    /// read may retry immediately.
    Failed,
    /// An explicit background caller may use this to hold a degraded state for
    /// the TTL. Interactive chart timeouts must not record this marker in the
    /// shared local-usage cache because a bounded chart read is not evidence of
    /// missing usage.
    TimedOut,
}

pub(crate) fn cost_fetch_failure_allows_early_retry(failure: CostFetchFailure) -> bool {
    !matches!(failure, CostFetchFailure::TimedOut)
}

pub(crate) fn token_cost_cache_is_fresh(
    loaded_at: Option<Instant>,
    now: Instant,
    ttl: Duration,
) -> bool {
    loaded_at
        .and_then(|loaded| now.checked_duration_since(loaded))
        .map(|age| age <= ttl)
        .unwrap_or(false)
}

/// Record an enrichment failure as a degraded cache entry: the provider keeps
/// its core quota snapshot untouched, but local-usage reads see no data and
/// the timestamp tells the next read whether it may retry immediately.
fn record_local_usage_fetch_failure(provider_id: &str, failure: CostFetchFailure) {
    let loaded_at = if cost_fetch_failure_allows_early_retry(failure) {
        Instant::now() - LOCAL_USAGE_TTL - Duration::from_secs(1)
    } else {
        Instant::now()
    };
    store_local_usage_summary_at(provider_id, None, loaded_at);
}

fn store_local_usage_summary(provider_id: &str, summary: Option<ProviderLocalUsageSummary>) {
    store_local_usage_summary_at(provider_id, summary, Instant::now());
}

fn store_local_usage_summary_at(
    provider_id: &str,
    summary: Option<ProviderLocalUsageSummary>,
    loaded_at: Instant,
) {
    if let Ok(mut guard) = local_usage_cache().lock() {
        guard.insert(
            provider_id.to_string(),
            CachedLocalUsage { loaded_at, summary },
        );
    }
}

/// Read the local-usage cache without triggering a scan. Used by tests to
/// observe the degraded marker; production reads go through
/// `load_local_usage_summary` / `load_local_usage_summary_cached`.
#[cfg(test)]
pub(crate) fn cached_provider_local_usage_summary(
    provider_id: &str,
) -> Option<ProviderLocalUsageSummary> {
    let Ok(guard) = local_usage_cache().lock() else {
        return None;
    };
    guard
        .get(provider_id)
        .and_then(|entry| entry.summary.clone())
}

/// Apply one enrichment scan result: a successful summary is cached, a
/// missing summary marks the provider degraded so the next pass can retry
/// without leaving stale data visible.
fn apply_local_usage_scan(provider_id: String, summary: Option<ProviderLocalUsageSummary>) {
    match summary {
        Some(summary) => store_local_usage_summary(&provider_id, Some(summary)),
        None => record_local_usage_fetch_failure(&provider_id, CostFetchFailure::Failed),
    }
}

/// Background enrichment stage (UP-M-001): refresh the local-usage cache for
/// the given providers after the core quota refresh has already published its
/// results. Runs on the blocking pool so a slow scan never delays the core
/// refresh command, the tray update or the UI snapshot events; a failure only
/// marks the enrichment cache degraded and leaves the core results intact.
pub(crate) async fn refresh_provider_local_usage_cache(provider_ids: Vec<String>) {
    if provider_ids.is_empty() {
        return;
    }

    let failure_provider_ids = provider_ids.clone();
    let scans = match tauri::async_runtime::spawn_blocking(move || {
        provider_ids
            .into_iter()
            .map(|provider_id| {
                let summary = load_local_usage_summary(&provider_id, None);
                (provider_id, summary)
            })
            .collect::<Vec<_>>()
    })
    .await
    {
        Ok(scans) => scans,
        Err(err) => {
            tracing::warn!("Provider local usage refresh worker failed: {err}");
            for provider_id in failure_provider_ids {
                record_local_usage_fetch_failure(&provider_id, CostFetchFailure::Failed);
            }
            return;
        }
    };

    for (provider_id, summary) in scans {
        apply_local_usage_scan(provider_id, summary);
    }
}

fn load_local_usage_summary_cached(
    provider_id: &str,
    cancel: Option<&AtomicBool>,
) -> Option<ProviderLocalUsageSummary> {
    let cache = local_usage_cache();
    if let Ok(guard) = cache.lock()
        && let Some(entry) = guard.get(provider_id)
        && token_cost_cache_is_fresh(Some(entry.loaded_at), Instant::now(), LOCAL_USAGE_TTL)
    {
        return entry.summary.clone();
    }

    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }

    let summary = load_local_usage_summary(provider_id, cancel);
    if cancel.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return None;
    }

    store_local_usage_summary(provider_id, summary.clone());
    summary
}

fn localized_estimate_note(provider_id: &str, lang: codexbar::settings::Language) -> String {
    match provider_id {
        "claude" => locale::get_text(lang, LocaleKey::PanelEstimatedFromLocalLogsClaude),
        _ => locale::get_text(lang, LocaleKey::PanelEstimatedFromLocalLogs),
    }
}

fn scan_local_cost(
    provider_id: &str,
    days: u32,
    cancel: Option<&AtomicBool>,
) -> Option<CostSummary> {
    let scanner = CostScanner::new(days);
    match provider_id {
        "codex" => Some(scanner.scan_codex_with_cancel(cancel)),
        "claude" => Some(scanner.scan_claude_with_cancel(cancel)),
        "grok" => Some(scanner.scan_grok_with_cancel(cancel)),
        // Everything else has no local log this app knows how to read, so the
        // recent-usage block stays absent rather than showing a zero it did not
        // measure.
        _ => None,
    }
}

fn total_tokens(summary: &CostSummary) -> u64 {
    summary.input_tokens + summary.output_tokens
}

fn non_zero_f64(value: f64) -> Option<f64> {
    (value > 0.0).then_some(value)
}

fn non_zero_u64(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

fn top_model(summary: &CostSummary) -> Option<String> {
    summary
        .by_model_tokens
        .iter()
        .max_by_key(|(_, counts)| counts.total())
        .map(|(model, _)| model.clone())
        .or_else(|| {
            summary
                .by_model
                .iter()
                .max_by(|a, b| a.1.total_cmp(b.1))
                .map(|(model, _)| model.clone())
        })
}

fn load_openai_dashboard_chart_data(
    provider_id: &str,
    account_email: Option<&str>,
) -> (Vec<DailyCostPoint>, Vec<DailyUsageBreakdown>) {
    if provider_id != "codex" && provider_id != "openai" {
        return (Vec::new(), Vec::new());
    }

    let Some(account_email) = account_email else {
        return (Vec::new(), Vec::new());
    };

    let Some(cache) = OpenAIDashboardCacheStore::load() else {
        return (Vec::new(), Vec::new());
    };

    if !cache.account_email.eq_ignore_ascii_case(account_email) {
        return (Vec::new(), Vec::new());
    }

    let snapshot = &cache.snapshot;

    let breakdown_source = if !snapshot.daily_breakdown.is_empty() {
        &snapshot.daily_breakdown
    } else if !snapshot.usage_breakdown.is_empty() {
        &snapshot.usage_breakdown
    } else {
        return (Vec::new(), Vec::new());
    };

    let credits_history: Vec<DailyCostPoint> = breakdown_source
        .iter()
        .map(|d| DailyCostPoint {
            date: d.day.clone(),
            value: d.total_credits_used,
        })
        .collect();

    let usage_breakdown: Vec<DailyUsageBreakdown> = snapshot
        .usage_breakdown
        .iter()
        .map(|d| DailyUsageBreakdown {
            day: d.day.clone(),
            services: d
                .services
                .iter()
                .map(|s| ServiceUsagePoint {
                    service: s.service.clone(),
                    credits_used: s.credits_used,
                })
                .collect(),
            total_credits_used: d.total_credits_used,
        })
        .collect();

    (credits_history, usage_breakdown)
}

#[cfg(test)]
mod tests {
    use super::{
        CostFetchFailure, DailyCostPoint, ProviderChartData, ProviderLocalUsageSummary,
        apply_local_usage_scan, cache_provider_chart_data, cache_provider_chart_data_if_complete,
        cached_provider_chart_data, cached_provider_local_usage_summary,
        clear_provider_local_usage_cache, cost_fetch_failure_allows_early_retry,
        load_local_usage_summary, local_usage_cache, localized_estimate_note,
        record_local_usage_fetch_failure, refresh_provider_local_usage_cache,
        token_cost_cache_is_fresh,
    };
    use codexbar::settings::Language;
    use std::sync::{Arc, Mutex, OnceLock, atomic::AtomicBool};
    use std::time::{Duration, Instant};

    // These tests intentionally exercise process-wide caches. Serialize the
    // module so parallel Rust test execution cannot clear another case's
    // fixture halfway through its assertion (which otherwise poisons the
    // mutex and produces misleading failures).
    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    #[test]
    fn japanese_estimate_note_is_localized() {
        let _test_guard = test_guard();
        assert_eq!(
            localized_estimate_note("codex", Language::Japanese),
            "ローカルログから推定したもので、請求書と異なる場合があります"
        );
        assert_eq!(
            localized_estimate_note("claude", Language::Japanese),
            "ClaudeのローカルログからAPIレートで推定したもので、トークン総数が請求書と異なる場合があります"
        );
    }

    #[test]
    fn english_estimate_note_is_localized() {
        let _test_guard = test_guard();
        assert_eq!(
            localized_estimate_note("codex", Language::English),
            "Estimated from local logs; may differ from your bill"
        );
        assert_eq!(
            localized_estimate_note("claude", Language::English),
            "Estimated from local Claude logs at API rates; token totals may differ from your bill"
        );
    }

    #[test]
    fn provider_chart_cache_reuses_prewarmed_data_for_account_view() {
        let _test_guard = test_guard();
        clear_provider_local_usage_cache();
        let data = ProviderChartData::empty("cache-test".to_string());
        cache_provider_chart_data(&data, None);

        let cached = cached_provider_chart_data("cache-test", Some("user@example.com"))
            .expect("prewarmed provider data");
        assert_eq!(cached.provider_id, "cache-test");

        clear_provider_local_usage_cache();
        assert!(cached_provider_chart_data("cache-test", None).is_none());
    }

    // ── UP-M-001 enrichment stage ─────────────────────────────────────

    #[test]
    fn hard_failure_marks_enrichment_degraded_and_allows_immediate_retry() {
        let _test_guard = test_guard();
        clear_provider_local_usage_cache();
        record_local_usage_fetch_failure("codex", CostFetchFailure::Failed);

        let cache = local_usage_cache();
        let guard = cache.lock().unwrap();
        let entry = guard.get("codex").expect("degraded entry recorded");
        assert!(entry.summary.is_none());
        assert!(!token_cost_cache_is_fresh(
            Some(entry.loaded_at),
            Instant::now(),
            super::LOCAL_USAGE_TTL
        ));
        drop(guard);
        assert!(cost_fetch_failure_allows_early_retry(
            CostFetchFailure::Failed
        ));
    }

    #[test]
    fn timed_out_enrichment_holds_degraded_state_until_ttl() {
        let _test_guard = test_guard();
        clear_provider_local_usage_cache();
        record_local_usage_fetch_failure("codex", CostFetchFailure::TimedOut);

        let cache = local_usage_cache();
        let guard = cache.lock().unwrap();
        let entry = guard.get("codex").expect("degraded entry recorded");
        assert!(entry.summary.is_none());
        assert!(token_cost_cache_is_fresh(
            Some(entry.loaded_at),
            Instant::now(),
            super::LOCAL_USAGE_TTL
        ));
        drop(guard);
        assert!(!cost_fetch_failure_allows_early_retry(
            CostFetchFailure::TimedOut
        ));
    }

    #[test]
    fn timed_out_chart_read_does_not_poison_successful_local_usage() {
        let _test_guard = test_guard();
        clear_provider_local_usage_cache();
        let summary = ProviderLocalUsageSummary {
            today_cost: Some(0.1),
            today_tokens: Some(10),
            seven_day_cost: Some(0.7),
            seven_day_tokens: Some(70),
            thirty_day_cost: Some(2.0),
            thirty_day_tokens: Some(200),
            today_top_model: Some("gpt-5".to_string()),
            seven_day_top_model: Some("gpt-5".to_string()),
            thirty_day_top_model: Some("gpt-5".to_string()),
            estimate_note: "estimated".to_string(),
        };
        apply_local_usage_scan("codex".to_string(), Some(summary));

        // An already-cancelled interactive chart is the deterministic stand-in
        // for the three-second budget expiring. It may return no local usage in
        // this bundle, but it must leave the authoritative cache untouched.
        let cancel = Arc::new(AtomicBool::new(true));
        let (chart, scan_stopped) =
            super::build_provider_chart_data_with_cancel("codex".to_string(), None, Some(cancel));
        assert!(chart.local_usage.is_none());
        assert!(scan_stopped);
        assert_eq!(
            cached_provider_local_usage_summary("codex")
                .expect("successful summary survives chart timeout")
                .thirty_day_tokens,
            Some(200)
        );
    }

    #[test]
    fn timed_out_chart_bundle_does_not_replace_persisted_chart_cache() {
        let _test_guard = test_guard();
        let provider_id = "chart-timeout-cache-test";
        let mut complete = ProviderChartData::empty(provider_id.to_string());
        complete.cost_history.push(DailyCostPoint {
            date: "2026-09-01".to_string(),
            value: 3.5,
        });
        cache_provider_chart_data(&complete, None);

        let partial = ProviderChartData::empty(provider_id.to_string());
        cache_provider_chart_data_if_complete(&partial, None, true, false);

        let cached = cached_provider_chart_data(provider_id, None)
            .expect("complete chart remains cached after timeout");
        assert_eq!(cached.cost_history.len(), 1);
        assert_eq!(cached.cost_history[0].value, 3.5);
    }

    #[test]
    fn enrichment_failure_marks_only_the_scanned_provider() {
        let _test_guard = test_guard();
        clear_provider_local_usage_cache();
        record_local_usage_fetch_failure("codex", CostFetchFailure::Failed);

        // The failure only writes the enrichment cache for the scanned
        // provider; nothing touches the quota path or other providers, so the
        // core cards keep their last published snapshot.
        let cache = local_usage_cache();
        assert!(cache.lock().unwrap().get("claude").is_none());
        assert!(cached_provider_local_usage_summary("claude").is_none());
    }

    #[test]
    fn enrichment_pass_stores_success_and_marks_failed_scan_degraded() {
        let _test_guard = test_guard();
        clear_provider_local_usage_cache();
        let summary = ProviderLocalUsageSummary {
            today_cost: Some(1.0),
            today_tokens: Some(10),
            seven_day_cost: None,
            seven_day_tokens: None,
            thirty_day_cost: Some(2.0),
            thirty_day_tokens: Some(200),
            today_top_model: None,
            seven_day_top_model: None,
            thirty_day_top_model: Some("gpt-5".to_string()),
            estimate_note: "estimated".to_string(),
        };
        apply_local_usage_scan("codex".to_string(), Some(summary));
        assert!(cached_provider_local_usage_summary("codex").is_some());

        apply_local_usage_scan("claude".to_string(), None);
        let cache = local_usage_cache();
        let guard = cache.lock().unwrap();
        let entry = guard.get("claude").expect("degraded entry recorded");
        assert!(entry.summary.is_none());
        assert!(!token_cost_cache_is_fresh(
            Some(entry.loaded_at),
            Instant::now(),
            super::LOCAL_USAGE_TTL
        ));
        drop(guard);
    }

    #[test]
    fn enrichment_pass_with_unknown_provider_records_degradation_without_scanning() {
        let _test_guard = test_guard();
        clear_provider_local_usage_cache();
        tauri::async_runtime::block_on(refresh_provider_local_usage_cache(vec![
            "not-a-local-provider".to_string(),
        ]));

        let cache = local_usage_cache();
        let guard = cache.lock().unwrap();
        let entry = guard
            .get("not-a-local-provider")
            .expect("degraded entry recorded");
        assert!(entry.summary.is_none());
        assert!(!token_cost_cache_is_fresh(
            Some(entry.loaded_at),
            Instant::now(),
            super::LOCAL_USAGE_TTL
        ));
        drop(guard);
        // The on-demand path still returns nothing for the degraded provider
        // instead of blocking on a re-scan.
        assert!(load_local_usage_summary("not-a-local-provider", None).is_none());
    }

    #[test]
    fn enrichment_ttl_is_independent_of_provider_quota_cache() {
        let _test_guard = test_guard();
        // A degraded enrichment entry expiring does not make the core quota
        // cache stale: the core snapshot keeps publishing on its own clock.
        let now = Instant::now();
        let enrichment_loaded = now - Duration::from_secs(31);
        let provider_updated = now;
        assert!(!token_cost_cache_is_fresh(
            Some(enrichment_loaded),
            now,
            super::LOCAL_USAGE_TTL
        ));
        assert!(crate::commands::is_provider_cache_fresh(
            Some(provider_updated),
            Duration::from_secs(30)
        ));
    }
}
