use crate::paths::get_db_path;
use crate::revision::{
    canonical_live_payload, canonical_tombstone_payload, deterministic_conflict_uuid, sha256_hex,
};
use once_cell::sync::OnceCell;
use rusqlite::{Connection, DatabaseName, OptionalExtension, Result as SqliteResult, Row};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

static DB: OnceCell<Mutex<Connection>> = OnceCell::new();

const SCHEMA_VERSION: i64 = 4;
const EXPORT_FORMAT_ID: &str = "snipvault.snippets";
const EXPORT_SCHEMA_VERSION: u32 = 1;
const OUTBOX_KIND_UPSERT: &str = "upsert";
const OUTBOX_KIND_DELETE: &str = "delete";
const REVISION_ORIGIN_LOCAL: &str = "local";
const REVISION_ORIGIN_IMPORT: &str = "import";
const REVISION_ORIGIN_REMOTE: &str = "remote";
pub const MAX_PENDING_OUTBOX_COUNT: usize = 10_000;
pub const MAX_PENDING_OUTBOX_BYTES: usize = 64 * 1024 * 1024;
const MAX_REVISION_PAYLOAD_BYTES: usize = MAX_CONTENT_BYTES + 256 * 1024;
const MAX_SYNC_ANCESTRY_OBJECTS: usize = 25_000;
const MAX_SYNC_ANCESTRY_BYTES: usize = 64 * 1024 * 1024;
const CONTENT_PREVIEW_BYTES: usize = 768;
const DEFAULT_PAGE_SIZE: usize = 100;
const MAX_PAGE_SIZE: usize = 200;
const MAX_QUERY_CHARS: usize = 1_000;
const CURSOR_SEPARATOR: char = '\u{1f}';
const MAX_IMPORT_BYTES: usize = 25 * 1024 * 1024;
const MAX_IMPORT_ITEMS: usize = 10_000;
const MAX_ID_BYTES: usize = 256;
const MAX_TITLE_CHARS: usize = 512;
const MAX_CONTENT_BYTES: usize = 10 * 1024 * 1024;
const MAX_DESCRIPTION_CHARS: usize = 100_000;
const MAX_LANGUAGE_CHARS: usize = 64;
const MAX_TAGS: usize = 100;
const MAX_TAG_CHARS: usize = 256;

pub fn init_db() -> SqliteResult<()> {
    let _ = DB.get_or_try_init(|| {
        let db_path = get_db_path();
        log::info!("Initializing database");
        let parent = db_path.parent().ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("database path has no parent directory".into())
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            rusqlite::Error::InvalidParameterName(format!(
                "failed to create database directory: {error}"
            ))
        })?;
        let conn = open_database_with_recovery(&db_path)?;

        log::info!("Database initialized successfully");
        Ok::<Mutex<Connection>, rusqlite::Error>(Mutex::new(conn))
    })?;
    Ok(())
}

fn schema_version(conn: &Connection) -> SqliteResult<i64> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

fn open_database_with_recovery(path: &Path) -> SqliteResult<Connection> {
    let existed_before_open = path.exists();
    let mut conn = Connection::open(path)?;
    let version = schema_version(&conn)?;
    if version > SCHEMA_VERSION {
        return Err(unsupported_schema_error(version));
    }

    if !existed_before_open || version == SCHEMA_VERSION {
        initialize_connection(&mut conn)?;
        return Ok(conn);
    }

    let backup_path = create_preflight_backup(&conn, path, version, SCHEMA_VERSION)?;
    match initialize_connection(&mut conn) {
        Ok(()) => Ok(conn),
        Err(_migration_error) => {
            log::error!("Database migration failed; attempting preflight restore");
            drop(conn);
            restore_preflight_backup(path, &backup_path, version)?;
            Err(rusqlite::Error::InvalidParameterName(
                "database migration failed; preflight backup restored".into(),
            ))
        }
    }
}

fn unsupported_schema_error(version: i64) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(format!(
        "database schema version {version} is newer than supported version {SCHEMA_VERSION}"
    ))
}

fn unique_sibling_path(path: &Path, label: &str, extension: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("snippets.db");
    let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.6fZ");
    for suffix in 0_u32.. {
        let suffix = if suffix == 0 {
            String::new()
        } else {
            format!("-{suffix}")
        };
        let candidate = parent.join(format!(".{stem}.{label}-{timestamp}{suffix}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded unique path allocation")
}

fn create_preflight_backup(
    conn: &Connection,
    db_path: &Path,
    source_version: i64,
    target_version: i64,
) -> SqliteResult<PathBuf> {
    let label = format!("pre-v{target_version}");
    let backup_path = unique_sibling_path(db_path, &label, "bak");
    conn.backup(DatabaseName::Main, &backup_path, None)?;
    let backup = Connection::open(&backup_path)?;
    let integrity: String = backup.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" || schema_version(&backup)? != source_version {
        let _ = fs::remove_file(&backup_path);
        return Err(rusqlite::Error::InvalidParameterName(
            "database preflight backup verification failed".into(),
        ));
    }
    Ok(backup_path)
}

fn restore_preflight_backup(
    db_path: &Path,
    backup_path: &Path,
    source_version: i64,
) -> SqliteResult<()> {
    let failed_path = unique_sibling_path(db_path, "migration-failed", "db");
    if db_path.exists() {
        fs::rename(db_path, &failed_path).map_err(|error| {
            rusqlite::Error::InvalidParameterName(format!(
                "failed to preserve unsuccessful migration database: {error}"
            ))
        })?;
    }

    if let Err(error) = fs::copy(backup_path, db_path) {
        let _ = fs::rename(&failed_path, db_path);
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "failed to restore database preflight backup: {error}"
        )));
    }

    let verification = (|| -> SqliteResult<()> {
        let restored = Connection::open(db_path)?;
        let integrity: String =
            restored.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" || schema_version(&restored)? != source_version {
            return Err(rusqlite::Error::InvalidParameterName(
                "restored database backup did not pass verification".into(),
            ));
        }
        Ok(())
    })();

    if let Err(error) = verification {
        let _ = fs::remove_file(db_path);
        let _ = fs::rename(&failed_path, db_path);
        return Err(error);
    }
    Ok(())
}

fn initialize_connection(conn: &mut Connection) -> SqliteResult<()> {
    let mut version = schema_version(conn)?;
    if version > SCHEMA_VERSION {
        return Err(unsupported_schema_error(version));
    }

    if version < 1 {
        migrate_to_v1(conn)?;
        version = 1;
    }
    if version == 1 {
        migrate_to_v2(conn)?;
        version = 2;
    }
    if version == 2 {
        migrate_to_v3(conn)?;
        version = 3;
    }
    if version == 3 {
        migrate_to_v4(conn)?;
    }

    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> SqliteResult<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        rusqlite::params![table],
        |row| row.get(0),
    )
}

fn migrate_to_v1(conn: &mut Connection) -> SqliteResult<()> {
    let had_snippets_table = table_exists(conn, "snippets")?;
    let tx = conn.transaction()?;

    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS snippets (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL DEFAULT '',
            content TEXT NOT NULL DEFAULT '',
            language TEXT NOT NULL DEFAULT 'plaintext',
            description TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '[]',
            is_favorite INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sync_versions (
            id TEXT PRIMARY KEY,
            synced_at TEXT NOT NULL,
            direction TEXT NOT NULL,
            snippet_count INTEGER NOT NULL DEFAULT 0,
            uploaded_count INTEGER NOT NULL DEFAULT 0,
            downloaded_count INTEGER NOT NULL DEFAULT 0,
            message TEXT NOT NULL DEFAULT ''
        );
        ",
    )?;

    // Seed only a genuinely new database. An existing empty table represents a
    // valid user state and must remain empty after restart.
    if !had_snippets_table {
        insert_samples(&tx)?;
    }

    tx.pragma_update(None, "user_version", 1)?;
    tx.commit()
}

fn migrate_to_v2(conn: &mut Connection) -> SqliteResult<()> {
    let tx = conn.transaction()?;

    // A v1 database may have been modified outside SnipVault. Decode every row
    // strictly before creating or backfilling the index so damage aborts the
    // entire migration rather than becoming missing search data.
    {
        let mut stmt = tx.prepare(
            "SELECT id, title, content, language, description, tags, is_favorite, created_at, updated_at, 'legacy-pending' FROM snippets",
        )?;
        let rows = stmt.query_map([], |row| Snippet::try_from(row))?;
        for row in rows {
            row?;
        }
    }
    {
        let mut stmt = tx.prepare(
            "SELECT id, synced_at, direction, snippet_count, uploaded_count, downloaded_count,
                    0, 0, 1, 0, message FROM sync_versions",
        )?;
        let rows = stmt.query_map([], |row| SyncVersion::try_from(row))?;
        for row in rows {
            row?;
        }
    }

    tx.execute_batch(
        "CREATE TABLE app_metadata (
             key TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );",
    )?;

    let tokenizer = match tx.execute_batch(
        "CREATE VIRTUAL TABLE snippets_fts USING fts5(
             title, content, description, tags,
             content='snippets', content_rowid='rowid', tokenize='trigram'
         );",
    ) {
        Ok(()) => "trigram",
        Err(error) => {
            log::warn!("SQLite trigram tokenizer unavailable; using unicode61 fallback: {error}");
            tx.execute_batch(
                "DROP TABLE IF EXISTS snippets_fts;
                 CREATE VIRTUAL TABLE snippets_fts USING fts5(
                     title, content, description, tags,
                     content='snippets', content_rowid='rowid', tokenize='unicode61'
                 );",
            )?;
            "unicode61"
        }
    };

    tx.execute(
        "INSERT INTO app_metadata (key, value) VALUES ('fts_tokenizer', ?1)",
        [tokenizer],
    )?;
    tx.execute_batch(
        "CREATE TRIGGER snippets_fts_ai AFTER INSERT ON snippets BEGIN
             INSERT INTO snippets_fts(rowid, title, content, description, tags)
             VALUES (new.rowid, new.title, new.content, new.description, new.tags);
         END;
         CREATE TRIGGER snippets_fts_ad AFTER DELETE ON snippets BEGIN
             INSERT INTO snippets_fts(snippets_fts, rowid, title, content, description, tags)
             VALUES ('delete', old.rowid, old.title, old.content, old.description, old.tags);
         END;
         CREATE TRIGGER snippets_fts_au AFTER UPDATE ON snippets BEGIN
             INSERT INTO snippets_fts(snippets_fts, rowid, title, content, description, tags)
             VALUES ('delete', old.rowid, old.title, old.content, old.description, old.tags);
             INSERT INTO snippets_fts(rowid, title, content, description, tags)
             VALUES (new.rowid, new.title, new.content, new.description, new.tags);
         END;
         INSERT INTO snippets_fts(rowid, title, content, description, tags)
         SELECT rowid, title, content, description, tags FROM snippets;",
    )?;

    tx.pragma_update(None, "user_version", 2)?;
    tx.commit()
}

fn migrate_to_v3(conn: &mut Connection) -> SqliteResult<()> {
    let tx = conn.transaction()?;
    let device_id = uuid::Uuid::new_v4().to_string();

    tx.execute_batch(
        "ALTER TABLE sync_versions ADD COLUMN deleted_count INTEGER NOT NULL DEFAULT 0 CHECK(deleted_count >= 0);
         ALTER TABLE sync_versions ADD COLUMN conflict_count INTEGER NOT NULL DEFAULT 0 CHECK(conflict_count >= 0);
         ALTER TABLE sync_versions ADD COLUMN protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version >= 1);
         ALTER TABLE sync_versions ADD COLUMN generation INTEGER NOT NULL DEFAULT 0 CHECK(generation >= 0);

         CREATE TABLE sync_identity (
             singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
             device_id TEXT NOT NULL UNIQUE CHECK(length(device_id) BETWEEN 1 AND 128),
             created_at TEXT NOT NULL
         );

         CREATE TABLE snippet_heads (
             snippet_id TEXT PRIMARY KEY,
             revision_id TEXT NOT NULL UNIQUE,
             parent_revision_id TEXT,
             device_id TEXT NOT NULL,
             content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
             revision_time TEXT NOT NULL,
             deleted INTEGER NOT NULL CHECK(deleted IN (0, 1))
         );

         CREATE TABLE revision_outbox (
             sequence INTEGER PRIMARY KEY AUTOINCREMENT,
             revision_id TEXT NOT NULL UNIQUE,
             snippet_id TEXT NOT NULL,
             parent_revision_id TEXT,
             device_id TEXT NOT NULL,
             content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
             revision_time TEXT NOT NULL,
             deleted INTEGER NOT NULL CHECK(deleted IN (0, 1)),
             operation_kind TEXT NOT NULL CHECK(operation_kind IN ('upsert', 'delete')),
             origin TEXT NOT NULL CHECK(origin IN ('local', 'import', 'remote', 'conflict')),
             payload_json TEXT NOT NULL,
             payload_bytes INTEGER NOT NULL CHECK(payload_bytes >= 0),
             created_at TEXT NOT NULL
         );
         CREATE INDEX revision_outbox_snippet_sequence_idx
             ON revision_outbox(snippet_id, sequence);
         CREATE TRIGGER revision_outbox_immutable
             BEFORE UPDATE ON revision_outbox
             BEGIN SELECT RAISE(ABORT, 'revision outbox rows are immutable'); END;

         CREATE TABLE sync_conflicts (
             conflict_id TEXT PRIMARY KEY,
             source_snippet_id TEXT NOT NULL,
             local_revision_id TEXT NOT NULL,
             incoming_revision_id TEXT NOT NULL,
             conflict_snippet_id TEXT NOT NULL UNIQUE,
             detected_at TEXT NOT NULL,
             UNIQUE(source_snippet_id, local_revision_id, incoming_revision_id)
         );
         CREATE INDEX sync_conflicts_source_idx
             ON sync_conflicts(source_snippet_id, detected_at);

         CREATE TABLE sync_remote_state (
             remote_id TEXT PRIMARY KEY,
             protocol_version INTEGER NOT NULL DEFAULT 1 CHECK(protocol_version >= 1),
             manifest_etag TEXT,
             manifest_hash TEXT,
             generation INTEGER NOT NULL DEFAULT 0 CHECK(generation >= 0),
             bootstrap_state TEXT NOT NULL DEFAULT 'pending'
                 CHECK(bootstrap_state IN ('pending', 'ready', 'blocked')),
             last_success_at TEXT,
             updated_at TEXT NOT NULL
         );",
    )?;
    tx.execute(
        "INSERT INTO sync_identity(singleton, device_id, created_at) VALUES (1, ?1, ?2)",
        rusqlite::params![device_id, chrono::Utc::now().to_rfc3339()],
    )?;

    let snippets = {
        let mut statement = tx.prepare(
            "SELECT s.id, s.title, s.content, s.language, s.description, s.tags, s.is_favorite, s.created_at, s.updated_at, 'legacy-pending'
             FROM snippets s ORDER BY s.id",
        )?;
        let rows = statement.query_map([], |row| Snippet::try_from(row))?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };
    for snippet in snippets {
        let payload = canonical_revision_payload(&snippet, false)?;
        let hash = sha256_hex(payload.as_bytes());
        let revision_id = legacy_revision_id(&snippet.id, &hash, &snippet.updated_at);
        tx.execute(
            "INSERT INTO snippet_heads
             (snippet_id, revision_id, parent_revision_id, device_id, content_hash, revision_time, deleted)
             VALUES (?1, ?2, NULL, 'legacy-v2', ?3, ?4, 0)",
            rusqlite::params![snippet.id, revision_id, hash, snippet.updated_at],
        )?;
    }

    tx.pragma_update(None, "user_version", 3)?;
    tx.commit()
}

