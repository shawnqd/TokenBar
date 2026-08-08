//! A fully self-drawn popup menu, replacing `TrackPopupMenu`.
//!
//! # Why not a native menu
//!
//! The previous implementation was a real `CreatePopupMenu` menu with
//! `MF_OWNERDRAW` rows. Owner-draw only hands us the *inside* of each row —
//! the window itself still belongs to the system menu class (`#32768`), which
//! means four things the reference design (Telegram's taskbar menu) needs are
//! simply not reachable:
//!
//! * the 1px system frame is painted by the class and cannot be removed
//!   (`DWMWA_BORDER_COLOR` was tried and does not apply to `#32768`);
//! * the drop shadow is the class's own `CS_DROPSHADOW` — fixed offset, fixed
//!   opacity, visibly heavy toward the bottom-right;
//! * the corner radius is whatever DWM's `DWMWCP_ROUND` gives, not a value we
//!   choose;
//! * the open animation is governed by `SPI_SETMENUANIMATION`/`SPI_SETMENUFADE`
//!   system-wide, so a directional expand is impossible.
//!
//! So the menu is its own window here: `WS_POPUP` + `WS_EX_LAYERED`, painted
//! per-pixel through `UpdateLayeredWindow`. That buys full control of frame,
//! shadow, radius and animation, at the cost of re-implementing the hover,
//! click, dismissal and keyboard handling `TrackPopupMenu` gave away for free —
//! which is what the bulk of this file is.
//!
//! # Pixel format
//!
//! `UpdateLayeredWindow` with `ULW_ALPHA` consumes **premultiplied** BGRA. The
//! card body and its shadow are composed arithmetically into the DIB's bits
//! (see [`compose_base`]), then labels and check marks are drawn on top with
//! ordinary GDI. GDI is not alpha-aware and zeroes the alpha byte of every pixel it
//! touches, so [`restore_alpha`] writes the computed alpha channel back
//! afterwards. That is safe precisely because text never reaches the
//! antialiased card edge, where alpha is fractional.

#![cfg(windows)]

use std::ffi::c_void;
use std::sync::Mutex;
use std::time::Instant;

use crate::taskbar_widget::taskbar_is_light;

// ── Public shape ────────────────────────────────────────────────────────────

/// The check mark, drawn in the left column of a checked row.
///
/// Segoe MDL2 Assets is the icon font present on both Windows 10 and 11. This
/// is the only glyph the menu draws — per-row icons were removed at the user's
/// request, and the check now occupies the column they used to.
const CHECK_GLYPH: char = '\u{E73E}';

/// One entry supplied by the caller. `id` is echoed back to the owner window
/// as the `WM_COMMAND` wparam, matching what `TPM_RETURNCMD` used to deliver,
/// so existing command handlers need no change.
#[derive(Clone, Debug)]
pub struct MenuItem {
    pub id: usize,
    pub label: String,
    pub checked: bool,
    pub separator: bool,
    pub disabled: bool,
}

