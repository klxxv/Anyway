import type {
  InfluenceResult,
  LayoutMode,
  LayoutResult,
  LogicChainMode,
  LogicChainResult,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNodeType,
  ScenarioDiff,
  TraversalRequest,
  TraversalResult,
} from "./research-types";

export const cloneProject = (project: ProjectState): ProjectState =>
  JSON.parse(JSON.stringify(project)) as ProjectState;

export const CURRENT_SCHEMA_VERSION = 1;

export function migrateProject(input: unknown): ProjectState {
  if (!input || typeof input !== "object") {
    throw new Error("Project must be a JSON object.");
  }
  const candidate = input as Partial<ProjectState> & Record<string, unknown>;
  if (!Array.isArray(candidate.nodes) || !Array.isArray(candidate.edges)) {
    throw new Error("Project nodes and edges are required.");
  }
  const sourceVersion =
    typeof candidate.schemaVersion === "number" ? candidate.schemaVersion : 0;
  if (sourceVersion > CURRENT_SCHEMA_VERSION) {
    throw new Error(`Project schema ${sourceVersion} is newer than this application.`);
  }

  const now = new Date().toISOString();
  const legacyNodes = candidate.nodes as Array<
    ProjectState["nodes"][number] & { x?: number; y?: number; width?: number; height?: number }
  >;
  const placements =
    Array.isArray(candidate.placements) && candidate.placements.length
      ? candidate.placements
      : legacyNodes.map((node, index) => ({
          id: `placement-${node.id}`,
          viewId: "view-main",
          nodeId: node.id,
          x: node.x ?? 80 + (index % 4) * 270,
          y: node.y ?? 80 + Math.floor(index / 4) * 160,
          width: node.width ?? 230,
          height: node.height ?? 116,
        }));

  return {
    schemaVersion: CURRENT_SCHEMA_VERSION,
    id: typeof candidate.id === "string" ? candidate.id : makeId("project"),
    title:
      typeof candidate.title === "string" && candidate.title.trim()
        ? candidate.title
        : "Migrated research project",
    discipline:
      typeof candidate.discipline === "string" ? candidate.discipline : "General research",
    revision: typeof candidate.revision === "number" ? candidate.revision : 1,
    updatedAt: typeof candidate.updatedAt === "string" ? candidate.updatedAt : now,
    nodes: candidate.nodes,
    edges: candidate.edges,
    evidence: Array.isArray(candidate.evidence) ? candidate.evidence : [],
    placements,
    scenarios: Array.isArray(candidate.scenarios)
      ? candidate.scenarios.map((scenario) => ({
          ...scenario,
          disabledNodeIds: scenario.disabledNodeIds ?? [],
          disabledEdgeIds: scenario.disabledEdgeIds ?? [],
          nodeOverrides: scenario.nodeOverrides ?? {},
          edgeOverrides: scenario.edgeOverrides ?? {},
          parameters: scenario.parameters ?? {},
        }))
      : [],
    navigation:
      candidate.navigation &&
      typeof candidate.navigation === "object" &&
      Array.isArray((candidate.navigation as ProjectState["navigation"])?.recentNodeIds) &&
      Array.isArray((candidate.navigation as ProjectState["navigation"])?.pinnedNodeIds)
        ? (candidate.navigation as ProjectState["navigation"])
        : { recentNodeIds: [], pinnedNodeIds: [] },
    activity: Array.isArray(candidate.activity) ? candidate.activity : [],
  };
}

export const makeId = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

export function resolveEdges(project: ProjectState, scenarioId?: string) {
  const scenario = project.scenarios.find((item) => item.id === scenarioId);
  const disabledNodes = new Set(scenario?.disabledNodeIds ?? []);
  const disabledEdges = new Set(scenario?.disabledEdgeIds ?? []);

  return project.edges
    .filter(
      (edge) =>
        !disabledEdges.has(edge.id) &&
        !disabledNodes.has(edge.source) &&
        !disabledNodes.has(edge.target),
    )
    .map((edge) => ({ ...edge, ...(scenario?.edgeOverrides[edge.id] ?? {}) }))
    .sort((a, b) => a.id.localeCompare(b.id));
}

