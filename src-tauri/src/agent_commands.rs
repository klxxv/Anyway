//! Agent Tauri 命令——安全边界：Agent 不持有 API Key、文件系统句柄、网络访问、Graph store 写权限。
//! 宿主管理一切，Agent 输出只能进入 reviewRequired GraphPatch。
//!
//! Agent Tauri commands — security boundary: Agents hold no API keys, file handles,
//! network access, or graph store write permissions. The host manages everything;
//! agent output can only enter a reviewRequired GraphPatch.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::Digest;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Manager, State};

use crate::agent_host::{
    AgentHost, AgentJob, DocumentFormat, ImportBatch, JobState, PublicProgressUpdate,
    ReasoningActivity, RepairAuditEntry, RepairAuditRecord,
};
use crate::deepseek_client;
use crate::kernel::identity::{PrincipalId, WorkerId};
use crate::kernel::lifecycle::FailureReason;
use crate::kernel::scheduler::Scheduler;
use crate::kernel::supervisor::{Supervisor, SupervisorAction, WorkerObservation};
use crate::llm_client::{
    ApiFormat, CallRole, LlmClientAdapter, PdfAgentLlmClient, PdfAgentLlmConfig, PdfAgentTransport,
};
use crate::native_plugins::pdf_canvas_agent;
use crate::pdf_pipeline::{DocumentInput, PdfPipeline};
use semantic_pipeline::pipeline::{LlmProvider, ResponseFormat};
use semantic_pipeline::{
    parse_json_with_repair, AuditEntry, AuditReport, AuditSeverity, Pipeline, PipelineConfig,
    RepairOptions, RepairOutcome,
};

/// 宿主管理的全局 AgentHost 状态 / Host-managed global AgentHost state.
pub struct AgentHostState(pub Mutex<AgentHost>, pub AgentJobGate);

impl AgentHostState {
    /// Serial fallback for tests and legacy callers: a quota-one gate with
    /// private scheduler and supervisor planes. Production wiring uses
    /// [`AgentHostState::with_gate`] so jobs share the kernel planes.
    pub fn new(host: AgentHost) -> Self {
        Self(Mutex::new(host), AgentJobGate::serial())
    }

    /// Production constructor: agent batches queue through the kernel-owned
    /// gate and report observations to the kernel supervisor.
    pub fn with_gate(host: AgentHost, gate: AgentJobGate) -> Self {
        Self(Mutex::new(host), gate)
    }
}

/// Phase 4 transport gate for agent batches.
///
/// The gate pairs a Tokio semaphore (the async waiting point) with the pure
/// kernel [`Scheduler`] (the accounting ledger). The semaphore is sized to
/// the same quota, so the scheduler admission cannot fail for a gate-built
/// permit; the scheduler snapshot remains the kernel-owned source of truth
/// for inflight work per principal.
pub struct AgentJobGate {
    scheduler: Arc<RwLock<Scheduler>>,
    supervisor: Arc<RwLock<Supervisor>>,
    worker_id: WorkerId,
    principal: PrincipalId,
    quota: usize,
    semaphore: Arc<tokio::sync::Semaphore>,
}

impl AgentJobGate {
    /// Build a gate backed by kernel scheduler and supervisor planes.
    pub fn new(
        scheduler: Arc<RwLock<Scheduler>>,
        supervisor: Arc<RwLock<Supervisor>>,
        worker_id: WorkerId,
        principal: PrincipalId,
        quota: usize,
    ) -> Self {
        Self {
            scheduler,
            supervisor,
            worker_id,
            principal,
            quota,
            semaphore: Arc::new(tokio::sync::Semaphore::new(quota)),
        }
    }

    /// Quota-one gate with private planes, preserving the previous serial
    /// behavior for tests and legacy construction paths.
    pub fn serial() -> Self {
        Self::new(
            Arc::new(RwLock::new(Scheduler::with_default_quota(1).expect("quota one"))),
            Arc::new(RwLock::new(Supervisor::default())),
            WorkerId::new("worker.agent-host.serial").expect("serial worker id"),
            PrincipalId::new(crate::kernel::policy::NATIVE_UI_PRINCIPAL_NAME)
                .expect("native ui principal"),
            1,
        )
    }

    pub fn principal(&self) -> &PrincipalId {
        &self.principal
    }

    pub fn quota(&self) -> usize {
        self.quota
    }

    /// Wait for a job slot, then record the reservation in the kernel
    /// scheduler. The returned permit releases both on drop.
    pub async fn acquire(&self) -> JobPermit {
        let semaphore = Arc::clone(&self.semaphore);
        let semaphore_permit = semaphore
            .acquire_owned()
            .await
            .expect("agent gate semaphore is never closed");
        {
            let mut scheduler = self
                .scheduler
                .write()
                .expect("agent gate scheduler lock");
            scheduler
                .acquire(&self.principal)
                .expect("semaphore already bounds concurrency to the quota");
        }
        JobPermit {
            _semaphore: semaphore_permit,
            scheduler: Arc::clone(&self.scheduler),
            principal: self.principal.clone(),
        }
    }

