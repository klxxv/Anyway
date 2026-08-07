//! 图编译器语义内核 / Semantic kernel of the graph compiler.
//!
//! 模块化门面：规范化+双哈希（`canonical`）、图不变式（`invariants`）、
//! 可达性/路径（`algorithms`）、环/矛盾链/逻辑链/场景比对（`analysis`）、
//! 编译管线与版控差异（本模块）。图属性一律硬计算，绝不由 LLM/代理生成。
//!
//! 编译管线（§15.1）：不变式 → 实体 blockHash → contentRootHash → fileHash。
//! 版控差异（§6）：基于 blockHash 的语义 diff，而非文本 diff —— JSON 键序、
//! 格式变化不产生伪差异；差异可转成可审阅的 GraphPatch 协议。

pub mod algorithms;
pub mod analysis;
pub mod canonical;
pub mod invariants;

pub use algorithms::{TraversalDirection, TraversalRequest, TraversalResult, TraversalStrategy};
pub use analysis::{
    compare_scenario_reachability, compute_logic_chain, contradiction_chains, detect_cycles,
};
pub use canonical::{
    block_hash, canonicalize, compute_block_hashes, content_root_hash, content_root_hash_from_hashes,
    edge_claim, evidence_claim, file_hash, node_claim, normalize_key, normalize_text, sha256_hex,
};
pub use invariants::{check_invariants, InvariantViolation, Severity};

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::HashMap;

/// 编译产物：注入哈希后的项目 + 哈希明细 + 不变式违规。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileResult {
    /// 注入 blockHash / contentRootHash / fileHash 后的项目。
    pub project: Value,
    /// entityId → blockHash(12 hex)。
    pub block_hashes: HashMap<String, String>,
    /// 语义区根哈希（64 hex）。
    pub content_root_hash: String,
    /// 全文件哈希（64 hex）。
    pub file_hash: String,
    pub violations: Vec<InvariantViolation>,
}

/// 哈希校验结果。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub valid: bool,
    pub mismatches: Vec<String>,
}

/// 把 blockHash 注入每个 ① 区实体。
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
pub fn compile(project: &Value) -> CompileResult {
    let violations = invariants::check_invariants(project);
    let block_hashes = canonical::compute_block_hashes(project);
    let content_root_hash = canonical::content_root_hash_from_hashes(&block_hashes);

    let mut compiled = project.clone();
    inject_block_hashes(&mut compiled, &block_hashes);
    if let Some(root) = compiled.as_object_mut() {
        root.insert("contentRootHash".to_string(), Value::String(content_root_hash.clone()));
    }
    let file_hash = canonical::file_hash(&compiled);
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

/// 重新计算全部哈希并与文件内嵌值比对（编辑级联自校验，§3.5）。
pub fn verify_hashes(project: &Value) -> VerifyResult {
    let mut mismatches = Vec::new();
    let block_hashes = canonical::compute_block_hashes(project);

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

    let expected_root = canonical::content_root_hash(project);
    match project.get("contentRootHash").and_then(Value::as_str) {
        Some(embedded) if embedded == expected_root => {}
        _ => mismatches.push("contentRootHash mismatch".to_string()),
    }
    let expected_file = canonical::file_hash(project);
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
// 5. 版控差异 / Version-control diff (§6)
// ---------------------------------------------------------------------------

/// 两个项目版本间的语义差异（基于 blockHash，非文本）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphDiff {
    pub added_node_ids: Vec<String>,
    pub removed_node_ids: Vec<String>,
    pub changed_node_ids: Vec<String>,
    pub added_edge_ids: Vec<String>,
    pub removed_edge_ids: Vec<String>,
    pub changed_edge_ids: Vec<String>,
    pub added_evidence_ids: Vec<String>,
    pub removed_evidence_ids: Vec<String>,
    pub changed_evidence_ids: Vec<String>,
}

/// 单集合的实体 blockHash（id → 12 hex）。
fn hashes_for(project: &Value, collection: &str) -> HashMap<String, String> {
    let claim: fn(&Value) -> Value = match collection {
        "nodes" => canonical::node_claim,
        "edges" => canonical::edge_claim,
        _ => canonical::evidence_claim,
    };
    let mut map = HashMap::new();
    let empty = Vec::new();
    for entity in project.get(collection).and_then(Value::as_array).unwrap_or(&empty) {
        if let Some(id) = entity.get("id").and_then(Value::as_str) {
            map.insert(id.to_string(), canonical::block_hash(&claim(entity)));
        }
    }
    map
}

/// 语义差异：同一 id 在两侧的 blockHash 不同 ⇒ changed；单侧存在 ⇒ added/removed。
pub fn graph_diff(old: &Value, new: &Value) -> GraphDiff {
    let mut diff = GraphDiff {
        added_node_ids: Vec::new(),
        removed_node_ids: Vec::new(),
        changed_node_ids: Vec::new(),
        added_edge_ids: Vec::new(),
        removed_edge_ids: Vec::new(),
        changed_edge_ids: Vec::new(),
        added_evidence_ids: Vec::new(),
        removed_evidence_ids: Vec::new(),
        changed_evidence_ids: Vec::new(),
    };
    for (collection, added, removed, changed) in [
        ("nodes", &mut diff.added_node_ids, &mut diff.removed_node_ids, &mut diff.changed_node_ids),
        ("edges", &mut diff.added_edge_ids, &mut diff.removed_edge_ids, &mut diff.changed_edge_ids),
        ("evidence", &mut diff.added_evidence_ids, &mut diff.removed_evidence_ids, &mut diff.changed_evidence_ids),
    ] {
        let old_hashes = hashes_for(old, collection);
        let new_hashes = hashes_for(new, collection);
        let mut ids: Vec<&String> = old_hashes.keys().chain(new_hashes.keys()).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            match (old_hashes.get(id), new_hashes.get(id)) {
                (None, Some(_)) => added.push(id.clone()),
                (Some(_), None) => removed.push(id.clone()),
                (Some(a), Some(b)) if a != b => changed.push(id.clone()),
                _ => {}
            }
        }
    }
    diff
}

