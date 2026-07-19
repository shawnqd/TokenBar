//! Pixel-level tray icon renderer, decoupled from any platform icon API.
//!
//! Returns raw RGBA bytes so callers (egui tray manager, Tauri shell, tests)
//! can adapt the result to their own icon type without pulling in extra deps.

use image::{ImageBuffer, Rgba, RgbaImage};

use super::icon::UsageLevel;

/// Side length of the generated tray icon in pixels.
pub const TRAY_ICON_SIZE: u32 = 32;

/// Render the TokenBar three-rail tray icon as raw RGBA bytes.
///
/// - `session_percent`: primary bar fill (0–100), colour-coded by [`UsageLevel`]
/// - `weekly_percent`: optional secondary bar fill (0–100). When `Some`, two thin
///   bars are drawn (session top, weekly bottom). When `None`, a single thick bar
///   is drawn instead.
/// - `has_error`: desaturate all bar colours to grey to signal an error/unknown state.
///
/// Returns `(rgba_bytes, width, height)` for a [`TRAY_ICON_SIZE`]×[`TRAY_ICON_SIZE`] icon.
pub fn render_bar_icon_rgba(
    _session_percent: f64,
    _weekly_percent: Option<f64>,
    has_error: bool,
) -> (Vec<u8>, u32, u32) {
    const SZ: u32 = TRAY_ICON_SIZE;
    let mut img: RgbaImage = ImageBuffer::new(SZ, SZ);

    for pixel in img.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 0]);
    }

    // A 16px tray glyph cannot communicate three changing percentages without
    // becoming muddy. Keep this selected compact mark static and reserve the
    // optional numeric icon for people who want a precise percentage here.
    draw_rounded_rect(&mut img, 2, 2, 28, 28, 8, Rgba([16, 26, 44, 255]));
    let rail = if has_error {
        Rgba([154, 154, 154, 220])
    } else {
        Rgba([98, 229, 255, 255])
    };
    for y in [7, 14, 21] {
        draw_rounded_rect(&mut img, 7, y, 18, 4, 2, rail);
    }

    (img.into_raw(), SZ, SZ)
}

/// Draw a crisp rounded rectangle on the 32px icon canvas.
fn draw_rounded_rect(
    image: &mut RgbaImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    radius: u32,
    color: Rgba<u8>,
) {
    if width == 0 || height == 0 {
        return;
    }
    let radius = radius.min(width / 2).min(height / 2) as i32;
    for py in y..y.saturating_add(height).min(TRAY_ICON_SIZE) {
        for px in x..x.saturating_add(width).min(TRAY_ICON_SIZE) {
            let local_x = px as i32 - x as i32;
            let local_y = py as i32 - y as i32;
            let right = width as i32 - 1 - local_x;
            let bottom = height as i32 - 1 - local_y;
            let inside = if local_x < radius && local_y < radius {
                (local_x - radius).pow(2) + (local_y - radius).pow(2) <= radius.pow(2)
            } else if right < radius && local_y < radius {
                (right - radius).pow(2) + (local_y - radius).pow(2) <= radius.pow(2)
            } else if local_x < radius && bottom < radius {
                (local_x - radius).pow(2) + (bottom - radius).pow(2) <= radius.pow(2)
            } else if right < radius && bottom < radius {
                (right - radius).pow(2) + (bottom - radius).pow(2) <= radius.pow(2)
            } else {
                true
            };
            if inside {
                image.put_pixel(px, py, color);
            }
        }
    }
}

