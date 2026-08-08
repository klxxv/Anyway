//! `canvas` — Research Canvas 图编译器 CLI（CI 验证用）。
//!
//! 入口：`src/bin/canvas.rs`，引用 `crates/research-graph-compiler` 语义内核。
//! 本 CLI 不实现任何图算法：规范化、双哈希、不变式、逻辑链、矛盾链、BP、
//! 布局与 Mermaid 导出全部委托内核（与 Tauri 薄转发层 / Registry 端同源），
//! 保证同一 fixture 下 CLI 输出与 Tauri/Registry 端逐字节一致。
//!
//! 当前子命令：`compile`（`canvas diff` 预留，见 docs/architecture/canvas-diff-design.md §9.5）。
//!
//! 退出码：0 编译通过（无错误诊断）；1 存在错误诊断（或 `--strict` 下存在
//! 任意诊断）或解析失败；2 用法错误；3 文件读取失败。

use research_graph_compiler::{
    compile_project_with_options, export_mermaid, verify_hashes, CompileFailure, CompileOptions,
    CompileResult, InvariantViolation, Severity,
};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 矛盾链搜索预算（GC-11 maxDepth，骨架阶段不生效）。
const DEFAULT_MAX_DEPTH: usize = 16;

const EXIT_OK: u8 = 0;
const EXIT_DIAGNOSTICS: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_IO: u8 = 3;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match dispatch(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("canvas: {message}");
            eprintln!("Try 'canvas --help' for usage.");
            EXIT_USAGE
        }
    };
    std::process::exit(i32::from(code));
}

fn dispatch(args: &[String]) -> Result<u8, String> {
    if args.is_empty() {
        print_usage();
        return Ok(EXIT_USAGE);
    }
    match args[0].as_str() {
        "--help" | "-h" | "help" => {
            print_usage();
            Ok(EXIT_OK)
        }
        "--version" | "-V" | "version" => {
            println!("canvas {VERSION}");
            Ok(EXIT_OK)
        }
        "compile" => run_compile(&args[1..]),
        other => Err(format!("unknown subcommand {other:?}; expected 'compile'")),
    }
}

fn print_usage() {
    println!(
        "canvas {VERSION} — Research Canvas graph compiler CLI (CI verification)\n\
         \n\
         USAGE:\n\
         \x20   canvas <SUBCOMMAND> [OPTIONS] <PROJECT.mycproj>\n\
         \n\
         SUBCOMMANDS:\n\
         \x20   compile    Compile a .mycproj project and print the compile report\n\
         \n\
         GLOBAL:\n\
         \x20   -h, --help       Print help\n\
         \x20   -V, --version    Print version\n\
         \n\
         canvas compile [OPTIONS] <PROJECT.mycproj>\n\
         \n\
         OPTIONS:\n\
         \x20   --strict            Fail (exit 1) on any diagnostic, including warnings\n\
         \x20   --layout            Compute deterministic layout (GC-13)\n\
         \x20   --logic             Include logic chains + contradiction chains analysis\n\
         \x20   --bp                Include dual-channel belief propagation results\n\
         \x20   --output <fmt>      Report format: json (default), text, mermaid\n\
         \x20   -h, --help          Print compile help\n\
         \n\
         EXIT CODES:\n\
         \x20   0  compile passed (no error diagnostics)\n\
         \x20   1  error diagnostics, or any diagnostic with --strict, or parse failure\n\
         \x20   2  usage error\n\
         \x20   3  cannot read the input file"
    );
}

// ---------------------------------------------------------------------------
// 选项解析 / Option parsing
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputFormat {
    Json,
    Text,
    Mermaid,
}

impl OutputFormat {
    fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "json" => Ok(OutputFormat::Json),
            "text" => Ok(OutputFormat::Text),
            "mermaid" => Ok(OutputFormat::Mermaid),
            other => Err(format!(
                "invalid --output value {other:?}; expected json, mermaid or text"
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            OutputFormat::Json => "json",
            OutputFormat::Text => "text",
            OutputFormat::Mermaid => "mermaid",
        }
    }
}

