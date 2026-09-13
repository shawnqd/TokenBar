//! The detached tray panel window.
//!
//! This module is deliberately the only owner of the flyout window lifecycle.
//! React owns the provider cards; this module owns native visibility,
//! anchoring, resize persistence, outside-click dismissal and the close
//! animation.  In particular, `Focused(false)` is never a hide command:
//! WebView2 can emit it while focus moves between controls in the same panel.
//! Outside clicks are classified by the global mouse hook against the native
//! window rectangle and then handled on the Tauri main thread.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl};

use crate::state::AppState;
use crate::surface::SurfaceMode;

pub const FLYOUT_LABEL: &str = "flyout";
pub const TRAY_PANEL_REVEALED_EVENT: &str = "tray-panel-revealed";
pub const TRAY_PANEL_CLOSING_EVENT: &str = "tray-panel-closing";
pub const TRAY_PANEL_HIDDEN_EVENT: &str = "tray-panel-hidden";
pub const TRAY_PANEL_FROST_EVENT: &str = "tray-panel-frost";

const CLOSE_ANIMATION_DURATION: Duration = Duration::from_millis(180);
static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

/// The native lifecycle is deliberately kept in one small controller.  The
/// tray callback, the mouse hook and Tauri window events may arrive from
/// different threads, so several unrelated atomics made it possible for an
/// old callback to observe a new phase.  A single lock gives every transition
/// one ordering and keeps the animation generation beside the visible phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlyoutPhase {
    Hidden,
    Showing,
    Visible,
    Closing,
}

#[derive(Debug, Clone, Copy)]
struct FlyoutController {
    phase: FlyoutPhase,
    close_generation: u64,
    input_generation: u64,
}

impl Default for FlyoutController {
    fn default() -> Self {
        Self {
            phase: FlyoutPhase::Hidden,
            close_generation: 0,
            input_generation: 0,
        }
    }
}

impl FlyoutController {
    fn set_phase(&mut self, phase: FlyoutPhase) {
        self.phase = phase;
    }

    fn phase_is(&self, phase: FlyoutPhase) -> bool {
        self.phase == phase
    }

    fn cancel_pending_close(&mut self) -> bool {
        if self.phase != FlyoutPhase::Closing {
            return false;
        }
        self.close_generation = self.close_generation.wrapping_add(1);
        self.phase = FlyoutPhase::Visible;
        true
    }

    fn next_input_generation(&mut self) -> u64 {
        self.input_generation = self.input_generation.wrapping_add(1);
        self.input_generation
    }

    fn invalidate_pending_input(&mut self) {
        let _ = self.next_input_generation();
    }

    fn input_generation_is_current(&self, generation: u64) -> bool {
        self.input_generation == generation
    }

    fn begin_close(&mut self) -> Option<u64> {
        if self.phase == FlyoutPhase::Closing {
            return None;
        }
        self.phase = FlyoutPhase::Closing;
        self.close_generation = self.close_generation.wrapping_add(1);
        Some(self.close_generation)
    }

    fn close_is_current(&self, generation: u64) -> bool {
        self.phase == FlyoutPhase::Closing && self.close_generation == generation
    }
}

static FLYOUT_CONTROLLER: OnceLock<Mutex<FlyoutController>> = OnceLock::new();

fn controller() -> &'static Mutex<FlyoutController> {
    FLYOUT_CONTROLLER.get_or_init(|| Mutex::new(FlyoutController::default()))
}

fn with_controller<T>(operation: impl FnOnce(&mut FlyoutController) -> T) -> T {
    let mut guard = controller()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    operation(&mut guard)
}

fn set_phase(phase: FlyoutPhase) {
    with_controller(|controller| controller.set_phase(phase));
}

fn phase_is(phase: FlyoutPhase) -> bool {
    with_controller(|controller| controller.phase_is(phase))
}

fn cancel_pending_close() -> bool {
    with_controller(FlyoutController::cancel_pending_close)
}

