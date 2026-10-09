//! Grok provider implementation.
//!
//! Uses Grok CLI's bearer-authenticated billing endpoint when a local
//! `~/.grok/auth.json` login is available, with grok.com gRPC-web as a
//! compatibility fallback.

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

const BILLING_ENDPOINT: &str = "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";
const CLI_BILLING_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
const CLI_SETTINGS_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/settings";
const CLI_USER_ENDPOINT: &str = "https://cli-chat-proxy.grok.com/v1/user?include=subscription";
const CLI_SUBSCRIPTIONS_ENDPOINT: &str = "https://grok.com/rest/subscriptions";
const FALLBACK_GROK_CLI_VERSION: &str = "0.2.93";
const GRPC_BILLING_REQUEST_BODY: &[u8] = &[0, 0, 0, 0, 2, 0x08, 0x00];

pub struct GrokProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl GrokProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::Grok,
                display_name: "Grok",
                // The CLI-proxy can report monthly or unknown billing cycles;
                // the desktop bridge derives a cycle label from each result.
                // Keep metadata neutral so unknown cycles are never guessed.
                session_label: "Credits",
                weekly_label: "On-demand",
                supports_opus: false,
                supports_credits: false,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://grok.com/?_s=usage"),
                status_page_url: Some("https://status.x.ai"),
            },
            client: crate::core::credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    fn auth_file_path() -> Option<PathBuf> {
        if let Ok(home) = std::env::var("GROK_HOME")
            && !home.trim().is_empty()
        {
            return Some(PathBuf::from(home).join("auth.json"));
        }
        dirs::home_dir().map(|home| home.join(".grok").join("auth.json"))
    }

    fn load_credentials(kind: GrokAuthKind) -> Result<GrokCredentials, ProviderError> {
        let path = Self::auth_file_path()
            .ok_or_else(|| ProviderError::NotInstalled("Grok auth path not found".to_string()))?;
        let text = std::fs::read_to_string(&path).map_err(|_| {
            ProviderError::NotInstalled("Grok auth.json not found. Run `grok login`.".to_string())
        })?;
        GrokCredentials::parse_for_kind(&text, kind)
    }

    async fn fetch_with_auth(
        &self,
        credentials: &GrokCredentials,
        kind: GrokAuthKind,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let cli_version = Self::detect_cli_version_async()
            .await
            .unwrap_or_else(|| FALLBACK_GROK_CLI_VERSION.to_string());
        let (acp_billing, plan, cli_billing) = tokio::join!(
            self.fetch_cli_acp_billing(kind),
            self.fetch_cli_subscription_tier(credentials, &cli_version),
            self.fetch_cli_billing(credentials, &cli_version)
        );
        let mut billing_plan = plan;
        let mut preferred_billing = None;
        if let Some(value) = acp_billing {
            if let Some((snapshot, tier)) = parse_current_grok_acp_billing(&value, Utc::now()) {
                preferred_billing = Some(snapshot);
                if billing_plan.is_none() {
                    billing_plan = tier;
                }
            }
        }
        if let Some(billing) = preferred_billing.filter(|billing| billing.usage_known) {
            let billing = match cli_billing {
                Ok((proxy, tier)) => {
                    billing_plan = billing_plan.or(tier);
                    merge_acp_usage_with_cli_proxy_metadata(billing, proxy)
                }
                Err(error) => {
                    tracing::debug!(
                        "Grok CLI-proxy billing unavailable; keeping current ACP usage: {error}"
                    );
                    billing
                }
            };
            return Ok(result_from_billing(
                billing,
                if kind == GrokAuthKind::Cli {
                    "grok-cli-acp"
                } else {
                    "grok-oauth"
                },
                credentials.email.clone(),
                credentials.team_id.clone(),
                billing_plan,
            ));
        }

        let billing = match cli_billing {
            Ok((billing, tier)) if billing.usage_known => {
                billing_plan = billing_plan.or(tier);
                merge_cli_proxy_with_acp_fallback(billing, preferred_billing)
            }
            Ok((billing, tier)) => {
                billing_plan = billing_plan.or(tier);
                let preferred =
                    merge_cli_proxy_with_acp_fallback(billing, preferred_billing.take());
                match self
                    .fetch_billing(Some(format!("Bearer {}", credentials.access_token)), None)
                    .await
                {
                    Ok(fallback) => merge_billing_usage_fallback(preferred, fallback),
                    Err(error) => {
                        tracing::debug!(
                            "Grok bearer gRPC usage fallback unavailable; keeping CLI-proxy period: {error}"
                        );
                        preferred
                    }
                }
            }
            Err(cli_error) => {
                tracing::debug!(
                    "Grok CLI-proxy billing unavailable; trying bearer gRPC compatibility path: {cli_error}"
                );
                match self
                    .fetch_billing(Some(format!("Bearer {}", credentials.access_token)), None)
                    .await
                {
                    Ok(fallback) => preferred_billing
                        .take()
                        .map(|preferred| merge_billing_usage_fallback(preferred, fallback))
                        .unwrap_or(fallback),
                    Err(fallback_error) => {
                        if let Some(preferred) = preferred_billing {
                            preferred
                        } else if matches!(cli_error, ProviderError::AuthRequired)
                            && matches!(fallback_error, ProviderError::AuthRequired)
                        {
                            return Err(ProviderError::AuthRequired);
                        } else {
                            return Err(ProviderError::Other(format!(
                                "Grok billing sources failed (CLI proxy: {cli_error}; bearer fallback: {fallback_error})"
                            )));
                        }
                    }
                }
            }
        };
        Ok(result_from_billing(
            billing,
            if kind == GrokAuthKind::Cli {
                "grok-cli"
            } else {
                "grok-oauth"
            },
            credentials.email.clone(),
            credentials.team_id.clone(),
            billing_plan,
        ))
    }

    /// The Grok CLI exposes the same billing data over its ACP JSON-RPC
    /// surface. Use it only for the CLI principal: a token-account/OAuth
    /// bearer can represent a different identity than the local CLI process.
    async fn fetch_cli_acp_billing(&self, kind: GrokAuthKind) -> Option<Value> {
        if kind != GrokAuthKind::Cli {
            return None;
        }
        match fetch_grok_acp_billing().await {
            Ok(value) => Some(value),
            Err(_) => {
                tracing::debug!("Grok CLI ACP billing unavailable; using bearer HTTP fallback");
                None
            }
        }
    }

    /// The CLI settings endpoint is the preferred exact tier source. The user
    /// endpoint carries a subscription enum on accounts where settings omits it.
    async fn fetch_cli_subscription_tier(
        &self,
        credentials: &GrokCredentials,
        cli_version: &str,
    ) -> Option<String> {
        if let Ok(settings) = self
            .fetch_cli_json(credentials, CLI_SETTINGS_ENDPOINT, cli_version)
            .await
            && let Some(tier) = grok_tier_from_settings(&settings)
        {
            return Some(tier);
        }
        if let Ok(user) = self
            .fetch_cli_json(credentials, CLI_USER_ENDPOINT, cli_version)
            .await
            && let Some(tier) = grok_tier_from_user(&user)
        {
            return Some(tier);
        }
        let subscriptions = self
            .fetch_cli_json(credentials, CLI_SUBSCRIPTIONS_ENDPOINT, cli_version)
            .await
            .ok()?;
        grok_tier_from_subscriptions(&subscriptions)
    }

    async fn fetch_cli_billing(
        &self,
        credentials: &GrokCredentials,
        cli_version: &str,
    ) -> Result<(GrokBillingSnapshot, Option<String>), ProviderError> {
        let value = self
            .fetch_cli_json(credentials, CLI_BILLING_ENDPOINT, cli_version)
            .await?;
        parse_current_grok_billing(&value, Utc::now()).ok_or_else(|| {
            ProviderError::Other("Grok CLI-proxy billing response was not current".to_string())
        })
    }

    async fn fetch_cli_json(
        &self,
        credentials: &GrokCredentials,
        endpoint: &str,
        cli_version: &str,
    ) -> Result<Value, ProviderError> {
        let mut request = self
            .client
            .get(endpoint)
            .timeout(std::time::Duration::from_secs(2))
            .header(
                "Authorization",
                format!("Bearer {}", credentials.access_token),
            )
            .header("x-xai-token-auth", "xai-grok-cli")
            .header("x-grok-cli-version", cli_version)
            .header("x-grok-client-version", cli_version)
            .header("x-grok-client-surface", "grok-cli")
            .header("Accept", "application/json")
            .header("User-Agent", format!("grok-cli/{cli_version}"));
        if endpoint == CLI_SUBSCRIPTIONS_ENDPOINT
            && let Some(user_id) = credentials.user_id.as_deref()
        {
            request = request.header("x-userid", user_id);
        }
        let response = request.send().await?;
        let status = response.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(ProviderError::AuthRequired);
        }
        if !status.is_success() {
            return Err(ProviderError::Other(format!(
                "Grok CLI endpoint returned status {status}"
            )));
        }
        response.json().await.map_err(|_| {
            ProviderError::Parse("Grok CLI endpoint returned invalid JSON".to_string())
        })
    }

    async fn fetch_with_cookie(
        &self,
        cookie_header: &str,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let billing = self
            .fetch_billing(None, Some(cookie_header.to_string()))
            .await
            .map_err(|error| match error {
                ProviderError::AuthRequired => ProviderError::Other(
                    "Grok rejected browser-cookie billing authentication. Sign in with `grok login` to use the CLI account session.".to_string(),
                ),
                other => other,
            })?;
        Ok(result_from_billing(
            billing,
            "grok-browser",
            None,
            None,
            // Cookie auth can't read the tier; still surface a name in the bar.
            Some("Grok".to_string()),
        ))
    }

    async fn fetch_billing(
        &self,
        authorization: Option<String>,
        cookie_header: Option<String>,
    ) -> Result<GrokBillingSnapshot, ProviderError> {
        let mut request = self
            .client
            .post(BILLING_ENDPOINT)
            .body(GRPC_BILLING_REQUEST_BODY)
            .header("Origin", "https://grok.com")
            .header("Referer", "https://grok.com/?_s=usage")
            .header("Accept", "*/*")
            .header("Content-Type", "application/grpc-web+proto")
            .header("x-grpc-web", "1")
            .header("x-user-agent", "connect-es/2.1.1")
            .header("User-Agent", "CodexBar");
        if let Some(auth) = authorization {
            request = request.header("Authorization", auth);
        }
        if let Some(cookie) = cookie_header {
            request = request.header("Cookie", cookie);
        }

        let response = request.send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.bytes().await?;
        if !status.is_success() {
            if status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN
            {
                return Err(ProviderError::AuthRequired);
            }
            return Err(ProviderError::Other(format!(
                "Grok web billing returned status {status}"
            )));
        }
        validate_grpc_response_status(&headers, &bytes)?;
        let billing = parse_grpc_web_response(&bytes)?;
        current_grok_billing_snapshot(billing, Utc::now()).ok_or_else(|| {
            ProviderError::Other("Grok gRPC billing response was not current".to_string())
        })
    }

    async fn detect_cli_version_async() -> Option<String> {
        let mut command = tokio::process::Command::new("grok");
        command
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.as_std_mut().creation_flags(0x08000000);

        let output = tokio::time::timeout(std::time::Duration::from_secs(2), command.output())
            .await
            .ok()?
            .ok()?;
        output.status.success().then_some(())?;
        parse_grok_cli_version(&output.stdout)
    }
}

fn parse_grok_cli_version(stdout: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(stdout);
    let first_line = text.lines().next()?.trim();
    let version = first_line
        .strip_prefix("grok ")
        .unwrap_or(first_line)
        .trim();
    (!version.is_empty()).then(|| version.to_string())
}

