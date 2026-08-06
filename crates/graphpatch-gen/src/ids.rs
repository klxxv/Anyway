//! 确定性 ID 生成——从 tempId + 内容 + DOI 生成永久 12 hex ID。
//!
//! 与 research-graph-compiler 中的 block_hash 风格一致（sha256 12 hex）。

use sha2::{Digest, Sha256};

/// 从语义候选生成确定性永久 ID（12 hex）。
///
/// 输入：前缀 + 内容 + DOI。
/// 相同候选在不同运行中产生相同 ID，避免重复添加。
pub fn generate_permanent_id(prefix: &str, content: &str, doi: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prefix.as_bytes());
    hasher.update(b":");
    hasher.update(content.as_bytes());
    hasher.update(b":");
    hasher.update(doi.as_bytes());
    // 6 bytes = 12 hex chars
    bytes_to_hex(&hasher.finalize()[..6])
}

/// 从 ExtractedEntity + DOI 生成永久 node ID。
pub fn entity_node_id(entity_label: &str, entity_text: &str, doi: &str) -> String {
    generate_permanent_id("entity", &format!("{entity_label}:{entity_text}"), doi)
}

/// 从 source_id + target_id + relation_type + DOI 生成永久 edge ID。
pub fn edge_id(source: &str, target: &str, relation_type: &str, doi: &str) -> String {
    generate_permanent_id("edge", &format!("{source}->{target}:{relation_type}"), doi)
}

/// 从 evidence text + DOI 生成永久 evidence ID。
pub fn evidence_id(evidence_text: &str, doi: &str) -> String {
    generate_permanent_id("evidence", evidence_text, doi)
}

/// 将字节转为 hex 字符串。
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 将 tempId 映射的查找表。
/// key: Pass B 分配的 tempId（如 "v1", "cl3"）
/// value: 确定性生成的永久 ID（12 hex）
pub type TempIdMap = std::collections::HashMap<String, String>;

/// 为所有实体生成 tempId → permanent ID 的映射表。
pub fn build_temp_id_map(
    temp_ids: &[String],
    labels_and_texts: &[(String, String)],
    doi: &str,
) -> TempIdMap {
    temp_ids
        .iter()
        .zip(labels_and_texts.iter())
        .map(|(tid, (label, text))| {
            let perm_id = entity_node_id(label, text, doi);
            (tid.clone(), perm_id)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_id_same_input_same_output() {
        let id1 = generate_permanent_id("test", "hello", "doi:123");
        let id2 = generate_permanent_id("test", "hello", "doi:123");
        assert_eq!(id1, id2);
    }

    #[test]
    fn deterministic_id_different_input_different_output() {
        let id1 = generate_permanent_id("test", "hello", "doi:123");
        let id2 = generate_permanent_id("test", "world", "doi:123");
        let id3 = generate_permanent_id("test", "hello", "doi:456");
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn id_length_is_12() {
        let id = generate_permanent_id("var", "learning_rate", "10.0000/test");
        assert_eq!(id.len(), 12);
    }

    #[test]
    fn temp_id_map_builds_correctly() {
        let temp_ids: Vec<String> = vec!["v1".into(), "v2".into()];
        let labels_texts: Vec<(String, String)> = vec![
            ("lr".into(), "learning rate".into()),
            ("bs".into(), "batch size".into()),
        ];
        let map = build_temp_id_map(&temp_ids, &labels_texts, "doi:test");
        assert_eq!(map.len(), 2);
        assert!(map.contains_key("v1"));
        assert!(map.contains_key("v2"));
        assert_ne!(map["v1"], map["v2"]);
    }
}
