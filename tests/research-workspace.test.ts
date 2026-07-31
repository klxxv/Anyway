import assert from "node:assert/strict";
import test from "node:test";
import { LAYOUT_MODES, type ResearchEdge } from "../app/lib/research-types";
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
    }),
    {
      commandDensity: "compact",
      hoverDelay: 80,
      defaultLayout: "table",
      showMiniMap: false,
      showLinkCounts: false,
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
