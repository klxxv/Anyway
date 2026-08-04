//! 确定性布局 / Deterministic layout (§11 of canvas-format-v3).
//!
//! 从 `app/lib/layout/compute.ts` 的 `computeLayout` 逐行移植，保证相同输入
//! 产生逐位一致的输出 —— 确定性是正确性约束，不是性能优化。
//! Ported line-by-line from `computeLayout` in `app/lib/layout/compute.ts`;
//! identical input must produce bit-identical output — determinism is a
//! correctness constraint, not a performance optimization.
//!
//! v3 布局模型（§11）：
//! - 布局意图：`views[].layout = { mode, params }`，params 控制根节点与网格间距；
//! - 计算后的 positions：全部节点坐标由纯函数硬计算，LLM/代理不参与；
//! - 仅人工 pinned 的坐标覆盖计算结果（`apply_pinned`）；
//! - 拖拽不产生 diff churn：未 pinned 的坐标不持久化，只有 pinned 写入 placements。
//!
//! 支持 6 种模式：evidence-chain / refutation-chain / tree / prefix-Huffman /
//! table / neural-network。输出结构 `LayoutResult` 与 TS `LayoutResult`
//! 逐字段对齐，供 Phase 1.4 双实现逐位比对测试使用。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::cmp::Reverse;

/// 全部布局模式（顺序即 v3 文档中的枚举顺序）。
/// All layout modes, in v3 enumeration order.
pub const LAYOUT_MODES: &[&str] = &[
    "evidence-chain",
    "refutation-chain",
    "tree",
    "huffman",
    "table",
    "neural-network",
];

/// 单个节点的展示坐标 / Presentation coordinates of a single node.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutPosition {
    pub x: f64,
    pub y: f64,
}

/// 布局意图参数（§11）：根节点 + 网格几何。所有字段都有与 TS `computeLayout`
/// 硬编码一致的默认值；`defaults_for_mode` 给出模式特定默认。
/// Layout-intent parameters: root node plus grid geometry. Every field carries
/// a default matching the TS `computeLayout` hardcoded constants;
/// `defaults_for_mode` returns the mode-specific defaults.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LayoutParams {
    /// tree 模式的遍历根 / Traversal root for tree mode.
    pub root_id: Option<String>,
    /// 首列 x 坐标 / x of the first column.
    pub origin_x: f64,
    /// 首行 y 坐标 / y of the first row.
    pub origin_y: f64,
    /// 层/列之间的水平间距 / Horizontal gap between layers or columns.
    pub column_gap: f64,
    /// 同层节点之间的垂直间距 / Vertical gap between nodes in a layer.
    pub row_gap: f64,
    /// 未定位节点回退网格的起点 x（fallback 模式）/
    /// Fallback grid origin x for nodes the mode did not position.
    pub fallback_origin_x: f64,
    /// fallback 首行相对 maxY 的偏移 / Fallback first-row offset from maxY.
    pub fallback_offset_y: f64,
    /// fallback 每行节点数 / Fallback nodes per row.
    pub fallback_columns: u32,
    /// fallback 列间距 / Fallback column gap.
    pub fallback_column_gap: f64,
    /// fallback 行间距 / Fallback row gap.
    pub fallback_row_gap: f64,
}

impl Default for LayoutParams {
    /// 层布局（evidence/refutation/neural-network）的默认几何。
    /// Defaults matching the layered-layout geometry.
    fn default() -> Self {
        Self {
            root_id: None,
            origin_x: 75.0,
            origin_y: 85.0,
            column_gap: 360.0,
            row_gap: 190.0,
            fallback_origin_x: 80.0,
            fallback_offset_y: 210.0,
            fallback_columns: 4,
            fallback_column_gap: 235.0,
            fallback_row_gap: 170.0,
        }
    }
}

/// 模式特定默认几何，与 TS `computeLayout` 的硬编码常量逐一对应。
/// Mode-specific geometry defaults, one-to-one with the hardcoded constants
/// in TS `computeLayout`.
pub fn defaults_for_mode(mode: &str) -> LayoutParams {
    let mut params = LayoutParams::default();
    match mode {
        "tree" => {
            params.origin_x = 80.0;
            params.origin_y = 80.0;
            params.column_gap = 350.0;
            params.row_gap = 182.0;
        }
        "table" => {
            params.origin_x = 70.0;
            params.origin_y = 105.0;
            params.column_gap = 310.0;
            params.row_gap = 168.0;
        }
        "huffman" => {
            params.origin_x = 70.0;
            params.origin_y = 80.0;
            params.column_gap = 320.0;
            params.row_gap = 172.0;
        }
        _ => {}
    }
    params
}

/// 布局计算结果：意图 + 计算后的展示信息（§11）。
/// Layout result: the intent plus computed presentation data.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayoutResult {
    pub mode: String,
    pub params: LayoutParams,
    /// nodeId → 坐标。键排序保证序列化确定性。
    /// nodeId → coordinates. Sorted keys keep serialization deterministic.
    pub positions: BTreeMap<String, LayoutPosition>,
    /// nodeId → 展示注解（table 行号 / huffman 前缀 / NN 层号）。
    /// nodeId → display annotation (table row / huffman prefix / NN layer).
    pub annotations: BTreeMap<String, String>,
    /// 按展示顺序排列的节点 id（决定行内顺序）。
    /// Node ids in presentation order (determines in-row order).
    pub node_ids: Vec<String>,
    /// 按展示顺序排列的边 id / Edge ids in presentation order.
    pub edge_ids: Vec<String>,
}

// ---------------------------------------------------------------------------
// 项目视图 / Project view
// ---------------------------------------------------------------------------

