//! Deterministic compiler: ExtractionV3 → CanvasIRV3 (Steps 8–9 of 16).
//!
//! This module builds the Block and Operator IR, pins the legacy-edge →
//! operator convergence (implementation-plan.md §2.3), and fills semantic /
//! instance hashes (handoff-spec.md §39, §40). The LLM extracts semantics;
//! this compiler decides computation.
//!
//! Hard invariants enforced here: OP-001..OP-006 (handoff-spec.md §42) and
//! V3-04..V3-07.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::canonicalize::ConceptCanonicalizer;
use crate::extract::ExtractionV3;
use crate::hash::{hash_json, instance_hash};
use crate::intervention::compile_intervention;
use crate::ir::{Block, BlockType, CanvasIRV3, Operator};
use crate::state::{canonical_variable_value, resolve_state, StateValue};
use crate::state_diff::diff_states;
use crate::validator::ValidationReport;
use crate::OperatorKind;

/// Deterministic block-id prefixes. Stable across every later step.
pub fn variable_block_id(id: &str) -> String {
    format!("block_var_{id}")
}
pub fn result_block_id(id: &str) -> String {
    format!("block_result_{id}")
}
pub fn state_block_id(id: &str) -> String {
    format!("block_state_{id}")
}
pub fn concept_block_id(id: &str) -> String {
    format!("block_concept_{id}")
}
pub fn axiom_block_id(id: &str) -> String {
    format!("block_axiom_{id}")
}

/// Where a legacy canvas edge converges (implementation-plan.md §2.3).
///
/// `supports`/`contradicts` are not edges at all: they become evidence status.
/// The remaining edges collapse onto the five-operator basis. `causes` and
/// `controls` require an intervention to become an identifiable kernel.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeConvergence {
    /// → evidence status, not an edge.
    Evidence,
    /// → K. `requires_intervention` is true for causal/control edges.
    Kernel { requires_intervention: bool },
    /// → T.
    Transform,
    /// → M.
    Marginalize,
}

/// Map a legacy edge type onto the five-operator basis.
///
/// Returns `None` for an unrecognized edge type (rejected during migration).
pub fn converge_edge(edge_type: &str) -> Option<EdgeConvergence> {
    match edge_type {
        "supports" | "contradicts" => Some(EdgeConvergence::Evidence),
        "causes" | "controls" => Some(EdgeConvergence::Kernel {
            requires_intervention: true,
        }),
        "mediates" | "moderates" | "correlates" => Some(EdgeConvergence::Kernel {
            requires_intervention: false,
        }),
        "depends_on" | "derived_from" | "uses" | "measures" => Some(EdgeConvergence::Transform),
        "part_of" => Some(EdgeConvergence::Marginalize),
        _ => None,
    }
}

