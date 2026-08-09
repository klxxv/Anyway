//! LLM Provider 抽象层 / LLM Provider abstraction layer.
//!
//! 提供：
//! 1. `LlmClient` trait — 富操作接口（角色路由、推理缓存、重试）
//! 2. `LlmClientAdapter` — 桥接到 `semantic_pipeline::LlmProvider`
//! 3. `OpenAiCompatibleClient` — OpenAI 兼容 API 的具体实现（可配置 base URL）
//! 4. 所有请求/响应类型、重试/截断修复/缓存工具函数
//!
//! API Key 绝不硬编码、绝不合并进 prompt、绝不记录到日志。
//! The API key is never hard-coded, never merged into prompts, and never logged.

use async_trait::async_trait;
use rand::Rng as _;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

// ── 常量 / Constants ──────────────────────────────────────────────────────

pub const DEFAULT_TIMEOUT_SECS: u64 = 120;
pub const MAX_RETRIES: u32 = 3;
pub const INITIAL_BACKOFF_MS: u64 = 1000;
pub const MAX_BACKOFF_MS: u64 = 16_000;
/// 最大允许的 HTTP 响应体大小（1 MiB）。LLM 响应再大即视为畸形/攻击载荷。
pub const MAX_RESPONSE_BODY_BYTES: usize = 1 * 1024 * 1024;
/// 单个客户端实例的推理缓存条目上限。防止长会话无界增长。
pub const MAX_REASONING_CACHE_ENTRIES: usize = 256;

// ── 模型路由 / Model routing ──────────────────────────────────────────────

/// 调用角色决定了使用哪个模型和参数。
/// The call role determines which model and parameters to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallRole {
    /// 提取阶段：快速模型，非思考模式，JSON 输出。
    Extraction,
    /// 综合阶段：强模型，高思考深度，JSON 输出。
    Synthesis,
    /// 修复/恢复阶段：强模型，非思考模式，用于修复截断 JSON。
    Recovery,
}

/// 单个角色的模型路由配置。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRoute {
    /// 模型名（如 "deepseek-chat" / "gpt-4o" / "llama3.1"）。
    pub model: String,
    /// 是否启用思考模式（reasoner 模型有效）。
    pub thinking: bool,
    /// 思考深度：None = 默认，"low" / "medium" / "high"。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    /// 是否要求 JSON 结构化输出。
    pub json_output: bool,
}

/// 模型路由表。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelRouting {
    pub extraction: ModelRoute,
    pub synthesis: ModelRoute,
    pub recovery: ModelRoute,
}

impl Default for ModelRouting {
    fn default() -> Self {
        // 默认路由指向 DeepSeek（向后兼容）。
        Self {
            extraction: ModelRoute {
                model: "deepseek-chat".to_string(),
                thinking: false,
                thinking_level: None,
                json_output: true,
            },
            synthesis: ModelRoute {
                model: "deepseek-reasoner".to_string(),
                thinking: true,
                thinking_level: Some("high".to_string()),
                json_output: true,
            },
            recovery: ModelRoute {
                model: "deepseek-chat".to_string(),
                thinking: false,
                thinking_level: None,
                json_output: false,
            },
        }
    }
}

impl ModelRouting {
    /// 根据调用角色返回对应的模型路由。
    pub fn route(&self, role: CallRole) -> &ModelRoute {
        match role {
            CallRole::Extraction => &self.extraction,
            CallRole::Synthesis => &self.synthesis,
            CallRole::Recovery => &self.recovery,
        }
    }

    /// 用统一模型创建路由表（所有角色使用相同模型，非思考模式）。
    /// 适用于 Ollama / vLLM / LM Studio 等本地模型。
    pub fn uniform(model: &str, json_output: bool) -> Self {
        let route = ModelRoute {
            model: model.to_string(),
            thinking: false,
            thinking_level: None,
            json_output,
        };
        Self {
            extraction: route.clone(),
            synthesis: route.clone(),
            recovery: ModelRoute {
                json_output: false,
                ..route
            },
        }
    }
}

