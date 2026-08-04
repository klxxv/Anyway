# Module boundaries

## Stable kernel

`src-tauri/src/graph_compiler/` (Rust) is the semantic kernel: canonicalization,
hashing (blockHash/fileHash), graph invariants, logic chains, contradiction chains,
reachability, graph diff, deterministic layout, digest/mermaid exports. Graph
properties are hard-computed here — never by LLM/agents (§15 of canvas-format-v3.md).
Modules stay ≤500 lines each: `canonical` (canonicalization + dual hashing),
`invariants` (invariant checks), `algorithms` (traversal/paths), `analysis`
(cycles/contradiction chains/logic chains/scenario diffs), `mod` (compile pipeline
§15.1 + version-control diff §6).

The same crate is reused by the desktop app (Tauri), the registry server, and the
`canvas compile` CLI for CI verification.

`app/lib/research-types.ts` remains the renderer- and desktop-independent type
contract. Client graph algorithms live in `app/lib/graph` and `app/lib/analysis`;
fixtures and tests stay there, and their Rust twins in `graph_compiler` are kept
bit-identical through dual-implementation tests until the UI is switched to Tauri
commands (`tests/research-core.test.ts` ↔ `src-tauri/tests/graph_algorithms.rs`).

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
