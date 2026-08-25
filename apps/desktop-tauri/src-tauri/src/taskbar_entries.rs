//! Resolving the user's ordered taskbar entries into printable strip lines.
//!
//! The taskbar strip used to render one fixed shape ("provider session% " plus
//! "weekly%"). It now renders whatever ordered `provider + window` list the user
//! composed, so this module answers one question per entry: *what text should
//! this line show, and is it real data?*
//!
//! # Never a silent zero
//!
//! An entry can point at something that does not exist right now — a provider
//! the user later disabled, a weekly window a provider does not publish, a
//! balance on a subscription account. Rendering `0%` for those would be a
//! fabricated measurement, which the task package explicitly forbids. Every
//! unresolvable entry therefore carries an explicit reason instead, and the
//! caller renders that reason rather than a number.

use codexbar::settings::{Settings, TASKBAR_PROVIDER_AUTO};

use crate::commands::{ProviderUsageSnapshot, RateWindowSnapshot};

/// Why an entry could not be rendered as a percentage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryUnavailable {
    /// The named provider is not in the enabled set.
    ProviderDisabled,
    /// The provider is enabled but produced no snapshot this cycle.
    NoData,
    /// The provider's last fetch failed.
    ProviderError,
    /// The provider does not publish this window at all.
    WindowUnsupported,
}

/// One resolved line of the strip.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedEntry {
    /// The provider this resolved to, so the caller can look up its brand mark.
    /// Stays `auto` when the entry follows the tray and nothing was picked —
    /// that has no mark, and the caller falls back to printing the label.
    pub provider_id: String,
    /// Short provider label, e.g. `Codex`.
    pub provider_label: String,
    /// Which window, as the caller's localized label.
    pub window: String,
    /// The window kind (`session|weekly|daily|monthly|balance|speed|primary`),
    /// used by the renderer's preview/state model.
    pub window_kind: String,
    /// The percentage to print, when the entry resolved to real data.
    pub percent: Option<f64>,
    /// A prepaid balance, already formatted (`¥38.88`), for `balance` entries.
    ///
    /// Separate from `percent` because it is not one: it carries a currency and
    /// no denominator, so it can never drive a bar or a threshold colour. The
    /// strip prints it verbatim.
    pub amount: Option<String>,
    /// Set when both values are `None`; the caller renders a reason, not a zero.
    pub unavailable: Option<EntryUnavailable>,
}

impl ResolvedEntry {
    fn unavailable(
        provider_id: impl Into<String>,
        provider_label: impl Into<String>,
        window: impl Into<String>,
        window_kind: impl Into<String>,
        reason: EntryUnavailable,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            provider_label: provider_label.into(),
            window: window.into(),
            window_kind: window_kind.into(),
            percent: None,
            amount: None,
            unavailable: Some(reason),
        }
    }
}

/// The prepaid balance a provider carries, formatted for one strip cell.
///
/// Balance providers have no structured field for this: each stuffs a
/// human-readable string into a rate window's `reset_description`, which is
/// semantically meant for reset text — DeepSeek uses `primary`, MiMo uses
/// `secondary`. Rather than hardcode that per-provider map (which drifts every
/// time a provider is added), every window is scanned and the first one whose
/// description is shaped like money wins.
///
/// Only the leading amount is kept. The full text is
/// `"¥38.88 (Paid: ¥38.88 / Granted: ¥0.00)"`; a strip cell has room for the
/// number, and the breakdown is what the dashboard card is for.
///
/// Mirrors `lib/providerBalance.ts::parseBalanceText`, which does the same job
/// for the React surfaces. Kept deliberately narrower: that one also classifies
/// status sentences and unavailable states, while the strip only ever needs
/// "is there an amount, and what is it".
pub fn balance_amount(snapshot: &ProviderUsageSnapshot) -> Option<String> {
    let windows = [
        Some(&snapshot.primary),
        snapshot.secondary.as_ref(),
        snapshot.tertiary.as_ref(),
    ];
    windows
        .into_iter()
        .flatten()
        .filter_map(|w| w.reset_description.as_deref())
        .find_map(parse_balance_amount)
}

