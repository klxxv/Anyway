/**
 * Anyway Schema v4 — shared constants and the finite operator basis.
 *
 * Mirrors `crates/anyway-schema-v4/src/lib.rs`. Two versioned root contracts:
 * `myc.llm.v4` (LLM extraction) and `myc.graph-ir.v4` (compiled graph IR).
 * The LLM extracts semantics; the compiler constructs computation.
 */

export const LLM_SCHEMA_VERSION = "myc.llm.v4" as const;
export const GRAPH_IR_SCHEMA_VERSION = "myc.graph-ir.v4" as const;

/**
 * The five-operator basis (handoff-spec.md §28, mvp-architecture.md §3):
 * T=Transform, K=Kernel, I=Intervention, M=Marginalization, Q=Quotient.
 */
export type OperatorKind = "T" | "K" | "I" | "M" | "Q";

export const OPERATOR_KINDS: readonly OperatorKind[] = ["T", "K", "I", "M", "Q"];

export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export * from "./canonicalize";
export * from "./compiler";
export * from "./extract";
export * from "./intervention";
export * from "./ir";
export * from "./matcher";
export * from "./state";
export * from "./state_diff";
