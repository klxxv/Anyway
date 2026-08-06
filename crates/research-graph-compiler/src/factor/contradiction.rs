//! 结构矛盾与最小见证 / Structural contradictions & minimal witnesses (spec GC-11)。
//! 输出可复算的图结构矛盾：直接 contradicts、正负双路径、奇数负边环、
//! 自相矛盾 claim。与形式化证明不可满足明确区分。当前为骨架。

use serde::Serialize;

/// 矛盾见证（骨架占位）：见证由节点/边路径组成。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionWitness {
    /// 见证类型：direct-edge / path-pair / signed-cycle / self-contradiction。
    pub kind: &'static str,
    /// 见证涉及的边路径（按稳定排序，可复算）。
    pub paths: Vec<Vec<String>>,
}

/// 矛盾检查结果：见证列表 + 是否截断（预算）。
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContradictionReport {
    /// 全部见证（按稳定排序，最短在前）。
    pub witnesses: Vec<ContradictionWitness>,
    /// 是否因 maxDepth 预算截断（不得声称无矛盾）。
    pub truncated: bool,
}

/// 计算结构矛盾见证（骨架占位：暂返回空报告）。
/// 预算随 GC-11 实现接入；CLI 以 DEFAULT_MAX_DEPTH 调用本函数。
pub fn find_contradictions(_project: &serde_json::Value, _max_depth: usize) -> ContradictionReport {
    ContradictionReport::default()
}
