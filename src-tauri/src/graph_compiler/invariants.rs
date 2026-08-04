//! 图不变式检查 / Graph invariant checks.
//!
//! 覆盖：dangling 引用 / id 唯一性（跨集合全局）/ 证据接地完整性 / 边极性一致性 /
//! 布局区 placement 引用 / 场景区禁用与覆盖引用 / 导航区最近与固定节点引用。
//! 编译管线（§15.1）在 `canonical::compile` 中调用本模块。

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// 违规严重度 / Violation severity.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// 一条图不变式违规 / A single graph-invariant violation.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InvariantViolation {
    /// 机器可读代码 / Machine-readable code, e.g. "dangling-node-reference".
    pub code: String,
    pub severity: Severity,
    /// 违规实体位置，如 "edge:x1" / Entity location, e.g. "edge:x1".
    pub entity: String,
    pub message: String,
}

impl InvariantViolation {
    fn new(code: &str, severity: Severity, entity: &str, message: String) -> Self {
        Self {
            code: code.to_string(),
            severity,
            entity: entity.to_string(),
            message,
        }
    }
}

/// 检查 id 唯一性（全局，跨集合）并报告缺失 id。
fn collect_ids(
    entities: &[Value],
    location: &str,
    seen: &mut HashMap<String, String>,
    violations: &mut Vec<InvariantViolation>,
) {
    for entity in entities {
        match entity.get("id").and_then(Value::as_str) {
            Some(id) => {
                if let Some(previous) = seen.insert(id.to_string(), location.to_string()) {
                    violations.push(InvariantViolation::new(
                        "duplicate-id",
                        Severity::Error,
                        location,
                        format!("id {id:?} already used by {previous}"),
                    ));
                }
            }
            None => violations.push(InvariantViolation::new(
                "missing-id",
                Severity::Error,
                location,
                "entity has no string id".to_string(),
            )),
        }
    }
}

/// 检查实体的 evidenceIds 是否全部可解析（证据接地完整性的另一半）。
fn check_evidence_ids(
    entity: &Value,
    location: &str,
    evidence_ids: &HashSet<&str>,
    violations: &mut Vec<InvariantViolation>,
) {
    if let Some(ids) = entity.get("evidenceIds").and_then(Value::as_array) {
        for value in ids {
            if let Some(id) = value.as_str() {
                if !evidence_ids.contains(id) {
                    violations.push(InvariantViolation::new(
                        "unresolved-evidence-reference",
                        Severity::Error,
                        location,
                        format!("{location} references missing evidence {id:?}"),
                    ));
                }
            }
        }
    }
}

/// 边极性一致性：值合法 + 语义与类型匹配。
fn check_polarity(edge: &Value, location: &str, violations: &mut Vec<InvariantViolation>) {
    const VALID: &[&str] = &["positive", "negative", "mixed", "unknown"];
    let Some(polarity) = edge.get("polarity").and_then(Value::as_str) else {
        return;
    };
    let edge_type = edge.get("type").and_then(Value::as_str).unwrap_or("");
    if !VALID.contains(&polarity) {
        violations.push(InvariantViolation::new(
            "polarity-conflict",
            Severity::Error,
            location,
            format!("{location} has invalid polarity {polarity:?}"),
        ));
        return;
    }
    match edge_type {
        "contradicts" => match polarity {
            "positive" => violations.push(InvariantViolation::new(
                "polarity-conflict",
                Severity::Error,
                location,
                format!("contradicts edge {location} cannot have positive polarity"),
            )),
            "mixed" => violations.push(InvariantViolation::new(
                "polarity-conflict",
                Severity::Warning,
                location,
                format!("contradicts edge {location} has mixed polarity"),
            )),
            _ => {}
        },
        "supports" | "derived_from" | "uses" | "measures" => {
            if polarity == "negative" {
                violations.push(InvariantViolation::new(
                    "polarity-conflict",
                    Severity::Error,
                    location,
                    format!("{edge_type} edge {location} cannot have negative polarity"),
                ));
            }
        }
        _ => {}
    }
}

