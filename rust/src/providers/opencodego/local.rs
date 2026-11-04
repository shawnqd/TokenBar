//! OpenCode Go local usage reader (SQLite).
//!
//! Mirrors upstream `OpenCodeGoLocalUsageReader`: sums `opencode-go` assistant
//! message / step-finish costs from the local OpenCode database and maps them
//! onto session ($12 / 5h), weekly ($30), and monthly ($60) windows.
//!
//! Dedupe contract (v1 `message`/`part` vs v2 `session_message`): a local
//! install can hold both schemas for the same turn, so v1 keeps priority per
//! **id** and all three passes (cost, tokens, output speed) use the same
//! `v1_usable` id set, derived from the actual v1 cost rows. An id v1 owns is
//! never read from v2 for *any* metric — when the v1 row lacks tokens or
//! timing, that metric stays empty rather than mixing schemas within one turn.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone, Timelike, Utc};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

use crate::core::{ProviderError, ProviderFetchResult, RateWindow, UsageSnapshot};

const FIVE_HOURS_MS: i64 = 5 * 60 * 60 * 1000;
const WEEK_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const SESSION_LIMIT_USD: f64 = 12.0;
const WEEKLY_LIMIT_USD: f64 = 30.0;
const MONTHLY_LIMIT_USD: f64 = 60.0;
const PROVIDER_ID: &str = "opencode-go";

// ---- v1 (`message` / `part`) -------------------------------------------------

/// v1 usage rows from `message` alone (databases without a `part` table).
const V1_MESSAGE_USAGE_SQL: &str = r#"
SELECT
  id AS id,
  CAST(COALESCE(json_extract(data, '$.time.created'), time_created) AS INTEGER) AS createdMs,
  CAST(json_extract(data, '$.cost') AS REAL) AS cost,
  1 AS requestCount,
  COALESCE(json_extract(data, '$.modelID'), '') AS modelID
FROM message
WHERE json_valid(data)
  AND json_extract(data, '$.providerID') = ?1
  AND json_extract(data, '$.role') = 'assistant'
  AND json_type(data, '$.cost') IN ('integer', 'real')
"#;

/// v1 usage rows for databases that also carry a `part` table. A message with
/// priced `step-finish` parts is counted once per part; otherwise the message
/// cost is used. Row ids are the message id, so a v2 copy of the same turn can
/// be anti-joined away.
const V1_PARTS_USAGE_CTE: &str = r#"
provider_messages AS (
  SELECT
    id AS messageID,
    CAST(COALESCE(json_extract(data, '$.time.created'), time_created) AS INTEGER) AS createdMs,
    CAST(json_extract(data, '$.cost') AS REAL) AS cost,
    json_type(data, '$.cost') IN ('integer', 'real') AS hasCost,
    COALESCE(json_extract(data, '$.modelID'), '') AS modelID
  FROM message
  WHERE json_valid(data)
    AND json_extract(data, '$.providerID') = ?1
    AND json_extract(data, '$.role') = 'assistant'
),
v1_cost AS (
  SELECT
    m.messageID AS id,
    CAST(COALESCE(json_extract(p.data, '$.time.created'), p.time_created, m.createdMs) AS INTEGER)
      AS createdMs,
    CAST(json_extract(p.data, '$.cost') AS REAL) AS cost,
    1 AS requestCount,
    m.modelID AS modelID
  FROM part p
  JOIN provider_messages m ON m.messageID = p.message_id
  WHERE json_valid(p.data)
    AND json_extract(p.data, '$.type') = 'step-finish'
    AND json_type(p.data, '$.cost') IN ('integer', 'real')
  UNION ALL
  SELECT m.messageID AS id, m.createdMs AS createdMs, m.cost AS cost, 1 AS requestCount,
         m.modelID AS modelID
  FROM provider_messages m
  WHERE m.hasCost
    AND NOT EXISTS (
      SELECT 1
      FROM part p
      WHERE p.message_id = m.messageID
        AND json_valid(p.data)
        AND json_extract(p.data, '$.type') = 'step-finish'
        AND json_type(p.data, '$.cost') IN ('integer', 'real')
    )
)"#;

/// Empty stand-in used when a v1 table is absent, so a single parameterised
/// query can always be prepared (`?1` stays bound).
const EMPTY_V1_USAGE_SQL: &str =
    "SELECT NULL AS id, 0 AS createdMs, 0.0 AS cost, 0 AS requestCount, '' AS modelID WHERE 0";

/// `v1_cost` CTE: the v1 rows the cost pass actually reads.
///
/// Parts-priced rows when the `part` table exists (that is the legacy
/// "step-finish wins over message cost" rule), the message cost alone otherwise.
/// Emitting this as a named CTE lets the `v1_usable` suppression set be derived
/// from the same rows instead of re-deriving the rule in a second query.
fn v1_cost_cte(has_message: bool, has_part: bool) -> String {
    if has_message && has_part {
        return V1_PARTS_USAGE_CTE.trim_end().to_string();
    }
    let mut cte = String::from("v1_cost AS (");
    cte.push_str(if has_message {
        V1_MESSAGE_USAGE_SQL
    } else {
        EMPTY_V1_USAGE_SQL
    });
    cte.push(')');
    cte
}

/// `v1_usable` CTE: the ids whose **v1** rows are authoritative for a turn.
///
/// Derived from the actual `v1_cost` rows (`createdMs > 0`, numeric cost
/// `>= 0`) so the suppression set and the emitted v1 cost rows cannot diverge.
/// A turn whose v1 cost resolves to `0` through a priced `step-finish` part
/// still owns its id (legacy priority: v2 must not take over), while a turn
/// whose v1 cost row is negative is not owned at all, so the v2 copy keeps its
/// cost/tokens/speed instead of the turn being dropped.
///
/// Legacy-first rule, shared by all three passes (cost, tokens, speed): an id
/// present in both schemas is read from v1 only. Deriving the suppression set
/// per metric made one turn contribute a v1 cost beside v2 tokens/speed, and a
/// v1 row with tokens but no usable cost silently handed that id's *cost* to
/// v2 while keeping its tokens. One id therefore gets one source per metric —
/// and since the set is a whole id, a v1 row that lacks tokens or speed fields
/// yields "no data" for that metric instead of borrowing the v2 copy.
fn v1_usable_cte() -> &'static str {
    "v1_usable AS (SELECT DISTINCT id FROM v1_cost WHERE createdMs > 0 AND cost >= 0)"
}

// ---- v2 (`session_message` / `session_part`) ---------------------------------

/// v2 usage rows: the message kind lives in the `type` column (not in the JSON
/// payload), `compaction` turns are billable like `assistant` ones, and the
/// provider/model sit under `data.model`.
const V2_USAGE_SQL: &str = r#"
  SELECT
    s.id AS id,
    CAST(COALESCE(json_extract(s.data, '$.time.created'), s.time_created) AS INTEGER) AS createdMs,
    CAST(json_extract(s.data, '$.cost') AS REAL) AS cost,
    1 AS requestCount,
    COALESCE(json_extract(s.data, '$.model.id'), '') AS modelID
  FROM session_message s
  WHERE json_valid(s.data)
    AND s.type IN ('assistant', 'compaction')
    AND json_extract(s.data, '$.model.providerID') = ?1
    AND json_type(s.data, '$.cost') IN ('integer', 'real')
"#;

/// Token totals live on the message level only — step-finish parts carry cost
/// without token counts — so the token pass reads message rows directly.
const V1_MESSAGE_TOKENS_SQL: &str = r#"
SELECT
  CAST(COALESCE(json_extract(data, '$.time.created'), time_created) AS INTEGER) AS createdMs,
  COALESCE(json_extract(data, '$.modelID'), '') AS modelID,
  CAST(COALESCE(json_extract(data, '$.tokens.input'), 0) AS INTEGER) AS inputTokens,
  CAST(COALESCE(json_extract(data, '$.tokens.output'), 0) AS INTEGER) AS outputTokens,
  CAST(COALESCE(json_extract(data, '$.tokens.cache.read'), 0) AS INTEGER) AS cacheRead,
  CAST(COALESCE(json_extract(data, '$.tokens.cache.write'), 0) AS INTEGER) AS cacheWrite,
  id AS id
FROM message
WHERE json_valid(data)
  AND json_extract(data, '$.providerID') = ?1
  AND json_extract(data, '$.role') = 'assistant'
  AND json_type(data, '$.tokens') = 'object'
"#;

const V2_TOKENS_SQL: &str = r#"
  SELECT
    CAST(COALESCE(json_extract(s.data, '$.time.created'), s.time_created) AS INTEGER) AS createdMs,
    COALESCE(json_extract(s.data, '$.model.id'), '') AS modelID,
    CAST(COALESCE(json_extract(s.data, '$.tokens.input'), 0) AS INTEGER) AS inputTokens,
    CAST(COALESCE(json_extract(s.data, '$.tokens.output'), 0) AS INTEGER) AS outputTokens,
    CAST(COALESCE(json_extract(s.data, '$.tokens.cache.read'), 0) AS INTEGER) AS cacheRead,
    CAST(COALESCE(json_extract(s.data, '$.tokens.cache.write'), 0) AS INTEGER) AS cacheWrite,
    s.id AS id
  FROM session_message s
  WHERE json_valid(s.data)
    AND s.type IN ('assistant', 'compaction')
    AND json_extract(s.data, '$.model.providerID') = ?1
    AND json_type(s.data, '$.tokens') = 'object'
"#;

const EMPTY_V1_TOKENS_SQL: &str = "SELECT 0 AS createdMs, '' AS modelID, 0 AS inputTokens, \
     0 AS outputTokens, 0 AS cacheRead, 0 AS cacheWrite, NULL AS id WHERE 0";

/// Failure-close message: an unreadable schema must not look like "no usage".
const NO_MESSAGE_TABLES: &str =
    "no OpenCode message tables found (expected message/part or session_message)";

/// v1 and v2 may coexist while a local install is mid-migration, so both
/// branches are always attempted and v2 rows are anti-joined on id. The v1 side
/// is `v1_cost` verbatim; `v1_usable` is derived from those same rows and is the
/// suppression set the token and speed passes share, so one id never mixes
/// schemas across metrics.
fn usage_rows_sql(has_message: bool, has_part: bool, has_v2: bool) -> String {
    let mut sql = String::from("WITH ");
    sql.push_str(&v1_cost_cte(has_message, has_part));
    if has_v2 {
        sql.push_str(",\n");
        sql.push_str(v1_usable_cte());
        sql.push_str(",\nv2 AS (");
        sql.push_str(V2_USAGE_SQL.trim_end());
        sql.push_str(")\n");
        // The v1 rows are emitted as-is. `read_rows_for` still drops a row it
        // cannot use (negative/non-finite cost, missing timestamp); such an id
        // is absent from `v1_usable`, so its v2 copy is kept and no cost is lost.
        sql.push_str(
            "SELECT createdMs, cost, requestCount, modelID FROM v1_cost\nUNION ALL\n\
             SELECT createdMs, cost, requestCount, modelID FROM v2\n\
             WHERE NOT EXISTS (SELECT 1 FROM v1_usable u WHERE u.id = v2.id)",
        );
    } else {
        sql.push_str("\nSELECT createdMs, cost, requestCount, modelID FROM v1_cost");
    }
    sql
}

fn tokens_rows_sql(has_message: bool, has_part: bool, has_v2: bool) -> String {
    let mut sql = String::from("WITH ");
    if has_v2 {
        // `v1_usable` is derived from the cost rows, not from the token CTE:
        // whether v1 owns an id is a cost/timestamp question, so a v1 row
        // without `tokens` must still suppress its v2 copy (see `v1_usable_cte`).
        sql.push_str(&v1_cost_cte(has_message, has_part));
        sql.push_str(",\n");
        sql.push_str(v1_usable_cte());
        sql.push_str(",\n");
    }
    sql.push_str("v1 AS (");
    sql.push_str(if has_message {
        V1_MESSAGE_TOKENS_SQL
    } else {
        EMPTY_V1_TOKENS_SQL
    });
    sql.push(')');
    if has_v2 {
        sql.push_str(",\nv2 AS (");
        sql.push_str(V2_TOKENS_SQL.trim_end());
        sql.push_str(")\n");
        // Both sides are gated by `v1_usable`: v1 token rows only count for the
        // ids v1 owns, so an id v1 loses to v2 cannot contribute twice.
        sql.push_str(
            "SELECT createdMs, modelID, inputTokens, outputTokens, cacheRead, cacheWrite\n\
             FROM v1 AS v\n\
             WHERE EXISTS (SELECT 1 FROM v1_usable u WHERE u.id = v.id)\n\
             UNION ALL\n\
             SELECT createdMs, modelID, inputTokens, outputTokens, cacheRead, cacheWrite\n\
             FROM v2\n\
             WHERE NOT EXISTS (SELECT 1 FROM v1_usable u WHERE u.id = v2.id)",
        );
    } else {
        sql.push_str(
            "\nSELECT createdMs, modelID, inputTokens, outputTokens, cacheRead, cacheWrite FROM v1",
        );
    }
    sql
}

