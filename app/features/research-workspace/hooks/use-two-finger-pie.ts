"use client";

import {
  useCallback,
  useRef,
  type MouseEvent as ReactMouseEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type { NativeTrackpadContact } from "../../../platform/trackpad";
import type { ResearchNodeType } from "../../../lib/research-types";
import type { PieMenuState } from "../workspace-types";
import {
  TWO_FINGER_HOLD_MS,
  isStableTwoFingerHold,
  measureTwoFingerFrame,
  selectPieNodeType,
  type GesturePoint,
  type TwoFingerFrame,
} from "./two-finger-gesture";

type GestureSource = "pointer" | "trackpad";

type GestureState = {
  source: GestureSource | null;
  points: Map<string, GesturePoint>;
  origin: TwoFingerFrame | null;
  latest: TwoFingerFrame | null;
  timer: ReturnType<typeof setTimeout> | null;
  opened: boolean;
  selectedType: ResearchNodeType | null;
  target: HTMLElement | null;
  capturedPointerIds: number[];
  suppressContextUntil: number;
};

type GestureCallbacks = {
  toFlowPoint: (screen: GesturePoint) => GesturePoint;
  onOpen: (menu: PieMenuState) => void;
  onHighlight: (type: ResearchNodeType | null) => void;
  onChoose: (type: ResearchNodeType, flowPoint: GesturePoint) => void;
  onDismiss: () => void;
};

/**
 * 识别“一秒双指长按—移动选择—松开提交”，并让捏合缩放保持在 React Flow 中。
 * Recognizes one-second hold, directional selection, and release while leaving pinch to React Flow.
 */
export function useTwoFingerPie(callbacks: GestureCallbacks) {
  const { toFlowPoint, onOpen, onHighlight, onChoose, onDismiss } = callbacks;
  const state = useRef<GestureState>({
    source: null,
    points: new Map(),
    origin: null,
    latest: null,
    timer: null,
    opened: false,
    selectedType: null,
    target: null,
    capturedPointerIds: [],
    suppressContextUntil: 0,
  });

  const reset = useCallback((keepContextSuppression = false) => {
    const gesture = state.current;
    if (gesture.timer) clearTimeout(gesture.timer);
    if (gesture.target) {
      gesture.capturedPointerIds.forEach((pointerId) => {
        try {
          if (gesture.target?.hasPointerCapture(pointerId)) {
            gesture.target.releasePointerCapture(pointerId);
          }
        } catch {
          // Pointer capture is an enhancement; some touch/driver stacks omit it.
        }
      });
    }
    gesture.source = null;
    gesture.points.clear();
    gesture.origin = null;
    gesture.latest = null;
    gesture.timer = null;
    gesture.opened = false;
    gesture.selectedType = null;
    gesture.target = null;
    gesture.capturedPointerIds = [];
    if (!keepContextSuppression) gesture.suppressContextUntil = 0;
  }, []);

  const armHold = useCallback(() => {
    const gesture = state.current;
    if (gesture.timer) clearTimeout(gesture.timer);
    const frame = measureTwoFingerFrame([...gesture.points.values()]);
    if (!frame) return;
    gesture.origin = frame;
    gesture.latest = frame;
    gesture.timer = setTimeout(() => {
      if (!gesture.origin || !gesture.latest || gesture.points.size !== 2) return;
      if (!isStableTwoFingerHold(gesture.origin, gesture.latest)) return;
      gesture.opened = true;
      gesture.suppressContextUntil = Number.POSITIVE_INFINITY;
      if (gesture.target) {
        gesture.capturedPointerIds.forEach((pointerId) => {
          try {
            gesture.target?.setPointerCapture(pointerId);
          } catch {
            // Native trackpad observers and older WebViews may not support capture.
          }
        });
      }
      const origin = gesture.origin.center;
      const flow = toFlowPoint(origin);
      onOpen({
        screenX: origin.x,
        screenY: origin.y,
        flowX: flow.x,
        flowY: flow.y,
        gestureActive: true,
        selectedType: null,
      });
    }, TWO_FINGER_HOLD_MS);
  }, [onOpen, toFlowPoint]);

  const beginContact = useCallback(
    (
      source: GestureSource,
      key: string,
      point: GesturePoint,
      target: HTMLElement | null = null,
      pointerId?: number,
    ) => {
      const gesture = state.current;
      if (gesture.source && gesture.source !== source) reset();
      gesture.source = source;
      gesture.points.set(key, point);
      if (target) gesture.target = target;
      if (pointerId !== undefined && !gesture.capturedPointerIds.includes(pointerId)) {
        gesture.capturedPointerIds.push(pointerId);
      }
      if (gesture.points.size > 2) {
        if (gesture.opened) onDismiss();
        reset();
        return;
      }
      if (gesture.points.size === 2) armHold();
    },
    [armHold, onDismiss, reset],
  );

  const moveContact = useCallback(
    (key: string, point: GesturePoint) => {
      const gesture = state.current;
      if (!gesture.points.has(key)) return;
      gesture.points.set(key, point);
      const frame = measureTwoFingerFrame([...gesture.points.values()]);
      if (!frame || !gesture.origin) return;
      gesture.latest = frame;
      if (!gesture.opened && !isStableTwoFingerHold(gesture.origin, frame)) {
        if (gesture.timer) clearTimeout(gesture.timer);
        gesture.timer = null;
        return;
      }
      if (!gesture.opened) return;
      const selectedType = selectPieNodeType(gesture.origin.center, frame.center);
      if (selectedType !== gesture.selectedType) {
        gesture.selectedType = selectedType;
        onHighlight(selectedType);
      }
    },
    [onHighlight],
  );

  const finishContact = useCallback(
    (key: string, point: GesturePoint) => {
      const gesture = state.current;
      if (!gesture.points.has(key)) return;
      gesture.points.set(key, point);
      const frame = measureTwoFingerFrame([...gesture.points.values()]);
      if (frame) gesture.latest = frame;
      if (!gesture.opened || !gesture.origin) {
        reset();
        return;
      }
      const origin = gesture.origin.center;
      const selectedType = gesture.latest
        ? selectPieNodeType(origin, gesture.latest.center)
        : gesture.selectedType;
      gesture.suppressContextUntil = Date.now() + 700;
      reset(true);
      if (selectedType) onChoose(selectedType, toFlowPoint(origin));
      else onDismiss();
    },
    [onChoose, onDismiss, reset, toFlowPoint],
  );

  const onPointerDownCapture = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      if (event.pointerType !== "touch") return;
      const bounds = event.currentTarget.getBoundingClientRect();
      beginContact(
        "pointer",
        `pointer:${event.pointerId}`,
        { x: event.clientX - bounds.left, y: event.clientY - bounds.top },
        event.currentTarget,
        event.pointerId,
      );
    },
    [beginContact],
  );

  const onPointerMoveCapture = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const key = `pointer:${event.pointerId}`;
      if (!state.current.points.has(key)) return;
      const bounds = event.currentTarget.getBoundingClientRect();
      moveContact(key, {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });
      if (state.current.opened) {
        event.preventDefault();
        event.stopPropagation();
      }
    },
    [moveContact],
  );

  const onPointerUpCapture = useCallback(
    (event: ReactPointerEvent<HTMLElement>) => {
      const key = `pointer:${event.pointerId}`;
      if (!state.current.points.has(key)) return;
      const opened = state.current.opened;
      const bounds = event.currentTarget.getBoundingClientRect();
      finishContact(key, {
        x: event.clientX - bounds.left,
        y: event.clientY - bounds.top,
      });
      if (opened) {
        event.preventDefault();
        event.stopPropagation();
      }
    },
    [finishContact],
  );

  const onNativeTrackpadContact = useCallback(
    (contact: NativeTrackpadContact, canvasOffset: GesturePoint) => {
      const key = `trackpad:${contact.pointerId}`;
      const point = { x: contact.x - canvasOffset.x, y: contact.y - canvasOffset.y };
      if (contact.phase === "down") beginContact("trackpad", key, point);
      else if (contact.phase === "move") moveContact(key, point);
      else finishContact(key, point);
    },
    [beginContact, finishContact, moveContact],
  );

  const onPointerCancelCapture = useCallback(() => {
    if (state.current.opened) onDismiss();
    reset();
  }, [onDismiss, reset]);

  const onContextMenuCapture = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    if (Date.now() < state.current.suppressContextUntil) {
      event.preventDefault();
      event.stopPropagation();
    }
  }, []);

  return {
    onPointerDownCapture,
    onPointerMoveCapture,
    onPointerUpCapture,
    onPointerCancelCapture,
    onContextMenuCapture,
    onNativeTrackpadContact,
  };
}
