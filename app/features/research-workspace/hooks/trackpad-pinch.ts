export type GesturePoint = { x: number; y: number };

export type GestureViewport = GesturePoint & { zoom: number };

export type GestureSize = { width: number; height: number };

export const TRACKPAD_MIN_ZOOM = 0.45;
export const TRACKPAD_MAX_ZOOM = 1.7;

/**
 * Applies the Demo's complete two-contact frame as one composed viewport
 * transform: the physical center controls pan while the physical span controls
 * zoom. Neither axis is emitted or accumulated independently.
 *
 * 将 Demo 的完整双触点帧一次性合成为视口变换：物理中心控制平移，
 * 物理间距控制缩放，不再独立发送或累加 X/Y 轴。
 */
export function viewportForCompleteTrackpadFrame(
  originViewport: GestureViewport,
  originCursor: GesturePoint,
  normalizedPan: GesturePoint,
  canvasSize: GestureSize,
  scale: number,
  minZoom = TRACKPAD_MIN_ZOOM,
  maxZoom = TRACKPAD_MAX_ZOOM,
): GestureViewport {
  const safeScale = Number.isFinite(scale) ? Math.max(0.25, Math.min(4, scale)) : 1;
  const nextZoom = Math.max(
    minZoom,
    Math.min(maxZoom, originViewport.zoom * safeScale),
  );
  const flowX = (originCursor.x - originViewport.x) / originViewport.zoom;
  const flowY = (originCursor.y - originViewport.y) / originViewport.zoom;
  const currentCursor = {
    x: originCursor.x + normalizedPan.x * canvasSize.width,
    y: originCursor.y + normalizedPan.y * canvasSize.height,
  };
  return {
    x: currentCursor.x - flowX * nextZoom,
    y: currentCursor.y - flowY * nextZoom,
    zoom: nextZoom,
  };
}
