use serde::{Deserialize, Serialize};

/// The surfaces the desktop shell can present. PopOut (the internal dashboard
/// window) was removed in V5-07; the shared `main` window only ever holds
/// Hidden or Settings now, and TrayPanel survives as a data key for the
/// dedicated flyout window's size/properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SurfaceMode {
    #[default]
    Hidden,
    TrayPanel,
    Settings,
}

impl SurfaceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::TrayPanel => "trayPanel",
            Self::Settings => "settings",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "hidden" => Some(Self::Hidden),
            "trayPanel" => Some(Self::TrayPanel),
            "settings" => Some(Self::Settings),
            _ => None,
        }
    }

    /// Window properties that the shell must apply when entering this mode.
    pub fn window_properties(self) -> WindowProperties {
        match self {
            Self::Hidden => WindowProperties {
                visible: false,
                decorations: false,
                resizable: false,
                width: 0.0,
                height: 0.0,
                min_width: None,
                min_height: None,
                max_width: None,
                max_height: None,
                always_on_top: false,
                blur_dismiss: false,
                skip_taskbar: true,
            },
            // TrayPanel is the anchored "Open Tray Panel" surface: it keeps
            // the design size on first open, but remains natively resizable so
            // the user can make the scroll area taller/wider. It auto-hides on
            // click-outside (blur) and never shows in the taskbar.
            Self::TrayPanel => WindowProperties {
                visible: true,
                decorations: false,
                resizable: true,
                // Card is the HWND minus the chrome gutter (flyout_window's
                // CHROME_GUTTER_DIP). User preference: the panel opens at its
                // minimum width — 320×776 is the default/min CARD size.
                width: 320.0,
                height: 776.0,
                min_width: Some(320.0),
                min_height: Some(380.0),
                max_width: Some(480.0),
                max_height: None,
                always_on_top: true,
                blur_dismiss: true,
                skip_taskbar: true,
            },
            Self::Settings => WindowProperties {
                visible: true,
                decorations: true,
                resizable: true,
                width: 1120.0,
                height: 780.0,
                min_width: None,
                min_height: None,
                max_width: None,
                max_height: None,
                always_on_top: false,
                blur_dismiss: false,
                skip_taskbar: false,
            },
        }
    }
}

/// Describes what the window should look like in a given surface mode.
#[derive(Debug, Clone)]
pub struct WindowProperties {
    pub visible: bool,
    pub decorations: bool,
    pub resizable: bool,
    pub width: f64,
    pub height: f64,
    pub min_width: Option<f64>,
    pub min_height: Option<f64>,
    pub max_width: Option<f64>,
    pub max_height: Option<f64>,
    pub always_on_top: bool,
    /// Whether the window should auto-hide when it loses focus.
    #[allow(dead_code)]
    pub blur_dismiss: bool,
    /// Whether the window should be hidden from the Windows taskbar. Widget
    /// surfaces (TrayPanel) stay hidden; the Settings window shows there.
    pub skip_taskbar: bool,
}

/// Returned by the state machine when a transition succeeds.
#[derive(Debug)]
pub struct SurfaceTransition {
    pub from: SurfaceMode,
    pub to: SurfaceMode,
    pub properties: WindowProperties,
}

/// Tracks the current surface mode and validates transitions.
pub struct SurfaceStateMachine {
    current: SurfaceMode,
}

impl Default for SurfaceStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl SurfaceStateMachine {
    pub fn new() -> Self {
        Self {
            current: SurfaceMode::Hidden,
        }
    }

    pub fn current(&self) -> SurfaceMode {
        self.current
    }

    /// Attempt to transition to `target`. Returns `None` if already in that mode.
    pub fn transition(&mut self, target: SurfaceMode) -> Option<SurfaceTransition> {
        if self.current == target {
            return None;
        }
        let from = self.current;
        self.current = target;
        Some(SurfaceTransition {
            from,
            to: target,
            properties: target.window_properties(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use crate::surface_target::SurfaceTarget;

    #[test]
    fn starts_hidden() {
        let sm = SurfaceStateMachine::new();
        assert_eq!(sm.current(), SurfaceMode::Hidden);
    }

    #[test]
    fn app_state_starts_hidden_with_summary_target() {
        let state = AppState::new();
        assert_eq!(state.surface_machine.current(), SurfaceMode::Hidden);
        assert_eq!(state.current_target, SurfaceTarget::Summary);
    }

    #[test]
    fn noop_for_same_mode() {
        let mut sm = SurfaceStateMachine::new();
        assert!(sm.transition(SurfaceMode::Hidden).is_none());
    }

    #[test]
    fn hidden_to_tray_panel() {
        let mut sm = SurfaceStateMachine::new();
        let t = sm.transition(SurfaceMode::TrayPanel).unwrap();
        assert_eq!(t.from, SurfaceMode::Hidden);
        assert_eq!(t.to, SurfaceMode::TrayPanel);
        assert!(t.properties.visible);
        assert!(!t.properties.decorations);
        assert!(t.properties.always_on_top);
        assert!(t.properties.blur_dismiss);
        assert_eq!(sm.current(), SurfaceMode::TrayPanel);
    }

    #[test]
    fn settings_to_hidden() {
        let mut sm = SurfaceStateMachine::new();
        sm.transition(SurfaceMode::Settings);
        let t = sm.transition(SurfaceMode::Hidden).unwrap();
        assert!(!t.properties.visible);
    }

    #[test]
    fn round_trip_all_modes() {
        let mut sm = SurfaceStateMachine::new();
        for mode in [
            SurfaceMode::TrayPanel,
            SurfaceMode::Settings,
            SurfaceMode::Hidden,
        ] {
            let t = sm.transition(mode).unwrap();
            assert_eq!(t.to, mode);
        }
    }

    #[test]
    fn parse_round_trip() {
        for mode in [
            SurfaceMode::Hidden,
            SurfaceMode::TrayPanel,
            SurfaceMode::Settings,
        ] {
            assert_eq!(SurfaceMode::parse(mode.as_str()), Some(mode));
        }
    }

    #[test]
    fn parse_unknown_returns_none() {
        assert_eq!(SurfaceMode::parse("bogus"), None);
    }

    #[test]
    fn tray_panel_properties() {
        let props = SurfaceMode::TrayPanel.window_properties();
        assert_eq!(props.width, 320.0);
        assert_eq!(props.height, 776.0);
    }

    #[test]
    fn tray_panel_has_resize_bounds() {
        let props = SurfaceMode::TrayPanel.window_properties();
        assert_eq!(props.min_width, Some(320.0));
        assert_eq!(props.min_height, Some(380.0));
        assert_eq!(props.max_width, Some(480.0));
        assert_eq!(props.max_height, None);
    }

    #[test]
    fn tray_panel_is_resizable_blur_dismiss_flyout() {
        let props = SurfaceMode::TrayPanel.window_properties();
        // "Open Tray Panel" surface: default-sized, anchored, user-resizable,
        // auto-hide, no taskbar.
        assert!(props.resizable);
        assert!(props.blur_dismiss);
        assert!(props.always_on_top);
        assert!(props.skip_taskbar);
        assert!(!props.decorations);
        assert_eq!(props.min_width, Some(320.0));
        assert_eq!(props.min_height, Some(380.0));
    }

    #[test]
    fn settings_properties() {
        let props = SurfaceMode::Settings.window_properties();
        assert_eq!(props.width, 1120.0);
        assert_eq!(props.height, 780.0);
    }
}
