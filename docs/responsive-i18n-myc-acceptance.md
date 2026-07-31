# Responsive, i18n, and `.myc` acceptance

Date: 2026-08-01
Branch: `codex/responsive-i18n-myc`

## Responsive workspace

The Zen workspace keeps the graph as the primary surface. At the reference
desktop viewport, the top command bar, breadcrumb, canvas, minimap, interactive
link legend, and inspector remain simultaneously usable. Narrow viewports use
the existing fit-to-view and compact-command rules rather than changing graph
semantics. Visual and interaction evidence is recorded in `design-qa.md` and
`output/design-qa`.

## Internationalization

- Locales: English and Simplified Chinese.
- Default: normalized from the operating-system/browser language.
- Persistence: `research-canvas.locale.v1` in device-local storage.
- Coverage: workspace commands, hover menus, project menu, node/relation/layout
  labels, settings, link legend, and plugin-store controls.
- Boundary: research records and plugin-authored metadata retain their source
  language and are never machine-translated implicitly.
- Safety: `MessageKey` is derived from the English catalog and the Chinese
  catalog must satisfy the same complete record at compile time.

## `.myc` packages and store

The store lists only metadata returned by the Rust installer. It accepts local
`.myc` drops, filters installed/runtime packages, persists enabled state, and
shows runtime language plus the verified entry SHA-256. Runtime self-tests are
available only after a package is enabled.

Three package kinds are accepted:

- `ThemePlugin`: declarative `theme.json`, `theme.register`, no permissions.
- `EdgeStylePlugin`: declarative `edge-style.json`,
  `edge.style.register`, no permissions.
- `AnalysisPlugin`: `plugin.wasm`, `wasm32-myc`, `analysis.run`, explicit
  Rust/C++/other source label, no permissions.

The deterministic runtime fixture is
`plugins/packages/researchcanvas.runtime-smoke@1.0.0.myc`. Its end-to-end test
validates archive installation, manifest registration, SHA-256 metadata, VM
execution, JSON output, and fuel accounting.

## Executable plugin isolation

Rust and C++ plugins compile to one portable guest ABI. The native Rust host
creates a fresh `wasmi` instance for each call and rejects all host imports.
Enforced limits include a 16 MB guest memory ceiling, 5,000,000 fuel units,
1 MB JSON input/output, one instance, one memory, and one table. Infinite loops
trap on fuel exhaustion; native libraries are never loaded. See
`docs/plugin-runtime.md` for the ABI and threat boundary.

Packages are currently local and unsigned. SHA-256 identifies the installed
entry but does not authenticate a publisher.

## Automated acceptance

The root `npm test` command is the acceptance gate. It builds the frontend,
checks rendered HTML, runs the graph kernel/workspace/platform suites, verifies
both SDK contracts, checks the Rust SDK, and runs native installer/VM tests.
Additional gates are `npm run lint`, `npx tsc --noEmit`, `cargo fmt --check`, and
`git diff --check`.

Manual desktop acceptance verifies language switching, plugin discovery,
enablement, and the runtime-smoke self-test in the packaged Tauri boundary.
