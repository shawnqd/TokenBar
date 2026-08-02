//! Usage Pace Prediction
//!
//! Calculates whether the user is On Track, Ahead, or Behind their usage quota
//! based on elapsed time and consumption rate.

use chrono::{DateTime, Utc};

use super::RateWindow;

/// Usage pace stage indicating consumption rate relative to time
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaceStage {
    /// Within 2% of expected usage
    OnTrack,
    /// 2-6% ahead of expected
    SlightlyAhead,
    /// 6-12% ahead of expected
    Ahead,
    /// More than 12% ahead of expected
    FarAhead,
    /// 2-6% behind expected
    SlightlyBehind,
    /// 6-12% behind expected
    Behind,
    /// More than 12% behind expected
    FarBehind,
}

impl PaceStage {
    /// Get a short label for the stage
    pub fn label(&self) -> &'static str {
        match self {
            PaceStage::OnTrack => "On Track",
            PaceStage::SlightlyAhead => "Slightly Ahead",
            PaceStage::Ahead => "Ahead",
            PaceStage::FarAhead => "Far Ahead",
            PaceStage::SlightlyBehind => "Slightly Behind",
            PaceStage::Behind => "Behind",
            PaceStage::FarBehind => "Far Behind",
        }
    }

    /// Get an emoji indicator for the stage
    pub fn emoji(&self) -> &'static str {
        match self {
            PaceStage::OnTrack => "✓",
            PaceStage::SlightlyAhead | PaceStage::Ahead | PaceStage::FarAhead => "⚡",
            PaceStage::SlightlyBehind | PaceStage::Behind | PaceStage::FarBehind => "🐢",
        }
    }

    /// Whether the user is consuming faster than expected
    pub fn is_ahead(&self) -> bool {
        matches!(
            self,
            PaceStage::SlightlyAhead | PaceStage::Ahead | PaceStage::FarAhead
        )
    }

    /// Whether the user is consuming slower than expected
    pub fn is_behind(&self) -> bool {
        matches!(
            self,
            PaceStage::SlightlyBehind | PaceStage::Behind | PaceStage::FarBehind
        )
    }
}

/// Usage pace prediction result
#[derive(Debug, Clone)]
pub struct UsagePace {
    /// The pace stage
    pub stage: PaceStage,
    /// Delta between actual and expected usage (positive = ahead)
    pub delta_percent: f64,
    /// Expected usage percent based on elapsed time
    pub expected_used_percent: f64,
    /// Actual usage percent
    pub actual_used_percent: f64,
    /// Estimated time until quota is exhausted (if ahead of pace)
    pub eta_seconds: Option<f64>,
    /// Whether current pace will last until reset
    pub will_last_to_reset: bool,
    /// How many times the current burn rate could increase and still reach the
    /// reset without exhausting the window.
    ///
    /// `remaining_capacity / projected_remaining_usage`. `None` when the ratio
    /// is meaningless (nothing left, or nothing projected to be spent), so a
    /// caller can never render a fabricated headroom figure. Ported from macOS
    /// `UsagePace.safeSpeedMultiplier`.
    pub speed_multiplier_to_reset: Option<f64>,
}

