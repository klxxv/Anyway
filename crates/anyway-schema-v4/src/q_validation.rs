//! Q candidate validation (handoff-spec.md §33, §34, §58).
//!
//! The compiler evaluates a proposed quotient/abstraction against four
//! metrics — Compression, PredictionLoss, CommutationError, ConflictIncrease —
//! and **stores the measurements without automatic promotion** (handoff-spec.md
//! §34). An LLM abstraction always begins as `candidate` (V3-08); any other
//! LLM status is rejected as Q-001.

use serde::{Deserialize, Serialize};

use crate::extract::AbstractionCandidate;
use crate::validator::ValidationReport;

/// The four Q validation metrics (handoff-spec.md §34).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct QCandidateMetrics {
    pub compression: f64,
    pub prediction_loss: f64,
    pub commutation_error: f64,
    pub conflict_increase: f64,
}

impl Default for QCandidateMetrics {
    fn default() -> Self {
        Self {
            compression: 0.0,
            prediction_loss: 0.0,
            commutation_error: 0.0,
            conflict_increase: 0.0,
        }
    }
}

/// Acceptance thresholds (handoff-spec.md §34). Not enforced by the MVP.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct QThresholds {
    pub compression: f64,
    pub prediction_loss: f64,
    pub commutation_error: f64,
    pub conflict_increase: f64,
}

impl Default for QThresholds {
    fn default() -> Self {
        Self {
            compression: 0.5,
            prediction_loss: 0.1,
            commutation_error: 0.05,
            conflict_increase: 0.0,
        }
    }
}

/// Per-metric acceptance outcome (informational; never auto-applied).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct QAcceptanceCriteria {
    pub compression: bool,
    pub prediction_loss: bool,
    pub commutation_error: bool,
    pub conflict_increase: bool,
}

/// The stored result of evaluating one Q candidate.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct QValidationResult {
    pub candidate_id: String,
    /// Always `"candidate"` in the MVP (measurements are stored, not promoted).
    pub status: String,
    pub metrics: QCandidateMetrics,
    pub criteria: QAcceptanceCriteria,
}

/// A simple compression heuristic: `n` inputs collapse to one concept.
pub fn compression_metric(input_count: usize) -> f64 {
    if input_count <= 1 {
        0.0
    } else {
        (input_count as f64 - 1.0) / input_count as f64
    }
}

/// Evaluate a Q candidate. Returns Q-001 if the LLM status is anything other
/// than `candidate` (handoff-spec.md §33, V3-08).
pub fn validate_q(
    candidate: &AbstractionCandidate,
    metrics: QCandidateMetrics,
    thresholds: &QThresholds,
) -> Result<QValidationResult, ValidationReport> {
    if candidate.status != "candidate" {
        let mut report = ValidationReport::default();
        report.error(
            "Q-001",
            &format!("$.abstraction_candidates[{}].status", candidate.id),
            "an LLM abstraction must be a candidate, never validated or accepted",
        );
        return Err(report);
    }

    let criteria = QAcceptanceCriteria {
        compression: metrics.compression > thresholds.compression,
        prediction_loss: metrics.prediction_loss < thresholds.prediction_loss,
        commutation_error: metrics.commutation_error < thresholds.commutation_error,
        conflict_increase: metrics.conflict_increase < thresholds.conflict_increase,
    };

    Ok(QValidationResult {
        candidate_id: candidate.id.clone(),
        status: "candidate".to_string(),
        metrics,
        criteria,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(status: &str) -> AbstractionCandidate {
        AbstractionCandidate {
            id: "qcan_001".to_string(),
            input_concept_ids: vec![
                "representation.fourier.enabled".to_string(),
                "representation.siren.enabled".to_string(),
                "representation.wavelet.enabled".to_string(),
            ],
            proposed_concept_id: "representation.spectral_enrichment.enabled".to_string(),
            rationale_evidence_refs: vec![],
            status: status.to_string(),
        }
    }

    #[test]
    fn compression_metric_rewards_collapse() {
        assert_eq!(compression_metric(0), 0.0);
        assert_eq!(compression_metric(1), 0.0);
        assert_eq!(compression_metric(3), 2.0 / 3.0);
    }

    #[test]
    fn candidate_stays_candidate_with_stored_metrics() {
        let result = validate_q(
            &candidate("candidate"),
            QCandidateMetrics {
                compression: 0.8,
                ..QCandidateMetrics::default()
            },
            &QThresholds::default(),
        )
        .unwrap();
        assert_eq!(result.status, "candidate");
        assert!(result.criteria.compression);
        assert_eq!(result.metrics.compression, 0.8);
    }

    #[test]
    fn non_candidate_status_is_q_001() {
        let report = validate_q(
            &candidate("validated"),
            QCandidateMetrics::default(),
            &QThresholds::default(),
        )
        .unwrap_err();
        assert!(report.errors.iter().any(|e| e.code == "Q-001"));
    }
}
