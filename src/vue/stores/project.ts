import {
  computed,
  onBeforeUnmount,
  onMounted,
  ref,
  shallowRef,
  watch,
  type ComputedRef,
  type Ref,
  type ShallowRef,
} from "vue";
import { defineStore } from "pinia";
import type {
  LayoutMode,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNode,
} from "../../../app/lib/research-types";
import type { PluginGraphPatch } from "../../../app/plugins/contracts";
import type { LinkLegendFilter } from "../../../app/features/research-workspace/workspace-layout";
import { zenWorkspaceFixture } from "../../../app/features/research-workspace/workspace-fixture";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  NodeDraft,
  WorkspaceHistory,
} from "../../../app/features/research-workspace/workspace-types";
import {
  applyLayoutInDraft,
  cloneProject,
  createEdgeInDraft,
  createNodeInDraft,
  createSelectionClipboard,
  duplicateNodeInDraft,
  type GraphSelectionClipboard,
  type GraphSelectionResult,
  makeId,
  moveNodeInDraft,
  moveNodesInDraft,
  type NodeMove,
  pasteSelectionClipboardInDraft,
  pushHistoryEntry,
  redoHistory,
  removeEdgeInDraft,
  removeNodeInDraft,
  removeSelectionInDraft,
  reverseEdgeInDraft,
  stampDraftRevision,
  undoHistory,
  updateEdgeInDraft,
  updateNodeInDraft,
} from "../../../app/features/research-workspace/hooks/commit-logic";
import { applyGraphPatchToDraft } from "../../../app/features/research-workspace/hooks/patch-apply";
import {
  createLocalStorageProjectStorage,
  hydrateFromStorage,
  PROJECT_STORAGE_KEY,
  type ProjectStorage,
} from "../../../app/features/research-workspace/hooks/sync-logic";

export type ProjectStoreOptions = {
  fixture?: ProjectState;
  storage?: ProjectStorage;
  storageKey?: string;
};

/**
 * Pinia setup store for the workspace project domain.
 *
 * The domain mutation, history, patch, and persistence behavior stays in the
 * existing pure modules. This store owns only the Vue state lifecycle and
 * exposes the same operations that the former composable exposed.
 */
