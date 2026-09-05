//! ZCode Start Plan / 体验套餐 remaining.
//!
//! Coding Plan 5h/week still comes from `quota/limit`. The trial package is a
//! separate ZCode billing envelope (`GET /api/v1/zcode-plan/billing/balance`)
//! authenticated with the local `zcodejwttoken`, not the BigModel API key.
//! Weekend campaigns (`zcode-v3-start-plan-wk-*`) count as start-plan too.
//!
//! Local credentials are AES-256-GCM `enc:v1:` blobs. The JWT is decrypted in
//! memory and never logged.

use std::path::PathBuf;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::core::{NamedRateWindow, RateWindow, UsageSnapshot};

use super::settings::{self, EnvMap};

const ENC_V1_PREFIX: &str = "enc:v1:";
const JWT_ENV: &str = "ZCODE_JWT";
const JWT_ENV_ALIAS: &str = "Z_AI_ZCODE_JWT";
const SECRET_ENV: &str = "ZCODE_CREDENTIAL_SECRET";
const CREDENTIALS_PATH_ENV: &str = "ZCODE_CREDENTIALS_PATH";
const DEVICE_MID_ENV: &str = "ZCODE_DEVICE_MID";
const APP_VERSION_ENV: &str = "ZCODE_APP_VERSION";
const BILLING_URL_ENV: &str = "ZCODE_BILLING_BALANCE_URL";
const DEFAULT_APP_VERSION: &str = "3.11.2";
const DEFAULT_BILLING_URL: &str = "https://zcode.z.ai/api/v1/zcode-plan/billing/balance";
const JWT_KEY: &str = "zcodejwttoken";
const START_PLAN_TITLE: &str = "体验套餐";

#[derive(Debug, Deserialize)]
struct BillingEnvelope {
    #[serde(default)]
    code: Option<i32>,
    #[serde(default)]
    data: Option<BillingData>,
}

#[derive(Debug, Default, Deserialize)]
struct BillingData {
    #[serde(default)]
    plans: Vec<BillingPlan>,
    #[serde(default)]
    balances: Vec<BillingBalance>,
}

#[derive(Debug, Deserialize)]
struct BillingPlan {
    #[serde(default)]
    plan_id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    ends_at: Option<Value>,
    #[serde(default)]
    entitlements: Vec<BillingEntitlement>,
}

#[derive(Debug, Deserialize)]
struct BillingEntitlement {
    #[serde(default)]
    entitlement_id: Option<String>,
    #[serde(default)]
    show_name: Option<String>,
    #[serde(default)]
    period: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BillingBalance {
    #[serde(default)]
    plan_id: Option<String>,
    #[serde(default)]
    entitlement_id: Option<String>,
    #[serde(default)]
    show_name: Option<String>,
    #[serde(default)]
    total_units: Option<Value>,
    #[serde(default)]
    used_units: Option<Value>,
    #[serde(default)]
    remaining_units: Option<Value>,
    #[serde(default)]
    available_units: Option<Value>,
    #[serde(default)]
    expires_at: Option<Value>,
    #[serde(default)]
    capabilities: Vec<String>,
}

/// Best-effort Start Plan windows. Missing JWT, decrypt failure, WAF or 401
/// must not fail the z.ai fetch — they also must not be treated as "no trial".
pub(super) async fn fetch_start_plan_windows(
    client: &reqwest::Client,
    env: &EnvMap,
) -> Vec<NamedRateWindow> {
    let Some(jwt) = resolve_zcode_jwt(env) else {
        tracing::info!("z.ai ZCode start-plan skipped: no jwt");
        return Vec::new();
    };
    let url = billing_balance_url(env);
    let app_version = app_version(env);
    let authorization = super::authorization_header(&jwt);
    let platform = node_platform();
    let arch = node_arch();
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut request = client
        .get(&url)
        .header("Authorization", authorization)
        .header("Accept", "application/json")
        .header("User-Agent", format!("ZCode/{app_version}"))
        .header("HTTP-Referer", "https://zcode.z.ai")
        .header("X-Title", "Z Code@electron")
        .header("X-ZCode-App-Version", app_version.as_str())
        .header("X-Platform", format!("{platform}-{arch}"))
        .header("X-Release-Channel", "production")
        .header("X-Client-Language", "zh-CN")
        .header("X-Client-Timezone", "Asia/Shanghai")
        .header("X-Os-Category", os_category(platform))
        .header("x-request-id", request_id)
        .timeout(std::time::Duration::from_secs(15));
    if let Some(device_mid) = device_mid(env) {
        request = request.header("X-Device-Mid", device_mid);
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(err) => {
            tracing::info!("z.ai ZCode start-plan request failed: {err}");
            return Vec::new();
        }
    };
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        tracing::info!("z.ai ZCode start-plan JWT rejected; not treating as no trial");
        return Vec::new();
    }
    if !status.is_success() {
        tracing::info!("z.ai ZCode start-plan HTTP {status}");
        return Vec::new();
    }
    let body: Value = match response.json().await {
        Ok(body) => body,
        Err(err) => {
            tracing::info!("z.ai ZCode start-plan body: {err}");
            return Vec::new();
        }
    };
    let windows = parse_start_plan_windows(&body);
    tracing::info!("z.ai ZCode start-plan windows={}", windows.len());
    windows
}

