# Design QA — Zen Research Canvas

## Acceptance target

- Source visual truth: `C:\Users\admin\.codex\attachments\e63f07f0-b64f-40f3-9b7f-0e723d3e436f\image-1.png`
- Source dimensions: 1487 × 1027 px.
- Implementation capture: `C:\Users\admin\Documents\Anyway\output\design-qa\implementation-final-clean.jpg`
- Full-view comparison: `C:\Users\admin\Documents\Anyway\output\design-qa\comparison-final-clean.jpg`
- Focused comparisons:
  - `C:\Users\admin\Documents\Anyway\output\design-qa\focus-topbar.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\focus-canvas.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\focus-inspector.jpg`
- Capture viewport: 1443 × 931 physical/CSS px at device scale factor 1; the 29 px native
  title bar is excluded from the 1443 × 902 design comparison.
- Compared state: Urban Heat Islands fixture, Tree Canopy Cover selected, eight-way quick-add
  menu open, enum variable editor and observed bool fact visible.

## Intentional product differences

- The supplied warm paper/wood tint is replaced with white and near-white surfaces, per the
  user's explicit instruction.
- Gray-black remains the default hierarchy. The selected node, active command, minimap
  marker, focus rings, and primary actions use blue rather than the source's olive accent.
- The quick-add menu contains eight complete node kinds, including Group and Data, to retain
  the requested high-functionality interaction.

## Comparison history

### Pass 1

- [P1][layout] The inherited application shell had a permanent sidebar and a dense multi-panel
  layout unlike the canvas-first reference.
  - Fix: replaced the 5,821-line shell with an 8-line public entrypoint and a feature-based
    workspace composition root.
- [P1][surface] Legacy theme styles produced warm surfaces, heavy containers, and inconsistent
  spacing.
  - Fix: introduced Tailwind v4 tokens for white/paper, gray-black ink, blue state, and thin
    borders; removed three legacy global style contracts.

### Pass 2

- [P1][layout] React Flow's automatic fit shifted the central graph and radial menu away from
  the reference geometry.
  - Fix: calibrated the fixture, 320 px inspector, and the standard desktop viewport to
    `x=111`, `y=17`, `zoom=0.93`; smaller viewports keep fit-to-view fallback behavior.
- [P2][behavior] Escape did not close all transient states.
  - Fix: unified Escape handling for the project menu, search palette, composer, and connect
    mode.
- [P2][icons] The first radial version had six items and lacked the source's spokes.
  - Fix: implemented eight semantic items, eight dividers, a central Add action, and a real
    Tabler hand icon.

### Pass 3

- [P2][density] A permanently visible destructive button made the first inspector card taller
  and noisier than the reference.
  - Fix: moved deletion into the existing overflow menu.
- [P2][typography] The original fallback stack was too delicate at the graph's scaled viewport.
  - Fix: used the reference-aligned Georgia/Cambria/Times stack and retained compact, readable
    graph labels.
- [P2][functionality] Primary reference controls required interaction evidence.
  - Fix: verified Menu/reset, eight-way Add, Find/Escape, Connect/Escape, enum values, bool
    observed fact, inspector editing controls, minimap, zoom controls, and keyboard focus in
    the running Tauri application.

## Final rubric

| Surface | Result | Evidence |
| --- | --- | --- |
| Layout and spacing | Passed | Full-view comparison aligns top bar, breadcrumb, graph nodes, pie menu, minimap, legend, and inspector. |
| Typography | Passed | Serif hierarchy and wrapping match the source's research-document character without clipping. |
| Color and tokens | Passed | Intentional white/gray-black/blue mapping is consistent and accessible. |
| Icons and shapes | Passed | Tabler icons, thin graph outlines, circular question nodes, straight semantic edges, and eight pie wedges are complete. |
| Copy and content | Passed | Climate fixture, enum values, observed bool fact, methods, evidence, and relation labels use coherent realistic data. |
| States and interactions | Passed | Computer Use evidence covers navigation, search, add, connect, Escape dismissal, selected, active, and disabled states. |
| Accessibility | Passed | Semantic buttons/menus, labels, focus-visible styles, reduced-motion handling, and keyboard dismissal are present. |
| Viewport resilience | Passed | Reference geometry is exact at desktop size; widths below 1050 px switch to fit-to-view and the 1180 px density rule. |
| AI shortcut artifacts | Passed | No emoji, handcrafted SVG, placeholder illustration, decorative blob, or fake asset substitutes are used. |

No unresolved P0, P1, or P2 finding remains.

final result: passed
