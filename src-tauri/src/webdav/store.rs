use super::error::SyncError;
use crate::db::{self, ApplyRemotePlanResult, PublishCommit, SyncSnapshot, ValidatedRemotePlan};
#[cfg(test)]
use crate::db::{MergeResult, Snippet};

/// Production v2 persistence boundary. Every call acquires and releases the
/// SQLite mutex internally; the engine owns plain data while performing HTTP.
pub(crate) trait V2SyncStore {
    fn load_snapshot(&self, remote_id: &str) -> Result<SyncSnapshot, SyncError>;
    fn load_vault_id(&self, remote_id: &str) -> Result<Option<String>, SyncError>;
    fn apply_remote_plan(
        &self,
        plan: &ValidatedRemotePlan,
    ) -> Result<ApplyRemotePlanResult, SyncError>;
    fn commit_published(&self, commit: &PublishCommit) -> Result<usize, SyncError>;
}

#[derive(Debug, Default)]
pub(crate) struct ProductionStore;

impl V2SyncStore for ProductionStore {
    fn load_snapshot(&self, remote_id: &str) -> Result<SyncSnapshot, SyncError> {
        db::load_sync_snapshot(remote_id).map_err(|_error| {
            log::error!("Reading synchronization snapshot failed");
            SyncError::local("Reading local synchronization data failed")
        })
    }

    fn load_vault_id(&self, remote_id: &str) -> Result<Option<String>, SyncError> {
        db::load_remote_vault_id(remote_id).map_err(|_error| {
            log::error!("Reading synchronization vault identity failed");
            SyncError::local("Reading synchronization vault identity failed")
        })
    }

    fn apply_remote_plan(
        &self,
        plan: &ValidatedRemotePlan,
    ) -> Result<ApplyRemotePlanResult, SyncError> {
        db::apply_validated_remote_plan(plan).map_err(|_error| {
            log::error!("Applying remote synchronization plan failed");
            SyncError::local("Persisting downloaded synchronization data failed")
        })
    }

    fn commit_published(&self, commit: &PublishCommit) -> Result<usize, SyncError> {
        db::commit_published_revisions(commit).map_err(|_error| {
            log::error!("Committing published synchronization revisions failed");
            SyncError::local("Persisting synchronization status failed")
        })
    }
}

/// Compatibility seam retained exclusively for the v1 behavior engine/tests.
#[cfg(test)]
pub(crate) trait SyncStore {
    fn snapshot(&self) -> Result<Vec<Snippet>, SyncError>;
    fn merge(&self, snippets: Vec<Snippet>) -> Result<MergeResult, SyncError>;
}

#[cfg(test)]
impl ProductionStore {
    #[allow(dead_code)]
    pub(crate) fn record_v1_success(
        &self,
        total: usize,
        uploaded: usize,
        downloaded: usize,
        message: &str,
    ) -> Result<(), SyncError> {
        db::record_sync_version("merge", total, uploaded, downloaded, 0, 0, 1, 0, message).map_err(
            |error| {
                log::error!("Recording v1 synchronization history failed: {error}");
                SyncError::local("Recording synchronization history failed")
            },
        )
    }
}

#[cfg(test)]
impl SyncStore for ProductionStore {
    fn snapshot(&self) -> Result<Vec<Snippet>, SyncError> {
        db::get_all_for_upload().map_err(|error| {
            log::error!("Reading v1 synchronization snapshot failed: {error}");
            SyncError::local("Reading local synchronization data failed")
        })
    }

    fn merge(&self, snippets: Vec<Snippet>) -> Result<MergeResult, SyncError> {
        db::merge_sync_snippets(snippets).map_err(|error| {
            log::error!("Merging v1 synchronized snippets failed: {error}");
            SyncError::local("Persisting downloaded synchronization data failed")
        })
    }
}
