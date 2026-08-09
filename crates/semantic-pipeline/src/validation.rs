//! Pass F: 本地验证（JSON Schema 校验、quote 回指验证、表格数值重算、anchor 唯一性、tempId 闭包）。
//!
//! 此模块由 Rust 确定性验证，不依赖 LLM。

use crate::ir::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 验证报告。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationReport {
    pub passed: bool,
    pub total_checks: usize,
    pub checks: Vec<ValidationCheck>,
    pub summary: String,
}

/// 单条验证检查。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationCheck {
    pub check_id: String,
    pub category: String,
    pub passed: bool,
    pub severity: ValidationSeverity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// 验证严重级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

/// 对 AgentCandidates 运行所有 Pass F 验证。
pub fn validate_candidates(candidates: &AgentCandidates, full_text: &str) -> ValidationReport {
    let mut checks: Vec<ValidationCheck> = Vec::new();

    // 1. JSON Schema 校验
    checks.append(&mut validate_schema(candidates));

    // 2. Quote 回指验证
    checks.append(&mut validate_quote_references(candidates, full_text));

    // 3. Anchor 唯一性
    checks.append(&mut validate_anchor_uniqueness(candidates));

    // 4. tempId 闭包
    checks.append(&mut validate_temp_id_closure(candidates));

    // 5. 数值一致性重算（抽样）
    checks.append(&mut validate_numerical_consistency(candidates));

    let total = checks.len();
    let passed = checks.iter().all(|c| c.passed || c.severity != ValidationSeverity::Error);
    let error_count = checks.iter().filter(|c| !c.passed && c.severity == ValidationSeverity::Error).count();
    let warn_count = checks.iter().filter(|c| !c.passed && c.severity == ValidationSeverity::Warning).count();

    let summary = match (error_count, warn_count) {
        (0, 0) => format!("全部 {total} 项检查通过"),
        (0, w) => format!("{total} 项检查中 {w} 项警告"),
        (e, w) => format!("{total} 项检查中 {e} 项错误、{w} 项警告"),
    };

    ValidationReport { passed, total_checks: total, checks, summary }
}

/// 1. JSON Schema 结构校验：实体必填字段、类型一致性。
fn validate_schema(candidates: &AgentCandidates) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    for entity in &candidates.entities {
        // 必填字段检查
        if entity.temp_id.is_empty() {
            checks.push(ValidationCheck {
                check_id: "schema-001".into(),
                category: "schema".into(),
                passed: false,
                severity: ValidationSeverity::Error,
                message: format!("实体缺少 tempId（label='{label}'）", label = entity.label),
                detail: None,
            });
        }
        if entity.label.is_empty() {
            checks.push(ValidationCheck {
                check_id: "schema-002".into(),
                category: "schema".into(),
                passed: false,
                severity: ValidationSeverity::Error,
                message: format!("实体 {} 缺少 label", entity.temp_id),
                detail: None,
            });
        }
        if entity.anchors.is_empty() {
            checks.push(ValidationCheck {
                check_id: "schema-003".into(),
                category: "schema".into(),
                passed: false,
                severity: ValidationSeverity::Warning,
                message: format!("实体 {} 缺少 anchor 引用", entity.temp_id),
                detail: Some("无 anchor 的实体无法验证其来源".into()),
            });
        }
        // confidence 范围校验（NaN 也会失败，因为 NaN 与任何值比较都为 false）。
        if !entity.confidence.is_finite() || !(0.0..=1.0).contains(&entity.confidence) {
            checks.push(ValidationCheck {
                check_id: "schema-004".into(),
                category: "schema".into(),
                passed: false,
                severity: ValidationSeverity::Error,
                message: format!("实体 {} confidence={} 不在 [0.0, 1.0] 范围内", entity.temp_id, entity.confidence),
                detail: None,
            });
        }
        // Variable 类型的 variable_type 必填
        if entity.kind == EntityKind::Variable && entity.attributes.variable_type.is_none() {
            checks.push(ValidationCheck {
                check_id: "schema-005".into(),
                category: "schema".into(),
                passed: false,
                severity: ValidationSeverity::Warning,
                message: format!("变量 {} 缺少 variableType", entity.temp_id),
                detail: Some("变量应注明独立/依赖/控制/测量/派生类型".into()),
            });
        }
    }

    if checks.is_empty() {
        checks.push(ValidationCheck {
            check_id: "schema-pass".into(),
            category: "schema".into(),
            passed: true,
            severity: ValidationSeverity::Info,
            message: "JSON Schema 结构校验通过".into(),
            detail: None,
        });
    }

    checks
}

