//! Cost Usage Pricing
//!
//! Model-specific token pricing for Codex (OpenAI) and Claude (Anthropic) models.
//! Supports tiered pricing for models with token thresholds.

#![allow(dead_code)]

use super::{codex_routed_pricing, models_dev_pricing};
use chrono::NaiveDate;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

/// Whole-request Codex rates for input above the model context threshold.
#[derive(Debug, Clone, Copy)]
pub struct CodexLongContextRates {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    pub cache_read_input_cost_per_token: f64,
}

/// Codex (OpenAI) model pricing
#[derive(Debug, Clone, Copy)]
pub struct CodexPricing {
    /// Cost per input token in USD
    pub input_cost_per_token: f64,
    /// Cost per output token in USD
    pub output_cost_per_token: f64,
    /// Cost per cached input token in USD
    pub cache_read_input_cost_per_token: f64,
    /// Optional display label override (e.g. "Research Preview")
    pub display_label: Option<&'static str>,
    /// Whole-request rates above the Codex long-context threshold.
    pub long_context: Option<CodexLongContextRates>,
}

/// Claude (Anthropic) model pricing with optional tiered pricing
#[derive(Debug, Clone, Copy)]
pub struct ClaudePricing {
    /// Cost per input token in USD
    pub input_cost_per_token: f64,
    /// Cost per output token in USD
    pub output_cost_per_token: f64,
    /// Cost per cache creation input token in USD
    pub cache_creation_input_cost_per_token: f64,
    /// Cost per cache read input token in USD
    pub cache_read_input_cost_per_token: f64,
    /// Token threshold for tiered pricing (None = no tiering)
    pub threshold_tokens: Option<i32>,
    /// Cost per input token above threshold
    pub input_cost_per_token_above_threshold: Option<f64>,
    /// Cost per output token above threshold
    pub output_cost_per_token_above_threshold: Option<f64>,
    /// Cost per cache creation input token above threshold
    pub cache_creation_input_cost_per_token_above_threshold: Option<f64>,
    /// Cost per cache read input token above threshold
    pub cache_read_input_cost_per_token_above_threshold: Option<f64>,
}

