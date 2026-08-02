use super::error::SyncError;
use super::protocol::{canonical_snippet_bytes, Manifest, SnippetMeta};
use super::store::SyncStore;
use super::transport::{Clock, RemoteTransport};
use crate::db::Snippet;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub(crate) const MAX_CONVERGENCE_ROUNDS: usize = 4;
pub(crate) const MAX_REMOTE_REREAD_ROUNDS: usize = 2;
pub(crate) const DEFAULT_SYNC_DEADLINE: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReconcileDecision {
    Upload,
    Download,
    VerifyRemotePayload,
    NoChange,
    RejectRemote,
}

pub(crate) fn reconcile_decision(
    local_updated_at: Option<&str>,
    remote_updated_at: Option<&str>,
    remote_payload_present: Option<bool>,
) -> ReconcileDecision {
    match (local_updated_at, remote_updated_at) {
        (Some(_), None) => ReconcileDecision::Upload,
        (None, Some(remote)) if parse_timestamp(remote).is_none() => {
            ReconcileDecision::RejectRemote
        }
        (None, Some(_)) => ReconcileDecision::Download,
        (None, None) => ReconcileDecision::NoChange,
        (Some(local), Some(remote)) => {
            let (Some(local), Some(remote)) = (parse_timestamp(local), parse_timestamp(remote))
            else {
                return ReconcileDecision::RejectRemote;
            };
            if local > remote {
                ReconcileDecision::Upload
            } else if remote > local {
                ReconcileDecision::Download
            } else {
                match remote_payload_present {
                    Some(true) => ReconcileDecision::NoChange,
                    Some(false) => ReconcileDecision::Upload,
                    None => ReconcileDecision::VerifyRemotePayload,
                }
            }
        }
    }
}

fn parse_timestamp(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    chrono::DateTime::parse_from_rfc3339(value).ok()
}

fn timestamp_is_newer(left: &str, right: &str) -> bool {
    parse_timestamp(left).is_some()
        && parse_timestamp(right).is_some()
        && crate::db::timestamp_is_newer(left, right)
}

fn compare_snippet_content(left: &Snippet, right: &Snippet) -> Result<Ordering, SyncError> {
    let left = canonical_snippet_bytes(left)
        .map_err(|_| SyncError::validation("Snippet canonical serialization failed"))?;
    let right = canonical_snippet_bytes(right)
        .map_err(|_| SyncError::validation("Snippet canonical serialization failed"))?;
    Ok(left.cmp(&right))
}

