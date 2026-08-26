use crate::db;
use crate::paths::get_snapshots_dir;
use crate::settings::{self, SnapshotSettingsInput};
use once_cell::sync::Lazy;
use rusqlite::{backup::Backup, Connection, DatabaseName, OpenFlags};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

const SNAPSHOT_EXTENSION: &str = "sqlite";
const SNAPSHOT_POLL_INTERVAL: Duration = Duration::from_secs(15 * 60);
const SNAPSHOT_WORKER_FAILURE_BACKOFF: Duration = Duration::from_secs(60 * 60);
const MAX_SNAPSHOT_LIST: usize = 100;
const SCHEMA_VERSION: i64 = db::SCHEMA_VERSION;
const PREVIOUS_SCHEMA_VERSION: i64 = SCHEMA_VERSION - 1;

static SNAPSHOT_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));
static RESTORE_WRITE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

pub fn mutation_guard() -> std::sync::MutexGuard<'static, ()> {
    RESTORE_WRITE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

pub fn restore_guard() -> std::sync::MutexGuard<'static, ()> {
    mutation_guard()
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LocalSnapshot {
    pub id: String,
    pub created_at: String,
    pub schema_version: i64,
    pub byte_count: u64,
    pub snippet_count: usize,
    pub verified_at: String,
    pub unavailable_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotStatus {
    pub snapshots: Vec<LocalSnapshot>,
    pub latest_created_at: Option<String>,
    pub automatic_enabled: bool,
    pub frequency: String,
    pub retention: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RestoreResult {
    pub restored_snapshot_id: String,
    pub emergency_snapshot_id: String,
}

#[derive(Debug, Clone)]
struct CatalogEntry {
    id: String,
    filename: String,
    created_at: String,
    schema_version: i64,
    byte_count: u64,
    snippet_count: usize,
    checksum: String,
    verified_at: String,
    unavailable_at: Option<String>,
}

#[derive(Debug)]
struct SnapshotInspection {
    schema_version: i64,
    byte_count: u64,
    snippet_count: usize,
    checksum: String,
}

fn snapshot_error(message: &str) -> rusqlite::Error {
    rusqlite::Error::InvalidParameterName(message.into())
}

fn snapshot_now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn valid_snapshot_id(value: &str) -> bool {
    uuid::Uuid::parse_str(value)
        .map(|parsed| parsed.hyphenated().to_string() == value.to_ascii_lowercase())
        .unwrap_or(false)
}

fn filename_for_id(id: &str) -> String {
    format!("snapshot-{id}.{SNAPSHOT_EXTENSION}")
}

fn is_safe_filename(filename: &str) -> bool {
    let Some(stem) = filename
        .strip_prefix("snapshot-")
        .and_then(|value| value.strip_suffix(&format!(".{SNAPSHOT_EXTENSION}")))
    else {
        return false;
    };
    !filename.contains(['/', '\\']) && valid_snapshot_id(stem)
}

fn snapshot_path(filename: &str) -> Result<PathBuf, rusqlite::Error> {
    if !is_safe_filename(filename) {
        return Err(snapshot_error("invalid local snapshot catalog entry"));
    }
    Ok(get_snapshots_dir().join(filename))
}

fn temporary_snapshot_path(id: &str) -> PathBuf {
    get_snapshots_dir().join(format!(".snapshot-{id}.pending"))
}

fn ensure_snapshot_dir() -> Result<(), rusqlite::Error> {
    fs::create_dir_all(get_snapshots_dir())
        .map_err(|_| snapshot_error("snapshot directory unavailable"))
}

fn file_checksum(path: &Path) -> Result<(u64, String), rusqlite::Error> {
    let mut file = File::open(path).map_err(|_| snapshot_error("snapshot file unavailable"))?;
    let size = file
        .metadata()
        .map_err(|_| snapshot_error("snapshot file unavailable"))?
        .len();
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| snapshot_error("snapshot file unavailable"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

fn validate_snapshot_connection_at_version(
    conn: &Connection,
    allowed_schema_version: i64,
) -> Result<(i64, usize), rusqlite::Error> {
    let integrity: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        return Err(snapshot_error("snapshot integrity validation failed"));
    }
    let schema_version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if schema_version != allowed_schema_version {
        return Err(snapshot_error("snapshot schema is unsupported"));
    }
    let snippet_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM snippets s JOIN snippet_heads h ON h.snippet_id=s.id AND h.deleted=0",
        [],
        |row| row.get(0),
    )?;
    let snippet_count =
        usize::try_from(snippet_count).map_err(|_| snapshot_error("snapshot data is invalid"))?;
    let identity_count: i64 =
        conn.query_row("SELECT COUNT(*) FROM sync_identity", [], |row| row.get(0))?;
    if identity_count != 1 {
        return Err(snapshot_error("snapshot application state is invalid"));
    }
    Ok((schema_version, snippet_count))
}

fn validate_snapshot_connection(conn: &Connection) -> Result<(i64, usize), rusqlite::Error> {
    validate_snapshot_connection_at_version(conn, SCHEMA_VERSION)
}

fn inspect_snapshot_at_version(
    path: &Path,
    allowed_schema_version: i64,
) -> Result<SnapshotInspection, rusqlite::Error> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let (schema_version, snippet_count) =
        validate_snapshot_connection_at_version(&connection, allowed_schema_version)?;
    let (byte_count, checksum) = file_checksum(path)?;
    Ok(SnapshotInspection {
        schema_version,
        byte_count,
        snippet_count,
        checksum,
    })
}

fn inspect_snapshot(path: &Path) -> Result<SnapshotInspection, rusqlite::Error> {
    inspect_snapshot_at_version(path, SCHEMA_VERSION)
}

fn local_snapshot_from_catalog(entry: CatalogEntry) -> LocalSnapshot {
    LocalSnapshot {
        id: entry.id,
        created_at: entry.created_at,
        schema_version: entry.schema_version,
        byte_count: entry.byte_count,
        snippet_count: entry.snippet_count,
        verified_at: entry.verified_at,
        unavailable_at: entry.unavailable_at,
    }
}

fn decode_catalog_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<CatalogEntry> {
    let byte_count: i64 = row.get(4)?;
    let snippet_count: i64 = row.get(5)?;
    let entry = CatalogEntry {
        id: row.get(0)?,
        filename: row.get(1)?,
        created_at: row.get(2)?,
        schema_version: row.get(3)?,
        byte_count: u64::try_from(byte_count)
            .map_err(|_| snapshot_error("invalid local snapshot catalog entry"))?,
        snippet_count: usize::try_from(snippet_count)
            .map_err(|_| snapshot_error("invalid local snapshot catalog entry"))?,
        checksum: row.get(6)?,
        verified_at: row.get(7)?,
        unavailable_at: row.get(8)?,
    };
    if !valid_snapshot_id(&entry.id)
        || entry.filename != filename_for_id(&entry.id)
        || !is_safe_filename(&entry.filename)
        || entry.schema_version < PREVIOUS_SCHEMA_VERSION
        || entry.schema_version > SCHEMA_VERSION
        || entry.checksum.len() != 64
        || !entry.checksum.bytes().all(|byte| byte.is_ascii_hexdigit())
        || chrono::DateTime::parse_from_rfc3339(&entry.created_at).is_err()
        || chrono::DateTime::parse_from_rfc3339(&entry.verified_at).is_err()
        || entry
            .unavailable_at
            .as_deref()
            .is_some_and(|value| chrono::DateTime::parse_from_rfc3339(value).is_err())
    {
        return Err(snapshot_error("invalid local snapshot catalog entry"));
    }
    Ok(entry)
}

fn find_catalog_entry(conn: &Connection, id: &str) -> Result<CatalogEntry, rusqlite::Error> {
    if !valid_snapshot_id(id) {
        return Err(snapshot_error("invalid local snapshot request"));
    }
    conn.query_row(
        "SELECT id, filename, created_at, schema_version, byte_count, snippet_count, checksum, verified_at, unavailable_at
         FROM local_snapshots WHERE id=?1",
        [id],
        decode_catalog_entry,
    )
}

fn verify_catalog_entry(entry: &CatalogEntry) -> Result<PathBuf, rusqlite::Error> {
    let path = snapshot_path(&entry.filename)?;
    let allowed_version = if entry.schema_version == SCHEMA_VERSION {
        SCHEMA_VERSION
    } else if entry.schema_version == PREVIOUS_SCHEMA_VERSION {
        PREVIOUS_SCHEMA_VERSION
    } else {
        return Err(snapshot_error("snapshot schema is unsupported"));
    };
    let inspection = inspect_snapshot_at_version(&path, allowed_version)?;
    if inspection.schema_version != entry.schema_version
        || inspection.byte_count != entry.byte_count
        || inspection.snippet_count != entry.snippet_count
        || inspection.checksum != entry.checksum
    {
        return Err(snapshot_error("snapshot verification failed"));
    }
    Ok(path)
}

fn mark_unavailable(conn: &Connection, id: &str) {
    let _ = conn.execute(
        "UPDATE local_snapshots SET unavailable_at=COALESCE(unavailable_at, ?2) WHERE id=?1",
        rusqlite::params![id, snapshot_now()],
    );
}

fn create_snapshot_locked() -> Result<LocalSnapshot, rusqlite::Error> {
    ensure_snapshot_dir()?;
    let id = uuid::Uuid::new_v4().to_string();
    let filename = filename_for_id(&id);
    let pending_path = temporary_snapshot_path(&id);
    let final_path = snapshot_path(&filename)?;
    let created_at = snapshot_now();

    let result = (|| {
        db::with_db(|conn| conn.backup(DatabaseName::Main, &pending_path, None))?;
        let inspection = inspect_snapshot(&pending_path)?;
        fs::rename(&pending_path, &final_path)
            .map_err(|_| snapshot_error("snapshot publish failed"))?;
        let verified_at = snapshot_now();
        db::with_db(|conn| {
            conn.execute(
                "INSERT INTO local_snapshots
                 (id, filename, created_at, schema_version, byte_count, snippet_count, checksum, verified_at, unavailable_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL)",
                rusqlite::params![
                    id,
                    filename,
                    created_at,
                    inspection.schema_version,
                    inspection.byte_count as i64,
                    inspection.snippet_count as i64,
                    inspection.checksum,
                    verified_at,
                ],
            )?;
            Ok(())
        })?;
        Ok(LocalSnapshot {
            id,
            created_at,
            schema_version: inspection.schema_version,
            byte_count: inspection.byte_count,
            snippet_count: inspection.snippet_count,
            verified_at,
            unavailable_at: None,
        })
    })();

    if result.is_err() {
        let _ = fs::remove_file(&pending_path);
        let _ = fs::remove_file(&final_path);
    }
    result
}

fn prune_locked(retention: i32) -> Result<(), rusqlite::Error> {
    let retention =
        usize::try_from(retention).map_err(|_| snapshot_error("invalid snapshot retention"))?;
    db::with_db(|conn| {
        let mut statement = conn.prepare(
            "SELECT id, filename, created_at, schema_version, byte_count, snippet_count, checksum, verified_at, unavailable_at
             FROM local_snapshots
             WHERE unavailable_at IS NULL
             ORDER BY created_at DESC, id DESC",
        )?;
        let entries = statement
            .query_map([], decode_catalog_entry)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for entry in entries.into_iter().skip(retention.max(1)) {
            let path = snapshot_path(&entry.filename)?;
            if fs::remove_file(&path).is_ok() {
                conn.execute("DELETE FROM local_snapshots WHERE id=?1", [&entry.id])?;
            } else {
                mark_unavailable(conn, &entry.id);
            }
        }
        Ok(())
    })
}

pub fn create_snapshot() -> Result<LocalSnapshot, rusqlite::Error> {
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| snapshot_error("snapshot operation unavailable"))?;
    let snapshot = create_snapshot_locked()?;
    let retention = settings::get_settings().local_snapshot_retention;
    prune_locked(retention)?;
    Ok(snapshot)
}

