import {
  Comment,
  defineComponent,
  h,
  inject,
  provide,
  type InjectionKey,
  type PropType,
  type Slots,
  type VNodeChild,
} from "vue";
import {
  createUiIrActionRequest,
  UI_IR_API_VERSION,
  type UiIrActionBinding,
  type UiIrActionDispatcher,
  type UiIrDocument,
  type UiIrInputNode,
  type UiIrJsonRecord,
  type UiIrListItem,
  type UiIrNode,
  type UiIrSelectNode,
  type UiIrStateBinding,
} from "../../../../app/plugins/ui-ir";

export type UiIrState = Readonly<Record<string, unknown>>;

export type UiIrRuntimeContext = {
  readonly pluginId: string;
  readonly state?: UiIrState | (() => UiIrState);
  readonly dispatchAction?: UiIrActionDispatcher;
  readonly setState?: (binding: UiIrStateBinding, value: string) => void;
  readonly allowedSlots?: ReadonlySet<string> | readonly string[];
};

export type UiIrRenderOptions = UiIrRuntimeContext & {
  readonly slots?: Slots;
};

export const uiIrRuntimeKey: InjectionKey<UiIrRuntimeContext> = Symbol("anyway.ui-ir.runtime");

export function provideUiIrRuntime(context: UiIrRuntimeContext): UiIrRuntimeContext {
  provide(uiIrRuntimeKey, context);
  return context;
}

export function useUiIrRuntime(): UiIrRuntimeContext {
  const context = inject(uiIrRuntimeKey, undefined);
  if (!context) throw new Error("useUiIrRuntime must be used inside a UiIR runtime provider");
  return context;
}

function asSet(value: ReadonlySet<string> | readonly string[] | undefined): ReadonlySet<string> {
  if (value instanceof Set) return value;
  return new Set(value ?? []);
}

function readState(state: UiIrState | (() => UiIrState) | undefined, path: string): unknown {
  const source = typeof state === "function" ? state() : state;
  let current: unknown = source;
  for (const segment of path.split(".")) {
    if (typeof current !== "object" || current === null || Array.isArray(current)) return undefined;
    current = (current as Record<string, unknown>)[segment];
  }
  return current;
}

function resolveBinding(
  binding: UiIrStateBinding,
  state: UiIrState | (() => UiIrState) | undefined,
): unknown {
  const value = readState(state, binding.path);
  return value === undefined ? binding.fallback : value;
}

function resolveText(
  value: string | UiIrStateBinding,
  state: UiIrState | (() => UiIrState) | undefined,
): string {
  const resolved = typeof value === "string" ? value : resolveBinding(value, state);
  return resolved === null || resolved === undefined ? "" : String(resolved);
}

function resolveBoolean(
  value: boolean | UiIrStateBinding | undefined,
  state: UiIrState | (() => UiIrState) | undefined,
): boolean {
  if (value === undefined) return false;
  if (typeof value === "boolean") return value;
  return resolveBinding(value, state) === true;
}

function structuredParameters(value: Record<string, unknown>): UiIrJsonRecord {
  const parameters: Record<string, UiIrJsonRecord[string]> = {};
  for (const [key, item] of Object.entries(value)) {
    if (typeof item === "string" || typeof item === "number" || typeof item === "boolean" || item === null) {
      parameters[key] = item;
    }
  }
  return parameters;
}

function dispatch(
  pluginId: string,
  binding: UiIrActionBinding,
  dispatcher: UiIrActionDispatcher | undefined,
  parameters: Record<string, unknown> = {},
): void {
  if (!dispatcher) return;
  const request = createUiIrActionRequest(pluginId, binding, structuredParameters(parameters));
  void dispatcher(request);
}

function setState(
  binding: UiIrStateBinding | undefined,
  value: string,
  setter: ((binding: UiIrStateBinding, value: string) => void) | undefined,
): void {
  if (binding && setter) setter(binding, value);
}

function renderInput(
  node: UiIrInputNode,
  options: UiIrRenderOptions,
): VNodeChild {
  const value = node.value === undefined
    ? ""
    : typeof node.value === "string"
      ? node.value
      : resolveText(node.value, options.state);
  const input = h("input", {
    class: "ui-ir-input",
    type: "text",
    value,
    placeholder: node.placeholder,
    disabled: resolveBoolean(node.disabled, options.state),
    onInput: (event: Event) => {
      const next = event.target instanceof HTMLInputElement ? event.target.value : "";
      setState(node.bind, next, options.setState);
      if (node.action) dispatch(options.pluginId, node.action, options.dispatchAction, { value: next });
    },
  });
  return node.label ? h("label", { class: "ui-ir-field" }, [node.label, input]) : input;
}