// ── 请求与响应类型 / Request & Response types ─────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    /// 思考模式下 assistant 消息携带的 reasoning_content。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    /// 用户标识（不可逆项目级哈希），用于监控与限流，不包含个人信息。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionResponse {
    pub id: Option<String>,
    pub object: Option<String>,
    pub created: Option<u64>,
    pub model: Option<String>,
    pub choices: Option<Vec<Choice>>,
    pub usage: Option<Usage>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Choice {
    pub index: Option<u32>,
    pub message: Option<ChoiceMessage>,
    #[serde(rename = "finish_reason")]
    pub finish_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ChoiceMessage {
    pub role: Option<String>,
    pub content: Option<String>,
    /// 思考模式下返回的推理内容（不流入最终输出，仅用于后续请求思维链传递）。
    #[serde(rename = "reasoning_content")]
    pub reasoning_content: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
    /// 缓存命中 token 数（上下文缓存特性）。
    pub prompt_cache_hit_tokens: Option<u32>,
    /// 缓存未命中 token 数。
    pub prompt_cache_miss_tokens: Option<u32>,
}

/// 截断检测结果。
#[derive(Clone, Debug)]
pub enum TruncationStatus {
    /// 未截断，内容完整。
    Complete,
    /// finish_reason=length，JSON 被截断。
    Truncated {
        /// 已收到的截断文本。
        partial_content: String,
        /// 截断点附近的上下文（用于续写修复）。
        tail_context: String,
    },
}

// ── 错误类型 / Error types ────────────────────────────────────────────────

#[derive(Clone, Debug, thiserror::Error)]
pub enum LlmError {
    #[error("API key is not configured")]
    ApiKeyMissing,

    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("API rate limited (429) — retries exhausted")]
    RateLimited,

    #[error("API server error ({status}): {body}")]
    ServerError { status: u16, body: String },

    #[error("API client error ({status}): {body}")]
    ClientError { status: u16, body: String },

    #[error("Response parsing failed: {0}")]
    ParseError(String),

    #[error("JSON output was truncated and repair failed: {0}")]
    TruncationRepairFailed(String),

    #[error("Retry exhausted after {attempts} attempts: {last_error}")]
    RetryExhausted {
        attempts: u32,
        last_error: String,
    },

    #[error("Serialization error: {0}")]
    SerializationError(String),
}

impl From<reqwest::Error> for LlmError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            LlmError::HttpError("Request timed out".to_string())
        } else if error.is_connect() {
            LlmError::HttpError(format!("Connection failed: {error}"))
        } else {
            LlmError::HttpError(error.to_string())
        }
    }
}

impl From<serde_json::Error> for LlmError {
    fn from(error: serde_json::Error) -> Self {
        LlmError::ParseError(error.to_string())
    }
}

// ── 可终止错误判定 / Terminal error classification ───────────────────────

/// 判断 HTTP 状态码是否应终止重试（不再退避）。
pub fn is_terminal_http_status(status: u16) -> bool {
    matches!(status, 400 | 401 | 402 | 422)
}

/// 判断 HTTP 状态码是否可重试（指数退避后重试）。
#[allow(dead_code)]
pub fn is_retryable_http_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

// ── 不可逆项目级标识 / Irreversible project-level user_id ─────────────────

