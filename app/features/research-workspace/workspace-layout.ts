import type {
  LayoutMode,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
} from "../../lib/research-types";

export type LinkLegendFilter = "causal" | "control" | "derived" | "contradicts";

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

const controlTypes = new Set<ResearchEdgeType>(["controls", "mediates", "moderates"]);
const derivedTypes = new Set<ResearchEdgeType>(["derived_from"]);

/**
 * Converts persisted relation semantics into the four compact legend families.
 * 将持久化关系语义归并为图例中的四类紧凑关系。
 */
export function linkLegendFilterOf(edge: ResearchEdge): LinkLegendFilter {
  if (edge.type === "contradicts") return "contradicts";
  if (controlTypes.has(edge.type)) return "control";
  if (derivedTypes.has(edge.type)) return "derived";
  return "causal";
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