pub(super) fn merge_start_plan(
    usage: UsageSnapshot,
    windows: Vec<NamedRateWindow>,
) -> UsageSnapshot {
    if windows.is_empty() {
        return usage;
    }
    let mut usage = usage;
    if usage.primary.is_informational {
        usage = usage.with_login_method(START_PLAN_TITLE);
    }
    for window in windows {
        usage = usage.with_named_rate_window(window);
    }
    usage
}

pub(super) fn parse_start_plan_windows(body: &Value) -> Vec<NamedRateWindow> {
    let Ok(envelope) = serde_json::from_value::<BillingEnvelope>(body.clone()) else {
        return Vec::new();
    };
    if envelope
        .code
        .is_some_and(|code| code != 0 && code != 200)
    {
        return Vec::new();
    }
    let data = envelope.data.unwrap_or_default();
    let active_ids = active_start_plan_ids(&data.plans);
    let mut windows = Vec::new();
    for (index, balance) in data.balances.iter().enumerate() {
        let plan_id = balance.plan_id.as_deref().unwrap_or("");
        let allowed = if active_ids.is_empty() {
            is_start_plan_identity(plan_id, "")
        } else {
            active_ids.iter().any(|id| id == plan_id)
        };
        if !allowed {
            continue;
        }
        let plan = data
            .plans
            .iter()
            .find(|plan| plan.plan_id.as_deref() == Some(plan_id));
        if let Some(window) = balance_window(index, balance, plan) {
            windows.push(window);
        }
    }
    windows
}

fn active_start_plan_ids(plans: &[BillingPlan]) -> Vec<String> {
    plans
        .iter()
        .filter(|plan| {
            plan.status
                .as_deref()
                .is_some_and(|status| status.eq_ignore_ascii_case("active"))
                && is_start_plan_identity(
                    plan.plan_id.as_deref().unwrap_or(""),
                    plan.name.as_deref().unwrap_or(""),
                )
        })
        .filter_map(|plan| plan.plan_id.as_ref().and_then(|id| settings::cleaned(id)))
        .collect()
}

fn is_start_plan_identity(plan_id: &str, name: &str) -> bool {
    let haystack = format!("{plan_id} {name}").to_ascii_lowercase();
    haystack.contains("start-plan") || haystack.contains("start plan")
}

