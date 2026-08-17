/**
 * Joint Intervention compiler (handoff-spec.md §26, §27).
 *
 * Mirrors `crates/anyway-schema-v4/src/intervention.rs`. One multi-variable
 * diff compiles into exactly one joint intervention (V3-06), never split into
 * independent per-dimension effects (V3-07).
 */

import type { OperatorKind } from "./index";
import type { StateValue } from "./state";

export interface Change {
  concept_id: string;
  before: StateValue;
  after: StateValue;
}

export interface JointIntervention {
  id: string;
  operator: OperatorKind;
  from_state: string;
  to_state: string;
  changes: Change[];
}
