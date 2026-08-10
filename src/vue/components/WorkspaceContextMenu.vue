<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import {
  IconArrowsExchange,
  IconArrowsMaximize,
  IconArrowsMinimize,
  IconCopy,
  IconDatabase,
  IconFilter,
  IconFocus2,
  IconLayout,
  IconLink,
  IconNote,
  IconPlugConnected,
  IconSearch,
  IconSparkles,
  IconTrash,
  IconWand,
} from "@tabler/icons-vue";

import { useI18n } from "../runtime/i18n";
import {
  CONTEXT_MENU_ACTIONS,
  type ContextMenuActionId,
} from "../../../app/features/research-workspace/workspace-context-menu";
import type { ResolvedPluginContextMenuAction } from "../../../app/plugins/context-menu";
import type { WorkspaceShortcuts } from "../../../app/features/research-workspace/workspace-shortcuts";
import type {
  WorkspaceContextMenuEmits,
  WorkspaceContextMenuProps,
} from "./workspace-shell-types";

const props = defineProps<WorkspaceContextMenuProps>();
const emit = defineEmits<WorkspaceContextMenuEmits>();
const { t } = useI18n();
const menuRef = ref<HTMLElement | null>(null);

const builtInIcon = {
  inspect: IconFocus2,
  connect: IconPlugConnected,
  duplicate: IconCopy,
  delete: IconTrash,
  filter: IconFilter,
  reverse: IconArrowsExchange,
  add: IconSparkles,
  note: IconNote,
  layout: IconLayout,
  fit: IconArrowsMaximize,
  expand: IconArrowsMaximize,
  collapse: IconArrowsMinimize,
} as const;

const pluginIcon = {
  sparkles: IconSparkles,
  search: IconSearch,
  wand: IconWand,
  database: IconDatabase,
  link: IconLink,
} as const;

const actionShortcut: Partial<Record<ContextMenuActionId, keyof WorkspaceShortcuts>> = {
  "canvas.add": "add",
  "canvas.note": "note",
  "canvas.layout": "layout",
};

const actions = computed(() => {
  const definitions = [
    ...CONTEXT_MENU_ACTIONS.node,
    ...CONTEXT_MENU_ACTIONS.edge,
    ...CONTEXT_MENU_ACTIONS.canvas,
  ];
  return props.actionOrder.flatMap((id) => {
    const definition = definitions.find((item) => item.id === id);
    return definition ? [definition] : [];
  });
});

const position = computed(() => {
  const bottomPadding = Math.min(
    460,
    82 + (actions.value.length + props.pluginActions.length) * 38,
  );
  return {
    left: `${Math.max(10, Math.min(props.menu.screenX, props.width - 246))}px`,
    top: `${Math.max(10, Math.min(props.menu.screenY, props.height - bottomPadding))}px`,
  };
});

function closeOnPointer(event: PointerEvent) {
  const target = event.target as Node | null;
  if (!menuRef.value?.contains(target)) emit("close");
}

function closeOnKeyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    emit("close");
    return;
  }
  if (event.key !== "Delete") return;
  const deleteAction: ContextMenuActionId | null =
    props.menu.scope === "node"
      ? "node.delete"
      : props.menu.scope === "edge"
        ? "edge.delete"
        : null;
  if (deleteAction && props.actionOrder.includes(deleteAction)) {
    event.preventDefault();
    emit("built-in-action", deleteAction, props.menu);
  }
}

onMounted(() => {
  window.addEventListener("pointerdown", closeOnPointer, true);
  window.addEventListener("keydown", closeOnKeyboard);
});

onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", closeOnPointer, true);
  window.removeEventListener("keydown", closeOnKeyboard);
});

function pluginIconFor(action: ResolvedPluginContextMenuAction) {
  return pluginIcon[action.icon] ?? IconSparkles;
}
</script>

<template>
  <div
    ref="menuRef"
    class="absolute z-[70] w-[236px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper shadow-[0_14px_38px_rgba(31,34,38,.16)]"
    :style="position"
    role="menu"
    :aria-label="`${t(`contextMenu.${props.menu.scope}`)} context menu`"
    @contextmenu.prevent
  >
    <header class="border-b border-ink/12 px-3.5 py-2.5">
      <p class="font-sans text-[7px] uppercase tracking-[0.16em] text-blue">
        {{ t(`contextMenu.${props.menu.scope}`) }}
      </p>
      <p v-if="props.menu.title" class="mt-1 truncate font-serif text-[11px] text-ink/65">
        {{ props.menu.title }}
      </p>
    </header>

    <div class="p-1.5">
      <button
        v-for="action in actions"
        :key="action.id"
        :class="[
          'group flex min-h-9 w-full items-center gap-3 rounded-[4px] px-2.5 text-left font-serif text-[11px] transition focus-visible:outline-2 focus-visible:outline-blue',
          'danger' in action && action.danger
            ? 'text-alert hover:bg-alert/5'
            : 'text-ink/85 hover:bg-blue-soft hover:text-blue',
        ]"
        role="menuitem"
        @click="emit('built-in-action', action.id, props.menu)"
      >
        <component :is="builtInIcon[action.icon]" size="16" stroke="1.35" />
        <span class="min-w-0 flex-1">{{ t(action.labelKey) }}</span>
        <kbd
          v-if="actionShortcut[action.id] || ('danger' in action && action.danger)"
          class="font-sans text-[8px] font-medium text-ink/35 group-hover:text-blue/60"
        >
          {{ actionShortcut[action.id] ? props.shortcuts[actionShortcut[action.id]!] : "Del" }}
        </kbd>
      </button>
    </div>

    <div v-if="props.pluginActions.length > 0" class="border-t border-ink/12 p-1.5">
      <p class="px-2.5 pb-1 pt-1 font-sans text-[7px] uppercase tracking-[0.15em] text-ink/40">
        {{ t("contextMenu.pluginGroup") }}
      </p>
      <button
        v-for="action in props.pluginActions"
        :key="action.id"
        class="group flex min-h-9 w-full items-center gap-3 rounded-[4px] px-2.5 text-left font-serif text-[11px] text-ink/85 transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-blue"
        role="menuitem"
        :title="`${action.plugin.name} · ${t('contextMenu.pluginRun')}`"
        @click="emit('plugin-action', action, props.menu)"
      >
        <component :is="pluginIconFor(action)" size="16" stroke="1.35" />
        <span class="min-w-0 flex-1 truncate">{{ action.label }}</span>
        <span class="size-1.5 rounded-full bg-blue" aria-hidden="true" />
      </button>
    </div>
  </div>
</template>

<style scoped>
/* Shared context-menu visual tokens remain in app/globals.css. */
</style>
