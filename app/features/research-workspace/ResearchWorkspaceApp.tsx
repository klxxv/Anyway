"use client";

import { IconChevronRight, IconPlugConnected } from "@tabler/icons-react";
import { useCallback, useEffect, useState } from "react";
import type {
  LayoutMode,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../lib/research-types";
import { useI18n } from "../../i18n/provider";
import { ResearchGraphCanvas } from "./canvas/research-graph-canvas";
import { InspectorPanel } from "./components/inspector-panel";
import { PluginStoreDialog } from "./components/plugin-store-dialog";
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

const preferencesStorageKey = "research-canvas.workspace-preferences.v1";

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
  const [notice, setNotice] = useState("");
  const [preferences, setPreferences] = useState<WorkspacePreferences>(
    defaultWorkspacePreferences,
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
      setNotice(
        next ? `${connectType.replaceAll("_", " ")} · drag from one node to another` : "",
      );
      return next;
    });
  }, [connectType]);

  const addNote = useCallback(() => {
    setComposer({ type: "note", x: 610, y: 430 });
  }, []);

  const applyDefaultLayout = useCallback(() => {
    const mode = preferences.defaultLayout;
    workspace.applyLayout(mode, linkFilter);
    setLayoutMode(mode);
    setNotice(`${mode.replaceAll("-", " ")} layout applied`);
  }, [linkFilter, preferences.defaultLayout, workspace]);

  const exportProject = useCallback(() => downloadProject(workspace.project), [workspace.project]);

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
    };
    window.addEventListener("keydown", closeTransientUi);
    return () => window.removeEventListener("keydown", closeTransientUi);
  }, []);

  return (
    <main className="flex h-screen min-h-[680px] w-screen flex-col overflow-hidden bg-paper text-ink">
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
          setNotice(`${type.replaceAll("_", " ")} · drag from one node to another`);
        }}
        onNote={addNote}
        onFind={() => setSearchOpen(true)}
        activeLayout={layoutMode}
        onLayout={(mode) => {
          workspace.applyLayout(mode, linkFilter);
          setLayoutMode(mode);
          setNotice(`${mode.replaceAll("-", " ")} layout applied`);
        }}
        onUndo={workspace.undo}
        onRedo={workspace.redo}
        onExport={exportProject}
      />

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_320px]">
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
            addRequest={addRequest}
            connectMode={connectMode}
            linkFilter={linkFilter}
            showMiniMap={preferences.showMiniMap}
            showLinkCounts={preferences.showLinkCounts}
            referenceViewport={layoutMode === null && linkFilter === null}
            onLegendFilter={(nextFilter) => {
              const mode = layoutMode ?? preferences.defaultLayout;
              setLinkFilter(nextFilter);
              setLayoutMode(mode);
              workspace.applyLayout(mode, nextFilter);
              setNotice(
                nextFilter
                  ? `${nextFilter} links filtered and re-laid out`
                  : `All links restored in ${mode.replaceAll("-", " ")} layout`,
              );
            }}
            onSelectNode={(nodeId) => {
              workspace.setSelectedNodeId(nodeId);
              setInspectorOpen(true);
            }}
            onMoveNode={workspace.moveNode}
            onCreateEdge={(source, target) => {
              workspace.createEdge(source, target, connectType);
              setConnectMode(false);
              setNotice(`${connectType.replaceAll("_", " ")} relation created`);
            }}
            onRequestCreate={requestCreate}
          />
        </section>

        {inspectorOpen ? (
          <InspectorPanel
            node={workspace.selectedNode}
            onUpdate={workspace.updateNode}
            onDelete={workspace.removeNode}
            onClose={() => setInspectorOpen(false)}
          />
        ) : (
          <button
            className="absolute right-3 top-20 z-20 rounded-full border border-ink/25 bg-paper p-2 text-blue shadow-sm"
            onClick={() => setInspectorOpen(true)}
            aria-label="Open inspector"
          >
            <IconChevronRight className="rotate-180" size={17} stroke={1.35} />
          </button>
        )}
      </div>

      {connectMode && (
        <div className="pointer-events-none fixed left-1/2 top-[62px] z-50 -translate-x-1/2 rounded-full border border-blue/35 bg-blue-soft px-4 py-2 font-serif text-[11px] text-blue shadow-sm">
          <IconPlugConnected className="mr-2 inline" size={15} stroke={1.35} />
          Connect mode · drag between node handles
        </div>
      )}

      {notice && !connectMode && (
        <div className="fixed bottom-5 left-1/2 z-50 -translate-x-1/2 rounded-full border border-ink/20 bg-paper px-4 py-2 font-serif text-[11px] shadow-sm">
          {notice}
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
          onReset={() => {
            workspace.resetDemo();
            setLayoutMode(null);
            setLinkFilter(null);
            setNotice("");
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
            setNotice("Settings saved");
          }}
        />
      )}
      {pluginStoreOpen && (
        <PluginStoreDialog onClose={() => setPluginStoreOpen(false)} />
      )}
      {searchOpen && (
        <SearchPalette
          project={workspace.project}
          onClose={() => setSearchOpen(false)}
          onSelect={(nodeId) => {
            workspace.setSelectedNodeId(nodeId);
            setInspectorOpen(true);
            setSearchOpen(false);
          }}
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
            setNotice("Node added");
          }}
        />
      )}
    </main>
  );
}