#[derive(Debug, Clone)]
pub(crate) struct UsageRow {
    created_ms: i64,
    cost: f64,
    /// One provider invocation per step-finish part; message-only databases fall back to one.
    request_count: u32,
    /// The underlying model behind the `opencode-go` Zen proxy; empty when unattributed.
    model: String,
}
#[derive(Debug, Clone)]
pub struct LocalUsageSnapshot {
    pub rolling_usage_percent: f64,
    pub weekly_usage_percent: f64,
    pub monthly_usage_percent: f64,
    pub rolling_reset_in_sec: i64,
    pub weekly_reset_in_sec: i64,
    pub monthly_reset_in_sec: i64,
}

impl LocalUsageSnapshot {
    pub fn to_fetch_result(&self) -> ProviderFetchResult {
        let now = Utc::now();
        let primary = RateWindow::with_details(
            self.rolling_usage_percent,
            Some(300),
            Some(now + Duration::seconds(self.rolling_reset_in_sec)),
            None,
        );
        let mut snap = UsageSnapshot::new(primary).with_login_method("OpenCode Go");
        snap = snap.with_secondary(RateWindow::with_details(
            self.weekly_usage_percent,
            Some(10080),
            Some(now + Duration::seconds(self.weekly_reset_in_sec)),
            None,
        ));
        let monthly_reset = now + Duration::seconds(self.monthly_reset_in_sec);
        snap = snap.with_tertiary(RateWindow::with_details(
            self.monthly_usage_percent,
            Some(43200),
            Some(monthly_reset),
            None,
        ));
        // Upstream 0.51 (#2982): local SQLite quota reconstruction is useful
        // but it is not server-confirmed authority. Keep that distinction in
        // the data contract so CLI/React can present it without guessing.
        ProviderFetchResult::new(snap, super::LOCAL_ESTIMATE_SOURCE_LABEL)
            .with_non_authoritative_pace()
    }
}

/// Candidate (auth.json, opencode.db) pairs for local OpenCode installs.
///
/// The data root follows the XDG convention (`XDG_DATA_HOME`, then
/// `~/.local/share/opencode`, then the Windows desktop-app location
/// `%LOCALAPPDATA%\opencode`), and every `opencode*.db` channel variant
/// (`opencode.db`, `opencode-stable.db`, …) is offered per base — `-wal` /
/// `-shm` sidecars never match the `.db` suffix filter.
///
/// `OPENCODE_DB` is an exclusive override: when it is set to a path, that path
/// is the only candidate (absolute, or relative to an **absolute**
/// `XDG_DATA_HOME`/`~/.local/share` `opencode` root) and the scanned installs
/// are not consulted — a missing override must not silently read another
/// account's database. `OPENCODE_DB=:memory:` yields no candidates at all, so
/// local history reads report "not available" instead of touching files.
/// Resolution never creates files or directories.
pub fn candidate_paths() -> Vec<(PathBuf, PathBuf)> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .ok()
        .map(PathBuf::from)
        .or_else(dirs::home_dir);
    candidate_paths_with(|name| std::env::var(name).ok(), home)
}

fn candidate_paths_with<F>(read_env: F, home: Option<PathBuf>) -> Vec<(PathBuf, PathBuf)>
where
    F: Fn(&str) -> Option<String>,
{
    let env_value = |name: &str| {
        read_env(name)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    // Only an absolute `XDG_DATA_HOME` is a usable data root; a relative value
    // would otherwise turn into a cwd-relative scan.
    let xdg_base = env_value("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|xdg| xdg.is_absolute())
        .map(|xdg| xdg.join("opencode"));

    if let Some(spec) = env_value("OPENCODE_DB") {
        if spec == ":memory:" {
            return Vec::new();
        }
        let db = if Path::new(&spec).is_absolute() {
            Some(PathBuf::from(&spec))
        } else {
            xdg_base
                .clone()
                .or_else(|| home.as_ref().map(|home| local_share_opencode(home)))
                .map(|base| base.join(&spec))
        };
        return match db {
            Some(db) => {
                let auth = db
                    .parent()
                    .map(|parent| parent.join("auth.json"))
                    .unwrap_or_else(|| PathBuf::from("auth.json"));
                vec![(auth, db)]
            }
            // Nothing to fall back to: the override is exclusive by contract.
            None => Vec::new(),
        };
    }

    let mut bases: Vec<PathBuf> = Vec::new();

    let push_base = |base: PathBuf, bases: &mut Vec<PathBuf>| {
        if !bases.contains(&base) {
            bases.push(base);
        }
    };

    if let Some(base) = &xdg_base {
        push_base(base.clone(), &mut bases);
    }

    if let Some(home) = &home {
        push_base(
            home.join(".local").join("share").join("opencode"),
            &mut bases,
        );
    }

    if let Some(local) = env_value("LOCALAPPDATA") {
        push_base(PathBuf::from(local).join("opencode"), &mut bases);
    }

    let mut out: Vec<(PathBuf, PathBuf)> = Vec::new();

    for base in bases {
        let auth = base.join("auth.json");
        let mut databases: Vec<PathBuf> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&base) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                if path.is_file() && name.starts_with("opencode") && name.ends_with(".db") {
                    databases.push(path);
                }
            }
        }
        // The canonical `opencode.db` first, then channel variants by name.
        databases.sort_by_key(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
                != Some("opencode.db".to_string())
        });
        if databases.is_empty() {
            databases.push(base.join("opencode.db"));
        }
        for database in databases {
            let entry = (auth.clone(), database);
            if !out.contains(&entry) {
                out.push(entry);
            }
        }
    }
    out
}

fn local_share_opencode(home: &Path) -> PathBuf {
    home.join(".local").join("share").join("opencode")
}

pub fn fetch_local_usage(now: DateTime<Utc>) -> Result<LocalUsageSnapshot, ProviderError> {
    let mut last_err: Option<ProviderError> = None;
    for (auth, db) in candidate_paths() {
        match fetch_from_paths(&auth, &db, now) {
            Ok(snap) => return Ok(snap),
            Err(e) => last_err = Some(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        ProviderError::NotInstalled(
            "OpenCode Go not detected. Log in with OpenCode Go or use it locally first.".into(),
        )
    }))
}

pub fn fetch_from_paths(
    auth_path: &Path,
    db_path: &Path,
    now: DateTime<Utc>,
) -> Result<LocalUsageSnapshot, ProviderError> {
    let has_auth = has_auth_key(auth_path);
    if !db_path.exists() {
        return Err(if has_auth {
            ProviderError::Other(
                "OpenCode Go local usage history is unavailable: database not found".into(),
            )
        } else {
            ProviderError::NotInstalled(
                "OpenCode Go not detected. Log in with OpenCode Go or use it locally first.".into(),
            )
        });
    }

    let rows = read_rows(db_path)?;
    if !has_auth && rows.is_empty() {
        return Err(ProviderError::NotInstalled(
            "OpenCode Go not detected. Log in with OpenCode Go or use it locally first.".into(),
        ));
    }
    if rows.is_empty() {
        return Err(ProviderError::Other(
            "OpenCode Go local usage history is unavailable: no local usage rows".into(),
        ));
    }

    Ok(snapshot_from_rows(&rows, now))
}

fn has_auth_key(path: &Path) -> bool {
    has_auth_key_for(path, PROVIDER_ID)
}

fn has_auth_key_for(path: &Path, provider_id: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    value
        .get(provider_id)
        .and_then(|entry| entry.get("key"))
        .and_then(|key| key.as_str())
        .is_some_and(|key| !key.trim().is_empty())
}

fn read_rows(db_path: &Path) -> Result<Vec<UsageRow>, ProviderError> {
    read_rows_for(db_path, PROVIDER_ID)
}

fn read_rows_for(db_path: &Path, provider_filter: &str) -> Result<Vec<UsageRow>, ProviderError> {
    let conn = open_readonly_connection(db_path)?;
    conn.busy_timeout(std::time::Duration::from_millis(250))
        .map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
        })?;

    let has_message = has_table(&conn, "message");
    let has_part = has_table(&conn, "part");
    let has_v2 = has_table(&conn, "session_message");
    if !has_message && !has_v2 {
        return Err(ProviderError::Other(format!(
            "SQLite error reading OpenCode Go usage: {NO_MESSAGE_TABLES}"
        )));
    }

    let sql = usage_rows_sql(has_message, has_part, has_v2);

    let mut stmt = conn.prepare(&sql).map_err(|e| {
        ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
    })?;
    let rows = stmt
        .query_map(rusqlite::params![provider_filter], |row| {
            // A NULL `createdMs` (no JSON `time.created` and no `time_created`
            // column) is a row that cannot be placed on the timeline — not a
            // database error. Reading it as `Option` keeps a single unplaceable
            // row from failing the whole pass; the row is dropped below.
            Ok((
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, Option<f64>>(1)?,
                // Request counts from the DB are clamped to ≥1 and realistically
                // far below u32::MAX; try_from guards the pathological case.
                row.get::<_, i64>(2)
                    .map(|n| u32::try_from(n.max(1)).unwrap_or(u32::MAX))
                    .unwrap_or(1),
                row.get::<_, String>(3).unwrap_or_default(),
            ))
        })
        .map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
        })?;

    let mut out = Vec::new();
    for row in rows {
        let (created_ms, cost, request_count, model) = row.map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
        })?;
        let Some(created_ms) = created_ms else {
            continue;
        };
        let Some(cost) = cost else {
            continue;
        };
        if created_ms > 0 && cost.is_finite() && cost >= 0.0 {
            out.push(UsageRow {
                created_ms,
                cost,
                request_count,
                model,
            });
        }
    }
    Ok(out)
}

/// Aggregated token totals for one Zen subscription.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TokenSums {
    pub input: u64,
    pub output: u64,
    pub cached: u64,
}

/// Token totals plus a per-model split for one local OpenCode database.
fn read_tokens_for(
    db_path: &Path,
    provider_filter: &str,
    now: DateTime<Utc>,
    days: u32,
) -> Result<(TokenSums, std::collections::HashMap<String, TokenSums>), ProviderError> {
    let conn = open_readonly_connection(db_path)?;
    conn.busy_timeout(std::time::Duration::from_millis(250))
        .map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
        })?;

    let has_message = has_table(&conn, "message");
    let has_part = has_table(&conn, "part");
    let has_v2 = has_table(&conn, "session_message");
    if !has_message && !has_v2 {
        return Err(ProviderError::Other(format!(
            "SQLite error reading OpenCode Go usage: {NO_MESSAGE_TABLES}"
        )));
    }

    let sql = tokens_rows_sql(has_message, has_part, has_v2);
    let mut stmt = conn.prepare(&sql).map_err(|e| {
        ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
    })?;
    let rows = stmt
        .query_map(rusqlite::params![provider_filter], |row| {
            Ok((
                // `None` = no timestamp for this row; skipped below instead of
                // failing the whole token pass.
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, String>(1).unwrap_or_default(),
                row.get::<_, i64>(2).unwrap_or(0),
                row.get::<_, i64>(3).unwrap_or(0),
                row.get::<_, i64>(4).unwrap_or(0),
                row.get::<_, i64>(5).unwrap_or(0),
            ))
        })
        .map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
        })?;

    let mut totals = TokenSums::default();
    let mut by_model: std::collections::HashMap<String, TokenSums> =
        std::collections::HashMap::new();
    let (since_ms, now_ms) = history_bounds_ms(now, days);
    for row in rows {
        let (created_ms, model, input, output, cache_read, cache_write) = row.map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}"))
        })?;
        let Some(created_ms) = created_ms else {
            continue;
        };
        if created_ms <= 0 || created_ms < since_ms || created_ms > now_ms {
            continue;
        }
        // The cached bucket sits alongside the raw input count (the row's
        // `tokens.input` includes cache traffic), matching the Claude-style
        // accounting the shared CostSummary uses.
        let clamped_input = input.max(0) as u64;
        let clamped_output = output.max(0) as u64;
        let clamped_cached = (cache_read.max(0) + cache_write.max(0)) as u64;
        totals.input = totals.input.saturating_add(clamped_input);
        totals.output = totals.output.saturating_add(clamped_output);
        totals.cached = totals.cached.saturating_add(clamped_cached);

        let model = model.trim();
        let bucket = if model.is_empty() {
            by_model.entry(UNKNOWN_MODEL_NAME.to_string()).or_default()
        } else {
            by_model.entry(model.to_string()).or_default()
        };
        bucket.input = bucket.input.saturating_add(clamped_input);
        bucket.output = bucket.output.saturating_add(clamped_output);
        bucket.cached = bucket.cached.saturating_add(clamped_cached);
    }
    Ok((totals, by_model))
}

/// One completed assistant message shaped for the output-speed reader.
///
/// The database stores per-message wall-clock times and the output token
/// count; there is no per-model generation duration, so callers derive a
/// lower bound from `completed - created` exactly like the legacy JSON tree.
#[derive(Debug, Clone, PartialEq)]
pub struct MessageSpeed {
    pub created_ms: i64,
    pub completed_ms: i64,
    pub output_tokens: u64,
    pub model: Option<String>,
}