fn migrate_to_v4(conn: &mut Connection) -> SqliteResult<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "CREATE TABLE revision_objects (
             revision_id TEXT PRIMARY KEY,
             snippet_id TEXT NOT NULL,
             parent_revision_id TEXT,
             device_id TEXT NOT NULL,
             content_hash TEXT NOT NULL CHECK(length(content_hash) = 64),
             revision_time TEXT NOT NULL,
             deleted INTEGER NOT NULL CHECK(deleted IN (0, 1)),
             origin TEXT NOT NULL CHECK(origin IN ('local', 'import', 'remote', 'conflict')),
             payload_json TEXT NOT NULL,
             payload_bytes INTEGER NOT NULL CHECK(payload_bytes >= 0),
             conflict_of TEXT,
             created_at TEXT NOT NULL
         );
         CREATE INDEX revision_objects_snippet_time_idx
             ON revision_objects(snippet_id, revision_time, revision_id);
         CREATE TRIGGER revision_objects_immutable
             BEFORE UPDATE ON revision_objects
             BEGIN SELECT RAISE(ABORT, 'revision object rows are immutable'); END;",
    )?;

    let outbox_rows = {
        let mut statement = tx.prepare(
            "SELECT revision_id, snippet_id, parent_revision_id, device_id, content_hash,
                    revision_time, deleted, origin, payload_json, payload_bytes, created_at
             FROM revision_outbox ORDER BY sequence",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };
    for (
        revision_id,
        snippet_id,
        parent_revision_id,
        device_id,
        content_hash,
        revision_time,
        deleted,
        origin,
        payload_json,
        payload_bytes,
        created_at,
    ) in outbox_rows
    {
        if !validate_revision_token(&revision_id)
            || snippet_id.is_empty()
            || snippet_id.len() > MAX_ID_BYTES
            || parent_revision_id
                .as_deref()
                .is_some_and(|value| !validate_revision_token(value))
            || !validate_revision_token(&device_id)
            || content_hash.len() != 64
            || !content_hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            || chrono::DateTime::parse_from_rfc3339(&revision_time).is_err()
            || !matches!(deleted, 0 | 1)
            || !matches!(origin.as_str(), "local" | "import" | "remote" | "conflict")
            || usize::try_from(payload_bytes).ok().is_none_or(|bytes| {
                bytes != payload_json.len() || bytes > MAX_REVISION_PAYLOAD_BYTES
            })
            || serde_json::from_str::<serde_json::Value>(&payload_json).is_err()
            || sha256_hex(payload_json.as_bytes()) != content_hash
            || chrono::DateTime::parse_from_rfc3339(&created_at).is_err()
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "schema v4 outbox-object backfill validation failed".into(),
            ));
        }
        tx.execute(
            "INSERT INTO revision_objects
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash, revision_time,
              deleted, origin, payload_json, payload_bytes, conflict_of, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11)",
            rusqlite::params![
                revision_id,
                snippet_id,
                parent_revision_id,
                device_id,
                content_hash,
                revision_time,
                deleted,
                origin,
                payload_json,
                payload_bytes,
                created_at
            ],
        )?;
    }

    let snippets = {
        let mut statement = tx.prepare(
            "SELECT s.id, s.title, s.content, s.language, s.description, s.tags, s.is_favorite,
                    s.created_at, s.updated_at, h.revision_id, h.parent_revision_id, h.device_id,
                    h.content_hash, h.revision_time
             FROM snippets s JOIN snippet_heads h ON h.snippet_id=s.id AND h.deleted=0
             WHERE NOT EXISTS (
                 SELECT 1 FROM revision_objects r WHERE r.revision_id=h.revision_id
             )
             ORDER BY s.id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                Snippet::try_from(row)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, String>(11)?,
                row.get::<_, String>(12)?,
                row.get::<_, String>(13)?,
            ))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };
    for (snippet, parent_revision_id, device_id, content_hash, revision_time) in snippets {
        let payload = canonical_revision_payload(&snippet, false)?;
        if payload.len() > MAX_REVISION_PAYLOAD_BYTES
            || sha256_hex(payload.as_bytes()) != content_hash
            || snippet.updated_at != revision_time
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "schema v4 revision-object backfill validation failed".into(),
            ));
        }
        tx.execute(
            "INSERT INTO revision_objects
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash, revision_time,
              deleted, origin, payload_json, payload_bytes, conflict_of, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, 'local', ?7, ?8, NULL, ?6)",
            rusqlite::params![
                snippet.revision_id,
                snippet.id,
                parent_revision_id,
                device_id,
                content_hash,
                revision_time,
                payload,
                payload.len() as i64
            ],
        )?;
    }

    let tombstones = {
        let mut statement = tx.prepare(
            "SELECT h.snippet_id, h.revision_id, h.parent_revision_id, h.device_id,
                    h.content_hash, h.revision_time
             FROM snippet_heads h
             WHERE h.deleted=1 AND NOT EXISTS (
                 SELECT 1 FROM revision_objects r WHERE r.revision_id=h.revision_id
             )
             ORDER BY h.snippet_id",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };
    for (snippet_id, revision_id, parent_revision_id, device_id, content_hash, revision_time) in
        tombstones
    {
        let payload = canonical_tombstone_payload_sqlite(&snippet_id, &revision_time)?;
        if payload.len() > MAX_REVISION_PAYLOAD_BYTES
            || sha256_hex(payload.as_bytes()) != content_hash
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "schema v4 tombstone backfill validation failed".into(),
            ));
        }
        tx.execute(
            "INSERT INTO revision_objects
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash, revision_time,
              deleted, origin, payload_json, payload_bytes, conflict_of, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 'local', ?7, ?8, NULL, ?6)",
            rusqlite::params![
                revision_id,
                snippet_id,
                parent_revision_id,
                device_id,
                content_hash,
                revision_time,
                payload,
                payload.len() as i64
            ],
        )?;
    }

    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()
}

fn insert_samples(conn: &Connection) -> SqliteResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let samples = vec![
        ("Hello World", "puts \"Hello, World!\"", "ruby", "Ruby 示例"),
        ("Quick Sort", "fn quick_sort<T: Ord>(arr: &mut [T]) {\n    if arr.len() <= 1 { return; }\n    let pivot = arr.len() - 1;\n    let mut i = 0;\n    for j in 0..pivot {\n        if arr[j] <= arr[pivot] {\n            arr.swap(i, j);\n            i += 1;\n        }\n    }\n    arr.swap(i, pivot);\n    let (left, right) = arr.split_at_mut(i);\n    quick_sort(left);\n    quick_sort(right);\n}", "rust", "Rust 快速排序实现"),
        ("React Hook", "import { useState, useEffect } from 'react';\n\nexport function useDebounce<T>(value: T, delay: number): T {\n  const [debouncedValue, setDebouncedValue] = useState<T>(value);\n\n  useEffect(() => {\n    const timer = setTimeout(() => {\n      setDebouncedValue(value);\n    }, delay);\n\n    return () => clearTimeout(timer);\n  }, [value, delay]);\n\n  return debouncedValue;\n}", "typescript", "React 防抖 Hook"),
        ("Python Decorator", "from functools import wraps\nimport time\n\ndef retry(max_attempts=3, delay=1):\n    def decorator(func):\n        @wraps(func)\n        def wrapper(*args, **kwargs):\n            attempts = 0\n            while attempts < max_attempts:\n                try:\n                    return func(*args, **kwargs)\n                except Exception as e:\n                    attempts += 1\n                    if attempts >= max_attempts:\n                        raise\n                    time.sleep(delay * attempts)\n            return func(*args, **kwargs)\n        return wrapper\n    return decorator", "python", "Python 重试装饰器"),
        ("SQL Join", "SELECT\n  u.id,\n  u.name,\n  u.email,\n  COUNT(o.id) AS order_count,\n  COALESCE(SUM(o.total), 0) AS total_spent\nFROM users u\nLEFT JOIN orders o ON o.user_id = u.id\nWHERE u.created_at >= '2024-01-01'\nGROUP BY u.id, u.name, u.email\nHAVING COUNT(o.id) > 0\nORDER BY total_spent DESC\nLIMIT 100;", "sql", "SQL 用户订单统计查询"),
        ("CSS Grid Layout", ".grid-container {\n  display: grid;\n  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));\n  grid-auto-rows: minmax(120px, auto);\n  gap: 1.5rem;\n  padding: 2rem;\n}\n\n.grid-item {\n  background: var(--surface);\n  border-radius: 12px;\n  padding: 1.5rem;\n  box-shadow: 0 2px 8px rgba(0,0,0,0.08);\n  transition: transform 0.2s, box-shadow 0.2s;\n}\n\n.grid-item:hover {\n  transform: translateY(-2px);\n  box-shadow: 0 8px 24px rgba(0,0,0,0.12);\n}", "css", "响应式 CSS Grid 布局"),
        ("Docker Compose", "version: '3.9'\n\nservices:\n  app:\n    build:\n      context: .\n      dockerfile: Dockerfile\n    ports:\n      - '3000:3000'\n    environment:\n      - NODE_ENV=production\n      - DATABASE_URL=postgres://user:pass@db:5432/myapp\n    depends_on:\n      db:\n        condition: service_healthy\n    restart: unless-stopped\n\n  db:\n    image: postgres:16-alpine\n    volumes:\n      - pgdata:/var/lib/postgresql/data\n    environment:\n      POSTGRES_DB: myapp\n      POSTGRES_USER: user\n      POSTGRES_PASSWORD_FILE: /run/secrets/db_password\n    healthcheck:\n      test: ['CMD-SHELL', 'pg_isready -U user -d myapp']\n      interval: 10s\n      timeout: 5s\n      retries: 5\n\nvolumes:\n  pgdata:", "yaml", "Docker Compose 生产配置"),
    ];

    for (title, content, lang, desc) in samples {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO snippets (id, title, content, language, description, tags, is_favorite, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, '[]', 0, ?6, ?6)",
            rusqlite::params![id, title, content, lang, desc, now],
        )?;
    }
    log::info!("Inserted sample snippets");
    Ok(())
}

pub fn with_db<F, T>(f: F) -> SqliteResult<T>
where
    F: FnOnce(&Connection) -> SqliteResult<T>,
{
    init_db()?;
    let db = DB.get().expect("DB not initialized");
    let conn = db.lock().unwrap();
    f(&conn)
}

pub fn with_db_mut<F, T>(f: F) -> SqliteResult<T>
where
    F: FnOnce(&mut Connection) -> SqliteResult<T>,
{
    init_db()?;
    let db = DB.get().expect("DB not initialized");
    let mut conn = db.lock().unwrap();
    f(&mut conn)
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct Snippet {
    pub id: String,
    pub title: String,
    pub content: String,
    pub language: String,
    pub description: String,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub revision_id: String,
}

#[derive(Debug)]
pub enum MutationError {
    Sqlite(rusqlite::Error),
    StaleRevision { current_revision_id: String },
    PendingLimit,
}

impl From<rusqlite::Error> for MutationError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sqlite(error)
    }
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(error) => error.fmt(formatter),
            Self::StaleRevision { .. } => formatter.write_str("snippet base revision is stale"),
            Self::PendingLimit => formatter.write_str("pending revision limit reached"),
        }
    }
}

impl std::error::Error for MutationError {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RevisionHead {
    pub snippet_id: String,
    pub revision_id: String,
    pub parent_revision_id: Option<String>,
    pub device_id: String,
    pub content_hash: String,
    pub revision_time: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct OutboxRevision {
    pub sequence: i64,
    pub revision_id: String,
    pub snippet_id: String,
    pub parent_revision_id: Option<String>,
    pub device_id: String,
    pub content_hash: String,
    pub revision_time: String,
    pub deleted: bool,
    pub operation_kind: String,
    pub origin: String,
    pub payload_json: String,
    pub payload_bytes: usize,
    #[serde(default)]
    pub conflict_of: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct StoredRevisionObject {
    pub revision_id: String,
    pub snippet_id: String,
    pub parent_revision_id: Option<String>,
    pub device_id: String,
    pub content_hash: String,
    pub revision_time: String,
    pub deleted: bool,
    pub origin: String,
    pub payload_json: String,
    pub payload_bytes: usize,
    pub conflict_of: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct RemoteSyncState {
    pub remote_id: String,
    pub protocol_version: i64,
    pub manifest_etag: Option<String>,
    pub manifest_hash: Option<String>,
    pub generation: i64,
    pub bootstrap_state: String,
    pub last_success_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct SyncSnapshot {
    pub device_id: String,
    pub snippets: Vec<Snippet>,
    pub heads: Vec<RevisionHead>,
    pub pending: Vec<OutboxRevision>,
    pub revision_objects: Vec<StoredRevisionObject>,
    pub pending_bytes: usize,
    pub remote: Option<RemoteSyncState>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct RemotePlanEntry {
    pub snippet_id: String,
    pub revision_id: String,
    pub parent_revision_id: Option<String>,
    pub device_id: String,
    pub content_hash: String,
    pub revision_time: String,
    pub deleted: bool,
    pub snippet: Option<Snippet>,
    /// The head observed before remote I/O began. The transaction skips this
    /// entry if a local edit advanced the head while synchronization was in flight.
    #[serde(default)]
    pub expected_local_revision_id: Option<String>,
    #[serde(default)]
    pub preserve_local_as_conflict: bool,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct ValidatedRemotePlan {
    pub remote_id: String,
    pub protocol_version: i64,
    pub generation: i64,
    pub manifest_etag: Option<String>,
    pub manifest_hash: Option<String>,
    pub entries: Vec<RemotePlanEntry>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct ApplyRemotePlanResult {
    pub applied: usize,
    pub skipped: usize,
    pub conflicts_created: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishCommit {
    pub remote_id: String,
    pub vault_id: String,
    pub protocol_version: i64,
    pub manifest_etag: Option<String>,
    pub manifest_hash: Option<String>,
    pub generation: i64,
    pub acknowledged_revision_ids: Vec<String>,
    pub snippet_count: usize,
    pub uploaded_count: usize,
    pub downloaded_count: usize,
    pub deleted_count: usize,
    pub conflict_count: usize,
    pub message: String,
    pub succeeded_at: String,
}

fn canonical_revision_payload(snippet: &Snippet, deleted: bool) -> SqliteResult<String> {
    if deleted {
        return Err(rusqlite::Error::InvalidParameterName(
            "live revision payload cannot be deleted".into(),
        ));
    }
    canonical_live_payload(snippet).map_err(rusqlite::Error::InvalidParameterName)
}

fn canonical_tombstone_payload_sqlite(id: &str, deleted_at: &str) -> SqliteResult<String> {
    canonical_tombstone_payload(id, deleted_at).map_err(rusqlite::Error::InvalidParameterName)
}

fn legacy_revision_id(id: &str, content_hash: &str, updated_at: &str) -> String {
    let material = format!("snipvault-v2-legacy\0{id}\0{content_hash}\0{updated_at}");
    format!("legacy-{}", sha256_hex(material.as_bytes()))
}

fn validate_revision_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}

fn decode_error(column: usize, kind: rusqlite::types::Type, reason: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        kind,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            reason.to_string(),
        )),
    )
}

fn decode_tags(row: &Row<'_>, column: usize) -> SqliteResult<Vec<String>> {
    let raw: String = row.get(column)?;
    let tags = serde_json::from_str::<Vec<String>>(&raw).map_err(|_| {
        decode_error(
            column,
            rusqlite::types::Type::Text,
            "stored snippet tags are not a JSON string array",
        )
    })?;
    if tags.len() > MAX_TAGS {
        return Err(decode_error(
            column,
            rusqlite::types::Type::Text,
            "stored snippet tags violate required field constraints",
        ));
    }
    let mut unique_tags = HashSet::new();
    if tags.iter().any(|tag| {
        tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS || !unique_tags.insert(tag)
    }) {
        return Err(decode_error(
            column,
            rusqlite::types::Type::Text,
            "stored snippet tags violate required field constraints",
        ));
    }
    Ok(tags)
}

fn decode_bool(row: &Row<'_>, column: usize) -> SqliteResult<bool> {
    match row.get::<_, i64>(column)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(decode_error(
            column,
            rusqlite::types::Type::Integer,
            "stored boolean is not 0 or 1",
        )),
    }
}

fn require_rfc3339(value: &str, column: usize, field: &str) -> SqliteResult<()> {
    chrono::DateTime::parse_from_rfc3339(value).map_err(|_| {
        decode_error(
            column,
            rusqlite::types::Type::Text,
            &format!("stored {field} is not RFC3339"),
        )
    })?;
    Ok(())
}

impl TryFrom<&Row<'_>> for Snippet {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> SqliteResult<Self> {
        let snippet = Snippet {
            id: row.get(0)?,
            title: row.get(1)?,
            content: row.get(2)?,
            language: row.get(3)?,
            description: row.get(4)?,
            tags: decode_tags(row, 5)?,
            is_favorite: decode_bool(row, 6)?,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
            revision_id: row.get(9)?,
        };
        require_rfc3339(&snippet.created_at, 7, "created_at")?;
        require_rfc3339(&snippet.updated_at, 8, "updated_at")?;
        if snippet.id.is_empty()
            || snippet.id.len() > MAX_ID_BYTES
            || snippet.id.chars().any(char::is_control)
            || snippet.title.chars().count() > MAX_TITLE_CHARS
            || snippet.content.len() > MAX_CONTENT_BYTES
            || snippet.description.chars().count() > MAX_DESCRIPTION_CHARS
            || snippet.language.is_empty()
            || snippet.language.chars().count() > MAX_LANGUAGE_CHARS
            || !validate_revision_token(&snippet.revision_id)
        {
            return Err(decode_error(
                0,
                rusqlite::types::Type::Text,
                "stored snippet violates required field constraints",
            ));
        }
        Ok(snippet)
    }
}

pub fn validate_snippet(snippet: &Snippet) -> Result<(), String> {
    if snippet.id.is_empty()
        || snippet.id.len() > MAX_ID_BYTES
        || snippet.id.chars().any(char::is_control)
    {
        return Err("片段 ID 无效".into());
    }
    if snippet.title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("片段标题过长: {}", snippet.id));
    }
    if snippet.content.len() > MAX_CONTENT_BYTES {
        return Err(format!("片段内容过大: {}", snippet.id));
    }
    if snippet.description.chars().count() > MAX_DESCRIPTION_CHARS {
        return Err(format!("片段描述过长: {}", snippet.id));
    }
    if snippet.language.is_empty() || snippet.language.chars().count() > MAX_LANGUAGE_CHARS {
        return Err(format!("片段语言无效: {}", snippet.id));
    }
    if snippet.tags.len() > MAX_TAGS {
        return Err(format!("片段标签过多: {}", snippet.id));
    }

