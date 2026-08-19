import {
  convergeLegacyEdgeType,
  NODE_TYPES,
  type ProjectState,
  type ResearchEdgeType,
} from "./research-types";

export const projectFileExtensions = ["mycproj", "json"] as const;

export interface NativeProjectFileResult {
  path: string;
  bytes: number;
  savedAt?: string;
  project?: unknown;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

/** 结构化导入闸门；语义迁移保持显式和版本化 / Structural import gate; semantic migrations remain explicit and versioned. */
export function isProjectState(value: unknown): value is ProjectState {
  if (!isRecord(value)) return false;
  if (
    value.schemaVersion !== 2 ||
    typeof value.id !== "string" ||
    value.id.length === 0 ||
    typeof value.title !== "string" ||
    typeof value.discipline !== "string" ||
    typeof value.updatedAt !== "string" ||
    !Number.isInteger(value.revision) ||
    !Array.isArray(value.nodes) ||
    !Array.isArray(value.edges) ||
    !Array.isArray(value.evidence) ||
    !Array.isArray(value.placements) ||
    !Array.isArray(value.scenarios) ||
    !Array.isArray(value.activity)
  ) {
    return false;
  }
  const nodeIds = new Set<string>();
  for (const node of value.nodes) {
    if (
      !isRecord(node) ||
      typeof node.id !== "string" ||
      nodeIds.has(node.id) ||
      !NODE_TYPES.includes(node.type as (typeof NODE_TYPES)[number]) ||
      typeof node.title !== "string" ||
      typeof node.body !== "string" ||
      !Array.isArray(node.tags) ||
      !node.tags.every((tag) => typeof tag === "string") ||
      !isRecord(node.data)
    ) {
      return false;
    }
    nodeIds.add(node.id);
  }
  const edgeIds = new Set<string>();
  for (const edge of value.edges) {
    if (
      !isRecord(edge) ||
      typeof edge.id !== "string" ||
      edgeIds.has(edge.id) ||
      (typeof edge.type !== "string" || convergeLegacyEdgeType(edge.type) === null) ||
      typeof edge.source !== "string" ||
      typeof edge.target !== "string" ||
      !nodeIds.has(edge.source) ||
      !nodeIds.has(edge.target)
    ) {
      return false;
    }
    edgeIds.add(edge.id);
  }
  const placementIds = new Set<string>();
  for (const placement of value.placements) {
    if (
      !isRecord(placement) ||
      typeof placement.id !== "string" ||
      placementIds.has(placement.id) ||
      typeof placement.nodeId !== "string" ||
      !nodeIds.has(placement.nodeId) ||
      ![placement.x, placement.y, placement.width, placement.height].every(Number.isFinite)
    ) {
      return false;
    }
    placementIds.add(placement.id);
  }
  return true;
}

/** Migrate legacy 12-type edges onto the five-operator basis in place. */
export function migrateLegacyEdgeTypes(project: ProjectState): ProjectState {
  const migrate = (type: string | undefined): string | undefined => {
    if (typeof type !== "string") return type;
    return convergeLegacyEdgeType(type) ?? type;
  };
  return {
    ...project,
    edges: project.edges.map((edge) => ({
      ...edge,
      type: migrate(edge.type) as ResearchEdgeType,
    })),
    scenarios: project.scenarios.map((scenario) => ({
      ...scenario,
      edgeOverrides: Object.fromEntries(
        Object.entries(scenario.edgeOverrides).map(([id, override]) => [
          id,
          override.type ? { ...override, type: migrate(override.type) as ResearchEdgeType } : override,
        ]),
      ),
    })),
  };
}

export function parseProjectText(text: string): ProjectState {
  if (text.length > 32 * 1024 * 1024) throw new Error("PROJECT_FILE_TOO_LARGE");
  const value: unknown = JSON.parse(text);
  if (!isProjectState(value)) throw new Error("PROJECT_FILE_INVALID");
  return migrateLegacyEdgeTypes(value);
}

export function projectFileStem(project: Pick<ProjectState, "title" | "id">) {
  const stem = project.title
    .trim()
    .toLowerCase()
    .replaceAll(/[^a-z0-9\u4e00-\u9fff]+/g, "-")
    .replaceAll(/^-+|-+$/g, "");
  return Array.from(stem || project.id || "research-project").slice(0, 80).join("");
}
