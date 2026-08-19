<script setup lang="ts">
import {
  computed,
  markRaw,
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
  SelectionMode,
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
import RadialAddMenu from "../components/RadialAddMenu.vue";
import {
  setCursorLongPress,
  setCursorSelectionMode,
} from "../components/CustomCursor.vue";
import type {
  CanvasNodeMove,
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
import {
  chromiumTrackpadPinchScale,
  emptyTrackpadLowPassState,
  lowPassCompleteTrackpadFrame,
  viewportForCoalescedWheelFrame,
  viewportForCompleteTrackpadFrame,
  wheelPanDelta,
  type TrackpadLowPassState,
} from "../../../app/features/research-workspace/hooks/trackpad-pinch";
import {
  compileRadialMenu,
  radialSelectionForNormalizedDisplacement,
} from "../../../app/features/research-workspace/workspace-radial-menu";
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
  selectedNodeIds,
  selectedEdgeIds,
  selectedElementCount,
  selectionMode,
  interactionMode,
} = storeToRefs(canvasStore);

const nodeTypes = { researchNode: markRaw(ResearchNodeCard) };
const edgeTypes = { researchEdge: markRaw(ResearchEdgeLine) };
const fitViewOptions = { padding: 0.12, maxZoom: 1 };
const minZoom = 0.45;
const maxZoom = 1.7;

const wrapperRef = ref<HTMLElement | null>(null);
const flowRef = shallowRef<VueFlowViewportApi | null>(null);
const radialMenuRef = shallowRef<{ updateGesture: (sector: number | null, active: boolean) => void } | null>(null);
const nodes = ref<ResearchVueFlowNode[]>([]);
const edges = ref<ResearchVueFlowEdge[]>([]);
const canvasSize = ref({ width: 0, height: 0 });
const resizeObserver = shallowRef<ResizeObserver | null>(null);
const radialMenuCache = computed(() => compileRadialMenu(props.radialMenu));
const selectionContextMenu = ref<{
  screenX: number;
  screenY: number;
  count: number;
} | null>(null);
const hasMultipleSelectedElements = computed(() => selectedElementCount.value > 1);

type NativeRadialGesture = {
  originCenterX: number;
  originCenterY: number;
  inverseDeviceWidth: number;
  inverseDeviceHeight: number;
  selectedSector: number | null;
  selectedItem: RadialMenuItem | null;
  flowX: number;
  flowY: number;
};

const nativeTrackpad = {
  latestFrame: null as CanvasTrackpadFrame | null,
  originViewport: null as GraphViewport | null,
  anchor: null as { x: number; y: number } | null,
  bounds: null as DOMRect | null,
  animationFrame: null as number | null,
  ending: false,
  filterState: emptyTrackpadLowPassState() as TrackpadLowPassState,
  radial: null as NativeRadialGesture | null,
};
const chromiumWheel = {
  panX: 0,
  panY: 0,
  scale: 1,
  cursor: null as { x: number; y: number } | null,
  animationFrame: null as number | null,
};
let nativeGestureActive = false;

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
  T: "Transform",
  K: "Kernel",
  I: "Intervention",
  M: "Marginalize",
  Q: "Quotient",
};

function translate(key: string, fallback: string): string {
  return props.translate?.(key) || fallback;
}

function nodeTypeLabel(type: ResearchNodeType): string {
  return translate(`node.${type === "concept" ? "group" : type === "dataset" ? "data" : type}`, defaultNodeLabels[type] ?? type);
}

const edgeTypeI18nKey: Record<ResearchEdgeType, string> = {
  T: "transform",
  K: "kernel",
  I: "intervention",
  M: "marginalize",
  Q: "quotient",
};

function edgeTypeLabel(type: ResearchEdgeType): string {
  return translate(`edgeType.${edgeTypeI18nKey[type]}`, edgeLabelFallbacks[type]);
}

function selectNode(nodeId: string) {
  props.onSelectNode?.(nodeId);
  emit("select-node", nodeId);
}

