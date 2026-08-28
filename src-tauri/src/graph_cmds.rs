//! 图编译器 Tauri 命令边界 / Tauri command boundary for the graph compiler.
//!
//! 语义内核（graph_compiler）是纯 Rust 库；本模块是它到 Webview 的唯一桥：
//! 参数经 JSON 校验后交给纯函数，结果序列化为 camelCase JSON。不持有状态。

use serde_json::Value;

/// 解析 diff 输入：接受完整 ProjectState JSON，或 `{ fileHash, project }` 包装。
/// 包装形式会校验 fileHash 与 project 内容一致（git 式自编码自校验），
/// 纯 fileHash 字符串不可逆，拒绝并提示。
fn resolve_diff_input(value: &Value) -> Result<Value, String> {
    if let Some(object) = value.as_object() {
        if let Some(project) = object.get("project") {
            let expected = object
                .get("fileHash")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    "diff input { fileHash, project } requires a fileHash string".to_string()
                })?;
            let actual = research_graph_compiler::hash::file_hash(project);
            if actual != expected {
                return Err(format!(
                    "diff input fileHash mismatch: expected {expected}, computed {actual}"
                ));
            }
            return Ok(project.clone());
        }
        if object.contains_key("nodes") || object.contains_key("edges") {
            return Ok(value.clone());
        }
    }
    if value.is_string() {
        return Err("diff input cannot be a bare fileHash: it is not reversible; pass the ProjectState JSON or { fileHash, project }".to_string());
    }
    Err("diff input must be a ProjectState JSON object or { fileHash, project }".to_string())
}

/// 两个版本的结构化 diff（内核层 Canvas Diff）。
/// 每个参数接受完整 ProjectState JSON，或带 fileHash 自校验的 `{ fileHash, project }` 包装。
/// 返回 CanvasDiffResult：addedNodes / removedNodes / modifiedNodes / changedBlockHashes /
/// addedEdges / removedEdges 等（camelCase，见 canvas-diff-design.md §2.1）。
#[tauri::command]
pub fn compute_diff(v1: Value, v2: Value) -> Result<Value, String> {
    let project_v1 = resolve_diff_input(&v1)?;
    let project_v2 = resolve_diff_input(&v2)?;
    let entity_count = ["nodes", "edges", "evidence"]
        .iter()
        .map(|collection| {
            project_v1
                .get(*collection)
                .and_then(Value::as_array)
                .map(|array| array.len())
                .unwrap_or(0)
                + project_v2
                    .get(*collection)
                    .and_then(Value::as_array)
                    .map(|array| array.len())
                    .unwrap_or(0)
        })
        .sum::<usize>();
    if entity_count > 100_000 {
        return Err(format!(
            "project too large for diff: {entity_count} entities exceeds the 100k limit"
        ));
    }
    let result = research_graph_compiler::diff::canvas_diff(&project_v1, &project_v2);
    serde_json::to_value(result).map_err(|error| error.to_string())
}

/// 确定性布局：给定项目 + 模式 + 可选根节点与参数，返回计算后的展示结果。
/// Deterministic layout: given a project, mode, optional root id and params,
/// return the computed presentation result (positions / annotations / node & edge ids).
#[tauri::command]
#[allow(dead_code)] // superseded by compile_project + frontend layout; retained for compiler parity
pub fn compute_graph_layout(
    project: Value,
    mode: String,
    root_id: Option<String>,
    params: Option<Value>,
) -> Result<Value, String> {
    let params = crate::graph_compiler::layout::resolve_params(&mode, params.as_ref());
    let result = crate::graph_compiler::layout::compute_layout(
        &project,
        &mode,
        root_id.as_deref(),
        Some(&params),
    );
    serde_json::to_value(result).map_err(|error| error.to_string())
}