/// The deterministic extraction → IR compiler.
#[derive(Clone, Debug, Default)]
pub struct Compiler {
    canonicalizer: ConceptCanonicalizer,
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            canonicalizer: ConceptCanonicalizer::pinn_seed(),
        }
    }

    pub fn with_canonicalizer(canonicalizer: ConceptCanonicalizer) -> Self {
        Self { canonicalizer }
    }

    /// Compile the whole IR. Fails as a whole on any error (no partial fallback).
    pub fn compile(&self, extraction: &ExtractionV3) -> Result<CanvasIRV3, ValidationReport> {
        let mut report = ValidationReport::default();

        let (blocks, block_report) = self.compile_blocks(extraction);
        report.errors.extend(block_report.errors);
        report.warnings.extend(block_report.warnings);

        let (operators, operator_report) = self.compile_operators(extraction, &blocks);
        report.errors.extend(operator_report.errors);
        report.warnings.extend(operator_report.warnings);

        if !report.ok() {
            return Err(report);
        }

        Ok(CanvasIRV3 {
            schema_version: crate::GRAPH_IR_SCHEMA_VERSION.to_string(),
            blocks,
            operators,
            ..CanvasIRV3::default()
        })
    }

    /// Build every Block (handoff-spec.md §37, §38) with semantic and instance
    /// hashes filled (handoff-spec.md §39, §40).
    pub fn compile_blocks(&self, extraction: &ExtractionV3) -> (Vec<Block>, ValidationReport) {
        let mut report = ValidationReport::default();
        let mut blocks = Vec::new();
        let document_id = document_id(extraction);

        // Result variables become `result` blocks; every other variable is a
        // `variable` block. Classification is driven by state `result_refs`.
        let result_ids: HashSet<&str> = extraction
            .experiments
            .iter()
            .flat_map(|experiment| experiment.states.iter())
            .flat_map(|state| state.result_refs.iter())
            .map(String::as_str)
            .collect();

        let mut variable_semantic: HashMap<String, String> = HashMap::new();

        for variable in &extraction.variables {
            let Ok(value) = canonical_variable_value(variable) else {
                // The value violates its declared primitive type; report and
                // skip (compile() will reject the whole document).
                let mut value_report = ValidationReport::default();
                value_report.error(
                    "STATE-001",
                    &format!("$.variables[{}]", variable.id),
                    format!("variable {} has an invalid value", variable.id),
                );
                report.errors.extend(value_report.errors);
                continue;
            };

            let canonical = self.canonicalizer.canonicalize(&variable.concept_id);
            let (id, block_type) = if result_ids.contains(variable.id.as_str()) {
                (result_block_id(&variable.id), BlockType::Outcome)
            } else {
                (variable_block_id(&variable.id), BlockType::Variable)
            };
            let (semantic_hash, instance_hash) = block_hashes(
                &block_type,
                Some(&canonical.canonical_concept_id),
                Some(&value),
                None,
                None,
                &[],
                document_id,
                &variable.evidence_refs,
            );
            variable_semantic.insert(variable.id.clone(), semantic_hash.clone());
            blocks.push(Block {
                id,
                block_type,
                concept_id: Some(canonical.canonical_concept_id),
                variable_ref: Some(variable.id.clone()),
                member_refs: Vec::new(),
                context_ref: None,
                axiom_set_ref: None,
                semantic_hash,
                instance_hash,
            });
        }

        for experiment in &extraction.experiments {
            for state in &experiment.states {
                let member_hashes = sorted_semantic_members(&state.variable_refs, &variable_semantic);
                let (semantic_hash, instance_hash) = block_hashes(
                    &BlockType::State,
                    None,
                    None,
                    experiment.context_ref.as_deref(),
                    experiment.axiom_set_ref.as_deref(),
                    &member_hashes,
                    document_id,
                    &state.evidence_refs,
                );
                blocks.push(Block {
                    id: state_block_id(&state.id),
                    block_type: BlockType::State,
                    concept_id: None,
                    variable_ref: None,
                    member_refs: state
                        .variable_refs
                        .iter()
                        .map(|variable_ref| variable_block_id(variable_ref))
                        .collect(),
                    context_ref: experiment.context_ref.clone(),
                    axiom_set_ref: experiment.axiom_set_ref.clone(),
                    semantic_hash,
                    instance_hash,
                });
            }
        }

        for axiom_set in &extraction.axiom_sets {
            let member_hashes =
                sorted_semantic_members(&axiom_set.constraint_refs, &variable_semantic);
            let (semantic_hash, instance_hash) = block_hashes(
                &BlockType::Axiom,
                None,
                None,
                None,
                None,
                &member_hashes,
                document_id,
                &axiom_set.evidence_refs,
            );
            blocks.push(Block {
                id: axiom_block_id(&axiom_set.id),
                block_type: BlockType::Axiom,
                concept_id: None,
                variable_ref: None,
                member_refs: axiom_set
                    .constraint_refs
                    .iter()
                    .map(|constraint_ref| variable_block_id(constraint_ref))
                    .collect(),
                context_ref: None,
                axiom_set_ref: None,
                semantic_hash,
                instance_hash,
            });
        }

        for candidate in &extraction.abstraction_candidates {
            let canonical = self.canonicalizer.canonicalize(&candidate.proposed_concept_id);
            let mut member_concepts: Vec<String> = candidate.input_concept_ids.clone();
            member_concepts.sort();
            member_concepts.dedup();
            let (semantic_hash, instance_hash) = block_hashes(
                &BlockType::Concept,
                Some(&canonical.canonical_concept_id),
                None,
                None,
                None,
                &member_concepts,
                document_id,
                &candidate.rationale_evidence_refs,
            );
            blocks.push(Block {
                id: concept_block_id(&candidate.id),
                block_type: BlockType::Concept,
                concept_id: Some(canonical.canonical_concept_id),
                variable_ref: None,
                member_refs: candidate.input_concept_ids.clone(),
                context_ref: None,
                axiom_set_ref: None,
                semantic_hash,
                instance_hash,
            });
        }

        (blocks, report)
    }

    /// Build every compiled Operator (handoff-spec.md §41, §42).
    ///
    /// Sources: explicit [`extract::OperatorCandidate`] relations and
    /// StateDiff-derived joint interventions. Each is validated against
    /// OP-001..OP-006.
    pub fn compile_operators(
        &self,
        extraction: &ExtractionV3,
        blocks: &[Block],
    ) -> (Vec<Operator>, ValidationReport) {
        let mut report = ValidationReport::default();
        let mut operators = Vec::new();

        let block_ids: HashSet<&str> = blocks.iter().map(|block| block.id.as_str()).collect();
        let block_semantic: HashMap<&str, &str> = blocks
            .iter()
            .map(|block| (block.id.as_str(), block.semantic_hash.as_str()))
            .collect();
        let variable_to_block: HashMap<&str, String> = extraction
            .variables
            .iter()
            .map(|variable| (variable.id.as_str(), variable_block_id(&variable.id)))
            .collect();
        let context_ids: HashSet<&str> = extraction.contexts.iter().map(|c| c.id.as_str()).collect();
        let axiom_ids: HashSet<&str> = extraction.axiom_sets.iter().map(|a| a.id.as_str()).collect();
        let document_id = document_id(extraction);

        for candidate in &extraction.operator_candidates {
            if let Some(operator) = self.build_candidate_operator(
                candidate,
                &variable_to_block,
                &block_ids,
                &block_semantic,
                &context_ids,
                &axiom_ids,
                document_id,
                &mut report,
            ) {
                operators.push(operator);
            }
        }

        for experiment in &extraction.experiments {
            self.build_intervention_operators(
                experiment,
                extraction,
                &block_semantic,
                &context_ids,
                &axiom_ids,
                document_id,
                &mut operators,
                &mut report,
            );
        }

        (operators, report)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_candidate_operator(
        &self,
        candidate: &crate::extract::OperatorCandidate,
        variable_to_block: &HashMap<&str, String>,
        block_ids: &HashSet<&str>,
        block_semantic: &HashMap<&str, &str>,
        context_ids: &HashSet<&str>,
        axiom_ids: &HashSet<&str>,
        document_id: &str,
        report: &mut ValidationReport,
    ) -> Option<Operator> {
        let path = format!("$.operator_candidates[{}]", candidate.id);

        // OP-004: every input/output reference resolves to a block.
        let mut input_refs = Vec::new();
        for reference in &candidate.input_refs {
            match resolve_ref(reference, variable_to_block, block_ids) {
                Some(block_id) => input_refs.push(block_id),
                None => {
                    report.error(
                        "OP-004",
                        &format!("{path}.input_refs"),
                        format!("unresolved input reference: {reference}"),
                    );
                    return None;
                }
            }
        }
        let mut output_refs = Vec::new();
        for reference in &candidate.output_refs {
            match resolve_ref(reference, variable_to_block, block_ids) {
                Some(block_id) => output_refs.push(block_id),
                None => {
                    report.error(
                        "OP-004",
                        &format!("{path}.output_refs"),
                        format!("unresolved output reference: {reference}"),
                    );
                    return None;
                }
            }
        }

        // OP-005 / OP-006: conditioning references resolve when present.
        if let Some(context_ref) = &candidate.context_ref {
            if !context_ids.contains(context_ref.as_str()) {
                report.error(
                    "OP-005",
                    &format!("{path}.context_ref"),
                    format!("unresolved context reference: {context_ref}"),
                );
                return None;
            }
        }
        if let Some(axiom_set_ref) = &candidate.axiom_set_ref {
            if !axiom_ids.contains(axiom_set_ref.as_str()) {
                report.error(
                    "OP-006",
                    &format!("{path}.axiom_set_ref"),
                    format!("unresolved axiom_set reference: {axiom_set_ref}"),
                );
                return None;
            }
        }

        let (semantic_hash, instance_hash) = operator_hashes(
            candidate.operator,
            &input_refs,
            &output_refs,
            &candidate.payload,
            candidate.context_ref.as_deref(),
            candidate.axiom_set_ref.as_deref(),
            block_semantic,
            document_id,
            &candidate.evidence_refs,
        );

        Some(Operator {
            id: candidate.id.clone(),
            operator: candidate.operator,
            input_refs,
            output_refs,
            payload: candidate.payload.clone(),
            context_ref: candidate.context_ref.clone(),
            axiom_set_ref: candidate.axiom_set_ref.clone(),
            evidence_refs: candidate.evidence_refs.clone(),
            semantic_hash,
            instance_hash,
        })
    }

    fn build_intervention_operators(
        &self,
        experiment: &crate::extract::Experiment,
        extraction: &ExtractionV3,
        block_semantic: &HashMap<&str, &str>,
        context_ids: &HashSet<&str>,
        axiom_ids: &HashSet<&str>,
        document_id: &str,
        operators: &mut Vec<Operator>,
        report: &mut ValidationReport,
    ) {
        let states: HashMap<&str, &crate::extract::State> = experiment
            .states
            .iter()
            .map(|state| (state.id.as_str(), state))
            .collect();

        // OP-005 / OP-006 apply to the experiment conditioning as well.
        if let Some(context_ref) = &experiment.context_ref {
            if !context_ids.contains(context_ref.as_str()) {
                report.error(
                    "OP-005",
                    &format!("$.experiments[{}].context_ref", experiment.id),
                    format!("unresolved context reference: {context_ref}"),
                );
                return;
            }
        }
        if let Some(axiom_set_ref) = &experiment.axiom_set_ref {
            if !axiom_ids.contains(axiom_set_ref.as_str()) {
                report.error(
                    "OP-006",
                    &format!("$.experiments[{}].axiom_set_ref", experiment.id),
                    format!("unresolved axiom_set reference: {axiom_set_ref}"),
                );
                return;
            }
        }

        for comparison in &experiment.comparisons {
            let (Some(from_state), Some(to_state)) = (
                states.get(comparison.from_state.as_str()),
                states.get(comparison.to_state.as_str()),
            ) else {
                report.error(
                    "REF-001",
                    &format!("$.experiments[{}].comparisons", experiment.id),
                    format!(
                        "unresolved state comparison {} -> {}",
                        comparison.from_state, comparison.to_state
                    ),
                );
                continue;
            };

            let from = match resolve_state(from_state, extraction, &self.canonicalizer) {
                Ok(state) => state,
                Err(mut state_report) => {
                    report.errors.append(&mut state_report.errors);
                    report.warnings.append(&mut state_report.warnings);
                    continue;
                }
            };
            let to = match resolve_state(to_state, extraction, &self.canonicalizer) {
                Ok(state) => state,
                Err(mut state_report) => {
                    report.errors.append(&mut state_report.errors);
                    report.warnings.append(&mut state_report.warnings);
                    continue;
                }
            };

            let diff = diff_states(&from, &to);
            let Some(intervention) = compile_intervention(&diff) else {
                continue;
            };

            let input_refs = vec![state_block_id(&comparison.from_state)];
            let output_refs = vec![state_block_id(&comparison.to_state)];
            let payload = json!({ "changes": intervention.changes });
            let (semantic_hash, instance_hash) = operator_hashes(
                OperatorKind::I,
                &input_refs,
                &output_refs,
                &payload,
                experiment.context_ref.as_deref(),
                experiment.axiom_set_ref.as_deref(),
                block_semantic,
                document_id,
                &comparison.evidence_refs,
            );

            operators.push(Operator {
                id: intervention.id,
                operator: OperatorKind::I,
                input_refs,
                output_refs,
                payload,
                context_ref: experiment.context_ref.clone(),
                axiom_set_ref: experiment.axiom_set_ref.clone(),
                evidence_refs: comparison.evidence_refs.clone(),
                semantic_hash,
                instance_hash,
            });
        }
    }

}

