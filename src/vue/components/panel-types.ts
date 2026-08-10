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
import type {
  PluginSecretMutation,
  PluginSettingsSnapshot,
  PluginSettingsWrite,
} from "../../../app/plugins/tauri-client";
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

export type HostPluginSettingType = "boolean" | "number" | "text" | "select" | "secret";

export type HostPluginSettingOption = {
  value: string;
  label: string;
};

/** UI-side compatibility shape; `secret` is accepted without changing the shared install contract. */
export type HostPluginSettingDefinition = {
  id: string;
  label: string;
  description?: string;
  type: HostPluginSettingType;
  default?: boolean | number | string;
  min?: number;
  max?: number;
  step?: number;
  options?: HostPluginSettingOption[];
  placeholder?: string;
  required?: boolean;
  group?: string;
};

export type PluginSecretDraft = {
  action: "keep" | "set" | "clear";
  value: string;
};

export type PluginSettingsDraft = Record<string, boolean | number | string | PluginSecretDraft>;

export type PluginSettingsTarget = {
  source: "builtin" | "installed";
  reference: PluginReference;
  name: string;
  version: string;
  kind: string;
  description: string;
  publisher?: string;
  developer?: string;
  developerUuid?: string;
  signaturePresent?: boolean;
  update?: { latestVersion?: string; url?: string; releaseNotes?: string };
  definitions: HostPluginSettingDefinition[];
  /** Built-in catalog entries use the browser-safe store until native installation exists. */
  native: boolean;
  uninstallable: boolean;
};

export function normalizePluginSettingDefinitions(input: unknown): HostPluginSettingDefinition[] {
  if (!Array.isArray(input)) return [];
  return input.flatMap((candidate) => {
    if (!candidate || typeof candidate !== "object") return [];
    const source = candidate as Record<string, unknown>;
    const id = typeof source.id === "string" ? source.id.trim() : "";
    if (!id) return [];
    const declaredType = source.type;
    const type = source.secret === true || source.writeOnly === true ? "secret" : declaredType;
    if (type !== "boolean" && type !== "number" && type !== "text" && type !== "select" && type !== "secret") {
      return [];
    }
    const options = Array.isArray(source.options)
      ? source.options.flatMap((option) => {
          if (!option || typeof option !== "object") return [];
          const item = option as Record<string, unknown>;
          return typeof item.value === "string" && typeof item.label === "string"
            ? [{ value: item.value, label: item.label }]
            : [];
        })
      : undefined;
    const definition: HostPluginSettingDefinition = {
      id,
      label: typeof source.label === "string" && source.label.trim() ? source.label : id,
      description: typeof source.description === "string" ? source.description : undefined,
      type,
      placeholder: typeof source.placeholder === "string" ? source.placeholder : undefined,
      required: source.required === true,
      group: typeof source.group === "string" ? source.group : undefined,
      default: type !== "secret" && (
        typeof source.default === "boolean" || typeof source.default === "number" || typeof source.default === "string"
      )
        ? source.default
        : undefined,
      min: typeof source.min === "number" && Number.isFinite(source.min) ? source.min : undefined,
      max: typeof source.max === "number" && Number.isFinite(source.max) ? source.max : undefined,
      step: typeof source.step === "number" && Number.isFinite(source.step) && source.step > 0 ? source.step : undefined,
      options,
    };
    if (definition.type === "select" && !definition.options?.length) return [];
    return [definition];
  });
}

function defaultForSetting(definition: HostPluginSettingDefinition): boolean | number | string | undefined {
  if (definition.type === "secret") return undefined;
  if (definition.type === "boolean") return typeof definition.default === "boolean" ? definition.default : false;
  if (definition.type === "number") {
    const value = typeof definition.default === "number" && Number.isFinite(definition.default)
      ? definition.default
      : definition.min ?? 0;
    return Math.min(definition.max ?? value, Math.max(definition.min ?? value, value));
  }
  if (definition.type === "select") {
    const selected = typeof definition.default === "string" && definition.options?.some((option) => option.value === definition.default)
      ? definition.default
      : definition.options?.[0]?.value;
    return selected ?? "";
  }
  return typeof definition.default === "string" ? definition.default : "";
}

export function defaultPluginSettingsDraft(
  definitions: readonly HostPluginSettingDefinition[],
  configuredSecrets: Readonly<Record<string, boolean>> = {},
): PluginSettingsDraft {
  const draft: PluginSettingsDraft = {};
  for (const definition of definitions) {
    if (definition.type === "secret") {
      draft[definition.id] = { action: configuredSecrets[definition.id] ? "keep" : "clear", value: "" };
      continue;
    }
    const value = defaultForSetting(definition);
    if (value !== undefined) draft[definition.id] = value;
  }
  return draft;
}

