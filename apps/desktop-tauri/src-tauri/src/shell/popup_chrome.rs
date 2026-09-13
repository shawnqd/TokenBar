//! Shared chrome recipe for every app-owned popup surface.
//!
//! TokenBar draws two popups that must read as one design language:
//!
//! * the tray right-click menu ([`crate::taskbar_menu`]) — a fully self-drawn
//!   `WS_POPUP` + `WS_EX_LAYERED` window whose shadow is composed arithmetically
//!   into the per-pixel bitmap;
//! * the tray flyout ([`super::flyout_window`]) — a borderless WebView2 window
//!   whose shadow/radius/animation are painted by the page (styles.css /
//!   tray-v5.css).
//!
//! A layered bitmap and a WebView cannot share their *painting* code, so the
//! unification point is the **recipe**: one set of DIP values that every side
//! copies instead of keeping its own magic numbers. This module is the single
//! source of truth. When a value changes here, the CSS counterparts flagged
//! with "keep in lockstep with popup_chrome.rs" change in the same commit —
//! nothing else needs hunting down.
//!
//! This is the deliberate boundary of the hybrid approach: one window recipe,
//! two rendering pipelines. Going further (drawing panel content natively)
//! was evaluated and rejected for now; see AGENT_HANDOFF.md's hybrid notes.

// ── Corner radius ───────────────────────────────────────────────────────────

/// Popup corner radius, aligned with `--window-radius` in the frontend so the
/// app-owned silhouette matches what the settings window and the design page
/// show. Scaled once per-DPI by each consumer.
pub const CORNER_RADIUS_DIP: i32 = 12;

// ── Open/close gesture ──────────────────────────────────────────────────────

/// Slide-up-and-fade arrival shared by both popups, matching Windows' own
/// flyouts. The menu drives it frame-by-frame through `UpdateLayeredWindow`'s
/// destination point and constant alpha; the flyout replays it as the CSS
/// animation in styles.css (`--tray-menu-animation-duration`,
/// `--tray-menu-animation-travel`) whose values mirror these constants.
///
/// History worth not repeating (from the menu's tuning passes): scaling the
/// bitmap resampled every glyph per frame and warped text; a circular reveal
/// kept glyphs crisp but was the wrong gesture — Windows flyouts do not
/// uncover, they arrive.
pub const ANIM_DURATION_MS: u128 = 180;
pub const ANIM_TRAVEL_DIP: i32 = 22;

// ── Contact shadow ──────────────────────────────────────────────────────────

/// Blur radius of the down-only contact shadow. Tuned by eye over three passes:
/// 11 DIP spread the falloff over ~40 px and peaked at 13 % darkening, which
/// read as no shadow at all; 7 DIP at 0.30 overshot the other way. 3 DIP keeps
/// the shadow as a tight contact edge rather than a halo.
pub const SHADOW_BLUR_DIP: i32 = 3;
/// A small downward offset only — horizontally symmetric. The old system
/// shadow read bottom-*right* because Windows draws it diagonally.
pub const SHADOW_OFFSET_Y_DIP: i32 = 3;
/// Alpha of the shadow's darkest texel, dark and light theme variants. The
/// frontend mirrors these as the rgba() alphas in tray-v5.css and
/// settings-v5.css box-shadows.
/// Room the popup window reserves around the card so the blurred shadow can
/// fade out unclipped. Mirrors the menu's own margin; the CSS counterpart
/// (.tray-panel-reveal padding and ::before inset in tray-v5.css) copies
/// this value, and the native window adds it on every side of the panel.
pub const SHADOW_GUTTER_DIP: i32 = 22;
/// Gap between the visible card's bottom edge and the top of the taskbar.
/// The tray flyout copies the right-click menu (`taskbar_menu::TASKBAR_GAP_DIP`).
pub const TASKBAR_GAP_DIP: i32 = 6;
/// Backdrop blur radius (DIP). Menu and flyout both run three box passes
/// at this radius over the captured desktop.
pub const BACKDROP_BLUR_DIP: i32 = 14;
/// How much of the card colour sits over the blurred desktop (menu + flyout).
pub fn backdrop_tint(light: bool) -> f32 {
    if light { 0.82 } else { 0.78 }
}

pub const SHADOW_STRENGTH_DARK: f32 = 0.32;
pub const SHADOW_STRENGTH_LIGHT: f32 = 0.15;

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the recipe against accidental drift: the frontend copies these
    /// values by hand (CSS cannot read Rust), so a silent change here would
    /// desynchronise the two pipelines instead of failing a build.
    #[test]
    fn recipe_matches_frontend_contract() {
        assert_eq!(CORNER_RADIUS_DIP, 12, "--window-radius counterpart");
        assert_eq!(ANIM_DURATION_MS, 180, "--tray-menu-animation-duration");
        assert_eq!(ANIM_TRAVEL_DIP, 22, "--tray-menu-animation-travel");
        assert_eq!(SHADOW_BLUR_DIP, 3, "blur half of the 0 3px 3px css shadow");
        assert_eq!(SHADOW_OFFSET_Y_DIP, 3, "offset-y half of 0 3px 3px");
        assert!((0.0..=1.0).contains(&SHADOW_STRENGTH_DARK));
        assert!((0.0..=1.0).contains(&SHADOW_STRENGTH_LIGHT));
        assert!(SHADOW_STRENGTH_DARK > SHADOW_STRENGTH_LIGHT);
        // Full falloff must fit: offset + ~2*sigma. sigma ~= 3.46 for three
        // box passes at r=3, so extent ~= 3 + 14 < 22.
        assert!(SHADOW_GUTTER_DIP >= SHADOW_OFFSET_Y_DIP + 2 * 5);
        assert_eq!(
            TASKBAR_GAP_DIP, 6,
            "menu and flyout share the card-to-taskbar gap"
        );
        assert_eq!(
            BACKDROP_BLUR_DIP, 14,
            "menu and flyout share the frost kernel"
        );
        assert_eq!(backdrop_tint(false), 0.78);
        assert_eq!(backdrop_tint(true), 0.82);
    }
}
