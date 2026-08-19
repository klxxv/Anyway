import { MarkerType } from "@vue-flow/core";
import type {
  EdgeChange,
  EdgeMarkerType,
  NodeChange,
} from "@vue-flow/core";
import type { DiffOverlayState } from "../../../app/lib/graph/canvas-diff";
import type {
  EdgeStyleManifest,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNode,
} from "../../../app/lib/research-types";
import type {
  LinkLegendFilter,
  WorkspaceEdgeData,
  WorkspaceNodeData,
} from "./canvas-types";
import { computeEdgeRoutes } from "./edge-routing";
import { isExpandableVariable, variableBranchValues } from "./variable-branches";

export type CanvasNodeModel = {
  id: string;
  type: "researchNode";
  position: { x: number; y: number };
  width: number;
  height: number;
  selected: boolean;
  selectable?: boolean;
  dragging?: boolean;
  connectable?: boolean;
  focusable?: boolean;
  data: WorkspaceNodeData;
};

/** The adapter boundary keeps Vue Flow's runtime node fields concrete. */
export type ResearchVueFlowNode = CanvasNodeModel;

export type CanvasEdgeModel = {
  id: string;
  source: string;
  target: string;
  sourceHandle?: string;
  targetHandle?: string;
  type: "researchEdge";
  selected: boolean;
  markerEnd?: EdgeMarkerType;
  selectable?: boolean;
  focusable?: boolean;
  data: WorkspaceEdgeData;
};

/** The adapter boundary keeps Vue Flow's runtime edge fields concrete. */
export type ResearchVueFlowEdge = CanvasEdgeModel;

export function linkLegendFilterOf(edge: ResearchEdge): LinkLegendFilter {
  return edge.type;
}

export function projectForLegendFilter(
  project: ProjectState,
  filter: LinkLegendFilter | null,
): ProjectState {
  if (filter === null) return project;
  const edges = project.edges.filter((edge) => linkLegendFilterOf(edge) === filter);
  const nodeIds = new Set(edges.flatMap((edge) => [edge.source, edge.target]));
  return {
    ...project,
    nodes: project.nodes.filter((node) => nodeIds.has(node.id)),
    edges,
    placements: project.placements.filter((placement) => nodeIds.has(placement.nodeId)),
  };
}

export function customEdgeNote(edge: ResearchEdge): string {
  const note = edge.note?.trim() ?? "";
  return note === edge.type.replaceAll("_", " ") ? "" : note;
}

function markerForEdge(record: ResearchEdge, edgeStyle: EdgeStyleManifest): EdgeMarkerType | undefined {
  if (!record.directed || edgeStyle.marker.type === "none") return undefined;
  return {
    type: edgeStyle.marker.type === "closed-arrow" ? MarkerType.ArrowClosed : MarkerType.Arrow,
    width: Math.max(4, edgeStyle.marker.size || 8),
    height: Math.max(4, edgeStyle.marker.size || 8),
    color: edgeStyle.stroke.color,
  };
}

function defaultEdgeLabel(type: ResearchEdgeType): string {
  return type.replaceAll("_", " ");
}

export type BuildCanvasNodeOptions = {
  selectedNodeId: string;
  selectedNodeIds?: ReadonlySet<string>;
  filter: LinkLegendFilter | null;
  expandedNodeIds: ReadonlySet<string>;
  onToggleExpanded?: (nodeId: string) => void;
  typeLabel?: (type: ResearchNode["type"]) => string;
  highlightedNodeIds?: ReadonlySet<string>;
  diffOverlay?: DiffOverlayState | null;
};

