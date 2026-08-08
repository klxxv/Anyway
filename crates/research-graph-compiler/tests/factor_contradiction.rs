//! 结构矛盾见证集成测试（spec GC-11）。

use research_graph_compiler::{find_contradictions, ContradictionOptions, Severity};
use serde_json::{json, Value};

fn project(edges: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 3,
        "nodes": [
            {"id": "a", "type": "claim", "title": "A", "data": {}},
            {"id": "b", "type": "claim", "title": "B", "data": {}},
            {"id": "c", "type": "claim", "title": "C", "data": {}},
            {"id": "d", "type": "claim", "title": "D", "data": {}}
        ],
        "edges": edges,
        "evidence": []
    })
}

fn edge(id: &str, etype: &str, source: &str, target: &str) -> Value {
    json!({"id": id, "type": etype, "source": source, "target": target,
           "directed": true, "polarity": if etype == "contradicts" { "negative" } else { "positive" }})
}

#[test]
fn gc11_01_direct_contradiction_is_one_edge_witness() {
    let report = find_contradictions(
        &project(vec![edge("e1", "contradicts", "a", "b")]),
        &ContradictionOptions::default(),
    );
    assert_eq!(report.witnesses.len(), 1);
    assert_eq!(report.witnesses[0].kind, "direct-edge");
    assert_eq!(report.witnesses[0].paths, vec![vec!["e1"]]);
    assert!(!report.truncated);
}

#[test]
fn gc11_02_positive_and_negative_path_pair() {
    let report = find_contradictions(
        &project(vec![
            edge("e1", "contradicts", "a", "b"),
            edge("e2", "supports", "a", "b"),
        ]),
        &ContradictionOptions::default(),
    );
    let pair = report
        .witnesses
        .iter()
        .find(|w| w.kind == "path-pair")
        .expect("path-pair witness");
    assert_eq!(pair.paths[0], vec!["e2"]); // 正路径
    assert_eq!(pair.paths[1], vec!["e1"]); // 负边
}

/// 回归:多边正路径必须完整进入见证(parent 键带 ±1 parity,
/// 重建游标若把 parity 写成 0,路径会被截断成最后一条边)。
#[test]
fn multi_edge_positive_path_witness_is_complete() {
    let report = find_contradictions(
        &project(vec![
            edge("e1", "supports", "a", "b"),
            edge("e2", "supports", "b", "c"),
            edge("e3", "contradicts", "a", "c"),
        ]),
        &ContradictionOptions::default(),
    );
    let pair = report
        .witnesses
        .iter()
        .find(|w| w.kind == "path-pair")
        .expect("path-pair witness");
    assert_eq!(
        pair.paths[0],
        vec!["e1", "e2"],
        "the two-hop positive path must be witnessed in full, not truncated to its last edge"
    );
    assert_eq!(pair.paths[1], vec!["e3"]);
}

#[test]
fn gc11_03_negative_path_alone_is_not_conflict() {
    let report = find_contradictions(
        &project(vec![edge("e1", "contradicts", "a", "b")]),
        &ContradictionOptions::default(),
    );
    assert!(
        report.witnesses.iter().all(|w| w.kind != "path-pair"),
        "negative path alone must not be a dual-path conflict"
    );
    assert_eq!(report.witnesses.len(), 1); // 仅 direct-edge
}

#[test]
fn gc11_04_odd_negative_edge_cycle_detected() {
    // a→b(+), b→c(+), c→a(−)：一个负边环。
    let report = find_contradictions(
        &project(vec![
            edge("e1", "supports", "a", "b"),
            edge("e2", "supports", "b", "c"),
            edge("e3", "contradicts", "c", "a"),
        ]),
        &ContradictionOptions::default(),
    );
    let cycle = report.witnesses.iter().find(|w| w.kind == "signed-cycle");
    assert!(cycle.is_some(), "{:?}", report.witnesses);
}

