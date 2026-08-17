//! Identifiability engine (handoff-spec.md §50, §51; mvp-architecture Phase 5).
//!
//! The graph engine — never the LLM — decides which effects are identifiable
//! (V3-17). For a joint intervention over `n` Boolean dimensions targeting an
//! outcome:
//!
//! - the **joint** effect is identifiable when both the baseline and proposed
//!   corners are observed;
//! - a **component** (main) effect for dimension `i` is identifiable when the
//!   one-hot control (dimension `i` flipped, others at baseline) is observed;
//! - a pairwise **interaction** is identifiable when all four corners of its
//!   `2×2` sub-matrix are observed;
//! - every unobserved intermediate configuration is reported as a required
//!   missing control.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::ir::{
    ComponentEffect, EffectStatus, EffectStatusEntry, Identifiability, InteractionEntry,
    MissingControl,
};
use crate::matcher::{enumerate_configurations, FactorialControl};

/// Deterministic key for a Boolean configuration (BTreeMap is already sorted).
pub fn config_key(assignments: &BTreeMap<String, bool>) -> String {
    assignments
        .iter()
        .map(|(concept_id, value)| format!("{concept_id}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// The engine's assessment of one joint intervention.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Assessment {
    pub joint: EffectStatus,
    pub components: Vec<ComponentEffect>,
    pub interactions: Vec<InteractionEntry>,
    pub missing_controls: Vec<FactorialControl>,
}

/// Assess identifiability for a joint intervention.
///
/// `changed_concepts` lists the changed Boolean dimensions in a deterministic
/// order. `observations` maps a [`config_key`] to an observed outcome value.
pub fn assess(
    changed_concepts: &[String],
    baseline: &BTreeMap<String, bool>,
    proposed: &BTreeMap<String, bool>,
    observations: &HashMap<String, f64>,
) -> Assessment {
    let n = changed_concepts.len();
    let configurations = enumerate_configurations(changed_concepts, baseline, proposed);

    let joint = if observations.contains_key(&config_key(baseline))
        && observations.contains_key(&config_key(proposed))
    {
        EffectStatus::Identifiable
    } else {
        EffectStatus::InsufficientEvidence
    };

    let mut components = Vec::new();
    for (index, concept) in changed_concepts.iter().enumerate() {
        let one_hot = &configurations[1usize << index];
        components.push(ComponentEffect {
            concept_id: concept.clone(),
            status: if observations.contains_key(&config_key(one_hot)) {
                EffectStatus::Identifiable
            } else {
                EffectStatus::Unresolved
            },
        });
    }

    let mut interactions = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let masks = [0usize, 1 << i, 1 << j, (1 << i) | (1 << j)];
            let all_observed = masks
                .iter()
                .all(|mask| observations.contains_key(&config_key(&configurations[*mask])));
            interactions.push(InteractionEntry {
                concept_refs: vec![changed_concepts[i].clone(), changed_concepts[j].clone()],
                status: if all_observed {
                    EffectStatus::Identifiable
                } else {
                    EffectStatus::Unresolved
                },
            });
        }
    }

    let mut missing_controls = Vec::new();
    for (mask, configuration) in configurations.iter().enumerate() {
        if mask == 0 || mask == (1usize << n) - 1 {
            continue;
        }
        if !observations.contains_key(&config_key(configuration)) {
            missing_controls.push(FactorialControl {
                assignments: configuration.clone(),
            });
        }
    }

    Assessment {
        joint,
        components,
        interactions,
        missing_controls,
    }
}

/// Wrap an [`Assessment`] into the compiled-IR [`Identifiability`] record.
pub fn build_identifiability(
    id: &str,
    target_ref: &str,
    intervention_ref: &str,
    assessment: &Assessment,
) -> Identifiability {
    Identifiability {
        id: id.to_string(),
        target_ref: target_ref.to_string(),
        intervention_ref: intervention_ref.to_string(),
        joint_effect: EffectStatusEntry {
            status: assessment.joint,
        },
        component_effects: assessment.components.clone(),
        interactions: assessment.interactions.clone(),
        missing_controls: assessment
            .missing_controls
            .iter()
            .map(|control| MissingControl {
                configuration: json!(control.assignments),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline() -> BTreeMap<String, bool> {
        BTreeMap::from([("r".to_string(), false), ("f".to_string(), false)])
    }

    fn proposed() -> BTreeMap<String, bool> {
        BTreeMap::from([("r".to_string(), true), ("f".to_string(), true)])
    }

    #[test]
    fn joint_only_observation_leaves_components_and_interaction_unresolved() {
        let concepts = vec!["r".to_string(), "f".to_string()];
        let observations = HashMap::from([
            (config_key(&baseline()), 0.11),
            (config_key(&proposed()), 0.021),
        ]);

        let assessment = assess(&concepts, &baseline(), &proposed(), &observations);

        assert_eq!(assessment.joint, EffectStatus::Identifiable);
        assert_eq!(assessment.components.len(), 2);
        assert!(assessment.components.iter().all(|c| c.status == EffectStatus::Unresolved));
        assert_eq!(assessment.interactions.len(), 1);
        assert_eq!(assessment.interactions[0].status, EffectStatus::Unresolved);
        // Two one-hot controls are missing: r=f? no — (r1,f0) and (r0,f1).
        assert_eq!(assessment.missing_controls.len(), 2);
    }

    #[test]
    fn full_factorial_observation_identifies_everything() {
        let concepts = vec!["r".to_string(), "f".to_string()];
        let base = baseline();
        let prop = proposed();
        let mut observations = HashMap::new();
        // All four corners observed.
        for configuration in enumerate_configurations(&concepts, &base, &prop) {
            observations.insert(config_key(&configuration), 0.05);
        }

        let assessment = assess(&concepts, &base, &prop, &observations);

        assert_eq!(assessment.joint, EffectStatus::Identifiable);
        assert!(assessment.components.iter().all(|c| c.status == EffectStatus::Identifiable));
        assert_eq!(assessment.interactions[0].status, EffectStatus::Identifiable);
        assert!(assessment.missing_controls.is_empty());
    }

    #[test]
    fn no_observation_is_insufficient_evidence() {
        let concepts = vec!["r".to_string()];
        let assessment = assess(&concepts, &baseline(), &proposed(), &HashMap::new());
        assert_eq!(assessment.joint, EffectStatus::InsufficientEvidence);
        assert_eq!(assessment.components[0].status, EffectStatus::Unresolved);
    }

    #[test]
    fn build_identifiability_wraps_into_ir_record() {
        let concepts = vec!["r".to_string(), "f".to_string()];
        let observations = HashMap::from([
            (config_key(&baseline()), 0.11),
            (config_key(&proposed()), 0.021),
        ]);
        let assessment = assess(&concepts, &baseline(), &proposed(), &observations);
        let record = build_identifiability("idn_1", "block_result", "op_I_1", &assessment);
        assert_eq!(record.id, "idn_1");
        assert_eq!(record.target_ref, "block_result");
        assert_eq!(record.intervention_ref, "op_I_1");
        assert_eq!(record.missing_controls.len(), 2);
    }
}
