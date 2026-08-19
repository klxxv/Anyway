import { ref } from "vue";
import { defineStore } from "pinia";

import {
  defaultWorkspacePreferences,
} from "../../../app/features/research-workspace/workspace-preferences";
import type {
  ComposerState,
  DiffFocus,
  DiffMode,
  PdfCompileResult,
  WorkspaceNotice,
  WorkspacePreferences,
} from "../components/workspace-shell-types";
import type { LayoutMode, ResearchEdgeType } from "../../../app/lib/research-types";
import type { LinkLegendFilter } from "../../../app/features/research-workspace/workspace-layout";

export type WorkspaceHighlightChain = {
  nodeIds: string[];
  edgeIds: string[];
};

/**
 * Vue-only workspace presentation state.
 *
 * Project data, history, selection entities, persistence, and platform/plugin
 * results stay in their existing domain/composable boundaries. This store is
 * limited to state that coordinates the workspace shell and its panels.
 */
export const useWorkspaceUiStore = defineStore("workspace-ui", () => {
  const menuOpen = ref(false);
  const settingsOpen = ref(false);
  const pluginStoreOpen = ref(false);
  const searchOpen = ref(false);
  const inspectorOpen = ref(true);
  const connectMode = ref(false);
  const connectType = ref<ResearchEdgeType>("T");
  const addRequest = ref(0);
  const composer = ref<ComposerState | null>(null);
  const layoutMode = ref<LayoutMode | null>(null);
  const linkFilter = ref<LinkLegendFilter | null>(null);
  const notice = ref<WorkspaceNotice | null>(null);
  const noticeSequence = ref(0);
  const preferences = ref<WorkspacePreferences>({ ...defaultWorkspacePreferences });
  const pdfDialogOpen = ref(false);
  const reviewJobId = ref<string | null>(null);
  const highlightChain = ref<WorkspaceHighlightChain | null>(null);
  const pdfCompileResult = ref<PdfCompileResult | null>(null);
  const pdfCompileError = ref("");
  const diffOpen = ref(false);
  const diffMode = ref<DiffMode>("side-by-side");
  const diffBaseId = ref("current");
  const diffCompareId = ref("current");
  const diffFocus = ref<DiffFocus | null>(null);
  const diffFocusSequence = ref(0);

  function toggleMenu() {
    menuOpen.value = !menuOpen.value;
  }

  function requestAdd() {
    addRequest.value += 1;
  }

  function openComposer(nextComposer: ComposerState) {
    composer.value = { ...nextComposer };
  }

  function closeComposer() {
    composer.value = null;
  }

  function setConnectMode(nextMode: boolean) {
    connectMode.value = nextMode;
  }

  function setConnectType(nextType: ResearchEdgeType) {
    connectType.value = nextType;
  }

  function showNotice(text: string) {
    noticeSequence.value += 1;
    notice.value = { id: noticeSequence.value, text };
  }

  function clearNotice() {
    notice.value = null;
  }

  function setPreferences(nextPreferences: WorkspacePreferences) {
    preferences.value = nextPreferences;
  }

  function openDiff(baseId: string, compareId = "current") {
    diffOpen.value = true;
    diffMode.value = "side-by-side";
    diffFocus.value = null;
    diffBaseId.value = baseId;
    diffCompareId.value = compareId;
  }

  function closeDiff() {
    diffOpen.value = false;
    diffFocus.value = null;
  }

  function focusDiff(kind: DiffFocus["kind"], id: string) {
    diffMode.value = "overlay";
    diffFocusSequence.value += 1;
    diffFocus.value = { kind, id, nonce: diffFocusSequence.value };
  }

  function closeTransientUi() {
    composer.value = null;
    searchOpen.value = false;
    menuOpen.value = false;
    settingsOpen.value = false;
    pluginStoreOpen.value = false;
    connectMode.value = false;
    pdfDialogOpen.value = false;
  }

  function resetForProject() {
    layoutMode.value = null;
    linkFilter.value = null;
    notice.value = null;
    highlightChain.value = null;
    pdfCompileResult.value = null;
    pdfCompileError.value = "";
    menuOpen.value = false;
  }

  return {
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
    noticeSequence,
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
    diffFocusSequence,
    toggleMenu,
    requestAdd,
    openComposer,
    closeComposer,
    setConnectMode,
    setConnectType,
    showNotice,
    clearNotice,
    setPreferences,
    openDiff,
    closeDiff,
    focusDiff,
    closeTransientUi,
    resetForProject,
  };
});

export type WorkspaceUiStore = ReturnType<typeof useWorkspaceUiStore>;
