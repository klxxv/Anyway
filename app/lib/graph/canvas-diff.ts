/**
 * Canvas Diff 前端契约与计算。
 * TS-side contract + computation for the kernel-layer Canvas Diff.
 *
 * 桌面端：调用 Host SDK `graph.diff` operation（确定性、与内核一致）。
 * Web 端：本地 fallback 实现同一算法（claim 字段选择 + 规范化 + SHA-256 前 12 hex），
 * 仅用于浏览器无桌面桥时的交互演示；两端独立运行，互不交叉比对。
 */

import type { ProjectState, ResearchEdge, ResearchNode } from "../research-types";
import { HostSdk } from "../../platform/host-sdk";
import { createDefaultTauriHostSdkTransport } from "../../platform/host-sdk-tauri";

/** 与 Rust `CanvasDiffResult`（camelCase）对齐的 TS 契约（canvas-diff-design.md §2.2）。 */
export interface CanvasDiffResult {
  addedNodes: string[];
  removedNodes: string[];
  modifiedNodes: ModifiedEntity[];
  addedEdges: string[];
  removedEdges: string[];
  modifiedEdges: ModifiedEntity[];
  addedEvidence: string[];
  removedEvidence: string[];
  modifiedEvidence: ModifiedEntity[];
  changedBlockHashes: Record<string, [string, string]>;
  durationMs: number;
}

export interface ModifiedEntity {
  entityId: string;
  entityKind: "nodes" | "edges" | "evidence";
  oldBlockHash: string;
  newBlockHash: string;
  changedFields: string[];
}

/** 画布 diff overlay 的输入；每个参数可为 ProjectState 或带 fileHash 自校验的包装。 */
export type DiffInput = ProjectState | { fileHash: string; project: ProjectState };

/** 画布叠加模式的状态：三色标记 + 幽灵实体（removed 在 compare 版本中不存在）。 */
export type DiffState = "added" | "removed" | "modified";

export interface DiffOverlayState {
  /** nodeId → 变更状态（仅 compare 版本存在的节点）。 */
  nodes: Record<string, DiffState>;
  /** edgeId → 变更状态（仅 compare 版本存在的边）。 */
  edges: Record<string, DiffState>;
  /** 从 base 版本注入的幽灵节点（removed，带 base 坐标）。 */
  removedNodes: Array<{ record: ResearchNode; x: number; y: number }>;
  /** 从 base 版本注入的幽灵边（removed，端点是幽灵或现存节点）。 */
  removedEdges: Array<{ record: ResearchEdge }>;
}

/** 空 diff 结果（幂等保证的基线）。 */
export function emptyDiffResult(): CanvasDiffResult {
  return {
    addedNodes: [],
    removedNodes: [],
    modifiedNodes: [],
    addedEdges: [],
    removedEdges: [],
    modifiedEdges: [],
    addedEvidence: [],
    removedEvidence: [],
    modifiedEvidence: [],
    changedBlockHashes: {},
    durationMs: 0,
  };
}

const hasTauriRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let desktopHostSdk: HostSdk | undefined;

function getDesktopHostSdk(): HostSdk {
  desktopHostSdk ??= new HostSdk(createDefaultTauriHostSdkTransport());
  return desktopHostSdk;
}

/**
 * 计算两个版本的结构化 diff。
 * Desktop: delegates to the Rust kernel via the `graph.diff` Host SDK operation;
 * Web: local fallback.
 */
export async function computeCanvasDiff(v1: DiffInput, v2: DiffInput): Promise<CanvasDiffResult> {
  if (hasTauriRuntime()) {
    return getDesktopHostSdk().call<CanvasDiffResult>("graph.diff", { v1, v2 });
  }
  return computeLocalDiff(v1, v2);
}

// ---------------------------------------------------------------------------
// 本地 fallback（与 Rust 算法同构：claim → canonicalize → SHA-256 前 12 hex）
// ---------------------------------------------------------------------------

const NODE_CLAIM_FIELDS = ["id", "type", "title", "body", "tags", "data"] as const;
const EDGE_CLAIM_FIELDS = [
  "id",
  "type",
  "source",
  "target",
  "directed",
  "polarity",
  "confidence",
  "conditions",
  "note",
  "experiment",
] as const;
const EVIDENCE_CLAIM_FIELDS = ["id", "sourceType", "sourceId", "title", "authors", "year", "doi", "url"] as const;

