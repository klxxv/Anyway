import assert from "node:assert/strict";
import test from "node:test";
import type { ProjectState, ResearchNodeType } from "../app/lib/research-types";
import { computeLayout } from "../app/lib/layout";
import type { PluginGraphPatch } from "../app/plugins/contracts";
import { zenWorkspaceFixture } from "../app/features/research-workspace/workspace-fixture";
import { projectForLegendFilter } from "../app/features/research-workspace/workspace-layout";
import {
  HISTORY_LIMIT,
  applyLayoutInDraft,
  cloneProject,
  createEdgeInDraft,
  createNodeInDraft,
  createSelectionClipboard,
  duplicateNodeInDraft,
  moveNodeInDraft,
  moveNodesInDraft,
  pasteSelectionClipboardInDraft,
  pushHistoryEntry,
  redoHistory,
  removeEdgeInDraft,
  removeNodeInDraft,
  removeSelectionInDraft,
  reverseEdgeInDraft,
  stampDraftRevision,
  undoHistory,
  updateNodeInDraft,
} from "../app/features/research-workspace/hooks/commit-logic";
import {
  applyGraphPatchToDraft,
  sanitizePluginActorId,
  sortGraphPatchOperations,
} from "../app/features/research-workspace/hooks/patch-apply";
import {
  PROJECT_STORAGE_KEY,
  createLocalStorageProjectStorage,
  hydrateFromStorage,
  parseStoredProject,
  resolveHydratedProject,
  type ProjectStorage,
} from "../app/features/research-workspace/hooks/sync-logic";

const now = "2026-08-05T00:00:00.000Z";

/** In-memory Storage-compatible backend for tests. */
function memoryStorage(initial: Record<string, string> = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key: string) => map.get(key) ?? null,
    setItem: (key: string, value: string) => void map.set(key, value),
    removeItem: (key: string) => void map.delete(key),
  };
}

function cloneFixture(): ProjectState {
  return cloneProject(zenWorkspaceFixture);
}

// ---------------------------------------------------------------------------
// commit-logic: history stack
// ---------------------------------------------------------------------------

test("history stack caps at HISTORY_LIMIT entries", () => {
  let past: Array<{ project: ProjectState; label: string }> = [];
  for (let index = 0; index < HISTORY_LIMIT + 10; index += 1) {
    past = pushHistoryEntry(past, { project: cloneFixture(), label: `step-${index}` });
  }
  assert.equal(past.length, HISTORY_LIMIT);
  assert.equal(past[0].label, "step-10");
  assert.equal(past.at(-1)?.label, `step-${HISTORY_LIMIT + 9}`);
});

test("undo on an empty past stack returns null", () => {
  assert.equal(undoHistory([], [], cloneFixture()), null);
});

test("redo on an empty future stack returns null", () => {
  assert.equal(redoHistory([], [], cloneFixture()), null);
});

test("undo and redo round-trip the project snapshot", () => {
  const original = cloneFixture();
  const edited = cloneFixture();
  edited.nodes[0]!.title = "edited title";

  const past = pushHistoryEntry([], { project: original, label: "Edit" });
  const undone = undoHistory(past, [], edited);
  assert.ok(undone);
  assert.equal(undone.project.nodes[0]!.title, original.nodes[0]!.title);
  assert.equal(undone.past.length, 0);
  assert.equal(undone.future.length, 1);
  assert.equal(undone.future[0]!.label, "Edit");
  // The undone snapshot must equal the pre-edit project, not share its identity.
  assert.notEqual(undone.project, original);
  assert.deepEqual(undone.project, original);

  const redone = redoHistory(undone.past, undone.future, undone.project);
  assert.ok(redone);
  assert.equal(redone.project.nodes[0]!.title, "edited title");
  assert.equal(redone.past.length, 1);
  assert.equal(redone.future.length, 0);
});

