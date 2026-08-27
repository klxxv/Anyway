import assert from "node:assert/strict";
import test from "node:test";
import { Comment } from "vue";

import {
  UI_IR_API_VERSION,
  permissionPolicyForContributions,
  type UiIrDocument,
  type UiIrJsonRecord,
  type UiIrSlotContribution,
} from "../app/plugins/ui-ir";
import {
  HOST_SDK_API_VERSION,
  HostSdk,
  type HostCallRequest,
  type HostSdkTransport,
} from "../app/platform/host-sdk";
import {
  UiIrActionDisallowedError,
  createUiIrActionDispatcher,
} from "../app/platform/ui-ir-dispatch";
import { PluginSlot, type UiIrPluginContribution } from "../src/vue/runtime/vue-ir";

class RecordingTransport implements HostSdkTransport {
  requests: HostCallRequest[] = [];

  async invoke<T>(request: HostCallRequest) {
    this.requests.push(request);
    return {
      apiVersion: HOST_SDK_API_VERSION,
      requestId: request.requestId,
      result: { accepted: true } as T,
    };
  }
}

const action = (
  actionId: string,
  capability: string,
  parameters: UiIrJsonRecord = {},
) => ({
  type: "action-binding" as const,
  actionId,
  capability,
  ...(Object.keys(parameters).length > 0 ? { parameters } : {}),
});

test("permissionPolicyForContributions collects declared action bindings", () => {
  const contribution: UiIrSlotContribution = {
    slotId: "settings-panel",
    ir: {
      apiVersion: UI_IR_API_VERSION,
      root: {
        type: "stack",
        children: [
          {
            type: "button",
            label: "Save",
            action: action("settings.save", "settings.write", { source: "ui-ir" }),
          },
          {
            type: "input",
            label: "Name",
            action: action("settings.change", "settings.write"),
          },
          {
            type: "grid",
            columns: 1,
            children: [
              {
                type: "select",
                label: "Mode",
                options: [{ label: "Fast", value: "fast" }],
                action: action("settings.mode", "settings.write"),
              },
            ],
          },
        ],
      },
    },
  };

  const policy = permissionPolicyForContributions([contribution]);
  assert.ok(policy.allowedActions.has("settings.save"));
  assert.ok(policy.allowedActions.has("settings.change"));
  assert.ok(policy.allowedActions.has("settings.mode"));
  assert.ok(policy.allowedCapabilities.has("settings.write"));
  assert.deepEqual([...policy.allowedActionCapabilities.get("settings.save")!], ["settings.write"]);
  assert.equal(policy.requireActionAllowlist, true);

  const none = permissionPolicyForContributions(undefined);
  assert.equal(none.allowedActions.size, 0);
  assert.equal(none.allowedCapabilities.size, 0);
});

test("createUiIrActionDispatcher forwards exactly one Host SDK call", async () => {
  const transport = new RecordingTransport();
  const sdk = new HostSdk(transport);
  const dispatch = createUiIrActionDispatcher(sdk, "example.plugin", {
    allowedActions: ["settings.save"],
    allowedCapabilities: ["settings.write"],
    allowedActionCapabilities: new Map([["settings.save", ["settings.write"]]]),
  });

  const result = await dispatch({
    apiVersion: UI_IR_API_VERSION,
    pluginId: "example.plugin",
    actionId: "settings.save",
    capability: "settings.write",
    parameters: { source: "ui-ir" },
  });

  assert.deepEqual(result, { accepted: true });
  assert.equal(transport.requests.length, 1);
  const request = transport.requests[0];
  assert.equal(request.operation, "settings.write");
  assert.equal(request.payload.kind, "inline");
  const value = (request.payload as { kind: "inline"; value: { actionId: string; parameters: unknown } }).value;
  assert.equal(value.actionId, "settings.save");
  assert.deepEqual(value.parameters, { source: "ui-ir" });
});

