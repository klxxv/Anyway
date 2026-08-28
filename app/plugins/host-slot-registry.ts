import type {
  HostSlotDescriptor,
  MountedSlotInstance,
  SlotCatalog,
} from "./plugin-frontend-contract";

export const DEFAULT_HOST_SLOT_CATALOG: SlotCatalog = Object.freeze([
  {
    id: "workspace.toolbar.actions",
    label: "Workspace toolbar actions",
    region: "toolbar",
    cardinality: "multiple",
    order: 100,
    accepts: ["trusted-module"],
  },
  {
    id: "workspace.dialogs",
    label: "Workspace dialogs",
    region: "dialog",
    cardinality: "multiple",
    order: 200,
    accepts: ["trusted-module"],
  },
  {
    id: "workspace.status",
    label: "Workspace status",
    region: "status",
    cardinality: "multiple",
    order: 300,
    accepts: ["trusted-module", "declarative-ui"],
  },
  {
    id: "compat.declarative.surface",
    label: "Legacy declarative UI surface",
    region: "compat",
    cardinality: "multiple",
    order: 900,
    accepts: ["declarative-ui"],
  },
] satisfies HostSlotDescriptor[]);

export type HostSlotRegistrySnapshot = {
  readonly catalog: SlotCatalog;
  readonly mounted: readonly MountedSlotInstance[];
};

export type HostSlotRegistryListener = (snapshot: HostSlotRegistrySnapshot) => void;

export interface HostSlotRegistry {
  readonly catalog: SlotCatalog;
  get(slotId: string): HostSlotDescriptor | undefined;
  has(slotId: string): boolean;
  mounted(slotId?: string): readonly MountedSlotInstance[];
  register(instance: Omit<MountedSlotInstance, "mountedAt" | "owner"> & { readonly mountedAt?: number }): () => void;
  subscribe(listener: HostSlotRegistryListener): () => void;
  snapshot(): HostSlotRegistrySnapshot;
}

function sortCatalog(catalog: SlotCatalog): SlotCatalog {
  return Object.freeze([...catalog].sort((a, b) => a.order - b.order || a.id.localeCompare(b.id)));
}

export function createHostSlotRegistry(catalog: SlotCatalog = DEFAULT_HOST_SLOT_CATALOG): HostSlotRegistry {
  const normalizedCatalog = sortCatalog(catalog);
  const descriptors = new Map(normalizedCatalog.map((slot) => [slot.id, slot]));
  const mounted = new Map<string, MountedSlotInstance>();
  const listeners = new Set<HostSlotRegistryListener>();

  const snapshot = (): HostSlotRegistrySnapshot => Object.freeze({
    catalog: normalizedCatalog,
    mounted: Object.freeze([...mounted.values()].sort((a, b) => a.mountedAt - b.mountedAt || a.instanceId.localeCompare(b.instanceId))),
  });
  const emit = () => {
    const current = snapshot();
    for (const listener of listeners) listener(current);
  };

  return {
    catalog: normalizedCatalog,
    get(slotId) {
      return descriptors.get(slotId);
    },
    has(slotId) {
      return descriptors.has(slotId);
    },
    mounted(slotId) {
      const instances = [...mounted.values()].filter((instance) => !slotId || instance.slotId === slotId);
      return Object.freeze(instances.sort((a, b) => a.mountedAt - b.mountedAt || a.instanceId.localeCompare(b.instanceId)));
    },
    register(instance) {
      if (!descriptors.has(instance.slotId)) return () => undefined;
      const mountedInstance: MountedSlotInstance = Object.freeze({
        instanceId: instance.instanceId,
        slotId: instance.slotId,
        owner: "host",
        mountedAt: instance.mountedAt ?? Date.now(),
        context: instance.context ? Object.freeze({ ...instance.context }) : undefined,
      });
      mounted.set(instance.instanceId, mountedInstance);
      emit();
      return () => {
        if (!mounted.delete(instance.instanceId)) return;
        emit();
      };
    },
    subscribe(listener) {
      listeners.add(listener);
      listener(snapshot());
      return () => {
        listeners.delete(listener);
      };
    },
    snapshot,
  };
}

export const hostSlotRegistry = createHostSlotRegistry();

