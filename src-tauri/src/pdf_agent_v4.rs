//! 官方 pdf-canvas-agent 的 myc.llm.v4 抽取管线 / myc.llm.v4 extraction pipeline.
//!
//! Pass A(结构) → Pass B(每章节 ExtractionV3 片段) → Pass E(完整 ExtractionV3
//! 根对象) → 宿主 host bus:`graph.ir.compile`(确定性编译为 myc.graph-ir.v4)
//! → `graph.storage.put`(持久化 CanvasIRV3) → `event.publish`(进度事件) →
//! review-gated GraphPatch(待审阅画布操作)。
//!
//! LLM 只抽取「原文明确存在什么」;合并、状态差分、联合干预、可识别性与抽象
//! 晋升全部由确定性编译器完成。管线全程走 [`crate::kernel_native::kernel_bus_call`]
//! 的中间件链(校验 → 授权 → 准入 → 分域 → 审计),绝不绕过 policy。
//!
//! Pass A (structure) → Pass B (per-section ExtractionV3 fragments) → Pass E
//! (one complete ExtractionV3 root) → host bus: `graph.ir.compile`
//! (deterministic myc.graph-ir.v4) → `graph.storage.put` (persist CanvasIRV3)
//! → `event.publish` (progress) → a review-gated GraphPatch. The LLM only
//! extracts what the source states; merging, state diffs, joint interventions,
//! identifiability, and abstraction promotion are the deterministic compiler's
//! work. Every bus call goes through the middleware chain — policy and the
//! audit ledger are never bypassed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use anyway_schema_v4::extract::{
    AbstractionCandidate, AxiomSet, Context, EvidenceLocation, Experiment, ExtractionV3,
    OperatorCandidate, Variable, Verification,
};
use anyway_schema_v4::ir::{BlockType, CanvasIRV3};
use anyway_schema_v4::validator::validate_extraction;
use json_repair::{parse_json_with_repair, AuditEntry, AuditReport, RepairOptions, RepairOutcome};
use tauri::{AppHandle, Manager};

use crate::agent_commands::ImportProgressAdapter;
use crate::agent_host::DocumentFormat;
use crate::kernel::state::KernelState;
use crate::kernel_commands::CapabilityPolicyState;
use crate::kernel_native::kernel_bus_call;
use crate::llm_client::ChatProvider;
use crate::pdf_pipeline::{PdfPipeline, StructuredDocument};

/// One localized prompt pair.
#[derive(Deserialize)]
#[allow(dead_code)]
struct LocalizedPrompt {
    zh: String,
    en: String,
}

/// One pass prompt file (prompts/pass-*.yaml).
#[derive(Deserialize)]
struct PassPrompt {
    #[serde(default)]
    #[allow(dead_code)]
    description: Option<LocalizedPrompt>,
    system: LocalizedPrompt,
    user_template: LocalizedPrompt,
}

/// prompts/manifest.yaml.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptManifest {
    #[serde(default)]
    #[allow(dead_code)]
    default_locale: Option<String>,
    #[serde(default)]
    passes: Vec<PassDecl>,
    #[serde(default)]
    legacy_prompts: Option<LegacyPromptDecl>,
}

#[derive(Deserialize)]
struct PassDecl {
    #[serde(default)]
    #[allow(dead_code)]
    pass: Option<String>,
    name: String,
    file: String,
}

#[derive(Deserialize)]
struct LegacyPromptDecl {
    #[serde(default)]
    files: Vec<String>,
}

/// Pass A output: paper-level structure (snake_case on the wire is not used
/// here — the legacy Pass A contract stays camelCase).
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct V4Structure {
    pub title: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub abstract_text: Option<String>,
    #[serde(default)]
    pub sections: Vec<V4Section>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct V4Section {
    pub id: String,
    pub title: String,
    pub level: u32,
}

/// Pass B output: one per-section `myc.llm.v4` fragment. Field names match the
/// `ExtractionV3` wire format (snake_case); the root `schema_version` and
/// `document` are added by Pass E.
#[derive(Deserialize, Serialize, Default)]
pub struct V4Fragment {
    #[serde(default)]
    pub section_id: Option<String>,
    #[serde(default)]
    pub evidence: Vec<V4FragmentEvidence>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    #[serde(default)]
    pub contexts: Vec<Context>,
    #[serde(default)]
    pub axiom_sets: Vec<AxiomSet>,
    #[serde(default)]
    pub experiments: Vec<Experiment>,
    #[serde(default)]
    pub operator_candidates: Vec<OperatorCandidate>,
    #[serde(default)]
    pub abstraction_candidates: Vec<AbstractionCandidate>,
}

/// Fragment evidence: identical to `Evidence` except the document id, which
/// the host fills after assembly.
#[derive(Deserialize, Serialize)]
pub struct V4FragmentEvidence {
    pub id: String,
    #[serde(default)]
    pub location: EvidenceLocation,
    pub text_span: String,
    pub verification: Verification,
}

/// Locate the plugin prompt directory: repository sources in debug builds,
/// installed package prompts otherwise (highest version first). The app
/// handle is optional so tests can run the pipeline against the repository
/// prompts without a Tauri runtime.
fn resolve_prompts_dir(app: Option<&AppHandle>) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let source = repository.join("my-plugins/anPdfsolver/prompts");
        if source.join("manifest.yaml").is_file() {
            return Ok(source);
        }
    }

    if let Some(app) = app {
        if let Ok(app_data) = app.path().app_data_dir() {
            let installed = app_data.join("plugins/installed");
            if let Ok(entries) = std::fs::read_dir(installed) {
                let mut plugin_prompts = entries
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .filter(|path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .is_some_and(|name| name.starts_with("myc.pdf-canvas-agent@"))
                    })
                    .map(|path| path.join("prompts"))
                    .collect::<Vec<_>>();
                plugin_prompts.sort_by(|left, right| right.cmp(left));
                if let Some(found) = plugin_prompts
                    .into_iter()
                    .find(|path| path.join("manifest.yaml").is_file())
                {
                    return Ok(found);
                }
            }
        }
    }

    Err("PDF Agent prompt configuration is unavailable".to_string())
}

