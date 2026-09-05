use super::*;
use std::sync::Arc;

const MAX_CONCURRENT_PROVIDER_FETCHES: usize = 8;

// ── Provider refresh commands ────────────────────────────────────────

/// Build a `FetchContext` for a provider using persisted cookies/keys.
pub(crate) fn build_fetch_context(
    id: ProviderId,
    settings: &Settings,
    cookies: &ManualCookies,
    api_keys: &ApiKeys,
    token_accounts: &HashMap<ProviderId, ProviderAccountData>,
) -> FetchContext {
    let cookie_source = settings.cookie_source(id);
    let stored_cookie = cookies.get(id.cli_name()).map(|s| s.to_string());
    let stored_api_key = api_keys
        .get(id.cli_name())
        // Coding Plan and Agent Plan use the same Volcengine IAM identity as
        // the existing Ark/Doubao provider.  Reuse that credential so users
        // configure an access-key pair once rather than three times.
        .or_else(|| match id {
            ProviderId::ArkCodingPlan | ProviderId::ArkAgentPlan => {
                api_keys.get(ProviderId::Doubao.cli_name())
            }
            _ => None,
        })
        .map(|s| s.to_string());
    let token_override = token_accounts
        .get(&id)
        .and_then(|data| data.active_account())
        .cloned()
        .map(|account| TokenAccountOverride::from_account(id, account));
    let active_token_cookie = token_override
        .as_ref()
        .and_then(|override_data| override_data.cookie_header.clone());
    let active_token_env = token_override
        .as_ref()
        .and_then(|override_data| override_data.env_override.as_ref());
    let active_token_api_key = active_token_env.and_then(|env| env.values().next().cloned());
    let usage_source = SourceMode::parse(settings.usage_source(id)).unwrap_or_default();
    let api_key = stored_api_key.or(active_token_api_key);
    let has_kimi_code_api_key =
        id == ProviderId::Kimi && api_key.as_deref().is_some_and(|key| !key.trim().is_empty());
    // MiMo's pay-as-you-go provider validates an `sk-...` key and optionally
    // supplements it with the first-party console cookie for the shared
    // account balance. It has no CLI usage reader, so the generic
    // `manual cookie is empty -> Cli` fallback is invalid for this provider.
    let has_mimo_api_key =
        id == ProviderId::MiMoApi && api_key.as_deref().is_some_and(|key| !key.trim().is_empty());

    let provider_supports_web = instantiate_provider(id).supports_web();
    let (source_mode, cookie_header) = if has_mimo_api_key {
        // Keep an explicitly imported per-card cookie available, but always
        // execute the API-capable Auto source. MiMoApiProvider also falls back
        // to the shared `mimo` platform session for a balance lookup.
        (SourceMode::Auto, active_token_cookie.or(stored_cookie))
    } else if !provider_supports_web || provider_cookie_domain(id, settings).is_none() {
        let source_mode = if active_token_env.is_some() {
            SourceMode::OAuth
        } else {
            usage_source
        };
        (source_mode, None)
    } else {
        match cookie_source {
            _ if active_token_env.is_some() => (SourceMode::OAuth, None),
            "off" if id == ProviderId::Claude && usage_source != SourceMode::Cli => {
                (SourceMode::OAuth, None)
            }
            "off" if has_kimi_code_api_key && usage_source == SourceMode::Auto => {
                (SourceMode::Auto, None)
            }
            "off" => (SourceMode::Cli, None),
            "manual" => {
                let cookie_header = active_token_cookie.or(stored_cookie);
                let source_mode = if has_kimi_code_api_key && usage_source == SourceMode::Auto {
                    SourceMode::Auto
                } else if cookie_header.is_some() {
                    // A cookie the user supplied on purpose — pasted, imported
                    // from a file, or captured by the in-app login window — is
                    // an instruction to read that web session, so honour it.
                    SourceMode::Web
                } else {
                    // No cookie. Pinning the provider to one source here is
                    // what made a cookie feel mandatory: a provider holding a
                    // perfectly good local CLI token or OAuth credential was
                    // never allowed to try it. Hand control back to the
                    // provider's own ladder, which reads what is already on
                    // disk before it reaches for any browser session.
                    usage_source
                };
                (source_mode, cookie_header)
            }
            // `browser` is accepted as a legacy alias from older settings.
            // 2026-08-30 认证合同:这是默认路径 —— 先读取浏览器 Cookie，
            // 手动保存的 Cookie 只作高级故障兜底；拿到可用会话才走网页策略，
            // 全部不可用才回退 provider 自己的 OAuth/CLI/API 阶梯。
            "auto" | "browser" | "web" => {
                let browser_cookie = provider_cookie_domain(id, settings).and_then(|domain| {
                    match codexbar::browser::cookies::get_cookie_header(domain) {
                        Ok(header) if !header.trim().is_empty() => Some(header),
                        Ok(_) => None,
                        Err(error) => {
                            tracing::debug!(
                                provider = id.cli_name(),
                                domain,
                                error = %error,
                                "automatic browser cookie read failed; trying provider fallback"
                            );
                            None
                        }
                    }
                });
                let cookie_header = active_token_cookie.or(browser_cookie).or(stored_cookie);
                let source_mode = if has_kimi_code_api_key && usage_source == SourceMode::Auto {
                    SourceMode::Auto
                } else if cookie_header.is_some() {
                    SourceMode::Web
                } else {
                    usage_source
                };
                (source_mode, cookie_header)
            }
            _ => (usage_source, stored_cookie),
        }
    };

    let workspace_id = settings.workspace_id(id).trim().to_string();
    let api_region = settings.api_region(id).trim().to_string();
    let gateway_url = settings.gateway_url(id).trim().to_string();

    FetchContext {
        source_mode,
        manual_cookie_header: cookie_header,
        api_key,
        workspace_id: (!workspace_id.is_empty()).then_some(workspace_id),
        api_region: (!api_region.is_empty()).then_some(api_region),
        gateway_url: (!gateway_url.is_empty()).then_some(gateway_url),
        ..FetchContext::default()
    }
}