test("a new commit after undo clears the future stack", () => {
  const original = cloneFixture();
  const first = cloneFixture();
  const second = cloneFixture();
  const past = pushHistoryEntry([], { project: original, label: "First" });
  const undone = undoHistory(past, [], first);
  assert.ok(undone);
  const branch = pushHistoryEntry(undone.past, { project: second, label: "Branch" });
  assert.equal(branch.length, 1);
  assert.equal(branch[0]!.label, "Branch");
});

test("stampDraftRevision bumps revision and updatedAt", () => {
  const draft = cloneFixture();
  const before = draft.revision;
  stampDraftRevision(draft, now);
  assert.equal(draft.revision, before + 1);
  assert.equal(draft.updatedAt, now);
});

// ---------------------------------------------------------------------------
// commit-logic: draft transforms
// ---------------------------------------------------------------------------

test("createNodeInDraft adds a node with type-specific placement geometry", () => {
  const draft = cloneFixture();
  createNodeInDraft(
    draft,
    "node-new",
    {
      type: "question",
      title: "  新问题  ",
      body: "   ",
      tags: ["标签"],
      data: {},
    },
    12,
    34,
    now,
  );
  const node = draft.nodes.find((item) => item.id === "node-new");
  assert.ok(node);
  assert.equal(node.title, "新问题");
  assert.equal(node.body, "Add a concise research note.");
  assert.equal(node.status, "draft");
  assert.equal(node.createdAt, now);
  const placement = draft.placements.find((item) => item.nodeId === "node-new");
  assert.ok(placement);
  assert.deepEqual(
    { x: placement.x, y: placement.y, width: placement.width, height: placement.height },
    { x: 12, y: 34, width: 136, height: 136 },
  );

  createNodeInDraft(
    draft,
    "node-note",
    { type: "note", title: "笔记", body: "内容", tags: [], data: {} },
    1,
    2,
    now,
  );
  const notePlacement = draft.placements.find((item) => item.nodeId === "node-note");
  assert.deepEqual(
    { width: notePlacement?.width, height: notePlacement?.height },
    { width: 164, height: 116 },
  );
});

test("updateNodeInDraft merges fields and stamps updatedAt; unknown ids are no-ops", () => {
  const draft = cloneFixture();
  const target = draft.nodes[0]!;
  updateNodeInDraft(draft, target.id, { title: "新标题", status: "confirmed" }, now);
  assert.equal(target.title, "新标题");
  assert.equal(target.status, "confirmed");
  assert.equal(target.updatedAt, now);

  const count = draft.nodes.length;
  updateNodeInDraft(draft, "missing-node", { title: "x" }, now);
  assert.equal(draft.nodes.length, count);
});

test("moveNodeInDraft repositions only the matching placement", () => {
  const draft = cloneFixture();
  const placement = draft.placements[0]!;
  moveNodeInDraft(draft, placement.nodeId, 555, 666);
  assert.equal(placement.x, 555);
  assert.equal(placement.y, 666);
  moveNodeInDraft(draft, "missing-node", 1, 1);
  assert.equal(draft.placements.length, cloneFixture().placements.length);
});

test("moveNodesInDraft repositions a group atomically", () => {
  const draft = cloneFixture();
  const first = draft.placements[0]!;
  const second = draft.placements[1]!;

  moveNodesInDraft(draft, [
    { nodeId: first.nodeId, x: 321, y: 654 },
    { nodeId: second.nodeId, x: 987, y: 123 },
  ]);

  assert.deepEqual(
    draft.placements.find((item) => item.nodeId === first.nodeId),
    { ...first, x: 321, y: 654 },
  );
  assert.deepEqual(
    draft.placements.find((item) => item.nodeId === second.nodeId),
    { ...second, x: 987, y: 123 },
  );
});

