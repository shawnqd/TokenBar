//! Settings management for CodexBar
//!
//! Handles persistent configuration including:
//! - Enabled/disabled providers
//! - Refresh interval
//! - Manual cookies
//! - Other user preferences

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::core::ProviderId;

mod api_keys;
mod manual_cookies;
mod provider_workspace;
mod raw;
mod status;
mod types;

pub use api_keys::*;
pub use manual_cookies::*;
pub use provider_workspace::*;
use raw::RawSettings;
pub use status::*;
pub use types::*;

#[cfg(test)]
mod tests;

/// Application settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "RawSettings", default)]
pub struct Settings {
    /// Enabled provider IDs (by CLI name)
    pub enabled_providers: HashSet<String>,

    /// Refresh interval in seconds (0 = manual only)
    pub refresh_interval_secs: u64,

    /// Retry provider requests after timeouts before pausing automatic refresh.
    /// When enabled, a timed-out request is retried three times with a short
    /// backoff; only a fourth consecutive timeout pauses that provider until
    /// the next manual refresh.
    #[serde(default = "default_true")]
    pub provider_timeout_recovery_enabled: bool,

    /// Force-refresh enabled providers whenever the tray/menu surface opens.
    #[serde(default)]
    pub refresh_all_providers_on_menu_open: bool,

    /// Whether to start minimized
    pub start_minimized: bool,

    /// Whether to start at login
    pub start_at_login: bool,

    /// Whether to show notifications
    pub show_notifications: bool,

    /// Whether to play sound effects for threshold alerts
    pub sound_enabled: bool,

    /// Sound volume for alerts (0-100)
    pub sound_volume: u8,

    /// High usage threshold for warnings (percentage)
    pub high_usage_threshold: f64,

    /// Critical usage threshold for alerts (percentage)
    pub critical_usage_threshold: f64,

    /// Merge mode: show all enabled providers in a single tray icon
    pub merge_tray_icons: bool,

    /// Tray icon display mode: single icon or per-provider icons
    #[serde(default)]
    pub tray_icon_mode: TrayIconMode,

    /// Show provider icons in the merged switcher UI
    #[serde(default = "default_true")]
    pub switcher_shows_icons: bool,

    /// Prefer the provider closest to its limit in merged menu bar display
    #[serde(default)]
    pub menu_bar_shows_highest_usage: bool,

    /// Legacy global "show usage bars as used (true) or remaining (false)".
    ///
    /// Superseded by the per-component fields
    /// [`float_bar_show_as_used`](Settings::float_bar_show_as_used),
    /// [`dashboard_show_as_used`](Settings::dashboard_show_as_used) and
    /// [`taskbar_show_as_used`](Settings::taskbar_show_as_used). It is retained
    /// only so existing `settings.json` files keep loading and can seed those
    /// fields once; no display surface reads it any more.
    pub show_as_used: bool,

    /// Enable UI animations (chart entrances, transitions)
    pub enable_animations: bool,

    /// Legacy global "show reset times as relative (e.g. "2h 30m") instead of
    /// absolute ("3:00 PM")".
    ///
    /// Superseded by the per-component `*_reset_time_relative` fields and kept
    /// only as a migration source, exactly like
    /// [`show_as_used`](Settings::show_as_used).
    pub reset_time_relative: bool,

    /// Menu bar display mode: "minimal", "compact", or "detailed"
    pub menu_bar_display_mode: String,

    /// Show recent model output speed in the tray flyout.
    #[serde(default = "default_true")]
    pub output_speed_enabled: bool,
    /// 设置页可见时是否保留托盘面板(false=维持现状之外的隐藏偏好)。
    pub keep_tray_panel_on_settings: bool,

    /// Which period the panel's local-usage stats lead with: "today", "7d", or "30d".
    #[serde(default = "default_local_usage_period")]
    pub local_usage_period: String,

    /// Show all token accounts in provider menus instead of collapsing behind switchers
    #[serde(default)]
    pub show_all_token_accounts_in_menu: bool,

    /// Per-provider configuration map (cookie/usage source, region, manual
    /// headers, API tokens, etc). Replaces the legacy flat per-provider
    /// fields; legacy `settings.json` files are migrated via [`RawSettings`].
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub provider_configs: HashMap<ProviderId, ProviderConfig>,

    /// Disable credential/keychain-style reads where supported
    #[serde(default)]
    pub disable_keychain_access: bool,

    /// Hide personal info (emails, account names) for streaming/sharing
    pub hide_personal_info: bool,

    /// Update channel for receiving updates (Stable or Beta)
    pub update_channel: UpdateChannel,

    /// Per-provider metric preference for tray display
    #[serde(default)]
    pub provider_metrics: HashMap<String, MetricPreference>,

    /// Preferred display order of provider IDs (CLI names).
    ///
    /// An empty list means "fall back to the canonical `ProviderId::all()`
    /// order". Unknown or duplicated ids are filtered out on load; new
    /// providers are appended in their canonical order.
    #[serde(default)]
    pub provider_order: Vec<String>,

    /// Global keyboard shortcut to open the menu (e.g., "Ctrl+Shift+U")
    #[serde(default = "default_global_shortcut")]
    pub global_shortcut: String,

    /// Additional Codex home or sessions directories to include in local cost scans.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub codex_custom_sessions_dirs: Vec<String>,

    /// Automatically download updates in the background
    #[serde(default)]
    pub auto_download_updates: bool,

    /// Install pending updates when quitting the application
    #[serde(default)]
    pub install_updates_on_quit: bool,

    /// UI language for the application (English default for backward compatibility)
    #[serde(default)]
    pub ui_language: Language,

    /// UI theme preference (Phase 12). Defaults to Auto (prefers-color-scheme).
    #[serde(default)]
    pub theme: ThemePreference,

    /// Tray flyout display scale, in the inclusive range 100..=200.
    /// 100 % is normal size; higher values enlarge the flyout content.
    #[serde(default = "default_tray_scale_percent")]
    pub tray_scale_percent: u16,

    /// Show the always-on-top floating capacity bar.
    #[serde(default)]
    pub float_bar_enabled: bool,

    /// Opacity of the floating bar window, in the inclusive range 30..=100.
    /// Stored as `u8` so the on-disk format remains stable.
    #[serde(default = "default_float_bar_opacity")]
    pub float_bar_opacity: u8,

