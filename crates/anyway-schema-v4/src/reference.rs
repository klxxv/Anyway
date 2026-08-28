//! Reference resolution and the evidence gate (handoff-spec.md §6.1, §64).
//!
//! Step 3 of the compiler pipeline: after structural validation, every
//! `*_refs` field must resolve to an object that exists in the same root, and
//! only `supported` evidence enters the default computational graph.

use std::collections::{HashMap, HashSet};

use crate::extract::{EvidenceStatus, ExtractionV3};
use crate::ir::CanvasIRV3;
use crate::validator::ValidationReport;

/// Partition of evidence by verification status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvidenceGate {
    /// Evidence that may enter the default computational graph.
    pub supported: Vec<String>,
    /// Evidence retained in the record but not promoted to canonical state.
    pub ambiguous: Vec<String>,
    /// Evidence that must never become canonical scientific state.
    pub unsupported: Vec<String>,
}

/// The evidence gate: partition evidence ids by status.
///
/// Only [`EvidenceGate::supported`] enters the default graph; `ambiguous` is
/// retained; `unsupported` is never canonical.
pub fn gate_evidence(extraction: &ExtractionV3) -> EvidenceGate {
    let mut gate = EvidenceGate::default();
    for evidence in &extraction.evidence {
        match evidence.verification.status {
            EvidenceStatus::Supported => gate.supported.push(evidence.id.clone()),
            EvidenceStatus::Ambiguous => gate.ambiguous.push(evidence.id.clone()),
            EvidenceStatus::Unsupported => gate.unsupported.push(evidence.id.clone()),
        }
    }
    gate
}

