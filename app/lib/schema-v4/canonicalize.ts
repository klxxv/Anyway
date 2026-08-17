/**
 * Concept / unit / expression canonicalization (handoff-spec.md §70–§74).
 *
 * Mirrors `crates/anyway-schema-v4/src/canonicalize.rs`. The Rust crate is
 * authoritative for the deterministic algorithm; these types are the
 * cross-boundary contract only. Raw phrases, values, and expressions always
 * remain recoverable (V3-15, V3-16).
 */

export type MappingType = "exact" | "alias" | "new_child" | "unresolved";

export interface CanonicalizationRecord {
  raw_concept: string;
  canonical_concept_id: string;
  mapping_type: MappingType;
  confidence: number;
}

export interface UnitCanonicalization {
  value_raw: number;
  unit_raw: string | null;
  value_canonical: number;
  unit_canonical: string | null;
}

export interface ExpressionNormalization {
  raw: string;
  normalized: string;
  raw_hash: string;
  normalized_hash: string;
}
