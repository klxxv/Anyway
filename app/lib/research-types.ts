/**
 * Renderer-agnostic domain contract for persisted research projects, graph
 * algorithms, plugins, and fixtures. Keep UI-only fields out of this module.
 */
import type {
  PluginSettingDefinition,
  PluginUpdateInfo,
} from "../plugins/contracts";
/** 允许持久化的研究实体类别 / Persisted research entity categories. */
export const NODE_TYPES = [
  "question",
  "concept",
  "variable",
  "hypothesis",
  "method",
  "evidence",
  "paper",
  "dataset",
  "experiment",
  "result",
  "metric",
  "formula",
  "artifact",
  "note",
] as const;

export type ResearchNodeType = (typeof NODE_TYPES)[number];

/**
 * 允许持久化的关系语义，收敛为 5 个抽象算子（schema-v4 T/K/I/M/Q）。
 * Persisted relation semantics, constrained to the five abstract operators
 * (schema-v4 T/K/I/M/Q): Transform / Kernel / Intervention / Marginalization /
 * Quotient. Evidence support/refutation is no longer an edge — it lives on
 * `experiment.outcome` and `polarity`.
 */
export const EDGE_TYPES = ["T", "K", "I", "M", "Q"] as const;

export type ResearchEdgeType = (typeof EDGE_TYPES)[number];

/**
 * Legacy 12-type → 5-operator convergence (implementation-plan.md §2.3).
 * `supports`/`contradicts` fold into `K`; the refutation signal moves to
 * `polarity`/`experiment.outcome`. Used to migrate persisted projects and
 * incoming plugin patches.
 */
const LEGACY_EDGE_TO_OPERATOR: Record<string, ResearchEdgeType> = {
  causes: "K",
  correlates: "K",
  supports: "K",
  contradicts: "K",
  depends_on: "T",
  derived_from: "T",
  uses: "T",
  measures: "T",
  part_of: "M",
  controls: "K",
  mediates: "K",
  moderates: "K",
};

export function convergeLegacyEdgeType(type: string): ResearchEdgeType | null {
  if ((EDGE_TYPES as readonly string[]).includes(type)) return type as ResearchEdgeType;
  return LEGACY_EDGE_TO_OPERATOR[type] ?? null;
}

export type ReviewStatus = "draft" | "confirmed" | "disputed" | "deprecated";
export type EvidenceStatus = "candidate" | "confirmed" | "verified" | "disputed";

/**
 * 数据来源与审阅链路；它用于可追溯性而非权限授予。
 * Provenance and review trail; this records traceability, not authorization.
 */
export interface Provenance {
  origin: "human" | "ai" | "import" | "python";
  actorId?: string;
  modelId?: string;
  promptVersion?: string;
  reviewedBy?: string;
  reviewedAt?: string;
  sourceRefs?: string[];
}

/**
 * 研究图中的语义节点，布局信息刻意不放在这里。
 * Semantic graph node; canvas placement intentionally lives elsewhere.
 */
export interface ResearchNode {
  id: string;
  type: ResearchNodeType;
  title: string;
  body: string;
  tags: string[];
  status: ReviewStatus;
  evidenceIds: string[];
  data: Record<string, unknown>;
  provenance: Provenance;
  createdAt: string;
  updatedAt: string;
}

/**
 * 节点间的带证据关系；`directed` 决定图算法的可达性语义。
 * Evidence-backed relation; `directed` controls reachability semantics.
 */
export interface ResearchEdge {
  id: string;
  type: ResearchEdgeType;
  source: string;
  target: string;
  directed: boolean;
  polarity: "positive" | "negative" | "mixed" | "unknown";
  confidence?: number;
  conditions: string[];
  evidenceIds: string[];
  note?: string;
  experiment?: {
    id: string;
    label: string;
    metric: string;
    baseline?: number;
    value?: number;
    delta?: number;
    outcome: "baseline" | "supports" | "refutes" | "neutral";
    status: "planned" | "running" | "completed";
    commit?: string;
    durationSeconds?: number;
  };
  provenance: Provenance;
}

/**
 * 可定位的证据记录，可由节点和边复用。
 * Addressable evidence record shared by nodes and edges.
 */
