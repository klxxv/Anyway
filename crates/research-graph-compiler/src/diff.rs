//! Canvas Diff——内核层结构比较（spec GC-12/14, docs/architecture/canvas-diff-design.md）。
//! 确定性结构比较：① 语义区三区（nodes/edges/evidence）独立 diff，基于 blockHash 归约；
//! 输出 added/removed/modified + changedBlockHashes，可直接驱动 GraphPatch 与前端高亮。
//! 约束：确定性（BTreeMap 排序）、幂等（diff(P,P)=空）、对称（added/removed 互换）、
//! O(N log N)、零侵入（仅复用 hash/canonical 原语，不改动 graph_compiler.rs）。

use crate::hash::{block_hash, edge_claim, evidence_claim, node_claim};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::time::Instant;

/// 比较粒度控制 / Diff granularity.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffGranularity {
    /// 仅比较 blockHash（默认，最快）/ Block-hash only (default, fastest).
    BlockHash,
    /// 比较到字段级（较慢但提供精确变更信息）/ Field-level (slower, precise).
    FieldLevel,
}

/// 差异操作类型 / Diff operation kind.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DiffOperation {
    Added,
    Removed,
    Modified,
}

/// 单个字段的精细变化 / One field-level change.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub field: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

/// 一个被修改的实体 / One modified entity.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifiedEntity {
    pub entity_id: String,
    /// 实体类别：nodes / edges / evidence。
    pub entity_kind: String,
    pub old_block_hash: String,
    pub new_block_hash: String,
    /// 具体变更的字段列表（FieldLevel 粒度下填充）。
    pub changed_fields: Vec<String>,
}

/// Canvas Diff 结果——内核层结构比较的完整产物。
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CanvasDiffResult {
    pub added_nodes: Vec<String>,
    pub removed_nodes: Vec<String>,
    pub modified_nodes: Vec<ModifiedEntity>,
    pub added_edges: Vec<String>,
    pub removed_edges: Vec<String>,
    pub modified_edges: Vec<ModifiedEntity>,
    pub added_evidence: Vec<String>,
    pub removed_evidence: Vec<String>,
    pub modified_evidence: Vec<ModifiedEntity>,
    /// changedBlockHashes：entityId → (oldHash, newHash)。
    /// 覆盖 added（old=""）、removed（new=""）、modified（两者不同）。
    /// BTreeMap:序列化字节级确定(D4 确定性头)。
    pub changed_block_hashes: BTreeMap<String, (String, String)>,
    /// Diff 计算耗时（毫秒）/ Elapsed time in milliseconds.
    pub duration_ms: u64,
}

/// 差异块（git-diff-hunk 风格，供前端渲染器 / For the renderer).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub entity_id: String,
    pub entity_kind: String,
    pub operation: DiffOperation,
    pub old_block_hash: String,
    pub new_block_hash: String,
    pub changed_fields: Vec<FieldChange>,
}

/// 超大项目防护（B7）：超过该实体总数直接返回空 diff，防止 OOM。
const MAX_DIFF_ENTITIES: usize = 100_000;

