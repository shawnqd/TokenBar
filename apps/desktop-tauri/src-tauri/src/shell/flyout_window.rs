//! The detached tray panel window.
//!
//! This module is deliberately the only owner of the flyout window lifecycle.
//! React owns the provider cards; this module owns native visibility,
//! anchoring, resize persistence, outside-click dismissal and the close
//! animation.  In particular, `Focused(false)` is never a hide command:
//! WebView2 can emit it while focus moves between controls in the same panel.
//! Outside clicks are classified by the global mouse hook against the native
//! window rectangle and then handled on the Tauri main thread.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl};

use crate::state::AppState;
use crate::surface::SurfaceMode;

pub const FLYOUT_LABEL: &str = "flyout";
pub const TRAY_PANEL_REVEALED_EVENT: &str = "tray-panel-revealed";
pub const TRAY_PANEL_CLOSING_EVENT: &str = "tray-panel-closing";
pub const TRAY_PANEL_HIDDEN_EVENT: &str = "tray-panel-hidden";

const CLOSE_ANIMATION_DURATION: Duration = Duration::from_millis(180);
const STATE_HIDDEN: u8 = 0;
const STATE_SHOWING: u8 = 1;
const STATE_VISIBLE: u8 = 2;
const STATE_CLOSING: u8 = 3;

static FLYOUT_STATE: AtomicU8 = AtomicU8::new(STATE_HIDDEN);
static CLOSE_GENERATION: AtomicU64 = AtomicU64::new(0);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

fn set_state(state: u8) {
    FLYOUT_STATE.store(state, Ordering::SeqCst);
}

fn state_is(state: u8) -> bool {
    FLYOUT_STATE.load(Ordering::SeqCst) == state
}

fn cancel_pending_close() -> bool {
    if !state_is(STATE_CLOSING) {
        return false;
    }
    CLOSE_GENERATION.fetch_add(1, Ordering::SeqCst);
    set_state(STATE_VISIBLE);
    true
}

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
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    crate::geometry_store::save_entry(
        FLYOUT_LABEL,
        crate::geometry_store::StoredGeometry {
            x: position.x,
            y: position.y,
            width: Some((size.width as f64 / scale).round().max(1.0) as u32),
            height: Some((size.height as f64 / scale).round().max(1.0) as u32),
        },
    );
}

/// A physical-pixel rectangle.  Win32 mouse-hook coordinates and Tauri's
/// `outer_position`/`outer_size` use the same screen coordinate space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl ScreenRect {
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x
            && x < self.x.saturating_add(self.width)
            && y >= self.y
            && y < self.y.saturating_add(self.height)
    }
}

/// The pure part of the outside-click decision. Keeping this independent of
/// Tauri makes the close boundary testable without starting WebView2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickOutsideContext {
    pub flyout_rect: ScreenRect,
    pub settings_visible: bool,
    pub proof_mode: bool,
    pub native_menu_tracking: bool,
}

pub fn should_dismiss_for_click(ctx: &ClickOutsideContext, x: i32, y: i32) -> bool {
    !ctx.proof_mode
        && !ctx.settings_visible
        && !ctx.native_menu_tracking
        && !ctx.flyout_rect.contains(x, y)
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn SetWindowsHookExW(
        id_hook: i32,
        callback: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        module: isize,
        thread_id: u32,
    ) -> isize;
    fn CallNextHookEx(hook: isize, code: i32, wparam: usize, lparam: isize) -> isize;
    fn GetAncestor(hwnd: isize, flags: u32) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut WinRect) -> i32;
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(module_name: *const u16) -> isize;
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct Msg {
    hwnd: isize,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt_x: i32,
    pt_y: i32,
    private: u32,
}

#[cfg(windows)]
#[repr(C)]
struct MsllPoint {
    x: i32,
    y: i32,
}

#[cfg(windows)]
#[repr(C)]
struct MsllHookStruct {
    pt: MsllPoint,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(windows)]