function renderSelect(
  node: UiIrSelectNode,
  options: UiIrRenderOptions,
): VNodeChild {
  const selected = node.value === undefined
    ? ""
    : typeof node.value === "string"
      ? node.value
      : resolveText(node.value, options.state);
  const select = h(
    "select",
    {
      class: "ui-ir-select",
      value: selected,
      disabled: resolveBoolean(node.disabled, options.state),
      onChange: (event: Event) => {
        const next = event.target instanceof HTMLSelectElement ? event.target.value : "";
        setState(node.bind, next, options.setState);
        if (node.action) dispatch(options.pluginId, node.action, options.dispatchAction, { value: next });
      },
    },
    node.options.map((option) => h("option", { value: option.value }, option.label)),
  );
  return node.label ? h("label", { class: "ui-ir-field" }, [node.label, select]) : select;
}

function renderList(
  items: readonly UiIrListItem[],
  emptyText: string | undefined,
): VNodeChild {
  if (items.length === 0) return h("div", { class: "ui-ir-list-empty" }, emptyText ?? "");
  return h(
    "ul",
    { class: "ui-ir-list" },
    items.map((item) => h("li", { key: item.value ?? item.label }, item.label)),
  );
}

function resolveListItems(
  value: readonly UiIrListItem[] | UiIrStateBinding,
  state: UiIrState | (() => UiIrState) | undefined,
): readonly UiIrListItem[] {
  if (Array.isArray(value)) return value as readonly UiIrListItem[];
  const resolved = resolveBinding(value as UiIrStateBinding, state);
  if (!Array.isArray(resolved)) return [];
  return resolved.flatMap((item): UiIrListItem[] => {
    if (typeof item === "string") return [{ label: item }];
    if (typeof item !== "object" || item === null || Array.isArray(item)) return [];
    const label = (item as Record<string, unknown>).label;
    const itemValue = (item as Record<string, unknown>).value;
    if (typeof label !== "string") return [];
    return [{ label, ...(typeof itemValue === "string" ? { value: itemValue } : {}) }];
  });
}

function renderNode(
  node: UiIrNode,
  options: UiIrRenderOptions,
): VNodeChild {
  switch (node.type) {
    case "slot": {
      const allowedSlots = asSet(options.allowedSlots);
      if (!allowedSlots.has(node.name)) return h(Comment, "slot not allowed");
      const nativeSlot = options.slots?.[node.name]?.({});
      return h(
        "div",
        { class: "ui-ir-slot", "data-ui-ir-slot": node.name },
        [...node.children.map((child) => renderNode(child, options)), ...(nativeSlot ?? [])],
      );
    }
    case "stack":
      return h(
        "div",
        { class: ["ui-ir-stack", `ui-ir-stack--${node.direction ?? "column"}`], "data-ui-ir-gap": node.gap },
        node.children.map((child) => renderNode(child, options)),
      );
    case "grid":
      return h(
        "div",
        { class: "ui-ir-grid", "data-ui-ir-columns": node.columns, "data-ui-ir-gap": node.gap },
        node.children.map((child) => renderNode(child, options)),
      );
    case "text":
      return h("span", { class: ["ui-ir-text", `ui-ir-text--${node.tone ?? "default"}`] }, resolveText(node.text, options.state));
    case "button":
      return h(
        "button",
        {
          class: ["ui-ir-button", `ui-ir-button--${node.variant ?? "default"}`],
          type: "button",
          disabled: resolveBoolean(node.disabled, options.state),
          onClick: () => dispatch(options.pluginId, node.action, options.dispatchAction),
        },
        resolveText(node.label, options.state),
      );
    case "input":
      return renderInput(node, options);
    case "select":
      return renderSelect(node, options);
    case "list":
      return renderList(resolveListItems(node.items, options.state), node.emptyText);
  }
}

export function renderUiIR(document: UiIrDocument, options: UiIrRenderOptions): VNodeChild {
  if (document.apiVersion !== UI_IR_API_VERSION) return h(Comment, "unsupported UI IR version");
  return renderNode(document.root, options);
}

export const UiIRRenderer = defineComponent({
  name: "UiIRRenderer",
  props: {
    ir: { type: Object as PropType<UiIrDocument>, required: true },
    pluginId: { type: String, required: true },
    state: { type: Object as PropType<UiIrState>, required: false },
    dispatchAction: { type: Function as PropType<UiIrActionDispatcher>, required: false },
    setState: { type: Function as PropType<UiIrRuntimeContext["setState"]>, required: false },
    allowedSlots: { type: Array as PropType<readonly string[]>, required: false },
  },
  setup(props, { slots }) {
    const provided = inject(uiIrRuntimeKey, undefined);
    return () => renderUiIR(props.ir, {
      pluginId: props.pluginId || provided?.pluginId || "plugin",
      state: props.state ?? provided?.state,
      dispatchAction: props.dispatchAction ?? provided?.dispatchAction,
      setState: props.setState ?? provided?.setState,
      allowedSlots: props.allowedSlots ?? provided?.allowedSlots,
      slots,
    });
  },
});

export const UiIrRenderer = UiIRRenderer;
