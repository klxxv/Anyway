<script setup lang="ts">
import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import { storeToRefs } from "pinia";
import {
  ConnectionMode,
  VueFlow,
  type Connection,
  type EdgeChange,
  type EdgeMouseEvent,
  type GraphEdge,
  type GraphNode,
  type NodeChange,
  type NodeDragEvent,
  type NodeMouseEvent,
} from "@vue-flow/core";
import ResearchEdgeLine from "./ResearchEdgeLine.vue";
import ResearchNodeCard from "./ResearchNodeCard.vue";
import type {
  CanvasTrackpadFrame,
  CanvasTrackpadGesture,
  ContextMenuActionId,
  PieMenuState,
  RadialMenuItem,
  ResearchGraphCanvasEmits,
  ResearchGraphCanvasProps,
  ResolvedPluginContextMenuAction,
  WorkspaceContextMenuState,
} from "./canvas-types";
import type { ResearchEdgeType, ResearchNodeType } from "../../../app/lib/research-types";
import {
  applyEdgeChangesCompat,
  applyNodeChangesCompat,
  buildCanvasEdgeModels,
  buildCanvasNodeModels,
  createPreviewEdge,
  linkLegendFilterOf,
  toVueFlowEdges,
  toVueFlowNodes,
  type GraphViewport,
  type ResearchVueFlowEdge,
  type ResearchVueFlowNode,
type VueFlowViewportApi,
} from "./vue-flow-adapter";
import { isExpandableVariable } from "./variable-branches";
import { useCanvasInteractionStore } from "../stores/canvas-interaction";
import "./vue-flow-compat.css";

const props = defineProps<ResearchGraphCanvasProps>();
const emit = defineEmits<ResearchGraphCanvasEmits>();
const canvasStore = useCanvasInteractionStore();
const {
  viewport: storedViewport,
  contextMenu,
  radialMenu: pieMenu,
  expandedNodeIds,
  draggingNodeId,
  manualMove,
  lastTrackpadFrameId,
  interactionMode,
} = storeToRefs(canvasStore);

const nodeTypes = { researchNode: ResearchNodeCard };
const edgeTypes = { researchEdge: ResearchEdgeLine };
const fitViewOptions = { padding: 0.12, maxZoom: 1 };
const minZoom = 0.45;
const maxZoom = 1.7;

const wrapperRef = ref<HTMLElement | null>(null);
const flowRef = shallowRef<VueFlowViewportApi | null>(null);
const nodes = ref<ResearchVueFlowNode[]>([]);
const edges = ref<ResearchVueFlowEdge[]>([]);
const canvasSize = ref({ width: 0, height: 0 });
const resizeObserver = shallowRef<ResizeObserver | null>(null);

function researchNodeOf(node: GraphNode): ResearchVueFlowNode | null {
  if (node.type !== "researchNode" || !node.data || typeof node.data !== "object" || !("record" in node.data)) {
    return null;
  }
  return node as unknown as ResearchVueFlowNode;
}

function researchEdgeOf(edge: GraphEdge): ResearchVueFlowEdge | null {
  if (edge.type !== "researchEdge" || !edge.data || typeof edge.data !== "object" || !("record" in edge.data)) {
    return null;
  }
  return edge as unknown as ResearchVueFlowEdge;
}

const isTauriRuntime = computed(
  () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window,
);

const defaultNodeLabels: Partial<Record<ResearchNodeType, string>> = {
  question: "Question",
  concept: "Concept",
  variable: "Variable",
  method: "Method",
  dataset: "Dataset",
  evidence: "Evidence",
  paper: "Paper",
  result: "Result",
  note: "Note",
};

const edgeLabelFallbacks: Record<ResearchEdgeType, string> = {
  causes: "causes",
  correlates: "correlates",
  supports: "supports",
  contradicts: "contradicts",
  depends_on: "depends on",
  derived_from: "derived from",
  part_of: "part of",
  controls: "controls",
  mediates: "mediates",
  moderates: "moderates",
  uses: "uses",
  measures: "measures",
};

function translate(key: string, fallback: string): string {
  return props.translate?.(key) || fallback;
}

function nodeTypeLabel(type: ResearchNodeType): string {
  return translate(`node.${type === "concept" ? "group" : type === "dataset" ? "data" : type}`, defaultNodeLabels[type] ?? type);
}

