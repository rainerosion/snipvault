use crate::db::{self, Snippet};
use crate::settings::{self, Settings};
use crate::webdav::{self, SyncResult};
use tauri::{command, AppHandle, Manager};

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
    // Detect Windows system language via environment variable
    #[cfg(windows)]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("powershell")
            .args(["-NoProfile", "-Command", "(Get-Culture).TwoLetterISOLanguageName"])
            .output()
        {
            let lang = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
            if lang == "zh" {
                return "zh".to_string();
            } else if lang == "en" {
                return "en".to_string();
            }
        }
        // Fallback: check LANG environment variable
        if let Ok(lang) = std::env::var("LANG") {
            if lang.starts_with("zh") {
                return "zh".to_string();
            }
        }
        // Fallback: check LC_ALL
        if let Ok(lang) = std::env::var("LC_ALL") {
            if lang.starts_with("zh") {
                return "zh".to_string();
            }
        }
        "en".to_string()
    }
    #[cfg(not(windows))]
    {
        if let Ok(lang) = std::env::var("LANG") {
            if lang.starts_with("zh") {
                return "zh".to_string();
            }
        }
        "en".to_string()
    }
}