test("createEdgeInDraft pushes a directed edge with polarity derived from type", () => {
  const draft = cloneFixture();
  createEdgeInDraft(draft, "edge-new", "variable-canopy", "paper-landsat", "contradicts");
  const edge = draft.edges.find((item) => item.id === "edge-new");
  assert.ok(edge);
  assert.equal(edge.directed, true);
  assert.equal(edge.polarity, "negative");
  assert.equal(edge.confidence, 1);
  assert.deepEqual(edge.provenance, { origin: "human", actorId: "local-researcher" });

  createEdgeInDraft(draft, "edge-positive", "question-tree", "result-canopy", "supports");
  assert.equal(
    draft.edges.find((item) => item.id === "edge-positive")?.polarity,
    "positive",
  );
});

test("removeNodeInDraft cascades incident edges and placements", () => {
  const draft = cloneFixture();
  const beforeEdges = draft.edges.length;
  removeNodeInDraft(draft, "variable-canopy");
  assert.equal(draft.nodes.some((node) => node.id === "variable-canopy"), false);
  assert.equal(
    draft.edges.some(
      (edge) => edge.source === "variable-canopy" || edge.target === "variable-canopy",
    ),
    false,
  );
  assert.equal(draft.edges.length, beforeEdges - 5);
  assert.equal(draft.placements.some((p) => p.nodeId === "variable-canopy"), false);
});

test("selection clipboard remaps copied graph fragments and offsets their placements", () => {
  const draft = cloneFixture();
  const sourceEdge = draft.edges[0]!;
  const clipboard = createSelectionClipboard(draft, [], [sourceEdge.id]);
  assert.ok(clipboard);
  assert.equal(clipboard.nodes.length, 2);
  assert.ok(clipboard.edges.some((edge) => edge.id === sourceEdge.id));

  clipboard.nodes[0]!.title = "clipboard-only change";
  assert.notEqual(
    draft.nodes.find((node) => node.id === sourceEdge.source)?.title,
    "clipboard-only change",
  );

  const pasted = pasteSelectionClipboardInDraft(
    draft,
    clipboard,
    40,
    now,
  );
  assert.equal(pasted.nodeIds.length, 2);
  assert.ok(pasted.edgeIds.length >= 1);
  const pastedNodeIds = new Set(pasted.nodeIds);
  assert.ok(
    draft.edges
      .filter((edge) => pasted.edgeIds.includes(edge.id))
      .every((edge) => pastedNodeIds.has(edge.source) && pastedNodeIds.has(edge.target)),
  );
  const copiedNode = clipboard.nodes[0]!;
  const originalPlacement = clipboard.placements.find(
    (placement) => placement.nodeId === copiedNode.id,
  );
  const pastedNode = draft.nodes.find(
    (node) => node.title === `${copiedNode.title} copy`,
  );
  const pastedPlacement = draft.placements.find(
    (placement) => placement.nodeId === pastedNode?.id,
  );
  assert.ok(originalPlacement);
  assert.ok(pastedPlacement);
  assert.equal(pastedPlacement.x, originalPlacement.x + 40);
  assert.equal(pastedPlacement.y, originalPlacement.y + 40);
});

test("removeSelectionInDraft deletes selected relations and cascades selected nodes", () => {
  const draft = cloneFixture();
  const selectedNodeId = draft.edges[0]!.source;
  const explicitEdgeId = draft.edges.find(
    (edge) => edge.source !== selectedNodeId && edge.target !== selectedNodeId,
  )?.id;
  assert.ok(explicitEdgeId);

  removeSelectionInDraft(draft, [selectedNodeId], [explicitEdgeId]);

  assert.equal(draft.nodes.some((node) => node.id === selectedNodeId), false);
  assert.equal(draft.placements.some((placement) => placement.nodeId === selectedNodeId), false);
  assert.equal(
    draft.edges.some(
      (edge) =>
        edge.id === explicitEdgeId ||
        edge.source === selectedNodeId ||
        edge.target === selectedNodeId,
    ),
    false,
  );
});

