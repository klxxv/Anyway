//! Fiber grouping (handoff-spec.md §46, §47, §48).
//!
//! A fiber is a collection of chains that share conditioning (held-fixed
//! concept → value pairs) and vary only within an allowed set of concepts.
//! Grouping is query-dependent and many-to-many: a chain participates in a
//! fiber for every analytical projection that covers it (handoff-spec.md §48).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::hash::hash_json;
use crate::ir::{ConditioningEntry, Fiber};

/// One chain's conditioning and varying dimensions under a single analytical
/// projection.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ChainProjection {
    pub chain_id: String,
    pub conditioning: Vec<ConditioningEntry>,
    pub varying_concepts: Vec<String>,
}

/// Deterministic signature of a conditioning set (order-independent).
pub fn conditioning_signature(conditioning: &[ConditioningEntry]) -> String {
    let mut entries: Vec<(&str, &str)> = conditioning
        .iter()
        .map(|entry| (entry.concept_id.as_str(), entry.semantic_value_hash.as_str()))
        .collect();
    entries.sort_unstable();
    entries.dedup();
    entries
        .iter()
        .map(|(concept_id, hash)| format!("{concept_id}={hash}"))
        .collect::<Vec<_>>()
        .join("|")
}

/// Group chain projections into fibers by conditioning signature.
///
/// Chains with identical conditioning join one fiber; the fiber's varying
/// dimensions are the sorted union of its members' varying concepts.
pub fn group_fibers(projections: &[ChainProjection]) -> Vec<Fiber> {
    type Group = (Vec<ConditioningEntry>, BTreeSet<String>, BTreeSet<String>);
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();

    for projection in projections {
        let signature = conditioning_signature(&projection.conditioning);
        let group = groups
            .entry(signature)
            .or_insert_with(|| (projection.conditioning.clone(), BTreeSet::new(), BTreeSet::new()));
        group.1.insert(projection.chain_id.clone());
        for varying in &projection.varying_concepts {
            group.2.insert(varying.clone());
        }
    }

    let mut fibers = Vec::new();
    let mut index = 0usize;
    for (_, (mut conditioning, chain_refs, varying)) in groups {
        conditioning.sort_by(|a, b| a.concept_id.cmp(&b.concept_id));
        conditioning.dedup_by(|a, b| a.concept_id == b.concept_id);
        let varying_concepts: Vec<String> = varying.into_iter().collect();
        let chain_refs: Vec<String> = chain_refs.into_iter().collect();

        index += 1;
        let semantic_hash = hash_json(&serde_json::json!({
            "conditioning": conditioning,
            "varying_concepts": varying_concepts,
        }));

        fibers.push(Fiber {
            id: format!("fiber_{index:03}"),
            conditioning,
            varying_concepts,
            chain_refs,
            semantic_hash,
        });
    }

    fibers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(concept_id: &str, hash: &str) -> ConditioningEntry {
        ConditioningEntry {
            concept_id: concept_id.to_string(),
            semantic_value_hash: hash.to_string(),
        }
    }

    #[test]
    fn chains_with_same_conditioning_join_one_fiber() {
        let projections = vec![
            ChainProjection {
                chain_id: "chain_001".to_string(),
                conditioning: vec![entry("optimizer.adam.enabled", "h1")],
                varying_concepts: vec!["representation.fourier.enabled".to_string()],
            },
            ChainProjection {
                chain_id: "chain_002".to_string(),
                conditioning: vec![entry("optimizer.adam.enabled", "h1")],
                varying_concepts: vec!["representation.fourier.enabled".to_string()],
            },
        ];
        let fibers = group_fibers(&projections);
        assert_eq!(fibers.len(), 1);
        assert_eq!(fibers[0].chain_refs, vec!["chain_001", "chain_002"]);
        assert_eq!(fibers[0].varying_concepts, vec!["representation.fourier.enabled"]);
    }

    #[test]
    fn different_conditioning_yields_separate_fibers() {
        let projections = vec![
            ChainProjection {
                chain_id: "chain_001".to_string(),
                conditioning: vec![entry("optimizer.adam.enabled", "h1")],
                varying_concepts: vec![],
            },
            ChainProjection {
                chain_id: "chain_002".to_string(),
                conditioning: vec![entry("optimizer.lbfgs.enabled", "h2")],
                varying_concepts: vec![],
            },
        ];
        let fibers = group_fibers(&projections);
        assert_eq!(fibers.len(), 2);
    }

    #[test]
    fn conditioning_signature_is_order_insensitive() {
        let a = vec![entry("a", "h1"), entry("b", "h2")];
        let b = vec![entry("b", "h2"), entry("a", "h1")];
        assert_eq!(conditioning_signature(&a), conditioning_signature(&b));
    }
}
