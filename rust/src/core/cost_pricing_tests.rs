use super::*;
use chrono::NaiveDate;

#[test]
fn test_normalize_codex_model() {
    assert_eq!(CostUsagePricing::normalize_codex_model("gpt-5"), "gpt-5");
    assert_eq!(
        CostUsagePricing::normalize_codex_model("openai/gpt-5"),
        "gpt-5"
    );
    assert_eq!(
        CostUsagePricing::normalize_codex_model("gpt-5-codex"),
        "gpt-5"
    );
    assert_eq!(
        CostUsagePricing::normalize_codex_model(""),
        CostUsagePricing::CODEX_UNATTRIBUTED_MODEL
    );
    assert_eq!(
        CostUsagePricing::normalize_codex_model("unknown"),
        CostUsagePricing::CODEX_UNATTRIBUTED_MODEL
    );
}

#[test]
fn unattributed_codex_usage_stays_unpriced() {
    assert!(
        CostUsagePricing::codex_cost_usd(CostUsagePricing::CODEX_UNATTRIBUTED_MODEL, 1_000, 0, 500)
            .is_none()
    );
    assert!(CostUsagePricing::is_codex_unattributed_model("unknown"));
    assert!(CostUsagePricing::is_codex_unattributed_model("  "));
}

#[test]
fn routed_codex_models_are_not_native_subscription_usage() {
    assert!(CostUsagePricing::counts_toward_codex_subscription("gpt-5"));
    assert!(CostUsagePricing::counts_toward_codex_subscription(
        "OPENAI/gpt-5"
    ));
    assert!(!CostUsagePricing::counts_toward_codex_subscription(
        "deepseek/model"
    ));
    assert!(!CostUsagePricing::counts_toward_codex_subscription(
        "opencode-go-qwen/model"
    ));
    assert!(!CostUsagePricing::counts_toward_codex_subscription(
        "codex-auto-review"
    ));
    assert!(CostUsagePricing::codex_cost_usd("acme/model", 1_000_000, 0, 1_000_000).is_none());
}

#[test]
fn test_normalize_claude_model() {
    assert_eq!(
        CostUsagePricing::normalize_claude_model("claude-sonnet-4-5"),
        "claude-sonnet-4-5"
    );
    assert_eq!(
        CostUsagePricing::normalize_claude_model("anthropic.claude-sonnet-4-5"),
        "claude-sonnet-4-5"
    );
}

#[test]
fn test_codex_cost() {
    let cost = CostUsagePricing::codex_cost_usd("gpt-5", 1000, 0, 500).unwrap();
    assert!((cost - 0.00625).abs() < 1e-10);
}

#[test]
fn test_claude_cost() {
    assert!(
        CostUsagePricing::claude_cost_usd("claude-haiku-4-5-20251001", 1000, 0, 0, 500).is_some()
    );
}

#[test]
fn test_opus_4_8_cost() {
    let cost = CostUsagePricing::claude_cost_usd("claude-opus-4-8", 1_000, 0, 0, 500).unwrap();
    assert!((cost - 0.0175).abs() < 1e-10);
}

#[test]
fn test_fable_5_cost() {
    let cost = CostUsagePricing::claude_cost_usd("claude-fable-5", 1_000, 0, 0, 500).unwrap();
    assert!((cost - 0.035).abs() < 1e-10);
}

#[test]
fn test_claude_input_cost_per_token() {
    assert_eq!(
        CostUsagePricing::claude_input_cost_per_token("claude-opus-4-8"),
        Some(5e-6)
    );
    assert_eq!(
        CostUsagePricing::claude_input_cost_per_token("claude-fable-5"),
        Some(1e-5)
    );
    assert_eq!(
        CostUsagePricing::claude_input_cost_per_token("totally-unknown-model"),
        None
    );
}

#[test]
fn test_format_model_name() {
    assert_eq!(
        CostUsagePricing::format_model_name("claude-3.5-sonnet"),
        "Sonnet 3.5"
    );
    assert_eq!(
        CostUsagePricing::format_model_name("claude-opus-4"),
        "Opus 4"
    );
    assert_eq!(CostUsagePricing::format_model_name("gpt-5"), "GPT-5");
}

#[test]
fn test_gpt54_mini_cost() {
    let cost = CostUsagePricing::codex_cost_usd("gpt-5.4-mini", 1000, 0, 500).unwrap();
    assert!((cost - 0.003).abs() < 1e-10);
}

#[test]
fn test_gpt54_nano_cost() {
    let cost = CostUsagePricing::codex_cost_usd("gpt-5.4-nano", 1000, 0, 500).unwrap();
    assert!((cost - 0.000825).abs() < 1e-10);
}

#[test]
fn test_normalize_gpt54_codex() {
    assert_eq!(
        CostUsagePricing::normalize_codex_model("gpt-5.4-mini-codex"),
        "gpt-5.4-mini"
    );
}

#[test]
fn test_gpt55_pricing() {
    assert_eq!(
        CostUsagePricing::normalize_codex_model("openai/gpt-5.5-2026-04-23"),
        "gpt-5.5"
    );
    assert_eq!(
        CostUsagePricing::normalize_codex_model("gpt-5.5-pro-2026-04-23"),
        "gpt-5.5-pro"
    );
    let cost = CostUsagePricing::codex_cost_usd("gpt-5.5", 1000, 500, 500).unwrap();
    assert!((cost - 0.01775).abs() < 1e-10);
}

#[test]
fn test_format_gpt54_mini() {
    assert_eq!(
        CostUsagePricing::format_model_name("gpt-5.4-mini"),
        "GPT-5.4 Mini"
    );
}