/// Codex model pricing table
static CODEX_PRICING: LazyLock<HashMap<&'static str, CodexPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // GPT-5 pricing
    m.insert(
        "gpt-5",
        CodexPricing {
            input_cost_per_token: 1.25e-6,
            output_cost_per_token: 1e-5,
            cache_read_input_cost_per_token: 1.25e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5-codex",
        CodexPricing {
            input_cost_per_token: 1.25e-6,
            output_cost_per_token: 1e-5,
            cache_read_input_cost_per_token: 1.25e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5-mini",
        CodexPricing {
            input_cost_per_token: 2.5e-7,
            output_cost_per_token: 2e-6,
            cache_read_input_cost_per_token: 2.5e-8,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5-nano",
        CodexPricing {
            input_cost_per_token: 5e-8,
            output_cost_per_token: 4e-7,
            cache_read_input_cost_per_token: 5e-9,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5-pro",
        CodexPricing {
            input_cost_per_token: 1.5e-5,
            output_cost_per_token: 1.2e-4,
            cache_read_input_cost_per_token: 1.5e-5,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.1",
        CodexPricing {
            input_cost_per_token: 1.25e-6,
            output_cost_per_token: 1e-5,
            cache_read_input_cost_per_token: 1.25e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.1-codex",
        CodexPricing {
            input_cost_per_token: 1.25e-6,
            output_cost_per_token: 1e-5,
            cache_read_input_cost_per_token: 1.25e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.1-codex-max",
        CodexPricing {
            input_cost_per_token: 1.25e-6,
            output_cost_per_token: 1e-5,
            cache_read_input_cost_per_token: 1.25e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.1-codex-mini",
        CodexPricing {
            input_cost_per_token: 2.5e-7,
            output_cost_per_token: 2e-6,
            cache_read_input_cost_per_token: 2.5e-8,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.2",
        CodexPricing {
            input_cost_per_token: 1.75e-6,
            output_cost_per_token: 1.4e-5,
            cache_read_input_cost_per_token: 1.75e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.2-codex",
        CodexPricing {
            input_cost_per_token: 1.75e-6,
            output_cost_per_token: 1.4e-5,
            cache_read_input_cost_per_token: 1.75e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.2-pro",
        CodexPricing {
            input_cost_per_token: 2.1e-5,
            output_cost_per_token: 1.68e-4,
            cache_read_input_cost_per_token: 2.1e-5,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.3-codex",
        CodexPricing {
            input_cost_per_token: 1.75e-6,
            output_cost_per_token: 1.4e-5,
            cache_read_input_cost_per_token: 1.75e-7,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.3-codex-spark",
        CodexPricing {
            input_cost_per_token: 0.0,
            output_cost_per_token: 0.0,
            cache_read_input_cost_per_token: 0.0,
            display_label: Some("Research Preview"),
            long_context: None,
        },
    );

    // GPT-5.4 pricing (updated to match upstream 0.22; long-context rates
    // from OpenAI official pricing, 2026-10-01: 5/0.50/22.50 above 272K)
    m.insert(
        "gpt-5.4",
        CodexPricing {
            input_cost_per_token: 2.5e-6,
            output_cost_per_token: 1.5e-5,
            cache_read_input_cost_per_token: 2.5e-7,
            display_label: None,
            long_context: Some(CodexLongContextRates {
                input_cost_per_token: 5e-6,
                output_cost_per_token: 2.25e-5,
                cache_read_input_cost_per_token: 5e-7,
            }),
        },
    );
    m.insert(
        "gpt-5.4-codex",
        CodexPricing {
            input_cost_per_token: 2.5e-6,
            output_cost_per_token: 1.5e-5,
            cache_read_input_cost_per_token: 2.5e-7,
            display_label: None,
            long_context: None,
        },
    );

    // GPT-5.4 Mini pricing (updated to match upstream 0.22)
    m.insert(
        "gpt-5.4-mini",
        CodexPricing {
            input_cost_per_token: 7.5e-7,
            output_cost_per_token: 4.5e-6,
            cache_read_input_cost_per_token: 7.5e-8,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.4-mini-codex",
        CodexPricing {
            input_cost_per_token: 7.5e-7,
            output_cost_per_token: 4.5e-6,
            cache_read_input_cost_per_token: 7.5e-8,
            display_label: None,
            long_context: None,
        },
    );

    // GPT-5.4 Nano pricing (updated to match upstream 0.22)
    m.insert(
        "gpt-5.4-nano",
        CodexPricing {
            input_cost_per_token: 2e-7,
            output_cost_per_token: 1.25e-6,
            cache_read_input_cost_per_token: 2e-8,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.4-nano-codex",
        CodexPricing {
            input_cost_per_token: 2e-7,
            output_cost_per_token: 1.25e-6,
            cache_read_input_cost_per_token: 2e-8,
            display_label: None,
            long_context: None,
        },
    );

    // GPT-5.4 Pro
    m.insert(
        "gpt-5.4-pro",
        CodexPricing {
            input_cost_per_token: 3e-5,
            output_cost_per_token: 1.8e-4,
            cache_read_input_cost_per_token: 3e-5,
            display_label: None,
            long_context: None,
        },
    );
    m.insert(
        "gpt-5.5",
        CodexPricing {
            input_cost_per_token: 5e-6,
            output_cost_per_token: 3e-5,
            cache_read_input_cost_per_token: 5e-7,
            display_label: None,
            // OpenAI official long-context pricing, 2026-10-01: 10/1/45.
            long_context: Some(CodexLongContextRates {
                input_cost_per_token: 1e-5,
                output_cost_per_token: 4.5e-5,
                cache_read_input_cost_per_token: 1e-6,
            }),
        },
    );
    m.insert(
        "gpt-5.5-pro",
        CodexPricing {
            input_cost_per_token: 3e-5,
            output_cost_per_token: 1.8e-4,
            // OpenAI lists no cached-input discount for Pro; cache reads price
            // at the input rate.
            cache_read_input_cost_per_token: 3e-5,
            display_label: None,
            // OpenAI official long-context pricing, 2026-10-01: 60/-/270.
            long_context: Some(CodexLongContextRates {
                input_cost_per_token: 6e-5,
                output_cost_per_token: 2.7e-4,
                cache_read_input_cost_per_token: 6e-5,
            }),
        },
    );
    m.insert(
        "gpt-5.6-sol",
        CodexPricing {
            // OpenAI official pricing (developers.openai.com, 2026-10-01):
            // short context 4/0.40/20, long context 8/0.80/30 — promotional
            // rates per OpenAI's footnote, offered through at least
            // 2026-11-21. Usage before the 2026-07-30 price cutover prices at
            // the historical 5/30 short and 10/45/1 long rates (see
            // `codex_cost_usd_at_date`).
            input_cost_per_token: 4e-6,
            output_cost_per_token: 2e-5,
            cache_read_input_cost_per_token: 4e-7,
            display_label: None,
            long_context: Some(CodexLongContextRates {
                input_cost_per_token: 8e-6,
                output_cost_per_token: 3e-5,
                cache_read_input_cost_per_token: 8e-7,
            }),
        },
    );
    m.insert(
        "gpt-5.6-terra",
        CodexPricing {
            // Upstream 0.50.1 post-2026-07-30 rates.
            input_cost_per_token: 2e-6,
            output_cost_per_token: 1.2e-5,
            cache_read_input_cost_per_token: 2e-7,
            display_label: None,
            long_context: Some(CodexLongContextRates {
                input_cost_per_token: 4e-6,
                output_cost_per_token: 1.8e-5,
                cache_read_input_cost_per_token: 4e-7,
            }),
        },
    );
    m.insert(
        "gpt-5.6-luna",
        CodexPricing {
            input_cost_per_token: 2e-7,
            output_cost_per_token: 1.2e-6,
            cache_read_input_cost_per_token: 2e-8,
            display_label: None,
            long_context: Some(CodexLongContextRates {
                input_cost_per_token: 4e-7,
                output_cost_per_token: 1.8e-6,
                cache_read_input_cost_per_token: 4e-8,
            }),
        },
    );

    m
});

const CODEX_LONG_CONTEXT_THRESHOLD: u64 = 272_000;

/// Claude model pricing table
static CLAUDE_PRICING: LazyLock<HashMap<&'static str, ClaudePricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();

    // Fable 5
    m.insert(
        "claude-fable-5",
        ClaudePricing {
            input_cost_per_token: 1e-5,
            output_cost_per_token: 5e-5,
            cache_creation_input_cost_per_token: 1.25e-5,
            cache_read_input_cost_per_token: 1e-6,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );

    // Haiku 4.5
    m.insert(
        "claude-haiku-4-5",
        ClaudePricing {
            input_cost_per_token: 1e-6,
            output_cost_per_token: 5e-6,
            cache_creation_input_cost_per_token: 1.25e-6,
            cache_read_input_cost_per_token: 1e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );
    m.insert(
        "claude-haiku-4-5-20251001",
        ClaudePricing {
            input_cost_per_token: 1e-6,
            output_cost_per_token: 5e-6,
            cache_creation_input_cost_per_token: 1.25e-6,
            cache_read_input_cost_per_token: 1e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );

    // Opus 4.6
    m.insert(
        "claude-opus-4-6",
        ClaudePricing {
            input_cost_per_token: 5e-6,
            output_cost_per_token: 2.5e-5,
            cache_creation_input_cost_per_token: 6.25e-6,
            cache_read_input_cost_per_token: 5e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );
    m.insert(
        "claude-opus-4-6-20260205",
        ClaudePricing {
            input_cost_per_token: 5e-6,
            output_cost_per_token: 2.5e-5,
            cache_creation_input_cost_per_token: 6.25e-6,
            cache_read_input_cost_per_token: 5e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );

    // Opus 4.7 (same pricing as Opus 4.6)
    m.insert(
        "claude-opus-4-7",
        ClaudePricing {
            input_cost_per_token: 5e-6,
            output_cost_per_token: 2.5e-5,
            cache_creation_input_cost_per_token: 6.25e-6,
            cache_read_input_cost_per_token: 5e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );

    // Opus 4.8 (same pricing as Opus 4.5/4.6/4.7)
    m.insert(
        "claude-opus-4-8",
        ClaudePricing {
            input_cost_per_token: 5e-6,
            output_cost_per_token: 2.5e-5,
            cache_creation_input_cost_per_token: 6.25e-6,
            cache_read_input_cost_per_token: 5e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );

    // Opus 4.5
    m.insert(
        "claude-opus-4-5",
        ClaudePricing {
            input_cost_per_token: 5e-6,
            output_cost_per_token: 2.5e-5,
            cache_creation_input_cost_per_token: 6.25e-6,
            cache_read_input_cost_per_token: 5e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );
    m.insert(
        "claude-opus-4-5-20251101",
        ClaudePricing {
            input_cost_per_token: 5e-6,
            output_cost_per_token: 2.5e-5,
            cache_creation_input_cost_per_token: 6.25e-6,
            cache_read_input_cost_per_token: 5e-7,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );

    // Sonnet 4.5 (with tiered pricing at 200k tokens)
    m.insert(
        "claude-sonnet-4-5",
        ClaudePricing {
            input_cost_per_token: 3e-6,
            output_cost_per_token: 1.5e-5,
            cache_creation_input_cost_per_token: 3.75e-6,
            cache_read_input_cost_per_token: 3e-7,
            threshold_tokens: Some(200_000),
            input_cost_per_token_above_threshold: Some(6e-6),
            output_cost_per_token_above_threshold: Some(2.25e-5),
            cache_creation_input_cost_per_token_above_threshold: Some(7.5e-6),
            cache_read_input_cost_per_token_above_threshold: Some(6e-7),
        },
    );
    m.insert(
        "claude-sonnet-4-5-20250929",
        ClaudePricing {
            input_cost_per_token: 3e-6,
            output_cost_per_token: 1.5e-5,
            cache_creation_input_cost_per_token: 3.75e-6,
            cache_read_input_cost_per_token: 3e-7,
            threshold_tokens: Some(200_000),
            input_cost_per_token_above_threshold: Some(6e-6),
            output_cost_per_token_above_threshold: Some(2.25e-5),
            cache_creation_input_cost_per_token_above_threshold: Some(7.5e-6),
            cache_read_input_cost_per_token_above_threshold: Some(6e-7),
        },
    );

    // Sonnet 4.6 (same pricing as Sonnet 4.5, with 200k tier)
    m.insert(
        "claude-sonnet-4-6",
        ClaudePricing {
            input_cost_per_token: 3e-6,
            output_cost_per_token: 1.5e-5,
            cache_creation_input_cost_per_token: 3.75e-6,
            cache_read_input_cost_per_token: 3e-7,
            threshold_tokens: Some(200_000),
            input_cost_per_token_above_threshold: Some(6e-6),
            output_cost_per_token_above_threshold: Some(2.25e-5),
            cache_creation_input_cost_per_token_above_threshold: Some(7.5e-6),
            cache_read_input_cost_per_token_above_threshold: Some(6e-7),
        },
    );

    // Opus 4
    m.insert(
        "claude-opus-4-20250514",
        ClaudePricing {
            input_cost_per_token: 1.5e-5,
            output_cost_per_token: 7.5e-5,
            cache_creation_input_cost_per_token: 1.875e-5,
            cache_read_input_cost_per_token: 1.5e-6,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );
    m.insert(
        "claude-opus-4-1",
        ClaudePricing {
            input_cost_per_token: 1.5e-5,
            output_cost_per_token: 7.5e-5,
            cache_creation_input_cost_per_token: 1.875e-5,
            cache_read_input_cost_per_token: 1.5e-6,
            threshold_tokens: None,
            input_cost_per_token_above_threshold: None,
            output_cost_per_token_above_threshold: None,
            cache_creation_input_cost_per_token_above_threshold: None,
            cache_read_input_cost_per_token_above_threshold: None,
        },
    );

    // Sonnet 4
    m.insert(
        "claude-sonnet-4-20250514",
        ClaudePricing {
            input_cost_per_token: 3e-6,
            output_cost_per_token: 1.5e-5,
            cache_creation_input_cost_per_token: 3.75e-6,
            cache_read_input_cost_per_token: 3e-7,
            threshold_tokens: Some(200_000),
            input_cost_per_token_above_threshold: Some(6e-6),
            output_cost_per_token_above_threshold: Some(2.25e-5),
            cache_creation_input_cost_per_token_above_threshold: Some(7.5e-6),
            cache_read_input_cost_per_token_above_threshold: Some(6e-7),
        },
    );

    m
});

fn codex_cost_from_rates(
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    input_rate: f64,
    cache_read_rate: f64,
    output_rate: f64,
) -> f64 {
    let cached = cached_input_tokens.min(input_tokens);
    let non_cached = input_tokens.saturating_sub(cached);
    (non_cached as f64) * input_rate
        + (cached as f64) * cache_read_rate
        + (output_tokens as f64) * output_rate
}

/// Cost usage pricing utilities
pub struct CostUsagePricing;

impl CostUsagePricing {
    /// Sentinel model key for model-less Codex token events.
    ///
    /// Usage remains visible under this key but is never priced as a real model
    /// (including catalog collisions with a generic "unknown" entry).
    pub const CODEX_UNATTRIBUTED_MODEL: &'static str = "unknown";

    /// True when `model` is the unattributed / model-less sentinel.
    pub fn is_codex_unattributed_model(model: &str) -> bool {
        Self::normalize_codex_model(model) == Self::CODEX_UNATTRIBUTED_MODEL
    }

    /// Provider-qualified routes are not native Codex subscription usage.
    pub fn counts_toward_codex_subscription(model: &str) -> bool {
        codex_routed_pricing::counts_toward_codex_subscription(model)
    }

    /// Return the known provider for a routed model, if one is available.
    pub fn codex_routed_provider(model: &str) -> Option<&'static str> {
        codex_routed_pricing::codex_routed_provider(model)
    }

    /// Normalize a Codex model name for pricing lookup
    pub fn normalize_codex_model(raw: &str) -> String {
        let mut trimmed = raw.trim().to_string();
        if trimmed.is_empty()
            || trimmed.eq_ignore_ascii_case("unknown")
            || trimmed.eq_ignore_ascii_case("unpriced")
        {
            return Self::CODEX_UNATTRIBUTED_MODEL.to_string();
        }

        // Remove the explicit native route prefix without making the model
        // id case-sensitive.
        if let Some((prefix, rest)) = trimmed.split_once('/')
            && prefix.eq_ignore_ascii_case("openai")
        {
            trimmed = rest.to_string();
        }

        // Check if base model (without -codex suffix) exists in pricing
        if let Some(idx) = trimmed.find("-codex") {
            let base = &trimmed[..idx];
            if CODEX_PRICING.contains_key(base) || base == "gpt-5.6" {
                trimmed = base.to_string();
            }
        }

        let date_pattern = regex_lite::Regex::new(r"-\d{4}-\d{2}-\d{2}$").unwrap();
        if let Some(mat) = date_pattern.find(&trimmed) {
            let base = &trimmed[..mat.start()];
            if CODEX_PRICING.contains_key(base) || base == "gpt-5.6" {
                trimmed = base.to_string();
            }
        }

        if trimmed == "gpt-5.6" {
            return "gpt-5.6-sol".to_string();
        }

        trimmed
    }

    /// Get the display label for a Codex model (e.g. "Research Preview")
    pub fn codex_display_label(model: &str) -> Option<&'static str> {
        let key = Self::normalize_codex_model(model);
        CODEX_PRICING
            .get(key.as_str())
            .and_then(|p| p.display_label)
    }

    /// Normalize a Claude model name for pricing lookup
    pub fn normalize_claude_model(raw: &str) -> String {
        let mut trimmed = raw.trim().to_string();

        // Remove "anthropic." prefix
        if let Some(rest) = trimmed.strip_prefix("anthropic.") {
            trimmed = rest.to_string();
        }

        // Handle nested model names like "anthropic.claude-sonnet-4.claude-sonnet-4-20250514"
        if trimmed.contains("claude-")
            && let Some(last_dot) = trimmed.rfind('.')
        {
            let tail = &trimmed[last_dot + 1..];
            if tail.starts_with("claude-") {
                trimmed = tail.to_string();
            }
        }

        // Remove version suffix like "-v1:0"
        let version_pattern = regex_lite::Regex::new(r"-v\d+:\d+$").unwrap();
        trimmed = version_pattern.replace(&trimmed, "").to_string();

        // Try without date suffix if base exists in pricing
        let date_pattern = regex_lite::Regex::new(r"-\d{8}$").unwrap();
        if let Some(mat) = date_pattern.find(&trimmed) {
            let base = &trimmed[..mat.start()];
            if CLAUDE_PRICING.contains_key(base) {
                return base.to_string();
            }
        }

        trimmed
    }

    /// Calculate cost for Codex usage in USD
    pub fn codex_cost_usd_at_date(
        model: &str,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        pricing_date: NaiveDate,
    ) -> Option<f64> {
        // Terra/Luna had a higher pre-cutover price.  Historical charts must
        // use the rate in effect on the event day rather than today's table.
        let cutoff = NaiveDate::from_ymd_opt(2026, 7, 30).expect("valid pricing cutoff");
        if pricing_date < cutoff {
            let key = Self::normalize_codex_model(model);
            let rates = match (key.as_str(), input_tokens > CODEX_LONG_CONTEXT_THRESHOLD) {
                ("gpt-5.6-terra", false) => Some((2.5e-6, 2.5e-7, 1.5e-5)),
                ("gpt-5.6-terra", true) => Some((5e-6, 5e-7, 2.25e-5)),
                ("gpt-5.6-luna", false) => Some((1e-6, 1e-7, 6e-6)),
                ("gpt-5.6-luna", true) => Some((2e-6, 2e-7, 9e-6)),
                ("gpt-5.6-sol", false) => Some((5e-6, 5e-7, 3e-5)),
                ("gpt-5.6-sol", true) => Some((1e-5, 1e-6, 4.5e-5)),
                _ => None,
            };
            if let Some((input_rate, cache_rate, output_rate)) = rates {
                return Some(codex_cost_from_rates(
                    input_tokens,
                    cached_input_tokens,
                    output_tokens,
                    input_rate,
                    cache_rate,
                    output_rate,
                ));
            }
        }
        Self::codex_cost_usd(model, input_tokens, cached_input_tokens, output_tokens)
    }

    /// Calculate cost for Codex usage in USD
    pub fn codex_cost_usd(
        model: &str,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
    ) -> Option<f64> {
        let key = Self::normalize_codex_model(model);
        // Model-less / deliberately unattributed usage stays unpriced even if a
        // pricing catalog later contains a colliding generic entry.
        if key == Self::CODEX_UNATTRIBUTED_MODEL {
            return None;
        }

        // A fresh daily catalog outranks the built-in table: promotional or
        // revised upstream prices self-correct within a day of models.dev
        // tracking them, with no release required. The catalog does not carry
        // OpenAI long-context tiers, so the built-in long-context table (when
        // present) still applies on top of the fresh base rates. Unit tests
        // pin the built-in tables deterministically and skip this path; the
        // rate application is shared and covered below.
        #[cfg(not(test))]
        if let Some(dynamic) = models_dev_pricing::lookup("openai", model) {
            let cache_rate = dynamic
                .cache_read_input_cost_per_token
                .unwrap_or(dynamic.input_cost_per_token);
            let long_context = CODEX_PRICING
                .get(key.as_str())
                .and_then(|pricing| pricing.long_context);
            let (input_rate, cache_read_rate, output_rate) =
                if input_tokens > CODEX_LONG_CONTEXT_THRESHOLD {
                    long_context.map_or(
                        (
                            dynamic.input_cost_per_token,
                            cache_rate,
                            dynamic.output_cost_per_token,
                        ),
                        |lc| {
                            (
                                lc.input_cost_per_token,
                                lc.cache_read_input_cost_per_token,
                                lc.output_cost_per_token,
                            )
                        },
                    )
                } else {
                    (
                        dynamic.input_cost_per_token,
                        cache_rate,
                        dynamic.output_cost_per_token,
                    )
                };
            return Some(codex_cost_from_rates(
                input_tokens,
                cached_input_tokens,
                output_tokens,
                input_rate,
                cache_read_rate,
                output_rate,
            ));
        }

        if let Some(pricing) = CODEX_PRICING.get(key.as_str()) {
            let (input_rate, cache_read_rate, output_rate) =
                if input_tokens > CODEX_LONG_CONTEXT_THRESHOLD {
                    if let Some(long_context) = pricing.long_context {
                        (
                            long_context.input_cost_per_token,
                            long_context.cache_read_input_cost_per_token,
                            long_context.output_cost_per_token,
                        )
                    } else {
                        (
                            pricing.input_cost_per_token,
                            pricing.cache_read_input_cost_per_token,
                            pricing.output_cost_per_token,
                        )
                    }
                } else {
                    (
                        pricing.input_cost_per_token,
                        pricing.cache_read_input_cost_per_token,
                        pricing.output_cost_per_token,
                    )
                };
            return Some(codex_cost_from_rates(
                input_tokens,
                cached_input_tokens,
                output_tokens,
                input_rate,
                cache_read_rate,
                output_rate,
            ));
        }

        // Never guess an OpenAI price for a provider-qualified route.  The
        // route either has its own provider catalog or remains explicitly
        // unpriced; both cases are safer than inflating native Codex usage.
        let (provider, lookup_model) = match codex_routed_pricing::codex_routed_provider(model) {
            Some(provider) => (provider, codex_routed_pricing::strip_route_prefix(model)),
            None if model
                .trim()
                .split_once('/')
                .is_some_and(|(prefix, _)| !prefix.eq_ignore_ascii_case("openai")) =>
            {
                return None;
            }
            None => {
                let lookup_model = model
                    .trim()
                    .split_once('/')
                    .filter(|(prefix, _)| prefix.eq_ignore_ascii_case("openai"))
                    .map(|(_, rest)| rest)
                    .unwrap_or(model);
                ("openai", lookup_model)
            }
        };
        let pricing = models_dev_pricing::lookup(provider, lookup_model)?;
        let use_tier = pricing
            .threshold_tokens
            .is_some_and(|threshold| input_tokens > threshold);
        Some(codex_cost_from_rates(
            input_tokens,
            cached_input_tokens,
            output_tokens,
            if use_tier {
                pricing
                    .input_cost_per_token_above_threshold
                    .unwrap_or(pricing.input_cost_per_token)
            } else {
                pricing.input_cost_per_token
            },
            if use_tier {
                pricing
                    .cache_read_input_cost_per_token_above_threshold
                    .or(pricing.cache_read_input_cost_per_token)
                    .unwrap_or(pricing.input_cost_per_token)
            } else {
                pricing
                    .cache_read_input_cost_per_token
                    .unwrap_or(pricing.input_cost_per_token)
            },
            if use_tier {
                pricing
                    .output_cost_per_token_above_threshold
                    .unwrap_or(pricing.output_cost_per_token)
            } else {
                pricing.output_cost_per_token
            },
        ))
    }

    /// Calculate cost for Claude usage in USD
    pub fn claude_cost_usd(
        model: &str,
        input_tokens: i32,
        cache_read_input_tokens: i32,
        cache_creation_input_tokens: i32,
        output_tokens: i32,
    ) -> Option<f64> {
        let key = Self::normalize_claude_model(model);

        // Fresh daily catalog first (production builds): the same precedence
        // rule as the Codex path — promotional or revised prices self-correct
        // without a release. Unit tests pin the built-in table and skip this
        // path; the application helper is shared with the stale-catalog
        // fallback below.
        #[cfg(not(test))]
        if let Some(pricing) = models_dev_pricing::lookup("anthropic", model)
            .or_else(|| models_dev_pricing::lookup_model_anywhere(model))
        {
            // GLM/DeepSeek/Kimi token lines routed through Claude Code are not
            // Anthropic models: the cross-provider lookup prices them at their
            // own vendor rates instead of the Sonnet fallback.
            return Some(Self::claude_cost_from_dynamic_pricing(
                pricing,
                input_tokens,
                cache_read_input_tokens,
                cache_creation_input_tokens,
                output_tokens,
            ));
        }

        if let Some(pricing) = CLAUDE_PRICING.get(key.as_str()) {
            /// Calculate tiered cost
            fn tiered(tokens: i32, base: f64, above: Option<f64>, threshold: Option<i32>) -> f64 {
                let tokens = tokens.max(0);
                match (threshold, above) {
                    (Some(thresh), Some(above_rate)) => {
                        let below = tokens.min(thresh);
                        let over = (tokens - thresh).max(0);
                        (below as f64) * base + (over as f64) * above_rate
                    }
                    _ => (tokens as f64) * base,
                }
            }

            let cost = tiered(
                input_tokens,
                pricing.input_cost_per_token,
                pricing.input_cost_per_token_above_threshold,
                pricing.threshold_tokens,
            ) + tiered(
                cache_read_input_tokens,
                pricing.cache_read_input_cost_per_token,
                pricing.cache_read_input_cost_per_token_above_threshold,
                pricing.threshold_tokens,
            ) + tiered(
                cache_creation_input_tokens,
                pricing.cache_creation_input_cost_per_token,
                pricing.cache_creation_input_cost_per_token_above_threshold,
                pricing.threshold_tokens,
            ) + tiered(
                output_tokens,
                pricing.output_cost_per_token,
                pricing.output_cost_per_token_above_threshold,
                pricing.threshold_tokens,
            );

            return Some(cost);
        }

        let pricing = models_dev_pricing::lookup("anthropic", model)
            .or_else(|| models_dev_pricing::lookup_model_anywhere(model))?;
        Some(Self::claude_cost_from_dynamic_pricing(
            pricing,
            input_tokens,
            cache_read_input_tokens,
            cache_creation_input_tokens,
            output_tokens,
        ))
    }

    /// Apply a dynamic-catalog price to Claude token counts. Shared by the
    /// fresh-catalog path (production, runs before the built-in table) and
    /// the stale-catalog fallback after a built-in miss.
    fn claude_cost_from_dynamic_pricing(
        pricing: crate::core::models_dev_pricing::DynamicModelPricing,
        input_tokens: i32,
        cache_read_input_tokens: i32,
        cache_creation_input_tokens: i32,
        output_tokens: i32,
    ) -> f64 {
        let input_tokens = input_tokens.max(0);
        let cache_read_input_tokens = cache_read_input_tokens.max(0);
        let cache_creation_input_tokens = cache_creation_input_tokens.max(0);
        let output_tokens = output_tokens.max(0);
        let use_tier = pricing.threshold_tokens.is_some_and(|threshold| {
            (input_tokens as u64)
                + (cache_read_input_tokens as u64)
                + (cache_creation_input_tokens as u64)
                > threshold
        });
        let input_rate = if use_tier {
            pricing
                .input_cost_per_token_above_threshold
                .unwrap_or(pricing.input_cost_per_token)
        } else {
            pricing.input_cost_per_token
        };
        let cache_read_rate = if use_tier {
            pricing
                .cache_read_input_cost_per_token_above_threshold
                .or(pricing.cache_read_input_cost_per_token)
                .unwrap_or(input_rate)
        } else {
            pricing
                .cache_read_input_cost_per_token
                .unwrap_or(input_rate)
        };
        let cache_write_rate = if use_tier {
            pricing
                .cache_write_input_cost_per_token_above_threshold
                .or(pricing.cache_write_input_cost_per_token)
                .unwrap_or(input_rate)
        } else {
            pricing
                .cache_write_input_cost_per_token
                .unwrap_or(input_rate)
        };
        let output_rate = if use_tier {
            pricing
                .output_cost_per_token_above_threshold
                .unwrap_or(pricing.output_cost_per_token)
        } else {
            pricing.output_cost_per_token
        };
        (input_tokens as f64) * input_rate
            + (cache_read_input_tokens as f64) * cache_read_rate
            + (cache_creation_input_tokens as f64) * cache_write_rate
            + (output_tokens as f64) * output_rate
    }

    /// Base per-token input rate for a Claude model. Exposed for callers that
    /// need a rate the standard cost function doesn't model — e.g. the usage
    /// scanner's one-hour cache-write premium, billed at 2x the input rate.
    pub fn claude_input_cost_per_token(model: &str) -> Option<f64> {
        let key = Self::normalize_claude_model(model);
        if let Some(pricing) = CLAUDE_PRICING.get(key.as_str()) {
            return Some(pricing.input_cost_per_token);
        }
        models_dev_pricing::lookup("anthropic", model).map(|p| p.input_cost_per_token)
    }

    /// Price a token bundle for an arbitrary provider/model pair.
    ///
    /// Order: the canonical embedded OpenAI/Anthropic tables first (exact and
    /// offline), then the models.dev catalog through the supplied provider
    /// candidates. `None` means no canonical rate is known; callers must mark
    /// the model unpriced instead of guessing a dollar value. `input_tokens`
    /// is the non-cached portion, `cached_input_tokens` the read/cache portion
    /// and `output_tokens` the generated portion.
    pub(crate) fn generic_model_cost_usd_with_root(
        provider_candidates: &[&str],
        model: &str,
        input_tokens: u64,
        cached_input_tokens: u64,
        output_tokens: u64,
        cache_root: Option<&Path>,
    ) -> Option<f64> {
        let model = model.trim();
        if model.is_empty() {
            return None;
        }
        let lower = model.to_ascii_lowercase();
        let clamp = |value: u64| value.min(i32::MAX as u64) as i32;

        // Family-routed models stay on the canonical tables so Codex/Claude
        // costs remain identical no matter which provider's logs carried them
        // (e.g. the same Claude model appearing through Copilot).
        if lower.contains("claude")
            && let Some(cost) = Self::claude_cost_usd(
                model,
                clamp(input_tokens),
                clamp(cached_input_tokens),
                0,
                clamp(output_tokens),
            )
        {
            return Some(cost);
        }

        let openai_family = lower.starts_with("gpt-")
            || lower.starts_with("o1")
            || lower.starts_with("o3")
            || lower.starts_with("o4")
            || lower.contains("codex");
        if openai_family
            && let Some(cost) = Self::codex_cost_usd(
                model,
                // The Codex path expects an input count inclusive of cached
                // tokens and subtracts the cached portion itself, while this
                // API takes the non-cached portion separately.
                input_tokens.saturating_add(cached_input_tokens),
                cached_input_tokens,
                output_tokens,
            )
        {
            return Some(cost);
        }

        for provider in provider_candidates {
            if let Some(pricing) = models_dev_pricing::lookup_with_root(provider, model, cache_root)
            {
                return Some(dynamic_pricing_cost(
                    &pricing,
                    input_tokens,
                    cached_input_tokens,
                    output_tokens,
                ));
            }
        }
        None
    }

    /// Format model name for display (e.g., "claude-3.5-sonnet" → "Sonnet 3.5")
    pub fn format_model_name(model: &str) -> String {
        let lower = model.to_lowercase();

        // GPT models: format as "GPT-{version}[ Mini| Nano]"
        if lower.contains("gpt-") {
            let version = regex_lite::Regex::new(r"gpt-(\d+(?:\.\d+)?)")
                .ok()
                .and_then(|re| re.captures(&lower))
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().to_string());

            let suffix = if lower.contains("nano") {
                " Nano"
            } else if lower.contains("mini") {
                " Mini"
            } else {
                ""
            };

            return match version {
                Some(v) => format!("GPT-{}{}", v, suffix),
                None => model.to_string(),
            };
        }

        // Claude models: extract version and family
        let version = regex_lite::Regex::new(r"(\d+(?:\.\d+)?)")
            .ok()
            .and_then(|re| re.find(&lower))
            .map(|m| m.as_str().to_string());

        let family = if lower.contains("opus") {
            "Opus"
        } else if lower.contains("sonnet") {
            "Sonnet"
        } else if lower.contains("haiku") {
            "Haiku"
        } else {
            return model.to_string();
        };

        match version {
            Some(v) => format!("{} {}", family, v),
            None => family.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "cost_pricing_tests.rs"]
mod tests;

/// Cost of one token bundle at catalog rates, honouring the >200k context
/// tier when the catalog publishes one.
fn dynamic_pricing_cost(
    pricing: &models_dev_pricing::DynamicModelPricing,
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
) -> f64 {
    let use_tier = pricing
        .threshold_tokens
        .is_some_and(|threshold| input_tokens > threshold);
    let input_rate = if use_tier {
        pricing
            .input_cost_per_token_above_threshold
            .unwrap_or(pricing.input_cost_per_token)
    } else {
        pricing.input_cost_per_token
    };
    let cache_read_rate = if use_tier {
        pricing
            .cache_read_input_cost_per_token_above_threshold
            .or(pricing.cache_read_input_cost_per_token)
            .unwrap_or(input_rate)
    } else {
        pricing
            .cache_read_input_cost_per_token
            .unwrap_or(input_rate)
    };
    let output_rate = if use_tier {
        pricing
            .output_cost_per_token_above_threshold
            .unwrap_or(pricing.output_cost_per_token)
    } else {
        pricing.output_cost_per_token
    };
    (input_tokens as f64) * input_rate
        + (cached_input_tokens as f64) * cache_read_rate
        + (output_tokens as f64) * output_rate
}
