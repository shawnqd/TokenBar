//! Detached Settings window: opens Settings/About in a separate window
//! so the tray panel stays open.

use std::sync::Mutex;

use tauri::{Emitter, Manager, PhysicalPosition, WebviewUrl};

use crate::state::AppState;

pub const SETTINGS_LABEL: &str = "settings";
/// Fired when the window goes from hidden to visible, so the frontend can play
/// its entrance animation.
///
/// The window is prewarmed and reused rather than created per open, so the
/// React tree never remounts and a mount-time animation would run exactly once
/// per app launch. This event is the reveal signal the DOM otherwise has no way
/// to observe — a hidden window's webview does not reliably get a visibility
/// change from Chromium.
pub const SETTINGS_REVEALED_EVENT: &str = "settings-window-revealed";
/// Fired after the window is hidden, so the frontend can park its frame back at
/// zero opacity.
///
/// Without this the frame rests opaque while hidden, and the next `show()`
/// paints a fully drawn window for the frame or two it takes the reveal event
/// to cross the IPC boundary. The fade then starts from zero — so the window
/// appears, blinks out, and fades back in. That reads as a bug, which is what it
/// is. Parking the frame at zero while hidden means `show()` can only ever
/// reveal something already transparent.
pub const SETTINGS_HIDDEN_EVENT: &str = "settings-window-hidden";
// The frontend frame reserves a transparent gutter on every side to hold its
// drop shadow, so the window is larger than the card the user sees. Kept in
// step with `.settings-surface--full.settings-window-frame`'s padding —
// widening the gutter alone would shrink the usable area.
const SETTINGS_GUTTER: f64 = 24.0;
const SETTINGS_WIDTH: f64 = 912.0 + SETTINGS_GUTTER * 2.0;
const SETTINGS_HEIGHT: f64 = 580.0 + SETTINGS_GUTTER * 2.0;

/// Whether the detached Settings window is visibly open. The tray flyout uses
/// this to stay on screen as a live settings preview while focus moves between
/// these two companion surfaces.
pub fn is_visible(app: &tauri::AppHandle) -> bool {
    app.get_webview_window(SETTINGS_LABEL)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
}

fn build_hidden(app: &tauri::AppHandle, tab: &str) -> Result<(), String> {
    if app.get_webview_window(SETTINGS_LABEL).is_some() {
        return Ok(());
    }

    let url = WebviewUrl::App(format!("index.html?window=settings&tab={tab}").into());

    let builder = tauri::WebviewWindowBuilder::new(app, SETTINGS_LABEL, url)
        .title("CodexBar Settings")
        .inner_size(SETTINGS_WIDTH, SETTINGS_HEIGHT)
        .decorations(false)
        .shadow(false)
        .resizable(true)
        // Never expose WebView2's blank backing surface. The React Settings
        // tree calls `reveal_settings_window` after its lazy chunk and first
        // layout are ready.
        .visible(false)
        // Dynamically-built windows default to drag-drop ENABLED, which
        // intercepts the HTML5 draggable events the Providers sidebar's
        // drag-reorder relies on before React sees them.
        .disable_drag_drop_handler()
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

    // Keep WS_THICKFRAME for resizing while removing its square paint. The
    // frontend frame owns the radius, hairline and shadow.
    super::dwm::force_borderless_transparent_resizable(&win);

    // Tauri's `.center()` is unreliable for dynamically-built windows on
    // Windows, so center against the primary monitor explicitly.
    if let Ok(Some(monitor)) = win.primary_monitor() {
        let pos = monitor.position();
        let size = monitor.size();
        let scale = win.scale_factor().unwrap_or(1.0);
        let win_w = (SETTINGS_WIDTH * scale) as i32;
        let win_h = (SETTINGS_HEIGHT * scale) as i32;
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
        let was_hidden = !window.is_visible().unwrap_or(false);
        window.show().map_err(|e| e.to_string())?;
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
        window.hide().map_err(|e| e.to_string())?;
        // Emitted after the hide: the frame drops to zero opacity behind an
        // already-invisible window, so the reset itself is never seen.
        return app
            .emit_to(SETTINGS_LABEL, SETTINGS_HIDDEN_EVENT, ())
            .map_err(|e| e.to_string());
    }

    crate::shell::hide_to_tray_if_current(app, |mode| {
        mode == crate::surface::SurfaceMode::Settings
    })?;
    Ok(())
}
