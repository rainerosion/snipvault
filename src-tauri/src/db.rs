use crate::paths::get_db_path;
use once_cell::sync::OnceCell;
use rusqlite::{Connection, Result as SqliteResult};
use std::sync::Mutex;

static DB: OnceCell<Mutex<Connection>> = OnceCell::new();

pub fn init_db() -> SqliteResult<()> {
    let db_path = get_db_path();
    log::info!("Initializing database at: {:?}", db_path);
    std::fs::create_dir_all(db_path.parent().unwrap()).ok();
    let conn = Connection::open(&db_path)?;

    conn.execute_batch(
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

    // Insert sample data if empty
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))?;
    if count == 0 {
        insert_samples(&conn)?;
    }

    DB.set(Mutex::new(conn)).map_err(|_| {
        rusqlite::Error::InvalidParameterName("DB already initialized".into())
    })?;
    log::info!("Database initialized successfully");
    Ok(())
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
    let db = DB.get().expect("DB not initialized");
    let conn = db.lock().unwrap();
    f(&conn)
}

// --- CRUD ---

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
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
}

impl From<&rusqlite::Row<'_>> for Snippet {
    fn from(row: &rusqlite::Row<'_>) -> Snippet {
        let tags_str: String = row.get(5).unwrap_or_default();
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        Snippet {
            id: row.get(0).unwrap_or_default(),
            title: row.get(1).unwrap_or_default(),
            content: row.get(2).unwrap_or_default(),
            language: row.get(3).unwrap_or_default(),
            description: row.get(4).unwrap_or_default(),
            tags,
            is_favorite: row.get::<_, i64>(6).unwrap_or(0) != 0,
            created_at: row.get(7).unwrap_or_default(),
            updated_at: row.get(8).unwrap_or_default(),
        }
    }
}

pub fn get_all_snippets() -> SqliteResult<Vec<Snippet>> {
    with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, content, language, description, tags, is_favorite, created_at, updated_at FROM snippets ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |row| Ok(Snippet::from(row)))?;
        rows.collect()
    })
}

pub fn create_snippet(s: &Snippet) -> SqliteResult<()> {
    with_db(|conn| {
        let tags_json = serde_json::to_string(&s.tags).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO snippets (id, title, content, language, description, tags, is_favorite, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![s.id, s.title, s.content, s.language, s.description, tags_json, s.is_favorite as i64, s.created_at, s.updated_at],
        )?;
        Ok(())
    })
}

pub fn update_snippet(s: &Snippet) -> SqliteResult<()> {
    with_db(|conn| {
        let tags_json = serde_json::to_string(&s.tags).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE snippets SET title=?2, content=?3, language=?4, description=?5, tags=?6, is_favorite=?7, updated_at=?8 WHERE id=?1",
            rusqlite::params![s.id, s.title, s.content, s.language, s.description, tags_json, s.is_favorite as i64, s.updated_at],
        )?;
        Ok(())
    })
}

pub fn delete_snippet(id: &str) -> SqliteResult<()> {
    with_db(|conn| {
        conn.execute("DELETE FROM snippets WHERE id=?1", rusqlite::params![id])?;
        Ok(())
    })
}

pub fn search_snippets(query: &str, language_filter: Option<&str>, tag_filter: Option<&str>) -> SqliteResult<Vec<Snippet>> {
    with_db(|conn| {
        let like_pattern = format!("%{}%", query.to_lowercase());
        let sql = if tag_filter.is_some() {
            "SELECT DISTINCT s.id, s.title, s.content, s.language, s.description, s.tags, s.is_favorite, s.created_at, s.updated_at FROM snippets s WHERE (LOWER(s.title) LIKE ?1 OR LOWER(s.content) LIKE ?1 OR LOWER(s.description) LIKE ?1) AND (?2 IS NULL OR s.language = ?2) ORDER BY s.is_favorite DESC, s.updated_at DESC"
        } else {
            "SELECT id, title, content, language, description, tags, is_favorite, created_at, updated_at FROM snippets WHERE (LOWER(title) LIKE ?1 OR LOWER(content) LIKE ?1 OR LOWER(description) LIKE ?1) AND (?2 IS NULL OR language = ?2) ORDER BY is_favorite DESC, updated_at DESC"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(rusqlite::params![like_pattern, language_filter], |row| Ok(Snippet::from(row)))?;
        rows.collect()
    })
}

pub fn toggle_favorite(id: &str) -> SqliteResult<bool> {
    with_db(|conn| {
        conn.execute("UPDATE snippets SET is_favorite = NOT is_favorite, updated_at=?2 WHERE id=?1", rusqlite::params![id, chrono::Utc::now().to_rfc3339()])?;
        let fav: i64 = conn.query_row("SELECT is_favorite FROM snippets WHERE id=?1", rusqlite::params![id], |r| r.get(0))?;
        Ok(fav != 0)
    })
}

pub fn export_snippets() -> SqliteResult<String> {
    let snippets = get_all_snippets()?;
    serde_json::to_string_pretty(&snippets).map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))
}

