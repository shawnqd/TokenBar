//! A text readout positioned over the Windows taskbar.
//!
//! The tray icon is a 16px square — enough for one two-digit number rendered
//! from a 3x5 bitmap font, and not enough to read comfortably. This puts a
//! real, GDI-drawn text strip in the taskbar's free space instead, the way
//! TrafficMonitor does.
//!
//! # Why a plain Win32 window and not a WebView
//!
//! A WebView2 embedded into Explorer's window tree would be heavy and fragile
//! for what is a single strip of text; GDI's `DrawTextW` gives a real font at
//! any size, which is the entire point of the exercise.
//!
//! # Native taskbar window
//!
//! This follows TrafficMonitor's Win32 path: create a native popup, attach it
//! to `Shell_TrayWnd`, and render it from the app's message loop. It is kept
//! native (not a WebView) so Explorer owns the taskbar placement and receives
//! the same child-window relationship as other taskbar monitors.
//!
//! # What is fragile
//!
//! Everything about positioning inside the taskbar is undocumented. The timer
//! re-applies position because taskbar moves, DPI changes, theme changes and
//! Explorer restarts all invalidate the geometry.
//!
//! Controlled by `Settings::taskbar_widget_enabled`, surfaced as a Settings
//! toggle. Off by default.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};

use tauri::Manager;

/// One printable line of the strip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StripLine {
    /// The provider's brand mark, drawn in its own colour ahead of the text.
    /// `None` for a provider with no registered mark, in which case `text`
    /// still carries the provider's name and nothing is lost.
    pub mark: Option<crate::provider_mark::ProviderMark>,
    pub text: String,
}

/// What the strip prints, one entry per configured line, in the user's order.
///
/// Replaced wholesale by [`set_entries`]. Only [`MAX_VISIBLE_ENTRIES`] fit;
/// anything beyond that is dropped, which is why order is itself a user setting
/// — position decides what survives truncation.
static LINES: Mutex<Vec<StripLine>> = Mutex::new(Vec::new());

/// Text rows that physically fit in the strip's height. Two is what a Windows
/// taskbar gives you at a readable font size, and it is not negotiable.
pub const MAX_VISIBLE_ROWS: usize = 2;
/// A third and fourth entry start a second COLUMN rather than a third row —
/// the layout Windows' own network/CPU monitor widgets use for four readings.
pub const MAX_VISIBLE_COLUMNS: usize = 2;
/// How many entries the strip can show at once.
pub const MAX_VISIBLE_ENTRIES: usize = MAX_VISIBLE_ROWS * MAX_VISIBLE_COLUMNS;

static WIDGET_HWND: Mutex<isize> = Mutex::new(0);
/// The color-key painter is visually transparent, so this sibling keeps the
/// full rectangle interactive without covering the taskbar pixels.
static HIT_PROXY_HWND: Mutex<isize> = Mutex::new(0);
/// TASK-021 item 9: the strip's own hover tooltip (`tooltips_class32`),
/// distinct from the notification-area tray icon's tooltip in `tray_bridge.rs`
/// but composed from the same `build_tooltip` text.
static TOOLTIP_HWND: Mutex<isize> = Mutex::new(0);
static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

#[derive(Clone, Copy, PartialEq, Eq)]
enum WidgetPosition {
    Notification,
    Left,
}

static WIDGET_POSITION: Mutex<WidgetPosition> = Mutex::new(WidgetPosition::Notification);
static WIDGET_FONT_WEIGHT: Mutex<i32> = Mutex::new(400);
static WIDGET_FONT_SIZE: Mutex<i32> = Mutex::new(12);
static WIDGET_WIDTH: Mutex<i32> = Mutex::new(132);
static WIDGET_TEXT_ALIGN: Mutex<u32> = Mutex::new(0);
static WIDGET_CONTENT: Mutex<String> = Mutex::new(String::new());
static WIDGET_FONT_FAMILY: Mutex<String> = Mutex::new(String::new());

const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const GWL_STYLE: i32 = -16;

const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_LAYERED: u32 = 0x0008_0000;

const SWP_FRAMECHANGED: u32 = 0x0020;
const SWP_NOZORDER: u32 = 0x0004;
const SW_SHOW: i32 = 5;

const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_TIMER: u32 = 0x0113;
const WM_COMMAND: u32 = 0x0111;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_NCHITTEST: u32 = 0x0084;
const WM_MOUSEACTIVATE: u32 = 0x0021;
const HTCLIENT: isize = 1;
const MA_NOACTIVATE: isize = 3;

const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const LWA_COLORKEY: u32 = 0x0000_0001;
const DT_SINGLELINE: u32 = 0x0020;
const DT_VCENTER: u32 = 0x0004;
const DT_LEFT: u32 = 0x0000;
const DT_CENTER: u32 = 0x0001;
const DT_RIGHT: u32 = 0x0002;
const DT_NOPREFIX: u32 = 0x0800;
const DT_END_ELLIPSIS: u32 = 0x8000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;

// ── Hover tooltip (TASK-021 item 9) ─────────────────────────────────────────
const WM_USER: u32 = 0x0400;
const TTS_ALWAYSTIP: u32 = 0x0001;
const TTS_NOPREFIX: u32 = 0x0002;
const TTF_IDISHWND: u32 = 0x0001;
const TTF_SUBCLASS: u32 = 0x0010;
const TTM_ACTIVATE: u32 = WM_USER + 1;
const TTM_ADDTOOLW: u32 = WM_USER + 50;
const TTM_SETMAXTIPWIDTH: u32 = WM_USER + 24;
const TTM_UPDATETIPTEXTW: u32 = WM_USER + 57;
const ICC_WIN95_CLASSES: u32 = 0x0000_00FF;
const CW_USEDEFAULT: i32 = 0x8000_0000u32 as i32;

const TRANSPARENT_BK: i32 = 1;
const REASSERT_TIMER_ID: usize = 1;
const REASSERT_INTERVAL_MS: u32 = 1000;

/// Gap kept between the strip and the task-button band beside it.
const WIDGET_MARGIN_DIP: i32 = 8;
/// Keep the child shorter than the taskbar so the two text lines align with
/// neighboring taskbar status items instead of occupying the full row.
const WIDGET_HEIGHT_DIP: i32 = 36;

#[repr(C)]
#[derive(Default, Clone, Copy, Debug)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
struct PaintStruct {
    hdc: isize,
    f_erase: i32,
    rc_paint: Rect,
    f_restore: i32,
    f_inc_update: i32,
    rgb_reserved: [u8; 32],
}

