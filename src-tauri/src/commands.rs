use crate::db::{self, Snippet};
use crate::error::CommandError;
use crate::settings::{self, SecretAction, Settings, SettingsInput, SettingsView};
use crate::webdav::{self, SyncResult};
use once_cell::sync::{Lazy, OnceCell};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;
use tauri::{command, AppHandle, Manager};

#[derive(serde::Serialize)]
pub struct ExportResult {
    pub saved_in_downloads: bool,
}

#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustedDirectory {
    Data,
    Export,
}

const PROJECT_REPOSITORY_URL: &str = "https://github.com/rainerosion/snipvault";

pub static BOOT_START: OnceCell<Instant> = OnceCell::new();
pub static WINDOW_SHOWN: AtomicBool = AtomicBool::new(false);
static SETTINGS_WRITE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

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
        boot_log(
            "window_show_error",
            &format!("reason={} err=no_main_window", reason),
        );
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
pub fn get_snippets() -> Result<Vec<Snippet>, CommandError> {
    db::get_all_snippets().map_err(|error| {
        log::error!("get_snippets failed");
        CommandError::database(&error)
    })
}

#[command]
pub fn query_snippets(request: db::SnippetQuery) -> Result<db::SnippetQueryResult, CommandError> {
    db::query_snippets(&request).map_err(|error| {
        log::error!("query_snippets failed");
        CommandError::database(&error)
    })
}

#[command]
pub fn get_snippet(id: String) -> Result<Snippet, CommandError> {
    db::get_snippet(&id).map_err(|error| {
        log::error!("get_snippet failed");
        CommandError::database(&error)
    })
}

#[command]
pub fn get_snippet_tags() -> Result<Vec<String>, CommandError> {
    db::list_distinct_tags().map_err(|error| {
        log::error!("get_snippet_tags failed");
        CommandError::database(&error)
    })
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
) -> Result<Snippet, CommandError> {
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
        revision_id: String::new(),
    };
    db::validate_snippet(&snippet).map_err(|error| {
        log::warn!("create_snippet validation failed: {error}");
        CommandError::validation()
    })?;
    db::create_snippet(&snippet).map_err(|error| {
        log::error!("create_snippet database write failed");
        CommandError::mutation(&error)
    })
}

#[command]
#[allow(clippy::too_many_arguments)]
pub fn update_snippet(
    id: String,
    title: String,
    content: String,
    language: String,
    description: String,
    tags: Vec<String>,
    is_favorite: bool,
    base_revision_id: String,
) -> Result<Snippet, CommandError> {
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
        revision_id: String::new(),
    };
    db::validate_snippet(&snippet).map_err(|error| {
        log::warn!("update_snippet validation failed: {error}");
        CommandError::validation()
    })?;
    db::update_snippet(&snippet, &base_revision_id).map_err(|error| {
        log::error!("update_snippet database write failed");
        CommandError::mutation(&error)
    })
}

#[command]
pub fn delete_snippet(id: String) -> Result<db::RevisionHead, CommandError> {
    db::delete_snippet(&id).map_err(|error| {
        log::error!("delete_snippet failed");
        CommandError::mutation(&error)
    })
}

#[command]
pub fn search_snippets(
    query: String,
    language: Option<String>,
    tag: Option<String>,
) -> Result<Vec<Snippet>, CommandError> {
    db::search_snippets(&query, language.as_deref(), tag.as_deref()).map_err(|error| {
        log::error!("search_snippets failed");
        CommandError::database(&error)
    })
}

#[command]
pub fn toggle_favorite(id: String) -> Result<Snippet, CommandError> {
    db::toggle_favorite(&id).map_err(|error| {
        log::error!("toggle_favorite failed");
        CommandError::mutation(&error)
    })
}

#[command]
pub fn export_snippets() -> Result<String, CommandError> {
    db::export_snippets().map_err(|_error| {
        log::error!("export_snippets failed");
        CommandError::export()
    })
}

#[command]
pub fn export_snippets_to_file() -> Result<ExportResult, CommandError> {
    let json = db::export_snippets().map_err(|_error| {
        log::error!("export_snippets_to_file serialization failed");
        CommandError::export()
    })?;

    let (export_dir, saved_in_downloads) = crate::paths::get_export_dir();
    std::fs::create_dir_all(&export_dir).map_err(|_error| {
        log::error!("creating export directory failed");
        CommandError::export()
    })?;

    let filename_stem = format!(
        "snipvault-backup-{}",
        chrono::Local::now().format("%Y-%m-%d_%H-%M-%S")
    );

    db::write_export_file(&export_dir, &filename_stem, &json).map_err(|_error| {
        log::error!("writing export file failed");
        CommandError::export()
    })?;

    Ok(ExportResult { saved_in_downloads })
}

