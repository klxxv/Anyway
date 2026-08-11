import type { DiffState } from "../../lib/graph/canvas-diff";
import type {
  EdgeStyleManifest,
  ProjectState,
  ResearchEdge,
  ResearchNode,
  ResearchNodeType,
} from "../../lib/research-types";

/**
 * UI-only value schema. It intentionally lives outside the persisted domain types.
 * 仅供界面编辑的变量模式；刻意与持久化领域类型分离。
 */
export type VariableValueType = "enum" | "bool" | "number" | "text";

export type WorkspaceNodeData = {
  record: ResearchNode;
  shape: "card" | "circle";
  expanded: boolean;
  onToggleExpanded: (nodeId: string) => void;
  /** 逻辑链高亮（阶段 4） / Logic-chain highlight (phase 4). */
  highlighted?: boolean;
  /** Canvas Diff 叠加标记（added/removed/modified）；removed 为幽灵节点。 */
  diffState?: DiffState;
};
export type WorkspaceEdgeData = {
  record: ResearchEdge;
  label: string;
  edgeStyle: EdgeStyleManifest;
  labelOffsetX?: number;
  labelOffsetY?: number;
  /** Uses a cheap path and suppresses labels while an incident node is moving. */
  dragPreview?: boolean;
  /** 逻辑链高亮（阶段 4） / Logic-chain highlight (phase 4). */
  highlighted?: boolean;
  /** Canvas Diff 叠加标记（added/removed/modified）。 */
  diffState?: DiffState;
};

export type PieMenuState = {
  screenX: number;
  screenY: number;
  flowX: number;
  flowY: number;
  gestureActive?: boolean;
};

export type NodeDraft = {
  title: string;
  body: string;
  type: ResearchNodeType;
  tags: string[];
  data: Record<string, unknown>;
};

export type WorkspaceHistory = {
  project: ProjectState;
  label: string;
};

export type InspectorUpdate = Partial<
  Pick<ResearchNode, "title" | "body" | "status" | "tags" | "data">
>;

export type EdgeInspectorUpdate = Partial<
  Pick<
    ResearchEdge,
    "type" | "source" | "target" | "directed" | "polarity" | "confidence" | "conditions" | "note"
  >
>;
