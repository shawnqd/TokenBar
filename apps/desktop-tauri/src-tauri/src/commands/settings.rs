use super::*;

// ── Settings mutation ─────────────────────────────────────────────────

/// Partial settings update — every field is optional so the frontend can
/// send only what changed.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SettingsUpdate {
    pub enabled_providers: Option<Vec<String>>,
    pub refresh_interval_secs: Option<u64>,
    pub refresh_all_providers_on_menu_open: Option<bool>,
    pub start_at_login: Option<bool>,
    pub start_minimized: Option<bool>,
    pub show_notifications: Option<bool>,
    pub sound_enabled: Option<bool>,
    pub sound_volume: Option<u8>,
    pub high_usage_threshold: Option<f64>,
    pub critical_usage_threshold: Option<f64>,
    pub tray_icon_mode: Option<String>,
    pub switcher_shows_icons: Option<bool>,
    pub menu_bar_shows_highest_usage: Option<bool>,
    pub show_as_used: Option<bool>,
    pub show_all_token_accounts_in_menu: Option<bool>,
    pub enable_animations: Option<bool>,
    pub reset_time_relative: Option<bool>,
    pub menu_bar_display_mode: Option<String>,
    pub output_speed_enabled: Option<bool>,
    pub local_usage_period: Option<String>,
    pub hide_personal_info: Option<bool>,
    pub update_channel: Option<String>,
    pub auto_download_updates: Option<bool>,
    pub install_updates_on_quit: Option<bool>,
    pub global_shortcut: Option<String>,
    pub codex_custom_sessions_dirs: Option<Vec<String>>,
    pub ui_language: Option<String>,
    pub theme: Option<String>,
    pub window_scale_percent: Option<u16>,
    pub tray_scale_percent: Option<u16>,
    pub claude_avoid_keychain_prompts: Option<bool>,
    pub disable_keychain_access: Option<bool>,
    /// Map of provider CLI name → metric preference label.
    pub provider_metrics: Option<std::collections::HashMap<String, String>>,
    pub float_bar_enabled: Option<bool>,
    pub float_bar_opacity: Option<u8>,
    pub float_bar_scale: Option<u8>,
    pub float_bar_orientation: Option<String>,
    pub float_bar_style: Option<String>,
    pub float_bar_click_through: Option<bool>,
    pub float_bar_provider_ids: Option<Vec<String>>,
    pub float_bar_dark_text: Option<bool>,
    pub float_bar_show_reset_inline: Option<bool>,
    pub float_bar_reset_windows: Option<Vec<String>>,
    pub float_bar_show_cost: Option<bool>,
    pub taskbar_widget_enabled: Option<bool>,
    pub taskbar_widget_position: Option<String>,
    pub taskbar_widget_font_weight: Option<u16>,
    pub menu_font_weight: Option<u16>,
    pub menu_font_family: Option<String>,
    pub menu_font_size: Option<u8>,
    pub taskbar_widget_content: Option<String>,
    /// The ordered strip entries (item G). Absent before this field existed,
    /// which meant every edit in the composer was silently discarded by serde.
    pub taskbar_widget_entries: Option<Vec<TaskbarEntryBridge>>,
    pub taskbar_widget_font_family: Option<String>,
    pub taskbar_widget_font_size: Option<u8>,
    pub taskbar_widget_width: Option<u16>,
    pub taskbar_widget_text_align: Option<String>,
    // Per-component quota presentation. Each surface owns its own pair; the
    // legacy `show_as_used` / `reset_time_relative` above are migration-only.
    pub float_bar_show_as_used: Option<bool>,
    pub float_bar_reset_time_relative: Option<bool>,
    pub dashboard_show_as_used: Option<bool>,
    pub dashboard_reset_time_relative: Option<bool>,
    /// Which providers the tray flyout and pop-out panel show. Empty = all
    /// enabled, the counterpart of `float_bar_provider_ids`.
    pub dashboard_provider_ids: Option<Vec<String>>,
    /// Which quota-window cycles the dashboard cards render. Empty = all.
    pub dashboard_quota_windows: Option<Vec<String>>,
    pub taskbar_show_as_used: Option<bool>,
    pub taskbar_reset_time_relative: Option<bool>,
    pub taskbar_tooltip_entries: Option<Vec<TaskbarEntryBridge>>,
}

