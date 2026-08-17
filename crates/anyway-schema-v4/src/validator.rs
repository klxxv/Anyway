//! Schema v4 structural validator (handoff-spec.md §84–§87, §13, §42, §44).
//!
//! This is the compiler-owned validation gate. It enforces the *structural*
//! invariants (value-type consistency, operator cardinality, chain path
//! length, required fields) and reports errors with a stable code plus a JSON
//! path. Cross-reference resolution and the evidence gate are a later step.
//!
//! Errors never grant a fallback: an invalid document is rejected as a whole,
//! with every violation reported so a caller can surface them all at once.

use crate::extract::{
    self, EvidenceStatus, ExtractionV3, PrimitiveType,
};
use crate::ir::{self, CanvasIRV3};

/// A structural violation with a stable code and a JSON path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub code: String,
    pub path: String,
    pub message: String,
}

/// A non-fatal advisory (handoff-spec.md §87).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    pub code: String,
    pub path: String,
    pub message: String,
}

/// The aggregate result of validating one root object.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationReport {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<Warning>,
}

impl ValidationReport {
    /// `true` when there are no errors (warnings do not fail compilation).
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Append the errors/warnings of another report, prefixing every path.
    pub fn merge_from(&mut self, prefix: &str, other: ValidationReport) {
        self.errors.extend(other.errors.into_iter().map(|mut e| {
            e.path = format!("{prefix}{}", e.path);
            e
        }));
        self.warnings.extend(other.warnings.into_iter().map(|mut w| {
            w.path = format!("{prefix}{}", w.path);
            w
        }));
    }

    fn error(&mut self, code: &str, path: &str, message: impl Into<String>) {
        self.errors.push(ValidationError {
            code: code.to_string(),
            path: path.to_string(),
            message: message.into(),
        });
    }
}

/// Validate the LLM extraction root (`myc.llm.v4`).
pub fn validate_extraction(extraction: &ExtractionV3) -> ValidationReport {
    let mut report = ValidationReport::default();

    if extraction.schema_version != extract::SCHEMA_VERSION {
        report.error(
            "VERSION",
            "$.schema_version",
            format!(
                "expected {} but got {}",
                extract::SCHEMA_VERSION,
                extraction.schema_version
            ),
        );
    }

    if let Some(document) = &extraction.document {
        validate_document(document, &mut report);
    } else {
        report.error("DOC-001", "$.document", "document is required");
    }

    for (index, evidence) in extraction.evidence.iter().enumerate() {
        validate_evidence(evidence, index, &mut report);
    }
    for (index, variable) in extraction.variables.iter().enumerate() {
        validate_variable(variable, index, &mut report);
    }

    report
}

/// Validate the compiled graph IR root (`myc.graph-ir.v4`).
pub fn validate_ir(ir: &CanvasIRV3) -> ValidationReport {
    let mut report = ValidationReport::default();

    if ir.schema_version != ir::SCHEMA_VERSION {
        report.error(
            "VERSION",
            "$.schema_version",
            format!(
                "expected {} but got {}",
                ir::SCHEMA_VERSION,
                ir.schema_version
            ),
        );
    }

    for (index, operator) in ir.operators.iter().enumerate() {
        validate_operator(operator, index, &mut report);
    }
    for (index, chain) in ir.chains.iter().enumerate() {
        validate_chain(chain, index, &mut report);
    }

    report
}

fn validate_document(document: &extract::Document, report: &mut ValidationReport) {
    if document.document_id.trim().is_empty() {
        report.error(
            "DOC-001",
            "$.document.document_id",
            "document_id is required and must not be empty",
        );
    }
    if document.source_type.trim().is_empty() {
        report.error(
            "DOC-002",
            "$.document.source_type",
            "source_type is required and must not be empty",
        );
    }
}

