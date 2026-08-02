//! Sign in to a provider inside the app instead of exporting cookies by hand.
//!
//! Windows already extracts browser cookies automatically (`browser::cookies`
//! decrypts Chrome's store through DPAPI), but Chrome 127+ moved the key behind
//! App-Bound Encryption and that path now fails for most users — which is why a
//! manual Cookie paste became the only way to read a web-only quota.
//!
//! This module sidesteps the problem rather than attacking it. The session is
//! established in a webview *we own*, so its cookie store is ours to read; no
//! other application's encryption is involved. What comes out is stored in the
//! same protected `ManualCookies` file the paste box writes to, so every
//! provider fetcher picks it up with no per-provider wiring.

use super::*;
use tauri::{Url, WebviewUrl, WebviewWindowBuilder};

use browser_import::{ImportedCookie, cookie_header_from, dedupe_exact_cookies, push_cookie};

/// The one login window. Reused across providers so a forgotten window can
/// never quietly hold a second provider's session.
const LOGIN_WINDOW_LABEL: &str = "provider-login";

const LOGIN_WINDOW_WIDTH: f64 = 1000.0;
const LOGIN_WINDOW_HEIGHT: f64 = 760.0;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderLoginTargetBridge {
    pub provider_id: String,
    pub provider: String,
    pub url: String,
}

/// Resolve the page the login window should open for `id`.
///
/// The destination is derived from the provider's own cookie domain — the same
/// domain the capture step filters on — so the window can only ever be pointed
/// at the site whose cookies we are about to read. Regional providers resolve
/// through `provider_cookie_domain`, so a `cn` account does not get sent to the
/// international host.
fn login_target(id: ProviderId, settings: &Settings) -> Result<(String, Url), String> {
    let domain = provider_cookie_domain(id, settings).ok_or_else(|| {
        format!(
            "{} does not sign in with a web session",
            id.display_name()
        )
    })?;
    let url = Url::parse(&format!("https://{domain}/"))
        .map_err(|error| format!("Could not build a login URL for {domain}: {error}"))?;
    Ok((domain.to_string(), url))
}

#[tauri::command]
pub fn get_provider_login_target(
    provider_id: String,
) -> Result<ProviderLoginTargetBridge, String> {
    let id = parse_provider_arg(&provider_id)?;
    let settings = Settings::load();
    let (_, url) = login_target(id, &settings)?;
    Ok(ProviderLoginTargetBridge {
        provider_id: id.cli_name().to_string(),
        provider: id.display_name().to_string(),
        url: url.to_string(),
    })
}

/// Open the provider's own sign-in page in a plain browser window.
///
/// The window carries no capability grants (the label is absent from
/// `capabilities/default.json`), so the remote page has no IPC and cannot reach
/// a single one of our commands.
#[tauri::command]
pub fn open_provider_login(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<ProviderLoginTargetBridge, String> {
    let id = parse_provider_arg(&provider_id)?;
    let settings = Settings::load();
    let (_, url) = login_target(id, &settings)?;

    if let Some(existing) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        existing.navigate(url.clone()).map_err(|e| e.to_string())?;
        existing.show().map_err(|e| e.to_string())?;
        existing.set_focus().map_err(|e| e.to_string())?;
    } else {
        WebviewWindowBuilder::new(
            &app,
            LOGIN_WINDOW_LABEL,
            WebviewUrl::External(url.clone()),
        )
        .title(format!("{} — Sign in", id.display_name()))
        .inner_size(LOGIN_WINDOW_WIDTH, LOGIN_WINDOW_HEIGHT)
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    }

    Ok(ProviderLoginTargetBridge {
        provider_id: id.cli_name().to_string(),
        provider: id.display_name().to_string(),
        url: url.to_string(),
    })
}

/// Read the session the user just established and store it for `provider_id`.
///
/// Async on purpose: reading the WebView2 cookie store blocks on the event
/// loop, so a synchronous command would deadlock the whole app (see the
/// platform note on `WebviewWindow::cookies`).
#[tauri::command]
pub async fn capture_provider_login(
    app: tauri::AppHandle,
    provider_id: String,
) -> Result<Vec<CookieInfoBridge>, String> {
    let id = parse_provider_arg(&provider_id)?;
    let settings = Settings::load();
    let (domain, _) = login_target(id, &settings)?;

    let window = app
        .get_webview_window(LOGIN_WINDOW_LABEL)
        .ok_or_else(|| "The login window is not open".to_string())?;

    let cookies = window.cookies().map_err(|error| {
        format!("Could not read the login window's session: {error}")
    })?;

    let header = provider_cookie_header(id, &domain, &cookies)?;
    validate_single_line_secret(&header, "Cookie header", MAX_COOKIE_HEADER_LEN)?;

    let mut manual = ManualCookies::load();
    manual.set(id.cli_name(), &header);
    manual.save().map_err(|error| error.to_string())?;

    let _ = window.close();

    Ok(get_manual_cookies())
}

