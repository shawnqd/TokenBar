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
//! Everything about positioning inside the taskbar is undocumented. Geometry
//! is sampled by a disposable background worker because taskbar moves, DPI
//! changes, theme changes and Explorer restarts can invalidate it. The owner
//! thread only consumes a versioned result; it never walks Explorer's child
//! tree or samples screen pixels while painting.
//!
//! Controlled by `Settings::taskbar_widget_enabled`, surfaced as a Settings
//! toggle. Off by default.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

/// One printable cell of the strip under the `[icon][tag][value]` cluster
/// model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StripLine {
    /// The provider's brand mark, used only as the glyph fallback when the
    /// provider has no official SVG to rasterise.
    pub mark: Option<crate::provider_mark::ProviderMark>,
    /// The provider id whose official SVG is drawn into the icon slot; `None`
    /// when the entry resolved to an unknown provider (glyph fallback).
    pub icon_provider_id: Option<String>,
    /// The short dimmed tag (a cycle count, a window word, …).
    pub tag: String,
    /// The bold value run (a percentage, amount, speed, or the unavailable
    /// reason text — never a fabricated 0%/100%).
    pub value: String,
    /// Which window this cell shows (`session|weekly|daily|monthly|balance|speed|primary`).
    pub window_kind: String,
    /// Render state (`ready|loading|refreshing|stale|error|notConfigured|unsupported|unknown`).
    pub state: String,
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
/// The Shell_TrayWnd currently hosting the strip. Explorer can replace this
/// window during a shell restart or a taskbar/virtual-desktop transition, so
/// the geometry worker must never treat the handle captured at startup as
/// permanent.
static CURRENT_PARENT_HWND: Mutex<isize> = Mutex::new(0);
/// Latest parent discovered by the worker and waiting for the widget owner
/// thread to apply with SetParent. `Some(0)` means Explorer temporarily has no
/// taskbar and both windows must be hidden until one returns.
static PENDING_PARENT_HWND: Mutex<Option<isize>> = Mutex::new(None);
pub(crate) static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

