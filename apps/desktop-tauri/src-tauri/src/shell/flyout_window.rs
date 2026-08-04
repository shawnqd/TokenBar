//! Detached "Open Tray Panel" window: a default-sized, resizable,
//! always-on-top,
//! tray-anchored panel that auto-hides on click-outside (blur-dismiss).
//!
//! Runs as an auxiliary Tauri window labeled `flyout`, independent of the
//! `main` window's surface state machine — it coexists with "Open Dashboard"
//! (`SurfaceMode::PopOut`, which stays on `main`) instead of being a
//! mutually-exclusive state of the same window.
//!
//! Structurally modeled on `crate::floatbar` (self-contained module owning
//! its window + a `handle_window_event` hook dispatched from `main.rs`
//! before the `main`-window-only handling); the window itself is built with
//! `settings_window.rs`'s builder recipe (async open, manual DWM dark-caption
//! pass, `WebviewUrl::App` with a `?window=` query marker).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl};

use crate::state::AppState;
use crate::surface::SurfaceMode;

pub const FLYOUT_LABEL: &str = "flyout";

fn remembered_size(props: &crate::surface::WindowProperties) -> (f64, f64) {
    let stored = crate::geometry_store::load_entry(FLYOUT_LABEL);
    let width = stored
        .and_then(|geometry| geometry.width)
        .map(|value| value as f64)
        .unwrap_or(props.width);
    let height = stored
        .and_then(|geometry| geometry.height)
        .map(|value| value as f64)
        .unwrap_or(props.height);
    let width = props.min_width.map_or(width, |min| width.max(min));
    let height = props.min_height.map_or(height, |min| height.max(min));
    let width = props.max_width.map_or(width, |max| width.min(max));
    let height = props.max_height.map_or(height, |max| height.min(max));
    (width, height)
}

fn remember_geometry(window: &tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) || window.is_minimized().unwrap_or(false) {
        return;
    }
    let Ok(position) = window.outer_position() else {
        return;
    };
    let Ok(size) = window.outer_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    crate::geometry_store::save_entry(
        FLYOUT_LABEL,
        crate::geometry_store::StoredGeometry {
            // The flyout is re-anchored on every open, so only its last size
            // is restored. Keeping the position is still useful for future
            // diagnostics and matches the shared geometry schema.
            x: position.x,
            y: position.y,
            width: Some((size.width as f64 / scale).round().max(1.0) as u32),
            height: Some((size.height as f64 / scale).round().max(1.0) as u32),
        },
    );
}

/// Same window used to close a same-click blur-dismiss/reopen race as the
/// pre-split tray panel handling (formerly `shell::transition::handle_tray_panel_click`,
/// removed once its only caller — the tray-icon left-click handler — was
/// retargeted to call this module directly).
const BLUR_DISMISS_CLICK_WINDOW: Duration = Duration::from_millis(250);
/// Grace period after showing the flyout during which a spurious Windows
/// blur (tray click focus race) is ignored — mirrors `main.rs`'s 500ms
/// `was_tray_panel_recently_shown` guard for the old shared window.
const RECENTLY_SHOWN_GRACE: Duration = Duration::from_millis(500);

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetAsyncKeyState(vkey: i32) -> i16;
}

/// Native edge-resizing starts in Windows before the WebView can report a
/// pointer gesture. The resize modal loop may briefly send `Focused(false)`
/// to the flyout; identify that specific case so blur-dismiss does not hide
/// the panel underneath the user's drag.
#[cfg(windows)]
fn native_resize_gesture(window: &tauri::Window) -> bool {
    const VK_LBUTTON: i32 = 0x01;
    if unsafe { GetAsyncKeyState(VK_LBUTTON) } >= 0 {
        return false;
    }

    let Ok(cursor) = window.cursor_position() else {
        return false;
    };
    let Ok(position) = window.outer_position() else {
        return false;
    };
    let Ok(size) = window.outer_size() else {
        return false;
    };

    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let edge = (8.0 * scale).round().max(6.0);
    let left = position.x as f64;
    let top = position.y as f64;
    let right = left + size.width as f64;
    let bottom = top + size.height as f64;
    let inside_x = cursor.x >= left && cursor.x <= right;
    let inside_y = cursor.y >= top && cursor.y <= bottom;
    let near_left = cursor.x >= left && cursor.x <= left + edge;
    let near_right = cursor.x >= right - edge && cursor.x <= right;
    let near_top = cursor.y >= top && cursor.y <= top + edge;
    let near_bottom = cursor.y >= bottom - edge && cursor.y <= bottom;

    (inside_y && (near_left || near_right)) || (inside_x && (near_top || near_bottom))
}

#[cfg(not(windows))]
fn native_resize_gesture(_window: &tauri::Window) -> bool {
    false
}

