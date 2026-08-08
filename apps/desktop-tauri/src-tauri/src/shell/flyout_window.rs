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
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

// ─── Click-outside dismissal (focus-independent) ───────────────────────────
//
// `handle_window_event`'s `Focused(false)` arm is the primary dismiss path,
// but it only fires when Windows actually changes window activation. Some
// outside clicks never do that: a window that answers `WM_MOUSEACTIVATE`
// with `MA_NOACTIVATE` (the taskbar strip in `taskbar_widget.rs`, by design,
// so clicking it never steals input focus from whatever app is active) can
// be clicked while the flyout is open without the flyout ever losing
// activation — so `Focused(false)` never fires and the panel is stuck open.
// The fix is a second, focus-independent trigger: a low-level mouse hook
// that watches every button-down system-wide and hides the flyout when one
// lands outside both the flyout's own rect and its companion surfaces.

/// A physical-pixel screen rectangle — the coordinate space Win32 mouse
/// hooks and window rects share.
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// Everything the click-outside decision needs, gathered from live
/// window/state queries by the (platform-specific, window-owning) caller.
/// Deliberately free of any HWND/`AppHandle` so the decision function below
/// is unit-testable without a live window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickOutsideContext {
    pub flyout_rect: ScreenRect,
    /// Settings is a companion window: mirrors the existing rule in
    /// `handle_window_event`'s `Focused(false)` arm that a blur while
    /// Settings is visible is never treated as an outside click.
    pub settings_visible: bool,
    pub proof_mode: bool,
    pub recently_shown: bool,
    pub gesture_guard_active: bool,
    /// The app's popup menu (`taskbar_menu`) is on screen. It paints *outside*
    /// the flyout's rect, so without this carve-out, clicking one of its rows
    /// reads as an outside click and hides the flyout mid-interaction.
    pub native_menu_tracking: bool,
}

/// Pure decision: should a mouse-button-down at `(x, y)` (physical screen
/// pixels) dismiss the flyout? This is the click-driven counterpart to the
/// focus-driven guards in `handle_window_event`'s `Focused(false)` arm, and
/// applies the same three guards (proof mode, recently-shown grace, gesture
/// blur guard) plus the same companion-window carve-out, so the two dismiss
/// paths agree on when *not* to hide the panel — they differ only in what
/// triggers the check in the first place.
pub fn should_dismiss_for_click(ctx: &ClickOutsideContext, x: i32, y: i32) -> bool {
    if ctx.proof_mode {
        return false;
    }
    if ctx.flyout_rect.contains(x, y) {
        return false;
    }
    if ctx.settings_visible {
        return false;
    }
    if ctx.recently_shown {
        return false;
    }
    if ctx.gesture_guard_active {
        return false;
    }
    if ctx.native_menu_tracking {
        return false;
    }
    true
}

#[cfg(windows)]
/// `MSG`, for the hook thread's message loop.
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

#[link(name = "user32")]
unsafe extern "system" {
    fn GetMessageW(msg: *mut Msg, hwnd: isize, min: u32, max: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn SetWindowsHookExW(
        id_hook: i32,
        lpfn: Option<unsafe extern "system" fn(i32, usize, isize) -> isize>,
        hmod: isize,
        thread_id: u32,
    ) -> isize;
    fn CallNextHookEx(hhk: isize, code: i32, wparam: usize, lparam: isize) -> isize;
}

/// Is the app's popup menu currently on screen?
///
/// This used to probe `FindWindowW("#32768")`, the system menu class, back when
/// the strip's right-click menu was a `TrackPopupMenu` popup. It is now our own
/// self-drawn layered window (`taskbar_menu`), which that probe would never
/// match, so the module reports its own state instead — more direct, and it
/// cannot be confused by some other process's menu being up.
#[cfg(windows)]
fn native_menu_is_tracking() -> bool {
    crate::taskbar_menu::is_open()
}

#[cfg(windows)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(module_name: *const u16) -> isize;
}

#[cfg(windows)]
const WH_MOUSE_LL: i32 = 14;
#[cfg(windows)]
const WM_LBUTTONDOWN: usize = 0x0201;
#[cfg(windows)]
const WM_RBUTTONDOWN: usize = 0x0204;

#[cfg(windows)]
#[repr(C)]
struct MsllPoint {
    x: i32,
    y: i32,
}