impl MenuItem {
    pub fn action(id: usize, label: String) -> Self {
        Self {
            id,
            label,
            checked: false,
            separator: false,
            disabled: false,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub fn separator() -> Self {
        Self {
            id: 0,
            label: String::new(),
            checked: false,
            separator: true,
            disabled: false,
        }
    }
}

// ── Design constants (DIPs, scaled by the anchor window's DPI) ──────────────

/// Room reserved around the card for the shadow. Must exceed
/// `SHADOW_BLUR_DIP + SHADOW_OFFSET_Y_DIP` or the blur gets clipped.
const SHADOW_MARGIN_DIP: i32 = 22;
/// Blur radius. Tuned by eye over three passes: 11 DIP spread the falloff over
/// ~40 px and peaked at 13 % darkening, which read as no shadow at all; 7 DIP
/// at 0.30 strength overshot the other way. 3 DIP keeps the shadow as a tight
/// contact edge rather than a halo.
const SHADOW_BLUR_DIP: i32 = 3;
/// A small downward offset only — the old shadow read as bottom-*right*
/// because the system draws it diagonally. Horizontally this one is symmetric.
const SHADOW_OFFSET_Y_DIP: i32 = 3;
const CORNER_RADIUS_DIP: i32 = 8;
const ROW_HEIGHT_DIP: i32 = 30;
const SEPARATOR_BLOCK_DIP: i32 = 7;
/// Vertical padding inside the card, above the first row and below the last.
const CARD_PAD_V_DIP: i32 = 4;
/// The hover highlight is inset from the card edge so its own rounded corners
/// stay visible inside the card's, the way the tray panel's rows do.
const HOVER_INSET_DIP: i32 = 4;
const HOVER_RADIUS_DIP: i32 = 5;
/// The left column, reserved for the check mark on checked rows and left empty
/// on the rest. Its width is what stops labels from hugging the card's left
/// edge; together with `TEXT_PAD_RIGHT_DIP` it brackets the text roughly
/// evenly, which is the balance the reference menu has.
const CHECK_COLUMN_DIP: i32 = 26;
const CHECK_GLYPH_DIP: i32 = 13;
const TEXT_GAP_DIP: i32 = 2;
const TEXT_PAD_RIGHT_DIP: i32 = 26;
/// Fallback font family, used only when the persisted family is empty.
///
/// The family, weight and size are all user settings now, read at open time —
/// the same trio the taskbar strip exposes, driven by the same DirectWrite
/// renderer. An earlier version hardcoded "Microsoft YaHei UI Light" and
/// mapped weights onto GDI's three faces; that whole dance existed because GDI
/// cannot synthesise lighter than the named face, and it is unnecessary now
/// that the labels go through the `wght` axis.
const FALLBACK_FONT_FAMILY: &str = "Microsoft YaHei UI";
/// Fallback em size in DIPs, used only when the persisted size is out of range.
const FALLBACK_FONT_SIZE_DIP: i32 = 12;

/// Deliberately modest: a wide floor leaves short labels stranded against the
/// left column and makes the whole card read as left-heavy.
const MIN_CARD_WIDTH_DIP: i32 = 148;
/// Gap between the card's bottom edge and the top of the taskbar.
const TASKBAR_GAP_DIP: i32 = 6;

/// Open animation: the card slides up into place while fading in, matching the
/// Windows flyouts.
///
/// Two earlier attempts are worth not repeating. Scaling the bitmap up from
/// 0.86 resampled every glyph each frame and visibly warped the text. A
/// circular reveal from the bottom-left corner kept the glyphs crisp but was
/// the wrong gesture — Windows' own flyouts do not uncover, they arrive.
///
/// This version transforms nothing: the card is composed once at full size and
/// the *window* is moved, via `UpdateLayeredWindow`'s destination point, from
/// `ANIM_TRAVEL_DIP` below its resting place up to it. Fading is the layered
/// window's constant alpha. Both are free — no per-frame pixel work at all.
const ANIM_DURATION_MS: u128 = 180;
const ANIM_TICK_MS: u32 = 10;
/// How far below its resting place the card starts. The window overlaps the
/// taskbar for the first frames, which is what the reference does too.
const ANIM_TRAVEL_DIP: i32 = 22;
/// Deactivation arriving within this window of the menu appearing is ignored —
/// the activation change that *shows* the menu can itself produce one.
const ACTIVATE_GRACE_MS: u128 = 150;

// ── Win32 ───────────────────────────────────────────────────────────────────

const CLASS_NAME: &str = "CodexBarPopupMenu";

const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_TOPMOST: u32 = 0x0000_0008;

const SW_SHOW: i32 = 5;

const WM_DESTROY: u32 = 0x0002;
const WM_ACTIVATE: u32 = 0x0006;
const WM_KILLFOCUS: u32 = 0x0008;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_KEYDOWN: u32 = 0x0100;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONDOWN: u32 = 0x0204;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_CAPTURECHANGED: u32 = 0x0215;
const WM_MOUSELEAVE: u32 = 0x02A3;
/// Private message that tells the menu to take the mouse capture.
///
/// It cannot be taken inline in [`show`], because `show` runs from the strip's
/// `WM_RBUTTONUP` handler: the strip still holds the implicit capture Windows
/// grants on button-down, and releasing it once that handler returns would
/// immediately hand us a `WM_CAPTURECHANGED` and close the menu we just
/// opened. Posting defers the call until button processing has finished.
const WM_TAKE_CAPTURE: u32 = 0x8000 + 1;

const WA_INACTIVE: usize = 0;

const VK_ESCAPE: usize = 0x1B;
const VK_RETURN: usize = 0x0D;
const VK_UP: usize = 0x26;
const VK_DOWN: usize = 0x28;

const IDC_ARROW: usize = 32512;
const TME_LEAVE: u32 = 0x0000_0002;

const SPI_GETWORKAREA: u32 = 0x0030;

const DIB_RGB_COLORS: u32 = 0;
const BI_RGB: u32 = 0;
const AC_SRC_OVER: u8 = 0x00;
const AC_SRC_ALPHA: u8 = 0x01;
const ULW_ALPHA: u32 = 0x0000_0002;

const TRANSPARENT_BK: i32 = 1;
const DT_SINGLELINE: u32 = 0x0020;
const DT_VCENTER: u32 = 0x0004;
const DT_LEFT: u32 = 0x0000;
const DT_CENTER: u32 = 0x0001;
const DT_NOPREFIX: u32 = 0x0800;
const DT_END_ELLIPSIS: u32 = 0x8000;
const DT_CALCRECT: u32 = 0x0400;

const ANIM_TIMER_ID: usize = 1;

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
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
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [u32; 3],
}

#[repr(C)]
struct TrackMouseEventStruct {
    cb_size: u32,
    dw_flags: u32,
    hwnd_track: isize,
    dw_hover_time: u32,
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
    fn DestroyWindow(hwnd: isize) -> i32;
    fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn IsWindow(hwnd: isize) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, hdc: isize) -> i32;
    fn GetCursorPos(point: *mut Point) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn SystemParametersInfoW(action: u32, param: u32, data: *mut c_void, win_ini: u32) -> i32;
    fn PostMessageW(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> i32;
    fn SetTimer(hwnd: isize, id: usize, elapse: u32, func: *const c_void) -> usize;
    fn KillTimer(hwnd: isize, id: usize) -> i32;
    fn LoadCursorW(instance: isize, name: usize) -> isize;
    fn SetCapture(hwnd: isize) -> isize;
    fn ReleaseCapture() -> i32;
    fn TrackMouseEvent(event: *mut TrackMouseEventStruct) -> i32;
    fn DrawTextW(hdc: isize, text: *const u16, count: i32, rect: *mut Rect, format: u32) -> i32;
    fn UpdateLayeredWindow(
        hwnd: isize,
        hdc_dst: isize,
        ppt_dst: *const Point,
        psize: *const Size,
        hdc_src: isize,
        ppt_src: *const Point,
        crkey: u32,
        pblend: *const BlendFunction,
        flags: u32,
    ) -> i32;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(hdc: isize) -> isize;
    fn DeleteDC(hdc: isize) -> i32;
    fn CreateDIBSection(
        hdc: isize,
        bmi: *const BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: isize,
        offset: u32,
    ) -> isize;
    fn SelectObject(hdc: isize, obj: isize) -> isize;
    fn DeleteObject(obj: isize) -> i32;
    fn SetTextColor(hdc: isize, color: u32) -> u32;
    fn SetBkMode(hdc: isize, mode: i32) -> i32;
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
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> isize;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ── Layout ──────────────────────────────────────────────────────────────────

/// Every metric the menu draws with, already in physical pixels.
#[derive(Clone, Copy)]
struct Metrics {
    dpi: i32,
    margin: i32,
    blur: i32,
    shadow_dy: i32,
    radius: i32,
    row_h: i32,
    sep_block: i32,
    pad_v: i32,
    hover_inset: i32,
    hover_radius: i32,
    check_col: i32,
    check_glyph: i32,
    travel: i32,
    text_gap: i32,
    pad_right: i32,
    font_px: i32,
    min_width: i32,
}

impl Metrics {
    fn new(dpi: i32, font_size_dip: i32) -> Self {
        let s = |dip: i32| ((dip * dpi) / 96).max(1);
        Self {
            dpi,
            margin: s(SHADOW_MARGIN_DIP),
            blur: s(SHADOW_BLUR_DIP),
            shadow_dy: s(SHADOW_OFFSET_Y_DIP),
            radius: s(CORNER_RADIUS_DIP),
            row_h: s(ROW_HEIGHT_DIP),
            sep_block: s(SEPARATOR_BLOCK_DIP),
            pad_v: s(CARD_PAD_V_DIP),
            hover_inset: s(HOVER_INSET_DIP),
            hover_radius: s(HOVER_RADIUS_DIP),
            check_col: s(CHECK_COLUMN_DIP),
            check_glyph: s(CHECK_GLYPH_DIP),
            travel: s(ANIM_TRAVEL_DIP),
            text_gap: s(TEXT_GAP_DIP),
            pad_right: s(TEXT_PAD_RIGHT_DIP),
            font_px: s(font_size_dip),
            min_width: s(MIN_CARD_WIDTH_DIP),
        }
    }
}

/// One laid-out row. `top`/`height` are in window-client coordinates, i.e. the
/// card's own origin is already offset by `Metrics::margin`.
struct Row {
    item: MenuItem,
    top: i32,
    height: i32,
}

/// Colours: (card background, hover highlight, text, muted text, icon,
/// separator).
///
/// "Muted" is what disabled rows draw in — the live status readouts at the top
/// and the header a flattened submenu leaves behind. They must read as labels
/// rather than as targets you failed to click.
///
/// The light pair matches the tray panel's own surface so the menu reads as
/// part of the app rather than a system popup.
fn theme_colors(light: bool) -> (u32, u32, u32, u32, u32, u32) {
    if light {
        (
            0x00FF_FFFF,
            0x00EF_F1F0,
            0x0026_2626,
            0x0091_9191,
            0x005A_5A5A,
            0x00E4_E6E5,
        )
    } else {
        (
            0x0028_2828,
            0x003A_3A3A,
            0x00F2_F2F2,
            0x0085_8585,
            0x00C4_C4C4,
            0x0040_4040,
        )
    }
}

/// Does the menu paint on a light surface? Deliberately **not**
/// `taskbar_is_light()` alone: the strip is embedded in the taskbar and must
/// match it, but this menu floats above and is meant to read as one of the
/// app's own surfaces, so it follows the app's theme preference and only falls
/// back to the taskbar's brightness under `Auto`.
fn surface_is_light() -> bool {
    match codexbar::settings::Settings::load().theme {
        codexbar::settings::ThemePreference::Light => true,
        codexbar::settings::ThemePreference::Dark => false,
        codexbar::settings::ThemePreference::Auto => taskbar_is_light(),
    }
}

/// Width of one label, measured the way it will be drawn.
///
/// DirectWrite first, because that is what [`paint`] draws with. GDI is the
/// fallback, and only correct when the GDI fallback path is what ends up
/// drawing: GDI collapses the `wght` axis onto an installed static face, so on
/// a light or variable family it measures a different typeface than DirectWrite
/// renders. Sizing the card from GDI numbers left the menu slightly too wide or
/// ellipsized text that would have fit.
fn measure_text_width(text: &str, font_px: i32, family: &str, weight: i32) -> i32 {
    let style = crate::taskbar_text::TextStyle {
        family,
        weight: weight as f32,
        size_px: font_px as f32,
        align: crate::taskbar_text::TextAlign::Left,
        color_rgb: 0,
    };
    if let Some(width) = crate::taskbar_text::measure_width(text, &style) {
        return width.ceil() as i32;
    }
    measure_text_width_gdi(text, font_px, family, weight)
}

fn measure_text_width_gdi(text: &str, font_px: i32, family: &str, weight: i32) -> i32 {
    let hdc = unsafe { GetDC(0) };
    if hdc == 0 {
        return text.chars().count() as i32 * font_px;
    }
    let font = unsafe {
        CreateFontW(
            -font_px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            4,
            0,
            wide(family).as_ptr(),
        )
    };
    let previous = unsafe { SelectObject(hdc, font) };
    let text_w = wide(text);
    let mut rect = Rect::default();
    unsafe {
        DrawTextW(
            hdc,
            text_w.as_ptr(),
            -1,
            &raw mut rect,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(hdc, previous);
        DeleteObject(font);
        ReleaseDC(0, hdc);
    }
    rect.right - rect.left
}

/// Stacks the rows and returns `(rows, card_width, card_height)`.
fn lay_out(items: Vec<MenuItem>, m: &Metrics, family: &str, weight: i32) -> (Vec<Row>, i32, i32) {
    // **Disabled rows do not get a vote on the width.** They are the live
    // status readouts, and theirs is by far the longest string in the menu —
    // letting it size the card made the whole menu roughly twice as wide as its
    // actions needed, for a row that is not even clickable. It is trimmed with
    // an ellipsis instead, which is the trade the user accepted.
    let mut width = m.min_width;
    for item in &items {
        if item.separator || item.disabled {
            continue;
        }
        let text_w = measure_text_width(&item.label, m.font_px, family, weight);
        width = width.max(m.check_col + m.text_gap + text_w + m.pad_right);
    }

    let mut rows = Vec::with_capacity(items.len());
    let mut y = m.margin + m.pad_v;
    for item in items {
        let height = if item.separator { m.sep_block } else { m.row_h };
        rows.push(Row {
            item,
            top: y,
            height,
        });
        y += height;
    }
    let card_h = y + m.pad_v - m.margin;
    (rows, width, card_h)
}

// ── Pixel composition ───────────────────────────────────────────────────────

/// Signed distance from `(x, y)` to a rounded rectangle. Negative inside.
fn rounded_box_sdf(x: f32, y: f32, rect: (f32, f32, f32, f32), radius: f32) -> f32 {
    let (l, t, r, b) = rect;
    let half_w = (r - l) * 0.5;
    let half_h = (b - t) * 0.5;
    let cx = l + half_w;
    let cy = t + half_h;
    let radius = radius.min(half_w).min(half_h).max(0.0);
    let dx = (x - cx).abs() - (half_w - radius);
    let dy = (y - cy).abs() - (half_h - radius);
    let outside = dx.max(0.0).hypot(dy.max(0.0));
    let inside = dx.max(dy).min(0.0);
    outside + inside - radius
}

/// Antialiased coverage of a rounded rectangle at a pixel centre.
fn coverage(x: i32, y: i32, rect: (f32, f32, f32, f32), radius: f32) -> f32 {
    let d = rounded_box_sdf(x as f32 + 0.5, y as f32 + 0.5, rect, radius);
    (0.5 - d).clamp(0.0, 1.0)
}

/// Separable box blur, run three times to approximate a Gaussian. Operates on
/// a single-channel f32 mask sized `w * h`.
fn blur_mask(mask: &mut Vec<f32>, w: i32, h: i32, radius: i32) {
    if radius <= 0 {
        return;
    }
    let mut scratch = vec![0.0f32; mask.len()];
    for _ in 0..3 {
        // Horizontal.
        for y in 0..h {
            let base = (y * w) as usize;
            let mut sum = 0.0;
            for x in -radius..=radius {
                sum += mask[base + x.clamp(0, w - 1) as usize];
            }
            let scale = 1.0 / (2 * radius + 1) as f32;
            for x in 0..w {
                scratch[base + x as usize] = sum * scale;
                let out = (x - radius).clamp(0, w - 1) as usize;
                let inn = (x + radius + 1).clamp(0, w - 1) as usize;
                sum += mask[base + inn] - mask[base + out];
            }
        }
        // Vertical.
        let scale = 1.0 / (2 * radius + 1) as f32;
        for x in 0..w {
            let mut sum = 0.0;
            for y in -radius..=radius {
                sum += scratch[(y.clamp(0, h - 1) * w + x) as usize];
            }
            for y in 0..h {
                mask[(y * w + x) as usize] = sum * scale;
                let out = (y - radius).clamp(0, h - 1);
                let inn = (y + radius + 1).clamp(0, h - 1);
                sum += scratch[(inn * w + x) as usize] - scratch[(out * w + x) as usize];
            }
        }
    }
}

/// Builds the premultiplied BGRA bytes for the card and its shadow, plus the
/// alpha channel on its own so [`restore_alpha`] can put it back after GDI has
/// trampled it.
fn compose_base(w: i32, h: i32, card: (f32, f32, f32, f32), m: &Metrics, light: bool) -> (Vec<u8>, Vec<u8>) {
    let (bg, ..) = theme_colors(light);
    let bg_r = ((bg >> 16) & 0xFF) as f32;
    let bg_g = ((bg >> 8) & 0xFF) as f32;
    let bg_b = (bg & 0xFF) as f32;

    // The shadow is the card silhouette, nudged down, blurred wide and kept
    // faint. Black, so its premultiplied colour contribution is zero and only
    // its alpha matters.
    let mut shadow = vec![0.0f32; (w * h) as usize];
    let shadow_rect = (
        card.0,
        card.1 + m.shadow_dy as f32,
        card.2,
        card.3 + m.shadow_dy as f32,
    );
    for y in 0..h {
        for x in 0..w {
            shadow[(y * w + x) as usize] = coverage(x, y, shadow_rect, m.radius as f32);
        }
    }
    blur_mask(&mut shadow, w, h, m.blur);
    // Dark keeps the same ratio to light it has always had — a shadow needs
    // more opacity to register against a dark surface than a white one.
    let strength = if light { 0.15 } else { 0.32 };

    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let mut alpha = vec![0u8; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let idx = (y * w + x) as usize;
            let c = coverage(x, y, card, m.radius as f32);
            let s = (shadow[idx] * strength).clamp(0.0, 1.0);
            let a = c + s * (1.0 - c);
            let out = idx * 4;
            pixels[out] = (bg_b * c) as u8;
            pixels[out + 1] = (bg_g * c) as u8;
            pixels[out + 2] = (bg_r * c) as u8;
            pixels[out + 3] = (a * 255.0) as u8;
            alpha[idx] = (a * 255.0) as u8;
        }
    }
    (pixels, alpha)
}

/// Blends an opaque rounded rectangle (the hover highlight) into premultiplied
/// pixels that are already fully opaque there.
fn blend_highlight(pixels: &mut [u8], w: i32, h: i32, rect: (f32, f32, f32, f32), radius: f32, color: u32) {
    let cr = ((color >> 16) & 0xFF) as f32;
    let cg = ((color >> 8) & 0xFF) as f32;
    let cb = (color & 0xFF) as f32;
    let x0 = (rect.0.floor() as i32 - 1).max(0);
    let x1 = (rect.2.ceil() as i32 + 1).min(w);
    let y0 = (rect.1.floor() as i32 - 1).max(0);
    let y1 = (rect.3.ceil() as i32 + 1).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let c = coverage(x, y, rect, radius);
            if c <= 0.0 {
                continue;
            }
            let out = ((y * w + x) * 4) as usize;
            for (offset, channel) in [(0, cb), (1, cg), (2, cr)] {
                let existing = pixels[out + offset] as f32;
                pixels[out + offset] = (existing + (channel - existing) * c) as u8;
            }
        }
    }
}

