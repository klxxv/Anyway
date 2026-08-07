import type { EdgeStyleManifest } from "../lib/research-types";
import { builtInEdgeStyleCatalog } from "./catalog";
import type { InstalledMycPlugin } from "./contracts";
import { normalizeInstalledEdgeStyle } from "./contracts";

const fallbackEdgeStyle = builtInEdgeStyleCatalog.find(
  (style) => style.id === "research-orthogonal",
);

if (!fallbackEdgeStyle) {
  throw new Error("Built-in orthogonal edge style is missing");
}

/**
 * Resolves one canvas-wide connector policy. ThemePlugins with embedded
 * edgeStyle take precedence, then standalone EdgeStylePlugins, falling back to
 * the built-in orthogonal grid.
 *
 * 解析画布级连线策略；优先从当前主题读取边样式，其次独立 EdgeStylePlugin，
 * 最后回退内置正交路由。
 */
export function resolveEdgeStyle(
  activePlugins: readonly InstalledMycPlugin[],
): EdgeStyleManifest {
  // 1) ThemePlugin with embedded edgeStyle takes top priority.
  for (let index = activePlugins.length - 1; index >= 0; index -= 1) {
    const plugin = activePlugins[index];
    if (plugin.manifest.kind !== "ThemePlugin") continue;
    const edgeStyle = normalizeInstalledEdgeStyle(plugin);
    if (edgeStyle) return edgeStyle;
  }
  // 2) Standalone EdgeStylePlugin (deprecated but still supported).
  for (let index = activePlugins.length - 1; index >= 0; index -= 1) {
    const plugin = activePlugins[index];
    if (!plugin.manifest.spec.capabilities.includes("edge.style.register")) continue;
    const edgeStyle = normalizeInstalledEdgeStyle(plugin);
    if (edgeStyle) return edgeStyle;
  }
  return fallbackEdgeStyle!;
}