#[derive(Clone, Debug)]
struct Options {
    input: Option<PathBuf>,
    strict: bool,
    layout: bool,
    logic: bool,
    bp: bool,
    output: OutputFormat,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            input: None,
            strict: false,
            layout: false,
            logic: false,
            bp: false,
            output: OutputFormat::Json,
        }
    }
}

fn parse_compile_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options::default();
    let mut positionals: Vec<String> = Vec::new();
    let mut only_positional = false;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if only_positional {
            positionals.push(arg.clone());
        } else {
            match arg.as_str() {
                "--" => only_positional = true,
                "--strict" => options.strict = true,
                "--layout" => options.layout = true,
                "--logic" => options.logic = true,
                "--bp" => options.bp = true,
                "--output" => {
                    index += 1;
                    let raw = args
                        .get(index)
                        .ok_or_else(|| "--output requires a value (json|mermaid|text)".to_string())?;
                    options.output = OutputFormat::parse(raw)?;
                }
                value if value.starts_with("--output=") => {
                    options.output =
                        OutputFormat::parse(&value["--output=".len()..])?;
                }
                "--help" | "-h" => {
                    print_compile_help();
                    std::process::exit(EXIT_OK.into());
                }
                flag if flag.starts_with('-') => {
                    return Err(format!("unknown option {flag:?}"));
                }
                _ => positionals.push(arg.clone()),
            }
        }
        index += 1;
    }

    match positionals.len() {
        0 => Err("missing required argument <PROJECT.mycproj>".to_string()),
        1 => {
            options.input = Some(PathBuf::from(&positionals[0]));
            Ok(options)
        }
        _ => Err(format!(
            "unexpected extra arguments: {}",
            positionals[1..].join(" ")
        )),
    }
}

fn print_compile_help() {
    println!(
        "canvas compile [OPTIONS] <PROJECT.mycproj>\n\
         \n\
         Compile a .mycproj project and print the compile report.\n\
         \n\
         OPTIONS:\n\
         \x20   --strict            Fail (exit 1) on any diagnostic, including warnings\n\
         \x20   --layout            Compute deterministic layout (GC-13)\n\
         \x20   --logic             Include logic chains + contradiction chains analysis\n\
         \x20   --bp                Include dual-channel belief propagation results\n\
         \x20   --output <fmt>      Report format: json (default), text, mermaid\n\
         \x20   -h, --help          Print this help\n\
         \n\
         REPORT (json):\n\
         \x20   diagnostics           Invariant violations (code/severity/entity/message)\n\
         \x20   hashes                blockHashes, contentRootHash, fileHash, verified\n\
         \x20   logicChains           Factor graph (--logic)\n\
         \x20   contradictionChains   Structural contradiction witnesses (--logic)\n\
         \x20   bp                    Dual-channel belief states (--bp)\n\
         \x20   layout                Deterministic positions (--layout)"
    );
}

// ---------------------------------------------------------------------------
// 编译与报告 / Compile & report
// ---------------------------------------------------------------------------

/// 项目摘要：只提取报告所需字段，不携带完整项目。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    id: Option<String>,
    title: Option<String>,
    schema_version: Option<u64>,
    revision: Option<u64>,
    counts: EntityCounts,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EntityCounts {
    nodes: usize,
    edges: usize,
    evidence: usize,
    placements: usize,
    scenarios: usize,
}