pub(crate) fn provider_cookie_domain(id: ProviderId, settings: &Settings) -> Option<&'static str> {
    if id == ProviderId::MiniMax {
        return Some(
            codexbar::providers::MiniMaxProvider::cookie_domain_for_region(Some(
                settings.api_region(id),
            )),
        );
    }
    if id == ProviderId::Alibaba {
        return Some(
            codexbar::providers::AlibabaProvider::cookie_domain_for_region(Some(
                settings.api_region(id),
            )),
        );
    }
    id.cookie_domain()
}

const DEFAULT_PROVIDER_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(35);
const SLOW_PROVIDER_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(75);
const MAX_CONTEXT_FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(65);
/// A timeout is usually a transient network/provider stall. Retry it three
/// times, with a short linear backoff, then pause this provider for automatic
/// refreshes until the user explicitly refreshes again.
pub(crate) const MAX_TIMEOUT_RETRIES: u8 = 3;
const TIMEOUT_RETRY_BASE_DELAY: std::time::Duration = std::time::Duration::from_millis(250);

pub(crate) fn timeout_retry_delay(retry_number: u8) -> std::time::Duration {
    TIMEOUT_RETRY_BASE_DELAY.saturating_mul(u32::from(retry_number.max(1)))
}

pub(crate) fn provider_fetch_timeout(id: ProviderId, ctx: &FetchContext) -> std::time::Duration {
    let provider_timeout = match id {
        ProviderId::Claude | ProviderId::Codex | ProviderId::Copilot => SLOW_PROVIDER_FETCH_TIMEOUT,
        _ => DEFAULT_PROVIDER_FETCH_TIMEOUT,
    };
    let context_timeout = std::time::Duration::from_secs(ctx.web_timeout.saturating_add(5));
    provider_timeout.max(context_timeout.min(MAX_CONTEXT_FETCH_TIMEOUT))
}

pub(crate) fn is_provider_cache_fresh(
    updated_at: Option<std::time::Instant>,
    stale_after: std::time::Duration,
) -> bool {
    updated_at
        .map(|updated| updated.elapsed() <= stale_after)
        .unwrap_or(false)
}

pub(crate) fn upsert_provider_cache(
    cache: &mut Vec<ProviderUsageSnapshot>,
    snapshot: ProviderUsageSnapshot,
) {
    if let Some(existing) = cache
        .iter_mut()
        .find(|existing| existing.provider_id == snapshot.provider_id)
    {
        *existing = snapshot;
    } else {
        cache.push(snapshot);
    }
}

