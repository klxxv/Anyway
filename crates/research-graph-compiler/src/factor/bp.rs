//! 双通道信念传播 / Dual-channel belief propagation (spec §4, GC-10)。
//! support/refutation 双通道：树图精确；环图固定顺序 + 阻尼 Loopy BP，
//! 输出净信念与冲突。当前为骨架。

use serde::Serialize;

/// 信念状态 / Belief state (spec §4).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BeliefState {
    /// 支持通道 logit。
    pub support_logit: f64,
    /// 反驳通道 logit。
    pub refutation_logit: f64,
    /// 支持概率（0..1）。
    pub support: f64,
    /// 反驳概率（0..1）。
    pub refutation: f64,
    /// 净信念 support − refutation。
    pub net_belief: f64,
    /// 冲突 1 − |net_belief|（0..1）。
    pub conflict: f64,
}

impl BeliefState {
    /// 从双通道概率构造信念状态。
    pub fn from_probabilities(support: f64, refutation: f64) -> Self {
        let net_belief = support - refutation;
        Self {
            support_logit: (support / (1.0 - support).max(f64::EPSILON)).ln(),
            refutation_logit: (refutation / (1.0 - refutation).max(f64::EPSILON)).ln(),
            support,
            refutation,
            net_belief,
            conflict: 1.0 - net_belief.abs(),
        }
    }
}

/// 树形 BP（骨架占位：单变量无因子先验 0 时 support=refutation=0.5）。
pub fn tree_belief_propagation(_factor_graph: &super::FactorGraph) -> Vec<BeliefState> {
    vec![BeliefState::from_probabilities(0.5, 0.5)]
}