/// Whether the flyout window currently exists and is visible. Canonical
/// replacement for the pre-split `surface_machine.current() == TrayPanel`
/// check, now that the flyout is not a state of the shared machine.
pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(FLYOUT_LABEL)
        .is_some_and(|w| w.is_visible().unwrap_or(false))
}

/// Build (first open) or show + focus (subsequent opens) the flyout window at
/// `position`, if given, else the default tray-anchored position.
///
/// `WebviewWindowBuilder::build` deadlocks when called synchronously from a
/// Tauri command on Windows (see `commands/surface.rs::open_settings_window`
/// precedent) — callers must invoke this from an async context (an `async`
/// command, or `tauri::async_runtime::spawn`), never a sync command handler.
pub fn open_or_focus(app: &AppHandle, position: Option<(i32, i32)>) -> Result<(), String> {
    open_or_focus_inner(app, position, true)
}

fn open_or_focus_inner(
    app: &AppHandle,
    position: Option<(i32, i32)>,
    reveal_when_ready: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FLYOUT_LABEL) {
        if let Some((x, y)) = position {
            let _ = window.set_position(PhysicalPosition::new(x, y));
        } else {
            reanchor(app)?;
        }
        let frontend_ready = app
            .try_state::<Mutex<AppState>>()
            .and_then(|state| {
                state
                    .lock()
                    .ok()
                    .map(|guard| guard.is_flyout_frontend_ready())
            })
            .unwrap_or(false);
        if !frontend_ready {
            if reveal_when_ready {
                arm_reveal(app)?;
            }
            return Ok(());
        }
        // WebView2 can recreate the root HWND style while a prewarmed window
        // is being revealed. Reapply the borderless subclass at this boundary
        // so edge hit-testing and transparent corners remain native-owned.
        super::dwm::force_borderless_transparent_resizable(&window);
        window.show().map_err(|e| e.to_string())?;
        super::dwm::force_borderless_transparent_resizable(&window);
        window.set_focus().map_err(|e| e.to_string())?;
        if show_grace_starts_now(false) {
            mark_shown(app);
        }
        return Ok(());
    }

    // Derive window properties from `SurfaceMode::TrayPanel.window_properties()`
    // — the historical single source of truth for the flyout's shape (size,
    // resizability, always-on-top, taskbar visibility). The variant is kept
    // specifically so this builder (and the geometry-store key, and
    // `default_surface_position`'s positioning branch) have one place to read
    // from, rather than duplicating these values as independent constants
    // that could silently drift from `surface.rs`.
    let props = SurfaceMode::TrayPanel.window_properties();
    // Keep the reference size for the first open, then restore only a size the
    // user chose. The native window remains the sole owner of drag-resizing;
    // React must not fight it with a later setSize call.
    let (width, height) = remembered_size(&props);

    let url = WebviewUrl::App("index.html?window=flyout".into());

    let mut builder = tauri::WebviewWindowBuilder::new(app, FLYOUT_LABEL, url)
        .title("CodexBar")
        .inner_size(width, height)
        .decorations(props.decorations)
        .shadow(false)
        .resizable(props.resizable)
        .always_on_top(props.always_on_top)
        .skip_taskbar(props.skip_taskbar)
        // CRITICAL: dynamically-built windows default to drag-drop ENABLED,
        // which intercepts the HTML5 draggable events the provider grid's
        // drag-reorder (ProviderGrid.tsx) relies on — see `main`'s
        // `dragDropEnabled: false` in tauri.conf.json for why this must be
        // disabled explicitly on every window that hosts that grid.
        .disable_drag_drop_handler()
        .visible(false);
    if let (Some(min_w), Some(min_h)) = (props.min_width, props.min_height) {
        builder = builder.min_inner_size(min_w, min_h);
    }
    if let (Some(max_w), Some(max_h)) = (props.max_width, props.max_height) {
        builder = builder.max_inner_size(max_w, max_h);
    }

    // WebView2 only honors an alpha (transparent) background when the native
    // window is itself created transparent (see floatbar/window.rs for the
    // same fix) — without this the page's transparent areas around the
    // rounded `.menu-surface--tray` shell render as an opaque gray square.
    #[cfg(windows)]
    let builder = builder.transparent(true);

    let win = builder
        .background_color(tauri::utils::config::Color(0, 0, 0, 0))
        .build()
        .map_err(|e| e.to_string())?;

    // Strip the caption while retaining WS_THICKFRAME. This keeps the native
    // edge hit-tests for drag-resizing without painting a square frame over
    // the frontend's rounded transparent shell.
    super::dwm::force_borderless_transparent_resizable(&win);

    let target_position =
        position.or_else(|| super::position::default_surface_position(app, SurfaceMode::TrayPanel));
    if let Some((x, y)) = target_position {
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }

    // Left `.visible(false)` above — the frontend reveals the window itself
    // after its first layout pass, then `reveal_tray_panel_window` starts the
    // recently-shown blur grace at the real show/focus boundary.
    if show_grace_starts_now(true) {
        mark_shown(app);
    }
    if reveal_when_ready {
        arm_reveal(app)?;
    }
    Ok(())
}

