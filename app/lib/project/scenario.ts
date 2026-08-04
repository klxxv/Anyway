import type { ProjectState } from "../research-types";

/** Applies a scenario's non-destructive exclusions and edge overrides. */
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

/** 返回场景启用的节点集合 / Returns the node set enabled by a scenario. */
export function resolvedNodeIds(project: ProjectState, scenarioId?: string) {
  const scenario = project.scenarios.find((item) => item.id === scenarioId);
  const disabled = new Set(scenario?.disabledNodeIds ?? []);
  return new Set(project.nodes.filter((node) => !disabled.has(node.id)).map((node) => node.id));
}
