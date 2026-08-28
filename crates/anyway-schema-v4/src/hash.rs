//! Semantic / instance hashing (handoff-spec.md §39, §40, §45, §46).
//!
//! Semantic hashes ignore provenance and identify a scientific object by its
//! canonical content (concept, canonical value, conditioning). Instance hashes
//! fold provenance (document id, evidence refs) on top. All hashing goes
//! through [`canonical_json`], so identical content produces identical bytes
//! regardless of map insertion order or array order (HASH-001, V3-20).
//!
//! The wire form is `sha256:<64 lowercase hex>`.

use serde_json::{Number, Value};
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::state::StateValue;

/// sha256 → 64 lowercase hex chars (no prefix).
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// `sha256:<hex>` over arbitrary bytes.
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("sha256:{}", sha256_hex(bytes))
}

/// `sha256:<hex>` over the canonical JSON bytes of a value.
pub fn hash_json(value: &Value) -> String {
    hash_bytes(&canonical_json(value))
}

/// Canonical JSON serialization (HASH-001): object keys NFC-normalized and
/// sorted; arrays sorted as sets; integral floats rendered without a decimal
/// tail; strings NFC-normalized. Deterministic for identical content.
pub fn canonical_json(value: &Value) -> Vec<u8> {
    match value {
        Value::Null => b"null".to_vec(),
        Value::Bool(true) => b"true".to_vec(),
        Value::Bool(false) => b"false".to_vec(),
        Value::Number(number) => canonical_number(number).into_bytes(),
        Value::String(text) => quoted_string(&text.nfc().collect::<String>()),
        Value::Array(items) => {
            let mut canonical: Vec<Vec<u8>> = items.iter().map(canonical_json).collect();
            canonical.sort();
            let mut out = Vec::with_capacity(canonical.iter().map(Vec::len).sum::<usize>() + 2);
            out.push(b'[');
            for (index, item) in canonical.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(item);
            }
            out.push(b']');
            out
        }
        Value::Object(map) => {
            let mut entries: std::collections::BTreeMap<String, Vec<u8>> =
                std::collections::BTreeMap::new();
            for (key, entry) in map {
                entries.insert(key.nfc().collect(), canonical_json(entry));
            }
            let mut out = Vec::with_capacity(map.len() * 8);
            out.push(b'{');
            for (index, (key, bytes)) in entries.iter().enumerate() {
                if index > 0 {
                    out.push(b',');
                }
                out.extend_from_slice(&quoted_string(key));
                out.push(b':');
                out.extend_from_slice(bytes);
            }
            out.push(b'}');
            out
        }
    }
}

/// The semantic hash of a single canonical value (used for fiber conditioning
/// entries, handoff-spec.md §46).
pub fn value_semantic_hash(value: &StateValue) -> String {
    hash_json(
        &serde_json::to_value(value).expect("StateValue is always JSON-serializable"),
    )
}

/// The instance hash for an object whose semantic hash is already known
/// (handoff-spec.md §40): `H(semanticHash, documentId, evidenceRefs)`.
pub fn instance_hash(semantic_hash: &str, document_id: &str, evidence_refs: &[String]) -> String {
    hash_json(&serde_json::json!({
        "semantic_hash": semantic_hash,
        "document_id": document_id,
        "evidence_refs": evidence_refs,
    }))
}

/// Recursive semantic chain hash (handoff-spec.md §45):
/// `H(h_B0, h_o1, h_B1, h_o2, …, h_Bn)` in path order.
pub fn chain_semantic_hash(block_hashes: &[String], operator_hashes: &[String]) -> String {
    let mut parts: Vec<&str> = Vec::with_capacity(block_hashes.len() + operator_hashes.len());
    parts.push(block_hashes.first().map(String::as_str).unwrap_or(""));
    for (index, operator_hash) in operator_hashes.iter().enumerate() {
        parts.push(operator_hash.as_str());
        parts.push(block_hashes.get(index + 1).map(String::as_str).unwrap_or(""));
    }
    // Length-prefixed, order-preserving, unambiguous encoding.
    let mut input = String::new();
    for part in parts {
        input.push_str(&part.len().to_string());
        input.push(':');
        input.push_str(part);
        input.push(';');
    }
    hash_bytes(input.as_bytes())
}

/// Recursive instance chain hash (handoff-spec.md §45):
/// `H(h_γ^s, sourceExperiments, evidence)`.
pub fn chain_instance_hash(
    chain_semantic_hash: &str,
    source_experiment_refs: &[String],
    evidence_refs: &[String],
) -> String {
    hash_json(&serde_json::json!({
        "semantic_hash": chain_semantic_hash,
        "source_experiment_refs": source_experiment_refs,
        "evidence_refs": evidence_refs,
    }))
}

fn canonical_number(number: &Number) -> String {
    if let Some(float) = number.as_f64() {
        if float.is_finite() && float.fract() == 0.0 && float.abs() < 9_007_199_254_740_992.0 {
            return (float as i64).to_string();
        }
    }
    number.to_string()
}

fn quoted_string(text: &str) -> Vec<u8> {
    serde_json::to_vec(&Value::String(text.to_string())).unwrap_or_else(|_| {
        let mut out = Vec::with_capacity(text.len() + 2);
        out.push(b'"');
        for byte in text.bytes() {
            if byte == b'"' || byte == b'\\' {
                out.push(b'\\');
            }
            out.push(byte);
        }
        out.push(b'"');
        out
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canonical_json_is_order_insensitive() {
        let a = json!({"b": 1, "a": [2, 1]});
        let b = json!({"a": [1, 2], "b": 1});
        assert_eq!(canonical_json(&a), canonical_json(&b));
    }

    #[test]
    fn integral_float_hashes_like_integer() {
        assert_eq!(
            canonical_json(&json!(80.0)),
            canonical_json(&json!(80))
        );
    }

    #[test]
    fn hash_form_is_sha256_prefixed() {
        let hash = hash_json(&json!({"a": 1}));
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), "sha256:".len() + 64);
    }

    #[test]
    fn semantic_hash_ignores_provenance_but_instance_does_not() {
        let same_semantic = value_semantic_hash(&StateValue::Bool(true));
        assert_eq!(
            same_semantic,
            value_semantic_hash(&StateValue::Bool(true))
        );

        let doc_a = instance_hash(&same_semantic, "doc_a", &["ev_1".to_string()]);
        let doc_b = instance_hash(&same_semantic, "doc_b", &["ev_1".to_string()]);
        assert_ne!(doc_a, doc_b);
    }

    #[test]
    fn chain_semantic_hash_is_order_sensitive() {
        let forward = chain_semantic_hash(
            &["sha256:A".to_string(), "sha256:B".to_string()],
            &["sha256:o".to_string()],
        );
        let backward = chain_semantic_hash(
            &["sha256:B".to_string(), "sha256:A".to_string()],
            &["sha256:o".to_string()],
        );
        assert_ne!(forward, backward);
    }
}
