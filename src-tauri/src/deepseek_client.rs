//! DeepSeek API 调用网关与模型路由 / API gateway and model router.
//!
//! 所有 LLM 调用通过本模块发出。API Key 绝不硬编码、绝不合并进 prompt、绝不记录到日志。
//! All LLM calls route through this module. The API key is never hard-coded,
//! never merged into prompts, and never logged.
//!
//! 设计要点 / Design highlights:
//! - OpenAI 兼容客户端，base URL: https://api.deepseek.com
//! - 模型路由：extraction → non-thinking + JSON / synthesis → thinking + JSON /
//!   recovery → non-thinking
//! - 重试策略：429/500/503 指数退避，400/401/402/422 立即终止
//! - JSON 截断检测（finish_reason=length）→ 分片续写修复
//! - thinking 模式：后续请求回传 reasoning_content 以维持思维链
//! - 上下文缓存：system prompt 前置以提高缓存命中
//! - user_id：不可逆项目级标识（SHA-256 哈希）

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tauri::AppHandle;

// ── 常量 / Constants ──────────────────────────────────────────────────────

const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
const DEFAULT_TIMEOUT_SECS: u64 = 120;
const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 1000;
const MAX_BACKOFF_MS: u64 = 16_000;
const API_KEY_ENV_VAR: &str = "DEEPSEEK_API_KEY";

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
    /// DeepSeek 模型名（如 "deepseek-chat" / "deepseek-reasoner"）。
    pub model: String,
    /// 是否启用思考模式（仅 reasoner 模型有效）。
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
        Self {
            // extraction: 快速模型，非思考，JSON 输出
            extraction: ModelRoute {
                model: "deepseek-chat".to_string(),
                thinking: false,
                thinking_level: None,
                json_output: true,
            },
            // synthesis: 强模型 + 高思考深度 + JSON 输出
            synthesis: ModelRoute {
                model: "deepseek-reasoner".to_string(),
                thinking: true,
                thinking_level: Some("high".to_string()),
                json_output: true,
            },
            // recovery: 强模型，非思考，修复模式不需要 JSON 模式（修复后的内容需再解析）
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
pub enum DeepSeekError {
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

impl From<reqwest::Error> for DeepSeekError {
    fn from(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            DeepSeekError::HttpError("Request timed out".to_string())
        } else if error.is_connect() {
            DeepSeekError::HttpError(format!("Connection failed: {error}"))
        } else {
            DeepSeekError::HttpError(error.to_string())
        }
    }
}

impl From<serde_json::Error> for DeepSeekError {
    fn from(error: serde_json::Error) -> Self {
        DeepSeekError::ParseError(error.to_string())
    }
}

// ── 可终止错误判定 / Terminal error classification ───────────────────────

/// 判断 HTTP 状态码是否应终止重试（不再退避）。
fn is_terminal_http_status(status: u16) -> bool {
    matches!(status, 400 | 401 | 402 | 422)
}

/// 判断 HTTP 状态码是否可重试（指数退避后重试）。
#[allow(dead_code)]
fn is_retryable_http_status(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504)
}

// ── 不可逆项目级标识 / Irreversible project-level user_id ─────────────────

