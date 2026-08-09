import type { ContextMenuScope } from "../features/research-workspace/workspace-context-menu";
import type {
  InstalledMycPlugin,
  PluginContextMenuIcon,
  PluginReference,
} from "./contracts";
import { pluginReference } from "./contracts";

export const enabledPluginsStorageKey = "research-canvas.enabled-plugins.v1";
export const pluginsChangedEvent = "research-canvas.plugins-changed";

export type ResolvedPluginContextMenuAction = {
  id: string;
  contributionId: string;
  scope: ContextMenuScope;
  label: string;
  icon: PluginContextMenuIcon;
  plugin: PluginReference;
  capability: string;
};

/**
 * Resolves executable contributions from the host-arbitrated active snapshot.
 * 仅解析可执行、已启用且显式声明对应能力的菜单贡献。
 */
export function contextMenuContributionsFromPlugins(
  plugins: InstalledMycPlugin[],
): ResolvedPluginContextMenuAction[] {
  return plugins.flatMap((plugin) => {
    const { manifest } = plugin;
    if (
      !plugin.runtime ||
      !manifest.spec.capabilities.includes("context-menu.contribute")
    ) {
      return [];
    }
    return (manifest.spec.contributes?.contextMenus ?? [])
      .filter(
        (item) =>
          /^[a-z0-9][a-z0-9._-]{0,63}$/i.test(item.id) &&
          ["node", "edge", "canvas"].includes(item.scope) &&
          item.label.trim().length > 0 &&
          item.label.length <= 64,
      )
      .map((item) => ({
        id: `${manifest.metadata.id}:${item.id}`,
        contributionId: item.id,
        scope: item.scope,
        label: item.label.trim(),
        icon: item.icon ?? "sparkles",
        plugin: pluginReference(plugin),
        capability: "context-menu.contribute",
      }));
  });
}

export function readEnabledPluginKeys(): Set<string> {
  if (typeof window === "undefined") return new Set();
  try {
    const value = JSON.parse(window.localStorage.getItem(enabledPluginsStorageKey) ?? "[]");
    return new Set(Array.isArray(value) ? value.filter((item) => typeof item === "string") : []);
  } catch {
    window.localStorage.removeItem(enabledPluginsStorageKey);
    return new Set();
  }
}
