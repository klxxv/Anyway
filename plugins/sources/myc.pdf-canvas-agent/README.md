# PDF Canvas Agent declarative Harness

This directory contains the versioned, plugin-owned Harness assets for the PDF Canvas Agent. The assets describe the semantic pipeline; they do not implement the host security boundary.

## Pipeline

`agent.yml` declares the following DAG:

```text
Pass A: structure extraction ───────────────┐
                                             ├─ Pass E: paper synthesis ─┐
Pass B: section entity extraction ─┬────────┘                            │
                                  └─ Pass C: variable fission ─┬────────┘
                                                               └─ Pass D: merge
                                                                            │
                                                                            v
                                                   Pass F: local validation + transform
                                                                            │
                                                                            v
                                      reviewRequired GraphPatch (v1alpha1)
```

Pass A and Pass B are independent and may run concurrently. Pass C depends on B, Pass D depends on B and C, and Pass E depends on A-D. Pass F is local validation and transformation; it does not call an LLM.

## Model slots and user settings

The pipeline declares three model slots:

- `extraction`: Pass A-D;
- `synthesis`: Pass E;
- `recovery`: truncated JSON recovery and retry handling.

Each slot references the plugin-owned native `kimi-k2.6` provider profile plus host-owned settings for `credential`, `model`, and `thinking` under `agent.models.<slot>.*`. The plugin package contains no API key or credential value. The host must resolve the credential reference from its secure credential store and must never expose the raw secret to the plugin or put it in a job checkpoint.

The plugin declares Kimi K2.6 as its native provider profile in `agent.yml`, with user-selectable OpenAI SSE (`/v1`) and Anthropic SSE (`/anthropic`) parser backends. Each selection changes the request shape, authentication headers, endpoint path, and stream event parser together. The profile also declares the `thinking.type` contract, omission of incompatible sampling fields, and the Kimi Files `purpose=file-extract` flow. File upload always uses the regional `/v1/files` API even when chat uses the Anthropic-compatible endpoint. Local extraction remains the safe default; Kimi Files is used only after an explicit user selection.

## Host/plugin boundary

The host owns:

- PDF file selection, authorization, size limits, and basic extraction services;
- model HTTP calls through a provider gateway;
- API key and current-session credential handling;
- credential resolution and execution of the plugin-declared Kimi K2.6 request policy;
- job state, cancellation, checkpoints, retry and recovery;
- Draft 2020-12 schema validation and Pass F deterministic checks;
- GraphPatch validation, review UI, and project writes.

The plugin owns:

- this DAG and its input bindings;
- prompt text and localization;
- output schemas and field naming;
- PDF semantic pass policy;
- the native Kimi K2.6 provider/request profile, including thinking and file-extract metadata;
- the domain-level intermediate representation and GraphPatch mapping policy.

The plugin must not use arbitrary filesystem access, direct network access, raw credentials, or direct graph-store writes. Its final output is review-gated and cannot bypass the host review flow.

## Inputs and outputs

The host supplies `application/pdf` through `host.pdf.extract`. The normalized input is expected to expose:

- `structuredDocument`;
- `fullText`;
- `documentMap`;
- the host-derived `sectionText(sectionId)` service;
- host-derived experiment-related paragraphs where available.

Pass A output stays camelCase (`abstractText`). Pass B fragments and the Pass E root
follow the `myc.llm.v4` snake_case contract (`crates/anyway-schema-v4/src/extract.rs`).
In particular:

- Pass A uses `abstractText`;
- Pass B emits per-section `ExtractionV3` fragments (evidence, variables, contexts, axiom_sets, experiments, operator_candidates, abstraction_candidates);
- Pass E emits the complete `myc.llm.v4` root with `schema_version`;
- the host compiles the root deterministically into `myc.graph-ir.v4` (`CanvasIRV3`) and persists it through `graph.storage.put`;
- the final GraphPatch uses `researchcanvas.dev/graph-patch/v1alpha1` and `reviewRequired: true`.

`GraphPatch` operations are proposals only. The host must apply them only after user review.

## Failure and partial-result policy

The declared policy is:

1. Retry a failed model call up to two times.
2. Use the `recovery` slot for truncated JSON before normal validation.
3. Keep successful pass outputs when a later pass fails, and mark the Job as incomplete.
4. Do not emit an applicable GraphPatch when Pass F has an error-level validation failure.
5. Warnings such as unverifiable quotes may be retained in the validation report, but must be visible in the review surface.
6. Every emitted patch remains `reviewRequired: true`.

Partial results are diagnostic/intermediate artifacts until the host has validated their references and produced a GraphPatch.

## Schema policy

Every schema is Draft 2020-12. Objects use `additionalProperties: false` unless the corresponding Rust IR field is an open `serde_json::Value`, such as operator `payload`, node `data`, edge `data`, and operation `changes`. Optional Rust `Option<T>` fields may be omitted or set to `null`; non-optional IR fields are required.

`pass-b-v4.schema.json` and `pass-e-v4.schema.json` mirror `crates/anyway-schema-v4/src/extract.rs` (`ExtractionV3`, `myc.llm.v4`) with snake_case field names. The host schema loader must resolve relative `$ref` values within the plugin package and reject references that escape the package root.

## myc.llm.v4 pipeline

The extraction contract is the schema-v4 root `myc.llm.v4`:

1. **Pass A** — paper structure (title, authors, abstract, section tree, references).
2. **Pass B** — one `ExtractionV3` fragment per section (evidence, variables, contexts, axiom sets, experiments, operator candidates, abstraction candidates).
3. **Pass E** — one complete `ExtractionV3` root with `schema_version: "myc.llm.v4"`.
4. **Pass F (host bus)** — the host validates the root, compiles it deterministically via `graph.ir.compile` into `myc.graph-ir.v4` (`CanvasIRV3`), persists the canvas through `graph.storage.put`, publishes progress through `event.publish`, and converts the compiled canvas into a `reviewRequired: true` GraphPatch for the review UI.

The LLM only extracts what the source states; variable merging, state diffs, joint interventions, identifiability, and abstraction promotion are the deterministic compiler's work.
