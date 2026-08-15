import {
  DEFAULT_UI_IR_LIMITS,
  UI_IR_API_VERSION,
  isUiIrIdentifier,
  type UiIrActionBinding,
  type UiIrBinding,
  type UiIrDocument,
  type UiIrGridNode,
  type UiIrInputNode,
  type UiIrJsonRecord,
  type UiIrJsonValue,
  type UiIrListItem,
  type UiIrListNode,
  type UiIrNode,
  type UiIrParserOptions,
  type UiIrSelectNode,
  type UiIrSelectOption,
  type UiIrSlotNode,
  type UiIrStackNode,
  type UiIrStateBinding,
  type UiIrTextNode,
  type UiIrButtonNode,
  type UiIrLimits,
  type UiIrPermissionPolicy,
} from "../../../../app/plugins/ui-ir";

export type UiIrValidationCode =
  | "root-invalid"
  | "api-version-invalid"
  | "type-invalid"
  | "attribute-not-allowed"
  | "value-invalid"
  | "depth-exceeded"
  | "node-limit-exceeded"
  | "string-limit-exceeded"
  | "array-limit-exceeded"
  | "property-limit-exceeded"
  | "cycle-detected"
  | "action-not-allowed"
  | "capability-not-allowed"
  | "action-policy-missing";

export class UiIrValidationError extends Error {
  readonly name = "UiIrValidationError";

  constructor(
    readonly code: UiIrValidationCode,
    readonly path: string,
    message: string,
  ) {
    super(`${message} at ${path}`);
  }
}

type ParserContext = {
  readonly limits: UiIrLimits;
  readonly permissions: UiIrPermissionPolicy;
  readonly activeObjects: WeakSet<object>;
  nodeCount: number;
};

type Mutable<T> = { -readonly [Key in keyof T]: T[Key] };

const NODE_KEYS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  slot: ["type", "name", "children"],
  stack: ["type", "direction", "gap", "children"],
  grid: ["type", "columns", "gap", "children"],
  text: ["type", "text", "tone"],
  button: ["type", "label", "action", "variant", "disabled"],
  input: ["type", "label", "placeholder", "value", "bind", "action", "disabled"],
  select: ["type", "label", "options", "value", "bind", "action", "disabled"],
  list: ["type", "items", "emptyText"],
});

const BINDING_KEYS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  "state-binding": ["type", "path", "fallback"],
  "action-binding": ["type", "actionId", "capability", "parameters"],
});

const IDENTIFIER_PATTERN = /^[A-Za-z][A-Za-z0-9._:-]{0,127}$/u;
const STATE_PATH_PATTERN = /^[A-Za-z][A-Za-z0-9_.-]{0,127}$/u;

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

function fail(
  code: UiIrValidationCode,
  path: string,
  message: string,
): never {
  throw new UiIrValidationError(code, path, message);
}

function enterObject(context: ParserContext, value: object, path: string): void {
  if (context.activeObjects.has(value)) fail("cycle-detected", path, "cyclic IR is not allowed");
  context.activeObjects.add(value);
}

function leaveObject(context: ParserContext, value: object): void {
  context.activeObjects.delete(value);
}

function assertProperties(
  context: ParserContext,
  record: Record<string, unknown>,
  allowed: readonly string[],
  path: string,
): void {
  const symbolKeys = Object.getOwnPropertySymbols(record);
  if (symbolKeys.length > 0) fail("attribute-not-allowed", path, "symbol properties are not allowed");
  const keys = Object.keys(record);
  if (keys.length > context.limits.maxObjectProperties) {
    fail("property-limit-exceeded", path, "too many object properties");
  }
  for (const key of keys) {
    if (!allowed.includes(key)) {
      fail("attribute-not-allowed", `${path}.${key}`, `attribute '${key}' is not allowed`);
    }
  }
}

function countNode(context: ParserContext, path: string): void {
  context.nodeCount += 1;
  if (context.nodeCount > context.limits.maxNodes) {
    fail("node-limit-exceeded", path, "UI IR node limit exceeded");
  }
}

