/**
 * Deterministic compiler contract (Step 8 of 16).
 *
 * Mirrors `crates/anyway-schema-v4/src/compiler.rs`. The legacy 12-edge
 * convergence (§2.3 of implementation-plan.md) is mirrored here so the
 * frontend can migrate existing canvas edges onto the five-operator basis.
 * The Rust crate is authoritative for block/operator construction.
 */

export type EdgeConvergence =
  | { kind: "evidence" }
  | { kind: "kernel"; requires_intervention: boolean }
  | { kind: "transform" }
  | { kind: "marginalize" };

/**
 * Map a legacy canvas edge type onto the five-operator basis. Returns `null`
 * for an unrecognized edge type (rejected during migration).
 */
export function convergeEdge(edgeType: string): EdgeConvergence | null {
  switch (edgeType) {
    case "supports":
    case "contradicts":
      return { kind: "evidence" };
    case "causes":
    case "controls":
      return { kind: "kernel", requires_intervention: true };
    case "mediates":
    case "moderates":
    case "correlates":
      return { kind: "kernel", requires_intervention: false };
    case "depends_on":
    case "derived_from":
    case "uses":
    case "measures":
      return { kind: "transform" };
    case "part_of":
      return { kind: "marginalize" };
    default:
      return null;
  }
}

/** Deterministic block-id prefixes (mirrors `compiler.rs`). */
export const variableBlockId = (id: string): string => `block_var_${id}`;
export const resultBlockId = (id: string): string => `block_result_${id}`;
export const stateBlockId = (id: string): string => `block_state_${id}`;
export const conceptBlockId = (id: string): string => `block_concept_${id}`;
export const axiomBlockId = (id: string): string => `block_axiom_${id}`;
