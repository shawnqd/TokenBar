//! Config command implementation
//!
//! Utilities for validating and inspecting configuration.

use clap::{Parser, Subcommand};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::Path;

use crate::core::{ProviderId, TokenAccountStore, instantiate_provider};
use crate::secure_file::{self, SecureFileStatus};
use crate::settings::{ApiKeys, ManualCookies, Settings};

/// Arguments for the config command
#[derive(Parser, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCommand {
    /// Validate configuration files
    Validate,
    /// Dump configuration to stdout
    Dump {
        /// Output format: json or toml
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Include secret-bearing fields as `<redacted>` placeholders (never raw values)
        #[arg(long)]
        debug: bool,
    },
    /// List providers and enabled state
    Providers,
    /// Enable a provider
    Enable {
        /// Provider CLI name or alias
        provider: String,
    },
    /// Disable a provider
    Disable {
        /// Provider CLI name or alias
        provider: String,
    },
    /// Store an API key for a provider
    SetApiKey {
        /// Provider CLI name or alias
        provider: String,
        /// API key to store
        #[arg(long = "api-key")]
        api_key: Option<String>,
        /// Read API key from stdin
        #[arg(long)]
        stdin: bool,
        /// Store the key without enabling the provider
        #[arg(long = "no-enable")]
        no_enable: bool,
    },
    /// Show configuration file paths
    Path,
}

/// Run the config command
pub async fn run(args: ConfigArgs) -> anyhow::Result<()> {
    match args.command {
        ConfigCommand::Validate => validate_config().await,
        ConfigCommand::Dump { format, debug } => dump_config(&format, debug).await,
        ConfigCommand::Providers => list_providers().await,
        ConfigCommand::Enable { provider } => set_provider_enabled(&provider, true).await,
        ConfigCommand::Disable { provider } => set_provider_enabled(&provider, false).await,
        ConfigCommand::SetApiKey {
            provider,
            api_key,
            stdin,
            no_enable,
        } => set_api_key(&provider, api_key.as_deref(), stdin, !no_enable).await,
        ConfigCommand::Path => show_paths().await,
    }
}

/// Validate configuration files
async fn validate_config() -> anyhow::Result<()> {
    let mut report = ValidationReport::default();
    validate_settings_config(&mut report);
    validate_api_keys_config(&mut report);
    validate_manual_cookies_config(&mut report);
    validate_token_accounts_config(&mut report);
    report.finish()
}

#[derive(Default)]
struct ValidationReport {
    errors: Vec<String>,
    warnings: Vec<String>,
}

impl ValidationReport {
    fn error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }

    fn warning(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    fn finish(self) -> anyhow::Result<()> {
        print_validation_summary(&self.errors, &self.warnings)
    }
}

fn validate_settings_config(report: &mut ValidationReport) {
    print!("Checking settings.json... ");
    let Some(path) = Settings::settings_path() else {
        println!("ERROR");
        report.error("settings.json: Could not determine config path");
        return;
    };

    if validate_optional_json_file::<Settings>(&path, "settings.json", report) {
        return;
    }

    println!("NOT FOUND (using defaults)");
    report.warning("settings.json: File does not exist, using defaults");
}

fn validate_manual_cookies_config(report: &mut ValidationReport) {
    print!("Checking manual_cookies.json... ");
    let Some(path) = ManualCookies::cookies_path() else {
        println!("SKIP");
        return;
    };

    if !validate_optional_json_file::<ManualCookies>(&path, "manual_cookies.json", report) {
        println!("NOT FOUND (none configured)");
    }
}

fn validate_api_keys_config(report: &mut ValidationReport) {
    print!("Checking api_keys.json... ");
    let Some(path) = ApiKeys::keys_path() else {
        println!("SKIP");
        return;
    };

    if !validate_optional_json_file::<ApiKeys>(&path, "api_keys.json", report) {
        println!("NOT FOUND (none configured)");
    }
}

fn validate_optional_json_file<T>(path: &Path, label: &str, report: &mut ValidationReport) -> bool
where
    T: DeserializeOwned,
{
    let storage_status = secure_file::status(path);
    if matches!(storage_status, SecureFileStatus::Missing) {
        return false;
    }

    if let SecureFileStatus::Unreadable(error) = &storage_status {
        println!("INVALID");
        report.error(format!("{label}: {error}"));
        return true;
    }

    let storage_label = secure_file_status_label(&storage_status);

    match read_json_config::<T>(path) {
        Ok(()) => {
            println!("OK ({storage_label})");
            warn_about_plaintext_storage(label, &storage_status, report);
        }
        Err(ConfigFileError::Read(e)) => {
            println!("ERROR");
            report.error(format!("{label}: Could not read file: {e}"));
        }
        Err(ConfigFileError::Parse(e)) => {
            println!("INVALID");
            report.error(format!("{label}: {e}"));
        }
    }

    true
}

