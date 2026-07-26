"use client";

import {
  Background,
  BackgroundVariant,
  BaseEdge,
  Controls,
  EdgeLabelRenderer,
  Handle,
  MarkerType,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  getBezierPath,
  type Connection,
  type Edge,
  type EdgeProps,
  type Node,
  type NodeChange,
  type NodeProps,
  useReactFlow,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import {
  Activity,
  Binary,
  BrainCircuit,
  ArrowDownToLine,
  ArrowUpFromLine,
  Bot,
  Braces,
  Check,
  ChevronDown,
  ChevronRight,
  CircleDotDashed,
  Download,
  FileJson,
  FileText,
  Filter,
  Focus,
  Gauge,
  GitCommit,
  GitBranch,
  GitFork,
  History,
  LayoutDashboard,
  Link2,
  ListTree,
  Maximize2,
  Network,
  Palette,
  PanelLeftClose,
  PanelRightClose,
  Plus,
  Redo2,
  RotateCcw,
  Search,
  Scissors,
  Settings2,
  ShoppingBag,
  SlidersHorizontal,
  Sparkles,
  Trash2,
  Undo2,
  Upload,
  X,
} from "lucide-react";
import {
  cloneProject,
  compareScenarioReachability,
  computeLayout,
  computeLogicChain,
  detectCycles,
  exportCsv,
  exportJsonCanvas,
  exportMarkdown,
  makeId,
  migrateProject,
  propagateInfluence,
  traverseGraph,
} from "../lib/research-core";
import { initialProject, initialSuggestions } from "../lib/fixtures";
import { createMnistProject, mnistRunSummary } from "../lib/mnist-fixture";
import { pluginCatalog, themeCatalog } from "../lib/plugins";
import {
  EDGE_TYPES,
  LAYOUT_MODES,
  NODE_TYPES,
  type GraphSuggestion,
  type InfluenceResult,
  type LayoutMode,
  type LogicChainMode,
  type LogicChainResult,
  type ProjectState,
  type ResearchEdgeType,
  type ResearchNode,
  type ResearchNodeType,
  type TraversalResult,
} from "../lib/research-types";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ChangeEvent,
  type CSSProperties,
} from "react";

type CanvasNodeData = {
  record: ResearchNode;
  disabled: boolean;
  depth?: number;
  traversed: boolean;
  chainState?: "effective" | "evidence" | "refutation";
  annotation?: string;
  influence?: number;
};

type CanvasNode = Node<CanvasNodeData, "researchNode">;
type CanvasEdgeData = {
  type: ResearchEdgeType;
  confidence?: number;
  disabled: boolean;
  traversed: boolean;
  treeEdge: boolean;
  backEdge: boolean;
  chainState?: "effective" | "evidence" | "refutation";
  experimentLabel?: string;
  experimentDelta?: number;
};
type CanvasEdge = Edge<CanvasEdgeData, "researchEdge">;

type HistoryEntry = {
  label: string;
  snapshot: ProjectState;
};

const nodeTypeLabels: Record<ResearchNodeType, string> = {
  question: "Question",
  concept: "Concept",
  variable: "Variable",
  hypothesis: "Hypothesis",
  method: "Method",
  evidence: "Evidence",
  paper: "Paper",
  dataset: "Dataset",
  experiment: "Experiment",
  result: "Result",
  metric: "Metric",
  formula: "Formula",
  artifact: "Artifact",
  note: "Note",
};

const compactNodeTypes: ResearchNodeType[] = [
  "question",
  "variable",
  "hypothesis",
  "method",
  "evidence",
  "experiment",
  "metric",
  "result",
];

const edgeTypeLabels: Record<ResearchEdgeType, string> = {
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

function ResearchNodeCard({ data, selected }: NodeProps<CanvasNode>) {
  const { record, disabled, depth, traversed, chainState, annotation, influence } = data;
  return (
    <div
      className={[
        "research-node",
        `node-${record.type}`,
        selected ? "is-selected" : "",
        disabled ? "is-disabled" : "",
        traversed ? "is-traversed" : "",
        chainState ? `is-chain-${chainState}` : "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-testid={`node-${record.id}`}
    >
      <Handle type="target" position={Position.Left} className="research-handle" />
      <div className="node-card-topline">
        <span className="node-type-label">{nodeTypeLabels[record.type]}</span>
        <span className={`status-dot status-${record.status}`} title={record.status} />
      </div>
      <div className="node-title">{record.title}</div>
      <div className="node-summary">{record.body}</div>
      <div className="node-meta-row">
        <span>{record.evidenceIds.length} evidence</span>
        {traversed && typeof depth === "number" ? (
          <span className="depth-badge">depth {depth}</span>
        ) : (
          <span>{record.tags[0] ?? "untagged"}</span>
        )}
      </div>
      {(annotation || typeof influence === "number") && (
        <div className="node-analysis-row">
          {annotation && <span>{annotation}</span>}
          {typeof influence === "number" && (
            <span className={influence < 0 ? "negative" : "positive"}>
              BP {influence >= 0 ? "+" : ""}
              {(influence * 100).toFixed(0)}%
            </span>
          )}
        </div>
      )}
      {disabled && <div className="disabled-ribbon">disabled in scenario</div>}
      <Handle type="source" position={Position.Right} className="research-handle" />
    </div>
  );
}

function ResearchEdgeLine({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  markerEnd,
  data,
  selected,
}: EdgeProps<CanvasEdge>) {
  const [path, labelX, labelY] = getBezierPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
    curvature: 0.28,
  });
  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        className={[
          "research-edge-path",
          selected ? "is-selected" : "",
          data?.disabled ? "is-disabled" : "",
          data?.traversed ? "is-traversed" : "",
          data?.treeEdge ? "is-tree-edge" : "",
          data?.backEdge ? "is-back-edge" : "",
          data?.chainState ? `is-chain-${data.chainState}` : "",
        ]
          .filter(Boolean)
          .join(" ")}
      />
      <EdgeLabelRenderer>
        <button
          className={[
            "edge-label",
            selected ? "is-selected" : "",
            data?.traversed ? "is-traversed" : "",
            data?.chainState ? `is-chain-${data.chainState}` : "",
          ]
            .filter(Boolean)
            .join(" ")}
          style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
          tabIndex={-1}
        >
          {data?.experimentLabel ? (
            <>
              <span>{data.experimentLabel}</span>
              {typeof data.experimentDelta === "number" && (
                <small>
                  Δ {(data.experimentDelta * 100).toFixed(2)} pp
                </small>
              )}
            </>
          ) : data ? (
            edgeTypeLabels[data.type]
          ) : (
            "related"
          )}
        </button>
      </EdgeLabelRenderer>
    </>
  );
}

const nodeTypes = { researchNode: ResearchNodeCard };
const edgeTypes = { researchEdge: ResearchEdgeLine };

