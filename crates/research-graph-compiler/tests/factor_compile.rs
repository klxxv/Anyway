//! 因子图编译集成测试（spec GC-08 + Agent 注入拒绝）。

use research_graph_compiler::{compile_factor_graph, FactorDiagnostic, FactorKind, Severity};
use serde_json::{json, Value};

fn project_with_edges(edges: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}},
            {"id": "c", "type": "claim", "title": "C", "data": {}},
            {"id": "v", "type": "variable", "title": "V",
             "data": {"valueType": "enum", "enumValues": ["x", "y"]}},
            {"id": "paper", "type": "paper", "title": "P", "data": {}}
        ],
        "edges": edges,
        "evidence": []
    })
}

/// 回归:未知边类型不得静默编译为 Supports(自有文档声明不生成因子)。
#[test]
fn unknown_edge_type_is_skipped_with_diagnostic_not_supports() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "causes", "source": "a", "target": "b",
        "directed": true, "polarity": "positive"
    })]);
    let graph = compile_factor_graph(&project);
    assert!(
        graph.factors.is_empty(),
        "causes has no factor semantics and must not silently become Supports"
    );
    assert!(
        graph
            .diagnostics
            .iter()
            .any(|d| d.code == "unsupported-edge-factor"),
        "{:?}",
        graph.diagnostics
    );
}

/// 回归:无 data 字段的 claim 不得从因子图静默消失(按默认布尔变量编译)。
#[test]
fn claim_without_data_still_enters_factor_graph() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "bare", "type": "claim", "title": "No data claim"},
            {"id": "a", "type": "claim", "title": "A", "data": {}}
        ],
        "edges": [
            {"id": "e1", "type": "supports", "source": "a", "target": "bare",
             "directed": true, "polarity": "positive"}
        ],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    assert!(
        graph.variable_index("bare").is_some(),
        "claim without data must compile to a default boolean variable"
    );
    assert!(
        graph.factors.iter().any(|f| f.variables.iter().any(|v| v == "bare")),
        "edges to a data-less claim must not dangle out of BP"
    );
}

#[test]
fn gc08_01_supports_edge_compiles_binary_factor() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "supports", "source": "a", "target": "b",
        "directed": true, "polarity": "positive",
        "data": {"bayesFactor": 10.0}
    })]);
    let graph = compile_factor_graph(&project);
    assert_eq!(graph.factors.len(), 1);
    assert_eq!(graph.factors[0].kind, FactorKind::Supports);
    assert_eq!(graph.factors[0].variables, vec!["a", "b"]);
    assert!(graph.factors[0].grounded);
    assert!((graph.factors[0].effective_message.unwrap() - 10f64.ln()).abs() < 1e-12);
    assert!(
        !graph
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error),
        "{:?}",
        graph.diagnostics
    );
}

#[test]
fn gc08_02_contradicts_edge_flips_polarity() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "contradicts", "source": "a", "target": "b",
        "directed": true, "polarity": "negative",
        "data": {"bayesFactor": 5.0}
    })]);
    let graph = compile_factor_graph(&project);
    assert_eq!(graph.factors[0].kind, FactorKind::Contradicts);
    // contradicts 边默认方向 refutes → BF 取倒数，λ = −ln(5)（反驳证据）。
    // 翻转极性发生在 BP 消息层（Contradicts 因子消息进入反驳通道）。
    assert!(graph.factors[0].local_log_evidence.unwrap() < 0.0);
    assert_eq!(graph.factors[0].local_log_evidence.unwrap(), -5f64.ln());
}

#[test]
fn gc08_03_implies_compiles() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "implies", "source": "a", "target": "b",
        "directed": true, "polarity": "positive"
    })]);
    let graph = compile_factor_graph(&project);
    assert_eq!(graph.factors[0].kind, FactorKind::Implies);
    assert!(!graph.factors[0].grounded);
}

#[test]
fn gc08_04_and_or_depends_on_compile() {
    let project = project_with_edges(vec![
        json!({"id": "e1", "type": "and", "source": "a", "target": "b", "directed": true}),
        json!({"id": "e2", "type": "or", "source": "b", "target": "c", "directed": true}),
        json!({"id": "e3", "type": "depends_on", "source": "a", "target": "c", "directed": true,
               "data": {"quality": {"design": 0.5}} }),
    ]);
    let graph = compile_factor_graph(&project);
    let kinds: Vec<FactorKind> = graph.factors.iter().map(|f| f.kind).collect();
    assert!(kinds.contains(&FactorKind::And));
    assert!(kinds.contains(&FactorKind::Or));
    assert!(kinds.contains(&FactorKind::DependsOn));
    // depends_on 门控：η=0.5。
    let depends = graph
        .factors
        .iter()
        .find(|f| f.kind == FactorKind::DependsOn)
        .unwrap();
    assert_eq!(depends.efficacy, 0.5);
    assert_eq!(depends.variables, vec!["a", "c"]);
}

#[test]
fn gc08_06_depends_on_zero_efficacy_gates_factor() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "depends_on", "source": "a", "target": "b", "directed": true,
        "data": {"quality": {"design": 0.0}}
    })]);
    let graph = compile_factor_graph(&project);
    let depends = &graph.factors[0];
    assert_eq!(depends.efficacy, 0.0);
    assert_eq!(depends.effective_message, None); // η=0 → 无有效消息
}

#[test]
fn gc08_07_enum_variable_has_finite_domain() {
    let project = project_with_edges(vec![]);
    let graph = compile_factor_graph(&project);
    let variable = graph.variable_index("v").unwrap();
    assert_eq!(graph.variables[variable].domain, vec!["x", "y"]);
}