const WH_MOUSE_LL: i32 = 14;
#[cfg(windows)]
const WM_LBUTTONDOWN: usize = 0x0201;
#[cfg(windows)]
const WM_RBUTTONDOWN: usize = 0x0204;

#[cfg(windows)]
static WATCHED_APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

#[cfg(windows)]
mod published {
    use std::sync::atomic::{AtomicBool, Ordering};

    pub(super) static VISIBLE: AtomicBool = AtomicBool::new(false);

    pub(super) fn set_hidden() {
        VISIBLE.store(false, Ordering::Release);
    }

    pub(super) fn set_visible() {
        VISIBLE.store(true, Ordering::Release);
    }

    pub(super) fn visible() -> bool {
        VISIBLE.load(Ordering::Acquire)
    }
}

#[cfg(not(windows))]
mod published {
    pub(super) fn set_hidden() {}
    pub(super) fn set_visible() {}
}

pub fn publish_flyout_geometry(app: &AppHandle) {
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        published::set_hidden();
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        published::set_hidden();
        return;
    }
    #[cfg(windows)]
    {
        let Some(rect) = actual_native_rect(&window) else {
            published::set_hidden();
            return;
        };
        tracing::debug!(?rect, "flyout: published native geometry");
        published::set_visible();
    }
    #[cfg(not(windows))]
    {
        published::set_visible();
    }
}

/// Read the top-level native window rectangle, rather than relying on a
/// cached Tauri position. WebView2 and DPI changes can move the outer HWND
/// without immediately updating `outer_position`; outside-click decisions
/// must use the rectangle Windows is actually hit-testing.
#[cfg(windows)]
fn actual_native_rect(window: &tauri::WebviewWindow) -> Option<ScreenRect> {
    use raw_window_handle::HasWindowHandle;

    let handle = window.window_handle().ok()?;
    let raw_window_handle::RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return None;
    };
    const GA_ROOT: u32 = 2;
    let inner = handle.hwnd.get();
    let hwnd = unsafe { GetAncestor(inner, GA_ROOT) };
    let hwnd = if hwnd != 0 { hwnd } else { inner };
    let mut rect = WinRect::default();
    if unsafe { GetWindowRect(hwnd, &raw mut rect) } == 0 {
        return None;
    }
    Some(ScreenRect {
        x: rect.left,
        y: rect.top,
        width: rect.right.saturating_sub(rect.left),
        height: rect.bottom.saturating_sub(rect.top),
    })
}

#[cfg(windows)]
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    if code >= 0 && matches!(wparam, WM_LBUTTONDOWN | WM_RBUTTONDOWN) {
        let info = unsafe { &*(lparam as *const MsllHookStruct) };
        let (x, y) = (info.pt.x, info.pt.y);
        // The hook only decides whether to post a check. It never calls a
        // Tauri API or hides a window, so the desktop input path stays fast.
        if published::visible()
            && let Some(app) = WATCHED_APP.get()
        {
            // Do not classify the click in the hook thread. The published
            // rectangle is only a hint and can be stale after a move/resize.
            // Always ask the main thread to compare against the current
            // top-level HWND rectangle instead.
            tracing::debug!(x, y, "flyout: global mouse button queued");
            let posted = app.clone();
            let _ = app.run_on_main_thread(move || close_if_outside(&posted, x, y));
        }
    }
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

#[cfg(windows)]
fn install_click_outside_watcher(app: &AppHandle) {
    let _ = WATCHED_APP.set(app.clone());
    if HOOK_INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    let result = std::thread::Builder::new()
        .name("codexbar-mouse-hook".into())
        .spawn(|| {
            let module = unsafe { GetModuleHandleW(std::ptr::null()) };
            let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), module, 0) };
            if hook == 0 {
                HOOK_INSTALLED.store(false, Ordering::Release);
                tracing::warn!("flyout: failed to install outside-click hook");
                return;
            }
            tracing::info!("flyout: outside-click hook installed");
            let mut msg = Msg::default();
            while unsafe { GetMessageW(&raw mut msg, 0, 0, 0) } > 0 {
                unsafe {
                    TranslateMessage(&raw const msg);
                    DispatchMessageW(&raw const msg);
                }
            }
        });
    if let Err(error) = result {
        HOOK_INSTALLED.store(false, Ordering::Release);
        tracing::warn!("flyout: failed to start outside-click hook thread: {error}");
    }
}

