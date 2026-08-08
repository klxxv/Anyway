//! CLI 与内核（Tauri/Registry 端）一致性测试。
//!
//! Tauri 端经 `src-tauri/src/graph_compiler.rs` 薄转发层直接复用
//! `research_graph_compiler` 公共 API（Registry 端同源）。本测试对固定
//! fixture 运行编译出的 `canvas compile` 二进制，并把 stdout 与直接调用
//! 内核 API 计算的结果逐一比对 —— 证明 CLI 输出与 Tauri/Registry 端一致。

use research_graph_compiler::{compile_project, export_mermaid, verify_hashes};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/pinn-architecture.mycproj")
}

/// 运行 `canvas compile` 并返回 (stdout, stderr, exit_code)。
fn run_compile(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_canvas"))
        .arg("compile")
        .args(args)
        .output()
        .expect("failed to run canvas compile");
    (
        String::from_utf8(output.stdout).expect("stdout is utf-8"),
        String::from_utf8(output.stderr).expect("stderr is utf-8"),
        output.status.code().expect("exit code"),
    )
}

fn compile_fixture() -> research_graph_compiler::CompileResult {
    let bytes = std::fs::read(fixture()).expect("read fixture");
    compile_project(&bytes).expect("kernel compiles the fixture")
}

#[test]
fn json_report_matches_kernel_api() {
    let (stdout, stderr, code) = run_compile(&[
        fixture().to_str().unwrap(),
        "--output",
        "json",
    ]);
    assert_eq!(stderr, "");
    assert_eq!(code, 0);

    let report: Value = serde_json::from_str(&stdout).expect("stdout is valid json");
    let compiled = compile_fixture();

    // 哈希区：与内核 compile_project 的产物逐项一致。
    assert_eq!(report["ok"], Value::Bool(true));
    assert_eq!(
        report["hashes"]["contentRootHash"],
        Value::String(compiled.content_root_hash.clone())
    );
    assert_eq!(
        report["hashes"]["fileHash"],
        Value::String(compiled.file_hash.clone())
    );
    assert_eq!(
        report["hashes"]["blockHashes"],
        serde_json::to_value(&compiled.block_hashes).unwrap()
    );
    assert_eq!(
        report["hashes"]["verified"],
        Value::Bool(verify_hashes(&compiled.project).valid)
    );

    // 诊断区：与内核不变式检查结果一致（同一 InvariantViolation 序列化）。
    assert_eq!(
        report["diagnostics"],
        serde_json::to_value(&compiled.violations).unwrap()
    );

    // 项目摘要：fixture 元数据。
    assert_eq!(report["project"]["id"], json!("pinn-plugin-architecture"));
    assert_eq!(report["project"]["schemaVersion"], json!(2));
    assert_eq!(report["project"]["counts"]["nodes"], json!(12));
    assert_eq!(report["project"]["counts"]["edges"], json!(11));

    // 未开启 --logic/--bp/--layout 时不出现分析区。
    assert!(report.get("logicChains").is_none());
    assert!(report.get("contradictionChains").is_none());
    assert!(report.get("bp").is_none());
    assert!(report.get("layout").is_none());
}

#[test]
fn json_report_matches_kernel_api_with_hashes_field_injected() {
    // 编译产物把 blockHash 注入每个实体：CLI 报告的 project 摘要不携带实体，
    // 但 mermaid/分析区都基于内核注入后的项目 —— 验证注入本身可用。
    let compiled = compile_fixture();
    let bytes = serde_json::to_vec(&compiled.project).unwrap();
    let reparsed: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(reparsed["nodes"][0]["blockHash"].as_str().unwrap().len(), 12);
    assert_eq!(reparsed["contentRootHash"].as_str().unwrap().len(), 64);
    assert_eq!(reparsed["fileHash"].as_str().unwrap().len(), 64);
}

#[test]
fn analysis_flags_gate_sections() {
    // --logic --bp --layout 全部开启 → 三个分析区都出现。
    let (stdout, _, code) = run_compile(&[
        fixture().to_str().unwrap(),
        "--output",
        "json",
        "--logic",
        "--bp",
        "--layout",
    ]);
    assert_eq!(code, 0);
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert!(report.get("logicChains").is_some());
    assert!(report.get("contradictionChains").is_some());
    assert!(report.get("bp").is_some());
    assert!(report.get("layout").is_some());
    assert_eq!(report["options"]["logic"], json!(true));
    assert_eq!(report["options"]["bp"], json!(true));
    assert_eq!(report["options"]["layout"], json!(true));

    // 仅 --logic → bp/layout 不出现。
    let (stdout, _, _) = run_compile(&[fixture().to_str().unwrap(), "--logic"]);
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert!(report.get("logicChains").is_some());
    assert!(report.get("contradictionChains").is_some());
    assert!(report.get("bp").is_none());
    assert!(report.get("layout").is_none());
}