fn trusted_directory_path(directory: TrustedDirectory) -> std::path::PathBuf {
    match directory {
        TrustedDirectory::Data => crate::paths::get_data_dir(),
        TrustedDirectory::Export => crate::paths::get_export_dir().0,
    }
}

#[command]
pub fn open_project_repository() -> Result<(), CommandError> {
    opener::open_browser(PROJECT_REPOSITORY_URL).map_err(|_error| {
        log::error!("Opening the allowlisted project URL failed");
        CommandError::open()
    })
}

#[command]
pub fn open_trusted_directory(directory: TrustedDirectory) -> Result<(), CommandError> {
    let path = trusted_directory_path(directory);
    std::fs::create_dir_all(&path).map_err(|_error| {
        log::error!("Creating a trusted application directory failed");
        CommandError::open()
    })?;
    opener::open(&path).map_err(|_error| {
        log::error!("Opening a trusted application directory failed");
        CommandError::open()
    })
}

#[command]
pub fn import_snippets(json_data: String) -> Result<db::ImportResult, CommandError> {
    db::import_snippets(&json_data).map_err(|_error| {
        log::warn!("import_snippets failed");
        CommandError::import()
    })
}

// --- Settings ---

#[command]
pub fn get_settings() -> Result<SettingsView, CommandError> {
    Ok(settings::get_settings_view())
}

fn settings_write_guard() -> std::sync::MutexGuard<'static, ()> {
    SETTINGS_WRITE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner())
}

fn apply_auto_start(app: &AppHandle, enabled: bool) -> Result<(), CommandError> {
    use tauri_plugin_autostart::ManagerExt;
    let autostart = app.autolaunch();
    let result = if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    };
    result.map_err(|error| {
        log::error!("applying autostart state failed: {error}");
        CommandError::autostart()
    })
}

fn persist_auto_start(app: &AppHandle, enabled: bool) -> Result<Settings, CommandError> {
    let current = settings::get_settings();
    if enabled == current.auto_start {
        return Ok(current);
    }

    apply_auto_start(app, enabled)?;
    match settings::update_settings(|settings| settings.auto_start = enabled) {
        Ok(saved) => Ok(saved),
        Err(save_error) => {
            log::error!("persisting autostart setting failed: {save_error}");
            if let Err(rollback_error) = apply_auto_start(app, current.auto_start) {
                log::error!("rolling back autostart state failed: {rollback_error}");
            }
            Err(CommandError::settings())
        }
    }
}

trait AutostartController {
    fn apply(&self, enabled: bool) -> Result<(), CommandError>;
}

struct TauriAutostartController<'a>(&'a AppHandle);

impl AutostartController for TauriAutostartController<'_> {
    fn apply(&self, enabled: bool) -> Result<(), CommandError> {
        apply_auto_start(self.0, enabled)
    }
}

fn apply_secret_action(
    store: &dyn crate::credentials::CredentialStore,
    action: &SecretAction,
) -> Result<(), crate::credentials::CredentialFailure> {
    match action {
        SecretAction::Keep => Ok(()),
        SecretAction::Replace(value) => store.write_secret(value),
        SecretAction::Clear => store.clear_secret(),
    }
}

fn restore_secret(
    store: &dyn crate::credentials::CredentialStore,
    previous: &Option<String>,
) -> Result<(), crate::credentials::CredentialFailure> {
    match previous {
        Some(value) => store.write_secret(value),
        None => store.clear_secret(),
    }
}

fn save_settings_transaction(
    current: &Settings,
    candidate: Settings,
    secret_action: &SecretAction,
    autostart: &dyn AutostartController,
    store: &dyn crate::credentials::CredentialStore,
    persist: impl FnOnce(Settings) -> Result<Settings, String>,
    mark_recovery: &dyn Fn(),
) -> Result<Settings, CommandError> {
    settings::validate_settings(&candidate).map_err(|error| {
        log::warn!("save_settings validation failed: {error}");
        CommandError::validation()
    })?;
    settings::validate_secret_action(secret_action).map_err(|error| {
        log::warn!("save_settings secret action validation failed: {error}");
        CommandError::validation()
    })?;

    let recovery_required =
        current.credential_recovery_status != settings::CredentialRecoveryStatus::None;
    if recovery_required && matches!(secret_action, SecretAction::Keep) {
        return Err(CommandError::credential(false));
    }

    let previous_secret = if matches!(secret_action, SecretAction::Keep) {
        None
    } else {
        Some(store.read_secret().map_err(|failure| {
            log::warn!("Credential snapshot failed safely: {failure}");
            CommandError::credential(matches!(
                failure,
                crate::credentials::CredentialFailure::Unavailable
            ))
        })?)
    };

    if let Err(failure) = apply_secret_action(store, secret_action) {
        log::warn!("Credential action failed safely: {failure}");
        return Err(CommandError::credential(matches!(
            failure,
            crate::credentials::CredentialFailure::Unavailable
        )));
    }

    let autostart_changed = candidate.auto_start != current.auto_start;
    if autostart_changed {
        if let Err(error) = autostart.apply(candidate.auto_start) {
            let credential_rollback_failed = previous_secret
                .as_ref()
                .map(|previous| restore_secret(store, previous).is_err())
                .unwrap_or(false);
            if credential_rollback_failed {
                mark_recovery();
                return Err(CommandError::recovery());
            }
            return Err(error);
        }
    }

    match persist(candidate) {
        Ok(saved) => Ok(saved),
        Err(save_error) => {
            log::error!("Saving sanitized settings failed: {save_error}");
            let autostart_rollback_failed =
                autostart_changed && autostart.apply(current.auto_start).is_err();
            let credential_rollback_failed = previous_secret
                .as_ref()
                .map(|previous| restore_secret(store, previous).is_err())
                .unwrap_or(false);
            if autostart_rollback_failed || credential_rollback_failed {
                mark_recovery();
                Err(CommandError::recovery())
            } else {
                Err(CommandError::settings())
            }
        }
    }
}

