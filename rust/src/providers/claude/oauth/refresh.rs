//! OAuth token refresh HTTP call.
//!
//! POSTs `grant_type=refresh_token` to the OAuth token endpoint, mirroring the
//! Claude CLI's own refresh call, and builds the new credentials.
//!
//! Error classification (upstream PR #309):
//! - Terminal: HTTP 400/401 with `error: "invalid_grant"` in the response body.
//!   The refresh token is permanently invalid; the user must re-authenticate.
//! - Transient: HTTP 400/401 without `invalid_grant`, or other HTTP/network
//!   errors. These are temporary and should be retried with backoff.

use chrono::Utc;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

use super::ClaudeOAuthCredentials;
use crate::core::ProviderError;

/// OAuth token endpoint + client id used to refresh an expired access token.
/// Mirrors the Claude CLI's own prod `TOKEN_URL` / `CLIENT_ID`.
const TOKEN_REFRESH_URL: &str = "https://platform.claude.com/v1/oauth/token";
const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";
/// Fallback access-token lifetime if a refresh response omits `expires_in`.
const DEFAULT_ACCESS_TTL_SECS: i64 = 3600;

/// Response from the OAuth token refresh endpoint (`grant_type=refresh_token`).
#[derive(Debug, Deserialize)]
struct RefreshTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

/// Error response from the OAuth token endpoint.
#[derive(Debug, Deserialize)]
struct OAuthErrorResponse {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    error_description: Option<String>,
}

/// Classify an OAuth refresh error response as terminal or transient.
///
/// Terminal: HTTP 400/401 with `error: "invalid_grant"` in the response body.
/// The refresh token is permanently invalid — the user must re-authenticate.
///
/// Transient: any other error (network timeout, 5xx, 400/401 without
/// `invalid_grant`, 429). These are temporary and should be retried.
pub(super) fn classify_refresh_error(
    status: reqwest::StatusCode,
    body: &str,
) -> ProviderError {
    let status_code = status.as_u16();
    let oauth_err: Option<OAuthErrorResponse> = serde_json::from_str(body).ok();

    let is_invalid_grant = oauth_err
        .as_ref()
        .and_then(|e| e.error.as_deref())
        .map(|e| e.eq_ignore_ascii_case("invalid_grant"))
        .unwrap_or(false);

    let body_preview = body.chars().take(200).collect::<String>();

    if is_invalid_grant {
        // Terminal: refresh token is permanently invalid
        ProviderError::OAuth(format!(
            "Token refresh failed (terminal): HTTP {status_code} invalid_grant. \
             Run `claude` to re-authenticate."
        ))
    } else if status_code == 400 || status_code == 401 {
        // Transient: 400/401 without invalid_grant (e.g., malformed request)
        let error_code = oauth_err
            .as_ref()
            .and_then(|e| e.error.as_deref())
            .unwrap_or("unknown");
        ProviderError::OAuth(format!(
            "Token refresh failed (transient): HTTP {status_code} ({error_code}): {body_preview}"
        ))
    } else if status_code == 429 {
        ProviderError::OAuth(format!(
            "Token refresh rate limited (transient): HTTP 429. Will retry after backoff."
        ))
    } else {
        // Other HTTP errors (5xx, etc.) — transient
        ProviderError::OAuth(format!(
            "Token refresh failed (transient): HTTP {status_code}: {body_preview}"
        ))
    }
}

