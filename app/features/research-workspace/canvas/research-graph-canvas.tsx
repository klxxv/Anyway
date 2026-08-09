"use client";

import {
  Background,
  BackgroundVariant,
  ConnectionMode,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  addEdge,
  applyEdgeChanges,
  applyNodeChanges,
  type Connection,
  type EdgeChange,
  type MiniMapNodeProps,
  type NodeChange,
  type ReactFlowProps,
  type ReactFlowInstance,
} from "@xyflow/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import type { MessageKey } from "../../../i18n/catalog";
import { useI18n } from "../../../i18n/provider";
import type { DiffOverlayState } from "../../../lib/graph/canvas-diff";
import type {
  EdgeStyleManifest,
  ProjectState,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../../lib/research-types";
import type { ResolvedPluginContextMenuAction } from "../../../plugins/context-menu";
import {
  listenForNativeTrackpadFrames,
  type NativeTrackpadFrame,
} from "../../../platform/trackpad";
import {
  chromiumTrackpadPinchScale,
  emptyTrackpadLowPassState,
  lowPassCompleteTrackpadFrame,
  viewportForCoalescedWheelFrame,
  viewportForCompleteTrackpadFrame,
  wheelPanDelta,
  type GesturePoint,
  type GestureViewport,
  type TrackpadLowPassState,
} from "../hooks/trackpad-pinch";
import type { PieMenuState, WorkspaceEdge, WorkspaceNode } from "../workspace-types";
import {
  type ContextMenuActionId,
  type ContextMenuPreferences,
  type WorkspaceContextMenuState,
} from "../workspace-context-menu";
import type { WorkspaceShortcuts } from "../workspace-shortcuts";
import { customEdgeNote, edgeTypeMessageKeys } from "../workspace-edge-labels";
import {
  linkLegendFilterOf,
  projectForLegendFilter,
  type LinkLegendFilter,
} from "../workspace-layout";
import {
  RadialAddMenu,
  type RadialAddMenuHandle,
} from "../components/radial-add-menu";
import { WorkspaceContextMenu } from "../components/workspace-context-menu";
import {
  compileRadialMenu,
  nodeTypeForRadialAction,
  radialSelectionForNormalizedDisplacement,
  type RadialMenuCache,
  type RadialMenuItem,
  type RadialMenuPreferences,
} from "../workspace-radial-menu";
import { ResearchEdgeLine } from "./research-edge-line";
import { ResearchNodeCard } from "./research-node-card";
import { computeEdgeRoutes } from "./edge-routing";
import { isExpandableVariable, variableBranchValues } from "./variable-branches";
import { setCursorLongPress } from "../../../components/CustomCursor";

const nodeTypes = { researchNode: ResearchNodeCard };
const edgeTypes = { researchEdge: ResearchEdgeLine };
const fitViewOptions = { padding: 0.12, maxZoom: 1 } as const;
const proOptions = { hideAttribution: true } as const;
type WorkspaceReactFlowProps = ReactFlowProps<WorkspaceNode, WorkspaceEdge>;
const subscribeRuntime = () => () => undefined;
const readTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type ResearchGraphCanvasProps = {
  project: ProjectState;
  selectedNodeId: string;
  selectedEdgeId: string;
  addRequest: number;
  connectMode: boolean;
  connectType: ResearchEdgeType;
  inspectorOpen: boolean;
  linkFilter: LinkLegendFilter | null;
  /** 逻辑链高亮（阶段 4）：nodeIds/edgeIds 命中路径 / Logic-chain highlight (phase 4). */
  highlightChain?: { nodeIds: string[]; edgeIds: string[] } | null;
  showMiniMap: boolean;
  showMiniMapRelations: boolean;
  showLinkCounts: boolean;
  trackpadSensitivity: number;
  trackpadFilterStrength: number;
  edgeStyle: EdgeStyleManifest;
  referenceViewport: boolean;
  contextMenus: ContextMenuPreferences;
  radialMenu: RadialMenuPreferences;
  shortcuts: WorkspaceShortcuts;
  pluginContextMenuActions: ResolvedPluginContextMenuAction[];
  /** Canvas Diff 叠加状态：三色标记 + 幽灵节点/边（null 表示不叠加）。 */
  diffOverlay?: DiffOverlayState | null;
  /** 点击 diff 条目后的定位请求；nonce 保证重复点击同一实体仍触发。 */
  diffFocus?: { id: string; kind: "node" | "edge"; nonce: number } | null;
  onLegendFilter: (filter: LinkLegendFilter | null) => void;
  onSelectNode: (nodeId: string) => void;
  onSelectEdge: (edgeId: string) => void;
  onMoveNode: (nodeId: string, x: number, y: number) => void;
  onCreateEdge: (source: string, target: string) => void;
  onRequestCreate: (type: ResearchNodeType, x: number, y: number) => void;
  onRequestConnect: (nodeId: string) => void;
  onDuplicateNode: (nodeId: string) => void;
  onDeleteNode: (nodeId: string) => void;
  onReverseEdge: (edgeId: string) => void;
  onDeleteEdge: (edgeId: string) => void;
  onApplyDefaultLayout: () => void;
  onPluginContextMenuAction: (
    action: ResolvedPluginContextMenuAction,
    context: WorkspaceContextMenuState,
  ) => void;
};

function buildNodes(
  project: ProjectState,
  selectedNodeId: string,
  filter: LinkLegendFilter | null,
  expandedNodeIds: ReadonlySet<string>,
  onToggleExpanded: (nodeId: string) => void,
  highlightedNodeIds?: ReadonlySet<string>,
  diffOverlay?: DiffOverlayState | null,
): WorkspaceNode[] {
  const projected = projectForLegendFilter(project, filter);
  const nodes: WorkspaceNode[] = projected.nodes.map((record) => {
    const placement = projected.placements.find((item) => item.nodeId === record.id);
    const circle = record.data.shape === "circle" || record.type === "question";
    const expanded = !circle && expandedNodeIds.has(record.id) && isExpandableVariable(record);
    const branchCount = expanded ? variableBranchValues(record).length : 0;
    return {
      id: record.id,
      type: "researchNode",
      position: { x: placement?.x ?? 0, y: placement?.y ?? 0 },
      width: expanded ? Math.max(placement?.width ?? 0, 188) : placement?.width ?? (circle ? 136 : 164),
      height: expanded
        ? Math.max(placement?.height ?? 0, 132 + Math.max(branchCount, 2) * 25)
        : placement?.height ?? (circle ? 136 : 116),
      selected: record.id === selectedNodeId,
      data: {
        record,
        shape: circle ? "circle" : "card",
        expanded,
        onToggleExpanded,
        highlighted: highlightedNodeIds?.has(record.id),
        diffState: diffOverlay?.nodes[record.id],
      },
    };
  });
  // 幽灵节点：removed 实体从 base 版本注入，红色虚化且不可交互。
  if (diffOverlay) {
    for (const ghost of diffOverlay.removedNodes) {
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
          shape: ghost.record.data.shape === "circle" || ghost.record.type === "question" ? "circle" : "card",
          expanded: false,
          onToggleExpanded,
          diffState: "removed",
        },
      });
    }
  }
  return nodes;
}

