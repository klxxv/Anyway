import assert from "node:assert/strict";
import test from "node:test";
import {
  EDGE_TYPES,
  LAYOUT_MODES,
  type ProjectState,
  type ResearchEdge,
  type ResearchNode,
} from "../app/lib/research-types";
import { translate } from "../app/i18n/catalog";
import { zenWorkspaceFixture } from "../app/features/research-workspace/workspace-fixture";
import {
  edgeMatchesLegendFilter,
  layoutOptions,
  linkLegendFilterOf,
  projectForLegendFilter,
} from "../app/features/research-workspace/workspace-layout";
import {
  defaultWorkspacePreferences,
  normalizeWorkspacePreferences,
} from "../app/features/research-workspace/workspace-preferences";
import {
  defaultWorkspaceShortcuts,
  shortcutConflicts,
  shortcutFromKeyboardEvent,
} from "../app/features/research-workspace/workspace-shortcuts";
import { computeEdgeRoutes } from "../app/features/research-workspace/canvas/edge-routing";
import {
  compileRadialMenu,
  defaultRadialMenuPreferences,
  normalizeRadialMenuPreferences,
  radialSelectionForNormalizedDisplacement,
} from "../app/features/research-workspace/workspace-radial-menu";
import {
  customEdgeNote,
  edgeTypeMessageKeys,
} from "../app/features/research-workspace/workspace-edge-labels";
import {
  chromiumTrackpadPinchScale,
  emptyTrackpadLowPassState,
  lowPassCompleteTrackpadFrame,
  viewportForCoalescedWheelFrame,
  viewportForCompleteTrackpadFrame,
  wheelPanDelta,
} from "../app/features/research-workspace/hooks/trackpad-pinch";
import {
  isExpandableVariable,
  variableBranchValues,
} from "../app/features/research-workspace/canvas/variable-branches";

function edge(type: ResearchEdge["type"], override: Partial<ResearchEdge> = {}): ResearchEdge {
  return {
    id: `edge-${type}`,
    source: "source",
    target: "target",
    type,
    directed: true,
    polarity: type === "contradicts" ? "negative" : "positive",
    conditions: [],
    evidenceIds: [],
    provenance: { origin: "human" },
    ...override,
  };
}

test("restores every domain layout mode in the workspace menu", () => {
  assert.deepEqual(
    layoutOptions.map((option) => option.mode),
    [...LAYOUT_MODES],
  );
});

test("bundled workspace example is a complete Chinese research project", () => {
  assert.equal(zenWorkspaceFixture.schemaVersion, 2);
  assert.equal(zenWorkspaceFixture.title, "城市树冠与热岛效应");
  assert.equal(zenWorkspaceFixture.discipline, "城市气候研究");
  assert.equal(zenWorkspaceFixture.nodes.length, 9);
  assert.equal(zenWorkspaceFixture.edges.length, 9);
  assert.ok(zenWorkspaceFixture.nodes.every((node) => /[\u3400-\u9fff]/u.test(node.title)));
  assert.ok(zenWorkspaceFixture.edges.every((edge) => /[\u3400-\u9fff]/u.test(edge.note ?? "")));
});

test("legend families classify relation semantics deterministically", () => {
  assert.equal(linkLegendFilterOf(edge("causes")), "causal");
  assert.equal(linkLegendFilterOf(edge("supports")), "causal");
  assert.equal(linkLegendFilterOf(edge("controls")), "control");
  assert.equal(linkLegendFilterOf(edge("mediates")), "control");
  assert.equal(linkLegendFilterOf(edge("derived_from")), "derived");
  assert.equal(linkLegendFilterOf(edge("contradicts")), "contradicts");
  assert.equal(edgeMatchesLegendFilter(edge("uses"), "causal"), true);
  assert.equal(edgeMatchesLegendFilter(edge("uses"), "derived"), false);
});

test("every persisted relation type has English and Chinese UI copy", () => {
  assert.deepEqual(Object.keys(edgeTypeMessageKeys).sort(), [...EDGE_TYPES].sort());
  for (const type of EDGE_TYPES) {
    const key = edgeTypeMessageKeys[type];
    assert.ok(translate("en", key).trim());
    assert.ok(translate("zh-CN", key).trim());
    assert.notEqual(translate("zh-CN", key), type);
  }
});

