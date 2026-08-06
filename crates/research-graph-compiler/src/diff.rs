//! 版本 Diff / Graph diff (spec GC-12/14, docs/architecture/canvas-diff-design.md)。
//! 确定性结构比较：规范化后比较，区分语义区变化（contentRoot）与
//! 编辑/布局变化（fileHash），节点内容变化归约为 modifiedNode + idRemap。
//! 当前为骨架，设计契约见 canvas-diff-design.md §2.1。

use serde_json::Value;

/// 单条 diff 条目（骨架占位）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffEntry {
    /// 操作：added / removed / modified / idRemap。
    pub op: &'static str,
    /// 实体集合：nodes / edges / evidence / placements / scenarios。
    pub collection: &'static str,
    /// 实体 id。
    pub id: String,
}

/// 图 diff 结果：按层次分开的条目列表。
#[derive(Clone, Debug, Default)]
pub struct GraphDiff {
    /// 语义区（contentRoot）变化条目。
    pub semantic: Vec<DiffEntry>,
    /// 编辑/布局区（fileHash 级）变化条目。
    pub editorial: Vec<DiffEntry>,
}

/// 规范化后比较两个项目（骨架占位：暂返回空 diff）。
pub fn diff_projects(_left: &Value, _right: &Value) -> GraphDiff {
    GraphDiff::default()
}