/// Writes a 1px separator line into premultiplied opaque pixels.
fn blend_separator(pixels: &mut [u8], w: i32, y: i32, x0: i32, x1: i32, color: u32) {
    if y < 0 {
        return;
    }
    let cr = ((color >> 16) & 0xFF) as u8;
    let cg = ((color >> 8) & 0xFF) as u8;
    let cb = (color & 0xFF) as u8;
    for x in x0.max(0)..x1.min(w) {
        let out = ((y * w + x) * 4) as usize;
        pixels[out] = cb;
        pixels[out + 1] = cg;
        pixels[out + 2] = cr;
    }
}

/// Puts the alpha channel back after GDI text/glyph drawing zeroed it.
fn restore_alpha(pixels: &mut [u8], alpha: &[u8]) {
    for (i, a) in alpha.iter().enumerate() {
        pixels[i * 4 + 3] = *a;
    }
}

// ── DIB wrapper ─────────────────────────────────────────────────────────────

/// A top-down 32bpp DIB section with its own memory DC. Handles are stored as
/// `isize`/`usize` so [`MenuState`] stays `Send`.
struct Dib {
    hdc: isize,
    bitmap: isize,
    previous: isize,
    bits: usize,
    len: usize,
}

impl Dib {
    fn new(w: i32, h: i32) -> Option<Self> {
        let info = BitmapInfo {
            header: BitmapInfoHeader {
                size: std::mem::size_of::<BitmapInfoHeader>() as u32,
                width: w,
                // Negative height = top-down, so row 0 is the top row and the
                // composition code can index straight from `y * w + x`.
                height: -h,
                planes: 1,
                bit_count: 32,
                compression: BI_RGB,
                size_image: 0,
                x_pels_per_meter: 0,
                y_pels_per_meter: 0,
                clr_used: 0,
                clr_important: 0,
            },
            colors: [0; 3],
        };
        let hdc = unsafe { CreateCompatibleDC(0) };
        if hdc == 0 {
            return None;
        }
        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = unsafe {
            CreateDIBSection(hdc, &raw const info, DIB_RGB_COLORS, &raw mut bits, 0, 0)
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
            len: (w * h * 4) as usize,
        })
    }

    fn slice(&self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.bits as *mut u8, self.len) }
    }

    fn destroy(&self) {
        unsafe {
            SelectObject(self.hdc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.hdc);
        }
    }
}

