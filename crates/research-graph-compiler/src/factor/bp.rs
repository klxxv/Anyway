//! 双通道信念传播 / Dual-channel belief propagation (spec §4, GC-10)。
//!
//! support/refutation 双通道：树图两遍消息传递精确；环图固定顺序 +
//! 阻尼 Loopy BP。消息在 logit 域累加，σ 输出稳定概率；极大 LLR 由
//! logit clamp 限幅（GC10-12）。η=0 边贡献恒 0，与删除边逐位一致
//! （GC10-10）；遍历顺序固定（变量按名、因子按编译序），插入顺序无关
//! （GC10-11）。
//!
//! 信念定义（与 GC10-01 对齐）：
//! - support = σ(support_logit)，refutation = σ(refutation_logit)
//! - net_belief = σ(support_logit − refutation_logit)（0.5 = 中性）
//! - conflict = support × refutation（两通道同时激活程度）

use super::{Factor, FactorDiagnostic, FactorGraph, FactorKind};
use crate::invariant::Severity;
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
    /// 净信念 σ(support_logit − refutation_logit)（0..1，0.5 = 中性）。
    pub net_belief: f64,
    /// 冲突 support × refutation（0..1）。
    pub conflict: f64,
}

/// 数值边界 / Numerical bounds (GC10-12 溢出防护)。
const LOGIT_CLAMP: f64 = 50.0;
const PROB_EPSILON: f64 = 1e-15;

/// 稳定 sigmoid（输入先限幅到 ±LOGIT_CLAMP；+inf→1，−inf→0，NaN→0.5 中性）。
pub fn sigmoid(x: f64) -> f64 {
    if x.is_nan() {
        return 0.5;
    }
    if x.is_infinite() {
        return if x > 0.0 { 1.0 } else { 0.0 };
    }
    1.0 / (1.0 + (-x.clamp(-LOGIT_CLAMP, LOGIT_CLAMP)).exp())
}

/// logit（概率先夹紧到 (0,1) 开区间防除零；非有限输入 → 0 中性）。
pub fn logit(p: f64) -> f64 {
    if !p.is_finite() {
        return 0.0;
    }
    let p = p.clamp(PROB_EPSILON, 1.0 - PROB_EPSILON);
    (p / (1.0 - p)).ln()
}

impl BeliefState {
    /// 无信息状态（先验 logit 0）：support=refutation=0.5，net=0.5，conflict=0.25。
    pub fn uninformative() -> Self {
        Self {
            support_logit: 0.0,
            refutation_logit: 0.0,
            support: 0.5,
            refutation: 0.5,
            net_belief: 0.5,
            conflict: 0.25,
        }
    }

    /// 从双通道概率构造信念状态（net/conflict 按本模块定义）。
    pub fn from_probabilities(support: f64, refutation: f64) -> Self {
        let support_logit = logit(support);
        let refutation_logit = logit(refutation);
        Self {
            support_logit,
            refutation_logit,
            support,
            refutation,
            net_belief: sigmoid(support_logit - refutation_logit),
            conflict: support * refutation,
        }
    }
}

/// BP 选项 / BP options (spec §1.3: 固定算法、规定舍入)。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BpOptions {
    /// 阻尼系数 α（loopy：new = α·candidate + (1−α)·old）。
    pub damping: f64,
    /// 最大迭代轮数（loopy）。
    pub max_iterations: usize,
    /// 残差收敛阈值。
    pub tolerance: f64,
    /// logit 限幅（溢出防护，GC10-12）。
    pub logit_clamp: f64,
}

impl Default for BpOptions {
    fn default() -> Self {
        Self {
            damping: 0.5,
            max_iterations: 100,
            tolerance: 1e-9,
            logit_clamp: LOGIT_CLAMP,
        }
    }
}

/// BP 终止状态 / BP termination status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BpStatus {
    /// 收敛（树 BP 恒收敛）。
    Converged,
    /// 达到最大迭代仍未收敛。
    MaxIterationsReached,
    /// 检测到振荡（GC10-09，结果带残差）。
    Unstable,
    /// 树 BP 收到环图（应改走 loopy，GC08-12）。
    TreeOnCyclicGraph,
    /// 因子图包含零变量或未知种类因子，无法运行 BP。
    InvalidFactorGraph,
}

