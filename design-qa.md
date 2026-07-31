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
- Follow-up source visual truth:
  - `C:\Users\admin\AppData\Local\Temp\codex-clipboard-9756470c-bf58-4b3b-9d3c-192e1528c31e.png` (393 × 138 px, remove gesture hand)
  - `C:\Users\admin\AppData\Local\Temp\codex-clipboard-a74cab1e-14c6-4097-970a-3545990e6745.png` (574 × 651 px, enrich composer)
  - `C:\Users\admin\AppData\Local\Temp\codex-clipboard-73133fa0-3080-4640-8178-7f05e69c61a5.png` (270 × 235 px, interactive legend)
- Follow-up implementation captures:
  - `C:\Users\admin\Documents\Anyway\output\design-qa\workspace-radial-menu.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\workspace-variable-composer.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\workspace-layout-menu.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\workspace-derived-filter.jpg`
- Follow-up normalized comparison:
  `C:\Users\admin\Documents\Anyway\output\design-qa\comparison-source-vs-implementation.jpg`
  (1288 × 1600 px; source on the left, implementation on the right).
- Hover/settings source visual truth:
  - `C:\Users\admin\AppData\Local\Temp\codex-clipboard-0694ee0b-67a6-4f67-ab48-03dc8d6caf57.png` (835 × 153 px, command bar)
  - `C:\Users\admin\AppData\Local\Temp\codex-clipboard-f7ce832c-f6d7-480b-a351-30859578bf8f.png` (353 × 480 px, layout menu)
- Hover/settings implementation captures:
  - `C:\Users\admin\Documents\Anyway\output\design-qa\hover-add-menu.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\hover-connect-menu.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\hover-layout-menu.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\settings-interface.jpg`
  - `C:\Users\admin\Documents\Anyway\output\design-qa\workspace-final-hover-settings-clean.jpg`
- Hover/settings normalized comparison:
  `C:\Users\admin\Documents\Anyway\output\design-qa\comparison-hover-settings.jpg`
  (1480 × 1430 px; source and implementation are paired in the same image input).
- Trackpad/overflow source visual truth:
  `C:\Users\admin\AppData\Local\Temp\codex-clipboard-cc5a3e37-f37c-411e-af6a-231f421749f4.png`
  (373 × 238 px, compact minimap and zoom controls with node overflow).
- Trackpad/overflow implementation capture:
  `C:\Users\admin\Documents\Anyway\output\design-qa\trackpad-overflow-implementation.png`
  (1443 × 931 px, running Tauri desktop window).
- Trackpad/overflow focused capture:
  `C:\Users\admin\Documents\Anyway\output\design-qa\trackpad-overflow-focused.png`
  (373 × 238 px after cropping the 250 × 160 px lower-left region and normalizing it to
  the source dimensions).
- Trackpad/overflow same-input comparison:
  `C:\Users\admin\Documents\Anyway\output\design-qa\trackpad-overflow-comparison.png`
  (746 × 238 px; source left, corrected implementation right).
- Capture viewport: 1443 × 931 physical/CSS px at device scale factor 1; the 29 px native
  title bar is excluded from the 1443 × 902 design comparison.
- Compared state: Urban Heat Islands fixture, Tree Canopy Cover selected, eight-way quick-add
  menu open, enum variable editor and observed bool fact visible. Follow-up captures use the
  running Tauri window at 1442 × 930 px and device scale factor 1.

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

### Pass 4

- [P1][functionality] The node composer exposed only type, title, and note, so unlike research
  objects were forced through the same shallow schema.
  - Fix: added type-specific profiles for question, group, variable, method, data, evidence,
    result, and note nodes. Variable profiles support enum, bool, number, and text schemas;
    method and evidence profiles retain reproducibility and provenance-oriented metadata.
- [P1][functionality] Layout had no discoverable way to restore the six domain projections.
  - Fix: added a Layout dropdown with Evidence chain, Refutation chain, Tree, Huffman, Table,
    and Neural network modes, including descriptions and active-state feedback.
- [P1][behavior] The link legend was static and could not focus the graph.
  - Fix: converted all four legend rows into semantic toggle buttons with counts, selected
    states, relation projection, and automatic re-layout. Clicking the active row restores
    all relations and re-runs the current layout.
- [P2][visual hierarchy] The persistent hand illustration competed with the eight-way radial
  chooser after the interaction had already been learned.
  - Fix: removed the hand and gesture caption while preserving the radial node affordance.
- [P3][copy] Resetting the demo could leave the previous layout toast visible.
  - Fix: clear transient notice text when restoring the reference demo.
- Post-fix evidence: the same-input comparison shows the enriched composer, hand-free radial
  menu, and count-bearing legend. The dedicated filtered-state capture shows only the derived
  relation and its connected nodes after automatic re-layout.

### Pass 5

