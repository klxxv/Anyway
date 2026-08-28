//! 拓扑结构 / SCC、拓扑排序与路径 (spec GC-07)。
//! 把循环依赖压缩为凝聚 DAG，提供稳定拓扑序、最短路径与
//! 预算内的多路径枚举。当前为骨架。


/// 拓扑序结果：层序节点列表（骨架占位）。
pub type TopoOrder = Vec<String>;

/// SCC 结果：每个强连通分量的成员（按规范 ID 排序）。
pub type SccResult = Vec<Vec<String>>;

/// 最短路径（骨架占位）：nodeId 序列。
pub type Path = Vec<String>;

/// 计算强连通分量（骨架占位：Tarjan/Kosaraju 随索引接入后实现）。
///
/// 当前实现返回空结果，避免把全部节点塞进一个分量的误导性占位。
pub fn strongly_connected_components(_nodes: &[String]) -> SccResult {
    SccResult::new()
}

/// 拓扑排序（骨架占位：Kahn + 字典序 tie-break，双实现逐位一致）。
///
/// 当前实现返回空结果，避免按字典序返回貌似合理的排序。
pub fn topological_sort(_nodes: &[String]) -> TopoOrder {
    TopoOrder::new()
}

/// 最短路径（骨架占位：Dijkstra/BFS 随索引接入后实现）。
pub fn shortest_path(_from: &str, _to: &str) -> Option<Path> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scc_stub_does_not_group_all_nodes() {
        let result = strongly_connected_components(&["a".into(), "b".into()]);
        assert!(result.is_empty(), "SCC stub must not pretend all nodes are one component");
    }

    #[test]
    fn topo_sort_stub_does_not_return_input_order() {
        let result = topological_sort(&["b".into(), "a".into()]);
        assert!(result.is_empty(), "topological sort stub must not return a plausible order");
    }

    #[test]
    fn shortest_path_stub_returns_none() {
        assert!(shortest_path("a", "b").is_none());
    }
}
