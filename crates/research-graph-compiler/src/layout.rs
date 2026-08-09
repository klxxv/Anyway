//! 确定性布局 / Deterministic layout (spec GC-13)。
//! 布局是可重算的编译产物：固定算法、固定遍历顺序、不依赖字体测量；
//! pinned 节点保持坐标，其余节点确定性重排。当前为骨架。

use serde::Serialize;
use serde_json::Value;

/// 布局模式（spec GC-13: 与前端 view layout mode 对齐）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutMode {
    /// 分层布局（SCC 同层，内部确定性排列）。
    Hierarchical,
    /// 力导向（确定性初值 + 固定迭代）。
    ForceDirected,
}

/// 单个节点的布局结果。
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// 布局结果：nodeId → 坐标。
pub type LayoutResult = std::collections::BTreeMap<String, Position>;

/// 计算确定性布局（骨架占位：返回空坐标集合）。
///
/// 空集合是明确的无数据占位，不会给调用方一个看似合理的布局。
pub fn compute_layout(_project: &Value, _mode: LayoutMode) -> LayoutResult {
    LayoutResult::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_stub_returns_empty() {
        let result = compute_layout(&Value::Null, LayoutMode::Hierarchical);
        assert!(result.is_empty(), "layout stub must return an empty result");
    }
}