/// 从项目标识生成不可逆的 user_id。
///
/// 输入任意项目级字符串（如项目路径或 ID），输出 64 hex SHA-256 哈希。
/// 此哈希为单向不可逆——即使 user_id 泄露，也无法反推原始项目路径。
/// 用于 API 的 `user` 字段，辅助监控与限流，不携带个人信息。
pub fn project_user_id(project_identifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"research-canvas:");
    hasher.update(project_identifier.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── JSON 截断检测 / JSON truncation detection ─────────────────────────────

/// 检测 LLM 返回的 JSON 是否被截断。
pub fn detect_truncation(
    content: &str,
    finish_reason: Option<&str>,
) -> Result<TruncationStatus, LlmError> {
    if finish_reason != Some("length") {
        if serde_json::from_str::<Value>(content.trim()).is_ok() {
            return Ok(TruncationStatus::Complete);
        }
        if let Some(extracted) = extract_json_substring(content) {
            if serde_json::from_str::<Value>(&extracted).is_ok() {
                return Ok(TruncationStatus::Complete);
            }
        }
        return Err(LlmError::ParseError(
            "JSON output is invalid and was not truncated".to_string(),
        ));
    }

    let content = content.trim();
    match serde_json::from_str::<Value>(content) {
        Ok(_) => Ok(TruncationStatus::Complete),
        Err(_) => {
            let tail_start = if content.len() > 512 {
                content.len() - 512
            } else {
                0
            };
            let tail_context = content[tail_start..].to_string();
            Ok(TruncationStatus::Truncated {
                partial_content: content.to_string(),
                tail_context,
            })
        }
    }
}

/// 从非纯 JSON 文本中提取 JSON 对象子串。
pub fn extract_json_substring(text: &str) -> Option<String> {
    let text = text.trim();
    if let Some(start) = text.find('{') {
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        for (i, ch) in text[start..].chars().enumerate() {
            if escape {
                escape = false;
                continue;
            }
            match ch {
                '\\' if in_string => escape = true,
                '"' => in_string = !in_string,
                '{' if !in_string => depth += 1,
                '}' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[start..start + i + 1].to_string());
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(start) = text.find('[') {
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escape = false;
        for (i, ch) in text[start..].chars().enumerate() {
            if escape {
                escape = false;
                continue;
            }
            match ch {
                '\\' if in_string => escape = true,
                '"' => in_string = !in_string,
                '[' if !in_string => depth += 1,
                ']' if !in_string => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[start..start + i + 1].to_string());
                    }
                }
                _ => {}
            }
        }
    }
    None
}

// ── 分片续写修复 / Shard repair via continuation ──────────────────────────

/// 构建截断续写请求。
pub fn build_continuation_request(
    original_messages: &[ChatMessage],
    partial_content: &str,
    tail_context: &str,
    route: &ModelRoute,
) -> ChatCompletionRequest {
    let mut messages = original_messages.to_vec();
    messages.push(ChatMessage {
        role: MessageRole::Assistant,
        content: partial_content.to_string(),
        reasoning_content: None,
    });
    messages.push(ChatMessage {
        role: MessageRole::User,
        content: format!(
            "继续输出，从截断处接着写完。上文末尾为：\n```\n{}\n```\n请直接从截断处续写，不要重复已输出的部分。只输出续写的内容。",
            tail_context
        ),
        reasoning_content: None,
    });
    ChatCompletionRequest {
        model: route.model.clone(),
        messages,
        response_format: None,
        temperature: Some(0.1),
        max_tokens: Some(4096),
        stream: Some(false),
        user: None,
    }
}

/// 尝试合并截断输出与续写输出。
pub fn merge_truncated_output(partial: &str, continuation: &str) -> String {
    let partial = partial.trim();
    let continuation = continuation.trim();
    let max_overlap = partial.len().min(continuation.len()).min(200);
    let mut best_split = 0usize;
    for overlap_len in (1..=max_overlap).rev() {
        let partial_tail = &partial[partial.len() - overlap_len..];
        let cont_head = &continuation[..overlap_len.min(continuation.len())];
        if partial_tail == cont_head {
            best_split = overlap_len;
            break;
        }
    }
    if best_split > 0 {
        format!(
            "{}{}",
            partial,
            &continuation[best_split.min(continuation.len())..]
        )
    } else {
        format!("{}{}", partial, continuation)
    }
}

// ── 重试工具 / Retry utilities ────────────────────────────────────────────

/// 计算第 n 次重试的退避时间（指数退避 + 真随机抖动）。
pub fn backoff_duration(attempt: u32) -> Duration {
    let base = INITIAL_BACKOFF_MS * 2u64.pow(attempt.saturating_sub(1));
    let capped = base.min(MAX_BACKOFF_MS);
    // 抖动范围 [0, capped/4]；使用 rand::thread_rng 提供真随机性。
    let jitter = rand::thread_rng().gen_range(0..=capped / 4);
    Duration::from_millis(capped.saturating_add(jitter))
}

/// 判断错误是否应终止重试。
pub fn should_terminate(error: &LlmError) -> bool {
    matches!(
        error,
        LlmError::ApiKeyMissing
            | LlmError::ClientError { .. }
            | LlmError::ParseError(_)
            | LlmError::SerializationError(_)
    )
}

// ── 安全输出类型 / Safe output types ──────────────────────────────────────

