//! Native boundary for the built-in PDF Canvas Agent.
//!
//! Kimi K2.6 exposes OpenAI- and Anthropic-compatible chat surfaces with a
//! small but important contract: it uses provider-specific streaming events,
//! rejects arbitrary sampling values, and extracts PDFs through the OpenAI-like
//! Files API before sending text to chat. Keeping that contract here prevents
//! the generic LLM kernel from accumulating Kimi-specific request branches.

use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, ReadBuf};
use tokio_util::io::ReaderStream;

use crate::llm_client::{
    is_terminal_http_status, ApiFormat, CallRole, LlmClient, LlmError, PdfAgentLlmConfig,
    PdfAgentTransport, MAX_RESPONSE_BODY_BYTES, MAX_RETRIES,
};

pub const KIMI_K26_MODEL: &str = "kimi-k2.6";
pub const KIMI_CN_BASE_URL: &str = "https://api.moonshot.cn/v1";
pub const KIMI_GLOBAL_BASE_URL: &str = "https://api.moonshot.ai/v1";
pub const KIMI_CN_ANTHROPIC_BASE_URL: &str = "https://api.moonshot.cn/anthropic";
pub const KIMI_GLOBAL_ANTHROPIC_BASE_URL: &str = "https://api.moonshot.ai/anthropic";
pub const KIMI_DEFAULT_CREDENTIAL_ENV: &str = "MOONSHOT_API_KEY";
const MAX_PDF_UPLOAD_BYTES: u64 = 50 * 1024 * 1024;
const UPLOAD_CHUNK_BYTES: usize = 64 * 1024;
// SSE carries one JSON envelope per delta, so wire bytes can be much larger
// than the assembled model content (especially with thinking enabled).
const MAX_KIMI_SSE_WIRE_BYTES: usize = 16 * 1024 * 1024;
const MAX_PUBLIC_PROGRESS_EVENTS: usize = 6;
const MAX_PUBLIC_PROGRESS_STAGE_CHARS: usize = 64;
const MAX_PUBLIC_PROGRESS_SUMMARY_CHARS: usize = 240;
const MAX_PUBLIC_PROGRESS_PAYLOAD_BYTES: usize = 8 * 1024;

const MYC_PROGRESS_OPEN: &str = "<myc_progress>";
const MYC_PROGRESS_CLOSE: &str = "</myc_progress>";
const MYC_RESULT_OPEN: &str = "<myc_result>";
const MYC_RESULT_CLOSE: &str = "</myc_result>";

/// Provider-neutral transport operations. The host may map these operations
/// to job stages without teaching this adapter about Tauri or a specific UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransportOperation {
    FileUpload,
    FileExtraction,
    ChatCompletion,
}

/// Provider-neutral, safe-to-display transport telemetry.
///
/// Deliberately contains no prompt, completion, API key, file content, or raw
/// reasoning text. Reasoning is represented only as aggregate activity.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TransportProgressEvent {
    Started {
        operation: TransportOperation,
        attempt: u32,
        total_bytes: Option<u64>,
    },
    BytesTransferred {
        operation: TransportOperation,
        attempt: u32,
        transferred_bytes: u64,
        total_bytes: u64,
    },
    ReasoningActivity {
        attempt: u32,
        chunks: u64,
        utf8_bytes: u64,
    },
    ContentActivity {
        attempt: u32,
        chunks: u64,
        utf8_bytes: u64,
    },
    /// A model-authored, user-visible progress summary from ordinary content.
    /// This is deliberately separate from provider thinking/reasoning events.
    PublicProgress {
        stage: String,
        summary: String,
        evidence_count: Option<u64>,
        warning_count: Option<u64>,
    },
    Completed {
        operation: TransportOperation,
        attempt: u32,
        transferred_bytes: Option<u64>,
    },
    Retrying {
        operation: TransportOperation,
        completed_attempt: u32,
        max_attempts: u32,
        delay_ms: u64,
    },
}

/// A non-blocking observer implemented by the host. Implementations should
/// enqueue events quickly; network work invokes this synchronously.
pub trait ProgressSink: Send + Sync {
    fn emit(&self, event: TransportProgressEvent);
}

impl<F> ProgressSink for F
where
    F: Fn(TransportProgressEvent) + Send + Sync,
{
    fn emit(&self, event: TransportProgressEvent) {
        self(event);
    }
}

#[derive(Default)]
struct NoopProgressSink;

impl ProgressSink for NoopProgressSink {
    fn emit(&self, _event: TransportProgressEvent) {}
}

struct ProgressReader<R> {
    inner: R,
    sink: Arc<dyn ProgressSink>,
    transferred: u64,
    total: u64,
    attempt: u32,
}