    let mut tags = HashSet::new();
    for tag in &snippet.tags {
        if tag.is_empty() || tag.chars().count() > MAX_TAG_CHARS || !tags.insert(tag) {
            return Err(format!("片段标签无效或重复: {}", snippet.id));
        }
    }

    if snippet.revision_id.len() > 128 || snippet.revision_id.chars().any(char::is_control) {
        return Err(format!("片段修订标识无效: {}", snippet.id));
    }
    chrono::DateTime::parse_from_rfc3339(&snippet.created_at)
        .map_err(|_| format!("片段创建时间无效: {}", snippet.id))?;
    chrono::DateTime::parse_from_rfc3339(&snippet.updated_at)
        .map_err(|_| format!("片段更新时间无效: {}", snippet.id))?;
    Ok(())
}

fn validate_snippet_batch(snippets: &[Snippet]) -> Result<(), String> {
    if snippets.len() > MAX_IMPORT_ITEMS {
        return Err(format!("片段数量超过上限 {MAX_IMPORT_ITEMS}"));
    }

    let mut ids = HashSet::new();
    for snippet in snippets {
        validate_snippet(snippet)?;
        if !ids.insert(&snippet.id) {
            return Err(format!("存在重复片段 ID: {}", snippet.id));
        }
    }
    Ok(())
}

pub fn get_all_snippets() -> SqliteResult<Vec<Snippet>> {
    with_db(get_all_snippets_on_connection)
}

fn get_all_snippets_on_connection(conn: &Connection) -> SqliteResult<Vec<Snippet>> {
    let mut stmt = conn.prepare(
        "SELECT s.id, s.title, s.content, s.language, s.description, s.tags, s.is_favorite, s.created_at, s.updated_at, h.revision_id
         FROM snippets s JOIN snippet_heads h ON h.snippet_id=s.id WHERE h.deleted=0
         ORDER BY s.updated_at DESC, s.id DESC",
    )?;
    let rows = stmt.query_map([], |row| Snippet::try_from(row))?;
    rows.collect()
}

pub fn get_snippet(id: &str) -> SqliteResult<Snippet> {
    with_db(|conn| get_snippet_on_connection(conn, id))
}

fn get_snippet_on_connection(conn: &Connection, id: &str) -> SqliteResult<Snippet> {
    conn.query_row(
        "SELECT s.id, s.title, s.content, s.language, s.description, s.tags, s.is_favorite, s.created_at, s.updated_at, h.revision_id
         FROM snippets s JOIN snippet_heads h ON h.snippet_id=s.id
         WHERE s.id=?1 AND h.deleted=0",
        rusqlite::params![id],
        |row| Snippet::try_from(row),
    )
}

fn serialize_tags(tags: &[String]) -> SqliteResult<String> {
    serde_json::to_string(tags).map_err(|_| {
        rusqlite::Error::InvalidParameterName("snippet tags could not be serialized".into())
    })
}

fn local_device_id(conn: &Connection) -> SqliteResult<String> {
    conn.query_row(
        "SELECT device_id FROM sync_identity WHERE singleton=1",
        [],
        |row| row.get(0),
    )
}

fn pending_usage(conn: &Connection) -> SqliteResult<(usize, usize)> {
    let (count, bytes): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(payload_bytes), 0) FROM revision_outbox",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let count = usize::try_from(count)
        .map_err(|_| rusqlite::Error::InvalidParameterName("invalid outbox count".into()))?;
    let bytes = usize::try_from(bytes)
        .map_err(|_| rusqlite::Error::InvalidParameterName("invalid outbox byte count".into()))?;
    Ok((count, bytes))
}

fn ensure_pending_capacity(conn: &Connection, payload_bytes: usize) -> Result<(), MutationError> {
    if payload_bytes > MAX_REVISION_PAYLOAD_BYTES {
        return Err(MutationError::PendingLimit);
    }
    let (count, bytes) = pending_usage(conn)?;
    if count >= MAX_PENDING_OUTBOX_COUNT
        || bytes
            .checked_add(payload_bytes)
            .is_none_or(|total| total > MAX_PENDING_OUTBOX_BYTES)
    {
        return Err(MutationError::PendingLimit);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_revision(
    conn: &Connection,
    snippet_id: &str,
    parent_revision_id: Option<&str>,
    revision_id: &str,
    device_id: &str,
    content_hash: &str,
    revision_time: &str,
    deleted: bool,
    operation_kind: &str,
    origin: &str,
    payload_json: &str,
    conflict_of: Option<&str>,
    enqueue: bool,
) -> Result<(), MutationError> {
    if enqueue {
        ensure_pending_capacity(conn, payload_json.len())?;
    }
    conn.execute(
        "INSERT INTO snippet_heads
         (snippet_id, revision_id, parent_revision_id, device_id, content_hash, revision_time, deleted)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(snippet_id) DO UPDATE SET
           revision_id=excluded.revision_id,
           parent_revision_id=excluded.parent_revision_id,
           device_id=excluded.device_id,
           content_hash=excluded.content_hash,
           revision_time=excluded.revision_time,
           deleted=excluded.deleted",
        rusqlite::params![
            snippet_id,
            revision_id,
            parent_revision_id,
            device_id,
            content_hash,
            revision_time,
            deleted as i64
        ],
    )?;
    conn.execute(
        "INSERT INTO revision_objects
         (revision_id, snippet_id, parent_revision_id, device_id, content_hash, revision_time,
          deleted, origin, payload_json, payload_bytes, conflict_of, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?6)
         ON CONFLICT(revision_id) DO NOTHING",
        rusqlite::params![
            revision_id,
            snippet_id,
            parent_revision_id,
            device_id,
            content_hash,
            revision_time,
            deleted as i64,
            origin,
            payload_json,
            payload_json.len() as i64,
            conflict_of
        ],
    )?;
    if enqueue {
        conn.execute(
            "INSERT INTO revision_outbox
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash, revision_time,
              deleted, operation_kind, origin, payload_json, payload_bytes, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?6)",
            rusqlite::params![
                revision_id,
                snippet_id,
                parent_revision_id,
                device_id,
                content_hash,
                revision_time,
                deleted as i64,
                operation_kind,
                origin,
                payload_json,
                payload_json.len() as i64
            ],
        )?;
    }
    Ok(())
}

fn write_snippet_row(conn: &Connection, snippet: &Snippet) -> SqliteResult<()> {
    let tags_json = serialize_tags(&snippet.tags)?;
    conn.execute(
        "INSERT INTO snippets
         (id, title, content, language, description, tags, is_favorite, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(id) DO UPDATE SET
           title=excluded.title,
           content=excluded.content,
           language=excluded.language,
           description=excluded.description,
           tags=excluded.tags,
           is_favorite=excluded.is_favorite,
           created_at=excluded.created_at,
           updated_at=excluded.updated_at",
        rusqlite::params![
            snippet.id,
            snippet.title,
            snippet.content,
            snippet.language,
            snippet.description,
            tags_json,
            snippet.is_favorite as i64,
            snippet.created_at,
            snippet.updated_at
        ],
    )?;
    Ok(())
}

fn create_snippet_on_connection(
    conn: &Connection,
    snippet: &Snippet,
    origin: &str,
) -> Result<Snippet, MutationError> {
    let already_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM snippet_heads WHERE snippet_id=?1)",
        [&snippet.id],
        |row| row.get(0),
    )?;
    if already_exists {
        return Err(MutationError::Sqlite(
            rusqlite::Error::InvalidParameterName(
                "snippet identifier already has revision history".into(),
            ),
        ));
    }
    let mut authoritative = snippet.clone();
    let revision_id = uuid::Uuid::new_v4().to_string();
    let payload = canonical_revision_payload(&authoritative, false)?;
    let hash = sha256_hex(payload.as_bytes());
    let device_id = local_device_id(conn)?;
    authoritative.revision_id = revision_id.clone();
    write_snippet_row(conn, &authoritative)?;
    insert_revision(
        conn,
        &authoritative.id,
        None,
        &revision_id,
        &device_id,
        &hash,
        &authoritative.updated_at,
        false,
        OUTBOX_KIND_UPSERT,
        origin,
        &payload,
        None,
        true,
    )?;
    Ok(authoritative)
}

pub fn create_snippet(snippet: &Snippet) -> Result<Snippet, MutationError> {
    with_db_mut(|conn| {
        let tx = conn.transaction()?;
        let result = create_snippet_on_connection(&tx, snippet, REVISION_ORIGIN_LOCAL)
            .map_err(mutation_into_sqlite)?;
        tx.commit()?;
        Ok(result)
    })
    .map_err(MutationError::Sqlite)
}

fn mutation_into_sqlite(error: MutationError) -> rusqlite::Error {
    match error {
        MutationError::Sqlite(error) => error,
        MutationError::StaleRevision {
            current_revision_id,
        } => rusqlite::Error::InvalidParameterName(format!("stale_revision:{current_revision_id}")),
        MutationError::PendingLimit => {
            rusqlite::Error::InvalidParameterName("pending_revision_limit".into())
        }
    }
}

fn mutation_from_sqlite(error: rusqlite::Error) -> MutationError {
    match &error {
        rusqlite::Error::InvalidParameterName(value) if value == "pending_revision_limit" => {
            MutationError::PendingLimit
        }
        rusqlite::Error::InvalidParameterName(value) if value.starts_with("stale_revision:") => {
            MutationError::StaleRevision {
                current_revision_id: value["stale_revision:".len()..].to_string(),
            }
        }
        _ => MutationError::Sqlite(error),
    }
}

fn update_snippet_on_connection(
    conn: &Connection,
    snippet: &Snippet,
    base_revision_id: &str,
) -> Result<Snippet, MutationError> {
    let existing = get_snippet_on_connection(conn, &snippet.id)?;
    if existing.revision_id != base_revision_id {
        return Err(MutationError::StaleRevision {
            current_revision_id: existing.revision_id,
        });
    }
    let mut authoritative = snippet.clone();
    authoritative.created_at = existing.created_at;
    authoritative.updated_at = chrono::Utc::now().to_rfc3339();
    authoritative.revision_id = uuid::Uuid::new_v4().to_string();
    let payload = canonical_revision_payload(&authoritative, false)?;
    let hash = sha256_hex(payload.as_bytes());
    let device_id = local_device_id(conn)?;
    write_snippet_row(conn, &authoritative)?;
    insert_revision(
        conn,
        &authoritative.id,
        Some(base_revision_id),
        &authoritative.revision_id,
        &device_id,
        &hash,
        &authoritative.updated_at,
        false,
        OUTBOX_KIND_UPSERT,
        REVISION_ORIGIN_LOCAL,
        &payload,
        None,
        true,
    )?;
    Ok(authoritative)
}

pub fn update_snippet(snippet: &Snippet, base_revision_id: &str) -> Result<Snippet, MutationError> {
    let result = with_db_mut(|conn| {
        let tx = conn.transaction()?;
        let authoritative = update_snippet_on_connection(&tx, snippet, base_revision_id)
            .map_err(mutation_into_sqlite)?;
        tx.commit()?;
        Ok(authoritative)
    });
    result.map_err(mutation_from_sqlite)
}

pub fn delete_snippet(id: &str) -> Result<RevisionHead, MutationError> {
    let result = with_db_mut(|conn| {
        let tx = conn.transaction()?;
        let existing = get_snippet_on_connection(&tx, id)?;
        let revision_id = uuid::Uuid::new_v4().to_string();
        let deleted_at = chrono::Utc::now().to_rfc3339();
        let payload = canonical_tombstone_payload_sqlite(id, &deleted_at)?;
        let hash = sha256_hex(payload.as_bytes());
        let device_id = local_device_id(&tx)?;
        tx.execute("DELETE FROM snippets WHERE id=?1", [id])?;
        insert_revision(
            &tx,
            id,
            Some(&existing.revision_id),
            &revision_id,
            &device_id,
            &hash,
            &deleted_at,
            true,
            OUTBOX_KIND_DELETE,
            REVISION_ORIGIN_LOCAL,
            &payload,
            None,
            true,
        )
        .map_err(mutation_into_sqlite)?;
        tx.commit()?;
        Ok(RevisionHead {
            snippet_id: id.to_string(),
            revision_id,
            parent_revision_id: Some(existing.revision_id),
            device_id,
            content_hash: hash,
            revision_time: deleted_at,
            deleted: true,
        })
    });
    result.map_err(mutation_from_sqlite)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct SnippetSummary {
    pub id: String,
    pub title: String,
    pub language: String,
    pub description: String,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub revision_id: String,
    pub content_preview: String,
}

impl TryFrom<&Row<'_>> for SnippetSummary {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> SqliteResult<Self> {
        let summary = SnippetSummary {
            id: row.get(0)?,
            title: row.get(1)?,
            language: row.get(2)?,
            description: row.get(3)?,
            tags: decode_tags(row, 4)?,
            is_favorite: decode_bool(row, 5)?,
            created_at: row.get(6)?,
            updated_at: row.get(7)?,
            revision_id: row.get(8)?,
            content_preview: bounded_preview(&row.get::<_, String>(9)?),
        };
        let content_bytes: i64 = row.get(10)?;
        if summary.id.is_empty()
            || summary.id.len() > MAX_ID_BYTES
            || summary.id.chars().any(char::is_control)
            || summary.title.chars().count() > MAX_TITLE_CHARS
            || summary.language.is_empty()
            || summary.language.chars().count() > MAX_LANGUAGE_CHARS
            || summary.description.chars().count() > MAX_DESCRIPTION_CHARS
            || !validate_revision_token(&summary.revision_id)
            || content_bytes < 0
            || content_bytes as usize > MAX_CONTENT_BYTES
        {
            return Err(decode_error(
                0,
                rusqlite::types::Type::Text,
                "stored snippet summary violates required field constraints",
            ));
        }
        require_rfc3339(&summary.created_at, 6, "created_at")?;
        require_rfc3339(&summary.updated_at, 7, "updated_at")?;
        Ok(summary)
    }
}

