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

A `.myc` file is a ZIP archive with `plugin.yml` and one kind-specific entry:

```text
plugin.yml             identity, contributions, capabilities, no ambient permissions
theme.json             ThemePlugin entry
edge-style.json        EdgeStylePlugin entry
plugin.wasm            AnalysisPlugin entry
workspace-plugin.json  host-mediated WorkspacePlugin entry
locales/<tag>.json     declarative LocalePlugin entries
```

Build a package:

```bash
python scripts/build_myc_plugin.py \
  plugins/sources/researchcanvas.onedarkpro \
  plugins/packages/researchcanvas.onedarkpro@1.0.0.myc
```

In development, the Tauri client scans `plugins/packages`, extracts verified
packages into `plugins/installed/<id>@<version>`, and loads their manifests.
Release bundles embed the same packages under the Tauri resource directory and
install each immutable `id@version` once into application data. Dropping a
`.myc` file onto the Plugin Store invokes the same installer.

Theme and edge-style plugins remain declarative. `AnalysisPlugin` packages may
contain `plugin.wasm` produced by Rust or C++; they execute in the native Rust
VM with no host imports, a 16 MB memory ceiling, 5,000,000 fuel units, and 1 MB
JSON input/output limits. The stable ABI exports `memory`,
`myc_alloc(i32) -> i32`, and `myc_run(i32, i32) -> i64`.

Native machine code is never loaded. Both C++ and Rust plugins compile to the
same portable WebAssembly boundary, so the installer can verify the artifact
before the VM executes it.

## Workspace capabilities

Workspace plugins do not execute native code and do not receive filesystem or
Git handles. They contribute commands that name a capability; the Rust host
re-resolves the installed package and proves that capability on every call.
The current bounded capabilities are:

| Capability | Host action |
|---|---|
| `project.export` | Save a PDF, SVG, or PNG to a path selected by the user |
| `project.folder` | Read compatible project metadata below a selected folder |
| `git.repository.read` | Read a selected repository with fixed Git arguments |
| `git.autosave` | Save and commit only `.research-canvas/*.mycproj` |
| `graph.patch.propose` | Describe a review-required graph proposal |

The reference packages are `researchcanvas.export-suite`,
`researchcanvas.folder-workspaces`, and `researchcanvas.git-workspace`. All
three use `tests/fixtures/pinn-architecture.mycproj`, which covers Fourier
embedding, widths 32/64/128, depths 8/10/12, residual links, cos/sin hard
constraints, PDE/separated/auto-weighted losses, and a Git-linked ablation.

## Portable graph synchronization

Community Torch/ONNX adapters should inspect a model outside the application
and return `researchcanvas.dev/graph-patch/v1alpha1`. A patch identifies its
source, contains bounded add/update node and edge operations, and must set
`reviewRequired: true`. The host validates it, shows it for review, and only the
workspace store can apply accepted operations. Plugins never receive a mutable
store reference. This is the stable seam for mapping `torch.nn.Module` paths or
network blocks into Research Canvas without coupling the host to PyTorch.
The language-neutral contract is published at
`plugins/sdk/graph-patch.schema.json`; the dependency-free Python SDK includes
`GraphPatch`, node/edge proposal helpers, and a `NetworkBlockExtractor`
protocol for Torch/ONNX community adapters.

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
