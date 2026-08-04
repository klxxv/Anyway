import type { ProjectState } from "../research-types";

/** Produces a detached JSON-safe copy for history snapshots and mutations. */
export const cloneProject = (project: ProjectState): ProjectState =>
  JSON.parse(JSON.stringify(project)) as ProjectState;

/** 当前可读取的最新存档版本 / Latest persisted schema this build can read. */
export const CURRENT_SCHEMA_VERSION = 1;

/**
 * 为客户端暂存对象生成可读 ID；它不是加密随机值也不是全局数据库主键。
 * Generates readable IDs for client drafts; not cryptographic randomness or a database key.
 */
export const makeId = (prefix: string) =>
  `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;

/**
 * Normalizes legacy serialized projects into the current, renderable shape.
 * Missing optional collections receive safe defaults; newer schemas are rejected.
 */
export function migrateProject(input: unknown): ProjectState {
  if (!input || typeof input !== "object") {
    throw new Error("Project must be a JSON object.");
  }
  const candidate = input as Partial<ProjectState> & Record<string, unknown>;
  if (!Array.isArray(candidate.nodes) || !Array.isArray(candidate.edges)) {
    throw new Error("Project nodes and edges are required.");
  }
  const sourceVersion =
    typeof candidate.schemaVersion === "number" ? candidate.schemaVersion : 0;
  if (sourceVersion > CURRENT_SCHEMA_VERSION) {
    throw new Error(`Project schema ${sourceVersion} is newer than this application.`);
  }

  const now = new Date().toISOString();
  const legacyNodes = candidate.nodes as Array<
    ProjectState["nodes"][number] & { x?: number; y?: number; width?: number; height?: number }
  >;
  const placements =
    Array.isArray(candidate.placements) && candidate.placements.length
      ? candidate.placements
      : legacyNodes.map((node, index) => ({
          id: `placement-${node.id}`,
          viewId: "view-main",
          nodeId: node.id,
          x: node.x ?? 80 + (index % 4) * 270,
          y: node.y ?? 80 + Math.floor(index / 4) * 160,
          width: node.width ?? 230,
          height: node.height ?? 116,
        }));

  return {
    schemaVersion: CURRENT_SCHEMA_VERSION,
    id: typeof candidate.id === "string" ? candidate.id : makeId("project"),
    title:
      typeof candidate.title === "string" && candidate.title.trim()
        ? candidate.title
        : "Migrated research project",
    discipline:
      typeof candidate.discipline === "string" ? candidate.discipline : "General research",
    revision: typeof candidate.revision === "number" ? candidate.revision : 1,
    updatedAt: typeof candidate.updatedAt === "string" ? candidate.updatedAt : now,
    nodes: candidate.nodes,
    edges: candidate.edges,
    evidence: Array.isArray(candidate.evidence) ? candidate.evidence : [],
    placements,
    scenarios: Array.isArray(candidate.scenarios)
      ? candidate.scenarios.map((scenario) => ({
          ...scenario,
          disabledNodeIds: scenario.disabledNodeIds ?? [],
          disabledEdgeIds: scenario.disabledEdgeIds ?? [],
          nodeOverrides: scenario.nodeOverrides ?? {},
          edgeOverrides: scenario.edgeOverrides ?? {},
          parameters: scenario.parameters ?? {},
        }))
      : [],
    navigation:
      candidate.navigation &&
      typeof candidate.navigation === "object" &&
      Array.isArray((candidate.navigation as ProjectState["navigation"])?.recentNodeIds) &&
      Array.isArray((candidate.navigation as ProjectState["navigation"])?.pinnedNodeIds)
        ? (candidate.navigation as ProjectState["navigation"])
        : { recentNodeIds: [], pinnedNodeIds: [] },
    activity: Array.isArray(candidate.activity) ? candidate.activity : [],
  };
}
