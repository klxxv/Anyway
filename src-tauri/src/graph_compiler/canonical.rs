//! 规范化与双哈希 / Canonicalization and dual hashing (§3, §3.4).
//!
//! 双哈希方案（§3）：每个 ① 区实体有 `blockHash`(12 hex)；语义区整体有
//! `contentRootHash`(64 hex)；全文件有 `fileHash`(64 hex)。
//! 规范化（§3.4）：对象键排序（含嵌套 data）、数组规范化后排序、数字规范
//! 序列化、文本 NFC 归一化 + 空白折叠。编辑级联（§3.5）由此模块驱动。
//!
//! 边界定案（E4/E5）：布局、审阅、时间戳、status、证据定位与 evidenceIds
//! 一律不进入语义哈希 —— 主张=身份，证据=悬挂字段。

use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

/// 文本规范化：NFC 归一化 + 折叠空白（任意 Unicode 空白序列 → 单个空格，并去首尾）。
pub fn normalize_text(input: &str) -> String {
    input.nfc().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 键规范化：仅 NFC 归一化，不折叠空白 —— 键是结构标识，合并键会掩盖数据错误。
pub fn normalize_key(input: &str) -> String {
    input.nfc().collect()
}

/// 数字规范序列化：整数值去掉小数尾（1.0 → 1，-0.0 → 0）；非整数用最短往返表示。
pub fn canonical_number(number: &Number) -> String {
    if let Some(float) = number.as_f64() {
        // 2^53 以内可精确转换；超出则回退到 serde_json 的原始表示（保留大整数）。
        if float.is_finite() && float.fract() == 0.0 && float.abs() < 9_007_199_254_740_992.0 {
            return (float as i64).to_string();
        }
    }
    number.to_string()
}

/// 值 → 规范 JSON 字节：对象键 NFC 后字典序排序（含嵌套 data）、数组按规范字节
/// 排序（顺序不敏感）、数字走 `canonical_number`、字符串走 `normalize_text`。
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

/// 带 JSON 转义的带引号字符串字节。
fn quoted_string(text: &str) -> Vec<u8> {
    serde_json::to_vec(&Value::String(text.to_string())).expect("string serialization cannot fail")
}

/// sha256 → 64 位小写 hex。
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// 被排除出语义哈希的键（§3 边界定案 E4/E5 + 规则 6/7）。
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

/// 递归剔除被排除的键。
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

/// 实体 `blockHash`（12 hex）= sha256(规范化内容) 的前 12 字符。输入为 claim 或
/// 完整实体皆可；被排除的键会在哈希前统一剔除（含嵌套，如 `experiment.status`）。
pub fn block_hash(content: &Value) -> String {
    let mut claim = content.clone();
    strip_excluded(&mut claim);
    sha256_hex(&canonicalize(&claim))[..12].to_string()
}

/// node claim 字段：id、type、title、body、tags、data。不含 evidenceIds（悬挂
/// 字段）、status、provenance、时间戳。
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
pub fn node_claim(node: &Value) -> Value {
    pick_fields(node, NODE_CLAIM_FIELDS)
}

/// edge 主张（claim）。
pub fn edge_claim(edge: &Value) -> Value {
    pick_fields(edge, EDGE_CLAIM_FIELDS)
}

/// evidence 主张（claim）。
pub fn evidence_claim(evidence: &Value) -> Value {
    pick_fields(evidence, EVIDENCE_CLAIM_FIELDS)
}

/// 计算全部 ① 区实体的 blockHash（entityId → 12 hex）。
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
pub fn content_root_hash_from_hashes(block_hashes: &HashMap<String, String>) -> String {
    let mut sorted: Vec<&String> = block_hashes.values().collect();
    sorted.sort();
    let mut input = String::with_capacity(sorted.len() * 12);
    for hash in sorted {
        input.push_str(hash);
    }
    sha256_hex(input.as_bytes())
}

/// 由项目直接计算语义区根哈希。
pub fn content_root_hash(project: &Value) -> String {
    content_root_hash_from_hashes(&compute_block_hashes(project))
}

/// `fileHash`（64 hex）= sha256(全文件规范字节，fileHash 字段置空)，git 式自编码。
/// 规范化形式使哈希跨格式与键序稳定（§3.4）。
pub fn file_hash(project: &Value) -> String {
    let mut blanked = project.clone();
    if let Some(root) = blanked.as_object_mut() {
        root.insert("fileHash".to_string(), Value::String(String::new()));
    }
    sha256_hex(&canonicalize(&blanked))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonicalize_sorts_keys_arrays_numbers_and_text() {
        let a: Value = serde_json::from_str(r#"{"z":1,"a":{"y":2,"x":3}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"x":3,"y":2},"z":1}"#).unwrap();
        assert_eq!(canonicalize(&a), canonicalize(&b));
        let list_a: Value = serde_json::from_str(r#"[{"b":1},{"a":2}]"#).unwrap();
        let list_b: Value = serde_json::from_str(r#"[{"a":2},{"b":1}]"#).unwrap();
        assert_eq!(canonicalize(&list_a), canonicalize(&list_b));
        let one_int: Value = serde_json::from_str(r#"{"n":1}"#).unwrap();
        let one_float: Value = serde_json::from_str(r#"{"n":1.0}"#).unwrap();
        assert_eq!(canonicalize(&one_int), canonicalize(&one_float));
        let composed: Value = serde_json::from_str(r#"{"text":"\u00e9"}"#).unwrap();
        let decomposed: Value = serde_json::from_str(r#"{"text":"e\u0301"}"#).unwrap();
        assert_eq!(canonicalize(&composed), canonicalize(&decomposed));
        let spaced: Value = serde_json::from_str(r#"{"body":"  a\t\n b  c\r\n "}"#).unwrap();
        assert_eq!(
            String::from_utf8(canonicalize(&spaced)).unwrap(),
            r#"{"body":"a b c"}"#
        );
        let negative_zero: Value = serde_json::from_str(r#"{"n":-0.0}"#).unwrap();
        assert_eq!(String::from_utf8(canonicalize(&negative_zero)).unwrap(), r#"{"n":0}"#);
    }

    #[test]
    fn block_hash_is_12_hex_and_ignores_hanging_and_editorial_fields() {
        let with_metadata = json!({
            "id": "n1", "type": "question", "title": "Q", "body": "b",
            "tags": [], "data": {},
            "evidenceIds": ["e1"], "status": "confirmed",
            "provenance": {"origin": "human", "reviewedBy": "x"},
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-02-02T00:00:00Z"
        });
        let plain = json!({"id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [], "data": {}});
        let hash = block_hash(&plain);
        assert_eq!(hash.len(), 12);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(block_hash(&with_metadata), hash);

        // 嵌套 status（experiment.status）与证据 locator 同样被剔除。
        let with_locator = json!({
            "id": "e1", "sourceType": "paper", "sourceId": "p-1", "title": "T",
            "locator": {"page": 3, "quote": "q", "startOffset": 1, "endOffset": 2, "section": "s"}
        });
        let plain_evidence = json!({"id": "e1", "sourceType": "paper", "sourceId": "p-1", "title": "T"});
        assert_eq!(block_hash(&with_locator), block_hash(&plain_evidence));

        // 键顺序无关。
        let other_order = json!({"body": "b", "tags": [], "data": {}, "id": "n1", "title": "Q", "type": "question"});
        assert_eq!(block_hash(&plain), block_hash(&other_order));
    }

    #[test]
    fn root_hashes_cover_semantics_only_and_file_hash_covers_layout() {
        let project = json!({
            "schemaVersion": 2, "id": "p", "title": "T", "discipline": "D",
            "updatedAt": "2026-08-01T00:00:00Z", "revision": 1,
            "nodes": [{"id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [], "data": {}}],
            "edges": [], "evidence": [], "placements": [], "scenarios": [], "activity": []
        });
        let mut layout_changed = project.clone();
        layout_changed["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 1, "y": 2, "width": 3, "height": 4}]);
        let mut node_changed = project.clone();
        node_changed["nodes"][0]["body"] = json!("编辑后的正文");

        assert_eq!(content_root_hash(&project), content_root_hash(&layout_changed));
        assert_ne!(content_root_hash(&project), content_root_hash(&node_changed));
        assert_eq!(content_root_hash(&project).len(), 64);
        assert_eq!(file_hash(&project).len(), 64);
        assert_ne!(file_hash(&project), file_hash(&layout_changed));
        assert_ne!(file_hash(&project), file_hash(&node_changed));
        let key_reordered: Value = serde_json::from_str(r#"{"z":1,"a":2}"#).unwrap();
        assert_eq!(file_hash(&key_reordered), file_hash(&serde_json::from_str::<Value>(r#"{"a":2,"z":1}"#).unwrap()));
    }
}
