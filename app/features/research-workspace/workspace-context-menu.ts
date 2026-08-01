import type { MessageKey } from "../../i18n/catalog";

/** 右键菜单的语义目标 / Semantic targets supported by the workspace context menu. */
export type ContextMenuScope = "node" | "edge" | "canvas";

export const CONTEXT_MENU_ACTIONS = {
  node: [
    { id: "node.inspect", labelKey: "contextMenu.inspect", icon: "inspect" },
    { id: "node.connect", labelKey: "contextMenu.connect", icon: "connect" },
    { id: "node.duplicate", labelKey: "contextMenu.duplicate", icon: "duplicate" },
    { id: "node.delete", labelKey: "contextMenu.deleteNode", icon: "delete", danger: true },
  ],
  edge: [
    { id: "edge.filter", labelKey: "contextMenu.filterRelation", icon: "filter" },
    { id: "edge.reverse", labelKey: "contextMenu.reverse", icon: "reverse" },
    { id: "edge.delete", labelKey: "contextMenu.deleteEdge", icon: "delete", danger: true },
  ],
  canvas: [
    { id: "canvas.add", labelKey: "contextMenu.quickAdd", icon: "add" },
    { id: "canvas.note", labelKey: "contextMenu.addNote", icon: "note" },
    { id: "canvas.layout", labelKey: "contextMenu.applyLayout", icon: "layout" },
    { id: "canvas.fit", labelKey: "contextMenu.fitView", icon: "fit" },
  ],
} as const satisfies Record<
  ContextMenuScope,
  ReadonlyArray<{
    id: string;
    labelKey: MessageKey;
    icon: string;
    danger?: boolean;
  }>
>;

export type ContextMenuActionId =
  (typeof CONTEXT_MENU_ACTIONS)[ContextMenuScope][number]["id"];

export type ContextMenuPreferences = Record<ContextMenuScope, ContextMenuActionId[]>;

export const defaultContextMenuPreferences: ContextMenuPreferences = {
  node: CONTEXT_MENU_ACTIONS.node.map((action) => action.id),
  edge: CONTEXT_MENU_ACTIONS.edge.map((action) => action.id),
  canvas: CONTEXT_MENU_ACTIONS.canvas.map((action) => action.id),
};

/** 过滤旧版本或损坏的本地设置 / Filters stale or malformed locally persisted menu actions. */
export function normalizeContextMenuPreferences(
  value?: Partial<Record<ContextMenuScope, unknown>> | null,
): ContextMenuPreferences {
  return (Object.keys(CONTEXT_MENU_ACTIONS) as ContextMenuScope[]).reduce(
    (result, scope) => {
      const allowed = new Set<string>(
        CONTEXT_MENU_ACTIONS[scope].map((action) => action.id),
      );
      const candidate = value?.[scope];
      result[scope] = Array.isArray(candidate)
        ? [...new Set(candidate.filter((id): id is ContextMenuActionId => typeof id === "string" && allowed.has(id)))]
        : [...defaultContextMenuPreferences[scope]];
      return result;
    },
    { node: [], edge: [], canvas: [] } as ContextMenuPreferences,
  );
}

export type WorkspaceContextMenuState = {
  scope: ContextMenuScope;
  targetId?: string;
  title?: string;
  screenX: number;
  screenY: number;
  flowX: number;
  flowY: number;
};