impl SettingsUpdate {
    fn notifies_float_bar(&self) -> bool {
        self.enabled_providers.is_some()
            || self.refresh_interval_secs.is_some()
            || self.codex_custom_sessions_dirs.is_some()
            || self.high_usage_threshold.is_some()
            || self.critical_usage_threshold.is_some()
            || self.float_bar_show_as_used.is_some()
            || self.float_bar_reset_time_relative.is_some()
    }

    fn rebuilds_tray_menu(&self) -> bool {
        self.float_bar_enabled.is_some() || self.ui_language.is_some()
    }

    fn refreshes_tray_presentation(&self) -> bool {
        self.tray_icon_mode.is_some()
            || self.switcher_shows_icons.is_some()
            || self.menu_bar_shows_highest_usage.is_some()
            || self.dashboard_show_as_used.is_some()
            || self.dashboard_reset_time_relative.is_some()
            || self.dashboard_provider_ids.is_some()
            || self.dashboard_quota_windows.is_some()
            || self.taskbar_show_as_used.is_some()
            || self.taskbar_reset_time_relative.is_some()
            || self.menu_bar_display_mode.is_some()
            || self.output_speed_enabled.is_some()
            || self.local_usage_period.is_some()
            || self.provider_metrics.is_some()
            || self.enabled_providers.is_some()
            || self.ui_language.is_some()
            || self.taskbar_widget_content.is_some()
            || self.taskbar_widget_entries.is_some()
            || self.taskbar_widget_font_weight.is_some()
            || self.menu_font_weight.is_some()
            || self.menu_font_family.is_some()
            || self.menu_font_size.is_some()
            || self.taskbar_widget_font_family.is_some()
            || self.taskbar_widget_font_size.is_some()
            || self.taskbar_widget_width.is_some()
            || self.taskbar_widget_text_align.is_some()
            || self.taskbar_widget_position.is_some()
            || self.taskbar_widget_enabled.is_some()
            || self.taskbar_tooltip_entries.is_some()
    }

    fn validate_shortcut_change(
        &self,
        app: &tauri::AppHandle,
        current_shortcut: &str,
    ) -> Result<(), String> {
        let Some(new_shortcut) = &self.global_shortcut else {
            return Ok(());
        };

        if new_shortcut.trim().is_empty() {
            crate::shortcut_bridge::unregister_shortcut(app, current_shortcut)?;
        } else if new_shortcut != current_shortcut {
            crate::shortcut_bridge::reregister_shortcut(app, current_shortcut, new_shortcut)?;
        }

        Ok(())
    }

    fn apply_provider_settings(self, settings: &mut Settings) -> Self {
        if let Some(providers) = self.enabled_providers.clone() {
            settings.enabled_providers = providers.into_iter().collect::<HashSet<_>>();
        }
        if let Some(v) = self.refresh_interval_secs {
            settings.refresh_interval_secs = v;
        }
        if let Some(v) = self.refresh_all_providers_on_menu_open {
            settings.refresh_all_providers_on_menu_open = v;
        }
        if let Some(ref s) = self.tray_icon_mode
            && let Some(mode) = parse_tray_icon_mode(s)
        {
            settings.tray_icon_mode = mode;
        }
        if let Some(v) = self.provider_metrics.clone() {
            apply_provider_metrics(settings, v);
        }
        self
    }

    fn apply_general_settings(self, settings: &mut Settings) -> Result<Self, String> {
        if let Some(v) = self.start_at_login {
            settings.set_start_at_login(v).map_err(|e| e.to_string())?;
        }
        if let Some(v) = self.start_minimized {
            settings.start_minimized = v;
        }
        if let Some(v) = self.global_shortcut.clone() {
            settings.global_shortcut = v;
        }
        if let Some(v) = self.ui_language.as_deref().and_then(parse_language)
            && settings.ui_language != v
        {
            settings.ui_language = v;
        }
        if let Some(v) = self.theme.as_deref().and_then(parse_theme) {
            settings.theme = v;
        }
        Ok(self)
    }

