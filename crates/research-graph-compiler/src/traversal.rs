//! 遍历 / Traversal: BFS/DFS 与可达性 (spec GC-06)。
//! 稳定节点序（规范 ID 作最终 tie-break）、maxDepth 边界、
//! 环安全（back edge 标记并终止）、多源去重保最小距离。当前为骨架。

use std::collections::{BTreeMap, VecDeque};

/// 一次 BFS 的结果：nodeId → (距离, 来源集合)。
/// 距离按起点层序；来源集合与边序稳定。
pub type BfsResult = BTreeMap<String, (usize, Vec<String>)>;

/// BFS 选项：方向、类型过滤、最大深度、起点保留策略。
#[derive(Clone, Copy, Debug)]
pub struct BfsOptions {
    /// 遍历方向：forward（后继）或 reverse（祖先）。
    pub reverse: bool,
    /// 最大深度（0 = 只含起点）。
    pub max_depth: usize,
}

impl Default for BfsOptions {
    fn default() -> Self {
        Self {
            reverse: false,
            max_depth: usize::MAX,
        }
    }
}

/// 广度优先遍历（骨架占位：仅按输入节点初始化结果，边遍历随索引接入）。
pub fn bfs(_nodes: &[String], _options: &BfsOptions) -> BfsResult {
    let mut result = BfsResult::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    for node in _nodes {
        queue.push_back(node.clone());
    }
    while let Some(node) = queue.pop_front() {
        result.entry(node).or_insert_with(|| (0, Vec::new()));
    }
    result
}
