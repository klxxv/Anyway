import type { ProjectState } from "../research-types";

/**
 * 找出引用指定证据的节点和边，以支持证据审计。
 * Finds nodes and edges citing one evidence record for evidence audits.
 */
export function evidenceBacklinks(project: ProjectState, evidenceId: string) {
  return {
    nodeIds: project.nodes
      .filter((node) => node.evidenceIds.includes(evidenceId))
      .map((node) => node.id)
      .sort(),
    edgeIds: project.edges
      .filter((edge) => edge.evidenceIds.includes(evidenceId))
      .map((edge) => edge.id)
      .sort(),
  };
}