test("legacy raw relation notes fall back to localized labels", () => {
  const legacy = edge("depends_on");
  legacy.note = "depends on";
  assert.equal(customEdgeNote(legacy), "");
  legacy.note = "仅在高温日成立";
  assert.equal(customEdgeNote(legacy), "仅在高温日成立");
});

test("legend projection keeps connected nodes without mutating the project", () => {
  const before = JSON.stringify(zenWorkspaceFixture);
  const projected = projectForLegendFilter(zenWorkspaceFixture, "contradicts");

  assert.equal(projected.edges.length, 1);
  assert.deepEqual(
    projected.nodes.map((node) => node.id).sort(),
    ["paper-nguyen", "paper-zhang"],
  );
  assert.deepEqual(
    projected.placements.map((placement) => placement.nodeId).sort(),
    ["paper-nguyen", "paper-zhang"],
  );
  assert.equal(JSON.stringify(zenWorkspaceFixture), before);
});

test("workspace preferences restore only supported values", () => {
  assert.deepEqual(normalizeWorkspacePreferences(null), defaultWorkspacePreferences);
  assert.deepEqual(
    normalizeWorkspacePreferences({
      commandDensity: "compact",
      hoverDelay: 80,
      trackpadSensitivity: 1,
      trackpadFilterStrength: 0.55,
      defaultLayout: "table",
      showMiniMap: false,
      showLinkCounts: false,
      contextMenus: defaultWorkspacePreferences.contextMenus,
      showPluginContextMenuActions: true,
      shortcuts: defaultWorkspacePreferences.shortcuts,
      radialMenu: defaultWorkspacePreferences.radialMenu,
    }),
    {
      commandDensity: "compact",
      hoverDelay: 80,
      trackpadSensitivity: 1,
      trackpadFilterStrength: 0.55,
      defaultLayout: "table",
      showMiniMap: false,
      showLinkCounts: false,
      contextMenus: defaultWorkspacePreferences.contextMenus,
      showPluginContextMenuActions: true,
      shortcuts: defaultWorkspacePreferences.shortcuts,
      radialMenu: defaultWorkspacePreferences.radialMenu,
    },
  );
  assert.deepEqual(
    normalizeWorkspacePreferences({
      hoverDelay: 999 as never,
      defaultLayout: "unknown" as never,
    }),
    defaultWorkspacePreferences,
  );
});

test("context menu preferences preserve scope-specific order and disabled actions", () => {
  const preferences = normalizeWorkspacePreferences({
    contextMenus: {
      node: ["node.delete", "node.inspect"],
      edge: [],
      canvas: ["canvas.fit", "unknown.action" as never],
    },
  });
  assert.deepEqual(preferences.contextMenus.node, ["node.delete", "node.inspect"]);
  assert.deepEqual(preferences.contextMenus.edge, []);
  assert.deepEqual(preferences.contextMenus.canvas, ["canvas.fit"]);
});

test("edge routing chooses facing sides and separates shared source lanes", () => {
  const routes = computeEdgeRoutes(zenWorkspaceFixture);
  assert.equal(routes["edge-canopy-temp"].sourceHandle, "left");
  assert.equal(routes["edge-canopy-temp"].targetHandle, "right");
  assert.equal(routes["edge-question-canopy"].sourceHandle, "bottom");
  assert.equal(routes["edge-question-canopy"].targetHandle, "top");
  assert.equal(routes["edge-canopy-landsat"].sourceHandle, "right-top");
  assert.equal(routes["edge-canopy-ndvi"].sourceHandle, "right-bottom");
  assert.notEqual(
    routes["edge-canopy-landsat"].labelOffsetY,
    routes["edge-canopy-ndvi"].labelOffsetY,
  );
  assert.equal(routes["edge-result-zhang"].sourceHandle, "bottom-left");
  assert.equal(routes["edge-result-nguyen"].sourceHandle, "bottom-right");
});