    /// Floating-bar visual scale, in the inclusive range 75..=200.
    #[serde(default = "default_float_bar_scale")]
    pub float_bar_scale: u8,

    /// Floating-bar orientation: "horizontal" (default) or "vertical".
    #[serde(default = "default_float_bar_orientation")]
    pub float_bar_orientation: String,

    /// Floating-bar visual style: "floating" (default) or "taskbar".
    #[serde(default = "default_float_bar_style")]
    pub float_bar_style: String,

    /// When true the floating bar is fully click-through (overlay mode).
    #[serde(default)]
    pub float_bar_click_through: bool,

    /// Provider CLI names to display in the floating bar. Empty = all enabled.
    #[serde(default)]
    pub float_bar_provider_ids: Vec<String>,

    /// Ordered floating-bar entries (provider + window). Empty = all enabled.
    #[serde(default)]
    pub float_bar_entries: Vec<TaskbarEntry>,

    /// When true, the floating bar uses a dark-on-light palette so it
    /// stays legible on light desktop backgrounds. Defaults to false
    /// (light-on-dark, the original look).
    #[serde(default)]
    pub float_bar_dark_text: bool,

    /// When true, show the next reset inline in each pill.
    #[serde(default)]
    pub float_bar_show_reset_inline: bool,

    /// Which quota windows' resets the floating bar prints, in order.
    ///
    /// The bar used to hardcode the provider's `primary` window, which means
    /// different providers were silently showing different cycles — Codex's
    /// weekly next to Claude's 5-hour session, with nothing on screen saying
    /// so. Naming the windows explicitly makes the bar comparable across
    /// providers, and `primary` stays available (and is the default) so an
    /// upgrade changes nothing until the user asks for it.
    #[serde(default = "default_float_bar_reset_windows")]
    pub float_bar_reset_windows: Vec<String>,

    /// When true, show local cost summaries in the floating bar.
    #[serde(default)]
    pub float_bar_show_cost: bool,

    /// When true, embed a usage readout strip directly in the Windows
    /// taskbar (next to the running-app icons), in addition to the tray
    /// icon. Windows only; ignored on other platforms.
    #[serde(default)]
    pub taskbar_widget_enabled: bool,

    /// Taskbar overlay placement: "notification" (before the notification
    /// area) or "left" (at the left edge of the taskbar).
    #[serde(default = "default_taskbar_widget_position")]
    pub taskbar_widget_position: String,

    /// Windows taskbar text weight as an OpenType `wght` axis value, 100..=1000.
    ///
    /// Genuinely continuous on a variable font: the native strip renders through
    /// DirectWrite's `SetFontAxisValues`, which was measured on this machine to
    /// produce distinct stroke weights for values between the named stops. On a
    /// static family DirectWrite still picks the nearest installed face, which is
    /// why [`taskbar_widget_font_family`] is a user choice and the UI reports
    /// which families support the axis.
    #[serde(default = "default_taskbar_widget_font_weight")]
    pub taskbar_widget_font_weight: u16,

    /// Stroke weight for the self-drawn right-click menu's labels.
    ///
    /// Separate from [`taskbar_widget_font_weight`] on purpose: that one is
    /// chosen for a two-line readout squeezed into the taskbar, and the menu is
    /// a different surface with different legibility constraints. Shares the
    /// same clamp, so the two stay comparable.
    #[serde(default = "default_menu_font_weight")]
    pub menu_font_weight: u16,

    /// DirectWrite font family and em size for the right-click menu. Mirrors
    /// the taskbar strip's trio (family / weight / size) because the menu is
    /// drawn by the same renderer — the settings UI reuses the strip's controls
    /// rather than inventing menu-specific ones.
    #[serde(default = "default_menu_font_family")]
    pub menu_font_family: String,
    #[serde(default = "default_menu_font_size")]
    pub menu_font_size: u8,

    /// DirectWrite font family for the taskbar strip.
    ///
    /// Must be a real installed family; the settings UI populates the choices
    /// from `IDWriteFontCollection` rather than a hardcoded list, so it can never
    /// offer something this machine cannot render.
    #[serde(default = "default_taskbar_widget_font_family")]
    pub taskbar_widget_font_family: String,

    /// Legacy single-choice taskbar content: usage, speed, or usage_speed.
    ///
    /// **Migration source only.** Superseded by [`taskbar_widget_entries`],
    /// which can express the same three shapes and much more. Still
    /// deserialized so an older `settings.json` seeds the ordered list once.
    #[serde(default = "default_taskbar_widget_content")]
    pub taskbar_widget_content: String,

    /// Ordered taskbar strip entries: which provider's which quota window, in
    /// the order the user wants them stacked.
    ///
    /// An ordered list rather than a widening enum because the user composes
    /// this freely (`Codex · 5h`, `Claude · weekly`, ...) and order is itself a
    /// setting — the strip has room for only the first few, so position decides
    /// what survives truncation.
    #[serde(default = "default_taskbar_widget_entries")]
    pub taskbar_widget_entries: Vec<TaskbarEntry>,

    /// Taskbar status text size in logical pixels (10..=16).
    #[serde(default = "default_taskbar_widget_font_size")]
    pub taskbar_widget_font_size: u8,

    /// Taskbar status strip width in logical pixels (96..=240).
    #[serde(default = "default_taskbar_widget_width")]
    pub taskbar_widget_width: u16,

    /// Taskbar status text alignment: left, center, or right.
    #[serde(default = "default_taskbar_widget_text_align")]
    pub taskbar_widget_text_align: String,

    /// Taskbar strip icon size in logical pixels (10..=18).
    #[serde(default = "default_taskbar_widget_icon_size")]
    pub taskbar_widget_icon_size: u8,

    /// Taskbar strip icon render style: "pure", "badge", or "solid".
    #[serde(default = "default_taskbar_widget_icon_style")]
    pub taskbar_widget_icon_style: String,

    /// Taskbar strip gap between icon and tag in logical pixels (0..=12).
    #[serde(default = "default_taskbar_widget_icon_gap_px")]
    pub taskbar_widget_icon_gap_px: u8,

    /// Taskbar strip gap between tag and value in logical pixels (0..=8).
    #[serde(default = "default_taskbar_widget_value_gap_px")]
    pub taskbar_widget_value_gap_px: u8,

