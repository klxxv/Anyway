//! 规范化与双哈希 / Canonicalization and dual hashing (§3 of canvas-format-v3).
//!
//! 规范化(§3.4)：对象键排序（含嵌套 data）、数组规范化后排序、数字规范序列化、
//! 文本 NFC 归一化 + 空白折叠。哈希(§3)：每个 ① 区实体 `blockHash`(12 hex)，
//! 语义区整体 `contentRootHash`(64 hex)，全文件 `fileHash`(64 hex)。
//! 边界定案(E4/E5)：布局、审阅、时间戳、status、证据定位、evidenceIds 不进入语义哈希。
//!
//! 本模块还承载编译管线(§15.1)：不变式检查 → 实体 blockHash → contentRootHash →
//! fileHash，以及 `verify_hashes` 自校验（编辑级联 §3.5）。

use serde::Serialize;
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

use crate::graph_compiler::invariants::check_invariants;

// ---------------------------------------------------------------------------
// 1. 规范化 / Canonicalization (§3.4)
// ---------------------------------------------------------------------------

/// 文本规范化：NFC 归一化 + 折叠空白（任意 Unicode 空白序列 → 单个空格，并去首尾）。
/// Text normalization: NFC + whitespace folding (any run of Unicode whitespace
/// collapses to one space, with leading/trailing whitespace trimmed).
pub fn normalize_text(input: &str) -> String {
    input.nfc().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 键规范化：仅 NFC 归一化，不折叠空白 —— 键是结构标识，合并键会掩盖数据错误。
/// Key normalization: NFC only, no whitespace folding — keys are structural
/// identifiers and merging them could hide data errors.
pub fn normalize_key(input: &str) -> String {
    input.nfc().collect()
}

/// 数字规范序列化：整数值去掉小数尾（1.0 → 1，-0.0 → 0）；非整数用最短往返表示。
/// Canonical number serialization: integral floats drop their decimal tail
/// (1.0 → 1, -0.0 → 0); non-integral floats use the shortest round-trip form.
pub fn canonical_number(number: &Number) -> String {
    if let Some(float) = number.as_f64() {
        // 2^53 以内可精确转换；超出则回退到 serde_json 的原始表示（保留大整数）。
        if float.is_finite() && float.fract() == 0.0 && float.abs() < 9_007_199_254_740_992.0 {
            return (float as i64).to_string();
        }
    }
    number.to_string()
}

/// 值 → 规范 JSON 字节 / Value → canonical JSON bytes:
/// - 对象：键 NFC 归一化后按字典序排序（含嵌套 data），值递归；
/// - 数组：元素规范化后按规范字节排序（顺序不敏感）；
/// - 数字：`canonical_number`；字符串：`normalize_text`。
pub fn canonicalize(value: &Value) -> Vec<u8> {
    match value {
        Value::Null => b"null".to_vec(),
        Value::Bool(true) => b"true".to_vec(),
        Value::Bool(false) => b"false".to_vec(),
        Value::Number(number) => canonical_number(number).into_bytes(),
        Value::String(text) => quoted_string(&normalize_text(text)),
        Value::Array(items) => {
            let mut canonical_items: Vec<Vec<u8>> = items.iter().map(canonicalize).collect();
            canonical_items.sort();
            let mut out = Vec::with_capacity(
                canonical_items.iter().map(Vec::len).sum::<usize>() + 2,
            );
            out.push(b'[');
            for (index, bytes) in canonical_items.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(bytes);
            }
            out.push(b']');
            out
        }
        Value::Object(map) => {
            // 键先 NFC 归一化，再按字典序排序；归一化冲突时后者覆盖，保证规范 JSON 合法。
            let mut entries: Vec<(String, &Value)> = Vec::with_capacity(map.len());
            for (key, entry) in map {
                let normalized = normalize_key(key);
                match entries.iter_mut().find(|(existing, _)| *existing == normalized) {
                    Some((_, existing_value)) => *existing_value = entry,
                    None => entries.push((normalized, entry)),
                }
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let mut out = Vec::with_capacity(map.len() * 8);
            out.push(b'{');
            for (index, (key, entry)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(&quoted_string(key));
                out.push(b':');
                out.extend_from_slice(&canonicalize(entry));
            }
            out.push(b'}');
            out
        }
    }
}

/// 带 JSON 转义的带引号字符串字节 / Quoted, JSON-escaped string bytes.
fn quoted_string(text: &str) -> Vec<u8> {
    serde_json::to_vec(&Value::String(text.to_string())).expect("string serialization cannot fail")
}

// ---------------------------------------------------------------------------
// 2. 双哈希 / Dual hashing (§3)
// ---------------------------------------------------------------------------

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
const EXCLUDED_KEYS: &[&str] = &[
    "derivedFrom",
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
const EVIDENCE_CLAIM_FIELDS: &[&str] =
    &["id", "sourceType", "sourceId", "title", "authors", "year", "doi", "url"];

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
pub fn compute_block_hashes(project: &Value) -> HashMap<String, String> {
    let mut hashes = HashMap::new();
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
pub fn content_root_hash_from_hashes(block_hashes: &HashMap<String, String>) -> String {
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

// ---------------------------------------------------------------------------
// 3. 编译入口 / Compile entry point (§15.1)
// ---------------------------------------------------------------------------

/// 编译产物：注入哈希后的项目 + 哈希明细 + 不变式违规。
/// Compile output: the project with hashes injected, hash details, and
/// invariant violations.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    /// 注入 blockHash / contentRootHash / fileHash 后的项目。
    pub project: Value,
    /// entityId → blockHash(12 hex)。
    pub block_hashes: HashMap<String, String>,
    /// 语义区根哈希（64 hex）/ Semantic zone root hash (64 hex).
    pub content_root_hash: String,
    /// 全文件哈希（64 hex）/ Whole-file hash (64 hex).
    pub file_hash: String,
    pub violations: Vec<crate::graph_compiler::invariants::InvariantViolation>,
}

/// 把 blockHash 注入每个 ① 区实体 / Inject blockHash into every ①-zone entity.
fn inject_block_hashes(project: &mut Value, block_hashes: &HashMap<String, String>) {
    for key in ["nodes", "edges", "evidence"] {
        if let Some(entities) = project.get_mut(key).and_then(Value::as_array_mut) {
            for entity in entities {
                if let Some(object) = entity.as_object_mut() {
                    if let Some(id) = object.get("id").and_then(Value::as_str) {
                        if let Some(hash) = block_hashes.get(id) {
                            object.insert("blockHash".to_string(), Value::String(hash.clone()));
                        }
                    }
                }
            }
        }
    }
}

/// 编译管线（§15.1）：不变式检查 → 实体 blockHash → contentRootHash → fileHash。
/// Compile pipeline: invariants → entity block hashes → content root hash →
/// file hash (git-style self-encoding, the `fileHash` field itself is blanked).
pub fn compile(project: &Value) -> CompileResult {
    let violations = check_invariants(project);
    let block_hashes = compute_block_hashes(project);
    let content_root_hash = content_root_hash_from_hashes(&block_hashes);

    let mut compiled = project.clone();
    inject_block_hashes(&mut compiled, &block_hashes);
    if let Some(root) = compiled.as_object_mut() {
        root.insert("contentRootHash".to_string(), Value::String(content_root_hash.clone()));
    }
    let file_hash = file_hash(&compiled);
    if let Some(root) = compiled.as_object_mut() {
        root.insert("fileHash".to_string(), Value::String(file_hash.clone()));
    }

    CompileResult {
        project: compiled,
        block_hashes,
        content_root_hash,
        file_hash,
        violations,
    }
}

/// 哈希校验结果 / Hash verification result.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub valid: bool,
    pub mismatches: Vec<String>,
}

/// 重新计算全部哈希并与文件内嵌值比对（编辑级联自校验）。
/// Recomputes every hash and compares it with the embedded values — the
/// self-check that catches edit cascades (§3.5).
pub fn verify_hashes(project: &Value) -> VerifyResult {
    let mut mismatches = Vec::new();
    let block_hashes = compute_block_hashes(project);

    for (kind, collection) in [("node", "nodes"), ("edge", "edges"), ("evidence", "evidence")] {
        let entities = project.get(collection).and_then(Value::as_array);
        let empty: Vec<Value> = Vec::new();
        for entity in entities.unwrap_or(&empty) {
            let Some(id) = entity.get("id").and_then(Value::as_str) else {
                continue;
            };
            let expected = block_hashes.get(id);
            let embedded = entity.get("blockHash").and_then(Value::as_str);
            if let (Some(expected), Some(embedded)) = (expected, embedded) {
                if expected != embedded {
                    mismatches.push(format!("{kind}:{id} blockHash mismatch"));
                }
            } else {
                mismatches.push(format!("{kind}:{id} blockHash missing or unhashable"));
            }
        }
    }

    let expected_root = content_root_hash(project);
    match project.get("contentRootHash").and_then(Value::as_str) {
        Some(embedded) if embedded == expected_root => {}
        _ => mismatches.push("contentRootHash mismatch".to_string()),
    }
    let expected_file = file_hash(project);
    match project.get("fileHash").and_then(Value::as_str) {
        Some(embedded) if embedded == expected_file => {}
        _ => mismatches.push("fileHash mismatch".to_string()),
    }

    VerifyResult {
        valid: mismatches.is_empty(),
        mismatches,
    }
}

// ---------------------------------------------------------------------------
// 4. 测试 / Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    pub fn sample_project() -> Value {
        json!({
            "schemaVersion": 2,
            "id": "project-pinn",
            "title": "PINN architecture",
            "discipline": "Physics-informed neural networks",
            "updatedAt": "2026-08-01T00:00:00Z",
            "revision": 1,
            "nodes": [
                {
                    "id": "n1",
                    "type": "question",
                    "title": "主问题",
                    "body": "如何建模?",
                    "tags": ["问题"],
                    "status": "confirmed",
                    "evidenceIds": ["e1"],
                    "data": {"shape": "circle"},
                    "provenance": {"origin": "human", "actorId": "researcher"},
                    "createdAt": "2026-08-01T00:00:00Z",
                    "updatedAt": "2026-08-01T00:00:00Z"
                },
                {
                    "id": "n2",
                    "type": "concept",
                    "title": "先验约束",
                    "body": "物理守恒",
                    "tags": [],
                    "status": "confirmed",
                    "evidenceIds": [],
                    "data": {},
                    "provenance": {"origin": "human"},
                    "createdAt": "2026-08-01T00:00:00Z",
                    "updatedAt": "2026-08-01T00:00:00Z"
                }
            ],
            "edges": [
                {
                    "id": "x1",
                    "type": "supports",
                    "source": "n1",
                    "target": "n2",
                    "directed": true,
                    "polarity": "positive",
                    "confidence": 0.9,
                    "conditions": [],
                    "evidenceIds": ["e1"],
                    "note": "支持关系",
                    "provenance": {"origin": "human"}
                }
            ],
            "evidence": [
                {
                    "id": "e1",
                    "sourceType": "paper",
                    "sourceId": "paper-rope-2024",
                    "title": "RoPE 论文",
                    "authors": "Chen, Rao & Li",
                    "year": 2024,
                    "doi": "10.0000/example.rope",
                    "url": "https://example.org/papers/rope",
                    "locator": {"page": 7, "section": "4.2", "quote": "引文", "startOffset": 1, "endOffset": 2},
                    "status": "verified",
                    "provenance": {"origin": "human"}
                }
            ],
            "placements": [],
            "scenarios": [],
            "activity": []
        })
    }

    #[test]
    fn canonicalize_sorts_object_keys_and_nested_data() {
        let a: Value = serde_json::from_str(r#"{"z":1,"a":{"y":2,"x":3}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"x":3,"y":2},"z":1}"#).unwrap();
        assert_eq!(canonicalize(&a), canonicalize(&b));
        assert_eq!(
            String::from_utf8(canonicalize(&a)).unwrap(),
            r#"{"a":{"x":3,"y":2},"z":1}"#
        );
    }

    #[test]
    fn canonicalize_sorts_array_elements_by_canonical_bytes() {
        let a: Value = serde_json::from_str(r#"[3,1,2]"#).unwrap();
        let b: Value = serde_json::from_str(r#"[1,2,3]"#).unwrap();
        assert_eq!(canonicalize(&a), canonicalize(&b));
        let objects: Value = serde_json::from_str(r#"[{"b":1},{"a":2}]"#).unwrap();
        assert_eq!(
            String::from_utf8(canonicalize(&objects)).unwrap(),
            r#"[{"a":2},{"b":1}]"#
        );
    }

    #[test]
    fn canonicalize_normalizes_numbers() {
        let one_int: Value = serde_json::from_str(r#"{"n":1}"#).unwrap();
        let one_float: Value = serde_json::from_str(r#"{"n":1.0}"#).unwrap();
        assert_eq!(canonicalize(&one_int), canonicalize(&one_float));
        assert_eq!(
            String::from_utf8(canonicalize(&one_int)).unwrap(),
            r#"{"n":1}"#
        );
        let negative_zero: Value = serde_json::from_str(r#"{"n":-0.0}"#).unwrap();
        assert_eq!(
            String::from_utf8(canonicalize(&negative_zero)).unwrap(),
            r#"{"n":0}"#
        );
        let large_integer: Value = serde_json::from_str(r#"{"n":9007199254740993}"#).unwrap();
        assert_eq!(
            String::from_utf8(canonicalize(&large_integer)).unwrap(),
            r#"{"n":9007199254740993}"#
        );
        let fraction: Value = serde_json::from_str(r#"{"n":3.5}"#).unwrap();
        assert_eq!(
            String::from_utf8(canonicalize(&fraction)).unwrap(),
            r#"{"n":3.5}"#
        );
    }

    #[test]
    fn canonicalize_applies_nfc_normalization() {
        let composed: Value = serde_json::from_str(r#"{"text":"\u00e9"}"#).unwrap();
        let decomposed: Value = serde_json::from_str(r#"{"text":"e\u0301"}"#).unwrap();
        assert_eq!(canonicalize(&composed), canonicalize(&decomposed));
        assert_eq!(
            String::from_utf8(canonicalize(&composed)).unwrap(),
            r#"{"text":"é"}"#
        );
    }

    #[test]
    fn canonicalize_folds_whitespace() {
        let value: Value = serde_json::from_str(r#"{"body":"  a\t\n b  c\r\n "}"#).unwrap();
        assert_eq!(
            String::from_utf8(canonicalize(&value)).unwrap(),
            r#"{"body":"a b c"}"#
        );
    }

    #[test]
    fn block_hash_is_12_hex_and_deterministic() {
        let claim = node_claim(&sample_project()["nodes"][0]);
        let hash = block_hash(&claim);
        assert_eq!(hash.len(), 12);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(block_hash(&claim), block_hash(&claim));
    }

    #[test]
    fn block_hash_ignores_hanging_and_editorial_fields() {
        let with_metadata = json!({
            "id": "n1", "type": "question", "title": "Q", "body": "b",
            "tags": [], "data": {},
            "evidenceIds": ["e1"], "status": "confirmed",
            "provenance": {"origin": "human", "reviewedBy": "x"},
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-02-02T00:00:00Z"
        });
        let plain = json!({"id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [], "data": {}});
        assert_eq!(block_hash(&with_metadata), block_hash(&plain));

        // 嵌套 status（experiment.status）同样被剔除。
        let with_experiment_status = json!({
            "id": "x1", "type": "supports", "source": "n1", "target": "n2",
            "directed": true, "polarity": "positive", "conditions": [],
            "experiment": {"id": "exp", "label": "L", "metric": "acc", "outcome": "supports", "status": "completed"}
        });
        let without = json!({
            "id": "x1", "type": "supports", "source": "n1", "target": "n2",
            "directed": true, "polarity": "positive", "conditions": [],
            "experiment": {"id": "exp", "label": "L", "metric": "acc", "outcome": "supports"}
        });
        assert_eq!(block_hash(&with_experiment_status), block_hash(&without));
    }

    #[test]
    fn block_hash_ignores_evidence_locator() {
        let with_locator = json!({
            "id": "e1", "sourceType": "paper", "sourceId": "p-1", "title": "T",
            "locator": {"page": 3, "quote": "q", "startOffset": 1, "endOffset": 2, "section": "s"}
        });
        let plain = json!({"id": "e1", "sourceType": "paper", "sourceId": "p-1", "title": "T"});
        assert_eq!(block_hash(&with_locator), block_hash(&plain));
    }

    #[test]
    fn block_hash_changes_with_content_but_not_with_metadata() {
        let project = sample_project();
        let claim = node_claim(&project["nodes"][0]);
        let mut renamed = project.clone();
        renamed["nodes"][0]["title"] = json!("另一个问题");
        assert_ne!(block_hash(&claim), block_hash(&node_claim(&renamed["nodes"][0])));
        // 键顺序无关：同一对象的字段重排后哈希不变。
        let other_order = json!({"body": "b", "tags": [], "data": {}, "id": "n1", "title": "Q"});
        assert_eq!(
            block_hash(&json!({"id": "n1", "title": "Q", "body": "b", "tags": [], "data": {}})),
            block_hash(&other_order)
        );
    }

    #[test]
    fn content_root_hash_covers_semantics_only() {
        let base = sample_project();

        let mut layout_changed = base.clone();
        layout_changed["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 1, "y": 2, "width": 3, "height": 4}]);
        let mut title_changed = base.clone();
        title_changed["title"] = json!("另一个标题");
        let mut node_changed = base.clone();
        node_changed["nodes"][0]["body"] = json!("编辑后的正文");
        let mut evidence_hanging_changed = base.clone();
        evidence_hanging_changed["nodes"][0]["evidenceIds"] = json!([]);

        assert_eq!(content_root_hash(&base), content_root_hash(&layout_changed));
        assert_eq!(content_root_hash(&base), content_root_hash(&title_changed));
        // evidenceIds 是悬挂字段：增删证据不改变主张哈希。
        assert_eq!(content_root_hash(&base), content_root_hash(&evidence_hanging_changed));
        assert_ne!(content_root_hash(&base), content_root_hash(&node_changed));
        assert_eq!(content_root_hash(&base).len(), 64);
    }

    #[test]
    fn file_hash_is_64_hex_and_covers_layout() {
        let base = sample_project();
        let mut layout_changed = base.clone();
        layout_changed["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 1, "y": 2, "width": 3, "height": 4}]);
        let mut node_changed = base.clone();
        node_changed["nodes"][0]["body"] = json!("编辑后的正文");

        assert_eq!(file_hash(&base).len(), 64);
        assert_eq!(file_hash(&base), file_hash(&base));
        assert_ne!(file_hash(&base), file_hash(&layout_changed));
        assert_ne!(file_hash(&base), file_hash(&node_changed));
    }

    #[test]
    fn file_hash_is_stable_across_key_order() {
        let a: Value = serde_json::from_str(r#"{"z":1,"a":2}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":2,"z":1}"#).unwrap();
        assert_eq!(file_hash(&a), file_hash(&b));
    }

    #[test]
    fn compile_injects_hashes_and_verifies() {
        let result = compile(&sample_project());
        assert_eq!(result.block_hashes.len(), 4, "{:?}", result.block_hashes.keys());
        assert!(result.violations.is_empty(), "{:?}", result.violations);
        assert_eq!(result.content_root_hash.len(), 64);
        assert_eq!(result.file_hash.len(), 64);

        for collection in ["nodes", "edges", "evidence"] {
            for entity in result.project[collection].as_array().unwrap() {
                let hash = entity["blockHash"].as_str().expect("blockHash injected");
                assert_eq!(hash.len(), 12);
            }
        }
        assert_eq!(result.project["contentRootHash"].as_str().unwrap().len(), 64);
        assert_eq!(result.project["fileHash"].as_str().unwrap().len(), 64);

        let verified = verify_hashes(&result.project);
        assert!(verified.valid, "{:?}", verified.mismatches);
    }

    #[test]
    fn verify_catches_edit_cascades() {
        let result = compile(&sample_project());
        // 编辑级联：任一内容变化 → fileHash 失效 → 校验失败。
        let mut edited = result.project.clone();
        edited["nodes"][0]["body"] = json!("编辑后的正文");
        let after = verify_hashes(&edited);
        assert!(!after.valid);
        assert!(after.mismatches.iter().any(|m| m.contains("node:n1")));
        assert!(after.mismatches.iter().any(|m| m == "contentRootHash mismatch"));
        assert!(after.mismatches.iter().any(|m| m == "fileHash mismatch"));

        // 布局变化 → fileHash 失效，但 contentRootHash 与 blockHash 仍有效。
        let mut moved = result.project.clone();
        moved["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 9, "y": 9, "width": 1, "height": 1}]);
        let after_move = verify_hashes(&moved);
        assert!(!after_move.valid);
        assert!(after_move.mismatches.iter().any(|m| m == "fileHash mismatch"));
        assert!(!after_move.mismatches.iter().any(|m| m.contains("blockHash") || m.contains("contentRootHash")));
    }

    #[test]
    fn verify_rejects_raw_project_without_hashes() {
        assert!(!verify_hashes(&sample_project()).valid);
    }

    #[test]
    fn compiled_project_round_trips_through_canonical_form() {
        let result = compile(&sample_project());
        let bytes = canonicalize(&result.project);
        let reparsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(canonicalize(&reparsed), bytes);
        assert!(verify_hashes(&reparsed).valid);
    }
}
