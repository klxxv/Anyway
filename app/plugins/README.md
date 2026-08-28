# Plugin frontend boundary

- `contracts.ts` defines the versioned `.myc` manifest, verified runtime
  metadata, and JSON execution result.
- `catalog.ts` contains only built-in metadata and semantic theme tokens.
- `tauri-client.ts` is the sole browser-to-Tauri adapter.
- `src/vue/runtime/plugin-host.ts` is the single discovery and activation lifecycle owner.
- `identity.ts` validates compatibility and selects one version per plugin id.
- `context-menu.ts` resolves enabled, capability-approved menu contributions.

Graph stores and Vue Flow never import filesystem or archive APIs. A plugin
can register a capability through `PluginContext`; it cannot reach application
state unless that capability is explicitly added to the context.

Executable `AnalysisPlugin` packages cross the boundary only through a
`PluginReference` and `researchcanvas.dev/plugin-call/v1alpha1` envelope. Rust
re-reads the installed manifest and entry before each invocation; the Webview
never supplies an arbitrary path.
Context menu entries are declarative manifest data and execute through this
same verified boundary with a bounded target context.