export interface EvidenceRecord {
  id: string;
  sourceType: "paper" | "dataset" | "experiment" | "code" | "note";
  sourceId: string;
  title: string;
  authors?: string;
  year?: number;
  doi?: string;
  url?: string;
  locator: {
    fileName?: string;
    page?: number;
    section?: string;
    quote?: string;
    startOffset?: number;
    endOffset?: number;
  };
  status: EvidenceStatus;
  provenance: Provenance;
}

/**
 * 单个视图中的节点几何信息；与研究语义分离以支持多视图。
 * Per-view geometry, separated from semantics to support multiple views.
 */
export interface PlacementRecord {
  id: string;
  viewId: string;
  nodeId: string;
  x: number;
  y: number;
  width: number;
  height: number;
  collapsed?: boolean;
  pinned?: boolean;
}

/**
 * 非破坏性消融覆盖层，绝不修改基础项目数据。
 * Non-destructive ablation overlay; it never mutates base project data.
 */
export interface ScenarioRecord {
  id: string;
  name: string;
  disabledNodeIds: string[];
  disabledEdgeIds: string[];
  nodeOverrides: Record<string, Partial<ResearchNode>>;
  edgeOverrides: Record<string, Partial<ResearchEdge>>;
  parameters: Record<string, unknown>;
  hypothesis: string;
  expectedEffect: string;
  createdAt: string;
}

/**
 * 可序列化的项目聚合根，也是导入、导出与历史快照的单位。
 * Serializable project aggregate; the unit for import, export, and history.
 */
export interface ProjectState {
  schemaVersion: number;
  id: string;
  title: string;
  discipline: string;
  updatedAt: string;
  revision: number;
  nodes: ResearchNode[];
  edges: ResearchEdge[];
  evidence: EvidenceRecord[];
  placements: PlacementRecord[];
  scenarios: ScenarioRecord[];
  navigation?: {
    recentNodeIds: string[];
    pinnedNodeIds: string[];
  };
  activity: Array<{
    id: string;
    label: string;
    origin: Provenance["origin"];
    createdAt: string;
  }>;
}

/**
 * 需要人工接受或拒绝的候选变更，避免自动代理直接改图。
 * Human-reviewable proposed change; prevents agents from mutating the graph directly.
 */
export interface GraphSuggestion {
  id: string;
  kind: "node" | "edge";
  operation?: "add" | "update" | "delete";
  title: string;
  description: string;
  confidence: number;
  rationale: string;
  evidenceLabel: string;
  status: "proposed" | "accepted" | "rejected";
  node?: Omit<ResearchNode, "id" | "createdAt" | "updatedAt">;
  edge?: Omit<ResearchEdge, "id">;
}

/**
 * 遍历的过滤与深度约束；结果保持稳定排序以便复现。
 * Traversal filters and depth bound; results are stably ordered for reproducibility.
 */
export interface TraversalRequest {
  startId: string;
  strategy: "bfs" | "dfs";
  direction: "in" | "out" | "both";
  maxDepth: number;
  edgeTypes?: ResearchEdgeType[];
  nodeTypes?: ResearchNodeType[];
  scenarioId?: string;
}

/**
 * 供算法解释和画布高亮使用的遍历产物。
 * Traversal artifact used for algorithm explanation and canvas highlighting.
 */
export interface TraversalResult {
  strategy: "bfs" | "dfs";
  startId: string;
  order: string[];
  edgeIds: string[];
  depth: Record<string, number>;
  parent: Record<string, string | null>;
  treeEdgeIds: string[];
  crossEdgeIds: string[];
  backEdgeIds: string[];
  stoppedByDepth: string[];
  durationMs: number;
}

/** 基线与场景可达性差异 / Reachability delta between base and scenario. */
export interface ScenarioDiff {
  disabledNodeIds: string[];
  disabledEdgeIds: string[];
  lostReachableNodeIds: string[];
  retainedReachableNodeIds: string[];
  alternatePathNodeIds: string[];
}

/** 仅影响展示位置的布局模式 / Layout modes that affect presentation only. */
export const LAYOUT_MODES = [
  "evidence-chain",
  "refutation-chain",
  "tree",
  "huffman",
  "table",
  "neural-network",
] as const;

