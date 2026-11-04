use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::core::ProviderId;

const ZED_DEFAULT_URL: &str = "https://cloud.zed.dev/client/users/me";

pub fn validate_provider_workspace_value(
    provider_id: ProviderId,
    raw: &str,
) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    match provider_id {
        ProviderId::OpenAIApi => validate_id(trimmed, "OpenAI project ID", |value| {
            value.starts_with("proj_") && has_safe_id_chars(value)
        }),
        ProviderId::OpenCodeGo => validate_opencode_workspace(trimmed, "OpenCode Go workspace ID"),
        ProviderId::OpenCode => validate_opencode_workspace(trimmed, "OpenCode workspace ID"),
        ProviderId::Devin => validate_id(trimmed, "Devin organization", |value| {
            let Some((prefix, org)) = value.split_once('/') else {
                return false;
            };
            prefix == "org"
                && (2..=80).contains(&org.len())
                && org
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        }),
        ProviderId::Zed => validate_zed_url(trimmed),
        ProviderId::LiteLLM => validate_token_endpoint(trimmed, "LiteLLM base URL", |_| true),
        _ => Ok(trimmed.to_string()),
    }
}

fn validate_id(
    value: &str,
    label: &str,
    is_valid: impl FnOnce(&str) -> bool,
) -> Result<String, String> {
    if value.chars().any(|c| c.is_control() || c.is_whitespace()) || !is_valid(value) {
        return Err(format!("{label} is invalid"));
    }
    Ok(value.to_string())
}

fn has_safe_id_chars(value: &str) -> bool {
    (6..=128).contains(&value.len())
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// OpenCode and OpenCode Go share one workspace shape: a bare `wrk_`/`org_` id
/// (Zen workspaces are `wrk_…`, personal orgs `org_…`) or the console URL users
/// copy from the address bar. Both console normalizers expect the bare id, so
/// settings store what they expect.
fn validate_opencode_workspace(raw: &str, label: &str) -> Result<String, String> {
    if raw.chars().any(char::is_control) {
        return Err(format!("{label} is invalid"));
    }
    normalize_opencode_workspace(raw).ok_or_else(|| format!("{label} is invalid"))
}

fn normalize_opencode_workspace(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        return opencode_workspace_id_from_url(trimmed);
    }
    is_opencode_workspace_id(trimmed).then(|| trimmed.to_string())
}

fn opencode_workspace_id_from_url(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "opencode.ai" && host != "www.opencode.ai" {
        return None;
    }
    let segments: Vec<&str> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();
    for pair in segments.windows(2) {
        if matches!(pair[0], "console" | "workspace") && is_opencode_workspace_id(pair[1]) {
            return Some(pair[1].to_string());
        }
    }
    None
}

fn is_opencode_workspace_id(value: &str) -> bool {
    (value.starts_with("wrk_") || value.starts_with("org_")) && has_safe_id_chars(value)
}

fn validate_zed_url(raw: &str) -> Result<String, String> {
    let url = validate_token_endpoint(raw, "Zed API URL", |host| host == "cloud.zed.dev")?;
    if url != ZED_DEFAULT_URL {
        return Err(
            "Zed API URL must be the default https://cloud.zed.dev/client/users/me endpoint"
                .to_string(),
        );
    }
    Ok(url)
}

fn validate_token_endpoint(
    raw: &str,
    label: &str,
    host_allowed: impl FnOnce(&str) -> bool,
) -> Result<String, String> {
    let lower = raw.to_ascii_lowercase();
    if raw.chars().any(|c| c.is_control())
        || ["%2f", "%5c", "%3f", "%23", "%40", "%3a"]
            .iter()
            .any(|encoded| lower.contains(encoded))
    {
        return Err(format!(
            "{label} must not contain control characters or encoded host delimiters"
        ));
    }

    let url = url::Url::parse(raw).map_err(|e| format!("Invalid {label}: {e}"))?;
    let host = url
        .host_str()
        .ok_or_else(|| format!("{label} must include a host"))?
        .to_ascii_lowercase();

    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || host.contains('%')
        || host.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err(format!(
            "{label} must use HTTPS without user info or encoded host tricks"
        ));
    }
    if is_blocked_host(&host) || !host_allowed(&host) {
        return Err(format!("{label} host is not allowed"));
    }

    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn is_blocked_host(host: &str) -> bool {
    let normalized = host.trim_end_matches('.').to_ascii_lowercase();
    if normalized == "localhost" || normalized.ends_with(".localhost") {
        return true;
    }
    let ip_candidate = normalized
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(&normalized);
    ip_candidate.parse::<IpAddr>().is_ok_and(|ip| match ip {
        IpAddr::V4(ip) => is_blocked_ipv4(ip),
        IpAddr::V6(ip) => is_blocked_ipv6(ip),
    })
}

fn is_blocked_ipv4(ip: Ipv4Addr) -> bool {
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.octets()[0] == 0
        || ip.octets()[0] >= 224
}

