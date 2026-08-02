import type { ResearchNode } from "../../../lib/research-types";

/** Returns display-safe branches only for finite enum and boolean variables. */
export function variableBranchValues(node: ResearchNode): string[] {
  if (node.type !== "variable") return [];
  if (node.data.valueType === "bool") return ["true", "false"];
  if (node.data.valueType !== "enum" || !Array.isArray(node.data.enumValues)) return [];
  return node.data.enumValues
    .filter((value) => ["string", "number", "boolean"].includes(typeof value))
    .map(String)
    .filter((value, index, values) => value.length > 0 && values.indexOf(value) === index)
    .slice(0, 12);
}

export function isExpandableVariable(node: ResearchNode): boolean {
  return variableBranchValues(node).length > 0;
}
