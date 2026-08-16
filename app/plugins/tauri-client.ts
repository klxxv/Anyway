import type {
  InstalledMycPlugin,
  PluginCallEnvelope,
  PluginExecutionResult,
  PluginReference,
} from "./contracts";
import type { NativeVsixImportReport } from "./vsix-contracts";
import { PLUGIN_CALL_API_VERSION } from "./contracts";
import { HostSdk } from "../platform/host-sdk";
import { createDefaultTauriHostSdkTransport } from "../platform/host-sdk-tauri";

export type PluginSettingsSnapshot = {
  pluginId: string;
  pluginVersion: string;
  values: Record<string, unknown>;
  /** Secret values are never returned; only their configured state is exposed. */
  configuredSecrets: Record<string, boolean>;
};

export type PluginSecretMutation =
  | { action: "keep" }
  | { action: "clear" }
  | { action: "set"; value: string };

export type PluginSettingsWrite = {
  values: Record<string, unknown>;
  secrets: Record<string, PluginSecretMutation>;
};

export type PluginConnectionTestResult = {
  ok: boolean;
  /** Stable host code; plugin UI can localize this without parsing message text. */
  code: string;
  message: string;
};

type PluginSettingDefinitionLike = {
  id?: unknown;
  type?: unknown;
  secret?: unknown;
  writeOnly?: unknown;
  default?: unknown;
  min?: unknown;
  max?: unknown;
  step?: unknown;
  options?: unknown;
};

/** Accept the canonical `text + secret: true` shape and a legacy `secret` type. */
function isSecretDefinition(definition: PluginSettingDefinitionLike): boolean {
  return definition.type === "secret" || definition.secret === true || definition.writeOnly === true;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function cloneSettingValue(value: unknown): unknown {
  if (Array.isArray(value)) return [...value];
  if (isRecord(value)) return { ...value };
  return value;
}

function fallbackValue(definition: PluginSettingDefinitionLike): unknown {
  const type = definition.type;
  if (isSecretDefinition(definition)) return undefined;
  if (definition.default !== undefined) return cloneSettingValue(definition.default);
  if (type === "boolean") return false;
  if (type === "number") {
    const options = isRecord(definition) ? definition : {};
    const min = typeof options.min === "number" ? options.min : undefined;
    return min ?? 0;
  }
  if (type === "select" && Array.isArray(definition.options)) {
    const first = definition.options.find((option) => isRecord(option) && typeof option.value === "string");
    return isRecord(first) ? first.value : "";
  }
  return "";
}

function browserSettingsKey(plugin: PluginReference): string {
  return `research-canvas.plugin-settings.v1:${encodeURIComponent(plugin.id)}@${encodeURIComponent(plugin.version)}`;
}

function fallbackSnapshot(
  plugin: PluginReference,
  definitions: readonly PluginSettingDefinitionLike[],
): PluginSettingsSnapshot {
  const values: Record<string, unknown> = {};
  const configuredSecrets: Record<string, boolean> = {};
  for (const definition of definitions) {
    if (typeof definition.id !== "string" || definition.id.length === 0) continue;
    const value = fallbackValue(definition);
    if (isSecretDefinition(definition)) configuredSecrets[definition.id] = false;
    else values[definition.id] = value;
  }
  return { pluginId: plugin.id, pluginVersion: plugin.version, values, configuredSecrets };
}

function readBrowserSnapshot(
  plugin: PluginReference,
  definitions: readonly PluginSettingDefinitionLike[],
): PluginSettingsSnapshot {
  const defaults = fallbackSnapshot(plugin, definitions);
  if (typeof window === "undefined") return defaults;
  try {
    const raw = JSON.parse(window.localStorage.getItem(browserSettingsKey(plugin)) ?? "null");
    if (!isRecord(raw)) return defaults;
    return normalizePluginSettingsSnapshot(raw, plugin, definitions, defaults);
  } catch {
    return defaults;
  }
}

function writeBrowserSnapshot(snapshot: PluginSettingsSnapshot): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(
      browserSettingsKey({ id: snapshot.pluginId, version: snapshot.pluginVersion, name: snapshot.pluginId }),
      JSON.stringify({ values: snapshot.values, configuredSecrets: snapshot.configuredSecrets }),
    );
  } catch {
    // Browser storage is optional; the in-memory Pinia snapshot remains usable.
  }
}

