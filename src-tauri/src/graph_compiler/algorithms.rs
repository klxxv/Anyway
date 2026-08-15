//! 图算法：可达性遍历与路径 / Traversal and paths (§15.2 编译器独占清单)。
//!
//! BFS/DFS 遍历（深度层 + tree/cross/back 边分类）、最短路径、等长路径枚举。
//! 纯函数：输入 ProjectState-like JSON（`&Value`），无副作用，稳定排序保证
//! 可复现（与 TS 侧 bit-identical）。场景解析工具供 `analysis.rs` 复用。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// 遍历策略。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TraversalStrategy {
    Bfs,
    Dfs,
}

/// 遍历方向。
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TraversalDirection {
    In,
    Out,
    Both,
}

/// 遍历的过滤与深度约束；结果保持稳定排序以便复现。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TraversalRequest {
    pub start_id: String,
    pub strategy: TraversalStrategy,
    pub direction: TraversalDirection,
    pub max_depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scenario_id: Option<String>,
}

/// 供算法解释和画布高亮使用的遍历产物。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraversalResult {
    pub strategy: TraversalStrategy,
    pub start_id: String,
    pub order: Vec<String>,
    pub edge_ids: Vec<String>,
    pub depth: BTreeMap<String, u32>,
    pub parent: BTreeMap<String, Option<String>>,
    pub tree_edge_ids: Vec<String>,
    pub cross_edge_ids: Vec<String>,
    pub back_edge_ids: Vec<String>,
    pub stopped_by_depth: Vec<String>,
    pub duration_ms: f64,
}

/// 内部边视图：场景覆盖已合并，仅保留算法所需字段。
pub(crate) struct EdgeView {
    pub(crate) id: String,
    pub(crate) source: String,
    pub(crate) target: String,
    pub(crate) edge_type: String,
    pub(crate) directed: bool,
    pub(crate) confidence: f64,
    pub(crate) outcome: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) delta: Option<f64>,
    pub(crate) experiment_id: Option<String>,
}

/// 按 id 查找场景（对齐 TS `project.scenarios.find`）。
pub(crate) fn scenario<'a>(project: &'a Value, scenario_id: Option<&str>) -> Option<&'a Value> {
    let id = scenario_id?;
    project
        .get("scenarios")
        .and_then(Value::as_array)?
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(id))
}

/// 数组字段 → HashSet（disabledNodeIds 等）。
pub(crate) fn string_set(value: Option<&Value>) -> HashSet<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// 应用场景的非破坏性排除与边覆盖（对齐 TS resolveEdges），并按边 id 排序。
pub(crate) fn resolve_edges_view(project: &Value, scenario_id: Option<&str>) -> Vec<EdgeView> {
    let scenario = scenario(project, scenario_id);
    let disabled_nodes = string_set(scenario.and_then(|s| s.get("disabledNodeIds")));
    let disabled_edges = string_set(scenario.and_then(|s| s.get("disabledEdgeIds")));
    let overrides = scenario
        .and_then(|s| s.get("edgeOverrides"))
        .and_then(Value::as_object);
    let empty = Vec::new();
    let mut views = Vec::new();
    for edge in project
        .get("edges")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
    {
        let merged = overrides
            .and_then(|map| map.get(edge.get("id").and_then(Value::as_str).unwrap_or("")))
            .map(|extra| {
                let mut copy = edge.clone();
                if let (Some(base), Some(extra)) = (copy.as_object_mut(), extra.as_object()) {
                    for (key, value) in extra {
                        base.insert(key.clone(), value.clone());
                    }
                }
                copy
            })
            .unwrap_or_else(|| edge.clone());
        let (Some(id), Some(source), Some(target)) = (
            merged.get("id").and_then(Value::as_str),
            merged.get("source").and_then(Value::as_str),
            merged.get("target").and_then(Value::as_str),
        ) else {
            continue;
        };
        if disabled_edges.contains(id)
            || disabled_nodes.contains(source)
            || disabled_nodes.contains(target)
        {
            continue;
        }
        let experiment = merged.get("experiment");
        views.push(EdgeView {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            edge_type: merged
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            directed: merged
                .get("directed")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            confidence: merged
                .get("confidence")
                .and_then(Value::as_f64)
                .unwrap_or(0.5),
            outcome: experiment
                .and_then(|e| e.get("outcome"))
                .and_then(Value::as_str)
                .map(str::to_string),
            status: experiment
                .and_then(|e| e.get("status"))
                .and_then(Value::as_str)
                .map(str::to_string),
            delta: experiment
                .and_then(|e| e.get("delta"))
                .and_then(Value::as_f64),
            experiment_id: experiment
                .and_then(|e| e.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string),
        });
    }
    views.sort_by(|a, b| a.id.cmp(&b.id));
    views
}

