//! 语义图 → 因子图编译 / Semantic graph → factor graph (spec GC-08)。
//!
//! 类型边与复合逻辑节点编译为明确因子及变量域：supports → 二元证据因子；
//! contradicts → 翻转极性因子；implies → 只惩罚 A 真 B 假；AND/OR →
//! 真值表因子；depends_on → 门控因子（η=0 无影响）。统计证据经
//! `statistics::normalize_statistical_evidence` 标准化为有效消息 w=η·λ。
//!
//! 硬边界（spec §1.1）：编译器 MUST 拒绝 Agent 注入哈希、后验概率或布局
//! 坐标作为可信事实 —— 这些键若出现在节点/边 data 中，一律产出 Error 级
//! 诊断且绝不参与编译。连续 float 变量拒绝进入 BP（GC08-08）。

use super::statistics::{
    normalize_statistical_evidence, EdgeQuality, EvidenceDirection, StatisticalEvidence,
};
use super::{Factor, FactorDiagnostic, FactorGraph, FactorKind, FactorVariable};
use crate::invariant::Severity;
use serde_json::Value;

/// Agent 被禁止注入的可信事实键（spec §1.1：哈希/后验概率/布局坐标）。
/// 这些键若出现在 data 中，编译拒绝并忽略，绝不作为输入。
const INJECTED_TRUST_KEYS: &[&str] = &[
    "hash",
    "blockHash",
    "contentRootHash",
    "fileHash",
    "posterior",
    "posteriorProbability",
    "belief",
    "beliefState",
    "layout",
    "x",
    "y",
    "coordinates",
    "position",
];

/// 连续 float 变量域键（GC08-08：拒绝直接进入 BP，要求离散化适配器）。
const FLOAT_DOMAIN_KEYS: &[&str] = &["min", "max", "step"];

/// 主张类节点类型（paper/evidence/dataset 等非主张节点不成为信念变量）。
const CLAIM_NODE_TYPES: &[&str] = &[
    "claim",
    "hypothesis",
    "concept",
    "result",
    "metric",
    "variable",
    "question",
];