#[tauri::command]
pub fn close_provider_login(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LOGIN_WINDOW_LABEL) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Keep only the cookies that belong to `id` and fold them into one header.
///
/// The webview store holds every site the user visited in that window, so the
/// domain filter is what stops one provider's login from walking off with
/// another site's session. It is the same filter the file importer uses.
fn provider_cookie_header(
    id: ProviderId,
    domain: &str,
    cookies: &[tauri::webview::Cookie<'static>],
) -> Result<String, String> {
    let mut collected = Vec::new();
    for cookie in cookies {
        let Some(cookie_domain) = cookie.domain() else {
            continue;
        };
        push_cookie(&mut collected, cookie_domain, cookie.name(), cookie.value());
    }

    let matching = dedupe_exact_cookies(
        collected
            .into_iter()
            .filter(|cookie: &ImportedCookie| {
                provider_domain_matches(id, &cookie.domain, domain)
            })
            .collect::<Vec<_>>(),
    );

    if matching.is_empty() {
        return Err(format!(
            "No {} session found yet. Finish signing in on {domain}, then try again.",
            id.display_name()
        ));
    }

    Ok(cookie_header_from(&matching))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cookie(domain: &str, name: &str, value: &str) -> tauri::webview::Cookie<'static> {
        tauri::webview::Cookie::build((name.to_string(), value.to_string()))
            .domain(domain.to_string())
            .build()
    }

    #[test]
    fn login_opens_the_domain_whose_cookies_are_captured() {
        let settings = Settings::default();
        let (domain, url) = login_target(ProviderId::Cursor, &settings).unwrap();

        assert_eq!(domain, "cursor.com");
        assert_eq!(url.as_str(), "https://cursor.com/");
    }

    /// MiniMax and Alibaba serve different hosts per region. Deriving the login
    /// page from the same resolver the capture filter uses keeps a `cn` account
    /// from being sent to sign in on a host whose cookies would then be
    /// discarded.
    #[test]
    fn a_regional_provider_signs_in_on_its_own_host() {
        let mut settings = Settings::default();
        settings.set_api_region(ProviderId::MiniMax, "cn");

        let (domain, url) = login_target(ProviderId::MiniMax, &settings).unwrap();

        assert_eq!(domain, "platform.minimaxi.com");
        assert_eq!(url.as_str(), "https://platform.minimaxi.com/");
    }

    #[test]
    fn a_provider_without_a_web_session_has_nowhere_to_sign_in() {
        let settings = Settings::default();
        assert!(login_target(ProviderId::Copilot, &settings).is_err());
    }

    #[test]
    fn capture_keeps_only_the_signed_in_provider_domain() {
        let cookies = vec![
            cookie("cursor.com", "WorkosCursorSessionToken", "wanted"),
            cookie(".cursor.com", "prefs", "also-wanted"),
            cookie("accounts.google.com", "SID", "someone-elses"),
        ];

        let header = provider_cookie_header(ProviderId::Cursor, "cursor.com", &cookies).unwrap();

        assert_eq!(header, "WorkosCursorSessionToken=wanted; prefs=also-wanted");
    }

    /// Claude's session legitimately spans its companion domains, and the web
    /// fetcher tries all of them. Capture has to keep them for the same reason
    /// the file importer does.
    #[test]
    fn capture_keeps_claudes_companion_domains() {
        let cookies = vec![
            cookie("claude.ai", "sessionKey", "session"),
            cookie("claude.com", "lastActiveOrg", "org"),
        ];

        let header = provider_cookie_header(ProviderId::Claude, "claude.ai", &cookies).unwrap();

        assert_eq!(header, "sessionKey=session; lastActiveOrg=org");
    }

    #[test]
    fn capture_before_signing_in_says_so_instead_of_storing_nothing() {
        let cookies = vec![cookie("accounts.google.com", "SID", "someone-elses")];

        let error =
            provider_cookie_header(ProviderId::Cursor, "cursor.com", &cookies).unwrap_err();

        assert!(error.contains("Finish signing in"));
    }
}
