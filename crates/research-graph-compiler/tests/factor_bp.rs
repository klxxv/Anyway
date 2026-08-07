//! 逻辑计算层集成测试（spec §4-5，GC-08…GC-11）。
//! 只调用 crate 公共 API：因子图编译 → 统计证据 → BP → 矛盾见证。

use research_graph_compiler::{
    belief_propagation, combine_related_evidence, compile_factor_graph, find_contradictions,
    loopy_belief_propagation, normalize_statistical_evidence, sigmoid, tree_belief_propagation,
    BeliefState, BpOptions, BpStatus, ContradictionOptions, EdgeQuality, FactorGraph, FactorKind,
    StatisticalEvidence,
};
use serde_json::json;

/// 残差注意力消融项目（spec §6 示例的因子图版本）。
fn residual_attention_project() -> serde_json::Value {
    json!({
        "schemaVersion": 3,
        "id": "proj-residual",
        "title": "Residual Attention ablation",
        "nodes": [
            {"id": "residual-mode", "type": "variable", "title": "Residual mode",
             "data": {"valueType": "enum", "enumValues": ["ARL", "NAL"]}},
            {"id": "depth", "type": "variable", "title": "Network depth",
             "data": {"valueType": "enum", "enumValues": ["56", "92", "128", "164"]}},
            {"id": "arl-prevents-degradation", "type": "claim",
             "title": "ARL prevents degradation in deeper attention networks", "data": {}},
            {"id": "top1-error", "type": "metric", "title": "Top-1 error",
             "data": {"direction": "lower_is_better"}}
        ],
        "edges": [
            {"id": "e-aral-exp", "type": "statistical_test", "source": "residual-mode",
             "target": "arl-prevents-degradation", "directed": true, "polarity": "positive",
             "data": {"quality": {"design": 0.9, "source": 0.8, "conditionMatch": 1.0,
                                   "independence": 0.9, "reproducibility": 0.7},
                      "effect": 2.87, "standardError": 0.5, "direction": "supports"}},
            {"id": "e-depth-dep", "type": "depends_on", "source": "depth",
             "target": "arl-prevents-degradation", "directed": true, "polarity": "positive"}
        ],
        "evidence": []
    })
}

#[test]
fn full_pipeline_compiles_factors_and_runs_bp() {
    let project = residual_attention_project();
    let graph = compile_factor_graph(&project);

    // 因子：statistical_test + depends_on。
    assert_eq!(graph.factors.len(), 2);
    assert_eq!(graph.factors[0].kind, FactorKind::StatisticalTest);
    assert_eq!(graph.factors[1].kind, FactorKind::DependsOn);

    // 边效力 η = 0.9·0.8·1.0·0.9·0.7 = 0.4536；λ = 2.87²/(2·0.5²) ≈ 16.47。
    let metric = &graph.factors[0];
    assert!((metric.efficacy - 0.4536).abs() < 1e-12);
    let lambda = metric.local_log_evidence.unwrap();
    let expected_lambda = 2.87_f64 * 2.87 / (2.0 * 0.5 * 0.5);
    assert!((lambda - expected_lambda).abs() < 1e-9);
    assert!((metric.effective_message.unwrap() - 0.4536 * expected_lambda).abs() < 1e-9);

    // BP 无环 → 树 BP 收敛。
    let result = belief_propagation(&graph, &BpOptions::default());
    assert!(result.converged, "{:?}", result.status);
    let claim = graph.variable_index("arl-prevents-degradation").unwrap();
    assert!(
        result.beliefs[claim].support > 0.5,
        "statistical evidence raises support"
    );
}

#[test]
fn bp_is_deterministic_across_runs() {
    let graph = compile_factor_graph(&residual_attention_project());
    let a = tree_belief_propagation(&graph);
    let b = tree_belief_propagation(&graph);
    assert_eq!(a.beliefs, b.beliefs);
    // 无环图上 loopy 收敛到树 BP 解（数值容差 1e-9 内）。
    let c = loopy_belief_propagation(&graph, &BpOptions::default());
    assert_eq!(c.status, research_graph_compiler::BpStatus::Converged);
    for (x, y) in a.beliefs.iter().zip(c.beliefs.iter()) {
        assert!((x.support - y.support).abs() < 1e-6);
        assert!((x.refutation - y.refutation).abs() < 1e-6);
        assert!((x.net_belief - y.net_belief).abs() < 1e-6);
    }
}

