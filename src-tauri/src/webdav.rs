#[cfg(test)]
mod engine;
mod engine_v2;
pub(crate) mod error;
mod protocol;
mod store;
mod transport;

use engine_v2::{V2SyncEngine, DEFAULT_SYNC_DEADLINE};
use error::SyncError;
use once_cell::sync::Lazy;
use protocol::WebDavBase;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;
use store::ProductionStore;
use transport::{Clock, ReqwestTransport, SystemClock, WebDavAuth};

static SYNC_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

/// Structured native synchronization failure kept intact until the IPC adapter.
/// Its display text is always a redacted, static diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncFailure(SyncError);

impl SyncFailure {
    pub fn is_busy(&self) -> bool {
        self.0.kind == error::SyncErrorKind::Busy
    }

    pub(crate) fn kind(&self) -> error::SyncErrorKind {
        self.0.kind
    }

    pub fn is_retryable(&self) -> bool {
        self.0.is_retryable()
    }
}

impl From<SyncError> for SyncFailure {
    fn from(error: SyncError) -> Self {
        Self(error)
    }
}

impl std::fmt::Display for SyncFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for SyncFailure {}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct SyncResult {
    pub success: bool,
    pub message: String,
    /// Compatibility field retained for the existing frontend contract.
    pub uploaded: bool,
    pub uploaded_count: usize,
    pub downloaded_count: usize,
    pub deleted_count: usize,
    pub conflict_count: usize,
    pub pending_count: usize,
    pub protocol_version: u64,
    pub manifest_generation: u64,
    pub total_count: usize,
}

pub fn validate_base_url(raw: &str) -> Result<(), String> {
    WebDavBase::parse(raw).map(|_| ())
}

fn try_sync_guard() -> Result<MutexGuard<'static, ()>, SyncError> {
    SYNC_LOCK.try_lock().map_err(|_| SyncError::busy())
}

fn sync_merge_internal() -> Result<SyncResult, SyncError> {
    let _guard = try_sync_guard()?;
    let current_settings = crate::settings::get_settings();
    if current_settings.webdav_url.trim().is_empty() {
        return Ok(SyncResult {
            success: false,
            message: "WebDAV 地址未配置，请在设置中填写".into(),
            ..Default::default()
        });
    }

    let base = WebDavBase::parse(&current_settings.webdav_url)
        .map_err(|_| SyncError::configuration("WebDAV address is invalid"))?;
    let secret = crate::settings::get_webdav_secret().map_err(|failure| {
        log::warn!("WebDAV credential lookup failed safely: {failure}");
        SyncError::configuration(
            "WebDAV credential is unavailable; open Settings and replace or clear it",
        )
    })?;
    let auth = WebDavAuth::from_settings(
        &current_settings.webdav_auth_mode,
        &current_settings.webdav_username,
        secret.as_deref().unwrap_or(""),
    )?;
    log::info!(
        "sync_merge: auth_mode={} transport={}",
        auth.mode.as_str(),
        if base.is_insecure() { "http" } else { "https" }
    );

    let clock = SystemClock;
    let configured_timeout = Duration::from_secs(current_settings.webdav_timeout_secs);
    let remote_id = base.remote_id(&current_settings.webdav_username);
    let transport = ReqwestTransport::new(base.clone(), auth, configured_timeout)?;
    let deadline = clock
        .now()
        .checked_add(DEFAULT_SYNC_DEADLINE)
        .ok_or_else(SyncError::deadline)?;
    let store = ProductionStore;
    let result =
        V2SyncEngine::new(&transport, &store, &clock, remote_id.clone(), deadline).run()?;

    let warning = if base.is_insecure() {
        "；警告：当前使用未加密 HTTP"
    } else {
        ""
    };
    let message = if result.uploaded_count == 0
        && result.downloaded_count == 0
        && result.deleted_count == 0
        && result.conflict_count == 0
    {
        format!(
            "同步完成：本地与远程数据一致（当前共 {} 条）{warning}",
            result.total_count
        )
    } else {
        format!(
            "同步完成：上传 {} 条，下载 {} 条，删除 {} 条，冲突副本 {} 条（当前共 {} 条）{warning}",
            result.uploaded_count,
            result.downloaded_count,
            result.deleted_count,
            result.conflict_count,
            result.total_count
        )
    };

    let pending_count = crate::db::load_sync_snapshot(&remote_id)
        .map_err(|_error| {
            log::error!("Reading pending synchronization count failed");
            SyncError::local("Reading synchronization status failed")
        })?
        .pending
        .len();
    crate::settings::update_settings(|settings| {
        settings.last_sync_at = chrono::Utc::now().to_rfc3339();
    })
    .map_err(|_error| {
        log::error!("Persisting last synchronization time failed");
        SyncError::local("Persisting synchronization status failed")
    })?;

    log::info!(
        "sync_merge: complete uploaded={} downloaded={} total={}",
        result.uploaded_count,
        result.downloaded_count,
        result.total_count
    );
    Ok(SyncResult {
        success: true,
        message,
        uploaded: result.uploaded_count > 0,
        uploaded_count: result.uploaded_count,
        downloaded_count: result.downloaded_count,
        deleted_count: result.deleted_count,
        conflict_count: result.conflict_count,
        pending_count,
        protocol_version: 2,
        manifest_generation: result.generation,
        total_count: result.total_count,
    })
}

