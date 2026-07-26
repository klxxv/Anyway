import type { InstalledMycPlugin } from "./contracts";

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function listInstalledMycPlugins(): Promise<InstalledMycPlugin[]> {
  if (!hasTauriRuntime()) return [];
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<InstalledMycPlugin[]>("list_installed_plugins");
}

export async function installMycPlugin(path: string): Promise<InstalledMycPlugin> {
  if (!hasTauriRuntime()) {
    throw new Error("MYC_DESKTOP_REQUIRED");
  }
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<InstalledMycPlugin>("install_myc_plugin", { path });
}

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