/// The Explorer taskbar geometry is sampled away from the widget's window
/// procedure. Explorer owns the parent window, so querying its child tree from
/// our UI thread can synchronously wait for Explorer and make the whole strip
/// appear hung. The UI thread only consumes this cached, versioned geometry.
type ScreenRect = (i32, i32, i32, i32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Geometry {
    screen: ScreenRect,
    child: ScreenRect,
    parent: isize,
    dpi: u32,
    desktop: Option<[u8; 16]>,
}
static GEOMETRY_CACHE: Mutex<Option<Geometry>> = Mutex::new(None);
static GEOMETRY_GENERATION: AtomicU64 = AtomicU64::new(0);
/// Theme registry reads are also kept off WM_PAINT. The geometry worker
/// refreshes the system brightness value; the renderer uses alpha and never
/// samples or keys against a taskbar pixel.
static TASKBAR_THEME_CACHE: Mutex<Option<bool>> = Mutex::new(None);
/// Keep settings/refresh bursts from filling the widget's owner queue with
/// duplicate invalidations. Cleared when the owner consumes the message.
static REPAINT_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Explorer can emit several layout notifications for one visual change.
static REASSERT_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Reparenting is a window-tree mutation and therefore runs only on the
/// widget's owner thread. This flag coalesces the worker's repeated discovery
/// of the same new Shell_TrayWnd handle.
static REPARENT_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// A desktop/theme transition asks the disposable worker to refresh the system
/// brightness cache. No desktop pixel is sampled by the renderer.
static THEME_REFRESH_REQUESTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// A virtual-desktop transition invalidates the composed child surface even
/// when Explorer reports the same HWND and rectangle. Consume this on the
/// owner thread so the first frame after the transition cannot reuse stale
/// geometry or pixels; the worker publishes a fresh version before showing it
/// again.
static DESKTOP_SWITCH_PENDING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
/// System virtual-desktop notifications are delivered to this process through
/// an out-of-context WinEvent hook. The handle is kept so stop() can unhook it
/// before destroying the child windows.
static DESKTOP_SWITCH_HOOK: Mutex<isize> = Mutex::new(0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum WidgetPosition {
    Notification,
    Left,
}

static WIDGET_POSITION: Mutex<WidgetPosition> = Mutex::new(WidgetPosition::Notification);
static WIDGET_FONT_WEIGHT: Mutex<i32> = Mutex::new(400);
static WIDGET_FONT_SIZE: Mutex<i32> = Mutex::new(12);
static WIDGET_WIDTH: Mutex<i32> = Mutex::new(136);
static WIDGET_TEXT_ALIGN: Mutex<u32> = Mutex::new(0);
static WIDGET_CONTENT: Mutex<String> = Mutex::new(String::new());
static WIDGET_FONT_FAMILY: Mutex<String> = Mutex::new(String::new());
/// The global Appearance → Theme preference used by the native strip. Auto
/// means the Windows taskbar brightness; Light/Dark are explicit text-contrast
/// overrides. The setting is cached here so WM_PAINT never loads settings.json.
static WIDGET_THEME: Mutex<codexbar::settings::ThemePreference> =
    Mutex::new(codexbar::settings::ThemePreference::Auto);
/// Official-icon slot size in logical pixels, clamped 10..=18 (default 14).
static WIDGET_ICON_SIZE: Mutex<i32> = Mutex::new(14);
/// Icon render style: `pure` | `badge` | `solid` (default `pure`).
static WIDGET_ICON_STYLE: Mutex<String> = Mutex::new(String::new());
static WIDGET_ICON_GAP: Mutex<i32> = Mutex::new(5);
static WIDGET_VALUE_GAP: Mutex<i32> = Mutex::new(2);

const WS_POPUP: u32 = 0x8000_0000;
const WS_CHILD: u32 = 0x4000_0000;
const WS_SYSMENU: u32 = 0x0008_0000;
const GWL_STYLE: i32 = -16;

const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;

const SWP_FRAMECHANGED: u32 = 0x0020;
const SWP_NOZORDER: u32 = 0x0004;
/// Do not let USER32 copy pixels from the old client rectangle when the
/// native strip moves between taskbar layout states. The old rectangle can
/// belong to a third-party monitor (or to a previous strip position), and
/// copying it is how a one-frame ghost survives a reassert.
const SWP_NOCOPYBITS: u32 = 0x0100;
/// `HWND_TOP` brings the native child above Explorer's taskbar paint siblings.
/// The measured rectangle is already clamped to the free taskbar band, so this
/// only restores visibility after Explorer/DWM reorders the child tree.
const HWND_TOP: isize = 0;
const SW_HIDE: i32 = 0;
const SW_SHOW: i32 = 5;

const WM_DESTROY: u32 = 0x0002;
const WM_SIZE: u32 = 0x0005;
const WM_SETTINGCHANGE: u32 = 0x001A;
const WM_DISPLAYCHANGE: u32 = 0x007E;
const WM_PAINT: u32 = 0x000F;
const WM_DPICHANGED: u32 = 0x02E0;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_COMMAND: u32 = 0x0111;
const WM_APP_REASSERT: u32 = 0x8000 + 0x41;
const WM_APP_REPAINT: u32 = 0x8000 + 0x42;
const WM_APP_REPARENT: u32 = 0x8000 + 0x43;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_NCHITTEST: u32 = 0x0084;
const WM_MOUSEACTIVATE: u32 = 0x0021;
const HTCLIENT: isize = 1;
const MA_NOACTIVATE: isize = 3;

const SWP_NOACTIVATE: u32 = 0x0010;
const DT_SINGLELINE: u32 = 0x0020;
const DT_VCENTER: u32 = 0x0004;
const DT_LEFT: u32 = 0x0000;
const DT_CENTER: u32 = 0x0001;
const DT_RIGHT: u32 = 0x0002;
const DT_NOPREFIX: u32 = 0x0800;

const TRANSPARENT_BK: i32 = 1;

// Layered-window upload constants. The mini taskbar now uses a single
// premultiplied-alpha surface rather than a colour-keyed child plus hit proxy.
const ULW_ALPHA: u32 = 0x0000_0002;
const AC_SRC_OVER: u8 = 0;
const AC_SRC_ALPHA: u8 = 1;
const DIB_RGB_COLORS: u32 = 0;
const BI_RGB: u32 = 0;
/// Geometry sampling is deliberately slow and off the window message thread.
/// It is only used to notice taskbar/DPI/layout changes; content repainting is
/// driven by data/settings changes instead of a heartbeat. A 250ms cadence is
/// cheap on the disposable worker and closes the visible stale-frame window
/// after a desktop switch or an Explorer taskbar relayout.
const GEOMETRY_POLL_INTERVAL: Duration = Duration::from_millis(250);
/// Pixel/theme sampling is more expensive than rectangle queries, so it keeps
/// its original two-second cadence unless a desktop switch explicitly asks for
/// an immediate refresh.
const THEME_POLL_INTERVAL: Duration = Duration::from_millis(2000);

const EVENT_SYSTEM_DESKTOPSWITCH: u32 = 0x0023;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;

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

type WinEventProc = unsafe extern "system" fn(
    hook: isize,
    event: u32,
    hwnd: isize,
    id_object: isize,
    id_child: isize,
    event_thread: u32,
    event_time: u32,
);

#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
    fn FindWindowExW(parent: isize, after: isize, class: *const u16, name: *const u16) -> isize;
    fn SetParent(hwnd: isize, new_parent: isize) -> isize;
    fn GetParent(hwnd: isize) -> isize;
    fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, process_id: *mut u32) -> u32;
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
    fn DrawTextW(hdc: isize, text: *const u16, count: i32, rect: *mut Rect, format: u32) -> i32;
    fn GetClientRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn SetWindowPos(hwnd: isize, after: isize, x: i32, y: i32, w: i32, h: i32, flags: u32) -> i32;
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn ScreenToClient(hwnd: isize, point: *mut Point) -> i32;
    fn DestroyWindow(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd_show: i32) -> i32;
    fn UpdateLayeredWindow(
        hwnd: isize,
        hdc_dst: isize,
        ppt_dst: *const Point,
        psize: *const Size,
        hdc_src: isize,
        ppt_src: *const Point,
        cr_key: u32,
        blend: *const BlendFunction,
        flags: u32,
    ) -> i32;
    fn SetWinEventHook(
        event_min: u32,
        event_max: u32,
        hmod_win_event_proc: isize,
        lpfn_win_event_proc: Option<WinEventProc>,
        id_process: u32,
        id_thread: u32,
        flags: u32,
    ) -> isize;
    fn UnhookWinEvent(hwin_event_hook: isize) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn DeleteDC(hdc: isize) -> i32;
    fn CreateDIBSection(
        hdc: isize,
        bitmap_info: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: isize,
        offset: u32,
    ) -> isize;
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
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
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

/// Returns the current Windows virtual-desktop identifier when Explorer has
/// published it. The value is a stable 16-byte GUID in the same per-user
/// registry location used by the shell. It is read only from the disposable
/// geometry worker as a fallback for systems that do not deliver
/// `EVENT_SYSTEM_DESKTOPSWITCH` to an out-of-context hook (or deliver it after
/// the first frame has already been composed).
fn current_virtual_desktop() -> Option<[u8; 16]> {
    let sub_key = wide(r"Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops");
    let value_name = wide("CurrentVirtualDesktop");
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
        return None;
    }
    let mut data = [0u8; 16];
    let mut size = data.len() as u32;
    let mut value_type: u32 = 0;
    let queried = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null(),
            &raw mut value_type,
            data.as_mut_ptr(),
            &raw mut size,
        )
    };
    unsafe { RegCloseKey(hkey) };
    if queried == 0 && size >= data.len() as u32 {
        Some(data)
    } else {
        None
    }
}

#[repr(C)]
#[derive(Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Size {
    cx: i32,
    cy: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct BlendFunction {
    blend_op: u8,
    blend_flags: u8,
    source_constant_alpha: u8,
    alpha_format: u8,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_pels_per_meter: i32,
    y_pels_per_meter: i32,
    clr_used: u32,
    clr_important: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 3],
}

/// A short-lived top-down 32-bit canvas used by the single taskbar window.
/// Keeping it on the owner thread makes the layered upload atomic with the
/// corresponding `SetWindowPos` and eliminates the old click proxy window.
struct SurfaceCanvas {
    hdc: isize,
    bitmap: isize,
    previous: isize,
    bits: usize,
    len: usize,
}