#[derive(Debug, Clone, Default, serde::Deserialize, PartialEq)]
pub struct SnippetQuery {
    #[serde(default)]
    pub query: String,
    pub language: Option<String>,
    pub favorite: Option<bool>,
    pub exact_tag: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct SnippetQueryResult {
    pub items: Vec<SnippetSummary>,
    pub next_cursor: Option<String>,
    pub total: usize,
}

#[derive(Debug)]
struct DecodedCursor {
    updated_at: String,
    id: String,
}

fn bounded_preview(content: &str) -> String {
    if content.len() <= CONTENT_PREVIEW_BYTES {
        return content.to_string();
    }
    let mut end = CONTENT_PREVIEW_BYTES;
    while !content.is_char_boundary(end) {
        end -= 1;
    }
    content[..end].to_string()
}

fn encode_cursor(summary: &SnippetSummary) -> String {
    format!("{}{}{}", summary.updated_at, CURSOR_SEPARATOR, summary.id)
}

fn decode_cursor(cursor: &str) -> SqliteResult<DecodedCursor> {
    let (updated_at, id) = cursor.split_once(CURSOR_SEPARATOR).ok_or_else(|| {
        rusqlite::Error::InvalidParameterName("invalid snippet query cursor".into())
    })?;
    chrono::DateTime::parse_from_rfc3339(updated_at).map_err(|_| {
        rusqlite::Error::InvalidParameterName("invalid snippet query cursor".into())
    })?;
    if id.is_empty()
        || id.len() > MAX_ID_BYTES
        || id.chars().any(char::is_control)
        || id.contains(CURSOR_SEPARATOR)
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid snippet query cursor".into(),
        ));
    }
    Ok(DecodedCursor {
        updated_at: updated_at.to_string(),
        id: id.to_string(),
    })
}

fn escape_like_literal(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len() + 2);
    escaped.push('%');
    for character in query.chars() {
        if matches!(character, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.extend(character.to_lowercase());
    }
    escaped.push('%');
    escaped
}

fn escape_fts_literal(query: &str) -> String {
    format!("\"{}\"", query.replace('"', "\"\""))
}

fn fts_tokenizer(conn: &Connection) -> SqliteResult<String> {
    conn.query_row(
        "SELECT value FROM app_metadata WHERE key='fts_tokenizer'",
        [],
        |row| {
            let tokenizer: String = row.get(0)?;
            if matches!(tokenizer.as_str(), "trigram" | "unicode61") {
                Ok(tokenizer)
            } else {
                Err(decode_error(
                    0,
                    rusqlite::types::Type::Text,
                    "stored FTS tokenizer metadata is unsupported",
                ))
            }
        },
    )
}

fn summary_select() -> &'static str {
    "SELECT s.id, s.title, s.language, s.description, s.tags, s.is_favorite, s.created_at, s.updated_at,
            h.revision_id, substr(s.content, 1, 769), length(CAST(s.content AS BLOB))
     FROM snippets s JOIN snippet_heads h ON h.snippet_id=s.id AND h.deleted=0"
}

pub fn query_snippets(request: &SnippetQuery) -> SqliteResult<SnippetQueryResult> {
    with_db(|conn| query_snippets_on_connection(conn, request))
}

fn query_snippets_on_connection(
    conn: &Connection,
    request: &SnippetQuery,
) -> SqliteResult<SnippetQueryResult> {
    let normalized = request.query.trim();
    if normalized.chars().count() > MAX_QUERY_CHARS {
        return Err(rusqlite::Error::InvalidParameterName(
            "snippet query is too long".into(),
        ));
    }
    if request
        .language
        .as_ref()
        .is_some_and(|value| value.chars().count() > MAX_LANGUAGE_CHARS)
        || request
            .exact_tag
            .as_ref()
            .is_some_and(|value| value.chars().count() > MAX_TAG_CHARS)
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "snippet query filter is invalid".into(),
        ));
    }

    let limit = request
        .limit
        .unwrap_or(DEFAULT_PAGE_SIZE)
        .clamp(1, MAX_PAGE_SIZE);
    let cursor = request.cursor.as_deref().map(decode_cursor).transpose()?;
    let tokenizer = fts_tokenizer(conn)?;
    let use_fts =
        !normalized.is_empty() && normalized.chars().count() >= 3 && tokenizer == "trigram";
    let search_value = if use_fts {
        escape_fts_literal(normalized)
    } else if normalized.is_empty() {
        String::new()
    } else {
        escape_like_literal(normalized)
    };
    let favorite = request.favorite.map(i64::from);
    let cursor_updated = cursor.as_ref().map(|value| value.updated_at.as_str());
    let cursor_id = cursor.as_ref().map(|value| value.id.as_str());

    let search_predicate = if normalized.is_empty() {
        "1=1"
    } else if use_fts {
        "s.rowid IN (SELECT rowid FROM snippets_fts WHERE snippets_fts MATCH ?1)"
    } else {
        "(LOWER(s.title) LIKE ?1 ESCAPE '\\' OR LOWER(s.content) LIKE ?1 ESCAPE '\\' OR LOWER(s.description) LIKE ?1 ESCAPE '\\' OR EXISTS (
             SELECT 1 FROM json_each(s.tags) search_tag
             WHERE search_tag.type='text' AND LOWER(search_tag.value) LIKE ?1 ESCAPE '\\'
         ))"
    };
    let filters = format!(
        "{search_predicate}
         AND h.deleted=0
         AND (?2 IS NULL OR s.language = ?2)
         AND (?3 IS NULL OR s.is_favorite = ?3)
         AND (?4 IS NULL OR EXISTS (
             SELECT 1 FROM json_each(s.tags) tag
             WHERE tag.type='text' AND tag.value=?4
         ))"
    );
    let page_sql = format!(
        "{} WHERE {}
         AND (?5 IS NULL OR s.updated_at < ?5 OR (s.updated_at = ?5 AND s.id < ?6))
         ORDER BY s.updated_at DESC, s.id DESC LIMIT ?7",
        summary_select(),
        filters
    );
    let mut statement = conn.prepare(&page_sql)?;
    let rows = statement.query_map(
        rusqlite::params![
            search_value,
            request.language,
            favorite,
            request.exact_tag,
            cursor_updated,
            cursor_id,
            (limit + 1) as i64
        ],
        |row| SnippetSummary::try_from(row),
    )?;
    let mut items: Vec<SnippetSummary> = rows.collect::<SqliteResult<_>>()?;
    let has_more = items.len() > limit;
    if has_more {
        items.truncate(limit);
    }
    let next_cursor = has_more.then(|| encode_cursor(items.last().expect("non-empty page")));

    let count_sql = format!(
        "SELECT COUNT(*) FROM snippets s
         JOIN snippet_heads h ON h.snippet_id=s.id
         WHERE {filters}"
    );
    let total: i64 = conn.query_row(
        &count_sql,
        rusqlite::params![search_value, request.language, favorite, request.exact_tag],
        |row| row.get(0),
    )?;

    Ok(SnippetQueryResult {
        items,
        next_cursor,
        total: usize::try_from(total)
            .map_err(|_| rusqlite::Error::InvalidParameterName("invalid snippet count".into()))?,
    })
}

