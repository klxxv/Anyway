//! Joint Intervention compiler (handoff-spec.md §26, §27).
//!
//! A multi-variable state diff compiles into **one** joint intervention
//! (V3-06). The intervention carries every confirmed changed dimension as a
//! single `changes` payload; the compiler must **not** split it into
//! independent per-dimension effects (V3-07). Independent effects require
//! additional controls and are the identifiability engine's job (Step 11).

use serde::{Deserialize, Serialize};

use crate::state::StateValue;
use crate::state_diff::StateDiff;
use crate::OperatorKind;

/// One changed dimension of a joint intervention (handoff-spec.md §26).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Change {
    pub concept_id: String,
    pub before: StateValue,
    pub after: StateValue,
}

/// The single joint intervention compiled from a state diff.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct JointIntervention {
    pub id: String,
    /// Always [`OperatorKind::I`].
    pub operator: OperatorKind,
    pub from_state: String,
    pub to_state: String,
    pub changes: Vec<Change>,
}

impl JointIntervention {
    /// Number of confirmed changed dimensions in this intervention.
    pub fn len(&self) -> usize {
        self.changes.len()
    }

    /// `true` when the intervention carries no changed dimensions.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Compile a joint intervention from a state diff (handoff-spec.md §26, §27).
///
/// Returns `None` when no confirmed dimension changed (an empty diff produces
/// no intervention). Deterministic: the id is derived from the two state ids
/// and the change order follows the diff's stable group order.
pub fn compile_intervention(diff: &StateDiff) -> Option<JointIntervention> {
    let mut changes: Vec<Change> = Vec::new();

    for bool_diff in &diff.bool_diffs {
        if bool_diff.delta != 0 {
            changes.push(Change {
                concept_id: bool_diff.concept_id.clone(),
                before: StateValue::Bool(bool_diff.from),
                after: StateValue::Bool(bool_diff.to),
            });
        }
    }

    for number_diff in &diff.number_diffs {
        if number_diff.delta != 0.0 {
            changes.push(Change {
                concept_id: number_diff.concept_id.clone(),
                before: StateValue::Number(number_diff.from),
                after: StateValue::Number(number_diff.to),
            });
        }
    }

    for expression_diff in &diff.expression_diffs {
        if expression_diff.changed {
            changes.push(Change {
                concept_id: expression_diff.concept_id.clone(),
                before: StateValue::Expression(expression_diff.from.clone()),
                after: StateValue::Expression(expression_diff.to.clone()),
            });
        }
    }

    if changes.is_empty() {
        return None;
    }

    Some(JointIntervention {
        id: format!("op_I_{}->{}", diff.from_id, diff.to_id),
        operator: OperatorKind::I,
        from_state: diff.from_id.clone(),
        to_state: diff.to_id.clone(),
        changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{CompilerState, StateEntry};

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
    fn multi_variable_diff_compiles_to_one_joint_intervention() {
        let baseline = state(
            "baseline",
            vec![
                entry("residual.strong_form.enabled", StateValue::Bool(true)),
                entry("representation.fourier.enabled", StateValue::Bool(false)),
                entry("loss.dynamic.enabled", StateValue::Bool(false)),
            ],
        );
        let proposed = state(
            "proposed",
            vec![
                entry("residual.strong_form.enabled", StateValue::Bool(false)),
                entry("representation.fourier.enabled", StateValue::Bool(true)),
                entry("loss.dynamic.enabled", StateValue::Bool(true)),
            ],
        );

        let diff = crate::state_diff::diff_states(&baseline, &proposed);
        let intervention = compile_intervention(&diff).expect("three changed dims");

        assert_eq!(intervention.operator, OperatorKind::I);
        assert_eq!(intervention.len(), 3);
        // One joint intervention, not three independent ones (V3-06).
        assert_eq!(
            intervention.changes.iter().map(|c| c.concept_id.as_str()).collect::<Vec<_>>(),
            vec![
                "loss.dynamic.enabled",
                "representation.fourier.enabled",
                "residual.strong_form.enabled",
            ]
        );
    }

    #[test]
    fn unchanged_diff_compiles_to_no_intervention() {
        let baseline = state("s", vec![entry("loss.dynamic.enabled", StateValue::Bool(false))]);
        let diff = crate::state_diff::diff_states(&baseline, &baseline);
        assert!(compile_intervention(&diff).is_none());
    }

    #[test]
    fn number_and_expression_changes_are_carried_canonically() {
        let from = state(
            "s0",
            vec![
                entry("result.relative_l2_error", StateValue::Number(0.11)),
                entry("residual.expression", StateValue::Expression("u_t + u_x".to_string())),
            ],
        );
        let to = state(
            "s1",
            vec![
                entry("result.relative_l2_error", StateValue::Number(0.021)),
                entry("residual.expression", StateValue::Expression("u_t + u_x + u_xx".to_string())),
            ],
        );

        let diff = crate::state_diff::diff_states(&from, &to);
        let intervention = compile_intervention(&diff).expect("two changed dims");
        assert_eq!(intervention.len(), 2);
        assert!(intervention.changes.iter().any(|c| {
            c.concept_id == "result.relative_l2_error"
                && c.before == StateValue::Number(0.11)
                && c.after == StateValue::Number(0.021)
        }));
        assert!(intervention.changes.iter().any(|c| {
            c.concept_id == "residual.expression"
                && c.before == StateValue::Expression("u_t + u_x".to_string())
                && c.after == StateValue::Expression("u_t + u_x + u_xx".to_string())
        }));
    }

    #[test]
    fn one_sided_dimension_is_never_a_joint_change() {
        let from = state("s0", vec![entry("representation.fourier.enabled", StateValue::Bool(false))]);
        let to = state(
            "s1",
            vec![
                entry("representation.fourier.enabled", StateValue::Bool(true)),
                entry("loss.dynamic.enabled", StateValue::Bool(true)),
            ],
        );
        let diff = crate::state_diff::diff_states(&from, &to);
        let intervention = compile_intervention(&diff).expect("one confirmed changed dim");
        assert_eq!(intervention.len(), 1);
        assert_eq!(intervention.changes[0].concept_id, "representation.fourier.enabled");
    }
}
