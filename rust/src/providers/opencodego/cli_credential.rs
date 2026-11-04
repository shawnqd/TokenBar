//! Read-only access to the OpenCode CLI login (upstream #4771 / #4815).
//!
//! OpenCode v2 keeps credentials in the `credential` table of its SQLite
//! database; older installs keep an `opencode-go` API entry in `auth.json`.
//! Once a credential table exists anywhere it is the only source, so a stale
//! legacy file cannot revive a logged-out account — not even when another data
//! root only has `auth.json`, or when one store cannot be read. OpenCode owns
//! writes and token renewal; this reader never modifies either store and never
//! creates the database or its WAL sidecars.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::types::ValueRef;
use serde::Deserialize;

use crate::core::ProviderError;

const ACTIVE_CREDENTIAL_SQL: &str = r#"
SELECT integration_id, value FROM credential
WHERE active = 1 AND integration_id IN ('opencode-go', 'opencode')
ORDER BY CASE integration_id WHEN 'opencode-go' THEN 0 ELSE 1 END
LIMIT 2
"#;

/// The active OpenCode login as far as TokenBar may use it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CliCredential {
    pub(super) token: String,
    /// `metadata.orgID` of an OAuth login; sent as `x-opencode-org-id`.
    pub(super) org_id: Option<String>,
}

/// Where the OpenCode CLI keeps `auth.json` and its database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CredentialStore {
    pub(super) auth_path: PathBuf,
    pub(super) db_path: PathBuf,
}

#[derive(Deserialize)]
struct StoredCredential {
    #[serde(rename = "type")]
    kind: String,
    key: Option<String>,
    access: Option<String>,
    expires: Option<f64>,
    metadata: Option<StoredMetadata>,
}

#[derive(Deserialize)]
struct StoredMetadata {
    #[serde(rename = "orgID")]
    org_id: Option<String>,
}

/// Credential store candidates in lookup order, paired exactly as
/// `local::candidate_paths()` resolved them.
///
/// Nothing here rebuilds a path: channel variant databases
/// (`opencode-stable.db`, …), an `OPENCODE_DB` override, an extra data root and
/// a `:memory:` install all have to reach the reader exactly as the local usage
/// reader sees them. A pair without both halves is dropped — an empty
/// `OPENCODE_DB` is "unset", and joining it would name the data directory
/// itself, i.e. a directory is never a credential database.
fn credential_stores(candidates: Vec<(PathBuf, PathBuf)>) -> Vec<CredentialStore> {
    candidates
        .into_iter()
        .filter(|(auth, db)| !auth.as_os_str().is_empty() && !db.as_os_str().is_empty())
        .map(|(auth_path, db_path)| CredentialStore { auth_path, db_path })
        .collect()
}

/// What one candidate store contributed to the verdict.
enum StoreState {
    /// No database and no `auth.json`: nothing to read here.
    Empty,
    /// No `credential` table (or no database at all): the legacy file decides.
    Legacy(Result<Option<CliCredential>, ()>),
    /// The `credential` table exists and is authoritative for this store.
    Modern(Option<CliCredential>),
    /// A database or its `credential` table exists but could not be read.
    Unreadable,
}

/// The active OpenCode CLI login, if any. Reread on every call so a logout or
/// renewal in OpenCode takes effect on the next refresh.
///
/// All install candidates are consulted, because the CLI may have logged in
/// under a channel variant database or a different data root while an older
/// root still carries a stale `auth.json`. See `resolve_states` for the verdict
/// rules; a modern table anywhere outranks every legacy file.
pub(super) fn load_cli_credential() -> Result<Option<CliCredential>, ProviderError> {
    let stores = credential_stores(super::local::candidate_paths());
    load_from_stores(&stores, Utc::now())
}

/// Read one store on its own. Kept for single-store tests; `load_cli_credential`
/// always passes the full candidate list.
pub(super) fn load_from_store(
    store: &CredentialStore,
    now: DateTime<Utc>,
) -> Result<Option<CliCredential>, ProviderError> {
    load_from_stores(std::slice::from_ref(store), now)
}

