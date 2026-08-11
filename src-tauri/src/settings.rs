use crate::credentials::{self, CredentialFailure, CredentialStore};
use crate::paths::get_settings_path;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

static SETTINGS: OnceCell<Mutex<SettingsState>> = OnceCell::new();
static CREDENTIAL_STORE: OnceCell<Arc<dyn CredentialStore>> = OnceCell::new();

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CredentialRecoveryStatus {
    #[default]
    None,
    LegacyMigrationRequired,
    CompensationRequired,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SettingsRecoveryStatus {
    #[default]
    None,
    BackupRestored,
    DefaultsLoaded,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub theme: String,
    pub accent_preset: String,
    pub language: String,
    pub webdav_url: String,
    pub webdav_username: String,
    pub webdav_auth_mode: String,
    pub webdav_timeout_secs: u64,
    pub auto_sync: bool,
    pub sync_interval_minutes: i32,
    pub editor_line_wrap: bool,
    pub last_sync_at: String,
    pub credential_revision: u64,
    pub credential_recovery_status: CredentialRecoveryStatus,
    pub settings_recovery_status: SettingsRecoveryStatus,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_start: false,
            minimize_to_tray: true,
            theme: "system".into(),
            accent_preset: "sky".into(),
            language: "zh".into(),
            webdav_url: String::new(),
            webdav_username: String::new(),
            webdav_auth_mode: "auto".into(),
            webdav_timeout_secs: 30,
            auto_sync: false,
            sync_interval_minutes: 30,
            editor_line_wrap: true,
            last_sync_at: String::new(),
            credential_revision: 0,
            credential_recovery_status: CredentialRecoveryStatus::None,
            settings_recovery_status: SettingsRecoveryStatus::None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct LegacySettings {
    auto_start: bool,
    minimize_to_tray: bool,
    theme: String,
    accent_preset: String,
    language: String,
    webdav_url: String,
    webdav_username: String,
    webdav_password: Option<String>,
    webdav_auth_mode: String,
    webdav_timeout_secs: u64,
    auto_sync: bool,
    sync_interval_minutes: i32,
    editor_line_wrap: bool,
    last_sync_at: String,
    credential_revision: u64,
    credential_recovery_status: CredentialRecoveryStatus,
    settings_recovery_status: SettingsRecoveryStatus,
}

impl Default for LegacySettings {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl LegacySettings {
    fn with_defaults() -> Self {
        let defaults = Settings::default();
        Self {
            auto_start: defaults.auto_start,
            minimize_to_tray: defaults.minimize_to_tray,
            theme: defaults.theme,
            accent_preset: defaults.accent_preset,
            language: defaults.language,
            webdav_url: defaults.webdav_url,
            webdav_username: defaults.webdav_username,
            webdav_password: None,
            webdav_auth_mode: defaults.webdav_auth_mode,
            webdav_timeout_secs: defaults.webdav_timeout_secs,
            auto_sync: defaults.auto_sync,
            sync_interval_minutes: defaults.sync_interval_minutes,
            editor_line_wrap: defaults.editor_line_wrap,
            last_sync_at: defaults.last_sync_at,
            credential_revision: defaults.credential_revision,
            credential_recovery_status: defaults.credential_recovery_status,
            settings_recovery_status: defaults.settings_recovery_status,
        }
    }

    fn into_parts(self) -> (Settings, Option<String>, bool) {
        let had_legacy_field = self.webdav_password.is_some();
        let legacy_secret = self.webdav_password.filter(|secret| !secret.is_empty());
        (
            Settings {
                auto_start: self.auto_start,
                minimize_to_tray: self.minimize_to_tray,
                theme: self.theme,
                accent_preset: self.accent_preset,
                language: self.language,
                webdav_url: self.webdav_url,
                webdav_username: self.webdav_username,
                webdav_auth_mode: self.webdav_auth_mode,
                webdav_timeout_secs: self.webdav_timeout_secs,
                auto_sync: self.auto_sync,
                sync_interval_minutes: self.sync_interval_minutes,
                editor_line_wrap: self.editor_line_wrap,
                last_sync_at: self.last_sync_at,
                credential_revision: self.credential_revision,
                credential_recovery_status: self.credential_recovery_status,
                settings_recovery_status: self.settings_recovery_status,
            },
            legacy_secret,
            had_legacy_field,
        )
    }
}

#[derive(Clone)]
struct SettingsState {
    settings: Settings,
    runtime_credential_recovery: Option<CredentialRecoveryStatus>,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatusKind {
    Configured,
    NotConfigured,
    Unavailable,
    Denied,
    Invalid,
    Ambiguous,
    MigrationRequired,
    RecoveryRequired,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
pub struct CredentialStatus {
    pub kind: CredentialStatusKind,
    pub action_required: bool,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct SettingsView {
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub theme: String,
    pub accent_preset: String,
    pub language: String,
    pub webdav_url: String,
    pub webdav_username: String,
    pub webdav_auth_mode: String,
    pub webdav_timeout_secs: u64,
    pub auto_sync: bool,
    pub sync_interval_minutes: i32,
    pub editor_line_wrap: bool,
    pub last_sync_at: String,
    pub webdav_secret_configured: bool,
    pub credential_status: CredentialStatus,
    pub settings_recovery_status: SettingsRecoveryStatus,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct SettingsInput {
    pub auto_start: bool,
    pub minimize_to_tray: bool,
    pub theme: String,
    pub accent_preset: String,
    pub language: String,
    pub webdav_url: String,
    pub webdav_username: String,
    pub webdav_auth_mode: String,
    pub webdav_timeout_secs: u64,
    pub auto_sync: bool,
    pub sync_interval_minutes: i32,
    pub editor_line_wrap: bool,
}

impl SettingsInput {
    pub fn apply_to(&self, current: &Settings) -> Settings {
        Settings {
            auto_start: self.auto_start,
            minimize_to_tray: self.minimize_to_tray,
            theme: self.theme.clone(),
            accent_preset: self.accent_preset.clone(),
            language: self.language.clone(),
            webdav_url: self.webdav_url.clone(),
            webdav_username: self.webdav_username.clone(),
            webdav_auth_mode: self.webdav_auth_mode.clone(),
            webdav_timeout_secs: self.webdav_timeout_secs,
            auto_sync: self.auto_sync,
            sync_interval_minutes: self.sync_interval_minutes,
            editor_line_wrap: self.editor_line_wrap,
            last_sync_at: current.last_sync_at.clone(),
            credential_revision: current.credential_revision,
            credential_recovery_status: current.credential_recovery_status,
            settings_recovery_status: current.settings_recovery_status,
        }
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(tag = "action", content = "value", rename_all = "snake_case")]
pub enum SecretAction {
    Keep,
    Replace(String),
    Clear,
}

pub fn validate_settings(settings: &Settings) -> Result<(), String> {
    if !matches!(settings.theme.as_str(), "system" | "dark" | "light") {
        return Err("invalid theme setting".into());
    }
    if !matches!(
        settings.accent_preset.as_str(),
        "sky" | "violet" | "emerald" | "amber" | "rose" | "white"
    ) {
        return Err("invalid accent preset".into());
    }
    if !matches!(settings.language.as_str(), "zh" | "en") {
        return Err("invalid language setting".into());
    }
    if !matches!(
        settings.webdav_auth_mode.as_str(),
        "auto" | "basic" | "digest" | "bearer" | "none"
    ) {
        return Err("invalid WebDAV authentication mode".into());
    }
    if !(5..=300).contains(&settings.webdav_timeout_secs) {
        return Err("invalid WebDAV timeout".into());
    }
    if !(0..=24 * 60).contains(&settings.sync_interval_minutes)
        || (settings.auto_sync && settings.sync_interval_minutes == 0)
    {
        return Err("invalid sync interval".into());
    }
    if settings.webdav_url.len() > 4096 || settings.webdav_username.len() > 1024 {
        return Err("WebDAV setting is too long".into());
    }
    if !settings.webdav_url.trim().is_empty() {
        crate::webdav::validate_base_url(&settings.webdav_url)?;
    }
    if !settings.last_sync_at.is_empty()
        && chrono::DateTime::parse_from_rfc3339(&settings.last_sync_at).is_err()
    {
        return Err("invalid last synchronization time".into());
    }
    Ok(())
}

pub fn validate_secret_action(action: &SecretAction) -> Result<(), String> {
    if let SecretAction::Replace(secret) = action {
        if secret.is_empty() || secret.len() > 8192 {
            return Err("replacement credential must contain 1 to 8192 bytes".into());
        }
    }
    Ok(())
}

pub fn configure_credential_store(store: Arc<dyn CredentialStore>) {
    let _ = CREDENTIAL_STORE.set(store);
}

pub fn credential_store() -> Arc<dyn CredentialStore> {
    CREDENTIAL_STORE
        .get_or_init(credentials::platform_store)
        .clone()
}

pub fn init_settings() {
    if SETTINGS.get().is_some() {
        return;
    }

    let path = get_settings_path();
    let store = credential_store();
    let state = match load_settings_from_path(&path, store.as_ref()) {
        Ok(state) => state,
        Err(error) => {
            log::error!("Settings initialization failed safely: {error}");
            SettingsState {
                settings: Settings::default(),
                runtime_credential_recovery: None,
            }
        }
    };
    let _ = SETTINGS.set(Mutex::new(state));
    log::info!("Settings initialized");
}

fn backup_path(path: &Path) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.json");
    path.with_file_name(format!("{filename}.bak"))
}

fn unique_sibling_path(path: &Path, label: &str) -> PathBuf {
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("settings.json");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    for counter in 0_u32.. {
        let candidate = path.with_file_name(format!(
            ".{filename}.{}.{stamp}.{counter}.{label}",
            std::process::id()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u32 path suffixes exhausted")
}

fn read_legacy_settings(path: &Path) -> Result<(Settings, Option<String>, bool), String> {
    let text = std::fs::read_to_string(path).map_err(|_| "settings file could not be read")?;
    let legacy: LegacySettings =
        serde_json::from_str(&text).map_err(|_| "settings file contains invalid JSON")?;
    let parts = legacy.into_parts();
    validate_settings(&parts.0)?;
    Ok(parts)
}

fn restore_credential(
    store: &dyn CredentialStore,
    previous: &Option<String>,
) -> Result<(), CredentialFailure> {
    match previous {
        Some(secret) => store.write_secret(secret),
        None => store.clear_secret(),
    }
}

fn migrate_legacy_secret(
    path: &Path,
    mut settings: Settings,
    secret: Option<String>,
    store: &dyn CredentialStore,
) -> SettingsState {
    let previous = match store.read_secret() {
        Ok(previous) => previous,
        Err(failure) => {
            log::warn!("Legacy credential migration is blocked: {failure}");
            settings.credential_recovery_status = CredentialRecoveryStatus::LegacyMigrationRequired;
            return SettingsState {
                settings,
                runtime_credential_recovery: Some(
                    CredentialRecoveryStatus::LegacyMigrationRequired,
                ),
            };
        }
    };

    let migration_result = match secret.as_deref() {
        Some(secret) => store.write_secret(secret),
        None => Ok(()),
    };
    if let Err(failure) = migration_result {
        log::warn!("Legacy credential migration is blocked: {failure}");
        settings.credential_recovery_status = CredentialRecoveryStatus::LegacyMigrationRequired;
        return SettingsState {
            settings,
            runtime_credential_recovery: Some(CredentialRecoveryStatus::LegacyMigrationRequired),
        };
    }

    settings.credential_revision = settings.credential_revision.saturating_add(1);
    settings.credential_recovery_status = CredentialRecoveryStatus::None;
    if let Err(error) = write_settings_to_path(path, &settings) {
        log::error!("Sanitizing migrated settings failed: {error}");
        let rollback_failed = restore_credential(store, &previous).is_err();
        settings.credential_recovery_status = if rollback_failed {
            CredentialRecoveryStatus::CompensationRequired
        } else {
            CredentialRecoveryStatus::LegacyMigrationRequired
        };
        let recovery_status = settings.credential_recovery_status;
        return SettingsState {
            settings,
            runtime_credential_recovery: Some(recovery_status),
        };
    }

    SettingsState {
        settings,
        runtime_credential_recovery: None,
    }
}

fn load_settings_from_path(
    path: &Path,
    store: &dyn CredentialStore,
) -> Result<SettingsState, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "settings directory could not be created")?;
    }
    let backup = backup_path(path);

    if !path.exists() && backup.exists() {
        std::fs::rename(&backup, path).map_err(|_| "settings backup could not be restored")?;
    }

    if !path.exists() {
        return Ok(SettingsState {
            settings: Settings::default(),
            runtime_credential_recovery: None,
        });
    }

    match read_legacy_settings(path) {
        Ok((settings, secret, had_legacy_field)) => {
            if had_legacy_field {
                Ok(migrate_legacy_secret(path, settings, secret, store))
            } else {
                if backup.exists() {
                    let _ = std::fs::remove_file(backup);
                }
                Ok(SettingsState {
                    settings,
                    runtime_credential_recovery: None,
                })
            }
        }
        Err(primary_error) => {
            let quarantine = unique_sibling_path(path, "corrupt");
            std::fs::rename(path, &quarantine)
                .map_err(|_| "damaged settings could not be quarantined")?;
            log::warn!("Damaged settings were quarantined: {primary_error}");

            if backup.exists() {
                if let Ok((mut settings, secret, had_legacy_field)) = read_legacy_settings(&backup)
                {
                    settings.settings_recovery_status = SettingsRecoveryStatus::BackupRestored;
                    let state = if had_legacy_field {
                        migrate_legacy_secret(path, settings, secret, store)
                    } else {
                        write_settings_to_path(path, &settings)?;
                        SettingsState {
                            settings,
                            runtime_credential_recovery: None,
                        }
                    };
                    return Ok(state);
                }
            }

            let settings = Settings {
                settings_recovery_status: SettingsRecoveryStatus::DefaultsLoaded,
                ..Settings::default()
            };
            write_settings_to_path(path, &settings)?;
            Ok(SettingsState {
                settings,
                runtime_credential_recovery: None,
            })
        }
    }
}

fn write_settings_to_path(path: &Path, settings: &Settings) -> Result<(), String> {
    validate_settings(settings)?;
    if path.components().next().is_none() || !path.is_absolute() {
        return Err("settings path is invalid".into());
    }

    let parent = path
        .parent()
        .ok_or_else(|| "settings directory is invalid".to_string())?;
    std::fs::create_dir_all(parent).map_err(|_| "settings directory could not be created")?;

    let json = serde_json::to_vec_pretty(settings)
        .map_err(|_| "settings could not be serialized".to_string())?;
    let temporary = unique_sibling_path(path, "tmp");
    let backup = backup_path(path);

    let write_result = (|| -> Result<(), String> {
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|_| "temporary settings file could not be created")?;
        file.write_all(&json)
            .map_err(|_| "temporary settings file could not be written")?;
        file.sync_all()
            .map_err(|_| "temporary settings file could not be synchronized")?;
        drop(file);

        if path.exists() {
            if backup.exists() {
                std::fs::remove_file(&backup)
                    .map_err(|_| "stale settings backup could not be removed")?;
            }
            std::fs::rename(path, &backup)
                .map_err(|_| "current settings could not be backed up")?;
        }

        if let Err(_error) = std::fs::rename(&temporary, path) {
            if backup.exists() && !path.exists() {
                let _ = std::fs::rename(&backup, path);
            }
            return Err("settings file could not be replaced".into());
        }

        if backup.exists() {
            if let Err(error) = std::fs::remove_file(&backup) {
                log::warn!("Replaced settings backup could not be removed: {error}");
            }
        }

        #[cfg(unix)]
        if let Ok(directory) = std::fs::File::open(parent) {
            let _ = directory.sync_all();
        }

        Ok(())
    })();

    if temporary.exists() {
        let _ = std::fs::remove_file(&temporary);
    }
    write_result
}

pub fn with_settings<F, T>(f: F) -> T
where
    F: FnOnce(&Settings) -> T,
{
    init_settings();
    let state = SETTINGS.get().expect("Settings not initialized");
    let guard = state.lock().unwrap_or_else(|error| error.into_inner());
    f(&guard.settings)
}

pub fn update_settings<F>(f: F) -> Result<Settings, String>
where
    F: FnOnce(&mut Settings),
{
    init_settings();
    let path = get_settings_path();
    let state = SETTINGS.get().expect("Settings not initialized");
    let mut guard = state
        .lock()
        .map_err(|_| "settings lock could not be acquired".to_string())?;

    if effective_recovery(&guard) != CredentialRecoveryStatus::None {
        return Err("credential recovery action is required".into());
    }

    let mut candidate = guard.settings.clone();
    f(&mut candidate);
    validate_settings(&candidate)?;
    write_settings_to_path(&path, &candidate)?;
    guard.settings = candidate.clone();
    log::info!("Settings saved");
    Ok(candidate)
}

pub fn replace_settings(candidate: Settings) -> Result<Settings, String> {
    init_settings();
    validate_settings(&candidate)?;
    let path = get_settings_path();
    write_settings_to_path(&path, &candidate)?;
    let state = SETTINGS.get().expect("Settings not initialized");
    let mut guard = state
        .lock()
        .map_err(|_| "settings lock could not be acquired".to_string())?;
    guard.settings = candidate.clone();
    guard.runtime_credential_recovery = None;
    log::info!("Settings saved");
    Ok(candidate)
}

pub fn mark_credential_recovery_required() {
    init_settings();
    if let Some(state) = SETTINGS.get() {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        guard.runtime_credential_recovery = Some(CredentialRecoveryStatus::CompensationRequired);
        guard.settings.credential_recovery_status = CredentialRecoveryStatus::CompensationRequired;
    }
}

pub fn get_settings() -> Settings {
    init_settings();
    SETTINGS
        .get()
        .map(|state| {
            state
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .settings
                .clone()
        })
        .unwrap_or_default()
}

fn effective_recovery(state: &SettingsState) -> CredentialRecoveryStatus {
    state
        .runtime_credential_recovery
        .unwrap_or(state.settings.credential_recovery_status)
}

fn credential_status_for(
    settings: &Settings,
    recovery: CredentialRecoveryStatus,
    store: &dyn CredentialStore,
) -> CredentialStatus {
    match recovery {
        CredentialRecoveryStatus::LegacyMigrationRequired => CredentialStatus {
            kind: CredentialStatusKind::MigrationRequired,
            action_required: true,
        },
        CredentialRecoveryStatus::CompensationRequired => CredentialStatus {
            kind: CredentialStatusKind::RecoveryRequired,
            action_required: true,
        },
        CredentialRecoveryStatus::None => match store.read_secret() {
            Ok(Some(_)) => CredentialStatus {
                kind: CredentialStatusKind::Configured,
                action_required: false,
            },
            Ok(None) => CredentialStatus {
                kind: CredentialStatusKind::NotConfigured,
                action_required: settings.auto_sync
                    && !matches!(settings.webdav_auth_mode.as_str(), "none"),
            },
            Err(CredentialFailure::Unavailable) => CredentialStatus {
                kind: CredentialStatusKind::Unavailable,
                action_required: true,
            },
            Err(CredentialFailure::Denied) => CredentialStatus {
                kind: CredentialStatusKind::Denied,
                action_required: true,
            },
            Err(CredentialFailure::Invalid) => CredentialStatus {
                kind: CredentialStatusKind::Invalid,
                action_required: true,
            },
            Err(CredentialFailure::Ambiguous) => CredentialStatus {
                kind: CredentialStatusKind::Ambiguous,
                action_required: true,
            },
        },
    }
}

pub fn get_settings_view() -> SettingsView {
    init_settings();
    let state = SETTINGS.get().expect("Settings not initialized");
    let guard = state.lock().unwrap_or_else(|error| error.into_inner());
    let recovery = effective_recovery(&guard);
    let status = credential_status_for(&guard.settings, recovery, credential_store().as_ref());
    let settings = &guard.settings;
    SettingsView {
        auto_start: settings.auto_start,
        minimize_to_tray: settings.minimize_to_tray,
        theme: settings.theme.clone(),
        accent_preset: settings.accent_preset.clone(),
        language: settings.language.clone(),
        webdav_url: settings.webdav_url.clone(),
        webdav_username: settings.webdav_username.clone(),
        webdav_auth_mode: settings.webdav_auth_mode.clone(),
        webdav_timeout_secs: settings.webdav_timeout_secs,
        auto_sync: settings.auto_sync,
        sync_interval_minutes: settings.sync_interval_minutes,
        editor_line_wrap: settings.editor_line_wrap,
        last_sync_at: settings.last_sync_at.clone(),
        webdav_secret_configured: status.kind == CredentialStatusKind::Configured,
        credential_status: status,
        settings_recovery_status: settings.settings_recovery_status,
    }
}

pub fn get_webdav_secret() -> Result<Option<String>, CredentialFailure> {
    init_settings();
    let state = SETTINGS.get().expect("Settings not initialized");
    let guard = state.lock().unwrap_or_else(|error| error.into_inner());
    if effective_recovery(&guard) != CredentialRecoveryStatus::None {
        return Err(CredentialFailure::Unavailable);
    }
    drop(guard);
    credential_store().read_secret()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::tests::MemoryCredentialStore;

    fn test_path(name: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("snipvault-settings-test-{stamp}"))
            .join(name)
    }

    fn configured_store() -> MemoryCredentialStore {
        MemoryCredentialStore::with_secret(None)
    }

    #[test]
    fn serialization_never_contains_secret_field_or_value() {
        let settings = Settings {
            language: "en".into(),
            ..Settings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(!json.contains("webdav_password"));
        assert!(!json.contains("test-secret-value"));
    }

    #[test]
    fn writes_and_reads_settings_without_secret_field() {
        let path = test_path("settings.json");
        let settings = Settings {
            language: "en".into(),
            ..Settings::default()
        };
        write_settings_to_path(&path, &settings).unwrap();
        let state = load_settings_from_path(&path, &configured_store()).unwrap();
        assert_eq!(state.settings, settings);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("webdav_password"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recovers_backup_after_interrupted_replace() {
        let path = test_path("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let settings = Settings::default();
        let backup = backup_path(&path);
        std::fs::write(&backup, serde_json::to_vec(&settings).unwrap()).unwrap();

        let state = load_settings_from_path(&path, &configured_store()).unwrap();
        assert_eq!(state.settings, settings);
        assert!(path.exists());
        assert!(!backup.exists());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_invalid_enum_interval_and_non_loopback_http() {
        let invalid_theme = Settings {
            theme: "invalid".into(),
            ..Settings::default()
        };
        assert!(validate_settings(&invalid_theme).is_err());

        let enabled_with_zero_interval = Settings {
            auto_sync: true,
            sync_interval_minutes: 0,
            ..Settings::default()
        };
        assert!(validate_settings(&enabled_with_zero_interval).is_err());

        let insecure = Settings {
            webdav_url: "http://example.test/dav".into(),
            ..Settings::default()
        };
        assert!(validate_settings(&insecure).is_err());
    }

    #[test]
    fn migrates_legacy_secret_then_sanitizes_json() {
        let path = test_path("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"webdav_password":"migration-value","theme":"system","language":"zh","webdav_auth_mode":"auto","webdav_timeout_secs":30,"sync_interval_minutes":30,"minimize_to_tray":true,"editor_line_wrap":true}"#,
        )
        .unwrap();
        let store = configured_store();

        let state = load_settings_from_path(&path, &store).unwrap();
        assert_eq!(store.secret().as_deref(), Some("migration-value"));
        assert_eq!(
            state.settings.credential_recovery_status,
            CredentialRecoveryStatus::None
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("webdav_password"));
        assert!(!text.contains("migration-value"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn migration_failure_preserves_legacy_file_and_blocks_sync() {
        let path = test_path("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let legacy = r#"{"webdav_password":"migration-value","theme":"system","language":"zh","webdav_auth_mode":"auto","webdav_timeout_secs":30,"sync_interval_minutes":30,"minimize_to_tray":true,"editor_line_wrap":true}"#;
        std::fs::write(&path, legacy).unwrap();
        let store = configured_store();
        store.fail_next_write(CredentialFailure::Denied);

        let state = load_settings_from_path(&path, &store).unwrap();
        assert_eq!(
            state.runtime_credential_recovery,
            Some(CredentialRecoveryStatus::LegacyMigrationRequired)
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), legacy);
        assert_eq!(store.secret(), None);
        let serialized = serde_json::to_string(&credential_status_for(
            &state.settings,
            effective_recovery(&state),
            &store,
        ))
        .unwrap();
        assert!(!serialized.contains("migration-value"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_primary_uses_valid_backup_and_quarantines_without_overwrite() {
        let path = test_path("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not-json").unwrap();
        let backup = backup_path(&path);
        std::fs::write(&backup, serde_json::to_vec(&Settings::default()).unwrap()).unwrap();

        let state = load_settings_from_path(&path, &configured_store()).unwrap();
        assert_eq!(
            state.settings.settings_recovery_status,
            SettingsRecoveryStatus::BackupRestored
        );
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.iter().any(|name| name.ends_with(".corrupt")));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_primary_and_backup_load_defaults_with_recovery_status() {
        let path = test_path("settings.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not-json").unwrap();
        std::fs::write(backup_path(&path), "also-not-json").unwrap();

        let state = load_settings_from_path(&path, &configured_store()).unwrap();
        assert_eq!(
            state.settings.settings_recovery_status,
            SettingsRecoveryStatus::DefaultsLoaded
        );
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(!written.contains("webdav_password"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn redacted_view_reports_state_without_secret() {
        let settings = Settings::default();
        let store = MemoryCredentialStore::with_secret(Some("redaction-value"));
        let status = credential_status_for(&settings, CredentialRecoveryStatus::None, &store);
        assert_eq!(status.kind, CredentialStatusKind::Configured);
        let serialized = serde_json::to_string(&status).unwrap();
        assert!(!serialized.contains("redaction-value"));
    }
}
