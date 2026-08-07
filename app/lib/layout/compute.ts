import type {
  LayoutMode,
  LayoutResult,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
} from "../research-types";
import { traverseGraph } from "../graph";

/**
 * 在有向无环部分计算分层深度；循环节点保留为未分层。
 * Computes ranks for the directed acyclic portion; cycle members remain unranked.
 */
function topologicalDepths(project: ProjectState, edgeIds?: Set<string>) {
  const nodeIds = new Set(project.nodes.map((node) => node.id));
  const edges = project.edges.filter(
    (edge) => nodeIds.has(edge.source) && nodeIds.has(edge.target) && (!edgeIds || edgeIds.has(edge.id)),
  );
  const incoming = new Map<string, number>();
  const outgoing = new Map<string, ResearchEdge[]>();
  const depth = new Map<string, number>();

  for (const id of nodeIds) {
    incoming.set(id, 0);
    depth.set(id, 0);
  }
  for (const edge of edges) {
    incoming.set(edge.target, (incoming.get(edge.target) ?? 0) + 1);
    outgoing.set(edge.source, [...(outgoing.get(edge.source) ?? []), edge]);
  }

  const queue = [...nodeIds].filter((id) => (incoming.get(id) ?? 0) === 0).sort();
  let visited = 0;
  while (queue.length) {
    const current = queue.shift()!;
    visited += 1;
    for (const edge of outgoing.get(current) ?? []) {
      depth.set(edge.target, Math.max(depth.get(edge.target) ?? 0, (depth.get(current) ?? 0) + 1));
      incoming.set(edge.target, (incoming.get(edge.target) ?? 1) - 1);
      if (incoming.get(edge.target) === 0) queue.push(edge.target);
    }
    queue.sort();
  }

  // Feedback graphs are valid research objects. Place unresolved nodes in a
  // final layer instead of discarding them.
  if (visited < nodeIds.size) {
    const maxDepth = Math.max(0, ...depth.values());
    for (const [id, count] of incoming) {
      if (count > 0) depth.set(id, maxDepth + 1);
    }
  }
  return depth;
}

/** 从关系语义选择证据或反驳链边 / Selects evidence or refutation-chain edges by relation semantics. */
function chainEdgeIds(project: ProjectState, mode: "evidence" | "refutation") {
  const supportTypes = new Set<ResearchEdgeType>(["supports", "derived_from", "measures", "uses"]);
  return new Set(
    project.edges
      .filter((edge) => {
        if (mode === "refutation") {
          return edge.type === "contradicts" || edge.experiment?.outcome === "refutes";
        }
        return supportTypes.has(edge.type) || edge.experiment?.outcome === "supports";
      })
      .map((edge) => edge.id),
  );
}

/**
 * 以节点度数作为演示频率生成确定性 Huffman 编码。
 * Builds deterministic Huffman codes from node degree as a demonstration frequency.
 */
function huffmanCodes(project: ProjectState) {
  type HuffmanItem = { weight: number; ids: string[]; codes: Record<string, string> };
  const queue: HuffmanItem[] = project.nodes.map((node) => ({
    weight:
      1 +
      node.evidenceIds.length * 2 +
      project.edges.filter((edge) => edge.source === node.id || edge.target === node.id).length,
    ids: [node.id],
    codes: { [node.id]: "" },
  }));

  if (queue.length === 1) return { [queue[0].ids[0]]: "0" };
  while (queue.length > 1) {
    queue.sort((a, b) => a.weight - b.weight || a.ids.join().localeCompare(b.ids.join()));
    const left = queue.shift()!;
    const right = queue.shift()!;
    const codes: Record<string, string> = {};
    for (const [id, code] of Object.entries(left.codes)) codes[id] = `0${code}`;
    for (const [id, code] of Object.entries(right.codes)) codes[id] = `1${code}`;
    queue.push({
      weight: left.weight + right.weight,
      ids: [...left.ids, ...right.ids],
      codes,
    });
  }
  return queue[0]?.codes ?? {};
}

/**
 * 计算展示坐标和解释注解；绝不改变项目中保存的 placement。
 * Computes presentation coordinates and annotations without mutating saved placements.
 */
export function computeLayout(
  project: ProjectState,
  mode: LayoutMode,
  rootId = project.nodes[0]?.id,
): LayoutResult {
  const positions: LayoutResult["positions"] = {};
  const annotations: LayoutResult["annotations"] = {};
  let nodeIds = project.nodes.map((node) => node.id);
  let edgeIds = project.edges.map((edge) => edge.id);

  if (mode === "tree") {
    const traversal = traverseGraph(project, {
      startId: rootId,
      strategy: "bfs",
      direction: "out",
      maxDepth: Number.MAX_SAFE_INTEGER,
    });
    const included = new Set(traversal.order);
    nodeIds = [...traversal.order, ...project.nodes.map((node) => node.id).filter((id) => !included.has(id))];
    edgeIds = traversal.treeEdgeIds;
    const rows = new Map<number, string[]>();
    for (const id of nodeIds) {
      const depth = traversal.depth[id] ?? Math.max(1, ...Object.values(traversal.depth)) + 1;
      rows.set(depth, [...(rows.get(depth) ?? []), id]);
    }
    for (const [depth, ids] of rows) {
      ids.forEach((id, index) => {
        positions[id] = { x: 80 + depth * 350, y: 80 + index * 182 };
      });
    }
  } else if (mode === "table") {
    const columns = [...new Set(project.nodes.map((node) => node.type))];
    columns.forEach((type, column) => {
      project.nodes
        .filter((node) => node.type === type)
        .forEach((node, row) => {
          positions[node.id] = { x: 70 + column * 310, y: 105 + row * 168 };
          annotations[node.id] = `${type} · row ${row + 1}`;
        });
    });
  } else if (mode === "huffman") {
    const codes = huffmanCodes(project);
    const ordered = project.nodes
      .map((node) => ({ id: node.id, code: codes[node.id] ?? "" }))
      .sort((a, b) => a.code.localeCompare(b.code));
    const rowsByDepth = new Map<number, number>();
    for (const item of ordered) {
      const depth = item.code.length;
      const row = rowsByDepth.get(depth) ?? 0;
      positions[item.id] = { x: 70 + depth * 320, y: 80 + row * 172 };
      annotations[item.id] = `prefix ${item.code || "0"}`;
      rowsByDepth.set(depth, row + 1);
    }
  } else {
    const selectedEdges =
      mode === "evidence-chain"
        ? chainEdgeIds(project, "evidence")
        : mode === "refutation-chain"
          ? chainEdgeIds(project, "refutation")
          : new Set(project.edges.map((edge) => edge.id));
    edgeIds = [...selectedEdges];
    if (mode !== "neural-network") {
      const linkedNodes = new Set<string>();
      project.edges.forEach((edge) => {
        if (selectedEdges.has(edge.id)) {
          linkedNodes.add(edge.source);
          linkedNodes.add(edge.target);
        }
      });
      nodeIds = [...linkedNodes];
    }
    const depth = topologicalDepths(project, selectedEdges);
    const rows = new Map<number, string[]>();
    for (const id of nodeIds) {
      const layer = depth.get(id) ?? 0;
      rows.set(layer, [...(rows.get(layer) ?? []), id]);
    }
    for (const [layer, ids] of rows) {
      ids.forEach((id, index) => {
        positions[id] = { x: 75 + layer * 360, y: 85 + index * 190 };
        if (mode === "neural-network") annotations[id] = `layer ${layer}`;
      });
    }
  }

  return { mode, positions, annotations, nodeIds, edgeIds };
}