fn save_settings_candidate(
    current: &Settings,
    mut candidate: Settings,
    secret_action: &SecretAction,
    app: &AppHandle,
) -> Result<Settings, CommandError> {
    let changes_secret = !matches!(secret_action, SecretAction::Keep);
    if changes_secret {
        candidate.credential_revision = current.credential_revision.saturating_add(1);
    }
    candidate.credential_recovery_status = settings::CredentialRecoveryStatus::None;
    candidate.settings_recovery_status = settings::SettingsRecoveryStatus::None;

    let autostart_changed = candidate.auto_start != current.auto_start;
    let store = settings::credential_store();
    let saved = save_settings_transaction(
        current,
        candidate,
        secret_action,
        &TauriAutostartController(app),
        store.as_ref(),
        settings::replace_settings,
        &settings::mark_credential_recovery_required,
    )?;
    if autostart_changed {
        if let Err(error) = crate::tray::refresh_menu(app) {
            log::error!("Refreshing tray menu after settings save failed: {error}");
        }
    }
    Ok(saved)
}

#[command]
pub fn save_settings(
    new_settings: SettingsInput,
    secret_action: SecretAction,
    app: AppHandle,
) -> Result<SettingsView, CommandError> {
    let _write_guard = settings_write_guard();
    let current = settings::get_settings();
    let candidate = new_settings.apply_to(&current);
    save_settings_candidate(&current, candidate, &secret_action, &app)?;
    Ok(settings::get_settings_view())
}

// --- Auto-start ---

fn refresh_tray_after_auto_start_change(app: &AppHandle) {
    if let Err(error) = crate::tray::refresh_menu(app) {
        log::error!("refreshing tray menu after autostart change failed: {error}");
    }
}

pub fn toggle_auto_start_from_tray(app: &AppHandle) -> Result<bool, CommandError> {
    let _write_guard = settings_write_guard();
    let enabled = !settings::get_settings().auto_start;
    persist_auto_start(app, enabled)?;
    refresh_tray_after_auto_start_change(app);
    Ok(enabled)
}

#[command]
pub fn set_auto_start(enabled: bool, app: AppHandle) -> Result<(), CommandError> {
    let _write_guard = settings_write_guard();
    persist_auto_start(&app, enabled)?;
    refresh_tray_after_auto_start_change(&app);
    Ok(())
}

#[command]
pub fn is_auto_start_enabled(app: AppHandle) -> Result<bool, CommandError> {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().map_err(|error| {
        log::error!("reading autostart state failed: {error}");
        CommandError::autostart()
    })
}

// --- WebDAV Sync ---

#[command]
pub fn sync_upload() -> Result<SyncResult, CommandError> {
    webdav::sync_to_webdav().map_err(|error| {
        log::error!("sync_upload failed: {error}");
        CommandError::sync(&error)
    })
}

#[command]
pub fn sync_download() -> Result<SyncResult, CommandError> {
    webdav::sync_from_webdav().map_err(|error| {
        log::error!("sync_download failed: {error}");
        CommandError::sync(&error)
    })
}

#[command]
pub fn get_sync_versions() -> Result<Vec<db::SyncVersion>, CommandError> {
    db::get_sync_versions().map_err(|error| {
        log::error!("get_sync_versions failed");
        CommandError::database(&error)
    })
}

