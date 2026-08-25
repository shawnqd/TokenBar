//! Building and dispatching the shared right-click menu.
//!
//! Split out of `taskbar_widget`, which had grown to hold three unrelated jobs:
//! hosting a window inside Explorer's taskbar, painting a status strip, and
//! this. Only the third is shared — since M3 the notification-area icon shows
//! the *same* menu, owned by `menu_host`'s message-only window instead of the
//! strip — so keeping it inside the strip's module made a shared thing look
//! like the strip's private business.
//!
//! # Two design points to know before touching this
//!
//! **Ids travel as row positions.** The self-drawn menu posts a `usize` through
//! `WM_COMMAND`, but actions are identified by the tray's string ids
//! (`"refresh"`, `"toggle_provider:codex"`, …). [`MENU_COMMAND_IDS`] is the
//! translation table, captured when the menu opens; a row's **1-based** index
//! is what travels, because wparam 0 cannot be told from "no selection".
//!
//! **Submenus are not expanded.** The row count matches the tray icon's menu,
//! where a submenu is one row. An earlier pass expanded 提供方 into a header
//! plus one row per provider, which turned a nine-row menu into an arbitrarily
//! long one and was rejected. 提供方 is one row that opens Settings →
//! Providers, which owns the same toggles.

#![cfg(windows)]

use std::sync::Mutex;

use crate::taskbar_widget::{APP_HANDLE, set_enabled};

/// Builds the strip's right-click menu and hands it to the self-drawn popup in
/// [`crate::taskbar_menu`].
///
/// This window stays the menu's owner: the chosen row's id comes back as a
/// `WM_COMMAND`, exactly the shape `TPM_RETURNCMD` used to deliver, so
/// `handle_context_command` needs no change. Unlike `TrackPopupMenu` the call
/// returns immediately rather than running a modal loop.
/// The string ids of the rows in the menu currently on screen, in order.
///
/// The self-drawn menu posts a `usize` back through `WM_COMMAND`, but the
/// actions are identified by the same string ids the notification-area tray
/// menu uses (`"refresh"`, `"toggle_provider:codex"`, …) so both menus share
/// one set of handlers. This is the translation table: a row's 1-based index
/// is what travels, and it is resolved here. Index 0 is never used, because
/// `WM_COMMAND` wparam 0 is indistinguishable from "no selection".
static MENU_COMMAND_IDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Local-only id for the strip's own visibility toggle. Not a tray menu id —
/// the tray has no equivalent row, so it is handled before delegating.
const STRIP_TOGGLE_ID: &str = "toggle_taskbar_strip";

/// Strip the leading status readouts, and the separator that trailed them.
///
/// `build_tray_menu_with` emits the rows as a block at the very top followed by
/// one separator, so removing them means dropping the run of disabled rows from
/// the front and then the separator they left behind — otherwise the menu opens
/// with a rule above its first action.
fn drop_status_rows(spec: &mut Vec<crate::tray_menu::TrayMenuEntry>) {
    let status_rows = spec
        .iter()
        .take_while(|entry| entry.disabled && !entry.is_separator)
        .count();
    if status_rows == 0 {
        return;
    }
    let trailing_separator = usize::from(
        spec.get(status_rows)
            .is_some_and(|entry| entry.is_separator),
    );
    spec.drain(..status_rows + trailing_separator);
}

/// Converts the tray menu tree into the flat row list the self-drawn menu
/// draws, and records each row's id.
///
/// **Submenu children are not expanded.** The row count has to match the tray
/// icon's menu, and there a submenu is a single row — expanding 提供方 into a
/// header plus one row per provider turned a nine-row menu into an arbitrarily
/// long one. The parent stays one row; [`handle_context_command`] sends it
/// somewhere that can show the children.
fn flatten_menu_entries(
    entries: &[crate::tray_menu::TrayMenuEntry],
    items: &mut Vec<crate::taskbar_menu::MenuItem>,
    ids: &mut Vec<String>,
) {
    use crate::taskbar_menu::MenuItem;

    for entry in entries {
        if entry.is_separator {
            items.push(MenuItem::separator());
            ids.push(String::new());
            continue;
        }

        let mut item = MenuItem::action(ids.len() + 1, entry.label.clone());
        if let Some(checked) = entry.checked {
            item = item.checked(checked);
        }
        item.disabled = entry.disabled;
        items.push(item);
        ids.push(entry.id.clone().unwrap_or_default());
    }
}

