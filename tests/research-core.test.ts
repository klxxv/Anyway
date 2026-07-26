import assert from "node:assert/strict";
import test from "node:test";
import {
  cloneProject,
  allShortestPaths,
  compareScenarioReachability,
  computeLayout,
  computeLogicChain,
  detectCycles,
  evidenceBacklinks,
  exportCsv,
  exportJsonCanvas,
  exportMarkdown,
  migrateProject,
  resolveEdges,
  propagateInfluence,
  shortestPath,
  traverseGraph,
} from "../app/lib/research-core";
import { initialProject } from "../app/lib/fixtures";
import { createMnistProject } from "../app/lib/mnist-fixture";
import { createSocialScienceProject } from "../app/lib/social-fixture";
import type { ProjectState } from "../app/lib/research-types";

test("BFS is deterministic and returns depth, parent, and tree-edge evidence", () => {
  const result = traverseGraph(initialProject, {
    startId: "q1",
    strategy: "bfs",
    direction: "out",
    maxDepth: 4,
  });

  assert.deepEqual(result.order.slice(0, 3), ["q1", "h1", "m1"]);
  assert.equal(result.depth.q1, 0);
  assert.equal(result.parent.h1, "q1");
  assert.ok(result.treeEdgeIds.includes("e-q-h1"));
  assert.ok(result.order.includes("r1"));
  assert.equal(new Set(result.order).size, result.order.length);
});

test("DFS distinguishes tree and back edges when a directed cycle exists", () => {
  const project = cloneProject(initialProject);
  project.edges.push({
    id: "cycle-r1-q1",
    source: "r1",
    target: "q1",
    type: "causes",
    directed: true,
    polarity: "positive",
    confidence: 0.5,
    conditions: [],
    evidenceIds: [],
    provenance: { origin: "human" },
  });

  const result = traverseGraph(project, {
    startId: "q1",
    strategy: "dfs",
    direction: "out",
    maxDepth: 12,
  });
  const cycles = detectCycles(project);

  assert.ok(result.backEdgeIds.includes("cycle-r1-q1"));
  assert.ok(cycles.length >= 1);
  assert.ok(cycles.every((cycle) => cycle.nodeIds.includes("q1")));
  assert.ok(cycles.some((cycle) => cycle.edgeIds.includes("cycle-r1-q1")));
});

test("scenario overlay filters nodes and edges without mutating the base graph", () => {
  const before = JSON.stringify(initialProject);
  const baseEdges = resolveEdges(initialProject);
  const scenarioEdges = resolveEdges(initialProject, "scenario-no-rope");
  const diff = compareScenarioReachability(initialProject, "q1", "scenario-no-rope");

  assert.ok(baseEdges.some((edge) => edge.id === "e-m1-m2"));
  assert.ok(!scenarioEdges.some((edge) => edge.id === "e-m1-m2"));
  assert.deepEqual(diff.disabledNodeIds, ["m2"]);
  assert.ok(diff.lostReachableNodeIds.includes("m3"));
  assert.equal(JSON.stringify(initialProject), before);
});

test("shortest path follows semantic direction and returns stable IDs", () => {
  assert.deepEqual(shortestPath(initialProject, "q1", "r1"), ["q1", "h1", "x1", "r1"]);
  assert.deepEqual(shortestPath(initialProject, "r1", "q1"), []);
});

test("all equally short paths and evidence backlinks remain deterministic", () => {
  const project = cloneProject(initialProject);
  project.edges.push({
    id: "parallel-q1-r1",
    source: "q1",
    target: "r1",
    type: "supports",
    directed: true,
    polarity: "positive",
    confidence: 0.8,
    conditions: [],
    evidenceIds: ["ev1"],
    provenance: { origin: "human" },
  });
  const paths = allShortestPaths(project, "q1", "r1");
  const backlinks = evidenceBacklinks(project, "ev1");

  assert.deepEqual(paths, [["q1", "r1"]]);
  assert.ok(backlinks.nodeIds.includes("q1"));
  assert.ok(backlinks.edgeIds.includes("parallel-q1-r1"));
  assert.deepEqual(evidenceBacklinks(project, "missing"), { nodeIds: [], edgeIds: [] });
});