test("three downward sibling edges receive distinct border anchors", () => {
  const project = structuredClone(zenWorkspaceFixture) as ProjectState;
  project.nodes = project.nodes.filter((node) =>
    ["variable-canopy", "variable-temperature", "method-ndvi", "result-canopy"].includes(node.id),
  );
  project.placements = [
    { id: "p-source", viewId: "view-main", nodeId: "variable-canopy", x: 120, y: 0, width: 164, height: 116 },
    { id: "p-left", viewId: "view-main", nodeId: "variable-temperature", x: 0, y: 240, width: 164, height: 116 },
    { id: "p-center", viewId: "view-main", nodeId: "method-ndvi", x: 120, y: 240, width: 164, height: 116 },
    { id: "p-right", viewId: "view-main", nodeId: "result-canopy", x: 240, y: 240, width: 164, height: 116 },
  ];
  project.edges = [
    edge("causes", { id: "edge-left", source: "variable-canopy", target: "variable-temperature" }),
    edge("causes", { id: "edge-center", source: "variable-canopy", target: "method-ndvi" }),
    edge("causes", { id: "edge-right", source: "variable-canopy", target: "result-canopy" }),
  ];
  const routes = computeEdgeRoutes(project);
  assert.deepEqual(
    [routes["edge-left"].sourceHandle, routes["edge-center"].sourceHandle, routes["edge-right"].sourceHandle],
    ["bottom-left", "bottom", "bottom-right"],
  );
});

test("new editor, search, and settings surfaces have Chinese copy", () => {
  for (const key of [
    "composer.title",
    "composer.noteKind",
    "search.placeholder",
    "settings.comfortableHint",
    "settings.colorSystemSummary",
    "inspector.instances",
  ] as const) {
    assert.notEqual(translate("zh-CN", key), translate("en", key));
  }
});

test("shortcut capture is canonical and duplicate bindings are detected", () => {
  assert.equal(
    shortcutFromKeyboardEvent({
      key: "z",
      ctrlKey: true,
      altKey: false,
      shiftKey: true,
      metaKey: false,
    }),
    "Ctrl+Shift+Z",
  );
  assert.equal(
    shortcutFromKeyboardEvent({
      key: "Control",
      ctrlKey: true,
      altKey: false,
      shiftKey: false,
      metaKey: false,
    }),
    null,
  );
  assert.equal(shortcutConflicts(defaultWorkspaceShortcuts).size, 0);
  assert.deepEqual(
    [...shortcutConflicts({ ...defaultWorkspaceShortcuts, note: "A" })].sort(),
    ["add", "note"],
  );
});

test("one complete trackpad frame composes two-axis pan and zoom", () => {
  const viewport = { x: 10, y: 20, zoom: 1 };
  const cursor = { x: 210, y: 170 };
  const composed = viewportForCompleteTrackpadFrame(
    viewport,
    cursor,
    { x: 0.1, y: -0.05 },
    { width: 1000, height: 800 },
    1.5,
  );
  assert.deepEqual(composed, { x: 10, y: -95, zoom: 1.5 });

  const panOnly = viewportForCompleteTrackpadFrame(
    viewport,
    cursor,
    { x: 0.1, y: -0.05 },
    { width: 1000, height: 800 },
    1,
  );
  assert.deepEqual(panOnly, { x: 110, y: -20, zoom: 1 });

  assert.equal(
    viewportForCompleteTrackpadFrame(
      { ...viewport, zoom: 1.7 },
      cursor,
      { x: 0, y: 0 },
      { width: 1000, height: 800 },
      100,
    ).zoom,
    1.7,
  );
});

test("touchpad flick directions select configured radial actions", () => {
  const width = 4000;
  const height = 2400;
  const cache = compileRadialMenu(defaultRadialMenuPreferences);
  const select = (x: number, y: number) =>
    radialSelectionForNormalizedDisplacement(cache, x / width, y / height)?.item.action;
  assert.equal(select(0, -240), "create:question");
  assert.equal(select(400, -240), "create:concept");
  assert.equal(select(400, 0), "create:variable");
  assert.equal(select(400, 240), "create:method");
  assert.equal(select(0, 240), "create:dataset");
  assert.equal(select(-400, 240), "create:evidence");
  assert.equal(select(-400, 0), "create:result");
  assert.equal(select(-400, -240), "create:note");
  assert.equal(select(20, 10), undefined);
});

