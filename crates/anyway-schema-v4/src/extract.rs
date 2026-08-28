//! `myc.llm.v4` — LLM extraction schema (ExtractionV3).
//!
//! The extractor answers *"what is explicitly present in the source?"*. It
//! extracts variables, values, expressions, baseline/proposed states, context,
//! assumptions, experiments, reported results, candidate relations, and exact
//! provenance. It never performs final mathematical judgment.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::OperatorKind;

/// Root schema version for this contract.
pub const SCHEMA_VERSION: &str = crate::LLM_SCHEMA_VERSION;

/// The three scientific primitive value types (handoff-spec.md §7).
///
/// There is no first-class enum, string category, array, distribution, or
/// method type; complex concepts are composed from these three.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveType {
    Bool,
    Number,
    Expression,
}

/// Evidence verification status (handoff-spec.md §6.1).
///
/// Only [`EvidenceStatus::Supported`] evidence enters the default
/// computational graph; `ambiguous` may remain in the record; `unsupported`
/// must never become canonical scientific state.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Supported,
    Ambiguous,
    Unsupported,
}

/// Experimental state role (handoff-spec.md §21).
///
/// Role is document-structure metadata, not part of the scientific value
/// type algebra.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StateRole {
    Baseline,
    Proposed,
    Ablation,
    Control,
    Variant,
    Reference,
}

/// Root extraction object (handoff-spec.md §3). All arrays may be empty, but
/// the LLM must never omit a required root field.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct ExtractionV3 {
    pub schema_version: String,
    #[serde(default)]
    pub document: Option<Document>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub variables: Vec<Variable>,
    #[serde(default)]
    pub contexts: Vec<Context>,
    #[serde(default)]
    pub axiom_sets: Vec<AxiomSet>,
    #[serde(default)]
    pub experiments: Vec<Experiment>,
    #[serde(default)]
    pub operator_candidates: Vec<OperatorCandidate>,
    #[serde(default)]
    pub abstraction_candidates: Vec<AbstractionCandidate>,
}

impl ExtractionV3 {
    /// A well-formed root with the current schema version and no content.
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            ..Self::default()
        }
    }
}

/// Source document provenance (handoff-spec.md §5). Minimum required:
/// `document_id` and `source_type`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Document {
    pub document_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doi: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arxiv_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub source_type: String,
}

/// Immutable provenance anchor (handoff-spec.md §6, §6.2).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Evidence {
    pub id: String,
    pub document_id: String,
    #[serde(default)]
    pub location: EvidenceLocation,
    pub text_span: String,
    pub verification: Verification,
}

/// Physical location of an evidence span inside a source.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct EvidenceLocation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub figure: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equation: Option<String>,
}

/// Extractor confidence plus evidence status.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Verification {
    pub status: EvidenceStatus,
    pub confidence: f64,
}

/// Canonical extraction envelope for one scientific variable (handoff-spec.md §8).
///
/// The `value` field is a JSON scalar (`bool` / finite `number` / `null`); the
/// relationship between `value_type`, `observed`, `value`, and `expression_raw`
/// is enforced by the validator (handoff-spec.md §13), not by this data model.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Variable {
    pub id: String,
    pub concept_id: String,
    pub value_type: PrimitiveType,
    pub observed: bool,
    pub value: Value,
    /// Required field; `null` when absent (handoff-spec.md §8 lists it as required).
    pub unit_raw: Option<String>,
    /// Required field; `null` when absent (handoff-spec.md §8 lists it as required).
    pub expression_raw: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// Conditioning context (handoff-spec.md §17). References variables; it does
/// not duplicate their values.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Context {
    pub id: String,
    #[serde(default)]
    pub variable_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// Assumptions under which inference is valid (handoff-spec.md §19).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct AxiomSet {
    pub id: String,
    #[serde(default)]
    pub constraint_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// A group of comparable states (handoff-spec.md §20).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Experiment {
    pub id: String,
    #[serde(default)]
    pub context_ref: Option<String>,
    #[serde(default)]
    pub axiom_set_ref: Option<String>,
    #[serde(default)]
    pub states: Vec<State>,
    #[serde(default)]
    pub comparisons: Vec<StateComparison>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// A sparse experimental configuration (handoff-spec.md §21, §24).