- [P1][behavior] Add, Connect, and Layout required clicks before their available actions became
  discoverable.
  - Fix: introduced one shared delayed-hover disclosure contract. Pointer hover opens each menu,
    keyboard focus opens it immediately, and the primary button click still performs its direct
    action.
- [P1][icons] Layout descriptions were text-only and slow to distinguish at a glance.
  - Fix: assigned a real Tabler SVG icon to every node, relation, and layout option; active
    relation and layout choices retain blue state feedback.
- [P1][functionality] The project menu had no settings destination.
  - Fix: added a persistent Settings action at the bottom and a three-section settings interface
    for command density, hover delay, default filter layout, minimap visibility, and link counts.
    Settings are normalized before restoration and persist in local storage.
- [P2][state] Visual QA changed command density and canvas pan while testing.
  - Fix: restored default preferences, refreshed the Tauri window, and captured a clean final
    state without transient text selection.
- Post-fix evidence: Computer Use opened Add and Connect by pointer entry, opened Layout with
  distinct icons, entered Settings from the menu footer, saved Compact density, observed the
  topbar change, and restored defaults. The paired comparison has no unresolved P0/P1/P2 drift.

### Pass 6

- [P1][behavior] A stable two-finger contact and a pinch shared the same center-point heuristic,
  so zoom gestures could be mistaken for the quick-add hold; a two-finger context click could
  also open the pie immediately.
  - Fix: added an explicit 1000 ms hold recognizer that tracks both midpoint drift and finger
    span. Span changes cancel the hold and remain available to React Flow pinch zoom. Context
    clicks are suppressed instead of opening the pie.
- [P1][native input] Browser pointer events do not consistently expose stationary Precision
  Touchpad contacts on Windows.
  - Fix: added an observation-only `PT_TOUCHPAD` window procedure bridge in Tauri. It forwards
    contact phase, identity, count, and client coordinates without taking over system gestures;
    browser touch handling remains the fallback if native registration is unavailable.
- [P1][layout] MiniMap node rectangles could paint beyond the rounded MiniMap frame at compact
  sizes.
  - Fix: constrained the MiniMap and its SVG to the component box with `overflow: hidden`,
    inherited radius, and paint containment. The same-input focused comparison shows every gray
    and blue node marker inside the border after the fix.
- [P2][copy] The reference project and inspector mixed English fixture text into a Chinese UI.
  - Fix: replaced the bundled fixture with the complete Chinese “城市树冠与热岛效应” project,
    migrated only the built-in demo from schema v1 to v2, and localized the inspector labels and
    observed-fact example.
- [P2][selection feedback] Direction choice during the hold gesture was not visually emphatic.
  - Fix: the active wedge now uses the blue state token, heavier label weight, stronger icon
    stroke, and a live center label; release creates the highlighted node, while release in the
    dead zone dismisses the menu.
- Post-fix evidence: the 373 × 238 same-input comparison normalizes the compact lower-left
  region and shows the overflow removed without changing the white/gray-black/blue design
  language. The full 1443 × 931 Tauri capture verifies the Chinese project and inspector. Pure
  gesture tests cover the one-second threshold, pinch rejection, eight directions, and edge
  clamping; physical multi-touch injection is not available through Computer Use.

## Final rubric

| Surface | Result | Evidence |
| --- | --- | --- |
| Layout and spacing | Passed | Full-view comparison aligns top bar, breadcrumb, graph nodes, pie menu, minimap, legend, and inspector. |
| Typography | Passed | Serif hierarchy and wrapping match the source's research-document character without clipping. |
| Color and tokens | Passed | Intentional white/gray-black/blue mapping is consistent and accessible. |
| Icons and shapes | Passed | Tabler icons, thin graph outlines, circular question nodes, straight semantic edges, and eight pie wedges are complete; the obsolete gesture hand is absent. |
| Copy and content | Passed | The complete Chinese climate fixture, enum values, observed bool fact, methods, evidence, relation labels, and inspector copy form one coherent example. |
| States and interactions | Passed | Computer Use evidence covers navigation, search, hover-open Add/Connect/Layout, direct clicks, six layout modes, four legend filters, settings persistence, automatic re-layout, Escape dismissal, selected, active, and disabled states. Automated gesture tests cover the non-injectable physical multi-touch path. |
| Accessibility | Passed | Semantic buttons/menus, labels, focus-visible styles, reduced-motion handling, and keyboard dismissal are present. |
| Viewport resilience | Passed | Reference geometry is exact at desktop size; widths below 1050 px switch to fit-to-view, and the compact MiniMap clips all markers inside its border. |
| AI shortcut artifacts | Passed | No emoji, handcrafted SVG, placeholder illustration, decorative blob, or fake asset substitutes are used. |

No unresolved P0, P1, or P2 finding remains.

final result: passed