/// Newest completed assistant messages for one OpenCode-family provider,
/// newest first. Returns an empty list when no database or no matching rows.
///
/// Only rows with numeric `time.created` / `time.completed` and a positive
/// `tokens.output` qualify; an in-progress or token-less row cannot produce a
/// rate and is skipped rather than guessed.
pub fn recent_message_speeds(provider_filter: &str, limit: usize) -> Vec<MessageSpeed> {
    let limit = limit.clamp(1, 200);
    for (_, db) in candidate_paths() {
        if !db.exists() {
            continue;
        }
        match read_recent_message_speeds(&db, provider_filter, limit) {
            Ok(rows) if !rows.is_empty() => return rows,
            _ => continue,
        }
    }
    Vec::new()
}

const V1_RECENT_MESSAGE_SPEEDS_SQL: &str = r#"
SELECT
  CAST(json_extract(data, '$.time.created') AS INTEGER) AS createdMs,
  CAST(json_extract(data, '$.time.completed') AS INTEGER) AS completedMs,
  CAST(json_extract(data, '$.tokens.output') AS INTEGER) AS outputTokens,
  COALESCE(json_extract(data, '$.modelID'), '') AS modelID,
  id AS id
FROM message
WHERE json_valid(data)
  AND json_extract(data, '$.providerID') = ?1
  AND json_extract(data, '$.role') = 'assistant'
  AND json_type(data, '$.time.created') IN ('integer', 'real')
  AND json_type(data, '$.time.completed') IN ('integer', 'real')
  AND json_type(data, '$.tokens.output') IN ('integer', 'real')
"#;

const V2_RECENT_MESSAGE_SPEEDS_SQL: &str = r#"
  SELECT
    CAST(json_extract(s.data, '$.time.created') AS INTEGER) AS createdMs,
    CAST(json_extract(s.data, '$.time.completed') AS INTEGER) AS completedMs,
    CAST(json_extract(s.data, '$.tokens.output') AS INTEGER) AS outputTokens,
    COALESCE(json_extract(s.data, '$.model.id'), '') AS modelID,
    s.id AS id
  FROM session_message s
  WHERE json_valid(s.data)
    AND s.type = 'assistant'
    AND json_extract(s.data, '$.model.providerID') = ?1
    AND json_type(s.data, '$.time.created') IN ('integer', 'real')
    AND json_type(s.data, '$.time.completed') IN ('integer', 'real')
    AND json_type(s.data, '$.tokens.output') IN ('integer', 'real')
"#;

const EMPTY_V1_SPEEDS_SQL: &str = "SELECT 0 AS createdMs, 0 AS completedMs, 0 AS outputTokens, \
     '' AS modelID, NULL AS id WHERE 0";

/// Speed rows use the same `v1_usable` suppression set as cost and tokens: a v1
/// turn without `time.completed`/`tokens.output` yields no speed sample instead
/// of borrowing the v2 copy's timing (see `v1_usable_cte`).
fn speeds_rows_sql(has_message: bool, has_part: bool, has_v2: bool) -> String {
    let mut sql = String::from("WITH ");
    if has_v2 {
        sql.push_str(&v1_cost_cte(has_message, has_part));
        sql.push_str(",\n");
        sql.push_str(v1_usable_cte());
        sql.push_str(",\n");
    }
    sql.push_str("v1 AS (");
    sql.push_str(if has_message {
        V1_RECENT_MESSAGE_SPEEDS_SQL
    } else {
        EMPTY_V1_SPEEDS_SQL
    });
    sql.push(')');
    if has_v2 {
        sql.push_str(",\nv2 AS (");
        sql.push_str(V2_RECENT_MESSAGE_SPEEDS_SQL.trim_end());
        sql.push_str(")\n");
        // Same gating as the token pass: one speed sample per id, from the
        // schema that owns it.
        sql.push_str(
            "SELECT createdMs, completedMs, outputTokens, modelID\n\
             FROM v1 AS v\n\
             WHERE EXISTS (SELECT 1 FROM v1_usable u WHERE u.id = v.id)\n\
             UNION ALL\n\
             SELECT createdMs, completedMs, outputTokens, modelID\n\
             FROM v2\n\
             WHERE NOT EXISTS (SELECT 1 FROM v1_usable u WHERE u.id = v2.id)\n\
             ORDER BY completedMs DESC\nLIMIT ?2",
        );
    } else {
        sql.push_str(
            "\nSELECT createdMs, completedMs, outputTokens, modelID FROM v1\n\
             ORDER BY completedMs DESC\nLIMIT ?2",
        );
    }
    sql
}

fn read_recent_message_speeds(
    db_path: &Path,
    provider_filter: &str,
    limit: usize,
) -> Result<Vec<MessageSpeed>, ProviderError> {
    let conn = open_readonly_connection(db_path)?;
    let has_message = has_table(&conn, "message");
    let has_part = has_table(&conn, "part");
    let has_v2 = has_table(&conn, "session_message");
    if !has_message && !has_v2 {
        return Err(ProviderError::Other(format!(
            "SQLite error reading OpenCode message speeds: {NO_MESSAGE_TABLES}"
        )));
    }
    let sql = speeds_rows_sql(has_message, has_part, has_v2);
    let mut stmt = conn.prepare(&sql).map_err(|e| {
        ProviderError::Other(format!("SQLite error reading OpenCode message speeds: {e}"))
    })?;
    let rows = stmt
        .query_map(rusqlite::params![provider_filter, limit as i64], |row| {
            Ok((
                // `None` = no timestamp for this row; skipped below instead of
                // failing the whole speed read.
                row.get::<_, Option<i64>>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2).map(|n| n.max(0) as u64)?,
                row.get::<_, String>(3).ok(),
            ))
        })
        .map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode message speeds: {e}"))
        })?;

    let mut out = Vec::new();
    for row in rows {
        let (created_ms, completed_ms, output_tokens, model) = row.map_err(|e| {
            ProviderError::Other(format!("SQLite error reading OpenCode message speeds: {e}"))
        })?;
        let Some(created_ms) = created_ms else {
            continue;
        };
        let row = MessageSpeed {
            created_ms,
            completed_ms,
            output_tokens,
            model: model
                .map(|model| model.trim().to_string())
                .filter(|model| !model.is_empty()),
        };
        if row.output_tokens > 0 && row.completed_ms >= row.created_ms {
            out.push(row);
        }
    }
    Ok(out)
}

/// Open a read-only connection without creating `-wal`/`-shm` sidecars for idle
/// WAL-mode databases (upstream #2544).
fn open_readonly_connection(db_path: &Path) -> Result<Connection, ProviderError> {
    crate::core::open_readonly_sqlite_connection(db_path, std::time::Duration::from_millis(250))
        .map_err(|e| ProviderError::Other(format!("SQLite error reading OpenCode Go usage: {e}")))
}

fn has_table(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

fn snapshot_from_rows(rows: &[UsageRow], now: DateTime<Utc>) -> LocalUsageSnapshot {
    let now_ms = now.timestamp_millis();
    let session_start = now_ms - FIVE_HOURS_MS;
    let week_start_ms = start_of_utc_iso_week_ms(now);
    let week_end_ms = week_start_ms + WEEK_MS;
    let earliest_ms = rows.iter().map(|r| r.created_ms).min();
    let (month_start_ms, month_end_ms) = month_bounds_ms(now, earliest_ms);

    let mut session_cost = 0.0;
    let mut weekly_cost = 0.0;
    let mut monthly_cost = 0.0;
    let mut oldest_session_ms: Option<i64> = None;

    for row in rows {
        if row.created_ms >= session_start && row.created_ms < now_ms {
            session_cost += row.cost;
            oldest_session_ms = Some(match oldest_session_ms {
                Some(prev) => prev.min(row.created_ms),
                None => row.created_ms,
            });
        }
        if row.created_ms >= week_start_ms && row.created_ms < week_end_ms {
            weekly_cost += row.cost;
        }
        if row.created_ms >= month_start_ms && row.created_ms < month_end_ms {
            monthly_cost += row.cost;
        }
    }

    let oldest = oldest_session_ms.unwrap_or(now_ms);
    let rolling_reset_in_sec = ((oldest + FIVE_HOURS_MS - now_ms) / 1000).max(0);

    LocalUsageSnapshot {
        rolling_usage_percent: percent(session_cost, SESSION_LIMIT_USD),
        weekly_usage_percent: percent(weekly_cost, WEEKLY_LIMIT_USD),
        monthly_usage_percent: percent(monthly_cost, MONTHLY_LIMIT_USD),
        rolling_reset_in_sec,
        weekly_reset_in_sec: ((week_end_ms - now_ms) / 1000).max(0),
        monthly_reset_in_sec: ((month_end_ms - now_ms) / 1000).max(0),
    }
}

fn percent(used: f64, limit: f64) -> f64 {
    if !used.is_finite() || limit <= 0.0 {
        return 0.0;
    }
    let value = (used / limit * 100.0).clamp(0.0, 100.0);
    (value * 10.0).round() / 10.0
}

/// Bucket label for rows whose local `modelID` is missing or blank (upstream #2649).
const UNKNOWN_MODEL_NAME: &str = "unknown";

/// One (day, model) cost bucket for the daily per-model breakdown (upstream #2649).
///
/// Mirrors `CostUsageDailyReport.ModelBreakdown` plus the day key, so the shared
/// cost-history chart can render OpenCode Go the same way it renders Codex/Claude
/// without a bespoke chart surface. Entries are sorted by `(day_key, model)`.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyModelCost {
    /// `yyyy-MM-dd` local calendar day (matches Codex/Claude cost-history keying).
    pub day_key: String,
    /// Trimmed model id, or `UNKNOWN_MODEL_NAME` when the row had none.
    pub model: String,
    /// Cost in USD accumulated for this (day, model) bucket.
    pub cost: f64,
    /// Number of provider invocations (step-finish parts, or one per message).
    pub request_count: u32,
}

/// Provider-local cost summary reusing the shared `CostSummary` fields the chart
/// already consumes (`total_cost_usd`, `by_model`, `period_start/end`). Built
/// from the same local rows as the daily breakdown so the two surfaces agree.
#[derive(Debug, Clone, Default)]
pub struct ModelCostSummary {
    pub total_cost_usd: f64,
    pub by_model: std::collections::HashMap<String, f64>,
    pub request_count: u32,
    pub period_start: Option<NaiveDate>,
    pub period_end: Option<NaiveDate>,
    /// Token totals from the message-level token pass; zero on databases that
    /// predate token recording.
    pub tokens: TokenSums,
    /// Per-model token split; models mirror `by_model` plus the unknown bucket.
    pub by_model_tokens: std::collections::HashMap<String, TokenSums>,
}

/// Local calendar-day key (`yyyy-MM-dd`) for a UTC millisecond timestamp,
/// matching how Codex/Claude cost history is keyed.
fn day_key_local(ms: i64) -> Option<String> {
    Local
        .timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.date_naive().format("%Y-%m-%d").to_string())
}

/// Local "today" derived from a UTC instant, so the day window is deterministic
/// under test rather than snapping to wall-clock `Local::now()`.
fn local_today_from_utc(now: DateTime<Utc>) -> NaiveDate {
    Local.from_utc_datetime(&now.naive_utc()).date_naive()
}

/// Return the same inclusive local-calendar history bounds used by cost rows.
/// Token totals must use this helper too; otherwise a database token pass can
/// accidentally report all-time totals beside a bounded cost summary.
fn history_bounds_ms(now: DateTime<Utc>, days: u32) -> (i64, i64) {
    let clamped = days.clamp(1, 365);
    let today = local_today_from_utc(now);
    let since = today - Duration::days(clamped as i64 - 1);
    let since_ms = Local
        .from_local_datetime(&since.and_hms_opt(0, 0, 0).unwrap_or_default())
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    (since_ms, now.timestamp_millis())
}

/// Group rows into `(day, model)` cost buckets (upstream #2649).
///
/// Rows outside the `[since, now]` window are dropped; model ids are trimmed and
/// blanks collapse to `UNKNOWN_MODEL_NAME`. The result is sorted by
/// `(day_key, model)` for deterministic ordering.
pub fn daily_model_costs(
    rows: &[UsageRow],
    now: DateTime<Utc>,
    history_days: u32,
) -> Vec<DailyModelCost> {
    let (since_ms, now_ms) = history_bounds_ms(now, history_days);

    let mut by_day_model: std::collections::BTreeMap<
        String,
        std::collections::BTreeMap<String, (f64, u32)>,
    > = std::collections::BTreeMap::new();
    for row in rows {
        if row.created_ms < since_ms || row.created_ms > now_ms {
            continue;
        }
        let Some(key) = day_key_local(row.created_ms) else {
            continue;
        };
        let trimmed = row.model.trim();
        let model = if trimmed.is_empty() {
            UNKNOWN_MODEL_NAME
        } else {
            trimmed
        };
        let entry = by_day_model.entry(key).or_default();
        let bucket = entry.entry(model.to_string()).or_insert((0.0, 0));
        bucket.0 += row.cost;
        bucket.1 = bucket.1.saturating_add(row.request_count);
    }

    let mut out = Vec::new();
    for (day_key, models) in by_day_model {
        for (model, (cost, request_count)) in models {
            out.push(DailyModelCost {
                day_key: day_key.clone(),
                model,
                cost,
                request_count,
            });
        }
    }
    out
}

