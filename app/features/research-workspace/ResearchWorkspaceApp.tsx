"use client";

import { IconChevronRight, IconPlugConnected } from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  LayoutMode,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../lib/research-types";
import { useI18n } from "../../i18n/provider";
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
  type FolderProjectSummary,
  type GitHubAccountStatus,
  type GitWorkspaceSnapshot,
} from "../../platform/native-project";
import {
  normalizePluginGraphPatch,
  workspaceCommandsFromPlugins,
  type EnabledWorkspaceCommand,
} from "../../plugins/workspace";
import {
  contextMenuContributionsFromPlugins,
} from "../../plugins/context-menu";
import { resolveEdgeStyle } from "../../plugins/edge-style";
import { usePluginHost } from "../../plugins/plugin-host";
import { resolveTheme, themeCssVariables } from "../../plugins/theme";
import { runAnalysisPlugin } from "../../plugins/tauri-client";
import {
  compileProject,
  type PdfCompileResult,
} from "../../platform/agent-client";
import type { PluginGraphPatch } from "../../plugins/contracts";
import { ResearchGraphCanvas } from "./canvas/research-graph-canvas";
import { InspectorPanel } from "./components/inspector-panel";
import { PdfUploadDialog } from "./components/pdf-upload-dialog";
import { AgentReviewPanel } from "./components/agent-review-panel";
import { PluginStoreDialog } from "./components/plugin-store-dialog";
import {
  FolderWorkspaceDialog,
  GitWorkspaceDialog,
} from "./components/workspace-plugin-dialogs";
import {
  NodeComposer,
  ProjectMenu,
  SearchPalette,
  SettingsDialog,
  type ComposerState,
} from "./components/workspace-dialogs";
import { WorkspaceTopbar } from "./components/workspace-topbar";
import { useWorkspaceProject } from "./hooks/use-workspace-project";
import type { LinkLegendFilter } from "./workspace-layout";
import {
  defaultWorkspacePreferences,
  normalizeWorkspacePreferences,
  type WorkspacePreferences,
} from "./workspace-preferences";
import {
  isEditableShortcutTarget,
  SHORTCUT_ACTIONS,
  shortcutFromKeyboardEvent,
} from "./workspace-shortcuts";
import type { WorkspaceContextMenuState } from "./workspace-context-menu";
import { edgeTypeMessageKeys } from "./workspace-edge-labels";

const preferencesStorageKey = "research-canvas.workspace-preferences.v2";
const toastVisibleMs = 3_200;

const layoutLabelKeys = {
  "evidence-chain": "layout.evidenceChain",
  "refutation-chain": "layout.refutationChain",
  tree: "layout.tree",
  huffman: "layout.huffman",
  table: "layout.table",
  "neural-network": "layout.neural",
} as const;

const linkFilterLabelKeys = {
  causal: "relation.causal",
  control: "relation.control",
  derived: "relation.derived",
  contradicts: "relation.contradicts",
} as const;

type WorkspaceNotice = { id: number; text: string };