function edgeTypeLabel(type: ResearchEdgeType): string {
  const key = type === "depends_on" ? "dependsOn" : type === "derived_from" ? "derivedFrom" : type;
  return translate(`edgeType.${key}`, edgeLabelFallbacks[type]);
}

function selectNode(nodeId: string) {
  props.onSelectNode?.(nodeId);
  emit("select-node", nodeId);
}

function selectEdge(edgeId: string) {
  props.onSelectEdge?.(edgeId);
  emit("select-edge", edgeId);
}

function moveNode(nodeId: string, x: number, y: number) {
  props.onMoveNode?.(nodeId, x, y);
  emit("move-node", nodeId, x, y);
}

function requestCreate(type: ResearchNodeType, x: number, y: number) {
  props.onRequestCreate?.(type, x, y);
  emit("request-create", type, x, y);
}

function closeContextMenu() {
  canvasStore.closeContextMenu();
}

function clearTransientMenus() {
  canvasStore.clearTransientMenus();
}

function toggleExpanded(nodeId: string) {
  canvasStore.toggleExpanded(nodeId);
}

function rebuildGraph() {
  const highlightedNodeIds = props.highlightChain
    ? new Set(props.highlightChain.nodeIds)
    : undefined;
  const highlightedEdgeIds = props.highlightChain
    ? new Set(props.highlightChain.edgeIds)
    : undefined;
  nodes.value = toVueFlowNodes(
    buildCanvasNodeModels(props.project, {
      selectedNodeId: props.selectedNodeId,
      filter: props.linkFilter,
      expandedNodeIds: new Set(expandedNodeIds.value),
      onToggleExpanded: toggleExpanded,
      typeLabel: nodeTypeLabel,
      highlightedNodeIds,
      diffOverlay: props.diffOverlay,
    }),
  );
  edges.value = toVueFlowEdges(
    buildCanvasEdgeModels(props.project, {
      selectedEdgeId: props.selectedEdgeId,
      filter: props.linkFilter,
      edgeTypeLabel,
      edgeStyle: props.edgeStyle,
      highlightedEdgeIds,
      diffOverlay: props.diffOverlay,
    }),
  );
}

watch(
  () => [
    props.project,
    props.selectedNodeId,
    props.selectedEdgeId,
    props.linkFilter,
    props.edgeStyle,
    props.highlightChain,
    props.diffOverlay,
    expandedNodeIds.value,
  ],
  rebuildGraph,
  { immediate: true, deep: true },
);

function viewportApi(): VueFlowViewportApi | null {
  return flowRef.value;
}

function getViewport(): GraphViewport {
  const nextViewport = viewportApi()?.getViewport?.();
  if (nextViewport) {
    canvasStore.setViewport(nextViewport);
    return nextViewport;
  }
  return storedViewport.value;
}

function setViewport(viewport: GraphViewport, duration = 0) {
  canvasStore.setViewport(viewport);
  const instance = viewportApi();
  if (!instance?.setViewport) return;
  void instance.setViewport(viewport, duration ? { duration } : undefined);
}

function fitView(options: Record<string, unknown> = fitViewOptions) {
  void viewportApi()?.fitView?.(options);
}

function zoomIn() {
  void viewportApi()?.zoomIn?.({ duration: 120 });
}

function zoomOut() {
  void viewportApi()?.zoomOut?.({ duration: 120 });
}

async function applyWorkspaceViewport() {
  const width = wrapperRef.value?.clientWidth ?? 0;
  const height = wrapperRef.value?.clientHeight ?? 0;
  if (props.referenceViewport && width >= 1050 && height >= 760) {
    setViewport({ x: 111, y: 17, zoom: 0.93 }, 140);
    return;
  }
  fitView({ ...fitViewOptions, duration: 220 });
}

function screenToFlow(clientX: number, clientY: number) {
  const bounds = wrapperRef.value?.getBoundingClientRect();
  const screen = {
    x: clientX - (bounds?.left ?? 0),
    y: clientY - (bounds?.top ?? 0),
  };
  const instance = viewportApi();
  const point = instance?.screenToFlowPosition?.({ x: clientX, y: clientY })
    ?? instance?.screenToFlowCoordinate?.({ x: clientX, y: clientY });
  return { screen, flow: point ?? screen };
}

function pointerToFlow(event: MouseEvent | TouchEvent) {
  const point = "clientX" in event
    ? { x: event.clientX, y: event.clientY }
    : {
        x: event.touches[0]?.clientX ?? 0,
        y: event.touches[0]?.clientY ?? 0,
      };
  return screenToFlow(point.x, point.y);
}