#[cfg(not(windows))]
fn install_click_outside_watcher(_app: &AppHandle) {}

#[cfg(windows)]
fn native_menu_is_tracking() -> bool {
    crate::taskbar_menu::is_open()
}

#[cfg(not(windows))]
fn native_menu_is_tracking() -> bool {
    false
}

fn close_if_outside(app: &AppHandle, x: i32, y: i32) {
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let rect = {
        #[cfg(windows)]
        {
            actual_native_rect(&window)
        }
        #[cfg(not(windows))]
        {
            window
                .outer_position()
                .ok()
                .zip(window.outer_size().ok())
                .map(|(position, size)| ScreenRect {
                    x: position.x,
                    y: position.y,
                    width: size.width as i32,
                    height: size.height as i32,
                })
        }
    };
    let Some(flyout_rect) = rect else {
        return;
    };
    let context = ClickOutsideContext {
        flyout_rect,
        settings_visible: crate::shell::settings_window::is_visible(app),
        proof_mode: crate::proof_harness::is_proof_mode(app),
        native_menu_tracking: native_menu_is_tracking(),
    };
    tracing::debug!(x, y, ?context.flyout_rect, "flyout: main-thread outside check");
    if should_dismiss_for_click(&context, x, y) {
        tracing::debug!(x, y, "flyout: outside click requested close");
        let _ = request_close(app);
    }
}

/// Whether the detached window is currently visible.  Closing remains open
/// until the reverse animation has completed so a second tray click can
/// cancel that animation instead of opening a second window.
pub fn is_open(app: &AppHandle) -> bool {
    app.get_webview_window(FLYOUT_LABEL)
        .is_some_and(|window| window.is_visible().unwrap_or(false))
}

