//! 双哈希 / Dual hashing (§3, spec GC-03)。
//! 每个 ① 区实体有 `blockHash`(12 hex)；全文件有 `fileHash`(64 hex)；
//! 语义区整体有 `contentRootHash`(64 hex)。被排除的键（layout/review/证据
//! 定位等）在哈希前统一剔除，主张=身份，证据=悬挂字段。

use crate::canonical::canonicalize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// sha256 → 64 位小写 hex / sha256 → 64 lowercase hex chars.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// 被排除出语义哈希的键（§3 边界定案 E4/E5 + 规则 6/7）。
/// Keys excluded from semantic hashing: review trail, timestamps, status,
/// layout, evidence locator details, and the hanging `evidenceIds` field.
///
/// `blockHash` and `fileHash` are also excluded because they are derived
/// fields, not part of the semantic claim; otherwise hashing would become
/// circular/git-style self-encoding would fail.
const EXCLUDED_KEYS: &[&str] = &[
    "blockHash",
    "contentRootHash",
    "derivedFrom",
    "fileHash",
    "review",
    "status",
    "layout",
    "provenance",
    "evidenceIds",
    "createdAt",
    "updatedAt",
    "reviewedAt",
    "startOffset",
    "endOffset",
    "page",
    "locator",
    "quote",
];

fn is_excluded_key(key: &str) -> bool {
    EXCLUDED_KEYS.contains(&key)
}

/// 递归剔除被排除的键 / Recursively strip excluded keys from a claim value.
fn strip_excluded(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|key, _| !is_excluded_key(key));
            for nested in map.values_mut() {
                strip_excluded(nested);
            }
        }
        Value::Array(items) => {
            for item in items {
                strip_excluded(item);
            }
        }
        _ => {}
    }
}

/// 实体 `blockHash`（12 hex）= sha256(规范化内容) 的前 12 字符。
/// Entity block hash (12 hex) = first 12 chars of sha256(canonical content).
///
/// 输入为 claim（见 `node_claim` / `edge_claim` / `evidence_claim`）或完整实体皆可：
/// 被排除的键会在哈希前统一剔除（含嵌套，如 `experiment.status`）。
pub fn block_hash(content: &Value) -> String {
    let mut claim = content.clone();
    strip_excluded(&mut claim);
    sha256_hex(&canonicalize(&claim))[..12].to_string()
}

/// node claim 字段：id、type、title、body、tags、data。
/// 不含 evidenceIds（悬挂字段）、status、provenance、时间戳。
const NODE_CLAIM_FIELDS: &[&str] = &["id", "type", "title", "body", "tags", "data"];

/// edge claim 字段：id、type、source、target、directed、polarity、confidence、
/// conditions、note、experiment。不含 evidenceIds、provenance。
const EDGE_CLAIM_FIELDS: &[&str] = &[
    "id",
    "type",
    "source",
    "target",
    "directed",
    "polarity",
    "confidence",
    "conditions",
    "note",
    "experiment",
];

/// evidence claim 字段：id、sourceType、sourceId、title、authors、year、doi、url。
/// 不含 locator（fileName/page/section/quote/startOffset/endOffset）、status、provenance。
const EVIDENCE_CLAIM_FIELDS: &[&str] = &[
    "id",
    "sourceType",
    "sourceId",
    "title",
    "authors",
    "year",
    "doi",
    "url",
];

fn pick_fields(entity: &Value, fields: &[&str]) -> Value {
    let mut claim = Map::new();
    if let Some(object) = entity.as_object() {
        for field in fields {
            if let Some(value) = object.get(*field) {
                claim.insert((*field).to_string(), value.clone());
            }
        }
    }
    Value::Object(claim)
}

/// node 主张（claim）：主张=身份，证据=悬挂字段。
/// Node claim: the claim is the identity; evidence is a hanging field.
pub fn node_claim(node: &Value) -> Value {
    pick_fields(node, NODE_CLAIM_FIELDS)
}

/// edge 主张（claim）/ Edge claim.
pub fn edge_claim(edge: &Value) -> Value {
    pick_fields(edge, EDGE_CLAIM_FIELDS)
}

/// evidence 主张（claim）/ Evidence claim.
pub fn evidence_claim(evidence: &Value) -> Value {
    pick_fields(evidence, EVIDENCE_CLAIM_FIELDS)
}

/// 计算全部 ① 区实体的 blockHash（entityId → 12 hex）。
/// Computes block hashes for every ①-zone entity (entityId → 12 hex).
pub fn compute_block_hashes(project: &Value) -> BTreeMap<String, String> {
    let mut hashes = BTreeMap::new();
    for (collection, claim) in [
        ("nodes", node_claim as fn(&Value) -> Value),
        ("edges", edge_claim),
        ("evidence", evidence_claim),
    ] {
        if let Some(entities) = project.get(collection).and_then(Value::as_array) {
            for entity in entities {
                if let Some(id) = entity.get("id").and_then(Value::as_str) {
                    hashes.insert(id.to_string(), block_hash(&claim(entity)));
                }
            }
        }
    }
    hashes
}

/// `contentRootHash`（64 hex）= sha256(sorted(所有 ① 区实体 blockHash) 拼接)。
/// Content root hash = sha256 over the concatenation of the sorted block hashes.
pub fn content_root_hash_from_hashes(block_hashes: &BTreeMap<String, String>) -> String {
    let mut sorted: Vec<&String> = block_hashes.values().collect();
    sorted.sort();
    let mut input = String::with_capacity(sorted.len() * 12);
    for hash in sorted {
        input.push_str(hash);
    }
    sha256_hex(input.as_bytes())
}

/// 由项目直接计算语义区根哈希 / Content root hash straight from a project.
pub fn content_root_hash(project: &Value) -> String {
    content_root_hash_from_hashes(&compute_block_hashes(project))
}

/// `fileHash`（64 hex）= sha256(全文件规范字节，fileHash 字段置空)，git 式自编码。
/// File hash (64 hex) = sha256 over the canonical bytes of the whole file with
/// the `fileHash` field blanked — git-style self-encoding. Hashing the canonical
/// form keeps the hash stable across formatting and key order (§3.4).
pub fn file_hash(project: &Value) -> String {
    let mut blanked = project.clone();
    if let Some(root) = blanked.as_object_mut() {
        root.insert("fileHash".to_string(), Value::String(String::new()));
    }
    sha256_hex(&canonicalize(&blanked))
}
