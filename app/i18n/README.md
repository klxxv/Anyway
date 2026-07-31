# Internationalization boundary

`catalog.ts` owns locale normalization and typed message lookup. `provider.tsx`
owns the device-local `research-canvas.locale.v1` preference, keeps the document
language synchronized, and exposes `useI18n()` to the workspace.

UI components depend on typed message keys rather than an i18n framework, so a
future catalog loader can replace the bundled dictionaries without changing
graph semantics. English is the fallback for every key; tests require both
English and Simplified Chinese catalogs to be complete and non-empty.

Project content and plugin-authored metadata are never translated implicitly.
Application chrome, commands, settings, store controls, node-kind labels, link
semantics, and layout choices use the selected UI locale.
