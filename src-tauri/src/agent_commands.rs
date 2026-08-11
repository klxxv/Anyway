//! Agent Tauri 命令——安全边界：Agent 不持有 API Key、文件系统句柄、网络访问、Graph store 写权限。
//! 宿主管理一切，Agent 输出只能进入 reviewRequired GraphPatch。
//!
//! Agent Tauri commands — security boundary: Agents hold no API keys, file handles,
//! network access, or graph store write permissions. The host manages everything;
//! agent output can only enter a reviewRequired GraphPatch.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};

use crate::agent_host::{AgentHost, AgentJob, JobState};
use crate::deepseek_client;
use crate::llm_client::{
    ApiFormat, CallRole, LlmClientAdapter, PdfAgentLlmClient, PdfAgentLlmConfig, PdfAgentTransport,
};
use crate::pdf_pipeline::PdfPipeline;
use semantic_pipeline::{Pipeline, PipelineConfig};

/// 宿主管理的全局 AgentHost 状态 / Host-managed global AgentHost state.
pub struct AgentHostState(pub Mutex<AgentHost>);

// ── 命令输入 / 输出类型 ──

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartPdfJobRequest {
    pub pdf_path: String,
    /// New host settings envelope. Older callers omit this field; the host
    /// then reads public persisted settings and environment fallbacks.
    #[serde(default)]
    pub settings: Option<Value>,
    #[serde(default)]
    pub plugin_settings: Option<Value>,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub api_format: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking: Option<Value>,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub credential_source: Option<String>,
    #[serde(default)]
    pub credential_env_var: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfJobStatus {
    pub job_id: String,
    pub pdf_path: String,
    pub file_hash: String,
    pub state: String,
    pub progress: (usize, usize),
    pub created_at: u64,
    pub updated_at: u64,
    pub error: Option<String>,
    pub result: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPatchRequest {
    pub job_id: String,
    pub accept: bool,
}

#[derive(Clone, Debug)]
struct PdfAgentRuntimeConfig {
    llm: PdfAgentLlmConfig,
    credential_source: String,
    credential_env_var: String,
}

impl PdfAgentRuntimeConfig {
    fn safe_snapshot(&self) -> Value {
        serde_json::json!({
            "apiUrl": self.llm.api_url,
            "apiFormat": self.llm.api_format,
            "provider": self.llm.provider,
            "model": self.llm.model,
            "thinking": self.llm.thinking,
            "thinkingLevel": self.llm.thinking_level,
            "transport": self.llm.transport,
            "credentialSource": self.credential_source,
            "credentialEnvVar": self.credential_env_var,
        })
    }
}

impl From<&AgentJob> for PdfJobStatus {
    fn from(job: &AgentJob) -> Self {
        Self {
            job_id: job.job_id.clone(),
            pdf_path: job.pdf_path.clone(),
            file_hash: job.file_hash.clone(),
            state: job.state.label().to_string(),
            progress: job.progress(),
            created_at: job.created_at,
            updated_at: job.updated_at,
            error: job.error.clone(),
            result: job.result.clone(),
        }
    }
}

fn merge_settings_value(target: &mut Map<String, Value>, value: &Value) {
    let Some(object) = value.as_object() else {
        return;
    };
    for (key, value) in object {
        if matches!(
            key.as_str(),
            "effectiveValues" | "values" | "settings" | "configuration"
        ) {
            merge_settings_value(target, value);
        } else {
            target.insert(key.clone(), value.clone());
        }
    }
}

fn persisted_agent_settings(app: &AppHandle) -> Map<String, Value> {
    let Ok(config_dir) = app.path().app_config_dir() else {
        return Map::new();
    };
    let path = config_dir.join("plugin-settings").join("settings.json");
    let Ok(bytes) = std::fs::read(path) else {
        return Map::new();
    };
    let Ok(document) = serde_json::from_slice::<Value>(&bytes) else {
        return Map::new();
    };
    let mut merged = Map::new();
    if let Some(plugins) = document.get("plugins").and_then(Value::as_object) {
        for (key, entry) in plugins {
            if key.starts_with("myc.pdf-canvas-agent@") || key.starts_with("pdf-canvas-agent@") {
                merge_settings_value(&mut merged, entry.get("values").unwrap_or(entry));
            }
        }
    }
    merged
}

fn setting<'a>(settings: &'a Map<String, Value>, names: &[&str]) -> Option<&'a Value> {
    names.iter().find_map(|name| settings.get(*name))
}

