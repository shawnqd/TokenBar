use super::*;
use serde::Deserialize;
use tauri::{Emitter, Manager};

// ── SurfaceRegistry host primitives ────────────────────────────────
//
// Host-side mapping for TS SurfaceRegistry / actionDispatcher.
//
// | TS SurfaceAction (`type`)  | Rust host primitive (existing)          | Notes |
// |----------------------------|-----------------------------------------|-------|
// | refresh                    | `refresh_providers`                     | background fetch, no surface change |
// | openSettings {tab}         | `open_settings_window(tab)`             | detached Settings window |
// | quit                       | `quit_app`                              | exits process |
// | selectProvider {providerId}| `open_flyout_window` + emit select      | TrayPanel dedicated window |
// | openProviderDetail         | `open_settings_window("providers")`     | detail pane lives in Settings |
// | openExternalUsage          | `open_provider_dashboard` (system.rs)   | external browser |
// | openExternalStatus         | `open_provider_status_page`             | external browser |
// | triggerLogin               | `trigger_provider_login`                | OAuth / dashboard fallback |
//
// TS dispatcher may either invoke the individual commands above or the
// unified `surface_action` thin router below. The router is intentionally
// thin — it contains no new surface behavior, quota logic, auth or
// secure-storage changes — it only routes to the primitives listed above.
// Frontend invoke signature — nested target matches TS `SurfaceAction`:
// ```ts
// invoke<string>("surface_action", { action: { type: "openSettings", target: { kind: "settings", tab: "general" } } })
// invoke<string>("surface_action", { action: { type: "openExternalUsage", target: { kind: "provider", providerId: "codex" } } })
// ```