/** Normalizes both the native snapshot and the browser fallback shape. */
export function normalizePluginSettingsSnapshot(
  raw: unknown,
  plugin: PluginReference,
  definitions: readonly PluginSettingDefinitionLike[],
  defaults = fallbackSnapshot(plugin, definitions),
): PluginSettingsSnapshot {
  const source = isRecord(raw) && isRecord(raw.settings) ? raw.settings : raw;
  const valuesSource = isRecord(source) && isRecord(source.values)
    ? source.values
    : isRecord(source) && isRecord(source.effectiveValues)
      ? source.effectiveValues
    : isRecord(source)
      ? source
      : {};
  const configuredSource = isRecord(source) && isRecord(source.configuredSecrets)
    ? source.configuredSecrets
    : isRecord(source) && isRecord(source.secretConfigured)
      ? source.secretConfigured
      : {};
  const values: Record<string, unknown> = { ...defaults.values };
  const configuredSecrets: Record<string, boolean> = { ...defaults.configuredSecrets };
  for (const definition of definitions) {
    if (typeof definition.id !== "string" || definition.id.length === 0) continue;
    if (isSecretDefinition(definition)) {
      const configured = configuredSource[definition.id];
      configuredSecrets[definition.id] = configured === true || (isRecord(configured) && configured.configured === true);
      delete values[definition.id];
      continue;
    }
    if (Object.prototype.hasOwnProperty.call(valuesSource, definition.id)) {
      values[definition.id] = cloneSettingValue(valuesSource[definition.id]);
    }
  }
  return { pluginId: plugin.id, pluginVersion: plugin.version, values, configuredSecrets };
}

/** 检测可选桌面桥接；浏览器构建必须保持可用 / Detects optional desktop bridge; browser builds must remain usable. */
function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

let desktopHostSdk: HostSdk | undefined;

function getDesktopHostSdk(): HostSdk {
  desktopHostSdk ??= new HostSdk(createDefaultTauriHostSdkTransport());
  return desktopHostSdk;
}

/** 桌面端列出安装包；Web 环境返回空列表 / Lists installed packages on desktop; returns empty in web environments. */
export async function listInstalledMycPlugins(): Promise<InstalledMycPlugin[]> {
  if (!hasTauriRuntime()) return [];
  return getDesktopHostSdk().call<InstalledMycPlugin[]>("plugin.list", {});
}

/** 委托 Rust 安装器校验和提取包，前端绝不自行解压 / Delegates validation and extraction to Rust; frontend never unpacks archives. */
export async function installMycPlugin(path: string): Promise<InstalledMycPlugin> {
  if (!hasTauriRuntime()) {
    throw new Error("MYC_DESKTOP_REQUIRED");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<InstalledMycPlugin>("install_myc_plugin", { path });
}

/** Removes one exact installed version; bundled packages stay suppressed until explicitly reinstalled. */
export async function uninstallMycPlugin(plugin: PluginReference): Promise<void> {
  if (!hasTauriRuntime()) throw new Error("MYC_DESKTOP_REQUIRED");
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("uninstall_myc_plugin", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
  });
}

/**
 * Reads host-owned plugin settings. The host operation receives only the
 * identity; the host remains authoritative for definitions and secrets.
 * Non-desktop and built-in catalog entries use a safe local fallback.
 */
export async function getPluginSettings(
  plugin: PluginReference,
  definitions: readonly PluginSettingDefinitionLike[],
  options: { native?: boolean } = {},
): Promise<PluginSettingsSnapshot> {
  if (!hasTauriRuntime() || options.native === false) {
    return readBrowserSnapshot(plugin, definitions);
  }
  const raw = await getDesktopHostSdk().call<unknown>("plugin.settings.read", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
  });
  return normalizePluginSettingsSnapshot(raw, plugin, definitions);
}

/** Saves non-secret values and secret mutations without exposing secret text in the snapshot. */
export async function savePluginSettings(
  plugin: PluginReference,
  definitions: readonly PluginSettingDefinitionLike[],
  write: PluginSettingsWrite,
  options: { native?: boolean } = {},
): Promise<PluginSettingsSnapshot> {
  const current = readBrowserSnapshot(plugin, definitions);
  const next: PluginSettingsSnapshot = {
    ...current,
    values: { ...current.values, ...write.values },
    configuredSecrets: { ...current.configuredSecrets },
  };
  for (const [id, mutation] of Object.entries(write.secrets)) {
    if (mutation.action === "set") next.configuredSecrets[id] = mutation.value.length > 0;
    if (mutation.action === "clear") next.configuredSecrets[id] = false;
  }
  if (!hasTauriRuntime() || options.native === false) {
    writeBrowserSnapshot(next);
    return next;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const nativeValues: Record<string, unknown> = { ...write.values };
  for (const [id, mutation] of Object.entries(write.secrets)) {
    if (mutation.action === "set") nativeValues[id] = mutation.value;
    if (mutation.action === "clear") nativeValues[id] = null;
  }
  const raw = await invoke<unknown>("set_plugin_settings", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
    values: nativeValues,
  });
  return normalizePluginSettingsSnapshot(raw ?? next, plugin, definitions, next);
}

