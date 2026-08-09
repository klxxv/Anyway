//! TS↔Rust 逐位比对用编译入口。
//! Harness binary used by the compiler-parity test: reads one JSON request on
//! stdin and writes one JSON result to stdout. It only calls pure graph
//! functions in the local `graph_algorithms_harness` module, so results are deterministic.
//!
//! Note: this module is intentionally a bit-identical port of the TS originals
//! for parity testing only; the production graph compiler lives in
//! `src/graph_compiler`.
//!
//! Request shape (stdin):
//! ```json
//! { "project": <ProjectState JSON>, "command": "<cmd>", "args": { ... } }
//! ```
//!
//! Supported commands:
//! - `canonicalize`   (args: {})            -> { "bytes": "<utf8 canonical json>" }
//! - `traverse`       (args: request)       -> TraversalResult (durationMs=0)
//! - `cycles`         (args: {scenarioId})  -> Cycle[]
//! - `shortestPath`   (args: {source,target,scenarioId}) -> string[]
//! - `allShortestPaths`(args: {source,target,scenarioId}) -> string[][]
//! - `reachability`   (args: {root,scenarioId}) -> ScenarioDiff
//! - `logicChain`     (args: {mode,targetId}) -> LogicChainResult
//! - `influence`      (args: {targetId,maxIterations}) -> InfluenceResult
//! - `layout`         (args: {mode,rootId})  -> LayoutResult

mod graph_algorithms_harness;

use serde_json::{json, Value};
use std::io::{self, Read};

fn main() {
    let mut input = String::new();
    if io::stdin().read_to_string(&mut input).is_err() {
        eprintln!(r#"{{"ok":false,"error":"read-stdin"}}"#);
        std::process::exit(2);
    }
    let request: Value = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => {
            eprintln!(
                r#"{{"ok":false,"error":"parse-request","detail":{}}}"#,
                json!(error.to_string())
            );
            std::process::exit(3);
        }
    };
    let project = &request["project"];
    let command = request["command"].as_str().unwrap_or("");
    let args = request.get("args").cloned().unwrap_or_else(|| json!({}));

    let result = match command {
        "canonicalize" => json!({
            "bytes": String::from_utf8(crate_canonicalize(project)).expect("canonical bytes are utf8")
        }),
        "traverse" => graph_algorithms_harness::traverse(project, &args),
        "cycles" => graph_algorithms_harness::detect_cycles(
            project,
            args.get("scenarioId").and_then(Value::as_str),
        ),
        "shortestPath" => graph_algorithms_harness::shortest_path(
            project,
            args["source"].as_str().unwrap_or_default(),
            args["target"].as_str().unwrap_or_default(),
            args.get("scenarioId").and_then(Value::as_str),
        ),
        "allShortestPaths" => graph_algorithms_harness::all_shortest_paths(
            project,
            args["source"].as_str().unwrap_or_default(),
            args["target"].as_str().unwrap_or_default(),
            args.get("scenarioId").and_then(Value::as_str),
        ),
        "reachability" => graph_algorithms_harness::compare_reachability(
            project,
            args["root"].as_str().unwrap_or_default(),
            args["scenarioId"].as_str().unwrap_or_default(),
        ),
        "logicChain" => graph_algorithms_harness::compute_logic_chain(
            project,
            args["mode"].as_str().unwrap_or_default(),
            args.get("targetId").and_then(Value::as_str),
        ),
        "influence" => graph_algorithms_harness::propagate_influence(
            project,
            args["targetId"].as_str().unwrap_or_default(),
            args.get("maxIterations").and_then(Value::as_i64),
        ),
        "layout" => graph_algorithms_harness::compute_layout(
            project,
            args["mode"].as_str().unwrap_or_default(),
            args.get("rootId").and_then(Value::as_str),
        ),
        _ => {
            eprintln!(r#"{{"ok":false,"error":"unknown-command","command":{}}}"#, json!(command));
            std::process::exit(4);
        }
    };
    println!("{}", result);
}

/// 包装 graph_compiler 的 canonicalize，供本引导程序复用。
fn crate_canonicalize(project: &Value) -> Vec<u8> {
    research_canvas_desktop_lib::graph_compiler::canonicalize(project)
}
