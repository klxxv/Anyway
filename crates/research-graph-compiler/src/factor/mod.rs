//! 逻辑计算层：语义图 → 因子图 / Factor graph compilation (spec §4, GC-08…GC-11)。
//!
//! 用户语义层（有向、带类型、带属性的研究图）经确定性编译为因子图
//! \\(\mathcal F=(X,F,E_F)\\)：`X` 是布尔/有限枚举主张变量，`F` 是
//! supports / contradicts / implies / and / or / depends_on /
//! statistical_test / meta_evidence 等因子。后续由 bp 做双通道信念传播，
//! contradiction 输出结构矛盾见证。

pub mod bp;
pub mod compile;
pub mod contradiction;
pub mod statistics;

/// 因子种类 / Factor kinds (spec §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FactorKind {
    Supports,
    Contradicts,
    Implies,
    Equivalent,
    And,
    Or,
    DependsOn,
    StatisticalTest,
    MetaEvidence,
    Interaction,
}

/// 单个因子：变量域 + 真值表/参数（骨架占位）。
#[derive(Clone, Debug)]
pub struct Factor {
    /// 因子种类。
    pub kind: FactorKind,
    /// 参与变量（输入 + 输出）。
    pub variables: Vec<String>,
}

/// 因子图（骨架占位）：变量集合 + 因子列表。
#[derive(Clone, Debug, Default)]
pub struct FactorGraph {
    /// 变量名集合（布尔/有限枚举）。
    pub variables: Vec<String>,
    /// 因子列表。
    pub factors: Vec<Factor>,
}
