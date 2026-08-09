//! 统计证据与边效力 / Statistical evidence & edge efficacy (spec §4, GC-09)。
//!
//! p 值/效应量/Bayes factor → 可追踪的局部对数证据 \\(\lambda_e\\)，按设计、
//! 来源、匹配、独立性与复现度衰减为效力 \\(\eta_e\\)，有效消息
//! \\(w_e=\\eta_e\\lambda_e\\)。
//!
//! 关键纪律（spec §1.1 / GC-09）：`pValue` 是观测统计量，绝不能直接等同为
//! 主张为真的概率。只有 p 值时采用显式、保守、可追踪的校准
//! `ConservativePValueLlR` 并在输出标记 `calibration`。所有非法输入
//! （p∉[0,1]、Bayes factor≤0、CI 反转、quality∉[0,1]）一律拒绝，不自动截断。

use serde::Serialize;

/// 边质量五元组 / Edge quality quintuple (spec §4).
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
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

impl EdgeQuality {
    /// 五元逐一校验 ∈ [0,1]（GC09-11：不得自动截断）。
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("design", self.design),
            ("source", self.source),
            ("conditionMatch", self.condition_match),
            ("independence", self.independence),
            ("reproducibility", self.reproducibility),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(format!(
                    "invalid-quality-score: {name}={value} must be in [0,1]"
                ));
            }
        }
        Ok(())
    }
}

/// 证据方向 / Evidence direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceDirection {
    Supports,
    Refutes,
    Neutral,
}

/// 统计证据种类（可追踪校准的输入）/ Statistical evidence variants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StatisticalEvidence {
    /// 只有 p 值：采用保守似然比上界校准（GC09-01/02/03）。
    PValue {
        p: f64,
        direction: EvidenceDirection,
    },
    /// Bayes factor：λ = ln(BF)（GC09-05/06）。
    BayesFactor { factor: f64 },
    /// 效应量 + 标准误：按点零 vs 点备择正态模型（GC09-07）。
    NormalEffect {
        effect: f64,
        standard_error: f64,
        direction: EvidenceDirection,
    },
    /// 置信区间 [lower, upper]：上下界反转即拒绝（GC09-08）。
    ConfidenceInterval {
        lower: f64,
        upper: f64,
        direction: EvidenceDirection,
    },
}

/// 校准方法 / Calibration method (GC09-01: 非 DirectProbability 时标记)。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalibrationMethod {
    /// p 值保守似然比上界：λ = sign·ln(1/max(p, p_min))。
    ConservativePValueLlR,
    /// Bayes factor 直接对数：λ = ln(BF)。
    BayesFactorDirect,
    /// 点零 vs 点备择正态模型：λ = sign·effect²/(2·SE²)。
    NormalEffectModel,
    /// 置信区间上下界对数比：λ = sign·ln(upper/lower)（区间不含 1 才有信息）。
    ConfidenceIntervalLlR,
    /// 无统计证据：逻辑因子或未接地（GC08-10）。
    Ungrounded,
}

/// 编译后的边度量 / Compiled edge metric (spec §4).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledEdgeMetric {
    /// 局部对数证据（LLR），None = 无统计证据。
    pub local_log_evidence: Option<f64>,
    /// 边效力 η = 五质量乘积（默认全 1 时 = 1）。
    pub efficacy: f64,
    /// 有效消息 w = η·λ（GC09-10：任一 quality=0 时 η=0，消息归零）。
    pub effective_message: Option<f64>,
    /// 校准方法（无证据时为 Ungrounded）。
    pub calibration: CalibrationMethod,
    /// 警告（kebab-case 代码，稳定排序）。
    pub warnings: Vec<String>,
}

/// p 值下界夹紧（GC09-02：p=0 → 最小正数，保证有限 LLR）。
pub const P_VALUE_FLOOR: f64 = 1e-6;

/// 边效力 η = q_design × q_source × q_match × q_independence × q_reproducibility。
/// 调用方须先 `validate`，此处假定五元均在 [0,1]。
pub fn edge_efficacy(quality: &EdgeQuality) -> f64 {
    quality.design
        * quality.source
        * quality.condition_match
        * quality.independence
        * quality.reproducibility
}

/// 方向符号：supports=+1, refutes=-1, neutral=0。
fn direction_sign(direction: EvidenceDirection) -> f64 {
    match direction {
        EvidenceDirection::Supports => 1.0,
        EvidenceDirection::Refutes => -1.0,
        EvidenceDirection::Neutral => 0.0,
    }
}

