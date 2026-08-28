<script setup lang="ts">
import { onBeforeUnmount, ref, type Component, type Ref } from "vue";
import {
  IconAdjustmentsHorizontal,
  IconArrowBackUp,
  IconArrowForwardUp,
  IconArrowUpRight,
  IconBinaryTree,
  IconChartHistogram,
  IconCheck,
  IconChevronDown,
  IconDatabase,
  IconFileText,
  IconFlag3,
  IconFlask2,
  IconGitBranch,
  IconGitCompare,
  IconHelp,
  IconHierarchy2,
  IconListTree,
  IconMenu2,
  IconNetwork,
  IconNote,
  IconRouteAltLeft,
  IconSearch,
  IconSparkles,
  IconTable,
  IconUsersGroup,
} from "@tabler/icons-vue";

import type { MessageKey } from "../../../app/i18n/catalog";
import { layoutOptions } from "../../../app/features/research-workspace/workspace-layout";
import { shortcutToAria } from "../../../app/features/research-workspace/workspace-shortcuts";
import {
  type LayoutMode,
  type ResearchEdgeType,
  type ResearchNodeType,
} from "../../../app/lib/research-types";
import { useI18n } from "../runtime/i18n";
import type {
  CommandDensity,
  HoverDelay,
} from "../../../app/features/research-workspace/workspace-preferences";
import type {
  WorkspaceShortcuts,
} from "../../../app/features/research-workspace/workspace-shortcuts";
import type {
  WorkspaceTopbarEmits,
  WorkspaceTopbarProps,
} from "./workspace-shell-types";

const props = withDefaults(defineProps<WorkspaceTopbarProps>(), {
  exportFormats: () => [],
  compareEnabled: false,
});
const emit = defineEmits<WorkspaceTopbarEmits>();
const { t } = useI18n();

type MenuOption<T extends string> = {
  value: T;
  labelKey: MessageKey;
  descriptionKey: MessageKey;
  icon: Component;
};

const nodeOptions: Array<MenuOption<ResearchNodeType>> = [
  { value: "question", labelKey: "node.question", descriptionKey: "node.questionDesc", icon: IconHelp },
  { value: "concept", labelKey: "node.group", descriptionKey: "node.groupDesc", icon: IconUsersGroup },
  { value: "variable", labelKey: "node.variable", descriptionKey: "node.variableDesc", icon: IconChartHistogram },
  { value: "method", labelKey: "node.method", descriptionKey: "node.methodDesc", icon: IconFlask2 },
  { value: "dataset", labelKey: "node.data", descriptionKey: "node.dataDesc", icon: IconDatabase },
  { value: "evidence", labelKey: "node.evidence", descriptionKey: "node.evidenceDesc", icon: IconFileText },
  { value: "result", labelKey: "node.result", descriptionKey: "node.resultDesc", icon: IconCheck },
  { value: "note", labelKey: "node.note", descriptionKey: "node.noteDesc", icon: IconNote },
];

const connectionOptions: Array<MenuOption<ResearchEdgeType>> = [
  { value: "T", labelKey: "edgeType.transform", descriptionKey: "edgeType.transformDesc", icon: IconArrowUpRight },
  { value: "K", labelKey: "edgeType.kernel", descriptionKey: "edgeType.kernelDesc", icon: IconGitBranch },
  { value: "I", labelKey: "edgeType.intervention", descriptionKey: "edgeType.interventionDesc", icon: IconAdjustmentsHorizontal },
  { value: "M", labelKey: "edgeType.marginalize", descriptionKey: "edgeType.marginalizeDesc", icon: IconHierarchy2 },
  { value: "Q", labelKey: "edgeType.quotient", descriptionKey: "edgeType.quotientDesc", icon: IconNetwork },
];

const layoutMessageKeys: Record<LayoutMode, [MessageKey, MessageKey]> = {
  "evidence-chain": ["layout.evidenceChain", "layout.evidenceChainDesc"],
  "refutation-chain": ["layout.refutationChain", "layout.refutationChainDesc"],
  tree: ["layout.tree", "layout.treeDesc"],
  huffman: ["layout.huffman", "layout.huffmanDesc"],
  table: ["layout.table", "layout.tableDesc"],
  "neural-network": ["layout.neural", "layout.neuralDesc"],
};

