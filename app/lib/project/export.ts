import type { ProjectState } from "../research-types";

/** 将项目映射为 Obsidian JSON Canvas 的最小兼容结构 / Maps a project to minimal Obsidian JSON Canvas. */
export function exportJsonCanvas(project: ProjectState) {
  const nodes = project.nodes.map((node) => {
    const placement = project.placements.find((item) => item.nodeId === node.id);
    return {
      id: node.id,
      type: "text",
      text: `# ${node.title}\n\n${node.body}\n\n**Type:** ${node.type}\n**Status:** ${node.status}`,
      x: Math.round(placement?.x ?? 0),
      y: Math.round(placement?.y ?? 0),
      width: Math.round(placement?.width ?? 230),
      height: Math.round(placement?.height ?? 110),
      color: node.type === "question" ? "4" : node.type === "result" ? "5" : undefined,
    };
  });
  const edges = project.edges.map((edge) => ({
    id: edge.id,
    fromNode: edge.source,
    toNode: edge.target,
    fromSide: "right",
    toSide: "left",
    label: edge.type,
  }));
  return { nodes, edges };
}

/** 导出人类可读的研究摘要，而不丢失语义引用 / Exports a readable summary while retaining semantic references. */
export function exportMarkdown(project: ProjectState) {
  const byId = new Map(project.nodes.map((node) => [node.id, node]));
  return [
    `# ${project.title}`,
    "",
    `> ${project.nodes.length} nodes · ${project.edges.length} relations · ${project.evidence.length} evidence records`,
    "",
    "## Research nodes",
    "",
    ...project.nodes.flatMap((node) => [
      `### ${node.title}`,
      "",
      `- Type: ${node.type}`,
      `- Status: ${node.status}`,
      `- Tags: ${node.tags.join(", ") || "—"}`,
      `- Evidence: ${node.evidenceIds.length}`,
      "",
      node.body,
      "",
    ]),
    "## Relations",
    "",
    ...project.edges.map(
      (edge) =>
        `- ${byId.get(edge.source)?.title ?? edge.source} — **${edge.type}** → ${byId.get(edge.target)?.title ?? edge.target}`,
    ),
  ].join("\n");
}

/** RFC 4180 风格单元格转义 / RFC 4180-style CSV cell escaping. */
const csvCell = (value: unknown) =>
  `"${String(value ?? "")
    .replaceAll("\r\n", "\n")
    .replaceAll("\r", "\n")
    .replaceAll('"', '""')}"`;

/**
 * 按节点或关系导出审计友好的 CSV；嵌套字段显式扁平化。
 * Exports audit-friendly node or edge CSV with explicit flattening of nested fields.
 */
export function exportCsv(project: ProjectState, kind: "nodes" | "edges") {
  if (kind === "nodes") {
    return [
      ["id", "type", "title", "status", "tags", "evidence_count", "body"].map(csvCell).join(","),
      ...project.nodes.map((node) =>
        [
          node.id,
          node.type,
          node.title,
          node.status,
          node.tags.join("|"),
          node.evidenceIds.length,
          node.body,
        ]
          .map(csvCell)
          .join(","),
      ),
    ].join("\n");
  }

  return [
    [
      "id",
      "source",
      "target",
      "type",
      "directed",
      "polarity",
      "confidence",
      "evidence_count",
      "experiment_id",
      "experiment_delta",
      "experiment_verdict",
    ]
      .map(csvCell)
      .join(","),
    ...project.edges.map((edge) =>
      [
        edge.id,
        edge.source,
        edge.target,
        edge.type,
        edge.directed,
        edge.polarity,
        edge.confidence,
        edge.evidenceIds.length,
        edge.experiment?.id,
        edge.experiment?.delta,
        edge.experiment?.outcome,
      ]
        .map(csvCell)
        .join(","),
    ),
  ].join("\n");
}
