"use client";

/**
 * 当前研究工作区的客户端组合根：连接 React Flow、本地 UI 状态、图分析、插件展示和弹窗。
 * Current client composition root: bridges React Flow, local UI state, graph analysis,
 * plugin presentation, and dialogs.
 *
 * TODO(architecture): 将功能面板和状态/操作拆到专用模块；此文件是审计中最大的耦合边界。
 * TODO(architecture): split feature panels and state/actions into dedicated modules;
 * this file is the largest coupling boundary identified by the audit.
 */

import {
  Background,
  BackgroundVariant,
  BaseEdge,
  Controls,
  EdgeLabelRenderer,
  MarkerType,
  MiniMap,
  Position,
  ReactFlow,
  ReactFlowProvider,
  getBezierPath,
  getSmoothStepPath,
  getStraightPath,
  type Connection,
  type Edge,
  type EdgeProps,
  type Node,
  type NodeChange,
  useReactFlow,
  useUpdateNodeInternals,
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
  ClipboardCopy,
  ClipboardPaste,
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
  Languages,
  Link2,
  ListTree,
  Maximize2,
  Network,
  Palette,
  PackageOpen,
  PanelLeftClose,
  PanelRightClose,
  Plus,
  Pin,
  PinOff,
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
  allShortestPaths,
  compareScenarioReachability,
  computeLayout,
  computeLogicChain,
  detectCycles,
  evidenceBacklinks,
  exportCsv,
  exportJsonCanvas,
  exportMarkdown,
  makeId,
  migrateProject,
  propagateInfluence,
  resolveEdges,
  traverseGraph,
} from "../lib/research-core";
import { initialProject, initialSuggestions } from "../lib/fixtures";
import {
  createMnistProject,
  mnistRunSummary,
  mnistSuggestions,
} from "../lib/mnist-fixture";
import { createSocialScienceProject } from "../lib/social-fixture";
import { normalizeLocale, translate, type Locale, type MessageKey } from "../i18n/catalog";
import {
  builtInEdgeStyleCatalog,
  builtInPluginCatalog,
  builtInThemeCatalog,
} from "../plugins/catalog";
import {
  isMycFileName,
  normalizeInstalledEdgeStyle,
  normalizeInstalledTheme,
  type InstalledMycPlugin,
} from "../plugins/contracts";
import {
  installMycPlugin,
  listInstalledMycPlugins,
  listenForMycDrops,
} from "../plugins/tauri-client";
import { canvasEdgeTypes, canvasNodeTypes } from "./canvas-renderers";
import {
  EDGE_TYPES,
  LAYOUT_MODES,
  NODE_TYPES,
  type BlockStyleId,
  type EdgeStyleManifest,
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

/** React Flow 适配数据，不属于持久化研究模型 / React Flow adapter data, not persisted domain state. */
type CanvasNodeData = {
  record: ResearchNode;
  blockStyleId: BlockStyleId;
  disabled: boolean;
  depth?: number;
  traversed: boolean;
  chainState?: "effective" | "evidence" | "refutation";
  annotation?: string;
  influence?: number;
  collapsed?: boolean;
  pinned?: boolean;
  onResizeStart?: () => void;
  onResizeEnd?: () => void;
};

type CanvasNode = Node<CanvasNodeData, "researchNode">;
/** 连线的纯展示状态，由领域关系和当前 UI 筛选共同派生 / Presentation-only edge state derived from domain data and UI filters. */
type CanvasEdgeData = {
  type: ResearchEdgeType;
  edgeStyle: EdgeStyleManifest;
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

/** 可撤销操作的快照；项目本身保持 JSON 可序列化 / Undoable snapshot; project stays JSON-serializable. */
type HistoryEntry = {
  label: string;
  snapshot: ProjectState;
};

type SettingsSection = "display" | "canvas" | "integrations" | "shortcuts";
type DisplayDensity = "auto" | "compact" | "comfortable" | "spacious";
type DisplayProfile = {
  scaleFactor: number;
  dpi: number;
  source: "tauri" | "browser";
};

type ProjectLibraryEntry = {
  id: string;
  title: string;
  updatedAt: string;
  nodeCount: number;
  snapshot: ProjectState;
};

const displayDensityOptions: Array<{
  id: DisplayDensity;
  label: string;
  description: string;
}> = [
  { id: "auto", label: "Auto", description: "Match this display" },
  { id: "compact", label: "Compact", description: "More canvas space" },
  { id: "comfortable", label: "Comfortable", description: "Balanced reading" },
  { id: "spacious", label: "Spacious", description: "Largest controls" },
];

const blockStyleOptions: Array<{
  id: BlockStyleId;
  label: string;
  labelZh: string;
  description: string;
  descriptionZh: string;
  icon: typeof LayoutDashboard;
}> = [
  {
    id: "research-card",
    label: "Research card",
    labelZh: "研究卡片",
    description: "Balanced evidence detail",
    descriptionZh: "平衡证据与内容细节",
    icon: LayoutDashboard,
  },
  {
    id: "compact-block",
    label: "Compact block",
    labelZh: "紧凑块",
    description: "Dense overview and tables",
    descriptionZh: "适合总览与表格布局",
    icon: ListTree,
  },
  {
    id: "signal-block",
    label: "Signal block",
    labelZh: "信号块",
    description: "Clear variable flow and ports",
    descriptionZh: "突出变量流与连接端口",
    icon: Network,
  },
];

/** 根据显示器缩放给出保守的界面缩放建议 / Gives a conservative UI scale recommendation for display scaling. */
function recommendedUiScale(scaleFactor: number) {
  return Math.min(1.44, Math.max(1.32, 1.32 + (scaleFactor - 1) * 0.08));
}

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

/** Temporary compatibility state for the legacy edge renderer during extraction. */
type EdgePresentationState = {
  disabled?: boolean;
  traversed?: boolean;
  treeEdge?: boolean;
  backEdge?: boolean;
  chainState?: "effective" | "evidence" | "refutation";
  selected?: boolean;
};

function resolveEdgePresentation(
  edgeStyle: EdgeStyleManifest,
  edgeType: ResearchEdgeType,
  state: EdgePresentationState,
) {
  const relation = edgeStyle.relations?.[edgeType];
  let color = relation?.color ?? edgeStyle.stroke.color;
  let width = relation?.width ?? edgeStyle.stroke.width;
  let opacity = relation?.opacity ?? edgeStyle.stroke.opacity;
  let dash = relation?.dash ?? edgeStyle.stroke.dash;

  if (state.treeEdge) color = "#5b67c9";
  if (state.backEdge) {
    color = "#cf435b";
    dash = [6, 4];
  }
  if (state.traversed) {
    color = "var(--accent)";
    width = Math.max(width, 2.1);
  }
  if (state.chainState === "effective") {
    color = "#149b76";
    width = Math.max(width, 3.1);
    dash = undefined;
  }
  if (state.chainState === "evidence") {
    color = "#4e6fd2";
    width = Math.max(width, 3.1);
    dash = undefined;
  }
  if (state.chainState === "refutation") {
    color = "#d5485c";
    width = Math.max(width, 3.1);
    dash = [8, 5];
  }
  if (state.disabled) {
    color = "#9aa4b2";
    opacity = 0.38;
    dash = [5, 5];
  }
  if (state.selected) {
    width = Math.max(width, relation?.selectedWidth ?? edgeStyle.stroke.selectedWidth);
  }

  return { color, width, opacity, dash };
}

/**
 * 研究节点的 React Flow 渲染器；仅呈现已派生的状态，不直接修改项目。
 * React Flow renderer for one research node; renders derived state without mutating the project.
 */
/**
 * 将声明式边样式与关系专属覆盖合并为最终渲染属性。
 * Merges declarative edge style with relation-specific overrides into render attributes.
 */

/**
 * 研究关系的 React Flow 渲染器，支持多种路由和语义化视觉状态。
 * React Flow relation renderer supporting multiple routes and semantic visual states.
 */
// TODO(cleanup): remove this compatibility renderer after the visual regression suite
// covers all connector routes. React Flow now uses `canvasEdgeTypes` below.
// eslint-disable-next-line @typescript-eslint/no-unused-vars
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
  const route = data?.edgeStyle.routing ?? "bezier";
  const pathOptions = {
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  };
  const [path, labelX, labelY] =
    route === "orthogonal"
      ? getSmoothStepPath({
          ...pathOptions,
          borderRadius: 0,
          offset: data?.edgeStyle.stroke.offset ?? 24,
        })
      : route === "smooth-step"
        ? getSmoothStepPath({
            ...pathOptions,
            borderRadius: data?.edgeStyle.stroke.cornerRadius ?? 8,
            offset: data?.edgeStyle.stroke.offset ?? 22,
          })
        : route === "straight"
          ? getStraightPath(pathOptions)
          : getBezierPath({ ...pathOptions, curvature: 0.28 });
  const presentation = resolveEdgePresentation(
    data?.edgeStyle ?? builtInEdgeStyleCatalog[0],
    data?.type ?? "depends_on",
    {
      disabled: data?.disabled,
      traversed: data?.traversed,
      treeEdge: data?.treeEdge,
      backEdge: data?.backEdge,
      chainState: data?.chainState,
      selected,
    },
  );
  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        interactionWidth={Math.max(18, presentation.width * 8)}
        style={{
          stroke: presentation.color,
          strokeWidth: presentation.width,
          strokeOpacity: presentation.opacity,
          strokeDasharray: presentation.dash?.join(" "),
        }}
        data-edge-routing={route}
        data-edge-style={data?.edgeStyle.id ?? builtInEdgeStyleCatalog[0].id}
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
          style={
            {
              transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
              "--edge-color": presentation.color,
            } as CSSProperties
          }
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

/** React Flow receives extracted renderers; AppShell only derives their data. */
const nodeTypes = canvasNodeTypes;
const edgeTypes = canvasEdgeTypes;

/** 在浏览器下载纯文本导出，不依赖服务器端存储 / Downloads a text export in-browser without server storage. */
function downloadText(filename: string, content: string, mime = "application/json") {
  const blob = new Blob([content], { type: mime });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}

/** 阻断模态框内部事件，避免触发遮罩关闭 / Stops modal-internal events from closing their backdrop. */
function stopEvent(event: React.SyntheticEvent) {
  event.stopPropagation();
}

/** 创建独立的新项目，避免复用演示 fixture 的可变引用 / Creates an independent blank project without reusing mutable fixture references. */
function createBlankProject(title = "Untitled research project"): ProjectState {
  const now = new Date().toISOString();
  const questionId = makeId("question");
  return {
    schemaVersion: 1,
    id: makeId("project"),
    title,
    discipline: "General research",
    updatedAt: now,
    revision: 1,
    nodes: [
      {
        id: questionId,
        type: "question",
        title: "Research question",
        body: "State the question this graph will investigate.",
        tags: ["starting-point"],
        status: "draft",
        evidenceIds: [],
        data: {},
        provenance: { origin: "human", actorId: "local-researcher" },
        createdAt: now,
        updatedAt: now,
      },
    ],
    edges: [],
    evidence: [],
    placements: [
      {
        id: `placement-${questionId}`,
        viewId: "view-main",
        nodeId: questionId,
        x: 120,
        y: 160,
        width: 250,
        height: 126,
      },
    ],
    scenarios: [],
    navigation: { recentNodeIds: [questionId], pinnedNodeIds: [] },
    activity: [
      {
        id: makeId("activity"),
        label: "Created project",
        origin: "human",
        createdAt: now,
      },
    ],
  };
}

/**
 * 在 UI 层拒绝无效自连和重复边；领域算法仍应能处理外部导入。
 * Rejects invalid self-links and duplicate edges in the UI; domain algorithms still tolerate imports.
 */
function validateEdgeConnection(
  project: ProjectState,
  sourceId: string,
  targetId: string,
  type: ResearchEdgeType,
  ignoreEdgeId?: string,
) {
  if (!sourceId || !targetId || sourceId === targetId) {
    return "A relation must connect two different nodes.";
  }
  if (
    project.edges.some(
      (edge) =>
        edge.id !== ignoreEdgeId &&
        edge.source === sourceId &&
        edge.target === targetId &&
        edge.type === type,
    )
  ) {
    return "That typed relation already exists.";
  }
  const source = project.nodes.find((node) => node.id === sourceId);
  const target = project.nodes.find((node) => node.id === targetId);
  if (!source || !target) return "Both relation endpoints must exist.";
  if (type === "controls" && !["variable", "method", "dataset"].includes(source.type)) {
    return "A controls relation must start from a variable, method, or dataset.";
  }
  if (type === "measures" && !["metric", "result", "variable"].includes(target.type)) {
    return "A measures relation must target a metric, result, or variable.";
  }
  if (
    ["supports", "contradicts"].includes(type) &&
    !["hypothesis", "result", "concept", "variable", "method", "metric"].includes(target.type)
  ) {
    return "Evidence relations must target a claim, result, variable, method, or metric.";
  }
  return "";
}

/**
 * 工作区协调器。重构期间应把状态/操作与视图分离，而不是继续往此函数添加功能。
 * Workspace coordinator. New features should move state/actions and views outward rather than grow this function.
 */
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
  const [evidenceSourceFilter, setEvidenceSourceFilter] = useState("");
  const [minimumConfidence, setMinimumConfidence] = useState(0);
  const [experimentsOnly, setExperimentsOnly] = useState(false);
  const [layoutMode, setLayoutMode] = useState<LayoutMode>("neural-network");
  const [layoutAnnotations, setLayoutAnnotations] = useState<Record<string, string>>({});
  const [logicChain, setLogicChain] = useState<LogicChainResult | null>(null);
  const [logicChainMode, setLogicChainMode] = useState<LogicChainMode>("effective");
  const [influence, setInfluence] = useState<InfluenceResult | null>(null);
  const [zenMode, setZenMode] = useState(false);
  const [themeId, setThemeId] = useState("research-light");
  const [blockStyleId, setBlockStyleId] = useState<BlockStyleId>("signal-block");
  const [edgeStyleId, setEdgeStyleId] = useState("research-orthogonal");
  const [locale, setLocale] = useState<Locale>("en");
  const [installedMycPlugins, setInstalledMycPlugins] = useState<InstalledMycPlugin[]>([]);
  const [mycDropActive, setMycDropActive] = useState(false);
  const [mycInstalling, setMycInstalling] = useState(false);
  const [snapEnabled, setSnapEnabled] = useState(true);
  const [alignmentGuide, setAlignmentGuide] = useState<{ x?: number; y?: number } | null>(
    null,
  );
  const [trackpadPan, setTrackpadPan] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("display");
  const [displayDensity, setDisplayDensity] = useState<DisplayDensity>("auto");
  const [displayProfile, setDisplayProfile] = useState<DisplayProfile>({
    scaleFactor: 1,
    dpi: 96,
    source: "browser",
  });
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
  const [pathTargetId, setPathTargetId] = useState("r1");
  const [shortestPaths, setShortestPaths] = useState<string[][]>([]);
  const [graphExplanation, setGraphExplanation] = useState("");
  const [cycles, setCycles] = useState<ReturnType<typeof detectCycles>>([]);
  const [saveState, setSaveState] = useState<"saved" | "saving">("saved");
  const [past, setPast] = useState<HistoryEntry[]>([]);
  const [future, setFuture] = useState<HistoryEntry[]>([]);
  const [modal, setModal] = useState<
    "new-node" | "new-edge" | "evidence" | "split-node" | null
  >(null);
  const [projectLibraryOpen, setProjectLibraryOpen] = useState(false);
  const [projectNameDraft, setProjectNameDraft] = useState("");
  const [projectLibrary, setProjectLibrary] = useState<ProjectLibraryEntry[]>([]);
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
    authors: "",
    year: "",
    doi: "",
    page: "",
    section: "",
    quote: "",
    url: "",
    fileName: "",
    startOffset: "",
    endOffset: "",
    status: "confirmed" as "candidate" | "confirmed" | "disputed",
  });
  const [selectedSourceText, setSelectedSourceText] = useState("");
  const [editingSuggestionId, setEditingSuggestionId] = useState("");
  const [scenarioOverrideKey, setScenarioOverrideKey] = useState("value");
  const [scenarioOverrideValue, setScenarioOverrideValue] = useState("");
  const [splitParts, setSplitParts] = useState({ first: "", second: "" });
  const clipboardNode = useRef<ResearchNode | null>(null);
  const dragSnapshot = useRef<ProjectState | null>(null);
  const displayPreferencesHydrated = useRef(false);
  const responsiveCompactRef = useRef<boolean | null>(null);
  const searchInput = useRef<HTMLInputElement | null>(null);
  const importInput = useRef<HTMLInputElement | null>(null);
  const runResultInput = useRef<HTMLInputElement | null>(null);
  const evidenceFileInput = useRef<HTMLInputElement | null>(null);
  const mycPluginInput = useRef<HTMLInputElement | null>(null);
  const { fitView, setCenter, getViewport } = useReactFlow<CanvasNode, CanvasEdge>();
  const updateNodeInternals = useUpdateNodeInternals();
  const t = useCallback((key: MessageKey) => translate(locale, key), [locale]);
  const themeCatalog = useMemo(
    () => [
      ...builtInThemeCatalog,
      ...installedMycPlugins
        .map(normalizeInstalledTheme)
        .filter((theme): theme is NonNullable<typeof theme> => Boolean(theme)),
    ],
    [installedMycPlugins],
  );
  const edgeStyleCatalog = useMemo(
    () => [
      ...builtInEdgeStyleCatalog,
      ...installedMycPlugins
        .map(normalizeInstalledEdgeStyle)
        .filter((edgeStyle): edgeStyle is NonNullable<typeof edgeStyle> => Boolean(edgeStyle)),
    ],
    [installedMycPlugins],
  );
  const pluginCatalog = useMemo(
    () => [
      ...builtInPluginCatalog,
      ...installedMycPlugins.map((plugin) => ({
        id: plugin.manifest.metadata.id,
        name: plugin.manifest.metadata.name,
        version: plugin.manifest.metadata.version,
        category:
          plugin.manifest.kind === "ThemePlugin"
            ? ("theme" as const)
            : plugin.manifest.kind === "EdgeStylePlugin"
              ? ("style" as const)
              : ("analysis" as const),
        description: plugin.manifest.metadata.description,
        status: "installed" as const,
        permissions: plugin.manifest.spec.permissions,
        capabilities: plugin.manifest.spec.capabilities,
        publisher: plugin.manifest.metadata.publisher,
      })),
    ],
    [installedMycPlugins],
  );
  const activeTheme = themeCatalog.find((theme) => theme.id === themeId) ?? themeCatalog[0];
  const activeEdgeStyle =
    edgeStyleCatalog.find((edgeStyle) => edgeStyle.id === edgeStyleId) ?? edgeStyleCatalog[0];
  const uiScale = useMemo(() => {
    if (displayDensity === "auto") return recommendedUiScale(displayProfile.scaleFactor);
    if (displayDensity === "compact") return 1.2;
    if (displayDensity === "comfortable") return 1.34;
    return 1.48;
  }, [displayDensity, displayProfile.scaleFactor]);
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
    "--ui-scale": uiScale.toFixed(3),
  } as CSSProperties;

  useEffect(() => {
    const handle = window.setTimeout(() => {
      try {
        const saved = localStorage.getItem("research-canvas-project-v1");
        const savedSuggestions = localStorage.getItem("research-canvas-suggestions-v1");
        const savedLibrary = localStorage.getItem("research-canvas-project-library-v1");
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
        if (savedLibrary) {
          const parsed = JSON.parse(savedLibrary) as ProjectLibraryEntry[];
          setProjectLibrary(
            parsed
              .map((entry) => ({ ...entry, snapshot: migrateProject(entry.snapshot) }))
              .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
              .slice(0, 8),
          );
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
      setProjectLibrary((current) => {
        const entry: ProjectLibraryEntry = {
          id: project.id,
          title: project.title,
          updatedAt: project.updatedAt,
          nodeCount: project.nodes.length,
          snapshot: cloneProject(project),
        };
        const next = [entry, ...current.filter((item) => item.id !== project.id)]
          .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
          .slice(0, 8);
        localStorage.setItem("research-canvas-project-library-v1", JSON.stringify(next));
        return next;
      });
      setSaveState("saved");
    }, 450);
    return () => window.clearTimeout(handle);
  }, [project, suggestions]);

  useEffect(() => {
    const handle = window.setTimeout(() => {
      try {
        const saved = localStorage.getItem("research-canvas-display-v1");
        if (saved) {
          const preferences = JSON.parse(saved) as {
            density?: DisplayDensity;
            themeId?: string;
            blockStyleId?: BlockStyleId;
            edgeStyleId?: string;
            snapEnabled?: boolean;
            trackpadPan?: boolean;
            locale?: Locale;
          };
          if (preferences.density) setDisplayDensity(preferences.density);
          if (preferences.themeId) setThemeId(preferences.themeId);
          if (preferences.blockStyleId) setBlockStyleId(preferences.blockStyleId);
          if (preferences.edgeStyleId) setEdgeStyleId(preferences.edgeStyleId);
          setLocale(preferences.locale ?? normalizeLocale(navigator.language));
          if (typeof preferences.snapEnabled === "boolean") {
            setSnapEnabled(preferences.snapEnabled);
          }
          if (typeof preferences.trackpadPan === "boolean") {
            setTrackpadPan(preferences.trackpadPan);
          }
        } else setLocale(normalizeLocale(navigator.language));
      } catch {
        setToast("Display preferences were reset because the saved data was invalid.");
      } finally {
        displayPreferencesHydrated.current = true;
      }
    }, 0);
    return () => window.clearTimeout(handle);
  }, []);

  useEffect(() => {
    if (!displayPreferencesHydrated.current) return;
    localStorage.setItem(
      "research-canvas-display-v1",
      JSON.stringify({
        density: displayDensity,
        themeId,
        blockStyleId,
        edgeStyleId,
        snapEnabled,
        trackpadPan,
        locale,
      }),
    );
  }, [
    blockStyleId,
    displayDensity,
    edgeStyleId,
    locale,
    snapEnabled,
    themeId,
    trackpadPan,
  ]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const updateProfile = (scaleFactor: number, source: DisplayProfile["source"]) => {
      if (disposed || !Number.isFinite(scaleFactor) || scaleFactor <= 0) return;
      setDisplayProfile({
        scaleFactor,
        dpi: Math.round(96 * scaleFactor),
        source,
      });
    };

    const detectDisplay = async () => {
      if ("__TAURI_INTERNALS__" in window) {
        try {
          const { getCurrentWindow } = await import("@tauri-apps/api/window");
          const currentWindow = getCurrentWindow();
          updateProfile(await currentWindow.scaleFactor(), "tauri");
          unlisten = await currentWindow.onScaleChanged(({ payload }) => {
            updateProfile(payload.scaleFactor, "tauri");
          });
          return;
        } catch {
          // Browser fallback below also supports restricted development webviews.
        }
      }

      const updateBrowserProfile = () =>
        updateProfile(window.devicePixelRatio || 1, "browser");
      updateBrowserProfile();
      window.addEventListener("resize", updateBrowserProfile);
      unlisten = () => window.removeEventListener("resize", updateBrowserProfile);
    };

    void detectDisplay();
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

  useEffect(() => {
    const media = window.matchMedia("(max-width: 1040px)");
    const applyResponsivePanels = () => {
      if (responsiveCompactRef.current === media.matches) return;
      responsiveCompactRef.current = media.matches;
      setLeftCollapsed(media.matches);
      setRightCollapsed(media.matches);
    };
    applyResponsivePanels();
    media.addEventListener("change", applyResponsivePanels);
    return () => media.removeEventListener("change", applyResponsivePanels);
  }, []);

  const registerInstalledPlugin = useCallback(
    (plugin: InstalledMycPlugin, activateVisual = false) => {
      setInstalledMycPlugins((current) => [
        ...current.filter(
          (item) =>
            item.manifest.metadata.id !== plugin.manifest.metadata.id ||
            item.manifest.metadata.version !== plugin.manifest.metadata.version,
        ),
        plugin,
      ]);
      setLoadedPlugins((current) =>
        current.includes(plugin.manifest.metadata.id)
          ? current
          : [...current, plugin.manifest.metadata.id],
      );
      const theme = normalizeInstalledTheme(plugin);
      const edgeStyle = normalizeInstalledEdgeStyle(plugin);
      if (theme && activateVisual) setThemeId(theme.id);
      if (edgeStyle && activateVisual) setEdgeStyleId(edgeStyle.id);
    },
    [],
  );

  const installMycPaths = useCallback(
    async (paths: string[]) => {
      const packages = paths.filter(isMycFileName);
      if (!packages.length) return;
      setMycInstalling(true);
      try {
        let latest: InstalledMycPlugin | null = null;
        for (const path of packages) {
          latest = await installMycPlugin(path);
          registerInstalledPlugin(latest, true);
        }
        if (latest) {
          setToast(`${t("plugins.installedToast")}: ${latest.manifest.metadata.name}`);
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setToast(
          message === "MYC_DESKTOP_REQUIRED" ? t("plugins.desktopOnly") : `Plugin error: ${message}`,
        );
      } finally {
        setMycInstalling(false);
        setMycDropActive(false);
      }
    },
    [registerInstalledPlugin, t],
  );

  useEffect(() => {
    let disposed = false;
    void listInstalledMycPlugins()
      .then((plugins) => {
        if (disposed) return;
        for (const plugin of plugins) registerInstalledPlugin(plugin);
      })
      .catch(() => {
        // Browser preview and a missing plugin directory are both valid states.
      });
    return () => {
      disposed = true;
    };
  }, [registerInstalledPlugin]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listenForMycDrops((paths) => {
      if (pluginStoreOpen && paths.length) void installMycPaths(paths);
    }).then((dispose) => {
      unlisten = dispose;
    });
    return () => unlisten?.();
  }, [installMycPaths, pluginStoreOpen]);

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
  const evidenceSources = useMemo(
    () =>
      [...new Map(project.evidence.map((item) => [item.sourceId, item])).values()].sort((a, b) =>
        a.title.localeCompare(b.title),
      ),
    [project.evidence],
  );
  const collapsedNodeIds = useMemo(() => {
    const hidden = new Set<string>();
    for (const placement of project.placements) {
      if (!placement.collapsed) continue;
      const result = traverseGraph(project, {
        startId: placement.nodeId,
        strategy: "bfs",
        direction: "out",
        maxDepth: Number.MAX_SAFE_INTEGER,
        edgeTypes: ["depends_on", "derived_from", "part_of", "uses", "measures", "supports"],
        scenarioId: activeScenarioId || undefined,
      });
      result.order.slice(1).forEach((id) => hidden.add(id));
    }
    return hidden;
  }, [project, activeScenarioId]);
  const evidenceMatchedNodeIds = useMemo(() => {
    if (!evidenceSourceFilter) return null;
    const evidenceIds = new Set(
      project.evidence
        .filter((item) => item.sourceId === evidenceSourceFilter)
        .map((item) => item.id),
    );
    const matched = new Set(
      project.nodes
        .filter((node) => node.evidenceIds.some((id) => evidenceIds.has(id)))
        .map((node) => node.id),
    );
    for (const edge of project.edges) {
      if (edge.evidenceIds.some((id) => evidenceIds.has(id))) {
        matched.add(edge.source);
        matched.add(edge.target);
      }
    }
    return matched;
  }, [project.nodes, project.edges, project.evidence, evidenceSourceFilter]);
  const evidenceMatchedEdgeIds = useMemo(() => {
    if (!evidenceSourceFilter) return null;
    const evidenceIds = new Set(
      project.evidence
        .filter((item) => item.sourceId === evidenceSourceFilter)
        .map((item) => item.id),
    );
    return new Set(
      project.edges
        .filter((edge) => edge.evidenceIds.some((id) => evidenceIds.has(id)))
        .map((edge) => edge.id),
    );
  }, [project.edges, project.evidence, evidenceSourceFilter]);
  const visibleNodeIds = useMemo(() => {
    const typeSet = new Set(canvasNodeTypes);
    return new Set(
      project.nodes
        .filter(
          (node) =>
            !collapsedNodeIds.has(node.id) &&
            (!typeSet.size || typeSet.has(node.type)) &&
            (!evidenceMatchedNodeIds || evidenceMatchedNodeIds.has(node.id)),
        )
        .map((node) => node.id),
    );
  }, [project.nodes, canvasNodeTypes, collapsedNodeIds, evidenceMatchedNodeIds]);
  const visibleEdgeRecords = useMemo(() => {
    const edgeSet = new Set(canvasEdgeTypes);
    return project.edges.filter(
      (edge) =>
        visibleNodeIds.has(edge.source) &&
        visibleNodeIds.has(edge.target) &&
        (!edgeSet.size || edgeSet.has(edge.type)) &&
        (!evidenceMatchedEdgeIds || evidenceMatchedEdgeIds.has(edge.id)) &&
        (edge.confidence ?? 0) >= minimumConfidence &&
        (!experimentsOnly || Boolean(edge.experiment)),
    );
  }, [
    project.edges,
    visibleNodeIds,
    canvasEdgeTypes,
    evidenceMatchedEdgeIds,
    minimumConfidence,
    experimentsOnly,
  ]);

  useEffect(() => {
    const nodeIds = Array.from(visibleNodeIds);
    const frame = requestAnimationFrame(() => {
      updateNodeInternals(nodeIds);
    });
    return () => cancelAnimationFrame(frame);
  }, [blockStyleId, updateNodeInternals, visibleNodeIds]);

  const flowNodes = useMemo<CanvasNode[]>(
    () =>
      project.nodes.filter((record) => visibleNodeIds.has(record.id)).map((record) => {
        const placement = project.placements.find(
          (item) => item.nodeId === record.id && item.viewId === "view-main",
        );
        const influenceScore = influence?.scores[record.id];
        const nodeWidth = placement?.width ?? 230;
        const nodeHeight = placement?.height ?? 116;
        const portSize = blockStyleId === "signal-block" ? 10 : 8;
        const portY = nodeHeight / 2 - portSize / 2;
        return {
          id: record.id,
          type: "researchNode",
          position: { x: placement?.x ?? 0, y: placement?.y ?? 0 },
          sourcePosition: Position.Right,
          targetPosition: Position.Left,
          handles: [
            {
              type: "target",
              position: Position.Left,
              x: -portSize / 2,
              y: portY,
              width: portSize,
              height: portSize,
            },
            {
              type: "source",
              position: Position.Right,
              x: nodeWidth - portSize / 2,
              y: portY,
              width: portSize,
              height: portSize,
            },
          ],
          data: {
            record,
            blockStyleId,
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
            collapsed: Boolean(placement?.collapsed),
            pinned: Boolean(placement?.pinned),
            onResizeStart: () => {
              dragSnapshot.current = cloneProject(project);
            },
            onResizeEnd: () => {
              if (!dragSnapshot.current) return;
              setPast((items) => [
                ...items.slice(-39),
                { label: "Resize node", snapshot: dragSnapshot.current! },
              ]);
              setFuture([]);
              dragSnapshot.current = null;
            },
          },
          selected: selectedNodeId === record.id,
          draggable: true,
          width: nodeWidth,
          height: nodeHeight,
          zIndex: selectedNodeId === record.id ? 20 : 1,
        };
      }),
    [
      disabledNodes,
      traversalNodes,
      traversal,
      selectedNodeId,
      visibleNodeIds,
      logicNodeIds,
      logicChain,
      layoutAnnotations,
      influence,
      project,
      blockStyleId,
    ],
  );

  const flowEdges = useMemo<CanvasEdge[]>(
    () =>
      visibleEdgeRecords.map((record) => {
        const scenarioDisabled =
          disabledEdges.has(record.id) ||
          disabledNodes.has(record.source) ||
          disabledNodes.has(record.target);
        const chainState = logicEdgeIds.has(record.id) ? logicChain?.mode : undefined;
        const presentation = resolveEdgePresentation(activeEdgeStyle, record.type, {
          disabled: scenarioDisabled,
          traversed: traversalEdges.has(record.id),
          treeEdge: treeEdges.has(record.id),
          backEdge: backEdges.has(record.id),
          chainState,
          selected: selectedEdgeId === record.id,
        });
        const markerColor =
          presentation.color === "var(--accent)"
            ? activeTheme.colors.accent
            : presentation.color;
        return {
          id: record.id,
          type: "researchEdge",
          source: record.source,
          target: record.target,
          selected: selectedEdgeId === record.id,
          markerEnd:
            activeEdgeStyle.marker.type === "none"
              ? undefined
              : {
                  type:
                    activeEdgeStyle.marker.type === "closed-arrow"
                      ? MarkerType.ArrowClosed
                      : MarkerType.Arrow,
                  width: activeEdgeStyle.marker.size,
                  height: activeEdgeStyle.marker.size,
                  color: markerColor,
                },
          data: {
            type: record.type,
            edgeStyle: activeEdgeStyle,
            confidence: record.confidence,
            disabled: scenarioDisabled,
            traversed: traversalEdges.has(record.id),
            treeEdge: treeEdges.has(record.id),
            backEdge: backEdges.has(record.id),
            chainState,
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
      activeEdgeStyle,
      activeTheme.colors.accent,
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
    const dimensionChanges = changes.filter(
      (
        change,
      ): change is Extract<NodeChange<CanvasNode>, { type: "dimensions" }> =>
        change.type === "dimensions" && Boolean(change.dimensions),
    );
    if (dimensionChanges.length) {
      setProject((current) => {
        const next = cloneProject(current);
        for (const change of dimensionChanges) {
          const placement = next.placements.find((item) => item.nodeId === change.id);
          if (placement && change.dimensions) {
            placement.width = change.dimensions.width;
            placement.height = change.dimensions.height;
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
      const issue = validateEdgeConnection(
        project,
        connection.source ?? "",
        connection.target ?? "",
        "depends_on",
      );
      if (issue) {
        setToast(issue);
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
    [commit, project],
  );

  const onReconnect = useCallback(
    (oldEdge: CanvasEdge, connection: Connection) => {
      const record = project.edges.find((edge) => edge.id === oldEdge.id);
      if (!record) return;
      const issue = validateEdgeConnection(
        project,
        connection.source ?? "",
        connection.target ?? "",
        record.type,
        record.id,
      );
      if (issue) {
        setToast(issue);
        return;
      }
      commit("Reconnect typed relation", (draft) => {
        const edge = draft.edges.find((item) => item.id === record.id);
        if (edge) {
          edge.source = connection.source!;
          edge.target = connection.target!;
        }
      });
      setToast("Relation endpoint reconnected.");
    },
    [commit, project],
  );

  const focusNode = useCallback(
    (nodeId: string) => {
      const placement = project.placements.find((item) => item.nodeId === nodeId);
      if (!placement) return;
      setSelectedNodeId(nodeId);
      setSelectedEdgeId("");
      setProject((current) => {
        const next = cloneProject(current);
        next.navigation ??= { recentNodeIds: [], pinnedNodeIds: [] };
        next.navigation.recentNodeIds = [
          nodeId,
          ...next.navigation.recentNodeIds.filter((id) => id !== nodeId),
        ].slice(0, 6);
        return next;
      });
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
    setShortestPaths([]);
    setGraphExplanation("");
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

  const runShortestPath = useCallback(() => {
    const sourceId = selectedNodeId || project.nodes[0]?.id;
    if (!sourceId || !pathTargetId) {
      setToast("Choose a source node and target node.");
      return;
    }
    const paths = allShortestPaths(
      project,
      sourceId,
      pathTargetId,
      activeScenarioId || undefined,
    );
    setShortestPaths(paths);
    if (!paths.length) {
      setTraversal(null);
      setGraphExplanation(
        `No directed path exists from the selected source to the target under ${
          activeScenario?.name ?? "the base graph"
        }.`,
      );
      setToast("No directed path found.");
      return;
    }
    const nodeIds = [...new Set(paths.flat())];
    const edgeRecords = resolveEdges(project, activeScenarioId || undefined);
    const edgeIds = new Set<string>();
    for (const path of paths) {
      for (let index = 0; index < path.length - 1; index += 1) {
        const edge = edgeRecords.find(
          (candidate) =>
            candidate.source === path[index] && candidate.target === path[index + 1],
        );
        if (edge) edgeIds.add(edge.id);
      }
    }
    const first = paths[0];
    setTraversal({
      strategy: "bfs",
      startId: sourceId,
      order: nodeIds,
      edgeIds: [...edgeIds],
      depth: Object.fromEntries(first.map((id, index) => [id, index])),
      parent: Object.fromEntries(
        first.map((id, index) => [id, index ? first[index - 1] : null]),
      ),
      treeEdgeIds: [...edgeIds],
      crossEdgeIds: [],
      backEdgeIds: [],
      stoppedByDepth: [],
      durationMs: 0,
    });
    setGraphExplanation(
      `${paths.length} equally short directed path${paths.length === 1 ? "" : "s"} found at ${
        first.length - 1
      } relations. Scenario and evidence filters were applied before traversal.`,
    );
    setBottomTab("traversal");
    setBottomExpanded(true);
    setToast(`${paths.length} shortest path${paths.length === 1 ? "" : "s"} highlighted.`);
  }, [activeScenario, activeScenarioId, pathTargetId, project, selectedNodeId]);

  const explainSelectedNeighborhood = useCallback(() => {
    if (!selectedNode) {
      setGraphExplanation("Select a node before requesting a local explanation.");
      return;
    }
    const incoming = project.edges.filter((edge) => edge.target === selectedNode.id);
    const outgoing = project.edges.filter((edge) => edge.source === selectedNode.id);
    const evidenceCount = new Set([
      ...selectedNode.evidenceIds,
      ...incoming.flatMap((edge) => edge.evidenceIds),
      ...outgoing.flatMap((edge) => edge.evidenceIds),
    ]).size;
    const strongest = [...incoming, ...outgoing]
      .sort((a, b) => (b.confidence ?? 0) - (a.confidence ?? 0))[0];
    setGraphExplanation(
      `${selectedNode.title} has ${incoming.length} incoming and ${outgoing.length} outgoing typed relations, backed by ${evidenceCount} distinct evidence record${
        evidenceCount === 1 ? "" : "s"
      }. ${
        strongest
          ? `The highest-confidence local relation is “${edgeTypeLabels[strongest.type]}” at ${Math.round(
              (strongest.confidence ?? 0) * 100,
            )}%.`
          : "It is currently isolated."
      } This explanation reads only the selected one-hop neighborhood.`,
    );
    setBottomTab("traversal");
    setBottomExpanded(true);
  }, [project.edges, selectedNode]);

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
    setSuggestions(JSON.parse(JSON.stringify(mnistSuggestions)) as GraphSuggestion[]);
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
      window.setTimeout(() => fitView({ padding: 0.16, duration: 550 }), 100);
    }, 80);
    setToast(`Git plugin loaded MNIST study at ${mnistRunSummary.environment.gitCommit}.`);
  }, [fitView]);

  const loadSocialScienceStudy = useCallback(() => {
    const next = createSocialScienceProject();
    setProject(cloneProject(next));
    setSuggestions([]);
    setPast([]);
    setFuture([]);
    setTraversal(null);
    setCycles([]);
    setInfluence(null);
    setLogicChain(null);
    setSelectedNodeId("soc-polarization");
    setSelectedEdgeId("");
    setLayoutMode("evidence-chain");
    setPluginStoreOpen(false);
    window.setTimeout(() => {
      const layout = computeLayout(next, "evidence-chain", "soc-q");
      setProject((current) => {
        const laidOut = cloneProject(current);
        for (const [nodeId, position] of Object.entries(layout.positions)) {
          const placement = laidOut.placements.find((item) => item.nodeId === nodeId);
          if (placement) Object.assign(placement, position);
        }
        return laidOut;
      });
      setLayoutAnnotations(layout.annotations);
      window.setTimeout(() => fitView({ padding: 0.16, duration: 550 }), 100);
    }, 80);
    setToast("Social-science acceptance fixture loaded.");
  }, [fitView]);

  const openProjectLibrary = useCallback(() => {
    setProjectNameDraft(project.title);
    setProjectLibraryOpen(true);
  }, [project.title]);

  const startNewProject = useCallback(() => {
    const next = createBlankProject("Untitled research project");
    setProject(next);
    setPast([]);
    setFuture([]);
    setSelectedNodeId(next.nodes[0]?.id ?? "");
    setSelectedEdgeId("");
    setActiveScenarioId("");
    setTraversal(null);
    setLogicChain(null);
    setInfluence(null);
    setProjectNameDraft(next.title);
    setProjectLibraryOpen(false);
    setToast("New local project created.");
  }, []);

  const openLibraryProject = useCallback((entry: ProjectLibraryEntry) => {
    const next = migrateProject(entry.snapshot);
    setProject(cloneProject(next));
    setPast([]);
    setFuture([]);
    setSelectedNodeId(
      next.navigation?.recentNodeIds[0] ??
        next.nodes.find((node) => node.type === "question")?.id ??
        next.nodes[0]?.id ??
        "",
    );
    setSelectedEdgeId("");
    setActiveScenarioId("");
    setProjectNameDraft(next.title);
    setProjectLibraryOpen(false);
    setToast(`Opened “${next.title}”.`);
  }, []);

  const renameCurrentProject = useCallback(() => {
    const title = projectNameDraft.trim();
    if (!title || title === project.title) return;
    commit("Rename project", (draft) => {
      draft.title = title;
    });
    setToast("Project renamed.");
  }, [commit, project.title, projectNameDraft]);

  const toggleCollapsedSubtree = useCallback(() => {
    if (!selectedNode) return;
    commit("Toggle collapsed subtree", (draft) => {
      const placement = draft.placements.find((item) => item.nodeId === selectedNode.id);
      if (placement) placement.collapsed = !placement.collapsed;
    });
    window.setTimeout(() => fitView({ padding: 0.16, duration: 350 }), 60);
  }, [commit, fitView, selectedNode]);

  const togglePinnedNode = useCallback(() => {
    if (!selectedNode) return;
    commit("Toggle pinned node", (draft) => {
      const placement = draft.placements.find((item) => item.nodeId === selectedNode.id);
      if (!placement) return;
      placement.pinned = !placement.pinned;
      draft.navigation ??= { recentNodeIds: [], pinnedNodeIds: [] };
      draft.navigation.pinnedNodeIds = placement.pinned
        ? [
            selectedNode.id,
            ...draft.navigation.pinnedNodeIds.filter((id) => id !== selectedNode.id),
          ]
        : draft.navigation.pinnedNodeIds.filter((id) => id !== selectedNode.id);
    });
  }, [commit, selectedNode]);

  const copySelectedNode = useCallback(() => {
    if (!selectedNode) return;
    clipboardNode.current = JSON.parse(JSON.stringify(selectedNode)) as ResearchNode;
    setToast("Node copied to the Research Canvas clipboard.");
  }, [selectedNode]);

  const pasteCopiedNode = useCallback(() => {
    const copied = clipboardNode.current;
    if (!copied) {
      setToast("Copy a node before pasting.");
      return;
    }
    const id = makeId("node");
    commit("Paste node", (draft) => {
      draft.nodes.push({
        ...JSON.parse(JSON.stringify(copied)),
        id,
        title: `${copied.title} copy`,
        status: "draft",
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      });
      const sourcePlacement = draft.placements.find((item) => item.nodeId === copied.id);
      draft.placements.push({
        id: `placement-${id}`,
        viewId: "view-main",
        nodeId: id,
        x: (sourcePlacement?.x ?? 320) + 44,
        y: (sourcePlacement?.y ?? 260) + 44,
        width: sourcePlacement?.width ?? 230,
        height: sourcePlacement?.height ?? 116,
      });
    });
    setSelectedNodeId(id);
    setSelectedEdgeId("");
  }, [commit]);

  const stageSelectionAsSuggestions = useCallback(() => {
    const text = selectedSourceText.trim();
    if (!text) {
      setToast("Paste or select source text first.");
      return;
    }
    const candidates = text
      .split(/\n|[.;。；]/)
      .map((value) => value.trim())
      .filter((value) => value.length >= 8)
      .slice(0, 4);
    const createdAt = new Date().toISOString();
    const staged: GraphSuggestion[] = candidates.map((value, index) => ({
      id: makeId(`selection-${index}`),
      kind: "node",
      operation: "add",
      title: value.length > 58 ? `${value.slice(0, 55)}…` : value,
      description: value,
      confidence: Math.max(0.58, 0.78 - index * 0.04),
      rationale:
        "This phrase names a potentially testable research object. The researcher must verify its type and scope.",
      evidenceLabel: `Selected text · ${createdAt.slice(0, 10)} · candidate`,
      status: "proposed",
      node: {
        type: index === 0 ? "hypothesis" : "variable",
        title: value.length > 58 ? `${value.slice(0, 55)}…` : value,
        body: value,
        tags: ["text-extraction", "candidate"],
        status: "draft",
        evidenceIds: [],
        data: { selectedText: text },
        provenance: {
          origin: "ai",
          modelId: "local-extraction-adapter",
          promptVersion: "selected-text-v0.1",
          sourceRefs: [`selection:${createdAt}`],
        },
      },
    }));
    setSuggestions((items) => [...staged, ...items]);
    setSelectedSourceText("");
    setToast(`${staged.length} structured candidates staged; the graph is unchanged.`);
  }, [selectedSourceText]);

  const applyScenarioOverride = useCallback(() => {
    if (!activeScenario || !selectedNode || !scenarioOverrideKey.trim()) {
      setToast("Choose an active scenario and a node before applying an override.");
      return;
    }
    const raw = scenarioOverrideValue.trim();
    const numeric = Number(raw);
    const value: unknown =
      raw === "true" ? true : raw === "false" ? false : raw && Number.isFinite(numeric) ? numeric : raw;
    commit("Set scenario property override", (draft) => {
      const scenario = draft.scenarios.find((item) => item.id === activeScenario.id);
      if (!scenario) return;
      const existing = scenario.nodeOverrides[selectedNode.id] ?? {};
      scenario.nodeOverrides[selectedNode.id] = {
        ...existing,
        data: {
          ...selectedNode.data,
          ...((existing.data as Record<string, unknown> | undefined) ?? {}),
          [scenarioOverrideKey.trim()]: value,
        },
      };
    });
    setToast("Scenario-only property override saved.");
  }, [
    activeScenario,
    commit,
    scenarioOverrideKey,
    scenarioOverrideValue,
    selectedNode,
  ]);

  const toggleSelectionInScenario = useCallback(() => {
    if (!activeScenario || (!selectedNodeId && !selectedEdgeId)) {
      setToast("Choose an active scenario and select a node or relation.");
      return;
    }
    commit("Toggle scenario disabled item", (draft) => {
      const scenario = draft.scenarios.find((item) => item.id === activeScenario.id);
      if (!scenario) return;
      if (selectedNodeId) {
        scenario.disabledNodeIds = scenario.disabledNodeIds.includes(selectedNodeId)
          ? scenario.disabledNodeIds.filter((id) => id !== selectedNodeId)
          : [...scenario.disabledNodeIds, selectedNodeId];
      }
      if (selectedEdgeId) {
        scenario.disabledEdgeIds = scenario.disabledEdgeIds.includes(selectedEdgeId)
          ? scenario.disabledEdgeIds.filter((id) => id !== selectedEdgeId)
          : [...scenario.disabledEdgeIds, selectedEdgeId];
      }
    });
    setToast("Scenario overlay updated; the base graph is unchanged.");
  }, [activeScenario, commit, selectedEdgeId, selectedNodeId]);

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
    const issue = validateEdgeConnection(
      project,
      newEdge.source,
      newEdge.target,
      newEdge.type,
    );
    if (issue) {
      setToast(issue);
      return;
    }
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
  }, [newEdge, commit, project]);

  const createEvidence = useCallback(() => {
    if (!selectedNode || !newEvidence.title.trim()) return;
    const evidenceId = makeId("evidence");
    commit("Attach evidence", (draft) => {
      draft.evidence.push({
        id: evidenceId,
        sourceType: "paper",
        sourceId: makeId("source"),
        title: newEvidence.title.trim(),
        authors: newEvidence.authors.trim() || undefined,
        year: newEvidence.year ? Number(newEvidence.year) : undefined,
        doi: newEvidence.doi.trim() || undefined,
        url: newEvidence.url.trim() || undefined,
        locator: {
          fileName: newEvidence.fileName || undefined,
          page: newEvidence.page ? Number(newEvidence.page) : undefined,
          section: newEvidence.section.trim() || undefined,
          quote: newEvidence.quote.trim() || undefined,
          startOffset: newEvidence.startOffset
            ? Number(newEvidence.startOffset)
            : undefined,
          endOffset: newEvidence.endOffset ? Number(newEvidence.endOffset) : undefined,
        },
        status: newEvidence.status,
        provenance: { origin: "human", actorId: "local-researcher" },
      });
      const node = draft.nodes.find((item) => item.id === selectedNode.id);
      node?.evidenceIds.push(evidenceId);
    });
    setNewEvidence({
      title: "",
      authors: "",
      year: "",
      doi: "",
      page: "",
      section: "",
      quote: "",
      url: "",
      fileName: "",
      startOffset: "",
      endOffset: "",
      status: "confirmed",
    });
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
            const reviewedEdge = {
              ...suggestion.edge,
              provenance: {
                ...suggestion.edge.provenance,
                reviewedBy: "local-researcher",
                reviewedAt: new Date().toISOString(),
              },
            };
            const existing =
              suggestion.operation === "update"
                ? draft.edges.find(
                    (edge) =>
                      edge.source === suggestion.edge!.source &&
                      edge.target === suggestion.edge!.target,
                  )
                : undefined;
            if (existing) Object.assign(existing, reviewedEdge);
            else draft.edges.push({ ...reviewedEdge, id: makeId("edge") });
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
            const existing =
              suggestion.operation === "update"
                ? draft.edges.find(
                    (edge) =>
                      edge.source === suggestion.edge!.source &&
                      edge.target === suggestion.edge!.target,
                  )
                : undefined;
            if (existing) {
              Object.assign(existing, suggestion.edge, {
                provenance: {
                  ...suggestion.edge.provenance,
                  reviewedBy: "local-researcher",
                  reviewedAt: new Date().toISOString(),
                },
              });
            } else {
              draft.edges.push({ ...suggestion.edge, id: makeId(`edge-${index}`) });
            }
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
          artifact?: Record<string, unknown>;
        };
        const id = makeId("result");
        const evidenceId = makeId("evidence");
        commit(
          "Import external run result",
          (draft) => {
            draft.evidence.push({
              id: evidenceId,
              sourceType: "experiment",
              sourceId: result.scenarioId ?? file.name,
              title: result.metric
                ? `External run: ${result.metric}`
                : "External connector run",
              locator: {
                fileName: file.name,
                quote:
                  result.summary ??
                  "Structured RunResult imported through the connector protocol.",
              },
              status: "candidate",
              provenance: { origin: "python", sourceRefs: [file.name] },
            });
            draft.nodes.push({
              id,
              type: "result",
              title: result.metric ? `${result.metric}: ${result.value ?? "complete"}` : "Imported run result",
              body: result.summary ?? "External result imported through the connector protocol.",
              tags: ["run-result", result.scenarioId ?? activeScenarioId].filter(Boolean),
              status: "draft",
              evidenceIds: [evidenceId],
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
      if (command && event.key === ",") {
        event.preventDefault();
        setSettingsOpen(true);
        return;
      }
      if (command && event.key === "Enter") {
        event.preventDefault();
        runTraversal();
        return;
      }
      if (command && event.key.toLowerCase() === "c" && selectedNodeId && !editing) {
        event.preventDefault();
        copySelectedNode();
        return;
      }
      if (command && event.key.toLowerCase() === "v" && !editing) {
        event.preventDefault();
        pasteCopiedNode();
        return;
      }
      if (command && event.key.toLowerCase() === "d" && selectedNodeId && !editing) {
        event.preventDefault();
        duplicateSelected();
        return;
      }
      if (command && event.key.toLowerCase() === "z" && !editing) {
        event.preventDefault();
        if (event.shiftKey) redo();
        else undo();
        return;
      }
      if (command && event.key.toLowerCase() === "y" && !editing) {
        event.preventDefault();
        redo();
        return;
      }
      if (event.key === "Escape") {
        setModal(null);
        setPluginStoreOpen(false);
        setProjectLibraryOpen(false);
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
  }, [
    copySelectedNode,
    deleteSelected,
    duplicateSelected,
    fitView,
    pasteCopiedNode,
    redo,
    runTraversal,
    selectedEdgeId,
    selectedNodeId,
    toggleZenMode,
    undo,
  ]);

  return (
    <main
      className={`app-shell ${zenMode ? "zen-mode" : ""} ${
        activeTheme.source === "myc" ? "plugin-theme-active" : ""
      }`}
      data-testid="research-canvas-app"
      data-theme={activeTheme.id}
      data-block-style={blockStyleId}
      data-edge-style={activeEdgeStyle.id}
      data-edge-routing={activeEdgeStyle.routing}
      style={themeStyle}
    >
      <header className="topbar">
        <div className="brand">
          <div className="brand-mark">
            <Network size={18} strokeWidth={2.4} />
          </div>
          <div>
            <div className="brand-name">{t("app.name")}</div>
            <button
              className="project-breadcrumb"
              onClick={openProjectLibrary}
              data-testid="project-switcher"
            >
              {t("app.workspace")} <ChevronRight size={11} /> {project.title}
              <ChevronDown size={11} />
            </button>
          </div>
        </div>

        <div className="topbar-center">
          <span className="discipline-chip">{project.discipline}</span>
          <span className="save-status" aria-live="polite">
            <span className={`save-dot ${saveState}`} />
            {saveState === "saved" ? t("status.saved") : t("status.saving")}
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
              <span className="responsive-label">{t("toolbar.layout")}</span>
            </button>
          </div>
          <button
            className="icon-button"
            onClick={toggleZenMode}
            title={`${t("toolbar.zen")} (Z)`}
            aria-label={t("toolbar.zen")}
          >
            <Focus size={16} />
          </button>
          <button
            className="icon-button"
            onClick={() => setPluginStoreOpen(true)}
            title={`${t("toolbar.plugins")} (Ctrl/Cmd+Shift+P)`}
            aria-label={t("toolbar.plugins")}
          >
            <ShoppingBag size={16} />
          </button>
          <button
            className="icon-button"
            onClick={() => setSettingsOpen(true)}
            title={t("toolbar.settings")}
            aria-label={t("toolbar.settings")}
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
              <span className="responsive-label">{t("toolbar.export")}</span>
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
                    downloadText(
                      "long-context-ablation.research.json",
                      JSON.stringify(project, null, 2),
                    );
                    setExportOpen(false);
                    setToast("Obsidian Canvas and semantic sidecar exported.");
                  }}
                >
                  <Braces size={16} />
                  <span>
                    <strong>Obsidian bundle</strong>
                    <small>.canvas + semantic sidecar</small>
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
                {activeScenario && (
                  <button
                    onClick={() => {
                      downloadText(
                        `${activeScenario.id}.scenario.json`,
                        JSON.stringify(
                          {
                            schemaVersion: 1,
                            projectId: project.id,
                            projectRevision: project.revision,
                            scenario: activeScenario,
                          },
                          null,
                          2,
                        ),
                      );
                      setExportOpen(false);
                    }}
                  >
                    <CircleDotDashed size={16} />
                    <span>
                      <strong>Active scenario</strong>
                      <small>Stable overlay JSON</small>
                    </span>
                  </button>
                )}
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
            <span>{t("nav.navigator")}</span>
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
              placeholder={t("nav.search")}
              aria-label="Search graph"
            />
            <kbd>⌘K</kbd>
          </div>

          <div className="sidebar-scroll">
            <section className="sidebar-section">
              <div className="section-label">
                {t("nav.views")} <span>3</span>
              </div>
              <button className="nav-row active">
                <Network size={15} />
                {t("nav.researchMap")}
                <span className="count-badge">{project.nodes.length}</span>
              </button>
              <button className="nav-row" onClick={applyTreeLayout}>
                <ListTree size={15} />
                {t("nav.dependencyTree")}
                <span className="muted-badge">BFS</span>
              </button>
              <button className="nav-row" onClick={() => setActiveInspectorTab("evidence")}>
                <FileText size={15} />
                {t("nav.evidenceMap")}
                <span className="count-badge">{project.evidence.length}</span>
              </button>
            </section>

            <section className="sidebar-section">
              <div className="section-label">
                {t("nav.pinnedRecent")} <Pin size={12} />
              </div>
              {(project.navigation?.pinnedNodeIds ?? []).slice(0, 4).map((id) => {
                const node = project.nodes.find((item) => item.id === id);
                return node ? (
                  <button className="node-result pinned" key={`pinned-${id}`} onClick={() => focusNode(id)}>
                    <Pin size={12} />
                    <span>{node.title}</span>
                  </button>
                ) : null;
              })}
              {(project.navigation?.recentNodeIds ?? [])
                .filter((id) => !project.navigation?.pinnedNodeIds.includes(id))
                .slice(0, 4)
                .map((id) => {
                  const node = project.nodes.find((item) => item.id === id);
                  return node ? (
                    <button className="node-result" key={`recent-${id}`} onClick={() => focusNode(id)}>
                      <History size={12} />
                      <span>{node.title}</span>
                    </button>
                  ) : null;
                })}
              {!project.navigation?.recentNodeIds.length &&
                !project.navigation?.pinnedNodeIds.length && (
                  <p className="sidebar-empty">Visited and pinned nodes appear here.</p>
                )}
            </section>

            <section className="sidebar-section">
              <div className="section-label">
                {t("nav.evidenceSource")} <FileText size={12} />
              </div>
              <select
                className="sidebar-select"
                value={evidenceSourceFilter}
                onChange={(event) => setEvidenceSourceFilter(event.target.value)}
                aria-label="Filter by evidence source"
              >
                <option value="">All evidence sources</option>
                {evidenceSources.map((source) => (
                  <option key={source.sourceId} value={source.sourceId}>
                    {source.title}
                  </option>
                ))}
              </select>
            </section>

            <section className="sidebar-section">
              <div className="section-label">
                {t("nav.scenarios")} <span>{project.scenarios.length + 1}</span>
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
                {t("nav.nodeTypes")} <Filter size={12} />
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
              <span>{t("toolbar.node")}</span>
            </button>
            <button className="tool-button" onClick={() => setModal("new-edge")}>
              <Link2 size={15} />
              <span>{t("toolbar.relation")}</span>
            </button>
            <div className="canvas-filter-wrap">
              <button
                className={`tool-button ${canvasFilterOpen ? "active" : ""}`}
                onClick={() => setCanvasFilterOpen((value) => !value)}
                data-testid="canvas-filter"
              >
                <SlidersHorizontal size={15} />
                <span>{t("toolbar.filter")}</span>
                {(canvasNodeTypes.length ||
                  canvasEdgeTypes.length ||
                  evidenceSourceFilter ||
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
                        setEvidenceSourceFilter("");
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
                  <label className="field-label compact-filter-field">
                    Evidence source
                    <select
                      value={evidenceSourceFilter}
                      onChange={(event) => setEvidenceSourceFilter(event.target.value)}
                    >
                      <option value="">Any source</option>
                      {evidenceSources.map((source) => (
                        <option key={source.sourceId} value={source.sourceId}>
                          {source.title}
                        </option>
                      ))}
                    </select>
                  </label>
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
                <span>{t("toolbar.highlight")}</span>
              </button>
            </div>
            <div className="toolbar-divider" />
            <button
              className="tool-button"
              onClick={copySelectedNode}
              disabled={!selectedNode}
              title="Copy selected node (Ctrl/Cmd+C)"
              aria-label="Copy selected node"
            >
              <ClipboardCopy size={15} />
            </button>
            <button
              className="tool-button"
              onClick={pasteCopiedNode}
              title="Paste copied node (Ctrl/Cmd+V)"
              aria-label="Paste copied node"
            >
              <ClipboardPaste size={15} />
            </button>
            <button
              className="tool-button"
              onClick={duplicateSelected}
              disabled={!selectedNode}
              title="Duplicate selected node"
              aria-label="Duplicate selected node"
            >
              <GitFork size={15} />
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
              aria-label="Split selected node"
            >
              <Scissors size={15} />
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
            <button
              className="tool-button"
              onClick={() => fitView({ padding: 0.16, duration: 450 })}
              aria-label="Fit view"
              title="Fit view"
            >
              <Maximize2 size={15} />
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
            onReconnect={onReconnect}
            onError={(code, message) => {
              setToast(`Canvas ${code}: ${message}`);
            }}
            edgesReconnectable
            reconnectRadius={18}
            onNodeClick={(_, node) => {
              setSelectedNodeId(node.id);
              setSelectedEdgeId("");
              setActiveInspectorTab("properties");
              setProject((current) => {
                const next = cloneProject(current);
                next.navigation ??= { recentNodeIds: [], pinnedNodeIds: [] };
                next.navigation.recentNodeIds = [
                  node.id,
                  ...next.navigation.recentNodeIds.filter((id) => id !== node.id),
                ].slice(0, 6);
                return next;
              });
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
            onNodeDrag={(_, node) => {
              if (!snapEnabled) return;
              const alignedX = project.placements.find(
                (item) =>
                  item.nodeId !== node.id && Math.abs(item.x - node.position.x) <= 8,
              )?.x;
              const alignedY = project.placements.find(
                (item) =>
                  item.nodeId !== node.id && Math.abs(item.y - node.position.y) <= 8,
              )?.y;
              const viewport = getViewport();
              setAlignmentGuide(
                typeof alignedX === "number" || typeof alignedY === "number"
                  ? {
                      x:
                        typeof alignedX === "number"
                          ? alignedX * viewport.zoom + viewport.x
                          : undefined,
                      y:
                        typeof alignedY === "number"
                          ? alignedY * viewport.zoom + viewport.y
                          : undefined,
                    }
                  : null,
              );
            }}
            onNodeDragStop={() => {
              setAlignmentGuide(null);
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
            {typeof alignmentGuide?.x === "number" && (
              <div
                className="alignment-guide vertical"
                style={{ left: alignmentGuide.x }}
                aria-hidden="true"
              />
            )}
            {typeof alignmentGuide?.y === "number" && (
              <div
                className="alignment-guide horizontal"
                style={{ top: alignmentGuide.y }}
                aria-hidden="true"
              />
            )}
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
              maskColor={
                activeTheme.source === "myc"
                  ? "rgba(30, 34, 42, 0.78)"
                  : "rgba(247, 248, 250, 0.78)"
              }
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
                  <button className="secondary-button" onClick={toggleCollapsedSubtree}>
                    <ListTree size={15} />
                    {project.placements.find((item) => item.nodeId === selectedNode.id)?.collapsed
                      ? "Expand subtree"
                      : "Collapse subtree"}
                  </button>
                  <button className="secondary-button" onClick={togglePinnedNode}>
                    {project.placements.find((item) => item.nodeId === selectedNode.id)?.pinned ? (
                      <PinOff size={15} />
                    ) : (
                      <Pin size={15} />
                    )}
                    {project.placements.find((item) => item.nodeId === selectedNode.id)?.pinned
                      ? "Unpin node"
                      : "Pin node"}
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
                <button
                  className="secondary-button"
                  onClick={toggleSelectionInScenario}
                  disabled={!activeScenario}
                >
                  <CircleDotDashed size={15} />
                  {activeScenario?.disabledEdgeIds.includes(selectedEdge.id)
                    ? "Enable in scenario"
                    : "Disable in scenario"}
                </button>
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
                      {selectedNode || selectedEdge
                        ? `${
                            selectedNode?.evidenceIds.length ?? selectedEdge?.evidenceIds.length ?? 0
                          } bound records`
                        : "Select a node or relation"}
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
                {(selectedNode?.evidenceIds ?? selectedEdge?.evidenceIds ?? []).map((evidenceId) => {
                  const evidence = project.evidence.find((item) => item.id === evidenceId);
                  if (!evidence) return null;
                  const backlinks = evidenceBacklinks(project, evidence.id);
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
                        {[
                          evidence.locator.fileName,
                          evidence.locator.section ?? evidence.sourceType,
                          typeof evidence.locator.startOffset === "number"
                            ? `${evidence.locator.startOffset}–${evidence.locator.endOffset ?? "?"}`
                            : "",
                        ]
                          .filter(Boolean)
                          .join(" · ")}
                      </div>
                      <div className="evidence-backlinks">
                        Used by {backlinks.nodeIds.length} node
                        {backlinks.nodeIds.length === 1 ? "" : "s"} and{" "}
                        {backlinks.edgeIds.length} relation
                        {backlinks.edgeIds.length === 1 ? "" : "s"}
                      </div>
                    </article>
                  );
                })}
                {(selectedNode || selectedEdge) &&
                  (selectedNode?.evidenceIds.length ?? selectedEdge?.evidenceIds.length ?? 0) ===
                    0 && (
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
                <div className="selection-extractor">
                  <label className="field-label">
                    Selected paper text
                    <textarea
                      rows={4}
                      value={selectedSourceText}
                      onChange={(event) => setSelectedSourceText(event.target.value)}
                      placeholder="Paste a precise passage. Extraction creates staged candidates only."
                    />
                  </label>
                  <button className="secondary-button" onClick={stageSelectionAsSuggestions}>
                    <Sparkles size={14} />
                    Extract candidates
                  </button>
                </div>
                <div className="graph-patch-summary">
                  {(["add", "update", "delete"] as const).map((operation) => (
                    <span key={operation} className={`operation-${operation}`}>
                      {operation}{" "}
                      {
                        suggestions.filter(
                          (item) => (item.operation ?? "add") === operation,
                        ).length
                      }
                    </span>
                  ))}
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
                        {suggestion.operation ?? "add"} {suggestion.kind}
                      </span>
                      <span className="confidence">
                        {Math.round(suggestion.confidence * 100)}%
                      </span>
                    </div>
                    {editingSuggestionId === suggestion.id ? (
                      <>
                        <input
                          className="suggestion-edit-title"
                          value={suggestion.title}
                          onChange={(event) =>
                            setSuggestions((items) =>
                              items.map((item) =>
                                item.id === suggestion.id
                                  ? {
                                      ...item,
                                      title: event.target.value,
                                      node: item.node
                                        ? { ...item.node, title: event.target.value }
                                        : item.node,
                                    }
                                  : item,
                              ),
                            )
                          }
                        />
                        <textarea
                          rows={3}
                          value={suggestion.description}
                          onChange={(event) =>
                            setSuggestions((items) =>
                              items.map((item) =>
                                item.id === suggestion.id
                                  ? {
                                      ...item,
                                      description: event.target.value,
                                      node: item.node
                                        ? { ...item.node, body: event.target.value }
                                        : item.node,
                                    }
                                  : item,
                              ),
                            )
                          }
                        />
                      </>
                    ) : (
                      <>
                        <h3>{suggestion.title}</h3>
                        <p>{suggestion.description}</p>
                      </>
                    )}
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
                        <button
                          className="edit-button"
                          onClick={() =>
                            setEditingSuggestionId((current) =>
                              current === suggestion.id ? "" : suggestion.id,
                            )
                          }
                        >
                          {editingSuggestionId === suggestion.id ? "Done" : "Edit"}
                        </button>
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
                <label className="path-target-control">
                  Path target
                  <select
                    value={pathTargetId}
                    onChange={(event) => setPathTargetId(event.target.value)}
                  >
                    {project.nodes.map((node) => (
                      <option key={node.id} value={node.id}>
                        {node.title}
                      </option>
                    ))}
                  </select>
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
                <button className="secondary-button compact" onClick={runShortestPath}>
                  <Link2 size={14} />
                  Shortest path
                </button>
                <button className="secondary-button compact" onClick={explainSelectedNeighborhood}>
                  <Sparkles size={14} />
                  Explain local graph
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
                <div className="algorithm-explanation">
                  <span className="eyebrow">Algorithm explanation</span>
                  <strong>
                    {shortestPaths.length
                      ? `${shortestPaths.length} equally short path${
                          shortestPaths.length === 1 ? "" : "s"
                        }`
                      : `${traversalStrategy.toUpperCase()} · ${traversalDirection} · depth ${maxDepth}`}
                  </strong>
                  <p>
                    {graphExplanation ||
                      (traversal
                        ? `Applied ${
                            edgeTypeFilter.length
                              ? edgeTypeFilter.map((type) => edgeTypeLabels[type]).join(", ")
                              : "all relation types"
                          } in ${
                            activeScenario?.name ?? "the base graph"
                          }. ${
                            traversal.stoppedByDepth.length
                              ? `${traversal.stoppedByDepth.length} branch(es) stopped at the depth limit.`
                              : "No branch was stopped by the depth limit."
                          }`
                        : "Run an analysis to see its parameters, filters, and stopping reason.")}
                  </p>
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
                <>
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
                  <details className="scenario-override-editor">
                    <summary>
                      <span>
                        <strong>Scenario overrides</strong>
                        <small>
                          {Object.keys(activeScenario.nodeOverrides).length +
                            Object.keys(activeScenario.edgeOverrides).length}{" "}
                          overridden object(s) · base graph stays unchanged
                        </small>
                      </span>
                      <ChevronDown size={15} />
                    </summary>
                    <div className="scenario-override-form">
                      <label>
                        Property
                        <input
                          value={scenarioOverrideKey}
                          onChange={(event) => setScenarioOverrideKey(event.target.value)}
                          placeholder="value"
                        />
                      </label>
                      <label>
                        Scenario value
                        <input
                          value={scenarioOverrideValue}
                          onChange={(event) => setScenarioOverrideValue(event.target.value)}
                          placeholder="e.g. 16, tanh, false"
                        />
                      </label>
                      <button
                        className="secondary-button"
                        onClick={applyScenarioOverride}
                        disabled={!selectedNode}
                      >
                        Apply to selected
                      </button>
                      <button className="secondary-button" onClick={toggleSelectionInScenario}>
                        <CircleDotDashed size={14} />
                        Toggle selected
                      </button>
                      <button
                        className="secondary-button"
                        onClick={() =>
                          downloadText(
                            `${activeScenario.id}.scenario.json`,
                            JSON.stringify(
                              {
                                schemaVersion: 1,
                                projectId: project.id,
                                projectRevision: project.revision,
                                scenario: activeScenario,
                              },
                              null,
                              2,
                            ),
                          )
                        }
                      >
                        <Download size={14} />
                        Export
                      </button>
                    </div>
                  </details>
                </>
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
      <input
        ref={mycPluginInput}
        className="hidden-input"
        type="file"
        accept=".myc,application/zip"
        onChange={(event) => {
          const file = event.target.files?.[0] as (File & { path?: string }) | undefined;
          if (file?.path) void installMycPaths([file.path]);
          else if (file) setToast(t("plugins.desktopOnly"));
          event.target.value = "";
        }}
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
                    Authors
                    <input
                      value={newEvidence.authors}
                      onChange={(event) =>
                        setNewEvidence((current) => ({
                          ...current,
                          authors: event.target.value,
                        }))
                      }
                    />
                  </label>
                  <label className="field-label">
                    Year
                    <input
                      type="number"
                      min="1000"
                      max="2100"
                      value={newEvidence.year}
                      onChange={(event) =>
                        setNewEvidence((current) => ({ ...current, year: event.target.value }))
                      }
                    />
                  </label>
                </div>
                <div className="field-grid">
                  <label className="field-label">
                    DOI
                    <input
                      value={newEvidence.doi}
                      onChange={(event) =>
                        setNewEvidence((current) => ({ ...current, doi: event.target.value }))
                      }
                      placeholder="10.xxxx/..."
                    />
                  </label>
                  <label className="field-label">
                    Review status
                    <select
                      value={newEvidence.status}
                      onChange={(event) =>
                        setNewEvidence((current) => ({
                          ...current,
                          status: event.target.value as typeof current.status,
                        }))
                      }
                    >
                      <option value="candidate">Candidate</option>
                      <option value="confirmed">User confirmed</option>
                      <option value="disputed">Disputed</option>
                    </select>
                  </label>
                </div>
                <div className="pdf-reference-row">
                  <button
                    className="secondary-button"
                    onClick={() => evidenceFileInput.current?.click()}
                    type="button"
                  >
                    <Upload size={14} />
                    Choose PDF reference
                  </button>
                  <span>{newEvidence.fileName || "No local PDF reference selected"}</span>
                </div>
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
                <div className="field-grid">
                  <label className="field-label">
                    Start offset
                    <input
                      type="number"
                      min="0"
                      value={newEvidence.startOffset}
                      onChange={(event) =>
                        setNewEvidence((current) => ({
                          ...current,
                          startOffset: event.target.value,
                        }))
                      }
                    />
                  </label>
                  <label className="field-label">
                    End offset
                    <input
                      type="number"
                      min="0"
                      value={newEvidence.endOffset}
                      onChange={(event) =>
                        setNewEvidence((current) => ({
                          ...current,
                          endOffset: event.target.value,
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
                  URL
                  <input
                    value={newEvidence.url}
                    onChange={(event) =>
                      setNewEvidence((current) => ({ ...current, url: event.target.value }))
                    }
                  />
                </label>
                <input
                  ref={evidenceFileInput}
                  className="hidden-input"
                  type="file"
                  accept="application/pdf,.pdf"
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) {
                      setNewEvidence((current) => ({
                        ...current,
                        fileName: file.name,
                      }));
                    }
                    event.target.value = "";
                  }}
                />
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

      {projectLibraryOpen && (
        <div className="modal-backdrop" onMouseDown={() => setProjectLibraryOpen(false)}>
          <div
            className="modal-card wide project-library"
            onMouseDown={stopEvent}
            role="dialog"
            aria-modal="true"
            data-testid="project-library"
          >
            <div className="modal-head">
              <div>
                <span className="eyebrow">Local workspace</span>
                <h2>Projects</h2>
                <p>Create, rename, or reopen a locally saved research graph.</p>
              </div>
              <button className="icon-button ghost" onClick={() => setProjectLibraryOpen(false)}>
                <X size={17} />
              </button>
            </div>
            <div className="project-library-current">
              <div>
                <span className="eyebrow">Current project</span>
                <strong>{project.title}</strong>
                <small>
                  {project.nodes.length} nodes · revision {project.revision}
                </small>
              </div>
              <label className="field-label">
                Project name
                <input
                  value={projectNameDraft}
                  onChange={(event) => setProjectNameDraft(event.target.value)}
                />
              </label>
              <button className="secondary-button" onClick={renameCurrentProject}>
                Rename
              </button>
              <button className="primary-button" onClick={startNewProject}>
                <Plus size={15} />
                New project
              </button>
            </div>
            <div className="recent-project-grid">
              {projectLibrary.map((entry) => (
                <button
                  key={entry.id}
                  className={entry.id === project.id ? "active" : ""}
                  onClick={() => openLibraryProject(entry)}
                >
                  <FileJson size={18} />
                  <span>
                    <strong>{entry.title}</strong>
                    <small>
                      {entry.nodeCount} nodes ·{" "}
                      {new Date(entry.updatedAt).toLocaleString(undefined, {
                        month: "short",
                        day: "numeric",
                        hour: "2-digit",
                        minute: "2-digit",
                      })}
                    </small>
                  </span>
                  {entry.id === project.id ? <Check size={15} /> : <ChevronRight size={15} />}
                </button>
              ))}
            </div>
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
                <span className="eyebrow">{t("plugins.eyebrow")}</span>
                <h2>{t("plugins.title")}</h2>
                <p>{t("plugins.subtitle")}</p>
              </div>
              <button className="icon-button ghost" onClick={() => setPluginStoreOpen(false)}>
                <X size={17} />
              </button>
            </div>
            <div
              className={`myc-drop-zone ${mycDropActive ? "active" : ""}`}
              onDragEnter={(event) => {
                event.preventDefault();
                setMycDropActive(true);
              }}
              onDragOver={(event) => {
                event.preventDefault();
                setMycDropActive(true);
              }}
              onDragLeave={(event) => {
                if (event.currentTarget === event.target) setMycDropActive(false);
              }}
              onDrop={(event) => {
                event.preventDefault();
                setMycDropActive(false);
                const paths = Array.from(event.dataTransfer.files)
                  .filter((file) => isMycFileName(file.name))
                  .map((file) => (file as File & { path?: string }).path)
                  .filter((path): path is string => Boolean(path));
                if (paths.length) void installMycPaths(paths);
                else setToast(t("plugins.desktopOnly"));
              }}
              data-testid="myc-drop-zone"
            >
              <PackageOpen size={24} />
              <div>
                <strong>{mycInstalling ? t("plugins.installing") : t("plugins.dropTitle")}</strong>
                <small>{t("plugins.dropHint")}</small>
              </div>
              <button
                className="secondary-button"
                onClick={() => mycPluginInput.current?.click()}
                disabled={mycInstalling}
              >
                {t("plugins.browse")}
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
              <div className="fixture-load-actions">
                <button className="primary-button" onClick={loadMnistStudy} data-testid="load-mnist">
                  {t("plugins.loadMnist")}
                </button>
                <button className="secondary-button" onClick={loadSocialScienceStudy}>
                  {t("plugins.loadSocial")}
                </button>
              </div>
            </div>
            <div className="plugin-grid">
              {pluginCatalog.map((plugin) => (
                <article className="plugin-card" key={plugin.id}>
                  <div className="plugin-card-head">
                    <span className={`plugin-status ${plugin.status}`}>
                      {plugin.status === "installed"
                        ? t("plugins.installed")
                        : plugin.status === "reserved"
                          ? t("plugins.reserved")
                          : t("plugins.available")}
                    </span>
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
                      {loadedPlugins.includes(plugin.id)
                        ? t("plugins.enabled")
                        : t("plugins.enable")}
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
            <div className="modal-head settings-modal-head">
              <div>
                <span className="eyebrow">{t("settings.eyebrow")}</span>
                <div className="settings-heading-row">
                  <h2>{t("settings.title")}</h2>
                  <span className="display-detected-badge">
                    {displayProfile.source === "tauri" ? "Windows display" : "Browser preview"}
                    {" · "}
                    {Math.round(displayProfile.scaleFactor * 100)}%
                  </span>
                </div>
                <p>{t("settings.subtitle")}</p>
              </div>
              <button
                className="icon-button ghost settings-close"
                onClick={() => setSettingsOpen(false)}
                aria-label="Close settings"
              >
                <X size={20} />
              </button>
            </div>
            <div className="settings-layout">
              <nav className="settings-nav" aria-label="Settings sections">
                {(
                  [
                    ["display", Gauge, t("settings.display"), t("settings.displayHint")],
                    ["canvas", SlidersHorizontal, t("settings.canvas"), t("settings.canvasHint")],
                    [
                      "integrations",
                      Settings2,
                      t("settings.integrations"),
                      t("settings.integrationsHint"),
                    ],
                    [
                      "shortcuts",
                      Binary,
                      t("settings.shortcuts"),
                      t("settings.shortcutsHint"),
                    ],
                  ] as const
                ).map(([id, Icon, label, hint]) => (
                  <button
                    key={id}
                    className={settingsSection === id ? "active" : ""}
                    onClick={() => setSettingsSection(id)}
                  >
                    <Icon size={18} />
                    <span>
                      <strong>{label}</strong>
                      <small>{hint}</small>
                    </span>
                    <ChevronRight size={15} />
                  </button>
                ))}
                <div className="settings-nav-note">
                  <span>{t("settings.localFirst")}</span>
                  {t("settings.localFirstHint")}
                </div>
              </nav>

              <div className="settings-content">
                {settingsSection === "display" && (
                  <>
                    <section className="settings-section settings-section-first">
                      <div className="settings-title">
                        <Gauge size={20} />
                        <div>
                          <strong>Display & readability</strong>
                          <small>
                            Tauri listens for Windows scale changes and updates the interface
                            automatically.
                          </small>
                        </div>
                      </div>

                      <div className="display-status-card">
                        <div className="display-status-icon">
                          <LayoutDashboard size={22} />
                        </div>
                        <div>
                          <span>Current display</span>
                          <strong>
                            {Math.round(displayProfile.scaleFactor * 100)}% Windows scaling
                            <small>≈ {displayProfile.dpi} DPI</small>
                          </strong>
                        </div>
                        <span className="auto-status">
                          <Check size={14} />
                          {displayProfile.source === "tauri" ? "Live detected" : "Preview detected"}
                        </span>
                      </div>

                      <div className="density-heading">
                        <div>
                          <strong>Interface size</strong>
                          <small>Effective scale: {Math.round(uiScale * 100)}%</small>
                        </div>
                        {displayDensity === "auto" && <span>Recommended</span>}
                      </div>
                      <div className="density-grid">
                        {displayDensityOptions.map((option) => (
                          <button
                            key={option.id}
                            className={displayDensity === option.id ? "active" : ""}
                            onClick={() => setDisplayDensity(option.id)}
                            aria-pressed={displayDensity === option.id}
                          >
                            <span className={`density-aa density-${option.id}`}>Aa</span>
                            <strong>{option.label}</strong>
                            <small>{option.description}</small>
                            {displayDensity === option.id && (
                              <span className="density-check">
                                <Check size={13} />
                              </span>
                            )}
                          </button>
                        ))}
                      </div>

                      <div className="readability-preview">
                        <div>
                          <span className="eyebrow">Live preview</span>
                          <strong>Evidence chain remains legible at a glance</strong>
                          <p>
                            Variable labels, experiment outcomes, and citations use a consistent
                            reading scale.
                          </p>
                        </div>
                        <span>Supports · Δ +1.84 pp</span>
                      </div>
                    </section>

                    <section className="settings-section">
                      <div className="settings-title">
                        <Languages size={20} />
                        <div>
                          <strong>{t("settings.language")}</strong>
                          <small>{t("settings.languageHint")}</small>
                        </div>
                      </div>
                      <div className="language-grid" role="radiogroup" aria-label={t("settings.language")}>
                        {(
                          [
                            ["en", "EN", t("settings.english")],
                            ["zh-CN", "中", t("settings.chinese")],
                          ] as const
                        ).map(([id, badge, label]) => (
                          <button
                            key={id}
                            className={locale === id ? "active" : ""}
                            onClick={() => {
                              setLocale(id);
                              setToast(translate(id, "locale.changed"));
                            }}
                            role="radio"
                            aria-checked={locale === id}
                          >
                            <span>{badge}</span>
                            <strong>{label}</strong>
                            {locale === id && <Check size={16} />}
                          </button>
                        ))}
                      </div>
                    </section>

                    <section className="settings-section">
                      <div className="settings-title">
                        <Palette size={20} />
                        <div>
                          <strong>Color theme</strong>
                          <small>VS Code-style manifests use stable semantic color tokens.</small>
                        </div>
                      </div>
                      <div className="theme-grid">
                        {themeCatalog.map((theme) => (
                          <button
                            key={theme.id}
                            className={themeId === theme.id ? "active" : ""}
                            onClick={() => setThemeId(theme.id)}
                          >
                            <span
                              className="theme-swatch"
                              style={{ background: theme.colors.canvas }}
                            >
                              <i style={{ background: theme.colors.accent }} />
                            </span>
                            <span>
                              <strong>{theme.name}</strong>
                              <small>
                                {theme.publisher}
                                {theme.source === "myc" ? " · .myc" : ""}
                              </small>
                            </span>
                            {themeId === theme.id && <Check size={16} />}
                          </button>
                        ))}
                      </div>
                    </section>
                  </>
                )}

                {settingsSection === "canvas" && (
                  <>
                    <section className="settings-section settings-section-first">
                      <div className="settings-title">
                        <LayoutDashboard size={20} />
                        <div>
                          <strong>{locale === "zh-CN" ? "节点块外观" : "Block appearance"}</strong>
                          <small>
                            {locale === "zh-CN"
                              ? "选择每个节点在画布中呈现的研究信息密度。"
                              : "Choose how much research context each node reveals on the canvas."}
                          </small>
                        </div>
                      </div>
                      <div className="renderer-option-grid block-style-grid">
                        {blockStyleOptions.map((option) => {
                          const Icon = option.icon;
                          return (
                            <button
                              key={option.id}
                              className={blockStyleId === option.id ? "active" : ""}
                              onClick={() => setBlockStyleId(option.id)}
                              aria-pressed={blockStyleId === option.id}
                              data-testid={`block-style-${option.id}`}
                            >
                              <span className={`renderer-option-icon preview-${option.id}`}>
                                <Icon size={20} />
                              </span>
                              <span>
                                <strong>
                                  {locale === "zh-CN" ? option.labelZh : option.label}
                                </strong>
                                <small>
                                  {locale === "zh-CN"
                                    ? option.descriptionZh
                                    : option.description}
                                </small>
                              </span>
                              {blockStyleId === option.id && <Check size={16} />}
                            </button>
                          );
                        })}
                      </div>
                    </section>

                    <section className="settings-section">
                      <div className="settings-title">
                        <GitBranch size={20} />
                        <div>
                          <strong>{locale === "zh-CN" ? "连接线样式" : "Connector style"}</strong>
                          <small>
                            {locale === "zh-CN"
                              ? "路由与语义笔触可由内置样式或 .myc 插件提供。"
                              : "Routing and semantic strokes are supplied by built-in or .myc styles."}
                          </small>
                        </div>
                      </div>
                      <div className="renderer-option-grid edge-style-grid">
                        {edgeStyleCatalog.map((edgeStyle) => (
                          <button
                            key={edgeStyle.id}
                            className={edgeStyleId === edgeStyle.id ? "active" : ""}
                            onClick={() => setEdgeStyleId(edgeStyle.id)}
                            aria-pressed={edgeStyleId === edgeStyle.id}
                            data-testid={`edge-style-${edgeStyle.id}`}
                          >
                            <span className={`renderer-option-icon route-${edgeStyle.routing}`}>
                              <GitBranch size={20} />
                            </span>
                            <span>
                              <strong>{edgeStyle.name}</strong>
                              <small>
                                {edgeStyle.routing === "orthogonal"
                                  ? locale === "zh-CN"
                                    ? "严格 90° 正交路由"
                                    : "Strict 90° routing"
                                  : edgeStyle.routing === "bezier"
                                    ? locale === "zh-CN"
                                      ? "贝塞尔曲线"
                                      : "Bezier curve"
                                    : edgeStyle.routing === "straight"
                                      ? locale === "zh-CN"
                                        ? "直线"
                                        : "Straight line"
                                      : edgeStyle.routing.replace("-", " ")}
                                {edgeStyle.source === "myc" ? " · .myc" : ""}
                              </small>
                            </span>
                            {edgeStyleId === edgeStyle.id && <Check size={16} />}
                          </button>
                        ))}
                      </div>
                      <div className="settings-tip connector-tip">
                        <GitBranch size={18} />
                        <div>
                          <strong>{activeEdgeStyle.name}</strong>
                          <p>
                            {locale === "zh-CN"
                              ? "支持、反驳、控制和测量关系会保留各自的语义线型。"
                              : `${activeEdgeStyle.description} Support, refutation, control, and measurement relations keep separate semantic strokes.`}
                          </p>
                        </div>
                      </div>
                    </section>

                    <section className="settings-section">
                      <div className="settings-title">
                        <SlidersHorizontal size={20} />
                        <div>
                          <strong>{locale === "zh-CN" ? "画布交互" : "Canvas interaction"}</strong>
                          <small>
                            {locale === "zh-CN"
                              ? "遵循 Windows 与 macOS 触控板的原生操作习惯。"
                              : "Native-feeling defaults for Windows and macOS trackpads."}
                          </small>
                        </div>
                      </div>
                      <label className="settings-toggle">
                        <span>
                          <strong>{locale === "zh-CN" ? "双指平移" : "Two-finger pan"}</strong>
                          <small>
                            {locale === "zh-CN"
                              ? "滚轮与双指手势平移画布，同时保留捏合缩放。"
                              : "Wheel and trackpad gestures pan; pinch zoom remains enabled."}
                          </small>
                        </span>
                        <input
                          type="checkbox"
                          checked={trackpadPan}
                          onChange={(event) => setTrackpadPan(event.target.checked)}
                        />
                        <i aria-hidden="true" />
                      </label>
                      <label className="settings-toggle">
                        <span>
                          <strong>
                            {locale === "zh-CN" ? "吸附到 16 px 网格" : "Snap to 16 px grid"}
                          </strong>
                          <small>
                            {locale === "zh-CN"
                              ? "只在提交节点位置时应用，不干扰拖动过程。"
                              : "Applied only while committing node placements."}
                          </small>
                        </span>
                        <input
                          type="checkbox"
                          checked={snapEnabled}
                          onChange={(event) => setSnapEnabled(event.target.checked)}
                        />
                        <i aria-hidden="true" />
                      </label>
                      <div className="settings-tip">
                        <Focus size={18} />
                        <div>
                          <strong>{locale === "zh-CN" ? "禅模式" : "Zen mode"}</strong>
                          <p>
                            {locale === "zh-CN"
                              ? "按 Z 隐藏侧栏，只保留研究画布。"
                              : "Press Z to hide panels and keep only the research canvas in view."}
                          </p>
                        </div>
                      </div>
                    </section>
                  </>
                )}

                {settingsSection === "integrations" && (
                  <section className="settings-section settings-section-first">
                    <div className="settings-title">
                      <Settings2 size={20} />
                      <div>
                        <strong>Reserved integrations</strong>
                        <small>Every connector will request an explicit, reviewable grant.</small>
                      </div>
                    </div>
                    <div className="integration-settings-grid">
                      {pluginCatalog
                        .filter((plugin) => plugin.status === "reserved")
                        .map((plugin) => (
                          <article key={plugin.id}>
                            <div>
                              <strong>{plugin.name}</strong>
                              <small>{plugin.category}</small>
                            </div>
                            <span>Not connected</span>
                            <p>{plugin.description}</p>
                          </article>
                        ))}
                    </div>
                  </section>
                )}

                {settingsSection === "shortcuts" && (
                  <section className="settings-section settings-section-first">
                    <div className="settings-title">
                      <Binary size={20} />
                      <div>
                        <strong>Keyboard shortcuts</strong>
                        <small>Fast navigation for graph editing and analysis.</small>
                      </div>
                    </div>
                    <div className="shortcut-settings-preview">
                      {[
                        ["Ctrl / Cmd + K", "Focus search"],
                        ["Ctrl / Cmd + ,", "Open settings"],
                        ["Ctrl / Cmd + Enter", "Run graph analysis"],
                        ["Ctrl / Cmd + C / V", "Copy or paste a node"],
                        ["Ctrl / Cmd + Z", "Undo last change"],
                        ["F", "Fit graph"],
                        ["Z", "Zen mode"],
                      ].map(([keys, action]) => (
                        <div key={keys}>
                          <kbd>{keys}</kbd>
                          <span>{action}</span>
                        </div>
                      ))}
                    </div>
                    <button
                      className="secondary-button settings-shortcut-button"
                      onClick={() => {
                        setSettingsOpen(false);
                        setShortcutsOpen(true);
                      }}
                    >
                      <Binary size={16} />
                      Open full shortcut reference
                    </button>
                  </section>
                )}
              </div>
            </div>
            <div className="modal-footer settings-footer">
              <button
                className="settings-reset"
                onClick={() => {
                  setDisplayDensity("auto");
                  setThemeId("research-light");
                  setBlockStyleId("signal-block");
                  setEdgeStyleId("research-orthogonal");
                  setLocale(normalizeLocale(navigator.language));
                  setTrackpadPan(true);
                  setSnapEnabled(true);
                  setToast("Display and canvas preferences restored.");
                }}
              >
                <RotateCcw size={16} />
                {t("settings.restore")}
              </button>
              <span>{t("settings.autoSaved")}</span>
              <button className="primary-button" onClick={() => setSettingsOpen(false)}>
                {t("settings.done")}
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
                ["Ctrl/Cmd C", "Copy selected node"],
                ["Ctrl/Cmd V", "Paste copied node"],
                ["Ctrl/Cmd D", "Duplicate selected node"],
                ["Ctrl/Cmd Z", "Undo"],
                ["Ctrl/Cmd Shift Z / Y", "Redo"],
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
