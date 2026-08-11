<script lang="ts">
type LongPressListener = (active: boolean) => void;
const longPressListeners = new Set<LongPressListener>();
type SelectionModeListener = (active: boolean) => void;
const selectionModeListeners = new Set<SelectionModeListener>();

/** Call from the canvas gesture handler when long-press starts or ends. */
export function setCursorLongPress(active: boolean) {
  for (const listener of longPressListeners) listener(active);
}

/** Call from the canvas when box-selection mode is entered or exited. */
export function setCursorSelectionMode(active: boolean) {
  for (const listener of selectionModeListeners) listener(active);
}
</script>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";

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
  ".vue-flow__node",
].join(",");

function isTouchDevice() {
  if (typeof window === "undefined") return false;
  return window.matchMedia("(hover: none), (pointer: coarse)").matches;
}

const position = ref({ x: -100, y: -100 });
const hover = ref(false);
const visible = ref(false);
const longPress = ref(false);
const selectionMode = ref(false);
const enabled = ref(!isTouchDevice());

let animationFrame = 0;
let targetX = -100;
let targetY = -100;

function onMouseMove(event: MouseEvent) {
  targetX = event.clientX;
  targetY = event.clientY;
  visible.value = true;
  if (animationFrame) return;
  animationFrame = window.requestAnimationFrame(() => {
    position.value = { x: targetX, y: targetY };
    animationFrame = 0;
  });
}

function onMouseEnter() {
  visible.value = true;
}

function onMouseLeave() {
  visible.value = false;
}

function onMouseOver(event: MouseEvent) {
  const target = event.target;
  const element = target instanceof Element ? target : null;
  hover.value = Boolean(element?.closest(INTERACTIVE_SELECTOR));
}

function onLongPress(active: boolean) {
  longPress.value = active;
}

function onSelectionMode(active: boolean) {
  selectionMode.value = active;
}

onMounted(() => {
  if (enabled.value) {
    document.documentElement.classList.add("custom-cursor-active");
    document.addEventListener("mousemove", onMouseMove, { passive: true });
    document.addEventListener("mouseenter", onMouseEnter);
    document.addEventListener("mouseleave", onMouseLeave);
    document.addEventListener("mouseover", onMouseOver);
  }
  longPressListeners.add(onLongPress);
  selectionModeListeners.add(onSelectionMode);
});

onBeforeUnmount(() => {
  if (enabled.value) {
    document.documentElement.classList.remove("custom-cursor-active");
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseenter", onMouseEnter);
    document.removeEventListener("mouseleave", onMouseLeave);
    document.removeEventListener("mouseover", onMouseOver);
    if (animationFrame) window.cancelAnimationFrame(animationFrame);
  }
  longPressListeners.delete(onLongPress);
  selectionModeListeners.delete(onSelectionMode);
});
</script>

<template>
  <template v-if="enabled">
    <div
      class="custom-cursor-ring"
      :class="{ 'is-hover': hover, 'is-longpress': longPress, 'is-selection': selectionMode }"
      :style="{ translate: `${position.x}px ${position.y}px`, opacity: visible ? 1 : 0 }"
      aria-hidden="true"
    />
    <div
      class="custom-cursor-dot"
      :class="{ 'is-longpress': longPress, 'is-selection': selectionMode }"
      :style="{ transform: `translate(${position.x}px, ${position.y}px)`, opacity: visible ? 1 : 0 }"
      aria-hidden="true"
    />
  </template>
</template>

<style scoped>
/* Cursor geometry and theme tokens remain in app/globals.css. */
</style>
