use crate::paths::get_settings_path;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

static SETTINGS: OnceCell<Mutex<Settings>> = OnceCell::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub theme: String,
    pub language: String,
    pub webdav_url: String,
    pub webdav_username: String,
    pub webdav_password: String,
    pub webdav_timeout_secs: u64,
    pub auto_sync: bool,
    pub sync_interval_minutes: i32,
    pub last_sync_at: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_start: false,
            minimize_to_tray: true,
            theme: "system".into(),
            language: "zh".into(),
            webdav_url: String::new(),
            webdav_username: String::new(),
            webdav_password: String::new(),
            webdav_timeout_secs: 30,
            auto_sync: false,
            sync_interval_minutes: 30,
            last_sync_at: String::new(),
        }
    }
}

pub fn init_settings() {
    if SETTINGS.get().is_some() {
        return;
    }

    let path = get_settings_path();
    log::info!("init_settings: reading settings from {:?}", path);
    std::fs::create_dir_all(path.parent().unwrap()).ok();
    let settings = if path.exists() {
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default())
            .unwrap_or_default()
    } else {
        Settings::default()
    };
    SETTINGS.set(Mutex::new(settings)).ok();
    log::info!("Settings initialized");
}

pub fn with_settings<F, T>(f: F) -> T
where
    F: FnOnce(&Settings) -> T,
{
    init_settings();
    let settings = SETTINGS.get().expect("Settings not initialized");
    let guard = settings.lock().unwrap();
    f(&guard)
}

pub fn update_settings<F>(f: F) -> Result<(), String>
where
    F: FnOnce(&mut Settings),
{
    init_settings();

    let path = get_settings_path();
    log::info!("update_settings: target path = {:?}", path);

    // Validate path is absolute and non-empty
    if path.components().next().is_none() || !path.is_absolute() {
        log::error!("update_settings: invalid path (not absolute): {:?}", path);
        return Err("配置路径无效，请重启应用".into());
    }

    // Ensure the directory exists before writing
    if let Some(parent) = path.parent() {
        log::info!("update_settings: ensuring dir exists: {:?}", parent);
        std::fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败 ({}): {}", parent.display(), e))?;
        // Verify dir now exists
        if !parent.is_dir() {
            log::error!("update_settings: directory still not created: {:?}", parent);
            return Err(format!("无法创建配置目录: {}", parent.display()));
        }
    }

    let settings = SETTINGS.get().expect("Settings not initialized");
    let mut guard = settings.lock().map_err(|e| format!("配置锁失败: {}", e))?;
    f(&mut guard);
    let json = serde_json::to_string_pretty(&*guard).map_err(|e| format!("序列化配置失败: {}", e))?;
    log::info!("update_settings: writing {} bytes to {:?}", json.len(), path);
    std::fs::write(&path, &json).map_err(|e| format!("写入配置文件失败 ({}): {}", path.display(), e))?;
    log::info!("Settings saved to {}", path.display());
    Ok(())
}

pub fn get_settings() -> Settings {
    init_settings();
    let path = get_settings_path();
    log::info!("get_settings: reading from {:?}", path);
    SETTINGS.get().map(|m| m.lock().unwrap().clone()).unwrap_or_default()
}
