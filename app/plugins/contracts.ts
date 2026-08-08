import type {
  EdgeStyleContent,
  EdgeStyleManifest,
  PluginManifest,
  ThemeManifest,
} from "../lib/research-types";

/** 桌面安装器与前端共享的 `.myc` 清单版本 / Shared `.myc` manifest version for desktop installer and frontend. */
export const MYC_API_VERSION = "researchcanvas.dev/v1alpha1";
export const PLUGIN_CALL_API_VERSION = "researchcanvas.dev/plugin-call/v1alpha1";

export interface PluginReference {
  id: string;
  version: string;
  name: string;
}

export interface PluginCallEnvelope<TContext = unknown, TPayload = unknown> {
  apiVersion: typeof PLUGIN_CALL_API_VERSION;
  operation: string;
  context?: TContext;
  payload?: TPayload;
}

export type MycPluginKind =
  | "ThemePlugin"
  | "EdgeStylePlugin"
  | "WorkspacePlugin"
  | "LocalePlugin"
  | "SourcePlugin"
  | "ConnectorPlugin"
  | "AnalysisPlugin"
  | "AgentPlugin"
  | "ProviderPlugin";

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
  locales?: PluginLocaleContribution[];
  commands?: PluginCommandContribution[];
}

/** 语言包是声明式数据，只能覆盖已知宿主消息键 / Locale bundles are declarative overlays for known host keys. */
export interface PluginLocaleContribution {
  locale: string;
  name: string;
  path: string;
}

export type PluginCommandCategory = "export" | "folder" | "git" | "import" | "llm-provider";

/** 由宿主中介的命令元数据；执行时仍需复核命名能力 / Host-mediated metadata still requires its named capability. */
export interface PluginCommandContribution {
  id: string;
  label: string;
  description: string;
  category: PluginCommandCategory;
  capability: string;
  formats?: Array<"pdf" | "svg" | "png">;
}

export interface InstalledPluginLocale {
  locale: string;
  name: string;
  messages: Record<string, string>;
}

export interface WorkspacePluginDescriptor {
  schemaVersion: 1;
  mode: "export" | "folder" | "git";
  testFixture?: string;
  config?: Record<string, unknown>;
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

/** Provider 模型路由条目 / Provider model route entry. */
export interface ProviderRoutingEntry {
  model: string;
  thinking: boolean;
  thinkingLevel?: "low" | "medium" | "high";
  jsonOutput: boolean;
}

/** Provider 配置 / Provider configuration descriptor. */
export interface ProviderConfig {
  type: "openai-compatible";
  baseUrl: string;
  chatCompletionsPath: string;
  defaultRouting: {
    extraction: ProviderRoutingEntry;
    synthesis: ProviderRoutingEntry;
    recovery: ProviderRoutingEntry;
  };
  requiresApiKey: boolean;
  apiKeyLabel?: string;
  timeoutSecs?: number;
}

/** Provider 描述符 / Provider descriptor (provider.json). */
export interface ProviderDescriptor {
  schemaVersion: 1;
  provider: ProviderConfig;
}

/** Agent 描述符 / Agent descriptor (agent-manifest.json). */
export interface AgentPluginDescriptor {
  schemaVersion: 1;
  mode: "agent";
  agentType?: string;
  reviewGated: true;
  capabilities?: string[];
  securityBoundary?: Record<string, boolean>;
  pipeline?: Record<string, unknown>;
}

export interface InstalledMycPlugin {
  manifest: MycPluginManifest;
  installPath: string;
  theme?: ThemeManifest;
  edgeStyle?: EdgeStyleManifest;
  runtime?: MycPluginRuntime;
  locales?: InstalledPluginLocale[];
  workspace?: WorkspacePluginDescriptor;
  provider?: ProviderDescriptor;
  agent?: AgentPluginDescriptor;
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

export function pluginReference(plugin: InstalledMycPlugin): PluginReference {
  return {
    id: plugin.manifest.metadata.id,
    version: plugin.manifest.metadata.version,
    name: plugin.manifest.metadata.name,
  };
}

export type GraphPatchOperation =
  | {
      op: "add-node";
      node: {
        id: string;
        type: string;
        title: string;
        body?: string;
        tags?: string[];
        data?: Record<string, unknown>;
      };
    }
  | {
      op: "add-edge";
      edge: {
        id: string;
        source: string;
        target: string;
        type: string;
        note?: string;
        data?: Record<string, unknown>;
      };
    }
  | { op: "update-node"; nodeId: string; changes: Record<string, unknown> }
  | { op: "update-edge"; edgeId: string; changes: Record<string, unknown> };

/**
 * 可移植、需审阅的图谱同步协议 / Portable, review-gated graph synchronization contract.
 * Torch/ONNX/社区适配器返回此结构；宿主永不暴露可变项目仓库引用。
 */
export interface PluginGraphPatch {
  apiVersion: "researchcanvas.dev/graph-patch/v1alpha1";
  source: {
    pluginId: string;
    operation: string;
    externalId?: string;
  };
  title: string;
  summary: string;
  reviewRequired: true;
  operations: GraphPatchOperation[];
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
  if (!plugin.edgeStyle) return null;
  if (plugin.manifest.kind === "ThemePlugin") {
    const content = plugin.edgeStyle as unknown as EdgeStyleContent;
    if (!content.routing || !content.stroke) return null;
    return {
      ...content,
      id: plugin.manifest.metadata.id,
      name: plugin.manifest.metadata.name,
      publisher: plugin.manifest.metadata.publisher,
      version: plugin.manifest.metadata.version,
      description: plugin.manifest.metadata.description,
      developer: plugin.manifest.metadata.developer,
      source: "myc",
    };
  }
  if (plugin.manifest.kind !== "EdgeStylePlugin") return null;
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
