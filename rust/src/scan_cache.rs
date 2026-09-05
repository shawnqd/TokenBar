//! Incremental per-file cache for Codex local-cost scans.
//!
//! The Codex scan is I/O-bound: resumed sessions keep appending to jsonl files
//! filed under their original start-date folder, so on active machines the
//! 30-day working set alone reaches gigabytes and one full scan costs minutes.
//! Every consumer cache is shorter than that (interactive scan budget 3s,
//! local-usage TTL 30s, chart TTL 5min), so Codex local usage read as
//! permanently absent even though the data exists.
//!
//! Session jsonl files only ever APPEND. The cache stores the exact ordered
//! per-turn stream plus lineage metadata; cost and speed buckets are priced
//! from the model string at replay time, never stored. That makes an entry
//! keyed by the file's stamp exact for unchanged files, while preserving the
//! sequence needed to deduplicate fork replays.
//!
//! Invariant contract: a cached turn stream is valid for a file exactly while
//! its content is a pure append of what was parsed (the Codex CLI's write
//! pattern). The stamp includes sub-second modification precision so an
//! in-place rewrite of equal length cannot silently reuse old usage. If a
//! writer changes the append-only contract, bump the cache layout version
//! and force a full re-scan.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

/// One Codex usage turn in file order: the exact per-event sequence a fork
/// file shares (as a prefix) with its ancestors, which is what lineage
/// deduplication aligns against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CachedTurn {
    pub day: String,
    pub model: String,
    pub input: u64,
    pub cached: u64,
    pub output: u64,
}

/// Cache layout version combined with the parser semantics version: either
/// changing invalidates every persisted entry, so a parser fix can never be
/// silently masked by buckets produced under the old parsing rules.
const VERSION: u32 =
    CACHE_LAYOUT_VERSION * 1000 + crate::core::CODEX_PARSE_SEMANTICS_VERSION;

/// Bump when the persisted shape changes (fields, keying, bucket schema).
const CACHE_LAYOUT_VERSION: u32 = 4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexFileCache {
    pub mtime: u64,
    pub len: u64,
    /// Thread-root id (forked_from/parent/own session id).
    pub thread: Option<String>,
    /// File creation timestamp string from session_meta.
    pub created: Option<String>,
    /// Full per-turn sequence in file order (lineage dedup input).
    pub turns: Vec<CachedTurn>,
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedCache {
    version: u32,
    files: BTreeMap<String, CodexFileCache>,
}

fn cache_path() -> Option<PathBuf> {
    // Tests must never touch the real per-user cache under %APPDATA%: they
    // clear and overwrite entries, which would wipe the production warm-up.
    #[cfg(test)]
    {
        return Some(test_cache_dir().join("codex-cost-scan-cache.json"));
    }
    #[allow(unreachable_code)]
    {
        crate::settings::Settings::settings_path()?
            .parent()
            .map(|parent| parent.join("codex-cost-scan-cache.json"))
    }
}

/// Test-only redirect target, kept alive for the process lifetime.
#[cfg(test)]
fn test_cache_dir() -> PathBuf {
    static DIR: OnceLock<Mutex<Option<tempfile::TempDir>>> = OnceLock::new();
    let mut guard = DIR
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        *guard = Some(tempfile::tempdir().expect("temp dir for scan cache tests"));
    }
    guard
        .as_ref()
        .expect("temp dir initialized")
        .path()
        .to_path_buf()
}

fn cache_slot() -> &'static Mutex<Option<PersistedCache>> {
    static SLOT: OnceLock<Mutex<Option<PersistedCache>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn loaded() -> std::sync::MutexGuard<'static, Option<PersistedCache>> {
    let mut guard = cache_slot().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        *guard = Some(load_from_disk());
    }
    guard
}

fn load_from_disk() -> PersistedCache {
    let Some(path) = cache_path() else {
        return PersistedCache::default();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return PersistedCache::default();
    };
    match serde_json::from_slice::<PersistedCache>(&bytes) {
        Ok(cache) if cache.version == VERSION => {
            #[cfg(test)]
            eprintln!("[scan-cache-debug] loaded {} entries", cache.files.len());
            cache
        }
        Ok(cache) => {
            eprintln!(
                "[scan-cache-debug] version mismatch: file={} expected={}",
                cache.version, VERSION
            );
            PersistedCache::default()
        }
        Err(error) => {
            eprintln!("[scan-cache-debug] DESERIALIZE FAILED: {error}");
            PersistedCache::default()
        }
    }
}