pub fn list_distinct_tags() -> SqliteResult<Vec<String>> {
    with_db(|conn| {
        let mut statement = conn.prepare(
            "SELECT DISTINCT tag.value
             FROM snippets s
             JOIN snippet_heads h ON h.snippet_id=s.id AND h.deleted=0,
                  json_each(s.tags) tag
             WHERE tag.type='text'
             ORDER BY tag.value COLLATE NOCASE, tag.value
             LIMIT 10000",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect()
    })
}

pub fn search_snippets(
    query: &str,
    language_filter: Option<&str>,
    tag_filter: Option<&str>,
) -> SqliteResult<Vec<Snippet>> {
    let summaries = query_snippets(&SnippetQuery {
        query: query.to_string(),
        language: language_filter.map(str::to_string),
        favorite: None,
        exact_tag: tag_filter.map(str::to_string),
        limit: Some(MAX_PAGE_SIZE),
        cursor: None,
    })?;
    summaries
        .items
        .iter()
        .map(|summary| get_snippet(&summary.id))
        .collect()
}

pub fn toggle_favorite(id: &str) -> Result<Snippet, MutationError> {
    let result = with_db_mut(|conn| {
        let tx = conn.transaction()?;
        let mut snippet = get_snippet_on_connection(&tx, id)?;
        let parent_revision_id = snippet.revision_id.clone();
        snippet.is_favorite = !snippet.is_favorite;
        snippet.updated_at = chrono::Utc::now().to_rfc3339();
        snippet.revision_id = uuid::Uuid::new_v4().to_string();
        let payload = canonical_revision_payload(&snippet, false)?;
        let hash = sha256_hex(payload.as_bytes());
        let device_id = local_device_id(&tx)?;
        write_snippet_row(&tx, &snippet)?;
        insert_revision(
            &tx,
            id,
            Some(&parent_revision_id),
            &snippet.revision_id,
            &device_id,
            &hash,
            &snippet.updated_at,
            false,
            OUTBOX_KIND_UPSERT,
            REVISION_ORIGIN_LOCAL,
            &payload,
            None,
            true,
        )
        .map_err(mutation_into_sqlite)?;
        tx.commit()?;
        Ok(snippet)
    });
    result.map_err(mutation_from_sqlite)
}

pub fn write_export_file(
    export_dir: &Path,
    filename_stem: &str,
    json: &str,
) -> std::io::Result<()> {
    for suffix in 0_u32.. {
        let suffix = if suffix == 0 {
            String::new()
        } else {
            format!("-{suffix}")
        };
        let target = export_dir.join(format!("{filename_stem}{suffix}.json"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
        {
            Ok(mut file) => {
                if let Err(error) = file
                    .write_all(json.as_bytes())
                    .and_then(|()| file.sync_all())
                {
                    drop(file);
                    let _ = fs::remove_file(&target);
                    return Err(error);
                }
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    unreachable!("unbounded export filename allocation")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct ExportEnvelope {
    pub format_id: String,
    pub schema_version: u32,
    pub app_version: String,
    pub exported_at: String,
    pub snippets: Vec<Snippet>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ImportDocument {
    Legacy(Vec<Snippet>),
    Envelope(ExportEnvelope),
}

pub fn export_snippets() -> SqliteResult<String> {
    let envelope = ExportEnvelope {
        format_id: EXPORT_FORMAT_ID.to_string(),
        schema_version: EXPORT_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: chrono::Utc::now().to_rfc3339(),
        snippets: get_all_snippets()?,
    };
    serde_json::to_string_pretty(&envelope).map_err(|_| {
        rusqlite::Error::InvalidParameterName("snippet export serialization failed".into())
    })
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct MergeResult {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub total: usize,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct ImportResult {
    pub input_count: usize,
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
}

fn parse_import_document(json_data: &str) -> SqliteResult<Vec<Snippet>> {
    match serde_json::from_str::<ImportDocument>(json_data).map_err(|_| {
        rusqlite::Error::InvalidParameterName("malformed snippet import document".into())
    })? {
        ImportDocument::Legacy(snippets) => Ok(snippets),
        ImportDocument::Envelope(envelope) => {
            if envelope.format_id != EXPORT_FORMAT_ID {
                return Err(rusqlite::Error::InvalidParameterName(
                    "unsupported snippet import format".into(),
                ));
            }
            if envelope.schema_version != EXPORT_SCHEMA_VERSION {
                return Err(rusqlite::Error::InvalidParameterName(format!(
                    "unsupported snippet import schema version {}",
                    envelope.schema_version
                )));
            }
            if envelope.app_version.trim().is_empty()
                || chrono::DateTime::parse_from_rfc3339(&envelope.exported_at).is_err()
            {
                return Err(rusqlite::Error::InvalidParameterName(
                    "malformed snippet import envelope metadata".into(),
                ));
            }
            Ok(envelope.snippets)
        }
    }
}

pub fn import_snippets(json_data: &str) -> SqliteResult<ImportResult> {
    if json_data.len() > MAX_IMPORT_BYTES {
        return Err(rusqlite::Error::InvalidParameterName(format!(
            "导入文件超过 {} MB",
            MAX_IMPORT_BYTES / 1024 / 1024
        )));
    }

    let snippets = parse_import_document(json_data)?;
    validate_snippet_batch(&snippets).map_err(rusqlite::Error::InvalidParameterName)?;

    let input_count = snippets.len();
    let merge = with_db_mut(|conn| {
        let tx = conn.transaction()?;
        let result =
            merge_snippets_on_connection(&tx, snippets, false, REVISION_ORIGIN_IMPORT, true)?;
        tx.commit()?;
        Ok(result)
    })?;

    Ok(ImportResult {
        input_count,
        inserted: merge.inserted,
        updated: merge.updated,
        skipped: merge.skipped,
    })
}

pub fn merge_snippets(snippets: Vec<Snippet>) -> SqliteResult<MergeResult> {
    merge_snippets_with_policy(snippets, false)
}

#[cfg(test)]
pub(crate) fn merge_sync_snippets(snippets: Vec<Snippet>) -> SqliteResult<MergeResult> {
    merge_snippets_with_policy(snippets, true)
}

fn merge_snippets_with_policy(
    snippets: Vec<Snippet>,
    replace_equal_timestamp: bool,
) -> SqliteResult<MergeResult> {
    validate_snippet_batch(&snippets).map_err(rusqlite::Error::InvalidParameterName)?;
    with_db_mut(|conn| {
        let tx = conn.transaction()?;
        let result = merge_snippets_on_connection(
            &tx,
            snippets,
            replace_equal_timestamp,
            if replace_equal_timestamp {
                REVISION_ORIGIN_REMOTE
            } else {
                REVISION_ORIGIN_LOCAL
            },
            !replace_equal_timestamp,
        )?;
        tx.commit()?;
        Ok(result)
    })
}

fn merge_snippets_on_connection(
    conn: &Connection,
    snippets: Vec<Snippet>,
    replace_equal_timestamp: bool,
    origin: &str,
    enqueue: bool,
) -> SqliteResult<MergeResult> {
    let mut inserted = 0;
    let mut updated = 0;
    let mut skipped = 0;

    for remote in snippets {
        let local_updated: Option<(String, String)> = conn
            .query_row(
                "SELECT s.updated_at, h.revision_id
                 FROM snippets s JOIN snippet_heads h ON h.snippet_id=s.id AND h.deleted=0
                 WHERE s.id=?1",
                rusqlite::params![remote.id],
                |row| {
                    let updated_at: String = row.get(0)?;
                    require_rfc3339(&updated_at, 0, "updated_at")?;
                    let revision_id: String = row.get(1)?;
                    if !validate_revision_token(&revision_id) {
                        return Err(decode_error(
                            1,
                            rusqlite::types::Type::Text,
                            "stored revision identifier is invalid",
                        ));
                    }
                    Ok((updated_at, revision_id))
                },
            )
            .optional()?;

        let should_update = local_updated
            .as_ref()
            .map(|(local, _)| {
                timestamp_is_newer(&remote.updated_at, local)
                    || (replace_equal_timestamp && remote.updated_at == *local)
            })
            .unwrap_or(true);

        if !should_update {
            skipped += 1;
            continue;
        }

        let parent_revision_id = local_updated
            .as_ref()
            .map(|(_, revision_id)| revision_id.as_str());
        let mut applied = remote;
        applied.revision_id = uuid::Uuid::new_v4().to_string();
        let payload = canonical_revision_payload(&applied, false)?;
        let hash = sha256_hex(payload.as_bytes());
        let device_id = local_device_id(conn)?;
        write_snippet_row(conn, &applied)?;
        insert_revision(
            conn,
            &applied.id,
            parent_revision_id,
            &applied.revision_id,
            &device_id,
            &hash,
            &applied.updated_at,
            false,
            OUTBOX_KIND_UPSERT,
            origin,
            &payload,
            None,
            enqueue,
        )
        .map_err(mutation_into_sqlite)?;

        if local_updated.is_some() {
            updated += 1;
        } else {
            inserted += 1;
        }
    }

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM snippets s JOIN snippet_heads h ON h.snippet_id=s.id AND h.deleted=0",
        [],
        |row| row.get(0),
    )?;
    Ok(MergeResult {
        inserted,
        updated,
        skipped,
        total: total as usize,
    })
}

pub(crate) fn timestamp_is_newer(remote: &str, local: &str) -> bool {
    match (
        chrono::DateTime::parse_from_rfc3339(remote),
        chrono::DateTime::parse_from_rfc3339(local),
    ) {
        (Ok(remote), Ok(local)) => remote > local,
        _ => remote > local,
    }
}

pub fn get_all_for_upload() -> SqliteResult<Vec<Snippet>> {
    get_all_snippets()
}

fn decode_revision_head(row: &Row<'_>) -> SqliteResult<RevisionHead> {
    let head = RevisionHead {
        snippet_id: row.get(0)?,
        revision_id: row.get(1)?,
        parent_revision_id: row.get(2)?,
        device_id: row.get(3)?,
        content_hash: row.get(4)?,
        revision_time: row.get(5)?,
        deleted: decode_bool(row, 6)?,
    };
    if head.snippet_id.is_empty()
        || !validate_revision_token(&head.revision_id)
        || head
            .parent_revision_id
            .as_deref()
            .is_some_and(|value| !validate_revision_token(value))
        || !validate_revision_token(&head.device_id)
        || head.content_hash.len() != 64
        || !head
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(decode_error(
            0,
            rusqlite::types::Type::Text,
            "stored revision head is invalid",
        ));
    }
    require_rfc3339(&head.revision_time, 5, "revision_time")?;
    Ok(head)
}

fn decode_stored_revision_object(row: &Row<'_>) -> SqliteResult<StoredRevisionObject> {
    let payload_bytes: i64 = row.get(9)?;
    let object = StoredRevisionObject {
        revision_id: row.get(0)?,
        snippet_id: row.get(1)?,
        parent_revision_id: row.get(2)?,
        device_id: row.get(3)?,
        content_hash: row.get(4)?,
        revision_time: row.get(5)?,
        deleted: decode_bool(row, 6)?,
        origin: row.get(7)?,
        payload_json: row.get(8)?,
        payload_bytes: usize::try_from(payload_bytes).map_err(|_| {
            decode_error(
                9,
                rusqlite::types::Type::Integer,
                "stored revision-object payload size is invalid",
            )
        })?,
        conflict_of: row.get(10)?,
    };
    if !validate_revision_token(&object.revision_id)
        || object.snippet_id.is_empty()
        || object.snippet_id.len() > MAX_ID_BYTES
        || object
            .parent_revision_id
            .as_deref()
            .is_some_and(|value| !validate_revision_token(value))
        || !validate_revision_token(&object.device_id)
        || object.content_hash.len() != 64
        || !object
            .content_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || object.payload_json.len() != object.payload_bytes
        || object.payload_bytes > MAX_REVISION_PAYLOAD_BYTES
        || !matches!(
            object.origin.as_str(),
            "local" | "import" | "remote" | "conflict"
        )
        || object
            .conflict_of
            .as_deref()
            .is_some_and(|value| !validate_revision_token(value))
        || serde_json::from_str::<serde_json::Value>(&object.payload_json).is_err()
        || sha256_hex(object.payload_json.as_bytes()) != object.content_hash
    {
        return Err(decode_error(
            0,
            rusqlite::types::Type::Text,
            "stored revision object is invalid",
        ));
    }
    require_rfc3339(&object.revision_time, 5, "revision_time")?;
    Ok(object)
}

fn decode_outbox_revision(row: &Row<'_>) -> SqliteResult<OutboxRevision> {
    let payload_bytes: i64 = row.get(11)?;
    let object_revision_id: Option<String> = row.get(13)?;
    if object_revision_id.is_none() {
        return Err(decode_error(
            13,
            rusqlite::types::Type::Null,
            "stored outbox revision object is missing",
        ));
    }
    let revision = OutboxRevision {
        sequence: row.get(0)?,
        revision_id: row.get(1)?,
        snippet_id: row.get(2)?,
        parent_revision_id: row.get(3)?,
        device_id: row.get(4)?,
        content_hash: row.get(5)?,
        revision_time: row.get(6)?,
        deleted: decode_bool(row, 7)?,
        operation_kind: row.get(8)?,
        origin: row.get(9)?,
        payload_json: row.get(10)?,
        payload_bytes: usize::try_from(payload_bytes).map_err(|_| {
            decode_error(
                11,
                rusqlite::types::Type::Integer,
                "stored revision payload size is invalid",
            )
        })?,
        conflict_of: row.get(12)?,
    };
    if revision.sequence < 1
        || !validate_revision_token(&revision.revision_id)
        || revision.snippet_id.is_empty()
        || revision
            .parent_revision_id
            .as_deref()
            .is_some_and(|value| !validate_revision_token(value))
        || revision
            .conflict_of
            .as_deref()
            .is_some_and(|value| !validate_revision_token(value))
        || !validate_revision_token(&revision.device_id)
        || revision.content_hash.len() != 64
        || revision.payload_json.len() != revision.payload_bytes
        || revision.payload_bytes > MAX_REVISION_PAYLOAD_BYTES
        || !matches!(revision.operation_kind.as_str(), "upsert" | "delete")
        || !matches!(
            revision.origin.as_str(),
            "local" | "import" | "remote" | "conflict"
        )
        || serde_json::from_str::<serde_json::Value>(&revision.payload_json).is_err()
        || (revision.deleted != (revision.operation_kind == OUTBOX_KIND_DELETE))
        || sha256_hex(revision.payload_json.as_bytes()) != revision.content_hash
    {
        return Err(decode_error(
            0,
            rusqlite::types::Type::Text,
            "stored outbox revision is invalid",
        ));
    }
    require_rfc3339(&revision.revision_time, 6, "revision_time")?;
    Ok(revision)
}

fn decode_remote_state(row: &Row<'_>) -> SqliteResult<RemoteSyncState> {
    let state = RemoteSyncState {
        remote_id: row.get(0)?,
        protocol_version: row.get(1)?,
        manifest_etag: row.get(2)?,
        manifest_hash: row.get(3)?,
        generation: row.get(4)?,
        bootstrap_state: row.get(5)?,
        last_success_at: row.get(6)?,
        updated_at: row.get(7)?,
    };
    if !valid_remote_id(&state.remote_id)
        || state.protocol_version < 1
        || state.generation < 0
        || !matches!(
            state.bootstrap_state.as_str(),
            "pending" | "ready" | "blocked"
        )
        || state.manifest_hash.as_deref().is_some_and(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(decode_error(
            0,
            rusqlite::types::Type::Text,
            "stored remote state is invalid",
        ));
    }
    if let Some(last_success_at) = &state.last_success_at {
        require_rfc3339(last_success_at, 6, "last_success_at")?;
    }
    require_rfc3339(&state.updated_at, 7, "updated_at")?;
    Ok(state)
}

fn valid_remote_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}

fn remote_vault_metadata_key(remote_id: &str) -> String {
    format!("sync_vault_id:{remote_id}")
}

pub fn load_remote_vault_id(remote_id: &str) -> SqliteResult<Option<String>> {
    if !valid_remote_id(remote_id) {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid remote identifier".into(),
        ));
    }
    with_db(|conn| {
        conn.query_row(
            "SELECT value FROM app_metadata WHERE key=?1",
            [remote_vault_metadata_key(remote_id)],
            |row| row.get(0),
        )
        .optional()
    })
}

pub fn load_sync_snapshot(remote_id: &str) -> SqliteResult<SyncSnapshot> {
    if !valid_remote_id(remote_id) {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid remote identifier".into(),
        ));
    }
    with_db(|conn| load_sync_snapshot_on_connection(conn, remote_id))
}

fn load_sync_snapshot_on_connection(
    conn: &Connection,
    remote_id: &str,
) -> SqliteResult<SyncSnapshot> {
    let device_id = local_device_id(conn)?;
    let snippets = get_all_snippets_on_connection(conn)?;
    let heads = {
        let mut statement = conn.prepare(
            "SELECT snippet_id, revision_id, parent_revision_id, device_id, content_hash,
                    revision_time, deleted
             FROM snippet_heads ORDER BY snippet_id",
        )?;
        let rows = statement.query_map([], decode_revision_head)?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };
    let revision_objects = {
        let mut statement = conn.prepare(
            "WITH RECURSIVE seeds(revision_id) AS (
                 SELECT revision_id FROM snippet_heads
                 UNION
                 SELECT revision_id FROM revision_outbox
             ),
             required(revision_id) AS (
                 SELECT revision_id FROM seeds
                 UNION
                 SELECT r.parent_revision_id
                 FROM revision_objects r JOIN required q ON r.revision_id=q.revision_id
                 WHERE r.parent_revision_id IS NOT NULL
             )
             SELECT r.revision_id, r.snippet_id, r.parent_revision_id, r.device_id,
                    r.content_hash, r.revision_time, r.deleted, r.origin, r.payload_json,
                    r.payload_bytes, r.conflict_of
             FROM revision_objects r JOIN required q ON q.revision_id=r.revision_id
             ORDER BY r.revision_time, r.revision_id",
        )?;
        let rows = statement.query_map([], decode_stored_revision_object)?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };
    let revision_object_bytes = revision_objects
        .iter()
        .try_fold(0_usize, |total, revision| {
            total.checked_add(revision.payload_bytes).ok_or_else(|| {
                rusqlite::Error::InvalidParameterName("revision-object byte count overflow".into())
            })
        })?;
    if revision_objects.len() > MAX_SYNC_ANCESTRY_OBJECTS
        || revision_object_bytes > MAX_SYNC_ANCESTRY_BYTES
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "stored revision ancestry exceeds safety limits".into(),
        ));
    }
    let pending = {
        let mut statement = conn.prepare(
            "SELECT o.sequence, o.revision_id, o.snippet_id, o.parent_revision_id, o.device_id,
                    o.content_hash, o.revision_time, o.deleted, o.operation_kind, o.origin,
                    o.payload_json, o.payload_bytes, r.conflict_of, r.revision_id
             FROM revision_outbox o
             LEFT JOIN revision_objects r ON r.revision_id=o.revision_id
             ORDER BY o.sequence",
        )?;
        let rows = statement.query_map([], decode_outbox_revision)?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };
    let pending_bytes = pending.iter().try_fold(0_usize, |total, revision| {
        total.checked_add(revision.payload_bytes).ok_or_else(|| {
            rusqlite::Error::InvalidParameterName("outbox byte count overflow".into())
        })
    })?;
    if pending.len() > MAX_PENDING_OUTBOX_COUNT || pending_bytes > MAX_PENDING_OUTBOX_BYTES {
        return Err(rusqlite::Error::InvalidParameterName(
            "stored outbox exceeds safety limits".into(),
        ));
    }
    let remote = conn
        .query_row(
            "SELECT remote_id, protocol_version, manifest_etag, manifest_hash, generation,
                    bootstrap_state, last_success_at, updated_at
             FROM sync_remote_state WHERE remote_id=?1",
            [remote_id],
            decode_remote_state,
        )
        .optional()?;
    Ok(SyncSnapshot {
        device_id,
        snippets,
        heads,
        pending,
        revision_objects,
        pending_bytes,
        remote,
    })
}

fn validate_remote_plan(plan: &ValidatedRemotePlan) -> SqliteResult<()> {
    if !valid_remote_id(&plan.remote_id) || plan.protocol_version < 1 || plan.generation < 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid remote plan metadata".into(),
        ));
    }
    if plan
        .manifest_hash
        .as_deref()
        .is_some_and(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid remote manifest hash".into(),
        ));
    }
    let mut revisions = HashSet::new();
    let mut snippets = HashSet::new();
    for entry in &plan.entries {
        if entry.snippet_id.is_empty()
            || entry.snippet_id.len() > MAX_ID_BYTES
            || !validate_revision_token(&entry.revision_id)
            || entry
                .parent_revision_id
                .as_deref()
                .is_some_and(|value| !validate_revision_token(value))
            || !validate_revision_token(&entry.device_id)
            || entry.content_hash.len() != 64
            || !entry
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !revisions.insert(&entry.revision_id)
            || !snippets.insert(&entry.snippet_id)
        {
            return Err(rusqlite::Error::InvalidParameterName(
                "invalid or duplicate remote revision".into(),
            ));
        }
        chrono::DateTime::parse_from_rfc3339(&entry.revision_time).map_err(|_| {
            rusqlite::Error::InvalidParameterName("invalid remote revision time".into())
        })?;
        if entry.deleted {
            if entry.snippet.is_some() {
                return Err(rusqlite::Error::InvalidParameterName(
                    "remote tombstone includes live payload".into(),
                ));
            }
            let payload =
                canonical_tombstone_payload_sqlite(&entry.snippet_id, &entry.revision_time)?;
            if sha256_hex(payload.as_bytes()) != entry.content_hash {
                return Err(rusqlite::Error::InvalidParameterName(
                    "remote tombstone hash mismatch".into(),
                ));
            }
        } else {
            let snippet = entry.snippet.as_ref().ok_or_else(|| {
                rusqlite::Error::InvalidParameterName("remote upsert is missing payload".into())
            })?;
            if snippet.id != entry.snippet_id || snippet.updated_at != entry.revision_time {
                return Err(rusqlite::Error::InvalidParameterName(
                    "remote upsert metadata mismatch".into(),
                ));
            }
            validate_snippet(snippet).map_err(rusqlite::Error::InvalidParameterName)?;
            let payload = canonical_revision_payload(snippet, false)?;
            if sha256_hex(payload.as_bytes()) != entry.content_hash {
                return Err(rusqlite::Error::InvalidParameterName(
                    "remote upsert hash mismatch".into(),
                ));
            }
        }
    }
    Ok(())
}

fn create_conflict_copy(
    conn: &Connection,
    current: &Snippet,
    incoming_revision_id: &str,
    detected_at: &str,
) -> SqliteResult<bool> {
    let conflict_id =
        deterministic_conflict_uuid(&current.id, &current.revision_id, incoming_revision_id);
    let conflict_snippet_id = conflict_id.clone();
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO sync_conflicts
         (conflict_id, source_snippet_id, local_revision_id, incoming_revision_id,
          conflict_snippet_id, detected_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            conflict_id,
            current.id,
            current.revision_id,
            incoming_revision_id,
            conflict_snippet_id,
            detected_at
        ],
    )?;
    if inserted == 0 {
        return Ok(false);
    }

    let mut conflict = current.clone();
    conflict.id = conflict_snippet_id;
    conflict.title = format!("{} (Conflict Copy)", current.title);
    conflict.created_at = detected_at.to_string();
    conflict.updated_at = detected_at.to_string();
    conflict.revision_id = conflict_id;
    let payload = canonical_revision_payload(&conflict, false)?;
    let hash = sha256_hex(payload.as_bytes());
    write_snippet_row(conn, &conflict)?;
    insert_revision(
        conn,
        &conflict.id,
        None,
        &conflict.revision_id,
        "conflict-copy",
        &hash,
        detected_at,
        false,
        OUTBOX_KIND_UPSERT,
        "conflict",
        &payload,
        Some(incoming_revision_id),
        true,
    )
    .map_err(mutation_into_sqlite)?;
    Ok(true)
}

pub fn apply_validated_remote_plan(
    plan: &ValidatedRemotePlan,
) -> SqliteResult<ApplyRemotePlanResult> {
    with_db_mut(|conn| apply_validated_remote_plan_on_connection(conn, plan))
}