/// 输出类型：仅包含安全的摘要信息（不含原始 reasoning_content 或 API key）。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResult {
    pub content: String,
    pub finish_reason: Option<String>,
    pub model: String,
    pub usage: Option<Usage>,
    /// 是否有 reasoning_content（不输出具体内容，仅标记）。
    pub has_reasoning: bool,
    /// 上下文缓存是否命中。
    pub cache_hit: bool,
}

/// 从 ChatCompletionResponse 提取安全的 ChatResult（不含原始 reasoning）。
pub fn response_to_result(response: &ChatCompletionResponse) -> Option<ChatResult> {
    let choice = response.choices.as_ref()?.first()?;
    let message = choice.message.as_ref()?;
    let content = message.content.as_deref().unwrap_or("").to_string();
    let has_reasoning = message.reasoning_content.is_some();
    let cache_hit = response
        .usage
        .as_ref()
        .and_then(|u| u.prompt_cache_hit_tokens)
        .is_some_and(|hit| hit > 0);

    Some(ChatResult {
        content,
        finish_reason: choice.finish_reason.clone(),
        model: response.model.clone().unwrap_or_default(),
        usage: response.usage.clone(),
        has_reasoning,
        cache_hit,
    })
}

// ── LlmClient trait / LLM 客户端抽象 ──────────────────────────────────────

/// 富 LLM 客户端 trait：角色路由、推理缓存、结构化错误。
/// 实现者：OpenAiCompatibleClient（可配置 base URL）、mock 等。
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 发送聊天补全请求。`role` 决定模型选择。
    async fn chat(
        &self,
        role: CallRole,
        system_prompt: &str,
        user_prompt: &str,
        project_id: &str,
        reasoning_key: Option<&str>,
    ) -> Result<String, LlmError>;

    /// 可读的 provider 名称（如 "DeepSeek", "Ollama", "OpenAI"）。
    fn provider_name(&self) -> &str;

    /// 指定角色当前使用的模型名。
    fn model_for_role(&self, role: CallRole) -> &str;

    /// 唯一 provider 实例标识（对应 plugin id@version）。
    fn provider_id(&self) -> &str;
}

// ── LlmClientAdapter: 桥接 LlmClient → semantic_pipeline::LlmProvider ─────

/// 将富 `LlmClient` 适配为 `semantic_pipeline::LlmProvider` trait。
/// Pipeline 对每个 CallRole 创建一个适配器。
pub struct LlmClientAdapter {
    client: Arc<dyn LlmClient>,
    role: CallRole,
    project_id: String,
}

impl LlmClientAdapter {
    pub fn new(client: Arc<dyn LlmClient>, role: CallRole, project_id: String) -> Self {
        Self {
            client,
            role,
            project_id,
        }
    }
}

#[async_trait]
impl semantic_pipeline::pipeline::LlmProvider for LlmClientAdapter {
    async fn chat(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        _format: semantic_pipeline::pipeline::ResponseFormat,
    ) -> Result<String, String> {
        self.client
            .chat(self.role, system_prompt, user_prompt, &self.project_id, None)
            .await
            .map_err(|e| e.to_string())
    }

    fn name(&self) -> &str {
        self.client.provider_name()
    }

    fn model(&self) -> &str {
        self.client.model_for_role(self.role)
    }
}

// ── OpenAI 兼容客户端 / OpenAI Compatible Client ──────────────────────────

/// OpenAI 兼容 provider 的配置。
#[derive(Clone, Debug)]
pub struct OpenAiCompatibleProviderConfig {
    /// 基础 URL（如 "https://api.deepseek.com" 或 "http://localhost:11434/v1"）。
    pub base_url: String,
    /// Chat completions 路径（如 "/chat/completions"）。
    /// 与 base_url 拼接得到完整 endpoint。
    pub chat_completions_path: String,
    pub api_key: String,
    pub routing: ModelRouting,
    pub timeout_secs: u64,
    pub provider_id: String,
    pub provider_name: String,
}

impl Default for OpenAiCompatibleProviderConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.deepseek.com".to_string(),
            chat_completions_path: "/v1/chat/completions".to_string(),
            api_key: String::new(),
            routing: ModelRouting::default(),
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            provider_id: "builtin-deepseek@1.0.0".to_string(),
            provider_name: "DeepSeek".to_string(),
        }
    }
}