fn project_summary(project: &Value) -> ProjectSummary {
    let count = |key: &str| {
        project
            .get(key)
            .and_then(Value::as_array)
            .map(Vec::len)
            .unwrap_or(0)
    };
    ProjectSummary {
        id: project.get("id").and_then(Value::as_str).map(str::to_string),
        title: project
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        schema_version: project.get("schemaVersion").and_then(Value::as_u64),
        revision: project.get("revision").and_then(Value::as_u64),
        counts: EntityCounts {
            nodes: count("nodes"),
            edges: count("edges"),
            evidence: count("evidence"),
            placements: count("placements"),
            scenarios: count("scenarios"),
        },
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HashSummary {
    /// entityId → blockHash(12 hex)，按 id 排序保证 JSON 输出确定。
    block_hashes: BTreeMap<String, String>,
    content_root_hash: String,
    file_hash: String,
    verified: bool,
    verify_mismatches: Vec<String>,
}

fn hash_summary(compiled: &CompileResult) -> HashSummary {
    let verification = verify_hashes(&compiled.project);
    HashSummary {
        block_hashes: compiled
            .block_hashes
            .iter()
            .map(|(id, hash)| (id.clone(), hash.clone()))
            .collect(),
        content_root_hash: compiled.content_root_hash.clone(),
        file_hash: compiled.file_hash.clone(),
        verified: verification.valid,
        verify_mismatches: verification.mismatches,
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OptionsEcho {
    strict: bool,
    layout: bool,
    logic: bool,
    bp: bool,
    output: String,
}

/// 编译报告（稳定契约，camelCase，与内核 `CompileResult` 字段一致）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CompileReport {
    cli_version: String,
    input: String,
    ok: bool,
    options: OptionsEcho,
    project: ProjectSummary,
    diagnostics: Vec<InvariantViolation>,
    hashes: HashSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    logic_chains: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    contradiction_chains: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bp: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    layout: Option<Value>,
}

fn run_compile(args: &[String]) -> Result<u8, String> {
    let options = parse_compile_args(args)?;
    let input = options
        .input
        .clone()
        .expect("parse_compile_args guarantees an input path");
    let bytes = match std::fs::read(&input) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("canvas compile: cannot read {}: {error}", input.display());
            return Ok(EXIT_IO);
        }
    };

    let kernel_options = CompileOptions {
        strict_schema: options.strict,
        compute_layouts: options.layout,
    };
    match compile_project_with_options(&bytes, &kernel_options) {
        Ok(compiled) => {
            let report = build_report(&options, &input, &compiled);
            let fails = report_has_failures(&options, &report.diagnostics);
            render_report(&options, &report, Some(&compiled));
            Ok(if fails { EXIT_DIAGNOSTICS } else { EXIT_OK })
        }
        Err(failure) => {
            let diagnostics = vec![parse_failure_diagnostic(&failure)];
            let fails = report_has_failures(&options, &diagnostics);
            let report = failure_report(&options, &input, &diagnostics);
            render_report(&options, &report, None);
            Ok(if fails { EXIT_DIAGNOSTICS } else { EXIT_OK })
        }
    }
}

fn parse_failure_diagnostic(failure: &CompileFailure) -> InvariantViolation {
    InvariantViolation {
        code: "parse-error".to_string(),
        severity: Severity::Error,
        entity: "project".to_string(),
        message: failure.to_string(),
    }
}

fn report_has_failures(options: &Options, diagnostics: &[InvariantViolation]) -> bool {
    let any_error = diagnostics
        .iter()
        .any(|violation| violation.severity == Severity::Error);
    any_error || (options.strict && !diagnostics.is_empty())
}

/// 解析失败时的报告：除诊断外无哈希与项目摘要。
fn failure_report(options: &Options, input: &Path, diagnostics: &[InvariantViolation]) -> CompileReport {
    CompileReport {
        cli_version: VERSION.to_string(),
        input: input.display().to_string(),
        ok: false,
        options: options_echo(options),
        project: ProjectSummary {
            id: None,
            title: None,
            schema_version: None,
            revision: None,
            counts: EntityCounts {
                nodes: 0,
                edges: 0,
                evidence: 0,
                placements: 0,
                scenarios: 0,
            },
        },
        diagnostics: diagnostics.to_vec(),
        hashes: HashSummary {
            block_hashes: BTreeMap::new(),
            content_root_hash: String::new(),
            file_hash: String::new(),
            verified: false,
            verify_mismatches: Vec::new(),
        },
        logic_chains: None,
        contradiction_chains: None,
        bp: None,
        layout: None,
    }
}

fn options_echo(options: &Options) -> OptionsEcho {
    OptionsEcho {
        strict: options.strict,
        layout: options.layout,
        logic: options.logic,
        bp: options.bp,
        output: options.output.name().to_string(),
    }
}

fn build_report(options: &Options, input: &Path, compiled: &CompileResult) -> CompileReport {
    let mut logic_chains = None;
    let mut contradiction_chains = None;
    let mut bp = None;
    let mut layout = None;

    if options.logic {
        let factor_graph = research_graph_compiler::factor::compile::compile_factor_graph(
            &compiled.project,
        );
        logic_chains = serde_json::to_value(&factor_graph).ok();
        let contradictions =
            research_graph_compiler::factor::contradiction::find_contradictions(
                &compiled.project,
                &research_graph_compiler::factor::contradiction::ContradictionOptions {
                    max_depth: DEFAULT_MAX_DEPTH,
                    ..Default::default()
                },
            );
        contradiction_chains = serde_json::to_value(&contradictions).ok();
    }
    if options.bp {
        let factor_graph = research_graph_compiler::factor::compile::compile_factor_graph(
            &compiled.project,
        );
        let beliefs = research_graph_compiler::factor::bp::belief_propagation(
            &factor_graph,
            &research_graph_compiler::factor::bp::BpOptions::default(),
        );
        bp = serde_json::to_value(&beliefs).ok();
    }
    if options.layout {
        let positions = research_graph_compiler::layout::compute_layout(
            &compiled.project,
            research_graph_compiler::layout::LayoutMode::Hierarchical,
        );
        layout = serde_json::to_value(&positions).ok();
    }

    CompileReport {
        cli_version: VERSION.to_string(),
        input: input.display().to_string(),
        ok: compiled.violations.iter().all(|violation| {
            violation.severity != Severity::Error
        }),
        options: options_echo(options),
        project: project_summary(&compiled.project),
        diagnostics: compiled.violations.clone(),
        hashes: hash_summary(compiled),
        logic_chains,
        contradiction_chains,
        bp,
        layout,
    }
}

// ---------------------------------------------------------------------------
// 渲染 / Rendering
// ---------------------------------------------------------------------------

fn render_report(options: &Options, report: &CompileReport, compiled: Option<&CompileResult>) {
    match options.output {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(report).unwrap());
        }
        OutputFormat::Text => print_text_report(report),
        OutputFormat::Mermaid => {
            if let Some(compiled) = compiled {
                print!("{}", export_mermaid(&compiled.project));
            }
            // 解析失败时 mermaid 无图可导，诊断已由调用方输出到报告；这里不再输出。
        }
    }
}