async fn fetch_grok_acp_billing() -> Result<Value, ProviderError> {
    use tokio::process::Command;

    let mut command = Command::new("grok");
    command
        .args(["agent", "stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.as_std_mut().creation_flags(0x08000000);

    let mut child = command.spawn().map_err(|_| {
        ProviderError::NotInstalled("Grok CLI ACP process could not be started".to_string())
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| ProviderError::Other("Grok CLI ACP stdin was unavailable".to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ProviderError::Other("Grok CLI ACP stdout was unavailable".to_string()))?;
    let mut lines = BufReader::new(stdout).lines();

    let exchange = async {
        write_grok_acp_request(
            &mut stdin,
            1,
            "initialize",
            serde_json::json!({
                "protocolVersion": "1",
                "clientCapabilities": {
                    "fs": { "readTextFile": false, "writeTextFile": false },
                    "terminal": false
                }
            }),
        )
        .await?;
        let _ = read_grok_acp_result(&mut lines, 1).await?;

        write_grok_acp_request(&mut stdin, 2, "x.ai/billing", serde_json::json!({})).await?;
        read_grok_acp_result(&mut lines, 2).await
    };

    let result = match tokio::time::timeout(std::time::Duration::from_secs(7), exchange).await {
        Ok(result) => result,
        Err(_) => Err(ProviderError::Timeout),
    };
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

async fn write_grok_acp_request<W>(
    writer: &mut W,
    id: u64,
    method: &str,
    params: Value,
) -> Result<(), ProviderError>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut line = grok_acp_request_line(id, method, params)?;
    line.push(b'\n');
    writer
        .write_all(&line)
        .await
        .map_err(|_| ProviderError::Other("Grok CLI ACP stdin closed".to_string()))?;
    writer
        .flush()
        .await
        .map_err(|_| ProviderError::Other("Grok CLI ACP stdin closed".to_string()))
}

fn grok_acp_request_line(id: u64, method: &str, params: Value) -> Result<Vec<u8>, ProviderError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    });
    serde_json::to_vec(&request)
        .map_err(|_| ProviderError::Parse("Could not encode Grok CLI ACP request".to_string()))
}

async fn read_grok_acp_result<R>(
    reader: &mut tokio::io::Lines<R>,
    expected_id: u64,
) -> Result<Value, ProviderError>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    loop {
        let line = reader
            .next_line()
            .await
            .map_err(|_| ProviderError::Other("Grok CLI ACP stdout could not be read".to_string()))?
            .ok_or_else(|| {
                ProviderError::Other("Grok CLI ACP closed before replying".to_string())
            })?;
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            return Err(ProviderError::Parse(
                "Grok CLI ACP returned a non-JSON-RPC line".to_string(),
            ));
        };
        match grok_acp_response_result(message, expected_id)? {
            Some(result) => return Ok(result),
            None => continue,
        }
    }
}

fn grok_acp_response_result(
    message: Value,
    expected_id: u64,
) -> Result<Option<Value>, ProviderError> {
    if message.get("id").and_then(Value::as_u64) != Some(expected_id) {
        return Ok(None);
    }
    if let Some(error) = message.get("error") {
        let code = error.get("code").and_then(Value::as_i64);
        let description = if code == Some(-32601) {
            "Grok CLI ACP billing method is unavailable"
        } else {
            "Grok CLI ACP billing request failed"
        };
        return Err(ProviderError::Other(description.to_string()));
    }
    message
        .get("result")
        .cloned()
        .map(Some)
        .ok_or_else(|| ProviderError::Parse("Grok CLI ACP response omitted result".to_string()))
}

impl Default for GrokProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for GrokProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Grok
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        match ctx.source_mode {
            SourceMode::Auto => {
                if ctx.auto_prefer_web {
                    if let Some(cookie_header) = &ctx.manual_cookie_header {
                        // A selected Grok token-account cookie pins the account.
                        // Do not silently replace it with machine-wide auth.json.
                        return self.fetch_with_cookie(cookie_header).await;
                    }
                    if ctx.api_key.is_none() {
                        return Err(ProviderError::NoCookies);
                    }
                }
                if let Some(token) = ctx.api_key.as_deref() {
                    let credentials = GrokCredentials::from_bearer(token);
                    match self
                        .fetch_with_auth(&credentials, GrokAuthKind::OAuth)
                        .await
                    {
                        Ok(result) => return Ok(result),
                        Err(error) => tracing::debug!(
                            "Grok configured bearer path failed in Auto; continuing the source ladder: {error}"
                        ),
                    }
                }
                for kind in [GrokAuthKind::Cli, GrokAuthKind::OAuth] {
                    if let Ok(credentials) = Self::load_credentials(kind) {
                        match self.fetch_with_auth(&credentials, kind).await {
                            Ok(result) => return Ok(result),
                            Err(ProviderError::AuthRequired) => {}
                            Err(error) => {
                                tracing::debug!("Grok login path failed in Auto: {error}")
                            }
                        }
                    }
                }
                if let Some(cookie_header) = &ctx.manual_cookie_header {
                    return self.fetch_with_cookie(cookie_header).await;
                }
                if ctx.allow_browser_cookie {
                    match crate::providers::browser_cookie_header(&["grok.com"]) {
                        Ok(cookie_header) => self.fetch_with_cookie(&cookie_header).await,
                        Err(e) => Err(e),
                    }
                } else {
                    Err(ProviderError::NoCookies)
                }
            }
            SourceMode::Web => {
                if let Some(cookie_header) = &ctx.manual_cookie_header {
                    return self.fetch_with_cookie(cookie_header).await;
                }
                if ctx.allow_browser_cookie {
                    match crate::providers::browser_cookie_header(&["grok.com"]) {
                        Ok(cookie_header) => self.fetch_with_cookie(&cookie_header).await,
                        Err(e) => Err(e),
                    }
                } else {
                    Err(ProviderError::NoCookies)
                }
            }
            SourceMode::Cli => {
                let credentials = Self::load_credentials(GrokAuthKind::Cli)?;
                self.fetch_with_auth(&credentials, GrokAuthKind::Cli).await
            }
            SourceMode::OAuth => {
                let credentials = if let Some(token) = ctx.api_key.as_deref() {
                    GrokCredentials::from_bearer(token)
                } else {
                    Self::load_credentials(GrokAuthKind::OAuth)?
                };
                self.fetch_with_auth(&credentials, GrokAuthKind::OAuth)
                    .await
            }
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![
            SourceMode::Auto,
            SourceMode::Cli,
            SourceMode::OAuth,
            SourceMode::Web,
        ]
    }

    fn supports_web(&self) -> bool {
        true
    }

    fn detect_version(&self) -> Option<String> {
        // CLI version probing is async and bounded inside billing fetches. This
        // synchronous capability hook must not spawn a potentially stalled CLI.
        None
    }
}

/// Which principal inside auth.json a credential belongs to: the Grok CLI
/// login or an OAuth session. Pinned fetches only use their own principal;
/// Auto tries both. This auth mode does not establish a subscription tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrokAuthKind {
    Cli,
    OAuth,
}

#[derive(Debug, Clone)]
struct GrokCredentials {
    access_token: String,
    email: Option<String>,
    team_id: Option<String>,
    user_id: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

impl GrokCredentials {
    fn from_bearer(token: &str) -> Self {
        Self {
            access_token: token.trim().to_string(),
            email: None,
            team_id: None,
            user_id: None,
            expires_at: None,
        }
    }

    fn parse_for_kind(text: &str, kind: GrokAuthKind) -> Result<Self, ProviderError> {
        let root: Value = serde_json::from_str(text)
            .map_err(|e| ProviderError::Parse(format!("Failed to decode Grok auth.json: {e}")))?;
        let map = root
            .as_object()
            .ok_or_else(|| ProviderError::Parse("Invalid Grok auth.json".to_string()))?;
        let now = Utc::now();
        let selected = map.iter().find(|(scope, entry)| {
            let has_key = entry
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty());
            if !has_key || grok_auth_entry_is_expired(entry, now) {
                return false;
            }
            let is_oauth = scope.starts_with("https://auth.x.ai::")
                || entry
                    .get("auth_mode")
                    .and_then(Value::as_str)
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("oidc"));
            match kind {
                GrokAuthKind::Cli => !is_oauth,
                GrokAuthKind::OAuth => is_oauth,
            }
        });
        let (_, entry) = selected.ok_or(ProviderError::AuthRequired)?;
        let access_token = entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or(ProviderError::AuthRequired)?
            .to_string();
        let expires_at = entry
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|dt| dt.with_timezone(&Utc));
        if expires_at.is_some_and(|dt| dt <= Utc::now()) {
            return Err(ProviderError::AuthRequired);
        }
        Ok(Self {
            access_token,
            email: text_field(entry, "email"),
            team_id: text_field(entry, "team_id"),
            user_id: text_field(entry, "user_id"),
            expires_at,
        })
    }

    fn parse(text: &str) -> Result<Self, ProviderError> {
        let root: Value = serde_json::from_str(text)
            .map_err(|e| ProviderError::Parse(format!("Failed to decode Grok auth.json: {e}")))?;
        let map = root
            .as_object()
            .ok_or_else(|| ProviderError::Parse("Invalid Grok auth.json".to_string()))?;
        let now = Utc::now();
        let mut selected: Option<(&String, &Value)> = None;
        for (scope, entry) in map {
            if entry
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
                && !grok_auth_entry_is_expired(entry, now)
                && (scope.starts_with("https://auth.x.ai::")
                    || selected.is_none()
                    || scope.contains("/sign-in"))
            {
                selected = Some((scope, entry));
                if scope.starts_with("https://auth.x.ai::") {
                    break;
                }
            }
        }
        let (_, entry) = selected.ok_or(ProviderError::AuthRequired)?;
        let access_token = entry
            .get("key")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or(ProviderError::AuthRequired)?
            .to_string();
        let expires_at = entry
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
            .map(|dt| dt.with_timezone(&Utc));
        if expires_at.is_some_and(|dt| dt <= Utc::now()) {
            return Err(ProviderError::AuthRequired);
        }
        Ok(Self {
            access_token,
            email: text_field(entry, "email"),
            team_id: text_field(entry, "team_id"),
            user_id: text_field(entry, "user_id"),
            expires_at,
        })
    }
}

fn text_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

fn grok_auth_entry_is_expired(entry: &Value, now: DateTime<Utc>) -> bool {
    entry
        .get("expires_at")
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .is_some_and(|expires_at| expires_at.with_timezone(&Utc) <= now)
}

fn grok_plan_display_name(raw: Option<&str>) -> Option<String> {
    let trimmed = raw?.trim();
    if trimmed.is_empty() {
        return None;
    }
    let compact: String = trimmed
        .to_ascii_lowercase()
        .chars()
        .filter(|ch| ch.is_ascii_alphabetic())
        .collect();
    if compact.ends_with("supergrokheavy") || compact == "heavy" {
        Some("SuperGrok Heavy".to_string())
    } else if compact.ends_with("supergrok") {
        Some("SuperGrok".to_string())
    } else {
        Some(trimmed.to_string())
    }
}

fn grok_user_subscription_tier(subscription: &Value) -> Option<String> {
    grok_plan_display_name(
        text_field(subscription, "tier")
            .or_else(|| text_field(subscription, "subscriptionTier"))
            .or_else(|| text_field(subscription, "subscription_tier"))
            .or_else(|| text_field(subscription, "plan"))
            .as_deref(),
    )
}