/// 布局算法所需的节点投影 / Node projection needed by layout algorithms.
#[derive(Clone, Debug)]
struct LNode<'a> {
    id: &'a str,
    typ: &'a str,
    evidence_len: usize,
}

/// 布局算法所需的边投影 / Edge projection needed by layout algorithms.
#[derive(Clone, Debug)]
struct LEdge<'a> {
    id: &'a str,
    source: &'a str,
    target: &'a str,
    typ: &'a str,
    outcome: Option<&'a str>,
    directed: bool,
}

fn project_nodes(project: &Value) -> Vec<LNode<'_>> {
    project
        .get("nodes")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|node| LNode {
                    id: node.get("id").and_then(Value::as_str).unwrap_or(""),
                    typ: node.get("type").and_then(Value::as_str).unwrap_or(""),
                    evidence_len: node
                        .get("evidenceIds")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn project_edges(project: &Value) -> Vec<LEdge<'_>> {
    project
        .get("edges")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|edge| LEdge {
                    id: edge.get("id").and_then(Value::as_str).unwrap_or(""),
                    source: edge.get("source").and_then(Value::as_str).unwrap_or(""),
                    target: edge.get("target").and_then(Value::as_str).unwrap_or(""),
                    typ: edge.get("type").and_then(Value::as_str).unwrap_or(""),
                    outcome: edge
                        .get("experiment")
                        .and_then(|exp| exp.get("outcome"))
                        .and_then(Value::as_str),
                    directed: edge
                        .get("directed")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 默认根：项目第一个节点的 id / Default root: the first node's id.
fn default_root_id(project: &Value) -> Option<String> {
    project_nodes(project).first().map(|node| node.id.to_string())
}

// ---------------------------------------------------------------------------
// 核心入口 / Core entry point
// ---------------------------------------------------------------------------

/// 纯计算布局（等价 TS `computeLayout`）：6 种模式，不读 placements。
/// Pure layout computation (equivalent to TS `computeLayout`) across all six
/// modes; it does not read placements.
///
/// - `mode`：六种模式之一；未知模式按 `evidence-chain` 处理。
/// - `root_id`：tree 模式遍历根；`None` 时用 `params.root_id`，再缺省用
///   `project.nodes[0].id`（与 TS 默认一致）。
/// - `params`：`None` 时使用 `defaults_for_mode(mode)`。
pub fn compute_layout(
    project: &Value,
    mode: &str,
    root_id: Option<&str>,
    params: Option<&LayoutParams>,
) -> LayoutResult {
    let resolved_mode = if LAYOUT_MODES.contains(&mode) { mode } else { "evidence-chain" };
    let mut params = params.cloned().unwrap_or_else(|| defaults_for_mode(resolved_mode));
    let root_id = root_id
        .map(str::to_string)
        .or_else(|| params.root_id.clone())
        .or_else(|| default_root_id(project));
    params.root_id = root_id.clone();

    let (positions, annotations, node_ids, edge_ids) = match resolved_mode {
        "tree" => tree_layout(project, root_id.as_deref(), &params),
        "table" => table_layout(project, &params),
        "huffman" => huffman_layout(project, &params),
        _ => layered_layout(project, resolved_mode, &params),
    };

    LayoutResult {
        mode: resolved_mode.to_string(),
        params,
        positions,
        annotations,
        node_ids,
        edge_ids,
    }
}

/// 计算后补充未定位节点到回退网格（对应 `applyLayout` 的 fallback 逻辑）。
/// After computing, place any still-unpositioned nodes on a fallback grid
/// (mirrors the fallback logic in `applyLayout`).
pub fn apply_fallback(mut result: LayoutResult, project: &Value) -> LayoutResult {
    let positioned: HashSet<String> = result.positions.keys().cloned().collect();
    let max_y = result
        .positions
        .values()
        .map(|position| position.y)
        .fold(80.0_f64, f64::max);
    let params = result.params.clone();
    let mut index = 0usize;
    for node in project_nodes(project) {
        if positioned.contains(node.id) {
            continue;
        }
        let columns = params.fallback_columns.max(1) as usize;
        let column = index % columns;
        let row = index / columns;
        result.positions.insert(
            node.id.to_string(),
            LayoutPosition {
                x: params.fallback_origin_x + column as f64 * params.fallback_column_gap,
                y: max_y + params.fallback_offset_y + row as f64 * params.fallback_row_gap,
            },
        );
        index += 1;
    }
    result
}

/// 仅人工 pinned 的坐标覆盖计算结果（§11）：pinned placement 覆盖计算位置；
/// 若 pinned 节点不在计算结果中（如链模式未链接节点被钉住），追加到 node_ids 末尾。
/// Human-pinned coordinates override computed ones. A pinned node missing from
/// the computed set (e.g. pinned but unlinked in chain mode) is appended to
/// `node_ids` so the canvas still renders it.
pub fn apply_pinned(mut result: LayoutResult, placements: &[Value]) -> LayoutResult {
    for placement in placements {
        let pinned = placement
            .get("pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        if !pinned {
            continue;
        }
        let (Some(node_id), Some(x), Some(y)) = (
            placement.get("nodeId").and_then(Value::as_str),
            placement.get("x").and_then(Value::as_f64),
            placement.get("y").and_then(Value::as_f64),
        ) else {
            continue;
        };
        if !result.node_ids.iter().any(|id| id == node_id) {
            result.node_ids.push(node_id.to_string());
        }
        result.positions.insert(
            node_id.to_string(),
            LayoutPosition {
                x,
                y,
            },
        );
    }
    result
}

/// 从可选 params JSON 解析布局参数：先取模式默认，再逐字段覆盖。
/// Resolve layout params from an optional JSON object: mode defaults first,
/// then per-field overrides (rootId / originX / originY / columnGap / rowGap).
pub fn resolve_params(mode: &str, params: Option<&Value>) -> LayoutParams {
    let mut resolved = defaults_for_mode(mode);
    if let Some(object) = params.and_then(Value::as_object) {
        if let Some(value) = object.get("rootId").and_then(Value::as_str) {
            resolved.root_id = Some(value.to_string());
        }
        if let Some(value) = object.get("originX").and_then(Value::as_f64) {
            resolved.origin_x = value;
        }
        if let Some(value) = object.get("originY").and_then(Value::as_f64) {
            resolved.origin_y = value;
        }
        if let Some(value) = object.get("columnGap").and_then(Value::as_f64) {
            resolved.column_gap = value;
        }
        if let Some(value) = object.get("rowGap").and_then(Value::as_f64) {
            resolved.row_gap = value;
        }
    }
    resolved
}

/// 从 `views[]` 中一项解析布局意图（§11）：`view.layout = { mode, params }`，
/// 缺省字段回落模式默认；随后计算 → fallback → pinned 覆盖。
/// Parse a layout intent from one `views[]` entry (`view.layout = { mode, params }`),
/// falling back to mode defaults, then compute → fallback → pinned override.
pub fn layout_view(project: &Value, view: &Value, placements: &[Value]) -> LayoutResult {
    let layout = view.get("layout");
    let mode = layout
        .and_then(|item| item.get("mode"))
        .and_then(Value::as_str)
        .unwrap_or(LAYOUT_MODES[0]);
    let params = resolve_params(mode, layout.and_then(|item| item.get("params")));
    let result = compute_layout(project, mode, None, Some(&params));
    let result = apply_fallback(result, project);
    apply_pinned(result, placements)
}

// ---------------------------------------------------------------------------
// tree 模式 / Tree mode
// ---------------------------------------------------------------------------

/// 确定性 BFS 遍历（对应 TS `traverseGraph` 的 BFS 分支，方向 out，无限深度）：
/// 邻居按 (nodeId, edgeId) 排序，保证 order 与 tree 边确定。
/// Deterministic BFS (the BFS branch of TS `traverseGraph`, direction out,
/// unbounded depth): neighbors sorted by (nodeId, edgeId) keep the order and
/// tree edges deterministic.
fn bfs_tree(project: &Value, start: &str) -> (Vec<String>, Vec<String>, HashMap<String, usize>) {
    let nodes = project_nodes(project);
    let node_ids: HashSet<&str> = nodes.iter().map(|node| node.id).collect();
    let edges = project_edges(project);

    // 邻居索引：有向边按 source→target；无向边再补 target→source。
    let mut neighbors: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for edge in &edges {
        if !node_ids.contains(edge.source) || !node_ids.contains(edge.target) {
            continue;
        }
        neighbors
            .entry(edge.source)
            .or_default()
            .push((edge.target, edge.id));
        if !edge.directed {
            neighbors
                .entry(edge.target)
                .or_default()
                .push((edge.source, edge.id));
        }
    }
    for list in neighbors.values_mut() {
        list.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));
    }

    let mut order = Vec::new();
    let mut tree_edge_ids = Vec::new();
    let mut depth: HashMap<String, usize> = HashMap::new();
    if !node_ids.contains(start) {
        return (order, tree_edge_ids, depth);
    }

    let mut visited: HashSet<&str> = HashSet::new();
    let mut queue: Vec<&str> = Vec::new();
    visited.insert(start);
    depth.insert(start.to_string(), 0);
    queue.push(start);
    let mut head = 0usize;
    while head < queue.len() {
        let current = queue[head];
        head += 1;
        order.push(current.to_string());
        let current_depth = depth[current];
        for (neighbor, edge_id) in neighbors.get(current).into_iter().flatten() {
            if !visited.contains(neighbor) {
                visited.insert(neighbor);
                depth.insert(neighbor.to_string(), current_depth + 1);
                tree_edge_ids.push((*edge_id).to_string());
                queue.push(neighbor);
            }
        }
    }
    (order, tree_edge_ids, depth)
}

