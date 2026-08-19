//! DirectWrite/Direct2D text rendering for the taskbar status strip.
//!
//! # Why this replaced GDI
//!
//! The strip used to draw with `CreateFontW` + `DrawTextW`. A live probe on this
//! machine showed GDI collapsing every requested weight from 100..=550 to
//! Regular 400 and everything above to Bold 700, because GDI has no access to a
//! font's OpenType variation axes — it can only pick among installed static
//! faces. That is why the earlier "100–900 slider" was reduced to three honest
//! stops.
//!
//! DirectWrite does expose the axes. `IDWriteTextFormat3::SetFontAxisValues`
//! sets the `wght` axis directly, so a variable font renders a genuinely
//! different weight for every value.
//!
//! # The honest limit
//!
//! Continuity is a property of the *font*, not of DirectWrite. `Segoe UI
//! Variable` ships with Windows 11 and has a real `wght` axis; `Microsoft YaHei
//! UI` does not — it is a family of static faces, so DirectWrite can only pick
//! the nearest installed one there too. [`FontFamilyInfo::variable_weight`]
//! reports which is which so the settings UI can tell the truth instead of
//! offering a slider that silently snaps.
//!
//! # Why Direct2D and not a custom IDWriteTextRenderer
//!
//! Drawing an `IDWriteTextLayout` directly requires implementing the
//! `IDWriteTextRenderer` callback interface. `ID2D1DCRenderTarget::BindDC` plus
//! `DrawTextLayout` reaches the same pixels with consumer-side calls only, and
//! it renders onto the *existing* HDC — which matters, because the strip's
//! layered-window colour-key transparency, taskbar background sampling and
//! hit-testing took several rounds to get right and must not be disturbed.

#![cfg(windows)]

use std::cell::RefCell;

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_IGNORE, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_SOFTWARE,
    D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE, D2D1_ROUNDED_RECT, D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
    ID2D1DCRenderTarget, ID2D1Factory, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_AXIS_ATTRIBUTES_VARIABLE,
    DWRITE_FONT_AXIS_RANGE, DWRITE_FONT_AXIS_TAG_WEIGHT, DWRITE_FONT_AXIS_VALUE,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_METRICS, DWRITE_TRIMMING, DWRITE_TRIMMING_GRANULARITY_CHARACTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_WORD_WRAPPING_NO_WRAP,
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteFontFace5,
    IDWriteFontFamily2, IDWriteTextFormat, IDWriteTextFormat3,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::core::Interface;

/// Horizontal placement of a line inside the strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl TextAlign {
    fn to_dwrite(self) -> DWRITE_TEXT_ALIGNMENT {
        match self {
            Self::Left => DWRITE_TEXT_ALIGNMENT_LEADING,
            Self::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
            Self::Right => DWRITE_TEXT_ALIGNMENT_TRAILING,
        }
    }
}

/// Re-exported so `taskbar_widget` can build rectangles without taking a direct
/// dependency on the `windows` crate: everything else in that module is
/// hand-rolled FFI, and mixing the two vocabularies there would be confusing.
pub type WinRect = RECT;

/// A provider mark drawn ahead of a line, in its own brand colour.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LineMark {
    pub glyph: char,
    /// 0x00RRGGBB, same convention as [`TextStyle::color_rgb`].
    pub color_rgb: u32,
}

/// One line of the strip and the rectangle it occupies, in device pixels.
pub struct TextLine<'a> {
    pub text: &'a str,
    pub rect: RECT,
    /// Drawn centred in a fixed-width column at the leading edge of `rect`,
    /// with `text` laid out in what is left. The column width is derived from
    /// the em size rather than measured per glyph, so every row's text starts
    /// on the same x — a ragged left edge is what makes a grid of readings hard
    /// to scan, and the marks are not all the same width.
    pub mark: Option<LineMark>,
}

/// How the strip should be drawn.
pub struct TextStyle<'a> {
    pub family: &'a str,
    /// OpenType `wght` axis value, 1..=1000. Continuous only on a variable font.
    pub weight: f32,
    /// Em size in device pixels (already DPI-scaled by the caller).
    pub size_px: f32,
    pub align: TextAlign,
    /// 0x00RRGGBB, matching the GDI convention the rest of the module uses.
    pub color_rgb: u32,
}

// The factories are cheap to keep and expensive to recreate, and every paint
// happens on the UI thread, so a thread-local single-threaded D2D factory is
// both the fastest and the simplest correct option.
thread_local! {
    static RENDERER: RefCell<Option<Renderer>> = const { RefCell::new(None) };
}

struct Renderer {
    dwrite: IDWriteFactory,
    target: ID2D1DCRenderTarget,
}

impl Renderer {
    fn new() -> windows::core::Result<Self> {
        let d2d: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let dwrite: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };

        // `D2D1_ALPHA_MODE_IGNORE` keeps this an opaque draw onto the DC, which
        // is what the colour-key transparency downstream expects: the strip
        // paints the sampled taskbar colour and Explorer's pixels show through
        // wherever that exact colour survives.
        // Software, not DEFAULT. DEFAULT lets Direct2D spin up a D3D device;
        // for a two-line text strip drawn onto a GDI DC that buys nothing and
        // costs a GPU/driver dependency — and it was observed to block
        // indefinitely when the process has no COM apartment or no usable
        // adapter, which is exactly the situation in a test binary. The
        // rasterizer's output is identical here.
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_IGNORE,
            },
            dpiX: 0.0,
            dpiY: 0.0,
            usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let target = unsafe { d2d.CreateDCRenderTarget(&properties)? };

        // Grayscale, never ClearType. ClearType's subpixel fringes are coloured,
        // and a coloured fringe next to a colour-keyed background is exactly the
        // magenta/blue edge artifact this widget already fought once.
        unsafe { target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE) };

        Ok(Self { dwrite, target })
    }
}