impl UsagePace {
    /// Calculate weekly usage pace
    ///
    /// # Arguments
    /// * `window` - The rate window to analyze
    /// * `now` - Current time (defaults to Utc::now())
    /// * `default_window_minutes` - Default window duration if not specified (10080 = 7 days)
    pub fn weekly(
        window: &RateWindow,
        now: Option<DateTime<Utc>>,
        default_window_minutes: u32,
    ) -> Option<Self> {
        let now = now.unwrap_or_else(Utc::now);
        let resets_at = window.resets_at?;
        let minutes = window.window_minutes.unwrap_or(default_window_minutes);

        if minutes == 0 {
            return None;
        }

        let duration_secs = f64::from(minutes) * 60.0;
        let time_until_reset = (resets_at - now).num_seconds() as f64;

        // Must be before reset and within the window duration
        if time_until_reset <= 0.0 || time_until_reset > duration_secs {
            return None;
        }

        let elapsed = Self::clamp(duration_secs - time_until_reset, 0.0, duration_secs);
        let expected = Self::clamp((elapsed / duration_secs) * 100.0, 0.0, 100.0);
        let actual = Self::clamp(window.used_percent, 0.0, 100.0);

        // If no time has elapsed but there's usage, something's wrong
        if elapsed == 0.0 && actual > 0.0 {
            return None;
        }

        let delta = actual - expected;
        let stage = Self::stage_for_delta(delta);

        // Projected spend between now and reset if the current rate holds, and
        // the factor by which that rate could grow while still fitting into what
        // is left. Computed before the ETA branch so it is available whether or
        // not the window is projected to last.
        let projected_remaining_usage = if elapsed > 0.0 {
            actual * time_until_reset / elapsed
        } else {
            0.0
        };
        let speed_multiplier_to_reset =
            Self::safe_speed_multiplier(100.0 - actual, projected_remaining_usage);

        let mut eta_seconds = None;
        let mut will_last_to_reset = false;

        if elapsed > 0.0 && actual > 0.0 {
            let rate = actual / elapsed; // percent per second
            if rate > 0.0 {
                let remaining = (100.0 - actual).max(0.0);
                let candidate = remaining / rate;
                if candidate >= time_until_reset {
                    will_last_to_reset = true;
                } else {
                    eta_seconds = Some(candidate);
                }
            }
        } else if elapsed > 0.0 && actual == 0.0 {
            // No usage yet, will definitely last
            will_last_to_reset = true;
        }

        Some(UsagePace {
            stage,
            delta_percent: delta,
            expected_used_percent: expected,
            actual_used_percent: actual,
            eta_seconds,
            will_last_to_reset,
            speed_multiplier_to_reset,
        })
    }

    /// Calculate the stage for a given delta percentage
    /// `None` rather than infinity/NaN when the inputs make the ratio
    /// meaningless, so callers never render a fabricated headroom figure.
    fn safe_speed_multiplier(
        remaining_capacity: f64,
        projected_remaining_usage: f64,
    ) -> Option<f64> {
        if remaining_capacity <= 0.0 || projected_remaining_usage <= 0.0 {
            return None;
        }
        let multiplier = remaining_capacity / projected_remaining_usage;
        multiplier.is_finite().then_some(multiplier)
    }

    fn stage_for_delta(delta: f64) -> PaceStage {
        let abs_delta = delta.abs();

        if abs_delta <= 2.0 {
            PaceStage::OnTrack
        } else if abs_delta <= 6.0 {
            if delta >= 0.0 {
                PaceStage::SlightlyAhead
            } else {
                PaceStage::SlightlyBehind
            }
        } else if abs_delta <= 12.0 {
            if delta >= 0.0 {
                PaceStage::Ahead
            } else {
                PaceStage::Behind
            }
        } else if delta >= 0.0 {
            PaceStage::FarAhead
        } else {
            PaceStage::FarBehind
        }
    }

    fn clamp(value: f64, lower: f64, upper: f64) -> f64 {
        value.clamp(lower, upper)
    }

    /// Format the ETA as a human-readable string
    pub fn format_eta(&self) -> Option<String> {
        let secs = self.eta_seconds?;
        let hours = (secs / 3600.0) as i64;
        let minutes = ((secs % 3600.0) / 60.0) as i64;

        if hours > 24 {
            let days = hours / 24;
            Some(format!("{}d {}h", days, hours % 24))
        } else if hours > 0 {
            Some(format!("{}h {}m", hours, minutes))
        } else {
            Some(format!("{}m", minutes))
        }
    }

