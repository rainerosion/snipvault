use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyncErrorKind {
    Busy,
    Configuration,
    Authentication,
    Authorization,
    Validation,
    Network,
    LocalPersistence,
    CasConflict,
    LegacyChanged,
    OutboxFull,
    RetryLimit,
    Deadline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SyncError {
    pub(crate) kind: SyncErrorKind,
    pub(crate) retryable: bool,
    message: &'static str,
}

impl SyncError {
    pub(crate) const fn new(kind: SyncErrorKind, retryable: bool, message: &'static str) -> Self {
        Self {
            kind,
            retryable,
            message,
        }
    }

    pub(crate) const fn is_retryable(&self) -> bool {
        self.retryable
    }

    pub(crate) const fn busy() -> Self {
        Self::new(
            SyncErrorKind::Busy,
            true,
            "已有同步任务正在运行，请稍后重试",
        )
    }

    pub(crate) const fn configuration(message: &'static str) -> Self {
        Self::new(SyncErrorKind::Configuration, false, message)
    }

    pub(crate) const fn authentication() -> Self {
        Self::new(
            SyncErrorKind::Authentication,
            false,
            "WebDAV authentication failed",
        )
    }

    pub(crate) const fn authorization() -> Self {
        Self::new(
            SyncErrorKind::Authorization,
            false,
            "WebDAV access was denied",
        )
    }

    pub(crate) const fn validation(message: &'static str) -> Self {
        Self::new(SyncErrorKind::Validation, false, message)
    }

    pub(crate) const fn network(message: &'static str) -> Self {
        Self::new(SyncErrorKind::Network, true, message)
    }

    pub(crate) const fn local(message: &'static str) -> Self {
        Self::new(SyncErrorKind::LocalPersistence, true, message)
    }

    pub(crate) const fn cas_conflict() -> Self {
        Self::new(
            SyncErrorKind::CasConflict,
            true,
            "Remote manifest changed during publication; retry synchronization",
        )
    }

    pub(crate) const fn legacy_changed() -> Self {
        Self::new(
            SyncErrorKind::LegacyChanged,
            true,
            "Legacy WebDAV data changed during protocol upgrade; retry synchronization",
        )
    }

    pub(crate) const fn outbox_full() -> Self {
        Self::new(
            SyncErrorKind::OutboxFull,
            false,
            "The pending synchronization queue has reached its safety limit",
        )
    }

    pub(crate) const fn retry_limit() -> Self {
        Self::new(
            SyncErrorKind::RetryLimit,
            true,
            "WebDAV operation exceeded its retry limit; retry synchronization later",
        )
    }

    pub(crate) const fn deadline() -> Self {
        Self::new(
            SyncErrorKind::Deadline,
            true,
            "Synchronization deadline exceeded; pending changes will be retried next time",
        )
    }

    #[cfg(test)]
    pub(crate) const fn convergence_limit() -> Self {
        Self::new(
            SyncErrorKind::RetryLimit,
            true,
            "Synchronization did not converge within the bounded rounds; pending changes will be retried next time",
        )
    }
}

impl fmt::Display for SyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.message)
    }
}

impl std::error::Error for SyncError {}

impl From<String> for SyncError {
    fn from(_error: String) -> Self {
        Self::validation("Remote synchronization data is invalid")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_expose_only_safe_static_messages() {
        let serialized = format!("{}", SyncError::network("WebDAV request failed"));
        assert!(!serialized.contains("https://"));
        assert!(!serialized.contains("secret"));
        assert!(!serialized.contains("server body"));
    }
}