/// 布局函数统一返回的元组：positions、annotations、node_ids、edge_ids。
/// The tuple every layout function returns: positions, annotations, node ids,
/// and edge ids.
type LayoutTuple = (
    BTreeMap<String, LayoutPosition>,
    BTreeMap<String, String>,
    Vec<String>,
    Vec<String>,
);

fn tree_layout(
    project: &Value,
    root_id: Option<&str>,
    params: &LayoutParams,
) -> LayoutTuple {
    let all_node_ids: Vec<String> = project_nodes(project)
        .iter()
        .map(|node| node.id.to_string())
        .collect();
    let (order, tree_edge_ids, depth) = bfs_tree(project, root_id.unwrap_or(""));

    let included: HashSet<&str> = order.iter().map(String::as_str).collect();
    let mut node_ids = order.clone();
    for id in &all_node_ids {
        if !included.contains(id.as_str()) {
            node_ids.push(id.clone());
        }
    }

    // 未访问节点落到最后一层之后（对应 TS `Math.max(1, ...depths) + 1`）。
    let fallback_depth = depth
        .values()
        .copied()
        .max()
        .map_or(0, |max| 1.max(max))
        + 1;

    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    let mut row_index: HashMap<usize, usize> = HashMap::new();
    for id in &node_ids {
        let layer = depth.get(id).copied().unwrap_or(fallback_depth);
        let index = row_index.entry(layer).or_insert_with(|| {
            rows.push((layer, Vec::new()));
            rows.len() - 1
        });
        rows[*index].1.push(id.clone());
    }

    let mut positions = BTreeMap::new();
    for (layer, ids) in &rows {
        for (index, id) in ids.iter().enumerate() {
            positions.insert(
                id.clone(),
                LayoutPosition {
                    x: params.origin_x + *layer as f64 * params.column_gap,
                    y: params.origin_y + index as f64 * params.row_gap,
                },
            );
        }
    }
    (positions, BTreeMap::new(), node_ids, tree_edge_ids)
}

