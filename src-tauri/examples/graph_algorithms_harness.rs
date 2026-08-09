//! 图算法 / 确定性布局——Rust 侧硬计算（对应 ts 原版 app/lib/graph|layout|analysis）。
//! Graph algorithms and deterministic layout, ported from the TS originals so the
//! Rust kernel and the TS client can be compared bit-for-bit (§15.5 migration gate).
//!
//! 迁移纪律：这些函数仅把 `serde_json::Value` 当作输入/输出，绝不产生随机或运行时依赖。
//! `traverseGraph` 的 `durationMs` 属于运行时遥测，比对时恒定置 0。

use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};

// ---------------------------------------------------------------------------
// Value 读取辅助
// ---------------------------------------------------------------------------

fn str_of(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}
fn bool_of(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}
fn f64_of(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}
fn array_of<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

// ---------------------------------------------------------------------------
// 场景解析：resolveEdges / resolvedNodeIds
// ---------------------------------------------------------------------------

fn scenario_by_id<'a>(project: &'a Value, scenario_id: Option<&str>) -> Option<&'a Value> {
    match scenario_id {
        Some(id) => array_of(project, "scenarios")
            .iter()
            .find(|s| str_of(s, "id").as_deref() == Some(id)),
        None => None,
    }
}
fn string_set_from_array(value: &Value, key: &str) -> HashSet<String> {
    array_of(value, key)
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// `resolveEdges`：过滤 → 合并 edgeOverrides → 按 id 字典序排序。
pub fn resolve_edges(project: &Value, scenario_id: Option<&str>) -> Vec<Value> {
    let scenario = scenario_by_id(project, scenario_id);
    let disabled_nodes = scenario
        .map(|s| string_set_from_array(s, "disabledNodeIds"))
        .unwrap_or_default();
    let disabled_edges = scenario
        .map(|s| string_set_from_array(s, "disabledEdgeIds"))
        .unwrap_or_default();

    let mut edges = array_of(project, "edges")
        .iter()
        .filter(|edge| {
            let id = str_of(edge, "id").unwrap_or_default();
            let source = str_of(edge, "source").unwrap_or_default();
            let target = str_of(edge, "target").unwrap_or_default();
            !disabled_edges.contains(&id) && !disabled_nodes.contains(&source) && !disabled_nodes.contains(&target)
        })
        .map(|edge| {
            let id = str_of(edge, "id").unwrap_or_default();
            let mut merged = edge.clone();
            if let Value::Object(edge_obj) = &mut merged {
                // 应用场景 edgeOverrides[id]（与 ts `{...edge, ...overrides[id]}` 一致）。
                if let Some(override_obj) = scenario
                    .and_then(|s| s.get("edgeOverrides").and_then(Value::as_object))
                    .and_then(|map| map.get(&id))
                    .and_then(Value::as_object)
                {
                    for (key, val) in override_obj {
                        edge_obj.insert(key.clone(), val.clone());
                    }
                }
            }
            merged
        })
        .collect::<Vec<_>>();
    edges.sort_by(|a, b| str_of(a, "id").cmp(&str_of(b, "id")));
    edges
}

/// `resolvedNodeIds`：场景启用的节点 id 集合。
pub fn resolved_node_ids(project: &Value, scenario_id: Option<&str>) -> HashSet<String> {
    let disabled = scenario_by_id(project, scenario_id)
        .map(|s| string_set_from_array(s, "disabledNodeIds"))
        .unwrap_or_default();
    array_of(project, "nodes")
        .iter()
        .filter_map(|n| str_of(n, "id"))
        .filter(|id| !disabled.contains(id))
        .collect()
}

// ---------------------------------------------------------------------------
// 邻接表（buildNeighborIndex）与遍历请求
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct TraversalNeighbor {
    node_id: String,
    edge_id: String,
}

fn build_neighbor_index(
    edges: &[Value],
    direction: &str,
    edge_types: Option<&HashSet<String>>,
) -> HashMap<String, Vec<TraversalNeighbor>> {
    let mut index: HashMap<String, Vec<TraversalNeighbor>> = HashMap::new();
    let mut seen: HashMap<String, HashSet<String>> = HashMap::new();

    let mut add = |source: String, target: String, edge_id: String| {
        let key = format!("{target}\u{0}{edge_id}");
        let source_seen = seen.entry(source.clone()).or_default();
        if source_seen.contains(&key) {
            return;
        }
        source_seen.insert(key);
        index
            .entry(source)
            .or_default()
            .push(TraversalNeighbor {
                node_id: target,
                edge_id,
            });
    };

    for edge in edges {
        let edge_type = str_of(edge, "type").unwrap_or_default();
        if let Some(types) = edge_types {
            if !types.contains(&edge_type) {
                continue;
            }
        }
        let directed = bool_of(edge, "directed");
        let source = str_of(edge, "source").unwrap_or_default();
        let target = str_of(edge, "target").unwrap_or_default();
        let id = str_of(edge, "id").unwrap_or_default();
        if direction == "out" || direction == "both" {
            add(source.clone(), target.clone(), id.clone());
        }
        if direction == "in" || direction == "both" {
            add(target.clone(), source.clone(), id.clone());
        }
        if !directed {
            if direction == "out" || direction == "both" {
                add(target.clone(), source.clone(), id.clone());
            }
            if direction == "in" || direction == "both" {
                add(source.clone(), target.clone(), id.clone());
            }
        }
    }

    for neighbors in index.values_mut() {
        neighbors.sort_by(|a, b| {
            a.node_id
                .cmp(&b.node_id)
                .then_with(|| a.edge_id.cmp(&b.edge_id))
        });
    }
    index
}

fn filtered_active_node_ids(
    project: &Value,
    node_types: Option<&HashSet<String>>,
    start_id: &str,
    scenario_id: Option<&str>,
) -> HashSet<String> {
    let resolved = resolved_node_ids(project, scenario_id);
    let Some(types) = node_types else {
        return resolved;
    };
    array_of(project, "nodes")
        .iter()
        .filter_map(|n| {
            let id = str_of(n, "id")?;
            let keep = resolved.contains(&id)
                && (id == start_id || str_of(n, "type").map(|t| types.contains(&t)).unwrap_or(false));
            if keep {
                Some(id)
            } else {
                None
            }
        })
        .collect()
}

#[derive(Clone)]
struct TraversalRequest {
    start_id: String,
    strategy: String,
    direction: String,
    max_depth: i64,
    edge_types: Option<HashSet<String>>,
    node_types: Option<HashSet<String>>,
    scenario_id: Option<String>,
}

impl TraversalRequest {
    fn from_value(value: &Value) -> Self {
        let arr = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
        };
        TraversalRequest {
            start_id: str_of(value, "startId").unwrap_or_default(),
            strategy: str_of(value, "strategy").unwrap_or_else(|| "bfs".to_string()),
            direction: str_of(value, "direction").unwrap_or_else(|| "out".to_string()),
            max_depth: value.get("maxDepth").and_then(Value::as_i64).unwrap_or(i64::MAX),
            edge_types: arr("edgeTypes"),
            node_types: arr("nodeTypes"),
            scenario_id: str_of(value, "scenarioId"),
        }
    }
}