export function resolvedNodeIds(project: ProjectState, scenarioId?: string) {
  const scenario = project.scenarios.find((item) => item.id === scenarioId);
  const disabled = new Set(scenario?.disabledNodeIds ?? []);
  return new Set(project.nodes.filter((node) => !disabled.has(node.id)).map((node) => node.id));
}

type TraversalNeighbor = { nodeId: string; edgeId: string };

function buildNeighborIndex(
  edges: ResearchEdge[],
  direction: TraversalRequest["direction"],
  edgeTypes?: ResearchEdgeType[],
) {
  const allowedTypes = edgeTypes?.length ? new Set(edgeTypes) : null;
  const index = new Map<string, TraversalNeighbor[]>();
  const seen = new Map<string, Set<string>>();

  const add = (source: string, target: string, edgeId: string) => {
    const key = `${target}\u0000${edgeId}`;
    const sourceSeen = seen.get(source) ?? new Set<string>();
    if (sourceSeen.has(key)) return;
    sourceSeen.add(key);
    seen.set(source, sourceSeen);
    const neighbors = index.get(source) ?? [];
    neighbors.push({ nodeId: target, edgeId });
    index.set(source, neighbors);
  };

  for (const edge of edges) {
    if (allowedTypes && !allowedTypes.has(edge.type)) continue;
    if (direction === "out" || direction === "both") {
      add(edge.source, edge.target, edge.id);
    }
    if (direction === "in" || direction === "both") {
      add(edge.target, edge.source, edge.id);
    }
    if (!edge.directed) {
      if (direction === "out" || direction === "both") {
        add(edge.target, edge.source, edge.id);
      }
      if (direction === "in" || direction === "both") {
        add(edge.source, edge.target, edge.id);
      }
    }
  }

  for (const neighbors of index.values()) {
    neighbors.sort((a, b) =>
      a.nodeId === b.nodeId ? a.edgeId.localeCompare(b.edgeId) : a.nodeId.localeCompare(b.nodeId),
    );
  }

  return index;
}

function filteredActiveNodeIds(
  project: ProjectState,
  request: TraversalRequest,
): Set<string> {
  const resolved = resolvedNodeIds(project, request.scenarioId);
  const allowedTypes: Set<ResearchNodeType> | null = request.nodeTypes?.length
    ? new Set(request.nodeTypes)
    : null;
  if (!allowedTypes) return resolved;
  return new Set(
    project.nodes
      .filter(
        (node) =>
          resolved.has(node.id) &&
          (node.id === request.startId || allowedTypes.has(node.type)),
      )
      .map((node) => node.id),
  );
}

