//! Core data models and traits

mod cost_pricing;
mod credential_migration;
mod credentials;
mod http;
mod jsonl_scanner;
mod models_dev_pricing;

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
pub use token_accounts::*;
pub use usage_pace::*;
pub use usage_snapshot::*;
pub use widget_snapshot::*;
