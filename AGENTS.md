# Repository Agent Guide

This file applies to the entire repository. A more deeply nested `AGENTS.md`
may add or override instructions for its subtree.

## Product and invariants

Anyway is a local-first, human-led research canvas. The semantic graph is the
research record, so changes must preserve these boundaries:

- Graph structure, layout, and analysis are deterministic. Do not move graph
  truth into prompts or model output.
- Rust is the privileged boundary for identity, authorization, package
  verification, worker supervision, Blob access, review state, and final graph
  commits.
- Plugins and agents may propose a `GraphPatch`; they must not mutate the graph
  store directly. Every durable graph change remains review-gated.
- Bulk data crosses process boundaries as authorized `BlobRef` values. Keep RPC
  frames, chunks, deadlines, and call counts bounded.
- Plugin capabilities are explicit and checked again at the Host boundary.
  Configuration must never grant an undeclared capability.
- Secrets must not enter manifests, logs, persisted project state, frontend
  stores, worker payloads, fixtures, or committed files.

Record changes to these invariants in `DECISIONS.md` and the relevant document
under `docs/architecture/` before changing their implementation.

## Repository map

| Path | Responsibility |
| --- | --- |
| `src/` | Vue 3 desktop renderer, Pinia stores, and trusted UI runtime |
| `app/` | Renderer-independent TypeScript domain, plugin contracts, i18n, and platform adapters |
| `src-tauri/` | Rust kernel, Tauri gateway, graph compiler, plugin installer, Host Bus, and workers |
| `my-plugins/` | Version-controlled sources for official plugins |
| `plugins/sdk/` | Cross-language plugin and worker SDK contracts |
| `plugins/packages/` | Deliberately tracked canonical `.myc` release artifacts |
| `config/plugin-loading.json` | Authority for dev, test, and release package staging |
| `tests/` | TypeScript integration, contract, workspace, and parity tests |
| `docs/architecture/` | Normative architecture and protocol documentation |

Generated directories such as `node_modules/`, `dist/`, `target/`, and
`.plugin-runtime/` are not source and must not be committed.

## Toolchain and common commands

Use Node.js 22.13 or newer, the Rust stable toolchain, and Python 3.

```bash
npm ci
npm run dev:desktop-web
npm run desktop:dev
npm run build
npm run lint
npm run typecheck:vue
```

Prefer `rg` and `rg --files` for repository discovery. Keep edits focused and
preserve unrelated working-tree changes.

## Verification

Run the smallest relevant checks while iterating, then broaden verification in
proportion to the change:

| Change area | Minimum checks |
| --- | --- |
| TypeScript domain or renderer | `npm run lint`, `npm run typecheck:vue`, and the affected `tsx --test` suite |
| Core graph behavior | `npm run test:core` and `npm run test:compiler` |
| Platform or plugin contracts | `npm run test:platform` |
| Workspace/UI behavior | `npm run test:workspace` |
| Python/C++/Rust SDKs | `npm run test:sdk` |
| Rust kernel or Tauri commands | `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check` and `npm run test:native` |
| Plugin packaging or staging | `npm run test:platform`, `npm run test:sdk`, and `npm run plugins:stage-test` |
| Cross-cutting or release work | `npm test` |

Tests that enforce a security or compatibility boundary should accompany the
implementation change. Never weaken a limit or skip a gate solely to make a
test pass.

## Plugin changes

For an official plugin release, keep the version and package identity aligned
across its source manifests, language metadata, frontend identifiers,
`config/plugin-loading.json`, documentation, tests, and the checked-in `.myc`
artifact. Build with the canonical packager:

```bash
node scripts/pack-plugin.mjs <source-directory> plugins/packages/<id>@<version>.myc
```

Do not treat `my-plugins/` as an installed runtime. Exercise packages through
the explicit staging commands. Retain only the canonical package versions that
the loading policy or removal ledger intentionally references.

When changing a shared protocol, update all implemented language contracts and
add boundary tests for valid maximums and one-step-over-limit failures.

## Code and documentation

- Keep TypeScript contracts explicit and compatible with strict checking.
- Format Rust with `cargo fmt`; keep authorization and validation fail-closed.
- Keep Python workers dependency-light and validate Host responses before use.
- Update English and Simplified Chinese host catalog entries together.
- Document current behavior, not aspirations. Mark proposals and historical
  recovery explicitly.
- Add a decision entry when a change affects trust, persistence, protocol
  compatibility, ownership, or a cross-module invariant.

## Git and pull requests

- Branch from `main`; use the `codex/` prefix for Codex-created branches.
- Use focused, imperative commits. Do not rewrite shared history.
- Before opening a PR, inspect the complete diff and report which checks ran.
- Merge only after required PR checks pass. After the merge, update local
  `main`, verify it matches `origin/main`, and delete only the merged feature
  branch locally and remotely.
- Pushing, merging, tagging, releasing, and deleting remote branches require an
  explicit user request.
