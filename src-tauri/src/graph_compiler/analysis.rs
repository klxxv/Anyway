//! 图分析：环、矛盾链、逻辑链、场景比对 (§15.2 编译器独占清单)。
//!
//! 有向环检测、最小矛盾集（contradicts/refutes 边）、按类型与极性构建的
//! 逻辑链、消融场景可达性差异。纯函数，复用 `algorithms.rs` 的场景解析。

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use super::algorithms::{
    bfs_request, resolved_node_ids, resolve_edges_view, scenario, shortest_path, string_set,
    traverse_graph,
};

/// 一个有向环（节点序列 + 对应边序列）。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Cycle {
    pub node_ids: Vec<String>,
    pub edge_ids: Vec<String>,
}

/// 有向环检测（对齐 TS detectCycles）：三色 DFS，按去重节点集合判重。
pub fn detect_cycles(project: &Value, scenario_id: Option<&str>) -> Vec<Cycle> {
    let edges = resolve_edges_view(project, scenario_id);
    let directed: Vec<_> = edges.into_iter().filter(|edge| edge.directed).collect();
    let mut node_ids: Vec<String> = resolved_node_ids(project, scenario_id).into_iter().collect();
    node_ids.sort();
    find_cycles(&directed, &node_ids)
        .into_iter()
        .map(|(node_ids, edge_ids)| Cycle { node_ids, edge_ids })
        .collect()
}

/// 三色 DFS 核心：`edges` 为参与环检测的边（按 id 排序），`node_ids` 为遍历起点。
fn find_cycles(edges: &[super::algorithms::EdgeView], node_ids: &[String]) -> Vec<(Vec<String>, Vec<String>)> {
    let mut outgoing: HashMap<String, Vec<&super::algorithms::EdgeView>> = HashMap::new();
    for edge in edges {
        outgoing.entry(edge.source.clone()).or_default().push(edge);
    }
    let mut colors: HashMap<String, u8> = HashMap::new();
    let mut stack: Vec<String> = Vec::new();
    let mut cycles: Vec<(Vec<String>, Vec<String>)> = Vec::new();
    fn visit(
        node: &str,
        edges: &[super::algorithms::EdgeView],
        outgoing: &HashMap<String, Vec<&super::algorithms::EdgeView>>,
        colors: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
        cycles: &mut Vec<(Vec<String>, Vec<String>)>,
    ) {
        colors.insert(node.to_string(), 1);
        stack.push(node.to_string());
        let mut list = outgoing.get(node).cloned().unwrap_or_default();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        for edge in list {
            let next = edge.target.clone();
            match colors.get(&next).copied().unwrap_or(0) {
                0 => visit(&next, edges, outgoing, colors, stack, cycles),
                1 => {
                    let index = stack.iter().rposition(|item| item == &next).expect("on stack");
                    let mut node_cycle: Vec<String> = stack[index..].to_vec();
                    node_cycle.push(next.clone());
                    let edge_cycle: Vec<String> = node_cycle
                        .windows(2)
                        .filter_map(|pair| {
                            edges
                                .iter()
                                .find(|candidate| candidate.source == pair[0] && candidate.target == pair[1])
                                .map(|candidate| candidate.id.clone())
                        })
                        .collect();
                    let mut key: Vec<&str> = node_cycle.iter().map(String::as_str).collect();
                    key.sort();
                    key.dedup();
                    let key = key.join("|");
                    if !cycles.iter().any(|(nodes, _)| {
                        let mut other: Vec<&str> = nodes.iter().map(String::as_str).collect();
                        other.sort();
                        other.dedup();
                        other.join("|") == key
                    }) {
                        cycles.push((node_cycle, edge_cycle));
                    }
                }
                _ => {}
            }
        }
        stack.pop();
        colors.insert(node.to_string(), 2);
    }
    for node in node_ids {
        if colors.get(node).copied().unwrap_or(0) == 0 {
            visit(node, edges, &outgoing, &mut colors, &mut stack, &mut cycles);
        }
    }
    cycles
}

/// 一个矛盾环。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionCycle {
    pub node_ids: Vec<String>,
    pub edge_ids: Vec<String>,
}

/// 最小矛盾集：contradicts / 实验 refutes 边形成的最小矛盾环。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionChains {
    /// 全部矛盾环，按节点数升序、字典序稳定排序。
    pub cycles: Vec<ContradictionCycle>,
    /// 最小环的节点数。
    pub minimal_size: Option<usize>,
    /// 纳入考虑的矛盾边 id（排序）。
    pub considered_edge_ids: Vec<String>,
}

/// 检测 contradicts（或实验 refutes）有向边构成的最小矛盾环。
pub fn contradiction_chains(project: &Value, scenario_id: Option<&str>) -> ContradictionChains {
    let edges = resolve_edges_view(project, scenario_id);
    let contradiction_edges: Vec<_> = edges
        .into_iter()
        .filter(|edge| {
            edge.directed
                && (edge.edge_type == "contradicts" || edge.outcome.as_deref() == Some("refutes"))
        })
        .collect();
    let mut node_ids: Vec<String> = resolved_node_ids(project, scenario_id).into_iter().collect();
    node_ids.sort();
    let mut cycles: Vec<ContradictionCycle> = find_cycles(&contradiction_edges, &node_ids)
        .into_iter()
        .map(|(node_ids, edge_ids)| ContradictionCycle { node_ids, edge_ids })
        .collect();
    // 环的 node_ids 含闭环重复起点（与 TS detectCycles 一致）；"最小"按唯一节点数排序。
    let unique_count = |cycle: &ContradictionCycle| {
        let mut ids: Vec<&str> = cycle.node_ids.iter().map(String::as_str).collect();
        ids.sort();
        ids.dedup();
        ids.len()
    };
    cycles.sort_by(|a, b| {
        unique_count(a)
            .cmp(&unique_count(b))
            .then_with(|| a.node_ids.join("|").cmp(&b.node_ids.join("|")))
    });
    let minimal_size = cycles.first().map(&unique_count);
    let mut considered: Vec<String> = contradiction_edges.iter().map(|edge| edge.id.clone()).collect();
    considered.sort();
    ContradictionChains {
        cycles,
        minimal_size,
        considered_edge_ids: considered,
    }
}

