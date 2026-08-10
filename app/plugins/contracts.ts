import type {
  EdgeStyleContent,
  EdgeStyleManifest,
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
  /** Stable publisher/developer UUID; optional for legacy manifests. */
  developerId?: string;
  description: string;
  homepage?: string;
  license?: string;
  update?: PluginUpdateInfo;
}

/** Declarative update metadata exposed by the plugin store. */
export interface PluginUpdateInfo {
  latestVersion?: string;
  url?: string;
  releaseNotes?: string;
}

export type PluginSettingType =
  | "boolean"
  | "number"
  | "text"
  | "select";

export interface PluginSettingOption {
  value: string;
  label: string;
}

/** A bounded, host-rendered setting; plugins never receive a React callback. */
export interface PluginSettingDefinition {
  id: string;
  label: string;
  type: PluginSettingType;
  /** Secret text is host-owned, write-only, and never exposed to plugins. */
  secret?: boolean;
  /** Whether the host must receive a value before enabling/executing the plugin. */
  required?: boolean;
  description?: string;
  /** Host-rendered input hint; never a secret value. */
  placeholder?: string;
  /** Stable UI grouping key, such as `model` or `advanced`. */
  group?: string;
  /** Derived UI hint; host treats `secret: true` as write-only. */
  writeOnly?: boolean;
  default?: boolean | number | string;
  min?: number;
  max?: number;
  step?: number;
  options?: PluginSettingOption[];
}

/** Agent model configuration is resolved by the host model gateway. */
export interface AgentModelConfiguration {
  ownership: "host-managed";
  invocation: "host-model-gateway";
  settingIds: string[];
  secretSettingIds: string[];
  agentReceives: string[];
  agentReceivesPlaintextSecrets: false;
  credentialPolicy?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isPluginSettingType(value: unknown): value is PluginSettingType {
  return (
    value === "boolean" ||
    value === "number" ||
    value === "text" ||
    value === "select"
  );
}

/**
 * Normalizes host-delivered manifest data for settings dialogs.
 * Invalid declarations are ignored; secret defaults are never forwarded.
 */
export function normalizePluginSettings(value: unknown): PluginSettingDefinition[] {
  if (!Array.isArray(value)) return [];

  return value.flatMap((candidate) => {
    if (!isRecord(candidate)) return [];
    const id = typeof candidate.id === "string" ? candidate.id.trim() : "";
    const label = typeof candidate.label === "string" ? candidate.label.trim() : "";
    const declaredType = candidate.type;
    // `type: secret` is accepted only as a legacy input spelling. The shared
    // contract stays canonical as `type: text, secret: true`, which lets the
    // native validator enforce one explicit secret boundary.
    const isSecret = candidate.secret === true || candidate.writeOnly === true || declaredType === "secret";
    const type = declaredType === "secret" ? "text" : declaredType;
    if (!id || !label || !isPluginSettingType(type)) return [];

    const setting: PluginSettingDefinition = { id, label, type };
    if (isSecret) {
      setting.secret = true;
      setting.writeOnly = true;
    }
    if (typeof candidate.required === "boolean") setting.required = candidate.required;
    if (typeof candidate.description === "string") setting.description = candidate.description;
    if (typeof candidate.placeholder === "string") setting.placeholder = candidate.placeholder;
    if (typeof candidate.group === "string") setting.group = candidate.group;

    if (!isSecret) {
      const defaultValue = candidate.default;
      if (
        typeof defaultValue === "boolean" ||
        typeof defaultValue === "number" ||
        typeof defaultValue === "string"
      ) {
        setting.default = defaultValue;
      }
    }
    if (typeof candidate.min === "number" && Number.isFinite(candidate.min)) {
      setting.min = candidate.min;
    }
    if (typeof candidate.max === "number" && Number.isFinite(candidate.max)) {
      setting.max = candidate.max;
    }
    if (
      typeof candidate.step === "number" &&
      Number.isFinite(candidate.step) &&
      candidate.step > 0
    ) {
      setting.step = candidate.step;
    }
    if (Array.isArray(candidate.options)) {
      const options = candidate.options.flatMap((option) => {
        if (!isRecord(option)) return [];
        if (typeof option.value !== "string" || typeof option.label !== "string") return [];
        return [{ value: option.value, label: option.label }];
      });
      if (options.length > 0) setting.options = options;
    }
    return [setting];
  });
}

export interface MycPluginSpec {
  engine: string;
  entry: string;
  language?: "rust" | "cpp" | "other";
  capabilities: string[];
  permissions: string[];
  contributes?: MycPluginContributions;
  settings?: PluginSettingDefinition[];
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
  /** 包内载荷文件的 sha256(构建期注入;签名经此覆盖全部载荷)/ Build-injected payload sha256 map; signatures cover payloads through it. */
  payloads?: Record<string, string>;
  /** 发布者签名(覆盖含 payloads 的清单 JSON)/ Publisher signature over the manifest JSON including payloads. */
  signature?: string;
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
  securityBoundary?: Record<string, boolean | string>;
  modelConfiguration?: AgentModelConfiguration;
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
    /** 可选：补丁生成时针对的项目 ID，用于防止打错内存中的图。 */
    projectId?: string;
  };
  title: string;
  summary: string;
  reviewRequired: true;
  operations: GraphPatchOperation[];
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
