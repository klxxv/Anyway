import type { LayoutMode } from "../../lib/research-types";
import {
  defaultWorkspaceShortcuts,
  SHORTCUT_ACTIONS,
  type WorkspaceShortcuts,
} from "./workspace-shortcuts";

export type CommandDensity = "comfortable" | "compact";
export type HoverDelay = 80 | 180 | 320;

export type WorkspacePreferences = {
  commandDensity: CommandDensity;
  hoverDelay: HoverDelay;
  defaultLayout: LayoutMode;
  showMiniMap: boolean;
  showLinkCounts: boolean;
  shortcuts: WorkspaceShortcuts;
};

export const defaultWorkspacePreferences: WorkspacePreferences = {
  commandDensity: "comfortable",
  hoverDelay: 180,
  defaultLayout: "tree",
  showMiniMap: true,
  showLinkCounts: true,
  shortcuts: { ...defaultWorkspaceShortcuts },
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
