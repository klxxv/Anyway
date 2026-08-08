//! 结构矛盾与最小见证 / Structural contradictions & minimal witnesses (spec GC-11)。
//!
//! 输出可复算的图结构矛盾：直接 contradicts（GC11-01）、正负双路径
//! （GC11-02/03）、奇数负边环（GC11-04/05）、自相矛盾 claim（GC11-10）、
//! AND 因子局部不可满足（GC11-11）。与形式化证明不可满足明确区分——
//! 这里是图结构层面的可复算见证。
//!
//! 确定性：见证按 (总边数, 种类, 字典序路径) 稳定排序；parity-BFS 保证
//! 最短路径；预算（maxDepth/maxWitnesses/maxWork）截断时置 `truncated`，
//! 绝不声称无矛盾（GC11-07/12）。

use crate::invariant::Severity;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};

/// 矛盾见证 / Contradiction witness (GC-11).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionWitness {
    /// 见证类型：direct-edge / path-pair / signed-cycle / self-contradiction /
    /// and-factor-inconsistent。
    pub kind: &'static str,
    /// 见证涉及的边路径（按稳定排序，可复算）。
    pub paths: Vec<Vec<String>>,
    /// 严重度（自相矛盾为 Error，其余 Warning）。
    pub severity: Severity,
}

/// 矛盾检查选项 / Contradiction options.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContradictionOptions {
    /// 路径深度上限（GC11-07：超限不得声称无矛盾）。
    pub max_depth: usize,
    /// 见证数量上限。
    pub max_witnesses: usize,
    /// 最低边置信度（GC11-09：低于阈值的边排除，原始结构仍可查询）。
    pub min_confidence: f64,
    /// parity-BFS 总状态展开预算（GC11-12）。
    pub max_work: usize,
}

impl Default for ContradictionOptions {
    fn default() -> Self {
        Self {
            max_depth: 8,
            max_witnesses: 64,
            min_confidence: 0.0,
            max_work: 100_000,
        }
    }
}

/// 矛盾检查结果：见证列表 + 是否截断（预算）。
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionReport {
    /// 全部见证（按稳定排序，最短在前）。
    pub witnesses: Vec<ContradictionWitness>,
    /// 是否因 maxDepth/预算截断（不得声称无矛盾）。
    pub truncated: bool,
}

/// 符号边 / Signed edge.
struct SignedEdge {
    id: String,
    source: String,
    target: String,
    sign: i8,
    /// 是否 contradicts 语义（负边）——直接见证源。
    contradicts: bool,
}

/// 从语义图构建符号图：supports/implies/depends_on/derived_from = +1；
/// contradicts = −1；polarity negative 强制 −1；experiment.outcome=refutes
/// 强制 −1；confidence 低于阈值（min_confidence）的边排除（GC11-09）。
fn build_signed_edges(project: &Value, min_confidence: f64) -> Vec<SignedEdge> {
    let mut edges = Vec::new();
    let Some(edge_array) = project.get("edges").and_then(Value::as_array) else {
        return edges;
    };
    for edge in edge_array {
        let Some(id) = edge.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(source) = edge.get("source").and_then(Value::as_str) else {
            continue;
        };
        let Some(target) = edge.get("target").and_then(Value::as_str) else {
            continue;
        };
        let confidence = edge
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.5);
        if confidence < min_confidence {
            continue;
        }
        let edge_type = edge.get("type").and_then(Value::as_str).unwrap_or("");
        let mut sign = match edge_type {
            "contradicts" => -1,
            "supports" | "implies" | "depends_on" | "derived_from" | "and" | "or" => 1,
            _ => 1,
        };
        if let Some(polarity) = edge.get("polarity").and_then(Value::as_str) {
            if polarity == "negative" {
                sign = -1;
            } else if polarity == "positive" {
                sign = 1;
            }
        }
        if let Some(outcome) = edge
            .get("experiment")
            .and_then(Value::as_object)
            .and_then(|exp| exp.get("outcome").and_then(Value::as_str))
        {
            if outcome == "refutes" {
                sign = -1;
            } else if outcome == "supports" {
                sign = 1;
            }
        }
        edges.push(SignedEdge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            sign,
            contradicts: edge_type == "contradicts",
        });
    }
    edges
}

