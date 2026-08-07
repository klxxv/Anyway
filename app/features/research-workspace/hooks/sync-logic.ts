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

/** 解析原始 JSON；损坏数据返回 null 而非抛出 / Parses raw JSON; corrupt data yields null. */
export function parseStoredProject(raw: string): ProjectState | null {
  try {
    return JSON.parse(raw) as ProjectState;
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
