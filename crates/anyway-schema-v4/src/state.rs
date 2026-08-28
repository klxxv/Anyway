//! Compiler-side State model (handoff-spec.md §21, §24, §25).
//!
//! A [`CompilerState`] is a **sparse configuration**: it holds exactly the
//! canonical concept dimensions the source establishes, and nothing else. A
//! concept that is absent from the map is **unknown**, never `false` (§24,
//! §5.4 of mvp-architecture.md). Only `observed = true` variables enter a
//! state as confirmed dimensions.
//!
//! This module resolves the LLM's reference-based [`extract::State`] into
//! canonical concept ids and canonical values, while preserving the raw
//! extraction (V3-15, V3-16): the raw concept phrase, raw JSON scalar, raw
//! unit, and raw expression all remain recoverable on every entry.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonicalize::{canonicalize_number, normalize_expression, ConceptCanonicalizer};
use crate::extract::{self, ExtractionV3, PrimitiveType, StateRole, Variable};
use crate::validator::ValidationReport;

/// A canonical, resolved value for one state dimension.
///
/// Serialized as a tagged object: `{"type":"bool","value":true}`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum StateValue {
    Bool(bool),
    Number(f64),
    Expression(String),
}

/// One sparse dimension of a resolved state: the canonical concept plus its
/// canonical value, with the raw extraction preserved alongside.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StateEntry {
    /// The raw concept phrase as extracted (V3-15).
    pub raw_concept: String,
    /// The canonical concept id (also the map key).
    pub canonical_concept_id: String,
    /// The raw JSON scalar as extracted (V3-16). `null` for expressions.
    pub raw_value: Value,
    /// Raw unit when the value is a number (preserved for traceability).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit_raw: Option<String>,
    /// Raw expression string when the value is an expression (preserved).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression_raw: Option<String>,
    /// The canonical resolved value.
    pub value: StateValue,
}

/// A resolved, canonical sparse configuration (handoff-spec.md §21, §24).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CompilerState {
    pub id: String,
    #[serde(default)]
    pub role: Option<StateRole>,
    /// Canonical concept id → resolved entry. Absence means unknown.
    #[serde(default)]
    pub entries: BTreeMap<String, StateEntry>,
    /// Result (outcome) variable refs, resolved by later steps.
    #[serde(default)]
    pub result_refs: Vec<String>,
    #[serde(default)]
    pub evidence_refs: Vec<String>,
}

impl CompilerState {
    /// Sparse lookup: `None` means the concept is **unknown**, never `false`.
    pub fn get(&self, canonical_concept_id: &str) -> Option<&StateEntry> {
        self.entries.get(canonical_concept_id)
    }

    /// Whether the state explicitly establishes this canonical concept.
    pub fn contains(&self, canonical_concept_id: &str) -> bool {
        self.entries.contains_key(canonical_concept_id)
    }

