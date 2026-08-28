import type {
  LayoutMode,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
} from "../../lib/research-types";

export type LinkLegendFilter = ResearchEdgeType;

export const layoutOptions: Array<{
  mode: LayoutMode;
  label: string;
  description: string;
}> = [
  {
    mode: "evidence-chain",
    label: "Evidence chain",
    description: "Follow supported evidence from source to finding.",
  },
  {
    mode: "refutation-chain",
    label: "Refutation chain",
    description: "Surface disputed and contradictory reasoning.",
  },
  {
    mode: "tree",
    label: "Tree",
    description: "Breadth-first hierarchy from the selected node.",
  },
  {
    mode: "huffman",
    label: "Huffman",
    description: "Group nodes by weighted prefix depth.",
  },
  {
    mode: "table",
    label: "Table",
    description: "Sort research objects into type columns.",
  },
  {
    mode: "neural-network",
    label: "Neural network",
    description: "Arrange nodes as connected semantic layers.",
  },
];

/**
 * The legend filter is the operator itself: every relation is exactly one of
 * the five operators (T/K/I/M/Q).
 */
export function linkLegendFilterOf(edge: ResearchEdge): LinkLegendFilter {
  return edge.type;
}

export function edgeMatchesLegendFilter(
  edge: ResearchEdge,
  filter: LinkLegendFilter | null,
) {
  return filter === null || linkLegendFilterOf(edge) === filter;
}

/**
 * Produces a renderer-only projection; the persisted research project is untouched.
 * 生成仅供渲染的投影，不改变持久化研究项目。
 */
export function projectForLegendFilter(
  project: ProjectState,
  filter: LinkLegendFilter | null,
): ProjectState {
  if (filter === null) return project;
  const edges = project.edges.filter((edge) => edgeMatchesLegendFilter(edge, filter));
  const nodeIds = new Set(edges.flatMap((edge) => [edge.source, edge.target]));
  return {
    ...project,
    nodes: project.nodes.filter((node) => nodeIds.has(node.id)),
    edges,
    placements: project.placements.filter((placement) => nodeIds.has(placement.nodeId)),
  };
}
