use crate::db::{self, Snippet};
use crate::settings::{self, Settings};
use crate::webdav::{self, SyncResult};
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tauri::{command, AppHandle, Manager};

#[derive(serde::Serialize)]
pub struct ExportResult {
    pub file_path: String,
    pub folder_path: String,
    pub saved_in_downloads: bool,
}

pub static BOOT_START: OnceCell<Instant> = OnceCell::new();
pub static WINDOW_SHOWN: AtomicBool = AtomicBool::new(false);

pub fn boot_log(stage: &str, meta: &str) {
    let elapsed_ms = BOOT_START
        .get()
        .map(|t| t.elapsed().as_millis())
        .unwrap_or(0);
    log::info!(
        "BOOT|side=native|t_ms={}|stage={}|meta={}",
        elapsed_ms,
        stage,
        meta
    );
}

pub fn show_main_window_if_needed(app: &AppHandle, reason: &str) {
    if WINDOW_SHOWN.swap(true, Ordering::SeqCst) {
        return;
    }

    if let Some(window) = app.get_webview_window("main") {
        boot_log("window_show_requested", reason);
        if let Err(e) = window.show() {
            boot_log("window_show_error", &format!("reason={} err={}", reason, e));
            return;
        }
        let _ = window.unminimize();
        let _ = window.set_focus();
        boot_log("window_show_ok", reason);
    } else {
        boot_log("window_show_error", &format!("reason={} err=no_main_window", reason));
    }
}

#[command]
pub fn frontend_ready(app: AppHandle, phase: Option<String>) {
    let phase = phase.unwrap_or_else(|| "from_web".to_string());
    boot_log("frontend_ready_received", &phase);
    show_main_window_if_needed(&app, &format!("frontend_ready:{phase}"));
}

#[command]
pub fn boot_mark(stage: String, t_ms: f64, app: AppHandle) {
    boot_log("web_mark", &format!("stage={} web_t_ms={:.2}", stage, t_ms));

    if stage == "main_eval_start" {
        show_main_window_if_needed(&app, "boot_mark:main_eval_start");
    }
}

#[command]
pub fn get_snippets() -> Result<Vec<Snippet>, String> {
    db::get_all_snippets().map_err(|e| format!("Database error: {e}"))
}

#[command]
pub fn create_snippet(
    id: String,
    title: String,
    content: String,
    language: String,
    description: String,
    tags: Vec<String>,
    is_favorite: bool,
) -> Result<(), String> {
    let now = chrono::Utc::now().to_rfc3339();
    let snippet = Snippet {
        id,
        title,
        content,
        language,
        description,
        tags,
        is_favorite,
        created_at: now.clone(),
        updated_at: now,
    };
    db::create_snippet(&snippet).map_err(|e| format!("Create error: {e}"))
}

#[command]
pub fn update_snippet(
    id: String,
    title: String,
    content: String,
    language: String,
    description: String,
    tags: Vec<String>,
    is_favorite: bool,
    updated_at: String,
) -> Result<(), String> {
    let snippet = Snippet {
        id,
        title,
        content,
        language,
        description,
        tags,
        is_favorite,
        created_at: String::new(),
        updated_at,
    };
    db::update_snippet(&snippet).map_err(|e| format!("Update error: {e}"))
}

#[command]
pub fn delete_snippet(id: String) -> Result<(), String> {
    db::delete_snippet(&id).map_err(|e| format!("Delete error: {e}"))
}

#[command]
pub fn search_snippets(query: String, language: Option<String>, tag: Option<String>) -> Result<Vec<Snippet>, String> {
    db::search_snippets(&query, language.as_deref(), tag.as_deref())
        .map_err(|e| format!("Search error: {e}"))
}

#[command]
pub fn toggle_favorite(id: String) -> Result<bool, String> {
    db::toggle_favorite(&id).map_err(|e| format!("Toggle error: {e}"))
}

