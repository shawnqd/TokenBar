//! Which cycle a quota window belongs to — answered once, for everybody.
//!
//! # Why this module exists
//!
//! Three surfaces show quota windows: the native taskbar strip, the floating
//! bar, and the dashboard/flyout card. The bridge used to hand each of them the
//! raw `window_minutes` and let every one work out for itself that "10080
//! minutes" means "weekly". The same arithmetic therefore existed three times —
//! once here in Rust and twice in TypeScript (`lib/quotaWindows.ts` and
//! `MenuCard.tsx`) — and the copies had already drifted:
//!
//!   * the TypeScript copies matched on length ALONE, so a provider that
//!     publishes a percentage without a declared length matched no cycle at all
//!     and the floating bar showed nothing where the strip showed a reading;
//!   * `MenuCard`'s weekly test was `minutes >= 7 days` with no upper bound, so
//!     a monthly window also counted as weekly and collected the weekly
//!     forecast — the strip called the same window "月".
//!
//! Guarding those copies against each other was considered and rejected: a
//! guard concedes that the duplication exists and only promises the copies stay
//! equal. Classifying once at the bridge and shipping the ANSWER removes the
//! second and third copies outright, so there is nothing left to keep in step.
//!
//! The objection to doing this earlier was that the strip is painted natively
//! and the bar in a WebView, so neither can call the other *at paint time*.
//! True, and beside the point: classification does not happen at paint time. It
//! happens once, when the snapshot is built, while only Rust is running.
//!
//! # What it is not
//!
//! Not a taxonomy every window must fit. A cycle in no band — a fortnight — and
//! a slot that is not a dated cycle at all — "Credits", "Balance" — both return
//! `None`, which is the honest answer. Callers render such a window without a
//! cycle word rather than rounding it to the nearest one.

/// Minutes below which a window counts as a session rather than a longer cycle.
const SESSION_MAX_MINUTES: u32 = 6 * 60;
const DAILY_MIN_MINUTES: u32 = 20 * 60;
const DAILY_MAX_MINUTES: u32 = 28 * 60;
const WEEKLY_MIN_MINUTES: u32 = 6 * 24 * 60;
const WEEKLY_MAX_MINUTES: u32 = 8 * 24 * 60;
const MONTHLY_MIN_MINUTES: u32 = 27 * 24 * 60;

/// Which cycle a window belongs to, from the two things a provider can tell us.
///
/// 1. `window_minutes`, the length the provider declares. Precise, but optional
///    — several providers derive it from a field their API only sometimes
///    returns, and it is simply absent when that field is missing.
/// 2. The provider's own name for the slot — `ProviderMetadata::session_label`
///    and `weekly_label`, which every provider fills in because they are plain
///    struct fields the compiler will not let a new provider omit.
///
/// Length wins when both are present: it is per-response data, while the label
/// is a compile-time constant that can go stale. Both are declarations, so when
/// they disagree the provider has a bug.
///
/// Asking source 2 is what stops this being a per-provider fix. Every provider
/// already had to name its windows to render a dashboard card at all, so a
/// provider added tomorrow arrives with its answer already filled in.
pub fn classify(window_minutes: Option<u32>, label: Option<&str>) -> Option<&'static str> {
    kind_from_minutes(window_minutes).or_else(|| label.and_then(kind_from_label))
}

/// The band a declared length falls into.
///
/// A cycle that fits no band — a fortnight, say — is real data that simply has
/// no short name, so it stays unnamed rather than being rounded to the nearest
/// word. Note the bands are CLOSED: "weekly" tops out at 8 days, so a monthly
/// window can never answer to it.
fn kind_from_minutes(minutes: Option<u32>) -> Option<&'static str> {
    Some(match minutes? {
        m if m <= SESSION_MAX_MINUTES => "session",
        m if (DAILY_MIN_MINUTES..=DAILY_MAX_MINUTES).contains(&m) => "daily",
        m if (WEEKLY_MIN_MINUTES..=WEEKLY_MAX_MINUTES).contains(&m) => "weekly",
        m if m >= MONTHLY_MIN_MINUTES => "monthly",
        _ => return None,
    })
}