/** Restores manifest defaults; the host must erase stored credentials atomically. */
export async function resetPluginSettings(
  plugin: PluginReference,
  definitions: readonly PluginSettingDefinitionLike[],
  options: { native?: boolean } = {},
): Promise<PluginSettingsSnapshot> {
  const defaults = fallbackSnapshot(plugin, definitions);
  if (!hasTauriRuntime() || options.native === false) {
    if (typeof window !== "undefined") {
      try { window.localStorage.removeItem(browserSettingsKey(plugin)); } catch { /* optional storage */ }
    }
    return defaults;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const raw = await invoke<unknown>("reset_plugin_settings", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
  });
  return normalizePluginSettingsSnapshot(raw ?? defaults, plugin, definitions, defaults);
}

/**
 * Runs a host-owned connection action using the current unsaved draft. The
 * legacy three-argument form defaults to `test-connection`; the five-argument
 * form can dispatch `test-connection` or `test-pdf-extraction`. Secret text is
 * sent only through the separate mutation map, and the native command returns
 * only a stable code plus safe, localizable status copy.
 */
export function testPluginConnection(
  plugin: PluginReference,
  connectionId: string,
  write: PluginSettingsWrite,
  options?: { native?: boolean },
): Promise<PluginConnectionTestResult>;
export function testPluginConnection(
  plugin: PluginReference,
  connectionId: string,
  actionId: string,
  write: PluginSettingsWrite,
  options?: { native?: boolean },
): Promise<PluginConnectionTestResult>;
export async function testPluginConnection(
  plugin: PluginReference,
  connectionId: string,
  actionOrWrite: string | PluginSettingsWrite,
  writeOrOptions?: PluginSettingsWrite | { native?: boolean },
  maybeOptions: { native?: boolean } = {},
): Promise<PluginConnectionTestResult> {
  const actionId = typeof actionOrWrite === "string" ? actionOrWrite : "test-connection";
  const write = (typeof actionOrWrite === "string" ? writeOrOptions : actionOrWrite) as PluginSettingsWrite;
  const options = (typeof actionOrWrite === "string" ? maybeOptions : writeOrOptions) as { native?: boolean } | undefined;
  if (!hasTauriRuntime() || options?.native === false) {
    throw new Error("MYC_DESKTOP_REQUIRED");
  }
  if (!write || typeof write !== "object") throw new Error("PLUGIN_SETTINGS_WRITE_REQUIRED");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<PluginConnectionTestResult>("test_plugin_connection", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
    connectionId,
    actionId,
    values: write.values,
    secrets: write.secrets,
  });
}

/**
 * Executes an installed WASM plugin inside the native capability sandbox.
 * `capability` is forwarded to the host so it can verify the invoked operation
 * against the plugin manifest instead of trusting the frontend alone.
 */
export async function runAnalysisPlugin<TContext = unknown, TPayload = unknown>(
  plugin: PluginReference,
  request: Omit<PluginCallEnvelope<TContext, TPayload>, "apiVersion">,
  capability?: string,
): Promise<PluginExecutionResult> {
  if (!hasTauriRuntime()) throw new Error("MYC_DESKTOP_REQUIRED");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<PluginExecutionResult>("execute_myc_plugin", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
    capability,
    input: { apiVersion: PLUGIN_CALL_API_VERSION, ...request },
  });
}

/** 仅在桌面端监听拖放，并只转发 `.myc` 候选路径 / Listens for desktop drops and forwards only `.myc` candidates. */
export async function listenForMycDrops(
  onDrop: (paths: string[]) => void,
): Promise<() => void> {
  if (!hasTauriRuntime()) return () => undefined;
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "drop") {
      onDrop(event.payload.paths.filter((path) => path.toLowerCase().endsWith(".myc")));
    }
  });
}

/** 打开原生文件对话框，让用户选择一个或多个 .myc 插件包 / Opens native file dialog for picking .myc packages. */
export async function pickMycFiles(): Promise<string[] | null> {
  if (!hasTauriRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: true,
    filters: [{ name: "Myc Plugin", extensions: ["myc"] }],
  });
  if (!selected) return null;
  const paths = Array.isArray(selected) ? selected : [selected];
  return paths.length > 0 ? paths : null;
}

/** Opens the native picker for one VSIX; production parsing stays in Rust. */
export async function pickVsixFile(): Promise<string | null> {
  if (!hasTauriRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "VS Code extension theme", extensions: ["vsix"] }],
  });
  return typeof selected === "string" ? selected : null;
}

export async function importVsixTheme(path: string): Promise<NativeVsixImportReport> {
  if (!hasTauriRuntime()) throw new Error("MYC_DESKTOP_REQUIRED");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<NativeVsixImportReport>("import_vscode_vsix", { path });
}

/** Resolves one host-validated IconThemePlugin asset as a safe data URL. */
export async function readIconThemeAsset(
  plugin: Pick<PluginReference, "id" | "version">,
  assetPath: string,
): Promise<string | null> {
  if (!hasTauriRuntime()) return null;
  return getDesktopHostSdk().call<string>("plugin.icon-theme.read", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
    assetPath,
  });
}
