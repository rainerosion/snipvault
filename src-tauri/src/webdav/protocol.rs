use super::error::SyncError;
use crate::db::{OutboxRevision, RevisionHead, Snippet, StoredRevisionObject};
use crate::revision::{
    canonical_live_payload, canonical_tombstone_payload, is_sha256, parse_uuid, sha256_hex,
    wire_device_uuid, wire_revision_uuid, CanonicalLivePayload, CanonicalTombstonePayload,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub(crate) const V1_PROTOCOL_VERSION: u64 = 1;
#[cfg(test)]
pub(crate) const REMOTE_PROTOCOL_VERSION: u64 = V1_PROTOCOL_VERSION;
pub(crate) const V2_PROTOCOL_VERSION: u64 = 2;
pub(crate) const MAX_MANIFEST_BYTES: usize = 5 * 1024 * 1024;
pub(crate) const MAX_SNIPPET_BYTES: usize = 10 * 1024 * 1024;
pub(crate) const MAX_REVISION_BYTES: usize = MAX_SNIPPET_BYTES + 256 * 1024;
pub(crate) const MAX_MARKER_BYTES: usize = 64 * 1024;
pub(crate) const MAX_ERROR_BODY_BYTES: usize = 4096;
pub(crate) const MAX_MANIFEST_ITEMS: usize = 100_000;
pub(crate) const MAX_PARENT_CHAIN_DEPTH: usize = 512;
pub(crate) const MAX_ANCESTRY_OBJECTS: usize = 25_000;
pub(crate) const MAX_ANCESTRY_BYTES: usize = 64 * 1024 * 1024;
const MAX_IDENTIFIER_BYTES: usize = 256;

#[derive(Debug, Clone)]
pub(crate) struct WebDavBase {
    url: reqwest::Url,
}

impl WebDavBase {
    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        let mut url =
            reqwest::Url::parse(raw.trim()).map_err(|_| "WebDAV address is invalid".to_string())?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err("WebDAV requires HTTPS".into());
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("WebDAV address must not contain user information".into());
        }
        if url.query().is_some() || url.fragment().is_some() {
            return Err("WebDAV address must not contain a query or fragment".into());
        }
        let host = url
            .host_str()
            .ok_or_else(|| "WebDAV address must include a host".to_string())?;
        let normalized_host = host
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or(host)
            .to_ascii_lowercase();
        let loopback_http = url.scheme() == "http"
            && matches!(normalized_host.as_str(), "localhost" | "127.0.0.1" | "::1");
        if url.scheme() != "https" && !loopback_http {
            return Err("WebDAV requires HTTPS except for loopback testing".into());
        }
        if !url.path().ends_with('/') {
            let path = format!("{}/", url.path());
            url.set_path(&path);
        }
        Ok(Self { url })
    }

    pub(crate) fn endpoint(
        &self,
        segments: &[&str],
        collection: bool,
    ) -> Result<reqwest::Url, String> {
        let mut url = self.url.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| "WebDAV address cannot be used as a collection".to_string())?;
            path.pop_if_empty();
            for segment in segments {
                path.push(segment);
            }
            if collection {
                path.push("");
            }
        }
        Ok(url)
    }

    pub(crate) fn collection_url(&self) -> Result<reqwest::Url, String> {
        self.endpoint(&["snipvault"], true)
    }

    pub(crate) fn objects_collection_url(&self) -> Result<reqwest::Url, String> {
        self.endpoint(&["snipvault", "objects"], true)
    }

    pub(crate) fn manifest_url(&self) -> Result<reqwest::Url, String> {
        self.endpoint(&["snipvault", "manifest.json"], false)
    }

    pub(crate) fn marker_url(&self) -> Result<reqwest::Url, String> {
        self.endpoint(&["snipvault", "protocol-v2.json"], false)
    }

    pub(crate) fn snippet_url(&self, id: &str) -> Result<reqwest::Url, String> {
        validate_identifier(id)?;
        let filename = format!("{id}.json");
        self.endpoint(&["snipvault", &filename], false)
    }

    pub(crate) fn revision_url(&self, revision_id: &str) -> Result<reqwest::Url, String> {
        parse_uuid(revision_id)?;
        let filename = format!("{revision_id}.json");
        self.endpoint(&["snipvault", "objects", &filename], false)
    }

    pub(crate) fn remote_id(&self, username: &str) -> String {
        sha256_hex(format!("{}\0{}", self.url.as_str(), username.trim()).as_bytes())
    }

    pub(crate) fn is_insecure(&self) -> bool {
        self.url.scheme() == "http"
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RemoteSnippet {
    pub id: String,
    pub title: String,
    pub content: String,
    pub language: String,
    pub description: String,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Snippet> for RemoteSnippet {
    fn from(snippet: &Snippet) -> Self {
        Self {
            id: snippet.id.clone(),
            title: snippet.title.clone(),
            content: snippet.content.clone(),
            language: snippet.language.clone(),
            description: snippet.description.clone(),
            tags: snippet.tags.clone(),
            is_favorite: snippet.is_favorite,
            created_at: snippet.created_at.clone(),
            updated_at: snippet.updated_at.clone(),
        }
    }
}

impl From<RemoteSnippet> for Snippet {
    fn from(snippet: RemoteSnippet) -> Self {
        Self {
            id: snippet.id,
            title: snippet.title,
            content: snippet.content,
            language: snippet.language,
            description: snippet.description,
            tags: snippet.tags,
            is_favorite: snippet.is_favorite,
            created_at: snippet.created_at,
            updated_at: snippet.updated_at,
            revision_id: String::new(),
        }
    }
}

#[cfg(test)]
pub(crate) fn canonical_snippet_bytes(snippet: &Snippet) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&RemoteSnippet::from(snippet))
        .map_err(|_| "Snippet canonical serialization failed".to_string())
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct SnippetMeta {
    pub id: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestV1 {
    pub version: u64,
    pub snippets: Vec<SnippetMeta>,
}

impl ManifestV1 {
    #[cfg(test)]
    pub(crate) fn from_snippets(snippets: &[Snippet]) -> Self {
        let mut snippets = snippets
            .iter()
            .map(|snippet| SnippetMeta {
                id: snippet.id.clone(),
                updated_at: snippet.updated_at.clone(),
            })
            .collect::<Vec<_>>();
        snippets.sort_by(|left, right| left.id.cmp(&right.id));
        Self {
            version: V1_PROTOCOL_VERSION,
            snippets,
        }
    }

    #[cfg(test)]
    pub(crate) fn version_map(&self) -> HashMap<String, String> {
        self.snippets
            .iter()
            .map(|meta| (meta.id.clone(), meta.updated_at.clone()))
            .collect()
    }
}

#[cfg(test)]
pub(crate) type Manifest = ManifestV1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestEntryV2 {
    pub snippet_id: String,
    pub head_revision_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestV2 {
    pub version: u64,
    pub vault_id: String,
    pub generation: u64,
    pub entries: Vec<ManifestEntryV2>,
}

impl ManifestV2 {
    pub(crate) fn new(vault_id: String, generation: u64, heads: &[RevisionHead]) -> Self {
        let mut entries = heads
            .iter()
            .map(|head| ManifestEntryV2 {
                snippet_id: head.snippet_id.clone(),
                head_revision_id: wire_revision_uuid(&head.revision_id),
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            left.snippet_id
                .cmp(&right.snippet_id)
                .then(left.head_revision_id.cmp(&right.head_revision_id))
        });
        Self {
            version: V2_PROTOCOL_VERSION,
            vault_id,
            generation,
            entries,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProtocolV2Marker {
    pub version: u64,
    pub vault_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RevisionObjectV2 {
    pub version: u64,
    pub revision_id: String,
    pub parent_revision_id: Option<String>,
    pub snippet_id: String,
    pub device_id: String,
    pub changed_at: String,
    pub deleted: bool,
    pub content_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict_of: Option<String>,
    pub snippet: Option<RemoteSnippet>,
}

impl RevisionObjectV2 {
    pub(crate) fn from_outbox(revision: &OutboxRevision) -> Result<Self, String> {
        let revision_id = wire_revision_uuid(&revision.revision_id);
        let parent_revision_id = revision
            .parent_revision_id
            .as_deref()
            .map(wire_revision_uuid);
        let snippet = if revision.deleted {
            let tombstone: CanonicalTombstonePayload = serde_json::from_str(&revision.payload_json)
                .map_err(|_| "pending tombstone body is invalid".to_string())?;
            if !tombstone.deleted
                || tombstone.id != revision.snippet_id
                || tombstone.deleted_at != revision.revision_time
            {
                return Err("pending tombstone metadata is inconsistent".into());
            }
            None
        } else {
            let payload: CanonicalLivePayload = serde_json::from_str(&revision.payload_json)
                .map_err(|_| "pending revision body is invalid".to_string())?;
            if payload.deleted
                || payload.id != revision.snippet_id
                || payload.updated_at != revision.revision_time
            {
                return Err("pending revision metadata is inconsistent".into());
            }
            Some(RemoteSnippet {
                id: payload.id,
                title: payload.title,
                content: payload.content,
                language: payload.language,
                description: payload.description,
                tags: payload.tags,
                is_favorite: payload.is_favorite,
                created_at: payload.created_at,
                updated_at: payload.updated_at,
            })
        };
        let object = Self {
            version: V2_PROTOCOL_VERSION,
            revision_id,
            parent_revision_id,
            snippet_id: revision.snippet_id.clone(),
            device_id: wire_device_uuid(&revision.device_id),
            changed_at: revision.revision_time.clone(),
            deleted: revision.deleted,
            content_hash: revision.content_hash.to_ascii_lowercase(),
            conflict_of: revision.conflict_of.as_deref().map(wire_revision_uuid),
            snippet,
        };
        validate_revision_object(&object)?;
        Ok(object)
    }

    pub(crate) fn from_stored(revision: &StoredRevisionObject) -> Result<Self, String> {
        let outbox = OutboxRevision {
            sequence: 1,
            revision_id: revision.revision_id.clone(),
            snippet_id: revision.snippet_id.clone(),
            parent_revision_id: revision.parent_revision_id.clone(),
            device_id: revision.device_id.clone(),
            content_hash: revision.content_hash.clone(),
            revision_time: revision.revision_time.clone(),
            deleted: revision.deleted,
            operation_kind: if revision.deleted {
                "delete".into()
            } else {
                "upsert".into()
            },
            origin: revision.origin.clone(),
            payload_json: revision.payload_json.clone(),
            payload_bytes: revision.payload_bytes,
            conflict_of: revision.conflict_of.clone(),
        };
        Self::from_outbox(&outbox)
    }

    pub(crate) fn from_head(
        head: &RevisionHead,
        snippet: Option<&Snippet>,
    ) -> Result<Self, String> {
        let revision_id = wire_revision_uuid(&head.revision_id);
        let snippet = snippet.map(RemoteSnippet::from);
        let object = Self {
            version: V2_PROTOCOL_VERSION,
            revision_id,
            parent_revision_id: head.parent_revision_id.as_deref().map(wire_revision_uuid),
            snippet_id: head.snippet_id.clone(),
            device_id: wire_device_uuid(&head.device_id),
            changed_at: head.revision_time.clone(),
            deleted: head.deleted,
            content_hash: head.content_hash.to_ascii_lowercase(),
            conflict_of: None,
            snippet,
        };
        validate_revision_object(&object)?;
        Ok(object)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ManifestDocument {
    V1(ManifestV1),
    V2(ManifestV2),
}

pub(crate) fn parse_manifest_document(bytes: &[u8]) -> Result<ManifestDocument, String> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err("Remote manifest exceeds the synchronization size limit".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "Remote manifest JSON is invalid".to_string())?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "Remote manifest version is invalid".to_string())?;
    match version {
        V1_PROTOCOL_VERSION => {
            let manifest: ManifestV1 = serde_json::from_value(value)
                .map_err(|_| "Remote v1 manifest JSON is invalid".to_string())?;
            validate_manifest_v1(&manifest)?;
            Ok(ManifestDocument::V1(manifest))
        }
        V2_PROTOCOL_VERSION => {
            let manifest: ManifestV2 = serde_json::from_value(value)
                .map_err(|_| "Remote v2 manifest JSON is invalid".to_string())?;
            validate_manifest_v2(&manifest)?;
            Ok(ManifestDocument::V2(manifest))
        }
        _ => Err("Remote manifest protocol version is unsupported".into()),
    }
}

pub(crate) fn validate_identifier(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > MAX_IDENTIFIER_BYTES
        || id == "."
        || id == ".."
        || id.chars().any(char::is_control)
    {
        return Err("Remote snippet identifier is invalid".into());
    }
    Ok(())
}

pub(crate) fn validate_manifest_v1(manifest: &ManifestV1) -> Result<(), String> {
    if manifest.version != V1_PROTOCOL_VERSION {
        return Err("Remote manifest protocol version is unsupported".into());
    }
    if manifest.snippets.len() > MAX_MANIFEST_ITEMS {
        return Err("Remote manifest contains too many entries".into());
    }
    let mut ids = HashSet::new();
    let mut previous = None;
    for meta in &manifest.snippets {
        validate_identifier(&meta.id)?;
        chrono::DateTime::parse_from_rfc3339(&meta.updated_at)
            .map_err(|_| "Remote manifest timestamp is invalid".to_string())?;
        if !ids.insert(&meta.id) {
            return Err("Remote manifest contains duplicate identifiers".into());
        }
        if previous.is_some_and(|value: &str| value > meta.id.as_str()) {
            return Err("Remote manifest entries are not canonically sorted".into());
        }
        previous = Some(meta.id.as_str());
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate_manifest(manifest: &ManifestV1) -> Result<(), String> {
    validate_manifest_v1(manifest)
}

pub(crate) fn validate_manifest_v2(manifest: &ManifestV2) -> Result<(), String> {
    if manifest.version != V2_PROTOCOL_VERSION {
        return Err("Remote manifest protocol version is unsupported".into());
    }
    parse_uuid(&manifest.vault_id).map_err(|_| "Remote vault identifier is invalid".to_string())?;
    if manifest.entries.len() > MAX_MANIFEST_ITEMS {
        return Err("Remote manifest contains too many entries".into());
    }
    let mut snippet_ids = HashSet::new();
    let mut revision_ids = HashSet::new();
    let mut previous: Option<(&str, &str)> = None;
    for entry in &manifest.entries {
        validate_identifier(&entry.snippet_id)?;
        parse_uuid(&entry.head_revision_id)?;
        if !snippet_ids.insert(&entry.snippet_id) || !revision_ids.insert(&entry.head_revision_id) {
            return Err("Remote manifest contains duplicate identifiers".into());
        }
        if previous.is_some_and(|value| {
            value > (entry.snippet_id.as_str(), entry.head_revision_id.as_str())
        }) {
            return Err("Remote manifest entries are not canonically sorted".into());
        }
        previous = Some((&entry.snippet_id, &entry.head_revision_id));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn manifest_json(manifest: &ManifestV1) -> Result<String, String> {
    validate_manifest_v1(manifest)?;
    let json = serde_json::to_string_pretty(manifest)
        .map_err(|_| "Remote manifest serialization failed".to_string())?;
    if json.len() > MAX_MANIFEST_BYTES {
        return Err("Remote manifest exceeds the synchronization size limit".into());
    }
    Ok(json)
}

pub(crate) fn manifest_v2_bytes(manifest: &ManifestV2) -> Result<Vec<u8>, String> {
    validate_manifest_v2(manifest)?;
    let bytes = serde_json::to_vec(manifest)
        .map_err(|_| "Remote manifest serialization failed".to_string())?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err("Remote manifest exceeds the synchronization size limit".into());
    }
    Ok(bytes)
}

pub(crate) fn manifest_v2_hash(manifest: &ManifestV2) -> Result<String, String> {
    manifest_v2_bytes(manifest).map(|bytes| sha256_hex(&bytes))
}

pub(crate) fn marker_bytes(marker: &ProtocolV2Marker) -> Result<Vec<u8>, String> {
    if marker.version != V2_PROTOCOL_VERSION {
        return Err("Remote protocol marker is invalid".into());
    }
    parse_uuid(&marker.vault_id)
        .map_err(|_| "Remote protocol marker vault identifier is invalid".to_string())?;
    let bytes = serde_json::to_vec(marker)
        .map_err(|_| "Remote protocol marker serialization failed".to_string())?;
    if bytes.len() > MAX_MARKER_BYTES {
        return Err("Remote protocol marker exceeds the size limit".into());
    }
    Ok(bytes)
}

pub(crate) fn parse_marker(bytes: &[u8]) -> Result<ProtocolV2Marker, String> {
    if bytes.len() > MAX_MARKER_BYTES {
        return Err("Remote protocol marker exceeds the size limit".into());
    }
    let marker: ProtocolV2Marker = serde_json::from_slice(bytes)
        .map_err(|_| "Remote protocol marker JSON is invalid".to_string())?;
    marker_bytes(&marker)?;
    Ok(marker)
}

pub(crate) fn revision_object_bytes(object: &RevisionObjectV2) -> Result<Vec<u8>, String> {
    validate_revision_object(object)?;
    let bytes = serde_json::to_vec(object)
        .map_err(|_| "Remote revision serialization failed".to_string())?;
    if bytes.len() > MAX_REVISION_BYTES {
        return Err("Remote revision exceeds the synchronization size limit".into());
    }
    Ok(bytes)
}

pub(crate) fn revision_object_hash(object: &RevisionObjectV2) -> Result<String, String> {
    revision_object_bytes(object).map(|bytes| sha256_hex(&bytes))
}

pub(crate) fn parse_revision_object(
    expected_revision_id: &str,
    bytes: &[u8],
) -> Result<RevisionObjectV2, String> {
    if bytes.len() > MAX_REVISION_BYTES {
        return Err("Remote revision exceeds the synchronization size limit".into());
    }
    let object: RevisionObjectV2 =
        serde_json::from_slice(bytes).map_err(|_| "Remote revision JSON is invalid".to_string())?;
    if object.revision_id != expected_revision_id {
        return Err("Remote revision identifier does not match its object path".into());
    }
    validate_revision_object(&object)?;
    Ok(object)
}

pub(crate) fn validate_revision_object(object: &RevisionObjectV2) -> Result<(), String> {
    if object.version != V2_PROTOCOL_VERSION {
        return Err("Remote revision protocol version is unsupported".into());
    }
    parse_uuid(&object.revision_id)?;
    if let Some(parent) = &object.parent_revision_id {
        parse_uuid(parent)?;
        if parent == &object.revision_id {
            return Err("Remote revision cannot be its own parent".into());
        }
    }
    if let Some(conflict_of) = &object.conflict_of {
        parse_uuid(conflict_of)?;
    }
    validate_identifier(&object.snippet_id)?;
    parse_uuid(&object.device_id)?;
    chrono::DateTime::parse_from_rfc3339(&object.changed_at)
        .map_err(|_| "Remote revision timestamp is invalid".to_string())?;
    if !is_sha256(&object.content_hash) {
        return Err("Remote revision content hash is invalid".into());
    }
    let canonical = if object.deleted {
        if object.snippet.is_some() {
            return Err("Remote tombstone includes a live snippet".into());
        }
        canonical_tombstone_payload(&object.snippet_id, &object.changed_at)?
    } else {
        let remote = object
            .snippet
            .as_ref()
            .ok_or_else(|| "Remote live revision is missing its snippet".to_string())?;
        if remote.id != object.snippet_id || remote.updated_at != object.changed_at {
            return Err("Remote revision metadata does not match its snippet".into());
        }
        let snippet: Snippet = remote.clone().into();
        crate::db::validate_snippet(&snippet)
            .map_err(|_| "Remote revision snippet validation failed".to_string())?;
        canonical_live_payload(&snippet)?
    };
    if sha256_hex(canonical.as_bytes()) != object.content_hash {
        return Err("Remote revision content hash does not match its canonical body".into());
    }
    Ok(())
}

pub(crate) fn validate_parent_chain(
    head_revision_id: &str,
    objects: &HashMap<String, RevisionObjectV2>,
) -> Result<Vec<String>, SyncError> {
    parse_uuid(head_revision_id)
        .map_err(|_| SyncError::validation("Remote head revision identifier is invalid"))?;
    let mut current = Some(head_revision_id.to_string());
    let mut visited = HashSet::new();
    let mut chain = Vec::new();
    let mut bytes = 0_usize;
    while let Some(revision_id) = current {
        if !visited.insert(revision_id.clone()) {
            return Err(SyncError::validation(
                "Remote revision ancestry contains a cycle",
            ));
        }
        if chain.len() >= MAX_PARENT_CHAIN_DEPTH || visited.len() > MAX_ANCESTRY_OBJECTS {
            return Err(SyncError::validation(
                "Remote revision ancestry exceeds its traversal limit",
            ));
        }
        let object = objects.get(&revision_id).ok_or_else(|| {
            SyncError::validation("Remote revision ancestry is missing an immutable object")
        })?;
        bytes = bytes
            .checked_add(
                revision_object_bytes(object)
                    .map_err(SyncError::from)?
                    .len(),
            )
            .ok_or_else(|| SyncError::validation("Remote revision ancestry is too large"))?;
        if bytes > MAX_ANCESTRY_BYTES {
            return Err(SyncError::validation(
                "Remote revision ancestry is too large",
            ));
        }
        chain.push(revision_id);
        current = object.parent_revision_id.clone();
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::revision::{canonical_live_payload, sha256_hex};

    fn snippet(id: &str) -> Snippet {
        Snippet {
            id: id.into(),
            title: "Title".into(),
            content: "body".into(),
            language: "rust".into(),
            description: String::new(),
            tags: vec!["sync".into()],
            is_favorite: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T00:00:00Z".into(),
            revision_id: uuid::Uuid::new_v4().to_string(),
        }
    }

    fn revision(id: &str, parent: Option<String>) -> RevisionObjectV2 {
        let snippet = snippet(id);
        RevisionObjectV2 {
            version: 2,
            revision_id: uuid::Uuid::new_v4().to_string(),
            parent_revision_id: parent,
            snippet_id: id.into(),
            device_id: uuid::Uuid::new_v4().to_string(),
            changed_at: snippet.updated_at.clone(),
            deleted: false,
            content_hash: sha256_hex(canonical_live_payload(&snippet).unwrap().as_bytes()),
            conflict_of: None,
            snippet: Some(RemoteSnippet::from(&snippet)),
        }
    }

    #[test]
    fn rejects_sensitive_or_ambiguous_base_urls() {
        assert!(WebDavBase::parse("https://example.com/dav/").is_ok());
        assert!(WebDavBase::parse("http://localhost:8080/dav/").is_ok());
        assert!(WebDavBase::parse("http://127.0.0.1/dav/").is_ok());
        assert!(WebDavBase::parse("http://[::1]/dav/").is_ok());
        assert!(WebDavBase::parse("http://example.com/dav/").is_err());
        assert!(WebDavBase::parse("https://user:pass@example.com/dav/").is_err());
        assert!(WebDavBase::parse("https://example.com/dav/?token=secret").is_err());
        assert!(WebDavBase::parse("ftp://example.com/dav/").is_err());
    }

    #[test]
    fn path_segments_are_encoded_as_segments() {
        let base = WebDavBase::parse("https://example.com/dav").unwrap();
        let url = base.endpoint(&["snipvault", "a/b"], false).unwrap();
        assert_eq!(url.as_str(), "https://example.com/dav/snipvault/a%2Fb");
        let snippet_url = base.snippet_url("a/b").unwrap();
        assert_eq!(
            snippet_url.as_str(),
            "https://example.com/dav/snipvault/a%2Fb.json"
        );
        let revision_id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            base.revision_url(&revision_id).unwrap().as_str(),
            format!("https://example.com/dav/snipvault/objects/{revision_id}.json")
        );
    }

    #[test]
    fn strict_manifest_document_preserves_v1_and_parses_v2() {
        let v1 = ManifestV1::from_snippets(&[snippet("one")]);
        assert!(matches!(
            parse_manifest_document(manifest_json(&v1).unwrap().as_bytes()).unwrap(),
            ManifestDocument::V1(_)
        ));

        let head = RevisionHead {
            snippet_id: "one".into(),
            revision_id: uuid::Uuid::new_v4().to_string(),
            parent_revision_id: None,
            device_id: uuid::Uuid::new_v4().to_string(),
            content_hash: "a".repeat(64),
            revision_time: "2026-01-01T00:00:00Z".into(),
            deleted: false,
        };
        let v2 = ManifestV2::new(uuid::Uuid::new_v4().to_string(), 1, &[head]);
        assert!(matches!(
            parse_manifest_document(&manifest_v2_bytes(&v2).unwrap()).unwrap(),
            ManifestDocument::V2(_)
        ));

        let mut unknown = serde_json::to_value(&v2).unwrap();
        unknown["unexpected"] = serde_json::json!(true);
        assert!(parse_manifest_document(&serde_json::to_vec(&unknown).unwrap()).is_err());
    }

    #[test]
    fn v2_manifest_canonical_sort_and_hash_are_stable() {
        let heads = vec![
            RevisionHead {
                snippet_id: "zeta".into(),
                revision_id: "6ba7b811-9dad-11d1-80b4-00c04fd430c8".into(),
                parent_revision_id: None,
                device_id: "6ba7b812-9dad-11d1-80b4-00c04fd430c8".into(),
                content_hash: "a".repeat(64),
                revision_time: "2026-01-01T00:00:00Z".into(),
                deleted: true,
            },
            RevisionHead {
                snippet_id: "alpha".into(),
                revision_id: "6ba7b810-9dad-11d1-80b4-00c04fd430c8".into(),
                parent_revision_id: None,
                device_id: "6ba7b812-9dad-11d1-80b4-00c04fd430c8".into(),
                content_hash: "b".repeat(64),
                revision_time: "2026-01-01T00:00:00Z".into(),
                deleted: false,
            },
        ];
        let manifest = ManifestV2::new("6ba7b814-9dad-11d1-80b4-00c04fd430c8".into(), 7, &heads);
        let bytes = manifest_v2_bytes(&manifest).unwrap();
        assert_eq!(
            String::from_utf8(bytes.clone()).unwrap(),
            r#"{"version":2,"vault_id":"6ba7b814-9dad-11d1-80b4-00c04fd430c8","generation":7,"entries":[{"snippet_id":"alpha","head_revision_id":"6ba7b810-9dad-11d1-80b4-00c04fd430c8"},{"snippet_id":"zeta","head_revision_id":"6ba7b811-9dad-11d1-80b4-00c04fd430c8"}]}"#
        );
        assert_eq!(manifest_v2_hash(&manifest).unwrap(), sha256_hex(&bytes));
    }

    #[test]
    fn marker_is_strict_and_stable_across_generations() {
        let marker = ProtocolV2Marker {
            version: 2,
            vault_id: uuid::Uuid::new_v4().to_string(),
        };
        let bytes = marker_bytes(&marker).unwrap();
        assert_eq!(parse_marker(&bytes).unwrap(), marker);
        let mut value = serde_json::to_value(&marker).unwrap();
        value["generation"] = serde_json::json!(1);
        assert!(parse_marker(&serde_json::to_vec(&value).unwrap()).is_err());
    }

    #[test]
    fn revision_hash_tamper_and_cycles_are_rejected() {
        let first = revision("one", None);
        assert!(validate_revision_object(&first).is_ok());
        let mut tampered = first.clone();
        tampered.snippet.as_mut().unwrap().content = "tampered".into();
        assert!(validate_revision_object(&tampered).is_err());

        let mut second = revision("one", Some(first.revision_id.clone()));
        let mut first_cycle = first.clone();
        first_cycle.parent_revision_id = Some(second.revision_id.clone());
        second.parent_revision_id = Some(first_cycle.revision_id.clone());
        let objects = HashMap::from([
            (first_cycle.revision_id.clone(), first_cycle.clone()),
            (second.revision_id.clone(), second.clone()),
        ]);
        assert!(validate_parent_chain(&first_cycle.revision_id, &objects).is_err());
    }

    #[test]
    fn v1_wire_omits_local_revision_metadata() {
        let snippet = snippet("one");
        let payload = canonical_snippet_bytes(&snippet).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 9);
        assert!(value.get("revision_id").is_none());
        assert!(value.get("parent_revision_id").is_none());
        assert!(value.get("device_id").is_none());
        assert!(value.get("deleted").is_none());
        let manifest = ManifestV1::from_snippets(&[snippet]);
        assert_eq!(manifest.version, 1);
    }
}
