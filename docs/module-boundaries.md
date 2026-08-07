# Module boundaries

## Stable kernel

`src-tauri/src/graph_compiler.rs` (Rust) is the semantic kernel: canonicalization,
hashing (blockHash/fileHash), and graph invariants. `src-tauri/src/graph_algorithms.rs`
is the companion kernel for the deterministic hard-computations that ship in
`app/lib/graph|layout|analysis`: BFS/DFS traversal, cycle detection, shortest
paths, scenario reachability, logic chains, influence propagation, and the six
deterministic layout projections. Graph properties are hard-computed here — never
by LLM/agents (§15 of canvas-format-v3.md).

The same crate is reused by the desktop app (Tauri), the registry server, and the
`canvas compile` CLI for CI verification.

`app/lib/research-types.ts` remains the renderer- and desktop-independent type
contract. `app/lib/research-core.ts` is the thin client wrapper: fixtures and
tests stay; algorithm implementations call the Rust kernel via Tauri commands.
The TS originals still exist in `app/lib/graph|layout|analysis` and are the active
client path. Parity with the Rust kernel is gated by `tests/compiler-parity.test.ts`
(npm run test:compiler), which drives the SAME MNIST + social fixtures through both
implementations and asserts bit-for-bit identical output. Only after the parity gate
passes is the TS implementation allowed to be removed (§15.5 migration path).

`app/lib/compiler-reference.ts` is a TS reference implementation of the v3
canonicalization. It is used ONLY as the comparison anchor for the canonicalize
byte-level parity check and is not part of any runtime path.

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