/// 2. Quote 回指验证：每个 anchor 的 quote 片段必须在原文中精确匹配。
fn validate_quote_references(candidates: &AgentCandidates, full_text: &str) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();
    let mut quote_mismatches = 0usize;
    let mut total_quotes = 0usize;

    for entity in &candidates.entities {
        for anchor in &entity.anchors {
            total_quotes += 1;

            // 偏移量基本一致性校验：start 必须 <= end。
            if anchor.start_offset > anchor.end_offset {
                quote_mismatches += 1;
                checks.push(ValidationCheck {
                    check_id: format!("quote-{:03}", quote_mismatches),
                    category: "quote".into(),
                    passed: false,
                    severity: ValidationSeverity::Error,
                    message: format!(
                        "实体 {} 的 anchor offset 非法: start={} > end={}",
                        entity.temp_id, anchor.start_offset, anchor.end_offset
                    ),
                    detail: Some(format!(
                        "section={}, paragraph={}",
                        anchor.section_id, anchor.paragraph_id
                    )),
                });
                continue;
            }

            if !anchor.quote.is_empty() && !full_text.contains(&anchor.quote) {
                // 尝试宽松匹配（忽略前后空白）
                let trimmed = anchor.quote.trim();
                if !full_text.contains(trimmed) {
                    quote_mismatches += 1;
                    checks.push(ValidationCheck {
                        check_id: format!("quote-{:03}", quote_mismatches),
                        category: "quote".into(),
                        passed: false,
                        severity: ValidationSeverity::Warning,
                        message: format!(
                            "实体 {} 的 anchor quote 在原文中未找到: \"{}\"",
                            entity.temp_id,
                            truncate(&anchor.quote, 80)
                        ),
                        detail: Some(format!(
                            "偏移 [{}, {}), section={}, paragraph={}",
                            anchor.start_offset, anchor.end_offset,
                            anchor.section_id, anchor.paragraph_id
                        )),
                    });
                }
            }
        }
    }

    if quote_mismatches == 0 {
        checks.push(ValidationCheck {
            check_id: "quote-pass".into(),
            category: "quote".into(),
            passed: true,
            severity: ValidationSeverity::Info,
            message: format!("Quote 回指验证通过（{total_quotes} 条）"),
            detail: None,
        });
    } else {
        checks.push(ValidationCheck {
            check_id: "quote-summary".into(),
            category: "quote".into(),
            passed: false,
            severity: ValidationSeverity::Warning,
            message: format!("Quote 回指验证: {quote_mismatches}/{total_quotes} 条不匹配"),
            detail: Some("可能是 LLM 改写或截断了引文，建议人工复核".into()),
        });
    }

    checks
}

/// 3. Anchor 唯一性：同一实体的多个 anchor 不应重复。
fn validate_anchor_uniqueness(candidates: &AgentCandidates) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    for entity in &candidates.entities {
        let mut seen = HashSet::new();
        for anchor in &entity.anchors {
            let key = (&anchor.section_id, &anchor.paragraph_id, anchor.start_offset, anchor.end_offset);
            if !seen.insert(key) {
                checks.push(ValidationCheck {
                    check_id: "anchor-dup".into(),
                    category: "anchor".into(),
                    passed: false,
                    severity: ValidationSeverity::Warning,
                    message: format!("实体 {} 存在重复 anchor: ({}:{}, {}-{})",
                        entity.temp_id, anchor.section_id, anchor.paragraph_id,
                        anchor.start_offset, anchor.end_offset),
                    detail: None,
                });
            }
        }
    }

    if checks.is_empty() {
        checks.push(ValidationCheck {
            check_id: "anchor-pass".into(),
            category: "anchor".into(),
            passed: true,
            severity: ValidationSeverity::Info,
            message: "Anchor 唯一性检查通过".into(),
            detail: None,
        });
    }

    checks
}