fn parse_balance_amount(raw: &str) -> Option<String> {
    let text = raw.trim();
    // "Balance unavailable for API calls" is a real state, but it is not an
    // amount — reporting it as one would print a number that does not exist.
    if text.is_empty()
        || text.to_ascii_lowercase().contains("unavailable")
        || text.contains("余额不可用")
    {
        return None;
    }

    // The amount is the leading token, before the "(Paid: …)" breakdown, an
    // em-dash aside, or a trailing " balance".
    let mut head = text;
    for cut in ['(', '（'] {
        if let Some(index) = head.find(cut) {
            head = &head[..index];
        }
    }
    if let Some(index) = head.find(" — ") {
        head = &head[..index];
    }
    if let Some(index) = head.to_ascii_lowercase().find(" balance") {
        head = &head[..index];
    }
    let head = head.trim().trim_start_matches("CNY").trim();

    // Must actually contain a digit: a plan name or a status sentence must not
    // be promoted to a balance just because it sat in the same field.
    if !head.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    // Reject a bare number with no currency at all — that is far more likely to
    // be a token count than money.
    let normalized = head.replace('￥', "¥");
    let has_currency = normalized.contains('¥')
        || normalized.contains('$')
        || normalized.to_ascii_uppercase().contains("CNY")
        || normalized.to_ascii_uppercase().contains("USD");
    if !has_currency {
        return None;
    }
    Some(normalized)
}

/// Which window kinds this provider can actually answer for, right now.
///
/// The composer's window dropdown is built from this rather than from the full
/// list of kinds, so a provider never offers a choice that resolves to
/// "unsupported" — Grok publishes only a monthly window, and DeepSeek is a
/// prepaid balance with no percentage quota at all.
///
/// Deliberately shares `window_by_kind` and [`balance_amount`] with
/// [`resolve_entries`]: if availability were computed independently the menu and
/// the strip could disagree, which is a worse failure than the one it fixes.
pub fn available_windows(
    snapshot: &ProviderUsageSnapshot,
    has_speed: bool,
) -> Vec<&'static str> {
    let mut out = Vec::new();
    // `primary` is the escape hatch, not a synonym. It is offered only when the
    // provider's main window could not be identified as a named cycle —
    // otherwise the menu would ask the user to choose between "月额度" and
    // "主额度" for one identical reading, which is the confusion that made the
    // option worth complaining about.
    if primary_window(snapshot).is_some_and(|window| window.kind.is_none()) {
        out.push("primary");
    }
    for kind in ["session", "daily", "weekly", "monthly"] {
        if window_by_kind(snapshot, kind).is_some() {
            out.push(kind);
        }
    }
    if balance_amount(snapshot).is_some() {
        out.push("balance");
    }
    if has_speed {
        out.push("speed");
    }
    out
}

/// Pick the rate window an entry names out of a provider's snapshot.
///
/// Providers disagree on which slot carries which cycle — Codex reports its
/// weekly quota as `primary` while Claude's `primary` is the 5-hour session — so
/// selection is by what the window IS, never by slot position. Reading slots
/// positionally is exactly the bug that once made Claude's session usage render
/// as its weekly figure.
///
/// What a window is comes from its `kind`, decided once at the bridge (see
/// [`crate::quota_cycle`]). This module used to derive it here, which is how it
/// drifted from the two TypeScript copies doing the same job.
fn window_by_kind<'a>(
    snapshot: &'a ProviderUsageSnapshot,
    kind: &str,
) -> Option<&'a RateWindowSnapshot> {
    if kind == "primary" {
        return primary_window(snapshot);
    }
    real_windows(snapshot)
        .into_iter()
        .find(|window| window.kind == Some(kind))
}

/// The window a `primary` entry resolves to: this provider's main quota,
/// whatever cycle it turns out to be.
///
/// Skips balance carriers. A prepaid provider synthesises a 0%/100% window
/// purely to smuggle its amount through `reset_description`, and it is not
/// flagged informational, so a length-agnostic search would happily print "0%" —
/// a fabricated measurement, which is exactly what this module exists to
/// prevent. Detected by the shape of the text, not by a provider list, so a new
/// prepaid provider is covered on arrival.
fn primary_window(snapshot: &ProviderUsageSnapshot) -> Option<&RateWindowSnapshot> {
    real_windows(snapshot).into_iter().find(|window| {
        window
            .reset_description
            .as_deref()
            .and_then(parse_balance_amount)
            .is_none()
    })
}