fn validate_payload_for_meta(snippet: &Snippet, meta: &SnippetMeta) -> Result<(), SyncError> {
    if snippet.id != meta.id || snippet.updated_at != meta.updated_at {
        return Err(SyncError::validation(
            "Remote snippet does not match its manifest entry",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EngineResult {
    pub(crate) uploaded_count: usize,
    pub(crate) downloaded_count: usize,
    pub(crate) total_count: usize,
}

pub(crate) struct SyncEngine<'a, T, S, C> {
    transport: &'a T,
    store: &'a S,
    clock: &'a C,
    deadline: Instant,
}

impl<'a, T, S, C> SyncEngine<'a, T, S, C>
where
    T: RemoteTransport,
    S: SyncStore,
    C: Clock,
{
    pub(crate) fn new(transport: &'a T, store: &'a S, clock: &'a C, deadline: Instant) -> Self {
        Self {
            transport,
            store,
            clock,
            deadline,
        }
    }

    fn ensure_deadline(&self) -> Result<(), SyncError> {
        if self.clock.now() >= self.deadline {
            Err(SyncError::deadline())
        } else {
            Ok(())
        }
    }

    fn verify_published_state(&self, manifest: &Manifest) -> Result<bool, SyncError> {
        let Some(observed) = self.transport.get_manifest(self.deadline)? else {
            return Ok(false);
        };
        if observed.version_map() != manifest.version_map() {
            return Ok(false);
        }
        for meta in &observed.snippets {
            self.ensure_deadline()?;
            let Some(snippet) = self.transport.get_snippet(&meta.id, self.deadline)? else {
                return Ok(false);
            };
            validate_payload_for_meta(&snippet, meta)?;
        }
        Ok(true)
    }

    pub(crate) fn run(&self) -> Result<EngineResult, SyncError> {
        self.ensure_deadline()?;
        self.transport.ensure_collection(self.deadline)?;
        let local_before = self.store.snapshot()?;
        let local_map: HashMap<&str, &Snippet> = local_before
            .iter()
            .map(|snippet| (snippet.id.as_str(), snippet))
            .collect();
        let remote_manifest = self.transport.get_manifest(self.deadline)?;
        let remote_map = remote_manifest
            .as_ref()
            .map(Manifest::version_map)
            .unwrap_or_default();

        let mut downloaded_count = 0;
        let mut forced_uploads = HashMap::new();
        if let Some(manifest) = &remote_manifest {
            for meta in &manifest.snippets {
                self.ensure_deadline()?;
                let local = local_map.get(meta.id.as_str()).copied();
                let local_updated_at = local.map(|snippet| snippet.updated_at.as_str());
                match reconcile_decision(local_updated_at, Some(&meta.updated_at), None) {
                    ReconcileDecision::Download => {
                        if let Some(snippet) =
                            self.transport.get_snippet(&meta.id, self.deadline)?
                        {
                            validate_payload_for_meta(&snippet, meta)?;
                            let merge = self.store.merge(vec![snippet])?;
                            downloaded_count += merge.inserted + merge.updated;
                        }
                    }
                    ReconcileDecision::VerifyRemotePayload => {
                        let remote_present =
                            self.transport.snippet_exists(&meta.id, self.deadline)?;
                        let remote = if remote_present {
                            self.transport.get_snippet(&meta.id, self.deadline)?
                        } else {
                            None
                        };
                        let Some(remote) = remote else {
                            if remote_present {
                                return Err(SyncError::validation(
                                    "Remote snippet disappeared after existence verification",
                                ));
                            }
                            if let Some(local) = local {
                                forced_uploads.insert(meta.id.clone(), local.clone());
                            }
                            continue;
                        };
                        validate_payload_for_meta(&remote, meta)?;
                        if let Some(local) = local {
                            match compare_snippet_content(local, &remote)? {
                                Ordering::Less => {
                                    let merge = self.store.merge(vec![remote])?;
                                    downloaded_count += merge.inserted + merge.updated;
                                }
                                Ordering::Greater => {
                                    forced_uploads.insert(meta.id.clone(), local.clone());
                                }
                                Ordering::Equal => {}
                            }
                        }
                    }
                    ReconcileDecision::RejectRemote => {
                        return Err(SyncError::validation(
                            "Remote manifest contains invalid synchronization metadata",
                        ));
                    }
                    ReconcileDecision::Upload | ReconcileDecision::NoChange => {}
                }
            }
        }

        let mut uploaded_count = 0;
        let mut uploaded_versions: HashMap<String, String> = remote_map.clone();
        for snippet in &local_before {
            self.ensure_deadline()?;
            let should_upload = remote_map
                .get(&snippet.id)
                .map(|remote| timestamp_is_newer(&snippet.updated_at, remote))
                .unwrap_or(true)
                || forced_uploads.contains_key(&snippet.id);
            if should_upload {
                self.transport.put_snippet(snippet, self.deadline)?;
                uploaded_versions.insert(snippet.id.clone(), snippet.updated_at.clone());
                uploaded_count += 1;
            }
        }

        let mut final_snippets = self.store.snapshot()?;
        for round in 0..MAX_CONVERGENCE_ROUNDS {
            self.ensure_deadline()?;
            for snippet in &final_snippets {
                if uploaded_versions.get(&snippet.id) != Some(&snippet.updated_at) {
                    self.transport.put_snippet(snippet, self.deadline)?;
                    uploaded_versions.insert(snippet.id.clone(), snippet.updated_at.clone());
                    uploaded_count += 1;
                }
            }
            let next_snippets = self.store.snapshot()?;
            let stable = next_snippets.len() == final_snippets.len()
                && next_snippets
                    .iter()
                    .all(|snippet| uploaded_versions.get(&snippet.id) == Some(&snippet.updated_at));
            final_snippets = next_snippets;
            if stable {
                let manifest = Manifest::from_snippets(&final_snippets);
                self.transport.put_manifest(&manifest, self.deadline)?;
                let mut verified = false;
                for _round in 0..MAX_REMOTE_REREAD_ROUNDS {
                    self.ensure_deadline()?;
                    if self.verify_published_state(&manifest)? {
                        verified = true;
                        break;
                    }
                }
                if !verified {
                    return Err(SyncError::convergence_limit());
                }
                return Ok(EngineResult {
                    uploaded_count,
                    downloaded_count,
                    total_count: final_snippets.len(),
                });
            }
            if round + 1 == MAX_CONVERGENCE_ROUNDS {
                return Err(SyncError::convergence_limit());
            }
        }
        Err(SyncError::convergence_limit())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::MergeResult;
    use crate::webdav::protocol::SnippetMeta;
    use std::sync::Mutex;

    fn snippet(id: &str, updated_at: &str) -> Snippet {
        Snippet {
            id: id.into(),
            title: id.into(),
            content: "body".into(),
            language: "text".into(),
            description: String::new(),
            tags: Vec::new(),
            is_favorite: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: updated_at.into(),
            revision_id: String::new(),
        }
    }

    #[test]
    fn pure_reconcile_decisions_cover_v1_cases() {
        let old = "2026-01-01T00:00:00Z";
        let new = "2026-01-02T00:00:00Z";
        assert_eq!(
            reconcile_decision(Some(old), None, None),
            ReconcileDecision::Upload
        );
        assert_eq!(
            reconcile_decision(None, Some(old), None),
            ReconcileDecision::Download
        );
        assert_eq!(
            reconcile_decision(Some(old), Some(old), None),
            ReconcileDecision::VerifyRemotePayload
        );
        assert_eq!(
            reconcile_decision(Some(old), Some(old), Some(true)),
            ReconcileDecision::NoChange
        );
        assert_eq!(
            reconcile_decision(Some(old), Some(old), Some(false)),
            ReconcileDecision::Upload
        );
        assert_eq!(
            reconcile_decision(Some(new), Some(old), None),
            ReconcileDecision::Upload
        );
        assert_eq!(
            reconcile_decision(Some(old), Some(new), None),
            ReconcileDecision::Download
        );
        assert_eq!(
            reconcile_decision(Some("bad"), Some(new), None),
            ReconcileDecision::RejectRemote
        );
        assert_eq!(
            reconcile_decision(None, None, None),
            ReconcileDecision::NoChange
        );
    }

    #[test]
    fn v1_absence_never_decides_delete() {
        for local in [None, Some("2026-01-01T00:00:00Z")] {
            let decision = reconcile_decision(local, None, None);
            assert!(matches!(
                decision,
                ReconcileDecision::Upload | ReconcileDecision::NoChange
            ));
        }
    }

    struct FakeClock(Instant);

    impl FakeClock {
        fn new() -> Self {
            Self(Instant::now())
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Instant {
            self.0
        }

        fn sleep(&self, _duration: Duration) {}
    }

    struct FakeStore {
        snapshots: Mutex<Vec<Vec<Snippet>>>,
        current: Mutex<Vec<Snippet>>,
    }

    impl FakeStore {
        fn new(snapshots: Vec<Vec<Snippet>>) -> Self {
            let current = snapshots.first().cloned().unwrap_or_default();
            Self {
                snapshots: Mutex::new(snapshots),
                current: Mutex::new(current),
            }
        }
    }

    impl SyncStore for FakeStore {
        fn snapshot(&self) -> Result<Vec<Snippet>, SyncError> {
            let mut snapshots = self.snapshots.lock().unwrap();
            if snapshots.is_empty() {
                return Ok(self.current.lock().unwrap().clone());
            }
            let next = snapshots.remove(0);
            *self.current.lock().unwrap() = next.clone();
            Ok(next)
        }

        fn merge(&self, snippets: Vec<Snippet>) -> Result<MergeResult, SyncError> {
            let mut current = self.current.lock().unwrap();
            let mut inserted = 0;
            let mut updated = 0;
            for incoming in snippets {
                if let Some(existing) = current.iter_mut().find(|item| item.id == incoming.id) {
                    if incoming.updated_at == existing.updated_at
                        || timestamp_is_newer(&incoming.updated_at, &existing.updated_at)
                    {
                        *existing = incoming;
                        updated += 1;
                    }
                } else {
                    current.push(incoming);
                    inserted += 1;
                }
            }
            Ok(MergeResult {
                inserted,
                updated,
                skipped: 0,
                total: current.len(),
            })
        }
    }

    #[derive(Default)]
    struct FakeTransport {
        manifest: Mutex<Option<Manifest>>,
        snippets: Mutex<HashMap<String, Snippet>>,
        put_versions: Mutex<Vec<String>>,
    }

    impl RemoteTransport for FakeTransport {
        fn ensure_collection(&self, _deadline: Instant) -> Result<(), SyncError> {
            Ok(())
        }

        fn get_manifest(&self, _deadline: Instant) -> Result<Option<Manifest>, SyncError> {
            Ok(self.manifest.lock().unwrap().clone())
        }

        fn put_manifest(&self, manifest: &Manifest, _deadline: Instant) -> Result<(), SyncError> {
            *self.manifest.lock().unwrap() = Some(manifest.clone());
            Ok(())
        }

        fn get_snippet(&self, id: &str, _deadline: Instant) -> Result<Option<Snippet>, SyncError> {
            Ok(self.snippets.lock().unwrap().get(id).cloned())
        }

        fn put_snippet(&self, snippet: &Snippet, _deadline: Instant) -> Result<(), SyncError> {
            self.put_versions
                .lock()
                .unwrap()
                .push(snippet.updated_at.clone());
            self.snippets
                .lock()
                .unwrap()
                .insert(snippet.id.clone(), snippet.clone());
            Ok(())
        }

        fn snippet_exists(&self, id: &str, _deadline: Instant) -> Result<bool, SyncError> {
            Ok(self.snippets.lock().unwrap().contains_key(id))
        }
    }

    #[test]
    fn controlled_store_converges_after_multiple_local_versions() {
        let first = snippet("one", "2026-01-01T00:00:00Z");
        let second = snippet("one", "2026-01-02T00:00:00Z");
        let store = FakeStore::new(vec![
            vec![first.clone()],
            vec![second.clone()],
            vec![second.clone()],
        ]);
        let transport = FakeTransport::default();
        let clock = FakeClock::new();
        let result = SyncEngine::new(
            &transport,
            &store,
            &clock,
            clock.now() + DEFAULT_SYNC_DEADLINE,
        )
        .run()
        .unwrap();

        assert_eq!(result.uploaded_count, 2);
        assert_eq!(
            transport.put_versions.lock().unwrap().as_slice(),
            [first.updated_at, second.updated_at]
        );
        assert_eq!(
            transport
                .manifest
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .snippets,
            vec![SnippetMeta {
                id: "one".into(),
                updated_at: "2026-01-02T00:00:00Z".into(),
            }]
        );
    }

    #[test]
    fn changing_store_is_stopped_by_named_convergence_bound() {
        let snapshots = (0..=MAX_CONVERGENCE_ROUNDS + 2)
            .map(|round| {
                vec![snippet(
                    "one",
                    &format!("2026-01-{:02}T00:00:00Z", round + 1),
                )]
            })
            .collect();
        let store = FakeStore::new(snapshots);
        let transport = FakeTransport::default();
        let clock = FakeClock::new();
        let error = SyncEngine::new(
            &transport,
            &store,
            &clock,
            clock.now() + DEFAULT_SYNC_DEADLINE,
        )
        .run()
        .unwrap_err();
        assert!(error.retryable);
        assert_eq!(error.kind, super::super::error::SyncErrorKind::RetryLimit);
        assert!(transport.manifest.lock().unwrap().is_none());
    }
}