fn read_json_config<T>(path: &Path) -> Result<(), ConfigFileError>
where
    T: DeserializeOwned,
{
    let content = secure_file::read_string(path).map_err(ConfigFileError::Read)?;
    serde_json::from_str::<T>(&content)
        .map(|_| ())
        .map_err(ConfigFileError::Parse)
}

fn secure_file_status_label(status: &SecureFileStatus) -> String {
    match status {
        SecureFileStatus::Missing => "missing".to_string(),
        SecureFileStatus::Plaintext => "plaintext".to_string(),
        SecureFileStatus::Protected(scope) => format!("protected:{scope}"),
        SecureFileStatus::Unreadable(error) => format!("unreadable:{error}"),
    }
}

fn warn_about_plaintext_storage(
    label: &str,
    status: &SecureFileStatus,
    report: &mut ValidationReport,
) {
    if cfg!(windows) && matches!(status, SecureFileStatus::Plaintext) {
        report.warning(format!(
            "{label}: file is plaintext; save it again to protect it with Windows DPAPI"
        ));
    }
}

enum ConfigFileError {
    Read(std::io::Error),
    Parse(serde_json::Error),
}

fn validate_token_accounts_config(report: &mut ValidationReport) {
    print!("Checking token-accounts.json... ");
    let path = TokenAccountStore::default_path();
    let storage_status = secure_file::status(&path);
    match &storage_status {
        SecureFileStatus::Missing => {
            println!("NOT FOUND (none configured)");
            return;
        }
        SecureFileStatus::Unreadable(error) => {
            println!("INVALID");
            report.error(format!("token-accounts.json: {error}"));
            return;
        }
        SecureFileStatus::Plaintext | SecureFileStatus::Protected(_) => {}
    }

    match TokenAccountStore::new().load() {
        Ok(_) => {
            println!("OK ({})", secure_file_status_label(&storage_status));
            warn_about_plaintext_storage("token-accounts.json", &storage_status, report);
        }
        Err(e) => {
            println!("INVALID");
            report.error(format!("token-accounts.json: {e}"));
        }
    }
}

fn print_validation_summary(errors: &[String], warnings: &[String]) -> anyhow::Result<()> {
    println!();
    if errors.is_empty() && warnings.is_empty() {
        println!("Configuration is valid.");
    } else {
        if !warnings.is_empty() {
            println!("Warnings:");
            for w in warnings {
                println!("  - {}", w);
            }
        }
        if !errors.is_empty() {
            println!("Errors:");
            for e in errors {
                println!("  - {}", e);
            }
            anyhow::bail!(
                "Configuration validation failed with {} error(s).",
                errors.len()
            );
        }
    }

    Ok(())
}

/// Dump configuration to stdout
async fn dump_config(format: &str, debug: bool) -> anyhow::Result<()> {
    let settings = load_settings_for_dump()?;
    let mut value = serde_json::to_value(&settings)?;
    redact_config_value(
        &mut value,
        if debug {
            DumpRedaction::Placeholder
        } else {
            DumpRedaction::Omit
        },
    );

    match format.to_lowercase().as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&value)?;
            println!("{}", json);
        }
        "toml" => {
            let toml_value = json_to_toml_value(&value)?;
            println!("{}", toml::to_string_pretty(&toml_value)?);
        }
        _ => {
            anyhow::bail!("Unknown format '{}'. Supported formats: json, toml", format);
        }
    }

    Ok(())
}