fn is_blocked_ipv6(ip: Ipv6Addr) -> bool {
    ip.is_loopback()
        || ip.is_unspecified()
        || ((ip.segments()[0] & 0xfe00) == 0xfc00)
        || ((ip.segments()[0] & 0xffc0) == 0xfe80)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_workspace_ids_by_provider() {
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenAIApi, " proj_abc-123 ").unwrap(),
            "proj_abc-123"
        );
        assert_eq!(
            validate_provider_workspace_value(ProviderId::Devin, "org/acme_123").unwrap(),
            "org/acme_123"
        );
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenCodeGo, " wrk_abc-123 ").unwrap(),
            "wrk_abc-123"
        );
        assert!(
            validate_provider_workspace_value(ProviderId::OpenAIApi, "https://evil.test").is_err()
        );
        assert!(
            validate_provider_workspace_value(ProviderId::Devin, "https://api.devin.ai").is_err()
        );
        assert!(validate_provider_workspace_value(ProviderId::OpenCodeGo, "wrk_abc/123").is_err());
    }

    #[test]
    fn validates_opencode_workspace_ids() {
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenCode, " wrk_abc-123 ").unwrap(),
            "wrk_abc-123"
        );
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenCode, "org_team_1").unwrap(),
            "org_team_1"
        );
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenCode, "   ").unwrap(),
            ""
        );
        // Settings now accept the address-bar URL and store the bare id, the
        // same shape both console normalizers produce.
        assert_eq!(
            validate_provider_workspace_value(
                ProviderId::OpenCode,
                "https://opencode.ai/console/wrk_abc-123/billing"
            )
            .unwrap(),
            "wrk_abc-123"
        );
        assert_eq!(
            validate_provider_workspace_value(
                ProviderId::OpenCode,
                "https://www.opencode.ai/workspace/org_team_1/go"
            )
            .unwrap(),
            "org_team_1"
        );
        for value in ["wrk_abc/123", "abc123", "wrk_"] {
            assert!(
                validate_provider_workspace_value(ProviderId::OpenCode, value).is_err(),
                "accepted {value}"
            );
        }
    }

    #[test]
    fn validates_opencode_go_workspace_ids_like_opencode() {
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenCodeGo, " wrk_abc-123 ").unwrap(),
            "wrk_abc-123"
        );
        // Go shares the console normalizer, so `org_` scopes and pasted URLs
        // must save instead of being rejected by a narrower `wrk_`-only rule.
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenCodeGo, "org_team_1").unwrap(),
            "org_team_1"
        );
        assert_eq!(
            validate_provider_workspace_value(
                ProviderId::OpenCodeGo,
                "https://opencode.ai/console/wrk_abc123/go"
            )
            .unwrap(),
            "wrk_abc123"
        );
        assert_eq!(
            validate_provider_workspace_value(ProviderId::OpenCodeGo, "  ").unwrap(),
            ""
        );
    }

    #[test]
    fn rejects_unsafe_opencode_workspace_inputs() {
        let long_id = format!("wrk_{}", "a".repeat(200));
        let cases = [
            "http://opencode.ai/console/wrk_abc123/billing".to_string(),
            "https://evil.test/console/wrk_abc123".to_string(),
            "https://opencode.ai.evil.test/console/wrk_abc123".to_string(),
            "https://opencode.ai/console/abc123".to_string(),
            "https://opencode.ai/console/wrk_abc123%20x/go".to_string(),
            "https://user@opencode.ai/console/wrk_abc123/go".to_string(),
            "https://opencode.ai/other/wrk_abc123/go".to_string(),
            "wrk_abc 123".to_string(),
            "wrk_abc\u{0}123".to_string(),
            long_id,
        ];
        for provider in [ProviderId::OpenCode, ProviderId::OpenCodeGo] {
            for value in &cases {
                assert!(
                    validate_provider_workspace_value(provider, value).is_err(),
                    "{provider:?} accepted {value:?}"
                );
            }
        }
    }

    #[test]
    fn validates_token_endpoint_hosts() {
        assert_eq!(
            validate_provider_workspace_value(
                ProviderId::LiteLLM,
                "https://litellm.example.com/v1"
            )
            .unwrap(),
            "https://litellm.example.com/v1"
        );
        for value in [
            "http://litellm.example.com",
            "https://user@litellm.example.com",
            "https://127.0.0.1",
            "https://10.0.0.5",
            "https://[::1]",
            "https://example.com%2f.evil.test",
        ] {
            assert!(
                validate_provider_workspace_value(ProviderId::LiteLLM, value).is_err(),
                "accepted {value}"
            );
        }
    }

    #[test]
    fn zed_allows_only_default_cloud_endpoint() {
        assert_eq!(
            validate_provider_workspace_value(
                ProviderId::Zed,
                "https://cloud.zed.dev/client/users/me"
            )
            .unwrap(),
            "https://cloud.zed.dev/client/users/me"
        );
        assert!(
            validate_provider_workspace_value(ProviderId::Zed, "https://evil.example/users/me")
                .is_err()
        );
        assert!(
            validate_provider_workspace_value(
                ProviderId::Zed,
                "https://cloud.zed.dev.evil/users/me"
            )
            .is_err()
        );
    }
}