pub fn list_snapshots() -> Result<Vec<LocalSnapshot>, rusqlite::Error> {
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| snapshot_error("snapshot operation unavailable"))?;
    db::with_db(|conn| {
        let mut statement = conn.prepare(
            "SELECT id, filename, created_at, schema_version, byte_count, snippet_count, checksum, verified_at, unavailable_at
             FROM local_snapshots
             ORDER BY created_at DESC, id DESC LIMIT ?1",
        )?;
        let entries = statement
            .query_map([MAX_SNAPSHOT_LIST as i64], decode_catalog_entry)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut result = Vec::with_capacity(entries.len());
        for entry in entries {
            let unavailable =
                entry.unavailable_at.is_some() || verify_catalog_entry(&entry).is_err();
            if unavailable && entry.unavailable_at.is_none() {
                mark_unavailable(conn, &entry.id);
            }
            let mut view = local_snapshot_from_catalog(entry);
            if unavailable && view.unavailable_at.is_none() {
                view.unavailable_at = Some(snapshot_now());
            }
            result.push(view);
        }
        Ok(result)
    })
}

pub fn get_status() -> Result<SnapshotStatus, rusqlite::Error> {
    let snapshots = list_snapshots()?;
    let settings = settings::get_settings();
    Ok(SnapshotStatus {
        latest_created_at: snapshots
            .iter()
            .find(|snapshot| snapshot.unavailable_at.is_none())
            .map(|snapshot| snapshot.created_at.clone()),
        snapshots,
        automatic_enabled: settings.local_snapshot_enabled,
        frequency: settings.local_snapshot_frequency,
        retention: settings.local_snapshot_retention,
    })
}

