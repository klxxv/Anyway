//! LLM Provider 注册表 / LLM Provider Registry.
//!
//! 管理所有已安装的 ProviderPlugin，跟踪活跃 provider，存储 API key。
//! 提供 Tauri 命令供前端调用。

use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

use crate::deepseek_client; // 向后兼容的 key 存储
use crate::llm_client::{
    LlmClient, ModelRouting, OpenAiCompatibleClient, OpenAiCompatibleProviderConfig,
};
use crate::llm_plugin::ProviderDescriptor;

/// 序列化给前端的 provider 摘要信息。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub plugin_id: String,
    pub plugin_version: String,
    pub provider_name: String,
    pub provider_type: String,
    pub base_url: String,
    pub is_active: bool,
    pub has_api_key: bool,
    pub requires_api_key: bool,
    pub api_key_label: Option<String>,
    pub default_routing: ModelRouting,
}

/// Provider 注册表。
pub struct ProviderRegistry {
    /// 所有已安装的 ProviderPlugin，key = "id@version"。
    providers: HashMap<String, ProviderDescriptor>,
    /// 当前活跃的 provider key。
    active_provider: Option<String>,
    /// 每个 provider 的 API key（仅内存，绝不输出到日志）。
    api_keys: HashMap<String, String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            active_provider: None,
            api_keys: HashMap::new(),
        }
    }

    /// 从已安装插件列表中刷新 provider 列表。
    pub fn refresh(&mut self, app: &AppHandle) -> Result<(), String> {
        let plugins = crate::plugins::list_installed_plugins(app.clone())?;
        self.providers.clear();
        for plugin in plugins {
            if let Some(provider) = &plugin.provider {
                let key = format!(
                    "{}@{}",
                    plugin.manifest.metadata.id, plugin.manifest.metadata.version
                );
                self.providers.insert(key, provider.clone());
            }
        }
        // 如果活跃 provider 已不存在，清除选择
        if let Some(ref active) = self.active_provider {
            if !self.providers.contains_key(active) {
                self.active_provider = None;
            }
        }
        Ok(())
    }

    /// 构建当前活跃 provider 的 `LlmClient`。
    pub fn build_client(&self) -> Result<Arc<dyn LlmClient>, String> {
        let active = self.active_provider.as_ref().ok_or_else(|| {
            "No active LLM provider configured. Install a ProviderPlugin and set it as active."
                .to_string()
        })?;

        let descriptor = self
            .providers
            .get(active)
            .ok_or_else(|| format!("Active provider {active} not found"))?;

        let api_key = self
            .api_keys
            .get(active)
            .cloned()
            .or_else(|| {
                // 回退：尝试从旧的 DeepSeek 配置文件读取（向后兼容）
                // 仅在 active provider 是 built-in DeepSeek 时生效
                None
            })
            .ok_or_else(|| {
                format!(
                    "No API key configured for provider {active}. Use set_llm_api_key to configure it."
                )
            })?;

        let config = OpenAiCompatibleProviderConfig {
            base_url: descriptor.provider.base_url.clone(),
            chat_completions_path: descriptor.provider.chat_completions_path.clone(),
            api_key,
            routing: descriptor.provider.default_routing.clone(),
            timeout_secs: descriptor.provider.timeout_secs.unwrap_or(120),
            provider_id: active.clone(),
            provider_name: descriptor
                .provider
                .api_key_label
                .clone()
                .unwrap_or_else(|| active.clone()),
        };

        Ok(Arc::new(
            OpenAiCompatibleClient::new(config).map_err(|e| e.to_string())?,
        ))
    }

    /// 设置活跃 provider。
    pub fn set_active(&mut self, provider_key: &str) -> Result<(), String> {
        if !self.providers.contains_key(provider_key) {
            return Err(format!("Provider {provider_key} is not installed"));
        }
        self.active_provider = Some(provider_key.to_string());
        Ok(())
    }

    /// 存储 provider 的 API key（持久化到磁盘）。
    pub fn set_api_key(
        &mut self,
        app: &AppHandle,
        provider_key: &str,
        api_key: &str,
    ) -> Result<(), String> {
        if !self.providers.contains_key(provider_key) {
            return Err(format!("Provider {provider_key} is not installed"));
        }
        let key = api_key.trim().to_string();
        if key.is_empty() {
            return Err("API key cannot be empty".to_string());
        }
        if key.len() > 512 {
            return Err("API key is too long".to_string());
        }

        // 持久化到磁盘（使用现有的 key 存储基础设施）
        deepseek_client::write_api_key_to_config(app, &key)?;

        self.api_keys.insert(provider_key.to_string(), key);
        Ok(())
    }

    /// 检查 provider 是否已配置 API key。
    pub fn has_api_key(&self, provider_key: &str) -> bool {
        self.api_keys.contains_key(provider_key)
    }

    /// 列出所有已安装的 provider。
    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|(key, descriptor)| {
                let (plugin_id, plugin_version) =
                    key.split_once('@').unwrap_or((key.as_str(), "unknown"));
                ProviderInfo {
                    plugin_id: plugin_id.to_string(),
                    plugin_version: plugin_version.to_string(),
                    provider_name: descriptor
                        .provider
                        .api_key_label
                        .clone()
                        .unwrap_or_else(|| key.clone()),
                    provider_type: descriptor.provider.provider_type.clone(),
                    base_url: descriptor.provider.base_url.clone(),
                    is_active: self.active_provider.as_deref() == Some(key.as_str()),
                    has_api_key: self.api_keys.contains_key(key),
                    requires_api_key: descriptor.provider.requires_api_key,
                    api_key_label: descriptor.provider.api_key_label.clone(),
                    default_routing: descriptor.provider.default_routing.clone(),
                }
            })
            .collect()
    }

    /// 清除 provider 的 API key。
    pub fn clear_api_key(&mut self, provider_key: &str) {
        self.api_keys.remove(provider_key);
    }
}