function pickFields(entity: unknown, fields: readonly string[]): Record<string, unknown> {
  const claim: Record<string, unknown> = {};
  if (entity && typeof entity === "object") {
    for (const field of fields) {
      const object = entity as Record<string, unknown>;
      if (field in object) claim[field] = object[field];
    }
  }
  return claim;
}

function claimFor(kind: string, entity: unknown): Record<string, unknown> {
  if (kind === "nodes") return pickFields(entity, NODE_CLAIM_FIELDS);
  if (kind === "edges") return pickFields(entity, EDGE_CLAIM_FIELDS);
  return pickFields(entity, EVIDENCE_CLAIM_FIELDS);
}

function compareBytes(a: string, b: string): number {
  const aBytes = new TextEncoder().encode(a);
  const bBytes = new TextEncoder().encode(b);
  const length = Math.min(aBytes.length, bBytes.length);
  for (let index = 0; index < length; index += 1) {
    if (aBytes[index] !== bBytes[index]) return aBytes[index] < bBytes[index] ? -1 : 1;
  }
  return aBytes.length - bBytes.length;
}

/** 与 Rust canonicalize 同构的规范化字符串（无 Buffer 依赖，浏览器安全）。 */
export function canonicalizeDiffValue(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number") {
    if (Number.isInteger(value) && Math.abs(value) < 9_007_199_254_740_992) return String(value);
    return String(value);
  }
  if (typeof value === "string") {
    const folded = value
      .normalize("NFC")
      .split(/\s+/)
      .filter((part) => part.length > 0)
      .join(" ");
    return JSON.stringify(folded);
  }
  if (Array.isArray(value)) {
    const items = value.map(canonicalizeDiffValue).sort(compareBytes);
    return `[${items.join(",")}]`;
  }
  if (typeof value === "object") {
    const entries = Object.entries(value as Record<string, unknown>).map(([key, item]) => [
      key.normalize("NFC"),
      item,
    ] as const);
    const unique = new Map<string, unknown>();
    for (const [key, item] of entries) unique.set(key, item);
    const parts = [...unique.keys()].sort(compareBytes).map((key) => {
      return `${JSON.stringify(key)}:${canonicalizeDiffValue(unique.get(key))}`;
    });
    return `{${parts.join(",")}}`;
  }
  throw new Error(`cannot canonicalize value of type ${typeof value}`);
}

