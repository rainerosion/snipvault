use crate::error::{CommandError, ErrorCode};
use crate::settings::Settings;
use crate::webdav::{self, SyncResult};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

pub const SYNC_EVENT_NAME: &str = "sync-complete";
pub const AUTO_SYNC_POLL_INTERVAL: Duration = Duration::from_secs(15);
const AUTO_SYNC_FAILURE_BACKOFF_CAP: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncSource {
    Toolbar,
    Settings,
    Tray,
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SyncStatus {
    Result,
    Error,
    Busy,
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncEventPayload {
    pub source: SyncSource,
    pub status: SyncStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<SyncResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}

impl SyncEventPayload {
    pub fn result(source: SyncSource, result: SyncResult) -> Self {
        Self {
            source,
            status: SyncStatus::Result,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(source: SyncSource, error: CommandError) -> Self {
        let status = if error.code == ErrorCode::SyncBusy {
            SyncStatus::Busy
        } else {
            SyncStatus::Error
        };
        Self {
            source,
            status,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutoSyncConfig {
    interval: Duration,
    webdav_url: String,
    webdav_username: String,
    credential_revision: u64,
    webdav_auth_mode: String,
    webdav_timeout_secs: u64,
}

impl AutoSyncConfig {
    fn from_settings(settings: &Settings) -> Option<Self> {
        if !settings.auto_sync
            || settings.sync_interval_minutes <= 0
            || settings.webdav_url.trim().is_empty()
        {
            return None;
        }

        let minutes = u64::try_from(settings.sync_interval_minutes).ok()?;
        let seconds = minutes.checked_mul(60)?;
        Some(Self {
            interval: Duration::from_secs(seconds),
            webdav_url: settings.webdav_url.clone(),
            webdav_username: settings.webdav_username.clone(),
            credential_revision: settings.credential_revision,
            webdav_auth_mode: settings.webdav_auth_mode.clone(),
            webdav_timeout_secs: settings.webdav_timeout_secs,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncAttemptOutcome {
    Success,
    Busy,
    Failure,
}

#[derive(Debug, Default)]
struct AutoSyncScheduler {
    config: Option<AutoSyncConfig>,
    next_due: Option<Instant>,
    consecutive_failures: u32,
}

impl AutoSyncScheduler {
    fn is_due(&mut self, now: Instant, config: Option<&AutoSyncConfig>) -> bool {
        let Some(config) = config else {
            self.reset();
            return false;
        };

        if self.config.as_ref() != Some(config) {
            self.config = Some(config.clone());
            self.next_due = Some(now);
            self.consecutive_failures = 0;
        }

        self.next_due.map(|due| now >= due).unwrap_or(true)
    }

    fn record_outcome(&mut self, now: Instant, outcome: SyncAttemptOutcome) {
        let Some(config) = self.config.as_ref() else {
            self.reset();
            return;
        };

        let delay = match outcome {
            SyncAttemptOutcome::Success => {
                self.consecutive_failures = 0;
                config.interval
            }
            SyncAttemptOutcome::Busy => AUTO_SYNC_POLL_INTERVAL,
            SyncAttemptOutcome::Failure => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                failure_backoff(self.consecutive_failures)
            }
        };
        self.next_due = now.checked_add(delay);
    }

    fn reset(&mut self) {
        self.config = None;
        self.next_due = None;
        self.consecutive_failures = 0;
    }
}

fn failure_backoff(consecutive_failures: u32) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(16);
    let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
    let seconds = AUTO_SYNC_POLL_INTERVAL
        .as_secs()
        .saturating_mul(multiplier)
        .min(AUTO_SYNC_FAILURE_BACKOFF_CAP.as_secs());
    Duration::from_secs(seconds)
}

fn run_sync(source: SyncSource) -> SyncEventPayload {
    match webdav::sync_merge() {
        Ok(result) => {
            log::info!("Sync completed for source={source:?}: {}", result.message);
            SyncEventPayload::result(source, result)
        }
        Err(error) => {
            log::error!("Sync failed for source={source:?}: {error}");
            SyncEventPayload::error(source, CommandError::sync(&error))
        }
    }
}

fn schedule_outcome(payload: &SyncEventPayload) -> SyncAttemptOutcome {
    match payload.status {
        SyncStatus::Busy => SyncAttemptOutcome::Busy,
        SyncStatus::Result
            if payload
                .result
                .as_ref()
                .map(|result| result.success)
                .unwrap_or(false) =>
        {
            SyncAttemptOutcome::Success
        }
        SyncStatus::Result | SyncStatus::Error => SyncAttemptOutcome::Failure,
    }
}

pub fn emit_sync_event(app: &AppHandle, payload: &SyncEventPayload) {
    let Some(window) = app.get_webview_window("main") else {
        log::warn!("Could not emit sync event because the main window is unavailable");
        return;
    };
    if let Err(error) = window.emit(SYNC_EVENT_NAME, payload) {
        log::error!("Emitting sync event failed: {error}");
    }
}

pub fn run_and_emit(app: &AppHandle, source: SyncSource) {
    let payload = run_sync(source);
    emit_sync_event(app, &payload);
}

pub fn start_auto_sync_worker(app: AppHandle) {
    std::thread::spawn(move || {
        let mut scheduler = AutoSyncScheduler::default();

        loop {
            let settings = crate::settings::get_settings();
            let config = AutoSyncConfig::from_settings(&settings);
            let now = Instant::now();
            if scheduler.is_due(now, config.as_ref()) {
                log::info!(
                    "Running scheduled auto-sync (interval={} minutes)",
                    settings.sync_interval_minutes
                );
                let payload = run_sync(SyncSource::Background);
                let outcome = schedule_outcome(&payload);
                emit_sync_event(&app, &payload);
                scheduler.record_outcome(Instant::now(), outcome);
            }

            std::thread::sleep(AUTO_SYNC_POLL_INTERVAL);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(interval: Duration, url: &str) -> AutoSyncConfig {
        AutoSyncConfig {
            interval,
            webdav_url: url.into(),
            webdav_username: "user".into(),
            credential_revision: 1,
            webdav_auth_mode: "auto".into(),
            webdav_timeout_secs: 30,
        }
    }

    #[test]
    fn initial_and_successful_attempts_follow_configured_interval() {
        let start = Instant::now();
        let config = config(Duration::from_secs(30 * 60), "https://one.test/dav");
        let mut scheduler = AutoSyncScheduler::default();

        assert!(scheduler.is_due(start, Some(&config)));
        scheduler.record_outcome(start, SyncAttemptOutcome::Success);
        assert!(!scheduler.is_due(start + Duration::from_secs(30 * 60 - 1), Some(&config)));
        assert!(scheduler.is_due(start + Duration::from_secs(30 * 60), Some(&config)));
    }

    #[test]
    fn busy_and_failures_use_bounded_retry_delays() {
        assert_eq!(failure_backoff(1), Duration::from_secs(15));
        assert_eq!(failure_backoff(2), Duration::from_secs(30));
        assert_eq!(failure_backoff(3), Duration::from_secs(60));
        assert_eq!(failure_backoff(32), AUTO_SYNC_FAILURE_BACKOFF_CAP);

        let start = Instant::now();
        let config = config(Duration::from_secs(30 * 60), "https://one.test/dav");
        let mut scheduler = AutoSyncScheduler::default();
        assert!(scheduler.is_due(start, Some(&config)));
        scheduler.record_outcome(start, SyncAttemptOutcome::Busy);
        assert!(!scheduler.is_due(
            start + AUTO_SYNC_POLL_INTERVAL - Duration::from_secs(1),
            Some(&config)
        ));
        assert!(scheduler.is_due(start + AUTO_SYNC_POLL_INTERVAL, Some(&config)));
    }

    #[test]
    fn disabling_and_relevant_config_changes_make_next_poll_due() {
        let start = Instant::now();
        let first = config(Duration::from_secs(30 * 60), "https://one.test/dav");
        let second = config(Duration::from_secs(30 * 60), "https://two.test/dav");
        let mut scheduler = AutoSyncScheduler::default();

        assert!(scheduler.is_due(start, Some(&first)));
        scheduler.record_outcome(start, SyncAttemptOutcome::Success);
        let config_change = start + AUTO_SYNC_POLL_INTERVAL;
        assert!(scheduler.is_due(config_change, Some(&second)));

        let disabled_at = config_change + Duration::from_secs(31);
        assert!(!scheduler.is_due(disabled_at, None));
        assert!(scheduler.is_due(disabled_at + Duration::from_secs(1), Some(&second)));
    }

    #[test]
    fn event_payload_serializes_source_status_and_safe_error() {
        let payload = SyncEventPayload::error(
            SyncSource::Tray,
            CommandError::new(
                ErrorCode::SyncBusy,
                "Another synchronization is already running.",
                true,
            ),
        );
        let value = serde_json::to_value(payload).expect("sync event should serialize");

        assert_eq!(value["source"], "tray");
        assert_eq!(value["status"], "busy");
        assert_eq!(value["error"]["code"], "sync_busy");
        assert!(value.get("result").is_none());
    }
}