export function traverseGraph(project: ProjectState, request: TraversalRequest): TraversalResult {
  const start = performance.now();
  const edges = resolveEdges(project, request.scenarioId);
  const activeNodes = filteredActiveNodeIds(project, request);
  const neighborsByNode = buildNeighborIndex(edges, request.direction, request.edgeTypes);
  const result: TraversalResult = {
    strategy: request.strategy,
    startId: request.startId,
    order: [],
    edgeIds: [],
    depth: {},
    parent: {},
    treeEdgeIds: [],
    crossEdgeIds: [],
    backEdgeIds: [],
    stoppedByDepth: [],
    durationMs: 0,
  };

  if (!activeNodes.has(request.startId)) {
    result.durationMs = performance.now() - start;
    return result;
  }

  if (request.strategy === "bfs") {
    const visited = new Set<string>([request.startId]);
    const usedEdges = new Set<string>();
    const treeEdges = new Set<string>();
    const queue: string[] = [request.startId];
    let queueIndex = 0;
    result.depth[request.startId] = 0;
    result.parent[request.startId] = null;

    while (queueIndex < queue.length) {
      const current = queue[queueIndex++]!;
      result.order.push(current);
      const currentDepth = result.depth[current];
      const neighbors = (neighborsByNode.get(current) ?? []).filter((neighbor) =>
        activeNodes.has(neighbor.nodeId),
      );

      if (currentDepth >= request.maxDepth) {
        if (neighbors.some((item) => !visited.has(item.nodeId))) {
          result.stoppedByDepth.push(current);
        }
        continue;
      }

      for (const neighbor of neighbors) {
        if (!usedEdges.has(neighbor.edgeId)) {
          result.edgeIds.push(neighbor.edgeId);
          usedEdges.add(neighbor.edgeId);
        }
        if (!visited.has(neighbor.nodeId)) {
          visited.add(neighbor.nodeId);
          result.depth[neighbor.nodeId] = currentDepth + 1;
          result.parent[neighbor.nodeId] = current;
          result.treeEdgeIds.push(neighbor.edgeId);
          treeEdges.add(neighbor.edgeId);
          queue.push(neighbor.nodeId);
        } else if (!treeEdges.has(neighbor.edgeId)) {
          result.crossEdgeIds.push(neighbor.edgeId);
        }
      }
    }
  } else {
    const colors = new Map<string, 0 | 1 | 2>();
    const usedEdges = new Set<string>();
    const treeEdges = new Set<string>();
    result.parent[request.startId] = null;
    result.depth[request.startId] = 0;

    const visit = (current: string) => {
      colors.set(current, 1);
      result.order.push(current);
      const currentDepth = result.depth[current];
      const neighbors = (neighborsByNode.get(current) ?? []).filter((neighbor) =>
        activeNodes.has(neighbor.nodeId),
      );

      if (currentDepth >= request.maxDepth) {
        if (neighbors.some((item) => !colors.get(item.nodeId))) {
          result.stoppedByDepth.push(current);
        }
        colors.set(current, 2);
        return;
      }

      for (const neighbor of neighbors) {
        if (!usedEdges.has(neighbor.edgeId)) {
          result.edgeIds.push(neighbor.edgeId);
          usedEdges.add(neighbor.edgeId);
        }
        const color = colors.get(neighbor.nodeId) ?? 0;
        if (color === 0) {
          result.parent[neighbor.nodeId] = current;
          result.depth[neighbor.nodeId] = currentDepth + 1;
          result.treeEdgeIds.push(neighbor.edgeId);
          treeEdges.add(neighbor.edgeId);
          visit(neighbor.nodeId);
        } else if (color === 1) {
          result.backEdgeIds.push(neighbor.edgeId);
        } else if (!treeEdges.has(neighbor.edgeId)) {
          result.crossEdgeIds.push(neighbor.edgeId);
        }
      }
      colors.set(current, 2);
    };

    visit(request.startId);
  }

  result.crossEdgeIds = [...new Set(result.crossEdgeIds)];
  result.backEdgeIds = [...new Set(result.backEdgeIds)];
  result.durationMs = performance.now() - start;
  return result;
}