#[test]
fn gc08_08_continuous_float_rejected_without_adapter() {
    let project = json!({
        "schemaVersion": 3,
        "nodes": [{"id": "cont", "type": "variable", "title": "Continuous",
                    "data": {"valueType": "float", "min": 0.0, "max": 1.0}}],
        "edges": [],
        "evidence": []
    });
    let graph = compile_factor_graph(&project);
    assert!(graph.diagnostics.iter().any(|d| {
        d.code == "continuous-variable-needs-discretization" && d.entity == "node:cont"
    }));
}

#[test]
fn gc08_10_ungrounded_edge_kept_as_logical_factor() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "supports", "source": "a", "target": "b",
        "directed": true, "polarity": "positive", "evidenceIds": []
    })]);
    let graph = compile_factor_graph(&project);
    assert_eq!(graph.factors.len(), 1);
    assert!(!graph.factors[0].grounded);
    assert_eq!(graph.factors[0].effective_message, None);
}

#[test]
fn gc08_11_isolated_variable_reports_no_evidence() {
    let project = project_with_edges(vec![]);
    let graph = compile_factor_graph(&project);
    assert!(graph.diagnostics.iter().any(|d| d.code == "no-evidence"));
    // 孤立变量保留在图中（先验状态）。
    assert!(graph.variable_index("a").is_some());
}

#[test]
fn gc08_12_cycle_allowed_for_loopy_bp() {
    let project = project_with_edges(vec![
        json!({"id": "e1", "type": "supports", "source": "a", "target": "b",
               "directed": true, "data": {"bayesFactor": 3.0}}),
        json!({"id": "e2", "type": "supports", "source": "b", "target": "a",
               "directed": true, "data": {"bayesFactor": 3.0}}),
        json!({"id": "e3", "type": "supports", "source": "b", "target": "c",
               "directed": true, "data": {"bayesFactor": 3.0}}),
    ]);
    let graph = compile_factor_graph(&project);
    assert!(graph.has_cycle(), "a↔b 构成环");
    assert_eq!(graph.factors.len(), 3);
}

#[test]
fn gc08_09_duplicate_edge_evidence_deduplicated() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "supports", "source": "a", "target": "b",
        "directed": true, "polarity": "positive",
        "evidenceIds": ["ev-1", "ev-1"]
    })]);
    let project = {
        let mut p = project;
        p["evidence"] = json!([
            {"id": "ev-1", "sourceType": "paper", "sourceId": "p", "title": "t",
             "locator": {}, "status": "verified", "provenance": {"origin": "human"}}
        ]);
        p
    };
    let graph = compile_factor_graph(&project);
    assert!(graph
        .diagnostics
        .iter()
        .any(|d| d.code == "duplicate-edge-evidence"));
    // 同一证据只编译一次（无重复因子）。
    assert_eq!(graph.factors.len(), 1);
}

#[test]
fn agent_injected_hash_posterior_layout_rejected() {
    let mut project = project_with_edges(vec![]);
    project["nodes"][0]["data"] = json!({
        "blockHash": "deadbeef", "posteriorProbability": 0.99, "x": 10, "y": 20
    });
    let graph = compile_factor_graph(&project);
    let injected: Vec<&FactorDiagnostic> = graph
        .diagnostics
        .iter()
        .filter(|d| d.code == "agent-injected-trust-fact")
        .collect();
    assert_eq!(injected.len(), 4, "{:?}", graph.diagnostics); // blockHash/posterior/x/y
                                                              // 注入值绝不影响编译：变量仍是无信息先验布尔变量。
    let variable = graph.variable_index("a").unwrap();
    assert_eq!(graph.variables[variable].prior_support_logit, 0.0);
    assert_eq!(graph.variables[variable].domain, vec!["true", "false"]);
}

#[test]
fn agent_injected_facts_on_edges_rejected() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "supports", "source": "a", "target": "b",
        "directed": true, "polarity": "positive",
        "data": {"posterior": 0.9}
    })]);
    let graph = compile_factor_graph(&project);
    assert!(graph
        .diagnostics
        .iter()
        .any(|d| d.code == "agent-injected-trust-fact"));
    // 注入的后验不进入有效消息（因子未接地、无 λ）。
    assert_eq!(graph.factors[0].effective_message, None);
    assert_eq!(graph.factors[0].local_log_evidence, None);
}

#[test]
fn factors_sorted_by_edge_id_for_determinism() {
    let project = project_with_edges(vec![
        json!({"id": "z9", "type": "supports", "source": "a", "target": "b", "directed": true}),
        json!({"id": "a1", "type": "supports", "source": "b", "target": "c", "directed": true}),
    ]);
    let graph = compile_factor_graph(&project);
    assert_eq!(graph.factors[0].source_edge.as_deref(), Some("a1"));
    assert_eq!(graph.factors[1].source_edge.as_deref(), Some("z9"));
}

#[test]
fn self_loop_support_does_not_amplify() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "supports", "source": "a", "target": "a",
        "directed": true, "polarity": "positive", "data": {"bayesFactor": 100.0}
    })]);
    let graph = compile_factor_graph(&project);
    assert!(graph
        .diagnostics
        .iter()
        .any(|d| d.code == "redundant-self-support"));
    assert_eq!(graph.factors[0].effective_message, None);
}

#[test]
fn paper_nodes_are_not_belief_variables() {
    let project = project_with_edges(vec![json!({
        "id": "e1", "type": "supports", "source": "paper", "target": "a",
        "directed": true, "polarity": "positive", "data": {"bayesFactor": 10.0}
    })]);
    let graph = compile_factor_graph(&project);
    // paper 节点不是主张变量 → 不生成因子。
    assert!(graph.factors.is_empty());
}
