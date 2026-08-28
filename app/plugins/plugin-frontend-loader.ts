import type { InstalledMycPlugin } from "./contracts";
import { HostSdk } from "../platform/host-sdk";
import { createDefaultTauriHostSdkTransport } from "../platform/host-sdk-tauri";
import * as VueRuntime from "vue";
import type {
  PluginContext,
  PluginFrontendManifest,
  PluginFrontendModule,
  PluginFrontendPluginIdentity,
} from "./plugin-frontend-contract";

type CacheEntry = {
  readonly module: PluginFrontendModule;
  readonly url: string;
  readonly revoke?: () => void;
};

const moduleCache = new Map<string, Promise<CacheEntry>>();
const activationDisposers = new Map<string, () => void | Promise<void>>();
const activationCounts = new Map<string, number>();
const VUE_RUNTIME_GLOBAL = "__ANYWAY_PLUGIN_VUE3_RUNTIME_V1__";
let vueBridgeUrl: string | undefined;

export type VerifiedPluginFrontendEntry =
  | {
      readonly kind: "source";
      readonly source: string;
      readonly contentType?: "text/javascript" | "application/javascript";
      readonly revision?: string;
    }
  | {
      readonly kind: "asset-url";
      readonly url: string;
      readonly revision?: string;
    };

export type PluginFrontendResolver = (
  plugin: InstalledMycPlugin,
  frontend: PluginFrontendManifest,
  sdk: HostSdk,
) => Promise<VerifiedPluginFrontendEntry>;

export type PluginFrontendLoaderOptions = {
  readonly sdk?: HostSdk;
  readonly resolve?: PluginFrontendResolver;
};

let pluginFrontendResolverSdk: HostSdk | undefined;

function getPluginFrontendResolverSdk(): HostSdk {
  pluginFrontendResolverSdk ??= new HostSdk(createDefaultTauriHostSdkTransport());
  return pluginFrontendResolverSdk;
}

function pluginIdentity(plugin: InstalledMycPlugin): PluginFrontendPluginIdentity {
  return {
    id: plugin.manifest.metadata.id,
    version: plugin.manifest.metadata.version,
    name: plugin.manifest.metadata.name,
    installPath: plugin.installPath,
  };
}

export function getPluginFrontendManifest(plugin: InstalledMycPlugin): PluginFrontendManifest | undefined {
  return plugin.manifest.frontend ?? plugin.manifest.spec.frontend;
}

function cacheKey(plugin: InstalledMycPlugin): string {
  const identity = pluginIdentity(plugin);
  const frontend = getPluginFrontendManifest(plugin);
  return `${identity.id}@${identity.version}:${frontend?.entry ?? ""}`;
}

export function assertSupportedPluginFrontend(plugin: InstalledMycPlugin): PluginFrontendManifest {
  const frontend = getPluginFrontendManifest(plugin);
  if (!frontend) throw new Error("PLUGIN_FRONTEND_MISSING");
  if (frontend.mode !== "trusted-module" || frontend.framework !== "vue3" || frontend.apiVersion !== "1") {
    throw new Error("PLUGIN_FRONTEND_UNSUPPORTED");
  }
  if (/\.vue($|\?)/u.test(frontend.entry)) throw new Error("PLUGIN_FRONTEND_ENTRY_MUST_BE_BUILT_MODULE");
  if (!/\.(mjs|js)($|\?)/u.test(frontend.entry)) throw new Error("PLUGIN_FRONTEND_ENTRY_NOT_MODULE");
  if (/^(?:https?:|file:|blob:|data:)/u.test(frontend.entry)) throw new Error("PLUGIN_FRONTEND_ENTRY_MUST_BE_RELATIVE");
  return frontend;
}

async function defaultResolvePluginFrontendEntry(
  plugin: InstalledMycPlugin,
  frontend: PluginFrontendManifest,
  sdk: HostSdk,
): Promise<VerifiedPluginFrontendEntry> {
  return sdk.call<VerifiedPluginFrontendEntry>("plugin.frontend.resolve", {
    pluginId: plugin.manifest.metadata.id,
    pluginVersion: plugin.manifest.metadata.version,
    entry: frontend.entry,
    framework: frontend.framework,
    apiVersion: frontend.apiVersion,
  });
}

