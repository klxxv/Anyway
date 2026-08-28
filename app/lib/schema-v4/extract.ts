/**
 * `myc.llm.v4` — LLM extraction schema (ExtractionV3).
 *
 * Mirrors `crates/anyway-schema-v4/src/extract.rs`. The extractor answers
 * "what is explicitly present in the source?" and never performs final
 * mathematical judgment.
 */

import type { JsonValue, OperatorKind } from "./index";

export type PrimitiveType = "bool" | "number" | "expression";
export type EvidenceStatus = "supported" | "ambiguous" | "unsupported";
export type StateRole =
  | "baseline"
  | "proposed"
  | "ablation"
  | "control"
  | "variant"
  | "reference";

export interface ExtractionV3 {
  schema_version: string;
  document?: Document;
  evidence: Evidence[];
  variables: Variable[];
  contexts: Context[];
  axiom_sets: AxiomSet[];
  experiments: Experiment[];
  operator_candidates: OperatorCandidate[];
  abstraction_candidates: AbstractionCandidate[];
}

export interface Document {
  document_id: string;
  title?: string;
  authors?: string[];
  year?: number;
  doi?: string;
  arxiv_id?: string;
  url?: string;
  source_type: string;
}

export interface Evidence {
  id: string;
  document_id: string;
  location?: EvidenceLocation;
  text_span: string;
  verification: Verification;
}

export interface EvidenceLocation {
  page?: number;
  section?: string;
  paragraph?: number;
  table?: string;
  figure?: string;
  equation?: string;
}

export interface Verification {
  status: EvidenceStatus;
  confidence: number;
}

export interface Variable {
  id: string;
  concept_id: string;
  value_type: PrimitiveType;
  observed: boolean;
  value: JsonValue;
  unit_raw: string | null;
  expression_raw: string | null;
  evidence_refs: string[];
}

export interface Context {
  id: string;
  variable_refs: string[];
  evidence_refs: string[];
}

export interface AxiomSet {
  id: string;
  constraint_refs: string[];
  evidence_refs: string[];
}

export interface Experiment {
  id: string;
  context_ref?: string;
  axiom_set_ref?: string;
  states: State[];
  comparisons: StateComparison[];
  evidence_refs: string[];
}

export interface State {
  id: string;
  role?: StateRole;
  variable_refs: string[];
  result_refs: string[];
  evidence_refs: string[];
}

export interface StateComparison {
  from_state: string;
  to_state: string;
  evidence_refs: string[];
}

export interface OperatorCandidate {
  id: string;
  operator: OperatorKind;
  input_refs: string[];
  output_refs: string[];
  payload: JsonValue;
  context_ref?: string;
  axiom_set_ref?: string;
  evidence_refs: string[];
  verification: Verification;
}

export interface AbstractionCandidate {
  id: string;
  input_concept_ids: string[];
  proposed_concept_id: string;
  rationale_evidence_refs: string[];
  /** Allowed LLM status is exactly `"candidate"`. */
  status: string;
}