test("Obsidian JSON Canvas and Markdown exports preserve semantic references", () => {
  const canvas = exportJsonCanvas(initialProject);
  const markdown = exportMarkdown(initialProject);
  const nodeCsv = exportCsv(initialProject, "nodes");
  const edgeCsv = exportCsv(initialProject, "edges");

  assert.equal(canvas.nodes.length, initialProject.nodes.length);
  assert.equal(canvas.edges.length, initialProject.edges.length);
  assert.equal(canvas.nodes.find((node) => node.id === "m2")?.x, 330);
  assert.equal(canvas.edges.find((edge) => edge.id === "e-m2-m3")?.label, "depends_on");
  assert.match(markdown, /# Long-context Transformer ablation/);
  assert.match(markdown, /Rotary positional encoding/);
  assert.match(markdown, /depends_on/);
  assert.match(nodeCsv, /"id","type","title"/);
  assert.match(edgeCsv, /"source","target","type"/);
});

test("all six view projections produce stable positions without mutating semantics", () => {
  const project = createMnistProject();
  const before = JSON.stringify(project.nodes);
  for (const mode of [
    "evidence-chain",
    "refutation-chain",
    "tree",
    "huffman",
    "table",
    "neural-network",
  ] as const) {
    const layout = computeLayout(project, mode, "mnist-question");
    assert.ok(Object.keys(layout.positions).length > 0, `${mode} should place nodes`);
    assert.equal(layout.mode, mode);
  }
  assert.equal(JSON.stringify(project.nodes), before);
});

test("MNIST experiment edges expose effective and refutation chains", () => {
  const project = createMnistProject();
  const effective = computeLogicChain(project, "effective", "mnist-conclusion");
  const refutation = computeLogicChain(project, "refutation");

  assert.ok(effective.edgeIds.includes("mnist-exp-no-normalization"));
  assert.ok(effective.edgeIds.includes("mnist-exp-width"));
  assert.ok(refutation.edgeIds.includes("mnist-exp-tanh"));
  assert.ok(
    project.edges.every((edge) => edge.experiment?.status === "completed"),
    "MNIST relations should retain completed experiment provenance",
  );
});

test("BP-like propagation ranks normalization above the smaller width ablation", () => {
  const project = createMnistProject();
  const influence = propagateInfluence(project, "mnist-accuracy");

  assert.equal(influence.targetId, "mnist-accuracy");
  assert.ok(
    Math.abs(influence.scores["mnist-normalization"]) >
      Math.abs(influence.scores["mnist-hidden"]),
  );
  assert.ok(influence.strongestEdgeIds.length > 0);
});

test("social-science acceptance fixture exposes mediation, filtering, cycles, and overlays", () => {
  const project = createSocialScienceProject();
  const upstream = traverseGraph(project, {
    startId: "soc-polarization",
    strategy: "bfs",
    direction: "in",
    maxDepth: 8,
    edgeTypes: ["causes", "mediates"],
  });
  const cycles = detectCycles(project);
  const diff = compareScenarioReachability(project, "soc-q", "soc-without-homophily");

  assert.equal(project.nodes.length, 20);
  assert.equal(project.evidence.length, 8);
  assert.equal(project.scenarios.length, 2);
  assert.ok(upstream.order.includes("soc-exposure"));
  assert.ok(upstream.order.includes("soc-homophily"));
  assert.ok(cycles.some((cycle) => cycle.nodeIds.includes("soc-trust")));
  assert.ok(diff.disabledNodeIds.includes("soc-homophily"));
  assert.equal(project.scenarios[1]?.nodeOverrides["soc-polarization"]?.data?.operationalization, "cross-group network distance");
});

test("indexed BFS handles the 5,000-node / 10,000-edge MVP target", () => {
  const now = "2026-07-26T00:00:00.000Z";
  const nodeCount = 5_000;
  const project: ProjectState = {
    schemaVersion: 1,
    id: "performance-fixture",
    title: "Traversal performance fixture",
    discipline: "Graph algorithms",
    revision: 1,
    updatedAt: now,
    nodes: Array.from({ length: nodeCount }, (_, index) => ({
      id: `n-${index}`,
      type: index === 0 ? ("question" as const) : ("variable" as const),
      title: `Node ${index}`,
      body: "",
      tags: [],
      status: "confirmed" as const,
      evidenceIds: [],
      data: {},
      provenance: { origin: "human" as const },
      createdAt: now,
      updatedAt: now,
    })),
    edges: Array.from({ length: nodeCount * 2 }, (_, index) => {
      const lane = index % 2;
      const source = Math.floor(index / 2);
      const target = lane === 0 ? source + 1 : Math.min(nodeCount - 1, source + 2);
      return {
        id: `e-${index}`,
        source: `n-${source}`,
        target: `n-${target}`,
        type: "depends_on" as const,
        directed: true,
        polarity: "unknown" as const,
        confidence: 1,
        conditions: [],
        evidenceIds: [],
        provenance: { origin: "human" as const },
      };
    }),
    evidence: [],
    placements: [],
    scenarios: [],
    activity: [],
  };

  const result = traverseGraph(project, {
    startId: "n-0",
    strategy: "bfs",
    direction: "out",
    maxDepth: nodeCount,
  });

  assert.equal(result.order.length, nodeCount);
  assert.ok(
    result.durationMs < 200,
    `indexed traversal took ${result.durationMs.toFixed(2)} ms`,
  );
});

test("legacy schema migrates node geometry into separate placements", () => {
  const legacy = {
    id: "legacy-project",
    title: "Legacy graph",
    nodes: [
      {
        ...initialProject.nodes[0],
        x: 123,
        y: 456,
      },
    ],
    edges: [],
  };

  const migrated = migrateProject(legacy);
  assert.equal(migrated.schemaVersion, 1);
  assert.equal(migrated.placements[0]?.nodeId, legacy.nodes[0].id);
  assert.equal(migrated.placements[0]?.x, 123);
  assert.equal(migrated.placements[0]?.y, 456);
});
