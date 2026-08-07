//! 管线配置加载——从 YAML manifest 读取各 Pass 的 prompt 模板路径和 Schema。

use crate::error::PipelineError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Prompt 清单配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    #[serde(default = "default_locale")]
    pub default_locale: String,
    pub passes: Vec<PassConfig>,
}

fn default_locale() -> String {
    "en".to_string()
}

/// 单个 Pass 的配置条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassConfig {
    pub pass: String,
    pub name: String,
    pub file: String,
    pub description: HashMap<String, String>,
    #[serde(default)]
    pub input_schema: HashMap<String, String>,
    #[serde(default)]
    pub output_schema: HashMap<String, String>,
}

/// 管线运行时配置。
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Prompt 模板目录的根路径。
    pub prompts_dir: PathBuf,
    /// 当前语言（zh / en）。
    pub locale: String,
    /// 清单文件内容。
    pub manifest: Manifest,
    /// 每个 Pass 的 prompt 模板（pass_name → locale → content）。
    pub templates: HashMap<String, HashMap<String, PromptFile>>,
}

/// 单个 prompt 模板文件的解析内容。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFile {
    pub version: u32,
    pub pass: String,
    pub name: String,
    pub description: HashMap<String, String>,
    pub system: HashMap<String, String>,
    pub user_template: HashMap<String, String>,
}

impl PipelineConfig {
    /// 从 prompts 目录加载完整配置。
    pub fn load(prompts_dir: &Path, locale: &str) -> Result<Self, PipelineError> {
        let manifest_path = prompts_dir.join("manifest.yaml");
        let manifest_bytes = std::fs::read(&manifest_path)
            .map_err(|e| PipelineError::Config(format!("无法读取 manifest: {e}")))?;
        let manifest: Manifest = serde_yaml::from_slice(&manifest_bytes)?;

        let mut templates: HashMap<String, HashMap<String, PromptFile>> = HashMap::new();

        for pass_config in &manifest.passes {
            let file_path = prompts_dir.join(&pass_config.file);
            let bytes = std::fs::read(&file_path)
                .map_err(|e| PipelineError::Config(format!("无法读取 {}: {e}", pass_config.file)))?;
            let prompt: PromptFile = serde_yaml::from_slice(&bytes)?;
            templates.insert(pass_config.name.clone(), {
                let mut m = HashMap::new();
                m.insert("zh".to_string(), prompt.clone());
                m.insert("en".to_string(), prompt);
                m
            });
        }

        Ok(PipelineConfig {
            prompts_dir: prompts_dir.to_path_buf(),
            locale: locale.to_string(),
            manifest,
            templates,
        })
    }

    /// 获取指定 Pass 和语言的 system prompt。
    pub fn system_prompt(&self, pass_name: &str, locale: &str) -> Option<&str> {
        let locale = self.resolve_locale(locale);
        self.templates
            .get(pass_name)?
            .get(locale)?
            .system
            .get(locale)
            .map(|s| s.as_str())
    }

    /// 获取指定 Pass 和语言的 user template。
    pub fn user_template(&self, pass_name: &str, locale: &str) -> Option<&str> {
        let locale = self.resolve_locale(locale);
        self.templates
            .get(pass_name)?
            .get(locale)?
            .user_template
            .get(locale)
            .map(|s| s.as_str())
    }

    /// 渲染 user template，将模板变量替换为实际值。
    pub fn render_user_template(
        &self,
        pass_name: &str,
        locale: &str,
        vars: &HashMap<String, String>,
    ) -> Result<String, PipelineError> {
        let template = self
            .user_template(pass_name, locale)
            .ok_or_else(|| PipelineError::Template(format!(
                "未找到 Pass '{pass_name}'、语言 '{locale}' 的 user template"
            )))?;
        let mut result = template.to_string();
        for (key, value) in vars {
            let placeholder = format!("{{{key}}}");
            result = result.replace(&placeholder, value);
        }
        Ok(result)
    }

    /// 解析语言：若请求的语言不可用，回退到 default_locale → en → 第一个可用。
    fn resolve_locale(&self, requested: &str) -> &str {
        // 简单实现：zh 或 en
        if requested == "zh" { "zh" } else { "en" }
    }
}