fn show_window(app: &AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let was_visible = window.is_visible().unwrap_or(false);
    let was_closing = cancel_pending_close();
    set_state(STATE_SHOWING);
    super::dwm::force_borderless_transparent_resizable(window);
    window.show().map_err(|error| error.to_string())?;
    super::dwm::force_borderless_transparent_resizable(window);
    window.set_focus().map_err(|error| error.to_string())?;
    set_state(STATE_VISIBLE);
    publish_flyout_geometry(app);
    if !was_visible || was_closing {
        app.emit_to(FLYOUT_LABEL, TRAY_PANEL_REVEALED_EVENT, ())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn arm_reveal(app: &AppHandle) -> Result<(), String> {
    let state = app
        .try_state::<Mutex<AppState>>()
        .ok_or_else(|| "app state unavailable".to_string())?;
    state
        .lock()
        .map_err(|error| error.to_string())?
        .arm_flyout_reveal();
    Ok(())
}

fn frontend_ready(app: &AppHandle) -> bool {
    app.try_state::<Mutex<AppState>>()
        .and_then(|state| {
            state
                .lock()
                .ok()
                .map(|guard| guard.is_flyout_frontend_ready())
        })
        .unwrap_or(false)
}

/// Reveal a prewarmed window after the React shell has reported that it is
/// mounted. `commands::reveal_tray_panel_window` is intentionally a thin
/// bridge to this function so showing/focusing has one native owner.
pub fn reveal_ready(app: &AppHandle) -> Result<(), String> {
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        return Ok(());
    };
    show_window(app, &window)
}

pub fn open_or_focus(app: &AppHandle, position: Option<(i32, i32)>) -> Result<(), String> {
    open_or_focus_inner(app, position, true)
}

fn open_or_focus_inner(
    app: &AppHandle,
    position: Option<(i32, i32)>,
    reveal_when_ready: bool,
) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(FLYOUT_LABEL) {
        cancel_pending_close();
        if let Some((x, y)) = position {
            let _ = window.set_position(PhysicalPosition::new(x, y));
        } else if !window.is_visible().unwrap_or(false) {
            reanchor(app)?;
        }
        if !frontend_ready(app) {
            if reveal_when_ready {
                arm_reveal(app)?;
            }
            return Ok(());
        }
        return show_window(app, &window);
    }

    let props = SurfaceMode::TrayPanel.window_properties();
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
        .disable_drag_drop_handler()
        .visible(false);
    if let (Some(min_w), Some(min_h)) = (props.min_width, props.min_height) {
        builder = builder.min_inner_size(min_w, min_h);
    }
    if let (Some(max_w), Some(max_h)) = (props.max_width, props.max_height) {
        builder = builder.max_inner_size(max_w, max_h);
    }
    #[cfg(windows)]
    let builder = builder.transparent(true);
    let window = builder
        .background_color(tauri::utils::config::Color(0, 0, 0, 0))
        .build()
        .map_err(|error| error.to_string())?;
    super::dwm::force_borderless_transparent_resizable(&window);
    let target =
        position.or_else(|| super::position::default_surface_position(app, SurfaceMode::TrayPanel));
    if let Some((x, y)) = target {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    set_state(STATE_HIDDEN);
    if reveal_when_ready {
        arm_reveal(app)?;
    }
    Ok(())
}

pub fn prewarm(app: &AppHandle) -> Result<(), String> {
    install_click_outside_watcher(app);
    open_or_focus_inner(app, None, false)
}

/// Toggle only has two meanings: tray click while visible means close, and
/// tray click while hidden means open.  It no longer consumes a blur marker or
/// guesses which event won a same-click race.
pub fn toggle_with_blur_consume(app: &AppHandle, position: Option<(i32, i32)>) {
    if state_is(STATE_CLOSING) {
        let _ = open_or_focus(app, position);
        return;
    }
    if is_open(app) {
        tracing::debug!("flyout: tray toggle requested close");
        let _ = request_close(app);
    } else {
        let _ = open_or_focus(app, position);
    }
}

/// Start the single idempotent close transition.  All close sources use this
/// function: outside click, Escape, tray toggle and native close request.
pub fn request_close(app: &AppHandle) -> Result<(), String> {
    tracing::debug!("flyout: request_close entered");
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        set_state(STATE_HIDDEN);
        return Ok(());
    };
    if !window.is_visible().unwrap_or(false) || state_is(STATE_CLOSING) {
        return Ok(());
    }
    set_state(STATE_CLOSING);
    let generation = CLOSE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    app.emit_to(FLYOUT_LABEL, TRAY_PANEL_CLOSING_EVENT, ())
        .map_err(|error| {
            set_state(STATE_VISIBLE);
            error.to_string()
        })?;

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CLOSE_ANIMATION_DURATION).await;
        if !state_is(STATE_CLOSING) || CLOSE_GENERATION.load(Ordering::SeqCst) != generation {
            return;
        }
        let main_handle = handle.clone();
        if handle
            .run_on_main_thread(move || {
                if !state_is(STATE_CLOSING) || CLOSE_GENERATION.load(Ordering::SeqCst) != generation
                {
                    return;
                }
                let Some(window) = main_handle.get_webview_window(FLYOUT_LABEL) else {
                    set_state(STATE_HIDDEN);
                    return;
                };
                remember_geometry(&window);
                if window.hide().is_ok() {
                    set_state(STATE_HIDDEN);
                    published::set_hidden();
                    let _ = main_handle.emit_to(FLYOUT_LABEL, TRAY_PANEL_HIDDEN_EVENT, ());
                } else {
                    set_state(STATE_VISIBLE);
                    let _ = main_handle.emit_to(FLYOUT_LABEL, TRAY_PANEL_REVEALED_EVENT, ());
                    publish_flyout_geometry(&main_handle);
                }
            })
            .is_err()
        {
            // If the UI thread is already shutting down, do not leave the
            // lifecycle latch in CLOSING forever.  A later tray click must be
            // able to reopen the panel instead of being swallowed by a stale
            // transition.
            set_state(STATE_VISIBLE);
        }
    });
    Ok(())
}