/// 图不变式检查：dangling 引用 / id 唯一性 / 证据接地完整性 / 边极性一致性。
/// Graph invariant checks: dangling references, id uniqueness, evidence
/// grounding completeness, and edge polarity consistency.
pub fn check_invariants(project: &Value) -> Vec<InvariantViolation> {
    let mut violations = Vec::new();

    let Some(root) = project.as_object() else {
        violations.push(InvariantViolation::new(
            "malformed-project",
            Severity::Error,
            "project",
            "project root must be a JSON object".to_string(),
        ));
        return violations;
    };

    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();
    let mut evidence: Vec<Value> = Vec::new();
    for (key, out) in [
        ("nodes", &mut nodes),
        ("edges", &mut edges),
        ("evidence", &mut evidence),
    ] {
        match root.get(key).and_then(Value::as_array) {
            Some(items) => *out = items.clone(),
            None => violations.push(InvariantViolation::new(
                "malformed-project",
                Severity::Error,
                key,
                format!("{key} must be an array"),
            )),
        }
    }

    // id 唯一性（跨集合全局唯一）/ Global id uniqueness across collections.
    let mut seen: HashMap<String, String> = HashMap::new();
    collect_ids(&nodes, "nodes", &mut seen, &mut violations);
    collect_ids(&edges, "edges", &mut seen, &mut violations);
    collect_ids(&evidence, "evidence", &mut seen, &mut violations);
    if let Some(placements) = root.get("placements").and_then(Value::as_array) {
        collect_ids(placements, "placements", &mut seen, &mut violations);
    }
    if let Some(scenarios) = root.get("scenarios").and_then(Value::as_array) {
        collect_ids(scenarios, "scenarios", &mut seen, &mut violations);
    }

    let node_ids: HashSet<&str> = nodes
        .iter()
        .filter_map(|node| node.get("id").and_then(Value::as_str))
        .collect();
    let edge_ids: HashSet<&str> = edges
        .iter()
        .filter_map(|edge| edge.get("id").and_then(Value::as_str))
        .collect();
    let evidence_ids: HashSet<&str> = evidence
        .iter()
        .filter_map(|record| record.get("id").and_then(Value::as_str))
        .collect();

    // 边：dangling 端点 + 证据接地 + 极性。
    for edge in &edges {
        let id = edge.get("id").and_then(Value::as_str).unwrap_or("?");
        let location = format!("edge:{id}");
        for field in ["source", "target"] {
            if let Some(referenced) = edge.get(field).and_then(Value::as_str) {
                if !node_ids.contains(referenced) {
                    violations.push(InvariantViolation::new(
                        "dangling-node-reference",
                        Severity::Error,
                        &location,
                        format!("edge {id:?} references missing node {referenced:?} via {field}"),
                    ));
                }
            }
        }
        check_evidence_ids(edge, &location, &evidence_ids, &mut violations);
        check_polarity(edge, &location, &mut violations);
    }

    // 节点：证据接地。
    for node in &nodes {
        let id = node.get("id").and_then(Value::as_str).unwrap_or("?");
        let location = format!("node:{id}");
        check_evidence_ids(node, &location, &evidence_ids, &mut violations);
    }

    // 证据接地完整性：每条证据至少被一个节点或边引用。
    let mut cited: HashSet<&str> = HashSet::new();
    for entity in nodes.iter().chain(edges.iter()) {
        if let Some(ids) = entity.get("evidenceIds").and_then(Value::as_array) {
            for value in ids {
                if let Some(id) = value.as_str() {
                    cited.insert(id);
                }
            }
        }
    }
    for record in &evidence {
        if let Some(id) = record.get("id").and_then(Value::as_str) {
            if !cited.contains(id) {
                violations.push(InvariantViolation::new(
                    "uncited-evidence",
                    Severity::Warning,
                    &format!("evidence:{id}"),
                    format!("evidence {id:?} is never cited by any node or edge"),
                ));
            }
        }
    }

    // 布局区：placement.nodeId 必须指向存在的节点。
    if let Some(placements) = root.get("placements").and_then(Value::as_array) {
        for placement in placements {
            let id = placement.get("id").and_then(Value::as_str).unwrap_or("?");
            if let Some(node_id) = placement.get("nodeId").and_then(Value::as_str) {
                if !node_ids.contains(node_id) {
                    violations.push(InvariantViolation::new(
                        "dangling-node-reference",
                        Severity::Error,
                        &format!("placement:{id}"),
                        format!("placement {id:?} references missing node {node_id:?}"),
                    ));
                }
            }
        }
    }

    // 场景区：禁用/覆盖引用必须存在。
    if let Some(scenarios) = root.get("scenarios").and_then(Value::as_array) {
        for scenario in scenarios {
            let id = scenario.get("id").and_then(Value::as_str).unwrap_or("?");
            let location = format!("scenario:{id}");
            if let Some(disabled) = scenario.get("disabledNodeIds").and_then(Value::as_array) {
                for value in disabled {
                    if let Some(node_id) = value.as_str() {
                        if !node_ids.contains(node_id) {
                            violations.push(InvariantViolation::new(
                                "dangling-node-reference",
                                Severity::Error,
                                &location,
                                format!("scenario {id:?} disables missing node {node_id:?}"),
                            ));
                        }
                    }
                }
            }
            if let Some(disabled) = scenario.get("disabledEdgeIds").and_then(Value::as_array) {
                for value in disabled {
                    if let Some(edge_id) = value.as_str() {
                        if !edge_ids.contains(edge_id) {
                            violations.push(InvariantViolation::new(
                                "dangling-edge-reference",
                                Severity::Error,
                                &location,
                                format!("scenario {id:?} disables missing edge {edge_id:?}"),
                            ));
                        }
                    }
                }
            }
            for (key, set, code, label) in [
                ("nodeOverrides", &node_ids, "dangling-node-reference", "node"),
                ("edgeOverrides", &edge_ids, "dangling-edge-reference", "edge"),
            ] {
                if let Some(overrides) = scenario.get(key).and_then(Value::as_object) {
                    for override_id in overrides.keys() {
                        if !set.contains(override_id.as_str()) {
                            violations.push(InvariantViolation::new(
                                code,
                                Severity::Error,
                                &location,
                                format!(
                                    "scenario {id:?} overrides missing {label} {override_id:?}"
                                ),
                            ));
                        }
                    }
                }
            }
        }
    }

    // 导航区：最近/固定节点必须存在。
    if let Some(navigation) = root.get("navigation").and_then(Value::as_object) {
        for key in ["recentNodeIds", "pinnedNodeIds"] {
            if let Some(ids) = navigation.get(key).and_then(Value::as_array) {
                for value in ids {
                    if let Some(node_id) = value.as_str() {
                        if !node_ids.contains(node_id) {
                            violations.push(InvariantViolation::new(
                                "dangling-node-reference",
                                Severity::Error,
                                "navigation",
                                format!("navigation.{key} references missing node {node_id:?}"),
                            ));
                        }
                    }
                }
            }
        }
    }

    violations
}