/// Draw the strip's lines onto `hdc`.
///
/// `bounds` is the full client rectangle; it is what the render target is bound
/// to, so every line rectangle must be expressed in the same coordinate space.
/// Returns `false` if DirectWrite could not be used at all, letting the caller
/// keep a GDI fallback rather than painting nothing.
pub fn draw_lines(hdc: isize, bounds: RECT, lines: &[TextLine<'_>], style: &TextStyle<'_>) -> bool {
    RENDERER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            match Renderer::new() {
                Ok(renderer) => *slot = Some(renderer),
                Err(err) => {
                    tracing::warn!("taskbar text: DirectWrite unavailable: {err}");
                    return false;
                }
            }
        }
        let renderer = slot.as_ref().expect("renderer initialized above");
        match unsafe { draw_with(renderer, hdc, bounds, lines, style) } {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!("taskbar text: draw failed: {err}");
                // Drop the cached objects: a lost device or a stale DC binding
                // is not recoverable by retrying the same target.
                *slot = None;
                false
            }
        }
    })
}

/// Drop the cached Direct2D render target.
///
/// `BindDC` may be called repeatedly, but only while the previously bound DC is
/// still alive. Binding a cached target to a new DC after the old one was
/// destroyed was observed to hang, so any caller that destroys the DC it drew
/// onto must reset first. The taskbar strip reuses one window DC and never hits
/// this; the offscreen tests create and delete a DC per render and do —
/// which is why this is `cfg(test)`: it has never had a production caller.
#[cfg(test)]
pub fn reset_renderer() {
    RENDERER.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

// A DirectWrite factory kept for measurement alone. Deliberately *not*
// `RENDERER`: measuring must not depend on Direct2D being available, and a
// caller that only measures (the popup menu, sizing its card before it has a
// DC) should not pay for a render target it never binds.
thread_local! {
    static MEASURER: RefCell<Option<IDWriteFactory>> = const { RefCell::new(None) };
}

/// Layout width, in device pixels, of `text` under the same font, weight and em
/// size [`draw_lines`] would use. `None` when DirectWrite is unavailable, which
/// is the caller's cue to fall back to whatever it uses for drawing too.
///
/// This exists because measuring with GDI and drawing with DirectWrite is a
/// mismatch: GDI has to collapse a `wght` axis value onto an installed static
/// face, so a light or a variable family measures at one weight and draws at
/// another. The card sized from those numbers is then slightly too wide (dead
/// space on the right) or slightly too narrow (the ellipsis appears on text
/// that would have fit).
pub fn measure_width(text: &str, style: &TextStyle<'_>) -> Option<f32> {
    if text.is_empty() {
        return Some(0.0);
    }
    MEASURER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            match unsafe { DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED) } {
                Ok(factory) => *slot = Some(factory),
                Err(err) => {
                    tracing::warn!("taskbar text: DirectWrite unavailable for measurement: {err}");
                    return None;
                }
            }
        }
        let dwrite = slot.as_ref()?;
        let format = unsafe { create_format(dwrite, style) }.ok()?;
        let utf16: Vec<u16> = text.encode_utf16().collect();
        // A layout box far wider than any surface here, so the format's ellipsis
        // trimming never engages and the reported width is the natural one. Not
        // `f32::MAX` — DirectWrite does arithmetic on this, and an infinity
        // propagates into the metrics.
        let layout = unsafe { dwrite.CreateTextLayout(&utf16, &format, 1.0e6, 1.0e6) }.ok()?;
        let mut metrics = DWRITE_TEXT_METRICS::default();
        unsafe { layout.GetMetrics(&mut metrics) }.ok()?;
        Some(metrics.width)
    })
}


// ── Strip cells (official icon + dimmed tag + bold value, one cluster) ──

/// One cell of the strip under the icon+tag+value cluster model.
pub struct StripCell<'a> {
    pub rect: RECT,
    pub tag: &'a str,
    pub value: &'a str,
    /// Official SVG provider id; when None, or when the provider has no SVG,
    /// the cell falls back to `glyph` in the same slot.
    pub icon_provider: Option<&'a str>,
    /// Fallback mark drawn into the icon slot when there is no SVG.
    pub glyph: Option<LineMark>,
}

/// Draw the strip's cells, each as one left-packed `[icon][tag][value]` cluster
/// that `style.align` moves as a whole (never stretched). Returns false when
/// DirectWrite is unavailable, letting the caller keep a GDI fallback.
pub fn draw_strip_cells(
    hdc: isize,
    bounds: RECT,
    cells: &[StripCell<'_>],
    style: &TextStyle<'_>,
    background_rgb: u32,
    icon_size_px: f32,
    icon_gap_px: f32,
    value_gap_px: f32,
    icon_style: crate::taskbar_icons::IconStyle,
) -> bool {
    RENDERER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            match Renderer::new() {
                Ok(renderer) => *slot = Some(renderer),
                Err(err) => {
                    tracing::warn!("taskbar text: DirectWrite unavailable: {err}");
                    return false;
                },
            }
        }
        let renderer = slot.as_ref().expect("renderer initialized above");
        match unsafe {
            draw_strip_cells_with(renderer, hdc, bounds, cells, style, background_rgb, icon_size_px, icon_gap_px, value_gap_px, icon_style)
        } {
            Ok(()) => true,
            Err(err) => {
                tracing::warn!("taskbar text: strip cell draw failed: {err}");
                *slot = None;
                false
            },
        }
    })
}

fn blend(rgb: u32, bg: u32, t: f32) -> u32 {
    let ch = |a: u32, b: u32| -> u8 { ((a as f32 * (1.0 - t)) + (b as f32 * t)) as u8 };
    let r = ch((rgb >> 16) & 0xFF, (bg >> 16) & 0xFF);
    let g = ch((rgb >> 8) & 0xFF, (bg >> 8) & 0xFF);
    let b = ch(rgb & 0xFF, bg & 0xFF);
    (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b)
}

