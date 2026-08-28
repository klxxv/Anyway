import { jsPDF } from "jspdf";
import type { ProjectState, ResearchEdgeType } from "../lib/research-types";
import type {
  InstalledMycPlugin,
  InstalledPluginLocale,
  PluginCommandContribution,
  PluginGraphPatch,
  PluginReference,
} from "./contracts";
import { pluginReference } from "./contracts";

export type EnabledWorkspaceCommand = PluginCommandContribution & {
  plugin: PluginReference;
};

/** 仅返回已启用且同包声明了对应能力的命令 / Returns enabled commands backed by a same-package capability. */
export function workspaceCommandsFromPlugins(
  plugins: InstalledMycPlugin[],
): EnabledWorkspaceCommand[] {
  return plugins.flatMap((plugin) => {
    const capabilities = new Set(plugin.manifest.spec.capabilities);
    return (plugin.manifest.spec.contributes?.commands ?? [])
      .filter((command) => capabilities.has(command.capability))
      .map((command) => ({
        ...command,
        plugin: pluginReference(plugin),
      }));
  });
}

/** 社区语言包仅含数据而非代码，且只有启用的包会生效 / Community locales are data-only and opt-in. */
export function localeBundlesFromPlugins(
  plugins: InstalledMycPlugin[],
): InstalledPluginLocale[] {
  return plugins.flatMap((plugin) => plugin.locales ?? []);
}

function escapeXml(value: string) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

function safeFileStem(value: string) {
  const stem = value.trim().toLowerCase().replaceAll(/[^a-z0-9\u4e00-\u9fff]+/g, "-");
  return stem.replaceAll(/^-+|-+$/g, "") || "research-canvas";
}

export function projectExportFileName(project: ProjectState, format: "pdf" | "svg" | "png") {
  return `${safeFileStem(project.title)}.${format}`;
}

/** SVG、PNG 与 PDF 插件动作共用的确定性矢量渲染器 / Deterministic renderer shared by export actions. */
export function projectToSvg(project: ProjectState): string {
  const placements = new Map(project.placements.map((placement) => [placement.nodeId, placement]));
  const visible = project.nodes.filter((node) => placements.has(node.id));
  const maxX = Math.max(
    1200,
    ...visible.map((node) => {
      const placement = placements.get(node.id)!;
      return placement.x + placement.width + 80;
    }),
  );
  const maxY = Math.max(
    760,
    ...visible.map((node) => {
      const placement = placements.get(node.id)!;
      return placement.y + placement.height + 80;
    }),
  );
  const edges = project.edges
    .map((edge) => {
      const source = placements.get(edge.source);
      const target = placements.get(edge.target);
      if (!source || !target) return "";
      const x1 = source.x + source.width / 2;
      const y1 = source.y + source.height / 2;
      const x2 = target.x + target.width / 2;
      const y2 = target.y + target.height / 2;
      const dash = edge.type === "I" ? ' stroke-dasharray="8 6"' : edge.type === "M" ? ' stroke-dasharray="3 5"' : "";
      const edgeColors: Record<ResearchEdgeType, string> = { T: "#5271c7", K: "#25836f", I: "#c98a2b", M: "#7d8796", Q: "#6d5bc1" };
      const color = edgeColors[edge.type];
      const label = escapeXml(edge.note?.trim() || edge.type);
      return `<g><line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="${color}" stroke-width="1.4"${dash}/><text x="${(x1 + x2) / 2}" y="${(y1 + y2) / 2 - 7}" text-anchor="middle" fill="${color}" font-size="12" font-family="serif">${label}</text></g>`;
    })
    .join("");
  const nodes = visible
    .map((node) => {
      const placement = placements.get(node.id)!;
      const selectedShape = node.type === "question" ? "ellipse" : "rect";
      const shape =
        selectedShape === "ellipse"
          ? `<ellipse cx="${placement.x + placement.width / 2}" cy="${placement.y + placement.height / 2}" rx="${placement.width / 2}" ry="${placement.height / 2}" fill="#fff" stroke="#3f4348"/>`
          : `<rect x="${placement.x}" y="${placement.y}" width="${placement.width}" height="${placement.height}" rx="4" fill="#fff" stroke="#3f4348"/>`;
      const title = escapeXml(node.title);
      const type = escapeXml(node.type);
      const centerX = placement.x + placement.width / 2;
      const centerY = placement.y + placement.height / 2;
      return `<g>${shape}<text x="${centerX}" y="${centerY - 4}" text-anchor="middle" fill="#25272a" font-size="15" font-family="serif">${title}</text><text x="${centerX}" y="${centerY + 18}" text-anchor="middle" fill="#6a6e73" font-size="10" font-family="sans-serif">${type}</text></g>`;
    })
    .join("");
  return `<?xml version="1.0" encoding="UTF-8"?><svg xmlns="http://www.w3.org/2000/svg" width="${maxX}" height="${maxY}" viewBox="0 0 ${maxX} ${maxY}"><rect width="100%" height="100%" fill="#ffffff"/><g>${edges}${nodes}</g></svg>`;
}