// ── State ───────────────────────────────────────────────────────────────────

struct MenuState {
    hwnd: isize,
    owner: isize,
    rows: Vec<Row>,
    metrics: Metrics,
    light: bool,
    family: String,
    font_weight: i32,
    /// Full window size, card plus shadow margin on all sides.
    size: Size,
    card_w: i32,
    /// Premultiplied card + shadow, before rows are drawn.
    base: Vec<u8>,
    alpha: Vec<u8>,
    canvas: Dib,
    /// Where the window rests once the open animation finishes. The animation
    /// moves the window, so this is the only record of its real position.
    origin: Point,
    hovered: Option<usize>,
    shown_at: Instant,
    animating: bool,
    /// Set the moment teardown starts. `DestroyWindow` releases the mouse
    /// capture, which sends `WM_CAPTURECHANGED`, whose handler calls `close`
    /// again — this flag is what stops that from recursing.
    closing: bool,
    /// Whether the menu has actually taken the mouse capture yet. Until it
    /// has, a `WM_CAPTURECHANGED` is somebody else's capture changing hands
    /// and must not dismiss us.
    captured: bool,
}

static MENU: Mutex<Option<MenuState>> = Mutex::new(None);
static CLASS_REGISTERED: Mutex<bool> = Mutex::new(false);