/// Resolve a candidate reference to a block id. References may name a variable
/// directly, or (already) a block id.
fn resolve_ref(
    reference: &str,
    variable_to_block: &HashMap<&str, String>,
    block_ids: &HashSet<&str>,
) -> Option<String> {
    if let Some(block_id) = variable_to_block.get(reference) {
        return Some(block_id.clone());
    }
    if block_ids.contains(reference) {
        return Some(reference.to_string());
    }
    None
}

fn document_id(extraction: &ExtractionV3) -> &str {
    extraction
        .document
        .as_ref()
        .map(|document| document.document_id.as_str())
        .unwrap_or("")
}

/// Sort and deduplicate the semantic hashes of member variables.
fn sorted_semantic_members(
    variable_refs: &[String],
    variable_semantic: &HashMap<String, String>,
) -> Vec<String> {
    let mut members: Vec<String> = variable_refs
        .iter()
        .map(|variable_ref| {
            variable_semantic
                .get(variable_ref)
                .cloned()
                .unwrap_or_default()
        })
        .collect();
    members.sort();
    members.dedup();
    members
}

/// Compute `(semantic_hash, instance_hash)` for a block.
#[allow(clippy::too_many_arguments)]
fn block_hashes(
    block_type: &BlockType,
    concept_id: Option<&str>,
    value: Option<&StateValue>,
    context_ref: Option<&str>,
    axiom_set_ref: Option<&str>,
    members: &[String],
    document_id: &str,
    evidence_refs: &[String],
) -> (String, String) {
    let semantic = hash_json(&json!({
        "block_type": block_type,
        "concept_id": concept_id,
        "value": value,
        "context_ref": context_ref,
        "axiom_set_ref": axiom_set_ref,
        "members": members,
    }));
    let instance = instance_hash(&semantic, document_id, evidence_refs);
    (semantic, instance)
}

