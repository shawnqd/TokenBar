//! Core data models and traits

mod codex_routed_pricing;
mod cost_pricing;
mod credential_migration;
mod credentials;
mod http;
mod jsonl_scanner;
mod models_dev_pricing;

/// User-facing app version. Debug builds (the dev chain always compiles the
/// debug profile) carry a `-dev` suffix so a development window or CLI can
/// never be mistaken for the installed stable build; release builds report
/// the bare package version.
pub fn display_version() -> &'static str {
    static DISPLAY_VERSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
        let base = env!("CARGO_PKG_VERSION");
        if cfg!(debug_assertions) {
            format!("{base}-dev")
        } else {
            base.to_string()
        }
    });
    DISPLAY_VERSION.as_str()
}

/// Window/tooltip title for the current build: the shipped title, plus a
/// ` (dev)` marker in debug builds so a development window is visually
/// distinguishable from the installed stable app.
pub fn dev_window_title(base: &str) -> String {
    if cfg!(debug_assertions) {
        format!("{base} (dev)")
    } else {
        base.to_string()
    }
}

/// Refresh the model price catalog if the cached copy has aged out.
///
/// Re-exported because the desktop shell has to be able to trigger it at
/// startup — the module itself stays private so nothing else can reach into
/// the cache format.
pub use models_dev_pricing::refresh_if_stale as refresh_pricing_if_stale;
mod openai_dashboard;
mod provider;
mod provider_factory;
mod rate_window;
mod redactor;
mod session_quota;
mod sqlite;
mod token_accounts;
mod usage_pace;
mod usage_snapshot;
mod widget_snapshot;

pub use cost_pricing::*;
pub use credential_migration::*;
pub use credentials::*;
pub use http::*;
pub use jsonl_scanner::*;
pub use openai_dashboard::*;
pub use provider::*;
pub use provider_factory::instantiate as instantiate_provider;
pub use rate_window::*;
pub use redactor::*;
pub use session_quota::*;
pub use sqlite::*;
pub use token_accounts::*;
pub use usage_pace::*;
pub use usage_snapshot::*;
pub use widget_snapshot::*;
