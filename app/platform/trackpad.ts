export type NativeTrackpadFrame = {
  phase: "start" | "update" | "end";
  frameId: number;
  contacts: Array<{ id: number; x: number; y: number }>;
  centerX: number;
  centerY: number;
  span: number;
  scale: number;
  panX: number;
  panY: number;
  deviceWidth: number;
  deviceHeight: number;
  cursorX: number;
  cursorY: number;
};

/**
 * 仅在 Tauri/Windows 监听原子化完整触控板帧；浏览器继续使用 WheelEvent 回退。
 * Listens for atomic complete touchpad frames only in Tauri/Windows.
 */
export async function listenForNativeTrackpadFrames(
  handler: (frame: NativeTrackpadFrame) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<NativeTrackpadFrame>("research-canvas://trackpad-frame", (event) => {
    handler(event.payload);
  });
}