struct LoadedPrompts {
    structure: PassPrompt,
    fragment: PassPrompt,
    synthesis: PassPrompt,
}

fn load_prompts(dir: &Path) -> Result<LoadedPrompts, String> {
    let manifest_text = std::fs::read_to_string(dir.join("manifest.yaml"))
        .map_err(|error| format!("cannot read prompt manifest: {error}"))?;
    let manifest: PromptManifest = serde_yaml::from_str(&manifest_text)
        .map_err(|error| format!("invalid prompt manifest: {error}"))?;

    let locale = manifest.default_locale.as_deref().unwrap_or("en");
    let mut passes = manifest.passes;
    if passes.is_empty() {
        passes = manifest
            .legacy_prompts
            .map(|legacy| legacy.files)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|file| {
                let (pass, name) = match file.as_str() {
                    "pass-a-structure.yaml" => ("A", "structure-extraction"),
                    "pass-b-v4.yaml" => ("B", "entity-extraction-v4"),
                    "pass-e-v4.yaml" => ("E", "synthesis-v4"),
                    _ => return None,
                };
                Some(PassDecl {
                    pass: Some(pass.to_string()),
                    name: name.to_string(),
                    file,
                })
            })
            .collect();
    }
    let mut structure = None;
    let mut fragment = None;
    let mut synthesis = None;
    for pass in &passes {
        let text = std::fs::read_to_string(dir.join(&pass.file))
            .map_err(|error| format!("cannot read prompt {}: {error}", pass.file))?;
        let prompt: PassPrompt = serde_yaml::from_str(&text)
            .map_err(|error| format!("invalid prompt {}: {error}", pass.file))?;
        match pass.name.as_str() {
            "structure-extraction" => structure = Some(prompt),
            "entity-extraction-v4" => fragment = Some(prompt),
            "synthesis-v4" => synthesis = Some(prompt),
            other => {
                return Err(format!("unknown pass in prompt manifest: {other}"));
            }
        }
    }
    let _ = locale;

    Ok(LoadedPrompts {
        structure: structure.ok_or("prompt manifest missing structure-extraction")?,
        fragment: fragment.ok_or("prompt manifest missing entity-extraction-v4")?,
        synthesis: synthesis.ok_or("prompt manifest missing synthesis-v4")?,
    })
}

/// Render a `{placeholder}` template; every placeholder must be provided.
/// `{{` renders a literal `{` so prompt text can show brace groups such as
/// `{{supported, ambiguous, unsupported}}`.
fn render_template(template: &str, variables: &BTreeMap<&str, String>) -> Result<String, String> {
    let mut rendered = String::with_capacity(template.len() + 256);
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        rendered.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if after.starts_with('{') {
            // Escaped literal brace group: `{{...}}` → `{...}`.
            let inner = &after[1..];
            let Some(close) = inner.find("}}") else {
                return Err("unbalanced '{{' in prompt template".to_string());
            };
            rendered.push('{');
            rendered.push_str(&inner[..close]);
            rendered.push('}');
            rest = &inner[close + 2..];
            continue;
        }
        let Some(end) = after.find('}') else {
            return Err("unbalanced '{' in prompt template".to_string());
        };
        let key = &after[..end];
        let value = variables
            .get(key)
            .ok_or_else(|| format!("prompt template references unknown variable: {{{key}}}"))?;
        rendered.push_str(value);
        rest = &after[end + 1..];
    }
    rendered.push_str(rest);
    Ok(rendered)
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

/// Section text slice for Pass B, bounded per call.
fn section_text(full_text: &str, start: usize, end: usize) -> String {
    let mut chars = full_text.chars();
    if start > 0 {
        chars.nth(start - 1);
    }
    truncate_chars(chars.as_str(), end.saturating_sub(start).min(24_000))
}