pub fn update_policy(input: SnapshotSettingsInput) -> Result<SnapshotStatus, String> {
    settings::update_settings(|settings| input.apply_to(settings))?;
    get_status().map_err(|_| "snapshot status unavailable".into())
}

fn list_catalog_entries(conn: &Connection) -> Result<Vec<CatalogEntry>, rusqlite::Error> {
    let mut statement = conn.prepare(
        "SELECT id, filename, created_at, schema_version, byte_count, snippet_count, checksum, verified_at, unavailable_at
         FROM local_snapshots
         ORDER BY created_at DESC, id DESC",
    )?;
    let entries = statement
        .query_map([], decode_catalog_entry)?
        .collect::<rusqlite::Result<Vec<_>>>();
    entries
}

fn reinsert_catalog_entries(
    conn: &mut Connection,
    entries: &[CatalogEntry],
) -> Result<(), rusqlite::Error> {
    let transaction = conn.transaction()?;
    for entry in entries {
        transaction.execute(
            "INSERT OR REPLACE INTO local_snapshots
             (id, filename, created_at, schema_version, byte_count, snippet_count, checksum, verified_at, unavailable_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                entry.id,
                entry.filename,
                entry.created_at,
                entry.schema_version,
                entry.byte_count as i64,
                entry.snippet_count as i64,
                entry.checksum,
                entry.verified_at,
                entry.unavailable_at,
            ],
        )?;
    }
    transaction.commit()
}

