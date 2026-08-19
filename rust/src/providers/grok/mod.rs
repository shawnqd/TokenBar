//! Grok provider implementation.
//!
//! Uses the grok.com billing gRPC-web endpoint via either browser cookies or
//! `~/.grok/auth.json` produced by `grok login`.

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use reqwest::Client;
use serde_json::Value;
use std::path::PathBuf;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::core::{
    FetchContext, Provider, ProviderError, ProviderFetchResult, ProviderId, ProviderMetadata,
    RateWindow, SourceMode, UsageSnapshot,
};

const BILLING_ENDPOINT: &str = "https://grok.com/grok_api_v2.GrokBuildBilling/GetGrokCreditsConfig";

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
                session_label: "Weekly",
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

    fn load_credentials() -> Result<GrokCredentials, ProviderError> {
        let path = Self::auth_file_path()
            .ok_or_else(|| ProviderError::NotInstalled("Grok auth path not found".to_string()))?;
        let text = std::fs::read_to_string(&path).map_err(|_| {
            ProviderError::NotInstalled("Grok auth.json not found. Run `grok login`.".to_string())
        })?;
        GrokCredentials::parse(&text)
    }

    async fn fetch_with_auth(
        &self,
        credentials: &GrokCredentials,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let billing = self
            .fetch_billing(Some(format!("Bearer {}", credentials.access_token)), None)
            .await?;
        Ok(result_from_billing(
            billing,
            "grok-web",
            credentials.email.clone(),
            credentials.team_id.clone(),
            credentials.login_method(),
        ))
    }

    async fn fetch_with_cookie(
        &self,
        cookie_header: &str,
    ) -> Result<ProviderFetchResult, ProviderError> {
        let billing = self
            .fetch_billing(None, Some(cookie_header.to_string()))
            .await?;
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
            .body(vec![0, 0, 0, 0, 0])
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
        validate_grpc_headers(&headers)?;
        parse_grpc_web_response(&bytes)
    }

    fn detect_cli_version() -> Option<String> {
        let mut command = std::process::Command::new("grok");
        command.arg("--version");
        hide_windows_console(&mut command);
        let output = command.output().ok()?;
        let text = String::from_utf8_lossy(&output.stdout);
        let trimmed = text
            .lines()
            .next()?
            .trim()
            .strip_prefix("grok ")
            .unwrap_or(text.trim());
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}

#[cfg(windows)]
fn hide_windows_console(command: &mut std::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x08000000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_windows_console(_command: &mut std::process::Command) {}

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
            SourceMode::Auto | SourceMode::Web => {
                if let Some(ref cookie_header) = ctx.manual_cookie_header {
                    return self.fetch_with_cookie(cookie_header).await;
                }
                match crate::providers::browser_cookie_header(&["grok.com"]) {
                    Ok(cookie_header) => match self.fetch_with_cookie(&cookie_header).await {
                        Ok(result) => return Ok(result),
                        Err(ProviderError::AuthRequired) => {}
                        Err(e) => return Err(e),
                    },
                    Err(ProviderError::NoCookies) => {}
                    Err(e) => return Err(e),
                }
                let credentials = Self::load_credentials()?;
                self.fetch_with_auth(&credentials).await
            }
            SourceMode::Cli => Err(ProviderError::UnsupportedSource(SourceMode::Cli)),
            SourceMode::OAuth => Err(ProviderError::UnsupportedSource(SourceMode::OAuth)),
        }
    }

    fn available_sources(&self) -> Vec<SourceMode> {
        vec![SourceMode::Auto, SourceMode::Web]
    }

    fn supports_web(&self) -> bool {
        true
    }

    fn detect_version(&self) -> Option<String> {
        Self::detect_cli_version()
    }
}

#[derive(Debug, Clone)]
struct GrokCredentials {
    access_token: String,
    auth_mode: Option<String>,
    email: Option<String>,
    team_id: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

impl GrokCredentials {
    fn parse(text: &str) -> Result<Self, ProviderError> {
        let root: Value = serde_json::from_str(text)
            .map_err(|e| ProviderError::Parse(format!("Failed to decode Grok auth.json: {e}")))?;
        let map = root
            .as_object()
            .ok_or_else(|| ProviderError::Parse("Invalid Grok auth.json".to_string()))?;
        let mut selected: Option<(&String, &Value)> = None;
        for (scope, entry) in map {
            if entry
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
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
            auth_mode: text_field(entry, "auth_mode"),
            email: text_field(entry, "email"),
            team_id: text_field(entry, "team_id"),
            expires_at,
        })
    }

