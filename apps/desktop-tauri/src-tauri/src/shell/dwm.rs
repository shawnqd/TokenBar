//! Windows DWM helpers for eliminating the non-client caption area.
//!
//! Even with `decorations(false)`, Windows keeps a thin caption strip
//! that DWM renders. We install a window subclass that intercepts
//! WM_NCCALCSIZE to zero the non-client area and WM_NCPAINT/WM_NCACTIVATE
//! to suppress DWM painting, making the window truly borderless.

#[cfg(windows)]
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

#[cfg(windows)]
#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmSetWindowAttribute(hwnd: isize, attr: u32, data: *const c_void, size: u32) -> i32;
    fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const Margins) -> i32;
}

#[cfg(windows)]
#[repr(C)]
struct Margins {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinPoint {
    x: i32,
    y: i32,
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

/// Win32 `MINMAXINFO`. `lparam` of `WM_GETMINMAXINFO` points at one of these.
#[cfg(windows)]
#[repr(C)]
struct MinMaxInfo {
    reserved: WinPoint,
    max_size: WinPoint,
    max_position: WinPoint,
    min_track_size: WinPoint,
    max_track_size: WinPoint,
}

/// Win32 `MONITORINFO` (40 bytes). `cb_size` must be set before the call.
#[cfg(windows)]
#[repr(C)]
struct MonitorInfo {
    cb_size: u32,
    rc_monitor: WinRect,
    rc_work: WinRect,
    dw_flags: u32,
}

#[cfg(windows)]
#[link(name = "user32")]
unsafe extern "system" {
    fn GetAncestor(hwnd: isize, flags: u32) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, new: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut WinRect) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn DefSubclassProc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn MonitorFromWindow(hwnd: isize, flags: u32) -> isize;
    fn GetMonitorInfoW(hmonitor: isize, info: *mut MonitorInfo) -> i32;
    fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
    fn DestroyIcon(hicon: isize) -> i32;
    fn SystemParametersInfoW(action: u32, ui_param: u32, pv: *mut c_void, win_ini: u32) -> i32;
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn DrawAnimatedRects(
        hwnd: isize,
        id_ani: i32,
        from: *const WinRect,
        to: *const WinRect,
    ) -> i32;
    fn IsIconic(hwnd: isize) -> i32;
    fn PrivateExtractIconsW(
        file: *const u16,
        index: i32,
        cx: i32,
        cy: i32,
        icons: *mut isize,
        icon_ids: *mut u32,
        n_icons: u32,
        flags: u32,
    ) -> u32;
}

#[cfg(windows)]
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(
        hwnd: isize,
        pfn: unsafe extern "system" fn(isize, u32, usize, isize, usize, usize) -> isize,
        id: usize,
        data: usize,
    ) -> i32;
}

#[cfg(windows)]
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateSolidBrush(color: u32) -> isize;
}

#[cfg(windows)]
static DARK_BRUSH: std::sync::OnceLock<isize> = std::sync::OnceLock::new();
#[cfg(windows)]
static SETTINGS_ROOT: AtomicIsize = AtomicIsize::new(0);
#[cfg(windows)]
static MINMAX_ARMED: AtomicBool = AtomicBool::new(false);