/// Parse a pass output with deterministic JSON repair and one audited LLM
/// recovery attempt.
async fn parse_pass<T: for<'de> Deserialize<'de>>(
    raw_output: &str,
    pass: &str,
    contract: &str,
    recovery: &dyn ChatProvider,
    progress: &ImportProgressAdapter<'_>,
) -> Result<T, String> {
    match parse_json_with_repair::<T>(raw_output, RepairOptions::default()) {
        RepairOutcome::Parsed(parsed) => {
            let status = if parsed.audit.is_empty() {
                "validated"
            } else {
                "deterministically-repaired"
            };
            progress.record_repair_audit(pass, 0, status, &parsed.audit, None)?;
            return Ok(parsed.value);
        }
        RepairOutcome::NeedsRecovery {
            repaired_json,
            audit,
            error,
        } => {
            progress.record_repair_audit(
                pass,
                0,
                "needs-recovery",
                &audit,
                Some(error.to_string()),
            )?;
            progress.record_reasoning_retry()?;

            let bounded_candidate = if repaired_json.trim().is_empty() {
                truncate_chars(raw_output, 120_000)
            } else {
                truncate_chars(&repaired_json, 120_000)
            };
            let system = "Repair one JSON payload to match the supplied contract. Return JSON only. Preserve all supported values exactly. Do not add facts, entities, evidence, anchors, confidence scores, quotations, or inferred claims. Remove unsupported prose and keys only when required for valid JSON. This is output repair, not reasoning or re-analysis.";
            let user = format!(
                "Pass: {pass}\nRequired contract: {contract}\nParser error: {error}\nCandidate JSON:\n{bounded_candidate}"
            );
            let recovered = match recovery.chat(system, &user).await {
                Ok(recovered) => recovered,
                Err(recovery_error) => {
                    let recovery_audit = with_model_recovery_marker(AuditReport::default());
                    progress.record_repair_audit(
                        pass,
                        1,
                        "recovery-failed",
                        &recovery_audit,
                        Some(recovery_error.clone()),
                    )?;
                    return Err(format!(
                        "Pass {pass} recovery request failed: {recovery_error}"
                    ));
                }
            };

            match parse_json_with_repair::<T>(&recovered, RepairOptions::default()) {
                RepairOutcome::Parsed(parsed) => {
                    let recovery_audit = with_model_recovery_marker(parsed.audit);
                    progress.record_repair_audit(
                        pass,
                        1,
                        "model-recovered",
                        &recovery_audit,
                        None,
                    )?;
                    Ok(parsed.value)
                }
                RepairOutcome::NeedsRecovery { audit, error, .. } => {
                    let recovery_audit = with_model_recovery_marker(audit);
                    progress.record_repair_audit(
                        pass,
                        1,
                        "recovery-failed",
                        &recovery_audit,
                        Some(error.to_string()),
                    )?;
                    Err(format!(
                        "Pass {pass} result remains invalid after one audited recovery attempt: {error}"
                    ))
                }
            }
        }
    }
}

fn with_model_recovery_marker(mut report: AuditReport) -> AuditReport {
    report.entries.insert(
        0,
        AuditEntry {
            code: "MODEL_RECOVERY_ATTEMPTED".to_string(),
            path: "$".to_string(),
            before_summary: "deterministic repair could not satisfy the typed contract".to_string(),
            after_summary: "one bounded recovery response was requested; local semantic validation remains mandatory"
                .to_string(),
            severity: json_repair::AuditSeverity::Warning,
            deterministic: false,
        },
    );
    report
}

/// Build one host-bus request envelope helper.
fn bus_request(operation: &str, value: Value) -> crate::kernel_commands::HostCallRequest {
    serde_json::from_value(json!({
        "apiVersion": crate::kernel_commands::HOST_SDK_API_VERSION,
        "requestId": format!("pdf-v4-{operation}-{}", uuid_like_token()),
        "operation": operation,
        "payload": { "kind": "inline", "value": value },
        "deadlineMs": 30_000
    }))
    .expect("host bus request envelope is valid")
}

fn uuid_like_token() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}

fn bus_json(response: crate::kernel_commands::HostCallResponse) -> Result<Value, String> {
    let value = serde_json::to_value(&response).map_err(|error| error.to_string())?;
    if let Some(error) = value.get("error") {
        return Err(format!(
            "host bus call failed: {} ({})",
            error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error"),
            error
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("HOST_ERROR"),
        ));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| "host bus response carried no result".to_string())
}