#[command]
pub fn export_snippets() -> Result<String, String> {
    db::export_snippets().map_err(|e| format!("Export error: {e}"))
}

#[command]
pub fn export_snippets_to_file() -> Result<ExportResult, String> {
    let json = db::export_snippets().map_err(|e| format!("Export error: {e}"))?;

    let (export_dir, saved_in_downloads) = crate::paths::get_export_dir();
    std::fs::create_dir_all(&export_dir)
        .map_err(|e| format!("创建导出目录失败 ({}): {e}", export_dir.display()))?;

    let filename = format!(
        "snipvault-backup-{}.json",
        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")
    );
    let target = export_dir.join(filename);

    std::fs::write(&target, json)
        .map_err(|e| format!("写入导出文件失败 ({}): {e}", target.display()))?;

    Ok(ExportResult {
        file_path: target.display().to_string(),
        folder_path: export_dir.display().to_string(),
        saved_in_downloads,
    })
}

#[command]
pub fn import_snippets(json_data: String) -> Result<usize, String> {
    db::import_snippets(&json_data).map_err(|e| format!("Import error: {e}"))
}

// --- Settings ---

#[command]
pub fn get_settings() -> Result<Settings, String> {
    Ok(settings::get_settings())
}

#[command]
pub fn save_settings(new_settings: Settings, app: AppHandle) -> Result<(), String> {
    let current = settings::get_settings();
    settings::update_settings(|s| {
        *s = new_settings.clone();
        // Preserve last_sync_at — it's not sent from the frontend
        // and must not be cleared when saving other settings
        s.last_sync_at = current.last_sync_at.clone();
    })?;

    // Sync autostart registration whenever auto_start setting changes
    if new_settings.auto_start != current.auto_start {
        use tauri_plugin_autostart::ManagerExt;
        let autostart = app.autolaunch();
        if new_settings.auto_start {
            autostart.enable().map_err(|e| format!("开启开机自启失败: {e}"))?;
        } else {
            autostart.disable().map_err(|e| format!("关闭开机自启失败: {e}"))?;
        }
    }

    Ok(())
}

// --- Auto-start ---

#[command]
pub fn set_auto_start(enabled: bool, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable().map_err(|e| format!("开启失败: {e}"))?;
    } else {
        autostart.disable().map_err(|e| format!("关闭失败: {e}"))?;
    }
    settings::update_settings(|s| s.auto_start = enabled)
}

#[command]
pub fn is_auto_start_enabled(app: AppHandle) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

// --- WebDAV Sync ---

#[command]
pub fn sync_upload() -> Result<SyncResult, String> {
    webdav::sync_to_webdav()
}

#[command]
pub fn sync_download() -> Result<SyncResult, String> {
    webdav::sync_from_webdav()
}

#[command]
pub fn get_sync_versions() -> Result<Vec<db::SyncVersion>, String> {
    db::get_sync_versions().map_err(|e| format!("读取同步历史失败: {e}"))
}

#[command]
pub fn get_system_theme(app: AppHandle) -> Result<String, String> {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(theme) = window.theme() {
            return Ok(match theme {
                tauri::Theme::Light => "light".into(),
                tauri::Theme::Dark => "dark".into(),
                _ => "dark".into(),
            });
        }
    }
    Ok("dark".into())
}

#[command]
pub fn get_system_locale() -> String {
    if let Some(locale) = sys_locale::get_locale() {
        let locale = locale.to_lowercase();
        if locale.starts_with("zh") {
            return "zh".to_string();
        }
        if locale.starts_with("en") {
            return "en".to_string();
        }
    }

    if let Ok(lang) = std::env::var("LANG") {
        let lang = lang.to_lowercase();
        if lang.starts_with("zh") {
            return "zh".to_string();
        }
        if lang.starts_with("en") {
            return "en".to_string();
        }
    }

    "en".to_string()
}

