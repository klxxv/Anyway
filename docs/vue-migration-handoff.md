# Vue migration handoff

## Current stack

The renderer is Vue 3 with the Composition API, TypeScript, Vite,
`@vue-flow/core`, and Pinia, packaged in a Tauri 2 desktop shell. Pinia is the
shared renderer-state architecture: stores use the setup-store form and the
application registers one shared root. The renderer-agnostic domain and
platform code remains under `app/`, while the Vue renderer is assembled under
`src/vue/` with `src/main.ts` and `src/App.vue` as the Vite entry. The legacy
React/Next renderer has been removed.

## Migration scope

Migrate the renderer entry, i18n/plugin runtime adapters, workspace composables,
canvas, workspace shell, dialogs, inspector, diff review, PDF Agent review,
plugin store, shortcuts, context menus, keyboard/trackpad gestures, and
responsive behavior. Preserve the visible behavior, keyboard commands,
serialization, plugin theme semantics, GraphPatch review gate, import/export,
and Tauri desktop behavior. The canvas adapter owns conversion between Vue
Flow nodes/edges and the renderer-agnostic workspace types.

The intended Vue structure is:

```text
src/
  main.ts
  App.vue
  vue/
    runtime/
      i18n.ts
      plugin-host.ts
      tauri-client.ts
    stores/
      pinia.ts
      project.ts
      canvas-interaction.ts
    composables/
      use-workspace-project.ts
      use-canvas-diff.ts
      use-trackpad-pinch.ts
    canvas/
      research-graph-canvas.vue
    ResearchWorkspaceApp.vue
    components/
      workspace-topbar.vue
      inspector-panel.vue
      workspace-dialogs.vue
      workspace-context-menu.vue
      agent-review-panel.vue
      diff-panel.vue
      plugin-store-dialog.vue
```

The structure test accepts PascalCase component filenames and the equivalent
`src/runtime`/`src/composables` layout, but every semantic role must still be
present.

## Protected files and interfaces

The following are contract inputs. Vue code may import them, but must not
rewrite, fork, or silently rename their public interfaces:

- `app/lib/**`, especially `research-types.ts`, `project-io.ts`, `project/**`,
  `graph/**`, `layout/**`, `analysis/**`, and `compiler-reference.ts`.
- `app/platform/**`, including native project, PDF Agent, and trackpad bridges.
- `app/plugins/**`, including manifest, provider, Agent, GraphPatch, workspace,
  theme, identity, and Tauri client contracts.
- `app/features/research-workspace/hooks/sync-logic.ts` and its project storage
  key; `src/vue/runtime/i18n.ts` and its locale key; the workspace preference key
  and plugin changed event declarations.
- `src-tauri/**`, Rust command registration, Tauri capabilities, and the
  project/compiler/plugin schemas.

Do not modify `TODO.md` or `OPENCODE_HANDOFF.md` as part of this worker handoff.

## Interface freeze rules

1. Keep every exported function name, constant name, module contract, argument
   shape, return type, promise behavior, and error code in the interface checker.
2. Keep all Tauri command strings exactly unchanged, including argument keys and
   camelCase serialization: `save_project_file`, `import_project_file`,
   `start_pdf_job`, `review_patch`, `execute_myc_plugin`, and the other commands
   enumerated by `scripts/check-vue-interface-contract.mjs`.
3. Keep these persisted/event identifiers unchanged: `research-canvas.zen-workspace.v1`,
   `research-canvas.locale.v1`, `research-canvas.workspace-preferences.v2`,
   `research-canvas.enabled-plugins.v1`, and
   `research-canvas.plugins-changed`.
4. Keep `CURRENT_SCHEMA_VERSION`, project extensions, `NODE_TYPES`,
   `EDGE_TYPES`, `LAYOUT_MODES`, plugin API versions, GraphPatch API version,
   Agent capabilities, and Provider capabilities unchanged.
5. Vue components may adapt props/emits and refs, but may not define a second
   `ProjectState`, `GraphPatch`, or persisted schema. They must call the
   platform/runtime adapter instead of invoking Rust commands directly.
6. Do not change the Rust kernel, `.mycproj` format, `.myc` package format,
   GraphPatch review gate, plugin ABI, or localStorage serialization.
7. `src/main.ts` must register the single shared Pinia root. Renderer stores
   under `src/vue/stores/` must use `defineStore("id", () => { ... })`; option-
   style stores and Vuex imports/APIs are prohibited.
8. `useWorkspaceProject` remains the public composable facade over
   `useProjectStore`, including its historical ref-shaped fields and mutation
   methods. It must use `storeToRefs` when exposing store state. Components that
   consume a store directly, such as the canvas, must also use `storeToRefs` at
   that boundary.
9. Pinia stores own renderer state, while domain/platform/plugin functions stay
   in their existing `app/**` adapters. Vue code must not call Tauri `invoke`
   directly or copy protected domain/GraphPatch schemas.

## Acceptance commands

Run these from the repository root:

```powershell
node scripts/check-vue-interface-contract.mjs
npx tsx --test tests/vue-interface-contract.test.ts tests/vue-migration-structure.test.ts
npx tsx --test tests/pinia-architecture.test.ts
npm run build
npm run test:core
npm run test:platform
npm run test:workspace
npm run test:sdk
npm run test:compiler
npm run build:desktop
```