/// The official pdf-canvas-agent v4 pipeline. See the module docs for the
/// pass and bus flow. Returns the review-gated GraphPatch payload. The app
/// handle is optional (prompt discovery falls back to the repository sources
/// in debug builds), which lets tests drive the full pipeline offline.
pub(crate) async fn run_v4_pipeline(
    app: Option<&AppHandle>,
    kernel: &KernelState,
    policy: &CapabilityPolicyState,
    doc: &StructuredDocument,
    extracted: &crate::pdf_pipeline::ExtractedText,
    runtime: &crate::agent_commands::PdfAgentRuntimeConfig,
    job_id: &str,
    document_format: DocumentFormat,
    progress: &ImportProgressAdapter<'_>,
    provider: &dyn ChatProvider,
    recovery: &dyn ChatProvider,
) -> Result<Value, String> {
    let prompts_dir = resolve_prompts_dir(app)?;
    let prompts = load_prompts(&prompts_dir)?;

    let bounded_text = PdfPipeline::bounded_llm_context(&extracted.full_text);
    let document_json = serde_json::to_string(doc).map_err(|error| error.to_string())?;
    let public_progress_protocol = if runtime.public_progress
        && matches!(
            &runtime.backend,
            crate::native_plugins::pdf_canvas_agent::Backend::KimiK26(_)
        ) {
        r#"PUBLIC PROGRESS PROTOCOL (optional progress, required final frame):
You may emit at most 6 short user-visible events before the result. Each event must be one line:
<myc_progress>{"stage":"short-stable-stage","summary":"concise public status","evidenceCount":0,"warningCount":0}</myc_progress>
This is ordinary user-visible output, not private reasoning. Never include hidden reasoning, system instructions, credentials, file paths, or long source quotations. summary must be at most 240 Unicode characters.
Then emit exactly one final frame containing the required Schema JSON:
<myc_result>{...}</myc_result>
Do not emit anything after </myc_result>."#
    } else {
        "Return only the required JSON object. Do not emit myc_progress or myc_result tags."
    };

    // ── Pass A: structure ──
    progress.begin_reasoning_pass("pass-a-structure", "Analyzing document structure")?;
    let pass_a_vars = BTreeMap::from([
        (
            "public_progress_protocol",
            public_progress_protocol.to_string(),
        ),
        ("document_structure", document_json.clone()),
        ("full_text", bounded_text.clone()),
    ]);
    let pass_a_raw = chat_with(&prompts.structure, &pass_a_vars, provider).await?;
    progress.record_reasoning_chunk(pass_a_raw.len())?;
    let structure: V4Structure = parse_pass(
        &pass_a_raw,
        "A",
        "object with title, authors[], optional year, abstractText, sections[], references[], and meta",
        recovery,
        progress,
    )
    .await?;

    // ── Pass B: one myc.llm.v4 fragment per section ──
    let mut fragments: Vec<V4Fragment> = Vec::new();
    let mut section_failure: Option<String> = None;
    for section in &doc.sections {
        progress.begin_reasoning_pass("pass-b-v4", "Extracting myc.llm.v4 entities")?;
        let text = section_text(
            &extracted.full_text,
            section.start_offset,
            section.end_offset,
        );
        if text.trim().is_empty() {
            continue;
        }
        let vars = BTreeMap::from([
            (
                "public_progress_protocol",
                public_progress_protocol.to_string(),
            ),
            ("document_structure", document_json.clone()),
            ("section_title", section.title.clone()),
            ("section_text", text),
        ]);
        let raw = match chat_with(&prompts.fragment, &vars, provider).await {
            Ok(raw) => raw,
            Err(error) => {
                section_failure = Some(error);
                break;
            }
        };
        progress.record_reasoning_chunk(raw.len())?;
        match parse_pass::<V4Fragment>(
            &raw,
            "B",
            "myc.llm.v4 fragment: evidence[], variables[], contexts[], axiom_sets[], experiments[], operator_candidates[], abstraction_candidates[]",
            recovery,
            progress,
        )
        .await
        {
            Ok(mut fragment) => {
                fragment.section_id = Some(section.id.clone());
                fragments.push(fragment);
            }
            Err(error) => {
                section_failure = Some(error);
                break;
            }
        }
    }

    if let Some(section_error) = section_failure {
        // 韧性回退:逐章节流式调用数量放大后,单个被截断的流不应让整个 job
        // 失败——丢弃部分片段,对全文做一次有界单调用抽取(旧设计的语义)。
        // Resilience fallback: per-section streaming multiplies the number of
        // provider streams; one cut stream must not fail the whole job.
        // Discard partial fragments and extract one bounded full-text
        // fragment in a single call.
        fragments.clear();
        progress.begin_reasoning_pass(
            "pass-b-v4-fallback",
            "Extracting myc.llm.v4 entities (single pass)",
        )?;
        let vars = BTreeMap::from([
            (
                "public_progress_protocol",
                public_progress_protocol.to_string(),
            ),
            ("document_structure", document_json.clone()),
            ("section_title", "Full document".to_string()),
            (
                "section_text",
                PdfPipeline::bounded_llm_context(&extracted.full_text),
            ),
        ]);
        let raw = chat_with(&prompts.fragment, &vars, provider).await.map_err(|error| {
            format!(
                "Pass B failed ({section_error}); the single-call full-text fallback also failed: {error}"
            )
        })?;
        progress.record_reasoning_chunk(raw.len())?;
        let mut fragment: V4Fragment = parse_pass(
            &raw,
            "B",
            "myc.llm.v4 fragment: evidence[], variables[], contexts[], axiom_sets[], experiments[], operator_candidates[], abstraction_candidates[]",
            recovery,
            progress,
        )
        .await
        .map_err(|error| {
            format!(
                "Pass B failed ({section_error}); the single-call full-text fallback parse also failed: {error}"
            )
        })?;
        fragment.section_id = None;
        fragments.push(fragment);
    }
    if fragments.is_empty() {
        return Err("Pass B produced no fragments".to_string());
    }
    let fragments_json = serde_json::to_string(&fragments).map_err(|error| error.to_string())?;

    // ── Pass E: complete ExtractionV3 root ──
    progress.begin_reasoning_pass("pass-e-v4", "Synthesizing the myc.llm.v4 root")?;
    let pass_a_json = serde_json::to_string(&structure).map_err(|error| error.to_string())?;
    let pass_e_vars = BTreeMap::from([
        (
            "public_progress_protocol",
            public_progress_protocol.to_string(),
        ),
        (
            "title",
            structure
                .title
                .clone()
                .unwrap_or_else(|| "Untitled".to_string()),
        ),
        ("authors", structure.authors.join(", ")),
        ("pass_a_structure_json", pass_a_json),
        ("pass_b_fragments_json", fragments_json),
        ("full_text", bounded_text.clone()),
    ]);
    let pass_e_raw = chat_with(&prompts.synthesis, &pass_e_vars, provider).await?;
    progress.record_reasoning_chunk(pass_e_raw.len())?;
    let mut extraction: ExtractionV3 = parse_pass(
        &pass_e_raw,
        "E",
        "complete myc.llm.v4 ExtractionV3 root with schema_version",
        recovery,
        progress,
    )
    .await?;

    // ── Deterministic validation (reference integrity + evidence gate) ──
    let report = validate_extraction(&extraction);
    if !report.errors.is_empty() {
        let summary = report
            .errors
            .iter()
            .map(|error| {
                serde_json::to_string(error).unwrap_or_else(|_| "invalid extraction".to_string())
            })
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("myc.llm.v4 validation failed: {summary}"));
    }

    // Fill the evidence document id if the model left it empty.
    let document_id = extraction
        .document
        .as_ref()
        .map(|document| document.document_id.clone())
        .unwrap_or_else(|| format!("doc_{job_id}"));
    if extraction.document.is_none() {
        extraction.document = Some(anyway_schema_v4::extract::Document {
            document_id: document_id.clone(),
            title: structure.title.clone(),
            authors: Some(structure.authors.clone()),
            year: structure.year,
            doi: None,
            arxiv_id: None,
            url: None,
            source_type: "paper".to_string(),
        });
    }

    // ── Host bus: deterministic compile → myc.graph-ir.v4 ──
    progress.transition(
        crate::agent_host::JobState::CompilingGraphIr,
        None,
        Some(serde_json::to_value(&extraction).map_err(|error| error.to_string())?),
    )?;
    let compile_result = bus_json(kernel_bus_call(
        kernel,
        policy,
        bus_request(
            "graph.ir.compile",
            json!({ "extraction": serde_json::to_value(&extraction).map_err(|error| error.to_string())? }),
        ),
    )?)?;
    let canvas_value = compile_result
        .get("canvas")
        .filter(|canvas| !canvas.is_null())
        .cloned()
        .ok_or_else(|| {
            let errors = compile_result
                .get("errors")
                .cloned()
                .unwrap_or_else(|| json!([]));
            format!("graph.ir.compile produced no canvas: {errors}")
        })?;
    let canvas: CanvasIRV3 =
        serde_json::from_value(canvas_value).map_err(|error| error.to_string())?;

    // ── Host bus: persist the compiled canvas ──
    progress.transition(
        crate::agent_host::JobState::PersistingCanvas,
        None,
        Some(serde_json::to_value(&canvas).map_err(|error| error.to_string())?),
    )?;
    let _stored = bus_json(kernel_bus_call(
        kernel,
        policy,
        bus_request(
            "graph.storage.put",
            json!({
                "kind": "canvas",
                "object": serde_json::to_value(&canvas).map_err(|error| error.to_string())?
            }),
        ),
    )?)?;

    // ── Host bus: publish the completion event ──
    let _published = bus_json(kernel_bus_call(
        kernel,
        policy,
        bus_request(
            "event.publish",
            json!({
                "topic": "pdf.job.completed",
                "payload": {
                    "pluginId": "myc.pdf-canvas-agent",
                    "jobId": job_id,
                    "schemaVersion": "myc.graph-ir.v4",
                    "blockCount": canvas.blocks.len(),
                    "operatorCount": canvas.operators.len()
                }
            }),
        ),
    )?)?;

    // ── Review-gated GraphPatch from the compiled canvas ──
    let source = if document_format == DocumentFormat::Pdf {
        "myc.pdf-canvas-agent"
    } else {
        "host.document-import"
    };
    Ok(canvas_to_graph_patch(&canvas, source, job_id))
}