fn balance_window(
    index: usize,
    balance: &BillingBalance,
    plan: Option<&BillingPlan>,
) -> Option<NamedRateWindow> {
    let total = finite_nonnegative(balance.total_units.as_ref())
        .or_else(|| finite_nonnegative(balance.available_units.as_ref()).and_then(|available| {
            finite_nonnegative(balance.used_units.as_ref()).map(|used| used + available)
        }))?;
    if total <= 0.0 {
        return None;
    }
    let remaining = finite_nonnegative(balance.remaining_units.as_ref())
        .or_else(|| finite_nonnegative(balance.available_units.as_ref()))
        .unwrap_or(0.0)
        .min(total);
    let used = finite_nonnegative(balance.used_units.as_ref())
        .unwrap_or((total - remaining).max(0.0))
        .clamp(0.0, total);
    let used_percent = (used / total * 100.0).clamp(0.0, 100.0);
    let show_name = [
        balance.show_name.as_deref(),
        entitlement_show_name(plan, balance.entitlement_id.as_deref()),
        model_from_capabilities(&balance.capabilities),
    ]
    .into_iter()
    .flatten()
    .map(str::trim)
    .find(|value| !value.is_empty());
    let title = match show_name {
        Some(name) => format!("{START_PLAN_TITLE} · {name}"),
        None => START_PLAN_TITLE.to_string(),
    };
    let period = entitlement_period(plan, balance.entitlement_id.as_deref());
    let window_minutes = period.and_then(window_minutes_for_period);
    let resets_at = unix_seconds(balance.expires_at.as_ref())
        .or_else(|| plan.and_then(|plan| unix_seconds(plan.ends_at.as_ref())));
    let mut parts = Vec::new();
    if let Some(period) = period.filter(|value| !value.eq_ignore_ascii_case("one_time")) {
        parts.push(period.to_string());
    }
    parts.push(format!("剩余 {}", format_token_amount(remaining)));
    Some(NamedRateWindow::new(
        format!("zai-zcode-{index}"),
        title,
        RateWindow::with_details(used_percent, window_minutes, resets_at, Some(parts.join(" · "))),
    ))
}

fn entitlement_show_name<'a>(
    plan: Option<&'a BillingPlan>,
    entitlement_id: Option<&str>,
) -> Option<&'a str> {
    let entitlement_id = entitlement_id?;
    plan?.entitlements.iter().find_map(|entitlement| {
        entitlement
            .entitlement_id
            .as_deref()
            .filter(|id| *id == entitlement_id)
            .and(entitlement.show_name.as_deref())
    })
}

fn entitlement_period<'a>(
    plan: Option<&'a BillingPlan>,
    entitlement_id: Option<&str>,
) -> Option<&'a str> {
    let plan = plan?;
    let matched = entitlement_id.and_then(|entitlement_id| {
        plan.entitlements.iter().find_map(|entitlement| {
            entitlement
                .entitlement_id
                .as_deref()
                .filter(|id| *id == entitlement_id)
                .and(entitlement.period.as_deref())
        })
    });
    matched
        .or_else(|| {
            plan.entitlements
                .iter()
                .find_map(|entitlement| entitlement.period.as_deref())
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn model_from_capabilities(capabilities: &[String]) -> Option<&str> {
    capabilities.iter().find_map(|raw| {
        let trimmed = raw.trim();
        trimmed
            .strip_prefix("model:")
            .or_else(|| trimmed.strip_prefix("MODEL:"))
            .map(str::trim)
            .filter(|value| !value.is_empty())
    })
}

fn window_minutes_for_period(period: &str) -> Option<u32> {
    match period.trim().to_ascii_lowercase().as_str() {
        "daily" | "day" => Some(1440),
        "weekly" | "week" => Some(10080),
        "monthly" | "month" => Some(43_200),
        _ => None,
    }
}

fn format_token_amount(value: f64) -> String {
    if value >= 100_000_000.0 {
        format!("{:.2}亿", value / 100_000_000.0)
    } else if value >= 10_000.0 {
        format!("{:.1}万", value / 10_000.0)
    } else {
        format!("{value:.0}")
    }
}

fn finite_nonnegative(value: Option<&Value>) -> Option<f64> {
    let value = value?;
    let number = value
        .as_f64()
        .or_else(|| value.as_i64().map(|n| n as f64))
        .or_else(|| value.as_u64().map(|n| n as f64))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))?;
    (number.is_finite() && number >= 0.0).then_some(number)
}

fn unix_seconds(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let value = value?;
    let seconds = value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|n| i64::try_from(n).ok()))
        .or_else(|| value.as_f64().map(|n| n as i64))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))?;
    if seconds <= 0 {
        return None;
    }
    DateTime::<Utc>::from_timestamp(seconds, 0)
}

fn resolve_zcode_jwt(env: &EnvMap) -> Option<String> {
    if let Some(raw) = env_cleaned(env, JWT_ENV).or_else(|| env_cleaned(env, JWT_ENV_ALIAS)) {
        return plaintext_jwt(&raw, env);
    }
    let path = credentials_path(env)?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let map: serde_json::Map<String, Value> = serde_json::from_str(&raw).ok()?;
    let stored = map.get(JWT_KEY)?.as_str()?.trim();
    if stored.is_empty() {
        return None;
    }
    plaintext_jwt(stored, env)
}