/// Create and load the flyout WebView during application startup while
/// keeping it hidden. The first tray click can then show an already-rendered
/// surface instead of spending that click creating WebView2.
pub fn prewarm(app: &AppHandle) -> Result<(), String> {
    open_or_focus_inner(app, None, false)
}

fn show_grace_starts_now(first_build_hidden: bool) -> bool {
    !first_build_hidden
}

fn arm_reveal(app: &AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<Mutex<AppState>>()
        .ok_or_else(|| "app state unavailable".to_string())?;
    state.lock().map_err(|e| e.to_string())?.arm_flyout_reveal();
    Ok(())
}

/// Toggle the flyout: hide if open, open (or focus) otherwise. Consumes a
/// same-click blur-dismissal first (see `handle_window_event`'s
/// `Focused(false)` path) so a tray click that just blur-dismissed the
/// flyout cleanly closes it instead of instantly reopening.
///
/// Must be called from an async context — see [`open_or_focus`].
pub fn toggle_with_blur_consume(app: &AppHandle, position: Option<(i32, i32)>) {
    let consumed_blur_dismissal = {
        let st = app.state::<Mutex<AppState>>();
        st.lock()
            .unwrap()
            .take_recent_blur_dismissal(Instant::now(), BLUR_DISMISS_CLICK_WINDOW)
    };

    if consumed_blur_dismissal {
        return;
    }

    if is_open(app) {
        // Proof captures may still click the tray icon while the panel is
        // being brought back to the foreground. Treat that click as a
        // refocus, not as an explicit close; the proof harness has a separate
        // `hide-surface` command for intentional teardown.
        if crate::proof_harness::is_proof_mode(app) {
            tracing::debug!("flyout: proof mode kept panel visible on tray toggle");
            if let Some(window) = app.get_webview_window(FLYOUT_LABEL) {
                let _ = window.set_focus();
            }
            return;
        }
        let _ = hide(app);
    } else {
        let _ = open_or_focus(app, position);
    }
}

/// Hide (never close) the flyout window. Hiding — rather than closing —
/// keeps the window's WebView2 instance alive across opens, matching
/// `settings_window::dismiss`'s rationale: closing risks Tauri's
/// process/window lifecycle treating it as an app-relevant close.
pub fn hide(app: &AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FLYOUT_LABEL) {
        remember_geometry(&window);
        let state = app
            .try_state::<Mutex<AppState>>()
            .ok_or_else(|| "app state unavailable".to_string())?;
        state
            .lock()
            .map_err(|e| e.to_string())?
            .clear_flyout_reveal();
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn mark_shown(app: &AppHandle) {
    if let Some(st) = app.try_state::<Mutex<AppState>>()
        && let Ok(mut guard) = st.lock()
    {
        guard.mark_tray_panel_shown(Instant::now());
    }
}

/// Handle a `WindowEvent` targeting the flyout window. Returns `true` when
/// the event was for the flyout (and was handled), `false` otherwise so the
/// caller (`main.rs`'s single `on_window_event` dispatcher) can fall through
/// to its own `main`-window-only handling.
///
/// Applies the flyout's blur guards (proof-mode suppression / recently-shown
/// 500ms grace / gesture blur guard) before hiding on click-outside.
pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) -> bool {
    if window.label() != FLYOUT_LABEL {
        return false;
    }

    let app = window.app_handle();

    match event {
        tauri::WindowEvent::Focused(false) => {
            if crate::proof_harness::is_proof_mode(app) {
                tracing::debug!("flyout: proof mode suppressed blur-dismiss");
                return true;
            }
            if native_resize_gesture(window) {
                if let Some(st) = app.try_state::<Mutex<AppState>>() {
                    if let Ok(mut guard) = st.lock() {
                        guard.begin_gesture_blur_guard(Instant::now());
                    }
                }
                tracing::debug!("flyout: native resize kept blur-dismiss armed");
                return true;
            }
            // Settings is a companion window, not an outside click: keep the
            // flyout visible so changes can be reviewed live side by side.
            if crate::shell::settings_window::is_visible(app) {
                return true;
            }
            let Some(st) = app.try_state::<Mutex<AppState>>() else {
                return true;
            };
            {
                let guard = st.lock().unwrap();
                if guard.was_tray_panel_recently_shown(Instant::now(), RECENTLY_SHOWN_GRACE) {
                    return true;
                }
                if guard.is_gesture_blur_guard_active(Instant::now()) {
                    return true;
                }
            }
            if hide(app).is_ok() {
                st.lock().unwrap().mark_blur_dismissed(Instant::now());
            }
            true
        }
        tauri::WindowEvent::Focused(true) => {
            if let Some(st) = app.try_state::<Mutex<AppState>>() {
                st.lock()
                    .unwrap()
                    .clear_gesture_guard_on_refocus(Instant::now());
            }
            true
        }
        // Position is always recomputed from the tray anchor when it opens;
        // the last user-chosen size is persisted when the panel hides.
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => true,
        tauri::WindowEvent::CloseRequested { api, .. } => {
            if crate::proof_harness::is_proof_mode(app) {
                tracing::debug!("flyout: proof mode suppressed close request");
                api.prevent_close();
                return true;
            }
            // Hide-not-close, matching Settings/FloatBar lifecycle handling —
            // the flyout must survive a native close (Alt+F4-equivalent from
            // a screen reader, etc.) so it can be reopened without rebuilding
            // the WebView2 instance.
            api.prevent_close();
            let _ = hide(app);
            true
        }
        _ => true,
    }
}

