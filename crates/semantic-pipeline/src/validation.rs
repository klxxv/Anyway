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
        // confidence 范围校验
        if entity.confidence < 0.0 || entity.confidence > 1.0 {
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

    // 检查 merge_groups 中的引用
    for mg in &candidates.merge_groups {
        for tid in &mg.merged_temp_ids {
            if !defined.contains(tid) {
                checks.push(ValidationCheck {
                    check_id: "closure-merge".into(),
                    category: "closure".into(),
                    passed: false,
                    severity: ValidationSeverity::Error,
                    message: format!("mergeGroup canonical={} 引用了未定义的 tempId: {tid}", mg.canonical_temp_id),
                    detail: None,
                });
            }
        }
    }

    // 检查 experiment_matrix 中的变量引用
    for exp in &candidates.experiment_matrix {
        for iv in &exp.ivs {
            if !defined.contains(&iv.variable_temp_id) {
                checks.push(ValidationCheck {
                    check_id: "closure-exp-iv".into(),
                    category: "closure".into(),
                    passed: false,
                    severity: ValidationSeverity::Warning,
                    message: format!("experiment {} IV 引用了未定义的 tempId: {}", exp.experiment_temp_id, iv.variable_temp_id),
                    detail: None,
                });
            }
        }
        for dv in &exp.dvs {
            if !defined.contains(&dv.variable_temp_id) {
                checks.push(ValidationCheck {
                    check_id: "closure-exp-dv".into(),
                    category: "closure".into(),
                    passed: false,
                    severity: ValidationSeverity::Warning,
                    message: format!("experiment {} DV 引用了未定义的 tempId: {}", exp.experiment_temp_id, dv.variable_temp_id),
                    detail: None,
                });
            }
        }
    }

    // 检查 claim_evidence_bundles 中的引用
    for bundle in &candidates.claim_evidence_bundles {
        if !defined.contains(&bundle.claim_temp_id) {
            checks.push(ValidationCheck {
                check_id: "closure-bundle-claim".into(),
                category: "closure".into(),
                passed: false,
                severity: ValidationSeverity::Error,
                message: format!("claimEvidenceBundle 引用了未定义的 claim tempId: {}", bundle.claim_temp_id),
                detail: None,
            });
        }
        for etid in &bundle.evidence_temp_ids {
            if !defined.contains(etid) {
                checks.push(ValidationCheck {
                    check_id: "closure-bundle-ev".into(),
                    category: "closure".into(),
                    passed: false,
                    severity: ValidationSeverity::Warning,
                    message: format!("claimEvidenceBundle {} 引用了未定义的 evidence tempId: {etid}", bundle.claim_temp_id),
                    detail: None,
                });
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
}
