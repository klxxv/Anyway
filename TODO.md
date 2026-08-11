# Research Canvas development TODO

Branch: `codex/responsive-i18n-myc`

## Responsive desktop UI

- [x] Contain the Vue Flow minimap at every supported viewport size.
- [x] Keep top-bar, canvas-toolbar, panel-tab, and inspector actions reachable while resizing.
- [x] Replace accidental responsive hiding with compact labels or horizontal overflow.
- [x] Verify 760×560, 1024×720, 1280×800, and 1440×900 layouts.

## Internationalization

- [x] Add a typed locale catalog and translation function.
- [x] Persist the selected locale.
- [x] Add English and Simplified Chinese.
- [x] Translate the primary navigation, canvas commands, settings, plugin store, and status messages.

## Plugin platform

- [x] Define a renderer-independent plugin manifest and runtime contract.
- [x] Define the `.myc` ZIP package layout with `plugin.yml` metadata.
- [x] Implement secure Tauri extraction into the application plugin directory.
- [x] Load installed theme plugins and expose their semantic color tokens.
- [x] Support native `.myc` drag-and-drop in the plugin store.
- [x] Add a Pythonic reference SDK for future source, connector, analysis, theme, and agent plugins.
- [x] Package and install a One Dark Pro theme plugin.

## Folder and dependency cleanup

- [x] Separate i18n, plugin contracts, built-in catalogs, responsive styles, and Tauri plugin commands.
- [x] Document module boundaries and package-building commands.
- [x] Keep graph semantics independent from Vue Flow, Tauri, and plugin UI code.

## Verification and delivery

- [x] Add unit coverage for i18n, manifests, and installed-theme normalization.
- [x] Run lint, TypeScript, core tests, rendered-shell tests, production build, and Cargo checks.
- [x] Update this TODO with results.
- [x] Commit the completed work.

## Block and connector rendering

- [x] Separate block appearance from graph semantics with research-card, compact-block, and signal-block presets.
- [x] Add Bézier, smooth-step, strict 90-degree orthogonal, and straight routing contracts.
- [x] Preserve semantic strokes for supporting, refuting, controlling, and measuring relations.
- [x] Precompute stable IN/OUT port geometry for Tauri WebView2 and SSR hydration.
- [x] Let declarative `.myc` packages register connector routing, stroke, dash, marker, and relation overrides.
- [x] Package, auto-install, and activate the permission-free Circuit Orthogonal connector plugin.
- [x] Re-space tree, table, Huffman, evidence, and neural-network projections for the new blocks.
- [x] Verify the MNIST effective chain in Tauri Zen mode with 90-degree plugin routing.

## Verification result

All automated checks passed. Playwright confirmed minimap containment, 10/10
reachable canvas commands, and no page-level horizontal overflow at all four
target viewport sizes. Tauri automatically extracted the packaged One Dark Pro
module, listed it in the plugin store, and applied it while Simplified Chinese
was active. The follow-up renderer pass also passed TypeScript, ESLint, 12 graph
core tests, 4 platform tests, 2 rendered-shell tests, and a production build.
Tauri rendered all 8 MNIST relations with explicit port geometry; the installed
Circuit Orthogonal `.myc` plugin switched them to strict 90-degree paths while
retaining green support, red dashed refutation, and muted control semantics.

## Agent plugin settings and host-rendered configuration

- [x] Finalize the cross-language setting contract for boolean, number, text, select, and secret values.
- [x] Add host-managed Agent model configuration for provider, credential, model, and thinking level.
- [x] Declare PDF Canvas Agent settings in its `.myc` manifest instead of hard-coding runtime choices.
- [x] Validate every setting definition and value in the Rust host; never trust values supplied in a plugin call envelope.
- [x] Persist non-secret values per `pluginId@version` and store secret values separately without returning plaintext to the UI.
- [x] Add Tauri commands to load, save, and reset effective plugin settings.
- [x] Extend the Pinia runtime plugin host with typed settings operations.
- [x] Render plugin-declared settings in a Vue SFC dialog using `<script setup>`, `<template>`, and scoped `<style>`.
- [x] Make Plugin Store rows open settings only when the plugin declares configurable fields.
- [x] Keep enable/disable, update, reset, and uninstall actions host-owned rather than plugin callbacks.
- [x] Add SDK declarations and parity checks for TypeScript, Python, Rust, and C++.
- [x] Add native validation/persistence tests and Vue/SDK rendering tests.
- [x] Run Vue typecheck, frontend tests/build, Rust tests, and SDK checks.
- [x] Update `HANDOFF.md` with final implementation state, known limitations, and verification results.

## Provider settings, PDF execution, and Vite cold start

- [x] Verify from the official DeepSeek compatibility documentation that
  response streaming is supported but PDF/document upload is not.
- [x] Verify the official Kimi Files workflow: multipart `file-extract` upload,
  extracted-content retrieval, text-message injection, and remote-file cleanup.
- [x] Extend the declarative plugin SDK with API URL, OpenAI/Anthropic format,
  credential source, and a host-rendered connection-test action.
- [x] Keep connection-test results bounded and redacted so API keys never cross
  the host-to-renderer boundary.
- [x] Make the PDF Agent consume host-validated settings and use a portable
  local-PDF-to-text path, with an explicit Kimi Files adapter where configured.
- [x] Add regression tests for provider payloads, PDF text handoff, settings
  rendering, secret handling, and connection-test feedback.
- [x] Pre-bundle the Vue canvas cold-start dependency set and lazy-load hidden
  dialogs without delaying the first canvas render.
- [x] Run focused Vue, SDK, Rust, and production-build gates; do not create a
  desktop installer in this pass.
- [x] Record the final verification results in the Vue migration handoff and
  commit the integrated change set.