/// BP 结果 / BP result.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BpResult {
    /// 每个变量的信念（按因子图变量排序）。
    pub beliefs: Vec<BeliefState>,
    /// 是否收敛。
    pub converged: bool,
    /// 实际迭代轮数。
    pub iterations: usize,
    /// 最终残差（max |Δmessage|）。
    pub residual: f64,
    pub status: BpStatus,
    /// 运行期诊断（如零变量因子）。
    pub diagnostics: Vec<FactorDiagnostic>,
}

/// 因子消息权重：证据因子取 w=η·λ；逻辑因子（implies/and/or/等价）取 1；
/// depends_on 取门控效力 η；未接地的证据因子取 0（无消息）。
fn factor_weight(factor: &Factor) -> f64 {
    match factor.effective_message {
        Some(w) => w,
        None => match factor.kind {
            FactorKind::Implies | FactorKind::Equivalent | FactorKind::And | FactorKind::Or => 1.0,
            FactorKind::DependsOn => factor.efficacy,
            _ => 0.0,
        },
    }
}

/// 因子对每个邻居变量的消息（logit 域，sup/ref 双通道非负）。
/// `nets[i]` = 变量 i 的净真度 σ(M_sup − M_ref)（0..1）。
/// 规则（GC-08/GC-10）：
/// - Supports：源真 ⇒ 目标正证据（w>0 → 支持通道，w<0 → 反驳通道）；反向不发。
/// - Contradicts：双向翻转极性（源真 ⇒ 目标反驳）。
/// - Implies：A 真 ⇒ B 真；B 假 ⇒ A 假（逆否合法）；B 真不推 A 真（GC10-06）。
/// - And：输出差值 = w·min(2·net−1)（任一输入假 ⇒ 输出受限，GC10-07）。
/// - Or：输出差值 = w·max(2·net−1)（任一输入真 ⇒ 输出支持）。
fn factor_messages(factor: &Factor, nets: &[f64]) -> Vec<(f64, f64)> {
    let weight = factor_weight(factor);
    let n = factor.variables.len();
    let mut messages = vec![(0.0, 0.0); n];
    // 防御性 guard：零变量因子或不足二元因子不触发索引 panic。
    if n == 0 {
        return messages;
    }
    let needs_two = matches!(
        factor.kind,
        FactorKind::Supports
            | FactorKind::StatisticalTest
            | FactorKind::MetaEvidence
            | FactorKind::Contradicts
            | FactorKind::Implies
            | FactorKind::Equivalent
            | FactorKind::DependsOn
    );
    if needs_two && n < 2 {
        return messages;
    }
    // 差值 m → 双通道拆分（m≥0 进支持，m<0 进反驳）。
    let split = |m: f64| if m >= 0.0 { (m, 0.0) } else { (0.0, -m) };
    match factor.kind {
        FactorKind::Supports | FactorKind::StatisticalTest | FactorKind::MetaEvidence => {
            // variables = [source, target]；只向 target 发消息。
            messages[1] = split(weight * nets[0]);
        }
        FactorKind::Contradicts => {
            messages[1] = (0.0, weight.abs() * nets[0]);
            messages[0] = (0.0, weight.abs() * nets[1]);
        }
        FactorKind::Implies => {
            // [source, target]：A 真 ⇒ B 真；B 假 ⇒ A 假。
            messages[1] = (weight * nets[0], 0.0);
            messages[0] = (0.0, weight * (1.0 - nets[1]));
        }
        FactorKind::Equivalent => {
            messages[1] = (weight * nets[0], weight * (1.0 - nets[0]));
            messages[0] = (weight * nets[1], weight * (1.0 - nets[1]));
        }
        FactorKind::And | FactorKind::Interaction => {
            // [x1..xn, y]；输出差值 = w·min_i(2·net_i−1)。不反向指认输入。
            let output = n - 1;
            let min_diff = nets[..output]
                .iter()
                .map(|&net| 2.0 * net - 1.0)
                .fold(1.0, f64::min);
            messages[output] = split(weight * min_diff);
        }
        FactorKind::Or => {
            let output = n - 1;
            let max_diff = nets[..output]
                .iter()
                .map(|&net| 2.0 * net - 1.0)
                .fold(-1.0, f64::max);
            messages[output] = split(weight * max_diff);
        }
        FactorKind::DependsOn => {
            messages[1] = (weight * nets[0], 0.0);
        }
    }
    messages
}