#[cfg(windows)]
const WM_NCCALCSIZE: u32 = 0x0083;
#[cfg(windows)]
const WM_NCHITTEST: u32 = 0x0084;
#[cfg(windows)]
const WM_NCPAINT: u32 = 0x0085;
#[cfg(windows)]
const WM_NCACTIVATE: u32 = 0x0086;
#[cfg(windows)]
const WM_GETMINMAXINFO: u32 = 0x0024;
#[cfg(windows)]
const SPI_GETANIMATION: u32 = 0x0048;
#[cfg(windows)]
const IDANI_CAPTION: i32 = 3;
#[cfg(windows)]
const WM_SYSCOMMAND: u32 = 0x0112;
#[cfg(windows)]
const WM_SIZE: u32 = 0x0005;
#[cfg(windows)]
const SIZE_MINIMIZED: usize = 1;
#[cfg(windows)]
const SC_MINIMIZE: usize = 0xF020;
#[cfg(windows)]
const SC_RESTORE: usize = 0xF120;
#[cfg(windows)]
const DWMWA_TRANSITIONS_FORCEDISABLED: u32 = 3;
#[cfg(windows)]
const DWMWA_NCRENDERING_POLICY: u32 = 2;
#[cfg(windows)]
const DWMNCRP_USEWINDOWSTYLE: u32 = 0;
#[cfg(windows)]
const DWMNCRP_DISABLED: u32 = 1;
#[cfg(windows)]
const GWL_EXSTYLE: i32 = -20;
#[cfg(windows)]
const WS_EX_LAYERED: isize = 0x0008_0000;
#[cfg(windows)]
const GWL_STYLE: i32 = -16;
#[cfg(windows)]
const WS_CAPTION: isize = 0x00C0_0000;
#[cfg(windows)]
const WS_SYSMENU: isize = 0x0008_0000;
#[cfg(windows)]
const WS_MINIMIZEBOX: isize = 0x0002_0000;
#[cfg(windows)]
const BORDERLESS_SUBCLASS_ID: usize = 0xC0DE_BA12;

#[cfg(windows)]
const HTLEFT: isize = 10;
#[cfg(windows)]
const HTRIGHT: isize = 11;
#[cfg(windows)]
const HTTOP: isize = 12;
#[cfg(windows)]
const HTTOPLEFT: isize = 13;
#[cfg(windows)]
const HTTOPRIGHT: isize = 14;
#[cfg(windows)]
const HTBOTTOM: isize = 15;
#[cfg(windows)]
const HTBOTTOMLEFT: isize = 16;
#[cfg(windows)]
const HTBOTTOMRIGHT: isize = 17;

/// Return the Win32 resize hit-test code for a point in screen coordinates.
///
/// Clearing the non-client area removes the default frame hit testing. Keep
/// this helper independent from the window procedure so the edge policy is
/// explicit and testable: only the outer 8-DIP band is native-resizable; the
/// rest of the transparent canvas is left to WebView2.
#[cfg(windows)]
fn resize_hit_test(rect: WinRect, x: i32, y: i32, border: i32) -> Option<isize> {
    let border = border.max(1);
    let left = x >= rect.left && x < rect.left + border;
    let right = x >= rect.right - border && x < rect.right;
    let top = y >= rect.top && y < rect.top + border;
    let bottom = y >= rect.bottom - border && y < rect.bottom;

    Some(match (left, right, top, bottom) {
        (true, false, true, false) => HTTOPLEFT,
        (false, true, true, false) => HTTOPRIGHT,
        (true, false, false, true) => HTBOTTOMLEFT,
        (false, true, false, true) => HTBOTTOMRIGHT,
        (true, false, false, false) => HTLEFT,
        (false, true, false, false) => HTRIGHT,
        (false, false, true, false) => HTTOP,
        (false, false, false, true) => HTBOTTOM,
        _ => return None,
    })
}

#[cfg(windows)]
fn screen_point_from_lparam(lparam: isize) -> (i32, i32) {
    let packed = lparam as u32;
    let x = (packed as u16) as i16 as i32;
    let y = ((packed >> 16) as u16) as i16 as i32;
    (x, y)
}