fn print_text_report(report: &CompileReport) {
    println!(
        "canvas compile v{} — {}\nproject: {} — {}  (schemaVersion={}, revision={}; nodes={}, edges={}, evidence={}, placements={}, scenarios={})\nok: {}\n",
        report.cli_version,
        report.input,
        report.project.id.as_deref().unwrap_or("-"),
        report.project.title.as_deref().unwrap_or("-"),
        report
            .project
            .schema_version
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string()),
        report
            .project
            .revision
            .map(|v| v.to_string())
            .unwrap_or_else(|| "-".to_string()),
        report.project.counts.nodes,
        report.project.counts.edges,
        report.project.counts.evidence,
        report.project.counts.placements,
        report.project.counts.scenarios,
        report.ok,
    );

    println!("diagnostics ({}):", report.diagnostics.len());
    for violation in &report.diagnostics {
        println!(
            "  [{}] {} {} — {}",
            severity_name(violation.severity),
            violation.entity,
            violation.code,
            violation.message
        );
    }

    println!("\nhashes:");
    println!(
        "  contentRootHash  {}",
        report.hashes.content_root_hash
    );
    println!("  fileHash         {}", report.hashes.file_hash);
    println!("  blockHashes:");
    for (id, hash) in &report.hashes.block_hashes {
        println!("    {id}  {hash}");
    }
    println!(
        "  verified: {} ({} mismatches)",
        report.hashes.verified,
        report.hashes.verify_mismatches.len()
    );
    for mismatch in &report.hashes.verify_mismatches {
        println!("    ! {mismatch}");
    }

    if let Some(logic_chains) = &report.logic_chains {
        println!("\nlogic chains (--logic):");
        println!(
            "  variables: {}  factors: {}",
            logic_chains
                .get("variables")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
            logic_chains
                .get("factors")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0)
        );
    }
    if let Some(contradictions) = &report.contradiction_chains {
        println!("\ncontradiction chains (--logic):");
        println!(
            "  witnesses: {}  truncated: {}",
            contradictions
                .get("witnesses")
                .and_then(Value::as_array)
                .map(Vec::len)
                .unwrap_or(0),
            contradictions
                .get("truncated")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        );
    }
    if let Some(bp) = &report.bp {
        println!("\nbp (--bp):");
        if let Some(states) = bp.as_array() {
            for (index, state) in states.iter().enumerate() {
                let support = state.get("support").and_then(Value::as_f64).unwrap_or(0.0);
                let refutation = state
                    .get("refutation")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let net = state.get("netBelief").and_then(Value::as_f64).unwrap_or(0.0);
                let conflict = state.get("conflict").and_then(Value::as_f64).unwrap_or(0.0);
                println!(
                    "  #{index}: support={support:.4} refutation={refutation:.4} netBelief={net:.4} conflict={conflict:.4}"
                );
            }
        }
    }
    if let Some(layout) = &report.layout {
        println!("\nlayout (--layout):");
        if let Some(positions) = layout.as_object() {
            if positions.is_empty() {
                println!("  (empty — GC-13 骨架)");
            }
            for (id, position) in positions {
                let x = position.get("x").and_then(Value::as_f64).unwrap_or(0.0);
                let y = position.get("y").and_then(Value::as_f64).unwrap_or(0.0);
                println!("    {id}  ({x:.2}, {y:.2})");
            }
        }
    }
}