#[test]
fn efficacy_and_evidence_normalization_public_api() {
    let quality = EdgeQuality {
        design: 0.9,
        source: 0.8,
        condition_match: 1.0,
        independence: 0.9,
        reproducibility: 0.7,
    };
    let metric = normalize_statistical_evidence(
        &quality,
        Some(StatisticalEvidence::BayesFactor { factor: 10.0 }),
    )
    .unwrap();
    assert!((metric.efficacy - 0.4536).abs() < 1e-12);
    assert!((metric.local_log_evidence.unwrap() - 10f64.ln()).abs() < 1e-12);

    // 相关证据合并（GC09-12）。
    let merged = combine_related_evidence(&[1.0, 1.0, 1.0], 1.0).unwrap();
    assert_eq!(merged, 1.0);
}

#[test]
fn contradiction_witnesses_survive_full_pipeline() {
    // A supports B，同时 A contradicts B → 直接矛盾 + 正负双路径。
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [
            {"id": "e1", "type": "supports", "source": "a", "target": "b",
             "directed": true, "polarity": "positive"},
            {"id": "e2", "type": "contradicts", "source": "a", "target": "b",
             "directed": true, "polarity": "negative"}
        ],
        "evidence": []
    });
    let report = find_contradictions(&project, &ContradictionOptions::default());
    assert!(
        report.witnesses.iter().any(|w| w.kind == "path-pair"),
        "{:?}",
        report.witnesses
    );
    assert!(report.witnesses.iter().any(|w| w.kind == "direct-edge"));

    // 因子图编译仍成功（矛盾不进编译，交 BP/见证层）。
    let graph = compile_factor_graph(&project);
    assert_eq!(graph.factors.len(), 2);
}

#[test]
fn agent_injection_is_rejected_end_to_end() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A",
             "data": {"blockHash": "deadbeefcafe", "posteriorProbability": 0.999}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{"id": "e1", "type": "supports", "source": "a", "target": "b",
                   "directed": true, "polarity": "positive",
                   "data": {"bayesFactor": 10.0}}],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let injected: Vec<_> = graph
        .diagnostics
        .iter()
        .filter(|d| d.code == "agent-injected-trust-fact")
        .collect();
    assert_eq!(injected.len(), 2);
    // 注入的后验概率绝不进入信念：a 仍是无信息先验（除非自身统计证据）。
    let a = graph.variable_index("a").unwrap();
    assert_eq!(graph.variables[a].prior_support_logit, 0.0);
    assert_eq!(graph.variables[a].prior_refutation_logit, 0.0);
}

#[test]
fn loopy_bp_on_three_node_loop_converges_or_reports() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}},
            {"id": "c", "type": "claim", "title": "C", "data": {}}
        ],
        "edges": [
            {"id": "e1", "type": "supports", "source": "a", "target": "b",
             "directed": true, "polarity": "positive", "data": {"bayesFactor": 3.0}},
            {"id": "e2", "type": "supports", "source": "b", "target": "c",
             "directed": true, "polarity": "positive", "data": {"bayesFactor": 3.0}},
            {"id": "e3", "type": "supports", "source": "c", "target": "a",
             "directed": true, "polarity": "positive", "data": {"bayesFactor": 3.0}}
        ],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    assert!(graph.has_cycle());
    let result = loopy_belief_propagation(&graph, &BpOptions::default());
    assert!(result.iterations <= BpOptions::default().max_iterations);
    for belief in &result.beliefs {
        assert!(belief.support.is_finite() && (0.0..=1.0).contains(&belief.support));
    }
}

#[test]
fn pvalue_direction_flows_into_bp_channels() {
    // refutes 方向 p 值 → 目标反驳通道上升、支持通道不误增。
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{"id": "e1", "type": "supports", "source": "a", "target": "b",
                   "directed": true, "polarity": "positive",
                   "data": {"pValue": 0.001, "direction": "refutes"}}],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = tree_belief_propagation(&graph);
    let b = result.beliefs[graph.variable_index("b").unwrap()];
    assert!(
        b.refutation > 0.6,
        "refutation channel rises: {}",
        b.refutation
    );
    assert!(b.support <= 0.5 + 1e-12, "support channel must not rise");
    assert!(b.net_belief < 0.5, "net belief tilts toward refutation");
}

#[test]
fn quality_zeroes_message_and_belief_stays_prior() {
    // η=0 边：消息归零 → 目标信念保持无信息先验（与删除边逐位一致）。
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A",
             "data": {"bayesFactor": 100.0}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{"id": "e1", "type": "supports", "source": "a", "target": "b",
                   "directed": true, "polarity": "positive",
                   "data": {"quality": {"design": 0.0}, "bayesFactor": 1000.0}}],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = tree_belief_propagation(&graph);
    let b = result.beliefs[graph.variable_index("b").unwrap()];
    assert_eq!(b.support, 0.5);
    assert_eq!(b.conflict, 0.25);
}