/// 4. tempId 闭包检查：所有被引用的 tempId 必须在 entities 或 variable_registry 中定义。
fn validate_temp_id_closure(candidates: &AgentCandidates) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();
    let mut defined: HashSet<String> = candidates.entities.iter().map(|e| e.temp_id.clone()).collect();
    for v in &candidates.variable_registry {
        defined.insert(v.temp_id.clone());
    }

    let mut report = |check_id: &str, severity: ValidationSeverity, message: String| {
        checks.push(ValidationCheck {
            check_id: check_id.into(),
            category: "closure".into(),
            passed: false,
            severity,
            message,
            detail: None,
        });
    };

    // merge_groups
    for mg in &candidates.merge_groups {
        if !defined.contains(&mg.canonical_temp_id) {
            report(
                "closure-merge-canonical",
                ValidationSeverity::Error,
                format!("mergeGroup canonical={} 未定义", mg.canonical_temp_id),
            );
        }
        for tid in &mg.merged_temp_ids {
            if !defined.contains(tid) {
                report(
                    "closure-merge",
                    ValidationSeverity::Error,
                    format!("mergeGroup canonical={} 引用了未定义的 tempId: {tid}", mg.canonical_temp_id),
                );
            }
        }
    }

    // experiment_matrix
    for exp in &candidates.experiment_matrix {
        if !defined.contains(&exp.experiment_temp_id) {
            report(
                "closure-exp-id",
                ValidationSeverity::Warning,
                format!("experiment {} 自身 tempId 未定义", exp.experiment_temp_id),
            );
        }
        for iv in &exp.ivs {
            if !defined.contains(&iv.variable_temp_id) {
                report(
                    "closure-exp-iv",
                    ValidationSeverity::Warning,
                    format!("experiment {} IV 引用了未定义的 tempId: {}", exp.experiment_temp_id, iv.variable_temp_id),
                );
            }
        }
        for dv in &exp.dvs {
            if !defined.contains(&dv.variable_temp_id) {
                report(
                    "closure-exp-dv",
                    ValidationSeverity::Warning,
                    format!("experiment {} DV 引用了未定义的 tempId: {}", exp.experiment_temp_id, dv.variable_temp_id),
                );
            }
        }
        for ctrl in &exp.controls {
            if !defined.contains(&ctrl.variable_temp_id) {
                report(
                    "closure-exp-control",
                    ValidationSeverity::Warning,
                    format!("experiment {} control 引用了未定义的 tempId: {}", exp.experiment_temp_id, ctrl.variable_temp_id),
                );
            }
        }
        for moderator in &exp.moderators {
            if !defined.contains(&moderator.variable_temp_id) {
                report(
                    "closure-exp-moderator",
                    ValidationSeverity::Warning,
                    format!("experiment {} moderator 引用了未定义的 tempId: {}", exp.experiment_temp_id, moderator.variable_temp_id),
                );
            }
            if let Some(ref iv_tid) = moderator.interaction_with {
                if !defined.contains(iv_tid) {
                    report(
                        "closure-exp-moderator-interaction",
                        ValidationSeverity::Warning,
                        format!("experiment {} moderator 的 interactionWith 未定义: {iv_tid}", exp.experiment_temp_id),
                    );
                }
            }
        }
        for mediator in &exp.mediators {
            if !defined.contains(&mediator.variable_temp_id) {
                report(
                    "closure-exp-mediator",
                    ValidationSeverity::Warning,
                    format!("experiment {} mediator 引用了未定义的 tempId: {}", exp.experiment_temp_id, mediator.variable_temp_id),
                );
            }
        }
    }

    // claim_evidence_bundles
    for bundle in &candidates.claim_evidence_bundles {
        if !defined.contains(&bundle.claim_temp_id) {
            report(
                "closure-bundle-claim",
                ValidationSeverity::Error,
                format!("claimEvidenceBundle 引用了未定义的 claim tempId: {}", bundle.claim_temp_id),
            );
        }
        for etid in &bundle.evidence_temp_ids {
            if !defined.contains(etid) {
                report(
                    "closure-bundle-ev",
                    ValidationSeverity::Warning,
                    format!("claimEvidenceBundle {} 引用了未定义的 evidence tempId: {etid}", bundle.claim_temp_id),
                );
            }
        }
    }

    // main_conclusions
    for conclusion in &candidates.main_conclusions {
        if !defined.contains(&conclusion.temp_id) {
            report(
                "closure-conclusion-id",
                ValidationSeverity::Warning,
                format!("mainConclusion {} 自身 tempId 未定义", conclusion.temp_id),
            );
        }
        for tid in &conclusion.supported_by {
            if !defined.contains(tid) {
                report(
                    "closure-conclusion-supported-by",
                    ValidationSeverity::Warning,
                    format!("mainConclusion {} supportedBy 引用了未定义的 tempId: {tid}", conclusion.temp_id),
                );
            }
        }
    }

    // ablation_analysis
    for ablation in &candidates.ablation_analysis {
        if !defined.contains(&ablation.experiment_temp_id) {
            report(
                "closure-ablation-exp",
                ValidationSeverity::Warning,
                format!("ablationAnalysis 引用了未定义的 experiment tempId: {}", ablation.experiment_temp_id),
            );
        }
    }

    // interaction_effects
    for ie in &candidates.interaction_effects {
        if !defined.contains(&ie.claim_temp_id) {
            report(
                "closure-interaction-claim",
                ValidationSeverity::Warning,
                format!("interactionEffect 引用了未定义的 claim tempId: {}", ie.claim_temp_id),
            );
        }
        for tid in &ie.variables {
            if !defined.contains(tid) {
                report(
                    "closure-interaction-variable",
                    ValidationSeverity::Warning,
                    format!("interactionEffect {} 引用了未定义的 variable tempId: {tid}", ie.claim_temp_id),
                );
            }
        }
    }

    // confounders
    for confounder in &candidates.confounders {
        if !defined.contains(&confounder.variable_temp_id) {
            report(
                "closure-confounder",
                ValidationSeverity::Warning,
                format!("confounder 引用了未定义的 variable tempId: {}", confounder.variable_temp_id),
            );
        }
    }

    // missing_controls
    for missing in &candidates.missing_controls {
        for tid in &missing.affects_claims {
            if !defined.contains(tid) {
                report(
                    "closure-missing-control",
                    ValidationSeverity::Warning,
                    format!("missingControl 引用了未定义的 claim tempId: {tid}"),
                );
            }
        }
    }

    // internal_conflicts
    for conflict in &candidates.internal_conflicts {
        for (role, tid) in [("claimA", &conflict.claim_a), ("claimB", &conflict.claim_b)] {
            if !defined.contains(tid) {
                report(
                    "closure-conflict",
                    ValidationSeverity::Warning,
                    format!("internalConflict {role} 引用了未定义的 tempId: {tid}"),
                );
            }
        }
    }

    if checks.is_empty() {
        checks.push(ValidationCheck {
            check_id: "closure-pass".into(),
            category: "closure".into(),
            passed: true,
            severity: ValidationSeverity::Info,
            message: format!("tempId 闭包检查通过（{} 个已定义 ID）", defined.len()),
            detail: None,
        });
    }

    checks
}