pub(crate) fn prune_provider_cache_to_enabled(
    cache: &mut Vec<ProviderUsageSnapshot>,
    enabled_ids: &[ProviderId],
) -> bool {
    let before = cache.len();
    cache.retain(|snapshot| {
        enabled_ids
            .iter()
            .any(|id| id.cli_name() == snapshot.provider_id)
    });
    cache.len() != before
}

pub(crate) fn invalidate_provider_refresh_and_prune_disabled(
    state: &tauri::State<'_, Mutex<AppState>>,
    enabled_ids: &[ProviderId],
) -> Result<Option<VersionedProviderProjection>, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.provider_refresh_generation = guard.provider_refresh_generation.wrapping_add(1);
    let pruned = prune_provider_cache_to_enabled(&mut guard.provider_cache, enabled_ids);
    let projection = if pruned {
        guard.provider_projection_version = guard.provider_projection_version.wrapping_add(1);
        Some(VersionedProviderProjection {
            version: guard.provider_projection_version,
            snapshots: guard.provider_cache.clone(),
        })
    } else {
        None
    };
    guard
        .transient_provider_failure_counts
        .retain(|id, _| enabled_ids.contains(id));
    guard
        .timeout_paused_providers
        .retain(|id| enabled_ids.contains(id));
    Ok(projection)
}

fn is_current_provider_refresh_generation(guard: &AppState, generation: u64) -> bool {
    guard.provider_refresh_generation == generation
}

/// Core refresh logic, usable from both the Tauri command and tray menu actions.
pub(crate) async fn do_refresh_providers(app: &tauri::AppHandle) -> Result<(), String> {
    do_refresh_providers_with_policy(app, true, None).await
}

pub(crate) async fn do_refresh_providers_if_stale(app: &tauri::AppHandle) -> Result<(), String> {
    do_refresh_providers_with_policy(app, false, None).await
}

pub(crate) async fn do_refresh_one_provider(
    app: &tauri::AppHandle,
    id: ProviderId,
) -> Result<(), String> {
    do_refresh_providers_with_policy(app, true, Some(id)).await
}

async fn do_refresh_providers_with_policy(
    app: &tauri::AppHandle,
    force: bool,
    only: Option<ProviderId>,
) -> Result<(), String> {
    let state = app.state::<Mutex<AppState>>();

    let Some(generation) = begin_provider_refresh(&state, force, only)? else {
        return Ok(());
    };

    let mut inputs = ProviderRefreshInputs::load();
    if let Some(id) = only {
        inputs.enabled_ids = vec![id];
    }
    let pruned_projection = if only.is_none()
        && let Ok(mut guard) = state.lock()
        && is_current_provider_refresh_generation(&guard, generation)
        && prune_provider_cache_to_enabled(&mut guard.provider_cache, &inputs.enabled_ids)
    {
        guard.provider_projection_version = guard.provider_projection_version.wrapping_add(1);
        Some(VersionedProviderProjection {
            version: guard.provider_projection_version,
            snapshots: guard.provider_cache.clone(),
        })
    } else {
        None
    };
    // Pruning disabled providers is itself a projection change. Publish it
    // before refresh-started so every WebView drops removed rows immediately,
    // including the zero-enabled-provider case.
    if let Some(projection) = pruned_projection.as_ref() {
        events::emit_provider_projection_updated(app, projection);
    }
    events::emit_refresh_started(app);
    let enabled_count = inputs.enabled_ids.len();

    let handles = spawn_provider_refreshes(
        app,
        &inputs,
        generation,
        force,
        inputs.settings.provider_timeout_recovery_enabled,
    );
    await_provider_refreshes(handles).await;

    let error_count = finish_provider_refresh(&state, generation)?;
    update_tray_and_notifications(app, &state, &inputs.settings)?;

    events::emit_refresh_complete(app, enabled_count, error_count);

    // UP-M-001: core quota snapshots are already published above (each
    // provider emits `provider-updated` as it completes, and the shared cache
    // is updated before this point). The optional enrichment stage — local
    // cost/credits cache — runs in the background only after the core results
    // are out, so a slow or failed enrichment never keeps the core cards
    // waiting or makes them blank.
    crate::auto_refresh::schedule_refresh_enrichment(&inputs.settings);

    Ok(())
}