// ---------------------------------------------------------------------------
// table 模式 / Table mode
// ---------------------------------------------------------------------------

fn table_layout(
    project: &Value,
    params: &LayoutParams,
) -> LayoutTuple {
    let nodes = project_nodes(project);
    let all_node_ids: Vec<String> = nodes.iter().map(|node| node.id.to_string()).collect();
    let all_edge_ids: Vec<String> = project_edges(project)
        .iter()
        .map(|edge| edge.id.to_string())
        .collect();

    // 列 = 类型出现顺序去重 / Columns = unique types in first-appearance order.
    let mut columns: Vec<&str> = Vec::new();
    for node in &nodes {
        if !columns.contains(&node.typ) {
            columns.push(node.typ);
        }
    }

    let mut positions = BTreeMap::new();
    let mut annotations = BTreeMap::new();
    for (column, typ) in columns.iter().enumerate() {
        for (row, node) in nodes.iter().filter(|node| node.typ == *typ).enumerate() {
            positions.insert(
                node.id.to_string(),
                LayoutPosition {
                    x: params.origin_x + column as f64 * params.column_gap,
                    y: params.origin_y + row as f64 * params.row_gap,
                },
            );
            annotations.insert(node.id.to_string(), format!("{typ} · row {}", row + 1));
        }
    }
    (positions, annotations, all_node_ids, all_edge_ids)
}

// ---------------------------------------------------------------------------
// prefix-Huffman 模式 / Prefix-Huffman mode
// ---------------------------------------------------------------------------

/// Huffman 队列项：权重 + 叶子 id 集合 + 编码表。
/// Huffman queue item: weight, leaf id set, and code table.
#[derive(Clone, Debug)]
struct HuffmanItem {
    weight: usize,
    ids: Vec<String>,
    codes: HashMap<String, String>,
}

/// 节点度数作为演示频率，生成确定性 Huffman 编码（对应 TS `huffmanCodes`）。
/// Deterministic Huffman codes from node degree as a demonstration frequency.
fn huffman_codes(project: &Value) -> HashMap<String, String> {
    let nodes = project_nodes(project);
    let edges = project_edges(project);

    let mut queue: Vec<HuffmanItem> = nodes
        .iter()
        .map(|node| {
            let degree = edges
                .iter()
                .filter(|edge| edge.source == node.id || edge.target == node.id)
                .count();
            let mut codes = HashMap::new();
            codes.insert(node.id.to_string(), String::new());
            HuffmanItem {
                weight: 1 + node.evidence_len * 2 + degree,
                ids: vec![node.id.to_string()],
                codes,
            }
        })
        .collect();

    // 单节点特例：编码固定为 "0"（对应 TS 的 early return）。
    if queue.len() == 1 {
        let mut codes = HashMap::new();
        codes.insert(queue[0].ids[0].clone(), "0".to_string());
        return codes;
    }

    while queue.len() > 1 {
        queue.sort_by(|a, b| {
            a.weight
                .cmp(&b.weight)
                .then_with(|| a.ids.join(",").cmp(&b.ids.join(",")))
        });
        let left = queue.remove(0);
        let right = queue.remove(0);
        let mut codes = HashMap::new();
        for (id, code) in &left.codes {
            codes.insert(id.clone(), format!("0{code}"));
        }
        for (id, code) in &right.codes {
            codes.insert(id.clone(), format!("1{code}"));
        }
        let mut ids = left.ids.clone();
        ids.extend(right.ids);
        queue.push(HuffmanItem {
            weight: left.weight + right.weight,
            ids,
            codes,
        });
    }
    queue.pop().map(|item| item.codes).unwrap_or_default()
}

fn huffman_layout(
    project: &Value,
    params: &LayoutParams,
) -> LayoutTuple {
    let nodes = project_nodes(project);
    let all_node_ids: Vec<String> = nodes.iter().map(|node| node.id.to_string()).collect();
    let all_edge_ids: Vec<String> = project_edges(project)
        .iter()
        .map(|edge| edge.id.to_string())
        .collect();
    let codes = huffman_codes(project);

    let mut ordered: Vec<(String, String)> = nodes
        .iter()
        .map(|node| {
            (
                node.id.to_string(),
                codes.get(node.id).cloned().unwrap_or_default(),
            )
        })
        .collect();
    ordered.sort_by(|a, b| a.1.cmp(&b.1));

    let mut positions = BTreeMap::new();
    let mut annotations = BTreeMap::new();
    let mut rows_by_depth: HashMap<usize, usize> = HashMap::new();
    for (id, code) in &ordered {
        let depth = code.len();
        let row = rows_by_depth.get(&depth).copied().unwrap_or(0);
        positions.insert(
            id.clone(),
            LayoutPosition {
                x: params.origin_x + depth as f64 * params.column_gap,
                y: params.origin_y + row as f64 * params.row_gap,
            },
        );
        annotations.insert(
            id.clone(),
            format!("prefix {}", if code.is_empty() { "0" } else { code }),
        );
        rows_by_depth.insert(depth, row + 1);
    }
    (positions, annotations, all_node_ids, all_edge_ids)
}

// ---------------------------------------------------------------------------
// 链 / 神经网络模式 / Chain and neural-network modes
// ---------------------------------------------------------------------------

/// 支持证据链的边类型 / Edge types that carry an evidence chain forward.
const SUPPORT_TYPES: &[&str] = &["supports", "derived_from", "measures", "uses"];