    /// Report a lifecycle observation to the kernel supervisor for the
    /// declared agent worker. The action is advisory for an in-process pool.
    pub fn observe(&self, observation: WorkerObservation) -> SupervisorAction {
        self.supervisor
            .write()
            .map_err(|error| format!("supervisor lock: {error}"))
            .map(|mut supervisor| {
                supervisor
                    .observe(&self.worker_id, observation)
                    .unwrap_or(SupervisorAction::Noop)
            })
            .unwrap_or(SupervisorAction::Noop)
    }
}

/// Held for the duration of one queued batch; releases the scheduler slot
/// and the semaphore permit on drop.
pub struct JobPermit {
    _semaphore: tokio::sync::OwnedSemaphorePermit,
    scheduler: Arc<RwLock<Scheduler>>,
    principal: PrincipalId,
}

impl Drop for JobPermit {
    fn drop(&mut self) {
        if let Ok(mut scheduler) = self.scheduler.write() {
            scheduler.release(&self.principal);
        }
    }
}

// ── 命令输入 / 输出类型 ──

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartDocumentBatchRequest {
    pub paths: Vec<String>,
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

impl StartDocumentBatchRequest {
    fn job_request(&self, path: String) -> StartPdfJobRequest {
        StartPdfJobRequest {
            pdf_path: path,
            settings: self.settings.clone(),
            plugin_settings: self.plugin_settings.clone(),
            api_url: self.api_url.clone(),
            api_format: self.api_format.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            thinking: self.thinking.clone(),
            transport: self.transport.clone(),
            credential_source: self.credential_source.clone(),
            credential_env_var: self.credential_env_var.clone(),
            extra: self.extra.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfJobStatus {
    pub job_id: String,
    pub file_path: String,
    pub document_format: Option<DocumentFormat>,
    pub batch_id: Option<String>,
    pub reasoning_activity: ReasoningActivity,
    pub public_progress: Vec<PublicProgressUpdate>,
    pub repair_audit: Vec<RepairAuditRecord>,
    pub upload_bytes: u64,
    pub upload_total_bytes: Option<u64>,
    /// Compatibility alias for the original review panel.
    pub pdf_path: String,
    pub file_hash: String,
    pub state: String,
    pub progress: (usize, usize),
    pub created_at: u64,
    pub updated_at: u64,
    pub error: Option<String>,
    pub result: Option<Value>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportBatchStatus {
    pub batch_id: String,
    pub state: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub jobs: Vec<PdfJobStatus>,
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
    backend: pdf_canvas_agent::Backend,
    public_progress: bool,
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
            "publicProgress": self.public_progress,
            "credentialSource": self.credential_source,
            "credentialEnvVar": self.credential_env_var,
        })
    }
}

impl From<&AgentJob> for PdfJobStatus {
    fn from(job: &AgentJob) -> Self {
        let completed_stages = match job.state {
            JobState::Queued | JobState::Created => 0,
            JobState::ValidatingFile => 0,
            JobState::ExtractingText => 1,
            JobState::OcrOptional => 2,
            JobState::BuildingDocumentMap => 3,
            JobState::ExtractingSemantics => 4,
            JobState::GeneratingPatch => 5,
            JobState::AwaitingReview
            | JobState::Accepted
            | JobState::Rejected
            | JobState::Cancelled
            | JobState::Failed => job.progress().0.min(7),
        };
        Self {
            job_id: job.job_id.clone(),
            file_path: job.pdf_path.clone(),
            document_format: job.document_format,
            batch_id: job.batch_id.clone(),
            reasoning_activity: {
                let mut activity = job.reasoning_activity.clone();
                if let Some(started_at) = activity.started_at {
                    activity.elapsed_ms = activity
                        .elapsed_ms
                        .max(unix_millis().saturating_sub(started_at));
                }
                activity
            },
            public_progress: job.reasoning_activity.public_progress.clone(),
            repair_audit: job.repair_audit.clone(),
            upload_bytes: job.upload_bytes,
            upload_total_bytes: job.upload_total_bytes,
            pdf_path: job.pdf_path.clone(),
            file_hash: job.file_hash.clone(),
            state: job.state.label().to_string(),
            progress: (
                if job.state == JobState::AwaitingReview {
                    7
                } else {
                    completed_stages
                },
                7,
            ),
            created_at: job.created_at,
            updated_at: job.updated_at,
            error: job.error.clone(),
            result: job.result.clone(),
        }
    }
}

fn unix_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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

fn setting_enabled(settings: &Map<String, Value>, names: &[&str], default: bool) -> bool {
    let Some(value) = setting(settings, names) else {
        return default;
    };
    if let Some(enabled) = value.as_bool() {
        return enabled;
    }
    value
        .as_str()
        .map(str::trim)
        .map(|value| {
            matches!(
                value.to_ascii_lowercase().as_str(),
                "true" | "on" | "enabled" | "yes"
            )
        })
        .unwrap_or(default)
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
    let api_format = ApiFormat::parse(api_format_value.as_deref().or(Some("openai")))
        .map_err(|error| error.to_string())?;
    let api_url = env_or_setting(
        &settings,
        &["api-url", "apiUrl", "api_url"],
        &["PDF_AGENT_API_URL"],
    )
    .ok_or_else(|| "PDF Agent API URL is not configured".to_string())?;
    let provider = env_or_setting(
        &settings,
        &["provider", "providerId", "provider-id"],
        &["PDF_AGENT_PROVIDER"],
    )
    .unwrap_or_else(|| {
        if pdf_canvas_agent::is_approved_endpoint(&api_url) {
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
    .unwrap_or_else(|| pdf_canvas_agent::KIMI_K26_MODEL.to_string());
    let (thinking, thinking_level) =
        if setting(&settings, &["thinking", "thinkingLevel", "thinking-level"]).is_some() {
            setting_bool_and_level(&settings)
        } else {
            (true, None)
        };
    let transport_value = env_or_setting(
        &settings,
        &["transport", "file-transport", "pdf-transport"],
        &["PDF_AGENT_TRANSPORT"],
    );
    let transport =
        PdfAgentTransport::parse(transport_value.as_deref()).map_err(|error| error.to_string())?;
    let public_progress = setting_enabled(
        &settings,
        &["public-progress", "publicProgress", "public_progress"],
        false,
    );

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
    .unwrap_or_else(|| {
        pdf_canvas_agent::default_credential_env_var(&api_url, &provider, &model).to_string()
    });
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
    let mut normalized = pdf_canvas_agent::normalize_config(llm)?;
    if let pdf_canvas_agent::Backend::KimiK26(config) = &mut normalized.backend {
        config.public_progress = public_progress;
    }
    Ok(PdfAgentRuntimeConfig {
        llm: normalized.llm,
        backend: normalized.backend,
        public_progress,
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
    let batch_request = StartDocumentBatchRequest {
        paths: vec![request.pdf_path],
        settings: request.settings,
        plugin_settings: request.plugin_settings,
        api_url: request.api_url,
        api_format: request.api_format,
        provider: request.provider,
        model: request.model,
        thinking: request.thinking,
        transport: request.transport,
        credential_source: request.credential_source,
        credential_env_var: request.credential_env_var,
        extra: request.extra,
    };
    let batch = {
        let mut host = state
            .0
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?;
        host.create_batch(&batch_request.paths)?
    };
    let queued = {
        let host = state
            .0
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?;
        let job_id = batch
            .job_ids
            .first()
            .ok_or_else(|| "PDF batch did not create a job".to_string())?;
        PdfJobStatus::from(
            host.get_job(job_id)
                .ok_or_else(|| "Job vanished".to_string())?,
        )
    };
    tauri::async_runtime::spawn(run_document_batch(app, batch, batch_request));
    Ok(queued)
}

/// Queue a document batch and return before validation, extraction, or model work begins.
#[tauri::command]
pub async fn start_document_batch(
    app: AppHandle,
    state: State<'_, AgentHostState>,
    request: StartDocumentBatchRequest,
) -> Result<ImportBatchStatus, String> {
    let batch = {
        let mut host = state
            .0
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?;
        host.create_batch(&request.paths)?
    };
    let snapshot = {
        let host = state
            .0
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?;
        import_batch_status(&host, &batch)?
    };
    tauri::async_runtime::spawn(run_document_batch(app, batch, request));
    Ok(snapshot)
}

async fn run_document_batch(
    app: AppHandle,
    batch: ImportBatch,
    request: StartDocumentBatchRequest,
) {
    let state = app.state::<AgentHostState>();
    let _permit = state.1.acquire().await;
    let mut healthy = true;
    for (job_id, path) in batch.job_ids.iter().zip(request.paths.iter()) {
        let terminal = state
            .0
            .lock()
            .ok()
            .and_then(|host| host.get_job(job_id).map(|job| job.state.is_terminal()))
            .unwrap_or(true);
        if terminal {
            continue;
        }
        let job_request = request.job_request(path.clone());
        let outcome = match resolve_pdf_agent_config(&app, &job_request) {
            Ok(runtime) => {
                run_document_stages(&app, &state.0, job_id, Path::new(path), runtime).await
            }
            Err(error) => Err(error),
        };
        if let Err(error) = outcome {
            healthy = false;
            if let Ok(mut host) = state.0.lock() {
                let terminal = host
                    .get_job(job_id)
                    .map(|job| job.state.is_terminal())
                    .unwrap_or(true);
                if !terminal {
                    let _ = host.advance_job(job_id, JobState::Failed, None, None, Some(&error));
                }
            }
        }
    }
    if healthy {
        state.1.observe(WorkerObservation::Healthy { ticks: 1 });
    } else {
        state
            .1
            .observe(WorkerObservation::Failed(FailureReason::Crash));
    }
}

fn import_batch_status(host: &AgentHost, batch: &ImportBatch) -> Result<ImportBatchStatus, String> {
    let jobs = batch
        .job_ids
        .iter()
        .map(|job_id| {
            host.get_job(job_id)
                .map(PdfJobStatus::from)
                .ok_or_else(|| format!("Job not found: {job_id}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let all_settled = jobs.iter().all(|job| {
        matches!(
            job.state.as_str(),
            "awaiting_review" | "accepted" | "rejected" | "cancelled" | "failed"
        )
    });
    let state = if jobs.iter().all(|job| job.state == "cancelled") {
        "cancelled"
    } else if all_settled {
        if jobs
            .iter()
            .any(|job| matches!(job.state.as_str(), "failed" | "cancelled"))
        {
            "completed_with_errors"
        } else {
            "completed"
        }
    } else if jobs.iter().all(|job| job.state == "queued") {
        "queued"
    } else {
        "running"
    };
    Ok(ImportBatchStatus {
        batch_id: batch.batch_id.clone(),
        state: state.to_string(),
        created_at: batch.created_at,
        updated_at: jobs
            .iter()
            .map(|job| job.updated_at)
            .max()
            .unwrap_or(batch.updated_at),
        jobs,
    })
}

#[tauri::command]
pub fn get_import_batch_status(
    state: State<'_, AgentHostState>,
    batch_id: String,
) -> Result<ImportBatchStatus, String> {
    let host = state
        .0
        .lock()
        .map_err(|error| format!("Lock error: {error}"))?;
    let batch = host
        .get_batch(&batch_id)
        .ok_or_else(|| format!("Batch not found: {batch_id}"))?;
    import_batch_status(&host, batch)
}

#[tauri::command]
pub fn list_import_jobs(state: State<'_, AgentHostState>) -> Result<Vec<PdfJobStatus>, String> {
    let host = state
        .0
        .lock()
        .map_err(|error| format!("Lock error: {error}"))?;
    let mut jobs = host
        .list_jobs()
        .into_iter()
        .map(PdfJobStatus::from)
        .collect::<Vec<_>>();
    jobs.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(jobs)
}

/// 短锁推进一个阶段;并发取消/失败导致转换非法时,若已是终态则返回干净的并发终止错误。
/// Narrow progress boundary for the queued importer. Hypatia's ProgressSink can
/// replace this adapter without coupling provider callbacks to AgentHost.
/// TODO(Hypatia): implement the shared ProgressSink trait here when its API lands.
struct ImportProgressAdapter<'a> {
    hosts: &'a Mutex<AgentHost>,
    job_id: &'a str,
}

/// Provider-neutral hook for Hypatia/native streaming and a future Tauri event
/// emitter. Implementations accept counts and fixed summaries only, never text.
#[allow(dead_code)]
trait NativeProgressSink {
    fn begin_reasoning_pass(&self, pass: &str, safe_summary: &str) -> Result<(), String>;
    fn record_reasoning_chunk(&self, bytes: usize) -> Result<(), String>;
    fn record_reasoning_retry(&self) -> Result<(), String>;
}

#[allow(dead_code)]
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportProgressEvent {
    job_id: String,
    stage: String,
    reasoning_activity: ReasoningActivity,
}

#[derive(Default)]
struct KimiProgressCursor {
    reasoning_attempt: u32,
    reasoning_chunks: u64,
    reasoning_bytes: u64,
    upload_total_bytes: u64,
}

struct KimiJobProgressSink {
    app: AppHandle,
    job_id: String,
    cursor: Mutex<KimiProgressCursor>,
}

impl KimiJobProgressSink {
    fn new(app: AppHandle, job_id: &str) -> Self {
        Self {
            app,
            job_id: job_id.to_string(),
            cursor: Mutex::new(KimiProgressCursor::default()),
        }
    }
}

impl pdf_canvas_agent::ProgressSink for KimiJobProgressSink {
    fn emit(&self, event: pdf_canvas_agent::TransportProgressEvent) {
        use pdf_canvas_agent::{TransportOperation, TransportProgressEvent};

        let state = self.app.state::<AgentHostState>();
        match event {
            TransportProgressEvent::Started {
                operation: TransportOperation::FileUpload,
                total_bytes: Some(total),
                ..
            } => {
                if let Ok(mut cursor) = self.cursor.lock() {
                    cursor.upload_total_bytes = total;
                }
                if let Ok(mut host) = state.0.lock() {
                    let _ = host.record_upload_progress(&self.job_id, 0, total);
                }
            }
            TransportProgressEvent::BytesTransferred {
                operation: TransportOperation::FileUpload,
                transferred_bytes,
                total_bytes,
                ..
            } => {
                if let Ok(mut host) = state.0.lock() {
                    let _ =
                        host.record_upload_progress(&self.job_id, transferred_bytes, total_bytes);
                }
            }
            TransportProgressEvent::Completed {
                operation: TransportOperation::FileUpload,
                transferred_bytes: Some(transferred),
                ..
            } => {
                let total = self
                    .cursor
                    .lock()
                    .map(|cursor| cursor.upload_total_bytes)
                    .unwrap_or(transferred);
                if let Ok(mut host) = state.0.lock() {
                    let _ = host.record_upload_progress(&self.job_id, transferred, total);
                }
            }
            TransportProgressEvent::Started {
                operation: TransportOperation::ChatCompletion,
                attempt,
                ..
            } => {
                if let Ok(mut cursor) = self.cursor.lock() {
                    cursor.reasoning_attempt = attempt;
                    cursor.reasoning_chunks = 0;
                    cursor.reasoning_bytes = 0;
                }
            }
            TransportProgressEvent::ReasoningActivity {
                attempt,
                chunks,
                utf8_bytes,
            } => {
                let (chunk_delta, byte_delta) = match self.cursor.lock() {
                    Ok(mut cursor) => {
                        if cursor.reasoning_attempt != attempt {
                            cursor.reasoning_attempt = attempt;
                            cursor.reasoning_chunks = 0;
                            cursor.reasoning_bytes = 0;
                        }
                        let delta = (
                            chunks.saturating_sub(cursor.reasoning_chunks),
                            utf8_bytes.saturating_sub(cursor.reasoning_bytes),
                        );
                        cursor.reasoning_chunks = chunks;
                        cursor.reasoning_bytes = utf8_bytes;
                        delta
                    }
                    Err(_) => return,
                };
                if let Ok(mut host) = state.0.lock() {
                    let _ = host.record_reasoning_activity(&self.job_id, chunk_delta, byte_delta);
                }
            }
            TransportProgressEvent::PublicProgress {
                stage,
                summary,
                evidence_count,
                warning_count,
            } => {
                if let Ok(mut host) = state.0.lock() {
                    let _ = host.record_public_progress(
                        &self.job_id,
                        stage,
                        summary,
                        evidence_count,
                        warning_count,
                    );
                }
            }
            TransportProgressEvent::Retrying { .. } => {
                if let Ok(mut host) = state.0.lock() {
                    let _ = host.record_reasoning_retry(&self.job_id);
                }
            }
            _ => {}
        }
    }
}

impl ImportProgressAdapter<'_> {
    fn transition(
        &self,
        next: JobState,
        output_hash: Option<&str>,
        data: Option<Value>,
    ) -> Result<(), String> {
        advance_stage(self.hosts, self.job_id, next, output_hash, data)
    }

    fn record_repair_audit(
        &self,
        pass: &str,
        attempt: u32,
        status: &str,
        report: &AuditReport,
        error: Option<String>,
    ) -> Result<(), String> {
        let entries = report
            .entries
            .iter()
            .map(|entry| RepairAuditEntry {
                code: entry.code.clone(),
                path: entry.path.clone(),
                before_summary: entry.before_summary.clone(),
                after_summary: entry.after_summary.clone(),
                severity: match entry.severity {
                    AuditSeverity::Info => "info",
                    AuditSeverity::Warning => "warning",
                    AuditSeverity::Error => "error",
                }
                .to_string(),
                deterministic: entry.deterministic,
            })
            .collect();
        self.hosts
            .lock()
            .map_err(|lock_error| format!("Lock error: {lock_error}"))?
            .record_repair_audit(
                self.job_id,
                RepairAuditRecord {
                    pass: pass.to_string(),
                    attempt,
                    status: status.to_string(),
                    entries,
                    error,
                    created_at: 0,
                },
            )
    }
}

impl NativeProgressSink for ImportProgressAdapter<'_> {
    fn begin_reasoning_pass(&self, pass: &str, safe_summary: &str) -> Result<(), String> {
        self.hosts
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?
            .begin_reasoning_pass(self.job_id, pass, safe_summary)
    }

    fn record_reasoning_chunk(&self, bytes: usize) -> Result<(), String> {
        self.hosts
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?
            .record_reasoning_chunk(self.job_id, bytes)
    }

    fn record_reasoning_retry(&self) -> Result<(), String> {
        self.hosts
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?
            .record_reasoning_retry(self.job_id)
    }
}

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
async fn run_document_stages(
    app: &AppHandle,
    hosts: &Mutex<AgentHost>,
    job_id: &str,
    selected_path: &Path,
    runtime: PdfAgentRuntimeConfig,
) -> Result<(), String> {
    let progress = ImportProgressAdapter { hosts, job_id };
    // ── 阶段 1：ValidatingFile ──
    progress.transition(
        JobState::ValidatingFile,
        Some("v1"),
        Some(runtime.safe_snapshot()),
    )?;

    let path_for_validation = selected_path.to_path_buf();
    let validated = tauri::async_runtime::spawn_blocking(move || {
        AgentHost::validate_document_file(&path_for_validation)
    })
    .await
    .map_err(|error| format!("Validation task join error: {error}"))??;
    {
        let mut host = hosts
            .lock()
            .map_err(|error| format!("Lock error: {error}"))?;
        host.mark_document_validated(job_id, &validated)?;
    }
    let abs_path = validated.path.as_path();
    let document_format = validated.format;

    // ── 阶段 2：ExtractingText ──
    let document_input = {
        if document_format == DocumentFormat::Pdf
            && runtime.llm.transport == PdfAgentTransport::KimiFileExtract
        {
            let text = match &runtime.backend {
                pdf_canvas_agent::Backend::KimiK26(config) => {
                    let sink = Arc::new(KimiJobProgressSink::new(app.clone(), job_id));
                    let client =
                        pdf_canvas_agent::KimiK26Client::new_with_progress(config.clone(), sink)
                            .map_err(|error| error.to_string())?;
                    client
                        .extract_pdf_text(abs_path)
                        .await
                        .map_err(|error| format!("Kimi file extraction failed: {error}"))?
                }
                pdf_canvas_agent::Backend::Generic => {
                    let client = PdfAgentLlmClient::new(runtime.llm.clone())
                        .map_err(|error| error.to_string())?;
                    client
                        .extract_kimi_file_text(abs_path)
                        .await
                        .map_err(|error| format!("PDF file extraction failed: {error}"))?
                }
            };
            DocumentInput {
                format: DocumentFormat::Pdf,
                source_name: abs_path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("document.pdf")
                    .to_string(),
                media_type: "application/pdf".to_string(),
                extracted: PdfPipeline::from_plain_text(text),
            }
        } else {
            let path = abs_path.to_path_buf();
            tauri::async_runtime::spawn_blocking(move || {
                PdfPipeline::extract_document(&path, document_format)
            })
            .await
            .map_err(|e| format!("Pipeline task join error: {e}"))??
        }
    };
    let extracted = document_input.extracted.clone();
    let text_hash = format!("{:x}", sha2::Sha256::digest(extracted.full_text.as_bytes()));
    progress.transition(
        JobState::ExtractingText,
        Some(&text_hash),
        Some(serde_json::to_value(&document_input).map_err(|e| e.to_string())?),
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
    progress.transition(
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
    progress.transition(
        JobState::BuildingDocumentMap,
        Some(&doc_hash),
        Some(serde_json::to_value(&doc).map_err(|e| e.to_string())?),
    )?;

    // ── 阶段 5：ExtractingSemantics ──
    // The host sends bounded text messages to the configured LLM. The PDF
    // bytes have already ended at local extraction (or Kimi text extraction).
    let patch = run_semantic_graph_pipeline(
        app,
        &doc,
        &final_extracted,
        &runtime,
        job_id,
        document_format,
        &progress,
    )
    .await?;
    let semantic_hash = format!(
        "{:x}",
        sha2::Sha256::digest(serde_json::to_string(&patch).unwrap_or_default().as_bytes())
    );
    progress.transition(
        JobState::ExtractingSemantics,
        Some(&semantic_hash),
        Some(patch.clone()),
    )?;

    // ── 阶段 6：GeneratingPatch ──
    progress.transition(
        JobState::GeneratingPatch,
        Some(&semantic_hash),
        Some(patch.clone()),
    )?;

    // ── 阶段 7:AwaitingReview(data 即审阅载荷,advance_job 写入 job.result)──
    progress.transition(JobState::AwaitingReview, Some(&semantic_hash), Some(patch))?;
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

async fn parse_pass_with_auditable_repair<T: DeserializeOwned>(
    raw_output: &str,
    pass: &str,
    schema_contract: &str,
    recovery_provider: &dyn LlmProvider,
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
                "Pass: {pass}\nRequired contract: {schema_contract}\nParser error: {error}\nCandidate JSON:\n{bounded_candidate}"
            );
            let recovered = match recovery_provider.chat(system, &user, ResponseFormat::Json).await {
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
            severity: AuditSeverity::Warning,
            deterministic: false,
        },
    );
    report
}

async fn run_semantic_graph_pipeline(
    app: &AppHandle,
    doc: &crate::pdf_pipeline::StructuredDocument,
    extracted: &crate::pdf_pipeline::ExtractedText,
    runtime: &PdfAgentRuntimeConfig,
    job_id: &str,
    document_format: DocumentFormat,
    progress: &ImportProgressAdapter<'_>,
) -> Result<Value, String> {
    let prompts_dir = resolve_prompts_dir(app)?;
    let config = PipelineConfig::load(&prompts_dir, "en").map_err(|error| error.to_string())?;
    let pipeline = Pipeline::new(config);
    let native_kimi_progress = matches!(&runtime.backend, pdf_canvas_agent::Backend::KimiK26(_));
    let client: Arc<dyn crate::llm_client::LlmClient> = match &runtime.backend {
        pdf_canvas_agent::Backend::KimiK26(config) => {
            let sink = Arc::new(KimiJobProgressSink::new(app.clone(), job_id));
            Arc::new(
                pdf_canvas_agent::KimiK26Client::new_with_progress(config.clone(), sink)
                    .map_err(|error| error.to_string())?,
            )
        }
        pdf_canvas_agent::Backend::Generic => Arc::new(
            PdfAgentLlmClient::new(runtime.llm.clone()).map_err(|error| error.to_string())?,
        ),
    };
    let provider = LlmClientAdapter::new(client.clone(), CallRole::Extraction, job_id.to_string());
    let recovery_provider =
        LlmClientAdapter::new(client, CallRole::Recovery, format!("{job_id}:recovery"));
    let bounded_text = PdfPipeline::bounded_llm_context(&extracted.full_text);
    let document_json = serde_json::to_string(doc).map_err(|error| error.to_string())?;
    let public_progress_protocol = if runtime.public_progress
        && matches!(&runtime.backend, pdf_canvas_agent::Backend::KimiK26(_))
    {
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

    progress.begin_reasoning_pass("pass-a-structure", "Analyzing document structure")?;
    let mut pass_a_vars = pipeline.prepare_pass_a_input(&bounded_text, &document_json);
    pass_a_vars.insert(
        "public_progress_protocol".into(),
        public_progress_protocol.into(),
    );
    let pass_a_raw = pipeline
        .call_llm("structure-extraction", &pass_a_vars, &provider)
        .await
        .map_err(|error| format!("Pass A structure extraction failed: {error}"))?;
    if !native_kimi_progress {
        progress.record_reasoning_chunk(pass_a_raw.len())?;
    }
    let structure: semantic_pipeline::StructureExtraction = parse_pass_with_auditable_repair(
        &pass_a_raw,
        "A",
        "object with title, authors[], optional year, abstractText, sections[], references[], and meta",
        &recovery_provider,
        progress,
    )
    .await?;

    // The bounded context is assembled from local text chunks. One bounded
    // Pass B call avoids duplicate temp ids that would otherwise be produced
    // by independently chunked entity calls.
    progress.begin_reasoning_pass("pass-b-entities", "Identifying research entities")?;
    let mut pass_b_vars =
        pipeline.prepare_pass_b_input(&document_json, "bounded PDF text chunks", &bounded_text);
    pass_b_vars.insert(
        "public_progress_protocol".into(),
        public_progress_protocol.into(),
    );
    let pass_b_raw = pipeline
        .call_llm("entity-extraction", &pass_b_vars, &provider)
        .await
        .map_err(|error| format!("Pass B entity extraction failed: {error}"))?;
    if !native_kimi_progress {
        progress.record_reasoning_chunk(pass_b_raw.len())?;
    }
    let entities: semantic_pipeline::EntityExtraction = parse_pass_with_auditable_repair(
        &pass_b_raw,
        "B",
        "object with entities[] and meta; never invent entity facts, anchors, or confidence",
        &recovery_provider,
        progress,
    )
    .await?;
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
    progress.begin_reasoning_pass("pass-c-variables", "Mapping variables and experiments")?;
    let mut pass_c_vars = pipeline.prepare_pass_c_input(
        &entities_json,
        &truncate_chars(&experiment_paragraphs, 40_000),
        &bounded_text,
    );
    pass_c_vars.insert(
        "public_progress_protocol".into(),
        public_progress_protocol.into(),
    );
    let pass_c_raw = pipeline
        .call_llm("variable-fission", &pass_c_vars, &provider)
        .await
        .map_err(|error| format!("Pass C variable extraction failed: {error}"))?;
    if !native_kimi_progress {
        progress.record_reasoning_chunk(pass_c_raw.len())?;
    }
    let variable_fission: semantic_pipeline::VariableFissionResult =
        parse_pass_with_auditable_repair(
            &pass_c_raw,
            "C",
            "object with experimentMatrix[] and variableRegistry[]",
            &recovery_provider,
            progress,
        )
        .await?;
    let variable_json =
        serde_json::to_string(&variable_fission).map_err(|error| error.to_string())?;

    progress.begin_reasoning_pass("pass-d-merge", "Merging cross-section evidence")?;
    let mut pass_d_vars =
        pipeline.prepare_pass_d_input(&entities_json, &variable_json, &bounded_text);
    pass_d_vars.insert(
        "public_progress_protocol".into(),
        public_progress_protocol.into(),
    );
    let pass_d_raw = pipeline
        .call_llm("cross-segment-merge", &pass_d_vars, &provider)
        .await
        .map_err(|error| format!("Pass D merge failed: {error}"))?;
    if !native_kimi_progress {
        progress.record_reasoning_chunk(pass_d_raw.len())?;
    }
    let merge_result: semantic_pipeline::CrossSegmentMergeResult =
        parse_pass_with_auditable_repair(
            &pass_d_raw,
            "D",
            "object with mergeGroups[], claimEvidenceBundles[], metricAlignment[], and datasetRegistry[]",
            &recovery_provider,
            progress,
        )
        .await?;
    let merge_json = serde_json::to_string(&merge_result).map_err(|error| error.to_string())?;

    progress.begin_reasoning_pass("pass-e-synthesis", "Synthesizing the review proposal")?;
    let mut pass_e_vars = pipeline.prepare_pass_e_input(
        structure.title.as_deref().unwrap_or("Untitled paper"),
        &structure.authors.join(", "),
        &serde_json::to_string(&structure).map_err(|error| error.to_string())?,
        &entities_json,
        &variable_json,
        &merge_json,
        &bounded_text,
    );
    pass_e_vars.insert(
        "public_progress_protocol".into(),
        public_progress_protocol.into(),
    );
    let pass_e_raw = pipeline
        .call_llm("paper-level-synthesis", &pass_e_vars, &provider)
        .await
        .map_err(|error| format!("Pass E synthesis failed: {error}"))?;
    if !native_kimi_progress {
        progress.record_reasoning_chunk(pass_e_raw.len())?;
    }
    let synthesis: semantic_pipeline::PaperSynthesisResult = parse_pass_with_auditable_repair(
        &pass_e_raw,
        "E",
        "object with mainConclusions[], ablationAnalysis[], interactionEffects[], confounders[], missingControls[], internalConflicts[], and synthesisSummary",
        &recovery_provider,
        progress,
    )
    .await?;

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
    let source = if document_format == DocumentFormat::Pdf {
        "myc.pdf-canvas-agent"
    } else {
        "host.document-import"
    };
    let patch = graphpatch_gen::build_graph_patch(&candidates, source);
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
            backend: pdf_canvas_agent::Backend::Generic,
            public_progress: false,
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

    #[test]
    fn agent_gate_admits_two_concurrent_batches_and_fails_closed_on_the_third() {
        let scheduler = Arc::new(RwLock::new(
            crate::kernel::scheduler::Scheduler::with_default_quota(2)
                .expect("quota two is valid"),
        ));
        let supervisor = Arc::new(RwLock::new(Supervisor::default()));
        let gate = AgentJobGate::new(
            Arc::clone(&scheduler),
            Arc::clone(&supervisor),
            WorkerId::new("worker.agent-host.test").expect("worker id"),
            PrincipalId::new("worker.agent-host.test").expect("principal"),
            2,
        );

        // Two independent batches occupy the quota concurrently.
        let first = tauri::async_runtime::block_on(gate.acquire());
        let second = tauri::async_runtime::block_on(gate.acquire());
        assert_eq!(
            scheduler
                .read()
                .expect("scheduler read lock")
                .inflight(gate.principal()),
            2
        );

        // A third batch for the same principal fails closed at the shared
        // accounting plane; the semaphore is sized to the same quota.
        assert_eq!(
            scheduler
                .write()
                .expect("scheduler write lock")
                .acquire(gate.principal()),
            Err(crate::kernel::scheduler::SchedulerError::QuotaExhausted {
                principal: gate.principal().clone(),
                quota: 2,
            })
        );

        // Dropping permits drains the ledger so slots return.
        drop(second);
        assert_eq!(
            scheduler
                .read()
                .expect("scheduler read lock")
                .inflight(gate.principal()),
            1
        );
        drop(first);
        assert_eq!(
            scheduler
                .read()
                .expect("scheduler read lock")
                .inflight(gate.principal()),
            0
        );
    }
}