/// Build the provider-local cost summary for the last `days` days.
pub fn model_cost_summary_from_rows(
    rows: &[UsageRow],
    now: DateTime<Utc>,
    days: u32,
) -> ModelCostSummary {
    let (since_ms, now_ms) = history_bounds_ms(now, days);

    let mut total = 0.0;
    let mut request_count = 0u32;
    let mut by_model: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut earliest: Option<NaiveDate> = None;
    let mut latest: Option<NaiveDate> = None;
    for row in rows {
        if row.created_ms < since_ms || row.created_ms > now_ms {
            continue;
        }
        total += row.cost;
        request_count = request_count.saturating_add(row.request_count);
        let trimmed = row.model.trim();
        let model = if trimmed.is_empty() {
            UNKNOWN_MODEL_NAME
        } else {
            trimmed
        };
        *by_model.entry(model.to_string()).or_insert(0.0) += row.cost;
        if let Some(day) = day_key_local(row.created_ms)
            .and_then(|k| NaiveDate::parse_from_str(&k, "%Y-%m-%d").ok())
        {
            earliest = Some(earliest.map(|e| e.min(day)).unwrap_or(day));
            latest = Some(latest.map(|l| l.max(day)).unwrap_or(day));
        }
    }
    ModelCostSummary {
        total_cost_usd: total,
        by_model,
        request_count,
        period_start: earliest,
        period_end: latest,
        tokens: TokenSums::default(),
        by_model_tokens: std::collections::HashMap::new(),
    }
}

/// Per-day cost series (`yyyy-MM-dd`, cost USD) for the shared cost-history chart,
/// summed across all models. Reads the first available local OpenCode install.
/// Empty when no install is detected.
pub fn daily_cost_series(now: DateTime<Utc>, history_days: u32) -> Vec<(String, f64)> {
    daily_cost_series_for(PROVIDER_ID, now, history_days)
}

/// Per-day cost series filtered to one Zen subscription's rows.
pub fn daily_cost_series_for(
    provider_filter: &str,
    now: DateTime<Utc>,
    history_days: u32,
) -> Vec<(String, f64)> {
    let Some(database) = first_available_db(provider_filter) else {
        return Vec::new();
    };
    let Ok(rows) = read_rows_for(&database, provider_filter) else {
        return Vec::new();
    };
    let buckets = daily_model_costs(&rows, now, history_days);
    let mut by_day: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for b in &buckets {
        *by_day.entry(b.day_key.clone()).or_insert(0.0) += b.cost;
    }
    by_day.into_iter().collect()
}

/// Provider-local cost summary for the chart's local-usage panel. `None` when no
/// local OpenCode install is detected.
pub fn model_cost_summary_scan(now: DateTime<Utc>, days: u32) -> Option<ModelCostSummary> {
    model_cost_summary_scan_for(PROVIDER_ID, now, days)
}

/// Provider-local cost summary filtered to one Zen subscription, with the
/// token pass attached from the same database the cost rows came from.
pub fn model_cost_summary_scan_for(
    provider_filter: &str,
    now: DateTime<Utc>,
    days: u32,
) -> Option<ModelCostSummary> {
    let database = first_available_db(provider_filter)?;
    let rows = read_rows_for(&database, provider_filter).ok()?;
    let mut summary = model_cost_summary_from_rows(&rows, now, days);
    if let Ok((tokens, by_model_tokens)) = read_tokens_for(&database, provider_filter, now, days) {
        summary.tokens = tokens;
        summary.by_model_tokens = by_model_tokens;
    }
    Some(summary)
}

/// First candidate database that yields rows or a matching auth entry.
/// Returns `None` when no install is reachable (auth+db both absent) rather
/// than propagating `NotInstalled`, since the cost surfaces treat "no data"
/// as empty.
fn first_available_db(provider_filter: &str) -> Option<PathBuf> {
    for (auth, db) in candidate_paths() {
        if !db.exists() {
            continue;
        }
        match read_rows_for(&db, provider_filter) {
            Ok(rows) if !rows.is_empty() || has_auth_key_for(&auth, provider_filter) => {
                return Some(db);
            }
            Ok(_) => continue,
            Err(_) => continue,
        }
    }
    None
}

/// Read usage rows from the first candidate install that yields any. Returns
/// `None` when no install is reachable (auth+db both absent) rather than
/// propagating `NotInstalled`, since the cost surfaces treat "no data" as empty.
fn read_available_rows() -> Option<Vec<UsageRow>> {
    let database = first_available_db(PROVIDER_ID)?;
    read_rows_for(&database, PROVIDER_ID).ok()
}

/// ISO week start (Monday 00:00 UTC), matching upstream calendar settings.
fn start_of_utc_iso_week_ms(now: DateTime<Utc>) -> i64 {
    let date = now.date_naive();
    let days_from_monday = date.weekday().num_days_from_monday() as i64;
    let monday = date - Duration::days(days_from_monday);
    Utc.from_utc_datetime(&monday.and_hms_opt(0, 0, 0).unwrap_or_default())
        .timestamp_millis()
}

fn month_bounds_ms(now: DateTime<Utc>, anchor_ms: Option<i64>) -> (i64, i64) {
    let Some(anchor_ms) = anchor_ms else {
        let start = NaiveDate::from_ymd_opt(now.year(), now.month(), 1)
            .unwrap_or_else(|| now.date_naive())
            .and_hms_opt(0, 0, 0)
            .unwrap_or_default();
        let start_dt = Utc.from_utc_datetime(&start);
        let end_dt = if now.month() == 12 {
            Utc.with_ymd_and_hms(now.year() + 1, 1, 1, 0, 0, 0)
                .single()
                .unwrap_or(start_dt)
        } else {
            Utc.with_ymd_and_hms(now.year(), now.month() + 1, 1, 0, 0, 0)
                .single()
                .unwrap_or(start_dt)
        };
        return (start_dt.timestamp_millis(), end_dt.timestamp_millis());
    };

    let anchor = DateTime::<Utc>::from_timestamp_millis(anchor_ms).unwrap_or(now);
    let mut year = now.year();
    let mut month = now.month();
    let mut start = anchored_month(year, month, &anchor);
    if start > now {
        if month == 1 {
            year -= 1;
            month = 12;
        } else {
            month -= 1;
        }
        start = anchored_month(year, month, &anchor);
    }
    let (end_year, end_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };
    let end = anchored_month(end_year, end_month, &anchor);
    (start.timestamp_millis(), end.timestamp_millis())
}

