use std::fmt;
use std::sync::Arc;

/// Stable credential identity. Do not change these values without a migration.
pub const CREDENTIAL_SERVICE: &str = "cn.rainss.snipvault.webdav";
pub const CREDENTIAL_ACCOUNT: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialFailure {
    Unavailable,
    Denied,
    Invalid,
    Ambiguous,
}

impl fmt::Display for CredentialFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable => "credential store unavailable",
            Self::Denied => "credential store access denied",
            Self::Invalid => "credential store request invalid",
            Self::Ambiguous => "credential store entry ambiguous",
        })
    }
}

pub trait CredentialStore: Send + Sync {
    fn read_secret(&self) -> Result<Option<String>, CredentialFailure>;
    fn write_secret(&self, secret: &str) -> Result<(), CredentialFailure>;
    fn clear_secret(&self) -> Result<(), CredentialFailure>;
}

#[derive(Debug, Default)]
pub struct PlatformCredentialStore;

impl PlatformCredentialStore {
    fn entry(&self) -> Result<keyring::Entry, CredentialFailure> {
        keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_ACCOUNT).map_err(map_keyring_error)
    }
}

impl CredentialStore for PlatformCredentialStore {
    fn read_secret(&self) -> Result<Option<String>, CredentialFailure> {
        match self.entry()?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(map_keyring_error(error)),
        }
    }

    fn write_secret(&self, secret: &str) -> Result<(), CredentialFailure> {
        if secret.is_empty() || secret.len() > 8192 {
            return Err(CredentialFailure::Invalid);
        }
        self.entry()?
            .set_password(secret)
            .map_err(map_keyring_error)
    }

    fn clear_secret(&self) -> Result<(), CredentialFailure> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(map_keyring_error(error)),
        }
    }
}

fn map_keyring_error(error: keyring::Error) -> CredentialFailure {
    match error {
        keyring::Error::NoStorageAccess(_) => CredentialFailure::Denied,
        keyring::Error::Ambiguous(_) => CredentialFailure::Ambiguous,
        keyring::Error::Invalid(_, _)
        | keyring::Error::TooLong(_, _)
        | keyring::Error::BadEncoding(_)
        | keyring::Error::BadDataFormat(_, _)
        | keyring::Error::BadStoreFormat(_)
        | keyring::Error::NotSupportedByStore(_) => CredentialFailure::Invalid,
        keyring::Error::NoEntry
        | keyring::Error::NoDefaultStore
        | keyring::Error::PlatformFailure(_) => CredentialFailure::Unavailable,
        _ => CredentialFailure::Unavailable,
    }
}

pub fn platform_store() -> Arc<dyn CredentialStore> {
    Arc::new(PlatformCredentialStore)
}

#[cfg(test)]
pub mod tests {
    use super::{CredentialFailure, CredentialStore};
    use std::collections::VecDeque;
    use std::sync::Mutex;

    #[derive(Debug, Default)]
    pub struct MemoryCredentialStore {
        secret: Mutex<Option<String>>,
        read_failures: Mutex<VecDeque<CredentialFailure>>,
        write_failures: Mutex<VecDeque<CredentialFailure>>,
        clear_failures: Mutex<VecDeque<CredentialFailure>>,
    }

    impl MemoryCredentialStore {
        pub fn with_secret(secret: Option<&str>) -> Self {
            Self {
                secret: Mutex::new(secret.map(str::to_string)),
                ..Self::default()
            }
        }

        pub fn fail_next_read(&self, failure: CredentialFailure) {
            self.read_failures.lock().unwrap().push_back(failure);
        }

        pub fn fail_next_write(&self, failure: CredentialFailure) {
            self.write_failures.lock().unwrap().push_back(failure);
        }

        pub fn fail_next_clear(&self, failure: CredentialFailure) {
            self.clear_failures.lock().unwrap().push_back(failure);
        }

        pub fn secret(&self) -> Option<String> {
            self.secret.lock().unwrap().clone()
        }
    }

    impl CredentialStore for MemoryCredentialStore {
        fn read_secret(&self) -> Result<Option<String>, CredentialFailure> {
            if let Some(failure) = self.read_failures.lock().unwrap().pop_front() {
                return Err(failure);
            }
            Ok(self.secret())
        }

        fn write_secret(&self, secret: &str) -> Result<(), CredentialFailure> {
            if let Some(failure) = self.write_failures.lock().unwrap().pop_front() {
                return Err(failure);
            }
            *self.secret.lock().unwrap() = Some(secret.to_string());
            Ok(())
        }

        fn clear_secret(&self) -> Result<(), CredentialFailure> {
            if let Some(failure) = self.clear_failures.lock().unwrap().pop_front() {
                return Err(failure);
            }
            *self.secret.lock().unwrap() = None;
            Ok(())
        }
    }
}