/// Places the strip's visibility toggle immediately after the floating bar's,
/// and renumbers every row.
///
/// Not appended: the two toggles do the same kind of thing — show or hide one
/// of the app's surfaces — and belong in the same group. Appending put this one
/// below Quit, which is the grouping defect the user reported.
///
/// The renumbering is the part that matters. A row's id *is* its 1-based
/// position, so inserting anywhere but the end invalidates every id after it.
fn insert_strip_toggle(
    items: &mut Vec<crate::taskbar_menu::MenuItem>,
    ids: &mut Vec<String>,
    label: String,
    checked: bool,
) {
    let insert_at = ids
        .iter()
        .position(|id| id == "toggle_float_bar")
        .map(|index| index + 1)
        .unwrap_or(items.len());
    items.insert(
        insert_at,
        crate::taskbar_menu::MenuItem::action(0, label).checked(checked),
    );
    ids.insert(insert_at, STRIP_TOGGLE_ID.to_string());
    for (position, item) in items.iter_mut().enumerate() {
        item.id = position + 1;
    }
}

/// Which surface a context menu was opened from.
///
/// The two menus carry the same *actions* — that is M2, and it must stay true —
/// but they do not need the same *readouts*, and since M3 each surface can say
/// so for itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MenuSurface {
    /// The taskbar strip, which is already painting the numbers.
    Strip,
    /// The notification-area icon, a 16 px glyph that is painting nothing.
    TrayIcon,
}

impl MenuSurface {
    /// Whether this surface's menu should carry the live status readouts.
    ///
    /// The strip does not: it is a status bar, so its own rows already show
    /// what the readout would repeat, and that readout was the widest element
    /// in the menu — one non-clickable line made the card roughly twice as wide
    /// as its actions needed (B5). The tray icon does: a 16 px icon shows no
    /// numbers at all, so dropping the rows there would cost real information.
    ///
    /// This is exactly the decision B5 had to defer until M3 landed, because
    /// before that both menus came from one builder with no way to differ.
    fn wants_status_rows(self) -> bool {
        matches!(self, Self::TrayIcon)
    }
}

/// Build the shared context menu and show it owned by `hwnd`.
///
/// `pub(crate)` because the notification-area tray icon shows the same menu,
/// owned by `menu_host`'s message-only window instead of the strip. That is the
/// whole of M3: one builder, one dispatcher, two owners.
pub(crate) fn show(hwnd: isize, surface: MenuSurface) {
    use crate::taskbar_menu::MenuItem;
    use codexbar::locale::{LocaleKey, get_text};

    // Whether the right-click reaches us at all is the first fact to establish.
    // The strip is a child inside Explorer's window tree, so "our menu is
    // broken" and "Explorer handled the click and ours never ran" look
    // identical from outside and need opposite fixes.
    tracing::info!(?surface, "strip menu: right-click received");
    let Some(app) = APP_HANDLE.get() else {
        tracing::warn!("strip menu: no app handle; nothing will open");
        return;
    };
    let settings = codexbar::settings::Settings::load();

    // Content comes from `build_tray_menu`, the same builder the
    // notification-area menu uses, so the two can no longer drift apart.
    let mut spec = crate::tray_bridge::tray_menu_spec(app);
    if !surface.wants_status_rows() {
        drop_status_rows(&mut spec);
    }
    let mut items: Vec<MenuItem> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    flatten_menu_entries(&spec, &mut items, &mut ids);

    // The strip's own visibility toggle has no tray equivalent, so it is added
    // here rather than coming from the builder — but it is *inserted next to
    // the floating bar's toggle*, not appended. The two do the same kind of
    // thing (show or hide one of the app's surfaces) and belong in the same
    // group; appending put this one below Quit, which is the grouping defect
    // the user reported. Named with item 7's vocabulary (小型状态栏), not
    // 任务栏: the strip lives *inside* the Windows taskbar but is not it.
    insert_strip_toggle(
        &mut items,
        &mut ids,
        get_text(
            settings.ui_language,
            LocaleKey::TaskbarContextMenuShowStrip,
        ),
        settings.taskbar_widget_enabled,
    );

    if let Ok(mut guard) = MENU_COMMAND_IDS.lock() {
        *guard = ids;
    }
    tracing::info!(rows = items.len(), "strip menu: handing rows to the renderer");
    crate::taskbar_menu::show(hwnd, items);
}