export function detectCycles(project: ProjectState, scenarioId?: string) {
  const edges = resolveEdges(project, scenarioId).filter((edge) => edge.directed);
  const nodeIds = [...resolvedNodeIds(project, scenarioId)].sort();
  const outgoing = new Map<string, ResearchEdge[]>();
  for (const edge of edges) {
    const list = outgoing.get(edge.source) ?? [];
    list.push(edge);
    outgoing.set(edge.source, list);
  }

  const color = new Map<string, 0 | 1 | 2>();
  const stack: string[] = [];
  const cycles: Array<{ nodeIds: string[]; edgeIds: string[] }> = [];

  const visit = (nodeId: string) => {
    color.set(nodeId, 1);
    stack.push(nodeId);
    for (const edge of (outgoing.get(nodeId) ?? []).sort((a, b) => a.id.localeCompare(b.id))) {
      const next = edge.target;
      if ((color.get(next) ?? 0) === 0) {
        visit(next);
      } else if (color.get(next) === 1) {
        const index = stack.lastIndexOf(next);
        const nodeCycle = [...stack.slice(index), next];
        const edgeCycle: string[] = [];
        for (let i = 0; i < nodeCycle.length - 1; i += 1) {
          const cycleEdge = edges.find(
            (candidate) =>
              candidate.source === nodeCycle[i] && candidate.target === nodeCycle[i + 1],
          );
          if (cycleEdge) edgeCycle.push(cycleEdge.id);
        }
        const key = [...new Set(nodeCycle)].sort().join("|");
        if (!cycles.some((cycle) => [...new Set(cycle.nodeIds)].sort().join("|") === key)) {
          cycles.push({ nodeIds: nodeCycle, edgeIds: edgeCycle });
        }
      }
    }
    stack.pop();
    color.set(nodeId, 2);
  };

  for (const nodeId of nodeIds) {
    if ((color.get(nodeId) ?? 0) === 0) visit(nodeId);
  }
  return cycles;
}

export function shortestPath(
  project: ProjectState,
  sourceId: string,
  targetId: string,
  scenarioId?: string,
) {
  const request: TraversalRequest = {
    startId: sourceId,
    strategy: "bfs",
    direction: "out",
    maxDepth: Number.MAX_SAFE_INTEGER,
    scenarioId,
  };
  const traversal = traverseGraph(project, request);
  if (!(targetId in traversal.parent)) return [];
  const path = [targetId];
  let cursor = targetId;
  while (traversal.parent[cursor]) {
    cursor = traversal.parent[cursor]!;
    path.unshift(cursor);
  }
  return path;
}

export function allShortestPaths(
  project: ProjectState,
  sourceId: string,
  targetId: string,
  scenarioId?: string,
) {
  if (sourceId === targetId) return [[sourceId]];
  const edges = resolveEdges(project, scenarioId);
  const outgoing = new Map<string, string[]>();
  for (const edge of edges) {
    const list = outgoing.get(edge.source) ?? [];
    list.push(edge.target);
    outgoing.set(edge.source, list);
    if (!edge.directed) {
      const reverse = outgoing.get(edge.target) ?? [];
      reverse.push(edge.source);
      outgoing.set(edge.target, reverse);
    }
  }
  for (const neighbors of outgoing.values()) {
    neighbors.sort((a, b) => a.localeCompare(b));
  }

  const distance = new Map<string, number>([[sourceId, 0]]);
  const parents = new Map<string, string[]>();
  const queue = [sourceId];
  let queueIndex = 0;
  while (queueIndex < queue.length) {
    const current = queue[queueIndex++]!;
    const nextDistance = distance.get(current)! + 1;
    for (const next of outgoing.get(current) ?? []) {
      if (!distance.has(next)) {
        distance.set(next, nextDistance);
        parents.set(next, [current]);
        queue.push(next);
      } else if (distance.get(next) === nextDistance) {
        const values = parents.get(next) ?? [];
        if (!values.includes(current)) values.push(current);
        values.sort((a, b) => a.localeCompare(b));
        parents.set(next, values);
      }
    }
  }
  if (!distance.has(targetId)) return [];

  const paths: string[][] = [];
  const build = (nodeId: string, suffix: string[]) => {
    if (nodeId === sourceId) {
      paths.push([sourceId, ...suffix]);
      return;
    }
    for (const parent of parents.get(nodeId) ?? []) {
      build(parent, [nodeId, ...suffix]);
      if (paths.length >= 100) return;
    }
  };
  build(targetId, []);
  return paths;
}

export function evidenceBacklinks(project: ProjectState, evidenceId: string) {
  return {
    nodeIds: project.nodes
      .filter((node) => node.evidenceIds.includes(evidenceId))
      .map((node) => node.id)
      .sort(),
    edgeIds: project.edges
      .filter((edge) => edge.evidenceIds.includes(evidenceId))
      .map((edge) => edge.id)
      .sort(),
  };
}