#[test]
fn gc11_05_even_negative_edge_cycle_not_marked() {
    // a→b(+), b→c(−), c→a(−)：两个负边，环乘积 +1 → 不标记符号矛盾。
    let report = find_contradictions(
        &project(vec![
            edge("e1", "supports", "a", "b"),
            edge("e2", "contradicts", "b", "c"),
            edge("e3", "contradicts", "c", "a"),
        ]),
        &ContradictionOptions::default(),
    );
    // direct-edge 见证来自两条 contradicts 边本身；不得出现 signed-cycle。
    assert!(
        report.witnesses.iter().all(|w| w.kind != "signed-cycle"),
        "{:?}",
        report.witnesses
    );
}

#[test]
fn gc11_06_minimum_witnesses_stable_sorted() {
    let report = find_contradictions(
        &project(vec![
            edge("e1", "contradicts", "a", "b"),
            edge("e2", "contradicts", "b", "c"),
            edge("e3", "contradicts", "c", "d"),
        ]),
        &ContradictionOptions::default(),
    );
    assert_eq!(report.witnesses.len(), 3);
    // 全部 direct-edge（1 边）最短在前且稳定。
    assert!(report.witnesses.iter().all(|w| w.kind == "direct-edge"));
}

#[test]
fn gc11_07_depth_budget_marks_truncated() {
    // 长链：a→b→c→d 全正 + a contradicts d。正路径长度 3 超 max_depth=2。
    let report = find_contradictions(
        &project(vec![
            edge("e1", "supports", "a", "b"),
            edge("e2", "supports", "b", "c"),
            edge("e3", "supports", "c", "d"),
            edge("e4", "contradicts", "a", "d"),
        ]),
        &ContradictionOptions {
            max_depth: 2,
            ..ContradictionOptions::default()
        },
    );
    assert!(
        report.truncated,
        "depth-limited search must not claim no contradiction"
    );
    assert!(report.witnesses.iter().any(|w| w.kind == "direct-edge"));
    assert!(report.witnesses.iter().all(|w| w.kind != "path-pair"));
}

#[test]
fn gc11_10_self_contradiction_high_severity() {
    let report = find_contradictions(
        &project(vec![edge("e1", "contradicts", "a", "a")]),
        &ContradictionOptions::default(),
    );
    let witness = &report.witnesses[0];
    assert_eq!(witness.kind, "self-contradiction");
    assert_eq!(witness.severity, Severity::Error);
}

#[test]
fn gc11_11_and_factor_local_inconsistency() {
    let report = find_contradictions(
        &project(vec![
            edge("e1", "and", "a", "c"),
            edge("e2", "and", "b", "c"),
            edge("e3", "contradicts", "a", "b"),
        ]),
        &ContradictionOptions::default(),
    );
    let inconsistent = report
        .witnesses
        .iter()
        .find(|w| w.kind == "and-factor-inconsistent");
    assert!(inconsistent.is_some(), "{:?}", report.witnesses);
}

#[test]
fn gc11_09_min_confidence_excludes_weak_edges() {
    let mut weak = edge("e1", "contradicts", "a", "b");
    weak["confidence"] = json!(0.1);
    let report = find_contradictions(
        &project(vec![weak]),
        &ContradictionOptions {
            min_confidence: 0.5,
            ..ContradictionOptions::default()
        },
    );
    assert!(report.witnesses.is_empty(), "{:?}", report.witnesses);
}

#[test]
fn gc11_12_max_witnesses_truncates() {
    let report = find_contradictions(
        &project(vec![
            edge("e1", "contradicts", "a", "b"),
            edge("e2", "contradicts", "b", "c"),
            edge("e3", "contradicts", "c", "d"),
        ]),
        &ContradictionOptions {
            max_witnesses: 1,
            ..ContradictionOptions::default()
        },
    );
    assert_eq!(report.witnesses.len(), 1);
    assert!(report.truncated);
}