function openRadialMenu(menu: PieMenuState) {
  canvasStore.openRadialMenu(menu);
  props.onRadialMenuOpen?.(menu);
  emit("radial-menu-open", menu);
}

function closeRadialMenu() {
  if (pieMenu.value) {
    props.onRadialMenuClose?.();
    emit("radial-menu-close");
  }
  canvasStore.closeRadialMenu();
}

function nodeTypeForRadialAction(action: RadialMenuItem["action"]): ResearchNodeType | null {
  return action.startsWith("create:") ? (action.slice("create:".length) as ResearchNodeType) : null;
}

function chooseRadialItem(item: RadialMenuItem) {
  const menu = pieMenu.value;
  if (!menu) return;
  const nodeType = nodeTypeForRadialAction(item.action);
  if (nodeType) requestCreate(nodeType, menu.flowX, menu.flowY);
  else if (item.action === "canvas:fit") fitView({ ...fitViewOptions, duration: 220 });
  else props.onApplyDefaultLayout?.();
  if (item.action === "canvas:default-layout") emit("apply-default-layout");
  props.onRadialMenuChoose?.(item, menu.flowX, menu.flowY);
  emit("radial-menu-choose", item, menu.flowX, menu.flowY);
  closeRadialMenu();
}

function handleFlowInit(instance: unknown) {
  flowRef.value = instance as VueFlowViewportApi;
  void nextTick(() => {
    if (pieMenu.value) {
      const point = screenToFlow(
        pieMenu.value.screenX + (wrapperRef.value?.getBoundingClientRect().left ?? 0),
        pieMenu.value.screenY + (wrapperRef.value?.getBoundingClientRect().top ?? 0),
      ).flow;
      canvasStore.openRadialMenu({ ...pieMenu.value, flowX: point.x, flowY: point.y });
    }
    window.setTimeout(() => void applyWorkspaceViewport(), 80);
  });
}

function setIncidentEdgePreview(nodeId: string, enabled: boolean) {
  const nextEdges: ResearchVueFlowEdge[] = [];
  for (const edge of edges.value) {
    if (edge.source !== nodeId && edge.target !== nodeId) {
      nextEdges.push(edge);
      continue;
    }
    if (Boolean(edge.data.dragPreview) === enabled) {
      nextEdges.push(edge);
      continue;
    }
    nextEdges.push({ ...edge, data: { ...edge.data, dragPreview: enabled } });
  }
  edges.value = nextEdges;
}

function handleNodeClick(payload: NodeMouseEvent) {
  const node = researchNodeOf(payload.node);
  if (!node) return;
  selectNode(node.id);
  clearTransientMenus();
}

function handleNodeDoubleClick(payload: NodeMouseEvent) {
  const node = researchNodeOf(payload.node);
  if (!node) return;
  props.onNodeDoubleClick?.(node.id);
  emit("node-double-click", node.id);
}

function handleNodeContextMenu(payload: NodeMouseEvent) {
  const node = researchNodeOf(payload.node);
  if (!node) return;
  const event = payload.event;
  event.preventDefault();
  event.stopPropagation();
  const point = pointerToFlow(event);
  selectNode(node.id);
  closeRadialMenu();
  canvasStore.openContextMenu({
    scope: "node",
    targetId: node.id,
    title: node.data.record.title,
    screenX: point.screen.x,
    screenY: point.screen.y,
    flowX: point.flow.x,
    flowY: point.flow.y,
  });
}

function handleEdgeClick(payload: EdgeMouseEvent) {
  const edge = researchEdgeOf(payload.edge);
  if (!edge) return;
  selectEdge(edge.id);
  clearTransientMenus();
}

function handleEdgeContextMenu(payload: EdgeMouseEvent) {
  const edge = researchEdgeOf(payload.edge);
  if (!edge) return;
  const event = payload.event;
  event.preventDefault();
  event.stopPropagation();
  const point = pointerToFlow(event);
  selectEdge(edge.id);
  closeRadialMenu();
  canvasStore.openContextMenu({
    scope: "edge",
    targetId: edge.id,
    title: edge.data?.label,
    screenX: point.screen.x,
    screenY: point.screen.y,
    flowX: point.flow.x,
    flowY: point.flow.y,
  });
}