/// Resolve every reference inside the extraction root.
///
/// A dangling reference produces a `REF-001` error whose path identifies the
/// offending field; a duplicate id is also reported (`REF-002`).
pub fn resolve_extraction_references(extraction: &ExtractionV3) -> ValidationReport {
    let mut report = ValidationReport::default();

    let mut evidence_ids: HashSet<&str> = HashSet::new();
    let mut variable_ids: HashSet<&str> = HashSet::new();
    let mut context_ids: HashSet<&str> = HashSet::new();
    let mut axiom_ids: HashSet<&str> = HashSet::new();
    let mut experiment_ids: HashSet<&str> = HashSet::new();
    let mut state_ids: HashMap<&str, &str> = HashMap::new(); // state id -> experiment id

    let document_id = extraction.document.as_ref().map(|d| d.document_id.as_str());

    for evidence in &extraction.evidence {
        if !evidence_ids.insert(evidence.id.as_str()) {
            report.error("REF-002", &format!("$.evidence[{}].id", 0), format!("duplicate evidence id {}", evidence.id));
        }
    }
    for variable in &extraction.variables {
        if !variable_ids.insert(variable.id.as_str()) {
            report.error("REF-002", "$.variables", format!("duplicate variable id {}", variable.id));
        }
    }
    for context in &extraction.contexts {
        if !context_ids.insert(context.id.as_str()) {
            report.error("REF-002", "$.contexts", format!("duplicate context id {}", context.id));
        }
    }
    for axiom in &extraction.axiom_sets {
        if !axiom_ids.insert(axiom.id.as_str()) {
            report.error("REF-002", "$.axiom_sets", format!("duplicate axiom set id {}", axiom.id));
        }
    }
    for experiment in &extraction.experiments {
        if !experiment_ids.insert(experiment.id.as_str()) {
            report.error("REF-002", "$.experiments", format!("duplicate experiment id {}", experiment.id));
        }
        for state in &experiment.states {
            state_ids.insert(state.id.as_str(), experiment.id.as_str());
        }
    }

    // Evidence document_id must reference the single document.
    for (index, evidence) in extraction.evidence.iter().enumerate() {
        let path = format!("$.evidence[{index}].document_id");
        match document_id {
            Some(id) if id == evidence.document_id => {}
            Some(_) => report.error("REF-001", &path, format!("unknown document id {}", evidence.document_id)),
            None => report.error("REF-001", &path, "no document is present to reference".to_string()),
        }
    }

    // Variable evidence_refs.
    for (index, variable) in extraction.variables.iter().enumerate() {
        let path = format!("$.variables[{index}].evidence_refs");
        for reference in &variable.evidence_refs {
            if !evidence_ids.contains(reference.as_str()) {
                report.error("REF-001", &path, format!("unknown evidence reference {reference}"));
            }
        }
    }

    // Context variable_refs + evidence_refs.
    for (index, context) in extraction.contexts.iter().enumerate() {
        for reference in &context.variable_refs {
            if !variable_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("$.contexts[{index}].variable_refs"), format!("unknown variable reference {reference}"));
            }
        }
        for reference in &context.evidence_refs {
            if !evidence_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("$.contexts[{index}].evidence_refs"), format!("unknown evidence reference {reference}"));
            }
        }
    }

    // AxiomSet constraint_refs (variables) + evidence_refs.
    for (index, axiom) in extraction.axiom_sets.iter().enumerate() {
        for reference in &axiom.constraint_refs {
            if !variable_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("$.axiom_sets[{index}].constraint_refs"), format!("unknown variable reference {reference}"));
            }
        }
        for reference in &axiom.evidence_refs {
            if !evidence_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("$.axiom_sets[{index}].evidence_refs"), format!("unknown evidence reference {reference}"));
            }
        }
    }

    // Experiments: context_ref, axiom_set_ref, states, comparisons.
    for (index, experiment) in extraction.experiments.iter().enumerate() {
        if let Some(context_ref) = &experiment.context_ref {
            if !context_ids.contains(context_ref.as_str()) {
                report.error("REF-001", &format!("$.experiments[{index}].context_ref"), format!("unknown context reference {context_ref}"));
            }
        }
        if let Some(axiom_ref) = &experiment.axiom_set_ref {
            if !axiom_ids.contains(axiom_ref.as_str()) {
                report.error("REF-001", &format!("$.experiments[{index}].axiom_set_ref"), format!("unknown axiom set reference {axiom_ref}"));
            }
        }
        for (state_index, state) in experiment.states.iter().enumerate() {
            let path = format!("$.experiments[{index}].states[{state_index}]");
            for reference in &state.variable_refs {
                if !variable_ids.contains(reference.as_str()) {
                    report.error("REF-001", &format!("{path}.variable_refs"), format!("unknown variable reference {reference}"));
                }
            }
            for reference in &state.result_refs {
                if !variable_ids.contains(reference.as_str()) {
                    report.error("REF-001", &format!("{path}.result_refs"), format!("unknown result reference {reference}"));
                }
            }
            for reference in &state.evidence_refs {
                if !evidence_ids.contains(reference.as_str()) {
                    report.error("REF-001", &format!("{path}.evidence_refs"), format!("unknown evidence reference {reference}"));
                }
            }
        }
        for (comparison_index, comparison) in experiment.comparisons.iter().enumerate() {
            let path = format!("$.experiments[{index}].comparisons[{comparison_index}]");
            if !state_ids.contains_key(comparison.from_state.as_str()) {
                report.error("REF-001", &format!("{path}.from_state"), format!("unknown state reference {}", comparison.from_state));
            }
            if !state_ids.contains_key(comparison.to_state.as_str()) {
                report.error("REF-001", &format!("{path}.to_state"), format!("unknown state reference {}", comparison.to_state));
            }
        }
    }

    // Operator candidates: input/output/context/axiom/evidence refs.
    for (index, candidate) in extraction.operator_candidates.iter().enumerate() {
        let path = format!("$.operator_candidates[{index}]");
        for reference in &candidate.input_refs {
            if !variable_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("{path}.input_refs"), format!("unknown variable reference {reference}"));
            }
        }
        for reference in &candidate.output_refs {
            if !variable_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("{path}.output_refs"), format!("unknown variable reference {reference}"));
            }
        }
        if let Some(context_ref) = &candidate.context_ref {
            if !context_ids.contains(context_ref.as_str()) {
                report.error("REF-001", &format!("{path}.context_ref"), format!("unknown context reference {context_ref}"));
            }
        }
        if let Some(axiom_ref) = &candidate.axiom_set_ref {
            if !axiom_ids.contains(axiom_ref.as_str()) {
                report.error("REF-001", &format!("{path}.axiom_set_ref"), format!("unknown axiom set reference {axiom_ref}"));
            }
        }
        for reference in &candidate.evidence_refs {
            if !evidence_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("{path}.evidence_refs"), format!("unknown evidence reference {reference}"));
            }
        }
    }

    // Abstraction candidates: rationale evidence refs.
    for (index, candidate) in extraction.abstraction_candidates.iter().enumerate() {
        for reference in &candidate.rationale_evidence_refs {
            if !evidence_ids.contains(reference.as_str()) {
                report.error("REF-001", &format!("$.abstraction_candidates[{index}].rationale_evidence_refs"), format!("unknown evidence reference {reference}"));
            }
        }
    }

    report
}

