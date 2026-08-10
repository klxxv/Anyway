"use client";

import { useEffect, useState } from "react";

// ---------------------------------------------------------------------------
// Module-level long-press broadcast so the canvas gesture system can signal
// the root-level cursor without creating a context dependency.
// ---------------------------------------------------------------------------

type LongPressListener = (active: boolean) => void;
const longPressListeners = new Set<LongPressListener>();

/** Call from the canvas gesture handler when long-press starts/ends. */
export function setCursorLongPress(active: boolean) {
  for (const fn of longPressListeners) fn(active);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const INTERACTIVE_SELECTOR = [
  "a",
  "button",
  "input",
  "textarea",
  "select",
  "[role='button']",
  "[role='menuitem']",
  "[role='menuitemradio']",
  "[data-cursor-hover]",
  ".clickable",
  ".zen-pie-item",
  ".react-flow__node",
].join(",");

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

function isTouchDevice() {
  if (typeof window === "undefined") return false;
  const mql = window.matchMedia("(hover: none), (pointer: coarse)");
  return mql.matches;
}

export function CustomCursor() {
  const [position, setPosition] = useState({ x: -100, y: -100 });
  const [hover, setHover] = useState(false);
  const [visible, setVisible] = useState(false);
  const [longPress, setLongPress] = useState(false);
  // Disable on touch devices at mount time, avoiding a state transition inside
  // an effect (which the React compiler/eslint flags as set-state-in-effect).
  const [enabled] = useState(() => !isTouchDevice());

  // Add/remove `custom-cursor-active` class on <html> to hide default cursor
  useEffect(() => {
    if (!enabled) return;
    const root = document.documentElement;
    root.classList.add("custom-cursor-active");
    return () => root.classList.remove("custom-cursor-active");
  }, [enabled]);

  // Mouse tracking + hover detection via event delegation
  useEffect(() => {
    if (!enabled) return;

    let raf = 0;
    let targetX = -100;
    let targetY = -100;

    const onMouseMove = (e: MouseEvent) => {
      targetX = e.clientX;
      targetY = e.clientY;
      if (!visible) setVisible(true);
      if (!raf) {
        raf = requestAnimationFrame(() => {
          setPosition({ x: targetX, y: targetY });
          raf = 0;
        });
      }
    };

    const onMouseEnterDoc = () => setVisible(true);
    const onMouseLeaveDoc = () => setVisible(false);

    const onMouseOver = (e: MouseEvent) => {
      const target = e.target as HTMLElement | null;
      if (!target) return;
      const interactive = target.closest(INTERACTIVE_SELECTOR);
      setHover(!!interactive);
    };

    document.addEventListener("mousemove", onMouseMove, { passive: true });
    document.addEventListener("mouseenter", onMouseEnterDoc);
    document.addEventListener("mouseleave", onMouseLeaveDoc);
    document.addEventListener("mouseover", onMouseOver);

    return () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseenter", onMouseEnterDoc);
      document.removeEventListener("mouseleave", onMouseLeaveDoc);
      document.removeEventListener("mouseover", onMouseOver);
      if (raf) cancelAnimationFrame(raf);
    };
  }, [enabled, visible]);

  // Long-press subscription
  useEffect(() => {
    const handler: LongPressListener = (active) => setLongPress(active);
    longPressListeners.add(handler);
    return () => {
      longPressListeners.delete(handler);
    };
  }, []);

  if (!enabled) return null;

  return (
    <>
      <div
        className={`custom-cursor-ring${hover ? " is-hover" : ""}${longPress ? " is-longpress" : ""}`}
        style={{
          translate: `${position.x}px ${position.y}px`,
          opacity: visible ? 1 : 0,
        }}
        aria-hidden="true"
      />
      <div
        className={`custom-cursor-dot${longPress ? " is-longpress" : ""}`}
        style={{
          transform: `translate(${position.x}px, ${position.y}px)`,
          opacity: visible ? 1 : 0,
        }}
        aria-hidden="true"
      />
    </>
  );
}
