//! Historical neighbor matcher (handoff-spec.md §52, §53).
//!
//! Given a new sparse state, rank historical states by sparse distance:
//!
//! - Bool:       `d_B = Σ 1[x_i ≠ y_i]` over dimensions both states establish;
//! - Number:     `d_N = Σ w_j |x_j - y_j|` (canonical units);
//! - Expression: four-level priority (exact → structural → symbolic →
//!   candidate); only exact/structural count as a match, candidate similarity
//!   is retrieval-only.
//!
//! The matcher also generates the missing factorial controls for a joint
//! intervention (§53): every `2^n - 2` intermediate Boolean configuration
//! between the baseline and proposed states.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::state::{CompilerState, StateValue};
use crate::state_diff::diff_states;

/// Expression match level, in priority order (handoff-spec.md §52).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExpressionMatchLevel {
    NoMatch,
    CandidateSimilar,
    Symbolic,
    Structural,
    Exact,
}

/// Whether a single expression comparison is a structural match.
pub fn match_expression(a: &str, b: &str) -> ExpressionMatchLevel {
    if a == b {
        return ExpressionMatchLevel::Exact;
    }
    // Level 2: commutative structural signature — token bags with numeric
    // literals folded to a placeholder are equal (e.g. `u + v` vs `v + u`).
    if structural_tokens(a) == structural_tokens(b) {
        return ExpressionMatchLevel::Structural;
    }
    // Level 4: overlap ratio as a retrieval-only candidate signal.
    if token_overlap(a, b) >= 0.5 {
        return ExpressionMatchLevel::CandidateSimilar;
    }
    ExpressionMatchLevel::NoMatch
}

/// Sparse distance between two canonical states.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct StateDistance {
    pub bool_distance: u32,
    pub number_distance: f64,
    pub expression_mismatches: u32,
}

impl StateDistance {
    /// A combined scalar for ranking (bool mismatches + weighted numeric
    /// distance + expression mismatches).
    pub fn total(&self) -> f64 {
        self.bool_distance as f64 + self.number_distance + self.expression_mismatches as f64
    }
}

/// Boolean distance (handoff-spec.md §52). Only dimensions both states
/// establish are compared; an absent dimension is unknown, not `false`.
pub fn bool_distance(query: &CompilerState, candidate: &CompilerState) -> u32 {
    let mut distance = 0u32;
    for (concept_id, query_entry) in &query.entries {
        if let Some(candidate_entry) = candidate.get(concept_id) {
            if let (StateValue::Bool(a), StateValue::Bool(b)) =
                (&query_entry.value, &candidate_entry.value)
            {
                if a != b {
                    distance += 1;
                }
            }
        }
    }
    distance
}

/// Weighted numerical distance (handoff-spec.md §52). Missing weights default
/// to `1.0`.
pub fn number_distance(
    query: &CompilerState,
    candidate: &CompilerState,
    weights: &HashMap<String, f64>,
) -> f64 {
    let mut distance = 0.0;
    for (concept_id, query_entry) in &query.entries {
        if let Some(candidate_entry) = candidate.get(concept_id) {
            if let (StateValue::Number(a), StateValue::Number(b)) =
                (&query_entry.value, &candidate_entry.value)
            {
                let weight = weights.get(concept_id).copied().unwrap_or(1.0);
                distance += weight * (a - b).abs();
            }
        }
    }
    distance
}

/// Count expression dimensions that are not an exact/structural match.
pub fn expression_mismatches(query: &CompilerState, candidate: &CompilerState) -> u32 {
    let mut mismatches = 0u32;
    for (concept_id, query_entry) in &query.entries {
        if let Some(candidate_entry) = candidate.get(concept_id) {
            if let (StateValue::Expression(a), StateValue::Expression(b)) =
                (&query_entry.value, &candidate_entry.value)
            {
                match match_expression(a, b) {
                    ExpressionMatchLevel::Exact | ExpressionMatchLevel::Structural => {}
                    _ => mismatches += 1,
                }
            }
        }
    }
    mismatches
}

/// Combined sparse distance.
pub fn state_distance(
    query: &CompilerState,
    candidate: &CompilerState,
    weights: &HashMap<String, f64>,
) -> StateDistance {
    StateDistance {
        bool_distance: bool_distance(query, candidate),
        number_distance: number_distance(query, candidate, weights),
        expression_mismatches: expression_mismatches(query, candidate),
    }
}

/// A ranked historical neighbor.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MatchResult {
    pub state_id: String,
    pub distance: StateDistance,
}

/// Rank candidate states by ascending sparse distance (handoff-spec.md §52).
pub fn rank_neighbors<'a>(
    query: &CompilerState,
    candidates: &'a [CompilerState],
    weights: &HashMap<String, f64>,
) -> Vec<MatchResult> {
    let mut results: Vec<MatchResult> = candidates
        .iter()
        .map(|candidate| MatchResult {
            state_id: candidate.id.clone(),
            distance: state_distance(query, candidate, weights),
        })
        .collect();
    results.sort_by(|a, b| {
        a.distance
            .total()
            .partial_cmp(&b.distance.total())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.state_id.cmp(&b.state_id))
    });
    results
}

/// One missing factorial control: a sparse Boolean assignment.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FactorialControl {
    /// Canonical concept id → Boolean value.
    pub assignments: BTreeMap<String, bool>,
}

