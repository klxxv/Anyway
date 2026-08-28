import type { HostSdk } from "./host-sdk";

export type PluginSurfaceFileAttachTarget = {
  readonly pluginId: string;
  readonly pluginVersion: string;
  readonly sessionId?: string;
  readonly surfaceIds: readonly string[];
};

export type NativeFilePicker = () => Promise<readonly string[]>;

async function defaultNativeFilePicker(): Promise<readonly string[]> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({ title: "Attach files", multiple: true, directory: false });
  if (!selected) return [];
  return Array.isArray(selected) ? selected : [selected];
}

/**
 * Host-only attachment bridge. Local paths exist only between the native
 * picker and the Rust command; plugin state and worker payloads receive only
 * a display label plus the Host-created BlobRef.
 */
export async function attachFilesToPluginSurface(
  sdk: HostSdk,
  target: PluginSurfaceFileAttachTarget,
  pickFiles: NativeFilePicker = defaultNativeFilePicker,
): Promise<readonly unknown[]> {
  const paths = await pickFiles();
  const results: unknown[] = [];
  for (const localPath of paths) {
    if (typeof localPath !== "string" || localPath.length === 0) continue;
    results.push(await sdk.call("plugin.surface.file-attach", {
      pluginId: target.pluginId,
      pluginVersion: target.pluginVersion,
      sessionId: target.sessionId,
      surfaceIds: [...target.surfaceIds],
      localPath,
    }));
  }
  return results;
}