fn setting_string(settings: &Map<String, Value>, names: &[&str]) -> Option<String> {
    setting(settings, names)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn setting_bool_and_level(settings: &Map<String, Value>) -> (bool, Option<String>) {
    let Some(value) = setting(settings, &["thinking", "thinkingLevel", "thinking-level"]) else {
        return (false, None);
    };
    if let Some(enabled) = value.as_bool() {
        return (enabled, None);
    }
    let Some(level) = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return (false, None);
    };
    if matches!(
        level.to_ascii_lowercase().as_str(),
        "false" | "off" | "none" | "disabled"
    ) {
        (false, None)
    } else {
        (true, Some(level.to_string()))
    }
}

fn env_or_setting(
    settings: &Map<String, Value>,
    names: &[&str],
    env_names: &[&str],
) -> Option<String> {
    setting_string(settings, names).or_else(|| {
        env_names.iter().find_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
    })
}

fn resolve_pdf_agent_config(
    app: &AppHandle,
    request: &StartPdfJobRequest,
) -> Result<PdfAgentRuntimeConfig, String> {
    let mut settings = persisted_agent_settings(app);
    for value in [&request.settings, &request.plugin_settings] {
        if let Some(value) = value {
            merge_settings_value(&mut settings, value);
        }
    }
    for (key, value) in &request.extra {
        settings.insert(key.clone(), value.clone());
    }
    // A frontend request is never a credential transport. Ignore any legacy
    // or malicious plaintext key fields before resolving the trusted source.
    for key in ["api-key", "apiKey", "api_key", "secret", "credential"] {
        settings.remove(key);
    }
    let direct = [
        ("apiUrl", request.api_url.clone()),
        ("apiFormat", request.api_format.clone()),
        ("provider", request.provider.clone()),
        ("model", request.model.clone()),
        ("transport", request.transport.clone()),
        ("credentialSource", request.credential_source.clone()),
        ("credentialEnvVar", request.credential_env_var.clone()),
    ];
    for (key, value) in direct {
        if let Some(value) = value {
            settings.insert(key.to_string(), Value::String(value));
        }
    }
    if let Some(thinking) = &request.thinking {
        settings.insert("thinking".to_string(), thinking.clone());
    }

    let api_format_value = env_or_setting(
        &settings,
        &["api-format", "apiFormat", "api_format"],
        &["PDF_AGENT_API_FORMAT"],
    );
    let api_format = ApiFormat::parse(api_format_value.as_deref().or(Some("anthropic")))
        .map_err(|error| error.to_string())?;
    let default_url = match api_format {
        ApiFormat::OpenAi => "https://api.deepseek.com",
        ApiFormat::Anthropic => "https://api.deepseek.com/anthropic",
    };
    let api_url = env_or_setting(
        &settings,
        &["api-url", "apiUrl", "api_url"],
        &["PDF_AGENT_API_URL"],
    )
    .unwrap_or_else(|| default_url.to_string());
    let provider = env_or_setting(
        &settings,
        &["provider", "providerId", "provider-id"],
        &["PDF_AGENT_PROVIDER"],
    )
    .unwrap_or_else(|| {
        if crate::llm_client::is_approved_kimi_endpoint(&api_url) {
            "moonshot".to_string()
        } else {
            "deepseek".to_string()
        }
    });
    let model = env_or_setting(
        &settings,
        &["model", "modelId", "model-id"],
        &["PDF_AGENT_MODEL", "DEEPSEEK_MODEL"],
    )
    .unwrap_or_else(|| "deepseek-v4-flash".to_string());
    let (thinking, thinking_level) = setting_bool_and_level(&settings);
    let transport_value = env_or_setting(
        &settings,
        &["transport", "file-transport", "pdf-transport"],
        &["PDF_AGENT_TRANSPORT"],
    );
    let transport =
        PdfAgentTransport::parse(transport_value.as_deref()).map_err(|error| error.to_string())?;

    let credential_source = env_or_setting(
        &settings,
        &["credential-source", "credentialSource"],
        &["PDF_AGENT_CREDENTIAL_SOURCE"],
    )
    .unwrap_or_else(|| "env".to_string());
    let credential_env_var = env_or_setting(
        &settings,
        &[
            "credential-env-var",
            "credentialEnvVar",
            "envVar",
            "env-var",
        ],
        &["PDF_AGENT_CREDENTIAL_ENV_VAR"],
    )
    .unwrap_or_else(|| "DEEPSEEK_API_KEY".to_string());
    let api_key = if credential_source.eq_ignore_ascii_case("legacy-config") {
        deepseek_client::read_api_key_from_config(app).ok()
    } else {
        crate::plugin_settings::resolve_connection_credentials(
            "myc.pdf-canvas-agent",
            None,
            &credential_source,
            &credential_env_var,
            "api-key",
        )?
    };
    let api_key = api_key.ok_or_else(|| format!("PDF Agent credential is not configured; set {credential_env_var} or choose a host credential source"))?;
    let llm = PdfAgentLlmConfig {
        api_url,
        api_format,
        api_key,
        model,
        thinking,
        thinking_level,
        provider,
        transport,
        timeout_secs: 120,
    };
    llm.validate().map_err(|error| error.to_string())?;
    Ok(PdfAgentRuntimeConfig {
        llm,
        credential_source,
        credential_env_var,
    })
}

