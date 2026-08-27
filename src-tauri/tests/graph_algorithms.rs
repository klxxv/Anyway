//! 图算法集成测试：与 `tests/research-core.test.ts` 的 TS 行为对齐
//! （bit-identical 双实现验证的 Rust 侧）。fixture 等价于 `app/lib/fixtures.ts`
//! 的 `initialProject`。

use research_canvas_desktop_lib::graph_compiler::algorithms::{
    all_shortest_paths, shortest_path, traverse_graph, TraversalDirection, TraversalRequest,
    TraversalStrategy,
};
use research_canvas_desktop_lib::graph_compiler::analysis::{
    compare_scenario_reachability, compute_logic_chain, contradiction_chains, detect_cycles,
};
use serde_json::{json, Value};

fn node(id: &str, node_type: &str, evidence: &[&str]) -> Value {
    json!({
        "id": id, "type": node_type, "title": id, "body": id, "tags": [],
        "status": "confirmed", "evidenceIds": evidence,
        "data": {}, "provenance": {"origin": "human"},
        "createdAt": "2026-07-26T02:00:00.000Z", "updatedAt": "2026-07-26T02:00:00.000Z"
    })
}

fn edge(id: &str, source: &str, target: &str, edge_type: &str, evidence: &[&str]) -> Value {
    json!({
        "id": id, "source": source, "target": target, "type": edge_type,
        "directed": true,
        "polarity": if edge_type == "contradicts" { "negative" } else { "positive" },
        "confidence": if evidence.is_empty() { 0.72 } else { 0.89 },
        "conditions": [], "evidenceIds": evidence,
        "provenance": {"origin": "human"}
    })
}

/// 等价于 TS `initialProject` 的图（id/类型/方向与测试断言一致）。
fn initial_project() -> Value {
    json!({
        "schemaVersion": 1, "id": "project-transformer-ablation",
        "title": "Long-context Transformer ablation", "discipline": "Neural Networks",
        "updatedAt": "2026-07-26T02:00:00.000Z", "revision": 18,
        "nodes": [
            node("q1", "question", &["ev1"]),
            node("d1", "dataset", &["ev3"]),
            node("m1", "method", &[]),
            node("m2", "method", &["ev1"]),
            node("m3", "method", &["ev2"]),
            node("v1", "variable", &[]),
            node("v2", "variable", &[]),
            node("v3", "variable", &[]),
            node("h1", "hypothesis", &["ev1"]),
            node("x1", "experiment", &[]),
            node("r1", "metric", &["ev3"]),
            node("r2", "metric", &[]),
            node("p1", "result", &[]),
            node("e1", "evidence", &["ev1"])
        ],
        "edges": [
            edge("e-q-m1", "q1", "m1", "depends_on", &[]),
            edge("e-q-h1", "q1", "h1", "depends_on", &[]),
            edge("e-m1-m2", "m1", "m2", "depends_on", &[]),
            edge("e-m2-m3", "m2", "m3", "depends_on", &["ev1"]),
            edge("e-v1-m2", "v1", "m2", "moderates", &[]),
            edge("e-m3-x1", "m3", "x1", "uses", &[]),
            edge("e-h1-x1", "h1", "x1", "derived_from", &[]),
            edge("e-v2-x1", "v2", "x1", "controls", &[]),
            edge("e-v3-x1", "v3", "x1", "controls", &[]),
            edge("e-d1-x1", "d1", "x1", "uses", &[]),
            edge("e-x1-r1", "x1", "r1", "measures", &[]),
            edge("e-x1-r2", "x1", "r2", "measures", &[]),
            edge("e-r1-p1", "r1", "p1", "supports", &[]),
            edge("e-r2-p1", "r2", "p1", "supports", &[]),
            edge("e-e1-h1", "e1", "h1", "supports", &["ev1"])
        ],
        "evidence": [
            {"id": "ev1", "sourceType": "paper", "sourceId": "paper-rope-2024", "title": "Extending context windows with rotary embeddings", "status": "verified", "provenance": {"origin": "human"}},
            {"id": "ev2", "sourceType": "paper", "sourceId": "paper-attention-2017", "title": "Attention mechanisms", "status": "verified", "provenance": {"origin": "human"}},
            {"id": "ev3", "sourceType": "dataset", "sourceId": "longbench-subset-v2", "title": "LongBench protocol", "status": "verified", "provenance": {"origin": "human"}}
        ],
        "placements": [],
        "scenarios": [
            {"id": "scenario-no-rope", "name": "No positional encoding", "disabledNodeIds": ["m2"], "disabledEdgeIds": [], "nodeOverrides": {}, "edgeOverrides": {}, "parameters": {"seeds": [13, 21, 34], "matchedCompute": true}, "hypothesis": "h", "expectedEffect": "Primary metric −6% to −10%", "createdAt": "2026-07-26T02:00:00.000Z"},
            {"id": "scenario-no-long-data", "name": "No long-context examples", "disabledNodeIds": ["d1"], "disabledEdgeIds": [], "nodeOverrides": {}, "edgeOverrides": {}, "parameters": {"trainingSteps": "unchanged"}, "hypothesis": "h", "expectedEffect": "e", "createdAt": "2026-07-26T02:00:00.000Z"}
        ],
        "activity": []
    })
}

