import assert from "node:assert/strict";
import test from "node:test";
import {
  EDGE_TYPES,
  LAYOUT_MODES,
  type ProjectState,
  type ResearchEdge,
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
  customEdgeNote,
  edgeTypeMessageKeys,
} from "../app/features/research-workspace/workspace-edge-labels";
import {
  measurePhysicalPinchSpan,
  shouldSuppressSyntheticPinchWheel,
  viewportForNativeTrackpadPinch,
  viewportForTrackpadWheel,
} from "../app/features/research-workspace/hooks/trackpad-pinch";

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
      defaultLayout: "table",
      showMiniMap: false,
      showLinkCounts: false,
      contextMenus: defaultWorkspacePreferences.contextMenus,
      showPluginContextMenuActions: true,
      shortcuts: defaultWorkspacePreferences.shortcuts,
    }),
    {
      commandDensity: "compact",
      hoverDelay: 80,
      defaultLayout: "table",
      showMiniMap: false,
      showLinkCounts: false,
      contextMenus: defaultWorkspacePreferences.contextMenus,
      showPluginContextMenuActions: true,
      shortcuts: defaultWorkspacePreferences.shortcuts,
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

test("browser and native trackpad pinch keep the cursor anchored and clamp scale", () => {
  const viewport = { x: 10, y: 20, zoom: 1 };
  const cursor = { x: 210, y: 170 };
  const zoomed = viewportForTrackpadWheel(viewport, cursor, -40);
  assert.ok(zoomed.zoom > viewport.zoom);
  assert.ok(Math.abs((cursor.x - zoomed.x) / zoomed.zoom - 200) < 1e-9);
  assert.ok(Math.abs((cursor.y - zoomed.y) / zoomed.zoom - 150) < 1e-9);
  assert.equal(
    viewportForTrackpadWheel({ ...viewport, zoom: 1.7 }, cursor, -10_000).zoom,
    1.7,
  );
  assert.equal(
    viewportForTrackpadWheel({ ...viewport, zoom: 0.45 }, cursor, 10_000).zoom,
    0.45,
  );
  assert.equal(
    measurePhysicalPinchSpan([
      { x: 1000, y: 2000 },
      { x: 4000, y: 6000 },
    ]),
    5000,
  );
  const nativeZoomed = viewportForNativeTrackpadPinch(viewport, cursor, 4000, 6000);
  assert.equal(nativeZoomed.zoom, 1.5);
  assert.ok(Math.abs((cursor.x - nativeZoomed.x) / nativeZoomed.zoom - 200) < 1e-9);
  assert.ok(Math.abs((cursor.y - nativeZoomed.y) / nativeZoomed.zoom - 150) < 1e-9);
});

test("Tauri keeps the WebView pinch fallback until native frames actually arrive", () => {
  assert.equal(shouldSuppressSyntheticPinchWheel(true, false), false);
  assert.equal(shouldSuppressSyntheticPinchWheel(false, true), false);
  assert.equal(shouldSuppressSyntheticPinchWheel(true, true), true);
});
