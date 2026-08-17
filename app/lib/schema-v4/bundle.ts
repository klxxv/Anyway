/**
 * Bundle grouping (handoff-spec.md §49).
 *
 * Mirrors `crates/anyway-schema-v4/src/bundle.rs` types. The Rust crate is
 * authoritative for the grouping algorithm; these types are the cross-boundary
 * contract only.
 */

export interface FiberTarget {
  fiber_id: string;
  target_concepts: string[];
}