impl<R> ProgressReader<R> {
    fn new(inner: R, sink: Arc<dyn ProgressSink>, total: u64, attempt: u32) -> Self {
        Self {
            inner,
            sink,
            transferred: 0,
            total,
            attempt,
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ProgressReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buffer.filled().len();
        match Pin::new(&mut self.inner).poll_read(cx, buffer) {
            Poll::Ready(Ok(())) => {
                let read = buffer.filled().len().saturating_sub(before) as u64;
                if read > 0 {
                    self.transferred = self.transferred.saturating_add(read).min(self.total);
                    self.sink.emit(TransportProgressEvent::BytesTransferred {
                        operation: TransportOperation::FileUpload,
                        attempt: self.attempt,
                        transferred_bytes: self.transferred,
                        total_bytes: self.total,
                    });
                }
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

/// Provider-neutral thinking mode exposed to the eventual generic request
/// policy hook in `llm_client.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThinkingMode {
    Enabled,
    Disabled,
}

impl ThinkingMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// Provider-neutral request policy. `None` is intentional: K2.6 should use
/// the service defaults and must not receive arbitrary sampling parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct RequestPolicy {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub thinking: Option<ThinkingMode>,
}

impl RequestPolicy {
    fn kimi_k26(thinking: bool) -> Self {
        Self {
            temperature: None,
            max_tokens: None,
            thinking: Some(if thinking {
                ThinkingMode::Enabled
            } else {
                ThinkingMode::Disabled
            }),
        }
    }
}

/// The normalized result consumed by `agent_commands`.
#[derive(Clone, Debug)]
pub struct NormalizedConfig {
    pub llm: PdfAgentLlmConfig,
    pub request_policy: RequestPolicy,
    pub backend: Backend,
}

#[derive(Clone, Debug)]
pub enum Backend {
    /// Use the existing generic client for non-Kimi providers.
    Generic,
    /// Use this module's native K2.6 client and file-extraction boundary.
    KimiK26(KimiK26Config),
}

#[derive(Clone, Debug)]
pub struct KimiK26Config {
    pub api_url: String,
    pub api_format: ApiFormat,
    pub api_key: String,
    pub model: String,
    pub thinking: bool,
    pub transport: PdfAgentTransport,
    pub timeout_secs: u64,
    pub request_policy: RequestPolicy,
    /// Enables ordinary-content `<myc_progress>` framing. Hidden provider
    /// thinking remains aggregate-only regardless of this flag.
    pub public_progress: bool,
}

/// Normalize a resolved host config at the native-plugin boundary.
///
/// Kimi recognition, defaults, and validation intentionally live here. A
/// non-Kimi config is validated by the existing generic client contract and
/// remains on the generic backend.
pub fn normalize_config(mut config: PdfAgentLlmConfig) -> Result<NormalizedConfig, String> {
    if !looks_like_kimi(&config) {
        config.validate().map_err(|error| error.to_string())?;
        return Ok(NormalizedConfig {
            llm: config,
            request_policy: RequestPolicy {
                temperature: None,
                max_tokens: None,
                thinking: None,
            },
            backend: Backend::Generic,
        });
    }

    let api_url = normalize_kimi_endpoint(&config.api_url, config.api_format)?;
    let model = normalize_kimi_model(&config.model)?;
    let thinking = config.thinking;
    let request_policy = RequestPolicy::kimi_k26(thinking);

    config.api_url = api_url.clone();
    config.model = model.clone();
    config.provider = "moonshot".to_string();
    // Reasoning levels do not exist for K2.6; only the boolean is forwarded.
    config.thinking_level = None;

    validate_kimi_config(&config)?;

    let kimi = KimiK26Config {
        api_url,
        api_format: config.api_format,
        api_key: config.api_key.clone(),
        model,
        thinking,
        transport: config.transport,
        timeout_secs: config.timeout_secs,
        request_policy: request_policy.clone(),
        public_progress: false,
    };

    Ok(NormalizedConfig {
        llm: config,
        request_policy,
        backend: Backend::KimiK26(kimi),
    })
}

/// Choose the credential environment variable before the full config is
/// normalized. This keeps provider recognition out of `agent_commands`.
pub fn default_credential_env_var(api_url: &str, provider: &str, model: &str) -> &'static str {
    if looks_like_kimi_hint(api_url, provider, model) {
        KIMI_DEFAULT_CREDENTIAL_ENV
    } else {
        "DEEPSEEK_API_KEY"
    }
}

/// Kimi's official regional OpenAI and Anthropic-compatible API base URLs.
pub fn is_approved_endpoint(api_url: &str) -> bool {
    matches!(
        api_url.trim_end_matches('/'),
        KIMI_CN_BASE_URL
            | KIMI_GLOBAL_BASE_URL
            | KIMI_CN_ANTHROPIC_BASE_URL
            | KIMI_GLOBAL_ANTHROPIC_BASE_URL
    )
}

fn looks_like_kimi(config: &PdfAgentLlmConfig) -> bool {
    looks_like_kimi_hint(&config.api_url, &config.provider, &config.model)
}

fn looks_like_kimi_hint(api_url: &str, provider: &str, model: &str) -> bool {
    let provider = provider.trim().to_ascii_lowercase();
    let model = model.trim().to_ascii_lowercase();
    let url = api_url.trim().to_ascii_lowercase();
    provider.contains("kimi")
        || provider.contains("moonshot")
        || model == KIMI_K26_MODEL
        || url.contains("api.moonshot.cn")
        || url.contains("api.moonshot.ai")
}

fn normalize_kimi_endpoint(api_url: &str, api_format: ApiFormat) -> Result<String, String> {
    let normalized = api_url.trim().trim_end_matches('/');
    let matches_format = match api_format {
        ApiFormat::OpenAi => matches!(normalized, KIMI_CN_BASE_URL | KIMI_GLOBAL_BASE_URL),
        ApiFormat::Anthropic => matches!(
            normalized,
            KIMI_CN_ANTHROPIC_BASE_URL | KIMI_GLOBAL_ANTHROPIC_BASE_URL
        ),
    };
    if matches_format {
        Ok(normalized.to_string())
    } else {
        let expected = match api_format {
            ApiFormat::OpenAi => format!("{KIMI_CN_BASE_URL} or {KIMI_GLOBAL_BASE_URL}"),
            ApiFormat::Anthropic => {
                format!("{KIMI_CN_ANTHROPIC_BASE_URL} or {KIMI_GLOBAL_ANTHROPIC_BASE_URL}")
            }
        };
        Err(format!(
            "The selected Kimi parser backend requires {expected}"
        ))
    }
}

fn normalize_kimi_model(model: &str) -> Result<String, String> {
    let model = model.trim();
    if model.is_empty() || model.starts_with("deepseek-") {
        return Ok(KIMI_K26_MODEL.to_string());
    }
    if model.eq_ignore_ascii_case(KIMI_K26_MODEL) {
        return Ok(KIMI_K26_MODEL.to_string());
    }
    Err(format!(
        "The native PDF Canvas Agent supports Kimi model {KIMI_K26_MODEL}; received {model}"
    ))
}

fn validate_kimi_config(config: &PdfAgentLlmConfig) -> Result<(), String> {
    if config.api_key.trim().is_empty() {
        return Err("API key is not configured".to_string());
    }
    if !is_approved_endpoint(&config.api_url) {
        return Err("Kimi API URL is not approved".to_string());
    }
    if config.model != KIMI_K26_MODEL {
        return Err(format!("Kimi model must be {KIMI_K26_MODEL}"));
    }
    Ok(())
}

/// Build the exact OpenAI-compatible body used by the native K2.6 adapter.
///
/// This is public so the main thread can later feed `RequestPolicy` into a
/// provider-neutral hook without moving Kimi recognition back into the
/// generic kernel.
pub fn build_chat_request(
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    policy: &RequestPolicy,
) -> Value {
    build_chat_request_with_public_progress(model, system_prompt, user_prompt, policy, false)
}

fn build_chat_request_with_public_progress(
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    policy: &RequestPolicy,
    public_progress: bool,
) -> Value {
    let mut body = json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "response_format": {"type": "json_object"},
        "stream": true
    });
    if let Some(temperature) = policy.temperature {
        body["temperature"] = json!(temperature);
    }
    if let Some(max_tokens) = policy.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }
    if let Some(thinking) = policy.thinking {
        body["thinking"] = json!({"type": thinking.as_str()});
    }
    if public_progress {
        // OpenAI JSON mode forbids the protocol's XML-like framing. The
        // stream filter still extracts and validates the final JSON frame.
        body.as_object_mut()
            .expect("chat request body is an object")
            .remove("response_format");
    }
    body
}

/// Build the Anthropic Messages-compatible request used by Kimi's
/// `/anthropic/v1/messages` surface. Unlike OpenAI JSON mode, the Anthropic
/// protocol relies on the pass prompt/schema to constrain the JSON result.
pub fn build_anthropic_chat_request(
    model: &str,
    system_prompt: &str,
    user_prompt: &str,
    policy: &RequestPolicy,
) -> Value {
    let max_tokens = policy.max_tokens.unwrap_or(32_768);
    let mut body = json!({
        "model": model,
        "system": system_prompt,
        "messages": [{"role": "user", "content": user_prompt}],
        "max_tokens": max_tokens,
        "stream": true
    });
    if matches!(policy.thinking, Some(ThinkingMode::Enabled)) {
        body["thinking"] = json!({
            "type": "enabled",
            "budget_tokens": 16_000_u32.min(max_tokens.saturating_sub(1))
        });
    }
    body
}

pub struct KimiK26Client {
    http: reqwest::Client,
    config: KimiK26Config,
    progress: Arc<dyn ProgressSink>,
}

impl KimiK26Client {
    pub fn new(config: KimiK26Config) -> Result<Self, LlmError> {
        Self::new_with_progress(config, Arc::new(NoopProgressSink))
    }