fn copy_snapshot_to_active_connection(
    source_path: &Path,
    source_schema_version: i64,
    expected_checksum: &str,
) -> Result<(), rusqlite::Error> {
    let (byte_count, checksum) = file_checksum(source_path)?;
    if byte_count == 0 || checksum != expected_checksum {
        return Err(snapshot_error("snapshot changed after verification"));
    }
    let source = Connection::open_with_flags(
        source_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    db::with_db_mut(|active| {
        let backup = Backup::new(&source, active)?;
        backup.run_to_completion(128, Duration::from_millis(5), None)?;
        drop(backup);
        if source_schema_version == PREVIOUS_SCHEMA_VERSION {
            db::migrate_connection_to_current(active)?;
        }
        validate_snapshot_connection(active)?;
        Ok(())
    })
}

pub fn restore_snapshot(id: &str) -> Result<RestoreResult, rusqlite::Error> {
    let _guard = SNAPSHOT_LOCK
        .lock()
        .map_err(|_| snapshot_error("snapshot operation unavailable"))?;
    let _sync_guard = crate::webdav::try_exclusive_operation_guard()
        .map_err(|_| snapshot_error("snapshot operation unavailable"))?;
    let _mutation_guard = restore_guard();
    let target = db::with_db(|conn| find_catalog_entry(conn, id))?;
    let catalog_before_restore = db::with_db(list_catalog_entries)?;
    let target_path = match verify_catalog_entry(&target) {
        Ok(path) => path,
        Err(error) => {
            db::with_db(|conn| {
                mark_unavailable(conn, &target.id);
                Ok(())
            })?;
            return Err(error);
        }
    };

    let emergency = create_snapshot_locked()?;
    let emergency_entry = db::with_db(|conn| find_catalog_entry(conn, &emergency.id))?;
    let emergency_path = verify_catalog_entry(&emergency_entry)?;
    let mut restored_catalog = catalog_before_restore;
    restored_catalog.push(emergency_entry.clone());

    let confirmation_was_required = settings::get_settings().sync_confirmation_required;
    crate::settings::require_manual_sync_confirmation()
        .map_err(|_| snapshot_error("snapshot restore state could not be saved"))?;

    let restore_result = (|| {
        copy_snapshot_to_active_connection(&target_path, target.schema_version, &target.checksum)?;
        db::with_db_mut(|conn| reinsert_catalog_entries(conn, &restored_catalog))
    })();
    if let Err(error) = restore_result {
        let rollback = copy_snapshot_to_active_connection(
            &emergency_path,
            emergency_entry.schema_version,
            &emergency_entry.checksum,
        );
        if !confirmation_was_required {
            if let Err(reset_error) = crate::settings::confirm_manual_sync() {
                log::error!("Could not restore the prior sync confirmation state: {reset_error}");
            }
        }
        if rollback.is_err() {
            return Err(snapshot_error("snapshot restore and recovery failed"));
        }
        if db::with_db_mut(|conn| {
            reinsert_catalog_entries(conn, std::slice::from_ref(&emergency_entry))
        })
        .is_err()
        {
            log::error!("Restoring emergency snapshot catalog entry failed");
        }
        return Err(error);
    }

    if prune_locked(settings::get_settings().local_snapshot_retention).is_err() {
        log::error!("Pruning retained local snapshots failed after restore");
    }

    if db::record_sync_notification(
        "settings",
        "result",
        "restore_required",
        None,
        false,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .is_err()
    {
        // The vault and durable manual-sync latch have already committed. Do not
        // misreport that successful restore as failed just because the optional
        // de-identified inbox record could not be stored.
        log::error!("Recording post-restore notification failed");
    }

    Ok(RestoreResult {
        restored_snapshot_id: target.id,
        emergency_snapshot_id: emergency.id,
    })
}

fn automatic_snapshot_due(latest: Option<&str>, frequency: &str) -> bool {
    let Some(latest) = latest else {
        return true;
    };
    let Ok(latest) = chrono::DateTime::parse_from_rfc3339(latest) else {
        return true;
    };
    let elapsed = chrono::Utc::now().signed_duration_since(latest.with_timezone(&chrono::Utc));
    let threshold = if frequency == "weekly" { 7 } else { 1 };
    elapsed.num_days() >= threshold
}

pub fn start_snapshot_worker() {
    std::thread::spawn(|| {
        let mut retry_after: Option<std::time::Instant> = None;
        loop {
            let policy = settings::get_settings();
            let now = std::time::Instant::now();
            let should_attempt = policy.local_snapshot_enabled
                && retry_after.is_none_or(|due| now >= due)
                && get_status()
                    .map(|status| {
                        automatic_snapshot_due(
                            status.latest_created_at.as_deref(),
                            &policy.local_snapshot_frequency,
                        )
                    })
                    .unwrap_or(false);
            if should_attempt {
                match create_snapshot() {
                    Ok(_) => retry_after = None,
                    Err(_) => retry_after = now.checked_add(SNAPSHOT_WORKER_FAILURE_BACKOFF),
                }
            }
            std::thread::sleep(SNAPSHOT_POLL_INTERVAL);
        }
    });
}
