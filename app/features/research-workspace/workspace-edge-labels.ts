import type { MessageKey } from "../../i18n/catalog";
import {
  EDGE_TYPES,
  type ResearchEdge,
  type ResearchEdgeType,
} from "../../lib/research-types";

/** 全量关系类型词条映射 / Complete i18n mapping for persisted relation semantics. */
export const edgeTypeMessageKeys: Record<ResearchEdgeType, MessageKey> = {
  causes: "edgeType.causes",
  correlates: "edgeType.correlates",
  supports: "edgeType.supports",
  contradicts: "edgeType.contradicts",
  depends_on: "edgeType.dependsOn",
  derived_from: "edgeType.derivedFrom",
  part_of: "edgeType.partOf",
  controls: "edgeType.controls",
  mediates: "edgeType.mediates",
  moderates: "edgeType.moderates",
  uses: "edgeType.uses",
  measures: "edgeType.measures",
};

export const editableEdgeTypes = [...EDGE_TYPES];

/** 旧版本曾把英文类型名写入 note；把它视为可本地化默认值，而非用户文案。 / Legacy raw type notes remain localizable defaults, not user copy. */
export function customEdgeNote(edge: ResearchEdge): string {
  const note = edge.note?.trim() ?? "";
  return note === edge.type.replaceAll("_", " ") ? "" : note;
}
