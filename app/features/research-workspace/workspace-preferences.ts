import type { LayoutMode } from "../../lib/research-types";
import {
  defaultWorkspaceShortcuts,
  SHORTCUT_ACTIONS,
  type WorkspaceShortcuts,
} from "./workspace-shortcuts";
import {
  defaultContextMenuPreferences,
  normalizeContextMenuPreferences,
  type ContextMenuPreferences,
} from "./workspace-context-menu";
import {
  defaultRadialMenuPreferences,
  normalizeRadialMenuPreferences,
  type RadialMenuPreferences,
} from "./workspace-radial-menu";

export type CommandDensity = "comfortable" | "compact";
export type HoverDelay = 80 | 180 | 320;

export type WorkspacePreferences = {
  commandDensity: CommandDensity;
  hoverDelay: HoverDelay;
  trackpadSensitivity: number;
  trackpadFilterStrength: number;
  defaultLayout: LayoutMode;
  showMiniMap: boolean;
  showLinkCounts: boolean;
  contextMenus: ContextMenuPreferences;
  showPluginContextMenuActions: boolean;
  shortcuts: WorkspaceShortcuts;
  radialMenu: RadialMenuPreferences;
};

export const defaultWorkspacePreferences: WorkspacePreferences = {
  commandDensity: "comfortable",
  hoverDelay: 180,
  trackpadSensitivity: 1,
  trackpadFilterStrength: 0.55,
  defaultLayout: "tree",
  showMiniMap: true,
  showLinkCounts: true,
  contextMenus: {
    node: [...defaultContextMenuPreferences.node],
    edge: [...defaultContextMenuPreferences.edge],
    canvas: [...defaultContextMenuPreferences.canvas],
  },
  showPluginContextMenuActions: true,
  shortcuts: { ...defaultWorkspaceShortcuts },
  radialMenu: {
    items: defaultRadialMenuPreferences.items.map((item) => ({ ...item })),
  },
};

/**
 * Accept only known preference values before restoring local settings.
 * 恢复本地设置前仅接受已知值，避免旧版本数据破坏界面。
 */
export function normalizeWorkspacePreferences(
  value: Partial<WorkspacePreferences> | null,
): WorkspacePreferences {
  if (!value) return defaultWorkspacePreferences;
  return {
    commandDensity:
      value.commandDensity === "compact" ? "compact" : "comfortable",
    hoverDelay:
      value.hoverDelay === 80 || value.hoverDelay === 320 ? value.hoverDelay : 180,
    trackpadSensitivity: Number.isFinite(value.trackpadSensitivity)
      ? Math.min(2, Math.max(0.5, value.trackpadSensitivity as number))
      : 1,
    trackpadFilterStrength: Number.isFinite(value.trackpadFilterStrength)
      ? Math.min(0.9, Math.max(0, value.trackpadFilterStrength as number))
      : 0.55,
    defaultLayout:
      value.defaultLayout === "evidence-chain" ||
      value.defaultLayout === "refutation-chain" ||
      value.defaultLayout === "tree" ||
      value.defaultLayout === "huffman" ||
      value.defaultLayout === "table" ||
      value.defaultLayout === "neural-network"
        ? value.defaultLayout
        : "tree",
    showMiniMap: value.showMiniMap !== false,
    showLinkCounts: value.showLinkCounts !== false,
    contextMenus: normalizeContextMenuPreferences(value.contextMenus),
    showPluginContextMenuActions: value.showPluginContextMenuActions !== false,
    radialMenu: normalizeRadialMenuPreferences(value.radialMenu),
    shortcuts: Object.fromEntries(
      SHORTCUT_ACTIONS.map((action) => [
        action,
        typeof value.shortcuts?.[action] === "string"
          ? value.shortcuts[action]
          : defaultWorkspaceShortcuts[action],
      ]),
    ) as WorkspaceShortcuts,
  };
}
