use super::*;

// ── Browser cookie import commands ────────────────────────────────────

const MAX_COOKIE_FILE_LEN: usize = 2 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieFileProviderBridge {
    pub provider_id: String,
    pub provider: String,
    pub cookie_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieFilePreviewBridge {
    pub format: String,
    pub providers: Vec<CookieFileProviderBridge>,
    pub unmatched_cookie_count: usize,
}

#[derive(Debug, Clone)]
struct ImportedCookie {
    domain: String,
    name: String,
    value: String,
}

/// Preview a user-exported Cookie file without persisting it. Supported formats
/// are Netscape cookies.txt and JSON exports containing domain/name/value fields.
#[tauri::command]
pub fn preview_cookie_file(contents: String) -> Result<CookieFilePreviewBridge, String> {
    let (format, cookies) = parse_cookie_file(&contents)?;
    let grouped = group_cookies_by_provider(&cookies);
    let matched = grouped.values().map(Vec::len).sum::<usize>();
    Ok(CookieFilePreviewBridge {
        format,
        providers: preview_providers(&grouped),
        unmatched_cookie_count: cookies.len().saturating_sub(matched),
    })
}

/// Persist selected provider cookies from a user-exported Cookie file. The
/// source file is never written to disk; only provider-specific headers are
/// stored in the existing protected manual-cookie store.
#[tauri::command]
pub fn import_cookie_file(
    contents: String,
    provider_ids: Vec<String>,
) -> Result<Vec<CookieInfoBridge>, String> {
    let (_, cookies) = parse_cookie_file(&contents)?;
    let grouped = group_cookies_by_provider(&cookies);
    let selected = provider_ids
        .iter()
        .map(|id| parse_provider_arg(id))
        .collect::<Result<Vec<_>, _>>()?;
    if selected.is_empty() {
        return Err("Select at least one detected provider".to_string());
    }

    let mut manual = ManualCookies::load();
    let mut imported = 0usize;
    for provider in selected {
        let Some(cookies) = grouped.get(&provider) else {
            continue;
        };
        let header = cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name, cookie.value))
            .collect::<Vec<_>>()
            .join("; ");
        validate_single_line_secret(&header, "Cookie header", MAX_COOKIE_HEADER_LEN)?;
        manual.set(provider.cli_name(), &header);
        imported += 1;
    }
    if imported == 0 {
        return Err("The selected providers have no matching cookies in this file".to_string());
    }
    manual.save().map_err(|error| error.to_string())?;
    Ok(get_manual_cookies())
}

fn parse_cookie_file(contents: &str) -> Result<(String, Vec<ImportedCookie>), String> {
    if contents.len() > MAX_COOKIE_FILE_LEN {
        return Err("Cookie file is too large".to_string());
    }
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return Err("Cookie file is empty".to_string());
    }
    if trimmed.starts_with('{') || trimmed.starts_with('[') {
        return parse_json_cookie_file(trimmed).map(|cookies| ("JSON".to_string(), cookies));
    }
    parse_netscape_cookie_file(trimmed).map(|cookies| ("Netscape cookies.txt".to_string(), cookies))
}

fn parse_netscape_cookie_file(contents: &str) -> Result<Vec<ImportedCookie>, String> {
    let mut cookies = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || (line.starts_with('#') && !line.starts_with("#HttpOnly_")) {
            continue;
        }
        let line = line.strip_prefix("#HttpOnly_").unwrap_or(line);
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 7 {
            continue;
        }
        push_cookie(&mut cookies, fields[0], fields[5], fields[6]);
    }
    if cookies.is_empty() {
        return Err(
            "No valid cookies found. Use a Netscape cookies.txt or Cookie JSON export.".to_string(),
        );
    }
    Ok(cookies)
}

fn parse_json_cookie_file(contents: &str) -> Result<Vec<ImportedCookie>, String> {
    let value: serde_json::Value =
        serde_json::from_str(contents).map_err(|_| "Invalid Cookie JSON export".to_string())?;
    let entries = value
        .as_array()
        .or_else(|| value.get("cookies").and_then(serde_json::Value::as_array))
        .ok_or_else(|| "Cookie JSON must be an array or contain a cookies array".to_string())?;
    let mut cookies = Vec::new();
    for entry in entries {
        let domain = entry
            .get("domain")
            .or_else(|| entry.get("host"))
            .or_else(|| entry.get("host_key"));
        let name = entry.get("name");
        let cookie_value = entry.get("value");
        if let (Some(domain), Some(name), Some(cookie_value)) = (
            domain.and_then(serde_json::Value::as_str),
            name.and_then(serde_json::Value::as_str),
            cookie_value.and_then(serde_json::Value::as_str),
        ) {
            push_cookie(&mut cookies, domain, name, cookie_value);
        }
    }
    if cookies.is_empty() {
        return Err("No valid cookies found in the JSON export".to_string());
    }
    Ok(cookies)
}

fn push_cookie(cookies: &mut Vec<ImportedCookie>, domain: &str, name: &str, value: &str) {
    let domain = domain.trim().trim_start_matches('.').to_ascii_lowercase();
    if domain.is_empty()
        || name.trim().is_empty()
        || name.contains(['\r', '\n', ';', '='])
        || value.contains(['\r', '\n'])
    {
        return;
    }
    cookies.push(ImportedCookie {
        domain,
        name: name.trim().to_string(),
        value: value.to_string(),
    });
}

