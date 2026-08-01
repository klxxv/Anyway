# Code comments, TODOs, and coupling audit

## Commenting policy

The codebase uses English/Chinese bilingual comments for module responsibility, invariants,
platform boundaries, gesture behavior, persistence, and non-obvious graph algorithms.
Comments intentionally avoid narrating obvious JSX or TypeScript line by line, because those
comments become stale and reduce signal.

The redesigned workspace follows the same policy. Its public entrypoint, composition root,
state hook, native pinch bridge, graph canvas, renderers, fixture, dialogs, inspector, top bar,
and radial menu each document their responsibility in both languages.

## TODOs still worth keeping

1. **`app/lib/research-core.ts` — configurable graph-analysis limits.**
   Move the all-shortest-path cap (`100`) into a named option and return truncation metadata.
2. **`src-tauri/src/plugins.rs` — transactional installation cleanup.**
   Remove staging data after extraction/read failures and add hostile-archive tests.
3. **`db/schema.ts` — explicit persistence ownership.**
   Implement the D1 schema and migration lifecycle, or remove the dormant adapter.
4. **`scripts/train_mnist_ablation.py` — configurable scientific thresholds.**
   Promote the 1.5 and 1.0 percentage-point policies to recorded CLI parameters.
5. **`app/features/research-workspace/workspace-fixture.ts` — fixture extraction.**
   Move the climate example into a versioned fixture file if the application gains additional
   reference studies or user-selectable templates.

## Coupling after the App-layer rewrite

| Area | Level | Evidence | Direction |
| --- | --- | --- | --- |
| `app/lib/research-types.ts` | Low | Type-only shared contract with no renderer or platform imports. | Keep renderer/storage agnostic. |
| `app/lib/research-core.ts` | Low | Pure deterministic operations tested without UI. | Preserve the boundary; expose limits through options. |
| `app/components/ResearchCanvasApp.tsx` | Low | Eight-line stable public export only. | Keep as the route-facing compatibility seam. |
| `ResearchWorkspaceApp.tsx` | Medium | 185-line composition root coordinates feature state and overlays through typed callbacks. | Do not move domain algorithms back into it. |
| `use-workspace-project.ts` | Medium | Owns persistence, history, and mutations but no rendering. | Split persistence adapter only if remote storage is added. |
| Graph canvas and renderers | Medium | React Flow-specific state is isolated from domain records. | Keep React Flow types inside the canvas boundary. |
| Inspector/dialog/top-bar components | Low | Presentational components receive typed data and callbacks. | Continue adding behavior through props or focused hooks. |
| `trackpad.rs` + `trackpad-pinch.ts` | Medium | Rust emits one atomic two-contact frame; the UI consumes only composed scale through rAF. | Keep the standalone hardware demo as the platform acceptance gate. |
| Tailwind tokens and component CSS | Medium | Global tokens are centralized; React Flow and pie geometry require named class contracts. | Keep complex third-party overrides in the dedicated component layer. |
| Tauri client and Rust installer | Medium | IPC is isolated from graph semantics but spans a browser/Rust boundary. | Add end-to-end installer and error cleanup tests. |

The former high-coupling hotspot is removed: the public App file fell from 5,821 lines to 8
lines, while behavior is distributed across cohesive feature modules. No current module is
assessed as high coupling.