// ── Tauri Commands ──

/// 启动 PDF 处理 Job。自动运行完整管线：
/// 文件校验 → 文本提取 → OCR fallback → DocumentMap → 语义提取 → GraphPatch 生成 → 进入审阅。
///
/// Start a PDF processing job. Automatically runs the full pipeline:
/// validate → extract text → OCR fallback → build DocumentMap → extract semantics →
/// generate GraphPatch → await review.
///
/// 并发模型:异步命令 + 每阶段短锁 + 重活进线程池。状态锁只在 create/advance
/// 时短暂持有,PDF 解析在 spawn_blocking 里跑 — UI 不冻结,cancel_job 能在
/// 阶段边界介入(下一阶段推进时撞终态而干净退出)。
#[tauri::command]
pub async fn start_pdf_job(
    app: AppHandle,
    state: State<'_, AgentHostState>,
    request: StartPdfJobRequest,
) -> Result<PdfJobStatus, String> {
    let runtime = resolve_pdf_agent_config(&app, &request)?;
    let pdf_path = Path::new(&request.pdf_path);
    let abs_path = pdf_path
        .canonicalize()
        .map_err(|e| format!("Cannot resolve path: {e}"))?;

    // 创建 job（含文件校验）——短锁,拿到 id 即释放。
    let job_id = {
        let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
        host.create_job(&abs_path)?.job_id.clone()
    };

    let outcome = run_pdf_stages(&app, &state.0, &job_id, &abs_path, runtime).await;
    if let Err(error) = outcome {
        // 管线失败必须落 Failed 终态,否则 job 永久卡死在非终态;
        // 若 job 已被并发取消/裁决,保持既有终态。
        let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
        let _ = host.advance_job(&job_id, JobState::Failed, None, None, Some(&error));
        return Err(error);
    }

    let host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    let job = host
        .get_job(&job_id)
        .ok_or_else(|| "Job vanished".to_string())?;
    Ok(PdfJobStatus::from(job))
}

