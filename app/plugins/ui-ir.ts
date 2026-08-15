/**
 * Versioned, declarative UI contract for untrusted plugins.
 *
 * This module deliberately contains data-only types and validation helpers.
 * It must not import Vue, DOM APIs, or a host transport implementation.
 */

export const UI_IR_API_VERSION = "anyway.dev/ui-ir/v1" as const;
export const UI_IR_VERSION = UI_IR_API_VERSION;

export type UiIrApiVersion = typeof UI_IR_API_VERSION;

export type UiIrPrimitive = string | number | boolean | null;
export type UiIrJsonValue =
  | UiIrPrimitive
  | readonly UiIrJsonValue[]
  | { readonly [key: string]: UiIrJsonValue };
export type UiIrJsonRecord = { readonly [key: string]: UiIrJsonValue };

export type UiIrStateBinding = {
  readonly type: "state-binding";
  /** Dotted state path, never a JavaScript expression. */
  readonly path: string;
  readonly fallback?: UiIrJsonValue;
};

export type UiIrActionBinding = {
  readonly type: "action-binding";
  /** Stable host action identifier, not a function or an expression. */
  readonly actionId: string;
  /** Capability required by the action. */
  readonly capability: string;
  /** Static JSON parameters supplied by the plugin. */
  readonly parameters?: UiIrJsonRecord;
};

export type UiIrBinding = UiIrStateBinding | UiIrActionBinding;

export type UiIrSlotNode = {
  readonly type: "slot";
  readonly name: string;
  readonly children: readonly UiIrNode[];
};

export type UiIrStackNode = {
  readonly type: "stack";
  readonly direction?: "row" | "column";
  readonly gap?: number;
  readonly children: readonly UiIrNode[];
};

export type UiIrGridNode = {
  readonly type: "grid";
  readonly columns: number;
  readonly gap?: number;
  readonly children: readonly UiIrNode[];
};

export type UiIrTextNode = {
  readonly type: "text";
  readonly text: string | UiIrStateBinding;
  readonly tone?: "default" | "muted" | "danger" | "success";
};

export type UiIrButtonNode = {
  readonly type: "button";
  readonly label: string | UiIrStateBinding;
  readonly action: UiIrActionBinding;
  readonly variant?: "default" | "primary" | "danger";
  readonly disabled?: boolean | UiIrStateBinding;
};

export type UiIrInputNode = {
  readonly type: "input";
  readonly label?: string;
  readonly placeholder?: string;
  readonly value?: string | UiIrStateBinding;
  readonly bind?: UiIrStateBinding;
  readonly action?: UiIrActionBinding;
  readonly disabled?: boolean | UiIrStateBinding;
};

export type UiIrSelectOption = {
  readonly label: string;
  readonly value: string;
};

export type UiIrSelectNode = {
  readonly type: "select";
  readonly label?: string;
  readonly options: readonly UiIrSelectOption[];
  readonly value?: string | UiIrStateBinding;
  readonly bind?: UiIrStateBinding;
  readonly action?: UiIrActionBinding;
  readonly disabled?: boolean | UiIrStateBinding;
};

export type UiIrListItem = {
  readonly label: string;
  readonly value?: string;
};

export type UiIrListNode = {
  readonly type: "list";
  readonly items: readonly UiIrListItem[] | UiIrStateBinding;
  readonly emptyText?: string;
};

export type UiIrNode =
  | UiIrSlotNode
  | UiIrStackNode
  | UiIrGridNode
  | UiIrTextNode
  | UiIrButtonNode
  | UiIrInputNode
  | UiIrSelectNode
  | UiIrListNode;

export type UiIrDocument = {
  readonly apiVersion: UiIrApiVersion;
  readonly root: UiIrNode;
  /** Optional declarations for tooling and future host preflight. */
  readonly bindings?: readonly UiIrBinding[];
};