fn anchored_month(year: i32, month: u32, anchor: &DateTime<Utc>) -> DateTime<Utc> {
    let day = anchor.day();
    let (hour, min, sec, nano) = (
        anchor.hour(),
        anchor.minute(),
        anchor.second(),
        anchor.nanosecond(),
    );
    if let Some(date) = NaiveDate::from_ymd_opt(year, month, day)
        && let Some(ndt) = date.and_hms_nano_opt(hour, min, sec, nano)
    {
        return Utc.from_utc_datetime(&ndt);
    }
    // Clamp to last day of month when anchor day overflows (e.g. 31 → Feb).
    let last_day = NaiveDate::from_ymd_opt(year, month, 1)
        .map(|d| {
            if month == 12 {
                NaiveDate::from_ymd_opt(year + 1, 1, 1)
            } else {
                NaiveDate::from_ymd_opt(year, month + 1, 1)
            }
            .unwrap_or(d)
                - Duration::days(1)
        })
        .map(|d| d.day())
        .unwrap_or(28);
    let date = NaiveDate::from_ymd_opt(year, month, last_day).unwrap_or_else(|| {
        NaiveDate::from_ymd_opt(year, month, 1).unwrap_or_else(|| Utc::now().date_naive())
    });
    let ndt = date
        .and_hms_nano_opt(hour, min, sec, nano)
        .or_else(|| date.and_hms_opt(0, 0, 0))
        .unwrap_or_default();
    Utc.from_utc_datetime(&ndt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Weekday;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn local_fetch_result_keeps_estimate_source_and_reset_windows() {
        let result = LocalUsageSnapshot {
            rolling_usage_percent: 12.0,
            weekly_usage_percent: 23.0,
            monthly_usage_percent: 34.0,
            rolling_reset_in_sec: 300,
            weekly_reset_in_sec: 1_000,
            monthly_reset_in_sec: 2_000,
        }
        .to_fetch_result();

        assert_eq!(
            result.source_label,
            super::super::LOCAL_ESTIMATE_SOURCE_LABEL
        );
        assert_eq!(result.usage.primary.used_percent, 12.0);
        assert_eq!(result.usage.secondary.as_ref().unwrap().used_percent, 23.0);
        assert_eq!(result.usage.tertiary.as_ref().unwrap().used_percent, 34.0);
        assert!(result.usage.primary.resets_at.is_some());
        assert!(result.usage.secondary.as_ref().unwrap().resets_at.is_some());
        assert!(result.usage.tertiary.as_ref().unwrap().resets_at.is_some());
    }

    fn temp_db_path(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("opencodego-local-{label}-{nanos}.db"))
    }

    fn write_message_db(path: &Path, rows: &[(i64, f64)]) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (
                id TEXT PRIMARY KEY,
                data TEXT,
                time_created INTEGER
            );",
        )
        .unwrap();
        for (i, (created_ms, cost)) in rows.iter().enumerate() {
            let data = format!(
                r#"{{"providerID":"opencode-go","role":"assistant","cost":{cost},"time":{{"created":{created_ms}}}}}"#
            );
            conn.execute(
                "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("m{i}"), data, created_ms],
            )
            .unwrap();
        }
    }

    /// Build a message-only DB with optional per-row `modelID` (upstream #2649 fixtures).
    fn write_message_db_with_model(path: &Path, rows: &[(i64, f64, Option<&str>)]) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (
                id TEXT PRIMARY KEY,
                data TEXT,
                time_created INTEGER
            );",
        )
        .unwrap();
        for (i, (created_ms, cost, model)) in rows.iter().enumerate() {
            let model_json = match model {
                Some(m) => format!(r#","modelID":"{m}""#),
                None => String::new(),
            };
            let data = format!(
                r#"{{"providerID":"opencode-go","role":"assistant","cost":{cost}{model_json},"time":{{"created":{created_ms}}}}}"#
            );
            conn.execute(
                "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("m{i}"), data, created_ms],
            )
            .unwrap();
        }
    }

    fn write_message_db_with_tokens(path: &Path, rows: &[(i64, &str, i64, i64, i64, i64)]) {
        let conn = Connection::open(path).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (
                id TEXT PRIMARY KEY,
                data TEXT,
                time_created INTEGER
            );",
        )
        .unwrap();
        for (i, (created_ms, model, input, output, cache_read, cache_write)) in
            rows.iter().enumerate()
        {
            let data = format!(
                r#"{{"providerID":"opencode-go","role":"assistant","modelID":"{model}","tokens":{{"input":{input},"output":{output},"cache":{{"read":{cache_read},"write":{cache_write}}}}},"time":{{"created":{created_ms}}}}}"#
            );
            conn.execute(
                "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
                rusqlite::params![format!("m{i}"), data, created_ms],
            )
            .unwrap();
        }
    }

    /// Insert one assistant message and return its id, optionally with a modelID.
    fn insert_message(
        conn: &Connection,
        id: &str,
        created_ms: i64,
        cost: Option<f64>,
        model: Option<&str>,
    ) {
        let cost_json = match cost {
            Some(c) => format!(r#","cost":{c}"#),
            None => String::new(),
        };
        let model_json = match model {
            Some(m) => format!(r#","modelID":"{m}""#),
            None => String::new(),
        };
        let data = format!(
            r#"{{"providerID":"opencode-go","role":"assistant"{cost_json}{model_json},"time":{{"created":{created_ms}}}}}"#
        );
        conn.execute(
            "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, data, created_ms],
        )
        .unwrap();
    }

    /// Insert a step-finish part carrying a cost, attached to `message_id`.
    fn insert_step_finish_part(
        conn: &Connection,
        id: &str,
        message_id: &str,
        created_ms: i64,
        cost: f64,
    ) {
        let data =
            format!(r#"{{"type":"step-finish","cost":{cost},"time":{{"created":{created_ms}}}}}"#);
        conn.execute(
            "INSERT INTO part (id, message_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, message_id, data, created_ms],
        )
        .unwrap();
    }

    fn iso_ms(iso: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(iso)
            .unwrap()
            .timestamp_millis()
    }

    // ---- v2 (`session_message` / `session_part`) fixtures ----------------

    /// One synthetic v2 `session_message` row.
    #[derive(Clone, Copy)]
    struct V2Row<'a> {
        id: &'a str,
        /// `session_message.type` — `assistant`, `compaction` or `user`.
        kind: &'a str,
        provider: &'a str,
        model: Option<&'a str>,
        cost: Option<f64>,
        /// `(input, output, cache read, cache write)`.
        tokens: Option<(i64, i64, i64, i64)>,
        created: Option<i64>,
        completed: Option<i64>,
    }

    impl Default for V2Row<'_> {
        fn default() -> Self {
            Self {
                id: "",
                kind: "assistant",
                provider: "opencode-go",
                model: None,
                cost: None,
                tokens: None,
                created: None,
                completed: None,
            }
        }
    }

    fn create_v2_tables(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE session_message (
                id TEXT PRIMARY KEY,
                session_id TEXT,
                type TEXT,
                seq INTEGER,
                time_created INTEGER,
                time_updated INTEGER,
                data TEXT
            );
            CREATE TABLE session_part (
                message_id TEXT,
                session_id TEXT,
                ordinal INTEGER,
                type TEXT,
                call_id TEXT,
                time_created INTEGER,
                time_updated INTEGER,
                data TEXT,
                PRIMARY KEY (message_id, ordinal)
            );",
        )
        .unwrap();
    }

    fn insert_v2(conn: &Connection, row: V2Row<'_>) {
        let mut model = serde_json::Map::new();
        model.insert(
            "providerID".to_string(),
            serde_json::Value::String(row.provider.to_string()),
        );
        if let Some(id) = row.model {
            model.insert("id".to_string(), serde_json::Value::String(id.to_string()));
        }
        let mut data = serde_json::Map::new();
        data.insert("model".to_string(), serde_json::Value::Object(model));
        if let Some(cost) = row.cost {
            data.insert("cost".to_string(), serde_json::json!(cost));
        }
        if let Some((input, output, cache_read, cache_write)) = row.tokens {
            data.insert(
                "tokens".to_string(),
                serde_json::json!({
                    "input": input,
                    "output": output,
                    "reasoning": 0,
                    "cache": {"read": cache_read, "write": cache_write},
                }),
            );
        }
        let mut time = serde_json::Map::new();
        if let Some(created) = row.created {
            time.insert("created".to_string(), serde_json::json!(created));
        }
        if let Some(completed) = row.completed {
            time.insert("completed".to_string(), serde_json::json!(completed));
        }
        if !time.is_empty() {
            data.insert("time".to_string(), serde_json::Value::Object(time));
        }
        conn.execute(
            "INSERT INTO session_message (id, session_id, type, seq, time_created, time_updated, data)
             VALUES (?1, 'ses_test', ?2, 0, ?3, ?3, ?4)",
            rusqlite::params![
                row.id,
                row.kind,
                row.created.unwrap_or(0),
                serde_json::Value::Object(data).to_string()
            ],
        )
        .unwrap();
    }

    fn insert_v1_message(conn: &Connection, id: &str, data: &str, time_created: i64) {
        conn.execute(
            "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, data, time_created],
        )
        .unwrap();
    }

    /// A v1 `message` row with an explicit `time_created` (NULL allowed), so the
    /// timestamp-fallback / NULL-timestamp cases are expressible.
    fn insert_v1_message_with_created(
        conn: &Connection,
        id: &str,
        data: &str,
        time_created: Option<i64>,
    ) {
        conn.execute(
            "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![id, data, time_created],
        )
        .unwrap();
    }

    fn create_v1_message_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);",
        )
        .unwrap();
    }

    /// A v1 assistant message row plus a v2 copy under the **same** id, sharing
    /// one fixture shape so cross-metric dedupe tests stay readable. Each v1
    /// row's `time_created` column mirrors its JSON `time.created` (NULL when the
    /// payload has none), matching how both paths resolve `createdMs`.
    fn mixed_v1_v2_db(
        label: &str,
        v1: &[(&str, &str)],
        v2: &[V2Row<'_>],
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(format!("opencode-{label}.db"));
        let conn = Connection::open(&db).unwrap();
        create_v1_message_table(&conn);
        create_v2_tables(&conn);
        for (id, data) in v1 {
            // `time_created` mirrors the JSON timestamp so the column fallback
            // agrees with the payload; callers that need NULL omit it.
            let created: Option<i64> = serde_json::from_str::<serde_json::Value>(data)
                .ok()
                .and_then(|value| value.get("time")?.get("created")?.as_i64());
            insert_v1_message_with_created(&conn, id, data, created);
        }
        for row in v2 {
            insert_v2(&conn, *row);
        }
        drop(conn);
        (dir, db)
    }

    #[test]
    fn not_detected_without_db_or_auth() {
        let dir = std::env::temp_dir().join(format!(
            "opencodego-missing-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        // Best-effort fixture setup; a failure surfaces as the fetch error below.
        let _created = std::fs::create_dir_all(&dir);
        let auth = dir.join("auth.json");
        let db = dir.join("opencode.db");
        let err = fetch_from_paths(&auth, &db, Utc::now()).unwrap_err();
        assert!(matches!(err, ProviderError::NotInstalled(_)));
        // Best-effort teardown; temp dir may already be gone.
        let _removed_dir = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sums_session_weekly_monthly_costs() {
        let db = temp_db_path("sums");
        let now = Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap(); // Wednesday
        let now_ms = now.timestamp_millis();
        // $6 in the rolling 5h window → 50% of $12
        // $15 in ISO week → 50% of $30
        // $30 in anchored month → 50% of $60
        let session_ms = now_ms - 60_000;
        let week_ms = start_of_utc_iso_week_ms(now) + 3_600_000;
        let month_anchor_ms = now_ms - 10 * 24 * 60 * 60 * 1000;
        write_message_db(
            &db,
            &[
                (session_ms, 6.0),
                (week_ms, 9.0), // plus session = 15 in week if session also in week
                (month_anchor_ms, 15.0),
            ],
        );

        // auth present so empty-rows path is not used; auth not required when rows exist
        let auth = db.with_extension("auth.json");
        // Fixture write; failure would make fetch_from_paths return an error.
        let _written = std::fs::write(&auth, r#"{"opencode-go":{"key":"test-key"}}"#);

        let snap = fetch_from_paths(&auth, &db, now).unwrap();
        assert!((snap.rolling_usage_percent - 50.0).abs() < 0.05, "{snap:?}");
        // session 6 + week-only 9 = 15 → 50%
        assert!((snap.weekly_usage_percent - 50.0).abs() < 0.05, "{snap:?}");
        // session 6 + week 9 + month 15 = 30 → 50%
        assert!((snap.monthly_usage_percent - 50.0).abs() < 0.05, "{snap:?}");

        // Best-effort teardown; leftover temp files are harmless.
        let _removed_db = std::fs::remove_file(&db);
        let _removed_auth = std::fs::remove_file(&auth);
    }

    #[test]
    fn prefers_step_finish_parts_when_present() {
        let db = temp_db_path("parts");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, data TEXT, time_created INTEGER);",
        )
        .unwrap();
        let now = Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap();
        let created = now.timestamp_millis() - 1_000;
        // Message cost would be $12 (100%), but step-finish parts sum to $3 (25%).
        conn.execute(
            "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "m1",
                format!(
                    r#"{{"providerID":"opencode-go","role":"assistant","cost":12,"time":{{"created":{created}}}}}"#
                ),
                created
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO part (id, message_id, data, time_created) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                "p1",
                "m1",
                format!(r#"{{"type":"step-finish","cost":3,"time":{{"created":{created}}}}}"#),
                created
            ],
        )
        .unwrap();
        drop(conn);

        let auth = db.with_extension("auth.json");
        // Fixture write; a failure would surface in fetch_from_paths.
        let _written_auth = std::fs::write(&auth, r#"{"opencode-go":{"key":"k"}}"#);
        let snap = fetch_from_paths(&auth, &db, now).unwrap();
        assert!(
            (snap.rolling_usage_percent - 25.0).abs() < 0.05,
            "expected step-finish cost only, got {snap:?}"
        );
        // Best-effort teardown; leftover temp files are harmless.
        let _removed_db = std::fs::remove_file(&db);
        let _removed_auth = std::fs::remove_file(&auth);
    }

    #[test]
    fn percent_rounds_to_one_decimal() {
        assert!((percent(1.0, 12.0) - 8.3).abs() < 0.05);
        assert_eq!(percent(0.0, 12.0), 0.0);
        assert_eq!(percent(f64::NAN, 12.0), 0.0);
    }

    #[test]
    fn iso_week_starts_monday_utc() {
        // 2026-03-18 is a Wednesday; week start should be 2026-03-16 00:00 UTC.
        let wed = Utc.with_ymd_and_hms(2026, 3, 18, 15, 0, 0).unwrap();
        let start = start_of_utc_iso_week_ms(wed);
        let expected = Utc
            .with_ymd_and_hms(2026, 3, 16, 0, 0, 0)
            .unwrap()
            .timestamp_millis();
        assert_eq!(start, expected);
        assert_eq!(wed.weekday(), Weekday::Wed);
    }

    #[test]
    fn idle_wal_mode_read_creates_no_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let auth = dir.path().join("auth.json");
        std::fs::write(&auth, r#"{"opencode-go":{"key":"k"}}"#).unwrap();

        // Build a WAL-mode DB, insert a row, leave journal_mode=WAL, then drop
        // any writer-created sidecars so the main file is an idle WAL header.
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
            conn.execute_batch(
                "CREATE TABLE message (
                    id TEXT PRIMARY KEY,
                    data TEXT,
                    time_created INTEGER
                );",
            )
            .unwrap();
            let now = Utc::now();
            let created = now.timestamp_millis() - 1_000;
            let data = format!(
                r#"{{"providerID":"opencode-go","role":"assistant","cost":3,"time":{{"created":{created}}}}}"#
            );
            conn.execute(
                "INSERT INTO message (id, data, time_created) VALUES ('m1', ?1, ?2)",
                rusqlite::params![data, created],
            )
            .unwrap();
            // Truncate empties WAL content before close; journal_mode stays WAL.
            // Best-effort checkpoint; a truncated WAL is just tidier.
            let _checkpointed = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
            drop(conn);
        }

        // Ensure idle-WAL case: header says WAL, no live sidecars.
        let wal = crate::core::sqlite_sidecar_path(&db, "-wal");
        let shm = crate::core::sqlite_sidecar_path(&db, "-shm");
        // Prefer rename-away over delete if OS still holds handles.
        for side in [&wal, &shm] {
            if side.exists() {
                let parked = side.with_extension("parked");
                // Best-effort parking; a locked sidecar just stays put.
                let _parked = std::fs::rename(side, parked);
            }
        }
        assert!(!wal.exists(), "precondition: no -wal");
        assert!(!shm.exists(), "precondition: no -shm");

        let snap = fetch_from_paths(&auth, &db, Utc::now()).expect("read idle WAL db");
        assert!(snap.rolling_usage_percent > 0.0, "{snap:?}");

        assert!(
            !wal.exists() && !shm.exists(),
            "reader must not create -wal/-shm sidecars"
        );
    }

    #[test]
    fn recent_message_speeds_filters_incomplete_rows_and_orders_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (
                id TEXT PRIMARY KEY,
                data TEXT,
                time_created INTEGER
            );",
        )
        .unwrap();
        let rows = [
            (
                "old",
                r#"{"providerID":"opencode","role":"assistant","modelID":"m-old","tokens":{"output":100},"time":{"created":10000,"completed":12000}}"#,
            ),
            (
                "new",
                r#"{"providerID":"opencode","role":"assistant","modelID":"m-new","tokens":{"output":200},"time":{"created":20000,"completed":21000}}"#,
            ),
            (
                "open",
                r#"{"providerID":"opencode","role":"assistant","modelID":"m-progress","tokens":{"output":50},"time":{"created":30000}}"#,
            ),
            (
                "zero",
                r#"{"providerID":"opencode","role":"assistant","modelID":"m-zero","tokens":{"output":0},"time":{"created":40000,"completed":41000}}"#,
            ),
            (
                "other",
                r#"{"providerID":"opencode-go","role":"assistant","modelID":"m-other","tokens":{"output":999},"time":{"created":50000,"completed":51000}}"#,
            ),
        ];
        for (id, data) in rows {
            conn.execute(
                "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, 0)",
                rusqlite::params![id, data],
            )
            .unwrap();
        }
        drop(conn);

        let speeds = read_recent_message_speeds(&db, "opencode", 20).unwrap();
        assert_eq!(speeds.len(), 2, "{speeds:?}");
        assert_eq!(speeds[0].model.as_deref(), Some("m-new"));
        assert_eq!(speeds[1].model.as_deref(), Some("m-old"));
        assert_eq!(speeds[0].output_tokens, 200);
        assert_eq!(speeds[0].completed_ms, 21000);
    }

    // ---- A14: per-model daily cost breakdown (upstream #2649) -------------

    fn a14_now() -> DateTime<Utc> {
        Utc.timestamp_opt(1_772_798_400, 0).unwrap()
    }

    fn a14_now_afternoon() -> DateTime<Utc> {
        Utc.timestamp_opt(1_772_798_400 + 4 * 3600, 0).unwrap()
    }

    #[test]
    fn daily_entries_group_cost_by_model_within_a_day() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let now = a14_now_afternoon();
        write_message_db_with_model(
            &db,
            &[
                (
                    iso_ms("2026-03-06T11:00:00.000Z"),
                    3.0,
                    Some("claude-sonnet-4-5"),
                ),
                (
                    iso_ms("2026-03-06T12:00:00.000Z"),
                    2.0,
                    Some("gpt-5.1-codex"),
                ),
                (
                    iso_ms("2026-03-06T13:00:00.000Z"),
                    1.0,
                    Some("claude-sonnet-4-5"),
                ),
            ],
        );
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, now, 30);

        // Day key is local-calendar; assert on tz-independent model aggregation.
        let total: f64 = buckets.iter().map(|b| b.cost).sum();
        assert!((total - 6.0).abs() < 1e-6, "total {total}");
        assert_eq!(buckets.iter().map(|b| b.request_count).sum::<u32>(), 3);
        let by_model: std::collections::HashMap<&str, f64> =
            buckets.iter().map(|b| (b.model.as_str(), b.cost)).collect();
        assert!((by_model["claude-sonnet-4-5"] - 4.0).abs() < 1e-6);
        assert!((by_model["gpt-5.1-codex"] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn step_finish_parts_inherit_their_model_from_the_parent_message() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, data TEXT, time_created INTEGER);",
        )
        .unwrap();
        let created = iso_ms("2026-03-06T11:00:00.000Z");
        insert_message(&conn, "m1", created, None, Some("grok-code-fast-1"));
        insert_step_finish_part(&conn, "p1", "m1", created, 3.0);
        drop(conn);

        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now(), 30);
        assert_eq!(buckets.len(), 1, "{buckets:?}");
        assert_eq!(buckets[0].model, "grok-code-fast-1");
        assert!((buckets[0].cost - 3.0).abs() < 1e-6);
    }

    #[test]
    fn messages_without_a_model_fall_back_to_the_unknown_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(&db, &[(iso_ms("2026-03-06T11:00:00.000Z"), 4.0, None)]);
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now(), 30);
        assert_eq!(buckets.len(), 1, "{buckets:?}");
        assert_eq!(buckets[0].model, UNKNOWN_MODEL_NAME);
        assert!((buckets[0].cost - 4.0).abs() < 1e-6);
    }

    #[test]
    fn whitespace_only_model_ids_fall_back_to_the_unknown_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(
            &db,
            &[(iso_ms("2026-03-06T11:00:00.000Z"), 5.0, Some("   "))],
        );
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now(), 30);
        assert_eq!(buckets.len(), 1, "{buckets:?}");
        assert_eq!(buckets[0].model, UNKNOWN_MODEL_NAME);
        assert!((buckets[0].cost - 5.0).abs() < 1e-6);
    }

    #[test]
    fn model_ids_with_incidental_whitespace_merge_with_the_trimmed_bucket() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(
            &db,
            &[
                (
                    iso_ms("2026-03-06T11:00:00.000Z"),
                    2.0,
                    Some("claude-sonnet-4-5"),
                ),
                (
                    iso_ms("2026-03-06T12:00:00.000Z"),
                    3.0,
                    Some("  claude-sonnet-4-5  "),
                ),
            ],
        );
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now_afternoon(), 30);
        assert_eq!(buckets.len(), 1, "{buckets:?}");
        assert_eq!(buckets[0].model, "claude-sonnet-4-5");
        assert!((buckets[0].cost - 5.0).abs() < 1e-6);
        assert_eq!(buckets[0].request_count, 2);
    }

    #[test]
    fn multiple_days_bucket_separately_and_sort_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(
            &db,
            &[
                (iso_ms("2026-03-05T11:00:00.000Z"), 1.0, Some("a")),
                (iso_ms("2026-03-06T11:00:00.000Z"), 2.0, Some("b")),
                (iso_ms("2026-03-07T11:00:00.000Z"), 3.0, Some("a")),
            ],
        );
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now_afternoon(), 30);
        assert!(
            buckets
                .windows(2)
                .all(|w| (w[0].day_key.as_str(), w[0].model.as_str())
                    <= (w[1].day_key.as_str(), w[1].model.as_str())),
            "not sorted: {buckets:?}"
        );
        assert!(
            buckets
                .iter()
                .map(|b| b.day_key.as_str())
                .collect::<std::collections::HashSet<_>>()
                .len()
                >= 2
        );
    }

    #[test]
    fn zero_cost_rows_are_kept_and_aggregated() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(
            &db,
            &[
                (iso_ms("2026-03-06T11:00:00.000Z"), 0.0, Some("a")),
                (iso_ms("2026-03-06T12:00:00.000Z"), 4.0, Some("a")),
            ],
        );
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now_afternoon(), 30);
        assert_eq!(buckets.len(), 1, "{buckets:?}");
        assert!((buckets[0].cost - 4.0).abs() < 1e-6);
        assert_eq!(buckets[0].request_count, 2);
    }

    #[test]
    fn malformed_rows_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let conn = Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "m1",
                r#"{"providerID":"opencode-go","role":"user","cost":9,"time":{"created":1772798400000}}"#,
                1772798400000i64
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "m2",
                r#"{"providerID":"opencode-go","role":"assistant","cost":null,"modelID":"x","time":{"created":1772798400000}}"#,
                1772798400000i64
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, data, time_created) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "m3",
                r#"{"providerID":"opencode-go","role":"assistant","cost":7,"modelID":"good","time":{"created":1772798400000}}"#,
                1772798400000i64
            ],
        )
        .unwrap();
        drop(conn);

        let rows = read_rows(&db).unwrap();
        assert_eq!(rows.len(), 1, "only the valid assistant+cost row survives");
        assert_eq!(rows[0].model, "good");
    }

    #[test]
    fn rows_outside_history_window_are_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let far_past = iso_ms("2025-01-01T00:00:00.000Z");
        let recent = a14_now().timestamp_millis();
        write_message_db_with_model(
            &db,
            &[(far_past, 1.0, Some("old")), (recent, 2.0, Some("new"))],
        );
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now(), 1);
        let models: Vec<&str> = buckets.iter().map(|b| b.model.as_str()).collect();
        assert!(
            !models.contains(&"old"),
            "old row should be outside the 1-day window: {buckets:?}"
        );
    }

    #[test]
    fn token_summary_uses_the_same_history_window_as_cost_summary() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let now = a14_now();
        let recent = now.timestamp_millis();
        let old = recent - 3 * 24 * 60 * 60 * 1000;
        write_message_db_with_tokens(
            &db,
            &[
                (old, "old-model", 100, 20, 3, 4),
                (recent, "new-model", 200, 30, 5, 6),
            ],
        );

        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", now, 1).unwrap();
        assert_eq!(tokens.input, 200);
        assert_eq!(tokens.output, 30);
        assert_eq!(tokens.cached, 11);
        assert!(by_model.contains_key("new-model"));
        assert!(!by_model.contains_key("old-model"));
    }

    #[test]
    fn day_boundary_keys_by_local_calendar_day() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let just_after_utc_midnight = iso_ms("2026-03-06T00:30:00.000Z");
        write_message_db_with_model(&db, &[(just_after_utc_midnight, 1.5, Some("edge"))]);
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now_afternoon(), 30);
        assert_eq!(buckets.len(), 1, "{buckets:?}");
        assert!(
            NaiveDate::parse_from_str(&buckets[0].day_key, "%Y-%m-%d").is_ok(),
            "day_key not yyyy-MM-dd: {}",
            buckets[0].day_key
        );
    }

    #[test]
    fn model_cost_summary_aggregates_total_and_by_model() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(
            &db,
            &[
                (iso_ms("2026-03-06T11:00:00.000Z"), 3.0, Some("a")),
                (iso_ms("2026-03-06T12:00:00.000Z"), 1.0, Some("b")),
                (iso_ms("2026-03-06T13:00:00.000Z"), 2.0, None),
            ],
        );
        let rows = read_rows(&db).unwrap();
        let summary = model_cost_summary_from_rows(&rows, a14_now_afternoon(), 30);
        assert!((summary.total_cost_usd - 6.0).abs() < 1e-6, "{summary:?}");
        assert_eq!(summary.request_count, 3);
        assert!((summary.by_model["a"] - 3.0).abs() < 1e-6);
        assert!((summary.by_model["b"] - 1.0).abs() < 1e-6);
        assert!((summary.by_model[UNKNOWN_MODEL_NAME] - 2.0).abs() < 1e-6);
        assert!(summary.period_start.is_some() && summary.period_end.is_some());
    }

    #[test]
    fn daily_series_sums_models_per_day_via_pure_aggregation() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(
            &db,
            &[
                (iso_ms("2026-03-06T11:00:00.000Z"), 3.0, Some("a")),
                (iso_ms("2026-03-06T12:00:00.000Z"), 2.0, Some("b")),
            ],
        );
        let rows = read_rows(&db).unwrap();
        let buckets = daily_model_costs(&rows, a14_now_afternoon(), 30);
        let mut by_day: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
        for b in &buckets {
            *by_day.entry(b.day_key.clone()).or_insert(0.0) += b.cost;
        }
        let series: Vec<(String, f64)> = by_day.into_iter().collect();
        assert_eq!(series.len(), 1, "{series:?}");
        assert!((series[0].1 - 5.0).abs() < 1e-6);
    }

    #[test]
    fn daily_aggregation_is_independent_of_zen_wait() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        write_message_db_with_model(&db, &[(iso_ms("2026-03-06T11:00:00.000Z"), 2.0, Some("a"))]);
        let rows = read_rows(&db).unwrap();
        let now = a14_now();
        let b1 = daily_model_costs(&rows, now, 30);
        let b2 = daily_model_costs(&rows, now, 30);
        assert_eq!(b1, b2, "pure aggregation must be deterministic");
        assert_eq!(b1.len(), 1);
    }

    // ---- v2 session history (session_message / session_part) -------------

    #[test]
    fn v2_only_database_supplies_cost_tokens_and_speeds() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let base = iso_ms("2026-03-06T11:00:00.000Z");
        {
            let conn = Connection::open(&db).unwrap();
            create_v2_tables(&conn);
            insert_v2(
                &conn,
                V2Row {
                    id: "go-1",
                    model: Some("claude-sonnet-4-5"),
                    cost: Some(3.0),
                    tokens: Some((100, 50, 10, 5)),
                    created: Some(base),
                    completed: Some(base + 2_000),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "go-2",
                    model: Some("gpt-5.1-codex"),
                    cost: Some(1.0),
                    tokens: Some((7, 9, 0, 0)),
                    created: Some(base + 60_000),
                    completed: Some(base + 61_000),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "zen-1",
                    provider: "opencode",
                    model: Some("misc"),
                    cost: Some(9.0),
                    tokens: Some((1, 1, 1, 1)),
                    created: Some(base),
                    completed: Some(base + 1_000),
                    ..V2Row::default()
                },
            );
        }

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        let total: f64 = rows.iter().map(|row| row.cost).sum();
        assert!((total - 4.0).abs() < 1e-6, "{rows:?}");
        let models: Vec<&str> = rows.iter().map(|row| row.model.as_str()).collect();
        assert!(models.contains(&"claude-sonnet-4-5"), "{rows:?}");
        assert!(models.contains(&"gpt-5.1-codex"), "{rows:?}");

        // Provider filtering must not leak the sibling Zen subscription.
        let zen_rows = read_rows_for(&db, "opencode").unwrap();
        assert_eq!(zen_rows.len(), 1, "{zen_rows:?}");
        assert_eq!(zen_rows[0].model, "misc");
        assert!((zen_rows[0].cost - 9.0).abs() < 1e-6);

        let tokens_now = DateTime::from_timestamp_millis(base + 3_600_000).unwrap();
        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", tokens_now, 30).unwrap();
        assert_eq!(tokens.input, 107);
        assert_eq!(tokens.output, 59);
        assert_eq!(tokens.cached, 15);
        assert!(by_model.contains_key("claude-sonnet-4-5"), "{by_model:?}");
        assert!(by_model.contains_key("gpt-5.1-codex"), "{by_model:?}");

        let zen_tokens = read_tokens_for(&db, "opencode", tokens_now, 30).unwrap().0;
        assert_eq!(zen_tokens.input, 1);

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 2, "{speeds:?}");
        assert_eq!(speeds[0].completed_ms, base + 61_000);
        assert_eq!(speeds[0].output_tokens, 9);
        assert_eq!(speeds[0].model.as_deref(), Some("gpt-5.1-codex"));
        assert_eq!(speeds[1].completed_ms, base + 2_000);
    }

    #[test]
    fn v1_and_v2_rows_for_the_same_turn_count_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);",
            )
            .unwrap();
            create_v2_tables(&conn);
            insert_v1_message(
                &conn,
                "dup",
                &format!(
                    r#"{{"providerID":"opencode-go","role":"assistant","cost":2,"modelID":"v1-model","tokens":{{"input":5,"output":5,"cache":{{"read":1,"write":0}}}},"time":{{"created":{at},"completed":{}}}}}"#,
                    at + 800
                ),
                at,
            );
            insert_v1_message(
                &conn,
                "v1-only",
                &format!(
                    r#"{{"providerID":"opencode-go","role":"assistant","cost":3,"modelID":"v1-only-model","time":{{"created":{}}}}}"#,
                    at + 1_000
                ),
                at + 1_000,
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "dup",
                    model: Some("v2-model"),
                    cost: Some(2.0),
                    tokens: Some((5, 5, 1, 0)),
                    created: Some(at),
                    completed: Some(at + 800),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "v2-only",
                    cost: Some(4.0),
                    tokens: Some((1, 1, 0, 0)),
                    created: Some(at + 2_000),
                    completed: Some(at + 3_000),
                    ..V2Row::default()
                },
            );
        }

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 3, "{rows:?}");
        let total: f64 = rows.iter().map(|row| row.cost).sum();
        assert!(
            (total - 9.0).abs() < 1e-6,
            "duplicate turn double-counted: {rows:?}"
        );
        let models: Vec<&str> = rows.iter().map(|row| row.model.as_str()).collect();
        assert!(models.contains(&"v1-model"), "{rows:?}");
        assert!(!models.contains(&"v2-model"), "{rows:?}");

        let (tokens, _) = read_tokens_for(
            &db,
            "opencode-go",
            DateTime::from_timestamp_millis(at + 3_600_000).unwrap(),
            30,
        )
        .unwrap();
        assert_eq!(tokens.input, 6, "v2 duplicate must not double-count tokens");
        assert_eq!(tokens.output, 6);
        assert_eq!(tokens.cached, 1);

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 2, "{speeds:?}");
        assert_eq!(speeds[0].completed_ms, at + 3_000);
        assert!(
            speeds
                .iter()
                .all(|speed| speed.model.as_deref() != Some("v2-model")),
            "v2 duplicate must not duplicate speeds: {speeds:?}"
        );
    }

    // ---- one id, one source (cost/tokens/speed dedupe) --------------------

    #[test]
    fn v1_same_id_wins_for_cost_tokens_and_speed() {
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        let v1_data = format!(
            r#"{{"providerID":"opencode-go","role":"assistant","cost":2,"modelID":"v1-model","tokens":{{"input":100,"output":50,"cache":{{"read":10,"write":5}}}},"time":{{"created":{at},"completed":{}}}}}"#,
            at + 2_000
        );
        let (_dir, db) = mixed_v1_v2_db(
            "same-id",
            &[("turn-1", v1_data.as_str())],
            &[V2Row {
                id: "turn-1",
                model: Some("v2-model"),
                cost: Some(7.0),
                tokens: Some((900, 900, 900, 900)),
                created: Some(at),
                completed: Some(at + 9_000),
                ..V2Row::default()
            }],
        );

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!(
            (rows[0].cost - 2.0).abs() < 1e-6,
            "cost must come from v1: {rows:?}"
        );
        assert_eq!(rows[0].model, "v1-model");

        let now = DateTime::from_timestamp_millis(at + 3_600_000).unwrap();
        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", now, 30).unwrap();
        assert_eq!(tokens.input, 100, "tokens must come from v1: {tokens:?}");
        assert_eq!(tokens.output, 50, "{tokens:?}");
        assert_eq!(tokens.cached, 15, "{tokens:?}");
        assert!(by_model.contains_key("v1-model"), "{by_model:?}");
        assert!(!by_model.contains_key("v2-model"), "{by_model:?}");

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 1, "{speeds:?}");
        assert_eq!(speeds[0].output_tokens, 50, "speed must come from v1");
        assert_eq!(speeds[0].completed_ms, at + 2_000);
        assert_eq!(speeds[0].model.as_deref(), Some("v1-model"));
    }

    #[test]
    fn v1_cost_row_without_tokens_or_timing_does_not_borrow_v2_metrics() {
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        // v1 owns the id through a usable cost but records neither tokens nor a
        // completion time: those metrics stay empty instead of mixing schemas.
        let v1_data = format!(
            r#"{{"providerID":"opencode-go","role":"assistant","cost":3,"modelID":"v1-model","time":{{"created":{at}}}}}"#
        );
        let (_dir, db) = mixed_v1_v2_db(
            "v1-partial",
            &[("turn-1", v1_data.as_str())],
            &[V2Row {
                id: "turn-1",
                model: Some("v2-model"),
                cost: Some(5.0),
                tokens: Some((10, 20, 1, 1)),
                created: Some(at),
                completed: Some(at + 1_000),
                ..V2Row::default()
            }],
        );

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert!((rows[0].cost - 3.0).abs() < 1e-6, "{rows:?}");

        let now = DateTime::from_timestamp_millis(at + 3_600_000).unwrap();
        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", now, 30).unwrap();
        assert_eq!(
            tokens,
            TokenSums::default(),
            "v1 owns the id, so there is no v2 token fallback"
        );
        assert!(by_model.is_empty(), "{by_model:?}");

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert!(
            speeds.is_empty(),
            "v1 owns the id, so there is no v2 speed fallback: {speeds:?}"
        );
    }

    #[test]
    fn v2_supplies_every_metric_when_v1_has_no_usable_cost() {
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        // turn-1 is missing `cost` entirely, turn-2 carries a non-numeric one:
        // neither makes v1 authoritative, so all three metrics come from v2.
        let v1_missing_cost = format!(
            r#"{{"providerID":"opencode-go","role":"assistant","modelID":"v1-model","tokens":{{"input":1,"output":1,"cache":{{"read":0,"write":0}}}},"time":{{"created":{at},"completed":{}}}}}"#,
            at + 500
        );
        let v1_text_cost = format!(
            r#"{{"providerID":"opencode-go","role":"assistant","cost":"n/a","modelID":"v1-model","time":{{"created":{}}}}}"#,
            at + 10
        );
        let (_dir, db) = mixed_v1_v2_db(
            "v1-no-cost",
            &[
                ("turn-1", v1_missing_cost.as_str()),
                ("turn-2", v1_text_cost.as_str()),
            ],
            &[
                V2Row {
                    id: "turn-1",
                    model: Some("v2-model"),
                    cost: Some(4.0),
                    tokens: Some((10, 20, 1, 1)),
                    created: Some(at),
                    completed: Some(at + 1_000),
                    ..V2Row::default()
                },
                V2Row {
                    id: "turn-2",
                    model: Some("v2-model"),
                    cost: Some(6.0),
                    tokens: Some((100, 200, 10, 10)),
                    created: Some(at + 10),
                    completed: Some(at + 2_000),
                    ..V2Row::default()
                },
            ],
        );

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        let total: f64 = rows.iter().map(|row| row.cost).sum();
        assert!((total - 10.0).abs() < 1e-6, "{rows:?}");
        assert!(rows.iter().all(|row| row.model == "v2-model"), "{rows:?}");

        let now = DateTime::from_timestamp_millis(at + 3_600_000).unwrap();
        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", now, 30).unwrap();
        assert_eq!(
            tokens.input, 110,
            "v1 token row must not double-count: {tokens:?}"
        );
        assert_eq!(tokens.output, 220, "{tokens:?}");
        assert_eq!(tokens.cached, 22, "{tokens:?}");
        assert!(by_model.contains_key("v2-model"), "{by_model:?}");
        assert!(!by_model.contains_key("v1-model"), "{by_model:?}");

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 2, "{speeds:?}");
        assert!(
            speeds
                .iter()
                .all(|speed| speed.model.as_deref() == Some("v2-model")),
            "{speeds:?}"
        );
    }

    #[test]
    fn step_finish_only_v1_cost_keeps_legacy_priority_over_the_v2_copy() {
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);
                 CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, data TEXT, time_created INTEGER);",
            )
            .unwrap();
            create_v2_tables(&conn);
            // Message carries no `cost`; only the step-finish part is priced.
            insert_message(&conn, "turn-1", at, None, Some("v1-model"));
            insert_step_finish_part(&conn, "p1", "turn-1", at, 3.0);
            insert_v2(
                &conn,
                V2Row {
                    id: "turn-1",
                    model: Some("v2-model"),
                    cost: Some(9.0),
                    tokens: Some((10, 20, 1, 1)),
                    created: Some(at),
                    completed: Some(at + 1_000),
                    ..V2Row::default()
                },
            );
        }

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(
            rows.len(),
            1,
            "parts-priced turn must not be double-counted: {rows:?}"
        );
        assert!((rows[0].cost - 3.0).abs() < 1e-6, "{rows:?}");

        let now = DateTime::from_timestamp_millis(at + 3_600_000).unwrap();
        let (tokens, _) = read_tokens_for(&db, "opencode-go", now, 30).unwrap();
        assert_eq!(
            tokens,
            TokenSums::default(),
            "the parts-priced v1 turn owns the id"
        );
        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert!(speeds.is_empty(), "{speeds:?}");
    }

    /// Shared fixture: a v1 message priced at 5 with tokens/timing, one
    /// `step-finish` part under the same id, and a v2 copy of that turn.
    fn v1_message_with_priced_part_db(
        label: &str,
        part_cost: f64,
    ) -> (tempfile::TempDir, PathBuf, i64) {
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join(format!("opencode-{label}.db"));
        let v1_data = format!(
            r#"{{"providerID":"opencode-go","role":"assistant","cost":5,"modelID":"v1-model","tokens":{{"input":100,"output":50,"cache":{{"read":10,"write":5}}}},"time":{{"created":{at},"completed":{}}}}}"#,
            at + 2_000
        );
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE message (id TEXT PRIMARY KEY, data TEXT, time_created INTEGER);
                 CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT, data TEXT, time_created INTEGER);",
            )
            .unwrap();
            create_v2_tables(&conn);
            insert_v1_message(&conn, "turn-1", &v1_data, at);
            insert_step_finish_part(&conn, "p1", "turn-1", at, part_cost);
            insert_v2(
                &conn,
                V2Row {
                    id: "turn-1",
                    model: Some("v2-model"),
                    cost: Some(9.0),
                    tokens: Some((900, 900, 90, 90)),
                    created: Some(at),
                    completed: Some(at + 9_000),
                    ..V2Row::default()
                },
            );
        }
        (dir, db, at)
    }

    #[test]
    fn zero_cost_step_finish_part_keeps_v1_priority_over_the_v2_copy() {
        // The v1 cost of this turn resolves to 0 through the priced part, which
        // is exactly what the cost pass emits. `v1_usable` is derived from that
        // same row, so the id stays v1-owned: v2 must not take over cost, tokens
        // or speed even though its own cost is 9.
        let (_dir, db, at) = v1_message_with_priced_part_db("part-zero", 0.0);

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 1, "one row, from v1: {rows:?}");
        assert_eq!(
            rows[0].cost, 0.0,
            "the priced part is the v1 cost: {rows:?}"
        );
        assert_eq!(rows[0].model, "v1-model");

        let now = DateTime::from_timestamp_millis(at + 3_600_000).unwrap();
        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", now, 30).unwrap();
        assert_eq!(tokens.input, 100, "tokens stay on the v1 side: {tokens:?}");
        assert_eq!(tokens.output, 50, "{tokens:?}");
        assert_eq!(tokens.cached, 15, "{tokens:?}");
        assert!(by_model.contains_key("v1-model"), "{by_model:?}");
        assert!(!by_model.contains_key("v2-model"), "{by_model:?}");

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 1, "{speeds:?}");
        assert_eq!(speeds[0].output_tokens, 50, "speed stays on the v1 side");
        assert_eq!(speeds[0].completed_ms, at + 2_000);
        assert_eq!(speeds[0].model.as_deref(), Some("v1-model"));
    }

    #[test]
    fn negative_v1_cost_row_is_not_usable_so_v2_supplies_every_metric() {
        // A negative priced part makes the v1 cost row unusable. That id must
        // not suppress v2 (which would lose the turn's cost entirely), and the
        // negative row itself must not reach the totals.
        let (_dir, db, at) = v1_message_with_priced_part_db("part-negative", -2.0);

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 1, "only the v2 copy survives: {rows:?}");
        assert!(
            (rows[0].cost - 9.0).abs() < 1e-6,
            "cost must come from v2: {rows:?}"
        );
        assert_eq!(rows[0].model, "v2-model");
        assert!(
            rows.iter().all(|row| row.cost >= 0.0),
            "no negative v1 row may be emitted: {rows:?}"
        );

        let now = DateTime::from_timestamp_millis(at + 3_600_000).unwrap();
        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", now, 30).unwrap();
        assert_eq!(tokens.input, 900, "tokens must come from v2: {tokens:?}");
        assert_eq!(tokens.output, 900, "{tokens:?}");
        assert_eq!(tokens.cached, 180, "{tokens:?}");
        assert!(by_model.contains_key("v2-model"), "{by_model:?}");

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 1, "{speeds:?}");
        assert_eq!(speeds[0].output_tokens, 900, "speed must come from v2");
        assert_eq!(speeds[0].completed_ms, at + 9_000);
        assert_eq!(speeds[0].model.as_deref(), Some("v2-model"));
    }

    #[test]
    fn v1_rows_without_a_created_timestamp_are_skipped_not_fatal() {
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        let v1_null_ts = r#"{"providerID":"opencode-go","role":"assistant","cost":5,"modelID":"null-ts","tokens":{"input":7,"output":7,"cache":{"read":0,"write":0}}}"#;
        let v1_ok = format!(
            r#"{{"providerID":"opencode-go","role":"assistant","cost":2,"modelID":"ok-model","tokens":{{"input":3,"output":4,"cache":{{"read":1,"write":0}}}},"time":{{"created":{at},"completed":{}}}}}"#,
            at + 1_000
        );

        // Message-only v1: a NULL `createdMs` row must be dropped row by row, not
        // turn the whole pass into an error.
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        {
            let conn = Connection::open(&db).unwrap();
            create_v1_message_table(&conn);
            insert_v1_message_with_created(&conn, "null-ts", v1_null_ts, None);
            insert_v1_message_with_created(&conn, "ok", &v1_ok, Some(at));
        }

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 1, "unplaceable row skipped: {rows:?}");
        assert_eq!(rows[0].model, "ok-model");

        let now = DateTime::from_timestamp_millis(at + 3_600_000).unwrap();
        let (tokens, by_model) = read_tokens_for(&db, "opencode-go", now, 30).unwrap();
        assert_eq!(tokens.input, 3, "undated token row skipped: {tokens:?}");
        assert_eq!(tokens.output, 4, "{tokens:?}");
        assert_eq!(tokens.cached, 1, "{tokens:?}");
        assert!(!by_model.contains_key("null-ts"), "{by_model:?}");

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 1, "{speeds:?}");
        assert_eq!(speeds[0].output_tokens, 4, "{speeds:?}");

        // With a v2 copy present the undated v1 row is simply not v1-owned, so
        // v2 supplies that id — again with no whole-read failure.
        let (_mixed_dir, mixed) = mixed_v1_v2_db(
            "null-ts-mixed",
            &[("null-ts", v1_null_ts), ("ok", v1_ok.as_str())],
            &[V2Row {
                id: "null-ts",
                model: Some("v2-null-ts"),
                cost: Some(1.0),
                tokens: Some((11, 12, 1, 1)),
                created: Some(at),
                completed: Some(at + 500),
                ..V2Row::default()
            }],
        );

        let rows = read_rows_for(&mixed, "opencode-go").unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        let models: Vec<&str> = rows.iter().map(|row| row.model.as_str()).collect();
        assert!(models.contains(&"ok-model"), "{rows:?}");
        assert!(models.contains(&"v2-null-ts"), "{rows:?}");
        let total: f64 = rows.iter().map(|row| row.cost).sum();
        assert!((total - 3.0).abs() < 1e-6, "{rows:?}");

        let (tokens, by_model) = read_tokens_for(&mixed, "opencode-go", now, 30).unwrap();
        assert_eq!(tokens.input, 14, "{tokens:?}");
        assert_eq!(tokens.output, 16, "{tokens:?}");
        assert_eq!(tokens.cached, 3, "{tokens:?}");
        assert!(by_model.contains_key("v2-null-ts"), "{by_model:?}");

        let speeds = read_recent_message_speeds(&mixed, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 2, "{speeds:?}");
        assert!(
            speeds
                .iter()
                .any(|speed| speed.model.as_deref() == Some("v2-null-ts")),
            "{speeds:?}"
        );
    }

    #[test]
    fn v2_type_column_counts_assistant_and_compaction_but_not_user_turns() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let at = iso_ms("2026-03-06T11:00:00.000Z");
        {
            let conn = Connection::open(&db).unwrap();
            create_v2_tables(&conn);
            insert_v2(
                &conn,
                V2Row {
                    id: "user-1",
                    kind: "user",
                    cost: Some(5.0),
                    created: Some(at),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "assistant-1",
                    cost: Some(2.0),
                    created: Some(at + 1_000),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "compaction-1",
                    kind: "compaction",
                    cost: Some(1.0),
                    created: Some(at + 2_000),
                    ..V2Row::default()
                },
            );
        }

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 2, "user turns are not billable: {rows:?}");
        let total: f64 = rows.iter().map(|row| row.cost).sum();
        assert!((total - 3.0).abs() < 1e-6, "{rows:?}");
    }

    #[test]
    fn v2_blank_model_and_missing_tokens_keep_the_cost_readable() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let at = a14_now().timestamp_millis();
        {
            let conn = Connection::open(&db).unwrap();
            create_v2_tables(&conn);
            insert_v2(
                &conn,
                V2Row {
                    id: "blank-model",
                    model: Some("   "),
                    cost: Some(2.5),
                    created: Some(at - 60_000),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "no-model",
                    cost: Some(1.5),
                    created: Some(at - 30_000),
                    ..V2Row::default()
                },
            );
        }

        let rows = read_rows_for(&db, "opencode-go").unwrap();
        assert_eq!(rows.len(), 2, "{rows:?}");
        let buckets = daily_model_costs(&rows, a14_now_afternoon(), 30);
        assert_eq!(buckets.len(), 1, "{buckets:?}");
        assert_eq!(buckets[0].model, UNKNOWN_MODEL_NAME);
        assert!((buckets[0].cost - 4.0).abs() < 1e-6);
        assert_eq!(buckets[0].request_count, 2);

        // Missing `data.tokens` leaves the cost untouched and the token pass at 0.
        let (tokens, by_model) =
            read_tokens_for(&db, "opencode-go", a14_now_afternoon(), 30).unwrap();
        assert_eq!(tokens, TokenSums::default());
        assert!(by_model.is_empty(), "{by_model:?}");
    }

    #[test]
    fn v2_unfinished_messages_are_skipped_by_the_speed_reader() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        {
            let conn = Connection::open(&db).unwrap();
            create_v2_tables(&conn);
            insert_v2(
                &conn,
                V2Row {
                    id: "done-old",
                    model: Some("m-old"),
                    tokens: Some((0, 100, 0, 0)),
                    created: Some(1_000),
                    completed: Some(2_000),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "done-new",
                    model: Some("m-new"),
                    tokens: Some((0, 200, 0, 0)),
                    created: Some(3_000),
                    completed: Some(4_000),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "in-progress",
                    model: Some("m-progress"),
                    tokens: Some((0, 300, 0, 0)),
                    created: Some(5_000),
                    ..V2Row::default()
                },
            );
            insert_v2(
                &conn,
                V2Row {
                    id: "no-output",
                    model: Some("m-zero"),
                    tokens: Some((0, 0, 0, 0)),
                    created: Some(6_000),
                    completed: Some(7_000),
                    ..V2Row::default()
                },
            );
        }

        let speeds = read_recent_message_speeds(&db, "opencode-go", 20).unwrap();
        assert_eq!(speeds.len(), 2, "{speeds:?}");
        assert_eq!(speeds[0].model.as_deref(), Some("m-new"));
        assert_eq!(speeds[1].model.as_deref(), Some("m-old"));
        assert_eq!(speeds[0].completed_ms, 4_000);
        assert_eq!(speeds[0].output_tokens, 200);
    }

    #[test]
    fn databases_without_message_tables_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch("CREATE TABLE unrelated (id TEXT);")
                .unwrap();
        }

        let errors = [
            read_rows_for(&db, "opencode-go").unwrap_err().to_string(),
            read_tokens_for(&db, "opencode-go", Utc::now(), 30)
                .unwrap_err()
                .to_string(),
            read_recent_message_speeds(&db, "opencode-go", 10)
                .unwrap_err()
                .to_string(),
        ];
        for message in errors {
            assert!(
                message.contains("no OpenCode message tables found"),
                "not diagnostic: {message}"
            );
        }
    }

    #[test]
    fn opencode_db_env_resolves_without_creating_files() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let xdg = dir.path().join("xdg");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&xdg).unwrap();

        // Absolute override is exclusive: the first (only) slot, no disk writes.
        let absolute = dir.path().join("custom").join("history.db");
        let candidates = candidate_paths_with(
            |name| match name {
                "OPENCODE_DB" => Some(absolute.to_string_lossy().to_string()),
                _ => None,
            },
            Some(home.clone()),
        );
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].1, absolute);
        assert_eq!(
            candidates[0].0,
            absolute.parent().unwrap().join("auth.json")
        );
        assert!(!absolute.exists(), "override must not be created");
        assert!(
            !absolute.parent().unwrap().exists(),
            "no parent dirs either"
        );

        // Relative override rides on an absolute XDG_DATA_HOME.
        let candidates = candidate_paths_with(
            |name| match name {
                "OPENCODE_DB" => Some("custom.db".to_string()),
                "XDG_DATA_HOME" => Some(xdg.to_string_lossy().to_string()),
                _ => None,
            },
            Some(home.clone()),
        );
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].1, xdg.join("opencode").join("custom.db"));
        assert_eq!(candidates[0].0, xdg.join("opencode").join("auth.json"));
        assert!(!xdg.join("opencode").exists(), "must not create the base");

        // Without XDG the relative override falls back to ~/.local/share/opencode.
        let candidates = candidate_paths_with(
            |name| match name {
                "OPENCODE_DB" => Some("nested/custom.db".to_string()),
                _ => None,
            },
            Some(home.clone()),
        );
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        let expected = home
            .join(".local")
            .join("share")
            .join("opencode")
            .join("nested")
            .join("custom.db");
        assert_eq!(candidates[0].1, expected);
        assert!(!home.join(".local").exists(), "must not create the base");

        // `:memory:` is never treated as a database file or an install, and it
        // suppresses scanning entirely.
        let candidates = candidate_paths_with(
            |name| match name {
                "OPENCODE_DB" => Some(":memory:".to_string()),
                _ => None,
            },
            Some(home.clone()),
        );
        assert!(candidates.is_empty(), "{candidates:?}");
        assert!(
            candidates
                .iter()
                .all(|(_, db)| db.as_path() != Path::new(":memory:")),
            "{candidates:?}"
        );
        assert!(!Path::new(":memory:").exists());
    }

    #[test]
    fn opencode_db_override_never_falls_back_to_a_scanned_install() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let base = home.join(".local").join("share").join("opencode");
        std::fs::create_dir_all(&base).unwrap();
        // A readable-looking install that the override must not touch.
        let scanned = base.join("opencode.db");
        std::fs::write(&scanned, b"scanned install").unwrap();

        let missing = dir.path().join("missing.db");
        let candidates = candidate_paths_with(
            |name| match name {
                "OPENCODE_DB" => Some(missing.to_string_lossy().to_string()),
                _ => None,
            },
            Some(home.clone()),
        );

        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].1, missing);
        assert_eq!(candidates[0].0, dir.path().join("auth.json"));
        assert!(
            candidates.iter().all(|(_, db)| db != &scanned),
            "override must not fall back to another account library: {candidates:?}"
        );
        assert!(!missing.exists(), "override must not be created");
        // And the resulting read fails instead of reading the scanned database.
        assert!(fetch_from_paths(&candidates[0].0, &candidates[0].1, Utc::now()).is_err());

        // A relative override resolves inside the opencode root and still never
        // lands on the scanned `opencode.db`.
        let candidates = candidate_paths_with(
            |name| match name {
                "OPENCODE_DB" => Some("missing.db".to_string()),
                _ => None,
            },
            Some(home.clone()),
        );
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(candidates[0].1, base.join("missing.db"));
        assert!(
            candidates.iter().all(|(_, db)| db != &scanned),
            "{candidates:?}"
        );
    }

    #[test]
    fn opencode_db_memory_disables_local_history() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let base = home.join(".local").join("share").join("opencode");
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(base.join("opencode.db"), b"scanned install").unwrap();

        let candidates = candidate_paths_with(
            |name| match name {
                "OPENCODE_DB" => Some(":memory:".to_string()),
                _ => None,
            },
            Some(home),
        );
        assert!(
            candidates.is_empty(),
            "`:memory:` must not scan installs: {candidates:?}"
        );
    }

    #[test]
    fn relative_xdg_data_home_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();

        let candidates = candidate_paths_with(
            |name| match name {
                "XDG_DATA_HOME" => Some("relative-xdg".to_string()),
                _ => None,
            },
            Some(home.clone()),
        );

        assert!(
            candidates.iter().all(|(_, db)| db.is_absolute()),
            "relative XDG must not produce relative candidates: {candidates:?}"
        );
        assert!(
            candidates
                .iter()
                .all(|(_, db)| !db.starts_with("relative-xdg")),
            "{candidates:?}"
        );
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        assert_eq!(
            candidates[0].1,
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("opencode.db")
        );
        assert_eq!(
            candidates[0].0,
            home.join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json")
        );
    }

    #[test]
    fn idle_wal_v2_database_read_creates_no_sidecars() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("opencode.db");
        let at = Utc::now().timestamp_millis() - 60_000;
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
            create_v2_tables(&conn);
            insert_v2(
                &conn,
                V2Row {
                    id: "go-1",
                    model: Some("m"),
                    cost: Some(3.0),
                    tokens: Some((10, 20, 0, 0)),
                    created: Some(at),
                    completed: Some(at + 1_000),
                    ..V2Row::default()
                },
            );
            let _checkpointed = conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
            drop(conn);
        }

        let wal = crate::core::sqlite_sidecar_path(&db, "-wal");
        let shm = crate::core::sqlite_sidecar_path(&db, "-shm");
        for side in [&wal, &shm] {
            if side.exists() {
                let parked = side.with_extension("parked");
                let _parked = std::fs::rename(side, parked);
            }
        }
        assert!(!wal.exists() && !shm.exists(), "precondition: no sidecars");

        let rows = read_rows_for(&db, "opencode-go").expect("read v2 idle WAL db");
        assert_eq!(rows.len(), 1, "{rows:?}");
        let speeds = read_recent_message_speeds(&db, "opencode-go", 10).expect("read speeds");
        assert_eq!(speeds.len(), 1, "{speeds:?}");

        assert!(
            !wal.exists() && !shm.exists(),
            "reader must not create -wal/-shm sidecars"
        );
    }
}