/// 校验并计算局部对数证据 λ（可追踪、固定算法）。
/// 非法输入返回 Err（kebab-case 前缀错误信息）；合法输入给出有限 λ。
pub fn log_likelihood_ratio(evidence: &StatisticalEvidence) -> Result<f64, String> {
    match *evidence {
        StatisticalEvidence::PValue { p, direction } => {
            if !p.is_finite() || !(0.0..=1.0).contains(&p) {
                return Err(format!("invalid-p-value: p={p} must be in [0,1] (GC09-04)"));
            }
            // 保守似然比上界：P(D|H)≤1，P(D|¬H)=p ⇒ LLR ≤ ln(1/p)。
            // p=0 夹紧到 P_VALUE_FLOOR 并警告；p=1 ⇒ λ=0（无信息，无 log(0)）。
            let clamped = p.max(P_VALUE_FLOOR);
            let magnitude = (1.0 / clamped).ln();
            Ok(direction_sign(direction) * magnitude)
        }
        StatisticalEvidence::BayesFactor { factor } => {
            if !factor.is_finite() || factor <= 0.0 {
                return Err(format!(
                    "invalid-bayes-factor: factor={factor} must be > 0 (GC09-06)"
                ));
            }
            Ok(factor.ln())
        }
        StatisticalEvidence::NormalEffect {
            effect,
            standard_error,
            direction,
        } => {
            if !effect.is_finite() {
                return Err(format!("invalid-effect: effect={effect} must be finite"));
            }
            if !standard_error.is_finite() || standard_error <= 0.0 {
                return Err(format!(
                    "invalid-standard-error: standardError={standard_error} must be > 0"
                ));
            }
            // 点零 vs 点备择正态模型：观测效应 x~N(μ,σ²)，H:μ=0，
            // LLR = x²/(2σ²)；方向由实验方向决定。
            let magnitude = (effect * effect) / (2.0 * standard_error * standard_error);
            Ok(direction_sign(direction) * magnitude)
        }
        StatisticalEvidence::ConfidenceInterval {
            lower,
            upper,
            direction,
        } => {
            if !lower.is_finite() || !upper.is_finite() {
                return Err("invalid-confidence-interval: bounds must be finite".to_string());
            }
            if lower > upper {
                return Err(format!(
                    "invalid-confidence-interval: lower={lower} > upper={upper} (GC09-08)"
                ));
            }
            if lower <= 0.0 {
                return Err(format!(
                    "invalid-confidence-interval: lower={lower} must be > 0 for LLR"
                ));
            }
            // 保守区间校准：λ = sign·ln(upper/lower)；区间包含 1（无效应）→ 无信息。
            let magnitude = (upper / lower).ln();
            Ok(direction_sign(direction) * magnitude)
        }
    }
}

/// 统计证据标准化：校验 + 效力衰减 + 有效消息（spec §4, GC-09）。
/// 无证据（None）时：η 照算，λ=None，有效消息=None，校准=Ungrounded。
pub fn normalize_statistical_evidence(
    quality: &EdgeQuality,
    evidence: Option<StatisticalEvidence>,
) -> Result<CompiledEdgeMetric, String> {
    quality.validate()?;
    let efficacy = edge_efficacy(quality);

    let Some(evidence) = evidence else {
        return Ok(CompiledEdgeMetric {
            local_log_evidence: None,
            efficacy,
            effective_message: None,
            calibration: CalibrationMethod::Ungrounded,
            warnings: Vec::new(),
        });
    };

    let mut warnings = Vec::new();
    let lambda = log_likelihood_ratio(&evidence)?;
    // p 值夹紧警告（GC09-02）。
    if let StatisticalEvidence::PValue { p, .. } = evidence {
        if p == 0.0 {
            warnings.push("p-value-clamped-to-floor".to_string());
        }
    }

    let calibration = match evidence {
        StatisticalEvidence::PValue { .. } => CalibrationMethod::ConservativePValueLlR,
        StatisticalEvidence::BayesFactor { .. } => CalibrationMethod::BayesFactorDirect,
        StatisticalEvidence::NormalEffect { .. } => CalibrationMethod::NormalEffectModel,
        StatisticalEvidence::ConfidenceInterval { .. } => CalibrationMethod::ConfidenceIntervalLlR,
    };

    // w = η·λ；η=0 ⇒ w=0（消息归零但保留原统计量，GC09-10）。
    let effective_message = Some(efficacy * lambda);
    Ok(CompiledEdgeMetric {
        local_log_evidence: Some(lambda),
        efficacy,
        effective_message,
        calibration,
        warnings,
    })
}