/// Width of one run via DirectWrite, in device pixels.
unsafe fn run_width(dwrite: &IDWriteFactory, text: &str, weight: f32, size_px: f32) -> f32 {
    if text.is_empty() { return 0.0; }
    let family: Vec<u16> = "x".encode_utf16().chain(std::iter::once(0)).collect();
    let locale: Vec<u16> = "en-us".encode_utf16().chain(std::iter::once(0)).collect();
    let Ok(format) = (unsafe {
        dwrite.CreateTextFormat(
            windows::core::PCWSTR(family.as_ptr()),
            None,
            DWRITE_FONT_WEIGHT(weight.round().clamp(1.0, 999.0) as i32),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size_px,
            windows::core::PCWSTR(locale.as_ptr()),
        )
    }) else { return 0.0 };
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let Ok(layout) = (unsafe { dwrite.CreateTextLayout(&utf16, &format, 1.0e6, 1.0e6) }) else { return 0.0 };
    let mut metrics = DWRITE_TEXT_METRICS::default();
    unsafe { layout.GetMetrics(&mut metrics) }.ok();
    metrics.width
}

unsafe fn draw_run(
    target: &ID2D1DCRenderTarget,
    format: &IDWriteTextFormat,
    text: &str,
    x: f32,
    top: f32,
    bottom: f32,
    brush: &ID2D1SolidColorBrush,
) {
    if text.is_empty() { return; }
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let rect = D2D_RECT_F { left: x, top, right: x + 1.0e6, bottom };
    unsafe {
        target.DrawText(
            &utf16,
            format,
            &rect,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        )
    };
}

