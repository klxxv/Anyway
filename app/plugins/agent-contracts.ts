import type { PluginGraphPatch } from "./contracts";

/** Document-scoped diff contract shared by Agent review and Vue. */
export type {
  CanvasDiffAgentResultEnvelope,
  CanvasDiffBatchRequest,
  CanvasDiffBatchResult,
  CanvasDiffDocumentInput,
  CanvasDiffDocumentProvenance,
  CanvasDiffDocumentResult,
  CanvasDiffEntityRef,
  CanvasDiffReviewContract,
} from "../domain/canvas-diff";

export type { PluginGraphPatch };

// ── LLM Provider 类型 / LLM Provider types ──

/** 前端可用的 provider 摘要信息 / Provider summary info for frontend. */
export interface LlmProviderInfo {
  pluginId: string;
  pluginVersion: string;
  providerName: string;
  providerType: string;
  baseUrl: string;
  isActive: boolean;
  hasApiKey: boolean;
  requiresApiKey: boolean;
  apiKeyLabel?: string;
  defaultRouting: {
    extraction: { model: string; thinking: boolean; thinkingLevel?: string; jsonOutput: boolean };
    synthesis: { model: string; thinking: boolean; thinkingLevel?: string; jsonOutput: boolean };
    recovery: { model: string; thinking: boolean; jsonOutput: boolean };
  };
}

// ── AgentPlugin 类型定义 / AgentPlugin type definition ──

/** Agent 运行阶段标签 / Agent pipeline stage labels. */
export type AgentJobStage =
  | "queued"
  | "created"
  | "validating_file"
  | "extracting_text"
  | "ocr_optional"
  | "building_document_map"
  | "extracting_semantics"
  | "compiling_graph_ir"
  | "persisting_canvas"
  | "generating_patch"
  | "awaiting_review"
  | "accepted"
  | "rejected"
  | "cancelled"
  | "failed";

export type ImportDocumentFormat = "pdf" | "docx" | "md";

/** Stable host setting id for the optional model-authored progress channel. */
export const PUBLIC_PROGRESS_SETTING_ID = "public-progress" as const;
export const PUBLIC_PROGRESS_SETTING_VALUES = ["disabled", "enabled"] as const;
export type PublicProgressSettingValue = (typeof PUBLIC_PROGRESS_SETTING_VALUES)[number];

export function isPublicProgressEnabled(value: unknown): value is "enabled" | true {
  return value === "enabled" || value === true;
}

/** Ordinary assistant-content progress; never provider thinking/reasoning text. */
export interface PublicProgressEvent {
  stage: string;
  summary: string;
  evidenceCount?: number;
  warningCount?: number;
  createdAt?: number;
}

export interface RepairAuditEntry {
  code: string;
  path: string;
  beforeSummary: string;
  afterSummary: string;
  severity: "info" | "warning" | "error";
  deterministic: boolean;
}

export interface RepairAuditRecord {
  pass: string;
  attempt: number;
  status:
    | "validated"
    | "deterministically-repaired"
    | "needs-recovery"
    | "model-recovered"
    | "recovery-failed";
  entries: RepairAuditEntry[];
  error: string | null;
  createdAt: number;
}

/** Safe activity telemetry; never contains provider reasoning text or hidden CoT. */
export interface ReasoningActivity {
  chunkCount: number;
  bytes: number;
  elapsedMs: number;
  currentPass: string | null;
  retryCount: number;
  safeSummary: string | null;
}

/** Agent Job 状态快照——从 Rust AgentHost 序列化到前端 / Agent job status snapshot serialized from Rust. */
export interface AgentJobStatus {
  jobId: string;
  filePath: string;
  documentFormat: ImportDocumentFormat | null;
  batchId: string | null;
  reasoningActivity: ReasoningActivity;
  uploadBytes: number;
  uploadTotalBytes: number | null;
  /** @deprecated Compatibility alias for the original PDF-only review UI. */
  pdfPath: string;
  fileHash: string;
  state: AgentJobStage;
  progress: [number, number]; // [completed checkpoints, total checkpoints]
  createdAt: number;
  updatedAt: number;
  error: string | null;
  result: Record<string, unknown> | null;
  /** Optional host bridge field populated when public-progress is enabled. */
  publicProgress?: PublicProgressEvent[];
  /** Bounded, user-inspectable record of deterministic repairs and recovery. */
  repairAudit?: RepairAuditRecord[];
}

