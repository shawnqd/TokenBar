//! Detached Settings window: opens Settings/About in a separate window
//! so the tray panel stays open.

use std::sync::Mutex;

use tauri::{Emitter, Manager, PhysicalPosition, WebviewUrl};

use crate::state::AppState;

pub const SETTINGS_LABEL: &str = "settings";
/// Fired when the window goes from hidden to visible. The frontend uses this
/// only for a small content motion; native visibility remains authoritative.
///
/// The window is prewarmed and reused rather than created per open, so the
/// React tree never remounts and a mount-time animation would run exactly once
/// per app launch. This event is the reveal signal the DOM otherwise has no way
/// to observe — a hidden window's webview does not reliably get a visibility
/// change from Chromium.
pub const SETTINGS_REVEALED_EVENT: &str = "settings-window-revealed";
// The frontend frame reserves a transparent gutter on every side to hold its
// drop shadow, so the window is larger than the card the user sees. Kept in
// step with `.settings-surface--full.settings-window-frame`'s padding —
// widening the gutter alone would shrink the usable area.
const SETTINGS_GUTTER: f64 = 12.0;
const SETTINGS_WIDTH: f64 = 1120.0 + SETTINGS_GUTTER * 2.0;
const SETTINGS_HEIGHT: f64 = 780.0 + SETTINGS_GUTTER * 2.0;
// Keep the window usable when the user makes it smaller than the reference
// design. The native frame owns this constraint; the React surface must be
// allowed to reflow inside it instead of resetting the size on every tab.
const SETTINGS_MIN_WIDTH: f64 = 720.0 + SETTINGS_GUTTER * 2.0;
const SETTINGS_MIN_HEIGHT: f64 = 480.0 + SETTINGS_GUTTER * 2.0;

fn apply_design_size_if_stale(window: &tauri::WebviewWindow) {
    let Ok(size) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let logical_w = size.width as f64 / scale;
    let logical_h = size.height as f64 / scale;
    if (logical_w - SETTINGS_WIDTH).abs() < 24.0 && (logical_h - SETTINGS_HEIGHT).abs() < 24.0 {
        return;
    }
    let _ = window.set_size(tauri::LogicalSize::new(SETTINGS_WIDTH, SETTINGS_HEIGHT));
}

fn remembered_size() -> (f64, f64) {
    // HTML card is 1120×780. Old remembered sizes (912-era or a stretched
    // proof-mode window) made the field column too narrow and wrapped labels
    // into a vertical strip. Only keep a stored size if it is still the V5
    // card (plus gutter), otherwise snap back to the design size.
    let stored = crate::geometry_store::load_entry(SETTINGS_LABEL);
    let width = stored
        .and_then(|geometry| geometry.width)
        .map(|value| value as f64)
        .filter(|value| (*value - SETTINGS_WIDTH).abs() < 24.0)
        .unwrap_or(SETTINGS_WIDTH)
        .max(SETTINGS_MIN_WIDTH);
    let height = stored
        .and_then(|geometry| geometry.height)
        .map(|value| value as f64)
        .filter(|value| (*value - SETTINGS_HEIGHT).abs() < 24.0)
        .unwrap_or(SETTINGS_HEIGHT)
        .max(SETTINGS_MIN_HEIGHT);
    (width, height)
}

