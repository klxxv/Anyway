import type { ResearchNodeType } from "../../../lib/research-types";

export type GesturePoint = { x: number; y: number };

export type GestureViewport = GesturePoint & { zoom: number };

export const TWO_FINGER_HOLD_MS = 1_000;
export const HOLD_CENTER_TOLERANCE_PX = 24;
export const HOLD_SPAN_TOLERANCE_PX = 24;
export const PIE_SELECTION_DEAD_ZONE_PX = 34;
export const PIE_DIAMETER_PX = 276;
export const TRACKPAD_MIN_ZOOM = 0.45;
export const TRACKPAD_MAX_ZOOM = 1.7;

const pieDirections: Array<{ angle: number; type: ResearchNodeType }> = [
  { angle: -90, type: "question" },
  { angle: -45, type: "concept" },
  { angle: 0, type: "variable" },
  { angle: 45, type: "method" },
  { angle: 90, type: "dataset" },
  { angle: 135, type: "evidence" },
  { angle: 180, type: "result" },
  { angle: -135, type: "note" },
];

export type TwoFingerFrame = {
  center: GesturePoint;
  span: number;
};

/**
 * 将 Windows/WebView 的合成捏合滚轮换算为以手势中心为锚点的视口。
 * Converts a Windows/WebView synthetic pinch wheel into a cursor-anchored viewport.
 */
export function viewportForTrackpadPinch(
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
  const flowX = (cursor.x - viewport.x) / viewport.zoom;
  const flowY = (cursor.y - viewport.y) / viewport.zoom;
  return {
    x: cursor.x - flowX * nextZoom,
    y: cursor.y - flowY * nextZoom,
    zoom: nextZoom,
  };
}

/** 计算双触点中心与间距；间距用于把捏合缩放和长按明确分流。 / Measures center and span so pinch and hold take separate paths. */
export function measureTwoFingerFrame(points: readonly GesturePoint[]): TwoFingerFrame | null {
  if (points.length !== 2) return null;
  return {
    center: {
      x: (points[0].x + points[1].x) / 2,
      y: (points[0].y + points[1].y) / 2,
    },
    span: Math.hypot(points[1].x - points[0].x, points[1].y - points[0].y),
  };
}

/** 只有中心漂移和双指间距都稳定时才允许进入长按菜单。 / A hold remains eligible only while center and finger span stay stable. */
export function isStableTwoFingerHold(
  origin: TwoFingerFrame,
  current: TwoFingerFrame,
): boolean {
  return (
    Math.hypot(current.center.x - origin.center.x, current.center.y - origin.center.y) <=
      HOLD_CENTER_TOLERANCE_PX &&
    Math.abs(current.span - origin.span) <= HOLD_SPAN_TOLERANCE_PX
  );
}

/** 将拖动方向映射到与八瓣饼图完全一致的节点类型。 / Maps drag direction to the exact eight-way pie order. */
export function selectPieNodeType(
  origin: GesturePoint,
  current: GesturePoint,
): ResearchNodeType | null {
  const dx = current.x - origin.x;
  const dy = current.y - origin.y;
  if (Math.hypot(dx, dy) < PIE_SELECTION_DEAD_ZONE_PX) return null;
  const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
  return pieDirections.reduce((nearest, candidate) => {
    const candidateDistance = Math.abs(((angle - candidate.angle + 540) % 360) - 180);
    const nearestDistance = Math.abs(((angle - nearest.angle + 540) % 360) - 180);
    return candidateDistance < nearestDistance ? candidate : nearest;
  }).type;
}

/** 保证饼图中心离画布边缘至少一个半径，避免菜单被裁切。 / Keeps the pie center one radius inside the canvas. */
export function clampPieMenuPoint(
  point: GesturePoint,
  width: number,
  height: number,
): GesturePoint {
  const inset = PIE_DIAMETER_PX / 2 + 10;
  const clamp = (value: number, extent: number) =>
    extent <= inset * 2 ? extent / 2 : Math.min(Math.max(value, inset), extent - inset);
  return { x: clamp(point.x, width), y: clamp(point.y, height) };
}
