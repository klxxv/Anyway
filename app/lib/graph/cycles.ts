import type { ProjectState, ResearchEdge } from "../research-types";
import { resolveEdges, resolvedNodeIds } from "../project";

/** Finds directed cycles among nodes enabled by the selected scenario. */
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