export function compareScenarioReachability(
  project: ProjectState,
  rootId: string,
  scenarioId: string,
): ScenarioDiff {
  const scenario = project.scenarios.find((item) => item.id === scenarioId);
  const base = traverseGraph(project, {
    startId: rootId,
    strategy: "bfs",
    direction: "out",
    maxDepth: Number.MAX_SAFE_INTEGER,
  });
  const ablated = traverseGraph(project, {
    startId: rootId,
    strategy: "bfs",
    direction: "out",
    maxDepth: Number.MAX_SAFE_INTEGER,
    scenarioId,
  });
  const baseSet = new Set(base.order);
  const ablatedSet = new Set(ablated.order);
  const disabled = new Set(scenario?.disabledNodeIds ?? []);
  const lost = [...baseSet].filter((id) => !ablatedSet.has(id) && !disabled.has(id));
  const retained = [...ablatedSet].filter((id) => baseSet.has(id));

  return {
    disabledNodeIds: scenario?.disabledNodeIds ?? [],
    disabledEdgeIds: scenario?.disabledEdgeIds ?? [],
    lostReachableNodeIds: lost,
    retainedReachableNodeIds: retained,
    alternatePathNodeIds: retained.filter((id) => base.parent[id] !== ablated.parent[id]),
  };
}

export function exportJsonCanvas(project: ProjectState) {
  const nodes = project.nodes.map((node) => {
    const placement = project.placements.find((item) => item.nodeId === node.id);
    return {
      id: node.id,
      type: "text",
      text: `# ${node.title}\n\n${node.body}\n\n**Type:** ${node.type}\n**Status:** ${node.status}`,
      x: Math.round(placement?.x ?? 0),
      y: Math.round(placement?.y ?? 0),
      width: Math.round(placement?.width ?? 230),
      height: Math.round(placement?.height ?? 110),
      color: node.type === "question" ? "4" : node.type === "result" ? "5" : undefined,
    };
  });
  const edges = project.edges.map((edge) => ({
    id: edge.id,
    fromNode: edge.source,
    toNode: edge.target,
    fromSide: "right",
    toSide: "left",
    label: edge.type,
  }));
  return { nodes, edges };
}

export function exportMarkdown(project: ProjectState) {
  const byId = new Map(project.nodes.map((node) => [node.id, node]));
  return [
    `# ${project.title}`,
    "",
    `> ${project.nodes.length} nodes · ${project.edges.length} relations · ${project.evidence.length} evidence records`,
    "",
    "## Research nodes",
    "",
    ...project.nodes.flatMap((node) => [
      `### ${node.title}`,
      "",
      `- Type: ${node.type}`,
      `- Status: ${node.status}`,
      `- Tags: ${node.tags.join(", ") || "—"}`,
      `- Evidence: ${node.evidenceIds.length}`,
      "",
      node.body,
      "",
    ]),
    "## Relations",
    "",
    ...project.edges.map(
      (edge) =>
        `- ${byId.get(edge.source)?.title ?? edge.source} — **${edge.type}** → ${byId.get(edge.target)?.title ?? edge.target}`,
    ),
  ].join("\n");
}

const csvCell = (value: unknown) =>
  `"${String(value ?? "")
    .replaceAll("\r\n", "\n")
    .replaceAll("\r", "\n")
    .replaceAll('"', '""')}"`;

