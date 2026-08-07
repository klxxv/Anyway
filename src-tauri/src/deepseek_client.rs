//! DeepSeek API key 管理 & 向后兼容层 / Backward-compatible API key management.
//!
//! 核心 LLM 类型与逻辑已迁移至 `llm_client` 模块。
//! 本模块保留原有 Tauri 命令 (`set_deepseek_api_key`, `has_deepseek_api_key`,
//! `clear_deepseek_api_key`) 以及 API key 文件存储函数，确保现有用户不受影响。
//!
//! 新代码应使用 `llm_client` 模块中的 `LlmClient` trait 和
//! `llm_provider_registry` 中的 provider 管理命令。

use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tauri::AppHandle;

#[cfg(not(debug_assertions))]
use tauri::Manager;

// ── 重导出核心类型（向后兼容）──────────────────────────────────────────

pub use crate::llm_client::{
    backoff_duration, build_continuation_request, detect_truncation, extract_json_substring,
    is_retryable_http_status, is_terminal_http_status, merge_truncated_output, project_user_id,
    response_to_result, should_terminate, CallRole, ChatCompletionRequest, ChatCompletionResponse,
    ChatMessage, ChatResult, Choice, ChoiceMessage, MessageRole, ModelRoute, ModelRouting,
    ResponseFormat, TruncationStatus, Usage, DEFAULT_TIMEOUT_SECS, INITIAL_BACKOFF_MS,
    MAX_BACKOFF_MS, MAX_RETRIES,
};

// ── 向后兼容类型别名 ─────────────────────────────────────────────────────

/// 向后兼容别名：`DeepSeekError` → `LlmError`。
pub use crate::llm_client::LlmError as DeepSeekError;

/// 向后兼容别名：`DeepSeekClient` → `OpenAiCompatibleClient`。
/// 使用默认 DeepSeek 配置构建。
pub use crate::llm_client::OpenAiCompatibleClient as DeepSeekClient;

// ── 常量 / Constants ──────────────────────────────────────────────────────

pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com";
pub const CHAT_COMPLETIONS_PATH: &str = "/v1/chat/completions";
pub const API_KEY_ENV_VAR: &str = "DEEPSEEK_API_KEY";

// ── API Key 安全存储 / Secure API key storage ─────────────────────────────

/// 在 app data dir 中存储 API key 的配置文件名。
const API_KEY_CONFIG_FILE: &str = "deepseek-config.json";

/// 获取存储 API key 配置的基目录。
/// 在开发模式下使用仓库根目录，发布模式下使用 Tauri app data 目录。
fn app_data_base(_app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        Ok(manifest_dir
            .parent()
            .ok_or_else(|| "Cannot resolve repository root".to_string())?
            .to_path_buf())
    }
    #[cfg(not(debug_assertions))]
    {
        _app.path()
            .app_data_dir()
            .map_err(|error: tauri::Error| error.to_string())
    }
}

/// 从 Tauri app data 目录读取 API key。
pub fn read_api_key_from_config(app: &AppHandle) -> Result<String, String> {
    let base = app_data_base(app)?;
    let config_path = base.join(API_KEY_CONFIG_FILE);
    if !config_path.is_file() {
        return Err("API key not configured".to_string());
    }
    let bytes = std::fs::read(&config_path)
        .map_err(|error| format!("Cannot read config: {error}"))?;
    let config: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Parse error: {error}"))?;
    config
        .get("apiKey")
        .and_then(Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "API key missing in config".to_string())
}

/// 将 API key 写入 Tauri app data 目录。
pub fn write_api_key_to_config(app: &AppHandle, api_key: &str) -> Result<(), String> {
    let base = app_data_base(app)?;
    std::fs::create_dir_all(&base)
        .map_err(|error| format!("Cannot create config dir: {error}"))?;
    let config_path = base.join(API_KEY_CONFIG_FILE);
    let config = json!({ "apiKey": api_key });
    let bytes = serde_json::to_vec_pretty(&config)
        .map_err(|error| format!("Serialization error: {error}"))?;
    std::fs::write(&config_path, bytes)
        .map_err(|error| format!("Cannot write config: {error}"))?;
    #[cfg(target_family = "unix")]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600));
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("attrib")
            .args(["+H", config_path.to_str().unwrap_or("")])
            .output();
    }
    Ok(())
}

// ── Tauri 命令 / Tauri commands ────────────────────────────────────────────

/// 设置 API key 的请求体。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetApiKeyRequest {
    pub api_key: String,
}

/// 存储 DeepSeek API key 到本地配置。
#[tauri::command]
pub fn set_deepseek_api_key(app: AppHandle, request: SetApiKeyRequest) -> Result<(), String> {
    let key = request.api_key.trim().to_string();
    if key.is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    if key.len() > 256 {
        return Err("API key is too long".to_string());
    }
    write_api_key_to_config(&app, &key)
}

/// 检查 DeepSeek API key 是否已配置（不返回 key 本身）。
#[tauri::command]
pub fn has_deepseek_api_key(app: AppHandle) -> Result<bool, String> {
    if let Ok(key) = std::env::var(API_KEY_ENV_VAR) {
        if !key.trim().is_empty() {
            return Ok(true);
        }
    }
    match read_api_key_from_config(&app) {
        Ok(_) => Ok(true),
        Err(e) if e == "API key not configured" || e == "API key missing in config" => Ok(false),
        Err(error) => Err(error),
    }
}

/// 清除已存储的 DeepSeek API key。
#[tauri::command]
pub fn clear_deepseek_api_key(app: AppHandle) -> Result<(), String> {
    let base = app_data_base(&app)?;
    let config_path = base.join(API_KEY_CONFIG_FILE);
    if config_path.is_file() {
        std::fs::remove_file(&config_path)
            .map_err(|error| format!("Cannot remove config: {error}"))?;
    }
    Ok(())
}
