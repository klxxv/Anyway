export type GesturePoint = { x: number; y: number };

export type GestureViewport = GesturePoint & { zoom: number };

export type GestureSize = { width: number; height: number };

export type TrackpadLowPassState = {
  pan: GesturePoint;
  scaleLog: number;
};

export const emptyTrackpadLowPassState = (): TrackpadLowPassState => ({
  pan: { x: 0, y: 0 },
  scaleLog: 0,
});

export const TRACKPAD_MIN_ZOOM = 0.45;
export const TRACKPAD_MAX_ZOOM = 1.7;

function applyDeadZone(value: number, deadZone: number): number {
  if (!Number.isFinite(value) || Math.abs(value) <= deadZone) return 0;
  return Math.sign(value) * (Math.abs(value) - deadZone);
}

/**
 * Filters the native bridge's cumulative two-contact frame. A normalized dead
 * zone keeps the canvas stationary during involuntary micro-movement, while an
 * EMA preserves composed two-dimensional motion after the threshold is crossed.
 *
 * 对原生桥接的累计双指帧做低通处理：归一化死区抑制微动，越过阈值后仍以同一
 * 帧合成二维平移与缩放。
 */
export function lowPassCompleteTrackpadFrame(
  previous: TrackpadLowPassState,
  rawPan: GesturePoint,
  rawScale: number,
  sensitivity: number,
  filterStrength: number,
): { state: TrackpadLowPassState; pan: GesturePoint; scale: number } {
  const safeSensitivity = Math.min(2, Math.max(0.5, sensitivity));
  const strength = Math.min(0.9, Math.max(0, filterStrength));
  const panDeadZone = 0.0008 + strength * 0.0045;
  const scaleDeadZone = 0.0015 + strength * 0.008;
  const alpha = 1 - strength * 0.72;
  const targetPan = {
    x: applyDeadZone(rawPan.x, panDeadZone) * safeSensitivity,
    y: applyDeadZone(rawPan.y, panDeadZone) * safeSensitivity,
  };
  const rawScaleLog = Number.isFinite(rawScale) && rawScale > 0 ? Math.log(rawScale) : 0;
  const targetScaleLog = applyDeadZone(rawScaleLog, scaleDeadZone) * safeSensitivity;
  const state = {
    pan: {
      x: previous.pan.x + (targetPan.x - previous.pan.x) * alpha,
      y: previous.pan.y + (targetPan.y - previous.pan.y) * alpha,
    },
    scaleLog: previous.scaleLog + (targetScaleLog - previous.scaleLog) * alpha,
  };
  if (targetPan.x === 0 && Math.abs(state.pan.x) < panDeadZone * 0.2) state.pan.x = 0;
  if (targetPan.y === 0 && Math.abs(state.pan.y) < panDeadZone * 0.2) state.pan.y = 0;
  if (targetScaleLog === 0 && Math.abs(state.scaleLog) < scaleDeadZone * 0.2) {
    state.scaleLog = 0;
  }
  return { state, pan: state.pan, scale: Math.exp(state.scaleLog) };
}

/**
 * Chromium exposes a trackpad pinch as Ctrl+Wheel and defines the inverse
 * conversion as exp(-deltaY / 100). Chrome's PDF viewer clamps each update to
 * avoid dramatic jumps from physical mouse wheels.
 * Chromium 将触控板捏合暴露为 Ctrl+Wheel，其反向换算公式为 exp(-deltaY / 100)。
 */
export function chromiumTrackpadPinchScale(deltaY: number): number {
  if (!Number.isFinite(deltaY)) return 1;
  return Math.min(1.25, Math.max(0.75, Math.exp(-deltaY / 100)));
}

/**
 * Converts a DOM wheel delta to CSS pixels without splitting its axes.
 * WebView2 precision-touchpad input is normally pixel based, while mouse and
 * accessibility drivers may emit line or page units.
 *
 * 将同一 WheelEvent 的双轴增量一次性换算为 CSS 像素，避免拆分 X/Y。
 */
export function wheelPanDelta(
  deltaX: number,
  deltaY: number,
  deltaMode: number,
): GesturePoint {
  const factor = deltaMode === 1 ? 20 : deltaMode === 2 ? 100 : 1;
  return {
    x: Number.isFinite(deltaX) ? deltaX * factor : 0,
    y: Number.isFinite(deltaY) ? deltaY * factor : 0,
  };
}

/**
 * Composes one coalesced WebView wheel frame. Pan and pinch are applied in a
 * single viewport write, keeping diagonal movement atomic and zoom anchored at
 * the physical cursor.
 *
 * 合成一个 WebView 滚轮帧：二维平移与捏合只写入一次视口。
 */
export function viewportForCoalescedWheelFrame(
  originViewport: GestureViewport,
  cursor: GesturePoint,
  panDelta: GesturePoint,
  scale: number,
  minZoom = TRACKPAD_MIN_ZOOM,
  maxZoom = TRACKPAD_MAX_ZOOM,
): GestureViewport {
  const safeScale = Number.isFinite(scale) ? Math.max(0.25, Math.min(4, scale)) : 1;
  const nextZoom = Math.max(
    minZoom,
    Math.min(maxZoom, originViewport.zoom * safeScale),
  );
  const flowX = (cursor.x - originViewport.x) / originViewport.zoom;
  const flowY = (cursor.y - originViewport.y) / originViewport.zoom;
  return {
    x: cursor.x - flowX * nextZoom - panDelta.x,
    y: cursor.y - flowY * nextZoom - panDelta.y,
    zoom: nextZoom,
  };
}

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
