import assert from "node:assert/strict";
import test from "node:test";
import {
  buildDiffOverlay,
  computeLocalDiff,
  emptyDiffResult,
  type CanvasDiffResult,
} from "../app/lib/graph/canvas-diff";
import { zenWorkspaceFixture } from "../app/features/research-workspace/workspace-fixture";
import { cloneProject } from "../app/features/research-workspace/hooks/commit-logic";
import type { ProjectState } from "../app/lib/research-types";

function fixture(): ProjectState {
  return cloneProject(zenWorkspaceFixture);
}

function summarize(result: CanvasDiffResult) {
  return {
    addedNodes: result.addedNodes.length,
    removedNodes: result.removedNodes.length,
    modifiedNodes: result.modifiedNodes.length,
    addedEdges: result.addedEdges.length,
    removedEdges: result.removedEdges.length,
    modifiedEdges: result.modifiedEdges.length,
    addedEvidence: result.addedEvidence.length,
    removedEvidence: result.removedEvidence.length,
    modifiedEvidence: result.modifiedEvidence.length,
  };
}

// ---------------------------------------------------------------------------
// 幂等性 / 确定性
// ---------------------------------------------------------------------------

test("diff(P, P) is empty (idempotence)", async () => {
  const project = fixture();
  const result = await computeLocalDiff(project, project);
  assert.deepEqual(summarize(result), {
    addedNodes: 0,
    removedNodes: 0,
    modifiedNodes: 0,
    addedEdges: 0,
    removedEdges: 0,
    modifiedEdges: 0,
    addedEvidence: 0,
    removedEvidence: 0,
    modifiedEvidence: 0,
  });
  assert.deepEqual(result.changedBlockHashes, {});
  assert.ok(result.durationMs >= 0);
});

test("diff is deterministic across runs", async () => {
  const project = fixture();
  const first = await computeLocalDiff(project, fixture());
  const second = await computeLocalDiff(project, fixture());
  const { durationMs: firstMs, ...firstRest } = first;
  const { durationMs: secondMs, ...secondRest } = second;
  assert.ok(firstMs >= 0 && secondMs >= 0);
  assert.deepEqual(firstRest, secondRest);
});

// ---------------------------------------------------------------------------
// 基本变更检测
// ---------------------------------------------------------------------------

test("detects added, removed and modified nodes", async () => {
  const base = fixture();
  const compare = fixture();
  compare.nodes.push({
    ...base.nodes[0],
    id: "new-node",
    title: "Brand new",
  });
  compare.nodes[0].title = "Renamed node";
  compare.nodes = compare.nodes.filter((node) => node.id !== "variable-canopy");

  const result = await computeLocalDiff(base, compare);
  assert.ok(result.addedNodes.includes("new-node"));
  assert.ok(result.removedNodes.includes("variable-canopy"));
  const modified = result.modifiedNodes.find((entity) => entity.entityId === base.nodes[0].id);
  assert.ok(modified, "first node should be modified");
  assert.notEqual(modified.oldBlockHash, modified.newBlockHash);
  assert.ok(result.changedBlockHashes["new-node"][0] === "");
  assert.ok(result.changedBlockHashes["variable-canopy"][1] === "");
});

test("detects edge and evidence changes", async () => {
  const base = fixture();
  const compare = fixture();
  compare.edges[0].confidence = (compare.edges[0].confidence ?? 0.5) + 0.1;
  compare.evidence.push({
    ...base.evidence[0],
    id: "evidence-extra",
    title: "Extra source",
  });
  const result = await computeLocalDiff(base, compare);
  assert.ok(result.modifiedEdges.length >= 1);
  assert.ok(result.addedEvidence.includes("evidence-extra"));
});

// ---------------------------------------------------------------------------
// 对称性
// ---------------------------------------------------------------------------

test("symmetry swaps added and removed", async () => {
  const base = fixture();
  const compare = fixture();
  compare.nodes.push({ ...base.nodes[0], id: "extra", title: "Extra" });
  compare.nodes = compare.nodes.filter((node) => node.id !== "variable-canopy");

  const forward = await computeLocalDiff(base, compare);
  const backward = await computeLocalDiff(compare, base);
  assert.deepEqual(forward.addedNodes, backward.removedNodes);
  assert.deepEqual(forward.removedNodes, backward.addedNodes);
});

// ---------------------------------------------------------------------------
// 语义区边界：布局/元数据/悬挂字段不入 diff
// ---------------------------------------------------------------------------

test("layout, title and evidenceIds changes do not enter the semantic diff", async () => {
  const base = fixture();
  const compare = fixture();
  compare.title = "Renamed project";
  compare.placements[0].x = 999;
  (compare.nodes[0] as ProjectState["nodes"][number] & { layout?: unknown }).layout = { x: 1, y: 2 };
  compare.nodes[0].evidenceIds = ["other-evidence"];
  compare.nodes[0].status = "confirmed";

  const result = await computeLocalDiff(base, compare);
  assert.deepEqual(summarize(result), {
    addedNodes: 0,
    removedNodes: 0,
    modifiedNodes: 0,
    addedEdges: 0,
    removedEdges: 0,
    modifiedEdges: 0,
    addedEvidence: 0,
    removedEvidence: 0,
    modifiedEvidence: 0,
  });
});

// ---------------------------------------------------------------------------
// 画布 overlay 构建
// ---------------------------------------------------------------------------

test("buildDiffOverlay injects ghosts for removed entities", async () => {
  const base = fixture();
  const compare = fixture();
  compare.nodes = compare.nodes.filter((node) => node.id !== "variable-canopy");
  compare.edges = compare.edges.filter(
    (edge) => edge.source !== "variable-canopy" && edge.target !== "variable-canopy",
  );
  compare.nodes.push({ ...base.nodes[0], id: "fresh", title: "Fresh" });

  const result = await computeLocalDiff(base, compare);
  const overlay = buildDiffOverlay(result, base, compare);
  assert.equal(overlay.nodes["fresh"], "added");
  assert.equal(overlay.nodes["variable-canopy"], "removed");
  assert.ok(overlay.removedNodes.some((node) => node.record.id === "variable-canopy"));
  // removed 边：端点均存在（幽灵或现存）才注入幽灵边。
  for (const ghostEdge of overlay.removedEdges) {
    const present = new Set([
      ...compare.nodes.map((node) => node.id),
      ...overlay.removedNodes.map((node) => node.record.id),
    ]);
    assert.ok(present.has(ghostEdge.record.source));
    assert.ok(present.has(ghostEdge.record.target));
  }
  assert.equal(emptyDiffResult().addedNodes.length, 0);
});
