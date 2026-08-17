//! Bundle grouping (handoff-spec.md §49).
//!
//! A bundle groups fibers that target the same outcome concept(s) but vary
//! across context (their conditioning differs). Conceptually
//! `B = ⊔_x F_x`: the disjoint union of fibers over conditioning variation.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::hash::hash_json;
use crate::ir::{Bundle, Fiber};

/// A fiber's outcome target under one analytical projection.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FiberTarget {
    pub fiber_id: String,
    pub target_concepts: Vec<String>,
}

/// Group fibers into bundles by target-concept signature.
pub fn group_bundles(fiber_targets: &[FiberTarget], fibers: &[Fiber]) -> Vec<Bundle> {
    let varying_of: HashMap<&str, &[String]> = fibers
        .iter()
        .map(|fiber| (fiber.id.as_str(), fiber.varying_concepts.as_slice()))
        .collect();

    type Group = (Vec<String>, BTreeSet<String>, BTreeSet<String>);
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();

    for target in fiber_targets {
        let mut concepts = target.target_concepts.clone();
        concepts.sort();
        concepts.dedup();
        let signature = concepts.join("|");

        let group = groups
            .entry(signature)
            .or_insert_with(|| (concepts, BTreeSet::new(), BTreeSet::new()));
        group.1.insert(target.fiber_id.clone());
        if let Some(varying) = varying_of.get(target.fiber_id.as_str()) {
            for concept in varying.iter().cloned() {
                group.2.insert(concept);
            }
        }
    }

    let mut bundles = Vec::new();
    let mut index = 0usize;
    for (_, (target_concepts, fiber_refs, varying_dimensions)) in groups {
        let fiber_refs: Vec<String> = fiber_refs.into_iter().collect();
        let varying_dimensions: Vec<String> = varying_dimensions.into_iter().collect();

        index += 1;
        let semantic_hash = hash_json(&json!({
            "target_concepts": target_concepts,
            "varying_dimensions": varying_dimensions,
        }));

        bundles.push(Bundle {
            id: format!("bundle_{index:03}"),
            target_concepts,
            fiber_refs,
            varying_dimensions,
            semantic_hash,
        });
    }

    bundles
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fiber(id: &str, varying: &[&str]) -> Fiber {
        Fiber {
            id: id.to_string(),
            conditioning: vec![],
            varying_concepts: varying.iter().map(|s| s.to_string()).collect(),
            chain_refs: vec![],
            semantic_hash: format!("sha256:{id}"),
        }
    }

    #[test]
    fn fibers_with_same_target_join_one_bundle() {
        let fibers = vec![
            fiber("fiber_001", &["representation.fourier.enabled"]),
            fiber("fiber_002", &["residual.adaptive.enabled"]),
        ];
        let targets = vec![
            FiberTarget {
                fiber_id: "fiber_001".to_string(),
                target_concepts: vec!["result.relative_l2_error".to_string()],
            },
            FiberTarget {
                fiber_id: "fiber_002".to_string(),
                target_concepts: vec!["result.relative_l2_error".to_string()],
            },
        ];
        let bundles = group_bundles(&targets, &fibers);
        assert_eq!(bundles.len(), 1);
        assert_eq!(bundles[0].target_concepts, vec!["result.relative_l2_error"]);
        assert_eq!(bundles[0].fiber_refs, vec!["fiber_001", "fiber_002"]);
        assert_eq!(
            bundles[0].varying_dimensions,
            vec!["representation.fourier.enabled", "residual.adaptive.enabled"]
        );
    }

    #[test]
    fn different_targets_yield_separate_bundles() {
        let fibers = vec![fiber("fiber_001", &[]), fiber("fiber_002", &[])];
        let targets = vec![
            FiberTarget {
                fiber_id: "fiber_001".to_string(),
                target_concepts: vec!["result.l2_error".to_string()],
            },
            FiberTarget {
                fiber_id: "fiber_002".to_string(),
                target_concepts: vec!["result.training_time".to_string()],
            },
        ];
        assert_eq!(group_bundles(&targets, &fibers).len(), 2);
    }
}