/// 短锁推进一个阶段;并发取消/失败导致转换非法时,若已是终态则返回干净的并发终止错误。
fn advance_stage(
    hosts: &Mutex<AgentHost>,
    job_id: &str,
    next: JobState,
    output_hash: Option<&str>,
    data: Option<Value>,
) -> Result<(), String> {
    let mut host = hosts.lock().map_err(|e| format!("Lock error: {e}"))?;
    match host.advance_job(job_id, next, output_hash, data, None) {
        Ok(_) => Ok(()),
        Err(error) => {
            let terminated = host
                .get_job(job_id)
                .map(|job| job.state.is_terminal())
                .unwrap_or(false);
            if terminated {
                Err("Job was cancelled or failed concurrently".to_string())
            } else {
                Err(error)
            }
        }
    }
}

/// 管线主体:阶段推进用短锁,重计算进线程池,失败即返回由调用方落 Failed。
async fn run_pdf_stages(
    app: &AppHandle,
    hosts: &Mutex<AgentHost>,
    job_id: &str,
    abs_path: &Path,
    runtime: PdfAgentRuntimeConfig,
) -> Result<(), String> {
    // ── 阶段 1：ValidatingFile ──
    advance_stage(
        hosts,
        job_id,
        JobState::ValidatingFile,
        Some("v1"),
        Some(runtime.safe_snapshot()),
    )?;

    // ── 阶段 2：ExtractingText ──
    let extracted = {
        if runtime.llm.transport == PdfAgentTransport::KimiFileExtract {
            let client =
                PdfAgentLlmClient::new(runtime.llm.clone()).map_err(|error| error.to_string())?;
            let text = client
                .extract_kimi_file_text(abs_path)
                .await
                .map_err(|error| format!("Kimi file extraction failed: {error}"))?;
            PdfPipeline::from_plain_text(text)
        } else {
            let path = abs_path.to_path_buf();
            tauri::async_runtime::spawn_blocking(move || PdfPipeline::extract_text(&path))
                .await
                .map_err(|e| format!("Pipeline task join error: {e}"))??
        }
    };
    let text_hash = format!("{:x}", sha2::Sha256::digest(extracted.full_text.as_bytes()));
    advance_stage(
        hosts,
        job_id,
        JobState::ExtractingText,
        Some(&text_hash),
        Some(serde_json::to_value(&extracted).map_err(|e| e.to_string())?),
    )?;

    // ── 阶段 3：OcrOptional ──
    // 复用阶段 2 的提取结果，不再重新解析 PDF。
    let ocr_triggered = PdfPipeline::needs_ocr(&extracted);
    let (final_extracted, ocr_confidence, ocr_error) = if ocr_triggered {
        let extracted_for_ocr = extracted.clone();
        match tauri::async_runtime::spawn_blocking(move || {
            PdfPipeline::ocr_fallback(&extracted_for_ocr)
        })
        .await
        .map_err(|e| format!("Pipeline task join error: {e}"))?
        {
            Ok(ocr_text) => (ocr_text, Some(1.0_f64), None),
            Err(error) => {
                // OCR 失败非致命：保留原文本并记录警告，后续阶段仍可推进。
                (extracted.clone(), None, Some(error))
            }
        }
    } else {
        (extracted.clone(), None, None)
    };
    let final_text_hash = format!(
        "{:x}",
        sha2::Sha256::digest(final_extracted.full_text.as_bytes())
    );
    advance_stage(
        hosts,
        job_id,
        JobState::OcrOptional,
        Some(&final_text_hash),
        Some(serde_json::json!({
            "ocrTriggered": ocr_triggered,
            "ocrConfidence": ocr_confidence,
            "ocrError": ocr_error
        })),
    )?;

    // ── 阶段 4：BuildingDocumentMap ──
    // 直接由已提取文本构建文档结构，不再第三次解析 PDF。
    let doc = {
        let extracted_for_doc = final_extracted.clone();
        tauri::async_runtime::spawn_blocking(move || {
            PdfPipeline::build_structured_document(extracted_for_doc, ocr_triggered, ocr_confidence)
        })
        .await
        .map_err(|e| format!("Pipeline task join error: {e}"))?
    };
    let doc_hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_string(&doc).unwrap_or_default().as_bytes())
    );
    advance_stage(
        hosts,
        job_id,
        JobState::BuildingDocumentMap,
        Some(&doc_hash),
        Some(serde_json::to_value(&doc).map_err(|e| e.to_string())?),
    )?;

    // ── 阶段 5：ExtractingSemantics ──
    // The host sends bounded text messages to the configured LLM. The PDF
    // bytes have already ended at local extraction (or Kimi text extraction).
    let patch = run_semantic_graph_pipeline(app, &doc, &final_extracted, &runtime, job_id).await?;
    let semantic_hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_string(&patch).unwrap_or_default().as_bytes())
    );
    advance_stage(
        hosts,
        job_id,
        JobState::ExtractingSemantics,
        Some(&semantic_hash),
        Some(patch.clone()),
    )?;

    // ── 阶段 6：GeneratingPatch ──
    advance_stage(
        hosts,
        job_id,
        JobState::GeneratingPatch,
        Some(&semantic_hash),
        Some(patch.clone()),
    )?;

    // ── 阶段 7:AwaitingReview(data 即审阅载荷,advance_job 写入 job.result)──
    advance_stage(
        hosts,
        job_id,
        JobState::AwaitingReview,
        Some(&semantic_hash),
        Some(patch),
    )?;
    Ok(())
}

