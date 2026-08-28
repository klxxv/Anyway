<script setup lang="ts">
import {
  computed,
  defineComponent,
  h,
  onBeforeUnmount,
  onErrorCaptured,
  onMounted,
  ref,
  shallowRef,
  watch,
  type Component,
  type PropType,
} from "vue";
import type { PluginUiContributionRef, PluginUiUserOrder } from "../../../app/plugins/plugin-surface-selection";
import {
  activatePluginFrontendModule,
  deactivatePluginFrontendModule,
} from "../../../app/plugins/plugin-frontend-loader";
import type {
  MountedSlotInstance,
  PluginContext,
  PluginFrontendComponentProps,
  PluginFrontendModule,
} from "../../../app/plugins/plugin-frontend-contract";
import {
  createPluginFrontendContext,
  useHostSlotRegistry,
  usePluginContributions,
} from "../runtime/plugin-contributions";

const props = withDefaults(defineProps<{
  slotId: string;
  userOrder?: PluginUiUserOrder;
  context?: Readonly<Record<string, unknown>>;
}>(), {
  userOrder: () => ({}),
  context: () => ({}),
});

type LoadedContribution = {
  key: string;
  ref: PluginUiContributionRef;
  component: Component;
  context: PluginContext;
  instance?: MountedSlotInstance;
};

const registry = useHostSlotRegistry();
const slot = computed(() => registry.get(props.slotId));
const contributions = usePluginContributions(props.slotId, props.userOrder);
const loaded = shallowRef<LoadedContribution[]>([]);
const errors = ref<Record<string, string>>({});
let unregisterSlot: (() => void) | undefined;
let loadGeneration = 0;

const PluginContributionErrorBoundary = defineComponent({
  name: "PluginContributionErrorBoundary",
  props: {
    item: { type: Object as PropType<LoadedContribution>, required: true },
    hostContext: { type: Object as PropType<Readonly<Record<string, unknown>>>, required: false },
  },
  setup(boundaryProps) {
    const error = ref("");
    onErrorCaptured((captured) => {
      error.value = captured instanceof Error ? captured.message : String(captured);
      return false;
    });
    return () => error.value
      ? h("div", {
        class: "plugin-contribution-error",
        "data-plugin-id": boundaryProps.item.ref.pluginId,
        "data-contribution-id": boundaryProps.item.ref.contribution.id,
      }, error.value)
      : h(boundaryProps.item.component, {
        plugin: {
          id: boundaryProps.item.ref.pluginId,
          version: boundaryProps.item.ref.pluginVersion,
          name: boundaryProps.item.ref.plugin.manifest.metadata.name,
          installPath: boundaryProps.item.ref.plugin.installPath,
        },
        contribution: boundaryProps.item.ref.contribution,
        context: boundaryProps.item.context,
        slotId: boundaryProps.item.ref.slotId,
        instance: boundaryProps.item.instance,
        hostContext: boundaryProps.hostContext,
      } satisfies PluginFrontendComponentProps);
  },
});

function contributionKey(ref: PluginUiContributionRef): string {
  return `${ref.pluginId}@${ref.pluginVersion}:${ref.contribution.id}`;
}

function isVueComponent(value: unknown): value is Component {
  return typeof value === "object" && value !== null || typeof value === "function";
}

function componentExport(module: PluginFrontendModule, exportName: string): Component {
  const candidate = module[exportName] ?? (exportName === "default" ? module.default : undefined);
  if (!isVueComponent(candidate)) throw new Error(`PLUGIN_FRONTEND_EXPORT_MISSING:${exportName}`);
  return candidate;
}

async function deactivateAll(items: readonly LoadedContribution[]): Promise<void> {
  await Promise.all(items.map((item) => deactivatePluginFrontendModule(item.ref.plugin, item.context).catch(() => undefined)));
}

async function loadContributions(nextContributions: readonly PluginUiContributionRef[]) {
  const generation = ++loadGeneration;
  const nextLoaded: LoadedContribution[] = [];
  const nextErrors: Record<string, string> = {};
  const previousByKey = new Map(loaded.value.map((item) => [item.key, item]));

  for (const ref of nextContributions) {
    const key = contributionKey(ref);
    const previous = previousByKey.get(key);
    if (previous) {
      nextLoaded.push(previous);
      continue;
    }
    const context = createPluginFrontendContext(ref.plugin, registry);
    try {
      const module = await activatePluginFrontendModule(ref.plugin, context);
      if (generation !== loadGeneration) return;
      nextLoaded.push({
        key,
        ref,
        context,
        component: componentExport(module, ref.contribution.export),
        instance: registry.mounted(props.slotId)[0],
      });
    } catch (error) {
      nextErrors[key] = error instanceof Error ? error.message : String(error);
    }
  }

  if (generation !== loadGeneration) return;
  const previous = loaded.value;
  loaded.value = nextLoaded;
  errors.value = nextErrors;
  void deactivateAll(previous.filter((item) => !nextLoaded.some((next) => next.key === item.key)));
}

function registerMountedSlot() {
  unregisterSlot?.();
  unregisterSlot = registry.register({
    instanceId: `host:${props.slotId}`,
    slotId: props.slotId,
    context: { component: "PluginContributionSlot", ...props.context },
  });
}

onMounted(() => {
  registerMountedSlot();
});

onBeforeUnmount(() => {
  loadGeneration += 1;
  unregisterSlot?.();
  void deactivateAll(loaded.value);
});

watch(contributions, (next) => {
  if (!slot.value) {
    loaded.value = [];
    errors.value = {};
    return;
  }
  void loadContributions(next);
}, { immediate: true });

watch(() => props.context, () => {
  registerMountedSlot();
});
</script>

<template>
  <div
    v-if="slot && (loaded.length || Object.keys(errors).length)"
    class="plugin-contribution-slot"
    :data-host-slot="props.slotId"
  >
    <PluginContributionErrorBoundary
      v-for="item in loaded"
      :key="item.key"
      :item="item"
      :host-context="props.context"
    />
    <div
      v-for="(message, key) in errors"
      :key="key"
      class="plugin-contribution-error"
    >
      {{ message }}
    </div>
  </div>
</template>

<style scoped>
.plugin-contribution-slot {
  display: contents;
}

.plugin-contribution-error {
  max-width: 320px;
  border: 1px solid rgba(180, 45, 45, 0.32);
  border-radius: 6px;
  background: rgba(180, 45, 45, 0.08);
  padding: 0.5rem 0.65rem;
  font-size: 11px;
  line-height: 1.35;
  color: rgb(125, 32, 32);
}
</style>