test("radial settings preserve unique positions, item count, and canvas functions", () => {
  const normalized = normalizeRadialMenuPreferences({
    items: [
      { id: "fit", position: "north", action: "canvas:fit" },
      { id: "layout", position: "east", action: "canvas:default-layout" },
      { id: "duplicate", position: "north", action: "create:note" },
    ],
  });
  assert.deepEqual(normalized.items, [
    { id: "fit", position: "north", action: "canvas:fit" },
    { id: "layout", position: "east", action: "canvas:default-layout" },
  ]);
  const cache = compileRadialMenu(normalized);
  assert.equal(cache.items.length, 2);
  assert.equal(cache.items[0].sectorIndex, 0);
  assert.equal(cache.items[1].sectorIndex, 2);
  assert.equal(radialSelectionForNormalizedDisplacement(cache, 0, -0.1)?.item.id, "fit");
  assert.equal(radialSelectionForNormalizedDisplacement(cache, 0.1, 0)?.item.id, "layout");
  assert.equal(radialSelectionForNormalizedDisplacement(cache, 0, 0.1), null);
});

test("Chromium trackpad pinch deltas invert to Chrome-compatible scale", () => {
  assert.ok(Math.abs(chromiumTrackpadPinchScale(-100 * Math.log(1.1)) - 1.1) < 1e-12);
  assert.ok(Math.abs(chromiumTrackpadPinchScale(-100 * Math.log(0.9)) - 0.9) < 1e-12);
  assert.equal(chromiumTrackpadPinchScale(-1000), 1.25);
  assert.equal(chromiumTrackpadPinchScale(1000), 0.75);
});

test("WebView wheel frames preserve and compose both pan axes", () => {
  assert.deepEqual(wheelPanDelta(3.5, -2.25, 0), { x: 3.5, y: -2.25 });
  assert.deepEqual(wheelPanDelta(1, -2, 1), { x: 20, y: -40 });
  assert.deepEqual(
    viewportForCoalescedWheelFrame(
      { x: 10, y: 20, zoom: 1 },
      { x: 200, y: 100 },
      { x: 12, y: -8 },
      1,
    ),
    { x: -2, y: 28, zoom: 1 },
  );
});

test("WebView wheel frames apply diagonal pan and anchored zoom atomically", () => {
  assert.deepEqual(
    viewportForCoalescedWheelFrame(
      { x: 0, y: 0, zoom: 1 },
      { x: 100, y: 80 },
      { x: 10, y: 20 },
      1.25,
    ),
    { x: -35, y: -40, zoom: 1.25 },
  );
});

test("enum and bool variables expose deterministic expandable branches", () => {
  const enumVariable = zenWorkspaceFixture.nodes.find(
    (node) => node.type === "variable" && node.data.valueType === "enum",
  );
  assert.ok(enumVariable);
  assert.deepEqual(variableBranchValues(enumVariable), ["低", "中", "高"]);
  const boolVariable: ResearchNode = {
    ...enumVariable,
    id: "bool-variable",
    data: { valueType: "bool" },
  };
  assert.deepEqual(variableBranchValues(boolVariable), ["true", "false"]);
  assert.equal(isExpandableVariable(boolVariable), true);
});

test("trackpad low-pass holds micro-motion still and preserves diagonal intent", () => {
  const still = lowPassCompleteTrackpadFrame(
    emptyTrackpadLowPassState(),
    { x: 0.0005, y: -0.0005 },
    1.001,
    1,
    0.7,
  );
  assert.deepEqual(still.pan, { x: 0, y: 0 });
  assert.equal(still.scale, 1);

  const moving = lowPassCompleteTrackpadFrame(
    still.state,
    { x: 0.04, y: -0.03 },
    1.08,
    1.25,
    0.55,
  );
  assert.ok(moving.pan.x > 0);
  assert.ok(moving.pan.y < 0);
  assert.ok(moving.scale > 1);
});

test("preferences supply trackpad defaults while preserving disabled canvas actions", () => {
  const preferences = normalizeWorkspacePreferences({
    commandDensity: "compact",
    contextMenus: {
      node: [],
      edge: [],
      canvas: ["canvas.fit"],
    },
  });
  assert.equal(preferences.trackpadSensitivity, 1);
  assert.equal(preferences.trackpadFilterStrength, 0.55);
  assert.deepEqual(preferences.contextMenus.canvas, ["canvas.fit"]);
});
