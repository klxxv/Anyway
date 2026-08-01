# Research Canvas plugins

The directory follows a module-oriented layout similar to Go modules:

```text
plugins/
  packages/                       distributable *.myc archives
  installed/                      verified expanded modules
  sdk/python/research_canvas.py   reference lifecycle contract
  sdk/rust/                       Rust-to-WASM ABI example
  sdk/cpp/                        C++-to-WASM ABI example
  sources/<plugin-id>/            source tree for one module
```

A `.myc` file is a ZIP archive with two root files:

```text
plugin.yml    version, developer, description, capabilities, permissions
theme.json    the entry declared by a ThemePlugin
```

Build a package:

```bash
python scripts/build_myc_plugin.py \
  plugins/sources/researchcanvas.onedarkpro \
  plugins/packages/researchcanvas.onedarkpro@1.0.0.myc
```

In development, the Tauri client scans `plugins/packages`, extracts verified
packages into `plugins/installed/<id>@<version>`, and loads their manifests.
Dropping a `.myc` file onto the Plugin Store invokes the same installer.

Theme and edge-style plugins remain declarative. `AnalysisPlugin` packages may
contain `plugin.wasm` produced by Rust or C++; they execute in the native Rust
VM with no host imports, a 16 MB memory ceiling, 5,000,000 fuel units, and 1 MB
JSON input/output limits. The stable ABI exports `memory`,
`myc_alloc(i32) -> i32`, and `myc_run(i32, i32) -> i64`.

Native machine code is never loaded. Both C++ and Rust plugins compile to the
same portable WebAssembly boundary, so the installer can verify the artifact
before the VM executes it.

## Context menu contributions

An executable plugin may add scoped node, edge, or canvas actions. It must
declare both `analysis.run` and `context-menu.contribute`; the desktop
installer rejects menu contributions on declarative plugins or unknown icons.

```yaml
spec:
  capabilities:
    - analysis.run
    - context-menu.contribute
  permissions: []
  contributes:
    contextMenus:
      - id: inspect-context
        scope: node
        label: Analyze node context
        icon: sparkles
```

When selected, the app invokes that same plugin with `operation:
"context-menu"` plus a bounded context containing only the project id, target
id, scope, and canvas position. Plugins never receive a React callback or a
reference to the workspace store.
