//! ProviderPlugin 描述符类型 / ProviderPlugin descriptor types.
//!
//! ProviderPlugin 是 host-mediated 插件，通过 `provider.json` 声明 LLM 后端配置。
//! 宿主（Tauri core）负责实际的 HTTP 调用；插件只提供配置（URL、模型路由、认证方式）。

use serde::{Deserialize, Serialize};

use crate::llm_client::ModelRouting;

/// ProviderPlugin 入口文件 `provider.json` 的顶层结构。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub schema_version: u64,
    pub provider: ProviderConfig,
}

/// Provider 配置。
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    /// Provider 类型：目前仅支持 "openai-compatible"。
    /// 将来可扩展为 "anthropic"、"google" 等。
    #[serde(rename = "type")]
    pub provider_type: String,

    /// API 基础 URL（如 "https://api.deepseek.com" 或 "http://localhost:11434/v1"）。
    pub base_url: String,

    /// Chat completions 端点路径（如 "/v1/chat/completions" 或 "/chat/completions"）。
    pub chat_completions_path: String,

    /// 默认模型路由表（extraction / synthesis / recovery 各用哪个模型）。
    pub default_routing: ModelRouting,

    /// 是否需要 API key。
    #[serde(default)]
    pub requires_api_key: bool,

    /// API key 的 UI 标签（如 "DeepSeek API Key"、"OpenAI API Key"）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_label: Option<String>,

    /// 请求超时时间（秒），默认 120。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
}

/// 验证 `provider.json` 描述符。
pub fn validate_provider_descriptor(descriptor: &ProviderDescriptor) -> Result<(), String> {
    if descriptor.schema_version != 1 {
        return Err(format!(
            "Unsupported provider.json schemaVersion: {}",
            descriptor.schema_version
        ));
    }

    let provider = &descriptor.provider;

    if provider.provider_type != "openai-compatible" {
        return Err(format!(
            "Unsupported provider type: {}. Only 'openai-compatible' is supported.",
            provider.provider_type
        ));
    }

    if provider.base_url.is_empty() {
        return Err("Provider base URL must not be empty".to_string());
    }

    // 本地/局域网 URL 允许 HTTP，远程 URL 要求 HTTPS
    let is_local = provider.base_url.starts_with("http://localhost")
        || provider.base_url.starts_with("http://127.")
        || provider.base_url.starts_with("http://192.168.")
        || provider.base_url.starts_with("http://10.")
        || provider.base_url.starts_with("http://172.");
    if !provider.base_url.starts_with("https://") && !is_local {
        return Err(format!(
            "Provider base URL must use HTTPS or be a local address: {}",
            provider.base_url
        ));
    }

    if provider.chat_completions_path.is_empty() {
        return Err("Provider chat completions path must not be empty".to_string());
    }

    Ok(())
}