function downloadText(filename: string, content: string, mime = "application/json") {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

function stopEvent(event: React.SyntheticEvent) {
  event.stopPropagation();
}

function AppShell() {
  const [project, setProject] = useState<ProjectState>(() => cloneProject(initialProject));
  const [suggestions, setSuggestions] = useState<GraphSuggestion[]>(() =>
    JSON.parse(JSON.stringify(initialSuggestions)),
  );
  const [selectedNodeId, setSelectedNodeId] = useState<string>("m2");
  const [selectedEdgeId, setSelectedEdgeId] = useState<string>("");
  const [activeScenarioId, setActiveScenarioId] = useState<string>("");
  const [activeInspectorTab, setActiveInspectorTab] = useState<
    "properties" | "evidence" | "suggestions"
  >("properties");
  const [bottomTab, setBottomTab] = useState<
    "traversal" | "scenario" | "influence" | "performance" | "activity"
  >(
    "traversal",
  );
  const [leftCollapsed, setLeftCollapsed] = useState(false);
  const [rightCollapsed, setRightCollapsed] = useState(false);
  const [bottomExpanded, setBottomExpanded] = useState(true);
  const [searchQuery, setSearchQuery] = useState("");
  const [nodeTypeFilter, setNodeTypeFilter] = useState<ResearchNodeType | "all">("all");
  const [canvasFilterOpen, setCanvasFilterOpen] = useState(false);
  const [canvasNodeTypes, setCanvasNodeTypes] = useState<ResearchNodeType[]>([]);
  const [canvasEdgeTypes, setCanvasEdgeTypes] = useState<ResearchEdgeType[]>([]);
  const [minimumConfidence, setMinimumConfidence] = useState(0);
  const [experimentsOnly, setExperimentsOnly] = useState(false);
  const [layoutMode, setLayoutMode] = useState<LayoutMode>("neural-network");
  const [layoutAnnotations, setLayoutAnnotations] = useState<Record<string, string>>({});
  const [logicChain, setLogicChain] = useState<LogicChainResult | null>(null);
  const [logicChainMode, setLogicChainMode] = useState<LogicChainMode>("effective");
  const [influence, setInfluence] = useState<InfluenceResult | null>(null);
  const [zenMode, setZenMode] = useState(false);
  const [themeId, setThemeId] = useState("research-light");
  const [snapEnabled, setSnapEnabled] = useState(true);
  const [trackpadPan, setTrackpadPan] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pluginStoreOpen, setPluginStoreOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [loadedPlugins, setLoadedPlugins] = useState<string[]>(["git-experiments"]);
  const [performanceStats, setPerformanceStats] = useState({
    fps: 60,
    frameMs: 16.7,
    heapMb: 0,
    nodeCount: 0,
    edgeCount: 0,
  });
  const [traversalStrategy, setTraversalStrategy] = useState<"bfs" | "dfs">("bfs");
  const [traversalDirection, setTraversalDirection] =
    useState<"in" | "out" | "both">("out");
  const [maxDepth, setMaxDepth] = useState(4);
  const [edgeTypeFilter, setEdgeTypeFilter] = useState<ResearchEdgeType[]>([]);
  const [traversal, setTraversal] = useState<TraversalResult | null>(null);
  const [cycles, setCycles] = useState<ReturnType<typeof detectCycles>>([]);
  const [saveState, setSaveState] = useState<"saved" | "saving">("saved");
  const [past, setPast] = useState<HistoryEntry[]>([]);
  const [future, setFuture] = useState<HistoryEntry[]>([]);
  const [modal, setModal] = useState<
    "new-node" | "new-edge" | "evidence" | "split-node" | null
  >(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [toast, setToast] = useState("");
  const [newNode, setNewNode] = useState({
    title: "",
    type: "variable" as ResearchNodeType,
    body: "",
  });
  const [newEdge, setNewEdge] = useState({
    source: "m2",
    target: "r1",
    type: "depends_on" as ResearchEdgeType,
  });
  const [newEvidence, setNewEvidence] = useState({
    title: "",
    page: "",
    section: "",
    quote: "",
    url: "",
  });
  const [splitParts, setSplitParts] = useState({ first: "", second: "" });
  const dragSnapshot = useRef<ProjectState | null>(null);
  const searchInput = useRef<HTMLInputElement | null>(null);
  const importInput = useRef<HTMLInputElement | null>(null);
  const runResultInput = useRef<HTMLInputElement | null>(null);
  const { fitView, setCenter } = useReactFlow<CanvasNode, CanvasEdge>();
  const activeTheme = themeCatalog.find((theme) => theme.id === themeId) ?? themeCatalog[0];
  const themeStyle = {
    "--app-bg": activeTheme.colors.app,
    "--panel": activeTheme.colors.panel,
    "--canvas": activeTheme.colors.canvas,
    "--ink": activeTheme.colors.text,
    "--text": activeTheme.colors.text,
    "--muted": activeTheme.colors.muted,
    "--accent": activeTheme.colors.accent,
    "--border": activeTheme.colors.border,
    "--line": activeTheme.colors.border,
  } as CSSProperties;

  useEffect(() => {
    const handle = window.setTimeout(() => {
      try {
        const saved = localStorage.getItem("research-canvas-project-v1");
        const savedSuggestions = localStorage.getItem("research-canvas-suggestions-v1");
        if (saved) {
          const restored = migrateProject(JSON.parse(saved));
          setProject(restored);
          setSelectedNodeId(
            restored.nodes.find((node) => node.type === "question")?.id ??
              restored.nodes[0]?.id ??
              "",
          );
          setSelectedEdgeId("");
        }
        if (savedSuggestions) {
          setSuggestions(JSON.parse(savedSuggestions) as GraphSuggestion[]);
        }
      } catch {
        setToast("Local recovery data was invalid. Opened the verified demo instead.");
      }
    }, 0);
    return () => window.clearTimeout(handle);
  }, []);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      localStorage.setItem("research-canvas-project-v1", JSON.stringify(project));
      localStorage.setItem("research-canvas-suggestions-v1", JSON.stringify(suggestions));
      setSaveState("saved");
    }, 450);
    return () => window.clearTimeout(handle);
  }, [project, suggestions]);

  useEffect(() => {
    if (!toast) return;
    const handle = window.setTimeout(() => setToast(""), 2600);
    return () => window.clearTimeout(handle);
  }, [toast]);

  useEffect(() => {
    let frame = 0;
    let frames = 0;
    let sampleStarted = performance.now();
    const sample = (now: number) => {
      frames += 1;
      if (now - sampleStarted >= 1000) {
        const frameMs = (now - sampleStarted) / Math.max(1, frames);
        const memory = (
          performance as Performance & { memory?: { usedJSHeapSize: number } }
        ).memory;
        setPerformanceStats({
          fps: Math.round(1000 / frameMs),
          frameMs: Number(frameMs.toFixed(1)),
          heapMb: memory ? Math.round(memory.usedJSHeapSize / 1024 / 1024) : 0,
          nodeCount: project.nodes.length,
          edgeCount: project.edges.length,
        });
        frames = 0;
        sampleStarted = now;
      }
      frame = requestAnimationFrame(sample);
    };
    frame = requestAnimationFrame(sample);
    return () => cancelAnimationFrame(frame);
  }, [project.nodes.length, project.edges.length]);

  const selectedNode = project.nodes.find((node) => node.id === selectedNodeId);
  const selectedEdge = project.edges.find((edge) => edge.id === selectedEdgeId);
  const activeScenario = project.scenarios.find((scenario) => scenario.id === activeScenarioId);

  const commit = useCallback(
    (label: string, mutator: (draft: ProjectState) => void, origin: "human" | "ai" | "python" = "human") => {
      setSaveState("saving");
      setProject((current) => {
        const before = cloneProject(current);
        const next = cloneProject(current);
        mutator(next);
        next.revision += 1;
        next.updatedAt = new Date().toISOString();
        next.activity.unshift({
          id: makeId("activity"),
          label,
          origin,
          createdAt: next.updatedAt,
        });
        setPast((items) => [...items.slice(-39), { label, snapshot: before }]);
        setFuture([]);
        return next;
      });
    },
    [],
  );

  const undo = useCallback(() => {
    setPast((entries) => {
      const entry = entries.at(-1);
      if (!entry) return entries;
      setProject((current) => {
        setFuture((items) => [
          ...items,
          { label: entry.label, snapshot: cloneProject(current) },
        ]);
        return cloneProject(entry.snapshot);
      });
      setSelectedNodeId((current) =>
        entry.snapshot.nodes.some((node) => node.id === current)
          ? current
          : (entry.snapshot.nodes.find((node) => node.type === "question")?.id ??
            entry.snapshot.nodes[0]?.id ??
            ""),
      );
      setSelectedEdgeId((current) =>
        entry.snapshot.edges.some((edge) => edge.id === current) ? current : "",
      );
      setToast(`Undid: ${entry.label}`);
      return entries.slice(0, -1);
    });
  }, []);

  const redo = useCallback(() => {
    setFuture((entries) => {
      const entry = entries.at(-1);
      if (!entry) return entries;
      setProject((current) => {
        setPast((items) => [...items, { label: entry.label, snapshot: cloneProject(current) }]);
        return cloneProject(entry.snapshot);
      });
      setSelectedNodeId((current) =>
        entry.snapshot.nodes.some((node) => node.id === current)
          ? current
          : (entry.snapshot.nodes.find((node) => node.type === "question")?.id ??
            entry.snapshot.nodes[0]?.id ??
            ""),
      );
      setSelectedEdgeId((current) =>
        entry.snapshot.edges.some((edge) => edge.id === current) ? current : "",
      );
      setToast(`Redid: ${entry.label}`);
      return entries.slice(0, -1);
    });
  }, []);

  const traversalNodes = useMemo(() => new Set(traversal?.order ?? []), [traversal]);
  const traversalEdges = useMemo(() => new Set(traversal?.edgeIds ?? []), [traversal]);
  const treeEdges = useMemo(() => new Set(traversal?.treeEdgeIds ?? []), [traversal]);
  const backEdges = useMemo(() => new Set(traversal?.backEdgeIds ?? []), [traversal]);
  const disabledNodes = useMemo(
    () => new Set(activeScenario?.disabledNodeIds ?? []),
    [activeScenario],
  );
  const disabledEdges = useMemo(
    () => new Set(activeScenario?.disabledEdgeIds ?? []),
    [activeScenario],
  );
  const logicNodeIds = useMemo(() => new Set(logicChain?.nodeIds ?? []), [logicChain]);
  const logicEdgeIds = useMemo(() => new Set(logicChain?.edgeIds ?? []), [logicChain]);
  const visibleNodeIds = useMemo(() => {
    const typeSet = new Set(canvasNodeTypes);
    return new Set(
      project.nodes
        .filter((node) => !typeSet.size || typeSet.has(node.type))
        .map((node) => node.id),
    );
  }, [project.nodes, canvasNodeTypes]);
  const visibleEdgeRecords = useMemo(() => {
    const edgeSet = new Set(canvasEdgeTypes);
    return project.edges.filter(
      (edge) =>
        visibleNodeIds.has(edge.source) &&
        visibleNodeIds.has(edge.target) &&
        (!edgeSet.size || edgeSet.has(edge.type)) &&
        (edge.confidence ?? 0) >= minimumConfidence &&
        (!experimentsOnly || Boolean(edge.experiment)),
    );
  }, [
    project.edges,
    visibleNodeIds,
    canvasEdgeTypes,
    minimumConfidence,
    experimentsOnly,
  ]);

  const flowNodes = useMemo<CanvasNode[]>(
    () =>
      project.nodes.filter((record) => visibleNodeIds.has(record.id)).map((record) => {
        const placement = project.placements.find(
          (item) => item.nodeId === record.id && item.viewId === "view-main",
        );
        const influenceScore = influence?.scores[record.id];
        return {
          id: record.id,
          type: "researchNode",
          position: { x: placement?.x ?? 0, y: placement?.y ?? 0 },
          data: {
            record,
            disabled: disabledNodes.has(record.id),
            traversed: traversalNodes.has(record.id),
            depth: traversal?.depth[record.id],
            chainState: logicNodeIds.has(record.id) ? logicChain?.mode : undefined,
            annotation: layoutAnnotations[record.id],
            influence:
              typeof influenceScore === "number" &&
              Math.abs(influenceScore) >= 0.001 &&
              (record.id === influence?.targetId ||
                (record.type === "variable" &&
                  record.data.role !== "control" &&
                  record.data.role !== "input"))
                ? influenceScore
                : undefined,
          },
          selected: selectedNodeId === record.id,
          draggable: true,
          width: placement?.width ?? 230,
          height: placement?.height ?? 116,
          zIndex: selectedNodeId === record.id ? 20 : 1,
        };
      }),
    [
      project.nodes,
      project.placements,
      disabledNodes,
      traversalNodes,
      traversal,
      selectedNodeId,
      visibleNodeIds,
      logicNodeIds,
      logicChain,
      layoutAnnotations,
      influence,
    ],
  );

  const flowEdges = useMemo<CanvasEdge[]>(
    () =>
      visibleEdgeRecords.map((record) => {
        const scenarioDisabled =
          disabledEdges.has(record.id) ||
          disabledNodes.has(record.source) ||
          disabledNodes.has(record.target);
        return {
          id: record.id,
          type: "researchEdge",
          source: record.source,
          target: record.target,
          selected: selectedEdgeId === record.id,
          markerEnd: {
            type: MarkerType.ArrowClosed,
            width: 15,
            height: 15,
            color: scenarioDisabled ? "#9aa4b2" : "#667085",
          },
          data: {
            type: record.type,
            confidence: record.confidence,
            disabled: scenarioDisabled,
            traversed: traversalEdges.has(record.id),
            treeEdge: treeEdges.has(record.id),
            backEdge: backEdges.has(record.id),
            chainState: logicEdgeIds.has(record.id) ? logicChain?.mode : undefined,
            experimentLabel: record.experiment?.label,
            experimentDelta: record.experiment?.delta,
          },
        };
      }),
    [
      visibleEdgeRecords,
      disabledEdges,
      disabledNodes,
      selectedEdgeId,
      traversalEdges,
      treeEdges,
      backEdges,
      logicEdgeIds,
      logicChain,
    ],
  );

  const filteredNodes = useMemo(() => {
    const normalized = searchQuery.trim().toLowerCase();
    return project.nodes.filter((node) => {
      const matchesType = nodeTypeFilter === "all" || node.type === nodeTypeFilter;
      const matchesText =
        !normalized ||
        `${node.title} ${node.body} ${node.tags.join(" ")}`.toLowerCase().includes(normalized);
      return matchesType && matchesText;
    });
  }, [project.nodes, searchQuery, nodeTypeFilter]);

  const scenarioDiff = useMemo(
    () => {
      const rootId =
        project.nodes.find((node) => node.type === "question")?.id ?? project.nodes[0]?.id;
      return activeScenarioId && rootId
        ? compareScenarioReachability(project, rootId, activeScenarioId)
        : null;
    },
    [project, activeScenarioId],
  );

  const onNodesChange = useCallback((changes: NodeChange<CanvasNode>[]) => {
    const positionChanges = changes.filter(
      (change): change is Extract<NodeChange<CanvasNode>, { type: "position" }> =>
        change.type === "position" && Boolean(change.position),
    );
    if (positionChanges.length) {
      setProject((current) => {
        const next = cloneProject(current);
        for (const change of positionChanges) {
          const placement = next.placements.find((item) => item.nodeId === change.id);
          if (placement && change.position) {
            placement.x = change.position.x;
            placement.y = change.position.y;
          }
        }
        return next;
      });
    }
    for (const change of changes) {
      if (change.type === "select" && change.selected) {
        setSelectedNodeId(change.id);
        setSelectedEdgeId("");
      }
      if (change.type === "remove") {
        commit("Delete node", (draft) => {
          draft.nodes = draft.nodes.filter((node) => node.id !== change.id);
          draft.edges = draft.edges.filter(
            (edge) => edge.source !== change.id && edge.target !== change.id,
          );
          draft.placements = draft.placements.filter((item) => item.nodeId !== change.id);
        });
      }
    }
  }, [commit]);

  const onConnect = useCallback(
    (connection: Connection) => {
      if (!connection.source || !connection.target || connection.source === connection.target) {
        setToast("Choose two different nodes to create a relation.");
        return;
      }
      commit("Connect nodes", (draft) => {
        draft.edges.push({
          id: makeId("edge"),
          source: connection.source!,
          target: connection.target!,
          type: "depends_on",
          directed: true,
          polarity: "unknown",
          confidence: 1,
          conditions: [],
          evidenceIds: [],
          provenance: { origin: "human", actorId: "local-researcher" },
        });
      });
      setToast("Relation created as “depends on”. Edit it in the inspector.");
    },
    [commit],
  );

  const focusNode = useCallback(
    (nodeId: string) => {
      const placement = project.placements.find((item) => item.nodeId === nodeId);
      if (!placement) return;
      setSelectedNodeId(nodeId);
      setSelectedEdgeId("");
      setCenter(placement.x + placement.width / 2, placement.y + placement.height / 2, {
        zoom: 1,
        duration: 450,
      });
    },
    [project.placements, setCenter],
  );

  const runTraversal = useCallback(() => {
    const startId = selectedNodeId || "q1";
    const result = traverseGraph(project, {
      startId,
      strategy: traversalStrategy,
      direction: traversalDirection,
      maxDepth,
      edgeTypes: edgeTypeFilter.length ? edgeTypeFilter : undefined,
      nodeTypes: nodeTypeFilter === "all" ? undefined : [nodeTypeFilter],
      scenarioId: activeScenarioId || undefined,
    });
    setTraversal(result);
    setBottomTab("traversal");
    setBottomExpanded(true);
    setToast(`${traversalStrategy.toUpperCase()} found ${result.order.length} nodes.`);
  }, [
    selectedNodeId,
    project,
    traversalStrategy,
    traversalDirection,
    maxDepth,
    edgeTypeFilter,
    nodeTypeFilter,
    activeScenarioId,
  ]);

  const runCycleDetection = useCallback(() => {
    const found = detectCycles(project, activeScenarioId || undefined);
    setCycles(found);
    setBottomTab("traversal");
    setBottomExpanded(true);
    setToast(found.length ? `Found ${found.length} directed cycle.` : "No directed cycles found.");
  }, [project, activeScenarioId]);

  const applyTreeLayout = useCallback(() => {
    const result = computeLayout(project, layoutMode, selectedNodeId || project.nodes[0]?.id);
    commit(`Apply ${layoutMode} layout`, (draft) => {
      for (const [nodeId, position] of Object.entries(result.positions)) {
        const placement = draft.placements.find((item) => item.nodeId === nodeId);
        if (placement && !placement.pinned) {
          placement.x = position.x;
          placement.y = position.y;
        }
      }
    });
    setLayoutAnnotations(result.annotations);
    window.setTimeout(() => fitView({ padding: 0.15, duration: 500 }), 60);
    setToast(`${layoutMode.replaceAll("-", " ")} layout applied.`);
  }, [project, layoutMode, selectedNodeId, commit, fitView]);

  const highlightLogicChain = useCallback(() => {
    const result = computeLogicChain(
      project,
      logicChainMode,
      selectedNodeId || project.nodes.find((node) => node.type === "result")?.id,
    );
    setLogicChain(result);
    setBottomTab("influence");
    setBottomExpanded(true);
    setToast(result.summary);
  }, [project, logicChainMode, selectedNodeId]);

  const runInfluencePropagation = useCallback(() => {
    const targetId =
      selectedNodeId ||
      project.nodes.find((node) => node.type === "metric")?.id ||
      project.nodes.at(-1)?.id;
    if (!targetId) return;
    const result = propagateInfluence(project, targetId);
    setInfluence(result);
    setBottomTab("influence");
    setBottomExpanded(true);
    setToast(`BP-like influence propagated to ${Object.keys(result.scores).length} variables.`);
  }, [project, selectedNodeId]);

  const toggleZenMode = useCallback(() => {
    setZenMode((current) => !current);
    window.setTimeout(() => fitView({ padding: 0.14, duration: 450 }), 80);
  }, [fitView]);

  const loadMnistStudy = useCallback(() => {
    const next = createMnistProject();
    setProject(cloneProject(next));
    setPast([]);
    setFuture([]);
    setTraversal(null);
    setInfluence(null);
    setLogicChain(null);
    setSelectedNodeId("mnist-accuracy");
    setSelectedEdgeId("");
    setLayoutMode("neural-network");
    setLoadedPlugins((items) =>
      items.includes("git-experiments") ? items : [...items, "git-experiments"],
    );
    setPluginStoreOpen(false);
    setSaveState("saving");
    window.setTimeout(() => {
      const layout = computeLayout(next, "neural-network", "mnist-question");
      setProject((current) => {
        const laidOut = cloneProject(current);
        for (const [nodeId, position] of Object.entries(layout.positions)) {
          const placement = laidOut.placements.find((item) => item.nodeId === nodeId);
          if (placement) Object.assign(placement, position);
        }
        return laidOut;
      });
      setLayoutAnnotations(layout.annotations);
      setLogicChain(computeLogicChain(next, "effective", "mnist-conclusion"));
      setInfluence(propagateInfluence(next, "mnist-accuracy"));
      setBottomTab("influence");
      setBottomExpanded(true);
      fitView({ padding: 0.16, duration: 550 });
    }, 80);
    setToast(`Git plugin loaded MNIST study at ${mnistRunSummary.environment.gitCommit}.`);
  }, [fitView]);

  const splitSelectedNode = useCallback(() => {
    if (!selectedNode || !splitParts.first.trim() || !splitParts.second.trim()) return;
    const firstId = makeId("split");
    const secondId = makeId("split");
    commit("Split research node", (draft) => {
      const source = draft.nodes.find((node) => node.id === selectedNode.id);
      if (!source) return;
      source.status = "deprecated";
      const base = {
        ...source,
        status: "draft" as const,
        evidenceIds: [...source.evidenceIds],
        tags: [...source.tags, "split"],
        provenance: {
          origin: "human" as const,
          actorId: "local-researcher",
          sourceRefs: [`split-from:${source.id}`],
        },
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      };
      draft.nodes.push(
        { ...base, id: firstId, title: splitParts.first.trim() },
        { ...base, id: secondId, title: splitParts.second.trim() },
      );
      const placement = draft.placements.find((item) => item.nodeId === source.id);
      const x = placement?.x ?? 300;
      const y = placement?.y ?? 220;
      draft.placements.push(
        {
          id: `placement-${firstId}`,
          viewId: "view-main",
          nodeId: firstId,
          x: x + 270,
          y: y - 70,
          width: placement?.width ?? 230,
          height: placement?.height ?? 116,
        },
        {
          id: `placement-${secondId}`,
          viewId: "view-main",
          nodeId: secondId,
          x: x + 270,
          y: y + 90,
          width: placement?.width ?? 230,
          height: placement?.height ?? 116,
        },
      );
      draft.edges.push(
        {
          id: makeId("edge"),
          source: source.id,
          target: firstId,
          type: "derived_from",
          directed: true,
          polarity: "unknown",
          confidence: 1,
          conditions: [],
          evidenceIds: [...source.evidenceIds],
          provenance: { origin: "human", actorId: "local-researcher" },
        },
        {
          id: makeId("edge"),
          source: source.id,
          target: secondId,
          type: "derived_from",
          directed: true,
          polarity: "unknown",
          confidence: 1,
          conditions: [],
          evidenceIds: [...source.evidenceIds],
          provenance: { origin: "human", actorId: "local-researcher" },
        },
      );
    });
    setSelectedNodeId(firstId);
    setSplitParts({ first: "", second: "" });
    setModal(null);
    setToast("Node split into two reviewable derived nodes.");
  }, [selectedNode, splitParts, commit]);

  const createNode = useCallback(() => {
    if (!newNode.title.trim()) return;
    const id = makeId("node");
    commit("Create node", (draft) => {
      draft.nodes.push({
        id,
        type: newNode.type,
        title: newNode.title.trim(),
        body: newNode.body.trim() || "Add a precise research definition.",
        tags: [],
        status: "draft",
        evidenceIds: [],
        data: {},
        provenance: { origin: "human", actorId: "local-researcher" },
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      });
      draft.placements.push({
        id: `placement-${id}`,
        viewId: "view-main",
        nodeId: id,
        x: 460 + (draft.nodes.length % 4) * 34,
        y: 210 + (draft.nodes.length % 5) * 42,
        width: 230,
        height: 116,
      });
    });
    setSelectedNodeId(id);
    setNewNode({ title: "", type: "variable", body: "" });
    setModal(null);
  }, [newNode, commit]);

  const createEdge = useCallback(() => {
    if (!newEdge.source || !newEdge.target || newEdge.source === newEdge.target) return;
    commit("Create typed relation", (draft) => {
      draft.edges.push({
        id: makeId("edge"),
        ...newEdge,
        directed: true,
        polarity: "unknown",
        confidence: 1,
        conditions: [],
        evidenceIds: [],
        provenance: { origin: "human", actorId: "local-researcher" },
      });
    });
    setModal(null);
  }, [newEdge, commit]);

  const createEvidence = useCallback(() => {
    if (!selectedNode || !newEvidence.title.trim()) return;
    const evidenceId = makeId("evidence");
    commit("Attach evidence", (draft) => {
      draft.evidence.push({
        id: evidenceId,
        sourceType: "paper",
        sourceId: makeId("source"),
        title: newEvidence.title.trim(),
        url: newEvidence.url.trim() || undefined,
        locator: {
          page: newEvidence.page ? Number(newEvidence.page) : undefined,
          section: newEvidence.section.trim() || undefined,
          quote: newEvidence.quote.trim() || undefined,
        },
        status: "verified",
        provenance: { origin: "human", actorId: "local-researcher" },
      });
      const node = draft.nodes.find((item) => item.id === selectedNode.id);
      node?.evidenceIds.push(evidenceId);
    });
    setNewEvidence({ title: "", page: "", section: "", quote: "", url: "" });
    setModal(null);
    setActiveInspectorTab("evidence");
  }, [selectedNode, newEvidence, commit]);

  const createScenario = useCallback(() => {
    if (!selectedNode) {
      setToast("Select a node before creating an ablation scenario.");
      return;
    }
    const id = makeId("scenario");
    commit("Create ablation scenario", (draft) => {
      draft.scenarios.push({
        id,
        name: `Without ${selectedNode.title}`,
        disabledNodeIds: [selectedNode.id],
        disabledEdgeIds: [],
        nodeOverrides: {},
        edgeOverrides: {},
        parameters: { matchedCompute: true, seeds: [13, 21, 34] },
        hypothesis: `Disabling ${selectedNode.title} changes its downstream reachable results.`,
        expectedEffect: "Review the structural impact before running an external experiment.",
        createdAt: new Date().toISOString(),
      });
    });
    setActiveScenarioId(id);
    setBottomTab("scenario");
    setBottomExpanded(true);
    setToast("Scenario created without changing the base graph.");
  }, [selectedNode, commit]);

  const acceptSuggestion = useCallback(
    (suggestion: GraphSuggestion) => {
      commit(
        `Accept AI suggestion: ${suggestion.title}`,
        (draft) => {
          if (suggestion.kind === "node" && suggestion.node) {
            const id = makeId("node");
            draft.nodes.push({
              ...suggestion.node,
              id,
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
              provenance: {
                ...suggestion.node.provenance,
                reviewedBy: "local-researcher",
                reviewedAt: new Date().toISOString(),
              },
            });
            draft.placements.push({
              id: `placement-${id}`,
              viewId: "view-main",
              nodeId: id,
              x: 700,
              y: 720 + draft.nodes.length * 8,
              width: 230,
              height: 116,
            });
          }
          if (suggestion.kind === "edge" && suggestion.edge) {
            draft.edges.push({
              ...suggestion.edge,
              id: makeId("edge"),
              provenance: {
                ...suggestion.edge.provenance,
                reviewedBy: "local-researcher",
                reviewedAt: new Date().toISOString(),
              },
            });
          }
        },
        "ai",
      );
      setSuggestions((items) =>
        items.map((item) =>
          item.id === suggestion.id ? { ...item, status: "accepted" } : item,
        ),
      );
      setToast("Suggestion accepted as one reversible transaction.");
    },
    [commit],
  );

  const acceptAllSuggestions = useCallback(() => {
    const proposed = suggestions.filter((item) => item.status === "proposed");
    if (!proposed.length) return;
    commit(
      `Accept ${proposed.length} AI suggestions`,
      (draft) => {
        proposed.forEach((suggestion, index) => {
          if (suggestion.kind === "node" && suggestion.node) {
            const id = makeId(`node-${index}`);
            draft.nodes.push({
              ...suggestion.node,
              id,
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
              provenance: {
                ...suggestion.node.provenance,
                reviewedBy: "local-researcher",
                reviewedAt: new Date().toISOString(),
              },
            });
            draft.placements.push({
              id: `placement-${id}`,
              viewId: "view-main",
              nodeId: id,
              x: 720,
              y: 740 + index * 150,
              width: 230,
              height: 116,
            });
          }
          if (suggestion.kind === "edge" && suggestion.edge) {
            draft.edges.push({ ...suggestion.edge, id: makeId(`edge-${index}`) });
          }
        });
      },
      "ai",
    );
    setSuggestions((items) =>
      items.map((item) =>
        item.status === "proposed" ? { ...item, status: "accepted" } : item,
      ),
    );
    setToast(`${proposed.length} suggestions accepted in one transaction.`);
  }, [suggestions, commit]);

  const rejectSuggestion = useCallback((suggestionId: string) => {
    setSuggestions((items) =>
      items.map((item) =>
        item.id === suggestionId ? { ...item, status: "rejected" } : item,
      ),
    );
    setToast("Suggestion rejected. The formal graph was unchanged.");
  }, []);

  const updateSelectedNode = useCallback(
    (field: "title" | "body", value: string) => {
      if (!selectedNode || value === selectedNode[field]) return;
      commit(`Edit node ${field}`, (draft) => {
        const node = draft.nodes.find((item) => item.id === selectedNode.id);
        if (node) {
          node[field] = value;
          node.updatedAt = new Date().toISOString();
        }
      });
    },
    [selectedNode, commit],
  );

  const duplicateSelected = useCallback(() => {
    if (!selectedNode) return;
    const id = makeId("node");
    commit("Duplicate node", (draft) => {
      draft.nodes.push({
        ...selectedNode,
        id,
        title: `${selectedNode.title} copy`,
        status: "draft",
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      });
      const placement = draft.placements.find((item) => item.nodeId === selectedNode.id);
      draft.placements.push({
        ...(placement ?? {
          id: "",
          viewId: "view-main",
          nodeId: id,
          x: 300,
          y: 300,
          width: 230,
          height: 116,
        }),
        id: `placement-${id}`,
        nodeId: id,
        x: (placement?.x ?? 300) + 36,
        y: (placement?.y ?? 300) + 36,
      });
    });
    setSelectedNodeId(id);
  }, [selectedNode, commit]);

  const deleteSelected = useCallback(() => {
    if (selectedNodeId) {
      commit("Delete node and incident relations", (draft) => {
        draft.nodes = draft.nodes.filter((node) => node.id !== selectedNodeId);
        draft.edges = draft.edges.filter(
          (edge) => edge.source !== selectedNodeId && edge.target !== selectedNodeId,
        );
        draft.placements = draft.placements.filter((item) => item.nodeId !== selectedNodeId);
      });
      setSelectedNodeId("");
      return;
    }
    if (selectedEdgeId) {
      commit("Delete relation", (draft) => {
        draft.edges = draft.edges.filter((edge) => edge.id !== selectedEdgeId);
      });
      setSelectedEdgeId("");
    }
  }, [selectedNodeId, selectedEdgeId, commit]);

  const handleProjectImport = useCallback((event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const imported = migrateProject(JSON.parse(String(reader.result)));
        setPast((items) => [...items, { label: "Import project", snapshot: cloneProject(project) }]);
        setProject(imported);
        setFuture([]);
        setSelectedNodeId(
          imported.nodes.find((node) => node.type === "question")?.id ??
            imported.nodes[0]?.id ??
            "",
        );
        setSelectedEdgeId("");
        setActiveScenarioId("");
        setToast(`Imported ${imported.nodes.length} research nodes.`);
      } catch {
        setToast("Could not import: unsupported project JSON.");
      }
    };
    reader.readAsText(file);
    event.target.value = "";
  }, [project]);

  const handleRunResultImport = useCallback((event: ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      try {
        const result = JSON.parse(String(reader.result)) as {
          scenarioId?: string;
          metric?: string;
          value?: number;
          summary?: string;
        };
        const id = makeId("result");
        commit(
          "Import external run result",
          (draft) => {
            draft.nodes.push({
              id,
              type: "result",
              title: result.metric ? `${result.metric}: ${result.value ?? "complete"}` : "Imported run result",
              body: result.summary ?? "External result imported through the connector protocol.",
              tags: ["run-result", result.scenarioId ?? activeScenarioId].filter(Boolean),
              status: "draft",
              evidenceIds: [],
              data: result,
              provenance: { origin: "python", sourceRefs: [file.name] },
              createdAt: new Date().toISOString(),
              updatedAt: new Date().toISOString(),
            });
            draft.placements.push({
              id: `placement-${id}`,
              viewId: "view-main",
              nodeId: id,
              x: 1260,
              y: 650,
              width: 230,
              height: 116,
            });
          },
          "python",
        );
        setSelectedNodeId(id);
        setToast("Run result imported as a reviewable result node.");
      } catch {
        setToast("Could not import: invalid RunResult JSON.");
      }
    };
    reader.readAsText(file);
    event.target.value = "";
  }, [commit, activeScenarioId]);

  const toggleEdgeFilter = (type: ResearchEdgeType) => {
    setEdgeTypeFilter((current) =>
      current.includes(type) ? current.filter((item) => item !== type) : [...current, type],
    );
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const editing =
        target?.tagName === "INPUT" ||
        target?.tagName === "TEXTAREA" ||
        target?.tagName === "SELECT" ||
        target?.isContentEditable;
      const command = event.metaKey || event.ctrlKey;
      if (command && event.key.toLowerCase() === "k") {
        event.preventDefault();
        searchInput.current?.focus();
        return;
      }
      if (command && event.shiftKey && event.key.toLowerCase() === "p") {
        event.preventDefault();
        setPluginStoreOpen(true);
        return;
      }
      if (command && event.key === "Enter") {
        event.preventDefault();
        runTraversal();
        return;
      }
      if (event.key === "Escape") {
        setModal(null);
        setPluginStoreOpen(false);
        setSettingsOpen(false);
        setShortcutsOpen(false);
        setCanvasFilterOpen(false);
        return;
      }
      if (editing) return;
      if (event.key.toLowerCase() === "n") setModal("new-node");
      if (event.key.toLowerCase() === "e") setModal("new-edge");
      if (event.key.toLowerCase() === "f") fitView({ padding: 0.16, duration: 350 });
      if (event.key.toLowerCase() === "z" && !command) toggleZenMode();
      if (event.key === "?") setShortcutsOpen(true);
      if ((event.key === "Delete" || event.key === "Backspace") && (selectedNodeId || selectedEdgeId)) {
        event.preventDefault();
        deleteSelected();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [deleteSelected, fitView, runTraversal, selectedEdgeId, selectedNodeId, toggleZenMode]);

  return (
    <main
      className={`app-shell ${zenMode ? "zen-mode" : ""}`}
      data-testid="research-canvas-app"
      data-theme={activeTheme.id}
      style={themeStyle}
    >
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">
            <Network size={18} strokeWidth={2.4} />
          </div>
          <div>
            <div className="brand-name">Research Canvas</div>
            <div className="project-breadcrumb">
              Workspace <ChevronRight size={11} /> {project.title}
            </div>
          </div>
        </div>

        <div className="topbar-center">
          <span className="discipline-chip">{project.discipline}</span>
          <span className="save-status" aria-live="polite">
            <span className={`save-dot ${saveState}`} />
            {saveState === "saved" ? "Saved locally" : "Saving…"}
          </span>
        </div>

        <div className="topbar-actions">
          <button className="icon-button" onClick={undo} disabled={!past.length} aria-label="Undo">
            <Undo2 size={16} />
          </button>
          <button className="icon-button" onClick={redo} disabled={!future.length} aria-label="Redo">
            <Redo2 size={16} />
          </button>
          <div className="topbar-separator" />
          <div className="layout-control">
            <select
              value={layoutMode}
              onChange={(event) => setLayoutMode(event.target.value as LayoutMode)}
              aria-label="Layout mode"
              data-testid="layout-mode"
            >
              {LAYOUT_MODES.map((mode) => (
                <option key={mode} value={mode}>
                  {mode
                    .replace("huffman", "prefix Huffman")
                    .replaceAll("-", " ")
                    .replace(/\b\w/g, (letter) => letter.toUpperCase())}
                </option>
              ))}
            </select>
            <button className="toolbar-button" onClick={applyTreeLayout} data-testid="apply-layout">
              <LayoutDashboard size={15} />
              Layout
            </button>
          </div>
          <button
            className="icon-button"
            onClick={toggleZenMode}
            title="Zen mode (Z)"
            aria-label="Toggle zen mode"
          >
            <Focus size={16} />
          </button>
          <button
            className="icon-button"
            onClick={() => setPluginStoreOpen(true)}
            title="Plugin store (Ctrl/Cmd+Shift+P)"
            aria-label="Open plugin store"
          >
            <ShoppingBag size={16} />
          </button>
          <button
            className="icon-button"
            onClick={() => setSettingsOpen(true)}
            title="Settings"
            aria-label="Open settings"
          >
            <Settings2 size={16} />
          </button>
          <div className="export-wrap">
            <button
              className="primary-button"
              onClick={() => setExportOpen((value) => !value)}
              data-testid="export-button"
            >
              <Download size={15} />
              Export
              <ChevronDown size={13} />
            </button>
            {exportOpen && (
              <div className="export-menu">
                <button
                  onClick={() => {
                    downloadText(
                      "long-context-ablation.research.json",
                      JSON.stringify(project, null, 2),
                    );
                    setExportOpen(false);
                  }}
                >
                  <FileJson size={16} />
                  <span>
                    <strong>Project JSON</strong>
                    <small>Lossless semantic graph</small>
                  </span>
                </button>
                <button
                  data-testid="export-canvas"
                  onClick={() => {
                    downloadText(
                      "long-context-ablation.canvas",
                      JSON.stringify(exportJsonCanvas(project), null, 2),
                    );
                    setExportOpen(false);
                    setToast("Obsidian JSON Canvas exported.");
                  }}
                >
                  <Braces size={16} />
                  <span>
                    <strong>Obsidian Canvas</strong>
                    <small>JSON Canvas 1.0</small>
                  </span>
                </button>
                <button
                  onClick={() => {
                    downloadText(
                      "long-context-ablation.md",
                      exportMarkdown(project),
                      "text/markdown",
                    );
                    setExportOpen(false);
                  }}
                >
                  <FileText size={16} />
                  <span>
                    <strong>Research summary</strong>
                    <small>Markdown with evidence links</small>
                  </span>
                </button>
                <button
                  onClick={() => {
                    downloadText(
                      "research-nodes.csv",
                      exportCsv(project, "nodes"),
                      "text/csv;charset=utf-8",
                    );
                    setExportOpen(false);
                  }}
                >
                  <FileText size={16} />
                  <span>
                    <strong>Node table</strong>
                    <small>CSV with stable semantic IDs</small>
                  </span>
                </button>
                <button
                  onClick={() => {
                    downloadText(
                      "research-relations.csv",
                      exportCsv(project, "edges"),
                      "text/csv;charset=utf-8",
                    );
                    setExportOpen(false);
                  }}
                >
                  <FileText size={16} />
                  <span>
                    <strong>Relation table</strong>
                    <small>CSV with experiment outcomes</small>
                  </span>
                </button>
                <button
                  onClick={() => {
                    downloadText(
                      "scenario-run-manifest.json",
                      JSON.stringify(
                        {
                          protocolVersion: "research-canvas-run/1",
                          projectId: project.id,
                          projectRevision: project.revision,
                          scenario: activeScenario ?? null,
                          requestedAt: new Date().toISOString(),
                        },
                        null,
                        2,
                      ),
                    );
                    setExportOpen(false);
                  }}
                >
                  <ArrowUpFromLine size={16} />
                  <span>
                    <strong>Python run manifest</strong>
                    <small>Connector-ready JSON</small>
                  </span>
                </button>
              </div>
            )}
          </div>
        </div>
      </header>

      <div
        className={[
          "workspace",
          leftCollapsed ? "left-collapsed" : "",
          rightCollapsed ? "right-collapsed" : "",
          bottomExpanded ? "bottom-open" : "",
        ]
          .filter(Boolean)
          .join(" ")}
      >
        <aside className="left-sidebar">
          <div className="sidebar-head">
            <span>Navigator</span>
            <button
              className="icon-button ghost"
              onClick={() => setLeftCollapsed(true)}
              aria-label="Collapse navigator"
            >
              <PanelLeftClose size={16} />
            </button>
          </div>
          <div className="search-box">
            <Search size={15} />
            <input
              ref={searchInput}
              value={searchQuery}
              onChange={(event) => setSearchQuery(event.target.value)}
              placeholder="Search nodes, tags, notes…"
              aria-label="Search graph"
            />
            <kbd>⌘K</kbd>
          </div>

          <div className="sidebar-scroll">
            <section className="sidebar-section">
              <div className="section-label">
                Views <span>3</span>
              </div>
              <button className="nav-row active">
                <Network size={15} />
                Research map
                <span className="count-badge">{project.nodes.length}</span>
              </button>
              <button className="nav-row" onClick={applyTreeLayout}>
                <ListTree size={15} />
                Dependency tree
                <span className="muted-badge">BFS</span>
              </button>
              <button className="nav-row" onClick={() => setActiveInspectorTab("evidence")}>
                <FileText size={15} />
                Evidence map
                <span className="count-badge">{project.evidence.length}</span>
              </button>
            </section>

            <section className="sidebar-section">
              <div className="section-label">
                Scenarios <span>{project.scenarios.length + 1}</span>
              </div>
              <button
                className={`scenario-row ${activeScenarioId === "" ? "active" : ""}`}
                onClick={() => {
                  setActiveScenarioId("");
                  setBottomTab("scenario");
                }}
              >
                <span className="scenario-radio" />
                <span>
                  <strong>Base graph</strong>
                  <small>All components active</small>
                </span>
              </button>
              {project.scenarios.map((scenario) => (
                <button
                  key={scenario.id}
                  className={`scenario-row ${activeScenarioId === scenario.id ? "active" : ""}`}
                  onClick={() => {
                    setActiveScenarioId(scenario.id);
                    setBottomTab("scenario");
                    setBottomExpanded(true);
                  }}
                  data-testid={`scenario-${scenario.id}`}
                >
                  <span className="scenario-radio" />
                  <span>
                    <strong>{scenario.name}</strong>
                    <small>
                      {scenario.disabledNodeIds.length} node · {scenario.disabledEdgeIds.length} edge
                    </small>
                  </span>
                </button>
              ))}
              <button className="text-action" onClick={createScenario}>
                <Plus size={14} />
                Create from selection
              </button>
            </section>

            <section className="sidebar-section">
              <div className="section-label">
                Node types <Filter size={12} />
              </div>
              <button
                className={`type-filter ${nodeTypeFilter === "all" ? "active" : ""}`}
                onClick={() => setNodeTypeFilter("all")}
              >
                <span className="type-dot type-all" />
                All nodes
                <span>{project.nodes.length}</span>
              </button>
              {compactNodeTypes.map((type) => (
                <button
                  key={type}
                  className={`type-filter ${nodeTypeFilter === type ? "active" : ""}`}
                  onClick={() => setNodeTypeFilter(type)}
                >
                  <span className={`type-dot type-${type}`} />
                  {nodeTypeLabels[type]}
                  <span>{project.nodes.filter((node) => node.type === type).length}</span>
                </button>
              ))}
            </section>

            <section className="sidebar-section node-results">
              <div className="section-label">
                Nodes <span>{filteredNodes.length}</span>
              </div>
              {filteredNodes.slice(0, 8).map((node) => (
                <button
                  key={node.id}
                  className={`node-result ${selectedNodeId === node.id ? "active" : ""}`}
                  onClick={() => focusNode(node.id)}
                >
                  <span className={`type-dot type-${node.type}`} />
                  <span>{node.title}</span>
                </button>
              ))}
            </section>
          </div>
          <div className="sidebar-footer">
            <button onClick={() => importInput.current?.click()}>
              <Upload size={14} /> Import project
            </button>
            <button onClick={() => runResultInput.current?.click()}>
              <ArrowDownToLine size={14} /> RunResult
            </button>
          </div>
        </aside>

        {leftCollapsed && (
          <button
            className="panel-reopen left"
            onClick={() => setLeftCollapsed(false)}
            aria-label="Open navigator"
          >
            <ChevronRight size={16} />
          </button>
        )}

        <section className="canvas-region">
          <div className="canvas-toolbar">
            <button className="tool-button primary" onClick={() => setModal("new-node")}>
              <Plus size={15} />
              Node
            </button>
            <button className="tool-button" onClick={() => setModal("new-edge")}>
              <Link2 size={15} />
              Relation
            </button>
            <div className="canvas-filter-wrap">
              <button
                className={`tool-button ${canvasFilterOpen ? "active" : ""}`}
                onClick={() => setCanvasFilterOpen((value) => !value)}
                data-testid="canvas-filter"
              >
                <SlidersHorizontal size={15} />
                Filter
                {(canvasNodeTypes.length ||
                  canvasEdgeTypes.length ||
                  minimumConfidence ||
                  experimentsOnly) && <span className="filter-active-dot" />}
              </button>
              {canvasFilterOpen && (
                <div className="filter-popover" data-testid="filter-popover">
                  <div className="filter-popover-head">
                    <strong>Canvas filter</strong>
                    <button
                      onClick={() => {
                        setCanvasNodeTypes([]);
                        setCanvasEdgeTypes([]);
                        setMinimumConfidence(0);
                        setExperimentsOnly(false);
                      }}
                    >
                      Reset
                    </button>
                  </div>
                  <span className="eyebrow">Node types</span>
                  <div className="filter-chip-grid">
                    {compactNodeTypes.map((type) => (
                      <button
                        key={type}
                        className={canvasNodeTypes.includes(type) ? "active" : ""}
                        onClick={() =>
                          setCanvasNodeTypes((types) =>
                            types.includes(type)
                              ? types.filter((item) => item !== type)
                              : [...types, type],
                          )
                        }
                      >
                        {nodeTypeLabels[type]}
                      </button>
                    ))}
                  </div>
                  <span className="eyebrow">Relation types</span>
                  <div className="filter-chip-grid">
                    {["supports", "contradicts", "controls", "measures"].map((type) => (
                      <button
                        key={type}
                        className={
                          canvasEdgeTypes.includes(type as ResearchEdgeType) ? "active" : ""
                        }
                        onClick={() =>
                          setCanvasEdgeTypes((types) =>
                            types.includes(type as ResearchEdgeType)
                              ? types.filter((item) => item !== type)
                              : [...types, type as ResearchEdgeType],
                          )
                        }
                      >
                        {type}
                      </button>
                    ))}
                  </div>
                  <label className="filter-range">
                    Minimum confidence {Math.round(minimumConfidence * 100)}%
                    <input
                      type="range"
                      min="0"
                      max="1"
                      step="0.1"
                      value={minimumConfidence}
                      onChange={(event) => setMinimumConfidence(Number(event.target.value))}
                    />
                  </label>
                  <label className="check-row">
                    <input
                      type="checkbox"
                      checked={experimentsOnly}
                      onChange={(event) => setExperimentsOnly(event.target.checked)}
                    />
                    Only relations backed by experiments
                  </label>
                </div>
              )}
            </div>
            <div className="logic-control">
              <select
                value={logicChainMode}
                onChange={(event) => setLogicChainMode(event.target.value as LogicChainMode)}
                aria-label="Logic chain mode"
              >
                <option value="effective">Effective experiments</option>
                <option value="evidence">Evidence chain</option>
                <option value="refutation">Refutation chain</option>
              </select>
              <button className="tool-button" onClick={highlightLogicChain}>
                <Sparkles size={15} />
                Highlight chain
              </button>
            </div>
            <div className="toolbar-divider" />
            <button
              className="tool-button"
              onClick={duplicateSelected}
              disabled={!selectedNode}
              title="Duplicate selected node"
            >
              <GitFork size={15} />
              Duplicate
            </button>
            <button
              className="tool-button"
              onClick={() => {
                if (!selectedNode) return;
                setSplitParts({
                  first: `${selectedNode.title} · A`,
                  second: `${selectedNode.title} · B`,
                });
                setModal("split-node");
              }}
              disabled={!selectedNode}
              title="Split selected node"
            >
              <Scissors size={15} />
              Split
            </button>
            <button
              className="tool-button danger-ghost"
              onClick={deleteSelected}
              disabled={!selectedNodeId && !selectedEdgeId}
              title="Delete selection"
            >
              <Trash2 size={15} />
            </button>
            <div className="toolbar-spacer" />
            <button className="tool-button" onClick={() => fitView({ padding: 0.16, duration: 450 })}>
              <Maximize2 size={15} />
              Fit view
            </button>
            <span className="canvas-stat">
              {project.nodes.length} nodes · {project.edges.length} relations
            </span>
          </div>

          <ReactFlow<CanvasNode, CanvasEdge>
            nodes={flowNodes}
            edges={flowEdges}
            nodeTypes={nodeTypes}
            edgeTypes={edgeTypes}
            onNodesChange={onNodesChange}
            onConnect={onConnect}
            onNodeClick={(_, node) => {
              setSelectedNodeId(node.id);
              setSelectedEdgeId("");
              setActiveInspectorTab("properties");
            }}
            onEdgeClick={(_, edge) => {
              setSelectedEdgeId(edge.id);
              setSelectedNodeId("");
              setActiveInspectorTab("properties");
            }}
            onPaneClick={() => {
              setSelectedNodeId("");
              setSelectedEdgeId("");
            }}
            onNodeDragStart={() => {
              dragSnapshot.current = cloneProject(project);
            }}
            onNodeDragStop={() => {
              if (dragSnapshot.current) {
                setPast((items) => [
                  ...items.slice(-39),
                  { label: "Move node", snapshot: dragSnapshot.current! },
                ]);
                setFuture([]);
                dragSnapshot.current = null;
              }
            }}
            fitView
            fitViewOptions={{ padding: 0.14 }}
            minZoom={0.18}
            maxZoom={1.8}
            snapToGrid={snapEnabled}
            snapGrid={[16, 16]}
            selectionOnDrag
            panOnDrag={[1, 2]}
            panOnScroll={trackpadPan}
            zoomOnScroll={!trackpadPan}
            zoomOnPinch
            preventScrolling
            deleteKeyCode={null}
            multiSelectionKeyCode={["Meta", "Control"]}
            proOptions={{ hideAttribution: true }}
            className="research-flow"
          >
            <Background
              variant={BackgroundVariant.Dots}
              gap={20}
              size={1.25}
              color="#d7dce3"
            />
            <MiniMap
              className="research-minimap"
              nodeColor={(node) => {
                const type = (node.data as CanvasNodeData | undefined)?.record.type;
                const colors: Partial<Record<ResearchNodeType, string>> = {
                  question: "#7c5cff",
                  method: "#2c6bed",
                  variable: "#0f9f91",
                  hypothesis: "#b46b12",
                  result: "#c33f64",
                  metric: "#5c6ac4",
                  evidence: "#687588",
                };
                return colors[type ?? "note"] ?? "#8a94a4";
              }}
              maskColor="rgba(247, 248, 250, 0.78)"
              pannable
              zoomable
            />
            <Controls className="research-controls" showInteractive={false} />
          </ReactFlow>

          <div className="scenario-banner">
            <span className={`scenario-indicator ${activeScenarioId ? "active" : ""}`} />
            <span>
              <small>Viewing</small>
              <strong>{activeScenario?.name ?? "Base graph"}</strong>
            </span>
            {activeScenarioId && (
              <button onClick={() => setActiveScenarioId("")}>
                Return to base
              </button>
            )}
          </div>
        </section>

        <aside className="right-sidebar">
          <div className="inspector-heading">
            <div>
              <span className="eyebrow">Inspector</span>
              <strong>{selectedNode?.title ?? selectedEdge?.type ?? "No selection"}</strong>
            </div>
            <button
              className="icon-button ghost"
              onClick={() => setRightCollapsed(true)}
              aria-label="Collapse inspector"
            >
              <PanelRightClose size={16} />
            </button>
          </div>

          <div className="inspector-tabs">
            <button
              className={activeInspectorTab === "properties" ? "active" : ""}
              onClick={() => setActiveInspectorTab("properties")}
            >
              Properties
            </button>
            <button
              className={activeInspectorTab === "evidence" ? "active" : ""}
              onClick={() => setActiveInspectorTab("evidence")}
            >
              Evidence
              <span>{selectedNode?.evidenceIds.length ?? selectedEdge?.evidenceIds.length ?? 0}</span>
            </button>
            <button
              className={activeInspectorTab === "suggestions" ? "active" : ""}
              onClick={() => setActiveInspectorTab("suggestions")}
            >
              AI review
              <span className="suggestion-count">
                {suggestions.filter((item) => item.status === "proposed").length}
              </span>
            </button>
          </div>

          <div className="inspector-scroll">
            {activeInspectorTab === "properties" && selectedNode && (
              <div className="inspector-content">
                <label className="field-label">
                  Title
                  <input
                    key={`${selectedNode.id}-title`}
                    defaultValue={selectedNode.title}
                    onBlur={(event) => updateSelectedNode("title", event.target.value)}
                  />
                </label>
                <div className="field-grid">
                  <label className="field-label">
                    Type
                    <select
                      value={selectedNode.type}
                      onChange={(event) =>
                        commit("Change node type", (draft) => {
                          const node = draft.nodes.find((item) => item.id === selectedNode.id);
                          if (node) node.type = event.target.value as ResearchNodeType;
                        })
                      }
                    >
                      {NODE_TYPES.map((type) => (
                        <option key={type} value={type}>
                          {nodeTypeLabels[type]}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="field-label">
                    Status
                    <select
                      value={selectedNode.status}
                      onChange={(event) =>
                        commit("Change review status", (draft) => {
                          const node = draft.nodes.find((item) => item.id === selectedNode.id);
                          if (node) node.status = event.target.value as ResearchNode["status"];
                        })
                      }
                    >
                      <option value="draft">Draft</option>
                      <option value="confirmed">Confirmed</option>
                      <option value="disputed">Disputed</option>
                      <option value="deprecated">Deprecated</option>
                    </select>
                  </label>
                </div>
                <label className="field-label">
                  Research note
                  <textarea
                    key={`${selectedNode.id}-body`}
                    rows={5}
                    defaultValue={selectedNode.body}
                    onBlur={(event) => updateSelectedNode("body", event.target.value)}
                  />
                </label>
                <div className="tag-section">
                  <div className="field-label static">Tags</div>
                  <div className="tag-list">
                    {selectedNode.tags.map((tag) => (
                      <span key={tag}>{tag}</span>
                    ))}
                    <button>+ add</button>
                  </div>
                </div>
                <div className="provenance-card">
                  <div className="provenance-icon">
                    {selectedNode.provenance.origin === "ai" ? <Bot size={16} /> : <History size={16} />}
                  </div>
                  <div>
                    <strong>
                      {selectedNode.provenance.origin === "ai"
                        ? "AI suggested · human reviewed"
                        : "Human authored"}
                    </strong>
                    <small>
                      Stable ID {selectedNode.id} · revision {project.revision}
                    </small>
                  </div>
                </div>
                <div className="inspector-actions">
                  <button className="secondary-button" onClick={createScenario}>
                    <CircleDotDashed size={15} />
                    Create ablation
                  </button>
                  <button className="secondary-button" onClick={runTraversal}>
                    <GitBranch size={15} />
                    Trace impact
                  </button>
                  <button
                    className="secondary-button"
                    onClick={() => {
                      setSplitParts({
                        first: `${selectedNode.title} · A`,
                        second: `${selectedNode.title} · B`,
                      });
                      setModal("split-node");
                    }}
                  >
                    <Scissors size={15} />
                    Split node
                  </button>
                  {layoutMode === "neural-network" && (
                    <button className="secondary-button" onClick={runInfluencePropagation}>
                      <BrainCircuit size={15} />
                      Propagate influence
                    </button>
                  )}
                </div>
              </div>
            )}

            {activeInspectorTab === "properties" && selectedEdge && (
              <div className="inspector-content">
                <div className="relation-summary">
                  <span>{project.nodes.find((node) => node.id === selectedEdge.source)?.title}</span>
                  <ChevronRight size={15} />
                  <span>{project.nodes.find((node) => node.id === selectedEdge.target)?.title}</span>
                </div>
                <label className="field-label">
                  Relation type
                  <select
                    value={selectedEdge.type}
                    onChange={(event) =>
                      commit("Change relation type", (draft) => {
                        const edge = draft.edges.find((item) => item.id === selectedEdge.id);
                        if (edge) edge.type = event.target.value as ResearchEdgeType;
                      })
                    }
                  >
                    {EDGE_TYPES.map((type) => (
                      <option key={type} value={type}>
                        {edgeTypeLabels[type]}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field-label">
                  Confidence
                  <input
                    type="range"
                    min="0"
                    max="1"
                    step="0.01"
                    value={selectedEdge.confidence ?? 1}
                    onChange={(event) =>
                      setProject((current) => {
                        const next = cloneProject(current);
                        const edge = next.edges.find((item) => item.id === selectedEdge.id);
                        if (edge) edge.confidence = Number(event.target.value);
                        return next;
                      })
                    }
                  />
                  <span className="range-value">
                    {Math.round((selectedEdge.confidence ?? 1) * 100)}%
                  </span>
                </label>
                {selectedEdge.experiment && (
                  <div className={`experiment-card ${selectedEdge.experiment.outcome}`}>
                    <span className="eyebrow">Experiment edge</span>
                    <strong>{selectedEdge.experiment.label}</strong>
                    <div className="experiment-metrics">
                      <span>
                        value{" "}
                        {typeof selectedEdge.experiment.value === "number"
                          ? `${(selectedEdge.experiment.value * 100).toFixed(2)}%`
                          : "planned"}
                      </span>
                      <span>
                        Δ{" "}
                        {typeof selectedEdge.experiment.delta === "number"
                          ? `${(selectedEdge.experiment.delta * 100).toFixed(2)} pp`
                          : "—"}
                      </span>
                    </div>
                    <small>
                      {selectedEdge.experiment.outcome} · commit{" "}
                      {selectedEdge.experiment.commit ?? "unbound"}
                    </small>
                  </div>
                )}
                <button className="danger-button" onClick={deleteSelected}>
                  <Trash2 size={15} />
                  Delete relation
                </button>
              </div>
            )}

            {activeInspectorTab === "properties" && !selectedNode && !selectedEdge && (
              <div className="empty-state">
                <Network size={28} />
                <strong>Select a node or relation</strong>
                <p>Inspect semantic fields, provenance, evidence, and scenario impact.</p>
              </div>
            )}

            {activeInspectorTab === "evidence" && (
              <div className="inspector-content">
                <div className="tab-intro">
                  <div>
                    <span className="eyebrow">Traceable sources</span>
                    <strong>
                      {selectedNode
                        ? `${selectedNode.evidenceIds.length} bound records`
                        : "Select a node"}
                    </strong>
                  </div>
                  <button
                    className="small-primary-button"
                    onClick={() => setModal("evidence")}
                    disabled={!selectedNode}
                  >
                    <Plus size={13} /> Add
                  </button>
                </div>
                {selectedNode?.evidenceIds.map((evidenceId) => {
                  const evidence = project.evidence.find((item) => item.id === evidenceId);
                  if (!evidence) return null;
                  return (
                    <article className="evidence-card" key={evidence.id}>
                      <div className="evidence-card-head">
                        <span className={`evidence-status ${evidence.status}`}>
                          {evidence.status}
                        </span>
                        <span>
                          {evidence.locator.page ? `p. ${evidence.locator.page}` : "source"}
                        </span>
                      </div>
                      <h3>{evidence.title}</h3>
                      <p className="evidence-authors">
                        {[evidence.authors, evidence.year].filter(Boolean).join(" · ")}
                      </p>
                      {evidence.locator.quote && (
                        <blockquote>“{evidence.locator.quote}”</blockquote>
                      )}
                      <div className="evidence-locator">
                        <FileText size={13} />
                        {evidence.locator.section ?? evidence.sourceType}
                      </div>
                    </article>
                  );
                })}
                {selectedNode && selectedNode.evidenceIds.length === 0 && (
                  <div className="empty-inline">
                    No evidence bound yet. Add a paper, page, section, and exact quote.
                  </div>
                )}
              </div>
            )}

            {activeInspectorTab === "suggestions" && (
              <div className="inspector-content suggestions-panel">
                <div className="ai-boundary-note">
                  <Sparkles size={17} />
                  <div>
                    <strong>AI can only propose changes</strong>
                    <p>Nothing enters the formal graph until you review it.</p>
                  </div>
                </div>
                <div className="suggestion-toolbar">
                  <span>
                    {suggestions.filter((item) => item.status === "proposed").length} pending
                  </span>
                  <button onClick={acceptAllSuggestions}>Accept all as one transaction</button>
                </div>
                {suggestions.map((suggestion) => (
                  <article
                    key={suggestion.id}
                    className={`suggestion-card status-${suggestion.status}`}
                    data-testid={`suggestion-${suggestion.id}`}
                  >
                    <div className="suggestion-head">
                      <span className="suggestion-kind">
                        {suggestion.kind === "node" ? <Plus size={12} /> : <Link2 size={12} />}
                        {suggestion.kind}
                      </span>
                      <span className="confidence">
                        {Math.round(suggestion.confidence * 100)}%
                      </span>
                    </div>
                    <h3>{suggestion.title}</h3>
                    <p>{suggestion.description}</p>
                    <div className="suggestion-source">
                      <FileText size={13} />
                      {suggestion.evidenceLabel}
                    </div>
                    {suggestion.status === "proposed" ? (
                      <div className="suggestion-actions">
                        <button
                          className="accept-button"
                          onClick={() => acceptSuggestion(suggestion)}
                          data-testid={`accept-${suggestion.id}`}
                        >
                          <Check size={14} />
                          Accept
                        </button>
                        <button className="edit-button">Edit</button>
                        <button
                          className="reject-button"
                          onClick={() => rejectSuggestion(suggestion.id)}
                        >
                          <X size={14} />
                          Reject
                        </button>
                      </div>
                    ) : (
                      <div className={`review-outcome ${suggestion.status}`}>
                        {suggestion.status === "accepted" ? <Check size={14} /> : <X size={14} />}
                        {suggestion.status}
                      </div>
                    )}
                  </article>
                ))}
              </div>
            )}
          </div>
        </aside>

        {rightCollapsed && (
          <button
            className="panel-reopen right"
            onClick={() => setRightCollapsed(false)}
            aria-label="Open inspector"
          >
            <ChevronDown size={16} />
          </button>
        )}

        <section className={`bottom-panel ${bottomExpanded ? "expanded" : ""}`}>
          <div className="bottom-panel-head">
            <div className="bottom-tabs">
              <button
                className={bottomTab === "traversal" ? "active" : ""}
                onClick={() => {
                  setBottomTab("traversal");
                  setBottomExpanded(true);
                }}
              >
                <GitBranch size={14} />
                Traversal
              </button>
              <button
                className={bottomTab === "scenario" ? "active" : ""}
                onClick={() => {
                  setBottomTab("scenario");
                  setBottomExpanded(true);
                }}
              >
                <CircleDotDashed size={14} />
                Scenario diff
                {activeScenarioId && <span className="tab-alert" />}
              </button>
              <button
                className={bottomTab === "influence" ? "active" : ""}
                onClick={() => {
                  setBottomTab("influence");
                  setBottomExpanded(true);
                }}
              >
                <BrainCircuit size={14} />
                Logic & influence
                {logicChain && <span className="tab-alert success" />}
              </button>
              <button
                className={bottomTab === "performance" ? "active" : ""}
                onClick={() => {
                  setBottomTab("performance");
                  setBottomExpanded(true);
                }}
              >
                <Gauge size={14} />
                Performance
              </button>
              <button
                className={bottomTab === "activity" ? "active" : ""}
                onClick={() => {
                  setBottomTab("activity");
                  setBottomExpanded(true);
                }}
              >
                <Activity size={14} />
                Activity
              </button>
            </div>
            <button
              className="bottom-toggle"
              onClick={() => setBottomExpanded((value) => !value)}
              aria-label="Toggle analysis panel"
            >
              <ChevronDown size={16} />
            </button>
          </div>

          {bottomExpanded && bottomTab === "traversal" && (
            <div className="traversal-panel">
              <div className="traversal-controls">
                <div className="segmented-control">
                  <button
                    className={traversalStrategy === "bfs" ? "active" : ""}
                    onClick={() => setTraversalStrategy("bfs")}
                    data-testid="strategy-bfs"
                  >
                    BFS
                  </button>
                  <button
                    className={traversalStrategy === "dfs" ? "active" : ""}
                    onClick={() => setTraversalStrategy("dfs")}
                    data-testid="strategy-dfs"
                  >
                    DFS
                  </button>
                </div>
                <label>
                  Direction
                  <select
                    value={traversalDirection}
                    onChange={(event) =>
                      setTraversalDirection(event.target.value as "in" | "out" | "both")
                    }
                  >
                    <option value="out">Downstream</option>
                    <option value="in">Upstream</option>
                    <option value="both">Both</option>
                  </select>
                </label>
                <label>
                  Depth
                  <input
                    type="number"
                    min="1"
                    max="12"
                    value={maxDepth}
                    onChange={(event) => setMaxDepth(Number(event.target.value))}
                  />
                </label>
                <div className="edge-filter-pills">
                  {(["depends_on", "controls", "supports", "measures"] as ResearchEdgeType[]).map(
                    (type) => (
                      <button
                        key={type}
                        className={edgeTypeFilter.includes(type) ? "active" : ""}
                        onClick={() => toggleEdgeFilter(type)}
                      >
                        {edgeTypeLabels[type]}
                      </button>
                    ),
                  )}
                </div>
                <button
                  className="run-analysis-button"
                  onClick={runTraversal}
                  data-testid="run-traversal"
                >
                  <GitBranch size={15} />
                  Run {traversalStrategy.toUpperCase()}
                </button>
                <button className="secondary-button compact" onClick={runCycleDetection}>
                  <RotateCcw size={14} />
                  Detect cycles
                </button>
              </div>

              <div className="traversal-results">
                <div className="result-summary">
                  <span className="eyebrow">Result</span>
                  {traversal ? (
                    <>
                      <strong>
                        {traversal.order.length} nodes in {traversal.durationMs.toFixed(2)} ms
                      </strong>
                      <p>
                        Start: {project.nodes.find((node) => node.id === traversal.startId)?.title} ·{" "}
                        {traversal.treeEdgeIds.length} tree edges ·{" "}
                        {traversal.crossEdgeIds.length} cross edges
                      </p>
                    </>
                  ) : (
                    <>
                      <strong>Ready to traverse</strong>
                      <p>Select a root node, direction, relation filters, and a maximum depth.</p>
                    </>
                  )}
                </div>
                <div className="depth-layers">
                  {traversal &&
                    [...new Set(Object.values(traversal.depth))]
                      .sort((a, b) => a - b)
                      .map((depth) => (
                        <div className="depth-layer" key={depth}>
                          <span>Depth {depth}</span>
                          <div>
                            {traversal.order
                              .filter((nodeId) => traversal.depth[nodeId] === depth)
                              .map((nodeId) => (
                                <button key={nodeId} onClick={() => focusNode(nodeId)}>
                                  {project.nodes.find((node) => node.id === nodeId)?.title}
                                </button>
                              ))}
                          </div>
                        </div>
                      ))}
                </div>
                <div className="cycle-result">
                  <span className="eyebrow">Cycle check</span>
                  <strong>
                    {cycles.length
                      ? `${cycles.length} cycle needs review`
                      : "No unresolved cycle in the latest check"}
                  </strong>
                  {cycles[0] && (
                    <p>
                      {cycles[0].nodeIds
                        .map((id) => project.nodes.find((node) => node.id === id)?.title ?? id)
                        .join(" → ")}
                    </p>
                  )}
                </div>
              </div>
            </div>
          )}

          {bottomExpanded && bottomTab === "scenario" && (
            <div className="scenario-panel">
              <div className="scenario-overview">
                <span className="eyebrow">Overlay comparison</span>
                <strong>{activeScenario?.name ?? "Choose an ablation scenario"}</strong>
                <p>
                  Scenarios store only disabled IDs and property overrides. The base semantic graph
                  remains untouched.
                </p>
              </div>
              <div className="diff-grid">
                <div className="diff-card disabled">
                  <span>Disabled</span>
                  <strong>{scenarioDiff?.disabledNodeIds.length ?? 0}</strong>
                  <p>
                    {scenarioDiff?.disabledNodeIds
                      .map((id) => project.nodes.find((node) => node.id === id)?.title)
                      .join(", ") || "No nodes disabled"}
                  </p>
                </div>
                <div className="diff-card lost">
                  <span>Lost reachability</span>
                  <strong>{scenarioDiff?.lostReachableNodeIds.length ?? 0}</strong>
                  <p>
                    {scenarioDiff?.lostReachableNodeIds
                      .slice(0, 3)
                      .map((id) => project.nodes.find((node) => node.id === id)?.title)
                      .join(", ") || "No downstream losses"}
                  </p>
                </div>
                <div className="diff-card retained">
                  <span>Still reachable</span>
                  <strong>{scenarioDiff?.retainedReachableNodeIds.length ?? 0}</strong>
                  <p>Nodes with valid paths after applying the overlay</p>
                </div>
                <div className="diff-card alternate">
                  <span>Alternate paths</span>
                  <strong>{scenarioDiff?.alternatePathNodeIds.length ?? 0}</strong>
                  <p>Reachable through a different parent relation</p>
                </div>
              </div>
              {activeScenario && (
                <div className="scenario-notes">
                  <div>
                    <span>Hypothesis</span>
                    <p>{activeScenario.hypothesis}</p>
                  </div>
                  <div>
                    <span>Expected effect</span>
                    <p>{activeScenario.expectedEffect}</p>
                  </div>
                </div>
              )}
            </div>
          )}

          {bottomExpanded && bottomTab === "influence" && (
            <div className="influence-panel" data-testid="influence-panel">
              <div className="logic-summary-card">
                <span className="eyebrow">Highlighted logic</span>
                <strong>{logicChain?.summary ?? "Choose a chain mode and highlight it."}</strong>
                <p>
                  {logicChain
                    ? `${logicChain.edgeIds.length} experiment/evidence relations · confidence ${Math.round(
                        logicChain.score * 100,
                      )}%`
                    : "Effective chains require completed experiment edges with a material metric change."}
                </p>
                <div className="logic-legend">
                  <span className="effective">effective experiment</span>
                  <span className="evidence">supporting evidence</span>
                  <span className="refutation">refutation</span>
                </div>
              </div>
              <div className="influence-controls">
                <div>
                  <span className="eyebrow">BP-like message passing</span>
                  <strong>
                    Target:{" "}
                    {project.nodes.find((node) => node.id === influence?.targetId)?.title ??
                      "select an output metric"}
                  </strong>
                  <p>
                    Experimental deltas and edge confidence are propagated backward as signed,
                    normalized influence—not as a causal estimate.
                  </p>
                </div>
                <button className="primary-button" onClick={runInfluencePropagation}>
                  <BrainCircuit size={15} />
                  Propagate from selection
                </button>
              </div>
              <div className="influence-ranking">
                {influence ? (
                  Object.entries(influence.scores)
                    .filter(([id, score]) => {
                      const node = project.nodes.find((candidate) => candidate.id === id);
                      return (
                        id !== influence.targetId &&
                        Math.abs(score) >= 0.001 &&
                        node?.type === "variable" &&
                        node.data.role !== "control" &&
                        node.data.role !== "input"
                      );
                    })
                    .sort((a, b) => Math.abs(b[1]) - Math.abs(a[1]))
                    .slice(0, 7)
                    .map(([id, score]) => (
                      <button key={id} onClick={() => focusNode(id)}>
                        <span>{project.nodes.find((node) => node.id === id)?.title ?? id}</span>
                        <span className="influence-bar">
                          <i
                            className={score < 0 ? "negative" : "positive"}
                            style={{ width: `${Math.max(4, Math.abs(score) * 100)}%` }}
                          />
                        </span>
                        <strong>
                          {score >= 0 ? "+" : ""}
                          {(score * 100).toFixed(0)}%
                        </strong>
                      </button>
                    ))
                ) : (
                  <div className="empty-inline">No propagation result yet.</div>
                )}
              </div>
            </div>
          )}

          {bottomExpanded && bottomTab === "performance" && (
            <div className="performance-panel" data-testid="performance-panel">
              <div className="perf-card">
                <span>Frame rate</span>
                <strong>{performanceStats.fps} FPS</strong>
                <small>{performanceStats.frameMs} ms/frame</small>
              </div>
              <div className="perf-card">
                <span>Graph projection</span>
                <strong>{flowNodes.length} / {project.nodes.length}</strong>
                <small>{flowEdges.length} visible relations</small>
              </div>
              <div className="perf-card">
                <span>JS heap</span>
                <strong>{performanceStats.heapMb ? `${performanceStats.heapMb} MB` : "N/A"}</strong>
                <small>Browser support dependent</small>
              </div>
              <div className="perf-card">
                <span>Last traversal</span>
                <strong>{traversal ? `${traversal.durationMs.toFixed(2)} ms` : "Not run"}</strong>
                <small>{traversal?.order.length ?? 0} visited nodes</small>
              </div>
              <div className="perf-diagnostics">
                <Gauge size={18} />
                <div>
                  <strong>Built-in diagnostics</strong>
                  <p>
                    Renderer counts, traversal latency, heap availability, frame sampling, current
                    filter projection, and active theme are observable without developer tools.
                  </p>
                </div>
              </div>
            </div>
          )}

          {bottomExpanded && bottomTab === "activity" && (
            <div className="activity-panel">
              {project.activity.slice(0, 6).map((activity) => (
                <div className="activity-row" key={activity.id}>
                  <span className={`activity-origin ${activity.origin}`}>
                    {activity.origin === "ai" ? <Bot size={14} /> : <History size={14} />}
                  </span>
                  <div>
                    <strong>{activity.label}</strong>
                    <small>
                      {activity.origin} · {new Date(activity.createdAt).toLocaleTimeString()}
                    </small>
                  </div>
                </div>
              ))}
            </div>
          )}
        </section>
      </div>

      {rightCollapsed && (
        <button
          className="floating-inspector-button"
          onClick={() => setRightCollapsed(false)}
          aria-label="Open inspector"
        >
          <Settings2 size={16} />
        </button>
      )}

      <input
        ref={importInput}
        className="hidden-input"
        type="file"
        accept="application/json,.json"
        onChange={handleProjectImport}
      />
      <input
        ref={runResultInput}
        className="hidden-input"
        type="file"
        accept="application/json,.json"
        onChange={handleRunResultImport}
      />

      {modal && (
        <div className="modal-backdrop" onMouseDown={() => setModal(null)}>
          <div className="modal-card" onMouseDown={stopEvent} role="dialog" aria-modal="true">
            <div className="modal-head">
              <div>
                <span className="eyebrow">
                  {modal === "new-node"
                    ? "Semantic graph"
                    : modal === "new-edge"
                      ? "Typed relation"
                      : modal === "split-node"
                        ? "Semantic decomposition"
                        : "Traceable source"}
                </span>
                <h2>
                  {modal === "new-node"
                    ? "Create research node"
                    : modal === "new-edge"
                      ? "Connect two nodes"
                      : modal === "split-node"
                        ? "Split selected node"
                        : "Attach evidence"}
                </h2>
              </div>
              <button className="icon-button ghost" onClick={() => setModal(null)}>
                <X size={17} />
              </button>
            </div>

            {modal === "new-node" && (
              <div className="modal-body">
                <label className="field-label">
                  Node title
                  <input
                    autoFocus
                    value={newNode.title}
                    onChange={(event) =>
                      setNewNode((current) => ({ ...current, title: event.target.value }))
                    }
                    placeholder="e.g. Random seed"
                  />
                </label>
                <label className="field-label">
                  Type
                  <select
                    value={newNode.type}
                    onChange={(event) =>
                      setNewNode((current) => ({
                        ...current,
                        type: event.target.value as ResearchNodeType,
                      }))
                    }
                  >
                    {NODE_TYPES.map((type) => (
                      <option key={type} value={type}>
                        {nodeTypeLabels[type]}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field-label">
                  Definition or note
                  <textarea
                    rows={4}
                    value={newNode.body}
                    onChange={(event) =>
                      setNewNode((current) => ({ ...current, body: event.target.value }))
                    }
                    placeholder="State what this object means in the research model."
                  />
                </label>
                <div className="modal-footer">
                  <button className="secondary-button" onClick={() => setModal(null)}>
                    Cancel
                  </button>
                  <button
                    className="primary-button"
                    onClick={createNode}
                    disabled={!newNode.title.trim()}
                    data-testid="create-node-confirm"
                  >
                    <Plus size={15} />
                    Create node
                  </button>
                </div>
              </div>
            )}

            {modal === "new-edge" && (
              <div className="modal-body">
                <label className="field-label">
                  From
                  <select
                    value={newEdge.source}
                    onChange={(event) =>
                      setNewEdge((current) => ({ ...current, source: event.target.value }))
                    }
                  >
                    {project.nodes.map((node) => (
                      <option key={node.id} value={node.id}>
                        {node.title}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field-label">
                  Relation
                  <select
                    value={newEdge.type}
                    onChange={(event) =>
                      setNewEdge((current) => ({
                        ...current,
                        type: event.target.value as ResearchEdgeType,
                      }))
                    }
                  >
                    {EDGE_TYPES.map((type) => (
                      <option key={type} value={type}>
                        {edgeTypeLabels[type]}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="field-label">
                  To
                  <select
                    value={newEdge.target}
                    onChange={(event) =>
                      setNewEdge((current) => ({ ...current, target: event.target.value }))
                    }
                  >
                    {project.nodes.map((node) => (
                      <option key={node.id} value={node.id}>
                        {node.title}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="modal-footer">
                  <button className="secondary-button" onClick={() => setModal(null)}>
                    Cancel
                  </button>
                  <button
                    className="primary-button"
                    onClick={createEdge}
                    disabled={newEdge.source === newEdge.target}
                  >
                    <Link2 size={15} />
                    Create relation
                  </button>
                </div>
              </div>
            )}

            {modal === "evidence" && (
              <div className="modal-body">
                <label className="field-label">
                  Paper or source title
                  <input
                    autoFocus
                    value={newEvidence.title}
                    onChange={(event) =>
                      setNewEvidence((current) => ({ ...current, title: event.target.value }))
                    }
                    placeholder="Source title"
                  />
                </label>
                <div className="field-grid">
                  <label className="field-label">
                    Page
                    <input
                      type="number"
                      value={newEvidence.page}
                      onChange={(event) =>
                        setNewEvidence((current) => ({ ...current, page: event.target.value }))
                      }
                    />
                  </label>
                  <label className="field-label">
                    Section
                    <input
                      value={newEvidence.section}
                      onChange={(event) =>
                        setNewEvidence((current) => ({
                          ...current,
                          section: event.target.value,
                        }))
                      }
                    />
                  </label>
                </div>
                <label className="field-label">
                  Exact quote
                  <textarea
                    rows={4}
                    value={newEvidence.quote}
                    onChange={(event) =>
                      setNewEvidence((current) => ({ ...current, quote: event.target.value }))
                    }
                    placeholder="Paste only the precise supporting text."
                  />
                </label>
                <label className="field-label">
                  URL or DOI
                  <input
                    value={newEvidence.url}
                    onChange={(event) =>
                      setNewEvidence((current) => ({ ...current, url: event.target.value }))
                    }
                  />
                </label>
                <div className="modal-footer">
                  <button className="secondary-button" onClick={() => setModal(null)}>
                    Cancel
                  </button>
                  <button
                    className="primary-button"
                    onClick={createEvidence}
                    disabled={!newEvidence.title.trim()}
                  >
                    <FileText size={15} />
                    Attach evidence
                  </button>
                </div>
              </div>
            )}

            {modal === "split-node" && selectedNode && (
              <div className="modal-body">
                <div className="split-source-card">
                  <span className="eyebrow">Original node becomes deprecated</span>
                  <strong>{selectedNode.title}</strong>
                  <p>
                    Evidence is copied to both candidates and two explicit derived-from relations
                    preserve provenance. Review the new nodes independently.
                  </p>
                </div>
                <label className="field-label">
                  First research object
                  <input
                    autoFocus
                    value={splitParts.first}
                    onChange={(event) =>
                      setSplitParts((current) => ({ ...current, first: event.target.value }))
                    }
                  />
                </label>
                <label className="field-label">
                  Second research object
                  <input
                    value={splitParts.second}
                    onChange={(event) =>
                      setSplitParts((current) => ({ ...current, second: event.target.value }))
                    }
                  />
                </label>
                <div className="modal-footer">
                  <button className="secondary-button" onClick={() => setModal(null)}>
                    Cancel
                  </button>
                  <button
                    className="primary-button"
                    onClick={splitSelectedNode}
                    disabled={!splitParts.first.trim() || !splitParts.second.trim()}
                    data-testid="split-node-confirm"
                  >
                    <Scissors size={15} />
                    Split node
                  </button>
                </div>
              </div>
            )}
          </div>
        </div>
      )}

      {pluginStoreOpen && (
        <div className="modal-backdrop" onMouseDown={() => setPluginStoreOpen(false)}>
          <div
            className="modal-card wide plugin-store"
            onMouseDown={stopEvent}
            role="dialog"
            aria-modal="true"
            data-testid="plugin-store"
          >
            <div className="modal-head">
              <div>
                <span className="eyebrow">Extension boundary</span>
                <h2>Research Canvas Plugin Store</h2>
                <p>VS Code-style manifests with explicit capabilities and permissions.</p>
              </div>
              <button className="icon-button ghost" onClick={() => setPluginStoreOpen(false)}>
                <X size={17} />
              </button>
            </div>
            <div className="plugin-store-feature">
              <div className="plugin-icon git">
                <GitCommit size={24} />
              </div>
              <div>
                <span className="eyebrow">Installed · verified demo</span>
                <h3>Git Experiments</h3>
                <p>
                  Load commit <code>{mnistRunSummary.environment.gitCommit}</code> with four real
                  CPU MNIST runs, metric evidence, scenarios, and experiment edges.
                </p>
              </div>
              <button className="primary-button" onClick={loadMnistStudy} data-testid="load-mnist">
                Load MNIST study
              </button>
            </div>
            <div className="plugin-grid">
              {pluginCatalog.map((plugin) => (
                <article className="plugin-card" key={plugin.id}>
                  <div className="plugin-card-head">
                    <span className={`plugin-status ${plugin.status}`}>{plugin.status}</span>
                    <small>{plugin.category}</small>
                  </div>
                  <h3>{plugin.name}</h3>
                  <p>{plugin.description}</p>
                  <div className="capability-list">
                    {plugin.capabilities.slice(0, 3).map((capability) => (
                      <span key={capability}>{capability}</span>
                    ))}
                  </div>
                  <small>
                    {plugin.publisher} · v{plugin.version}
                  </small>
                  {plugin.status === "available" && (
                    <button
                      className="secondary-button"
                      onClick={() => {
                        setLoadedPlugins((items) =>
                          items.includes(plugin.id) ? items : [...items, plugin.id],
                        );
                        setToast(`${plugin.name} enabled for this workspace.`);
                      }}
                    >
                      {loadedPlugins.includes(plugin.id) ? "Enabled" : "Enable"}
                    </button>
                  )}
                </article>
              ))}
            </div>
          </div>
        </div>
      )}

      {settingsOpen && (
        <div className="modal-backdrop" onMouseDown={() => setSettingsOpen(false)}>
          <div
            className="modal-card settings-modal"
            onMouseDown={stopEvent}
            role="dialog"
            aria-modal="true"
            data-testid="settings-modal"
          >
            <div className="modal-head">
              <div>
                <span className="eyebrow">Workspace preferences</span>
                <h2>Settings</h2>
              </div>
              <button className="icon-button ghost" onClick={() => setSettingsOpen(false)}>
                <X size={17} />
              </button>
            </div>
            <div className="settings-section">
              <div className="settings-title">
                <Palette size={17} />
                <div>
                  <strong>Color theme</strong>
                  <small>Theme manifests use stable semantic color tokens.</small>
                </div>
              </div>
              <div className="theme-grid">
                {themeCatalog.map((theme) => (
                  <button
                    key={theme.id}
                    className={themeId === theme.id ? "active" : ""}
                    onClick={() => setThemeId(theme.id)}
                  >
                    <span className="theme-swatch" style={{ background: theme.colors.canvas }}>
                      <i style={{ background: theme.colors.accent }} />
                    </span>
                    <span>
                      <strong>{theme.name}</strong>
                      <small>{theme.publisher}</small>
                    </span>
                  </button>
                ))}
              </div>
            </div>
            <div className="settings-section">
              <div className="settings-title">
                <SlidersHorizontal size={17} />
                <div>
                  <strong>Canvas interaction</strong>
                  <small>Windows and macOS trackpad-friendly defaults.</small>
                </div>
              </div>
              <label className="settings-toggle">
                <span>
                  <strong>Two-finger pan</strong>
                  <small>Wheel and trackpad gestures pan; pinch zoom remains enabled.</small>
                </span>
                <input
                  type="checkbox"
                  checked={trackpadPan}
                  onChange={(event) => setTrackpadPan(event.target.checked)}
                />
              </label>
              <label className="settings-toggle">
                <span>
                  <strong>Snap to 16 px grid</strong>
                  <small>Applied only while committing node placements.</small>
                </span>
                <input
                  type="checkbox"
                  checked={snapEnabled}
                  onChange={(event) => setSnapEnabled(event.target.checked)}
                />
              </label>
            </div>
            <div className="settings-section">
              <div className="settings-title">
                <Settings2 size={17} />
                <div>
                  <strong>Reserved integrations</strong>
                  <small>MCP, Agent, Zotero, and Python require explicit future grants.</small>
                </div>
              </div>
              <div className="reserved-row">
                {pluginCatalog
                  .filter((plugin) => plugin.status === "reserved")
                  .map((plugin) => (
                    <span key={plugin.id}>{plugin.name}</span>
                  ))}
              </div>
            </div>
            <div className="modal-footer">
              <button
                className="secondary-button"
                onClick={() => {
                  setSettingsOpen(false);
                  setShortcutsOpen(true);
                }}
              >
                <Binary size={15} />
                Keyboard shortcuts
              </button>
              <button className="primary-button" onClick={() => setSettingsOpen(false)}>
                Done
              </button>
            </div>
          </div>
        </div>
      )}

      {shortcutsOpen && (
        <div className="modal-backdrop shortcut-layer" onMouseDown={() => setShortcutsOpen(false)}>
          <div className="modal-card shortcut-modal" onMouseDown={stopEvent} role="dialog">
            <div className="modal-head">
              <div>
                <span className="eyebrow">Windows · macOS</span>
                <h2>Keyboard shortcuts</h2>
              </div>
              <button className="icon-button ghost" onClick={() => setShortcutsOpen(false)}>
                <X size={17} />
              </button>
            </div>
            <div className="shortcut-grid">
              {[
                ["Ctrl/Cmd K", "Focus search"],
                ["Ctrl/Cmd Enter", "Run BFS/DFS"],
                ["Ctrl/Cmd Shift P", "Plugin store"],
                ["N", "New node"],
                ["E", "New relation"],
                ["F", "Fit graph"],
                ["Z", "Zen mode"],
                ["Delete", "Delete selection"],
                ["?", "Shortcut reference"],
                ["Esc", "Close overlays"],
              ].map(([keys, action]) => (
                <div key={keys}>
                  <kbd>{keys}</kbd>
                  <span>{action}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {zenMode && (
        <button className="zen-exit" onClick={toggleZenMode}>
          <Focus size={15} />
          Exit Zen · Z
        </button>
      )}

      {toast && (
        <div className="toast" role="status">
          <Check size={15} />
          {toast}
        </div>
      )}
    </main>
  );
}

export function ResearchCanvasApp() {
  return (
    <ReactFlowProvider>
      <AppShell />
    </ReactFlowProvider>
  );
}