test("createUiIrActionDispatcher rejects a disallowed action asynchronously", async () => {
  const transport = new RecordingTransport();
  const sdk = new HostSdk(transport);
  const dispatch = createUiIrActionDispatcher(sdk, "example.plugin", {
    allowedActions: ["settings.save"],
    allowedCapabilities: ["settings.write"],
  });

  await assert.rejects(
    async () => dispatch({
      apiVersion: UI_IR_API_VERSION,
      pluginId: "example.plugin",
      actionId: "settings.delete",
      capability: "settings.write",
      parameters: {},
    }),
    UiIrActionDisallowedError,
  );
  await assert.rejects(
    async () => dispatch({
      apiVersion: UI_IR_API_VERSION,
      pluginId: "example.plugin",
      actionId: "settings.save",
      capability: "network.write",
      parameters: {},
    }),
    UiIrActionDisallowedError,
  );
  await assert.rejects(
    async () => dispatch({
      apiVersion: UI_IR_API_VERSION,
      pluginId: "other.plugin",
      actionId: "settings.save",
      capability: "settings.write",
      parameters: {},
    }),
    UiIrActionDisallowedError,
  );
  assert.equal(transport.requests.length, 0);
});

test("createUiIrActionDispatcher passes through without a policy", async () => {
  const transport = new RecordingTransport();
  const dispatch = createUiIrActionDispatcher(new HostSdk(transport), "example.plugin");
  await dispatch({
    apiVersion: UI_IR_API_VERSION,
    pluginId: "example.plugin",
    actionId: "settings.save",
    capability: "settings.write",
    parameters: {},
  });
  assert.equal(transport.requests.length, 1);
  assert.equal(transport.requests[0].operation, "settings.write");
});

function findVNode(
  vnode: unknown,
  predicate: (candidate: { type?: unknown; props?: Record<string, unknown> }) => boolean,
): boolean {
  if (!vnode || typeof vnode !== "object") return false;
  const candidate = vnode as { type?: unknown; props?: Record<string, unknown>; children?: unknown[] };
  if (predicate(candidate)) return true;
  if (Array.isArray(candidate.children)) {
    return candidate.children.some((child) => findVNode(child, predicate));
  }
  return false;
}

test("PluginSlot renders a known slot and returns Comment for an unknown slot", () => {
  const setup = PluginSlot.setup as unknown as (
    props: { slotId: string; contributions?: readonly UiIrPluginContribution[]; nativeSlotRenderers?: Readonly<Record<string, () => unknown>> },
    ctx: {
      slots: Record<string, unknown>;
      emit: () => void;
      attrs: Record<string, unknown>;
      expose: () => void;
    },
  ) => () => unknown;

  const ir: UiIrDocument = {
    apiVersion: UI_IR_API_VERSION,
    root: {
      type: "stack",
      children: [
        { type: "text", text: "Plugin body" },
        {
          type: "button",
          label: "Inspect",
          action: action("node.inspect", "analysis.run"),
        },
      ],
    },
  };
  const contributions: readonly UiIrPluginContribution[] = [
    { pluginId: "example.plugin", ir },
  ];

  const renderKnown = setup({
    slotId: "node-inspector",
    contributions,
    nativeSlotRenderers: { "node-inspector": () => ({ type: "span", props: { "data-ui-ir-native": "node-inspector" } }) },
  }, {
    slots: {},
    emit: () => undefined,
    attrs: {},
    expose: () => undefined,
  });
  const known = renderKnown() as {
    type: unknown;
    children?: unknown[];
    props?: Record<string, unknown>;
  };
  assert.equal(known.type, "div");
  assert.equal((known.props as Record<string, unknown> | undefined)?.["data-ui-ir-slot"], "node-inspector");
  assert.ok(findVNode(known, (candidate) => candidate.type === "span"));
  assert.ok(findVNode(known, (candidate) => candidate.type === "button"));
  assert.ok(findVNode(known, (candidate) => candidate.props?.["data-ui-ir-native"] === "node-inspector"));

  const renderUnknown = setup({ slotId: "not-registered", contributions }, {
    slots: {},
    emit: () => undefined,
    attrs: {},
    expose: () => undefined,
  });
  const unknown = renderUnknown() as { type: unknown };
  assert.equal(unknown.type, Comment);
});