const layoutIcons: Record<LayoutMode, Component> = {
  "evidence-chain": IconListTree,
  "refutation-chain": IconRouteAltLeft,
  tree: IconBinaryTree,
  huffman: IconHierarchy2,
  table: IconTable,
  "neural-network": IconNetwork,
};

type Disclosure = {
  open: Ref<boolean>;
  openSoon: () => void;
  closeSoon: () => void;
  clearTimer: () => void;
};

function useHoverDisclosure(delay: () => HoverDelay): Disclosure {
  const open = ref(false);
  let timer: number | null = null;

  const clearTimer = () => {
    if (timer !== null) window.clearTimeout(timer);
    timer = null;
  };
  const openSoon = () => {
    clearTimer();
    timer = window.setTimeout(() => {
      open.value = true;
    }, delay());
  };
  const closeSoon = () => {
    clearTimer();
    timer = window.setTimeout(() => {
      open.value = false;
    }, 120);
  };

  onBeforeUnmount(clearTimer);
  return { open, openSoon, closeSoon, clearTimer };
}

const addDisclosure = useHoverDisclosure(() => props.hoverDelay);
const connectDisclosure = useHoverDisclosure(() => props.hoverDelay);
const layoutDisclosure = useHoverDisclosure(() => props.hoverDelay);
const exportDisclosure = useHoverDisclosure(() => props.hoverDelay);
const addOpen = addDisclosure.open;
const connectOpen = connectDisclosure.open;
const layoutOpen = layoutDisclosure.open;
const exportOpen = exportDisclosure.open;

function commandClass(density: CommandDensity) {
  return `group relative inline-flex h-12 items-center gap-2 border-r border-ink/10 font-serif text-[15px] text-ink transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-blue ${
    density === "compact" ? "px-3.5" : "px-5"
  }`;
}

function handleFocus(disclosure: Disclosure) {
  disclosure.clearTimer();
  disclosure.open.value = true;
}

function handleBlur(disclosure: Disclosure, event: FocusEvent) {
  const currentTarget = event.currentTarget as HTMLElement | null;
  const relatedTarget = event.relatedTarget as Node | null;
  if (!currentTarget?.contains(relatedTarget)) disclosure.open.value = false;
}

function closeAfter(action: () => void, disclosure: Disclosure) {
  action();
  disclosure.open.value = false;
}

function onAddPrimary() {
  closeAfter(() => emit("add"), addDisclosure);
}

function onAddType(type: ResearchNodeType) {
  closeAfter(() => emit("add-type", type), addDisclosure);
}

function onConnectPrimary() {
  closeAfter(() => emit("connect"), connectDisclosure);
}

function onConnectType(type: ResearchEdgeType) {
  closeAfter(() => emit("connect-type", type), connectDisclosure);
}

function onLayout(mode: LayoutMode) {
  closeAfter(() => emit("layout", mode), layoutDisclosure);
}

function onExportPrimary() {
  closeAfter(() => emit("export"), exportDisclosure);
}

function onExportFormat(format: "pdf" | "svg" | "png") {
  closeAfter(() => emit("export-format", format), exportDisclosure);
}

function hasPluginFormats() {
  return props.exportFormats.length > 0;
}
</script>