fn load_from_stores(
    stores: &[CredentialStore],
    now: DateTime<Utc>,
) -> Result<Option<CliCredential>, ProviderError> {
    let states: Vec<StoreState> = stores.iter().map(|store| read_store(store, now)).collect();
    resolve_states(&states)
}

/// Aggregate the per-store verdicts into one login decision.
///
/// * Distinct active modern logins are ambiguous: fail closed instead of
///   guessing which account the user meant.
/// * Exactly one active modern login wins, even next to an unreadable store.
/// * A credential table without an active login means the CLI is signed out
///   (`Ok(None)`); the legacy file in any root is stale.
/// * Only installs with no credential table at all may fall back to `auth.json`,
///   and an unreadable store there fails closed rather than reviving legacy.
fn resolve_states(states: &[StoreState]) -> Result<Option<CliCredential>, ProviderError> {
    let mut active: Vec<&CliCredential> = Vec::new();
    let mut saw_credential_table = false;
    let mut saw_unreadable_store = false;
    let mut legacy: Vec<&Result<Option<CliCredential>, ()>> = Vec::new();

    for state in states {
        match state {
            StoreState::Empty => {}
            StoreState::Modern(None) => saw_credential_table = true,
            StoreState::Modern(Some(credential)) => {
                saw_credential_table = true;
                if !active.contains(&credential) {
                    active.push(credential);
                }
            }
            StoreState::Unreadable => saw_unreadable_store = true,
            StoreState::Legacy(result) => legacy.push(result),
        }
    }

    if active.len() > 1 {
        return Err(ProviderError::AuthRequired);
    }
    if let Some(credential) = active.first() {
        return Ok(Some((*credential).clone()));
    }
    if saw_credential_table {
        return Ok(None);
    }
    if saw_unreadable_store {
        return Err(ProviderError::AuthRequired);
    }
    // Candidate order: the first usable legacy login wins, a parseable file
    // that carries no `opencode-go` API entry just keeps looking.
    for result in legacy {
        match result {
            Ok(Some(credential)) => return Ok(Some(credential.clone())),
            Ok(None) => continue,
            Err(()) => return Err(ProviderError::AuthRequired),
        }
    }
    Ok(None)
}

fn read_store(store: &CredentialStore, now: DateTime<Utc>) -> StoreState {
    if !store.db_path.exists() {
        return legacy_state(&store.auth_path);
    }
    let Ok(conn) = crate::core::open_readonly_sqlite_connection(
        &store.db_path,
        crate::core::DEFAULT_SQLITE_BUSY_TIMEOUT,
    ) else {
        return StoreState::Unreadable;
    };
    match has_table(&conn, "credential") {
        // A database without the credential table is a history-only install.
        Ok(false) => return legacy_state(&store.auth_path),
        Err(()) => return StoreState::Unreadable,
        Ok(true) => {}
    }
    // The credential table is authoritative from here on: an empty, logged-out
    // or malformed table must not fall back to a stale legacy auth file.
    match active_credential(&conn, now) {
        Ok(credential) => StoreState::Modern(credential),
        Err(()) => StoreState::Unreadable,
    }
}

fn legacy_state(auth_path: &Path) -> StoreState {
    if !auth_path.exists() {
        return StoreState::Empty;
    }
    match legacy_credential(auth_path) {
        Ok(credential) => StoreState::Legacy(Ok(credential)),
        Err(()) => StoreState::Legacy(Err(())),
    }
}

fn has_table(conn: &rusqlite::Connection, name: &str) -> Result<bool, ()> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .map(|()| true)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(false),
        _ => Err(()),
    })
}