export type LayoutMode = (typeof LAYOUT_MODES)[number];

/** 布局计算的纯展示结果 / Pure presentation result from a layout calculation. */
export interface LayoutResult {
  mode: LayoutMode;
  positions: Record<string, { x: number; y: number }>;
  annotations: Record<string, string>;
  nodeIds: string[];
  edgeIds: string[];
}

export type LogicChainMode = "effective" | "evidence" | "refutation";

/** 供研究者审阅的有分数逻辑链 / Scored logic chain for researcher review. */
export interface LogicChainResult {
  mode: LogicChainMode;
  nodeIds: string[];
  edgeIds: string[];
  score: number;
  summary: string;
}

/** 迭代影响传播的可解释输出 / Explainable output from iterative influence propagation. */
export interface InfluenceResult {
  targetId: string;
  scores: Record<string, number>;
  edgeContributions: Record<string, number>;
  strongestEdgeIds: string[];
  iterations: number;
}

/**
 * UI 可展示的插件元数据，不等同于桌面端 `.myc` 安装清单。
 * UI-facing plugin metadata; distinct from the desktop `.myc` install manifest.
 */
export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  category: "source" | "connector" | "analysis" | "theme" | "style" | "agent";
  description: string;
  status: "installed" | "available" | "reserved";
  permissions: string[];
  capabilities: string[];
  publisher: string;
  developer?: string;
  homepage?: string;
  license?: string;
  update?: PluginUpdateInfo;
  settings?: PluginSettingDefinition[];
}

/** 声明式主题中可嵌入的视觉-only 边样式（不含 id/name 等元数据）。 */
export interface EdgeStyleContent {
  routing: EdgeRoutingMode;
  stroke: EdgeStrokeStyle & {
    color: string;
    width: number;
    selectedWidth: number;
    opacity: number;
    cornerRadius?: number;
    offset?: number;
  };
  relations?: Partial<Record<ResearchEdgeType, EdgeStrokeStyle>>;
  marker: {
    type: "arrow" | "closed-arrow" | "none";
    size: number;
  };
}

/** 声明式主题令牌，不携带可执行代码 / Declarative theme tokens with no executable code. */
export interface ThemeManifest {
  id: string;
  name: string;
  publisher: string;
  version?: string;
  description?: string;
  developer?: string;
  source?: "builtin" | "myc";
  colors: {
    app: string;
    panel: string;
    canvas: string;
    text: string;
    muted: string;
    accent: string;
    border: string;
  };
  /** 可选：主题内嵌的边样式（ThemePlugin 统一颜色+连线外观）。 */
  edgeStyle?: EdgeStyleContent;
  components?: {
    toast?: {
      background?: string;
      border?: string;
      text?: string;
      shadow?: string;
    };
    miniMap?: {
      background?: string;
      border?: string;
      mask?: string;
      selectedNode?: string;
      evidenceNode?: string;
      node?: string;
      relation?: string;
      showRelations?: boolean;
    };
    radialMenu?: {
      background?: string;
      border?: string;
      divider?: string;
      text?: string;
      active?: string;
      centerBackground?: string;
      centerText?: string;
      shadow?: string;
      activeShadow?: string;
    };
  };
}

export type BlockStyleId = "research-card" | "compact-block" | "signal-block";

export type EdgeRoutingMode = "bezier" | "smooth-step" | "orthogonal" | "straight";

export interface EdgeStrokeStyle {
  color?: string;
  width?: number;
  selectedWidth?: number;
  opacity?: number;
  dash?: number[];
}

/** 声明式连线外观与关系覆盖 / Declarative connector style and relation overrides. */
export interface EdgeStyleManifest {
  id: string;
  name: string;
  publisher: string;
  version?: string;
  description?: string;
  developer?: string;
  source?: "builtin" | "myc";
  routing: EdgeRoutingMode;
  stroke: EdgeStrokeStyle & {
    color: string;
    width: number;
    selectedWidth: number;
    opacity: number;
    cornerRadius?: number;
    offset?: number;
  };
  relations?: Partial<Record<ResearchEdgeType, EdgeStrokeStyle>>;
  marker: {
    type: "arrow" | "closed-arrow" | "none";
    size: number;
  };
}