fn load_settings_for_dump() -> anyhow::Result<Settings> {
    let Some(path) = Settings::settings_path() else {
        return Ok(Settings::default());
    };
    if !path.exists() {
        return Ok(Settings::default());
    }

    let content = secure_file::read_string(&path)
        .map_err(|error| anyhow::anyhow!("settings.json: could not read file: {error}"))?;
    serde_json::from_str(content.trim_start_matches('\u{feff}'))
        .map_err(|error| anyhow::anyhow!("settings.json: invalid JSON: {error}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DumpRedaction {
    Omit,
    Placeholder,
}

const REDACTED_VALUE: &str = "<redacted>";

/// Remove secret-bearing fields from a serialized settings tree.
///
/// This is deliberately key-based rather than value-based: a cookie or token
/// must not reach stdout merely because a new provider stores it under a
/// different object shape. Debug mode preserves the shape while exposing only
/// a fixed placeholder, never the original value.
fn redact_config_value(value: &mut Value, mode: DumpRedaction) {
    match value {
        Value::Object(object) => {
            let keys = object.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                if is_sensitive_config_key(&key) {
                    if mode == DumpRedaction::Placeholder {
                        object.insert(key, Value::String(REDACTED_VALUE.to_string()));
                    } else {
                        object.remove(&key);
                    }
                } else if let Some(child) = object.get_mut(&key) {
                    redact_config_value(child, mode);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact_config_value(value, mode);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn is_sensitive_config_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase().replace('-', "_");
    matches!(
        normalized.as_str(),
        "api_key"
            | "api_token"
            | "token"
            | "access_token"
            | "refresh_token"
            | "cookie"
            | "cookie_header"
            | "manual_cookie"
            | "manual_cookie_header"
            | "authorization"
            | "client_secret"
            | "secret"
            | "secret_key"
            | "password"
            | "session_key"
            | "session_token"
            | "private_key"
    ) || normalized.contains("cookie")
        || normalized.contains("secret")
        || normalized.ends_with("_token")
}

fn json_to_toml_value(value: &Value) -> anyhow::Result<toml::Value> {
    match value {
        Value::Null => {
            anyhow::bail!("Configuration contains a null value that TOML cannot represent")
        }
        Value::Bool(value) => Ok(toml::Value::Boolean(*value)),
        Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(toml::Value::Integer(value))
            } else if let Some(value) = value.as_u64() {
                let value = i64::try_from(value)
                    .map_err(|_| anyhow::anyhow!("Configuration integer is too large for TOML"))?;
                Ok(toml::Value::Integer(value))
            } else if let Some(value) = value.as_f64() {
                Ok(toml::Value::Float(value))
            } else {
                anyhow::bail!("Configuration contains an unsupported number")
            }
        }
        Value::String(value) => Ok(toml::Value::String(value.clone())),
        Value::Array(values) => Ok(toml::Value::Array(
            values
                .iter()
                .map(json_to_toml_value)
                .collect::<anyhow::Result<Vec<_>>>()?,
        )),
        Value::Object(values) => Ok(toml::Value::Table(
            values
                .iter()
                .map(|(key, value)| Ok((key.clone(), json_to_toml_value(value)?)))
                .collect::<anyhow::Result<toml::map::Map<_, _>>>()?,
        )),
    }
}

/// List provider enabled state.
async fn list_providers() -> anyhow::Result<()> {
    let settings = Settings::load();
    for id in ProviderId::all() {
        let state = if settings.is_provider_enabled(*id) {
            "enabled"
        } else {
            "disabled"
        };
        let default_marker = if instantiate_provider(*id).metadata().default_enabled {
            " default"
        } else {
            ""
        };
        println!(
            "{}: {}{} ({})",
            id.cli_name(),
            state,
            default_marker,
            id.display_name()
        );
    }
    Ok(())
}

/// Enable or disable a provider by CLI name.
async fn set_provider_enabled(provider: &str, enabled: bool) -> anyhow::Result<()> {
    let id = parse_provider(provider)?;
    let mut settings = Settings::load();
    if enabled {
        settings.enable_provider(id);
    } else {
        settings.disable_provider(id);
    }
    settings.save()?;
    let state = if enabled { "enabled" } else { "disabled" };
    println!("Config: {state} {}", id.display_name());
    Ok(())
}

/// Store an API key and optionally enable the provider.
async fn set_api_key(
    provider: &str,
    api_key: Option<&str>,
    read_from_stdin: bool,
    enable_provider: bool,
) -> anyhow::Result<()> {
    let id = parse_provider(provider)?;
    ensure_provider_accepts_api_key(id)?;
    let api_key = resolve_api_key_input(api_key, read_from_stdin)?;

    let mut keys = ApiKeys::load();
    keys.set(id.cli_name(), &api_key, None);
    keys.save()?;

    if enable_provider {
        let mut settings = Settings::load();
        settings.enable_provider(id);
        settings.save()?;
    }

    let suffix = if enable_provider { " and enabled" } else { "" };
    println!("Config: stored API key for {}{suffix}", id.display_name());
    Ok(())
}

fn parse_provider(raw: &str) -> anyhow::Result<ProviderId> {
    ProviderId::from_cli_name(raw).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown provider '{}'. Run `codexbar config providers` to list providers.",
            raw
        )
    })
}

fn ensure_provider_accepts_api_key(id: ProviderId) -> anyhow::Result<()> {
    if crate::settings::get_api_key_providers()
        .iter()
        .any(|provider| provider.id == id)
    {
        return Ok(());
    }
    anyhow::bail!("{} does not support stored API keys.", id.display_name())
}