export type UiIrActionRequest = {
  readonly apiVersion: UiIrApiVersion;
  readonly pluginId: string;
  readonly actionId: string;
  readonly capability: string;
  readonly parameters: UiIrJsonRecord;
};

export type UiIrActionDispatcher = (
  request: UiIrActionRequest,
) => Promise<unknown> | unknown;

export type UiIrLimits = {
  readonly maxDepth: number;
  readonly maxNodes: number;
  readonly maxStringLength: number;
  readonly maxArrayLength: number;
  readonly maxObjectProperties: number;
};

export type UiIrPermissionPolicy = {
  readonly allowedActions?: ReadonlySet<string> | readonly string[];
  readonly allowedCapabilities?: ReadonlySet<string> | readonly string[];
  readonly allowedActionCapabilities?: ReadonlyMap<
    string,
    ReadonlySet<string> | readonly string[]
  >;
  /** Secure default: action-bearing IR must provide both allowlists. */
  readonly requireActionAllowlist?: boolean;
};

export type UiIrParserOptions = {
  readonly limits?: Partial<UiIrLimits>;
  readonly permissions?: UiIrPermissionPolicy;
};

export const DEFAULT_UI_IR_LIMITS: UiIrLimits = Object.freeze({
  maxDepth: 16,
  maxNodes: 256,
  maxStringLength: 512,
  maxArrayLength: 128,
  maxObjectProperties: 16,
});

const SAFE_IDENTIFIER = /^[A-Za-z][A-Za-z0-9._:-]{0,127}$/u;

export function isUiIrIdentifier(value: string): boolean {
  return SAFE_IDENTIFIER.test(value);
}

export function isUiIrJsonValue(value: unknown): value is UiIrJsonValue {
  if (value === null) return true;
  if (typeof value === "string" || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (Array.isArray(value)) return value.every(isUiIrJsonValue);
  if (typeof value !== "object") return false;
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) return false;
  if (Object.getOwnPropertySymbols(value).length > 0) return false;
  const record = value as { readonly [key: string]: unknown };
  return Object.keys(record).every((key) => isUiIrJsonValue(record[key]));
}

export function isUiIrJsonRecord(value: unknown): value is UiIrJsonRecord {
  return (
    typeof value === "object" &&
    value !== null &&
    !Array.isArray(value) &&
    (Object.getPrototypeOf(value) === Object.prototype ||
      Object.getPrototypeOf(value) === null) &&
    isUiIrJsonValue(value)
  );
}

export function createUiIrActionRequest(
  pluginId: string,
  binding: UiIrActionBinding,
  parameters: UiIrJsonRecord = {},
): UiIrActionRequest {
  if (!isUiIrIdentifier(pluginId)) throw new Error("UI_IR_PLUGIN_ID_INVALID");
  if (!isUiIrIdentifier(binding.actionId)) throw new Error("UI_IR_ACTION_INVALID");
  if (!isUiIrIdentifier(binding.capability)) throw new Error("UI_IR_CAPABILITY_INVALID");
  if (!isUiIrJsonRecord(binding.parameters ?? {})) throw new Error("UI_IR_PARAMETERS_NOT_STRUCTURED");
  if (!isUiIrJsonRecord(parameters)) throw new Error("UI_IR_PARAMETERS_NOT_STRUCTURED");

  const mergedParameters = { ...(binding.parameters ?? {}), ...parameters };
  if (!isUiIrJsonRecord(mergedParameters)) throw new Error("UI_IR_PARAMETERS_NOT_STRUCTURED");
  return Object.freeze({
    apiVersion: UI_IR_API_VERSION,
    pluginId,
    actionId: binding.actionId,
    capability: binding.capability,
    parameters: Object.freeze(mergedParameters),
  });
}

// Friendly aliases for consumers that use the short names from the design doc.
export type UIIR = UiIrDocument;
export type UiIR = UiIrDocument;
export type UIIRNode = UiIrNode;
export type StateBinding = UiIrStateBinding;
export type ActionBinding = UiIrActionBinding;