/// A stale geometry entry can outlive a monitor, DPI, or taskbar change. Keep
/// the first hidden build inside the current monitor before restoring it; the
/// user can still resize freely within that monitor afterwards. Without this
/// guard a too-tall Settings window is clipped and makes the Providers tab
/// look like it is stretching the whole application.
fn clamp_size_to_monitor(window: &tauri::WebviewWindow, width: f64, height: f64) -> (f64, f64) {
    let Ok(Some(monitor)) = window.primary_monitor() else {
        return (width, height);
    };
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let monitor_width = monitor.size().width as f64 / scale;
    let monitor_height = monitor.size().height as f64 / scale;
    let max_width = (monitor_width - SETTINGS_GUTTER * 2.0).max(SETTINGS_MIN_WIDTH);
    // Leave a small taskbar/titlebar reserve so a restored window is never
    // reopened underneath the taskbar or beyond the bottom edge.
    let max_height = (monitor_height - SETTINGS_GUTTER * 2.0 - 48.0).max(SETTINGS_MIN_HEIGHT);
    (width.min(max_width), height.min(max_height))
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
        SETTINGS_LABEL,
        crate::geometry_store::StoredGeometry {
            // Positions are kept in physical pixels, matching the existing
            // detached-window geometry entries. Width/height are logical so
            // a HiDPI monitor does not reopen the window at 125%/150% size.
            x: position.x,
            y: position.y,
            width: Some((size.width as f64 / scale).round().max(1.0) as u32),
            height: Some((size.height as f64 / scale).round().max(1.0) as u32),
        },
    );
}

/// WebView2 can restore the top-level HWND style while the prewarmed window
/// is being initialized or revealed. Keep the native shell decoration-free at
/// every lifecycle boundary so the frontend title bar is the only title bar.
fn normalize_window_chrome(window: &tauri::WebviewWindow) {
    let _ = window.set_decorations(false);
    super::dwm::force_borderless_transparent_resizable(window);
    super::dwm::attach_settings_taskbar_button(window);
}

/// Whether the detached Settings window is visibly open. The tray flyout uses
/// this to stay on screen as a live settings preview while focus moves between
/// these two companion surfaces.
pub fn is_visible(app: &tauri::AppHandle) -> bool {
    app.get_webview_window(SETTINGS_LABEL)
        .is_some_and(|window| {
            window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
        })
}

fn build_hidden(app: &tauri::AppHandle, tab: &str) -> Result<(), String> {
    if app.get_webview_window(SETTINGS_LABEL).is_some() {
        return Ok(());
    }

    let url = WebviewUrl::App(format!("index.html?window=settings&tab={tab}").into());

    let (width, height) = remembered_size();
    let builder = tauri::WebviewWindowBuilder::new(app, SETTINGS_LABEL, url)
        .title("CodexBar Settings")
        .inner_size(width, height)
        .min_inner_size(SETTINGS_MIN_WIDTH, SETTINGS_MIN_HEIGHT)
        .decorations(false)
        .shadow(false)
        .resizable(true)
        // Dynamically-built windows default to drag-drop ENABLED, which
        // intercepts the HTML5 draggable events the Providers sidebar's
        // drag-reorder relies on before React sees them.
        .disable_drag_drop_handler()
        // Never expose WebView2's blank backing surface. The React Settings
        // tree calls `reveal_settings_window` after its lazy chunk and first
        // layout are ready.
        .visible(false);

    // Match the tray flyout's composition: WebView2 supplies transparent
    // pixels around the frontend-owned rounded frame instead of filling those
    // corners with an opaque rectangle.
    #[cfg(windows)]
    let builder = builder.transparent(true);

    let win = builder
        .background_color(tauri::utils::config::Color(0, 0, 0, 0))
        .build()
        .map_err(|e| e.to_string())?;

    let (safe_width, safe_height) = clamp_size_to_monitor(&win, width, height);
    if (safe_width - width).abs() > f64::EPSILON || (safe_height - height).abs() > f64::EPSILON {
        let _ = win.set_size(tauri::LogicalSize::new(safe_width, safe_height));
    }

    // Keep WS_THICKFRAME for resizing while removing its square paint. The
    // frontend frame owns the radius, hairline and shadow.
    normalize_window_chrome(&win);

    // The Settings window is a utility dialog: it always opens centered on
    // the primary monitor. Its SIZE is still remembered (and clamped to the
    // monitor), but its position is not — a stale geometry entry must not pin
    // the window off-center or off-screen forever. Within one app session a
    // window the user moved keeps its spot: `build_hidden` early-returns when
    // the window already exists, so only a fresh creation centers.
    // Tauri's `.center()` is unreliable for dynamically-built windows on
    // Windows, so this uses explicit native coordinates.
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let pos = monitor.position();
        let size = monitor.size();
        let scale = win.scale_factor().unwrap_or(1.0);
        let win_w = (width * scale) as i32;
        let win_h = (height * scale) as i32;
        let x = pos.x + (size.width as i32 - win_w) / 2;
        let y = pos.y + (size.height as i32 - win_h) / 2;
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }

    Ok(())
}

