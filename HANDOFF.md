# Agent Plugin Settings Handoff

Updated: 2026-08-10

## Objective

Move PDF Agent configuration ownership into the plugin contract while keeping credentials, model invocation, persistence, validation, and review enforcement in the trusted Rust host. The Plugin Store must parse the installed manifest and render the declared configuration through Vue and Pinia.

## Architectural boundary

The plugin owns declarative setting definitions, model preferences, prompts, schemas, and domain behavior. The host owns file authorization, secret storage, provider calls, retries, job checkpoints, GraphPatch validation, review gates, update, and uninstall.

Plugin settings are data, never UI callbacks. The host renders all controls. A setting cannot grant a capability that the manifest did not declare.

## Current baseline

- `app/plugins/contracts.ts` and `src-tauri/src/plugins.rs` already contain partial boolean/number/text/select setting definitions.
- Rust manifest validation already checks setting IDs, labels, bounds, select options, and default value types.
- The Python reference SDK has a partial `PluginSetting` type.
- `PluginStoreItem.vue` already has an optional `onOpenSettings` activation hook.
- `PluginStoreDialog.vue` does not yet open a settings editor.
- No native setting-value repository, get/save/reset commands, secret redaction, or execution-time setting injection exists.
- PDF Canvas Agent is still `host-mediated`; its active PDF pipeline does not currently invoke the repository's dormant semantic LLM pipeline.

## Target contract

- Ordinary values: boolean, number, text, select.
- Sensitive values: secret definitions rendered as password controls; plaintext is write-only from the UI and must not be returned by read APIs.
- Agent model controls: provider/model/thinking are declared by the PDF Agent and resolved by the host. API credentials remain host-managed.
- Values are isolated by exact `pluginId@version` for the first implementation.
- The native host resolves defaults plus stored overrides and validates the effective result on load, save, and execution.

## Planned work split

1. SDK and manifest worker: cross-language contract parity, PDF Agent declarations, examples, and contract tests.
2. Native host worker: setting-value validation, persistence, secret redaction, Tauri commands, and Rust tests.
3. Vue worker: Pinia/Tauri client integration and host-rendered settings dialog.
4. Main integration: reconcile contracts, connect command registration, run all checks, fix regressions, and update this handoff.

## Safety invariants

- Never serialize plaintext secrets into installed plugin metadata, Pinia snapshots, logs, Job checkpoints, or plugin call payloads.
- Plugins do not receive filesystem paths, network access, or Graph Store write access from settings.
- Update and uninstall remain host-owned actions.
- Graph mutations remain `reviewRequired` and flow through the project store.

## Verification required

- `npm run typecheck:vue`
- focused plugin SDK and workspace tests
- `npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm run test:sdk`

## Final implementation state

Completed:

- The shared TypeScript contract and Python, Rust, and C++ SDKs declare bounded settings. Canonical secrets use `type: text` plus `secret: true`; legacy `type: secret` input is normalized to that shape.
- PDF Canvas Agent `0.2.0` declares a required write-only `api-key`, plus required `model` (`luna`) and `thinking` (`extra_high`) settings. Its Agent manifest explicitly states that only the Host ModelGateway may consume the credential.
- Rust validates manifest definitions, user values, bounds, select options, required values at execution, developer UUIDs, and update metadata. It exposes `get_plugin_settings`, `set_plugin_settings`, and `reset_plugin_settings` Tauri commands.
- Public overrides are atomically persisted under the Tauri config directory by exact `pluginId@version`. Credentials are held only in Host process memory and are returned solely as `secretConfigured` booleans.
- `execute_myc_plugin` replaces any frontend-provided `host` value with a host allow-list. Guests receive only non-secret `effectiveValues` and `secretConfigured`; plaintext credentials are neither serialized nor exposed through an SDK getter.
- The Pinia runtime Plugin Host loads, saves, resets, and caches snapshots. `PluginSettingsDialog.vue` uses SFC `<script setup>`, `<template>`, and scoped `<style>` and renders boolean, number, text, select, and password controls from declarative settings.
- Plugin Store cards expose settings only when the manifest has definitions. Reset, update state, enable/disable, and uninstall remain Host-owned actions; update is intentionally disabled until a trusted update flow is implemented.
- The final package is [myc.pdf-canvas-agent@0.4.0.myc](C:/Users/admin/Documents/Anyway/plugins/packages/myc.pdf-canvas-agent@0.4.0.myc), SHA-256 `90885224ace6aeb700f57b6b288c26b62df4a3a324b4d51ea75f775bc6ff7936`. This is the official v4 build: `official: true`, myc.llm.v4 extraction prompts, host-bus transport (`graph.ir.compile` → `graph.storage.put` → `event.publish`), and a declared Vue IR review surface; the historical 0.1.0–0.3.0 packages were removed with the legacy semantic pipeline.

## Known limitations / next handoff

- Credentials are process-memory-only, not yet stored in Windows Credential Manager or another OS credential vault. They must be entered again after restarting the app.
- The existing `start_pdf_job` pipeline remains local PDF extraction, structural analysis, and review-gated GraphPatch generation. It does not yet invoke an external LLM, so the new API key/model/thinking values are stored and rendered but are not consumed until the Host ModelGateway and `agent.yml` executor are wired in.
- `agent.yml`, prompts, and schemas are shipped as constrained declarative assets. The Host currently validates/packages them but does not execute the Pass A–F DAG.
- `npm test` currently stops after the successful frontend build because its pre-existing aggregate script references missing `tests/rendered-html.test.mjs`. All callable component suites listed below passed independently.

## Verification

- `npm run typecheck:vue` — passed.
- `npm run build` — passed (Vite reports the pre-existing large-chunk warning only).
- `npm run test:core` — 12 passed.
- `npm run test:platform` — 17 passed, including settings normalization/redaction and PDF Agent declaration tests.
- `npm run test:workspace` — 70 passed.
- `npm run test:sdk` — passed, including Rust/C++ ABI and SDK secret-boundary checks.
- `npm run test:compiler` — 24 passed.
- `cargo test --manifest-path src-tauri/Cargo.toml` — 119 library tests plus 8 integration tests passed.
- `npm test` — blocked only by the missing aggregate-rendered HTML test file noted above.