/// Every rate window a provider actually publishes, in slot order.
fn real_windows(snapshot: &ProviderUsageSnapshot) -> Vec<&RateWindowSnapshot> {
    let mut all: Vec<&RateWindowSnapshot> = vec![&snapshot.primary];
    all.extend(snapshot.secondary.as_ref());
    all.extend(snapshot.tertiary.as_ref());
    all.extend(snapshot.extra_rate_windows.iter().map(|extra| &extra.window));
    all.retain(|window| !window.is_informational);
    all
}

/// Resolve every configured entry, in order.
///
/// `picked` is the provider the tray icon settled on; entries using the `auto`
/// provider follow it so the pre-entries default keeps behaving identically.
pub fn resolve_entries(
    settings: &Settings,
    snapshots: &[ProviderUsageSnapshot],
    picked: Option<&ProviderUsageSnapshot>,
    window_label: &dyn Fn(&str) -> String,
    speed_for: &dyn Fn(&str) -> Option<f64>,
) -> Vec<ResolvedEntry> {
    settings
        .taskbar_widget_entries
        .iter()
        .map(|entry| {
            let label = window_label(&entry.window);

            let snapshot = if entry.provider_id == TASKBAR_PROVIDER_AUTO {
                picked
            } else {
                snapshots
                    .iter()
                    .find(|s| s.provider_id.eq_ignore_ascii_case(&entry.provider_id))
            };

            let Some(snapshot) = snapshot else {
                // Distinguish "you turned this provider off" from "it is on but
                // has not reported yet": the first is the user's own doing and
                // actionable, the second resolves by itself.
                let named_but_disabled = entry.provider_id != TASKBAR_PROVIDER_AUTO
                    && !settings
                        .enabled_providers
                        .iter()
                        .any(|p| p.eq_ignore_ascii_case(&entry.provider_id));
                let reason = if named_but_disabled {
                    EntryUnavailable::ProviderDisabled
                } else {
                    EntryUnavailable::NoData
                };
                return ResolvedEntry::unavailable(
                    entry.provider_id.clone(),
                    entry.provider_id.clone(),
                    label,
                    entry.window.clone(),
                    reason,
                );
            };

            if snapshot.error.is_some() {
                return ResolvedEntry::unavailable(
                    snapshot.provider_id.clone(),
                    snapshot.display_name.clone(),
                    label,
                    entry.window.clone(),
                    EntryUnavailable::ProviderError,
                );
            }

            if entry.window == "speed" {
                return match speed_for(&snapshot.provider_id) {
                    Some(value) => ResolvedEntry {
                        provider_id: snapshot.provider_id.clone(),
                        provider_label: snapshot.display_name.clone(),
                        window: label,
                        window_kind: entry.window.clone(),
                        percent: Some(value),
                        amount: None,
                        unavailable: None,
                    },
                    None => ResolvedEntry::unavailable(
                        snapshot.provider_id.clone(),
                        snapshot.display_name.clone(),
                        label,
                        entry.window.clone(),
                        EntryUnavailable::WindowUnsupported,
                    ),
                };
            }

            if entry.window == "balance" {
                // Money, not a percentage — so it travels in `amount` and the
                // strip prints it verbatim. This used to be refused outright on
                // the grounds that "the strip prints percentages", which made
                // the dropdown's own 余额 option unusable for every provider
                // and left balance accounts like DeepSeek with no strip reading
                // at all, even though the floating bar shows theirs.
                return match balance_amount(snapshot) {
                    Some(amount) => ResolvedEntry {
                        provider_id: snapshot.provider_id.clone(),
                        provider_label: snapshot.display_name.clone(),
                        window: label,
                        window_kind: entry.window.clone(),
                        percent: None,
                        amount: Some(amount),
                        unavailable: None,
                    },
                    None => ResolvedEntry::unavailable(
                        snapshot.provider_id.clone(),
                        snapshot.display_name.clone(),
                        label,
                        entry.window.clone(),
                        EntryUnavailable::WindowUnsupported,
                    ),
                };
            }

            match window_by_kind(snapshot, &entry.window) {
                // A `primary` entry names itself after the cycle it landed on,
                // so the strip reads "◆ 周 12%" rather than the useless word
                // "primary" — and stays unlabelled ("◆ 12%") only when neither
                // the length nor the provider's own slot name states a cycle.
                Some(window) if entry.window == "primary" => ResolvedEntry {
                    provider_id: snapshot.provider_id.clone(),
                    provider_label: snapshot.display_name.clone(),
                    window: window.kind.map(window_label).unwrap_or_default(),
                    window_kind: "primary".to_string(),
                    amount: None,
                    percent: Some(if settings.taskbar_show_as_used {
                        window.used_percent
                    } else {
                        (100.0 - window.used_percent).clamp(0.0, 100.0)
                    }),
                    unavailable: None,
                },
                Some(window) => ResolvedEntry {
                    provider_id: snapshot.provider_id.clone(),
                    provider_label: snapshot.display_name.clone(),
                    window: label,
                    window_kind: entry.window.clone(),
                    amount: None,
                    percent: Some(if settings.taskbar_show_as_used {
                        window.used_percent
                    } else {
                        (100.0 - window.used_percent).clamp(0.0, 100.0)
                    }),
                    unavailable: None,
                },
                None => ResolvedEntry::unavailable(
                    snapshot.provider_id.clone(),
                    snapshot.display_name.clone(),
                    label,
                    entry.window.clone(),
                    EntryUnavailable::WindowUnsupported,
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codexbar::settings::TaskbarEntry;

    fn window(used: f64, minutes: Option<u32>) -> RateWindowSnapshot {
        labelled_window(used, minutes, None)
    }

    /// A window as the bridge builds it: the cycle is already decided, from the
    /// declared length and the provider's own name for the slot. Tests classify
    /// through the same function the bridge uses rather than asserting a `kind`
    /// by hand, so a fixture can never claim a cycle the real code would not.
    fn labelled_window(
        used: f64,
        minutes: Option<u32>,
        label: Option<&str>,
    ) -> RateWindowSnapshot {
        RateWindowSnapshot {
            used_percent: used,
            remaining_percent: 100.0 - used,
            kind: crate::quota_cycle::classify(minutes, label),
            window_minutes: minutes,
            resets_at: None,
            reset_description: None,
            is_exhausted: false,
            is_informational: false,
            reserve_percent: None,
            reserve_description: None,
            reserve_will_last_to_reset: false,
            reserve_eta_seconds: None,
        }
    }

    fn test_capabilities(id: &str) -> codexbar::core::ProviderCapabilities {
        let provider_id = codexbar::core::ProviderId::from_cli_name(id)
            .unwrap_or(codexbar::core::ProviderId::Codex);
        let provider_metadata = codexbar::core::ProviderMetadata {
            id: provider_id,
            display_name: "test",
            session_label: "Session",
            weekly_label: "Weekly",
            supports_opus: false,
            supports_credits: false,
            default_enabled: false,
            is_primary: false,
            dashboard_url: None,
            status_page_url: None,
        };
        codexbar::core::ProviderCapabilities::for_provider(provider_id, &provider_metadata)
    }

    fn snapshot(id: &str, name: &str) -> ProviderUsageSnapshot {
        ProviderUsageSnapshot {
            provider_id: id.to_string(),
            display_name: name.to_string(),
            primary: window(10.0, Some(300)),
            primary_label: None,
            secondary: Some(window(40.0, Some(10080))),
            secondary_label: None,
            model_specific: None,
            tertiary: None,
            extra_rate_windows: Vec::new(),
            cost: None,
            plan_name: None,
            account_email: None,
            source_label: "auto".into(),
            updated_at: "2026-07-31T00:00:00Z".into(),
            error: None,
            pace: None,
            account_organization: None,
            tray_status_label: None,
            fetch_duration_ms: None,
            wayfinder_usage: None,
            capabilities: test_capabilities(id),
        }
    }

    fn settings_with(entries: Vec<(&str, &str)>) -> Settings {
        let mut s = Settings::default();
        s.taskbar_widget_entries = entries
            .into_iter()
            .map(|(p, w)| TaskbarEntry {
                provider_id: p.to_string(),
                window: w.to_string(),
            })
            .collect();
        s
    }

    fn label(kind: &str) -> String {
        kind.to_string()
    }

    fn no_speed(_: &str) -> Option<f64> {
        None
    }

    /// Windows are matched by declared LENGTH, not slot. Claude keeps its weekly
    /// quota in `secondary`; asking for "weekly" must not return `primary`.
    #[test]
    fn selects_windows_by_length_not_slot() {
        let snaps = vec![snapshot("claude", "Claude")];
        let settings = settings_with(vec![("claude", "session"), ("claude", "weekly")]);
        let out = resolve_entries(&settings, &snaps, None, &label, &no_speed);

        assert_eq!(out[0].percent, Some(10.0), "session came from the 300m window");
        assert_eq!(out[1].percent, Some(40.0), "weekly came from the 10080m window");
    }

    /// The whole point of the reason codes: a provider that does not publish the
    /// requested window must say so rather than render a convincing 0%.
    #[test]
    fn unsupported_window_is_reported_not_zeroed() {
        let snaps = vec![snapshot("codex", "Codex")];
        let settings = settings_with(vec![("codex", "monthly")]);
        let out = resolve_entries(&settings, &snaps, None, &label, &no_speed);

        assert_eq!(out[0].percent, None);
        assert_eq!(out[0].unavailable, Some(EntryUnavailable::WindowUnsupported));
    }

    #[test]
    fn disabled_provider_is_distinguished_from_missing_data() {
        let settings = settings_with(vec![("grok", "session")]);
        // `grok` is not in the default enabled set, so this is the user's doing.
        let out = resolve_entries(&settings, &[], None, &label, &no_speed);
        assert_eq!(out[0].unavailable, Some(EntryUnavailable::ProviderDisabled));

        let mut enabled = settings_with(vec![("grok", "session")]);
        enabled.enabled_providers.insert("grok".to_string());
        let out = resolve_entries(&enabled, &[], None, &label, &no_speed);
        assert_eq!(
            out[0].unavailable,
            Some(EntryUnavailable::NoData),
            "enabled but silent is a different situation from switched off"
        );
    }

    #[test]
    fn auto_provider_follows_the_tray_pick() {
        let snaps = vec![snapshot("codex", "Codex")];
        let settings = settings_with(vec![(TASKBAR_PROVIDER_AUTO, "session")]);
        let out = resolve_entries(&settings, &snaps, Some(&snaps[0]), &label, &no_speed);
        assert_eq!(out[0].provider_label, "Codex");
        assert_eq!(out[0].percent, Some(10.0));
    }

    /// Entries are rendered in the order configured, because the strip truncates
    /// from the end — order is how the user says what matters most.
    #[test]
    fn preserves_configured_order() {
        let snaps = vec![snapshot("codex", "Codex")];
        let settings = settings_with(vec![("codex", "weekly"), ("codex", "session")]);
        let out = resolve_entries(&settings, &snaps, None, &label, &no_speed);
        assert_eq!(out[0].window, "weekly");
        assert_eq!(out[1].window, "session");
    }

    /// The taskbar's own used/remaining choice drives the number, exactly like
    /// every other surface's.
    #[test]
    fn honors_the_taskbar_used_versus_remaining_setting() {
        let snaps = vec![snapshot("codex", "Codex")];
        let mut settings = settings_with(vec![("codex", "session")]);
        settings.taskbar_show_as_used = false;
        let out = resolve_entries(&settings, &snaps, None, &label, &no_speed);
        assert_eq!(out[0].percent, Some(90.0));
    }

    /// A balance is money, not a percentage; the strip prints percentages.
    #[test]
    fn balance_is_unsupported_rather_than_coerced_into_a_percent() {
        let snaps = vec![snapshot("deepseek", "DeepSeek")];
        let settings = settings_with(vec![("deepseek", "balance")]);
        let out = resolve_entries(&settings, &snaps, None, &label, &no_speed);
        assert_eq!(out[0].unavailable, Some(EntryUnavailable::WindowUnsupported));
    }

    #[test]
    fn failed_provider_reports_its_error_state() {
        let mut snap = snapshot("codex", "Codex");
        snap.error = Some("boom".into());
        let settings = settings_with(vec![("codex", "session")]);
        let out = resolve_entries(&settings, &[snap], None, &label, &no_speed);
        assert_eq!(out[0].unavailable, Some(EntryUnavailable::ProviderError));
    }

    /// A prepaid-balance account, encoded the way DeepSeek encodes it: a
    /// synthetic percentage window whose `reset_description` carries the money.
    fn balance_snapshot(id: &str, description: &str) -> ProviderUsageSnapshot {
        let mut snap = snapshot(id, id);
        snap.primary = window(0.0, None);
        snap.primary.reset_description = Some(description.to_string());
        snap.secondary = None;
        snap
    }

    #[test]
    fn a_balance_entry_prints_the_amount_instead_of_reporting_unsupported() {
        let snap = balance_snapshot("deepseek", "\u{a5}38.88 (Paid: \u{a5}38.88 / Granted: \u{a5}0.00)");
        let mut settings = Settings::default();
        settings.taskbar_widget_entries = vec![codexbar::settings::TaskbarEntry {
            provider_id: "deepseek".into(),
            window: "balance".into(),
        }];
        settings.enabled_providers = ["deepseek".to_string()].into_iter().collect();

        let out = resolve_entries(&settings, &[snap], None, &label, &no_speed);
        assert_eq!(out.len(), 1);
        // The breakdown is dropped: a strip cell fits the number, not the
        // "(Paid: ... / Granted: ...)" tail.
        assert_eq!(out[0].amount.as_deref(), Some("\u{a5}38.88"));
        assert_eq!(out[0].percent, None, "money must never become a percentage");
        assert_eq!(out[0].unavailable, None);
    }

    /// The floating bar shows these accounts fine, so the strip refusing them
    /// was the defect — but an account with no balance at all still must not
    /// invent one.
    #[test]
    fn a_balance_entry_on_an_account_without_one_stays_unsupported() {
        let snap = snapshot("codex", "Codex");
        let mut settings = Settings::default();
        settings.taskbar_widget_entries = vec![codexbar::settings::TaskbarEntry {
            provider_id: "codex".into(),
            window: "balance".into(),
        }];
        settings.enabled_providers = ["codex".to_string()].into_iter().collect();

        let out = resolve_entries(&settings, &[snap], None, &label, &no_speed);
        assert_eq!(out[0].amount, None);
        assert_eq!(out[0].unavailable, Some(EntryUnavailable::WindowUnsupported));
    }

    #[test]
    fn balance_parsing_rejects_everything_that_is_not_money() {
        // Real encodings, from `lib/providerBalance.ts`.
        assert_eq!(
            parse_balance_amount("\u{a5}38.88 (Paid: \u{a5}38.88 / Granted: \u{a5}0.00)").as_deref(),
            Some("\u{a5}38.88")
        );
        assert_eq!(
            parse_balance_amount("12.50 CNY balance (Paid: 8.25 CNY / Granted: 4.25 CNY)").as_deref(),
            Some("12.50 CNY")
        );
        // Full-width yen normalises to the same symbol the rest of the UI uses.
        assert_eq!(parse_balance_amount("\u{ffe5}5.00 balance").as_deref(), Some("\u{a5}5.00"));

        // "Unavailable" is a state, not an amount \u{2014} printing a number here would
        // be inventing one.
        assert_eq!(parse_balance_amount("Balance unavailable for API calls"), None);
        // Reset prose that happens to share the field.
        assert_eq!(parse_balance_amount("Resets on the 1st"), None);
        // A bare count with no currency is far likelier to be tokens than money.
        assert_eq!(parse_balance_amount("120000"), None);
        assert_eq!(parse_balance_amount(""), None);
    }

    /// What the composer's dropdown is built from. A provider that publishes
    /// one cycle and nothing else is the case that made the menu offer "\u{5468}"
    /// and the strip then print "\u{4e0d}\u{652f}\u{6301}".
    #[test]
    fn available_windows_lists_only_what_the_provider_publishes() {
        let mut single = snapshot("single", "Single");
        single.primary = window(20.0, Some(30 * 24 * 60));
        single.secondary = None;
        assert_eq!(available_windows(&single, false), vec!["monthly"]);
        // Speed is not a window the provider publishes; it comes from the local
        // logs, so it is offered only once something has actually been measured.
        assert_eq!(available_windows(&single, true), vec!["monthly", "speed"]);

        // The default fixture has a 5-hour primary and a weekly secondary.
        let codex = snapshot("codex", "Codex");
        assert_eq!(available_windows(&codex, false), vec!["session", "weekly"]);

        // A balance account offers its balance and nothing else.
        let deepseek = balance_snapshot("deepseek", "\u{a5}38.88 (Paid: \u{a5}38.88 / Granted: \u{a5}0.00)");
        // NOT "primary": its 0% window is a balance carrier, and printing that 0%
        // would be a fabricated measurement.
        assert_eq!(available_windows(&deepseek, false), vec!["balance"]);
    }

    /// The menu and the strip must never disagree: every kind offered has to
    /// resolve, and every kind withheld has to be one that would not have.
    ///
    /// `primary` is the one deliberate exception to the second half. It always
    /// resolves, but it is withheld whenever a named cycle already describes the
    /// same window — the menu should say "月额度", not offer that and "主额度"
    /// side by side for one identical reading. Withholding it never breaks a
    /// stored entry, which is why the first half is still asserted for it.
    #[test]
    fn every_offered_window_actually_resolves() {
        let providers = [
            snapshot("codex", "Codex"),
            balance_snapshot("deepseek", "\u{a5}38.88 (Paid: \u{a5}38.88 / Granted: \u{a5}0.00)"),
        ];
        for snap in &providers {
            for kind in ["primary", "session", "daily", "weekly", "monthly", "balance"] {
                let offered = available_windows(snap, false).contains(&kind);
                let mut settings = Settings::default();
                settings.taskbar_widget_entries = vec![codexbar::settings::TaskbarEntry {
                    provider_id: snap.provider_id.clone(),
                    window: kind.to_string(),
                }];
                settings.enabled_providers =
                    [snap.provider_id.clone()].into_iter().collect();
                let out = resolve_entries(
                    &settings,
                    std::slice::from_ref(snap),
                    None,
                    &label,
                    &no_speed,
                );
                let resolved = out[0].unavailable.is_none();
                if offered {
                    assert!(
                        resolved,
                        "{} / {kind}: offered by the menu but does not resolve",
                        snap.provider_id
                    );
                } else if kind != "primary" {
                    assert!(
                        !resolved,
                        "{} / {kind}: resolves but the menu never offers it",
                        snap.provider_id
                    );
                }
            }
        }
    }

    /// Builds a one-provider settings/snapshot pair and resolves it.
    fn resolve_one(snap: ProviderUsageSnapshot, kind: &str) -> ResolvedEntry {
        let mut settings = Settings::default();
        settings.taskbar_widget_entries = vec![TaskbarEntry {
            provider_id: snap.provider_id.clone(),
            window: kind.to_string(),
        }];
        settings.enabled_providers = [snap.provider_id.clone()].into_iter().collect();
        resolve_entries(&settings, &[snap], None, &label, &no_speed)
            .into_iter()
            .next()
            .expect("one entry in, one entry out")
    }

    /// The shape of the bug that started this: a provider publishes a
    /// percentage but no length, and a length was all the matcher looked at, so
    /// *nothing* matched and every window kind read 不支持.
    ///
    /// A provider in that state is not silent about the answer — it still names
    /// the slot. Grok was the reported case and has since been fixed at source
    /// to declare its cycle, but the class survives it: any provider whose
    /// length comes from an optional field lands here the moment that field is
    /// missing.
    #[test]
    fn a_window_with_no_declared_length_is_identified_by_its_provider_label() {
        let mut snap = snapshot("p", "P");
        snap.primary = labelled_window(42.0, None, Some("Monthly"));
        snap.primary_label = Some("Monthly".into());
        snap.secondary = None;

        assert_eq!(
            available_windows(&snap, false),
            vec!["monthly"],
            "the menu offers the cycle the provider names"
        );

        let out = resolve_one(snap, "monthly");
        assert_eq!(out.unavailable, None, "and choosing it must resolve");
        assert_eq!(out.percent, Some(42.0));
    }

    /// A measured length outranks a hand-written constant, because the constant
    /// is the one that can go stale.
    #[test]
    fn a_declared_length_outranks_the_slot_name() {
        let mut snap = snapshot("p", "P");
        snap.primary = labelled_window(42.0, Some(300), Some("Monthly"));
        snap.primary_label = Some("Monthly".into());
        snap.secondary = None;

        assert_eq!(available_windows(&snap, false), vec!["session"]);
    }

    /// `primary` exists for the windows that genuinely cannot be named, and is
    /// withheld the moment one can be — otherwise the menu offers two options
    /// for one reading and the user has to guess which is which.
    #[test]
    fn primary_is_offered_only_when_no_cycle_can_be_named() {
        // Named by length, named by slot name: no `primary` either time.
        let codex = snapshot("codex", "Codex");
        assert!(!available_windows(&codex, false).contains(&"primary"));

        let mut named = snapshot("named", "Named");
        named.primary = labelled_window(42.0, None, Some("Monthly"));
        named.primary_label = Some("Monthly".into());
        named.secondary = None;
        assert!(!available_windows(&named, false).contains(&"primary"));

        // Neither source answers: a percentage with no length, in a slot the
        // provider calls "Credits". Something still has to be offered, or the
        // provider would have no strip reading at all.
        let mut unnamed = snapshot("mystery", "Mystery");
        unnamed.primary = labelled_window(42.0, None, Some("Credits"));
        unnamed.primary_label = Some("Credits".into());
        unnamed.secondary = None;
        assert_eq!(available_windows(&unnamed, false), vec!["primary"]);

        // Same when the cycle is real but fits no band.
        let mut fortnightly = snapshot("fortnight", "Fortnight");
        fortnightly.primary = window(42.0, Some(14 * 24 * 60));
        fortnightly.primary_label = None;
        fortnightly.secondary = None;
        assert_eq!(available_windows(&fortnightly, false), vec!["primary"]);
    }

    /// Withholding `primary` from the menu must not break an entry that already
    /// stored it. Rewriting a user's saved configuration behind their back was
    /// rejected, so the old value has to keep working.
    #[test]
    fn a_stored_primary_entry_still_resolves_and_names_itself() {
        let cases: [(Option<u32>, Option<&str>, &str); 5] = [
            (Some(300), None, "session"),
            (Some(24 * 60), None, "daily"),
            (Some(7 * 24 * 60), None, "weekly"),
            // Length absent, slot name answers instead.
            (None, Some("Monthly"), "monthly"),
            // Neither answers: unlabelled ("◆ 42%") rather than mislabelled.
            (Some(14 * 24 * 60), Some("Credits"), ""),
        ];

        for (minutes, slot, expected) in cases {
            let mut snap = snapshot("p", "P");
            snap.primary = labelled_window(42.0, minutes, slot);
            snap.primary_label = slot.map(str::to_string);
            snap.secondary = None;

            let out = resolve_one(snap, "primary");
            assert_eq!(out.unavailable, None, "{minutes:?}/{slot:?} must resolve");
            assert_eq!(out.percent, Some(42.0), "{minutes:?}/{slot:?}");
            assert_eq!(out.window, expected, "{minutes:?}/{slot:?}");
        }
    }

    /// The split cell model: `window_kind` mirrors each entry's kind, and a real
    /// percent / a balance amount / an unavailable reason all carry their
    /// distinct payload for the renderer's tag+value mapping.
    #[test]
    fn resolved_entries_carry_the_split_cell_payload() {
        // percent + kind = session
        let snaps = vec![snapshot("codex", "Codex")];
        let s = settings_with(vec![("codex", "session"), ("codex", "balance")]);
        let out = resolve_entries(&s, &snaps, None, &label, &no_speed);
        assert_eq!(out.len(), 2);
        // Session resolves to a real percent.
        assert_eq!(out[0].window_kind, "session");
        assert_eq!(out[0].percent, Some(10.0));
        assert_eq!(out[0].amount, None);
        assert_eq!(out[0].unavailable, None);
        // Balance on a provider with no balance record is unsupported (never a
        // fabricated amount).
        assert_eq!(out[1].window_kind, "balance");
        assert_eq!(out[1].amount, None);
        assert_eq!(out[1].unavailable, Some(EntryUnavailable::WindowUnsupported));

        // A disabled provider maps to the notConfigured-style unavailable.
        let mut disabled = settings_with(vec![("grok", "session")]);
        let out2 = resolve_entries(&disabled, &[], None, &label, &no_speed);
        assert_eq!(out2[0].window_kind, "session");
        assert_eq!(out2[0].unavailable, Some(EntryUnavailable::ProviderDisabled));
    }

}