/// Compute `(semantic_hash, instance_hash)` for an operator. Input/output
/// references are folded through their block semantic hashes so the operator
/// hash is provenance-independent.
#[allow(clippy::too_many_arguments)]
fn operator_hashes(
    operator: OperatorKind,
    input_refs: &[String],
    output_refs: &[String],
    payload: &serde_json::Value,
    context_ref: Option<&str>,
    axiom_set_ref: Option<&str>,
    block_semantic: &HashMap<&str, &str>,
    document_id: &str,
    evidence_refs: &[String],
) -> (String, String) {
    let mut input_hashes: Vec<&str> = input_refs
        .iter()
        .map(|reference| block_semantic.get(reference.as_str()).copied().unwrap_or(""))
        .collect();
    input_hashes.sort_unstable();
    input_hashes.dedup();
    let mut output_hashes: Vec<&str> = output_refs
        .iter()
        .map(|reference| block_semantic.get(reference.as_str()).copied().unwrap_or(""))
        .collect();
    output_hashes.sort_unstable();
    output_hashes.dedup();

    let semantic = hash_json(&json!({
        "operator": operator,
        "input_hashes": input_hashes,
        "output_hashes": output_hashes,
        "payload": payload,
        "context_ref": context_ref,
        "axiom_set_ref": axiom_set_ref,
    }));
    let instance = instance_hash(&semantic, document_id, evidence_refs);
    (semantic, instance)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn all_twelve_legacy_edges_converge() {
        let expected = [
            ("causes", Some(EdgeConvergence::Kernel { requires_intervention: true })),
            ("correlates", Some(EdgeConvergence::Kernel { requires_intervention: false })),
            ("supports", Some(EdgeConvergence::Evidence)),
            ("contradicts", Some(EdgeConvergence::Evidence)),
            ("depends_on", Some(EdgeConvergence::Transform)),
            ("derived_from", Some(EdgeConvergence::Transform)),
            ("part_of", Some(EdgeConvergence::Marginalize)),
            ("controls", Some(EdgeConvergence::Kernel { requires_intervention: true })),
            ("mediates", Some(EdgeConvergence::Kernel { requires_intervention: false })),
            ("moderates", Some(EdgeConvergence::Kernel { requires_intervention: false })),
            ("uses", Some(EdgeConvergence::Transform)),
            ("measures", Some(EdgeConvergence::Transform)),
        ];
        for (edge, convergence) in expected {
            assert_eq!(converge_edge(edge), convergence, "edge: {edge}");
        }
        assert_eq!(converge_edge("unknown_edge"), None);
    }

    #[test]
    fn blocks_classify_variable_state_result_and_axiom() {
        let extraction: ExtractionV3 = serde_json::from_value(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "doc", "source_type": "paper" },
            "evidence": [{ "id": "ev_001", "document_id": "doc", "text_span": "t",
                          "verification": { "status": "supported", "confidence": 0.9 } }],
            "variables": [
                { "id": "v_fourier", "concept_id": "representation.fourier.enabled",
                  "value_type": "bool", "observed": true, "value": true,
                  "unit_raw": null, "expression_raw": null, "evidence_refs": ["ev_001"] },
                { "id": "v_result", "concept_id": "result.relative_l2_error",
                  "value_type": "number", "observed": true, "value": 0.021,
                  "unit_raw": null, "expression_raw": null, "evidence_refs": ["ev_001"] }
            ],
            "contexts": [],
            "axiom_sets": [{ "id": "ax_1", "constraint_refs": [], "evidence_refs": [] }],
            "experiments": [{ "id": "exp_1", "states": [{
                "id": "state_1", "role": "baseline",
                "variable_refs": ["v_fourier"], "result_refs": ["v_result"],
                "evidence_refs": ["ev_001"]
            }], "comparisons": [], "evidence_refs": [] }],
            "operator_candidates": [],
            "abstraction_candidates": []
        })).unwrap();

        let compiler = Compiler::new();
        let (blocks, report) = compiler.compile_blocks(&extraction);
        assert!(report.ok());

        let by_id: HashMap<&str, &Block> = blocks.iter().map(|b| (b.id.as_str(), b)).collect();
        assert_eq!(by_id["block_var_v_fourier"].block_type, BlockType::Variable);
        assert_eq!(by_id["block_result_v_result"].block_type, BlockType::Outcome);
        assert_eq!(by_id["block_state_state_1"].block_type, BlockType::State);
        assert_eq!(by_id["block_axiom_ax_1"].block_type, BlockType::Axiom);
        assert_eq!(
            by_id["block_var_v_fourier"].concept_id.as_deref(),
            Some("representation.fourier.enabled")
        );
        assert_eq!(by_id["block_state_state_1"].member_refs, vec!["block_var_v_fourier"]);
    }

    #[test]
    fn state_diff_compiles_to_intervention_operator() {
        let extraction: ExtractionV3 = serde_json::from_value(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "doc", "source_type": "paper" },
            "evidence": [{ "id": "ev_001", "document_id": "doc", "text_span": "t",
                          "verification": { "status": "supported", "confidence": 0.9 } }],
            "variables": [
                { "id": "v_base", "concept_id": "representation.fourier.enabled",
                  "value_type": "bool", "observed": true, "value": false,
                  "unit_raw": null, "expression_raw": null, "evidence_refs": ["ev_001"] },
                { "id": "v_prop", "concept_id": "representation.fourier.enabled",
                  "value_type": "bool", "observed": true, "value": true,
                  "unit_raw": null, "expression_raw": null, "evidence_refs": ["ev_001"] }
            ],
            "contexts": [],
            "axiom_sets": [],
            "experiments": [{ "id": "exp_1", "states": [
                { "id": "state_base", "role": "baseline", "variable_refs": ["v_base"],
                  "result_refs": [], "evidence_refs": ["ev_001"] },
                { "id": "state_prop", "role": "proposed", "variable_refs": ["v_prop"],
                  "result_refs": [], "evidence_refs": ["ev_001"] }
            ], "comparisons": [{ "from_state": "state_base", "to_state": "state_prop",
                                 "evidence_refs": ["ev_001"] }], "evidence_refs": [] }],
            "operator_candidates": [],
            "abstraction_candidates": []
        })).unwrap();

        let compiler = Compiler::new();
        let ir = compiler.compile(&extraction).unwrap();
        assert_eq!(ir.operators.len(), 1);
        let operator = &ir.operators[0];
        assert_eq!(operator.operator, OperatorKind::I);
        assert_eq!(operator.input_refs, vec!["block_state_state_base"]);
        assert_eq!(operator.output_refs, vec!["block_state_state_prop"]);
        let changes = operator.payload["changes"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["concept_id"], "representation.fourier.enabled");
    }

    #[test]
    fn candidate_operator_with_dangling_ref_is_op_004() {
        let extraction: ExtractionV3 = serde_json::from_value(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "doc", "source_type": "paper" },
            "evidence": [],
            "variables": [],
            "contexts": [],
            "axiom_sets": [],
            "experiments": [],
            "operator_candidates": [{
                "id": "opc_1", "operator": "T",
                "input_refs": ["missing_var"], "output_refs": ["missing_var"],
                "payload": {}, "evidence_refs": [],
                "verification": { "status": "supported", "confidence": 0.8 }
            }],
            "abstraction_candidates": []
        })).unwrap();

        let compiler = Compiler::new();
        let report = compiler.compile(&extraction).unwrap_err();
        assert!(report.errors.iter().any(|e| e.code == "OP-004"));
    }

    #[test]
    fn hashes_are_filled_deterministic_and_provenance_aware() {
        let doc_a: ExtractionV3 = serde_json::from_value(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "doc_a", "source_type": "paper" },
            "evidence": [{ "id": "ev_001", "document_id": "doc_a", "text_span": "t",
                          "verification": { "status": "supported", "confidence": 0.9 } }],
            "variables": [{ "id": "v", "concept_id": "representation.fourier.enabled",
                            "value_type": "bool", "observed": true, "value": true,
                            "unit_raw": null, "expression_raw": null,
                            "evidence_refs": ["ev_001"] }],
            "contexts": [], "axiom_sets": [], "experiments": [],
            "operator_candidates": [], "abstraction_candidates": []
        })).unwrap();
        let mut doc_b = doc_a.clone();
        doc_b.document.as_mut().unwrap().document_id = "doc_b".to_string();

        let compiler = Compiler::new();
        let ir_a = compiler.compile(&doc_a).unwrap();
        let ir_a_again = compiler.compile(&doc_a).unwrap();

        assert!(!ir_a.blocks[0].semantic_hash.is_empty());
        assert!(!ir_a.blocks[0].instance_hash.is_empty());
        assert!(ir_a.blocks[0].semantic_hash.starts_with("sha256:"));

        // Determinism (V3-20): identical extraction → identical IR hashes.
        assert_eq!(
            ir_a.blocks[0].semantic_hash,
            ir_a_again.blocks[0].semantic_hash
        );
        assert_eq!(
            ir_a.blocks[0].instance_hash,
            ir_a_again.blocks[0].instance_hash
        );

        // Semantic hash ignores provenance; instance hash includes document id.
        let ir_b = compiler.compile(&doc_b).unwrap();
        assert_eq!(ir_a.blocks[0].semantic_hash, ir_b.blocks[0].semantic_hash);
        assert_ne!(ir_a.blocks[0].instance_hash, ir_b.blocks[0].instance_hash);
    }
}
