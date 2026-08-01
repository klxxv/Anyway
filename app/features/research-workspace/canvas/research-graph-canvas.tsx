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
  type NodeChange,
  type ReactFlowInstance,
} from "@xyflow/react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { MessageKey } from "../../../i18n/catalog";
import { useI18n } from "../../../i18n/provider";
import type {
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
  viewportForNativeTrackpadPinch,
  viewportForTrackpadWheel,
  shouldSuppressSyntheticPinchWheel,
  type GesturePoint,
  type GestureViewport,
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
import { RadialAddMenu } from "../components/radial-add-menu";
import { WorkspaceContextMenu } from "../components/workspace-context-menu";
import { ResearchEdgeLine } from "./research-edge-line";
import { ResearchNodeCard } from "./research-node-card";
import { computeEdgeRoutes } from "./edge-routing";

const nodeTypes = { researchNode: ResearchNodeCard };
const edgeTypes = { researchEdge: ResearchEdgeLine };

type ResearchGraphCanvasProps = {
  project: ProjectState;
  selectedNodeId: string;
  selectedEdgeId: string;
  addRequest: number;
  connectMode: boolean;
  connectType: ResearchEdgeType;
  inspectorOpen: boolean;
  linkFilter: LinkLegendFilter | null;
  showMiniMap: boolean;
  showLinkCounts: boolean;
  referenceViewport: boolean;
  contextMenus: ContextMenuPreferences;
  shortcuts: WorkspaceShortcuts;
  pluginContextMenuActions: ResolvedPluginContextMenuAction[];
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
): WorkspaceNode[] {
  const projected = projectForLegendFilter(project, filter);
  return projected.nodes.map((record) => {
    const placement = projected.placements.find((item) => item.nodeId === record.id);
    const circle = record.data.shape === "circle" || record.type === "question";
    return {
      id: record.id,
      type: "researchNode",
      position: { x: placement?.x ?? 0, y: placement?.y ?? 0 },
      width: placement?.width ?? (circle ? 136 : 164),
      height: placement?.height ?? (circle ? 136 : 116),
      selected: record.id === selectedNodeId,
      data: { record, shape: circle ? "circle" : "card" },
    };
  });
}

function buildEdges(
  project: ProjectState,
  filter: LinkLegendFilter | null,
  selectedEdgeId: string,
  edgeTypeLabel: (type: ResearchEdgeType) => string,
): WorkspaceEdge[] {
  const projected = projectForLegendFilter(project, filter);
  const routes = computeEdgeRoutes(projected);
  return projected.edges.map((record) => {
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
        labelOffsetX: route?.labelOffsetX,
        labelOffsetY: route?.labelOffsetY,
      },
    };
  });
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
    showMiniMap,
    showLinkCounts,
    referenceViewport,
    contextMenus,
    shortcuts,
    pluginContextMenuActions,
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
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const flowRef = useRef<ReactFlowInstance<WorkspaceNode, WorkspaceEdge> | null>(null);
  const nativePinchRef = useRef<{
    latestFrame: NativeTrackpadFrame | null;
    originViewport: GestureViewport | null;
    anchor: GesturePoint | null;
    originScale: number;
    animationFrame: number | null;
    ending: boolean;
  }>({
    latestFrame: null,
    originViewport: null,
    anchor: null,
    originScale: 1,
    animationFrame: null,
    ending: false,
  });
  // Only suppress WebView2's Ctrl+Wheel copy while native frames are actually
  // arriving. Registration alone is insufficient because WebView2 renders in
  // a child HWND that can remain the pointer target.
  // 仅在原生帧确实到达时屏蔽 WebView2 滚轮副本；不能只凭注册成功判断输入链路可用。
  const nativePinchActiveRef = useRef(false);
  const edgeTypeLabel = useCallback(
    (type: ResearchEdgeType) => t(edgeTypeMessageKeys[type]),
    [t],
  );
  const [nodes, setNodes] = useState(() =>
    buildNodes(project, selectedNodeId, linkFilter),
  );
  const [edges, setEdges] = useState(() =>
    buildEdges(project, linkFilter, selectedEdgeId, edgeTypeLabel),
  );
  const [canvasSize, setCanvasSize] = useState({ width: 0, height: 0 });
  const [contextMenu, setContextMenu] = useState<WorkspaceContextMenuState | null>(null);
  const [pieMenu, setPieMenu] = useState<PieMenuState | null>(null);
  const placementKey = useMemo(
    () =>
      project.placements
        .map((placement) => `${placement.nodeId}:${placement.x}:${placement.y}`)
        .join("|"),
    [project.placements],
  );

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

  useEffect(() => {
    const element = wrapperRef.current;
    if (!element) return;
    const update = () => setCanvasSize({ width: element.clientWidth, height: element.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      setNodes(buildNodes(project, selectedNodeId, linkFilter));
      setEdges(buildEdges(project, linkFilter, selectedEdgeId, edgeTypeLabel));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [edgeTypeLabel, linkFilter, project, selectedEdgeId, selectedNodeId]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      const instance = flowRef.current;
      if (!instance) return;
      if (inspectorOpen) void applyWorkspaceViewport(instance);
      else void instance.fitView({ padding: 0.12, maxZoom: 1, duration: 240 });
    }, 380);
    return () => window.clearTimeout(timer);
  }, [applyWorkspaceViewport, inspectorOpen]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      const instance = flowRef.current;
      if (!instance) return;
      void applyWorkspaceViewport(instance);
    }, 40);
    return () => window.clearTimeout(timer);
  }, [applyWorkspaceViewport, linkFilter, placementKey]);

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
    const bounds = wrapperRef.current?.getBoundingClientRect();
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
      nativePinch.originScale = 1;
      nativePinch.animationFrame = null;
      nativePinch.ending = false;
      nativePinchActiveRef.current = false;
    };
    const applyLatestFrame = () => {
      nativePinch.animationFrame = null;
      const frame = nativePinch.latestFrame;
      const originViewport = nativePinch.originViewport;
      const anchor = nativePinch.anchor;
      const instance = flowRef.current;
      nativePinch.latestFrame = null;
      if (frame && originViewport && anchor && instance) {
        const relativeScale = frame.scale / nativePinch.originScale;
        if (Math.abs(relativeScale - 1) >= 0.003) {
          void instance.setViewport(
            viewportForNativeTrackpadPinch(originViewport, anchor, 1, relativeScale),
          );
        }
      }
      if (nativePinch.ending) reset();
    };
    const scheduleLatestFrame = () => {
      if (nativePinch.animationFrame !== null) return;
      nativePinch.animationFrame = window.requestAnimationFrame(applyLatestFrame);
    };

    void listenForNativeTrackpadFrames((frame) => {
      const bounds = wrapperRef.current?.getBoundingClientRect();
      const instance = flowRef.current;
      if (!bounds || !instance) return;
      if (frame.phase === "start" || !nativePinch.originViewport || !nativePinch.anchor) {
        nativePinchActiveRef.current = true;
        nativePinch.originViewport = instance.getViewport();
        nativePinch.anchor = {
          x: frame.cursorX - bounds.left,
          y: frame.cursorY - bounds.top,
        };
        nativePinch.originScale = frame.scale > 0 ? frame.scale : 1;
        nativePinch.ending = false;
      }
      if (frame.phase === "end") {
        nativePinch.ending = true;
        if (nativePinch.latestFrame) scheduleLatestFrame();
        else reset();
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

  const handleNodesChange = useCallback(
    (changes: NodeChange<WorkspaceNode>[]) => {
      setNodes((current) => applyNodeChanges(changes, current));
    },
    [],
  );

  const handleEdgesChange = useCallback(
    (changes: EdgeChange<WorkspaceEdge>[]) => {
      setEdges((current) => applyEdgeChanges(changes, current));
    },
    [],
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
    [connectType, edgeTypeLabel, onCreateEdge],
  );

  const minimapColor = useCallback(
    (node: WorkspaceNode) =>
      node.id === selectedNodeId ? "#2457d6" : node.data.record.type === "evidence" ? "#6c737b" : "#a8adb3",
    [selectedNodeId],
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

  return (
    <div
      ref={wrapperRef}
      className="relative h-full min-h-0 overflow-hidden bg-canvas"
      onContextMenuCapture={(event) => {
        event.preventDefault();
      }}
      onWheelCapture={(event) => {
        if (!event.ctrlKey || !flowRef.current || !wrapperRef.current) return;
        // Suppress the synthetic wheel only after a native start frame reached
        // this component. Otherwise WebView2's child HWND owns the pointer and
        // Ctrl+Wheel is the functional fallback, not a duplicate.
        // 只有收到原生起始帧后才屏蔽合成滚轮；子窗口持有输入时必须保留回退链路。
        if (
          shouldSuppressSyntheticPinchWheel(
            "__TAURI_INTERNALS__" in window,
            nativePinchActiveRef.current,
          )
        ) {
          event.preventDefault();
          event.stopPropagation();
          return;
        }
        const bounds = wrapperRef.current.getBoundingClientRect();
        const nextViewport = viewportForTrackpadWheel(
          flowRef.current.getViewport(),
          { x: event.clientX - bounds.left, y: event.clientY - bounds.top },
          event.deltaY,
          event.deltaMode,
        );
        event.preventDefault();
        event.stopPropagation();
        void flowRef.current.setViewport(nextViewport);
      }}
    >
      <ReactFlow<WorkspaceNode, WorkspaceEdge>
        nodes={nodes}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onInit={(instance) => {
          flowRef.current = instance;
          setPieMenu((menu) => {
            if (!menu || !wrapperRef.current) return menu;
            const bounds = wrapperRef.current.getBoundingClientRect();
            const flow = instance.screenToFlowPosition({
              x: menu.screenX + bounds.left,
              y: menu.screenY + bounds.top,
            });
            return { ...menu, flowX: flow.x, flowY: flow.y };
          });
          window.setTimeout(() => {
            void applyWorkspaceViewport(instance);
          }, 80);
        }}
        onNodesChange={handleNodesChange}
        onEdgesChange={handleEdgesChange}
        onConnect={handleConnect}
        onNodeClick={(_, node) => {
          onSelectNode(node.id);
          setPieMenu(null);
          setContextMenu(null);
        }}
        onNodeContextMenu={(event, node) => {
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
        }}
        onEdgeContextMenu={(event, edge) => {
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
        }}
        onEdgeClick={(_, edge) => {
          onSelectEdge(edge.id);
          setPieMenu(null);
          setContextMenu(null);
        }}
        onNodeDragStop={(_, node) => onMoveNode(node.id, node.position.x, node.position.y)}
        onPaneClick={() => {
          setPieMenu(null);
          setContextMenu(null);
        }}
        onPaneContextMenu={(event) => {
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
        }}
        fitView
        fitViewOptions={{ padding: 0.12, maxZoom: 1 }}
        minZoom={0.45}
        maxZoom={1.7}
        zoomOnPinch
        zoomOnScroll={false}
        panOnScroll
        preventScrolling
        connectOnClick={connectMode}
        connectionMode={ConnectionMode.Loose}
        connectionRadius={26}
        className={canvasClass}
        proOptions={{ hideAttribution: true }}
      >
        <Background variant={BackgroundVariant.Dots} gap={28} size={0.55} color="#dfe2e5" />
        {showMiniMap && (
          <MiniMap
            nodeColor={minimapColor}
            maskColor="rgba(255,255,255,.72)"
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
          menu={pieMenu}
          onClose={() => setPieMenu(null)}
          onChoose={(type) => {
            onRequestCreate(type, pieMenu.flowX, pieMenu.flowY);
            setPieMenu(null);
          }}
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
