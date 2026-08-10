<script setup lang="ts">
import { computed } from "vue";
import type { PluginStoreItemProps } from "./panel-types";

const props = defineProps<PluginStoreItemProps>();
const clickable = computed(() => Boolean(props.onOpenSettings));
const activate = () => props.onOpenSettings?.();
const onKeyDown = (event: KeyboardEvent) => {
  if (!clickable.value || event.target !== event.currentTarget) return;
  if (event.key === "Enter" || event.key === " ") {
    event.preventDefault();
    activate();
  }
};
</script>

<template>
  <article
    class="rounded-[5px] border border-ink/18 p-4"
    :class="clickable ? 'cursor-pointer transition-colors hover:border-blue/40 hover:bg-blue/[0.025] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40' : ''"
    :role="clickable ? 'button' : undefined"
    :tabindex="clickable ? 0 : undefined"
    :aria-label="clickable ? `Open settings for ${props.name}` : undefined"
    @click="clickable ? activate() : undefined"
    @keydown="onKeyDown"
  >
    <div class="flex items-start gap-3">
      <span class="shrink-0">
        <slot name="icon">
          <span v-if="props.icon !== undefined && props.icon !== false">{{ props.icon }}</span>
        </slot>
      </span>
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-2"><h4 class="font-serif text-[13px]">{{ props.name }}</h4><span class="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/40">{{ props.kind ? `${props.kind} · ` : '' }}{{ props.version }}</span></div>
        <p class="mt-1 font-serif text-[9px] leading-[1.45] text-ink/50">{{ props.description }}</p>
        <slot />
      </div>
      <div v-if="props.status || props.actions || $slots.status || $slots.actions" class="flex shrink-0 items-start gap-2" @click.stop @keydown.stop>
        <slot name="status">
          <span v-if="props.status !== undefined && props.status !== false">{{ props.status }}</span>
        </slot>
        <slot name="actions">
          <span v-if="props.actions !== undefined && props.actions !== false">{{ props.actions }}</span>
        </slot>
      </div>
    </div>
  </article>
</template>

<style scoped>
/* Shared plugin-item visual tokens remain in app/globals.css. */
</style>