function handleNodeDragStart(payload: NodeDragEvent) {
  const node = researchNodeOf(payload.node);
  if (!node) return;
  canvasStore.setDraggingNode(node.id);
  setIncidentEdgePreview(node.id, true);
}

function handleNodeDragStop(payload: NodeDragEvent) {
  const node = researchNodeOf(payload.node);
  if (!node) return;
  setIncidentEdgePreview(node.id, false);
  canvasStore.setDraggingNode(null);
  canvasStore.setManualMove({ nodeId: node.id, x: node.position.x, y: node.position.y });
  moveNode(node.id, node.position.x, node.position.y);
}

function handlePaneClick() {
  clearTransientMenus();
}

function handlePaneContextMenu(event: MouseEvent) {
  event.preventDefault();
  const point = screenToFlow(event.clientX, event.clientY);
  closeRadialMenu();
  canvasStore.openContextMenu({
    scope: "canvas",
    screenX: point.screen.x,
    screenY: point.screen.y,
    flowX: point.flow.x,
    flowY: point.flow.y,
  });
}

function handleNodesChange(changes: NodeChange[]) {
  for (const change of changes) {
    if (change.type === "remove") props.onDeleteNode?.(change.id), emit("delete-node", change.id);
  }
  const nonRemove = changes.filter((change) => change.type !== "remove");
  if (nonRemove.length) nodes.value = applyNodeChangesCompat(nonRemove, nodes.value);
}

function handleEdgesChange(changes: EdgeChange[]) {
  for (const change of changes) {
    if (change.type === "remove") props.onDeleteEdge?.(change.id), emit("delete-edge", change.id);
  }
  const nonRemove = changes.filter((change) => change.type !== "remove");
  if (nonRemove.length) edges.value = applyEdgeChangesCompat(nonRemove, edges.value);
}

function handleConnect(connection: Connection) {
  edges.value = [
    ...edges.value,
    createPreviewEdge(
      { source: connection.source, target: connection.target, sourceHandle: connection.sourceHandle, targetHandle: connection.targetHandle },
      props.connectType,
      props.edgeStyle,
      edgeTypeLabel(props.connectType),
    ),
  ];
  props.onCreateEdge?.(connection.source, connection.target);
  emit("create-edge", connection.source, connection.target);
}

function handleContextAction(action: ContextMenuActionId) {
  const menu = contextMenu.value;
  if (!menu) return;
  closeContextMenu();
  switch (action) {
    case "node.inspect":
      if (menu.targetId) selectNode(menu.targetId);
      break;
    case "node.connect":
      if (menu.targetId) props.onRequestConnect?.(menu.targetId), emit("request-connect", menu.targetId);
      break;
    case "node.duplicate":
      if (menu.targetId) props.onDuplicateNode?.(menu.targetId), emit("duplicate-node", menu.targetId);
      break;
    case "node.delete":
      if (menu.targetId) props.onDeleteNode?.(menu.targetId), emit("delete-node", menu.targetId);
      break;
    case "edge.filter": {
      const edge = props.project.edges.find((item) => item.id === menu.targetId);
      if (edge) props.onLegendFilter?.(linkLegendFilterOf(edge)), emit("legend-filter", linkLegendFilterOf(edge));
      break;
    }
    case "edge.reverse":
      if (menu.targetId) props.onReverseEdge?.(menu.targetId), emit("reverse-edge", menu.targetId);
      break;
    case "edge.delete":
      if (menu.targetId) props.onDeleteEdge?.(menu.targetId), emit("delete-edge", menu.targetId);
      break;
    case "canvas.add":
      openRadialMenu({ screenX: menu.screenX, screenY: menu.screenY, flowX: menu.flowX, flowY: menu.flowY });
      break;
    case "canvas.note":
      requestCreate("note", menu.flowX, menu.flowY);
      break;
    case "canvas.expandAll":
      canvasStore.setExpandedNodeIds(props.project.nodes.filter(isExpandableVariable).map((node) => node.id));
      break;
    case "canvas.collapseAll":
      canvasStore.setExpandedNodeIds([]);
      break;
    case "canvas.layout":
      props.onApplyDefaultLayout?.();
      emit("apply-default-layout");
      break;
    case "canvas.fit":
      fitView({ ...fitViewOptions, duration: 220 });
      break;
  }
}

function handlePluginAction(action: ResolvedPluginContextMenuAction) {
  const menu = contextMenu.value;
  if (!menu) return;
  closeContextMenu();
  props.onPluginContextMenuAction?.(action, menu);
  emit("plugin-context-menu-action", action, menu);
}

