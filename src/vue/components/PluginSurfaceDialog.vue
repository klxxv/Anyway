<script setup lang="ts">
import { onUnmounted } from "vue";
import type { PluginSurfaceController } from "../../../app/plugins/plugin-surface-contract";
import PluginSurfaceSlot from "./PluginSurfaceSlot.vue";

const props = defineProps<{
  controller: PluginSurfaceController;
  surfaceIds: readonly string[];
  title: string;
  subtitle?: string;
  onClose: () => void;
}>();
onUnmounted(() => props.controller.dispose?.());
</script>

<template>
  <div class="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
    <section class="flex max-h-[min(760px,calc(100vh-32px))] w-[min(760px,calc(100vw-32px))] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]">
      <header class="flex items-start justify-between border-b border-ink/15 px-7 py-5">
        <div>
          <span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">Plugin surface</span>
          <h2 class="mt-1 font-serif text-[21px]">{{ props.title }}</h2>
          <p v-if="props.subtitle" class="mt-1 max-w-[560px] font-serif text-[10px] leading-[1.5] text-ink/50">{{ props.subtitle }}</p>
        </div>
        <button class="icon-quiet" aria-label="Close" @click="props.onClose">×</button>
      </header>
      <div class="min-h-0 flex-1 space-y-5 overflow-y-auto p-6">
        <PluginSurfaceSlot
          v-for="surfaceId in props.surfaceIds"
          :key="surfaceId"
          :slot-id="surfaceId"
          :plugin-id="props.controller.pluginId.value"
          :state="props.controller.state.value"
          :dispatch-action="props.controller.dispatchAction"
        />
      </div>
    </section>
  </div>
</template>