test("duplicateNodeInDraft clones the node with an offset placement", () => {
  const draft = cloneFixture();
  const source = draft.nodes.find((node) => node.id === "method-ndvi")!;
  const placement = draft.placements.find((item) => item.nodeId === "method-ndvi")!;
  duplicateNodeInDraft(draft, "method-ndvi", "node-copy", now);
  const copy = draft.nodes.find((node) => node.id === "node-copy")!;
  assert.equal(copy.title, `${source.title} copy`);
  assert.deepEqual(copy.data, source.data);
  assert.deepEqual(copy.tags, source.tags);
  assert.notEqual(copy.id, source.id);
  const copyPlacement = draft.placements.find((item) => item.nodeId === "node-copy")!;
  assert.deepEqual(
    { x: copyPlacement.x, y: copyPlacement.y },
    { x: placement.x + 28, y: placement.y + 28 },
  );
});

test("removeEdgeInDraft and reverseEdgeInDraft mutate only the target edge", () => {
  const draft = cloneFixture();
  reverseEdgeInDraft(draft, "edge-canopy-temp");
  const reversed = draft.edges.find((edge) => edge.id === "edge-canopy-temp")!;
  assert.deepEqual(
    [reversed.source, reversed.target],
    ["variable-temperature", "variable-canopy"],
  );

  removeEdgeInDraft(draft, "edge-canopy-temp");
  assert.equal(draft.edges.some((edge) => edge.id === "edge-canopy-temp"), false);
});

test("applyLayoutInDraft writes every projected placement position", () => {
  const draft = cloneFixture();
  const before = cloneProject(draft);
  applyLayoutInDraft(draft, "table", "question-tree", null);

  const projected = projectForLegendFilter(draft, null);
  const expected = computeLayout(projected, "table", "question-tree");
  for (const placement of draft.placements) {
    const position = expected.positions[placement.nodeId];
    assert.ok(position, `placement for ${placement.nodeId} should be positioned`);
    assert.deepEqual(
      { x: placement.x, y: placement.y },
      { x: position.x, y: position.y },
    );
  }
  // Layout must not touch any other field of the draft.
  assert.equal(draft.revision, before.revision);
  assert.equal(draft.title, before.title);
  assert.deepEqual(draft.nodes, before.nodes);
  assert.deepEqual(draft.edges, before.edges);
});

test("applyLayoutInDraft falls back to the first projected node when root is absent", () => {
  const draft = cloneFixture();
  const projected = projectForLegendFilter(draft, "contradicts");
  const expected = computeLayout(projected, "table", projected.nodes[0]?.id);
  applyLayoutInDraft(draft, "table", "no-such-node", "contradicts");
  const filtered = draft.placements.filter((p) =>
    projected.nodes.some((node) => node.id === p.nodeId),
  );
  for (const placement of filtered) {
    const position = expected.positions[placement.nodeId];
    assert.deepEqual({ x: placement.x, y: placement.y }, { x: position.x, y: position.y });
  }
});

// ---------------------------------------------------------------------------
// patch-apply
// ---------------------------------------------------------------------------

function patch(operations: PluginGraphPatch["operations"]): PluginGraphPatch {
  return {
    apiVersion: "researchcanvas.dev/graph-patch/v1alpha1",
    source: { pluginId: "test-plugin", operation: "sync", externalId: "ext-1" },
    title: "测试补丁",
    summary: "summary",
    reviewRequired: true,
    operations,
  };
}

test("patch operations sort into stable dependency phases", () => {
  const operations: PluginGraphPatch["operations"] = [
    { op: "add-edge", edge: { id: "e", source: "a", target: "b", type: "causes" } },
    { op: "update-node", nodeId: "a", changes: { title: "t" } },
    { op: "add-node", node: { id: "a", type: "note", title: "A" } },
    { op: "update-edge", edgeId: "e", changes: { note: "n" } },
  ];
  assert.deepEqual(
    sortGraphPatchOperations(operations).map((operation) => operation.op),
    ["add-node", "update-node", "add-edge", "update-edge"],
  );
});