/// Resolve a picked row's 1-based position back to its tray menu id and run it.
///
/// Shared with the tray icon's menu — see [`show_context_menu`].
pub(crate) fn handle_command(index: usize) {
    let Some(app) = APP_HANDLE.get() else {
        return;
    };
    let id = MENU_COMMAND_IDS
        .lock()
        .ok()
        .and_then(|ids| ids.get(index.wrapping_sub(1)).cloned())
        .unwrap_or_default();
    if id.is_empty() {
        return;
    }

    if id == STRIP_TOGGLE_ID {
        // Same read-modify-save-apply pattern `commands/settings.rs` uses for
        // this exact setting (`taskbar_widget_enabled`).
        let mut settings = codexbar::settings::Settings::load();
        let next = !settings.taskbar_widget_enabled;
        settings.taskbar_widget_enabled = next;
        let _ = settings.save();
        set_enabled(next);
        // Every other writer of this setting goes through `update_settings`,
        // which broadcasts afterwards. Without the same broadcast here the
        // Settings window keeps rendering its stale snapshot — its toggle still
        // reads "on", so the next click sends `false` against a setting that is
        // already false, and the strip looks impossible to turn back on.
        crate::events::emit_settings_changed(app);
        return;
    }

    if id == "providers" {
        // The tray menu opens a real submenu here. This menu has none, so the
        // row goes to the page that owns the same toggles instead of being a
        // dead end — building a nested flyout (hover-open timing, a second
        // window, keyboard descent) is a larger piece of work than this row is
        // worth, and is recorded in the backlog if it turns out to be wanted.
        let _ = crate::shell::settings_window::open_or_focus(app, "providers");
        return;
    }

    crate::tray_bridge::dispatch_menu_id(app, &id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_menu_drops_status_rows_but_keeps_every_action() {
        use crate::tray_menu::build_tray_menu_with;
        use codexbar::settings::Language;

        let catalog = vec![crate::commands::ProviderCatalogEntry {
            id: "codex".into(),
            display_name: "Codex".into(),
            cookie_domain: None,
        }];
        let enabled = ["codex".to_string()].into_iter().collect();
        let build = || {
            build_tray_menu_with(
                &catalog,
                &[("codex".to_string(), "Codex 30%".to_string())],
                &enabled,
                true,
                Language::English,
            )
        };

        let tray = build();
        let mut strip = build();
        drop_status_rows(&mut strip);

        assert!(
            tray.iter().any(|entry| entry.disabled && !entry.is_separator),
            "the fixture has a status row to drop, or this test proves nothing"
        );
        assert!(
            !strip
                .iter()
                .any(|entry| entry.disabled && !entry.is_separator),
            "no status readout survives on the strip"
        );
        assert!(
            !strip.first().is_some_and(|entry| entry.is_separator),
            "the separator the rows left behind goes with them"
        );

        let actions = |spec: &[crate::tray_menu::TrayMenuEntry]| {
            spec.iter()
                .filter(|entry| !entry.is_separator && !entry.disabled)
                .filter_map(|entry| entry.id.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            actions(&strip),
            actions(&tray),
            "the two menus differ in readouts only, never in what they can do"
        );
    }

    /// Dropping nothing must not corrupt the menu: a provider set with no live
    /// usage produces no status rows, and the first real row must survive.
    #[test]
    fn dropping_status_rows_is_a_no_op_when_there_are_none() {
        use crate::tray_menu::build_tray_menu_with;
        use codexbar::settings::Language;

        let enabled = std::collections::HashSet::new();
        let mut spec = build_tray_menu_with(&[], &[], &enabled, true, Language::English);
        let before = spec.len();
        drop_status_rows(&mut spec);
        assert_eq!(spec.len(), before, "nothing to drop, nothing dropped");
    }

    /// resolves through. They must stay index-aligned, because the row's
    /// 1-based position is the only thing that travels back in `WM_COMMAND`.
    #[test]
    fn flattening_keeps_rows_and_ids_aligned() {
        use crate::tray_menu::build_tray_menu_with;
        use codexbar::settings::Language;

        let catalog = vec![crate::commands::ProviderCatalogEntry {
            id: "codex".into(),
            display_name: "Codex".into(),
            cookie_domain: None,
        }];
        let enabled = ["codex".to_string()].into_iter().collect();
        let spec = build_tray_menu_with(
            &catalog,
            &[("codex".to_string(), "Codex 30%".to_string())],
            &enabled,
            true,
            Language::English,
        );

        let mut items = Vec::new();
        let mut ids = Vec::new();
        flatten_menu_entries(&spec, &mut items, &mut ids);

        assert_eq!(items.len(), ids.len(), "one id per drawn row");
        for (position, item) in items.iter().enumerate() {
            if item.separator {
                assert!(ids[position].is_empty(), "separators carry no action");
            } else {
                assert_eq!(item.id, position + 1, "ids are 1-based row positions");
            }
        }

        // The provider submenu stays ONE row. Expanding it is what made the
        // menu balloon past the tray icon's own length, which is the shape
        // this menu is supposed to match.
        assert!(
            ids.iter().any(|id| id == "providers"),
            "the providers row is present"
        );
        assert!(
            !ids.iter().any(|id| id.starts_with("toggle_provider:")),
            "individual providers must NOT be expanded into rows"
        );
        assert_eq!(
            items.len(),
            spec.len(),
            "one drawn row per top-level tray entry, no more"
        );

        // Status rows come from the builder already disabled.
        let status = ids
            .iter()
            .position(|id| id == "status_codex")
            .expect("status row present");
        assert!(items[status].disabled);

        // Nothing from the old fixed list survives.
        assert!(ids.iter().any(|id| id == "toggle_float_bar"), "float bar row");
        assert!(ids.iter().any(|id| id == "about"), "about row");
    }

    /// Inserting mid-list shifts every row after it, and a row's id *is* its
    /// position — so if the renumber is ever dropped, clicking 设置 fires 关于.
    #[test]
    fn inserting_the_strip_toggle_renumbers_every_row() {
        use crate::taskbar_menu::MenuItem;

        let mut items = vec![
            MenuItem::action(1, "Refresh".into()),
            MenuItem::action(2, "Float bar".into()),
            MenuItem::action(3, "Settings".into()),
            MenuItem::action(4, "Quit".into()),
        ];
        let mut ids = vec![
            "refresh".to_string(),
            "toggle_float_bar".to_string(),
            "settings".to_string(),
            "quit".to_string(),
        ];

        insert_strip_toggle(&mut items, &mut ids, "Show strip".into(), true);

        // Placed beside the other visibility toggle, not at the end.
        assert_eq!(ids[2], STRIP_TOGGLE_ID);
        assert_eq!(ids.last().map(String::as_str), Some("quit"));
        assert!(items[2].checked);

        assert_eq!(items.len(), ids.len());
        for (position, item) in items.iter().enumerate() {
            assert_eq!(item.id, position + 1, "row {position} carries a stale id");
        }
    }

    /// With no floating-bar row to anchor to, the toggle still has to land
    /// somewhere valid rather than being dropped.
    #[test]
    fn the_strip_toggle_falls_back_to_the_end() {
        use crate::taskbar_menu::MenuItem;

        let mut items = vec![MenuItem::action(1, "Quit".into())];
        let mut ids = vec!["quit".to_string()];
        insert_strip_toggle(&mut items, &mut ids, "Show strip".into(), false);

        assert_eq!(ids.last().map(String::as_str), Some(STRIP_TOGGLE_ID));
        assert_eq!(items.last().map(|item| item.id), Some(2));
    }
}
