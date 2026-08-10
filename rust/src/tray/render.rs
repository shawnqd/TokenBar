//! Pixel-level tray icon renderer, decoupled from any platform icon API.
//!
//! Returns raw RGBA bytes so callers (egui tray manager, Tauri shell, tests)
//! can adapt the result to their own icon type without pulling in extra deps.

use image::{imageops, load_from_memory, RgbaImage};
use std::sync::OnceLock;

/// Side length of the tray icon fed to the shell (Windows ~16 logical px slot).
pub const TRAY_ICON_SIZE: u32 = 32;

/// Product icon master (`rust/icons/icon.png`), embedded so the tray mark
/// always matches the app / taskbar icon without a second hand-drawn glyph.
const PRODUCT_ICON_PNG: &[u8] = include_bytes!("../../icons/icon.png");

/// Cached 32×32 RGBA of the product icon (decoded once).
static TRAY_RGBA: OnceLock<Vec<u8>> = OnceLock::new();

/// Grey desaturation for the error state — keep the silhouette, drop chroma.
fn desaturate_premultiplied(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for chunk in out.chunks_exact_mut(4) {
        let a = chunk[3] as u32;
        if a == 0 {
            continue;
        }
        let r = chunk[0] as u32;
        let g = chunk[1] as u32;
        let b = chunk[2] as u32;
        // Rec. 601 luma, then slightly lifted so the mark stays visible on dark trays.
        let y = ((r * 77 + g * 150 + b * 29) / 256).min(255) as u8;
        let y = y.saturating_add(12).min(220);
        chunk[0] = y;
        chunk[1] = y;
        chunk[2] = y;
        // Keep original alpha.
    }
    out
}

fn product_icon_tray_rgba() -> &'static [u8] {
    TRAY_RGBA.get_or_init(|| {
        let dyn_img = load_from_memory(PRODUCT_ICON_PNG)
            .expect("embedded product icon PNG must decode");
        let rgba = dyn_img.to_rgba8();
        let resized: RgbaImage = imageops::resize(
            &rgba,
            TRAY_ICON_SIZE,
            TRAY_ICON_SIZE,
            imageops::FilterType::Lanczos3,
        );
        resized.into_raw()
    })
}

/// Render the tray icon as raw RGBA bytes.
///
/// Uses the same product mark as `rust/icons/icon.png` (the user-supplied
/// CodexBar glyph), scaled to [`TRAY_ICON_SIZE`]. Percentage arguments are
/// kept for call-site compatibility; the mark itself is static. `has_error`
/// desaturates the mark to grey.
///
/// Returns `(rgba_bytes, width, height)`.
pub fn render_bar_icon_rgba(
    _session_percent: f64,
    _weekly_percent: Option<f64>,
    has_error: bool,
) -> (Vec<u8>, u32, u32) {
    let base = product_icon_tray_rgba();
    let rgba = if has_error {
        desaturate_premultiplied(base)
    } else {
        base.to_vec()
    };
    (rgba, TRAY_ICON_SIZE, TRAY_ICON_SIZE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
        let idx = ((y * w + x) * 4) as usize;
        [rgba[idx], rgba[idx + 1], rgba[idx + 2], rgba[idx + 3]]
    }

    #[test]
    fn render_produces_correct_dimensions() {
        let (rgba, w, h) = render_bar_icon_rgba(50.0, None, false);
        assert_eq!((w, h), (TRAY_ICON_SIZE, TRAY_ICON_SIZE));
        assert_eq!(rgba.len() as u32, w * h * 4);
    }

    #[test]
    fn static_mark_independent_of_usage() {
        let (a, _, _) = render_bar_icon_rgba(0.0, None, false);
        let (b, _, _) = render_bar_icon_rgba(100.0, None, false);
        assert_eq!(a, b);
    }

    #[test]
    fn mark_is_not_empty() {
        let (rgba, w, _) = render_bar_icon_rgba(0.0, None, false);
        // Centre of the 32×32 mark should be the dark tile or a cyan rail —
        // either way, not fully transparent.
        let c = pixel(&rgba, w, 16, 16);
        assert!(c[3] > 128, "center should be painted, got {c:?}");
    }

    #[test]
    fn outer_corner_stays_transparent_or_soft() {
        // Product icon has transparent corners after bg removal; alpha may be
        // low even if anti-aliased.
        let (rgba, w, _) = render_bar_icon_rgba(0.0, None, false);
        let c = pixel(&rgba, w, 0, 0);
        assert!(c[3] < 32, "corner should be near-transparent, got {c:?}");
    }

    #[test]
    fn error_state_desaturates() {
        let (normal, w, _) = render_bar_icon_rgba(0.0, None, false);
        let (error, _, _) = render_bar_icon_rgba(0.0, None, true);
        // Find a saturated pixel in the normal mark and check error is greyer.
        let mut found = false;
        for y in 0..TRAY_ICON_SIZE {
            for x in 0..TRAY_ICON_SIZE {
                let n = pixel(&normal, w, x, y);
                if n[3] < 200 {
                    continue;
                }
                let chroma = n[0].abs_diff(n[1]).max(n[1].abs_diff(n[2]));
                if chroma < 20 {
                    continue;
                }
                let e = pixel(&error, w, x, y);
                let e_chroma = e[0].abs_diff(e[1]).max(e[1].abs_diff(e[2]));
                assert!(
                    e_chroma < chroma,
                    "error should reduce chroma at ({x},{y}): normal={n:?} error={e:?}"
                );
                found = true;
                break;
            }
            if found {
                break;
            }
        }
        assert!(found, "expected at least one chromatic pixel in the product mark");
    }
}
