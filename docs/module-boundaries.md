# Module boundaries

## Stable kernel

`app/lib/research-types.ts` and `app/lib/research-core.ts` remain renderer- and
desktop-independent. They own semantic records, traversal, layouts, overlays,
influence propagation, exports, and migrations.

## Application features

- `app/i18n` owns locale catalogs and lookup.
- `app/plugins` owns plugin contracts, built-in catalogs, and the Tauri client.
- `app/styles` owns responsive behavior separately from the legacy component
  visual rules.
- `app/components` composes features and translates user actions into graph
  commands.

## Desktop boundary

`src-tauri/src/plugins.rs` is the only archive/filesystem installer. It validates
the API version, plugin kind, IDs, version, capability set, permissions, archive
paths, entry count, and expanded size before installation.

## Plugin source tree

`plugins/sources/<id>` is one module. `plugin.yml` is its public contract.
`plugins/packages/<id>@<version>.myc` is its distributable artifact.
`plugins/installed/<id>@<version>` is generated state and is ignored except for
the directory placeholder.
