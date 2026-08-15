import assert from "node:assert/strict";
import test from "node:test";
import { cloneProject } from "../app/features/research-workspace/hooks/commit-logic";
import { zenWorkspaceFixture } from "../app/features/research-workspace/workspace-fixture";
import {
  adaptProjectToCanvasDiffDocument,
  adaptPdfAgentResultForDiff,
  computeCanvasDiffBatch,
  createCanvasDiffEntityRef,
  type CanvasDiffDocumentInput,
} from "../app/domain/canvas-diff";
import type { PluginGraphPatch } from "../app/plugins/contracts";

function document(documentId: string): CanvasDiffDocumentInput {
  return adaptProjectToCanvasDiffDocument(cloneProject(zenWorkspaceFixture), {
    documentId,
    provenance: { origin: "import", fileName: `${documentId}.pdf` },
  });
}

test("document-scoped refs prevent same local ids from colliding", async () => {
  const paperA = document("paper-a");
  const paperB = document("paper-b");
  const compareA = document("paper-a");
  compareA.project.nodes[0].title = "Changed only in paper A";
  const result = await computeCanvasDiffBatch({
    baseline: [paperA, paperB],
    comparison: [compareA, paperB],
  });

  assert.equal(result.summary.nodes.modified, 1);
  assert.equal(result.documents.length, 2);
  const changed = result.documents.find((item) => item.documentId === "paper-a");
  const untouched = result.documents.find((item) => item.documentId === "paper-b");
  assert.equal(changed?.summary.changed, 1);
  assert.equal(untouched?.summary.changed, 0);
  assert.notEqual(
    createCanvasDiffEntityRef("paper-a", "node", "question-root").entityKey,
    createCanvasDiffEntityRef("paper-b", "node", "question-root").entityKey,
  );
});

test("two document groups report added and removed documents and entities", async () => {
  const baseline = document("baseline-paper");
  const comparison = document("new-paper");
  comparison.project.nodes = comparison.project.nodes.slice(0, 2);
  const result = await computeCanvasDiffBatch({
    baseline: { groupId: "old-set", label: "Old set", documents: [baseline] },
    comparison: { groupId: "new-set", label: "New set", documents: [comparison] },
  });

  assert.deepEqual(result.baseline.documentIds, ["baseline-paper"]);
  assert.deepEqual(result.comparison.documentIds, ["new-paper"]);
  assert.equal(result.summary.documents.added, 1);
  assert.equal(result.summary.documents.removed, 1);
  assert.equal(result.summary.nodes.added, 2);
  assert.equal(result.summary.nodes.removed, baseline.project.nodes.length);
  assert.ok(result.documents.every((item) => item.provenance.documentId === item.documentId));
});

test("batch output is stable regardless of input ordering", async () => {
  const first = await computeCanvasDiffBatch({
    baseline: [document("paper-b"), document("paper-a")],
    comparison: [document("paper-b"), document("paper-a")],
  });
  const second = await computeCanvasDiffBatch({
    baseline: [document("paper-a"), document("paper-b")],
    comparison: [document("paper-a"), document("paper-b")],
  });
  assert.deepEqual(second, first);
});

test("PDF agent adapter preserves document provenance and review patch", () => {
  const patch: PluginGraphPatch = {
    apiVersion: "researchcanvas.dev/graph-patch/v1alpha1",
    source: { pluginId: "myc.pdf-canvas-agent", operation: "extract" },
    title: "PDF graph proposal",
    summary: "One proposed node",
    reviewRequired: true,
    operations: [
      { op: "add-node", node: { id: "node-1", type: "concept", title: "Concept" } },
    ],
  };
  const adapted = adaptPdfAgentResultForDiff(patch, {
    documentId: "kimi-paper",
    provenance: { origin: "ai", modelId: "kimi-k2.6", fileName: "paper.pdf" },
  });
  assert.equal(adapted?.documentId, "kimi-paper");
  assert.equal(adapted?.provenance.documentId, "kimi-paper");
  assert.equal(adapted?.provenance.modelId, "kimi-k2.6");
  assert.equal(adapted?.graphPatch?.operations.length, 1);
  assert.equal(adapted?.project, null);
});
