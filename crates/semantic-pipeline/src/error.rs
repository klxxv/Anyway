//! 管线错误类型。

use std::fmt;

/// 语义管线错误。
#[derive(Debug)]
pub enum PipelineError {
    /// 配置文件加载失败。
    Config(String),
    /// 输入数据缺失或格式不正确。
    Input(String),
    /// 某个 Pass 执行失败。
    PhaseFailed {
        pass: &'static str,
        reason: String,
        retryable: bool,
    },
    /// JSON 解析失败（LLM 输出非 JSON）。
    JsonParse {
        pass: &'static str,
        raw_output: String,
        error: String,
    },
    /// Prompt 模板渲染失败。
    Template(String),
    /// Pass F 本地验证失败。
    Validation(String),
    /// IO 错误。
    Io(std::io::Error),
}

impl fmt::Display for PipelineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineError::Config(msg) => write!(f, "配置错误: {msg}"),
            PipelineError::Input(msg) => write!(f, "输入错误: {msg}"),
            PipelineError::PhaseFailed { pass, reason, retryable } => {
                write!(f, "Pass {pass} 失败 (可重试={retryable}): {reason}")
            }
            PipelineError::JsonParse { pass, error, .. } => {
                write!(f, "Pass {pass} JSON 解析失败: {error}")
            }
            PipelineError::Template(msg) => write!(f, "模板错误: {msg}"),
            PipelineError::Validation(msg) => write!(f, "验证失败: {msg}"),
            PipelineError::Io(e) => write!(f, "IO 错误: {e}"),
        }
    }
}

impl std::error::Error for PipelineError {}

impl From<std::io::Error> for PipelineError {
    fn from(e: std::io::Error) -> Self {
        PipelineError::Io(e)
    }
}

impl From<serde_json::Error> for PipelineError {
    fn from(e: serde_json::Error) -> Self {
        PipelineError::JsonParse {
            pass: "?",
            raw_output: String::new(),
            error: e.to_string(),
        }
    }
}

impl From<serde_yaml::Error> for PipelineError {
    fn from(e: serde_yaml::Error) -> Self {
        PipelineError::Config(e.to_string())
    }
}
