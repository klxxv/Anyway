<script setup lang="ts">
import { computed } from "vue";
import {
  BaseEdge,
  EdgeLabelRenderer,
  Position,
  getBezierPath,
  getSmoothStepPath,
  getStraightPath,
  type EdgeProps,
} from "@vue-flow/core";
import type { WorkspaceEdgeData } from "./canvas-types";

type ResearchEdgeProps = EdgeProps<WorkspaceEdgeData>;

const props = defineProps<ResearchEdgeProps>();

const relationType = computed(() => props.data?.record.type ?? "causes");
const edgeStyle = computed(() => props.data?.edgeStyle);
const routing = computed(() => edgeStyle.value?.routing ?? "orthogonal");
const dragPreview = computed(() => props.data?.dragPreview === true);
const highlighted = computed(() => props.data?.highlighted === true);

const pathParts = computed(() => {
  const sourcePosition = props.sourcePosition ?? Position.Right;
  const targetPosition = props.targetPosition ?? Position.Left;
  const options = {
    sourceX: props.sourceX,
    sourceY: props.sourceY,
    targetX: props.targetX,
    targetY: props.targetY,
    sourcePosition,
    targetPosition,
  };
  if (dragPreview.value || routing.value === "straight") return getStraightPath(options);
  if (routing.value === "bezier") return getBezierPath(options);
  return getSmoothStepPath({
    ...options,
    borderRadius: edgeStyle.value?.stroke.cornerRadius ?? 12,
    offset: edgeStyle.value?.stroke.offset ?? 24,
  });
});

const path = computed(() => pathParts.value[0]);
const labelX = computed(() => pathParts.value[1] + (props.data?.labelOffsetX ?? 0));
const labelY = computed(() => pathParts.value[2] + (props.data?.labelOffsetY ?? 0));
const contradictory = computed(() => relationType.value === "contradicts");
const relationStyle = computed(() => edgeStyle.value?.relations?.[relationType.value]);
const diffState = computed(() => props.data?.diffState);

const dash = computed(() => {
  if (diffState.value === "removed") return "6 5";
  if (relationStyle.value?.dash?.length) return relationStyle.value.dash.join(" ");
  if (relationType.value === "controls") return "7 6";
  if (relationType.value === "derived_from") return "2 4";
  if (contradictory.value) return "7 4";
  return undefined;
});

const strokeColor = computed(() => {
  if (diffState.value === "added") return "var(--color-diff-added)";
  if (diffState.value === "removed") return "var(--color-diff-removed)";
  if (diffState.value === "modified") return "var(--color-diff-modified)";
  if (props.selected) return "#2457d6";
  if (relationStyle.value?.color) return relationStyle.value.color;
  if (contradictory.value) return "#c85c55";
  return edgeStyle.value?.stroke.color ?? "#656b72";
});

const lineStyle = computed(() => ({
  stroke: highlighted.value ? "var(--color-blue)" : strokeColor.value,
  strokeWidth: highlighted.value
    ? (edgeStyle.value?.stroke.selectedWidth ?? 2.6) + 0.6
    : props.selected
      ? edgeStyle.value?.stroke.selectedWidth ?? 2.6
      : relationStyle.value?.width ?? edgeStyle.value?.stroke.width ?? 1.05,
  opacity: highlighted.value
    ? 1
    : diffState.value === "removed"
      ? 0.55
      : relationStyle.value?.opacity ?? edgeStyle.value?.stroke.opacity ?? 1,
  strokeDasharray: dash.value,
}));

const markerEnd = computed(() => {
  return props.markerEnd || undefined;
});
</script>

<template>
  <BaseEdge
    :id="id"
    :path="path"
    :style="lineStyle"
    :marker-end="markerEnd"
  />
  <EdgeLabelRenderer v-if="!dragPreview">
    <span
      :class="[
        'research-edge-label pointer-events-none absolute -translate-x-1/2 -translate-y-1/2 bg-canvas px-1.5 font-serif text-[10px] italic',
        contradictory ? 'text-alert' : selected ? 'text-blue' : 'text-ink/75',
      ]"
      :style="{
        transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
      }"
    >
      {{ data?.label }}
    </span>
  </EdgeLabelRenderer>
</template>

<style scoped>
.research-edge-label {
  white-space: nowrap;
}
</style>