/// 符号边索引：节点 → 出边列表。
type OutIndex = HashMap<String, Vec<usize>>;

fn build_out_index(edges: &[SignedEdge]) -> OutIndex {
    let mut index: OutIndex = HashMap::new();
    for (i, edge) in edges.iter().enumerate() {
        index.entry(edge.source.clone()).or_default().push(i);
    }
    for list in index.values_mut() {
        list.sort_unstable();
    }
    index
}

/// parity-BFS：状态 (node, parity) 中 parity ∈ {+1,−1} 为路径符号乘积。
/// 从 (start, start_parity) 出发，找 (goal, goal_parity) 的最短路径，
/// 返回边 id 序列。`work` 计入预算；深度/工作截断时置 `truncated`。
#[allow(clippy::too_many_arguments)]
fn parity_bfs(
    edges: &[SignedEdge],
    out: &OutIndex,
    start: &str,
    goal: &str,
    start_parity: i8,
    goal_parity: i8,
    max_depth: usize,
    work: &mut usize,
    truncated: &mut bool,
) -> Option<Vec<String>> {
    let mut queue: VecDeque<((String, i8), usize)> = VecDeque::new();
    let mut parent: HashMap<(String, i8), ((String, i8), usize)> = HashMap::new(); // (node,parity) → (prev, edge_idx)
    let mut depth: HashMap<(String, i8), usize> = HashMap::new();
    queue.push_back(((start.to_string(), start_parity), 0));
    depth.insert((start.to_string(), start_parity), 0);

    while let Some(((node, parity), d)) = queue.pop_front() {
        if d >= max_depth {
            *truncated = true;
            continue;
        }
        if *work > 0 {
            *work -= 1;
        } else {
            *truncated = true;
            break;
        }
        let Some(edge_indices) = out.get(&node) else {
            continue;
        };
        for &edge_idx in edge_indices {
            let edge = &edges[edge_idx];
            let next_parity = parity * edge.sign;
            let next = (edge.target.clone(), next_parity);
            if depth.contains_key(&next) {
                continue;
            }
            depth.insert(next.clone(), d + 1);
            parent.insert(next.clone(), ((node.clone(), parity), edge_idx));
            queue.push_back((next.clone(), d + 1));
            if next.0 == goal && next_parity == goal_parity {
                // 重建路径:parent 键是 (node, parity) 且 parity ∈ {+1,−1},
                // 游标必须携带真实 parity——写成 0 一跳就退出,见证被截断。
                let mut path = Vec::new();
                let mut cursor = next;
                while let Some(((prev, prev_parity), edge_idx)) = parent.get(&cursor) {
                    path.push(edges[*edge_idx].id.clone());
                    cursor = (prev.clone(), *prev_parity);
                }
                path.reverse();
                return Some(path);
            }
        }
    }
    None
}