// ── Unified thin router for TS SurfaceRegistry (CORE-05) ─────────────

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WireSurfaceTarget {
    App,
    Summary,
    Settings {
        #[serde(default)]
        tab: Option<String>,
    },
    Provider {
        #[serde(rename = "providerId")]
        provider_id: String,
    },
    ProviderOptional {
        #[serde(rename = "providerId")]
        provider_id: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SurfaceAction {
    Refresh {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        #[serde(default)]
        force: bool,
    },
    OpenSettings {
        target: WireSurfaceTarget,
    },
    Quit {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
    SelectProvider {
        target: WireSurfaceTarget,
    },
    OpenProviderDetail {
        target: WireSurfaceTarget,
    },
    OpenExternalUsage {
        target: WireSurfaceTarget,
    },
    OpenExternalStatus {
        target: WireSurfaceTarget,
    },
    TriggerLogin {
        target: WireSurfaceTarget,
    },
    Dismiss {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
    ReorderProviders {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        #[serde(rename = "providerIds", default)]
        provider_ids: Vec<String>,
    },
    ClearCache {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
    SetApiKey {
        target: WireSurfaceTarget,
        #[serde(rename = "apiKey")]
        api_key: String,
        #[serde(default)]
        label: Option<String>,
    },
    RemoveApiKey {
        target: WireSurfaceTarget,
    },
    SetManualCookie {
        target: WireSurfaceTarget,
        #[serde(rename = "cookieHeader")]
        cookie_header: String,
    },
    RemoveManualCookie {
        target: WireSurfaceTarget,
    },
    ImportCookieFile {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        contents: String,
        #[serde(rename = "providerIds", default)]
        provider_ids: Vec<String>,
    },
    OpenProviderLogin {
        target: WireSurfaceTarget,
    },
    CaptureProviderLogin {
        target: WireSurfaceTarget,
    },
    CloseProviderLogin {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
    AddTokenAccount {
        target: WireSurfaceTarget,
        label: String,
        token: String,
    },
    RemoveTokenAccount {
        target: WireSurfaceTarget,
        #[serde(rename = "accountId")]
        account_id: String,
    },
    SetActiveTokenAccount {
        target: WireSurfaceTarget,
        #[serde(rename = "accountId")]
        account_id: String,
    },
    RevokeCredentials {
        target: WireSurfaceTarget,
    },
    SetCookieSource {
        target: WireSurfaceTarget,
        source: String,
    },
    SetRegion {
        target: WireSurfaceTarget,
        region: String,
    },
    ResetSettings {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
    CloseSettings {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
    OpenExternalUrl {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        url: String,
    },
    OpenPath {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        path: String,
    },
    SetWorkspaceId {
        target: WireSurfaceTarget,
        #[serde(rename = "workspaceId")]
        workspace_id: String,
    },
    SetGatewayUrl {
        target: WireSurfaceTarget,
        #[serde(rename = "gatewayUrl")]
        gateway_url: String,
    },
    SetIdePath {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        path: String,
    },
    UpdateSettings {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        patch: super::SettingsUpdate,
    },
    RegisterGlobalShortcut {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        accelerator: String,
    },
    UnregisterGlobalShortcut {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
    SetUiLanguage {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
        language: String,
    },
    PlayNotificationSound {
        #[serde(default)]
        target: Option<WireSurfaceTarget>,
    },
}

fn provider_id_of(target: &WireSurfaceTarget) -> Result<&str, String> {
    match target {
        WireSurfaceTarget::Provider { provider_id } => Ok(provider_id),
        WireSurfaceTarget::ProviderOptional {
            provider_id: Some(id),
        } => Ok(id),
        _ => Err("missing providerId".to_string()),
    }
}

fn settings_tab_of(target: &WireSurfaceTarget) -> String {
    match target {
        WireSurfaceTarget::Settings { tab } => {
            tab.clone().unwrap_or_else(|| "general".to_string())
        }
        _ => "general".to_string(),
    }
}

fn optional_provider_id(target: &WireSurfaceTarget) -> Option<&str> {
    match target {
        WireSurfaceTarget::Provider { provider_id } => Some(provider_id.as_str()),
        WireSurfaceTarget::ProviderOptional { provider_id } => provider_id.as_deref(),
        _ => None,
    }
}

/// Thin router for the TS SurfaceRegistry / actionDispatcher.
///
/// Every variant delegates to an existing host primitive (see table above)
/// — no new provider, quota, auth or storage logic is introduced here.
/// `refresh` ignores its optional `providerId` and refreshes all enabled
/// providers, matching the existing `refresh_providers` semantics.
#[tauri::command]
pub async fn surface_action(
    app: tauri::AppHandle,
    action: SurfaceAction,
) -> Result<String, String> {
    match action {
        SurfaceAction::Refresh { force, .. } => {
            if force {
                crate::commands::refresh_providers(app).await?;
            } else {
                crate::commands::refresh_providers_if_stale(app).await?;
            }
            Ok("refreshed".to_string())
        }
        SurfaceAction::OpenSettings { target } => {
            let tab = settings_tab_of(&target);
            crate::shell::settings_window::open_or_focus(&app, &tab)
                .map(|_| format!("open_settings:{tab}"))
        }
        SurfaceAction::Quit { .. } => {
            app.exit(0);
            Ok("quit".to_string())
        }
        SurfaceAction::SelectProvider { target } => {
            crate::shell::flyout_window::open_or_focus(&app, None)
                .map_err(|e| e.to_string())?;
            if let Some(pid) = optional_provider_id(&target) {
                let _ = app.emit("flyout-select-provider", pid.to_string());
                return Ok(format!("select_provider:{pid}"));
            }
            Ok("select_provider".to_string())
        }
        SurfaceAction::OpenProviderDetail { target } => {
            let provider_id = provider_id_of(&target)?;
            crate::shell::settings_window::open_or_focus(&app, "providers")
                .map_err(|e| e.to_string())?;
            let _ = app.emit("settings-change-tab", "providers");
            Ok(format!("open_provider_detail:{provider_id}"))
        }
        SurfaceAction::OpenExternalUsage { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::open_provider_dashboard(provider_id.clone())?;
            Ok(format!("open_external_usage:{provider_id}"))
        }
        SurfaceAction::OpenExternalStatus { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::open_provider_status_page(provider_id.clone())?;
            Ok(format!("open_external_status:{provider_id}"))
        }
        SurfaceAction::TriggerLogin { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::trigger_provider_login(app, provider_id.clone()).await?;
            Ok(format!("trigger_login:{provider_id}"))
        }
        SurfaceAction::Dismiss { .. } => {
            crate::shell::flyout_window::hide(&app)?;
            Ok("dismissed".to_string())
        }
        SurfaceAction::ReorderProviders { provider_ids, .. } => {
            crate::commands::reorder_providers(app, provider_ids)?;
            Ok("reordered".to_string())
        }
        SurfaceAction::ClearCache { .. } => {
            crate::commands::clear_provider_local_usage_cache();
            Ok("cache_cleared".to_string())
        }
        SurfaceAction::SetApiKey {
            target,
            api_key,
            label,
        } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::set_api_key(provider_id.clone(), api_key, label)?;
            Ok(format!("set_api_key:{provider_id}"))
        }
        SurfaceAction::RemoveApiKey { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::remove_api_key(provider_id.clone())?;
            Ok(format!("remove_api_key:{provider_id}"))
        }
        SurfaceAction::SetManualCookie {
            target,
            cookie_header,
        } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::set_manual_cookie(provider_id.clone(), cookie_header)?;
            Ok(format!("set_manual_cookie:{provider_id}"))
        }
        SurfaceAction::RemoveManualCookie { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::remove_manual_cookie(provider_id.clone())?;
            Ok(format!("remove_manual_cookie:{provider_id}"))
        }
        SurfaceAction::ImportCookieFile {
            contents,
            provider_ids,
            ..
        } => {
            crate::commands::import_cookie_file(contents, provider_ids)?;
            Ok("import_cookie_file".to_string())
        }
        SurfaceAction::OpenProviderLogin { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::open_provider_login(app, provider_id.clone())?;
            Ok(format!("open_provider_login:{provider_id}"))
        }
        SurfaceAction::CaptureProviderLogin { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::capture_provider_login(app, provider_id.clone()).await?;
            Ok(format!("capture_provider_login:{provider_id}"))
        }
        SurfaceAction::CloseProviderLogin { .. } => {
            crate::commands::close_provider_login(app)?;
            Ok("close_provider_login".to_string())
        }
        SurfaceAction::AddTokenAccount {
            target,
            label,
            token,
        } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::add_token_account(provider_id.clone(), label, token)?;
            Ok(format!("add_token_account:{provider_id}"))
        }
        SurfaceAction::RemoveTokenAccount {
            target,
            account_id,
        } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::remove_token_account(provider_id.clone(), account_id)?;
            Ok(format!("remove_token_account:{provider_id}"))
        }
        SurfaceAction::SetActiveTokenAccount {
            target,
            account_id,
        } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::set_active_token_account(provider_id.clone(), account_id)?;
            Ok(format!("set_active_token_account:{provider_id}"))
        }
        SurfaceAction::RevokeCredentials { target } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::revoke_provider_credentials(provider_id.clone())?;
            Ok(format!("revoke_credentials:{provider_id}"))
        }
        SurfaceAction::SetCookieSource { target, source } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::set_provider_cookie_source(provider_id.clone(), source)?;
            Ok(format!("set_cookie_source:{provider_id}"))
        }
        SurfaceAction::SetRegion { target, region } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::set_provider_region(provider_id.clone(), region)?;
            Ok(format!("set_region:{provider_id}"))
        }
        SurfaceAction::ResetSettings { .. } => {
            crate::commands::reset_settings()?;
            Ok("reset_settings".to_string())
        }
        SurfaceAction::UpdateSettings { patch, .. } => {
            crate::commands::update_settings(app, patch).await?;
            Ok("settings_updated".to_string())
        }
        SurfaceAction::CloseSettings { .. } => {
            if let Some(window) = app.get_webview_window(crate::shell::settings_window::SETTINGS_LABEL)
            {
                crate::shell::settings_window::dismiss(&app, &window)?;
            }
            Ok("close_settings".to_string())
        }
        SurfaceAction::OpenExternalUrl { url, .. } => {
            crate::commands::open_external_url(url)?;
            Ok("open_external_url".to_string())
        }
        SurfaceAction::OpenPath { path, .. } => {
            crate::commands::open_path(path)?;
            Ok("open_path".to_string())
        }
        SurfaceAction::SetWorkspaceId {
            target,
            workspace_id,
        } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::set_provider_workspace_id(provider_id.clone(), workspace_id)?;
            Ok(format!("set_workspace_id:{provider_id}"))
        }
        SurfaceAction::SetGatewayUrl {
            target,
            gateway_url,
        } => {
            let provider_id = provider_id_of(&target)?.to_string();
            crate::commands::set_provider_gateway_url(provider_id.clone(), gateway_url)?;
            Ok(format!("set_gateway_url:{provider_id}"))
        }
        SurfaceAction::SetIdePath { path, .. } => {
            crate::commands::set_jetbrains_ide_path(path)?;
            Ok("set_ide_path".to_string())
        }
        SurfaceAction::RegisterGlobalShortcut { accelerator, .. } => {
            crate::commands::register_global_shortcut(app, accelerator.clone())?;
            Ok(format!("register_global_shortcut:{accelerator}"))
        }
        SurfaceAction::UnregisterGlobalShortcut { .. } => {
            crate::commands::unregister_global_shortcut(app)?;
            Ok("unregister_global_shortcut".to_string())
        }
        SurfaceAction::SetUiLanguage { language, .. } => {
            crate::commands::set_ui_language(app, language.clone())?;
            Ok(format!("set_ui_language:{language}"))
        }
        SurfaceAction::PlayNotificationSound { .. } => {
            crate::commands::play_notification_sound()?;
            Ok("play_notification_sound".to_string())
        }
    }
}

