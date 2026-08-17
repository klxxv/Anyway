/**
 * Q candidate validation (handoff-spec.md §33, §34).
 *
 * Mirrors `crates/anyway-schema-v4/src/q_validation.rs` types. The Rust crate
 * is authoritative for the evaluation; these types are the cross-boundary
 * contract only.
 */

export interface QCandidateMetrics {
  compression: number;
  prediction_loss: number;
  commutation_error: number;
  conflict_increase: number;
}

export interface QThresholds {
  compression: number;
  prediction_loss: number;
  commutation_error: number;
  conflict_increase: number;
}

export interface QAcceptanceCriteria {
  compression: boolean;
  prediction_loss: boolean;
  commutation_error: boolean;
  conflict_increase: boolean;
}

export interface QValidationResult {
  candidate_id: string;
  status: string;
  metrics: QCandidateMetrics;
  criteria: QAcceptanceCriteria;
}
