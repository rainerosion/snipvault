use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;

/// Stable, public error codes exposed across the Tauri command boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    Validation,
    NotFound,
    StaleRevision,
    OutboxFull,
    Database,
    Settings,
    Network,
    SyncBusy,
    SyncCasConflict,
    SyncLegacyChanged,
    Import,
    Export,
    Autostart,
    Credential,
    Recovery,
    Open,
    Unknown,
}

/// A serializable command error containing only safe, user-facing data.
///
/// Internal source errors may be logged in native code, but must never be copied
/// into `message` or `details`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, String>>,
}

impl CommandError {
    pub fn new(code: ErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code,
            message: message.into(),
            retryable,
            details: None,
        }
    }

    pub fn validation() -> Self {
        Self::new(
            ErrorCode::Validation,
            "Some provided values are invalid.",
            false,
        )
    }

    pub fn not_found() -> Self {
        Self::new(
            ErrorCode::NotFound,
            "The requested item could not be found.",
            false,
        )
    }

    pub fn database(error: &rusqlite::Error) -> Self {
        if matches!(error, rusqlite::Error::QueryReturnedNoRows) {
            Self::not_found()
        } else {
            Self::new(
                ErrorCode::Database,
                "The local database operation failed.",
                true,
            )
        }
    }

    pub fn mutation(error: &crate::db::MutationError) -> Self {
        match error {
            crate::db::MutationError::Sqlite(error) => Self::database(error),
            crate::db::MutationError::StaleRevision {
                current_revision_id,
            } => {
                let mut details = BTreeMap::new();
                details.insert("current_revision_id".into(), current_revision_id.clone());
                let mut public = Self::new(
                    ErrorCode::StaleRevision,
                    "The snippet changed since this draft was loaded.",
                    false,
                );
                public.details = Some(details);
                public
            }
            crate::db::MutationError::PendingLimit => Self::new(
                ErrorCode::OutboxFull,
                "The pending synchronization queue has reached its safety limit.",
                false,
            ),
        }
    }

    pub fn settings() -> Self {
        Self::new(ErrorCode::Settings, "The settings operation failed.", true)
    }

    pub fn import() -> Self {
        Self::new(
            ErrorCode::Import,
            "The snippet import could not be completed.",
            false,
        )
    }

    pub fn export() -> Self {
        Self::new(
            ErrorCode::Export,
            "The snippet export could not be completed.",
            true,
        )
    }

    pub fn autostart() -> Self {
        Self::new(
            ErrorCode::Autostart,
            "The startup setting could not be changed.",
            true,
        )
    }

    pub fn recovery() -> Self {
        Self::new(
            ErrorCode::Recovery,
            "The change could not be fully compensated. Open Settings to recover.",
            false,
        )
    }

    pub fn credential(retryable: bool) -> Self {
        Self::new(
            ErrorCode::Credential,
            "The operating-system credential store could not complete the request.",
            retryable,
        )
    }

    pub fn open() -> Self {
        Self::new(
            ErrorCode::Open,
            "The requested trusted location could not be opened.",
            true,
        )
    }

    pub fn sync(error: &crate::webdav::SyncFailure) -> Self {
        use crate::webdav::error::SyncErrorKind;

        match error.kind() {
            SyncErrorKind::Busy => Self::new(
                ErrorCode::SyncBusy,
                "Another synchronization is already running.",
                true,
            ),
            SyncErrorKind::CasConflict => Self::new(
                ErrorCode::SyncCasConflict,
                "The remote vault changed while SnipVault was publishing.",
                true,
            ),
            SyncErrorKind::LegacyChanged => Self::new(
                ErrorCode::SyncLegacyChanged,
                "Legacy WebDAV data changed while SnipVault was upgrading the vault.",
                true,
            ),
            SyncErrorKind::OutboxFull => Self::new(
                ErrorCode::OutboxFull,
                "The pending synchronization queue has reached its safety limit.",
                false,
            ),
            _ => Self::new(
                ErrorCode::Network,
                "The synchronization request failed.",
                error.is_retryable(),
            ),
        }
    }

    pub fn unknown() -> Self {
        Self::new(
            ErrorCode::Unknown,
            "The operation could not be completed.",
            false,
        )
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::{CommandError, ErrorCode};

    #[test]
    fn serializes_stable_snake_case_contract() {
        let value = serde_json::to_value(CommandError::new(
            ErrorCode::SyncBusy,
            "Another synchronization is already running.",
            true,
        ))
        .expect("command error should serialize");

        assert_eq!(value["code"], "sync_busy");
        assert_eq!(
            value["message"],
            "Another synchronization is already running."
        );
        assert_eq!(value["retryable"], true);
        assert!(value.get("details").is_none());
    }

    #[test]
    fn public_sync_error_does_not_echo_sensitive_source() {
        let source = crate::webdav::test_network_failure(
            "GET https://user:secret@example.test/dav failed: server body token=abc",
        );
        let serialized = serde_json::to_string(&CommandError::sync(&source))
            .expect("command error should serialize");

        assert!(!serialized.contains("example.test"));
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("token=abc"));
    }

    #[test]
    fn bounded_convergence_error_stays_retryable_and_redacted() {
        let source = crate::webdav::test_convergence_failure();
        let error = CommandError::sync(&source);
        assert_eq!(error.code, ErrorCode::Network);
        assert!(error.retryable);
        let serialized = serde_json::to_string(&error).unwrap();
        assert!(!serialized.contains("https://"));
        assert!(!serialized.contains("secret"));
    }

    #[test]
    fn authentication_and_validation_sync_failures_are_not_retryable() {
        for source in [
            crate::webdav::test_authentication_failure(),
            crate::webdav::test_validation_failure(),
        ] {
            let error = CommandError::sync(&source);
            assert_eq!(error.code, ErrorCode::Network);
            assert!(!error.retryable);
        }
    }

    #[test]
    fn maps_v2_sync_failures_to_stable_codes() {
        let cases = [
            (
                crate::webdav::test_cas_failure(),
                ErrorCode::SyncCasConflict,
                true,
            ),
            (
                crate::webdav::test_legacy_changed_failure(),
                ErrorCode::SyncLegacyChanged,
                true,
            ),
            (
                crate::webdav::test_outbox_full_failure(),
                ErrorCode::OutboxFull,
                false,
            ),
        ];
        for (failure, expected, retryable) in cases {
            let error = CommandError::sync(&failure);
            assert_eq!(error.code, expected);
            assert_eq!(error.retryable, retryable);
        }
    }

    #[test]
    fn maps_missing_rows_to_not_found() {
        let error = CommandError::database(&rusqlite::Error::QueryReturnedNoRows);
        assert_eq!(error.code, ErrorCode::NotFound);
        assert!(!error.retryable);
    }
}