impl SurfaceCanvas {
    fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let info = BitmapInfo {
            header: BitmapInfoHeader {
                size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                width,
                // Negative height means top-down; row 0 is the first visible
                // taskbar row and no coordinate inversion is needed.
                height: -height,
                planes: 1,
                bit_count: 32,
                compression: BI_RGB,
                ..BitmapInfoHeader::default()
            },
            colors: [0; 3],
        };
        let hdc = unsafe { CreateCompatibleDC(0) };
        if hdc == 0 {
            return None;
        }
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(
                hdc,
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                0,
                0,
            )
        };
        if bitmap == 0 || bits.is_null() {
            unsafe { DeleteDC(hdc) };
            return None;
        }
        let previous = unsafe { SelectObject(hdc, bitmap) };
        Some(Self {
            hdc,
            bitmap,
            previous,
            bits: bits as usize,
            len: (width as usize).saturating_mul(height as usize).saturating_mul(4),
        })
    }

    fn pixels(&self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits as *mut u8, self.len) }
    }
}

impl Drop for SurfaceCanvas {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.hdc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.hdc);
        }
    }
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

/// Inspect one window from a foreign process and, when it is a compact
/// taskbar-aligned surface, fold its right edge into the left-side anchor.
///
/// TrafficMonitor's current Win11 implementation keeps a visible dialog below
/// a hidden owner window, so looking only at direct `Shell_TrayWnd` children is
/// not enough. This predicate deliberately rejects full-screen/normal app
/// windows: only surfaces no taller than twice the taskbar are eligible. It is
/// called by the disposable geometry worker, never by the owner thread.
fn consider_foreign_taskbar_window(
    hwnd: isize,
    parent: isize,
    parent_process: u32,
    own_widget: isize,
    screen_parent: &Rect,
    parent_rect: &Rect,
    taskbar_height: i32,
    minimum_height: i32,
    rightmost: &mut Option<i32>,
) {
    if hwnd == 0 || hwnd == own_widget || unsafe { IsWindowVisible(hwnd) } == 0 {
        return;
    }
    let mut child_process = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, &raw mut child_process) };
    if child_process == 0 || child_process == parent_process {
        return;
    }

    let mut rect = Rect::default();
    if unsafe { GetWindowRect(hwnd, &raw mut rect) } == 0 {
        return;
    }
    let candidate_height = rect.bottom - rect.top;
    if candidate_height < minimum_height || candidate_height > taskbar_height.saturating_mul(2) {
        return;
    }
    let vertical_overlap = (rect.bottom.min(screen_parent.bottom)
        - rect.top.max(screen_parent.top))
        .max(0);
    if vertical_overlap < minimum_height {
        return;
    }

    let mut left_point = Point {
        x: rect.left,
        y: rect.top,
    };
    let mut right_point = Point {
        x: rect.right,
        y: rect.top,
    };
    if unsafe { ScreenToClient(parent, &raw mut left_point) } == 0 {
        return;
    }
    if unsafe { ScreenToClient(parent, &raw mut right_point) } == 0 {
        return;
    }
    if left_point.x < parent_rect.left || right_point.x > parent_rect.right {
        return;
    }
    *rightmost = Some(rightmost.unwrap_or(right_point.x).max(right_point.x));
}

/// Walk a bounded depth of a window's descendants. A bounded walk is enough
/// for taskbar monitors (owner → dialog → optional child) and prevents an
/// unrelated application's deep control tree from becoming a heartbeat cost.
fn scan_foreign_taskbar_descendants(
    root: isize,
    parent: isize,
    parent_process: u32,
    own_widget: isize,
    screen_parent: &Rect,
    parent_rect: &Rect,
    taskbar_height: i32,
    minimum_height: i32,
    depth: u8,
    rightmost: &mut Option<i32>,
) {
    let mut after = 0;
    loop {
        let child = unsafe { FindWindowExW(root, after, std::ptr::null(), std::ptr::null()) };
        if child == 0 {
            break;
        }
        after = child;
        consider_foreign_taskbar_window(
            child,
            parent,
            parent_process,
            own_widget,
            screen_parent,
            parent_rect,
            taskbar_height,
            minimum_height,
            rightmost,
        );
        if depth > 0 {
            scan_foreign_taskbar_descendants(
                child,
                parent,
                parent_process,
                own_widget,
                screen_parent,
                parent_rect,
                taskbar_height,
                minimum_height,
                depth - 1,
                rightmost,
            );
        }
    }
}