/// 校验因子是否满足 BP 运行最小元数要求。
fn validate_factor_for_bp(index: usize, factor: &Factor) -> Option<FactorDiagnostic> {
    let (required, name) = match factor.kind {
        FactorKind::And | FactorKind::Or | FactorKind::Interaction => (1, "and/or/interaction"),
        FactorKind::Supports
        | FactorKind::StatisticalTest
        | FactorKind::MetaEvidence
        | FactorKind::Contradicts
        | FactorKind::Implies
        | FactorKind::Equivalent
        | FactorKind::DependsOn => (2, "binary"),
    };
    if factor.variables.len() < required {
        let entity = factor
            .source_edge
            .as_deref()
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("factor:{index}"));
        Some(FactorDiagnostic::new(
            "bp-invalid-factor",
            Severity::Error,
            &entity,
            format!(
                "{name} factor has {} variables, needs at least {required}",
                factor.variables.len()
            ),
        ))
    } else {
        None
    }
}

/// 邻接（确定性构建）：变量 → (因子, 因子内位置)；因子 → 变量索引。
struct Adjacency {
    var_to_factor: Vec<Vec<(usize, usize)>>,
    factor_to_var: Vec<Vec<usize>>,
}

impl Adjacency {
    fn build(graph: &FactorGraph) -> Self {
        let mut var_to_factor: Vec<Vec<(usize, usize)>> = vec![Vec::new(); graph.variables.len()];
        let mut factor_to_var: Vec<Vec<usize>> = Vec::with_capacity(graph.factors.len());
        for (f, factor) in graph.factors.iter().enumerate() {
            let mut vars = Vec::with_capacity(factor.variables.len());
            for (pos, name) in factor.variables.iter().enumerate() {
                if let Some(v) = graph.variable_index(name) {
                    var_to_factor[v].push((f, pos));
                    vars.push(v);
                }
            }
            factor_to_var.push(vars);
        }
        // 每行按 (因子索引, 位置) 排序 → 遍历顺序固定（GC10-11）。
        for row in var_to_factor.iter_mut() {
            row.sort_unstable();
        }
        Self {
            var_to_factor,
            factor_to_var,
        }
    }
}