export function exportCsv(project: ProjectState, kind: "nodes" | "edges") {
  if (kind === "nodes") {
    return [
      ["id", "type", "title", "status", "tags", "evidence_count", "body"].map(csvCell).join(","),
      ...project.nodes.map((node) =>
        [
          node.id,
          node.type,
          node.title,
          node.status,
          node.tags.join("|"),
          node.evidenceIds.length,
          node.body,
        ]
          .map(csvCell)
          .join(","),
      ),
    ].join("\n");
  }

  return [
    [
      "id",
      "source",
      "target",
      "type",
      "directed",
      "polarity",
      "confidence",
      "evidence_count",
      "experiment_id",
      "experiment_delta",
      "experiment_verdict",
    ]
      .map(csvCell)
      .join(","),
    ...project.edges.map((edge) =>
      [
        edge.id,
        edge.source,
        edge.target,
        edge.type,
        edge.directed,
        edge.polarity,
        edge.confidence,
        edge.evidenceIds.length,
        edge.experiment?.id,
        edge.experiment?.delta,
        edge.experiment?.outcome,
      ]
        .map(csvCell)
        .join(","),
    ),
  ].join("\n");
}

function topologicalDepths(project: ProjectState, edgeIds?: Set<string>) {
  const nodeIds = new Set(project.nodes.map((node) => node.id));
  const edges = project.edges.filter(
    (edge) => nodeIds.has(edge.source) && nodeIds.has(edge.target) && (!edgeIds || edgeIds.has(edge.id)),
  );
  const incoming = new Map<string, number>();
  const outgoing = new Map<string, ResearchEdge[]>();
  const depth = new Map<string, number>();

  for (const id of nodeIds) {
    incoming.set(id, 0);
    depth.set(id, 0);
  }
  for (const edge of edges) {
    incoming.set(edge.target, (incoming.get(edge.target) ?? 0) + 1);
    outgoing.set(edge.source, [...(outgoing.get(edge.source) ?? []), edge]);
  }

  const queue = [...nodeIds].filter((id) => (incoming.get(id) ?? 0) === 0).sort();
  let visited = 0;
  while (queue.length) {
    const current = queue.shift()!;
    visited += 1;
    for (const edge of outgoing.get(current) ?? []) {
      depth.set(edge.target, Math.max(depth.get(edge.target) ?? 0, (depth.get(current) ?? 0) + 1));
      incoming.set(edge.target, (incoming.get(edge.target) ?? 1) - 1);
      if (incoming.get(edge.target) === 0) queue.push(edge.target);
    }
    queue.sort();
  }

  // Feedback graphs are valid research objects. Place unresolved nodes in a
  // final layer instead of discarding them.
  if (visited < nodeIds.size) {
    const maxDepth = Math.max(0, ...depth.values());
    for (const [id, count] of incoming) {
      if (count > 0) depth.set(id, maxDepth + 1);
    }
  }
  return depth;
}

function chainEdgeIds(project: ProjectState, mode: "evidence" | "refutation") {
  const supportTypes = new Set<ResearchEdgeType>(["supports", "derived_from", "measures", "uses"]);
  return new Set(
    project.edges
      .filter((edge) => {
        if (mode === "refutation") {
          return edge.type === "contradicts" || edge.experiment?.outcome === "refutes";
        }
        return supportTypes.has(edge.type) || edge.experiment?.outcome === "supports";
      })
      .map((edge) => edge.id),
  );
}

function huffmanCodes(project: ProjectState) {
  type HuffmanItem = { weight: number; ids: string[]; codes: Record<string, string> };
  const queue: HuffmanItem[] = project.nodes.map((node) => ({
    weight:
      1 +
      node.evidenceIds.length * 2 +
      project.edges.filter((edge) => edge.source === node.id || edge.target === node.id).length,
    ids: [node.id],
    codes: { [node.id]: "" },
  }));

  if (queue.length === 1) return { [queue[0].ids[0]]: "0" };
  while (queue.length > 1) {
    queue.sort((a, b) => a.weight - b.weight || a.ids.join().localeCompare(b.ids.join()));
    const left = queue.shift()!;
    const right = queue.shift()!;
    const codes: Record<string, string> = {};
    for (const [id, code] of Object.entries(left.codes)) codes[id] = `0${code}`;
    for (const [id, code] of Object.entries(right.codes)) codes[id] = `1${code}`;
    queue.push({
      weight: left.weight + right.weight,
      ids: [...left.ids, ...right.ids],
      codes,
    });
  }
  return queue[0]?.codes ?? {};
}