/// `TOOLINFOW` (commctrl.h). Field order/types match the SDK struct exactly so
/// `repr(C)` produces identical layout (including the implicit padding before
/// `hwnd` on 64-bit) without a manual padding field.
#[repr(C)]
struct ToolInfoW {
    cb_size: u32,
    u_flags: u32,
    hwnd: isize,
    u_id: usize,
    rect: Rect,
    h_inst: isize,
    lpsz_text: *mut u16,
    l_param: isize,
    lp_reserved: *mut c_void,
}

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
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
    fn FindWindowExW(parent: isize, after: isize, class: *const u16, name: *const u16) -> isize;
    fn GetClassNameW(hwnd: isize, class_name: *mut u16, max_count: i32) -> i32;
    fn SetParent(hwnd: isize, new_parent: isize) -> isize;
    fn GetParent(hwnd: isize) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
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
    fn BeginPaint(hwnd: isize, ps: *mut PaintStruct) -> isize;
    fn EndPaint(hwnd: isize, ps: *const PaintStruct) -> i32;
    fn FillRect(hdc: isize, rect: *const Rect, brush: isize) -> i32;
    fn DrawTextW(hdc: isize, text: *const u16, count: i32, rect: *mut Rect, format: u32) -> i32;
    fn GetClientRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn SetTimer(hwnd: isize, id: usize, elapse: u32, func: *const c_void) -> usize;
    fn KillTimer(hwnd: isize, id: usize) -> i32;
    fn InvalidateRect(hwnd: isize, rect: *const Rect, erase: i32) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn ScreenToClient(hwnd: isize, point: *mut Point) -> i32;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd_show: i32) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    fn GetPixel(hdc: isize, x: i32, y: i32) -> u32;
    fn SetLayeredWindowAttributes(hwnd: isize, color_key: u32, alpha: u8, flags: u32) -> i32;
    fn SendMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strikeout: u32,
        charset: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_and_family: u32,
        face: *const u16,
    ) -> isize;
    fn SelectObject(hdc: isize, obj: isize) -> isize;
    fn DeleteObject(obj: isize) -> i32;
    fn SetTextColor(hdc: isize, color: u32) -> u32;
    fn SetBkMode(hdc: isize, mode: i32) -> i32;
    fn CreateSolidBrush(color: u32) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

/// Only used to make sure `tooltips_class32` is registered before we create
/// the strip's hover tooltip — comctl32 v6 (already the default on Windows
/// 10/11) does this automatically, but calling it explicitly costs nothing
/// and removes the dependency on that default holding.
#[repr(C)]
struct InitCommonControlsExStruct {
    dw_size: u32,
    dw_icc: u32,
}

#[link(name = "comctl32")]
unsafe extern "system" {
    fn InitCommonControlsEx(icc: *const InitCommonControlsExStruct) -> i32;
}

const HKEY_CURRENT_USER: isize = -2147483647; // 0x80000001, sign-extended to isize
const KEY_READ: u32 = 0x20019;

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegOpenKeyExW(
        key: isize,
        sub_key: *const u16,
        options: u32,
        sam_desired: u32,
        result: *mut isize,
    ) -> i32;
    fn RegQueryValueExW(
        key: isize,
        value_name: *const u16,
        reserved: *const u32,
        value_type: *mut u32,
        data: *mut u8,
        data_size: *mut u32,
    ) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}

/// Reads `SystemUsesLightTheme`, the same registry value Explorer uses to
/// decide whether the taskbar itself (not just apps) is light or dark.
/// Defaults to dark, Windows' own out-of-box default, if the key is absent
/// or unreadable.
pub(crate) fn taskbar_is_light() -> bool {
    let sub_key = wide(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize");
    let value_name = wide("SystemUsesLightTheme");
    let mut hkey: isize = 0;
    let opened = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            sub_key.as_ptr(),
            0,
            KEY_READ,
            &raw mut hkey,
        )
    };
    if opened != 0 {
        return false;
    }
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let mut value_type: u32 = 0;
    let ok = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null(),
            &raw mut value_type,
            &raw mut data as *mut u32 as *mut u8,
            &raw mut size,
        )
    };
    unsafe { RegCloseKey(hkey) };
    ok == 0 && data != 0
}

#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Replaces the two printed lines and repaints.
pub fn set_entries(entries: Vec<StripLine>) {
    if let Ok(mut guard) = LINES.lock() {
        *guard = entries;
    }
    repaint();
    // TASK-021 item 9: this is the same beat `tray_bridge::update_tray_icon_and_tooltip`
    // rebuilds the tray icon's tooltip on (it calls `set_entries` immediately
    // before composing that tooltip) — not the 1s `REASSERT_TIMER_ID` tick,
    // which only repositions/repaints already-resolved data and would otherwise
    // reload `Settings` from disk once a second for no new information.
    update_tooltip_text();
}

/// The font family the strip paints with, falling back the same way `paint`
/// always has. Shared with the context menu so its label text matches the
/// strip's own typeface.
pub(crate) fn widget_font_family() -> String {
    let family = WIDGET_FONT_FAMILY
        .lock()
        .map(|f| f.clone())
        .unwrap_or_default();
    if family.trim().is_empty() {
        "Microsoft YaHei UI".to_string()
    } else {
        family
    }
}

/// Exactly what the strip is printing right now.
///
/// The settings page's preview reads this rather than composing its own
/// imitation. An imitation has to re-derive every rule the renderer applies —
/// which numbers are real, which entries resolved, how a balance prints — and
/// the moment one of those rules changes on one side only, the preview starts
/// lying about the thing it exists to show. It did: it printed invented sample
/// percentages and a hardcoded "unsupported" for every balance entry, while the
/// real strip beside it showed the true figure and the true balance.
pub fn current_entries() -> Vec<StripLine> {
    LINES.lock().map(|guard| guard.clone()).unwrap_or_default()
}

/// Supplies the app handle used by the native taskbar context menu.
pub fn set_app_handle(app: &tauri::AppHandle) {
    let _ = APP_HANDLE.set(app.clone());
}

fn taskbar() -> isize {
    unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) }
}