#[cfg(test)]
pub(crate) fn test_convergence_failure() -> SyncFailure {
    SyncError::convergence_limit().into()
}

#[cfg(test)]
pub(crate) fn test_authentication_failure() -> SyncFailure {
    SyncError::authentication().into()
}

#[cfg(test)]
pub(crate) fn test_validation_failure() -> SyncFailure {
    SyncError::validation("Remote synchronization data is invalid").into()
}

#[cfg(test)]
pub(crate) fn test_network_failure(_sensitive_source: &str) -> SyncFailure {
    SyncError::network("WebDAV request failed").into()
}

#[cfg(test)]
pub(crate) fn test_outbox_full_failure() -> SyncFailure {
    SyncError::outbox_full().into()
}

#[cfg(test)]
pub(crate) fn test_cas_failure() -> SyncFailure {
    SyncError::cas_conflict().into()
}

#[cfg(test)]
pub(crate) fn test_legacy_changed_failure() -> SyncFailure {
    SyncError::legacy_changed().into()
}

#[cfg(test)]
pub(crate) fn test_try_sync_lock() -> Result<(), SyncFailure> {
    let _guard = try_sync_guard()?;
    Ok(())
}

pub fn sync_merge() -> Result<SyncResult, SyncFailure> {
    sync_merge_internal().map_err(SyncFailure::from)
}

pub fn sync_to_webdav() -> Result<SyncResult, SyncFailure> {
    sync_merge()
}

pub fn sync_from_webdav() -> Result<SyncResult, SyncFailure> {
    sync_merge()
}

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_errors_never_include_sensitive_source_data() {
        let errors = [
            SyncError::authentication(),
            SyncError::authorization(),
            SyncError::validation("Remote snippet JSON is invalid"),
            SyncError::network("WebDAV request failed"),
            SyncError::convergence_limit(),
        ];
        for error in errors {
            let message = error.to_string();
            assert!(!message.contains("https://"));
            assert!(!message.contains("secret"));
            assert!(!message.contains("server body"));
            assert!(!message.contains("snippet content"));
        }
    }

    #[test]
    fn process_lock_rejects_concurrent_trigger_as_retryable_busy() {
        let guard = SYNC_LOCK.lock().unwrap();
        let failure = std::thread::spawn(test_try_sync_lock)
            .join()
            .unwrap()
            .unwrap_err();
        drop(guard);

        assert!(failure.is_busy());
        assert!(failure.is_retryable());
    }
}
