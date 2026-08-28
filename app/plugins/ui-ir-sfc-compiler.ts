import {
  baseParse,
  NodeTypes,
  type ElementNode,
  type RootNode,
  type TemplateChildNode,
} from "@vue/compiler-dom";
import {
  parse as parseSfc,
  type SFCParseResult,
  type SFCTemplateBlock,
} from "@vue/compiler-sfc";
import {
  UI_IR_API_VERSION,
  type UiIrActionBinding,
  type UiIrDocument,
  type UiIrJsonRecord,
  type UiIrNode,
  type UiIrStateBinding,
} from "./ui-ir";
import { parseUiIR } from "../../src/vue/runtime/vue-ir/parser";

const COMPONENTS = new Map<string, UiIrNode["type"]>([
  ["UiStack", "stack"], ["ui-stack", "stack"], ["UiGrid", "grid"], ["ui-grid", "grid"],
  ["UiText", "text"], ["ui-text", "text"], ["UiButton", "button"], ["ui-button", "button"],
  ["UiInput", "input"], ["ui-input", "input"], ["UiSelect", "select"], ["ui-select", "select"],
  ["UiList", "list"], ["ui-list", "list"], ["UiSlot", "slot"], ["ui-slot", "slot"],
]);
const SAFE_PATH = /^state(?:\.[A-Za-z][A-Za-z0-9_]{0,63})+$/u;
const SAFE_IDENTIFIER = /^[A-Za-z][A-Za-z0-9._:-]{0,127}$/u;
const SAFE_PARAMETER = /^[A-Za-z][A-Za-z0-9_:-]{0,63}$/u;

export class UiIrSfcCompileError extends Error {
  readonly name = "UiIrSfcCompileError";
  constructor(readonly code: string, readonly location: string, message: string) {
    super(code + " at " + location + ": " + message);
  }
}

type Attribute = { readonly name: string; readonly value: string | true };
type Binding = { readonly name: string; readonly expression: string };
type Props = {
  readonly attributes: ReadonlyMap<string, Attribute>;
  readonly bindings: ReadonlyMap<string, Binding>;
  readonly parameters: UiIrJsonRecord;
};

function fail(code: string, location: string, message: string): never {
  throw new UiIrSfcCompileError(code, location, message);
}

function nodeLocation(node: TemplateChildNode, path: string): string {
  const start = "loc" in node ? node.loc.start : undefined;
  return start ? path + "@" + start.line + ":" + start.column : path;
}

function readTemplate(source: string, filename: string): SFCTemplateBlock {
  let parsed: SFCParseResult;
  try {
    parsed = parseSfc(source, { filename });
  } catch (error) {
    fail("sfc-invalid", "sfc", error instanceof Error ? error.message : String(error));
  }
  if (parsed.errors.length > 0) fail("sfc-invalid", "sfc", parsed.errors.map(String).join("; "));
  const descriptor = parsed.descriptor;
  if (!descriptor.template || descriptor.template.src) fail("template-invalid", "template", "exactly one inline template is required");
  if (descriptor.script || descriptor.scriptSetup || descriptor.styles.length > 0 || descriptor.customBlocks.length > 0) {
    fail("sfc-block-forbidden", "sfc", "script, script setup, style and custom blocks are forbidden");
  }
  if (descriptor.template.lang && descriptor.template.lang !== "html") {
    fail("template-lang-forbidden", "template", "only HTML template syntax is supported");
  }
  return descriptor.template;
}