fn apply_validated_remote_plan_on_connection(
    conn: &mut Connection,
    plan: &ValidatedRemotePlan,
) -> SqliteResult<ApplyRemotePlanResult> {
    validate_remote_plan(plan)?;
    let tx = conn.transaction()?;
    let mut applied = 0;
    let mut skipped = 0;
    let mut conflicts_created = 0;
    for entry in &plan.entries {
        let current_head = tx
            .query_row(
                "SELECT snippet_id, revision_id, parent_revision_id, device_id, content_hash,
                        revision_time, deleted
                 FROM snippet_heads WHERE snippet_id=?1",
                [&entry.snippet_id],
                decode_revision_head,
            )
            .optional()?;
        if current_head
            .as_ref()
            .is_some_and(|head| head.revision_id == entry.revision_id)
        {
            skipped += 1;
            continue;
        }
        let current_revision_id = current_head.as_ref().map(|head| head.revision_id.as_str());
        if current_revision_id != entry.expected_local_revision_id.as_deref() {
            skipped += 1;
            continue;
        }
        if entry.preserve_local_as_conflict {
            if let Ok(current) = get_snippet_on_connection(&tx, &entry.snippet_id) {
                conflicts_created += usize::from(create_conflict_copy(
                    &tx,
                    &current,
                    &entry.revision_id,
                    &entry.revision_time,
                )?);
            }
        }
        if entry.deleted {
            tx.execute("DELETE FROM snippets WHERE id=?1", [&entry.snippet_id])?;
            let payload =
                canonical_tombstone_payload_sqlite(&entry.snippet_id, &entry.revision_time)?;
            insert_revision(
                &tx,
                &entry.snippet_id,
                entry.parent_revision_id.as_deref(),
                &entry.revision_id,
                &entry.device_id,
                &entry.content_hash,
                &entry.revision_time,
                true,
                OUTBOX_KIND_DELETE,
                REVISION_ORIGIN_REMOTE,
                &payload,
                None,
                false,
            )
            .map_err(mutation_into_sqlite)?;
        } else {
            let mut snippet = entry.snippet.clone().expect("validated remote payload");
            snippet.revision_id = entry.revision_id.clone();
            let payload = canonical_revision_payload(&snippet, false)?;
            write_snippet_row(&tx, &snippet)?;
            insert_revision(
                &tx,
                &entry.snippet_id,
                entry.parent_revision_id.as_deref(),
                &entry.revision_id,
                &entry.device_id,
                &entry.content_hash,
                &entry.revision_time,
                false,
                OUTBOX_KIND_UPSERT,
                REVISION_ORIGIN_REMOTE,
                &payload,
                None,
                false,
            )
            .map_err(mutation_into_sqlite)?;
        }
        applied += 1;
    }
    let now = chrono::Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO sync_remote_state
         (remote_id, protocol_version, manifest_etag, manifest_hash, generation,
          bootstrap_state, last_success_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'pending', NULL, ?6)
         ON CONFLICT(remote_id) DO UPDATE SET
           protocol_version=excluded.protocol_version,
           manifest_etag=excluded.manifest_etag,
           manifest_hash=excluded.manifest_hash,
           generation=excluded.generation,
           bootstrap_state='pending',
           updated_at=excluded.updated_at",
        rusqlite::params![
            plan.remote_id,
            plan.protocol_version,
            plan.manifest_etag,
            plan.manifest_hash,
            plan.generation,
            now
        ],
    )?;
    tx.commit()?;
    Ok(ApplyRemotePlanResult {
        applied,
        skipped,
        conflicts_created,
    })
}

pub fn commit_published_revisions(commit: &PublishCommit) -> SqliteResult<usize> {
    with_db_mut(|conn| commit_published_revisions_on_connection(conn, commit))
}

fn commit_published_revisions_on_connection(
    conn: &mut Connection,
    commit: &PublishCommit,
) -> SqliteResult<usize> {
    if !valid_remote_id(&commit.remote_id)
        || uuid::Uuid::parse_str(&commit.vault_id).is_err()
        || commit.protocol_version < 1
        || commit.generation < 0
        || chrono::DateTime::parse_from_rfc3339(&commit.succeeded_at).is_err()
        || commit.manifest_hash.as_deref().is_some_and(|hash| {
            hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid publish commit metadata".into(),
        ));
    }
    let unique: HashSet<&str> = commit
        .acknowledged_revision_ids
        .iter()
        .map(String::as_str)
        .collect();
    if unique.len() != commit.acknowledged_revision_ids.len()
        || unique.iter().any(|value| !validate_revision_token(value))
    {
        return Err(rusqlite::Error::InvalidParameterName(
            "invalid acknowledged revision set".into(),
        ));
    }

    let tx = conn.transaction()?;
    if let Some(existing) = tx
        .query_row(
            "SELECT generation FROM sync_remote_state WHERE remote_id=?1",
            [&commit.remote_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        if commit.generation < existing {
            return Err(rusqlite::Error::InvalidParameterName(
                "publish generation would regress remote state".into(),
            ));
        }
    }
    let mut acknowledged = 0;
    for revision_id in &commit.acknowledged_revision_ids {
        acknowledged += tx.execute(
            "DELETE FROM revision_outbox WHERE revision_id=?1",
            [revision_id],
        )?;
    }
    tx.execute(
        "INSERT INTO sync_remote_state
         (remote_id, protocol_version, manifest_etag, manifest_hash, generation,
          bootstrap_state, last_success_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'ready', ?6, ?6)
         ON CONFLICT(remote_id) DO UPDATE SET
           protocol_version=excluded.protocol_version,
           manifest_etag=excluded.manifest_etag,
           manifest_hash=excluded.manifest_hash,
           generation=excluded.generation,
           bootstrap_state='ready',
           last_success_at=excluded.last_success_at,
           updated_at=excluded.updated_at",
        rusqlite::params![
            commit.remote_id,
            commit.protocol_version,
            commit.manifest_etag,
            commit.manifest_hash,
            commit.generation,
            commit.succeeded_at
        ],
    )?;
    tx.execute(
        "INSERT INTO app_metadata(key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        rusqlite::params![
            remote_vault_metadata_key(&commit.remote_id),
            commit.vault_id
        ],
    )?;
    let history_id = uuid::Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO sync_versions
         (id, synced_at, direction, snippet_count, uploaded_count, downloaded_count,
          deleted_count, conflict_count, protocol_version, generation, message)
         VALUES (?1, ?2, 'publish', ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        rusqlite::params![
            history_id,
            commit.succeeded_at,
            commit.snippet_count as i64,
            commit.uploaded_count as i64,
            commit.downloaded_count as i64,
            commit.deleted_count as i64,
            commit.conflict_count as i64,
            commit.protocol_version,
            commit.generation,
            commit.message
        ],
    )?;
    tx.execute(
        "DELETE FROM sync_versions WHERE id NOT IN
         (SELECT id FROM sync_versions ORDER BY synced_at DESC, id DESC LIMIT 20)",
        [],
    )?;
    tx.commit()?;
    Ok(acknowledged)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncVersion {
    pub id: String,
    pub synced_at: String,
    pub direction: String,
    pub snippet_count: i64,
    pub uploaded_count: i64,
    pub downloaded_count: i64,
    pub deleted_count: i64,
    pub conflict_count: i64,
    pub protocol_version: i64,
    pub generation: i64,
    pub message: String,
}

impl TryFrom<&Row<'_>> for SyncVersion {
    type Error = rusqlite::Error;

    fn try_from(row: &Row<'_>) -> SqliteResult<Self> {
        let version = SyncVersion {
            id: row.get(0)?,
            synced_at: row.get(1)?,
            direction: row.get(2)?,
            snippet_count: row.get(3)?,
            uploaded_count: row.get(4)?,
            downloaded_count: row.get(5)?,
            deleted_count: row.get(6)?,
            conflict_count: row.get(7)?,
            protocol_version: row.get(8)?,
            generation: row.get(9)?,
            message: row.get(10)?,
        };
        if version.id.is_empty()
            || version.direction.is_empty()
            || version.snippet_count < 0
            || version.uploaded_count < 0
            || version.downloaded_count < 0
            || version.deleted_count < 0
            || version.conflict_count < 0
            || version.protocol_version < 1
            || version.generation < 0
        {
            return Err(decode_error(
                0,
                rusqlite::types::Type::Text,
                "stored sync history violates required field constraints",
            ));
        }
        require_rfc3339(&version.synced_at, 1, "synced_at")?;
        Ok(version)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn record_sync_version(
    direction: &str,
    snippet_count: usize,
    uploaded_count: usize,
    downloaded_count: usize,
    deleted_count: usize,
    conflict_count: usize,
    protocol_version: i64,
    generation: i64,
    message: &str,
) -> SqliteResult<()> {
    with_db_mut(|conn| {
        let tx = conn.transaction()?;
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO sync_versions
             (id, synced_at, direction, snippet_count, uploaded_count, downloaded_count,
              deleted_count, conflict_count, protocol_version, generation, message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                id,
                now,
                direction,
                snippet_count as i64,
                uploaded_count as i64,
                downloaded_count as i64,
                deleted_count as i64,
                conflict_count as i64,
                protocol_version,
                generation,
                message
            ],
        )?;
        tx.execute(
            "DELETE FROM sync_versions WHERE id NOT IN (SELECT id FROM sync_versions ORDER BY synced_at DESC, id DESC LIMIT 20)",
            [],
        )?;
        tx.commit()
    })
}

