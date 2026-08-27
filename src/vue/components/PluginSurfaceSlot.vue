<script setup lang="ts">
import { ref, watch } from "vue";
import type { UiIrHydratedContribution } from "../../../app/plugins/ui-ir";
import { selectPluginSurfaceContributions } from "../../../app/plugins/plugin-surface-selection";
import { PluginSlot, type UiIrNativeSlotRenderers, type UiIrPluginContribution } from "../runtime/vue-ir/PluginSlot";
import type { UiIrActionDispatcher } from "../../../app/plugins/ui-ir";
import type { UiIrState } from "../runtime/vue-ir/renderer";
import { usePluginHost } from "../runtime/plugin-host";

const props = defineProps<{
  slotId: string;
  pluginId?: string | null;
  state?: UiIrState;
  dispatchAction?: UiIrActionDispatcher;
  nativeSlotRenderers?: UiIrNativeSlotRenderers;
}>();
const contributions = ref<UiIrPluginContribution[]>([]);
const pluginHost = usePluginHost();

async function load() {
  try {
    const plugins = pluginHost.activePlugins;
    const candidates = plugins.flatMap((plugin) => (plugin.uiIrContributions ?? []).map((contribution: UiIrHydratedContribution) => ({ pluginId: plugin.manifest.metadata.id, slotId: contribution.slotId, contribution })));
    const selected = props.pluginId ? selectPluginSurfaceContributions(candidates, props.pluginId, props.slotId) : [];
    contributions.value = selected.map((contribution) => ({ pluginId: props.pluginId!, ir: contribution.ir }));
  } catch {
    contributions.value = [];
  }
}
watch(() => [props.slotId, props.pluginId, pluginHost.activePlugins] as const, () => void load(), { immediate: true });
</script>

<template>
  <PluginSlot
    v-if="contributions.length"
    :slot-id="props.slotId"
    :contributions="contributions"
    :state="props.state"
    :dispatch-action="props.dispatchAction"
    :native-slot-renderers="props.nativeSlotRenderers"
  />
</template>
