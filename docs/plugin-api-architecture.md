# Research Canvas plugin API architecture

## Design rules

The plugin host has one registry and four deliberately different execution
planes. A package never receives a React store, filesystem handle, Git handle,
or native callback. The registry selects at most one compatible version for
each plugin id; enabling a newer version atomically deactivates older versions.

1. **Registry plane** — discovers native-validated packages, arbitrates
   versions, persists activation, and publishes one immutable snapshot to the
   workspace, plugin store, i18n provider, and contribution selectors.
2. **Declarative plane** — Theme, EdgeStyle, and Locale plugins provide bounded
   data. The host maps tokens or messages onto known extension points.
3. **Host-mediated plane** — Workspace plugins declare commands and named
   capabilities. Native code re-reads the installed manifest on every call and
   performs one bounded operation selected by the user.
4. **Sandbox plane** — Analysis plugins receive a versioned JSON call envelope
   and execute the common Rust/C++ WebAssembly ABI with no imports, bounded
   memory, bounded input/output, and fuel accounting.

Graph-changing integrations use a fifth data contract,
`researchcanvas.dev/graph-patch/v1alpha1`. Graph patches are proposals, are
validated independently, and always require review before the workspace store
applies them.

## Unified identity and invocation

Every active contribution carries the same reference:

```ts
type PluginReference = { id: string; version: string; name: string };
```

Executable analysis calls use one envelope:

```json
{
  "apiVersion": "researchcanvas.dev/plugin-call/v1alpha1",
  "operation": "self-test | context-menu | community.operation",
  "context": {},
  "payload": {}
}
```

The native host validates the envelope and, for `context-menu`, proves that the
requested action id and scope were declared by that exact installed version.
Workspace commands use the same `PluginReference`, while their payload travels
through a capability-specific host function so arbitrary plugins cannot widen
filesystem or Git access.

## Reference plugin requirements

| Plugin | Plane | Declared capability | Host cooperation | Observable effect |
|---|---|---|---|---|
| One Dark Pro | Declarative | `theme.register` | Map semantic and toast/minimap component tokens to scoped CSS variables | Whole workspace theme changes without global CSS mutation |
| Circuit Orthogonal | Declarative | `edge.style.register` | Resolve routing and relation stroke manifest | Rounded 90-degree semantic connectors |
| Japanese UI | Declarative | `i18n.register` | Merge known message keys with English fallback | Japanese becomes an available locale |
| Runtime Smoke | Sandbox | `analysis.run`, `context-menu.contribute` | Validate call envelope and execute verified WASM | Self-test and node context action return bounded JSON |
| Export Suite | Host-mediated | `project.export` | Render in WebView, verify declared format, write selected path | PDF/SVG/PNG export commands |
| Folder Workspaces | Host-mediated | `project.folder` | Scan bounded metadata under selected folder | Folder project index |
| Git Workspace | Host-mediated | `git.repository.read/init`, `git.autosave`, `git.account.read/login`, `git.ssh.generate/upload`, `graph.patch.propose` | Fixed Git/GitHub CLI/OpenSSH arguments, browser auth, private-key isolation, explicit initialization, and bounded autosave path | Account state, SSH public keys, non-repository/empty-repository states, commit tree, and review-gated research graph proposal |

## Lifecycle

`discover → validate → select latest compatible version → enable → resolve
contributions → invoke through its plane → disable/replace`.

Installation and activation are separate. Installation may retain multiple
immutable versions, but contribution resolution cannot activate more than one
version per id. An incompatible or superseded package remains visible in the
store for diagnosis and can never silently contribute behavior. Bulk removal
records exact-version tombstones so bundled packages stay removed until an
explicit reinstall.
