import assert from "node:assert/strict";
import test from "node:test";
import {
  UI_IR_API_VERSION,
  createUiIrActionRequest,
  type UiIrJsonRecord,
  type UiIrParserOptions,
} from "../app/plugins/ui-ir";
import {
  UiIrValidationError,
  parseUiIR,
  renderUiIR,
} from "../src/vue/runtime/vue-ir";
import { Comment } from "vue";

const permissions = {
  allowedActions: ["settings.save", "settings.change"],
  allowedCapabilities: ["settings.write"],
  allowedActionCapabilities: new Map<string, readonly string[]>([
    ["settings.save", ["settings.write"]],
    ["settings.change", ["settings.write"]],
  ]),
};

function documentWith(root: unknown, bindings?: unknown[]): unknown {
  return {
    apiVersion: UI_IR_API_VERSION,
    root,
    ...(bindings ? { bindings } : {}),
  };
}

function expectValidation(
  input: unknown,
  code: string,
  options: UiIrParserOptions = {},
): void {
  let error: unknown;
  try {
    parseUiIR(input, options);
  } catch (caught) {
    error = caught;
  }
  assert.ok(error instanceof UiIrValidationError, "expected UI IR validation to fail");
  assert.equal(error.code, code);
}

test("安全 Vue UI IR rejects injection-shaped input", () => {
  expectValidation(documentWith({ type: "raw-html", html: "<img src=x>" }), "type-invalid");
  expectValidation(documentWith({ type: "component", name: "UserComponent" }), "type-invalid");
  expectValidation(documentWith({ type: "text", text: "safe", expression: "window.alert(1)" }), "attribute-not-allowed");
  expectValidation(documentWith({ type: "text", text: "safe", script: "alert(1)" }), "attribute-not-allowed");
  expectValidation(documentWith({ type: "text", text: "safe", style: "color:red" }), "attribute-not-allowed");
  expectValidation(documentWith({ type: "text", text: () => "unsafe" }), "value-invalid");
  expectValidation(documentWith({ type: "text", text: "safe", onClick: () => undefined }), "attribute-not-allowed");
});

test("安全 Vue UI IR rejects malformed bindings and unauthorized actions", () => {
  expectValidation(
    documentWith({
      type: "text",
      text: { type: "state-binding", path: "state[secret]" },
    }),
    "value-invalid",
  );
  expectValidation(
    documentWith({
      type: "button",
      label: "Save",
      action: { type: "action-binding", actionId: "settings.delete", capability: "settings.write" },
    }),
    "action-not-allowed",
    { permissions },
  );
  expectValidation(
    documentWith({
      type: "button",
      label: "Save",
      action: { type: "action-binding", actionId: "settings.save", capability: "network.write" },
    }),
    "action-policy-missing",
  );
});

test("安全 Vue UI IR enforces depth, node, string, and property limits", () => {
  const deep = (depth: number): unknown =>
    depth === 0
      ? { type: "text", text: "leaf" }
      : { type: "stack", children: [deep(depth - 1)] };

  expectValidation(documentWith(deep(4)), "depth-exceeded", { limits: { maxDepth: 2 } });
  expectValidation(
    documentWith({ type: "stack", children: [{ type: "text", text: "a" }, { type: "text", text: "b" }] }),
    "node-limit-exceeded",
    { limits: { maxNodes: 2 } },
  );
  expectValidation(
    documentWith({ type: "text", text: "0123456789" }),
    "string-limit-exceeded",
    { limits: { maxStringLength: 4 } },
  );
  expectValidation(
    documentWith({ type: "text", text: "safe", tone: "default" }),
    "property-limit-exceeded",
    { limits: { maxObjectProperties: 2 } },
  );
});

test("安全 Vue UI IR accepts the minimum union and renders allowlisted elements", () => {
  const ir = parseUiIR(
    documentWith(
      {
        type: "stack",
        direction: "column",
        children: [
          { type: "slot", name: "toolbar", children: [{ type: "text", text: "Plugin" }] },
          {
            type: "grid",
            columns: 2,
            children: [
              { type: "text", text: { type: "state-binding", path: "title", fallback: "Title" } },
              {
                type: "button",
                label: "Save",
                variant: "primary",
                action: {
                  type: "action-binding",
                  actionId: "settings.save",
                  capability: "settings.write",
                  parameters: { source: "ui-ir" },
                },
              },
              {
                type: "input",
                label: "Name",
                bind: { type: "state-binding", path: "name" },
                action: { type: "action-binding", actionId: "settings.change", capability: "settings.write" },
              },
              {
                type: "select",
                label: "Mode",
                options: [{ label: "Fast", value: "fast" }],
                bind: { type: "state-binding", path: "mode" },
              },
              { type: "list", items: [{ label: "One", value: "1" }] },
            ],
          },
        ],
      },
      [{ type: "state-binding", path: "title" }],
    ),
    { permissions },
  );

  assert.equal(ir.apiVersion, UI_IR_API_VERSION);
  assert.equal(ir.root.type, "stack");
  assert.equal(ir.bindings?.[0]?.type, "state-binding");

  const requests: unknown[] = [];
  const vnode = renderUiIR(ir, {
    pluginId: "example.plugin",
    state: { title: "Runtime title", name: "Ada", mode: "fast" },
    allowedSlots: ["toolbar"],
    dispatchAction: (request) => requests.push(request),
  });
  assert.equal(vnode && typeof vnode === "object" ? vnode.type : undefined, "div");

  const stackChildren = (vnode as { children: unknown[] }).children;
  const grid = stackChildren[1] as { type: unknown; children: unknown[] };
  assert.equal(grid.type, "div");
  const button = grid.children.find((child) => (child as { type?: unknown }).type === "button") as {
    props?: { onClick?: () => void };
  } | undefined;
  assert.ok(button);
  button.props?.onClick?.();
  assert.deepEqual(requests, [
    {
      apiVersion: UI_IR_API_VERSION,
      pluginId: "example.plugin",
      actionId: "settings.save",
      capability: "settings.write",
      parameters: { source: "ui-ir" },
    },
  ]);
});

test("安全 Vue UI IR denies unknown slots and function RPC parameters", () => {
  const ir = parseUiIR(documentWith({ type: "slot", name: "secret", children: [] }));
  const vnode = renderUiIR(ir, { pluginId: "example.plugin", allowedSlots: [] });
  assert.equal(vnode && typeof vnode === "object" ? vnode.type : undefined, Comment);

  assert.throws(
    () => createUiIrActionRequest(
      "example.plugin",
      { type: "action-binding", actionId: "settings.save", capability: "settings.write" },
      { callback: (() => undefined) as unknown as UiIrJsonRecord[string] },
    ),
    /UI_IR_PARAMETERS_NOT_STRUCTURED/,
  );
});