fn active_credential(
    conn: &rusqlite::Connection,
    now: DateTime<Utc>,
) -> Result<Option<CliCredential>, ()> {
    let mut statement = conn.prepare(ACTIVE_CREDENTIAL_SQL).map_err(|_| ())?;
    let mut rows = statement.query([]).map_err(|_| ())?;
    let Some(first) = rows.next().map_err(|_| ())? else {
        return Ok(None);
    };
    let integration: String = first.get(0).map_err(|_| ())?;
    let value = match first.get_ref(1).map_err(|_| ())? {
        ValueRef::Text(bytes) => bytes.to_vec(),
        _ => return Err(()),
    };
    // Two active rows for the chosen integration are ambiguous; never pick an
    // arbitrary account.
    if let Some(next) = rows.next().map_err(|_| ())? {
        let next_integration: String = next.get(0).map_err(|_| ())?;
        if next_integration == integration {
            return Err(());
        }
    }
    let credential: StoredCredential = serde_json::from_slice(&value).map_err(|_| ())?;
    // Since upstream #4815 the general `opencode` integration may authenticate
    // with either an OAuth login or an API key; only `opencode-go` rows are
    // ordered ahead of it.
    let org_id = credential.org_id();
    match credential.kind.as_str() {
        "key" => credential_token(credential.key, org_id).map(Some),
        "oauth" => {
            let expires = credential.expires.ok_or(())?;
            // `expires` is a whole number of epoch milliseconds.
            let valid = expires.is_finite()
                && expires >= 0.0
                && expires.trunc() == expires
                && expires > now.timestamp_millis() as f64;
            if !valid {
                return Err(());
            }
            credential_token(credential.access, org_id).map(Some)
        }
        _ => Err(()),
    }
}

fn legacy_credential(auth_path: &Path) -> Result<Option<CliCredential>, ()> {
    if !auth_path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(auth_path).map_err(|_| ())?;
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|_| ())?;
    let entries = value.as_object().ok_or(())?;
    let Some(entry) = entries.get("opencode-go") else {
        return Ok(None);
    };
    let entry = entry.as_object().ok_or(())?;
    if entry.get("type").and_then(serde_json::Value::as_str) != Some("api") {
        return Ok(None);
    }
    credential_token(
        entry
            .get("key")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string),
        None,
    )
    .map(Some)
}

impl StoredCredential {
    /// OAuth logins carry the organization scope of the issuing workspace.
    fn org_id(&self) -> Option<String> {
        let raw = self.metadata.as_ref()?.org_id.as_deref()?.trim();
        (!raw.is_empty() && !raw.chars().any(|c| c.is_whitespace() || c.is_control()))
            .then(|| raw.to_string())
    }
}

