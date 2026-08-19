import type { MessageKey } from "../../i18n/catalog";
import {
  EDGE_TYPES,
  type ResearchEdge,
  type ResearchEdgeType,
} from "../../lib/research-types";

/** 全量关系类型词条映射 / Complete i18n mapping for the five operators. */
export const edgeTypeMessageKeys: Record<ResearchEdgeType, MessageKey> = {
  T: "edgeType.transform",
  K: "edgeType.kernel",
  I: "edgeType.intervention",
  M: "edgeType.marginalize",
  Q: "edgeType.quotient",
};

export const editableEdgeTypes = [...EDGE_TYPES];

/** 旧版本曾把英文类型名写入 note；把它视为可本地化默认值，而非用户文案。 / Legacy raw type notes remain localizable defaults, not user copy. */
export function customEdgeNote(edge: ResearchEdge): string {
  const note = edge.note?.trim() ?? "";
  return note === edge.type.replaceAll("_", " ") ? "" : note;
}
