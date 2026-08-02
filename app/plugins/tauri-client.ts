import type {
  InstalledMycPlugin,
  PluginCallEnvelope,
  PluginExecutionResult,
  PluginReference,
} from "./contracts";
import { PLUGIN_CALL_API_VERSION } from "./contracts";

/** 检测可选桌面桥接；浏览器构建必须保持可用 / Detects optional desktop bridge; browser builds must remain usable. */
function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

/** 桌面端列出安装包；Web 环境返回空列表 / Lists installed packages on desktop; returns empty in web environments. */
export async function listInstalledMycPlugins(): Promise<InstalledMycPlugin[]> {
  if (!hasTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<InstalledMycPlugin[]>("list_installed_plugins");
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

/** Executes an installed WASM plugin inside the native capability sandbox. */
export async function runAnalysisPlugin<TContext = unknown, TPayload = unknown>(
  plugin: PluginReference,
  request: Omit<PluginCallEnvelope<TContext, TPayload>, "apiVersion">,
): Promise<PluginExecutionResult> {
  if (!hasTauriRuntime()) throw new Error("MYC_DESKTOP_REQUIRED");
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<PluginExecutionResult>("execute_myc_plugin", {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
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