/// Reposition the flyout so its bottom-right corner stays anchored to
/// the system-tray area. Canonical anchor-math implementation for the
/// flyout window; the `reanchor_tray_panel` Tauri command
/// (`commands/system.rs`) is a thin retarget onto this function.
pub fn reanchor(app: &AppHandle) -> Result<(), String> {
    use crate::window_positioner::{PanelSize, Rect};

    let window = app
        .get_webview_window(FLYOUT_LABEL)
        .ok_or_else(|| "flyout window unavailable".to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);

    let outer = window.outer_size().map_err(|e| e.to_string())?;
    let panel_size = PanelSize {
        width: (outer.width as f64 / scale).round() as u32,
        height: (outer.height as f64 / scale).round() as u32,
    };

    let anchor = app
        .try_state::<Mutex<AppState>>()
        .and_then(|state| state.lock().ok()?.tray_anchor);
    let monitors = window.available_monitors().unwrap_or_default();
    let monitor = anchor
        .and_then(|anchor| crate::shell::geometry::monitor_for_anchor(&monitors, anchor))
        .cloned()
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten())
        .ok_or_else(|| "no monitor".to_string())?;

    let work_area = crate::shell::geometry::monitor_work_area_rect(&monitor);
    let monitor_bounds = Rect {
        x: monitor.position().x,
        y: monitor.position().y,
        width: monitor.size().width,
        height: monitor.size().height,
    };

    let (x, y) = {
        if let Some(a) = anchor {
            crate::window_positioner::calculate_panel_position(
                &Rect {
                    x: a.x,
                    y: a.y,
                    width: a.width,
                    height: a.height,
                },
                &monitor_bounds,
                &work_area,
                &panel_size,
                scale,
            )
        } else {
            // No real click anchor yet: infer one from the taskbar side
            // (handles left/right/top-docked taskbars, not just bottom-right).
            crate::shell::inferred_tray_panel_position_for_monitor_size(&monitor, &panel_size)
        }
    };

    // Pass physical coordinates directly — tao converts PhysicalPosition to
    // OS logical internally by dividing by the window's scale factor.
    let pos = tauri::PhysicalPosition::new(x, y);
    tracing::debug!(
        "flyout_window::reanchor: panel={}x{} => ({},{})",
        panel_size.width,
        panel_size.height,
        pos.x,
        pos.y
    );
    let _ = window.set_position(pos);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flyout_label_is_stable() {
        assert_eq!(FLYOUT_LABEL, "flyout");
    }

    #[test]
    fn tray_panel_window_properties_still_the_single_source_for_flyout_shape() {
        // `open_or_focus`'s builder reads size/decorations/resizable/
        // always_on_top/skip_taskbar from
        // `SurfaceMode::TrayPanel.window_properties()` directly (not
        // independent duplicated constants) — this pins down the values that
        // relationship depends on, so a change to `surface.rs` shows up here
        // instead of silently drifting from what the flyout actually builds.
        let props = SurfaceMode::TrayPanel.window_properties();
        assert_eq!(props.width, 328.0);
        assert_eq!(props.height, 776.0);
        assert_eq!(props.min_width, Some(300.0));
        assert_eq!(props.min_height, Some(360.0));
        assert_eq!(props.max_width, None);
        assert_eq!(props.max_height, None);
        assert!(props.resizable);
        assert!(props.always_on_top);
        assert!(props.skip_taskbar);
        assert!(!props.decorations);
    }

    #[test]
    fn first_hidden_build_does_not_start_show_grace() {
        assert!(show_grace_starts_now(false));
        assert!(!show_grace_starts_now(true));
    }
}