fn plaintext_jwt(raw: &str, env: &EnvMap) -> Option<String> {
    let decrypted = if raw.starts_with(ENC_V1_PREFIX) {
        decrypt_enc_v1(raw, &credential_secret(env)).ok()?
    } else {
        raw.to_string()
    };
    let cleaned = settings::cleaned(&decrypted)?;
    if cleaned.starts_with(ENC_V1_PREFIX) {
        None
    } else {
        Some(cleaned)
    }
}

fn credentials_path(env: &EnvMap) -> Option<PathBuf> {
    if let Some(path) = env_cleaned(env, CREDENTIALS_PATH_ENV) {
        return Some(PathBuf::from(path));
    }
    let home = dirs::home_dir()?;
    Some(home.join(".zcode").join("v2").join("credentials.json"))
}

fn device_mid(env: &EnvMap) -> Option<String> {
    if let Some(mid) = env_cleaned(env, DEVICE_MID_ENV) {
        return Some(mid);
    }
    let path = credentials_path(env)?
        .parent()
        .map(|dir| dir.join("telemetry-state.json"))?;
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: Value = serde_json::from_str(&raw).ok()?;
    parsed
        .get("deviceMid")
        .and_then(Value::as_str)
        .and_then(settings::cleaned)
}

fn credential_secret(env: &EnvMap) -> String {
    if let Some(secret) = env_cleaned(env, SECRET_ENV) {
        return secret;
    }
    format!(
        "zcode-credential-fallback:{}:{}:{}",
        node_platform(),
        home_dir_string(),
        username()
    )
}

fn decrypt_enc_v1(ciphertext: &str, secret: &str) -> Result<String, ()> {
    let rest = ciphertext.strip_prefix(ENC_V1_PREFIX).ok_or(())?;
    let mut parts = rest.split('.');
    let iv = decode_b64url(parts.next().ok_or(())?)?;
    let tag = decode_b64url(parts.next().ok_or(())?)?;
    let body = decode_b64url(parts.next().ok_or(())?)?;
    if parts.next().is_some() || iv.len() != 12 || tag.len() != 16 {
        return Err(());
    }
    let key = Sha256::digest(secret.as_bytes());
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| ())?;
    let mut payload = body;
    payload.extend_from_slice(&tag);
    let plain = cipher
        .decrypt(Nonce::from_slice(&iv), payload.as_ref())
        .map_err(|_| ())?;
    String::from_utf8(plain).map_err(|_| ())
}

#[cfg(test)]
fn encrypt_enc_v1(plaintext: &str, secret: &str, iv: &[u8; 12]) -> String {
    let key = Sha256::digest(secret.as_bytes());
    let cipher = Aes256Gcm::new_from_slice(&key).expect("aes key");
    let sealed = cipher
        .encrypt(Nonce::from_slice(iv), plaintext.as_bytes())
        .expect("encrypt");
    let split = sealed.len().saturating_sub(16);
    let (body, tag) = sealed.split_at(split);
    format!(
        "{ENC_V1_PREFIX}{}.{}.{}",
        URL_SAFE_NO_PAD.encode(iv),
        URL_SAFE_NO_PAD.encode(tag),
        URL_SAFE_NO_PAD.encode(body)
    )
}

fn decode_b64url(raw: &str) -> Result<Vec<u8>, ()> {
    URL_SAFE_NO_PAD.decode(raw.trim()).map_err(|_| ())
}

fn billing_balance_url(env: &EnvMap) -> String {
    let mut url = env_cleaned(env, BILLING_URL_ENV).unwrap_or_else(|| DEFAULT_BILLING_URL.to_string());
    let version = app_version(env);
    if let Ok(mut parsed) = reqwest::Url::parse(&url) {
        parsed.query_pairs_mut().append_pair("app_version", &version);
        url = parsed.to_string();
    }
    url
}

fn app_version(env: &EnvMap) -> String {
    env_cleaned(env, APP_VERSION_ENV).unwrap_or_else(|| DEFAULT_APP_VERSION.to_string())
}