/// Where the native taskbar child should sit, returned in screen coordinates
/// so the same geometry can be measured before and after `SetParent`.
///
/// Anchored immediately left of the system tray/clock (`TrayNotifyWnd`) —
/// i.e. flush against the actual notification area, not merely "somewhere
/// left of the task buttons".
///
/// Two earlier anchors were tried and both landed on top of something:
///
/// - Left of the task-button band (`ReBarWindow32`) put it directly in space
///   a third-party monitor (TrafficMonitor) already paints into — and that
///   tool draws its stats straight onto taskbar pixels wider than its own
///   window bounds, so it silently overwrites whatever else is there.
/// - Right of `ReBarWindow32` still landed under running-app icons on this
///   machine: Windows 11 lays icons out via a separate XAML/composition
///   surface (`Windows.UI.Composition.DesktopWindowContentBridge`) that does
///   not reliably match `ReBarWindow32`'s own reported bounds, so "past the
///   rebar's right edge" was not actually past the visible icons.
///
/// `TrayNotifyWnd`'s left edge held steady across repeated measurements
/// where `ReBarWindow32`'s right edge visibly did not, so it is the more
/// trustworthy boundary to anchor from. `ReBarWindow32`'s edge is still used
/// as a clamp (never render on top of it) rather than as the primary anchor.
fn target_rect(parent: isize, position: WidgetPosition) -> Option<(i32, i32, i32, i32)> {
    let mut screen_parent = Rect::default();
    if unsafe { GetWindowRect(parent, &raw mut screen_parent) } == 0 {
        return None;
    }
    let mut parent_rect = Rect::default();
    if unsafe { GetClientRect(parent, &raw mut parent_rect) } == 0 {
        return None;
    }
    let taskbar_height = parent_rect.bottom - parent_rect.top;
    if taskbar_height <= 0 {
        return None;
    }

    let dpi = unsafe { GetDpiForWindow(parent) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let scale = |dip: i32| (dip * dpi as i32) / 96;
    let desired_width = scale(WIDGET_WIDTH.lock().map(|width| *width).unwrap_or(132));
    let margin = scale(WIDGET_MARGIN_DIP);
    let height = scale(WIDGET_HEIGHT_DIP).min(taskbar_height);

    let local_edge = |class: &str, use_right_edge: bool| -> Option<i32> {
        let hwnd = unsafe { FindWindowExW(parent, 0, wide(class).as_ptr(), std::ptr::null()) };
        if hwnd == 0 {
            return None;
        }
        let mut rect = Rect::default();
        if unsafe { GetWindowRect(hwnd, &raw mut rect) } == 0 {
            return None;
        }
        let mut point = Point {
            x: if use_right_edge {
                rect.right
            } else {
                rect.left
            },
            y: rect.top,
        };
        if unsafe { ScreenToClient(parent, &raw mut point) } == 0 {
            return None;
        }
        Some(point.x)
    };

    // Third-party taskbar monitors (notably TrafficMonitor) often draw their
    // text directly into the taskbar, beyond the bounds of their helper
    // window. Put our readout after the rightmost non-system child in the
    // left taskbar zone so those direct-to-screen paints cannot overwrite it.
    let status_component_right = || -> Option<i32> {
        let rebar_left = local_edge("ReBarWindow32", false).unwrap_or(parent_rect.right);
        let mut after = 0;
        let mut rightmost = None;
        loop {
            let child = unsafe { FindWindowExW(parent, after, std::ptr::null(), std::ptr::null()) };
            if child == 0 {
                break;
            }
            after = child;

            let mut class_buf = [0u16; 128];
            let class_len =
                unsafe { GetClassNameW(child, class_buf.as_mut_ptr(), class_buf.len() as i32) };
            let class = String::from_utf16_lossy(&class_buf[..class_len.max(0) as usize]);
            if matches!(
                class.as_str(),
                "Shell_TrayWnd"
                    | "TrayNotifyWnd"
                    | "ReBarWindow32"
                    | "Start"
                    | "TrayDummySearchControl"
                    | "Windows.UI.Core.CoreWindow"
                    | "Windows.UI.Composition.DesktopWindowContentBridge"
                    | "CodexBarTaskbarWidget"
                    | "CodexBarTaskbarWidgetHit"
            ) {
                continue;
            }

            let mut rect = Rect::default();
            if unsafe { GetWindowRect(child, &raw mut rect) } == 0 {
                continue;
            }
            if rect.right <= rect.left || rect.bottom <= rect.top {
                continue;
            }
            let mut right_point = Point {
                x: rect.right,
                y: rect.top,
            };
            if unsafe { ScreenToClient(parent, &raw mut right_point) } == 0 {
                continue;
            }
            let mut left_point = Point {
                x: rect.left,
                y: rect.top,
            };
            if unsafe { ScreenToClient(parent, &raw mut left_point) } == 0 {
                continue;
            }
            let vertically_aligned = rect.bottom > screen_parent.top
                && rect.top < screen_parent.bottom
                && rect.bottom - rect.top >= height / 2;
            if vertically_aligned && left_point.x >= parent_rect.left && right_point.x <= rebar_left
            {
                rightmost = Some(rightmost.unwrap_or(right_point.x).max(right_point.x));
            }
        }
        rightmost
    };

    let right_bound = if position == WidgetPosition::Left {
        parent_rect.right - margin
    } else {
        let notify_left = local_edge("TrayNotifyWnd", false);
        notify_left
            .map(|l| l - margin)
            .unwrap_or(parent_rect.right - margin)
    };
    let x = if position == WidgetPosition::Left {
        // Prefer the space immediately after an existing taskbar status
        // component. Fall back to the physical left edge only when Explorer
        // exposes no such child window.
        status_component_right()
            .map(|right| right + margin)
            .unwrap_or(margin)
    } else {
        let band_right = local_edge("ReBarWindow32", true);
        let left_bound = band_right.map(|r| r + margin).unwrap_or(margin);
        (right_bound - desired_width).max(left_bound)
    };
    let width = (right_bound - x).min(desired_width);
    if width <= 0 {
        return None;
    }
    // Geometry is measured in screen coordinates so it can be converted to
    // the taskbar-child client coordinates immediately before SetWindowPos.
    let y = screen_parent.top + (taskbar_height - height) / 2;
    Some((screen_parent.left + x, y, width, height))
}

/// Convert a measured screen rectangle to the taskbar child's client
/// coordinates. TrafficMonitor does the same conversion after it has a
/// `Shell_TrayWnd` parent; using `ScreenToClient` avoids guessing about the
/// taskbar's non-client border or DPI.
fn child_rect(parent: isize, rect: (i32, i32, i32, i32)) -> Option<(i32, i32, i32, i32)> {
    let (x, y, w, h) = rect;
    let mut point = Point { x, y };
    if unsafe { ScreenToClient(parent, &raw mut point) } == 0 {
        return None;
    }
    Some((point.x, point.y, w, h))
}

fn current_position() -> WidgetPosition {
    WIDGET_POSITION
        .lock()
        .map(|position| *position)
        .unwrap_or(WidgetPosition::Notification)
}

/// Updates placement immediately when the Settings page changes it.
pub fn set_position(position: &str) {
    let next = if position.eq_ignore_ascii_case("left") {
        WidgetPosition::Left
    } else {
        WidgetPosition::Notification
    };
    if let Ok(mut current) = WIDGET_POSITION.lock() {
        *current = next;
    }
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
        reassert(hwnd);
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
    }
}

pub fn set_font_weight(weight: u16) {
    let value = codexbar::settings::normalize_taskbar_widget_font_weight(weight) as i32;
    if let Ok(mut current) = WIDGET_FONT_WEIGHT.lock() {
        *current = value;
    }
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
    }
}

pub fn set_font_family(family: &str) {
    if let Ok(mut current) = WIDGET_FONT_FAMILY.lock() {
        *current = family.trim().to_string();
    }
    repaint();
}

pub fn set_font_size(size: u8) {
    if let Ok(mut current) = WIDGET_FONT_SIZE.lock() {
        *current = i32::from(size.clamp(10, 16));
    }
    repaint();
}

pub fn set_width(width: u16) {
    if let Ok(mut current) = WIDGET_WIDTH.lock() {
        *current = i32::from(width.clamp(96, 240));
    }
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
        reassert(hwnd);
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
    }
}

pub fn set_text_align(align: &str) {
    let value = match align {
        "center" => DT_CENTER,
        "right" => DT_RIGHT,
        _ => DT_LEFT,
    };
    if let Ok(mut current) = WIDGET_TEXT_ALIGN.lock() {
        *current = value;
    }
    repaint();
}

fn repaint() {
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
    }
}

pub fn set_content(content: &str) {
    let value = match content {
        "speed" | "usage_speed" => content,
        _ => "usage",
    };
    if let Ok(mut current) = WIDGET_CONTENT.lock() {
        *current = value.to_string();
    }
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
        unsafe { InvalidateRect(hwnd, std::ptr::null(), 1) };
    }
}

pub fn content() -> String {
    WIDGET_CONTENT
        .lock()
        .map(|value| {
            if value.is_empty() {
                "usage".to_string()
            } else {
                value.clone()
            }
        })
        .unwrap_or_else(|_| "usage".to_string())
}

// ── Hover tooltip (TASK-021 item 9) ─────────────────────────────────────────
//
// Standard Win32 `tooltips_class32` control with `TTF_SUBCLASS`: the tooltip
// control subclasses the target window itself (installs its own WNDPROC ahead
// of ours, then chains to ours for anything it doesn't need) and drives its
// own `WM_MOUSEMOVE`/hover/leave bookkeeping — no manual `TrackMouseEvent`
// state machine needed here, and it does not disturb the strip's or the hit
// proxy's own message handling below.
//
// The tool is registered against BOTH `hwnd` and the hit proxy, mirroring the
// same "either one might be the window that actually receives the mouse"
// uncertainty the file already documents for right-click handling — cheaper
// to cover both than to guess.