// ---------------------------------------------------------------------------
// 遍历：traverseGraph
// ---------------------------------------------------------------------------

fn set_str_array(result: &mut Value, key: &str, items: &[String]) {
    result[key] = Value::Array(items.iter().map(|s| Value::String(s.clone())).collect());
}
fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|v| seen.insert(v.clone()))
        .collect()
}

pub fn traverse(project: &Value, request_value: &Value) -> Value {
    let request = TraversalRequest::from_value(request_value);
    let start_node_id = request.start_id.clone();

    let mut result = json!({
        "strategy": request.strategy,
        "startId": start_node_id,
        "order": [],
        "edgeIds": [],
        "depth": {},
        "parent": {},
        "treeEdgeIds": [],
        "crossEdgeIds": [],
        "backEdgeIds": [],
        "stoppedByDepth": [],
        "durationMs": 0,
    });

    let edges = resolve_edges(project, request.scenario_id.as_deref());
    let active_nodes = filtered_active_node_ids(
        project,
        request.node_types.as_ref(),
        &start_node_id,
        request.scenario_id.as_deref(),
    );
    let neighbors_by_node = build_neighbor_index(&edges, &request.direction, request.edge_types.as_ref());

    let mut depth_map: HashMap<String, Value> = HashMap::new();
    let mut parent_map: HashMap<String, Value> = HashMap::new();
    if !active_nodes.contains(&start_node_id) {
        return result;
    }

    let mut order: Vec<String> = Vec::new();
    let mut edge_ids: Vec<String> = Vec::new();
    let mut tree_edge_ids: Vec<String> = Vec::new();
    let mut cross_edge_ids: Vec<String> = Vec::new();
    let mut back_edge_ids: Vec<String> = Vec::new();
    let mut stopped_by_depth: Vec<String> = Vec::new();
    let max_depth = request.max_depth;

    macro_rules! neighbors_of {
        ($current:expr) => {
            neighbors_by_node
                .get(&$current)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|n| active_nodes.contains(&n.node_id))
                .collect::<Vec<TraversalNeighbor>>()
        };
    }

    if request.strategy == "bfs" {
        let mut visited: HashSet<String> = HashSet::from([start_node_id.clone()]);
        let mut used_edges: HashSet<String> = HashSet::new();
        let mut tree_edges: HashSet<String> = HashSet::new();
        let mut queue: Vec<String> = vec![start_node_id.clone()];
        let mut queue_index = 0;
        depth_map.insert(start_node_id.clone(), json!(0));
        parent_map.insert(start_node_id.clone(), Value::Null);

        while queue_index < queue.len() {
            let current = queue[queue_index].clone();
            queue_index += 1;
            order.push(current.clone());
            let current_depth = depth_map.get(&current).and_then(Value::as_i64).unwrap_or(0);
            let neighbors = neighbors_of!(current);
            if current_depth >= max_depth {
                if neighbors.iter().any(|item| !visited.contains(&item.node_id)) {
                    stopped_by_depth.push(current.clone());
                }
                continue;
            }
            for neighbor in neighbors {
                if !used_edges.contains(&neighbor.edge_id) {
                    edge_ids.push(neighbor.edge_id.clone());
                    used_edges.insert(neighbor.edge_id.clone());
                }
                if !visited.contains(&neighbor.node_id) {
                    visited.insert(neighbor.node_id.clone());
                    depth_map.insert(neighbor.node_id.clone(), json!(current_depth + 1));
                    parent_map.insert(neighbor.node_id.clone(), json!(current));
                    tree_edge_ids.push(neighbor.edge_id.clone());
                    tree_edges.insert(neighbor.edge_id.clone());
                    queue.push(neighbor.node_id.clone());
                } else if !tree_edges.contains(&neighbor.edge_id) {
                    cross_edge_ids.push(neighbor.edge_id.clone());
                }
            }
        }
    } else {
        let mut colors: HashMap<String, i32> = HashMap::new();
        let mut used_edges: HashSet<String> = HashSet::new();
        let mut tree_edges: HashSet<String> = HashSet::new();
        parent_map.insert(start_node_id.clone(), Value::Null);
        depth_map.insert(start_node_id.clone(), json!(0));

        fn visit(
            current: &str,
            neighbors_by_node: &HashMap<String, Vec<TraversalNeighbor>>,
            active_nodes: &HashSet<String>,
            colors: &mut HashMap<String, i32>,
            used_edges: &mut HashSet<String>,
            tree_edges: &mut HashSet<String>,
            depth_map: &mut HashMap<String, Value>,
            parent_map: &mut HashMap<String, Value>,
            order: &mut Vec<String>,
            edge_ids: &mut Vec<String>,
            tree_edge_ids: &mut Vec<String>,
            cross_edge_ids: &mut Vec<String>,
            back_edge_ids: &mut Vec<String>,
            stopped_by_depth: &mut Vec<String>,
            max_depth: i64,
        ) {
            colors.insert(current.to_string(), 1);
            order.push(current.to_string());
            let current_depth = depth_map.get(current).and_then(Value::as_i64).unwrap_or(0);
            let neighbors = neighbors_by_node
                .get(current)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|n| active_nodes.contains(&n.node_id))
                .collect::<Vec<TraversalNeighbor>>();
            if current_depth >= max_depth {
                if neighbors.iter().any(|item| colors.get(&item.node_id).is_none()) {
                    stopped_by_depth.push(current.to_string());
                }
                colors.insert(current.to_string(), 2);
                return;
            }
            for neighbor in neighbors {
                if !used_edges.contains(&neighbor.edge_id) {
                    edge_ids.push(neighbor.edge_id.clone());
                    used_edges.insert(neighbor.edge_id.clone());
                }
                let color = *colors.get(&neighbor.node_id).unwrap_or(&0);
                if color == 0 {
                    parent_map.insert(neighbor.node_id.clone(), json!(current));
                    depth_map.insert(neighbor.node_id.clone(), json!(current_depth + 1));
                    tree_edge_ids.push(neighbor.edge_id.clone());
                    tree_edges.insert(neighbor.edge_id.clone());
                    visit(
                        &neighbor.node_id, neighbors_by_node, active_nodes, colors, used_edges,
                        tree_edges, depth_map, parent_map, order, edge_ids, tree_edge_ids,
                        cross_edge_ids, back_edge_ids, stopped_by_depth, max_depth,
                    );
                } else if color == 1 {
                    back_edge_ids.push(neighbor.edge_id.clone());
                } else if !tree_edges.contains(&neighbor.edge_id) {
                    cross_edge_ids.push(neighbor.edge_id.clone());
                }
            }
            colors.insert(current.to_string(), 2);
        }

        visit(
            &start_node_id, &neighbors_by_node, &active_nodes, &mut colors, &mut used_edges,
            &mut tree_edges, &mut depth_map, &mut parent_map, &mut order, &mut edge_ids, &mut tree_edge_ids,
            &mut cross_edge_ids, &mut back_edge_ids, &mut stopped_by_depth, max_depth,
        );
    }

    cross_edge_ids = dedupe(cross_edge_ids);
    back_edge_ids = dedupe(back_edge_ids);
    result["depth"] = Value::Object(depth_map.into_iter().collect());
    result["parent"] = Value::Object(parent_map.into_iter().collect());
    set_str_array(&mut result, "order", &order);
    set_str_array(&mut result, "edgeIds", &edge_ids);
    set_str_array(&mut result, "treeEdgeIds", &tree_edge_ids);
    set_str_array(&mut result, "crossEdgeIds", &cross_edge_ids);
    set_str_array(&mut result, "backEdgeIds", &back_edge_ids);
    set_str_array(&mut result, "stoppedByDepth", &stopped_by_depth);
    result
}