/// Backwards-compatible command name.  The implementation is now the same
/// explicit close transition used by every other path.
pub fn hide(app: &AppHandle) -> Result<(), String> {
    request_close(app)
}

pub fn handle_window_event(window: &tauri::Window, event: &tauri::WindowEvent) -> bool {
    if window.label() != FLYOUT_LABEL {
        return false;
    }
    let app = window.app_handle();
    match event {
        // Focus loss is intentionally informational only. The global click
        // hook handles actual outside clicks; ignoring blur prevents provider
        // and All-button clicks from disappearing the panel.
        tauri::WindowEvent::Focused(false) => true,
        tauri::WindowEvent::Focused(true) => {
            cancel_pending_close();
            true
        }
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
            publish_flyout_geometry(app);
            true
        }
        tauri::WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            if !crate::proof_harness::is_proof_mode(app) {
                let _ = request_close(app);
            }
            true
        }
        _ => true,
    }
}

/// Reposition the panel so its bottom-right corner stays on the tray anchor.
pub fn reanchor(app: &AppHandle) -> Result<(), String> {
    use crate::window_positioner::{PanelSize, Rect};

    let window = app
        .get_webview_window(FLYOUT_LABEL)
        .ok_or_else(|| "flyout window unavailable".to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0).max(1.0);
    let outer = window.outer_size().map_err(|error| error.to_string())?;
    let panel_size = PanelSize {
        width: (outer.width as f64 / scale).round() as u32,
        height: (outer.height as f64 / scale).round() as u32,
    };
    let anchor = app
        .try_state::<Mutex<AppState>>()
        .and_then(|state| state.lock().ok()?.tray_anchor);
    let monitors = window.available_monitors().unwrap_or_default();
    let monitor = anchor
        .and_then(|value| crate::shell::geometry::monitor_for_anchor(&monitors, value))
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
    let (x, y) = if let Some(anchor) = anchor {
        crate::window_positioner::calculate_panel_position(
            &Rect {
                x: anchor.x,
                y: anchor.y,
                width: anchor.width,
                height: anchor.height,
            },
            &monitor_bounds,
            &work_area,
            &panel_size,
            scale,
        )
    } else {
        crate::shell::inferred_tray_panel_position_for_monitor_size(&monitor, &panel_size)
    };
    window
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> ClickOutsideContext {
        ClickOutsideContext {
            flyout_rect: ScreenRect {
                x: 100,
                y: 200,
                width: 328,
                height: 776,
            },
            settings_visible: false,
            proof_mode: false,
            native_menu_tracking: false,
        }
    }

    #[test]
    fn inside_provider_or_all_click_is_not_outside() {
        assert!(!should_dismiss_for_click(&context(), 120, 220));
    }

    #[test]
    fn outside_click_is_the_only_default_close_trigger() {
        assert!(should_dismiss_for_click(&context(), 0, 0));
    }

    #[test]
    fn companion_surfaces_and_proof_mode_are_never_closed_by_hook() {
        let mut ctx = context();
        ctx.settings_visible = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
        ctx.settings_visible = false;
        ctx.proof_mode = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
        ctx.proof_mode = false;
        ctx.native_menu_tracking = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn screen_rect_uses_half_open_bounds() {
        let rect = context().flyout_rect;
        assert!(rect.contains(100, 200));
        assert!(rect.contains(427, 975));
        assert!(!rect.contains(428, 200));
        assert!(!rect.contains(100, 976));
    }

    #[test]
    fn tray_window_properties_keep_native_resize_contract() {
        let props = SurfaceMode::TrayPanel.window_properties();
        assert_eq!(props.width, 328.0);
        assert_eq!(props.height, 776.0);
        assert_eq!(props.min_width, Some(300.0));
        assert_eq!(props.min_height, Some(360.0));
        assert!(props.resizable);
        assert!(props.always_on_top);
        assert!(props.skip_taskbar);
    }
}