    /// Canonical concept ids present in this state, in deterministic order.
    pub fn concept_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// Number of explicitly established dimensions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when the state establishes no dimensions at all.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Resolve an LLM [`extract::State`] into a canonical [`CompilerState`].
///
/// Returns a [`ValidationReport`] describing every failure when any reference
/// is dangling (REF-001), any value violates its declared primitive type
/// (STATE-001), or the state establishes two conflicting values for one
/// canonical concept (EXP-002). Warnings never fail resolution.
pub fn resolve_state(
    raw: &extract::State,
    extraction: &ExtractionV3,
    canonicalizer: &ConceptCanonicalizer,
) -> Result<CompilerState, ValidationReport> {
    let mut report = ValidationReport::default();
    let variable_index: HashMap<&str, &Variable> = extraction
        .variables
        .iter()
        .map(|variable| (variable.id.as_str(), variable))
        .collect();

    let mut entries: BTreeMap<String, StateEntry> = BTreeMap::new();

    for variable_ref in &raw.variable_refs {
        let Some(variable) = variable_index.get(variable_ref.as_str()) else {
            report.error(
                "REF-001",
                &format!("$.states[{:?}].variable_refs", raw.id),
                format!("unresolved variable reference: {variable_ref}"),
            );
            continue;
        };

        let Some(value) = resolve_value(variable, &format!("$.states[{:?}]", raw.id), &mut report) else {
            continue;
        };

        let record = canonicalizer.canonicalize(&variable.concept_id);
        let canonical_concept_id = record.canonical_concept_id.clone();
        let entry = StateEntry {
            raw_concept: variable.concept_id.clone(),
            canonical_concept_id: canonical_concept_id.clone(),
            raw_value: variable.value.clone(),
            unit_raw: variable.unit_raw.clone(),
            expression_raw: variable.expression_raw.clone(),
            value,
        };

        match entries.entry(canonical_concept_id) {
            std::collections::btree_map::Entry::Vacant(slot) => {
                slot.insert(entry);
            }
            std::collections::btree_map::Entry::Occupied(slot) => {
                if slot.get().value == entry.value {
                    // Same canonical value under two raw forms: deduplicate.
                    continue;
                }
                report.error(
                    "EXP-002",
                    &format!("$.states[{:?}]", raw.id),
                    format!(
                        "state establishes conflicting values for concept {}",
                        entry.canonical_concept_id
                    ),
                );
            }
        }
    }

    if report.ok() {
        Ok(CompilerState {
            id: raw.id.clone(),
            role: raw.role,
            entries,
            result_refs: raw.result_refs.clone(),
            evidence_refs: raw.evidence_refs.clone(),
        })
    } else {
        Err(report)
    }
}

/// Convert one variable into its canonical [`StateValue`], independent of any
/// state, for use by the compiler and the hashing layer.
pub fn canonical_variable_value(variable: &Variable) -> Result<StateValue, ValidationReport> {
    let mut report = ValidationReport::default();
    match resolve_value(variable, &format!("$.variables[{}]", variable.id), &mut report) {
        Some(value) => Ok(value),
        None => Err(report),
    }
}

/// Convert one variable into its canonical [`StateValue`], pushing any
/// type-violation error (STATE-001) onto `report`.
fn resolve_value(
    variable: &Variable,
    path: &str,
    report: &mut ValidationReport,
) -> Option<StateValue> {
    match variable.value_type {
        PrimitiveType::Bool => match variable.value.as_bool() {
            Some(value) => Some(StateValue::Bool(value)),
            None => {
                report.error(
                    "STATE-001",
                    path,
                    format!(
                        "variable {} declares bool but carries non-bool value",
                        variable.id
                    ),
                );
                None
            }
        },
        PrimitiveType::Number => match variable.value.as_f64() {
            Some(raw_number) => {
                let canonical = canonicalize_number(raw_number, variable.unit_raw.as_deref());
                Some(StateValue::Number(canonical.value_canonical))
            }
            None => {
                report.error(
                    "STATE-001",
                    path,
                    format!(
                        "variable {} declares number but carries non-number value",
                        variable.id
                    ),
                );
                None
            }
        },
        PrimitiveType::Expression => match &variable.expression_raw {
            Some(raw_expression) => {
                let normalized = normalize_expression(raw_expression);
                Some(StateValue::Expression(normalized.normalized))
            }
            None => {
                report.error(
                    "STATE-001",
                    path,
                    format!(
                        "variable {} declares expression but carries no expression_raw",
                        variable.id
                    ),
                );
                None
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::{State as RawState, Variable as RawVariable};

    fn variable(id: &str, concept_id: &str, value_type: PrimitiveType, value: Value) -> RawVariable {
        RawVariable {
            id: id.to_string(),
            concept_id: concept_id.to_string(),
            value_type,
            observed: true,
            value,
            unit_raw: None,
            expression_raw: None,
            evidence_refs: vec!["ev".to_string()],
        }
    }

    fn number_variable(id: &str, concept_id: &str, value: f64, unit: Option<&str>) -> RawVariable {
        RawVariable {
            id: id.to_string(),
            concept_id: concept_id.to_string(),
            value_type: PrimitiveType::Number,
            observed: true,
            value: Value::from(value),
            unit_raw: unit.map(str::to_string),
            expression_raw: None,
            evidence_refs: vec!["ev".to_string()],
        }
    }

    fn expression_variable(id: &str, concept_id: &str, expression: &str) -> RawVariable {
        RawVariable {
            id: id.to_string(),
            concept_id: concept_id.to_string(),
            value_type: PrimitiveType::Expression,
            observed: true,
            value: Value::Null,
            unit_raw: None,
            expression_raw: Some(expression.to_string()),
            evidence_refs: vec!["ev".to_string()],
        }
    }

    #[test]
    fn missing_concept_is_unknown_not_false() {
        let mut state = CompilerState {
            id: "state".to_string(),
            role: None,
            entries: BTreeMap::new(),
            result_refs: vec![],
            evidence_refs: vec![],
        };
        state.entries.insert(
            "representation.fourier.enabled".to_string(),
            StateEntry {
                raw_concept: "fourier features".to_string(),
                canonical_concept_id: "representation.fourier.enabled".to_string(),
                raw_value: Value::Bool(true),
                unit_raw: None,
                expression_raw: None,
                value: StateValue::Bool(true),
            },
        );

        // Present dimension resolves to its value…
        assert_eq!(state.get("representation.fourier.enabled"), Some(&state.entries["representation.fourier.enabled"]));
        // …but an absent dimension is unknown, not false.
        assert_eq!(state.get("sampling.adaptive.enabled"), None);
        assert!(!state.contains("sampling.adaptive.enabled"));
    }

    #[test]
    fn resolves_bool_number_and_expression_dimensions() {
        let extraction = ExtractionV3 {
            variables: vec![
                variable(
                    "v_fourier",
                    "fourier features",
                    PrimitiveType::Bool,
                    Value::Bool(true),
                ),
                number_variable("v_sigma", "fourier sigma", 10.0, None),
                expression_variable("v_pde", "residual expression", "u_t + u_x"),
            ],
            ..ExtractionV3::new()
        };
        let raw = RawState {
            id: "state_proposed".to_string(),
            role: Some(StateRole::Proposed),
            variable_refs: vec![
                "v_fourier".to_string(),
                "v_sigma".to_string(),
                "v_pde".to_string(),
            ],
            result_refs: vec![],
            evidence_refs: vec![],
        };

        let state = resolve_state(&raw, &extraction, &ConceptCanonicalizer::pinn_seed()).unwrap();

        assert_eq!(state.role, Some(StateRole::Proposed));
        assert_eq!(state.len(), 3);
        assert_eq!(
            state.get("representation.fourier.enabled").unwrap().value,
            StateValue::Bool(true)
        );
        assert_eq!(
            state.get("representation.fourier.sigma").unwrap().value,
            StateValue::Number(10.0)
        );
        assert!(matches!(
            state.get("residual.expression").unwrap().value,
            StateValue::Expression(_)
        ));
    }

    #[test]
    fn megapascal_number_is_canonicalized_to_pascal() {
        let extraction = ExtractionV3 {
            variables: vec![number_variable(
                "v_stress",
                "pressure",
                80.0,
                Some("MPa"),
            )],
            ..ExtractionV3::new()
        };
        let raw = RawState {
            id: "state".to_string(),
            role: None,
            variable_refs: vec!["v_stress".to_string()],
            result_refs: vec![],
            evidence_refs: vec![],
        };

        let state = resolve_state(&raw, &extraction, &ConceptCanonicalizer::pinn_seed()).unwrap();
        let entry = state.get("pressure").unwrap();
        assert_eq!(entry.value, StateValue::Number(80_000_000.0));
        assert_eq!(entry.unit_raw.as_deref(), Some("MPa"));
        assert_eq!(entry.raw_value, Value::from(80.0));
    }

    #[test]
    fn dangling_variable_ref_is_ref_001() {
        let extraction = ExtractionV3::new();
        let raw = RawState {
            id: "state".to_string(),
            role: None,
            variable_refs: vec!["missing".to_string()],
            result_refs: vec![],
            evidence_refs: vec![],
        };

        let report = resolve_state(&raw, &extraction, &ConceptCanonicalizer::pinn_seed())
            .unwrap_err();
        assert!(report.errors.iter().any(|e| e.code == "REF-001"));
    }

    #[test]
    fn type_mismatch_is_state_001() {
        let extraction = ExtractionV3 {
            variables: vec![variable(
                "v_bad",
                "fourier features",
                PrimitiveType::Bool,
                Value::from(1),
            )],
            ..ExtractionV3::new()
        };
        let raw = RawState {
            id: "state".to_string(),
            role: None,
            variable_refs: vec!["v_bad".to_string()],
            result_refs: vec![],
            evidence_refs: vec![],
        };

        let report = resolve_state(&raw, &extraction, &ConceptCanonicalizer::pinn_seed())
            .unwrap_err();
        assert!(report.errors.iter().any(|e| e.code == "STATE-001"));
    }

    #[test]
    fn conflicting_values_for_one_concept_is_exp_002() {
        let extraction = ExtractionV3 {
            variables: vec![
                variable(
                    "v_a",
                    "fourier feature encoding",
                    PrimitiveType::Bool,
                    Value::Bool(true),
                ),
                variable(
                    "v_b",
                    "random fourier feature encoding",
                    PrimitiveType::Bool,
                    Value::Bool(false),
                ),
            ],
            ..ExtractionV3::new()
        };
        let raw = RawState {
            id: "state".to_string(),
            role: None,
            variable_refs: vec!["v_a".to_string(), "v_b".to_string()],
            result_refs: vec![],
            evidence_refs: vec![],
        };

        let report = resolve_state(&raw, &extraction, &ConceptCanonicalizer::pinn_seed())
            .unwrap_err();
        assert!(report.errors.iter().any(|e| e.code == "EXP-002"));
    }
}
