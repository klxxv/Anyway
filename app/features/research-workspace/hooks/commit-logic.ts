import { computeLayout } from "../../../lib/layout";
import type {
  LayoutMode,
  ProjectState,
  ResearchEdgeType,
} from "../../../lib/research-types";
import { projectForLegendFilter, type LinkLegendFilter } from "../workspace-layout";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  NodeDraft,
  WorkspaceHistory,
} from "../workspace-types";

/**
 * Pure, framework-free graph mutation and history logic behind the workspace hook.
 * 不依赖 React 的纯图谱变换与历史栈逻辑，可直接进行单元测试。
 */

/** 撤销栈保留的最大快照数 / Maximum undo checkpoints kept on the past stack. */
export const HISTORY_LIMIT = 40;

/** 深克隆任意可序列化值；用于历史快照与草稿副本 / Deep-clones any serializable value. */
export function cloneProject<T>(project: T): T {
  return JSON.parse(JSON.stringify(project)) as T;
}

/** 生成带时间戳与随机后缀的稳定 id / Generates a stable id with timestamp and random suffix. */
export function makeId(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

/** 将草稿标记为聚合的新修订 / Stamps the draft as a new revision of the aggregate. */
export function stampDraftRevision(draft: ProjectState, now: string): void {
  draft.revision += 1;
  draft.updatedAt = now;
}

/** 追加一个撤销检查点，最多保留 HISTORY_LIMIT 条 / Appends an undo checkpoint, capped at HISTORY_LIMIT. */
export function pushHistoryEntry(
  past: WorkspaceHistory[],
  entry: WorkspaceHistory,
): WorkspaceHistory[] {
  return [...past.slice(-(HISTORY_LIMIT - 1)), entry];
}

/** 一次撤销/重做后历史栈与项目的整体转移结果 / Result of one undo/redo transition. */
export type HistoryTransition = {
  past: WorkspaceHistory[];
  future: WorkspaceHistory[];
  project: ProjectState;
};

/** 弹出一个撤销检查点；空栈时返回 null / Pops one undo checkpoint; null on an empty stack. */
export function undoHistory(
  past: WorkspaceHistory[],
  future: WorkspaceHistory[],
  current: ProjectState,
): HistoryTransition | null {
  const previous = past.at(-1);
  if (!previous) return null;
  return {
    past: past.slice(0, -1),
    future: [{ project: cloneProject(current), label: previous.label }, ...future],
    project: cloneProject(previous.project),
  };
}

/** 重放一个重做检查点；空栈时返回 null / Replays one redo checkpoint; null on an empty stack. */
export function redoHistory(
  past: WorkspaceHistory[],
  future: WorkspaceHistory[],
  current: ProjectState,
): HistoryTransition | null {
  const next = future[0];
  if (!next) return null;
  return {
    past: pushHistoryEntry(past, { project: cloneProject(current), label: next.label }),
    future: future.slice(1),
    project: cloneProject(next.project),
  };
}

// ---------------------------------------------------------------------------
// 面向草稿的纯变换 / Pure draft transforms (mutate the draft in place)
// ---------------------------------------------------------------------------

export function updateNodeInDraft(
  draft: ProjectState,
  nodeId: string,
  update: InspectorUpdate,
  now: string,
): void {
  const node = draft.nodes.find((item) => item.id === nodeId);
  if (!node) return;
  Object.assign(node, update, { updatedAt: now });
}

export function updateEdgeInDraft(
  draft: ProjectState,
  edgeId: string,
  update: EdgeInspectorUpdate,
): void {
  const edge = draft.edges.find((item) => item.id === edgeId);
  if (!edge) return;
  Object.assign(edge, update);
}

export function moveNodeInDraft(
  draft: ProjectState,
  nodeId: string,
  x: number,
  y: number,
): void {
  const placement = draft.placements.find((item) => item.nodeId === nodeId);
  if (!placement) return;
  placement.x = x;
  placement.y = y;
}

export function createNodeInDraft(
  draft: ProjectState,
  id: string,
  draftNode: NodeDraft,
  x: number,
  y: number,
  now: string,
): void {
  draft.nodes.push({
    id,
    type: draftNode.type,
    title: draftNode.title.trim() || `Untitled ${draftNode.type}`,
    body: draftNode.body.trim() || "Add a concise research note.",
    tags: draftNode.tags,
    status: "draft",
    evidenceIds: [],
    data: draftNode.data,
    provenance: { origin: "human", actorId: "local-researcher" },
    createdAt: now,
    updatedAt: now,
  });
  draft.placements.push({
    id: `placement-${id}`,
    viewId: "view-main",
    nodeId: id,
    x,
    y,
    width: draftNode.type === "question" ? 136 : 164,
    height: draftNode.type === "question" ? 136 : 116,
  });
}

export function createEdgeInDraft(
  draft: ProjectState,
  id: string,
  source: string,
  target: string,
  type: ResearchEdgeType,
): void {
  draft.edges.push({
    id,
    source,
    target,
    type,
    directed: true,
    polarity: type === "contradicts" ? "negative" : "positive",
    confidence: 1,
    conditions: [],
    evidenceIds: [],
    provenance: { origin: "human", actorId: "local-researcher" },
  });
}

/** 删除节点并级联清理相关边与布局 / Removes a node and cascades incident edges and placements. */
export function removeNodeInDraft(draft: ProjectState, nodeId: string): void {
  draft.nodes = draft.nodes.filter((node) => node.id !== nodeId);
  draft.edges = draft.edges.filter(
    (edge) => edge.source !== nodeId && edge.target !== nodeId,
  );
  draft.placements = draft.placements.filter((placement) => placement.nodeId !== nodeId);
}

export function removeEdgeInDraft(draft: ProjectState, edgeId: string): void {
  draft.edges = draft.edges.filter((edge) => edge.id !== edgeId);
}

export function reverseEdgeInDraft(draft: ProjectState, edgeId: string): void {
  const edge = draft.edges.find((item) => item.id === edgeId);
  if (!edge) return;
  [edge.source, edge.target] = [edge.target, edge.source];
}

export function duplicateNodeInDraft(
  draft: ProjectState,
  nodeId: string,
  nextId: string,
  now: string,
): void {
  const source = draft.nodes.find((node) => node.id === nodeId);
  const placement = draft.placements.find((item) => item.nodeId === nodeId);
  if (!source || !placement) return;
  draft.nodes.push({
    ...cloneProject(source),
    id: nextId,
    title: `${source.title} copy`,
    provenance: { origin: "human", actorId: "local-researcher" },
    createdAt: now,
    updatedAt: now,
  });
  draft.placements.push({
    ...placement,
    id: `placement-${nextId}`,
    nodeId: nextId,
    x: placement.x + 28,
    y: placement.y + 28,
  });
}

/**
 * 仅对当前可见关系投影布局，并把坐标写入草稿。
 * Layouts only the visible relation projection, then persists positions into the draft.
 */
export function applyLayoutInDraft(
  draft: ProjectState,
  mode: LayoutMode,
  rootId: string | undefined,
  filter: LinkLegendFilter | null,
): void {
  const projected = projectForLegendFilter(draft, filter);
  const effectiveRoot = projected.nodes.some((node) => node.id === rootId)
    ? rootId
    : projected.nodes[0]?.id;
  const result = computeLayout(projected, mode, effectiveRoot);
  const positioned = new Set(Object.keys(result.positions));
  const fallbackNodes = projected.nodes.filter((node) => !positioned.has(node.id));
  const maxY = Math.max(
    80,
    ...Object.values(result.positions).map((position) => position.y),
  );
  fallbackNodes.forEach((node, index) => {
    result.positions[node.id] = {
      x: 80 + (index % 4) * 235,
      y: maxY + 210 + Math.floor(index / 4) * 170,
    };
  });
  draft.placements.forEach((placement) => {
    const position = result.positions[placement.nodeId];
    if (!position) return;
    placement.x = position.x;
    placement.y = position.y;
  });
}
