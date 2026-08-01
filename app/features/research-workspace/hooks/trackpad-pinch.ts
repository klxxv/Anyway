export type GesturePoint = { x: number; y: number };

export type GestureViewport = GesturePoint & { zoom: number };

export const TRACKPAD_MIN_ZOOM = 0.45;
export const TRACKPAD_MAX_ZOOM = 1.7;

function anchoredViewport(
  viewport: GestureViewport,
  cursor: GesturePoint,
  nextZoom: number,
): GestureViewport {
  const flowX = (cursor.x - viewport.x) / viewport.zoom;
  const flowY = (cursor.y - viewport.y) / viewport.zoom;
  return {
    x: cursor.x - flowX * nextZoom,
    y: cursor.y - flowY * nextZoom,
    zoom: nextZoom,
  };
}

/**
 * 将浏览器合成的捏合滚轮转换为以光标为锚点的视口。
 * Converts a browser-synthesized pinch wheel into a cursor-anchored viewport.
 */
export function viewportForTrackpadWheel(
  viewport: GestureViewport,
  cursor: GesturePoint,
  deltaY: number,
  deltaMode = 0,
  minZoom = TRACKPAD_MIN_ZOOM,
  maxZoom = TRACKPAD_MAX_ZOOM,
): GestureViewport {
  const modeScale = deltaMode === 1 ? 16 : deltaMode === 2 ? 120 : 1;
  const normalizedDelta = Math.max(-80, Math.min(80, deltaY * modeScale));
  const nextZoom = Math.max(
    minZoom,
    Math.min(maxZoom, viewport.zoom * Math.exp(-normalizedDelta * 0.005)),
  );
  return anchoredViewport(viewport, cursor, nextZoom);
}

/**
 * 使用 Precision Touchpad 的物理双指间距计算缩放，绕过 WebView 的滚轮转换差异。
 * Uses physical Precision Touchpad finger distance, bypassing WebView wheel conversion.
 */
export function viewportForNativeTrackpadPinch(
  originViewport: GestureViewport,
  cursor: GesturePoint,
  originSpan: number,
  currentSpan: number,
  minZoom = TRACKPAD_MIN_ZOOM,
  maxZoom = TRACKPAD_MAX_ZOOM,
): GestureViewport {
  if (!(originSpan > 0) || !(currentSpan > 0)) return originViewport;
  const ratio = Math.max(0.25, Math.min(4, currentSpan / originSpan));
  const nextZoom = Math.max(minZoom, Math.min(maxZoom, originViewport.zoom * ratio));
  return anchoredViewport(originViewport, cursor, nextZoom);
}

/** 计算两个设备坐标触点之间的物理距离。 / Measures two device-relative contacts. */
export function measurePhysicalPinchSpan(points: readonly GesturePoint[]): number | null {
  if (points.length !== 2) return null;
  return Math.hypot(points[1].x - points[0].x, points[1].y - points[0].y);
}