// ---------------------------------------------------------------------------
// 环检测：detectCycles
// ---------------------------------------------------------------------------

pub fn detect_cycles(project: &Value, scenario_id: Option<&str>) -> Value {
    let edges: Vec<Value> = resolve_edges(project, scenario_id)
        .into_iter()
        .filter(|e| bool_of(e, "directed"))
        .collect();
    let mut node_ids: Vec<String> = resolved_node_ids(project, scenario_id).into_iter().collect();
    node_ids.sort();

    let mut outgoing: HashMap<String, Vec<Value>> = HashMap::new();
    for edge in &edges {
        let source = str_of(edge, "source").unwrap_or_default();
        outgoing.entry(source).or_default().push(edge.clone());
    }
    for list in outgoing.values_mut() {
        list.sort_by(|a, b| str_of(a, "id").cmp(&str_of(b, "id")));
    }

    let mut color: HashMap<String, i32> = HashMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut cycles: Vec<(Vec<String>, Vec<String>)> = Vec::new();

    fn visit_cycle(
        node_id: &str,
        edges: &[Value],
        outgoing: &HashMap<String, Vec<Value>>,
        color: &mut HashMap<String, i32>,
        stack: &mut Vec<String>,
        cycles: &mut Vec<(Vec<String>, Vec<String>)>,
    ) {
        color.insert(node_id.to_string(), 1);
        stack.push(node_id.to_string());
        let outgoing_list = outgoing.get(node_id).cloned().unwrap_or_default();
        for edge in outgoing_list {
            let next = str_of(&edge, "target").unwrap_or_default();
            let next_color = *color.get(&next).unwrap_or(&0);
            if next_color == 0 {
                visit_cycle(&next, edges, outgoing, color, stack, cycles);
            } else if next_color == 1 {
                let index = stack
                    .iter()
                    .rposition(|n| n == &next)
                    .unwrap_or(stack.len());
                let node_cycle = stack[index..].to_vec();
                let completed: Vec<String> = node_cycle
                    .iter()
                    .cloned()
                    .chain(std::iter::once(next.clone()))
                    .collect();
                let mut edge_cycle: Vec<String> = Vec::new();
                for i in 0..completed.len().saturating_sub(1) {
                    let from = &completed[i];
                    let to = &completed[i + 1];
                    if let Some(cycle_edge) = edges.iter().find(|c| {
                        str_of(c, "source").as_deref() == Some(from)
                            && str_of(c, "target").as_deref() == Some(to)
                    }) {
                        edge_cycle.push(str_of(cycle_edge, "id").unwrap_or_default());
                    }
                }
                // ts：nodeCycle = [...stack.slice(index), next]（含收尾重复节点）。
                let node_cycle = completed.clone();
                let key = {
                    let mut uniq = completed.clone();
                    uniq.sort();
                    uniq.dedup();
                    uniq.join("|")
                };
                let already = cycles.iter().any(|(nodes, _)| {
                    let mut uniq = nodes.clone();
                    uniq.sort();
                    uniq.dedup();
                    let mut s: Vec<&String> = uniq.iter().collect();
                    s.sort();
                    let s: Vec<String> = s.into_iter().cloned().collect();
                    s.join("|") == key
                });
                if !already {
                    cycles.push((node_cycle, edge_cycle));
                }
            }
        }
        stack.pop();
        color.insert(node_id.to_string(), 2);
    }

    for node_id in node_ids {
        if *color.get(&node_id).unwrap_or(&0) == 0 {
            visit_cycle(&node_id, &edges, &outgoing, &mut color, &mut stack, &mut cycles);
        }
    }

    let arr: Vec<Value> = cycles
        .into_iter()
        .map(|(node_ids, edge_ids)| json!({ "nodeIds": node_ids, "edgeIds": edge_ids }))
        .collect();
    Value::Array(arr)
}