#[cfg(windows)]
unsafe extern "system" fn borderless_subclass_proc(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
    _id: usize,
    _data: usize,
) -> isize {
    match msg {
        WM_NCCALCSIZE => {
            // Settings min/restore needs a real frame for DWM's zoom-to-icon.
            if settings_minmax_armed(hwnd) {
                return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            }
            if wparam != 0 {
                // Returning 0 when wparam is TRUE tells Windows the
                // client area == the window area (no non-client area).
                return 0;
            }
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_NCHITTEST => {
            // WS_THICKFRAME is retained so Windows knows the window is
            // resizable, but WM_NCCALCSIZE above removes the non-client area
            // where the default edge hit testing normally happens. Recreate
            // only that small edge band and let WebView2 handle all interior
            // points normally.
            let mut rect = WinRect::default();
            let rect_ok = unsafe { GetWindowRect(hwnd, &mut rect) } != 0;
            if rect_ok {
                let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
                let border = ((dpi.saturating_mul(8) / 96) as i32).clamp(6, 14);
                let (x, y) = screen_point_from_lparam(lparam);
                if let Some(hit) = resize_hit_test(rect, x, y, border) {
                    return hit;
                }
            }
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_NCPAINT => {
            if settings_minmax_armed(hwnd) {
                return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            }
            // Suppress DWM non-client painting entirely (no Win32 caption).
            0
        }
        WM_NCACTIVATE => {
            if settings_minmax_armed(hwnd) {
                return unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            }
            // Accept activation but skip DWM caption painting.
            1
        }
        WM_SYSCOMMAND => {
            let cmd = wparam & 0xFFF0;
            if is_settings_root(hwnd) && (cmd == SC_MINIMIZE || cmd == SC_RESTORE) {
                arm_caption_for_dwm_minmax(hwnd);
            }
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        WM_SIZE => {
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            if is_settings_root(hwnd) && MINMAX_ARMED.load(Ordering::SeqCst) {
                // Stay armed while iconic so a later taskbar restore still
                // has a real caption frame. FRAMECHANGED while minimized
                // can undo SC_MINIMIZE and leave Settings stuck on screen.
                if should_reapply_borderless_after_size(wparam) {
                    MINMAX_ARMED.store(false, Ordering::SeqCst);
                    apply_borderless_chrome(hwnd, true, true);
                }
            }
            result
        }
        WM_GETMINMAXINFO => {
            // MUST delegate to `DefSubclassProc` FIRST here. tao (the windowing
            // layer under Tauri) fills in `min_track_size`/`max_track_size` from
            // whatever `set_min_size`/`set_max_size` the app configured by
            // handling this same message in the window's original proc — which
            // this subclass only reaches via `DefSubclassProc`. Every other
            // branch above either delegates or doesn't need to (paint/activate
            // messages have no default work to preserve), but this one used to
            // return early without delegating, which silently discarded the
            // window's configured min/max drag-resize bounds for every
            // borderless resizable window using this subclass (flyout,
            // Settings): `min_track_size`/`max_track_size` were left at
            // whatever Windows' undocumented pre-fill happened to be, not the
            // app's actual `set_min_size`/`set_max_size` values. Confirmed via
            // the flyout: dragging past its configured min/max width did
            // nothing to stop it before this fix.
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            // A borderless window whose non-client area is zeroed maximizes to
            // cover the entire monitor, including the taskbar. Constrain the
            // maximized position/size to the monitor work area instead. This
            // also intersects (not replaces) `max_track_size` with the work
            // area — a window with its own smaller configured max (the flyout)
            // keeps that max; one with no configured max (Settings)
            // still gets capped to the work area as before.
            const MONITOR_DEFAULTTONEAREST: u32 = 2;
            unsafe {
                let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
                if hmon != 0 && lparam != 0 {
                    let mut mi = MonitorInfo {
                        cb_size: std::mem::size_of::<MonitorInfo>() as u32,
                        rc_monitor: WinRect::default(),
                        rc_work: WinRect::default(),
                        dw_flags: 0,
                    };
                    if GetMonitorInfoW(hmon, &mut mi) != 0 {
                        let mmi = lparam as *mut MinMaxInfo;
                        let work_w = mi.rc_work.right - mi.rc_work.left;
                        let work_h = mi.rc_work.bottom - mi.rc_work.top;
                        (*mmi).max_position = WinPoint {
                            x: mi.rc_work.left - mi.rc_monitor.left,
                            y: mi.rc_work.top - mi.rc_monitor.top,
                        };
                        (*mmi).max_size = WinPoint {
                            x: work_w,
                            y: work_h,
                        };
                        (*mmi).max_track_size.x = (*mmi).max_track_size.x.min(work_w);
                        (*mmi).max_track_size.y = (*mmi).max_track_size.y.min(work_h);
                    }
                }
            }
            result
        }
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

/// Eliminate the DWM caption bar by subclassing the window to zero the
/// non-client area.  Safe to call on multiple windows — each gets its
/// own subclass via `SetWindowSubclass`.
///
/// When `resizable` is true, `WS_THICKFRAME` is preserved so the native
/// resize affordance still works.
#[cfg(windows)]
pub fn force_dark_caption(win: &tauri::WebviewWindow) {
    force_dark_caption_inner(win, false, false);
}

/// Transparent treatment for a resizable frontend-owned window shell.
///
/// Settings keeps `WS_THICKFRAME` for edge resizing, but disables native
/// non-client painting so the frontend's 24px rounded frame, hairline and
/// shadow are not covered by a square Win32 border.
#[cfg(windows)]
pub fn force_borderless_transparent_resizable(win: &tauri::WebviewWindow) {
    force_dark_caption_inner(win, true, true);
}

/// Flyout chrome is fully homemade: the CSS card is the window.
/// Zero the non-client area and disable DWM caption painting so Windows
/// cannot draw a title bar, min/max/close, or system frame.
#[cfg(windows)]
pub fn force_flyout_shell(win: &tauri::WebviewWindow) {
    force_borderless_transparent_resizable(win);
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    fn rect() -> WinRect {
        WinRect {
            left: 100,
            top: 100,
            right: 500,
            bottom: 400,
        }
    }

    #[test]
    fn resize_hit_test_returns_corner_and_edge_codes() {
        assert_eq!(resize_hit_test(rect(), 101, 101, 8), Some(HTTOPLEFT));
        assert_eq!(resize_hit_test(rect(), 499, 250, 8), Some(HTRIGHT));
        assert_eq!(resize_hit_test(rect(), 250, 399, 8), Some(HTBOTTOM));
        assert_eq!(resize_hit_test(rect(), 250, 250, 8), None);
    }

    #[test]
    fn minmax_caption_style_adds_caption_and_minimize_box() {
        let style = 0x1000_0000; // WS_VISIBLE
        let armed = style_with_minmax_caption(style);
        assert_ne!(armed & WS_CAPTION, 0);
        assert_ne!(armed & WS_MINIMIZEBOX, 0);
        assert_eq!(style_without_caption(armed) & WS_CAPTION, 0);
    }

    #[test]
    fn animation_info_matches_winuser_layout() {
        assert_eq!(std::mem::size_of::<AnimationInfo>(), 8);
    }

    #[test]
    fn close_zoom_icon_rect_is_centered_on_the_taskbar() {
        let tray = WinRect {
            left: 0,
            top: 1040,
            right: 1920,
            bottom: 1080,
        };
        let icon = close_zoom_icon_rect(tray, 24);
        assert_eq!(icon.right - icon.left, 24);
        assert_eq!(icon.bottom - icon.top, 24);
        assert_eq!(icon.left + 12, 960);
        assert_eq!(icon.top + 12, 1060);
    }

    #[test]
    fn minmax_stays_armed_while_iconic() {
        assert!(should_keep_minmax_armed_after_size(SIZE_MINIMIZED));
        assert!(!should_keep_minmax_armed_after_size(0));
        assert!(!should_reapply_borderless_after_size(SIZE_MINIMIZED));
        assert!(should_reapply_borderless_after_size(0));
    }

    #[test]
    fn layered_exstyle_is_stripped_for_dwm_then_restored() {
        let appwindow = 0x0004_0000;
        let layered = exstyle_without_layered(appwindow | WS_EX_LAYERED);
        assert_eq!(layered & WS_EX_LAYERED, 0);
        assert_eq!(layered, appwindow);
        assert_ne!(exstyle_with_layered(layered) & WS_EX_LAYERED, 0);
    }

}

#[cfg(windows)]
#[repr(C)]
struct AnimationInfo {
    cb_size: u32,
    min_animate: i32,
}

/// SPI_GETANIMATION: user-level "animate windows when minimizing and maximizing".
#[cfg(windows)]
pub fn window_min_animate_enabled() -> bool {
    let mut info = AnimationInfo {
        cb_size: std::mem::size_of::<AnimationInfo>() as u32,
        min_animate: 0,
    };
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETANIMATION,
            info.cb_size,
            &raw mut info as *mut c_void,
            0,
        )
    };
    ok == 0 || info.min_animate != 0
}

#[cfg(windows)]
fn is_settings_root(hwnd: isize) -> bool {
    hwnd != 0 && hwnd == SETTINGS_ROOT.load(Ordering::SeqCst)
}

#[cfg(windows)]
fn settings_minmax_armed(hwnd: isize) -> bool {
    is_settings_root(hwnd) && MINMAX_ARMED.load(Ordering::SeqCst)
}

/// Stay armed through `SIZE_MINIMIZED` so restore still has a caption frame.
#[cfg(windows)]
fn should_keep_minmax_armed_after_size(size_type: usize) -> bool {
    size_type == SIZE_MINIMIZED
}

#[cfg(windows)]
fn should_reapply_borderless_after_size(size_type: usize) -> bool {
    !should_keep_minmax_armed_after_size(size_type)
}

#[cfg(windows)]
fn exstyle_without_layered(ex: isize) -> isize {
    ex & !WS_EX_LAYERED
}

#[cfg(windows)]
fn exstyle_with_layered(ex: isize) -> isize {
    ex | WS_EX_LAYERED
}

#[cfg(windows)]
fn root_hwnd_from_window(win: &tauri::WebviewWindow) -> Option<isize> {
    use raw_window_handle::HasWindowHandle;
    let handle = win.window_handle().ok()?;
    let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
        return None;
    };
    const GA_ROOT: u32 = 2;
    let inner = h.hwnd.get();
    let hwnd = unsafe { GetAncestor(inner, GA_ROOT) };
    let hwnd = if hwnd != 0 { hwnd } else { inner };
    (hwnd != 0).then_some(hwnd)
}

#[cfg(windows)]
fn arm_caption_for_dwm_minmax(hwnd: isize) {
    MINMAX_ARMED.store(true, Ordering::SeqCst);
    let disable: i32 = 0;
    let nc_policy = DWMNCRP_USEWINDOWSTYLE;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &raw const disable as *const c_void,
            4,
        );
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &raw const nc_policy as *const c_void,
            4,
        );
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next_ex = exstyle_without_layered(ex);
        if next_ex != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next_ex);
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let next = style_with_minmax_caption(style);
        if next != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, next);
            frame_changed(hwnd);
        }
    }
}

