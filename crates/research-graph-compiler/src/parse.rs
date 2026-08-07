//! 解析与 schema 迁移 / Parsing, schema & migration (spec GC-01)。
//! 原始字节 → v3 内存模型：空输入/BOM/非法 UTF-8 稳定报错、版本迁移
//! （v2 → v3 旧 ID 重写与边同步）、资源上限与深度限制。当前为骨架，
//! 生产路径经 `crate::compile::compile_project` 直接走 serde_json 解析。

/// 解析错误上下文（骨架占位）：错误码与字节偏移（GC01-01/02/03）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// 机器可读错误码，如 "empty-input"、"invalid-utf8"。
    pub code: &'static str,
    /// 首个出错字节偏移（如适用）。
    pub offset: Option<usize>,
    /// 人类可读细节（不参与稳定性契约）。
    pub detail: String,
}

impl ParseError {
    /// 构造解析错误 / Build a parse error.
    pub fn new(code: &'static str, offset: Option<usize>, detail: impl Into<String>) -> Self {
        Self {
            code,
            offset,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.offset {
            Some(offset) => write!(
                formatter,
                "{} at offset {offset}: {}",
                self.code, self.detail
            ),
            None => write!(formatter, "{}: {}", self.code, self.detail),
        }
    }
}

impl std::error::Error for ParseError {}
