use super::engine::{SyncEngine, DEFAULT_SYNC_DEADLINE};
use super::engine_v2::{V2SyncEngine, DEFAULT_SYNC_DEADLINE as V2_SYNC_DEADLINE};
use super::error::SyncError;
use super::protocol::{
    manifest_v2_bytes, marker_bytes, parse_manifest_document, parse_marker, parse_revision_object,
    revision_object_bytes, Manifest, ManifestDocument, ManifestV2, ProtocolV2Marker,
    RevisionObjectV2, SnippetMeta, WebDavBase, MAX_MANIFEST_BYTES, MAX_SNIPPET_BYTES,
    REMOTE_PROTOCOL_VERSION,
};
use super::store::{SyncStore, V2SyncStore};
use super::transport::{
    CasOutcome, Clock, Precondition, RemoteTransport, ReqwestTransport, ResourceState, RetryPolicy,
    SystemClock, WebDavAuth,
};
use crate::db::{
    ApplyRemotePlanResult, MergeResult, OutboxRevision, PublishCommit, RevisionHead, Snippet,
    StoredRevisionObject, SyncSnapshot, ValidatedRemotePlan,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Response, Server, StatusCode};

#[derive(Clone)]
struct RequestRecord {
    method: String,
    path: String,
    authorization: Option<String>,
    if_match: Option<String>,
    if_none_match: Option<String>,
    body: Vec<u8>,
}

#[derive(Clone)]
enum PayloadMutation {
    Remove(String),
    Replace(String, Vec<u8>),
}

#[derive(Clone)]
struct MockState {
    collection_exists: bool,
    manifest: Option<Vec<u8>>,
    marker: Option<Vec<u8>>,
    revisions: HashMap<String, Vec<u8>>,
    payloads: HashMap<String, Vec<u8>>,
    head_status: Option<u16>,
    auth: AuthExpectation,
    forced_status: Option<u16>,
    fail_payload_get: usize,
    fail_payload_put: usize,
    fail_manifest_put: usize,
    payload_mutation_after_manifest_put: Option<PayloadMutation>,
    oversized_error_body: bool,
    requests: Vec<RequestRecord>,
}

impl Default for MockState {
    fn default() -> Self {
        Self {
            collection_exists: false,
            manifest: None,
            marker: None,
            revisions: HashMap::new(),
            payloads: HashMap::new(),
            head_status: None,
            auth: AuthExpectation::None,
            forced_status: None,
            fail_payload_get: 0,
            fail_payload_put: 0,
            fail_manifest_put: 0,
            payload_mutation_after_manifest_put: None,
            oversized_error_body: false,
            requests: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AuthExpectation {
    None,
    Basic(String),
    Bearer(String),
    DigestChallenge,
}

struct MockServer {
    base_url: String,
    state: Arc<Mutex<MockState>>,
    shutdown_url: String,
    thread: Option<thread::JoinHandle<()>>,
}

impl MockServer {
    fn start(state: MockState) -> Self {
        let server = Server::http("127.0.0.1:0").expect("loopback test server");
        let address = server.server_addr().to_string();
        let state = Arc::new(Mutex::new(state));
        let server_state = Arc::clone(&state);
        let thread = thread::spawn(move || {
            for mut request in server.incoming_requests() {
                if request.url() == "/__shutdown" {
                    let _ = request.respond(Response::empty(StatusCode(204)));
                    break;
                }
                let mut body = Vec::new();
                request
                    .as_reader()
                    .read_to_end(&mut body)
                    .expect("read mock request body");
                let authorization = request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv("Authorization"))
                    .map(|header| header.value.as_str().to_string());
                let if_match = request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv("If-Match"))
                    .map(|header| header.value.as_str().to_string());
                let if_none_match = request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv("If-None-Match"))
                    .map(|header| header.value.as_str().to_string());
                let method = request.method().as_str().to_string();
                let path = request.url().to_string();
                let response = {
                    let mut state = server_state.lock().unwrap();
                    state.requests.push(RequestRecord {
                        method: method.clone(),
                        path: path.clone(),
                        authorization: authorization.clone(),
                        if_match,
                        if_none_match,
                        body: body.clone(),
                    });
                    handle_request(
                        &mut state,
                        request.method(),
                        &path,
                        authorization.as_deref(),
                        body,
                    )
                };
                let _ = request.respond(response);
            }
        });
        Self {
            base_url: format!("http://{address}/dedicated-test-root/"),
            shutdown_url: format!("http://{address}/__shutdown"),
            state,
            thread: Some(thread),
        }
    }

    fn state(&self) -> MockState {
        self.state.lock().unwrap().clone()
    }
}