export function draftFromPluginSettings(
  definitions: readonly HostPluginSettingDefinition[],
  snapshot: PluginSettingsSnapshot,
): PluginSettingsDraft {
  const defaults = defaultPluginSettingsDraft(definitions, snapshot.configuredSecrets);
  for (const definition of definitions) {
    if (definition.type === "secret") continue;
    const value = snapshot.values[definition.id];
    if (typeof value === "boolean" || typeof value === "number" || typeof value === "string") {
      defaults[definition.id] = value;
    }
  }
  return defaults;
}

export function clonePluginSettingsDraft(draft: PluginSettingsDraft): PluginSettingsDraft {
  return Object.fromEntries(
    Object.entries(draft).map(([id, value]) => [id, typeof value === "object" ? { ...value } : value]),
  );
}

export function validatePluginSettingsDraft(
  definitions: readonly HostPluginSettingDefinition[],
  draft: PluginSettingsDraft,
  configuredSecrets: Readonly<Record<string, boolean>> = {},
): Record<string, string> {
  const errors: Record<string, string> = {};
  for (const definition of definitions) {
    const value = draft[definition.id];
    if (definition.type === "secret") {
      const secret: PluginSecretDraft = value && typeof value === "object"
        ? value as PluginSecretDraft
        : { action: "keep", value: "" };
      if (!["keep", "set", "clear"].includes(secret.action)) errors[definition.id] = "Invalid secret action.";
      if (secret.action === "set" && !secret.value.trim()) errors[definition.id] = "Enter a value or choose Clear.";
      if (
        definition.required &&
        (secret.action === "clear" || (secret.action === "keep" && !configuredSecrets[definition.id]))
      ) {
        errors[definition.id] = "A value is required.";
      }
      continue;
    }
    if (definition.type === "boolean" && typeof value !== "boolean") errors[definition.id] = "Choose true or false.";
    if (definition.type === "number") {
      if (typeof value !== "number" || !Number.isFinite(value)) errors[definition.id] = "Enter a valid number.";
      else if (definition.min !== undefined && value < definition.min) errors[definition.id] = `Value must be at least ${definition.min}.`;
      else if (definition.max !== undefined && value > definition.max) errors[definition.id] = `Value must be at most ${definition.max}.`;
    }
    if (definition.type === "text" && typeof value !== "string") errors[definition.id] = "Enter text.";
    if (definition.type === "select" && (typeof value !== "string" || !definition.options?.some((option) => option.value === value))) {
      errors[definition.id] = "Choose one of the listed options.";
    }
  }
  return errors;
}

export function settingsWriteFromDraft(
  definitions: readonly HostPluginSettingDefinition[],
  draft: PluginSettingsDraft,
): { values: Record<string, unknown>; secrets: Record<string, PluginSecretMutation> } {
  const values: Record<string, unknown> = {};
  const secrets: Record<string, PluginSecretMutation> = {};
  for (const definition of definitions) {
    const value = draft[definition.id];
    if (definition.type === "secret") {
      const secret: PluginSecretDraft = value && typeof value === "object"
        ? value as PluginSecretDraft
        : { action: "keep", value: "" };
      secrets[definition.id] = secret.action === "set"
        ? { action: "set", value: secret.value }
        : { action: secret.action };
    } else if (value !== undefined) {
      values[definition.id] = value;
    }
  }
  return { values, secrets };
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

export type PluginSettingsDialogProps = {
  target: PluginSettingsTarget;
  draft: PluginSettingsDraft;
  configuredSecrets: Readonly<Record<string, boolean>>;
  loading: boolean;
  saving: boolean;
  error: string;
  onClose: () => void;
  onSave: (draft: PluginSettingsDraft) => Promise<void>;
  onReset: () => Promise<void>;
  onUninstall?: () => Promise<void>;
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
  loadPluginSettings: (
    plugin: PluginReference,
    definitions: readonly HostPluginSettingDefinition[],
    native?: boolean,
  ) => Promise<PluginSettingsSnapshot>;
  savePluginSettings: (
    plugin: PluginReference,
    definitions: readonly HostPluginSettingDefinition[],
    write: PluginSettingsWrite,
    native?: boolean,
  ) => Promise<PluginSettingsSnapshot>;
  resetPluginSettings: (
    plugin: PluginReference,
    definitions: readonly HostPluginSettingDefinition[],
    native?: boolean,
  ) => Promise<PluginSettingsSnapshot>;
  uninstall: (plugin: InstalledMycPlugin) => Promise<void>;
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