    /// Creates a client with host-owned observability. Existing callers may
    /// continue using `new`; `agent_commands` can opt in without changing the
    /// generic LLM trait or exposing provider-specific event types to Vue.
    pub fn new_with_progress(
        config: KimiK26Config,
        progress: Arc<dyn ProgressSink>,
    ) -> Result<Self, LlmError> {
        let llm = PdfAgentLlmConfig {
            api_url: config.api_url.clone(),
            api_format: config.api_format,
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            thinking: config.thinking,
            thinking_level: None,
            provider: "moonshot".to_string(),
            transport: config.transport,
            timeout_secs: config.timeout_secs,
        };
        validate_kimi_config(&llm).map_err(|body| LlmError::ClientError { status: 0, body })?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs.max(1)))
            .build()
            .map_err(|error| LlmError::HttpError(error.to_string()))?;
        Ok(Self {
            http,
            config,
            progress,
        })
    }

    pub async fn extract_pdf_text(&self, pdf_path: &Path) -> Result<String, LlmError> {
        if self.config.transport != PdfAgentTransport::KimiFileExtract {
            return Err(LlmError::ClientError {
                status: 0,
                body: "Kimi PDF extraction requires the Files API transport".to_string(),
            });
        }

        let metadata = tokio::fs::metadata(pdf_path)
            .await
            .map_err(|error| LlmError::HttpError(format!("Cannot inspect PDF upload: {error}")))?;
        let total_bytes = metadata.len();
        if total_bytes == 0 || total_bytes > MAX_PDF_UPLOAD_BYTES {
            return Err(LlmError::ClientError {
                status: 0,
                body: format!(
                    "PDF upload size must be between 1 and {MAX_PDF_UPLOAD_BYTES} bytes; got {total_bytes}"
                ),
            });
        }

        let upload_body = {
            let mut completed = None;
            for attempt in 1..=MAX_RETRIES {
                match self.upload_file_once(pdf_path, total_bytes, attempt).await {
                    Ok(body) => {
                        completed = Some(body);
                        break;
                    }
                    Err(error) => {
                        if !is_retryable_transport_error(&error) || attempt >= MAX_RETRIES {
                            return Err(error);
                        }
                        let delay_ms = 500 * u64::from(attempt);
                        self.progress.emit(TransportProgressEvent::Retrying {
                            operation: TransportOperation::FileUpload,
                            completed_attempt: attempt,
                            max_attempts: MAX_RETRIES,
                            delay_ms,
                        });
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
            completed.ok_or_else(|| LlmError::RetryExhausted {
                attempts: MAX_RETRIES,
                last_error: "Kimi file upload failed".to_string(),
            })?
        };
        let upload_json: Value = serde_json::from_str(&upload_body)
            .map_err(|error| LlmError::ParseError(format!("Kimi file upload response: {error}")))?;
        let file_id = upload_json
            .get("id")
            .or_else(|| upload_json.get("file_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                LlmError::ParseError("Kimi file upload response omitted file id".to_string())
            })?
            .to_string();

        self.progress.emit(TransportProgressEvent::Started {
            operation: TransportOperation::FileExtraction,
            attempt: 1,
            total_bytes: None,
        });
        let result = self.download_file_text(&file_id).await;
        if let Ok(text) = &result {
            self.progress.emit(TransportProgressEvent::Completed {
                operation: TransportOperation::FileExtraction,
                attempt: 1,
                transferred_bytes: Some(text.len() as u64),
            });
        }
        let _ = self.delete_file(&file_id).await;
        result
    }

    async fn upload_file_once(
        &self,
        pdf_path: &Path,
        total_bytes: u64,
        attempt: u32,
    ) -> Result<String, LlmError> {
        self.progress.emit(TransportProgressEvent::Started {
            operation: TransportOperation::FileUpload,
            attempt,
            total_bytes: Some(total_bytes),
        });
        let file = tokio::fs::File::open(pdf_path)
            .await
            .map_err(|error| LlmError::HttpError(format!("Cannot open PDF upload: {error}")))?;
        let reader = ProgressReader::new(file, Arc::clone(&self.progress), total_bytes, attempt);
        let stream = ReaderStream::with_capacity(reader, UPLOAD_CHUNK_BYTES);
        let body = reqwest::Body::wrap_stream(stream);
        let file_name = upload_file_name(pdf_path);
        let part = reqwest::multipart::Part::stream_with_length(body, total_bytes)
            .file_name(file_name)
            .mime_str("application/pdf")
            .map_err(|error| {
                LlmError::HttpError(format!("Cannot prepare PDF MIME part: {error}"))
            })?;
        let form = reqwest::multipart::Form::new()
            .text("purpose", "file-extract")
            .part("file", part);
        let mut headers = self.files_auth_headers()?;
        headers.remove(CONTENT_TYPE);
        let response = self
            .http
            .post(self.files_endpoint())
            .headers(headers)
            .multipart(form)
            .send()
            .await?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(http_error(status, bounded_response_text(response).await));
        }
        let response_body = bounded_response_text(response).await;
        self.progress.emit(TransportProgressEvent::Completed {
            operation: TransportOperation::FileUpload,
            attempt,
            transferred_bytes: Some(total_bytes),
        });
        Ok(response_body)
    }

    fn files_endpoint(&self) -> String {
        let files_base = if self.config.api_url.contains("api.moonshot.cn") {
            KIMI_CN_BASE_URL
        } else {
            KIMI_GLOBAL_BASE_URL
        };
        format!("{files_base}/files")
    }

    fn chat_endpoint(&self) -> String {
        match self.config.api_format {
            ApiFormat::OpenAi => format!(
                "{}/chat/completions",
                self.config.api_url.trim_end_matches('/')
            ),
            ApiFormat::Anthropic => {
                format!("{}/v1/messages", self.config.api_url.trim_end_matches('/'))
            }
        }
    }

    fn files_auth_headers(&self) -> Result<HeaderMap, LlmError> {
        let mut headers = HeaderMap::new();
        let authorization = format!("Bearer {}", self.config.api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization)
                .map_err(|error| LlmError::HttpError(format!("Invalid API key: {error}")))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }

    fn chat_auth_headers(&self) -> Result<HeaderMap, LlmError> {
        if self.config.api_format == ApiFormat::OpenAi {
            return self.files_auth_headers();
        }
        let mut headers = HeaderMap::new();
        // Kimi's official Claude Code integration uses ANTHROPIC_AUTH_TOKEN,
        // which maps to Bearer auth on the Anthropic-compatible endpoint.
        let authorization = format!("Bearer {}", self.config.api_key);
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&authorization)
                .map_err(|error| LlmError::HttpError(format!("Invalid API key: {error}")))?,
        );
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }

    async fn download_file_text(&self, file_id: &str) -> Result<String, LlmError> {
        let response = self
            .http
            .get(format!("{}/{}/content", self.files_endpoint(), file_id))
            .headers(self.files_auth_headers()?)
            .send()
            .await?;
        let status = response.status().as_u16();
        let body = bounded_response_text(response).await;
        if !(200..300).contains(&status) {
            return Err(http_error(status, body));
        }
        if body.trim().is_empty() {
            return Err(LlmError::ParseError(
                "Kimi PDF extraction returned empty text".to_string(),
            ));
        }
        Ok(body)
    }

    async fn delete_file(&self, file_id: &str) -> Result<(), LlmError> {
        let response = self
            .http
            .delete(format!("{}/{}", self.files_endpoint(), file_id))
            .headers(self.files_auth_headers()?)
            .send()
            .await?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(())
        } else {
            Err(http_error(status, bounded_response_text(response).await))
        }
    }

    async fn chat_once(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        attempt: u32,
    ) -> Result<String, LlmError> {
        self.progress.emit(TransportProgressEvent::Started {
            operation: TransportOperation::ChatCompletion,
            attempt,
            total_bytes: None,
        });
        let body = match self.config.api_format {
            ApiFormat::OpenAi => build_chat_request_with_public_progress(
                &self.config.model,
                system_prompt,
                user_prompt,
                &self.config.request_policy,
                self.config.public_progress,
            ),
            ApiFormat::Anthropic => build_anthropic_chat_request(
                &self.config.model,
                system_prompt,
                user_prompt,
                &self.config.request_policy,
            ),
        };
        let response = self
            .http
            .post(self.chat_endpoint())
            .headers(self.chat_auth_headers()?)
            .json(&body)
            .send()
            .await?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(http_error(status, bounded_response_text(response).await));
        }
        let content = match self.config.api_format {
            ApiFormat::OpenAi => read_kimi_sse(response, &self.progress, attempt).await?,
            ApiFormat::Anthropic => {
                read_kimi_anthropic_sse(response, &self.progress, attempt).await?
            }
        };
        self.progress.emit(TransportProgressEvent::Completed {
            operation: TransportOperation::ChatCompletion,
            attempt,
            transferred_bytes: Some(content.len() as u64),
        });
        Ok(crate::llm_client::extract_json_substring(&content)
            .unwrap_or_else(|| content.trim().to_string()))
    }
}

