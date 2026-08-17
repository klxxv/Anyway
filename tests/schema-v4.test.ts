import assert from "node:assert/strict";
import test from "node:test";
import {
  GRAPH_IR_SCHEMA_VERSION,
  LLM_SCHEMA_VERSION,
  OPERATOR_KINDS,
  type ExtractionV3,
  type CanvasIRV3,
  type OperatorKind,
  type PrimitiveType,
} from "../app/lib/schema-v4";

test("schema v4 version constants are stable", () => {
  assert.equal(LLM_SCHEMA_VERSION, "myc.llm.v4");
  assert.equal(GRAPH_IR_SCHEMA_VERSION, "myc.graph-ir.v4");
});

test("operator basis is the five-kind set in a stable order", () => {
  assert.deepEqual(OPERATOR_KINDS, ["T", "K", "I", "M", "Q"]);
  const kinds = new Set<OperatorKind>(OPERATOR_KINDS);
  assert.equal(kinds.size, 5);
});

test("extraction root accepts a minimal PINN document", () => {
  const doc: ExtractionV3 = {
    schema_version: LLM_SCHEMA_VERSION,
    document: { document_id: "doc_pinn_001", source_type: "paper" },
    evidence: [
      {
        id: "ev_001",
        document_id: "doc_pinn_001",
        location: { page: 5, section: "Method" },
        text_span: "We introduce Fourier features with sigma = 10.",
        verification: { status: "supported", confidence: 0.98 },
      },
    ],
    variables: [
      {
        id: "var_001",
        concept_id: "representation.fourier.enabled",
        value_type: "bool",
        observed: true,
        value: true,
        unit_raw: null,
        expression_raw: null,
        evidence_refs: ["ev_001"],
      },
    ],
    contexts: [],
    axiom_sets: [],
    experiments: [],
    operator_candidates: [],
    abstraction_candidates: [],
  };
  assert.equal(doc.variables[0].value_type, "bool");
});

test("ir root accepts a minimal compiled graph", () => {
  const ir: CanvasIRV3 = {
    schema_version: GRAPH_IR_SCHEMA_VERSION,
    blocks: [],
    operators: [
      {
        id: "op_001",
        operator: "I",
        input_refs: ["block_state_A"],
        output_refs: ["block_state_B"],
        payload: { changes: [] },
        evidence_refs: ["ev_001"],
        semantic_hash: "sha256:cc",
        instance_hash: "sha256:dd",
      },
    ],
    chains: [],
    fibers: [],
    bundles: [],
    identifiability: [],
    consistency_checks: [],
    provenance_index: { ev_001: ["op_001"] },
  };
  assert.equal(ir.operators[0].operator, "I");
});

test("primitive type union is exactly three members", () => {
  const primitives: PrimitiveType[] = ["bool", "number", "expression"];
  assert.equal(primitives.length, 3);
});