function buildEdges(
  project: ProjectState,
  filter: LinkLegendFilter | null,
  selectedEdgeId: string,
  edgeTypeLabel: (type: ResearchEdgeType) => string,
  edgeStyle: EdgeStyleManifest,
  highlightedEdgeIds?: ReadonlySet<string>,
  diffOverlay?: DiffOverlayState | null,
): WorkspaceEdge[] {
  const projected = projectForLegendFilter(project, filter);
  const routes = computeEdgeRoutes(projected);
  const edges: WorkspaceEdge[] = projected.edges.map((record) => {
    const route = routes[record.id];
    return {
      id: record.id,
      source: record.source,
      target: record.target,
      sourceHandle: route?.sourceHandle,
      targetHandle: route?.targetHandle,
      type: "researchEdge",
      selected: record.id === selectedEdgeId,
      data: {
        record,
        label: customEdgeNote(record) || edgeTypeLabel(record.type),
        edgeStyle,
        labelOffsetX: route?.labelOffsetX,
        labelOffsetY: route?.labelOffsetY,
        highlighted: highlightedEdgeIds?.has(record.id),
        diffState: diffOverlay?.edges[record.id],
      },
    };
  });
  // 幽灵边：removed 关系从 base 版本注入（端点存在于 compare 或幽灵集合）。
  if (diffOverlay) {
    for (const ghost of diffOverlay.removedEdges) {
      edges.push({
        id: ghost.record.id,
        source: ghost.record.source,
        target: ghost.record.target,
        type: "researchEdge",
        selected: false,
        selectable: false,
        focusable: false,
        data: {
          record: ghost.record,
          label: customEdgeNote(ghost.record) || edgeTypeLabel(ghost.record.type),
          edgeStyle,
          diffState: "removed",
        },
      });
    }
  }
  return edges;
}

