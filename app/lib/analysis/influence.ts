import type { InfluenceResult, ProjectState, ResearchEdge } from "../research-types";

/** 将置信度规范到影响传播权重 / Normalizes confidence into an influence-propagation weight. */
function edgeWeight(edge: ResearchEdge) {
  if (edge.type === "controls") return 0;
  const experimental = Math.abs(edge.experiment?.delta ?? 0);
  if (experimental > 0) return Math.min(1, experimental * 8 + 0.08);
  return Math.max(0.05, Math.min(1, edge.confidence ?? 0.5));
}

/** 将关系极性转换为传播方向符号 / Converts relation polarity to a propagation sign. */
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

/**
 * 运行固定轮数的可解释影响传播；不是训练模型或因果识别器。
 * Runs fixed-round explainable influence propagation; it is not a trained model or causal estimator.
 */
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