/// Layout of `MSLLHOOKSTRUCT`, the payload Windows passes a `WH_MOUSE_LL`
/// hook procedure via `lParam`.
#[cfg(windows)]
#[repr(C)]
struct MsllHookStruct {
    pt: MsllPoint,
    mouse_data: u32,
    flags: u32,
    time: u32,
    dw_extra_info: usize,
}

/// The app handle the hook procedure dispatches into. Set once by
/// [`install_click_outside_watcher`]; a plain `extern "system"` callback has
/// no closure environment, so this is how it reaches Tauri state.
#[cfg(windows)]
static WATCHED_APP: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

/// The installed hook handle. `SetWindowsHookExW` handles are process-wide
/// and outlive any single window; the flyout itself is hidden rather than
/// destroyed for the whole app lifetime, so there is no natural "uninstall"
/// point before process exit, which already releases the hook for free.
#[cfg(windows)]
static HOOK_HANDLE: Mutex<Option<isize>> = Mutex::new(None);

/// End-to-end input delay, in milliseconds, past which a summary is a warning.
///
/// Windows drops a low-level hook that overruns `LowLevelHooksTimeout`
/// (default 300 ms) **silently** — the hook stops being called and the only
/// symptom is that outside-click dismissal quietly stops working. 20 ms is far
/// below that and is also roughly where a person starts to feel the pointer
/// lag, so it flags a problem long before Windows acts on it.
#[cfg(windows)]
const HOOK_BUDGET_MS: u64 = 20;

/// Whether the hook has run at all this session. See its single use below.
#[cfg(windows)]
static HOOK_SEEN_FIRST_CLICK: AtomicBool = AtomicBool::new(false);

/// Every hook invocation, moves included — the denominator for the lag
/// summary below.
#[cfg(windows)]
static HOOK_CALLS: AtomicU64 = AtomicU64::new(0);
/// How many invocations between summary lines. At a 1000 Hz mouse this is a
/// line every few seconds of continuous movement, and nothing at all while the
/// pointer is still.
#[cfg(windows)]
const HOOK_SUMMARY_EVERY: u64 = 5_000;

// ── What the hook is allowed to touch ───────────────────────────────────────
//
// **Nothing that can block, and nothing owned by another thread.**
//
// A `WH_MOUSE_LL` hook is not a callback in this process's own event loop: it
// is a synchronous interception of the *system's* input pipeline. Windows
// dispatches every mouse message — moves included, at whatever rate the mouse
// reports — to the thread that installed the hook, and the pointer does not
// move on **any** window on the desktop until that thread returns.
//
// The first version called `get_webview_window`, `is_visible`,
// `outer_position` and `outer_size` from in here, and installed the hook on the
// Tauri event-loop thread. So every mouse event on the desktop queued behind
// whatever that thread happened to be doing — pumping WebView2, building a
// window, dispatching an IPC command — and did four Tauri calls once it got
// there. That is why TokenBar being open made the mouse stutter.
//
// It also explains why the first attempt to measure this found nothing: the
// timer started when the callback *began running*, which is after the queueing
// is already over. `MSLLHOOKSTRUCT::time` is the only way to see it, and it is
// what `HOOK_LAG_MS` reports below.
//
// So the hook now reads four atomics and does integer arithmetic. Everything
// else — the guards, the actual hiding — is posted to the main thread, which is
// free to take as long as it likes because nothing is waiting on it.

/// The flyout's on-screen rectangle, published by the main thread.
///
/// Four `i32`s and a flag rather than a `Mutex<ScreenRect>`: a lock in here can
/// be *contended*, and a contended lock in a low-level input hook stalls the
/// desktop. Reads can tear between the four values, which at worst mis-hits a
/// click by a few pixels during a drag — the main thread re-checks the real
/// geometry before it actually hides anything.
#[cfg(windows)]
mod published {
    use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

    pub(super) static VISIBLE: AtomicBool = AtomicBool::new(false);
    static X: AtomicI32 = AtomicI32::new(0);
    static Y: AtomicI32 = AtomicI32::new(0);
    static W: AtomicI32 = AtomicI32::new(0);
    static H: AtomicI32 = AtomicI32::new(0);

    pub(super) fn set_hidden() {
        VISIBLE.store(false, Ordering::Relaxed);
    }

