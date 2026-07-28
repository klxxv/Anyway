# Code comments, TODOs, and coupling audit

## Commenting policy

Comments describe module responsibility, externally visible invariants, security
boundaries, and non-obvious algorithmic limits. Avoid line-by-line restatements
of TypeScript, because they quickly become stale and obscure the actual logic.

The domain contract and graph core now document their renderer independence,
legacy migration, deterministic traversal order, scenario overlays, and the
100-path cap used to protect the UI from dense-graph expansion.

## TODOs worth adding

1. **`app/components/ResearchCanvasApp.tsx` — split the application shell.**
   Extract state/action hooks and panels (canvas, inspector, plugin store,
   settings, analysis) behind typed props or a small application store. This is
   the highest-value TODO because it reduces change coupling and test setup.
2. **`app/i18n/catalog.ts` — repair and validate text encoding.**
   Several checked-in Chinese strings appear mojibaked in source output. Add a
   UTF-8 catalog validation test and replace corrupted literals with verified
   Simplified Chinese before translators extend the catalog.
3. **`app/lib/research-core.ts` — explicitly model graph-analysis limits.**
   Move the hard-coded all-shortest-path cap (`100`) to a named options value
   and return truncation metadata, so callers can explain partial results.
4. **`src-tauri/src/plugins.rs` — transactional installation cleanup.**
   Ensure the staging directory is removed when `read_installed_plugin` fails
   after extraction, and add archive-path, oversized-entry, and failure-cleanup
   tests.
5. **`db/schema.ts` — remove or implement the D1 placeholder.**
   The database adapter is available but the schema is intentionally empty.
   Either add migrations with an ownership/lifecycle design or delete the
   dormant adapter until persistence is in scope.
6. **`scripts/train_mnist_ablation.py` — make scientific thresholds configurable.**
   The 1.5 and 1.0 percentage-point decision thresholds should be command-line
   parameters recorded in the generated artifact, rather than implicit policy.

## Coupling assessment

| Area | Level | Evidence | Recommended direction |
| --- | --- | --- | --- |
| `app/lib/research-types.ts` | Low | Type-only shared contract with no platform imports. | Keep it renderer and storage agnostic. |
| `app/lib/research-core.ts` | Low | Pure operations over `ProjectState`; tests call it directly. | Preserve this boundary; pass limits as options. |
| Fixtures and i18n | Low–medium | Depend on the domain types, but fixtures include presentation-ready text. | Keep fixture builders isolated; move shared builders to a helper if more fixtures arrive. |
| Plugin contracts/catalog | Medium | Contracts correctly narrow the plugin context; catalog shares UI-facing manifests. | Separate installation/runtime state from display catalog if runtime behaviors grow. |
| Tauri client and Rust installer | Medium | A deliberate browser/Rust IPC pair, isolated from graph semantics. | Retain the adapter; add end-to-end installation tests. |
| Worker and D1 adapter | Low | Thin platform wiring with no UI/domain imports. | Keep database schema ownership explicit. |
| `ResearchCanvasApp.tsx` | High | One client component coordinates graph state, algorithms, persistence, keyboard input, React Flow, plugins, modals, and rendering. | Split by feature and introduce a focused state/action boundary. |
| Global CSS and component markup | Medium–high | Many class-name contracts span a large JSX file and several global stylesheets. | Co-locate feature styles or adopt CSS modules after the UI split. |

`ResearchCanvasApp.tsx` is the only high-coupling hotspot. The surrounding
architecture already has useful seams: domain logic, platform adapters, and
declarative plugin data are substantially decoupled.