export const useProjectStore = defineStore("project", () => {
  const fixture = shallowRef<ProjectState>(zenWorkspaceFixture);
  const project = shallowRef<ProjectState>(cloneProject(fixture.value));
  const history = shallowRef<WorkspaceHistory[]>([]);
  const future = shallowRef<WorkspaceHistory[]>([]);
  const selectedNodeId = ref("variable-canopy");
  const selectedEdgeId = ref("");
  const selectionClipboard = shallowRef<GraphSelectionClipboard | null>(null);
  const hydrated = ref(false);
  const hasMutatedSinceHydration = ref(false);
  const configured = ref(false);

  let storageBackend: ProjectStorage = createLocalStorageProjectStorage(
    PROJECT_STORAGE_KEY,
  );
  let hydrationFrame: number | null = null;
  let clipboardPasteCount = 0;

  /**
   * Applies compatibility options before the component-mounted hydration
   * phase. A separate Pinia instance can be used by the facade for callers
   * that need an isolated fixture or storage backend.
   */
  const configure = (options: ProjectStoreOptions = {}): void => {
    if (configured.value) return;

    fixture.value = options.fixture ?? zenWorkspaceFixture;
    storageBackend =
      options.storage ??
      createLocalStorageProjectStorage(options.storageKey ?? PROJECT_STORAGE_KEY);
    project.value = cloneProject(fixture.value);
    configured.value = true;
  };

  const projectRef = project;

  onMounted(() => {
    hasMutatedSinceHydration.value = false;
    const hydrate = () => {
      const restored = hydrateFromStorage(storageBackend, fixture.value);
      if (restored && !hasMutatedSinceHydration.value) {
        project.value = restored;
      } else if (!restored) {
        // Missing or corrupt payloads are cleared; absent keys are a no-op.
        storageBackend.clear();
      }
      hydrated.value = true;
    };
    hydrationFrame = window.requestAnimationFrame(hydrate);
  });

  onBeforeUnmount(() => {
    if (hydrationFrame !== null) {
      window.cancelAnimationFrame(hydrationFrame);
      hydrationFrame = null;
    }
  });

  watch(project, (nextProject) => {
    if (!hydrated.value) return;
    storageBackend.save(nextProject);
  });

  const selectedNode = computed<ResearchNode | null>(
    () => project.value.nodes.find((node) => node.id === selectedNodeId.value) ?? null,
  );
  const selectedEdge = computed<ResearchEdge | null>(
    () => project.value.edges.find((edge) => edge.id === selectedEdgeId.value) ?? null,
  );

  const selectNode = (nodeId: string): void => {
    selectedNodeId.value = nodeId;
    selectedEdgeId.value = "";
  };

  const selectEdge = (edgeId: string): void => {
    selectedEdgeId.value = edgeId;
    selectedNodeId.value = "";
  };

  const clearSelection = (): void => {
    selectedNodeId.value = "";
    selectedEdgeId.value = "";
  };

  const commit = (label: string, transform: (draft: ProjectState) => void): void => {
    const current = projectRef.value;
    const before = cloneProject(current);
    const draft = cloneProject(current);
    transform(draft);
    stampDraftRevision(draft, new Date().toISOString());
    hasMutatedSinceHydration.value = true;
    history.value = pushHistoryEntry(history.value, { project: before, label });
    future.value = [];
    project.value = draft;
  };

  const updateNode = (nodeId: string, update: InspectorUpdate): void => {
    commit("Update node", (draft) =>
      updateNodeInDraft(draft, nodeId, update, new Date().toISOString()),
    );
  };

  const updateEdge = (edgeId: string, update: EdgeInspectorUpdate): void => {
    commit("Update relation", (draft) => updateEdgeInDraft(draft, edgeId, update));
  };

  const moveNode = (nodeId: string, x: number, y: number): void => {
    commit("Move node", (draft) => moveNodeInDraft(draft, nodeId, x, y));
  };

  const moveNodes = (moves: readonly NodeMove[]): void => {
    const validMoves = moves.filter(
      (move) =>
        move.nodeId && Number.isFinite(move.x) && Number.isFinite(move.y),
    );
    if (!validMoves.length) return;
    commit("Move selected nodes", (draft) => moveNodesInDraft(draft, validMoves));
  };

  const copySelection = (
    selectedNodeIds: readonly string[],
    selectedEdgeIds: readonly string[],
  ): GraphSelectionClipboard | null => {
    const clipboard = createSelectionClipboard(
      projectRef.value,
      selectedNodeIds,
      selectedEdgeIds,
    );
    selectionClipboard.value = clipboard;
    clipboardPasteCount = 0;
    return clipboard;
  };

  const pasteSelection = (): GraphSelectionResult => {
    const clipboard = selectionClipboard.value;
    if (!clipboard) return { nodeIds: [], edgeIds: [] };
    let result: GraphSelectionResult = { nodeIds: [], edgeIds: [] };
    const offset = 32 * (clipboardPasteCount + 1);
    commit("Paste selection", (draft) => {
      result = pasteSelectionClipboardInDraft(
        draft,
        clipboard,
        offset,
        new Date().toISOString(),
      );
    });
    clipboardPasteCount += 1;
    selectedNodeId.value = result.nodeIds[0] ?? "";
    selectedEdgeId.value = result.nodeIds.length ? "" : result.edgeIds[0] ?? "";
    return result;
  };

  const createNode = (draftNode: NodeDraft, x: number, y: number): string => {
    const id = makeId("node");
    commit("Create node", (draft) =>
      createNodeInDraft(draft, id, draftNode, x, y, new Date().toISOString()),
    );
    selectedNodeId.value = id;
    selectedEdgeId.value = "";
    return id;
  };

  const createEdge = (
    source: string,
    target: string,
    type: ResearchEdgeType = "T",
  ): string | undefined => {
    if (!source || !target || source === target) return undefined;
    if (
      projectRef.value.edges.some(
        (edge) => edge.source === source && edge.target === target,
      )
    ) {
      return undefined;
    }
    const edgeId = makeId("edge");
    commit("Create relation", (draft) =>
      createEdgeInDraft(draft, edgeId, source, target, type),
    );
    selectedEdgeId.value = edgeId;
    selectedNodeId.value = "";
    return edgeId;
  };

  const removeNode = (nodeId: string): void => {
    commit("Delete node", (draft) => removeNodeInDraft(draft, nodeId));
    selectedNodeId.value = "";
    selectedEdgeId.value = "";
  };

  const removeSelection = (
    selectedNodeIds: readonly string[],
    selectedEdgeIds: readonly string[],
  ): GraphSelectionResult => {
    const nodeIds = [...new Set(selectedNodeIds)].filter((nodeId) =>
      projectRef.value.nodes.some((node) => node.id === nodeId),
    );
    const edgeIds = [...new Set(selectedEdgeIds)].filter((edgeId) =>
      projectRef.value.edges.some((edge) => edge.id === edgeId),
    );
    if (!nodeIds.length && !edgeIds.length) return { nodeIds: [], edgeIds: [] };
    commit("Delete selection", (draft) =>
      removeSelectionInDraft(draft, nodeIds, edgeIds),
    );
    selectedNodeId.value = "";
    selectedEdgeId.value = "";
    return { nodeIds, edgeIds };
  };

  const duplicateNode = (nodeId: string): void => {
    const nextId = makeId("node");
    commit("Duplicate node", (draft) =>
      duplicateNodeInDraft(draft, nodeId, nextId, new Date().toISOString()),
    );
    selectedNodeId.value = nextId;
    selectedEdgeId.value = "";
  };

  const removeEdge = (edgeId: string): void => {
    commit("Delete relation", (draft) => removeEdgeInDraft(draft, edgeId));
    selectedEdgeId.value = "";
  };

  const reverseEdge = (edgeId: string): void => {
    commit("Reverse relation", (draft) => reverseEdgeInDraft(draft, edgeId));
  };

  const applyLayout = (
    mode: LayoutMode,
    filter: LinkLegendFilter | null = null,
  ): void => {
    commit(`Apply ${mode} layout`, (draft) =>
      applyLayoutInDraft(draft, mode, selectedNodeId.value, filter),
    );
  };

  const undo = (): void => {
    const transition = undoHistory(history.value, future.value, projectRef.value);
    if (!transition) return;
    history.value = transition.past;
    future.value = transition.future;
    project.value = transition.project;
  };

  const redo = (): void => {
    const transition = redoHistory(history.value, future.value, projectRef.value);
    if (!transition) return;
    history.value = transition.past;
    future.value = transition.future;
    project.value = transition.project;
  };

  const resetDemo = (): void => {
    project.value = cloneProject(fixture.value);
    selectedNodeId.value = "variable-canopy";
    selectedEdgeId.value = "";
    history.value = [];
    future.value = [];
  };

  const replaceProject = (
    nextProject: ProjectState,
    label = "Import project",
  ): void => {
    const current = projectRef.value;
    history.value = pushHistoryEntry(history.value, {
      project: cloneProject(current),
      label,
    });
    future.value = [];
    project.value = cloneProject(nextProject);
    selectedNodeId.value = nextProject.nodes[0]?.id ?? "";
    selectedEdgeId.value = "";
  };

  const applyGraphPatch = (patch: PluginGraphPatch): void => {
    if (patch.reviewRequired !== true) {
      throw new Error("GraphPatch must be review-gated (reviewRequired=true)");
    }
    const targetProjectId = patch.source.projectId;
    if (targetProjectId && targetProjectId !== projectRef.value.id) {
      throw new Error(
        `GraphPatch target project mismatch: expected ${projectRef.value.id}, got ${targetProjectId}`,
      );
    }
    commit(`Apply plugin patch: ${patch.title}`, (draft) => {
      applyGraphPatchToDraft(draft, patch, new Date().toISOString());
    });
  };

  return {
    fixture,
    project,
    projectRef,
    history,
    selectedNode,
    selectedNodeId,
    selectedEdge,
    selectedEdgeId,
    canUndo: computed(() => history.value.length > 0),
    canRedo: computed(() => future.value.length > 0),
    configure,
    selectNode,
    selectEdge,
    clearSelection,
    updateNode,
    updateEdge,
    moveNode,
    moveNodes,
    copySelection,
    pasteSelection,
    createNode,
    createEdge,
    removeNode,
    removeSelection,
    duplicateNode,
    removeEdge,
    reverseEdge,
    applyLayout,
    undo,
    redo,
    resetDemo,
    replaceProject,
    applyGraphPatch,
  };
});

export type ProjectStore = ReturnType<typeof useProjectStore>;

export type ProjectStoreRefs = {
  project: ShallowRef<ProjectState>;
  projectRef: ShallowRef<ProjectState>;
  history: ShallowRef<WorkspaceHistory[]>;
  selectedNode: ComputedRef<ResearchNode | null>;
  selectedNodeId: Ref<string>;
  selectedEdge: ComputedRef<ResearchEdge | null>;
  selectedEdgeId: Ref<string>;
};