pub fn import_snippets(json_data: &str) -> SqliteResult<usize> {
    let snippets: Vec<Snippet> = serde_json::from_str(json_data).map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
    let mut count = 0;
    for s in snippets {
        if create_snippet(&s).is_ok() { count += 1; }
    }
    Ok(count)
}

// --- Merge sync ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct MergeResult {
    pub uploaded: usize,
    pub downloaded: usize,
    pub total: usize,
    pub message: String,
}

/// Merges remote snippets into local: insert remote if not exists, or replace
/// local if remote's updated_at is newer. Returns counts.
pub fn merge_snippets(remote_snippets: Vec<Snippet>) -> SqliteResult<MergeResult> {
    with_db(|conn| {
        let mut uploaded = 0;
        let mut downloaded = 0;

        for remote in remote_snippets {
            let existing: Option<(String, String)> = conn
                .query_row(
                    "SELECT id, updated_at FROM snippets WHERE id=?1",
                    rusqlite::params![remote.id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .ok();

            let should_update = match &existing {
                None => true, // remote is new
                Some((_, local_updated)) => remote.updated_at > *local_updated, // remote is newer
            };

            if should_update {
                let tags_json = serde_json::to_string(&remote.tags).unwrap_or_else(|_| "[]".into());
                conn.execute(
                    "INSERT OR REPLACE INTO snippets (id, title, content, language, description, tags, is_favorite, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        remote.id, remote.title, remote.content, remote.language,
                        remote.description, tags_json, remote.is_favorite as i64,
                        remote.created_at, remote.updated_at
                    ],
                )?;
                if existing.is_some() {
                    uploaded += 1;
                } else {
                    downloaded += 1;
                }
            }
        }

        let total: i64 = conn.query_row("SELECT COUNT(*) FROM snippets", [], |row| row.get(0))?;
        Ok(MergeResult {
            uploaded,
            downloaded,
            total: total as usize,
            message: format!("上传 {} 条，更新 {} 条，共 {} 条", uploaded, downloaded, total),
        })
    })
}

/// Returns all current snippets for uploading to remote
pub fn get_all_for_upload() -> SqliteResult<Vec<Snippet>> {
    get_all_snippets()
}

// --- Sync version history ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct SyncVersion {
    pub id: String,
    pub synced_at: String,
    pub direction: String,
    pub snippet_count: i64,
    pub uploaded_count: i64,
    pub downloaded_count: i64,
    pub message: String,
}

impl From<&rusqlite::Row<'_>> for SyncVersion {
    fn from(row: &rusqlite::Row<'_>) -> SyncVersion {
        SyncVersion {
            id: row.get(0).unwrap_or_default(),
            synced_at: row.get(1).unwrap_or_default(),
            direction: row.get(2).unwrap_or_default(),
            snippet_count: row.get(3).unwrap_or(0),
            uploaded_count: row.get(4).unwrap_or(0),
            downloaded_count: row.get(5).unwrap_or(0),
            message: row.get(6).unwrap_or_default(),
        }
    }
}

pub fn record_sync_version(
    direction: &str,
    snippet_count: usize,
    uploaded_count: usize,
    downloaded_count: usize,
    message: &str,
) -> SqliteResult<()> {
    with_db(|conn| {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sync_versions (id, synced_at, direction, snippet_count, uploaded_count, downloaded_count, message) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, now, direction, snippet_count as i64, uploaded_count as i64, downloaded_count as i64, message],
        )?;
        // Keep only last 20 versions
        conn.execute(
            "DELETE FROM sync_versions WHERE id NOT IN (SELECT id FROM sync_versions ORDER BY synced_at DESC LIMIT 20)",
            [],
        )?;
        Ok(())
    })
}

pub fn get_sync_versions() -> SqliteResult<Vec<SyncVersion>> {
    with_db(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, synced_at, direction, snippet_count, uploaded_count, downloaded_count, message FROM sync_versions ORDER BY synced_at DESC LIMIT 20",
        )?;
        let rows = stmt.query_map([], |row| Ok(SyncVersion::from(row)))?;
        rows.collect()
    })
}