/// 查询 Job 状态 / Query job status.
#[tauri::command]
pub fn get_job_status(
    state: State<'_, AgentHostState>,
    job_id: String,
) -> Result<PdfJobStatus, String> {
    let host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    let job = host
        .get_job(&job_id)
        .ok_or_else(|| format!("Job not found: {job_id}"))?;
    Ok(PdfJobStatus::from(job))
}

/// 审阅裁决：接受或拒绝 Agent 输出的 GraphPatch。
/// Review decision: accept or reject the agent's proposed GraphPatch.
#[tauri::command]
pub fn review_patch(
    state: State<'_, AgentHostState>,
    request: ReviewPatchRequest,
) -> Result<PdfJobStatus, String> {
    let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    host.review_patch(&request.job_id, request.accept)?;
    let job = host
        .get_job(&request.job_id)
        .ok_or_else(|| "Job vanished".to_string())?;
    Ok(PdfJobStatus::from(job))
}

/// 取消进行中的 Job / Cancel an in-progress job.
#[tauri::command]
pub fn cancel_job(
    state: State<'_, AgentHostState>,
    job_id: String,
    reason: Option<String>,
) -> Result<PdfJobStatus, String> {
    let mut host = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    let reason = reason.unwrap_or_else(|| "Cancelled by user".to_string());
    host.cancel_job(&job_id, &reason)?;
    let job = host
        .get_job(&job_id)
        .ok_or_else(|| "Job vanished".to_string())?;
    Ok(PdfJobStatus::from(job))
}