export function buildCanvasNodeModels(
  project: ProjectState,
  options: BuildCanvasNodeOptions,
): CanvasNodeModel[] {
  const projected = projectForLegendFilter(project, options.filter);
  const nodes: CanvasNodeModel[] = projected.nodes.map((record) => {
    const placement = projected.placements.find((item) => item.nodeId === record.id);
    const circle = record.data.shape === "circle" || record.type === "question";
    const expanded =
      !circle && options.expandedNodeIds.has(record.id) && isExpandableVariable(record);
    const branchCount = expanded ? variableBranchValues(record).length : 0;
    return {
      id: record.id,
      type: "researchNode",
      position: { x: placement?.x ?? 0, y: placement?.y ?? 0 },
      width: expanded
        ? Math.max(placement?.width ?? 0, 188)
        : placement?.width ?? (circle ? 136 : 164),
      height: expanded
        ? Math.max(placement?.height ?? 0, 132 + Math.max(branchCount, 2) * 25)
        : placement?.height ?? (circle ? 136 : 116),
      selected:
        record.id === options.selectedNodeId ||
        options.selectedNodeIds?.has(record.id) === true,
      data: {
        record,
        shape: circle ? "circle" : "card",
        expanded,
        onToggleExpanded: options.onToggleExpanded,
        typeLabel: options.typeLabel?.(record.type),
        highlighted: options.highlightedNodeIds?.has(record.id),
        diffState: options.diffOverlay?.nodes[record.id],
      },
    };
  });

  for (const ghost of options.diffOverlay?.removedNodes ?? []) {
    nodes.push({
      id: ghost.record.id,
      type: "researchNode",
      position: { x: ghost.x, y: ghost.y },
      width: 164,
      height: 116,
      selected: false,
      selectable: false,
      dragging: false,
      connectable: false,
      focusable: false,
      data: {
        record: ghost.record,
        shape:
          ghost.record.data.shape === "circle" || ghost.record.type === "question"
            ? "circle"
            : "card",
        expanded: false,
        onToggleExpanded: options.onToggleExpanded,
        typeLabel: options.typeLabel?.(ghost.record.type),
        diffState: "removed",
      },
    });
  }
  return nodes;
}

export type BuildCanvasEdgeOptions = {
  selectedEdgeId: string;
  selectedEdgeIds?: ReadonlySet<string>;
  filter: LinkLegendFilter | null;
  edgeTypeLabel?: (type: ResearchEdgeType) => string;
  edgeStyle: EdgeStyleManifest;
  highlightedEdgeIds?: ReadonlySet<string>;
  diffOverlay?: DiffOverlayState | null;
};

export function buildCanvasEdgeModels(
  project: ProjectState,
  options: BuildCanvasEdgeOptions,
): CanvasEdgeModel[] {
  const projected = projectForLegendFilter(project, options.filter);
  const routes = computeEdgeRoutes(projected);
  const edgeLabel = options.edgeTypeLabel ?? defaultEdgeLabel;
  const edges: CanvasEdgeModel[] = projected.edges.map((record) => {
    const route = routes[record.id];
    return {
      id: record.id,
      source: record.source,
      target: record.target,
      sourceHandle: route?.sourceHandle,
      targetHandle: route?.targetHandle,
      type: "researchEdge",
      selected:
        record.id === options.selectedEdgeId ||
        options.selectedEdgeIds?.has(record.id) === true,
      markerEnd: markerForEdge(record, options.edgeStyle),
      data: {
        record,
        label: customEdgeNote(record) || edgeLabel(record.type),
        edgeStyle: options.edgeStyle,
        labelOffsetX: route?.labelOffsetX,
        labelOffsetY: route?.labelOffsetY,
        highlighted: options.highlightedEdgeIds?.has(record.id),
        diffState: options.diffOverlay?.edges[record.id],
      },
    };
  });

  for (const ghost of options.diffOverlay?.removedEdges ?? []) {
    edges.push({
      id: ghost.record.id,
      source: ghost.record.source,
      target: ghost.record.target,
      type: "researchEdge",
      selected: false,
      markerEnd: markerForEdge(ghost.record, options.edgeStyle),
      selectable: false,
      focusable: false,
      data: {
        record: ghost.record,
        label: customEdgeNote(ghost.record) || edgeLabel(ghost.record.type),
        edgeStyle: options.edgeStyle,
        diffState: "removed",
      },
    });
  }
  return edges;
}

