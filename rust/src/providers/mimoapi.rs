//! Xiaomi MiMo pay-as-you-go API-key provider.
//!
//! MiMo exposes two independent credential families: `sk-...` keys for the
//! pay-as-you-go API and `tp-...` keys for Token Plan.  The latter is tracked
//! by the cookie-backed [`super::mimo`] provider.  The public inference API
//! does not document a quota endpoint, so this provider validates the API key
//! and opportunistically reads the console balance endpoint when it accepts
//! API-key authentication.  When the console endpoint is cookie-only, the
//! card reports that the key is valid and points the user to the dashboard
//! instead of fabricating a quota percentage.

use async_trait::async_trait;
use reqwest::{Client, StatusCode};
use serde::Deserialize;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};
use crate::settings::{ApiKeys, ManualCookies};

const MIMO_API_MODELS_URL: &str = "https://api.xiaomimimo.com/v1/models";
const MIMO_CONSOLE_BALANCE_URL: &str = "https://platform.xiaomimimo.com/api/v1/balance";

#[derive(Debug, Deserialize)]
struct BalanceResponse {
    code: i64,
    #[serde(default)]
    message: Option<String>,
    data: Option<BalanceData>,
}

#[derive(Debug, Deserialize)]
struct BalanceData {
    balance: NumericValue,
    #[serde(default)]
    currency: String,
    #[serde(default, alias = "cashBalance")]
    cash_balance: Option<NumericValue>,
    #[serde(default, alias = "giftBalance")]
    gift_balance: Option<NumericValue>,
}

/// The platform has returned both JSON numbers and quoted decimal strings for
/// balance fields. Accept either shape so a successful console response is
/// never discarded and replaced with the much less useful API-key probe.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NumericValue {
    Number(f64),
    String(String),
}

impl NumericValue {
    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::String(value) => value.trim().parse::<f64>().ok(),
        }
    }
}

#[derive(Debug, Deserialize, Default)]
struct ModelsResponse {
    #[serde(default)]
    data: Vec<serde_json::Value>,
}

pub struct MiMoApiProvider {
    metadata: ProviderMetadata,
    client: Client,
}

