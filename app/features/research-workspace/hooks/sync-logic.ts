import type { ProjectState } from "../../../lib/research-types";
import { cloneProject } from "./commit-logic";

/**
 * Storage abstraction and hydration/sync logic for the workspace project.
 * 存储后端抽象与项目水化/同步逻辑；通过注入后端实现可独立测试与切换。
 */

/** 默认本地存储键 / Default localStorage key for the bundled Zen workspace. */
export const PROJECT_STORAGE_KEY = "research-canvas.zen-workspace.v1";

/** 可切换的持久化后端接口；默认 localStorage，可替换为 Tauri FS / IndexedDB 等。 */
export interface ProjectStorage {
  load(): ProjectState | null;
  save(project: ProjectState): void;
  clear(): void;
}

type StorageBackend = Pick<Storage, "getItem" | "setItem" | "removeItem">;

/**
 * Creates the default localStorage-backed storage. The backend is resolved lazily
 * so this factory is safe to call during render (SSR) and in Node tests.
 * 创建默认 localStorage 后端；后端延迟解析，渲染期与 Node 测试中均安全。
 */
export function createLocalStorageProjectStorage(
  key = PROJECT_STORAGE_KEY,
  storage?: StorageBackend,
): ProjectStorage {
  const backend = () => storage ?? globalThis.localStorage;
  return {
    load() {
      const raw = backend().getItem(key);
      if (raw === null) return null;
      return parseStoredProject(raw);
    },
    save(project) {
      backend().setItem(key, JSON.stringify(project));
    },
    clear() {
      backend().removeItem(key);
    },
  };
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

/**
 * 轻量结构校验：保证关键字段存在且类型正确，避免损坏/恶意数据污染状态。
 * Lightweight structural validation: ensures key fields exist and have correct types.
 */
export function validateProjectState(value: unknown): ProjectState | null {
  if (!isPlainObject(value)) return null;
  if (typeof value.schemaVersion !== "number") return null;
  if (typeof value.id !== "string" || value.id.length === 0) return null;
  if (typeof value.title !== "string") return null;
  if (typeof value.discipline !== "string") return null;
  if (typeof value.updatedAt !== "string") return null;
  if (typeof value.revision !== "number") return null;
  if (!Array.isArray(value.nodes)) return null;
  if (!Array.isArray(value.edges)) return null;
  if (!Array.isArray(value.evidence)) return null;
  if (!Array.isArray(value.placements)) return null;
  if (!Array.isArray(value.scenarios)) return null;
  if (!Array.isArray(value.activity)) return null;

  for (const node of value.nodes) {
    if (!isPlainObject(node)) return null;
    if (typeof node.id !== "string" || node.id.length === 0) return null;
    if (typeof node.type !== "string") return null;
    if (typeof node.title !== "string") return null;
    if (typeof node.body !== "string") return null;
    if (!Array.isArray(node.tags)) return null;
    if (typeof node.status !== "string") return null;
    if (!Array.isArray(node.evidenceIds)) return null;
    if (!isPlainObject(node.data)) return null;
    if (!isPlainObject(node.provenance)) return null;
    if (typeof node.createdAt !== "string") return null;
    if (typeof node.updatedAt !== "string") return null;
  }

  for (const edge of value.edges) {
    if (!isPlainObject(edge)) return null;
    if (typeof edge.id !== "string" || edge.id.length === 0) return null;
    if (typeof edge.type !== "string") return null;
    if (typeof edge.source !== "string") return null;
    if (typeof edge.target !== "string") return null;
    if (typeof edge.directed !== "boolean") return null;
    if (typeof edge.polarity !== "string") return null;
    if (!Array.isArray(edge.conditions)) return null;
    if (!Array.isArray(edge.evidenceIds)) return null;
    if (!isPlainObject(edge.provenance)) return null;
  }

  for (const placement of value.placements) {
    if (!isPlainObject(placement)) return null;
    if (typeof placement.id !== "string" || placement.id.length === 0) return null;
    if (typeof placement.viewId !== "string") return null;
    if (typeof placement.nodeId !== "string") return null;
    if (typeof placement.x !== "number" || !Number.isFinite(placement.x)) return null;
    if (typeof placement.y !== "number" || !Number.isFinite(placement.y)) return null;
    if (typeof placement.width !== "number" || !Number.isFinite(placement.width)) return null;
    if (typeof placement.height !== "number" || !Number.isFinite(placement.height)) return null;
  }

  const navigation = value.navigation;
  if (navigation !== undefined) {
    if (!isPlainObject(navigation)) return null;
    if (!isStringArray(navigation.recentNodeIds)) return null;
    if (!isStringArray(navigation.pinnedNodeIds)) return null;
  }

  return value as unknown as ProjectState;
}

/** 解析原始 JSON；损坏数据返回 null 而非抛出 / Parses raw JSON; corrupt data yields null. */
export function parseStoredProject(raw: string): ProjectState | null {
  try {
    const parsed = JSON.parse(raw) as unknown;
    return validateProjectState(parsed);
  } catch {
    return null;
  }
}

/**
 * Migrates only the bundled example; unrelated user projects remain untouched.
 * 仅迁移内置示例（schemaVersion < 2）；用户创建的其他项目原样保留。
 */
export function resolveHydratedProject(
  saved: ProjectState,
  fixture: ProjectState,
): ProjectState {
  return saved.id === fixture.id && saved.schemaVersion < 2 ? cloneProject(fixture) : saved;
}

/** 从存储加载并解析项目；无数据或数据损坏时返回 null / Loads and resolves a project from storage. */
export function hydrateFromStorage(
  storage: ProjectStorage,
  fixture: ProjectState,
): ProjectState | null {
  const saved = storage.load();
  if (!saved) return null;
  return resolveHydratedProject(saved, fixture);
}