#[async_trait]
impl LlmClient for KimiK26Client {
    async fn chat(
        &self,
        _role: CallRole,
        system_prompt: &str,
        user_prompt: &str,
        _project_id: &str,
        _reasoning_key: Option<&str>,
    ) -> Result<String, LlmError> {
        let mut last_error = None;
        for attempt in 1..=MAX_RETRIES {
            match self.chat_once(system_prompt, user_prompt, attempt).await {
                Ok(content) => return Ok(content),
                Err(error) => {
                    let retryable = is_retryable_transport_error(&error);
                    if !retryable || attempt == MAX_RETRIES {
                        return Err(error);
                    }
                    last_error = Some(error);
                    let delay_ms = 500 * u64::from(attempt);
                    self.progress.emit(TransportProgressEvent::Retrying {
                        operation: TransportOperation::ChatCompletion,
                        completed_attempt: attempt,
                        max_attempts: MAX_RETRIES,
                        delay_ms,
                    });
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
        Err(last_error.unwrap_or_else(|| LlmError::RetryExhausted {
            attempts: MAX_RETRIES,
            last_error: "Kimi request failed".to_string(),
        }))
    }

    fn provider_name(&self) -> &str {
        "Moonshot Kimi"
    }

    fn model_for_role(&self, _role: CallRole) -> &str {
        &self.config.model
    }

    fn provider_id(&self) -> &str {
        "native-moonshot-kimi-k2.6"
    }
}

fn upload_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("document.pdf")
        .to_string()
}

fn is_retryable_transport_error(error: &LlmError) -> bool {
    match error {
        LlmError::RateLimited | LlmError::ServerError { .. } | LlmError::HttpError(_) => true,
        // A stream that terminates before its completion marker (or never
        // delivers content) is a provider-side transport anomaly: the partial
        // response is discarded either way, so a bounded retry is safe and
        // keeps per-section extraction from failing a whole job on one cut
        // stream.
        LlmError::ParseError(message) => {
            message.starts_with("Kimi SSE ended before")
                || message.starts_with("Kimi Anthropic SSE ended before")
                || message.starts_with("Kimi SSE did not contain content deltas")
                || message.starts_with("Kimi Anthropic SSE did not contain text deltas")
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicContentMode {
    Normal,
    Progress,
    Result,
}

impl Default for PublicContentMode {
    fn default() -> Self {
        Self::Normal
    }
}

/// Removes the optional public protocol framing from ordinary assistant
/// content while retaining only validated progress metadata. The filter is
/// incremental: both provider deltas and network chunks may split any marker
/// or its JSON payload.
#[derive(Default)]
struct PublicContentFilter {
    pending: String,
    mode: PublicContentMode,
    progress_events_seen: usize,
}

impl PublicContentFilter {
    fn push(&mut self, text: &str, sink: &Arc<dyn ProgressSink>, _attempt: u32) -> String {
        self.pending.push_str(text);
        self.drain(sink)
    }

    fn finish(&mut self, sink: &Arc<dyn ProgressSink>) -> String {
        let visible = match self.mode {
            PublicContentMode::Progress => {
                // An unclosed progress frame is intentionally discarded. Its
                // payload must never leak into the JSON result on malformed
                // output or a truncated stream.
                self.pending.clear();
                String::new()
            }
            PublicContentMode::Result | PublicContentMode::Normal => {
                // A missing result close tag is a safe compatibility fallback:
                // the opening marker is already removed, and the ordinary
                // result content remains available to the JSON extractor. Any
                // partial control marker is discarded rather than leaking into
                // the result.
                let keep = longest_marker_prefix_suffix(&self.pending);
                let split_at = self.pending.len().saturating_sub(keep);
                let visible = self.pending[..split_at].to_string();
                self.pending.clear();
                visible
            }
        };
        self.mode = PublicContentMode::Normal;
        let _ = sink;
        visible
    }

    fn drain(&mut self, sink: &Arc<dyn ProgressSink>) -> String {
        let mut visible = String::new();
        loop {
            match self.mode {
                PublicContentMode::Normal => {
                    let Some((index, marker)) = self.next_normal_marker() else {
                        let keep = longest_marker_prefix_suffix(&self.pending);
                        let split_at = self.pending.len().saturating_sub(keep);
                        visible.push_str(&self.pending[..split_at]);
                        self.pending.drain(..split_at);
                        break;
                    };
                    visible.push_str(&self.pending[..index]);
                    self.pending.drain(..index + marker.len());
                    self.mode = match marker {
                        MYC_PROGRESS_OPEN => PublicContentMode::Progress,
                        MYC_RESULT_OPEN => PublicContentMode::Result,
                        _ => PublicContentMode::Normal,
                    };
                }
                PublicContentMode::Progress => {
                    let progress_close = self.pending.find(MYC_PROGRESS_CLOSE);
                    let result_open = self.pending.find(MYC_RESULT_OPEN);
                    if let Some(result_index) = result_open.filter(|result_index| {
                        progress_close.is_none_or(|close_index| *result_index < close_index)
                    }) {
                        // Salvage a valid final frame even if the model forgot
                        // to close or malformed the preceding progress frame.
                        self.pending.drain(..result_index + MYC_RESULT_OPEN.len());
                        self.mode = PublicContentMode::Result;
                        continue;
                    }
                    let Some(index) = progress_close else {
                        if self.pending.len() > MAX_PUBLIC_PROGRESS_PAYLOAD_BYTES {
                            // Keep the parser bounded while waiting for a
                            // close marker that may never arrive, while still
                            // preserving a split final-result marker prefix.
                            let keep = marker_prefix_suffix(&self.pending, MYC_RESULT_OPEN);
                            let suffix = self.pending[self.pending.len() - keep..].to_string();
                            self.pending.clear();
                            self.pending.push_str(&suffix);
                        }
                        break;
                    };
                    let payload = self.pending[..index].to_string();
                    self.pending.drain(..index + MYC_PROGRESS_CLOSE.len());
                    self.emit_progress(&payload, sink);
                    self.mode = PublicContentMode::Normal;
                }
                PublicContentMode::Result => {
                    let Some(index) = self.pending.find(MYC_RESULT_CLOSE) else {
                        let keep = marker_prefix_suffix(&self.pending, MYC_RESULT_CLOSE);
                        let split_at = self.pending.len().saturating_sub(keep);
                        visible.push_str(&self.pending[..split_at]);
                        self.pending.drain(..split_at);
                        break;
                    };
                    visible.push_str(&self.pending[..index]);
                    self.pending.drain(..index + MYC_RESULT_CLOSE.len());
                    self.mode = PublicContentMode::Normal;
                }
            }
        }
        visible
    }

    fn next_normal_marker(&self) -> Option<(usize, &'static str)> {
        [
            MYC_PROGRESS_OPEN,
            MYC_PROGRESS_CLOSE,
            MYC_RESULT_OPEN,
            MYC_RESULT_CLOSE,
        ]
        .into_iter()
        .filter_map(|marker| self.pending.find(marker).map(|index| (index, marker)))
        .min_by_key(|(index, _)| *index)
    }

    fn emit_progress(&mut self, payload: &str, sink: &Arc<dyn ProgressSink>) {
        if self.progress_events_seen >= MAX_PUBLIC_PROGRESS_EVENTS
            || payload.len() > MAX_PUBLIC_PROGRESS_PAYLOAD_BYTES
        {
            return;
        }
        self.progress_events_seen = self.progress_events_seen.saturating_add(1);
        let Ok(value) = serde_json::from_str::<Value>(payload.trim()) else {
            return;
        };
        let Some(object) = value.as_object() else {
            return;
        };
        let Some(stage) =
            bounded_public_string(object.get("stage"), MAX_PUBLIC_PROGRESS_STAGE_CHARS)
        else {
            return;
        };
        let Some(summary) =
            bounded_public_string(object.get("summary"), MAX_PUBLIC_PROGRESS_SUMMARY_CHARS)
        else {
            return;
        };
        let evidence_count = bounded_public_count(object.get("evidenceCount"));
        let warning_count = bounded_public_count(object.get("warningCount"));
        sink.emit(TransportProgressEvent::PublicProgress {
            stage,
            summary,
            evidence_count,
            warning_count,
        });
    }
}

fn bounded_public_string(value: Option<&Value>, max_chars: usize) -> Option<String> {
    let value = value?.as_str()?.trim();
    if value.is_empty()
        || value.chars().count() > max_chars
        || value.chars().any(|character| character.is_control())
    {
        return None;
    }
    Some(value.to_string())
}

fn bounded_public_count(value: Option<&Value>) -> Option<u64> {
    value
        .and_then(Value::as_u64)
        .filter(|count| *count <= 1_000_000)
}

fn marker_prefix_suffix(value: &str, marker: &str) -> usize {
    (1..=value.len().min(marker.len().saturating_sub(1)))
        .rev()
        .find(|length| value.ends_with(&marker[..*length]))
        .unwrap_or(0)
}

fn longest_marker_prefix_suffix(value: &str) -> usize {
    [
        MYC_PROGRESS_OPEN,
        MYC_PROGRESS_CLOSE,
        MYC_RESULT_OPEN,
        MYC_RESULT_CLOSE,
    ]
    .into_iter()
    .map(|marker| marker_prefix_suffix(value, marker))
    .max()
    .unwrap_or(0)
}

#[derive(Default)]
struct SseState {
    pending: Vec<u8>,
    event_data: String,
    content: String,
    reasoning_chunks: u64,
    reasoning_bytes: u64,
    content_chunks: u64,
    content_bytes: u64,
    done: bool,
    public_content: PublicContentFilter,
}

impl SseState {
    fn push(
        &mut self,
        chunk: &[u8],
        sink: &Arc<dyn ProgressSink>,
        attempt: u32,
    ) -> Result<(), LlmError> {
        self.pending.extend_from_slice(chunk);
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.consume_line(&line, sink, attempt)?;
        }
        Ok(())
    }

    fn finish(&mut self, sink: &Arc<dyn ProgressSink>, attempt: u32) -> Result<(), LlmError> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.consume_line(&line, sink, attempt)?;
        }
        self.flush_event(sink, attempt)
    }

    fn consume_line(
        &mut self,
        line: &[u8],
        sink: &Arc<dyn ProgressSink>,
        attempt: u32,
    ) -> Result<(), LlmError> {
        // Network chunks may split immediately after indentation or other ASCII
        // whitespace. Treat whitespace-only records as SSE event separators and
        // tolerate leading whitespace before a field name without ever trimming
        // the JSON payload itself.
        let field_start = line
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(line.len());
        let line = &line[field_start..];
        if line.is_empty() {
            return self.flush_event(sink, attempt);
        }
        if line.starts_with(b":") {
            return Ok(());
        }
        let Some(data) = line.strip_prefix(b"data:") else {
            return Ok(());
        };
        let data = data.strip_prefix(b" ").unwrap_or(data);
        let data = std::str::from_utf8(data)
            .map_err(|error| LlmError::ParseError(format!("Kimi SSE was not UTF-8: {error}")))?;
        // Kimi normally terminates each event with an empty line. Be tolerant of
        // proxies that collapse that separator: once the buffered payload is a
        // complete JSON value, a following data field starts a new event. An
        // incomplete multi-line JSON value is still joined according to SSE.
        if !self.event_data.is_empty() && serde_json::from_str::<Value>(&self.event_data).is_ok() {
            self.flush_event(sink, attempt)?;
        }
        if !self.event_data.is_empty() {
            self.event_data.push('\n');
        }
        self.event_data.push_str(data);
        Ok(())
    }

    fn flush_event(&mut self, sink: &Arc<dyn ProgressSink>, attempt: u32) -> Result<(), LlmError> {
        if self.event_data.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.event_data);
        if data.trim() == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let value: Value = serde_json::from_str(&data)
            .map_err(|error| LlmError::ParseError(format!("Kimi SSE event: {error}")))?;
        let Some(delta) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"))
        else {
            return Ok(());
        };
        if let Some(reasoning) = delta.get("reasoning_content").and_then(Value::as_str) {
            if !reasoning.is_empty() {
                self.reasoning_chunks = self.reasoning_chunks.saturating_add(1);
                self.reasoning_bytes = self.reasoning_bytes.saturating_add(reasoning.len() as u64);
                sink.emit(TransportProgressEvent::ReasoningActivity {
                    attempt,
                    chunks: self.reasoning_chunks,
                    utf8_bytes: self.reasoning_bytes,
                });
            }
        }
        if let Some(content) = delta.get("content").and_then(Value::as_str) {
            if !content.is_empty() {
                let visible = self.public_content.push(content, sink, attempt);
                if self.content.len().saturating_add(visible.len()) > MAX_RESPONSE_BODY_BYTES {
                    return Err(LlmError::ParseError(format!(
                        "Kimi content exceeds the local {MAX_RESPONSE_BODY_BYTES}-byte safety limit"
                    )));
                }
                if !visible.is_empty() {
                    self.content.push_str(&visible);
                    self.content_chunks = self.content_chunks.saturating_add(1);
                    self.content_bytes = self.content_bytes.saturating_add(visible.len() as u64);
                    sink.emit(TransportProgressEvent::ContentActivity {
                        attempt,
                        chunks: self.content_chunks,
                        utf8_bytes: self.content_bytes,
                    });
                }
            }
        }
        Ok(())
    }
}

async fn read_kimi_sse(
    mut response: reqwest::Response,
    sink: &Arc<dyn ProgressSink>,
    attempt: u32,
) -> Result<String, LlmError> {
    let mut state = SseState::default();
    let mut received = 0_usize;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| LlmError::HttpError(format!("Kimi SSE read failed: {error}")))?
    {
        received = received.saturating_add(chunk.len());
        if received > MAX_KIMI_SSE_WIRE_BYTES {
            return Err(LlmError::ParseError(format!(
                "Kimi SSE transport exceeds the local {MAX_KIMI_SSE_WIRE_BYTES}-byte safety limit"
            )));
        }
        state.push(&chunk, sink, attempt)?;
        if state.done {
            break;
        }
    }
    state.finish(sink, attempt)?;
    let trailing = state.public_content.finish(sink);
    if state.content.len().saturating_add(trailing.len()) > MAX_RESPONSE_BODY_BYTES {
        return Err(LlmError::ParseError(format!(
            "Kimi content exceeds the local {MAX_RESPONSE_BODY_BYTES}-byte safety limit"
        )));
    }
    if !trailing.is_empty() {
        state.content.push_str(&trailing);
    }
    if !state.done {
        return Err(LlmError::ParseError(
            "Kimi SSE ended before the [DONE] marker".to_string(),
        ));
    }
    if state.content.trim().is_empty() {
        return Err(LlmError::ParseError(
            "Kimi SSE did not contain content deltas".to_string(),
        ));
    }
    Ok(state.content)
}