fn next_input_generation() -> u64 {
    with_controller(FlyoutController::next_input_generation)
}

fn invalidate_pending_input() {
    with_controller(FlyoutController::invalidate_pending_input);
}

fn input_generation_is_current(generation: u64) -> bool {
    with_controller(|controller| controller.input_generation_is_current(generation))
}

fn begin_close_transition() -> Option<u64> {
    with_controller(FlyoutController::begin_close)
}

fn close_transition_is_current(generation: u64) -> bool {
    with_controller(|controller| controller.close_is_current(generation))
}

/// The tray-v5 chrome gutter rooms the CSS contact shadow inside the HWND
/// (tray-v5.css `.tray-panel-reveal { padding: 6px }`, frozen design
/// `.flyout-chrome::before { inset: 6px }`). The sizes in
/// `SurfaceMode::TrayPanel.window_properties()` and the geometry store are
/// CARD sizes; the window bounds are the card plus this gutter on every
/// side, so the visible card keeps its design width. Keep in lockstep with
/// the CSS padding.
const CHROME_GUTTER_DIP: f64 = 6.0;

/// Resolve the remembered CARD size, then add the chrome gutter for the
/// window bounds.
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
    let (width, height) = snap_default_card_size(width, height, props);
    let width = props.min_width.map_or(width, |min| width.max(min));
    let height = props.min_height.map_or(height, |min| height.max(min));
    let width = props.max_width.map_or(width, |max| width.min(max));
    let height = props.max_height.map_or(height, |max| height.min(max));
    let gutter = CHROME_GUTTER_DIP * 2.0;
    (width + gutter, height + gutter)
}

fn remember_geometry(window: &tauri::WebviewWindow) {
    if window.is_maximized().unwrap_or(false) || window.is_minimized().unwrap_or(false) {
        return;
    }
    let Ok(position) = window.outer_position() else {
        return;
    };
    // Inner size is the HWND client (card + chrome gutter). Outer size includes
    // DWM shadow and was previously stored as if it were already logical, so a
    // 125% DPI session persisted ~407 instead of the 320 card minimum.
    let Ok(size) = window.inner_size() else {
        return;
    };
    let scale = window.scale_factor().unwrap_or(1.0).max(0.5);
    let (width, height) = logical_card_size_from_physical_inner(size.width, size.height, scale);
    let props = SurfaceMode::TrayPanel.window_properties();
    let (width, height) = snap_default_card_size(width as f64, height as f64, &props);
    crate::geometry_store::save_entry(
        FLYOUT_LABEL,
        crate::geometry_store::StoredGeometry {
            x: position.x,
            y: position.y,
            width: Some(width.round() as u32),
            height: Some(height.round() as u32),
        },
    );
}

fn logical_card_size_from_physical_inner(
    physical_width: u32,
    physical_height: u32,
    scale: f64,
) -> (u32, u32) {
    let scale = scale.max(0.5);
    let gutter = CHROME_GUTTER_DIP * 2.0;
    let width = ((physical_width as f64 / scale) - gutter).round().max(1.0) as u32;
    let height = ((physical_height as f64 / scale) - gutter).round().max(1.0) as u32;
    (width, height)
}

/// HWND client size jitters by a few DIP (DWM shadow, 125% rounding). That
/// noise used to be saved as a new card size, then the 12px gutter was added
/// again on open — 320 became 349, then wider every close. Snap back unless
/// the user clearly dragged past the default.
fn snap_default_card_size(
    width: f64,
    height: f64,
    props: &crate::surface::WindowProperties,
) -> (f64, f64) {
    const SNAP_DIP: f64 = 96.0;
    let width = if (width - props.width).abs() <= SNAP_DIP {
        props.width
    } else {
        width
    };
    let height = if (height - props.height).abs() <= SNAP_DIP {
        props.height
    } else {
        height
    };
    (width, height)
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
    /// The native tray icon that owns the flyout. A click here is a toggle
    /// event handled by `tray_bridge`, not an outside-dismiss event. Keeping
    /// this in the same main-thread decision prevents a tray click during the
    /// close animation from reopening and immediately closing the panel.
    pub tray_icon_rect: Option<ScreenRect>,
    pub settings_visible: bool,
    /// Mirrors `Settings::keep_tray_panel_on_settings`. When Settings is
    /// visible this is the only thing that can still allow an outside click
    /// to close the flyout — proof mode and the native menu stay exclusive.
    pub keep_tray_panel_on_settings: bool,
    pub proof_mode: bool,
    pub native_menu_tracking: bool,
}