/// Composes the same tooltip text the notification-area tray icon shows.
///
/// Reuses `tray_bridge::build_tooltip` (made `pub(crate)` there for this)
/// rather than re-deriving which window/provider resolves and how a balance
/// or error line prints — see that function's own doc comment for why a
/// second copy of those rules would eventually drift from the original.
fn compose_tooltip_text() -> String {
    let Some(app) = APP_HANDLE.get() else {
        return "CodexBar Desktop".to_string();
    };
    let settings = codexbar::settings::Settings::load();
    let snapshots = app
        .try_state::<Mutex<crate::state::AppState>>()
        .map(|state| state.lock().unwrap().provider_cache.clone())
        .unwrap_or_default();
    crate::tray_bridge::build_tooltip(&snapshots, settings.ui_language, &settings.taskbar_tooltip_entries)
}

/// Registers one tool (`target`) on the shared tooltip control, with the
/// current composed text as its initial content.
fn add_tooltip_tool(tooltip: isize, target: isize, instance: isize) {
    if target == 0 {
        return;
    }
    let mut text = wide(&compose_tooltip_text());
    let mut info = ToolInfoW {
        cb_size: std::mem::size_of::<ToolInfoW>() as u32,
        u_flags: TTF_SUBCLASS | TTF_IDISHWND,
        hwnd: target,
        u_id: target as usize,
        rect: Rect::default(),
        h_inst: instance,
        lpsz_text: text.as_mut_ptr(),
        l_param: 0,
        lp_reserved: std::ptr::null_mut(),
    };
    unsafe { SendMessageW(tooltip, TTM_ADDTOOLW, 0, &raw mut info as isize) };
}

/// Creates the strip's own hover tooltip and attaches it to both the painter
/// window and the hit proxy. Called once per `start()`, mirroring how
/// `WIDGET_HWND`/`HIT_PROXY_HWND` are themselves created once per `start()`.
fn create_tooltip(hwnd: isize, proxy: isize) {
    if TOOLTIP_HWND.lock().map(|g| *g).unwrap_or(0) != 0 {
        return;
    }
    let icc = InitCommonControlsExStruct {
        dw_size: std::mem::size_of::<InitCommonControlsExStruct>() as u32,
        dw_icc: ICC_WIN95_CLASSES,
    };
    unsafe { InitCommonControlsEx(&raw const icc) };

    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let tooltip = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            wide("tooltips_class32").as_ptr(),
            std::ptr::null(),
            WS_POPUP | TTS_ALWAYSTIP | TTS_NOPREFIX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            hwnd,
            0,
            instance,
            std::ptr::null_mut(),
        )
    };
    if tooltip == 0 {
        tracing::warn!("taskbar widget: tooltip window creation failed");
        return;
    }

    add_tooltip_tool(tooltip, proxy, instance);
    add_tooltip_tool(tooltip, hwnd, instance);
    unsafe {
        // Enables word-wrap sizing and, with it, honors the "\n" line breaks
        // `build_tooltip` joins its rows with — a plain tooltip otherwise
        // renders embedded newlines as garbage rather than separate lines.
        SendMessageW(tooltip, TTM_SETMAXTIPWIDTH, 0, 320);
        SendMessageW(tooltip, TTM_ACTIVATE, 1, 0);
    }

    if let Ok(mut guard) = TOOLTIP_HWND.lock() {
        *guard = tooltip;
    }
}

/// Pushes fresh tooltip text to both registered tools. Called from
/// `set_entries`, the same call site `tray_bridge` uses right before it
/// rebuilds the tray icon's own tooltip — see that call site's comment for why
/// this beat was chosen over the 1-second reassert timer.
fn update_tooltip_text() {
    let tooltip = TOOLTIP_HWND.lock().map(|g| *g).unwrap_or(0);
    if tooltip == 0 || unsafe { IsWindow(tooltip) } == 0 {
        return;
    }
    let text = compose_tooltip_text();
    let proxy = HIT_PROXY_HWND.lock().map(|g| *g).unwrap_or(0);
    let widget = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    for target in [proxy, widget] {
        if target == 0 {
            continue;
        }
        let mut wide_text = wide(&text);
        let mut info = ToolInfoW {
            cb_size: std::mem::size_of::<ToolInfoW>() as u32,
            u_flags: TTF_SUBCLASS | TTF_IDISHWND,
            hwnd: target,
            u_id: target as usize,
            rect: Rect::default(),
            h_inst: 0,
            lpsz_text: wide_text.as_mut_ptr(),
            l_param: 0,
            lp_reserved: std::ptr::null_mut(),
        };
        unsafe { SendMessageW(tooltip, TTM_UPDATETIPTEXTW, 0, &raw mut info as isize) };
    }
}

fn reassert(hwnd: isize) {
    let parent = taskbar();
    if parent == 0 {
        return;
    }
    if let Some(rect) = target_rect(parent, current_position()) {
        let (x, y, w, h) = child_rect(parent, rect).unwrap_or(rect);
        let proxy = HIT_PROXY_HWND.lock().map(|g| *g).unwrap_or(0);
        unsafe {
            SetWindowPos(hwnd, 0, x, y, w, h, SWP_NOACTIVATE | SWP_NOZORDER);
            if proxy != 0 && IsWindow(proxy) != 0 {
                SetWindowPos(proxy, hwnd, x, y, w, h, SWP_NOACTIVATE);
            }
        }
    } else {
        // Geometry unknown this tick — keep the existing position and only
        // reassert topmost order until Explorer exposes valid geometry again.
        unsafe {
            SetWindowPos(
                hwnd,
                0,
                0,
                0,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOZORDER | SWP_NOMOVE | SWP_NOSIZE,
            )
        };
    }
}

/// Builds the strip's right-click menu and hands it to the self-drawn popup in
/// [`crate::taskbar_menu`].
///
/// This window stays the menu's owner: the chosen row's id comes back as a
/// `WM_COMMAND`, exactly the shape `TPM_RETURNCMD` used to deliver, so
/// `handle_context_command` needs no change. Unlike `TrackPopupMenu` the call
/// returns immediately rather than running a modal loop.
/// The string ids of the rows in the menu currently on screen, in order.
///
/// The self-drawn menu posts a `usize` back through `WM_COMMAND`, but the
/// actions are identified by the same string ids the notification-area tray
/// menu uses (`"refresh"`, `"toggle_provider:codex"`, …) so both menus share
/// one set of handlers. This is the translation table: a row's 1-based index
/// is what travels, and it is resolved here. Index 0 is never used, because
/// `WM_COMMAND` wparam 0 is indistinguishable from "no selection".
static MENU_COMMAND_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Local-only id for the strip's own visibility toggle. Not a tray menu id —
/// the tray has no equivalent row, so it is handled before delegating.
const STRIP_TOGGLE_ID: &str = "toggle_taskbar_strip";

