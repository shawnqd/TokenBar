//! Cookie Header Cache
//!
//! Caches cookie headers per site (primary cookie domain) so browser-session
//! providers keep working while the browser holds an exclusive lock on its
//! own cookie database (Chrome active profiles). The cached value is only an
//! auth credential: fetches made with it still return live server data.
//! Entries are stored through the DPAPI-backed secure-file layer — a cached
//! cookie is a session credential and must never hit disk in plaintext.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// How long a cached cookie header may stand in for a live extraction.
/// Session cookies typically outlive this by far; the bound exists so a
/// server-revoked session cannot be served forever after a lock failure.
pub const COOKIE_CACHE_TTL_SECS: i64 = 24 * 60 * 60;

/// Test-only cache directory override (see [`CookieHeaderCache::cache_path`]).
#[cfg(test)]
static CACHE_DIR_OVERRIDE: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

#[cfg(test)]
pub(crate) fn set_cache_dir_for_tests(dir: PathBuf) {
    *CACHE_DIR_OVERRIDE.write().expect("cache dir override lock") = Some(dir);
}

#[cfg(test)]
fn clear_cache_dir_for_tests() {
    *CACHE_DIR_OVERRIDE.write().expect("cache dir override lock") = None;
}

/// Cached cookie header entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieHeaderEntry {
    /// The normalized cookie header string
    pub cookie_header: String,
    /// When this entry was stored
    pub stored_at: DateTime<Utc>,
    /// Source of the cookie (e.g., "Chrome", "Edge", "Manual")
    pub source_label: String,
}

impl CookieHeaderEntry {
    pub fn new(cookie_header: impl Into<String>, source_label: impl Into<String>) -> Self {
        Self {
            cookie_header: cookie_header.into(),
            stored_at: Utc::now(),
            source_label: source_label.into(),
        }
    }

    /// Check if the entry is stale (older than max_age_secs)
    pub fn is_stale(&self, max_age_secs: i64) -> bool {
        let age = Utc::now().signed_duration_since(self.stored_at);
        age.num_seconds() > max_age_secs
    }
}

/// Cookie header cache store
pub struct CookieHeaderCache;

impl CookieHeaderCache {
    /// Load cached cookie header for a site (primary cookie domain)
    pub fn load(site: &str) -> Option<CookieHeaderEntry> {
        let path = Self::cache_path(site)?;
        let content = crate::secure_file::read_string(&path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Store a cookie header for a site (primary cookie domain)
    pub fn store(
        site: &str,
        cookie_header: &str,
        source_label: &str,
    ) -> Result<(), CookieHeaderCacheError> {
        let trimmed = cookie_header.trim();

        // Normalize the cookie header
        let normalized = Self::normalize_cookie_header(trimmed);

        if normalized.is_empty() {
            // Clear the cache if the normalized header is empty
            Self::clear(site);
            return Ok(());
        }

        let entry = CookieHeaderEntry::new(normalized, source_label);
        let path = Self::cache_path(site).ok_or(CookieHeaderCacheError::PathNotAvailable)?;

        // Create parent directory
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&entry)?;
        crate::secure_file::write_string(&path, &json)?;

        tracing::debug!(
            site = %site,
            source = source_label,
            "Stored cookie header to cache"
        );

        Ok(())
    }

    /// Clear cached cookie header for a site
    pub fn clear(site: &str) {
        if let Some(path) = Self::cache_path(site)
            && let Err(e) = fs::remove_file(&path)
            && e.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(
                site = %site,
                error = %e,
                "Failed to remove cookie cache"
            );
        }
    }

    /// Get the cache file path for a site. Domains are filename-safe after
    /// mangling the characters filesystems dislike.
    fn cache_path(site: &str) -> Option<PathBuf> {
        let site = site.trim().to_ascii_lowercase();
        if site.is_empty() {
            return None;
        }
        let safe: String = site
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        // Tests redirect the cache dir via the in-process override: env vars
        // are unsafe to mutate in Rust 2024 and racy across parallel tests.
        #[cfg(test)]
        if let Some(dir) = CACHE_DIR_OVERRIDE
            .read()
            .expect("cache dir override lock")
            .clone()
        {
            return Some(dir.join(format!("{safe}-cookie.json")));
        }
        dirs::data_local_dir().map(|d| d.join("CodexBar").join(format!("{safe}-cookie.json")))
    }