/// 树形 BP：两遍消息传递（collect 叶→根，distribute 根→叶）在树上精确。
/// 非树图返回 `TreeOnCyclicGraph` 状态并给出生成树近似（应改走 loopy）。
pub fn tree_belief_propagation(graph: &FactorGraph) -> BpResult {
    let var_count = graph.variables.len();
    let factor_count = graph.factors.len();
    let adjacency = Adjacency::build(graph);

    let diagnostics: Vec<FactorDiagnostic> = graph
        .factors
        .iter()
        .enumerate()
        .filter_map(|(i, f)| validate_factor_for_bp(i, f))
        .collect();
    if !diagnostics.is_empty() {
        return BpResult {
            beliefs: Vec::new(),
            converged: false,
            iterations: 0,
            residual: 0.0,
            status: BpStatus::InvalidFactorGraph,
            diagnostics,
        };
    }

    // 消息表:msg_vf[u][k] ↔ var_to_factor[u][k];msg_fv[f][pos]。
    // 内层必须按各变量度数分配——hub 变量的度数可以超过变量总数,
    // 按变量数分配会在 2 claims + 4 平行边时越界 panic。
    let mut msg_vf: Vec<Vec<(f64, f64)>> = (0..var_count)
        .map(|u| vec![(0.0, 0.0); adjacency.var_to_factor[u].len()])
        .collect();
    let mut msg_fv: Vec<Vec<(f64, f64)>> = graph
        .factors
        .iter()
        .map(|factor| vec![(0.0, 0.0); factor.variables.len()])
        .collect();

    // ---- BFS 建树（统一节点编号：变量 0..var_count，因子 +var_count）----
    let total = var_count + factor_count;
    let mut parent = vec![usize::MAX; total];
    let mut order: Vec<usize> = Vec::with_capacity(total);
    let mut visited = vec![false; total];
    for root in 0..total {
        if visited[root] {
            continue;
        }
        let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        visited[root] = true;
        queue.push_back(root);
        while let Some(node) = queue.pop_front() {
            order.push(node);
            let neighbors: Vec<usize> = if node < var_count {
                adjacency.var_to_factor[node]
                    .iter()
                    .map(|&(f, _)| f + var_count)
                    .collect()
            } else {
                adjacency.factor_to_var[node - var_count].clone()
            };
            for next in neighbors {
                if !visited[next] {
                    visited[next] = true;
                    parent[next] = node;
                    queue.push_back(next);
                }
            }
        }
    }
    let cyclic = graph.has_cycle();

    // ---- collect（逆序，叶→根）----
    for &node in order.iter().rev() {
        if node < var_count {
            // 变量 u → 父因子 f：M = prior + Σ_{g≠f} m(g→u)
            let u = node;
            if let Some(f) = parent.get(node).copied().filter(|&f| f != usize::MAX) {
                let f = f - var_count;
                let mut sup = graph.variables[u].prior_support_logit;
                let mut rfu = graph.variables[u].prior_refutation_logit;
                for &(g, pos) in &adjacency.var_to_factor[u] {
                    if g != f {
                        sup += msg_fv[g][pos].0;
                        rfu += msg_fv[g][pos].1;
                    }
                }
                if let Some(k) = adjacency.var_to_factor[u].iter().position(|&(g, _)| g == f) {
                    msg_vf[u][k] = (sup, rfu);
                }
            }
        } else {
            // 因子 f → 父变量 v：m(f→v)，输入 nets 用全部子变量。
            let f = node - var_count;
            let factor = &graph.factors[f];
            if let Some(v) = parent.get(node).copied().filter(|&v| v != usize::MAX) {
                let nets: Vec<f64> = adjacency.factor_to_var[f]
                    .iter()
                    .map(|&w| {
                        if w == v {
                            0.0 // 父变量消息未算；factor_messages 不用目标自身 net
                        } else {
                            // msg_vf[w] 按 var_to_factor[w] 的槽位布局,
                            // 必须查变量侧位置(因子侧位置是另一个索引空间)。
                            let k = adjacency.var_to_factor[w]
                                .iter()
                                .position(|&(g, _)| g == f)
                                .unwrap_or(0);
                            let (sup, rfu) = msg_vf[w][k];
                            sigmoid(sup - rfu)
                        }
                    })
                    .collect();
                let messages = factor_messages(factor, &nets);
                if let Some(pos) = adjacency.factor_to_var[f].iter().position(|&w| w == v) {
                    msg_fv[f][pos] = messages[pos];
                }
            }
        }
    }

    // ---- distribute（正序，根→叶）----
    for &node in order.iter() {
        if node < var_count {
            // 变量 u → 每个子因子 g：M = prior + 父消息 + Σ_{h≠f,g} m(h→u)
            let u = node;
            let f = parent[node];
            for &(g, _) in &adjacency.var_to_factor[u] {
                if f != usize::MAX && g + var_count == f {
                    continue; // 父因子跳过
                }
                let mut sup = graph.variables[u].prior_support_logit;
                let mut rfu = graph.variables[u].prior_refutation_logit;
                for &(h, pos) in &adjacency.var_to_factor[u] {
                    if h != g {
                        sup += msg_fv[h][pos].0;
                        rfu += msg_fv[h][pos].1;
                    }
                }
                if let Some(k) = adjacency.var_to_factor[u].iter().position(|&(h, _)| h == g) {
                    msg_vf[u][k] = (sup, rfu);
                }
            }
        } else {
            // 因子 f → 每个子变量 w：m(f→w)，输入 nets 全部可用。
            let f = node - var_count;
            let factor = &graph.factors[f];
            let v = parent[node];
            for (pos, &w) in adjacency.factor_to_var[f].iter().enumerate() {
                if v != usize::MAX && w == v {
                    continue; // 父变量跳过
                }
                let nets: Vec<f64> = adjacency.factor_to_var[f]
                    .iter()
                    .map(|&x| {
                        // 同上:msg_vf 必须按变量侧槽位索引。
                        let k = adjacency.var_to_factor[x]
                            .iter()
                            .position(|&(g, _)| g == f)
                            .unwrap_or(0);
                        let (sup, rfu) = msg_vf[x][k];
                        sigmoid(sup - rfu)
                    })
                    .collect();
                let messages = factor_messages(factor, &nets);
                msg_fv[f][pos] = messages[pos];
            }
        }
    }

    // ---- 信念 ----
    let options = BpOptions::default();
    let beliefs = compute_beliefs(graph, &adjacency, &msg_fv, &options);

    let status = if cyclic {
        BpStatus::TreeOnCyclicGraph
    } else {
        BpStatus::Converged
    };
    BpResult {
        beliefs,
        converged: !cyclic,
        iterations: 2,
        residual: 0.0,
        status,
        diagnostics: Vec::new(),
    }
}

