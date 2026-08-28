//! `myc.graph-ir.v4` — compiled canvas IR (CanvasIRV3).
//!
//! The compiler answers *"what computational structure follows from these
//! extracted facts?"*. It constructs canonical variables, interventions,
//! Blocks, Operators, Chains, Fibers, Bundles, identifiability states,
//! consistency checks, hashes, and graph indexes. This representation is
//! computational; LLM output is evidential.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::OperatorKind;

/// Root schema version for this contract.
pub const SCHEMA_VERSION: &str = crate::GRAPH_IR_SCHEMA_VERSION;

/// Graph object type (handoff-spec.md §37). Distinct from scientific value
/// types.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BlockType {
    Variable,
    State,
    #[serde(rename = "result")]
    Outcome,
    Concept,
    Axiom,
}

/// Effect / identifiability status (handoff-spec.md §50).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffectStatus {
    Identifiable,
    PartiallyIdentifiable,
    Unresolved,
    Confounded,
    InsufficientEvidence,
}

/// Consistency check type (handoff-spec.md §54).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    Path,
    Representation,
    Branch,
    Abstraction,
    Conflict,
}

/// Root compiled-IR object (handoff-spec.md §36).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct CanvasIRV3 {
    pub schema_version: String,
    #[serde(default)]
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub operators: Vec<Operator>,
    #[serde(default)]
    pub chains: Vec<Chain>,
    #[serde(default)]
    pub fibers: Vec<Fiber>,
    #[serde(default)]
    pub bundles: Vec<Bundle>,
    #[serde(default)]
    pub identifiability: Vec<Identifiability>,
    #[serde(default)]
    pub consistency_checks: Vec<ConsistencyCheck>,
    #[serde(default)]
    pub provenance_index: ProvenanceIndex,
}

impl CanvasIRV3 {
    /// A well-formed root with the current schema version and no content.
    pub fn new() -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            ..Self::default()
        }
    }
}

/// The smallest addressable scientific object (handoff-spec.md §37, §38).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Block {
    pub id: String,
    pub block_type: BlockType,
    #[serde(default)]
    pub concept_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable_ref: Option<String>,
    #[serde(default)]
    pub member_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axiom_set_ref: Option<String>,
    pub semantic_hash: String,
    pub instance_hash: String,
}

/// A compiled operator `o: X -> Y` (handoff-spec.md §41, §42).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Operator {
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
    pub semantic_hash: String,
    pub instance_hash: String,
}

/// An ordered computational path `B0 -o1-> B1 -o2-> ... -on-> Bn`
/// (handoff-spec.md §43, §44).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Chain {
    pub id: String,
    #[serde(default)]
    pub block_path: Vec<String>,
    #[serde(default)]
    pub operator_path: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axiom_set_ref: Option<String>,
    #[serde(default)]
    pub source_experiment_refs: Vec<String>,
    pub semantic_hash: String,
    pub instance_hash: String,
}

/// A set of chains under shared conditioning (handoff-spec.md §46).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Fiber {
    pub id: String,
    #[serde(default)]
    pub conditioning: Vec<ConditioningEntry>,
    #[serde(default)]
    pub varying_concepts: Vec<String>,
    #[serde(default)]
    pub chain_refs: Vec<String>,
    pub semantic_hash: String,
}

/// One conditioning dimension: a concept pinned to a canonical value hash.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ConditioningEntry {
    pub concept_id: String,
    pub semantic_value_hash: String,
}

/// A group of fibers across context variation (handoff-spec.md §49).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Bundle {
    pub id: String,
    #[serde(default)]
    pub target_concepts: Vec<String>,
    #[serde(default)]
    pub fiber_refs: Vec<String>,
    #[serde(default)]
    pub varying_dimensions: Vec<String>,
    pub semantic_hash: String,
}

/// Compiler-generated identifiability record (handoff-spec.md §50).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Identifiability {
    pub id: String,
    pub target_ref: String,
    pub intervention_ref: String,
    pub joint_effect: EffectStatusEntry,
    #[serde(default)]
    pub component_effects: Vec<ComponentEffect>,
    #[serde(default)]
    pub interactions: Vec<InteractionEntry>,
    #[serde(default)]
    pub missing_controls: Vec<MissingControl>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct EffectStatusEntry {
    pub status: EffectStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ComponentEffect {
    pub concept_id: String,
    pub status: EffectStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct InteractionEntry {
    #[serde(default)]
    pub concept_refs: Vec<String>,
    pub status: EffectStatus,
}

/// A missing control configuration the historical graph should supply.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MissingControl {
    pub configuration: Value,
}

/// A consistency measurement (handoff-spec.md §54).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ConsistencyCheck {
    pub id: String,
    pub check_type: CheckType,
    #[serde(default)]
    pub input_refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f64>,
    pub status: String,
    #[serde(default)]
    pub details: Value,
}

/// Reverse lookup from evidence id to derived graph objects (handoff-spec.md §62).
pub type ProvenanceIndex = BTreeMap<String, Vec<String>>;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_type_serializes_result_as_outcome_variant() {
        assert_eq!(serde_json::to_string(&BlockType::Outcome).unwrap(), "\"result\"");
        assert_eq!(serde_json::to_string(&BlockType::Variable).unwrap(), "\"variable\"");
    }

    #[test]
    fn effect_status_serializes_snake_case() {
        assert_eq!(
            serde_json::to_string(&EffectStatus::PartiallyIdentifiable).unwrap(),
            "\"partially_identifiable\""
        );
    }

    #[test]
    fn ir_root_round_trips_minimal_graph() {
        let input = json!({
            "schema_version": "myc.graph-ir.v4",
            "blocks": [{
                "id": "block_001",
                "block_type": "variable",
                "concept_id": "representation.fourier.enabled",
                "variable_ref": "var_001",
                "member_refs": [],
                "context_ref": "ctx_001",
                "axiom_set_ref": "ax_001",
                "semantic_hash": "sha256:aa",
                "instance_hash": "sha256:bb"
            }],
            "operators": [{
                "id": "op_001",
                "operator": "I",
                "input_refs": ["block_state_A"],
                "output_refs": ["block_state_B"],
                "payload": { "changes": [] },
                "context_ref": "ctx_001",
                "axiom_set_ref": "ax_001",
                "evidence_refs": ["ev_001"],
                "semantic_hash": "sha256:cc",
                "instance_hash": "sha256:dd"
            }],
            "chains": [],
            "fibers": [],
            "bundles": [],
            "identifiability": [],
            "consistency_checks": [],
            "provenance_index": { "ev_001": ["op_001"] }
        });

        let parsed: CanvasIRV3 = serde_json::from_value(input.clone()).unwrap();
        assert_eq!(parsed.schema_version, "myc.graph-ir.v4");
        assert_eq!(parsed.blocks[0].block_type, BlockType::Variable);
        assert_eq!(parsed.operators[0].operator, OperatorKind::I);
        assert_eq!(parsed.provenance_index["ev_001"], vec!["op_001".to_string()]);

        let round_tripped = serde_json::to_value(&parsed).unwrap();
        assert_eq!(round_tripped, input);
    }

    #[test]
    fn ir_root_defaults_absent_collections() {
        let parsed: CanvasIRV3 = serde_json::from_value(json!({
            "schema_version": "myc.graph-ir.v4"
        }))
        .unwrap();
        assert!(parsed.blocks.is_empty());
        assert!(parsed.provenance_index.is_empty());
    }
}
