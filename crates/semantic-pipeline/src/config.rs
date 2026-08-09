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
    ///
    /// 单遍从左到右扫描:替换值写进输出后不再回扫——HashMap 迭代顺序不再
    /// 影响结果(确定性),值里夹带的 `{...}` 也不会被二次替换(注入通道关闭)。
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
        let mut out = String::with_capacity(template.len());
        let bytes = template.as_bytes();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != b'{' {
                // 普通文本(多字节字符按原切片拷贝,不逐字节拆)。
                let next = template[index..]
                    .find('{')
                    .map(|offset| index + offset)
                    .unwrap_or(bytes.len());
                out.push_str(&template[index..next]);
                index = next;
                continue;
            }
            // 候选占位符:{ + 标识符字符([A-Za-z0-9_]) + }。
            let mut end = index + 1;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
            {
                end += 1;
            }
            if end > index + 1 && end < bytes.len() && bytes[end] == b'}' {
                let key = &template[index + 1..end];
                match vars.get(key) {
                    // 值原样进入输出,永不回扫 → 注入不可能。
                    Some(value) => out.push_str(value),
                    // 未知占位符保持字面量(模板里的 JSON 示例等不受影响)。
                    None => out.push_str(&template[index..=end]),
                }
                index = end + 1;
            } else {
                // 不是占位符(JSON 骨架的 '{' 等):原样输出,继续扫描。
                out.push('{');
                index += 1;
            }
        }
        Ok(out)
    }

    /// 解析语言：若请求的语言不可用，回退到 default_locale → en → 第一个可用。
    fn resolve_locale(&self, requested: &str) -> &str {
        // 简单实现：zh 或 en
        if requested == "zh" { "zh" } else { "en" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(template: &str) -> PipelineConfig {
        let mut system = HashMap::new();
        system.insert("en".to_string(), "sys".to_string());
        let mut user_template = HashMap::new();
        user_template.insert("en".to_string(), template.to_string());
        let prompt = PromptFile {
            version: 1,
            pass: "X".to_string(),
            name: "test".to_string(),
            description: HashMap::new(),
            system,
            user_template,
        };
        let mut per_pass = HashMap::new();
        per_pass.insert("en".to_string(), prompt);
        let mut templates = HashMap::new();
        templates.insert("test".to_string(), per_pass);
        PipelineConfig {
            prompts_dir: PathBuf::new(),
            locale: "en".to_string(),
            manifest: Manifest {
                version: 1,
                default_locale: "en".to_string(),
                passes: vec![],
            },
            templates,
        }
    }

    #[test]
    fn render_is_deterministic_and_injection_safe() {
        let config = test_config("Claim: {claim}. Note: {note}.");
        let mut vars = HashMap::new();
        // 值里夹带占位符:单遍渲染不得二次替换(注入载荷原样输出)。
        vars.insert("claim".to_string(), "A {note} B".to_string());
        vars.insert("note".to_string(), "safe".to_string());
        let rendered = config
            .render_user_template("test", "en", &vars)
            .expect("render");
        assert_eq!(rendered, "Claim: A {note} B. Note: safe.");
    }

    #[test]
    fn render_keeps_unknown_placeholders_literal() {
        let config = test_config("{\"key\": {value}} and {unknown}");
        let mut vars = HashMap::new();
        vars.insert("value".to_string(), "42".to_string());
        let rendered = config
            .render_user_template("test", "en", &vars)
            .expect("render");
        assert_eq!(rendered, "{\"key\": 42} and {unknown}");
    }
}