function parseProps(element: ElementNode, path: string): Props {
  const attributes = new Map<string, Attribute>();
  const bindings = new Map<string, Binding>();
  const parameters: Record<string, string> = {};
  for (const prop of element.props) {
    if (prop.type === NodeTypes.ATTRIBUTE) {
      const value = prop.value ? prop.value.content : true;
      if (attributes.has(prop.name) || bindings.has(prop.name)) fail("attribute-duplicate", path + "." + prop.name, "duplicate prop");
      if (prop.name.startsWith("parameter-")) {
        const parameter = prop.name.slice("parameter-".length);
        if (!SAFE_PARAMETER.test(parameter) || value === true) fail("parameter-invalid", path + "." + prop.name, "parameter-* must be static string props");
        parameters[parameter] = value;
      } else {
        attributes.set(prop.name, { name: prop.name, value });
      }
      continue;
    }
    if (prop.type !== NodeTypes.DIRECTIVE || prop.name !== "bind" || !prop.arg ||
        prop.arg.type !== NodeTypes.SIMPLE_EXPRESSION || !prop.arg.isStatic || prop.modifiers.length > 0) {
      fail("directive-forbidden", path, "only modifier-free :prop bindings are allowed");
    }
    const name = prop.arg.content;
    const expression = prop.exp?.type === NodeTypes.SIMPLE_EXPRESSION
      ? prop.exp.content.trim()
      : "";
    if (!SAFE_IDENTIFIER.test(name) || !SAFE_PATH.test(expression)) {
      fail("expression-forbidden", path + "." + name, "binding must be a simple state.safe.dotted.path");
    }
    if (attributes.has(name) || bindings.has(name)) fail("attribute-duplicate", path + "." + name, "duplicate prop");
    bindings.set(name, { name, expression });
  }
  return { attributes, bindings, parameters };
}

function allowed(props: Props, names: readonly string[], path: string): void {
  const accepted = new Set(names);
  for (const name of [...props.attributes.keys(), ...props.bindings.keys()]) {
    if (!accepted.has(name) && !name.startsWith("parameter-")) fail("prop-forbidden", path + "." + name, "prop is not allowed for this component");
  }
}

function attribute(props: Props, name: string): string | true | undefined {
  return props.attributes.get(name)?.value;
}

function stringProp(props: Props, name: string, path: string): string | undefined {
  const value = attribute(props, name);
  if (value === true) fail("prop-type", path + "." + name, "prop must have a static string value");
  return value;
}

function binding(props: Props, name: string): UiIrStateBinding | undefined {
  const value = props.bindings.get(name);
  if (!value) return undefined;
  const fallback = attribute(props, name + "-fallback");
  return {
    type: "state-binding",
    path: value.expression.slice("state.".length),
    ...(fallback === undefined || fallback === true ? {} : { fallback }),
  };
}

function requiredString(props: Props, name: string, path: string): string {
  const value = stringProp(props, name, path);
  if (!value) fail("prop-required", path + "." + name, "required static prop is missing");
  return value;
}

function numberProp(props: Props, name: string, path: string): number | undefined {
  const value = attribute(props, name);
  if (value === undefined) return undefined;
  if (value === true || !/^(?:0|[1-9][0-9]*)$/u.test(value)) fail("prop-type", path + "." + name, "prop must be a non-negative integer");
  return Number(value);
}

function booleanProp(props: Props, name: string, path: string): boolean | UiIrStateBinding | undefined {
  const value = attribute(props, name);
  if (value !== undefined) {
    if (value === true || value === "true") return true;
    if (value === "false") return false;
    fail("prop-type", path + "." + name, "boolean prop must be true or false");
  }
  return binding(props, name);
}

function textChildren(element: ElementNode, path: string): string | undefined {
  const values: string[] = [];
  for (const child of element.children) {
    if (child.type === NodeTypes.TEXT) {
      if (child.content.trim()) values.push(child.content.trim().replace(/\s+/gu, " "));
    } else if (child.type === NodeTypes.COMMENT) {
      continue;
    } else if (child.type === NodeTypes.INTERPOLATION) {
      fail("expression-forbidden", nodeLocation(child, path), "interpolation is forbidden");
    } else {
      fail("children-forbidden", nodeLocation(child, path), "this component accepts text only");
    }
  }
  return values.length ? values.join(" ") : undefined;
}

