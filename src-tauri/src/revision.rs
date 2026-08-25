use crate::db::Snippet;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const REVISION_NAMESPACE: Uuid = Uuid::from_bytes([
    0x48, 0x57, 0x4f, 0x70, 0x2f, 0xe8, 0x57, 0x1d, 0xa6, 0x32, 0xc7, 0xe2, 0x67, 0x8f, 0x75, 0x62,
]);
const CONFLICT_NAMESPACE: Uuid = Uuid::from_bytes([
    0x8b, 0xb8, 0xe8, 0x86, 0x25, 0x6c, 0x55, 0x13, 0x94, 0xe2, 0xf5, 0x6a, 0x97, 0xef, 0x89, 0x62,
]);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CanonicalLivePayload {
    pub id: String,
    pub title: String,
    pub content: String,
    pub language: String,
    pub description: String,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub created_at: String,
    pub updated_at: String,
    pub deleted: bool,
}

impl From<&Snippet> for CanonicalLivePayload {
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
            deleted: false,
        }
    }
}

impl CanonicalLivePayload {
    pub fn into_snippet(self, revision_id: String) -> Snippet {
        Snippet {
            id: self.id,
            title: self.title,
            content: self.content,
            language: self.language,
            description: self.description,
            tags: self.tags,
            is_favorite: self.is_favorite,
            created_at: self.created_at,
            updated_at: self.updated_at,
            revision_id,
        }
    }
}

// Field order intentionally matches serde_json::Map's sorted output used by the
// SQLite-v3 tombstone foundation: deleted, deleted_at, id.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CanonicalTombstonePayload {
    pub deleted: bool,
    pub deleted_at: String,
    pub id: String,
}

pub fn canonical_live_payload(snippet: &Snippet) -> Result<String, String> {
    serde_json::to_string(&CanonicalLivePayload::from(snippet))
        .map_err(|_| "revision payload serialization failed".to_string())
}

pub fn canonical_tombstone_payload(id: &str, deleted_at: &str) -> Result<String, String> {
    serde_json::to_string(&CanonicalTombstonePayload {
        deleted: true,
        deleted_at: deleted_at.to_string(),
        id: id.to_string(),
    })
    .map_err(|_| "tombstone serialization failed".to_string())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

pub fn parse_uuid(value: &str) -> Result<Uuid, String> {
    let parsed = Uuid::parse_str(value).map_err(|_| "identifier is not a UUID".to_string())?;
    if parsed.hyphenated().to_string() != value.to_ascii_lowercase() {
        return Err("identifier is not a canonical UUID".into());
    }
    Ok(parsed)
}

fn sha1_digest(bytes: &[u8]) -> [u8; 20] {
    let bit_len = (bytes.len() as u64).wrapping_mul(8);
    let mut message = bytes.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    let mut state = [
        0x6745_2301_u32,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    for block in message.as_chunks::<64>().0 {
        let mut words = [0_u32; 80];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                block[offset],
                block[offset + 1],
                block[offset + 2],
                block[offset + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }

        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (function, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let next = a
                .rotate_left(5)
                .wrapping_add(function)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = next;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
    }

    let mut digest = [0_u8; 20];
    for (index, value) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&value.to_be_bytes());
    }
    digest
}

/// Produces an RFC 9562 UUIDv5 from a namespace UUID and arbitrary name bytes.
pub fn deterministic_uuid(namespace: Uuid, name: &[u8]) -> Uuid {
    let mut material = Vec::with_capacity(16 + name.len());
    material.extend_from_slice(namespace.as_bytes());
    material.extend_from_slice(name);
    let digest = sha1_digest(&material);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub fn wire_revision_uuid(value: &str) -> String {
    Uuid::parse_str(value)
        .unwrap_or_else(|_| deterministic_uuid(REVISION_NAMESPACE, value.as_bytes()))
        .hyphenated()
        .to_string()
}

pub fn wire_device_uuid(value: &str) -> String {
    Uuid::parse_str(value)
        .unwrap_or_else(|_| {
            deterministic_uuid(REVISION_NAMESPACE, format!("device\0{value}").as_bytes())
        })
        .hyphenated()
        .to_string()
}

pub fn deterministic_legacy_revision_uuid(
    snippet_id: &str,
    content_hash: &str,
    changed_at: &str,
) -> String {
    deterministic_uuid(
        REVISION_NAMESPACE,
        format!("legacy\0{snippet_id}\0{content_hash}\0{changed_at}").as_bytes(),
    )
    .hyphenated()
    .to_string()
}

pub fn deterministic_conflict_uuid(
    snippet_id: &str,
    left_revision_id: &str,
    right_revision_id: &str,
) -> String {
    let mut revisions = [left_revision_id, right_revision_id];
    revisions.sort_unstable();
    deterministic_uuid(
        CONFLICT_NAMESPACE,
        format!("conflict\0{snippet_id}\0{}\0{}", revisions[0], revisions[1]).as_bytes(),
    )
    .hyphenated()
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet() -> Snippet {
        Snippet {
            id: "片段-1".into(),
            title: "A \"quoted\" title".into(),
            content: "line 1\nline 2".into(),
            language: "rust".into(),
            description: String::new(),
            tags: vec!["β".into(), "alpha".into()],
            is_favorite: true,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-02T03:04:05Z".into(),
            revision_id: "ignored-by-canonical-body".into(),
        }
    }

    #[test]
    fn canonical_live_and_tombstone_vectors_are_stable() {
        let live = canonical_live_payload(&snippet()).unwrap();
        assert_eq!(
            live,
            r#"{"id":"片段-1","title":"A \"quoted\" title","content":"line 1\nline 2","language":"rust","description":"","tags":["β","alpha"],"is_favorite":true,"created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-02T03:04:05Z","deleted":false}"#
        );
        assert_eq!(
            sha256_hex(live.as_bytes()),
            "aba9b7ec5081a91eb325b6b276235d660a9ba7fa28ab5aa3a59e9a6d67ac2b6a"
        );

        let tombstone = canonical_tombstone_payload("片段-1", "2026-01-03T00:00:00Z").unwrap();
        assert_eq!(
            tombstone,
            r#"{"deleted":true,"deleted_at":"2026-01-03T00:00:00Z","id":"片段-1"}"#
        );
        assert_eq!(
            sha256_hex(tombstone.as_bytes()),
            "c0a551412b95e5878048b657fd0e44f0c5639802dfbb5592b1863e928f298e8b"
        );
    }

    #[test]
    fn rfc_uuid_v5_vector_is_stable() {
        let dns_namespace = Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap();
        assert_eq!(
            deterministic_uuid(dns_namespace, b"www.example.com").to_string(),
            "2ed6657d-e927-568b-95e1-2665a8aea6a2"
        );
    }

    #[test]
    fn deterministic_uuid_helpers_are_canonical_and_stable() {
        let legacy =
            deterministic_legacy_revision_uuid("one", &"a".repeat(64), "2026-01-01T00:00:00Z");
        assert_eq!(
            legacy,
            deterministic_legacy_revision_uuid("one", &"a".repeat(64), "2026-01-01T00:00:00Z")
        );
        assert!(parse_uuid(&legacy).is_ok());

        let left = Uuid::new_v4().to_string();
        let right = Uuid::new_v4().to_string();
        assert_eq!(
            deterministic_conflict_uuid("one", &left, &right),
            deterministic_conflict_uuid("one", &right, &left)
        );
    }
}