<template>
  <header class="flex h-12 shrink-0 items-center justify-between border-b border-ink/15 bg-paper">
    <nav class="flex h-full items-stretch" aria-label="Workspace commands">
      <button
        :class="commandClass(props.commandDensity)"
        @click="emit('menu')"
        :aria-keyshortcuts="shortcutToAria(props.shortcuts.menu)"
      >
        <IconMenu2 size="19" stroke="1.45" />
        {{ t("workspace.menu") }}
        <span
          v-if="props.shortcuts.menu"
          class="pointer-events-none absolute left-1/2 top-[52px] z-[100] -translate-x-1/2 whitespace-nowrap rounded-[4px] border border-ink/20 bg-ink px-2 py-1 font-sans text-[9px] font-medium tracking-wide text-paper opacity-0 shadow-sm transition-opacity delay-150 group-hover:opacity-100 group-focus-visible:opacity-100"
          role="tooltip"
        >{{ props.shortcuts.menu }}</span>
      </button>

      <div
        class="relative"
        @mouseenter="addDisclosure.openSoon"
        @mouseleave="addDisclosure.closeSoon"
        @focusin="handleFocus(addDisclosure)"
        @focusout="handleBlur(addDisclosure, $event)"
      >
        <button
          :class="commandClass(props.commandDensity)"
          @click="onAddPrimary"
          :aria-expanded="addOpen"
          aria-haspopup="menu"
          :aria-keyshortcuts="shortcutToAria(props.shortcuts.add)"
        >
          <IconSparkles size="19" stroke="1.4" />
          {{ t("workspace.add") }}
          <IconChevronDown size="13" stroke="1.35" class="ml-0.5 transition-transform" :class="{ 'rotate-180': addOpen }" />
        </button>
        <div
          v-if="addOpen"
          class="absolute left-2 top-[46px] z-[90] w-[286px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]"
          role="menu"
          :aria-label="`${t('workspace.add')} options`"
        >
          <div class="flex items-center justify-between px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            <span>{{ t("workspace.createObject") }}</span>
            <kbd v-if="props.shortcuts.add" class="shortcut-key">{{ props.shortcuts.add }}</kbd>
          </div>
          <button
            v-for="option in nodeOptions"
            :key="option.value"
            class="flex w-full items-start gap-3 rounded-[4px] px-3 py-2 text-left transition hover:bg-ink/5"
            role="menuitemradio"
            aria-checked="false"
            @click="onAddType(option.value)"
          >
            <component :is="option.icon" class="mt-0.5 shrink-0" size="17" stroke="1.35" />
            <span class="min-w-0">
              <span class="block font-serif text-[12px]">{{ t(option.labelKey) }}</span>
              <span class="mt-0.5 block font-serif text-[9px] leading-[1.35] text-ink/50">{{ t(option.descriptionKey) }}</span>
            </span>
          </button>
        </div>
      </div>

      <div
        class="relative"
        @mouseenter="connectDisclosure.openSoon"
        @mouseleave="connectDisclosure.closeSoon"
        @focusin="handleFocus(connectDisclosure)"
        @focusout="handleBlur(connectDisclosure, $event)"
      >
        <button
          :class="[commandClass(props.commandDensity), props.connectMode ? 'bg-blue-soft text-blue' : '']"
          @click="onConnectPrimary"
          :aria-expanded="connectOpen"
          aria-haspopup="menu"
          :aria-keyshortcuts="shortcutToAria(props.shortcuts.connect)"
        >
          <IconArrowUpRight size="19" stroke="1.4" />
          {{ t("workspace.connect") }}
          <IconChevronDown size="13" stroke="1.35" class="ml-0.5 transition-transform" :class="{ 'rotate-180': connectOpen }" />
        </button>
        <div
          v-if="connectOpen"
          class="absolute left-2 top-[46px] z-[90] w-[286px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]"
          role="menu"
          :aria-label="`${t('workspace.connect')} options`"
        >
          <div class="flex items-center justify-between px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            <span>{{ t("workspace.chooseRelation") }}</span>
            <kbd v-if="props.shortcuts.connect" class="shortcut-key">{{ props.shortcuts.connect }}</kbd>
          </div>
          <button
            v-for="option in connectionOptions"
            :key="option.value"
            :class="['flex w-full items-start gap-3 rounded-[4px] px-3 py-2 text-left transition', props.connectType === option.value ? 'bg-blue-soft text-blue' : 'hover:bg-ink/5']"
            role="menuitemradio"
            :aria-checked="props.connectType === option.value"
            @click="onConnectType(option.value)"
          >
            <component :is="option.icon" class="mt-0.5 shrink-0" size="17" stroke="1.35" />
            <span class="min-w-0">
              <span class="block font-serif text-[12px]">{{ t(option.labelKey) }}</span>
              <span class="mt-0.5 block font-serif text-[9px] leading-[1.35] text-ink/50">{{ t(option.descriptionKey) }}</span>
            </span>
          </button>
        </div>
      </div>

      <button
        :class="commandClass(props.commandDensity)"
        @click="emit('note')"
        :aria-keyshortcuts="shortcutToAria(props.shortcuts.note)"
      >
        <IconNote size="19" stroke="1.45" />
        {{ t("workspace.note") }}
        <span v-if="props.shortcuts.note" class="pointer-events-none absolute left-1/2 top-[52px] z-[100] -translate-x-1/2 whitespace-nowrap rounded-[4px] border border-ink/20 bg-ink px-2 py-1 font-sans text-[9px] font-medium tracking-wide text-paper opacity-0 shadow-sm transition-opacity delay-150 group-hover:opacity-100 group-focus-visible:opacity-100" role="tooltip">{{ props.shortcuts.note }}</span>
      </button>
      <button
        :class="commandClass(props.commandDensity)"
        @click="emit('find')"
        :aria-keyshortcuts="shortcutToAria(props.shortcuts.find)"
      >
        <IconSearch size="19" stroke="1.45" />
        {{ t("workspace.find") }}
        <span v-if="props.shortcuts.find" class="pointer-events-none absolute left-1/2 top-[52px] z-[100] -translate-x-1/2 whitespace-nowrap rounded-[4px] border border-ink/20 bg-ink px-2 py-1 font-sans text-[9px] font-medium tracking-wide text-paper opacity-0 shadow-sm transition-opacity delay-150 group-hover:opacity-100 group-focus-visible:opacity-100" role="tooltip">{{ props.shortcuts.find }}</span>
      </button>

      <div
        class="relative"
        @mouseenter="layoutDisclosure.openSoon"
        @mouseleave="layoutDisclosure.closeSoon"
        @focusin="handleFocus(layoutDisclosure)"
        @focusout="handleBlur(layoutDisclosure, $event)"
      >
        <button
          :class="commandClass(props.commandDensity)"
          @click="layoutOpen = !layoutOpen"
          :aria-expanded="layoutOpen"
          aria-haspopup="menu"
          :aria-keyshortcuts="shortcutToAria(props.shortcuts.layout)"
        >
          <IconHierarchy2 size="19" stroke="1.35" />
          {{ t("workspace.layout") }}
          <IconChevronDown size="14" stroke="1.35" class="transition-transform" :class="{ 'rotate-180': layoutOpen }" />
        </button>
        <div v-if="layoutOpen" class="absolute left-2 top-[46px] z-[90] w-[286px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]" role="menu" aria-label="Layout mode">
          <div class="flex items-center justify-between px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            <span>{{ t("workspace.arrangeResearch") }}</span>
            <kbd v-if="props.shortcuts.layout" class="shortcut-key">{{ props.shortcuts.layout }}</kbd>
          </div>
          <button
            v-for="option in layoutOptions"
            :key="option.mode"
            :class="['flex w-full items-start gap-3 rounded-[4px] px-3 py-2 text-left transition', props.activeLayout === option.mode ? 'bg-blue-soft text-blue' : 'hover:bg-ink/5']"
            role="menuitemradio"
            :aria-checked="props.activeLayout === option.mode"
            @click="onLayout(option.mode)"
          >
            <component :is="layoutIcons[option.mode]" class="mt-0.5 shrink-0" size="17" stroke="1.35" />
            <span class="min-w-0 flex-1">
              <span class="block font-serif text-[12px]">{{ t(layoutMessageKeys[option.mode][0]) }}</span>
              <span class="mt-0.5 block font-serif text-[9px] leading-[1.35] text-ink/50">{{ t(layoutMessageKeys[option.mode][1]) }}</span>
            </span>
            <IconCheck v-if="props.activeLayout === option.mode" class="mt-0.5" size="14" stroke="1.6" />
          </button>
        </div>
      </div>

      <button v-if="props.compareEnabled" :class="commandClass(props.commandDensity)" @click="emit('compare')" :aria-label="t('diff.compare')">
        <IconGitCompare size="19" stroke="1.4" />
        {{ t("diff.compare") }}
      </button>
    </nav>

    <nav class="flex h-full items-stretch" aria-label="History and export">
      <slot name="actions" />
      <button :class="commandClass(props.commandDensity)" @click="emit('undo')" :disabled="!props.canUndo" aria-label="Undo" :aria-keyshortcuts="shortcutToAria(props.shortcuts.undo)">
        <IconArrowBackUp size="19" stroke="1.45" />
        {{ t("workspace.undo") }}
        <span v-if="props.shortcuts.undo" class="pointer-events-none absolute left-1/2 top-[52px] z-[100] -translate-x-1/2 whitespace-nowrap rounded-[4px] border border-ink/20 bg-ink px-2 py-1 font-sans text-[9px] font-medium tracking-wide text-paper opacity-0 shadow-sm transition-opacity delay-150 group-hover:opacity-100 group-focus-visible:opacity-100" role="tooltip">{{ props.shortcuts.undo }}</span>
      </button>
      <button :class="commandClass(props.commandDensity)" @click="emit('redo')" :disabled="!props.canRedo" aria-label="Redo" :aria-keyshortcuts="shortcutToAria(props.shortcuts.redo)">
        <IconArrowForwardUp size="19" stroke="1.45" />
        {{ t("workspace.redo") }}
        <span v-if="props.shortcuts.redo" class="pointer-events-none absolute left-1/2 top-[52px] z-[100] -translate-x-1/2 whitespace-nowrap rounded-[4px] border border-ink/20 bg-ink px-2 py-1 font-sans text-[9px] font-medium tracking-wide text-paper opacity-0 shadow-sm transition-opacity delay-150 group-hover:opacity-100 group-focus-visible:opacity-100" role="tooltip">{{ props.shortcuts.redo }}</span>
      </button>

      <div
        class="relative"
        @mouseenter="hasPluginFormats() && exportDisclosure.openSoon()"
        @mouseleave="hasPluginFormats() && exportDisclosure.closeSoon()"
        @focusin="hasPluginFormats() && handleFocus(exportDisclosure)"
        @focusout="handleBlur(exportDisclosure, $event)"
      >
        <button :class="[commandClass(props.commandDensity), 'border-l border-r-0']" @click="onExportPrimary" :aria-keyshortcuts="shortcutToAria(props.shortcuts.export)" :aria-haspopup="hasPluginFormats() ? 'menu' : undefined" :aria-expanded="hasPluginFormats() ? exportOpen : undefined">
          <IconFlag3 size="19" stroke="1.4" />
          {{ t("workspace.export") }}
          <IconChevronDown v-if="hasPluginFormats()" size="13" stroke="1.35" />
          <span v-if="props.shortcuts.export" class="pointer-events-none absolute left-1/2 top-[52px] z-[100] -translate-x-1/2 whitespace-nowrap rounded-[4px] border border-ink/20 bg-ink px-2 py-1 font-sans text-[9px] font-medium tracking-wide text-paper opacity-0 shadow-sm transition-opacity delay-150 group-hover:opacity-100 group-focus-visible:opacity-100" role="tooltip">{{ props.shortcuts.export }}</span>
        </button>
        <div v-if="hasPluginFormats() && exportOpen" class="absolute right-2 top-[46px] z-[90] w-[220px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]" role="menu" :aria-label="t('workspace.export')">
          <div class="flex items-center justify-between px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            <span>{{ t("workspace.export") }}</span>
            <kbd v-if="props.shortcuts.export" class="shortcut-key">{{ props.shortcuts.export }}</kbd>
          </div>
          <button v-for="format in props.exportFormats" :key="format" class="flex w-full items-center gap-3 rounded-[4px] px-3 py-2 text-left transition hover:bg-blue-soft hover:text-blue" role="menuitem" @click="onExportFormat(format)">
            <IconFileText size="17" stroke="1.35" />
            <span class="font-serif text-[12px]">{{ format.toUpperCase() }}</span>
          </button>
        </div>
      </div>
    </nav>
  </header>
</template>

<style scoped>
/* Shared topbar visual tokens remain in app/globals.css. */
</style>