function childNodes(element: ElementNode, path: string): UiIrNode[] {
  return element.children
    .filter((child) => child.type !== NodeTypes.TEXT || child.content.trim().length > 0)
    .filter((child) => child.type !== NodeTypes.COMMENT)
    .map((child, index) => parseNode(child, path + ".children[" + index + "]"));
}

function action(props: Props, path: string): UiIrActionBinding {
  const actionId = requiredString(props, "action-id", path);
  const capability = requiredString(props, "capability", path);
  if (!SAFE_IDENTIFIER.test(actionId) || !SAFE_IDENTIFIER.test(capability)) fail("action-invalid", path, "action-id and capability must be static safe identifiers");
  const parameters = Object.fromEntries(Object.entries(props.parameters).sort(([a], [b]) => a.localeCompare(b)));
  return Object.keys(parameters).length ? { type: "action-binding", actionId, capability, parameters } : { type: "action-binding", actionId, capability };
}

function optionsProp(props: Props, path: string): Array<{ label: string; value: string }> {
  const value = stringProp(props, "options", path);
  if (value === undefined) fail("prop-required", path + ".options", "options must use label=value|label=value syntax");
  return value.split("|").map((item, index) => {
    const separator = item.indexOf("=");
    if (separator <= 0) fail("options-invalid", path + ".options[" + index + "]", "option must be label=value");
    return { label: item.slice(0, separator), value: item.slice(separator + 1) };
  });
}

function listItemsProp(props: Props, path: string): Array<{ label: string; value?: string }> {
  const value = stringProp(props, "items", path);
  if (value === undefined) fail("prop-required", path + ".items", "items must use label=value|label syntax");
  return value.split("|").map((item, index) => {
    const separator = item.indexOf("=");
    if (separator < 0) return { label: item };
    if (separator === 0) fail("items-invalid", path + ".items[" + index + "]", "item label is required");
    return { label: item.slice(0, separator), value: item.slice(separator + 1) };
  });
}

function optional<T>(value: T | undefined, key: string): Record<string, T> {
  return value === undefined ? {} : { [key]: value };
}