function assertDepth(context: ParserContext, depth: number, path: string): void {
  if (depth > context.limits.maxDepth) {
    fail("depth-exceeded", path, "UI IR depth limit exceeded");
  }
}

function parseString(
  context: ParserContext,
  value: unknown,
  path: string,
  options: { identifier?: boolean; statePath?: boolean; nonEmpty?: boolean } = {},
): string {
  if (typeof value !== "string") fail("value-invalid", path, "expected a string");
  if ([...value].length > context.limits.maxStringLength) {
    fail("string-limit-exceeded", path, "string length limit exceeded");
  }
  if (options.nonEmpty && value.trim().length === 0) {
    fail("value-invalid", path, "string must not be empty");
  }
  if (options.identifier && !IDENTIFIER_PATTERN.test(value)) {
    fail("value-invalid", path, "invalid stable identifier");
  }
  if (options.statePath && !STATE_PATH_PATTERN.test(value)) {
    fail("value-invalid", path, "invalid state path");
  }
  if (options.statePath && value.split(".").some((segment) => ["__proto__", "prototype", "constructor"].includes(segment))) {
    fail("value-invalid", path, "reserved state path segment");
  }
  return value;
}

function parseNumber(
  value: unknown,
  path: string,
  range: { min: number; max: number; integer?: boolean },
): number {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    value < range.min ||
    value > range.max ||
    (range.integer && !Number.isInteger(value))
  ) {
    fail("value-invalid", path, "number is outside the allowed range");
  }
  return value;
}

function parseEnum<T extends string>(
  value: unknown,
  path: string,
  allowed: readonly T[],
): T {
  if (typeof value !== "string" || !allowed.includes(value as T)) {
    fail("value-invalid", path, "value is not in the allowlist");
  }
  return value as T;
}

function parseJsonValue(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): UiIrJsonValue {
  assertDepth(context, depth, path);
  if (value === null || typeof value === "boolean") return value;
  if (typeof value === "string") return parseString(context, value, path);
  if (typeof value === "number") {
    if (!Number.isFinite(value)) fail("value-invalid", path, "non-finite numbers are not allowed");
    return value;
  }
  if (Array.isArray(value)) {
    if (value.length > context.limits.maxArrayLength) {
      fail("array-limit-exceeded", path, "array length limit exceeded");
    }
    enterObject(context, value, path);
    try {
      return value.map((item, index) => parseJsonValue(context, item, depth + 1, `${path}[${index}]`));
    } finally {
      leaveObject(context, value);
    }
  }
  if (!isPlainRecord(value)) fail("value-invalid", path, "only JSON values are allowed");
  enterObject(context, value, path);
  try {
    assertProperties(context, value, Object.keys(value), path);
    const result: Record<string, UiIrJsonValue> = {};
    for (const [key, item] of Object.entries(value)) {
      result[key] = parseJsonValue(context, item, depth + 1, `${path}.${key}`);
    }
    return result;
  } finally {
    leaveObject(context, value);
  }
}

function parseStateBinding(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): UiIrStateBinding {
  assertDepth(context, depth, path);
  if (!isPlainRecord(value)) fail("value-invalid", path, "state binding must be an object");
  countNode(context, path);
  enterObject(context, value, path);
  try {
    assertProperties(context, value, BINDING_KEYS["state-binding"], path);
    const binding: Mutable<UiIrStateBinding> = {
      type: "state-binding",
      path: parseString(context, value.path, `${path}.path`, { statePath: true, nonEmpty: true }),
    };
    if (Object.prototype.hasOwnProperty.call(value, "fallback")) {
      binding.fallback = parseJsonValue(context, value.fallback, depth + 1, `${path}.fallback`);
    }
    return binding;
  } finally {
    leaveObject(context, value);
  }
}

function toSet(value: ReadonlySet<string> | readonly string[] | undefined): ReadonlySet<string> | undefined {
  if (value === undefined) return undefined;
  return value instanceof Set ? value : new Set(value);
}