export function computeLayout(
  project: ProjectState,
  mode: LayoutMode,
  rootId = project.nodes[0]?.id,
): LayoutResult {
  const positions: LayoutResult["positions"] = {};
  const annotations: LayoutResult["annotations"] = {};
  let nodeIds = project.nodes.map((node) => node.id);
  let edgeIds = project.edges.map((edge) => edge.id);

  if (mode === "tree") {
    const traversal = traverseGraph(project, {
      startId: rootId,
      strategy: "bfs",
      direction: "out",
      maxDepth: Number.MAX_SAFE_INTEGER,
    });
    const included = new Set(traversal.order);
    nodeIds = [...traversal.order, ...project.nodes.map((node) => node.id).filter((id) => !included.has(id))];
    edgeIds = traversal.treeEdgeIds;
    const rows = new Map<number, string[]>();
    for (const id of nodeIds) {
      const depth = traversal.depth[id] ?? Math.max(1, ...Object.values(traversal.depth)) + 1;
      rows.set(depth, [...(rows.get(depth) ?? []), id]);
    }
    for (const [depth, ids] of rows) {
      ids.forEach((id, index) => {
        positions[id] = { x: 80 + depth * 310, y: 80 + index * 164 };
      });
    }
  } else if (mode === "table") {
    const columns = [...new Set(project.nodes.map((node) => node.type))];
    columns.forEach((type, column) => {
      project.nodes
        .filter((node) => node.type === type)
        .forEach((node, row) => {
          positions[node.id] = { x: 70 + column * 270, y: 105 + row * 154 };
          annotations[node.id] = `${type} · row ${row + 1}`;
        });
    });
  } else if (mode === "huffman") {
    const codes = huffmanCodes(project);
    const ordered = project.nodes
      .map((node) => ({ id: node.id, code: codes[node.id] ?? "" }))
      .sort((a, b) => a.code.localeCompare(b.code));
    const rowsByDepth = new Map<number, number>();
    for (const item of ordered) {
      const depth = item.code.length;
      const row = rowsByDepth.get(depth) ?? 0;
      positions[item.id] = { x: 70 + depth * 280, y: 80 + row * 150 };
      annotations[item.id] = `prefix ${item.code || "0"}`;
      rowsByDepth.set(depth, row + 1);
    }
  } else {
    const selectedEdges =
      mode === "evidence-chain"
        ? chainEdgeIds(project, "evidence")
        : mode === "refutation-chain"
          ? chainEdgeIds(project, "refutation")
          : new Set(project.edges.map((edge) => edge.id));
    edgeIds = [...selectedEdges];
    if (mode !== "neural-network") {
      const linkedNodes = new Set<string>();
      project.edges.forEach((edge) => {
        if (selectedEdges.has(edge.id)) {
          linkedNodes.add(edge.source);
          linkedNodes.add(edge.target);
        }
      });
      nodeIds = [...linkedNodes];
    }
    const depth = topologicalDepths(project, selectedEdges);
    const rows = new Map<number, string[]>();
    for (const id of nodeIds) {
      const layer = depth.get(id) ?? 0;
      rows.set(layer, [...(rows.get(layer) ?? []), id]);
    }
    for (const [layer, ids] of rows) {
      ids.forEach((id, index) => {
        positions[id] = { x: 75 + layer * 300, y: 85 + index * 160 };
        if (mode === "neural-network") annotations[id] = `layer ${layer}`;
      });
    }
  }

  return { mode, positions, annotations, nodeIds, edgeIds };
}