/// Tauri 管理的状态类型。
pub struct ProviderRegistryState(pub Mutex<ProviderRegistry>);

impl Default for ProviderRegistryState {
    fn default() -> Self {
        Self(Mutex::new(ProviderRegistry::new()))
    }
}

// ── Tauri 命令 / Tauri commands ────────────────────────────────────────────

/// 列出所有已安装的 LLM provider 插件。
#[tauri::command]
pub fn list_llm_providers(
    state: State<'_, ProviderRegistryState>,
    app: AppHandle,
) -> Result<Vec<ProviderInfo>, String> {
    let mut registry = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    registry.refresh(&app)?;
    Ok(registry.list_providers())
}

/// 设置当前活跃的 LLM provider。
#[tauri::command]
pub fn set_active_llm_provider(
    state: State<'_, ProviderRegistryState>,
    plugin_id: String,
    plugin_version: String,
) -> Result<(), String> {
    let key = format!("{plugin_id}@{plugin_version}");
    let mut registry = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    registry.set_active(&key)
}

/// 设置 provider 的 API key。
#[tauri::command]
pub fn set_llm_api_key(
    state: State<'_, ProviderRegistryState>,
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    api_key: String,
) -> Result<(), String> {
    let key = format!("{plugin_id}@{plugin_version}");
    let mut registry = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    registry.set_api_key(&app, &key, &api_key)
}

/// 检查 provider 是否已配置 API key。
#[tauri::command]
pub fn has_llm_api_key(
    state: State<'_, ProviderRegistryState>,
    plugin_id: String,
    plugin_version: String,
) -> Result<bool, String> {
    let key = format!("{plugin_id}@{plugin_version}");
    let registry = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    Ok(registry.has_api_key(&key))
}

/// 清除 provider 的 API key。
#[tauri::command]
pub fn clear_llm_api_key(
    state: State<'_, ProviderRegistryState>,
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
) -> Result<(), String> {
    let key = format!("{plugin_id}@{plugin_version}");
    let mut registry = state.0.lock().map_err(|e| format!("Lock error: {e}"))?;
    registry.clear_api_key(&key);
    // 同时清除磁盘上的 key
    let _ = deepseek_client::clear_deepseek_api_key(app);
    Ok(())
}
