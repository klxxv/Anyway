import type {
  EdgeStyleManifest,
  PluginManifest,
  ThemeManifest,
} from "../lib/research-types";

/** 桌面安装器与前端共享的 `.myc` 清单版本 / Shared `.myc` manifest version for desktop installer and frontend. */
export const MYC_API_VERSION = "researchcanvas.dev/v1alpha1";

export type MycPluginKind =
  | "ThemePlugin"
  | "EdgeStylePlugin"
  | "SourcePlugin"
  | "ConnectorPlugin"
  | "AnalysisPlugin"
  | "AgentPlugin";

export interface MycPluginMetadata {
  id: string;
  name: string;
  version: string;
  publisher: string;
  developer: string;
  description: string;
  homepage?: string;
  license?: string;
}

export interface MycPluginSpec {
  engine: string;
  entry: string;
  language?: "rust" | "cpp" | "other";
  capabilities: string[];
  permissions: string[];
  contributes?: MycPluginContributions;
}

export type PluginContextMenuIcon =
  | "sparkles"
  | "search"
  | "wand"
  | "database"
  | "link";

export interface PluginContextMenuContribution {
  id: string;
  scope: "node" | "edge" | "canvas";
  label: string;
  icon?: PluginContextMenuIcon;
}

/** 插件只能声明菜单与自己的沙箱命令，不能直接持有 UI 回调 / Plugins declare sandbox commands, never UI callbacks. */
export interface MycPluginContributions {
  contextMenus?: PluginContextMenuContribution[];
}

/**
 * 可安装包的最小声明式清单；可执行插件仍须经过更严格的权限模型。
 * Minimal declarative install manifest; executable plugins require a stricter permission model.
 */
export interface MycPluginManifest {
  apiVersion: typeof MYC_API_VERSION;
  kind: MycPluginKind;
  metadata: MycPluginMetadata;
  spec: MycPluginSpec;
}

export interface InstalledMycPlugin {
  manifest: MycPluginManifest;
  installPath: string;
  theme?: ThemeManifest;
  edgeStyle?: EdgeStyleManifest;
  runtime?: MycPluginRuntime;
}

export interface MycPluginRuntime {
  engine: "wasm32-myc";
  language: "rust" | "cpp" | "other";
  entrySha256: string;
}

export interface PluginExecutionResult {
  pluginId: string;
  pluginVersion: string;
  output: unknown;
  fuelConsumed: number;
  durationMs: number;
}

/**
 * 运行时向插件暴露的窄能力面，禁止直接访问应用状态。
 * Narrow runtime capability surface; plugins cannot access application state directly.
 */
export interface PluginContext {
  readonly projectId: string;
  readonly locale: string;
  readonly capabilities: ReadonlySet<string>;
  registerTheme(theme: ThemeManifest): void;
  registerEdgeStyle(edgeStyle: EdgeStyleManifest): void;
  notify(message: string): void;
}

/**
 * A deliberately small, Pythonic lifecycle: one object, explicit capabilities,
 * setup returns nothing, and teardown is optional. Plugins receive a narrow
 * context instead of importing application stores.
 */
export interface ResearchCanvasPlugin<TConfig = unknown> {
  readonly manifest: PluginManifest;
  setup(context: PluginContext, config?: TConfig): void | Promise<void>;
  teardown?(): void | Promise<void>;
}

/** 宽松的文件名预过滤，安全验证仍在 Rust 安装器完成 / Lenient filename prefilter; Rust installer performs security validation. */
export function isMycFileName(name: string): boolean {
  return name.toLowerCase().endsWith(".myc");
}

/** 将经桌面端验证的主题包适配为 UI 主题 / Adapts a desktop-validated theme package into a UI theme. */
export function normalizeInstalledTheme(plugin: InstalledMycPlugin): ThemeManifest | null {
  if (!plugin.theme || plugin.manifest.kind !== "ThemePlugin") return null;
  return {
    ...plugin.theme,
    id: plugin.manifest.metadata.id,
    name: plugin.manifest.metadata.name,
    publisher: plugin.manifest.metadata.publisher,
    version: plugin.manifest.metadata.version,
    description: plugin.manifest.metadata.description,
    developer: plugin.manifest.metadata.developer,
    source: "myc",
  };
}

/** 将经桌面端验证的连线样式包适配为 UI 样式 / Adapts a desktop-validated edge-style package into a UI style. */
export function normalizeInstalledEdgeStyle(
  plugin: InstalledMycPlugin,
): EdgeStyleManifest | null {
  if (!plugin.edgeStyle || plugin.manifest.kind !== "EdgeStylePlugin") return null;
  return {
    ...plugin.edgeStyle,
    id: plugin.manifest.metadata.id,
    name: plugin.manifest.metadata.name,
    publisher: plugin.manifest.metadata.publisher,
    version: plugin.manifest.metadata.version,
    description: plugin.manifest.metadata.description,
    developer: plugin.manifest.metadata.developer,
    source: "myc",
  };
}
