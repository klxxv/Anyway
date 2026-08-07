//! 统计证据与边效力 / Statistical evidence & edge efficacy (spec §4, GC-09)。
//! p 值/效应量/Bayes factor → 可追踪的局部对数证据 \\(\lambda_e\\)，
//! 按设计、来源、匹配、独立性与复现度衰减为效力 \\(\eta_e\\)。

/// 边质量五元组 / Edge quality quintuple (spec §4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeQuality {
    pub design: f64,
    pub source: f64,
    pub condition_match: f64,
    pub independence: f64,
    pub reproducibility: f64,
}

impl Default for EdgeQuality {
    fn default() -> Self {
        Self {
            design: 1.0,
            source: 1.0,
            condition_match: 1.0,
            independence: 1.0,
            reproducibility: 1.0,
        }
    }
}

/// 编译后的边度量（骨架占位）：效力 = 五质量乘积。
/// 有效证据消息 \\(w_e=\eta_e\lambda_e\\) 随统计证据接入后填充。
#[derive(Clone, Debug, PartialEq)]
pub struct CompiledEdgeMetric {
    /// 局部对数证据（LLR），可能为 None（无统计证据）。
    pub local_log_evidence: Option<f64>,
    /// 边效力 η（默认全 1 时 = 1）。
    pub efficacy: f64,
    /// 有效消息（效力 × 局部证据）。
    pub effective_message: Option<f64>,
}

/// 统计证据标准化（骨架占位：p 值保守校准 GC09-01/02/03 随实现接入）。
pub fn normalize_statistical_evidence(
    quality: &EdgeQuality,
    local_log_evidence: Option<f64>,
) -> CompiledEdgeMetric {
    let efficacy = quality.design
        * quality.source
        * quality.condition_match
        * quality.independence
        * quality.reproducibility;
    CompiledEdgeMetric {
        local_log_evidence,
        efficacy,
        effective_message: local_log_evidence.map(|lambda| efficacy * lambda),
    }
}