function handleWheel(event: WheelEvent) {
  event.preventDefault();
  event.stopPropagation();
  if (props.canvasInputBlocked) return;
  const bounds = wrapperRef.value?.getBoundingClientRect();
  const cursor = {
    x: event.clientX - (bounds?.left ?? 0),
    y: event.clientY - (bounds?.top ?? 0),
  };
  const current = getViewport();
  const sensitivity = Math.min(2, Math.max(0.5, props.trackpadSensitivity || 1));
  const panFactor = event.deltaMode === 1 ? 20 : event.deltaMode === 2 ? 100 : 1;
  const panX = event.ctrlKey ? 0 : event.deltaX * panFactor * sensitivity;
  const panY = event.ctrlKey ? 0 : event.deltaY * panFactor * sensitivity;
  const scale = event.ctrlKey ? Math.exp((-event.deltaY / 100) * sensitivity) : 1;
  const nextZoom = Math.min(maxZoom, Math.max(minZoom, current.zoom * Math.max(0.25, Math.min(4, scale))));
  const flowX = (cursor.x - current.x) / current.zoom;
  const flowY = (cursor.y - current.y) / current.zoom;
  const nextViewport = {
    x: cursor.x - flowX * nextZoom - panX,
    y: cursor.y - flowY * nextZoom - panY,
    zoom: nextZoom,
  };
  setViewport(nextViewport);
  const frame: CanvasTrackpadFrame = {
    phase: "update",
    frameId: Date.now(),
    contacts: [],
    centerX: event.clientX,
    centerY: event.clientY,
    span: 0,
    scale,
    panX,
    panY,
    deviceWidth: bounds?.width ?? 1,
    deviceHeight: bounds?.height ?? 1,
    cursorX: event.clientX,
    cursorY: event.clientY,
    heldMs: 0,
    held: false,
  };
  props.onTrackpadFrame?.(frame);
  emit("trackpad-frame", frame);
  const gesture: CanvasTrackpadGesture = { frame, viewport: nextViewport };
  props.onTrackpadGesture?.(gesture);
  emit("trackpad-gesture", gesture);
}

function handleTrackpadFrame(frame: CanvasTrackpadFrame) {
  if (props.canvasInputBlocked) return;
  if (lastTrackpadFrameId.value === frame.frameId && frame.phase !== "end") return;
  canvasStore.setLastTrackpadFrameId(frame.frameId);
  props.onTrackpadFrame?.(frame);
  emit("trackpad-frame", frame);
  const point = screenToFlow(frame.cursorX, frame.cursorY);
  if (frame.held && frame.phase !== "end") {
    const menu = pieMenu.value ?? {
      screenX: point.screen.x,
      screenY: point.screen.y,
      flowX: point.flow.x,
      flowY: point.flow.y,
      gestureActive: true,
    };
    openRadialMenu(menu);
    const gesture: CanvasTrackpadGesture = { frame, radial: menu };
    props.onTrackpadGesture?.(gesture);
    emit("trackpad-gesture", gesture);
    return;
  }
  if (!pieMenu.value) {
    const current = getViewport();
    const sensitivity = Math.min(2, Math.max(0.5, props.trackpadSensitivity || 1));
    const nextZoom = Math.min(maxZoom, Math.max(minZoom, current.zoom * Math.max(0.25, Math.min(4, frame.scale || 1))));
    setViewport({
      x: current.x - frame.panX * sensitivity,
      y: current.y - frame.panY * sensitivity,
      zoom: nextZoom,
    });
  }
  if (frame.phase === "end") closeRadialMenu();
  const gesture: CanvasTrackpadGesture = { frame, viewport: getViewport(), radial: pieMenu.value ?? undefined };
  props.onTrackpadGesture?.(gesture);
  emit("trackpad-gesture", gesture);
}

const contextActions = computed(() => {
  const scope = contextMenu.value?.scope;
  return scope ? props.contextMenus[scope] ?? [] : [];
});

const pluginActions = computed(() => {
  const scope = contextMenu.value?.scope;
  return scope ? props.pluginContextMenuActions.filter((action) => action.scope === scope) : [];
});

const contextMenuStyle = computed(() => {
  const menu = contextMenu.value;
  if (!menu) return {};
  return {
    left: `${Math.min(Math.max(menu.screenX, 6), Math.max(canvasSize.value.width - 206, 6))}px`,
    top: `${Math.min(Math.max(menu.screenY, 6), Math.max(canvasSize.value.height - 250, 6))}px`,
  };
});

