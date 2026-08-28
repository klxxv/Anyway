/**
 * Compiler StateDiff (handoff-spec.md §25).
 *
 * Mirrors `crates/anyway-schema-v4/src/state_diff.rs`. Bool Δ∈{-1,0,+1},
 * Number Δ = to - from, Expression Δ = (from, to). A dimension present on only
 * one side is unknown and never a confirmed intervention dimension.
 */

import type { StateValue } from "./state";

export type Side = "from" | "to";

export interface BoolDiff {
  concept_id: string;
  from: boolean;
  to: boolean;
  delta: -1 | 0 | 1;
}

export interface NumberDiff {
  concept_id: string;
  from: number;
  to: number;
  delta: number;
}

export interface ExpressionDiff {
  concept_id: string;
  from: string;
  to: string;
  changed: boolean;
}

export interface UnconfirmedDimension {
  concept_id: string;
  side: Side;
  value: StateValue;
}

export interface TypeConflict {
  concept_id: string;
  from_type: string;
  to_type: string;
}

export interface StateDiff {
  from_id: string;
  to_id: string;
  bool_diffs: BoolDiff[];
  number_diffs: NumberDiff[];
  expression_diffs: ExpressionDiff[];
  unconfirmed_dimensions: UnconfirmedDimension[];
  type_conflicts: TypeConflict[];
}
