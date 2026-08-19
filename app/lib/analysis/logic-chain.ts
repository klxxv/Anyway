import type { LogicChainMode, LogicChainResult, ProjectState } from "../research-types";
import { shortestPath } from "../graph";

/**
 * 对选中的逻辑链评分，分数用于审阅提示而非科学结论。
 * Scores the selected logic chain for review guidance, not scientific conclusions.
 */
export function computeLogicChain(
  project: ProjectState,
  mode: LogicChainMode,
  targetId?: string,
): LogicChainResult {
  const completed = project.edges.filter((edge) => edge.experiment?.status === "completed");
  const chosen =
    mode === "refutation"
      ? project.edges.filter(
          (edge) => edge.polarity === "negative" || edge.experiment?.outcome === "refutes",
        )
      : mode === "effective"
        ? completed.filter(
            (edge) =>
              edge.experiment?.outcome === "supports" &&
              Math.abs(edge.experiment?.delta ?? 0) >= 0.005,
          )
        : project.edges.filter(
            (edge) => edge.type === "T" || edge.experiment?.outcome === "supports",
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