/// 计算结构矛盾见证 / Find structural contradiction witnesses (GC-11)。
pub fn find_contradictions(project: &Value, options: &ContradictionOptions) -> ContradictionReport {
    let mut witnesses: Vec<ContradictionWitness> = Vec::new();
    let mut truncated = false;
    let mut work = options.max_work;

    let edges = build_signed_edges(project, options.min_confidence);
    let out = build_out_index(&edges);

    // 1) 直接 contradicts 边（GC11-01）与自相矛盾（GC11-10）。
    for edge in &edges {
        if !edge.contradicts {
            continue;
        }
        if edge.source == edge.target {
            witnesses.push(ContradictionWitness {
                kind: "self-contradiction",
                paths: vec![vec![edge.id.clone()]],
                severity: Severity::Error,
            });
            continue;
        }
        witnesses.push(ContradictionWitness {
            kind: "direct-edge",
            paths: vec![vec![edge.id.clone()]],
            severity: Severity::Warning,
        });
    }

    // 2) 正负双路径：contradicts 边 (a,b) 之外，a→b 存在正路径（GC11-02）。
    //    只有负路径无正路径 ⇒ 不构成双路径冲突（GC11-03）。
    let contradicts: Vec<&SignedEdge> = edges
        .iter()
        .filter(|edge| edge.contradicts && edge.source != edge.target)
        .collect();
    for edge in &contradicts {
        let path = parity_bfs(
            &edges,
            &out,
            &edge.source,
            &edge.target,
            1,
            1,
            options.max_depth,
            &mut work,
            &mut truncated,
        );
        let Some(path) = path else {
            continue;
        };
        // 正路径不得经过该 contradicts 边本身(自指见证无效)。
        // 注意:只看首边是数学错误——偶数条负边的乘积仍是 +1,
        // 被查边可以出现在路径中段;必须全路径排除。
        if !path.is_empty() && !path.iter().any(|id| id == &edge.id) {
            witnesses.push(ContradictionWitness {
                kind: "path-pair",
                paths: vec![path, vec![edge.id.clone()]],
                severity: Severity::Warning,
            });
        }
    }

    // 3) 奇数负边环（GC11-04）：对每条边 (u,v,sign)，若 v→u 存在符号乘积
    //    = −sign 的路径 ⇒ 环乘积 = sign·(−sign) = −1。
    for edge in &edges {
        let goal_parity = -edge.sign;
        let path = parity_bfs(
            &edges,
            &out,
            &edge.target,
            &edge.source,
            1,
            goal_parity,
            options.max_depth,
            &mut work,
            &mut truncated,
        );
        let Some(path) = path else {
            continue;
        };
        // 同上:被查边不得在路径任何位置重现(不只首边)。
        if !path.is_empty() && !path.iter().any(|id| id == &edge.id) {
            let mut cycle = vec![edge.id.clone()];
            cycle.extend(path);
            witnesses.push(ContradictionWitness {
                kind: "signed-cycle",
                paths: vec![cycle],
                severity: Severity::Warning,
            });
        }
    }

    // 4) AND 因子局部不可满足（GC11-11）：AND 因子的两个输入被 contradicts
    //    边直接连接 ⇒ 局部赋值冲突。
    if let Some(edge_array) = project.get("edges").and_then(Value::as_array) {
        let and_edges: Vec<&Value> = edge_array
            .iter()
            .filter(|edge| edge.get("type").and_then(Value::as_str) == Some("and"))
            .collect();
        for and_edge in &and_edges {
            let Some(target) = and_edge.get("target").and_then(Value::as_str) else {
                continue;
            };
            let inputs: Vec<String> = edge_array
                .iter()
                .filter(|edge| {
                    edge.get("type").and_then(Value::as_str) == Some("and")
                        && edge.get("target").and_then(Value::as_str) == Some(target)
                })
                .filter_map(|edge| {
                    edge.get("source")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .collect();
            for i in 0..inputs.len() {
                for j in (i + 1)..inputs.len() {
                    if let Some(contradict_edge) = edges.iter().find(|edge| {
                        edge.contradicts
                            && ((edge.source == inputs[i] && edge.target == inputs[j])
                                || (edge.source == inputs[j] && edge.target == inputs[i]))
                    }) {
                        let and_id = and_edge.get("id").and_then(Value::as_str).unwrap_or("?");
                        witnesses.push(ContradictionWitness {
                            kind: "and-factor-inconsistent",
                            paths: vec![vec![and_id.to_string()], vec![contradict_edge.id.clone()]],
                            severity: Severity::Warning,
                        });
                    }
                }
            }
        }
    }

    // 5) 稳定排序（GC11-06）：(总边数, 种类, 字典序)。
    witnesses.sort_by(|a, b| {
        let a_len: usize = a.paths.iter().map(Vec::len).sum();
        let b_len: usize = b.paths.iter().map(Vec::len).sum();
        a_len
            .cmp(&b_len)
            .then_with(|| a.kind.cmp(b.kind))
            .then_with(|| format!("{:?}", a.paths).cmp(&format!("{:?}", b.paths)))
    });
    if witnesses.len() > options.max_witnesses {
        witnesses.truncate(options.max_witnesses);
        truncated = true;
    }

    ContradictionReport {
        witnesses,
        truncated,
    }
}

// ---------------------------------------------------------------------------
// 单元测试 / Unit tests (GC-11)
// ---------------------------------------------------------------------------
