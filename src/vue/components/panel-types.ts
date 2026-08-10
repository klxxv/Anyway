import { onMounted, onUnmounted, ref, type VNodeChild } from "vue";
import {
  normalizeLocale,
  translate,
  type Locale,
  type MessageKey,
} from "../../../app/i18n/catalog";
import type { CanvasDiffResult, DiffState } from "../../../app/lib/graph/canvas-diff";
import type {
  ProjectState,
  ResearchEdge,
  ResearchNode,
  ResearchNodeType,
} from "../../../app/lib/research-types";
import type { PdfCompileResult } from "../../../app/platform/agent-client";
import type {
  FolderProjectSummary,
  GitHubAccountStatus,
  GitWorkspaceSnapshot,
} from "../../../app/platform/native-project";
import type { AgentJobStage, AgentJobStatus } from "../../../app/plugins/agent-contracts";
import type {
  GraphPatchOperation,
  InstalledMycPlugin,
  PluginGraphPatch,
  PluginReference,
} from "../../../app/plugins/contracts";
import type { ContextMenuActionId, ContextMenuScope } from "../../../app/features/research-workspace/workspace-context-menu";
import type { WorkspacePreferences } from "../../../app/features/research-workspace/workspace-preferences";
import type { RadialMenuAction, RadialMenuPosition } from "../../../app/features/research-workspace/workspace-radial-menu";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  NodeDraft,
  VariableValueType,
} from "../../../app/features/research-workspace/workspace-types";
import type { ShortcutAction, WorkspaceShortcuts } from "../../../app/features/research-workspace/workspace-shortcuts";

/** Shared Vue-side bridge for the existing i18n catalog. It keeps message keys
 * and interpolation identical while avoiding a dependency on the React provider. */
export function usePanelI18n() {
  const locale = ref<Locale>("en");
  const syncLocale = () => {
    if (typeof window === "undefined") return;
    locale.value = normalizeLocale(
      window.localStorage.getItem("research-canvas.locale.v1") ?? window.navigator.language,
    );
    document.documentElement.lang = locale.value;
  };
  const t = (key: MessageKey | string, parameters?: Readonly<Record<string, string | number>>) => {
    const aliases: Record<string, MessageKey> = { "composer.create": "composer.addNode" };
    return translate(locale.value, aliases[key] ?? key as MessageKey, {}, parameters);
  };

  onMounted(() => {
    syncLocale();
    window.addEventListener("storage", syncLocale);
    window.addEventListener("research-canvas.locale-changed", syncLocale);
  });
  onUnmounted(() => {
    if (typeof window === "undefined") return;
    window.removeEventListener("storage", syncLocale);
    window.removeEventListener("research-canvas.locale-changed", syncLocale);
  });

  return { locale, t, syncLocale };
}

export type ComposerState = { type: ResearchNodeType; x: number; y: number };
export type ComposerProps = {
  state: ComposerState;
  onClose: () => void;
  onCreate: (draft: NodeDraft, x: number, y: number) => void;
};

export type SearchPaletteProps = {
  project: ProjectState;
  onClose: () => void;
  onSelect: (nodeId: string) => void;
};

export type ProjectMenuProps = {
  project: ProjectState;
  onClose: () => void;
  onReset: () => void;
  onSettings: () => void;
  onPlugins: () => void;
  onSaveProject: () => void;
  onImportProject: () => void;
  onFolderWorkspace?: () => void;
  onGitWorkspace?: () => void;
};

export type SettingsDialogProps = {
  preferences: WorkspacePreferences;
  onClose: () => void;
  onSave: (preferences: WorkspacePreferences) => void;
};

export type InspectorPanelProps = {
  node: ResearchNode | null;
  edge: ResearchEdge | null;
  nodes: ResearchNode[];
  onUpdate: (nodeId: string, update: InspectorUpdate) => void;
  onUpdateEdge: (edgeId: string, update: EdgeInspectorUpdate) => void;
  onDelete: (nodeId: string) => void;
  onDeleteEdge: (edgeId: string) => void;
  onReverseEdge: (edgeId: string) => void;
  onClose: () => void;
};

export type DiffMode = "side-by-side" | "overlay";
export type DiffVersion = { id: string; label: string; project: ProjectState };
export type DiffPanelProps = {
  versions: DiffVersion[];
  baseId: string;
  compareId: string;
  mode: DiffMode;
  result: CanvasDiffResult | null;
  loading: boolean;
  error: string | null;
  onBaseChange: (id: string) => void;
  onCompareChange: (id: string) => void;
  onModeChange: (mode: DiffMode) => void;
  onClose: () => void;
  onFocus: (kind: "node" | "edge", entityId: string) => void;
};

export type ReviewDecision = { accept: boolean; edits?: Record<string, string> };
export type AgentReviewPanelProps = {
  jobId: string;
  compileResult?: PdfCompileResult | null;
  compileError?: string;
  onClose: () => void;
  onApply: (patch: PluginGraphPatch) => void;
  onReject: () => void;
  onRollback: () => void;
};

export type PdfUploadDialogProps = {
  onClose: () => void;
  onReady: (jobId: string, status: AgentJobStatus) => void;
};

export type FolderWorkspaceDialogProps = {
  root: string;
  projects: FolderProjectSummary[];
  onClose: () => void;
  onOpen: (path: string) => void;
};

