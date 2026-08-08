use super::*;

// ── Bridge snapshot types ────────────────────────────────────────────

/// A taskbar entry as the WebView sees it.
///
/// `codexbar::settings::TaskbarEntry` is the ON-DISK shape and stays snake_case
/// like the rest of the settings file. `#[serde(rename_all = "camelCase")]` on
/// the snapshot struct renames only that struct's own fields, never a nested
/// type's, so sending the settings type straight through shipped
/// `{"provider_id": ...}` to a frontend reading `providerId` — which is why the
/// provider dropdown rendered blank. Every other nested bridge type carries its
/// own rename for exactly this reason; this one was missing it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskbarEntryBridge {
    pub provider_id: String,
    pub window: String,
}

impl From<&codexbar::settings::TaskbarEntry> for TaskbarEntryBridge {
    fn from(entry: &codexbar::settings::TaskbarEntry) -> Self {
        Self {
            provider_id: entry.provider_id.clone(),
            window: entry.window.clone(),
        }
    }
}

impl From<&TaskbarEntryBridge> for codexbar::settings::TaskbarEntry {
    fn from(entry: &TaskbarEntryBridge) -> Self {
        Self {
            provider_id: entry.provider_id.clone(),
            window: entry.window.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RateWindowSnapshot {
    pub used_percent: f64,
    pub remaining_percent: f64,
    /// Which cycle this window is, decided once here so no surface has to work
    /// it out from `window_minutes` for itself. See [`crate::quota_cycle`] for
    /// why that matters — three surfaces used to derive it independently and
    /// disagreed. `None` for a window that is not a dated cycle, or whose cycle
    /// fits no named band; callers then render it without a cycle word.
    pub kind: Option<&'static str>,
    pub window_minutes: Option<u32>,
    pub resets_at: Option<String>,
    pub reset_description: Option<String>,
    pub is_exhausted: bool,
    pub is_informational: bool,
    pub reserve_percent: Option<f64>,
    pub reserve_description: Option<String>,
    pub reserve_will_last_to_reset: bool,
    pub reserve_eta_seconds: Option<f64>,
}

impl RateWindowSnapshot {
    /// Build a snapshot for a window whose slot the provider does not name.
    pub(super) fn from_rate_window(rw: &RateWindow) -> Self {
        Self::from_rate_window_labelled(rw, None)
    }

    /// Build a snapshot, classifying it with the provider's own name for the
    /// slot it sits in.
    ///
    /// The label is the second identification source and for some providers the
    /// only one: a percentage published without a declared length is nameless
    /// without it. Passing it here rather than at the call sites is what lets
    /// every consumer read `kind` instead of re-deriving it.
    pub(super) fn from_rate_window_labelled(rw: &RateWindow, label: Option<&str>) -> Self {
        Self {
            used_percent: rw.used_percent,
            remaining_percent: rw.remaining_percent(),
            kind: crate::quota_cycle::classify(rw.window_minutes, label),
            window_minutes: rw.window_minutes,
            resets_at: rw.resets_at.map(|dt| dt.to_rfc3339()),
            reset_description: rw.reset_description.clone(),
            is_exhausted: rw.is_exhausted(),
            is_informational: rw.is_informational,
            reserve_percent: None,
            reserve_description: None,
            reserve_will_last_to_reset: false,
            reserve_eta_seconds: None,
        }
    }

    /// Enrich with raw reserve info derived from pace analysis.
    /// delta_percent = actual - expected; negative means ahead (in reserve).
    /// Only meaningful for longer windows (weekly); skip if reserve rounds to 0.
    /// Localization happens at render time so cached snapshots stay language-neutral.
    fn with_pace_reserve(mut self, pace: &codexbar::core::UsagePace) -> Self {
        let reserve = pace.delta_percent.abs().round();
        if pace.delta_percent < 0.0 && reserve > 0.0 {
            self.reserve_percent = Some(reserve);
            self.reserve_will_last_to_reset = pace.will_last_to_reset;
            self.reserve_eta_seconds = pace.eta_seconds;
        }
        self
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostSnapshotBridge {
    pub used: f64,
    pub limit: Option<f64>,
    pub remaining: Option<f64>,
    pub currency_code: String,
    pub period: String,
    pub resets_at: Option<String>,
    pub formatted_used: String,
    pub formatted_limit: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedRateWindowSnapshot {
    pub id: String,
    pub title: String,
    pub window: RateWindowSnapshot,
}

/// Pace prediction snapshot for tray/bridge display.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaceSnapshot {
    pub stage: &'static str,
    pub delta_percent: f64,
    pub will_last_to_reset: bool,
    pub eta_seconds: Option<f64>,
    pub expected_used_percent: f64,
    pub actual_used_percent: f64,
    /// How many times the current burn rate could grow and still reach reset.
    /// `None` when the ratio is meaningless, so the UI can never show a
    /// fabricated headroom figure.
    pub speed_multiplier_to_reset: Option<f64>,
}

/// A frontend-friendly snapshot of one provider's usage data.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageSnapshot {
    pub provider_id: String,
    pub display_name: String,
    pub primary: RateWindowSnapshot,
    pub primary_label: Option<String>,
    pub secondary: Option<RateWindowSnapshot>,
    pub secondary_label: Option<String>,
    pub model_specific: Option<RateWindowSnapshot>,
    pub tertiary: Option<RateWindowSnapshot>,
    pub extra_rate_windows: Vec<NamedRateWindowSnapshot>,
    pub cost: Option<CostSnapshotBridge>,
    pub plan_name: Option<String>,
    pub account_email: Option<String>,
    pub source_label: String,
    pub updated_at: String,
    pub error: Option<String>,
    pub pace: Option<PaceSnapshot>,
    pub account_organization: Option<String>,
    pub tray_status_label: Option<String>,
    pub fetch_duration_ms: Option<u128>,
    pub wayfinder_usage: Option<codexbar::core::WayfinderUsageSnapshot>,
}

pub(crate) fn pace_stage_str(stage: codexbar::core::PaceStage) -> &'static str {
    use codexbar::core::PaceStage;
    match stage {
        PaceStage::OnTrack => "on_track",
        PaceStage::SlightlyAhead => "slightly_ahead",
        PaceStage::Ahead => "ahead",
        PaceStage::FarAhead => "far_ahead",
        PaceStage::SlightlyBehind => "slightly_behind",
        PaceStage::Behind => "behind",
        PaceStage::FarBehind => "far_behind",
    }
}

impl ProviderUsageSnapshot {
    pub(super) fn from_fetch_result(
        id: ProviderId,
        metadata: &ProviderMetadata,
        result: &ProviderFetchResult,
    ) -> Self {
        let usage = &result.usage;

        // The pace block answers "how far through the WEEKLY budget am I,
        // versus how far through the week?", so it must read the weekly
        // window. Providers disagree on which slot carries it: Codex reports
        // the weekly quota as `primary`, while Claude's `primary` is its
        // 5-hour session window and the weekly one lands in `secondary`.
        // Selecting by window LENGTH rather than by slot keeps this on the
        // weekly window for every provider — reading `primary` unconditionally
        // made Claude report its 5-hour usage as the weekly pace. A window
        // that declares no length is still treated as weekly, preserving the
        // previous `default_window_minutes` behaviour.
        const WEEKLY_WINDOW_MINUTES: u32 = 7 * 24 * 60;
        let weekly_window = [Some(&usage.primary), usage.secondary.as_ref()]
            .into_iter()
            .flatten()
            .find(|w| w.window_minutes.is_none_or(|m| m >= WEEKLY_WINDOW_MINUTES));

        let weekly_pace = weekly_window
            .and_then(|w| codexbar::core::UsagePace::weekly(w, None, WEEKLY_WINDOW_MINUTES));

        let pace = weekly_pace.as_ref().map(|p| PaceSnapshot {
            stage: pace_stage_str(p.stage),
            delta_percent: p.delta_percent,
            will_last_to_reset: p.will_last_to_reset,
            eta_seconds: p.eta_seconds,
            expected_used_percent: p.expected_used_percent,
            actual_used_percent: p.actual_used_percent,
            speed_multiplier_to_reset: p.speed_multiplier_to_reset,
        });

        // Compute pace for secondary window (weekly) to derive reserve info
        let secondary_pace = usage
            .secondary
            .as_ref()
            .and_then(|sw| codexbar::core::UsagePace::weekly(sw, None, 10080));

        // The slot labels are the provider's own names for these two windows,
        // and they are the only identification a provider that publishes no
        // length has. They are already sent as `primary_label`/`secondary_label`
        // below; classifying with them here is what lets every surface read the
        // answer instead of recomputing it.
        let primary_snap =
            RateWindowSnapshot::from_rate_window_labelled(&usage.primary, Some(metadata.session_label));

        let secondary_snap = usage.secondary.as_ref().map(|sw| {
            let mut s =
                RateWindowSnapshot::from_rate_window_labelled(sw, Some(metadata.weekly_label));
            if let Some(ref p) = secondary_pace {
                s = s.with_pace_reserve(p);
            }
            s
        });

        Self {
            provider_id: id.cli_name().to_string(),
            display_name: id.display_name().to_string(),
            primary: primary_snap,
            primary_label: Some(metadata.session_label.to_string()),
            secondary: secondary_snap,
            secondary_label: usage
                .secondary
                .as_ref()
                .map(|_| metadata.weekly_label.to_string()),
            model_specific: usage
                .model_specific
                .as_ref()
                .map(RateWindowSnapshot::from_rate_window),
            tertiary: usage
                .tertiary
                .as_ref()
                .map(RateWindowSnapshot::from_rate_window),
            extra_rate_windows: usage
                .extra_rate_windows
                .iter()
                .map(|extra| NamedRateWindowSnapshot {
                    id: extra.id.clone(),
                    title: extra.title.clone(),
                    window: RateWindowSnapshot::from_rate_window_labelled(
                        &extra.window,
                        Some(extra.title.as_str()),
                    ),
                })
                .collect(),
            cost: result.cost.as_ref().map(|c| CostSnapshotBridge {
                used: c.used,
                limit: c.limit,
                remaining: c.remaining(),
                currency_code: c.currency_code.clone(),
                period: c.period.clone(),
                resets_at: c.resets_at.map(|dt| dt.to_rfc3339()),
                formatted_used: c.format_used(),
                formatted_limit: c.format_limit(),
            }),
            plan_name: usage.login_method.clone(),
            account_email: usage.account_email.clone(),
            source_label: result.source_label.clone(),
            updated_at: usage.updated_at.to_rfc3339(),
            error: None,
            pace,
            account_organization: usage.account_organization.clone(),
            tray_status_label: None,
            fetch_duration_ms: None,
            wayfinder_usage: result.wayfinder_usage.clone(),
        }
    }

    pub(super) fn from_error(id: ProviderId, metadata: &ProviderMetadata, error: String) -> Self {
        let error = friendly_provider_error(id, &error);
        Self {
            provider_id: id.cli_name().to_string(),
            display_name: id.display_name().to_string(),
            primary: RateWindowSnapshot {
                used_percent: 0.0,
                remaining_percent: 100.0,
                // A failed fetch published no window, so there is no cycle to
                // name. The error is what this snapshot carries.
                kind: None,
                window_minutes: None,
                resets_at: None,
                reset_description: None,
                is_exhausted: false,
                is_informational: false,
                reserve_percent: None,
                reserve_description: None,
                reserve_will_last_to_reset: false,
                reserve_eta_seconds: None,
            },
            primary_label: Some(metadata.session_label.to_string()),
            secondary: None,
            secondary_label: None,
            model_specific: None,
            tertiary: None,
            extra_rate_windows: Vec::new(),
            cost: None,
            plan_name: None,
            account_email: None,
            source_label: String::new(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            error: Some(error),
            pace: None,
            account_organization: None,
            tray_status_label: None,
            fetch_duration_ms: None,
            wayfinder_usage: None,
        }
    }
}

/// Build a compact tray status label from a raw snapshot using the current language.
/// Localization is done at render time so cached snapshots stay language-neutral.
pub(crate) fn compact_tray_status_label(
    window: &RateWindowSnapshot,
    lang: codexbar::settings::Language,
    relative: bool,
) -> String {
    let pct = format!("{:.0}%", window.used_percent);
    if let Some(reset) = compact_reset_description(window, lang, relative) {
        format!("{pct} • {reset}")
    } else {
        pct
    }
}

fn compact_reset_description(
    window: &RateWindowSnapshot,
    lang: codexbar::settings::Language,
    relative: bool,
) -> Option<String> {
    if let Some(ref resets_at) = window.resets_at {
        let dt = chrono::DateTime::parse_from_rfc3339(resets_at)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))?;
        if !relative {
            return Some(format_absolute_reset_moment(dt));
        }
        return Some(format_compact_reset_countdown(dt, lang));
    }

    window
        .reset_description
        .as_deref()
        .map(|desc| normalize_reset_description(desc, lang))
        .filter(|desc| !desc.is_empty())
}

/// The reset moment itself, in the machine's own timezone.
///
/// Deliberately numeric and language-free: this string shares a very narrow row
/// with a provider name and a percentage, and a localized month name would cost
/// more width than it earns. Same-day resets drop the date entirely, which is
/// the common case for a five-hour window.
fn format_absolute_reset_moment(resets_at: chrono::DateTime<chrono::Utc>) -> String {
    use chrono::Local;
    let local = resets_at.with_timezone(&Local);
    let now = Local::now();
    if local.date_naive() == now.date_naive() {
        local.format("%H:%M").to_string()
    } else {
        local.format("%m-%d %H:%M").to_string()
    }
}

fn format_compact_reset_countdown(
    resets_at: chrono::DateTime<chrono::Utc>,
    lang: codexbar::settings::Language,
) -> String {
    let now = chrono::Utc::now();
    if resets_at <= now {
        return locale::get_text(lang, locale::LocaleKey::ResetInProgress);
    }

    let total_minutes = (resets_at - now).num_minutes().max(0);
    let days = total_minutes / 1440;
    let hours = (total_minutes % 1440) / 60;
    let minutes = total_minutes % 60;

    if days > 0 {
        locale::format_locale(
            lang,
            locale::LocaleKey::ResetsInDaysHours,
            &[&days.to_string(), &hours.to_string()],
        )
    } else {
        locale::format_locale(
            lang,
            locale::LocaleKey::ResetsInHoursMinutes,
            &[&hours.to_string(), &format!("{minutes:02}")],
        )
    }
}

fn normalize_reset_description(desc: &str, lang: codexbar::settings::Language) -> String {
    let trimmed = desc.trim();
    let lower = trimmed.to_ascii_lowercase();
    let prefix_len = ["resets in ", "reset in ", "in "]
        .iter()
        .find(|&&p| lower.starts_with(p))
        .map(|p| p.len())
        .unwrap_or(0);
    let body = trimmed[prefix_len..].trim_start();
    format!(
        "{} {body}",
        locale::get_text(lang, locale::LocaleKey::ResetsInShort)
    )
}

pub(crate) fn friendly_provider_error(id: ProviderId, error: &str) -> String {
    if id != ProviderId::Claude {
        return error.to_string();
    }

    let trimmed = error.trim();
    let lower = trimmed.to_lowercase();

    if lower.contains("swift.cancellationerror")
        || lower.contains("the operation couldn't be completed")
        || lower.contains("the operation could not be completed")
    {
        return "Claude usage fetch was cancelled before usage data was returned. Refresh Claude, or re-authenticate with Claude Code and try again.".to_string();
    }

    if lower.contains("claude oauth credentials not found") {
        return "Claude sign-in was not found. Run `claude` once to authenticate, then refresh Claude in Win-CodexBar.".to_string();
    }

    if lower.contains("oauth token expired") || lower.contains("token invalid or expired") {
        return "Claude sign-in expired. Run `claude` to refresh your Claude Code login, then refresh Claude in Win-CodexBar.".to_string();
    }

    if trimmed == "Authentication required" {
        return "Claude needs sign-in before Win-CodexBar can read usage. Run `claude` once, or add Claude cookies in Provider settings.".to_string();
    }

    if lower.starts_with("claude usage failed from all configured sources.") {
        return trimmed
            .replace(
                "OAuth: OAuth error: Claude OAuth credentials not found. Run `claude` to authenticate.",
                "OAuth: sign-in not found",
            )
            .replace(
                "Web: No cookies available for web API",
                "Web: no Claude cookies available",
            )
            .replace(
                "CLI: Provider not installed:",
                "CLI: not installed:",
            );
    }

    trimmed.to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapState {
    pub(crate) contract_version: &'static str,
    pub(crate) providers: Vec<ProviderCatalogEntry>,
    pub(crate) settings: SettingsSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentSurfaceState {
    pub mode: String,
    pub target: SurfaceTarget,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCatalogEntry {
    pub(crate) id: String,
    pub(crate) display_name: String,
    pub(crate) cookie_domain: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    enabled_providers: Vec<String>,
    provider_order: Vec<String>,
    refresh_interval_secs: u64,
    refresh_all_providers_on_menu_open: bool,
    start_at_login: bool,
    start_minimized: bool,
    show_notifications: bool,
    sound_enabled: bool,
    sound_volume: u8,
    high_usage_threshold: f64,
    critical_usage_threshold: f64,
    tray_icon_mode: &'static str,
    switcher_shows_icons: bool,
    menu_bar_shows_highest_usage: bool,
    show_as_used: bool,
    show_all_token_accounts_in_menu: bool,
    enable_animations: bool,
    reset_time_relative: bool,
    menu_bar_display_mode: String,
    output_speed_enabled: bool,
    local_usage_period: String,
    hide_personal_info: bool,
    update_channel: &'static str,
    auto_download_updates: bool,
    install_updates_on_quit: bool,
    global_shortcut: String,
    codex_custom_sessions_dirs: Vec<String>,
    ui_language: &'static str,
    theme: &'static str,
    window_scale_percent: u16,
    tray_scale_percent: u16,
    claude_avoid_keychain_prompts: bool,
    disable_keychain_access: bool,
    provider_metrics: std::collections::HashMap<String, &'static str>,
    float_bar_enabled: bool,
    float_bar_opacity: u8,
    float_bar_scale: u8,
    float_bar_orientation: String,
    float_bar_style: String,
    float_bar_click_through: bool,
    float_bar_provider_ids: Vec<String>,
    float_bar_dark_text: bool,
    float_bar_show_reset_inline: bool,
    float_bar_reset_windows: Vec<String>,
    float_bar_show_cost: bool,
    taskbar_widget_enabled: bool,
    taskbar_widget_position: String,
    taskbar_widget_font_weight: u16,
    menu_font_weight: u16,
    menu_font_family: String,
    menu_font_size: u8,
    taskbar_widget_content: String,
    taskbar_widget_entries: Vec<TaskbarEntryBridge>,
    taskbar_widget_font_family: String,
    taskbar_widget_font_size: u8,
    taskbar_widget_width: u16,
    taskbar_widget_text_align: String,
    // Per-component quota presentation. `show_as_used` / `reset_time_relative`
    // above are legacy migration sources and are no longer read by any surface.
    float_bar_show_as_used: bool,
    float_bar_reset_time_relative: bool,
    dashboard_show_as_used: bool,
    dashboard_reset_time_relative: bool,
    dashboard_provider_ids: Vec<String>,
    taskbar_show_as_used: bool,
    taskbar_reset_time_relative: bool,
    taskbar_context_menu_actions: Vec<String>,
    taskbar_tooltip_entries: Vec<TaskbarEntryBridge>,
}

#[tauri::command]
pub fn get_bootstrap_state() -> BootstrapState {
    let settings = Settings::load();
    BootstrapState {
        contract_version: "v1",
        providers: provider_catalog_for(&settings),
        settings: SettingsSnapshot::from(settings),
    }
}

#[tauri::command]
pub fn get_provider_catalog() -> Vec<ProviderCatalogEntry> {
    provider_catalog_for(&Settings::load())
}

#[tauri::command]
pub fn get_settings_snapshot() -> SettingsSnapshot {
    SettingsSnapshot::from(Settings::load())
}

impl From<Settings> for SettingsSnapshot {
    fn from(settings: Settings) -> Self {
        let avoid_keychain_prompts = settings.claude_avoid_keychain_prompts();

        let provider_order = settings.provider_display_order_names();
        let enabled_providers = provider_order
            .iter()
            .filter(|provider_id| settings.enabled_providers.contains(*provider_id))
            .cloned()
            .collect();

        let provider_metrics = settings
            .provider_metrics
            .into_iter()
            .map(|(k, v)| (k, metric_preference_label(v)))
            .collect();

        Self {
            enabled_providers,
            provider_order,
            refresh_interval_secs: settings.refresh_interval_secs,
            refresh_all_providers_on_menu_open: settings.refresh_all_providers_on_menu_open,
            start_at_login: settings.start_at_login,
            start_minimized: settings.start_minimized,
            show_notifications: settings.show_notifications,
            sound_enabled: settings.sound_enabled,
            sound_volume: settings.sound_volume,
            high_usage_threshold: settings.high_usage_threshold,
            critical_usage_threshold: settings.critical_usage_threshold,
            tray_icon_mode: tray_icon_mode_label(settings.tray_icon_mode),
            switcher_shows_icons: settings.switcher_shows_icons,
            menu_bar_shows_highest_usage: settings.menu_bar_shows_highest_usage,
            show_as_used: settings.show_as_used,
            show_all_token_accounts_in_menu: settings.show_all_token_accounts_in_menu,
            enable_animations: settings.enable_animations,
            reset_time_relative: settings.reset_time_relative,
            menu_bar_display_mode: settings.menu_bar_display_mode,
            output_speed_enabled: settings.output_speed_enabled,
            local_usage_period: settings.local_usage_period,
            hide_personal_info: settings.hide_personal_info,
            update_channel: update_channel_label(settings.update_channel),
            auto_download_updates: settings.auto_download_updates,
            install_updates_on_quit: settings.install_updates_on_quit,
            global_shortcut: settings.global_shortcut,
            codex_custom_sessions_dirs: settings.codex_custom_sessions_dirs,
            ui_language: language_label(settings.ui_language),
            theme: theme_label(settings.theme),
            window_scale_percent: settings.window_scale_percent,
            tray_scale_percent: settings.tray_scale_percent,
            claude_avoid_keychain_prompts: avoid_keychain_prompts,
            disable_keychain_access: settings.disable_keychain_access,
            provider_metrics,
            float_bar_enabled: settings.float_bar_enabled,
            float_bar_opacity: settings.float_bar_opacity,
            float_bar_scale: settings.float_bar_scale,
            float_bar_orientation: settings.float_bar_orientation,
            float_bar_style: settings.float_bar_style,
            float_bar_click_through: settings.float_bar_click_through,
            float_bar_provider_ids: settings.float_bar_provider_ids,
            float_bar_dark_text: settings.float_bar_dark_text,
            float_bar_show_reset_inline: settings.float_bar_show_reset_inline,
            float_bar_reset_windows: settings.float_bar_reset_windows.clone(),
            float_bar_show_cost: settings.float_bar_show_cost,
            taskbar_widget_enabled: settings.taskbar_widget_enabled,
            taskbar_widget_position: settings.taskbar_widget_position,
            taskbar_widget_font_weight: settings.taskbar_widget_font_weight,
            menu_font_weight: settings.menu_font_weight,
            menu_font_family: settings.menu_font_family.clone(),
            menu_font_size: settings.menu_font_size,
            taskbar_widget_content: settings.taskbar_widget_content,
            taskbar_widget_entries: settings
                .taskbar_widget_entries
                .iter()
                .map(TaskbarEntryBridge::from)
                .collect(),
            taskbar_widget_font_family: settings.taskbar_widget_font_family.clone(),
            taskbar_widget_font_size: settings.taskbar_widget_font_size,
            taskbar_widget_width: settings.taskbar_widget_width,
            taskbar_widget_text_align: settings.taskbar_widget_text_align,
            float_bar_show_as_used: settings.float_bar_show_as_used,
            float_bar_reset_time_relative: settings.float_bar_reset_time_relative,
            dashboard_show_as_used: settings.dashboard_show_as_used,
            dashboard_reset_time_relative: settings.dashboard_reset_time_relative,
            dashboard_provider_ids: settings.dashboard_provider_ids.clone(),
            taskbar_show_as_used: settings.taskbar_show_as_used,
            taskbar_reset_time_relative: settings.taskbar_reset_time_relative,
            taskbar_context_menu_actions: settings.taskbar_context_menu_actions.clone(),
            taskbar_tooltip_entries: settings
                .taskbar_tooltip_entries
                .iter()
                .map(TaskbarEntryBridge::from)
                .collect(),
        }
    }
}

pub(crate) fn provider_catalog_for(settings: &Settings) -> Vec<ProviderCatalogEntry> {
    settings
        .provider_display_order()
        .into_iter()
        // The MiMo balance endpoint is now owned by the cookie-backed MiMo
        // provider. Keep the legacy API-key provider in the core for backward
        // compatibility, but do not expose a duplicate card in this product.
        .filter(|provider| *provider != ProviderId::MiMoApi)
        .map(|provider| ProviderCatalogEntry {
            id: provider.cli_name().to_string(),
            display_name: provider.display_name().to_string(),
            cookie_domain: provider.cookie_domain().map(ToString::to_string),
        })
        .collect()
}

fn tray_icon_mode_label(mode: TrayIconMode) -> &'static str {
    match mode {
        TrayIconMode::Single => "single",
        TrayIconMode::PerProvider => "perProvider",
    }
}

pub(super) fn update_channel_label(channel: UpdateChannel) -> &'static str {
    match channel {
        UpdateChannel::Stable => "stable",
        UpdateChannel::Beta => "beta",
    }
}

pub(super) fn language_label(language: Language) -> &'static str {
    language.label()
}

fn theme_label(theme: ThemePreference) -> &'static str {
    match theme {
        ThemePreference::Auto => "auto",
        ThemePreference::Light => "light",
        ThemePreference::Dark => "dark",
    }
}

pub(super) fn parse_theme(s: &str) -> Option<ThemePreference> {
    match s {
        "auto" => Some(ThemePreference::Auto),
        "light" => Some(ThemePreference::Light),
        "dark" => Some(ThemePreference::Dark),
        _ => None,
    }
}

fn metric_preference_label(pref: MetricPreference) -> &'static str {
    match pref {
        MetricPreference::Automatic => "automatic",
        MetricPreference::Session => "session",
        MetricPreference::Weekly => "weekly",
        MetricPreference::Model => "model",
        MetricPreference::Tertiary => "tertiary",
        MetricPreference::Credits => "credits",
        MetricPreference::ExtraUsage => "extraUsage",
        MetricPreference::Average => "average",
    }
}