function selectEdge(edgeId: string) {
  props.onSelectEdge?.(edgeId);
  emit("select-edge", edgeId);
}

function clearProjectSelection() {
  props.onClearSelection?.();
  emit("clear-selection");
}

function setSelectedElements(nodeIds: Iterable<string>, edgeIds: Iterable<string>) {
  canvasStore.setSelectedElements(nodeIds, edgeIds);
}

function clearSelectedElements() {
  canvasStore.clearSelectedElements();
  clearProjectSelection();
}

function syncSelectedElementsFromGraph() {
  const nextNodeIds = nodes.value
    .filter((node) => node.selected && node.selectable !== false)
    .map((node) => node.id);
  const nextEdgeIds = edges.value
    .filter((edge) => edge.selected && edge.selectable !== false)
    .map((edge) => edge.id);
  setSelectedElements(nextNodeIds, nextEdgeIds);

  if (nextNodeIds.length) {
    selectNode(nextNodeIds[0]);
  } else if (nextEdgeIds.length) {
    selectEdge(nextEdgeIds[0]);
  } else {
    clearProjectSelection();
  }
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
  selectionContextMenu.value = null;
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
      selectedNodeIds: new Set(selectedNodeIds.value),
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
      selectedEdgeIds: new Set(selectedEdgeIds.value),
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
    selectedNodeIds.value,
    selectedEdgeIds.value,
  ],
  rebuildGraph,
  { immediate: true, deep: true },
);

