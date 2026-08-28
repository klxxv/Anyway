# Architecture Decision Log

This file records durable repository-wide decisions. Detailed protocol and
component specifications remain under `docs/architecture/`; this log explains
which constraints are accepted and why.

Statuses are **Proposed**, **Accepted**, **Superseded**, or **Rejected**. To
change an accepted decision, add a new entry that names the superseded decision
instead of silently rewriting history. The initial entries below recover
decisions already enforced by the repository as of 2026-08-29.

## D-001 — Keep the product local-first and human-led

- **Status:** Accepted
- **Date:** 2026-08-29 (documented from the existing baseline)
- **Context:** The research graph is a durable record of hypotheses, methods,
  evidence, and results. Model output can be useful without becoming implicit
  application truth.
- **Decision:** Store projects locally, make deterministic graph operations the
  source of derived structure, and require a human-visible review step before
  agent-proposed graph mutations become durable.
- **Consequences:** Offline work and export remain first-class. Agent output is
  a proposal or evidence, not an authoritative graph state.

## D-002 — Keep the privileged and deterministic boundary in Rust

- **Status:** Accepted
- **Date:** 2026-08-29 (documented from the existing baseline)
- **Context:** Identity, authorization, package activation, Blob access, and
  graph commits must not be bypassable by UI or plugin code.
- **Decision:** Rust owns the privileged kernel and canonical graph compiler.
  TypeScript graph behavior remains parity-tested while compatibility code
  exists; fixed inputs must produce bit-identical canonical results.
- **Consequences:** Frontend and plugin layers request operations through typed
  adapters. Kernel behavior changes require Rust tests and compiler parity
  coverage where the TypeScript implementation overlaps.
- **References:** `docs/module-boundaries.md`,
  `docs/architecture/anyway-kernel-host-architecture.md`

## D-003 — Use one Host SDK vocabulary without sharing authority

- **Status:** Accepted
- **Date:** 2026-08-29 (documented from the existing baseline)
- **Context:** Native UI, official extensions, community plugins, and language
  workers need common operations but have different trust levels.
- **Decision:** Route versioned requests through the Host gateway and preserve
  the original principal, delegated capabilities, deadlines, cancellation, and
  audit context end to end. Every Host operation reauthorizes the caller.
- **Consequences:** A proxy or extension host cannot lend its authority to a
  child. New operations require a capability mapping, admission limits, and
  transport tests.
- **References:** `docs/architecture/host-gateway.md`,
  `docs/architecture/host-bus-runtime.md`,
  `docs/architecture/capability-policy.md`

## D-004 — Separate plugin source, staged runtime, and installed state

- **Status:** Accepted
- **Date:** 2026-08-29 (documented from the existing baseline)
- **Context:** Loading editable source trees or stale packages makes runtime
  behavior non-reproducible and can bypass package validation.
- **Decision:** Keep official source in `my-plugins/`, canonical distributables
  in `plugins/packages/`, generated dev/test/release state in
  `.plugin-runtime/`, and formal installed state in application data.
  `config/plugin-loading.json` is the staging allowlist.
- **Consequences:** A source change is not runnable until explicitly packed or
  staged. Release version changes must update every manifest, reference, test,
  and canonical package together.
- **References:** `docs/architecture/plugin-runtime-layout.md`,
  `docs/architecture/plugin-system-v2.md`

## D-005 — Provide trusted native UI and untrusted declarative UI lanes

- **Status:** Accepted
- **Date:** 2026-08-29 (documented from the existing baseline)
- **Context:** Official audited features need full Vue ergonomics, while
  community plugins must not inject arbitrary executable renderer code.
- **Decision:** Allow ordinary Vue components only in the trusted native lane.
  Untrusted contributions use versioned Vue UI IR rendered through a static
  allowlist and known Host slots.
- **Consequences:** Raw HTML, scripts, arbitrary component names, dynamic
  imports, executable handlers, and unrestricted styling do not cross the
  untrusted boundary.
- **References:** `docs/architecture/vue-ui-ir.md`

## D-006 — Make every agent graph write review-gated

- **Status:** Accepted
- **Date:** 2026-08-29 (documented from the existing baseline)
- **Context:** Agents can extract and organize research material, but direct
  writes could silently corrupt the research record.
- **Decision:** Agent plugins declare explicit capabilities and submit immutable
  `GraphPatch` proposals. The Rust kernel validates identity, capability,
  schema, base revision, and review state before an atomic commit. Agents never
  receive direct graph-store write access.
- **Consequences:** UI review is part of the data-integrity boundary, not an
  optional presentation step. Tests must reject missing capabilities,
  non-review-gated descriptors, stale revisions, and direct-write attempts.
- **References:** `docs/module-boundaries.md`,
  `docs/architecture/python-worker-rpc.md`

## D-007 — Move bulk data by bounded, authorized Blob reads

- **Status:** Accepted
- **Date:** 2026-08-29
- **Context:** Inline JSON is appropriate for control messages, not PDF payloads.
  Worker stdio still needs explicit frame, memory, deadline, and amplification
  bounds.
- **Decision:** Pass immutable `BlobRef` metadata over RPC and resolve bytes
  through Host-authorized reads. The current worker contract permits 256 KiB
  Blob-read chunks, a 384 KiB encoded result ceiling, at most 128 reverse Host
  calls per parent request, and a 1 MiB frame ceiling. The local dependency-free
  PDF parser remains a 384 KiB acceptance slice; the Kimi file-extraction path
  may read at most 32 MiB under the same parent deadline and verifies the final
  digest before use.
- **Consequences:** These constants are a coordinated protocol contract across
  Rust, Python, tests, and architecture docs. Raising one limit requires an
  explicit resource analysis and boundary tests; possessing a digest never
  grants Blob access.
- **References:** `docs/architecture/blob-inside-rpc.md`,
  `docs/architecture/python-worker-rpc.md`

## New decision template

```markdown
## D-NNN — Short imperative title

- **Status:** Proposed
- **Date:** YYYY-MM-DD
- **Context:** What pressure or conflict requires a durable choice?
- **Decision:** What is the repository choosing?
- **Consequences:** What becomes easier, harder, required, or forbidden?
- **Supersedes:** D-NNN, if applicable
- **References:** Relevant code, issues, PRs, or architecture documents
```