// ---------------------------------------------------------------------------
// 测试 / Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn invariants_report_dangling_references_and_duplicates() {
        let broken = json!({
            "schemaVersion": 2, "id": "p", "title": "T", "discipline": "D",
            "updatedAt": "2026-01-01T00:00:00Z", "revision": 1,
            "nodes": [
                {"id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [], "data": {}, "evidenceIds": ["ghost-ev"], "status": "confirmed", "provenance": {}},
                {"id": "n1", "type": "concept", "title": "Dup", "body": "b", "tags": [], "data": {}, "evidenceIds": [], "status": "draft", "provenance": {}},
                {"type": "note", "title": "无 id", "body": "b", "tags": [], "data": {}, "evidenceIds": [], "status": "draft", "provenance": {}}
            ],
            "edges": [
                {"id": "x1", "type": "contradicts", "source": "ghost", "target": "n1", "directed": true, "polarity": "positive", "confidence": 0.9, "conditions": [], "evidenceIds": [], "provenance": {}},
                {"id": "x2", "type": "supports", "source": "n1", "target": "n1", "directed": true, "polarity": "negative", "conditions": [], "evidenceIds": [], "provenance": {}}
            ],
            "evidence": [
                {"id": "e1", "sourceType": "paper", "sourceId": "p", "title": "T", "status": "confirmed", "provenance": {}}
            ],
            "placements": [{"id": "pl", "viewId": "v", "nodeId": "ghost", "x": 0, "y": 0, "width": 1, "height": 1}],
            "scenarios": [{"id": "s1", "name": "S", "disabledNodeIds": ["ghost"], "disabledEdgeIds": ["ghost-edge"], "nodeOverrides": {"ghost2": {}}, "edgeOverrides": {}, "parameters": {}, "hypothesis": "h", "expectedEffect": "e", "createdAt": "x"}],
            "activity": []
        });
        let violations = check_invariants(&broken);
        let codes: Vec<&str> = violations.iter().map(|v| v.code.as_str()).collect();
        assert!(codes.contains(&"duplicate-id"), "{codes:?}");
        assert!(codes.contains(&"missing-id"), "{codes:?}");
        assert!(codes.contains(&"dangling-node-reference"), "{codes:?}");
        assert!(codes.contains(&"dangling-edge-reference"), "{codes:?}");
        assert!(codes.contains(&"unresolved-evidence-reference"), "{codes:?}");
        assert!(codes.contains(&"uncited-evidence"), "{codes:?}");
        assert!(codes.contains(&"polarity-conflict"), "{codes:?}");
        // dangling 位置标注正确。
        assert!(violations.iter().any(|v| v.entity == "edge:x1" && v.code == "dangling-node-reference"));
        assert!(violations.iter().any(|v| v.entity == "placement:pl" && v.code == "dangling-node-reference"));
    }

    #[test]
    fn clean_project_has_no_invariant_violations() {
        let violations = check_invariants(&super::super::canonical::tests::sample_project());
        assert!(violations.is_empty(), "{violations:?}");
    }
}
