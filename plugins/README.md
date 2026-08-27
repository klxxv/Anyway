# Research Canvas plugins

The directory follows a module-oriented layout similar to Go modules:

```text
plugins/
  packages/                       distributable *.myc archives
  installed/                      verified expanded modules
  sdk/python/research_canvas.py   reference lifecycle contract
  sdk/rust/                       Rust-to-WASM ABI example
  sdk/cpp/                        C++-to-WASM ABI example
  sources/<plugin-id>/            legacy/source trees not yet migrated

my-plugins/                       version-controlled official plugin sources
my-third-plugins/                 ignored local third-party package inbox
.plugin-runtime/dev/              ignored generated desktop-dev runtime
```

A `.myc` file is a ZIP archive with `plugin.json` and one kind-specific entry:

```text
plugin.json            identity, contributions, capabilities, no ambient permissions
theme.json             ThemePlugin entry
icon-theme.json        IconThemePlugin entry (declarative VSIX adapter)
edge-style.json        EdgeStylePlugin entry
plugin.wasm            AnalysisPlugin entry
workspace-plugin.json  host-mediated WorkspacePlugin entry
locales/<tag>.json     declarative LocalePlugin entries
```

Build a package:

```bash
# Canonical packager (deterministic STORE zip, optional Ed25519 signing):
node scripts/pack-plugin.mjs \
  my-plugins/anPdfsolver \
  plugins/packages/myc.pdf-canvas-agent@0.4.0.myc

```

In desktop development, `npm run plugins:stage-dev` materializes only the
explicit official package list in `config/plugin-loading.json` into the ignored
`.plugin-runtime/dev/packages` directory. Development sources under
`my-plugins/` are staged only when their plugin id is explicitly enabled in the
same config or passed to `scripts/stage-plugin-runtime.mjs` with
`--with-dev-plugin <pluginId>`. The Rust loader should apply the same staged
runtime boundary before extracting verified packages into
`.plugin-runtime/dev/installed/<id>@<version>`. It never scans
`my-third-plugins`, and the Japanese locale and One Dark Pro packages are not
part of the default desktop-dev list.
Release bundles embed the same packages under the Tauri resource directory and
install each immutable `id@version` once into application data. Dropping a
`.myc` file onto the Plugin Store invokes the same installer.
Removing incompatible packages records an exact `id@version` tombstone, so an
embedded package is not silently reinstalled on the next discovery pass.
Explicitly installing that package again clears its tombstone.

The staging script can also clean one exact generated version without touching
source directories:

```bash
node scripts/stage-plugin-runtime.mjs dev --clean-plugin myc.pdf-canvas-agent@0.4.0
```

That command is intentionally narrow: it accepts an exact `pluginId@version`
token and removes only matching generated paths under `.plugin-runtime/*`.

VSIX theme import is a separate conversion step:

```bash
npm run import:vsix -- path/to/theme.vsix output/vsix-adapter
```

The importer reads only `package.json` theme/icon-theme contributions and
referenced JSON, SVG, PNG, and font assets. It never loads VS Code extension
JavaScript. Commands, activation/main/browser entries, native binaries,
symbolic links, traversal paths, and unsafe archive ratios are rejected. The
generated `ThemePlugin`/`IconThemePlugin` resources can be reviewed and
packaged as normal declarative MYC resources; the importer does not install or
execute a VSIX.

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
| `git.repository.init` | Explicitly initialize the selected normal folder as a local repository |
| `git.autosave` | Save and commit only `.research-canvas/*.mycproj` |
| `git.account.read` | Read bounded GitHub CLI account status without exposing tokens |
| `git.account.login` | Start the official GitHub CLI browser login flow |
| `git.ssh.generate` | Generate one app-named Ed25519 key without returning private material |
| `git.ssh.upload` | Upload an explicitly selected `.pub` key through GitHub CLI |
| `graph.patch.propose` | Describe a review-required graph proposal |

The reference packages are `myc.export-suite`,
`myc.folder-workspaces`, and `myc.git-workspace`. All
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

```json
{
  "capabilities": [
    "analysis.run",
    "context-menu.contribute"
  ],
  "permissions": [],
  "contributes": {
    "menus": [
      {
        "id": "inspect-context",
        "scope": "node",
        "label": "Analyze node context",
        "icon": "sparkles"
      }
    ]
  }
}
```

When selected, the app invokes that same plugin with `operation:
"context-menu"` plus a bounded context containing only the project id, target
id, scope, and canvas position. Plugins never receive a renderer callback or a
reference to the workspace store.

## Theme component surfaces

`ThemePlugin` may optionally style host-owned toast, minimap, and radial-menu
surfaces through `theme.json.components.toast`, `components.miniMap`, and
`components.radialMenu`. These bounded tokens cover background, border,
text/shadow, node/relation colors, radial dividers and active states, and the
minimap `showRelations` switch. The host still owns layout and interaction.
