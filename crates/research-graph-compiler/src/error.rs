//! 稳定错误模型 / Stable error model (spec §3.1)。
//! 错误码、排序与结构 MUST 稳定：本模块是后续 parse/patch 各阶段
//! 致命错误与警告的单一出口。当前仅提供骨架枚举，随 GC-01…GC-15
//! 实现逐步扩展为带 json_pointer / args / suggested_fixes 的结构化诊断。

use std::fmt;

/// 编译失败（致命错误）/ Fatal compile failure.
///
/// 现阶段覆盖字节解析错误；后续任务按 spec §3.1 扩展
/// （`Diagnostic` 携带 `code`、`severity`、`entity_ref`、`json_pointer`、
/// `message_key`、`args`、`suggested_fixes`，文本可本地化，排序稳定）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileFailure {
    /// 字节流不是合法 JSON（GC01-01/02/03：空输入、BOM、非法 UTF-8 一并归入）。
    Parse(String),
    /// 尚未实现的阶段（占位）。
    NotImplemented(&'static str),
}

impl fmt::Display for CompileFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileFailure::Parse(detail) => write!(formatter, "parse error: {detail}"),
            CompileFailure::NotImplemented(stage) => {
                write!(formatter, "stage {stage:?} is not implemented yet")
            }
        }
    }
}

impl std::error::Error for CompileFailure {}