/// 按关系语义选择证据或反驳链边（对应 TS `chainEdgeIds`）。
/// Selects evidence or refutation-chain edges by relation semantics.
fn chain_edge_ids<'a>(project: &'a Value, mode: &str) -> Vec<&'a str> {
    project_edges(project)
        .iter()
        .filter(|edge| match mode {
            "refutation-chain" => edge.typ == "contradicts" || edge.outcome == Some("refutes"),
            _ => SUPPORT_TYPES.contains(&edge.typ) || edge.outcome == Some("supports"),
        })
        .map(|edge| edge.id)
        .collect()
}

/// 有向无环部分的分层深度（Kahn，队列按 id 字典序）；循环节点保留为未分层，
/// 统一放到最后一层（对应 TS `topologicalDepths`）。
/// Ranks the directed acyclic portion (Kahn, queue ordered by id); cycle
/// members stay unranked and land in the final layer.
fn topological_depths(project: &Value, selected: &HashSet<&str>) -> HashMap<String, usize> {
    let nodes = project_nodes(project);
    let node_ids: HashSet<&str> = nodes.iter().map(|node| node.id).collect();
    let edges = project_edges(project);

    let mut incoming: HashMap<&str, usize> = HashMap::new();
    let mut outgoing: HashMap<&str, Vec<&LEdge>> = HashMap::new();
    let mut depth: HashMap<String, usize> = HashMap::new();
    for node in &nodes {
        incoming.insert(node.id, 0);
        depth.insert(node.id.to_string(), 0);
    }
    for edge in &edges {
        if !node_ids.contains(edge.source) || !node_ids.contains(edge.target) {
            continue;
        }
        if !selected.contains(edge.id) {
            continue;
        }
        *incoming.entry(edge.target).or_insert(0) += 1;
        outgoing.entry(edge.source).or_default().push(edge);
    }

    let mut queue: BinaryHeap<Reverse<&str>> = node_ids
        .iter()
        .filter(|id| incoming.get(*id).copied().unwrap_or(0) == 0)
        .map(|id| Reverse(*id))
        .collect();
    let mut visited = 0usize;
    while let Some(Reverse(current)) = queue.pop() {
        visited += 1;
        let current_depth = depth[current];
        for edge in outgoing.get(current).into_iter().flatten() {
            let next = current_depth + 1;
            let entry = depth.entry(edge.target.to_string()).or_insert(0);
            if next > *entry {
                *entry = next;
            }
            let count = incoming.get_mut(edge.target).expect("target in incoming");
            *count -= 1;
            if *count == 0 {
                queue.push(Reverse(edge.target));
            }
        }
    }

    // 反馈图是合法研究对象：未分层节点放入最后一层而不是丢弃。
    if visited < node_ids.len() {
        let max_depth = depth.values().copied().max().unwrap_or(0);
        for (id, count) in incoming {
            if count > 0 {
                depth.insert(id.to_string(), max_depth + 1);
            }
        }
    }
    depth
}

fn layered_layout(
    project: &Value,
    mode: &str,
    params: &LayoutParams,
) -> LayoutTuple {
    let nodes = project_nodes(project);
    let all_node_ids: Vec<String> = nodes.iter().map(|node| node.id.to_string()).collect();

    let selected_ids = if mode == "evidence-chain" || mode == "refutation-chain" {
        chain_edge_ids(project, mode)
    } else {
        project_edges(project)
            .iter()
            .map(|edge| edge.id)
            .collect::<Vec<&str>>()
    };
    let selected: HashSet<&str> = selected_ids.iter().copied().collect();

    let node_ids: Vec<String> = if mode == "neural-network" {
        all_node_ids.clone()
    } else {
        // 链模式只保留被选中边连接到的节点（顺序 = 边出现顺序，source 先 target 后）。
        let mut linked: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for edge in project_edges(project) {
            if !selected.contains(edge.id) {
                continue;
            }
            for endpoint in [edge.source, edge.target] {
                if !seen.contains(endpoint) {
                    seen.insert(endpoint);
                    linked.push(endpoint);
                }
            }
        }
        linked.into_iter().map(str::to_string).collect()
    };

    let depth = topological_depths(project, &selected);

    // 行分组保持 node_ids 顺序：行首次出现的顺序即展示顺序。
    let mut rows: Vec<(usize, Vec<String>)> = Vec::new();
    let mut row_index: HashMap<usize, usize> = HashMap::new();
    for id in &node_ids {
        let layer = depth.get(id).copied().unwrap_or(0);
        let index = row_index.entry(layer).or_insert_with(|| {
            rows.push((layer, Vec::new()));
            rows.len() - 1
        });
        rows[*index].1.push(id.clone());
    }

    let mut positions = BTreeMap::new();
    let mut annotations = BTreeMap::new();
    for (layer, ids) in &rows {
        for (index, id) in ids.iter().enumerate() {
            positions.insert(
                id.clone(),
                LayoutPosition {
                    x: params.origin_x + *layer as f64 * params.column_gap,
                    y: params.origin_y + index as f64 * params.row_gap,
                },
            );
            if mode == "neural-network" {
                annotations.insert(id.clone(), format!("layer {layer}"));
            }
        }
    }

    let edge_ids: Vec<String> = selected_ids.into_iter().map(str::to_string).collect();
    (positions, annotations, node_ids, edge_ids)
}