function downloadProject(project: ReturnType<typeof useWorkspaceProject>["project"]) {
  const blob = new Blob([JSON.stringify(project, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${project.title.toLowerCase().replaceAll(/[^a-z0-9]+/g, "-")}.json`;
  link.click();
  URL.revokeObjectURL(url);
}

/**
 * Thin composition root for the white, canvas-first desktop experience.
 * 白色画布优先桌面体验的轻量组合根。
 */
export function ResearchWorkspaceApp() {
  const { t } = useI18n();
  const { activePlugins } = usePluginHost();
  const workspace = useWorkspaceProject();
  const [menuOpen, setMenuOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [pluginStoreOpen, setPluginStoreOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(true);
  const [connectMode, setConnectMode] = useState(false);
  const [connectType, setConnectType] = useState<ResearchEdgeType>("causes");
  const [addRequest, setAddRequest] = useState(0);
  const [composer, setComposer] = useState<ComposerState | null>(null);
  const [layoutMode, setLayoutMode] = useState<LayoutMode | null>(null);
  const [linkFilter, setLinkFilter] = useState<LinkLegendFilter | null>(null);
  const [notice, setNotice] = useState<WorkspaceNotice | null>(null);
  const noticeSequence = useRef(0);
  const [preferences, setPreferences] = useState<WorkspacePreferences>(
    defaultWorkspacePreferences,
  );
  const [folderWorkspace, setFolderWorkspace] = useState<{
    root: string;
    projects: FolderProjectSummary[];
  } | null>(null);
  const [gitSnapshot, setGitSnapshot] = useState<GitWorkspaceSnapshot | null>(null);
  const [gitHubAccount, setGitHubAccount] = useState<GitHubAccountStatus | null>(null);
  const [gitCommand, setGitCommand] = useState<EnabledWorkspaceCommand | null>(null);
  const [gitAutoSave, setGitAutoSave] = useState(false);
  const [pluginBusy, setPluginBusy] = useState(false);
  const [pdfDialogOpen, setPdfDialogOpen] = useState(false);
  const [reviewJobId, setReviewJobId] = useState<string | null>(null);
  const [highlightChain, setHighlightChain] = useState<{
    nodeIds: string[];
    edgeIds: string[];
  } | null>(null);
  const [pdfCompileResult, setPdfCompileResult] = useState<PdfCompileResult | null>(null);
  const [pdfCompileError, setPdfCompileError] = useState("");

  const edgeTypeLabel = useCallback(
    (type: ResearchEdgeType) => t(edgeTypeMessageKeys[type]),
    [t],
  );
  const layoutLabel = useCallback((mode: LayoutMode) => t(layoutLabelKeys[mode]), [t]);
  const linkFilterLabel = useCallback(
    (filter: LinkLegendFilter) => t(linkFilterLabelKeys[filter]),
    [t],
  );
  const showNotice = useCallback((text: string) => {
    noticeSequence.current += 1;
    setNotice({ id: noticeSequence.current, text });
  }, []);
  const showOperationError = useCallback(
    (error: unknown) => {
      console.warn("Research Canvas operation failed", error);
      showNotice(t("toast.operationFailed"));
    },
    [showNotice, t],
  );

  /** 阶段 3→4：应用 Agent 补丁后自动触发图编译器（不变式 + blockHash + 逻辑链 + BP）。 */
  const applyAgentPatch = useCallback(
    async (patch: PluginGraphPatch) => {
      setReviewJobId(null);
      setPdfDialogOpen(false);
      workspace.applyGraphPatch(patch);
      setPdfCompileResult(null);
      setPdfCompileError("");
      showNotice(t("agent.patchApplied"));
      try {
        const result = await compileProject(workspace.project);
        setPdfCompileResult(result);
        setHighlightChain({
          nodeIds: result.logicChain.nodeIds,
          edgeIds: result.logicChain.edgeIds,
        });
        showNotice(t("agent.highlightChain"));
      } catch (compileError) {
        const message = compileError instanceof Error ? compileError.message : String(compileError);
        setPdfCompileError(message);
        console.warn("Graph compile failed", compileError);
      }
    },
    [showNotice, t, workspace],
  );

  const rejectAgentPatch = useCallback(() => {
    setReviewJobId(null);
    setPdfDialogOpen(false);
    setHighlightChain(null);
    showNotice(t("agent.patchRejected"));
  }, [showNotice, t]);

  const pluginContextMenuActions = useMemo(
    () =>
      preferences.showPluginContextMenuActions
        ? contextMenuContributionsFromPlugins(activePlugins)
        : [],
    [activePlugins, preferences.showPluginContextMenuActions],
  );
  const edgeStyle = useMemo(
    () => resolveEdgeStyle(activePlugins),
    [activePlugins],
  );
  const theme = useMemo(() => resolveTheme(activePlugins), [activePlugins]);
  const themeStyle = useMemo(() => themeCssVariables(theme), [theme]);

  const workspaceCommands = useMemo(
    () => workspaceCommandsFromPlugins(activePlugins),
    [activePlugins],
  );
  const exportCommand = workspaceCommands.find((command) => command.category === "export");
  const folderCommand = workspaceCommands.find((command) => command.category === "folder");
  const availableGitCommand = workspaceCommands.find((command) => command.category === "git");
  const gitPatch = useMemo(
    () => normalizePluginGraphPatch(gitSnapshot?.graphPatch),
    [gitSnapshot?.graphPatch],
  );

  const requestCreate = useCallback(
    (type: ResearchNodeType, x: number, y: number) => {
      setComposer({ type, x, y });
    },
    [],
  );

  const toggleConnectMode = useCallback(() => {
    setConnectMode((current) => {
      const next = !current;
      if (next) showNotice(`${edgeTypeLabel(connectType)} · ${t("workspace.connectInstruction")}`);
      else setNotice(null);
      return next;
    });
  }, [connectType, edgeTypeLabel, showNotice, t]);

  const addNote = useCallback(() => {
    setComposer({ type: "note", x: 610, y: 430 });
  }, []);

  const applyDefaultLayout = useCallback(() => {
    const mode = preferences.defaultLayout;
    workspace.applyLayout(mode, linkFilter);
    setLayoutMode(mode);
    showNotice(t("toast.layoutApplied", { layout: layoutLabel(mode) }));
  }, [layoutLabel, linkFilter, preferences.defaultLayout, showNotice, t, workspace]);

  const exportProject = useCallback(() => downloadProject(workspace.project), [workspace.project]);

  const saveProject = useCallback(async () => {
    try {
      const result = await saveProjectNative(workspace.project);
      if (result) {
        showNotice(t("workspace.projectSaved"));
        setMenuOpen(false);
      }
    } catch (error) {
      if (error instanceof Error && error.message === "DESKTOP_REQUIRED") {
        downloadProject(workspace.project);
        showNotice(t("workspace.projectSaved"));
        setMenuOpen(false);
        return;
      }
      showOperationError(error);
    }
  }, [showNotice, showOperationError, t, workspace.project]);

  const importProject = useCallback(async () => {
    try {
      const result = await importProjectNative();
      if (!result) return;
      workspace.replaceProject(result.project, t("workspace.importProject"));
      setMenuOpen(false);
      setLayoutMode(null);
      setLinkFilter(null);
      showNotice(t("workspace.projectImported"));
    } catch (error) {
      showOperationError(error);
    }
  }, [showNotice, showOperationError, t, workspace]);

  const runPluginExport = useCallback(
    async (format: "pdf" | "svg" | "png") => {
      if (!exportCommand) return;
      try {
        const path = await exportProjectWithPlugin(workspace.project, exportCommand, format);
        if (path) showNotice(`${format.toUpperCase()} · ${t("workspace.exportComplete")}`);
      } catch (error) {
        showOperationError(error);
      }
    },
    [exportCommand, showNotice, showOperationError, t, workspace.project],
  );

  const openFolder = useCallback(async () => {
    if (!folderCommand) return;
    try {
      const result = await openFolderWorkspace(folderCommand);
      if (!result) return;
      setMenuOpen(false);
      setFolderWorkspace({ root: result.path, projects: result.projects });
    } catch (error) {
      showOperationError(error);
    }
  }, [folderCommand, showOperationError]);

  const openGit = useCallback(async () => {
    if (!availableGitCommand) return;
    try {
      const snapshot = await openGitWorkspace(availableGitCommand);
      if (!snapshot) return;
      const account = await readGitHubAccount(availableGitCommand);
      setMenuOpen(false);
      setGitCommand(availableGitCommand);
      setGitSnapshot(snapshot);
      setGitHubAccount(account);
    } catch (error) {
      showOperationError(error);
    }
  }, [availableGitCommand, showOperationError]);

  const saveGitSnapshot = useCallback(async () => {
    if (!gitCommand || !gitSnapshot?.isRepository || pluginBusy) return;
    setPluginBusy(true);
    try {
      const snapshot = await gitAutosaveProject(
        gitCommand,
        gitSnapshot.repoPath,
        workspace.project,
      );
      setGitSnapshot(snapshot);
      showNotice(t("workspace.gitSnapshotSaved"));
    } catch (error) {
      showOperationError(error);
    } finally {
      setPluginBusy(false);
    }
  }, [gitCommand, gitSnapshot, pluginBusy, showNotice, showOperationError, t, workspace.project]);

  const initializeGit = useCallback(async () => {
    if (!gitCommand || !gitSnapshot || gitSnapshot.isRepository || pluginBusy) return;
    setPluginBusy(true);
    try {
      const snapshot = await initializeGitWorkspace(gitCommand, gitSnapshot.repoPath);
      setGitSnapshot(snapshot);
      showNotice(t("workspace.gitInitialized"));
    } catch (error) {
      showOperationError(error);
    } finally {
      setPluginBusy(false);
    }
  }, [gitCommand, gitSnapshot, pluginBusy, showNotice, showOperationError, t]);

  const refreshGitHubAccount = useCallback(async () => {
    if (!gitCommand || pluginBusy) return;
    setPluginBusy(true);
    try {
      setGitHubAccount(await readGitHubAccount(gitCommand));
    } catch (error) {
      showOperationError(error);
    } finally {
      setPluginBusy(false);
    }
  }, [gitCommand, pluginBusy, showOperationError]);

  const loginGitHub = useCallback(async () => {
    if (!gitCommand || pluginBusy) return;
    setPluginBusy(true);
    try {
      setGitHubAccount(await loginGitHubAccount(gitCommand));
      showNotice(t("workspace.githubLoginComplete"));
    } catch (error) {
      showOperationError(error);
    } finally {
      setPluginBusy(false);
    }
  }, [gitCommand, pluginBusy, showNotice, showOperationError, t]);

  const generateGitHubKey = useCallback(async () => {
    if (!gitCommand || pluginBusy) return;
    setPluginBusy(true);
    try {
      const login = gitHubAccount?.login ?? "research-canvas";
      setGitHubAccount(
        await generateGitHubSshKey(gitCommand, `${login}@github.com`),
      );
      showNotice(t("workspace.githubKeyGenerated"));
    } catch (error) {
      showOperationError(error);
    } finally {
      setPluginBusy(false);
    }
  }, [gitCommand, gitHubAccount, pluginBusy, showNotice, showOperationError, t]);

  const uploadGitHubKey = useCallback(async (path: string) => {
    if (!gitCommand || pluginBusy) return;
    setPluginBusy(true);
    try {
      setGitHubAccount(await uploadGitHubSshKey(gitCommand, path));
      showNotice(t("workspace.githubKeyUploaded"));
    } catch (error) {
      showOperationError(error);
    } finally {
      setPluginBusy(false);
    }
  }, [gitCommand, pluginBusy, showNotice, showOperationError, t]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => {
      setNotice((current) => (current?.id === notice.id ? null : current));
    }, toastVisibleMs);
    return () => window.clearTimeout(timer);
  }, [notice]);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const saved = window.localStorage.getItem(preferencesStorageKey);
      if (!saved) return;
      try {
        setPreferences(
          normalizeWorkspacePreferences(
            JSON.parse(saved) as Partial<WorkspacePreferences>,
          ),
        );
      } catch {
        window.localStorage.removeItem(preferencesStorageKey);
      }
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    if (!gitAutoSave || !gitCommand || !gitSnapshot?.isRepository) return;
    const timer = window.setInterval(() => {
      void saveGitSnapshot();
    }, 300_000);
    return () => window.clearInterval(timer);
  }, [gitAutoSave, gitCommand, gitSnapshot, saveGitSnapshot]);

  useEffect(() => {
    const runShortcut = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.repeat ||
        isEditableShortcutTarget(event.target) ||
        settingsOpen ||
        pluginStoreOpen ||
        composer
      ) {
        return;
      }
      const binding = shortcutFromKeyboardEvent(event);
      if (!binding) return;
      const action = SHORTCUT_ACTIONS.find(
        (candidate) => preferences.shortcuts[candidate] === binding,
      );
      if (!action) return;
      event.preventDefault();
      switch (action) {
        case "menu":
          setMenuOpen((current) => !current);
          break;
        case "add":
          setAddRequest((value) => value + 1);
          break;
        case "connect":
          toggleConnectMode();
          break;
        case "note":
          addNote();
          break;
        case "find":
          setSearchOpen(true);
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
          setMenuOpen(false);
          setSettingsOpen(true);
          break;
      }
    };
    window.addEventListener("keydown", runShortcut);
    return () => window.removeEventListener("keydown", runShortcut);
  }, [
    addNote,
    applyDefaultLayout,
    composer,
    exportProject,
    pluginStoreOpen,
    preferences.shortcuts,
    settingsOpen,
    toggleConnectMode,
    workspace,
  ]);

  useEffect(() => {
    const closeTransientUi = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setComposer(null);
      setSearchOpen(false);
      setMenuOpen(false);
      setSettingsOpen(false);
      setPluginStoreOpen(false);
      setConnectMode(false);
      setPdfDialogOpen(false);
    };
    window.addEventListener("keydown", closeTransientUi);
    return () => window.removeEventListener("keydown", closeTransientUi);
  }, []);

  return (
    <main
      className="flex h-screen min-h-[680px] w-screen flex-col overflow-hidden bg-paper text-ink"
      style={themeStyle}
      data-plugin-theme={theme?.id ?? "research-light"}
    >
      <WorkspaceTopbar
        canUndo={workspace.canUndo}
        canRedo={workspace.canRedo}
        connectMode={connectMode}
        connectType={connectType}
        commandDensity={preferences.commandDensity}
        hoverDelay={preferences.hoverDelay}
        shortcuts={preferences.shortcuts}
        onMenu={() => setMenuOpen((current) => !current)}
        onAdd={() => setAddRequest((value) => value + 1)}
        onAddType={(type) =>
          setComposer({
            type,
            x: 720,
            y: 430,
          })
        }
        onConnect={toggleConnectMode}
        onConnectType={(type) => {
          setConnectType(type);
          setConnectMode(true);
          showNotice(`${edgeTypeLabel(type)} · ${t("workspace.connectInstruction")}`);
        }}
        onNote={addNote}
        onFind={() => setSearchOpen(true)}
        activeLayout={layoutMode}
        onLayout={(mode) => {
          workspace.applyLayout(mode, linkFilter);
          setLayoutMode(mode);
          showNotice(t("toast.layoutApplied", { layout: layoutLabel(mode) }));
        }}
        onUndo={workspace.undo}
        onRedo={workspace.redo}
        onExport={exportProject}
        exportFormats={exportCommand?.formats}
        onExportFormat={exportCommand ? runPluginExport : undefined}
        onImportPdf={() => setPdfDialogOpen(true)}
      />

      <div
        className={`relative grid min-h-0 flex-1 transition-[grid-template-columns] duration-[360ms] ease-[cubic-bezier(.22,1,.36,1)] motion-reduce:transition-none ${
          inspectorOpen
            ? "grid-cols-[minmax(0,1fr)_min(320px,40vw)]"
            : "grid-cols-[minmax(0,1fr)_0px]"
        }`}
      >
        <section className="grid min-h-0 grid-rows-[42px_minmax(0,1fr)] bg-canvas">
          <div className="flex items-center gap-2 border-b border-ink/10 px-6 font-serif text-[12px] text-olive">
            <button className="hover:text-blue" onClick={() => setMenuOpen(true)}>
              {t("workspace.projects")}
            </button>
            <IconChevronRight size={13} stroke={1.3} />
            <span>{workspace.project.discipline}</span>
            <IconChevronRight size={13} stroke={1.3} />
            <span>{workspace.project.title}</span>
          </div>
          <ResearchGraphCanvas
            project={workspace.project}
            selectedNodeId={workspace.selectedNodeId}
            selectedEdgeId={workspace.selectedEdgeId}
            addRequest={addRequest}
            connectMode={connectMode}
            connectType={connectType}
            inspectorOpen={inspectorOpen}
            linkFilter={linkFilter}
            showMiniMap={preferences.showMiniMap}
            showMiniMapRelations={theme?.components?.miniMap?.showRelations ?? false}
            showLinkCounts={preferences.showLinkCounts}
            trackpadSensitivity={preferences.trackpadSensitivity}
            trackpadFilterStrength={preferences.trackpadFilterStrength}
            edgeStyle={edgeStyle}
            referenceViewport={layoutMode === null && linkFilter === null}
            highlightChain={highlightChain}
            contextMenus={preferences.contextMenus}
            radialMenu={preferences.radialMenu}
            shortcuts={preferences.shortcuts}
            pluginContextMenuActions={pluginContextMenuActions}
            onLegendFilter={(nextFilter) => {
              const mode = layoutMode ?? preferences.defaultLayout;
              setLinkFilter(nextFilter);
              setLayoutMode(mode);
              workspace.applyLayout(mode, nextFilter);
              showNotice(
                nextFilter
                  ? t("toast.linksFiltered", { relation: linkFilterLabel(nextFilter) })
                  : t("toast.linksRestored", { layout: layoutLabel(mode) }),
              );
            }}
            onSelectNode={(nodeId) => {
              workspace.selectNode(nodeId);
              setInspectorOpen(true);
            }}
            onSelectEdge={(edgeId) => {
              workspace.selectEdge(edgeId);
              setInspectorOpen(true);
            }}
            onMoveNode={workspace.moveNode}
            onRequestConnect={(nodeId) => {
              workspace.selectNode(nodeId);
              setConnectMode(true);
              showNotice(`${edgeTypeLabel(connectType)} · ${t("workspace.connectInstruction")}`);
            }}
            onDuplicateNode={(nodeId) => {
              workspace.duplicateNode(nodeId);
              setInspectorOpen(true);
              showNotice(t("toast.nodeDuplicated"));
            }}
            onDeleteNode={(nodeId) => {
              workspace.removeNode(nodeId);
              showNotice(t("toast.nodeDeleted"));
            }}
            onReverseEdge={(edgeId) => {
              workspace.reverseEdge(edgeId);
              showNotice(t("toast.relationReversed"));
            }}
            onDeleteEdge={(edgeId) => {
              workspace.removeEdge(edgeId);
              showNotice(t("toast.relationDeleted"));
            }}
            onApplyDefaultLayout={applyDefaultLayout}
            onCreateEdge={(source, target) => {
              const edgeId = workspace.createEdge(source, target, connectType);
              setConnectMode(false);
              if (edgeId) {
                setInspectorOpen(true);
                showNotice(`${edgeTypeLabel(connectType)} · ${t("workspace.relationCreated")}`);
              }
            }}
            onRequestCreate={requestCreate}
            onPluginContextMenuAction={async (action, context: WorkspaceContextMenuState) => {
              try {
                const result = await runAnalysisPlugin(action.plugin, {
                  operation: "context-menu",
                  context: {
                    actionId: action.contributionId,
                    scope: context.scope,
                    targetId: context.targetId,
                    projectId: workspace.project.id,
                    position: { x: context.flowX, y: context.flowY },
                  },
                });
                const output = JSON.stringify(result.output);
                showNotice(
                  t("toast.pluginResult", {
                    plugin: action.plugin.name,
                    result: `${output.slice(0, 160)}${output.length > 160 ? "…" : ""}`,
                  }),
                );
              } catch (error) {
                showOperationError(error);
              }
            }}
          />
        </section>

        <div
          className={`min-w-0 overflow-hidden transition-[opacity,transform] duration-300 ease-out motion-reduce:transition-none ${
            inspectorOpen
              ? "translate-x-0 opacity-100"
              : "pointer-events-none translate-x-5 opacity-0"
          }`}
          aria-hidden={!inspectorOpen}
          inert={!inspectorOpen}
        >
          <InspectorPanel
            node={workspace.selectedNode}
            edge={workspace.selectedEdge}
            nodes={workspace.project.nodes}
            onUpdate={workspace.updateNode}
            onUpdateEdge={workspace.updateEdge}
            onDelete={workspace.removeNode}
            onDeleteEdge={workspace.removeEdge}
            onReverseEdge={workspace.reverseEdge}
            onClose={() => setInspectorOpen(false)}
          />
        </div>
        {!inspectorOpen && (
          <button
            className="absolute right-3 top-20 z-20 rounded-full border border-ink/25 bg-paper p-2 text-blue shadow-sm"
            onClick={() => setInspectorOpen(true)}
            aria-label={t("inspector.open")}
          >
            <IconChevronRight className="rotate-180" size={17} stroke={1.35} />
          </button>
        )}
      </div>

      {connectMode && (
        <div className="pointer-events-none fixed left-1/2 top-[62px] z-50 -translate-x-1/2 rounded-full border border-blue/35 bg-blue-soft px-4 py-2 font-serif text-[11px] text-blue shadow-sm">
          <IconPlugConnected className="mr-2 inline" size={15} stroke={1.35} />
          {edgeTypeLabel(connectType)} · {t("workspace.connectInstruction")}
        </div>
      )}

      {notice && !connectMode && (
        <div
          key={notice.id}
          className="workspace-toast"
          role="status"
          aria-live="polite"
        >
          {notice.text}
        </div>
      )}

      {menuOpen && (
        <ProjectMenu
          project={workspace.project}
          onClose={() => setMenuOpen(false)}
          onSettings={() => {
            setMenuOpen(false);
            setSettingsOpen(true);
          }}
          onPlugins={() => {
            setMenuOpen(false);
            setPluginStoreOpen(true);
          }}
          onSaveProject={() => void saveProject()}
          onImportProject={() => void importProject()}
          onFolderWorkspace={folderCommand ? () => void openFolder() : undefined}
          onGitWorkspace={availableGitCommand ? () => void openGit() : undefined}
          onReset={() => {
            workspace.resetDemo();
            setLayoutMode(null);
            setLinkFilter(null);
            setNotice(null);
            setHighlightChain(null);
            setPdfCompileResult(null);
            setPdfCompileError("");
            setMenuOpen(false);
          }}
        />
      )}
      {settingsOpen && (
        <SettingsDialog
          preferences={preferences}
          onClose={() => setSettingsOpen(false)}
          onSave={(nextPreferences) => {
            setPreferences(nextPreferences);
            window.localStorage.setItem(
              preferencesStorageKey,
              JSON.stringify(nextPreferences),
            );
            setSettingsOpen(false);
            showNotice(t("toast.settingsSaved"));
          }}
        />
      )}
      {pluginStoreOpen && (
        <PluginStoreDialog onClose={() => setPluginStoreOpen(false)} />
      )}
      {folderWorkspace && (
        <FolderWorkspaceDialog
          root={folderWorkspace.root}
          projects={folderWorkspace.projects}
          onClose={() => setFolderWorkspace(null)}
          onOpen={(path) => {
            void importProjectAtPath(path)
              .then((result) => {
                workspace.replaceProject(result.project, t("workspace.importProject"));
                setFolderWorkspace(null);
                setLayoutMode(null);
                setLinkFilter(null);
                showNotice(t("workspace.projectImported"));
              })
              .catch((error: unknown) => {
                showOperationError(error);
              });
          }}
        />
      )}
      {gitSnapshot && (
        <GitWorkspaceDialog
          snapshot={gitSnapshot}
          account={gitHubAccount}
          autoSave={gitAutoSave}
          busy={pluginBusy}
          patch={gitPatch}
          onClose={() => {
            setGitSnapshot(null);
            setGitCommand(null);
            setGitHubAccount(null);
            setGitAutoSave(false);
          }}
          onToggleAutoSave={setGitAutoSave}
          onInitialize={() => void initializeGit()}
          onRefreshAccount={() => void refreshGitHubAccount()}
          onLogin={() => void loginGitHub()}
          onGenerateSshKey={() => void generateGitHubKey()}
          onUploadSshKey={(path) => void uploadGitHubKey(path)}
          onSaveNow={() => void saveGitSnapshot()}
          onApplyPatch={() => {
            if (!gitPatch) return;
            workspace.applyGraphPatch(gitPatch);
            showNotice(t("workspace.patchApplied"));
          }}
        />
      )}
      {searchOpen && (
        <SearchPalette
          project={workspace.project}
          onClose={() => setSearchOpen(false)}
          onSelect={(nodeId) => {
            workspace.selectNode(nodeId);
            setInspectorOpen(true);
            setSearchOpen(false);
          }}
        />
      )}
      {pdfDialogOpen && (
        <PdfUploadDialog
          onClose={() => setPdfDialogOpen(false)}
          onReady={(jobId) => {
            setPdfDialogOpen(false);
            setReviewJobId(jobId);
          }}
        />
      )}
      {reviewJobId && (
        <AgentReviewPanel
          jobId={reviewJobId}
          compileResult={pdfCompileResult}
          compileError={pdfCompileError}
          onClose={() => setReviewJobId(null)}
          onApply={(patch) => void applyAgentPatch(patch)}
          onReject={rejectAgentPatch}
        />
      )}
      {composer && (
        <NodeComposer
          key={`${composer.type}-${composer.x}-${composer.y}`}
          state={composer}
          onClose={() => setComposer(null)}
          onCreate={(draft, x, y) => {
            workspace.createNode(draft, x, y);
            setComposer(null);
            setInspectorOpen(true);
            showNotice(t("toast.nodeAdded"));
          }}
        />
      )}
    </main>
  );
}
