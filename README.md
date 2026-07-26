# Research Canvas MVP

Research Canvas is a human-led, local-first research graph for organizing variables, methods, hypotheses, evidence, and results. Traditional graph algorithms provide navigation and structural analysis; AI suggestions remain staged until a researcher explicitly accepts them.

## MVP capabilities

- Typed semantic nodes and relations on an interactive React Flow canvas
- Separate semantic records and per-view placements
- Deterministic BFS/DFS traversal, depth layers, tree/cross/back edges, shortest path, and cycle detection
- Non-destructive ablation scenarios implemented as overlays
- Traceable paper evidence with page, section, quote, and review status
- AI GraphPatch-style suggestion staging with accept/reject and transactional undo
- Local autosave, project import, RunResult import, and revision activity
- Project JSON, JSON Canvas/Obsidian, Markdown, and Python run-manifest exports
- Six reversible automatic projections: evidence chain, refutation chain, tree,
  prefix Huffman, table, and neural-network views
- Semantic filters, effective-chain highlighting, node splitting, and BP-like
  variable influence propagation
- Plugin catalog with a working Git experiment fixture plus reserved Python,
  Zotero, MCP, and review-gated agent integration boundaries
- Built-in performance diagnostics, keyboard/trackpad settings, VS Code-style
  theme manifests, and a distraction-free Zen mode
- English and Simplified Chinese application chrome with device-local locale
  persistence
- Secure `.myc` theme packages with native Tauri drag-and-drop installation

## MNIST acceptance study

The bundled Git experiment plugin loads a real CPU-trained MNIST ablation study
from `app/data/mnist-experiment-results.json`. Reproduce the artifact with:

```bash
python scripts/train_mnist_ablation.py
```

The fixed-seed study compares a 64-unit ReLU baseline against removing pixel
normalization, reducing hidden width to 16, and replacing ReLU with tanh. Each
run is modeled as an experiment edge with metric evidence and provenance, so
the canvas can highlight effective and refuting logic chains without claiming
that graph reachability alone proves causality.

## Run locally

```bash
npm install
npm run dev
```

Open `http://localhost:3000`.

## Run the Windows desktop client

The Tauri development shell uses the standard Next.js development server while
the hosted build continues to use vinext:

```bash
npm run desktop:dev
```

The first Rust build can take several minutes; later UI and CSS edits hot reload
inside the existing desktop window. The Display settings page reads the native
WebView scale factor, listens for monitor scale changes, and selects a readable
text scale automatically. Compact, Comfortable, and Spacious overrides are
stored locally per device.

The responsive shell is verified at 760×560, 1024×720, 1280×800, and
1440×900. At compact widths, sidebars become explicit overlay panels, toolbar
labels compact without removing commands, and the minimap is clamped to the
canvas bounds.

## `.myc` plugins

`.myc` is a versioned ZIP package. `plugin.yml` is the public module contract;
the entry declared by a theme plugin is `theme.json`. Development packages live
in `plugins/packages`, verified installations live in `plugins/installed`, and
source modules live in `plugins/sources/<plugin-id>`.

Build the bundled One Dark Pro theme:

```bash
python scripts/build_myc_plugin.py \
  plugins/sources/researchcanvas.onedarkpro \
  plugins/packages/researchcanvas.onedarkpro@1.0.0.myc
```

The Tauri client scans the packages folder at startup and accepts native file
drops in the Plugin Store. The installer rejects traversal paths, unsupported
API versions, undeclared capabilities, theme permissions, oversized archives,
and packages that change identity during extraction. See
[`plugins/README.md`](plugins/README.md) and
[`docs/module-boundaries.md`](docs/module-boundaries.md).

## Verify

```bash
npm test
npm run lint
npx tsc --noEmit
cargo check --manifest-path src-tauri/Cargo.toml
```

The test suite builds the deployable vinext worker, verifies the rendered application shell, and checks traversal, cycle, scenario-overlay, shortest-path, and export behavior.

The connector boundary can be exercised without granting the application shell
permission to execute arbitrary commands:

```bash
python examples/python_connector_sdk.py \
  examples/run-manifest.example.json \
  output/run-result.example.json
```

Import the generated RunResult from the Navigator footer. The result becomes a
reviewable node with an experiment-evidence backlink, scenario ID, source
filename, summary, and artifact metadata.

The full Windows/Tauri acceptance matrix and captured states are documented in
[`docs/mvp-acceptance.md`](docs/mvp-acceptance.md).

## Architecture

The canonical `ProjectState` stores semantic nodes, typed edges, evidence, scenarios, placements, and activity independently from React Flow. `research-core.ts` is renderer-agnostic and can run under Node.js. React Flow is an adapter that projects semantic records and placements into interactive view objects.

Scenarios only store disabled IDs and overrides. AI-originated proposals live in a separate suggestion store and enter the canonical graph only after a human-reviewed transaction.