/// OpenAI 兼容客户端：可配置 base URL、模型路由、超时等。
/// 实现 `LlmClient` trait。所有 HTTP 逻辑（重试、截断修复、推理缓存）在此实现。
pub struct OpenAiCompatibleClient {
    http: reqwest::Client,
    config: OpenAiCompatibleProviderConfig,
    reasoning_cache: std::sync::Mutex<HashMap<String, String>>,
}

impl OpenAiCompatibleClient {
    /// 从配置创建客户端。
    pub fn new(config: OpenAiCompatibleProviderConfig) -> Result<Self, LlmError> {
        if config.api_key.trim().is_empty() {
            return Err(LlmError::ApiKeyMissing);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|error| LlmError::HttpError(error.to_string()))?;
        Ok(Self {
            http,
            config,
            reasoning_cache: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// 获取 base URL + path 构成的完整 endpoint。
    fn endpoint(&self) -> String {
        format!(
            "{}{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.chat_completions_path
        )
    }

    /// 获取当前 API key（仅内部使用，绝不泄露到日志）。
    fn auth_header(&self) -> Result<HeaderValue, LlmError> {
        let value = format!("Bearer {}", self.config.api_key);
        HeaderValue::from_str(&value)
            .map_err(|error| LlmError::HttpError(format!("Invalid API key: {error}")))
    }

    /// 构建请求头（不含日志输出 API key）。
    fn build_headers(&self) -> Result<HeaderMap, LlmError> {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, self.auth_header()?);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }

    // ── 核心调用 / Core call ──────────────────────────────────────────

    /// 发送聊天补全请求（含重试与截断修复）。
    /// 返回完整 response —— trait 实现中提取 content 字符串。
    pub async fn chat_full(
        &self,
        role: CallRole,
        system_prompt: &str,
        user_prompt: &str,
        project_id: &str,
        reasoning_key: Option<&str>,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let route = self.config.routing.route(role);
        let user_id = project_user_id(project_id);

        let mut messages = Vec::with_capacity(2);
        messages.push(ChatMessage {
            role: MessageRole::System,
            content: system_prompt.to_string(),
            reasoning_content: None,
        });

        if let Some(key) = reasoning_key {
            if let Some(reasoning) = self.get_cached_reasoning(key) {
                messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!(
                        "[上下文延续] 上一轮推理摘要：{}\n请基于此推理链继续分析。",
                        &reasoning[..reasoning.len().min(500)]
                    ),
                    reasoning_content: None,
                });
            }
        }

        messages.push(ChatMessage {
            role: MessageRole::User,
            content: user_prompt.to_string(),
            reasoning_content: None,
        });

        let response_format = if route.json_output {
            Some(ResponseFormat {
                format_type: "json_object".to_string(),
            })
        } else {
            None
        };

        let request = ChatCompletionRequest {
            model: route.model.clone(),
            messages,
            response_format,
            temperature: Some(0.1),
            max_tokens: if route.thinking {
                Some(8192)
            } else {
                Some(4096)
            },
            stream: Some(false),
            user: Some(user_id),
        };

        self.call_with_retry(&request, role, project_id, reasoning_key)
            .await
    }