    fn login_method(&self) -> Option<String> {
        match self.auth_mode.as_deref().map(str::to_lowercase).as_deref() {
            Some("oidc") => Some("SuperGrok".to_string()),
            Some("session") => Some("session".to_string()),
            Some(other) => Some(other.to_string()),
            // An authenticated Grok token always surfaces a name in the bar;
            // fall back to the brand name when the auth mode is unknown.
            None => Some("Grok".to_string()),
        }
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

#[derive(Debug, Clone, Copy)]
struct GrokBillingSnapshot {
    used_percent: f64,
    resets_at: Option<DateTime<Utc>>,
}

/// Length of Grok's usage cycle.
///
/// This is a **declaration, not a measurement**, and it has to be: the billing
/// response yields one percentage and one reset timestamp, and a single instant
/// cannot state how long the period that ends at it was. Nothing in the payload
/// carries the cycle, so somebody has to assert it.
///
/// It used to assert a calendar month, computed by subtracting one month from
/// the reset date. That looked like a derivation but was not one — it justified
/// itself by citing `session_label: "Monthly"`, a constant a few lines up in
/// this same file, so the two agreed only because they were the same guess.
/// Observed account behaviour is a weekly cycle, and an observation beats a
/// circular citation, so the declaration follows the observation.
///
/// Declared unconditionally rather than only when a reset date arrives. The
/// cycle length is a property of the plan, not of whether one response happened
/// to include a timestamp — and gating it on that timestamp is what left Grok
/// with no identifiable window at all whenever the date was missing.
///
/// The length is load-bearing beyond the label: `UsagePace` falls back to its
/// caller's default when it is absent, decides the reset is further away than
/// one window, and returns no forecast at all. The frontend independently
/// refuses to forecast a window with no `windowMinutes`, so it was gated twice.
const CYCLE_WINDOW_MINUTES: u32 = 7 * 24 * 60;

fn result_from_billing(
    billing: GrokBillingSnapshot,
    source_label: &str,
    email: Option<String>,
    team_id: Option<String>,
    login_method: Option<String>,
) -> ProviderFetchResult {
    let mut usage = UsageSnapshot::new(RateWindow::with_details(
        billing.used_percent,
        Some(CYCLE_WINDOW_MINUTES),
        billing.resets_at,
        None,
    ));
    usage.account_email = email;
    usage.account_organization = team_id;
    usage.login_method = login_method;
    ProviderFetchResult::new(usage, source_label)
}

fn validate_grpc_headers(headers: &reqwest::header::HeaderMap) -> Result<(), ProviderError> {
    if let Some(status) = headers
        .get("grpc-status")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u16>().ok())
        && status != 0
    {
        if status == 16 {
            return Err(ProviderError::AuthRequired);
        }
        return Err(ProviderError::Other(format!(
            "Grok RPC failed with status {status}"
        )));
    }
    Ok(())
}

fn parse_grpc_web_response(data: &[u8]) -> Result<GrokBillingSnapshot, ProviderError> {
    let frames = grpc_web_data_frames(data);
    if frames.is_empty() {
        return Err(ProviderError::Parse(
            "Grok web billing returned no payload".to_string(),
        ));
    }
    let mut scan = ProtoScan::default();
    for frame in frames {
        scan.scan_message(&frame, &mut Vec::new(), 0);
    }
    let used_percent = scan
        .fixed32
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
        // A billing response without a usable used-percent (quota just
        // refreshed, or the payload shape changed) used to fail the whole
        // parse; the stale lastGood snapshot (often a full quota) stayed on
        // screen together with the error. Degrade to 0.0 (0% used = full
        // quota) and warn, keeping the diagnostic so the real response shape
        // can be located later.
        .unwrap_or_else(|| {
            let ending_at_field_1 = scan
                .fixed32
                .iter()
                .filter(|field| field.path.last() == Some(&1))
                .count();
            tracing::warn!(
                "Grok billing has no usable used-percent (fixed32 ending at field 1, value in 0..=100); fixed32 total={} ending_at_field_1={}; treating used_percent as 0.0",
                scan.fixed32.len(),
                ending_at_field_1
            );
            0.0
        });

    let resets_at = scan
        .varints
        .iter()
        .filter_map(|field| {
            (1_700_000_000..=2_100_000_000)
                .contains(&field.value)
                .then(|| Utc.timestamp_opt(field.value as i64, 0).single())
                .flatten()
        })
        .filter(|dt| *dt > Utc::now())
        .min();
    Ok(GrokBillingSnapshot {
        used_percent,
        resets_at,
    })
}

fn grpc_web_data_frames(data: &[u8]) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    let mut index = 0;
    while index + 5 <= data.len() {
        let flags = data[index];
        let len = ((data[index + 1] as usize) << 24)
            | ((data[index + 2] as usize) << 16)
            | ((data[index + 3] as usize) << 8)
            | (data[index + 4] as usize);
        let start = index + 5;
        let end = start.saturating_add(len);
        if end > data.len() {
            break;
        }
        if flags & 0x80 == 0 {
            frames.push(data[start..end].to_vec());
        }
        index = end;
    }
    frames
}