// ---------------------------------------------------------------------------
// 测试 / Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 复刻 TS `initialProject` 的关键结构（无 scenario 影响）。
    /// Mirrors the TS `initialProject` structure for layout tests.
    fn fixture() -> Value {
        json!({
            "schemaVersion": 1,
            "id": "project-transformer-ablation",
            "title": "Long-context Transformer ablation",
            "discipline": "Neural Networks",
            "updatedAt": "2026-08-01T00:00:00Z",
            "revision": 18,
            "nodes": [
                {"id": "q1", "type": "question", "title": "Q", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": ["ev1"], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "d1", "type": "dataset", "title": "D", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": ["ev3"], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "m1", "type": "method", "title": "M1", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "m2", "type": "method", "title": "M2", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": ["ev1"], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "m3", "type": "method", "title": "M3", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": ["ev2"], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "v1", "type": "variable", "title": "V1", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "v2", "type": "variable", "title": "V2", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "v3", "type": "variable", "title": "V3", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "h1", "type": "hypothesis", "title": "H1", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": ["ev1"], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "x1", "type": "experiment", "title": "X1", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "r1", "type": "metric", "title": "R1", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": ["ev3"], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "r2", "type": "metric", "title": "R2", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "p1", "type": "result", "title": "P1", "body": "b", "tags": [], "status": "draft", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "e1", "type": "evidence", "title": "E1", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": ["ev1"], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"}
            ],
            "edges": [
                {"id": "e-q-m1", "type": "depends_on", "source": "q1", "target": "m1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-q-h1", "type": "depends_on", "source": "q1", "target": "h1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-m1-m2", "type": "depends_on", "source": "m1", "target": "m2", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-m2-m3", "type": "depends_on", "source": "m2", "target": "m3", "directed": true, "polarity": "positive", "confidence": 0.89, "conditions": [], "evidenceIds": ["ev1"], "provenance": {"origin": "human"}},
                {"id": "e-v1-m2", "type": "moderates", "source": "v1", "target": "m2", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-m3-x1", "type": "uses", "source": "m3", "target": "x1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-h1-x1", "type": "derived_from", "source": "h1", "target": "x1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-v2-x1", "type": "controls", "source": "v2", "target": "x1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-v3-x1", "type": "controls", "source": "v3", "target": "x1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-d1-x1", "type": "uses", "source": "d1", "target": "x1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-x1-r1", "type": "measures", "source": "x1", "target": "r1", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-x1-r2", "type": "measures", "source": "x1", "target": "r2", "directed": true, "polarity": "positive", "confidence": 0.72, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-r1-p1", "type": "supports", "source": "r1", "target": "p1", "directed": true, "polarity": "positive", "confidence": 0.89, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-r2-p1", "type": "supports", "source": "r2", "target": "p1", "directed": true, "polarity": "positive", "confidence": 0.89, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e-e1-h1", "type": "supports", "source": "e1", "target": "h1", "directed": true, "polarity": "positive", "confidence": 0.89, "conditions": [], "evidenceIds": ["ev1"], "provenance": {"origin": "human"}}
            ],
            "evidence": [],
            "placements": [],
            "scenarios": [],
            "activity": []
        })
    }

    fn positions_of(result: &LayoutResult) -> HashMap<&str, (f64, f64)> {
        result
            .positions
            .iter()
            .map(|(id, position)| (id.as_str(), (position.x, position.y)))
            .collect()
    }

    #[test]
    fn all_six_modes_are_deterministic() {
        let project = fixture();
        for mode in LAYOUT_MODES {
            let first = compute_layout(&project, mode, Some("q1"), None);
            let second = compute_layout(&project, mode, Some("q1"), None);
            assert_eq!(first, second, "mode {mode} must be deterministic");
            assert_eq!(first.mode.as_str(), *mode);
            // fixture 无 contradicts/refutes 边 → refutation-chain 正确为空；
            // 其余模式都必须有节点落位。
            if *mode != "refutation-chain" {
                assert!(!first.positions.is_empty(), "mode {mode} should place nodes");
            } else {
                assert!(first.positions.is_empty());
                assert!(first.edge_ids.is_empty());
            }
            // 同一输入两次计算序列化一致（逐位）。
            let json_a = serde_json::to_string(&first).unwrap();
            let json_b = serde_json::to_string(&second).unwrap();
            assert_eq!(json_a, json_b);
        }
    }

    #[test]
    fn tree_bfs_order_matches_ts_semantics() {
        let project = fixture();
        let result = compute_layout(&project, "tree", Some("q1"), None);
        // TS: BFS from q1 (direction out) — q1 first, then h1 (e-q-h1) before
        // m1 (e-q-m1) because neighbors sort by nodeId (h1 < m1).
        assert_eq!(result.node_ids[0], "q1");
        let (order, _, _) = bfs_tree(&project, "q1");
        assert!(order.iter().position(|id| id == "h1").unwrap()
            < order.iter().position(|id| id == "m1").unwrap());
        // 未包含节点排在 BFS order 之后（本项目 BFS 可达全部节点）。
        assert_eq!(result.node_ids.len(), 14);
        // 每个节点都有位置，且层级按 BFS 深度。
        let q1 = result.positions["q1"];
        assert_eq!(q1.x, 80.0);
        assert_eq!(q1.y, 80.0);
        // q1 的邻居按 nodeId 字典序：h1 先于 m1（e-q-h1 与 e-q-m1）。
        let h1 = result.positions["h1"];
        assert_eq!(h1.x, 80.0 + 350.0); // depth 1
        assert_eq!(h1.y, 80.0);
        let m1 = result.positions["m1"];
        assert_eq!(m1.x, 80.0 + 350.0);
        assert_eq!(m1.y, 80.0 + 182.0); // depth 1 的第二行
        let m3 = result.positions["m3"];
        assert_eq!(m3.x, 80.0 + 3.0 * 350.0); // q1→m1→m2→m3, depth 3
        // tree 边不含 cross/back 边：15 条边中非树边被排除。
        assert!(result.edge_ids.iter().all(|id| id.starts_with("e-")));
    }

    #[test]
    fn tree_fallback_layer_places_unreachable_nodes() {
        // 环 + 孤岛：q1 的 BFS 不可达 r2（孤岛），r2 落到最后一层。
        let mut project = fixture();
        project["nodes"].as_array_mut().unwrap().push(json!({
            "id": "iso", "type": "note", "title": "I", "body": "b", "tags": [],
            "status": "confirmed", "evidenceIds": [], "data": {},
            "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"
        }));
        let result = compute_layout(&project, "tree", Some("q1"), None);
        assert!(result.node_ids.iter().any(|id| id == "iso"));
        let iso = result.positions["iso"];
        // iso 在全部 BFS 深度之后：depth = max(1, 最大深度) + 1。
        assert!(iso.x > 80.0 + 350.0 * 4.0);
    }

    #[test]
    fn table_groups_columns_by_type_in_appearance_order() {
        let project = fixture();
        let result = compute_layout(&project, "table", None, None);
        // 列 = 类型出现顺序：question, dataset, method, variable, hypothesis,
        // experiment, metric, result, evidence。
        let question = result.positions["q1"];
        assert_eq!(question.x, 70.0);
        assert_eq!(question.y, 105.0);
        let dataset = result.positions["d1"];
        assert_eq!(dataset.x, 70.0 + 310.0);
        let method = result.positions["m1"];
        assert_eq!(method.x, 70.0 + 2.0 * 310.0);
        assert_eq!(method.y, 105.0);
        let m2 = result.positions["m2"];
        assert_eq!(m2.y, 105.0 + 168.0); // 同列第二行
        assert_eq!(result.annotations["m1"], "method · row 1");
        assert_eq!(result.annotations["m2"], "method · row 2");
        // 全部节点都进表。
        assert_eq!(result.positions.len(), 14);
    }

    #[test]
    fn huffman_assigns_prefix_codes_by_degree() {
        let project = fixture();
        let result = compute_layout(&project, "huffman", None, None);
        for (id, annotation) in &result.annotations {
            assert!(annotation.starts_with("prefix "), "{id}: {annotation}");
            // 注解中的编码必须与 positions 的层一致。
            let code = &annotation["prefix ".len()..];
            let depth = code.len();
            let position = &result.positions[id];
            assert_eq!(position.x, 70.0 + depth as f64 * 320.0);
        }
        // 全部节点都有编码。
        assert_eq!(result.positions.len(), 14);
    }

    #[test]
    fn huffman_single_node_uses_code_zero() {
        let project = json!({
            "nodes": [{"id": "only", "type": "note", "evidenceIds": []}],
            "edges": []
        });
        let result = compute_layout(&project, "huffman", None, None);
        assert_eq!(result.annotations["only"], "prefix 0");
        // 单节点特例编码为 "0"，长度 1 → 第 1 列（与 TS 行为一致）。
        assert_eq!(result.positions["only"].x, 70.0 + 320.0);
        assert_eq!(result.positions["only"].y, 80.0);
    }

    #[test]
    fn evidence_chain_selects_supporting_edges_only() {
        let project = fixture();
        let result = compute_layout(&project, "evidence-chain", None, None);
        // 支持型边（supports/derived_from/measures/uses）。
        for id in &result.edge_ids {
            let edge = project_edges(&project)
                .into_iter()
                .find(|edge| edge.id == id)
                .expect("edge exists");
            assert!(
                SUPPORT_TYPES.contains(&edge.typ),
                "edge {id} ({}) must be a support type",
                edge.typ
            );
        }
        // depends_on / controls / moderates 被排除。
        assert!(!result.edge_ids.iter().any(|id| id == "e-q-m1"));
        assert!(!result.edge_ids.iter().any(|id| id == "e-v2-x1"));
        // 未链接节点（如 v2、v3、d1？d1 通过 uses 链接）不在 node_ids 中。
        assert!(result.node_ids.iter().any(|id| id == "d1")); // e-d1-x1 uses
        assert!(!result.node_ids.iter().any(|id| id == "v2"));
        assert!(!result.node_ids.iter().any(|id| id == "v3"));
        assert!(result.node_ids.iter().any(|id| id == "q1") == false); // q1 只有 depends_on
    }

    #[test]
    fn refutation_chain_keeps_contradicts_and_refutes() {
        let mut project = fixture();
        project["edges"].as_array_mut().unwrap().push(json!({
            "id": "x-con", "type": "contradicts", "source": "p1", "target": "h1",
            "directed": true, "polarity": "negative", "confidence": 0.9,
            "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}
        }));
        project["edges"].as_array_mut().unwrap().push(json!({
            "id": "x-ref", "type": "supports", "source": "r1", "target": "p1",
            "directed": true, "polarity": "positive", "confidence": 0.9,
            "conditions": [], "evidenceIds": [],
            "experiment": {"id": "exp", "label": "L", "metric": "acc", "outcome": "refutes", "status": "completed"},
            "provenance": {"origin": "human"}
        }));
        let result = compute_layout(&project, "refutation-chain", None, None);
        assert!(result.edge_ids.iter().any(|id| id == "x-con"));
        assert!(result.edge_ids.iter().any(|id| id == "x-ref"));
        assert!(!result.edge_ids.iter().any(|id| id == "e-x1-r1")); // measures 不入选
        // 参与节点：p1、h1、r1。
        assert!(result.node_ids.iter().any(|id| id == "p1"));
        assert!(result.node_ids.iter().any(|id| id == "h1"));
        assert!(result.node_ids.iter().any(|id| id == "r1"));
    }

    #[test]
    fn neural_network_keeps_all_nodes_and_annotates_layers() {
        let project = fixture();
        let result = compute_layout(&project, "neural-network", None, None);
        assert_eq!(result.node_ids.len(), 14);
        assert_eq!(result.edge_ids.len(), 15);
        // 拓扑深度：q1 在第 0 层，x1 在第 4 层（q1→m1→m2→m3→x1）。
        assert_eq!(result.annotations["q1"], "layer 0");
        assert_eq!(result.annotations["x1"], "layer 4");
        assert_eq!(result.positions["x1"].x, 75.0 + 4.0 * 360.0);
    }

    #[test]
    fn cycles_land_in_final_layer() {
        let mut project = fixture();
        project["edges"].as_array_mut().unwrap().push(json!({
            "id": "cycle", "type": "causes", "source": "p1", "target": "h1",
            "directed": true, "polarity": "positive", "confidence": 0.5,
            "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}
        }));
        project["edges"].as_array_mut().unwrap().push(json!({
            "id": "cycle-back", "type": "causes", "source": "h1", "target": "p1",
            "directed": true, "polarity": "positive", "confidence": 0.5,
            "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}
        }));
        // p1 ↔ h1 构成环；在 NN 模式中它们落到最后一层（maxDepth + 1）。
        // TS 语义：依赖环节点的入度 > 0，全部归入最后一层（含 x1/r1/r2）。
        let result = compute_layout(&project, "neural-network", None, None);
        let h1 = result.positions["h1"];
        let p1 = result.positions["p1"];
        let x1 = result.positions["x1"];
        // 同层成员共享列位置（x 由 layer 决定），行位置（y）不同。
        assert_eq!(h1.x, p1.x, "cycle members share the final layer column");
        assert_eq!(h1.x, x1.x, "dependents of the cycle land in the final layer too");
        let max_ranked_x = result
            .positions
            .iter()
            .filter(|(id, _)| *id != "h1" && *id != "p1" && *id != "x1" && *id != "r1" && *id != "r2")
            .map(|(_, position)| position.x)
            .fold(0.0_f64, f64::max);
        assert!(h1.x > max_ranked_x, "cycle members land after ranked layers");
    }

    #[test]
    fn pinned_coordinates_override_computed_positions() {
        let project = fixture();
        let placements = vec![json!({
            "id": "pl-h1", "viewId": "view-1", "nodeId": "h1",
            "x": 1234.0, "y": 5678.0, "width": 230, "height": 116, "pinned": true
        })];
        let result = layout_view(&project, &json!({"id": "view-1", "layout": {"mode": "neural-network"}}), &placements);
        assert_eq!(result.positions["h1"].x, 1234.0);
        assert_eq!(result.positions["h1"].y, 5678.0);
        // 未 pinned 节点仍是计算值。
        assert_eq!(result.positions["q1"].x, 75.0);
    }

    #[test]
    fn unpinned_placements_do_not_affect_output() {
        let project = fixture();
        let placements = vec![json!({
            "id": "pl-q1", "viewId": "view-1", "nodeId": "q1",
            "x": 1.0, "y": 2.0, "width": 230, "height": 116
        })];
        let with_unpinned = layout_view(&project, &json!({"id": "view-1", "layout": {"mode": "tree"}}), &placements);
        let without = layout_view(&project, &json!({"id": "view-1", "layout": {"mode": "tree"}}), &[]);
        assert_eq!(with_unpinned, without, "unpinned drags must not churn the diff");
    }

    #[test]
    fn pinned_node_outside_computed_set_is_appended() {
        let project = fixture();
        // evidence-chain 模式 q1 未链接；人工把它钉住后仍应显示。
        let placements = vec![json!({
            "id": "pl-q1", "viewId": "view-1", "nodeId": "q1",
            "x": 99.0, "y": 88.0, "width": 230, "height": 116, "pinned": true
        })];
        let result = layout_view(
            &project,
            &json!({"id": "view-1", "layout": {"mode": "evidence-chain"}}),
            &placements,
        );
        assert!(result.node_ids.iter().any(|id| id == "q1"));
        assert_eq!(result.positions["q1"].x, 99.0);
    }

    #[test]
    fn fallback_places_unpositioned_nodes_in_a_grid() {
        let project = fixture();
        let computed = compute_layout(&project, "evidence-chain", None, None);
        assert!(computed.positions.len() < 14, "chain mode positions a subset");
        let with_fallback = apply_fallback(computed.clone(), &project);
        assert_eq!(with_fallback.positions.len(), 14);
        // fallback 行从 maxY + 210 开始。
        let max_y = computed.positions.values().map(|p| p.y).fold(80.0, f64::max);
        for (id, position) in &with_fallback.positions {
            if !computed.positions.contains_key(id) {
                assert!(position.y >= max_y + 210.0, "{id} should be below computed rows");
            }
        }
    }

    #[test]
    fn layout_view_parses_intent_params() {
        let project = fixture();
        let view = json!({
            "id": "view-1",
            "layout": {
                "mode": "tree",
                "params": {"rootId": "m1", "originX": 10.0, "originY": 20.0, "columnGap": 100.0, "rowGap": 50.0}
            }
        });
        let result = layout_view(&project, &view, &[]);
        assert_eq!(result.params.root_id.as_deref(), Some("m1"));
        let m1 = result.positions["m1"];
        assert_eq!(m1.x, 10.0);
        assert_eq!(m1.y, 20.0);
        // m1 的 BFS 子节点在下一层。
        let m2 = result.positions["m2"];
        assert_eq!(m2.x, 10.0 + 100.0);
        assert_eq!(m2.y, 20.0);
    }

    #[test]
    fn empty_project_never_panics() {
        let project = json!({"nodes": [], "edges": []});
        for mode in LAYOUT_MODES {
            let result = compute_layout(&project, mode, None, None);
            assert!(result.positions.is_empty());
            assert!(result.node_ids.is_empty());
            assert!(result.edge_ids.is_empty());
        }
    }

    #[test]
    fn unknown_mode_falls_back_to_evidence_chain() {
        let project = fixture();
        let result = compute_layout(&project, "not-a-mode", None, None);
        assert_eq!(result.mode, "evidence-chain");
    }
}