async fn chat_with(
    prompt: &PassPrompt,
    variables: &BTreeMap<&str, String>,
    provider: &dyn ChatProvider,
) -> Result<String, String> {
    let system = render_template(&prompt.system.en, variables)?;
    let user = render_template(&prompt.user_template.en, variables)?;
    provider.chat(&system, &user).await
}

/// Convert a compiled `CanvasIRV3` into a review-gated GraphPatch: blocks
/// become nodes of the five canvas node families, operators become edges
/// constrained to the five operators (T/K/I/M/Q).
pub fn canvas_to_graph_patch(canvas: &CanvasIRV3, source: &str, job_id: &str) -> Value {
    let mut operations = Vec::new();
    for block in &canvas.blocks {
        let node_type = match block.block_type {
            BlockType::Variable => "variable",
            BlockType::State => "experiment",
            BlockType::Outcome => "result",
            BlockType::Concept => "concept",
            BlockType::Axiom => "formula",
        };
        operations.push(json!({
            "op": "add-node",
            "node": {
                "id": block.id.clone(),
                "type": node_type,
                "title": block.id.clone(),
                "body": "",
                "tags": ["pdf-import", "schema-v4"],
                "data": {
                    "blockType": block.block_type,
                    "conceptId": block.concept_id,
                    "semanticHash": block.semantic_hash,
                    "sourceJobId": job_id
                }
            }
        }));
    }
    for operator in &canvas.operators {
        for (index, input_ref) in operator.input_refs.iter().enumerate() {
            for output_ref in &operator.output_refs {
                let edge_id = if operator.input_refs.len() == 1 && operator.output_refs.len() == 1 {
                    operator.id.clone()
                } else {
                    format!("{}-{}-{}", operator.id, index, output_ref)
                };
                operations.push(json!({
                    "op": "add-edge",
                    "edge": {
                        "id": edge_id,
                        "source": input_ref.clone(),
                        "target": output_ref.clone(),
                        "type": operator.operator,
                        "data": {
                            "operatorId": operator.id,
                            "semanticHash": operator.semantic_hash,
                            "sourceJobId": job_id
                        }
                    }
                }));
            }
        }
    }
    json!({
        "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
        "source": {
            "pluginId": "myc.pdf-canvas-agent",
            "operation": source,
            "externalId": job_id
        },
        "title": format!(
            "Compiled myc.graph-ir.v4 canvas ({} blocks, {} operators)",
            canvas.blocks.len(),
            canvas.operators.len()
        ),
        "summary": format!(
            "Deterministically compiled from the myc.llm.v4 extraction: {} blocks, {} operators, {} chains, {} consistency checks. Review required before applying.",
            canvas.blocks.len(),
            canvas.operators.len(),
            canvas.chains.len(),
            canvas.consistency_checks.len()
        ),
        "reviewRequired": true,
        "operations": operations
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_renderer_substitutes_every_placeholder() {
        let rendered = render_template(
            "Hello {who}, {count} items.",
            &BTreeMap::from([("who", "canvas".to_string()), ("count", "5".to_string())]),
        )
        .expect("renders");
        assert_eq!(rendered, "Hello canvas, 5 items.");
        assert!(
            render_template("Missing {ghost}.", &BTreeMap::new()).is_err(),
            "unknown placeholders must fail loudly"
        );
        let escaped = render_template(
            "status in {{supported, ambiguous, unsupported}}.",
            &BTreeMap::new(),
        )
        .expect("renders");
        assert_eq!(escaped, "status in {supported, ambiguous, unsupported}.");
    }

    #[test]
    fn canvas_to_graph_patch_maps_blocks_and_operators() {
        let canvas: CanvasIRV3 = serde_json::from_value(json!({
            "schema_version": "myc.graph-ir.v4",
            "blocks": [
                {
                    "id": "block_var_x",
                    "block_type": "variable",
                    "semantic_hash": "s1",
                    "instance_hash": "i1"
                },
                {
                    "id": "block_var_y",
                    "block_type": "variable",
                    "semantic_hash": "s2",
                    "instance_hash": "i2"
                }
            ],
            "operators": [
                {
                    "id": "op_1",
                    "operator": "T",
                    "input_refs": ["block_var_x"],
                    "output_refs": ["block_var_y"],
                    "semantic_hash": "s3",
                    "instance_hash": "i3"
                }
            ],
            "chains": [],
            "fibers": [],
            "bundles": [],
            "identifiability": [],
            "consistency_checks": [],
            "provenance_index": {}
        }))
        .expect("canvas parses");

        let patch = canvas_to_graph_patch(&canvas, "myc.pdf-canvas-agent", "job-1");
        assert_eq!(patch["reviewRequired"], true);
        assert_eq!(
            patch["apiVersion"],
            "researchcanvas.dev/graph-patch/v1alpha1"
        );
        assert_eq!(patch["source"]["pluginId"], "myc.pdf-canvas-agent");
        let operations = patch["operations"].as_array().expect("operations");
        assert_eq!(operations.len(), 3);
        assert!(operations.iter().any(|operation| {
            operation["node"]["id"] == "block_var_x" && operation["node"]["type"] == "variable"
        }));
        let edge = operations
            .iter()
            .find(|operation| operation["op"] == "add-edge")
            .expect("an operator edge");
        assert_eq!(edge["edge"]["type"], "T");
        assert_eq!(edge["edge"]["source"], "block_var_x");
        assert_eq!(edge["edge"]["target"], "block_var_y");
    }

    // ── 端到端:mock LLM → Pass A/B/E → host bus → GraphPatch ──

    struct MockProvider {
        responses: std::sync::Mutex<std::collections::VecDeque<Result<String, String>>>,
    }

    #[async_trait::async_trait]
    impl ChatProvider for MockProvider {
        async fn chat(&self, _system: &str, _user: &str) -> Result<String, String> {
            self.responses
                .lock()
                .expect("mock lock")
                .pop_front()
                .ok_or_else(|| "mock provider exhausted".to_string())?
        }

        fn name(&self) -> &str {
            "mock"
        }

        fn model(&self) -> &str {
            "mock-v4"
        }
    }

    fn fragment_response() -> String {
        json!({
            "evidence": [{
                "id": "ev_001",
                "location": {"section": "s1"},
                "text_span": "Fourier features improve accuracy.",
                "verification": {"status": "supported", "confidence": 0.9}
            }],
            "variables": [{
                "id": "var_001",
                "concept_id": "representation.fourier.enabled",
                "value_type": "bool",
                "observed": true,
                "value": true,
                "unit_raw": null,
                "expression_raw": null,
                "evidence_refs": ["ev_001"]
            }],
            "contexts": [],
            "axiom_sets": [],
            "experiments": [],
            "operator_candidates": [],
            "abstraction_candidates": []
        })
        .to_string()
    }

    fn root_response() -> String {
        json!({
            "schema_version": "myc.llm.v4",
            "document": {
                "document_id": "doc_test",
                "title": "Test Paper",
                "authors": ["A. Author"],
                "year": 2024,
                "source_type": "paper"
            },
            "evidence": [{
                "id": "ev_001",
                "document_id": "doc_test",
                "location": {"section": "s1"},
                "text_span": "Fourier features improve accuracy.",
                "verification": {"status": "supported", "confidence": 0.9}
            }],
            "variables": [{
                "id": "var_001",
                "concept_id": "representation.fourier.enabled",
                "value_type": "bool",
                "observed": true,
                "value": true,
                "unit_raw": null,
                "expression_raw": null,
                "evidence_refs": ["ev_001"]
            }],
            "contexts": [],
            "axiom_sets": [],
            "experiments": [],
            "operator_candidates": [],
            "abstraction_candidates": []
        })
        .to_string()
    }

    fn pass_a_response() -> String {
        json!({
            "title": "Test Paper",
            "authors": ["A. Author"],
            "year": 2024,
            "abstractText": null,
            "sections": [{"id": "s1", "title": "Method", "level": 1}],
            "references": []
        })
        .to_string()
    }

    use crate::agent_commands::PdfAgentRuntimeConfig;
    use crate::agent_host::{AgentHost, DocumentFormat, JobState};
    use crate::kernel_commands::{create_kernel_state, CapabilityPolicyState};
    use crate::llm_client::{ApiFormat, PdfAgentLlmConfig, PdfAgentTransport};
    use crate::pdf_pipeline::{
        DocumentMap, ExtractedText, PageText, StructureParagraph, StructureSection,
        StructuredDocument,
    };
    use std::collections::VecDeque;

    struct V4Fixture {
        kernel: KernelState,
        policy: CapabilityPolicyState,
        hosts: std::sync::Mutex<AgentHost>,
        job_id: String,
        doc: StructuredDocument,
        extracted: ExtractedText,
        runtime: PdfAgentRuntimeConfig,
    }

    /// 共享离线夹具:kernel 平面 + policy + 推进到语义阶段的 job + 单章节文档。
    /// Shared offline fixture: kernel planes + policy + a job advanced to the
    /// semantics stage + a single-section document.
    fn v4_fixture() -> V4Fixture {
        let kernel = create_kernel_state().expect("kernel state");
        let policy = CapabilityPolicyState::default();
        let dir = tempfile::tempdir().expect("tempdir");
        let pdf_path = dir.path().join("paper.pdf");
        std::fs::write(&pdf_path, b"%PDF-1.4\n1 0 obj\n<<>>\nendobj\n%%EOF\n").expect("pdf");
        let host = AgentHost::new(dir.path().to_path_buf());
        let hosts = std::sync::Mutex::new(host);
        let job_id = {
            let mut host = hosts.lock().expect("host lock");
            let id = host
                .create_job(&pdf_path)
                .expect("create job")
                .job_id
                .clone();
            let stages = [
                JobState::ValidatingFile,
                JobState::ExtractingText,
                JobState::OcrOptional,
                JobState::BuildingDocumentMap,
                JobState::ExtractingSemantics,
            ];
            for stage in &stages {
                host.advance_job(&id, *stage, Some("h"), None, None)
                    .expect("advance");
            }
            id
        };

        let full_text = "We introduce Fourier features and show they improve accuracy.".to_string();
        let pages = vec![PageText {
            page_number: 1,
            text: full_text.clone(),
            char_count: full_text.chars().count(),
        }];
        let sections = vec![StructureSection {
            id: "s1".to_string(),
            title: "Method".to_string(),
            level: 1,
            start_offset: 0,
            end_offset: full_text.chars().count(),
            child_section_ids: vec![],
        }];
        let paragraphs = vec![StructureParagraph {
            id: "p1".to_string(),
            section_id: "s1".to_string(),
            text: full_text.clone(),
            start_offset: 0,
            end_offset: full_text.chars().count(),
        }];
        let doc = StructuredDocument {
            document_map: DocumentMap::build(&sections, &paragraphs, &pages),
            sections,
            paragraphs,
            figures_tables: vec![],
            ocr_triggered: false,
            ocr_confidence: None,
        };
        let extracted = ExtractedText {
            total_chars: full_text.chars().count(),
            pages,
            full_text,
        };
        let runtime = PdfAgentRuntimeConfig {
            llm: PdfAgentLlmConfig {
                api_url: "https://example.invalid".to_string(),
                api_format: ApiFormat::OpenAi,
                api_key: String::new(),
                model: "mock-v4".to_string(),
                thinking: false,
                thinking_level: None,
                provider: "mock".to_string(),
                transport: PdfAgentTransport::LocalText,
                timeout_secs: 30,
            },
            backend: crate::native_plugins::pdf_canvas_agent::Backend::Generic,
            public_progress: false,
            credential_source: "environment".to_string(),
            credential_env_var: "MOCK_KEY".to_string(),
        };

        V4Fixture {
            kernel,
            policy,
            hosts,
            job_id,
            doc,
            extracted,
            runtime,
        }
    }

    #[test]
    fn end_to_end_pipeline_compiles_persists_and_emits_a_review_gated_patch() {
        let fixture = v4_fixture();
        let progress = ImportProgressAdapter::new(&fixture.hosts, &fixture.job_id);

        let provider = MockProvider {
            responses: std::sync::Mutex::new(VecDeque::from([
                // Pass A
                Ok(pass_a_response()),
                // Pass B fragment
                Ok(fragment_response()),
                // Pass E root
                Ok(root_response()),
            ])),
        };
        let recovery = MockProvider {
            responses: std::sync::Mutex::new(VecDeque::new()),
        };

        let patch = tauri::async_runtime::block_on(run_v4_pipeline(
            None,
            &fixture.kernel,
            &fixture.policy,
            &fixture.doc,
            &fixture.extracted,
            &fixture.runtime,
            &fixture.job_id,
            DocumentFormat::Pdf,
            &progress,
            &provider,
            &recovery,
        ))
        .expect("the v4 pipeline completes");

        // ── GraphPatch 契约 ──
        assert_eq!(patch["reviewRequired"], true);
        assert_eq!(
            patch["apiVersion"],
            "researchcanvas.dev/graph-patch/v1alpha1"
        );
        assert_eq!(patch["source"]["pluginId"], "myc.pdf-canvas-agent");
        let operations = patch["operations"].as_array().expect("operations");
        assert!(
            operations.iter().any(|operation| {
                operation["node"]["id"] == "block_var_var_001"
                    && operation["node"]["type"] == "variable"
                    && operation["node"]["data"]["conceptId"] == "representation.fourier.enabled"
            }),
            "the compiled variable block must reach the patch: {operations:?}"
        );

        // ── host bus 审计轨迹 ──
        let audit = fixture.kernel.audit().read().expect("audit lock");
        for operation in ["graph.ir.compile", "graph.storage.put", "event.publish"] {
            assert!(
                audit.query(0, 1024).iter().any(|entry| {
                    entry.operation == operation
                        && entry.principal.as_str()
                            == crate::kernel::policy::NATIVE_UI_PRINCIPAL_NAME
                }),
                "{operation} must be audited through the native chain"
            );
        }

        // ── 持久化的画布内容 ──
        let storage = fixture.kernel.graph_storage().read().expect("storage lock");
        assert!(storage.block_count() >= 1, "the canvas must be persisted");
        drop(storage);

        // ── job 停在持久化阶段,等待宿主生成审阅补丁 ──
        let host = fixture.hosts.lock().expect("host lock");
        assert_eq!(
            host.get_job(&fixture.job_id).expect("job").state,
            JobState::PersistingCanvas
        );
    }

    #[test]
    fn pass_b_section_failure_falls_back_to_a_single_full_text_call() {
        let fixture = v4_fixture();
        let progress = ImportProgressAdapter::new(&fixture.hosts, &fixture.job_id);

        let provider = MockProvider {
            responses: std::sync::Mutex::new(VecDeque::from([
                // Pass A
                Ok(pass_a_response()),
                // Pass B per-section call: a cut provider stream.
                Err("Kimi SSE ended before the [DONE] marker".to_string()),
                // Pass B full-text fallback call.
                Ok(fragment_response()),
                // Pass E root
                Ok(root_response()),
            ])),
        };
        let recovery = MockProvider {
            responses: std::sync::Mutex::new(VecDeque::new()),
        };

        let patch = tauri::async_runtime::block_on(run_v4_pipeline(
            None,
            &fixture.kernel,
            &fixture.policy,
            &fixture.doc,
            &fixture.extracted,
            &fixture.runtime,
            &fixture.job_id,
            DocumentFormat::Pdf,
            &progress,
            &provider,
            &recovery,
        ))
        .expect("the fallback keeps the pipeline alive");

        assert_eq!(patch["reviewRequired"], true);
        let operations = patch["operations"].as_array().expect("operations");
        assert!(
            operations.iter().any(|operation| {
                operation["node"]["id"] == "block_var_var_001"
                    && operation["node"]["type"] == "variable"
            }),
            "the fallback fragment must still reach the patch: {operations:?}"
        );

        let host = fixture.hosts.lock().expect("host lock");
        assert_eq!(
            host.get_job(&fixture.job_id).expect("job").state,
            JobState::PersistingCanvas
        );
    }

    #[test]
    fn pass_b_failure_propagates_when_the_fallback_also_fails() {
        let fixture = v4_fixture();
        let progress = ImportProgressAdapter::new(&fixture.hosts, &fixture.job_id);

        let provider = MockProvider {
            responses: std::sync::Mutex::new(VecDeque::from([
                Ok(pass_a_response()),
                Err("Kimi SSE ended before the [DONE] marker".to_string()),
                Err("Kimi SSE ended before the [DONE] marker".to_string()),
            ])),
        };
        let recovery = MockProvider {
            responses: std::sync::Mutex::new(VecDeque::new()),
        };

        let error = tauri::async_runtime::block_on(run_v4_pipeline(
            None,
            &fixture.kernel,
            &fixture.policy,
            &fixture.doc,
            &fixture.extracted,
            &fixture.runtime,
            &fixture.job_id,
            DocumentFormat::Pdf,
            &progress,
            &provider,
            &recovery,
        ))
        .expect_err("both Pass B attempts failed");

        assert!(
            error.contains("fallback also failed"),
            "the chained error must surface both failures: {error}"
        );
    }
}
