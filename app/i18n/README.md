# Internationalization boundary

`catalog.ts` owns locale normalization and typed message lookup.
`src/vue/runtime/i18n.ts` owns the device-local `research-canvas.locale.v1`
preference, keeps the document language synchronized, and exposes `useI18n()`
to the workspace.

UI components depend on typed message keys rather than an i18n framework.
English and Simplified Chinese are the only host-bundled languages and remain
complete, non-empty catalogs. English is the fallback for every missing key.

Every additional language is delivered as a declarative `LocalePlugin`. The
package declares `i18n.register`, contributes one or more `locales/<tag>.json`
files, and is merged only while that installed package is enabled. Locale files
are bounded string-to-string maps; they contain no executable code and cannot
replace graph content, project schemas, or native commands.

```yaml
kind: LocalePlugin
spec:
  engine: declarative
  entry: locales/ja-JP.json
  capabilities: [i18n.register]
  permissions: []
  contributes:
    locales:
      - locale: ja-JP
        name: 日本語
        path: locales/ja-JP.json
```

The optional Japanese package is kept outside the development source tree as
`my-third-plugins/myc.i18n-ja@1.0.1.myc`. That directory is ignored and is never
loaded by desktop development unless the user explicitly installs the package.
A partial community catalog is valid: unresolved keys deliberately fall back
to English.

Project content and plugin-authored metadata are never translated implicitly.
Application chrome, commands, settings, store controls, node-kind labels, link
semantics, and layout choices use the selected UI locale.
