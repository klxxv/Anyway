import type { ResearchNodeType } from "../../../lib/research-types";

export type GesturePoint = { x: number; y: number };

export const TWO_FINGER_HOLD_MS = 1_000;
export const HOLD_CENTER_TOLERANCE_PX = 14;
export const HOLD_SPAN_TOLERANCE_PX = 12;
export const PIE_SELECTION_DEAD_ZONE_PX = 34;
export const PIE_DIAMETER_PX = 276;

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