#[cfg(windows)]
pub(crate) fn style_with_minmax_caption(style: isize) -> isize {
    style | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX
}

#[cfg(windows)]
pub(crate) fn style_without_caption(style: isize) -> isize {
    style & !WS_CAPTION
}

#[cfg(windows)]
fn frame_changed(hwnd: isize) {
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;
    unsafe {
        SetWindowPos(
            hwnd,
            0,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
        );
    }
}

#[cfg(windows)]
fn apply_borderless_chrome(hwnd: isize, keep_resize: bool, transparent: bool) {
    const WS_THICKFRAME: isize = 0x00040000;
    unsafe {
        if transparent {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let next_ex = exstyle_with_layered(ex);
            if next_ex != ex {
                SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next_ex);
            }
            let nc_policy = DWMNCRP_DISABLED;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_NCRENDERING_POLICY,
                &raw const nc_policy as *const c_void,
                4,
            );
            const DWMWA_BORDER_COLOR: u32 = 34;
            const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;
            let border_color = DWMWA_COLOR_NONE;
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_BORDER_COLOR,
                &raw const border_color as *const c_void,
                4,
            );
        }
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let new_style = if keep_resize {
            style_without_caption(style)
        } else {
            style_without_caption(style) & !WS_THICKFRAME
        };
        if new_style != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, new_style);
        }
        frame_changed(hwnd);
    }
}

