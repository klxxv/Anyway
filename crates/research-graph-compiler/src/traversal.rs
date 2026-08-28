//! 遍历 / Traversal: BFS/DFS 与可达性 (spec GC-06)。
//! 稳定节点序（规范 ID 作最终 tie-break）、maxDepth 边界、
//! 环安全（back edge 标记并终止）、多源去重保最小距离。当前为骨架。

use std::collections::BTreeMap;

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
///
/// 当前实现返回空结果，避免把输入节点标记为距离 0 的误导性占位。
pub fn bfs(_nodes: &[String], _options: &BfsOptions) -> BfsResult {
    BfsResult::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bfs_stub_does_not_mark_inputs_as_reached() {
        let result = bfs(&["a".into(), "b".into()], &BfsOptions::default());
        assert!(result.is_empty(), "BFS stub must not return input nodes as reachable");
    }
}