    // ── Per-component quota presentation ─────────────────────────────
    //
    // The floating bar, the dashboard surfaces (tray flyout + PopOut panel)
    // and the Windows taskbar strip each own their own used-vs-remaining and
    // relative-vs-absolute reset choice. They are seeded once from the legacy
    // global fields when an older `settings.json` is loaded and are fully
    // independent afterwards, so changing one surface never silently changes
    // another.
    /// Floating bar: show quota as used (`true`) or remaining (`false`).
    #[serde(default = "default_true")]
    pub float_bar_show_as_used: bool,

    /// Floating bar: show reset times as relative (`true`) or absolute (`false`).
    #[serde(default = "default_true")]
    pub float_bar_reset_time_relative: bool,

    /// Dashboard surfaces: show quota as used (`true`) or remaining (`false`).
    #[serde(default = "default_true")]
    pub dashboard_show_as_used: bool,

    /// Dashboard surfaces: relative (`true`) or absolute (`false`) reset times.
    #[serde(default = "default_true")]
    pub dashboard_reset_time_relative: bool,

    /// Retained for settings-file compatibility. Dashboard surfaces now follow
    /// `enabled_providers` directly; this legacy dashboard-only filter is not
    /// consulted by the renderer.
    #[serde(default)]
    pub dashboard_provider_ids: Vec<String>,

    /// Retained for settings-file compatibility. Dashboard cards now render the
    /// quota windows returned by each provider, so this legacy filter is not
    /// consulted by the renderer.
    #[serde(default)]
    pub dashboard_quota_windows: Vec<String>,

    /// Windows taskbar strip and notification-area icon: show quota as used
    /// (`true`) or remaining (`false`). Consumed by
    /// `tray_bridge::selected_tray_percents`, which feeds both.
    ///
    #[serde(default = "default_true")]
    pub taskbar_show_as_used: bool,

    /// Same family as [`taskbar_show_as_used`]: how the strip's and the tray
    /// icon's surfaces phrase a reset time — `true` for a countdown ("14 小时后"),
    /// `false` for the moment itself ("08-07 14:30").
    ///
    /// This field was deliberately absent for most of TASK-021, because nothing
    /// in that family rendered reset text and an inert setting is worse than no
    /// setting. That changed when the self-drawn context menu grew a status row
    /// (`tray_bridge::provider_status_label`), which does.
    #[serde(default = "default_true")]
    pub taskbar_reset_time_relative: bool,

    /// Hover/tooltip entries for the mini status bar and tray icon, independent
    /// of the painted strip entries (TASK-021 item 9). Empty → reuse strip
    /// entries / primary tray lines.
    #[serde(default)]
    pub taskbar_tooltip_entries: Vec<TaskbarEntry>,
}

fn default_taskbar_widget_position() -> String {
    "notification".to_string()
}

fn default_taskbar_widget_font_weight() -> u16 {
    400
}

/// Light. The menu is a floating card read at a glance, not a dense readout,
/// and the Windows flyouts it sits next to are lighter than 400.
fn default_menu_font_weight() -> u16 {
    300
}

fn default_menu_font_family() -> String {
    "MiSans VF".to_string()
}

fn default_menu_font_size() -> u8 {
    12
}

/// Clamp a taskbar weight to the OpenType `wght` axis range.
///
/// This used to snap to 300/400/700 because the old GDI renderer could not do
/// anything else — `CreateFontW` collapsed 100..=550 to Regular and everything
/// above to Bold. The DirectWrite renderer drives the real axis, so intermediate
/// values are now meaningful and must not be quantized away. Legacy persisted
/// values (including the old three stops) remain valid inputs.
pub fn normalize_taskbar_widget_font_weight(value: u16) -> u16 {
    value.clamp(100, 1000)
}

/// Default taskbar font family.
///
/// MiSans VF is bundled with the app (DESIGN_SYSTEM.md §2), so a fresh install
/// gets a real `wght` axis without depending on the host font list.
fn default_taskbar_widget_font_family() -> String {
    "MiSans VF".to_string()
}

fn default_taskbar_widget_content() -> String {
    "usage".to_string()
}

fn default_taskbar_widget_icon_size() -> u8 {
    14
}

fn default_taskbar_widget_icon_gap_px() -> u8 { 5 }
fn default_taskbar_widget_value_gap_px() -> u8 { 2 }
fn default_taskbar_widget_icon_style() -> String {
    "pure".to_string()
}

/// One taskbar strip entry: a provider and which of its quota windows to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskbarEntry {
    /// Provider CLI name, or [`TASKBAR_PROVIDER_AUTO`] to follow whichever
    /// provider the tray icon is currently showing.
    pub provider_id: String,
    /// Which window: see [`normalize_taskbar_window`].
    pub window: String,
}

/// Follows the tray's own provider pick instead of naming one.
///
/// This is what lets the pre-entries default keep behaving exactly as before,
/// where the strip simply mirrored whatever the tray icon had selected.
pub const TASKBAR_PROVIDER_AUTO: &str = "auto";

/// Window kinds a taskbar entry may reference.
///
/// `speed` is not a quota window but is offered alongside them because the
/// legacy content setting could show it, and dropping it on migration would
/// silently remove a display the user had chosen.
/// Window kinds an entry may name.
///
/// `primary` is first and is the only one guaranteed to resolve: it means "this
/// provider's main quota, whatever cycle that turns out to be". The four named
/// cycles below it are matched by the window's DECLARED LENGTH against fixed
/// bands (session ≤6h, daily 20–28h, weekly 6–8d, monthly ≥27d), which is a
/// taxonomy, and a taxonomy always has providers that fall outside it — one
/// with a 14-day cycle, or one that reports a percentage without publishing a
/// length at all. Those used to be unrenderable no matter what the user picked.
/// Adding a provider must not require touching these bands.
pub const TASKBAR_WINDOWS: [&str; 7] = [
    "primary", "session", "weekly", "daily", "monthly", "balance", "speed",
];

/// Most entries the strip will keep. Beyond this the list is user noise: the
/// strip renders at most a couple of lines, and an unbounded list would let a
/// corrupt settings file grow without limit.
pub const TASKBAR_MAX_ENTRIES: usize = 6;