#[command]
pub fn get_system_theme(app: AppHandle) -> Result<String, CommandError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::tests::MemoryCredentialStore;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeAutostart {
        state: Mutex<bool>,
        failures: Mutex<Vec<bool>>,
    }

    impl FakeAutostart {
        fn new(enabled: bool) -> Self {
            Self {
                state: Mutex::new(enabled),
                ..Self::default()
            }
        }

        fn enabled(&self) -> bool {
            *self.state.lock().unwrap()
        }
    }

    impl AutostartController for FakeAutostart {
        fn apply(&self, enabled: bool) -> Result<(), CommandError> {
            if self.failures.lock().unwrap().pop().unwrap_or(false) {
                return Err(CommandError::autostart());
            }
            *self.state.lock().unwrap() = enabled;
            Ok(())
        }
    }

    fn candidate(current: &Settings) -> Settings {
        SettingsInput {
            auto_start: current.auto_start,
            minimize_to_tray: current.minimize_to_tray,
            theme: current.theme.clone(),
            accent_preset: current.accent_preset.clone(),
            language: current.language.clone(),
            webdav_url: current.webdav_url.clone(),
            webdav_username: current.webdav_username.clone(),
            webdav_auth_mode: current.webdav_auth_mode.clone(),
            webdav_timeout_secs: current.webdav_timeout_secs,
            auto_sync: current.auto_sync,
            sync_interval_minutes: current.sync_interval_minutes,
            editor_line_wrap: current.editor_line_wrap,
        }
        .apply_to(current)
    }

    #[test]
    fn keep_replace_and_clear_use_explicit_secret_actions() {
        let current = Settings::default();
        let autostart = FakeAutostart::new(false);
        let store = MemoryCredentialStore::with_secret(Some("old-value"));
        let mark_count = Arc::new(Mutex::new(0));
        let mark = {
            let mark_count = Arc::clone(&mark_count);
            move || *mark_count.lock().unwrap() += 1
        };

        save_settings_transaction(
            &current,
            candidate(&current),
            &SecretAction::Keep,
            &autostart,
            &store,
            Ok,
            &mark,
        )
        .unwrap();
        assert_eq!(store.secret().as_deref(), Some("old-value"));

        save_settings_transaction(
            &current,
            candidate(&current),
            &SecretAction::Replace("new-value".into()),
            &autostart,
            &store,
            Ok,
            &mark,
        )
        .unwrap();
        assert_eq!(store.secret().as_deref(), Some("new-value"));

        save_settings_transaction(
            &current,
            candidate(&current),
            &SecretAction::Clear,
            &autostart,
            &store,
            Ok,
            &mark,
        )
        .unwrap();
        assert_eq!(store.secret(), None);
        assert_eq!(*mark_count.lock().unwrap(), 0);
    }

    #[test]
    fn settings_failure_rolls_back_credential_and_autostart() {
        let current = Settings::default();
        let mut next = candidate(&current);
        next.auto_start = true;
        let autostart = FakeAutostart::new(false);
        let store = MemoryCredentialStore::with_secret(Some("old-value"));
        let marked = Mutex::new(false);

        let error = save_settings_transaction(
            &current,
            next,
            &SecretAction::Replace("new-value".into()),
            &autostart,
            &store,
            |_| Err("forced persistence failure".into()),
            &|| *marked.lock().unwrap() = true,
        )
        .unwrap_err();

        assert_eq!(error.code, crate::error::ErrorCode::Settings);
        assert_eq!(store.secret().as_deref(), Some("old-value"));
        assert!(!autostart.enabled());
        assert!(!*marked.lock().unwrap());
    }

    #[test]
    fn compensation_failure_returns_safe_recovery_error() {
        let current = Settings::default();
        let mut next = candidate(&current);
        next.auto_start = true;
        let autostart = FakeAutostart::new(false);
        // Stack order: candidate apply succeeds, rollback fails.
        autostart.failures.lock().unwrap().extend([true, false]);
        let store = MemoryCredentialStore::with_secret(Some("old-value"));
        let marked = Mutex::new(false);

        let error = save_settings_transaction(
            &current,
            next,
            &SecretAction::Replace("new-value".into()),
            &autostart,
            &store,
            |_| Err("forced persistence failure".into()),
            &|| *marked.lock().unwrap() = true,
        )
        .unwrap_err();

        assert_eq!(error.code, crate::error::ErrorCode::Recovery);
        assert_eq!(store.secret().as_deref(), Some("old-value"));
        assert!(*marked.lock().unwrap());
        let serialized = serde_json::to_string(&error).unwrap();
        assert!(!serialized.contains("old-value"));
        assert!(!serialized.contains("new-value"));
    }

    #[test]
    fn trusted_open_targets_are_fixed_and_path_derived() {
        assert_eq!(
            PROJECT_REPOSITORY_URL,
            "https://github.com/rainerosion/snipvault"
        );
        assert_eq!(
            trusted_directory_path(TrustedDirectory::Data),
            crate::paths::get_data_dir()
        );
        assert_eq!(
            trusted_directory_path(TrustedDirectory::Export),
            crate::paths::get_export_dir().0
        );
    }
}