#[derive(Default)]
struct ProtoScan {
    fixed32: Vec<Fixed32Field>,
    varints: Vec<VarintField>,
    order: usize,
}

struct Fixed32Field {
    path: Vec<u64>,
    value: f32,
    order: usize,
}

struct VarintField {
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
                break;
            };
            i = next;
            path.push(field);
            let Some(next) = self.scan_field(data, i, path, depth, wire) else {
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
            0 => self.scan_varint(data, i),
            2 => self.scan_length_delimited(data, i, path, depth),
            5 => self.scan_fixed32(data, i, path),
            1 => Some(i.saturating_add(8)),
            _ => None,
        }
    }

    fn scan_varint(&mut self, data: &[u8], i: usize) -> Option<usize> {
        let (value, next) = read_varint(data, i)?;
        self.varints.push(VarintField { value });
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
        let end = start.saturating_add(len as usize);
        if end <= data.len() {
            self.scan_message(&data[start..end], path, depth + 1);
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

fn read_key(data: &[u8], i: usize) -> Option<(u64, u64, usize)> {
    let (key, next) = read_varint(data, i)?;
    Some((key >> 3, key & 0x07, next))
}

fn read_varint(data: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut value = 0u64;
    let mut shift = 0;
    while i < data.len() && shift < 64 {
        let b = data[i];
        i += 1;
        value |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some((value, i));
        }
        shift += 7;
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
          "https://auth.x.ai::abc": {"key": "oidc", "auth_mode": "oidc", "email": "u@example.com"}
        }"#;
        let parsed = GrokCredentials::parse(auth).unwrap();
        assert_eq!(parsed.access_token, "oidc");
        assert_eq!(parsed.login_method().as_deref(), Some("SuperGrok"));
    }

    #[test]
    fn splits_grpc_web_data_frames() {
        let data = [0, 0, 0, 0, 2, 1, 2, 0x80, 0, 0, 0, 1, b'x'];
        assert_eq!(grpc_web_data_frames(&data), vec![vec![1, 2]]);
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

    /// The slot name and the declared length have to state the same cycle.
    ///
    /// They are read by two different consumers — the dashboard card prints the
    /// name, the taskbar strip identifies by the length — and when they
    /// disagreed the two surfaces described the same quota differently.
    #[test]
    fn the_slot_name_agrees_with_the_declared_length() {
        assert_eq!(GrokProvider::new().metadata.session_label, "Weekly");
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

    /// A reset-timestamp varint (field 2, three days in the future).
    fn future_reset_varint() -> Vec<u8> {
        let ts = Utc::now().timestamp() as u64 + 3 * 24 * 60 * 60;
        varint_field(2, ts)
    }

    /// Billing response with no usable used-percent must degrade to 0.0
    /// instead of erroring the whole parse (the bug: quota just refreshed,
    /// or the payload shape changed, kept the stale full lastGood snapshot).
    #[test]
    fn missing_percent_degrades_to_zero() {
        let data = grpc_web_frame(future_reset_varint());
        let snapshot = parse_grpc_web_response(&data)
            .unwrap_or_else(|e| panic!("missing percent must not error: {e}"));
        assert_eq!(snapshot.used_percent, 0.0);
        assert!(snapshot.resets_at.is_some());
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
        for bits in bad_bits {
            let mut message = fixed32_field(1, bits);
            message.extend_from_slice(&future_reset_varint());
            let data = grpc_web_frame(message);
            let snapshot = parse_grpc_web_response(&data)
                .unwrap_or_else(|e| panic!("out-of-range percent {bits} must not error: {e}"));
            assert_eq!(snapshot.used_percent, 0.0);
        }
    }

    /// A real 0% (full quota) fixed32 with a future reset parses unchanged.
    #[test]
    fn full_quota_parses_zero_percent_and_reset() {
        let mut message = fixed32_field(1, f32::to_bits(0.0));
        message.extend_from_slice(&future_reset_varint());
        let data = grpc_web_frame(message);
        let snapshot = parse_grpc_web_response(&data).unwrap();
        assert_eq!(snapshot.used_percent, 0.0);
        assert!(snapshot.resets_at.is_some());
    }
}
