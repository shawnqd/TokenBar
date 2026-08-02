//! Pixel-level tray icon renderer, decoupled from any platform icon API.
//!
//! Returns raw RGBA bytes so callers (egui tray manager, Tauri shell, tests)
//! can adapt the result to their own icon type without pulling in extra deps.

use image::{ImageBuffer, Rgba, RgbaImage};

/// Side length of the generated tray icon in pixels.
pub const TRAY_ICON_SIZE: u32 = 32;

/// Render the TokenBar three-rail tray icon as raw RGBA bytes.
///
/// The mark is deliberately static: a 16px tray glyph cannot communicate a
/// changing percentage without becoming muddy, and the percentages live on the
/// surfaces that have room for them. Only `has_error` changes what is drawn,
/// desaturating the rails to grey. The percentage arguments are kept so callers
/// need not know that, and so a future icon design can use them.
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
}
