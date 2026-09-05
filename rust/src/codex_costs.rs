//! Codex local-log cost aggregation helpers.

#[cfg(test)]
use chrono::Local;
use chrono::{Duration, NaiveDate};
use std::path::Path;

#[cfg(test)]
use crate::core::CodexUsageRecord;
use crate::core::{CostUsageDayRange, CostUsagePricing};
use crate::cost_scanner::{CostSummary, ModelTokenCounts};
use crate::scan_cache::CachedTurn;

pub(crate) fn codex_period_start(today: NaiveDate, days: u32) -> NaiveDate {
    today - Duration::days(days.saturating_sub(1) as i64)
}
/// Record-stream aggregation — the test oracle for the cached turn replay
/// (`apply_codex_turns_to_summary`): both must produce identical summaries
/// for the same records.
#[cfg(test)]
pub(crate) fn add_codex_records_to_summary(
    summary: &mut CostSummary,
    records: &[CodexUsageRecord],
    range: &CostUsageDayRange,
) -> (f64, bool) {
    let mut total_cost = 0.0;
    let mut has_tokens = false;

    for record in records.iter().filter(|record| {
        CostUsageDayRange::is_in_range(&record.day_key, &range.since_key, &range.until_key)
    }) {
        let tokens = CodexTokenCounts::from_values(record.input, record.cached, record.output);
        let pricing_day = CostUsageDayRange::parse_day_key(&record.day_key);
        if let Some(cost) = add_codex_tokens_to_summary(summary, &record.model, tokens, pricing_day)
        {
            total_cost += cost;
            has_tokens = true;
        }
    }

    (total_cost, has_tokens)
}

#[cfg(test)]
pub(crate) fn scan_codex_file_cost(path: &Path) -> f64 {
    let today = Local::now().date_naive();
    let range = CostUsageDayRange::new(codex_period_start(today, 30), today);
    scan_codex_file_cost_for_range(path, &range)
}

pub(crate) fn scan_codex_file_cost_for_range(path: &Path, range: &CostUsageDayRange) -> f64 {
    let Some(entry) = crate::cost_scanner::codex_file_turns(path) else {
        return 0.0;
    };

    codex_turns_cost(&entry.turns, range)
}

/// Replay lineage-deduplicated turns into a `CostSummary`, exactly like the
/// record oracle: the day range filters here, pricing/speed/unknown-model
/// handling all live in `add_codex_tokens_to_summary`.
pub(crate) fn apply_codex_turns_to_summary(
    summary: &mut CostSummary,
    turns: &[CachedTurn],
    range: &CostUsageDayRange,
) -> (f64, u32) {
    let mut total_cost = 0.0;
    let mut turn_count = 0u32;

    for turn in turns.iter().filter(|turn| {
        CostUsageDayRange::is_in_range(&turn.day, &range.since_key, &range.until_key)
    }) {
        let tokens = CodexTokenCounts::from_bucket(turn.input, turn.cached, turn.output);
        let pricing_day = CostUsageDayRange::parse_day_key(&turn.day);
        if let Some(cost) = add_codex_tokens_to_summary(summary, &turn.model, tokens, pricing_day)
        {
            total_cost += cost;
            turn_count += 1;
        }
    }

    (total_cost, turn_count)
}

pub(crate) fn codex_turn_cost_at_date(
    model: &str,
    input: u64,
    cached: u64,
    output: u64,
    pricing_day: Option<NaiveDate>,
) -> Option<f64> {
    if CostUsagePricing::is_codex_unattributed_model(model)
        || !CostUsagePricing::counts_toward_codex_subscription(model)
    {
        return None;
    }
    let tokens = CodexTokenCounts::from_bucket(input, cached, output);
    if tokens.is_empty() {
        return None;
    }
    Some(codex_cost_usd_for_day(
        model,
        tokens.input,
        tokens.cached,
        tokens.output,
        pricing_day,
    ))
}

