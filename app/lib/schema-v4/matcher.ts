/**
 * Historical neighbor matcher (handoff-spec.md §52, §53).
 *
 * Mirrors `crates/anyway-schema-v4/src/matcher.rs` types. The Rust crate is
 * authoritative for the distance/ranking algorithm; these types are the
 * cross-boundary contract only.
 */

export type ExpressionMatchLevel =
  | "no_match"
  | "candidate_similar"
  | "symbolic"
  | "structural"
  | "exact";

export interface StateDistance {
  bool_distance: number;
  number_distance: number;
  expression_mismatches: number;
}

export interface MatchResult {
  state_id: string;
  distance: StateDistance;
}

export interface FactorialControl {
  assignments: Record<string, boolean>;
}