function sharedVueBridgeUrl(): string {
  if (vueBridgeUrl) return vueBridgeUrl;
  Object.defineProperty(globalThis, VUE_RUNTIME_GLOBAL, {
    configurable: false,
    enumerable: false,
    writable: false,
    value: VueRuntime,
  });
  const exports = Object.keys(VueRuntime)
    .filter((name) => name !== "default" && /^[A-Za-z_$][A-Za-z0-9_$]*$/u.test(name))
    .map((name) => `export const ${name}=runtime[${JSON.stringify(name)}];`)
    .join("\n");
  const source = [
    `const runtime=globalThis[${JSON.stringify(VUE_RUNTIME_GLOBAL)}];`,
    `if(!runtime)throw new Error("PLUGIN_VUE_RUNTIME_MISSING");`,
    exports,
    "export default runtime;",
  ].join("\n");
  vueBridgeUrl = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
  return vueBridgeUrl;
}

function bindSharedVueRuntime(source: string): string {
  const bridge = sharedVueBridgeUrl();
  return source
    .replace(/(\bfrom\s*)(["'])vue\2/gu, (_match, prefix: string) => `${prefix}${JSON.stringify(bridge)}`)
    .replace(/(\bimport\s*)(["'])vue\2/gu, (_match, prefix: string) => `${prefix}${JSON.stringify(bridge)}`);
}

function urlFromVerifiedEntry(entry: VerifiedPluginFrontendEntry): Pick<CacheEntry, "url" | "revoke"> {
  if (entry.kind === "asset-url") {
    if (!/^(?:asset:|blob:)/u.test(entry.url)) throw new Error("PLUGIN_FRONTEND_ASSET_URL_UNSUPPORTED");
    return { url: entry.url };
  }
  const blob = new Blob([bindSharedVueRuntime(entry.source)], { type: entry.contentType ?? "text/javascript" });
  const url = URL.createObjectURL(blob);
  return { url, revoke: () => URL.revokeObjectURL(url) };
}

async function importFrontendModule(plugin: InstalledMycPlugin, options: PluginFrontendLoaderOptions): Promise<CacheEntry> {
  const frontend = assertSupportedPluginFrontend(plugin);
  const verified = await (options.resolve ?? defaultResolvePluginFrontendEntry)(
    plugin,
    frontend,
    options.sdk ?? getPluginFrontendResolverSdk(),
  );
  const { url, revoke } = urlFromVerifiedEntry(verified);
  const imported = await import(/* @vite-ignore */ url);
  return { module: imported as PluginFrontendModule, url, revoke };
}

export async function loadPluginFrontendModule(
  plugin: InstalledMycPlugin,
  options: PluginFrontendLoaderOptions = {},
): Promise<PluginFrontendModule> {
  const key = cacheKey(plugin);
  const cached = moduleCache.get(key);
  const entry = cached ?? importFrontendModule(plugin, options);
  if (!cached) moduleCache.set(key, entry);
  return (await entry).module;
}

export async function activatePluginFrontendModule(
  plugin: InstalledMycPlugin,
  context: PluginContext,
): Promise<PluginFrontendModule> {
  const key = cacheKey(plugin);
  const module = await loadPluginFrontendModule(plugin, { sdk: getPluginFrontendResolverSdk() });
  const currentCount = activationCounts.get(key) ?? 0;
  if (currentCount === 0) {
    if (typeof module.activate === "function") {
      const disposer = await module.activate({ plugin: pluginIdentity(plugin), context });
      if (typeof disposer === "function") activationDisposers.set(key, disposer);
      else activationDisposers.set(key, () => undefined);
    } else {
      activationDisposers.set(key, () => undefined);
    }
  }
  activationCounts.set(key, currentCount + 1);
  return module;
}

export async function deactivatePluginFrontendModule(plugin: InstalledMycPlugin, context: PluginContext): Promise<void> {
  const key = cacheKey(plugin);
  const currentCount = activationCounts.get(key) ?? 0;
  if (currentCount > 1) {
    activationCounts.set(key, currentCount - 1);
    return;
  }
  activationCounts.delete(key);
  const module = await loadPluginFrontendModule(plugin, { sdk: getPluginFrontendResolverSdk() }).catch(() => undefined);
  const disposer = activationDisposers.get(key);
  activationDisposers.delete(key);
  await disposer?.();
  await module?.deactivate?.({ plugin: pluginIdentity(plugin), context });
}

export function invalidatePluginFrontendModule(plugin?: InstalledMycPlugin): void {
  if (!plugin) {
    for (const cached of moduleCache.values()) void cached.then((entry) => entry.revoke?.()).catch(() => undefined);
    moduleCache.clear();
    activationDisposers.clear();
    activationCounts.clear();
    return;
  }
  const key = cacheKey(plugin);
  void moduleCache.get(key)?.then((entry) => entry.revoke?.()).catch(() => undefined);
  moduleCache.delete(key);
  activationDisposers.delete(key);
  activationCounts.delete(key);
}