/// 阻尼 Loopy BP：固定顺序 + 阻尼，支持收敛检测与振荡标记（GC10-08/09）。
pub fn loopy_belief_propagation(graph: &FactorGraph, options: &BpOptions) -> BpResult {
    let var_count = graph.variables.len();
    let adjacency = Adjacency::build(graph);

    let diagnostics: Vec<FactorDiagnostic> = graph
        .factors
        .iter()
        .enumerate()
        .filter_map(|(i, f)| validate_factor_for_bp(i, f))
        .collect();
    if !diagnostics.is_empty() {
        return BpResult {
            beliefs: Vec::new(),
            converged: false,
            iterations: 0,
            residual: 0.0,
            status: BpStatus::InvalidFactorGraph,
            diagnostics,
        };
    }

    // 同树形 BP:内层按各变量度数分配,而不是变量总数(见上方注释)。
    let msg_vf: Vec<Vec<(f64, f64)>> = (0..var_count)
        .map(|u| vec![(0.0, 0.0); adjacency.var_to_factor[u].len()])
        .collect();
    let mut msg_fv: Vec<Vec<(f64, f64)>> = graph
        .factors
        .iter()
        .map(|factor| vec![(0.0, 0.0); factor.variables.len()])
        .collect();

    let mut status = BpStatus::MaxIterationsReached;
    let mut residual = 0.0;
    let mut iterations = 0;
    let mut prev_residual = f64::INFINITY;
    let mut rising_streak = 0;
    // period-2 振荡检测(GC10-09):消息与"两轮前"重合但残差未收敛。
    // 需要 prev1/prev2 双快照;单快照在每轮末更新,实际比较的是一轮前,差一拍。
    let mut prev1_msg = msg_fv.clone();
    let mut prev2_msg = msg_fv.clone();

    for round in 0..options.max_iterations {
        iterations = round + 1;
        // 快照：全部 M_{u→g}（基于上一轮 msg_fv）。
        let mut snapshot = msg_vf.clone();
        for (u, variable) in graph.variables.iter().enumerate() {
            for (k, &(g, _)) in adjacency.var_to_factor[u].iter().enumerate() {
                let mut sup = variable.prior_support_logit;
                let mut rfu = variable.prior_refutation_logit;
                for &(h, pos) in &adjacency.var_to_factor[u] {
                    if h != g {
                        sup += msg_fv[h][pos].0;
                        rfu += msg_fv[h][pos].1;
                    }
                }
                snapshot[u][k] = (sup, rfu);
            }
        }

        // 更新因子消息（固定顺序：因子索引 → 因子内位置）。
        let mut max_delta = 0.0f64;
        for (f, factor) in graph.factors.iter().enumerate() {
            let nets: Vec<f64> = adjacency.factor_to_var[f]
                .iter()
                .map(|&w| {
                    // snapshot 与 msg_vf 同布局:按变量侧槽位索引。
                    let k = adjacency.var_to_factor[w]
                        .iter()
                        .position(|&(g, _)| g == f)
                        .unwrap_or(0);
                    let (sup, rfu) = snapshot[w][k];
                    sigmoid(sup - rfu)
                })
                .collect();
            let messages = factor_messages(factor, &nets);
            for (pos, (candidate_sup, candidate_rfu)) in messages.iter().enumerate() {
                let old = msg_fv[f][pos];
                let candidate = (*candidate_sup, *candidate_rfu);
                max_delta = max_delta
                    .max((candidate.0 - old.0).abs())
                    .max((candidate.1 - old.1).abs());
                let alpha = options.damping;
                msg_fv[f][pos] = (
                    alpha * candidate.0 + (1.0 - alpha) * old.0,
                    alpha * candidate.1 + (1.0 - alpha) * old.1,
                );
            }
        }
        residual = max_delta;

        if residual < options.tolerance {
            status = BpStatus::Converged;
            break;
        }
        // 振荡检测：残差连续上升（发散）或 period-2 跳变（GC10-09）。
        if residual > prev_residual {
            rising_streak += 1;
            if rising_streak >= 3 {
                status = BpStatus::Unstable;
                break;
            }
        } else {
            rising_streak = 0;
        }
        prev_residual = residual;
        // period-2：消息与两轮前重合而残差显著高于容差 ⇒ 真振荡（GC10-09）。
        // （残差与容差同量级是浮点收敛平台，不算振荡。）
        if iterations >= 3 && residual > 100.0 * options.tolerance {
            let mut period2 = 0.0f64;
            for (f, messages) in msg_fv.iter().enumerate() {
                for (pos, message) in messages.iter().enumerate() {
                    let old = prev2_msg[f][pos];
                    period2 = period2
                        .max((message.0 - old.0).abs())
                        .max((message.1 - old.1).abs());
                }
            }
            if period2 < options.tolerance && residual > options.tolerance {
                status = BpStatus::Unstable;
                break;
            }
        }
        prev2_msg = std::mem::replace(&mut prev1_msg, msg_fv.clone());
    }

    let beliefs = compute_beliefs(graph, &adjacency, &msg_fv, options);
    BpResult {
        converged: status == BpStatus::Converged,
        iterations,
        residual,
        status,
        beliefs,
        diagnostics: Vec::new(),
    }
}