fn credential_token(value: Option<String>, org_id: Option<String>) -> Result<CliCredential, ()> {
    match value {
        Some(token)
            if !token.is_empty() && !token.chars().any(|c| c.is_whitespace() || c.is_control()) =>
        {
            Ok(CliCredential { token, org_id })
        }
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-26T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn future_ms() -> i64 {
        now().timestamp_millis() + 3_600_000
    }

    fn store(dir: &Path) -> CredentialStore {
        CredentialStore {
            auth_path: dir.join("auth.json"),
            db_path: dir.join("opencode.db"),
        }
    }

    fn credential_db(store: &CredentialStore, rows: &[(&str, i64, &str)]) {
        let conn = Connection::open(&store.db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE credential (id INTEGER PRIMARY KEY, integration_id TEXT NOT NULL, active INTEGER NOT NULL, value TEXT NOT NULL);",
        )
        .unwrap();
        for (integration, active, value) in rows {
            conn.execute(
                "INSERT INTO credential (integration_id, active, value) VALUES (?1, ?2, ?3)",
                rusqlite::params![integration, active, value],
            )
            .unwrap();
        }
    }

    fn legacy_auth(store: &CredentialStore, key: &str) {
        std::fs::write(
            &store.auth_path,
            format!(r#"{{"opencode-go":{{"type":"api","key":"{key}"}}}}"#),
        )
        .unwrap();
    }

    fn oauth(access: &str, expires: i64) -> String {
        format!(r#"{{"type":"oauth","access":"{access}","refresh":"r","expires":{expires}}}"#)
    }

    #[test]
    fn active_go_key_wins_over_the_general_login() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        credential_db(
            &store,
            &[
                ("opencode", 1, &oauth("general-access", future_ms())),
                ("opencode-go", 1, r#"{"type":"key","key":"go-key"}"#),
                ("opencode-go", 0, r#"{"type":"key","key":"inactive"}"#),
            ],
        );
        assert_eq!(
            load_from_store(&store, now()).unwrap(),
            Some(CliCredential {
                token: "go-key".to_string(),
                org_id: None,
            })
        );
    }

    #[test]
    fn general_integration_accepts_both_key_and_oauth() {
        let dir = tempfile::tempdir().unwrap();
        let oauth_store = store(dir.path());
        credential_db(
            &oauth_store,
            &[("opencode", 1, &oauth("general-access", future_ms()))],
        );
        assert_eq!(
            load_from_store(&oauth_store, now())
                .unwrap()
                .map(|c| c.token),
            Some("general-access".to_string())
        );

        let dir = tempfile::tempdir().unwrap();
        let key_store = self::store(dir.path());
        credential_db(
            &key_store,
            &[("opencode", 1, r#"{"type":"key","key":"general-key"}"#)],
        );
        assert_eq!(
            load_from_store(&key_store, now()).unwrap().map(|c| c.token),
            Some("general-key".to_string()),
            "upstream #4815 allows a generic API key"
        );
    }

    #[test]
    fn oauth_metadata_org_id_is_returned_and_must_be_clean() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        credential_db(
            &store,
            &[(
                "opencode-go",
                1,
                &format!(
                    r#"{{"type":"oauth","access":"go-access","expires":{},"metadata":{{"orgID":"org_abc"}}}}"#,
                    future_ms()
                ),
            )],
        );
        assert_eq!(
            load_from_store(&store, now()).unwrap(),
            Some(CliCredential {
                token: "go-access".to_string(),
                org_id: Some("org_abc".to_string()),
            })
        );
    }

    #[test]
    fn blank_or_unusable_org_ids_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        credential_db(
            &store,
            &[(
                "opencode-go",
                1,
                &format!(
                    r#"{{"type":"oauth","access":"go-access","expires":{},"metadata":{{"orgID":"   "}}}}"#,
                    future_ms()
                ),
            )],
        );
        assert_eq!(
            load_from_store(&store, now()).unwrap(),
            Some(CliCredential {
                token: "go-access".to_string(),
                org_id: None,
            })
        );
    }

    #[test]
    fn inactive_unrelated_or_empty_rows_mean_no_login() {
        let cases: [&[(&str, i64, &str)]; 3] = [
            &[("opencode-go", 0, r#"{"type":"key","key":"inactive"}"#)],
            &[("some-other", 1, r#"{"type":"key","key":"other"}"#)],
            &[],
        ];
        for rows in cases {
            let dir = tempfile::tempdir().unwrap();
            let store = store(dir.path());
            credential_db(&store, rows);
            legacy_auth(&store, "stale-legacy");
            assert_eq!(
                load_from_store(&store, now()).unwrap(),
                None,
                "a credential table never revives stale legacy auth"
            );
        }
    }

    #[test]
    fn ambiguous_expired_or_malformed_rows_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let non_text = store(dir.path());
        {
            let conn = Connection::open(&non_text.db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE credential (id INTEGER PRIMARY KEY, integration_id TEXT NOT NULL, active INTEGER NOT NULL, value BLOB NOT NULL);",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO credential (integration_id, active, value) VALUES ('opencode-go', 1, ?1)",
                rusqlite::params![b"{\"type\":\"key\",\"key\":\"blob\"}".to_vec()],
            )
            .unwrap();
        }
        legacy_auth(&non_text, "stale-legacy");
        assert!(matches!(
            load_from_store(&non_text, now()),
            Err(ProviderError::AuthRequired)
        ));

        let cases: [&[(&str, i64, &str)]; 5] = [
            &[
                ("opencode-go", 1, r#"{"type":"key","key":"one"}"#),
                ("opencode-go", 1, r#"{"type":"key","key":"two"}"#),
            ],
            &[("opencode-go", 1, "not json")],
            &[("opencode-go", 1, r#"{"type":"key","key":"has space"}"#)],
            &[(
                "opencode",
                1,
                "{\"type\":\"oauth\",\"access\":\"stale\",\"expires\":1}",
            )],
            &[("opencode-go", 1, r#"{"type":"external","key":"x"}"#)],
        ];
        for rows in cases {
            let dir = tempfile::tempdir().unwrap();
            let store = store(dir.path());
            credential_db(&store, rows);
            legacy_auth(&store, "stale-legacy");
            assert!(
                matches!(
                    load_from_store(&store, now()),
                    Err(ProviderError::AuthRequired)
                ),
                "malformed modern rows must fail closed: {rows:?}"
            );
        }
    }

    #[test]
    fn legacy_auth_is_used_only_without_a_credential_table() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        legacy_auth(&store, "legacy-key");
        assert_eq!(
            load_from_store(&store, now()).unwrap(),
            Some(CliCredential {
                token: "legacy-key".to_string(),
                org_id: None,
            })
        );

        // A history-only database without a credential table keeps the legacy file.
        Connection::open(&store.db_path)
            .unwrap()
            .execute_batch("CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT);")
            .unwrap();
        assert_eq!(
            load_from_store(&store, now()).unwrap().map(|c| c.token),
            Some("legacy-key".to_string())
        );

        // Legacy still accepts only `opencode-go` / `type: "api"`.
        std::fs::write(
            &store.auth_path,
            r#"{"opencode":{"type":"api","key":"generic-legacy"}}"#,
        )
        .unwrap();
        assert_eq!(load_from_store(&store, now()).unwrap(), None);

        std::fs::write(
            &store.auth_path,
            r#"{"opencode-go":{"type":"oauth","access":"x"}}"#,
        )
        .unwrap();
        assert_eq!(load_from_store(&store, now()).unwrap(), None);
    }

    #[test]
    fn no_store_files_means_no_login() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_from_store(&store(dir.path()), now()).unwrap(), None);
    }

    #[test]
    fn candidate_pairs_never_get_rebuilt_or_invented() {
        // `local::candidate_paths()` owns every resolution rule (channel
        // variants, `OPENCODE_DB`, extra data roots, `:memory:`); the store list
        // must be a 1:1 map of the pairs it returns, including the auth file that
        // belongs to that database.
        let dir = tempfile::tempdir().unwrap();
        let auth = dir.path().join("auth.json");
        let channel_db = dir.path().join("opencode-stable.db");
        assert_eq!(
            credential_stores(vec![(auth.clone(), channel_db.clone())]),
            vec![CredentialStore {
                auth_path: auth.clone(),
                db_path: channel_db,
            }]
        );

        // `:memory:` yields no candidates at all, so no store and no login.
        assert!(credential_stores(Vec::new()).is_empty());
        assert_eq!(load_from_stores(&[], now()).unwrap(), None);

        // An empty `OPENCODE_DB` is "unset". A joined empty path would name the
        // data directory itself, so such a pair is never a store.
        assert!(credential_stores(vec![(auth.clone(), PathBuf::new())]).is_empty());
        assert!(
            credential_stores(vec![(PathBuf::new(), dir.path().join("opencode.db"))]).is_empty()
        );
        assert_eq!(
            load_from_stores(&credential_stores(vec![(auth, PathBuf::new())]), now()).unwrap(),
            None
        );
    }

    #[test]
    fn channel_variant_database_beats_a_stale_legacy_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore {
            auth_path: dir.path().join("auth.json"),
            db_path: dir.path().join("opencode-stable.db"),
        };
        credential_db(
            &store,
            &[("opencode-go", 1, r#"{"type":"key","key":"stable-key"}"#)],
        );
        // The same directory also has a legacy login that OpenCode already
        // replaced by the modern one.
        legacy_auth(&store, "stale-legacy");

        assert_eq!(
            load_from_stores(std::slice::from_ref(&store), now()).unwrap(),
            Some(CliCredential {
                token: "stable-key".to_string(),
                org_id: None,
            })
        );
    }

    #[test]
    fn modern_login_in_a_later_root_beats_an_earlier_legacy_file() {
        let legacy_dir = tempfile::tempdir().unwrap();
        let legacy = store(legacy_dir.path());
        legacy_auth(&legacy, "legacy-key");

        let modern_dir = tempfile::tempdir().unwrap();
        let modern = store(modern_dir.path());
        credential_db(
            &modern,
            &[("opencode-go", 1, &oauth("modern-access", future_ms()))],
        );

        let expected = Some(CliCredential {
            token: "modern-access".to_string(),
            org_id: None,
        });
        assert_eq!(
            load_from_stores(&[legacy.clone(), modern.clone()], now()).unwrap(),
            expected,
            "a stale legacy file in the first root must not mask a modern login"
        );
        assert_eq!(
            load_from_stores(&[modern, legacy], now()).unwrap(),
            expected,
            "root order must not change the verdict"
        );
    }

    #[test]
    fn two_different_modern_logins_are_ambiguous() {
        let first_dir = tempfile::tempdir().unwrap();
        let first = store(first_dir.path());
        credential_db(
            &first,
            &[("opencode-go", 1, r#"{"type":"key","key":"first-key"}"#)],
        );
        let second_dir = tempfile::tempdir().unwrap();
        let second = store(second_dir.path());
        credential_db(
            &second,
            &[("opencode-go", 1, r#"{"type":"key","key":"second-key"}"#)],
        );

        assert!(
            matches!(
                load_from_stores(&[first.clone(), second], now()),
                Err(ProviderError::AuthRequired)
            ),
            "two active accounts must not be resolved by picking one"
        );

        // The same account mirrored in a second install is not an ambiguity.
        let mirror_dir = tempfile::tempdir().unwrap();
        let mirror = store(mirror_dir.path());
        credential_db(
            &mirror,
            &[("opencode-go", 1, r#"{"type":"key","key":"first-key"}"#)],
        );
        assert_eq!(
            load_from_stores(&[first, mirror], now()).unwrap(),
            Some(CliCredential {
                token: "first-key".to_string(),
                org_id: None,
            })
        );
    }

    #[test]
    fn logged_out_modern_store_beats_another_roots_legacy_file() {
        let modern_dir = tempfile::tempdir().unwrap();
        let modern = store(modern_dir.path());
        credential_db(
            &modern,
            &[("opencode-go", 0, r#"{"type":"key","key":"signed-out"}"#)],
        );

        let legacy_dir = tempfile::tempdir().unwrap();
        let legacy = store(legacy_dir.path());
        legacy_auth(&legacy, "legacy-key");

        assert_eq!(
            load_from_stores(&[modern, legacy], now()).unwrap(),
            None,
            "a credential table says signed out; legacy auth is stale"
        );
    }

    #[test]
    fn unreadable_store_fails_closed_even_with_a_readable_legacy_file() {
        let broken_dir = tempfile::tempdir().unwrap();
        let broken = store(broken_dir.path());
        std::fs::write(&broken.db_path, b"not a sqlite database").unwrap();

        let legacy_dir = tempfile::tempdir().unwrap();
        let legacy = store(legacy_dir.path());
        legacy_auth(&legacy, "legacy-key");

        assert!(
            matches!(
                load_from_stores(&[broken, legacy], now()),
                Err(ProviderError::AuthRequired)
            ),
            "an unreadable modern store must not revive legacy auth"
        );
    }

    #[test]
    fn reading_never_creates_sidecars_or_modifies_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        credential_db(
            &store,
            &[("opencode-go", 1, r#"{"type":"key","key":"go"}"#)],
        );
        let before = std::fs::read(&store.db_path).unwrap();
        assert!(
            load_from_store(&store, now()).unwrap().is_some(),
            "synthetic Go key should be readable"
        );
        assert!(!crate::core::sqlite_sidecar_path(&store.db_path, "-wal").exists());
        assert!(!crate::core::sqlite_sidecar_path(&store.db_path, "-shm").exists());
        assert_eq!(
            std::fs::read(&store.db_path).unwrap(),
            before,
            "read-only credential access must not write the database"
        );
    }

    #[test]
    fn missing_database_is_never_created() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path());
        assert_eq!(load_from_store(&store, now()).unwrap(), None);
        assert!(
            !store.db_path.exists(),
            "reader must not create the database"
        );
    }
}
