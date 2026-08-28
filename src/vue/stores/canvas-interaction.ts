import { computed, ref } from "vue";
import { defineStore } from "pinia";
import type {
  PieMenuState,
  WorkspaceContextMenuState,
} from "../canvas/canvas-types";
import type { GraphViewport } from "../canvas/vue-flow-adapter";

type ManualNodeMove = {
  nodeId: string;
  x: number;
  y: number;
};

export const useCanvasInteractionStore = defineStore("canvas-interaction", () => {
  // Keep only plain serializable interaction data here. Vue Flow instances,
  // elements, callbacks, and DOM nodes remain local to the canvas component.
  const viewport = ref<GraphViewport>({ x: 0, y: 0, zoom: 1 });
  const contextMenu = ref<WorkspaceContextMenuState | null>(null);
  const radialMenu = ref<PieMenuState | null>(null);
  const expandedNodeIds = ref<string[]>([]);
  const draggingNodeId = ref<string | null>(null);
  const manualMove = ref<ManualNodeMove | null>(null);
  const lastTrackpadFrameId = ref<number | null>(null);
  const selectedNodeIds = ref<string[]>([]);
  const selectedEdgeIds = ref<string[]>([]);
  const selectionMode = ref(false);

  const selectedElementCount = computed(
    () => selectedNodeIds.value.length + selectedEdgeIds.value.length,
  );

  const interactionMode = computed<"idle" | "context-menu" | "radial-menu" | "dragging" | "selecting">(() => {
    if (draggingNodeId.value) return "dragging";
    if (radialMenu.value) return "radial-menu";
    if (contextMenu.value) return "context-menu";
    if (selectionMode.value) return "selecting";
    return "idle";
  });

  function setViewport(nextViewport: GraphViewport) {
    viewport.value = { ...nextViewport };
  }

  function openContextMenu(menu: WorkspaceContextMenuState) {
    contextMenu.value = { ...menu };
    radialMenu.value = null;
  }

  function closeContextMenu() {
    contextMenu.value = null;
  }

  function openRadialMenu(menu: PieMenuState) {
    radialMenu.value = { ...menu };
    contextMenu.value = null;
  }

  function closeRadialMenu() {
    radialMenu.value = null;
  }

  function clearTransientMenus() {
    contextMenu.value = null;
    radialMenu.value = null;
  }

  function toggleExpanded(nodeId: string) {
    expandedNodeIds.value = expandedNodeIds.value.includes(nodeId)
      ? expandedNodeIds.value.filter((id) => id !== nodeId)
      : [...expandedNodeIds.value, nodeId];
  }

  function setExpandedNodeIds(nodeIds: Iterable<string>) {
    expandedNodeIds.value = [...new Set(nodeIds)];
  }

  function setDraggingNode(nodeId: string | null) {
    draggingNodeId.value = nodeId;
  }

  function setManualMove(move: ManualNodeMove | null) {
    manualMove.value = move ? { ...move } : null;
  }

  function setLastTrackpadFrameId(frameId: number | null) {
    lastTrackpadFrameId.value = frameId;
  }

  function setSelectedElements(
    nodeIds: Iterable<string>,
    edgeIds: Iterable<string>,
  ) {
    selectedNodeIds.value = [...new Set([...nodeIds].filter(Boolean))];
    selectedEdgeIds.value = [...new Set([...edgeIds].filter(Boolean))];
  }

  function clearSelectedElements() {
    selectedNodeIds.value = [];
    selectedEdgeIds.value = [];
  }

  function setSelectionMode(active: boolean) {
    selectionMode.value = active;
  }

  return {
    viewport,
    contextMenu,
    radialMenu,
    expandedNodeIds,
    draggingNodeId,
    manualMove,
    lastTrackpadFrameId,
    selectedNodeIds,
    selectedEdgeIds,
    selectedElementCount,
    selectionMode,
    interactionMode,
    setViewport,
    openContextMenu,
    closeContextMenu,
    openRadialMenu,
    closeRadialMenu,
    clearTransientMenus,
    toggleExpanded,
    setExpandedNodeIds,
    setDraggingNode,
    setManualMove,
    setLastTrackpadFrameId,
    setSelectedElements,
    clearSelectedElements,
    setSelectionMode,
  };
});