#[derive(Default)]
struct AnthropicSseState {
    pending: Vec<u8>,
    event_data: String,
    content: String,
    reasoning_chunks: u64,
    reasoning_bytes: u64,
    content_chunks: u64,
    content_bytes: u64,
    done: bool,
    public_content: PublicContentFilter,
}

impl AnthropicSseState {
    fn push(
        &mut self,
        chunk: &[u8],
        sink: &Arc<dyn ProgressSink>,
        attempt: u32,
    ) -> Result<(), LlmError> {
        self.pending.extend_from_slice(chunk);
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut line = self.pending.drain(..=newline).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            self.consume_line(&line, sink, attempt)?;
        }
        Ok(())
    }

    fn finish(&mut self, sink: &Arc<dyn ProgressSink>, attempt: u32) -> Result<(), LlmError> {
        if !self.pending.is_empty() {
            let line = std::mem::take(&mut self.pending);
            self.consume_line(&line, sink, attempt)?;
        }
        self.flush_event(sink, attempt)
    }

    fn consume_line(
        &mut self,
        line: &[u8],
        sink: &Arc<dyn ProgressSink>,
        attempt: u32,
    ) -> Result<(), LlmError> {
        let field_start = line
            .iter()
            .position(|byte| !byte.is_ascii_whitespace())
            .unwrap_or(line.len());
        let line = &line[field_start..];
        if line.is_empty() {
            return self.flush_event(sink, attempt);
        }
        if line.starts_with(b":") || line.starts_with(b"event:") {
            return Ok(());
        }
        let Some(data) = line.strip_prefix(b"data:") else {
            return Ok(());
        };
        let data = data.strip_prefix(b" ").unwrap_or(data);
        let data = std::str::from_utf8(data).map_err(|error| {
            LlmError::ParseError(format!("Kimi Anthropic SSE was not UTF-8: {error}"))
        })?;
        if !self.event_data.is_empty() && serde_json::from_str::<Value>(&self.event_data).is_ok() {
            self.flush_event(sink, attempt)?;
        }
        if !self.event_data.is_empty() {
            self.event_data.push('\n');
        }
        self.event_data.push_str(data);
        Ok(())
    }

    fn flush_event(&mut self, sink: &Arc<dyn ProgressSink>, attempt: u32) -> Result<(), LlmError> {
        if self.event_data.is_empty() {
            return Ok(());
        }
        let data = std::mem::take(&mut self.event_data);
        if data.trim() == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let value: Value = serde_json::from_str(&data)
            .map_err(|error| LlmError::ParseError(format!("Kimi Anthropic SSE event: {error}")))?;
        match value.get("type").and_then(Value::as_str) {
            Some("message_stop") => {
                self.done = true;
            }
            Some("error") => {
                let message = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("Kimi Anthropic stream returned an error");
                return Err(LlmError::ServerError {
                    status: 200,
                    body: message.to_string(),
                });
            }
            Some("content_block_delta") => {
                let Some(delta) = value.get("delta") else {
                    return Ok(());
                };
                match delta.get("type").and_then(Value::as_str) {
                    Some("thinking_delta") => {
                        if let Some(reasoning) = delta.get("thinking").and_then(Value::as_str) {
                            if !reasoning.is_empty() {
                                self.reasoning_chunks = self.reasoning_chunks.saturating_add(1);
                                self.reasoning_bytes =
                                    self.reasoning_bytes.saturating_add(reasoning.len() as u64);
                                sink.emit(TransportProgressEvent::ReasoningActivity {
                                    attempt,
                                    chunks: self.reasoning_chunks,
                                    utf8_bytes: self.reasoning_bytes,
                                });
                            }
                        }
                    }
                    Some("text_delta") => {
                        if let Some(content) = delta.get("text").and_then(Value::as_str) {
                            if !content.is_empty() {
                                let visible = self.public_content.push(content, sink, attempt);
                                if self.content.len().saturating_add(visible.len())
                                    > MAX_RESPONSE_BODY_BYTES
                                {
                                    return Err(LlmError::ParseError(format!(
                                        "Kimi content exceeds the local {MAX_RESPONSE_BODY_BYTES}-byte safety limit"
                                    )));
                                }
                                if !visible.is_empty() {
                                    self.content.push_str(&visible);
                                    self.content_chunks = self.content_chunks.saturating_add(1);
                                    self.content_bytes =
                                        self.content_bytes.saturating_add(visible.len() as u64);
                                    sink.emit(TransportProgressEvent::ContentActivity {
                                        attempt,
                                        chunks: self.content_chunks,
                                        utf8_bytes: self.content_bytes,
                                    });
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(())
    }
}

async fn read_kimi_anthropic_sse(
    mut response: reqwest::Response,
    sink: &Arc<dyn ProgressSink>,
    attempt: u32,
) -> Result<String, LlmError> {
    let mut state = AnthropicSseState::default();
    let mut received = 0_usize;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| LlmError::HttpError(format!("Kimi Anthropic SSE read failed: {error}")))?
    {
        received = received.saturating_add(chunk.len());
        if received > MAX_KIMI_SSE_WIRE_BYTES {
            return Err(LlmError::ParseError(format!(
                "Kimi Anthropic SSE transport exceeds the local {MAX_KIMI_SSE_WIRE_BYTES}-byte safety limit"
            )));
        }
        state.push(&chunk, sink, attempt)?;
        if state.done {
            break;
        }
    }
    state.finish(sink, attempt)?;
    let trailing = state.public_content.finish(sink);
    if state.content.len().saturating_add(trailing.len()) > MAX_RESPONSE_BODY_BYTES {
        return Err(LlmError::ParseError(format!(
            "Kimi content exceeds the local {MAX_RESPONSE_BODY_BYTES}-byte safety limit"
        )));
    }
    if !trailing.is_empty() {
        state.content.push_str(&trailing);
    }
    if !state.done {
        return Err(LlmError::ParseError(
            "Kimi Anthropic SSE ended before message_stop".to_string(),
        ));
    }
    if state.content.trim().is_empty() {
        return Err(LlmError::ParseError(
            "Kimi Anthropic SSE did not contain text deltas".to_string(),
        ));
    }
    Ok(state.content)
}

async fn bounded_response_text(response: reqwest::Response) -> String {
    match response.bytes().await {
        Ok(bytes) => String::from_utf8_lossy(
            &bytes
                .iter()
                .take(MAX_RESPONSE_BODY_BYTES)
                .copied()
                .collect::<Vec<_>>(),
        )
        .into_owned(),
        Err(error) => format!("failed to read response body: {error}"),
    }
}

fn http_error(status: u16, body: String) -> LlmError {
    if is_terminal_http_status(status) {
        LlmError::ClientError { status, body }
    } else {
        LlmError::ServerError { status, body }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kimi_config() -> PdfAgentLlmConfig {
        PdfAgentLlmConfig {
            api_url: KIMI_CN_BASE_URL.to_string(),
            api_format: ApiFormat::OpenAi,
            api_key: "test-key".to_string(),
            model: "deepseek-v4-flash".to_string(),
            thinking: true,
            thinking_level: Some("extra_high".to_string()),
            provider: "Kimi".to_string(),
            transport: PdfAgentTransport::KimiFileExtract,
            timeout_secs: 30,
        }
    }

    fn kimi_anthropic_config() -> PdfAgentLlmConfig {
        PdfAgentLlmConfig {
            api_url: KIMI_CN_ANTHROPIC_BASE_URL.to_string(),
            api_format: ApiFormat::Anthropic,
            ..kimi_config()
        }
    }

    #[test]
    fn normalization_maps_kimi_defaults_and_boolean_thinking() {
        let normalized = normalize_config(kimi_config()).expect("normalize Kimi config");
        assert_eq!(normalized.llm.api_format, ApiFormat::OpenAi);
        assert_eq!(normalized.llm.model, KIMI_K26_MODEL);
        assert_eq!(normalized.llm.thinking_level, None);
        assert_eq!(normalized.llm.provider, "moonshot");
        assert_eq!(
            normalized.request_policy.thinking,
            Some(ThinkingMode::Enabled)
        );
        assert!(matches!(normalized.backend, Backend::KimiK26(_)));
    }

    #[test]
    fn stream_termination_errors_are_retryable_parse_failures() {
        // 被截断的流是 provider 侧瞬态异常:重试安全。真正的事件级 JSON
        // 解析失败仍不可重试(确定性)。
        // Truncated streams are provider-side transients: retrying is safe.
        // Genuine per-event JSON parse failures stay non-retryable.
        for message in [
            "Kimi SSE ended before the [DONE] marker",
            "Kimi Anthropic SSE ended before message_stop",
            "Kimi SSE did not contain content deltas",
            "Kimi Anthropic SSE did not contain text deltas",
        ] {
            assert!(
                is_retryable_transport_error(&LlmError::ParseError(message.to_string())),
                "{message} must retry"
            );
        }
        assert!(
            !is_retryable_transport_error(&LlmError::ParseError(
                "Kimi SSE event: expected ident".to_string()
            )),
            "event-level parse failures must stay non-retryable"
        );
    }

    #[test]
    fn normalization_maps_disabled_thinking_without_reasoning_levels() {
        let mut config = kimi_config();
        config.thinking = false;
        config.thinking_level = Some("high".to_string());
        let normalized = normalize_config(config).expect("normalize Kimi config");
        assert_eq!(normalized.llm.thinking_level, None);
        assert_eq!(
            normalized.request_policy.thinking,
            Some(ThinkingMode::Disabled)
        );
    }

    #[test]
    fn approved_endpoints_are_exact_and_https_only() {
        assert!(is_approved_endpoint(KIMI_CN_BASE_URL));
        assert!(is_approved_endpoint("https://api.moonshot.ai/v1/"));
        assert!(is_approved_endpoint(KIMI_CN_ANTHROPIC_BASE_URL));
        assert!(is_approved_endpoint("https://api.moonshot.ai/anthropic/"));
        assert!(!is_approved_endpoint("http://api.moonshot.cn/v1"));
        assert!(!is_approved_endpoint("https://api.moonshot.cn"));
        assert!(!is_approved_endpoint("https://evil.example/v1"));
    }

    #[test]
    fn parser_backend_requires_the_matching_official_base_path() {
        let normalized = normalize_config(kimi_anthropic_config()).expect("normalize Anthropic");
        assert_eq!(normalized.llm.api_format, ApiFormat::Anthropic);
        let Backend::KimiK26(config) = normalized.backend else {
            panic!("expected Kimi backend");
        };
        assert_eq!(config.api_format, ApiFormat::Anthropic);
        assert_eq!(config.api_url, KIMI_CN_ANTHROPIC_BASE_URL);

        let mut mismatched = kimi_config();
        mismatched.api_format = ApiFormat::Anthropic;
        assert!(normalize_config(mismatched).is_err());
    }

    #[test]
    fn request_policy_omits_arbitrary_sampling_fields() {
        let normalized = normalize_config(kimi_config()).expect("normalize Kimi config");
        let body = build_chat_request(
            &normalized.llm.model,
            "system",
            "user",
            &normalized.request_policy,
        );
        assert_eq!(body["model"], KIMI_K26_MODEL);
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["stream"], true);
        for field in [
            "temperature",
            "top_p",
            "n",
            "presence_penalty",
            "frequency_penalty",
            "max_tokens",
            "max_completion_tokens",
        ] {
            assert!(body.get(field).is_none(), "unexpected field: {field}");
        }
    }

    #[test]
    fn public_progress_disables_openai_json_mode_for_framed_streams() {
        let normalized = normalize_config(kimi_config()).expect("normalize Kimi config");
        let body = build_chat_request_with_public_progress(
            &normalized.llm.model,
            "system",
            "user",
            &normalized.request_policy,
            true,
        );
        assert!(body.get("response_format").is_none());
        assert_eq!(body["stream"], true);
    }

    #[test]
    fn anthropic_request_uses_messages_shape_and_streaming() {
        let normalized = normalize_config(kimi_anthropic_config()).expect("normalize Anthropic");
        let body = build_anthropic_chat_request(
            &normalized.llm.model,
            "system",
            "user",
            &normalized.request_policy,
        );
        assert_eq!(body["model"], KIMI_K26_MODEL);
        assert_eq!(body["system"], "system");
        assert_eq!(body["messages"][0]["content"], "user");
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["thinking"]["budget_tokens"], 16_000);
        assert_eq!(body["max_tokens"], 32_768);
        assert_eq!(body["stream"], true);
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn kimi_anthropic_endpoint_uses_bearer_auth() {
        let normalized = normalize_config(kimi_anthropic_config()).expect("normalize Anthropic");
        let Backend::KimiK26(config) = normalized.backend else {
            panic!("expected Kimi backend");
        };
        let client = KimiK26Client::new(config).expect("create client");
        let headers = client.chat_auth_headers().expect("Anthropic headers");
        assert_eq!(
            headers
                .get(AUTHORIZATION)
                .and_then(|value| value.to_str().ok()),
            Some("Bearer test-key")
        );
        assert!(headers.get("x-api-key").is_none());
        assert_eq!(
            headers
                .get("anthropic-version")
                .and_then(|value| value.to_str().ok()),
            Some("2023-06-01")
        );
    }

    #[test]
    fn pdf_request_uses_files_api_transport_contract() {
        let normalized = normalize_config(kimi_config()).expect("normalize Kimi config");
        let Backend::KimiK26(config) = normalized.backend else {
            panic!("expected Kimi backend");
        };
        assert_eq!(config.transport, PdfAgentTransport::KimiFileExtract);
        assert_eq!(
            format!("{}/files", config.api_url),
            "https://api.moonshot.cn/v1/files"
        );
    }

    #[test]
    fn sse_parser_collects_content_but_reports_reasoning_as_metadata_only() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let sink: Arc<dyn ProgressSink> = Arc::new(move |event| {
            captured.lock().expect("events lock").push(event);
        });
        let mut state = SseState::default();
        state
            .push(
                br#"data: {"choices":[{"delta":{"reasoning_content":"private reasoning"}}]}

data: {"choices":[{"delta":{"content":"{\"answer\":"}}]}
"#,
                &sink,
                1,
            )
            .expect("first SSE fragment");
        state
            .push(
                br#"data: {"choices":[{"delta":{"content":"true}"}}]}

data: [DONE]

"#,
                &sink,
                1,
            )
            .expect("second SSE fragment");
        state.finish(&sink, 1).expect("finish SSE");

        assert!(state.done);
        assert_eq!(state.content, r#"{"answer":true}"#);
        let serialized =
            serde_json::to_string(&*events.lock().expect("events lock")).expect("serialize events");
        assert!(serialized.contains("reasoningActivity"));
        assert!(serialized.contains("contentActivity"));
        assert!(!serialized.contains("private reasoning"));
        assert!(!serialized.contains("answer"));
    }

    #[test]
    fn anthropic_sse_parser_collects_text_but_only_reports_thinking_metadata() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let sink: Arc<dyn ProgressSink> = Arc::new(move |event| {
            captured.lock().expect("events lock").push(event);
        });
        let mut state = AnthropicSseState::default();
        state
            .push(
                br#"event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"private reasoning"}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"{\"answer\":"}}

"#,
                &sink,
                1,
            )
            .expect("Anthropic SSE deltas");
        state
            .push(
                br#"event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"true}"}}

event: message_stop
data: {"type":"message_stop"}

"#,
                &sink,
                1,
            )
            .expect("Anthropic SSE stop");
        state.finish(&sink, 1).expect("finish Anthropic SSE");

        assert!(state.done);
        assert_eq!(state.content, r#"{"answer":true}"#);
        let serialized =
            serde_json::to_string(&*events.lock().expect("events lock")).expect("serialize events");
        assert!(serialized.contains("reasoningActivity"));
        assert!(serialized.contains("contentActivity"));
        assert!(!serialized.contains("private reasoning"));
        assert!(!serialized.contains("answer"));
    }

    #[test]
    fn openai_sse_public_progress_survives_network_and_tag_boundaries() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let sink: Arc<dyn ProgressSink> = Arc::new(move |event| {
            captured.lock().expect("events lock").push(event);
        });
        let frames = [
            json!({"choices":[{"delta":{"reasoning_content":"private thinking"}}]}),
            json!({"choices":[{"delta":{"content":"<myc_pro"}}]}),
            json!({"choices":[{"delta":{"content":"gress>{\"stage\":\"structure\",\"summary\":\"Scanned sections\",\"evidenceCount\":3,\"warningCount\":1}"}}]}),
            json!({"choices":[{"delta":{"content":"</myc_progress><myc_result>{\"answer\":true}</myc_result>"}}]}),
        ];
        let mut wire = frames
            .iter()
            .map(|frame| format!("data: {}\n\n", frame))
            .collect::<String>();
        wire.push_str("data: [DONE]\n\n");

        let mut state = SseState::default();
        for chunk in wire.as_bytes().chunks(7) {
            state.push(chunk, &sink, 1).expect("OpenAI network chunk");
        }
        state.finish(&sink, 1).expect("finish OpenAI SSE");
        let trailing = state.public_content.finish(&sink);
        state.content.push_str(&trailing);

        assert!(state.done);
        assert_eq!(state.content, r#"{"answer":true}"#);
        let events = events.lock().expect("events lock");
        let progress = events.iter().find_map(|event| match event {
            TransportProgressEvent::PublicProgress {
                stage,
                summary,
                evidence_count,
                warning_count,
            } => Some((stage, summary, *evidence_count, *warning_count)),
            _ => None,
        });
        assert_eq!(
            progress,
            Some((
                &"structure".to_string(),
                &"Scanned sections".to_string(),
                Some(3),
                Some(1)
            ))
        );
        let serialized = serde_json::to_string(&*events).expect("serialize events");
        assert!(!serialized.contains("private thinking"));
        assert!(!serialized.contains("answer"));
    }

    #[test]
    fn anthropic_sse_public_progress_survives_network_and_tag_boundaries() {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = Arc::clone(&events);
        let sink: Arc<dyn ProgressSink> = Arc::new(move |event| {
            captured.lock().expect("events lock").push(event);
        });
        let frames = [
            (
                "content_block_delta",
                json!({
                    "type":"content_block_delta","index":0,
                    "delta":{"type":"thinking_delta","thinking":"private thinking"}
                }),
            ),
            (
                "content_block_delta",
                json!({
                    "type":"content_block_delta","index":1,
                    "delta":{"type":"text_delta","text":"<myc_progress>{\"stage\":\"references\",\"summary\":\"Checking refs\",\"warningCount\":2}</myc_pro"}
                }),
            ),
            (
                "content_block_delta",
                json!({
                    "type":"content_block_delta","index":1,
                    "delta":{"type":"text_delta","text":"gress><myc_result>{\"answer\":true}</myc_result>"}
                }),
            ),
            ("message_stop", json!({"type":"message_stop"})),
        ];
        let wire = frames
            .iter()
            .map(|(kind, frame)| format!("event: {kind}\ndata: {}\n\n", frame))
            .collect::<String>();
        let mut state = AnthropicSseState::default();
        for chunk in wire.as_bytes().chunks(5) {
            state
                .push(chunk, &sink, 2)
                .expect("Anthropic network chunk");
        }
        state.finish(&sink, 2).expect("finish Anthropic SSE");
        let trailing = state.public_content.finish(&sink);
        state.content.push_str(&trailing);

        assert!(state.done);
        assert_eq!(state.content, r#"{"answer":true}"#);
        let events = events.lock().expect("events lock");
        assert!(events.iter().any(|event| matches!(
            event,
            TransportProgressEvent::PublicProgress {
                stage,
                summary,
                evidence_count: None,
                warning_count: Some(2),
            } if stage == "references" && summary == "Checking refs"
        )));
        let serialized = serde_json::to_string(&*events).expect("serialize events");
        assert!(!serialized.contains("private thinking"));
        assert!(!serialized.contains("answer"));
    }

    #[test]
    fn malformed_public_progress_is_discarded_without_poisoning_result() {
        let sink: Arc<dyn ProgressSink> = Arc::new(|_event| {});
        let mut state = SseState::default();
        let frame = json!({"choices":[{"delta":{"content":"<myc_progress>{not-json}</myc_progress><myc_result>{\"answer\":true}</myc_result>"}}]});
        let wire = format!("data: {frame}\n\ndata: [DONE]\n\n");
        for chunk in wire.as_bytes().chunks(3) {
            state
                .push(chunk, &sink, 1)
                .expect("malformed progress chunk");
        }
        state.finish(&sink, 1).expect("finish malformed progress");
        let trailing = state.public_content.finish(&sink);
        state.content.push_str(&trailing);
        assert_eq!(state.content, r#"{"answer":true}"#);
    }

    #[test]
    fn unclosed_public_progress_does_not_hide_a_valid_result_frame() {
        let sink: Arc<dyn ProgressSink> = Arc::new(|_event| {});
        let mut filter = PublicContentFilter::default();
        let mut result = filter.push(
            "<myc_progress>{\"stage\":\"broken\"<myc_result>{\"answer\":true}</myc_result>",
            &sink,
            1,
        );
        result.push_str(&filter.finish(&sink));
        assert_eq!(result, r#"{"answer":true}"#);
    }
}
