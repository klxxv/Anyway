<script setup lang="ts">
import { computed, nextTick, watch } from "vue";
import { Handle, Position, useVueFlow, type NodeProps } from "@vue-flow/core";
import type { ResearchNodeType } from "../../../app/lib/research-types";
import DynamicIcon from "../components/DynamicIcon.vue";
import {
  CHEVRON_DOWN,
  CHEVRON_RIGHT,
  defaultNodeIcon,
  nodeTypeIconPaths,
} from "../icons";
import type { WorkspaceNodeData } from "./canvas-types";
import { variableBranchValues } from "./variable-branches";

type ResearchNodeProps = NodeProps<WorkspaceNodeData, object, "researchNode">;

const props = defineProps<ResearchNodeProps>();
const emit = defineEmits<{
  (event: "toggle-expanded", nodeId: string): void;
}>();

const { updateNodeInternals } = useVueFlow();

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
const iconPath = computed(
  () => nodeTypeIconPaths[props.data.record.type] ?? defaultNodeIcon,
);
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
    <DynamicIcon
      :icon="iconPath"
      :size="20"
      :stroke-width="1.35"
      class="mb-1"
      :class="selected ? 'text-blue' : 'text-ink/75'"
    />
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
      <DynamicIcon
        :icon="data.expanded ? CHEVRON_DOWN : CHEVRON_RIGHT"
        :size="14"
        :stroke-width="1.6"
        aria-hidden="true"
      />
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
