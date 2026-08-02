use super::*;

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
/// Used by `PopOutPanel`'s "back to tray" action, which previously called
/// `set_surface_mode("trayPanel", ...)` on the shared window — now that the
/// flyout is its own window, that action opens it directly instead.  Same
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
    use tauri::Manager;

    let Some(window) = app.get_webview_window(crate::shell::flyout_window::FLYOUT_LABEL) else {
        return Ok(());
    };
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.mark_flyout_frontend_ready();
    if !guard.take_pending_flyout_reveal() {
        return Ok(());
    }
    drop(guard);
    window.show().map_err(|e| e.to_string())?;
    state
        .lock()
        .map_err(|e| e.to_string())?
        .mark_tray_panel_shown(std::time::Instant::now());
    window.set_focus().map_err(|e| e.to_string())?;
    Ok(())
}

/// Mark the prewarmed Settings frontend ready and reveal it only when an open
/// request arrived before the lazy Settings surface finished its first render.
#[tauri::command]
pub fn reveal_settings_window(
    app: tauri::AppHandle,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), String> {
    use tauri::{Emitter, Manager};

    let Some(window) = app.get_webview_window(crate::shell::settings_window::SETTINGS_LABEL) else {
        return Ok(());
    };
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard.mark_settings_frontend_ready();
    let Some(tab) = guard.take_pending_settings_reveal() else {
        return Ok(());
    };
    drop(guard);

    app.emit_to(
        crate::shell::settings_window::SETTINGS_LABEL,
        "settings-change-tab",
        tab,
    )
    .map_err(|e| e.to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    // This path only runs for a reveal that was armed while the window was
    // still hidden, so it is always a genuine hidden -> visible transition.
    app.emit_to(
        crate::shell::settings_window::SETTINGS_LABEL,
        crate::shell::settings_window::SETTINGS_REVEALED_EVENT,
        (),
    )
    .map_err(|e| e.to_string())?;
    Ok(())
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
    // reports the ordinary "not valid for mode" error — only a well-formed
    // `trayPanel` + `summary` request (the only target that maps to
    // `SurfaceMode::TrayPanel` — see `SurfaceTarget::mode`) reaches this
    // branch and is rejected here.
    if mode == SurfaceMode::TrayPanel {
        return Err(
            "set_surface_mode does not support 'trayPanel': the tray panel is a dedicated window \
             now — call open_flyout_window instead"
                .into(),
        );
    }

    Ok(target)
}

fn target_label(target: &SurfaceTarget) -> String {
    match target {
        SurfaceTarget::Summary => "summary".into(),
        SurfaceTarget::Dashboard => "dashboard".into(),
        SurfaceTarget::Provider { provider_id } => format!("provider:{provider_id}"),
        SurfaceTarget::Settings { tab } => format!("settings:{tab}"),
    }
}
