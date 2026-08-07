import type { PluginGraphPatch } from "./contracts";

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
  | "created"
  | "validating_file"
  | "extracting_text"
  | "ocr_optional"
  | "building_document_map"
  | "extracting_semantics"
  | "generating_patch"
  | "awaiting_review"
  | "accepted"
  | "rejected"
  | "failed";

/** Agent Job 状态快照——从 Rust AgentHost 序列化到前端 / Agent job status snapshot serialized from Rust. */
export interface AgentJobStatus {
  jobId: string;
  pdfPath: string;
  fileHash: string;
  state: AgentJobStage;
  progress: [number, number]; // [completed checkpoints, total checkpoints]
  createdAt: number;
  updatedAt: number;
  error: string | null;
  result: Record<string, unknown> | null;
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
  return state === "accepted" || state === "rejected" || state === "failed";
}

/** 检查 job 是否等待审阅 / Check if a job is awaiting review. */
export function isJobAwaitingReview(state: AgentJobStage): boolean {
  return state === "awaiting_review";
}

/** AgentJobStage 的人类可读标签 / Human-readable labels for agent job stages. */
export const AGENT_STAGE_LABELS: Record<AgentJobStage, string> = {
  created: "已创建 / Created",
  validating_file: "校验文件 / Validating file",
  extracting_text: "提取文本 / Extracting text",
  ocr_optional: "OCR 检测 / OCR check",
  building_document_map: "构建文档映射 / Building document map",
  extracting_semantics: "提取语义 / Extracting semantics",
  generating_patch: "生成补丁 / Generating patch",
  awaiting_review: "等待审阅 / Awaiting review",
  accepted: "已接受 / Accepted",
  rejected: "已拒绝 / Rejected",
  failed: "失败 / Failed",
};