async fn run_semantic_graph_pipeline(
    app: &AppHandle,
    doc: &crate::pdf_pipeline::StructuredDocument,
    extracted: &crate::pdf_pipeline::ExtractedText,
    runtime: &PdfAgentRuntimeConfig,
    job_id: &str,
) -> Result<Value, String> {
    let prompts_dir = resolve_prompts_dir(app)?;
    let config = PipelineConfig::load(&prompts_dir, "en").map_err(|error| error.to_string())?;
    let pipeline = Pipeline::new(config);
    let client =
        Arc::new(PdfAgentLlmClient::new(runtime.llm.clone()).map_err(|error| error.to_string())?);
    let provider = LlmClientAdapter::new(client, CallRole::Extraction, job_id.to_string());
    let bounded_text = PdfPipeline::bounded_llm_context(&extracted.full_text);
    let document_json = serde_json::to_string(doc).map_err(|error| error.to_string())?;

    let pass_a_vars = pipeline.prepare_pass_a_input(&bounded_text, &document_json);
    let pass_a_raw = pipeline
        .call_llm("structure-extraction", &pass_a_vars, &provider)
        .await
        .map_err(|error| format!("Pass A structure extraction failed: {error}"))?;
    let structure = Pipeline::parse_pass_a_output(&pass_a_raw)
        .map_err(|error| format!("Pass A result parsing failed: {error}"))?;

    // The bounded context is assembled from local text chunks. One bounded
    // Pass B call avoids duplicate temp ids that would otherwise be produced
    // by independently chunked entity calls.
    let pass_b_vars =
        pipeline.prepare_pass_b_input(&document_json, "bounded PDF text chunks", &bounded_text);
    let pass_b_raw = pipeline
        .call_llm("entity-extraction", &pass_b_vars, &provider)
        .await
        .map_err(|error| format!("Pass B entity extraction failed: {error}"))?;
    let entities = Pipeline::parse_pass_b_output(&pass_b_raw)
        .map_err(|error| format!("Pass B result parsing failed: {error}"))?;
    let entities_json = serde_json::to_string(&entities).map_err(|error| error.to_string())?;

    let experiment_paragraphs = doc
        .paragraphs
        .iter()
        .filter(|paragraph| {
            let text = paragraph.text.to_ascii_lowercase();
            text.contains("experiment")
                || text.contains("method")
                || text.contains("result")
                || text.contains("ablation")
        })
        .map(|paragraph| paragraph.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let pass_c_vars = pipeline.prepare_pass_c_input(
        &entities_json,
        &truncate_chars(&experiment_paragraphs, 40_000),
        &bounded_text,
    );
    let pass_c_raw = pipeline
        .call_llm("variable-fission", &pass_c_vars, &provider)
        .await
        .map_err(|error| format!("Pass C variable extraction failed: {error}"))?;
    let variable_fission = Pipeline::parse_pass_c_output(&pass_c_raw)
        .map_err(|error| format!("Pass C result parsing failed: {error}"))?;
    let variable_json =
        serde_json::to_string(&variable_fission).map_err(|error| error.to_string())?;

    let pass_d_vars = pipeline.prepare_pass_d_input(&entities_json, &variable_json, &bounded_text);
    let pass_d_raw = pipeline
        .call_llm("cross-segment-merge", &pass_d_vars, &provider)
        .await
        .map_err(|error| format!("Pass D merge failed: {error}"))?;
    let merge_result = Pipeline::parse_pass_d_output(&pass_d_raw)
        .map_err(|error| format!("Pass D result parsing failed: {error}"))?;
    let merge_json = serde_json::to_string(&merge_result).map_err(|error| error.to_string())?;

    let pass_e_vars = pipeline.prepare_pass_e_input(
        structure.title.as_deref().unwrap_or("Untitled paper"),
        &structure.authors.join(", "),
        &serde_json::to_string(&structure).map_err(|error| error.to_string())?,
        &entities_json,
        &variable_json,
        &merge_json,
        &bounded_text,
    );
    let pass_e_raw = pipeline
        .call_llm("paper-level-synthesis", &pass_e_vars, &provider)
        .await
        .map_err(|error| format!("Pass E synthesis failed: {error}"))?;
    let synthesis = Pipeline::parse_pass_e_output(&pass_e_raw)
        .map_err(|error| format!("Pass E result parsing failed: {error}"))?;

    let candidates = Pipeline::build_candidates(
        &format!("pdf:{job_id}"),
        None,
        Some(&structure),
        Some(&entities),
        Some(&variable_fission),
        Some(&merge_result),
        Some(&synthesis),
    );
    let validation = Pipeline::run_validation(&candidates, &extracted.full_text);
    if !validation.passed {
        return Err(format!(
            "PDF semantic validation failed: {}",
            validation.summary
        ));
    }
    let patch = graphpatch_gen::build_graph_patch(&candidates, "myc.pdf-canvas-agent");
    serde_json::to_value(patch).map_err(|error| format!("GraphPatch serialization failed: {error}"))
}

fn resolve_prompts_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();

    #[cfg(debug_assertions)]
    {
        let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        candidates.push(repository.join("plugins/sources/myc.pdf-canvas-agent/prompts"));
        candidates.push(repository.join("config/prompts"));
    }

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
            candidates.extend(plugin_prompts);
        }
    }

    candidates
        .into_iter()
        .find(|path| path.join("manifest.yaml").is_file())
        .ok_or_else(|| "PDF Agent prompt configuration is unavailable".to_string())
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