pub fn normalize_taskbar_window(value: &str) -> Option<String> {
    let lowered = value.trim().to_ascii_lowercase();
    TASKBAR_WINDOWS
        .iter()
        .find(|candidate| **candidate == lowered)
        .map(|candidate| (*candidate).to_string())
}

/// Canonicalize a requested entry list: drop unknown windows, drop blank
/// providers, drop duplicates, and cap the length.
///
/// Returns the migrated default when nothing valid survives, so the strip is
/// never left with an empty configuration it cannot render.
pub fn normalize_taskbar_entries(requested: &[TaskbarEntry]) -> Vec<TaskbarEntry> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for entry in requested {
        let provider = entry.provider_id.trim();
        if provider.is_empty() {
            continue;
        }
        let Some(window) = normalize_taskbar_window(&entry.window) else {
            continue;
        };
        let key = (provider.to_ascii_lowercase(), window.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(TaskbarEntry {
            provider_id: provider.to_string(),
            window,
        });
        if out.len() >= TASKBAR_MAX_ENTRIES {
            break;
        }
    }
    if out.is_empty() {
        return default_taskbar_widget_entries();
    }
    out
}

/// Build ordered floating-bar entries from the legacy provider-id list.
pub fn float_bar_entries_from_ids(ids: &[String]) -> Vec<TaskbarEntry> {
    ids.iter()
        .map(|id| TaskbarEntry {
            provider_id: id.clone(),
            window: "primary".to_string(),
        })
        .collect()
}

/// Canonicalize floating-bar entries. Empty is meaningful (all enabled), so
/// unlike the taskbar strip we do not inject a default when nothing survives.
/// When no explicit entries exist yet, seed from the legacy provider-id list.
pub fn normalize_float_bar_entries(
    requested: &[TaskbarEntry],
    legacy_ids: &[String],
) -> Vec<TaskbarEntry> {
    let source = if requested.is_empty() {
        float_bar_entries_from_ids(legacy_ids)
    } else {
        requested.to_vec()
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for entry in source {
        let provider = entry.provider_id.trim();
        if provider.is_empty() {
            continue;
        }
        let Some(window) = normalize_taskbar_window(&entry.window) else {
            continue;
        };
        let key = (provider.to_ascii_lowercase(), window.clone());
        if !seen.insert(key) {
            continue;
        }
        out.push(TaskbarEntry {
            provider_id: provider.to_string(),
            window,
        });
        if out.len() >= TASKBAR_MAX_ENTRIES {
            break;
        }
    }
    out
}

/// Normalize the legacy dashboard quota setting for old configuration files.
/// The dashboard no longer exposes or consumes this setting; keeping the
/// canonical representation avoids breaking older settings migrations.
pub fn normalize_dashboard_quota_windows(requested: &[String]) -> Vec<String> {
    let has_weekly = requested
        .iter()
        .any(|value| value.trim().eq_ignore_ascii_case("weekly"));
    let has_session = requested
        .iter()
        .any(|value| value.trim().eq_ignore_ascii_case("session"));

    if has_weekly && !has_session {
        vec!["weekly".to_string()]
    } else {
        vec!["session".to_string(), "weekly".to_string()]
    }
}

/// Translate the retired single-choice content setting into the ordered list.
///
/// The mappings reproduce exactly what each legacy value used to render, so an
/// upgrade is invisible to the user.
pub fn taskbar_entries_from_legacy_content(content: &str) -> Vec<TaskbarEntry> {
    let auto = |window: &str| TaskbarEntry {
        provider_id: TASKBAR_PROVIDER_AUTO.to_string(),
        window: window.to_string(),
    };
    match content.trim() {
        "speed" => vec![auto("speed")],
        "usage_speed" => vec![auto("session"), auto("speed")],
        // "usage" and anything unrecognized: the session line plus the weekly
        // line, which is what the strip has always shown by default.
        _ => vec![auto("session"), auto("weekly")],
    }
}

fn default_taskbar_widget_entries() -> Vec<TaskbarEntry> {
    taskbar_entries_from_legacy_content("usage")
}

fn default_taskbar_widget_font_size() -> u8 {
    12
}

fn default_taskbar_widget_width() -> u16 {
    136
}

fn default_taskbar_widget_text_align() -> String {
    "left".to_string()
}

/// Window kinds the floating bar can print a reset for.
///
/// `primary` means "whichever window this provider leads with" and is what the
/// bar did before the setting existed. The rest select by the window's declared
/// length, the same rule the taskbar strip uses.
pub const FLOAT_BAR_RESET_WINDOWS: [&str; 5] = ["primary", "session", "weekly", "daily", "monthly"];

/// Most resets one pill will print. Each one costs horizontal space in a bar
/// that is meant to stay small, and beyond three the pill stops being glanceable.
pub const FLOAT_BAR_MAX_RESET_WINDOWS: usize = 3;

fn default_float_bar_reset_windows() -> Vec<String> {
    vec!["primary".to_string()]
}

/// Canonicalize the requested reset windows: drop unknown names and duplicates,
/// keep the user's order, and cap the length.
///
/// An empty result is returned as-is rather than replaced by the default: the
/// user clearing every window is a legitimate way to say "no reset text", and
/// silently re-adding one would override that.
pub fn normalize_float_bar_reset_windows(requested: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in requested {
        let lowered = value.trim().to_ascii_lowercase();
        let Some(known) = FLOAT_BAR_RESET_WINDOWS
            .iter()
            .find(|candidate| **candidate == lowered)
        else {
            continue;
        };
        if !seen.insert(*known) {
            continue;
        }
        out.push((*known).to_string());
        if out.len() >= FLOAT_BAR_MAX_RESET_WINDOWS {
            break;
        }
    }
    out
}

fn default_tray_scale_percent() -> u16 {
    100
}

pub fn clamp_tray_scale_percent(value: u16) -> u16 {
    value.clamp(100, 200)
}

fn default_float_bar_opacity() -> u8 {
    80
}

fn default_float_bar_scale() -> u8 {
    100
}

fn default_float_bar_orientation() -> String {
    "horizontal".to_string()
}

fn default_float_bar_style() -> String {
    "floating".to_string()
}

/// Clamp the floating-bar opacity to the supported range.
///
/// Opacity values below 30% would make the bar effectively invisible, so we
/// pin the lower bound; the upper bound is the natural 100%.
pub fn clamp_float_bar_opacity(value: u8) -> u8 {
    value.clamp(30, 100)
}

/// Clamp the floating-bar visual scale to the supported range.
pub fn clamp_float_bar_scale(value: u8) -> u8 {
    value.clamp(75, 200)
}

/// Normalize a floating-bar orientation string. Unknown values fall back to
/// the default ("horizontal") so a corrupt settings file can't put the
/// renderer into an undefined state.
pub fn normalize_float_bar_orientation(value: &str) -> String {
    match value {
        "vertical" => "vertical".to_string(),
        _ => "horizontal".to_string(),
    }
}

/// Normalize a floating-bar style string. Unknown values fall back to the
/// original floating style so existing settings keep their previous look.
pub fn normalize_float_bar_style(value: &str) -> String {
    match value {
        "taskbar" => "taskbar".to_string(),
        _ => "floating".to_string(),
    }
}

/// Canonicalize a requested provider display order.
///
/// Keeps requested provider IDs that map to a real [`ProviderId`], drops
/// duplicates, and appends omitted providers in canonical order. An empty
/// request intentionally returns the full canonical order so display callers
/// can use one path for default and customized ordering.
pub fn normalize_provider_order(requested: &[String]) -> Vec<String> {
    let canonical = ProviderId::all()
        .iter()
        .map(|provider| provider.cli_name().to_string())
        .collect::<Vec<_>>();
    let valid = canonical.iter().map(String::as_str).collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(canonical.len());

    for provider_id in requested {
        if valid.contains(provider_id.as_str()) && seen.insert(provider_id.clone()) {
            out.push(provider_id.clone());
        }
    }
    for provider_id in canonical {
        if seen.insert(provider_id.clone()) {
            out.push(provider_id);
        }
    }

    out
}

fn default_global_shortcut() -> String {
    "Ctrl+Shift+U".to_string()
}

fn default_true() -> bool {
    true
}

fn default_local_usage_period() -> String {
    "7d".to_string()
}

/// Default cookie source value for browser-authenticated providers.
///
/// Browser cookie extraction reads browser profile databases and decrypts
/// Chromium cookies via Windows DPAPI. This is the CodexBar-compatible default
/// for web-session providers; `manual` is an explicit advanced/fallback mode.
/// See docs/COOKIES.md for the source ladder and privacy boundary.
const DEFAULT_COOKIE_SOURCE: &str = "auto";

/// Default usage source value for any provider.
const DEFAULT_PROVIDER_SOURCE: &str = "auto";

/// Default API region for providers that expose one.
fn default_api_region(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Alibaba => crate::providers::AlibabaRegion::Singapore.settings_value(),
        ProviderId::Zai | ProviderId::MiniMax => "global",
        _ => "",
    }
}

