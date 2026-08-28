/**
 * Identifiability engine (handoff-spec.md §50, §51).
 *
 * Mirrors `crates/anyway-schema-v4/src/identifiability.rs`. The Rust crate is
 * authoritative for the assessment algorithm; this is the cross-boundary
 * contract only.
 */

import type {
  ComponentEffect,
  EffectStatus,
  InteractionEntry,
} from "./ir";
import type { FactorialControl } from "./matcher";

export interface Assessment {
  joint: EffectStatus;
  components: ComponentEffect[];
  interactions: InteractionEntry[];
  missing_controls: FactorialControl[];
}
