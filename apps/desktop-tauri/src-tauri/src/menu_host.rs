//! A message-only window that owns the self-drawn context menu.
//!
//! # Why this exists
//!
//! [`crate::taskbar_menu`] posts the chosen row's id to its **owner** window as
//! a `WM_COMMAND`. For the taskbar strip that owner is the strip itself, which
//! is fine — the strip is what was right-clicked, so it certainly exists.
//!
//! The notification-area tray icon has no window at all, and the strip cannot
//! stand in for it: `taskbar_widget_enabled` may be off, in which case the
//! strip's HWND does not exist, and the tray icon's menu would have nowhere to
//! send its result. So the menu needs an owner whose lifetime is the process's
//! rather than any surface's.
//!
//! A message-only window (`HWND_MESSAGE` as the parent) is exactly that: it is
//! never displayed, never appears in the taskbar or Alt-Tab, is not enumerated
//! by `EnumWindows`, and costs one window handle. It receives posted messages,
//! which is all this needs.
//!
//! # Thread affinity
//!
//! A window belongs to the thread that created it, and only that thread's
//! message loop delivers its messages. This one is created lazily from the tray
//! icon's event callback, which Tauri runs on the main thread — the same thread
//! that owns the app's event loop. Creating it anywhere else (an async command
//! worker, say) would produce a window whose `WM_COMMAND`s are never pumped,
//! and menu clicks would silently do nothing.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::Mutex;

const WM_COMMAND: u32 = 0x0111;
/// `HWND_MESSAGE`. A window with this parent is message-only.
const HWND_MESSAGE: isize = -3;

#[repr(C)]
struct WndClassW {
    style: u32,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: isize,
    h_icon: isize,
    h_cursor: isize,
    hbr_background: isize,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: isize,
        menu: isize,
        instance: isize,
        param: *mut c_void,
    ) -> isize;
    fn DefWindowProcW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn IsWindow(hwnd: isize) -> i32;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

const CLASS_NAME: &str = "CodexBarMenuHost";

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The cached host window. `0` means "not created yet".
static HOST: Mutex<isize> = Mutex::new(0);

unsafe extern "system" fn host_wnd_proc(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    if msg == WM_COMMAND {
        // Same routing the strip uses: the low word is the row's 1-based
        // position, resolved back to a tray menu id. Sharing the resolver is
        // the point — both menus are built from one spec, so they must also be
        // dispatched by one handler.
        crate::taskbar_context_menu::handle_command(wparam & 0xffff);
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// The message-only owner window, creating it on first use.
///
/// **Must be called from the main thread** — see the module docs. Returns
/// `None` only if the window could not be created, in which case the caller
/// should skip showing a menu rather than show one whose clicks go nowhere.
pub(crate) fn hwnd() -> Option<isize> {
    let mut guard = HOST.lock().ok()?;
    if *guard != 0 && unsafe { IsWindow(*guard) } != 0 {
        return Some(*guard);
    }

    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = wide(CLASS_NAME);
    let class_def = WndClassW {
        style: 0,
        lpfn_wnd_proc: Some(host_wnd_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: instance,
        h_icon: 0,
        h_cursor: 0,
        hbr_background: 0,
        lpsz_menu_name: std::ptr::null(),
        lpsz_class_name: class.as_ptr(),
    };
    // A second registration of the same class fails harmlessly; the create
    // below is what actually reports a problem, so the result is not checked.
    unsafe { RegisterClassW(&class_def) };

    // Bound, not inlined into the call: an inline `wide(..).as_ptr()` is a
    // temporary whose lifetime is easy to shorten by accident during a later
    // edit, and the failure mode is a dangling pointer into FFI.
    let title = wide("CodexBar menu host");
    let created = unsafe {
        CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            0,
            instance,
            std::ptr::null_mut(),
        )
    };
    if created == 0 {
        tracing::warn!(
            error = %std::io::Error::last_os_error(),
            "menu host: failed to create the message-only owner window"
        );
        return None;
    }
    *guard = created;
    Some(created)
}