// ── Surface-mode commands ────────────────────────────────────────────

#[tauri::command]
pub fn set_surface_mode(
    mode: String,
    target: SurfaceTarget,
    window: tauri::WebviewWindow,
) -> Result<String, String> {
    let mode = SurfaceMode::parse(&mode).ok_or_else(|| format!("unknown surface mode: {mode}"))?;
    let target = validate_surface_target(mode, target)?;

    crate::shell::transition_to_target(window.app_handle(), mode, target, None)
        .map(|mode| mode.as_str().to_string())
}

#[tauri::command]
pub fn dismiss_tray_panel(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::flyout_window::hide(&app)
}

/// Fallback native resize drag. The live tray uses Tauri
/// `startResizeDragging`; this keeps the same ReleaseCapture + HT* sequence
/// if a caller still invokes the command.
#[tauri::command]
pub fn begin_tray_panel_resize(app: tauri::AppHandle, dir: String) -> Result<(), String> {
    crate::shell::flyout_window::begin_resize(&app, &dir)
}

/// Arm the gesture blur guard before a resize-grip drag or drag-reorder
/// gesture starts its Win32/OLE modal loop, so the transient
/// `Focused(false)` that loop produces doesn't auto-hide the flyout.
#[tauri::command]
pub fn begin_flyout_gesture(app: tauri::AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<Mutex<AppState>>()
        .ok_or_else(|| "app state unavailable".to_string())?;
    state
        .lock()
        .map_err(|e| e.to_string())?
        .begin_gesture_blur_guard(std::time::Instant::now());
    Ok(())
}

