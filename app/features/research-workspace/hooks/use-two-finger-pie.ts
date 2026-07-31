"use client";

import { useCallback, useRef, type PointerEvent as ReactPointerEvent } from "react";
import type { ResearchNodeType } from "../../../lib/research-types";
import type { PieMenuState } from "../workspace-types";

type Point = { x: number; y: number };

type GestureState = {
  points: Map<number, Point>;
  origin: Point | null;
  latest: Point | null;
  timer: ReturnType<typeof setTimeout> | null;
  opened: boolean;
};

const directions: Array<{ angle: number; type: ResearchNodeType }> = [
  { angle: -90, type: "question" },
  { angle: -30, type: "variable" },
  { angle: 30, type: "method" },
  { angle: 90, type: "evidence" },
  { angle: 150, type: "result" },
  { angle: 210, type: "note" },
];

function midpoint(points: Point[]) {
  return {
    x: (points[0].x + points[1].x) / 2,
    y: (points[0].y + points[1].y) / 2,
  };
}
/**
 * Converts a two-touch hold/flick into the same accessible pie-menu actions.
 * 将双触点长按和甩动转换为与可访问饼菜单一致的操作。
 */
export function useTwoFingerPie({
  toFlowPoint,
  onOpen,
  onChoose,
}: {
  toFlowPoint: (screen: Point) => Point;
  onOpen: (menu: PieMenuState) => void;
  onChoose: (type: ResearchNodeType, flowPoint: Point) => void;
}) {
  const state = useRef<GestureState>({
    points: new Map(),
    origin: null,
    latest: null,
    timer: null,
    opened: false,
  });

  const reset = useCallback(() => {
    const gesture = state.current;
    if (gesture.timer) clearTimeout(gesture.timer);
    gesture.points.clear();
    gesture.origin = null;
    gesture.latest = null;
    gesture.timer = null;
    gesture.opened = false;
  }, []);

  const onPointerDown = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.pointerType !== "touch") return;
      const bounds = event.currentTarget.getBoundingClientRect();
      const gesture = state.current;
      gesture.points.set(event.pointerId, {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });
      if (gesture.points.size !== 2) {
        if (gesture.points.size > 2) reset();
        return;
      }
      gesture.origin = midpoint([...gesture.points.values()]);
      gesture.latest = gesture.origin;
      gesture.timer = setTimeout(() => {
        if (!gesture.origin || gesture.points.size !== 2) return;
        gesture.opened = true;
        const flow = toFlowPoint(gesture.origin);
        onOpen({
          screenX: gesture.origin.x,
          screenY: gesture.origin.y,
          flowX: flow.x,
          flowY: flow.y,
        });
      }, 380);
    },
    [onOpen, reset, toFlowPoint],
  );

  const onPointerMove = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const gesture = state.current;
      if (!gesture.points.has(event.pointerId)) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      gesture.points.set(event.pointerId, {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });
      if (gesture.points.size !== 2 || !gesture.origin) return;
      gesture.latest = midpoint([...gesture.points.values()]);
      const distance = Math.hypot(
        gesture.latest.x - gesture.origin.x,
        gesture.latest.y - gesture.origin.y,
      );
      if (!gesture.opened && distance > 14 && gesture.timer) {
        clearTimeout(gesture.timer);
        gesture.timer = null;
      }
    },
    [],
  );

  const onPointerUp = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const gesture = state.current;
      if (!gesture.points.has(event.pointerId)) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      gesture.points.set(event.pointerId, {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });
      if (gesture.points.size === 2) gesture.latest = midpoint([...gesture.points.values()]);
      gesture.points.delete(event.pointerId);
      if (gesture.points.size > 0) return;

      const origin = gesture.origin;
      const latest = gesture.latest;
      const opened = gesture.opened;
      reset();
      if (!opened || !origin || !latest) return;

      const dx = latest.x - origin.x;
      const dy = latest.y - origin.y;
      if (Math.hypot(dx, dy) < 28) return;
      const angle = (Math.atan2(dy, dx) * 180) / Math.PI;
      const selected = directions.reduce((nearest, candidate) => {
        const candidateDistance = Math.abs(((angle - candidate.angle + 540) % 360) - 180);
        const nearestDistance = Math.abs(((angle - nearest.angle + 540) % 360) - 180);
        return candidateDistance < nearestDistance ? candidate : nearest;
      });
      onChoose(selected.type, toFlowPoint(origin));
    },
    [onChoose, reset, toFlowPoint],
  );

  return { onPointerDown, onPointerMove, onPointerUp, onPointerCancel: reset };
}