pub(crate) fn codex_turns_cost(turns: &[CachedTurn], range: &CostUsageDayRange) -> f64 {
    let mut total_cost = 0.0;

    for turn in turns.iter().filter(|turn| {
        CostUsageDayRange::is_in_range(&turn.day, &range.since_key, &range.until_key)
    }) {
        if CostUsagePricing::is_codex_unattributed_model(&turn.model)
            || !CostUsagePricing::counts_toward_codex_subscription(&turn.model)
        {
            continue;
        }
        let tokens = CodexTokenCounts::from_bucket(turn.input, turn.cached, turn.output);
        if !tokens.is_empty() {
            let cost = codex_cost_usd_for_day(
                &turn.model,
                tokens.input,
                tokens.cached,
                tokens.output,
                CostUsageDayRange::parse_day_key(&turn.day),
            );
            total_cost += cost;
        }
    }

    total_cost
}

#[derive(Clone, Copy)]
struct CodexTokenCounts {
    input: u64,
    cached: u64,
    output: u64,
}

impl CodexTokenCounts {
    /// Bucket/turn 版本:sums 已是逐条 clamp 后的 u64。
    fn from_bucket(input: u64, cached: u64, output: u64) -> Self {
        Self {
            input,
            cached: cached.min(input),
            output,
        }
    }

    #[cfg(test)]
    fn from_values(input: u64, cached: u64, output: u64) -> Self {
        Self {
            input,
            cached: cached.min(input),
            output,
        }
    }

    fn is_empty(self) -> bool {
        self.input == 0 && self.cached == 0 && self.output == 0
    }
}

fn add_tokens(summary: &mut ModelTokenCounts, tokens: CodexTokenCounts) {
    summary.input_tokens += tokens.input;
    summary.output_tokens += tokens.output;
    summary.cached_tokens += tokens.cached;
}

fn add_codex_tokens_to_summary(
    summary: &mut CostSummary,
    model: &str,
    tokens: CodexTokenCounts,
    pricing_day: Option<NaiveDate>,
) -> Option<f64> {
    if tokens.is_empty() {
        return None;
    }

    // Provider-qualified rows belong to their own subscription.  Keeping them
    // in the native Codex total was the confirmed source of the 8%+ inflation
    // seen in current local logs.
    if !CostUsagePricing::counts_toward_codex_subscription(model) {
        return None;
    }

    let model_key = if CostUsagePricing::is_codex_unattributed_model(model) {
        CostUsagePricing::CODEX_UNATTRIBUTED_MODEL.to_string()
    } else {
        model.to_string()
    };

    // Unattributed usage is visible but never priced and must not trigger a
    // models.dev catalog refresh (it is deliberately unpriced, not "unknown yet").
    if CostUsagePricing::is_codex_unattributed_model(&model_key) {
        summary.input_tokens += tokens.input;
        summary.cached_tokens += tokens.cached;
        summary.output_tokens += tokens.output;
        summary.by_model.entry(model_key.clone()).or_insert(0.0);
        add_tokens(
            summary.by_model_tokens.entry(model_key).or_default(),
            tokens,
        );
        return Some(0.0);
    }

    let uses_fallback_pricing = pricing_day
        .and_then(|day| {
            CostUsagePricing::codex_cost_usd_at_date(
                &model_key,
                tokens.input,
                tokens.cached,
                tokens.output,
                day,
            )
        })
        .or_else(|| {
            CostUsagePricing::codex_cost_usd(
                &model_key,
                tokens.input,
                tokens.cached,
                tokens.output,
            )
        })
        .is_none();
    let cost = codex_cost_usd_for_day(
        &model_key,
        tokens.input,
        tokens.cached,
        tokens.output,
        pricing_day,
    );
    if uses_fallback_pricing {
        summary.unknown_models.insert(model_key.clone());
        summary
            .model_pricing_completeness
            .mark_partial(&model_key);
    }

    summary.input_tokens += tokens.input;
    summary.cached_tokens += tokens.cached;
    summary.output_tokens += tokens.output;
    *summary.by_model.entry(model_key.clone()).or_insert(0.0) += cost;

    let speed_bucket = codex_speed_bucket(&model_key);
    *summary
        .by_speed
        .entry(speed_bucket.to_string())
        .or_insert(0.0) += cost;
    add_tokens(
        summary.by_model_tokens.entry(model_key).or_default(),
        tokens,
    );
    add_tokens(
        summary
            .by_speed_tokens
            .entry(speed_bucket.to_string())
            .or_default(),
        tokens,
    );
    Some(cost)
}