fn validate_evidence(evidence: &extract::Evidence, index: usize, report: &mut ValidationReport) {
    let path = format!("$.evidence[{index}]");
    if evidence.id.trim().is_empty() {
        report.error("EV-001", &format!("{path}.id"), "evidence id is required");
    }
    if evidence.document_id.trim().is_empty() {
        report.error(
            "EV-002",
            &format!("{path}.document_id"),
            "evidence document_id is required",
        );
    }
    if evidence.text_span.trim().is_empty() {
        report.error(
            "EV-003",
            &format!("{path}.text_span"),
            "evidence text_span is required",
        );
    }
    if evidence.verification.status == EvidenceStatus::Unsupported {
        report.warnings.push(Warning {
            code: "UNSUPPORTED_EVIDENCE".to_string(),
            path: path,
            message: "unsupported evidence will never enter the canonical graph".to_string(),
        });
    }
}

/// Enforce VAR-001..VAR-008 (handoff-spec.md §13).
///
/// VAR-001 (`value_type` membership) is guaranteed by [`PrimitiveType`] being
/// an enum, so it cannot fail at runtime; the remaining invariants are checked
/// here because `value` is an untyped JSON scalar.
fn validate_variable(variable: &extract::Variable, index: usize, report: &mut ValidationReport) {
    let path = format!("$.variables[{index}]");

    // VAR-002: observed=false => value=null
    if !variable.observed && !variable.value.is_null() {
        report.error(
            "VAR-002",
            &format!("{path}.value"),
            "an unobserved variable must have a null value",
        );
    }
    // VAR-003: observed=false => expression_raw=null
    if !variable.observed && variable.expression_raw.is_some() {
        report.error(
            "VAR-003",
            &format!("{path}.expression_raw"),
            "an unobserved variable must have a null expression",
        );
    }
    // VAR-004: observed=true => evidence_refs.length >= 1
    if variable.observed && variable.evidence_refs.is_empty() {
        report.error(
            "VAR-004",
            &format!("{path}.evidence_refs"),
            "an observed variable requires at least one evidence reference",
        );
    }

    match variable.value_type {
        PrimitiveType::Bool => {
            // VAR-005: bool && observed => value ∈ {true,false}
            if variable.observed && !variable.value.is_boolean() {
                report.error(
                    "VAR-005",
                    &format!("{path}.value"),
                    "a bool variable must carry a boolean value",
                );
            }
        }
        PrimitiveType::Number => {
            // VAR-006: number && observed => value is finite numeric
            if variable.observed && !variable.value.is_number() {
                report.error(
                    "VAR-006",
                    &format!("{path}.value"),
                    "a number variable must carry a finite numeric value",
                );
            }
        }
        PrimitiveType::Expression => {
            // VAR-007: expression && observed => expression_raw != null
            if variable.observed && variable.expression_raw.is_none() {
                report.error(
                    "VAR-007",
                    &format!("{path}.expression_raw"),
                    "an observed expression variable requires expression_raw",
                );
            }
            // VAR-008: expression => value=null
            if !variable.value.is_null() {
                report.error(
                    "VAR-008",
                    &format!("{path}.value"),
                    "an expression variable must have a null value",
                );
            }
        }
    }
}

/// Enforce OP-002 and OP-003 (handoff-spec.md §42).
///
/// OP-001 (`operator` membership) is guaranteed by [`crate::OperatorKind`] being
/// an enum. OP-004..OP-006 (reference resolution) are a later step.
fn validate_operator(operator: &ir::Operator, index: usize, report: &mut ValidationReport) {
    let path = format!("$.operators[{index}]");
    if operator.input_refs.is_empty() {
        report.error(
            "OP-002",
            &format!("{path}.input_refs"),
            "an operator requires at least one input reference",
        );
    }
    if operator.output_refs.is_empty() {
        report.error(
            "OP-003",
            &format!("{path}.output_refs"),
            "an operator requires at least one output reference",
        );
    }
}