    pub(super) fn set_rect(x: i32, y: i32, w: i32, h: i32) {
        X.store(x, Ordering::Relaxed);
        Y.store(y, Ordering::Relaxed);
        W.store(w, Ordering::Relaxed);
        H.store(h, Ordering::Relaxed);
        VISIBLE.store(true, Ordering::Relaxed);
    }

    /// True when the flyout is up and the point is outside it. The only
    /// question the hook is allowed to ask.
    pub(super) fn click_is_outside(x: i32, y: i32) -> bool {
        if !VISIBLE.load(Ordering::Relaxed) {
            return false;
        }
        let (left, top) = (X.load(Ordering::Relaxed), Y.load(Ordering::Relaxed));
        let (w, h) = (W.load(Ordering::Relaxed), H.load(Ordering::Relaxed));
        x < left || y < top || x >= left + w || y >= top + h
    }
}

/// Publish the flyout's geometry for the hook to read.
///
/// Called from the main thread whenever the flyout is shown, moved or resized.
/// Cheap enough to call speculatively; the hook only ever reads.
#[cfg(windows)]
pub fn publish_flyout_geometry(app: &AppHandle) {
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        published::set_hidden();
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        published::set_hidden();
        return;
    }
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        // Unknown geometry: treat as hidden rather than publish a stale rect
        // the hook would test clicks against.
        published::set_hidden();
        return;
    };
    published::set_rect(
        position.x,
        position.y,
        size.width as i32,
        size.height as i32,
    );
}

#[cfg(not(windows))]
pub fn publish_flyout_geometry(_app: &AppHandle) {}

/// Worst end-to-end input delay seen since the last summary, in milliseconds.
///
/// `GetTickCount() - MSLLHOOKSTRUCT::time` — the age of the event by the time
/// the hook runs. **This is the number that corresponds to what a person
/// feels**, and it is the one the first instrumentation pass missed entirely by
/// timing the callback body instead.
#[cfg(windows)]
static HOOK_LAG_MS: AtomicU64 = AtomicU64::new(0);

#[link(name = "kernel32")]
#[cfg(windows)]
unsafe extern "system" {
    fn GetTickCount() -> u32;
}

#[cfg(windows)]
unsafe extern "system" fn click_outside_hook_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if code >= 0 {
        let info = unsafe { &*(lparam as *const MsllHookStruct) };

        // How stale the event already is. `MSLLHOOKSTRUCT::time` and
        // `GetTickCount` share a clock, so the difference is the delay the user
        // actually feels — queueing included. Timing the callback body cannot
        // see this, which is why the first instrumentation pass reported
        // microseconds while the pointer visibly stuttered.
        let age_ms = u64::from(unsafe { GetTickCount() }.wrapping_sub(info.time));
        HOOK_LAG_MS.fetch_max(age_ms, Ordering::Relaxed);

        if matches!(wparam, WM_LBUTTONDOWN | WM_RBUTTONDOWN)
            && published::click_is_outside(info.pt.x, info.pt.y)
            && let Some(app) = WATCHED_APP.get()
        {
            // Posted, never done here. Everything the decision needs — the
            // guards, the real geometry, the hiding itself — belongs to the
            // main thread, and the desktop's pointer must not wait for it.
            let posted = app.clone();
            let (x, y) = (info.pt.x, info.pt.y);
            if app
                .run_on_main_thread(move || consider_dismissal(&posted, x, y))
                .is_err()
            {
                tracing::warn!("flyout: could not post the outside-click check");
            }
        }
    }

    // Proof of life, once: an evicted hook and a fast one are equally silent,
    // and Windows evicts an over-budget low-level hook without saying so.
    if HOOK_SEEN_FIRST_CLICK
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        tracing::info!("flyout: click-outside hook is live");
    }

    let calls = HOOK_CALLS.fetch_add(1, Ordering::Relaxed) + 1;
    if calls.is_multiple_of(HOOK_SUMMARY_EVERY) {
        let worst = HOOK_LAG_MS.swap(0, Ordering::Relaxed);
        if worst >= HOOK_BUDGET_MS {
            tracing::warn!(
                calls,
                worst_lag_ms = worst,
                budget_ms = HOOK_BUDGET_MS,
                "flyout: mouse events are arriving stale — the desktop pointer                  is waiting on this process"
            );
        } else {
            tracing::info!(
                calls,
                worst_lag_ms = worst,
                "flyout: mouse hook end-to-end lag since the last summary"
            );
        }
    }

    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