function validateActionPermission(
  context: ParserContext,
  actionId: string,
  capability: string,
  path: string,
): void {
  const policy = context.permissions;
  const requireAllowlist = policy.requireActionAllowlist !== false;
  const actions = toSet(policy.allowedActions);
  const capabilities = toSet(policy.allowedCapabilities);
  if (requireAllowlist && (!actions || !capabilities)) {
    fail("action-policy-missing", path, "action and capability allowlists are required");
  }
  if (actions && !actions.has(actionId)) fail("action-not-allowed", `${path}.actionId`, "action is not allowed");
  if (capabilities && !capabilities.has(capability)) {
    fail("capability-not-allowed", `${path}.capability`, "capability is not allowed");
  }
  if (policy.allowedActionCapabilities) {
    const allowedCapabilities = toSet(policy.allowedActionCapabilities.get(actionId));
    if (!allowedCapabilities || !allowedCapabilities.has(capability)) {
      fail("capability-not-allowed", path, "action and capability are not an allowed pair");
    }
  }
}

function parseActionBinding(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): UiIrActionBinding {
  assertDepth(context, depth, path);
  if (!isPlainRecord(value)) fail("value-invalid", path, "action binding must be an object");
  countNode(context, path);
  enterObject(context, value, path);
  try {
    assertProperties(context, value, BINDING_KEYS["action-binding"], path);
    const actionId = parseString(context, value.actionId, `${path}.actionId`, {
      identifier: true,
      nonEmpty: true,
    });
    const capability = parseString(context, value.capability, `${path}.capability`, {
      identifier: true,
      nonEmpty: true,
    });
    validateActionPermission(context, actionId, capability, path);
    const binding: Mutable<UiIrActionBinding> = { type: "action-binding", actionId, capability };
    if (Object.prototype.hasOwnProperty.call(value, "parameters")) {
      if (!isPlainRecord(value.parameters)) {
        fail("value-invalid", `${path}.parameters`, "parameters must be a JSON object");
      }
      binding.parameters = parseJsonValue(context, value.parameters, depth + 1, `${path}.parameters`) as UiIrJsonRecord;
    }
    return binding;
  } finally {
    leaveObject(context, value);
  }
}

function parseStateOrString(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): string | UiIrStateBinding {
  return typeof value === "string"
    ? parseString(context, value, path)
    : parseStateBinding(context, value, depth, path);
}

function parseStateOrBoolean(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): boolean | UiIrStateBinding {
  return typeof value === "boolean"
    ? value
    : parseStateBinding(context, value, depth, path);
}

function parseChildren(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): readonly UiIrNode[] {
  if (!Array.isArray(value)) fail("value-invalid", path, "children must be an array");
  if (value.length > context.limits.maxArrayLength) fail("array-limit-exceeded", path, "children limit exceeded");
  return value.map((child, index) => parseNode(context, child, depth + 1, `${path}[${index}]`));
}

function parseOptions(
  context: ParserContext,
  value: unknown,
  path: string,
): readonly UiIrSelectOption[] {
  if (!Array.isArray(value)) fail("value-invalid", path, "options must be an array");
  if (value.length > context.limits.maxArrayLength) fail("array-limit-exceeded", path, "options limit exceeded");
  return value.map((option, index) => {
    const itemPath = `${path}[${index}]`;
    if (!isPlainRecord(option)) fail("value-invalid", itemPath, "option must be an object");
    enterObject(context, option, itemPath);
    try {
      assertProperties(context, option, ["label", "value"], itemPath);
      return {
        label: parseString(context, option.label, `${itemPath}.label`, { nonEmpty: true }),
        value: parseString(context, option.value, `${itemPath}.value`, { nonEmpty: true }),
      };
    } finally {
      leaveObject(context, option);
    }
  });
}