    fn apply_display_settings(self, settings: &mut Settings) -> Self {
        if let Some(v) = self.show_as_used {
            settings.show_as_used = v;
        }
        if let Some(v) = self.reset_time_relative {
            settings.reset_time_relative = v;
        }
        if let Some(v) = self.menu_bar_display_mode.clone() {
            settings.menu_bar_display_mode = v;
        }
        if let Some(v) = self.output_speed_enabled {
            settings.output_speed_enabled = v;
        }
        if let Some(ref v) = self.local_usage_period
            && matches!(v.as_str(), "today" | "7d" | "30d")
        {
            settings.local_usage_period = v.clone();
        }
        if let Some(v) = self.window_scale_percent {
            settings.window_scale_percent = codexbar::settings::clamp_window_scale_percent(v);
        }
        if let Some(v) = self.tray_scale_percent {
            settings.tray_scale_percent = codexbar::settings::clamp_tray_scale_percent(v);
        }
        if let Some(v) = self.switcher_shows_icons {
            settings.switcher_shows_icons = v;
        }
        if let Some(v) = self.menu_bar_shows_highest_usage {
            settings.menu_bar_shows_highest_usage = v;
        }
        if let Some(v) = self.show_all_token_accounts_in_menu {
            settings.show_all_token_accounts_in_menu = v;
        }
        // Per-component quota presentation. Deliberately applied one field at a
        // time with no cross-assignment: a floating-bar change must never touch
        // the dashboard's or the taskbar's stored choice.
        if let Some(v) = self.float_bar_show_as_used {
            settings.float_bar_show_as_used = v;
        }
        if let Some(v) = self.float_bar_reset_time_relative {
            settings.float_bar_reset_time_relative = v;
        }
        if let Some(v) = self.dashboard_show_as_used {
            settings.dashboard_show_as_used = v;
        }
        if let Some(v) = self.dashboard_reset_time_relative {
            settings.dashboard_reset_time_relative = v;
        }
        if let Some(ref ids) = self.dashboard_provider_ids {
            settings.dashboard_provider_ids = ids.clone();
        }
        if let Some(ref windows) = self.dashboard_quota_windows {
            settings.dashboard_quota_windows = windows.clone();
        }
        if let Some(v) = self.taskbar_reset_time_relative {
            settings.taskbar_reset_time_relative = v;
        }
        if let Some(v) = self.taskbar_show_as_used {
            settings.taskbar_show_as_used = v;
        }
        if let Some(ref entries) = self.taskbar_tooltip_entries {
            let requested: Vec<codexbar::settings::TaskbarEntry> =
                entries.iter().map(Into::into).collect();
            settings.taskbar_tooltip_entries =
                codexbar::settings::normalize_taskbar_entries(&requested);
        }
        self
    }

    fn apply_notification_settings(self, settings: &mut Settings) -> Self {
        if let Some(v) = self.show_notifications {
            settings.show_notifications = v;
        }
        if let Some(v) = self.sound_enabled {
            settings.sound_enabled = v;
        }
        if let Some(v) = self.sound_volume {
            settings.sound_volume = v;
        }
        if let Some(v) = self.high_usage_threshold {
            settings.high_usage_threshold = v.clamp(0.0, 100.0);
        }
        if let Some(v) = self.critical_usage_threshold {
            settings.critical_usage_threshold = v.clamp(0.0, 100.0);
        }
        self
    }

