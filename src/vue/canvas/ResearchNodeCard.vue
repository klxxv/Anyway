<script setup lang="ts">
import { computed, nextTick, watch } from "vue";
import { Handle, Position, useVueFlow, type NodeProps } from "@vue-flow/core";
import type { ResearchNodeType } from "../../../app/lib/research-types";
import type { WorkspaceNodeData } from "./canvas-types";
import { variableBranchValues } from "./variable-branches";

type ResearchNodeProps = NodeProps<WorkspaceNodeData, object, "researchNode">;

const props = defineProps<ResearchNodeProps>();
const emit = defineEmits<{
  (event: "toggle-expanded", nodeId: string): void;
}>();

const { updateNodeInternals } = useVueFlow();

const iconPaths: Partial<Record<ResearchNodeType, string>> = {
  question: "M10 17h.01M7.5 7.6a2.7 2.7 0 1 1 4.4 2.1c-1.1.9-1.9 1.3-1.9 3.3M4 3.5h12A1.5 1.5 0 0 1 17.5 5v10A1.5 1.5 0 0 1 16 16.5H4A1.5 1.5 0 0 1 2.5 15V5A1.5 1.5 0 0 1 4 3.5Z",
  concept: "M8 9.5a3 3 0 1 0 0-6 3 3 0 0 0 0 6Zm-5.5 6.2a5.5 5.5 0 0 1 11 0M14.5 8a2.5 2.5 0 1 0 0-5M14 10.4a4.6 4.6 0 0 1 3.5 4.5",
  variable: "M3 15.5V8.8m4 6.7V5m4 10.5v-8m4 8V3.5M2 15.5h14.5",
  method: "M8 2.5 9.7 6l3.8.5-2.8 2.6.8 3.9L8 11.2l-3.5 1.8.8-3.9L2.5 6.5 6.3 6 8 2.5Z",
  evidence: "M4 2.5h7l3 3v10H4a1.5 1.5 0 0 1-1.5-1.5V4A1.5 1.5 0 0 1 4 2.5Zm7 0v3h3M5.5 9h5M5.5 12h5",
  paper: "M4 2.5h7l3 3v10H4a1.5 1.5 0 0 1-1.5-1.5V4A1.5 1.5 0 0 1 4 2.5Zm7 0v3h3M5.5 9h5M5.5 12h5",
  dataset: "M3 5c0-1.1 2.2-2 5-2s5 .9 5 2-2.2 2-5 2-5-.9-5-2Zm0 0v4c0 1.1 2.2 2 5 2s5-.9 5-2V5m-10 4v4c0 1.1 2.2 2 5 2s5-.9 5-2V9",
  result: "m3 9 3.2 3.2L14.5 4M3 15.5h12",
};

const standardHandles = [
  { id: "left", position: Position.Left },
  { id: "right", position: Position.Right },
  { id: "top", position: Position.Top },
  { id: "bottom", position: Position.Bottom },
] as const;

const routeHandles = [
  { id: "left-top", position: Position.Left, style: { top: "32%" } },
  { id: "left-bottom", position: Position.Left, style: { top: "68%" } },
  { id: "right-top", position: Position.Right, style: { top: "32%" } },
  { id: "right-bottom", position: Position.Right, style: { top: "68%" } },
  { id: "top-left", position: Position.Top, style: { left: "32%" } },
  { id: "top-right", position: Position.Top, style: { left: "68%" } },
  { id: "bottom-left", position: Position.Bottom, style: { left: "32%" } },
  { id: "bottom-right", position: Position.Bottom, style: { left: "68%" } },
] as const;

watch(
  () => [props.data.expanded, props.id] as const,
  () => {
    void nextTick(() => updateNodeInternals([props.id]));
  },
  { immediate: true },
);

const toggleExpanded = () => {
  props.data.onToggleExpanded?.(props.id);
  emit("toggle-expanded", props.id);
};

const typeLabel = () => props.data.typeLabel ?? props.data.record.type;
const branchValues = () => variableBranchValues(props.data.record);
const iconPath = computed(() => iconPaths[props.data.record.type] ?? "M3 3h14v14H3z");
</script>