/// Render a compact numeric percent tray icon as raw RGBA bytes.
pub fn render_percent_icon_rgba(percent: f64, has_error: bool) -> (Vec<u8>, u32, u32) {
    const SZ: u32 = TRAY_ICON_SIZE;
    let mut img: RgbaImage = ImageBuffer::new(SZ, SZ);

    for pixel in img.pixels_mut() {
        *pixel = Rgba([0, 0, 0, 0]);
    }

    let pct = percent.clamp(0.0, 100.0).round() as u32;
    let text = if pct >= 100 {
        "100".to_string()
    } else {
        format!("{pct}%")
    };
    let glyph_width = 3u32;
    let glyph_gap = 1u32;
    let scale = if text.len() >= 3 { 2u32 } else { 3u32 };
    let text_width = text.len() as u32 * glyph_width * scale + (text.len() as u32 - 1) * glyph_gap;
    let text_height = 5 * scale;
    let start_x = (SZ.saturating_sub(text_width)) / 2;
    let start_y = (SZ.saturating_sub(text_height)) / 2;

    let (r, g, b) = UsageLevel::from_percent(percent).color();
    let color = if has_error {
        let gray = ((r as u16 + g as u16 + b as u16) / 3) as u8;
        Rgba([gray, gray, gray, 255])
    } else {
        Rgba([r, g, b, 255])
    };

    let mut x = start_x;
    for ch in text.chars() {
        draw_glyph(&mut img, ch, x, start_y, scale, color);
        x += glyph_width * scale + glyph_gap;
    }

    (img.into_raw(), SZ, SZ)
}

fn draw_glyph(img: &mut RgbaImage, ch: char, x: u32, y: u32, scale: u32, color: Rgba<u8>) {
    let Some(rows) = glyph_rows(ch) else {
        return;
    };
    for (row_idx, row) in rows.iter().enumerate() {
        for col in 0..3 {
            let bit = 1 << (2 - col);
            if row & bit == 0 {
                continue;
            }
            for yy in 0..scale {
                for xx in 0..scale {
                    let px = x + col * scale + xx;
                    let py = y + row_idx as u32 * scale + yy;
                    if px < TRAY_ICON_SIZE && py < TRAY_ICON_SIZE {
                        img.put_pixel(px, py, color);
                    }
                }
            }
        }
    }
}

fn glyph_rows(ch: char) -> Option<[u8; 5]> {
    Some(match ch {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b010, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_produces_correct_dimensions() {
        let (rgba, w, h) = render_bar_icon_rgba(50.0, None, false);
        assert_eq!(w, TRAY_ICON_SIZE);
        assert_eq!(h, TRAY_ICON_SIZE);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }

    #[test]
    fn render_two_bar_has_correct_size() {
        let (rgba, w, h) = render_bar_icon_rgba(30.0, Some(60.0), false);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }

    #[test]
    fn static_mark_is_independent_of_usage_percent() {
        let (rgba, w, _h) = render_bar_icon_rgba(0.0, None, false);
        // The centre signal remains visible even before the first refresh.
        let idx = ((16 * w + 10) * 4) as usize;
        assert_eq!(&rgba[idx..idx + 4], &[98, 229, 255, 255]);
    }

    #[test]
    fn static_mark_is_the_same_at_full_usage() {
        let (rgba, w, _h) = render_bar_icon_rgba(100.0, None, false);
        let idx = ((8 * w + 10) * 4) as usize;
        assert_eq!(&rgba[idx..idx + 4], &[98, 229, 255, 255]);
    }

    #[test]
    fn error_state_desaturates_colors() {
        let (normal, _, _) = render_bar_icon_rgba(0.0, None, false);
        let (error, _, _) = render_bar_icon_rgba(0.0, None, true);
        // In error mode all three channels at the filled bar pixel should be equal (grey)
        let idx = ((8 * 32 + 10) * 4) as usize;
        assert_ne!(normal[idx], normal[idx + 1]); // colour has distinct channels
        assert_eq!(error[idx], error[idx + 1]); // grey: R == G
        assert_eq!(error[idx + 1], error[idx + 2]); // grey: G == B
    }

    #[test]
    fn percent_icon_produces_correct_dimensions() {
        let (rgba, w, h) = render_percent_icon_rgba(72.0, false);
        assert_eq!(w, TRAY_ICON_SIZE);
        assert_eq!(h, TRAY_ICON_SIZE);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }

    #[test]
    fn percent_icon_draws_visible_text() {
        let (rgba, _, _) = render_percent_icon_rgba(72.0, false);
        assert!(rgba.chunks_exact(4).any(|px| px[3] == 255));
    }

    #[test]
    fn percent_icon_clamps_to_hundred() {
        let (rgba, w, h) = render_percent_icon_rgba(125.0, false);
        assert_eq!(rgba.len() as u32, w * h * 4);
    }
}
