import type { ProjectState, TraversalRequest } from "../research-types";
import { resolveEdges } from "../project";
import { traverseGraph } from "./traversal";

/**
 * 用 BFS 返回一条稳定的最短路径；它沿用有向边的语义。
 * Returns one stable shortest path via BFS, preserving directed-edge semantics.
 */
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

/** Enumerates up to 100 equal-length paths to protect the UI on dense graphs. */
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