/// Enforce CHAIN-001 (handoff-spec.md §44).
fn validate_chain(chain: &ir::Chain, index: usize, report: &mut ValidationReport) {
    let path = format!("$.chains[{index}]");
    // CHAIN-001: block_path.length = operator_path.length + 1
    if chain.block_path.len() != chain.operator_path.len() + 1 {
        report.error(
            "CHAIN-001",
            &format!("{path}"),
            format!(
                "a chain must have exactly one more block than operator ({} blocks vs {} operators)",
                chain.block_path.len(),
                chain.operator_path.len()
            ),
        );
    }
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
    fn valid_extraction_has_no_errors() {
        let report = validate_extraction(&extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "doc_1", "source_type": "paper" },
            "evidence": [{
                "id": "ev_1", "document_id": "doc_1",
                "text_span": "text",
                "verification": { "status": "supported", "confidence": 0.9 }
            }],
            "variables": [{
                "id": "var_1", "concept_id": "c", "value_type": "bool",
                "observed": true, "value": true,
                "unit_raw": null, "expression_raw": null, "evidence_refs": ["ev_1"]
            }]
        })));
        assert!(report.ok(), "unexpected errors: {:?}", report.errors);
    }

    #[test]
    fn version_mismatch_is_reported() {
        let report = validate_extraction(&extraction(json!({
            "schema_version": "wrong.version",
            "document": { "document_id": "doc_1", "source_type": "paper" }
        })));
        assert!(!report.ok());
        assert!(report.errors.iter().any(|e| e.code == "VERSION"));
    }

    #[test]
    fn unobserved_variable_must_have_null_value() {
        let report = validate_extraction(&extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "d", "source_type": "paper" },
            "variables": [{
                "id": "var_1", "concept_id": "c", "value_type": "number",
                "observed": false, "value": 3.0,
                "unit_raw": null, "expression_raw": null, "evidence_refs": []
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "VAR-002"));
    }

    #[test]
    fn observed_variable_requires_evidence() {
        let report = validate_extraction(&extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "d", "source_type": "paper" },
            "variables": [{
                "id": "var_1", "concept_id": "c", "value_type": "bool",
                "observed": true, "value": true,
                "unit_raw": null, "expression_raw": null, "evidence_refs": []
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "VAR-004"));
    }

    #[test]
    fn expression_variable_forces_null_value_and_raw_expression() {
        let report = validate_extraction(&extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "d", "source_type": "paper" },
            "variables": [{
                "id": "var_1", "concept_id": "c", "value_type": "expression",
                "observed": true, "value": "u_t",
                "unit_raw": null, "expression_raw": null, "evidence_refs": ["ev_1"]
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "VAR-007"));
        assert!(report.errors.iter().any(|e| e.code == "VAR-008"));
    }

    #[test]
    fn operator_requires_inputs_and_outputs() {
        let report = validate_ir(&ir(json!({
            "schema_version": "myc.graph-ir.v4",
            "operators": [{
                "id": "op_1", "operator": "I",
                "input_refs": [], "output_refs": [],
                "payload": {}, "evidence_refs": [],
                "semantic_hash": "h", "instance_hash": "h"
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "OP-002"));
        assert!(report.errors.iter().any(|e| e.code == "OP-003"));
    }

    #[test]
    fn chain_path_length_must_be_blocks_equals_operators_plus_one() {
        let report = validate_ir(&ir(json!({
            "schema_version": "myc.graph-ir.v4",
            "chains": [{
                "id": "chain_1",
                "block_path": ["a", "b", "c"],
                "operator_path": ["o1"],
                "semantic_hash": "h", "instance_hash": "h"
            }]
        })));
        assert!(report.errors.iter().any(|e| e.code == "CHAIN-001"));
    }

    #[test]
    fn error_paths_are_json_located() {
        let report = validate_extraction(&extraction(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "d", "source_type": "paper" },
            "variables": [{
                "id": "var_1", "concept_id": "c", "value_type": "bool",
                "observed": true, "value": true,
                "unit_raw": null, "expression_raw": null, "evidence_refs": []
            }]
        })));
        let var004 = report.errors.iter().find(|e| e.code == "VAR-004").unwrap();
        assert_eq!(var004.path, "$.variables[0].evidence_refs");
    }
}
