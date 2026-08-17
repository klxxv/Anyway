//! Compiler StateDiff (handoff-spec.md §25).
//!
//! The compiler compares two sparse states `X0 -> X1`:
//!
//! - Bool:       Δb ∈ {-1, 0, +1}
//! - Number:     Δx = x1 - x0
//! - Expression: Δe = (e0, e1)
//!
//! A dimension present in only one state is **unknown** on the other side and
//! therefore never produces a confirmed intervention dimension (§25). It is
//! reported separately as [`UnconfirmedDimension`] for diagnostics.

use serde::{Deserialize, Serialize};

use crate::state::{CompilerState, StateValue};

/// Which side of a comparison established a dimension.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    From,
    To,
}

/// A confirmed Bool delta (handoff-spec.md §25).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BoolDiff {
    pub concept_id: String,
    pub from: bool,
    pub to: bool,
    /// `-1`, `0`, or `+1`.
    pub delta: i8,
}

/// A confirmed Number delta (handoff-spec.md §25).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct NumberDiff {
    pub concept_id: String,
    pub from: f64,
    pub to: f64,
    /// `to - from` in canonical units.
    pub delta: f64,
}

/// A confirmed Expression delta (handoff-spec.md §25).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExpressionDiff {
    pub concept_id: String,
    /// `e0` (normalized).
    pub from: String,
    /// `e1` (normalized).
    pub to: String,
    pub changed: bool,
}

/// A dimension observed on exactly one side; its other side is unknown.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UnconfirmedDimension {
    pub concept_id: String,
    pub side: Side,
    pub value: StateValue,
}

/// A dimension whose primitive type differs across the two states.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct TypeConflict {
    pub concept_id: String,
    pub from_type: String,
    pub to_type: String,
}

/// The complete comparison of two sparse states.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct StateDiff {
    pub from_id: String,
    pub to_id: String,
    #[serde(default)]
    pub bool_diffs: Vec<BoolDiff>,
    #[serde(default)]
    pub number_diffs: Vec<NumberDiff>,
    #[serde(default)]
    pub expression_diffs: Vec<ExpressionDiff>,
    #[serde(default)]
    pub unconfirmed_dimensions: Vec<UnconfirmedDimension>,
    #[serde(default)]
    pub type_conflicts: Vec<TypeConflict>,
}

impl StateDiff {
    /// `true` when the two states are identical on every confirmed dimension
    /// and no dimension is unconfirmed or conflicting.
    pub fn is_empty(&self) -> bool {
        self.bool_diffs.iter().all(|d| d.delta == 0)
            && self.number_diffs.iter().all(|d| d.delta == 0.0)
            && self.expression_diffs.iter().all(|d| !d.changed)
            && self.unconfirmed_dimensions.is_empty()
            && self.type_conflicts.is_empty()
    }

    /// Confirmed dimensions that actually changed (delta != 0 / changed).
    /// These are the candidates for the joint intervention (Step 7).
    pub fn changed_dimensions(&self) -> Vec<&str> {
        let mut ids: Vec<&str> = Vec::new();
        ids.extend(self.bool_diffs.iter().filter(|d| d.delta != 0).map(|d| d.concept_id.as_str()));
        ids.extend(self.number_diffs.iter().filter(|d| d.delta != 0.0).map(|d| d.concept_id.as_str()));
        ids.extend(self.expression_diffs.iter().filter(|d| d.changed).map(|d| d.concept_id.as_str()));
        ids
    }
}

fn value_kind(value: &StateValue) -> &'static str {
    match value {
        StateValue::Bool(_) => "bool",
        StateValue::Number(_) => "number",
        StateValue::Expression(_) => "expression",
    }
}