/// 场景启用的节点集合（对齐 TS resolvedNodeIds）。
pub(crate) fn resolved_node_ids(project: &Value, scenario_id: Option<&str>) -> HashSet<String> {
    let disabled =
        string_set(scenario(project, scenario_id).and_then(|s| s.get("disabledNodeIds")));
    let empty = Vec::new();
    project
        .get("nodes")
        .and_then(Value::as_array)
        .unwrap_or(&empty)
        .iter()
        .filter_map(|node| node.get("id").and_then(Value::as_str))
        .filter(|id| !disabled.contains(*id))
        .map(str::to_string)
        .collect()
}

/// 启用节点内应用类型过滤，始终保留遍历起点（对齐 TS filteredActiveNodeIds）。
fn active_node_ids(
    project: &Value,
    scenario_id: Option<&str>,
    start_id: &str,
    node_types: Option<&[String]>,
) -> HashSet<String> {
    let resolved = resolved_node_ids(project, scenario_id);
    let Some(types) = node_types else {
        return resolved;
    };
    let allowed: HashSet<&str> = types.iter().map(String::as_str).collect();
    let empty = Vec::new();
    let nodes = project
        .get("nodes")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    resolved
        .into_iter()
        .filter(|id| {
            id == start_id
                || nodes
                    .iter()
                    .find(|node| node.get("id").and_then(Value::as_str) == Some(id.as_str()))
                    .and_then(|node| node.get("type"))
                    .and_then(Value::as_str)
                    .map(|node_type| allowed.contains(node_type))
                    .unwrap_or(false)
        })
        .collect()
}

type Neighbor = (String, String); // (node_id, edge_id)

/// 确定性邻接索引：尊重方向与类型过滤，去重后按 (节点, 边) 排序。
fn build_neighbor_index(
    edges: &[EdgeView],
    direction: TraversalDirection,
    edge_types: Option<&[String]>,
) -> HashMap<String, Vec<Neighbor>> {
    let allowed: Option<HashSet<&str>> =
        edge_types.map(|types| types.iter().map(String::as_str).collect());
    let mut index: HashMap<String, Vec<Neighbor>> = HashMap::new();
    let mut seen: HashMap<String, HashSet<(String, String)>> = HashMap::new();
    let mut add = |source: &str, target: &str, edge_id: &str| {
        let key = (target.to_string(), edge_id.to_string());
        if !seen
            .entry(source.to_string())
            .or_default()
            .insert(key.clone())
        {
            return;
        }
        index.entry(source.to_string()).or_default().push(key);
    };
    for edge in edges {
        if let Some(types) = &allowed {
            if !types.contains(edge.edge_type.as_str()) {
                continue;
            }
        }
        if direction == TraversalDirection::Out || direction == TraversalDirection::Both {
            add(&edge.source, &edge.target, &edge.id);
        }
        if direction == TraversalDirection::In || direction == TraversalDirection::Both {
            add(&edge.target, &edge.source, &edge.id);
        }
        if !edge.directed {
            if direction == TraversalDirection::Out || direction == TraversalDirection::Both {
                add(&edge.target, &edge.source, &edge.id);
            }
            if direction == TraversalDirection::In || direction == TraversalDirection::Both {
                add(&edge.source, &edge.target, &edge.id);
            }
        }
    }
    for neighbors in index.values_mut() {
        neighbors.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    }
    index
}

fn dedup_sorted(items: &[String]) -> Vec<String> {
    let mut items = items.to_vec();
    items.sort();
    items.dedup();
    items
}

fn neighbors_at<'a>(
    neighbors: &'a HashMap<String, Vec<Neighbor>>,
    active: &HashSet<String>,
    current: &str,
) -> Vec<&'a Neighbor> {
    neighbors
        .get(current)
        .map(|items| {
            items
                .iter()
                .filter(|(node, _)| active.contains(node))
                .collect()
        })
        .unwrap_or_default()
}

