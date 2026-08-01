# `.myc` plugin runtime

## Package contract

An executable package is a ZIP-compatible `.myc` archive containing root-level
`plugin.yml` and `plugin.wasm`. Its manifest must declare:

```yaml
apiVersion: researchcanvas.dev/v1alpha1
kind: AnalysisPlugin
spec:
  engine: wasm32-myc
  entry: plugin.wasm
  language: rust # rust, cpp, or other
  capabilities: [analysis.run]
  permissions: []
```

Rust and C++ are source-language labels, not native execution modes. Both SDKs
produce a portable WebAssembly module; the application never loads `.dll`,
`.so`, `.dylib`, or another native library from a plugin.

## Guest ABI

The module exports `memory`, `myc_alloc(i32) -> i32`, and
`myc_run(i32, i32) -> i64`. The host writes UTF-8 JSON into the allocated input
range. `myc_run` returns `(output_pointer << 32) | output_length`; the referenced
bytes must be valid UTF-8 JSON.

Every call creates a fresh VM instance and receives an immutable JSON request.
There is no ambient graph, filesystem, network, clock, process, environment, or
UI access. The executable WASM runtime continues to reject every import.

Filesystem, export, folder, Git, and i18n integrations are a separate,
non-executable `WorkspacePlugin`/`LocalePlugin` path. A declarative command names
a capability, and a fixed Rust command revalidates the installed package before
performing that one bounded action. These capabilities are not WASM imports and
cannot be used to escape the analysis VM.

## Enforced limits

| Boundary | Limit |
|---|---:|
| Archive | 16 MB |
| Expanded package | 32 MB |
| Archive entries | 128 |
| Guest linear memory | 16 MB |
| Fuel per call | 5,000,000 units |
| JSON request | 1 MB |
| JSON response | 1 MB |
| Instances/memories/tables | 1 / 1 / 1 |

The installer rejects unsafe ZIP paths, invalid identities, undeclared
capabilities, permission requests, non-WebAssembly entries, and manifest changes
during extraction. Installed executable metadata includes the entry SHA-256.

Packages are currently local and unsigned. SHA-256 provides integrity identity,
not publisher authentication; do not treat an unknown local package as trusted.

## GraphPatch boundary

Network extractors, including a future Torch block adapter, communicate through
`researchcanvas.dev/graph-patch/v1alpha1`. The patch is a portable proposal,
not an imperative callback: it has bounded operations, records plugin/external
provenance, requires review, and is applied by the host store only after explicit
user acceptance. This keeps model inspection, graph semantics, placement, and
project persistence independently replaceable.

## Lifecycle

1. The desktop client scans `plugins/packages` or receives a dropped `.myc`.
2. Rust validates and extracts into a staging directory.
3. Rust re-reads the staged manifest and entry, then atomically moves the package
   into `plugins/installed/<id>@<version>`.
4. The store lists Rust-verified metadata. Enablement is a local UI preference.
5. An enabled runtime plugin may run its self-test through the fixed IPC command.
6. Rust resolves the installed directory from validated ID/version values and
   executes `plugin.wasm` in the bounded VM.

The reproducible fixture at
`plugins/sources/researchcanvas.runtime-smoke` exercises this entire lifecycle.
