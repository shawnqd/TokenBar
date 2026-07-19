//! HTTP helpers shared by provider fetchers.

use reqwest::Url;

/// Build a client for requests that may carry cookies, OAuth tokens, API keys,
/// or other provider credentials.
///
/// Credentialed provider requests should not automatically follow redirects to
/// a different origin. Reqwest strips some sensitive headers during redirects,
/// but an explicit same-origin policy keeps the invariant local and testable.
pub fn credentialed_http_client_builder() -> reqwest::ClientBuilder {
    let builder =
        reqwest::Client::builder().redirect(reqwest::redirect::Policy::custom(|attempt| {
            let previous = attempt.previous();
            let Some(last_url) = previous.last() else {
                return attempt.follow();
            };

            if is_same_origin_redirect(last_url, attempt.url()) {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }));

    // Reqwest observes HTTP(S)_PROXY, but it does not read WinINET's per-user
    // system proxy settings. Desktop browsers do, which otherwise produces the
    // confusing state where ChatGPT works in a browser while this tray app
    // times out. Use the HTTPS entry (or a single proxy value) when Windows
    // has an HTTP proxy enabled. Invalid or unsupported values are ignored so
    // provider requests retain their normal direct-connection behavior.
    #[cfg(windows)]
    if let Some(proxy_url) = windows_system_proxy_url()
        && let Ok(proxy) = reqwest::Proxy::all(proxy_url)
    {
        return builder.proxy(proxy);
    }

    builder
}

#[cfg(windows)]
fn windows_system_proxy_url() -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let settings = hkcu
        .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Internet Settings")
        .ok()?;
    let enabled = settings.get_value::<u32, _>("ProxyEnable").ok()? == 1;
    if !enabled {
        return None;
    }
    let raw = settings.get_value::<String, _>("ProxyServer").ok()?;
    normalize_windows_proxy_url(&raw)
}

fn normalize_windows_proxy_url(raw: &str) -> Option<String> {
    // WinINET permits a single endpoint (`127.0.0.1:7890`) or protocol-specific
    // entries (`http=...;https=...`). Provider calls are HTTPS, so prefer that.
    let raw = raw.trim();
    let chosen = raw
        .split(';')
        .filter_map(|entry| {
            let entry = entry.trim();
            let (scheme, value) = entry.split_once('=')?;
            Some((scheme.trim().to_ascii_lowercase(), value.trim()))
        })
        .find_map(|(scheme, value)| (scheme == "https").then_some(value))
        .or_else(|| {
            raw.split(';')
                .find_map(|entry| entry.trim().split_once('=').map(|(_, value)| value.trim()))
        })
        .unwrap_or(raw);

    if chosen.is_empty() {
        return None;
    }
    if chosen.starts_with("http://") || chosen.starts_with("https://") {
        return Some(chosen.to_string());
    }
    // SOCKS requires a reqwest feature this app deliberately does not enable.
    if chosen.starts_with("socks") {
        return None;
    }
    Some(format!("http://{chosen}"))
}

fn is_same_origin_redirect(from: &Url, to: &Url) -> bool {
    from.scheme() == to.scheme()
        && from.host_str() == to.host_str()
        && from.port_or_known_default() == to.port_or_known_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(input: &str) -> Url {
        Url::parse(input).unwrap()
    }

    #[test]
    fn same_origin_redirect_allows_path_changes() {
        assert!(is_same_origin_redirect(
            &url("https://example.com/a"),
            &url("https://example.com/b?x=1"),
        ));
    }

    #[test]
    fn same_origin_redirect_rejects_host_changes() {
        assert!(!is_same_origin_redirect(
            &url("https://example.com/a"),
            &url("https://evil.example/b"),
        ));
    }

    #[test]
    fn same_origin_redirect_rejects_scheme_changes() {
        assert!(!is_same_origin_redirect(
            &url("https://example.com/a"),
            &url("http://example.com/b"),
        ));
    }

    #[test]
    fn normalizes_common_windows_proxy_formats() {
        assert_eq!(
            normalize_windows_proxy_url("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            normalize_windows_proxy_url("http=127.0.0.1:7890;https=127.0.0.1:7891").as_deref(),
            Some("http://127.0.0.1:7891")
        );
        assert_eq!(
            normalize_windows_proxy_url("https://proxy.example:8443").as_deref(),
            Some("https://proxy.example:8443")
        );
    }
}