// ── GraphPatch 构建（语义提取） ──

/// 从 StructuredDocument 构建 reviewRequired GraphPatch。
/// Agent 不能直接修改图存储——它的输出只能是这个待审阅的 GraphPatch。
///
/// Build a reviewRequired GraphPatch from a StructuredDocument.
/// The agent cannot directly mutate the graph store — its output can only be
/// this review-gated GraphPatch.
#[cfg(test)]
fn build_graph_patch_from_document(
    doc: &crate::pdf_pipeline::StructuredDocument,
    job_id: &str,
) -> Value {
    let mut operations = Vec::new();

    // 每个章节 → 一个 note 节点
    for section in &doc.sections {
        let node_id = format!("pdf-sec-{}", section.id);
        operations.push(serde_json::json!({
            "op": "add-node",
            "node": {
                "id": node_id,
                "type": "note",
                "title": section.title,
                "body": format!("Section level {} (offsets {}-{})", section.level, section.start_offset, section.end_offset),
                "tags": ["pdf-import", "section"],
                "data": {
                    "sectionId": section.id,
                    "level": section.level,
                    "childSectionIds": section.child_section_ids,
                    "sourceJobId": job_id
                }
            }
        }));

        // 父子章节 → depends_on 边
        for child_id in &section.child_section_ids {
            operations.push(serde_json::json!({
                "op": "add-edge",
                "edge": {
                    "id": format!("pdf-edge-{}-{}", section.id, child_id),
                    "source": format!("pdf-sec-{}", section.id),
                    "target": format!("pdf-sec-{}", child_id),
                    "type": "part_of",
                    "note": "section hierarchy"
                }
            }));
        }
    }

    // 每个段落 → 一个 evidence 节点
    for para in &doc.paragraphs {
        let snippet: String = para.text.chars().take(200).collect();
        operations.push(serde_json::json!({
            "op": "add-node",
            "node": {
                "id": format!("pdf-{}", para.id),
                "type": "evidence",
                "title": snippet,
                "body": para.text,
                "tags": ["pdf-import", "paragraph"],
                "data": {
                    "paragraphId": para.id,
                    "sectionId": para.section_id,
                    "startOffset": para.start_offset,
                    "endOffset": para.end_offset,
                    "sourceJobId": job_id
                }
            }
        }));

        // 段落 → 所属章节
        operations.push(serde_json::json!({
            "op": "add-edge",
            "edge": {
                "id": format!("pdf-edge-para-{}", para.id),
                "source": format!("pdf-{}", para.id),
                "target": format!("pdf-sec-{}", para.section_id),
                "type": "part_of",
                "note": "paragraph belongs to section"
            }
        }));
    }

    // 图表引用 → 独立的 evidence/concept 节点
    for ft in &doc.figures_tables {
        let kind_label = match ft.kind {
            crate::pdf_pipeline::FigureTableKind::Figure => "figure",
            crate::pdf_pipeline::FigureTableKind::Table => "table",
        };
        operations.push(serde_json::json!({
            "op": "add-node",
            "node": {
                "id": format!("pdf-{}", ft.id),
                "type": "concept",
                "title": ft.caption,
                "body": format!("{} reference at offset {}", kind_label, ft.caption_offset),
                "tags": ["pdf-import", kind_label],
                "data": {
                    "figureTableId": ft.id,
                    "kind": kind_label,
                    "captionOffset": ft.caption_offset,
                    "sourceJobId": job_id
                }
            }
        }));
    }

    serde_json::json!({
        "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
        "source": {
            "pluginId": "pdf-canvas-agent",
            "operation": "pdf-document-extraction",
            "externalId": job_id
        },
        "title": format!("PDF structure extraction ({} sections, {} paragraphs)",
            doc.sections.len(), doc.paragraphs.len()),
        "summary": format!(
            "Extracted document structure from PDF. {} sections, {} paragraphs, {} figures/tables{}",
            doc.sections.len(),
            doc.paragraphs.len(),
            doc.figures_tables.len(),
            if doc.ocr_triggered { " (OCR assisted)" } else { "" }
        ),
        "reviewRequired": true,
        "operations": operations
    })
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_snapshot_never_contains_api_key() {
        let runtime = PdfAgentRuntimeConfig {
            llm: PdfAgentLlmConfig {
                api_url: "https://api.deepseek.com".into(),
                api_format: ApiFormat::OpenAi,
                api_key: "must-not-escape".into(),
                model: "deepseek-chat".into(),
                thinking: false,
                thinking_level: None,
                provider: "deepseek".into(),
                transport: PdfAgentTransport::LocalText,
                timeout_secs: 120,
            },
            credential_source: "environment".into(),
            credential_env_var: "DEEPSEEK_API_KEY".into(),
        };
        let snapshot = runtime.safe_snapshot().to_string();
        assert!(!snapshot.contains("must-not-escape"));
        assert!(!snapshot.contains("apiKey"));
    }

    #[test]
    fn build_graph_patch_from_document_produces_review_required_operations() {
        // 构造最小 StructuredDocument
        let pages = vec![crate::pdf_pipeline::PageText {
            page_number: 1,
            text: "test".into(),
            char_count: 4,
        }];
        let sections = vec![
            crate::pdf_pipeline::StructureSection {
                id: "s1".into(),
                title: "Introduction".into(),
                level: 1,
                start_offset: 0,
                end_offset: 100,
                child_section_ids: vec!["s1.1".into()],
            },
            crate::pdf_pipeline::StructureSection {
                id: "s1.1".into(),
                title: "Background".into(),
                level: 2,
                start_offset: 50,
                end_offset: 100,
                child_section_ids: vec![],
            },
        ];
        let paragraphs = vec![crate::pdf_pipeline::StructureParagraph {
            id: "p1".into(),
            section_id: "s1".into(),
            text: "This is a test paragraph with important claims.".into(),
            start_offset: 0,
            end_offset: 45,
        }];
        let figures_tables = vec![crate::pdf_pipeline::FigureTableRef {
            id: "fig1".into(),
            kind: crate::pdf_pipeline::FigureTableKind::Figure,
            caption: "Overview of the proposed method.".into(),
            caption_offset: 60,
        }];
        let document_map = crate::pdf_pipeline::DocumentMap::build(&sections, &paragraphs, &pages);
        let doc = crate::pdf_pipeline::StructuredDocument {
            sections,
            paragraphs,
            figures_tables,
            document_map,
            ocr_triggered: false,
            ocr_confidence: None,
        };

        let patch = build_graph_patch_from_document(&doc, "test-job");

        assert_eq!(
            patch["apiVersion"],
            "researchcanvas.dev/graph-patch/v1alpha1"
        );
        assert_eq!(patch["reviewRequired"], true);
        assert_eq!(patch["source"]["pluginId"], "pdf-canvas-agent");

        let ops = patch["operations"].as_array().expect("operations array");
        // 2 sections + 3 edges (hierarchy + paragraph→section + figure node) + 1 paragraph + 1 figure = 7+
        assert!(
            ops.len() >= 6,
            "expected at least 6 operations, got {}",
            ops.len()
        );

        // 有 section 节点
        assert!(ops.iter().any(|op| op["node"]["id"] == "pdf-sec-s1"));
        // 有 paragraph 节点
        assert!(ops.iter().any(|op| op["node"]["id"] == "pdf-p1"));
        // 有 figure 节点
        assert!(ops.iter().any(|op| op["node"]["id"] == "pdf-fig1"));
        // 有结构边
        assert!(ops.iter().any(|op| {
            op["edge"]["id"] == "pdf-edge-s1-s1.1" && op["edge"]["type"] == "part_of"
        }));
    }
}