impl Drop for MockServer {
    fn drop(&mut self) {
        let _ = reqwest::blocking::get(&self.shutdown_url);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

type MockResponse = Response<std::io::Cursor<Vec<u8>>>;

fn response(status: u16, body: impl Into<Vec<u8>>) -> MockResponse {
    Response::from_data(body.into()).with_status_code(StatusCode(status))
}

fn etag(value: &str) -> Header {
    Header::from_bytes("ETag", value).expect("valid ETag header")
}

fn response_with_etag(status: u16, body: impl Into<Vec<u8>>, value: &str) -> MockResponse {
    response(status, body).with_header(etag(value))
}

fn challenge_response(scheme: &str) -> MockResponse {
    response(401, Vec::new()).with_header(
        Header::from_bytes("WWW-Authenticate", scheme).expect("valid challenge header"),
    )
}

fn auth_allows(state: &MockState, authorization: Option<&str>) -> Result<(), MockResponse> {
    match &state.auth {
        AuthExpectation::None => Ok(()),
        AuthExpectation::Basic(expected) => {
            if authorization == Some(expected.as_str()) {
                Ok(())
            } else {
                Err(challenge_response("Basic realm=\"snipvault-test\""))
            }
        }
        AuthExpectation::Bearer(expected) => {
            if authorization == Some(expected.as_str()) {
                Ok(())
            } else {
                Err(challenge_response("Bearer realm=\"snipvault-test\""))
            }
        }
        AuthExpectation::DigestChallenge => {
            if authorization
                .map(|value| value.starts_with("Digest "))
                .unwrap_or(false)
            {
                Ok(())
            } else {
                Err(challenge_response(
                    "Digest realm=\"snipvault-test\", nonce=\"abc123\", algorithm=MD5, qop=\"auth\"",
                ))
            }
        }
    }
}

fn handle_request(
    state: &mut MockState,
    method: &Method,
    path: &str,
    authorization: Option<&str>,
    body: Vec<u8>,
) -> MockResponse {
    if let Err(response) = auth_allows(state, authorization) {
        return response;
    }
    if let Some(status) = state.forced_status {
        return response(status, Vec::new());
    }
    let manifest_path = "/dedicated-test-root/snipvault/manifest.json";
    let marker_path = "/dedicated-test-root/snipvault/protocol-v2.json";
    let collection_path = "/dedicated-test-root/snipvault/";
    let objects_collection_path = "/dedicated-test-root/snipvault/objects/";
    let revision_id = path
        .strip_prefix(objects_collection_path)
        .and_then(|path| path.strip_suffix(".json"));
    let payload_id = path
        .strip_prefix("/dedicated-test-root/snipvault/")
        .and_then(|path| path.strip_suffix(".json"))
        .filter(|id| *id != "manifest");

    if method.as_str() == "MKCOL" && (path == collection_path || path == objects_collection_path) {
        if state.collection_exists {
            response(405, Vec::new())
        } else {
            state.collection_exists = true;
            response(201, Vec::new())
        }
    } else if method == &Method::Get && path == manifest_path {
        state
            .manifest
            .clone()
            .map(|body| response_with_etag(200, body, "\"manifest-etag\""))
            .unwrap_or_else(|| response(404, Vec::new()))
    } else if method == &Method::Put && path == manifest_path {
        if state.fail_manifest_put > 0 {
            state.fail_manifest_put -= 1;
            let body = if state.oversized_error_body {
                vec![b'x'; 16 * 1024]
            } else {
                b"sensitive-server-body token=never-return".to_vec()
            };
            response(500, body)
        } else {
            state.manifest = Some(body);
            if let Some(mutation) = state.payload_mutation_after_manifest_put.take() {
                match mutation {
                    PayloadMutation::Remove(id) => {
                        state.payloads.remove(&id);
                    }
                    PayloadMutation::Replace(id, body) => {
                        state.payloads.insert(id, body);
                    }
                }
            }
            response_with_etag(204, Vec::new(), "\"manifest-next\"")
        }
    } else if method == &Method::Get && path == marker_path {
        state
            .marker
            .clone()
            .map(|body| response_with_etag(200, body, "\"marker-etag\""))
            .unwrap_or_else(|| response(404, Vec::new()))
    } else if method == &Method::Put && path == marker_path {
        state.marker = Some(body);
        response_with_etag(201, Vec::new(), "\"marker-next\"")
    } else if let Some(id) = revision_id {
        match *method {
            Method::Get => state
                .revisions
                .get(id)
                .cloned()
                .map(|body| response_with_etag(200, body, "\"revision-etag\""))
                .unwrap_or_else(|| response(404, Vec::new())),
            Method::Put => {
                if state.revisions.contains_key(id) {
                    response(412, Vec::new())
                } else {
                    state.revisions.insert(id.to_string(), body);
                    response_with_etag(201, Vec::new(), "\"revision-next\"")
                }
            }
            _ => response(405, Vec::new()),
        }
    } else if let Some(id) = payload_id {
        match *method {
            Method::Get => {
                if state.fail_payload_get > 0 {
                    state.fail_payload_get -= 1;
                    response(500, b"sensitive payload error".to_vec())
                } else {
                    state
                        .payloads
                        .get(id)
                        .cloned()
                        .map(|body| response(200, body))
                        .unwrap_or_else(|| response(404, Vec::new()))
                }
            }
            Method::Head => state
                .head_status
                .map(|status| response(status, Vec::new()))
                .unwrap_or_else(|| {
                    if state.payloads.contains_key(id) {
                        response(200, Vec::new())
                    } else {
                        response(404, Vec::new())
                    }
                }),
            Method::Put => {
                if state.fail_payload_put > 0 {
                    state.fail_payload_put -= 1;
                    response(500, b"sensitive upload error".to_vec())
                } else {
                    state.payloads.insert(id.to_string(), body);
                    response(204, Vec::new())
                }
            }
            _ => response(405, Vec::new()),
        }
    } else {
        response(404, Vec::new())
    }
}

fn snippet(id: &str, updated_at: &str) -> Snippet {
    Snippet {
        id: id.into(),
        title: format!("title-{id}"),
        content: format!("body-{id}"),
        language: "text".into(),
        description: String::new(),
        tags: Vec::new(),
        is_favorite: false,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: updated_at.into(),
        revision_id: String::new(),
    }
}

fn remote_snippet_json(snippet: &Snippet) -> Vec<u8> {
    super::protocol::canonical_snippet_bytes(snippet).unwrap()
}

fn manifest(entries: &[(&str, &str)]) -> Vec<u8> {
    serde_json::to_vec(&Manifest {
        version: REMOTE_PROTOCOL_VERSION,
        snippets: entries
            .iter()
            .map(|(id, updated_at)| SnippetMeta {
                id: (*id).into(),
                updated_at: (*updated_at).into(),
            })
            .collect(),
    })
    .unwrap()
}

fn transport(
    server: &MockServer,
    mode: &str,
    username: &str,
    secret: &str,
) -> ReqwestTransport<SystemClock> {
    ReqwestTransport::with_clock(
        WebDavBase::parse(&server.base_url).unwrap(),
        WebDavAuth::from_settings(mode, username, secret).unwrap(),
        Duration::from_secs(2),
        RetryPolicy {
            max_attempts: 2,
            initial_backoff: Duration::ZERO,
            retry_after_cap: Duration::ZERO,
        },
        SystemClock,
    )
    .unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn remote_revision(item: &Snippet) -> RevisionObjectV2 {
    let payload = crate::revision::canonical_live_payload(item).unwrap();
    RevisionObjectV2 {
        version: 2,
        revision_id: item.revision_id.clone(),
        parent_revision_id: None,
        snippet_id: item.id.clone(),
        device_id: uuid::Uuid::new_v4().to_string(),
        changed_at: item.updated_at.clone(),
        deleted: false,
        content_hash: crate::revision::sha256_hex(payload.as_bytes()),
        conflict_of: None,
        snippet: Some(item.into()),
    }
}

#[derive(Clone)]
struct LoopbackV2Store {
    snapshot: Arc<Mutex<SyncSnapshot>>,
    vault_id: Arc<Mutex<Option<String>>>,
    commits: Arc<Mutex<Vec<PublishCommit>>>,
}

impl LoopbackV2Store {
    fn fresh(item: Snippet) -> Self {
        let payload = crate::revision::canonical_live_payload(&item).unwrap();
        let device_id = uuid::Uuid::new_v4().to_string();
        let head = RevisionHead {
            snippet_id: item.id.clone(),
            revision_id: item.revision_id.clone(),
            parent_revision_id: None,
            device_id: device_id.clone(),
            content_hash: crate::revision::sha256_hex(payload.as_bytes()),
            revision_time: item.updated_at.clone(),
            deleted: false,
        };
        let pending = OutboxRevision {
            sequence: 1,
            revision_id: item.revision_id.clone(),
            snippet_id: item.id.clone(),
            parent_revision_id: None,
            device_id: device_id.clone(),
            content_hash: head.content_hash.clone(),
            revision_time: item.updated_at.clone(),
            deleted: false,
            operation_kind: "upsert".into(),
            origin: "local".into(),
            payload_json: payload.clone(),
            payload_bytes: payload.len(),
            conflict_of: None,
        };
        let stored = StoredRevisionObject {
            revision_id: pending.revision_id.clone(),
            snippet_id: pending.snippet_id.clone(),
            parent_revision_id: None,
            device_id: pending.device_id.clone(),
            content_hash: pending.content_hash.clone(),
            revision_time: pending.revision_time.clone(),
            deleted: false,
            origin: pending.origin.clone(),
            payload_json: pending.payload_json.clone(),
            payload_bytes: pending.payload_bytes,
            conflict_of: None,
        };
        Self {
            snapshot: Arc::new(Mutex::new(SyncSnapshot {
                device_id,
                snippets: vec![item],
                heads: vec![head],
                pending: vec![pending],
                revision_objects: vec![stored],
                pending_bytes: payload.len(),
                remote: None,
            })),
            vault_id: Arc::new(Mutex::new(None)),
            commits: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl V2SyncStore for LoopbackV2Store {
    fn load_snapshot(&self, _remote_id: &str) -> Result<SyncSnapshot, SyncError> {
        Ok(self.snapshot.lock().unwrap().clone())
    }

    fn load_vault_id(&self, _remote_id: &str) -> Result<Option<String>, SyncError> {
        Ok(self.vault_id.lock().unwrap().clone())
    }

    fn apply_remote_plan(
        &self,
        _plan: &ValidatedRemotePlan,
    ) -> Result<ApplyRemotePlanResult, SyncError> {
        Ok(ApplyRemotePlanResult {
            applied: 0,
            skipped: 0,
            conflicts_created: 0,
        })
    }

    fn commit_published(&self, commit: &PublishCommit) -> Result<usize, SyncError> {
        let mut snapshot = self.snapshot.lock().unwrap();
        let pending = snapshot
            .pending
            .iter()
            .map(|revision| revision.revision_id.as_str())
            .collect::<std::collections::HashSet<_>>();
        if commit
            .acknowledged_revision_ids
            .iter()
            .any(|revision| !pending.contains(revision.as_str()))
        {
            return Err(SyncError::local(
                "Loopback store exact acknowledgement failed",
            ));
        }
        let acknowledged = commit.acknowledged_revision_ids.len();
        snapshot.pending.retain(|revision| {
            !commit
                .acknowledged_revision_ids
                .contains(&revision.revision_id)
        });
        snapshot.pending_bytes = snapshot
            .pending
            .iter()
            .map(|revision| revision.payload_bytes)
            .sum();
        *self.vault_id.lock().unwrap() = Some(commit.vault_id.clone());
        self.commits.lock().unwrap().push(commit.clone());
        Ok(acknowledged)
    }
}

#[test]
fn v2_engine_bootstraps_fresh_loopback_vault_and_exactly_acknowledges_outbox() {
    let mut item = snippet("engine-fresh", "2026-01-02T00:00:00Z");
    item.revision_id = uuid::Uuid::new_v4().to_string();
    let store = LoopbackV2Store::fresh(item.clone());
    let server = MockServer::start(MockState::default());
    let transport = transport(&server, "none", "", "");
    let clock = SystemClock;

    let result = V2SyncEngine::new(
        &transport,
        &store,
        &clock,
        "loopback-v2".into(),
        clock.now() + V2_SYNC_DEADLINE,
    )
    .run()
    .unwrap();

    assert_eq!(result.uploaded_count, 1);
    assert_eq!(result.downloaded_count, 0);
    assert_eq!(result.generation, 1);
    assert!(store.snapshot.lock().unwrap().pending.is_empty());
    let commits = store.commits.lock().unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(
        commits[0].acknowledged_revision_ids,
        vec![item.revision_id.clone()]
    );

    let state = server.state();
    let manifest = state.manifest.as_deref().unwrap();
    let ManifestDocument::V2(manifest) = parse_manifest_document(manifest).unwrap() else {
        panic!("fresh engine bootstrap must publish a v2 manifest");
    };
    assert_eq!(manifest.generation, 1);
    assert_eq!(manifest.entries.len(), 1);
    assert_eq!(manifest.entries[0].head_revision_id, item.revision_id);
    assert!(state.marker.is_some());
    assert!(state.revisions.contains_key(&item.revision_id));
    let manifest_put = state
        .requests
        .iter()
        .find(|request| request.method == "PUT" && request.path.ends_with("/manifest.json"))
        .unwrap();
    assert_eq!(manifest_put.if_none_match.as_deref(), Some("*"));
    let revision_put_index = state
        .requests
        .iter()
        .position(|request| {
            request.method == "PUT"
                && request
                    .path
                    .ends_with(&format!("/objects/{}.json", item.revision_id))
        })
        .unwrap();
    let marker_put_index = state
        .requests
        .iter()
        .position(|request| request.method == "PUT" && request.path.ends_with("/protocol-v2.json"))
        .unwrap();
    let manifest_put_index = state
        .requests
        .iter()
        .position(|request| request.method == "PUT" && request.path.ends_with("/manifest.json"))
        .unwrap();
    assert!(revision_put_index < manifest_put_index);
    assert!(manifest_put_index < marker_put_index);
}

#[test]
fn v2_transport_uses_exact_layout_conditional_headers_and_parsed_metadata() {
    let mut item = snippet("one", "2026-01-02T00:00:00Z");
    item.revision_id = uuid::Uuid::new_v4().to_string();
    let revision = remote_revision(&item);
    let vault_id = uuid::Uuid::new_v4().to_string();
    let marker = ProtocolV2Marker {
        version: 2,
        vault_id: vault_id.clone(),
    };
    let manifest = ManifestV2 {
        version: 2,
        vault_id,
        generation: 4,
        entries: vec![super::protocol::ManifestEntryV2 {
            snippet_id: item.id.clone(),
            head_revision_id: revision.revision_id.clone(),
        }],
    };
    let server = MockServer::start(MockState::default());
    let transport = transport(&server, "none", "", "");

    transport.ensure_collection(deadline()).unwrap();
    transport.ensure_objects_collection(deadline()).unwrap();
    transport
        .put_revision_immutable(&revision, deadline())
        .unwrap();
    transport
        .put_revision_immutable(&revision, deadline())
        .unwrap();
    assert_eq!(
        transport
            .put_manifest_v2_conditional(
                &manifest,
                &Precondition::Match("\"manifest-etag\"".into()),
                deadline(),
            )
            .unwrap(),
        CasOutcome::Published {
            etag: Some("\"manifest-next\"".into())
        }
    );
    assert_eq!(
        transport
            .put_marker_conditional(&marker, &Precondition::Create, deadline())
            .unwrap(),
        CasOutcome::Published {
            etag: Some("\"marker-next\"".into())
        }
    );

    let ResourceState::Present(observed_manifest) =
        transport.get_manifest_document(deadline()).unwrap()
    else {
        panic!("v2 manifest should exist");
    };
    assert_eq!(
        observed_manifest.value,
        ManifestDocument::V2(manifest.clone())
    );
    assert_eq!(observed_manifest.etag.as_deref(), Some("\"manifest-etag\""));
    assert_eq!(
        observed_manifest.body_bytes,
        manifest_v2_bytes(&manifest).unwrap().len()
    );
    let ResourceState::Present(observed_marker) = transport.get_marker(deadline()).unwrap() else {
        panic!("v2 marker should exist");
    };
    assert_eq!(observed_marker.value, marker);
    assert_eq!(
        observed_marker.body_bytes,
        marker_bytes(&marker).unwrap().len()
    );
    let ResourceState::Present(observed_revision) = transport
        .get_revision(&revision.revision_id, deadline())
        .unwrap()
    else {
        panic!("v2 revision should exist");
    };
    assert_eq!(observed_revision.value, revision);
    assert_eq!(
        observed_revision.body_bytes,
        revision_object_bytes(&revision).unwrap().len()
    );

    let state = server.state();
    assert!(state.requests.iter().any(|request| {
        request.method == "PUT"
            && request.path.ends_with("/snipvault/manifest.json")
            && request.if_match.as_deref() == Some("\"manifest-etag\"")
    }));
    assert!(state.requests.iter().any(|request| {
        request.method == "PUT"
            && request.path.ends_with("/snipvault/protocol-v2.json")
            && request.if_none_match.as_deref() == Some("*")
    }));
    assert!(state.requests.iter().any(|request| {
        request.method == "PUT"
            && request
                .path
                .ends_with(&format!("/snipvault/objects/{}.json", revision.revision_id))
            && request.if_none_match.as_deref() == Some("*")
    }));
    assert_eq!(
        parse_manifest_document(state.manifest.as_deref().unwrap()).unwrap(),
        ManifestDocument::V2(manifest)
    );
    assert_eq!(
        parse_marker(state.marker.as_deref().unwrap()).unwrap(),
        marker
    );
    assert_eq!(
        parse_revision_object(
            &revision.revision_id,
            state.revisions.get(&revision.revision_id).unwrap(),
        )
        .unwrap(),
        revision
    );
}

#[test]
fn mkcol_created_and_already_exists_then_manifest_get_and_put() {
    for existing in [false, true] {
        let server = MockServer::start(MockState {
            collection_exists: existing,
            ..Default::default()
        });
        let transport = transport(&server, "none", "", "");
        transport.ensure_collection(deadline()).unwrap();
        assert!(transport.get_manifest(deadline()).unwrap().is_none());
        transport
            .put_manifest(
                &Manifest {
                    version: REMOTE_PROTOCOL_VERSION,
                    snippets: Vec::new(),
                },
                deadline(),
            )
            .unwrap();
        assert!(transport.get_manifest(deadline()).unwrap().is_some());
    }
}

#[test]
fn payload_get_put_and_head_success_or_not_found() {
    let server = MockServer::start(MockState::default());
    let transport = transport(&server, "none", "", "");
    let item = snippet("one", "2026-01-01T00:00:00Z");
    assert!(!transport.snippet_exists("one", deadline()).unwrap());
    transport.put_snippet(&item, deadline()).unwrap();
    assert!(transport.snippet_exists("one", deadline()).unwrap());
    assert_eq!(
        transport.get_snippet("one", deadline()).unwrap(),
        Some(item)
    );
    assert!(transport
        .get_snippet("missing", deadline())
        .unwrap()
        .is_none());
}

#[test]
fn ambiguous_head_statuses_fall_back_to_bounded_get() {
    for head_status in [405, 501, 400, 500] {
        let item = snippet("one", "2026-01-01T00:00:00Z");
        let server = MockServer::start(MockState {
            payloads: HashMap::from([("one".into(), remote_snippet_json(&item))]),
            head_status: Some(head_status),
            ..Default::default()
        });
        let transport = transport(&server, "none", "", "");
        assert!(transport.snippet_exists("one", deadline()).unwrap());
        let state = server.state();
        assert!(state
            .requests
            .iter()
            .any(|request| request.method == "HEAD"));
        assert!(state.requests.iter().any(|request| request.method == "GET"));
    }
}

#[test]
fn head_auth_failures_remain_explicit_without_get_fallback() {
    let server = MockServer::start(MockState {
        auth: AuthExpectation::Bearer("Bearer right".into()),
        ..Default::default()
    });
    let transport = transport(&server, "bearer", "", "wrong");
    let error = transport.snippet_exists("one", deadline()).unwrap_err();
    assert!(!error.retryable);
    assert_eq!(server.state().requests.len(), 1);
}

#[test]
fn none_basic_bearer_digest_and_auto_challenge_paths() {
    let cases = [
        (AuthExpectation::None, "none", "", ""),
        (
            AuthExpectation::Basic("Basic dXNlcjpwYXNz".into()),
            "basic",
            "user",
            "pass",
        ),
        (
            AuthExpectation::Bearer("Bearer token".into()),
            "bearer",
            "",
            "token",
        ),
        (AuthExpectation::DigestChallenge, "digest", "user", "pass"),
        (AuthExpectation::DigestChallenge, "auto", "user", "pass"),
    ];
    for (auth, mode, username, secret) in cases {
        let server = MockServer::start(MockState {
            auth,
            ..Default::default()
        });
        let transport = transport(&server, mode, username, secret);
        assert!(transport.get_manifest(deadline()).unwrap().is_none());
        let state = server.state();
        if matches!(mode, "digest" | "auto") {
            assert!(state.requests.len() >= 2);
            assert!(state.requests.iter().any(|request| {
                request
                    .authorization
                    .as_deref()
                    .map(|value| value.starts_with("Digest "))
                    .unwrap_or(false)
            }));
        }
    }
}

#[test]
fn auto_falls_back_to_basic_when_digest_is_not_challenged() {
    let server = MockServer::start(MockState {
        auth: AuthExpectation::Basic("Basic dXNlcjpwYXNz".into()),
        ..Default::default()
    });
    let transport = transport(&server, "auto", "user", "pass");
    assert!(transport.get_manifest(deadline()).unwrap().is_none());
    assert!(server.state().requests.len() >= 2);
}

#[test]
fn authentication_and_validation_4xx_are_not_retried() {
    let cases = [
        (401, AuthExpectation::None, "none", "", ""),
        (
            401,
            AuthExpectation::Bearer("Bearer right".into()),
            "bearer",
            "",
            "wrong",
        ),
        (409, AuthExpectation::None, "none", "", ""),
        (422, AuthExpectation::None, "none", "", ""),
    ];
    for (status, auth, mode, username, secret) in cases {
        let server = MockServer::start(MockState {
            auth,
            forced_status: Some(status),
            ..Default::default()
        });
        let error = transport(&server, mode, username, secret)
            .get_manifest(deadline())
            .unwrap_err();
        assert!(!error.retryable);
        assert_eq!(server.state().requests.len(), 1, "status={status}");
    }
}

#[test]
fn missing_payload_and_malformed_or_oversized_remote_data_are_safe() {
    let malformed_manifest = MockServer::start(MockState {
        manifest: Some(b"{not-json".to_vec()),
        ..Default::default()
    });
    let error = transport(&malformed_manifest, "none", "", "")
        .get_manifest(deadline())
        .unwrap_err();
    assert!(!error.retryable);

    let oversized_manifest = MockServer::start(MockState {
        manifest: Some(vec![b'x'; MAX_MANIFEST_BYTES + 1]),
        ..Default::default()
    });
    assert!(transport(&oversized_manifest, "none", "", "")
        .get_manifest(deadline())
        .is_err());

    let malformed_payload = MockServer::start(MockState {
        payloads: HashMap::from([("one".into(), b"{not-json".to_vec())]),
        ..Default::default()
    });
    assert!(transport(&malformed_payload, "none", "", "")
        .get_snippet("one", deadline())
        .is_err());

    let oversized_payload = MockServer::start(MockState {
        payloads: HashMap::from([("one".into(), vec![b'x'; MAX_SNIPPET_BYTES + 1])]),
        ..Default::default()
    });
    assert!(transport(&oversized_payload, "none", "", "")
        .get_snippet("one", deadline())
        .is_err());

    let missing_payload = MockServer::start(MockState::default());
    assert!(transport(&missing_payload, "none", "", "")
        .get_snippet("one", deadline())
        .unwrap()
        .is_none());
}

#[test]
fn bounded_error_body_is_discarded_and_never_returned() {
    let server = MockServer::start(MockState {
        fail_manifest_put: 3,
        oversized_error_body: true,
        ..Default::default()
    });
    let error = transport(&server, "none", "", "")
        .put_manifest(
            &Manifest {
                version: REMOTE_PROTOCOL_VERSION,
                snippets: Vec::new(),
            },
            deadline(),
        )
        .unwrap_err()
        .to_string();
    assert!(!error.contains("token"));
    assert!(!error.contains("xxxxx"));
    assert!(!error.contains(&server.base_url));
}

struct MemoryStore {
    snippets: Mutex<Vec<Snippet>>,
}

impl MemoryStore {
    fn new(snippets: Vec<Snippet>) -> Self {
        Self {
            snippets: Mutex::new(snippets),
        }
    }
}

impl SyncStore for MemoryStore {
    fn snapshot(&self) -> Result<Vec<Snippet>, SyncError> {
        Ok(self.snippets.lock().unwrap().clone())
    }

    fn merge(&self, incoming: Vec<Snippet>) -> Result<MergeResult, SyncError> {
        let mut snippets = self.snippets.lock().unwrap();
        let mut inserted = 0;
        let mut updated = 0;
        let mut skipped = 0;
        for remote in incoming {
            if let Some(local) = snippets.iter_mut().find(|local| local.id == remote.id) {
                if remote.updated_at >= local.updated_at {
                    *local = remote;
                    updated += 1;
                } else {
                    skipped += 1;
                }
            } else {
                snippets.push(remote);
                inserted += 1;
            }
        }
        Ok(MergeResult {
            inserted,
            updated,
            skipped,
            total: snippets.len(),
        })
    }
}

fn run_engine(
    server: &MockServer,
    store: &MemoryStore,
) -> Result<super::engine::EngineResult, SyncError> {
    let transport = transport(server, "none", "", "");
    let clock = SystemClock;
    SyncEngine::new(
        &transport,
        store,
        &clock,
        Instant::now() + DEFAULT_SYNC_DEADLINE,
    )
    .run()
}

#[test]
fn manifest_and_payload_versions_must_agree() {
    let payload = snippet("one", "2026-01-01T00:00:00Z");
    let server = MockServer::start(MockState {
        manifest: Some(manifest(&[("one", "2026-01-02T00:00:00Z")])),
        payloads: HashMap::from([("one".into(), remote_snippet_json(&payload))]),
        ..Default::default()
    });
    let error = run_engine(&server, &MemoryStore::new(Vec::new())).unwrap_err();
    assert!(!error.retryable);
}

#[test]
fn partial_payload_and_manifest_failures_do_not_report_success() {
    let local = snippet("local", "2026-01-02T00:00:00Z");
    let payload_failure = MockServer::start(MockState {
        fail_payload_put: 3,
        ..Default::default()
    });
    assert!(run_engine(&payload_failure, &MemoryStore::new(vec![local.clone()])).is_err());
    assert!(payload_failure.state().manifest.is_none());

    let manifest_failure = MockServer::start(MockState {
        fail_manifest_put: 3,
        ..Default::default()
    });
    let error = run_engine(&manifest_failure, &MemoryStore::new(vec![local]))
        .unwrap_err()
        .to_string();
    assert!(!error.contains("server body"));
    let state = manifest_failure.state();
    assert!(state.payloads.contains_key("local"));
    assert!(state.manifest.is_none());
}

#[test]
fn partial_download_failure_does_not_mutate_local_store() {
    let remote = snippet("remote", "2026-01-02T00:00:00Z");
    let server = MockServer::start(MockState {
        manifest: Some(manifest(&[("remote", &remote.updated_at)])),
        payloads: HashMap::from([("remote".into(), remote_snippet_json(&remote))]),
        fail_payload_get: 3,
        ..Default::default()
    });
    let store = MemoryStore::new(Vec::new());
    assert!(run_engine(&server, &store).is_err());
    assert!(store.snapshot().unwrap().is_empty());
}

#[test]
fn multiple_invocations_converge_after_partial_remote_side_effects() {
    let local = snippet("local", "2026-01-02T00:00:00Z");
    let remote = snippet("remote", "2026-01-03T00:00:00Z");
    let server = MockServer::start(MockState {
        manifest: Some(manifest(&[("remote", &remote.updated_at)])),
        payloads: HashMap::from([("remote".into(), remote_snippet_json(&remote))]),
        fail_manifest_put: 3,
        ..Default::default()
    });
    let store = MemoryStore::new(vec![local]);
    assert!(run_engine(&server, &store).is_err());
    assert!(server.state().payloads.contains_key("local"));

    server.state.lock().unwrap().fail_manifest_put = 0;
    let result = run_engine(&server, &store).unwrap();
    assert_eq!(result.total_count, 2);
    let state = server.state();
    let final_manifest: Manifest =
        serde_json::from_slice(state.manifest.as_ref().unwrap()).unwrap();
    assert_eq!(final_manifest.snippets.len(), 2);
    assert_eq!(store.snapshot().unwrap().len(), 2);
}

#[test]
fn equal_timestamp_missing_payload_is_repaired_conservatively() {
    let local = snippet("one", "2026-01-02T00:00:00Z");
    let server = MockServer::start(MockState {
        manifest: Some(manifest(&[("one", &local.updated_at)])),
        ..Default::default()
    });
    let result = run_engine(&server, &MemoryStore::new(vec![local])).unwrap();
    assert_eq!(result.uploaded_count, 1);
    assert!(server.state().payloads.contains_key("one"));
}

#[test]
fn final_verification_rejects_payload_disappearance_after_manifest_put() {
    let local = snippet("one", "2026-01-02T00:00:00Z");
    let server = MockServer::start(MockState {
        payload_mutation_after_manifest_put: Some(PayloadMutation::Remove("one".into())),
        ..Default::default()
    });

    let error = run_engine(&server, &MemoryStore::new(vec![local])).unwrap_err();
    assert!(error.retryable);
    let state = server.state();
    let verification_gets = state
        .requests
        .iter()
        .filter(|request| request.method == "GET" && request.path.ends_with("/one.json"))
        .count();
    assert_eq!(verification_gets, super::engine::MAX_REMOTE_REREAD_ROUNDS);
}

#[test]
fn final_verification_rejects_payload_corruption_after_manifest_put() {
    let local = snippet("one", "2026-01-02T00:00:00Z");
    let server = MockServer::start(MockState {
        payload_mutation_after_manifest_put: Some(PayloadMutation::Replace(
            "one".into(),
            b"{not-json".to_vec(),
        )),
        ..Default::default()
    });

    let error = run_engine(&server, &MemoryStore::new(vec![local])).unwrap_err();
    assert!(!error.retryable);
}

#[test]
fn equal_timestamp_divergent_devices_converge_to_canonical_winner() {
    let updated_at = "2026-01-02T00:00:00Z";
    let mut lower = snippet("one", updated_at);
    lower.title = "alpha".into();
    let mut higher = lower.clone();
    higher.title = "omega".into();
    assert!(
        super::protocol::canonical_snippet_bytes(&higher).unwrap()
            > super::protocol::canonical_snippet_bytes(&lower).unwrap()
    );

    let server = MockServer::start(MockState {
        manifest: Some(manifest(&[("one", updated_at)])),
        payloads: HashMap::from([("one".into(), remote_snippet_json(&lower))]),
        ..Default::default()
    });
    let higher_device = MemoryStore::new(vec![higher.clone()]);
    let lower_device = MemoryStore::new(vec![lower]);

    let first = run_engine(&server, &higher_device).unwrap();
    assert_eq!(first.uploaded_count, 1);
    let second = run_engine(&server, &lower_device).unwrap();
    assert_eq!(second.downloaded_count, 1);
    assert_eq!(lower_device.snapshot().unwrap(), vec![higher.clone()]);
    assert_eq!(
        transport(&server, "none", "", "")
            .get_snippet("one", deadline())
            .unwrap(),
        Some(higher)
    );
}

#[test]
fn request_records_do_not_require_real_or_non_loopback_data() {
    let server = MockServer::start(MockState::default());
    let transport = transport(&server, "none", "", "");
    transport.get_manifest(deadline()).unwrap();
    let state = server.state();
    assert!(server.base_url.starts_with("http://127.0.0.1:"));
    assert!(state
        .requests
        .iter()
        .all(|request| request.path.starts_with("/dedicated-test-root/")));
    assert!(state.requests.iter().all(|request| request.body.is_empty()));
    for record in &state.requests {
        assert!(!record.method.contains("secret-value"));
        assert!(!record.path.contains("secret-value"));
        assert!(!record
            .body
            .windows(b"secret-value".len())
            .any(|window| window == b"secret-value"));
        assert_ne!(record.authorization.as_deref(), Some("Bearer secret-value"));
    }
}

#[test]
fn source_tagged_busy_event_contract_is_preserved() {
    let guard = super::SYNC_LOCK.lock().unwrap();
    let failure = thread::spawn(super::test_try_sync_lock)
        .join()
        .unwrap()
        .unwrap_err();
    drop(guard);
    let payload = crate::sync::SyncEventPayload::error(
        crate::sync::SyncSource::Background,
        crate::error::CommandError::sync(&failure),
    );
    let value = serde_json::to_value(payload).unwrap();
    assert_eq!(value["source"], "background");
    assert_eq!(value["status"], "busy");
    assert_eq!(value["error"]["code"], "sync_busy");
    assert_eq!(value["error"]["retryable"], true);
}