/// 由规范化项目编译因子图（确定性：变量按名排序、因子按边 id 排序）。
/// 诊断（含注入拒绝）写入 `FactorGraph::diagnostics`，不中断编译。
pub fn compile_factor_graph(project: &Value) -> FactorGraph {
    let mut graph = FactorGraph::default();

    let Some(root) = project.as_object() else {
        graph.diagnostics.push(FactorDiagnostic::new(
            "malformed-project",
            Severity::Error,
            "project",
            "project root must be a JSON object",
        ));
        return graph;
    };

    let nodes: Vec<&Value> = root
        .get("nodes")
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default();
    let edges: Vec<&Value> = root
        .get("edges")
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default();

    // ---- 变量域：主张类节点（claim/hypothesis/...）→ 布尔/枚举变量 ----
    let mut variable_names: Vec<String> = Vec::new();
    let mut variable_specs: std::collections::HashMap<String, FactorVariable> =
        std::collections::HashMap::new();
    for node in &nodes {
        let Some(id) = node.get("id").and_then(Value::as_str) else {
            continue;
        };
        let node_type = node.get("type").and_then(Value::as_str).unwrap_or("");
        if !CLAIM_NODE_TYPES.contains(&node_type) {
            continue; // paper/dataset/experiment 等不是主张变量。
        }
        // 无 data(或非对象)的主张节点按空 data 处理:默认布尔变量。
        // 之前直接 continue,导致这类 claim 从 BP 静默消失。
        let empty_data = serde_json::Map::new();
        let data = node
            .get("data")
            .and_then(Value::as_object)
            .unwrap_or(&empty_data);
        // 注入检测：data 中不得携带哈希/后验/布局（MUST 拒绝）。
        for key in INJECTED_TRUST_KEYS {
            if data.contains_key(*key) {
                graph.diagnostics.push(FactorDiagnostic::new(
                    "agent-injected-trust-fact",
                    Severity::Error,
                    &format!("node:{id}"),
                    format!("node data contains injected trust fact key {key:?}; hash/posterior/layout MUST NOT be trusted from agents"),
                ));
            }
        }
        let value_type = data.get("valueType").and_then(Value::as_str).unwrap_or("");
        let mut variable = FactorVariable::boolean(id);
        match value_type {
            "enum" => {
                let mut domain: Vec<String> = data
                    .get("enumValues")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                if domain.is_empty() {
                    graph.diagnostics.push(FactorDiagnostic::new(
                        "invalid-variable-domain",
                        Severity::Error,
                        &format!("node:{id}"),
                        "enum domain must have at least 2 values (GC04-08)",
                    ));
                    domain = vec!["true".to_string(), "false".to_string()];
                }
                variable.domain = domain;
            }
            "float" | "number" | "continuous" => {
                // GC08-08：连续 float 变量拒绝直接进入 BP，除非提供离散化域。
                if data
                    .keys()
                    .any(|key| FLOAT_DOMAIN_KEYS.contains(&key.as_str()))
                    || data.get("discretize").is_some()
                {
                    let domain: Vec<String> = data
                        .get("discretize")
                        .and_then(Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default();
                    if domain.is_empty() {
                        graph.diagnostics.push(FactorDiagnostic::new(
                            "continuous-variable-needs-discretization",
                            Severity::Error,
                            &format!("node:{id}"),
                            "continuous float variable MUST NOT enter BP directly; provide a discretize adapter (GC08-08)",
                        ));
                    }
                    variable.domain = domain;
                } else {
                    graph.diagnostics.push(FactorDiagnostic::new(
                        "continuous-variable-needs-discretization",
                        Severity::Error,
                        &format!("node:{id}"),
                        "continuous float variable MUST NOT enter BP directly; provide a discretize adapter (GC08-08)",
                    ));
                }
            }
            _ => {
                // 默认布尔主张变量（claim/hypothesis/concept/result/metric）。
            }
        }
        // 变量自身统计证据 → 先验 logit（原始观测，与边证据同待遇；
        // 后验概率/哈希/布局仍被拒绝注入）。
        apply_node_prior(&mut variable, node, id, &mut graph);
        variable.grounded = node_grounded(node)
            || variable.prior_support_logit != 0.0
            || variable.prior_refutation_logit != 0.0;
        variable_specs.insert(id.to_string(), variable);
    }
    let mut names: Vec<String> = variable_specs.keys().cloned().collect();
    names.sort();
    for name in names {
        variable_names.push(name.clone());
        graph.variables.push(variable_specs.remove(&name).unwrap());
    }

    // ---- 因子：按语义边 id 稳定排序 ----
    let mut edge_refs: Vec<(&Value, String)> = edges
        .iter()
        .filter_map(|edge| {
            edge.get("id")
                .and_then(Value::as_str)
                .map(|id| (*edge, id.to_string()))
        })
        .collect();
    edge_refs.sort_by(|a, b| a.1.cmp(&b.1));

    // 证据去重（GC08-09）：同一语义边 + 同一证据 → 只编译一次。
    let mut compiled_edge_evidence: std::collections::HashSet<(String, String)> =
        std::collections::HashSet::new();

    for (edge, edge_id) in &edge_refs {
        let location = format!("edge:{edge_id}");
        let Some(source) = edge.get("source").and_then(Value::as_str) else {
            continue;
        };
        let Some(target) = edge.get("target").and_then(Value::as_str) else {
            continue;
        };
        if !variable_specs_contains(&variable_names, source)
            || !variable_specs_contains(&variable_names, target)
        {
            continue; // 端点不是主张变量（如 paper/metric 节点），跳过因子。
        }
        let edge_type = edge.get("type").and_then(Value::as_str).unwrap_or("");
        // 未知边类型不生成因子(自有文档如此声明,此前却静默编译为 Supports,
        // 会给 causes/measures 等结构边注入正向证据)。显式跳过并留诊断。
        let Some(factor_kind) = kind_for_type(edge_type) else {
            graph.diagnostics.push(FactorDiagnostic::new(
                "unsupported-edge-factor",
                Severity::Warning,
                &location,
                format!("edge type {edge_type:?} has no factor semantics; skipped instead of defaulting to supports"),
            ));
            continue;
        };
        let evidence_ids: Vec<&str> = edge
            .get("evidenceIds")
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        if !evidence_ids.is_empty() {
            for evidence in &evidence_ids {
                if !compiled_edge_evidence.insert((edge_id.clone(), (*evidence).to_string())) {
                    graph.diagnostics.push(FactorDiagnostic::new(
                        "duplicate-edge-evidence",
                        Severity::Warning,
                        &location,
                        format!("same evidence {evidence:?} compiled twice on edge {edge_id:?}; deduplicated (GC08-09)"),
                    ));
                }
            }
        }

        // 注入检测（边 data 同样不得携带可信事实）。
        if let Some(data) = edge.get("data").and_then(Value::as_object) {
            for key in INJECTED_TRUST_KEYS {
                if data.contains_key(*key) {
                    graph.diagnostics.push(FactorDiagnostic::new(
                        "agent-injected-trust-fact",
                        Severity::Error,
                        &location,
                        format!("edge data contains injected trust fact key {key:?}; hash/posterior/layout MUST NOT be trusted from agents"),
                    ));
                }
            }
        }

        // 统计证据标准化 → 有效消息 w = η·λ。
        let quality = read_quality(edge);
        if let Err(message) = quality.validate() {
            graph.diagnostics.push(FactorDiagnostic::new(
                "invalid-quality-score",
                Severity::Error,
                &location,
                message,
            ));
        }
        let evidence = read_statistical_evidence(edge, edge_type);
        let metric = match normalize_statistical_evidence(&quality, evidence) {
            Ok(metric) => metric,
            Err(message) => {
                graph.diagnostics.push(FactorDiagnostic::new(
                    "invalid-statistical-evidence",
                    Severity::Error,
                    &location,
                    message,
                ));
                // 证据被拒绝 → 退化为未接地逻辑因子。
                let mut factor = Factor::logical(
                    factor_kind,
                    vec![source.to_string(), target.to_string()],
                    Some(edge_id.clone()),
                );
                factor.efficacy = quality.design
                    * quality.source
                    * quality.condition_match
                    * quality.independence
                    * quality.reproducibility;
                graph.factors.push(factor);
                continue;
            }
        };
        for warning in &metric.warnings {
            graph.diagnostics.push(FactorDiagnostic::new(
                warning,
                Severity::Warning,
                &location,
                warning.clone(),
            ));
        }

        let kind = factor_kind;
        let mut factor = Factor {
            kind,
            variables: vec![source.to_string(), target.to_string()],
            effective_message: metric.effective_message,
            efficacy: metric.efficacy,
            grounded: metric.local_log_evidence.is_some(),
            source_edge: Some(edge_id.clone()),
            local_log_evidence: metric.local_log_evidence,
            calibration: metric.calibration,
        };

        // 自环 supports（GC04-11）：不进 BP 重复放大。
        if source == target {
            graph.diagnostics.push(FactorDiagnostic::new(
                "redundant-self-support",
                Severity::Warning,
                &location,
                "self-loop supports edge MUST NOT amplify itself in BP",
            ));
            factor.effective_message = None;
        }
        graph.factors.push(factor);
    }

    // ---- 孤立变量诊断（GC08-11）：无任何因子连接 → 保留先验状态。----
    for (index, variable) in graph.variables.iter().enumerate() {
        if graph.is_isolated(index) {
            graph.diagnostics.push(FactorDiagnostic::new(
                "no-evidence",
                Severity::Warning,
                &format!("node:{}", variable.name),
                format!(
                    "variable {} has no factor; belief stays at prior (GC08-11)",
                    variable.name
                ),
            ));
        }
    }

    graph
}

/// 变量名集合查询辅助。
fn variable_specs_contains(names: &[String], name: &str) -> bool {
    names.binary_search(&name.to_string()).is_ok()
}

/// 节点是否接地（有证据引用或已审状态）。
fn node_grounded(node: &Value) -> bool {
    if let Some(ids) = node.get("evidenceIds").and_then(Value::as_array) {
        if !ids.is_empty() {
            return true;
        }
    }
    matches!(
        node.get("status").and_then(Value::as_str),
        Some("confirmed" | "verified")
    )
}

/// 边类型 → 因子种类（未知类型不生成因子）。
fn kind_for_type(edge_type: &str) -> Option<FactorKind> {
    Some(match edge_type {
        "supports" => FactorKind::Supports,
        "contradicts" => FactorKind::Contradicts,
        "implies" => FactorKind::Implies,
        "depends_on" => FactorKind::DependsOn,
        "statistical_test" => FactorKind::StatisticalTest,
        "meta_evidence" => FactorKind::MetaEvidence,
        "interaction" => FactorKind::Interaction,
        "and" => FactorKind::And,
        "or" => FactorKind::Or,
        "equivalent" => FactorKind::Equivalent,
        _ => return None,
    })
}

/// 从边 data 读取质量五元组（缺省全 1）。
fn read_quality(edge: &Value) -> EdgeQuality {
    let mut quality = EdgeQuality::default();
    let data = edge.get("data").and_then(Value::as_object);
    let container = data
        .and_then(|d| d.get("quality").and_then(Value::as_object))
        .or(data);
    if let Some(container) = container {
        let get = |key: &str| container.get(key).and_then(Value::as_f64);
        if let Some(value) = get("design") {
            quality.design = value;
        }
        if let Some(value) = get("source") {
            quality.source = value;
        }
        if let Some(value) = get("conditionMatch") {
            quality.condition_match = value;
        }
        if let Some(value) = get("independence") {
            quality.independence = value;
        }
        if let Some(value) = get("reproducibility") {
            quality.reproducibility = value;
        }
    }
    quality
}

/// 节点自身统计证据 → 变量先验 logit（原始观测；GC10-02/05 需要源变量带证据）。
/// λ≥0 进支持通道，λ<0 进反驳通道；非法统计量产出 Error 诊断且忽略。
fn apply_node_prior(
    variable: &mut FactorVariable,
    node: &Value,
    id: &str,
    graph: &mut FactorGraph,
) {
    let Some(evidence) = read_node_statistical_evidence(node) else {
        return;
    };
    match super::statistics::log_likelihood_ratio(&evidence) {
        Ok(lambda) => {
            if lambda >= 0.0 {
                variable.prior_support_logit += lambda;
            } else {
                variable.prior_refutation_logit += -lambda;
            }
        }
        Err(message) => graph.diagnostics.push(FactorDiagnostic::new(
            "invalid-statistical-evidence",
            Severity::Error,
            &format!("node:{id}"),
            message,
        )),
    }
}

/// 从节点 data 读取统计证据（节点自身证据，方向默认 supports）。
fn read_node_statistical_evidence(node: &Value) -> Option<StatisticalEvidence> {
    let data = node.get("data").and_then(Value::as_object)?;
    if let Some(p) = data.get("pValue").and_then(Value::as_f64) {
        return Some(StatisticalEvidence::PValue {
            p,
            direction: EvidenceDirection::Supports,
        });
    }
    if let Some(factor) = data.get("bayesFactor").and_then(Value::as_f64) {
        return Some(StatisticalEvidence::BayesFactor { factor });
    }
    if let (Some(effect), Some(standard_error)) = (
        data.get("effect").and_then(Value::as_f64),
        data.get("standardError").and_then(Value::as_f64),
    ) {
        return Some(StatisticalEvidence::NormalEffect {
            effect,
            standard_error,
            direction: EvidenceDirection::Supports,
        });
    }
    None
}

/// 从边 data 读取统计证据（pValue / bayesFactor / effect+standardError /
/// confidenceInterval），并结合实验 outcome 决定方向。
fn read_statistical_evidence(edge: &Value, edge_type: &str) -> Option<StatisticalEvidence> {
    let data = edge.get("data").and_then(Value::as_object)?;
    let default_direction = if edge_type == "contradicts" {
        EvidenceDirection::Refutes
    } else {
        EvidenceDirection::Supports
    };
    let direction = read_direction(edge, default_direction);

    if let Some(p) = data.get("pValue").and_then(Value::as_f64) {
        return Some(StatisticalEvidence::PValue { p, direction });
    }
    if let Some(factor) = data.get("bayesFactor").and_then(Value::as_f64) {
        // BF 同样受方向控制：refutes 取倒数（λ 变负），neutral 无信息。
        let factor = match direction {
            EvidenceDirection::Refutes => 1.0 / factor,
            EvidenceDirection::Neutral => 1.0,
            EvidenceDirection::Supports => factor,
        };
        return Some(StatisticalEvidence::BayesFactor { factor });
    }
    if let (Some(effect), Some(standard_error)) = (
        data.get("effect").and_then(Value::as_f64),
        data.get("standardError").and_then(Value::as_f64),
    ) {
        return Some(StatisticalEvidence::NormalEffect {
            effect,
            standard_error,
            direction,
        });
    }
    if let Some(interval) = data.get("confidenceInterval").and_then(Value::as_array) {
        if interval.len() == 2 {
            if let (Some(lower), Some(upper)) = (interval[0].as_f64(), interval[1].as_f64()) {
                return Some(StatisticalEvidence::ConfidenceInterval {
                    lower,
                    upper,
                    direction,
                });
            }
        }
    }
    None
}

/// 读取证据方向：data.direction 优先，其次 experiment.outcome。
fn read_direction(edge: &Value, default: EvidenceDirection) -> EvidenceDirection {
    if let Some(data) = edge.get("data").and_then(Value::as_object) {
        match data.get("direction").and_then(Value::as_str) {
            Some("refutes") => return EvidenceDirection::Refutes,
            Some("neutral") => return EvidenceDirection::Neutral,
            Some("supports") => return EvidenceDirection::Supports,
            _ => {}
        }
    }
    if let Some(experiment) = edge.get("experiment").and_then(Value::as_object) {
        match experiment.get("outcome").and_then(Value::as_str) {
            Some("refutes") => return EvidenceDirection::Refutes,
            Some("neutral") => return EvidenceDirection::Neutral,
            Some("supports") => return EvidenceDirection::Supports,
            _ => {}
        }
    }
    default
}

// ---------------------------------------------------------------------------
// 单元测试 / Unit tests (GC-08 + 注入拒绝)
// ---------------------------------------------------------------------------