#[cfg(windows)]
fn force_dark_caption_inner(win: &tauri::WebviewWindow, keep_resize: bool, transparent: bool) {
    use raw_window_handle::HasWindowHandle;

    let Ok(handle) = win.window_handle() else {
        tracing::warn!("dwm: couldn't get window handle");
        return;
    };
    let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
        tracing::warn!("dwm: not a Win32 handle");
        return;
    };

    const GA_ROOT: u32 = 2;
    let inner = h.hwnd.get();
    let hwnd = unsafe { GetAncestor(inner, GA_ROOT) };
    let hwnd = if hwnd != 0 { hwnd } else { inner };
    tracing::info!("dwm: inner={inner:#x} root={hwnd:#x}");

    const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    let dark_mode: u32 = 1;
    let caption_color: u32 = 0x001C1C1E;

    unsafe {
        let r1 = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &raw const dark_mode as *const c_void,
            4,
        );
        let r2 = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR,
            &raw const caption_color as *const c_void,
            4,
        );
        tracing::info!("dwm: dark_mode={r1:#x} caption_color={r2:#x}");

        if transparent {
            apply_borderless_chrome(hwnd, keep_resize, true);
        }

        if !transparent {
            // Extend DWM frame fully into client area
            let margins = Margins {
                left: -1,
                right: -1,
                top: -1,
                bottom: -1,
            };
            let r3 = DwmExtendFrameIntoClientArea(hwnd, &margins);
            tracing::info!("dwm: extend_frame={r3:#x}");
        }

        // Install subclass proc (safe for multiple windows)
        let ok = SetWindowSubclass(hwnd, borderless_subclass_proc, BORDERLESS_SUBCLASS_ID, 0);
        tracing::info!("dwm: subclass installed={ok}");

        if !transparent {
            // Set background brush to dark (reuse a single GDI brush)
            const GCL_HBRBACKGROUND: i32 = -10;
            let brush = *DARK_BRUSH.get_or_init(|| CreateSolidBrush(0x001C1C1E));
            if brush != 0 {
                SetWindowLongPtrW(hwnd, GCL_HBRBACKGROUND, brush);
            }
        }

        if !transparent {
            apply_borderless_chrome(hwnd, keep_resize, false);
        }
        if keep_resize {
            tracing::info!("dwm: stripped WS_CAPTION (kept WS_THICKFRAME for resize)");
        } else {
            tracing::info!("dwm: stripped WS_CAPTION/WS_THICKFRAME");
        }
    }
}