test("add-node and add-edge ops land on the draft with import provenance", () => {
  const draft = cloneFixture();
  applyGraphPatchToDraft(
    draft,
    patch([
      { op: "add-edge", edge: { id: "edge-new", source: "missing-a", target: "missing-b", type: "causes" } },
      { op: "add-node", node: { id: "node-new", type: "note", title: "新节点", tags: ["t"] } },
      { op: "add-edge", edge: { id: "edge-new-2", source: "node-new", target: "question-tree", type: "contradicts" } },
    ]),
    now,
  );
  const added = draft.nodes.find((node) => node.id === "node-new")!;
  assert.ok(added);
  assert.equal(added.title, "新节点");
  assert.deepEqual(added.provenance, {
    origin: "import",
    actorId: "test-plugin",
    sourceRefs: ["ext-1"],
  });
  // Edge referencing a missing endpoint is skipped.
  assert.equal(draft.edges.some((edge) => edge.id === "edge-new"), false);
  const addedEdge = draft.edges.find((edge) => edge.id === "edge-new-2")!;
  assert.ok(addedEdge);
  assert.equal(addedEdge.polarity, "negative");
  assert.equal(draft.placements.some((p) => p.nodeId === "node-new"), true);
});

test("patch application skips duplicate ids and unsupported types", () => {
  const draft = cloneFixture();
  applyGraphPatchToDraft(
    draft,
    patch([
      { op: "add-node", node: { id: "question-tree", type: "note", title: "重复" } },
      { op: "add-node", node: { id: "node-weird", type: "nonsense", title: "x" } },
      {
        op: "add-edge",
        edge: { id: "edge-weird", source: "question-tree", target: "result-canopy", type: "mystery" },
      },
    ]),
    now,
  );
  assert.equal(draft.nodes.some((node) => node.id === "node-weird"), false);
  assert.equal(draft.nodes.filter((node) => node.id === "question-tree").length, 1);
  assert.equal(draft.edges.some((edge) => edge.id === "edge-weird"), false);
});

test("update-node and update-edge ops respect type and range guards", () => {
  const draft = cloneFixture();
  const node = draft.nodes.find((item) => item.id === "variable-canopy")!;
  const edge = draft.edges.find((item) => item.id === "edge-canopy-temp")!;
  const originalBody = node.body;
  const originalTags = [...node.tags];
  const originalConfidence = edge.confidence;
  applyGraphPatchToDraft(
    draft,
    patch([
      {
        op: "update-node",
        nodeId: "variable-canopy",
        changes: {
          title: "更新标题",
          tags: "not-an-array",
          data: { extra: 1 },
          body: 42,
        },
      },
      {
        op: "update-edge",
        edgeId: "edge-canopy-temp",
        changes: { note: "新备注", type: "bogus", confidence: 5 },
      },
    ]),
    now,
  );
  assert.equal(node.title, "更新标题");
  assert.deepEqual(node.tags, originalTags); // non-array tags are ignored
  assert.equal(node.body, originalBody); // non-string body is ignored
  assert.deepEqual(node.data.extra, 1);
  assert.equal(node.updatedAt, now);
  assert.equal(edge.note, "新备注");
  assert.equal(edge.type, "causes"); // unsupported type ignored
  assert.equal(edge.confidence, originalConfidence); // out-of-range confidence ignored
});

test("patch application records one import activity entry", () => {
  const draft = cloneFixture();
  const result = applyGraphPatchToDraft(
    draft,
    patch([{ op: "add-node", node: { id: "n1", type: "note", title: "A" } }]),
    now,
  );
  assert.equal(result.applied, 1);
  assert.equal(result.skipped, 0);
  const activity = draft.activity.at(-1)!;
  assert.equal(activity.label, "测试补丁 · 1 applied · 0 skipped");
  assert.equal(activity.origin, "import");
  assert.equal(activity.createdAt, now);
  assert.ok(activity.id.startsWith("activity-"));
});