/// Is our popup menu currently on screen?
///
/// `shell::flyout_window`'s low-level mouse hook consults this: the menu paints
/// outside the tray flyout's rect, so without the carve-out a click on one of
/// its rows reads as an outside click and hides the flyout mid-interaction.
/// This replaces the old `FindWindowW("#32768")` probe, which no longer matches
/// anything now that the menu is our own window class.
pub fn is_open() -> bool {
    MENU.lock()
        .map(|guard| {
            guard
                .as_ref()
                .map(|state| !state.closing && unsafe { IsWindow(state.hwnd) } != 0)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

fn register_class() -> bool {
    let mut registered = match CLASS_REGISTERED.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if *registered {
        return true;
    }
    let class_name = wide(CLASS_NAME);
    let class = WndClassW {
        // No CS_DROPSHADOW: the shadow is ours, drawn into the layered
        // surface. No class background brush either — every pixel comes from
        // `UpdateLayeredWindow`.
        style: 0,
        lpfn_wnd_proc: Some(menu_proc),
        cb_cls_extra: 0,
        cb_wnd_extra: 0,
        h_instance: unsafe { GetModuleHandleW(std::ptr::null()) },
        h_icon: 0,
        h_cursor: unsafe { LoadCursorW(0, IDC_ARROW) },
        hbr_background: 0,
        lpsz_menu_name: std::ptr::null(),
        lpsz_class_name: class_name.as_ptr(),
    };
    let atom = unsafe { RegisterClassW(&raw const class) };
    *registered = atom != 0;
    *registered
}

/// Opens the menu anchored above the taskbar at the cursor's x position.
///
/// Non-blocking, unlike the `TrackPopupMenu` it replaces: the selected item's
/// id arrives at `owner` as a `WM_COMMAND` wparam once the user picks a row,
/// which is the same shape the old `TPM_RETURNCMD` path posted, so command
/// handlers need no change.
pub fn show(owner: isize, items: Vec<MenuItem>) {
    if items.is_empty() || !register_class() {
        return;
    }
    close();

    let dpi = {
        let value = unsafe { GetDpiForWindow(owner) };
        if value == 0 { 96 } else { value as i32 }
    };
    // Read once per open, not per frame: the menu is rebuilt each time it
    // appears, so a change in Settings takes effect on the next right-click.
    let settings = codexbar::settings::Settings::load();
    let weight = settings.menu_font_weight as i32;
    let family = if settings.menu_font_family.trim().is_empty() {
        FALLBACK_FONT_FAMILY.to_string()
    } else {
        settings.menu_font_family.clone()
    };
    let font_size_dip = if (10..=16).contains(&(settings.menu_font_size as i32)) {
        settings.menu_font_size as i32
    } else {
        FALLBACK_FONT_SIZE_DIP
    };
    let metrics = Metrics::new(dpi, font_size_dip);
    let light = surface_is_light();
    let (rows, card_w, card_h) = lay_out(items, &metrics, &family, weight);
    let size = Size {
        cx: card_w + metrics.margin * 2,
        cy: card_h + metrics.margin * 2,
    };

    let card = (
        metrics.margin as f32,
        metrics.margin as f32,
        (metrics.margin + card_w) as f32,
        (metrics.margin + card_h) as f32,
    );
    let (base, alpha) = compose_base(size.cx, size.cy, card, &metrics, light);

    let Some(canvas) = Dib::new(size.cx, size.cy) else {
        return;
    };
    let (x, y) = anchor_position(size, &metrics);
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let class_name = wide(CLASS_NAME);
    let empty_title = wide("");
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name.as_ptr(),
            empty_title.as_ptr(),
            WS_POPUP,
            x,
            y,
            size.cx,
            size.cy,
            0,
            0,
            instance,
            std::ptr::null_mut(),
        )
    };
    if hwnd == 0 {
        canvas.destroy();
        return;
    }

    let state = MenuState {
        hwnd,
        owner,
        rows,
        metrics,
        light,
        family,
        font_weight: weight,
        size,
        card_w,
        base,
        alpha,
        canvas,
        origin: Point { x, y },
        hovered: None,
        shown_at: Instant::now(),
        animating: true,
        closing: false,
        captured: false,
    };
    if let Ok(mut guard) = MENU.lock() {
        *guard = Some(state);
    } else {
        unsafe { DestroyWindow(hwnd) };
        return;
    }

    render_card();
    present(0.0);
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        // Foreground gives keyboard navigation and deactivation-based
        // dismissal; capture makes outside clicks reach us whether or not the
        // foreground request was granted. The strip answers `WM_MOUSEACTIVATE`
        // with `MA_NOACTIVATE`, so the process may well not be foreground when
        // the right-click arrives and `SetForegroundWindow` can be refused —
        // capture is what keeps dismissal working when it is.
        SetForegroundWindow(hwnd);
        PostMessageW(hwnd, WM_TAKE_CAPTURE, 0, 0);
        SetTimer(hwnd, ANIM_TIMER_ID, ANIM_TICK_MS, std::ptr::null());
    }
}

/// Bottom edge on the taskbar's top edge, left edge at the cursor, clamped to
/// the work area. `SPI_GETWORKAREA` excludes the taskbar, so its bottom *is*
/// the taskbar's top; when the taskbar is not at the bottom the work area's
/// bottom is the screen's, and the cursor is the better anchor.
fn anchor_position(size: Size, m: &Metrics) -> (i32, i32) {
    let mut cursor = Point::default();
    unsafe { GetCursorPos(&raw mut cursor) };
    let mut work = Rect::default();
    let has_work =
        unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, &raw mut work as *mut _, 0) != 0 };
    let gap = (TASKBAR_GAP_DIP * m.dpi) / 96;

    let card_bottom = if has_work && cursor.y >= work.bottom {
        work.bottom - gap
    } else {
        cursor.y - gap
    };
    let mut card_left = cursor.x;
    if has_work {
        let card_w = size.cx - m.margin * 2;
        card_left = card_left.min(work.right - card_w - gap).max(work.left + gap);
    }
    // The window is the card grown by `margin` on every side, so the card's
    // bottom edge sits `size.cy - margin` below the window's top.
    (card_left - m.margin, card_bottom - size.cy + m.margin)
}

// ── Rendering ───────────────────────────────────────────────────────────────

