//! 导出 / Export: digest、Mermaid 与机器可读报告 (spec GC-13)。
//! 导出必须稳定、可转义、可预算截断；换行固定 LF，浮点固定小数位。
//! 当前为骨架。

use serde_json::Value;

/// 导出选项：换行、浮点小数位、预算截断。
#[derive(Clone, Copy, Debug)]
pub struct ExportOptions {
    /// 浮点坐标小数位。
    pub float_digits: usize,
    /// digest 最大字节数（超出截断并带摘要）。
    pub max_bytes: usize,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            float_digits: 2,
            max_bytes: 1_048_576,
        }
    }
}

/// 项目 digest（骨架占位）：规范输入的稳定摘要。
#[derive(Clone, Debug, Default)]
pub struct Digest {
    /// 摘要字节（截断规则 GC13-10 随实现接入）。
    pub bytes: Vec<u8>,
}

/// 生成项目 digest（骨架占位：暂取规范 JSON 字节前 64 字节）。
pub fn project_digest(project: &Value, _options: &ExportOptions) -> Digest {
    let bytes = crate::canonical::canonicalize(project);
    Digest {
        bytes: bytes.iter().take(64).copied().collect(),
    }
}

/// 导出 Mermaid（骨架占位：返回空字符串，转义规则 GC13-09 随实现接入）。
pub fn export_mermaid(_project: &Value) -> String {
    String::new()
}