test("patch apply reports skipped operations and records accurate activity", () => {
  const draft = cloneFixture();
  const result = applyGraphPatchToDraft(
    draft,
    patch([
      { op: "add-node", node: { id: "n1", type: "note", title: "A" } },
      { op: "add-node", node: { id: "n1", type: "note", title: "Duplicate" } },
      { op: "add-edge", edge: { id: "e1", source: "n1", target: "missing", type: "causes" } },
    ]),
    now,
  );
  assert.equal(result.applied, 1);
  assert.equal(result.skipped, 2);
  assert.equal(result.byOp["add-node"]?.applied, 1);
  assert.equal(result.byOp["add-node"]?.skipped, 1);
  assert.equal(result.byOp["add-edge"]?.skipped, 1);
  const activity = draft.activity.at(-1)!;
  assert.equal(activity.label, "测试补丁 · 1 applied · 2 skipped");
});

test("patch apply with zero effective operations records skipped activity", () => {
  const draft = cloneFixture();
  const result = applyGraphPatchToDraft(
    draft,
    patch([
      { op: "add-node", node: { id: "question-tree", type: "note", title: "Duplicate" } },
      { op: "add-edge", edge: { id: "e1", source: "missing", target: "missing", type: "causes" } },
    ]),
    now,
  );
  assert.equal(result.applied, 0);
  assert.equal(result.skipped, 2);
  const activity = draft.activity.at(-1)!;
  assert.match(activity.label, /no effective operations/);
});

test("sanitizePluginActorId rejects unsafe plugin ids and keeps safe ones", () => {
  assert.equal(sanitizePluginActorId("myc.pdf-canvas-agent"), "myc.pdf-canvas-agent");
  assert.equal(sanitizePluginActorId("plugin@v1"), null);
  assert.equal(sanitizePluginActorId("../../evil"), null);
  assert.equal(sanitizePluginActorId("a".repeat(161)), null);
});

test("patch provenance actorId is sanitized to unknown-plugin when unsafe", () => {
  const draft = cloneFixture();
  applyGraphPatchToDraft(
    draft,
    {
      ...patch([{ op: "add-node", node: { id: "n2", type: "note", title: "B" } }]),
      source: { pluginId: "plugin@v1", operation: "bad" },
    },
    now,
  );
  const added = draft.nodes.find((node) => node.id === "n2")!;
  assert.equal(added.provenance.actorId, "unknown-plugin");
});

// ---------------------------------------------------------------------------
// sync-logic: storage backend
// ---------------------------------------------------------------------------

test("localStorage-backed storage round-trips save and load", () => {
  const storage = createLocalStorageProjectStorage("custom-key", memoryStorage());
  assert.equal(storage.load(), null);
  storage.save(cloneFixture());
  const loaded = storage.load();
  assert.deepEqual(loaded, cloneFixture());
  storage.clear();
  assert.equal(storage.load(), null);
});

test("localStorage-backed storage uses the default project key", () => {
  const backend = memoryStorage();
  const storage = createLocalStorageProjectStorage(undefined, backend);
  storage.save(cloneFixture());
  assert.ok(backend.getItem(PROJECT_STORAGE_KEY));
});

test("corrupt persisted JSON loads as null instead of throwing", () => {
  const storage = createLocalStorageProjectStorage("key", memoryStorage({ key: "{oops" }));
  assert.equal(storage.load(), null);
  assert.equal(parseStoredProject("not json"), null);
  // 仅含 schemaVersion 的 JSON 不满足结构校验，应返回 null。
  assert.equal(parseStoredProject('{"schemaVersion":2}'), null);
});