#[test]
fn test_opus_4_7_cost() {
    assert!(CostUsagePricing::claude_cost_usd("claude-opus-4-7", 1000, 0, 0, 500).is_some());
}

#[test]
fn test_sonnet_4_6_cost() {
    assert!(CostUsagePricing::claude_cost_usd("claude-sonnet-4-6", 1000, 0, 0, 500).is_some());
}

#[test]
fn test_gpt5_pro_cost() {
    let cost = CostUsagePricing::codex_cost_usd("gpt-5-pro", 1000, 0, 500).unwrap();
    assert!((cost - 0.075).abs() < 1e-10);
}

#[test]
fn test_gpt56_standard_pricing() {
    for (model, expected) in [
        // 600 non-cached x 4/M + 400 cached x 0.4/M + 1000 out x 20/M.
        ("gpt-5.6-sol", 0.02256),
        ("gpt-5.6-terra", 0.01328),
        ("gpt-5.6-luna", 0.001328),
    ] {
        let cost = CostUsagePricing::codex_cost_usd(model, 1_000, 400, 1_000);
        assert!((cost.unwrap() - expected).abs() < 1e-10, "{model}");
    }
}

#[test]
fn test_gpt56_long_context_pricing() {
    for (model, expected) in [
        // 272001 cached x 0.8/M + 1M out x 30/M (official long-context rates).
        ("gpt-5.6-sol", 30.2176008),
        ("gpt-5.6-terra", 18.1088004),
        ("gpt-5.6-luna", 1.81088004),
    ] {
        let cost = CostUsagePricing::codex_cost_usd(model, 272_001, 272_001, 1_000_000);
        assert!((cost.unwrap() - expected).abs() < 1e-10, "{model}");
    }
}

#[test]
fn test_gpt56_context_threshold_is_exclusive() {
    for (model, expected) in [
        // 272000 exactly sits on the short-context side of the exclusive
        // threshold: 272000 x 0.4/M.
        ("gpt-5.6-sol", 0.1088),
        ("gpt-5.6-terra", 0.0544),
        ("gpt-5.6-luna", 0.00544),
    ] {
        let cost = CostUsagePricing::codex_cost_usd(model, 272_000, 272_000, 0);
        assert!((cost.unwrap() - expected).abs() < 1e-10, "{model}");
    }
}

#[test]
fn test_gpt56_historical_rates_use_event_date() {
    let before_cutover = NaiveDate::from_ymd_opt(2026, 7, 29).unwrap();
    let after_cutover = NaiveDate::from_ymd_opt(2026, 7, 30).unwrap();

    let before = CostUsagePricing::codex_cost_usd_at_date(
        "gpt-5.6-terra",
        1_000_000,
        0,
        1_000_000,
        before_cutover,
    )
    .unwrap();
    let after = CostUsagePricing::codex_cost_usd_at_date(
        "gpt-5.6-terra",
        1_000_000,
        0,
        1_000_000,
        after_cutover,
    )
    .unwrap();

    // One million input tokens exercises the long-context tier: the legacy
    // Terra rate was $5/M input + $22.50/M output, then moved to $4/M + $18/M.
    assert!(
        (before - 27.5).abs() < 1e-10,
        "before={before}, after={after}"
    );
    assert!(
        (after - 22.0).abs() < 1e-10,
        "before={before}, after={after}"
    );
}

#[test]
fn test_normalize_gpt56_aliases() {
    for model in [
        "gpt-5.6",
        "openai/gpt-5.6",
        "gpt-5.6-codex",
        "gpt-5.6-2099-01-01",
        "openai/gpt-5.6-codex-2099-01-01",
    ] {
        assert_eq!(
            CostUsagePricing::normalize_codex_model(model),
            "gpt-5.6-sol",
            "{model}"
        );
    }
}

#[test]
fn test_codex_display_label() {
    assert_eq!(
        CostUsagePricing::codex_display_label("gpt-5.3-codex-spark"),
        Some("Research Preview")
    );
    assert_eq!(CostUsagePricing::codex_display_label("gpt-5.4"), None);
}

#[test]
fn generic_pricing_keeps_family_models_on_the_canonical_tables() {
    // The Codex table takes an input count inclusive of cached tokens; the
    // generic API takes the non-cached portion and adapts before delegating.
    let generic = CostUsagePricing::generic_model_cost_usd_with_root(
        &["github-copilot"],
        "gpt-5",
        140,
        10,
        45,
        None,
    )
    .expect("gpt-5 is priced");
    let direct = CostUsagePricing::codex_cost_usd("gpt-5", 150, 10, 45).expect("direct");
    assert!((generic - direct).abs() < 1e-15);

    let generic_claude = CostUsagePricing::generic_model_cost_usd_with_root(
        &["github-copilot"],
        "claude-opus-4-8",
        1_000,
        0,
        500,
        None,
    )
    .expect("claude is priced");
    let direct_claude =
        CostUsagePricing::claude_cost_usd("claude-opus-4-8", 1_000, 0, 0, 500).expect("direct");
    assert!((generic_claude - direct_claude).abs() < 1e-15);
}

#[test]
fn generic_pricing_returns_none_when_no_rate_resolves() {
    let empty = tempfile::tempdir().expect("temp dir");
    assert!(
        CostUsagePricing::generic_model_cost_usd_with_root(
            &["google"],
            "gemini-mystery-model",
            1_000,
            0,
            500,
            Some(empty.path()),
        )
        .is_none(),
        "unknown model with an empty catalog root must stay unpriced"
    );
    assert!(
        CostUsagePricing::generic_model_cost_usd_with_root(
            &["google"],
            "",
            1,
            0,
            1,
            Some(empty.path()),
        )
        .is_none(),
        "an empty model id is never priced"
    );
}
