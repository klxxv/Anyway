//! Deterministic SSE -> Canvas -> BP -> Canvas Diff artifact fixture.
//!
//! The model-authored SSE contains research entities and statistical inputs,
//! never posterior/belief values. Those are computed by the graph compiler.

use research_graph_compiler::diff::{
    canvas_diff_with_granularity, diff_hunks, DiffGranularity,
};
use research_graph_compiler::{belief_propagation, compile_factor_graph, BpOptions};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::PathBuf;

fn node(
    id: &str,
    kind: &str,
    title: &str,
    body: &str,
    tags: &[&str],
    data: Value,
) -> Value {
    json!({
        "id": id,
        "type": kind,
        "title": title,
        "body": body,
        "tags": tags,
        "status": "confirmed",
        "evidenceIds": [],
        "data": data,
        "provenance": {"origin": "ai", "modelId": "sse-fixture"},
        "createdAt": "2026-08-13T00:00:00.000Z",
        "updatedAt": "2026-08-13T00:00:00.000Z"
    })
}

fn edge(
    id: &str,
    kind: &str,
    source: &str,
    target: &str,
    confidence: f64,
    p_value: Option<f64>,
    direction: &str,
    experiment: Option<Value>,
) -> Value {
    let mut data = json!({
        "direction": direction,
        "quality": {
            "design": 0.94,
            "source": 0.96,
            "conditionMatch": 0.91,
            "independence": 0.90,
            "reproducibility": 0.88
        }
    });
    if let Some(value) = p_value {
        data["pValue"] = json!(value);
    }
    json!({
        "id": id,
        "type": kind,
        "source": source,
        "target": target,
        "directed": true,
        "polarity": if direction == "refutes" { "negative" } else { "positive" },
        "confidence": confidence,
        "conditions": [],
        "evidenceIds": [],
        "experiment": experiment,
        "data": data,
        "provenance": {"origin": "ai", "modelId": "sse-fixture"}
    })
}

fn base_project() -> Value {
    let nodes = vec![
        node(
            "q-retention",
            "question",
            "Does residual attention retain long context?",
            "Evaluate retrieval and perplexity under long-context stress.",
            &["question", "long-context"],
            json!({}),
        ),
        node(
            "h-residual",
            "hypothesis",
            "Residual attention improves retention",
            "A learned residual gate preserves earlier token evidence.",
            &["hypothesis", "residual-attention"],
            json!({}),
        ),
        node(
            "v-residual-gate",
            "variable",
            "Residual attention gate α",
            "Primary independent variable.",
            &["variable", "independent"],
            json!({"valueType":"enum", "enumValues":["off","0.25","0.50","0.75"]}),
        ),
        node(
            "v-context-length",
            "variable",
            "Context length",
            "Controlled context window.",
            &["variable", "control"],
            json!({"valueType":"enum", "enumValues":["32k","64k","128k"]}),
        ),
        node(
            "exp-primary",
            "experiment",
            "Primary residual-attention ablation",
            "Compare α=off against α=0.50 at 64k context.",
            &["experiment", "ablation"],
            json!({"round":1}),
        ),
        node(
            "r-retrieval",
            "result",
            "Needle retrieval +8.4 pp",
            "Residual attention improves exact retrieval.",
            &["result", "retrieval"],
            json!({"pValue":0.004, "metric":"accuracy", "delta":8.4}),
        ),
        node(
            "r-perplexity",
            "result",
            "Perplexity −5.1%",
            "Lower perplexity under 64k context.",
            &["result", "perplexity"],
            json!({"pValue":0.032, "metric":"perplexity", "delta":-5.1}),
        ),
        node(
            "r-latency",
            "result",
            "Decode latency +6.2%",
            "Residual path increases decode cost.",
            &["result", "latency"],
            json!({"pValue":0.081, "metric":"latency", "delta":6.2}),
        ),
    ];
    let experiment = json!({
        "id":"exp-primary", "label":"Primary ablation", "metric":"retrieval-accuracy",
        "baseline":71.2, "value":79.6, "delta":8.4, "outcome":"supports", "status":"completed"
    });
    let edges = vec![
        edge("e-q-h", "depends_on", "q-retention", "h-residual", 0.82, None, "supports", None),
        edge("e-gate-h", "supports", "v-residual-gate", "h-residual", 0.91, Some(0.004), "supports", Some(experiment)),
        edge("e-context-h", "depends_on", "v-context-length", "h-residual", 0.76, None, "supports", None),
        edge("e-retrieval-h", "supports", "r-retrieval", "h-residual", 0.93, Some(0.004), "supports", None),
        edge("e-ppl-h", "supports", "r-perplexity", "h-residual", 0.84, Some(0.032), "supports", None),
        edge("e-latency-h", "contradicts", "r-latency", "h-residual", 0.67, Some(0.081), "refutes", None),
    ];
    project("sse-residual-attention-a", "Residual Attention · SSE baseline", nodes, edges)
}