    /// 带重试和截断修复的底层调用。
    async fn call_with_retry(
        &self,
        request: &ChatCompletionRequest,
        role: CallRole,
        project_id: &str,
        reasoning_key: Option<&str>,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let url = self.endpoint();
        let mut last_error = String::new();

        for attempt in 1..=MAX_RETRIES {
            let result = self.execute_request(&url, request).await;

            match result {
                Ok(response) => {
                    if let Some(choices) = &response.choices {
                        if let Some(choice) = choices.first() {
                            let content = choice
                                .message
                                .as_ref()
                                .and_then(|m| m.content.as_deref())
                                .unwrap_or("");
                            let finish_reason = choice.finish_reason.as_deref();

                            if let Some(reasoning) = choice
                                .message
                                .as_ref()
                                .and_then(|m| m.reasoning_content.as_deref())
                            {
                                if let Some(key) = reasoning_key {
                                    self.cache_reasoning(key, reasoning);
                                }
                            }

                            if request.response_format.is_some() {
                                match detect_truncation(content, finish_reason)? {
                                    TruncationStatus::Complete => return Ok(response),
                                    TruncationStatus::Truncated {
                                        partial_content,
                                        tail_context,
                                    } => {
                                        eprintln!(
                                            "[llm] JSON truncated (len={}), attempting repair...",
                                            partial_content.len()
                                        );
                                        match self
                                            .repair_truncation(
                                                request,
                                                &partial_content,
                                                &tail_context,
                                                role,
                                                project_id,
                                                reasoning_key,
                                            )
                                            .await
                                        {
                                            Ok(repaired) => return Ok(repaired),
                                            Err(error) => {
                                                last_error = error.to_string();
                                                if attempt < MAX_RETRIES {
                                                    eprintln!(
                                                        "[llm] Repair attempt {} failed, retrying...",
                                                        attempt
                                                    );
                                                    tokio::time::sleep(backoff_duration(attempt))
                                                        .await;
                                                    continue;
                                                }
                                                return Err(error);
                                            }
                                        }
                                    }
                                }
                            }

                            return Ok(response);
                        }
                    }
                    return Ok(response);
                }
                Err(error) => {
                    last_error = error.to_string();
                    if should_terminate(&error) {
                        return Err(error);
                    }
                    if attempt < MAX_RETRIES {
                        eprintln!(
                            "[llm] Attempt {}/{} failed, retrying in {:?}...",
                            attempt,
                            MAX_RETRIES,
                            backoff_duration(attempt)
                        );
                        tokio::time::sleep(backoff_duration(attempt)).await;
                    }
                }
            }
        }

        Err(LlmError::RetryExhausted {
            attempts: MAX_RETRIES,
            last_error,
        })
    }

    /// 读取响应文本并限制最大长度，避免 OOM 与把攻击载荷嵌进错误串。
    async fn bounded_response_text(response: reqwest::Response) -> String {
        match response.bytes().await {
            Ok(bytes) => {
                let truncated = bytes.iter().take(MAX_RESPONSE_BODY_BYTES).copied().collect::<Vec<u8>>();
                String::from_utf8_lossy(&truncated).into_owned()
            }
            Err(error) => format!("(failed to read response body: {})", error),
        }
    }

    /// 执行单次 HTTP 请求（不包含重试逻辑）。
    async fn execute_request(
        &self,
        url: &str,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let headers = self.build_headers()?;
        let body = serde_json::to_vec(request)
            .map_err(|error| LlmError::SerializationError(error.to_string()))?;

        let response = self
            .http
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if status == 200 {
            let bytes = response
                .bytes()
                .await
                .map_err(|error| LlmError::HttpError(error.to_string()))?;
            if bytes.len() > MAX_RESPONSE_BODY_BYTES {
                return Err(LlmError::ServerError {
                    status,
                    body: format!(
                        "response body exceeds {} bytes (got {})",
                        MAX_RESPONSE_BODY_BYTES,
                        bytes.len()
                    ),
                });
            }
            let full_body: Value = serde_json::from_slice(&bytes)
                .map_err(|error| LlmError::ParseError(error.to_string()))?;
            let parsed: ChatCompletionResponse = serde_json::from_value(full_body)
                .map_err(|error| LlmError::ParseError(error.to_string()))?;
            Ok(parsed)
        } else if is_terminal_http_status(status) {
            let body = Self::bounded_response_text(response).await;
            Err(LlmError::ClientError { status, body })
        } else {
            let body = Self::bounded_response_text(response).await;
            Err(LlmError::ServerError { status, body })
        }
    }