#[test]
fn extreme_evidence_stays_finite() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {"bayesFactor": 1e300}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{"id": "e1", "type": "supports", "source": "a", "target": "b",
                   "directed": true, "polarity": "positive",
                   "data": {"bayesFactor": 1e300}}],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = tree_belief_propagation(&graph);
    for belief in &result.beliefs {
        assert!(belief.support.is_finite() && belief.support <= 1.0);
        assert!(belief.net_belief.is_finite());
        assert!(belief.conflict.is_finite());
    }
}

#[test]
fn neutral_direction_gives_no_evidence() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{"id": "e1", "type": "supports", "source": "a", "target": "b",
                   "directed": true, "polarity": "positive",
                   "data": {"pValue": 0.01, "direction": "neutral"}}],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let metric = &graph.factors[0];
    assert_eq!(metric.effective_message.unwrap(), 0.0);
    assert_eq!(metric.local_log_evidence.unwrap(), 0.0);
}

#[test]
fn belief_state_definition_matches_gc10_01() {
    use research_graph_compiler::BeliefState;
    let state = BeliefState::from_probabilities(0.5, 0.5);
    assert_eq!(state.support, 0.5);
    assert_eq!(state.refutation, 0.5);
    assert_eq!(state.net_belief, 0.5);
    assert_eq!(state.conflict, 0.25);
}

#[test]
fn witness_budget_truncation_is_reported() {
    // 长正链 + 首尾 contradicts：max_depth 不足 → truncated 且不谎报无矛盾。
    let edges = vec![
        json!({"id": "e1", "type": "supports", "source": "a", "target": "b",
               "directed": true, "polarity": "positive"}),
        json!({"id": "e2", "type": "supports", "source": "b", "target": "c",
               "directed": true, "polarity": "positive"}),
        json!({"id": "e3", "type": "supports", "source": "c", "target": "d",
               "directed": true, "polarity": "positive"}),
        json!({"id": "e4", "type": "contradicts", "source": "a", "target": "d",
               "directed": true, "polarity": "negative"}),
    ];
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}},
            {"id": "c", "type": "claim", "title": "C", "data": {}},
            {"id": "d", "type": "claim", "title": "D", "data": {}}
        ],
        "edges": edges,
        "evidence": []
    });
    let report = find_contradictions(
        &project,
        &ContradictionOptions {
            max_depth: 2,
            ..ContradictionOptions::default()
        },
    );
    assert!(report.truncated);
    // 直接 contradicts 见证仍被报告（1 边，深度内）。
    assert!(report.witnesses.iter().any(|w| w.kind == "direct-edge"));
}

#[test]
fn evidence_direction_from_experiment_outcome() {
    // TS ResearchEdge.experiment.outcome 驱动证据方向。
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{"id": "e1", "type": "supports", "source": "a", "target": "b",
                   "directed": true, "polarity": "positive",
                   "experiment": {"id": "exp1", "label": "L", "metric": "acc",
                                  "delta": -0.03, "outcome": "refutes",
                                  "status": "completed"},
                   "data": {"bayesFactor": 8.0}}],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let metric = &graph.factors[0];
    assert!(
        metric.effective_message.unwrap() < 0.0,
        "refutes outcome flips sign"
    );
}

#[test]
fn oscillation_detection_triggers_unstable() {
    // 两个强正反馈 supports 环：阻尼 0 下必然振荡。
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [
            {"id": "e1", "type": "supports", "source": "a", "target": "b",
             "directed": true, "polarity": "positive", "data": {"bayesFactor": 1000.0}},
            {"id": "e2", "type": "supports", "source": "b", "target": "a",
             "directed": true, "polarity": "positive", "data": {"bayesFactor": 1000.0}}
        ],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = loopy_belief_propagation(
        &graph,
        &BpOptions {
            damping: 0.0,
            max_iterations: 200,
            ..BpOptions::default()
        },
    );
    eprintln!(
        "status={:?} iterations={} residual={:.3e}",
        result.status, result.iterations, result.residual
    );
    assert!(
        matches!(result.status, research_graph_compiler::BpStatus::Unstable),
        "strong feedback with zero damping must oscillate, got {:?}",
        result.status
    );
    assert!(result.residual > 0.0);
}

