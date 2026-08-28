/**
 * Fiber grouping (handoff-spec.md §46, §47, §48).
 *
 * Mirrors `crates/anyway-schema-v4/src/fiber.rs` types. The Rust crate is
 * authoritative for the grouping algorithm; these types are the cross-boundary
 * contract only.
 */

import type { ConditioningEntry } from "./ir";

export interface ChainProjection {
  chain_id: string;
  conditioning: ConditioningEntry[];
  varying_concepts: string[];
}