export function computeLogicChain(
  project: ProjectState,
  mode: LogicChainMode,
  targetId?: string,
): LogicChainResult {
  const completed = project.edges.filter((edge) => edge.experiment?.status === "completed");
  const chosen =
    mode === "refutation"
      ? project.edges.filter(
          (edge) => edge.type === "contradicts" || edge.experiment?.outcome === "refutes",
        )
      : mode === "effective"
        ? completed.filter(
            (edge) =>
              edge.experiment?.outcome === "supports" &&
              Math.abs(edge.experiment?.delta ?? 0) >= 0.005,
          )
        : project.edges.filter(
            (edge) =>
              edge.type === "supports" ||
              edge.type === "derived_from" ||
              edge.experiment?.outcome === "supports",
          );

  const targetFiltered = targetId
    ? chosen.filter((edge) => {
        const path = shortestPath(project, edge.target, targetId);
        return edge.target === targetId || path.length > 0;
      })
    : chosen;
  const edges = targetFiltered.length ? targetFiltered : chosen;
  const edgeIds = edges.map((edge) => edge.id);
  const nodeIds = [...new Set(edges.flatMap((edge) => [edge.source, edge.target]))];
  const experimentCount = new Set(
    edges.map((edge) => edge.experiment?.id).filter((id): id is string => Boolean(id)),
  ).size;
  const meanConfidence =
    edges.reduce((sum, edge) => sum + (edge.confidence ?? 0.5), 0) / Math.max(1, edges.length);

  const summary =
    mode === "effective"
      ? `${experimentCount} completed experiments changed the target metric by at least 0.5 percentage points.`
      : mode === "refutation"
        ? `${experimentCount || edges.length} experiments or sources challenge the current explanation.`
        : `${edges.length} supported relations form the currently reviewable evidence chain.`;
  return { mode, nodeIds, edgeIds, score: meanConfidence, summary };
}

function edgeWeight(edge: ResearchEdge) {
  if (edge.type === "controls") return 0;
  const experimental = Math.abs(edge.experiment?.delta ?? 0);
  if (experimental > 0) return Math.min(1, experimental * 8 + 0.08);
  return Math.max(0.05, Math.min(1, edge.confidence ?? 0.5));
}

function edgeSign(edge: ResearchEdge) {
  if (
    edge.type === "contradicts" ||
    edge.polarity === "negative" ||
    edge.experiment?.outcome === "refutes"
  ) {
    return -1;
  }
  return 1;
}

export function propagateInfluence(
  project: ProjectState,
  targetId: string,
  maxIterations = Math.max(2, project.nodes.length),
): InfluenceResult {
  const raw: Record<string, number> = Object.fromEntries(project.nodes.map((node) => [node.id, 0]));
  const edgeContributions: Record<string, number> = {};
  raw[targetId] = 1;
  let frontier: Record<string, number> = { [targetId]: 1 };
  let iterations = 0;

  for (; iterations < maxIterations; iterations += 1) {
    const next: Record<string, number> = {};
    for (const edge of project.edges) {
      const downstream = frontier[edge.target];
      if (typeof downstream !== "number" || Math.abs(downstream) < 0.001) continue;
      const weight = edgeWeight(edge);
      if (weight === 0) continue;
      const contribution = downstream * weight * edgeSign(edge);
      next[edge.source] = (next[edge.source] ?? 0) + contribution;
      raw[edge.source] = (raw[edge.source] ?? 0) + contribution;
      edgeContributions[edge.id] = (edgeContributions[edge.id] ?? 0) + contribution;
    }
    frontier = next;
    if (!Object.keys(frontier).length) break;
  }

  const maxAbs = Math.max(1, ...Object.values(raw).map((value) => Math.abs(value)));
  const scores = Object.fromEntries(
    Object.entries(raw).map(([id, value]) => [id, value / maxAbs]),
  );
  const strongestEdgeIds = Object.entries(edgeContributions)
    .sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]))
    .slice(0, 8)
    .map(([id]) => id);

  return { targetId, scores, edgeContributions, strongestEdgeIds, iterations };
}