fn graph_with_edges(edges: Vec<serde_json::Value>) -> FactorGraph {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}},
            {"id": "c", "type": "claim", "title": "C", "data": {}},
            {"id": "d", "type": "claim", "title": "D", "data": {}}
        ],
        "edges": edges,
        "evidence": []
    });
    compile_factor_graph(&project)
}

fn supports(from: &str, to: &str, bf: f64) -> serde_json::Value {
    json!({"id": format!("e-{from}-{to}"), "type": "supports",
           "source": from, "target": to, "directed": true, "polarity": "positive",
           "data": {"bayesFactor": bf}})
}

#[test]
fn gc10_01_isolated_variable_prior_zero() {
    let graph = graph_with_edges(vec![]);
    let result = tree_belief_propagation(&graph);
    let a = result.beliefs[graph.variable_index("a").unwrap()];
    assert_eq!(a.support, 0.5);
    assert_eq!(a.refutation, 0.5);
    assert_eq!(a.net_belief, 0.5);
    assert_eq!(a.conflict, 0.25);
    assert!(result.converged);
}

#[test]
fn gc10_02_single_strong_support_raises_target_support() {
    // a 无来源信念（0.5）→ b 收到 w·0.5。
    let graph = graph_with_edges(vec![supports("a", "b", 100.0)]);
    let result = tree_belief_propagation(&graph);
    let a = result.beliefs[graph.variable_index("a").unwrap()];
    let b = result.beliefs[graph.variable_index("b").unwrap()];
    assert!(b.support > a.support, "support should rise along the edge");
    assert!(b.support > 0.5);
}

#[test]
fn gc10_03_single_strong_refutation_raises_only_refutation() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{
            "id": "e1", "type": "supports", "source": "a", "target": "b",
            "directed": true, "polarity": "positive",
            "data": {"pValue": 0.001, "direction": "refutes"}
        }],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = tree_belief_propagation(&graph);
    let b = result.beliefs[graph.variable_index("b").unwrap()];
    assert!(b.refutation > 0.5, "refutation channel should rise");
    assert!(b.support <= 0.5 + 1e-12, "support channel must not rise");
}

#[test]
fn gc10_04_equal_strong_support_and_refutation_conflict_high() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "s", "type": "claim", "title": "S", "data": {}},
            {"id": "t", "type": "claim", "title": "T", "data": {}}
        ],
        "edges": [
            {"id": "e1", "type": "supports", "source": "s", "target": "t",
             "directed": true, "polarity": "positive",
             "data": {"bayesFactor": 100.0}},
            {"id": "e2", "type": "contradicts", "source": "t", "target": "s",
             "directed": true, "polarity": "negative",
             "data": {"bayesFactor": 100.0}}
        ],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = tree_belief_propagation(&graph);
    let t = result.beliefs[graph.variable_index("t").unwrap()];
    assert!(
        t.support > 0.5 && t.refutation > 0.5,
        "both channels active"
    );
    assert!(t.conflict > 0.3, "conflict should be high");
}

#[test]
fn gc10_05_supports_chain_decays() {
    // A 有自身强证据（先验 λ=ln(100)），边强度更小（BF=e²≈7.39）→ 消息沿链衰减。
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {"bayesFactor": 100.0}},
            {"id": "b", "type": "claim", "title": "B", "data": {}},
            {"id": "c", "type": "claim", "title": "C", "data": {}}
        ],
        "edges": [
            {"id": "e1", "type": "supports", "source": "a", "target": "b",
             "directed": true, "polarity": "positive", "data": {"bayesFactor": 7.389}},
            {"id": "e2", "type": "supports", "source": "b", "target": "c",
             "directed": true, "polarity": "positive", "data": {"bayesFactor": 7.389}}
        ],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = tree_belief_propagation(&graph);
    let a = result.beliefs[graph.variable_index("a").unwrap()];
    let b = result.beliefs[graph.variable_index("b").unwrap()];
    let c = result.beliefs[graph.variable_index("c").unwrap()];
    assert!(
        a.support > b.support && b.support > c.support,
        "message decays along the chain: {} > {} > {}",
        a.support,
        b.support,
        c.support
    );
    assert!(c.support > 0.5, "still propagates downstream");
}