fn bfs_request(start_id: &str, scenario_id: Option<&str>) -> TraversalRequest {
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

#[test]
fn bfs_is_deterministic_and_returns_depth_parent_and_tree_edges() {
    let result = traverse_graph(&initial_project(), &bfs_request("q1", None));
    assert_eq!(&result.order[..3], &["q1", "h1", "m1"]);
    assert_eq!(result.depth.get("q1"), Some(&0));
    assert_eq!(result.parent.get("h1"), Some(&Some("q1".to_string())));
    assert!(result.tree_edge_ids.iter().any(|id| id == "e-q-h1"));
    assert!(result.order.iter().any(|id| id == "r1"));
    let unique: std::collections::HashSet<&String> = result.order.iter().collect();
    assert_eq!(unique.len(), result.order.len());
}

#[test]
fn dfs_distinguishes_tree_and_back_edges_when_a_directed_cycle_exists() {
    let mut project = initial_project();
    project["edges"]
        .as_array_mut()
        .unwrap()
        .push(edge("cycle-r1-q1", "r1", "q1", "causes", &[]));
    let result = traverse_graph(
        &project,
        &TraversalRequest {
            start_id: "q1".to_string(),
            strategy: TraversalStrategy::Dfs,
            direction: TraversalDirection::Out,
            max_depth: 12,
            edge_types: None,
            node_types: None,
            scenario_id: None,
        },
    );
    let cycles = detect_cycles(&project, None);
    assert!(result.back_edge_ids.iter().any(|id| id == "cycle-r1-q1"));
    assert!(!cycles.is_empty());
    assert!(cycles
        .iter()
        .all(|cycle| cycle.node_ids.contains(&"q1".to_string())));
    assert!(cycles
        .iter()
        .any(|cycle| cycle.edge_ids.iter().any(|id| id == "cycle-r1-q1")));
}

#[test]
fn scenario_overlay_filters_nodes_and_edges_without_mutating_the_base_graph() {
    let base = initial_project();
    let before = serde_json::to_string(&base).unwrap();
    let diff = compare_scenario_reachability(&base, "q1", "scenario-no-rope");
    assert_eq!(diff.disabled_node_ids, vec!["m2"]);
    assert!(diff.lost_reachable_node_ids.iter().any(|id| id == "m3"));
    assert_eq!(serde_json::to_string(&base).unwrap(), before);
}

#[test]
fn shortest_path_follows_semantic_direction_and_returns_stable_ids() {
    assert_eq!(
        shortest_path(&initial_project(), "q1", "r1", None),
        vec!["q1", "h1", "x1", "r1"]
    );
    assert!(shortest_path(&initial_project(), "r1", "q1", None).is_empty());
}

#[test]
fn all_equally_short_paths_remain_deterministic() {
    let mut project = initial_project();
    project["edges"].as_array_mut().unwrap().push(edge(
        "parallel-q1-r1",
        "q1",
        "r1",
        "supports",
        &["ev1"],
    ));
    let paths = all_shortest_paths(&project, "q1", "r1", None);
    assert_eq!(paths, vec![vec!["q1", "r1"]]);
    // 无平行边时回到唯一最短路径。
    assert_eq!(
        all_shortest_paths(&initial_project(), "q1", "r1", None),
        vec![vec!["q1", "h1", "x1", "r1"]]
    );
}

#[test]
fn clean_graph_has_no_cycles_but_a_contradiction_ring_is_detected_as_minimal() {
    let base = initial_project();
    assert!(detect_cycles(&base, None).is_empty());

    let mut ring = base.clone();
    ring["edges"]
        .as_array_mut()
        .unwrap()
        .push(edge("cx-a-b", "n-a", "n-b", "contradicts", &[]));
    ring["edges"]
        .as_array_mut()
        .unwrap()
        .push(edge("cx-b-c", "n-b", "n-c", "contradicts", &[]));
    ring["edges"]
        .as_array_mut()
        .unwrap()
        .push(edge("cx-c-a", "n-c", "n-a", "contradicts", &[]));
    ring["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node("n-a", "concept", &[]));
    ring["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node("n-b", "concept", &[]));
    ring["nodes"]
        .as_array_mut()
        .unwrap()
        .push(node("n-c", "concept", &[]));
    let chains = contradiction_chains(&ring, None);
    assert_eq!(chains.minimal_size, Some(3));
    assert_eq!(chains.cycles.len(), 1);
    assert_eq!(chains.cycles[0].edge_ids.len(), 3);
    assert!(chains.considered_edge_ids.iter().any(|id| id == "cx-a-b"));
}

#[test]
fn logic_chain_selects_supporting_relations_and_scores_mean_confidence() {
    let project = initial_project();
    let evidence = compute_logic_chain(&project, "evidence", None);
    assert!(evidence.edge_ids.iter().any(|id| id == "e-r1-p1"));
    assert!(evidence.edge_ids.iter().any(|id| id == "e-e1-h1"));
    assert!(evidence.edge_ids.iter().any(|id| id == "e-h1-x1")); // derived_from
    assert!(!evidence.edge_ids.contains(&"e-m2-m3".to_string())); // depends_on 不在链内
                                                                  // 四边：e-h1-x1(0.72) + e-r1-p1(0.72) + e-r2-p1(0.72) + e-e1-h1(0.89)。
    let expected = (0.72 * 3.0 + 0.89) / 4.0;
    assert!(
        (evidence.score - expected).abs() < 1e-9,
        "score = {}",
        evidence.score
    );

    // 无实验边时 effective/refutation 为空链，score 回落 0.0（与 TS 一致：0 / max(1, 0)）。
    let effective = compute_logic_chain(&project, "effective", None);
    assert!(effective.edge_ids.is_empty());
    assert_eq!(effective.score, 0.0);
}

#[test]
fn compiled_project_round_trips_and_algorithm_inputs_are_pure() {
    use research_canvas_desktop_lib::graph_compiler::{compile, verify_hashes};
    let project = initial_project();
    let result = compile(&project);
    assert!(result.violations.is_empty(), "{:?}", result.violations);
    assert!(verify_hashes(&result.project).valid);
    // 算法不修改输入（纯函数）。
    let before = serde_json::to_string(&project).unwrap();
    let _ = traverse_graph(&project, &bfs_request("q1", None));
    let _ = contradiction_chains(&project, None);
    assert_eq!(serde_json::to_string(&project).unwrap(), before);
}