fn group_cookies_by_provider(
    cookies: &[ImportedCookie],
) -> HashMap<codexbar::core::ProviderId, Vec<ImportedCookie>> {
    let settings = Settings::load();
    let mut grouped = HashMap::new();
    for provider in codexbar::core::ProviderId::all() {
        let Some(domain) = super::provider_cookie_domain(*provider, &settings) else {
            continue;
        };
        let matches = dedupe_exact_cookies(
            cookies
                .iter()
                .filter(|cookie| provider_domain_matches(*provider, &cookie.domain, domain))
                .cloned()
                .collect::<Vec<_>>(),
        );
        if !matches.is_empty() {
            grouped.insert(*provider, matches);
        }
    }
    grouped
}

/// Cookie exporters can emit the same domain/name/value tuple more than once
/// (for example when Chrome partition metadata is flattened into Netscape
/// format). Sending those exact duplicates produces a malformed-looking Cookie
/// header without adding any authentication data. Preserve genuinely distinct
/// path/domain values while removing only byte-for-byte duplicates.
fn dedupe_exact_cookies(cookies: Vec<ImportedCookie>) -> Vec<ImportedCookie> {
    let mut seen = std::collections::HashSet::new();
    cookies
        .into_iter()
        .filter(|cookie| {
            seen.insert((
                cookie.domain.clone(),
                cookie.name.clone(),
                cookie.value.clone(),
            ))
        })
        .collect()
}

/// Claude's browser session can legitimately span its public app domains.
/// The web fetcher already tries these domains, so file import must retain the
/// matching cookies instead of silently dropping the companion session keys.
fn provider_domain_matches(
    provider: codexbar::core::ProviderId,
    cookie_domain: &str,
    provider_domain: &str,
) -> bool {
    if provider == codexbar::core::ProviderId::Claude {
        return [
            "claude.ai",
            "claude.com",
            "console.anthropic.com",
            "anthropic.com",
        ]
        .iter()
        .any(|domain| domain_matches(cookie_domain, domain));
    }
    domain_matches(cookie_domain, provider_domain)
}

fn domain_matches(cookie_domain: &str, provider_domain: &str) -> bool {
    cookie_domain == provider_domain
        || cookie_domain.ends_with(&format!(".{provider_domain}"))
        || provider_domain.ends_with(&format!(".{cookie_domain}"))
}

fn preview_providers(
    grouped: &HashMap<codexbar::core::ProviderId, Vec<ImportedCookie>>,
) -> Vec<CookieFileProviderBridge> {
    let mut providers = grouped
        .iter()
        .map(|(provider, cookies)| CookieFileProviderBridge {
            provider_id: provider.cli_name().to_string(),
            provider: provider.display_name().to_string(),
            cookie_count: cookies.len(),
        })
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| left.provider.cmp(&right.provider));
    providers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_netscape_export_without_retaining_comment_lines() {
        let (_, cookies) = parse_cookie_file(
            "# Netscape HTTP Cookie File\n.chatgpt.com\tTRUE\t/\tTRUE\t0\tsession\tvalue\n",
        )
        .unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].domain, "chatgpt.com");
        assert_eq!(cookies[0].name, "session");
    }

    #[test]
    fn parses_json_export_and_rejects_multiline_values() {
        let (_, cookies) = parse_cookie_file(
            r#"[{"domain":".claude.ai","name":"session","value":"ok"},{"domain":"claude.ai","name":"bad","value":"line\nnext"}]"#,
        )
        .unwrap();
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].domain, "claude.ai");
    }

    #[test]
    fn domain_matching_keeps_cookies_within_the_provider_domain() {
        assert!(domain_matches("auth.chatgpt.com", "chatgpt.com"));
        assert!(!domain_matches("evilchatgpt.com", "chatgpt.com"));
    }

    #[test]
    fn file_import_removes_only_exact_duplicate_cookies() {
        let cookies = vec![
            ImportedCookie {
                domain: "platform.xiaomimimo.com".into(),
                name: "api-platform_serviceToken".into(),
                value: "current".into(),
            },
            ImportedCookie {
                domain: "platform.xiaomimimo.com".into(),
                name: "api-platform_serviceToken".into(),
                value: "current".into(),
            },
            ImportedCookie {
                domain: "platform.xiaomimimo.com".into(),
                name: "api-platform_serviceToken".into(),
                value: "different".into(),
            },
        ];

        let deduped = dedupe_exact_cookies(cookies);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].value, "current");
        assert_eq!(deduped[1].value, "different");
    }

    #[test]
    fn claude_import_keeps_companion_claude_domains() {
        assert!(provider_domain_matches(
            codexbar::core::ProviderId::Claude,
            "claude.com",
            "claude.ai"
        ));
        assert!(provider_domain_matches(
            codexbar::core::ProviderId::Claude,
            "console.anthropic.com",
            "claude.ai"
        ));
        assert!(!provider_domain_matches(
            codexbar::core::ProviderId::Claude,
            "example.com",
            "claude.ai"
        ));
    }
}