/// Resolve every internal reference inside the compiled IR root.
///
/// Enforces OP-004 (operator input/output resolve), CHAIN-004 (chain path
/// resolves), FIB-001 (fiber member is a chain), and bundle fiber resolution.
pub fn resolve_ir_references(ir: &CanvasIRV3) -> ValidationReport {
    let mut report = ValidationReport::default();

    let block_ids: HashSet<&str> = ir.blocks.iter().map(|b| b.id.as_str()).collect();
    let operator_ids: HashSet<&str> = ir.operators.iter().map(|o| o.id.as_str()).collect();
    let chain_ids: HashSet<&str> = ir.chains.iter().map(|c| c.id.as_str()).collect();
    let fiber_ids: HashSet<&str> = ir.fibers.iter().map(|f| f.id.as_str()).collect();

    for (index, operator) in ir.operators.iter().enumerate() {
        let path = format!("$.operators[{index}]");
        for reference in &operator.input_refs {
            if !block_ids.contains(reference.as_str()) {
                report.error("OP-004", &format!("{path}.input_refs"), format!("unknown block reference {reference}"));
            }
        }
        for reference in &operator.output_refs {
            if !block_ids.contains(reference.as_str()) {
                report.error("OP-004", &format!("{path}.output_refs"), format!("unknown block reference {reference}"));
            }
        }
    }

    for (index, chain) in ir.chains.iter().enumerate() {
        let path = format!("$.chains[{index}]");
        for reference in &chain.block_path {
            if !block_ids.contains(reference.as_str()) {
                report.error("CHAIN-004", &format!("{path}.block_path"), format!("unknown block reference {reference}"));
            }
        }
        for reference in &chain.operator_path {
            if !operator_ids.contains(reference.as_str()) {
                report.error("CHAIN-004", &format!("{path}.operator_path"), format!("unknown operator reference {reference}"));
            }
        }
    }

    for (index, fiber) in ir.fibers.iter().enumerate() {
        let path = format!("$.fibers[{index}]");
        for reference in &fiber.chain_refs {
            if !chain_ids.contains(reference.as_str()) {
                report.error("FIB-001", &format!("{path}.chain_refs"), format!("unknown chain reference {reference}"));
            }
        }
    }

    for (index, bundle) in ir.bundles.iter().enumerate() {
        let path = format!("$.bundles[{index}]");
        for reference in &bundle.fiber_refs {
            if !fiber_ids.contains(reference.as_str()) {
                report.error("BUN-001", &format!("{path}.fiber_refs"), format!("unknown fiber reference {reference}"));
            }
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn extraction(value: serde_json::Value) -> ExtractionV3 {
        serde_json::from_value(value).expect("extraction parses")
    }

    fn ir(value: serde_json::Value) -> CanvasIRV3 {
        serde_json::from_value(value).expect("ir parses")
    }

    #[test]
    fn evidence_gate_partitions_by_status() {
        let extraction = extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "d", "source_type": "paper" },
            "evidence": [
                { "id": "ev_s", "document_id": "d", "text_span": "s",
                  "verification": { "status": "supported", "confidence": 0.9 } },
                { "id": "ev_a", "document_id": "d", "text_span": "a",
                  "verification": { "status": "ambiguous", "confidence": 0.5 } },
                { "id": "ev_u", "document_id": "d", "text_span": "u",
                  "verification": { "status": "unsupported", "confidence": 0.1 } }
            ]
        }));
        let gate = gate_evidence(&extraction);
        assert_eq!(gate.supported, vec!["ev_s".to_string()]);
        assert_eq!(gate.ambiguous, vec!["ev_a".to_string()]);
        assert_eq!(gate.unsupported, vec!["ev_u".to_string()]);
    }

    #[test]
    fn dangling_variable_evidence_is_ref_001() {
        let report = resolve_extraction_references(&extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "d", "source_type": "paper" },
            "variables": [{
                "id": "var_1", "concept_id": "c", "value_type": "bool",
                "observed": true, "value": true,
                "unit_raw": null, "expression_raw": null, "evidence_refs": ["missing"]
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "REF-001"));
    }

    #[test]
    fn operator_reference_to_unknown_block_is_op_004() {
        let report = resolve_ir_references(&ir(json!({
            "schema_version": "myc.graph-ir.v4",
            "operators": [{
                "id": "op_1", "operator": "I",
                "input_refs": ["ghost"], "output_refs": [],
                "payload": {}, "evidence_refs": [],
                "semantic_hash": "h", "instance_hash": "h"
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "OP-004"));
    }

    #[test]
    fn fiber_member_must_be_a_chain() {
        let report = resolve_ir_references(&ir(json!({
            "schema_version": "myc.graph-ir.v4",
            "fibers": [{
                "id": "fiber_1", "conditioning": [],
                "varying_concepts": [], "chain_refs": ["ghost_chain"],
                "semantic_hash": "h"
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "FIB-001"));
    }

    #[test]
    fn experiment_state_comparison_resolves() {
        let report = resolve_extraction_references(&extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "d", "source_type": "paper" },
            "experiments": [{
                "id": "exp_1",
                "states": [{ "id": "state_a", "variable_refs": [], "result_refs": [], "evidence_refs": [] }],
                "comparisons": [{ "from_state": "state_a", "to_state": "ghost", "evidence_refs": [] }]
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "REF-001"));
    }
}
