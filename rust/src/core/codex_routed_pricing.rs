//! Classification helpers for provider-qualified models found in Codex logs.
//!
//! A Codex transcript can contain requests dispatched through another provider
//! (for example `deepseek/...` or `opencode-go/...`).  Those rows are useful for
//! diagnostics, but they do not belong to the native OpenAI/Codex subscription
//! total.  Keep the decision in one small module so scanners and pricing use the
//! same rule.

/// Return the provider portion of a known routed model identifier.
///
/// The returned value is intended for a models.dev lookup.  Unknown prefixes
/// are deliberately not guessed: callers should leave those rows unpriced.
pub fn codex_routed_provider(model: &str) -> Option<&'static str> {
    let (prefix, _) = model.trim().split_once('/')?;
    match prefix.to_ascii_lowercase().as_str() {
        "deepseek" => Some("deepseek"),
        "kimi" => Some("kimi"),
        "opencode" | "opencode-go" => Some("opencode"),
        // Some OpenCode integrations encode the selected backend in the
        // prefix (for example opencode-go-qwen/model).  It is still routed
        // traffic, but there is no reliable provider-specific price here.
        prefix if prefix.starts_with("opencode-go-") => Some("opencode"),
        _ => None,
    }
}

/// Remove a known route prefix before looking up provider-specific pricing.
pub fn strip_route_prefix(model: &str) -> &str {
    let trimmed = model.trim();
    trimmed
        .split_once('/')
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed)
}

/// Whether a model contributes to the native Codex/OpenAI subscription.
///
/// Unqualified model ids and the explicit `openai/` route are native. Any
/// other provider-qualified id is owned by that provider and must not inflate
/// Codex's local subscription usage.
pub fn counts_toward_codex_subscription(model: &str) -> bool {
    let trimmed = model.trim();
    if trimmed.eq_ignore_ascii_case("codex-auto-review") {
        return false;
    }
    let Some((prefix, _)) = trimmed.split_once('/') else {
        return true;
    };
    prefix.eq_ignore_ascii_case("openai")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_and_routed_models_are_classified_separately() {
        assert!(counts_toward_codex_subscription("gpt-5.6-sol"));
        assert!(counts_toward_codex_subscription("openai/gpt-5.6-sol"));
        assert!(!counts_toward_codex_subscription("deepseek/deepseek-chat"));
        assert!(!counts_toward_codex_subscription("opencode-go-qwen/qwen3.7-plus"));
        assert!(!counts_toward_codex_subscription("codex-auto-review"));
    }

    #[test]
    fn known_route_provider_is_not_guessed_for_unknown_prefix() {
        assert_eq!(codex_routed_provider("deepseek/deepseek-chat"), Some("deepseek"));
        assert_eq!(codex_routed_provider("opencode/foo"), Some("opencode"));
        assert_eq!(codex_routed_provider("vendor/model"), None);
        assert_eq!(strip_route_prefix("opencode/foo"), "foo");
    }
}
