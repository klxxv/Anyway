//! 确定性 ID 生成——从 tempId + 内容 + DOI 生成永久 12 hex ID。
//!
//! 与 research-graph-compiler 中的 block_hash 风格一致：先规范化（canonicalize）
//! 内容字段，再用 sha256 取前 12 hex。字段之间用不可能出现在 JSON 输出中的
//! NUL 字节分隔，消除冒号/箭头分隔符的歧义拼接问题。

use sha2::{Digest, Sha256};

/// 稳定的字段分隔符（JSON 规范字节中不可能出现 NUL）。
const ID_DELIMITER: &[u8] = b"\0";

/// 从语义候选生成确定性永久 ID（12 hex）。
///
/// 输入：`prefix` 为类别标签（entity/edge/evidence），`content` 为参与哈希的
/// 内容对象，`doi` 为作用域标识。所有值先经 `research_graph_compiler::canonicalize`
/// 规范化，再用 NUL 分隔后 sha256，避免字段内容注入导致的碰撞。
pub fn generate_permanent_id(prefix: &str, content: serde_json::Value, doi: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(research_graph_compiler::canonicalize(&serde_json::Value::String(
        prefix.to_string(),
    )));
    hasher.update(ID_DELIMITER);
    hasher.update(research_graph_compiler::canonicalize(&content));
    hasher.update(ID_DELIMITER);
    hasher.update(research_graph_compiler::canonicalize(&serde_json::Value::String(
        doi.to_string(),
    )));
    // 6 bytes = 12 hex chars
    bytes_to_hex(&hasher.finalize()[..6])
}

/// 从 ExtractedEntity + DOI 生成永久 node ID。
pub fn entity_node_id(entity_label: &str, entity_text: &str, doi: &str) -> String {
    generate_permanent_id(
        "entity",
        serde_json::json!({ "label": entity_label, "text": entity_text }),
        doi,
    )
}

/// 从 source_id + target_id + relation_type + DOI 生成永久 edge ID。
pub fn edge_id(source: &str, target: &str, relation_type: &str, doi: &str) -> String {
    generate_permanent_id(
        "edge",
        serde_json::json!({
            "source": source,
            "target": target,
            "relation": relation_type,
        }),
        doi,
    )
}

/// 从 evidence text + DOI 生成永久 evidence ID。
pub fn evidence_id(evidence_text: &str, doi: &str) -> String {
    generate_permanent_id(
        "evidence",
        serde_json::json!({ "text": evidence_text }),
        doi,
    )
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
        let id1 = generate_permanent_id("test", serde_json::json!("hello"), "doi:123");
        let id2 = generate_permanent_id("test", serde_json::json!("hello"), "doi:123");
        assert_eq!(id1, id2);
    }

    #[test]
    fn deterministic_id_different_input_different_output() {
        let id1 = generate_permanent_id("test", serde_json::json!("hello"), "doi:123");
        let id2 = generate_permanent_id("test", serde_json::json!("world"), "doi:123");
        let id3 = generate_permanent_id("test", serde_json::json!("hello"), "doi:456");
        assert_ne!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn id_length_is_12() {
        let id = generate_permanent_id("var", serde_json::json!("learning_rate"), "10.0000/test");
        assert_eq!(id.len(), 12);
    }

    #[test]
    fn delimiter_is_injectionsafe() {
        // 旧方案："a:b" + "c" 与 "a" + ":bc" 会碰撞；新方案不会。
        let id1 = generate_permanent_id(
            "entity",
            serde_json::json!({ "label": "a:b", "text": "c" }),
            "doi",
        );
        let id2 = generate_permanent_id(
            "entity",
            serde_json::json!({ "label": "a", "text": ":bc" }),
            "doi",
        );
        assert_ne!(id1, id2);
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