/// 计算最终信念（logit clamp + σ；GC10-12 有限输出）。
fn compute_beliefs(
    graph: &FactorGraph,
    adjacency: &Adjacency,
    msg_fv: &[Vec<(f64, f64)>],
    options: &BpOptions,
) -> Vec<BeliefState> {
    graph
        .variables
        .iter()
        .enumerate()
        .map(|(u, variable)| {
            let mut support_logit = variable.prior_support_logit;
            let mut refutation_logit = variable.prior_refutation_logit;
            for &(f, pos) in &adjacency.var_to_factor[u] {
                support_logit += msg_fv[f][pos].0;
                refutation_logit += msg_fv[f][pos].1;
            }
            let clamp = options.logit_clamp;
            let support_logit = support_logit.clamp(-clamp, clamp);
            let refutation_logit = refutation_logit.clamp(-clamp, clamp);
            let support = sigmoid(support_logit);
            let refutation = sigmoid(refutation_logit);
            BeliefState {
                support_logit,
                refutation_logit,
                support,
                refutation,
                net_belief: sigmoid(support_logit - refutation_logit),
                conflict: support * refutation,
            }
        })
        .collect()
}

/// 变量 w 在因子 f 的邻接中的消息槽位置。
/// 统一入口：树图走精确树 BP，环图走阻尼 Loopy BP（GC08-12 语义图环 → loopy）。
pub fn belief_propagation(graph: &FactorGraph, options: &BpOptions) -> BpResult {
    if graph.has_cycle() {
        loopy_belief_propagation(graph, options)
    } else {
        tree_belief_propagation(graph)
    }
}

// ---------------------------------------------------------------------------
// 单元测试 / Unit tests (GC-10)
// ---------------------------------------------------------------------------