/// Win11 taskbar icon pixel size at `dpi`.
///
/// Microsoft's app-icon table: 24px @ 100%, 30 @ 125%, 36 @ 150%, 48 @ 200%.
/// https://learn.microsoft.com/en-us/windows/apps/design/iconography/app-icon-construction
pub fn taskbar_icon_px(dpi: u32) -> i32 {
    let dpi = dpi.max(96);
    ((24 * dpi as i32) / 96).max(16)
}

/// Make Settings a real unowned app-window on the taskbar and give Explorer a
/// DPI-exact icon from `icon.ico`.
///
/// The press/bounce on a taskbar button is drawn by Explorer's
/// `Taskbar.View.dll` XAML. There is no public `ITaskbarList*` method for it
/// (`ITaskbarList` only adds/deletes/activates tabs; `ITaskbarList3` is
/// overlay/progress/thumbnails). An unowned `WS_EX_APPWINDOW` window is what
/// the Shell documents as the way to get a normal button, including that
/// animation: https://learn.microsoft.com/en-us/previous-versions/bb776822(v=vs.85)
///
/// Flyout/tray hosts must NOT call this — they stay `skip_taskbar`.
#[cfg(windows)]
pub fn attach_settings_taskbar_button(win: &tauri::WebviewWindow) {
    use raw_window_handle::HasWindowHandle;

    let Ok(handle) = win.window_handle() else {
        return;
    };
    let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };
    const GA_ROOT: u32 = 2;
    let inner = h.hwnd.get();
    let hwnd = unsafe { GetAncestor(inner, GA_ROOT) };
    let hwnd = if hwnd != 0 { hwnd } else { inner };
    if hwnd == 0 {
        return;
    }
    SETTINGS_ROOT.store(hwnd, Ordering::SeqCst);
    let disable_transitions: i32 = 0;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &raw const disable_transitions as *const c_void,
            4,
        );
    }

    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_APPWINDOW: isize = 0x0004_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    unsafe {
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = (ex | WS_EX_APPWINDOW) & !WS_EX_TOOLWINDOW;
        if next != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
            const SWP_FRAMECHANGED: u32 = 0x0020;
            const SWP_NOMOVE: u32 = 0x0002;
            const SWP_NOSIZE: u32 = 0x0001;
            const SWP_NOZORDER: u32 = 0x0004;
            SetWindowPos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            );
        }
    }

    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let small_px = taskbar_icon_px(dpi);
    let big_px = ((32 * dpi as i32) / 96).max(small_px);
    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;
    if let Some(small) = extract_icon_at(small_px) {
        unsafe {
            let prev = SendMessageW(hwnd, WM_SETICON, ICON_SMALL, small);
            if prev != 0 && prev != small {
                DestroyIcon(prev);
            }
        }
    }
    if let Some(big) = extract_icon_at(big_px) {
        unsafe {
            let prev = SendMessageW(hwnd, WM_SETICON, ICON_BIG, big);
            if prev != 0 && prev != big {
                DestroyIcon(prev);
            }
        }
    }
    tracing::info!(small_px, big_px, dpi, "settings taskbar icon applied");
}