fn comparison_project() -> Value {
    let mut project = base_project();
    let nodes = project["nodes"].as_array_mut().expect("nodes");
    nodes.retain(|node| node["id"] != "r-perplexity");
    for item in nodes.iter_mut() {
        match item["id"].as_str() {
            Some("exp-primary") => {
                item["title"] = json!("Primary ablation · independent replication");
                item["body"] = json!("Second test at 128k context with a fresh random seed.");
                item["data"] = json!({"round":2, "diffIntent":"retest", "replication":"independent"});
                item["updatedAt"] = json!("2026-08-14T00:00:00.000Z");
            }
            Some("v-residual-gate") => {
                item["title"] = json!("Residual gate α · decomposed");
                item["data"] = json!({
                    "valueType":"enum", "enumValues":["off","0.25","0.50","0.75"],
                    "diffIntent":"variable-fission", "components":["head gate","depth gate"]
                });
                item["updatedAt"] = json!("2026-08-14T00:00:00.000Z");
            }
            _ => {}
        }
    }
    nodes.push(node(
        "v-head-gate",
        "variable",
        "Per-head residual gate",
        "Variable fission: isolate head-specific gating.",
        &["variable", "variable-fission"],
        json!({"valueType":"enum", "enumValues":["shared","per-head"], "diffIntent":"variable-fission"}),
    ));
    nodes.push(node(
        "v-depth-gate",
        "variable",
        "Per-depth residual gate",
        "Variable fission: isolate layer-depth gating.",
        &["variable", "variable-fission"],
        json!({"valueType":"enum", "enumValues":["shared","early","late"], "diffIntent":"variable-fission"}),
    ));
    nodes.push(node(
        "r-replication",
        "result",
        "128k replication +6.9 pp",
        "Independent replication remains positive at 128k context.",
        &["result", "replication"],
        json!({"pValue":0.011, "metric":"accuracy", "delta":6.9}),
    ));

    let edges = project["edges"].as_array_mut().expect("edges");
    edges.retain(|edge| edge["id"] != "e-ppl-h");
    if let Some(gate_edge) = edges.iter_mut().find(|edge| edge["id"] == "e-gate-h") {
        gate_edge["confidence"] = json!(0.94);
        gate_edge["data"]["pValue"] = json!(0.003);
        gate_edge["experiment"] = json!({
            "id":"exp-primary", "label":"Independent 128k replication", "metric":"retrieval-accuracy",
            "baseline":68.3, "value":75.2, "delta":6.9, "outcome":"supports", "status":"completed"
        });
    }
    edges.push(edge("e-head-h", "supports", "v-head-gate", "h-residual", 0.88, Some(0.018), "supports", None));
    edges.push(edge("e-depth-h", "supports", "v-depth-gate", "h-residual", 0.86, Some(0.024), "supports", None));
    edges.push(edge("e-replication-h", "supports", "r-replication", "h-residual", 0.91, Some(0.011), "supports", None));
    project["id"] = json!("sse-residual-attention-b");
    project["title"] = json!("Residual Attention · SSE replication");
    project["revision"] = json!(2);
    project["updatedAt"] = json!("2026-08-14T00:00:00.000Z");
    project
}

fn project(id: &str, title: &str, nodes: Vec<Value>, edges: Vec<Value>) -> Value {
    json!({
        "schemaVersion": 3,
        "id": id,
        "title": title,
        "discipline": "Machine Learning",
        "updatedAt": "2026-08-13T00:00:00.000Z",
        "revision": 1,
        "nodes": nodes,
        "edges": edges,
        "evidence": [],
        "placements": [],
        "scenarios": [],
        "activity": []
    })
}

fn sse_for(project: &Value, label: &str) -> String {
    let result = serde_json::to_string(&json!({"project":project})).expect("fixture JSON");
    let one = result.len() / 3;
    let two = result.len() * 2 / 3;
    let parts = [
        "<myc_pro".to_string(),
        format!("gress>{{\"stage\":\"{label}\",\"summary\":\"Canvas entities assembled\",\"evidenceCount\":3,\"warningCount\":0}}</myc_progress><myc_res"),
        format!("ult>{}", &result[..one]),
        result[one..two].to_string(),
        format!("{}</myc_result>", &result[two..]),
    ];
    let mut wire = String::new();
    wire.push_str("data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"private and ignored\"}}]}\n\n");
    for content in parts {
        wire.push_str("data: ");
        wire.push_str(&json!({"choices":[{"delta":{"content":content}}]}).to_string());
        wire.push_str("\n\n");
    }
    wire.push_str("data: [DONE]\n\n");
    wire
}

fn parse_openai_sse_project(wire: &str) -> Result<Value, String> {
    let mut content = String::new();
    for line in wire.lines() {
        let Some(data) = line.strip_prefix("data: ") else { continue };
        if data == "[DONE]" { break; }
        let event: Value = serde_json::from_str(data).map_err(|error| error.to_string())?;
        if let Some(delta) = event.pointer("/choices/0/delta/content").and_then(Value::as_str) {
            content.push_str(delta);
        }
    }
    let open = "<myc_result>";
    let close = "</myc_result>";
    let start = content.find(open).ok_or("missing myc_result open marker")? + open.len();
    let end = content[start..]
        .find(close)
        .map(|offset| start + offset)
        .ok_or("missing myc_result close marker")?;
    let envelope: Value = serde_json::from_str(&content[start..end]).map_err(|error| error.to_string())?;
    envelope.get("project").cloned().ok_or("missing project result".to_string())
}

