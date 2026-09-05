//! System tray icon setup: left-click opens the tray panel, right-click opens
//! the context menu.
//!
//! On Windows that menu is the **self-drawn** one in [`crate::taskbar_menu`],
//! the same popup the taskbar strip shows, owned by [`crate::menu_host`]'s
//! message-only window (M3). Tauri offers no owner-draw hook for a native
//! `#32768` menu, which is why the strip stopped using one; the tray icon now
//! follows. Every other platform keeps its retained native menu.

use std::sync::Mutex;

// Native-menu construction only. Windows builds no retained tray menu since M3
// — it shows the self-drawn popup from `taskbar_menu` — so these are dead there.
// `test` is in the list because the tests below build catalog fixtures on every
// platform, and `cargo check` alone does not compile them: gating this on
// `not(windows)` alone passes a check and fails the test build.
#[cfg(any(not(windows), test))]
use crate::commands::ProviderCatalogEntry;
use codexbar::core::ProviderId;
use codexbar::settings::{MetricPreference, Settings, TrayIconMode};
use tauri::image::Image;
#[cfg(not(windows))]
use tauri::menu::{CheckMenuItemBuilder, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

use codexbar::tray::render_bar_icon_rgba;

use crate::shell;
use crate::state::{AppState, TrayAnchor};
use crate::surface::SurfaceMode;
use crate::surface_target::SurfaceTarget;
use crate::tray_menu::{TrayMenuEntry, build_tray_menu_with};

#[derive(Debug, Clone, Copy)]
struct MonitorScaleInfo {
    physical_x: i32,
    physical_y: i32,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
}

impl MonitorScaleInfo {
    fn from_monitor(monitor: &tauri::Monitor) -> Self {
        let scale_factor = monitor.scale_factor();
        let safe_scale = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        let position = monitor.position();
        let size = monitor.size();

        Self {
            physical_x: position.x,
            physical_y: position.y,
            physical_width: size.width,
            physical_height: size.height,
            scale_factor: safe_scale,
        }
    }
}

fn scale_factor_for_physical_point(x: f64, y: f64, monitors: &[MonitorScaleInfo]) -> Option<f64> {
    monitors
        .iter()
        .find(|monitor| {
            x >= monitor.physical_x as f64
                && x < (monitor.physical_x + monitor.physical_width as i32) as f64
                && y >= monitor.physical_y as f64
                && y < (monitor.physical_y + monitor.physical_height as i32) as f64
        })
        .map(|monitor| monitor.scale_factor)
}

fn logical_to_physical_anchor(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
) -> TrayAnchor {
    let safe_scale = if scale_factor.is_finite() && scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };

    TrayAnchor {
        x: (x * safe_scale).round() as i32,
        y: (y * safe_scale).round() as i32,
        width: ((width * safe_scale).round().max(1.0)) as u32,
        height: ((height * safe_scale).round().max(1.0)) as u32,
    }
}

fn resolve_tray_anchor(
    rect: &tauri::Rect,
    click_position: tauri::PhysicalPosition<f64>,
    monitors: &[MonitorScaleInfo],
) -> Option<TrayAnchor> {
    let click_scale = scale_factor_for_physical_point(click_position.x, click_position.y, monitors);

    match (rect.position, rect.size) {
        (tauri::Position::Physical(position), tauri::Size::Physical(size)) => Some(TrayAnchor {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        }),
        (tauri::Position::Logical(position), tauri::Size::Logical(size)) => {
            click_scale.map(|scale| {
                logical_to_physical_anchor(position.x, position.y, size.width, size.height, scale)
            })
        }
        (tauri::Position::Physical(position), tauri::Size::Logical(size)) => {
            click_scale.map(|scale| TrayAnchor {
                x: position.x,
                y: position.y,
                width: ((size.width * scale).round().max(1.0)) as u32,
                height: ((size.height * scale).round().max(1.0)) as u32,
            })
        }
        (tauri::Position::Logical(position), tauri::Size::Physical(size)) => {
            click_scale.map(|scale| TrayAnchor {
                x: (position.x * scale).round() as i32,
                y: (position.y * scale).round() as i32,
                width: size.width,
                height: size.height,
            })
        }
    }
}

/// Only the non-Windows tray uses a retained native menu; Windows shows the
/// self-drawn popup from `taskbar_menu` instead (M3).
#[cfg(not(windows))]
fn build_native_tray_menu(
    app: &AppHandle,
    providers: &[ProviderCatalogEntry],
    status_labels: &[(String, String)],
) -> tauri::Result<Menu<tauri::Wry>> {
    let settings = Settings::load();
    let enabled = settings.enabled_providers.clone();
    let spec = build_tray_menu_with(
        providers,
        status_labels,
        &enabled,
        settings.float_bar_enabled,
        settings.ui_language,
    );
    let entries = spec
        .iter()
        .map(|entry| build_native_menu_entry(app, entry))
        .collect::<tauri::Result<Vec<_>>>()?;
    let item_refs = entries
        .iter()
        .map(NativeMenuEntry::as_item)
        .collect::<Vec<_>>();

    Menu::with_items(app, &item_refs)
}

/// Broadcast to the tray-panel flyout window so it can select the
/// deep-linked provider. The flyout's frontend listener is wired
/// separately (TrayPanel owns the selection state).
const FLYOUT_SELECT_PROVIDER_EVENT: &str = "flyout-select-provider";