#[cfg(windows)]
fn extract_icon_at(px: i32) -> Option<isize> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../rust/icons/icon.ico");
    if !path.is_file() {
        return None;
    }
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut icon: isize = 0;
    let mut id: u32 = 0;
    let got = unsafe {
        PrivateExtractIconsW(wide.as_ptr(), 0, px, px, &mut icon, &mut id, 1, 0)
    };
    if got == 1 && icon != 0 {
        Some(icon)
    } else {
        None
    }
}

/// Minimize Settings with DWM's zoom-to-icon and the Explorer taskbar bounce.
///
/// Close must not call this: `SC_MINIMIZE` leaves the HWND iconic so the
/// close button cannot finish. Titlebar minus / taskbar click use this path;
/// close stays `hide()`.
#[cfg(windows)]
pub fn minimize_settings_to_taskbar(win: &tauri::WebviewWindow) {
    let Some(hwnd) = root_hwnd_from_window(win) else {
        return;
    };
    SETTINGS_ROOT.store(hwnd, Ordering::SeqCst);
    if unsafe { IsIconic(hwnd) } != 0 {
        return;
    }
    arm_caption_for_dwm_minmax(hwnd);
    unsafe {
        SendMessageW(hwnd, WM_SYSCOMMAND, SC_MINIMIZE, 0);
    }
}

/// Restore an iconic Settings window with the matching DWM zoom-out + bounce.
///
/// Do not strip chrome immediately afterwards: `WM_SIZE` reapplies borderless
/// once the window is no longer minimized.
#[cfg(windows)]
pub fn restore_settings_from_taskbar(win: &tauri::WebviewWindow) {
    let Some(hwnd) = root_hwnd_from_window(win) else {
        return;
    };
    SETTINGS_ROOT.store(hwnd, Ordering::SeqCst);
    if unsafe { IsIconic(hwnd) } == 0 {
        return;
    }
    arm_caption_for_dwm_minmax(hwnd);
    unsafe {
        SendMessageW(hwnd, WM_SYSCOMMAND, SC_RESTORE, 0);
    }
}

