use super::error::{SyncError, SyncErrorKind};
use super::protocol::{
    manifest_v2_hash, revision_object_hash, validate_parent_chain, ManifestDocument, ManifestV1,
    ManifestV2, ProtocolV2Marker, RevisionObjectV2, MAX_ANCESTRY_BYTES, MAX_ANCESTRY_OBJECTS,
    MAX_PARENT_CHAIN_DEPTH, V2_PROTOCOL_VERSION,
};
use super::store::V2SyncStore;
use super::transport::{
    require_strong_etag, CasOutcome, Clock, Precondition, RemoteTransport, ResourceState,
};
use crate::db::{
    ApplyRemotePlanResult, OutboxRevision, PublishCommit, RemotePlanEntry, RevisionHead, Snippet,
    StoredRevisionObject, SyncSnapshot, ValidatedRemotePlan,
};
use crate::revision::{
    canonical_live_payload, deterministic_legacy_revision_uuid, sha256_hex, wire_revision_uuid,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub(crate) const MAX_CAS_ROUNDS: usize = 4;
pub(crate) const DEFAULT_SYNC_DEADLINE: Duration = Duration::from_secs(5 * 60);
const LEGACY_BOOTSTRAP_DEVICE: &str = "00000000-0000-0000-0000-000000000001";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EngineResult {
    pub(crate) uploaded_count: usize,
    pub(crate) downloaded_count: usize,
    pub(crate) deleted_count: usize,
    pub(crate) conflict_count: usize,
    pub(crate) total_count: usize,
    pub(crate) generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum BootstrapState {
    Fresh,
    Legacy(ManifestV1),
    Ready(ManifestV2),
}

#[derive(Debug, Clone)]
struct ObservedManifest {
    state: BootstrapState,
    precondition: Precondition,
    etag: Option<String>,
    hash: Option<String>,
    marker_missing: bool,
}

#[derive(Debug, Clone, Default)]
struct RemoteGraph {
    heads: HashMap<String, RevisionObjectV2>,
    objects: HashMap<String, RevisionObjectV2>,
    downloaded_objects: usize,
}

#[derive(Debug, Clone, Default)]
struct ReconcilePlan {
    apply: Vec<RemotePlanEntry>,
    publish_heads: Vec<RevisionHead>,
    publish_objects: Vec<RevisionObjectV2>,
    acknowledge: Vec<String>,
    downloaded_count: usize,
    deleted_count: usize,
    conflict_count: usize,
}

pub(crate) struct V2SyncEngine<'a, T, S, C> {
    transport: &'a T,
    store: &'a S,
    clock: &'a C,
    remote_id: String,
    deadline: Instant,
}

impl<'a, T, S, C> V2SyncEngine<'a, T, S, C>
where
    T: RemoteTransport,
    S: V2SyncStore,
    C: Clock,
{
    pub(crate) fn new(
        transport: &'a T,
        store: &'a S,
        clock: &'a C,
        remote_id: String,
        deadline: Instant,
    ) -> Self {
        Self {
            transport,
            store,
            clock,
            remote_id,
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

    pub(crate) fn run(&self) -> Result<EngineResult, SyncError> {
        self.ensure_deadline()?;
        self.transport.ensure_collection(self.deadline)?;
        self.transport.ensure_objects_collection(self.deadline)?;

        // Owned snapshot: the DB mutex has been released before the first HTTP GET.
        let initial_snapshot = self.store.load_snapshot(&self.remote_id)?;
        let local_vault_id = self.store.load_vault_id(&self.remote_id)?;
        if initial_snapshot.pending_bytes > crate::db::MAX_PENDING_OUTBOX_BYTES {
            return Err(SyncError::outbox_full());
        }

        for round in 0..MAX_CAS_ROUNDS {
            self.ensure_deadline()?;
            let snapshot = if round == 0 {
                initial_snapshot.clone()
            } else {
                self.store.load_snapshot(&self.remote_id)?
            };
            let observed = self.observe_bootstrap_state(&snapshot, local_vault_id.as_deref())?;
            let vault_id = match &observed.state {
                BootstrapState::Ready(manifest) => manifest.vault_id.clone(),
                BootstrapState::Fresh | BootstrapState::Legacy(_) => local_vault_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            };

            let remote_graph = match &observed.state {
                BootstrapState::Fresh => RemoteGraph::default(),
                BootstrapState::Legacy(legacy) => self.load_legacy_graph_stable(legacy)?,
                BootstrapState::Ready(manifest) => self.load_remote_graph(manifest)?,
            };
            let plan = reconcile(&snapshot, &remote_graph)?;

            let apply_result = if plan.apply.is_empty() {
                ApplyRemotePlanResult {
                    applied: 0,
                    skipped: 0,
                    conflicts_created: 0,
                }
            } else {
                let generation = match &observed.state {
                    BootstrapState::Ready(manifest) => manifest.generation,
                    BootstrapState::Fresh | BootstrapState::Legacy(_) => 0,
                };
                self.store.apply_remote_plan(&ValidatedRemotePlan {
                    remote_id: self.remote_id.clone(),
                    protocol_version: V2_PROTOCOL_VERSION as i64,
                    generation: i64::try_from(generation)
                        .map_err(|_| SyncError::validation("Remote generation is too large"))?,
                    manifest_etag: observed.etag.clone(),
                    manifest_hash: observed.hash.clone(),
                    entries: plan.apply.clone(),
                })?
            };

            // If a local edit raced the remote apply, re-observe instead of
            // overwriting or publishing a stale head.
            if apply_result.skipped > 0 {
                if round + 1 == MAX_CAS_ROUNDS {
                    return Err(SyncError::cas_conflict());
                }
                continue;
            }

            let publish_snapshot = self.store.load_snapshot(&self.remote_id)?;
            let mut publish_plan = reconcile(&publish_snapshot, &remote_graph)?;
            if matches!(&observed.state, BootstrapState::Legacy(_)) {
                // Legacy payload files are read-only migration inputs. Their
                // deterministic revisions do not yet exist under objects/, so
                // publish every translated immutable object before the first v2
                // manifest references it. The legacy payload files stay intact.
                publish_plan
                    .publish_objects
                    .extend(remote_graph.objects.values().cloned());
            }
            // The pre-apply plan can contain a losing concurrent branch that is
            // no longer a head after applying the remote winner. Publish those
            // immutable revisions and acknowledge their exact outbox rows only
            // after the manifest is durable, preventing an endless pending
            // conflict without resurrecting the losing branch.
            publish_plan
                .publish_objects
                .extend(plan.publish_objects.iter().cloned());
            publish_plan
                .acknowledge
                .extend(plan.acknowledge.iter().cloned());
            publish_plan
                .publish_objects
                .sort_by(|left, right| left.revision_id.cmp(&right.revision_id));
            publish_plan
                .publish_objects
                .dedup_by(|left, right| left.revision_id == right.revision_id);
            publish_plan.acknowledge.sort();
            publish_plan.acknowledge.dedup();
            publish_plan.conflict_count = publish_plan
                .conflict_count
                .saturating_add(plan.conflict_count.max(apply_result.conflicts_created));
            publish_plan.downloaded_count = publish_plan
                .downloaded_count
                .saturating_add(plan.downloaded_count);
            publish_plan.deleted_count = publish_plan
                .deleted_count
                .saturating_add(plan.deleted_count);

            for object in &publish_plan.publish_objects {
                self.publish_immutable(object)?;
            }
            publish_plan
                .publish_objects
                .sort_by(|left, right| left.revision_id.cmp(&right.revision_id));

            let base_generation = match &observed.state {
                BootstrapState::Ready(manifest) => manifest.generation,
                BootstrapState::Fresh | BootstrapState::Legacy(_) => 0,
            };
            let next_generation = base_generation
                .checked_add(1)
                .ok_or_else(|| SyncError::validation("Remote generation overflow"))?;
            let manifest = ManifestV2::new(
                vault_id.clone(),
                next_generation,
                &publish_plan.publish_heads,
            );
            let manifest_hash = manifest_v2_hash(&manifest).map_err(SyncError::from)?;

            match self.transport.put_manifest_v2_conditional(
                &manifest,
                &observed.precondition,
                self.deadline,
            )? {
                CasOutcome::PreconditionFailed => {
                    if round + 1 == MAX_CAS_ROUNDS {
                        return Err(SyncError::cas_conflict());
                    }
                    continue;
                }
                CasOutcome::PreconditionRequired => {
                    return Err(SyncError::configuration(
                        "WebDAV server did not accept conditional manifest publication",
                    ));
                }
                CasOutcome::Published { .. } => {}
            }

            // The marker is only an upgrade discriminator. It intentionally has
            // no generation/hash and is created after the v2 manifest is durable.
            let marker = ProtocolV2Marker {
                version: V2_PROTOCOL_VERSION,
                vault_id: vault_id.clone(),
            };
            if observed.marker_missing {
                match self.transport.put_marker_conditional(
                    &marker,
                    &Precondition::Create,
                    self.deadline,
                )? {
                    CasOutcome::Published { .. } | CasOutcome::PreconditionFailed => {}
                    CasOutcome::PreconditionRequired => {
                        return Err(SyncError::configuration(
                            "WebDAV server did not accept conditional protocol marker publication",
                        ));
                    }
                }
            }

            // Never trust an absent/weak PUT response ETag. Re-read the exact
            // manifest and marker, validate bytes, and require a strong ETag.
            let published = self.verify_publication(&manifest, &marker, &manifest_hash)?;
            let current_snapshot = self.store.load_snapshot(&self.remote_id)?;
            let acknowledged = exact_acknowledgements(&current_snapshot, &publish_plan.acknowledge);
            let downloaded_count = publish_plan.downloaded_count;
            let deleted_count = publish_plan.deleted_count;
            let conflict_count = publish_plan.conflict_count;
            let uploaded_count = acknowledged.len();
            let total_count = current_snapshot.snippets.len();
            let succeeded_at = chrono::Utc::now().to_rfc3339();
            let message = format!(
                "同步完成：上传 {uploaded_count} 条，下载 {downloaded_count} 条，删除 {deleted_count} 条，冲突副本 {conflict_count} 条（当前共 {total_count} 条）"
            );
            self.store.commit_published(&PublishCommit {
                remote_id: self.remote_id.clone(),
                vault_id: vault_id.clone(),
                protocol_version: V2_PROTOCOL_VERSION as i64,
                manifest_etag: Some(published),
                manifest_hash: Some(manifest_hash),
                generation: i64::try_from(next_generation)
                    .map_err(|_| SyncError::validation("Remote generation is too large"))?,
                acknowledged_revision_ids: acknowledged,
                snippet_count: total_count,
                uploaded_count,
                downloaded_count,
                deleted_count,
                conflict_count,
                message,
                succeeded_at,
            })?;

            return Ok(EngineResult {
                uploaded_count,
                downloaded_count,
                deleted_count,
                conflict_count,
                total_count,
                generation: next_generation,
            });
        }
        Err(SyncError::cas_conflict())
    }

    fn observe_bootstrap_state(
        &self,
        snapshot: &SyncSnapshot,
        local_vault_id: Option<&str>,
    ) -> Result<ObservedManifest, SyncError> {
        let marker = self.transport.get_marker(self.deadline)?;
        let manifest = self.transport.get_manifest_document(self.deadline)?;
        let has_committed_v2 = snapshot.remote.as_ref().is_some_and(|state| {
            state.protocol_version >= V2_PROTOCOL_VERSION as i64 && state.bootstrap_state == "ready"
        }) || local_vault_id.is_some();
        match (marker, manifest) {
            (ResourceState::Missing, ResourceState::Missing) if has_committed_v2 => Err(
                SyncError::validation("Committed WebDAV v2 vault is missing remotely"),
            ),
            (ResourceState::Missing, ResourceState::Missing) => Ok(ObservedManifest {
                state: BootstrapState::Fresh,
                precondition: Precondition::Create,
                etag: None,
                hash: None,
                marker_missing: true,
            }),
            (ResourceState::Missing, ResourceState::Present(resource)) => match resource.value {
                ManifestDocument::V1(_) if has_committed_v2 => Err(SyncError::validation(
                    "Committed WebDAV v2 vault was replaced by a legacy manifest",
                )),
                ManifestDocument::V1(manifest) => Ok(ObservedManifest {
                    state: BootstrapState::Legacy(manifest),
                    precondition: Precondition::Match(require_strong_etag(
                        resource.etag.as_deref(),
                    )?),
                    etag: resource.etag,
                    hash: Some(resource.body_hash),
                    marker_missing: true,
                }),
                ManifestDocument::V2(manifest) => {
                    if let Some(expected) = local_vault_id {
                        if expected != manifest.vault_id {
                            return Err(SyncError::validation(
                                "Remote WebDAV v2 vault identity changed",
                            ));
                        }
                    }
                    Ok(ObservedManifest {
                        state: BootstrapState::Ready(manifest),
                        precondition: Precondition::Match(require_strong_etag(
                            resource.etag.as_deref(),
                        )?),
                        etag: resource.etag,
                        hash: Some(resource.body_hash),
                        marker_missing: true,
                    })
                }
            },
            (ResourceState::Present(_), ResourceState::Missing) => Err(SyncError::validation(
                "Remote protocol marker exists without a manifest",
            )),
            (ResourceState::Present(marker), ResourceState::Present(resource)) => {
                let ManifestDocument::V2(manifest) = resource.value else {
                    return Err(SyncError::validation(
                        "Remote protocol marker conflicts with a legacy manifest",
                    ));
                };
                if marker.value.vault_id != manifest.vault_id {
                    return Err(SyncError::validation(
                        "Remote protocol marker and manifest vault identifiers differ",
                    ));
                }
                if let Some(expected) = local_vault_id {
                    if expected != manifest.vault_id {
                        return Err(SyncError::validation(
                            "Remote WebDAV v2 vault identity changed",
                        ));
                    }
                }
                Ok(ObservedManifest {
                    state: BootstrapState::Ready(manifest),
                    precondition: Precondition::Match(require_strong_etag(
                        resource.etag.as_deref(),
                    )?),
                    etag: resource.etag,
                    hash: Some(resource.body_hash),
                    marker_missing: false,
                })
            }
        }
    }

    fn load_legacy_graph_stable(&self, manifest: &ManifestV1) -> Result<RemoteGraph, SyncError> {
        let ResourceState::Present(before) = self.transport.get_manifest_document(self.deadline)?
        else {
            return Err(SyncError::legacy_changed());
        };
        let ManifestDocument::V1(observed_before) = before.value else {
            return Err(SyncError::legacy_changed());
        };
        let before_etag = require_strong_etag(before.etag.as_deref())?;
        if observed_before != *manifest {
            return Err(SyncError::legacy_changed());
        }

        let graph = self.load_legacy_graph(manifest)?;
        let ResourceState::Present(reread) = self.transport.get_manifest_document(self.deadline)?
        else {
            return Err(SyncError::legacy_changed());
        };
        let ManifestDocument::V1(observed) = reread.value else {
            return Err(SyncError::legacy_changed());
        };
        let reread_etag = require_strong_etag(reread.etag.as_deref())?;
        if observed != *manifest
            || reread.body_hash != before.body_hash
            || reread_etag != before_etag
        {
            return Err(SyncError::legacy_changed());
        }
        Ok(graph)
    }

    fn load_legacy_graph(&self, manifest: &ManifestV1) -> Result<RemoteGraph, SyncError> {
        let mut graph = RemoteGraph::default();
        for meta in &manifest.snippets {
            self.ensure_deadline()?;
            let snippet = self
                .transport
                .get_snippet(&meta.id, self.deadline)?
                .ok_or_else(SyncError::legacy_changed)?;
            if snippet.id != meta.id || snippet.updated_at != meta.updated_at {
                return Err(SyncError::legacy_changed());
            }
            let content_hash = sha256_hex(
                canonical_live_payload(&snippet)
                    .map_err(SyncError::from)?
                    .as_bytes(),
            );
            let revision_id =
                deterministic_legacy_revision_uuid(&snippet.id, &content_hash, &snippet.updated_at);
            let head = RevisionHead {
                snippet_id: snippet.id.clone(),
                revision_id,
                parent_revision_id: None,
                device_id: LEGACY_BOOTSTRAP_DEVICE.into(),
                content_hash,
                revision_time: snippet.updated_at.clone(),
                deleted: false,
            };
            let object =
                RevisionObjectV2::from_head(&head, Some(&snippet)).map_err(SyncError::from)?;
            graph
                .objects
                .insert(object.revision_id.clone(), object.clone());
            graph.heads.insert(snippet.id.clone(), object);
            graph.downloaded_objects += 1;
        }
        Ok(graph)
    }

    fn load_remote_graph(&self, manifest: &ManifestV2) -> Result<RemoteGraph, SyncError> {
        let mut graph = RemoteGraph::default();
        let mut ancestry_objects = 0_usize;
        let mut ancestry_bytes = 0_usize;
        for entry in &manifest.entries {
            self.ensure_deadline()?;
            let mut current = Some(entry.head_revision_id.clone());
            let mut chain = HashMap::new();
            while let Some(revision_id) = current {
                if chain.contains_key(&revision_id) {
                    return Err(SyncError::validation(
                        "Remote revision ancestry contains a cycle",
                    ));
                }
                if chain.len() >= MAX_PARENT_CHAIN_DEPTH {
                    return Err(SyncError::validation(
                        "Remote revision ancestry exceeds its traversal limit",
                    ));
                }
                if let Some(known) = graph.objects.get(&revision_id) {
                    if known.snippet_id != entry.snippet_id {
                        return Err(SyncError::validation(
                            "Remote revision ancestry crosses snippet histories",
                        ));
                    }
                    current = known.parent_revision_id.clone();
                    chain.insert(revision_id, known.clone());
                    continue;
                }
                let resource = match self.transport.get_revision(&revision_id, self.deadline)? {
                    ResourceState::Present(resource) => resource,
                    ResourceState::Missing => {
                        return Err(SyncError::validation(
                            "Remote revision ancestry is missing an immutable object",
                        ))
                    }
                };
                if resource.value.snippet_id != entry.snippet_id {
                    return Err(SyncError::validation(
                        "Remote manifest head belongs to a different snippet",
                    ));
                }
                ancestry_objects = ancestry_objects.checked_add(1).ok_or_else(|| {
                    SyncError::validation("Remote revision ancestry exceeds its traversal limit")
                })?;
                ancestry_bytes =
                    ancestry_bytes
                        .checked_add(resource.body_bytes)
                        .ok_or_else(|| {
                            SyncError::validation("Remote revision ancestry is too large")
                        })?;
                if ancestry_objects > MAX_ANCESTRY_OBJECTS || ancestry_bytes > MAX_ANCESTRY_BYTES {
                    return Err(SyncError::validation(
                        "Remote revision ancestry exceeds its aggregate safety limit",
                    ));
                }
                let object = resource.value;
                current = object.parent_revision_id.clone();
                graph.downloaded_objects += 1;
                chain.insert(revision_id, object);
            }
            validate_parent_chain(&entry.head_revision_id, &chain)?;
            let head = chain
                .get(&entry.head_revision_id)
                .expect("validated head object")
                .clone();
            graph.heads.insert(entry.snippet_id.clone(), head);
            graph.objects.extend(chain);
        }
        Ok(graph)
    }

    fn publish_immutable(&self, object: &RevisionObjectV2) -> Result<(), SyncError> {
        match self.transport.put_revision_immutable(object, self.deadline) {
            Ok(()) => Ok(()),
            Err(error) if error.retryable && error.kind == SyncErrorKind::Network => {
                // A lost PUT response is ambiguous. Verify the exact immutable
                // object; never issue a blind second write.
                let expected_hash = revision_object_hash(object).map_err(SyncError::from)?;
                match self
                    .transport
                    .get_revision(&object.revision_id, self.deadline)?
                {
                    ResourceState::Present(resource) if resource.body_hash == expected_hash => {
                        Ok(())
                    }
                    ResourceState::Present(_) => Err(SyncError::validation(
                        "Remote immutable revision identifier collision detected",
                    )),
                    ResourceState::Missing => Err(error),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn verify_publication(
        &self,
        expected: &ManifestV2,
        marker: &ProtocolV2Marker,
        expected_hash: &str,
    ) -> Result<String, SyncError> {
        let ResourceState::Present(manifest) =
            self.transport.get_manifest_document(self.deadline)?
        else {
            return Err(SyncError::cas_conflict());
        };
        let ManifestDocument::V2(actual) = manifest.value else {
            return Err(SyncError::cas_conflict());
        };
        if actual != *expected || manifest.body_hash != expected_hash {
            return Err(SyncError::cas_conflict());
        }
        let etag = require_strong_etag(manifest.etag.as_deref())?;
        let ResourceState::Present(actual_marker) = self.transport.get_marker(self.deadline)?
        else {
            return Err(SyncError::cas_conflict());
        };
        if actual_marker.value != *marker {
            return Err(SyncError::cas_conflict());
        }
        Ok(etag)
    }
}

fn object_to_head(object: &RevisionObjectV2) -> RevisionHead {
    RevisionHead {
        snippet_id: object.snippet_id.clone(),
        revision_id: object.revision_id.clone(),
        parent_revision_id: object.parent_revision_id.clone(),
        device_id: object.device_id.clone(),
        content_hash: object.content_hash.clone(),
        revision_time: object.changed_at.clone(),
        deleted: object.deleted,
    }
}

fn object_to_plan(
    object: &RevisionObjectV2,
    expected_local_revision_id: Option<String>,
    preserve_local_as_conflict: bool,
) -> RemotePlanEntry {
    RemotePlanEntry {
        snippet_id: object.snippet_id.clone(),
        revision_id: object.revision_id.clone(),
        parent_revision_id: object.parent_revision_id.clone(),
        device_id: object.device_id.clone(),
        content_hash: object.content_hash.clone(),
        revision_time: object.changed_at.clone(),
        deleted: object.deleted,
        snippet: object.snippet.clone().map(Into::into),
        expected_local_revision_id,
        preserve_local_as_conflict,
    }
}

fn local_wire_objects(
    snapshot: &SyncSnapshot,
) -> Result<HashMap<String, RevisionObjectV2>, SyncError> {
    let mut objects = HashMap::new();
    for stored in &snapshot.revision_objects {
        let object = RevisionObjectV2::from_stored(stored).map_err(SyncError::from)?;
        if objects.insert(object.revision_id.clone(), object).is_some() {
            return Err(SyncError::validation(
                "Local revision ancestry contains duplicate immutable objects",
            ));
        }
    }
    Ok(objects)
}

fn is_ancestor(
    ancestor: &str,
    descendant: &str,
    objects: &HashMap<String, RevisionObjectV2>,
) -> Result<bool, SyncError> {
    let chain = validate_parent_chain(descendant, objects)?;
    Ok(chain.iter().skip(1).any(|revision| revision == ancestor))
}

fn local_object(
    head: &RevisionHead,
    snippets: &HashMap<&str, &Snippet>,
    pending: &HashMap<&str, &OutboxRevision>,
    stored: &HashMap<&str, &StoredRevisionObject>,
) -> Result<RevisionObjectV2, SyncError> {
    if let Some(outbox) = pending.get(head.revision_id.as_str()) {
        return RevisionObjectV2::from_outbox(outbox).map_err(SyncError::from);
    }
    if let Some(object) = stored.get(head.revision_id.as_str()) {
        return RevisionObjectV2::from_stored(object).map_err(SyncError::from);
    }
    RevisionObjectV2::from_head(head, snippets.get(head.snippet_id.as_str()).copied())
        .map_err(SyncError::from)
}

fn local_publish_objects(
    head: &RevisionHead,
    snippets: &HashMap<&str, &Snippet>,
    pending: &HashMap<&str, &OutboxRevision>,
    stored: &HashMap<&str, &StoredRevisionObject>,
    remote_objects: &HashMap<String, RevisionObjectV2>,
) -> Result<Vec<RevisionObjectV2>, SyncError> {
    let mut objects = Vec::new();
    let mut current = Some(head.revision_id.clone());
    let mut seen = HashSet::new();
    while let Some(revision_id) = current {
        if !seen.insert(revision_id.clone()) {
            return Err(SyncError::validation(
                "Local pending revision ancestry contains a cycle",
            ));
        }
        if remote_objects.contains_key(&wire_revision_uuid(&revision_id)) {
            break;
        }
        let (object, parent) = if let Some(outbox) = pending.get(revision_id.as_str()) {
            (
                RevisionObjectV2::from_outbox(outbox).map_err(SyncError::from)?,
                outbox.parent_revision_id.clone(),
            )
        } else if let Some(stored_object) = stored.get(revision_id.as_str()) {
            (
                RevisionObjectV2::from_stored(stored_object).map_err(SyncError::from)?,
                stored_object.parent_revision_id.clone(),
            )
        } else if revision_id == head.revision_id {
            (
                local_object(head, snippets, pending, stored)?,
                head.parent_revision_id.clone(),
            )
        } else {
            return Err(SyncError::validation(
                "Local revision ancestry is missing a durable immutable object",
            ));
        };
        let parent_available = parent.as_ref().is_some_and(|parent| {
            remote_objects.contains_key(&wire_revision_uuid(parent))
                || pending.contains_key(parent.as_str())
                || stored.contains_key(parent.as_str())
        });
        if parent.is_some() && !parent_available {
            return Err(SyncError::validation(
                "Local revision ancestry is missing a durable parent object",
            ));
        }
        current = parent;
        objects.push(object);
    }
    objects.reverse();
    Ok(objects)
}

fn pending_revision_ids(
    objects: &[RevisionObjectV2],
    pending: &HashMap<&str, &OutboxRevision>,
) -> Vec<String> {
    objects
        .iter()
        .filter_map(|object| {
            pending
                .values()
                .find(|revision| wire_revision_uuid(&revision.revision_id) == object.revision_id)
                .map(|revision| revision.revision_id.clone())
        })
        .collect()
}

fn reconcile(snapshot: &SyncSnapshot, remote: &RemoteGraph) -> Result<ReconcilePlan, SyncError> {
    let snippets = snapshot
        .snippets
        .iter()
        .map(|snippet| (snippet.id.as_str(), snippet))
        .collect::<HashMap<_, _>>();
    let pending = snapshot
        .pending
        .iter()
        .map(|revision| (revision.revision_id.as_str(), revision))
        .collect::<HashMap<_, _>>();
    let stored = snapshot
        .revision_objects
        .iter()
        .map(|revision| (revision.revision_id.as_str(), revision))
        .collect::<HashMap<_, _>>();
    let mut ancestry = remote.objects.clone();
    for (revision_id, object) in local_wire_objects(snapshot)? {
        if let Some(remote_object) = ancestry.get(&revision_id) {
            if remote_object != &object {
                return Err(SyncError::validation(
                    "Local and remote immutable revision objects disagree",
                ));
            }
        } else {
            ancestry.insert(revision_id, object);
        }
    }
    let local_heads = snapshot
        .heads
        .iter()
        .map(|head| (head.snippet_id.as_str(), head))
        .collect::<HashMap<_, _>>();
    let mut snippet_ids = local_heads
        .keys()
        .map(|value| (*value).to_string())
        .collect::<HashSet<_>>();
    snippet_ids.extend(remote.heads.keys().cloned());
    let mut snippet_ids = snippet_ids.into_iter().collect::<Vec<_>>();
    snippet_ids.sort();

    let mut plan = ReconcilePlan::default();
    for snippet_id in snippet_ids {
        let local = local_heads.get(snippet_id.as_str()).copied();
        let remote_head = remote.heads.get(&snippet_id);
        match (local, remote_head) {
            (Some(local), None) => {
                let objects =
                    local_publish_objects(local, &snippets, &pending, &stored, &remote.objects)?;
                plan.acknowledge
                    .extend(pending_revision_ids(&objects, &pending));
                plan.publish_objects.extend(objects);
                plan.publish_heads.push(local.clone());
            }
            (None, Some(remote_head)) => {
                plan.apply.push(object_to_plan(remote_head, None, false));
                plan.publish_heads.push(object_to_head(remote_head));
                plan.downloaded_count += usize::from(!remote_head.deleted);
                plan.deleted_count += usize::from(remote_head.deleted);
            }
            (Some(local), Some(remote_head))
                if wire_revision_uuid(&local.revision_id) == remote_head.revision_id =>
            {
                plan.publish_heads.push(object_to_head(remote_head));
                let objects =
                    local_publish_objects(local, &snippets, &pending, &stored, &remote.objects)?;
                plan.acknowledge
                    .extend(pending_revision_ids(&objects, &pending));
            }
            (Some(local), Some(remote_head)) => {
                let local_wire = wire_revision_uuid(&local.revision_id);
                let local_is_ancestor =
                    is_ancestor(&local_wire, &remote_head.revision_id, &remote.objects)?;
                if local_is_ancestor {
                    plan.apply.push(object_to_plan(
                        remote_head,
                        Some(local.revision_id.clone()),
                        false,
                    ));
                    plan.publish_heads.push(object_to_head(remote_head));
                    plan.downloaded_count += usize::from(!remote_head.deleted);
                    plan.deleted_count += usize::from(remote_head.deleted);
                    continue;
                }

                let remote_is_ancestor = ancestry.contains_key(&local_wire)
                    && is_ancestor(&remote_head.revision_id, &local_wire, &ancestry)?;
                if remote_is_ancestor {
                    let objects = local_publish_objects(
                        local,
                        &snippets,
                        &pending,
                        &stored,
                        &remote.objects,
                    )?;
                    plan.acknowledge
                        .extend(pending_revision_ids(&objects, &pending));
                    plan.publish_objects.extend(objects);
                    plan.publish_heads.push(local.clone());
                    continue;
                }

                // Concurrent branches always preserve the already-published
                // remote head as the original. The losing local live branch is
                // copied transactionally; a losing tombstone remains durable.
                let local_objects =
                    local_publish_objects(local, &snippets, &pending, &stored, &remote.objects)?;
                plan.acknowledge
                    .extend(pending_revision_ids(&local_objects, &pending));
                plan.publish_objects.extend(local_objects);
                plan.apply.push(object_to_plan(
                    remote_head,
                    Some(local.revision_id.clone()),
                    !local.deleted,
                ));
                plan.publish_heads.push(object_to_head(remote_head));
                plan.downloaded_count += usize::from(!remote_head.deleted);
                plan.deleted_count += usize::from(remote_head.deleted);
                plan.conflict_count += 1;
            }
            (None, None) => unreachable!(),
        }
    }
    // Every durable pending revision is an immutable publish candidate, even if
    // a remote winner made it unreachable from the current local heads during
    // an earlier interrupted CAS round. This keeps crash/retry behavior
    // convergent without making the losing branch a manifest head.
    for revision in &snapshot.pending {
        let object = RevisionObjectV2::from_outbox(revision).map_err(SyncError::from)?;
        if ancestry
            .get(&object.revision_id)
            .is_some_and(|durable| durable != &object)
        {
            return Err(SyncError::validation(
                "Pending and durable immutable revision objects disagree",
            ));
        }
        if object.parent_revision_id.as_ref().is_some_and(|parent| {
            !ancestry.contains_key(parent) && !remote.objects.contains_key(parent)
        }) {
            return Err(SyncError::validation(
                "Pending revision ancestry is missing a durable parent object",
            ));
        }
        plan.publish_objects.push(object);
        plan.acknowledge.push(revision.revision_id.clone());
    }
    plan.publish_heads
        .sort_by(|left, right| left.snippet_id.cmp(&right.snippet_id));
    plan.publish_objects
        .sort_by(|left, right| left.revision_id.cmp(&right.revision_id));
    plan.publish_objects
        .dedup_by(|left, right| left.revision_id == right.revision_id);
    plan.acknowledge.sort();
    plan.acknowledge.dedup();
    Ok(plan)
}

fn exact_acknowledgements(snapshot: &SyncSnapshot, candidates: &[String]) -> Vec<String> {
    let pending = snapshot
        .pending
        .iter()
        .map(|revision| revision.revision_id.as_str())
        .collect::<HashSet<_>>();
    let mut exact = candidates
        .iter()
        .filter(|revision| pending.contains(revision.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    exact.sort();
    exact.dedup();
    exact
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::revision::{canonical_live_payload, sha256_hex};

    fn snippet(id: &str, revision_id: &str, updated_at: &str, title: &str) -> Snippet {
        Snippet {
            id: id.into(),
            title: title.into(),
            content: "body".into(),
            language: "text".into(),
            description: String::new(),
            tags: Vec::new(),
            is_favorite: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: updated_at.into(),
            revision_id: revision_id.into(),
        }
    }

    fn stored_revision(revision: &OutboxRevision) -> StoredRevisionObject {
        StoredRevisionObject {
            revision_id: revision.revision_id.clone(),
            snippet_id: revision.snippet_id.clone(),
            parent_revision_id: revision.parent_revision_id.clone(),
            device_id: revision.device_id.clone(),
            content_hash: revision.content_hash.clone(),
            revision_time: revision.revision_time.clone(),
            deleted: revision.deleted,
            origin: revision.origin.clone(),
            payload_json: revision.payload_json.clone(),
            payload_bytes: revision.payload_bytes,
            conflict_of: revision.conflict_of.clone(),
        }
    }

    fn snapshot(item: &Snippet) -> SyncSnapshot {
        let payload = canonical_live_payload(item).unwrap();
        let head = RevisionHead {
            snippet_id: item.id.clone(),
            revision_id: item.revision_id.clone(),
            parent_revision_id: None,
            device_id: uuid::Uuid::new_v4().to_string(),
            content_hash: sha256_hex(payload.as_bytes()),
            revision_time: item.updated_at.clone(),
            deleted: false,
        };
        let pending = OutboxRevision {
            sequence: 1,
            revision_id: head.revision_id.clone(),
            snippet_id: head.snippet_id.clone(),
            parent_revision_id: None,
            device_id: head.device_id.clone(),
            content_hash: head.content_hash.clone(),
            revision_time: head.revision_time.clone(),
            deleted: false,
            operation_kind: "upsert".into(),
            origin: "local".into(),
            payload_bytes: payload.len(),
            payload_json: payload,
            conflict_of: None,
        };
        SyncSnapshot {
            device_id: uuid::Uuid::new_v4().to_string(),
            snippets: vec![item.clone()],
            heads: vec![head],
            pending: vec![pending.clone()],
            revision_objects: vec![stored_revision(&pending)],
            pending_bytes: pending.payload_bytes,
            remote: None,
        }
    }

    fn tombstone_snapshot(
        snippet_id: &str,
        revision_id: &str,
        parent_revision_id: Option<String>,
        changed_at: &str,
    ) -> SyncSnapshot {
        let payload = crate::revision::canonical_tombstone_payload(snippet_id, changed_at).unwrap();
        let revision = OutboxRevision {
            sequence: 1,
            revision_id: revision_id.into(),
            snippet_id: snippet_id.into(),
            parent_revision_id: parent_revision_id.clone(),
            device_id: uuid::Uuid::new_v4().to_string(),
            content_hash: sha256_hex(payload.as_bytes()),
            revision_time: changed_at.into(),
            deleted: true,
            operation_kind: "delete".into(),
            origin: "local".into(),
            payload_json: payload,
            payload_bytes: 0,
            conflict_of: None,
        };
        let mut revision = revision;
        revision.payload_bytes = revision.payload_json.len();
        SyncSnapshot {
            device_id: uuid::Uuid::new_v4().to_string(),
            snippets: Vec::new(),
            heads: vec![RevisionHead {
                snippet_id: snippet_id.into(),
                revision_id: revision_id.into(),
                parent_revision_id,
                device_id: revision.device_id.clone(),
                content_hash: revision.content_hash.clone(),
                revision_time: changed_at.into(),
                deleted: true,
            }],
            pending: vec![revision.clone()],
            revision_objects: vec![stored_revision(&revision)],
            pending_bytes: revision.payload_bytes,
            remote: None,
        }
    }

    fn remote_live(item: &Snippet, parent_revision_id: Option<String>) -> RevisionObjectV2 {
        let payload = canonical_live_payload(item).unwrap();
        RevisionObjectV2 {
            version: V2_PROTOCOL_VERSION,
            revision_id: wire_revision_uuid(&item.revision_id),
            parent_revision_id: parent_revision_id.as_deref().map(wire_revision_uuid),
            snippet_id: item.id.clone(),
            device_id: uuid::Uuid::new_v4().to_string(),
            changed_at: item.updated_at.clone(),
            deleted: false,
            content_hash: sha256_hex(payload.as_bytes()),
            conflict_of: None,
            snippet: Some(item.into()),
        }
    }

    fn remote_tombstone(
        snippet_id: &str,
        revision_id: &str,
        parent_revision_id: Option<String>,
        changed_at: &str,
    ) -> RevisionObjectV2 {
        RevisionObjectV2 {
            version: V2_PROTOCOL_VERSION,
            revision_id: wire_revision_uuid(revision_id),
            parent_revision_id: parent_revision_id.as_deref().map(wire_revision_uuid),
            snippet_id: snippet_id.into(),
            device_id: uuid::Uuid::new_v4().to_string(),
            changed_at: changed_at.into(),
            deleted: true,
            content_hash: sha256_hex(
                crate::revision::canonical_tombstone_payload(snippet_id, changed_at)
                    .unwrap()
                    .as_bytes(),
            ),
            conflict_of: None,
            snippet: None,
        }
    }

    fn graph(head: RevisionObjectV2, ancestry: Vec<RevisionObjectV2>) -> RemoteGraph {
        let mut objects = ancestry
            .into_iter()
            .map(|object| (object.revision_id.clone(), object))
            .collect::<HashMap<_, _>>();
        objects.insert(head.revision_id.clone(), head.clone());
        RemoteGraph {
            heads: HashMap::from([(head.snippet_id.clone(), head)]),
            objects,
            downloaded_objects: 1,
        }
    }

    fn snapshot_with_pending_chain(items: &[Snippet]) -> SyncSnapshot {
        let mut snapshot = snapshot(&items[0]);
        snapshot.snippets = vec![items.last().unwrap().clone()];
        snapshot.heads.clear();
        snapshot.pending.clear();
        snapshot.revision_objects.clear();
        snapshot.pending_bytes = 0;
        let mut parent_revision_id: Option<String> = None;
        for (index, item) in items.iter().enumerate() {
            let payload = canonical_live_payload(item).unwrap();
            let head = RevisionHead {
                snippet_id: item.id.clone(),
                revision_id: item.revision_id.clone(),
                parent_revision_id: parent_revision_id.clone(),
                device_id: snapshot.device_id.clone(),
                content_hash: sha256_hex(payload.as_bytes()),
                revision_time: item.updated_at.clone(),
                deleted: false,
            };
            snapshot.pending_bytes += payload.len();
            let revision = OutboxRevision {
                sequence: i64::try_from(index + 1).unwrap(),
                revision_id: item.revision_id.clone(),
                snippet_id: item.id.clone(),
                parent_revision_id: parent_revision_id.clone(),
                device_id: snapshot.device_id.clone(),
                content_hash: head.content_hash.clone(),
                revision_time: item.updated_at.clone(),
                deleted: false,
                operation_kind: "upsert".into(),
                origin: "local".into(),
                payload_bytes: payload.len(),
                payload_json: payload,
                conflict_of: None,
            };
            snapshot.revision_objects.push(stored_revision(&revision));
            snapshot.pending.push(revision);
            if index + 1 == items.len() {
                snapshot.heads.push(head);
            }
            parent_revision_id = Some(item.revision_id.clone());
        }
        snapshot
    }

    #[test]
    fn fresh_local_snapshot_publishes_only_exact_pending_revision() {
        let local = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-02T00:00:00Z",
            "local",
        );
        let snapshot = snapshot(&local);
        let plan = reconcile(&snapshot, &RemoteGraph::default()).unwrap();
        assert_eq!(plan.publish_heads.len(), 1);
        assert_eq!(plan.publish_objects.len(), 1);
        assert_eq!(plan.acknowledge, vec![local.revision_id]);
    }

    #[test]
    fn publishes_pending_ancestry_before_the_current_head() {
        let first = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-02T00:00:00Z",
            "first",
        );
        let second = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-03T00:00:00Z",
            "second",
        );
        let snapshot = snapshot_with_pending_chain(&[first.clone(), second.clone()]);
        let plan = reconcile(&snapshot, &RemoteGraph::default()).unwrap();

        let mut published = plan
            .publish_objects
            .iter()
            .map(|object| object.revision_id.clone())
            .collect::<Vec<_>>();
        published.sort();
        let mut expected_published = vec![
            wire_revision_uuid(&first.revision_id),
            wire_revision_uuid(&second.revision_id),
        ];
        expected_published.sort();
        assert_eq!(published, expected_published);
        let mut expected_acknowledgements = vec![first.revision_id, second.revision_id];
        expected_acknowledgements.sort();
        assert_eq!(plan.acknowledge, expected_acknowledgements);
    }

    #[test]
    fn orphaned_pending_losing_branch_remains_publishable_after_remote_apply() {
        let local = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-02T00:00:00Z",
            "local",
        );
        let mut snapshot = snapshot(&local);
        let remote = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-03T00:00:00Z",
            "remote",
        );
        let remote_object = remote_live(&remote, None);
        snapshot.snippets = vec![remote.clone()];
        snapshot.heads = vec![object_to_head(&remote_object)];

        let plan = reconcile(&snapshot, &graph(remote_object, Vec::new())).unwrap();

        assert!(plan.apply.is_empty());
        assert_eq!(plan.publish_heads[0].revision_id, remote.revision_id);
        assert_eq!(plan.publish_objects.len(), 1);
        assert_eq!(plan.publish_objects[0].revision_id, local.revision_id);
        assert_eq!(plan.acknowledge, vec![local.revision_id]);
    }

    #[test]
    fn exact_acknowledgement_never_consumes_later_edit() {
        let first = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-02T00:00:00Z",
            "first",
        );
        let first_snapshot = snapshot(&first);
        let mut later = first_snapshot.clone();
        let second_id = uuid::Uuid::new_v4().to_string();
        let second = snippet("one", &second_id, "2026-01-03T00:00:00Z", "second");
        let payload = canonical_live_payload(&second).unwrap();
        later.snippets = vec![second.clone()];
        later.heads[0].revision_id = second_id.clone();
        later.heads[0].parent_revision_id = Some(first.revision_id.clone());
        later.heads[0].content_hash = sha256_hex(payload.as_bytes());
        later.heads[0].revision_time = second.updated_at.clone();
        later.pending.push(OutboxRevision {
            sequence: 2,
            revision_id: second_id.clone(),
            snippet_id: "one".into(),
            parent_revision_id: Some(first.revision_id.clone()),
            device_id: later.device_id.clone(),
            content_hash: sha256_hex(payload.as_bytes()),
            revision_time: second.updated_at,
            deleted: false,
            operation_kind: "upsert".into(),
            origin: "local".into(),
            payload_bytes: payload.len(),
            payload_json: payload,
            conflict_of: None,
        });
        assert_eq!(
            exact_acknowledgements(&later, std::slice::from_ref(&first.revision_id)),
            vec![first.revision_id]
        );
        assert!(!exact_acknowledgements(&later, &["absent".into()]).contains(&second_id));
    }

    #[test]
    fn remote_tombstone_wins_concurrent_live_and_requests_conflict_copy() {
        let local = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-02T00:00:00Z",
            "local",
        );
        let snapshot = snapshot(&local);
        let remote_id = uuid::Uuid::new_v4().to_string();
        let remote = RevisionObjectV2 {
            version: 2,
            revision_id: remote_id.clone(),
            parent_revision_id: None,
            snippet_id: "one".into(),
            device_id: uuid::Uuid::new_v4().to_string(),
            changed_at: "2026-01-03T00:00:00Z".into(),
            deleted: true,
            content_hash: sha256_hex(
                crate::revision::canonical_tombstone_payload("one", "2026-01-03T00:00:00Z")
                    .unwrap()
                    .as_bytes(),
            ),
            conflict_of: None,
            snippet: None,
        };
        let graph = RemoteGraph {
            heads: HashMap::from([("one".into(), remote.clone())]),
            objects: HashMap::from([(remote_id, remote)]),
            downloaded_objects: 1,
        };
        let plan = reconcile(&snapshot, &graph).unwrap();
        assert_eq!(plan.apply.len(), 1);
        assert!(plan.apply[0].deleted);
        assert!(plan.apply[0].preserve_local_as_conflict);
        assert_eq!(plan.deleted_count, 1);
        assert_eq!(plan.conflict_count, 1);
    }

    #[test]
    fn durable_local_ancestry_detects_remote_ancestor_and_acknowledges_full_chain() {
        let remote_base = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-01T00:00:00Z",
            "remote base",
        );
        let first = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-02T00:00:00Z",
            "local first",
        );
        let second = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-03T00:00:00Z",
            "local second",
        );
        let mut snapshot = snapshot_with_pending_chain(&[first.clone(), second.clone()]);
        snapshot.pending[0].parent_revision_id = Some(remote_base.revision_id.clone());
        snapshot.revision_objects[0].parent_revision_id = Some(remote_base.revision_id.clone());
        let base_object = remote_live(&remote_base, None);
        let plan = reconcile(&snapshot, &graph(base_object, Vec::new())).unwrap();

        assert!(plan.apply.is_empty());
        assert_eq!(plan.publish_objects.len(), 2);
        assert_eq!(plan.publish_heads[0].revision_id, second.revision_id);
        let mut expected = vec![first.revision_id, second.revision_id];
        expected.sort();
        assert_eq!(plan.acknowledge, expected);
    }

    #[test]
    fn concurrent_live_live_keeps_remote_original_and_copies_local_branch() {
        let local = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-02T00:00:00Z",
            "local",
        );
        let remote = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-03T00:00:00Z",
            "remote",
        );
        let plan = reconcile(
            &snapshot(&local),
            &graph(remote_live(&remote, None), Vec::new()),
        )
        .unwrap();

        assert_eq!(plan.apply.len(), 1);
        assert!(plan.apply[0].preserve_local_as_conflict);
        assert_eq!(
            plan.publish_heads[0].revision_id,
            wire_revision_uuid(&remote.revision_id)
        );
        assert_eq!(plan.conflict_count, 1);
        assert_eq!(plan.downloaded_count, 1);
    }

    #[test]
    fn concurrent_local_tombstone_remote_live_preserves_and_acknowledges_losing_revision() {
        let local_revision = uuid::Uuid::new_v4().to_string();
        let snapshot = tombstone_snapshot("one", &local_revision, None, "2026-01-02T00:00:00Z");
        let remote = snippet(
            "one",
            &uuid::Uuid::new_v4().to_string(),
            "2026-01-03T00:00:00Z",
            "remote",
        );
        let plan = reconcile(&snapshot, &graph(remote_live(&remote, None), Vec::new())).unwrap();

        assert_eq!(plan.apply.len(), 1);
        assert!(!plan.apply[0].preserve_local_as_conflict);
        assert_eq!(
            plan.publish_heads[0].revision_id,
            wire_revision_uuid(&remote.revision_id)
        );
        assert_eq!(plan.conflict_count, 1);
        assert_eq!(plan.publish_objects.len(), 1);
        assert_eq!(plan.publish_objects[0].revision_id, local_revision);
        assert_eq!(plan.acknowledge, vec![local_revision]);
    }

    #[test]
    fn concurrent_tombstone_tombstone_preserves_and_acknowledges_losing_revision() {
        let local_revision = uuid::Uuid::new_v4().to_string();
        let remote_revision = uuid::Uuid::new_v4().to_string();
        let snapshot = tombstone_snapshot("one", &local_revision, None, "2026-01-02T00:00:00Z");
        let remote = remote_tombstone("one", &remote_revision, None, "2026-01-03T00:00:00Z");
        let plan = reconcile(&snapshot, &graph(remote, Vec::new())).unwrap();

        assert_eq!(plan.apply.len(), 1);
        assert!(!plan.apply[0].preserve_local_as_conflict);
        assert_eq!(plan.publish_heads[0].revision_id, remote_revision);
        assert_eq!(plan.conflict_count, 1);
        assert_eq!(plan.deleted_count, 1);
        assert_eq!(plan.publish_objects.len(), 1);
        assert_eq!(plan.publish_objects[0].revision_id, local_revision);
        assert_eq!(plan.acknowledge, vec![local_revision]);
    }
}