pub fn should_dismiss_for_click(ctx: &ClickOutsideContext, x: i32, y: i32) -> bool {
    !ctx.proof_mode
        && !(ctx.settings_visible && ctx.keep_tray_panel_on_settings)
        && !ctx.native_menu_tracking
        && !ctx.flyout_rect.contains(x, y)
        && !ctx
            .tray_icon_rect
            .is_some_and(|tray_icon_rect| tray_icon_rect.contains(x, y))
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
    fn FindWindowExW(parent: isize, after: isize, class: *const u16, name: *const u16) -> isize;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn GetCursorPos(pt: *mut CursorPoint) -> i32;
    fn ReleaseCapture() -> i32;
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
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct CursorPoint {
    x: i32,
    y: i32,
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
/// The tauri-runtime drag-resize child (TAURI_DRAG_RESIZE_BORDERS) is created
/// once at build. WebView2 can restack its own child HWNDs above it across
/// hide/show cycles (the reveal path also fires two SWP_FRAMECHANGED around
/// show()); when that happens the webview swallows every edge hit as a client
/// click and the panel feels stuck in both axes. Re-top the child after each
/// reveal + focus so edge drags reach the sizing child again.
#[cfg(windows)]
fn reassert_drag_resize_child(window: &tauri::WebviewWindow) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    const CLASS: &str = "TAURI_DRAG_RESIZE_BORDERS";
    const NAME: &str = "TAURI_DRAG_RESIZE_WINDOW";
    // HWND_TOP = 0; keep this cross-window restack asynchronous so a delayed
    // WebView2 child repair cannot wait on another GUI thread.
    const SWP_NOACTIVATE_NOMOVE_NOSIZE: u32 = 0x10 | 0x02 | 0x01 | 0x4000;
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return;
    };
    let inner = handle.hwnd.get();
    const GA_ROOT: u32 = 2;
    let parent = unsafe {
        let r = GetAncestor(inner, GA_ROOT);
        if r != 0 { r } else { inner }
    };
    let class: Vec<u16> = CLASS.encode_utf16().chain(std::iter::once(0)).collect();
    let name: Vec<u16> = NAME.encode_utf16().chain(std::iter::once(0)).collect();
    let mut child = unsafe { FindWindowExW(parent, 0, class.as_ptr(), name.as_ptr()) };
    if child == 0 {
        child = unsafe { FindWindowExW(parent, 0, class.as_ptr(), std::ptr::null()) };
    }
    if child != 0 {
        unsafe { SetWindowPos(child, 0, 0, 0, 0, 0, SWP_NOACTIVATE_NOMOVE_NOSIZE) };
    }
}
/// Begin a native resize from a frontend edge handle (fallback path).
///
/// WebView2 covers the client, so parent WM_NCHITTEST never sees the card
/// stroke. The live frontend uses Tauri `startResizeDragging`; this command
/// stays as the same OS sequence: ReleaseCapture (WebView2 already owns the
/// mouse on mousedown) then PostMessage WM_NCLBUTTONDOWN with packed POINTS.
/// SendMessage without ReleaseCapture nested a modal loop inside IPC while
/// WebView2 still held capture, so the sizing loop never saw mouse moves.
/// DefWindowProc returns 0 for this message — that is success, not failure.
#[cfg(windows)]
pub fn begin_resize(app: &AppHandle, dir: &str) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    const WM_NCLBUTTONDOWN_MSG: u32 = 0x00A1;
    const HT_LEFT: isize = 10;
    const HT_RIGHT: isize = 11;
    const HT_TOP: isize = 12;
    const HT_TOPLEFT: isize = 13;
    const HT_TOPRIGHT: isize = 14;
    const HT_BOTTOM: isize = 15;
    const HT_BOTTOMLEFT: isize = 16;
    const HT_BOTTOMRIGHT: isize = 17;
    const GA_ROOT_RESIZE: u32 = 2;
    let hit: isize = match dir {
        "n" => HT_TOP,
        "s" => HT_BOTTOM,
        "e" => HT_RIGHT,
        "w" => HT_LEFT,
        "ne" => HT_TOPRIGHT,
        "nw" => HT_TOPLEFT,
        "se" => HT_BOTTOMRIGHT,
        "sw" => HT_BOTTOMLEFT,
        other => return Err(format!("unknown resize direction: {other}")),
    };
    let window = app
        .get_webview_window(FLYOUT_LABEL)
        .ok_or_else(|| "flyout window unavailable".to_string())?;
    let Ok(handle) = window.window_handle() else {
        return Err("flyout handle unavailable".to_string());
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("flyout is not a Win32 window".to_string());
    };
    let inner = handle.hwnd.get();
    let root = unsafe { GetAncestor(inner, GA_ROOT_RESIZE) };
    let root = if root != 0 { root } else { inner };
    let mut pt = CursorPoint::default();
    if unsafe { GetCursorPos(&raw mut pt) } == 0 {
        return Err("GetCursorPos failed".to_string());
    }
    let packed = ((pt.y as u16 as u32) << 16) | (pt.x as u16 as u32);
    let lparam = packed as isize;
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        if let Ok(mut guard) = state.lock() {
            guard.begin_gesture_blur_guard(std::time::Instant::now());
        }
    }
    unsafe { ReleaseCapture() };
    let posted = unsafe { PostMessageW(root, WM_NCLBUTTONDOWN_MSG, hit as usize, lparam) };
    if posted == 0 {
        if let Some(state) = app.try_state::<Mutex<AppState>>() {
            if let Ok(mut guard) = state.lock() {
                guard.end_gesture_blur_guard();
            }
        }
        return Err("PostMessageW(WM_NCLBUTTONDOWN) failed".to_string());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn begin_resize(_app: &AppHandle, _dir: &str) -> Result<(), String> {
    Err("resize handles are Windows-only".to_string())
}

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
            let generation = next_input_generation();
            tracing::debug!(x, y, generation, "flyout: global mouse button queued");
            post_input_event(app, FlyoutInputEvent::MouseDown { x, y, generation });
        }
    }
    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