test("structurally invalid project is rejected during hydration", () => {
  const bad = {
    schemaVersion: 2,
    id: "bad",
    title: "Bad",
    discipline: "test",
    updatedAt: now,
    revision: 1,
    nodes: [{ id: "n1", type: "note" }], // missing required fields
    edges: [],
    evidence: [],
    placements: [],
    scenarios: [],
    activity: [],
  };
  const storage = createLocalStorageProjectStorage("bad-key", memoryStorage({ "bad-key": JSON.stringify(bad) }));
  assert.equal(storage.load(), null);
});

test("minimal structurally valid project hydrates successfully", () => {
  const minimal = {
    schemaVersion: 2,
    id: "minimal",
    title: "Minimal",
    discipline: "test",
    updatedAt: now,
    revision: 1,
    nodes: [
      {
        id: "n1",
        type: "note",
        title: "Note",
        body: "body",
        tags: [],
        status: "draft",
        evidenceIds: [],
        data: {},
        provenance: { origin: "human" },
        createdAt: now,
        updatedAt: now,
      },
    ],
    edges: [],
    evidence: [],
    placements: [
      {
        id: "p1",
        viewId: "view-main",
        nodeId: "n1",
        x: 0,
        y: 0,
        width: 100,
        height: 100,
      },
    ],
    scenarios: [],
    activity: [],
  };
  const storage = createLocalStorageProjectStorage("min-key", memoryStorage({ "min-key": JSON.stringify(minimal) }));
  const loaded = storage.load();
  assert.ok(loaded);
  assert.equal(loaded?.id, "minimal");
  assert.equal(loaded?.nodes[0]?.id, "n1");
});

test("resolveHydratedProject migrates only the bundled example under schemaVersion 2", () => {
  const fixture = cloneFixture();

  const legacy = cloneFixture();
  legacy.schemaVersion = 1;
  const migrated = resolveHydratedProject(legacy, fixture);
  assert.equal(migrated.schemaVersion, 2);
  assert.deepEqual(migrated, fixture);

  const unrelated = cloneFixture();
  unrelated.id = "user-project-42";
  unrelated.schemaVersion = 1;
  const kept = resolveHydratedProject(unrelated, fixture);
  assert.equal(kept, unrelated);
  assert.equal(kept.schemaVersion, 1);

  const current = cloneFixture();
  current.schemaVersion = 2;
  const keptCurrent = resolveHydratedProject(current, fixture);
  assert.equal(keptCurrent, current);
});

test("hydrateFromStorage resolves the stored project against the fixture", () => {
  const legacy = cloneFixture();
  legacy.schemaVersion = 1;
  const storage: ProjectStorage = {
    load: () => legacy,
    save: () => {},
    clear: () => {},
  };
  assert.deepEqual(hydrateFromStorage(storage, cloneFixture()), cloneFixture());
  const empty: ProjectStorage = { load: () => null, save: () => {}, clear: () => {} };
  assert.equal(hydrateFromStorage(empty, cloneFixture()), null);
});

test("cloneProject produces deep, independent copies", () => {
  const original = cloneFixture();
  const copy = cloneProject(original);
  copy.nodes[0]!.title = "mutated";
  copy.placements[0]!.x = 999;
  assert.notEqual(original.nodes[0]!.title, "mutated");
  assert.notEqual(original.placements[0]!.x, 999);
  assert.deepEqual(original, cloneFixture());
});

test("new node drafts keep type safety for every node type", () => {
  const draft = cloneFixture();
  const types: ResearchNodeType[] = [
    "question",
    "concept",
    "variable",
    "hypothesis",
    "method",
    "evidence",
    "paper",
    "dataset",
    "experiment",
    "result",
    "metric",
    "formula",
    "artifact",
    "note",
  ];
  for (const type of types) {
    const id = `node-${type}`;
    createNodeInDraft(draft, id, { type, title: type, body: "b", tags: [], data: {} }, 0, 0, now);
    assert.equal(draft.nodes.find((node) => node.id === id)?.type, type);
  }
});
