# Internationalization boundary

`catalog.ts` owns locale normalization and typed message lookup. UI components
depend on `translate(locale, key)` rather than an i18n framework, so the desktop
shell can later replace the catalog loader without changing graph semantics.

Project content is never translated implicitly. Only application chrome and
plugin metadata supplied with localized fields are localized.