/// The full outside-click decision, on the main thread.
///
/// Split out of the hook deliberately: this reads Tauri window state and the
/// shared `AppState`, and neither belongs in an input hook. Re-checks the real
/// geometry rather than trusting the published rect, which may have torn or
/// gone stale between the hook's read and this running.
#[cfg(windows)]
fn consider_dismissal(app: &AppHandle, x: i32, y: i32) {
    let Some(window) = app.get_webview_window(FLYOUT_LABEL) else {
        return;
    };
    if !window.is_visible().unwrap_or(false) {
        return;
    }
    let (Ok(position), Ok(size)) = (window.outer_position(), window.outer_size()) else {
        return;
    };

    let ctx = ClickOutsideContext {
        flyout_rect: ScreenRect {
            x: position.x,
            y: position.y,
            width: size.width as i32,
            height: size.height as i32,
        },
        settings_visible: crate::shell::settings_window::is_visible(app),
        proof_mode: crate::proof_harness::is_proof_mode(app),
        // Plain `lock` is fine here and `try_lock` was not: this is the main
        // thread now, not an input hook, so waiting for the state costs a
        // moment of this app's own responsiveness rather than the whole
        // desktop's pointer.
        recently_shown: guard_state(app, |st| {
            st.was_tray_panel_recently_shown(Instant::now(), RECENTLY_SHOWN_GRACE)
        }),
        gesture_guard_active: guard_state(app, |st| {
            st.is_gesture_blur_guard_active(Instant::now())
        }),
        native_menu_tracking: native_menu_is_tracking(),
    };

    if !should_dismiss_for_click(&ctx, x, y) {
        return;
    }
    if hide(app).is_ok()
        && let Some(st) = app.try_state::<Mutex<AppState>>()
    {
        st.lock().unwrap().mark_blur_dismissed(Instant::now());
    }
}

/// Read one boolean out of [`AppState`] without ever blocking the calling
/// thread. Returns `true` (guard active → do not dismiss) when the state is
/// missing or the lock is held, so a contended or poisoned lock can never
/// turn into a surprise dismissal or a panic inside the input hook.
#[cfg(windows)]
fn guard_state(app: &AppHandle, read: impl FnOnce(&AppState) -> bool) -> bool {
    let Some(state) = app.try_state::<Mutex<AppState>>() else {
        return true;
    };
    match state.try_lock() {
        Ok(st) => read(&st),
        Err(_) => true,
    }
}

/// Install the global low-level mouse hook **on a thread of its own**.
///
/// A `WH_MOUSE_LL` hook belongs to the thread that installs it, and Windows
/// dispatches every mouse message to that thread and waits. Installed from the
/// Tauri event-loop thread — as this was — the whole desktop's pointer queues
/// behind whatever the app is doing: pumping WebView2, building a window,
/// handling an IPC command. That is a system-wide stutter caused by an app
/// being merely *busy*, and no amount of making the callback itself faster
/// fixes it.
///
/// This thread does one thing: pump messages so the hook can run. It never
/// touches Tauri, never takes a lock the app holds, and is never busy, so the
/// dispatch is immediate no matter what the rest of the process is doing.
///
/// The message loop is not optional. Low-level hooks are delivered through the
/// installing thread's message queue, so a thread that installs one and then
/// sleeps never runs its callback at all.
///
/// Idempotent: only `prewarm` calls this, once, but a second call would only
/// refresh the watched `AppHandle`.
#[cfg(windows)]
fn install_click_outside_watcher(app: &AppHandle) {
    let _ = WATCHED_APP.set(app.clone());
    {
        let guard = HOOK_HANDLE.lock().unwrap();
        if guard.is_some() {
            return;
        }
    }

    std::thread::Builder::new()
        .name("codexbar-mouse-hook".into())
        .spawn(|| {
            let hmod = unsafe { GetModuleHandleW(std::ptr::null()) };
            let handle =
                unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(click_outside_hook_proc), hmod, 0) };
            if handle == 0 {
                tracing::warn!(
                    error = %std::io::Error::last_os_error(),
                    "flyout: failed to install global click-outside mouse hook"
                );
                return;
            }
            if let Ok(mut guard) = HOOK_HANDLE.lock() {
                *guard = Some(handle);
            }
            tracing::info!(
                thread = "codexbar-mouse-hook",
                "flyout: click-outside mouse hook installed on its own thread"
            );

            // Blocks forever, which is the point: this thread exists to be
            // available. `GetMessageW` returns 0 only on `WM_QUIT`, and nothing
            // posts one, so the loop ends with the process.
            let mut msg = Msg::default();
            while unsafe { GetMessageW(&raw mut msg, 0, 0, 0) } > 0 {
                unsafe {
                    TranslateMessage(&raw const msg);
                    DispatchMessageW(&raw const msg);
                }
            }
        })
        .map_err(|error| tracing::warn!("flyout: mouse hook thread failed to start: {error}"))
        .ok();
}

