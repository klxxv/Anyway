<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { storeToRefs } from "pinia";
import { IconChevronRight, IconPlugConnected } from "@tabler/icons-vue";

import {
  compileProject,
} from "../../app/platform/agent-client";
import {
  exportProjectWithPlugin,
  generateGitHubSshKey,
  gitAutosaveProject,
  initializeGitWorkspace,
  importProjectAtPath,
  importProjectNative,
  loginGitHubAccount,
  openFolderWorkspace,
  openGitWorkspace,
  readGitHubAccount,
  saveProjectNative,
  uploadGitHubSshKey,
} from "../../app/platform/native-project";
import type { PluginGraphPatch } from "../../app/plugins/contracts";
import type { ResolvedPluginContextMenuAction as AppPluginContextMenuAction } from "../../app/plugins/context-menu";
import { runAnalysisPlugin } from "../../app/plugins/tauri-client";
import { contextMenuContributionsFromPlugins } from "../../app/plugins/context-menu";
import { normalizePluginGraphPatch, workspaceCommandsFromPlugins } from "../../app/plugins/workspace";
import { resolveEdgeStyle } from "../../app/plugins/edge-style";
import { resolveTheme, themeCssVariables } from "../../app/plugins/theme";
import type { EnabledWorkspaceCommand } from "../../app/plugins/workspace";
import type {
  LayoutMode,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../app/lib/research-types";
import { edgeTypeMessageKeys } from "../../app/features/research-workspace/workspace-edge-labels";
import {
  isEditableShortcutTarget,
  SHORTCUT_ACTIONS,
  shortcutFromKeyboardEvent,
} from "../../app/features/research-workspace/workspace-shortcuts";
import {
  normalizeWorkspacePreferences,
} from "../../app/features/research-workspace/workspace-preferences";
import type { LinkLegendFilter } from "../../app/features/research-workspace/workspace-layout";

import { useI18n } from "./runtime/i18n";
import { usePluginHost } from "./runtime/plugin-host";
import { useCanvasDiff } from "./composables/use-canvas-diff";
import { useNativeTrackpadFrames } from "./composables/use-trackpad-pinch";
import { useWorkspaceProject } from "./composables/use-workspace-project";
import { useWorkspaceUiStore } from "./stores/workspace-ui";
import ResearchGraphCanvas from "./canvas/ResearchGraphCanvas.vue";
import AgentReviewPanel from "./components/AgentReviewPanel.vue";
import DiffPanel from "./components/DiffPanel.vue";
import InspectorPanel from "./components/InspectorPanel.vue";
import PdfUploadDialog from "./components/PdfUploadDialog.vue";
import PluginStoreDialog from "./components/PluginStoreDialog.vue";
import WorkspaceDialogs from "./components/WorkspaceDialogs.vue";
import WorkspacePluginDialogs from "./components/WorkspacePluginDialogs.vue";
import WorkspaceTopbar from "./components/WorkspaceTopbar.vue";
import type {
  DiffVersion,
  FolderWorkspaceState,
  GitWorkspaceState,
  NodeDraft,
  WorkspaceContextMenuState,
  WorkspacePreferences,
  WorkspaceProjectComposable,
} from "./components/workspace-shell-types";
import type {
  ResolvedPluginContextMenuAction as CanvasPluginContextMenuAction,
} from "./canvas/canvas-types";

const preferencesStorageKey = "research-canvas.workspace-preferences.v2";
const toastVisibleMs = 3_200;

const layoutLabelKeys: Record<LayoutMode, string> = {
  "evidence-chain": "layout.evidenceChain",
  "refutation-chain": "layout.refutationChain",
  tree: "layout.tree",
  huffman: "layout.huffman",
  table: "layout.table",
  "neural-network": "layout.neural",
};

const linkFilterLabelKeys: Record<LinkLegendFilter, string> = {
  causal: "relation.causal",
  control: "relation.control",
  derived: "relation.derived",
  contradicts: "relation.contradicts",
};

const { t } = useI18n();
const { activePlugins } = usePluginHost();
const workspace = useWorkspaceProject() as WorkspaceProjectComposable;
const workspaceUi = useWorkspaceUiStore();
const {
  menuOpen,
  settingsOpen,
  pluginStoreOpen,
  searchOpen,
  inspectorOpen,
  connectMode,
  connectType,
  addRequest,
  composer,
  layoutMode,
  linkFilter,
  notice,
  preferences,
  pdfDialogOpen,
  reviewJobId,
  highlightChain,
  pdfCompileResult,
  pdfCompileError,
  diffOpen,
  diffMode,
  diffBaseId,
  diffCompareId,
  diffFocus,
} = storeToRefs(workspaceUi);
const {
  clearNotice,
  closeComposer,
  closeDiff,
  closeTransientUi: closeUiTransient,
  focusDiff,
  openComposer,
  openDiff,
  requestAdd,
  resetForProject,
  setConnectMode,
  setConnectType,
  setPreferences,
  showNotice,
  toggleMenu,
} = workspaceUi;
const { trackpadFrame } = useNativeTrackpadFrames();

const project = computed(() => workspace.project.value);
const history = computed(() => workspace.history.value);
const selectedNode = computed(() => workspace.selectedNode.value);
const selectedEdge = computed(() => workspace.selectedEdge.value);
const selectedNodeId = computed(() => workspace.selectedNodeId.value);
const selectedEdgeId = computed(() => workspace.selectedEdgeId.value);
const canUndo = computed(() => workspace.canUndo.value);
const canRedo = computed(() => workspace.canRedo.value);

const folderWorkspace = ref<FolderWorkspaceState | null>(null);
const gitSnapshot = ref<GitWorkspaceState["snapshot"] | null>(null);
const gitHubAccount = ref<GitWorkspaceState["account"]>(null);
const gitCommand = ref<EnabledWorkspaceCommand | null>(null);
const gitAutoSave = ref(false);
const pluginBusy = ref(false);

const hasPdfAgent = computed(() =>
    activePlugins.some(
    (plugin) =>
      plugin.manifest.kind === "AgentPlugin" &&
      plugin.manifest.spec.capabilities.includes("agent.graph.patch.propose"),
  ),
);
const hasDiffCapability = computed(() =>
  activePlugins.some(
    (plugin) => plugin.manifest.kind === "AgentPlugin" || plugin.manifest.kind === "AnalysisPlugin",
  ),
);
const pluginContextMenuActions = computed(() =>
  preferences.value.showPluginContextMenuActions
    ? contextMenuContributionsFromPlugins(activePlugins)
    : [],
);
const edgeStyle = computed(() => resolveEdgeStyle(activePlugins));
const theme = computed(() => resolveTheme(activePlugins));
const themeStyle = computed(() => themeCssVariables(theme.value));
const workspaceCommands = computed(() => workspaceCommandsFromPlugins(activePlugins));
const exportCommand = computed(() => workspaceCommands.value.find((command) => command.category === "export"));
const folderCommand = computed(() => workspaceCommands.value.find((command) => command.category === "folder"));
const availableGitCommand = computed(() => workspaceCommands.value.find((command) => command.category === "git"));
const gitPatch = computed(() => normalizePluginGraphPatch(gitSnapshot.value?.graphPatch));

const edgeTypeLabel = (type: ResearchEdgeType) => t(edgeTypeMessageKeys[type]);
const layoutLabel = (mode: LayoutMode) => t(layoutLabelKeys[mode] as never);
const linkFilterLabel = (filter: LinkLegendFilter) => t(linkFilterLabelKeys[filter] as never);

const diffVersions = computed<DiffVersion[]>(() => {
  const versions: DiffVersion[] = [];
  for (let index = history.value.length - 1; index >= 0; index -= 1) {
    const entry = history.value[index];
    versions.push({
      id: `history-${index}`,
      label: t("diff.historyVersion", { label: entry.label }),
      project: entry.project,
    });
  }
  versions.push({ id: "current", label: t("diff.currentVersion"), project: project.value });
  return versions;
});

const diffBase = computed(() => diffVersions.value.find((version) => version.id === diffBaseId.value)?.project ?? null);
const diffCompare = computed(() => diffVersions.value.find((version) => version.id === diffCompareId.value)?.project ?? null);
const diffEnabled = computed(() => diffOpen.value && diffBase.value !== diffCompare.value);
const diff = useCanvasDiff(diffBase, diffCompare, diffEnabled);
const diffResult = computed(() => diff.result.value);
const diffOverlay = computed(() => diff.overlay.value);
const diffLoading = computed(() => diff.loading.value);
const diffError = computed(() => diff.error.value);
let preferencesFrame: number | null = null;

function downloadProject(nextProject: typeof project.value) {
  const blob = new Blob([JSON.stringify(nextProject, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${nextProject.title.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-")}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

function showOperationError(error: unknown) {
  console.warn("Research Canvas operation failed", error);
  showNotice(t("toast.operationFailed"));
}

async function applyAgentPatch(patch: PluginGraphPatch) {
  pdfDialogOpen.value = false;
  workspace.applyGraphPatch(patch);
  pdfCompileResult.value = null;
  pdfCompileError.value = "";
  showNotice(t("agent.patchApplied"));
  try {
    const result = await compileProject(workspace.projectRef.value);
    pdfCompileResult.value = result;
    highlightChain.value = {
      nodeIds: result.logicChain.nodeIds,
      edgeIds: result.logicChain.edgeIds,
    };
    showNotice(t("agent.highlightChain"));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    pdfCompileError.value = message;
    showNotice(t("agent.compileFailed", { error: message }));
  }
}

function rejectAgentPatch() {
  reviewJobId.value = null;
  pdfDialogOpen.value = false;
  highlightChain.value = null;
  showNotice(t("agent.patchRejected"));
}

function requestCreate(type: ResearchNodeType, x: number, y: number) {
  openComposer({ type, x, y });
}

function toggleConnectMode() {
  setConnectMode(!connectMode.value);
  if (connectMode.value) {
    showNotice(`${edgeTypeLabel(connectType.value)} 路 ${t("workspace.connectInstruction")}`);
  } else {
    clearNotice();
  }
}

function addNote() {
  openComposer({ type: "note", x: 610, y: 430 });
}

function applyDefaultLayout() {
  const mode = preferences.value.defaultLayout;
  workspace.applyLayout(mode, linkFilter.value);
  layoutMode.value = mode;
  showNotice(t("toast.layoutApplied", { layout: layoutLabel(mode) }));
}

function exportProject() {
  downloadProject(project.value);
}

async function saveProject() {
  try {
    const result = await saveProjectNative(project.value);
    if (result) {
      showNotice(t("workspace.projectSaved"));
      menuOpen.value = false;
    }
  } catch (error) {
    if (error instanceof Error && error.message === "DESKTOP_REQUIRED") {
      downloadProject(project.value);
      showNotice(t("workspace.projectSaved"));
      menuOpen.value = false;
      return;
    }
    showOperationError(error);
  }
}

async function importProject() {
  try {
    const result = await importProjectNative();
    if (!result) return;
    workspace.replaceProject(result.project, t("workspace.importProject"));
    menuOpen.value = false;
    layoutMode.value = null;
    linkFilter.value = null;
    showNotice(t("workspace.projectImported"));
  } catch (error) {
    showOperationError(error);
  }
}

async function runPluginExport(format: "pdf" | "svg" | "png") {
  if (!exportCommand.value) return;
  try {
    const path = await exportProjectWithPlugin(project.value, exportCommand.value, format);
    if (path) showNotice(`${format.toUpperCase()} 路 ${t("workspace.exportComplete")}`);
  } catch (error) {
    showOperationError(error);
  }
}

async function openFolder() {
  if (!folderCommand.value) return;
  try {
    const result = await openFolderWorkspace(folderCommand.value);
    if (!result) return;
    menuOpen.value = false;
    folderWorkspace.value = { root: result.path, projects: result.projects };
  } catch (error) {
    showOperationError(error);
  }
}

async function openGit() {
  if (!availableGitCommand.value) return;
  try {
    const snapshot = await openGitWorkspace(availableGitCommand.value);
    if (!snapshot) return;
    const account = await readGitHubAccount(availableGitCommand.value);
    menuOpen.value = false;
    gitCommand.value = availableGitCommand.value;
    gitSnapshot.value = snapshot;
    gitHubAccount.value = account;
  } catch (error) {
    showOperationError(error);
  }
}

async function saveGitSnapshot() {
  if (!gitCommand.value || !gitSnapshot.value?.isRepository || pluginBusy.value) return;
  pluginBusy.value = true;
  try {
    const snapshot = await gitAutosaveProject(gitCommand.value, gitSnapshot.value.repoPath, project.value);
    gitSnapshot.value = snapshot;
    showNotice(t("workspace.gitSnapshotSaved"));
  } catch (error) {
    showOperationError(error);
  } finally {
    pluginBusy.value = false;
  }
}

async function initializeGit() {
  if (!gitCommand.value || !gitSnapshot.value || gitSnapshot.value.isRepository || pluginBusy.value) return;
  pluginBusy.value = true;
  try {
    gitSnapshot.value = await initializeGitWorkspace(gitCommand.value, gitSnapshot.value.repoPath);
    showNotice(t("workspace.gitInitialized"));
  } catch (error) {
    showOperationError(error);
  } finally {
    pluginBusy.value = false;
  }
}

async function refreshGitHubAccount() {
  if (!gitCommand.value || pluginBusy.value) return;
  pluginBusy.value = true;
  try {
    gitHubAccount.value = await readGitHubAccount(gitCommand.value);
  } catch (error) {
    showOperationError(error);
  } finally {
    pluginBusy.value = false;
  }
}

async function loginGitHub() {
  if (!gitCommand.value || pluginBusy.value) return;
  pluginBusy.value = true;
  try {
    gitHubAccount.value = await loginGitHubAccount(gitCommand.value);
    showNotice(t("workspace.githubLoginComplete"));
  } catch (error) {
    showOperationError(error);
  } finally {
    pluginBusy.value = false;
  }
}

async function generateGitHubKey() {
  if (!gitCommand.value || pluginBusy.value) return;
  pluginBusy.value = true;
  try {
    const login = gitHubAccount.value?.login ?? "research-canvas";
    gitHubAccount.value = await generateGitHubSshKey(gitCommand.value, `${login}@github.com`);
    showNotice(t("workspace.githubKeyGenerated"));
  } catch (error) {
    showOperationError(error);
  } finally {
    pluginBusy.value = false;
  }
}

async function uploadGitHubKey(path: string) {
  if (!gitCommand.value || pluginBusy.value) return;
  pluginBusy.value = true;
  try {
    gitHubAccount.value = await uploadGitHubSshKey(gitCommand.value, path);
    showNotice(t("workspace.githubKeyUploaded"));
  } catch (error) {
    showOperationError(error);
  } finally {
    pluginBusy.value = false;
  }
}

function openDiffPanel() {
  openDiff(history.value.length > 0 ? `history-${history.value.length - 1}` : "current");
}

function closeDiffPanel() {
  closeDiff();
}

function handleLegendFilter(nextFilter: LinkLegendFilter | null) {
  const mode = layoutMode.value ?? preferences.value.defaultLayout;
  linkFilter.value = nextFilter;
  layoutMode.value = mode;
  workspace.applyLayout(mode, nextFilter);
  showNotice(
    nextFilter
      ? t("toast.linksFiltered", { relation: linkFilterLabel(nextFilter) })
      : t("toast.linksRestored", { layout: layoutLabel(mode) }),
  );
}

function handleSelectNode(nodeId: string) {
  workspace.selectNode(nodeId);
  inspectorOpen.value = true;
}

function handleSelectEdge(edgeId: string) {
  workspace.selectEdge(edgeId);
  inspectorOpen.value = true;
}

function handleRequestConnect(nodeId: string) {
  workspace.selectNode(nodeId);
  setConnectMode(true);
  showNotice(`${edgeTypeLabel(connectType.value)} 路 ${t("workspace.connectInstruction")}`);
}

function handleDuplicateNode(nodeId: string) {
  workspace.duplicateNode(nodeId);
  inspectorOpen.value = true;
  showNotice(t("toast.nodeDuplicated"));
}

function handleDeleteNode(nodeId: string) {
  workspace.removeNode(nodeId);
  showNotice(t("toast.nodeDeleted"));
}

function handleReverseEdge(edgeId: string) {
  workspace.reverseEdge(edgeId);
  showNotice(t("toast.relationReversed"));
}

function handleDeleteEdge(edgeId: string) {
  workspace.removeEdge(edgeId);
  showNotice(t("toast.relationDeleted"));
}

function handleCreateEdge(source: string, target: string) {
  const edgeId = workspace.createEdge(source, target, connectType.value);
  setConnectMode(false);
  if (edgeId) {
    inspectorOpen.value = true;
    showNotice(`${edgeTypeLabel(connectType.value)} 路 ${t("workspace.relationCreated")}`);
  }
}

async function handlePluginContextMenuAction(
  action: CanvasPluginContextMenuAction,
  context: WorkspaceContextMenuState,
) {
  try {
    if (!action.plugin) {
      showOperationError(new Error("PLUGIN_CONTEXT_ACTION_MISSING_PLUGIN"));
      return;
    }
    const appAction = action as unknown as AppPluginContextMenuAction;
    const result = await runAnalysisPlugin(
      appAction.plugin,
      {
        operation: "context-menu",
        context: {
          actionId: action.contributionId,
          scope: context.scope,
          targetId: context.targetId,
          projectId: project.value.id,
          position: { x: context.flowX, y: context.flowY },
        },
      },
      action.capability,
    );
    const output = JSON.stringify(result.output);
    showNotice(
      t("toast.pluginResult", {
        plugin: appAction.plugin.name,
        result: `${output.slice(0, 160)}${output.length > 160 ? "…" : ""}`,
      }),
    );
  } catch (error) {
    showOperationError(error);
  }
}

function runShortcut(event: KeyboardEvent) {
  if (
    event.defaultPrevented ||
    event.repeat ||
    isEditableShortcutTarget(event.target) ||
    settingsOpen.value ||
    pluginStoreOpen.value ||
    composer.value ||
    reviewJobId.value
  ) {
    return;
  }
  const binding = shortcutFromKeyboardEvent(event);
  if (!binding) return;
  const action = SHORTCUT_ACTIONS.find((candidate) => preferences.value.shortcuts[candidate] === binding);
  if (!action) return;
    event.preventDefault();
  switch (action) {
    case "menu":
      toggleMenu();
      break;
    case "add":
      requestAdd();
      break;
    case "connect":
      toggleConnectMode();
      break;
    case "note":
      addNote();
      break;
    case "find":
      searchOpen.value = true;
      break;
    case "layout":
      applyDefaultLayout();
      break;
    case "undo":
      workspace.undo();
      break;
    case "redo":
      workspace.redo();
      break;
    case "export":
      exportProject();
      break;
    case "settings":
      menuOpen.value = false;
      settingsOpen.value = true;
      break;
  }
}

function closeTransientUiOnEscape(event: KeyboardEvent) {
  if (event.key !== "Escape") return;
  closeUiTransient();
}

function handleSettingsSave(nextPreferences: WorkspacePreferences) {
  setPreferences(nextPreferences);
  window.localStorage.setItem(preferencesStorageKey, JSON.stringify(nextPreferences));
  settingsOpen.value = false;
  showNotice(t("toast.settingsSaved"));
}

function resetProject() {
  workspace.resetDemo();
  resetForProject();
}

async function openFolderProject(path: string) {
  try {
    const result = await importProjectAtPath(path);
    workspace.replaceProject(result.project, t("workspace.importProject"));
    folderWorkspace.value = null;
    layoutMode.value = null;
    linkFilter.value = null;
    showNotice(t("workspace.projectImported"));
  } catch (error) {
    showOperationError(error);
  }
}

function createNodeFromComposer(draft: NodeDraft, x: number, y: number) {
  workspace.createNode(draft, x, y);
  closeComposer();
  inspectorOpen.value = true;
  showNotice(t("toast.nodeAdded"));
}

function handlePdfReady(jobId: string) {
  pdfDialogOpen.value = false;
  reviewJobId.value = jobId;
}

function closeGitWorkspace() {
  gitSnapshot.value = null;
  gitCommand.value = null;
  gitHubAccount.value = null;
  gitAutoSave.value = false;
}

function applyGitPatch(acceptedPatch: PluginGraphPatch | null) {
  if (!acceptedPatch) return;
  workspace.applyGraphPatch(acceptedPatch);
  if (gitSnapshot.value) gitSnapshot.value = { ...gitSnapshot.value, graphPatch: undefined };
  showNotice(t("workspace.patchApplied"));
}

const stopNoticeWatch = watch(notice, (current, _previous, onCleanup) => {
  if (!current) return;
  const timer = window.setTimeout(() => {
    if (notice.value?.id === current.id) notice.value = null;
  }, toastVisibleMs);
  onCleanup(() => window.clearTimeout(timer));
});

onMounted(() => {
  preferencesFrame = window.requestAnimationFrame(() => {
    const saved = window.localStorage.getItem(preferencesStorageKey);
    if (!saved) return;
    try {
      preferences.value = normalizeWorkspacePreferences(JSON.parse(saved) as Partial<WorkspacePreferences>);
    } catch {
      window.localStorage.removeItem(preferencesStorageKey);
    }
  });

  window.addEventListener("keydown", runShortcut);
  window.addEventListener("keydown", closeTransientUiOnEscape);
});

onBeforeUnmount(() => {
  stopNoticeWatch();
  if (preferencesFrame !== null) window.cancelAnimationFrame(preferencesFrame);
  window.removeEventListener("keydown", runShortcut);
  window.removeEventListener("keydown", closeTransientUiOnEscape);
});

watch([gitAutoSave, gitCommand, gitSnapshot], (_current, _previous, onCleanup) => {
  if (!gitAutoSave.value || !gitCommand.value || !gitSnapshot.value?.isRepository) return;
  const timer = window.setInterval(() => {
    void saveGitSnapshot();
  }, 300_000);
  onCleanup(() => window.clearInterval(timer));
});
</script>

<template>
  <main
    class="flex h-screen min-h-[680px] w-screen flex-col overflow-hidden bg-paper text-ink"
    :style="themeStyle"
    :data-plugin-theme="theme?.id ?? 'research-light'"
  >
    <WorkspaceTopbar
      :can-undo="canUndo"
      :can-redo="canRedo"
      :connect-mode="connectMode"
      :connect-type="connectType"
      :command-density="preferences.commandDensity"
      :hover-delay="preferences.hoverDelay"
      :shortcuts="preferences.shortcuts"
      :active-layout="layoutMode"
      :compare-enabled="hasDiffCapability"
      :import-pdf-enabled="hasPdfAgent"
      :export-formats="exportCommand?.formats"
      @menu="toggleMenu"
      @add="requestAdd"
      @add-type="(type) => requestCreate(type, 720, 430)"
      @connect="toggleConnectMode"
      @connect-type="(type) => { connectType = type; connectMode = true; showNotice(`${edgeTypeLabel(type)} 路 ${t('workspace.connectInstruction')}`) }"
      @note="addNote"
      @find="searchOpen = true"
      @layout="(mode) => { workspace.applyLayout(mode, linkFilter); layoutMode = mode; showNotice(t('toast.layoutApplied', { layout: layoutLabel(mode) })) }"
      @compare="openDiffPanel"
      @undo="workspace.undo"
      @redo="workspace.redo"
      @export="exportProject"
      @export-format="runPluginExport"
      @import-pdf="pdfDialogOpen = true"
    />

    <div
      class="relative grid min-h-0 flex-1 transition-[grid-template-columns] duration-[360ms] ease-[cubic-bezier(.22,1,.36,1)] motion-reduce:transition-none"
      :class="inspectorOpen ? 'grid-cols-[minmax(0,1fr)_min(320px,40vw)]' : 'grid-cols-[minmax(0,1fr)_0px]'"
    >
      <section class="grid min-h-0 grid-rows-[42px_minmax(0,1fr)] bg-canvas">
        <div class="flex items-center gap-2 border-b border-ink/10 px-6 font-serif text-[12px] text-olive">
          <button class="hover:text-blue" @click="menuOpen = true">{{ t("workspace.projects") }}</button>
          <IconChevronRight size="13" stroke="1.3" />
          <span>{{ project.discipline }}</span>
          <IconChevronRight size="13" stroke="1.3" />
          <span>{{ project.title }}</span>
        </div>
        <ResearchGraphCanvas
          :project="project"
          :selected-node-id="selectedNodeId"
          :selected-edge-id="selectedEdgeId"
          :add-request="addRequest"
          :connect-mode="connectMode"
          :connect-type="connectType"
          :inspector-open="inspectorOpen"
          :link-filter="linkFilter"
          :show-mini-map="preferences.showMiniMap"
          :show-mini-map-relations="theme?.components?.miniMap?.showRelations ?? false"
          :show-link-counts="preferences.showLinkCounts"
          :trackpad-sensitivity="preferences.trackpadSensitivity"
          :trackpad-filter-strength="preferences.trackpadFilterStrength"
          :edge-style="edgeStyle"
          :reference-viewport="layoutMode === null && linkFilter === null"
          :highlight-chain="highlightChain"
          :context-menus="preferences.contextMenus"
          :radial-menu="preferences.radialMenu"
          :shortcuts="preferences.shortcuts"
          :plugin-context-menu-actions="pluginContextMenuActions"
          :trackpad-frame="trackpadFrame"
          :diff-overlay="diffOpen && diffMode === 'overlay' ? diffOverlay : null"
          :diff-focus="diffOpen && diffMode === 'overlay' ? diffFocus : null"
          @legend-filter="handleLegendFilter"
          @select-node="handleSelectNode"
          @select-edge="handleSelectEdge"
          @move-node="workspace.moveNode"
          @request-connect="handleRequestConnect"
          @duplicate-node="handleDuplicateNode"
          @delete-node="handleDeleteNode"
          @reverse-edge="handleReverseEdge"
          @delete-edge="handleDeleteEdge"
          @apply-default-layout="applyDefaultLayout"
          @create-edge="handleCreateEdge"
          @request-create="requestCreate"
          @plugin-context-menu-action="handlePluginContextMenuAction"
        />
      </section>

      <div
        class="min-w-0 overflow-hidden transition-[opacity,transform] duration-300 ease-out motion-reduce:transition-none"
        :class="inspectorOpen ? 'translate-x-0 opacity-100' : 'pointer-events-none translate-x-5 opacity-0'"
        :aria-hidden="!inspectorOpen"
        :inert="!inspectorOpen"
      >
        <InspectorPanel
          :node="selectedNode"
          :edge="selectedEdge"
          :nodes="project.nodes"
          @update="workspace.updateNode"
          @update-edge="workspace.updateEdge"
          @delete="workspace.removeNode"
          @delete-edge="workspace.removeEdge"
          @reverse-edge="workspace.reverseEdge"
          @close="inspectorOpen = false"
        />
      </div>
      <button v-if="!inspectorOpen" class="absolute right-3 top-20 z-20 rounded-full border border-ink/25 bg-paper p-2 text-blue shadow-sm" @click="inspectorOpen = true" :aria-label="t('inspector.open')">
        <IconChevronRight class="rotate-180" size="17" stroke="1.35" />
      </button>
    </div>

    <div v-if="connectMode" class="pointer-events-none fixed left-1/2 top-[62px] z-50 -translate-x-1/2 rounded-full border border-blue/35 bg-blue-soft px-4 py-2 font-serif text-[11px] text-blue shadow-sm">
      <IconPlugConnected class="mr-2 inline" size="15" stroke="1.35" />
      {{ edgeTypeLabel(connectType) }} 路 {{ t("workspace.connectInstruction") }}
    </div>
    <div v-if="notice && !connectMode" :key="notice.id" class="workspace-toast" role="status" aria-live="polite">{{ notice.text }}</div>

    <WorkspaceDialogs
      v-if="menuOpen"
      view="project-menu"
      :project="project"
      :on-close="() => { menuOpen = false }"
      :on-settings="() => { menuOpen = false; settingsOpen = true }"
      :on-plugins="() => { menuOpen = false; pluginStoreOpen = true }"
      :on-save-project="saveProject"
      :on-import-project="importProject"
      :on-folder-workspace="folderCommand ? openFolder : undefined"
      :on-git-workspace="availableGitCommand ? openGit : undefined"
      :on-reset="resetProject"
    />
    <WorkspaceDialogs
      v-if="settingsOpen"
      view="settings"
      :preferences="preferences"
      :on-close="() => { settingsOpen = false }"
      :on-save="handleSettingsSave"
    />
    <PluginStoreDialog v-if="pluginStoreOpen" @close="pluginStoreOpen = false" />

    <WorkspacePluginDialogs
      v-if="folderWorkspace"
      :root="folderWorkspace.root"
      :projects="folderWorkspace.projects"
      @close="folderWorkspace = null"
      @open="openFolderProject"
    />
    <WorkspacePluginDialogs
      v-if="gitSnapshot"
      :root="gitSnapshot.repoPath"
      :projects="[]"
      :snapshot="gitSnapshot"
      :account="gitHubAccount"
      :auto-save="gitAutoSave"
      :busy="pluginBusy"
      :patch="gitPatch"
      @close="closeGitWorkspace"
      @open="openFolderProject"
      @toggle-auto-save="gitAutoSave = $event"
      @initialize="initializeGit"
      @refresh-account="refreshGitHubAccount"
      @login="loginGitHub"
      @generate-ssh-key="generateGitHubKey"
      @upload-ssh-key="uploadGitHubKey"
      @save-now="saveGitSnapshot"
      @apply-patch="applyGitPatch"
    />
    <WorkspaceDialogs
      v-if="searchOpen"
      view="search"
      :project="project"
      :on-close="() => { searchOpen = false }"
      :on-select="handleSelectNode"
    />
    <PdfUploadDialog v-if="pdfDialogOpen" @close="pdfDialogOpen = false" @ready="handlePdfReady" />
    <AgentReviewPanel
      v-if="reviewJobId"
      :job-id="reviewJobId"
      :compile-result="pdfCompileResult"
      :compile-error="pdfCompileError"
      @close="reviewJobId = null"
      @apply="(patch) => void applyAgentPatch(patch)"
      @reject="rejectAgentPatch"
      @rollback="workspace.undo"
    />
    <WorkspaceDialogs
      v-if="composer"
      view="composer"
      :key="`${composer.type}-${composer.x}-${composer.y}`"
      :state="composer"
      :on-close="() => { composer = null }"
      :on-create="createNodeFromComposer"
    />
    <DiffPanel
      v-if="diffOpen"
      :versions="diffVersions"
      :base-id="diffBaseId"
      :compare-id="diffCompareId"
      :mode="diffMode"
      :result="diffResult"
      :loading="diffLoading"
      :error="diffError ? t('diff.error', { error: diffError }) : null"
      @base-change="(id) => { diffBaseId = id; diffFocus = null }"
      @compare-change="(id) => { diffCompareId = id; diffFocus = null }"
      @mode-change="diffMode = $event"
      @close="closeDiffPanel"
      @focus="focusDiff"
    />
  </main>
</template>

<style scoped>
/* Shared workspace visual tokens remain in app/globals.css. */
</style>