function parseNode(node: TemplateChildNode, path: string): UiIrNode {
  if (node.type !== NodeTypes.ELEMENT) {
    if (node.type === NodeTypes.INTERPOLATION) fail("expression-forbidden", nodeLocation(node, path), "interpolation is forbidden");
    fail("native-html-forbidden", nodeLocation(node, path), "only allowlisted Ui* components are permitted");
  }
  const element = node;
  const type = COMPONENTS.get(element.tag);
  if (!type) fail("native-html-forbidden", nodeLocation(node, path), "tag " + element.tag + " is not an allowlisted Ui component");
  const props = parseProps(element, path);
  if (type === "stack") {
    allowed(props, ["direction", "gap"], path);
    const direction = stringProp(props, "direction", path);
    if (direction !== undefined && direction !== "row" && direction !== "column") fail("prop-type", path + ".direction", "direction must be row or column");
    return { type, direction: direction ?? "column", ...optional(numberProp(props, "gap", path), "gap"), children: childNodes(element, path) };
  }
  if (type === "grid") {
    allowed(props, ["columns", "gap"], path);
    const columns = numberProp(props, "columns", path);
    if (columns === undefined) fail("prop-required", path + ".columns", "columns is required");
    return { type, columns, ...optional(numberProp(props, "gap", path), "gap"), children: childNodes(element, path) };
  }
  if (type === "text") {
    allowed(props, ["text", "text-fallback", "tone"], path);
    const value = binding(props, "text") ?? stringProp(props, "text", path) ?? textChildren(element, path);
    if (value === undefined) fail("prop-required", path + ".text", "text prop or static text child is required");
    const tone = stringProp(props, "tone", path);
    if (tone !== undefined && !["default", "muted", "danger", "success"].includes(tone)) fail("prop-type", path + ".tone", "unsupported text tone");
    return { type, text: value, ...optional(tone as "default" | "muted" | "danger" | "success" | undefined, "tone") };
  }
  if (type === "button") {
    allowed(props, ["label", "label-fallback", "variant", "disabled", "disabled-fallback", "action-id", "capability"], path);
    const label = binding(props, "label") ?? stringProp(props, "label", path) ?? textChildren(element, path);
    if (label === undefined) fail("prop-required", path + ".label", "label prop or static text child is required");
    const variant = stringProp(props, "variant", path);
    if (variant !== undefined && !["default", "primary", "danger"].includes(variant)) fail("prop-type", path + ".variant", "unsupported button variant");
    return { type, label, action: action(props, path), ...optional(variant as "default" | "primary" | "danger" | undefined, "variant"), ...optional(booleanProp(props, "disabled", path), "disabled") };
  }
  if (type === "input") {
    allowed(props, ["label", "placeholder", "value", "value-fallback", "bind", "action-id", "capability", "disabled", "disabled-fallback"], path);
    return { type, ...optional(stringProp(props, "label", path), "label"), ...optional(stringProp(props, "placeholder", path), "placeholder"), ...optional(binding(props, "value") ?? stringProp(props, "value", path), "value"), ...optional(binding(props, "bind"), "bind"), ...(props.attributes.has("action-id") ? { action: action(props, path) } : {}), ...optional(booleanProp(props, "disabled", path), "disabled") };
  }
  if (type === "select") {
    allowed(props, ["label", "options", "value", "value-fallback", "bind", "action-id", "capability", "disabled", "disabled-fallback"], path);
    return { type, ...optional(stringProp(props, "label", path), "label"), options: optionsProp(props, path), ...optional(binding(props, "value") ?? stringProp(props, "value", path), "value"), ...optional(binding(props, "bind"), "bind"), ...(props.attributes.has("action-id") ? { action: action(props, path) } : {}), ...optional(booleanProp(props, "disabled", path), "disabled") };
  }
  if (type === "list") {
    allowed(props, ["items", "items-fallback", "empty-text"], path);
    const items = binding(props, "items") ?? listItemsProp(props, path);
    return { type, items, ...optional(stringProp(props, "empty-text", path), "emptyText") };
  }
  allowed(props, ["name"], path);
  return { type: "slot", name: requiredString(props, "name", path), children: childNodes(element, path) };
}

export function compileUiIrSfc(source: string, filename = "plugin-ui.vue"): UiIrDocument {
  const template = readTemplate(source, filename);
  let root: RootNode;
  const errors: unknown[] = [];
  try {
    root = baseParse(template.content, { comments: true, onError: (error) => errors.push(error) });
  } catch (error) {
    fail("template-invalid", "template", error instanceof Error ? error.message : String(error));
  }
  if (errors.length) fail("template-invalid", "template", errors.map(String).join("; "));
  const significant = root.children.filter((child) => child.type !== NodeTypes.TEXT || child.content.trim().length > 0);
  if (significant.length !== 1) fail("root-invalid", "template", "template must contain exactly one root Ui component");
  const raw = { apiVersion: UI_IR_API_VERSION, root: parseNode(significant[0], "root") };
  try {
    // Build-time parsing validates the artifact shape and limits. Permission
    // policy is applied again by the Host after manifest hydration.
    return parseUiIR(raw, { permissions: { requireActionAllowlist: false } });
  } catch (error) {
    const candidate = error as { code?: unknown; path?: unknown; message?: unknown };
    if (typeof candidate.code === "string") {
      fail(candidate.code, typeof candidate.path === "string" ? candidate.path : "artifact", typeof candidate.message === "string" ? candidate.message : String(error));
    }
    throw error;
  }
}

export function compileUiIrSfcArtifact(source: string, filename = "plugin-ui.vue"): string {
  return JSON.stringify(compileUiIrSfc(source, filename)) + "\n";
}