unsafe fn draw_strip_cells_with(
    renderer: &Renderer,
    hdc: isize,
    bounds: RECT,
    cells: &[StripCell<'_>],
    style: &TextStyle<'_>,
    background_rgb: u32,
    icon_size_px: f32,
    icon_gap_px: f32,
    value_gap_px: f32,
    icon_style: crate::taskbar_icons::IconStyle,
) -> windows::core::Result<()> {
    let target = &renderer.target;
    unsafe {
        target.BindDC(windows::Win32::Graphics::Gdi::HDC(hdc as *mut _), &bounds)?
    };

    let tag_style = TextStyle {
        family: style.family, weight: style.weight, size_px: style.size_px,
        align: TextAlign::Left, color_rgb: style.color_rgb,
    };
    let value_style = TextStyle {
        family: style.family, weight: style.weight, size_px: style.size_px,
        align: TextAlign::Left, color_rgb: style.color_rgb,
    };
    let tag_format = unsafe { create_format(&renderer.dwrite, &tag_style)? };
    let value_format = unsafe { create_format(&renderer.dwrite, &value_style)? };
    let text_brush = unsafe { target.CreateSolidColorBrush(&color_of(style.color_rgb), None) }?;
    let tag_color = blend(style.color_rgb, background_rgb, 0.35);
    let tag_brush = unsafe { target.CreateSolidColorBrush(&color_of(tag_color), None) }?;

    let mark_style = TextStyle {
        family: style.family, weight: style.weight, size_px: style.size_px,
        align: TextAlign::Center, color_rgb: style.color_rgb,
    };
    let mark_format = unsafe { create_format(&renderer.dwrite, &mark_style)? };

    let gap = icon_gap_px.max(0.0);
    let value_gap = value_gap_px.max(0.0);
    unsafe { target.BeginDraw() };
    for cell in cells {
        let left = cell.rect.left as f32;
        let right = cell.rect.right as f32;
        let top = cell.rect.top as f32;
        let bottom = cell.rect.bottom as f32;
        let avail = (right - left).max(0.0);
        let slot_px = icon_size_px.max(1.0);
        let tag_w = unsafe { run_width(&renderer.dwrite, cell.tag, style.weight, style.size_px) };
        // Match value_style.weight so the layout width agrees with
        // the text run actually drawn; a hard-coded 600 made the value
        // drift visually away from the tag at non-default font weights.
        let value_w = unsafe { run_width(&renderer.dwrite, cell.value, style.weight, style.size_px) };
        let cluster_w = slot_px + gap + tag_w + value_gap + value_w;
        let start_x = match style.align {
            TextAlign::Left => left,
            TextAlign::Center => left + ((avail - cluster_w).max(0.0)) * 0.5,
            TextAlign::Right => left + (avail - cluster_w).max(0.0),
        };

        let ctop = top + (bottom - top - slot_px).max(0.0) * 0.5;
        let slot = crate::taskbar_icons::IconSlot { left: start_x, top: ctop, size_px: slot_px };
        let mut cursor = start_x;
        let mut drew_svg = false;
        if let Some(provider) = cell.icon_provider {
            drew_svg = crate::taskbar_icons::draw_icon(target, provider, icon_style, slot);
        }
        if !drew_svg {
            draw_glyph_fallback(target, &mark_format, cell.glyph, icon_style, slot);
        }
        cursor += slot_px + gap;

        unsafe { draw_run(target, &tag_format, cell.tag, cursor, top, bottom, &tag_brush) };
        cursor += tag_w + value_gap;
        unsafe { draw_run(target, &value_format, cell.value, cursor, top, bottom, &text_brush) };
    }
    unsafe { target.EndDraw(None, None)? };
    Ok(())
}

/// Draw the glyph/mark fallback into the icon slot with the style's tile rules
/// (badge: 18%-alpha brand tile + brand glyph; solid: solid brand tile + white
/// glyph; pure: brand glyph).
fn draw_glyph_fallback(
    target: &ID2D1DCRenderTarget,
    format: &IDWriteTextFormat,
    glyph: Option<LineMark>,
    style: crate::taskbar_icons::IconStyle,
    slot: crate::taskbar_icons::IconSlot,
) {
    let Some(mark) = glyph else { return };
    let brand = mark.color_rgb;
    let rounded = D2D1_ROUNDED_RECT {
        rect: D2D_RECT_F { left: slot.left, top: slot.top, right: slot.left + slot.size_px, bottom: slot.top + slot.size_px },
        radiusX: slot.size_px * 3.0 / 14.0,
        radiusY: slot.size_px * 3.0 / 14.0,
    };
    match style {
        crate::taskbar_icons::IconStyle::Badge => {
            if let Ok(b) = unsafe { target.CreateSolidColorBrush(&color_of_blend(brand, 0.18), None) } {
                unsafe { target.FillRoundedRectangle(&rounded, &b) };
            }
        },
        crate::taskbar_icons::IconStyle::Solid => {
            if let Ok(b) = unsafe { target.CreateSolidColorBrush(&color_of(brand), None) } {
                unsafe { target.FillRoundedRectangle(&rounded, &b) };
            }
        },
        crate::taskbar_icons::IconStyle::Pure => {},
    }
    let glyph_rgb = if style == crate::taskbar_icons::IconStyle::Solid { 0xFF_FFFFu32 } else { brand };
    let mut buf = [0u16; 2];
    let glyph: Vec<u16> = mark.glyph.encode_utf16(&mut buf).to_vec();
    if let Ok(brush) = unsafe { target.CreateSolidColorBrush(&color_of(glyph_rgb), None) } {
        let rect = D2D_RECT_F { left: slot.left, top: slot.top, right: slot.left + slot.size_px, bottom: slot.top + slot.size_px };
        unsafe {
            target.DrawText(&glyph, format, &rect, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE, DWRITE_MEASURING_MODE_NATURAL)
        };
    }
}

fn color_of_blend(rgb: u32, alpha: f32) -> D2D1_COLOR_F {
    let mut c = color_of(rgb);
    c.a = alpha;
    c
}



unsafe fn draw_with(
    renderer: &Renderer,
    hdc: isize,
    bounds: RECT,
    lines: &[TextLine<'_>],
    style: &TextStyle<'_>,
) -> windows::core::Result<()> {
    let target = &renderer.target;
    unsafe { target.BindDC(windows::Win32::Graphics::Gdi::HDC(hdc as *mut _), &bounds)? };

    let format = unsafe { create_format(&renderer.dwrite, style)? };
    // The mark sits alone in its column, so it is centred there regardless of
    // how the caller aligns the text beside it.
    let mark_format = unsafe { create_format(&renderer.dwrite, style)? };
    unsafe { mark_format.SetTextAlignment(TextAlign::Center.to_dwrite())? };
    let brush = unsafe { target.CreateSolidColorBrush(&color_of(style.color_rgb), None)? };
    let mark_brush = unsafe { target.CreateSolidColorBrush(&color_of(style.color_rgb), None)? };
    let mark_column = mark_column_width(style.size_px);

    unsafe { target.BeginDraw() };
    for line in lines {
        let mut text_left = line.rect.left as f32;

        if let Some(mark) = line.mark {
            // Never let the mark eat the whole cell: a strip too narrow to hold
            // both is better off showing the numbers than a lone glyph.
            let available = (line.rect.right - line.rect.left) as f32;
            if mark_column * 2.0 <= available {
                let glyph: Vec<u16> = {
                    let mut buf = [0u16; 2];
                    mark.glyph.encode_utf16(&mut buf).to_vec()
                };
                unsafe {
                    mark_brush.SetColor(&color_of(mark.color_rgb));
                    target.DrawText(
                        &glyph,
                        &mark_format,
                        &D2D_RECT_F {
                            left: text_left,
                            top: line.rect.top as f32,
                            right: text_left + mark_column,
                            bottom: line.rect.bottom as f32,
                        },
                        &mark_brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
                    )
                };
                text_left += mark_column;
            }
        }

        if line.text.is_empty() {
            continue;
        }
        let text: Vec<u16> = line.text.encode_utf16().collect();
        let rect = D2D_RECT_F {
            left: text_left,
            top: line.rect.top as f32,
            right: line.rect.right as f32,
            bottom: line.rect.bottom as f32,
        };
        unsafe {
            target.DrawText(
                &text,
                &format,
                &rect,
                &brush,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                windows::Win32::Graphics::DirectWrite::DWRITE_MEASURING_MODE_NATURAL,
            )
        };
    }
    unsafe { target.EndDraw(None, None)? };
    Ok(())
}

/// Width of the mark column, including the gap before the text.
///
/// Proportional to the em size so it holds at any font size and DPI. 1.15em
/// covers the widest mark in the registry — the CJK `阿` used by the Alibaba
/// providers, which is a full em — plus a thin gap.
fn mark_column_width(size_px: f32) -> f32 {
    (size_px * 1.15).max(1.0)
}

unsafe fn create_format(
    dwrite: &IDWriteFactory,
    style: &TextStyle<'_>,
) -> windows::core::Result<IDWriteTextFormat> {
    let family: Vec<u16> = style.family.encode_utf16().chain(std::iter::once(0)).collect();
    let locale: Vec<u16> = "en-us".encode_utf16().chain(std::iter::once(0)).collect();

    // The static weight passed here is the fallback DirectWrite uses when the
    // family has no `wght` axis; the axis value set below wins on a variable
    // font. Rounding to the nearest hundred matches how static families name
    // their faces.
    let static_weight = DWRITE_FONT_WEIGHT(style.weight.round().clamp(1.0, 999.0) as i32);
    let format = unsafe {
        dwrite.CreateTextFormat(
            windows::core::PCWSTR(family.as_ptr()),
            None,
            static_weight,
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            style.size_px,
            windows::core::PCWSTR(locale.as_ptr()),
        )?
    };

    unsafe {
        format.SetTextAlignment(style.align.to_dwrite())?;
        format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
        // The strip is a fixed-width single line per row; wrapping would push
        // the second half out of the visible rectangle instead of truncating.
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP)?;

        // Overflow ends in an ellipsis rather than being sliced mid-glyph. The
        // strip is only ~130px wide and an entry label can be long, so *some*
        // truncation is inevitable — this makes it legible and obviously
        // truncated instead of looking like the text simply ends there.
        if let Ok(sign) = dwrite.CreateEllipsisTrimmingSign(&format) {
            let trimming = DWRITE_TRIMMING {
                granularity: DWRITE_TRIMMING_GRANULARITY_CHARACTER,
                delimiter: 0,
                delimiterCount: 0,
            };
            let _ = format.SetTrimming(&trimming, &sign);
        }
    }

    // The continuous part. Only available from Windows 10 1809 onward, and only
    // meaningful on a variable font, so a failed cast is not an error — it just
    // means this OS or font gets the nearest static face.
    if let Ok(format3) = format.cast::<IDWriteTextFormat3>() {
        let axis = [DWRITE_FONT_AXIS_VALUE {
            axisTag: DWRITE_FONT_AXIS_TAG_WEIGHT,
            value: style.weight.clamp(1.0, 1000.0),
        }];
        unsafe {
            // Without this, DirectWrite keeps deriving axis values from the
            // static weight/style/stretch triple and overrides what we set.
            let _ = format3.SetAutomaticFontAxes(
                windows::Win32::Graphics::DirectWrite::DWRITE_AUTOMATIC_FONT_AXES_NONE,
            );
            let _ = format3.SetFontAxisValues(&axis);
        }
    }

    Ok(format)
}

fn color_of(rgb: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((rgb >> 16) & 0xFF) as f32 / 255.0,
        g: ((rgb >> 8) & 0xFF) as f32 / 255.0,
        b: (rgb & 0xFF) as f32 / 255.0,
        a: 1.0,
    }
}

