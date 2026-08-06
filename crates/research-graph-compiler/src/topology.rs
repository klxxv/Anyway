//! 拓扑结构 / SCC、拓扑排序与路径 (spec GC-07)。
//! 把循环依赖压缩为凝聚 DAG，提供稳定拓扑序、最短路径与
//! 预算内的多路径枚举。当前为骨架。

use std::collections::BTreeMap;

/// 拓扑序结果：层序节点列表（骨架占位）。
pub type TopoOrder = Vec<String>;

/// SCC 结果：每个强连通分量的成员（按规范 ID 排序）。
pub type SccResult = Vec<Vec<String>>;

/// 最短路径（骨架占位）：nodeId 序列。
pub type Path = Vec<String>;

/// 计算强连通分量（骨架占位：Tarjan/Kosaraju 随索引接入后实现）。
pub fn strongly_connected_components(_nodes: &[String]) -> SccResult {
    let mut result = SccResult::new();
    let mut nodes: Vec<String> = _nodes.to_vec();
    nodes.sort();
    if !nodes.is_empty() {
        result.push(nodes);
    }
    result
}

/// 拓扑排序（骨架占位：Kahn + 字典序 tie-break，双实现逐位一致）。
pub fn topological_sort(_nodes: &[String]) -> TopoOrder {
    let mut order = _nodes.to_vec();
    order.sort();
    order
}

/// 最短路径（骨架占位：Dijkstra/BFS 随索引接入后实现）。
pub fn shortest_path(_from: &str, _to: &str) -> Option<Path> {
    let _ = (BTreeMap::<String, usize>::new(), _from, _to);
    None
}
