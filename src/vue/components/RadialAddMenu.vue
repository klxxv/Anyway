<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  IconChartHistogram,
  IconCheck,
  IconDatabase,
  IconFileText,
  IconFlask2,
  IconHelp,
  IconNote,
  IconPlus,
  IconUsersGroup,
} from "@tabler/icons-vue";

import type { MessageKey } from "../../../app/i18n/catalog";
import { useI18n } from "../runtime/i18n";
import type { ResearchNodeType } from "../../../app/lib/research-types";
import type {
  RadialMenuAction,
  RadialMenuItem,
} from "../../../app/features/research-workspace/workspace-radial-menu";
import type {
  PieMenuState,
  RadialAddMenuEmits,
  RadialAddMenuProps,
} from "./workspace-shell-types";

const props = defineProps<RadialAddMenuProps>();
const emit = defineEmits<RadialAddMenuEmits>();
const { t } = useI18n();

const icons = {
  question: IconHelp,
  concept: IconUsersGroup,
  variable: IconChartHistogram,
  method: IconFlask2,
  dataset: IconDatabase,
  evidence: IconFileText,
  result: IconCheck,
  note: IconNote,
} as const;

const labelKeys: Partial<Record<ResearchNodeType, MessageKey>> = {
  question: "node.question",
  concept: "node.group",
  variable: "node.variable",
  method: "node.method",
  dataset: "node.data",
  evidence: "node.evidence",
  result: "node.result",
  note: "node.note",
};

const activeSector = ref<number | null>(null);
const gestureActive = ref(Boolean(props.menu.gestureActive));

watch(
  () => props.menu.gestureActive,
  (active) => {
    gestureActive.value = Boolean(active);
  },
);

function updateGesture(sector: number | null, active: boolean) {
  activeSector.value = sector;
  gestureActive.value = active;
}

defineExpose({ updateGesture });

function actionLabelKey(
  action: RadialMenuAction,
  nodeType: ResearchNodeType | null,
): MessageKey {
  if (nodeType) return labelKeys[nodeType] ?? "node.note";
  return action === "canvas:fit" ? "contextMenu.fitView" : "contextMenu.applyLayout";
}

function selectionArcPath(index: number) {
  const point = (angle: number) => {
    const radians = ((angle - 90) * Math.PI) / 180;
    return {
      x: 138 + 136 * Math.cos(radians),
      y: 138 + 136 * Math.sin(radians),
    };
  };
  const start = point(index * 45 - 19.5);
  const end = point(index * 45 + 19.5);
  return `M ${start.x} ${start.y} A 136 136 0 0 1 ${end.x} ${end.y}`;
}

const selectionArcPaths = Array.from({ length: 8 }, (_, index) => selectionArcPath(index));
const menuStyle = computed(() => ({
  left: `${props.menu.screenX}px`,
  top: `${props.menu.screenY}px`,
}));

function itemIcon(item: RadialMenuItem, nodeType: ResearchNodeType | null) {
  if (nodeType) return icons[nodeType as keyof typeof icons] ?? IconNote;
  return item.action === "canvas:fit" ? IconCheck : IconChartHistogram;
}
</script>

<template>
  <div
    class="zen-pie-menu"
    :class="{ 'is-gesture-active': gestureActive }"
    :style="menuStyle"
    role="menu"
    :aria-label="t('gesture.quickAdd')"
  >
    <div class="zen-pie-spokes" aria-hidden="true">
      <span
        v-for="index in 8"
        :key="index"
        class="zen-pie-spoke"
        :style="{ transform: `rotate(${(index - 1) * 45 + 22.5}deg)` }"
      />
    </div>
    <svg v-if="activeSector !== null" class="zen-pie-selection-arc" viewBox="0 0 276 276" aria-hidden="true">
      <path :d="selectionArcPaths[activeSector]" />
    </svg>
    <button
      v-for="{ item, nodeType, sectorIndex } in props.cache.items"
      :key="item.id"
      :class="[`zen-pie-item zen-pie-item-${sectorIndex}`, { 'is-active': activeSector === sectorIndex }]"
      @click="emit('choose', item)"
      role="menuitem"
      :aria-current="activeSector === sectorIndex ? 'true' : undefined"
    >
      <component :is="itemIcon(item, nodeType)" size="18" stroke="1.35" />
      <span>{{ t(actionLabelKey(item.action, nodeType)) }}</span>
    </button>
    <button class="zen-pie-center" @click="emit('close')" :aria-label="t('gesture.close')">
      <IconPlus size="24" stroke="1.35" />
      <span>{{ t("workspace.add") }}</span>
    </button>
  </div>
</template>

<style scoped>
/* Shared radial-menu visual tokens remain in app/globals.css. */
</style>
