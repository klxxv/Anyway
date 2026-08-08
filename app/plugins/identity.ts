import {
  MYC_API_VERSION,
  type InstalledMycPlugin,
  type MycPluginKind,
} from "./contracts";

export type PluginCompatibility = {
  compatible: boolean;
  issues: string[];
};

export function pluginKey(plugin: InstalledMycPlugin): string {
  return `${plugin.manifest.metadata.id}@${plugin.manifest.metadata.version}`;
}

function numericVersion(value: string): [number, number, number, string] {
  const [core, suffix = ""] = value.split("-", 2);
  const [major = 0, minor = 0, patch = 0] = core.split(".").map(Number);
  return [major || 0, minor || 0, patch || 0, suffix];
}

export function comparePluginVersions(left: string, right: string): number {
  const a = numericVersion(left);
  const b = numericVersion(right);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return (a[index] as number) - (b[index] as number);
  }
  if (a[3] === b[3]) return 0;
  if (!a[3]) return 1;
  if (!b[3]) return -1;
  return a[3].localeCompare(b[3]);
}

function expectsCapability(plugin: InstalledMycPlugin, capability: string, issues: string[]) {
  if (!plugin.manifest.spec.capabilities.includes(capability)) {
    issues.push(`Missing capability: ${capability}`);
  }
}

function validateWorkspacePlugin(plugin: InstalledMycPlugin, issues: string[]) {
  if (plugin.manifest.spec.engine !== "host-mediated") {
    issues.push("WorkspacePlugin engine must be host-mediated");
  }
  if (!plugin.workspace) issues.push("Workspace descriptor is missing");
  const commands = plugin.manifest.spec.contributes?.commands ?? [];
  if (commands.length === 0) issues.push("Workspace commands are missing");
  for (const command of commands) {
    expectsCapability(plugin, command.capability, issues);
    if (plugin.workspace && command.category !== plugin.workspace.mode) {
      issues.push(`Command ${command.id} does not match workspace mode ${plugin.workspace.mode}`);
    }
  }
}

/**
 * Frontend defense-in-depth for native-validated packages. This is also the
 * activation contract used by the registry and every contribution selector.
 */
export function pluginCompatibility(plugin: InstalledMycPlugin): PluginCompatibility {
  const issues: string[] = [];
  if (plugin.manifest.apiVersion !== MYC_API_VERSION) issues.push("Unsupported API version");
  if (plugin.manifest.spec.permissions.length > 0) issues.push("Ambient permissions are not allowed");
  const kind: MycPluginKind = plugin.manifest.kind;
  switch (kind) {
    case "ThemePlugin":
      expectsCapability(plugin, "theme.register", issues);
      if (plugin.manifest.spec.engine !== "declarative") {
        issues.push("ThemePlugin engine must be declarative");
      }
      if (!plugin.theme) issues.push("Theme descriptor is missing");
      break;
    case "EdgeStylePlugin":
      expectsCapability(plugin, "edge.style.register", issues);
      if (plugin.manifest.spec.engine !== "declarative") {
        issues.push("EdgeStylePlugin engine must be declarative");
      }
      if (!plugin.edgeStyle) issues.push("Edge style descriptor is missing");
      break;
    case "LocalePlugin":
      expectsCapability(plugin, "i18n.register", issues);
      if (plugin.manifest.spec.engine !== "declarative") {
        issues.push("LocalePlugin engine must be declarative");
      }
      if (!plugin.locales?.length) issues.push("Locale bundles are missing");
      break;
    case "AnalysisPlugin":
      expectsCapability(plugin, "analysis.run", issues);
      if (plugin.manifest.spec.engine !== "wasm32-myc") {
        issues.push("AnalysisPlugin engine must be wasm32-myc");
      }
      if (!plugin.runtime) issues.push("Verified WebAssembly runtime is missing");
      break;
    case "WorkspacePlugin":
      validateWorkspacePlugin(plugin, issues);
      break;
    case "ProviderPlugin":
      if (plugin.manifest.spec.engine !== "host-mediated") {
        issues.push("ProviderPlugin engine must be host-mediated");
      }
      if (
        !plugin.manifest.spec.capabilities.some(
          (capability) => capability === "llm.chat" || capability === "llm.configure",
        )
      ) {
        issues.push("Missing capability: llm.chat or llm.configure");
      }
      if (!plugin.provider) issues.push("Provider descriptor is missing");
      break;
    case "AgentPlugin":
      if (plugin.manifest.spec.engine !== "host-mediated") {
        issues.push("AgentPlugin engine must be host-mediated");
      }
      if (
        !plugin.manifest.spec.capabilities.some(
          (capability) => capability === "agent.graph.patch.propose",
        )
      ) {
        issues.push("Missing capability: agent.graph.patch.propose");
      }
      if (!plugin.agent) issues.push("Agent descriptor is missing");
      if (plugin.agent && plugin.agent.reviewGated !== true) {
        issues.push("Agent descriptor must be review-gated");
      }
      break;
    default:
      issues.push(`Unsupported plugin kind: ${kind}`);
  }
  return { compatible: issues.length === 0, issues };
}

export function latestCompatiblePlugins(
  plugins: readonly InstalledMycPlugin[],
): InstalledMycPlugin[] {
  const latest = new Map<string, InstalledMycPlugin>();
  for (const plugin of plugins) {
    if (!pluginCompatibility(plugin).compatible) continue;
    const id = plugin.manifest.metadata.id;
    const current = latest.get(id);
    if (
      !current ||
      comparePluginVersions(
        plugin.manifest.metadata.version,
        current.manifest.metadata.version,
      ) > 0
    ) {
      latest.set(id, plugin);
    }
  }
  return [...latest.values()].sort((left, right) =>
    left.manifest.metadata.id.localeCompare(right.manifest.metadata.id),
  );
}

/** Selects at most one enabled, compatible version for every plugin id. */
export function activePlugins(
  plugins: readonly InstalledMycPlugin[],
  enabledKeys: ReadonlySet<string>,
): InstalledMycPlugin[] {
  const enabled = plugins.filter((plugin) => enabledKeys.has(pluginKey(plugin)));
  return latestCompatiblePlugins(enabled);
}

export function enableLatestPluginKeys(
  plugins: readonly InstalledMycPlugin[],
): Set<string> {
  return new Set(latestCompatiblePlugins(plugins).map(pluginKey));
}

/**
 * Preserves the user's enabled plugin ids while moving each id to its newest
 * compatible installed version. Removed and incompatible packages drop out.
 */
export function migrateEnabledPluginKeys(
  plugins: readonly InstalledMycPlugin[],
  storedKeys: ReadonlySet<string>,
): Set<string> {
  const enabledIds = new Set(
    plugins
      .filter((plugin) => storedKeys.has(pluginKey(plugin)))
      .map((plugin) => plugin.manifest.metadata.id),
  );
  return new Set(
    latestCompatiblePlugins(plugins)
      .filter((plugin) => enabledIds.has(plugin.manifest.metadata.id))
      .map(pluginKey),
  );
}

export function updateEnabledPluginKeys(
  plugins: readonly InstalledMycPlugin[],
  current: ReadonlySet<string>,
  target: InstalledMycPlugin,
  enabled: boolean,
): Set<string> {
  const next = new Set(current);
  const targetId = target.manifest.metadata.id;
  for (const plugin of plugins) {
    if (plugin.manifest.metadata.id === targetId) next.delete(pluginKey(plugin));
  }
  if (enabled && pluginCompatibility(target).compatible) next.add(pluginKey(target));
  return next;
}