/// POST `grant_type=refresh_token` to the OAuth token endpoint, mirroring the
/// Claude CLI's own refresh call, and build the new credentials.
pub(super) async fn refresh_access_token(
    client: &Client,
    refresh_token: &str,
    current: &ClaudeOAuthCredentials,
) -> Result<ClaudeOAuthCredentials, ProviderError> {
    let mut body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh_token,
        "client_id": OAUTH_CLIENT_ID,
    });
    if !current.scopes.is_empty() {
        body["scope"] = serde_json::Value::String(current.scopes.join(" "));
    }

    let response = client
        .post(TOKEN_REFRESH_URL)
        .header("anthropic-beta", OAUTH_BETA_HEADER)
        .header("Accept", "application/json")
        .json(&body)
        .timeout(Duration::from_secs(15))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(classify_refresh_error(status, &text));
    }

    let refreshed: RefreshTokenResponse = response
        .json()
        .await
        .map_err(|e| ProviderError::Parse(format!("Failed to parse refresh response: {e}")))?;

    let access_token = refreshed.access_token.trim().to_string();
    if access_token.is_empty() {
        return Err(ProviderError::OAuth(
            "Token refresh returned an empty access token".to_string(),
        ));
    }

    // The endpoint returns `expires_in`; if it is ever omitted, fall back to
    // a conservative TTL so the token is still treated as fresh for a bounded
    // window (and cached) instead of triggering a per-poll refresh storm.
    let ttl_secs = refreshed.expires_in.unwrap_or(DEFAULT_ACCESS_TTL_SECS);
    let expires_at = Some(Utc::now() + chrono::Duration::seconds(ttl_secs));

    let refresh_token = refreshed
        .refresh_token
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .or_else(|| current.refresh_token.clone());

    let scopes = refreshed
        .scope
        .map(|s| s.split_whitespace().map(str::to_string).collect::<Vec<_>>())
        .filter(|scopes| !scopes.is_empty())
        .unwrap_or_else(|| current.scopes.clone());

    Ok(ClaudeOAuthCredentials {
        access_token,
        refresh_token,
        expires_at,
        scopes,
        rate_limit_tier: current.rate_limit_tier.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::classify_refresh_error;
    use super::RefreshTokenResponse;
    use reqwest::StatusCode;

    #[test]
    fn parses_refresh_token_response() {
        let resp: RefreshTokenResponse = serde_json::from_str(
            r#"{
                "access_token": "new-access",
                "refresh_token": "new-refresh",
                "expires_in": 28800,
                "scope": "user:inference user:profile",
                "token_type": "Bearer"
            }"#,
        )
        .expect("refresh response should parse");

        assert_eq!(resp.access_token, "new-access");
        assert_eq!(resp.refresh_token.as_deref(), Some("new-refresh"));
        assert_eq!(resp.expires_in, Some(28800));
        assert_eq!(resp.scope.as_deref(), Some("user:inference user:profile"));
    }

    // --- Error classification fixtures ---

    #[test]
    fn classifies_invalid_grant_as_terminal() {
        let err = classify_refresh_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_grant","error_description":"The refresh token is invalid."}"#,
        );
        let msg = err.to_string();
        assert!(msg.contains("terminal"), "terminal error: {msg}");
        assert!(msg.contains("invalid_grant"), "invalid_grant: {msg}");
        assert!(msg.contains("re-authenticate"), "re-auth hint: {msg}");
    }

    #[test]
    fn classifies_401_invalid_grant_as_terminal() {
        let err = classify_refresh_error(
            StatusCode::UNAUTHORIZED,
            r#"{"error":"invalid_grant","error_description":"Token expired."}"#,
        );
        let msg = err.to_string();
        assert!(msg.contains("terminal"), "terminal error: {msg}");
        assert!(msg.contains("invalid_grant"), "invalid_grant: {msg}");
    }

    #[test]
    fn classifies_400_without_invalid_grant_as_transient() {
        let err = classify_refresh_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":"invalid_request","error_description":"Missing parameter."}"#,
        );
        let msg = err.to_string();
        assert!(msg.contains("transient"), "transient error: {msg}");
        assert!(!msg.contains("invalid_grant"), "no invalid_grant: {msg}");
    }

    #[test]
    fn classifies_401_without_invalid_grant_as_transient() {
        let err = classify_refresh_error(
            StatusCode::UNAUTHORIZED,
            r#"{"error":"invalid_token","error_description":"Token expired."}"#,
        );
        let msg = err.to_string();
        assert!(msg.contains("transient"), "transient error: {msg}");
    }

    #[test]
    fn classifies_empty_body_as_transient() {
        let err = classify_refresh_error(StatusCode::BAD_GATEWAY, "");
        let msg = err.to_string();
        assert!(msg.contains("transient"), "transient error: {msg}");
    }

    #[test]
    fn classifies_rate_limit_as_transient() {
        let err = classify_refresh_error(StatusCode::TOO_MANY_REQUESTS, "{}");
        let msg = err.to_string();
        assert!(msg.contains("transient"), "transient error: {msg}");
        assert!(msg.contains("rate limited"), "rate limited: {msg}");
    }

    #[test]
    fn classifies_server_error_as_transient() {
        let err = classify_refresh_error(StatusCode::INTERNAL_SERVER_ERROR, "{}");
        let msg = err.to_string();
        assert!(msg.contains("transient"), "transient error: {msg}");
    }

    #[test]
    fn classifies_invalid_grant_case_insensitive() {
        let err = classify_refresh_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":"Invalid_Grant"}"#,
        );
        let msg = err.to_string();
        assert!(msg.contains("terminal"), "terminal error: {msg}");
    }

    #[test]
    fn classifies_upstream_format_change_without_error_field() {
        // If the upstream changes the error format, we must not crash
        let err = classify_refresh_error(
            StatusCode::BAD_REQUEST,
            r#"{"errorCode":"invalid_grant","message":"The refresh token is invalid."}"#,
        );
        let msg = err.to_string();
        // Without an "error" field, this is treated as transient (not terminal)
        assert!(msg.contains("transient"), "transient error: {msg}");
    }

    #[test]
    fn classifies_null_body_as_transient() {
        let err = classify_refresh_error(StatusCode::BAD_REQUEST, "null");
        let msg = err.to_string();
        assert!(msg.contains("transient"), "transient error: {msg}");
    }

    #[test]
    fn classifies_non_json_body_as_transient() {
        let err = classify_refresh_error(StatusCode::SERVICE_UNAVAILABLE, "Service Unavailable");
        let msg = err.to_string();
        assert!(msg.contains("transient"), "transient error: {msg}");
    }
}