    fn apply_advanced_settings(self, settings: &mut Settings) -> Self {
        if let Some(v) = self.enable_animations {
            settings.enable_animations = v;
        }
        if let Some(v) = self.hide_personal_info {
            settings.hide_personal_info = v;
        }
        if let Some(v) = self
            .update_channel
            .as_deref()
            .and_then(parse_update_channel)
        {
            settings.update_channel = v;
        }
        if let Some(v) = self.auto_download_updates {
            settings.auto_download_updates = v;
        }
        if let Some(v) = self.codex_custom_sessions_dirs.clone() {
            settings.codex_custom_sessions_dirs = normalize_custom_sessions_dirs(v);
        }
        if let Some(v) = self.install_updates_on_quit {
            settings.install_updates_on_quit = v;
        }
        if let Some(v) = self.claude_avoid_keychain_prompts {
            settings.set_claude_avoid_keychain_prompts(v);
        }
        if let Some(v) = self.disable_keychain_access {
            settings.disable_keychain_access = v;
            if v {
                settings.set_claude_avoid_keychain_prompts(true);
            }
        }
        if let Some(v) = self.taskbar_widget_enabled {
            settings.taskbar_widget_enabled = v;
        }
        if let Some(v) = self.taskbar_widget_position.as_deref() {
            settings.taskbar_widget_position = match v {
                "left" => "left".to_string(),
                _ => "notification".to_string(),
            };
        }
        if let Some(v) = self.menu_font_weight {
            // Same clamp as the taskbar's weight — the two surfaces pick their
            // own value but share the axis range.
            settings.menu_font_weight = codexbar::settings::normalize_taskbar_widget_font_weight(v);
        }
        if let Some(ref v) = self.menu_font_family
            && !v.trim().is_empty()
        {
            settings.menu_font_family = v.clone();
        }
        if let Some(v) = self.menu_font_size {
            settings.menu_font_size = v.clamp(10, 16);
        }
        if let Some(v) = self.taskbar_widget_font_weight {
            settings.taskbar_widget_font_weight =
                codexbar::settings::normalize_taskbar_widget_font_weight(v);
        }
        if let Some(v) = self.taskbar_widget_content.as_deref() {
            settings.taskbar_widget_content = match v {
                "speed" | "usage_speed" => v.to_string(),
                _ => "usage".to_string(),
            };
        }
        if let Some(ref v) = self.taskbar_widget_font_family {
            let trimmed = v.trim();
            if !trimmed.is_empty() {
                settings.taskbar_widget_font_family = trimmed.to_string();
            }
        }
        if let Some(v) = self.taskbar_widget_font_size {
            settings.taskbar_widget_font_size = v.clamp(10, 16);
        }
        if let Some(v) = self.taskbar_widget_width {
            settings.taskbar_widget_width = v.clamp(96, 240);
        }
        if let Some(v) = self.taskbar_widget_text_align.as_deref() {
            settings.taskbar_widget_text_align = match v {
                "center" | "right" => v.to_string(),
                _ => "left".to_string(),
            };
        }
        if let Some(ref entries) = self.taskbar_widget_entries {
            // Normalized here rather than trusted: the request can name an
            // unknown window or repeat a pair, and the strip must never be left
            // with a configuration it cannot render.
            let requested: Vec<codexbar::settings::TaskbarEntry> =
                entries.iter().map(Into::into).collect();
            settings.taskbar_widget_entries =
                codexbar::settings::normalize_taskbar_entries(&requested);
        }
        self
    }

    fn float_bar_patch(&self) -> crate::floatbar::SettingsPatch {
        crate::floatbar::SettingsPatch {
            enabled: self.float_bar_enabled,
            opacity: self.float_bar_opacity,
            scale: self.float_bar_scale,
            orientation: self.float_bar_orientation.clone(),
            style: self.float_bar_style.clone(),
            click_through: self.float_bar_click_through,
            provider_ids: self.float_bar_provider_ids.clone(),
            dark_text: self.float_bar_dark_text,
            show_reset_inline: self.float_bar_show_reset_inline,
            reset_windows: self.float_bar_reset_windows.clone(),
            show_cost: self.float_bar_show_cost,
        }
    }

    fn apply_to(self, settings: &mut Settings) -> Result<crate::floatbar::SettingsPatch, String> {
        let float_bar_patch = self.float_bar_patch();
        self.apply_provider_settings(settings)
            .apply_general_settings(settings)?
            .apply_display_settings(settings)
            .apply_notification_settings(settings)
            .apply_advanced_settings(settings);
        float_bar_patch.apply(settings);
        Ok(float_bar_patch)
    }
}

fn normalize_custom_sessions_dirs(dirs: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for dir in dirs {
        let trimmed = dir.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = trimmed.replace('/', "\\").to_ascii_lowercase();
        if seen.insert(key) {
            out.push(trimmed.to_string());
        }
    }

    out
}

fn apply_provider_metrics(
    settings: &mut Settings,
    metrics_map: std::collections::HashMap<String, String>,
) {
    for (provider, label) in metrics_map {
        if let Some(pref) = parse_metric_preference(&label) {
            settings.provider_metrics.insert(provider, pref);
        }
    }
}

fn parse_tray_icon_mode(s: &str) -> Option<TrayIconMode> {
    match s {
        "single" => Some(TrayIconMode::Single),
        "perProvider" => Some(TrayIconMode::PerProvider),
        _ => None,
    }
}

fn parse_update_channel(s: &str) -> Option<UpdateChannel> {
    match s {
        "stable" => Some(UpdateChannel::Stable),
        "beta" => Some(UpdateChannel::Beta),
        _ => None,
    }
}