fn resolve_menu_target(id: &str) -> Option<shell::ShellTransitionRequest> {
    match id {
        // NOTE: "pop_out" ("Open Tray Panel") is NOT handled here — it opens
        // the dedicated flyout window (MenuAction::OpenFlyout in
        // resolve_menu_action below), not a `shell::ShellTransitionRequest`
        // against the `main`-window surface-mode machine. `SurfaceMode::TrayPanel`
        // remains as a data key (geometry-key / window_properties source /
        // panel-size reference) but `main` no longer transitions into it.
        //
        // Provider deep links: the internal PopOut window was removed, so a
        // provider target is hosted by the tray panel (mode TrayPanel). The
        // dispatch in `handle_menu_event` recognizes that host mode and opens
        // the dedicated flyout window, which is the only surface that can
        // show a provider today.
        _ if id.starts_with("provider:") => Some(shell::ShellTransitionRequest {
            mode: SurfaceMode::TrayPanel,
            target: SurfaceTarget::parse(id)?,
            position: None,
        }),
        _ => None,
    }
}

enum MenuAction {
    Transition(shell::ShellTransitionRequest),
    /// Open Settings/About in a detached window.
    OpenSettings(String),
    /// Open (or focus) the dedicated flyout ("Open Tray Panel") window.
    OpenFlyout,
    Refresh,
    /// Toggle the enabled/disabled state of the provider with the given CLI name.
    ToggleProvider(String),
    /// Toggle the floating bar window on/off.
    ToggleFloatBar,
    Quit,
}

fn resolve_menu_action(id: &str) -> Option<MenuAction> {
    match id {
        "refresh" => Some(MenuAction::Refresh),
        "quit" => Some(MenuAction::Quit),
        "settings" => Some(MenuAction::OpenSettings("general".into())),
        "about" => Some(MenuAction::OpenSettings("about".into())),
        "toggle_float_bar" => Some(MenuAction::ToggleFloatBar),
        // Legacy id from the removed PopOut dashboard — now the tray flyout.
        "pop_out" => Some(MenuAction::OpenFlyout),
        _ if id.starts_with("toggle_provider:") => {
            let provider_id = id["toggle_provider:".len()..].to_string();
            Some(MenuAction::ToggleProvider(provider_id))
        }
        _ => resolve_menu_target(id).map(MenuAction::Transition),
    }
}

/// Store the tray icon bounds from a click event into shared state.
fn store_anchor(app: &AppHandle, rect: &tauri::Rect, click_position: tauri::PhysicalPosition<f64>) {
    let monitors = app
        .get_webview_window("main")
        .and_then(|window| window.available_monitors().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|monitor| MonitorScaleInfo::from_monitor(&monitor))
        .collect::<Vec<_>>();

    let Some(anchor) = resolve_tray_anchor(rect, click_position, &monitors) else {
        return;
    };

    // A poisoned lock means some other thread panicked while holding the state.
    // Losing this anchor costs the next tray panel its click position — it falls
    // back to the default placement. Taking the whole app down over that would
    // be the worse trade, and the panic that poisoned the lock has already been
    // reported by whoever caused it.
    if let Some(st) = app.try_state::<Mutex<AppState>>()
        && let Ok(mut guard) = st.lock()
    {
        guard.tray_anchor = Some(anchor);
    }
}