/// Caption zoom from the Settings window into a taskbar-sized square.
///
/// `DrawAnimatedRects(IDANI_CAPTION)` is the documented minimize/maximize
/// caption animation. It does not leave the window iconic, so the caller can
/// `hide()` afterwards and Settings actually closes.
/// https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-drawanimatedrects
#[cfg(windows)]
pub fn play_settings_close_zoom(win: &tauri::WebviewWindow) {
    if !window_min_animate_enabled() {
        return;
    }
    use raw_window_handle::HasWindowHandle;
    let Ok(handle) = win.window_handle() else {
        return;
    };
    let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };
    const GA_ROOT: u32 = 2;
    let inner = h.hwnd.get();
    let hwnd = unsafe { GetAncestor(inner, GA_ROOT) };
    let hwnd = if hwnd != 0 { hwnd } else { inner };
    if hwnd == 0 {
        return;
    }
    let mut from = WinRect::default();
    if unsafe { GetWindowRect(hwnd, &mut from) } == 0 {
        return;
    }
    let tray = unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) };
    let mut tray_rect = WinRect::default();
    if tray == 0 || unsafe { GetWindowRect(tray, &mut tray_rect) } == 0 {
        return;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    let to = close_zoom_icon_rect(tray_rect, taskbar_icon_px(dpi));
    unsafe {
        DrawAnimatedRects(hwnd, IDANI_CAPTION, &from, &to);
    }
}

#[cfg(windows)]
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// A 24px-at-96dpi square on the taskbar, centered in the tray window.
/// Win11's running-app cluster sits on this bar; this is the documented
/// caption-minimize target when the exact button rect is not public.
#[cfg(windows)]
fn close_zoom_icon_rect(tray: WinRect, icon_px: i32) -> WinRect {
    let icon_px = icon_px.max(16);
    let cx = (tray.left + tray.right) / 2;
    let cy = (tray.top + tray.bottom) / 2;
    let half = icon_px / 2;
    WinRect {
        left: cx - half,
        top: cy - half,
        right: cx - half + icon_px,
        bottom: cy - half + icon_px,
    }
}

#[cfg(all(test, windows))]
mod taskbar_icon_px_tests {
    use super::taskbar_icon_px;

    #[test]
    fn matches_microsoft_taskbar_table() {
        assert_eq!(taskbar_icon_px(96), 24);
        assert_eq!(taskbar_icon_px(120), 30);
        assert_eq!(taskbar_icon_px(144), 36);
        assert_eq!(taskbar_icon_px(192), 48);
    }

    #[test]
    fn icon_ico_contains_win11_taskbar_sizes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../rust/icons/icon.ico");
        let bytes = std::fs::read(&path).expect("icon.ico");
        assert!(bytes.len() >= 6);
        let count = u16::from_le_bytes(bytes[4..6].try_into().unwrap()) as usize;
        let mut sizes = Vec::new();
        for i in 0..count {
            let off = 6 + i * 16;
            let w = bytes[off];
            sizes.push(if w == 0 { 256 } else { w as i32 });
        }
        for need in [16, 20, 24, 30, 32, 36, 40, 48, 256] {
            assert!(sizes.contains(&need), "icon.ico missing {need}px, have {sizes:?}");
        }
    }
}

#[cfg(not(windows))]
pub fn force_dark_caption(_win: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn force_borderless_transparent_resizable(_win: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn force_flyout_shell(_win: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn attach_settings_taskbar_button(_win: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn play_settings_close_zoom(_win: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn minimize_settings_to_taskbar(_win: &tauri::WebviewWindow) {}

#[cfg(not(windows))]
pub fn restore_settings_from_taskbar(_win: &tauri::WebviewWindow) {}