    /// Format the pace as a status line
    pub fn format_status(&self) -> String {
        let stage_text = self.stage.label();

        if self.will_last_to_reset {
            format!("{} - will last to reset", stage_text)
        } else if let Some(eta) = self.format_eta() {
            format!("{} - exhausted in {}", stage_text, eta)
        } else {
            stage_text.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_stage_for_delta() {
        assert_eq!(UsagePace::stage_for_delta(0.0), PaceStage::OnTrack);
        assert_eq!(UsagePace::stage_for_delta(1.5), PaceStage::OnTrack);
        assert_eq!(UsagePace::stage_for_delta(-1.5), PaceStage::OnTrack);

        assert_eq!(UsagePace::stage_for_delta(4.0), PaceStage::SlightlyAhead);
        assert_eq!(UsagePace::stage_for_delta(-4.0), PaceStage::SlightlyBehind);

        assert_eq!(UsagePace::stage_for_delta(10.0), PaceStage::Ahead);
        assert_eq!(UsagePace::stage_for_delta(-10.0), PaceStage::Behind);

        assert_eq!(UsagePace::stage_for_delta(20.0), PaceStage::FarAhead);
        assert_eq!(UsagePace::stage_for_delta(-20.0), PaceStage::FarBehind);
    }

    #[test]
    fn test_pace_calculation() {
        let now = Utc::now();
        // Window resets in 3.5 days (halfway through a 7-day window)
        let resets_at = now + Duration::days(3) + Duration::hours(12);

        // User has used 50% - exactly on track
        let window = RateWindow::with_details(50.0, Some(10080), Some(resets_at), None);
        let pace = UsagePace::weekly(&window, Some(now), 10080).unwrap();

        assert_eq!(pace.stage, PaceStage::OnTrack);
        assert!(pace.delta_percent.abs() < 2.0);
    }

    #[test]
    fn test_pace_ahead() {
        let now = Utc::now();
        // Window resets in 3.5 days (halfway through a 7-day window)
        let resets_at = now + Duration::days(3) + Duration::hours(12);

        // User has used 80% - way ahead of schedule
        let window = RateWindow::with_details(80.0, Some(10080), Some(resets_at), None);
        let pace = UsagePace::weekly(&window, Some(now), 10080).unwrap();

        assert!(pace.stage.is_ahead());
        assert!(pace.delta_percent > 0.0);
    }

    #[test]
    fn test_pace_labels() {
        assert_eq!(PaceStage::OnTrack.label(), "On Track");
        assert_eq!(PaceStage::FarAhead.label(), "Far Ahead");
        assert_eq!(PaceStage::SlightlyBehind.emoji(), "🐢");
    }

    /// Headroom is the factor the current burn rate could grow by and still
    /// reach the reset. Half-way through a window at 25% used projects another
    /// 25% spent, leaving 75% of capacity for it — 3x headroom.
    #[test]
    fn speed_multiplier_reports_real_headroom() {
        let now = Utc::now();
        let window = RateWindow::with_details(25.0, Some(10080), Some(now + Duration::days(3) + Duration::hours(12)), None);
        let pace = UsagePace::weekly(&window, Some(now), 10080).expect("pace");
        let multiplier = pace.speed_multiplier_to_reset.expect("headroom");
        assert!(
            (multiplier - 3.0).abs() < 0.05,
            "expected about 3x headroom, got {multiplier}"
        );
    }

    /// A full window has no capacity left, so there is no headroom figure to
    /// report — `None`, never infinity or a fabricated number.
    #[test]
    fn speed_multiplier_is_absent_when_nothing_remains() {
        let now = Utc::now();
        let window = RateWindow::with_details(100.0, Some(10080), Some(now + Duration::days(3)), None);
        let pace = UsagePace::weekly(&window, Some(now), 10080).expect("pace");
        assert_eq!(pace.speed_multiplier_to_reset, None);
    }

    /// Nothing spent yet means nothing is projected to be spent, so the ratio is
    /// undefined rather than infinite.
    #[test]
    fn speed_multiplier_is_absent_before_any_usage() {
        let now = Utc::now();
        let window = RateWindow::with_details(0.0, Some(10080), Some(now + Duration::days(3)), None);
        let pace = UsagePace::weekly(&window, Some(now), 10080).expect("pace");
        assert_eq!(pace.speed_multiplier_to_reset, None);
    }
}