fn grok_tier_from_settings(value: &Value) -> Option<String> {
    let settings = value.get("settings").unwrap_or(value);
    grok_plan_display_name(
        text_field(settings, "subscription_tier_display")
            .or_else(|| text_field(settings, "subscriptionTierDisplay"))
            .or_else(|| text_field(settings, "subscription_tier"))
            .or_else(|| text_field(settings, "subscriptionTier"))
            .as_deref(),
    )
}

fn grok_tier_from_billing(value: &Value) -> Option<String> {
    let config = value.get("config").unwrap_or(value);
    let direct_subscription_tier = config
        .get("subscription")
        .filter(|item| item.is_object())
        .filter(|item| {
            text_field(item, "status").is_none_or(|status| is_active_subscription_status(&status))
        })
        .and_then(grok_user_subscription_tier);
    if direct_subscription_tier.is_some() {
        return direct_subscription_tier;
    }
    if let Some(tier) = active_grok_subscription(config).and_then(grok_user_subscription_tier) {
        return Some(tier);
    }
    if has_grok_subscription_evidence(config) {
        return None;
    }
    grok_plan_display_name(
        text_field(config, "subscription_tier")
            .or_else(|| text_field(config, "subscriptionTier"))
            .as_deref(),
    )
}

fn grok_tier_from_user(value: &Value) -> Option<String> {
    let user = value.get("user").unwrap_or(value);
    let direct_subscription_tier = user
        .get("subscription")
        .filter(|item| item.is_object())
        .filter(|item| {
            text_field(item, "status").is_none_or(|status| is_active_subscription_status(&status))
        })
        .and_then(grok_user_subscription_tier);
    if direct_subscription_tier.is_some() {
        return direct_subscription_tier;
    }
    if let Some(tier) = active_grok_subscription(user).and_then(grok_user_subscription_tier) {
        return Some(tier);
    }
    if has_grok_subscription_evidence(user) {
        return None;
    }
    let raw = text_field(user, "subscriptionTier")
        .or_else(|| text_field(user, "subscription_tier"))
        .or_else(|| text_field(user, "planType"))
        .or_else(|| text_field(user, "plan_type"));
    grok_plan_display_name(raw.as_deref())
}

fn active_grok_subscription(value: &Value) -> Option<&Value> {
    let subscriptions = value
        .get("subscriptions")
        .or_else(|| value.get("items"))
        .and_then(Value::as_array)
        .or_else(|| value.as_array())?;
    subscriptions.iter().find(|subscription| {
        text_field(subscription, "status")
            .is_some_and(|status| is_active_subscription_status(&status))
    })
}

fn has_grok_subscription_evidence(value: &Value) -> bool {
    let has_direct_subscription = value.get("subscription").is_some_and(Value::is_object);
    let has_listed_subscriptions = value
        .get("subscriptions")
        .or_else(|| value.get("items"))
        .and_then(Value::as_array)
        .is_some_and(|subscriptions| !subscriptions.is_empty());
    has_direct_subscription || has_listed_subscriptions
}

fn is_active_subscription_status(status: &str) -> bool {
    status.eq_ignore_ascii_case("SUBSCRIPTION_STATUS_ACTIVE")
        || status.eq_ignore_ascii_case("SubscriptionStatusActive")
        || status.eq_ignore_ascii_case("active")
}

fn grok_tier_from_subscriptions(value: &Value) -> Option<String> {
    let subscription = active_grok_subscription(value)?;
    grok_plan_display_name(
        text_field(subscription, "tier")
            .or_else(|| text_field(subscription, "subscription_tier"))
            .or_else(|| text_field(subscription, "subscriptionTier"))
            .or_else(|| text_field(subscription, "plan"))
            .or_else(|| text_field(subscription, "plan_name"))
            .or_else(|| text_field(subscription, "planName"))
            .as_deref(),
    )
}

fn parse_cli_billing_response(value: &Value) -> Result<GrokBillingSnapshot, ProviderError> {
    parse_cli_billing_response_at(value, None)
}

fn parse_cli_billing_response_at(
    value: &Value,
    now: Option<DateTime<Utc>>,
) -> Result<GrokBillingSnapshot, ProviderError> {
    let config = value.get("config").unwrap_or(value);
    let periods = cli_billing_periods(value, config);
    let usage = config.get("usage").or_else(|| value.get("usage"));
    let explicit_percent = percent_field(config, "creditUsagePercent")
        .or_else(|| percent_field(config, "credit_usage_percent"));
    let weekly_percent = weekly_credit_usage_percent(value, config);
    let other_credit_percent = credit_usage_percent(value, config);
    let monthly_percent = monthly_usage_percent(value, config, usage);
    // On-demand spend is a separate pool; it is only a last-resort percentage
    // and does not inherit the subscription period or reset timestamp.
    let on_demand_percent = on_demand_usage_percent(value, config);

    let (used_percent, quota_pool) = if let Some(percent) = explicit_percent {
        (
            Some(percent),
            periods.first().and_then(|period| period.quota_pool),
        )
    } else if let Some(percent) = weekly_percent {
        (Some(percent), Some(GrokQuotaPool::Weekly))
    } else if let Some(percent) = other_credit_percent {
        (
            Some(percent),
            periods.first().and_then(|period| period.quota_pool),
        )
    } else if let Some(percent) = monthly_percent {
        (Some(percent), Some(GrokQuotaPool::Monthly))
    } else if let Some(percent) = on_demand_percent {
        (Some(percent), Some(GrokQuotaPool::OnDemand))
    } else {
        (None, periods.first().and_then(|period| period.quota_pool))
    };
    let period = select_grok_billing_period(&periods, quota_pool, now);
    let window_minutes = match quota_pool {
        Some(GrokQuotaPool::Weekly) => Some(
            period
                .and_then(|period| period.window_minutes)
                .unwrap_or(CYCLE_WINDOW_MINUTES),
        ),
        Some(GrokQuotaPool::Monthly) => Some(
            period
                .and_then(|period| period.window_minutes)
                .unwrap_or(MONTH_WINDOW_MINUTES),
        ),
        Some(GrokQuotaPool::OnDemand) => None,
        None => period.and_then(|period| period.window_minutes),
    };

    Ok(GrokBillingSnapshot {
        used_percent: used_percent.unwrap_or(0.0),
        resets_at: period.and_then(|period| period.end),
        window_minutes,
        period_start: period.and_then(|period| period.start),
        quota_pool,
        usage_known: used_percent.is_some(),
    })
}

const MONTH_WINDOW_MINUTES: u32 = 30 * 24 * 60;

fn cli_billing_periods(value: &Value, config: &Value) -> Vec<GrokBillingPeriod> {
    let mut periods = Vec::new();
    let current_period = config
        .get("currentPeriod")
        .or_else(|| config.get("current_period"));
    if let Some(period) = current_period {
        push_grok_billing_period(
            &mut periods,
            date_time_field(period, "start"),
            date_time_field(period, "end"),
            text_field(period, "type").or_else(|| text_field(period, "periodType")),
            false,
        );
    }

    let billing_cycle = config
        .get("billingCycle")
        .or_else(|| config.get("billing_cycle"));
    if let Some(cycle) = billing_cycle {
        push_grok_billing_period(
            &mut periods,
            date_time_field(cycle, "billingPeriodStart")
                .or_else(|| date_time_field(cycle, "billing_period_start")),
            date_time_field(cycle, "billingPeriodEnd")
                .or_else(|| date_time_field(cycle, "billing_period_end")),
            text_field(cycle, "type")
                .or_else(|| text_field(cycle, "periodType"))
                .or_else(|| text_field(cycle, "billingPeriodType")),
            false,
        );
    }

    let has_monthly_limit = amount_field(config.get("monthlyLimit"))
        .or_else(|| amount_field(value.get("monthlyLimit")))
        .is_some();
    push_grok_billing_period(
        &mut periods,
        date_time_field(config, "billingPeriodStart")
            .or_else(|| date_time_field(config, "billing_period_start")),
        date_time_field(config, "billingPeriodEnd")
            .or_else(|| date_time_field(config, "billing_period_end")),
        text_field(config, "billingPeriodType")
            .or_else(|| text_field(config, "billing_period_type")),
        has_monthly_limit,
    );

    if value.get("config").is_some() {
        push_grok_billing_period(
            &mut periods,
            date_time_field(value, "billingPeriodStart")
                .or_else(|| date_time_field(value, "billing_period_start")),
            date_time_field(value, "billingPeriodEnd")
                .or_else(|| date_time_field(value, "billing_period_end")),
            text_field(value, "billingPeriodType")
                .or_else(|| text_field(value, "billing_period_type")),
            has_monthly_limit,
        );
    }
    periods
}

fn push_grok_billing_period(
    periods: &mut Vec<GrokBillingPeriod>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    kind: Option<String>,
    monthly_legacy_hint: bool,
) {
    let measured_window = start
        .zip(end)
        .filter(|(start, end)| end > start)
        .and_then(|(start, end)| u32::try_from((end - start).num_minutes()).ok())
        .filter(|minutes| *minutes > 0);
    let quota_pool = kind
        .as_deref()
        .and_then(grok_quota_pool_from_kind)
        .or_else(|| measured_window.and_then(grok_quota_pool_from_window))
        .or_else(|| (monthly_legacy_hint && end.is_some()).then_some(GrokQuotaPool::Monthly));
    let window_minutes = measured_window.or_else(|| match quota_pool {
        Some(GrokQuotaPool::Weekly) => Some(CYCLE_WINDOW_MINUTES),
        Some(GrokQuotaPool::Monthly) => Some(MONTH_WINDOW_MINUTES),
        Some(GrokQuotaPool::OnDemand) | None => None,
    });
    if quota_pool.is_some() || start.is_some() || end.is_some() {
        periods.push(GrokBillingPeriod {
            quota_pool,
            start,
            end,
            window_minutes,
        });
    }
}

fn select_grok_billing_period(
    periods: &[GrokBillingPeriod],
    quota_pool: Option<GrokQuotaPool>,
    now: Option<DateTime<Utc>>,
) -> Option<GrokBillingPeriod> {
    if quota_pool == Some(GrokQuotaPool::OnDemand) {
        return None;
    }
    // When the quota pool is unknown, bind the usage to only the primary
    // period candidate; borrowing a lower-priority cycle could splice a
    // current reset onto usage whose own (unknown) period has expired.
    let candidates: Vec<_> = match quota_pool {
        Some(pool) => periods
            .iter()
            .filter(|period| period.quota_pool == Some(pool))
            .copied()
            .collect(),
        None => periods.first().copied().into_iter().collect(),
    };
    if let Some(now) = now {
        // A primary period with explicit but unclassifiable expired/future
        // bounds cannot be discarded merely because the usage shape (such as
        // weeklyCredits or monthlyLimit) independently identifies its pool.
        // Preserve that boundary so the caller's currentness gate fails closed.
        if let Some(primary_unknown_period) = periods.first().filter(|period| {
            period.quota_pool.is_none() && !grok_billing_period_is_current_or_unknown(period, now)
        }) {
            return Some(*primary_unknown_period);
        }
        if let Some(current) = candidates
            .iter()
            .find(|period| grok_billing_period_is_current_or_unknown(period, now))
        {
            return Some(*current);
        }
    }
    candidates.first().copied()
}

fn grok_billing_period_is_current_or_unknown(
    period: &GrokBillingPeriod,
    now: DateTime<Utc>,
) -> bool {
    period.start.is_none_or(|start| start <= now)
        && period.end.is_none_or(|end| end > now)
        && period
            .start
            .zip(period.end)
            .is_none_or(|(start, end)| end > start)
}

fn grok_quota_pool_from_kind(kind: &str) -> Option<GrokQuotaPool> {
    let kind = kind.to_ascii_lowercase();
    if kind.contains("week") {
        Some(GrokQuotaPool::Weekly)
    } else if kind.contains("month") {
        Some(GrokQuotaPool::Monthly)
    } else {
        None
    }
}