    /// 截断修复：构造续写请求并合并输出。
    async fn repair_truncation(
        &self,
        original_request: &ChatCompletionRequest,
        partial_content: &str,
        tail_context: &str,
        _role: CallRole,
        _project_id: &str,
        _reasoning_key: Option<&str>,
    ) -> Result<ChatCompletionResponse, LlmError> {
        let recovery_route = self.config.routing.route(CallRole::Recovery);
        let continuation_request = build_continuation_request(
            &original_request.messages,
            partial_content,
            tail_context,
            recovery_route,
        );

        let url = self.endpoint();
        let mut last_error = String::new();

        for attempt in 1..=2 {
            let result = self.execute_request(&url, &continuation_request).await;
            match result {
                Ok(response) => {
                    if let Some(choices) = &response.choices {
                        if let Some(choice) = choices.first() {
                            let continuation = choice
                                .message
                                .as_ref()
                                .and_then(|m| m.content.as_deref())
                                .unwrap_or("");

                            let merged =
                                merge_truncated_output(partial_content, continuation);
                            let merged_len = merged.len();

                            match serde_json::from_str::<Value>(&merged) {
                                Ok(_) => {
                                    let mut repaired = response.clone();
                                    if let Some(ref mut choices) = repaired.choices {
                                        if let Some(ref mut first) = choices.first_mut() {
                                            if let Some(ref mut msg) = first.message {
                                                msg.content = Some(merged);
                                            }
                                        }
                                    }
                                    eprintln!(
                                        "[llm] Truncation repair succeeded ({} chars merged)",
                                        merged_len
                                    );
                                    return Ok(repaired);
                                }
                                Err(_) => {
                                    if attempt < 2 {
                                        tokio::time::sleep(Duration::from_millis(500)).await;
                                        continue;
                                    }
                                    return Err(LlmError::TruncationRepairFailed(
                                        "Merged JSON still invalid after repair".to_string(),
                                    ));
                                }
                            }
                        }
                    }
                    return Ok(response);
                }
                Err(error) => {
                    last_error = error.to_string();
                    if should_terminate(&error) {
                        return Err(error);
                    }
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }
        }

        Err(LlmError::TruncationRepairFailed(format!(
            "Repair retries exhausted: {last_error}"
        )))
    }

    // ── 推理缓存 / Reasoning cache ─────────────────────────────────────

    fn cache_reasoning(&self, key: &str, reasoning: &str) {
        if let Ok(mut cache) = self.reasoning_cache.lock() {
            if cache.len() >= MAX_REASONING_CACHE_ENTRIES {
                // Evict oldest entry deterministically to cap memory.
                let oldest = cache.keys().next().cloned();
                if let Some(oldest) = oldest {
                    cache.remove(&oldest);
                }
            }
            cache.insert(key.to_string(), reasoning.to_string());
        }
    }

    pub fn get_cached_reasoning(&self, key: &str) -> Option<String> {
        self.reasoning_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
    }

    /// 清除指定 key 的推理缓存。
    pub fn clear_reasoning_cache(&self, key: &str) {
        if let Ok(mut cache) = self.reasoning_cache.lock() {
            cache.remove(key);
        }
    }
}

// ── LlmClient 实现 / LlmClient impl for OpenAiCompatibleClient ────────────

#[async_trait]
impl LlmClient for OpenAiCompatibleClient {
    async fn chat(
        &self,
        role: CallRole,
        system_prompt: &str,
        user_prompt: &str,
        project_id: &str,
        reasoning_key: Option<&str>,
    ) -> Result<String, LlmError> {
        let response = self
            .chat_full(role, system_prompt, user_prompt, project_id, reasoning_key)
            .await?;
        Ok(response
            .choices
            .as_ref()
            .and_then(|c| c.first())
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.as_deref())
            .unwrap_or_default()
            .to_string())
    }

    fn provider_name(&self) -> &str {
        &self.config.provider_name
    }

    fn model_for_role(&self, role: CallRole) -> &str {
        self.config.routing.route(role).model.as_str()
    }

    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }
}