fn compile_manifest(project: &Value) -> Value {
    let factor_graph = compile_factor_graph(project);
    let bp = belief_propagation(&factor_graph, &BpOptions::default());
    let beliefs = factor_graph
        .variables
        .iter()
        .zip(bp.beliefs.iter())
        .map(|(variable, belief)| {
            (variable.name.clone(), serde_json::to_value(belief).expect("belief JSON"))
        })
        .collect::<BTreeMap<_, _>>();
    let mean = if bp.beliefs.is_empty() {
        0.5
    } else {
        bp.beliefs.iter().map(|belief| belief.net_belief).sum::<f64>() / bp.beliefs.len() as f64
    };
    json!({
        "project": project,
        "beliefsByNode": beliefs,
        "bp": {
            "converged": bp.converged,
            "iterations": bp.iterations,
            "residual": bp.residual,
            "status": bp.status,
            "meanNetBelief": mean,
            "variableCount": bp.beliefs.len(),
            "diagnostics": factor_graph.diagnostics,
        }
    })
}

fn diff_manifest(base: &Value, comparison: &Value) -> Value {
    let diff = canvas_diff_with_granularity(base, comparison, DiffGranularity::FieldLevel);
    let hunks = diff_hunks(base, comparison, &diff);
    let mut yellow_nodes = BTreeSet::new();
    for project in [base, comparison] {
        for node in project["nodes"].as_array().into_iter().flatten() {
            if node.pointer("/data/diffIntent").and_then(Value::as_str)
                .is_some_and(|intent| matches!(intent, "retest" | "variable-fission"))
            {
                if let Some(id) = node["id"].as_str() { yellow_nodes.insert(id.to_string()); }
            }
        }
    }
    json!({
        "baseline": base,
        "comparison": comparison,
        "diff": diff,
        "hunks": hunks,
        "renderPolicy": {
            "added": "green",
            "removed": "red",
            "specialModifiedOrFission": "yellow",
            "yellowNodeIds": yellow_nodes,
        }
    })
}

fn generate() -> Result<Value, String> {
    let base_wire = sse_for(&base_project(), "baseline");
    let comparison_wire = sse_for(&comparison_project(), "replication");
    let base = parse_openai_sse_project(&base_wire)?;
    let comparison = parse_openai_sse_project(&comparison_wire)?;
    Ok(json!({
        "schemaVersion": 1,
        "source": "OpenAI-compatible SSE fixtures with myc_progress/myc_result framing",
        "base": compile_manifest(&base),
        "comparison": compile_manifest(&comparison),
        "canvasDiff": diff_manifest(&base, &comparison),
        "sse": {"base": base_wire, "comparison": comparison_wire}
    }))
}

fn main() -> Result<(), String> {
    let output = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from("target/sse-canvas-artifacts/manifest.json")
    });
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let manifest = generate()?;
    fs::write(&output, serde_json::to_vec_pretty(&manifest).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    println!("{}", output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_is_materialized_before_bp_and_never_imports_reasoning() {
        let manifest = generate().expect("fixture generation");
        let base = &manifest["base"];
        assert_eq!(base["project"]["nodes"].as_array().unwrap().len(), 8);
        assert_eq!(base["bp"]["converged"], true);
        assert!(base["bp"]["meanNetBelief"].as_f64().is_some_and(|value| value > 0.5 && value < 1.0));
        assert!(!serde_json::to_string(&base["project"]).unwrap().contains("private and ignored"));
        assert!(base["bp"]["diagnostics"].as_array().unwrap().iter().all(|item| item["code"] != "agent-injected-trust-fact"));
    }

    #[test]
    fn diff_has_green_red_and_special_yellow_semantics() {
        let manifest = generate().expect("fixture generation");
        let canvas_diff = &manifest["canvasDiff"];
        let diff = &canvas_diff["diff"];
        assert!(diff["addedNodes"].as_array().unwrap().iter().any(|id| id == "r-replication"));
        assert!(diff["removedNodes"].as_array().unwrap().iter().any(|id| id == "r-perplexity"));
        let yellow = canvas_diff["renderPolicy"]["yellowNodeIds"].as_array().unwrap();
        for expected in ["exp-primary", "v-residual-gate", "v-head-gate", "v-depth-gate"] {
            assert!(yellow.iter().any(|id| id == expected), "missing yellow {expected}");
        }
    }

    #[test]
    fn changed_experiment_and_gate_have_field_level_hunks() {
        let manifest = generate().expect("fixture generation");
        let hunks = manifest["canvasDiff"]["hunks"].as_array().unwrap();
        for expected in ["exp-primary", "v-residual-gate"] {
            let hunk = hunks.iter().find(|item| item["entityId"] == expected).expect("modified hunk");
            assert_eq!(hunk["operation"], "modified");
            assert!(!hunk["changedFields"].as_array().unwrap().is_empty());
        }
    }
}