/// 编译项目（阶段 4 逻辑链计算）：不变式检查 → blockHash → 逻辑链 → 矛盾链 → BP 信念。
/// Compile a project: invariant check → block hashes → logic chain → contradiction
/// chains → belief propagation. 纯函数，不持有状态；结果序列化为 camelCase JSON。
#[tauri::command]
pub fn compile_project(project: Value) -> Result<Value, String> {
    // 1) 编译管线：不变式 → 实体 blockHash → contentRootHash → fileHash。
    let compiled = crate::graph_compiler::canonical::compile(&project);
    // 2) 逻辑链（evidence 模式，与 TS computeLogicChain 对齐）。
    let logic_chain =
        crate::graph_compiler::analysis::compute_logic_chain(&project, "evidence", None);
    // 3) 矛盾链（contradicts / refutes 边的最小矛盾环）。
    let contradictions = crate::graph_compiler::analysis::contradiction_chains(&project, None);
    // 4) 因子图编译 + 双通道信念传播（BP 信念分数）。
    let factor_graph = research_graph_compiler::compile_factor_graph(&project);
    let bp = research_graph_compiler::loopy_belief_propagation(
        &factor_graph,
        &research_graph_compiler::BpOptions::default(),
    );
    let mean_net_belief = if bp.beliefs.is_empty() {
        0.5
    } else {
        bp.beliefs
            .iter()
            .map(|belief| belief.net_belief)
            .sum::<f64>()
            / bp.beliefs.len() as f64
    };

    serde_json::to_value(serde_json::json!({
        "compile": compiled,
        "logicChain": logic_chain,
        "contradictions": contradictions,
        "beliefs": {
            "converged": bp.converged,
            "iterations": bp.iterations,
            "residual": bp.residual,
            "status": bp.status,
            "meanNetBelief": mean_net_belief,
            "variableCount": bp.beliefs.len(),
        },
    }))
    .map_err(|error| error.to_string())
}