/// Create the Settings WebView at application startup without showing it.
/// This lets WebView2, bootstrap data and the lazy Settings bundle settle
/// before the user asks for the window.
pub fn prewarm(app: &tauri::AppHandle) -> Result<(), String> {
    build_hidden(app, "general")
}

/// Open the detached Settings window, or focus it if already open.
///
/// A ready, prewarmed window is shown immediately. If a click arrives during
/// cold startup, the requested tab is retained and the frontend-ready
/// handshake reveals the window after the first complete Settings render.
pub fn open_or_focus(app: &tauri::AppHandle, tab: &str) -> Result<(), String> {
    build_hidden(app, tab)?;
    let window = app
        .get_webview_window(SETTINGS_LABEL)
        .ok_or_else(|| "settings window unavailable after creation".to_string())?;

    let state = app
        .try_state::<Mutex<AppState>>()
        .ok_or_else(|| "app state unavailable".to_string())?;
    let frontend_ready = state
        .lock()
        .map_err(|e| e.to_string())?
        .is_settings_frontend_ready();

    if frontend_ready {
        app.emit_to(SETTINGS_LABEL, "settings-change-tab", tab)
            .map_err(|e| e.to_string())?;
        // Only a window coming back from hidden gets the entrance animation.
        // Clicking the tray while Settings is already up is a focus request, and
        // replaying a fade over a window the user is reading is noise, not
        // polish — so the visibility is sampled BEFORE `show()` makes it true.
        let minimized = window.is_minimized().unwrap_or(false);
        let was_hidden = minimized || !window.is_visible().unwrap_or(false);
        if minimized {
            // Native SC_RESTORE: zoom out of the taskbar button and bounce
            // the icon. Do not FRAMECHANGED / strip chrome here — that aborts
            // DWM's restore animation. WM_SIZE reapplies borderless after.
            super::dwm::restore_settings_from_taskbar(&window);
            window.show().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())?;
            return Ok(());
        }
        apply_design_size_if_stale(&window);
        let _ = window.set_min_size(Some(tauri::LogicalSize::new(
            SETTINGS_MIN_WIDTH,
            SETTINGS_MIN_HEIGHT,
        )));
        // Reapply after the prewarmed WebView is about to become visible. The
        // WebView2 root can replace the HWND style during initialization;
        // without this pass the window looks borderless but loses native edge
        // hit-testing and cannot be resized reliably.
        normalize_window_chrome(&window);
        window.show().map_err(|e| e.to_string())?;
        normalize_window_chrome(&window);
        window.set_focus().map_err(|e| e.to_string())?;
        if was_hidden {
            app.emit_to(SETTINGS_LABEL, SETTINGS_REVEALED_EVENT, ())
                .map_err(|e| e.to_string())?;
        }
    } else {
        state
            .lock()
            .map_err(|e| e.to_string())?
            .arm_settings_reveal(tab);
    }

    Ok(())
}

/// Dismiss Settings without exiting CodexBar.
///
/// The detached Settings window is hidden instead of closed so its prewarmed
/// WebView remains ready for the next open.
pub fn dismiss(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    if window.label() == SETTINGS_LABEL {
        remember_geometry(window);
        // Zoom the caption toward the taskbar, then hide. Minimize was
        // leaving Settings iconic so the close button could not finish.
        super::dwm::play_settings_close_zoom(window);
        if window.is_minimized().unwrap_or(false) {
            let _ = window.unminimize();
        }
        window.hide().map_err(|e| e.to_string())?;
        return Ok(());
    }

    crate::shell::hide_to_tray_if_current(app, |mode| {
        mode == crate::surface::SurfaceMode::Settings
    })?;
    Ok(())
}