const actionLabels: Record<ContextMenuActionId, string> = {
  "node.inspect": "Inspect",
  "node.connect": "Connect",
  "node.duplicate": "Duplicate",
  "node.delete": "Delete node",
  "edge.filter": "Filter relation",
  "edge.reverse": "Reverse edge",
  "edge.delete": "Delete edge",
  "canvas.add": "Quick add",
  "canvas.note": "Add note",
  "canvas.expandAll": "Expand all",
  "canvas.collapseAll": "Collapse all",
  "canvas.layout": "Apply layout",
  "canvas.fit": "Fit view",
};

const legendItems = [
  { key: "causal" as const, label: "Causal", lineClass: "border-ink/70" },
  { key: "control" as const, label: "Control", lineClass: "border-dashed border-ink/70" },
  { key: "derived" as const, label: "Derived", lineClass: "border-dotted border-ink/70" },
  { key: "contradicts" as const, label: "Contradicts", lineClass: "border-dashed border-alert", textClass: "text-alert" },
];

const legendCounts = computed(() => {
  const counts = { causal: 0, control: 0, derived: 0, contradicts: 0 };
  props.project.edges.forEach((edge) => {
    counts[linkLegendFilterOf(edge)] += 1;
  });
  return counts;
});

function minimapColor(node: ResearchVueFlowNode) {
  if (node.id === props.selectedNodeId) return "var(--minimap-selected-node)";
  if (node.data.record.type === "evidence") return "var(--minimap-evidence-node)";
  return "var(--minimap-node)";
}

const minimapNodes = computed(() =>
  nodes.value.map((node) => ({
    id: node.id,
    x: node.position.x,
    y: node.position.y,
    width: node.width,
    height: node.height,
    color: minimapColor(node),
  })),
);

const minimapRelations = computed(() => {
  const byId = new Map(minimapNodes.value.map((node) => [node.id, node]));
  return edges.value.flatMap((edge) => {
    const source = byId.get(edge.source);
    const target = byId.get(edge.target);
    if (!source || !target) return [];
    return [{
      id: edge.id,
      x1: source.x + source.width / 2,
      y1: source.y + source.height / 2,
      x2: target.x + target.width / 2,
      y2: target.y + target.height / 2,
    }];
  });
});

const minimapViewBox = computed(() => {
  if (!nodes.value.length) return "0 0 1 1";
  const minX = Math.min(...nodes.value.map((node) => node.position.x));
  const minY = Math.min(...nodes.value.map((node) => node.position.y));
  const maxX = Math.max(...nodes.value.map((node) => node.position.x + (node.width ?? 164)));
  const maxY = Math.max(...nodes.value.map((node) => node.position.y + (node.height ?? 116)));
  return `${minX - 20} ${minY - 20} ${Math.max(maxX - minX + 40, 1)} ${Math.max(maxY - minY + 40, 1)}`;
});

const canvasClass = computed(() => `zen-flow h-full w-full ${props.connectMode ? "is-connecting" : ""}`);

watch(
  () => props.addRequest,
  (request) => {
    if (!request) return;
    const width = wrapperRef.value?.clientWidth ?? 1040;
    const height = wrapperRef.value?.clientHeight ?? 780;
    const screenX = Math.round(width * 0.8);
    const screenY = Math.round(height * 0.48);
    const point = screenToFlow(
      screenX + (wrapperRef.value?.getBoundingClientRect().left ?? 0),
      screenY + (wrapperRef.value?.getBoundingClientRect().top ?? 0),
    );
    openRadialMenu({ screenX, screenY, flowX: point.flow.x, flowY: point.flow.y });
  },
);

watch(
  () => props.diffFocus,
  (focus) => {
    if (!focus) return;
    const targetId = focus.kind === "edge"
      ? edges.value.find((edge) => edge.id === focus.id)?.source ?? focus.id
      : focus.id;
    if (!nodes.value.some((node) => node.id === targetId)) return;
    window.setTimeout(() => fitView({ nodes: [{ id: targetId }], padding: 0.35, maxZoom: 1.15, duration: 260 }), 140);
  },
);

watch(
  () => [props.inspectorOpen, props.linkFilter, props.project.placements],
  () => {
    window.setTimeout(() => void applyWorkspaceViewport(), props.inspectorOpen ? 380 : 40);
  },
  { deep: true },
);