/// Composes the whole card — background, hover highlight, separators, icons,
/// labels — into `canvas`. Does not present it.
fn render_card() {
    let Ok(mut guard) = MENU.lock() else { return };
    let Some(state) = guard.as_mut() else { return };

    let (_, hover, text_color, muted_color, icon_color, sep_color) = theme_colors(state.light);
    let m = state.metrics;
    let w = state.size.cx;
    let h = state.size.cy;

    let pixels = state.canvas.slice();
    pixels.copy_from_slice(&state.base);

    if let Some(index) = state.hovered {
        if let Some(row) = state.rows.get(index) {
            let rect = (
                (m.margin + m.hover_inset) as f32,
                row.top as f32,
                (m.margin + state.card_w - m.hover_inset) as f32,
                (row.top + row.height) as f32,
            );
            blend_highlight(pixels, w, h, rect, m.hover_radius as f32, hover);
        }
    }

    for row in &state.rows {
        if row.item.separator {
            blend_separator(
                pixels,
                w,
                row.top + row.height / 2,
                m.margin + m.hover_inset,
                m.margin + state.card_w - m.hover_inset,
                sep_color,
            );
        }
    }

    // Text and the check glyph go through GDI, which is not alpha-aware — it
    // zeroes the alpha byte of every pixel it touches. `restore_alpha` below
    // repairs that. Safe because rows are inset well inside the card, never
    // over the antialiased edge where alpha is fractional.
    unsafe { SetBkMode(state.canvas.hdc, TRANSPARENT_BK) };
    let check_font = create_font(m.check_glyph, 400, "Segoe MDL2 Assets");

    // The check mark sits in the left column, where the per-row icons used to
    // be. Unchecked rows leave it empty rather than shifting their label, so
    // every label starts on the same x. Still GDI: it is one icon glyph, with
    // no weight axis to honour.
    for row in &state.rows {
        if row.item.checked && !row.item.separator {
            let mut check = Rect {
                left: m.margin,
                top: row.top,
                right: m.margin + m.check_col,
                bottom: row.top + row.height,
            };
            draw_glyph(state.canvas.hdc, check_font, &mut check, CHECK_GLYPH, icon_color);
        }
    }
    unsafe { DeleteObject(check_font) };

    // Labels go through the same DirectWrite renderer the strip uses, so the
    // weight setting drives a real `wght` axis rather than GDI's three faces.
    // Two passes because `TextStyle` carries one colour and disabled rows are
    // muted; each pass binds its own DC render target.
    //
    // `draw_lines` binds to `bounds` and puts the drawing origin at its
    // top-left, so line rects are *relative* to it. Binding the whole canvas
    // makes relative and absolute coincide, which is why the rects below are
    // plain canvas coordinates.
    let canvas_bounds = crate::taskbar_text::WinRect {
        left: 0,
        top: 0,
        right: w,
        bottom: h,
    };
    let label_rect = |row: &Row| crate::taskbar_text::WinRect {
        left: m.margin + m.check_col + m.text_gap,
        top: row.top,
        right: m.margin + state.card_w - m.pad_right,
        bottom: row.top + row.height,
    };

    let mut drawn = true;
    for (disabled, colour) in [(false, text_color), (true, muted_color)] {
        let lines: Vec<crate::taskbar_text::TextLine<'_>> = state
            .rows
            .iter()
            .filter(|row| !row.item.separator && row.item.disabled == disabled)
            .map(|row| crate::taskbar_text::TextLine {
                text: row.item.label.as_str(),
                rect: label_rect(row),
                mark: None,
            })
            .collect();
        if lines.is_empty() {
            continue;
        }
        drawn &= crate::taskbar_text::draw_lines(
            state.canvas.hdc,
            canvas_bounds,
            &lines,
            &crate::taskbar_text::TextStyle {
                family: &state.family,
                weight: state.font_weight as f32,
                size_px: m.font_px as f32,
                align: crate::taskbar_text::TextAlign::Left,
                color_rgb: colour,
            },
        );
    }

    if !drawn {
        // GDI fallback, mirroring the strip's own: the menu must still show
        // readable labels if DirectWrite is unavailable. Weight is whatever
        // GDI can map the axis value onto, which is the limitation the
        // DirectWrite path exists to escape.
        let text_font = create_font(m.font_px, state.font_weight, &state.family);
        for row in &state.rows {
            if row.item.separator {
                continue;
            }
            let previous = unsafe { SelectObject(state.canvas.hdc, text_font) };
            let colour = if row.item.disabled {
                muted_color
            } else {
                text_color
            };
            unsafe { SetTextColor(state.canvas.hdc, colour) };
            let label = wide(&row.item.label);
            let bounds = label_rect(row);
            let mut rect = Rect {
                left: bounds.left,
                top: bounds.top,
                right: bounds.right,
                bottom: bounds.bottom,
            };
            unsafe {
                DrawTextW(
                    state.canvas.hdc,
                    label.as_ptr(),
                    -1,
                    &raw mut rect,
                    DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS,
                );
                SelectObject(state.canvas.hdc, previous);
            }
        }
        unsafe { DeleteObject(text_font) };
    }
    restore_alpha(pixels, &state.alpha);
}

fn create_font(px: i32, weight: i32, family: &str) -> isize {
    unsafe {
        CreateFontW(
            -px,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            // ANTIALIASED_QUALITY, not ClearType: subpixel antialiasing on a
            // layered surface produces coloured fringes against whatever ends
            // up behind the menu.
            4,
            0,
            wide(family).as_ptr(),
        )
    }
}

fn draw_glyph(hdc: isize, font: isize, rect: &mut Rect, glyph: char, color: u32) {
    let previous = unsafe { SelectObject(hdc, font) };
    unsafe { SetTextColor(hdc, color) };
    let mut buf = [0u16; 2];
    let encoded = glyph.encode_utf16(&mut buf);
    let len = encoded.len() as i32;
    unsafe {
        DrawTextW(
            hdc,
            encoded.as_ptr(),
            len,
            &raw mut *rect,
            DT_SINGLELINE | DT_CENTER | DT_VCENTER | DT_NOPREFIX,
        );
        SelectObject(hdc, previous);
    }
}

/// Where the window sits and how opaque it is at a given point in the open
/// animation. `progress` is already eased. Returns `(y offset below the resting
/// place, constant alpha)`.
///
/// Split out from [`present`] so the curve can be tested without a window.
fn slide_frame(progress: f32, travel: i32) -> (i32, u8) {
    let remaining = (1.0 - progress).clamp(0.0, 1.0);
    // Opacity leads the movement — the card should be readable by the time it
    // is most of the way up, not still fading in as it settles.
    let opacity = (progress * 1.6).clamp(0.0, 1.0);
    ((remaining * travel as f32).round() as i32, (opacity * 255.0) as u8)
}

/// Pushes `canvas` to the screen. Below `progress` 1.0 the window is placed
/// short of its resting position and drawn at reduced opacity, which is the
/// whole open animation: the bitmap itself is never touched.
fn present(progress: f32) {
    let Ok(guard) = MENU.lock() else { return };
    let Some(state) = guard.as_ref() else { return };

    let (drop, alpha) = if progress >= 1.0 {
        (0, 255)
    } else {
        slide_frame(progress, state.metrics.travel)
    };
    let blend = BlendFunction {
        blend_op: AC_SRC_OVER,
        blend_flags: 0,
        source_constant_alpha: alpha,
        alpha_format: AC_SRC_ALPHA,
    };
    // `UpdateLayeredWindow` moves the window when given a destination point,
    // so the slide costs one field rather than a `SetWindowPos` per frame.
    let destination = Point {
        x: state.origin.x,
        y: state.origin.y + drop,
    };
    let source_origin = Point::default();

    unsafe {
        UpdateLayeredWindow(
            state.hwnd,
            0,
            &raw const destination,
            &raw const state.size,
            state.canvas.hdc,
            &raw const source_origin,
            0,
            &raw const blend,
            ULW_ALPHA,
        )
    };
}

// ── Interaction ─────────────────────────────────────────────────────────────

/// Which row is under `(x, y)` in window-client coordinates, if any.
///
/// The x bound matters because the menu holds the mouse capture: coordinates
/// well outside the window still arrive here, and only checking y would treat
/// a click three monitors away as a row hit.
fn hit_test(x: i32, y: i32) -> Option<usize> {
    let guard = MENU.lock().ok()?;
    let state = guard.as_ref()?;
    let m = &state.metrics;
    if x < m.margin || x >= m.margin + state.card_w {
        return None;
    }
    state.rows.iter().position(|row| {
        !row.item.separator && !row.item.disabled && y >= row.top && y < row.top + row.height
    })
}

fn mouse_position(lparam: isize) -> (i32, i32) {
    (
        (lparam & 0xFFFF) as u16 as i16 as i32,
        ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32,
    )
}

fn set_hovered(next: Option<usize>) {
    let changed = match MENU.lock() {
        Ok(mut guard) => match guard.as_mut() {
            Some(state) if state.hovered != next => {
                state.hovered = next;
                !state.animating
            }
            _ => false,
        },
        Err(_) => false,
    };
    if changed {
        render_card();
        present(1.0);
    }
}