pub(super) fn parse_metric_preference(s: &str) -> Option<MetricPreference> {
    match s {
        "automatic" => Some(MetricPreference::Automatic),
        "session" => Some(MetricPreference::Session),
        "weekly" => Some(MetricPreference::Weekly),
        "model" => Some(MetricPreference::Model),
        "tertiary" => Some(MetricPreference::Tertiary),
        "credits" => Some(MetricPreference::Credits),
        "extraUsage" | "extrausage" => Some(MetricPreference::ExtraUsage),
        "average" => Some(MetricPreference::Average),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot_window_with(
        used_percent: f64,
        window_minutes: Option<u32>,
        resets_at: Option<chrono::DateTime<chrono::Utc>>,
        reset_description: Option<String>,
    ) -> RateWindowSnapshot {
        RateWindowSnapshot {
            used_percent,
            remaining_percent: 100.0 - used_percent,
            kind: crate::quota_cycle::classify(window_minutes, None),
            window_minutes,
            resets_at: resets_at.map(|dt| dt.to_rfc3339()),
            reset_description,
            is_exhausted: false,
            is_informational: false,
            reserve_percent: None,
            reserve_description: None,
            reserve_will_last_to_reset: false,
            reserve_eta_seconds: None,
        }
    }

    #[test]
    fn tray_status_prefers_relative_reset_countdown() {
        let window = snapshot_window_with(
            13.0,
            Some(300),
            Some(chrono::Utc::now() + chrono::Duration::minutes(125)),
            Some("Jun 10 at 3:00PM".to_string()),
        );

        let label = compact_tray_status_label(&window, Language::English, true);

        assert!(label.starts_with("13% • Resets in 2h "));
        assert!(label.ends_with('m'));
        assert!(!label.contains("Jun 10"));
    }

    #[test]
    fn tray_status_normalizes_fallback_reset_description() {
        let window = snapshot_window_with(8.0, Some(300), None, Some("2h 05m".to_string()));

        assert_eq!(
            compact_tray_status_label(&window, Language::English, true),
            "8% • Resets in 2h 05m"
        );
    }

    #[test]
    fn japanese_tray_status_label_has_no_english_reset_text() {
        use codexbar::settings::Language;

        let window = snapshot_window_with(
            13.0,
            Some(300),
            Some(chrono::Utc::now() + chrono::Duration::minutes(125)),
            None,
        );

        let label = compact_tray_status_label(&window, Language::Japanese, true);

        assert!(label.contains("リセットまで"), "{label}");
        assert!(!label.to_ascii_lowercase().contains("resets in"), "{label}");
        assert!(label.contains("13%"), "{label}");
    }

    #[test]
    fn japanese_tray_status_strips_english_fallback_reset_prefix() {
        use codexbar::settings::Language;

        let window =
            snapshot_window_with(8.0, Some(300), None, Some("Resets in 2h 05m".to_string()));

        let label = compact_tray_status_label(&window, Language::Japanese, true);

        assert!(label.contains("リセットまで"), "{label}");
        assert!(!label.to_ascii_lowercase().contains("resets in"), "{label}");
        assert!(label.contains("2h 05m"), "{label}");
    }

    #[test]
    fn tray_status_label_relocalizes_without_refetch() {
        let window = snapshot_window_with(
            13.0,
            Some(300),
            Some(chrono::Utc::now() + chrono::Duration::minutes(125)),
            None,
        );

        let english = compact_tray_status_label(&window, Language::English, true);
        let japanese = compact_tray_status_label(&window, Language::Japanese, true);

        assert!(english.contains("Resets in"), "{english}");
        assert!(japanese.contains("リセットまで"), "{japanese}");
        assert!(
            !japanese.to_ascii_lowercase().contains("resets in"),
            "{japanese}"
        );
    }
}