/// Disarm the gesture blur guard when a gesture ends (mouseup / dragend),
/// so a genuine outside click can dismiss the flyout again immediately.
#[tauri::command]
pub fn end_flyout_gesture(app: tauri::AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<Mutex<AppState>>()
        .ok_or_else(|| "app state unavailable".to_string())?;
    state
        .lock()
        .map_err(|e| e.to_string())?
        .end_gesture_blur_guard();
    Ok(())
}

/// Open (or focus) a detached Settings/About window.
///
/// Unlike `set_surface_mode`, this spawns a *separate* window so the tray
/// panel stays open.  On Windows, `WebviewWindowBuilder::build` deadlocks
/// inside synchronous Tauri commands, so this must be `async`.
#[tauri::command]
pub async fn open_settings_window(app: tauri::AppHandle, tab: String) -> Result<(), String> {
    crate::shell::settings_window::open_or_focus(&app, &tab)
}

/// Open (or focus) the detached "Open Tray Panel" window.
///
/// Used by the tray panel's "back to tray" flows. Opening the flyout was
/// previously `set_surface_mode("trayPanel", ...)` on the shared window — now
/// that the flyout is its own window, this opens it directly instead. Same
/// `async` requirement as `open_settings_window`: `WebviewWindowBuilder::build`
/// deadlocks inside synchronous Tauri commands on Windows.
#[tauri::command]
pub async fn open_flyout_window(app: tauri::AppHandle) -> Result<(), String> {
    crate::shell::flyout_window::open_or_focus(&app, None)
}

