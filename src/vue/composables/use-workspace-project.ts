import { createPinia, getActivePinia, storeToRefs } from "pinia";
import type {
  ComputedRef,
  Ref,
  ShallowRef,
} from "vue";
import type {
  LayoutMode,
  ProjectState,
  ResearchEdge,
  ResearchEdgeType,
  ResearchNode,
} from "../../../app/lib/research-types";
import type { PluginGraphPatch } from "../../../app/plugins/contracts";
import type { LinkLegendFilter } from "../../../app/features/research-workspace/workspace-layout";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  NodeDraft,
  WorkspaceHistory,
} from "../../../app/features/research-workspace/workspace-types";
import type {
  GraphSelectionClipboard,
  GraphSelectionResult,
  NodeMove,
} from "../../../app/features/research-workspace/hooks/commit-logic";
import {
  useProjectStore,
  type ProjectStoreOptions,
} from "../stores/project";

export type WorkspaceProjectOptions = ProjectStoreOptions;

export type WorkspaceProjectResult = {
  project: ShallowRef<ProjectState>;
  /** Latest project snapshot used by imperative workspace handlers. */
  projectRef: ShallowRef<ProjectState>;
  history: ShallowRef<WorkspaceHistory[]>;
  selectedNode: ComputedRef<ResearchNode | null>;
  selectedNodeId: Ref<string>;
  selectedEdge: ComputedRef<ResearchEdge | null>;
  selectedEdgeId: Ref<string>;
  canUndo: ComputedRef<boolean>;
  canRedo: ComputedRef<boolean>;
  selectNode: (nodeId: string) => void;
  selectEdge: (edgeId: string) => void;
  clearSelection: () => void;
  updateNode: (nodeId: string, update: InspectorUpdate) => void;
  updateEdge: (edgeId: string, update: EdgeInspectorUpdate) => void;
  moveNode: (nodeId: string, x: number, y: number) => void;
  moveNodes: (moves: readonly NodeMove[]) => void;
  copySelection: (
    selectedNodeIds: readonly string[],
    selectedEdgeIds: readonly string[],
  ) => GraphSelectionClipboard | null;
  pasteSelection: () => GraphSelectionResult;
  createNode: (draftNode: NodeDraft, x: number, y: number) => string;
  createEdge: (
    source: string,
    target: string,
    type?: ResearchEdgeType,
  ) => string | undefined;
  removeNode: (nodeId: string) => void;
  removeSelection: (
    selectedNodeIds: readonly string[],
    selectedEdgeIds: readonly string[],
  ) => GraphSelectionResult;
  duplicateNode: (nodeId: string) => void;
  removeEdge: (edgeId: string) => void;
  reverseEdge: (edgeId: string) => void;
  applyLayout: (mode: LayoutMode, filter?: LinkLegendFilter | null) => void;
  undo: () => void;
  redo: () => void;
  resetDemo: () => void;
  replaceProject: (nextProject: ProjectState, label?: string) => void;
  applyGraphPatch: (patch: PluginGraphPatch) => void;
};

function requiresIsolatedPinia(options: WorkspaceProjectOptions): boolean {
  return (
    options.fixture !== undefined ||
    options.storage !== undefined ||
    options.storageKey !== undefined
  );
}

/**
 * Compatibility facade for existing Vue callers.
 *
 * Pinia unwraps setup-store refs on the store proxy, so `storeToRefs` is used
 * here deliberately to preserve the composable's historical Ref/ShallowRef
 * return shapes and keep every caller interface unchanged.
 */
export function useWorkspaceProject(
  options: WorkspaceProjectOptions = {},
): WorkspaceProjectResult {
  const pinia = requiresIsolatedPinia(options)
    ? createPinia()
    : (getActivePinia() ?? createPinia());
  const store = useProjectStore(pinia);
  store.configure(options);
  const refs = storeToRefs(store);

  return {
    project: refs.project,
    projectRef: refs.project,
    history: refs.history,
    selectedNode: refs.selectedNode,
    selectedNodeId: refs.selectedNodeId,
    selectedEdge: refs.selectedEdge,
    selectedEdgeId: refs.selectedEdgeId,
    canUndo: refs.canUndo,
    canRedo: refs.canRedo,
    selectNode: store.selectNode,
    selectEdge: store.selectEdge,
    clearSelection: store.clearSelection,
    updateNode: store.updateNode,
    updateEdge: store.updateEdge,
    moveNode: store.moveNode,
    moveNodes: store.moveNodes,
    copySelection: store.copySelection,
    pasteSelection: store.pasteSelection,
    createNode: store.createNode,
    createEdge: store.createEdge,
    removeNode: store.removeNode,
    removeSelection: store.removeSelection,
    duplicateNode: store.duplicateNode,
    removeEdge: store.removeEdge,
    reverseEdge: store.reverseEdge,
    applyLayout: store.applyLayout,
    undo: store.undo,
    redo: store.redo,
    resetDemo: store.resetDemo,
    replaceProject: store.replaceProject,
    applyGraphPatch: store.applyGraphPatch,
  };
}