#[test]
fn analysis_sections_match_kernel_output() {
    // 分析区内容同样来自内核：因子图 / 矛盾报告 / 信念状态 / 布局序列化。
    let (stdout, _, _) = run_compile(&[
        fixture().to_str().unwrap(),
        "--logic",
        "--bp",
        "--layout",
    ]);
    let report: Value = serde_json::from_str(&stdout).unwrap();
    let compiled = compile_fixture();

    let factor_graph = research_graph_compiler::factor::compile::compile_factor_graph(
        &compiled.project,
    );
    let contradictions = research_graph_compiler::factor::contradiction::find_contradictions(
        &compiled.project,
        &research_graph_compiler::factor::contradiction::ContradictionOptions {
            max_depth: 16,
            ..Default::default()
        },
    );
    let beliefs = research_graph_compiler::factor::bp::tree_belief_propagation(&factor_graph);
    let layout = research_graph_compiler::layout::compute_layout(
        &compiled.project,
        research_graph_compiler::layout::LayoutMode::Hierarchical,
    );

    assert_eq!(
        report["logicChains"],
        serde_json::to_value(&factor_graph).unwrap()
    );
    assert_eq!(
        report["contradictionChains"],
        serde_json::to_value(&contradictions).unwrap()
    );
    assert_eq!(report["bp"], serde_json::to_value(&beliefs).unwrap());
    assert_eq!(report["layout"], serde_json::to_value(&layout).unwrap());
}

#[test]
fn mermaid_output_matches_kernel_export() {
    let (stdout, stderr, code) = run_compile(&[
        fixture().to_str().unwrap(),
        "--output",
        "mermaid",
    ]);
    assert_eq!(stderr, "");
    assert_eq!(code, 0);

    let compiled = compile_fixture();
    assert_eq!(stdout, export_mermaid(&compiled.project));
    // 确定性：两次运行逐字节一致。
    let (again, _, _) = run_compile(&[fixture().to_str().unwrap(), "--output", "mermaid"]);
    assert_eq!(again, stdout);
}

#[test]
fn text_output_contains_kernel_hashes() {
    let (stdout, _, code) = run_compile(&[fixture().to_str().unwrap(), "--output", "text"]);
    assert_eq!(code, 0);
    let compiled = compile_fixture();
    assert!(stdout.contains(&compiled.content_root_hash));
    assert!(stdout.contains(&compiled.file_hash));
    assert!(stdout.contains("verified: true"));
}

#[test]
fn clean_fixture_exits_zero() {
    let (_, _, code) = run_compile(&[fixture().to_str().unwrap()]);
    assert_eq!(code, 0);
}

#[test]
fn strict_mode_fails_on_warning_fixture() {
    // 构造含 warning（uncited-evidence）的临时项目：默认通过，--strict 失败。
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("warning.mycproj");
    let project = json!({
        "schemaVersion": 2, "id": "warn-p", "title": "W", "discipline": "D",
        "updatedAt": "2026-01-01T00:00:00Z", "revision": 1,
        "nodes": [
            {"id": "n1", "type": "question", "title": "Q", "body": "b", "tags": [],
             "data": {}, "evidenceIds": [], "status": "confirmed", "provenance": {}}
        ],
        "edges": [],
        "evidence": [
            {"id": "e1", "sourceType": "paper", "sourceId": "p", "title": "T",
             "status": "confirmed", "provenance": {}}
        ],
        "placements": [], "scenarios": [], "activity": []
    });
    std::fs::write(&path, serde_json::to_vec(&project).unwrap()).unwrap();

    let (stdout, _, default_code) = run_compile(&[path.to_str().unwrap()]);
    assert_eq!(default_code, 0, "warnings alone must not fail");
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["severity"], json!("warning"));

    let (_, _, strict_code) = run_compile(&[path.to_str().unwrap(), "--strict"]);
    assert_eq!(strict_code, 1, "--strict fails on any diagnostic");
}

#[test]
fn parse_failure_reports_diagnostic_and_exits_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("broken.mycproj");
    std::fs::write(&path, b"{ not valid json").unwrap();

    let (stdout, _, code) = run_compile(&[path.to_str().unwrap()]);
    assert_eq!(code, 1);
    let report: Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(report["ok"], json!(false));
    assert_eq!(report["diagnostics"][0]["code"], json!("parse-error"));
    assert_eq!(report["diagnostics"][0]["severity"], json!("error"));
}

#[test]
fn io_error_exits_three() {
    let (_, stderr, code) = run_compile(&["definitely-missing-project.mycproj"]);
    assert_eq!(code, 3);
    assert!(stderr.contains("cannot read"));
}

#[test]
fn usage_error_exits_two() {
    for args in [
        &[][..],
        &["--fancy", "p.mycproj"][..],
        &["--output", "xml", fixture().to_str().unwrap()][..],
    ] {
        let (_, stderr, code) = run_compile(args);
        assert_eq!(code, 2, "args: {args:?}");
        assert!(!stderr.is_empty());
    }
}

#[test]
fn mermaid_is_deterministic_and_sorted() {
    let compiled = compile_fixture();
    let mermaid = export_mermaid(&compiled.project);
    assert!(mermaid.starts_with("flowchart LR\n"));
    // 节点行按 id 排序：第一行应为字母序最前的节点。
    let mut lines: Vec<&str> = mermaid.lines().skip(1).collect();
    let Some(first) = lines.first() else {
        panic!("mermaid has no node lines");
    };
    assert!(first.contains("auto-weighted-loss"), "first node {first}");
    let node_lines: Vec<&str> = lines
        .drain(..)
        .filter(|line| line.contains("[\""))
        .collect();
    assert_eq!(node_lines.len(), 12);
    // 边行引用合法端点。
    assert!(mermaid.lines().any(|line| line.contains("-->|controls|")));
    assert!(mermaid.lines().any(|line| line.contains("-->|derived_from|")));
}
