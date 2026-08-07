//! 图编译器 Tauri 命令边界 / Tauri command boundary for the graph compiler.
//!
//! 语义内核（graph_compiler）是纯 Rust 库；本模块是它到 Webview 的唯一桥：
//! 参数经 JSON 校验后交给纯函数，结果序列化为 camelCase JSON。不持有状态。

use serde_json::Value;

/// 确定性布局：给定项目 + 模式 + 可选根节点与参数，返回计算后的展示结果。
/// Deterministic layout: given a project, mode, optional root id and params,
/// return the computed presentation result (positions / annotations / node & edge ids).
#[tauri::command]
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

/// v3 意图驱动布局（§11）：从 `views[]` 中一项读取 mode + params，计算 →
/// fallback → 仅 pinned 坐标覆盖。拖拽产生的未 pinned 坐标不进入输出 diff。
/// Intent-driven layout: read mode + params from one `views[]` entry, then
/// compute → fallback → override with pinned coordinates only.
#[tauri::command]
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
        let result = compute_graph_layout(project, "evidence-chain".to_string(), None, None).unwrap();
        let object = result.as_object().expect("result is an object");
        assert_eq!(object["mode"], "evidence-chain");
        assert!(object.get("positions").is_some());
        assert!(object.get("nodeIds").is_some());
        assert!(object.get("edgeIds").is_some());
        // 键按 camelCase 序列化。
        assert_eq!(object["params"]["columnGap"], 360.0);
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
}