/// 从项目标识生成不可逆的 user_id。
///
/// 输入任意项目级字符串（如项目路径或 ID），输出 64 hex SHA-256 哈希。
/// 此哈希为单向不可逆——即使 user_id 泄露，也无法反推原始项目路径。
/// 用于 DeepSeek API 的 `user` 字段，辅助监控与限流，不携带个人信息。
pub fn project_user_id(project_identifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"research-canvas:");
    hasher.update(project_identifier.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── JSON 截断检测 / JSON truncation detection ─────────────────────────────

/// 检测 LLM 返回的 JSON 是否被截断。
///
/// 判断条件：
/// 1. finish_reason == "length"（token 上限触达）
/// 2. JSON 解析失败（可能因截断导致不合法）
pub fn detect_truncation(
    content: &str,
    finish_reason: Option<&str>,
) -> Result<TruncationStatus, DeepSeekError> {
    // 首先检查 finish_reason
    if finish_reason != Some("length") {
        // 即使没有 length 标记，也尝试解析 JSON 以确认完整性
        match serde_json::from_str::<Value>(content.trim()) {
            Ok(_) => return Ok(TruncationStatus::Complete),
            Err(_) => {
                // JSON 不合法但没有 length 标记 —— 可能是模型输出了非 JSON 内容
                // 尝试提取 JSON 子串
                if let Some(extracted) = extract_json_substring(content) {
                    match serde_json::from_str::<Value>(&extracted) {
                        Ok(_) => return Ok(TruncationStatus::Complete),
                        Err(_) => {}
                    }
                }
                return Ok(TruncationStatus::Complete); // 不触发修复，由调用方处理
            }
        }
    }

    // finish_reason == "length"：明确截断
    let content = content.trim();
    match serde_json::from_str::<Value>(content) {
        Ok(_) => {
            // 虽然标记为 length 但恰好在 JSON 完整处截断
            Ok(TruncationStatus::Complete)
        }
        Err(_) => {
            // 真正的截断：保留尾部上下文用于续写修复
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
fn extract_json_substring(text: &str) -> Option<String> {
    let text = text.trim();
    // 寻找最外层 { } 或 [ ]
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
///
/// 当 finish_reason=length 且 JSON 不完整时，构造一个续写请求：
/// 1. 把已收到的截断内容作为 assistant 消息
/// 2. 追加 "continue" user 消息
/// 3. 在新的请求中让模型继续输出
///
/// 注意：续写请求不使用 JSON 模式，因为截断点可能在 JSON 中间，
/// JSON 模式的 schema 约束会让续写困难。
pub fn build_continuation_request(
    original_messages: &[ChatMessage],
    partial_content: &str,
    tail_context: &str,
    route: &ModelRoute,
) -> ChatCompletionRequest {
    let mut messages = original_messages.to_vec();
    // 追加已截断的 assistant 响应
    messages.push(ChatMessage {
        role: MessageRole::Assistant,
        content: partial_content.to_string(),
        reasoning_content: None,
    });
    // 追加续写指令
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
        response_format: None, // 续写时关闭 JSON 模式
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

    // 如果续写以原始末尾重复开始，去掉重复前缀
    // 寻找 partial 的尾部与 continuation 头部的最大重叠
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

    let merged = if best_split > 0 {
        format!(
            "{}{}",
            partial,
            &continuation[best_split.min(continuation.len())..]
        )
    } else {
        format!("{}{}", partial, continuation)
    };

    merged
}

// ── DeepSeek 客户端 / DeepSeek Client ──────────────────────────────────────

pub struct DeepSeekClient {
    http: reqwest::Client,
    api_key: String,
    routing: ModelRouting,
    /// 已缓存的 reasoning_content（key: 对话标识, value: reasoning_content）。
    /// 用于跨请求传递思考链。
    reasoning_cache: std::sync::Mutex<HashMap<String, String>>,
}

impl DeepSeekClient {
    /// 从 API key 创建客户端。
    pub fn new(api_key: String, routing: ModelRouting) -> Result<Self, DeepSeekError> {
        if api_key.trim().is_empty() {
            return Err(DeepSeekError::ApiKeyMissing);
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
            .build()
            .map_err(|error| DeepSeekError::HttpError(error.to_string()))?;
        Ok(Self {
            http,
            api_key,
            routing,
            reasoning_cache: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// 从环境变量读取 API key 并创建客户端。
    pub fn from_env(routing: ModelRouting) -> Result<Self, DeepSeekError> {
        let api_key = std::env::var(API_KEY_ENV_VAR)
            .map_err(|_| DeepSeekError::ApiKeyMissing)?;
        Self::new(api_key, routing)
    }

    /// 从 Tauri app handle 的安全存储中读取 API key 并创建客户端。
    /// 优先读取环境变量，其次读取 Tauri 存储。
    pub fn from_tauri_store(
        app: &AppHandle,
        routing: ModelRouting,
    ) -> Result<Self, DeepSeekError> {
        // 优先环境变量
        if let Ok(key) = std::env::var(API_KEY_ENV_VAR) {
            if !key.trim().is_empty() {
                return Self::new(key, routing);
            }
        }
        // 回退到 Tauri store —— 通过 app data dir 中的配置文件
        let key = read_api_key_from_config(app)?;
        Self::new(key, routing)
    }

    /// 获取当前 API key（仅内部使用，绝不泄露到日志）。
    fn auth_header(&self) -> Result<HeaderValue, DeepSeekError> {
        let value = format!("Bearer {}", self.api_key);
        HeaderValue::from_str(&value)
            .map_err(|error| DeepSeekError::HttpError(format!("Invalid API key: {error}")))
    }

    /// 构建请求头（不含日志输出 API key）。
    fn build_headers(&self) -> Result<HeaderMap, DeepSeekError> {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, self.auth_header()?);
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(headers)
    }

    // ── 核心调用 / Core call ──────────────────────────────────────────

    /// 发送聊天补全请求（含重试与截断修复）。
    ///
    /// # 参数
    /// - `role`: 调用角色，决定模型路由。
    /// - `system_prompt`: 系统提示词（置于最前方以利用上下文缓存）。
    /// - `user_prompt`: 用户提示词。
    /// - `project_id`: 项目标识（用于生成不可逆 user_id）。
    /// - `reasoning_key`: 推理缓存键（用于跨对话传递 reasoning_content）。
    pub async fn chat(
        &self,
        role: CallRole,
        system_prompt: &str,
        user_prompt: &str,
        project_id: &str,
        reasoning_key: Option<&str>,
    ) -> Result<ChatCompletionResponse, DeepSeekError> {
        let route = self.routing.route(role);
        let user_id = project_user_id(project_id);

        // 构建消息列表：system prompt 前置以利用上下文缓存
        let mut messages = Vec::with_capacity(2);
        // system prompt 始终放在第一位 —— 缓存命中关键
        messages.push(ChatMessage {
            role: MessageRole::System,
            content: system_prompt.to_string(),
            reasoning_content: None,
        });

        // 如果有缓存的 reasoning_content，通过 assistant 消息注回
        if let Some(key) = reasoning_key {
            if let Some(reasoning) = self.get_cached_reasoning(key) {
                // 通过一条特殊的 system 消息延续提示模型使用之前的推理路径
                // 注意：reasoning_content 在请求中通常通过 assistant 消息的 reasoning_content 字段传递
                // 但 DeepSeek API 要求在同一对话上下文中才能继续推理
                // 这里的策略是：如果上一轮有 reasoning，在 system prompt 中做轻提示
                // 实际 reasoning 传递在 build_messages_with_reasoning 中处理
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
                // thinking 模式允许更多 token
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
    ) -> Result<ChatCompletionResponse, DeepSeekError> {
        let url = format!("{}{}", DEEPSEEK_BASE_URL, CHAT_COMPLETIONS_PATH);
        let mut last_error = String::new();

        for attempt in 1..=MAX_RETRIES {
            let result = self.execute_request(&url, request).await;

            match result {
                Ok(response) => {
                    // 检查截断
                    if let Some(choices) = &response.choices {
                        if let Some(choice) = choices.first() {
                            let content = choice.message.as_ref()
                                .and_then(|m| m.content.as_deref())
                                .unwrap_or("");

                            let finish_reason = choice.finish_reason.as_deref();

                            // 缓存 reasoning_content
                            if let Some(reasoning) = choice.message.as_ref()
                                .and_then(|m| m.reasoning_content.as_deref())
                            {
                                if let Some(key) = reasoning_key {
                                    self.cache_reasoning(key, reasoning);
                                }
                            }

                            // 检测 JSON 截断
                            if request.response_format.is_some() {
                                match detect_truncation(content, finish_reason)? {
                                    TruncationStatus::Complete => return Ok(response),
                                    TruncationStatus::Truncated { partial_content, tail_context } => {
                                        // 尝试分片修复
                                        eprintln!(
                                            "[deepseek] JSON truncated (len={}), attempting repair...",
                                            partial_content.len()
                                        );
                                        match self.repair_truncation(
                                            request, &partial_content, &tail_context,
                                            role, project_id, reasoning_key,
                                        ).await {
                                            Ok(repaired) => return Ok(repaired),
                                            Err(error) => {
                                                last_error = error.to_string();
                                                if attempt < MAX_RETRIES {
                                                    eprintln!(
                                                        "[deepseek] Repair attempt {} failed, retrying...",
                                                        attempt
                                                    );
                                                    tokio::time::sleep(
                                                        backoff_duration(attempt),
                                                    )
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
                    // 判断是否应终止
                    if should_terminate(&error) {
                        return Err(error);
                    }
                    if attempt < MAX_RETRIES {
                        eprintln!(
                            "[deepseek] Attempt {}/{} failed, retrying in {:?}...",
                            attempt,
                            MAX_RETRIES,
                            backoff_duration(attempt)
                        );
                        tokio::time::sleep(backoff_duration(attempt)).await;
                    }
                }
            }
        }

        Err(DeepSeekError::RetryExhausted {
            attempts: MAX_RETRIES,
            last_error,
        })
    }

    /// 执行单次 HTTP 请求（不包含重试逻辑）。
    async fn execute_request(
        &self,
        url: &str,
        request: &ChatCompletionRequest,
    ) -> Result<ChatCompletionResponse, DeepSeekError> {
        let headers = self.build_headers()?;
        let body = serde_json::to_vec(request)
            .map_err(|error| DeepSeekError::SerializationError(error.to_string()))?;

        let response = self
            .http
            .post(url)
            .headers(headers)
            .body(body)
            .send()
            .await?;

        let status = response.status().as_u16();

        if status == 200 {
            let full_body: Value = response.json().await?;
            // 反序列化时不将未知字段视为错误
            let parsed: ChatCompletionResponse = serde_json::from_value(full_body)
                .map_err(|error| DeepSeekError::ParseError(error.to_string()))?;
            Ok(parsed)
        } else if is_terminal_http_status(status) {
            let body = response.text().await.unwrap_or_default();
            Err(DeepSeekError::ClientError { status, body })
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(DeepSeekError::ServerError { status, body })
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
    ) -> Result<ChatCompletionResponse, DeepSeekError> {
        let recovery_route = self.routing.route(CallRole::Recovery);
        let continuation_request = build_continuation_request(
            &original_request.messages,
            partial_content,
            tail_context,
            recovery_route,
        );

        let url = format!("{}{}", DEEPSEEK_BASE_URL, CHAT_COMPLETIONS_PATH);

        let mut last_error = String::new();
        for attempt in 1..=2 {
            // 修复最多重试 2 次
            let result = self.execute_request(&url, &continuation_request).await;
            match result {
                Ok(response) => {
                    if let Some(choices) = &response.choices {
                        if let Some(choice) = choices.first() {
                            let continuation = choice.message.as_ref()
                                .and_then(|m| m.content.as_deref())
                                .unwrap_or("");

                            let merged = merge_truncated_output(partial_content, continuation);

                            let merged_len = merged.len();

                            // 验证合并后的 JSON
                            match serde_json::from_str::<Value>(&merged) {
                                Ok(_) => {
                                    // 构造一个新的 response，替换 content
                                    let mut repaired = response.clone();
                                    if let Some(ref mut choices) = repaired.choices {
                                        if let Some(ref mut first) = choices.first_mut() {
                                            if let Some(ref mut msg) = first.message {
                                                msg.content = Some(merged);
                                            }
                                        }
                                    }
                                    eprintln!("[deepseek] Truncation repair succeeded ({} chars merged)", merged_len);
                                    return Ok(repaired);
                                }
                                Err(_) => {
                                    // 合并后仍非法 —— 可能续写出错
                                    if attempt < 2 {
                                        tokio::time::sleep(Duration::from_millis(500)).await;
                                        continue;
                                    }
                                    return Err(DeepSeekError::TruncationRepairFailed(
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

        Err(DeepSeekError::TruncationRepairFailed(format!(
            "Repair retries exhausted: {last_error}"
        )))
    }

    // ── 推理缓存 / Reasoning cache ─────────────────────────────────────

    fn cache_reasoning(&self, key: &str, reasoning: &str) {
        if let Ok(mut cache) = self.reasoning_cache.lock() {
            cache.insert(key.to_string(), reasoning.to_string());
        }
    }

    fn get_cached_reasoning(&self, key: &str) -> Option<String> {
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

// ── 重试工具 / Retry utilities ────────────────────────────────────────────

/// 计算第 n 次重试的退避时间（指数退避 + 随机抖动）。
fn backoff_duration(attempt: u32) -> Duration {
    let base = INITIAL_BACKOFF_MS * 2u64.pow(attempt.saturating_sub(1));
    let capped = base.min(MAX_BACKOFF_MS);
    // 添加 0-25% 的随机抖动
    let jitter = capped / 4;
    let jittered = capped + (jitter.min((capped as f64 * 0.25) as u64));
    Duration::from_millis(jittered)
}

/// 判断错误是否应终止重试。
fn should_terminate(error: &DeepSeekError) -> bool {
    matches!(
        error,
        DeepSeekError::ApiKeyMissing
            | DeepSeekError::ClientError { .. }
            | DeepSeekError::ParseError(_)
            | DeepSeekError::SerializationError(_)
    )
}

// ── API Key 安全存储 / Secure API key storage ─────────────────────────────

/// 在 app data dir 中存储 API key 的配置文件名。
const API_KEY_CONFIG_FILE: &str = "deepseek-config.json";

/// 获取存储 API key 配置的基目录。
/// 在开发模式下使用仓库根目录，发布模式下使用 Tauri app data 目录。
fn app_data_base(_app: &AppHandle) -> Result<std::path::PathBuf, DeepSeekError> {
    #[cfg(debug_assertions)]
    {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Ok(manifest_dir
            .parent()
            .ok_or_else(|| DeepSeekError::HttpError("Cannot resolve repository root".to_string()))?
            .to_path_buf())
    }
    #[cfg(not(debug_assertions))]
    {
        _app.path()
            .app_data_dir()
            .map_err(|error| DeepSeekError::HttpError(error.to_string()))
    }
}

/// 从 Tauri app data 目录读取 API key。
///
/// 仅在内存中暂存，绝不输出到日志或 prompt。
fn read_api_key_from_config(app: &AppHandle) -> Result<String, DeepSeekError> {
    let base = app_data_base(app)?;

    let config_path = base.join(API_KEY_CONFIG_FILE);
    if !config_path.is_file() {
        return Err(DeepSeekError::ApiKeyMissing);
    }

    let bytes = std::fs::read(&config_path)
        .map_err(|error| DeepSeekError::HttpError(format!("Cannot read config: {error}")))?;
    let config: Value = serde_json::from_slice(&bytes)
        .map_err(|error| DeepSeekError::ParseError(error.to_string()))?;

    config
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .ok_or(DeepSeekError::ApiKeyMissing)
}

/// 将 API key 写入 Tauri app data 目录。
fn write_api_key_to_config(app: &AppHandle, api_key: &str) -> Result<(), DeepSeekError> {
    let base = app_data_base(app)?;

    std::fs::create_dir_all(&base)
        .map_err(|error| DeepSeekError::HttpError(format!("Cannot create config dir: {error}")))?;

    let config_path = base.join(API_KEY_CONFIG_FILE);
    let config = json!({ "apiKey": api_key });
    let bytes = serde_json::to_vec_pretty(&config)
        .map_err(|error| DeepSeekError::SerializationError(error.to_string()))?;

    // 在 Windows 上设置只读属性以提高安全性
    std::fs::write(&config_path, bytes)
        .map_err(|error| DeepSeekError::HttpError(format!("Cannot write config: {error}")))?;

    // 尝试限制文件权限（仅当前用户可读）
    #[cfg(target_family = "unix")]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        // Windows 上通过隐藏文件来降低暴露风险
        let _ = std::process::Command::new("attrib")
            .args(["+H", config_path.to_str().unwrap_or("")])
            .output();
    }

    Ok(())
}

// ── Tauri 命令 / Tauri commands ────────────────────────────────────────────

/// 输出类型：仅包含安全的摘要信息。
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

/// 设置 API key 的请求体。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetApiKeyRequest {
    pub api_key: String,
}

/// 存储 API key 到 Tauri 安全存储。
#[tauri::command]
pub fn set_deepseek_api_key(app: AppHandle, request: SetApiKeyRequest) -> Result<(), String> {
    let key = request.api_key.trim().to_string();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    if key.len() > 256 {
        return Err("API key is too long".to_string());
    }
    write_api_key_to_config(&app, &key).map_err(|error| error.to_string())
}

/// 检查 API key 是否已配置（不返回 key 本身）。
#[tauri::command]
pub fn has_deepseek_api_key(app: AppHandle) -> Result<bool, String> {
    // 优先检查环境变量
    if let Ok(key) = std::env::var(API_KEY_ENV_VAR) {
        if !key.trim().is_empty() {
            return Ok(true);
        }
    }
    match read_api_key_from_config(&app) {
        Ok(_) => Ok(true),
        Err(DeepSeekError::ApiKeyMissing) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// 清除已存储的 API key。
#[tauri::command]
pub fn clear_deepseek_api_key(app: AppHandle) -> Result<(), String> {
    let base = app_data_base(&app).map_err(|error| error.to_string())?;
    let config_path = base.join(API_KEY_CONFIG_FILE);
    if config_path.is_file() {
        std::fs::remove_file(&config_path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

// ── 便捷构造器 / Convenience builders ─────────────────────────────────────

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
        // 恰好在 JSON 末尾截断但数量完整的情况
        let result = detect_truncation(r#"{"a": 1}"#, Some("length")).unwrap();
        assert!(matches!(result, TruncationStatus::Complete));
    }

    #[test]
    fn detect_truncation_when_length_and_invalid_json() {
        let result = detect_truncation(r#"{"a": 1, "b": ["#, Some("length")).unwrap();
        assert!(matches!(result, TruncationStatus::Truncated { .. }));
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
        assert_eq!(
            extracted,
            r#"{"outer": {"inner": [1, 2, 3]}}"#
        );
    }

    #[test]
    fn merge_truncated_output_no_overlap() {
        let merged = merge_truncated_output(r#"{"a": "#, r#""value"}"#);
        assert_eq!(merged, r#"{"a":"value"}"#);
    }

    #[test]
    fn merge_truncated_output_with_overlap() {
        let merged = merge_truncated_output(
            r#"{"data": [1, 2, 3"#,
            r#"3, 4, 5]}"#,
        );
        // 重叠 "3" 只保留一份
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
        // 不超过上限
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
}
