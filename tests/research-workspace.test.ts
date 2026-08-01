import assert from "node:assert/strict";
import test from "node:test";
import { EDGE_TYPES, LAYOUT_MODES, type ResearchEdge } from "../app/lib/research-types";
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
  clampPieMenuPoint,
  isStableTwoFingerHold,
  measureTwoFingerFrame,
  selectPieNodeType,
  TWO_FINGER_HOLD_MS,
} from "../app/features/research-workspace/hooks/two-finger-gesture";

function edge(type: ResearchEdge["type"]): ResearchEdge {
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

test("two-finger hold rejects pinch span changes without blocking stable contact", () => {
  assert.equal(TWO_FINGER_HOLD_MS, 1000);
  const origin = measureTwoFingerFrame([
    { x: 100, y: 100 },
    { x: 160, y: 100 },
  ]);
  const stable = measureTwoFingerFrame([
    { x: 103, y: 102 },
    { x: 163, y: 102 },
  ]);
  const pinch = measureTwoFingerFrame([
    { x: 80, y: 100 },
    { x: 180, y: 100 },
  ]);
  assert.ok(origin && stable && pinch);
  assert.equal(isStableTwoFingerHold(origin, stable), true);
  assert.equal(isStableTwoFingerHold(origin, pinch), false);
});

test("pie selection covers all eight visible node directions", () => {
  const origin = { x: 100, y: 100 };
  assert.equal(selectPieNodeType(origin, { x: 100, y: 40 }), "question");
  assert.equal(selectPieNodeType(origin, { x: 145, y: 55 }), "concept");
  assert.equal(selectPieNodeType(origin, { x: 160, y: 100 }), "variable");
  assert.equal(selectPieNodeType(origin, { x: 145, y: 145 }), "method");
  assert.equal(selectPieNodeType(origin, { x: 100, y: 160 }), "dataset");
  assert.equal(selectPieNodeType(origin, { x: 55, y: 145 }), "evidence");
  assert.equal(selectPieNodeType(origin, { x: 40, y: 100 }), "result");
  assert.equal(selectPieNodeType(origin, { x: 55, y: 55 }), "note");
  assert.equal(selectPieNodeType(origin, { x: 110, y: 110 }), null);
});

test("pie menu is clamped inside compact canvases", () => {
  assert.deepEqual(clampPieMenuPoint({ x: 12, y: 780 }, 900, 820), {
    x: 148,
    y: 672,
  });
  assert.deepEqual(clampPieMenuPoint({ x: 20, y: 20 }, 240, 200), {
    x: 120,
    y: 100,
  });
});
