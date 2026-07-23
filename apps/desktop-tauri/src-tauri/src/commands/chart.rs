//! Provider chart data commands and DTOs.
//!
//! Cost history comes from the shared JSONL cost scanner and is available for
//! every provider. Credits history + usage breakdowns currently only apply to
//! the Codex / OpenAI dashboard cache and require an `account_email` to scope
//! reads to the right cached bundle.

use codexbar::core::OpenAIDashboardCacheStore;
use codexbar::cost_scanner::{CostScanner, CostSummary, get_daily_cost_history};
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
        let data = build_provider_chart_data_with_cancel(
            provider_id,
            account_email.clone(),
            Some(cancel.clone()),
        );
        if !cancel.load(Ordering::Relaxed) {
            cache_provider_chart_data(&data, account_email.as_deref());
        }
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
        for provider_id in ["codex", "claude"] {
            let data = build_provider_chart_data_with_cancel(provider_id.to_string(), None, None);
            cache_provider_chart_data(&data, None);
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
    tauri::async_runtime::spawn_blocking(move || load_provider_local_usage_summary(&provider_id))
        .await
        .unwrap_or_else(|err| {
            tracing::warn!("Provider local usage worker failed: {}", err);
            None
        })
}

#[cfg(test)]
pub(crate) fn build_provider_chart_data(
    provider_id: String,
    account_email: Option<String>,
) -> ProviderChartData {
    build_provider_chart_data_with_cancel(provider_id, account_email, None)
}

fn build_provider_chart_data_with_cancel(
    provider_id: String,
    account_email: Option<String>,
    cancel: Option<Arc<AtomicBool>>,
) -> ProviderChartData {
    // Keep one year available so the UI's 7 day / 30 day / quarter / year
    // switch changes the actual data window instead of only relabelling it.
    let raw_cost = get_daily_cost_history(&provider_id, 365);
    let cost_history: Vec<DailyCostPoint> = raw_cost
        .into_iter()
        .map(|(date, value)| DailyCostPoint { date, value })
        .collect();

    let (credits_history, usage_breakdown) =
        load_openai_dashboard_chart_data(&provider_id, account_email.as_deref());
    let local_usage = if cancel
        .as_deref()
        .is_some_and(|flag| flag.load(Ordering::Relaxed))
    {
        None
    } else {
        load_local_usage_summary_cached(&provider_id, cancel.as_deref())
    };

    ProviderChartData {
        provider_id,
        cost_history,
        credits_history,
        usage_breakdown,
        local_usage,
    }
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

fn load_local_usage_summary_cached(
    provider_id: &str,
    cancel: Option<&AtomicBool>,
) -> Option<ProviderLocalUsageSummary> {
    let cache = local_usage_cache();
    if let Ok(guard) = cache.lock()
        && let Some(entry) = guard.get(provider_id)
        && entry.loaded_at.elapsed() <= LOCAL_USAGE_TTL
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

    if let Ok(mut guard) = cache.lock() {
        guard.insert(
            provider_id.to_string(),
            CachedLocalUsage {
                loaded_at: Instant::now(),
                summary: summary.clone(),
            },
        );
    }
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
        ProviderChartData, cache_provider_chart_data, cached_provider_chart_data,
        clear_provider_local_usage_cache, localized_estimate_note,
    };
    use codexbar::settings::Language;

    #[test]
    fn japanese_estimate_note_is_localized() {
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
        clear_provider_local_usage_cache();
        let data = ProviderChartData::empty("cache-test".to_string());
        cache_provider_chart_data(&data, None);

        let cached = cached_provider_chart_data("cache-test", Some("user@example.com"))
            .expect("prewarmed provider data");
        assert_eq!(cached.provider_id, "cache-test");

        clear_provider_local_usage_cache();
        assert!(cached_provider_chart_data("cache-test", None).is_none());
    }
}
