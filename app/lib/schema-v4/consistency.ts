/**
 * Consistency checks (handoff-spec.md §54–§61).
 *
 * Mirrors `crates/anyway-schema-v4/src/consistency.rs` types. The Rust crate
 * is authoritative for the metrics and conflict classification; these types
 * are the cross-boundary contract only.
 */

export type ConflictClass =
  | "contextual_divergence"
  | "axiomatic_divergence"
  | "internal_conflict"
  | "insufficient_resolution";
