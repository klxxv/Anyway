/**
 * `myc.graph-ir.v4` — compiled canvas IR (CanvasIRV3).
 *
 * Mirrors `crates/anyway-schema-v4/src/ir.rs`. The compiler answers "what
 * computational structure follows from these extracted facts?". This
 * representation is computational; LLM output is evidential.
 */

import type { JsonValue, OperatorKind } from "./index";

export type BlockType = "variable" | "state" | "result" | "concept" | "axiom";
export type EffectStatus =
  | "identifiable"
  | "partially_identifiable"
  | "unresolved"
  | "confounded"
  | "insufficient_evidence";
export type CheckType = "path" | "representation" | "branch" | "abstraction" | "conflict";

export interface CanvasIRV3 {
  schema_version: string;
  blocks: Block[];
  operators: Operator[];
  chains: Chain[];
  fibers: Fiber[];
  bundles: Bundle[];
  identifiability: Identifiability[];
  consistency_checks: ConsistencyCheck[];
  provenance_index: ProvenanceIndex;
}

export interface Block {
  id: string;
  block_type: BlockType;
  concept_id?: string;
  variable_ref?: string;
  member_refs: string[];
  context_ref?: string;
  axiom_set_ref?: string;
  semantic_hash: string;
  instance_hash: string;
}

export interface Operator {
  id: string;
  operator: OperatorKind;
  input_refs: string[];
  output_refs: string[];
  payload: JsonValue;
  context_ref?: string;
  axiom_set_ref?: string;
  evidence_refs: string[];
  semantic_hash: string;
  instance_hash: string;
}

export interface Chain {
  id: string;
  block_path: string[];
  operator_path: string[];
  context_ref?: string;
  axiom_set_ref?: string;
  source_experiment_refs: string[];
  semantic_hash: string;
  instance_hash: string;
}

export interface Fiber {
  id: string;
  conditioning: ConditioningEntry[];
  varying_concepts: string[];
  chain_refs: string[];
  semantic_hash: string;
}

export interface ConditioningEntry {
  concept_id: string;
  semantic_value_hash: string;
}

export interface Bundle {
  id: string;
  target_concepts: string[];
  fiber_refs: string[];
  varying_dimensions: string[];
  semantic_hash: string;
}

export interface Identifiability {
  id: string;
  target_ref: string;
  intervention_ref: string;
  joint_effect: EffectStatusEntry;
  component_effects: ComponentEffect[];
  interactions: InteractionEntry[];
  missing_controls: MissingControl[];
}

export interface EffectStatusEntry {
  status: EffectStatus;
}

export interface ComponentEffect {
  concept_id: string;
  status: EffectStatus;
}

export interface InteractionEntry {
  concept_refs: string[];
  status: EffectStatus;
}

export interface MissingControl {
  configuration: JsonValue;
}

export interface ConsistencyCheck {
  id: string;
  check_type: CheckType;
  input_refs: string[];
  metric?: string;
  value?: number;
  threshold?: number;
  status: string;
  details: JsonValue;
}

/** Reverse lookup from evidence id to derived graph object ids. */
export type ProvenanceIndex = Record<string, string[]>;