async function svgCanvas(svg: string) {
  const source = new Blob([svg], { type: "image/svg+xml;charset=utf-8" });
  const url = URL.createObjectURL(source);
  try {
    const image = new Image();
    image.decoding = "async";
    image.src = url;
    await image.decode();
    const maxDimension = 4096;
    const scale = Math.min(2, maxDimension / Math.max(image.naturalWidth, image.naturalHeight));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(image.naturalWidth * scale));
    canvas.height = Math.max(1, Math.round(image.naturalHeight * scale));
    const context = canvas.getContext("2d");
    if (!context) throw new Error("Canvas 2D rendering is unavailable");
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    return canvas;
  } finally {
    URL.revokeObjectURL(url);
  }
}

function canvasPng(canvas: HTMLCanvasElement): Promise<Uint8Array> {
  return new Promise((resolve, reject) => {
    canvas.toBlob(async (blob) => {
      if (!blob) {
        reject(new Error("PNG encoding failed"));
        return;
      }
      resolve(new Uint8Array(await blob.arrayBuffer()));
    }, "image/png");
  });
}

export async function renderProjectExport(
  project: ProjectState,
  format: "pdf" | "svg" | "png",
): Promise<Uint8Array> {
  const svg = projectToSvg(project);
  if (format === "svg") return new TextEncoder().encode(svg);
  const canvas = await svgCanvas(svg);
  if (format === "png") return canvasPng(canvas);
  const landscape = canvas.width >= canvas.height;
  const document = new jsPDF({
    orientation: landscape ? "landscape" : "portrait",
    unit: "px",
    format: [canvas.width, canvas.height],
    compress: true,
  });
  document.addImage(canvas.toDataURL("image/png"), "PNG", 0, 0, canvas.width, canvas.height);
  return new Uint8Array(document.output("arraybuffer"));
}

/** 校验可移植审阅协议但不修改项目状态 / Validates the portable review contract without applying it. */
export function normalizePluginGraphPatch(value: unknown): PluginGraphPatch | null {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<PluginGraphPatch>;
  if (
    candidate.apiVersion !== "researchcanvas.dev/graph-patch/v1alpha1" ||
    candidate.reviewRequired !== true ||
    !candidate.source ||
    typeof candidate.source.pluginId !== "string" ||
    candidate.source.pluginId.length === 0 ||
    candidate.source.pluginId.length > 160 ||
    typeof candidate.source.operation !== "string" ||
    candidate.source.operation.length === 0 ||
    candidate.source.operation.length > 160 ||
    (candidate.source.projectId !== undefined &&
      (typeof candidate.source.projectId !== "string" ||
        candidate.source.projectId.length === 0 ||
        candidate.source.projectId.length > 160)) ||
    typeof candidate.title !== "string" ||
    candidate.title.length === 0 ||
    candidate.title.length > 500 ||
    typeof candidate.summary !== "string" ||
    candidate.summary.length > 2_000 ||
    !Array.isArray(candidate.operations) ||
    candidate.operations.length > 2_000
  ) {
    return null;
  }
  const allowed = new Set(["add-node", "add-edge", "update-node", "update-edge"]);
  const isText = (item: unknown, limit = 500): item is string =>
    typeof item === "string" && item.length > 0 && item.length <= limit;
  const isRecord = (item: unknown): item is Record<string, unknown> =>
    Boolean(item) && typeof item === "object" && !Array.isArray(item);
  const validOperation = (operation: unknown) => {
    if (!isRecord(operation) || !isText(operation.op, 32) || !allowed.has(operation.op)) {
      return false;
    }
    if (operation.op === "add-node") {
      const node = operation.node;
      return (
        isRecord(node) &&
        isText(node.id, 160) &&
        isText(node.type, 40) &&
        isText(node.title) &&
        (node.body === undefined || isText(node.body, 10_000)) &&
        (node.tags === undefined ||
          (Array.isArray(node.tags) && node.tags.every((tag) => isText(tag, 80)))) &&
        (node.data === undefined || isRecord(node.data))
      );
    }
    if (operation.op === "add-edge") {
      const edge = operation.edge;
      return (
        isRecord(edge) &&
        isText(edge.id, 160) &&
        isText(edge.source, 160) &&
        isText(edge.target, 160) &&
        isText(edge.type, 40) &&
        (edge.note === undefined || isText(edge.note, 2_000)) &&
        (edge.data === undefined || isRecord(edge.data))
      );
    }
    const targetKey = operation.op === "update-node" ? "nodeId" : "edgeId";
    return isText(operation[targetKey], 160) && isRecord(operation.changes);
  };
  if (candidate.operations.some((operation) => !validOperation(operation))) {
    return null;
  }
  return candidate as PluginGraphPatch;
}