/// 相关证据合并 / Related-evidence combination (GC09-12)。
/// 同一 cohort 的两项证据相关系数 ρ：ρ=1 时完全相关，合并为一份
/// （避免当作独立证据相加）；0≤ρ<1 时按有效样本数缩放 1/(1+(n-1)ρ)。
/// 返回合并后的有效证据量（在 logit 域相加前的缩放系数）。
pub fn combine_related_evidence(weights: &[f64], correlation: f64) -> Result<f64, String> {
    if weights.is_empty() {
        return Ok(0.0);
    }
    if weights.iter().any(|w| !w.is_finite()) {
        return Err("invalid-weight: weights must be finite (NaN rejected)".to_string());
    }
    if !correlation.is_finite() || !(0.0..=1.0).contains(&correlation) {
        return Err(format!(
            "invalid-correlation: ρ={correlation} must be in [0,1]"
        ));
    }
    let n = weights.len() as f64;
    if correlation >= 1.0 {
        // 完全相关：信息不随数量增加，取绝对值最大者（保留符号）。
        // 对负权重而言，-5.0 比 -1.0 更强，原 max 会错选 -1.0。
        let strongest = weights
            .iter()
            .cloned()
            .max_by(|a, b| a.abs().partial_cmp(&b.abs()).unwrap())
            .unwrap_or(0.0);
        return Ok(strongest);
    }
    // 有效样本缩放：n_eff = n / (1 + (n-1)ρ)。
    let scale = 1.0 / (1.0 + (n - 1.0) * correlation);
    Ok(weights.iter().sum::<f64>() * scale)
}