fn resolve_api_key_input(api_key: Option<&str>, read_from_stdin: bool) -> anyhow::Result<String> {
    if api_key.is_some() && read_from_stdin {
        anyhow::bail!("Use either --api-key or --stdin, not both.");
    }

    let raw = if read_from_stdin {
        let mut buffer = String::new();
        use std::io::Read;
        std::io::stdin().read_to_string(&mut buffer)?;
        Some(buffer)
    } else {
        api_key.map(ToString::to_string)
    };

    let mut value = raw
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("Missing API key. Pass --api-key <key> or use --stdin."))?;

    if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value.remove(0);
        value.pop();
    }

    let value = value.trim().to_string();
    if value.is_empty() {
        anyhow::bail!("Missing API key. Pass --api-key <key> or use --stdin.");
    }
    Ok(value)
}

/// Show configuration file paths
async fn show_paths() -> anyhow::Result<()> {
    println!("Configuration paths:");

    if let Some(path) = Settings::settings_path() {
        let exists = if path.exists() { "" } else { " (not found)" };
        println!("  Settings:       {}{}", path.display(), exists);
    } else {
        println!("  Settings:       (could not determine path)");
    }

    if let Some(path) = ManualCookies::cookies_path() {
        let exists = if path.exists() { "" } else { " (not found)" };
        println!("  Manual cookies: {}{}", path.display(), exists);
    } else {
        println!("  Manual cookies: (could not determine path)");
    }

    if let Some(path) = ApiKeys::keys_path() {
        let exists = if path.exists() { "" } else { " (not found)" };
        println!("  API keys:       {}{}", path.display(), exists);
    } else {
        println!("  API keys:       (could not determine path)");
    }

    let token_path = TokenAccountStore::default_path();
    let exists = if token_path.exists() {
        ""
    } else {
        " (not found)"
    };
    println!("  Token accounts: {}{}", token_path.display(), exists);

    // Show config directory
    if let Some(config_dir) = dirs::config_dir() {
        let codexbar_dir = config_dir.join("CodexBar");
        println!();
        println!("Config directory: {}", codexbar_dir.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ProviderConfig;

    fn settings_with_secrets() -> Settings {
        let mut settings = Settings::default();
        settings.provider_configs.insert(
            ProviderId::Claude,
            ProviderConfig {
                manual_cookie_header: Some("sessionKey=do-not-print".to_string()),
                api_token: Some("api-token-do-not-print".to_string()),
                ..ProviderConfig::default()
            },
        );
        settings
    }

    #[test]
    fn default_dump_omits_secret_fields_and_values() {
        let mut value = serde_json::to_value(settings_with_secrets()).unwrap();
        redact_config_value(&mut value, DumpRedaction::Omit);
        let text = value.to_string();

        assert!(!text.contains("do-not-print"));
        assert!(!text.contains("api_token"));
        assert!(!text.contains("manual_cookie_header"));
    }

    #[test]
    fn debug_dump_only_contains_fixed_redaction_placeholder() {
        let mut value = serde_json::to_value(settings_with_secrets()).unwrap();
        redact_config_value(&mut value, DumpRedaction::Placeholder);
        let text = value.to_string();

        assert!(!text.contains("do-not-print"));
        assert_eq!(
            value["provider_configs"]["claude"]["api_token"],
            Value::String(REDACTED_VALUE.to_string())
        );
        assert_eq!(
            value["provider_configs"]["claude"]["manual_cookie_header"],
            Value::String(REDACTED_VALUE.to_string())
        );
    }

    #[test]
    fn toml_dump_preserves_redaction_without_null_values() {
        let mut value = serde_json::to_value(settings_with_secrets()).unwrap();
        redact_config_value(&mut value, DumpRedaction::Placeholder);
        let toml = toml::to_string_pretty(&json_to_toml_value(&value).unwrap()).unwrap();

        assert!(toml.contains("<redacted>"));
        assert!(!toml.contains("do-not-print"));
    }

    #[test]
    fn validation_reads_plaintext_json_through_secure_file_layer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"refresh_interval_secs":60}"#).unwrap();
        let mut report = ValidationReport::default();

        assert!(validate_optional_json_file::<Settings>(
            &path,
            "settings.json",
            &mut report
        ));
        assert!(report.errors.is_empty());
    }

    #[test]
    fn validation_reports_corrupt_secure_wrapper() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"format":"codexbar.secure-file","version":1,"protection":"windows-dpapi-user","payload":"not-base64"}"#,
        )
        .unwrap();
        let mut report = ValidationReport::default();

        assert!(validate_optional_json_file::<Settings>(
            &path,
            "settings.json",
            &mut report
        ));
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].contains("settings.json"));
        assert!(!report.errors[0].contains("not-base64"));
    }

    #[test]
    fn validation_returns_false_for_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.json");
        let mut report = ValidationReport::default();

        assert!(!validate_optional_json_file::<Settings>(
            &path,
            "settings.json",
            &mut report
        ));
        assert!(report.errors.is_empty());
    }
}
