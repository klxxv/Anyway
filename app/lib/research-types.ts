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

export const EDGE_TYPES = [
  "causes",
  "correlates",
  "supports",
  "contradicts",
  "depends_on",
  "derived_from",
  "part_of",
  "controls",
  "mediates",
  "moderates",
  "uses",
  "measures",
] as const;

export type ResearchEdgeType = (typeof EDGE_TYPES)[number];
export type ReviewStatus = "draft" | "confirmed" | "disputed" | "deprecated";
export type EvidenceStatus = "candidate" | "confirmed" | "verified" | "disputed";

export interface Provenance {
  origin: "human" | "ai" | "import" | "python";
  actorId?: string;
  modelId?: string;
  promptVersion?: string;
  reviewedBy?: string;
  reviewedAt?: string;
  sourceRefs?: string[];
}

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

export interface TraversalRequest {
  startId: string;
  strategy: "bfs" | "dfs";
  direction: "in" | "out" | "both";
  maxDepth: number;
  edgeTypes?: ResearchEdgeType[];
  nodeTypes?: ResearchNodeType[];
  scenarioId?: string;
}

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

export interface ScenarioDiff {
  disabledNodeIds: string[];
  disabledEdgeIds: string[];
  lostReachableNodeIds: string[];
  retainedReachableNodeIds: string[];
  alternatePathNodeIds: string[];
}

export const LAYOUT_MODES = [
  "evidence-chain",
  "refutation-chain",
  "tree",
  "huffman",
  "table",
  "neural-network",
] as const;

export type LayoutMode = (typeof LAYOUT_MODES)[number];

export interface LayoutResult {
  mode: LayoutMode;
  positions: Record<string, { x: number; y: number }>;
  annotations: Record<string, string>;
  nodeIds: string[];
  edgeIds: string[];
}

export type LogicChainMode = "effective" | "evidence" | "refutation";

export interface LogicChainResult {
  mode: LogicChainMode;
  nodeIds: string[];
  edgeIds: string[];
  score: number;
  summary: string;
}

export interface InfluenceResult {
  targetId: string;
  scores: Record<string, number>;
  edgeContributions: Record<string, number>;
  strongestEdgeIds: string[];
  iterations: number;
}

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
}

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