fn parse_language(s: &str) -> Option<Language> {
    Language::resolve(s)
}

/// Which quota windows each provider can actually answer for, right now.
///
/// The composer's window dropdown is built from this, so it never offers a
/// choice that would render "unsupported" on the strip: Grok publishes only a
/// monthly window, and a prepaid-balance account such as DeepSeek has no
/// percentage quota at all and only its balance to show.
///
/// Computed from the same `window_by_kind` / `balance_amount` the strip itself
/// resolves with, so the menu and the strip cannot disagree.
#[tauri::command]
pub fn get_taskbar_window_availability(
    state: tauri::State<'_, Mutex<AppState>>,
) -> std::collections::HashMap<String, Vec<String>> {
    let snapshots = state
        .lock()
        .map(|guard| guard.provider_cache.clone())
        .unwrap_or_default();
    let speed = crate::commands::get_output_speed_snapshot();
    snapshots
        .iter()
        .map(|snapshot| {
            let has_speed = match snapshot.provider_id.as_str() {
                "codex" => speed.codex.tokens_per_second.is_some(),
                "claude" => speed.claude.tokens_per_second.is_some(),
                "grok" => speed.grok.tokens_per_second.is_some(),
                _ => false,
            };
            (
                snapshot.provider_id.clone(),
                crate::taskbar_entries::available_windows(snapshot, has_speed)
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
            )
        })
        .collect()
}

/// One cell of the taskbar strip, exactly as it is being painted right now.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskbarPreviewLine {
    /// The provider's brand mark, or `None` when the text carries its name.
    pub glyph: Option<String>,
    /// The mark's own colour as `#rrggbb`; the identity is in the colour, not
    /// the shape, so the preview has to reproduce it to be worth anything.
    pub color: Option<String>,
    pub text: String,
}

/// What the strip is printing, for the settings page to show back to the user.
///
/// Reads the renderer's own line buffer instead of rebuilding an approximation
/// in TypeScript. The preview used to do the latter and was wrong in two
/// visible ways at once: it printed fixed sample percentages (a Grok entry read
/// 18% while the strip beside it read 47%) and it printed "unsupported" for
/// every balance entry, months after balances started rendering properly.
///
/// A preview that re-derives its content will drift from the thing it previews
/// every time either side changes. Reading the buffer makes drift impossible
/// rather than merely unlikely.
///
/// `update_settings` refreshes the tray presentation before it returns, so the
/// buffer is already current when the frontend re-fetches after an edit.
#[tauri::command]
pub fn get_taskbar_preview_lines() -> Vec<TaskbarPreviewLine> {
    #[cfg(windows)]
    {
        crate::taskbar_widget::current_entries()
            .into_iter()
            .map(|line| TaskbarPreviewLine {
                glyph: line.mark.map(|mark| mark.glyph.to_string()),
                color: line.mark.map(|mark| format!("#{:06x}", mark.color_rgb)),
                text: line.text,
            })
            .collect()
    }
    #[cfg(not(windows))]
    Vec::new()
}