/// 有界 BFS/DFS，输出深度层与 tree/cross/back 边分类（对齐 TS traverseGraph）。
pub fn traverse_graph(project: &Value, request: &TraversalRequest) -> TraversalResult {
    let started = std::time::Instant::now();
    let edges = resolve_edges_view(project, request.scenario_id.as_deref());
    let active = active_node_ids(
        project,
        request.scenario_id.as_deref(),
        &request.start_id,
        request.node_types.as_deref(),
    );
    let neighbors = build_neighbor_index(&edges, request.direction, request.edge_types.as_deref());
    let mut result = TraversalResult {
        strategy: request.strategy,
        start_id: request.start_id.clone(),
        order: Vec::new(),
        edge_ids: Vec::new(),
        depth: BTreeMap::new(),
        parent: BTreeMap::new(),
        tree_edge_ids: Vec::new(),
        cross_edge_ids: Vec::new(),
        back_edge_ids: Vec::new(),
        stopped_by_depth: Vec::new(),
        duration_ms: 0.0,
    };
    if active.contains(request.start_id.as_str()) {
        match request.strategy {
            TraversalStrategy::Bfs => bfs(request, &neighbors, &active, &mut result),
            TraversalStrategy::Dfs => dfs(request, &neighbors, &active, &mut result),
        }
        result.cross_edge_ids = dedup_sorted(&result.cross_edge_ids);
        result.back_edge_ids = dedup_sorted(&result.back_edge_ids);
    }
    result.duration_ms = started.elapsed().as_secs_f64() * 1000.0;
    result
}

fn bfs(
    request: &TraversalRequest,
    neighbors: &HashMap<String, Vec<Neighbor>>,
    active: &HashSet<String>,
    result: &mut TraversalResult,
) {
    let mut visited: HashSet<String> = HashSet::from([request.start_id.clone()]);
    let mut used_edges: HashSet<String> = HashSet::new();
    let mut tree_edges: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::from([request.start_id.clone()]);
    result.depth.insert(request.start_id.clone(), 0);
    result.parent.insert(request.start_id.clone(), None);
    while let Some(current) = queue.pop_front() {
        result.order.push(current.clone());
        let current_depth = result.depth[&current];
        let list = neighbors_at(neighbors, active, &current);
        if current_depth >= request.max_depth {
            if list
                .iter()
                .any(|(node, _)| !visited.contains(node.as_str()))
            {
                result.stopped_by_depth.push(current.clone());
            }
            continue;
        }
        for (node_id, edge_id) in list {
            if !used_edges.contains(edge_id.as_str()) {
                result.edge_ids.push(edge_id.clone());
                used_edges.insert(edge_id.clone());
            }
            if !visited.contains(node_id.as_str()) {
                visited.insert(node_id.clone());
                result.depth.insert(node_id.clone(), current_depth + 1);
                result.parent.insert(node_id.clone(), Some(current.clone()));
                result.tree_edge_ids.push(edge_id.clone());
                tree_edges.insert(edge_id.clone());
                queue.push_back(node_id.clone());
            } else if !tree_edges.contains(edge_id.as_str()) {
                result.cross_edge_ids.push(edge_id.clone());
            }
        }
    }
}

fn dfs(
    request: &TraversalRequest,
    neighbors: &HashMap<String, Vec<Neighbor>>,
    active: &HashSet<String>,
    result: &mut TraversalResult,
) {
    struct Ctx<'a> {
        request: &'a TraversalRequest,
        neighbors: &'a HashMap<String, Vec<Neighbor>>,
        active: &'a HashSet<String>,
        colors: HashMap<String, u8>,
        used_edges: HashSet<String>,
        tree_edges: HashSet<String>,
        result: &'a mut TraversalResult,
    }
    fn visit(current: &str, ctx: &mut Ctx) {
        ctx.colors.insert(current.to_string(), 1);
        ctx.result.order.push(current.to_string());
        let current_depth = ctx.result.depth[current];
        let list = neighbors_at(ctx.neighbors, ctx.active, current);
        if current_depth >= ctx.request.max_depth {
            if list
                .iter()
                .any(|(node, _)| ctx.colors.get(node.as_str()).copied().unwrap_or(0) == 0)
            {
                ctx.result.stopped_by_depth.push(current.to_string());
            }
            ctx.colors.insert(current.to_string(), 2);
            return;
        }
        for (node_id, edge_id) in list {
            if !ctx.used_edges.contains(edge_id.as_str()) {
                ctx.result.edge_ids.push(edge_id.clone());
                ctx.used_edges.insert(edge_id.clone());
            }
            match ctx.colors.get(node_id.as_str()).copied().unwrap_or(0) {
                0 => {
                    ctx.result
                        .parent
                        .insert(node_id.clone(), Some(current.to_string()));
                    ctx.result.depth.insert(node_id.clone(), current_depth + 1);
                    ctx.result.tree_edge_ids.push(edge_id.clone());
                    ctx.tree_edges.insert(edge_id.clone());
                    visit(node_id, ctx);
                }
                1 => ctx.result.back_edge_ids.push(edge_id.clone()),
                _ => {
                    if !ctx.tree_edges.contains(edge_id.as_str()) {
                        ctx.result.cross_edge_ids.push(edge_id.clone());
                    }
                }
            }
        }
        ctx.colors.insert(current.to_string(), 2);
    }
    let mut ctx = Ctx {
        request,
        neighbors,
        active,
        colors: HashMap::new(),
        used_edges: HashSet::new(),
        tree_edges: HashSet::new(),
        result,
    };
    ctx.result.parent.insert(request.start_id.clone(), None);
    ctx.result.depth.insert(request.start_id.clone(), 0);
    visit(&request.start_id, &mut ctx);
}