#[derive(Debug, Clone, Copy)]
enum FlyoutInputEvent {
    MouseDown { x: i32, y: i32, generation: u64 },
}

/// Serialize native input with tray toggles and window transitions.  The hook
/// thread only posts a value; all lifecycle decisions happen on Tauri's main
/// thread, through this one dispatcher.
fn post_input_event(app: &AppHandle, event: FlyoutInputEvent) {
    let posted = app.clone();
    if app
        .run_on_main_thread(move || dispatch_input_event(&posted, event))
        .is_err()
    {
        tracing::debug!(?event, "flyout: input event dropped while shutting down");
    }
}

fn dispatch_input_event(app: &AppHandle, event: FlyoutInputEvent) {
    match event {
        FlyoutInputEvent::MouseDown { x, y, generation } => close_if_outside(app, x, y, generation),
    }
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

fn close_if_outside(app: &AppHandle, x: i32, y: i32, generation: u64) {
    if !input_generation_is_current(generation) {
        tracing::debug!(x, y, generation, "flyout: stale outside check ignored");
        return;
    }
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
        tray_icon_rect: app
            .try_state::<Mutex<AppState>>()
            .and_then(|state| state.lock().ok()?.tray_anchor)
            .map(|anchor| ScreenRect {
                x: anchor.x,
                y: anchor.y,
                width: anchor.width as i32,
                height: anchor.height as i32,
            }),
        settings_visible: crate::shell::settings_window::is_visible(app),
        keep_tray_panel_on_settings: codexbar::settings::Settings::load()
            .keep_tray_panel_on_settings,
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

#[cfg(windows)]
fn spawn_frost_finish(app: &AppHandle, snap: Option<super::frost::Snapshot>) {
    let Some(snap) = snap else {
        return;
    };
    let app = app.clone();
    std::thread::spawn(move || {
        if let Some(css) = super::frost::finish(snap) {
            let _ = app.emit_to(FLYOUT_LABEL, TRAY_PANEL_FROST_EVENT, css);
        }
    });
}

fn show_window(app: &AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    invalidate_pending_input();
    let was_visible = window.is_visible().unwrap_or(false);
    let was_closing = cancel_pending_close();
    set_phase(FlyoutPhase::Showing);
    if !was_visible {
        // Prewarmed HWND keeps the last client size. Re-apply the snapped
        // card so a leftover 349-wide window does not survive a geometry
        // migration or a snap-to-default save.
        let props = SurfaceMode::TrayPanel.window_properties();
        let (width, height) = remembered_size(&props);
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
    }
    super::dwm::force_flyout_shell(window);
    // Grab a tiny desktop snapshot while hidden (~1ms). Blur/BMP stay off
    // this thread so click-to-show matches FluentFlyout / Telegram: the
    // window appears first.
    #[cfg(windows)]
    let frost_snap = if !was_visible {
        super::frost::snapshot(window)
    } else {
        None
    };
    if let Err(error) = window.show() {
        set_phase(FlyoutPhase::Hidden);
        return Err(error.to_string());
    }
    let _ = window.set_resizable(true);
    super::dwm::force_flyout_shell(window);
    #[cfg(windows)]
    spawn_frost_finish(app, frost_snap);
    if let Err(error) = window.set_focus() {
        // The native window is already visible. Keep the controller honest so
        // the next tray click can close/reopen it instead of being trapped in
        // a permanent SHOWING phase.
        set_phase(FlyoutPhase::Visible);
        return Err(error.to_string());
    }
    set_phase(FlyoutPhase::Visible);
    #[cfg(windows)]
    {
        reassert_drag_resize_child(window);
        // WebView2 restack is async (reveal fires two SWP_FRAMECHANGED around show()).
        // Single reassert can be undone by pending frame; schedule delayed re-tops.
        let app_for_reassert = app.clone();
        let label_for_reassert = window.label().to_string();
        tauri::async_runtime::spawn(async move {
            for delay_ms in [80u64, 260u64] {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                let app_c = app_for_reassert.clone();
                let label_c = label_for_reassert.clone();
                let handle = app_c.clone();
                let _ = app_c.run_on_main_thread(move || {
                    if let Some(w) = handle.get_webview_window(&label_c) {
                        reassert_drag_resize_child(&w);
                    }
                });
            }
        });
    }
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
    invalidate_pending_input();
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
        .shadow(true)
        .resizable(props.resizable)
        .always_on_top(props.always_on_top)
        .skip_taskbar(props.skip_taskbar)
        .disable_drag_drop_handler()
        .visible(false);
    if let (Some(min_w), Some(min_h)) = (props.min_width, props.min_height) {
        // Min sizes are card sizes too; the HWND min includes the gutter so
        // the visible card can never shrink below the design minimum.
        builder = builder.min_inner_size(
            min_w as f64 + CHROME_GUTTER_DIP * 2.0,
            min_h as f64 + CHROME_GUTTER_DIP * 2.0,
        );
    }
    if props.max_width.is_some() || props.max_height.is_some() {
        let max_w = props
            .max_width
            .map(|w| w as f64 + CHROME_GUTTER_DIP * 2.0)
            .unwrap_or(4000.0);
        let max_h = props
            .max_height
            .map(|h| h as f64 + CHROME_GUTTER_DIP * 2.0)
            .unwrap_or(4000.0);
        builder = builder.max_inner_size(max_w, max_h);
    }
    #[cfg(windows)]
    let builder = builder.transparent(true);
    let window = builder
        .background_color(tauri::utils::config::Color(0, 0, 0, 0))
        .build()
        .map_err(|error| error.to_string())?;
    super::dwm::force_flyout_shell(&window);
    let target =
        position.or_else(|| super::position::default_surface_position(app, SurfaceMode::TrayPanel));
    if let Some((x, y)) = target {
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
    set_phase(FlyoutPhase::Hidden);
    #[cfg(windows)]
    spawn_frost_finish(app, super::frost::snapshot(&window));
    if reveal_when_ready {
        arm_reveal(app)?;
    }
    Ok(())
}

pub fn prewarm(app: &AppHandle) -> Result<(), String> {
    install_click_outside_watcher(app);
    open_or_focus_inner(app, None, false)
}

/// Toggle is the only tray-panel visibility entry point: a click while the
/// panel is visible starts the close animation, and a click while it is hidden
/// opens the existing prewarmed window.  The controller invalidates queued
/// mouse-hook events at each transition, so a click can never be handled by
/// both the outside-dismiss path and this toggle path.
pub fn toggle(app: &AppHandle, position: Option<(i32, i32)>) {
    if phase_is(FlyoutPhase::Closing) {
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
    invalidate_pending_input();
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        set_phase(FlyoutPhase::Hidden);
        return Ok(());
    };
    if !window.is_visible().unwrap_or(false) || phase_is(FlyoutPhase::Closing) {
        return Ok(());
    }
    let Some(generation) = begin_close_transition() else {
        return Ok(());
    };
    app.emit_to(FLYOUT_LABEL, TRAY_PANEL_CLOSING_EVENT, ())
        .map_err(|error| {
            set_phase(FlyoutPhase::Visible);
            error.to_string()
        })?;

    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(CLOSE_ANIMATION_DURATION).await;
        if !close_transition_is_current(generation) {
            return;
        }
        let main_handle = handle.clone();
        if handle
            .run_on_main_thread(move || {
                if !close_transition_is_current(generation) {
                    return;
                }
                let Some(window) = main_handle.get_webview_window(FLYOUT_LABEL) else {
                    set_phase(FlyoutPhase::Hidden);
                    return;
                };
                remember_geometry(&window);
                if window.hide().is_ok() {
                    set_phase(FlyoutPhase::Hidden);
                    published::set_hidden();
                    let _ = main_handle.emit_to(FLYOUT_LABEL, TRAY_PANEL_HIDDEN_EVENT, ());
                    #[cfg(windows)]
                    spawn_frost_finish(&main_handle, super::frost::snapshot(&window));
                } else {
                    set_phase(FlyoutPhase::Visible);
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
            set_phase(FlyoutPhase::Visible);
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
            #[cfg(windows)]
            if let Some(w) = app.get_webview_window(FLYOUT_LABEL) {
                reassert_drag_resize_child(&w);
            }
            true
        }
        tauri::WindowEvent::Moved(_) => {
            publish_flyout_geometry(app);
            true
        }
        tauri::WindowEvent::Resized(_) => {
            publish_flyout_geometry(app);
            #[cfg(windows)]
            if let Some(w) = app.get_webview_window(FLYOUT_LABEL) {
                reassert_drag_resize_child(&w);
            }
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
                width: 320,
                height: 776,
            },
            tray_icon_rect: None,
            settings_visible: false,
            keep_tray_panel_on_settings: true,
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
    fn tray_icon_click_is_not_an_outside_dismissal() {
        let mut ctx = context();
        ctx.tray_icon_rect = Some(ScreenRect {
            x: 40,
            y: 60,
            width: 24,
            height: 24,
        });

        assert!(!should_dismiss_for_click(&ctx, 52, 72));
        assert!(should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn queued_outside_check_is_invalidated_by_a_later_transition() {
        let mut controller = FlyoutController::default();
        let queued = controller.next_input_generation();
        assert!(controller.input_generation_is_current(queued));

        // Opening or closing the flyout advances the generation. A callback
        // queued for the previous click must not act on the newly transitioned
        // window state.
        controller.invalidate_pending_input();
        assert!(!controller.input_generation_is_current(queued));
    }

    #[test]
    fn controller_serializes_close_and_reopen_transitions() {
        let mut controller = FlyoutController {
            phase: FlyoutPhase::Visible,
            ..FlyoutController::default()
        };
        let generation = controller.begin_close().expect("visible panel closes");
        assert_eq!(controller.phase, FlyoutPhase::Closing);
        assert!(controller.close_is_current(generation));
        assert!(controller.cancel_pending_close());
        assert_eq!(controller.phase, FlyoutPhase::Visible);
        assert!(!controller.close_is_current(generation));
        assert!(controller.begin_close().is_some());
    }

    #[test]
    fn companion_surfaces_and_proof_mode_are_never_closed_by_hook() {
        let mut ctx = context();
        ctx.settings_visible = true;
        ctx.keep_tray_panel_on_settings = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
        ctx.settings_visible = false;
        ctx.proof_mode = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
        ctx.proof_mode = false;
        ctx.native_menu_tracking = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn settings_open_keeps_tray_only_when_opted_in() {
        let mut ctx = context();
        ctx.settings_visible = true;
        ctx.keep_tray_panel_on_settings = true;
        assert!(
            !should_dismiss_for_click(&ctx, 0, 0),
            "default: Settings open blocks outside-click dismiss"
        );
        ctx.keep_tray_panel_on_settings = false;
        assert!(
            should_dismiss_for_click(&ctx, 0, 0),
            "turning the option off restores outside-click dismiss"
        );
    }

    #[test]
    fn screen_rect_uses_half_open_bounds() {
        let rect = context().flyout_rect;
        assert!(rect.contains(100, 200));
        assert!(rect.contains(419, 975));
        assert!(!rect.contains(420, 200));
        assert!(!rect.contains(100, 976));
    }

    #[test]
    fn tray_window_properties_keep_native_resize_contract() {
        let props = SurfaceMode::TrayPanel.window_properties();
        assert_eq!(props.width, 320.0);
        assert_eq!(props.height, 776.0);
        assert_eq!(props.min_width, Some(320.0));
        assert_eq!(props.min_height, Some(380.0));
        assert!(props.resizable);
        assert!(props.always_on_top);
        assert!(props.skip_taskbar);
    }

    #[test]
    fn physical_inner_at_125_percent_dpi_stores_the_320_card() {
        // 320 card + 6px gutter each side = 332 logical inner; at 125% DPI
        // that is 415 physical. Saving outer physical minus gutter as if it
        // were already logical is how 407 ended up in window_geometry.json.
        assert_eq!(
            logical_card_size_from_physical_inner(415, 985, 1.25),
            (320, 776)
        );
    }

    #[test]
    fn chrome_noise_snaps_back_to_the_320_card() {
        let props = SurfaceMode::TrayPanel.window_properties();
        // Live AppData after v3: 349×856. That is DWM/DPI leftover, not a
        // user drag. Snap it so the next open is not 349+12 gutter.
        assert_eq!(snap_default_card_size(349.0, 856.0, &props), (320.0, 776.0));
        // A real drag past the snap window is kept.
        assert_eq!(snap_default_card_size(420.0, 900.0, &props), (420.0, 900.0));
    }

    #[test]
    fn unscaled_physical_outer_must_not_be_treated_as_the_card() {
        assert_ne!(
            logical_card_size_from_physical_inner(419, 964, 1.0),
            (320, 776)
        );
    }
}
