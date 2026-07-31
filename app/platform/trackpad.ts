export type NativeTrackpadContact = {
  phase: "down" | "move" | "up";
  pointerId: number;
  contactCount: number;
  x: number;
  y: number;
  timestampMs: number;
};

/** 仅在 Tauri/Windows 提供原始触控板事件；浏览器继续使用标准 PointerEvent。 / Uses native contacts only when Tauri exposes them; browsers retain PointerEvent fallbacks. */
export async function listenForNativeTrackpadContacts(
  handler: (contact: NativeTrackpadContact) => void,
): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => undefined;
  const { listen } = await import("@tauri-apps/api/event");
  return listen<NativeTrackpadContact>("research-canvas://trackpad-contact", (event) => {
    handler(event.payload);
  });
}