fn begin_provider_refresh(
    state: &tauri::State<'_, Mutex<AppState>>,
    force: bool,
    only: Option<ProviderId>,
) -> Result<Option<u64>, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    if guard.is_refreshing {
        return Ok(None);
    }
    if only.is_none() && provider_cache_can_skip_refresh(&guard, force) {
        return Ok(None);
    }

    guard.provider_refresh_generation = guard.provider_refresh_generation.wrapping_add(1);
    let generation = guard.provider_refresh_generation;
    if force {
        // A user-initiated refresh is the explicit recovery action for a
        // provider paused after repeated timeouts.
        if let Some(id) = only {
            guard.timeout_paused_providers.remove(&id);
        } else {
            guard.timeout_paused_providers.clear();
        }
    }
    guard.is_refreshing = true;
    guard.provider_refresh_started_at = Some(std::time::Instant::now());
    Ok(Some(generation))
}

fn provider_cache_can_skip_refresh(guard: &AppState, force: bool) -> bool {
    !force
        && !guard.provider_cache.is_empty()
        && is_provider_cache_fresh(guard.provider_cache_updated_at, PROVIDER_CACHE_STALE_AFTER)
}

struct ProviderRefreshInputs {
    settings: Settings,
    enabled_ids: Vec<ProviderId>,
    manual_cookies: ManualCookies,
    api_keys: ApiKeys,
    token_accounts: HashMap<ProviderId, ProviderAccountData>,
}

impl ProviderRefreshInputs {
    fn load() -> Self {
        let settings = Settings::load();
        let enabled_ids = settings.get_enabled_provider_ids();
        let manual_cookies = ManualCookies::load();
        let api_keys = ApiKeys::load();
        let token_accounts = TokenAccountStore::new().load().unwrap_or_else(|e| {
            tracing::warn!("failed to load token accounts for provider refresh: {e}");
            HashMap::new()
        });

        Self {
            settings,
            enabled_ids,
            manual_cookies,
            api_keys,
            token_accounts,
        }
    }
}

fn spawn_provider_refreshes(
    app: &tauri::AppHandle,
    inputs: &ProviderRefreshInputs,
    generation: u64,
    force: bool,
    timeout_recovery_enabled: bool,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut handles = Vec::with_capacity(inputs.enabled_ids.len());
    let fetch_permits = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_PROVIDER_FETCHES));
    let paused_providers = if force || !timeout_recovery_enabled {
        std::collections::HashSet::new()
    } else {
        app.state::<Mutex<AppState>>()
            .lock()
            .map(|guard| guard.timeout_paused_providers.clone())
            .unwrap_or_default()
    };

    for id in &inputs.enabled_ids {
        let id = *id;
        if paused_providers.contains(&id) {
            tracing::debug!(
                provider = id.cli_name(),
                "skipping automatic refresh for provider paused after timeout retries"
            );
            continue;
        }
        let app_handle = app.clone();
        let fetch_permits = Arc::clone(&fetch_permits);
        let ctx = build_fetch_context(
            id,
            &inputs.settings,
            &inputs.manual_cookies,
            &inputs.api_keys,
            &inputs.token_accounts,
        );

        handles.push(tokio::spawn(async move {
            let Ok(_permit) = fetch_permits.acquire_owned().await else {
                return;
            };
            refresh_provider(app_handle, id, ctx, generation, timeout_recovery_enabled).await;
        }));
    }

    handles
}

