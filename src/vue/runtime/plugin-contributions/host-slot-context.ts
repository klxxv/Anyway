import {
  computed,
  inject,
  onBeforeUnmount,
  provide,
  shallowRef,
  type ComputedRef,
  type InjectionKey,
} from "vue";
import {
  DEFAULT_HOST_SLOT_CATALOG,
  hostSlotRegistry,
  type HostSlotRegistry,
  type HostSlotRegistrySnapshot,
} from "../../../../app/plugins/host-slot-registry";
import type { HostSlotDescriptor, SlotCatalog } from "../../../../app/plugins/plugin-frontend-contract";
import {
  selectPluginUiContributions,
  type PluginUiContributionRef,
  type PluginUiUserOrder,
} from "../../../../app/plugins/plugin-surface-selection";
import { usePluginHost } from "../plugin-host";

export const hostSlotRegistryKey: InjectionKey<HostSlotRegistry> = Symbol("anyway.host-slot-registry");

export function provideHostSlotRegistry(registry: HostSlotRegistry = hostSlotRegistry): HostSlotRegistry {
  provide(hostSlotRegistryKey, registry);
  return registry;
}

export function useHostSlotRegistry(): HostSlotRegistry {
  return inject(hostSlotRegistryKey, hostSlotRegistry);
}

export function useHostSlotCatalog(): SlotCatalog {
  return useHostSlotRegistry().catalog ?? DEFAULT_HOST_SLOT_CATALOG;
}

export function useMountedSlotInstances(): ComputedRef<HostSlotRegistrySnapshot["mounted"]> {
  const registry = useHostSlotRegistry();
  const snapshot = shallowRef(registry.snapshot());
  const unsubscribe = registry.subscribe((next) => {
    snapshot.value = next;
  });
  onBeforeUnmount(unsubscribe);
  return computed(() => snapshot.value.mounted);
}

export function usePluginContributions(
  slotId: string,
  userOrder: PluginUiUserOrder = {},
): ComputedRef<readonly PluginUiContributionRef[]> {
  const registry = useHostSlotRegistry();
  const pluginHost = usePluginHost();
  return computed(() => {
    const slot = registry.get(slotId);
    if (!slot) return [];
    return selectPluginUiContributions(pluginHost.activePlugins, slot, userOrder);
  });
}

export function hostSlotDescriptor(slotId: string): HostSlotDescriptor | undefined {
  return hostSlotRegistry.get(slotId);
}