/// A font family the taskbar strip can use, and whether its weight is continuous.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FontFamilyInfo {
    /// The family name to pass to DirectWrite.
    pub name: String,
    /// True when the family exposes an OpenType `wght` variation axis, i.e. the
    /// weight slider is genuinely continuous rather than snapping to installed
    /// faces.
    pub variable_weight: bool,
    /// True when the family can actually draw Chinese. A Latin-only family is
    /// not *wrong* — DirectWrite falls back per character — but the strip then
    /// mixes two typefaces mid-line, so the picker has to be able to say which
    /// families keep one look throughout.
    pub has_cjk: bool,
    /// Whether this family belongs in the short list the picker shows first.
    ///
    /// A typical Windows install has ~400 families, almost all of them symbol
    /// fonts, script faces and per-app bundles that have no business in a 12px
    /// status strip. Scrolling that list to find the handful of usable ones was
    /// the actual complaint; everything still remains reachable behind the
    /// picker's "show all" switch.
    pub recommended: bool,
}

/// Variable families that deserve a boost when several real `wght` fonts are
/// installed. Static Microsoft faces (YaHei / Segoe UI) deliberately do *not*
/// live here — they only have discrete installed weights, so the slider snaps
/// and they must not appear in a "continuous weight" picker.
const PREFERRED_VARIABLE_FAMILIES: &[&str] = &[
    "MiSans",
    "MiSans VF",
    "HarmonyOS Sans SC",
    "Source Han Sans SC",
    "Noto Sans SC",
    "Alibaba PuHuiTi 3.0",
    "OPPOSans",
    "Inter",
    "Segoe UI Variable",
    "Bahnschrift",
];

fn is_preferred_variable_family(name: &str) -> bool {
    PREFERRED_VARIABLE_FAMILIES
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

/// Ordering rank for the picker. Lower sorts first.
///
/// Only families with a genuine continuous `wght` axis are recommended. Among
/// those, CJK-capable and known UI faces sort first so the short list is
/// useful on a Chinese taskbar strip.
fn family_rank(info: &FontFamilyInfo) -> u8 {
    if !info.variable_weight {
        // Static faces (Microsoft YaHei, regular Segoe UI, …) stay in the full
        // enumeration for diagnostics but never enter the recommended set.
        return 4;
    }
    match (info.has_cjk, is_preferred_variable_family(&info.name)) {
        (true, true) => 0,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 3,
    }
}

/// Enumerating the system font collection means opening a font face for every
/// font in every family — hundreds of COM round-trips. It is far too expensive
/// to redo per call, and the installed font set does not change while the app
/// runs, so it is computed once.
static FAMILY_CACHE: std::sync::OnceLock<Vec<FontFamilyInfo>> = std::sync::OnceLock::new();

/// Enumerate installed font families usable by the strip.
///
/// Returns real installed names only — the settings UI must not offer a family
/// this machine cannot render. Sorted, with the variable-weight families first
/// so the honest "continuous" choices are the easy ones to find.
pub fn font_families() -> &'static [FontFamilyInfo] {
    FAMILY_CACHE.get_or_init(|| match unsafe { enumerate_families() } {
        Ok(mut families) => {
            families.sort_by(|a, b| {
                family_rank(a)
                    .cmp(&family_rank(b))
                    .then_with(|| a.name.cmp(&b.name))
            });
            families
        }
        Err(err) => {
            tracing::warn!("taskbar text: font enumeration failed: {err}");
            Vec::new()
        }
    })
}

// NOTE: there is deliberately no `best_default_family()` here. The list this
// function returns is already sorted best-first, and the settings page reads
// its first recommended entry — one ranking, in one place. A second copy of the
// preference order in Rust could drift from the one the user sees.

unsafe fn enumerate_families() -> windows::core::Result<Vec<FontFamilyInfo>> {
    let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
    let mut collection: Option<IDWriteFontCollection> = None;
    unsafe { dwrite.GetSystemFontCollection(&mut collection, false)? };
    let Some(collection) = collection else {
        return Ok(Vec::new());
    };

    let count = unsafe { collection.GetFontFamilyCount() };
    let mut out = Vec::with_capacity(count as usize);
    for index in 0..count {
        let Ok(family) = (unsafe { collection.GetFontFamily(index) }) else {
            continue;
        };
        let Ok(names) = (unsafe { family.GetFamilyNames() }) else {
            continue;
        };
        let Ok(length) = (unsafe { names.GetStringLength(0) }) else {
            continue;
        };
        let mut buffer = vec![0u16; length as usize + 1];
        if unsafe { names.GetString(0, &mut buffer) }.is_err() {
            continue;
        }
        let name = String::from_utf16_lossy(&buffer[..length as usize]);
        if name.is_empty() {
            continue;
        }
        let traits = unsafe { family_traits(&family) };
        let mut info = FontFamilyInfo {
            name,
            variable_weight: traits.variable_weight,
            has_cjk: traits.has_cjk,
            recommended: false,
        };
        // Recommended ≡ genuine continuous weight. The picker only offers
        // those by default; static multi-face families look continuous on a
        // stepped slider but are not (Microsoft YaHei, Segoe UI, …).
        info.recommended = info.variable_weight;
        out.push(info);
    }
    Ok(out)
}

