//! 逻辑计算层：语义图 → 因子图 / Factor graph compilation (spec §4, GC-08…GC-11)。
//!
//! 用户语义层（有向、带类型、带属性的研究图）经确定性编译为因子图
//! \\(\mathcal F=(X,F,E_F)\\)：`X` 是布尔/有限枚举主张变量，`F` 是
//! supports / contradicts / implies / and / or / depends_on /
//! statistical_test / meta_evidence 等因子。后续由 bp 做双通道信念传播，
//! contradiction 输出结构矛盾见证。
//!
//! 硬边界（spec §1.1）：LLM/Agent 只能提议节点、变量、边、证据锚和实验
//! 设计；编译器 MUST 拒绝 Agent 注入哈希、后验概率或布局坐标作为可信
//! 事实（见 `compile::compile_factor_graph` 的注入检测）。

use crate::invariant::Severity;
use serde::Serialize;

pub mod bp;
pub mod compile;
pub mod contradiction;
pub mod statistics;

/// 因子种类 / Factor kinds (spec §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FactorKind {
    /// supports：源为真 ⇒ 目标收到正证据（有向）。
    Supports,
    /// contradicts：源为真 ⇒ 目标收到反驳证据（双向翻转极性）。
    Contradicts,
    /// implies：只惩罚 A 真 B 假；B 真不推 A 真（非逆命题误用）。
    Implies,
    /// equivalent：双向 implies。
    Equivalent,
    /// and：输出真 ⇔ 全部输入真（真值表因子）。
    And,
    /// or：输出真 ⇔ 至少一个输入真。
    Or,
    /// depends_on：门控因子，效力 η 为 0 时无影响。
    DependsOn,
    /// statistical_test：实验统计量 → 方向性证据因子。
    StatisticalTest,
    /// meta_evidence：元证据（同一 cohort 相关证据合并）。
    MetaEvidence,
    /// interaction：组合效应（按 And 语义处理）。
    Interaction,
}

/// 变量规格 / Variable specification.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactorVariable {
    /// 变量名（取自节点 id）。
    pub name: String,
    /// 有限枚举取值（按规范顺序；布尔变量即 ["true","false"]）。
    pub domain: Vec<String>,
    /// 支持通道先验 logit（默认 0 = 无信息；GC10-01 先验 0）。
    pub prior_support_logit: f64,
    /// 反驳通道先验 logit（默认 0 = 无信息）。
    pub prior_refutation_logit: f64,
    /// 是否接地（有证据/已审）；false 时 BP 诊断 NoEvidence。
    pub grounded: bool,
}

impl FactorVariable {
    /// 布尔变量（默认先验 0，无信息）。
    pub fn boolean(name: &str) -> Self {
        Self {
            name: name.to_string(),
            domain: vec!["true".to_string(), "false".to_string()],
            prior_support_logit: 0.0,
            prior_refutation_logit: 0.0,
            grounded: false,
        }
    }
}

/// 单个因子：变量域 + 证据参数（spec §4）。
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Factor {
    /// 因子种类。
    pub kind: FactorKind,
    /// 参与变量（输入 + 输出，按规范顺序）。
    pub variables: Vec<String>,
    /// 有效消息 \\(w_e=\\eta_e\\lambda_e\\)；None = 无统计证据（未接地）。
    pub effective_message: Option<f64>,
    /// 边效力 η = 五质量乘积（默认全 1 时 = 1）。
    pub efficacy: f64,
    /// 是否接地（有统计证据/已审）。
    pub grounded: bool,
    /// 语义边 id（可追溯回源图）。
    pub source_edge: Option<String>,
    /// 局部对数证据 λ（统计模块产出）。
    pub local_log_evidence: Option<f64>,
    /// 校准方法（无证据时为 Ungrounded）。
    pub calibration: statistics::CalibrationMethod,
}

impl Factor {
    /// 构造布尔逻辑因子（无统计证据，逻辑约束强度 1）。
    pub fn logical(kind: FactorKind, variables: Vec<String>, source_edge: Option<String>) -> Self {
        Self {
            kind,
            variables,
            effective_message: None,
            efficacy: 1.0,
            grounded: false,
            source_edge,
            local_log_evidence: None,
            calibration: statistics::CalibrationMethod::Ungrounded,
        }
    }
}

/// 编译诊断 / Compile diagnostic (kebab-case 错误码，排序稳定)。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactorDiagnostic {
    /// 机器可读代码，如 "invalid-p-value"。
    pub code: String,
    pub severity: Severity,
    /// 违规实体位置，如 "edge:x1"。
    pub entity: String,
    pub message: String,
}

impl FactorDiagnostic {
    /// 构造诊断 / Construct a diagnostic.
    pub fn new(code: &str, severity: Severity, entity: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            severity,
            entity: entity.to_string(),
            message: message.into(),
        }
    }
}

/// 因子图 \\(\mathcal F=(X,F,E_F)\\)（spec §1.2）。
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactorGraph {
    /// 变量集合（按变量名排序）。
    pub variables: Vec<FactorVariable>,
    /// 因子列表（按语义边 id 排序，BP 遍历顺序固定）。
    pub factors: Vec<Factor>,
    /// 编译诊断（稳定排序）。
    pub diagnostics: Vec<FactorDiagnostic>,
}

impl FactorGraph {
    /// 变量名 → 索引的查找表（变量已按名排序）。
    pub fn variable_index(&self, name: &str) -> Option<usize> {
        self.variables
            .binary_search_by(|v| v.name.as_str().cmp(name))
            .ok()
    }

    /// 变量是否孤立（无任何因子连接）——GC08-11 NoEvidence。
    pub fn is_isolated(&self, var_index: usize) -> bool {
        self.factors
            .iter()
            .all(|factor| !factor.variables.contains(&self.variables[var_index].name))
    }

    /// 是否包含环（二部图层面）——真时需 loopy BP（GC08-12）。
    pub fn has_cycle(&self) -> bool {
        let var_count = self.variables.len();
        let factor_count = self.factors.len();
        // 二部图：变量节点 0..var_count，因子节点 var_count..var_count+factor_count。
        let total = var_count + factor_count;
        let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); total];
        for (f, factor) in self.factors.iter().enumerate() {
            for name in &factor.variables {
                if let Some(v) = self.variable_index(name) {
                    adjacency[v].push(var_count + f);
                    adjacency[var_count + f].push(v);
                }
            }
        }
        let mut visited = vec![false; total];
        let mut parent = vec![usize::MAX; total];
        for start in 0..total {
            if visited[start] {
                continue;
            }
            // 迭代 DFS，检测环（无向二部图，非树边即环）。
            let mut stack: Vec<(usize, usize)> = vec![(start, usize::MAX)];
            visited[start] = true;
            while let Some((node, from)) = stack.pop() {
                for &next in &adjacency[node] {
                    if next == from {
                        continue;
                    }
                    if visited[next] {
                        return true;
                    }
                    visited[next] = true;
                    parent[next] = node;
                    stack.push((next, node));
                }
            }
        }
        false
    }
}