///
/// A missing concept does not automatically receive `false`; it stays unknown
/// unless evidence establishes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct State {
    pub id: String,
    #[serde(default)]
    pub role: Option<StateRole>,
    #[serde(default)]
    pub variable_refs: Vec<String>,
    #[serde(default)]
    pub result_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// The LLM only records that the source compares two states (handoff-spec.md §23).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct StateComparison {
    pub from_state: String,
    pub to_state: String,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

/// An explicit relation the LLM identified in the source (handoff-spec.md §28).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OperatorCandidate {
    pub id: String,
    pub operator: OperatorKind,
    #[serde(default)]
    pub input_refs: Vec<String>,
    #[serde(default)]
    pub output_refs: Vec<String>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axiom_set_ref: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    pub verification: Verification,
}

/// A higher-level concept proposal (handoff-spec.md §33). The LLM only ever
/// emits `candidate`; promotion is the compiler's decision.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AbstractionCandidate {
    pub id: String,
    #[serde(default)]
    pub input_concept_ids: Vec<String>,
    pub proposed_concept_id: String,
    #[serde(default)]
    pub rationale_evidence_refs: Vec<String>,
    /// Allowed LLM status is exactly `"candidate"`.
    pub status: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn primitive_types_serialize_lowercase() {
        assert_eq!(serde_json::to_string(&PrimitiveType::Bool).unwrap(), "\"bool\"");
        assert_eq!(serde_json::to_string(&PrimitiveType::Number).unwrap(), "\"number\"");
        assert_eq!(serde_json::to_string(&PrimitiveType::Expression).unwrap(), "\"expression\"");
    }

    #[test]
    fn evidence_status_serializes_snake_case() {
        assert_eq!(serde_json::to_string(&EvidenceStatus::Supported).unwrap(), "\"supported\"");
        assert_eq!(serde_json::to_string(&EvidenceStatus::Unsupported).unwrap(), "\"unsupported\"");
    }

    #[test]
    fn extraction_root_round_trips_minimal_document() {
        let input = json!({
            "schema_version": "myc.llm.v4",
            "document": {
                "document_id": "doc_pinn_001",
                "title": "Example PINN Paper",
                "year": 2025,
                "source_type": "paper"
            },
            "evidence": [{
                "id": "ev_001",
                "document_id": "doc_pinn_001",
                "location": { "page": 5, "section": "Method" },
                "text_span": "We introduce Fourier features with sigma = 10.",
                "verification": { "status": "supported", "confidence": 0.98 }
            }],
            "variables": [
                {
                    "id": "var_001",
                    "concept_id": "representation.fourier.enabled",
                    "value_type": "bool",
                    "observed": true,
                    "value": true,
                    "unit_raw": null,
                    "expression_raw": null,
                    "evidence_refs": ["ev_001"]
                },
                {
                    "id": "var_002",
                    "concept_id": "representation.fourier.sigma",
                    "value_type": "number",
                    "observed": true,
                    "value": 10.0,
                    "unit_raw": null,
                    "expression_raw": null,
                    "evidence_refs": ["ev_001"]
                }
            ],
            "contexts": [],
            "axiom_sets": [],
            "experiments": [],
            "operator_candidates": [],
            "abstraction_candidates": []
        });

        let parsed: ExtractionV3 = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(parsed.schema_version, "myc.llm.v4");
        assert_eq!(parsed.document.as_ref().unwrap().document_id, "doc_pinn_001");
        assert_eq!(parsed.variables.len(), 2);
        assert_eq!(parsed.variables[0].value_type, PrimitiveType::Bool);
        assert_eq!(parsed.variables[1].value_type, PrimitiveType::Number);
        assert_eq!(parsed.variables[1].value, json!(10.0));

        let round_tripped = serde_json::to_value(&parsed).unwrap();
        assert_eq!(round_tripped, input);
    }

    #[test]
    fn extraction_root_defaults_absent_collections() {
        let parsed: ExtractionV3 = serde_json::from_value(json!({
            "schema_version": "myc.llm.v4",
            "document": { "document_id": "doc_1", "source_type": "paper" }
        }))
        .unwrap();
        assert!(parsed.evidence.is_empty());
        assert!(parsed.operator_candidates.is_empty());
    }
}
