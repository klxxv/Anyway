# Research Canvas development TODO

Branch: `codex/responsive-i18n-myc`

## Responsive desktop UI

- [x] Contain the React Flow minimap at every supported viewport size.
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
- [x] Keep graph semantics independent from React Flow, Tauri, and plugin UI code.

## Verification and delivery

- [x] Add unit coverage for i18n, manifests, and installed-theme normalization.
- [x] Run lint, TypeScript, core tests, rendered-shell tests, production build, and Cargo checks.
- [x] Update this TODO with results.
- [x] Commit the completed work.

## Verification result

All automated checks passed. Playwright confirmed minimap containment, 10/10
reachable canvas commands, and no page-level horizontal overflow at all four
target viewport sizes. Tauri automatically extracted the packaged One Dark Pro
module, listed it in the plugin store, and applied it while Simplified Chinese
was active.