fn grok_quota_pool_from_window(window_minutes: u32) -> Option<GrokQuotaPool> {
    if (4 * 24 * 60..=10 * 24 * 60).contains(&window_minutes) {
        Some(GrokQuotaPool::Weekly)
    } else if (21 * 24 * 60..=35 * 24 * 60).contains(&window_minutes) {
        Some(GrokQuotaPool::Monthly)
    } else {
        None
    }
}

fn weekly_credit_usage_percent(value: &Value, config: &Value) -> Option<f64> {
    let credits = config
        .get("weeklyCredits")
        .or_else(|| config.get("weekly_credits"))
        .or_else(|| value.get("weeklyCredits"))
        .or_else(|| value.get("weekly_credits"));
    credits.and_then(credit_bag_usage_percent)
}

fn credit_usage_percent(billing: &Value, config: &Value) -> Option<f64> {
    [
        billing.get("credits"),
        billing.get("creditBalance"),
        billing.get("usage"),
        config.get("credits"),
        config.get("usage"),
        config.get("includedCredits"),
        config.get("subscriptionCredits"),
        config.get("sharedPool"),
    ]
    .into_iter()
    .flatten()
    .find_map(credit_bag_usage_percent)
}

fn monthly_usage_percent(value: &Value, config: &Value, usage: Option<&Value>) -> Option<f64> {
    let usage = usage?;
    let used =
        amount_field(usage.get("totalUsed")).or_else(|| amount_field(usage.get("includedUsed")))?;
    let cap = amount_field(config.get("monthlyLimit"))
        .or_else(|| amount_field(value.get("monthlyLimit")))?;
    (cap > 0.0).then_some((used / cap * 100.0).clamp(0.0, 100.0))
}

fn on_demand_usage_percent(value: &Value, config: &Value) -> Option<f64> {
    let used = amount_field(config.get("onDemandUsed"))
        .or_else(|| amount_field(config.get("on_demand_used")))
        .or_else(|| amount_field(value.get("onDemandUsed")))
        .or_else(|| amount_field(value.get("on_demand_used")))?;
    let cap = amount_field(config.get("onDemandCap"))
        .or_else(|| amount_field(config.get("on_demand_cap")))
        .or_else(|| amount_field(value.get("onDemandCap")))
        .or_else(|| amount_field(value.get("on_demand_cap")))?;
    (cap > 0.0).then_some((used / cap * 100.0).clamp(0.0, 100.0))
}

fn credit_bag_usage_percent(value: &Value) -> Option<f64> {
    if let Some(items) = value.as_array() {
        return items.iter().find_map(credit_bag_usage_percent);
    }
    let object = value.as_object()?;
    let total = first_amount(object, &["total", "limit", "cap", "allocation", "amount"]);
    let used = first_amount(object, &["used", "spent", "consumed", "usage"]);
    let remaining = first_amount(object, &["remaining", "balance", "left"]);
    if let Some(total) = total.filter(|total| total.is_finite() && *total > 0.0) {
        let used = used.or_else(|| remaining.map(|remaining| (total - remaining).max(0.0)))?;
        return Some((used.max(0.0) / total * 100.0).clamp(0.0, 100.0));
    }
    object
        .get("bags")
        .or_else(|| object.get("items"))
        .and_then(credit_bag_usage_percent)
}

fn first_amount(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| amount_field(object.get(*key)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GrokQuotaPool {
    Weekly,
    Monthly,
    OnDemand,
}

#[derive(Debug, Clone, Copy, Default)]
struct GrokBillingPeriod {
    quota_pool: Option<GrokQuotaPool>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    window_minutes: Option<u32>,
}

fn merge_billing_usage_fallback(
    mut preferred: GrokBillingSnapshot,
    fallback: GrokBillingSnapshot,
) -> GrokBillingSnapshot {
    if !grok_billing_snapshots_share_period(preferred, fallback) {
        return preferred;
    }
    preferred.resets_at = preferred.resets_at.or(fallback.resets_at);
    if !preferred.usage_known && fallback.usage_known {
        preferred.used_percent = fallback.used_percent;
        preferred.usage_known = true;
    }
    preferred
}

fn merge_cli_proxy_with_acp_fallback(
    cli_proxy: GrokBillingSnapshot,
    acp: Option<GrokBillingSnapshot>,
) -> GrokBillingSnapshot {
    acp.map(|fallback| merge_billing_usage_fallback(cli_proxy, fallback))
        .unwrap_or(cli_proxy)
}

fn merge_acp_usage_with_cli_proxy_metadata(
    acp: GrokBillingSnapshot,
    cli_proxy: GrokBillingSnapshot,
) -> GrokBillingSnapshot {
    if !grok_billing_snapshots_share_period(acp, cli_proxy) {
        return acp;
    }
    GrokBillingSnapshot {
        used_percent: acp.used_percent,
        resets_at: cli_proxy.resets_at.or(acp.resets_at),
        window_minutes: cli_proxy.window_minutes.or(acp.window_minutes),
        period_start: cli_proxy.period_start.or(acp.period_start),
        quota_pool: acp.quota_pool,
        usage_known: acp.usage_known,
    }
}

fn grok_billing_snapshots_share_period(
    first: GrokBillingSnapshot,
    second: GrokBillingSnapshot,
) -> bool {
    first.quota_pool.is_some()
        && first.quota_pool == second.quota_pool
        && first.resets_at.is_some()
        && first.resets_at == second.resets_at
        && match (first.period_start, second.period_start) {
            (Some(first_start), Some(second_start)) => first_start == second_start,
            _ => true,
        }
}

fn current_grok_billing_snapshot(
    snapshot: GrokBillingSnapshot,
    now: DateTime<Utc>,
) -> Option<GrokBillingSnapshot> {
    let expired = snapshot.resets_at.is_some_and(|resets_at| resets_at <= now);
    let not_started = snapshot.period_start.is_some_and(|start| start > now);
    let invalid_bounds = snapshot
        .period_start
        .zip(snapshot.resets_at)
        .is_some_and(|(start, end)| end <= start);
    (!expired && !not_started && !invalid_bounds).then_some(snapshot)
}

fn parse_current_grok_billing(
    value: &Value,
    now: DateTime<Utc>,
) -> Option<(GrokBillingSnapshot, Option<String>)> {
    let snapshot = parse_cli_billing_response_at(value, Some(now)).ok()?;
    let snapshot = current_grok_billing_snapshot(snapshot, now)?;
    Some((snapshot, grok_tier_from_billing(value)))
}

fn parse_current_grok_acp_billing(
    value: &Value,
    now: DateTime<Utc>,
) -> Option<(GrokBillingSnapshot, Option<String>)> {
    parse_current_grok_billing(value, now)
}

fn percent_field(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|percent| percent.is_finite() && (0.0..=100.0).contains(percent))
}

fn amount_field(value: Option<&Value>) -> Option<f64> {
    let value = value.and_then(|value| value.get("val").or(Some(value)))?;
    let amount = value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())?;
    Some(amount).filter(|amount| amount.is_finite() && *amount >= 0.0)
}

fn date_time_field(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|dt| dt.with_timezone(&Utc))
}

#[derive(Debug, Clone, Copy, Default)]
struct GrokBillingSnapshot {
    used_percent: f64,
    resets_at: Option<DateTime<Utc>>,
    window_minutes: Option<u32>,
    period_start: Option<DateTime<Utc>>,
    quota_pool: Option<GrokQuotaPool>,
    /// False when no usable used-percent existed in the billing payload. The
    /// window then carries unknown usage and must never read as a real 0%
    /// (untouched) quota.
    usage_known: bool,
}

/// Used for explicitly weekly CLI-proxy contracts and Grok's legacy weekly
/// credits gRPC endpoint. Other CLI-proxy responses must carry a period or a
/// recognizable monthly/weekly field; reset timestamps alone do not reveal a
/// cycle's full duration.
const CYCLE_WINDOW_MINUTES: u32 = 7 * 24 * 60;

fn result_from_billing(
    billing: GrokBillingSnapshot,
    source_label: &str,
    email: Option<String>,
    team_id: Option<String>,
    login_method: Option<String>,
) -> ProviderFetchResult {
    // An unknown usage must not masquerade as a fresh 0%-used (full) quota;
    // informational keeps the cycle declaration and reset without inventing
    // a percentage.
    let primary = if billing.usage_known {
        RateWindow::with_details(
            billing.used_percent,
            billing.window_minutes,
            billing.resets_at,
            None,
        )
    } else {
        let mut window =
            RateWindow::informational("Usage unknown (billing response had no usable percent)");
        window.window_minutes = billing.window_minutes;
        window.resets_at = billing.resets_at;
        window
    };
    let mut usage = UsageSnapshot::new(primary);
    usage.account_email = email;
    usage.account_organization = team_id;
    usage.login_method = login_method;
    ProviderFetchResult::new(usage, source_label)
}

fn validate_grpc_headers(headers: &reqwest::header::HeaderMap) -> Result<bool, ProviderError> {
    if let Some(value) = headers.get("grpc-status") {
        let raw = value.to_str().map_err(|_| {
            ProviderError::Parse("Grok gRPC returned a non-text grpc-status header".to_string())
        })?;
        let status = raw.parse::<u16>().map_err(|_| {
            ProviderError::Parse("Grok gRPC returned an invalid grpc-status header".to_string())
        })?;
        grpc_status_error(status)?;
        return Ok(true);
    }
    Ok(false)
}

fn validate_grpc_response_status(
    headers: &reqwest::header::HeaderMap,
    data: &[u8],
) -> Result<(), ProviderError> {
    let header_has_status = validate_grpc_headers(headers)?;
    let trailer_has_status = validate_grpc_web_trailers(data)?;
    if !header_has_status && !trailer_has_status {
        return Err(ProviderError::Parse(
            "Grok gRPC response omitted grpc-status".to_string(),
        ));
    }
    Ok(())
}

fn grpc_status_error(status: u16) -> Result<(), ProviderError> {
    if status == 0 {
        return Ok(());
    }
    if status == 16 {
        return Err(ProviderError::AuthRequired);
    }
    Err(ProviderError::Other(format!(
        "Grok RPC failed with status {status}"
    )))
}

fn validate_grpc_web_trailers(data: &[u8]) -> Result<bool, ProviderError> {
    let mut index = 0;
    let mut has_status = false;
    while index < data.len() {
        if data.len() - index < 5 {
            return Err(ProviderError::Parse(
                "Grok web billing returned a truncated gRPC-web frame header".to_string(),
            ));
        }
        let flags = data[index];
        let len = ((data[index + 1] as usize) << 24)
            | ((data[index + 2] as usize) << 16)
            | ((data[index + 3] as usize) << 8)
            | (data[index + 4] as usize);
        let start = index + 5;
        let end = start.saturating_add(len);
        if end > data.len() {
            return Err(ProviderError::Parse(
                "Grok web billing returned a truncated gRPC-web frame".to_string(),
            ));
        }
        if flags != 0 && flags != 0x80 {
            return Err(ProviderError::Parse(
                "Grok gRPC-web response used a reserved frame flag".to_string(),
            ));
        }
        if flags == 0x80 {
            let trailers = std::str::from_utf8(&data[start..end]).map_err(|_| {
                ProviderError::Parse("Grok gRPC-web trailers were not valid UTF-8".to_string())
            })?;
            for line in trailers.lines() {
                let Some((name, raw_status)) = line.split_once(':') else {
                    continue;
                };
                if !name.trim().eq_ignore_ascii_case("grpc-status") {
                    continue;
                }
                let status = raw_status.trim().parse::<u16>().map_err(|_| {
                    ProviderError::Parse(
                        "Grok gRPC-web returned an invalid grpc-status".to_string(),
                    )
                })?;
                grpc_status_error(status)?;
                has_status = true;
            }
        }
        index = end;
    }
    Ok(has_status)
}

