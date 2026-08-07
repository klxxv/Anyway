//! Prompt 模板类型——从 YAML 文件加载的模板结构。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 从 YAML 文件加载的 prompt 模板。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub version: u32,
    pub pass: String,
    pub name: String,
    pub description: HashMap<String, String>,
    pub system: HashMap<String, String>,
    pub user_template: HashMap<String, String>,
}

impl PromptTemplate {
    /// 获取指定语言的 system prompt，带缺省回退。
    pub fn system(&self, locale: &str) -> Option<&str> {
        self.resolve(locale, &self.system)
    }

    /// 获取指定语言的 user template，带缺省回退。
    pub fn user_template(&self, locale: &str) -> Option<&str> {
        self.resolve(locale, &self.user_template)
    }

    /// 获取指定语言的描述，带缺省回退。
    pub fn description(&self, locale: &str) -> Option<&str> {
        self.resolve(locale, &self.description)
    }

    fn resolve<'a>(&'a self, locale: &str, map: &'a HashMap<String, String>) -> Option<&'a str> {
        // 1. 精确匹配
        if let Some(v) = map.get(locale) {
            return Some(v.as_str());
        }
        // 2. 回退到 en
        if locale != "en" {
            if let Some(v) = map.get("en") {
                return Some(v.as_str());
            }
        }
        // 3. 取第一个可用
        map.values().next().map(|s| s.as_str())
    }
}