function parseListItems(
  context: ParserContext,
  value: unknown,
  path: string,
): readonly UiIrListItem[] {
  if (!Array.isArray(value)) fail("value-invalid", path, "items must be an array or state binding");
  if (value.length > context.limits.maxArrayLength) fail("array-limit-exceeded", path, "items limit exceeded");
  return value.map((item, index) => {
    const itemPath = `${path}[${index}]`;
    if (!isPlainRecord(item)) fail("value-invalid", itemPath, "list item must be an object");
    enterObject(context, item, itemPath);
    try {
      assertProperties(context, item, ["label", "value"], itemPath);
      const parsed: Mutable<UiIrListItem> = {
        label: parseString(context, item.label, `${itemPath}.label`, { nonEmpty: true }),
      };
      if (Object.prototype.hasOwnProperty.call(item, "value")) {
        parsed.value = parseString(context, item.value, `${itemPath}.value`);
      }
      return parsed;
    } finally {
      leaveObject(context, item);
    }
  });
}

function parseNode(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): UiIrNode {
  assertDepth(context, depth, path);
  if (!isPlainRecord(value)) fail("value-invalid", path, "node must be a plain object");
  const type = parseString(context, value.type, `${path}.type`, { identifier: true, nonEmpty: true });
  const allowed = NODE_KEYS[type];
  if (!allowed) fail("type-invalid", `${path}.type`, `node type '${type}' is not allowed`);
  countNode(context, path);
  enterObject(context, value, path);
  try {
    assertProperties(context, value, allowed, path);
    switch (type) {
      case "slot": {
        const node: Mutable<UiIrSlotNode> = {
          type: "slot" as const,
          name: parseString(context, value.name, `${path}.name`, { identifier: true, nonEmpty: true }),
          children: parseChildren(context, value.children, depth, `${path}.children`),
        };
        return node;
      }
      case "stack": {
        const node: Mutable<UiIrStackNode> = {
          type: "stack" as const,
          direction: value.direction === undefined
            ? "column"
            : parseEnum(value.direction, `${path}.direction`, ["row", "column"] as const),
          children: parseChildren(context, value.children, depth, `${path}.children`),
        };
        if (value.gap !== undefined) node.gap = parseNumber(value.gap, `${path}.gap`, { min: 0, max: 64 });
        return node;
      }
      case "grid": {
        const node: Mutable<UiIrGridNode> = {
          type: "grid" as const,
          columns: parseNumber(value.columns, `${path}.columns`, { min: 1, max: 12, integer: true }),
          children: parseChildren(context, value.children, depth, `${path}.children`),
        };
        if (value.gap !== undefined) node.gap = parseNumber(value.gap, `${path}.gap`, { min: 0, max: 64 });
        return node;
      }
      case "text": {
        const node: Mutable<UiIrTextNode> = {
          type: "text" as const,
          text: parseStateOrString(context, value.text, depth + 1, `${path}.text`),
        };
        if (value.tone !== undefined) {
          node.tone = parseEnum(value.tone, `${path}.tone`, ["default", "muted", "danger", "success"] as const);
        }
        return node;
      }
      case "button": {
        const node: Mutable<UiIrButtonNode> = {
          type: "button",
          label: parseStateOrString(context, value.label, depth + 1, `${path}.label`),
          action: parseActionBinding(context, value.action, depth + 1, `${path}.action`),
        };
        if (value.variant !== undefined) {
          node.variant = parseEnum(value.variant, `${path}.variant`, ["default", "primary", "danger"] as const);
        }
        if (value.disabled !== undefined) {
          node.disabled = parseStateOrBoolean(context, value.disabled, depth + 1, `${path}.disabled`);
        }
        return node;
      }
      case "input": {
        const node: Mutable<UiIrInputNode> = { type: "input" };
        if (value.label !== undefined) node.label = parseString(context, value.label, `${path}.label`);
        if (value.placeholder !== undefined) node.placeholder = parseString(context, value.placeholder, `${path}.placeholder`);
        if (value.value !== undefined) node.value = parseStateOrString(context, value.value, depth + 1, `${path}.value`);
        if (value.bind !== undefined) node.bind = parseStateBinding(context, value.bind, depth + 1, `${path}.bind`);
        if (value.action !== undefined) node.action = parseActionBinding(context, value.action, depth + 1, `${path}.action`);
        if (value.disabled !== undefined) node.disabled = parseStateOrBoolean(context, value.disabled, depth + 1, `${path}.disabled`);
        return node;
      }
      case "select": {
        const node: Mutable<UiIrSelectNode> = {
          type: "select" as const,
          options: parseOptions(context, value.options, `${path}.options`),
        };
        if (value.label !== undefined) node.label = parseString(context, value.label, `${path}.label`);
        if (value.value !== undefined) node.value = parseStateOrString(context, value.value, depth + 1, `${path}.value`);
        if (value.bind !== undefined) node.bind = parseStateBinding(context, value.bind, depth + 1, `${path}.bind`);
        if (value.action !== undefined) node.action = parseActionBinding(context, value.action, depth + 1, `${path}.action`);
        if (value.disabled !== undefined) node.disabled = parseStateOrBoolean(context, value.disabled, depth + 1, `${path}.disabled`);
        return node;
      }
      case "list": {
        const node: Mutable<UiIrListNode> = {
          type: "list" as const,
          items: isPlainRecord(value.items)
            ? parseStateBinding(context, value.items, depth + 1, `${path}.items`)
            : parseListItems(context, value.items, `${path}.items`),
        };
        if (value.emptyText !== undefined) node.emptyText = parseString(context, value.emptyText, `${path}.emptyText`);
        return node;
      }
      default:
        fail("type-invalid", `${path}.type`, "node type is not allowed");
    }
  } finally {
    leaveObject(context, value);
  }
}

