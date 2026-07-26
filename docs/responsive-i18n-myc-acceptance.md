# Responsive, i18n, and `.myc` acceptance

Date: 2026-07-27  
Branch: `codex/responsive-i18n-myc`

## Responsive geometry

Playwright evaluated the rendered bounding boxes after each live resize:

| Viewport | Minimap contained | Canvas toolbar reachable | Page overflow |
|---|---:|---:|---:|
| 760×560 | yes | 10/10 | none |
| 1024×720 | yes | 10/10 | none |
| 1280×800 | yes | 10/10 | none |
| 1440×900 | yes | 10/10 | none |

At widths up to 1040 pixels the Navigator and Inspector become explicit overlay
panels. The core commands remain icon buttons instead of being removed. Tab
strips stay horizontally scrollable when their labels do not fit.

## Internationalization

- Locale catalog: English and Simplified Chinese.
- Default: normalized from the operating-system/browser language.
- Persistence: `research-canvas-display-v1`.
- Scope: primary application chrome, navigation, canvas commands, settings,
  plugin store, and plugin installation status.
- Research content is not translated implicitly.

## `.myc` package

Artifact:
`plugins/packages/researchcanvas.onedarkpro@1.0.0.myc`

```text
SHA-256 0285B39FD095A9045001571A80C5241846C248A0AD8BFEF9451A8265CE5FD279
Size    761 bytes
Files   plugin.yml, theme.json
```

The native client scanned `plugins/packages`, verified the package, extracted it
to `plugins/installed/researchcanvas.onedarkpro@1.0.0`, registered One Dark Pro,
and applied the theme. Native file-drop installation calls the same Rust
installer.

Installer validation includes:

- `.myc` extension and 16 MB archive limit
- at most 128 entries and 32 MB expanded data
- ZIP enclosed paths to block traversal
- exact API version, safe ID and version slugs
- `ThemePlugin` kind, `theme.json` entry, and `theme.register`
- no permissions for declarative theme packages
- manifest identity recheck after extraction

## Automated checks

```text
npm run lint             pass
npx tsc --noEmit         pass
npm run test:core        12/12 pass
npm run test:platform    3/3 pass
rendered shell tests     2/2 pass
npm run build            pass
cargo check              pass
git diff --check         pass
```

## Visual evidence

Screenshots are in `output/responsive-acceptance`:

1. `01-760x560.png`
2. `02-1024x720.png`
3. `03-1280x800.png`
4. `04-1440x900.png`
5. `05-myc-plugin-store.png`
6. `06-onedarkpro-zh-settings.png`
7. `07-onedarkpro-canvas.png`