/// Where the native taskbar child should sit, returned in screen coordinates
/// so the same geometry can be measured before and after `SetParent`.
///
/// Anchored immediately left of the system tray/clock (`TrayNotifyWnd`) —
/// i.e. flush against the actual notification area, not merely "somewhere
/// left of the task buttons".
///
/// `TrayNotifyWnd` is the stable Win11 anchor used by TrafficMonitor.  The
/// left-side mode additionally discovers visible *foreign-process* taskbar
/// children (for example TrafficMonitor) and starts after their actual bounds;
/// it does not depend on Explorer's private XAML class names.  Explorer's
/// composition surface is never treated as an authoritative pixel rectangle.
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

    // Third-party taskbar monitors (notably TrafficMonitor) can paint through
    // an owned dialog rather than a direct Shell_TrayWnd child. Walk a small
    // descendant depth below the taskbar and below top-level owners so the
    // anchor follows the actual occupied surface without hard-coded Explorer
    // class names. This runs only on the disposable geometry worker.
    let foreign_component_right = || -> Option<i32> {
        let mut parent_process = 0u32;
        unsafe { GetWindowThreadProcessId(parent, &raw mut parent_process) };
        let own_widget = WIDGET_HWND.lock().map(|guard| *guard).unwrap_or(0);
        let minimum_height = (height / 2).max(1);
        let mut rightmost = None;

        // First cover Shell_TrayWnd descendants. This also catches monitors
        // that Explorer exposes directly in its child/owner chain.
        scan_foreign_taskbar_descendants(
            parent,
            parent,
            parent_process,
            own_widget,
            &screen_parent,
            &parent_rect,
            taskbar_height,
            minimum_height,
            4,
            &mut rightmost,
        );

        // TrafficMonitor's Win11 dialog is owned by a hidden top-level window,
        // so it is not guaranteed to appear as a direct taskbar child. A depth
        // two scan of top-level owners finds that dialog while rejecting large
        // app windows through the size/vertical-overlap predicate above.
        let mut after = 0;
        loop {
            let top = unsafe { FindWindowExW(0, after, std::ptr::null(), std::ptr::null()) };
            if top == 0 {
                break;
            }
            after = top;
            if top == parent {
                continue;
            }
            scan_foreign_taskbar_descendants(
                top,
                parent,
                parent_process,
                own_widget,
                &screen_parent,
                &parent_rect,
                taskbar_height,
                minimum_height,
                2,
                &mut rightmost,
            );
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
        // Prefer the space immediately after an existing foreign status
        // component. Fall back to the taskbar's own left inset when there is
        // no such component.
        foreign_component_right()
            .map(|right| right + margin)
            .unwrap_or(margin)
    } else {
        (right_bound - desired_width).max(parent_rect.left + margin)
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

fn current_widget_theme() -> codexbar::settings::ThemePreference {
    WIDGET_THEME
        .lock()
        .map(|theme| *theme)
        .unwrap_or(codexbar::settings::ThemePreference::Auto)
}

fn resolve_widget_theme(theme: codexbar::settings::ThemePreference, system_is_light: bool) -> bool {
    match theme {
        codexbar::settings::ThemePreference::Light => true,
        codexbar::settings::ThemePreference::Dark => false,
        codexbar::settings::ThemePreference::Auto => system_is_light,
    }
}

/// Returns the effective text contrast without touching the registry. The
/// worker refreshes `TASKBAR_THEME_CACHE`; an explicit Appearance override is
/// therefore visible on the very next paint rather than after the next poll.
fn widget_is_light() -> bool {
    resolve_widget_theme(current_widget_theme(), cached_taskbar_is_light())
}

/// Applies the global Appearance → Theme preference to the native strip.
///
/// This is intentionally one shared setting rather than a second taskbar-only
/// theme switch. `Auto` follows the actual Windows taskbar; explicit values
/// choose the text contrast while the alpha surface keeps the strip visually
/// merged with Explorer's surface.
pub fn set_theme(theme: codexbar::settings::ThemePreference) {
    if let Ok(mut current) = WIDGET_THEME.lock() {
        *current = theme;
    }
    // Theme changes often arrive with no geometry notification. Ask the
    // disposable worker to refresh the system brightness cache, then repaint
    // the alpha surface on the owner thread.
    THEME_REFRESH_REQUESTED.store(true, Ordering::Release);
    repaint();
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
    if hwnd != 0 {
        request_reassert(hwnd);
        request_repaint(hwnd);
    }
}

pub fn set_font_weight(weight: u16) {
    let value = codexbar::settings::normalize_taskbar_widget_font_weight(weight) as i32;
    if let Ok(mut current) = WIDGET_FONT_WEIGHT.lock() {
        *current = value;
    }
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 {
        request_repaint(hwnd);
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
    if hwnd != 0 {
        request_reassert(hwnd);
        request_repaint(hwnd);
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

/// Official-icon slot size in logical pixels (clamped 10..=18).
pub fn set_icon_size(size: u8) {
    if let Ok(mut current) = WIDGET_ICON_SIZE.lock() {
        *current = i32::from(size.clamp(10, 18));
    }
    repaint();
}

/// Icon render style: `pure` | `badge` | `solid`.
/// Gap between taskbar icon and tag in logical pixels (0..=12).
pub fn set_icon_gap(px: u8) {
    if let Ok(mut current) = WIDGET_ICON_GAP.lock() {
        *current = i32::from(px.min(12));
    }
    repaint();
}

/// Gap between taskbar tag and value in logical pixels (0..=8).
pub fn set_value_gap(px: u8) {
    if let Ok(mut current) = WIDGET_VALUE_GAP.lock() {
        *current = i32::from(px.min(8));
    }
    repaint();
}

pub fn set_icon_style(style: &str) {
    let value = match style {
        "badge" => "badge",
        "solid" => "solid",
        _ => "pure",
    };
    if let Ok(mut current) = WIDGET_ICON_STYLE.lock() {
        *current = value.to_string();
    }
    repaint();
}

fn repaint() {
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 {
        request_repaint(hwnd);
    }
}

/// Post a coalesced repaint to the owner thread. Keeping the window operation
/// on that thread avoids mixing settings/refresh workers with a cross-thread
/// native child.
fn request_repaint(hwnd: isize) {
    if hwnd == 0 || REPAINT_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    if unsafe { PostMessageW(hwnd, WM_APP_REPAINT, 0, 0) } == 0 {
        REPAINT_PENDING.store(false, Ordering::Release);
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
    if hwnd != 0 {
        request_repaint(hwnd);
    }
}

/// Ask the widget's owning thread to apply the most recently sampled geometry.
/// `SetWindowPos` is intentionally never called directly by settings/refresh
/// worker threads: Windows sends window-position messages to the owner thread
/// synchronously, which can deadlock when that thread is painting.
fn request_reassert(hwnd: isize) {
    if hwnd == 0 || REASSERT_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    if unsafe { PostMessageW(hwnd, WM_APP_REASSERT, 0, 0) } == 0 {
        REASSERT_PENDING.store(false, Ordering::Release);
    }
}

fn take_pending_parent() -> Option<isize> {
    PENDING_PARENT_HWND
        .lock()
        .ok()
        .and_then(|mut pending| pending.take())
}

/// Queues a parent-window change for the widget's owner thread. `SetParent`
/// and the accompanying style/Z-order changes are deliberately not executed
/// from the geometry worker: doing so would make the child receive synchronous
/// window messages on a thread that does not own its message pump.
fn request_reparent(hwnd: isize, parent: isize) {
    if hwnd == 0 {
        return;
    }
    if let Ok(mut pending) = PENDING_PARENT_HWND.lock() {
        *pending = Some(parent);
    }
    if REPARENT_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }
    if unsafe { PostMessageW(hwnd, WM_APP_REPARENT, 0, 0) } == 0 {
        REPARENT_PENDING.store(false, Ordering::Release);
    }
}

/// The callback is intentionally tiny. WinEvent callbacks may run while the
/// shell is in the middle of switching desktops; querying its window tree or
/// moving a cross-process child here would reintroduce the freeze this module
/// is designed to avoid. The owner thread consumes the posted messages,
/// while the worker refreshes the system theme cache on its next pass.
unsafe extern "system" fn desktop_switch_proc(
    _hook: isize,
    event: u32,
    _hwnd: isize,
    _id_object: isize,
    _id_child: isize,
    _event_thread: u32,
    _event_time: u32,
) {
    if event != EVENT_SYSTEM_DESKTOPSWITCH {
        return;
    }
    DESKTOP_SWITCH_PENDING.store(true, Ordering::Release);
    THEME_REFRESH_REQUESTED.store(true, Ordering::Release);
    let hwnd = WIDGET_HWND.lock().map(|guard| *guard).unwrap_or(0);
    if hwnd != 0 {
        request_reassert(hwnd);
        request_repaint(hwnd);
    }
}

fn install_desktop_switch_hook() {
    if DESKTOP_SWITCH_HOOK.lock().map(|hook| *hook).unwrap_or(0) != 0 {
        return;
    }
    let hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_DESKTOPSWITCH,
            EVENT_SYSTEM_DESKTOPSWITCH,
            0,
            Some(desktop_switch_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    if hook == 0 {
        tracing::warn!("taskbar widget: could not install virtual-desktop switch hook");
    } else if let Ok(mut guard) = DESKTOP_SWITCH_HOOK.lock() {
        *guard = hook;
    }
}

fn uninstall_desktop_switch_hook() {
    let hook = DESKTOP_SWITCH_HOOK
        .lock()
        .map(|mut guard| std::mem::take(&mut *guard))
        .unwrap_or(0);
    if hook != 0 {
        unsafe { UnhookWinEvent(hook) };
    }
}

/// Samples Explorer's child tree away from the widget window procedure. If a
/// shell/XAML call stalls, only this disposable worker is affected; the Tauri
/// and taskbar message loops keep running. A changed result is posted back to
/// the widget thread, which performs only the short cached `SetWindowPos`.
fn start_geometry_worker(hwnd: isize, parent: isize) {
    let generation = GEOMETRY_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    thread::spawn(move || {
        let mut last_theme_sample: Option<Instant> = None;
        let mut observed_parent = parent;
        let mut last_virtual_desktop = current_virtual_desktop();
        loop {
            if GEOMETRY_GENERATION.load(Ordering::SeqCst) != generation
                || unsafe { IsWindow(hwnd) } == 0
            {
                break;
            }

            // Some Windows builds do not deliver EVENT_SYSTEM_DESKTOPSWITCH
            // reliably to an out-of-context hook. Comparing Explorer's
            // published virtual-desktop GUID keeps the detection on the same
            // disposable worker and still invalidates the old composed frame
            // before the next visible paint.
            let virtual_desktop = current_virtual_desktop();
            let desktop_changed = match (last_virtual_desktop, virtual_desktop) {
                (Some(previous), Some(current)) => previous != current,
                _ => false,
            };
            if virtual_desktop.is_some() {
                last_virtual_desktop = virtual_desktop;
            }
            if desktop_changed {
                DESKTOP_SWITCH_PENDING.store(true, Ordering::Release);
                THEME_REFRESH_REQUESTED.store(true, Ordering::Release);
                request_reassert(hwnd);
                request_repaint(hwnd);
            }

            let current_parent = taskbar();
            let attached_parent = CURRENT_PARENT_HWND.lock().map(|guard| *guard).unwrap_or(0);
            let actual_parent = unsafe { GetParent(hwnd) };
            if current_parent == 0 {
                // Keep the hidden child in its last valid parent while Explorer
                // is between taskbar hosts. Comparing `actual_parent` here
                // would enqueue the same hide message every 250ms because the
                // zero-parent transition intentionally keeps the child owned.
                if attached_parent != 0 {
                    request_reparent(hwnd, 0);
                }
                last_theme_sample = None;
                thread::sleep(GEOMETRY_POLL_INTERVAL);
                continue;
            }
            if current_parent != observed_parent
                || current_parent != attached_parent
                || actual_parent != current_parent
            {
                observed_parent = current_parent;
                last_theme_sample = None;
                request_reparent(hwnd, current_parent);
                thread::sleep(GEOMETRY_POLL_INTERVAL);
                continue;
            }

            if let Some(screen) = target_rect(current_parent, current_position()) {
                let theme_due = THEME_REFRESH_REQUESTED.swap(false, Ordering::AcqRel)
                    || last_theme_sample
                        .map(|sampled| sampled.elapsed() >= THEME_POLL_INTERVAL)
                        .unwrap_or(true);
                if theme_due {
                    let light = taskbar_is_light();
                    if let Ok(mut cache) = TASKBAR_THEME_CACHE.lock() {
                        *cache = Some(light);
                    }
                    last_theme_sample = Some(Instant::now());
                }
                if let Some(child) = child_rect(current_parent, screen) {
                    let dpi = unsafe { GetDpiForWindow(current_parent) }.max(96);
                    let next = Geometry {
                        screen,
                        child,
                        parent: current_parent,
                        dpi,
                        desktop: virtual_desktop,
                    };
                    let changed = GEOMETRY_CACHE
                        .lock()
                        .map(|mut cache| {
                            let changed = cache.as_ref() != Some(&next);
                            *cache = Some(next);
                            changed
                        })
                        .unwrap_or(false);
                    // A desktop switch can preserve both the rectangle and
                    // the visible bit while discarding the composed child
                    // surface. The WinEvent callback posts an immediate
                    // reassert; this visibility check is the fallback for
                    // shells that do not deliver that event to our process.
                    let hidden = unsafe { IsWindowVisible(hwnd) } == 0;
                    if changed || hidden {
                        request_reassert(hwnd);
                    }
                }
            }

            thread::sleep(GEOMETRY_POLL_INTERVAL);
        }
    });
}

/// Applies a newly discovered Shell_TrayWnd parent on the widget's owner
/// thread. Keeping the single surface hidden until the worker measures a rectangle is
/// important: a reparented child otherwise keeps its old taskbar-relative
/// coordinates for one paint and can briefly cover a different monitor.
fn apply_parent(hwnd: isize, parent: isize) {
    if parent == 0 {
        unsafe {
            if IsWindow(hwnd) != 0 {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
        if let Ok(mut cache) = GEOMETRY_CACHE.lock() {
            *cache = None;
        }
        if let Ok(mut cache) = TASKBAR_THEME_CACHE.lock() {
            *cache = None;
        }
        if let Ok(mut current) = CURRENT_PARENT_HWND.lock() {
            *current = 0;
        }
        return;
    }
    if unsafe { IsWindow(parent) } == 0 || unsafe { IsWindow(hwnd) } == 0 {
        return;
    }

    let current = unsafe { GetParent(hwnd) };
    unsafe {
        // Hide before SetParent so the old screen rectangle cannot be
        // composited while Explorer is changing its child tree.
        ShowWindow(hwnd, SW_HIDE);
        if current != parent {
            SetParent(hwnd, parent);
            SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_CHILD | WS_SYSMENU) as isize);
        }
    }

    if unsafe { GetParent(hwnd) } != parent {
        tracing::warn!(
            "taskbar widget: reparent failed widget={hwnd:#x} expected_host={parent:#x}"
        );
        return;
    }

    // The old child coordinates belong to the old parent. A hidden 1×1
    // placement makes the intermediate state deterministic; the worker will
    // publish the real screen/client rectangle on its next pass.
    unsafe {
        SetWindowPos(hwnd, HWND_TOP, 0, 0, 1, 1, SWP_NOACTIVATE | SWP_NOCOPYBITS);
    }
    if let Ok(mut current_parent) = CURRENT_PARENT_HWND.lock() {
        *current_parent = parent;
    }
    if let Ok(mut cache) = GEOMETRY_CACHE.lock() {
        *cache = None;
    }
    if let Ok(mut cache) = TASKBAR_THEME_CACHE.lock() {
        *cache = None;
    }
    THEME_REFRESH_REQUESTED.store(true, Ordering::Release);
    tracing::info!("taskbar widget: reattached to Shell_TrayWnd host={parent:#x}");
}

fn reassert(hwnd: isize) {
    if DESKTOP_SWITCH_PENDING.swap(false, Ordering::AcqRel) {
        unsafe {
            // Do not publish the old taskbar-relative frame while
            // DWM/Explorer is switching desktop surfaces. The geometry worker
            // will repopulate the versioned cache, then call reassert again.
            if hwnd != 0 && IsWindow(hwnd) != 0 {
                ShowWindow(hwnd, SW_HIDE);
            }
        }
        if let Ok(mut cache) = GEOMETRY_CACHE.lock() {
            *cache = None;
        }
        if let Ok(mut cache) = TASKBAR_THEME_CACHE.lock() {
            *cache = None;
        }
        return;
    }
    let Some(geometry) = GEOMETRY_CACHE.lock().ok().and_then(|cache| *cache) else {
        return;
    };
    if hwnd == 0 || unsafe { IsWindow(hwnd) } == 0 {
        return;
    }
    if unsafe { GetParent(hwnd) } != geometry.parent {
        // The worker has observed a different Explorer host.  Do not apply a
        // rectangle belonging to the previous host; the next pass will queue
        // the reparent operation and publish a new generation.
        return;
    }
    let (x, y, w, h) = geometry.child;
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOP,
            x,
            y,
            w,
            h,
            // This handler runs on the widget's owning thread. A synchronous
            // move keeps the widget and the Explorer taskbar in one committed
            // layout state instead of leaving an asynchronous old rectangle
            // visible for one or more paints. Do not suppress Z-order here:
            // Explorer may put its composition surface above a child while a
            // virtual desktop is changing, which hides or partially replaces
            // the strip until the next taskbar redraw.
            SWP_NOACTIVATE | SWP_NOCOPYBITS,
        );
        // Visibility is published only after one measured anchor. There is no
        // second proxy whose stale rectangle could cover the newly rendered
        // surface.
        ShowWindow(hwnd, SW_SHOW);
    }
    // Render and upload after the position is committed. The upload is
    // premultiplied-alpha and therefore cannot retain a previous rectangle or
    // a guessed background colour.
    render_surface(hwnd);
}

fn cached_taskbar_is_light() -> bool {
    TASKBAR_THEME_CACHE
        .lock()
        .ok()
        .and_then(|cache| *cache)
        .unwrap_or(false)
}

unsafe extern "system" fn wnd_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        // Painting is fully covered by WM_PAINT; letting the default proc
        // erase first only produces a flash of the class brush.
        WM_ERASEBKGND => 1,
        // Layered windows are presented with `UpdateLayeredWindow`; WM_PAINT
        // is only validated so an Explorer invalidate cannot trigger a second
        // renderer with a different theme/geometry snapshot.
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
        WM_APP_REPAINT => {
            REPAINT_PENDING.store(false, Ordering::Release);
            render_surface(hwnd);
            0
        }
        WM_APP_REPARENT => {
            if let Some(parent) = take_pending_parent() {
                apply_parent(hwnd, parent);
            }
            // A worker update can race the owner-thread handler. Clear the
            // coalescing bit only after consuming the pending value, then
            // immediately schedule the newest value if one arrived meanwhile.
            REPARENT_PENDING.store(false, Ordering::Release);
            if let Some(parent) = PENDING_PARENT_HWND.lock().ok().and_then(|pending| *pending) {
                request_reparent(hwnd, parent);
            }
            0
        }
        WM_SETTINGCHANGE | WM_DISPLAYCHANGE | WM_DPICHANGED => {
            // Theme/DPI/display changes invalidate the cached system brightness
            // and the layered bitmap. No screen pixels are sampled here.
            THEME_REFRESH_REQUESTED.store(true, Ordering::Release);
            request_reassert(hwnd);
            request_repaint(hwnd);
            0
        }
        WM_SIZE => {
            request_reassert(hwnd);
            request_repaint(hwnd);
            0
        }
        // Geometry changes are posted by the background worker. Keeping this
        // work out of a heartbeat timer prevents Explorer layout probes and
        // full repaints from running once a second on the window thread.
        WM_APP_REASSERT => {
            REASSERT_PENDING.store(false, Ordering::Release);
            reassert(hwnd);
            0
        }
        WM_NCHITTEST => HTCLIENT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE,
        WM_RBUTTONUP => {
            crate::taskbar_context_menu::show(
                hwnd,
                crate::taskbar_context_menu::MenuSurface::Strip,
            );
            0
        }
        WM_COMMAND => {
            crate::taskbar_context_menu::handle_command(wparam & 0xffff);
            0
        }
        WM_DESTROY => {
            GEOMETRY_GENERATION.fetch_add(1, Ordering::SeqCst);
            REPAINT_PENDING.store(false, Ordering::Release);
            REASSERT_PENDING.store(false, Ordering::Release);
            REPARENT_PENDING.store(false, Ordering::Release);
            THEME_REFRESH_REQUESTED.store(false, Ordering::Release);
            DESKTOP_SWITCH_PENDING.store(false, Ordering::Release);
            if let Ok(mut pending) = PENDING_PARENT_HWND.lock() {
                *pending = None;
            }
            if let Ok(mut parent) = CURRENT_PARENT_HWND.lock() {
                *parent = 0;
            }
            if let Ok(mut guard) = WIDGET_HWND.lock() {
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

fn render_surface(hwnd: isize) {
    let mut client = Rect::default();
    if unsafe { GetClientRect(hwnd, &raw mut client) } == 0 {
        return;
    }
    let width = (client.right - client.left).max(0);
    let height = (client.bottom - client.top).max(0);
    let Some(canvas) = SurfaceCanvas::new(width, height) else {
        tracing::warn!("taskbar widget: could not allocate {width}x{height} alpha surface");
        return;
    };
    let hdc = canvas.hdc;
    tracing::debug!("taskbar widget: render surface hdc={hdc:#x} size={width}x{height}");


    let light = widget_is_light();
    let text = if light {
        0x003A_3A3Au32
    } else {
        0x00FF_FFFFu32
    };
    // This nominal colour is used only for muted-tag contrast.  The canvas
    // itself remains transparent; DWM supplies the real taskbar background.
    let background = if cached_taskbar_is_light() {
        0x00E9_EFEE
    } else {
        0x0020_2020
    };
    // Clear before either renderer so a Direct2D failure cannot expose bytes
    // from a previous provider frame through the layered surface.
    canvas.pixels().fill(0);

    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let font_size = WIDGET_FONT_SIZE.lock().map(|size| *size).unwrap_or(12);
    let size_px = ((font_size * dpi as i32) as f32) / 96.0;
    let font_weight = WIDGET_FONT_WEIGHT
        .lock()
        .map(|weight| *weight)
        .unwrap_or(400) as f32;
    let family = widget_font_family();
    let icon_size_px =
        ((WIDGET_ICON_SIZE.lock().map(|s| *s).unwrap_or(14) * dpi as i32) as f32) / 96.0;
    let icon_gap_px =
        ((WIDGET_ICON_GAP.lock().map(|s| *s).unwrap_or(5) * dpi as i32) as f32) / 96.0;
    let value_gap_px =
        ((WIDGET_VALUE_GAP.lock().map(|s| *s).unwrap_or(2) * dpi as i32) as f32) / 96.0;
    let icon_style = crate::taskbar_icons::IconStyle::parse(
        &WIDGET_ICON_STYLE
            .lock()
            .map(|s| s.clone())
            .unwrap_or_else(|_| "pure".to_string()),
    );

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
    let text_align = match align {
        DT_CENTER => crate::taskbar_text::TextAlign::Center,
        DT_RIGHT => crate::taskbar_text::TextAlign::Right,
        _ => crate::taskbar_text::TextAlign::Left,
    };

    // DirectWrite, not GDI. `CreateFontW` cannot reach a font's OpenType
    // variation axes, so it collapsed every requested weight to Regular or
    // Bold; `taskbar_text` drives the `wght` axis directly.
    let to_win_rect = |r: &Rect| crate::taskbar_text::WinRect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    };
    let cells: Vec<crate::taskbar_text::StripCell<'_>> = visible
        .iter()
        .zip(line_rects.iter())
        .map(|(line, rect)| crate::taskbar_text::StripCell {
            rect: to_win_rect(rect),
            tag: line.tag.as_str(),
            value: line.value.as_str(),
            // Official brand SVG in the icon slot. The earlier 0xc000041d in
            // WM_PAINT was the by-value Vector2 ABI mismatch in the geometry
            // sink (fixed by windows-numerics); draw_strip_cells_with now
            // exercises the same path in an offscreen regression test.
            icon_provider: line.icon_provider_id.as_deref(),
            glyph: line.mark.map(|mark| crate::taskbar_text::LineMark {
                glyph: mark.glyph,
                color_rgb: mark.color_rgb,
            }),
        })
        .collect();
    let drawn = crate::taskbar_text::draw_strip_cells_premultiplied(
        hdc,
        to_win_rect(&client),
        &cells,
        &crate::taskbar_text::TextStyle {
            family: &family,
            weight: font_weight,
            size_px,
            align: text_align,
            color_rgb: text,
        },
        background,
        icon_size_px,
        icon_gap_px,
        value_gap_px,
        icon_style,
    );

    // GDI fallback. If DirectWrite is unavailable the strip must still show
    // something readable rather than an empty rectangle over the taskbar.
    if !drawn {
        let font_height = -((font_size * dpi as i32) / 96);
        let font = unsafe {
            CreateFontW(
                font_height,
                0,
                0,
                0,
                font_weight as i32,
                0,
                0,
                0,
                1,
                0,
                0,
                4,
                0,
                wide(&family).as_ptr(),
            )
        };
        let previous = unsafe { SelectObject(hdc, font) };
        unsafe {
            SetBkMode(hdc, TRANSPARENT_BK);
            SetTextColor(hdc, text);
        }
        // DrawTextW clips to the supplied cell rectangle by default. Do not
        // use DT_END_ELLIPSIS here: DirectWrite can fail transiently during a
        // taskbar/DPI transition, and the fallback must obey the same hard
        // boundary instead of reintroducing the intermittent `...` that the
        // native renderer intentionally removed.
        let format = align | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
        for (line, rect) in visible.iter().zip(line_rects.iter()) {
            let mut r = *rect;
            // GDI has one text colour per DC, so the fallback prints the mark,
            // tag and value inline.  Including the mark even when an official
            // SVG exists keeps the degraded frame identifiable instead of
            // showing a misleading text-only cell.
            let body = match line.mark {
                Some(mark) => format!("{} {} {}", mark.glyph, line.tag, line.value),
                None => format!("{} {}", line.tag, line.value).trim().to_string(),
            };
            let wide_text = wide(&body);
            unsafe { DrawTextW(hdc, wide_text.as_ptr(), -1, &raw mut r, format) };
        }
        unsafe {
            SelectObject(hdc, previous);
            DeleteObject(font);
        }
        // GDI does not preserve alpha in a DIB section.  Promote only pixels
        // it actually touched; untouched pixels stay alpha zero and continue
        // to reveal Explorer's taskbar surface.
        for pixel in canvas.pixels().chunks_exact_mut(4) {
            if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                pixel[3] = 0xFF;
            }
        }
    }

    let size = Size { cx: width, cy: height };
    let source = Point::default();
    let blend = BlendFunction {
        blend_op: AC_SRC_OVER,
        blend_flags: 0,
        source_constant_alpha: 0xFF,
        alpha_format: AC_SRC_ALPHA,
    };
    let uploaded = unsafe {
        UpdateLayeredWindow(
            hwnd,
            0,
            std::ptr::null(),
            &raw const size,
            canvas.hdc,
            &raw const source,
            0,
            &raw const blend,
            ULW_ALPHA,
        )
    };
    if uploaded == 0 {
        tracing::warn!("taskbar widget: UpdateLayeredWindow failed; keeping last frame");
    }
}

/// Reads the persisted setting at startup and creates the strip if the user
/// previously enabled it. Failure is logged, never fatal: the taskbar child is
/// a convenience surface and must not take the app down with it.
pub fn install() {
    let settings = codexbar::settings::Settings::load();
    set_theme(settings.theme);
    set_position(&settings.taskbar_widget_position);
    set_font_weight(settings.taskbar_widget_font_weight);
    set_content(&settings.taskbar_widget_content);
    set_font_family(&settings.taskbar_widget_font_family);
    set_font_size(settings.taskbar_widget_font_size);
    set_width(settings.taskbar_widget_width);
    set_text_align(&settings.taskbar_widget_text_align);
    set_icon_size(settings.taskbar_widget_icon_size);
    set_icon_style(&settings.taskbar_widget_icon_style);
    set_icon_gap(settings.taskbar_widget_icon_gap_px);
    set_value_gap(settings.taskbar_widget_value_gap_px);
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
pub(crate) fn set_enabled(enabled: bool) {
    let Some(app) = APP_HANDLE.get() else {
        // Before `set_app_handle` — only reachable from `install()`'s own
        // thread, which is already the right one.
        apply_enabled(enabled);
        return;
    };
    let app = app.clone();
    if app
        .run_on_main_thread(move || apply_enabled(enabled))
        .is_err()
    {
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
    GEOMETRY_GENERATION.fetch_add(1, Ordering::SeqCst);
    REPAINT_PENDING.store(false, Ordering::Release);
    REASSERT_PENDING.store(false, Ordering::Release);
    REPARENT_PENDING.store(false, Ordering::Release);
    THEME_REFRESH_REQUESTED.store(false, Ordering::Release);
    DESKTOP_SWITCH_PENDING.store(false, Ordering::Release);
    uninstall_desktop_switch_hook();
    if let Ok(mut pending) = PENDING_PARENT_HWND.lock() {
        *pending = None;
    }
    if let Ok(mut parent) = CURRENT_PARENT_HWND.lock() {
        *parent = 0;
    }
    if let Ok(mut cache) = TASKBAR_THEME_CACHE.lock() {
        *cache = None;
    }
    if let Ok(mut cache) = GEOMETRY_CACHE.lock() {
        *cache = None;
    }
    let hwnd = WIDGET_HWND.lock().map(|g| *g).unwrap_or(0);
    if hwnd != 0 && unsafe { IsWindow(hwnd) } != 0 {
        unsafe { DestroyWindow(hwnd) };
    }
    if let Ok(mut guard) = WIDGET_HWND.lock() {
        *guard = 0;
    }
}

/// Computes a safe first rectangle without walking Explorer's child tree. The
/// detailed anchor is filled in by the background geometry worker after the
/// window has been created.
fn fallback_screen_rect(parent: isize, position: WidgetPosition) -> Option<ScreenRect> {
    let mut screen_parent = Rect::default();
    let mut client = Rect::default();
    if unsafe { GetWindowRect(parent, &raw mut screen_parent) } == 0
        || unsafe { GetClientRect(parent, &raw mut client) } == 0
    {
        return None;
    }
    let taskbar_height = client.bottom - client.top;
    if taskbar_height <= 0 {
        return None;
    }
    let dpi = unsafe { GetDpiForWindow(parent) };
    let dpi = if dpi == 0 { 96 } else { dpi };
    let scale = |dip: i32| (dip * dpi as i32) / 96;
    let desired_width = scale(WIDGET_WIDTH.lock().map(|width| *width).unwrap_or(136));
    let margin = scale(WIDGET_MARGIN_DIP);
    let height = scale(WIDGET_HEIGHT_DIP).min(taskbar_height);
    let right_bound = client.right - margin;
    let x = match position {
        WidgetPosition::Left => margin,
        WidgetPosition::Notification => (right_bound - desired_width).max(margin),
    };
    let width = (right_bound - x).min(desired_width);
    if width <= 0 {
        return None;
    }
    let y = screen_parent.top + (taskbar_height - height) / 2;
    Some((screen_parent.left + x, y, width, height))
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
    let screen_rect =
        fallback_screen_rect(parent, current_position()).ok_or("taskbar geometry unavailable")?;
    let (x, y, w, h) = child_rect(parent, screen_rect).unwrap_or(screen_rect);

    // Match TrafficMonitor: create a native popup first, then attach it to
    // Explorer's taskbar and convert it to a child window. Creating first is
    // important for the cross-process SetParent path. The first placement is
    // deliberately a cheap rectangle; the detailed Explorer anchor is filled
    // in by the background worker after the window exists.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
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
            // Keep the fallback rectangle invisible. It is only a creation
            // coordinate; showing it before Explorer's first measured anchor
            // briefly places the strip at the taskbar's physical left edge,
            // where it can cover TrafficMonitor or leave a stale frame behind.
            (WS_CHILD | WS_SYSMENU) as isize,
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
    }

    if let Ok(mut guard) = WIDGET_HWND.lock() {
        *guard = hwnd;
    }
    if let Ok(mut current_parent) = CURRENT_PARENT_HWND.lock() {
        *current_parent = parent;
    }
    if let Ok(mut pending) = PENDING_PARENT_HWND.lock() {
        *pending = None;
    }
    DESKTOP_SWITCH_PENDING.store(false, Ordering::Release);
    // The creation rectangle above is intentionally not a valid cache entry:
    // the worker must publish one Explorer-measured geometry before the surface
    // becomes visible. This makes the first frame obey the same anchor rule as
    // every later frame, including when TrafficMonitor is already present.
    if let Ok(mut cache) = GEOMETRY_CACHE.lock() {
        *cache = None;
    }
    install_desktop_switch_hook();
    start_geometry_worker(hwnd, parent);

    tracing::info!(
        "taskbar widget: hwnd={hwnd:#x} host={parent:#x} rect=({x},{y},{w},{h}) mode=single-layered-child"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 200 wide, 40 tall, 6px padding — roughly the real strip at 96 DPI.
    fn client() -> Rect {
        Rect {
            left: 0,
            top: 0,
            right: 200,
            bottom: 40,
        }
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

    #[test]
    fn widget_theme_auto_uses_system_brightness_but_overrides_are_explicit() {
        use codexbar::settings::ThemePreference;

        assert!(resolve_widget_theme(ThemePreference::Auto, true));
        assert!(!resolve_widget_theme(ThemePreference::Auto, false));
        assert!(resolve_widget_theme(ThemePreference::Light, false));
        assert!(!resolve_widget_theme(ThemePreference::Dark, true));
    }

    /// A strip narrower than its own padding must not produce inverted rects,
    /// which DirectWrite would reject and leave the strip blank.
    #[test]
    fn degenerate_widths_stay_ordered() {
        let narrow = Rect {
            left: 0,
            top: 0,
            right: 8,
            bottom: 40,
        };
        for r in cell_rects(&narrow, 4, 6) {
            assert!(r.right >= r.left, "inverted rect: {r:?}");
            assert!(r.bottom >= r.top, "inverted rect: {r:?}");
        }
    }
}
