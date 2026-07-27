import type {
  EdgeStyleManifest,
  PluginManifest,
  ThemeManifest,
} from "../lib/research-types";

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
  capabilities: string[];
  permissions: string[];
}

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
}

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

export function isMycFileName(name: string): boolean {
  return name.toLowerCase().endsWith(".myc");
}

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