async fn refresh_provider(
    app: tauri::AppHandle,
    id: ProviderId,
    ctx: FetchContext,
    generation: u64,
    timeout_recovery_enabled: bool,
) {
    let fetch = fetch_provider_snapshot(id, ctx, timeout_recovery_enabled).await;
    let snapshot = fetch.snapshot;

    let state = app.state::<Mutex<AppState>>();
    if let Ok(mut guard) = state.lock() {
        if !is_current_provider_refresh_generation(&guard, generation) {
            tracing::debug!(
                provider = id.cli_name(),
                generation,
                "dropping superseded provider refresh result"
            );
            return;
        }
        if timeout_recovery_enabled && fetch.timeout_retries_exhausted {
            guard.timeout_paused_providers.insert(id);
            tracing::warn!(
                provider = id.cli_name(),
                retries = MAX_TIMEOUT_RETRIES,
                "pausing provider after timeout retries; manual refresh will resume it"
            );
        } else {
            // A successful result, or a non-timeout error, must not leave an
            // old timeout pause blocking future automatic refreshes.
            guard.timeout_paused_providers.remove(&id);
        }
        let snapshot = preserve_last_good_transient_failure(&mut guard, id, snapshot);
        upsert_provider_cache(&mut guard.provider_cache, snapshot.clone());
        guard.provider_projection_version = guard.provider_projection_version.wrapping_add(1);
        let projection = VersionedProviderProjection {
            version: guard.provider_projection_version,
            snapshots: guard.provider_cache.clone(),
        };
        drop(guard);
        events::emit_provider_updated(&app, &snapshot);
        events::emit_provider_projection_updated(&app, &projection);
    } else {
        events::emit_provider_updated(&app, &snapshot);
    }
}

pub(super) fn preserve_last_good_transient_failure(
    guard: &mut AppState,
    id: ProviderId,
    snapshot: ProviderUsageSnapshot,
) -> ProviderUsageSnapshot {
    if snapshot.error.is_none() {
        guard.transient_provider_failure_counts.remove(&id);
        return snapshot;
    }

    if id != ProviderId::Claude || !is_transient_claude_auth_error(snapshot.error.as_deref()) {
        guard.transient_provider_failure_counts.remove(&id);
        return snapshot;
    }

    let Some(previous) = guard
        .provider_cache
        .iter()
        .find(|cached| cached.provider_id == id.cli_name() && cached.error.is_none())
        .cloned()
    else {
        return snapshot;
    };

    let count = guard
        .transient_provider_failure_counts
        .entry(id)
        .or_insert(0);
    if *count == 0 {
        *count = 1;
        tracing::warn!(
            provider = id.cli_name(),
            "preserving last good provider snapshot after transient auth failure"
        );
        previous
    } else {
        *count = count.saturating_add(1);
        snapshot
    }
}

fn is_transient_claude_auth_error(error: Option<&str>) -> bool {
    let Some(error) = error else {
        return false;
    };
    let lower = error.to_ascii_lowercase();
    lower.contains("unauthorized")
        || lower.contains("authentication required")
        || lower.contains("auth required")
        || lower.contains("oauth")
}

struct ProviderFetchAttempt {
    snapshot: ProviderUsageSnapshot,
    timeout_retries_exhausted: bool,
}

pub(crate) fn provider_error_is_timeout(error: &codexbar::core::ProviderError) -> bool {
    match error {
        codexbar::core::ProviderError::Timeout => true,
        codexbar::core::ProviderError::Network(error) => error.is_timeout(),
        codexbar::core::ProviderError::Other(message) => {
            let lower = message.to_ascii_lowercase();
            lower.contains("timeout")
                || lower.contains("timed out")
                || lower.contains("deadline exceeded")
                || lower.contains("gateway timeout")
                || lower.contains("504")
        }
        _ => false,
    }
}

async fn fetch_provider_snapshot(
    id: ProviderId,
    ctx: FetchContext,
    timeout_recovery_enabled: bool,
) -> ProviderFetchAttempt {
    let provider = instantiate_provider(id);
    let metadata = provider.metadata().clone();
    let started = std::time::Instant::now();

    let mut retries = 0;
    loop {
        let (mut snapshot, timed_out) = match tokio::time::timeout(
            provider_fetch_timeout(id, &ctx),
            provider.fetch_usage(&ctx),
        )
        .await
        {
            Ok(Ok(result)) => (
                ProviderUsageSnapshot::from_fetch_result(id, &metadata, &result),
                false,
            ),
            Ok(Err(error)) => {
                let timed_out = provider_error_is_timeout(&error);
                (
                    ProviderUsageSnapshot::from_error(
                        id,
                        &metadata,
                        codexbar::logging::safe_error_message(error),
                    ),
                    timed_out,
                )
            }
            Err(_) => (
                ProviderUsageSnapshot::from_error(id, &metadata, "Timeout".to_string()),
                true,
            ),
        };

        if timeout_recovery_enabled && timed_out && retries < MAX_TIMEOUT_RETRIES {
            retries += 1;
            tracing::warn!(
                provider = id.cli_name(),
                retry = retries,
                max_retries = MAX_TIMEOUT_RETRIES,
                "provider refresh timed out; retrying"
            );
            tokio::time::sleep(timeout_retry_delay(retries)).await;
            continue;
        }

        if timeout_recovery_enabled && timed_out {
            tracing::warn!(
                provider = id.cli_name(),
                retries,
                "provider refresh timed out after retries; pausing automatic refresh"
            );
        }
        record_provider_fetch_duration(id, &mut snapshot, started);
        return ProviderFetchAttempt {
            snapshot,
            timeout_retries_exhausted: timeout_recovery_enabled && timed_out,
        };
    }
}