/// 用 BFS 返回一条稳定最短路径，沿用有向边语义（对齐 TS shortestPath）。
pub fn shortest_path(
    project: &Value,
    source_id: &str,
    target_id: &str,
    scenario_id: Option<&str>,
) -> Vec<String> {
    let traversal = traverse_graph(
        project,
        &TraversalRequest {
            start_id: source_id.to_string(),
            strategy: TraversalStrategy::Bfs,
            direction: TraversalDirection::Out,
            max_depth: u32::MAX,
            edge_types: None,
            node_types: None,
            scenario_id: scenario_id.map(str::to_string),
        },
    );
    if !traversal.parent.contains_key(target_id) {
        return Vec::new();
    }
    let mut path = vec![target_id.to_string()];
    let mut cursor = target_id.to_string();
    while let Some(parent) = traversal
        .parent
        .get(&cursor)
        .and_then(Option::as_ref)
        .cloned()
    {
        cursor = parent;
        path.insert(0, cursor.clone());
    }
    path
}

/// 便捷请求构造：BFS 外向、无限深（供 `analysis.rs` 场景比对等使用）。
pub(crate) fn bfs_request(start_id: &str, scenario_id: Option<&str>) -> TraversalRequest {
    TraversalRequest {
        start_id: start_id.to_string(),
        strategy: TraversalStrategy::Bfs,
        direction: TraversalDirection::Out,
        max_depth: u32::MAX,
        edge_types: None,
        node_types: None,
        scenario_id: scenario_id.map(str::to_string),
    }
}

/// 枚举至多 100 条等长最短路径（对齐 TS allShortestPaths）。
pub fn all_shortest_paths(
    project: &Value,
    source_id: &str,
    target_id: &str,
    scenario_id: Option<&str>,
) -> Vec<Vec<String>> {
    if source_id == target_id {
        return vec![vec![source_id.to_string()]];
    }
    let edges = resolve_edges_view(project, scenario_id);
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    for edge in &edges {
        outgoing
            .entry(edge.source.clone())
            .or_default()
            .push(edge.target.clone());
        if !edge.directed {
            outgoing
                .entry(edge.target.clone())
                .or_default()
                .push(edge.source.clone());
        }
    }
    for list in outgoing.values_mut() {
        list.sort();
    }
    let mut distance: HashMap<String, u32> = HashMap::from([(source_id.to_string(), 0)]);
    let mut parents: HashMap<String, Vec<String>> = HashMap::new();
    let mut queue: VecDeque<String> = VecDeque::from([source_id.to_string()]);
    while let Some(current) = queue.pop_front() {
        let next_distance = distance[&current] + 1;
        for next in outgoing.get(&current).cloned().unwrap_or_default() {
            if !distance.contains_key(&next) {
                distance.insert(next.clone(), next_distance);
                parents.insert(next.clone(), vec![current.clone()]);
                queue.push_back(next.clone());
            } else if distance[&next] == next_distance {
                let values = parents.entry(next.clone()).or_default();
                if !values.contains(&current) {
                    values.push(current.clone());
                    values.sort();
                }
            }
        }
    }
    if !distance.contains_key(target_id) {
        return Vec::new();
    }
    let mut paths: Vec<Vec<String>> = Vec::new();
    fn build(
        node: &str,
        source: &str,
        parents: &HashMap<String, Vec<String>>,
        suffix: &mut Vec<String>,
        paths: &mut Vec<Vec<String>>,
    ) {
        if node == source {
            let mut path = vec![source.to_string()];
            path.extend(suffix.iter().cloned());
            paths.push(path);
            return;
        }
        for parent in parents.get(node).cloned().unwrap_or_default() {
            suffix.insert(0, node.to_string());
            build(&parent, source, parents, suffix, paths);
            suffix.remove(0);
            if paths.len() >= 100 {
                return;
            }
        }
    }
    build(target_id, source_id, &parents, &mut Vec::new(), &mut paths);
    paths
}
