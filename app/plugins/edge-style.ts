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
 * Resolves one canvas-wide connector policy. Enabled edge-style plugins can
 * replace the built-in rounded orthogonal route without coupling React Flow to
 * the package loader.
 *
 * 解析画布级连线策略；已启用的连线插件可替换内置圆角正交路由，React Flow
 * 无需依赖插件安装器。
 */
export function resolveEdgeStyle(
  activePlugins: readonly InstalledMycPlugin[],
): EdgeStyleManifest {
  for (let index = activePlugins.length - 1; index >= 0; index -= 1) {
    const plugin = activePlugins[index];
    if (!plugin.manifest.spec.capabilities.includes("edge.style.register")) continue;
    const edgeStyle = normalizeInstalledEdgeStyle(plugin);
    if (edgeStyle) return edgeStyle;
  }
  return fallbackEdgeStyle!;
}