fn find_entity<'a>(project: &'a Value, collection: &str, id: &str) -> Option<&'a Value> {
    project
        .get(collection)
        .and_then(Value::as_array)
        .and_then(|entities| {
            entities
                .iter()
                .find(|entity| entity.get("id").and_then(Value::as_str) == Some(id))
        })
}

/// 新旧实体的 claim 差异字段（update 操作的 changes）。
fn semantic_changes(
    old_entity: &Value,
    new_entity: &Value,
    claim: fn(&Value) -> Value,
) -> Map<String, Value> {
    let mut changes = Map::new();
    if let (Some(old_obj), Some(new_obj)) = (claim(old_entity).as_object(), claim(new_entity).as_object()) {
        for (key, new_value) in new_obj {
            match old_obj.get(key) {
                Some(old_value) if old_value == new_value => {}
                _ => {
                    changes.insert(key.clone(), new_value.clone());
                }
            }
        }
    }
    changes
}

/// 语义 diff → 可审阅 GraphPatch 协议（§6 → `researchcanvas.dev/graph-patch/v1alpha1`）。
/// removed 实体不入协议（协议无删除操作），仅在 summary 中计数。
pub fn graph_patch_from_diff(
    old: &Value,
    new: &Value,
    plugin_id: &str,
    operation: &str,
    title: &str,
) -> Value {
    let diff = graph_diff(old, new);
    let mut operations: Vec<Value> = Vec::new();
    for id in &diff.added_node_ids {
        if let Some(node) = find_entity(new, "nodes", id) {
            operations.push(json!({ "op": "add-node", "node": node.clone() }));
        }
    }
    for id in &diff.added_edge_ids {
        if let Some(edge) = find_entity(new, "edges", id) {
            operations.push(json!({ "op": "add-edge", "edge": edge.clone() }));
        }
    }
    for id in &diff.changed_node_ids {
        if let (Some(old_entity), Some(new_entity)) =
            (find_entity(old, "nodes", id), find_entity(new, "nodes", id))
        {
            let changes = semantic_changes(old_entity, new_entity, canonical::node_claim);
            if !changes.is_empty() {
                operations.push(json!({ "op": "update-node", "nodeId": id, "changes": changes }));
            }
        }
    }
    for id in &diff.changed_edge_ids {
        if let (Some(old_entity), Some(new_entity)) =
            (find_entity(old, "edges", id), find_entity(new, "edges", id))
        {
            let changes = semantic_changes(old_entity, new_entity, canonical::edge_claim);
            if !changes.is_empty() {
                operations.push(json!({ "op": "update-edge", "edgeId": id, "changes": changes }));
            }
        }
    }
    let added = diff.added_node_ids.len() + diff.added_edge_ids.len();
    let removed =
        diff.removed_node_ids.len() + diff.removed_edge_ids.len() + diff.removed_evidence_ids.len();
    let changed = diff.changed_node_ids.len()
        + diff.changed_edge_ids.len()
        + diff.changed_evidence_ids.len();
    json!({
        "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
        "source": { "pluginId": plugin_id, "operation": operation },
        "title": title,
        "summary": format!("{added} added, {changed} changed, {removed} removed across nodes, edges and evidence"),
        "reviewRequired": true,
        "operations": operations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_project() -> Value {
        json!({
            "schemaVersion": 2, "id": "project-pinn", "title": "PINN architecture",
            "discipline": "Physics-informed neural networks",
            "updatedAt": "2026-08-01T00:00:00Z", "revision": 1,
            "nodes": [
                {"id": "n1", "type": "question", "title": "主问题", "body": "如何建模?", "tags": ["问题"], "status": "confirmed", "evidenceIds": ["e1"], "data": {"shape": "circle"}, "provenance": {"origin": "human"}, "createdAt": "2026-08-01T00:00:00Z", "updatedAt": "2026-08-01T00:00:00Z"},
                {"id": "n2", "type": "concept", "title": "先验约束", "body": "物理守恒", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "2026-08-01T00:00:00Z", "updatedAt": "2026-08-01T00:00:00Z"}
            ],
            "edges": [
                {"id": "x1", "type": "supports", "source": "n1", "target": "n2", "directed": true, "polarity": "positive", "confidence": 0.9, "conditions": [], "evidenceIds": ["e1"], "note": "支持关系", "provenance": {"origin": "human"}}
            ],
            "evidence": [
                {"id": "e1", "sourceType": "paper", "sourceId": "paper-rope-2024", "title": "RoPE 论文", "authors": "Chen, Rao & Li", "year": 2024, "doi": "10.0000/example.rope", "url": "https://example.org/papers/rope", "locator": {"page": 7, "section": "4.2", "quote": "引文", "startOffset": 1, "endOffset": 2}, "status": "verified", "provenance": {"origin": "human"}}
            ],
            "placements": [], "scenarios": [], "activity": []
        })
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
                assert_eq!(entity["blockHash"].as_str().unwrap().len(), 12);
            }
        }
        assert!(verify_hashes(&result.project).valid);
    }

    #[test]
    fn verify_catches_edit_cascades() {
        let result = compile(&sample_project());
        let mut edited = result.project.clone();
        edited["nodes"][0]["body"] = json!("编辑后的正文");
        let after = verify_hashes(&edited);
        assert!(!after.valid);
        assert!(after.mismatches.iter().any(|m| m.contains("node:n1")));
        assert!(after.mismatches.iter().any(|m| m == "contentRootHash mismatch"));
        assert!(after.mismatches.iter().any(|m| m == "fileHash mismatch"));

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
    fn graph_diff_detects_add_remove_and_change_across_collections() {
        let base = sample_project();
        let mut next = base.clone();
        next["nodes"].as_array_mut().unwrap().push(json!({
            "id": "n3", "type": "concept", "title": "新增", "body": "b", "tags": [], "data": {},
            "status": "draft", "evidenceIds": [], "provenance": {}
        }));
        next["edges"].as_array_mut().unwrap().push(json!({
            "id": "x2", "type": "derived_from", "source": "n2", "target": "n3", "directed": true,
            "polarity": "positive", "confidence": 0.7, "conditions": [], "evidenceIds": [], "provenance": {}
        }));
        next["nodes"].as_array_mut().unwrap().remove(0); // 删除 n1
        next["nodes"][0]["title"] = json!("改名"); // 变更 n2（仍在 index 0）
        next["edges"][0]["confidence"] = json!(0.5); // 变更 x1
        next["evidence"].as_array_mut().unwrap().push(json!({
            "id": "e2", "sourceType": "note", "sourceId": "s", "title": "T2", "status": "candidate", "provenance": {}
        }));

        let diff = graph_diff(&base, &next);
        assert_eq!(diff.added_node_ids, vec!["n3"]);
        assert_eq!(diff.removed_node_ids, vec!["n1"]);
        assert_eq!(diff.changed_node_ids, vec!["n2"]);
        assert_eq!(diff.added_edge_ids, vec!["x2"]);
        assert_eq!(diff.changed_edge_ids, vec!["x1"]);
        assert_eq!(diff.added_evidence_ids, vec!["e2"]);
    }

    #[test]
    fn graph_diff_ignores_layout_and_editorial_changes() {
        let base = sample_project();
        let mut next = base.clone();
        next["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 1, "y": 2, "width": 3, "height": 4}]);
        next["nodes"][0]["status"] = json!("disputed"); // 编辑性字段
        let diff = graph_diff(&base, &next);
        assert_eq!(diff.changed_node_ids, Vec::<String>::new());
        assert!(diff.added_node_ids.is_empty() && diff.removed_node_ids.is_empty());
    }

    #[test]
    fn graph_patch_from_diff_produces_reviewable_operations() {
        let base = sample_project();
        let mut next = base.clone();
        next["nodes"].as_array_mut().unwrap().push(json!({
            "id": "n3", "type": "concept", "title": "新增", "body": "b", "tags": [], "data": {},
            "status": "draft", "evidenceIds": [], "provenance": {}
        }));
        next["nodes"][0]["body"] = json!("改过的正文");
        let patch = graph_patch_from_diff(&base, &next, "researchcanvas.git-workspace", "git-history-import", "Git history research graph");
        assert_eq!(patch["apiVersion"], "researchcanvas.dev/graph-patch/v1alpha1");
        assert_eq!(patch["reviewRequired"], true);
        let operations = patch["operations"].as_array().unwrap();
        assert!(operations.iter().any(|op| op["op"] == "add-node" && op["node"]["id"] == "n3"));
        let update = operations.iter().find(|op| op["op"] == "update-node").expect("update-node op");
        assert_eq!(update["nodeId"], "n1");
        assert_eq!(update["changes"]["body"], "改过的正文");
        // 布局/编辑性变化不产生操作。
        let mut layout_only = base.clone();
        layout_only["placements"] = json!([{"id": "pl", "viewId": "v", "nodeId": "n1", "x": 5, "y": 5, "width": 1, "height": 1}]);
        let no_op = graph_patch_from_diff(&base, &layout_only, "p", "op", "t");
        assert!(no_op["operations"].as_array().unwrap().is_empty());
    }
}
