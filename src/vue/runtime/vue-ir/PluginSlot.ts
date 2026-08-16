/**
 * PluginSlot: renders every plugin contribution bound to one host slot.
 *
 * A `.vue`-free `defineComponent` that uses `h()` directly, matching the
 * allowlist renderer. It deliberately does not import Pinia: state, action
 * dispatch, and the registry are passed in or injected.
 */
import {
  Comment,
  defineComponent,
  h,
  inject,
  type PropType,
  type VNodeChild,
} from "vue";
import {
  permissionPolicyForContributions,
  type UiIrDocument,
  type UiIrSlotContribution,
  type UiIrStateBinding,
} from "../../../../app/plugins/ui-ir";
import type { HostSdk } from "../../../../app/platform/host-sdk";
import { createUiIrActionDispatcher } from "../../../../app/platform/ui-ir-dispatch";
import { parseUiIR } from "./parser";
import { renderUiIR, type UiIrState } from "./renderer";
import {
  DEFAULT_SLOT_REGISTRY,
  mergeUiIrPermissionPolicies,
  uiIrSlotRegistryKey,
  type UiIrSlotRegistry,
} from "./slot-registry";

export type UiIrPluginContribution = {
  readonly pluginId: string;
  readonly ir: UiIrDocument;
};

export type PluginSlotProps = {
  readonly slotId: string;
  readonly contributions?: readonly UiIrPluginContribution[];
  readonly registry?: UiIrSlotRegistry;
  readonly hostSdk?: HostSdk;
  readonly state?: UiIrState;
  readonly setState?: (binding: UiIrStateBinding, value: string) => void;
};

function isEmptyVNodeChild(value: VNodeChild | undefined): boolean {
  return value === undefined || value === null || value === false || value === "";
}

function renderPluginSlot(
  props: PluginSlotProps,
  registry: UiIrSlotRegistry,
): VNodeChild {
  const descriptor = registry.get(props.slotId);
  if (!descriptor) return h(Comment, `ui-ir slot '${props.slotId}' is not registered`);

  const contributions = props.contributions ?? [];
  const byPlugin = new Map<string, UiIrPluginContribution[]>();
  for (const contribution of contributions) {
    const group = byPlugin.get(contribution.pluginId);
    if (group) group.push(contribution);
    else byPlugin.set(contribution.pluginId, [contribution]);
  }

  const children: VNodeChild[] = [];
  for (const contribution of contributions) {
    // The plugin allowlist covers every contribution the plugin declared in
    // this slot, so one contribution can never slip past the manifest's own
    // actions. Contributions carry no slotId: the slot is this component.
    const pluginDeclarations: readonly UiIrSlotContribution[] = (
      byPlugin.get(contribution.pluginId) ?? []
    ).map((declared) => ({ slotId: props.slotId, ir: declared.ir }));
    const permissions = mergeUiIrPermissionPolicies(
      descriptor.policy,
      permissionPolicyForContributions(pluginDeclarations),
    );
    let document: UiIrDocument;
    try {
      document = parseUiIR(contribution.ir, { permissions });
    } catch {
      // An invalid or disallowed contribution is skipped, never rendered.
      continue;
    }
    children.push(renderUiIR(document, {
      pluginId: contribution.pluginId,
      state: props.state,
      setState: props.setState,
      allowedSlots: [...registry.keys()],
      dispatchAction: props.hostSdk
        ? createUiIrActionDispatcher(props.hostSdk, contribution.pluginId, permissions)
        : undefined,
    }));
  }

  const native = descriptor.render?.();
  if (children.length === 0 && isEmptyVNodeChild(native)) {
    return h(Comment, `no UI IR contribution for slot '${props.slotId}'`);
  }
  return h(
    "div",
    { class: "ui-ir-slot-host", "data-ui-ir-slot": props.slotId },
    [...children, ...(isEmptyVNodeChild(native) ? [] : [native])],
  );
}

export const PluginSlot = defineComponent({
  name: "PluginSlot",
  props: {
    slotId: { type: String, required: true },
    contributions: {
      type: Array as PropType<readonly UiIrPluginContribution[]>,
      default: () => [],
    },
    registry: { type: Object as PropType<UiIrSlotRegistry>, required: false },
    hostSdk: { type: Object as PropType<HostSdk>, required: false },
    state: { type: Object as PropType<UiIrState>, required: false },
    setState: { type: Function as PropType<PluginSlotProps["setState"]>, required: false },
  },
  setup(props) {
    const ambient = inject(uiIrSlotRegistryKey, undefined);
    const registry = props.registry ?? ambient ?? DEFAULT_SLOT_REGISTRY;
    return () => renderPluginSlot(props, registry);
  },
});
