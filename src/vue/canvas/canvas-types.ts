import type {
  EdgeStyleManifest,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNode,
  ResearchNodeType,
} from "../../../app/lib/research-types";
import type { DiffState } from "../../../app/lib/graph/canvas-diff";

export type LinkLegendFilter = "causal" | "control" | "derived" | "contradicts";

export type ContextMenuScope = "node" | "edge" | "canvas";

export type ContextMenuActionId =
  | "node.inspect"
  | "node.connect"
  | "node.duplicate"
  | "node.delete"
  | "edge.filter"
  | "edge.reverse"
  | "edge.delete"
  | "canvas.add"
  | "canvas.note"
  | "canvas.expandAll"
  | "canvas.collapseAll"
  | "canvas.layout"
  | "canvas.fit";

export type ContextMenuPreferences = Record<ContextMenuScope, ContextMenuActionId[]>;

export type WorkspaceContextMenuState = {
  scope: ContextMenuScope;
  targetId?: string;
  title?: string;
  screenX: number;
  screenY: number;
  flowX: number;
  flowY: number;
};

export type WorkspaceShortcuts = Record<string, string>;

export type ResolvedPluginContextMenuAction = {
  id: string;
  contributionId: string;
  scope: ContextMenuScope;
  label: string;
  icon?: string;
  plugin?: unknown;
  capability?: string;
};

export type RadialMenuPosition =
  | "north"
  | "north-east"
  | "east"
  | "south-east"
  | "south"
  | "south-west"
  | "west"
  | "north-west";

export type RadialMenuAction =
  | `create:${ResearchNodeType}`
  | "canvas:fit"
  | "canvas:default-layout";

export type RadialMenuItem = {
  id: string;
  position: RadialMenuPosition;
  action: RadialMenuAction;
};

export type RadialMenuPreferences = { items: RadialMenuItem[] };

export type PieMenuState = {
  screenX: number;
  screenY: number;
  flowX: number;
  flowY: number;
  gestureActive?: boolean;
};

export type CanvasTrackpadFrame = {
  phase: "start" | "update" | "end";
  frameId: number;
  contacts: Array<{ id: number; x: number; y: number }>;
  centerX: number;
  centerY: number;
  span: number;
  scale: number;
  panX: number;
  panY: number;
  deviceWidth: number;
  deviceHeight: number;
  cursorX: number;
  cursorY: number;
  heldMs: number;
  held: boolean;
};

export type CanvasTrackpadGesture = {
  frame: CanvasTrackpadFrame;
  viewport?: { x: number; y: number; zoom: number };
  radial?: PieMenuState;
};

export type WorkspaceNodeData = {
  record: ResearchNode;
  shape: "card" | "circle";
  expanded: boolean;
  onToggleExpanded?: (nodeId: string) => void;
  typeLabel?: string;
  highlighted?: boolean;
  diffState?: DiffState;
};

export type WorkspaceEdgeData = {
  record: ResearchEdge;
  label: string;
  edgeStyle: EdgeStyleManifest;
  labelOffsetX?: number;
  labelOffsetY?: number;
  dragPreview?: boolean;
  highlighted?: boolean;
  diffState?: DiffState;
};

export type ResearchGraphCanvasProps = {
  project: ProjectState;
  selectedNodeId: string;
  selectedEdgeId: string;
  addRequest: number;
  connectMode: boolean;
  connectType: ResearchEdgeType;
  inspectorOpen: boolean;
  linkFilter: LinkLegendFilter | null;
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
  diffOverlay?: import("../../../app/lib/graph/canvas-diff").DiffOverlayState | null;
  diffFocus?: { id: string; kind: "node" | "edge"; nonce: number } | null;
  /** Optional host translation bridge. The Vue worker has no dependency on the React provider. */
  translate?: (key: string) => string;
  /** Native frames can be forwarded by the platform/runtime layer without coupling this component to it. */
  trackpadFrame?: CanvasTrackpadFrame | null;
  /** A modal host surface is open; canvas gestures must not mutate the viewport. */
  canvasInputBlocked?: boolean;
  onLegendFilter?: (filter: LinkLegendFilter | null) => void;
  onSelectNode?: (nodeId: string) => void;
  onNodeDoubleClick?: (nodeId: string) => void;
  onSelectEdge?: (edgeId: string) => void;
  onMoveNode?: (nodeId: string, x: number, y: number) => void;
  onCreateEdge?: (source: string, target: string) => void;
  onRequestCreate?: (type: ResearchNodeType, x: number, y: number) => void;
  onRequestConnect?: (nodeId: string) => void;
  onDuplicateNode?: (nodeId: string) => void;
  onDeleteNode?: (nodeId: string) => void;
  onReverseEdge?: (edgeId: string) => void;
  onDeleteEdge?: (edgeId: string) => void;
  onApplyDefaultLayout?: () => void;
  onPluginContextMenuAction?: (
    action: ResolvedPluginContextMenuAction,
    context: WorkspaceContextMenuState,
  ) => void;
  onTrackpadFrame?: (frame: CanvasTrackpadFrame) => void;
  onTrackpadGesture?: (gesture: CanvasTrackpadGesture) => void;
  onRadialMenuOpen?: (menu: PieMenuState) => void;
  onRadialMenuChoose?: (item: RadialMenuItem, flowX: number, flowY: number) => void;
  onRadialMenuClose?: () => void;
};

export type ResearchGraphCanvasEmits = {
  (event: "legend-filter", filter: LinkLegendFilter | null): void;
  (event: "select-node", nodeId: string): void;
  (event: "node-double-click", nodeId: string): void;
  (event: "select-edge", edgeId: string): void;
  (event: "move-node", nodeId: string, x: number, y: number): void;
  (event: "create-edge", source: string, target: string): void;
  (event: "request-create", type: ResearchNodeType, x: number, y: number): void;
  (event: "request-connect", nodeId: string): void;
  (event: "duplicate-node", nodeId: string): void;
  (event: "delete-node", nodeId: string): void;
  (event: "reverse-edge", edgeId: string): void;
  (event: "delete-edge", edgeId: string): void;
  (event: "apply-default-layout"): void;
  (event: "plugin-context-menu-action", action: ResolvedPluginContextMenuAction, context: WorkspaceContextMenuState): void;
  (event: "trackpad-frame", frame: CanvasTrackpadFrame): void;
  (event: "trackpad-gesture", gesture: CanvasTrackpadGesture): void;
  (event: "radial-menu-open", menu: PieMenuState): void;
  (event: "radial-menu-choose", item: RadialMenuItem, flowX: number, flowY: number): void;
  (event: "radial-menu-close"): void;
};