/// 读取分区实体数组；缺失键视为空数组（B1）。
fn entities(project: &Value, collection: &str) -> Vec<Value> {
    project
        .get(collection)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// 按实体类别选取 claim（主张=身份，证据=悬挂字段）。
fn claim_for(kind: &str, entity: &Value) -> Value {
    match kind {
        "nodes" => node_claim(entity),
        "edges" => edge_claim(entity),
        _ => evidence_claim(entity),
    }
}

/// 字段级精细比较：claim 中值规范化后不同的字段。
fn changed_fields(entity1: &Value, entity2: &Value, kind: &str) -> Vec<String> {
    let claim1 = claim_for(kind, entity1);
    let claim2 = claim_for(kind, entity2);
    let empty_map = Map::new();
    let object1 = claim1.as_object().unwrap_or(&empty_map);
    let object2 = claim2.as_object().unwrap_or(&empty_map);
    let mut keys: Vec<&String> = object1
        .keys()
        .chain(object2.keys())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    keys.sort();
    keys.into_iter()
        .filter(|key| {
            let old = object1.get(*key).map(crate::canonical::canonicalize);
            let new = object2.get(*key).map(crate::canonical::canonicalize);
            old != new
        })
        .cloned()
        .collect()
}

/// 单个实体类别的 diff：返回 (added, removed, modified) 并填充 changedBlockHashes。
fn diff_zone(
    v1: &Value,
    v2: &Value,
    kind: &str,
    granularity: DiffGranularity,
    changed: &mut BTreeMap<String, (String, String)>,
) -> (Vec<String>, Vec<String>, Vec<ModifiedEntity>) {
    let entities1 = entities(v1, kind);
    let entities2 = entities(v2, kind);

    // BTreeMap 以 id 为键，迭代即字典序，保证确定性（D4）。
    let mut by_id1: BTreeMap<&str, &Value> = BTreeMap::new();
    let mut by_id2: BTreeMap<&str, &Value> = BTreeMap::new();
    for entity in &entities1 {
        if let Some(id) = entity.get("id").and_then(Value::as_str) {
            by_id1.insert(id, entity);
        }
    }
    for entity in &entities2 {
        if let Some(id) = entity.get("id").and_then(Value::as_str) {
            by_id2.insert(id, entity);
        }
    }

    let mut added = Vec::new();
    let mut removed = Vec::new();
    let mut modified = Vec::new();

    for (id, entity2) in &by_id2 {
        match by_id1.get(id) {
            None => {
                added.push((*id).to_string());
                let hash = block_hash(&claim_for(kind, entity2));
                changed.insert((*id).to_string(), (String::new(), hash));
            }
            Some(entity1) => {
                let old_hash = block_hash(&claim_for(kind, entity1));
                let new_hash = block_hash(&claim_for(kind, entity2));
                if old_hash != new_hash {
                    let fields = if granularity == DiffGranularity::FieldLevel {
                        changed_fields(entity1, entity2, kind)
                    } else {
                        Vec::new()
                    };
                    modified.push(ModifiedEntity {
                        entity_id: (*id).to_string(),
                        entity_kind: kind.to_string(),
                        old_block_hash: old_hash.clone(),
                        new_block_hash: new_hash.clone(),
                        changed_fields: fields,
                    });
                    changed.insert((*id).to_string(), (old_hash, new_hash));
                }
            }
        }
    }
    for (id, entity1) in &by_id1 {
        if !by_id2.contains_key(id) {
            removed.push((*id).to_string());
            let hash = block_hash(&claim_for(kind, entity1));
            changed.insert((*id).to_string(), (hash, String::new()));
        }
    }
    (added, removed, modified)
}

/// 两个 ProjectState 的完整结构差异（默认 blockHash 级别）。
pub fn canvas_diff(v1: &Value, v2: &Value) -> CanvasDiffResult {
    canvas_diff_with_granularity(v1, v2, DiffGranularity::BlockHash)
}

/// 带粒度控制的 diff：FieldLevel 慢但提供精确字段变更信息。
pub fn canvas_diff_with_granularity(
    v1: &Value,
    v2: &Value,
    granularity: DiffGranularity,
) -> CanvasDiffResult {
    let started = Instant::now();
    let mut result = CanvasDiffResult::default();

    let entity_count = ["nodes", "edges", "evidence"]
        .iter()
        .map(|collection| entities(v1, collection).len() + entities(v2, collection).len())
        .sum::<usize>();
    if entity_count > MAX_DIFF_ENTITIES {
        // B7：超限返回空结果，不 panic；command 层另行报错。
        return result;
    }

    let (added_nodes, removed_nodes, modified_nodes) =
        diff_zone(v1, v2, "nodes", granularity, &mut result.changed_block_hashes);
    let (added_edges, removed_edges, modified_edges) =
        diff_zone(v1, v2, "edges", granularity, &mut result.changed_block_hashes);
    let (added_evidence, removed_evidence, modified_evidence) =
        diff_zone(v1, v2, "evidence", granularity, &mut result.changed_block_hashes);

    result.added_nodes = added_nodes;
    result.removed_nodes = removed_nodes;
    result.modified_nodes = modified_nodes;
    result.added_edges = added_edges;
    result.removed_edges = removed_edges;
    result.modified_edges = modified_edges;
    result.added_evidence = added_evidence;
    result.removed_evidence = removed_evidence;
    result.modified_evidence = modified_evidence;
    result.duration_ms = started.elapsed().as_millis() as u64;
    result
}

/// 从 CanvasDiffResult 生成 git-diff hunk 风格的结构化变更列表。
pub fn diff_hunks(v1: &Value, v2: &Value, result: &CanvasDiffResult) -> Vec<DiffHunk> {
    let mut hunks = Vec::new();
    let mut emit_zone = |kind: &str,
                         added: &[String],
                         removed: &[String],
                         modified: &[ModifiedEntity]| {
        for id in added {
            let hash = result
                .changed_block_hashes
                .get(id)
                .map(|(_, new)| new.clone())
                .unwrap_or_default();
            hunks.push(DiffHunk {
                entity_id: id.clone(),
                entity_kind: kind.to_string(),
                operation: DiffOperation::Added,
                old_block_hash: String::new(),
                new_block_hash: hash,
                changed_fields: Vec::new(),
            });
        }
        for entity in modified {
            let fields = if entity.changed_fields.is_empty() && kind != "evidence" {
                let zone1 = entities(v1, kind);
                let zone2 = entities(v2, kind);
                let e1 = zone1
                    .iter()
                    .find(|item| item.get("id").and_then(Value::as_str) == Some(entity.entity_id.as_str()));
                let e2 = zone2
                    .iter()
                    .find(|item| item.get("id").and_then(Value::as_str) == Some(entity.entity_id.as_str()));
                match (e1, e2) {
                    (Some(a), Some(b)) => changed_fields(a, b, kind)
                        .into_iter()
                        .map(|field| {
                            let object1 = claim_for(kind, a).as_object().cloned().unwrap_or_default();
                            let object2 = claim_for(kind, b).as_object().cloned().unwrap_or_default();
                            FieldChange {
                                field: field.clone(),
                                old_value: object1
                                    .get(&field)
                                    .map(crate::canonical::canonicalize)
                                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
                                new_value: object2
                                    .get(&field)
                                    .map(crate::canonical::canonicalize)
                                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
                            }
                        })
                        .collect(),
                    _ => Vec::new(),
                }
            } else {
                Vec::new()
            };
            hunks.push(DiffHunk {
                entity_id: entity.entity_id.clone(),
                entity_kind: kind.to_string(),
                operation: DiffOperation::Modified,
                old_block_hash: entity.old_block_hash.clone(),
                new_block_hash: entity.new_block_hash.clone(),
                changed_fields: fields,
            });
        }
        for id in removed {
            let hash = result
                .changed_block_hashes
                .get(id)
                .map(|(old, _)| old.clone())
                .unwrap_or_default();
            hunks.push(DiffHunk {
                entity_id: id.clone(),
                entity_kind: kind.to_string(),
                operation: DiffOperation::Removed,
                old_block_hash: hash,
                new_block_hash: String::new(),
                changed_fields: Vec::new(),
            });
        }
    };
    emit_zone("nodes", &result.added_nodes, &result.removed_nodes, &result.modified_nodes);
    emit_zone("edges", &result.added_edges, &result.removed_edges, &result.modified_edges);
    emit_zone("evidence", &result.added_evidence, &result.removed_evidence, &result.modified_evidence);
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project() -> Value {
        json!({
            "schemaVersion": 3,
            "id": "p1",
            "title": "Demo",
            "nodes": [
                {"id": "a", "type": "note", "title": "Alpha", "evidenceIds": [], "status": "confirmed"},
                {"id": "b", "type": "note", "title": "Beta", "evidenceIds": []}
            ],
            "edges": [
                {"id": "e1", "type": "causes", "source": "a", "target": "b", "directed": true}
            ],
            "evidence": [
                {"id": "ev1", "sourceType": "paper", "sourceId": "s1", "title": "Study 1",
                 "locator": {"page": 3}, "status": "verified"}
            ],
            "placements": [{"id": "pl1", "nodeId": "a", "x": 0.0, "y": 0.0}]
        })
    }

    #[test]
    fn identical_projects_yield_empty_diff() {
        let project = project();
        let result = canvas_diff(&project, &project);
        assert!(result.added_nodes.is_empty());
        assert!(result.removed_nodes.is_empty());
        assert!(result.modified_nodes.is_empty());
        assert!(result.changed_block_hashes.is_empty());
    }

    #[test]
    fn detects_added_removed_and_modified() {
        let mut v2 = project();
        v2["nodes"].as_array_mut().unwrap().push(json!({"id": "c", "type": "note", "title": "Gamma"}));
        v2["nodes"][0]["title"] = json!("Alpha renamed");
        v2["nodes"].as_array_mut().unwrap().remove(1);
        let result = canvas_diff(&project(), &v2);
        assert_eq!(result.added_nodes, vec!["c"]);
        assert_eq!(result.removed_nodes, vec!["b"]);
        assert_eq!(result.modified_nodes.len(), 1);
        assert_eq!(result.modified_nodes[0].entity_id, "a");
        // 修改节点的 old/new blockHash 必须不同，且已写入 changedBlockHashes。
        let (old_hash, new_hash) = &result.changed_block_hashes["a"];
        assert_ne!(old_hash, new_hash);
        assert_eq!(old_hash, &result.modified_nodes[0].old_block_hash);
        assert_eq!(new_hash, &result.modified_nodes[0].new_block_hash);
        // 新增/移除节点的哈希占位分别为空串。
        assert_eq!(result.changed_block_hashes["c"].0, "");
        assert_eq!(result.changed_block_hashes["b"].1, "");
    }

    #[test]
    fn symmetry_swaps_added_and_removed() {
        let mut v2 = project();
        v2["nodes"].as_array_mut().unwrap().push(json!({"id": "c", "type": "note", "title": "Gamma"}));
        let forward = canvas_diff(&project(), &v2);
        let backward = canvas_diff(&v2, &project());
        assert_eq!(forward.added_nodes, backward.removed_nodes);
        assert_eq!(forward.removed_nodes, backward.added_nodes);
    }

    #[test]
    fn layout_and_editorial_changes_do_not_enter_semantic_diff() {
        let mut v2 = project();
        v2["nodes"][0]["layout"] = json!({"x": 999.0});
        v2["placements"][0]["x"] = json!(500.0);
        v2["title"] = json!("Renamed project");
        let result = canvas_diff(&project(), &v2);
        assert!(result.modified_nodes.is_empty());
        assert!(result.changed_block_hashes.is_empty());
    }

    #[test]
    fn field_level_granularity_reports_changed_fields() {
        let mut v2 = project();
        v2["nodes"][0]["title"] = json!("Alpha renamed");
        v2["nodes"][0]["data"] = json!({"sampleSize": 42});
        let result = canvas_diff_with_granularity(&project(), &v2, DiffGranularity::FieldLevel);
        assert_eq!(result.modified_nodes.len(), 1);
        let fields = &result.modified_nodes[0].changed_fields;
        assert!(fields.contains(&"title".to_string()));
        assert!(fields.contains(&"data".to_string()));
    }

    #[test]
    fn hunks_cover_all_zones_and_operations() {
        let mut v2 = project();
        v2["nodes"].as_array_mut().unwrap().push(json!({"id": "c", "type": "note", "title": "Gamma"}));
        v2["nodes"][0]["title"] = json!("Alpha renamed");
        v2["edges"].as_array_mut().unwrap().clear();
        let result = canvas_diff(&project(), &v2);
        let hunks = diff_hunks(&project(), &v2, &result);
        let ops: Vec<&str> = hunks.iter().map(|hunk| match hunk.operation {
            DiffOperation::Added => "added",
            DiffOperation::Removed => "removed",
            DiffOperation::Modified => "modified",
        }).collect();
        assert!(ops.contains(&"added"));
        assert!(ops.contains(&"removed"));
        assert!(ops.contains(&"modified"));
        assert!(hunks.iter().any(|hunk| hunk.entity_kind == "edges"));
    }
}
