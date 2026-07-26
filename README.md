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

## Verify

```bash
npm test
npm run lint
```

The test suite builds the deployable vinext worker, verifies the rendered application shell, and checks traversal, cycle, scenario-overlay, shortest-path, and export behavior.

## Architecture

The canonical `ProjectState` stores semantic nodes, typed edges, evidence, scenarios, placements, and activity independently from React Flow. `research-core.ts` is renderer-agnostic and can run under Node.js. React Flow is an adapter that projects semantic records and placements into interactive view objects.

Scenarios only store disabled IDs and overrides. AI-originated proposals live in a separate suggestion store and enter the canonical graph only after a human-reviewed transaction.
