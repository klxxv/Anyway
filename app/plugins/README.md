# Plugin frontend boundary

- `contracts.ts` defines the versioned `.myc` manifest and narrow lifecycle API.
- `catalog.ts` contains only built-in metadata and semantic theme tokens.
- `tauri-client.ts` is the sole browser-to-Tauri adapter.

Graph stores and React Flow never import filesystem or archive APIs. A plugin
can register a capability through `PluginContext`; it cannot reach application
state unless that capability is explicitly added to the context.