#[derive(Default)]
struct FamilyTraits {
    variable_weight: bool,
    has_cjk: bool,
}

/// A representative CJK ideograph ("中", U+4E2D).
///
/// Coverage is asked of the font rather than guessed from the family name: a
/// name-based rule would be wrong for every third-party face and would have to
/// be maintained forever.
const CJK_PROBE_CHAR: u32 = 0x4E2D;

/// Inspect one family once, for both properties the picker needs.
///
/// Merged into a single pass on purpose. Enumeration already opens every face
/// in every family across ~400 families, and that cost is what made an earlier
/// per-family lookup hang the settings page; walking the collection twice would
/// double it for no reason.
unsafe fn family_traits(
    family: &windows::Win32::Graphics::DirectWrite::IDWriteFontFamily,
) -> FamilyTraits {
    let mut traits = FamilyTraits::default();
    let Ok(family2) = family.cast::<IDWriteFontFamily2>() else {
        return traits;
    };
    let count = unsafe { family2.GetFontCount() };
    for index in 0..count {
        let Ok(font) = (unsafe { family2.GetFont(index) }) else {
            continue;
        };
        if !traits.has_cjk && unsafe { font.HasCharacter(CJK_PROBE_CHAR) }.as_bool() {
            traits.has_cjk = true;
        }
        if !traits.variable_weight && unsafe { font_has_weight_axis(&font) } {
            traits.variable_weight = true;
        }
        if traits.has_cjk && traits.variable_weight {
            break;
        }
    }
    traits
}

/// Whether one face has a *variable* `wght` axis (min < max), not merely a
/// derived static weight.
///
/// DirectWrite exposes `wght`/`wdth`/`ital`/`slnt` axis *values* for every
/// font — static families included — by synthesising them from the face's
/// weight/stretch/style. Checking `GetFontAxisValues` for a WEIGHT tag is
/// therefore a false positive for Microsoft YaHei and friends. The resource
/// side reports real axis ranges and the VARIABLE attribute; that is what
/// separates MiSans-style continuous faces from stepped multi-face families.
unsafe fn font_has_weight_axis(
    font: &windows::Win32::Graphics::DirectWrite::IDWriteFont3,
) -> bool {
    let Ok(face) = (unsafe { font.CreateFontFace() }) else {
        return false;
    };
    let Ok(face5) = face.cast::<IDWriteFontFace5>() else {
        return false;
    };
    if !unsafe { face5.HasVariations() }.as_bool() {
        return false;
    }
    let Ok(resource) = (unsafe { face5.GetFontResource() }) else {
        return false;
    };
    if !unsafe { resource.HasVariations() }.as_bool() {
        return false;
    }
    let axis_count = unsafe { resource.GetFontAxisCount() };
    if axis_count == 0 {
        return false;
    }
    let mut ranges = vec![DWRITE_FONT_AXIS_RANGE::default(); axis_count as usize];
    if unsafe { resource.GetFontAxisRanges(&mut ranges) }.is_err() {
        return false;
    }
    for (index, range) in ranges.iter().enumerate() {
        if range.axisTag != DWRITE_FONT_AXIS_TAG_WEIGHT {
            continue;
        }
        let attrs = unsafe { resource.GetFontAxisAttributes(index as u32) };
        let is_variable = (attrs.0 & DWRITE_FONT_AXIS_ATTRIBUTES_VARIABLE.0) != 0;
        // A real axis spans a range; equal min/max is a static synthesised tag.
        if is_variable && range.maxValue > range.minValue {
            return true;
        }
    }
    false
}