/// Default for the codex `openai_web_extras` boolean (true = show extras).
const DEFAULT_CODEX_OPENAI_WEB_EXTRAS: bool = true;

impl Default for Settings {
    fn default() -> Self {
        let mut enabled = HashSet::new();
        // Default enabled providers
        enabled.insert("claude".to_string());
        enabled.insert("codex".to_string());

        Self {
            enabled_providers: enabled,
            refresh_interval_secs: 300, // 5 minutes
            provider_timeout_recovery_enabled: true,
            refresh_all_providers_on_menu_open: false,
            start_minimized: false,
            start_at_login: false,
            show_notifications: true,
            sound_enabled: true,
            sound_volume: 100,
            high_usage_threshold: 70.0,
            critical_usage_threshold: 90.0,
            merge_tray_icons: false, // Show single provider by default
            tray_icon_mode: TrayIconMode::default(), // Single icon by default
            switcher_shows_icons: true,
            menu_bar_shows_highest_usage: false,
            show_as_used: true,        // Show as "used" by default
            enable_animations: true,   // Animations enabled by default
            reset_time_relative: true, // Show relative times by default
            menu_bar_display_mode: "detailed".to_string(), // Detailed mode by default
            output_speed_enabled: true,
            keep_tray_panel_on_settings: true,
            local_usage_period: default_local_usage_period(),
            show_all_token_accounts_in_menu: false,
            provider_configs: HashMap::new(),
            disable_keychain_access: false,
            hide_personal_info: false, // Show personal info by default
            update_channel: UpdateChannel::default(), // Stable by default
            provider_metrics: HashMap::new(), // Empty = use Automatic for all
            provider_order: Vec::new(), // Empty = canonical ProviderId::all() order
            global_shortcut: default_global_shortcut(), // Ctrl+Shift+U by default
            codex_custom_sessions_dirs: Vec::new(),
            auto_download_updates: false, // Require explicit opt-in for background downloads
            install_updates_on_quit: false, // Don't auto-install on quit by default
            ui_language: Language::default(), // English by default
            theme: ThemePreference::default(), // Auto (follows prefers-color-scheme)
            tray_scale_percent: default_tray_scale_percent(),
            float_bar_enabled: false,
            float_bar_opacity: default_float_bar_opacity(),
            float_bar_scale: default_float_bar_scale(),
            float_bar_orientation: default_float_bar_orientation(),
            float_bar_style: default_float_bar_style(),
            float_bar_click_through: false,
            float_bar_provider_ids: Vec::new(),
            float_bar_entries: Vec::new(),
            float_bar_dark_text: false,
            float_bar_show_reset_inline: false,
            float_bar_reset_windows: default_float_bar_reset_windows(),
            float_bar_show_cost: false,
            taskbar_widget_enabled: false,
            taskbar_widget_position: default_taskbar_widget_position(),
            taskbar_widget_font_weight: default_taskbar_widget_font_weight(),
            menu_font_weight: default_menu_font_weight(),
            menu_font_family: default_menu_font_family(),
            menu_font_size: default_menu_font_size(),
            taskbar_widget_font_family: default_taskbar_widget_font_family(),
            taskbar_widget_content: default_taskbar_widget_content(),
            taskbar_widget_entries: default_taskbar_widget_entries(),
            taskbar_widget_font_size: default_taskbar_widget_font_size(),
            taskbar_widget_icon_size: default_taskbar_widget_icon_size(),
            taskbar_widget_icon_style: default_taskbar_widget_icon_style(),
            taskbar_widget_icon_gap_px: default_taskbar_widget_icon_gap_px(),
            taskbar_widget_value_gap_px: default_taskbar_widget_value_gap_px(),
            taskbar_widget_width: default_taskbar_widget_width(),
            taskbar_widget_text_align: default_taskbar_widget_text_align(),
            float_bar_show_as_used: true,
            float_bar_reset_time_relative: true,
            dashboard_show_as_used: true,
            dashboard_reset_time_relative: true,
            dashboard_provider_ids: Vec::new(),
            dashboard_quota_windows: vec!["session".to_string(), "weekly".to_string()],
            taskbar_show_as_used: true,
            taskbar_reset_time_relative: true,
            taskbar_tooltip_entries: Vec::new(),
        }
    }
}

