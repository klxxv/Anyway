import type { ProjectState, ScenarioDiff } from "../research-types";
import { traverseGraph } from "./traversal";

/**
 * 对比基础图与消融场景，区分直接禁用与意外失去可达性。
 * Compares base graph and ablation, separating disabled items from lost reachability.
 */
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
