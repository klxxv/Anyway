/**
 * Compiler-side State model (handoff-spec.md §21, §24, §25).
 *
 * Mirrors `crates/anyway-schema-v4/src/state.rs`. A state is a sparse
 * configuration: a concept absent from `entries` is unknown, never `false`.
 * The Rust crate is authoritative for resolution/diff; these types are the
 * cross-boundary contract only.
 */

import type { JsonValue } from "./index";
import type { StateRole } from "./extract";

export type StateValue =
  | { type: "bool"; value: boolean }
  | { type: "number"; value: number }
  | { type: "expression"; value: string };

export interface StateEntry {
  raw_concept: string;
  canonical_concept_id: string;
  raw_value: JsonValue;
  unit_raw?: string | null;
  expression_raw?: string | null;
  value: StateValue;
}

export interface CompilerState {
  id: string;
  role?: StateRole | null;
  entries: Record<string, StateEntry>;
  result_refs: string[];
  evidence_refs: string[];
}