/// Converts the tray menu tree into the flat row list the self-drawn menu
/// draws, and records each row's id.
///
/// **Submenu children are not expanded.** The row count has to match the tray
/// icon's menu, and there a submenu is a single row — expanding 提供方 into a
/// header plus one row per provider turned a nine-row menu into an arbitrarily
/// long one. The parent stays one row; [`handle_context_command`] sends it
/// somewhere that can show the children.
fn flatten_menu_entries(
    entries: &[crate::tray_menu::TrayMenuEntry],
    items: &mut Vec<crate::taskbar_menu::MenuItem>,
    ids: &mut Vec<String>,
) {
    use crate::taskbar_menu::MenuItem;

    for entry in entries {
        if entry.is_separator {
            items.push(MenuItem::separator());
            ids.push(String::new());
            continue;
        }

        let mut item = MenuItem::action(ids.len() + 1, entry.label.clone());
        if let Some(checked) = entry.checked {
            item = item.checked(checked);
        }
        item.disabled = entry.disabled;
        items.push(item);
        ids.push(entry.id.clone().unwrap_or_default());
    }
}

/// Places the strip's visibility toggle immediately after the floating bar's,
/// and renumbers every row.
///
/// Not appended: the two toggles do the same kind of thing — show or hide one
/// of the app's surfaces — and belong in the same group. Appending put this one
/// below Quit, which is the grouping defect the user reported.
///
/// The renumbering is the part that matters. A row's id *is* its 1-based
/// position, so inserting anywhere but the end invalidates every id after it.
fn insert_strip_toggle(
    items: &mut Vec<crate::taskbar_menu::MenuItem>,
    ids: &mut Vec<String>,
    label: String,
    checked: bool,
) {
    let insert_at = ids
        .iter()
        .position(|id| id == "toggle_float_bar")
        .map(|index| index + 1)
        .unwrap_or(items.len());
    items.insert(
        insert_at,
        crate::taskbar_menu::MenuItem::action(0, label).checked(checked),
    );
    ids.insert(insert_at, STRIP_TOGGLE_ID.to_string());
    for (position, item) in items.iter_mut().enumerate() {
        item.id = position + 1;
    }
}

fn show_context_menu(hwnd: isize) {
    use crate::taskbar_menu::MenuItem;
    use codexbar::locale::{LocaleKey, get_text};

    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let settings = codexbar::settings::Settings::load();

    // Content comes from `build_tray_menu`, the same builder the
    // notification-area menu uses, so the two can no longer drift apart. The
    // strip's `taskbar_context_menu_actions` setting no longer selects rows —
    // it described a four-entry menu that this replaces.
    let mut items: Vec<MenuItem> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    flatten_menu_entries(&crate::tray_bridge::tray_menu_spec(app), &mut items, &mut ids);

    // The strip's own visibility toggle has no tray equivalent, so it is added
    // here rather than coming from the builder — but it is *inserted next to
    // the floating bar's toggle*, not appended. The two do the same kind of
    // thing (show or hide one of the app's surfaces) and belong in the same
    // group; appending put this one below Quit, which is the grouping defect
    // the user reported. Named with item 7's vocabulary (小型状态栏), not
    // 任务栏: the strip lives *inside* the Windows taskbar but is not it.
    insert_strip_toggle(
        &mut items,
        &mut ids,
        get_text(
            settings.ui_language,
            LocaleKey::TaskbarContextMenuShowStrip,
        ),
        settings.taskbar_widget_enabled,
    );

    if let Ok(mut guard) = MENU_COMMAND_IDS.lock() {
        *guard = ids;
    }
    crate::taskbar_menu::show(hwnd, items);
}

fn handle_context_command(index: usize) {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let id = MENU_COMMAND_IDS
        .lock()
        .ok()
        .and_then(|ids| ids.get(index.wrapping_sub(1)).cloned())
        .unwrap_or_default();
    if id.is_empty() {
        return;
    }

    if id == STRIP_TOGGLE_ID {
        // Same read-modify-save-apply pattern `commands/settings.rs` uses for
        // this exact setting (`taskbar_widget_enabled`).
        let mut settings = codexbar::settings::Settings::load();
        let next = !settings.taskbar_widget_enabled;
        settings.taskbar_widget_enabled = next;
        let _ = settings.save();
        set_enabled(next);
        // Every other writer of this setting goes through `update_settings`,
        // which broadcasts afterwards. Without the same broadcast here the
        // Settings window keeps rendering its stale snapshot — its toggle still
        // reads "on", so the next click sends `false` against a setting that is
        // already false, and the strip looks impossible to turn back on.
        crate::events::emit_settings_changed(app);
        return;
    }

    if id == "providers" {
        // The tray menu opens a real submenu here. This menu has none, so the
        // row goes to the page that owns the same toggles instead of being a
        // dead end — building a nested flyout (hover-open timing, a second
        // window, keyboard descent) is a larger piece of work than this row is
        // worth, and is recorded in the backlog if it turns out to be wanted.
        let _ = crate::shell::settings_window::open_or_focus(app, "providers");
        return;
    }

    crate::tray_bridge::dispatch_menu_id(app, &id);
}

fn taskbar_background_color(light: bool) -> u32 {
    let parent = taskbar();
    if parent != 0 {
        let mut parent_screen = Rect::default();
        let mut widget_screen = Rect::default();
        if unsafe { GetWindowRect(parent, &raw mut parent_screen) } != 0
            && unsafe {
                GetWindowRect(
                    WIDGET_HWND.lock().map(|g| *g).unwrap_or(0),
                    &raw mut widget_screen,
                )
            } != 0
        {
            let sample_x = if widget_screen.left > parent_screen.left + 2 {
                widget_screen.left - 2
            } else {
                (widget_screen.right + 2).min(parent_screen.right - 1)
            };
            let sample_y = (parent_screen.top + parent_screen.bottom) / 2;
            // Sample the real desktop surface, not Shell_TrayWnd's client DC;
            // Explorer's XAML taskbar background is otherwise reported as
            // white even when the visible acrylic surface is tinted.
            let hdc = unsafe { GetDC(0) };
            if hdc != 0 {
                let color = unsafe { GetPixel(hdc, sample_x, sample_y) };
                unsafe { ReleaseDC(0, hdc) };
                if color != 0xFFFF_FFFF {
                    return color;
                }
            }
        }
    }
    if light { 0x00E9_EFEE } else { 0x0020_2020 }
}

