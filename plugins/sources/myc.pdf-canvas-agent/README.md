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

Each slot references host-owned settings for `provider`, `credential`, `model`, and `thinking` under `agent.models.<slot>.*`. The plugin package contains no API key or credential value. The host must resolve the credential reference from its secure credential store and must never expose the raw secret to the plugin or put it in a job checkpoint.

The plugin may declare defaults and user-editable setting metadata in the plugin manifest. This `agent.yml` only binds those settings to model slots; provider selection, credential resolution, Thinking capability checks, rate limits, retries, and request redaction remain host responsibilities.

## Host/plugin boundary

The host owns:

- PDF file selection, authorization, size limits, and basic extraction services;
- model HTTP calls through a provider gateway;
- API key and credential storage;
- model and Thinking capability validation;
- job state, cancellation, checkpoints, retry and recovery;
- Draft 2020-12 schema validation and Pass F deterministic checks;
- GraphPatch validation, review UI, and project writes.

The plugin owns:

- this DAG and its input bindings;
- prompt text and localization;
- output schemas and field naming;
- PDF semantic pass policy;
- the domain-level intermediate representation and GraphPatch mapping policy.

The plugin must not use arbitrary filesystem access, direct network access, raw credentials, or direct graph-store writes. Its final output is review-gated and cannot bypass the host review flow.

## Inputs and outputs

The host supplies `application/pdf` through `host.pdf.extract`. The normalized input is expected to expose:

- `structuredDocument`;
- `fullText`;
- `documentMap`;
- the host-derived `sectionText(sectionId)` service;
- host-derived experiment-related paragraphs where available.

Pass A-E outputs are aligned with `crates/semantic-pipeline/src/ir.rs` and use camelCase Serde names. In particular:

- Pass A uses `abstractText`;
- Pass E uses `conclusionType`;
- Pass F consumes merged `AgentCandidates`;
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

Every schema is Draft 2020-12. Objects use `additionalProperties: false` unless the corresponding Rust IR field is an open `serde_json::Value`, such as `ivSettings`, node `data`, edge `data`, and operation `changes`. Optional Rust `Option<T>` fields may be omitted or set to `null`; non-optional IR fields are required.

`agent-candidates.schema.json` references the item definitions in the Pass B-E schemas. The host schema loader must resolve relative `$ref` values within the plugin package and reject references that escape the package root.

## Current integration limitation

These assets are now present in the plugin source package, but the current Rust host still executes its existing built-in PDF path and reads `config/prompts`. The host does not yet discover `agent.yml`, resolve this DAG, inject model-slot settings, or run the plugin-owned schemas. A follow-up host integration must add:

- package-relative `agent.yml` discovery and signature/version validation;
- model-slot settings resolution with secure credential references;
- provider gateway calls that return structured JSON to the declared stage;
- prompt rendering and schema loading from the package;
- Pass F validation/transform dispatch;
- checkpoint metadata for pipeline version, prompt version, model id, and non-secret settings snapshot;
- review-gated GraphPatch handoff to the existing review UI.

No host implementation is modified by this asset change.