/// The cycle a provider's own slot name states.
///
/// Two shapes appear across `ProviderMetadata`: a spelled-out length ("5-hour",
/// "7-Day", "Session (5h)") and a bare cycle word ("Monthly", "Weekly cost",
/// "Daily AFP"). A stated length is converted to minutes and run through the
/// same bands as a declared one, so a label and a length can never disagree
/// about where a boundary sits — and "7-Day" is a week, not a day.
///
/// Names that state no cycle — "Credits", "Balance", "On-demand", "Gemini Pro" —
/// return `None`. That is the correct answer rather than a failure: those slots
/// genuinely are not a dated cycle, and guessing one would put a wrong word on
/// screen.
fn kind_from_label(label: &str) -> Option<&'static str> {
    if let Some(minutes) = minutes_from_label(label) {
        // A stated length is the whole answer, including when it names no band.
        return kind_from_minutes(Some(minutes));
    }
    let lower = label.to_ascii_lowercase();
    if lower.contains("month") {
        Some("monthly")
    } else if lower.contains("week") {
        Some("weekly")
    // Both spellings: "daily" does not contain "day".
    } else if lower.contains("daily") || lower.contains("day") {
        Some("daily")
    } else if lower.contains("session") || lower.contains("hour") {
        Some("session")
    } else {
        None
    }
}

/// Minutes named inside a slot label: "5-hour" → 300, "7-Day" → 10080,
/// "Session (5h)" → 300.
///
/// A unit must follow the number immediately (allowing `-`, space, `_` or `(`
/// between), so a version or model number never reads as a duration: "GPT-4"
/// and "Sonnet 4.5" both yield `None`. A bare "m" is skipped as well, because
/// it means minutes to one reader and months to another.
fn minutes_from_label(label: &str) -> Option<u32> {
    let bytes = label.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let digits_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        let Ok(value) = label[digits_start..index].parse::<u32>() else {
            continue;
        };

        let mut cursor = index;
        while cursor < bytes.len() && matches!(bytes[cursor], b'-' | b' ' | b'_' | b'(') {
            cursor += 1;
        }
        let unit_start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_alphabetic() {
            cursor += 1;
        }
        let per_unit = match label[unit_start..cursor].to_ascii_lowercase().as_str() {
            "h" | "hr" | "hrs" | "hour" | "hours" => 60,
            "d" | "day" | "days" => 24 * 60,
            "w" | "wk" | "week" | "weeks" => 7 * 24 * 60,
            "mo" | "month" | "months" => 30 * 24 * 60,
            _ => continue,
        };
        return value.checked_mul(per_unit);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_declared_length_names_its_band() {
        assert_eq!(classify(Some(300), None), Some("session"));
        assert_eq!(classify(Some(24 * 60), None), Some("daily"));
        assert_eq!(classify(Some(7 * 24 * 60), None), Some("weekly"));
        assert_eq!(classify(Some(30 * 24 * 60), None), Some("monthly"));
    }

    /// The regression this module was extracted to kill: `MenuCard` tested
    /// weekly as `minutes >= 7 days` with no upper bound, so a monthly window
    /// answered to "weekly" and collected the weekly forecast — while the strip,
    /// using closed bands, called the same window monthly.
    #[test]
    fn a_monthly_window_is_never_also_weekly() {
        assert_eq!(classify(Some(30 * 24 * 60), None), Some("monthly"));
        assert_eq!(classify(Some(31 * 24 * 60), None), Some("monthly"));
    }

    /// The other drift: the TypeScript copies matched on length alone, so a
    /// provider reporting usage without a declared length matched nothing.
    #[test]
    fn a_slot_name_answers_when_no_length_is_declared() {
        assert_eq!(classify(None, Some("Monthly")), Some("monthly"));
        assert_eq!(classify(None, Some("Weekly quota")), Some("weekly"));
        assert_eq!(classify(None, Some("Daily AFP")), Some("daily"));
        assert_eq!(classify(None, Some("5-hour Requests")), Some("session"));
        assert_eq!(classify(None, Some("Session (5h)")), Some("session"));
        // "7-Day" is a week, not a day.
        assert_eq!(classify(None, Some("7-Day")), Some("weekly"));
    }

    #[test]
    fn a_declared_length_outranks_the_slot_name() {
        assert_eq!(classify(Some(300), Some("Monthly")), Some("session"));
    }

    #[test]
    fn a_slot_that_is_not_a_cycle_stays_unnamed() {
        for label in ["Credits", "Balance", "On-demand", "Gemini Pro", "DIEM"] {
            assert_eq!(classify(None, Some(label)), None, "{label}");
        }
    }

    /// A version number is not a duration.
    #[test]
    fn a_number_with_no_unit_is_not_a_length() {
        assert_eq!(classify(None, Some("GPT-4")), None);
        assert_eq!(classify(None, Some("Sonnet 4.5")), None);
    }

    /// Real data that fits no band keeps its numbers and loses only its name.
    #[test]
    fn a_cycle_in_no_band_is_unnamed_rather_than_rounded() {
        assert_eq!(classify(Some(14 * 24 * 60), None), None);
        assert_eq!(classify(Some(12 * 60), None), None);
    }

    #[test]
    fn nothing_declared_answers_nothing() {
        assert_eq!(classify(None, None), None);
    }
}