unsafe extern "system" fn wnd_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        // Painting is fully covered by WM_PAINT; letting the default proc
        // erase first only produces a flash of the class brush.
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            unsafe { paint(hwnd) };
            0
        }
        WM_TIMER if wparam == REASSERT_TIMER_ID => {
            reassert(hwnd);
            unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
            0
        }
        WM_NCHITTEST => HTCLIENT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_RBUTTONUP => {
            show_context_menu(hwnd);
            0
        }
        WM_COMMAND => {
            handle_context_command(wparam & 0xffff);
            0
        }
        WM_DESTROY => {
            unsafe { KillTimer(hwnd, REASSERT_TIMER_ID) };
            if let Ok(mut guard) = WIDGET_HWND.lock() {
                *guard = 0;
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn hit_proxy_proc(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = PaintStruct {
                hdc: 0,
                f_erase: 0,
                rc_paint: Rect::default(),
                f_restore: 0,
                f_inc_update: 0,
                rgb_reserved: [0; 32],
            };
            let hdc = unsafe { BeginPaint(hwnd, &raw mut ps) };
            if hdc != 0 {
                unsafe { EndPaint(hwnd, &raw const ps) };
            }
            0
        }
        WM_NCHITTEST => HTCLIENT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_RBUTTONUP => {
            show_context_menu(hwnd);
            0
        }
        WM_COMMAND => {
            handle_context_command(wparam & 0xffff);
            0
        }
        WM_DESTROY => {
            if let Ok(mut guard) = HIT_PROXY_HWND.lock() {
                *guard = 0;
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Where each visible entry is drawn, filling **column-major**.
///
/// Column-major so one and two entries keep exactly the layout they had before
/// a grid existed: adding a third entry opens a second column instead of
/// reflowing entry 2 from the bottom-left to the top-right. It also groups the
/// way the reference layout does — a column is a pair that belongs together
/// (up/down, or one provider's two windows), not two unrelated readings.
///
/// The row count follows the entry count up to [`MAX_VISIBLE_ROWS`], so a
/// single entry still spans the full height rather than sitting in the top half
/// of an otherwise empty grid.
fn cell_rects(client: &Rect, count: usize, pad: i32) -> Vec<Rect> {
    let count = count.max(1).min(MAX_VISIBLE_ENTRIES);
    let columns = count.div_ceil(MAX_VISIBLE_ROWS).max(1);
    let rows = count.div_ceil(columns).max(1);

    let inner_left = client.left + pad;
    let inner_right = client.right - pad;
    let inner_width = (inner_right - inner_left).max(0);
    let height = client.bottom - client.top;
    // Only between columns, so a single-column strip is laid out exactly as
    // before and the text still starts at the same x.
    let gutter = if columns > 1 { pad } else { 0 };

    (0..count)
        .map(|index| {
            let column = (index / rows) as i32;
            let row = (index % rows) as i32;
            let columns = columns as i32;
            let rows = rows as i32;
            let left = inner_left + (inner_width * column) / columns;
            let right = inner_left + (inner_width * (column + 1)) / columns;
            Rect {
                left,
                top: client.top + (height * row) / rows,
                // The gutter comes out of this column's own width, so the last
                // column still ends on the padded right edge.
                right: (right - if column + 1 < columns { gutter } else { 0 }).max(left),
                bottom: client.top + (height * (row + 1)) / rows,
            }
        })
        .collect()
}

unsafe fn paint(hwnd: isize) {
    let mut ps = PaintStruct {
        hdc: 0,
        f_erase: 0,
        rc_paint: Rect::default(),
        f_restore: 0,
        f_inc_update: 0,
        rgb_reserved: [0; 32],
    };
    let hdc = unsafe { BeginPaint(hwnd, &raw mut ps) };
    tracing::debug!("taskbar widget: paint hdc={hdc:#x}");
    if hdc == 0 {
        return;
    }

    let mut client = Rect::default();
    unsafe { GetClientRect(hwnd, &raw mut client) };

    let light = taskbar_is_light();
    let text = if light {
        0x003A_3A3Au32
    } else {
        0x00FF_FFFFu32
    };
    // Use the adjacent taskbar pixel as both the temporary GDI canvas and the
    // color key. Antialiased glyph edges therefore blend with Explorer's real
    // surface instead of producing magenta fringes or an opaque rectangle.
    let background = taskbar_background_color(light);
    let brush = unsafe { CreateSolidBrush(background) };
    unsafe {
        FillRect(hdc, &raw const client, brush);
        DeleteObject(brush);
        SetLayeredWindowAttributes(hwnd, background, 0, LWA_COLORKEY);
    }

    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let font_size = WIDGET_FONT_SIZE.lock().map(|size| *size).unwrap_or(12);
    let size_px = ((font_size * dpi as i32) as f32) / 96.0;
    let font_weight = WIDGET_FONT_WEIGHT
        .lock()
        .map(|weight| *weight)
        .unwrap_or(400) as f32;
    let family = widget_font_family();

    let entries = LINES.lock().map(|g| g.clone()).unwrap_or_default();
    let pad = (6 * dpi as i32) / 96;
    // Only as many entries as physically fit; the list is ordered, so this keeps
    // the ones the user put first.
    let visible: Vec<&StripLine> = entries.iter().take(MAX_VISIBLE_ENTRIES).collect();
    let line_rects = cell_rects(&client, visible.len(), pad);
    let align = WIDGET_TEXT_ALIGN
        .lock()
        .map(|align| *align)
        .unwrap_or(DT_LEFT);

    // DirectWrite, not GDI. `CreateFontW` cannot reach a font's OpenType
    // variation axes, so it collapsed every requested weight to Regular or
    // Bold; `taskbar_text` drives the `wght` axis directly and was measured to
    // render distinct strokes for values between the named stops.
    let to_win_rect = |r: &Rect| crate::taskbar_text::WinRect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    };
    let text_lines: Vec<crate::taskbar_text::TextLine<'_>> = visible
        .iter()
        .zip(line_rects.iter())
        .map(|(line, rect)| crate::taskbar_text::TextLine {
            text: line.text.as_str(),
            rect: to_win_rect(rect),
            mark: line.mark.map(|mark| crate::taskbar_text::LineMark {
                glyph: mark.glyph,
                color_rgb: mark.color_rgb,
            }),
        })
        .collect();
    let drawn = crate::taskbar_text::draw_lines(
        hdc,
        to_win_rect(&client),
        &text_lines,
        &crate::taskbar_text::TextStyle {
            family: &family,
            weight: font_weight,
            size_px,
            align: match align {
                DT_CENTER => crate::taskbar_text::TextAlign::Center,
                DT_RIGHT => crate::taskbar_text::TextAlign::Right,
                _ => crate::taskbar_text::TextAlign::Left,
            },
            color_rgb: text,
        },
    );

    // GDI fallback. If DirectWrite is unavailable the strip must still show
    // something readable rather than an empty rectangle over the taskbar.
    if !drawn {
        let font_height = -((font_size * dpi as i32) / 96);
        let font = unsafe {
            CreateFontW(
                font_height, 0, 0, 0, font_weight as i32, 0, 0, 0, 1, 0, 0, 4, 0,
                wide(&family).as_ptr(),
            )
        };
        let previous = unsafe { SelectObject(hdc, font) };
        unsafe {
            SetBkMode(hdc, TRANSPARENT_BK);
            SetTextColor(hdc, text);
        }
        let format = align | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX | DT_END_ELLIPSIS;
        for (line, rect) in visible.iter().zip(line_rects.iter()) {
            let mut r = *rect;
            // GDI has one text colour per DC, so the fallback prints the mark
            // inline and monochrome rather than in the brand colour. Losing the
            // colour is acceptable here; losing the reading is not.
            let body = match line.mark {
                Some(mark) => format!("{} {}", mark.glyph, line.text),
                None => line.text.clone(),
            };
            let wide_text = wide(&body);
            unsafe { DrawTextW(hdc, wide_text.as_ptr(), -1, &raw mut r, format) };
        }
        unsafe {
            SelectObject(hdc, previous);
            DeleteObject(font);
        }
    }

    unsafe {
        EndPaint(hwnd, &raw const ps);
    }
}

/// Reads the persisted setting at startup and creates the strip if the user
/// previously enabled it. Failure is logged, never fatal: the taskbar child is
/// a convenience surface and must not take the app down with it.
pub fn install() {
    let settings = codexbar::settings::Settings::load();
    set_position(&settings.taskbar_widget_position);
    set_font_weight(settings.taskbar_widget_font_weight);
    set_content(&settings.taskbar_widget_content);
    set_font_family(&settings.taskbar_widget_font_family);
    set_font_size(settings.taskbar_widget_font_size);
    set_width(settings.taskbar_widget_width);
    set_text_align(&settings.taskbar_widget_text_align);
    if !settings.taskbar_widget_enabled {
        return;
    }
    if let Err(err) = start() {
        tracing::warn!("taskbar widget unavailable: {err}");
    }
}

/// Applies a live toggle: creates the strip when turned on, destroys it when
/// turned off. No-op if already in the requested state.
///
/// **Marshals to the main thread**, because both halves are thread-affine and
/// its main caller is not on it. `update_settings` is an `async` Tauri command,
/// so it runs on the async runtime's worker pool:
///
/// * `start()` there would create the window on a thread with no message pump.
///   `CreateWindowExW` succeeds, so the strip looks created and `WIDGET_HWND`
///   goes non-zero — but nothing ever dispatches its `WM_PAINT`, its reassert
///   timer or its right-click, and because `start()` early-returns while
///   `WIDGET_HWND` is set, no later call can repair it.
/// * `DestroyWindow` outright refuses to destroy a window owned by another
///   thread, so `stop()` there left the strip on screen while the app forgot
///   about it — and the next enable would build a second one.
///
/// Turning the strip off from the right-click menu and then back on from
/// Settings hit both halves in sequence, which is what made the Settings toggle
/// look dead.
pub fn set_enabled(enabled: bool) {
    let Some(app) = APP_HANDLE.get() else {
        // Before `set_app_handle` — only reachable from `install()`'s own
        // thread, which is already the right one.
        apply_enabled(enabled);
        return;
    };
    let app = app.clone();
    if app.run_on_main_thread(move || apply_enabled(enabled)).is_err() {
        tracing::warn!("taskbar widget toggle could not reach the main thread");
    }
}

fn apply_enabled(enabled: bool) {
    if enabled {
        if let Err(err) = start() {
            tracing::warn!("taskbar widget unavailable: {err}");
        }
    } else {
        stop();
    }
}

/// Destroys the strip window, if any. Safe to call when it doesn't exist.
pub fn stop() {
    // Destroyed first: the tooltip control subclassed both target windows
    // (`TTF_SUBCLASS`), and tearing it down before its subclassed targets are
    // themselves destroyed lets its own `WM_NCDESTROY` handling un-subclass
    // them cleanly.
    let tooltip = TOOLTIP_HWND.lock().map(|g| *g).unwrap_or(0);
    if tooltip != 0 && unsafe { IsWindow(tooltip) } != 0 {
        unsafe { DestroyWindow(tooltip) };
    }
    if let Ok(mut guard) = TOOLTIP_HWND.lock() {
        *guard = 0;
    }
    let proxy = HIT_PROXY_HWND.lock().map(|g| *g).unwrap_or(0);
    if proxy != 0 && unsafe { IsWindow(proxy) } != 0 {
        unsafe { DestroyWindow(proxy) };
    }
    if let Ok(mut guard) = HIT_PROXY_HWND.lock() {
        *guard = 0;
    }
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
        unsafe { DestroyWindow(hwnd) };
    }
    if let Ok(mut guard) = WIDGET_HWND.lock() {
        *guard = 0;
    }
}

/// Creates the strip. No-op when the taskbar cannot be found or the window
/// already exists.
///
/// Must be called from the thread that runs the app's message loop: the child
/// lives in explorer's window tree but its messages are dispatched by us.
pub fn start() -> Result<(), String> {
    if WIDGET_HWND.lock().map(|g| *g).unwrap_or(0) != 0 {
        return Ok(());
    }

    let parent = taskbar();
    if parent == 0 {
        return Err("Shell_TrayWnd not found".into());
    }

    let class_name = wide("CodexBarTaskbarWidget");
    let hit_class_name = wide("CodexBarTaskbarWidgetHit");
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class = WndClassW {
        style: 0,
        lpfn_wnd_proc: Some(wnd_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: instance,
        h_icon: 0,
        h_cursor: 0,
        hbr_background: 0,
        lpsz_menu_name: std::ptr::null(),
        lpsz_class_name: class_name.as_ptr(),
    };
    // A duplicate registration is fine — only the first call in a process
    // succeeds and the class persists.
    unsafe { RegisterClassW(&raw const class) };
    let hit_class = WndClassW {
        style: 0,
        lpfn_wnd_proc: Some(hit_proxy_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: instance,
        h_icon: 0,
        h_cursor: 0,
        hbr_background: 0,
        lpsz_menu_name: std::ptr::null(),
        lpsz_class_name: hit_class_name.as_ptr(),
    };
    unsafe { RegisterClassW(&raw const hit_class) };

    let screen_rect =
        target_rect(parent, current_position()).ok_or("taskbar geometry unavailable")?;
    let (x, y, w, h) = child_rect(parent, screen_rect).unwrap_or(screen_rect);

    // Match TrafficMonitor: create a native popup first, then attach it to
    // Explorer's taskbar and convert it to a child window. Creating first is
    // important for the cross-process SetParent path.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name.as_ptr(),
            wide("TokenBar Taskbar").as_ptr(),
            WS_POPUP | WS_SYSMENU,
            x,
            y,
            w,
            h,
            0,
            0,
            instance,
            std::ptr::null_mut(),
        )
    };
    if hwnd == 0 {
        return Err("CreateWindowExW failed".into());
    }

    unsafe {
        SetParent(hwnd, parent);
        SetWindowLongPtrW(
            hwnd,
            GWL_STYLE,
            (WS_CHILD | WS_VISIBLE | WS_SYSMENU) as isize,
        );
    }
    if unsafe { GetParent(hwnd) } != parent {
        unsafe { DestroyWindow(hwnd) };
        return Err("SetParent did not attach taskbar widget to Shell_TrayWnd".into());
    }

    unsafe {
        SetWindowPos(
            hwnd,
            0,
            x,
            y,
            w,
            h,
            SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOZORDER,
        );
        ShowWindow(hwnd, SW_SHOW);
    }

    let proxy = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            hit_class_name.as_ptr(),
            wide("TokenBar Taskbar Hit").as_ptr(),
            WS_CHILD | WS_VISIBLE,
            x,
            y,
            w,
            h,
            parent,
            0,
            instance,
            std::ptr::null_mut(),
        )
    };
    if proxy == 0 {
        unsafe { DestroyWindow(hwnd) };
        return Err("CreateWindowExW hit proxy failed".into());
    }
    unsafe {
        SetWindowPos(proxy, hwnd, x, y, w, h, SWP_NOACTIVATE);
        ShowWindow(proxy, SW_SHOW);
    }
    if let Ok(mut guard) = HIT_PROXY_HWND.lock() {
        *guard = proxy;
    }

    if let Ok(mut guard) = WIDGET_HWND.lock() {
        *guard = hwnd;
    }
    // TASK-021 item 9: hover tooltip, driven by the same data the
    // notification-area tray icon's tooltip uses.
    create_tooltip(hwnd, proxy);
    unsafe {
        SetTimer(
            hwnd,
            REASSERT_TIMER_ID,
            REASSERT_INTERVAL_MS,
            std::ptr::null(),
        )
    };
    reassert(hwnd);

    tracing::info!(
        "taskbar widget: hwnd={hwnd:#x} hit_proxy={proxy:#x} host={parent:#x} rect=({x},{y},{w},{h}) mode=native-child"
    );
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;

    /// 200 wide, 40 tall, 6px padding — roughly the real strip at 96 DPI.
    fn client() -> Rect {
        Rect { left: 0, top: 0, right: 200, bottom: 40 }
    }

    /// One entry keeps the whole strip, exactly as before the grid existed.
    #[test]
    fn a_single_entry_spans_the_full_height() {
        let rects = cell_rects(&client(), 1, 6);
        assert_eq!(rects.len(), 1);
        assert_eq!((rects[0].top, rects[0].bottom), (0, 40));
        assert_eq!((rects[0].left, rects[0].right), (6, 194));
    }

    /// Two entries stack, also exactly as before — no reflow for existing users.
    #[test]
    fn two_entries_stack_in_one_column() {
        let rects = cell_rects(&client(), 2, 6);
        assert_eq!(rects.len(), 2);
        assert_eq!((rects[0].top, rects[0].bottom), (0, 20));
        assert_eq!((rects[1].top, rects[1].bottom), (20, 40));
        // Both still span the full padded width.
        for r in &rects {
            assert_eq!((r.left, r.right), (6, 194));
        }
    }

    /// The point of the change: a third entry opens a column, it does not add a
    /// row the taskbar has no height for.
    #[test]
    fn a_third_entry_opens_a_second_column_rather_than_a_third_row() {
        let rects = cell_rects(&client(), 3, 6);
        assert_eq!(rects.len(), 3);
        for r in &rects {
            assert!(r.bottom - r.top == 20, "rows stay half the strip: {r:?}");
        }
        // Column-major: 1 and 2 fill the left column, 3 starts the right one.
        assert_eq!((rects[0].top, rects[1].top, rects[2].top), (0, 20, 0));
        assert_eq!(rects[0].left, rects[1].left);
        assert!(rects[2].left > rects[0].left);
    }

    #[test]
    fn four_entries_fill_a_two_by_two_grid_column_major() {
        let rects = cell_rects(&client(), 4, 6);
        assert_eq!(rects.len(), 4);
        // Left column top/bottom, then right column top/bottom.
        assert_eq!((rects[0].top, rects[0].bottom), (0, 20));
        assert_eq!((rects[1].top, rects[1].bottom), (20, 40));
        assert_eq!((rects[2].top, rects[2].bottom), (0, 20));
        assert_eq!((rects[3].top, rects[3].bottom), (20, 40));
        assert_eq!(rects[0].left, rects[1].left);
        assert_eq!(rects[2].left, rects[3].left);
        // Rows line up across columns, which is what makes it read as a grid.
        assert_eq!(rects[0].top, rects[2].top);
    }

    /// A gutter must separate the columns, or the two readings run together.
    #[test]
    fn columns_do_not_touch() {
        let rects = cell_rects(&client(), 4, 6);
        assert!(
            rects[2].left - rects[0].right >= 6,
            "left column ends at {}, right starts at {}",
            rects[0].right,
            rects[2].left
        );
        // The gutter comes out of the left column, so the right one still ends
        // on the padded edge.
        assert_eq!(rects[2].right, 194);
    }

    /// More entries than fit are dropped by the caller; the layout must not
    /// invent a third column for them if one slips through.
    #[test]
    fn never_lays_out_more_than_the_strip_can_show() {
        assert_eq!(cell_rects(&client(), 9, 6).len(), MAX_VISIBLE_ENTRIES);
        // An empty list still yields one rect, so an empty strip paints its
        // placeholder instead of nothing at all.
        assert_eq!(cell_rects(&client(), 0, 6).len(), 1);
    }

    /// A strip narrower than its own padding must not produce inverted rects,
    /// which DirectWrite would reject and leave the strip blank.
    #[test]
    fn degenerate_widths_stay_ordered() {
        let narrow = Rect { left: 0, top: 0, right: 8, bottom: 40 };
        for r in cell_rects(&narrow, 4, 6) {
            assert!(r.right >= r.left, "inverted rect: {r:?}");
            assert!(r.bottom >= r.top, "inverted rect: {r:?}");
        }
    }


    /// The flattened list is what the menu draws; `ids` is what a click
    /// resolves through. They must stay index-aligned, because the row's
    /// 1-based position is the only thing that travels back in `WM_COMMAND`.
    #[test]
    fn flattening_keeps_rows_and_ids_aligned() {
        use crate::tray_menu::build_tray_menu_with;
        use codexbar::settings::Language;

        let catalog = vec![crate::commands::ProviderCatalogEntry {
            id: "codex".into(),
            display_name: "Codex".into(),
            cookie_domain: None,
        }];
        let enabled = ["codex".to_string()].into_iter().collect();
        let spec = build_tray_menu_with(
            &catalog,
            &[("codex".to_string(), "Codex 30%".to_string())],
            &enabled,
            true,
            Language::English,
        );

        let mut items = Vec::new();
        let mut ids = Vec::new();
        flatten_menu_entries(&spec, &mut items, &mut ids);

        assert_eq!(items.len(), ids.len(), "one id per drawn row");
        for (position, item) in items.iter().enumerate() {
            if item.separator {
                assert!(ids[position].is_empty(), "separators carry no action");
            } else {
                assert_eq!(item.id, position + 1, "ids are 1-based row positions");
            }
        }

        // The provider submenu stays ONE row. Expanding it is what made the
        // menu balloon past the tray icon's own length, which is the shape
        // this menu is supposed to match.
        assert!(
            ids.iter().any(|id| id == "providers"),
            "the providers row is present"
        );
        assert!(
            !ids.iter().any(|id| id.starts_with("toggle_provider:")),
            "individual providers must NOT be expanded into rows"
        );
        assert_eq!(
            items.len(),
            spec.len(),
            "one drawn row per top-level tray entry, no more"
        );

        // Status rows come from the builder already disabled.
        let status = ids
            .iter()
            .position(|id| id == "status_codex")
            .expect("status row present");
        assert!(items[status].disabled);

        // Nothing from the old fixed list survives.
        assert!(ids.iter().any(|id| id == "show_panel"), "dashboard row");
        assert!(ids.iter().any(|id| id == "toggle_float_bar"), "float bar row");
        assert!(ids.iter().any(|id| id == "about"), "about row");
    }

    /// Inserting mid-list shifts every row after it, and a row's id *is* its
    /// position — so if the renumber is ever dropped, clicking 设置 fires 关于.
    #[test]
    fn inserting_the_strip_toggle_renumbers_every_row() {
        use crate::taskbar_menu::MenuItem;

        let mut items = vec![
            MenuItem::action(1, "Refresh".into()),
            MenuItem::action(2, "Float bar".into()),
            MenuItem::action(3, "Settings".into()),
            MenuItem::action(4, "Quit".into()),
        ];
        let mut ids = vec![
            "refresh".to_string(),
            "toggle_float_bar".to_string(),
            "settings".to_string(),
            "quit".to_string(),
        ];

        insert_strip_toggle(&mut items, &mut ids, "Show strip".into(), true);

        // Placed beside the other visibility toggle, not at the end.
        assert_eq!(ids[2], STRIP_TOGGLE_ID);
        assert_eq!(ids.last().map(String::as_str), Some("quit"));
        assert!(items[2].checked);

        assert_eq!(items.len(), ids.len());
        for (position, item) in items.iter().enumerate() {
            assert_eq!(item.id, position + 1, "row {position} carries a stale id");
        }
    }

    /// With no floating-bar row to anchor to, the toggle still has to land
    /// somewhere valid rather than being dropped.
    #[test]
    fn the_strip_toggle_falls_back_to_the_end() {
        use crate::taskbar_menu::MenuItem;

        let mut items = vec![MenuItem::action(1, "Quit".into())];
        let mut ids = vec!["quit".to_string()];
        insert_strip_toggle(&mut items, &mut ids, "Show strip".into(), false);

        assert_eq!(ids.last().map(String::as_str), Some(STRIP_TOGGLE_ID));
        assert_eq!(items.last().map(|item| item.id), Some(2));
    }
}