async function sha256Hex(input: string): Promise<string> {
  const bytes = new TextEncoder().encode(input);
  const digest = await crypto.subtle.digest("SHA-256", bytes);
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

async function blockHash(entity: unknown, kind: string): Promise<string> {
  const claim = claimFor(kind, entity);
  const canonical = canonicalizeDiffValue(claim);
  return (await sha256Hex(canonical)).slice(0, 12);
}

interface DiffZoneResult {
  added: string[];
  removed: string[];
  modified: ModifiedEntity[];
}

async function diffZone(
  v1: unknown,
  v2: unknown,
  kind: string,
  changed: Record<string, [string, string]>,
): Promise<DiffZoneResult> {
  const entities1 = Array.isArray((v1 as Record<string, unknown>)?.[kind]) ? (v1 as Record<string, unknown>)[kind] as unknown[] : [];
  const entities2 = Array.isArray((v2 as Record<string, unknown>)?.[kind]) ? (v2 as Record<string, unknown>)[kind] as unknown[] : [];
  const byId1 = new Map<string, unknown>();
  const byId2 = new Map<string, unknown>();
  for (const entity of entities1) {
    const id = (entity as Record<string, unknown>).id;
    if (typeof id === "string") byId1.set(id, entity);
  }
  for (const entity of entities2) {
    const id = (entity as Record<string, unknown>).id;
    if (typeof id === "string") byId2.set(id, entity);
  }
  const added: string[] = [];
  const removed: string[] = [];
  const modified: ModifiedEntity[] = [];
  for (const [id, entity2] of [...byId2.entries()].sort(([a], [b]) => compareBytes(a, b))) {
    const entity1 = byId1.get(id);
    if (entity1 === undefined) {
      added.push(id);
      changed[id] = ["", await blockHash(entity2, kind)];
      continue;
    }
    const oldHash = await blockHash(entity1, kind);
    const newHash = await blockHash(entity2, kind);
    if (oldHash !== newHash) {
      modified.push({
        entityId: id,
        entityKind: kind as ModifiedEntity["entityKind"],
        oldBlockHash: oldHash,
        newBlockHash: newHash,
        changedFields: changedClaimFields(entity1, entity2, kind),
      });
      changed[id] = [oldHash, newHash];
    }
  }
  for (const [id, entity1] of [...byId1.entries()].sort(([a], [b]) => compareBytes(a, b))) {
    if (!byId2.has(id)) {
      removed.push(id);
      changed[id] = [await blockHash(entity1, kind), ""];
    }
  }
  return { added, removed, modified };
}

function changedClaimFields(entity1: unknown, entity2: unknown, kind: string): string[] {
  const claim1 = claimFor(kind, entity1);
  const claim2 = claimFor(kind, entity2);
  const keys = new Set([...Object.keys(claim1), ...Object.keys(claim2)]);
  return [...keys].filter((key) => {
    return canonicalizeDiffValue(claim1[key]) !== canonicalizeDiffValue(claim2[key]);
  });
}

/** 本地 fallback 实现；与 Rust `canvas_diff` 同构。 */
export async function computeLocalDiff(v1: DiffInput, v2: DiffInput): Promise<CanvasDiffResult> {
  const project1 = unwrapDiffInput(v1);
  const project2 = unwrapDiffInput(v2);
  const started = performance.now();
  const changed: Record<string, [string, string]> = {};
  const [nodes, edges, evidence] = await Promise.all([
    diffZone(project1, project2, "nodes", changed),
    diffZone(project1, project2, "edges", changed),
    diffZone(project1, project2, "evidence", changed),
  ]);
  return {
    addedNodes: nodes.added,
    removedNodes: nodes.removed,
    modifiedNodes: nodes.modified,
    addedEdges: edges.added,
    removedEdges: edges.removed,
    modifiedEdges: edges.modified,
    addedEvidence: evidence.added,
    removedEvidence: evidence.removed,
    modifiedEvidence: evidence.modified,
    changedBlockHashes: changed,
    durationMs: Math.round(performance.now() - started),
  };
}

function unwrapDiffInput(input: DiffInput): unknown {
  if (typeof input === "object" && input !== null && "project" in input) {
    return (input as { project: ProjectState }).project;
  }
  return input;
}

// ---------------------------------------------------------------------------
// 画布 overlay 构建（叠加模式的输入）
// ---------------------------------------------------------------------------

/** 由 diff 结果与 base/compare 两个版本构建画布 overlay 状态。 */
export function buildDiffOverlay(
  result: CanvasDiffResult,
  base: ProjectState,
  compare: ProjectState,
): DiffOverlayState {
  const nodes: Record<string, DiffState> = {};
  const edges: Record<string, DiffState> = {};
  for (const id of result.addedNodes) nodes[id] = "added";
  for (const id of result.removedNodes) nodes[id] = "removed";
  for (const entity of result.modifiedNodes) nodes[entity.entityId] = "modified";
  for (const id of result.addedEdges) edges[id] = "added";
  for (const id of result.removedEdges) edges[id] = "removed";
  for (const entity of result.modifiedEdges) edges[entity.entityId] = "modified";

  const removedById = new Map(result.removedNodes.map((id) => [id, id]));
  const removedNodes = base.nodes
    .filter((node) => removedById.has(node.id))
    .map((record) => {
      const placement = base.placements.find((item) => item.nodeId === record.id);
      return {
        record,
        x: placement?.x ?? 0,
        y: placement?.y ?? 0,
      };
    });

  const removedEdgeIds = new Set(result.removedEdges);
  const presentNodeIds = new Set([
    ...compare.nodes.map((node) => node.id),
    ...removedNodes.map((node) => node.record.id),
  ]);
  const removedEdges = base.edges
    .filter((edge) => removedEdgeIds.has(edge.id))
    .filter((edge) => presentNodeIds.has(edge.source) && presentNodeIds.has(edge.target))
    .map((record) => ({ record }));

  return { nodes, edges, removedNodes, removedEdges };
}