fn codex_speed_bucket(model: &str) -> &'static str {
    let normalized = model.to_ascii_lowercase();
    if normalized.contains("fast")
        || normalized.contains("priority")
        || normalized.contains("spark")
        || normalized.contains("smoke")
    {
        "fast"
    } else {
        "standard"
    }
}

fn codex_cost_usd(model: &str, input: u64, cached: u64, output: u64) -> f64 {
    codex_cost_usd_for_day(model, input, cached, output, None)
}

fn codex_cost_usd_for_day(
    model: &str,
    input: u64,
    cached: u64,
    output: u64,
    pricing_day: Option<NaiveDate>,
) -> f64 {
    if CostUsagePricing::is_codex_unattributed_model(model) {
        return 0.0;
    }
    if !CostUsagePricing::counts_toward_codex_subscription(model) {
        return 0.0;
    }
    if let Some(cost) = pricing_day
        .and_then(|day| CostUsagePricing::codex_cost_usd_at_date(model, input, cached, output, day))
        .or_else(|| CostUsagePricing::codex_cost_usd(model, input, cached, output))
    {
        return cost;
    }

    let (input_price, cached_price, output_price) = match model.to_lowercase().as_str() {
        m if m.contains("gpt-4o-mini") => (0.15, 0.075, 0.60),
        m if m.contains("gpt-4o") => (2.50, 1.25, 10.00),
        m if m.contains("gpt-4-turbo") => (10.00, 5.00, 30.00),
        m if m.contains("gpt-4") => (30.00, 15.00, 60.00),
        m if m.contains("o1-mini") => (3.00, 1.50, 12.00),
        m if m.contains("o1") => (15.00, 7.50, 60.00),
        _ => (2.50, 1.25, 10.00),
    };

    let cached = cached.min(input);
    let non_cached = input.saturating_sub(cached);
    let input_cost = (non_cached as f64 / 1_000_000.0) * input_price;
    let cached_cost = (cached as f64 / 1_000_000.0) * cached_price;
    let output_cost = (output as f64 / 1_000_000.0) * output_price;

    input_cost + cached_cost + output_cost
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::CodexUsageRecord;

    #[test]
    fn test_codex_pricing() {
        // Test GPT-4o pricing: $2.50/1M input, $10/1M output
        let cost = codex_cost_usd("gpt-4o", 1_000_000, 0, 1_000_000);
        assert!((cost - 12.50).abs() < 0.01);
    }

    #[test]
    fn test_codex_pricing_uses_gpt55_standard_short_context_rates() {
        let cost = codex_cost_usd("gpt-5.5", 1_000_000, 400_000, 1_000_000);

        // GPT-5.5 standard short-context pricing:
        // 600k non-cached input at $5/M, 400k cached input at $0.50/M,
        // and 1M output at $30/M.
        assert!((cost - 33.20).abs() < 0.01);
    }

    #[test]
    fn codex_summary_prices_gpt56_usage_records_individually() {
        let target = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        let range = CostUsageDayRange::new(target, target);
        let records = vec![
            CodexUsageRecord {
                day_key: "2026-05-31".to_string(),
                model: "gpt-5.6-sol".to_string(),
                input: 200_000,
                cached: 0,
                output: 0,
            },
            CodexUsageRecord {
                day_key: "2026-05-31".to_string(),
                model: "gpt-5.6-sol".to_string(),
                input: 200_000,
                cached: 0,
                output: 0,
            },
            CodexUsageRecord {
                day_key: "2026-05-30".to_string(),
                model: "gpt-5.6-sol".to_string(),
                input: 200_000,
                cached: 0,
                output: 0,
            },
        ];
        let mut summary = CostSummary::default();

        let (cost, has_tokens) = add_codex_records_to_summary(&mut summary, &records, &range);

        assert!(has_tokens);
        assert_eq!(summary.input_tokens, 400_000);
        assert!((cost - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn model_less_codex_usage_is_visible_but_unpriced() {
        let target = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        let range = CostUsageDayRange::new(target, target);
        let records = vec![CodexUsageRecord {
            day_key: "2026-05-31".to_string(),
            model: CostUsagePricing::CODEX_UNATTRIBUTED_MODEL.to_string(),
            input: 55_000_000,
            cached: 0,
            output: 0,
        }];
        let mut summary = CostSummary::default();

        let (cost, has_tokens) = add_codex_records_to_summary(&mut summary, &records, &range);

        assert!(has_tokens);
        assert_eq!(cost, 0.0);
        assert_eq!(summary.input_tokens, 55_000_000);
        assert_eq!(
            summary
                .by_model
                .get(CostUsagePricing::CODEX_UNATTRIBUTED_MODEL)
                .copied(),
            Some(0.0)
        );
        assert!(summary.unknown_models.is_empty());
    }

    #[test]
    fn records_unknown_codex_model_while_using_fallback_cost() {
        let target = NaiveDate::from_ymd_opt(2026, 5, 31).unwrap();
        let range = CostUsageDayRange::new(target, target);
        let records = vec![CodexUsageRecord {
            day_key: "2026-05-31".to_string(),
            model: "gpt-mystery".to_string(),
            input: 1_000_000,
            cached: 0,
            output: 1_000_000,
        }];
        let mut summary = CostSummary::default();

        let (cost, has_tokens) = add_codex_records_to_summary(&mut summary, &records, &range);

        assert!(has_tokens);
        assert!(cost > 0.0);
        assert!(summary.unknown_models.contains("gpt-mystery"));
    }

    #[test]
    fn routed_models_do_not_enter_native_codex_summary_or_cost() {
        let target = NaiveDate::from_ymd_opt(2026, 8, 19).unwrap();
        let range = CostUsageDayRange::new(target, target);
        let records = vec![
            CodexUsageRecord {
                day_key: "2026-08-19".to_string(),
                model: "gpt-5.6-sol".to_string(),
                input: 100,
                cached: 0,
                output: 5,
            },
            CodexUsageRecord {
                day_key: "2026-08-19".to_string(),
                model: "deepseek/deepseek-chat".to_string(),
                input: 1_000_000,
                cached: 0,
                output: 1_000_000,
            },
            CodexUsageRecord {
                day_key: "2026-08-19".to_string(),
                model: "codex-auto-review".to_string(),
                input: 1_000_000,
                cached: 0,
                output: 1_000_000,
            },
        ];
        let mut summary = CostSummary::default();

        let (cost, has_tokens) = add_codex_records_to_summary(&mut summary, &records, &range);

        assert!(has_tokens);
        assert_eq!(summary.input_tokens, 100);
        assert_eq!(summary.output_tokens, 5);
        assert!(!summary.by_model.contains_key("deepseek/deepseek-chat"));
        assert!(!summary.by_model.contains_key("codex-auto-review"));
        assert!(cost < 0.01, "routed cost leaked into native Codex: {cost}");
    }

    #[test]
    fn test_codex_speed_bucket() {
        assert_eq!(codex_speed_bucket("gpt-5.5-fast"), "fast");
        assert_eq!(codex_speed_bucket("gpt-5.3-codex-spark"), "fast");
        assert_eq!(codex_speed_bucket("gpt-5-codex"), "standard");
    }
}