    /// Normalize a cookie header string
    fn normalize_cookie_header(header: &str) -> String {
        // Remove duplicate cookies, normalize whitespace, and sort for consistency
        let mut cookies: Vec<(&str, &str)> = Vec::new();

        for part in header.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }

            if let Some((name, value)) = part.split_once('=') {
                let name = name.trim();
                let value = value.trim();

                // Only keep the last occurrence of each cookie name
                cookies.retain(|(n, _)| *n != name);
                cookies.push((name, value));
            }
        }

        // Sort by cookie name for consistency
        cookies.sort_by(|a, b| a.0.cmp(b.0));

        // Rebuild the header
        cookies
            .into_iter()
            .map(|(name, value)| format!("{}={}", name, value))
            .collect::<Vec<_>>()
            .join("; ")
    }

    /// Check if a cached entry exists and is fresh
    pub fn has_fresh_cache(site: &str, max_age_secs: i64) -> bool {
        match Self::load(site) {
            Some(entry) => !entry.is_stale(max_age_secs),
            None => false,
        }
    }
}

/// Cookie header cache errors
#[derive(Debug, thiserror::Error)]
pub enum CookieHeaderCacheError {
    #[error("Cache path not available")]
    PathNotAvailable,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_cookie_header() {
        let header = "foo=bar; baz=qux; foo=updated";
        let normalized = CookieHeaderCache::normalize_cookie_header(header);
        // foo should be updated (last occurrence), sorted alphabetically
        assert!(normalized.contains("baz=qux"));
        assert!(normalized.contains("foo=updated"));
        assert!(!normalized.contains("foo=bar"));
    }

    #[test]
    fn test_entry_staleness() {
        let entry = CookieHeaderEntry::new("foo=bar", "test");
        assert!(!entry.is_stale(60)); // Not stale within 60 seconds

        // We can't easily test staleness without mocking time
    }

    #[test]
    fn test_empty_normalization() {
        let empty = CookieHeaderCache::normalize_cookie_header("");
        assert!(empty.is_empty());

        let whitespace = CookieHeaderCache::normalize_cookie_header("   ;  ;  ");
        assert!(whitespace.is_empty());
    }

    // The cache stores session credentials: roundtrip must go through the
    // DPAPI-backed secure-file layer, keyed per site, isolated per test via
    // the dir override.
    #[test]
    fn cache_roundtrip_is_site_scoped_and_secure() {
        let dir =
            std::env::temp_dir().join(format!("codexbar-cookie-cache-test-{}", std::process::id()));
        set_cache_dir_for_tests(dir.clone());

        CookieHeaderCache::store("platform.xiaomimimo.com", "b=2; a=1", "browser")
            .expect("store should succeed");
        let entry = CookieHeaderCache::load("platform.xiaomimimo.com").expect("cache entry loads");
        // Normalized (sorted) header survives the roundtrip.
        assert_eq!(entry.cookie_header, "a=1; b=2");
        assert_eq!(entry.source_label, "browser");
        assert!(!entry.is_stale(COOKIE_CACHE_TTL_SECS));

        // Sites do not share entries.
        assert!(CookieHeaderCache::load("grok.com").is_none());

        // The stored file must be protected, never plaintext.
        let raw = std::fs::read_to_string(dir.join("platform.xiaomimimo.com-cookie.json"))
            .expect("cache file exists");
        assert!(
            raw.contains("codexbar.secure-file") && raw.contains("windows-dpapi-user"),
            "cache file must use the DPAPI secure-file envelope"
        );
        assert!(
            !raw.contains("a=1"),
            "the cookie payload must not be readable plaintext"
        );

        // Clearing removes only the requested site.
        CookieHeaderCache::clear("platform.xiaomimimo.com");
        assert!(CookieHeaderCache::load("platform.xiaomimimo.com").is_none());

        clear_cache_dir_for_tests();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