fn record_provider_fetch_duration(
    id: ProviderId,
    snapshot: &mut ProviderUsageSnapshot,
    started: std::time::Instant,
) {
    let fetch_duration_ms = started.elapsed().as_millis();
    snapshot.fetch_duration_ms = Some(fetch_duration_ms);
    if fetch_duration_ms > 5_000 {
        tracing::warn!(
            provider = id.cli_name(),
            fetch_duration_ms,
            "slow provider refresh"
        );
    }
}

async fn await_provider_refreshes(handles: Vec<tokio::task::JoinHandle<()>>) {
    for handle in handles {
        let _ = handle.await;
    }
}

fn finish_provider_refresh(
    state: &tauri::State<'_, Mutex<AppState>>,
    generation: u64,
) -> Result<usize, String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.is_refreshing = false;
    if is_current_provider_refresh_generation(&guard, generation) {
        guard.provider_cache_updated_at = Some(std::time::Instant::now());
    }
    guard.provider_refresh_started_at = None;
    Ok(guard
        .provider_cache
        .iter()
        .filter(|s| s.error.is_some())
        .count())
}

fn update_tray_and_notifications(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, Mutex<AppState>>,
    settings: &Settings,
) -> Result<(), String> {
    let cached = {
        let guard = state.lock().map_err(|e| e.to_string())?;
        guard.provider_cache.clone()
    };
    crate::tray_bridge::update_tray_status_items(app, &cached);
    crate::tray_bridge::update_tray_icon_and_tooltip(app, &cached);
    notify_usage_thresholds(state, settings, &cached);
    Ok(())
}

fn notify_usage_thresholds(
    state: &tauri::State<'_, Mutex<AppState>>,
    settings: &Settings,
    cached: &[ProviderUsageSnapshot],
) {
    let cli_map = codexbar::core::cli_name_map();
    if let Ok(mut guard) = state.lock() {
        for snapshot in cached {
            if snapshot.error.is_none()
                && let Some(&provider) = cli_map.get(snapshot.provider_id.as_str())
            {
                guard.notification_manager.check_and_notify(
                    provider,
                    snapshot.primary.used_percent,
                    settings,
                );
                guard.notification_manager.check_session_transition(
                    provider,
                    snapshot.primary.used_percent,
                    settings,
                );
            }
        }
    }
}

#[tauri::command]
pub async fn refresh_providers(app: tauri::AppHandle) -> Result<(), String> {
    do_refresh_providers(&app).await
}

#[tauri::command]
pub async fn refresh_providers_if_stale(app: tauri::AppHandle) -> Result<(), String> {
    do_refresh_providers_if_stale(&app).await
}

#[tauri::command]
pub fn get_cached_providers(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Vec<ProviderUsageSnapshot> {
    state
        .lock()
        .map(|guard| guard.provider_cache.clone())
        .unwrap_or_default()
}

/// Read the complete authoritative provider projection and its monotonic
/// version. Frontends consume this once at boot and then follow the matching
/// `provider-projection-updated` event; no WebView performs a partial upsert.
#[tauri::command]
pub fn get_provider_projection(
    state: tauri::State<'_, Mutex<AppState>>,
) -> VersionedProviderProjection {
    state
        .lock()
        .map(|guard| VersionedProviderProjection {
            version: guard.provider_projection_version,
            snapshots: guard.provider_cache.clone(),
        })
        .unwrap_or_else(|_| VersionedProviderProjection {
            version: 0,
            snapshots: Vec::new(),
        })
}