/// Enumerate every Boolean configuration over the changed dimensions, in
/// mask order (`0` = all baseline, `2^n - 1` = all proposed).
pub fn enumerate_configurations(
    changed_concepts: &[String],
    baseline: &BTreeMap<String, bool>,
    proposed: &BTreeMap<String, bool>,
) -> Vec<BTreeMap<String, bool>> {
    let n = changed_concepts.len();
    let mut configurations = Vec::with_capacity(1usize << n);
    for mask in 0usize..(1usize << n) {
        let mut configuration = BTreeMap::new();
        for (index, concept) in changed_concepts.iter().enumerate() {
            let take_proposed = (mask >> index) & 1 == 1;
            let value = if take_proposed {
                proposed.get(concept).copied().unwrap_or(false)
            } else {
                baseline.get(concept).copied().unwrap_or(false)
            };
            configuration.insert(concept.clone(), value);
        }
        configurations.push(configuration);
    }
    configurations
}

/// Generate the missing factorial controls for a joint intervention
/// (handoff-spec.md §53). Returns every `2^n - 2` intermediate Boolean
/// configuration over the changed Boolean dimensions (excluding the pure
/// baseline and pure proposed corners).
pub fn missing_factorial_controls(
    from: &CompilerState,
    to: &CompilerState,
) -> Vec<FactorialControl> {
    let diff = diff_states(from, to);
    let changed: Vec<&crate::state_diff::BoolDiff> = diff
        .bool_diffs
        .iter()
        .filter(|bool_diff| bool_diff.delta != 0)
        .collect();

    let concepts: Vec<String> = changed.iter().map(|d| d.concept_id.clone()).collect();
    let baseline: BTreeMap<String, bool> = changed
        .iter()
        .map(|d| (d.concept_id.clone(), d.from))
        .collect();
    let proposed: BTreeMap<String, bool> = changed
        .iter()
        .map(|d| (d.concept_id.clone(), d.to))
        .collect();

    let n = concepts.len();
    enumerate_configurations(&concepts, &baseline, &proposed)
        .into_iter()
        .enumerate()
        .filter(|(mask, _)| *mask != 0 && *mask != (1usize << n) - 1)
        .map(|(_, assignments)| FactorialControl { assignments })
        .collect()
}

fn structural_tokens(expression: &str) -> BTreeMap<String, usize> {
    let mut tokens: BTreeMap<String, usize> = BTreeMap::new();
    for token in expression.split_whitespace() {
        let key = token
            .chars()
            .map(|c| if c.is_ascii_digit() || c == '.' { 'N' } else { c })
            .collect::<String>();
        *tokens.entry(key).or_insert(0) += 1;
    }
    tokens
}

fn token_overlap(a: &str, b: &str) -> f64 {
    let set_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let set_b: std::collections::HashSet<&str> = b.split_whitespace().collect();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        return 0.0;
    }
    set_a.intersection(&set_b).count() as f64 / union as f64
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
    fn bool_distance_counts_only_differing_overlap() {
        let query = state(
            "q",
            vec![
                entry("a", StateValue::Bool(true)),
                entry("b", StateValue::Bool(false)),
                entry("only_query", StateValue::Bool(true)),
            ],
        );
        let candidate = state(
            "c",
            vec![
                entry("a", StateValue::Bool(false)),
                entry("b", StateValue::Bool(false)),
                entry("only_candidate", StateValue::Bool(true)),
            ],
        );
        assert_eq!(bool_distance(&query, &candidate), 1);
    }

    #[test]
    fn number_distance_is_weighted_absolute_difference() {
        let query = state("q", vec![entry("lr", StateValue::Number(1e-3))]);
        let candidate = state("c", vec![entry("lr", StateValue::Number(2e-3))]);
        let mut weights = HashMap::new();
        weights.insert("lr".to_string(), 1000.0);
        assert_eq!(number_distance(&query, &candidate, &weights), 1.0);
    }

    #[test]
    fn expression_matching_prefers_exact_then_structural_then_candidate() {
        assert_eq!(match_expression("u + v", "u + v"), ExpressionMatchLevel::Exact);
        assert_eq!(match_expression("u + v", "v + u"), ExpressionMatchLevel::Structural);
        assert_eq!(
            match_expression("u + v + w", "u + v + x"),
            ExpressionMatchLevel::CandidateSimilar
        );
        assert_eq!(match_expression("u + v", "x + y"), ExpressionMatchLevel::NoMatch);
    }

    #[test]
    fn rank_neighbors_sorts_by_ascending_distance() {
        let query = state("q", vec![entry("a", StateValue::Bool(true))]);
        let near = state("near", vec![entry("a", StateValue::Bool(true))]);
        let far = state("far", vec![entry("a", StateValue::Bool(false))]);
        let ranked = rank_neighbors(&query, &[far.clone(), near.clone()], &HashMap::new());
        assert_eq!(ranked[0].state_id, "near");
        assert_eq!(ranked[1].state_id, "far");
        assert_eq!(ranked[0].distance.bool_distance, 0);
        assert_eq!(ranked[1].distance.bool_distance, 1);
    }

    #[test]
    fn three_boolean_controls_yield_six_missing_factorial_states() {
        let from = state(
            "base",
            vec![
                entry("r", StateValue::Bool(false)),
                entry("f", StateValue::Bool(false)),
                entry("w", StateValue::Bool(false)),
            ],
        );
        let to = state(
            "prop",
            vec![
                entry("r", StateValue::Bool(true)),
                entry("f", StateValue::Bool(true)),
                entry("w", StateValue::Bool(true)),
            ],
        );
        let controls = missing_factorial_controls(&from, &to);
        assert_eq!(controls.len(), 6);
        // Each control must differ from both corners on at least one dimension.
        for control in &controls {
            assert!(!control.assignments.values().all(|v| !v));
            assert!(!control.assignments.values().all(|v| *v));
        }
    }
}