fn env_cleaned(env: &EnvMap, key: &str) -> Option<String> {
    env.get(key).and_then(|raw| settings::cleaned(raw))
}

fn node_platform() -> &'static str {
    if cfg!(windows) {
        "win32"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    }
}

fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    }
}

fn os_category(platform: &str) -> &'static str {
    match platform {
        "darwin" => "macos",
        "win32" => "windows",
        _ => "linux",
    }
}

fn home_dir_string() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .to_string_lossy()
        .into_owned()
}

fn username() -> String {
    std::env::var("USERNAME")
        .ok()
        .or_else(|| std::env::var("USER").ok())
        .and_then(|raw| settings::cleaned(&raw))
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weekend_fixture() -> Value {
        serde_json::json!({
            "code": 0,
            "msg": "",
            "data": {
                "server_time": 1788575999,
                "plans": [{
                    "user_plan_id": "upl_test",
                    "plan_id": "zcode-v3-start-plan-wk-0904",
                    "name": "ZCode Weekend Build",
                    "description": "ZCode 周末活动",
                    "status": "active",
                    "starts_at": 1788525435,
                    "ends_at": 1788706800,
                    "entitlements": [{
                        "entitlement_id": "ent-zcode-v3-start-plan-wk-0904-1",
                        "show_name": "GLM-5.3-Flash",
                        "meter": "model_usage",
                        "unit_type": "token",
                        "capabilities": ["model:glm-5.3-flash"],
                        "grant_units": 300000000,
                        "period": "one_time"
                    }]
                }],
                "balances": [{
                    "plan_id": "zcode-v3-start-plan-wk-0904",
                    "entitlement_id": "ent-zcode-v3-start-plan-wk-0904-1",
                    "show_name": "GLM-5.3-Flash",
                    "meter": "model_usage",
                    "total_units": 300000000,
                    "used_units": 0,
                    "remaining_units": 300000000,
                    "available_units": 300000000,
                    "reserved_units": null,
                    "capabilities": ["model:glm-5.3-flash"]
                }]
            }
        })
    }

    #[test]
    fn weekend_start_plan_becomes_trial_window_with_remaining() {
        let windows = parse_start_plan_windows(&weekend_fixture());
        assert_eq!(windows.len(), 1);
        let window = &windows[0];
        assert_eq!(window.id, "zai-zcode-0");
        assert_eq!(window.title, "体验套餐 · GLM-5.3-Flash");
        assert!((window.window.used_percent - 0.0).abs() < f64::EPSILON);
        assert_eq!(window.window.window_minutes, None);
        assert_eq!(
            window.window.reset_description.as_deref(),
            Some("剩余 3.00亿")
        );
        assert!(window.window.resets_at.is_some());
    }

    #[test]
    fn daily_preview_split_keeps_one_window_per_model() {
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "plans": [{
                    "plan_id": "zcode-v3-start-plan",
                    "name": "Start Plan",
                    "status": "active",
                    "entitlements": [
                        {"entitlement_id": "e1", "show_name": "GLM-5.3", "period": "daily"},
                        {"entitlement_id": "e2", "show_name": "GLM-5.3-Flash", "period": "daily"}
                    ]
                }],
                "balances": [
                    {
                        "plan_id": "zcode-v3-start-plan",
                        "entitlement_id": "e1",
                        "show_name": "GLM-5.3",
                        "total_units": 3000000,
                        "used_units": 500000,
                        "remaining_units": 2500000
                    },
                    {
                        "plan_id": "zcode-v3-start-plan",
                        "entitlement_id": "e2",
                        "show_name": "GLM-5.3-Flash",
                        "total_units": 5000000,
                        "used_units": 0,
                        "remaining_units": 5000000
                    }
                ]
            }
        });
        let windows = parse_start_plan_windows(&body);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].title, "体验套餐 · GLM-5.3");
        assert_eq!(windows[0].window.window_minutes, Some(1440));
        assert!((windows[0].window.used_percent - (500000.0 / 3000000.0 * 100.0)).abs() < 0.01);
        assert_eq!(
            windows[0].window.reset_description.as_deref(),
            Some("daily · 剩余 250.0万")
        );
        assert_eq!(windows[1].title, "体验套餐 · GLM-5.3-Flash");
    }

    #[test]
    fn coding_plan_balances_are_not_trial_windows() {
        let body = serde_json::json!({
            "code": 0,
            "data": {
                "plans": [{
                    "plan_id": "builtin-bigmodel-coding-plan",
                    "name": "Coding Plan",
                    "status": "active"
                }],
                "balances": [{
                    "plan_id": "builtin-bigmodel-coding-plan",
                    "show_name": "GLM-5.3",
                    "total_units": 1000,
                    "remaining_units": 900,
                    "used_units": 100
                }]
            }
        });
        assert!(parse_start_plan_windows(&body).is_empty());
    }

    #[test]
    fn error_codes_and_missing_data_are_empty() {
        assert!(parse_start_plan_windows(&serde_json::json!({"code": 3012, "msg": "blocked"})).is_empty());
        assert!(parse_start_plan_windows(&serde_json::json!({"code": 0})).is_empty());
    }

    #[test]
    fn enc_v1_roundtrip_matches_zcode_cipher() {
        let secret = "zcode-credential-fallback:test";
        let iv = [7u8; 12];
        let token = "header.payload.signature";
        let enc = encrypt_enc_v1(token, secret, &iv);
        assert!(enc.starts_with(ENC_V1_PREFIX));
        assert_eq!(decrypt_enc_v1(&enc, secret).as_deref(), Ok(token));
        assert!(decrypt_enc_v1(&enc, "wrong-secret").is_err());
    }

    #[test]
    fn encrypted_blob_is_not_used_as_bearer() {
        let mut env = EnvMap::new();
        env.insert(
            JWT_ENV.to_string(),
            "enc:v1:aaaa.bbbb.cccc".to_string(),
        );
        assert!(resolve_zcode_jwt(&env).is_none());
    }

    #[test]
    fn plaintext_env_jwt_wins() {
        let mut env = EnvMap::new();
        env.insert(JWT_ENV.to_string(), "  eyJhbGciOi.plain.jwt  ".to_string());
        assert_eq!(resolve_zcode_jwt(&env).as_deref(), Some("eyJhbGciOi.plain.jwt"));
    }

    #[test]
    fn merge_keeps_titled_extra_on_informational_primary() {
        let windows = parse_start_plan_windows(&weekend_fixture());
        let usage = merge_start_plan(
            UsageSnapshot::new(RateWindow::informational("无生效套餐")).with_login_method("智谱 GLM"),
            windows,
        );
        assert!(usage.primary.is_informational);
        assert_eq!(usage.login_method.as_deref(), Some("体验套餐"));
        assert_eq!(usage.extra_rate_windows[0].title, "体验套餐 · GLM-5.3-Flash");
        assert_eq!(
            usage.extra_rate_windows[0].window.reset_description.as_deref(),
            Some("剩余 3.00亿")
        );
    }

    #[test]
    fn credentials_file_decrypts_zcodejwttoken() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("credentials.json");
        let secret = "unit-test-secret";
        let enc = encrypt_enc_v1("eyJ-test.jwt.token", secret, &[3u8; 12]);
        std::fs::write(
            &path,
            serde_json::json!({ JWT_KEY: enc }).to_string(),
        )
        .expect("write");
        let mut env = EnvMap::new();
        env.insert(CREDENTIALS_PATH_ENV.to_string(), path.to_string_lossy().into_owned());
        env.insert(SECRET_ENV.to_string(), secret.to_string());
        assert_eq!(resolve_zcode_jwt(&env).as_deref(), Some("eyJ-test.jwt.token"));
    }

    #[test]
    fn device_mid_reads_telemetry_state_next_to_credentials() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cred = dir.path().join("credentials.json");
        std::fs::write(&cred, "{}").expect("write cred");
        std::fs::write(
            dir.path().join("telemetry-state.json"),
            r#"{"deviceMid":"11111111-2222-3333-4444-555555555555"}"#,
        )
        .expect("write mid");
        let mut env = EnvMap::new();
        env.insert(
            CREDENTIALS_PATH_ENV.to_string(),
            cred.to_string_lossy().into_owned(),
        );
        assert_eq!(
            device_mid(&env).as_deref(),
            Some("11111111-2222-3333-4444-555555555555")
        );
    }
}
