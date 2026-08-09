import type {
  ProjectState,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../../lib/research-types";
import { EDGE_TYPES, NODE_TYPES } from "../../../lib/research-types";
import type { GraphPatchOperation, PluginGraphPatch } from "../../../plugins/contracts";
import { makeId } from "./commit-logic";

/**
 * Pure application of review-gated plugin GraphPatches onto a draft.
 * 将需审阅的插件 GraphPatch 纯函数式地应用到草稿；插件永不直接修改存储。
 */

const SAFE_PLUGIN_ID = /^[a-zA-Z0-9._-]{1,160}$/;

/** 校验 pluginId 是否可安全写入 provenance；防止注入或超长字符串。 */
export function sanitizePluginActorId(pluginId: string): string | null {
  if (!SAFE_PLUGIN_ID.test(pluginId)) return null;
  return pluginId;
}

/** 操作阶段：先加节点，再更新，最后加边（保证边引用的端点已存在）。 */
const PATCH_PHASE = { "add-node": 0, "update-node": 1, "add-edge": 2, "update-edge": 3 } as const;

/**
 * Orders operations by phase so referenced nodes exist before edges reference them.
 * 同一阶段内保持原始相对顺序（Array.prototype.sort 稳定）。
 */
export function sortGraphPatchOperations(
  operations: GraphPatchOperation[],
): GraphPatchOperation[] {
  return [...operations].sort(
    (left, right) => PATCH_PHASE[left.op] - PATCH_PHASE[right.op],
  );
}

export type PatchApplyResult = {
  applied: number;
  skipped: number;
  byOp: Record<string, { applied: number; skipped: number }>;
};

/**
 * Applies every operation of a portable GraphPatch onto the draft in place.
 * 将 GraphPatch 的每个操作应用到草稿（就地变更），并追加一条导入活动记录。
 * 返回实际应用/跳过的计数，避免"0 effective also reports applied"。
 */
export function applyGraphPatchToDraft(
  draft: ProjectState,
  patch: PluginGraphPatch,
  now: string,
): PatchApplyResult {
  const actorId = sanitizePluginActorId(patch.source.pluginId) ?? "unknown-plugin";
  const result: PatchApplyResult = { applied: 0, skipped: 0, byOp: {} };
  const bump = (op: string, applied: boolean) => {
    const entry = (result.byOp[op] ??= { applied: 0, skipped: 0 });
    if (applied) {
      result.applied += 1;
      entry.applied += 1;
    } else {
      result.skipped += 1;
      entry.skipped += 1;
    }
  };

  for (const operation of sortGraphPatchOperations(patch.operations)) {
    if (operation.op === "add-node") {
      if (draft.nodes.some((node) => node.id === operation.node.id)) {
        bump(operation.op, false);
        continue;
      }
      if (!NODE_TYPES.includes(operation.node.type as (typeof NODE_TYPES)[number])) {
        bump(operation.op, false);
        continue;
      }
      const index = draft.nodes.length;
      draft.nodes.push({
        id: operation.node.id,
        type: operation.node.type as ResearchNodeType,
        title: operation.node.title,
        body: operation.node.body ?? "Imported through a reviewed plugin GraphPatch.",
        tags: operation.node.tags ?? [],
        status: "draft",
        evidenceIds: [],
        data: operation.node.data ?? {},
        provenance: {
          origin: "import",
          actorId,
          sourceRefs: patch.source.externalId ? [patch.source.externalId] : [],
        },
        createdAt: now,
        updatedAt: now,
      });
      draft.placements.push({
        id: `placement-${operation.node.id}`,
        viewId: "view-main",
        nodeId: operation.node.id,
        x: 120 + (index % 5) * 220,
        y: 140 + Math.floor(index / 5) * 160,
        width: operation.node.type === "question" ? 136 : 176,
        height: operation.node.type === "question" ? 136 : 118,
      });
      bump(operation.op, true);
    } else if (operation.op === "add-edge") {
      if (
        draft.edges.some((edge) => edge.id === operation.edge.id) ||
        !EDGE_TYPES.includes(operation.edge.type as (typeof EDGE_TYPES)[number]) ||
        !draft.nodes.some((node) => node.id === operation.edge.source) ||
        !draft.nodes.some((node) => node.id === operation.edge.target)
      ) {
        bump(operation.op, false);
        continue;
      }
      draft.edges.push({
        id: operation.edge.id,
        type: operation.edge.type as ResearchEdgeType,
        source: operation.edge.source,
        target: operation.edge.target,
        directed: true,
        polarity: operation.edge.type === "contradicts" ? "negative" : "positive",
        conditions: [],
        evidenceIds: [],
        note: operation.edge.note,
        provenance: { origin: "import", actorId },
      });
      bump(operation.op, true);
    } else if (operation.op === "update-node") {
      const node = draft.nodes.find((item) => item.id === operation.nodeId);
      if (!node) {
        bump(operation.op, false);
        continue;
      }
      if (typeof operation.changes.title === "string") node.title = operation.changes.title;
      if (typeof operation.changes.body === "string") node.body = operation.changes.body;
      if (
        Array.isArray(operation.changes.tags) &&
        operation.changes.tags.every((tag) => typeof tag === "string")
      ) {
        node.tags = operation.changes.tags;
      }
      if (
        operation.changes.data &&
        typeof operation.changes.data === "object" &&
        !Array.isArray(operation.changes.data)
      ) {
        node.data = { ...node.data, ...operation.changes.data };
      }
      node.updatedAt = now;
      bump(operation.op, true);
    } else if (operation.op === "update-edge") {
      const edge = draft.edges.find((item) => item.id === operation.edgeId);
      if (!edge) {
        bump(operation.op, false);
        continue;
      }
      if (typeof operation.changes.note === "string") edge.note = operation.changes.note;
      if (
        typeof operation.changes.type === "string" &&
        EDGE_TYPES.includes(operation.changes.type as ResearchEdgeType)
      ) {
        edge.type = operation.changes.type as ResearchEdgeType;
      }
      if (
        typeof operation.changes.confidence === "number" &&
        operation.changes.confidence >= 0 &&
        operation.changes.confidence <= 1
      ) {
        edge.confidence = operation.changes.confidence;
      }
      bump(operation.op, true);
    }
  }

  const effective = result.applied;
  draft.activity.push({
    id: makeId("activity"),
    label:
      effective === 0
        ? `${patch.title} · no effective operations (${result.skipped} skipped)`
        : `${patch.title} · ${effective} applied · ${result.skipped} skipped`,
    origin: "import",
    createdAt: now,
  });

  return result;
}
