"use client";

import {
  Background,
  BackgroundVariant,
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
import type { ProjectState, ResearchNodeType } from "../../../lib/research-types";
import { useTwoFingerPie } from "../hooks/use-two-finger-pie";
import type { PieMenuState, WorkspaceEdge, WorkspaceNode } from "../workspace-types";
import {
  linkLegendFilterOf,
  projectForLegendFilter,
  type LinkLegendFilter,
} from "../workspace-layout";
import { RadialAddMenu } from "../components/radial-add-menu";
import { ResearchEdgeLine } from "./research-edge-line";
import { ResearchNodeCard } from "./research-node-card";

const nodeTypes = { researchNode: ResearchNodeCard };
const edgeTypes = { researchEdge: ResearchEdgeLine };

type ResearchGraphCanvasProps = {
  project: ProjectState;
  selectedNodeId: string;
  addRequest: number;
  connectMode: boolean;
  linkFilter: LinkLegendFilter | null;
  showMiniMap: boolean;
  showLinkCounts: boolean;
  referenceViewport: boolean;
  onLegendFilter: (filter: LinkLegendFilter | null) => void;
  onSelectNode: (nodeId: string) => void;
  onMoveNode: (nodeId: string, x: number, y: number) => void;
  onCreateEdge: (source: string, target: string) => void;
  onRequestCreate: (type: ResearchNodeType, x: number, y: number) => void;
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

function buildEdges(project: ProjectState, filter: LinkLegendFilter | null): WorkspaceEdge[] {
  return projectForLegendFilter(project, filter).edges.map((record) => ({
    id: record.id,
    source: record.source,
    target: record.target,
    type: "researchEdge",
    data: { record, label: record.note ?? record.type.replaceAll("_", " ") },
  }));
}

function ResearchGraphInner(props: ResearchGraphCanvasProps) {
  const { t } = useI18n();
  const {
    project,
    selectedNodeId,
    addRequest,
    connectMode,
    linkFilter,
    showMiniMap,
    showLinkCounts,
    referenceViewport,
    onLegendFilter,
    onSelectNode,
    onMoveNode,
    onCreateEdge,
    onRequestCreate,
  } = props;
  const wrapperRef = useRef<HTMLDivElement | null>(null);
  const flowRef = useRef<ReactFlowInstance<WorkspaceNode, WorkspaceEdge> | null>(null);
  const [nodes, setNodes] = useState(() =>
    buildNodes(project, selectedNodeId, linkFilter),
  );
  const [edges, setEdges] = useState(() => buildEdges(project, linkFilter));
  const [pieMenu, setPieMenu] = useState<PieMenuState | null>({
    screenX: 888,
    screenY: 432,
    flowX: 911,
    flowY: 390,
  });
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
    const frame = window.requestAnimationFrame(() => {
      setNodes(buildNodes(project, selectedNodeId, linkFilter));
      setEdges(buildEdges(project, linkFilter));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [linkFilter, project, selectedNodeId]);

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

  const toFlowPoint = useCallback(
    (point: { x: number; y: number }) => {
      const bounds = wrapperRef.current?.getBoundingClientRect();
      if (!bounds) return point;
      return (
        flowRef.current?.screenToFlowPosition({
          x: point.x + bounds.left,
          y: point.y + bounds.top,
        }) ?? point
      );
    },
    [],
  );

  const chooseFromGesture = useCallback(
    (type: ResearchNodeType, point: { x: number; y: number }) => {
      setPieMenu(null);
      onRequestCreate(type, point.x, point.y);
    },
    [onRequestCreate],
  );

  const gesture = useTwoFingerPie({
    toFlowPoint,
    onOpen: setPieMenu,
    onChoose: chooseFromGesture,
  });

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
              label: "causes",
              record: {
                id: "edge-preview",
                source: connection.source,
                target: connection.target,
                type: "causes",
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
    [onCreateEdge],
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
      {...gesture}
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
        }}
        onNodeDragStop={(_, node) => onMoveNode(node.id, node.position.x, node.position.y)}
        onPaneClick={() => setPieMenu(null)}
        onPaneContextMenu={(event) => {
          event.preventDefault();
          const bounds = wrapperRef.current?.getBoundingClientRect();
          if (!bounds) return;
          const screen = { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
          const flow =
            flowRef.current?.screenToFlowPosition({
              x: event.clientX,
              y: event.clientY,
            }) ?? screen;
          setPieMenu({
            screenX: screen.x,
            screenY: screen.y,
            flowX: flow.x,
            flowY: flow.y,
          });
        }}
        fitView
        fitViewOptions={{ padding: 0.12, maxZoom: 1 }}
        minZoom={0.45}
        maxZoom={1.7}
        connectOnClick={connectMode}
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
          <span>Link filter</span>
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
            const flow = toFlowPoint({ x: pieMenu.screenX, y: pieMenu.screenY });
            onRequestCreate(type, flow.x, flow.y);
            setPieMenu(null);
          }}
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