/// Whether a specific family name supports a continuous weight axis.
///
#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, HBITMAP, SelectObject,
    };

    const W: i32 = 320;
    const H: i32 = 64;

    /// Render one line and return how much ink landed on the bitmap.
    ///
    /// "Ink" is the count of pixels that differ from the background, which is a
    /// direct proxy for stroke thickness: a heavier weight covers strictly more
    /// pixels for the same glyphs at the same size. That makes weight continuity
    /// measurable in an automated test rather than only visible in a screenshot.
    fn ink_for(family: &str, weight: f32) -> Option<u64> {
        let (b, g, r) = render_channels(
            &[TextLine {
                text: "Codex 59% 周额度",
                rect: RECT { left: 0, top: 0, right: W, bottom: H },
                mark: None,
            }],
            &TextStyle {
                family,
                weight,
                size_px: 28.0,
                align: TextAlign::Left,
                color_rgb: 0x00FF_FFFF,
            },
        )?;
        Some(b + g + r)
    }

    /// Render onto an offscreen bitmap and sum each colour channel separately.
    ///
    /// Channels rather than one total because the provider mark is drawn in its
    /// own brand colour: a red mark beside blue text is only distinguishable
    /// from "the mark never drew" if red can be counted on its own.
    fn render_channels(lines: &[TextLine<'_>], style: &TextStyle<'_>) -> Option<(u64, u64, u64)> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: W,
                    // Top-down so the pointer walk below is in reading order.
                    biHeight: -H,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap: HBITMAP =
                match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                    Ok(bitmap) if !bits.is_null() => bitmap,
                    _ => {
                        let _ = DeleteDC(dc);
                        return None;
                    }
                };
            let previous = SelectObject(dc, bitmap.into());

            // Black background, white text: maximum contrast, so the ink count
            // is insensitive to antialiasing thresholds.
            std::ptr::write_bytes(bits.cast::<u8>(), 0, (W * H * 4) as usize);

            let bounds = RECT {
                left: 0,
                top: 0,
                right: W,
                bottom: H,
            };
            // Each sample builds a throwaway DC and destroys it again. The
            // cached render target still holds the previous, now-invalid DC, and
            // re-binding it to a fresh one was observed to block forever. The
            // live widget binds to one long-lived window DC, so this is a
            // harness concern only — but it must be reset per sample or the
            // measurement never completes.
            reset_renderer();
            let drawn = draw_lines(dc.0 as isize, bounds, lines, style);

            // 32bpp BI_RGB lays out as B, G, R, unused.
            let ink = if drawn {
                let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), (W * H * 4) as usize);
                pixels.chunks_exact(4).fold((0u64, 0u64, 0u64), |acc, px| {
                    (
                        acc.0 + u64::from(px[0]),
                        acc.1 + u64::from(px[1]),
                        acc.2 + u64::from(px[2]),
                    )
                })
            } else {
                (0, 0, 0)
            };

            SelectObject(dc, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            drawn.then_some(ink)
        }
    }

    /// The whole justification for adding DirectWrite: a variable font must
    /// actually render heavier for a heavier `wght` axis value. GDI could not do
    /// this — it snapped everything to 400 or 700 — so if this ever fails the
    /// continuous weight slider has become a lie again and must be relabelled.
    ///
    /// Both the named stops and the values between them are checked in ONE test
    /// on purpose. libtest gives each test its own thread, `RENDERER` is
    /// thread-local, and creating a second single-threaded Direct2D factory on a
    /// thread that has no COM apartment blocks indefinitely. The live widget only
    /// ever draws from the app's UI thread, so one renderer per process is the
    /// real usage pattern; splitting this across tests would only be testing the
    /// harness.
    #[test]
    fn variable_font_weight_axis_is_continuous() {
        let Some(family) = font_families()
            .iter()
            .find(|f| f.variable_weight)
            .map(|f| f.name.clone())
        else {
            eprintln!("no variable-weight font family installed; skipping");
            return;
        };
        eprintln!("chosen variable family: {family:?}");

        let mut samples: Vec<(f32, u64)> = Vec::new();
        // 430 and 560 sit between the named stops: four static faces would
        // render those identically, a real axis will not.
        for weight in [200.0_f32, 400.0, 430.0, 560.0, 700.0, 900.0] {
            match ink_for(&family, weight) {
                Some(ink) => {
                    eprintln!("  {family} @ {weight} -> ink {ink}");
                    samples.push((weight, ink));
                }
                None => {
                    eprintln!("DirectWrite unavailable in this environment; skipping");
                    return;
                }
            }
        }

        for pair in samples.windows(2) {
            let (light_w, light_ink) = pair[0];
            let (heavy_w, heavy_ink) = pair[1];
            assert!(
                heavy_ink > light_ink,
                "{family}: weight {heavy_w} rendered {heavy_ink} ink, not more than                  weight {light_w}'s {light_ink} — the wght axis is not being applied"
            );
        }

        mark_is_drawn_in_its_own_colour(&family);
        run_strip_cell_render_checks(&family);
    }

    /// Second phase of the test above, deliberately called from it rather than
    /// standing as its own `#[test]`.
    ///
    /// `RENDERER` is thread-local and libtest gives every test its own thread,
    /// so a second drawing test would build a second single-threaded Direct2D
    /// factory on a thread with no COM apartment and hang — the same constraint
    /// documented on the caller.
    ///
    /// What it proves: the provider mark takes the provider's BRAND colour while
    /// the rest of the line takes the strip's text colour. That is the whole
    /// reason the strip can drop the provider's name — at 14px it is colour, not
    /// outline, that separates `◈` Claude from `◆` Codex. Pure-red mark beside
    /// pure-blue text, so red on the bitmap can only have come from the mark.
    fn mark_is_drawn_in_its_own_colour(family: &str) {
        let rect = RECT { left: 0, top: 0, right: W, bottom: H };
        let style = TextStyle {
            family,
            weight: 400.0,
            size_px: 28.0,
            align: TextAlign::Left,
            color_rgb: 0x0000_00FF,
        };

        let Some((_, _, red_without)) =
            render_channels(&[TextLine { text: "5h 12%", rect, mark: None }], &style)
        else {
            return;
        };
        let Some((_, _, red_with)) = render_channels(
            &[TextLine {
                text: "5h 12%",
                rect,
                mark: Some(LineMark { glyph: '◆', color_rgb: 0x00FF_0000 }),
            }],
            &style,
        ) else {
            return;
        };

        assert_eq!(
            red_without, 0,
            "blue text put {red_without} red on the bitmap; the channel probe is invalid"
        );
        assert!(
            red_with > 0,
            "a red mark beside blue text rendered no red at all — it was either skipped \
             or drawn with the text brush"
        );
    }

    #[test]
    fn enumerated_families_are_non_empty_and_named() {
        let families = font_families();
        if families.is_empty() {
            eprintln!("DirectWrite font collection unavailable; skipping");
            return;
        }
        let recommended: Vec<_> = families.iter().filter(|f| f.recommended).collect();
        eprintln!(
            "{} families, {} with a weight axis, {} recommended",
            families.len(),
            families.iter().filter(|f| f.variable_weight).count(),
            recommended.len()
        );
        for family in &recommended {
            eprintln!(
                "  {} (variable={}, cjk={})",
                family.name, family.variable_weight, family.has_cjk
            );
        }
        assert!(families.iter().all(|f| !f.name.trim().is_empty()));
    }

    /// The picker's whole purpose is to be short. If the curated list ever grows
    /// to the size of the raw collection the filter has silently stopped
    /// working and the original complaint is back.
    #[test]
    fn the_recommended_list_stays_short() {
        let families = font_families();
        if families.is_empty() {
            return;
        }
        let recommended = families.iter().filter(|f| f.recommended).count();
        assert!(
            recommended <= 40,
            "{recommended} of {} families were marked recommended; the picker is \
             supposed to show a short list",
            families.len()
        );
        assert!(
            families[..recommended].iter().all(|f| f.recommended),
            "recommended families must sort ahead of the rest"
        );
    }

    /// Ranking drives both the sort order and the default pick, so it is checked
    /// directly rather than only through whatever happens to be installed.
    #[test]
    fn continuous_weight_outranks_statics_and_prefers_cjk() {
        let variable_cjk = FontFamilyInfo {
            name: "MiSans VF".into(),
            variable_weight: true,
            has_cjk: true,
            recommended: true,
        };
        let variable_latin = FontFamilyInfo {
            name: "Bahnschrift".into(),
            variable_weight: true,
            has_cjk: false,
            recommended: true,
        };
        let static_microsoft = FontFamilyInfo {
            name: "Microsoft YaHei UI".into(),
            variable_weight: false,
            has_cjk: true,
            recommended: false,
        };
        let other = FontFamilyInfo {
            name: "Wingdings".into(),
            variable_weight: false,
            has_cjk: false,
            recommended: false,
        };

        assert!(family_rank(&variable_cjk) < family_rank(&variable_latin));
        assert!(family_rank(&variable_latin) < family_rank(&static_microsoft));
        assert_eq!(
            family_rank(&static_microsoft),
            4,
            "static multi-face families must not enter the continuous list"
        );
        assert_eq!(family_rank(&other), 4);
    }

    /// Microsoft YaHei / Segoe UI are the classic false-positive case: DirectWrite
    /// synthesises a `wght` axis value for every static face. They must never
    /// report `variable_weight = true` on a real Windows install that has them.
    #[test]
    fn microsoft_static_ui_faces_are_not_marked_variable() {
        let families = font_families();
        if families.is_empty() {
            return;
        }
        for name in ["Microsoft YaHei UI", "Microsoft YaHei", "Segoe UI"] {
            if let Some(info) = families
                .iter()
                .find(|f| f.name.eq_ignore_ascii_case(name))
            {
                assert!(
                    !info.variable_weight,
                    "{name} was marked variable_weight — the axis-range check \
                     is treating a static multi-face family as continuous"
                );
                assert!(
                    !info.recommended,
                    "{name} must not appear in the recommended continuous list"
                );
            }
        }
    }

    /// Render a set of strip cells onto an offscreen DIB and return total ink.
    fn cells_ink(cells: &[StripCell<'_>], family: &str, color_rgb: u32) -> Option<u64> {
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() { return None; }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: W,
                    biHeight: -H,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap: HBITMAP = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(bitmap) if !bits.is_null() => bitmap,
                _ => { let _ = DeleteDC(dc); return None; },
            };
            let previous = SelectObject(dc, bitmap.into());
            std::ptr::write_bytes(bits.cast::<u8>(), 0, (W * H * 4) as usize);
            let bounds = RECT { left: 0, top: 0, right: W, bottom: H };
            reset_renderer();
            let style = TextStyle {
                family,
                weight: 400.0,
                size_px: 24.0,
                align: TextAlign::Left,
                color_rgb,
            };
            let drawn = draw_strip_cells(
                dc.0 as isize,
                bounds,
                cells,
                &style,
                0x00_1010u32,
                20.0,
                5.0,
                2.0,
                crate::taskbar_icons::IconStyle::Pure,
            );
            let ink = if drawn {
                let pixels = std::slice::from_raw_parts(bits.cast::<u8>(), (W * H * 4) as usize);
                pixels.chunks_exact(4).fold(0u64, |acc, px| acc + u64::from(px[0]) + u64::from(px[1]) + u64::from(px[2]))
            } else { 0 };
            SelectObject(dc, previous);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            drawn.then_some(ink)
        }
    }

    /// Strip-cell render checks, chained off the variable-weight test so they
    /// reuse its single Direct2D factory thread (see the module note on why a
    /// second single-threaded factory on another test thread blocks).
    fn run_strip_cell_render_checks(family: &str) {
        // The tag+value cluster draws real ink (not nothing).
        let rect = RECT { left: 0, top: 0, right: W, bottom: H };
        let mut c = StripCell { rect, tag: "周", value: "", icon_provider: None, glyph: None };
        c.value = "54%";
        if let Some(ink) = cells_ink(std::slice::from_ref(&c), family, 0x00FF_FFFF) {
            assert!(ink > 0, "tag+value rendered no ink — the cluster is empty");
        } else {
            eprintln!("DirectWrite unavailable; skipping");
            return;
        }

        // Regression: the official SVG icon path must draw through the same
        // full strip pipeline (this is what the widget's WM_PAINT calls with
        // icon_provider set). A by-value ABI mismatch in the geometry sink once
        // fast-failed the real paint path while the smoke test passed; an icon
        // cell must now produce ink here too, on the same DC target.
        let icon_cell = StripCell { rect, tag: "周", value: "54%", icon_provider: Some("codex"), glyph: None };
        let icon_ink = cells_ink(std::slice::from_ref(&icon_cell), family, 0x00FF_FFFF);
        let icon_ink = match icon_ink {
            Some(ink) => ink,
            None => { eprintln!("DirectWrite unavailable; skipping icon regression"); return; },
        };
        assert!(
            icon_ink > 0,
            "SVG icon cell rendered no ink — the official-mark path is dead"
        );

        // The value run is drawn heavier (wght 600) than the tag's base weight.
        let half = RECT { left: 0, top: 0, right: W / 2, bottom: H };
        let tag_cell = StripCell { rect: half, tag: "88", value: "", icon_provider: None, glyph: None };
        let val_cell = StripCell { rect: half, tag: "", value: "88", icon_provider: None, glyph: None };
        let (Some(tag_ink), Some(val_ink)) = (
            cells_ink(std::slice::from_ref(&tag_cell), family, 0x00FF_FFFF),
            cells_ink(std::slice::from_ref(&val_cell), family, 0x00FF_FFFF),
        ) else {
            eprintln!("DirectWrite unavailable; skipping");
            return;
        };
        assert!(tag_ink > 0, "base-weight tag rendered nothing");
        assert!(
            val_ink > tag_ink,
            "value (wght 600) rendered {val_ink} ink, tag (base) {tag_ink} — expected the value to be heavier"
        );
    }
}

/// Silence the unused-import warning for `HWND` on builds that do not need it.
#[allow(dead_code)]
fn _hwnd_type_is_used(_: HWND) {}
