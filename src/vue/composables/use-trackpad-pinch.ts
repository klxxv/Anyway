import { onBeforeUnmount, onMounted, shallowReadonly, shallowRef } from "vue";

import type { CanvasTrackpadFrame } from "../canvas/canvas-types";
import { listenForNativeTrackpadFrames } from "../runtime/tauri-client";

export function useNativeTrackpadFrames() {
  const trackpadFrame = shallowRef<CanvasTrackpadFrame | null>(null);
  let disposed = false;
  let stopListening: (() => void) | null = null;

  onMounted(() => {
    void listenForNativeTrackpadFrames((frame) => {
      trackpadFrame.value = frame;
    }).then((stop) => {
      if (disposed) stop();
      else stopListening = stop;
    });
  });

  onBeforeUnmount(() => {
    disposed = true;
    stopListening?.();
    stopListening = null;
  });

  return {
    trackpadFrame: shallowReadonly(trackpadFrame),
  };
}