#[cfg(not(windows))]
fn install_click_outside_watcher(_app: &AppHandle) {}

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
        publish_flyout_geometry(app);
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
    // Click-outside dismissal cannot rely on `Focused(false)` alone (see the
    // "Click-outside dismissal" section above), so arm the focus-independent
    // watcher alongside the window itself, once, at startup.
    install_click_outside_watcher(app);
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
    // The hook tests clicks against a published rect; leaving a stale one would
    // have it consider dismissing a window that is already gone.
    publish_flyout_geometry(app);
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
        tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
            // Republish for the hook. This arm is also the catch-all for reveal
            // paths that do not go through `open_or_focus_inner`: a window that
            // becomes visible always moves or sizes on its way there, so the
            // published rect cannot stay stuck at "hidden" while the flyout is
            // in fact on screen.
            publish_flyout_geometry(&window.app_handle().clone());
            true
        }
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

    fn sample_flyout_rect() -> ScreenRect {
        ScreenRect {
            x: 1600,
            y: 800,
            width: 328,
            height: 776,
        }
    }

    fn clear_context() -> ClickOutsideContext {
        ClickOutsideContext {
            flyout_rect: sample_flyout_rect(),
            settings_visible: false,
            proof_mode: false,
            recently_shown: false,
            gesture_guard_active: false,
            native_menu_tracking: false,
        }
    }

    #[test]
    fn native_menu_tracking_suppresses_dismissal() {
        // The taskbar strip's right-click menu paints outside the flyout's
        // rect. Without this carve-out, clicking one of its rows would read as
        // an outside click and hide the flyout mid-interaction.
        let mut ctx = clear_context();
        ctx.native_menu_tracking = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn screen_rect_contains_is_half_open() {
        let rect = sample_flyout_rect();
        // Top-left corner is inside; the far edge (x + width, y + height) is
        // exclusive, matching how outer_position/outer_size describe bounds.
        assert!(rect.contains(1600, 800));
        assert!(rect.contains(1600 + 327, 800 + 775));
        assert!(!rect.contains(1600 + 328, 800));
        assert!(!rect.contains(1600, 800 + 776));
        assert!(!rect.contains(1599, 850));
    }

    #[test]
    fn click_inside_flyout_never_dismisses() {
        let ctx = clear_context();
        // A resize-grip drag or a card drag-reorder mousedown lands inside
        // the flyout's own outer rect (the resize border is part of it), so
        // this also covers those gestures without needing a focus-based
        // guard for this trigger.
        assert!(!should_dismiss_for_click(&ctx, 1650, 900));
    }

    #[test]
    fn click_outside_with_no_guards_active_dismisses() {
        // Regression for the reported bug: a click on a window that answers
        // WM_MOUSEACTIVATE with MA_NOACTIVATE (e.g. the taskbar strip) never
        // fires Focused(false), so this click-driven path must be able to
        // dismiss on its own.
        let ctx = clear_context();
        assert!(should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn click_outside_during_proof_mode_never_dismisses() {
        let mut ctx = clear_context();
        ctx.proof_mode = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn click_outside_while_settings_visible_never_dismisses() {
        // Settings is a companion window: mirrors handle_window_event's
        // Focused(false) rule that any blur while Settings is open is not an
        // outside click, regardless of where on screen it landed.
        let mut ctx = clear_context();
        ctx.settings_visible = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn click_outside_during_recently_shown_grace_never_dismisses() {
        let mut ctx = clear_context();
        ctx.recently_shown = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
    }

    #[test]
    fn click_outside_during_gesture_guard_never_dismisses() {
        let mut ctx = clear_context();
        ctx.gesture_guard_active = true;
        assert!(!should_dismiss_for_click(&ctx, 0, 0));
    }
}
