//! Consistency checks (handoff-spec.md §54–§61).
//!
//! Five check kinds — `path`, `representation`, `branch`, `abstraction`,
//! `conflict` — plus the conflict classification rule (§61):
//!
//! ```text
//! Conflict → ContextComparison → AxiomComparison → InternalConflict
//! ```
//!
//! Contradictory high-certainty systems are never averaged into `0.5`; they
//! are partitioned by context and axiom set and preserved separately (V3-18).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::ir::{CheckType, ConsistencyCheck};

/// Conflict classification (handoff-spec.md §59).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictClass {
    ContextualDivergence,
    AxiomaticDivergence,
    InternalConflict,
    InsufficientResolution,
}

/// Classify a conflict between two chains/operators by their conditioning
/// (handoff-spec.md §59, §61). Context is compared before axioms (V3-18).
pub fn classify_conflict(
    a_context: Option<&str>,
    a_axiom: Option<&str>,
    b_context: Option<&str>,
    b_axiom: Option<&str>,
) -> ConflictClass {
    if let (Some(ca), Some(cb)) = (a_context, b_context) {
        if ca != cb {
            return ConflictClass::ContextualDivergence;
        }
    }
    if let (Some(aa), Some(ab)) = (a_axiom, b_axiom) {
        if aa != ab {
            return ConflictClass::AxiomaticDivergence;
        }
    }
    // A conditioning dimension present on one side but missing on the other
    // cannot be compared fully.
    if a_context.is_some() != b_context.is_some() || a_axiom.is_some() != b_axiom.is_some() {
        return ConflictClass::InsufficientResolution;
    }
    ConflictClass::InternalConflict
}

/// Relative difference between two numerical outcomes (handoff-spec.md §55).
/// Returns `(relative_difference, status)`.
pub fn path_consistency(value_a: f64, value_b: f64, threshold: f64) -> (f64, String) {
    let denominator = value_a.abs().max(value_b.abs());
    let relative_difference = if denominator == 0.0 {
        (value_a - value_b).abs()
    } else {
        (value_a - value_b).abs() / denominator
    };
    let status = if relative_difference <= threshold {
        "pass".to_string()
    } else {
        "flag".to_string()
    };
    (relative_difference, status)
}

/// Branch consistency: distance between a coarse outcome and the aggregate of
/// fine-grained outcomes (handoff-spec.md §57).
pub fn branch_consistency(coarse: f64, fine: &[f64]) -> (f64, String) {
    let mean = fine.iter().sum::<f64>() / fine.len().max(1) as f64;
    let distance = (coarse - mean).abs();
    (distance, if distance == 0.0 { "pass".to_string() } else { "flag".to_string() })
}

/// Build a [`ConsistencyCheck`] with the common field shape.
#[allow(clippy::too_many_arguments)]
pub fn make_check(
    id: &str,
    check_type: CheckType,
    input_refs: &[String],
    metric: Option<&str>,
    value: Option<f64>,
    threshold: Option<f64>,
    status: &str,
    details: Value,
) -> ConsistencyCheck {
    ConsistencyCheck {
        id: id.to_string(),
        check_type,
        input_refs: input_refs.to_vec(),
        metric: metric.map(str::to_string),
        value,
        threshold,
        status: status.to_string(),
        details,
    }
}

/// Build a conflict check record (handoff-spec.md §60).
pub fn conflict_check(
    id: &str,
    input_refs: &[String],
    classification: ConflictClass,
) -> ConsistencyCheck {
    make_check(
        id,
        CheckType::Conflict,
        input_refs,
        None,
        None,
        None,
        "flag",
        json!({ "classification": classification }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_compared_before_axioms() {
        assert_eq!(
            classify_conflict(Some("ctx_a"), Some("ax_1"), Some("ctx_b"), Some("ax_1")),
            ConflictClass::ContextualDivergence
        );
        assert_eq!(
            classify_conflict(Some("ctx_a"), Some("ax_1"), Some("ctx_a"), Some("ax_2")),
            ConflictClass::AxiomaticDivergence
        );
        assert_eq!(
            classify_conflict(Some("ctx_a"), Some("ax_1"), Some("ctx_a"), Some("ax_1")),
            ConflictClass::InternalConflict
        );
    }

    #[test]
    fn missing_conditioning_is_insufficient_resolution() {
        assert_eq!(
            classify_conflict(Some("ctx_a"), None, None, None),
            ConflictClass::InsufficientResolution
        );
        assert_eq!(
            classify_conflict(None, None, None, None),
            ConflictClass::InternalConflict
        );
    }

    #[test]
    fn path_consistency_reports_relative_difference() {
        let (value, status) = path_consistency(0.10, 0.102, 0.05);
        assert_eq!(status, "pass");
        assert!(value <= 0.05);

        let (_, flag) = path_consistency(0.10, 0.20, 0.05);
        assert_eq!(flag, "flag");
    }

    #[test]
    fn branch_consistency_compares_coarse_to_fine_mean() {
        let (distance, status) = branch_consistency(1.0, &[1.0, 1.0, 1.0]);
        assert_eq!(status, "pass");
        assert_eq!(distance, 0.0);

        let (distance, status) = branch_consistency(1.5, &[0.5, 1.0]);
        assert_eq!(status, "flag");
        assert!((distance - 0.75).abs() < 1e-9);
    }

    #[test]
    fn conflict_check_builds_ir_record() {
        let check = conflict_check("check_1", &["c1".to_string(), "c2".to_string()], ConflictClass::ContextualDivergence);
        assert_eq!(check.check_type, CheckType::Conflict);
        assert_eq!(check.status, "flag");
        assert_eq!(check.details["classification"], "contextual_divergence");
    }
}