/// Reveal the flyout window after the frontend's fixed shell is ready. Called
/// by `useTrayPanelLayout` after the native size and tray anchor have been
/// applied, so Windows never shows a blank/backing frame during first paint.
///
/// No-ops when the flyout window doesn't exist or no one-shot reveal is pending.
#[tauri::command]
pub fn reveal_tray_panel_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.mark_flyout_frontend_ready();
    if !guard.take_pending_flyout_reveal() {
        return Ok(());
    }
    drop(guard);
    // The native flyout module is the sole owner of show/focus/geometry and
    // reveal events. Keeping this command as a readiness bridge prevents a
    // second, subtly different reveal path from reintroducing focus races.
    crate::shell::flyout_window::reveal_ready(&app)
}

/// Mark the prewarmed Settings frontend ready and reveal it only when an open
/// request arrived before the lazy Settings surface finished its first render.
#[tauri::command]
pub fn reveal_settings_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<bool, String> {
    use tauri::{Emitter, Manager};

    let Some(window) = app.get_webview_window(crate::shell::settings_window::SETTINGS_LABEL) else {
        return Ok(false);
    };
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.mark_settings_frontend_ready();
    let Some(tab) = guard.take_pending_settings_reveal() else {
        // During prewarm there is no pending reveal and the window is hidden;
        // during a Vite/HMR remount the same call can happen while Settings is
        // already visible. Returning that fact lets the frontend unpark only
        // the latter case, avoiding a first-open flash without breaking live
        // development remounts.
        return Ok(window.is_visible().unwrap_or(false));
    };
    drop(guard);

    app.emit_to(
        crate::shell::settings_window::SETTINGS_LABEL,
        "settings-change-tab",
        tab,
    )
    .map_err(|e| e.to_string())?;
    let _ = window.set_decorations(false);
    crate::shell::dwm::force_borderless_transparent_resizable(&window);
    window.show().map_err(|e| e.to_string())?;
    let _ = window.set_decorations(false);
    crate::shell::dwm::force_borderless_transparent_resizable(&window);
    window.set_focus().map_err(|e| e.to_string())?;
    // This path only runs for a reveal that was armed while the window was
    // still hidden, so it is always a genuine hidden -> visible transition.
    app.emit_to(
        crate::shell::settings_window::SETTINGS_LABEL,
        crate::shell::settings_window::SETTINGS_REVEALED_EVENT,
        (),
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub fn close_settings_window(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
) -> Result<(), String> {
    crate::shell::settings_window::dismiss(&app, &window)
}

#[tauri::command]
pub fn get_current_surface_mode(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<String, String> {
    Ok(state
        .lock()
        .map_err(|e| e.to_string())?
        .surface_machine
        .current()
        .as_str()
        .to_string())
}

#[tauri::command]
pub fn get_current_surface_state(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<CurrentSurfaceState, String> {
    let guard = state.lock().map_err(|e| e.to_string())?;
    Ok(CurrentSurfaceState {
        mode: guard.surface_machine.current().as_str().to_string(),
        target: guard.current_target.clone(),
    })
}

#[tauri::command]
pub fn get_proof_state(app: tauri::AppHandle) -> Result<ProofStatePayload, String> {
    proof_harness::ensure_proof_mode(&app)?;
    proof_harness::capture_state(&app)
}

// `async` so `open-tray-panel` can safely reach `flyout_window::open_or_focus`
// on its first-ever call (before the flyout window exists yet): that path's
// `WebviewWindowBuilder::build()` deadlocks on Windows if driven from a sync
// Tauri command's own thread. `proof_harness::run_command` itself stays sync
// — moving just this outer command onto the async dispatch path is enough,
// matching the existing `open_flyout_window` async command.
#[tauri::command]
pub async fn run_proof_command(
    app: tauri::AppHandle,
    command: String,
) -> Result<ProofStatePayload, String> {
    let command =
        ProofCommand::parse(&command).ok_or_else(|| format!("unknown proof command: {command}"))?;
    proof_harness::run_command(&app, command)
}

pub(crate) fn validate_surface_target(
    mode: SurfaceMode,
    target: SurfaceTarget,
) -> Result<SurfaceTarget, String> {
    if mode == SurfaceMode::Hidden {
        return Err("set_surface_mode only supports visible surfaces".into());
    }

    if target.mode() != mode {
        return Err(format!(
            "surface target '{}' is not valid for mode '{}'",
            target_label(&target),
            mode.as_str()
        ));
    }

    // `trayPanel` is no longer a state the shared `main` window may enter —
    // the tray panel is its own dedicated `flyout` window now (see
    // `shell::flyout_window`). This check runs AFTER the mismatch check above
    // so a mismatched trayPanel request (e.g. `target: settings`) still
    // reports the ordinary "not valid for mode" error — only targets whose
    // host mode is `SurfaceMode::TrayPanel` (summary and provider deep links)
    // reach this branch and are rejected here.
    if mode == SurfaceMode::TrayPanel {
        return Err(
            "set_surface_mode does not support 'trayPanel': the tray panel is a dedicated window \
             now — call open_flyout_window instead"
                .into(),
        );
    }

    if mode == SurfaceMode::Settings {
        return Err(
            "set_surface_mode does not support 'settings': Settings is a dedicated window \
             now — call open_settings_window instead"
                .into(),
        );
    }

    Ok(target)
}

fn target_label(target: &SurfaceTarget) -> String {
    match target {
        SurfaceTarget::Summary => "summary".into(),
        SurfaceTarget::Provider { provider_id } => format!("provider:{provider_id}"),
        SurfaceTarget::Settings { tab } => format!("settings:{tab}"),
    }
}

#[cfg(test)]
mod tests_surface_action {
    use super::SurfaceAction;

    #[test]
    fn surface_action_deserializes_refresh_and_open_settings() {
        let a: SurfaceAction =
            serde_json::from_str(r#"{"type":"refresh"}"#).expect("refresh");
        matches!(a, SurfaceAction::Refresh { .. });
        let b: SurfaceAction = serde_json::from_str(
            r#"{"type":"openSettings","target":{"kind":"settings","tab":"general"}}"#,
        )
        .expect("openSettings");
        match b {
            SurfaceAction::OpenSettings { target } => match target {
                super::WireSurfaceTarget::Settings { tab } => {
                    assert_eq!(tab.as_deref(), Some("general"))
                }
                _ => panic!("wrong target"),
            },
            _ => panic!("wrong variant"),
        }
        let c: SurfaceAction =
            serde_json::from_str(r#"{"type":"quit","target":{"kind":"app"}}"#).expect("quit");
        matches!(c, SurfaceAction::Quit { .. });
    }

    #[test]
    fn surface_action_deserializes_provider_variants() {
        let cases = [
            (
                r#"{"type":"selectProvider","target":{"kind":"providerOptional","providerId":"codex"}}"#,
                "selectProvider",
            ),
            (
                r#"{"type":"openProviderDetail","target":{"kind":"provider","providerId":"claude"}}"#,
                "openProviderDetail",
            ),
            (
                r#"{"type":"openExternalUsage","target":{"kind":"provider","providerId":"codex"}}"#,
                "openExternalUsage",
            ),
            (
                r#"{"type":"openExternalStatus","target":{"kind":"provider","providerId":"codex"}}"#,
                "openExternalStatus",
            ),
            (
                r#"{"type":"triggerLogin","target":{"kind":"provider","providerId":"codex"}}"#,
                "triggerLogin",
            ),
        ];
        for (json, _) in cases {
            let _: SurfaceAction = serde_json::from_str(json).expect(json);
        }
    }

    #[test]
    fn surface_action_refresh_accepts_optional_provider_target() {
        let a: SurfaceAction = serde_json::from_str(
            r#"{"type":"refresh","target":{"kind":"provider","providerId":"codex"}}"#,
        )
        .expect("refresh with id");
        match a {
            SurfaceAction::Refresh { target, .. } => match target {
                Some(super::WireSurfaceTarget::Provider { provider_id }) => {
                    assert_eq!(provider_id, "codex")
                }
                _ => panic!("wrong target"),
            },
            _ => panic!("wrong variant"),
        }
        let b: SurfaceAction =
            serde_json::from_str(r#"{"type":"refresh","target":null}"#).expect("refresh null");
        matches!(b, SurfaceAction::Refresh { .. });
    }

    #[test]
    fn flat_action_shape_is_rejected() {
        let flat = serde_json::from_str::<SurfaceAction>(
            r#"{"type":"openExternalUsage","providerId":"codex"}"#,
        );
        assert!(flat.is_err(), "flat providerId must not deserialize");
    }

    #[test]
    fn surface_action_deserializes_dismiss_reorder_and_clear_cache() {
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"dismiss","target":{"kind":"summary"}}"#,
        )
        .expect("dismiss");
        let reorder: SurfaceAction = serde_json::from_str(
            r#"{"type":"reorderProviders","target":{"kind":"summary"},"providerIds":["claude","codex"]}"#,
        )
        .expect("reorder");
        match reorder {
            SurfaceAction::ReorderProviders { provider_ids, .. } => {
                assert_eq!(provider_ids, ["claude", "codex"]);
            }
            _ => panic!("wrong variant"),
        }
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"clearCache","target":{"kind":"settings"}}"#,
        )
        .expect("clearCache");
    }

    #[test]
    fn surface_action_deserializes_credential_and_account_writes() {
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"setApiKey","target":{"kind":"provider","providerId":"openrouter"},"apiKey":"sk-test","label":"prod"}"#,
        )
        .expect("setApiKey");
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"setManualCookie","target":{"kind":"provider","providerId":"claude"},"cookieHeader":"a=b"}"#,
        )
        .expect("setManualCookie");
        let import: SurfaceAction = serde_json::from_str(
            r#"{"type":"importCookieFile","target":{"kind":"settings"},"contents":"netscape","providerIds":["claude"]}"#,
        )
        .expect("importCookieFile");
        match import {
            SurfaceAction::ImportCookieFile { provider_ids, .. } => {
                assert_eq!(provider_ids, ["claude"]);
            }
            _ => panic!("wrong variant"),
        }
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"addTokenAccount","target":{"kind":"provider","providerId":"codex"},"label":"work","token":"tok"}"#,
        )
        .expect("addTokenAccount");
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"openExternalUrl","target":{"kind":"app"},"url":"https://example.com"}"#,
        )
        .expect("openExternalUrl");
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"closeSettings","target":{"kind":"settings"}}"#,
        )
        .expect("closeSettings");
        let update: SurfaceAction = serde_json::from_str(
            r#"{"type":"updateSettings","target":{"kind":"settings"},"patch":{"theme":"dark"}}"#,
        )
        .expect("updateSettings");
        match update {
            SurfaceAction::UpdateSettings { .. } => {}
            _ => panic!("wrong variant"),
        }
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"registerGlobalShortcut","target":{"kind":"app"},"accelerator":"Ctrl+Shift+U"}"#,
        )
        .expect("registerGlobalShortcut");
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"unregisterGlobalShortcut","target":{"kind":"app"}}"#,
        )
        .expect("unregisterGlobalShortcut");
        let lang: SurfaceAction = serde_json::from_str(
            r#"{"type":"setUiLanguage","target":{"kind":"settings"},"language":"chinese"}"#,
        )
        .expect("setUiLanguage");
        match lang {
            SurfaceAction::SetUiLanguage { language, .. } => {
                assert_eq!(language, "chinese");
            }
            _ => panic!("wrong variant"),
        }
        let _: SurfaceAction = serde_json::from_str(
            r#"{"type":"playNotificationSound","target":{"kind":"settings"}}"#,
        )
        .expect("playNotificationSound");
    }
}