/// Moves the highlight to the next selectable row in `delta` direction,
/// wrapping, so Up/Down work the way they did under `TrackPopupMenu`.
fn move_hover(delta: i32) {
    let next = {
        let Ok(guard) = MENU.lock() else { return };
        let Some(state) = guard.as_ref() else { return };
        let selectable: Vec<usize> = state
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| !row.item.separator && !row.item.disabled)
            .map(|(i, _)| i)
            .collect();
        if selectable.is_empty() {
            return;
        }
        let current = state
            .hovered
            .and_then(|h| selectable.iter().position(|i| *i == h));
        let position = match current {
            Some(p) => (p as i32 + delta).rem_euclid(selectable.len() as i32) as usize,
            None if delta > 0 => 0,
            None => selectable.len() - 1,
        };
        Some(selectable[position])
    };
    set_hovered(next);
}

/// Closes the menu and, when a row was picked, posts its id to the owner.
///
/// The `WM_COMMAND` is posted *after* the window is gone so the handler runs
/// with the menu already dismissed — the old code relied on `TrackPopupMenu`
/// having returned for the same reason.
fn commit(index: Option<usize>) {
    let selected = {
        let Ok(guard) = MENU.lock() else { return };
        let Some(state) = guard.as_ref() else { return };
        let id = index
            .and_then(|i| state.rows.get(i))
            .filter(|row| !row.item.separator && !row.item.disabled)
            .map(|row| row.item.id);
        id.map(|id| (state.owner, id))
    };
    close();
    if let Some((owner, id)) = selected {
        if id != 0 {
            unsafe { PostMessageW(owner, WM_COMMAND, id, 0) };
        }
    }
}

/// Dismisses the menu if one is open. Safe to call when none is, and safe to
/// re-enter — teardown itself provokes messages whose handlers call back here.
pub fn close() {
    let hwnd = match MENU.lock() {
        Ok(mut guard) => match guard.as_mut() {
            Some(state) if !state.closing => {
                state.closing = true;
                state.hwnd
            }
            _ => return,
        },
        Err(_) => return,
    };
    if unsafe { IsWindow(hwnd) } != 0 {
        unsafe { DestroyWindow(hwnd) };
    }
    // `WM_DESTROY` normally releases the state; do it unconditionally so a
    // failed `DestroyWindow` cannot leave a stale entry that blocks the next
    // `show`.
    release_state();
}

/// Drops the live menu, freeing its GDI objects. Idempotent.
fn release_state() {
    let state = MENU.lock().ok().and_then(|mut guard| guard.take());
    if let Some(state) = state {
        state.canvas.destroy();
    }
}

fn advance_animation() {
    let (elapsed, hwnd) = {
        let Ok(guard) = MENU.lock() else { return };
        let Some(state) = guard.as_ref() else { return };
        (state.shown_at.elapsed().as_millis(), state.hwnd)
    };
    if elapsed >= ANIM_DURATION_MS {
        unsafe { KillTimer(hwnd, ANIM_TIMER_ID) };
        if let Ok(mut guard) = MENU.lock() {
            if let Some(state) = guard.as_mut() {
                state.animating = false;
            }
        }
        present(1.0);
        return;
    }
    let p = elapsed as f32 / ANIM_DURATION_MS as f32;
    // Ease-out cubic: the circle sweeps out fast and settles, so the menu
    // feels like it is already there rather than still arriving.
    present(1.0 - (1.0 - p).powi(3));
}

fn track_mouse_leave(hwnd: isize) {
    let mut track = TrackMouseEventStruct {
        cb_size: std::mem::size_of::<TrackMouseEventStruct>() as u32,
        dw_flags: TME_LEAVE,
        hwnd_track: hwnd,
        dw_hover_time: 0,
    };
    unsafe { TrackMouseEvent(&raw mut track) };
}