export type GitWorkspaceDialogProps = {
  snapshot: GitWorkspaceSnapshot;
  account: GitHubAccountStatus | null;
  autoSave: boolean;
  busy: boolean;
  patch: PluginGraphPatch | null;
  onClose: () => void;
  onToggleAutoSave: (enabled: boolean) => void;
  onInitialize: () => void;
  onRefreshAccount: () => void;
  onLogin: () => void;
  onGenerateSshKey: () => void;
  onUploadSshKey: (path: string) => void;
  onSaveNow: () => void;
  onApplyPatch: (acceptedPatch: PluginGraphPatch | null) => void;
};

export type PluginStoreItemProps = {
  name: string;
  version: string;
  kind?: string;
  description: string;
  icon?: VNodeChild;
  status?: VNodeChild;
  actions?: VNodeChild;
  onOpenSettings?: () => void;
};

export type PluginStoreDialogProps = { onClose: () => void };

export type PluginHostSnapshot = {
  installedPlugins: InstalledMycPlugin[];
  activePluginKeys: ReadonlySet<string>;
  loading: boolean;
  error: string;
  refresh: () => Promise<void>;
  install: (path: string) => Promise<InstalledMycPlugin>;
  setPluginEnabled: (plugin: InstalledMycPlugin, enabled: boolean) => void;
  enableAll: () => void;
  removeIncompatible: () => Promise<number>;
};

export type PluginRunRequest = {
  plugin: PluginReference;
  capability?: string;
  input: Record<string, unknown>;
};

export const nodeTypeMessageKeys: Partial<Record<ResearchNodeType, MessageKey>> = {
  question: "node.question",
  concept: "node.group",
  variable: "node.variable",
  method: "node.method",
  dataset: "node.data",
  evidence: "node.evidence",
  result: "node.result",
  note: "node.note",
};

export function valueTypeOf(node: ResearchNode): VariableValueType {
  const valueType = node.data.valueType;
  return valueType === "enum" || valueType === "bool" || valueType === "number" || valueType === "text"
    ? valueType
    : "text";
}

export const diffMeta: Record<DiffState, { labelKey: MessageKey; textClass: string; bgClass: string }> = {
  added: { labelKey: "diff.added", textClass: "text-diff-added", bgClass: "bg-diff-added-soft" },
  removed: { labelKey: "diff.removed", textClass: "text-diff-removed", bgClass: "bg-diff-removed-soft" },
  modified: { labelKey: "diff.modified", textClass: "text-diff-modified", bgClass: "bg-diff-modified-soft" },
};

export function versionById(versions: DiffVersion[], id: string): DiffVersion | null {
  return versions.find((version) => version.id === id) ?? null;
}

export function nodeTitleMap(project: ProjectState): Map<string, string> {
  return new Map(project.nodes.map((node) => [node.id, node.title]));
}

/** Per-operation review decision summary. Kept byte-for-byte equivalent in behavior. */
export function operationSubject(operation: GraphPatchOperation): string {
  switch (operation.op) {
    case "add-node":
      return operation.node.title;
    case "add-edge":
      return `${operation.edge.source} → ${operation.edge.target}`;
    case "update-node":
      return operation.nodeId;
    case "update-edge":
      return operation.edgeId;
  }
}

/** Filters a patch by explicit decisions and applies local title/note edits. */
export function buildAcceptedPatch(
  patch: PluginGraphPatch,
  decisions: Record<number, ReviewDecision>,
): PluginGraphPatch | null {
  const operations = patch.operations
    .map((operation, index) => {
      const decision = decisions[index];
      if (!decision || !decision.accept) return null;
      if (!decision.edits) return operation;
      if (operation.op === "add-node" && decision.edits.title !== undefined) {
        return { ...operation, node: { ...operation.node, title: decision.edits.title } };
      }
      if (operation.op === "add-edge" && decision.edits.note !== undefined) {
        return { ...operation, edge: { ...operation.edge, note: decision.edits.note } };
      }
      return operation;
    })
    .filter((operation): operation is GraphPatchOperation => operation !== null);
  if (operations.length === 0) return null;
  return { ...patch, operations };
}

export function countAccepted(decisions: Record<number, ReviewDecision>, total: number): number {
  let count = 0;
  for (let index = 0; index < total; index += 1) {
    if (decisions[index]?.accept) count += 1;
  }
  return count;
}

export const PDF_PIPELINE_STAGES: readonly AgentJobStage[] = [
  "created",
  "validating_file",
  "extracting_text",
  "ocr_optional",
  "building_document_map",
  "extracting_semantics",
  "generating_patch",
  "awaiting_review",
];

export function jobStageIndex(state: AgentJobStage): number {
  return PDF_PIPELINE_STAGES.indexOf(state);
}

export function deriveUploadProgress(status: AgentJobStatus): {
  done: number;
  total: number;
  percent: number;
  stage: AgentJobStage;
} {
  const [done, total] = status.progress;
  const percent = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  return { done, total, percent, stage: status.state };
}

export type { AgentJobStage, AgentJobStatus, CanvasDiffResult, DiffState, GraphPatchOperation, InstalledMycPlugin, MessageKey, NodeDraft, PluginGraphPatch, ProjectState, ResearchEdge, ResearchNode, ResearchNodeType, VariableValueType, WorkspacePreferences, WorkspaceShortcuts, ShortcutAction, ContextMenuActionId, ContextMenuScope, RadialMenuAction, RadialMenuPosition, EdgeInspectorUpdate, InspectorUpdate };