/// Font families the taskbar strip can actually render, with whether each one
/// supports a continuous weight axis.
///
/// Enumerated from the live `IDWriteFontCollection` rather than hardcoded, so
/// the settings UI can never offer a family this machine does not have, and can
/// tell the user honestly which choices make the weight slider continuous.
/// Returns an empty list off Windows.
#[tauri::command]
pub async fn get_taskbar_font_families() -> Vec<TaskbarFontFamily> {
    #[cfg(windows)]
    {
        crate::taskbar_text::font_families()
            .iter()
            .map(|f| TaskbarFontFamily {
                name: f.name.clone(),
                variable_weight: f.variable_weight,
                has_cjk: f.has_cjk,
                recommended: f.recommended,
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarFontFamily {
    pub name: String,
    pub variable_weight: bool,
    /// Whether the family can draw Chinese without falling back mid-line.
    pub has_cjk: bool,
    /// Whether the picker shows this family before "show all" is switched on.
    pub recommended: bool,
}

#[tauri::command]
pub async fn update_settings(
    app: tauri::AppHandle,
    patch: SettingsUpdate,
) -> Result<SettingsSnapshot, String> {
    let mut settings = Settings::load();
    let notify_float_bar = patch.notifies_float_bar();
    let clear_local_usage_cache = patch.codex_custom_sessions_dirs.is_some();
    let rebuild_tray_menu = patch.rebuilds_tray_menu();
    let refresh_tray_presentation = patch.refreshes_tray_presentation();
    let enabled_providers_changed = patch.enabled_providers.is_some();
    let previous_language = settings.ui_language;
    #[cfg(windows)]
    let taskbar_widget_toggled = patch.taskbar_widget_enabled;
    #[cfg(windows)]
    let taskbar_widget_position = patch.taskbar_widget_position.clone();
    #[cfg(windows)]
    let taskbar_widget_font_weight = patch.taskbar_widget_font_weight;
    #[cfg(windows)]
    let taskbar_widget_content = patch.taskbar_widget_content.clone();
    #[cfg(windows)]
    let taskbar_widget_font_family = patch.taskbar_widget_font_family.clone();
    let taskbar_widget_font_size = patch.taskbar_widget_font_size;
    #[cfg(windows)]
    let taskbar_widget_width = patch.taskbar_widget_width;
    #[cfg(windows)]
    let taskbar_widget_text_align = patch.taskbar_widget_text_align.clone();

    patch.validate_shortcut_change(&app, &settings.global_shortcut)?;
    let float_bar_patch = patch.apply_to(&mut settings)?;

    if settings.ui_language != previous_language {
        let _ = app.emit(events::LOCALE_CHANGED, language_label(settings.ui_language));
    }

    settings.save().map_err(|e| e.to_string())?;
    if enabled_providers_changed {
        let enabled_ids = settings.get_enabled_provider_ids();
        let state = app.state::<Mutex<AppState>>();
        invalidate_provider_refresh_and_prune_disabled(&state, &enabled_ids)?;
    }
    if clear_local_usage_cache {
        crate::commands::clear_provider_local_usage_cache();
    }

    crate::floatbar::after_settings_saved(&app, &float_bar_patch, &settings, notify_float_bar);
    #[cfg(windows)]
    if let Some(enabled) = taskbar_widget_toggled {
        crate::taskbar_widget::set_enabled(enabled);
    }
    #[cfg(windows)]
    if let Some(position) = taskbar_widget_position {
        crate::taskbar_widget::set_position(&position);
    }
    #[cfg(windows)]
    if let Some(font_weight) = taskbar_widget_font_weight {
        crate::taskbar_widget::set_font_weight(font_weight);
    }
    #[cfg(windows)]
    if let Some(content) = taskbar_widget_content {
        crate::taskbar_widget::set_content(&content);
    }
    #[cfg(windows)]
    if let Some(ref font_family) = taskbar_widget_font_family {
        crate::taskbar_widget::set_font_family(font_family);
    }
    if let Some(font_size) = taskbar_widget_font_size {
        crate::taskbar_widget::set_font_size(font_size);
    }
    #[cfg(windows)]
    if let Some(width) = taskbar_widget_width {
        crate::taskbar_widget::set_width(width);
    }
    #[cfg(windows)]
    if let Some(text_align) = taskbar_widget_text_align {
        crate::taskbar_widget::set_text_align(&text_align);
    }
    if rebuild_tray_menu {
        crate::tray_bridge::rebuild_tray_menu(&app);
    }
    if refresh_tray_presentation {
        crate::tray_bridge::refresh_tray_presentation(&app);
    }

    // Notify other windows (PopOut dashboard, tray, float bar) so they re-read
    // settings live — e.g. the Display tab's window-scale slider takes effect
    // immediately instead of only after the PopOut is reopened.
    events::emit_settings_changed(&app);

    Ok(SettingsSnapshot::from(settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_settings_that_affect_tray_trigger_presentation_refresh() {
        assert!(
            SettingsUpdate {
                switcher_shows_icons: Some(false),
                ..Default::default()
            }
            .refreshes_tray_presentation()
        );
        assert!(
            SettingsUpdate {
                dashboard_reset_time_relative: Some(false),
                ..Default::default()
            }
            .refreshes_tray_presentation()
        );
        assert!(
            SettingsUpdate {
                taskbar_show_as_used: Some(false),
                ..Default::default()
            }
            .refreshes_tray_presentation()
        );
    }

    /// The floating bar has its own notify channel. A floating-bar-only change
    /// must not force a tray/taskbar repaint, and a dashboard-only change must
    /// not wake the floating bar.
    #[test]
    fn per_component_display_changes_notify_only_their_own_surface() {
        let float_bar_only = SettingsUpdate {
            float_bar_show_as_used: Some(false),
            ..Default::default()
        };
        assert!(float_bar_only.notifies_float_bar());
        assert!(!float_bar_only.refreshes_tray_presentation());

        let dashboard_only = SettingsUpdate {
            dashboard_show_as_used: Some(false),
            ..Default::default()
        };
        assert!(dashboard_only.refreshes_tray_presentation());
        assert!(!dashboard_only.notifies_float_bar());
    }

    /// Writing one component's preference must leave the other two alone —
    /// this is the regression the per-component split exists to prevent.
    #[test]
    fn apply_display_settings_keeps_components_independent() {
        let mut settings = Settings::default();

        SettingsUpdate {
            float_bar_show_as_used: Some(false),
            taskbar_show_as_used: Some(false),
            ..Default::default()
        }
        .apply_display_settings(&mut settings);

        assert!(!settings.float_bar_show_as_used);
        assert!(!settings.taskbar_show_as_used);
        // Everything else keeps its default.
        assert!(settings.dashboard_show_as_used);
        assert!(settings.float_bar_reset_time_relative);
        assert!(settings.dashboard_reset_time_relative);
        // The legacy globals are not written back by the new fields.
        assert!(settings.show_as_used);
        assert!(settings.reset_time_relative);
    }

    /// The reset-time toggle, from the exact JSON the settings page sends to the
    /// snapshot the cards read back.
    ///
    /// Both ends were already covered — `apply_display_settings` above writes
    /// the field, and `quotaDisplay.test.ts` proves the formatter branches on it
    /// — while the wire between them was not. That is the shape of the bug that
    /// left the taskbar entry composer inert: `SettingsUpdate` had never been
    /// told about the key, so serde dropped it in silence. `apply_to` is used
    /// here rather than `apply_display_settings` because the real command calls
    /// that, and a display field reachable only by the narrower helper would be
    /// just as dead.
    #[test]
    fn dashboard_reset_time_mode_survives_the_bridge_round_trip() {
        let patch: SettingsUpdate =
            serde_json::from_str(r#"{"dashboardResetTimeRelative":false}"#)
                .expect("camelCase toggle must deserialize");
        assert_eq!(
            patch.dashboard_reset_time_relative,
            Some(false),
            "the field must be populated, not silently dropped"
        );
        // The dashboard is the tray flyout, so its change has to reach the tray.
        assert!(patch.refreshes_tray_presentation());

        let mut settings = Settings::default();
        assert!(settings.dashboard_reset_time_relative, "default is a countdown");
        patch.apply_to(&mut settings).expect("patch applies");
        assert!(!settings.dashboard_reset_time_relative);
        // The floating bar owns a separate copy and must not follow along.
        assert!(settings.float_bar_reset_time_relative);

        // What the frontend reads back is what decides the rendered text.
        let json = serde_json::to_value(SettingsSnapshot::from(settings))
            .expect("snapshot serializes");
        assert_eq!(json["dashboardResetTimeRelative"], serde_json::json!(false));
        assert_eq!(json["floatBarResetTimeRelative"], serde_json::json!(true));
    }

    /// The same round trip for the floating bar's own pair, which is notified
    /// through a different channel: the bar does not use `useSettings`, it waits
    /// on `float-bar-config-changed`, and only `notifies_float_bar()` emits it.
    /// A field missing from that list would persist and never reach the bar.
    #[test]
    fn float_bar_display_settings_survive_the_bridge_round_trip() {
        let patch: SettingsUpdate = serde_json::from_str(
            r#"{"floatBarShowAsUsed":false,"floatBarResetTimeRelative":false}"#,
        )
        .expect("camelCase toggles must deserialize");
        assert_eq!(patch.float_bar_show_as_used, Some(false));
        assert_eq!(patch.float_bar_reset_time_relative, Some(false));
        assert!(
            patch.notifies_float_bar(),
            "without this the bar keeps rendering the old snapshot"
        );

        let mut settings = Settings::default();
        patch.apply_to(&mut settings).expect("patch applies");
        assert!(!settings.float_bar_show_as_used);
        assert!(!settings.float_bar_reset_time_relative);
        // The dashboard owns a separate copy and must not follow along.
        assert!(settings.dashboard_show_as_used);
        assert!(settings.dashboard_reset_time_relative);

        let json = serde_json::to_value(SettingsSnapshot::from(settings))
            .expect("snapshot serializes");
        assert_eq!(json["floatBarShowAsUsed"], serde_json::json!(false));
        assert_eq!(json["floatBarResetTimeRelative"], serde_json::json!(false));
    }

    #[test]
    fn ui_language_change_refreshes_tray_presentation() {
        assert!(
            SettingsUpdate {
                ui_language: Some("japanese".to_string()),
                ..Default::default()
            }
            .refreshes_tray_presentation()
        );
    }

    #[test]
    fn apply_display_settings_clamps_window_scale_percent() {
        let mut settings = Settings::default();

        SettingsUpdate {
            window_scale_percent: Some(300),
            ..Default::default()
        }
        .apply_display_settings(&mut settings);
        assert_eq!(settings.window_scale_percent, 250);

        SettingsUpdate {
            window_scale_percent: Some(50),
            ..Default::default()
        }
        .apply_display_settings(&mut settings);
        assert_eq!(settings.window_scale_percent, 100);
    }

    #[test]
    fn apply_advanced_settings_sets_taskbar_widget_enabled() {
        let mut settings = Settings::default();
        assert!(!settings.taskbar_widget_enabled);

        SettingsUpdate {
            taskbar_widget_enabled: Some(true),
            ..Default::default()
        }
        .apply_advanced_settings(&mut settings);
        assert!(settings.taskbar_widget_enabled);
    }

    #[test]
    fn apply_advanced_settings_normalizes_taskbar_widget_position() {
        let mut settings = Settings::default();
        SettingsUpdate {
            taskbar_widget_position: Some("left".to_string()),
            ..Default::default()
        }
        .apply_advanced_settings(&mut settings);
        assert_eq!(settings.taskbar_widget_position, "left");

        SettingsUpdate {
            taskbar_widget_position: Some("unexpected".to_string()),
            ..Default::default()
        }
        .apply_advanced_settings(&mut settings);
        assert_eq!(settings.taskbar_widget_position, "notification");
    }

    #[test]
    fn apply_advanced_settings_normalizes_taskbar_widget_display_options() {
        let mut settings = Settings::default();
        SettingsUpdate {
            taskbar_widget_font_weight: Some(625),
            taskbar_widget_content: Some("usage_speed".to_string()),
            taskbar_widget_font_size: Some(16),
            taskbar_widget_width: Some(220),
            taskbar_widget_text_align: Some("center".to_string()),
            ..Default::default()
        }
        .apply_advanced_settings(&mut settings);
        // 625 is a legitimate `wght` axis value now, not something to snap to a
        // named face: the DirectWrite renderer draws it distinctly.
        assert_eq!(settings.taskbar_widget_font_weight, 625);
        assert_eq!(settings.taskbar_widget_content, "usage_speed");
        assert_eq!(settings.taskbar_widget_font_size, 16);
        assert_eq!(settings.taskbar_widget_width, 220);
        assert_eq!(settings.taskbar_widget_text_align, "center");

        SettingsUpdate {
            taskbar_widget_font_weight: Some(999),
            taskbar_widget_content: Some("unexpected".to_string()),
            taskbar_widget_font_size: Some(2),
            taskbar_widget_width: Some(999),
            taskbar_widget_text_align: Some("unexpected".to_string()),
            ..Default::default()
        }
        .apply_advanced_settings(&mut settings);
        assert_eq!(settings.taskbar_widget_font_weight, 999);
        assert_eq!(settings.taskbar_widget_content, "usage");
        assert_eq!(settings.taskbar_widget_font_size, 10);
        assert_eq!(settings.taskbar_widget_width, 240);
        assert_eq!(settings.taskbar_widget_text_align, "left");
    }

    #[test]
    fn apply_display_settings_clamps_tray_scale_percent() {
        let mut settings = Settings::default();

        SettingsUpdate {
            tray_scale_percent: Some(300),
            ..Default::default()
        }
        .apply_display_settings(&mut settings);
        assert_eq!(settings.tray_scale_percent, 200);

        SettingsUpdate {
            tray_scale_percent: Some(50),
            ..Default::default()
        }
        .apply_display_settings(&mut settings);
        assert_eq!(settings.tray_scale_percent, 100);
    }
}