impl MiMoApiProvider {
    pub fn new() -> Self {
        Self {
            metadata: ProviderMetadata {
                id: ProviderId::MiMoApi,
                display_name: "Xiaomi MiMo API",
                session_label: "API Key",
                weekly_label: "Balance",
                supports_opus: false,
                supports_credits: true,
                default_enabled: false,
                is_primary: false,
                dashboard_url: Some("https://platform.xiaomimimo.com/#/console/balance"),
                status_page_url: None,
            },
            client: crate::core::credentialed_http_client_builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| Client::new()),
        }
    }

    fn resolve_api_key(ctx: &FetchContext) -> Option<String> {
        ctx.api_key
            .as_deref()
            .map(str::trim)
            .filter(|key| !key.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| {
                ["MIMO_API_KEY", "XIAOMI_MIMO_API_KEY", "XIAOMI_API_KEY"]
                    .iter()
                    .find_map(|name| {
                        std::env::var(name)
                            .ok()
                            .map(|value| value.trim().to_string())
                            .filter(|value| !value.is_empty())
                    })
            })
            .or_else(|| {
                ApiKeys::load()
                    .get("mimoapi")
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(ToOwned::to_owned)
            })
    }

    /// The API key authorizes the inference probe. The platform session is a
    /// separate first-party credential used only for the prepaid balance
    /// endpoint. Reuse the MiMo platform cookie imported for Token Plan so a
    /// user does not need to upload the same browser session twice.
    fn resolve_console_cookie(ctx: &FetchContext) -> Option<String> {
        let cookie = ctx
            .manual_cookie_header
            .as_deref()
            .map(ToOwned::to_owned)
            .or_else(|| ManualCookies::load().get("mimo").map(ToOwned::to_owned))
            .or_else(|| {
                crate::providers::browser_cookie_header(&["platform.xiaomimimo.com"]).ok()
            })?;
        super::mimo::normalize_cookie_header(&cookie)
    }

    async fn fetch_api(
        &self,
        api_key: &str,
        console_cookie: Option<&str>,
    ) -> Result<UsageSnapshot, ProviderError> {
        // Validate the pay-as-you-go key first. A console cookie never replaces
        // the API-key check, so the card cannot report a browser-only session
        // as a working API configuration.
        let key_snapshot = self.probe_models(api_key).await?;

        if let Some(cookie) = console_cookie {
            match self.fetch_console_balance_with_cookie(cookie).await {
                Ok(snapshot) => return Ok(snapshot),
                Err(ProviderError::AuthRequired) => {
                    let mut status = key_snapshot;
                    status.primary.reset_description = Some(
                        "API key valid; MiMo console cookie was rejected. Re-import a fresh MiMo Cookie to read balance.".into(),
                    );
                    return Ok(status);
                }
                // An unavailable console endpoint must not turn a working API
                // key into an error. Keep the legacy API-key probe below.
                Err(_) => {}
            }
        }

        // Older platform deployments briefly accepted API-key auth here. Keep
        // that compatibility path, but the normal path is the first-party
        // console session above.
        match self.fetch_console_balance_with_api_key(api_key).await {
            Ok(snapshot) => Ok(snapshot),
            Err(ProviderError::AuthRequired)
            | Err(ProviderError::Parse(_))
            | Err(ProviderError::Other(_)) => Ok(key_snapshot),
            Err(error) => Err(error),
        }
    }

    async fn fetch_console_balance_with_api_key(
        &self,
        api_key: &str,
    ) -> Result<UsageSnapshot, ProviderError> {
        let response = self
            .client
            .get(MIMO_CONSOLE_BALANCE_URL)
            .header("api-key", api_key)
            .bearer_auth(api_key)
            .header("Accept", "application/json")
            .header("User-Agent", "CodexBar/1.0")
            .send()
            .await?;

        self.snapshot_from_balance_response(response).await
    }

    async fn fetch_console_balance_with_cookie(
        &self,
        cookie: &str,
    ) -> Result<UsageSnapshot, ProviderError> {
        let response = self
            .client
            .get(MIMO_CONSOLE_BALANCE_URL)
            .header("Cookie", cookie)
            .header("Accept", "application/json, text/plain, */*")
            .header("Origin", "https://platform.xiaomimimo.com")
            .header(
                "Referer",
                "https://platform.xiaomimimo.com/#/console/balance",
            )
            .header("x-timeZone", "UTC+08:00")
            .header("User-Agent", "CodexBar/1.0")
            .send()
            .await?;

        self.snapshot_from_balance_response(response).await
    }

    async fn snapshot_from_balance_response(
        &self,
        response: reqwest::Response,
    ) -> Result<UsageSnapshot, ProviderError> {
        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(ProviderError::AuthRequired);
        }
        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "MiMo balance endpoint returned status {}",
                response.status()
            )));
        }

        let payload: BalanceResponse = response.json().await.map_err(|error| {
            ProviderError::Parse(format!("Failed to parse MiMo balance response: {error}"))
        })?;
        if payload.code == 401 {
            return Err(ProviderError::AuthRequired);
        }
        if payload.code != 0 {
            return Err(ProviderError::Other(payload.message.unwrap_or_else(|| {
                format!("MiMo balance error code {}", payload.code)
            })));
        }
        let data = payload
            .data
            .ok_or_else(|| ProviderError::Parse("MiMo balance payload missing".into()))?;
        let balance = data
            .balance
            .as_f64()
            .ok_or_else(|| ProviderError::Parse("MiMo balance value invalid".into()))?;
        let cash_balance = data
            .cash_balance
            .as_ref()
            .and_then(NumericValue::as_f64)
            .map(|value| value.to_string());
        let gift_balance = data
            .gift_balance
            .as_ref()
            .and_then(NumericValue::as_f64)
            .map(|value| value.to_string());

        let description = super::mimo::balance_description(
            balance,
            &data.currency,
            cash_balance.as_deref(),
            gift_balance.as_deref(),
        );
        Ok(balance_snapshot(description))
    }

    async fn probe_models(&self, api_key: &str) -> Result<UsageSnapshot, ProviderError> {
        let response = self
            .client
            .get(MIMO_API_MODELS_URL)
            .header("api-key", api_key)
            .header("Accept", "application/json")
            .header("User-Agent", "CodexBar/1.0")
            .send()
            .await?;

        if response.status() == StatusCode::UNAUTHORIZED
            || response.status() == StatusCode::FORBIDDEN
        {
            return Err(ProviderError::AuthRequired);
        }
        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "MiMo API returned status {}",
                response.status()
            )));
        }

        let models = response
            .json::<ModelsResponse>()
            .await
            .unwrap_or_default()
            .data
            .len();
        let description = if models > 0 {
            format!(
                "API key valid; {models} models available. Balance is shown in the MiMo console"
            )
        } else {
            "API key valid. Balance is shown in the MiMo console".to_string()
        };
        Ok(balance_snapshot(description).with_login_method("Pay-as-you-go API key"))
    }
}

fn balance_snapshot(description: String) -> UsageSnapshot {
    let primary = RateWindow::with_details(0.0, None, None, Some(description));
    UsageSnapshot::new(primary).with_login_method("Pay-as-you-go API key")
}

impl Default for MiMoApiProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for MiMoApiProvider {
    fn id(&self) -> ProviderId {
        ProviderId::MiMoApi
    }

    fn metadata(&self) -> &ProviderMetadata {
        &self.metadata
    }

    async fn fetch_usage(&self, ctx: &FetchContext) -> Result<ProviderFetchResult, ProviderError> {
        match ctx.source_mode {
            SourceMode::Auto | SourceMode::Web => {
                let api_key = Self::resolve_api_key(ctx).ok_or(ProviderError::AuthRequired)?;
                let console_cookie = Self::resolve_console_cookie(ctx);
                Ok(ProviderFetchResult::new(
                    self.fetch_api(&api_key, console_cookie.as_deref()).await?,
                    "api",
                ))
            }
            source => Err(ProviderError::UnsupportedSource(source)),
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Web]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_response_accepts_numeric_console_fields() {
        let response: BalanceResponse = serde_json::from_str(
            r#"{
                "code": 0,
                "data": {
                    "balance": 34.46,
                    "currency": "CNY",
                    "cashBalance": 30,
                    "giftBalance": "4.46"
                }
            }"#,
        )
        .expect("the platform's numeric balance payload should deserialize");

        let data = response.data.expect("response should include balance data");
        assert_eq!(data.balance.as_f64(), Some(34.46));
        assert_eq!(
            data.cash_balance.as_ref().and_then(NumericValue::as_f64),
            Some(30.0)
        );
        assert_eq!(
            data.gift_balance.as_ref().and_then(NumericValue::as_f64),
            Some(4.46)
        );
    }
}