/** 审阅决策载荷 / Review decision payload. */
export interface ReviewPatchRequest {
  jobId: string;
  accept: boolean;
}

/** 启动 PDF Job 请求 / Start PDF job request. */
export interface StartPdfJobRequest {
  pdfPath: string;
}

/** Starts a host-owned serial batch. File contents are validated in Rust. */
export interface StartDocumentBatchRequest {
  paths: string[];
}

export type ImportBatchState =
  | "queued"
  | "running"
  | "completed"
  | "completed_with_errors"
  | "cancelled";

export interface ImportBatchStatus {
  batchId: string;
  state: ImportBatchState;
  createdAt: number;
  updatedAt: number;
  jobs: AgentJobStatus[];
}

// ── 能力声明 / Capability declarations ──

/**
 * AgentPlugin 可声明的能力全集。
 * Agent plugins may declare these capabilities; the host enforces them.
 */
export const AGENT_CAPABILITIES = [
  /** 读取 PDF 文件（由宿主代理，Agent 不直接持有文件句柄） */
  "agent.pdf.read",
  /** 提交 reviewRequired GraphPatch（Agent 唯一写入路径） */
  "agent.graph.patch.propose",
  /** 请求人工审阅 UI */
  "agent.review.request",
] as const;

export type AgentCapability = (typeof AGENT_CAPABILITIES)[number];

/**
 * AgentPlugin 权限声明——Agent 不持有任何环境权限。
 * AgentPlugin permission declarations — agents hold zero ambient permissions.
 */
export const AGENT_PERMISSIONS: readonly string[] = [
  "No API key storage",
  "No filesystem handles (host-mediated PDF read only)",
  "No network access",
  "No graph store write (GraphPatch proposal only)",
];

// ── 审阅 UI 契约 / Review UI contract ──

/**
 * 审阅 UI 必须实现的契约：
 * - 展示 Agent 提议的 GraphPatch 操作
 * - 允许逐项接受/拒绝
 * - 最终发出 review_patch 命令
 *
 * Review UI contract:
 * - Display the agent's proposed GraphPatch operations
 * - Allow per-item accept/reject
 * - Emit the review_patch command
 */
export interface ReviewUIContract {
  /** 当前待审阅的 GraphPatch */
  patch: PluginGraphPatch | null;
  /** Job 状态 */
  jobStatus: AgentJobStatus | null;
  /** 加载中 */
  loading: boolean;
  /** 错误信息 */
  error: string;
  /** 接受整个 patch */
  acceptAll: () => Promise<void>;
  /** 拒绝整个 patch */
  rejectAll: () => Promise<void>;
  /** 刷新 job 状态 */
  refresh: () => Promise<void>;
}

/**
 * 审阅 UI 组件的 props 契约。
 * Props contract for the review UI component.
 */
export interface AgentReviewPanelProps {
  jobId: string;
  onComplete?: (accepted: boolean) => void;
}

// ── Agent 插件清单 / Agent plugin manifest ──

/**
 * Agent 插件类别常量 / Agent plugin category constant.
 * 与 catalog.ts 中 PluginManifest.category 对应。
 */
export const AGENT_PLUGIN_CATEGORY = "agent" as const;

/**
 * pdf-canvas-agent 的声明式清单。
 * Declarative manifest for the pdf-canvas-agent.
 */
export const PDF_CANVAS_AGENT_MANIFEST = {
  id: "pdf-canvas-agent",
  name: "PDF Canvas Agent",
  version: "0.1.0",
  category: AGENT_PLUGIN_CATEGORY,
  description:
    "从 PDF 论文中提取 DocumentMap → 语义结构 → 审阅门控的 GraphPatch。Agent 不持有 API Key、文件句柄或网络权限；宿主管理一切，Agent 输出仅可进入 reviewRequired GraphPatch。",
  status: "installed" as const,
  permissions: [...AGENT_PERMISSIONS],
  capabilities: [...AGENT_CAPABILITIES],
  publisher: "Research Canvas",
};

// ── 类型守卫 / Type guards ──

/** 检查 job 是否处于终态 / Check if a job is in a terminal state. */
export function isJobTerminal(state: AgentJobStage): boolean {
  return state === "accepted" || state === "rejected" || state === "cancelled" || state === "failed";
}

/** 检查 job 是否等待审阅 / Check if a job is awaiting review. */
export function isJobAwaitingReview(state: AgentJobStage): boolean {
  return state === "awaiting_review";
}