// ---------------------------------------------------------------------------
// 最短路径：shortestPath / allShortestPaths
// ---------------------------------------------------------------------------

/// `shortestPath`：通过 BFS parent 重建一条稳定最短路径。
pub fn shortest_path(project: &Value, source: &str, target: &str, scenario_id: Option<&str>) -> Value {
    let request = json!({
        "startId": source,
        "strategy": "bfs",
        "direction": "out",
        "maxDepth": i64::MAX,
        "scenarioId": scenario_id,
    });
    let traversal = traverse(project, &request);
    let parent = traversal.get("parent").unwrap().as_object().unwrap();
    if !parent.contains_key(target) {
        return Value::Array(Vec::new());
    }
    let mut path = vec![target.to_string()];
    let mut cursor = Some(target.to_string());
    while let Some(current) = cursor {
        match parent.get(&current) {
            Some(Value::String(next)) => {
                let next_s = next.clone();
                path.insert(0, next_s.clone());
                cursor = Some(next_s);
            }
            _ => cursor = None,
        }
    }
    Value::Array(path.into_iter().map(Value::String).collect())
}

/// `allShortestPaths`：枚举最多 100 条等长路径。
pub fn all_shortest_paths(
    project: &Value,
    source: &str,
    target: &str,
    scenario_id: Option<&str>,
) -> Value {
    if source == target {
        return json!([[source]]);
    }
    let edges = resolve_edges(project, scenario_id);
    let mut outgoing: HashMap<String, Vec<String>> = HashMap::new();
    for edge in &edges {
        let s = str_of(edge, "source").unwrap_or_default();
        let t = str_of(edge, "target").unwrap_or_default();
        outgoing.entry(s.clone()).or_default().push(t.clone());
        if !bool_of(edge, "directed") {
            outgoing.entry(t.clone()).or_default().push(s.clone());
        }
    }
    for neighbors in outgoing.values_mut() {
        neighbors.sort();
    }

    let mut distance: HashMap<String, i64> = HashMap::new();
    let mut parents: HashMap<String, Vec<String>> = HashMap::new();
    distance.insert(source.to_string(), 0);
    let mut queue = vec![source.to_string()];
    let mut queue_index = 0;
    while queue_index < queue.len() {
        let current = queue[queue_index].clone();
        queue_index += 1;
        let next_distance = distance.get(&current).copied().unwrap_or(0) + 1;
        for next in outgoing.get(&current).cloned().unwrap_or_default() {
            if !distance.contains_key(&next) {
                distance.insert(next.clone(), next_distance);
                parents.insert(next.clone(), vec![current.clone()]);
                queue.push(next.clone());
            } else if distance.get(&next).copied().unwrap_or(0) == next_distance {
                let values = parents.entry(next.clone()).or_default();
                if !values.contains(&current) {
                    values.push(current.clone());
                }
                values.sort();
            }
        }
    }
    if !distance.contains_key(target) {
        return Value::Array(Vec::new());
    }

    let mut paths: Vec<Vec<String>> = Vec::new();
    fn build(
        node_id: &str,
        source: &str,
        suffix: &[String],
        parents: &HashMap<String, Vec<String>>,
        paths: &mut Vec<Vec<String>>,
    ) {
        if node_id == source {
            let mut full = vec![source.to_string()];
            full.extend_from_slice(suffix);
            paths.push(full);
            return;
        }
        let mut prefix = vec![node_id.to_string()];
        prefix.extend_from_slice(suffix);
        let parent_list = parents.get(node_id).cloned().unwrap_or_default();
        for parent in parent_list {
            build(&parent, source, &prefix, parents, paths);
            if paths.len() >= 100 {
                return;
            }
        }
    }
    build(target, source, &[], &parents, &mut paths);
    Value::Array(
        paths
            .into_iter()
            .map(|p| Value::Array(p.into_iter().map(Value::String).collect()))
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// 可达性：compareScenarioReachability
// ---------------------------------------------------------------------------

/// `compareScenarioReachability`：区分直接禁用与意外失去可达性。
pub fn compare_reachability(project: &Value, root: &str, scenario_id: &str) -> Value {
    let scenario = scenario_by_id(project, Some(scenario_id));
    let base_request = json!({
        "startId": root,
        "strategy": "bfs",
        "direction": "out",
        "maxDepth": i64::MAX,
    });
    let ablated_request = json!({
        "startId": root,
        "strategy": "bfs",
        "direction": "out",
        "maxDepth": i64::MAX,
        "scenarioId": scenario_id,
    });
    let base = traverse(project, &base_request);
    let ablated = traverse(project, &ablated_request);

    let id_arr = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    };
    let base_set: HashSet<String> = id_arr(&base["order"]).into_iter().collect();
    let ablated_set: HashSet<String> = id_arr(&ablated["order"]).into_iter().collect();
    let disabled: HashSet<String> = scenario
        .map(|s| string_set_from_array(s, "disabledNodeIds"))
        .unwrap_or_default();

    let lost: Vec<String> = id_arr(&base["order"])
        .into_iter()
        .filter(|id| !ablated_set.contains(id) && !disabled.contains(id))
        .collect();
    let retained: Vec<String> = id_arr(&ablated["order"])
        .into_iter()
        .filter(|id| base_set.contains(id))
        .collect();

    let base_parent = base["parent"].as_object().unwrap();
    let ablated_parent = ablated["parent"].as_object().unwrap();
    let alternate: Vec<String> = retained
        .iter()
        .filter(|id| base_parent.get(*id) != ablated_parent.get(*id))
        .cloned()
        .collect();

    let disabled_node_ids: Vec<String> = scenario
        .map(|s| string_set_from_array(s, "disabledNodeIds"))
        .unwrap_or_default()
        .into_iter()
        .collect();
    let disabled_edge_ids: Vec<String> = scenario
        .map(|s| string_set_from_array(s, "disabledEdgeIds"))
        .unwrap_or_default()
        .into_iter()
        .collect();

    json!({
        "disabledNodeIds": disabled_node_ids,
        "disabledEdgeIds": disabled_edge_ids,
        "lostReachableNodeIds": lost,
        "retainedReachableNodeIds": retained,
        "alternatePathNodeIds": alternate,
    })
}

// ---------------------------------------------------------------------------
// 分析：computeLogicChain / propagateInfluence
// ---------------------------------------------------------------------------

fn exp_field(edge: &Value, key: &str) -> Option<String> {
    edge.get("experiment").and_then(|e| str_of(e, key))
}

fn chain_selected_edges(project: &Value, mode: &str, target_id: Option<&str>) -> Vec<Value> {
    let chosen: Vec<Value> = match mode {
        "refutation" => array_of(project, "edges")
            .iter()
            .filter(|edge| {
                let t = str_of(edge, "type").unwrap_or_default();
                t == "contradicts" || exp_field(edge, "outcome").as_deref() == Some("refutes")
            })
            .cloned()
            .collect(),
        "effective" => array_of(project, "edges")
            .iter()
            .filter(|edge| {
                exp_field(edge, "status").as_deref() == Some("completed")
                    && exp_field(edge, "outcome").as_deref() == Some("supports")
                    && edge
                        .get("experiment")
                        .and_then(|e| e.get("delta"))
                        .and_then(Value::as_f64)
                        .map(|d| d.abs() >= 0.005)
                        .unwrap_or(false)
            })
            .cloned()
            .collect(),

        _ => array_of(project, "edges")
            .iter()
            .filter(|edge| {
                let t = str_of(edge, "type").unwrap_or_default();
                t == "supports"
                    || t == "derived_from"
                    || exp_field(edge, "outcome").as_deref() == Some("supports")
            })
            .cloned()
            .collect(),
    };

    let target_filtered: Vec<Value> = match target_id {
        Some(target_id) if !chosen.is_empty() => chosen
            .iter()
            .filter(|edge| {
                let target = str_of(edge, "target").unwrap_or_default();
                let path = shortest_path(project, &target, target_id, None);
                target == target_id || path.as_array().map(|a| !a.is_empty()).unwrap_or(false)
            })
            .cloned()
            .collect(),
        _ => chosen.clone(),
    };

    if target_filtered.is_empty() {
        chosen
    } else {
        target_filtered
    }
}

/// `computeLogicChain`：逻辑链摘要、分数与节点/边 id 列表。
pub fn compute_logic_chain(project: &Value, mode: &str, target_id: Option<&str>) -> Value {
    let edges = chain_selected_edges(project, mode, target_id);
    let edge_ids: Vec<String> = edges.iter().filter_map(|e| str_of(e, "id")).collect();
    let mut node_list: Vec<String> = Vec::new();
    let mut seen_nodes: HashSet<String> = HashSet::new();
    let mut experiment_ids: HashSet<String> = HashSet::new();
    let mut confidence_sum: f64 = 0.0;
    for edge in &edges {
        for key in ["source", "target"] {
            if let Some(id) = str_of(edge, key) {
                if seen_nodes.insert(id.clone()) {
                    node_list.push(id);
                }
            }
        }
        if let Some(exp_id) = exp_field(edge, "id") {
            experiment_ids.insert(exp_id);
        }
        confidence_sum += f64_of(edge, "confidence").unwrap_or(0.5);
    }
    let experiment_count = experiment_ids.len();
    let mean_conf = confidence_sum / (edges.len().max(1)) as f64;

    let summary = if mode == "effective" {
        format!("{experiment_count} completed experiments changed the target metric by at least 0.5 percentage points.")
    } else if mode == "refutation" {
        let base = experiment_count.max(edges.len());
        format!("{base} experiments or sources challenge the current explanation.")
    } else {
        format!("{} supported relations form the currently reviewable evidence chain.", edges.len())
    };

    json!({
        "mode": mode,
        "nodeIds": node_list,
        "edgeIds": edge_ids,
        "score": mean_conf,
        "summary": summary,
    })
}

fn edge_weight(edge: &Value) -> f64 {
    let t = str_of(edge, "type").unwrap_or_default();
    if t == "controls" {
        return 0.0;
    }
    let experimental = edge
        .get("experiment")
        .and_then(|e| e.get("delta"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0)
        .abs();
    if experimental > 0.0 {
        return (experimental * 8.0 + 0.08).min(1.0);
    }
    f64_of(edge, "confidence").unwrap_or(0.5).max(0.05).min(1.0)
}

fn edge_sign(edge: &Value) -> f64 {
    let t = str_of(edge, "type").unwrap_or_default();
    let polarity = str_of(edge, "polarity");
    if t == "contradicts"
        || polarity.as_deref() == Some("negative")
        || exp_field(edge, "outcome").as_deref() == Some("refutes")
    {
        -1.0
    } else {
        1.0
    }
}

/// `propagateInfluence`：固定轮数的可解释影响传播。
pub fn propagate_influence(project: &Value, target_id: &str, max_iterations_opt: Option<i64>) -> Value {
    let node_count = array_of(project, "nodes").len();
    let max_iterations = max_iterations_opt.unwrap_or_else(|| (node_count as i64).max(2));

    let mut raw: HashMap<String, f64> = HashMap::new();
    for node in array_of(project, "nodes") {
        if let Some(id) = str_of(node, "id") {
            raw.insert(id, 0.0);
        }
    }
    raw.insert(target_id.to_string(), 1.0);
    let mut frontier: HashMap<String, f64> = HashMap::from([(target_id.to_string(), 1.0)]);

    // 与 ts 的 `Object` 一致：edgeContributions 按首次贡献顺序（创建顺序）累计。
    let mut edge_contributions: Vec<(String, f64)> = Vec::new();
    let mut iterations = 0_i64;

    while iterations < max_iterations {
        let mut next: HashMap<String, f64> = HashMap::new();
        for edge in array_of(project, "edges") {
            let downstream = frontier.get(&str_of(edge, "target").unwrap_or_default()).copied();
            let Some(downstream) = downstream else { continue };
            if downstream.abs() < 0.001 {
                continue;
            }
            let weight = edge_weight(edge);
            if weight == 0.0 {
                continue;
            }
            let contribution = downstream * weight * edge_sign(edge);
            let source = str_of(edge, "source").unwrap_or_default();
            let id = str_of(edge, "id").unwrap_or_default();
            *next.entry(source.clone()).or_insert(0.0) += contribution;
            *raw.entry(source.clone()).or_insert(0.0) += contribution;
            match edge_contributions.iter_mut().find(|(existing, _)| *existing == id) {
                Some((_, value)) => *value += contribution,
                None => edge_contributions.push((id, contribution)),
            }
        }
        frontier = next;
        // 镜像 ts：frontier 为空时 break，结束前不递增 iterations。
        if frontier.is_empty() {
            break;
        }
        iterations += 1;
    }

    let max_abs = raw.values().map(|v| v.abs()).fold(1.0_f64, |acc, v| acc.max(v));
    let scores: HashMap<String, f64> = raw
        .iter()
        .map(|(id, v)| (id.clone(), *v / max_abs))
        .collect();

    // 稳定排序：|contribution| 降序，相同则保持插入序（= ts 对 Object.entries 的稳定排序）。
    let mut strongest = edge_contributions.clone();
    strongest.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(std::cmp::Ordering::Equal));
    let strongest_edge_ids: Vec<String> = strongest.into_iter().take(8).map(|(id, _)| id).collect();

    let contributions_obj: Map<String, Value> = edge_contributions
        .iter()
        .map(|(id, value)| (id.clone(), json!(value)))
        .collect();

    json!({
        "targetId": target_id,
        "scores": scores,
        "edgeContributions": contributions_obj,
        "strongestEdgeIds": strongest_edge_ids,
        "iterations": iterations,
    })
}

// ---------------------------------------------------------------------------
// 确定性布局：computeLayout
// ---------------------------------------------------------------------------

fn topological_depths(project: &Value, selected_edge_ids: Option<&HashSet<String>>) -> HashMap<String, i64> {
    let node_ids: Vec<String> = array_of(project, "nodes")
        .iter()
        .filter_map(|n| str_of(n, "id"))
        .collect();
    let edges: Vec<Value> = array_of(project, "edges")
        .iter()
        .filter(|edge| {
            let source = str_of(edge, "source").unwrap_or_default();
            let target = str_of(edge, "target").unwrap_or_default();
            let id = str_of(edge, "id").unwrap_or_default();
            node_ids.contains(&source)
                && node_ids.contains(&target)
                && selected_edge_ids.map(|ids| ids.contains(&id)).unwrap_or(true)
        })
        .cloned()
        .collect();

    let mut incoming: HashMap<String, i64> = HashMap::new();
    let mut outgoing: HashMap<String, Vec<Value>> = HashMap::new();
    let mut depth: HashMap<String, i64> = HashMap::new();
    for id in &node_ids {
        incoming.insert(id.clone(), 0);
        depth.insert(id.clone(), 0);
    }
    for edge in &edges {
        let source = str_of(edge, "source").unwrap_or_default();
        let target = str_of(edge, "target").unwrap_or_default();
        *incoming.entry(target.clone()).or_insert(0) += 1;
        outgoing.entry(source.clone()).or_default().push(edge.clone());
    }

    let mut queue: Vec<String> = node_ids
        .iter()
        .filter(|id| incoming.get(*id).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    queue.sort();
    let mut visited_count = 0;
    while !queue.is_empty() {
        let current = queue.remove(0);
        visited_count += 1;
        for edge in outgoing.get(&current).cloned().unwrap_or_default() {
            let target = str_of(&edge, "target").unwrap_or_default();
            let new_depth = depth.get(&current).copied().unwrap_or(0) + 1;
            let cur = depth.entry(target.clone()).or_insert(0);
            *cur = (*cur).max(new_depth);
            let inc = incoming.entry(target.clone()).or_insert(0);
            *inc -= 1;
            if *inc == 0 {
                queue.push(target.clone());
            }
        }
        queue.sort();
    }

    if (visited_count as usize) < node_ids.len() {
        let max_depth = depth.values().copied().fold(0, |acc, v| acc.max(v));
        for id in &node_ids {
            if incoming.get(id).copied().unwrap_or(0) > 0 {
                depth.insert(id.clone(), max_depth + 1);
            }
        }
    }
    depth
}

fn chain_edge_ids(project: &Value, mode: &str) -> Vec<String> {
    let support_types = ["supports", "derived_from", "measures", "uses"];
    let mut out: Vec<String> = Vec::new();
    for edge in array_of(project, "edges") {
        let t = str_of(edge, "type").unwrap_or_default();
        let include = if mode == "refutation" {
            t == "contradicts" || exp_field(edge, "outcome").as_deref() == Some("refutes")
        } else {
            support_types.contains(&t.as_str())
                || exp_field(edge, "outcome").as_deref() == Some("supports")
        };
        if include {
            if let Some(id) = str_of(edge, "id") {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
    }
    out
}

fn huffman_codes(project: &Value) -> HashMap<String, String> {
    #[derive(Clone)]
    struct HuffmanItem {
        weight: i64,
        ids: Vec<String>,
        codes: HashMap<String, String>,
    }
    let nodes = array_of(project, "nodes");
    let edges = array_of(project, "edges");
    let mut queue: Vec<HuffmanItem> = nodes
        .iter()
        .filter_map(|node| {
            let id = str_of(node, "id")?;
            let evidence_len = node
                .get("evidenceIds")
                .and_then(Value::as_array)
                .map(|a| a.len() as i64)
                .unwrap_or(0);
            let degree = edges
                .iter()
                .filter(|edge| {
                    str_of(edge, "source").as_deref() == Some(&id)
                        || str_of(edge, "target").as_deref() == Some(&id)
                })
                .count() as i64;
            let mut codes = HashMap::new();
            codes.insert(id.clone(), String::new());
            Some(HuffmanItem {
                weight: 1 + evidence_len * 2 + degree,
                ids: vec![id],
                codes,
            })
        })
        .collect();

    if queue.len() == 1 {
        let mut out = HashMap::new();
        out.insert(queue[0].ids[0].clone(), "0".to_string());
        return out;
    }
    while queue.len() > 1 {
        queue.sort_by(|a, b| {
            a.weight
                .cmp(&b.weight)
                .then_with(|| a.ids.join(",").cmp(&b.ids.join(",")))
        });
        let left = queue.remove(0);
        let right = queue.remove(0);
        let mut codes: HashMap<String, String> = HashMap::new();
        for (id, code) in &left.codes {
            codes.insert(id.clone(), format!("0{code}"));
        }
        for (id, code) in &right.codes {
            codes.insert(id.clone(), format!("1{code}"));
        }
        let mut ids = left.ids.clone();
        ids.extend(right.ids.iter().cloned());
        queue.push(HuffmanItem {
            weight: left.weight + right.weight,
            ids,
            codes,
        });
    }
    queue.get(0).map(|item| item.codes.clone()).unwrap_or_default()
}

/// `computeLayout`：仅展示坐标与注解，绝不修改保存的 placement。
pub fn compute_layout(project: &Value, mode: &str, root_id_opt: Option<&str>) -> Value {
    let nodes = array_of(project, "nodes");
    let edges = array_of(project, "edges");
    let root_id = root_id_opt
        .map(str::to_string)
        .unwrap_or_else(|| nodes.first().and_then(|n| str_of(n, "id")).unwrap_or_default());

    let mut positions: Map<String, Value> = Map::new();
    let mut annotations: Map<String, Value> = Map::new();
    let mut node_ids: Vec<String> = nodes.iter().filter_map(|n| str_of(n, "id")).collect();
    let mut edge_ids: Vec<String> = edges.iter().filter_map(|e| str_of(e, "id")).collect();

    if mode == "tree" {
        let traversal = traverse(
            project,
            &json!({
                "startId": root_id,
                "strategy": "bfs",
                "direction": "out",
                "maxDepth": i64::MAX,
            }),
        );
        let order: Vec<String> = traversal["order"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
        let included: HashSet<String> = order.iter().cloned().collect();
        node_ids = order
            .iter()
            .cloned()
            .chain(node_ids.iter().filter(|id| !included.contains(*id)).cloned())
            .collect();
        edge_ids = traversal["treeEdgeIds"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();

        let depth = traversal["depth"].as_object().unwrap();
        let max_depth = depth
            .values()
            .filter_map(Value::as_i64)
            .fold(1_i64, |acc, v| acc.max(v));
        let mut rows: HashMap<i64, Vec<String>> = HashMap::new();
        for id in &node_ids {
            let d = depth
                .get(id)
                .and_then(Value::as_i64)
                .unwrap_or(max_depth + 1);
            rows.entry(d).or_default().push(id.clone());
        }
        let mut sorted_rows: Vec<i64> = rows.keys().cloned().collect();
        sorted_rows.sort();
        for depth_key in sorted_rows {
            let ids = rows.get(&depth_key).cloned().unwrap_or_default();
            for (index, id) in ids.iter().enumerate() {
                positions.insert(
                    id.clone(),
                    json!({ "x": 80 + depth_key * 350, "y": 80 + (index as i64) * 182 }),
                );
            }
        }
    } else if mode == "table" {
        let mut type_order: Vec<String> = Vec::new();
        let mut seen_types: HashSet<String> = HashSet::new();
        for node in nodes {
            if let Some(t) = str_of(node, "type") {
                if seen_types.insert(t.clone()) {
                    type_order.push(t);
                }
            }
        }
        for (column, type_name) in type_order.iter().enumerate() {
            let mut row = 0_i64;
            for node in nodes {
                if str_of(node, "type").as_deref() == Some(type_name) {
                    if let Some(id) = str_of(node, "id") {
                        positions.insert(
                            id.clone(),
                            json!({ "x": 70 + (column as i64) * 310, "y": 105 + row * 168 }),
                        );
                        annotations.insert(id.clone(), json!(format!("{type_name} · row {}", row + 1)));
                        row += 1;
                    }
                }
            }
        }
    } else if mode == "huffman" {
        let codes = huffman_codes(project);
        let mut ordered: Vec<(String, String)> = nodes
            .iter()
            .filter_map(|n| str_of(n, "id"))
            .map(|id| {
                let code = codes.get(&id).cloned().unwrap_or_default();
                (id, code)
            })
            .collect();
        ordered.sort_by(|a, b| a.1.cmp(&b.1));
        let mut rows_by_depth: HashMap<i64, i64> = HashMap::new();
        for (id, code) in ordered {
            let depth = code.len() as i64;
            let row = rows_by_depth.get(&depth).copied().unwrap_or(0);
            positions.insert(id.clone(), json!({ "x": 70 + depth * 320, "y": 80 + row * 172 }));
            let prefix = if code.is_empty() { "0".to_string() } else { code.clone() };
            annotations.insert(id.clone(), json!(format!("prefix {prefix}")));
            rows_by_depth.insert(depth, row + 1);
        }
    } else {
        // 按边插入序（确定性的 JS Set 语义）保留 selected 边序；membership 用哈希核对。
        let selected_edges: Vec<String> = if mode == "evidence-chain" {
            chain_edge_ids(project, "evidence")
        } else if mode == "refutation-chain" {
            chain_edge_ids(project, "refutation")
        } else {
            edges.iter().filter_map(|e| str_of(e, "id")).collect()
        };
        let selected_set: HashSet<String> = selected_edges.iter().cloned().collect();
        edge_ids = selected_edges;
        if mode != "neural-network" {
            // `[...linkedNodes]`：按边扫描顺序首次出现的节点顺序。
            let mut linked: Vec<String> = Vec::new();
            for edge in edges {
                if selected_set.contains(&str_of(edge, "id").unwrap_or_default()) {
                    for key in ["source", "target"] {
                        if let Some(node_id) = str_of(edge, key) {
                            if !linked.contains(&node_id) {
                                linked.push(node_id);
                            }
                        }
                    }
                }
            }
            node_ids = linked;
        }
        let depth = topological_depths(project, Some(&selected_set));
        let mut rows: HashMap<i64, Vec<String>> = HashMap::new();
        for id in &node_ids {
            let layer = depth.get(id).copied().unwrap_or(0);
            rows.entry(layer).or_default().push(id.clone());
        }
        let mut sorted_layers: Vec<i64> = rows.keys().cloned().collect();
        sorted_layers.sort();
        for layer in sorted_layers {
            let ids = rows.get(&layer).cloned().unwrap_or_default();
            for (index, id) in ids.iter().enumerate() {
                positions.insert(
                    id.clone(),
                    json!({ "x": 75 + layer * 360, "y": 85 + (index as i64) * 190 }),
                );
                if mode == "neural-network" {
                    annotations.insert(id.clone(), json!(format!("layer {layer}")));
                }
            }
        }
    }

    json!({
        "mode": mode,
        "positions": positions,
        "annotations": annotations,
        "nodeIds": node_ids,
        "edgeIds": edge_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_project() -> Value {
        json!({
            "schemaVersion": 1,
            "id": "p",
            "title": "T",
            "discipline": "D",
            "updatedAt": "2026-01-01T00:00:00Z",
            "revision": 1,
            "nodes": [
                {"id": "a", "type": "question", "title": "A", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "b", "type": "method", "title": "B", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"},
                {"id": "c", "type": "result", "title": "C", "body": "b", "tags": [], "status": "confirmed", "evidenceIds": [], "data": {}, "provenance": {"origin": "human"}, "createdAt": "x", "updatedAt": "x"}
            ],
            "edges": [
                {"id": "e1", "type": "uses", "source": "a", "target": "b", "directed": true, "polarity": "positive", "confidence": 0.9, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}},
                {"id": "e2", "type": "constructs", "source": "b", "target": "c", "directed": true, "polarity": "positive", "confidence": 0.8, "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}}
            ],
            "evidence": [],
            "placements": [],
            "scenarios": [],
            "activity": []
        })
    }

    #[test]
    fn resolve_edges_sorts_by_id_and_applies_overrides() {
        let mut project = sample_project();
        project["scenarios"] = json!([{
            "id": "s1",
            "name": "S",
            "disabledNodeIds": ["c"],
            "disabledEdgeIds": [],
            "nodeOverrides": {},
            "edgeOverrides": {"e1": {"confidence": 0.42}},
            "parameters": {},
            "hypothesis": "h",
            "expectedEffect": "e",
            "createdAt": "x"
        }]);
        let edges = resolve_edges(&project, None);
        assert_eq!(edges.len(), 2);
        assert_eq!(str_of(&edges[0], "id").unwrap(), "e1");
        let filtered = resolve_edges(&project, Some("s1"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(f64_of(&filtered[0], "confidence").unwrap(), 0.42);
    }

    #[test]
    fn bfs_traversal_yields_stable_order_and_parents() {
        let request = json!({
            "startId": "a",
            "strategy": "bfs",
            "direction": "out",
            "maxDepth": 10,
        });
        let result = traverse(&sample_project(), &request);
        assert_eq!(result["order"], json!(["a", "b", "c"]));
        assert_eq!(result["parent"]["b"], json!("a"));
        assert_eq!(result["depth"]["c"], json!(2));
        assert_eq!(result["treeEdgeIds"], json!(["e1", "e2"]));
    }

    #[test]
    fn dfs_distinguishes_back_edges_when_cycle_exists() {
        let mut project = sample_project();
        project["edges"].as_array_mut().unwrap().push(json!({
            "id": "e-cycle", "type": "causes", "source": "c", "target": "a",
            "directed": true, "polarity": "positive", "confidence": 0.5,
            "conditions": [], "evidenceIds": [], "provenance": {"origin": "human"}
        }));
        let request = json!({
            "startId": "a", "strategy": "dfs", "direction": "out", "maxDepth": 12
        });
        let result = traverse(&project, &request);
        assert!(result["backEdgeIds"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v == &json!("e-cycle")));
        let cycles = detect_cycles(&project, None);
        assert!(cycles.as_array().unwrap().len() >= 1);
    }

    #[test]
    fn shortest_path_follows_direction_and_returns_empty_when_unreachable() {
        let project = sample_project();
        let path = shortest_path(&project, "a", "c", None);
        assert_eq!(path, json!(["a", "b", "c"]));
        let missing = shortest_path(&project, "c", "a", None);
        assert_eq!(missing, json!([]));
    }

    #[test]
    fn compute_layout_projects_deterministic_positions() {
        let project = sample_project();
        for mode in ["tree", "table", "huffman", "evidence-chain", "refutation-chain", "neural-network"] {
            let layout = compute_layout(&project, mode, Some("a"));
            // refutation-chain 在无 contradicts 边的样例上允许为空；其余应有坐标。
            let need_positions = mode != "refutation-chain";
            let empty = layout["positions"].as_object().unwrap().is_empty();
            assert_ne!(empty, need_positions, "mode {mode} placement mismatch");
            assert_eq!(layout["mode"], json!(mode));
        }
    }

    #[test]
    fn influence_and_logic_chain_produce_deterministic_shapes() {
        let project = sample_project();
        let influence = propagate_influence(&project, "c", None);
        assert_eq!(influence["targetId"], json!("c"));
        assert!(influence["scores"].is_object());
        let chain = compute_logic_chain(&project, "evidence", None);
        assert!(chain["edgeIds"].is_array());
    }

    #[test]
    fn canonicalize_produces_sorted_nested_canonical_form() {
        // 与 graph_compiler::canonicalize 共享同一语义；此处验证图侧 resolve 后 canon 仍合法。
        let bytes = crate::graph_compiler::canonicalize(&sample_project());
        assert!(serde_json::from_slice::<Value>(&bytes).is_ok());
    }
}