/// 供研究者审阅的有分数逻辑链。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogicChainResult {
    pub mode: String,
    pub node_ids: Vec<String>,
    pub edge_ids: Vec<String>,
    pub score: f64,
    pub summary: String,
}

/// 按边类型与实验极性构建有效链并评分（对齐 TS computeLogicChain）。
pub fn compute_logic_chain(project: &Value, mode: &str, target_id: Option<&str>) -> LogicChainResult {
    let edges = resolve_edges_view(project, None);
    let completed: Vec<_> = edges
        .iter()
        .filter(|edge| edge.status.as_deref() == Some("completed"))
        .collect();
    let chosen: Vec<_> = match mode {
        "refutation" => edges
            .iter()
            .filter(|edge| edge.edge_type == "contradicts" || edge.outcome.as_deref() == Some("refutes"))
            .collect(),
        "effective" => completed
            .into_iter()
            .filter(|edge| {
                edge.outcome.as_deref() == Some("supports")
                    && edge.delta.map(|delta| delta.abs() >= 0.005).unwrap_or(false)
            })
            .collect(),
        _ => edges
            .iter()
            .filter(|edge| {
                edge.edge_type == "supports"
                    || edge.edge_type == "derived_from"
                    || edge.outcome.as_deref() == Some("supports")
            })
            .collect(),
    };
    let target_filtered: Vec<_> = match target_id {
        Some(target) => chosen
            .iter()
            .copied()
            .filter(|edge| {
                edge.target == target
                    || !shortest_path(project, &edge.target, target, None).is_empty()
            })
            .collect(),
        None => Vec::new(),
    };
    let selected = if target_filtered.is_empty() { chosen } else { target_filtered };
    let edge_ids: Vec<String> = selected.iter().map(|edge| edge.id.clone()).collect();
    let mut node_ids: Vec<String> = Vec::new();
    for edge in &selected {
        if !node_ids.contains(&edge.source) {
            node_ids.push(edge.source.clone());
        }
        if !node_ids.contains(&edge.target) {
            node_ids.push(edge.target.clone());
        }
    }
    let experiment_count = selected
        .iter()
        .filter_map(|edge| edge.experiment_id.as_ref())
        .collect::<HashSet<_>>()
        .len();
    let mean_confidence = selected.iter().map(|edge| edge.confidence).sum::<f64>() / (selected.len().max(1) as f64);
    let summary = match mode {
        "effective" => format!("{experiment_count} completed experiments changed the target metric by at least 0.5 percentage points."),
        "refutation" => format!(
            "{} experiments or sources challenge the current explanation.",
            if experiment_count > 0 { experiment_count } else { selected.len() }
        ),
        _ => format!("{} supported relations form the currently reviewable evidence chain.", selected.len()),
    };
    LogicChainResult {
        mode: mode.to_string(),
        node_ids,
        edge_ids,
        score: mean_confidence,
        summary,
    }
}

/// 基线与场景可达性差异。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDiff {
    pub disabled_node_ids: Vec<String>,
    pub disabled_edge_ids: Vec<String>,
    pub lost_reachable_node_ids: Vec<String>,
    pub retained_reachable_node_ids: Vec<String>,
    pub alternate_path_node_ids: Vec<String>,
}

/// 对比基础图与消融场景，区分直接禁用与意外失去可达性
/// （对齐 TS compareScenarioReachability）。
pub fn compare_scenario_reachability(project: &Value, root_id: &str, scenario_id: &str) -> ScenarioDiff {
    let base = traverse_graph(project, &bfs_request(root_id, None));
    let ablated = traverse_graph(project, &bfs_request(root_id, Some(scenario_id)));
    let scenario = scenario(project, Some(scenario_id));
    let disabled_nodes = string_set(scenario.and_then(|s| s.get("disabledNodeIds")));
    let disabled_edges = string_set(scenario.and_then(|s| s.get("disabledEdgeIds")));
    let base_set: HashSet<&str> = base.order.iter().map(String::as_str).collect();
    let ablated_set: HashSet<&str> = ablated.order.iter().map(String::as_str).collect();
    let lost: Vec<String> = base
        .order
        .iter()
        .filter(|id| !ablated_set.contains(id.as_str()) && !disabled_nodes.contains(*id))
        .cloned()
        .collect();
    let retained: Vec<String> = ablated.order.iter().filter(|id| base_set.contains(id.as_str())).cloned().collect();
    let alternate: Vec<String> = retained
        .iter()
        .filter(|id| base.parent.get(*id) != ablated.parent.get(*id))
        .cloned()
        .collect();
    let mut disabled_nodes: Vec<String> = disabled_nodes.into_iter().collect();
    disabled_nodes.sort();
    let mut disabled_edges: Vec<String> = disabled_edges.into_iter().collect();
    disabled_edges.sort();
    ScenarioDiff {
        disabled_node_ids: disabled_nodes,
        disabled_edge_ids: disabled_edges,
        lost_reachable_node_ids: lost,
        retained_reachable_node_ids: retained,
        alternate_path_node_ids: alternate,
    }
}
