# Anyway

Human-led research canvas. Map variables, methods, hypotheses, evidence, and results on an interactive graph — with deterministic algorithms for navigation and structural analysis.

## Why "Anyway"

Research is messy. You run experiments, read papers, form hypotheses, and change your mind. **Anyway** gives you a persistent, local-first workspace where every idea, variable, and result is a node you can connect, question, and revisit. No cloud lock-in. No AI black box. Your graph, your rules.

## Core concepts

- **Semantic graph** — typed nodes (question, hypothesis, method, evidence, result…) and directed edges (supports, contradicts, causes, mediates…). The graph IS the research record.
- **Deterministic algorithms** — BFS/DFS traversal, cycle detection, shortest paths, logic chains, contradiction chains, scenario reachability, and BP-style influence propagation. Hard-computed, never LLM-generated.
- **Six layout projections** — evidence chain, refutation chain, tree, prefix Huffman, table, and neural-network views. Switch between them instantly.
- **Ablation scenarios** — non-destructive overlays that disable nodes/edges. Compare "what if we remove this variable?" without mutating the base graph.
- **Plugin system (.myc)** — versioned, Ed25519-signed ZIP packages. Theme plugins, edge style plugins, analysis plugins (WebAssembly), workspace plugins, and locale plugins. Reference experiment blocks from other researchers by importing their `.myc` packages.
- **Local-first** — everything lives on your machine. Optional Git integration for version control and collaboration.

## Quick start

```bash
npm install
npm run dev:desktop-web    # start the Vite dev server
npm run desktop:dev        # start the Tauri desktop app
```

Open `http://127.0.0.1:5173`.

## Scripts

| Command | Description |
|---|---|
| `npm run dev:desktop-web` | Vite dev server |
| `npm run build:desktop` | Production build for Tauri |
| `npm run desktop:dev` | Tauri desktop dev mode |
| `npm test` | Full test suite |
| `npm run lint` | ESLint |
| `npm run typecheck:vue` | Vue + TypeScript type check |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Rust tests |

## `.myc` plugin format

`.myc` is a ZIP archive with a `plugin.yml` manifest at its root. Plugins are verified at install time:

- **Manifest validation** — API version, plugin kind, capability declarations
- **Signature verification** — Ed25519 signatures over the manifest content, verified against trusted publisher keys
- **Archive safety** — path traversal prevention, size limits (16 MB archive, 32 MB expanded), entry count caps
- **Atomic install** — staged extraction, verified identity, then atomically renamed into `installed/`

Build a plugin:

```bash
python scripts/build_myc_plugin.py \
  plugins/sources/myc.onedarkpro \
  plugins/packages/myc.onedarkpro@1.3.0.myc
```

Plugin kinds:

| Kind | Engine | Purpose |
|---|---|---|
| `ThemePlugin` | declarative | Color themes and visual tokens |
| `EdgeStylePlugin` | declarative | Connector routing, stroke, markers |
| `AnalysisPlugin` | wasm32-myc | WebAssembly compute kernels |
| `WorkspacePlugin` | host-mediated | Export, folder scan, Git actions |
| `LocalePlugin` | declarative | Community language packs |

## Architecture

```
src/                  Vue 3 renderer (Vite + Vue Flow + Pinia)
  vue/                Workspace components, composables, and stores
app/                  Renderer-agnostic TypeScript domain and platform layer
  lib/
    graph/            Graph algorithms (traversal, cycles, paths, reachability)
    layout/           Deterministic layout projections
    analysis/         Logic chains, influence propagation
    project/          State management, export, scenarios
src-tauri/            Rust desktop backend
  src/
    graph_compiler/   Canonicalization, hashing, invariants, layout, algorithms
    plugins.rs        .myc installer and validator
    signing.rs        Ed25519 signature verification
    plugin_vm.rs      WebAssembly sandbox (wasmi)
    workspace_host.rs Native workspace actions
plugins/              .myc plugin sources, packages, and installed state
tests/                Test suites (core, platform, workspace, SDK, compiler parity)
```

The TypeScript and Rust graph implementations are kept bit-identical through a compiler parity test suite (`tests/compiler-parity.test.ts`). Both implementations process the same fixtures and assert identical output for every algorithm.

## Tech stack

| Layer | Technology |
|---|---|
| Frontend | Vue 3, Vite, Vue Flow, Pinia, Tailwind CSS |
| Desktop | Tauri 2, WebView2 (Windows) |
| Graph kernel (TS) | TypeScript, pure functions |
| Graph kernel (Rust) | Rust, serde, sha2 |
| Plugin runtime | WebAssembly (wasmi) |
| Cryptography | Ed25519 (ed25519-dalek) |
| CI/CD | GitHub Actions |

## License

See [LICENSE](LICENSE).
