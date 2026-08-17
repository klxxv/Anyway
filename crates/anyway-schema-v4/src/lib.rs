//! Anyway Schema v4 data models.
//!
//! Two versioned root contracts (handoff-spec.md):
//!
//! - `myc.llm.v4`     — LLM extraction output ([`extract::ExtractionV3`]).
//! - `myc.graph-ir.v4` — deterministic compiler IR ([`ir::CanvasIRV3`]).
//!
//! The central boundary: **the LLM extracts semantics; the compiler
//! constructs computation.** These types are the stable cross-module contract;
//! storage, model providers, and graph engines may change behind them.

pub mod canonicalize;
pub mod compiler;
pub mod extract;
pub mod hash;
pub mod intervention;
pub mod ir;
pub mod matcher;
pub mod reference;
pub mod state;
pub mod state_diff;
pub mod validator;

use serde::{Deserialize, Serialize};

/// Root schema version for the LLM extraction contract.
pub const LLM_SCHEMA_VERSION: &str = "myc.llm.v4";

/// Root schema version for the compiled graph IR contract.
pub const GRAPH_IR_SCHEMA_VERSION: &str = "myc.graph-ir.v4";

/// The finite operator basis shared by extraction candidates and compiled IR.
///
/// ```text
/// T = Transform        (deterministic:  y = T(x))
/// K = Kernel           (conditional/stochastic: K(Y | X, C, A))
/// I = Intervention     (configuration change: I: X0 -> X1)
/// M = Marginalization  (aggregation: M: X_fine -> X_coarse)
/// Q = Quotient         (abstraction: Q: X -> X/~)
/// ```
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OperatorKind {
    T,
    K,
    I,
    M,
    Q,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_kind_round_trips_single_letters() {
        for (kind, wire) in [
            (OperatorKind::T, "\"T\""),
            (OperatorKind::K, "\"K\""),
            (OperatorKind::I, "\"I\""),
            (OperatorKind::M, "\"M\""),
            (OperatorKind::Q, "\"Q\""),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), wire);
            assert_eq!(serde_json::from_str::<OperatorKind>(wire).unwrap(), kind);
        }
    }

    #[test]
    fn schema_version_constants_are_stable() {
        assert_eq!(LLM_SCHEMA_VERSION, "myc.llm.v4");
        assert_eq!(GRAPH_IR_SCHEMA_VERSION, "myc.graph-ir.v4");
    }
}