pub fn get_sync_versions() -> SqliteResult<Vec<SyncVersion>> {
    with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, synced_at, direction, snippet_count, uploaded_count, downloaded_count,
                    deleted_count, conflict_count, protocol_version, generation, message
             FROM sync_versions ORDER BY synced_at DESC, id DESC LIMIT 20",
        )?;
        let rows = stmt.query_map([], |row| SyncVersion::try_from(row))?;
        rows.collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(id: &str, updated_at: &str, tags: Vec<&str>) -> Snippet {
        Snippet {
            id: id.into(),
            title: format!("Snippet {id}"),
            content: "content".into(),
            language: "rust".into(),
            description: String::new(),
            tags: tags.into_iter().map(str::to_string).collect(),
            is_favorite: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: updated_at.into(),
            revision_id: String::new(),
        }
    }

    #[test]
    fn seeds_only_a_genuinely_new_database() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        let initial: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(initial, 7);

        conn.execute("DELETE FROM snippets", []).unwrap();
        initialize_connection(&mut conn).unwrap();
        let reopened: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(reopened, 0);
    }

    #[test]
    fn does_not_seed_an_existing_empty_v0_table() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE snippets (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                language TEXT NOT NULL DEFAULT 'plaintext',
                description TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                is_favorite INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
        )
        .unwrap();

        initialize_connection(&mut conn).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn merge_reports_insert_update_and_skip() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();

        let first = merge_snippets_on_connection(
            &conn,
            vec![snippet("one", "2026-01-01T00:00:00Z", vec!["Rust"])],
            false,
            REVISION_ORIGIN_REMOTE,
            false,
        )
        .unwrap();
        assert_eq!((first.inserted, first.updated, first.skipped), (1, 0, 0));

        let result = merge_snippets_on_connection(
            &conn,
            vec![
                snippet("one", "2026-01-02T00:00:00Z", vec!["Rust"]),
                snippet("two", "2026-01-01T00:00:00Z", vec!["Web"]),
                snippet("one", "2025-12-31T00:00:00Z", vec!["Old"]),
            ],
            false,
            REVISION_ORIGIN_REMOTE,
            false,
        )
        .unwrap();
        assert_eq!((result.inserted, result.updated, result.skipped), (1, 1, 1));
    }

    #[test]
    fn synchronization_merge_can_replace_an_equal_timestamp_winner() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();

        let local = snippet("one", "2026-01-01T00:00:00Z", vec!["Local"]);
        merge_snippets_on_connection(
            &conn,
            vec![local.clone()],
            false,
            REVISION_ORIGIN_REMOTE,
            false,
        )
        .unwrap();
        let mut winner = local.clone();
        winner.title = "deterministic-remote-winner".into();
        winner.tags = vec!["Remote".into()];

        let ordinary = merge_snippets_on_connection(
            &conn,
            vec![winner.clone()],
            false,
            REVISION_ORIGIN_REMOTE,
            false,
        )
        .unwrap();
        assert_eq!((ordinary.updated, ordinary.skipped), (0, 1));

        let synchronization = merge_snippets_on_connection(
            &conn,
            vec![winner.clone()],
            true,
            REVISION_ORIGIN_REMOTE,
            false,
        )
        .unwrap();
        assert_eq!((synchronization.updated, synchronization.skipped), (1, 0));
        let mut stored = get_snippet_on_connection(&conn, "one").unwrap();
        stored.revision_id.clear();
        assert_eq!(stored, winner);
        let search = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                query: "deterministic-remote-winner".into(),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(search.items.len(), 1);
        assert_eq!(search.items[0].id, "one");
    }

    #[test]
    fn invalid_batch_does_not_write_partial_results() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();

        let mut invalid = snippet("invalid", "not-a-time", vec!["Rust"]);
        invalid.title = "Invalid".into();
        let batch = vec![
            snippet("valid", "2026-01-01T00:00:00Z", vec!["Rust"]),
            invalid,
        ];
        assert!(validate_snippet_batch(&batch).is_err());

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    fn create_v1_schema(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE snippets (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                language TEXT NOT NULL DEFAULT 'plaintext',
                description TEXT NOT NULL DEFAULT '',
                tags TEXT NOT NULL DEFAULT '[]',
                is_favorite INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE sync_versions (
                id TEXT PRIMARY KEY,
                synced_at TEXT NOT NULL,
                direction TEXT NOT NULL,
                snippet_count INTEGER NOT NULL DEFAULT 0,
                uploaded_count INTEGER NOT NULL DEFAULT 0,
                downloaded_count INTEGER NOT NULL DEFAULT 0,
                message TEXT NOT NULL DEFAULT ''
            );
            PRAGMA user_version=1;",
        )
        .unwrap();
    }

    fn insert_on_connection(conn: &Connection, item: &Snippet) {
        let tags = serde_json::to_string(&item.tags).unwrap();
        conn.execute(
            "INSERT INTO snippets (id, title, content, language, description, tags, is_favorite, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                item.id,
                item.title,
                item.content,
                item.language,
                item.description,
                tags,
                item.is_favorite as i64,
                item.created_at,
                item.updated_at
            ],
        )
        .unwrap();
        if table_exists(conn, "snippet_heads").unwrap_or(false) {
            let payload = canonical_revision_payload(item, false).unwrap();
            let hash = sha256_hex(payload.as_bytes());
            let revision_id = if item.revision_id.is_empty() {
                uuid::Uuid::new_v4().to_string()
            } else {
                item.revision_id.clone()
            };
            conn.execute(
                "INSERT INTO snippet_heads
                 (snippet_id, revision_id, parent_revision_id, device_id, content_hash, revision_time, deleted)
                 VALUES (?1, ?2, NULL, 'test-device', ?3, ?4, 0)",
                rusqlite::params![item.id, revision_id, hash, item.updated_at],
            )
            .unwrap();
        }
    }

    #[test]
    fn strict_decoders_reject_corrupt_rows_without_content_diagnostics() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();
        conn.execute(
            "INSERT INTO snippets VALUES ('bad', 'secret title', 'FULL SECRET CONTENT', 'rust', '', 'not-json', 2, 'not-time', 'not-time')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO snippet_heads VALUES ('bad', 'bad-revision', NULL, 'test-device', ?1, '2026-01-01T00:00:00Z', 0)",
            ["0".repeat(64)],
        )
        .unwrap();

        let error = get_all_snippets_on_connection(&conn).unwrap_err();
        let diagnostic = error.to_string();
        assert!(!diagnostic.contains("FULL SECRET CONTENT"));
        assert!(!diagnostic.contains("secret title"));
    }

    #[test]
    fn strict_snippet_decoder_rejects_required_types_tags_booleans_and_timestamps() {
        let conn = Connection::open_in_memory().unwrap();
        let cases = [
            "SELECT NULL, 'title', 'content', 'rust', '', '[]', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'revision'",
            "SELECT 'id', 'title', X'00', 'rust', '', '[]', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'revision'",
            "SELECT 'id', 'title', 'content', 'rust', '', '\"tag\"', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'revision'",
            "SELECT 'id', 'title', 'content', 'rust', '', '[\"duplicate\",\"duplicate\"]', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'revision'",
            "SELECT 'id', 'title', 'content', 'rust', '', '[]', 2, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 'revision'",
            "SELECT 'id', 'title', 'content', 'rust', '', '[]', 0, 'not-time', '2026-01-01T00:00:00Z', 'revision'",
            "SELECT 'id', 'title', 'content', 'rust', '', '[]', 0, '2026-01-01T00:00:00Z', 'not-time', 'revision'",
        ];
        for sql in cases {
            let error = conn
                .query_row(sql, [], |row| Snippet::try_from(row))
                .unwrap_err();
            assert!(
                matches!(
                    error,
                    rusqlite::Error::FromSqlConversionFailure(..)
                        | rusqlite::Error::InvalidColumnType(..)
                ),
                "unexpected decoder error for {sql}: {error}"
            );
        }
    }

    #[test]
    fn strict_sync_history_decoder_rejects_invalid_timestamp_and_counts() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO sync_versions
             (id, synced_at, direction, snippet_count, uploaded_count, downloaded_count,
              deleted_count, conflict_count, protocol_version, generation, message)
             VALUES ('history', 'bad-time', 'merge', -1, 0, 0, 0, 0, 1, 0, 'message')",
            [],
        )
        .unwrap();
        let error = conn
            .query_row(
                "SELECT id, synced_at, direction, snippet_count, uploaded_count, downloaded_count,
                        deleted_count, conflict_count, protocol_version, generation, message
                 FROM sync_versions",
                [],
                |row| SyncVersion::try_from(row),
            )
            .unwrap_err();
        assert!(matches!(
            error,
            rusqlite::Error::FromSqlConversionFailure(..)
        ));
    }

    #[test]
    fn strict_metadata_and_merge_timestamp_reads_reject_corruption() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();
        conn.execute(
            "UPDATE app_metadata SET value='unexpected' WHERE key='fts_tokenizer'",
            [],
        )
        .unwrap();
        assert!(matches!(
            fts_tokenizer(&conn).unwrap_err(),
            rusqlite::Error::FromSqlConversionFailure(..)
        ));

        conn.execute(
            "UPDATE app_metadata SET value='unicode61' WHERE key='fts_tokenizer'",
            [],
        )
        .unwrap();
        let corrupt = snippet("existing", "2026-01-01T00:00:00Z", vec!["Rust"]);
        insert_on_connection(&conn, &corrupt);
        conn.execute(
            "UPDATE snippets SET updated_at='not-rfc3339' WHERE id='existing'",
            [],
        )
        .unwrap();
        let incoming = snippet("existing", "2026-01-02T00:00:00Z", vec!["Rust"]);
        assert!(matches!(
            merge_snippets_on_connection(
                &conn,
                vec![incoming],
                false,
                REVISION_ORIGIN_REMOTE,
                false
            )
            .unwrap_err(),
            rusqlite::Error::FromSqlConversionFailure(..)
        ));
    }

    #[test]
    fn import_accepts_legacy_and_envelope_and_rejects_future_before_merge() {
        let item = snippet("compatible", "2026-01-02T00:00:00Z", vec!["Rust"]);
        let legacy = serde_json::to_string(&vec![item.clone()]).unwrap();
        assert_eq!(parse_import_document(&legacy).unwrap(), vec![item.clone()]);

        let envelope = ExportEnvelope {
            format_id: EXPORT_FORMAT_ID.into(),
            schema_version: EXPORT_SCHEMA_VERSION,
            app_version: "2.1.0".into(),
            exported_at: "2026-01-02T00:00:00Z".into(),
            snippets: vec![item.clone()],
        };
        assert_eq!(
            parse_import_document(&serde_json::to_string(&envelope).unwrap()).unwrap(),
            vec![item]
        );

        let future = serde_json::json!({
            "format_id": EXPORT_FORMAT_ID,
            "schema_version": EXPORT_SCHEMA_VERSION + 1,
            "app_version": "999.0.0",
            "exported_at": "2026-01-02T00:00:00Z",
            "snippets": []
        });
        assert!(parse_import_document(&future.to_string()).is_err());

        let malformed = serde_json::json!({
            "format_id": EXPORT_FORMAT_ID,
            "schema_version": EXPORT_SCHEMA_VERSION,
            "app_version": "2.1.0",
            "exported_at": "not-rfc3339",
            "snippets": []
        });
        assert!(parse_import_document(&malformed.to_string()).is_err());
    }

    #[test]
    fn collision_safe_export_uses_deterministic_suffix() {
        let directory =
            std::env::temp_dir().join(format!("snipvault-export-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        write_export_file(&directory, "backup", "first").unwrap();
        write_export_file(&directory, "backup", "second").unwrap();
        assert_eq!(
            fs::read_to_string(directory.join("backup.json")).unwrap(),
            "first"
        );
        assert_eq!(
            fs::read_to_string(directory.join("backup-1.json")).unwrap(),
            "second"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn migrates_empty_v1_without_seeding_or_losing_schema_state() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_v1_schema(&conn);
        initialize_connection(&mut conn).unwrap();

        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(table_exists(&conn, "snippets_fts").unwrap());
        assert!(table_exists(&conn, "app_metadata").unwrap());
    }

    #[test]
    fn migrates_populated_v1_and_keeps_fts_synchronized() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_v1_schema(&conn);
        let mut item = snippet("fts", "2026-01-02T00:00:00Z", vec!["CJK"]);
        item.content = "前缀中文搜索后缀".into();
        insert_on_connection(&conn, &item);
        initialize_connection(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);

        let result = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                query: "中文搜".into(),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.items[0].id, "fts");

        conn.execute(
            "UPDATE snippets SET content='transactionally updated phrase' WHERE id='fts'",
            [],
        )
        .unwrap();
        let result = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                query: "updated".into(),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.items[0].id, "fts");
        conn.execute("DELETE FROM snippets WHERE id='fts'", [])
            .unwrap();
        let result = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                query: "updated".into(),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(result.items.is_empty());
    }

    #[test]
    fn migration_rejects_corrupt_backfill_and_rolls_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_v1_schema(&conn);
        conn.execute(
            "INSERT INTO snippets VALUES ('bad', '', 'content', 'rust', '', 'not-json', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        assert!(migrate_to_v2(&mut conn).is_err());
        assert_eq!(schema_version(&conn).unwrap(), 1);
        assert!(!table_exists(&conn, "app_metadata").unwrap());
    }

    #[test]
    fn disk_migration_creates_pre_v4_backups_and_restores_source_versions() {
        let directory =
            std::env::temp_dir().join(format!("snipvault-migration-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();

        let new_path = directory.join("new.db");
        drop(open_database_with_recovery(&new_path).unwrap());
        assert!(fs::read_dir(&directory).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("new.db.pre-v4")));

        let current_path = directory.join("current.db");
        {
            let mut current = Connection::open(&current_path).unwrap();
            initialize_connection(&mut current).unwrap();
        }
        drop(open_database_with_recovery(&current_path).unwrap());
        assert!(fs::read_dir(&directory).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("current.db.pre-v4")));

        let high_path = directory.join("future.db");
        {
            let high = Connection::open(&high_path).unwrap();
            high.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .unwrap();
        }
        assert!(open_database_with_recovery(&high_path).is_err());
        assert!(fs::read_dir(&directory).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("future.db.pre-v4")));

        let corrupt_v1_path = directory.join("corrupt-v1.db");
        {
            let corrupt = Connection::open(&corrupt_v1_path).unwrap();
            create_v1_schema(&corrupt);
            corrupt
                .execute(
                    "INSERT INTO snippets VALUES ('bad', '', 'content', 'rust', '', 'not-json', 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                    [],
                )
                .unwrap();
        }
        assert!(open_database_with_recovery(&corrupt_v1_path).is_err());
        let restored_v1 = Connection::open(&corrupt_v1_path).unwrap();
        assert_eq!(schema_version(&restored_v1).unwrap(), 1);
        let tags: String = restored_v1
            .query_row("SELECT tags FROM snippets WHERE id='bad'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(tags, "not-json");
        assert!(fs::read_dir(&directory).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("corrupt-v1.db.pre-v4")));
        drop(restored_v1);

        let corrupt_v2_path = directory.join("corrupt-v2.db");
        {
            let mut corrupt = Connection::open(&corrupt_v2_path).unwrap();
            create_v2_schema(&mut corrupt);
            let item = snippet("bad-v2", "2026-01-02T00:00:00Z", vec!["Rust"]);
            insert_on_connection(&corrupt, &item);
            corrupt
                .execute("UPDATE snippets SET tags='not-json' WHERE id='bad-v2'", [])
                .unwrap();
        }
        assert!(open_database_with_recovery(&corrupt_v2_path).is_err());
        let restored_v2 = Connection::open(&corrupt_v2_path).unwrap();
        assert_eq!(schema_version(&restored_v2).unwrap(), 2);
        let tags: String = restored_v2
            .query_row("SELECT tags FROM snippets WHERE id='bad-v2'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(tags, "not-json");
        assert!(!table_exists(&restored_v2, "snippet_heads").unwrap());
        assert!(fs::read_dir(&directory).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("corrupt-v2.db.pre-v4")));

        drop(restored_v2);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn repeated_current_open_and_high_version_are_safe() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        initialize_connection(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        conn.pragma_update(None, "user_version", 999).unwrap();
        assert!(initialize_connection(&mut conn).is_err());
        assert_eq!(schema_version(&conn).unwrap(), 999);
    }

    fn create_v2_schema(conn: &mut Connection) {
        create_v1_schema(conn);
        migrate_to_v2(conn).unwrap();
    }

    #[test]
    fn migrates_v2_with_stable_device_and_deterministic_legacy_heads() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_v2_schema(&mut conn);
        let item = snippet("legacy", "2026-01-02T00:00:00Z", vec!["Rust"]);
        insert_on_connection(&conn, &item);

        migrate_to_v3(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 3);
        migrate_to_v4(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        let first_device = local_device_id(&conn).unwrap();
        assert!(uuid::Uuid::parse_str(&first_device).is_ok());
        let first = get_snippet_on_connection(&conn, "legacy").unwrap();
        assert!(first.revision_id.starts_with("legacy-"));
        let expected_payload = canonical_revision_payload(&item, false).unwrap();
        assert_eq!(
            first.revision_id,
            legacy_revision_id(
                "legacy",
                &sha256_hex(expected_payload.as_bytes()),
                &item.updated_at
            )
        );
        assert_eq!(pending_usage(&conn).unwrap(), (0, 0));

        let mut export_item = item.clone();
        export_item.revision_id = first.revision_id.clone();
        let envelope = ExportEnvelope {
            format_id: EXPORT_FORMAT_ID.into(),
            schema_version: EXPORT_SCHEMA_VERSION,
            app_version: "2.1.0".into(),
            exported_at: "2026-01-02T00:00:00Z".into(),
            snippets: vec![export_item.clone()],
        };
        assert_eq!(
            parse_import_document(&serde_json::to_string(&envelope).unwrap()).unwrap(),
            vec![export_item]
        );

        initialize_connection(&mut conn).unwrap();
        assert_eq!(local_device_id(&conn).unwrap(), first_device);
        assert_eq!(
            get_snippet_on_connection(&conn, "legacy")
                .unwrap()
                .revision_id,
            first.revision_id
        );
    }

    #[test]
    fn migrates_v3_revision_objects_for_outbox_live_and_tombstone_heads() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_v2_schema(&mut conn);
        conn.execute(
            "INSERT INTO snippets VALUES ('legacy', 'legacy', 'body', 'text', '', '[]', 0,
             '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z')",
            [],
        )
        .unwrap();
        migrate_to_v3(&mut conn).unwrap();
        let device_id = local_device_id(&conn).unwrap();

        let live = get_snippet_on_connection(&conn, "legacy").unwrap();
        let live_payload = canonical_revision_payload(&live, false).unwrap();
        let pending_id = uuid::Uuid::new_v4().to_string();
        let mut pending = live.clone();
        pending.id = "pending".into();
        pending.revision_id = pending_id.clone();
        pending.title = "pending".into();
        pending.updated_at = "2026-01-03T00:00:00Z".into();
        let pending_payload = canonical_revision_payload(&pending, false).unwrap();
        conn.execute(
            "INSERT INTO revision_outbox
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash, revision_time,
              deleted, operation_kind, origin, payload_json, payload_bytes, created_at)
             VALUES (?1, ?2, NULL, ?3, ?4, ?5, 0, 'upsert', 'local', ?6, ?7, ?5)",
            rusqlite::params![
                pending_id,
                pending.id,
                device_id,
                sha256_hex(pending_payload.as_bytes()),
                pending.updated_at,
                pending_payload,
                pending_payload.len() as i64
            ],
        )
        .unwrap();

        let tombstone_id = uuid::Uuid::new_v4().to_string();
        let tombstone_time = "2026-01-04T00:00:00Z";
        let tombstone_payload =
            canonical_tombstone_payload_sqlite("deleted", tombstone_time).unwrap();
        conn.execute(
            "INSERT INTO snippet_heads
             (snippet_id, revision_id, parent_revision_id, device_id, content_hash, revision_time, deleted)
             VALUES ('deleted', ?1, NULL, ?2, ?3, ?4, 1)",
            rusqlite::params![
                tombstone_id,
                device_id,
                sha256_hex(tombstone_payload.as_bytes()),
                tombstone_time
            ],
        )
        .unwrap();

        migrate_to_v4(&mut conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), SCHEMA_VERSION);
        assert_eq!(local_device_id(&conn).unwrap(), device_id);
        for (revision_id, expected_payload, deleted) in [
            (live.revision_id.as_str(), live_payload.as_str(), 0_i64),
            (pending_id.as_str(), pending_payload.as_str(), 0_i64),
            (tombstone_id.as_str(), tombstone_payload.as_str(), 1_i64),
        ] {
            let stored: (String, i64, i64) = conn
                .query_row(
                    "SELECT payload_json, payload_bytes, deleted FROM revision_objects WHERE revision_id=?1",
                    [revision_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                stored,
                (
                    expected_payload.into(),
                    expected_payload.len() as i64,
                    deleted
                )
            );
        }
        initialize_connection(&mut conn).unwrap();
        let object_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM revision_objects", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(object_count, 3);
    }

    #[test]
    fn v4_migration_rejects_malformed_outbox_and_rolls_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        create_v2_schema(&mut conn);
        migrate_to_v3(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO revision_outbox
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash, revision_time,
              deleted, operation_kind, origin, payload_json, payload_bytes, created_at)
             VALUES ('bad', 'bad', NULL, 'device', ?1, '2026-01-01T00:00:00Z', 0,
                     'upsert', 'local', '{}', 999, '2026-01-01T00:00:00Z')",
            ["0".repeat(64)],
        )
        .unwrap();

        assert!(migrate_to_v4(&mut conn).is_err());
        assert_eq!(schema_version(&conn).unwrap(), 3);
        assert!(!table_exists(&conn, "revision_objects").unwrap());
        let outbox_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM revision_outbox", [], |row| row.get(0))
            .unwrap();
        assert_eq!(outbox_count, 1);
    }

    #[test]
    fn local_mutations_are_atomic_revisioned_and_delete_removes_fts() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();
        conn.execute("DELETE FROM snippet_heads", []).unwrap();
        conn.execute("DELETE FROM revision_outbox", []).unwrap();

        let tx = conn.transaction().unwrap();
        let created = create_snippet_on_connection(
            &tx,
            &snippet("atomic", "2026-01-01T00:00:00Z", vec!["Rust"]),
            REVISION_ORIGIN_LOCAL,
        )
        .unwrap();
        tx.commit().unwrap();
        assert!(validate_revision_token(&created.revision_id));
        assert_eq!(pending_usage(&conn).unwrap().0, 1);

        let mut changed = created.clone();
        changed.content = "searchable updated body".into();
        let tx = conn.transaction().unwrap();
        let payload = canonical_revision_payload(&changed, false).unwrap();
        let device = local_device_id(&tx).unwrap();
        changed.revision_id = uuid::Uuid::new_v4().to_string();
        write_snippet_row(&tx, &changed).unwrap();
        insert_revision(
            &tx,
            &changed.id,
            Some(&created.revision_id),
            &changed.revision_id,
            &device,
            &sha256_hex(payload.as_bytes()),
            &changed.updated_at,
            false,
            OUTBOX_KIND_UPSERT,
            REVISION_ORIGIN_LOCAL,
            &payload,
            None,
            true,
        )
        .unwrap();
        tx.commit().unwrap();
        assert_eq!(pending_usage(&conn).unwrap().0, 2);
        assert_eq!(
            query_snippets_on_connection(
                &conn,
                &SnippetQuery {
                    query: "updated".into(),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            1
        );

        let tx = conn.transaction().unwrap();
        let deleted_at = "2026-01-03T00:00:00Z";
        let tombstone = canonical_tombstone_payload_sqlite("atomic", deleted_at).unwrap();
        tx.execute("DELETE FROM snippets WHERE id='atomic'", [])
            .unwrap();
        insert_revision(
            &tx,
            "atomic",
            Some(&changed.revision_id),
            "delete-revision",
            &device,
            &sha256_hex(tombstone.as_bytes()),
            deleted_at,
            true,
            OUTBOX_KIND_DELETE,
            REVISION_ORIGIN_LOCAL,
            &tombstone,
            None,
            true,
        )
        .unwrap();
        tx.commit().unwrap();
        assert!(get_snippet_on_connection(&conn, "atomic").is_err());
        assert_eq!(
            query_snippets_on_connection(
                &conn,
                &SnippetQuery {
                    query: "updated".into(),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            0
        );
        assert_eq!(pending_usage(&conn).unwrap().0, 3);
    }

    #[test]
    fn stale_base_and_outbox_failures_roll_back_without_partial_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();
        conn.execute("DELETE FROM snippet_heads", []).unwrap();
        conn.execute("DELETE FROM revision_outbox", []).unwrap();
        let tx = conn.transaction().unwrap();
        let created = create_snippet_on_connection(
            &tx,
            &snippet("stale", "2026-01-01T00:00:00Z", vec![]),
            REVISION_ORIGIN_LOCAL,
        )
        .unwrap();
        tx.commit().unwrap();

        let before = pending_usage(&conn).unwrap();
        let current = get_snippet_on_connection(&conn, "stale").unwrap();
        let mut stale_attempt = current.clone();
        stale_attempt.title = "stale draft".into();
        assert!(matches!(
            update_snippet_on_connection(&conn, &stale_attempt, "wrong-base"),
            Err(MutationError::StaleRevision { .. })
        ));
        assert_eq!(pending_usage(&conn).unwrap(), before);
        assert_eq!(current.title, created.title);

        conn.execute_batch(
            "CREATE TRIGGER fail_outbox_insert BEFORE INSERT ON revision_outbox
             BEGIN SELECT RAISE(ABORT, 'test outbox failure'); END;",
        )
        .unwrap();
        let tx = conn.transaction().unwrap();
        let mut attempted = current.clone();
        attempted.title = "must roll back".into();
        attempted.revision_id = uuid::Uuid::new_v4().to_string();
        let payload = canonical_revision_payload(&attempted, false).unwrap();
        write_snippet_row(&tx, &attempted).unwrap();
        assert!(insert_revision(
            &tx,
            &attempted.id,
            Some(&current.revision_id),
            &attempted.revision_id,
            &local_device_id(&tx).unwrap(),
            &sha256_hex(payload.as_bytes()),
            &attempted.updated_at,
            false,
            OUTBOX_KIND_UPSERT,
            REVISION_ORIGIN_LOCAL,
            &payload,
            None,
            true,
        )
        .is_err());
        drop(tx);
        assert_eq!(get_snippet_on_connection(&conn, "stale").unwrap(), current);
        assert_eq!(pending_usage(&conn).unwrap(), before);
    }

    #[test]
    fn remote_plan_is_no_echo_conflict_idempotent_and_exact_ack_preserves_later_edits() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();
        conn.execute("DELETE FROM snippet_heads", []).unwrap();
        conn.execute("DELETE FROM revision_outbox", []).unwrap();
        let tx = conn.transaction().unwrap();
        let local = create_snippet_on_connection(
            &tx,
            &snippet("remote", "2026-01-01T00:00:00Z", vec![]),
            REVISION_ORIGIN_LOCAL,
        )
        .unwrap();
        tx.commit().unwrap();
        let original_pending = load_sync_snapshot_on_connection(&conn, "remote-test")
            .unwrap()
            .pending;

        let mut incoming = local.clone();
        incoming.title = "remote winner".into();
        incoming.updated_at = "2026-01-02T00:00:00Z".into();
        incoming.revision_id.clear();
        let incoming_payload = canonical_revision_payload(&incoming, false).unwrap();
        let plan = ValidatedRemotePlan {
            remote_id: "remote-test".into(),
            protocol_version: 2,
            generation: 1,
            manifest_etag: Some("etag-1".into()),
            manifest_hash: Some("a".repeat(64)),
            entries: vec![RemotePlanEntry {
                snippet_id: incoming.id.clone(),
                revision_id: "remote-revision".into(),
                parent_revision_id: Some(local.revision_id.clone()),
                device_id: "remote-device".into(),
                content_hash: sha256_hex(incoming_payload.as_bytes()),
                revision_time: incoming.updated_at.clone(),
                deleted: false,
                snippet: Some(incoming.clone()),
                expected_local_revision_id: Some(local.revision_id.clone()),
                preserve_local_as_conflict: true,
            }],
        };
        let result = apply_validated_remote_plan_on_connection(&mut conn, &plan).unwrap();
        assert_eq!(
            result,
            ApplyRemotePlanResult {
                applied: 1,
                skipped: 0,
                conflicts_created: 1,
            }
        );
        let repeated = apply_validated_remote_plan_on_connection(&mut conn, &plan).unwrap();
        assert_eq!(
            repeated,
            ApplyRemotePlanResult {
                applied: 0,
                skipped: 1,
                conflicts_created: 0,
            }
        );
        let after_remote = load_sync_snapshot_on_connection(&conn, "remote-test").unwrap();
        assert_eq!(after_remote.pending.len(), original_pending.len() + 1);
        let conflict_pending = after_remote
            .pending
            .iter()
            .find(|revision| revision.origin == "conflict")
            .expect("conflict copy must remain pending for publication");
        assert_eq!(
            conflict_pending.conflict_of.as_deref(),
            Some("remote-revision")
        );
        let pending_after_remote = after_remote.pending.clone();
        let conflict_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sync_conflicts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(conflict_count, 1);
        let before_invalid = get_snippet_on_connection(&conn, "remote").unwrap();
        let mut invalid_plan = plan.clone();
        invalid_plan.entries[0].revision_id = "different-remote-revision".into();
        invalid_plan.entries[0].content_hash = "0".repeat(64);
        assert!(apply_validated_remote_plan_on_connection(&mut conn, &invalid_plan).is_err());
        assert_eq!(
            get_snippet_on_connection(&conn, "remote").unwrap(),
            before_invalid
        );
        assert_eq!(
            load_sync_snapshot_on_connection(&conn, "remote-test")
                .unwrap()
                .pending,
            pending_after_remote
        );

        let later_revision = uuid::Uuid::new_v4().to_string();
        let mut later = get_snippet_on_connection(&conn, "remote").unwrap();
        later.title = "later local edit".into();
        later.updated_at = "2026-01-03T00:00:00Z".into();
        let parent = later.revision_id.clone();
        later.revision_id = later_revision.clone();
        let payload = canonical_revision_payload(&later, false).unwrap();
        let tx = conn.transaction().unwrap();
        write_snippet_row(&tx, &later).unwrap();
        insert_revision(
            &tx,
            &later.id,
            Some(&parent),
            &later_revision,
            &local_device_id(&tx).unwrap(),
            &sha256_hex(payload.as_bytes()),
            &later.updated_at,
            false,
            OUTBOX_KIND_UPSERT,
            REVISION_ORIGIN_LOCAL,
            &payload,
            None,
            true,
        )
        .unwrap();
        tx.commit().unwrap();
        let original_revision = original_pending[0].revision_id.clone();
        let commit = PublishCommit {
            remote_id: "remote-test".into(),
            vault_id: uuid::Uuid::new_v4().to_string(),
            protocol_version: 2,
            manifest_etag: Some("etag-2".into()),
            manifest_hash: Some("b".repeat(64)),
            generation: 2,
            acknowledged_revision_ids: vec![original_revision.clone()],
            snippet_count: 2,
            uploaded_count: 1,
            downloaded_count: 1,
            deleted_count: 0,
            conflict_count: 1,
            message: "published".into(),
            succeeded_at: "2026-01-04T00:00:00Z".into(),
        };
        assert_eq!(
            commit_published_revisions_on_connection(&mut conn, &commit).unwrap(),
            1
        );
        assert_eq!(
            commit_published_revisions_on_connection(&mut conn, &commit).unwrap(),
            0
        );
        let pending = load_sync_snapshot_on_connection(&conn, "remote-test")
            .unwrap()
            .pending;
        assert_eq!(pending.len(), 2);
        assert!(pending
            .iter()
            .any(|revision| revision.revision_id == later_revision));
        assert!(pending
            .iter()
            .any(|revision| revision.revision_id == conflict_pending.revision_id));
        let remote_state = load_sync_snapshot_on_connection(&conn, "remote-test")
            .unwrap()
            .remote
            .unwrap();
        assert_eq!(remote_state.generation, 2);
        assert_eq!(
            remote_state.last_success_at.as_deref(),
            Some(commit.succeeded_at.as_str())
        );

        let invalid = PublishCommit {
            acknowledged_revision_ids: vec![later_revision.clone()],
            succeeded_at: "not-a-time".into(),
            ..commit.clone()
        };
        assert!(commit_published_revisions_on_connection(&mut conn, &invalid).is_err());
        let pending = load_sync_snapshot_on_connection(&conn, "remote-test")
            .unwrap()
            .pending;
        assert_eq!(pending.len(), 2);
        assert!(pending
            .iter()
            .any(|revision| revision.revision_id == later_revision));
        assert!(pending
            .iter()
            .any(|revision| revision.revision_id == conflict_pending.revision_id));
        let original_still_pending: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM revision_outbox WHERE revision_id=?1)",
                [&original_revision],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!original_still_pending);
    }

    #[test]
    fn pending_limit_boundaries_are_inclusive_and_one_over_is_rejected() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM revision_outbox", []).unwrap();

        let filler_bytes = MAX_PENDING_OUTBOX_BYTES - MAX_REVISION_PAYLOAD_BYTES;
        conn.execute(
            "INSERT INTO revision_outbox
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash,
              revision_time, deleted, operation_kind, origin, payload_json, payload_bytes, created_at)
             VALUES ('filler', 'filler', NULL, 'device', ?1, '2026-01-01T00:00:00Z', 0,
                     'upsert', 'local', '{}', ?2, '2026-01-01T00:00:00Z')",
            rusqlite::params!["0".repeat(64), filler_bytes as i64],
        )
        .unwrap();
        assert!(ensure_pending_capacity(&conn, MAX_REVISION_PAYLOAD_BYTES).is_ok());
        assert!(ensure_pending_capacity(&conn, MAX_REVISION_PAYLOAD_BYTES + 1).is_err());
        assert!(
            ensure_pending_capacity(&conn, MAX_REVISION_PAYLOAD_BYTES.saturating_sub(1)).is_ok()
        );

        conn.execute(
            "UPDATE revision_outbox SET payload_bytes=payload_bytes+1 WHERE revision_id='filler'",
            [],
        )
        .unwrap_err();
        conn.execute("DELETE FROM revision_outbox", []).unwrap();
        for batch_start in (0..MAX_PENDING_OUTBOX_COUNT - 1).step_by(500) {
            let batch_end = (batch_start + 500).min(MAX_PENDING_OUTBOX_COUNT - 1);
            let mut sql = String::from(
                "INSERT INTO revision_outbox
                 (revision_id, snippet_id, parent_revision_id, device_id, content_hash,
                  revision_time, deleted, operation_kind, origin, payload_json, payload_bytes, created_at) VALUES ",
            );
            for index in batch_start..batch_end {
                if index > batch_start {
                    sql.push(',');
                }
                sql.push_str(&format!(
                    "('revision-{index}', 'snippet-{index}', NULL, 'device', '{}', '2026-01-01T00:00:00Z', 0, 'upsert', 'local', '{{}}', 2, '2026-01-01T00:00:00Z')",
                    "0".repeat(64)
                ));
            }
            conn.execute_batch(&sql).unwrap();
        }
        assert_eq!(
            pending_usage(&conn).unwrap().0,
            MAX_PENDING_OUTBOX_COUNT - 1
        );
        assert!(ensure_pending_capacity(&conn, 0).is_ok());
        conn.execute(
            "INSERT INTO revision_outbox
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash,
              revision_time, deleted, operation_kind, origin, payload_json, payload_bytes, created_at)
             VALUES ('count-limit', 'count-limit', NULL, 'device', ?1, '2026-01-01T00:00:00Z', 0,
                     'upsert', 'local', '', 0, '2026-01-01T00:00:00Z')",
            ["0".repeat(64)],
        )
        .unwrap();
        assert_eq!(pending_usage(&conn).unwrap().0, MAX_PENDING_OUTBOX_COUNT);
        assert!(ensure_pending_capacity(&conn, 0).is_err());
    }

    #[test]
    fn strict_outbox_decoder_and_limits_reject_corruption() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM revision_outbox", []).unwrap();
        conn.execute(
            "INSERT INTO revision_outbox
             (revision_id, snippet_id, parent_revision_id, device_id, content_hash,
              revision_time, deleted, operation_kind, origin, payload_json, payload_bytes, created_at)
             VALUES ('bad', 'id', NULL, 'device', ?1, '2026-01-01T00:00:00Z', 0,
                     'upsert', 'local', '{}', 999, '2026-01-01T00:00:00Z')",
            ["0".repeat(64)],
        )
        .unwrap();
        let error = load_sync_snapshot_on_connection(&conn, "remote-test").unwrap_err();
        assert!(matches!(
            error,
            rusqlite::Error::FromSqlConversionFailure(..)
        ));
        conn.execute("DELETE FROM revision_outbox", []).unwrap();
        assert!(ensure_pending_capacity(&conn, MAX_REVISION_PAYLOAD_BYTES + 1).is_err());
    }

    #[test]
    fn query_composes_filters_paginates_and_treats_literals_safely() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();
        let mut first = snippet("b", "2026-01-03T00:00:00Z", vec!["100%", "C++"]);
        first.title = "literal % _ quote \" backslash \\".into();
        first.is_favorite = true;
        insert_on_connection(&conn, &first);
        let mut second = snippet("a", "2026-01-03T00:00:00Z", vec!["Other"]);
        second.language = "python".into();
        insert_on_connection(&conn, &second);

        for query in ["%", "_", "quote \"", "backslash \\"] {
            let result = query_snippets_on_connection(
                &conn,
                &SnippetQuery {
                    query: query.into(),
                    limit: Some(10),
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(result.items.len(), 1, "literal query {query}");
        }
        let tag_match = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                query: "C++".into(),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(tag_match.items.len(), 1);
        assert_eq!(tag_match.items[0].id, "b");

        let first_page = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                limit: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(first_page.items[0].id, "b");
        assert_eq!(first_page.total, 2);
        let second_page = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                limit: Some(1),
                cursor: first_page.next_cursor,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(second_page.items[0].id, "a");

        let capped = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                limit: Some(MAX_PAGE_SIZE + 1_000),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(capped.items.len(), 2);

        let filtered = query_snippets_on_connection(
            &conn,
            &SnippetQuery {
                language: Some("rust".into()),
                favorite: Some(true),
                exact_tag: Some("100%".into()),
                limit: Some(10),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(filtered.items.len(), 1);
    }

    #[test]
    #[ignore = "benchmark-style checkpoint; run explicitly with --ignored --nocapture"]
    fn benchmark_query_1k_and_10k() {
        for count in [1_000, 10_000] {
            let mut conn = Connection::open_in_memory().unwrap();
            initialize_connection(&mut conn).unwrap();
            conn.execute("DELETE FROM snippets", []).unwrap();
            let tx = conn.transaction().unwrap();
            for index in 0..count {
                let mut item = snippet(
                    &format!("bench-{index:05}"),
                    "2026-01-01T00:00:00Z",
                    vec!["bench"],
                );
                item.title = format!("benchmark item {index}");
                item.content = format!("payload searchable needle {index}");
                insert_on_connection(&tx, &item);
            }
            tx.commit().unwrap();
            let started = std::time::Instant::now();
            let result = query_snippets_on_connection(
                &conn,
                &SnippetQuery {
                    query: "needle".into(),
                    limit: Some(100),
                    ..Default::default()
                },
            )
            .unwrap();
            let payload = serde_json::to_vec(&result).unwrap().len();
            println!(
                "rows={count} returned={} total={} payload_bytes={payload} latency_us={}",
                result.items.len(),
                result.total,
                started.elapsed().as_micros()
            );
        }
    }

    #[test]
    fn exact_tag_filter_handles_json_characters() {
        let mut conn = Connection::open_in_memory().unwrap();
        initialize_connection(&mut conn).unwrap();
        conn.execute("DELETE FROM snippets", []).unwrap();
        let item = snippet("one", "2026-01-01T00:00:00Z", vec!["C++", "100%"]);
        let tags = serde_json::to_string(&item.tags).unwrap();
        conn.execute(
            "INSERT INTO snippets (id, title, content, language, description, tags, is_favorite, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8)",
            rusqlite::params![item.id, item.title, item.content, item.language, item.description, tags, item.created_at, item.updated_at],
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM snippets s WHERE EXISTS (
                   SELECT 1 FROM json_each(CASE WHEN json_valid(s.tags) THEN s.tags ELSE '[]' END) tag
                   WHERE tag.type='text' AND tag.value=?1
                 )",
                ["100%"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