<template>
  <article
    :class="[
      'zen-node group relative flex h-full w-full flex-col items-center border bg-paper px-3 py-3 text-center text-ink transition',
      data.expanded ? 'justify-start' : 'justify-center',
      data.shape === 'circle' ? 'rounded-full' : 'rounded-[3px]',
      data.diffState === 'added'
        ? 'border-diff-added bg-diff-added-soft'
        : data.diffState === 'removed'
          ? 'border-diff-removed border-dashed bg-diff-removed-soft'
          : data.diffState === 'modified'
            ? 'border-diff-modified bg-diff-modified-soft'
            : selected
              ? 'border-blue shadow-[0_0_0_1px_#2457d6]'
              : data.highlighted
                ? 'chain-highlight-node'
                : data.record.tags.includes('disputed')
                  ? 'border-alert'
                  : 'border-ink/70 hover:border-ink',
      data.diffState === 'removed' ? 'pointer-events-none opacity-60' : '',
    ]"
    :aria-label="`${typeLabel()}: ${data.record.title}`"
  >
    <svg
      viewBox="0 0 20 20"
      width="20"
      height="20"
      fill="none"
      stroke="currentColor"
      stroke-width="1.35"
      stroke-linecap="round"
      stroke-linejoin="round"
      :class="selected ? 'mb-1 text-blue' : 'mb-1 text-ink/75'"
      aria-hidden="true"
    >
      <path :d="iconPath" />
    </svg>
    <h3 class="max-w-[13rem] font-serif text-[14px] leading-[1.18]">{{ data.record.title }}</h3>

    <p
      v-if="data.record.type !== 'question'"
      class="mt-2 font-serif text-[10px] leading-none text-ink/60"
    >
      {{ typeLabel() }} 路
      {{ typeof data.record.data.valueType === 'string' ? data.record.data.valueType : data.record.tags[0] || 'summary' }}
    </p>

    <button
      v-if="branchValues().length > 0"
      type="button"
      class="nodrag nopan absolute right-1.5 top-1.5 grid size-6 place-items-center rounded-full text-ink/45 transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-blue"
      :aria-label="data.expanded ? 'Collapse branches' : 'Expand branches'"
      :aria-expanded="data.expanded"
      @click.stop="toggleExpanded"
    >
      <span aria-hidden="true">{{ data.expanded ? '⌄' : '›' }}</span>
    </button>

    <div
      v-if="data.expanded && branchValues().length > 0"
      :class="[
        'nodrag nopan mt-3 w-full border-t border-ink/12 pt-3',
        data.record.data.valueType === 'bool' ? 'grid grid-cols-2 gap-2' : 'space-y-1.5',
      ]"
      aria-label="Variable values"
    >
      <div
        v-for="(value, index) in branchValues()"
        :key="`${value}-${index}`"
        :class="data.record.data.valueType === 'bool'
          ? 'relative border-t border-ink/30 pt-2 before:absolute before:-top-2 before:left-1/2 before:h-2 before:border-l before:border-ink/30'
          : 'relative flex items-center pl-4 before:absolute before:left-1 before:top-1/2 before:w-3 before:border-t before:border-ink/30 after:absolute after:bottom-1/2 after:left-1 after:top-[-7px] after:border-l after:border-ink/30'"
      >
        <span class="block w-full truncate rounded-[3px] border border-ink/15 bg-canvas px-2 py-1 font-sans text-[9px] text-ink/70">
          {{ value }}
        </span>
      </div>
    </div>

    <span
      v-if="data.diffState"
      :class="[
        'absolute -right-3 -top-3 grid size-6 place-items-center rounded-full border bg-paper font-serif text-[13px] font-bold shadow-sm',
        data.diffState === 'added'
          ? 'border-diff-added text-diff-added'
          : data.diffState === 'removed'
            ? 'border-diff-removed text-diff-removed'
            : 'border-diff-modified text-diff-modified',
      ]"
      aria-hidden="true"
    >
      {{ data.diffState === 'added' ? '+' : data.diffState === 'removed' ? '−' : '~' }}
    </span>

    <span
      v-if="data.record.tags.includes('disputed')"
      class="absolute -bottom-3 -right-3 grid size-6 place-items-center rounded-full border border-alert bg-paper font-serif text-[14px] text-alert"
      aria-hidden="true"
    >
      !
    </span>

    <Handle
      v-for="handle in standardHandles"
      :id="handle.id"
      :key="handle.id"
      class="zen-handle"
      type="source"
      :position="handle.position"
    />
    <Handle
      v-for="handle in routeHandles"
      :id="handle.id"
      :key="handle.id"
      class="zen-route-handle"
      type="source"
      :position="handle.position"
      :style="handle.style"
      :connectable="false"
    />
  </article>
</template>

<style scoped>
.zen-node {
  transform: translateZ(0);
}
</style>