/// Initialise the system tray icon, context menu, and event handlers.
///
/// - **Left-click** toggles the custom tray panel via the surface state machine.
/// - **Right-click** opens the native context menu with shell actions.
pub fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Use the branded three-rail TokenBar mark even before the first provider
    // refresh. Its rail lengths become live usage data after refresh.
    let (rgba, width, height) = render_bar_icon_rgba(0.0, None, false);
    let icon = Image::new_owned(rgba, width, height);

    // **No `.menu()` on Windows.** Attaching one makes tray-icon show the
    // native `#32768` popup on right-click, and that is the menu M3 replaces:
    // Tauri exposes no owner-draw hook for it, so five of the six style defects
    // the user reported are unreachable there. Right-click is handled below
    // instead, showing the same self-drawn menu the taskbar strip shows.
    //
    // Removing it also retires the rebuild-on-every-refresh path: a native menu
    // is a retained object that has to be reconstructed whenever a status label
    // or a provider toggle changes, whereas the self-drawn one is built from
    // `tray_menu_spec` at the moment it opens and is therefore never stale.
    // Held across `build` so the borrow below stays valid. Windows never builds
    // one; every other platform still uses its own native tray menu, and the
    // macOS line is out of scope for this task.
    #[cfg(not(windows))]
    let native_menu =
        build_native_tray_menu(app.handle(), &crate::commands::get_provider_catalog(), &[])?;

    let builder = TrayIconBuilder::with_id("codexbar-main")
        .icon(icon)
        .tooltip("CodexBar Desktop")
        .show_menu_on_left_click(false);
    #[cfg(not(windows))]
    let builder = builder
        .menu(&native_menu)
        .on_menu_event(|app, event| handle_menu_event(app, event.id().as_ref()));

    let _tray = builder
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button,
                button_state,
                position,
                rect,
                ..
            } = event
            {
                let app = tray.app_handle();
                if button == MouseButton::Left {
                    // Publish the current icon bounds on mouse-down as well as
                    // mouse-up. The global flyout hook can queue its outside
                    // check before the up event toggles the panel; using the
                    // fresh anchor prevents a moved taskbar icon from being
                    // mistaken for an outside click during the close animation.
                    store_anchor(app, &rect, position);
                }
                if button == MouseButton::Left && button_state == MouseButtonState::Up {
                    // Left-click toggles the dedicated flyout window ("Open
                    // Tray Panel"): open it, or cleanly close it when this
                    // same click already blur-dismissed it (no open→close
                    // flicker). This is the only in-app surface a tray click
                    // opens — the internal PopOut dashboard window was
                    // removed. Called directly (not spawned): native
                    // tray-icon event callbacks run on the same main-thread
                    // event-loop context as `on_menu_event` below, where
                    // `settings_window::open_or_focus` is also called
                    // synchronously — the WebviewWindowBuilder deadlock only
                    // affects builds invoked from *synchronous Tauri IPC
                    // commands*, not native event-loop callbacks.
                    shell::flyout_window::toggle(app, None);
                }
                #[cfg(windows)]
                if button == MouseButton::Right && button_state == MouseButtonState::Up {
                    // The owner is the message-only window, not the strip: the
                    // strip's HWND does not exist while the strip is switched
                    // off, and the tray menu has to work either way. This
                    // callback runs on the main thread, which is where that
                    // window must be created — see `menu_host`.
                    if let Some(owner) = crate::menu_host::hwnd() {
                        crate::taskbar_context_menu::show(
                            owner,
                            // Keeps the status readouts: a 16 px icon shows no
                            // numbers, so this menu is the only place they
                            // appear. The strip drops them — it is already a
                            // status bar. See `MenuSurface`.
                            crate::taskbar_context_menu::MenuSurface::TrayIcon,
                        );
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// Route a native menu-item click to the corresponding shell action.
fn handle_menu_event(app: &AppHandle, id: &str) {
    match resolve_menu_action(id) {
        Some(MenuAction::Transition(request)) => {
            match request.mode {
                // Provider deep links are hosted by the tray panel, which is
                // the dedicated flyout window — `main`'s surface machine can
                // no longer host `SurfaceMode::TrayPanel` (see
                // `commands::set_surface_mode`). Open the flyout with the
                // provider's tray-anchored default position and tell it which
                // provider to select.
                SurfaceMode::TrayPanel => {
                    let _ = shell::flyout_window::open_or_focus(app, request.position);
                    if let SurfaceTarget::Provider { provider_id } = &request.target {
                        let _ = app.emit_to(
                            crate::shell::flyout_window::FLYOUT_LABEL,
                            FLYOUT_SELECT_PROVIDER_EVENT,
                            provider_id,
                        );
                    }
                }
                _ => {
                    let _ = shell::transition_to_target(
                        app,
                        request.mode,
                        request.target,
                        request.position,
                    );
                }
            }
        }
        Some(MenuAction::OpenSettings(tab)) => {
            let _ = shell::settings_window::open_or_focus(app, &tab);
        }
        Some(MenuAction::OpenFlyout) => {
            // Pass None: open_or_focus falls back to the tray-anchored
            // default position (same placement chain the old TrayPanel
            // transition used) when no explicit position is given.
            let _ = shell::flyout_window::open_or_focus(app, None);
        }
        Some(MenuAction::Refresh) => {
            let handle = app.clone();
            tauri::async_runtime::spawn(async move {
                let _ = crate::commands::do_refresh_providers(&handle).await;
            });
        }
        Some(MenuAction::ToggleProvider(provider_id)) => {
            let mut settings = Settings::load();
            if settings.enabled_providers.contains(&provider_id) {
                settings.enabled_providers.remove(&provider_id);
            } else {
                settings.enabled_providers.insert(provider_id);
            }
            let _ = settings.save();
            crate::floatbar::notify_settings_changed(app);
            rebuild_tray_menu(app);
        }
        Some(MenuAction::ToggleFloatBar) => {
            crate::floatbar::toggle(app);
            rebuild_tray_menu(app);
        }
        Some(MenuAction::Quit) => {
            app.exit(0);
        }
        None => {}
    }
}

/// The tray menu's content, as the `TrayMenuEntry` tree the native menu is
/// built from.
///
/// Exposed so the taskbar strip's self-drawn menu can carry identical content
/// instead of maintaining a second list that drifts. Ids are the same strings,
/// which is what lets [`dispatch_menu_id`] serve both menus.
pub(crate) fn tray_menu_spec(app: &AppHandle) -> Vec<crate::tray_menu::TrayMenuEntry> {
    let catalog = crate::commands::get_provider_catalog();
    let settings = Settings::load();
    let status_labels = tray_status_labels(app, &settings);
    build_tray_menu_with(
        &catalog,
        &status_labels,
        &settings.enabled_providers,
        settings.float_bar_enabled,
        settings.ui_language,
    )
}

/// Perform whatever a tray menu id means. The strip's menu posts the same ids,
/// so both surfaces share one set of handlers.
pub(crate) fn dispatch_menu_id(app: &AppHandle, id: &str) {
    handle_menu_event(app, id);
}

/// Live status rows for the current provider cache, or none when the app state
/// is not available yet.
fn tray_status_labels(app: &AppHandle, settings: &Settings) -> Vec<(String, String)> {
    let Some(state) = app.try_state::<Mutex<AppState>>() else {
        return vec![];
    };
    // `try_lock`, not `lock`: this is reachable from a window procedure while
    // another thread holds the state, and blocking the message loop there
    // would freeze the strip and its menu.
    match state.try_lock() {
        Ok(guard) => {
            status_labels_for_settings(settings, &guard.provider_cache, settings.ui_language)
        }
        Err(_) => vec![],
    }
}

/// Rebuild the native tray menu from current provider + settings state.
/// Push a freshly built native tray menu after a setting changed.
///
/// **No-op on Windows.** There is no retained menu there since M3: the
/// self-drawn popup is built from [`tray_menu_spec`] at the moment it opens, so
/// it always reflects current settings and cannot go stale. The callers are
/// left in place rather than made conditional — "make sure the tray menu is
/// current" is still the right thing for them to ask for, and it is this
/// function's business how much work that takes on a given platform.
#[allow(unused_variables)]
pub(crate) fn rebuild_tray_menu(app: &AppHandle) {
    #[cfg(not(windows))]
    {
        let catalog = crate::commands::get_provider_catalog();
        let settings = Settings::load();
        let status_labels = tray_status_labels(app, &settings);
        if let Ok(menu) = build_native_tray_menu(app, &catalog, &status_labels)
            && let Some(tray) = app.tray_by_id("codexbar-main")
        {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// Rebuild the tray menu with current provider status labels after a refresh cycle.
///
/// No-op on Windows, for the same reason as [`rebuild_tray_menu`] — and this is
/// the one that used to run after *every* refresh cycle.
#[allow(unused_variables)]
pub fn update_tray_status_items(
    app: &AppHandle,
    snapshots: &[crate::commands::ProviderUsageSnapshot],
) {
    #[cfg(not(windows))]
    {
        let catalog = crate::commands::get_provider_catalog();
        let settings = Settings::load();
        let status_labels = status_labels_for_settings(&settings, snapshots, settings.ui_language);

        if let Ok(menu) = build_native_tray_menu(app, &catalog, &status_labels)
            && let Some(tray) = app.tray_by_id("codexbar-main")
        {
            let _ = tray.set_menu(Some(menu));
        }
    }
}

/// Refresh every native tray surface that depends on settings and cached provider data.
pub(crate) fn refresh_tray_presentation(app: &AppHandle) {
    // Same reasoning as `tray_anchor` above: an unreadable cache redraws the
    // tray from an empty snapshot list, which is what a fresh launch shows
    // anyway. `unwrap_or_default` already handled "no state at all"; this
    // extends it to "state exists but is poisoned".
    let snapshots = app
        .try_state::<Mutex<AppState>>()
        .and_then(|st| st.lock().ok().map(|guard| guard.provider_cache.clone()))
        .unwrap_or_default();

    update_tray_status_items(app, &snapshots);
    update_tray_icon_and_tooltip(app, &snapshots);
}

/// Update the tray icon pixels and tooltip text to reflect current provider usage.
///
/// Behaviour mirrors egui's `choose_tray_update_plan` (rust/src/native_ui/app.rs):
/// - If `menu_bar_shows_highest_usage` is on OR `menu_bar_display_mode == "minimal"`,
///   render the bar from the healthy provider with the highest session usage.
/// - Otherwise render from the first enabled healthy provider (catalog order).
/// - When any provider exposes a weekly/secondary window, the icon shows both
///   bars from the same picked provider.
/// - With zero healthy providers but at least one error, fall back to an
///   error-styled icon using the last known max percentage so the tray
///   still communicates "something is wrong".
pub fn update_tray_icon_and_tooltip(
    app: &AppHandle,
    snapshots: &[crate::commands::ProviderUsageSnapshot],
) {
    let Some(tray) = app.tray_by_id("codexbar-main") else {
        return;
    };

    // ── Icon ─────────────────────────────────────────────────────────────
    let settings = Settings::load();
    let ordered_snapshots = ordered_snapshot_refs(&settings, snapshots);
    let ok_snapshots: Vec<_> = ordered_snapshots
        .iter()
        .copied()
        .filter(|s| s.error.is_none())
        .collect();
    let all_error = ok_snapshots.is_empty() && !snapshots.is_empty();

    let prefer_highest = settings.menu_bar_shows_highest_usage
        || settings.menu_bar_display_mode.as_str() == "minimal";

    let picked = pick_tray_provider(&ok_snapshots, prefer_highest);

    let (session_pct, weekly_pct) = match picked {
        Some(s) => selected_tray_percents(s, &settings),
        None => (
            ok_snapshots
                .iter()
                .map(|s| selected_tray_percents(s, &settings).0)
                .fold(0.0_f64, f64::max),
            None,
        ),
    };

    let (rgba, w, h) = render_bar_icon_rgba(session_pct, weekly_pct, all_error);
    let icon = Image::new_owned(rgba, w, h);
    let _ = tray.set_icon(Some(icon));

    // The taskbar strip renders the user's ordered entry list. Resolution and
    // the "never a silent zero" rules live in `taskbar_entries`; this only turns
    // resolved entries into localized strings.
    #[cfg(windows)]
    {
        use codexbar::locale::{LocaleKey, get_text};
        let lang = settings.ui_language;
        let window_label = |kind: &str| {
            get_text(
                lang,
                match kind {
                    "weekly" => LocaleKey::TaskbarWindowWeekly,
                    "daily" => LocaleKey::TaskbarWindowDaily,
                    "monthly" => LocaleKey::TaskbarWindowMonthly,
                    "balance" => LocaleKey::TaskbarWindowBalance,
                    "speed" => LocaleKey::TaskbarWindowSpeed,
                    _ => LocaleKey::TaskbarWindowSession,
                },
            )
        };
        let speed_snapshot = crate::commands::get_output_speed_snapshot();
        let speed_for = |provider_id: &str| match provider_id {
            "codex" => speed_snapshot.codex.tokens_per_second,
            "claude" => speed_snapshot.claude.tokens_per_second,
            "grok" => speed_snapshot.grok.tokens_per_second,
            _ => None,
        };

        let resolved = crate::taskbar_entries::resolve_entries(
            &settings,
            snapshots,
            picked,
            &window_label,
            &speed_for,
        );

        let lines = resolved
            .into_iter()
            .map(|entry| {
                use crate::taskbar_entries::EntryUnavailable;
                // The mark is the glyph fallback only; the icon slot is filled by
                // the official SVG via `icon_provider_id` when the entry names a
                // real provider. An `auto` entry with nothing picked has no
                // artwork and nothing to identify a provider with, so it falls
                // back to a bare glyph.
                let mark = crate::provider_mark::provider_mark(&entry.provider_id);
                let icon_provider_id = if entry.provider_id
                    == codexbar::settings::TASKBAR_PROVIDER_AUTO
                {
                    None
                } else {
                    Some(entry.provider_id.clone())
                };
                let window_kind = entry.window_kind.clone();
                let tag = entry.window.trim().to_string();
                let (value, state) = if let Some(ref amount) = entry.amount {
                    // Money prints verbatim; the window word already says what it is.
                    (amount.clone(), "ready")
                } else {
                    match (entry.percent, entry.unavailable) {
                        // Speed is a rate, not a percentage, so it keeps its unit.
                        (Some(speed), None) if window_kind == "speed" => {
                            (format!("{speed:.1} t/s"), "ready")
                        },
                        (Some(percent), None) => (
                            crate::commands::format_quota_percent(percent),
                            "ready",
                        ),
                        (_, Some(reason)) => {
                            // The reason replaces the number outright — an entry
                            // that cannot be measured must never print a fabricated
                            // percentage.
                            let key = match reason {
                                EntryUnavailable::ProviderDisabled => LocaleKey::TaskbarEntryProviderDisabled,
                                EntryUnavailable::NoData => LocaleKey::TaskbarEntryNoData,
                                EntryUnavailable::ProviderError => LocaleKey::TaskbarEntryError,
                                EntryUnavailable::WindowUnsupported => LocaleKey::TaskbarEntryUnsupported,
                            };
                            let state = match reason {
                                EntryUnavailable::ProviderDisabled => "notConfigured",
                                EntryUnavailable::NoData => "unknown",
                                EntryUnavailable::ProviderError => "error",
                                EntryUnavailable::WindowUnsupported => "unsupported",
                            };
                            (get_text(lang, key), state)
                        },
                        _ => (entry.provider_label.clone(), "ready"),
                    }
                };
                crate::taskbar_widget::StripLine {
                    mark,
                    icon_provider_id,
                    tag,
                    value,
                    window_kind,
                    state: state.to_string(),
                }
            })
            .collect::<Vec<_>>();

        crate::taskbar_widget::set_entries(lines);
    }

    let tooltip = build_tooltip(
        snapshots,
        settings.ui_language,
        &settings.taskbar_tooltip_entries,
    );
    let _ = tray.set_tooltip(Some(tooltip));
}

fn status_labels_for_settings(
    settings: &Settings,
    snapshots: &[crate::commands::ProviderUsageSnapshot],
    lang: codexbar::settings::Language,
) -> Vec<(String, String)> {
    let ordered_snapshots = ordered_snapshot_refs(settings, snapshots);
    let healthy: Vec<_> = ordered_snapshots
        .into_iter()
        .filter(|s| s.error.is_none())
        .collect();
    if settings.tray_icon_mode == TrayIconMode::PerProvider {
        return healthy
            .into_iter()
            .map(|s| provider_status_label(s, lang, settings.taskbar_reset_time_relative))
            .collect::<Vec<_>>();
    }

    let Some(selected) = pick_tray_provider(
        &healthy,
        settings.menu_bar_shows_highest_usage || settings.menu_bar_display_mode == "minimal",
    ) else {
        return vec![];
    };

    let (_, label) = provider_status_label(selected, lang, settings.taskbar_reset_time_relative);
    vec![("status_summary".to_string(), label)]
}

fn ordered_snapshot_refs<'a>(
    settings: &Settings,
    snapshots: &'a [crate::commands::ProviderUsageSnapshot],
) -> Vec<&'a crate::commands::ProviderUsageSnapshot> {
    let order = settings
        .provider_display_order_names()
        .into_iter()
        .enumerate()
        .map(|(index, provider_id)| (provider_id, index))
        .collect::<std::collections::HashMap<_, _>>();
    let mut ordered = snapshots.iter().collect::<Vec<_>>();
    ordered.sort_by(|a, b| {
        let a_order = order.get(&a.provider_id);
        let b_order = order.get(&b.provider_id);
        match (a_order, b_order) {
            (Some(a_order), Some(b_order)) if a_order != b_order => a_order.cmp(b_order),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            _ => a.display_name.cmp(&b.display_name),
        }
    });
    ordered
}

fn provider_status_label(
    snapshot: &crate::commands::ProviderUsageSnapshot,
    lang: codexbar::settings::Language,
    relative_reset: bool,
) -> (String, String) {
    // Antigravity's summary probe publishes the real buckets as named extras
    // and leaves the primary slot as a skipped placeholder; the tray label must
    // use the most restricted known bucket from the same named windows.
    let window = if snapshot.provider_id == "antigravity" {
        most_restricted_known_window(snapshot)
    } else {
        &snapshot.primary
    };
    let label = crate::commands::compact_tray_status_label(window, lang, relative_reset);
    (
        snapshot.provider_id.clone(),
        format!("{} {}", snapshot.display_name, label),
    )
}

/// The most constrained non-informational window in a snapshot. Used where a
/// provider's primary slot is a placeholder (Antigravity's quota summary) so
/// the compact/tray reading stays the real, most-restricted known bucket.
fn most_restricted_known_window(
    snapshot: &crate::commands::ProviderUsageSnapshot,
) -> &crate::commands::RateWindowSnapshot {
    let mut best: Option<&crate::commands::RateWindowSnapshot> = None;
    let mut best_percent = -1.0;
    for window in std::iter::once(&snapshot.primary)
        .chain(snapshot.secondary.iter())
        .chain(snapshot.model_specific.iter())
        .chain(snapshot.tertiary.iter())
        .chain(snapshot.extra_rate_windows.iter().map(|extra| &extra.window))
    {
        if window.is_informational {
            continue;
        }
        if window.used_percent > best_percent {
            best_percent = window.used_percent;
            best = Some(window);
        }
    }
    best.unwrap_or(&snapshot.primary)
}

/// Pick the provider whose usage the tray icon should render.
///
/// Exposed so that the unit tests can exercise both `highest` and `first`
/// paths without needing a live Tauri app handle.
fn pick_tray_provider<'a>(
    ok_snapshots: &'a [&'a crate::commands::ProviderUsageSnapshot],
    prefer_highest: bool,
) -> Option<&'a crate::commands::ProviderUsageSnapshot> {
    if ok_snapshots.is_empty() {
        return None;
    }
    if prefer_highest {
        ok_snapshots.iter().copied().max_by(|a, b| {
            provider_usage_percent(a)
                .partial_cmp(&provider_usage_percent(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    } else {
        Some(ok_snapshots[0])
    }
}

fn provider_usage_percent(snapshot: &crate::commands::ProviderUsageSnapshot) -> f64 {
    if snapshot.provider_id == "antigravity" {
        most_restricted_known_window(snapshot).used_percent
    } else {
        snapshot.primary.used_percent
    }
}

fn selected_tray_percents(
    snapshot: &crate::commands::ProviderUsageSnapshot,
    settings: &Settings,
) -> (f64, Option<f64>) {
    let provider = ProviderId::from_cli_name(snapshot.provider_id.as_str());
    let preference = provider
        .map(|id| settings.get_provider_metric(id))
        .unwrap_or(MetricPreference::Automatic);
    let primary = selected_metric_percent(snapshot, provider, preference)
        .or_else(|| selected_metric_percent(snapshot, provider, MetricPreference::Automatic))
        .unwrap_or(snapshot.primary.used_percent);

    // The notification-area icon and the taskbar status strip are one component
    // in the settings model (both live on the Taskbar page), so both follow
    // `taskbar_show_as_used` rather than the retired global `show_as_used`.
    let show_as_used = settings.taskbar_show_as_used;
    let secondary = snapshot
        .secondary
        .as_ref()
        .map(|w| display_metric_percent(w.used_percent, show_as_used));

    (display_metric_percent(primary, show_as_used), secondary)
}

fn display_metric_percent(used_percent: f64, show_as_used: bool) -> f64 {
    let used = used_percent.clamp(0.0, 100.0);
    if show_as_used { used } else { 100.0 - used }
}

fn selected_metric_percent(
    snapshot: &crate::commands::ProviderUsageSnapshot,
    provider: Option<ProviderId>,
    preference: MetricPreference,
) -> Option<f64> {
    match preference {
        MetricPreference::Automatic => automatic_metric_percent(snapshot, provider),
        MetricPreference::Session => Some(snapshot.primary.used_percent),
        MetricPreference::Weekly => snapshot
            .secondary
            .as_ref()
            .map(|w| w.used_percent)
            .or(Some(snapshot.primary.used_percent)),
        MetricPreference::Model => snapshot
            .model_specific
            .as_ref()
            .map(|w| w.used_percent)
            .or(Some(snapshot.primary.used_percent)),
        MetricPreference::Tertiary => snapshot
            .tertiary
            .as_ref()
            .map(|w| w.used_percent)
            .or_else(|| snapshot.secondary.as_ref().map(|w| w.used_percent))
            .or(Some(snapshot.primary.used_percent)),
        MetricPreference::Credits => cost_metric_percent(snapshot),
        MetricPreference::ExtraUsage => {
            extra_rate_window_percent(snapshot).or_else(|| cost_metric_percent(snapshot))
        }
        MetricPreference::Average => average_metric_percent(snapshot),
    }
}

fn automatic_metric_percent(
    snapshot: &crate::commands::ProviderUsageSnapshot,
    provider: Option<ProviderId>,
) -> Option<f64> {
    match provider {
        Some(ProviderId::Cursor) => max_metric_percent([
            Some(snapshot.primary.used_percent),
            snapshot.secondary.as_ref().map(|w| w.used_percent),
            snapshot.tertiary.as_ref().map(|w| w.used_percent),
        ]),
        Some(ProviderId::Zai) => max_metric_percent([
            Some(snapshot.primary.used_percent),
            snapshot.tertiary.as_ref().map(|w| w.used_percent),
            None,
        ])
        .or_else(|| snapshot.secondary.as_ref().map(|w| w.used_percent)),
        Some(ProviderId::Factory) | Some(ProviderId::Kimi) => snapshot
            .secondary
            .as_ref()
            .map(|w| w.used_percent)
            .or(Some(snapshot.primary.used_percent)),
        Some(ProviderId::Copilot) => max_metric_percent([
            Some(snapshot.primary.used_percent),
            snapshot.secondary.as_ref().map(|w| w.used_percent),
            extra_rate_window_percent(snapshot),
        ]),
        // The summary probe leaves the primary slot as a skipped placeholder;
        // the automatic metric reads the most restricted known named bucket.
        Some(ProviderId::Antigravity) => {
            Some(most_restricted_known_window(snapshot).used_percent)
        }
        _ => Some(snapshot.primary.used_percent),
    }
}

fn average_metric_percent(snapshot: &crate::commands::ProviderUsageSnapshot) -> Option<f64> {
    let secondary = snapshot.secondary.as_ref()?;
    Some((snapshot.primary.used_percent + secondary.used_percent) / 2.0)
}

fn cost_metric_percent(snapshot: &crate::commands::ProviderUsageSnapshot) -> Option<f64> {
    let cost = snapshot.cost.as_ref()?;
    let limit = cost.limit?;
    if limit <= 0.0 {
        return None;
    }
    Some(((cost.used / limit) * 100.0).clamp(0.0, 100.0))
}

fn extra_rate_window_percent(snapshot: &crate::commands::ProviderUsageSnapshot) -> Option<f64> {
    snapshot
        .extra_rate_windows
        .iter()
        .map(|extra| extra.window.used_percent)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

fn max_metric_percent<const N: usize>(values: [Option<f64>; N]) -> Option<f64> {
    values
        .into_iter()
        .flatten()
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

/// Build a compact multi-line tooltip string from provider snapshots.
///
/// TASK-021 item 9: when the user configured `taskbar_tooltip_entries`, those
/// (provider + window) rows drive the hover text. Otherwise the historical
/// per-enabled-provider primary line is used. Raw secrets/long errors are
/// truncated; auth/network categories are not dumped as full payloads.
pub(crate) fn build_tooltip(
    snapshots: &[crate::commands::ProviderUsageSnapshot],
    lang: codexbar::settings::Language,
    tooltip_entries: &[codexbar::settings::TaskbarEntry],
) -> String {
    use codexbar::locale::{LocaleKey, get_text};

    if snapshots.is_empty() {
        return "CodexBar Desktop".to_string();
    }

    let error_label = get_text(lang, LocaleKey::TrayStatusRowError);
    let mut lines: Vec<String> = Vec::new();

    if !tooltip_entries.is_empty() {
        for entry in tooltip_entries.iter().take(6) {
            let provider = if entry.provider_id == codexbar::settings::TASKBAR_PROVIDER_AUTO
                || entry.provider_id.is_empty()
            {
                snapshots.first()
            } else {
                snapshots
                    .iter()
                    .find(|s| s.provider_id == entry.provider_id)
            };
            let Some(s) = provider else {
                lines.push(format!("{}: —", entry.provider_id));
                continue;
            };
            if let Some(ref err) = s.error {
                let short = truncate_tooltip_text(err, 28);
                lines.push(format!(
                    "{}: {} ({})",
                    s.display_name, error_label, short
                ));
                continue;
            }
            let window = match entry.window.as_str() {
                "primary" => Some(&s.primary),
                kind => s
                    .extra_rate_windows
                    .iter()
                    .find(|w| {
                        w.window
                            .kind
                            .as_deref()
                            .map(|k| k.eq_ignore_ascii_case(kind))
                            .unwrap_or(false)
                            || w.id.eq_ignore_ascii_case(kind)
                    })
                    .map(|w| &w.window)
                    .or(Some(&s.primary)),
            };
            let label = window
                .map(|w| crate::commands::compact_tray_status_label(w, lang, true))
                .unwrap_or_else(|| "—".to_string());
            lines.push(format!(
                "{}: {}",
                s.display_name,
                truncate_tooltip_text(&label, 42)
            ));
        }
    } else {
        for s in snapshots {
            let status = if let Some(ref err) = s.error {
                let short = truncate_tooltip_text(err, 36);
                format!("{}: {} ({})", s.display_name, error_label, short)
            } else {
                let label = crate::commands::compact_tray_status_label(&s.primary, lang, true);
                format!("{}: {}", s.display_name, truncate_tooltip_text(&label, 42))
            };
            lines.push(status);
        }
    }

    format!("CodexBar\n{}", lines.join("\n"))
}

fn truncate_tooltip_text(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[allow(dead_code)]
fn menu_contains(menu: &[TrayMenuEntry], id: &str) -> bool {
    menu.iter().any(|entry| {
        entry.id.as_deref() == Some(id)
            || (!entry.children.is_empty() && menu_contains(&entry.children, id))
    })
}

/// Only the non-Windows tray uses a retained native menu; Windows shows the
/// self-drawn popup from `taskbar_menu` instead (M3).
#[cfg(not(windows))]
enum NativeMenuEntry {
    Item(MenuItem<tauri::Wry>),
    CheckItem(tauri::menu::CheckMenuItem<tauri::Wry>),
    Submenu(Submenu<tauri::Wry>),
    Separator(PredefinedMenuItem<tauri::Wry>),
}

#[cfg(not(windows))]
impl NativeMenuEntry {
    fn as_item(&self) -> &dyn IsMenuItem<tauri::Wry> {
        match self {
            Self::Item(item) => item,
            Self::CheckItem(item) => item,
            Self::Submenu(item) => item,
            Self::Separator(item) => item,
        }
    }
}

/// Only the non-Windows tray uses a retained native menu; Windows shows the
/// self-drawn popup from `taskbar_menu` instead (M3).
#[cfg(not(windows))]
fn build_native_menu_entry(
    app: &AppHandle,
    entry: &TrayMenuEntry,
) -> tauri::Result<NativeMenuEntry> {
    if entry.is_separator {
        return Ok(NativeMenuEntry::Separator(PredefinedMenuItem::separator(
            app,
        )?));
    }

    if !entry.children.is_empty() {
        let children = entry
            .children
            .iter()
            .map(|child| build_native_menu_entry(app, child))
            .collect::<tauri::Result<Vec<_>>>()?;
        let child_refs = children
            .iter()
            .map(NativeMenuEntry::as_item)
            .collect::<Vec<_>>();

        return Ok(NativeMenuEntry::Submenu(Submenu::with_items(
            app,
            &entry.label,
            true,
            &child_refs,
        )?));
    }

    // Render as a checkbox item when `checked` is set.
    if let Some(checked) = entry.checked {
        return Ok(NativeMenuEntry::CheckItem(
            CheckMenuItemBuilder::with_id(entry.id.clone().unwrap_or_default(), &entry.label)
                .enabled(!entry.disabled)
                .checked(checked)
                .build(app)?,
        ));
    }

    Ok(NativeMenuEntry::Item(MenuItem::with_id(
        app,
        entry.id.clone().unwrap_or_default(),
        &entry.label,
        !entry.disabled,
        None::<&str>,
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_provider_catalog() -> Vec<ProviderCatalogEntry> {
        vec![
            ProviderCatalogEntry {
                id: "codex".into(),
                display_name: "Codex".into(),
                cookie_domain: None,
            },
            ProviderCatalogEntry {
                id: "claude".into(),
                display_name: "Claude".into(),
                cookie_domain: None,
            },
        ]
    }

    use crate::tray_menu::build_tray_menu;

    #[test]
    fn tray_menu_includes_about_and_provider_entries() {
        let menu = build_tray_menu(
            &sample_provider_catalog(),
            &[],
            &["codex".to_string(), "claude".to_string()]
                .into_iter()
                .collect(),
        );
        assert!(menu_contains(&menu, "about"));
        assert!(menu_contains(&menu, "toggle_provider:codex"));
        assert!(menu_contains(&menu, "quit"));
    }

    #[test]
    fn toggle_float_bar_routes_to_toggle_action() {
        let action = resolve_menu_action("toggle_float_bar").expect("float bar action");
        assert!(matches!(action, MenuAction::ToggleFloatBar));
    }

    #[test]
    fn settings_menu_routes_to_open_settings_action() {
        let action = resolve_menu_action("about").expect("about action");
        match action {
            MenuAction::OpenSettings(tab) => assert_eq!(tab, "about"),
            _ => panic!("expected OpenSettings for 'about'"),
        }

        let action = resolve_menu_action("settings").expect("settings action");
        match action {
            MenuAction::OpenSettings(tab) => assert_eq!(tab, "general"),
            _ => panic!("expected OpenSettings for 'settings'"),
        }
    }

    #[test]
    fn provider_menu_routes_to_provider_tray_panel_target() {
        // The internal PopOut window is gone, so a provider deep link is
        // hosted by the tray panel (dispatching opens the flyout window).
        let action = resolve_menu_target("provider:codex").expect("provider target");
        assert_eq!(action.mode, SurfaceMode::TrayPanel);
        assert_eq!(
            action.target,
            SurfaceTarget::Provider {
                provider_id: "codex".into()
            }
        );
    }

    #[test]
    fn pop_out_menu_routes_to_open_flyout_action() {
        // "Open Tray Panel" opens the dedicated flyout window — not a
        // `shell::ShellTransitionRequest` against the `main`-window surface
        // machine. With the internal PopOut dashboard removed, this is the
        // only in-app surface the menu's "Open" row offers.
        let action = resolve_menu_action("pop_out").expect("pop_out action");
        assert!(matches!(action, MenuAction::OpenFlyout));

        // resolve_menu_target no longer resolves "pop_out" at all — it is
        // intercepted earlier in resolve_menu_action.
        assert!(resolve_menu_target("pop_out").is_none());

        // "show_panel" (the old "Open Dashboard" / PopOut entry) is gone.
        assert!(resolve_menu_target("show_panel").is_none());

        // SurfaceMode::TrayPanel remains the single source for the anchored
        // flyout's default size, minimum bounds, and window behavior.
        let props = SurfaceMode::TrayPanel.window_properties();
        assert!(props.resizable && props.blur_dismiss && props.skip_taskbar);
    }

    #[test]
    fn logical_tray_anchor_uses_click_monitor_scale() {
        let monitors = vec![
            MonitorScaleInfo {
                physical_x: 0,
                physical_y: 0,
                physical_width: 1920,
                physical_height: 1080,
                scale_factor: 1.0,
            },
            MonitorScaleInfo {
                physical_x: 1920,
                physical_y: 0,
                physical_width: 2560,
                physical_height: 1440,
                scale_factor: 2.0,
            },
        ];

        let rect = tauri::Rect {
            position: tauri::Position::Logical(tauri::LogicalPosition::new(1500.0, 500.0)),
            size: tauri::Size::Logical(tauri::LogicalSize::new(12.0, 12.0)),
        };
        let anchor = resolve_tray_anchor(
            &rect,
            tauri::PhysicalPosition::new(1510.0, 500.0),
            &monitors,
        )
        .expect("matching click monitor scale");

        assert_eq!(anchor.x, 1500);
        assert_eq!(anchor.y, 500);
        assert_eq!(anchor.width, 12);
        assert_eq!(anchor.height, 12);
    }

    #[test]
    fn logical_tray_anchor_skips_conversion_without_click_monitor() {
        let monitors = vec![MonitorScaleInfo {
            physical_x: 0,
            physical_y: 0,
            physical_width: 1920,
            physical_height: 1080,
            scale_factor: 1.0,
        }];
        let rect = tauri::Rect {
            position: tauri::Position::Logical(tauri::LogicalPosition::new(1500.0, 500.0)),
            size: tauri::Size::Logical(tauri::LogicalSize::new(12.0, 12.0)),
        };

        // The logical anchor has no scale to convert with when the click is
        // outside every monitor, so `resolve_tray_anchor` must skip it.
        let anchor = resolve_tray_anchor(
            &rect,
            tauri::PhysicalPosition::new(2000.0, 600.0),
            &monitors,
        );

        assert!(anchor.is_none(), "without a click monitor the anchor must be skipped");
    }
}