export function toVueFlowNodes(models: CanvasNodeModel[]): ResearchVueFlowNode[] {
  return models.map((model): ResearchVueFlowNode => ({
    id: model.id,
    type: model.type,
    position: model.position,
    width: model.width,
    height: model.height,
    selected: model.selected,
    selectable: model.selectable,
    dragging: model.dragging,
    connectable: model.connectable,
    focusable: model.focusable,
    data: model.data,
  }));
}

export function toVueFlowEdges(models: CanvasEdgeModel[]): ResearchVueFlowEdge[] {
  return models.map((model): ResearchVueFlowEdge => ({
    id: model.id,
    source: model.source,
    target: model.target,
    sourceHandle: model.sourceHandle,
    targetHandle: model.targetHandle,
    type: model.type,
    selected: model.selected,
    markerEnd: model.markerEnd,
    selectable: model.selectable,
    focusable: model.focusable,
    data: model.data,
  }));
}

export function createPreviewEdge(
  connection: { source: string; target: string; sourceHandle?: string | null; targetHandle?: string | null },
  type: ResearchEdgeType,
  edgeStyle: EdgeStyleManifest,
  label: string,
): ResearchVueFlowEdge {
  const record: ResearchEdge = {
    id: "edge-preview",
    source: connection.source,
    target: connection.target,
    type,
    directed: true,
    polarity: "positive",
    conditions: [],
    evidenceIds: [],
    provenance: { origin: "human" },
  };
  return {
    id: `edge-preview-${Date.now()}`,
    source: connection.source,
    target: connection.target,
    sourceHandle: connection.sourceHandle ?? undefined,
    targetHandle: connection.targetHandle ?? undefined,
    type: "researchEdge",
    selected: false,
    markerEnd: markerForEdge(record, edgeStyle),
    data: { record, label, edgeStyle },
  };
}

export function applyNodeChangesCompat(
  changes: NodeChange[],
  current: ResearchVueFlowNode[],
): ResearchVueFlowNode[] {
  let next = current;
  for (const change of changes) {
    if (change.type === "remove" || change.type === "add") continue;
    const index = next.findIndex((node) => node.id === change.id);
    if (index < 0) continue;
    const node = next[index];
    const updated: ResearchVueFlowNode = { ...node };
    if (change.type === "position") {
      // Vue Flow emits position changes without a `position` on drag-end
      // (`changed=false`); guard so we never clobber a node's position with
      // `undefined` (vue-flow-core reads `node.position.x`).
      if (change.position) updated.position = change.position;
      updated.dragging = change.dragging;
    } else if (change.type === "select") {
      updated.selected = change.selected;
    }
    if (next === current) next = [...current];
    next[index] = updated;
  }
  return next;
}

export function applyEdgeChangesCompat(
  changes: EdgeChange[],
  current: ResearchVueFlowEdge[],
): ResearchVueFlowEdge[] {
  let next = current;
  for (const change of changes) {
    if (change.type === "remove" || change.type === "add") continue;
    const index = next.findIndex((edge) => edge.id === change.id);
    if (index < 0) continue;
    if (next === current) next = [...current];
    if (change.type === "select") {
      next[index] = { ...next[index], selected: change.selected };
    }
  }
  return next;
}

export type GraphViewport = { x: number; y: number; zoom: number };

export type VueFlowViewportApi = {
  fitView?: (options?: Record<string, unknown>) => void | Promise<void>;
  setViewport?: (viewport: GraphViewport, options?: Record<string, unknown>) => void | Promise<void>;
  getViewport?: () => GraphViewport;
  zoomIn?: (options?: Record<string, unknown>) => Promise<unknown>;
  zoomOut?: (options?: Record<string, unknown>) => Promise<unknown>;
  screenToFlowPosition?: (point: { x: number; y: number }) => { x: number; y: number };
  screenToFlowCoordinate?: (point: { x: number; y: number }) => { x: number; y: number };
};
