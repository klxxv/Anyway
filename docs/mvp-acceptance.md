# Research Canvas MVP acceptance

Date: 2026-07-27  
Target: native Windows Tauri development client  
Display under test: Windows scaling 150%, approximately 144 DPI

## Outcome

The human-led research-canvas loop is complete for the MVP:

1. Open or create a local project.
2. Load a Git-backed MNIST experiment fixture.
3. Inspect typed variables, experiment relations, and source evidence.
4. Filter the graph or project it into one of six layouts.
5. Highlight supporting, refuting, or effective experimental chains.
6. Compare a non-destructive ablation scenario with the base graph.
7. Review BP-like signed influence rankings without presenting them as causal estimates.
8. Stage, edit, accept, or reject AI suggestions.
9. Export project, Obsidian, tabular, scenario, or connector artifacts.

## Requested capability matrix

| Capability | Status | Verification |
|---|---|---|
| Evidence, refutation, tree, prefix Huffman, table, and neural-network layouts | Pass | Core projection test plus `07-six-layout-modes.jpg` |
| Node, relation, evidence-source, confidence, and experiment filters | Pass | `08-semantic-filters.jpg` |
| Effective/supporting/refuting chain highlighting | Pass | MNIST core test plus `02` and `03` screenshots |
| Split node with provenance | Pass | `09-node-split-provenance.jpg` |
| BP-like signed influence in neural-network mode | Pass | Core ranking test plus `02-mnist-effective-chain-bp.jpg` |
| Plugin store | Pass | Working Git experiment fixture; reserved Python, Zotero, MCP, and agent boundaries |
| Built-in performance diagnostics | Pass | `05-performance-diagnostics.jpg` |
| Settings experience | Pass | Display, canvas, appearance, and keyboard sections |
| DPI-aware typography | Pass | Native Tauri scale-factor read and monitor-change listener; 150% / 144 DPI test |
| Keyboard, trackpad, and Zen mode | Pass | Shortcut reference, two-finger pan preference, snap controls, `12-zen-mode.jpg` |
| VS Code-style theme manifests | Pass | Theme catalog applies CSS-variable manifests without changing graph data |
| Project lifecycle and local persistence | Pass | New, rename, recent, autosave, import/export; `10-project-lifecycle.jpg` |
| Evidence metadata and backlinks | Pass | Authors/year/DOI/URL/file/page/section/quote/offsets plus backlink index |
| Scenario overrides and export | Pass | Overlay reachability/path comparison and scenario JSON |
| Python connector protocol example | Pass | Example manifest produces a structured RunResult and importable evidence |

## MNIST verification

The fixture records Git commit `b7e21ac` and four fixed-seed CPU runs:

| Experiment | Accuracy | Delta vs. baseline | Canvas interpretation |
|---|---:|---:|---|
| Baseline, 64 ReLU units | 92.73% | 0.00 pp | Baseline |
| Ablate pixel normalization | 86.27% | -6.46 pp | Supporting evidence for normalization |
| Reduce hidden width to 16 | 90.53% | -2.20 pp | Supporting evidence for representation capacity |
| Replace ReLU with tanh | 91.60% | -1.13 pp | Refutes the claim that ReLU is uniquely necessary |

The influence panel propagates signed, confidence-weighted experiment messages
backward from test accuracy. It is a research-navigation aid, not a causal or
gradient estimate.

## Automated verification

```text
npm run lint             pass
npx tsc --noEmit         pass
npm run test:core        12/12 pass
rendered shell tests     2/2 pass
npm run build            pass
cargo check              pass
python connector example pass
```

The 5,000-node / 10,000-edge indexed BFS target is included in the core test
suite. The previous development `ResizeObserver` error overlay did not recur
during the native acceptance flow.

## Screenshot evidence

- `01-plugin-store-git-mnist.jpg`
- `02-mnist-effective-chain-bp.jpg`
- `03-mnist-refutation-chain.jpg`
- `04-mnist-scenario-diff.jpg`
- `05-performance-diagnostics.jpg`
- `06-dpi-settings.jpg`
- `07-six-layout-modes.jpg`
- `08-semantic-filters.jpg`
- `09-node-split-provenance.jpg`
- `10-project-lifecycle.jpg`
- `11-ai-staging-review.jpg`
- `12-zen-mode.jpg`

All screenshots are stored in `output/mvp-acceptance`.

## Intentional MVP boundaries

Python, Zotero, MCP, and agent plugins are contracts or reserved catalog entries,
not live integrations. Agent changes remain review-gated GraphPatch proposals.
The MNIST Git fixture is the single loaded demo plugin used to prove the closed
research-graph loop.
