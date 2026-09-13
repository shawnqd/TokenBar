use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;

use codexbar::settings::Settings;
use tauri::Manager;

use crate::state::AppState;

const AUTO_REFRESH_POLL_INTERVAL: Duration = Duration::from_secs(15);

/// Serializes background enrichment passes (UP-M-001). Only one local-usage
/// scan runs at a time; a refresh that completes while a scan is still in
/// flight simply skips scheduling so the core refresh command is never blocked.
static ENRICHMENT_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

pub fn install(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            if should_refresh(&app) {
                let _ = crate::commands::do_refresh_providers_if_stale(&app).await;
            }
            tokio::time::sleep(AUTO_REFRESH_POLL_INTERVAL).await;
        }
    });
}

fn should_refresh(app: &tauri::AppHandle) -> bool {
    let settings = Settings::load();
    let Some(interval) = refresh_interval(settings.refresh_interval_secs) else {
        return false;
    };

    let state = app.state::<Mutex<AppState>>();
    state
        .lock()
        .map(|guard| should_refresh_from_state(&guard, interval))
        .unwrap_or(false)
}

fn refresh_interval(seconds: u64) -> Option<Duration> {
    (seconds > 0).then(|| Duration::from_secs(seconds))
}

/// Providers eligible for the optional enrichment stage (UP-M-001): enabled
/// providers whose local cost logs the app knows how to scan. Core quota
/// snapshots are never part of this stage.
pub(crate) fn enrichment_provider_ids(settings: &Settings) -> Vec<String> {
    settings
        .get_enabled_provider_ids()
        .into_iter()
        .map(|provider| provider.cli_name().to_string())
        .filter(|provider_id| matches!(provider_id.as_str(), "codex" | "claude"))
        .collect()
}

/// Schedule the optional enrichment stage after the core provider refresh has
/// published its results (UP-M-001 / upstream Win-CodexBar main
/// `schedule_refresh_enrichment`). Runs in the background: a slow or failed
/// enrichment keeps the already-published core snapshots and only marks the
/// enrichment cache degraded, so the core cards never go blank waiting for it.
pub(crate) fn schedule_refresh_enrichment(settings: &Settings) {
    let provider_ids = enrichment_provider_ids(settings);
    if provider_ids.is_empty() {
        return;
    }
    let Ok(guard) =
        Arc::clone(ENRICHMENT_LOCK.get_or_init(|| Arc::new(tokio::sync::Mutex::new(()))))
            .try_lock_owned()
    else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let _guard = guard;
        crate::commands::refresh_provider_local_usage_cache(provider_ids).await;
    });
}

fn should_refresh_from_state(state: &AppState, interval: Duration) -> bool {
    if state.is_refreshing {
        return false;
    }
    match state.provider_cache_updated_at {
        Some(updated_at) => updated_at.elapsed() >= interval,
        None => true,
    }
}

#[cfg(test)]
pub(crate) fn should_refresh_from_values(
    is_refreshing: bool,
    updated_at: Option<Instant>,
    interval_secs: u64,
) -> bool {
    let Some(interval) = refresh_interval(interval_secs) else {
        return false;
    };
    if is_refreshing {
        return false;
    }
    updated_at
        .map(|updated| updated.elapsed() >= interval)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_refresh_setting_disables_background_refresh() {
        assert!(!should_refresh_from_values(false, None, 0));
    }

    #[test]
    fn missing_cache_triggers_background_refresh() {
        assert!(should_refresh_from_values(false, None, 300));
    }

    #[test]
    fn fresh_cache_does_not_refresh_before_interval() {
        assert!(!should_refresh_from_values(
            false,
            Some(Instant::now() - Duration::from_secs(299)),
            300,
        ));
    }

    #[test]
    fn stale_cache_refreshes_after_configured_interval() {
        assert!(should_refresh_from_values(
            false,
            Some(Instant::now() - Duration::from_secs(300)),
            300,
        ));
    }

    #[test]
    fn active_refresh_blocks_overlapping_background_refresh() {
        assert!(!should_refresh_from_values(true, None, 300));
    }

    #[test]
    fn enrichment_only_includes_enabled_local_log_providers() {
        let mut settings = Settings::default();
        settings.enabled_providers = [
            "codex".to_string(),
            "claude".to_string(),
            "cursor".to_string(),
        ]
        .into_iter()
        .collect();

        let mut ids = enrichment_provider_ids(&settings);
        ids.sort();
        assert_eq!(ids, vec!["claude".to_string(), "codex".to_string()]);

        settings.enabled_providers = ["cursor".to_string()].into_iter().collect();
        assert!(enrichment_provider_ids(&settings).is_empty());
    }

    #[test]
    fn scheduling_enrichment_with_no_enrichable_providers_is_a_noop() {
        let mut settings = Settings::default();
        settings.enabled_providers = ["cursor".to_string()].into_iter().collect();
        // Must return without spawning or panicking; nothing to enrich.
        schedule_refresh_enrichment(&settings);
    }

    #[test]
    fn core_refresh_completion_is_not_blocked_by_inflight_enrichment() {
        // A previous enrichment pass is still scanning, so hold the shared
        // lock the same way the background task does. Scheduling must skip
        // immediately instead of waiting, letting the core refresh command
        // finish and publish its results.
        let _guard =
            Arc::clone(ENRICHMENT_LOCK.get_or_init(|| Arc::new(tokio::sync::Mutex::new(()))))
                .try_lock_owned()
                .expect("test holds enrichment lock");
        let settings = Settings::default();
        schedule_refresh_enrichment(&settings);
    }
}