/// v3 意图驱动布局（§11）：从 `views[]` 中一项读取 mode + params，计算 →
/// fallback → 仅 pinned 坐标覆盖。拖拽产生的未 pinned 坐标不进入输出 diff。
/// Intent-driven layout: read mode + params from one `views[]` entry, then
/// compute → fallback → override with pinned coordinates only.
#[tauri::command]
#[allow(dead_code)] // superseded by compile_project + frontend layout; retained for compiler parity
pub fn layout_project_view(
    project: Value,
    view: Value,
    placements: Vec<Value>,
) -> Result<Value, String> {
    let result = crate::graph_compiler::layout::layout_view(&project, &view, &placements);
    serde_json::to_value(result).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_graph_layout_returns_camel_case_result() {
        let project = serde_json::json!({
            "nodes": [
                {"id": "a", "type": "note", "evidenceIds": []},
                {"id": "b", "type": "note", "evidenceIds": []}
            ],
            "edges": [{"id": "x", "type": "supports", "source": "a", "target": "b", "directed": true}]
        });
        let result =
            compute_graph_layout(project, "evidence-chain".to_string(), None, None).unwrap();
        let object = result.as_object().expect("result is an object");
        assert_eq!(object["mode"], "evidence-chain");
        assert!(object.get("positions").is_some());
        assert!(object.get("nodeIds").is_some());
        assert!(object.get("edgeIds").is_some());
        // 键按 camelCase 序列化。
        assert_eq!(object["params"]["columnGap"], 360.0);
    }

    #[test]
    fn compile_project_returns_hashes_chain_and_beliefs() {
        // TS ProjectState 形状的 JSON（nodes/edges/evidence/placements）。
        let project = serde_json::json!({
            "schemaVersion": 3,
            "id": "proj-1",
            "title": "PDF test",
            "discipline": "cs",
            "nodes": [
                {"id": "pdf-sec-s1", "type": "note", "title": "Introduction", "tags": [], "evidenceIds": [], "data": {}},
                {"id": "pdf-p1", "type": "evidence", "title": "A novel approach…", "tags": [], "evidenceIds": [], "data": {}}
            ],
            "edges": [
                {"id": "pdf-edge-para-p1", "type": "part_of", "source": "pdf-p1", "target": "pdf-sec-s1", "directed": true}
            ],
            "evidence": [],
            "placements": [],
            "scenarios": [],
            "activity": []
        });

        let result = compile_project(project).expect("compile succeeds");
        let object = result.as_object().expect("result is an object");

        let compiled = &object["compile"];
        assert!(compiled["blockHashes"]["pdf-sec-s1"].as_str().is_some());
        assert!(compiled["contentRootHash"].as_str().is_some());
        assert!(compiled["fileHash"].as_str().is_some());
        assert!(compiled["violations"].is_array());

        // 逻辑链（evidence 模式：supports/derived_from 边）。
        let logic_chain = &object["logicChain"];
        assert_eq!(logic_chain["mode"], "evidence");
        assert!(logic_chain["score"].as_f64().is_some());

        // 矛盾链结构与字段存在。
        assert!(object["contradictions"]["cycles"].is_array());
        assert!(object["contradictions"]["minimalSize"].is_null());

        // BP 信念均值在 (0,1) 区间。
        let beliefs = &object["beliefs"];
        let mean = beliefs["meanNetBelief"].as_f64().expect("mean belief");
        assert!(mean > 0.0 && mean < 1.0, "mean belief in range, got {mean}");
        assert!(beliefs["converged"].as_bool().is_some());
        assert!(beliefs["status"].as_str().is_some());
    }

    #[test]
    fn layout_project_view_honours_pinned_overrides() {
        let project = serde_json::json!({
            "nodes": [
                {"id": "a", "type": "note", "evidenceIds": []},
                {"id": "b", "type": "note", "evidenceIds": []}
            ],
            "edges": [{"id": "x", "type": "supports", "source": "a", "target": "b", "directed": true}]
        });
        let view = serde_json::json!({"id": "view-1", "layout": {"mode": "tree", "params": {"rootId": "a"}}});
        let placements = vec![serde_json::json!({
            "id": "pl-b", "viewId": "view-1", "nodeId": "b",
            "x": 321.0, "y": 654.0, "width": 200, "height": 100, "pinned": true
        })];
        let result = layout_project_view(project, view, placements).unwrap();
        let object = result.as_object().unwrap();
        assert_eq!(object["positions"]["b"]["x"], 321.0);
        assert_eq!(object["positions"]["b"]["y"], 654.0);
        // 未 pinned 节点保持计算坐标。
        assert_eq!(object["positions"]["a"]["x"], 80.0);
    }

    #[test]
    fn compute_diff_reports_structural_changes() {
        let v1 = serde_json::json!({
            "nodes": [
                {"id": "a", "type": "note", "title": "Alpha", "evidenceIds": []},
                {"id": "b", "type": "note", "title": "Beta", "evidenceIds": []}
            ],
            "edges": [{"id": "e1", "type": "causes", "source": "a", "target": "b", "directed": true}]
        });
        let mut v2 = v1.clone();
        v2["nodes"][0]["title"] = serde_json::json!("Alpha renamed");
        v2["nodes"].as_array_mut().unwrap().push(serde_json::json!({
            "id": "c", "type": "note", "title": "Gamma", "evidenceIds": []
        }));
        let result = compute_diff(v1, v2).unwrap();
        let object = result.as_object().unwrap();
        assert_eq!(object["addedNodes"][0], "c");
        assert_eq!(object["modifiedNodes"][0]["entityId"], "a");
        assert!(object.get("changedBlockHashes").is_some());
        assert!(object.get("durationMs").is_some());
    }

    #[test]
    fn compute_diff_accepts_filehash_wrapped_input() {
        let project = serde_json::json!({
            "nodes": [
                {"id": "a", "type": "note", "title": "Alpha", "evidenceIds": []}
            ],
            "edges": []
        });
        let wrapped = serde_json::json!({
            "fileHash": research_graph_compiler::hash::file_hash(&project),
            "project": project.clone()
        });
        let result = compute_diff(wrapped, project.clone()).unwrap();
        assert!(result.as_object().unwrap()["addedNodes"]
            .as_array()
            .unwrap()
            .is_empty());

        // 包装形式必须通过 fileHash 自校验；篡改内容应报错。
        let tampered = serde_json::json!({
            "fileHash": research_graph_compiler::hash::file_hash(&project),
            "project": serde_json::json!({"nodes": [{"id": "x", "type": "note"}]})
        });
        assert!(compute_diff(tampered, project).is_err());
    }

    #[test]
    fn compute_diff_rejects_bare_filehash_and_oversized_projects() {
        assert!(
            compute_diff(serde_json::json!("0123456789abcdef"), serde_json::json!({})).is_err()
        );
        let big_nodes: Vec<Value> = (0..100_001)
            .map(|index| serde_json::json!({"id": format!("n{index}"), "type": "note"}))
            .collect();
        let big = serde_json::json!({"nodes": big_nodes, "edges": []});
        assert!(compute_diff(big.clone(), serde_json::json!({"nodes": [], "edges": []})).is_err());
    }
}