fn parse_grpc_web_response(data: &[u8]) -> Result<GrokBillingSnapshot, ProviderError> {
    parse_grpc_web_response_at(data, Utc::now())
}

fn parse_grpc_web_response_at(
    data: &[u8],
    now: DateTime<Utc>,
) -> Result<GrokBillingSnapshot, ProviderError> {
    let frames = grpc_web_data_frames(data);
    if frames.is_empty() {
        return Err(ProviderError::Parse(
            "Grok web billing returned no payload".to_string(),
        ));
    }
    let mut scan = ProtoScan::default();
    for frame in &frames {
        scan.scan_message(frame, &mut Vec::new(), 0);
    }
    let parsed_percent = if scan.is_complete {
        scan.fixed32
            .iter()
            .filter(|field| {
                field.path.last() == Some(&1)
                    && field.value.is_finite()
                    && field.value >= 0.0
                    && field.value <= 100.0
            })
            .min_by(|a, b| {
                a.path
                    .len()
                    .cmp(&b.path.len())
                    .then_with(|| a.order.cmp(&b.order))
            })
            .map(|field| field.value as f64)
    } else {
        None
    };

    let timestamp_fields: Vec<_> = scan
        .varints
        .iter()
        .filter_map(|field| timestamp_from_varint(field).map(|date| (field.path.as_slice(), date)))
        .collect();
    let future_timestamps: Vec<_> = timestamp_fields
        .iter()
        .filter(|(_, date)| *date > now)
        .map(|(path, date)| (*path, *date))
        .collect();
    let current_period_start = timestamp_fields
        .iter()
        .find(|(path, _)| *path == [1, 8, 2, 1])
        .map(|(_, date)| *date);
    let current_period_end = timestamp_fields
        .iter()
        .find(|(path, _)| *path == [1, 8, 3, 1])
        .map(|(_, date)| *date);
    // Preserve the schema's declared current-period end even when it is past:
    // currentness validation must reject a stale explicit percentage instead
    // of losing the expired boundary and accepting it as an unknown window.
    let resets_at = current_period_end.or_else(|| {
        future_timestamps
            .iter()
            .filter(|(path, _)| *path == [1, 5, 1])
            .map(|(_, date)| *date)
            .min()
            .or_else(|| future_timestamps.iter().map(|(_, date)| *date).min())
    });
    let period_kind = scan
        .varints
        .iter()
        .find(|field| field.path == [1, 8, 1])
        .map(|field| field.value);
    let quota_pool = match period_kind {
        Some(1) => Some(GrokQuotaPool::Weekly),
        Some(2) => Some(GrokQuotaPool::Monthly),
        _ => None,
    };
    let active_period_kind = quota_pool.is_some();
    let has_active_current_period = active_period_kind
        && current_period_start.is_some_and(|start| start <= now)
        && current_period_end.is_some_and(|end| end > now);
    let has_usage_period = scan.varints.iter().any(|field| {
        field.path.starts_with(&[1, 6]) || (field.path == [1, 8, 1] && matches!(field.value, 1 | 2))
    });
    // Proto3 omits a scalar float when its value is zero. Match the verified
    // Grok web-client behavior only for one complete data frame with a
    // schema-recognized active billing period; never infer zero from a reset
    // timestamp alone, malformed bytes, history, or opaque nested fields.
    let implicit_zero = parsed_percent.is_none()
        && scan.fixed32.is_empty()
        && frames.len() == 1
        && scan.is_complete
        && resets_at.is_some()
        && has_usage_period
        && has_active_current_period;
    let usage_known = parsed_percent.is_some() || implicit_zero;
    if !usage_known {
        tracing::warn!(
            "Grok billing has no usable percentage; complete={}, fixed32_fields={}, data_frames={}, active_period={}; keeping usage unknown",
            scan.is_complete,
            scan.fixed32.len(),
            frames.len(),
            has_active_current_period
        );
    }
    let used_percent = parsed_percent.unwrap_or(0.0);
    let window_minutes = current_period_start
        .zip(current_period_end)
        .filter(|(start, end)| end > start)
        .and_then(|(start, end)| u32::try_from((end - start).num_minutes()).ok())
        .filter(|minutes| *minutes > 0);

    Ok(GrokBillingSnapshot {
        used_percent,
        resets_at,
        window_minutes,
        period_start: current_period_start,
        quota_pool,
        usage_known,
    })
}

fn timestamp_from_varint(field: &VarintField) -> Option<DateTime<Utc>> {
    (1_700_000_000..=2_100_000_000)
        .contains(&field.value)
        .then(|| Utc.timestamp_opt(field.value as i64, 0).single())
        .flatten()
}

fn grpc_web_data_frames(data: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index < data.len() {
        if index + 5 > data.len() {
            return Vec::new();
        }
        let flags = data[index];
        if flags != 0 && flags != 0x80 {
            return Vec::new();
        }
        let len = ((data[index + 1] as usize) << 24)
            | ((data[index + 2] as usize) << 16)
            | ((data[index + 3] as usize) << 8)
            | (data[index + 4] as usize);
        let start = index + 5;
        let end = start.saturating_add(len);
        if end > data.len() {
            return Vec::new();
        }
        if flags == 0 {
            frames.push(data[start..end].to_vec());
        }
        index = end;
    }
    frames
}

struct ProtoScan {
    fixed32: Vec<Fixed32Field>,
    varints: Vec<VarintField>,
    order: usize,
    is_complete: bool,
}

impl Default for ProtoScan {
    fn default() -> Self {
        Self {
            fixed32: Vec::new(),
            varints: Vec::new(),
            order: 0,
            is_complete: true,
        }
    }
}

struct Fixed32Field {
    path: Vec<u64>,
    value: f32,
    order: usize,
}

struct VarintField {
    path: Vec<u64>,
    value: u64,
}

impl ProtoScan {
    fn scan_message(&mut self, data: &[u8], path: &mut Vec<u64>, depth: usize) {
        if depth > 8 {
            return;
        }
        let mut i = 0;
        while i < data.len() {
            let Some((field, wire, next)) = read_key(data, i) else {
                self.is_complete = false;
                break;
            };
            i = next;
            path.push(field);
            let Some(next) = self.scan_field(data, i, path, depth, wire) else {
                self.is_complete = false;
                path.pop();
                break;
            };
            i = next;
            path.pop();
        }
    }

    fn scan_field(
        &mut self,
        data: &[u8],
        i: usize,
        path: &mut Vec<u64>,
        depth: usize,
        wire: u64,
    ) -> Option<usize> {
        match wire {
            0 => self.scan_varint(data, i, path),
            2 => self.scan_length_delimited(data, i, path, depth),
            5 => self.scan_fixed32(data, i, path),
            1 if i.saturating_add(8) <= data.len() => Some(i + 8),
            _ => None,
        }
    }

    fn scan_varint(&mut self, data: &[u8], i: usize, path: &[u64]) -> Option<usize> {
        let (value, next) = read_varint(data, i)?;
        self.varints.push(VarintField {
            path: path.to_vec(),
            value,
        });
        Some(next)
    }

    fn scan_length_delimited(
        &mut self,
        data: &[u8],
        i: usize,
        path: &mut Vec<u64>,
        depth: usize,
    ) -> Option<usize> {
        let (len, next) = read_varint(data, i)?;
        let start = next;
        let end = start.checked_add(usize::try_from(len).ok()?)?;
        if end <= data.len() {
            if depth < 4 && is_known_billing_message(path) {
                self.scan_message(&data[start..end], path, depth + 1);
            }
            Some(end)
        } else {
            None
        }
    }

    fn scan_fixed32(&mut self, data: &[u8], i: usize, path: &[u64]) -> Option<usize> {
        if i + 4 > data.len() {
            return None;
        }
        let bytes = [data[i], data[i + 1], data[i + 2], data[i + 3]];
        self.fixed32.push(Fixed32Field {
            path: path.to_vec(),
            value: f32::from_le_bytes(bytes),
            order: self.order,
        });
        self.order += 1;
        Some(i + 4)
    }
}

fn is_known_billing_message(path: &[u64]) -> bool {
    matches!(
        path,
        [1] | [1, 2]
            | [1, 3]
            | [1, 4]
            | [1, 5]
            | [1, 6]
            | [1, 7]
            | [1, 8]
            | [1, 12]
            | [1, 6, 1]
            | [1, 6, 2]
            | [1, 6, 3]
            | [1, 8, 2]
            | [1, 8, 3]
            | [1, 6, 3, 2]
            | [1, 6, 3, 3]
    )
}

fn read_key(data: &[u8], i: usize) -> Option<(u64, u64, usize)> {
    let (key, next) = read_varint(data, i)?;
    let field = key >> 3;
    (field > 0 && field <= 0x1fff_ffff).then_some((field, key & 0x07, next))
}

