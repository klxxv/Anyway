# Plugin Runtime Layout

This document defines the source, staging, and installed-state directories for
Anyway plugin development. The rule is simple: source directories are never the
runtime. Every runnable plugin is first packed or copied into an ignored runtime
directory, then Desktop installs from that staged package.

## Directory Table

| Layer | Path | Git | Owner | Purpose |
|---|---|---|---|---|
| Official/development source | `my-plugins/<plugin-folder>/` | tracked | plugin developers | Editable plugin sources such as `anPdfsolver`, `ancordis`, and `anmarket`. These are not scanned by Desktop dev unless explicitly enabled. |
| Third-party source/cache | `my-third-plugins/` | ignored | local user | Local marketplace downloads, imported packages, Japanese locale package, One Dark Pro package, or other external packages. Never auto-loaded by Desktop dev. |
| Dev staged runtime | `.plugin-runtime/dev/` | ignored | staging script + Desktop dev | Generated runtime used by `npm run desktop dev`. Contains `packages/`, `installed/`, `quarantine/`, and `dev-manifest.json`. |
| Test staged runtime | `.plugin-runtime/test/` | ignored | automated tests | Disposable runtime used by staging tests so test cleanup never mutates the developer's active runtime. |
| Release staged runtime | `.plugin-runtime/release-staging/` | ignored | release packaging | Generated release resource input. The release bundle copies only explicit packages listed in `config/plugin-loading.json`. |
| Formal installed state | Desktop-managed app data | outside repo | Desktop installer | Verified expanded packages and user install state. This location is not a development source root and is managed by the application. |

## Desktop Dev Policy

`config/plugin-loading.json` is the only authority for staged packages. Desktop
dev defaults to `official-bundled-only`, which means:

- official packages listed in `desktopDev.packageFiles` are staged;
- `my-third-plugins/` is never scanned;
- Japanese locale and One Dark Pro packages are not in the default list;
- `my-plugins/` sources are not loaded merely because they exist;
- development sources are staged only when the id appears in
  `desktopDev.enabledDevelopmentPluginIds` or is passed through
  `--with-dev-plugin <pluginId>`.

This keeps plugin development source, local external packages, and the active
runtime separate. It also prevents a stale source tree from becoming active just
because a developer has it checked out.

## Command Flow

Stage the default desktop-dev runtime:

```bash
node scripts/stage-plugin-runtime.mjs dev
```

Stage desktop dev with one explicitly declared development plugin:

```bash
node scripts/stage-plugin-runtime.mjs dev --with-dev-plugin myc.pdf-canvas-agent
```

Stage release resources:

```bash
node scripts/stage-plugin-runtime.mjs release
```

Stage an isolated disposable test runtime:

```bash
node scripts/stage-plugin-runtime.mjs test
```

Pack a plugin source into a package:

```bash
node scripts/pack-plugin.mjs my-plugins/anPdfsolver .plugin-runtime/dev/packages/myc.pdf-canvas-agent@0.5.3.myc
```

Clean one exact staged plugin version from the generated runtime:

```bash
node scripts/stage-plugin-runtime.mjs dev --clean-plugin myc.pdf-canvas-agent@0.5.3
```

The clean command accepts only an exact `pluginId@version` token and removes
only matching paths inside the selected `.plugin-runtime/*` root.

## Desktop Wiring

Desktop dev discovery, release resource staging, and Rust loader resolution are
wired to the generated staging boundary: `.plugin-runtime/dev` for development
and `.plugin-runtime/release-staging` for release resources. Formal installation
continues to live in Desktop-managed application data, outside the repository.

The staged package for `anPdfsolver` contains a trusted frontend module:
`frontend.mode="trusted-module"`, `frontend.entry="dist/frontend.mjs"`, and
`contributes.ui` entries that target physical Host slots. The plugin's internal
Vue component slots remain ordinary Vue implementation details and are not part
of the Host Slot Catalog.