unsafe extern "system" fn menu_proc(hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize {
    match msg {
        // Every pixel arrives via UpdateLayeredWindow; there is nothing for
        // the default erase to do but flicker.
        WM_ERASEBKGND => 1,
        WM_TAKE_CAPTURE => {
            unsafe { SetCapture(hwnd) };
            if let Ok(mut guard) = MENU.lock() {
                if let Some(state) = guard.as_mut() {
                    state.captured = true;
                }
            }
            0
        }
        WM_TIMER if wparam == ANIM_TIMER_ID => {
            advance_animation();
            0
        }
        WM_MOUSEMOVE => {
            track_mouse_leave(hwnd);
            let (x, y) = mouse_position(lparam);
            set_hovered(hit_test(x, y));
            0
        }
        WM_MOUSELEAVE => {
            set_hovered(None);
            0
        }
        // Press anywhere outside dismisses without acting, the way the system
        // menu did. Inside, the row is committed on release rather than press
        // so a press-and-drag off the menu can still be abandoned.
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let (x, y) = mouse_position(lparam);
            if hit_test(x, y).is_none() {
                close();
            }
            0
        }
        WM_LBUTTONUP => {
            let (x, y) = mouse_position(lparam);
            commit(hit_test(x, y));
            0
        }
        WM_RBUTTONUP => {
            close();
            0
        }
        // Something else took the mouse — a drag, another popup, an Explorer
        // restart. Without the menu holding capture its dismissal guarantees
        // are gone, so it should not stay up. Ignored before the menu has
        // taken capture of its own, when the message belongs to someone else.
        WM_CAPTURECHANGED => {
            let ours = MENU
                .lock()
                .ok()
                .and_then(|guard| guard.as_ref().map(|state| state.captured))
                .unwrap_or(false);
            if ours {
                close();
            }
            0
        }
        WM_KEYDOWN => {
            match wparam {
                VK_ESCAPE => close(),
                VK_UP => move_hover(-1),
                VK_DOWN => move_hover(1),
                VK_RETURN => {
                    let hovered = MENU
                        .lock()
                        .ok()
                        .and_then(|guard| guard.as_ref().and_then(|state| state.hovered));
                    commit(hovered);
                }
                _ => {}
            }
            0
        }
        // Losing activation is how a click outside reaches us: the menu takes
        // the foreground when it opens, so anything else the user clicks
        // deactivates it. The grace window guards against the activation
        // change that shows the menu producing a spurious deactivate.
        WM_ACTIVATE if wparam & 0xFFFF == WA_INACTIVE => {
            let settled = MENU
                .lock()
                .ok()
                .and_then(|guard| {
                    guard
                        .as_ref()
                        .map(|state| state.shown_at.elapsed().as_millis() >= ACTIVATE_GRACE_MS)
                })
                .unwrap_or(false);
            if settled {
                close();
            }
            0
        }
        WM_KILLFOCUS => {
            let settled = MENU
                .lock()
                .ok()
                .and_then(|guard| {
                    guard
                        .as_ref()
                        .map(|state| state.shown_at.elapsed().as_millis() >= ACTIVATE_GRACE_MS)
                })
                .unwrap_or(false);
            if settled {
                close();
            }
            0
        }
        WM_DESTROY => {
            // Release the state *first*: `ReleaseCapture` below sends
            // `WM_CAPTURECHANGED`, and its handler must find nothing to close.
            release_state();
            unsafe {
                KillTimer(hwnd, ANIM_TIMER_ID);
                ReleaseCapture();
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_items() -> Vec<MenuItem> {
        vec![
            MenuItem::action(1, "Open Panel".into()),
            MenuItem::action(2, "Refresh".into()),
            MenuItem::action(5, "Show Strip".into()).checked(true),
            MenuItem::separator(),
            MenuItem::action(4, "Quit".into()),
        ]
    }

    #[test]
    fn rows_stack_without_gaps_and_separators_are_shorter() {
        let m = Metrics::new(96, FALLBACK_FONT_SIZE_DIP);
        let (rows, _, card_h) = lay_out(sample_items(), &m, "Segoe UI", 400);
        assert_eq!(rows.len(), 5);
        for pair in rows.windows(2) {
            assert_eq!(pair[0].top + pair[0].height, pair[1].top);
        }
        assert_eq!(rows[3].height, m.sep_block);
        assert!(rows[3].height < rows[0].height);
        // Card height covers every row plus the padding above and below.
        let spanned: i32 = rows.iter().map(|r| r.height).sum();
        assert_eq!(card_h, spanned + m.pad_v * 2);
    }

    /// Checking a row must not change the card's width, because the check mark
    /// lives in the left column every row already reserves rather than in a
    /// trailing one only checked rows pay for. Otherwise toggling 显示小型状态栏
    /// would resize the menu the next time it opens.
    #[test]
    fn checking_a_row_does_not_change_the_width() {
        let m = Metrics::new(96, FALLBACK_FONT_SIZE_DIP);
        let unchecked = vec![MenuItem::action(1, "A".repeat(40))];
        let checked = vec![MenuItem::action(1, "A".repeat(40)).checked(true)];
        let (_, plain_w, _) = lay_out(unchecked, &m, "Segoe UI", 400);
        let (_, checked_w, _) = lay_out(checked, &m, "Segoe UI", 400);
        assert_eq!(plain_w, checked_w);
    }

    /// The card must start below its resting place and end exactly on it —
    /// an off-by-one here leaves the menu permanently a pixel low, which is
    /// invisible in a screenshot and obvious in motion.
    #[test]
    fn the_slide_lands_exactly_on_the_resting_position() {
        let travel = 27;
        assert_eq!(slide_frame(0.0, travel), (travel, 0));
        let (drop, alpha) = slide_frame(0.5, travel);
        assert!(drop > 0 && drop < travel);
        assert!(alpha > 0);
        assert_eq!(slide_frame(1.0, travel).0, 0);
        // Opacity leads the movement: fully opaque before the card settles.
        assert_eq!(slide_frame(0.7, travel).1, 255);
        assert!(slide_frame(0.7, travel).0 > 0);
    }

    #[test]
    fn metrics_scale_with_dpi() {
        let low = Metrics::new(96, FALLBACK_FONT_SIZE_DIP);
        let high = Metrics::new(192, FALLBACK_FONT_SIZE_DIP);
        assert_eq!(high.row_h, low.row_h * 2);
        assert_eq!(high.radius, low.radius * 2);
        assert_eq!(high.font_px, low.font_px * 2);
    }

    #[test]
    fn rounded_box_coverage_is_one_inside_and_zero_outside() {
        let rect = (10.0, 10.0, 110.0, 60.0);
        assert_eq!(coverage(60, 35, rect, 8.0), 1.0);
        assert_eq!(coverage(0, 0, rect, 8.0), 0.0);
        // The corner is cut away by the radius; the same point on a square
        // rectangle would be fully covered.
        assert!(coverage(11, 11, rect, 8.0) < 1.0);
        assert_eq!(coverage(11, 11, rect, 0.0), 1.0);
    }

    #[test]
    fn blur_spreads_coverage_beyond_the_silhouette_and_conserves_it() {
        let (w, h) = (40, 40);
        let mut mask = vec![0.0f32; (w * h) as usize];
        for y in 15..25 {
            for x in 15..25 {
                mask[(y * w + x) as usize] = 1.0;
            }
        }
        let before: f32 = mask.iter().sum();
        blur_mask(&mut mask, w, h, 4);
        assert!(mask[(20 * w + 12) as usize] > 0.0, "blur must reach outside");
        assert!(mask[(20 * w + 20) as usize] < 1.0, "centre must soften");
        let after: f32 = mask.iter().sum();
        // A box blur is energy-preserving away from the edges.
        assert!((after - before).abs() / before < 0.05);
    }

    fn luminance(color: u32) -> f32 {
        let r = ((color >> 16) & 0xFF) as f32;
        let g = ((color >> 8) & 0xFF) as f32;
        let b = (color & 0xFF) as f32;
        (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255.0
    }

    /// The palette used to pin the strip's exact `paint()` text colours. That
    /// pin is gone deliberately: this menu is a card floating *above* the
    /// taskbar, not part of it, so it is free to pick its own contrast — what
    /// still has to hold is that each theme reads the right way round and a
    /// hovered row is distinguishable from an unhovered one.
    #[test]
    fn each_theme_is_legible_and_hover_is_visible() {
        for light in [true, false] {
            let (bg, hover, text, muted, icon, separator) = theme_colors(light);
            let bg_l = luminance(bg);
            assert_eq!(bg_l > 0.5, light, "background must follow the theme");
            assert!(
                (luminance(text) - bg_l).abs() > 0.5,
                "label must contrast with the card"
            );
            assert!(
                (luminance(icon) - bg_l).abs() > 0.25,
                "icon must contrast with the card"
            );
            assert_ne!(bg, hover, "a hovered row must be distinguishable");
            // Disabled rows (status readouts, flattened submenu headers) have
            // to be readable but visibly weaker than a row you can click, or
            // they invite clicks that do nothing.
            let muted_contrast = (luminance(muted) - bg_l).abs();
            assert!(muted_contrast > 0.15, "muted text must stay readable");
            assert!(
                muted_contrast < (luminance(text) - bg_l).abs(),
                "muted text must be weaker than an actionable label"
            );
            assert!(
                (luminance(separator) - bg_l).abs() > 0.005,
                "separator must be visible against the card"
            );
        }
    }

    #[test]
    fn separator_rows_are_never_hit_targets() {
        let m = Metrics::new(96, FALLBACK_FONT_SIZE_DIP);
        let (rows, ..) = lay_out(sample_items(), &m, "Segoe UI", 400);
        let separator = &rows[3];
        let hit = rows.iter().position(|row| {
            !row.item.separator
                && !row.item.disabled
                && separator.top >= row.top
                && separator.top < row.top + row.height
        });
        assert_eq!(hit, None);
    }


    /// A long status readout must not widen the card. This is the whole reason
    /// the menu was twice as wide as its actions needed.
    #[test]
    fn disabled_rows_do_not_widen_the_card() {
        let m = Metrics::new(96, FALLBACK_FONT_SIZE_DIP);
        let actions = vec![MenuItem::action(1, "Refresh".into())];
        let (_, actions_only, _) = lay_out(actions, &m, "Segoe UI", 400);

        let mut with_status = vec![MenuItem::action(1, "Refresh".into())];
        let mut status = MenuItem::action(2, "Codex 81% • ".repeat(8));
        status.disabled = true;
        with_status.insert(0, status);
        let (_, widened, _) = lay_out(with_status, &m, "Segoe UI", 400);

        assert_eq!(widened, actions_only);
    }
}
