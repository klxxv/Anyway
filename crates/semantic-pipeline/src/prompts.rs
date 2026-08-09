//! Prompt 模板类型——从 YAML 文件加载的模板结构。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 从 YAML 文件加载的 prompt 模板。
///
/// 使用 `BTreeMap` 保证语言 key 迭代顺序确定，回退到 "第一个可用" 时
/// 不再依赖 `HashMap` 的不稳定顺序。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptTemplate {
    pub version: u32,
    pub pass: String,
    pub name: String,
    pub description: BTreeMap<String, String>,
    pub system: BTreeMap<String, String>,
    pub user_template: BTreeMap<String, String>,
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

    fn resolve<'a>(&'a self, locale: &str, map: &'a BTreeMap<String, String>) -> Option<&'a str> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_to_first_available_is_deterministic() {
        let mut template = PromptTemplate {
            version: 1,
            pass: "X".into(),
            name: "test".into(),
            description: BTreeMap::new(),
            system: BTreeMap::new(),
            user_template: BTreeMap::new(),
        };
        template.system.insert("fr".into(), "sys-fr".into());
        template.system.insert("de".into(), "sys-de".into());
        // 请求不存在的 en，应稳定回退到字典序第一个可用 de。
        assert_eq!(template.system("en"), Some("sys-de"));
    }
}