// ── 单元测试 / Unit tests ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_user_id_is_deterministic() {
        let a = project_user_id("project-pinn");
        let b = project_user_id("project-pinn");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn project_user_id_differs_per_project() {
        let a = project_user_id("project-a");
        let b = project_user_id("project-b");
        assert_ne!(a, b);
    }

    #[test]
    fn detect_truncation_complete_when_valid_json() {
        let result = detect_truncation(r#"{"key": "value"}"#, None).unwrap();
        assert!(matches!(result, TruncationStatus::Complete));
    }

    #[test]
    fn detect_truncation_when_length_but_valid_json() {
        let result = detect_truncation(r#"{"a": 1}"#, Some("length")).unwrap();
        assert!(matches!(result, TruncationStatus::Complete));
    }

    #[test]
    fn detect_truncation_when_length_and_invalid_json() {
        let result = detect_truncation(r#"{"a": 1, "b": ["#, Some("length")).unwrap();
        assert!(matches!(result, TruncationStatus::Truncated { .. }));
    }

    #[test]
    fn detect_truncation_rejects_invalid_json_when_not_truncated() {
        let result = detect_truncation(r#"{"a": 1, "b": ["#, None);
        assert!(result.is_err(), "obviously broken JSON should be rejected");
    }

    #[test]
    fn extract_json_substring_finds_outer_object() {
        let text = r#"前缀 {"key": "value"} 后缀"#;
        let extracted = extract_json_substring(text).unwrap();
        assert_eq!(extracted, r#"{"key": "value"}"#);
    }

    #[test]
    fn extract_json_substring_handles_nested_braces() {
        let text = r#"{"outer": {"inner": [1, 2, 3]}}"#;
        let extracted = extract_json_substring(text).unwrap();
        assert_eq!(extracted, r#"{"outer": {"inner": [1, 2, 3]}}"#);
    }

    #[test]
    fn merge_truncated_output_no_overlap() {
        let merged = merge_truncated_output(r#"{"a": "#, r#""value"}"#);
        assert_eq!(merged, r#"{"a":"value"}"#);
    }

    #[test]
    fn merge_truncated_output_with_overlap() {
        let merged =
            merge_truncated_output(r#"{"data": [1, 2, 3"#, r#"3, 4, 5]}"#);
        assert!(merged.contains("[1, 2, 3, 4, 5]"));
    }

    #[test]
    fn is_terminal_status_identifies_non_retryable() {
        assert!(is_terminal_http_status(400));
        assert!(is_terminal_http_status(401));
        assert!(is_terminal_http_status(402));
        assert!(is_terminal_http_status(422));
        assert!(!is_terminal_http_status(429));
        assert!(!is_terminal_http_status(500));
    }

    #[test]
    fn is_retryable_status_identifies_retryable() {
        assert!(is_retryable_http_status(429));
        assert!(is_retryable_http_status(500));
        assert!(is_retryable_http_status(502));
        assert!(is_retryable_http_status(503));
        assert!(is_retryable_http_status(504));
        assert!(!is_retryable_http_status(400));
    }

    #[test]
    fn backoff_grows_exponentially() {
        let d1 = backoff_duration(1);
        let d2 = backoff_duration(2);
        let d3 = backoff_duration(3);
        assert!(d2 > d1, "d2={:?} should be > d1={:?}", d2, d1);
        assert!(d3 > d2, "d3={:?} should be > d2={:?}", d3, d2);
        assert!(d3 <= Duration::from_millis(MAX_BACKOFF_MS + MAX_BACKOFF_MS / 4));
    }

    #[test]
    fn default_routing_maps_roles_correctly() {
        let routing = ModelRouting::default();
        assert_eq!(routing.route(CallRole::Extraction).model, "deepseek-chat");
        assert!(!routing.route(CallRole::Extraction).thinking);
        assert!(routing.route(CallRole::Extraction).json_output);

        assert_eq!(
            routing.route(CallRole::Synthesis).model,
            "deepseek-reasoner"
        );
        assert!(routing.route(CallRole::Synthesis).thinking);
        assert!(routing.route(CallRole::Synthesis).json_output);

        assert_eq!(routing.route(CallRole::Recovery).model, "deepseek-chat");
        assert!(!routing.route(CallRole::Recovery).thinking);
        assert!(!routing.route(CallRole::Recovery).json_output);
    }

    #[test]
    fn uniform_routing_uses_same_model() {
        let routing = ModelRouting::uniform("llama3.1", true);
        assert_eq!(routing.route(CallRole::Extraction).model, "llama3.1");
        assert_eq!(routing.route(CallRole::Synthesis).model, "llama3.1");
        assert_eq!(routing.route(CallRole::Recovery).model, "llama3.1");
        // Recovery 应该关闭 JSON 模式
        assert!(!routing.route(CallRole::Recovery).json_output);
        assert!(routing.route(CallRole::Extraction).json_output);
    }
}
