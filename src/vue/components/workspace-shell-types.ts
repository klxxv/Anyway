import type { Ref } from "vue";

import type {
  LayoutMode,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNode,
  ResearchNodeType,
} from "../../../app/lib/research-types";
import type {
  CanvasDiffResult,
  DiffOverlayState,
} from "../../../app/lib/graph/canvas-diff";
import type {
  FolderProjectSummary,
  GitHubAccountStatus,
  GitWorkspaceSnapshot,
} from "../../../app/platform/native-project";
import type { PluginGraphPatch } from "../../../app/plugins/contracts";
import type { ResolvedPluginContextMenuAction } from "../../../app/plugins/context-menu";
import type { EnabledWorkspaceCommand } from "../../../app/plugins/workspace";
import type {
  ContextMenuActionId,
  ContextMenuPreferences,
  WorkspaceContextMenuState,
} from "../../../app/features/research-workspace/workspace-context-menu";
import type {
  CachedRadialMenuItem,
  RadialMenuAction,
  RadialMenuCache,
  RadialMenuItem,
  RadialMenuPreferences,
} from "../../../app/features/research-workspace/workspace-radial-menu";
import type { LinkLegendFilter } from "../../../app/features/research-workspace/workspace-layout";
import type {
  CommandDensity,
  HoverDelay,
  WorkspacePreferences,
} from "../../../app/features/research-workspace/workspace-preferences";
import type { WorkspaceShortcuts } from "../../../app/features/research-workspace/workspace-shortcuts";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  NodeDraft,
  WorkspaceHistory,
} from "../../../app/features/research-workspace/workspace-types";
import type {
  GraphSelectionClipboard,
  GraphSelectionResult,
  NodeMove,
} from "../../../app/features/research-workspace/hooks/commit-logic";

export type {
  CachedRadialMenuItem,
  ContextMenuActionId,
  ContextMenuPreferences,
  DiffOverlayState,
  EdgeInspectorUpdate,
  FolderProjectSummary,
  GitHubAccountStatus,
  GitWorkspaceSnapshot,
  InspectorUpdate,
  LayoutMode,
  LinkLegendFilter,
  GraphSelectionClipboard,
  GraphSelectionResult,
  NodeMove,
  NodeDraft,
  PluginGraphPatch,
  ProjectState,
  RadialMenuAction,
  RadialMenuCache,
  RadialMenuItem,
  RadialMenuPreferences,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNode,
  ResearchNodeType,
  ResolvedPluginContextMenuAction,
  WorkspaceContextMenuState,
  WorkspaceHistory,
  WorkspacePreferences,
  WorkspaceShortcuts,
};

export type WorkspaceNotice = {
  id: number;
  text: string;
};

export type ComposerState = {
  type: ResearchNodeType;
  x: number;
  y: number;
};

export type DiffMode = "side-by-side" | "overlay";

export type DiffVersion = {
  id: string;
  label: string;
  project: ProjectState;
};

export type PieMenuState = {
  screenX: number;
  screenY: number;
  flowX: number;
  flowY: number;
  gestureActive?: boolean;
};

export type WorkspaceTopbarProps = {
  canUndo: boolean;
  canRedo: boolean;
  connectMode: boolean;
  connectType: ResearchEdgeType;
  commandDensity: CommandDensity;
  hoverDelay: HoverDelay;
  shortcuts: WorkspaceShortcuts;
  activeLayout: LayoutMode | null;
  exportFormats?: Array<"pdf" | "svg" | "png">;
  compareEnabled?: boolean;
};

export type WorkspaceTopbarEmits = {
  (event: "menu"): void;
  (event: "add"): void;
  (event: "add-type", type: ResearchNodeType): void;
  (event: "connect"): void;
  (event: "connect-type", type: ResearchEdgeType): void;
  (event: "note"): void;
  (event: "find"): void;
  (event: "layout", mode: LayoutMode): void;
  (event: "compare"): void;
  (event: "undo"): void;
  (event: "redo"): void;
  (event: "export"): void;
  (event: "export-format", format: "pdf" | "svg" | "png"): void;
};

export type WorkspaceContextMenuProps = {
  menu: WorkspaceContextMenuState;
  width: number;
  height: number;
  actionOrder: ContextMenuActionId[];
  shortcuts: WorkspaceShortcuts;
  pluginActions: ResolvedPluginContextMenuAction[];
};

export type WorkspaceContextMenuEmits = {
  (event: "built-in-action", action: ContextMenuActionId, menu: WorkspaceContextMenuState): void;
  (
    event: "plugin-action",
    action: ResolvedPluginContextMenuAction,
    menu: WorkspaceContextMenuState,
  ): void;
  (event: "close"): void;
};

export type RadialAddMenuProps = {
  menu: PieMenuState;
  cache: RadialMenuCache;
};

export type RadialAddMenuEmits = {
  (event: "choose", item: RadialMenuItem): void;
  (event: "close"): void;
};

export type WorkspaceProjectComposable = {
  project: Ref<ProjectState>;
  projectRef: Ref<ProjectState>;
  history: Ref<WorkspaceHistory[]>;
  selectedNode: Ref<ResearchNode | null>;
  selectedNodeId: Ref<string>;
  selectedEdge: Ref<ResearchEdge | null>;
  selectedEdgeId: Ref<string>;
  canUndo: Ref<boolean>;
  canRedo: Ref<boolean>;
  selectNode: (nodeId: string) => void;
  selectEdge: (edgeId: string) => void;
  clearSelection: () => void;
  updateNode: (nodeId: string, update: InspectorUpdate) => void;
  updateEdge: (edgeId: string, update: EdgeInspectorUpdate) => void;
  moveNode: (nodeId: string, x: number, y: number) => void;
  moveNodes: (moves: readonly NodeMove[]) => void;
  copySelection: (
    selectedNodeIds: readonly string[],
    selectedEdgeIds: readonly string[],
  ) => GraphSelectionClipboard | null;
  pasteSelection: () => GraphSelectionResult;
  createNode: (draft: NodeDraft, x: number, y: number) => string | undefined;
  createEdge: (source: string, target: string, type?: ResearchEdgeType) => string | undefined;
  removeNode: (nodeId: string) => void;
  removeSelection: (
    selectedNodeIds: readonly string[],
    selectedEdgeIds: readonly string[],
  ) => GraphSelectionResult;
  duplicateNode: (nodeId: string) => void;
  removeEdge: (edgeId: string) => void;
  reverseEdge: (edgeId: string) => void;
  applyLayout: (mode: LayoutMode, filter?: LinkLegendFilter | null) => void;
  undo: () => void;
  redo: () => void;
  resetDemo: () => void;
  replaceProject: (project: ProjectState, label?: string) => void;
  applyGraphPatch: (patch: PluginGraphPatch) => void;
};

export type WorkspaceDiffComposable = {
  result: Ref<CanvasDiffResult | null>;
  overlay: Ref<DiffOverlayState | null>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
};

export type FolderWorkspaceState = {
  root: string;
  projects: FolderProjectSummary[];
  command: import("../../../app/plugins/workspace").EnabledWorkspaceCommand;
};

export type GitWorkspaceState = {
  snapshot: GitWorkspaceSnapshot;
  account: GitHubAccountStatus | null;
  autoSave: boolean;
  busy: boolean;
  patch: PluginGraphPatch | null;
};

export type DiffFocus = {
  id: string;
  kind: "node" | "edge";
  nonce: number;
};

export type { CachedRadialMenuItem as WorkspaceCachedRadialMenuItem, EnabledWorkspaceCommand };