function ResearchGraphInner(props: ResearchGraphCanvasProps) {
  const { t } = useI18n();
  const {
    project,
    selectedNodeId,
    selectedEdgeId,
    addRequest,
    connectMode,
    connectType,
    inspectorOpen,
    linkFilter,
    highlightChain,
    showMiniMap,
    showMiniMapRelations,
    showLinkCounts,
    trackpadSensitivity,
    trackpadFilterStrength,
    edgeStyle,
    referenceViewport,
    contextMenus,
    radialMenu,
    shortcuts,
    pluginContextMenuActions,
    diffOverlay,
    diffFocus,
    onLegendFilter,
    onSelectNode,
    onSelectEdge,
    onMoveNode,
    onCreateEdge,
    onRequestCreate,
    onRequestConnect,
    onDuplicateNode,
    onDeleteNode,
    onReverseEdge,
    onDeleteEdge,
    onApplyDefaultLayout,
    onPluginContextMenuAction,
  } = props;
  const radialMenuCache = useMemo(() => compileRadialMenu(radialMenu), [radialMenu]);
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const canvasBoundsRef = useRef<DOMRect | null>(null);
  const flowRef = useRef<ReactFlowInstance<WorkspaceNode, WorkspaceEdge> | null>(null);
  const radialMenuRef = useRef<RadialAddMenuHandle | null>(null);
  const manualMoveRef = useRef<{ nodeId: string; x: number; y: number } | null>(null);
  const nativePinchRef = useRef<{
    latestFrame: NativeTrackpadFrame | null;
    originViewport: GestureViewport | null;
    anchor: GesturePoint | null;
    animationFrame: number | null;
    ending: boolean;
    filterState: TrackpadLowPassState;
    radial: {
      originCenterX: number;
      originCenterY: number;
      inverseDeviceWidth: number;
      inverseDeviceHeight: number;
      selectedSector: number | null;
      selectedItem: RadialMenuItem | null;
      flowX: number;
      flowY: number;
    } | null;
  }>({
    latestFrame: null,
    originViewport: null,
    anchor: null,
    animationFrame: null,
    ending: false,
    filterState: emptyTrackpadLowPassState(),
    radial: null,
  });
  const trackpadTuningRef = useRef({
    sensitivity: trackpadSensitivity,
    filterStrength: trackpadFilterStrength,
  });
  const radialMenuCacheRef = useRef<RadialMenuCache>(radialMenuCache);
  const runRadialActionRef = useRef<
    (item: RadialMenuItem, flowX: number, flowY: number) => void
  >(() => undefined);
  const nativeGestureActiveRef = useRef(false);
  const chromiumWheelRef = useRef<{
    panX: number;
    panY: number;
    scale: number;
    cursor: GesturePoint | null;
    animationFrame: number | null;
  }>({
    panX: 0,
    panY: 0,
    scale: 1,
    cursor: null,
    animationFrame: null,
  });
  const edgeTypeLabel = useCallback(
    (type: ResearchEdgeType) => t(edgeTypeMessageKeys[type]),
    [t],
  );
  const [expandedNodeIds, setExpandedNodeIds] = useState<Set<string>>(() => new Set());
  const toggleNodeExpanded = useCallback((nodeId: string) => {
    setExpandedNodeIds((current) => {
      const next = new Set(current);
      if (next.has(nodeId)) next.delete(nodeId);
      else next.add(nodeId);
      return next;
    });
  }, []);
  const highlightedNodeIds = useMemo(
    () => (highlightChain ? new Set(highlightChain.nodeIds) : undefined),
    [highlightChain],
  );
  const highlightedEdgeIds = useMemo(
    () => (highlightChain ? new Set(highlightChain.edgeIds) : undefined),
    [highlightChain],
  );
  const [nodes, setNodes] = useState(() =>
    buildNodes(project, selectedNodeId, linkFilter, expandedNodeIds, toggleNodeExpanded, highlightedNodeIds, diffOverlay),
  );
  const [edges, setEdges] = useState(() =>
    buildEdges(project, linkFilter, selectedEdgeId, edgeTypeLabel, edgeStyle, highlightedEdgeIds, diffOverlay),
  );
  const [canvasSize, setCanvasSize] = useState({ width: 0, height: 0 });
  const [contextMenu, setContextMenu] = useState<WorkspaceContextMenuState | null>(null);
  const [pieMenu, setPieMenu] = useState<PieMenuState | null>(null);
  const [draggingNodeId, setDraggingNodeId] = useState<string | null>(null);
  const isTauriRuntime = useSyncExternalStore(
    subscribeRuntime,
    readTauriRuntime,
    () => false,
  );
  useEffect(() => {
    radialMenuCacheRef.current = radialMenuCache;
  }, [radialMenuCache]);
  useEffect(() => {
    trackpadTuningRef.current = {
      sensitivity: trackpadSensitivity,
      filterStrength: trackpadFilterStrength,
    };
  }, [trackpadFilterStrength, trackpadSensitivity]);

  const applyWorkspaceViewport = useCallback(
    async (instance: ReactFlowInstance<WorkspaceNode, WorkspaceEdge>) => {
      const width = wrapperRef.current?.clientWidth ?? 0;
      const height = wrapperRef.current?.clientHeight ?? 0;
      if (referenceViewport && width >= 1050 && height >= 760) {
        await instance.setViewport({ x: 111, y: 17, zoom: 0.93 }, { duration: 140 });
        return;
      }
      await instance.fitView({ padding: 0.12, maxZoom: 1, duration: 220 });
    },
    [referenceViewport],
  );

  const runRadialAction = useCallback(
    (item: RadialMenuItem, flowX: number, flowY: number) => {
      const nodeType = nodeTypeForRadialAction(item.action);
      if (nodeType) {
        onRequestCreate(nodeType, flowX, flowY);
        return;
      }
      if (item.action === "canvas:fit") {
        const instance = flowRef.current;
        if (instance) void instance.fitView({ padding: 0.12, maxZoom: 1, duration: 220 });
        return;
      }
      onApplyDefaultLayout();
    },
    [onApplyDefaultLayout, onRequestCreate],
  );

  useEffect(() => {
    runRadialActionRef.current = runRadialAction;
  }, [runRadialAction]);

  useEffect(() => {
    const element = wrapperRef.current;
    if (!element) return;
    const update = () => {
      canvasBoundsRef.current = element.getBoundingClientRect();
      const width = element.clientWidth;
      const height = element.clientHeight;
      setCanvasSize((current) =>
        current.width === width && current.height === height
          ? current
          : { width, height },
      );
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      setNodes(
        buildNodes(
          project,
          selectedNodeId,
          linkFilter,
          expandedNodeIds,
          toggleNodeExpanded,
          highlightedNodeIds,
          diffOverlay,
        ),
      );
      setEdges(buildEdges(project, linkFilter, selectedEdgeId, edgeTypeLabel, edgeStyle, highlightedEdgeIds, diffOverlay));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [
    diffOverlay,
    edgeStyle,
    edgeTypeLabel,
    expandedNodeIds,
    highlightedEdgeIds,
    highlightedNodeIds,
    linkFilter,
    project,
    selectedEdgeId,
    selectedNodeId,
    toggleNodeExpanded,
  ]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      const instance = flowRef.current;
      if (!instance) return;
      if (inspectorOpen) void applyWorkspaceViewport(instance);
      else void instance.fitView({ padding: 0.12, maxZoom: 1, duration: 240 });
    }, 380);
    return () => window.clearTimeout(timer);
  }, [applyWorkspaceViewport, inspectorOpen]);

  // Canvas Diff 条目定位：点击变更项后 fitView 到目标节点（边定位到其源节点）。
  useEffect(() => {
    if (!diffFocus) return;
    const instance = flowRef.current;
    if (!instance) return;
    const timer = window.setTimeout(() => {
      const targetId =
        diffFocus.kind === "edge"
          ? edges.find((edge) => edge.id === diffFocus.id)?.source ?? diffFocus.id
          : diffFocus.id;
      if (!nodes.some((node) => node.id === targetId)) return;
      void instance.fitView({
        nodes: [{ id: targetId }],
        padding: 0.35,
        maxZoom: 1.15,
        duration: 260,
      });
    }, 140);
    return () => window.clearTimeout(timer);
  }, [diffFocus, edges, nodes]);

  useEffect(() => {
    const manualMove = manualMoveRef.current;
    if (manualMove) {
      manualMoveRef.current = null;
      const placement = project.placements.find(
        (candidate) => candidate.nodeId === manualMove.nodeId,
      );
      if (
        placement &&
        Math.abs(placement.x - manualMove.x) < 0.5 &&
        Math.abs(placement.y - manualMove.y) < 0.5
      ) {
        return;
      }
    }
    const timer = window.setTimeout(() => {
      const instance = flowRef.current;
      if (!instance) return;
      void applyWorkspaceViewport(instance);
    }, 40);
    return () => window.clearTimeout(timer);
  }, [applyWorkspaceViewport, linkFilter, project.placements]);

  useEffect(() => {
    if (addRequest === 0) return;
    const width = wrapperRef.current?.clientWidth ?? 1040;
    const height = wrapperRef.current?.clientHeight ?? 780;
    const screen = { x: Math.round(width * 0.8), y: Math.round(height * 0.48) };
    const flow = flowRef.current?.screenToFlowPosition(screen) ?? screen;
    setPieMenu({
      screenX: screen.x,
      screenY: screen.y,
      flowX: flow.x,
      flowY: flow.y,
    });
  }, [addRequest]);

  const contextPoint = useCallback((clientX: number, clientY: number) => {
    const bounds = canvasBoundsRef.current ?? wrapperRef.current?.getBoundingClientRect();
    const screen = {
      x: clientX - (bounds?.left ?? 0),
      y: clientY - (bounds?.top ?? 0),
    };
    const flow = flowRef.current?.screenToFlowPosition({ x: clientX, y: clientY }) ?? screen;
    return { screen, flow };
  }, []);

  const handleContextAction = useCallback(
    (action: ContextMenuActionId, menu: WorkspaceContextMenuState) => {
      setContextMenu(null);
      switch (action) {
        case "node.inspect":
          if (menu.targetId) onSelectNode(menu.targetId);
          break;
        case "node.connect":
          if (menu.targetId) onRequestConnect(menu.targetId);
          break;
        case "node.duplicate":
          if (menu.targetId) onDuplicateNode(menu.targetId);
          break;
        case "node.delete":
          if (menu.targetId) onDeleteNode(menu.targetId);
          break;
        case "edge.filter": {
          const edge = project.edges.find((item) => item.id === menu.targetId);
          if (edge) onLegendFilter(linkLegendFilterOf(edge));
          break;
        }
        case "edge.reverse":
          if (menu.targetId) onReverseEdge(menu.targetId);
          break;
        case "edge.delete":
          if (menu.targetId) onDeleteEdge(menu.targetId);
          break;
        case "canvas.add":
          setPieMenu({
            screenX: menu.screenX,
            screenY: menu.screenY,
            flowX: menu.flowX,
            flowY: menu.flowY,
          });
          break;
        case "canvas.note":
          onRequestCreate("note", menu.flowX, menu.flowY);
          break;
        case "canvas.expandAll":
          setExpandedNodeIds(
            new Set(project.nodes.filter(isExpandableVariable).map((node) => node.id)),
          );
          break;
        case "canvas.collapseAll":
          setExpandedNodeIds(new Set());
          break;
        case "canvas.layout":
          onApplyDefaultLayout();
          break;
        case "canvas.fit":
          void flowRef.current?.fitView({ padding: 0.12, maxZoom: 1, duration: 220 });
          break;
      }
    },
    [
      onApplyDefaultLayout,
      onDeleteEdge,
      onDeleteNode,
      onDuplicateNode,
      onLegendFilter,
      onRequestConnect,
      onRequestCreate,
      onReverseEdge,
      onSelectNode,
      project.edges,
      project.nodes,
    ],
  );

  useEffect(() => {
    let cancelled = false;
    let stop: () => void = () => undefined;
    const nativePinch = nativePinchRef.current;
    const reset = () => {
      nativePinch.latestFrame = null;
      nativePinch.originViewport = null;
      nativePinch.anchor = null;
      nativePinch.animationFrame = null;
      nativePinch.ending = false;
      nativePinch.filterState = emptyTrackpadLowPassState();
      nativePinch.radial = null;
      nativeGestureActiveRef.current = false;
    };
    const applyRadialFrame = (frame: NativeTrackpadFrame) => {
      const radial = nativePinch.radial;
      if (!radial) return;
      const selection = radialSelectionForNormalizedDisplacement(
        radialMenuCacheRef.current,
        (frame.centerX - radial.originCenterX) * radial.inverseDeviceWidth,
        (frame.centerY - radial.originCenterY) * radial.inverseDeviceHeight,
      );
      const selectedSector = selection?.sectorIndex ?? null;
      const selectedItem = selection?.item ?? null;
      if (
        radial.selectedSector === selectedSector &&
        radial.selectedItem?.id === selectedItem?.id
      ) {
        return;
      }
      radial.selectedSector = selectedSector;
      radial.selectedItem = selectedItem;
      radialMenuRef.current?.updateGesture(selectedSector, true);
    };
    const applyLatestFrame = () => {
      nativePinch.animationFrame = null;
      const frame = nativePinch.latestFrame;
      const originViewport = nativePinch.originViewport;
      const anchor = nativePinch.anchor;
      const instance = flowRef.current;
      const bounds = canvasBoundsRef.current;
      nativePinch.latestFrame = null;
      if (frame && nativePinch.radial) {
        applyRadialFrame(frame);
        return;
      }
      if (frame && originViewport && anchor && instance && bounds) {
        // Pan and zoom are composed from the same complete native frame and
        // committed once per animation frame.
        // 平移与缩放来自同一个原生完整帧，每个动画帧只提交一次视口变换。
        const filtered = lowPassCompleteTrackpadFrame(
          nativePinch.filterState,
          { x: frame.panX, y: frame.panY },
          frame.scale,
          trackpadTuningRef.current.sensitivity,
          trackpadTuningRef.current.filterStrength,
        );
        nativePinch.filterState = filtered.state;
        void instance.setViewport(
          viewportForCompleteTrackpadFrame(
            originViewport,
            anchor,
            filtered.pan,
            { width: bounds.width, height: bounds.height },
            filtered.scale,
          ),
        );
      }
      if (nativePinch.ending) reset();
    };
    const scheduleLatestFrame = () => {
      if (nativePinch.animationFrame !== null) return;
      nativePinch.animationFrame = window.requestAnimationFrame(applyLatestFrame);
    };

    void listenForNativeTrackpadFrames((frame) => {
      if (frame.phase === "start" && wrapperRef.current) {
        canvasBoundsRef.current = wrapperRef.current.getBoundingClientRect();
      }
      const bounds = canvasBoundsRef.current;
      const instance = flowRef.current;
      if (!bounds || !instance) return;
      const insideCanvas =
        frame.cursorX >= bounds.left &&
        frame.cursorX <= bounds.right &&
        frame.cursorY >= bounds.top &&
        frame.cursorY <= bounds.bottom;
      const pointerTarget = frame.phase === "start"
        ? document.elementFromPoint(frame.cursorX, frame.cursorY)
        : null;
      const canvasSurfaceOwnsPointer = frame.phase === "start"
        ? Boolean(
            pointerTarget &&
              wrapperRef.current?.contains(pointerTarget) &&
              pointerTarget.closest(".react-flow"),
          )
        : insideCanvas;
      if (frame.phase === "start" && !canvasSurfaceOwnsPointer) return;
      if (frame.phase !== "start" && !nativePinch.originViewport) return;
      if (frame.phase === "start" || !nativePinch.originViewport || !nativePinch.anchor) {
        nativeGestureActiveRef.current = true;
        nativePinch.originViewport = instance.getViewport();
        nativePinch.anchor = {
          x: frame.cursorX - bounds.left,
          y: frame.cursorY - bounds.top,
        };
        nativePinch.ending = false;
        nativePinch.filterState = emptyTrackpadLowPassState();
        nativePinch.radial = null;
      }
      if (frame.phase === "end") {
        if (nativePinch.radial) {
          if (nativePinch.animationFrame !== null) {
            window.cancelAnimationFrame(nativePinch.animationFrame);
            nativePinch.animationFrame = null;
          }
          if (nativePinch.latestFrame) applyRadialFrame(nativePinch.latestFrame);
          const radial = nativePinch.radial;
          if (radial.selectedItem) {
            runRadialActionRef.current(radial.selectedItem, radial.flowX, radial.flowY);
          }
          radialMenuRef.current?.updateGesture(null, false);
          setCursorLongPress(false);
          setPieMenu(null);
          reset();
          return;
        }
        nativePinch.ending = true;
        if (nativePinch.latestFrame) scheduleLatestFrame();
        else reset();
        return;
      }
      if (nativePinch.radial) {
        if (!insideCanvas) {
          if (nativePinch.animationFrame !== null) {
            window.cancelAnimationFrame(nativePinch.animationFrame);
          }
          radialMenuRef.current?.updateGesture(null, false);
          setCursorLongPress(false);
          setPieMenu(null);
          reset();
          return;
        }
        nativePinch.latestFrame = frame;
        scheduleLatestFrame();
        return;
      }
      if (!canvasSurfaceOwnsPointer) {
        if (nativePinch.animationFrame !== null) {
          window.cancelAnimationFrame(nativePinch.animationFrame);
        }
        reset();
        return;
      }
      if (frame.held) {
        if (nativePinch.animationFrame !== null) {
          window.cancelAnimationFrame(nativePinch.animationFrame);
          nativePinch.animationFrame = null;
        }
        nativePinch.latestFrame = null;
        const screen = {
          x: frame.cursorX - bounds.left,
          y: frame.cursorY - bounds.top,
        };
        const flow = instance.screenToFlowPosition({
          x: frame.cursorX,
          y: frame.cursorY,
        });
        nativePinch.radial = {
          originCenterX: frame.centerX,
          originCenterY: frame.centerY,
          inverseDeviceWidth: 1 / Math.max(frame.deviceWidth, 1),
          inverseDeviceHeight: 1 / Math.max(frame.deviceHeight, 1),
          selectedSector: null,
          selectedItem: null,
          flowX: flow.x,
          flowY: flow.y,
        };
        setContextMenu(null);
        setPieMenu({
          screenX: screen.x,
          screenY: screen.y,
          flowX: flow.x,
          flowY: flow.y,
          gestureActive: true,
        });
        setCursorLongPress(true);
        return;
      }
      nativePinch.latestFrame = frame;
      scheduleLatestFrame();
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    });
    return () => {
      cancelled = true;
      if (nativePinch.animationFrame !== null) {
        window.cancelAnimationFrame(nativePinch.animationFrame);
      }
      reset();
      stop();
    };
  }, []);

  useEffect(() => {
    const element = wrapperRef.current;
    if (!isTauriRuntime || !element) return;
    const chromiumWheel = chromiumWheelRef.current;
    const applyChromiumWheel = () => {
      chromiumWheel.animationFrame = null;
      const instance = flowRef.current;
      const cursor = chromiumWheel.cursor;
      const tuning = trackpadTuningRef.current;
      const wheelDeadZone = 0.25 + tuning.filterStrength * 1.75;
      const tuneAxis = (value: number) =>
        Math.abs(value) <= wheelDeadZone
          ? 0
          : Math.sign(value) * (Math.abs(value) - wheelDeadZone) * tuning.sensitivity;
      const panDelta = {
        x: tuneAxis(chromiumWheel.panX),
        y: tuneAxis(chromiumWheel.panY),
      };
      const rawScaleLog = Math.log(Math.max(chromiumWheel.scale, 0.01));
      const scaleDeadZone = 0.0015 + tuning.filterStrength * 0.008;
      const scale =
        Math.abs(rawScaleLog) <= scaleDeadZone
          ? 1
          : Math.exp(
              Math.sign(rawScaleLog) *
                (Math.abs(rawScaleLog) - scaleDeadZone) *
                tuning.sensitivity,
            );
      chromiumWheel.panX = 0;
      chromiumWheel.panY = 0;
      chromiumWheel.scale = 1;
      chromiumWheel.cursor = null;
      if (!instance || !cursor) return;
      if (panDelta.x === 0 && panDelta.y === 0 && scale === 1) return;
      void instance.setViewport(
        viewportForCoalescedWheelFrame(
          instance.getViewport(),
          cursor,
          panDelta,
          scale,
        ),
      );
    };
    const observeWheel = (event: WheelEvent) => {
      if (nativeGestureActiveRef.current) {
        event.preventDefault();
        event.stopImmediatePropagation();
        return;
      }
      event.preventDefault();
      event.stopImmediatePropagation();
      const bounds = canvasBoundsRef.current ?? element.getBoundingClientRect();
      chromiumWheel.cursor = {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      };
      if (event.ctrlKey) {
        // Chromium encodes pinch in deltaY; do not reinterpret it as vertical
        // pan. Any ordinary pan events arriving in this frame remain composed.
        // Chromium 使用 deltaY 表示捏合，不能再把它当作垂直平移。
        chromiumWheel.scale *= chromiumTrackpadPinchScale(event.deltaY);
      } else {
        const pan = wheelPanDelta(event.deltaX, event.deltaY, event.deltaMode);
        chromiumWheel.panX += pan.x;
        chromiumWheel.panY += pan.y;
      }
      if (chromiumWheel.animationFrame === null) {
        chromiumWheel.animationFrame = window.requestAnimationFrame(applyChromiumWheel);
      }
    };
    element.addEventListener("wheel", observeWheel, {
      capture: true,
      passive: false,
    });
    return () => {
      element.removeEventListener("wheel", observeWheel, { capture: true });
      if (chromiumWheel.animationFrame !== null) {
        window.cancelAnimationFrame(chromiumWheel.animationFrame);
      }
      chromiumWheel.panX = 0;
      chromiumWheel.panY = 0;
      chromiumWheel.scale = 1;
      chromiumWheel.cursor = null;
      chromiumWheel.animationFrame = null;
    };
  }, [isTauriRuntime]);

  const handleNodesChange = useCallback(
    (changes: NodeChange<WorkspaceNode>[]) => {
      const nonRemove: NodeChange<WorkspaceNode>[] = [];
      for (const change of changes) {
        if (change.type === "remove") {
          onDeleteNode(change.id);
        } else {
          nonRemove.push(change);
        }
      }
      if (nonRemove.length > 0) {
        setNodes((current) => applyNodeChanges(nonRemove, current));
      }
    },
    [onDeleteNode],
  );

  const handleEdgesChange = useCallback(
    (changes: EdgeChange<WorkspaceEdge>[]) => {
      const nonRemove: EdgeChange<WorkspaceEdge>[] = [];
      for (const change of changes) {
        if (change.type === "remove") {
          onDeleteEdge(change.id);
        } else {
          nonRemove.push(change);
        }
      }
      if (nonRemove.length > 0) {
        setEdges((current) => applyEdgeChanges(nonRemove, current));
      }
    },
    [onDeleteEdge],
  );

  const handleConnect = useCallback(
    (connection: Connection) => {
      setEdges((current) =>
        addEdge(
          {
            ...connection,
            id: `edge-preview-${Date.now()}`,
            type: "researchEdge",
            data: {
              label: edgeTypeLabel(connectType),
              edgeStyle,
              record: {
                id: "edge-preview",
                source: connection.source,
                target: connection.target,
                type: connectType,
                directed: true,
                polarity: "positive",
                conditions: [],
                evidenceIds: [],
                provenance: { origin: "human" },
              },
            },
          },
          current,
        ),
      );
      onCreateEdge(connection.source, connection.target);
    },
    [connectType, edgeStyle, edgeTypeLabel, onCreateEdge],
  );

  const setIncidentEdgePreview = useCallback((nodeId: string, enabled: boolean) => {
    setEdges((current) =>
      current.map((edge) => {
        if (edge.source !== nodeId && edge.target !== nodeId) return edge;
        if (Boolean(edge.data?.dragPreview) === enabled) return edge;
        return {
          ...edge,
          data: edge.data ? { ...edge.data, dragPreview: enabled } : edge.data,
        };
      }),
    );
  }, []);

  const minimapColor = useCallback(
    (node: WorkspaceNode) =>
      node.id === selectedNodeId
        ? "var(--minimap-selected-node)"
        : node.data.record.type === "evidence"
          ? "var(--minimap-evidence-node)"
          : "var(--minimap-node)",
    [selectedNodeId],
  );
  const minimapNodeLookupRef = useRef(new Map<string, WorkspaceNode>());
  useEffect(() => {
    minimapNodeLookupRef.current = new Map(nodes.map((node) => [node.id, node]));
  }, [nodes]);
  const minimapTargetsBySource = useMemo(() => {
    const lookup = new Map<string, Array<{ edgeId: string; targetId: string }>>();
    edges.forEach((edge) => {
      const targets = lookup.get(edge.source) ?? [];
      targets.push({ edgeId: edge.id, targetId: edge.target });
      lookup.set(edge.source, targets);
    });
    return lookup;
  }, [edges]);
  const minimapNodeComponent = useCallback(
    ({ id, x, y, width, height, borderRadius, color, strokeColor, strokeWidth }: MiniMapNodeProps) => (
      <g>
        {showMiniMapRelations && !draggingNodeId &&
          (minimapTargetsBySource.get(id) ?? [])
            .map(({ edgeId, targetId }) => {
              const target = minimapNodeLookupRef.current.get(targetId);
              if (!target) return null;
              const targetWidth = target.measured?.width ?? target.width ?? 0;
              const targetHeight = target.measured?.height ?? target.height ?? 0;
              return (
                <line
                  key={edgeId}
                  x1={x + width / 2}
                  y1={y + height / 2}
                  x2={target.position.x + targetWidth / 2}
                  y2={target.position.y + targetHeight / 2}
                  stroke="var(--minimap-relation)"
                  strokeWidth={3}
                  opacity={0.72}
                  vectorEffect="non-scaling-stroke"
                />
              );
            })}
        <rect
          x={x}
          y={y}
          width={width}
          height={height}
          rx={borderRadius}
          ry={borderRadius}
          fill={color}
          stroke={strokeColor}
          strokeWidth={strokeWidth}
        />
      </g>
    ),
    [draggingNodeId, minimapTargetsBySource, showMiniMapRelations],
  );

  const canvasClass = useMemo(
    () => `zen-flow h-full w-full ${connectMode ? "is-connecting" : ""}`,
    [connectMode],
  );
  const legendCounts = useMemo(() => {
    const counts: Record<LinkLegendFilter, number> = {
      causal: 0,
      control: 0,
      derived: 0,
      contradicts: 0,
    };
    project.edges.forEach((edge) => {
      counts[linkLegendFilterOf(edge)] += 1;
    });
    return counts;
  }, [project.edges]);
  const legendItems: Array<{
    key: LinkLegendFilter;
    labelKey: MessageKey;
    lineClass: string;
    textClass?: string;
  }> = [
    { key: "causal", labelKey: "relation.causal", lineClass: "border-ink/70" },
    { key: "control", labelKey: "relation.control", lineClass: "border-dashed border-ink/70" },
    { key: "derived", labelKey: "relation.derived", lineClass: "border-dotted border-ink/70" },
    {
      key: "contradicts",
      labelKey: "relation.contradicts",
      lineClass: "border-dashed border-alert",
      textClass: "text-alert",
    },
  ];

  const handleFlowInit = useCallback<NonNullable<WorkspaceReactFlowProps["onInit"]>>(
    (instance) => {
      flowRef.current = instance;
      setPieMenu((menu) => {
        if (!menu) return menu;
        const bounds = canvasBoundsRef.current;
        if (!bounds) return menu;
        const flow = instance.screenToFlowPosition({
          x: menu.screenX + bounds.left,
          y: menu.screenY + bounds.top,
        });
        return { ...menu, flowX: flow.x, flowY: flow.y };
      });
      window.setTimeout(() => {
        void applyWorkspaceViewport(instance);
      }, 80);
    },
    [applyWorkspaceViewport],
  );
  const handleNodeClick = useCallback<NonNullable<WorkspaceReactFlowProps["onNodeClick"]>>(
    (_, node) => {
      onSelectNode(node.id);
      setPieMenu(null);
      setContextMenu(null);
    },
    [onSelectNode],
  );
  const handleNodeContextMenu = useCallback<
    NonNullable<WorkspaceReactFlowProps["onNodeContextMenu"]>
  >(
    (event, node) => {
      event.preventDefault();
      event.stopPropagation();
      const point = contextPoint(event.clientX, event.clientY);
      onSelectNode(node.id);
      setPieMenu(null);
      setContextMenu({
        scope: "node",
        targetId: node.id,
        title: node.data.record.title,
        screenX: point.screen.x,
        screenY: point.screen.y,
        flowX: point.flow.x,
        flowY: point.flow.y,
      });
    },
    [contextPoint, onSelectNode],
  );
  const handleEdgeContextMenu = useCallback<
    NonNullable<WorkspaceReactFlowProps["onEdgeContextMenu"]>
  >(
    (event, edge) => {
      event.preventDefault();
      event.stopPropagation();
      const point = contextPoint(event.clientX, event.clientY);
      onSelectEdge(edge.id);
      setPieMenu(null);
      setContextMenu({
        scope: "edge",
        targetId: edge.id,
        title: edge.data?.label,
        screenX: point.screen.x,
        screenY: point.screen.y,
        flowX: point.flow.x,
        flowY: point.flow.y,
      });
    },
    [contextPoint, onSelectEdge],
  );
  const handleEdgeClick = useCallback<NonNullable<WorkspaceReactFlowProps["onEdgeClick"]>>(
    (_, edge) => {
      onSelectEdge(edge.id);
      setPieMenu(null);
      setContextMenu(null);
    },
    [onSelectEdge],
  );
  const handleNodeDragStart = useCallback<
    NonNullable<WorkspaceReactFlowProps["onNodeDragStart"]>
  >(
    (_, node) => {
      setDraggingNodeId(node.id);
      setIncidentEdgePreview(node.id, true);
    },
    [setIncidentEdgePreview],
  );
  const handleNodeDragStop = useCallback<
    NonNullable<WorkspaceReactFlowProps["onNodeDragStop"]>
  >(
    (_, node) => {
      setIncidentEdgePreview(node.id, false);
      setDraggingNodeId(null);
      manualMoveRef.current = {
        nodeId: node.id,
        x: node.position.x,
        y: node.position.y,
      };
      onMoveNode(node.id, node.position.x, node.position.y);
    },
    [onMoveNode, setIncidentEdgePreview],
  );
  const handlePaneClick = useCallback(() => {
    setPieMenu(null);
    setContextMenu(null);
  }, []);
  const handlePaneContextMenu = useCallback<
    NonNullable<WorkspaceReactFlowProps["onPaneContextMenu"]>
  >(
    (event) => {
      event.preventDefault();
      const point = contextPoint(event.clientX, event.clientY);
      setPieMenu(null);
      setContextMenu({
        scope: "canvas",
        screenX: point.screen.x,
        screenY: point.screen.y,
        flowX: point.flow.x,
        flowY: point.flow.y,
      });
    },
    [contextPoint],
  );
  const closePieMenu = useCallback(() => {
    setCursorLongPress(false);
    radialMenuRef.current?.updateGesture(null, false);
    setPieMenu(null);
  }, []);
  const choosePieItem = useCallback(
    (item: RadialMenuItem) => {
      if (!pieMenu) return;
      setCursorLongPress(false);
      runRadialAction(item, pieMenu.flowX, pieMenu.flowY);
      radialMenuRef.current?.updateGesture(null, false);
      setPieMenu(null);
    },
    [pieMenu, runRadialAction],
  );

  return (
    <div
      ref={wrapperRef}
      className="relative h-full min-h-0 overflow-hidden bg-canvas"
      onContextMenuCapture={(event) => {
        event.preventDefault();
      }}
    >
      <ReactFlow<WorkspaceNode, WorkspaceEdge>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onInit={handleFlowInit}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={handleConnect}
        onNodeClick={handleNodeClick}
        onNodeContextMenu={handleNodeContextMenu}
        onEdgeContextMenu={handleEdgeContextMenu}
        onEdgeClick={handleEdgeClick}
        onNodeDragStart={handleNodeDragStart}
        onNodeDragStop={handleNodeDragStop}
        onPaneClick={handlePaneClick}
        onPaneContextMenu={handlePaneContextMenu}
        fitView
        fitViewOptions={fitViewOptions}
        minZoom={0.45}
        maxZoom={1.7}
        zoomOnPinch={!isTauriRuntime}
        zoomOnScroll={false}
        panOnScroll={!isTauriRuntime}
        preventScrolling
        connectOnClick={connectMode}
        connectionMode={ConnectionMode.Loose}
        connectionRadius={26}
        className={canvasClass}
        proOptions={proOptions}
      >
        <Background variant={BackgroundVariant.Dots} gap={28} size={0.55} color="#dfe2e5" />
        {showMiniMap && (
          <MiniMap
            nodeColor={minimapColor}
            nodeComponent={minimapNodeComponent}
            bgColor="var(--minimap-background)"
            maskColor="var(--minimap-mask)"
            pannable
            zoomable
            className="zen-minimap"
          />
        )}
        <Controls showInteractive={false} className="zen-controls" />
      </ReactFlow>

      <div className="absolute bottom-5 right-5 z-10 w-[154px] rounded-[5px] border border-ink/30 bg-paper/96 p-2 font-serif text-[10px] text-ink/80">
        <div className="flex items-center justify-between px-2 pb-1.5 font-sans text-[7px] uppercase tracking-[0.14em] text-ink/40">
          <span>{t("workspace.linkFilter")}</span>
          {showLinkCounts && (
            <span>{linkFilter ? legendCounts[linkFilter] : project.edges.length}</span>
          )}
        </div>
        {legendItems.map((item) => {
          const selected = linkFilter === item.key;
          return (
            <button
              key={item.key}
              className={`flex w-full items-center gap-3 rounded-[3px] px-2 py-1.5 text-left transition ${
                selected ? "bg-blue-soft ring-1 ring-inset ring-blue/25" : "hover:bg-ink/5"
              } ${item.textClass ?? ""}`}
              aria-pressed={selected}
              title={`${t(item.labelKey)} · ${t("workspace.layout")}`}
              onClick={() => onLegendFilter(selected ? null : item.key)}
            >
              <span className={`block w-9 border-t ${item.lineClass}`} />
              <span className="min-w-0 flex-1">{t(item.labelKey)}</span>
              {showLinkCounts && (
                <span className="font-sans text-[8px] text-ink/35">
                  {legendCounts[item.key]}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {pieMenu && (
        <RadialAddMenu
          ref={radialMenuRef}
          menu={pieMenu}
          cache={radialMenuCache}
          onClose={closePieMenu}
          onChoose={choosePieItem}
        />
      )}

      {contextMenu && (
        <WorkspaceContextMenu
          menu={contextMenu}
          width={canvasSize.width}
          height={canvasSize.height}
          actionOrder={contextMenus[contextMenu.scope]}
          shortcuts={shortcuts}
          pluginActions={pluginContextMenuActions.filter(
            (action) => action.scope === contextMenu.scope,
          )}
          onBuiltInAction={handleContextAction}
          onPluginAction={(action, menu) => {
            setContextMenu(null);
            onPluginContextMenuAction(action, menu);
          }}
          onClose={() => setContextMenu(null)}
        />
      )}
    </div>
  );
}

export function ResearchGraphCanvas(props: ResearchGraphCanvasProps) {
  return (
    <ReactFlowProvider>
      <ResearchGraphInner {...props} />
    </ReactFlowProvider>
  );
}