/// Stamp one file as (unix mtime nanoseconds, byte length), None when unreadable.
pub fn stamp(path: &Path) -> Option<(u64, u64)> {
    let metadata = std::fs::metadata(path).ok()?;
    let mtime = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos()
        .min(u64::MAX as u128) as u64;
    Some((mtime, metadata.len()))
}

/// Cached turns for one file, valid only while (mtime, len) are unchanged.
pub fn lookup(path: &Path, mtime: u64, len: u64) -> Option<CodexFileCache> {
    let key = path.to_string_lossy().to_string();
    let guard = loaded();
    let cache = guard.as_ref()?;
    let entry = cache.files.get(&key)?;
    if entry.mtime == mtime && entry.len == len {
        Some(entry.clone())
    } else {
        None
    }
}

/// Store (or overwrite) one file's turns in the in-memory cache. Callers
/// persist once at the end of a scan — `store` must not write here: a full
/// walk stores hundreds of files and rewriting the whole cache per file is
/// quadratic disk I/O.
pub fn store(entry: CodexFileCache, path: &Path) {
    let key = path.to_string_lossy().to_string();
    let mut guard = loaded();
    let cache = guard.as_mut().expect("cache loaded");
    cache.version = VERSION;
    cache.files.insert(key, entry);
}

/// Write the cache to disk (best effort; a failed write only costs a re-parse).
pub fn persist() {
    let Some(path) = cache_path() else {
        return;
    };
    let guard = loaded();
    let cache = guard.as_ref().expect("cache loaded");
    let Ok(bytes) = serde_json::to_vec(cache) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let temp = path.with_extension("json.tmp");
    if std::fs::write(&temp, bytes).is_ok() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::rename(&temp, &path);
    }
}

#[cfg(test)]
pub(crate) fn clear_for_test() {
    *loaded() = Some(PersistedCache::default());
}

/// Serialize tests that touch the process-wide cache slot and its backing
/// file (same pattern as the chart-cache tests); parallel tests would
/// otherwise reset each other's in-memory state mid-assertion.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(day: &str, input: u64) -> CachedTurn {
        CachedTurn {
            day: day.to_string(),
            model: "gpt-test".to_string(),
            input,
            cached: 0,
            output: 0,
        }
    }

    #[test]
    fn lookup_requires_matching_stamp() {
        let _guard = test_guard();
        clear_for_test();
        let entry = CodexFileCache {
            mtime: 100,
            len: 500,
            thread: Some("root".to_string()),
            created: None,
            turns: vec![turn("2026-08-30", 10)],
        };
        store(entry.clone(), Path::new("\\\\mock\\session-a.jsonl"));

        let hit = lookup(Path::new("\\\\mock\\session-a.jsonl"), 100, 500);
        assert_eq!(hit.map(|e| e.turns), Some(entry.turns));
        // Any stamp change must miss so the file gets re-parsed.
        assert!(lookup(Path::new("\\\\mock\\session-a.jsonl"), 101, 500).is_none());
        assert!(lookup(Path::new("\\\\mock\\session-a.jsonl"), 100, 501).is_none());
        assert!(lookup(Path::new("\\\\mock\\other.jsonl"), 100, 500).is_none());
    }

    #[test]
    fn persist_survives_reload_from_disk() {
        let _guard = test_guard();
        clear_for_test();
        let path = Path::new("\\\\mock\\session-b.jsonl");
        let entry = CodexFileCache {
            mtime: 42,
            len: 900,
            thread: Some("root".to_string()),
            created: None,
            turns: vec![turn("2026-08-29", 7)],
        };
        store(entry.clone(), path);
        persist();

        // Force the next accessor to reload from the file it just wrote.
        *cache_slot().lock().unwrap() = None;
        assert_eq!(
            lookup(path, 42, 900).map(|e| e.turns),
            Some(entry.turns),
            "persisted entry must survive a reload when the cache file exists"
        );
    }
}