fn severity_name(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

// ---------------------------------------------------------------------------
// 单元测试 / Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Options, String> {
        let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
        parse_compile_args(&owned)
    }

    #[test]
    fn parse_defaults() {
        let options = parse(&["project.mycproj"]).unwrap();
        assert_eq!(options.input, Some(PathBuf::from("project.mycproj")));
        assert!(!options.strict && !options.layout && !options.logic && !options.bp);
        assert_eq!(options.output, OutputFormat::Json);
    }

    #[test]
    fn parse_all_flags() {
        let options = parse(&[
            "--strict",
            "--layout",
            "--logic",
            "--bp",
            "--output",
            "mermaid",
            "p.mycproj",
        ])
        .unwrap();
        assert!(options.strict && options.layout && options.logic && options.bp);
        assert_eq!(options.output, OutputFormat::Mermaid);
    }

    #[test]
    fn parse_output_equals_form() {
        let options = parse(&["--output=text", "p.mycproj"]).unwrap();
        assert_eq!(options.output, OutputFormat::Text);
    }

    #[test]
    fn parse_double_dash_stops_flag_parsing() {
        let options = parse(&["--", "--strict.mycproj"]).unwrap();
        assert_eq!(options.input, Some(PathBuf::from("--strict.mycproj")));
        assert!(!options.strict);
    }

    #[test]
    fn parse_rejects_unknown_flag() {
        assert!(parse(&["--fancy", "p.mycproj"]).is_err());
        assert!(parse(&["p.mycproj", "--fancy"]).is_err());
    }

    #[test]
    fn parse_rejects_missing_input() {
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn parse_rejects_extra_positionals() {
        assert!(parse(&["a.mycproj", "b.mycproj"]).is_err());
    }

    #[test]
    fn parse_rejects_bad_output_value() {
        assert!(parse(&["--output", "xml", "p.mycproj"]).is_err());
        assert!(parse(&["--output", "p.mycproj"]).is_err());
    }

    #[test]
    fn output_format_parsing() {
        assert_eq!(OutputFormat::parse("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::parse("text").unwrap(), OutputFormat::Text);
        assert_eq!(OutputFormat::parse("mermaid").unwrap(), OutputFormat::Mermaid);
        assert!(OutputFormat::parse("JSON").is_err());
    }
}