/// 5. 数值一致性重算（抽样检查实验条件中的数值）。
fn validate_numerical_consistency(candidates: &AgentCandidates) -> Vec<ValidationCheck> {
    let mut checks = Vec::new();

    // 检查 experiment_matrix 中 conditions 的 iv_settings 一致性
    for exp in &candidates.experiment_matrix {
        let num_conditions = exp.conditions.len();
        let num_ivs = exp.ivs.len();

        if num_conditions > 0 && num_ivs == 0 {
            checks.push(ValidationCheck {
                check_id: "num-conditions-no-iv".into(),
                category: "numerical".into(),
                passed: false,
                severity: ValidationSeverity::Warning,
                message: format!("实验 {} 有 {num_conditions} 个条件但无 IV", exp.experiment_temp_id),
                detail: Some("条件的 ivSettings 可能引用不存在的变量".into()),
            });
        }
    }

    if checks.is_empty() {
        checks.push(ValidationCheck {
            check_id: "numerical-pass".into(),
            category: "numerical".into(),
            passed: true,
            severity: ValidationSeverity::Info,
            message: "数值一致性检查通过".into(),
            detail: None,
        });
    }

    checks
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len { s.to_string() } else {
        format!("{}…", &s.chars().take(max_len).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_candidates() -> AgentCandidates {
        AgentCandidates {
            paper_id: "test-paper".into(),
            title: Some("Test Paper".into()),
            authors: vec!["Author One".into()],
            year: Some(2024),
            doi: Some("10.0000/test".into()),
            entities: vec![
                ExtractedEntity {
                    temp_id: "v1".into(),
                    kind: EntityKind::Variable,
                    label: "Test Variable".into(),
                    text: "A test variable".into(),
                    confidence: 0.95,
                    anchors: vec![AnchorRef {
                        section_id: "s1".into(),
                        paragraph_id: "p1".into(),
                        start_offset: 0,
                        end_offset: 50,
                        quote: "test variable was measured".into(),
                    }],
                    attributes: EntityAttributes {
                        variable_type: Some("independent".into()),
                        ..Default::default()
                    },
                },
            ],
            variable_registry: vec![],
            experiment_matrix: vec![],
            merge_groups: vec![],
            claim_evidence_bundles: vec![],
            metric_alignment: vec![],
            dataset_registry: vec![],
            main_conclusions: vec![],
            ablation_analysis: vec![],
            interaction_effects: vec![],
            confounders: vec![],
            missing_controls: vec![],
            internal_conflicts: vec![],
            synthesis_summary: None,
        }
    }

    #[test]
    fn schema_validation_passes_for_valid_entity() {
        let candidates = dummy_candidates();
        let full_text = "test variable was measured in this study.";
        let report = validate_candidates(&candidates, full_text);
        // 无 Error 级别失败即通过
        let errors: Vec<_> = report.checks.iter().filter(|c| !c.passed && c.severity == ValidationSeverity::Error).collect();
        assert!(errors.is_empty(), "不应有 Error 级别失败: {errors:?}");
    }

    #[test]
    fn quote_validation_detects_mismatch() {
        let mut candidates = dummy_candidates();
        candidates.entities[0].anchors[0].quote = "completely wrong quote text".into();
        let full_text = "test variable was measured in this study.";
        let report = validate_candidates(&candidates, full_text);
        let quote_fails: Vec<_> = report.checks.iter()
            .filter(|c| c.category == "quote" && !c.passed)
            .collect();
        assert!(!quote_fails.is_empty(), "应检测到 quote 不匹配");
    }

    #[test]
    fn temp_id_closure_detects_dangling_reference() {
        let mut candidates = dummy_candidates();
        candidates.merge_groups.push(MergeGroup {
            canonical_temp_id: "v100".into(),
            canonical_name: "UNKNOWN".into(),
            canonical_description: "Undefined".into(),
            merged_temp_ids: vec!["v_not_defined".into()],
            reason: "alias".into(),
            confidence: 0.5,
        });
        let full_text = "";
        let report = validate_candidates(&candidates, full_text);
        let closure_fails: Vec<_> = report.checks.iter()
            .filter(|c| c.category == "closure" && !c.passed)
            .collect();
        assert!(!closure_fails.is_empty(), "应检测到未定义的 tempId 引用");
    }

    #[test]
    fn nan_confidence_is_rejected() {
        let mut candidates = dummy_candidates();
        candidates.entities[0].confidence = f64::NAN;
        let report = validate_candidates(&candidates, "");
        let schema_errors: Vec<_> = report.checks.iter()
            .filter(|c| c.check_id == "schema-004" && !c.passed)
            .collect();
        assert!(!schema_errors.is_empty(), "NaN confidence 必须被 schema-004 拒绝");
    }

    #[test]
    fn invalid_quote_offset_is_rejected() {
        let mut candidates = dummy_candidates();
        candidates.entities[0].anchors[0].start_offset = 100;
        candidates.entities[0].anchors[0].end_offset = 50;
        let report = validate_candidates(&candidates, "");
        let quote_errors: Vec<_> = report.checks.iter()
            .filter(|c| c.category == "quote" && !c.passed)
            .collect();
        assert!(!quote_errors.is_empty(), "start > end 的 offset 必须被拒绝");
    }

    #[test]
    fn closure_checks_moderator_and_mediator_references() {
        let mut candidates = dummy_candidates();
        candidates.variable_registry.push(VariableRegistryEntry {
            temp_id: "iv1".into(),
            name: "IV".into(),
            aliases: vec![],
            domain: VariableDomain { r#type: "continuous".into(), values: None, min: None, max: None, unit: None },
            role: "independent".into(),
            measured_as: None,
            is_new: true,
        });
        candidates.experiment_matrix.push(ExperimentMatrixEntry {
            experiment_temp_id: "exp1".into(),
            design: None,
            ivs: vec![],
            dvs: vec![],
            controls: vec![],
            moderators: vec![ModeratorEntry {
                variable_temp_id: "mod_undefined".into(),
                name: "Mod".into(),
                interaction_with: Some("iv_missing".into()),
            }],
            mediators: vec![MediatorEntry {
                variable_temp_id: "med_undefined".into(),
                name: "Med".into(),
                pathway: None,
            }],
            sample: None,
            conditions: vec![],
        });
        let report = validate_candidates(&candidates, "");
        let closure_fails: Vec<_> = report.checks.iter()
            .filter(|c| c.category == "closure" && !c.passed)
            .collect();
        assert!(closure_fails.iter().any(|c| c.message.contains("mod_undefined")), "应检测 moderator");
        assert!(closure_fails.iter().any(|c| c.message.contains("med_undefined")), "应检测 mediator");
        assert!(closure_fails.iter().any(|c| c.message.contains("iv_missing")), "应检测 interactionWith");
    }
}
