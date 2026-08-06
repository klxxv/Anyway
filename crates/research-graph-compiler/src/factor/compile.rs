//! 语义图 → 因子图编译 / Semantic graph → factor graph (spec GC-08)。
//! 类型边与复合逻辑节点编译为明确因子及变量域：
//! supports → 二元因子；contradicts → 翻转极性；implies → 只惩罚 A 真 B 假；
//! AND/OR → 真值表因子；depends_on → 门控因子。当前为骨架。

use super::FactorGraph;
use serde_json::Value;

/// 由规范化项目编译因子图（骨架占位：暂返回空因子图）。
pub fn compile_factor_graph(_project: &Value) -> FactorGraph {
    FactorGraph::default()
}

/// 变量：布尔或有限枚举（骨架占位）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariableSpec {
    /// 变量名。
    pub name: String,
    /// 有限枚举取值（按规范顺序；布尔变量即 ["true","false"]）。
    pub domain: Vec<String>,
}