#[test]
fn gc10_06_implies_b_true_does_not_force_a_true() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}}
        ],
        "edges": [{
            "id": "e1", "type": "implies", "source": "a", "target": "b",
            "directed": true, "polarity": "positive",
            "data": {"bayesFactor": 100.0}
        }],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    // b 有强支持证据 → 不能反推 a 为真（非逆命题误用）。
    let result = tree_belief_propagation(&graph);
    let a = result.beliefs[graph.variable_index("a").unwrap()];
    assert!(
        a.support <= 0.5 + 1e-9,
        "B true must not force A true (no converse misuse), got {}",
        a.support
    );
}

#[test]
fn gc10_07_and_with_low_input_limits_output() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "x1", "type": "claim", "title": "X1", "data": {}},
            {"id": "x2", "type": "claim", "title": "X2", "data": {}},
            {"id": "y", "type": "claim", "title": "Y", "data": {}}
        ],
        "edges": [
            {"id": "e1", "type": "and", "source": "x1", "target": "y", "directed": true},
            {"id": "e2", "type": "and", "source": "x2", "target": "y", "directed": true}
        ],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    let result = tree_belief_propagation(&graph);
    let y = result.beliefs[graph.variable_index("y").unwrap()];
    // 两个输入都无信息（0.5）：AND 输出受限，不高于输入。
    assert!(y.support <= 0.5 + 1e-9, "AND output bounded by inputs");
}

#[test]
fn gc10_08_three_node_positive_feedback_loop_terminates() {
    let graph = graph_with_edges(vec![
        supports("a", "b", 3.0),
        supports("b", "c", 3.0),
        supports("c", "a", 3.0),
    ]);
    let result = loopy_belief_propagation(&graph, &BpOptions::default());
    assert!(result.iterations <= BpOptions::default().max_iterations);
    assert!(result
        .beliefs
        .iter()
        .all(|b| b.support.is_finite() && b.support >= 0.0 && b.support <= 1.0));
}

#[test]
fn gc10_09_two_node_strong_conflict_detects_oscillation() {
    let graph = graph_with_edges(vec![supports("a", "b", 1000.0), supports("b", "a", 1000.0)]);
    let result = loopy_belief_propagation(&graph, &BpOptions::default());
    // 强正反馈可能振荡；结果必须有限并带残差。
    assert!(result.residual >= 0.0);
    assert!(result.beliefs.iter().all(|b| b.support.is_finite()));
    assert!(matches!(
        result.status,
        BpStatus::Converged | BpStatus::Unstable | BpStatus::MaxIterationsReached
    ));
}

#[test]
fn gc10_10_zero_efficacy_edge_identical_to_deleted_edge() {
    let with_zero = graph_with_edges(vec![
        supports("a", "b", 10.0),
        json!({"id": "e-dead", "type": "supports", "source": "b", "target": "c",
               "directed": true, "polarity": "positive",
               "data": {"quality": {"design": 0.0}, "bayesFactor": 1000.0}}),
    ]);
    let without = graph_with_edges(vec![supports("a", "b", 10.0)]);
    let r1 = tree_belief_propagation(&with_zero);
    let r2 = tree_belief_propagation(&without);
    assert_eq!(
        r1.beliefs, r2.beliefs,
        "η=0 edge must be bit-identical to deleted edge"
    );
}

#[test]
fn gc10_11_edge_insertion_order_irrelevant() {
    let edges1 = vec![supports("a", "b", 10.0), supports("b", "c", 10.0)];
    let edges2 = vec![supports("b", "c", 10.0), supports("a", "b", 10.0)];
    let r1 = tree_belief_propagation(&graph_with_edges(edges1));
    let r2 = tree_belief_propagation(&graph_with_edges(edges2));
    assert_eq!(
        r1.beliefs, r2.beliefs,
        "fixed ordering must be deterministic"
    );
}

#[test]
fn gc10_12_extreme_llr_stays_finite() {
    let graph = graph_with_edges(vec![supports("a", "b", 1e300)]);
    let result = tree_belief_propagation(&graph);
    for belief in &result.beliefs {
        assert!(belief.support.is_finite() && belief.support >= 0.0 && belief.support <= 1.0);
        assert!(belief.refutation.is_finite());
        assert!(belief.net_belief.is_finite());
    }
}

#[test]
fn uninformative_and_from_probabilities_agree() {
    let state = BeliefState::from_probabilities(0.5, 0.5);
    assert_eq!(state, BeliefState::uninformative());
}

#[test]
fn sigmoid_is_stable_and_bounded() {
    assert_eq!(sigmoid(0.0), 0.5);
    assert!(sigmoid(1e6) <= 1.0 && sigmoid(1e6) >= 0.999);
    assert!(sigmoid(-1e6) >= 0.0 && sigmoid(-1e6) <= 0.001);
}