// ---------------------------------------------------------------------------
// 单元测试 / Unit tests (GC-09)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn quality(v: f64) -> EdgeQuality {
        EdgeQuality {
            design: v,
            source: v,
            condition_match: v,
            independence: v,
            reproducibility: v,
        }
    }

    #[test]
    fn gc09_01_pvalue_uses_conservative_calibration() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::PValue {
                p: 0.05,
                direction: EvidenceDirection::Supports,
            }),
        )
        .unwrap();
        assert_eq!(metric.calibration, CalibrationMethod::ConservativePValueLlR);
        let expected = (1.0_f64 / 0.05).ln();
        assert!((metric.local_log_evidence.unwrap() - expected).abs() < 1e-12);
        assert!((metric.effective_message.unwrap() - expected).abs() < 1e-12);
    }

    #[test]
    fn gc09_02_zero_pvalue_clamps_and_warns() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::PValue {
                p: 0.0,
                direction: EvidenceDirection::Supports,
            }),
        )
        .unwrap();
        let expected = (1.0 / P_VALUE_FLOOR).ln();
        assert!((metric.local_log_evidence.unwrap() - expected).abs() < 1e-12);
        assert!(metric
            .warnings
            .contains(&"p-value-clamped-to-floor".to_string()));
        assert!(metric.local_log_evidence.unwrap().is_finite());
    }

    #[test]
    fn gc09_03_pvalue_one_is_neutral() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::PValue {
                p: 1.0,
                direction: EvidenceDirection::Supports,
            }),
        )
        .unwrap();
        assert_eq!(metric.local_log_evidence.unwrap(), 0.0);
    }

    #[test]
    fn gc09_04_pvalue_out_of_range_rejected() {
        for p in [-0.1, 1.5] {
            let err = normalize_statistical_evidence(
                &EdgeQuality::default(),
                Some(StatisticalEvidence::PValue {
                    p,
                    direction: EvidenceDirection::Supports,
                }),
            )
            .unwrap_err();
            assert!(err.starts_with("invalid-p-value"), "{err}");
        }
    }

    #[test]
    fn gc09_05_bayes_factor_ten_is_ln_ten() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::BayesFactor { factor: 10.0 }),
        )
        .unwrap();
        assert_eq!(metric.calibration, CalibrationMethod::BayesFactorDirect);
        assert!((metric.local_log_evidence.unwrap() - 10f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn gc09_06_nonpositive_bayes_factor_rejected() {
        for factor in [0.0, -1.0] {
            let err = normalize_statistical_evidence(
                &EdgeQuality::default(),
                Some(StatisticalEvidence::BayesFactor { factor }),
            )
            .unwrap_err();
            assert!(err.starts_with("invalid-bayes-factor"), "{err}");
        }
    }

    #[test]
    fn gc09_07_effect_and_se_generate_evidence() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::NormalEffect {
                effect: 2.0,
                standard_error: 1.0,
                direction: EvidenceDirection::Supports,
            }),
        )
        .unwrap();
        assert_eq!(metric.calibration, CalibrationMethod::NormalEffectModel);
        assert!((metric.local_log_evidence.unwrap() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn gc09_08_inverted_ci_rejected() {
        let err = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::ConfidenceInterval {
                lower: 1.5,
                upper: 0.5,
                direction: EvidenceDirection::Supports,
            }),
        )
        .unwrap_err();
        assert!(err.starts_with("invalid-confidence-interval"), "{err}");
    }

    #[test]
    fn gc09_09_all_ones_quality_gives_identity() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::BayesFactor { factor: 10.0 }),
        )
        .unwrap();
        assert_eq!(metric.efficacy, 1.0);
        assert!((metric.effective_message.unwrap() - 10f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn gc09_10_zero_quality_zeroes_message() {
        let metric = normalize_statistical_evidence(
            &quality(0.0),
            Some(StatisticalEvidence::BayesFactor { factor: 10.0 }),
        )
        .unwrap();
        assert_eq!(metric.efficacy, 0.0);
        assert_eq!(metric.effective_message.unwrap(), 0.0);
        // 原统计量保留。
        assert!((metric.local_log_evidence.unwrap() - 10f64.ln()).abs() < 1e-12);
    }

    #[test]
    fn gc09_11_out_of_range_quality_rejected() {
        for value in [-0.01, 1.01, f64::NAN, f64::INFINITY] {
            let err = normalize_statistical_evidence(
                &quality(value),
                Some(StatisticalEvidence::BayesFactor { factor: 10.0 }),
            )
            .unwrap_err();
            assert!(err.starts_with("invalid-quality-score"), "{err}");
        }
    }

    #[test]
    fn gc09_12_related_evidence_not_summed_as_independent() {
        // ρ=1：两项完全相关证据不翻倍。
        let merged = combine_related_evidence(&[1.0, 1.0], 1.0).unwrap();
        assert_eq!(merged, 1.0);
        // ρ=0：独立证据正常相加。
        let independent = combine_related_evidence(&[1.0, 1.0], 0.0).unwrap();
        assert_eq!(independent, 2.0);
        // 0<ρ<1：按有效样本缩放。
        let scaled = combine_related_evidence(&[1.0, 1.0], 0.5).unwrap();
        assert!((scaled - 4.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn rho_one_negative_weights_pick_strongest_magnitude() {
        let merged = combine_related_evidence(&[-1.0, -5.0], 1.0).unwrap();
        assert_eq!(merged, -5.0, "ρ=1 with negative weights must pick the strongest (most negative)");
    }

    #[test]
    fn nan_weights_are_rejected() {
        let err = combine_related_evidence(&[1.0, f64::NAN], 0.5).unwrap_err();
        assert!(err.starts_with("invalid-weight"), "{err}");
    }

    #[test]
    fn direction_sign_controls_refutation() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::PValue {
                p: 0.01,
                direction: EvidenceDirection::Refutes,
            }),
        )
        .unwrap();
        assert!(metric.local_log_evidence.unwrap() < 0.0);
        assert!(metric.effective_message.unwrap() < 0.0);
    }

    #[test]
    fn neutral_direction_gives_zero_evidence() {
        let metric = normalize_statistical_evidence(
            &EdgeQuality::default(),
            Some(StatisticalEvidence::PValue {
                p: 0.01,
                direction: EvidenceDirection::Neutral,
            }),
        )
        .unwrap();
        assert_eq!(metric.local_log_evidence.unwrap(), 0.0);
    }

    #[test]
    fn ungrounded_evidence_marks_calibration() {
        let metric = normalize_statistical_evidence(&EdgeQuality::default(), None).unwrap();
        assert_eq!(metric.calibration, CalibrationMethod::Ungrounded);
        assert_eq!(metric.local_log_evidence, None);
        assert_eq!(metric.effective_message, None);
        assert_eq!(metric.efficacy, 1.0);
    }
}
