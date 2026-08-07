# Module boundaries

## Stable kernel

`src-tauri/src/graph_compiler/` (Rust) is the semantic kernel, split into
`canonical.rs` (canonicalization + hashing), `invariants.rs` (graph invariants),
`layout.rs` (deterministic layout), and `algorithms.rs` (graph algorithms);
`mod.rs` re-exports the public API. Graph properties are hard-computed here —
never by LLM/agents (§15 of canvas-format-v3.md). Layout (§11) is intent-driven:
`views[].layout = { mode, params }` plus computed positions; only human-pinned
coordinates override the computation, and unpinned drags never persist.

The same crate is reused by the desktop app (Tauri), the registry server, and the
`canvas compile` CLI for CI verification.

### `canvas compile` CLI

`crates/canvas-cli` builds the `canvas` binary (entry `src/bin/canvas.rs`). It is a
thin consumer of the kernel — it implements no graph algorithm itself, so on a fixed
fixture its output is bit-identical to what the Tauri/Registry side produces through
the same public API (`crates/canvas-cli/tests/parity.rs` proves this).

```
cargo run -p canvas-cli -- compile project.mycproj [--strict] [--layout] [--logic] [--bp] [--output json|mermaid|text]
```

The report (json, default) always carries `diagnostics` (invariant violations) and
`hashes` (blockHashes / contentRootHash / fileHash / verified). `--logic` adds the
logic-chain factor graph and contradiction witnesses; `--bp` adds dual-channel
belief states; `--layout` adds deterministic layout positions — each section is
serialized straight from the kernel. `--strict` fails (exit 1) on any diagnostic
including warnings. Exit codes: 0 pass, 1 diagnostics/parse failure, 2 usage,
3 unreadable input. Mermaid output is the kernel's `export_mermaid` verbatim
(GC-13 baseline).

`cargo test -p canvas-cli` runs the parity suite against
`tests/fixtures/pinn-architecture.mycproj`.

`app/lib/research-types.ts` remains the renderer- and desktop-independent type
contract. `app/lib/research-core.ts` is the thin client wrapper: fixtures and
tests stay; algorithm implementations call the Rust compiler via Tauri commands
until parity is proven by bit-identical dual-implementation tests.

## Application features

- `app/i18n` owns the English/Chinese host catalogs, persisted device locale,
  community locale merging, English fallback, and typed lookup.
- `app/plugins` owns plugin contracts, built-in catalog metadata, and the sole
  browser-to-Tauri adapter. The plugin store is a workspace feature that only
  consumes this adapter.
- `app/styles` owns responsive behavior separately from the legacy component
  visual rules.
- `app/components` composes features and translates user actions into graph
  commands.

## Desktop boundary

`src-tauri/src/plugins.rs` is the only archive/filesystem installer. It validates
the API version, plugin kind, IDs, version, capability set, permissions, archive
paths, entry count, expanded size, WebAssembly magic, and manifest identity
before installation. It computes the executable entry SHA-256 and exposes only
verified metadata to the Webview.

`src-tauri/src/plugin_vm.rs` is the only executable plugin boundary. A new
`wasmi` store and instance are created for every invocation. Guest modules have
no host imports, receive JSON through linear memory, and are bounded by memory,
fuel, input, and output limits. Neither graph stores nor React components can
load native libraries or execute guest bytes directly.

`src-tauri/src/projects.rs` owns validated native `.mycproj`/JSON persistence.
`src-tauri/src/workspace_host.rs` owns fixed export, folder scan, and Git host
actions. These commands revalidate the installed `WorkspacePlugin` identity and
declared capability on every call; no filesystem handle or process API crosses
into plugin code.

`app/plugins/contracts.ts` defines the review-gated GraphPatch interchange.
Torch/ONNX/model adapters may propose semantic nodes and relations, but only
`use-workspace-project.ts` can apply the validated proposal and create layout,
provenance, undo, and activity state.

## Plugin source tree

`plugins/sources/<id>` is one module. `plugin.yml` is its public contract.
`plugins/packages/<id>@<version>.myc` is its distributable artifact.
`plugins/installed/<id>@<version>` is generated state and is ignored except for
the directory placeholder.

`plugins/sdk/rust` and `plugins/sdk/cpp` define the same guest ABI. Both compile
to `plugin.wasm`; native Rust/C++ code is never loaded by the application.
