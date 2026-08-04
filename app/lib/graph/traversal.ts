import type {
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNodeType,
  TraversalRequest,
  TraversalResult,
} from "../research-types";
import { resolveEdges, resolvedNodeIds } from "../project";

type TraversalNeighbor = { nodeId: string; edgeId: string };

/** Builds a deterministic adjacency index respecting direction and type filters. */
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

/**
 * 将节点类型过滤限定在启用节点内，并始终保留遍历起点。
 * Applies node-type filters within enabled nodes while always retaining the start node.
 */
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

/**
 * Runs a bounded BFS or DFS and classifies encountered edges for visualisation.
 * Stable ordering keeps results and tests reproducible across runtimes.
 */
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