onMounted(() => {
  const element = wrapperRef.value;
  if (!element) return;
  const resize = () => {
    const rect = element.getBoundingClientRect();
    canvasSize.value = { width: element.clientWidth, height: element.clientHeight };
    element.dataset.canvasBounds = `${rect.left},${rect.top},${rect.right},${rect.bottom}`;
  };
  resize();
  resizeObserver.value = typeof ResizeObserver !== "undefined" ? new ResizeObserver(resize) : null;
  resizeObserver.value?.observe(element);
  element.addEventListener("wheel", handleWheel, { capture: true, passive: false });
});

watch(
  () => props.trackpadFrame,
  (frame) => {
    if (frame) handleTrackpadFrame(frame);
  },
  { deep: true },
);

onBeforeUnmount(() => {
  resizeObserver.value?.disconnect();
  wrapperRef.value?.removeEventListener("wheel", handleWheel, { capture: true });
  resizeObserver.value = null;
  flowRef.value = null;
});

defineExpose({ fitView, openRadialMenu, closeRadialMenu, handleTrackpadFrame });
</script>

<template>
  <div
    ref="wrapperRef"
    class="research-graph-canvas relative h-full min-h-0 overflow-hidden bg-canvas"
    :data-interaction-mode="interactionMode"
    @contextmenu.capture.prevent
  >
    <VueFlow
      :nodes="nodes"
      :edges="edges"
      :node-types="nodeTypes"
      :edge-types="edgeTypes"
      :fit-view-on-init="true"
      :fit-view-options="fitViewOptions"
      :min-zoom="minZoom"
      :max-zoom="maxZoom"
      :zoom-on-pinch="!isTauriRuntime"
      :zoom-on-scroll="false"
      :pan-on-scroll="!isTauriRuntime"
      :prevent-scrolling="true"
      :connect-on-click="props.connectMode"
      :connection-mode="ConnectionMode.Loose"
      :connection-radius="26"
      :class="canvasClass"
      @init="handleFlowInit"
      @nodes-change="handleNodesChange"
      @edges-change="handleEdgesChange"
      @connect="handleConnect"
      @node-click="handleNodeClick"
      @node-double-click="handleNodeDoubleClick"
      @node-context-menu="handleNodeContextMenu"
      @edge-click="handleEdgeClick"
      @edge-context-menu="handleEdgeContextMenu"
      @node-drag-start="handleNodeDragStart"
      @node-drag-stop="handleNodeDragStop"
      @pane-click="handlePaneClick"
      @pane-context-menu="handlePaneContextMenu"
    >
      <div class="zen-flow-background" aria-hidden="true" />
      <div v-if="props.showMiniMap" class="vue-flow__minimap zen-minimap" aria-label="Canvas minimap">
        <svg :viewBox="minimapViewBox" preserveAspectRatio="none">
          <line
            v-for="relation in minimapRelations"
            :key="`mini-${relation.id}`"
            :x1="relation.x1"
            :y1="relation.y1"
            :x2="relation.x2"
            :y2="relation.y2"
            stroke="var(--minimap-relation)"
            stroke-width="3"
            opacity="0.72"
            vector-effect="non-scaling-stroke"
          />
          <rect
            v-for="node in minimapNodes"
            :key="`mini-node-${node.id}`"
            :x="node.x"
            :y="node.y"
            :width="node.width"
            :height="node.height"
            :fill="node.color"
            rx="3"
          />
        </svg>
      </div>
      <div class="vue-flow__controls zen-controls" aria-label="Canvas controls">
        <button class="vue-flow__controls-button" type="button" title="Zoom in" aria-label="Zoom in" @click="zoomIn">+</button>
        <button class="vue-flow__controls-button" type="button" title="Zoom out" aria-label="Zoom out" @click="zoomOut">−</button>
        <button class="vue-flow__controls-button" type="button" title="Fit view" aria-label="Fit view" @click="fitView({ ...fitViewOptions, duration: 220 })">⌖</button>
      </div>
    </VueFlow>

    <div class="absolute bottom-5 right-5 z-10 w-[154px] rounded-[5px] border border-ink/30 bg-paper/96 p-2 font-serif text-[10px] text-ink/80">
      <div class="flex items-center justify-between px-2 pb-1.5 font-sans text-[7px] uppercase tracking-[0.14em] text-ink/40">
        <span>{{ translate('workspace.linkFilter', 'Link filter') }}</span>
        <span v-if="props.showLinkCounts">{{ props.linkFilter ? legendCounts[props.linkFilter] : props.project.edges.length }}</span>
      </div>
      <button
        v-for="item in legendItems"
        :key="item.key"
        class="flex w-full items-center gap-3 rounded-[3px] px-2 py-1.5 text-left transition hover:bg-ink/5"
        :class="[props.linkFilter === item.key ? 'bg-blue-soft ring-1 ring-inset ring-blue/25' : '', item.textClass ?? '']"
        :aria-pressed="props.linkFilter === item.key"
        @click="props.onLegendFilter?.(props.linkFilter === item.key ? null : item.key); emit('legend-filter', props.linkFilter === item.key ? null : item.key)"
      >
        <span class="block w-9 border-t" :class="item.lineClass" />
        <span class="min-w-0 flex-1">{{ item.label }}</span>
        <span v-if="props.showLinkCounts" class="font-sans text-[8px] text-ink/35">{{ legendCounts[item.key] }}</span>
      </button>
    </div>

    <div
      v-if="contextMenu"
      class="zen-canvas-context-menu absolute z-30 min-w-[190px] rounded-[4px] border border-ink/25 bg-paper p-1.5 shadow-lg"
      :style="contextMenuStyle"
      @click.stop
    >
      <div v-if="contextMenu.title" class="truncate border-b border-ink/10 px-2 py-1.5 font-serif text-[11px] text-ink/55">
        {{ contextMenu.title }}
      </div>
      <button
        v-for="action in contextActions"
        :key="action"
        class="flex w-full items-center justify-between gap-4 rounded-[3px] px-2 py-1.5 text-left font-sans text-[10px] text-ink/80 hover:bg-blue-soft hover:text-blue"
        :class="action.endsWith('.delete') ? 'text-alert hover:text-alert' : ''"
        @click="handleContextAction(action)"
      >
        <span>{{ translate(`contextMenu.${action}`, actionLabels[action]) }}</span>
        <kbd v-if="props.shortcuts[action]" class="font-sans text-[8px] text-ink/35">{{ props.shortcuts[action] }}</kbd>
      </button>
      <template v-for="action in pluginActions" :key="action.id">
        <div class="my-1 border-t border-ink/10" />
        <button class="flex w-full items-center rounded-[3px] px-2 py-1.5 text-left font-sans text-[10px] text-ink/80 hover:bg-blue-soft hover:text-blue" @click="handlePluginAction(action)">
          <span>{{ action.label }}</span>
        </button>
      </template>
    </div>

    <div
      v-if="pieMenu"
      class="zen-radial-menu pointer-events-auto absolute z-30 size-44 -translate-x-1/2 -translate-y-1/2 rounded-full border border-ink/25 bg-paper/96 shadow-xl"
      :class="pieMenu.gestureActive ? 'is-gesture-active' : ''"
      :style="{ left: `${pieMenu.screenX}px`, top: `${pieMenu.screenY}px` }"
      @click.stop
    >
      <button
        v-for="item in props.radialMenu.items"
        :key="item.id"
        class="absolute grid size-12 -translate-x-1/2 -translate-y-1/2 place-items-center rounded-full border border-ink/15 bg-canvas px-1 text-center font-sans text-[8px] leading-tight text-ink/80 hover:border-blue hover:bg-blue-soft hover:text-blue"
        :style="{
          left: `${88 + Math.cos(['north','north-east','east','south-east','south','south-west','west','north-west'].indexOf(item.position) * Math.PI / 4 - Math.PI / 2) * 55}px`,
          top: `${88 + Math.sin(['north','north-east','east','south-east','south','south-west','west','north-west'].indexOf(item.position) * Math.PI / 4 - Math.PI / 2) * 55}px`,
        }"
        @click="chooseRadialItem(item)"
      >
        {{ item.action.replace('create:', '').replace('canvas:', '') }}
      </button>
      <button class="absolute left-1/2 top-1/2 grid size-10 -translate-x-1/2 -translate-y-1/2 place-items-center rounded-full border border-ink/20 bg-paper font-serif text-[10px] text-ink/55 hover:text-blue" @click="closeRadialMenu">
        ×
      </button>
    </div>
  </div>
</template>

<style scoped>
.research-graph-canvas {
  isolation: isolate;
}
</style>