watch(
  selectionMode,
  (active) => {
    setCursorSelectionMode(active);
    if (!active) selectionContextMenu.value = null;
  },
  { immediate: true },
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

function setBoxSelectionMode(active: boolean) {
  canvasStore.setSelectionMode(active);
  emit("selection-mode-change", active);
}

function isCanvasSurface(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  if (!target.closest(".vue-flow")) return false;
  return !target.closest(
    ".vue-flow__node, .vue-flow__edge, button, input, textarea, select, [role='menuitem']",
  );
}

function handleCanvasDoubleClick(event: MouseEvent) {
  if (props.canvasInputBlocked || props.connectMode || !isCanvasSurface(event.target)) return;
  event.preventDefault();
  clearTransientMenus();
  setBoxSelectionMode(!selectionMode.value);
}

function openSelectionContextMenu(event: MouseEvent) {
  if (!hasMultipleSelectedElements.value) return;
  event.preventDefault();
  event.stopPropagation();
  const point = pointerToFlow(event);
  closeRadialMenu();
  closeContextMenu();
  selectionContextMenu.value = {
    screenX: point.screen.x,
    screenY: point.screen.y,
    count: selectedElementCount.value,
  };
}

function handleSelectionContextAction(action: "copy" | "delete" | "clear") {
  selectionContextMenu.value = null;
  if (action === "copy") {
    props.onSelectionCopy?.();
    emit("selection-copy");
    return;
  }
  if (action === "delete") {
    props.onSelectionDelete?.();
    emit("selection-delete");
    return;
  }
  clearSelectedElements();
}

function researchNodesOf(nodesToResolve: GraphNode[]): ResearchVueFlowNode[] {
  return nodesToResolve
    .map(researchNodeOf)
    .filter((node): node is ResearchVueFlowNode => node !== null);
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

function setIncidentEdgePreviewForNodes(nodeIds: readonly string[], enabled: boolean) {
  nodeIds.forEach((nodeId) => setIncidentEdgePreview(nodeId, enabled));
}

function isMultiSelectionEvent(event: MouseEvent | TouchEvent): boolean {
  return "ctrlKey" in event && (event.ctrlKey || event.metaKey);
}

function handleNodeClick(payload: NodeMouseEvent) {
  const node = researchNodeOf(payload.node);
  if (!node) return;
  if (isMultiSelectionEvent(payload.event)) {
    void nextTick(syncSelectedElementsFromGraph);
  } else {
    setSelectedElements([node.id], []);
    selectNode(node.id);
  }
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
  if (
    event instanceof MouseEvent &&
    hasMultipleSelectedElements.value &&
    selectedNodeIds.value.includes(node.id)
  ) {
    openSelectionContextMenu(event);
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  const point = pointerToFlow(event);
  setSelectedElements([node.id], []);
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
  if (isMultiSelectionEvent(payload.event)) {
    void nextTick(syncSelectedElementsFromGraph);
  } else {
    setSelectedElements([], [edge.id]);
    selectEdge(edge.id);
  }
  clearTransientMenus();
}

function handleEdgeContextMenu(payload: EdgeMouseEvent) {
  const edge = researchEdgeOf(payload.edge);
  if (!edge) return;
  const event = payload.event;
  if (
    event instanceof MouseEvent &&
    hasMultipleSelectedElements.value &&
    selectedEdgeIds.value.includes(edge.id)
  ) {
    openSelectionContextMenu(event);
    return;
  }
  event.preventDefault();
  event.stopPropagation();
  const point = pointerToFlow(event);
  setSelectedElements([], [edge.id]);
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
  const draggedNodes = researchNodesOf(payload.nodes);
  if (!draggedNodes.length) return;
  const draggedNodeIds = draggedNodes.map((node) => node.id);
  setSelectedElements(draggedNodeIds, selectedEdgeIds.value);
  canvasStore.setDraggingNode(draggedNodeIds[0]);
  setIncidentEdgePreviewForNodes(draggedNodeIds, true);
}

function handleNodeDragStop(payload: NodeDragEvent) {
  const draggedNodes = researchNodesOf(payload.nodes);
  if (!draggedNodes.length) return;
  const moves: CanvasNodeMove[] = draggedNodes.map((node) => ({
    nodeId: node.id,
    x: node.position.x,
    y: node.position.y,
  }));
  setIncidentEdgePreviewForNodes(moves.map((move) => move.nodeId), false);
  canvasStore.setDraggingNode(null);
  canvasStore.setManualMove(moves[0]);
  if (moves.length === 1) {
    moveNode(moves[0].nodeId, moves[0].x, moves[0].y);
    return;
  }
  props.onMoveNodes?.(moves);
  emit("move-nodes", moves);
}

function handlePaneClick() {
  clearTransientMenus();
  clearSelectedElements();
}

function handlePaneContextMenu(event: MouseEvent) {
  event.preventDefault();
  if (hasMultipleSelectedElements.value) {
    openSelectionContextMenu(event);
    return;
  }
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

function handleSelectionContextMenu(payload: { event: MouseEvent; nodes: GraphNode[] }) {
  const nodeIds = researchNodesOf(payload.nodes).map((node) => node.id);
  if (nodeIds.length) setSelectedElements(nodeIds, selectedEdgeIds.value);
  openSelectionContextMenu(payload.event);
}

function handleNodesChange(changes: NodeChange[]) {
  // `remove` changes are prop-diff artifacts (e.g. the legend filter projecting
  // out nodes). Actual deletion flows through the context menu → store commit →
  // rebuild, never through Vue Flow's `remove` change, so we must not forward it
  // as a delete (that would erase filtered-out nodes from the project).
  const nonRemove = changes.filter((change) => change.type !== "remove");
  if (nonRemove.length) nodes.value = applyNodeChangesCompat(nonRemove, nodes.value);
  if (changes.some((change) => change.type === "select")) {
    syncSelectedElementsFromGraph();
  }
}

function handleEdgesChange(changes: EdgeChange[]) {
  const nonRemove = changes.filter((change) => change.type !== "remove");
  if (nonRemove.length) edges.value = applyEdgeChangesCompat(nonRemove, edges.value);
  if (changes.some((change) => change.type === "select")) {
    syncSelectedElementsFromGraph();
  }
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

function resetNativeTrackpad() {
  nativeTrackpad.latestFrame = null;
  nativeTrackpad.originViewport = null;
  nativeTrackpad.anchor = null;
  nativeTrackpad.bounds = null;
  nativeTrackpad.animationFrame = null;
  nativeTrackpad.ending = false;
  nativeTrackpad.filterState = emptyTrackpadLowPassState();
  nativeTrackpad.radial = null;
  nativeGestureActive = false;
}

function applyRadialFrame(frame: CanvasTrackpadFrame) {
  const radial = nativeTrackpad.radial;
  if (!radial) return;
  const selection = radialSelectionForNormalizedDisplacement(
    radialMenuCache.value,
    (frame.centerX - radial.originCenterX) * radial.inverseDeviceWidth,
    (frame.centerY - radial.originCenterY) * radial.inverseDeviceHeight,
  );
  const selectedSector = selection?.sectorIndex ?? null;
  const selectedItem = selection?.item ?? null;
  if (radial.selectedSector === selectedSector && radial.selectedItem?.id === selectedItem?.id) return;
  radial.selectedSector = selectedSector;
  radial.selectedItem = selectedItem;
  radialMenuRef.value?.updateGesture(selectedSector, true);
}

function applyLatestNativeTrackpadFrame() {
  nativeTrackpad.animationFrame = null;
  const frame = nativeTrackpad.latestFrame;
  const originViewport = nativeTrackpad.originViewport;
  const anchor = nativeTrackpad.anchor;
  const bounds = nativeTrackpad.bounds;
  nativeTrackpad.latestFrame = null;

  if (frame && nativeTrackpad.radial) {
    applyRadialFrame(frame);
  } else if (frame && originViewport && anchor && bounds) {
    const filtered = lowPassCompleteTrackpadFrame(
      nativeTrackpad.filterState,
      { x: frame.panX, y: frame.panY },
      frame.scale,
      props.trackpadSensitivity,
      props.trackpadFilterStrength,
    );
    nativeTrackpad.filterState = filtered.state;
    const viewport = viewportForCompleteTrackpadFrame(
      originViewport,
      anchor,
      filtered.pan,
      { width: bounds.width, height: bounds.height },
      filtered.scale,
      minZoom,
      maxZoom,
    );
    setViewport(viewport);
  }

  if (nativeTrackpad.ending) resetNativeTrackpad();
}

function scheduleNativeTrackpadFrame() {
  if (nativeTrackpad.animationFrame !== null) return;
  nativeTrackpad.animationFrame = window.requestAnimationFrame(applyLatestNativeTrackpadFrame);
}

function handleTrackpadFrame(frame: CanvasTrackpadFrame) {
  if (props.canvasInputBlocked) return;
  if (lastTrackpadFrameId.value === frame.frameId && frame.phase !== "end") return;
  canvasStore.setLastTrackpadFrameId(frame.frameId);
  props.onTrackpadFrame?.(frame);
  emit("trackpad-frame", frame);

  if (frame.phase === "start") {
    nativeTrackpad.bounds = wrapperRef.value?.getBoundingClientRect() ?? null;
  }
  const bounds = nativeTrackpad.bounds;
  if (!bounds) return;
  const insideCanvas = frame.cursorX >= bounds.left
    && frame.cursorX <= bounds.right
    && frame.cursorY >= bounds.top
    && frame.cursorY <= bounds.bottom;
  const pointerTarget = frame.phase === "start"
    ? document.elementFromPoint(frame.cursorX, frame.cursorY)
    : null;
  const canvasSurfaceOwnsPointer = frame.phase === "start"
    ? Boolean(pointerTarget && wrapperRef.value?.contains(pointerTarget) && pointerTarget.closest(".vue-flow"))
    : insideCanvas;
  if (frame.phase === "start" && !canvasSurfaceOwnsPointer) return;
  if (frame.phase !== "start" && !nativeTrackpad.originViewport) return;

  if (frame.phase === "start") {
    nativeGestureActive = true;
    nativeTrackpad.originViewport = getViewport();
    nativeTrackpad.anchor = {
      x: frame.cursorX - bounds.left,
      y: frame.cursorY - bounds.top,
    };
    nativeTrackpad.ending = false;
    nativeTrackpad.filterState = emptyTrackpadLowPassState();
    nativeTrackpad.radial = null;
  }

  if (frame.phase === "end") {
    if (nativeTrackpad.radial) {
      if (nativeTrackpad.animationFrame !== null) {
        window.cancelAnimationFrame(nativeTrackpad.animationFrame);
        nativeTrackpad.animationFrame = null;
      }
      if (nativeTrackpad.latestFrame) applyRadialFrame(nativeTrackpad.latestFrame);
      const selectedItem = nativeTrackpad.radial.selectedItem;
      radialMenuRef.value?.updateGesture(null, false);
      setCursorLongPress(false);
      if (selectedItem) chooseRadialItem(selectedItem);
      else closeRadialMenu();
      resetNativeTrackpad();
      return;
    }
    nativeTrackpad.ending = true;
    if (nativeTrackpad.latestFrame) scheduleNativeTrackpadFrame();
    else resetNativeTrackpad();
    return;
  }

  if (nativeTrackpad.radial) {
    if (!insideCanvas) {
      if (nativeTrackpad.animationFrame !== null) {
        window.cancelAnimationFrame(nativeTrackpad.animationFrame);
      }
      radialMenuRef.value?.updateGesture(null, false);
      setCursorLongPress(false);
      closeRadialMenu();
      resetNativeTrackpad();
      return;
    }
    nativeTrackpad.latestFrame = frame;
    scheduleNativeTrackpadFrame();
    return;
  }

  if (!canvasSurfaceOwnsPointer) {
    if (nativeTrackpad.animationFrame !== null) {
      window.cancelAnimationFrame(nativeTrackpad.animationFrame);
    }
    resetNativeTrackpad();
    return;
  }

  if (frame.held) {
    if (nativeTrackpad.animationFrame !== null) {
      window.cancelAnimationFrame(nativeTrackpad.animationFrame);
      nativeTrackpad.animationFrame = null;
    }
    nativeTrackpad.latestFrame = null;
    const point = screenToFlow(frame.cursorX, frame.cursorY);
    nativeTrackpad.radial = {
      originCenterX: frame.centerX,
      originCenterY: frame.centerY,
      inverseDeviceWidth: 1 / Math.max(frame.deviceWidth, 1),
      inverseDeviceHeight: 1 / Math.max(frame.deviceHeight, 1),
      selectedSector: null,
      selectedItem: null,
      flowX: point.flow.x,
      flowY: point.flow.y,
    };
    closeContextMenu();
    openRadialMenu({
      screenX: point.screen.x,
      screenY: point.screen.y,
      flowX: point.flow.x,
      flowY: point.flow.y,
      gestureActive: true,
    });
    setCursorLongPress(true);
    return;
  }

  nativeTrackpad.latestFrame = frame;
  scheduleNativeTrackpadFrame();
}

function applyChromiumWheel() {
  chromiumWheel.animationFrame = null;
  const cursor = chromiumWheel.cursor;
  const wheelDeadZone = 0.25 + props.trackpadFilterStrength * 1.75;
  const tuneAxis = (value: number) => Math.abs(value) <= wheelDeadZone
    ? 0
    : Math.sign(value) * (Math.abs(value) - wheelDeadZone) * props.trackpadSensitivity;
  const panDelta = {
    x: tuneAxis(chromiumWheel.panX),
    y: tuneAxis(chromiumWheel.panY),
  };
  const rawScaleLog = Math.log(Math.max(chromiumWheel.scale, 0.01));
  const scaleDeadZone = 0.0015 + props.trackpadFilterStrength * 0.008;
  const scale = Math.abs(rawScaleLog) <= scaleDeadZone
    ? 1
    : Math.exp(Math.sign(rawScaleLog) * (Math.abs(rawScaleLog) - scaleDeadZone) * props.trackpadSensitivity);
  chromiumWheel.panX = 0;
  chromiumWheel.panY = 0;
  chromiumWheel.scale = 1;
  chromiumWheel.cursor = null;
  if (!cursor || (panDelta.x === 0 && panDelta.y === 0 && scale === 1)) return;
  setViewport(viewportForCoalescedWheelFrame(getViewport(), cursor, panDelta, scale, minZoom, maxZoom));
}

function handleChromiumWheel(event: WheelEvent) {
  if (nativeGestureActive || props.canvasInputBlocked) {
    event.preventDefault();
    event.stopImmediatePropagation();
    return;
  }
  event.preventDefault();
  event.stopImmediatePropagation();
  const bounds = nativeTrackpad.bounds ?? wrapperRef.value?.getBoundingClientRect();
  if (!bounds) return;
  chromiumWheel.cursor = {
    x: event.clientX - bounds.left,
    y: event.clientY - bounds.top,
  };
  if (event.ctrlKey) {
    chromiumWheel.scale *= chromiumTrackpadPinchScale(event.deltaY);
  } else {
    const pan = wheelPanDelta(event.deltaX, event.deltaY, event.deltaMode);
    chromiumWheel.panX += pan.x;
    chromiumWheel.panY += pan.y;
  }
  if (chromiumWheel.animationFrame === null) {
    chromiumWheel.animationFrame = window.requestAnimationFrame(applyChromiumWheel);
  }
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

const selectionContextMenuStyle = computed(() => {
  const menu = selectionContextMenu.value;
  if (!menu) return {};
  return {
    left: `${Math.min(Math.max(menu.screenX, 6), Math.max(canvasSize.value.width - 206, 6))}px`,
    top: `${Math.min(Math.max(menu.screenY, 6), Math.max(canvasSize.value.height - 184, 6))}px`,
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
  { key: "T" as const, label: translate("edgeType.transform", "Transform"), lineClass: "border-ink/70" },
  { key: "K" as const, label: translate("edgeType.kernel", "Kernel"), lineClass: "border-ink/70" },
  { key: "I" as const, label: translate("edgeType.intervention", "Intervention"), lineClass: "border-dashed border-ink/70" },
  { key: "M" as const, label: translate("edgeType.marginalize", "Marginalize"), lineClass: "border-dotted border-ink/70" },
  { key: "Q" as const, label: translate("edgeType.quotient", "Quotient"), lineClass: "border-ink/40" },
];

const legendCounts = computed(() => {
  const counts: Record<ResearchEdgeType, number> = { T: 0, K: 0, I: 0, M: 0, Q: 0 };
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

const canvasClass = computed(
  () =>
    `zen-flow h-full w-full ${props.connectMode ? "is-connecting" : ""} ${selectionMode.value ? "is-selection-mode" : ""}`,
);

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
  if (isTauriRuntime.value) {
    element.addEventListener("wheel", handleChromiumWheel, { capture: true, passive: false });
  }
});

watch(
  () => props.trackpadFrame,
  (frame) => {
    if (frame) handleTrackpadFrame(frame);
  },
  { deep: true },
);

watch(
  () => props.canvasInputBlocked,
  (blocked) => {
    if (!blocked) return;
    if (nativeTrackpad.animationFrame !== null) {
      window.cancelAnimationFrame(nativeTrackpad.animationFrame);
    }
    if (chromiumWheel.animationFrame !== null) {
      window.cancelAnimationFrame(chromiumWheel.animationFrame);
      chromiumWheel.animationFrame = null;
    }
    resetNativeTrackpad();
    chromiumWheel.panX = 0;
    chromiumWheel.panY = 0;
    chromiumWheel.scale = 1;
    chromiumWheel.cursor = null;
  },
);

onBeforeUnmount(() => {
  resizeObserver.value?.disconnect();
  wrapperRef.value?.removeEventListener("wheel", handleChromiumWheel, { capture: true });
  if (nativeTrackpad.animationFrame !== null) window.cancelAnimationFrame(nativeTrackpad.animationFrame);
  if (chromiumWheel.animationFrame !== null) window.cancelAnimationFrame(chromiumWheel.animationFrame);
  setCursorLongPress(false);
  setCursorSelectionMode(false);
  resetNativeTrackpad();
  chromiumWheel.panX = 0;
  chromiumWheel.panY = 0;
  chromiumWheel.scale = 1;
  chromiumWheel.cursor = null;
  chromiumWheel.animationFrame = null;
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
    @dblclick.capture="handleCanvasDoubleClick"
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
      :pan-on-drag="selectionMode ? false : true"
      :selection-key-code="selectionMode ? true : undefined"
      :selection-mode="SelectionMode.Partial"
      :zoom-on-double-click="false"
      :delete-key-code="null"
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
      @selection-context-menu="handleSelectionContextMenu"
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

    <div
      v-if="selectionMode"
      class="pointer-events-none absolute left-1/2 top-4 z-20 -translate-x-1/2 rounded-full border border-blue/35 bg-paper/95 px-4 py-2 font-serif text-[11px] text-blue shadow-sm"
      data-testid="box-selection-hint"
      role="status"
      aria-live="polite"
    >
      ✣ {{ translate('workspace.selectionModeHint', 'Box select · drag an empty area to select · Esc to exit') }}
    </div>

    <div class="absolute bottom-5 right-5 z-10 w-[154px] rounded-[5px] border border-ink/30 bg-paper/96 p-2 font-serif text-[10px] text-ink/80">
      <div class="flex items-center justify-between px-2 pb-1.5 font-sans text-[7px] uppercase tracking-[0.14em] text-ink/40">
        <span>{{ translate('workspace.linkFilter', 'Link filter') }}</span>
        <span v-if="props.showLinkCounts">{{ props.linkFilter ? legendCounts[props.linkFilter] : props.project.edges.length }}</span>
      </div>
      <button
        v-for="item in legendItems"
        :key="item.key"
        class="flex w-full items-center gap-3 rounded-[3px] px-2 py-1.5 text-left transition hover:bg-ink/5"
        :class="[props.linkFilter === item.key ? 'bg-blue-soft ring-1 ring-inset ring-blue/25' : '']"
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
      v-if="selectionContextMenu"
      class="zen-selection-context-menu absolute z-40 min-w-[190px] rounded-[4px] border border-ink/25 bg-paper p-1.5 shadow-lg"
      :style="selectionContextMenuStyle"
      @click.stop
    >
      <div class="border-b border-ink/10 px-2 py-1.5 font-serif text-[11px] text-ink/55">
        {{ translate('contextMenu.selectionTitle', 'Selected') }} · {{ selectionContextMenu.count }}
      </div>
      <button
        class="flex w-full items-center justify-between gap-4 rounded-[3px] px-2 py-1.5 text-left font-sans text-[10px] text-ink/80 hover:bg-blue-soft hover:text-blue"
        @click="handleSelectionContextAction('copy')"
      >
        <span>{{ translate('contextMenu.copySelection', 'Copy selection') }}</span>
        <kbd class="font-sans text-[8px] text-ink/35">Ctrl+C</kbd>
      </button>
      <button
        class="flex w-full items-center justify-between gap-4 rounded-[3px] px-2 py-1.5 text-left font-sans text-[10px] text-alert hover:bg-alert/5"
        @click="handleSelectionContextAction('delete')"
      >
        <span>{{ translate('contextMenu.deleteSelection', 'Delete selection') }}</span>
        <kbd class="font-sans text-[8px] text-ink/35">Del</kbd>
      </button>
      <button
        class="flex w-full items-center justify-between gap-4 rounded-[3px] px-2 py-1.5 text-left font-sans text-[10px] text-ink/70 hover:bg-blue-soft hover:text-blue"
        @click="handleSelectionContextAction('clear')"
      >
        <span>{{ translate('contextMenu.clearSelection', 'Clear selection') }}</span>
        <kbd class="font-sans text-[8px] text-ink/35">Esc</kbd>
      </button>
    </div>

    <RadialAddMenu
      v-if="pieMenu"
      ref="radialMenuRef"
      :menu="pieMenu"
      :cache="radialMenuCache"
      @choose="chooseRadialItem"
      @close="closeRadialMenu"
    />
  </div>
</template>

<style scoped>
.research-graph-canvas {
  isolation: isolate;
}
</style>
