<script setup lang="ts">
/**
 * Dynamic (morphing) icon backed by morphicons.
 *
 * Pass a 24×24 SVG `d` string via `icon`; when the value changes, the previous
 * shape morphs into the new one with spring physics (no from/to wiring needed).
 * Extra `class`/`style`/`aria-*` attributes fall through to the underlying SVG.
 */

import { MorphIcon, type SpringPreset } from "morphicons/vue";

withDefaults(
  defineProps<{
    /** A 24×24 SVG path `d`. Changing this prop morphs the icon. */
    icon: string;
    size?: number | string;
    color?: string;
    strokeWidth?: number | string;
    /** Accessibility: with `label` → role="img" + <title>; without → aria-hidden. */
    label?: string;
    spring?: SpringPreset;
  }>(),
  {
    size: 20,
    color: "currentColor",
    strokeWidth: 1.35,
    label: undefined,
    spring: "snappy",
  },
);
</script>

<template>
  <MorphIcon
    :icon="icon"
    :size="size"
    :color="color"
    :stroke-width="strokeWidth"
    :label="label"
    :spring="spring"
  />
</template>
