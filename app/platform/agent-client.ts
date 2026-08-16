import type {
  AgentJobStatus,
  ImportBatchStatus,
  ReviewPatchRequest,
  StartPdfJobRequest,
} from "../plugins/agent-contracts";
import type { ProjectState } from "../lib/research-types";
import { HostSdk } from "./host-sdk";
import { createDefaultTauriHostSdkTransport } from "./host-sdk-tauri";

/**
 * PDF Agent 桌面桥接：仅做 invoke 参数序列化，不持有任何 Agent 状态。
 * Desktop bridge for the PDF agent pipeline — serializes invoke arguments only.
 * 浏览器（Web）构建通过 hasTauriRuntime 检测保持可用，返回 DESKTOP_REQUIRED。
 */

/** 编译结果 / Compile result of the graph compiler (phase 4). */
export interface PdfCompileResult {
  compile: {
    project: Record<string, unknown>;
    blockHashes: Record<string, string>;
    contentRootHash: string;
    fileHash: string;
    violations: Array<{ severity: string; code: string; message: string }>;
  };
  logicChain: {
    mode: string;
    nodeIds: string[];
    edgeIds: string[];
    score: number;
    summary: string;
  };
  contradictions: {
    cycles: Array<{ nodeIds: string[]; edgeIds: string[] }>;
    minimalSize: number | null;
    consideredEdgeIds: string[];
  };
  beliefs: {
    converged: boolean;
    iterations: number;
    residual: number;
    status: string;
    meanNetBelief: number;
    variableCount: number;
  };
}

function hasTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function desktopModules() {
  if (!hasTauriRuntime()) throw new Error("DESKTOP_REQUIRED");
  const [{ invoke }, dialog] = await Promise.all([
    import("@tauri-apps/api/core"),
    import("@tauri-apps/plugin-dialog"),
  ]);
  return { invoke, dialog };
}

let desktopHostSdk: HostSdk | undefined;

function getDesktopHostSdk(): HostSdk {
  desktopHostSdk ??= new HostSdk(createDefaultTauriHostSdkTransport());
  return desktopHostSdk;
}

/** 启动 PDF 处理 Job：调用 Rust 端完整管线（校验→提取→OCR→DocumentMap→语义→补丁→审阅）。 */
export async function startPdfJob(pdfPath: string): Promise<AgentJobStatus> {
  const { invoke } = await desktopModules();
  const request: StartPdfJobRequest = { pdfPath };
  return invoke<AgentJobStatus>("start_pdf_job", { request });
}

/** Queue a document batch. The command returns before validation or parsing starts. */
export async function startDocumentBatch(paths: string[]): Promise<ImportBatchStatus> {
  const { invoke } = await desktopModules();
  return invoke<ImportBatchStatus>("start_document_batch", { request: { paths } });
}

export async function getImportBatchStatus(batchId: string): Promise<ImportBatchStatus> {
  await desktopModules();
  return getDesktopHostSdk().call<ImportBatchStatus>("agent.batch.status", { batchId });
}

export async function listImportJobs(): Promise<AgentJobStatus[]> {
  await desktopModules();
  return getDesktopHostSdk().call<AgentJobStatus[]>("agent.job.list", {});
}

/** 查询 Job 状态快照 / Query a job's status snapshot. */
export async function getPdfJobStatus(jobId: string): Promise<AgentJobStatus> {
  await desktopModules();
  return getDesktopHostSdk().call<AgentJobStatus>("agent.job.status", { jobId });
}

/** 审阅裁决：接受或拒绝 Agent 提议的 GraphPatch / Review decision. */
export async function reviewPdfPatch(jobId: string, accept: boolean): Promise<AgentJobStatus> {
  const { invoke } = await desktopModules();
  const request: ReviewPatchRequest = { jobId, accept };
  return invoke<AgentJobStatus>("review_patch", { request });
}

/** 取消进行中的 Job / Cancel an in-progress job. */
export async function cancelPdfJob(jobId: string): Promise<AgentJobStatus> {
  const { invoke } = await desktopModules();
  return invoke<AgentJobStatus>("cancel_job", { jobId });
}

/** 阶段 4：图编译器——不变式、blockHash、逻辑链、矛盾链、BP 信念。 */
export async function compileProject(project: ProjectState): Promise<PdfCompileResult> {
  const { invoke } = await desktopModules();
  return invoke<PdfCompileResult>("compile_project", { project });
}

/** 弹出文件选择框选择 PDF / Pick a PDF via the native dialog. */
export async function pickImportFiles(): Promise<string[]> {
  const { dialog } = await desktopModules();
  const path = await dialog.open({
    title: "Import documents",
    multiple: true,
    directory: false,
    filters: [{ name: "Supported documents", extensions: ["pdf", "docx", "md"] }],
  });
  if (!path) return [];
  return Array.isArray(path) ? path : [path];
}

/** @deprecated Use pickImportFiles. */
export async function pickPdfFile(): Promise<string | null> {
  return (await pickImportFiles())[0] ?? null;
}

/** 仅桌面端监听拖放，只转发 `.pdf` 候选路径 / Listen for desktop drops, forward `.pdf` candidates. */
export async function listenForDocumentDrops(
  onDrop: (paths: string[]) => void,
): Promise<() => void> {
  if (!hasTauriRuntime()) return () => undefined;
  const { getCurrentWebview } = await import("@tauri-apps/api/webview");
  return getCurrentWebview().onDragDropEvent((event) => {
    if (event.payload.type === "drop") {
      onDrop(event.payload.paths.filter((path) => /\.(pdf|docx|md)$/i.test(path)));
    }
  });
}


/** @deprecated Use listenForDocumentDrops. */
export const listenForPdfDrops = listenForDocumentDrops;

/** 提取待审阅 GraphPatch 中的操作数组 / Extract the reviewable operations array. */
export function patchOperationsOf(status: AgentJobStatus): unknown[] {
  const result = status.result;
  if (!result || typeof result !== "object") return [];
  const operations = (result as Record<string, unknown>).operations;
  return Array.isArray(operations) ? operations : [];
}
