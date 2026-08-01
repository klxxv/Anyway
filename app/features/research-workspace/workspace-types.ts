import type { Edge, Node } from "@xyflow/react";
import type {
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
};
export type WorkspaceEdgeData = {
  record: ResearchEdge;
  label: string;
  labelOffsetX?: number;
  labelOffsetY?: number;
};

export type WorkspaceNode = Node<WorkspaceNodeData, "researchNode">;
export type WorkspaceEdge = Edge<WorkspaceEdgeData, "researchEdge">;

export type PieMenuState = {
  screenX: number;
  screenY: number;
  flowX: number;
  flowY: number;
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
