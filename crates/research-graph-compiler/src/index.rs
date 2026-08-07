//! 图索引 / Graph indexes (spec GC-06)。
//! 构建正反邻接、证据、哈希与引用索引，提供稳定遍历基座；
//! 禁用边 overlay 不污染 base 索引。当前为骨架。

use crate::model::GraphIndexes;
use serde_json::Value;

/// 由规范化项目构建全套索引（骨架占位：暂返回空索引）。
pub fn build_indexes(_project: &Value) -> GraphIndexes {
    GraphIndexes::default()
}

/// 构建索引的选项：是否纳入证据/哈希反向索引。
#[derive(Clone, Copy, Debug, Default)]
pub struct IndexOptions {
    /// 是否构建证据反向索引（evidenceId → 引用实体）。
    pub with_evidence_index: bool,
}
