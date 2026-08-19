/**
 * Fixed host slot registry: known slot ids, their host permission policy,
 * and an optional native renderer. Native slot content is always a VNode
 * factory — never a raw HTML string.
 */
import { h, inject, provide, type InjectionKey, type VNodeChild } from "vue";
import type { UiIrPermissionPolicy } from "../../../../app/plugins/ui-ir";

export type UiIrSlotRenderer = () => VNodeChild;

export type UiIrSlotDescriptor = {
  readonly slotId: string;
  readonly policy: UiIrPermissionPolicy;
  readonly render?: UiIrSlotRenderer;
};

export type UiIrSlotRegistry = ReadonlyMap<string, UiIrSlotDescriptor>;

export const uiIrSlotRegistryKey: InjectionKey<UiIrSlotRegistry> = Symbol("anyway.ui-ir.slot-registry");

export function provideUiIrSlotRegistry(registry: UiIrSlotRegistry): UiIrSlotRegistry {
  provide(uiIrSlotRegistryKey, registry);
  return registry;
}

export function useUiIrSlotRegistry(): UiIrSlotRegistry {
  const registry = inject(uiIrSlotRegistryKey, undefined);
  if (!registry) throw new Error("useUiIrSlotRegistry must be used inside a slot registry provider");
  return registry;
}

function addAll(
  target: Set<string>,
  source: ReadonlySet<string> | readonly string[] | undefined,
): void {
  if (!source) return;
  for (const item of source) target.add(item);
}

/**
 * Merges several permission policies into one: allowlists are unioned so a
 * plugin can never rely on another policy's declarations, and the pair map
 * keeps every (actionId, capability) combination the plugin declared. The
 * allowlist requirement stays on unless a policy explicitly opts out.
 */
export function mergeUiIrPermissionPolicies(
  ...policies: readonly UiIrPermissionPolicy[]
): UiIrPermissionPolicy {
  const allowedActions = new Set<string>();
  const allowedCapabilities = new Set<string>();
  const allowedActionCapabilities = new Map<string, Set<string>>();
  for (const policy of policies) {
    addAll(allowedActions, policy.allowedActions);
    addAll(allowedCapabilities, policy.allowedCapabilities);
    const pairs = policy.allowedActionCapabilities;
    if (pairs) {
      for (const [actionId, capabilities] of pairs) {
        const target = allowedActionCapabilities.get(actionId) ?? new Set<string>();
        addAll(target, capabilities);
        allowedActionCapabilities.set(actionId, target);
      }
    }
  }
  return {
    allowedActions,
    allowedCapabilities,
    allowedActionCapabilities,
    requireActionAllowlist: !policies.some((policy) => policy.requireActionAllowlist === false),
  };
}

const nativeSlotRenderer = (slotId: string): UiIrSlotRenderer => () =>
  h("div", { class: "ui-ir-native", "data-ui-ir-native": slotId }, `native:${slotId}`);

/** The fixed host registry shipped with the renderer. */
export const DEFAULT_SLOT_REGISTRY: UiIrSlotRegistry = new Map<string, UiIrSlotDescriptor>([
  ["node-inspector", {
    slotId: "node-inspector",
    policy: {
      allowedActions: ["node.inspect"],
      allowedCapabilities: ["analysis.run"],
      requireActionAllowlist: true,
    },
    render: nativeSlotRenderer("node-inspector"),
  }],
  ["canvas-toolbar", {
    slotId: "canvas-toolbar",
    policy: {
      allowedActions: ["canvas.zoom"],
      allowedCapabilities: ["canvas.view"],
      requireActionAllowlist: true,
    },
    render: nativeSlotRenderer("canvas-toolbar"),
  }],
  ["settings-panel", {
    slotId: "settings-panel",
    policy: {
      allowedActions: ["settings.save"],
      allowedCapabilities: ["settings.write"],
      requireActionAllowlist: true,
    },
    render: nativeSlotRenderer("settings-panel"),
  }],
  ["activity-sidebar", {
    slotId: "activity-sidebar",
    policy: {
      allowedActions: ["sidebar.open", "sidebar.select"],
      allowedCapabilities: ["project.folder", "git.repository.read"],
      requireActionAllowlist: true,
    },
    render: nativeSlotRenderer("activity-sidebar"),
  }],
  ["results-panel", {
    slotId: "results-panel",
    policy: {
      allowedActions: ["results.run", "results.inspect"],
      allowedCapabilities: ["analysis.run", "graph.validate", "chain.score", "run.manifest", "run.result"],
      requireActionAllowlist: true,
    },
    render: nativeSlotRenderer("results-panel"),
  }],
  ["agent-review-panel", {
    slotId: "agent-review-panel",
    policy: {
      allowedActions: ["agent-review.accept", "agent-review.reject"],
      allowedCapabilities: ["agent.review.request", "agent.graph.patch.propose"],
      requireActionAllowlist: true,
    },
    render: nativeSlotRenderer("agent-review-panel"),
  }],
  ["status-bar", {
    slotId: "status-bar",
    policy: {
      allowedActions: ["status.refresh"],
      allowedCapabilities: ["git.autosave", "git.account.read"],
      requireActionAllowlist: true,
    },
    render: nativeSlotRenderer("status-bar"),
  }],
]);