/// Compare two resolved states (handoff-spec.md §25).
///
/// Iteration order is deterministic: dimensions are visited in the sorted
/// union of both states' canonical concept ids.
pub fn diff_states(from: &CompilerState, to: &CompilerState) -> StateDiff {
    let mut keys: Vec<&str> = from.concept_ids().chain(to.concept_ids()).collect();
    keys.sort_unstable();
    keys.dedup();

    let mut diff = StateDiff {
        from_id: from.id.clone(),
        to_id: to.id.clone(),
        ..StateDiff::default()
    };

    for concept_id in keys {
        match (from.get(concept_id), to.get(concept_id)) {
            (Some(a), Some(b)) => match (&a.value, &b.value) {
                (StateValue::Bool(from_value), StateValue::Bool(to_value)) => {
                    let delta = match (from_value, to_value) {
                        (false, true) => 1,
                        (true, false) => -1,
                        (false, false) | (true, true) => 0,
                    };
                    diff.bool_diffs.push(BoolDiff {
                        concept_id: concept_id.to_string(),
                        from: *from_value,
                        to: *to_value,
                        delta,
                    });
                }
                (StateValue::Number(from_value), StateValue::Number(to_value)) => {
                    diff.number_diffs.push(NumberDiff {
                        concept_id: concept_id.to_string(),
                        from: *from_value,
                        to: *to_value,
                        delta: *to_value - *from_value,
                    });
                }
                (StateValue::Expression(from_value), StateValue::Expression(to_value)) => {
                    diff.expression_diffs.push(ExpressionDiff {
                        concept_id: concept_id.to_string(),
                        from: from_value.clone(),
                        to: to_value.clone(),
                        changed: from_value != to_value,
                    });
                }
                _ => {
                    diff.type_conflicts.push(TypeConflict {
                        concept_id: concept_id.to_string(),
                        from_type: value_kind(&a.value).to_string(),
                        to_type: value_kind(&b.value).to_string(),
                    });
                }
            },
            (Some(a), None) => {
                diff.unconfirmed_dimensions.push(UnconfirmedDimension {
                    concept_id: concept_id.to_string(),
                    side: Side::From,
                    value: a.value.clone(),
                });
            }
            (None, Some(b)) => {
                diff.unconfirmed_dimensions.push(UnconfirmedDimension {
                    concept_id: concept_id.to_string(),
                    side: Side::To,
                    value: b.value.clone(),
                });
            }
            (None, None) => unreachable!("key came from the union of both maps"),
        }
    }

    diff
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::StateEntry;

    fn entry(concept_id: &str, value: StateValue) -> (String, StateEntry) {
        (
            concept_id.to_string(),
            StateEntry {
                raw_concept: concept_id.to_string(),
                canonical_concept_id: concept_id.to_string(),
                raw_value: serde_json::Value::Null,
                unit_raw: None,
                expression_raw: None,
                value,
            },
        )
    }

    fn state(id: &str, entries: Vec<(String, StateEntry)>) -> CompilerState {
        CompilerState {
            id: id.to_string(),
            role: None,
            entries: entries.into_iter().collect(),
            result_refs: vec![],
            evidence_refs: vec![],
        }
    }

    #[test]
    fn bool_delta_is_minus_one_zero_or_plus_one() {
        let from = state(
            "s0",
            vec![entry("representation.fourier.enabled", StateValue::Bool(false))],
        );
        let to = state(
            "s1",
            vec![entry("representation.fourier.enabled", StateValue::Bool(true))],
        );
        let diff = diff_states(&from, &to);
        assert_eq!(diff.bool_diffs.len(), 1);
        assert_eq!(diff.bool_diffs[0].delta, 1);

        let reverse = diff_states(&to, &from);
        assert_eq!(reverse.bool_diffs[0].delta, -1);

        let same = diff_states(&from, &from);
        assert_eq!(same.bool_diffs[0].delta, 0);
        assert!(same.is_empty());
    }

    #[test]
    fn number_delta_is_to_minus_from() {
        let from = state("s0", vec![entry("result.relative_l2_error", StateValue::Number(0.11))]);
        let to = state("s1", vec![entry("result.relative_l2_error", StateValue::Number(0.021))]);
        let diff = diff_states(&from, &to);
        assert_eq!(diff.number_diffs.len(), 1);
        // Deterministic: delta is exactly `to - from` in f64 (0.021 - 0.11 = -0.089).
        assert_eq!(diff.number_diffs[0].delta, 0.021 - 0.11);
        assert!((diff.number_diffs[0].delta - (-0.089)).abs() < 1e-9);
        assert_eq!(diff.number_diffs[0].from, 0.11);
        assert_eq!(diff.number_diffs[0].to, 0.021);
    }

    #[test]
    fn expression_delta_is_a_pair() {
        let from = state(
            "s0",
            vec![entry("residual.expression", StateValue::Expression("u_t + u_x".to_string()))],
        );
        let to = state(
            "s1",
            vec![entry("residual.expression", StateValue::Expression("u_t + u_x + u_xx".to_string()))],
        );
        let diff = diff_states(&from, &to);
        assert_eq!(diff.expression_diffs.len(), 1);
        assert_eq!(diff.expression_diffs[0].from, "u_t + u_x");
        assert_eq!(diff.expression_diffs[0].to, "u_t + u_x + u_xx");
        assert!(diff.expression_diffs[0].changed);
    }

    #[test]
    fn unknown_dimension_is_not_a_confirmed_intervention() {
        let from = state("s0", vec![entry("representation.fourier.enabled", StateValue::Bool(false))]);
        let to = state(
            "s1",
            vec![
                entry("representation.fourier.enabled", StateValue::Bool(true)),
                entry("loss.dynamic.enabled", StateValue::Bool(true)),
            ],
        );
        let diff = diff_states(&from, &to);
        // Only the dimension present on both sides is confirmed…
        assert_eq!(diff.bool_diffs.len(), 1);
        assert_eq!(diff.bool_diffs[0].concept_id, "representation.fourier.enabled");
        // …and the one-sided dimension is unconfirmed, never a delta.
        assert_eq!(diff.unconfirmed_dimensions.len(), 1);
        assert_eq!(diff.unconfirmed_dimensions[0].concept_id, "loss.dynamic.enabled");
        assert_eq!(diff.unconfirmed_dimensions[0].side, Side::To);
        assert_eq!(diff.changed_dimensions(), vec!["representation.fourier.enabled"]);
    }

    #[test]
    fn type_conflict_is_reported_not_crashed() {
        let from = state("s0", vec![entry("optimizer.learning_rate", StateValue::Number(1e-3))]);
        let to = state("s1", vec![entry("optimizer.learning_rate", StateValue::Bool(true))]);
        let diff = diff_states(&from, &to);
        assert_eq!(diff.type_conflicts.len(), 1);
        assert_eq!(diff.type_conflicts[0].concept_id, "optimizer.learning_rate");
        assert_eq!(diff.type_conflicts[0].from_type, "number");
        assert_eq!(diff.type_conflicts[0].to_type, "bool");
    }
}