function parseBinding(
  context: ParserContext,
  value: unknown,
  depth: number,
  path: string,
): UiIrBinding {
  if (!isPlainRecord(value)) fail("value-invalid", path, "binding must be a plain object");
  const type = parseString(context, value.type, `${path}.type`, { identifier: true, nonEmpty: true });
  if (type === "state-binding") return parseStateBinding(context, value, depth, path);
  if (type === "action-binding") return parseActionBinding(context, value, depth, path);
  fail("type-invalid", `${path}.type`, "binding type is not allowed");
}

function mergeLimits(options: UiIrParserOptions): UiIrLimits {
  const candidate = { ...DEFAULT_UI_IR_LIMITS, ...(options.limits ?? {}) };
  for (const [key, value] of Object.entries(candidate)) {
    if (typeof value !== "number" || !Number.isInteger(value) || value < 1) {
      throw new Error(`UI_IR_LIMIT_INVALID:${key}`);
    }
  }
  return candidate;
}

export function parseUiIR(input: unknown, options: UiIrParserOptions = {}): UiIrDocument {
  if (!isPlainRecord(input)) fail("root-invalid", "root", "UI IR root must be a plain object");
  const context: ParserContext = {
    limits: mergeLimits(options),
    permissions: options.permissions ?? {},
    activeObjects: new WeakSet<object>(),
    nodeCount: 0,
  };
  enterObject(context, input, "root");
  try {
    assertProperties(context, input, ["apiVersion", "root", "bindings"], "root");
    const apiVersion = parseString(context, input.apiVersion, "root.apiVersion", { nonEmpty: true });
    if (apiVersion !== UI_IR_API_VERSION) {
      fail("api-version-invalid", "root.apiVersion", `expected ${UI_IR_API_VERSION}`);
    }
    const root = parseNode(context, input.root, 0, "root.root");
    let bindings: readonly UiIrBinding[] | undefined;
    if (input.bindings !== undefined) {
      if (!Array.isArray(input.bindings)) fail("value-invalid", "root.bindings", "bindings must be an array");
      if (input.bindings.length > context.limits.maxArrayLength) {
        fail("array-limit-exceeded", "root.bindings", "bindings limit exceeded");
      }
      bindings = input.bindings.map((binding, index) =>
        parseBinding(context, binding, 1, `root.bindings[${index}]`),
      );
    }
    const document: UiIrDocument = {
      apiVersion: UI_IR_API_VERSION,
      root,
    };
    if (bindings !== undefined) return { ...document, bindings };
    return document;
  } finally {
    leaveObject(context, input);
  }
}

export function isUiIR(input: unknown, options: UiIrParserOptions = {}): input is UiIrDocument {
  try {
    parseUiIR(input, options);
    return true;
  } catch (error) {
    if (error instanceof UiIrValidationError) return false;
    throw error;
  }
}