fn read_varint(data: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    for index in 0..10 {
        if i >= data.len() {
            return None;
        }
        let b = data[i];
        i += 1;
        if index == 9 && b > 1 {
            return None;
        }
        value |= u64::from(b & 0x7f) << (index * 7);
        if b & 0x80 == 0 {
            return Some((value, i));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_auth_file_prefer_oidc() {
        let auth = r#"{
          "https://accounts.x.ai/sign-in": {"key": "legacy"},
          "https://auth.x.ai::abc": {"key": "oidc", "auth_mode": "oidc", "email": "u@example.com", "user_id": "user-123"}
        }"#;
        let parsed = GrokCredentials::parse(auth).unwrap();
        assert_eq!(parsed.access_token, "oidc");
        assert_eq!(parsed.user_id.as_deref(), Some("user-123"));
    }

    #[test]
    fn expired_auth_entry_does_not_hide_a_later_valid_cli_token() {
        let auth = r#"{
          "https://accounts.x.ai/sign-in": {"key": "expired-cli", "expires_at": "2000-01-01T00:00:00Z"},
          "https://accounts.x.ai/sign-in/second": {"key": "valid-cli", "expires_at": "2099-01-01T00:00:00Z"}
        }"#;

        let cli = GrokCredentials::parse_for_kind(auth, GrokAuthKind::Cli).unwrap();
        let generic = GrokCredentials::parse(auth).unwrap();
        assert_eq!(cli.access_token, "valid-cli");
        assert_eq!(generic.access_token, "valid-cli");
    }

    #[test]
    fn splits_grpc_web_data_frames() {
        let data = [0, 0, 0, 0, 2, 1, 2, 0x80, 0, 0, 0, 1, b'x'];
        assert_eq!(grpc_web_data_frames(&data), vec![vec![1, 2]]);
    }

    #[test]
    fn grpc_web_auth_failure_in_trailer_frame_is_classified_as_auth_required() {
        let data = grpc_web_trailer_frame(
            "grpc-status: 16\r\ngrpc-message: WKE%3Dunauthenticated%3Ano-credentials\r\n",
        );
        assert!(matches!(
            validate_grpc_web_trailers(&data),
            Err(ProviderError::AuthRequired)
        ));
    }

    #[test]
    fn grpc_header_rejects_malformed_status_instead_of_treating_it_as_success() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("grpc-status", "not-a-status".parse().unwrap());
        assert!(matches!(
            validate_grpc_headers(&headers),
            Err(ProviderError::Parse(_))
        ));
    }

    #[test]
    fn grpc_trailer_validation_rejects_incomplete_frame_header() {
        let mut data = grpc_web_frame(vec![1, 2]);
        data.push(0x80);
        assert!(matches!(
            validate_grpc_web_trailers(&data),
            Err(ProviderError::Parse(_))
        ));
    }

    #[test]
    fn grpc_response_without_status_is_not_treated_as_success() {
        let headers = reqwest::header::HeaderMap::new();
        let data = grpc_web_frame(fixed32_field(1, f32::to_bits(25.0)));
        assert!(matches!(
            validate_grpc_response_status(&headers, &data),
            Err(ProviderError::Parse(_))
        ));
    }

    /// The cycle length must not depend on whether a reset date came back.
    ///
    /// It used to: the length was computed *from* the reset timestamp, so a
    /// billing response without one published a percentage with no window at
    /// all. Downstream that window is unidentifiable — the taskbar strip could
    /// not name it as any cycle, and the forecast refused to run.
    #[test]
    fn declares_its_cycle_with_or_without_a_reset_date() {
        for resets_at in [Some(Utc::now() + chrono::Duration::days(3)), None] {
            let result = result_from_billing(
                GrokBillingSnapshot {
                    used_percent: 42.0,
                    resets_at,
                    window_minutes: Some(CYCLE_WINDOW_MINUTES),
                    usage_known: true,
                    ..GrokBillingSnapshot::default()
                },
                "grok-web",
                None,
                None,
                None,
            );
            assert_eq!(
                result.usage.primary.window_minutes,
                Some(CYCLE_WINDOW_MINUTES),
                "resets_at={resets_at:?}"
            );
            assert_eq!(result.usage.primary.resets_at, resets_at);
        }
    }

    #[test]
    fn grpc_request_uses_the_grok_protobuf_false_field_frame() {
        assert_eq!(GRPC_BILLING_REQUEST_BODY, &[0, 0, 0, 0, 2, 0x08, 0x00]);
    }

    /// Grok's cycle is per-response data, so static provider metadata stays neutral.
    #[test]
    fn the_static_slot_label_does_not_assume_a_billing_cycle() {
        assert_eq!(GrokProvider::new().metadata.session_label, "Credits");
        assert_eq!(CYCLE_WINDOW_MINUTES, 7 * 24 * 60);
    }

    /// Encode a protobuf varint the same way the wire scanner decodes it.
    fn encode_varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                break;
            }
        }
        out
    }

    /// Protobuf field key, matching `read_key` in the scanner.
    fn field_key(field: u64, wire: u64) -> Vec<u8> {
        encode_varint((field << 3) | wire)
    }

    /// A fixed32 field with the bit pattern `value`, little-endian.
    fn fixed32_field(field: u64, value: u32) -> Vec<u8> {
        let mut out = field_key(field, 5);
        out.extend_from_slice(&value.to_le_bytes());
        out
    }

    /// A varint field (wire type 0) with `value`.
    fn varint_field(field: u64, value: u64) -> Vec<u8> {
        let mut out = field_key(field, 0);
        out.extend_from_slice(&encode_varint(value));
        out
    }

    /// Wrap protobuf `message` bytes in one grpc-web data frame.
    fn grpc_web_frame(message: Vec<u8>) -> Vec<u8> {
        let mut out = Vec::with_capacity(5 + message.len());
        out.push(0);
        out.push((message.len() >> 24) as u8);
        out.push((message.len() >> 16) as u8);
        out.push((message.len() >> 8) as u8);
        out.push(message.len() as u8);
        out.extend_from_slice(&message);
        out
    }

    fn grpc_web_trailer_frame(trailers: &str) -> Vec<u8> {
        let bytes = trailers.as_bytes();
        let mut out = Vec::with_capacity(5 + bytes.len());
        out.push(0x80);
        out.push((bytes.len() >> 24) as u8);
        out.push((bytes.len() >> 16) as u8);
        out.push((bytes.len() >> 8) as u8);
        out.push(bytes.len() as u8);
        out.extend_from_slice(bytes);
        out
    }

    fn length_delimited_field(field: u64, message: &[u8]) -> Vec<u8> {
        let mut out = field_key(field, 2);
        out.extend_from_slice(&encode_varint(message.len() as u64));
        out.extend_from_slice(message);
        out
    }

    fn grpc_billing_response(config: Vec<u8>) -> Vec<u8> {
        grpc_web_frame(length_delimited_field(1, &config))
    }

    fn grpc_current_period_config(start: i64, end: i64, kind: u64) -> Vec<u8> {
        let mut period = varint_field(1, kind);
        period.extend(length_delimited_field(2, &varint_field(1, start as u64)));
        period.extend(length_delimited_field(3, &varint_field(1, end as u64)));
        length_delimited_field(8, &period)
    }

    /// A reset by itself is not proof that omitted proto3 usage means zero.
    #[test]
    fn missing_percent_degrades_to_unknown_not_zero() {
        let now = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
        let future_reset = (now + chrono::Duration::days(3)).timestamp() as u64;
        let data = grpc_billing_response(length_delimited_field(5, &varint_field(1, future_reset)));
        let snapshot = parse_grpc_web_response_at(&data, now)
            .unwrap_or_else(|e| panic!("missing percent must not error: {e}"));
        assert_eq!(snapshot.used_percent, 0.0);
        assert!(!snapshot.usage_known);
        assert!(snapshot.resets_at.is_some());

        let result = result_from_billing(snapshot, "grok-web", None, None, None);
        assert!(result.usage.primary.is_informational);
        assert_eq!(result.usage.primary.window_minutes, None);
    }

    #[test]
    fn omitted_usage_is_zero_only_for_a_complete_active_billing_period() {
        let now = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 1, 8, 0, 0, 0).unwrap();
        for kind in [1, 2] {
            let data = grpc_billing_response(grpc_current_period_config(
                start.timestamp(),
                end.timestamp(),
                kind,
            ));
            let snapshot = parse_grpc_web_response_at(&data, now).unwrap();
            assert_eq!(snapshot.used_percent, 0.0);
            assert!(snapshot.usage_known);
            assert_eq!(snapshot.resets_at, Some(end));
            assert_eq!(snapshot.window_minutes, Some(7 * 24 * 60));
        }
    }

    #[test]
    fn omitted_usage_does_not_become_zero_for_inactive_or_malformed_periods() {
        let now = Utc.with_ymd_and_hms(2026, 1, 5, 0, 0, 0).unwrap();
        let future_start = Utc.with_ymd_and_hms(2026, 1, 6, 0, 0, 0).unwrap();
        let future_end = Utc.with_ymd_and_hms(2026, 1, 13, 0, 0, 0).unwrap();
        let future_period = grpc_billing_response(grpc_current_period_config(
            future_start.timestamp(),
            future_end.timestamp(),
            1,
        ));
        let future_snapshot = parse_grpc_web_response_at(&future_period, now).unwrap();
        assert!(!future_snapshot.usage_known);

        let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 1, 8, 0, 0, 0).unwrap();
        let mut malformed_config =
            grpc_current_period_config(start.timestamp(), end.timestamp(), 1);
        malformed_config.extend(length_delimited_field(6, &[0]));
        let malformed = grpc_billing_response(malformed_config);
        let malformed_snapshot = parse_grpc_web_response_at(&malformed, now).unwrap();
        assert!(!malformed_snapshot.usage_known);
    }

    #[test]
    fn explicit_grpc_percent_in_an_expired_period_is_rejected() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let start = Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 9, 30, 0, 0, 0).unwrap();
        let mut config = fixed32_field(1, f32::to_bits(42.0));
        config.extend(grpc_current_period_config(
            start.timestamp(),
            end.timestamp(),
            2,
        ));

        let snapshot = parse_grpc_web_response_at(&grpc_billing_response(config), now).unwrap();
        assert_eq!(snapshot.resets_at, Some(end));
        assert!(current_grok_billing_snapshot(snapshot, now).is_none());
    }

    #[test]
    fn explicit_percent_is_unknown_when_a_later_protobuf_field_is_malformed() {
        let mut config = fixed32_field(1, f32::to_bits(42.0));
        config.push(0x80); // Unterminated field key after an otherwise valid percent.
        let snapshot = parse_grpc_web_response(&grpc_billing_response(config)).unwrap();
        assert!(!snapshot.usage_known);
        let result = result_from_billing(snapshot, "grok-web", None, None, None);
        assert!(result.usage.primary.is_informational);
    }

    /// NaN, negative and >100 fixed32 percent fields are all unusable and
    /// must also degrade to 0.0 instead of erroring.
    #[test]
    fn out_of_range_percent_degrades_to_zero() {
        let bad_bits = [
            f32::to_bits(f32::NAN),
            f32::to_bits(-1.0),
            f32::to_bits(101.0),
        ];
        let now = Utc::now();
        let future_reset = (now + chrono::Duration::days(3)).timestamp() as u64;
        for bits in bad_bits {
            let mut config = fixed32_field(1, bits);
            config.extend(length_delimited_field(5, &varint_field(1, future_reset)));
            let data = grpc_billing_response(config);
            let snapshot = parse_grpc_web_response_at(&data, now)
                .unwrap_or_else(|e| panic!("out-of-range percent {bits} must not error: {e}"));
            assert_eq!(snapshot.used_percent, 0.0);
            assert!(!snapshot.usage_known);
        }
    }

    /// A real 0% (full quota) fixed32 with a future reset parses unchanged.
    #[test]
    fn full_quota_parses_zero_percent_and_reset() {
        let now = Utc::now();
        let future_reset = (now + chrono::Duration::days(3)).timestamp() as u64;
        let mut config = fixed32_field(1, f32::to_bits(0.0));
        config.extend(length_delimited_field(5, &varint_field(1, future_reset)));
        let data = grpc_billing_response(config);
        let snapshot = parse_grpc_web_response(&data).unwrap();
        assert_eq!(snapshot.used_percent, 0.0);
        assert!(snapshot.usage_known);
        assert!(snapshot.resets_at.is_some());
    }

    #[test]
    fn grpc_web_success_trailer_allows_usage_frame_parsing() {
        let mut data = grpc_billing_response(fixed32_field(1, f32::to_bits(32.0)));
        data.extend_from_slice(&grpc_web_trailer_frame("grpc-status: 0\r\n"));
        validate_grpc_web_trailers(&data).unwrap();
        let snapshot = parse_grpc_web_response(&data).unwrap();
        assert!(snapshot.usage_known);
        assert_eq!(snapshot.used_percent, 32.0);
    }

    #[test]
    fn acp_billing_request_uses_unescaped_json_rpc_method() {
        let line = String::from_utf8(
            grok_acp_request_line(2, "x.ai/billing", serde_json::json!({})).unwrap(),
        )
        .unwrap();
        assert!(line.contains("\"method\":\"x.ai/billing\""));
        assert!(!line.contains("x.ai\\/billing"));
    }

    #[test]
    fn acp_response_requires_matching_id_and_supports_method_unavailable_fallback() {
        assert_eq!(
            grok_acp_response_result(serde_json::json!({"jsonrpc":"2.0", "id":1, "result":{}}), 2,)
                .unwrap(),
            None
        );
        assert!(matches!(
            grok_acp_response_result(
                serde_json::json!({
                    "jsonrpc":"2.0",
                    "id":2,
                    "error":{"code":-32601,"message":"Method not found"}
                }),
                2,
            ),
            Err(ProviderError::Other(message)) if message.contains("unavailable")
        ));
    }

    #[test]
    fn cli_version_parser_extracts_the_first_line_and_rejects_empty_output() {
        assert_eq!(
            parse_grok_cli_version(b"grok 1.0.41\r\n"),
            Some("1.0.41".to_string())
        );
        assert_eq!(parse_grok_cli_version(b"\r\n"), None);
    }

    #[test]
    fn grpc_usage_fallback_rejects_a_different_pool_and_period() {
        let proxy_reset = Utc.with_ymd_and_hms(2026, 10, 2, 0, 0, 0).unwrap();
        let fallback_reset = Utc.with_ymd_and_hms(2026, 10, 3, 0, 0, 0).unwrap();
        let proxy_start = Utc.with_ymd_and_hms(2026, 9, 2, 0, 0, 0).unwrap();
        let fallback_start = Utc.with_ymd_and_hms(2026, 9, 26, 0, 0, 0).unwrap();
        let merged = merge_billing_usage_fallback(
            GrokBillingSnapshot {
                used_percent: 0.0,
                resets_at: Some(proxy_reset),
                window_minutes: Some(MONTH_WINDOW_MINUTES),
                period_start: Some(proxy_start),
                quota_pool: Some(GrokQuotaPool::Monthly),
                usage_known: false,
                ..GrokBillingSnapshot::default()
            },
            GrokBillingSnapshot {
                used_percent: 62.0,
                resets_at: Some(fallback_reset),
                window_minutes: Some(CYCLE_WINDOW_MINUTES),
                period_start: Some(fallback_start),
                quota_pool: Some(GrokQuotaPool::Weekly),
                usage_known: true,
                ..GrokBillingSnapshot::default()
            },
        );
        assert!(!merged.usage_known);
        assert_eq!(merged.used_percent, 0.0);
        assert_eq!(merged.resets_at, Some(proxy_reset));
        assert_eq!(merged.window_minutes, Some(MONTH_WINDOW_MINUTES));
    }

    #[test]
    fn grpc_usage_fallback_fills_percent_only_for_the_same_current_period() {
        let period_start = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let period_end = Utc.with_ymd_and_hms(2026, 10, 8, 0, 0, 0).unwrap();
        let merged = merge_billing_usage_fallback(
            GrokBillingSnapshot {
                resets_at: Some(period_end),
                period_start: Some(period_start),
                window_minutes: Some(CYCLE_WINDOW_MINUTES),
                quota_pool: Some(GrokQuotaPool::Weekly),
                usage_known: false,
                ..GrokBillingSnapshot::default()
            },
            GrokBillingSnapshot {
                used_percent: 62.0,
                resets_at: Some(period_end),
                period_start: Some(period_start),
                window_minutes: Some(CYCLE_WINDOW_MINUTES),
                quota_pool: Some(GrokQuotaPool::Weekly),
                usage_known: true,
                ..GrokBillingSnapshot::default()
            },
        );
        assert!(merged.usage_known);
        assert_eq!(merged.used_percent, 62.0);
        assert_eq!(merged.resets_at, Some(period_end));
        assert_eq!(merged.window_minutes, Some(CYCLE_WINDOW_MINUTES));
    }

    #[test]
    fn grpc_usage_fallback_does_not_invent_usage_when_both_sources_are_unknown() {
        let preferred = GrokBillingSnapshot {
            used_percent: 0.0,
            resets_at: None,
            window_minutes: None,
            usage_known: false,
            ..GrokBillingSnapshot::default()
        };
        let merged = merge_billing_usage_fallback(
            preferred,
            GrokBillingSnapshot {
                used_percent: 0.0,
                resets_at: None,
                window_minutes: None,
                usage_known: false,
                ..GrokBillingSnapshot::default()
            },
        );
        assert!(!merged.usage_known);
    }

    #[test]
    fn expired_acp_billing_snapshot_is_discarded_before_fallback() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let expired = GrokBillingSnapshot {
            used_percent: 87.0,
            resets_at: Some(now - chrono::Duration::seconds(1)),
            window_minutes: Some(MONTH_WINDOW_MINUTES),
            usage_known: true,
            quota_pool: Some(GrokQuotaPool::Monthly),
            ..GrokBillingSnapshot::default()
        };
        assert!(current_grok_billing_snapshot(expired, now).is_none());

        let current = GrokBillingSnapshot {
            resets_at: Some(now + chrono::Duration::days(1)),
            ..expired
        };
        assert!(
            current_grok_billing_snapshot(current, now)
                .is_some_and(|snapshot| snapshot.used_percent == 87.0)
        );
        let without_reset = GrokBillingSnapshot {
            resets_at: None,
            ..expired
        };
        assert!(current_grok_billing_snapshot(without_reset, now).is_some());
    }

    #[test]
    fn expired_acp_payload_does_not_contribute_a_plan() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let expired = serde_json::json!({
            "config": {
                "creditUsagePercent": 87.0,
                "subscription_tier": "SUPERGROK",
                "currentPeriod": {
                    "start": "2026-09-01T00:00:00Z",
                    "end": "2026-09-30T00:00:00Z"
                }
            }
        });
        assert!(parse_current_grok_acp_billing(&expired, now).is_none());

        let current = serde_json::json!({
            "config": {
                "creditUsagePercent": 42.0,
                "subscription_tier": "SUPERGROK",
                "currentPeriod": {
                    "start": "2026-09-30T00:00:00Z",
                    "end": "2026-10-30T00:00:00Z"
                }
            }
        });
        let (snapshot, tier) = parse_current_grok_acp_billing(&current, now).unwrap();
        assert!(snapshot.usage_known);
        assert_eq!(tier.as_deref(), Some("SuperGrok"));
    }

    #[test]
    fn future_acp_period_does_not_override_current_billing_or_plan() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let future = serde_json::json!({
            "config": {
                "creditUsagePercent": 87.0,
                "subscription_tier": "SUPERGROK_HEAVY",
                "currentPeriod": {
                    "type": "USAGE_PERIOD_TYPE_WEEKLY",
                    "start": "2026-10-02T00:00:00Z",
                    "end": "2026-10-09T00:00:00Z"
                }
            }
        });
        assert!(parse_current_grok_acp_billing(&future, now).is_none());
    }

    #[test]
    fn expired_current_period_falls_back_to_a_current_matching_billing_cycle() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let payload = serde_json::json!({
            "config": {
                "creditUsagePercent": 42.0,
                "subscription_tier": "SUPERGROK",
                "currentPeriod": {
                    "type": "monthly",
                    "start": "2026-09-01T00:00:00Z",
                    "end": "2026-09-30T00:00:00Z"
                },
                "billingCycle": {
                    "type": "monthly",
                    "billingPeriodStart": "2026-09-30T00:00:00Z",
                    "billingPeriodEnd": "2026-10-30T00:00:00Z"
                }
            }
        });

        let (snapshot, tier) = parse_current_grok_billing(&payload, now).unwrap();
        assert_eq!(
            snapshot.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap())
        );
        assert_eq!(
            snapshot.period_start,
            Some(Utc.with_ymd_and_hms(2026, 9, 30, 0, 0, 0).unwrap())
        );
        assert_eq!(snapshot.window_minutes, Some(MONTH_WINDOW_MINUTES));
        assert_eq!(tier.as_deref(), Some("SuperGrok"));
    }

    #[test]
    fn unknown_period_keeps_its_expired_boundary_and_is_rejected() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let payload = serde_json::json!({
            "config": {
                "creditUsagePercent": 42.0,
                "currentPeriod": {"type": "custom", "end": "2026-09-30T00:00:00Z"},
                "billingCycle": {
                    "type": "monthly",
                    "billingPeriodStart": "2026-09-30T00:00:00Z",
                    "billingPeriodEnd": "2026-10-30T00:00:00Z"
                }
            }
        });

        let parsed = parse_cli_billing_response(&payload).unwrap();
        assert_eq!(
            parsed.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 30, 0, 0, 0).unwrap())
        );
        assert!(parse_current_grok_billing(&payload, now).is_none());
    }

    #[test]
    fn typed_usage_pools_do_not_discard_an_expired_unclassified_primary_period() {
        let now = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let payloads = [
            serde_json::json!({
                "config": {
                    "weeklyCredits": {"total": 100, "remaining": 75},
                    "currentPeriod": {"type": "custom", "end": "2026-09-30T00:00:00Z"}
                }
            }),
            serde_json::json!({
                "config": {
                    "monthlyLimit": {"val": 1000},
                    "usage": {"totalUsed": {"val": 250}},
                    "currentPeriod": {"type": "custom", "end": "2026-09-30T00:00:00Z"}
                }
            }),
        ];

        for payload in payloads {
            assert!(
                parse_current_grok_billing(&payload, now).is_none(),
                "expired unclassified period was accepted: {payload}"
            );
        }
    }

    #[test]
    fn unclassified_current_period_preserves_its_measured_duration() {
        let now = Utc.with_ymd_and_hms(2026, 10, 7, 0, 0, 0).unwrap();
        let payload = serde_json::json!({
            "config": {
                "creditUsagePercent": 42.0,
                "currentPeriod": {
                    "type": "custom",
                    "start": "2026-10-01T00:00:00Z",
                    "end": "2026-10-15T00:00:00Z"
                }
            }
        });

        let (snapshot, _) = parse_current_grok_billing(&payload, now).unwrap();
        assert_eq!(snapshot.quota_pool, None);
        assert_eq!(snapshot.window_minutes, Some(14 * 24 * 60));
        assert_eq!(
            snapshot.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 10, 15, 0, 0, 0).unwrap())
        );
    }

    #[test]
    fn cli_proxy_does_not_merge_acp_usage_from_another_period() {
        let proxy_reset = Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap();
        let stale_acp_reset = Utc.with_ymd_and_hms(2026, 9, 25, 0, 0, 0).unwrap();
        let merged = merge_cli_proxy_with_acp_fallback(
            GrokBillingSnapshot {
                used_percent: 34.0,
                resets_at: Some(proxy_reset),
                window_minutes: Some(MONTH_WINDOW_MINUTES),
                usage_known: true,
                quota_pool: Some(GrokQuotaPool::Monthly),
                ..GrokBillingSnapshot::default()
            },
            Some(GrokBillingSnapshot {
                used_percent: 0.0,
                resets_at: Some(stale_acp_reset),
                window_minutes: Some(CYCLE_WINDOW_MINUTES),
                usage_known: false,
                quota_pool: Some(GrokQuotaPool::Weekly),
                ..GrokBillingSnapshot::default()
            }),
        );
        assert!(merged.usage_known);
        assert_eq!(merged.used_percent, 34.0);
        assert_eq!(merged.resets_at, Some(proxy_reset));
        assert_eq!(merged.window_minutes, Some(MONTH_WINDOW_MINUTES));
    }

    #[test]
    fn cli_proxy_period_and_reset_enrich_known_acp_usage() {
        let proxy_reset = Utc.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap();
        let merged = merge_acp_usage_with_cli_proxy_metadata(
            GrokBillingSnapshot {
                used_percent: 42.0,
                resets_at: Some(proxy_reset),
                window_minutes: None,
                period_start: Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()),
                quota_pool: Some(GrokQuotaPool::Monthly),
                usage_known: true,
                ..GrokBillingSnapshot::default()
            },
            GrokBillingSnapshot {
                used_percent: 19.0,
                resets_at: Some(proxy_reset),
                window_minutes: Some(MONTH_WINDOW_MINUTES),
                period_start: Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap()),
                usage_known: true,
                quota_pool: Some(GrokQuotaPool::Monthly),
                ..GrokBillingSnapshot::default()
            },
        );
        assert_eq!(merged.used_percent, 42.0);
        assert!(merged.usage_known);
        assert_eq!(merged.resets_at, Some(proxy_reset));
        assert_eq!(merged.window_minutes, Some(MONTH_WINDOW_MINUTES));
    }

    #[test]
    fn cli_billing_parses_current_period_percent_and_reset() {
        let payload = serde_json::json!({
            "config": {
                "creditUsagePercent": 37.5,
                "currentPeriod": {
                    "start": "2026-09-22T12:00:00Z",
                    "end": "2026-09-29T12:00:00Z"
                }
            }
        });
        let snapshot = parse_cli_billing_response(&payload).unwrap();
        assert_eq!(snapshot.used_percent, 37.5);
        assert!(snapshot.usage_known);
        assert_eq!(
            snapshot.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 9, 29, 12, 0, 0).unwrap())
        );
        assert_eq!(snapshot.window_minutes, Some(CYCLE_WINDOW_MINUTES));
    }

    #[test]
    fn cli_billing_falls_back_to_on_demand_ratio_without_subscription_period() {
        let payload = serde_json::json!({
            "config": {
                "onDemandUsed": {"val": 25},
                "onDemandCap": {"val": 100},
                "billingPeriodEnd": "2026-10-01T00:00:00Z"
            }
        });
        let snapshot = parse_cli_billing_response(&payload).unwrap();
        assert_eq!(snapshot.used_percent, 25.0);
        assert!(snapshot.usage_known);
        assert_eq!(snapshot.quota_pool, Some(GrokQuotaPool::OnDemand));
        assert_eq!(snapshot.resets_at, None);
        assert_eq!(snapshot.window_minutes, None);
    }

    #[test]
    fn cli_billing_parses_documented_total_usage_and_billing_cycle() {
        let payload = serde_json::json!({
            "billingCycle": {
                "billingPeriodStart": "2026-09-01T00:00:00Z",
                "billingPeriodEnd": "2026-10-01T00:00:00Z"
            },
            "monthlyLimit": {"val": 99900},
            "usage": {
                "includedUsed": {"val": 12345},
                "onDemandUsed": {"val": 0},
                "totalUsed": {"val": 12345}
            }
        });
        let snapshot = parse_cli_billing_response(&payload).unwrap();
        assert!((snapshot.used_percent - (12345.0 / 99900.0 * 100.0)).abs() < 0.001);
        assert!(snapshot.usage_known);
        assert_eq!(
            snapshot.resets_at,
            Some(Utc.with_ymd_and_hms(2026, 10, 1, 0, 0, 0).unwrap())
        );
        assert_eq!(snapshot.window_minutes, Some(MONTH_WINDOW_MINUTES));
    }

    #[test]
    fn cli_billing_parses_weekly_credit_bag_total_and_remaining() {
        let payload = serde_json::json!({
            "config": {
                "weeklyCredits": {
                    "total": {"val": 100},
                    "remaining": {"val": 75}
                }
            }
        });
        let snapshot = parse_cli_billing_response(&payload).unwrap();
        assert!(snapshot.usage_known);
        assert_eq!(snapshot.used_percent, 25.0);
        assert_eq!(snapshot.window_minutes, Some(CYCLE_WINDOW_MINUTES));
    }

    #[test]
    fn monthly_ratio_does_not_inherit_weekly_label_from_empty_credit_bag() {
        let payload = serde_json::json!({
            "config": {
                "weeklyCredits": {},
                "monthlyLimit": {"val": 1000},
                "usage": {"totalUsed": {"val": 250}}
            }
        });
        let snapshot = parse_cli_billing_response(&payload).unwrap();
        assert!(snapshot.usage_known);
        assert_eq!(snapshot.used_percent, 25.0);
        assert_eq!(snapshot.window_minutes, Some(MONTH_WINDOW_MINUTES));
    }

    #[test]
    fn weekly_subscription_credits_take_precedence_over_on_demand_ratio() {
        let payload = serde_json::json!({
            "config": {
                "onDemandUsed": {"val": 80},
                "onDemandCap": {"val": 100},
                "weeklyCredits": {
                    "total": {"val": 100},
                    "remaining": {"val": 75}
                }
            }
        });
        let snapshot = parse_cli_billing_response(&payload).unwrap();
        assert_eq!(snapshot.used_percent, 25.0);
        assert_eq!(snapshot.window_minutes, Some(CYCLE_WINDOW_MINUTES));
    }

    #[test]
    fn cli_billing_clamps_credit_bag_usage_and_accepts_string_amounts() {
        let payload = serde_json::json!({
            "credits": {"total": "10", "used": "15"}
        });
        let snapshot = parse_cli_billing_response(&payload).unwrap();
        assert!(snapshot.usage_known);
        assert_eq!(snapshot.used_percent, 100.0);
    }

    #[test]
    fn cli_billing_keeps_monthly_window_and_does_not_guess_unknown_cycles() {
        let monthly = parse_cli_billing_response(&serde_json::json!({
            "config": {
                "currentPeriod": {"type": "monthly", "start": "2026-09-01T00:00:00Z", "end": "2026-10-01T00:00:00Z"},
                "creditUsagePercent": 50.0
            }
        }))
        .unwrap();
        assert_eq!(monthly.window_minutes, Some(MONTH_WINDOW_MINUTES));

        let unknown = parse_cli_billing_response(&serde_json::json!({
            "config": {"creditUsagePercent": 50.0}
        }))
        .unwrap();
        assert_eq!(unknown.window_minutes, None);
        let result = result_from_billing(unknown, "grok-cli", None, None, None);
        assert_eq!(result.usage.primary.window_minutes, None);
    }

    #[test]
    fn grpc_fallback_does_not_replace_known_monthly_cycle() {
        let monthly_window = MONTH_WINDOW_MINUTES;
        let merged = merge_billing_usage_fallback(
            GrokBillingSnapshot {
                used_percent: 0.0,
                resets_at: None,
                window_minutes: Some(monthly_window),
                usage_known: false,
                quota_pool: Some(GrokQuotaPool::Monthly),
                ..GrokBillingSnapshot::default()
            },
            GrokBillingSnapshot {
                used_percent: 35.0,
                resets_at: None,
                window_minutes: Some(CYCLE_WINDOW_MINUTES),
                usage_known: true,
                quota_pool: Some(GrokQuotaPool::Weekly),
                ..GrokBillingSnapshot::default()
            },
        );
        assert!(!merged.usage_known);
        assert_eq!(merged.used_percent, 0.0);
        assert_eq!(merged.window_minutes, Some(monthly_window));
    }

    #[test]
    fn grpc_weekly_fallback_does_not_fill_an_unknown_proxy_cycle() {
        let merged = merge_billing_usage_fallback(
            GrokBillingSnapshot {
                used_percent: 0.0,
                resets_at: None,
                window_minutes: None,
                usage_known: false,
                ..GrokBillingSnapshot::default()
            },
            GrokBillingSnapshot {
                used_percent: 35.0,
                resets_at: None,
                window_minutes: Some(CYCLE_WINDOW_MINUTES),
                usage_known: true,
                quota_pool: Some(GrokQuotaPool::Weekly),
                ..GrokBillingSnapshot::default()
            },
        );
        assert!(!merged.usage_known);
        assert_eq!(merged.window_minutes, None);
    }

    #[test]
    fn cli_billing_missing_or_invalid_percent_stays_unknown() {
        for value in [
            serde_json::json!({"config": {"currentPeriod": {"end": "2026-10-01T00:00:00Z"}}}),
            serde_json::json!({"config": {"creditUsagePercent": 120.0}}),
            serde_json::json!({"config": {"onDemandUsed": {"val": 50}, "onDemandCap": {"val": 0}}}),
        ] {
            let snapshot = parse_cli_billing_response(&value).unwrap();
            assert!(!snapshot.usage_known, "payload={value}");
            let result = result_from_billing(snapshot, "grok-cli", None, None, None);
            assert!(result.usage.primary.is_informational, "payload={value}");
        }
    }

    #[test]
    fn cli_plan_sources_normalize_known_tiers_and_preserve_unknown_tiers() {
        assert_eq!(
            grok_tier_from_settings(&serde_json::json!({
                "subscription_tier_display": "SuperGrok Heavy"
            }))
            .as_deref(),
            Some("SuperGrok Heavy")
        );
        assert_eq!(
            grok_tier_from_user(&serde_json::json!({
                "user": {"subscription": {"tier": "SUBSCRIPTION_TIER_SUPERGROK"}}
            }))
            .as_deref(),
            Some("SuperGrok")
        );
        assert_eq!(
            grok_tier_from_user(&serde_json::json!({
                "subscription_tier": "FuturePlan"
            }))
            .as_deref(),
            Some("FuturePlan")
        );
        assert_eq!(
            grok_tier_from_billing(&serde_json::json!({
                "config": {"subscription_tier": "SUPERGROK_HEAVY"}
            }))
            .as_deref(),
            Some("SuperGrok Heavy")
        );
        assert_eq!(
            grok_tier_from_billing(&serde_json::json!({
                "config": {
                    "subscription_tier": "SUPERGROK",
                    "subscriptions": [{
                        "status": "SUBSCRIPTION_STATUS_CANCELED",
                        "tier": "OldPlan"
                    }]
                }
            })),
            None
        );
        assert_eq!(
            grok_tier_from_billing(&serde_json::json!({
                "config": {
                    "subscription_tier": "SUPERGROK",
                    "subscriptions": [{
                        "status": "SUBSCRIPTION_STATUS_ACTIVE",
                        "tier": "SUPERGROK_HEAVY"
                    }]
                }
            }))
            .as_deref(),
            Some("SuperGrok Heavy")
        );
        assert_eq!(
            grok_tier_from_billing(&serde_json::json!({
                "config": {
                    "subscription_tier": "SUPERGROK",
                    "subscriptions": [{"status": "SUBSCRIPTION_STATUS_ACTIVE"}]
                }
            })),
            None
        );
        assert_eq!(
            grok_tier_from_user(&serde_json::json!({
                "user": {
                    "subscriptionTier": "SUPERGROK",
                    "subscriptions": [{"status": "SUBSCRIPTION_STATUS_ACTIVE"}]
                }
            })),
            None
        );
        assert_eq!(
            grok_tier_from_subscriptions(&serde_json::json!({
                "subscriptions": [
                    {"status": "SUBSCRIPTION_STATUS_CANCELED", "tier": "OldPlan"},
                    {"status": "SUBSCRIPTION_STATUS_ACTIVE", "tier": "SUBSCRIPTION_TIER_SUPERGROK_HEAVY"}
                ]
            }))
            .as_deref(),
            Some("SuperGrok Heavy")
        );
        assert_eq!(grok_tier_from_settings(&serde_json::json!({})), None);
        assert_eq!(
            grok_tier_from_subscriptions(&serde_json::json!({"subscriptions": []})),
            None
        );
        assert_eq!(
            grok_tier_from_subscriptions(&serde_json::json!({
                "subscriptions": [{"status": "SUBSCRIPTION_STATUS_CANCELED", "tier": "OldPlan"}]
            })),
            None
        );
        assert_eq!(
            grok_tier_from_subscriptions(&serde_json::json!({
                "subscriptions": [{"tier": "UnverifiedPlan"}]
            })),
            None
        );
        assert_eq!(
            grok_tier_from_user(&serde_json::json!({
                "user": {"subscription": {"status": "canceled", "tier": "OldPlan"}}
            })),
            None
        );
        assert_eq!(
            grok_tier_from_user(&serde_json::json!({
                "user": {
                    "subscriptionTier": "SUPERGROK",
                    "subscription": {"status": "canceled", "tier": "OldPlan"}
                }
            })),
            None
        );
        assert_eq!(
            grok_tier_from_user(&serde_json::json!({
                "user": {
                    "planType": "SUPERGROK_HEAVY",
                    "subscriptions": [{"status": "SUBSCRIPTION_STATUS_CANCELED", "tier": "OldPlan"}]
                }
            })),
            None
        );
    }
}