The first command is the focused frozen-interface gate; it now includes the
Pinia registration, setup-store, facade, `storeToRefs`, Vuex, duplicate-schema,
and direct-Rust-invoke checks. The two Vue tests and the Pinia architecture test
should be run after the parallel Vue files are present. The full suite is the
final parity gate; any failures must identify whether they are caused by a
renderer adapter or by a protected contract.

## Pinia TODO checklist

- [x] Register one shared Pinia root from `src/main.ts`.
- [x] Keep the project and canvas interaction stores in setup-store form.
- [x] Preserve `useWorkspaceProject` as a compatibility facade with the
  historical ref-shaped return contract.
- [x] Use `storeToRefs` in the project facade and direct canvas store boundary.
- [x] Move workspace presentation state into `useWorkspaceUiStore` while
  keeping project data/history/persistence in `useProjectStore`.
- [x] Move plugin-host lifecycle state into `useRuntimePluginHostStore` while
  retaining `usePluginHost`/`providePluginHost` behavior and storage/event keys.
- [x] Move locale lifecycle state into `useRuntimeI18nStore` while retaining
  `research-canvas.locale.v1`, provider behavior, and protected-message filtering.
- [x] Move the trusted browser auth adapter into `useRuntimeAuthStore` while
  preserving all exported auth function signatures and redirect checks.
- [x] Reuse `PluginStoreItem.vue` for built-in and installed plugin entries,
  and make `PluginStoreDialog.vue` consume the shared Pinia plugin-host bridge
  instead of maintaining a second installed/enabled state source.
- [ ] Add runtime tests for isolated Pinia roots used by fixture/storage
  compatibility callers, including SSR/browser lifecycle safety.
- [x] Run Vue typecheck, desktop/Vite build, interface/SFC/Pinia gates, core,
  platform, workspace, compiler, and SDK parity tests.
- [ ] Manually smoke-test the production Vue renderer, plugin connection test,
  Kimi opt-in upload, and PDF-to-canvas result in the desktop application.

## Remaining known risks

The project store uses lifecycle hooks for hydration and the compatibility
facade can create an isolated Pinia root when fixture/storage options are
provided. This preserves existing test and embedding behavior, but still needs
browser, SSR, and multiple-instance coverage. Plugin-host and i18n providers
now act as compatibility bridges over Pinia stores; future cleanup must retain
their injection APIs, lifecycle start/stop behavior, persisted keys, plugin
events, and platform adapters.

## Current worker status

The Vue renderer now registers one Pinia root and uses setup-style stores for
project/history, workspace presentation, canvas interaction, plugin runtime,
i18n, and auth coordination. Compatibility facades preserve the former public
composable/provider APIs. The focused interface checker, Pinia architecture
test, SFC structure test, Vue typecheck, and Vite build must all remain green;
do not resolve failures by weakening required paths or protected interfaces.
The production browser smoke check renders the workspace and verifies that the
project menu and search state transition correctly through the Pinia store.

## Provider/PDF integration handoff

Provider configuration is a host boundary, not an Agent-owned HTTP client.
Plugin manifests may declare the API URL, request dialect, model, thinking
level, and credential source, while the Tauri host validates those values,
resolves secrets, performs connection tests, and makes model requests. Test
results must contain only bounded status/latency/provider metadata; API keys and
raw authorization headers must never be returned to Vue or plugin code.

DeepSeek's public OpenAI/Anthropic-compatible APIs support streamed model
responses, but the Anthropic compatibility table explicitly marks document and
container upload content as unsupported. The portable PDF path therefore reads
and extracts the PDF locally, sends bounded text context through the selected
OpenAI/Anthropic adapter, parses the model result, and keeps the GraphPatch
review gate intact.

Kimi provides a provider-specific Files workflow at
`https://api.moonshot.ai/v1`: upload with multipart `purpose=file-extract`, read
`/v1/files/{file_id}/content`, place the returned text (not the file id) in the
messages, then delete the temporary remote file. This path must be enabled only
by an explicit Kimi/Moonshot provider configuration; arbitrary OpenAI-compatible
URLs must retain local extraction so a PDF is never uploaded by inference.
The PDF Agent exposes this choice as `pdf-transport`: `local-text` is the safe
default, while `kimi-file-extract` explicitly opts into the Kimi upload flow.
Runtime prompt metadata ships inside the `.myc` archive under
`prompts/manifest.yaml`, so installed release plugins do not depend on the
developer source tree.

For Vite development cold starts, stable Vue/Pinia/Vue Flow dependencies are
pre-bundled and the renderer entry, workspace shell, canvas, top bar, and
inspector are warmed. Conditional PDF, plugin-store, Agent review, Diff, and
workspace dialog surfaces are async chunks. The core canvas remains synchronous
to preserve the first usable workspace render.

Final verification passed Vue typechecking, ESLint, production Vite build,
platform/workspace/core/compiler tests, all three SDK parity checks, and the
native Rust suite (130/130). This pass produced the updated PDF Agent `.myc`
runtime archive for manual testing, but intentionally did not build a desktop
installer.