impl Settings {
    /// Get the settings file path
    pub fn settings_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("CodexBar").join("settings.json"))
    }

    /// Load settings from disk
    pub fn load() -> Self {
        #[allow(unused_mut)]
        let mut settings = match Self::settings_path() {
            Some(path) if path.exists() => match crate::secure_file::read_string(&path) {
                Ok(content) => {
                    serde_json::from_str(content.trim_start_matches('\u{feff}')).unwrap_or_default()
                }
                Err(_) => Self::default(),
            },
            _ => Self::default(),
        };

        // Sync autostart toggle with actual registry state and repair stale commands from older builds.
        #[cfg(target_os = "windows")]
        {
            settings.start_at_login = Self::sync_start_at_login_registry();
        }

        // MiMo's pay-as-you-go API card used the same browser session as the
        // Token Plan card solely to read the prepaid balance. The MiMo card
        // now owns that balance lookup, so migrate any existing enablement to
        // the single user-facing provider and persist the cleanup once.
        if settings.retire_mimo_api_provider() {
            let _ = settings.save();
        }

        settings
    }

    fn retire_mimo_api_provider(&mut self) -> bool {
        if !self
            .enabled_providers
            .remove(ProviderId::MiMoApi.cli_name())
        {
            return false;
        }
        self.enabled_providers
            .insert(ProviderId::MiMo.cli_name().to_string());
        true
    }

    /// Save settings to disk
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::settings_path()
            .ok_or_else(|| anyhow::anyhow!("Could not determine settings path"))?;

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)?;
        crate::secure_file::write_string(&path, &json)?;

        Ok(())
    }

    fn start_at_login_exe_path(current_exe: &std::path::Path) -> std::path::PathBuf {
        let file_name = current_exe.file_name().and_then(|name| name.to_str());
        if file_name.is_some_and(|name| {
            name.eq_ignore_ascii_case("codexbar-cli.exe")
                || name.eq_ignore_ascii_case("codexbar-desktop.exe")
        }) && let Some(desktop_exe) = current_exe
            .parent()
            .map(|dir| dir.join("codexbar.exe"))
            .filter(|path| path.exists())
        {
            return desktop_exe;
        }

        current_exe.to_path_buf()
    }

    fn start_at_login_command(current_exe: &std::path::Path) -> String {
        let exe_path = Self::start_at_login_exe_path(current_exe);
        format!("\"{}\"", exe_path.display())
    }

    fn start_at_login_command_needs_repair(existing: &str, current_exe: &std::path::Path) -> bool {
        existing != Self::start_at_login_command(current_exe)
    }

    #[cfg(target_os = "windows")]
    pub fn apply_start_at_login_registry(enabled: bool) -> anyhow::Result<()> {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let run_key = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_READ | KEY_WRITE,
        )?;

        if enabled {
            let exe_path = std::env::current_exe()?;
            let command = Self::start_at_login_command(&exe_path);
            run_key.set_value("CodexBar", &command)?;
        } else {
            let _ = run_key.delete_value("CodexBar");
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn sync_start_at_login_registry() -> bool {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(run_key) = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Run",
            KEY_READ | KEY_WRITE,
        ) else {
            return false;
        };

        let Ok(existing) = run_key.get_value::<String, _>("CodexBar") else {
            return false;
        };

        match std::env::current_exe() {
            Ok(exe_path) if Self::start_at_login_command_needs_repair(&existing, &exe_path) => {
                let command = Self::start_at_login_command(&exe_path);
                if let Err(error) = run_key.set_value("CodexBar", &command) {
                    tracing::warn!("Failed to repair CodexBar start-at-login command: {error}");
                }
            }
            Err(error) => {
                tracing::warn!(
                    "Failed to resolve current executable for start-at-login sync: {error}"
                );
            }
            _ => {}
        }

        true
    }

    #[cfg(not(target_os = "windows"))]
    pub fn apply_start_at_login_registry(_enabled: bool) -> anyhow::Result<()> {
        Ok(())
    }

    /// Set start at login (updates Windows registry)
    pub fn set_start_at_login(&mut self, enabled: bool) -> anyhow::Result<()> {
        self.start_at_login = enabled;
        Self::apply_start_at_login_registry(enabled)?;
        Ok(())
    }

    /// Check if start at login is actually enabled in registry
    #[cfg(target_os = "windows")]
    pub fn is_start_at_login_enabled() -> bool {
        use winreg::RegKey;
        use winreg::enums::*;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(run_key) = hkcu.open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run") {
            run_key.get_value::<String, _>("CodexBar").is_ok()
        } else {
            false
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn is_start_at_login_enabled() -> bool {
        false
    }

    /// Check if a provider is enabled
    pub fn is_provider_enabled(&self, id: ProviderId) -> bool {
        self.enabled_providers.contains(id.cli_name())
    }

    /// Enable a provider
    pub fn enable_provider(&mut self, id: ProviderId) {
        self.enabled_providers.insert(id.cli_name().to_string());
    }

    /// Disable a provider
    pub fn disable_provider(&mut self, id: ProviderId) {
        self.enabled_providers.remove(id.cli_name());
    }

    /// Toggle a provider's enabled state
    pub fn toggle_provider(&mut self, id: ProviderId) -> bool {
        let name = id.cli_name().to_string();
        if self.enabled_providers.contains(&name) {
            self.enabled_providers.remove(&name);
            false
        } else {
            self.enabled_providers.insert(name);
            true
        }
    }

    /// Get list of enabled provider IDs
    pub fn get_enabled_provider_ids(&self) -> Vec<ProviderId> {
        self.provider_display_order()
            .into_iter()
            .filter(|id| self.is_provider_enabled(*id))
            .collect()
    }

    /// Get all available providers with their enabled status
    pub fn get_all_providers_status(&self) -> Vec<ProviderStatus> {
        self.provider_display_order()
            .into_iter()
            .map(|id| ProviderStatus {
                id: id.cli_name().to_string(),
                name: id.display_name().to_string(),
                enabled: self.is_provider_enabled(id),
            })
            .collect()
    }

    /// Provider display order as typed IDs, falling back to canonical order
    /// when no custom order has been persisted.
    pub fn provider_display_order(&self) -> Vec<ProviderId> {
        normalize_provider_order(&self.provider_order)
            .into_iter()
            .filter_map(|provider_id| ProviderId::from_cli_name(&provider_id))
            .collect()
    }

    /// Provider display order as CLI-name strings.
    pub fn provider_display_order_names(&self) -> Vec<String> {
        normalize_provider_order(&self.provider_order)
    }

    /// Get the metric preference for a provider
    pub fn get_provider_metric(&self, id: ProviderId) -> MetricPreference {
        self.provider_metrics
            .get(id.cli_name())
            .copied()
            .unwrap_or_default()
    }

    /// Set the metric preference for a provider
    pub fn set_provider_metric(&mut self, id: ProviderId, metric: MetricPreference) {
        self.provider_metrics
            .insert(id.cli_name().to_string(), metric);
    }

    // ── Per-provider configuration accessors ─────────────────────────
    //
    // These thin wrappers around `provider_configs` apply provider-specific
    // defaults (e.g. cookie/usage source defaults to `"auto"`) so callers
    // never have to reach into the raw `Option<String>` fields. The
    // `*_str` / boolean / setter pairs intentionally mirror the names of
    // the legacy flat fields so call-site migration is mechanical.

    /// Read-only access to a provider's stored config, if any.
    pub fn provider_config(&self, id: ProviderId) -> Option<&ProviderConfig> {
        self.provider_configs.get(&id)
    }

    /// Mutable access to a provider's config, lazily creating an empty
    /// entry if none exists.
    pub fn provider_config_mut(&mut self, id: ProviderId) -> &mut ProviderConfig {
        self.provider_configs.entry(id).or_default()
    }

    /// Cookie source for `id`, or the default `"auto"` if unset.
    pub fn cookie_source(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.cookie_source.as_deref())
            .unwrap_or(DEFAULT_COOKIE_SOURCE)
    }

    pub fn set_cookie_source(&mut self, id: ProviderId, source: impl Into<String>) {
        self.provider_config_mut(id).cookie_source = Some(source.into());
    }

    /// Usage source for `id`, or the default `"auto"` if unset.
    pub fn usage_source(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.usage_source.as_deref())
            .unwrap_or(DEFAULT_PROVIDER_SOURCE)
    }

    pub fn set_usage_source(&mut self, id: ProviderId, source: impl Into<String>) {
        self.provider_config_mut(id).usage_source = Some(source.into());
    }

    /// API region for `id`, or the provider-specific default if unset.
    pub fn api_region(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.api_region.as_deref())
            .unwrap_or_else(|| default_api_region(id))
    }

    pub fn set_api_region(&mut self, id: ProviderId, region: impl Into<String>) {
        self.provider_config_mut(id).api_region = Some(region.into());
    }

    /// Manual cookie header for `id`, or `""` if unset.
    pub fn manual_cookie_header(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.manual_cookie_header.as_deref())
            .unwrap_or("")
    }

    pub fn set_manual_cookie_header(&mut self, id: ProviderId, header: impl Into<String>) {
        self.provider_config_mut(id).manual_cookie_header = Some(header.into());
    }

    /// API token for `id`, or `""` if unset.
    pub fn api_token(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.api_token.as_deref())
            .unwrap_or("")
    }

    pub fn set_api_token(&mut self, id: ProviderId, token: impl Into<String>) {
        self.provider_config_mut(id).api_token = Some(token.into());
    }

    /// Workspace ID override for `id`, or `""` if unset.
    pub fn workspace_id(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.workspace_id.as_deref())
            .unwrap_or("")
    }

    pub fn set_workspace_id(&mut self, id: ProviderId, value: impl Into<String>) {
        self.provider_config_mut(id).workspace_id = Some(value.into());
    }

    pub fn gateway_url(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.gateway_url.as_deref())
            .unwrap_or_else(|| {
                if id == ProviderId::Wayfinder {
                    crate::providers::wayfinder::DEFAULT_GATEWAY_URL
                } else {
                    ""
                }
            })
    }

    pub fn set_gateway_url(&mut self, id: ProviderId, value: impl Into<String>) {
        self.provider_config_mut(id).gateway_url = Some(value.into());
    }

    /// IDE base path override for `id`, or `""` if unset.
    pub fn ide_base_path(&self, id: ProviderId) -> &str {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.ide_base_path.as_deref())
            .unwrap_or("")
    }

    pub fn set_ide_base_path(&mut self, id: ProviderId, value: impl Into<String>) {
        self.provider_config_mut(id).ide_base_path = Some(value.into());
    }

    /// Codex `openai_web_extras` toggle, default `true`.
    pub fn openai_web_extras(&self, id: ProviderId) -> bool {
        self.provider_configs
            .get(&id)
            .and_then(|c| c.openai_web_extras)
            .unwrap_or(DEFAULT_CODEX_OPENAI_WEB_EXTRAS)
    }

    pub fn set_openai_web_extras(&mut self, id: ProviderId, value: bool) {
        self.provider_config_mut(id).openai_web_extras = Some(value);
    }

    /// Per-provider historical-tracking toggle (currently codex-only).
    pub fn historical_tracking(&self, id: ProviderId) -> bool {
        self.provider_configs
            .get(&id)
            .map(|c| c.historical_tracking)
            .unwrap_or(false)
    }

    pub fn set_historical_tracking(&mut self, id: ProviderId, value: bool) {
        self.provider_config_mut(id).historical_tracking = value;
    }

    /// Per-provider "avoid keychain prompts" toggle (currently claude-only).
    pub fn avoid_keychain_prompts(&self, id: ProviderId) -> bool {
        self.provider_configs
            .get(&id)
            .map(|c| c.avoid_keychain_prompts)
            .unwrap_or(false)
    }

    pub fn set_avoid_keychain_prompts(&mut self, id: ProviderId, value: bool) {
        self.provider_config_mut(id).avoid_keychain_prompts = value;
    }

    // ── Legacy field-name aliases ────────────────────────────────────
    //
    // Keep the names of the old flat per-provider fields available as
    // accessor methods so existing call sites only need a `()` (read) or
    // `set_` prefix (write). New code should prefer the typed accessors
    // above.

    pub fn codex_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Codex)
    }
    pub fn set_codex_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Codex, v)
    }
    pub fn claude_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Claude)
    }
    pub fn set_claude_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Claude, v)
    }
    pub fn cursor_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Cursor)
    }
    pub fn set_cursor_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Cursor, v)
    }
    pub fn opencode_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::OpenCode)
    }
    pub fn set_opencode_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::OpenCode, v)
    }
    pub fn factory_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Factory)
    }
    pub fn set_factory_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Factory, v)
    }
    pub fn alibaba_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Alibaba)
    }
    pub fn set_alibaba_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Alibaba, v)
    }
    pub fn kimi_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Kimi)
    }
    pub fn set_kimi_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Kimi, v)
    }
    pub fn minimax_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::MiniMax)
    }
    pub fn set_minimax_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::MiniMax, v)
    }
    pub fn augment_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Augment)
    }
    pub fn set_augment_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Augment, v)
    }
    pub fn amp_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Amp)
    }
    pub fn set_amp_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Amp, v)
    }
    pub fn ollama_cookie_source(&self) -> &str {
        self.cookie_source(ProviderId::Ollama)
    }
    pub fn set_ollama_cookie_source(&mut self, v: impl Into<String>) {
        self.set_cookie_source(ProviderId::Ollama, v)
    }

    pub fn claude_usage_source(&self) -> &str {
        self.usage_source(ProviderId::Claude)
    }
    pub fn set_claude_usage_source(&mut self, v: impl Into<String>) {
        self.set_usage_source(ProviderId::Claude, v)
    }
    pub fn codex_usage_source(&self) -> &str {
        self.usage_source(ProviderId::Codex)
    }
    pub fn set_codex_usage_source(&mut self, v: impl Into<String>) {
        self.set_usage_source(ProviderId::Codex, v)
    }

    pub fn alibaba_api_region(&self) -> &str {
        self.api_region(ProviderId::Alibaba)
    }
    pub fn set_alibaba_api_region(&mut self, v: impl Into<String>) {
        self.set_api_region(ProviderId::Alibaba, v)
    }
    pub fn zai_api_region(&self) -> &str {
        self.api_region(ProviderId::Zai)
    }
    pub fn set_zai_api_region(&mut self, v: impl Into<String>) {
        self.set_api_region(ProviderId::Zai, v)
    }
    pub fn minimax_api_region(&self) -> &str {
        self.api_region(ProviderId::MiniMax)
    }
    pub fn set_minimax_api_region(&mut self, v: impl Into<String>) {
        self.set_api_region(ProviderId::MiniMax, v)
    }

    pub fn alibaba_cookie_header(&self) -> &str {
        self.manual_cookie_header(ProviderId::Alibaba)
    }
    pub fn set_alibaba_cookie_header(&mut self, v: impl Into<String>) {
        self.set_manual_cookie_header(ProviderId::Alibaba, v)
    }
    pub fn kimi_manual_cookie_header(&self) -> &str {
        self.manual_cookie_header(ProviderId::Kimi)
    }
    pub fn set_kimi_manual_cookie_header(&mut self, v: impl Into<String>) {
        self.set_manual_cookie_header(ProviderId::Kimi, v)
    }
    pub fn augment_cookie_header(&self) -> &str {
        self.manual_cookie_header(ProviderId::Augment)
    }
    pub fn set_augment_cookie_header(&mut self, v: impl Into<String>) {
        self.set_manual_cookie_header(ProviderId::Augment, v)
    }
    pub fn amp_cookie_header(&self) -> &str {
        self.manual_cookie_header(ProviderId::Amp)
    }
    pub fn set_amp_cookie_header(&mut self, v: impl Into<String>) {
        self.set_manual_cookie_header(ProviderId::Amp, v)
    }
    pub fn ollama_cookie_header(&self) -> &str {
        self.manual_cookie_header(ProviderId::Ollama)
    }
    pub fn set_ollama_cookie_header(&mut self, v: impl Into<String>) {
        self.set_manual_cookie_header(ProviderId::Ollama, v)
    }
    pub fn minimax_cookie_header(&self) -> &str {
        self.manual_cookie_header(ProviderId::MiniMax)
    }
    pub fn set_minimax_cookie_header(&mut self, v: impl Into<String>) {
        self.set_manual_cookie_header(ProviderId::MiniMax, v)
    }

    pub fn opencode_workspace_id(&self) -> &str {
        self.workspace_id(ProviderId::OpenCode)
    }
    pub fn set_opencode_workspace_id(&mut self, v: impl Into<String>) {
        self.set_workspace_id(ProviderId::OpenCode, v)
    }
    pub fn minimax_api_token(&self) -> &str {
        self.api_token(ProviderId::MiniMax)
    }
    pub fn set_minimax_api_token(&mut self, v: impl Into<String>) {
        self.set_api_token(ProviderId::MiniMax, v)
    }
    pub fn jetbrains_ide_base_path(&self) -> &str {
        self.ide_base_path(ProviderId::JetBrains)
    }
    pub fn set_jetbrains_ide_base_path(&mut self, v: impl Into<String>) {
        self.set_ide_base_path(ProviderId::JetBrains, v)
    }

    pub fn codex_openai_web_extras(&self) -> bool {
        self.openai_web_extras(ProviderId::Codex)
    }
    pub fn set_codex_openai_web_extras(&mut self, v: bool) {
        self.set_openai_web_extras(ProviderId::Codex, v)
    }
    pub fn codex_historical_tracking(&self) -> bool {
        self.historical_tracking(ProviderId::Codex)
    }
    pub fn set_codex_historical_tracking(&mut self, v: bool) {
        self.set_historical_tracking(ProviderId::Codex, v)
    }
    pub fn claude_avoid_keychain_prompts(&self) -> bool {
        self.avoid_keychain_prompts(ProviderId::Claude)
    }
    pub fn set_claude_avoid_keychain_prompts(&mut self, v: bool) {
        self.set_avoid_keychain_prompts(ProviderId::Claude, v)
    }
}
