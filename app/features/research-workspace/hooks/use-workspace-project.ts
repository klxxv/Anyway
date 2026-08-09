"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { computeLayout } from "../../../lib/layout";
import type {
  LayoutMode,
  ProjectState,
  ResearchEdgeType,
} from "../../../lib/research-types";
import type { PluginGraphPatch } from "../../../plugins/contracts";
import type { LinkLegendFilter } from "../workspace-layout";
import { zenWorkspaceFixture } from "../workspace-fixture";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  NodeDraft,
  WorkspaceHistory,
} from "../workspace-types";
import {
  applyLayoutInDraft,
  cloneProject,
  createEdgeInDraft,
  createNodeInDraft,
  duplicateNodeInDraft,
  makeId,
  moveNodeInDraft,
  pushHistoryEntry,
  redoHistory,
  removeEdgeInDraft,
  removeNodeInDraft,
  reverseEdgeInDraft,
  stampDraftRevision,
  undoHistory,
  updateEdgeInDraft,
  updateNodeInDraft,
} from "./commit-logic";
import { applyGraphPatchToDraft } from "./patch-apply";
import {
  createLocalStorageProjectStorage,
  hydrateFromStorage,
  PROJECT_STORAGE_KEY,
  type ProjectStorage,
} from "./sync-logic";

/**
 * 顶层 hook 的注入选项；默认值保持现有行为不变。
 * Injection points for the top-level hook; defaults preserve current behavior.
 */
export type WorkspaceProjectOptions = {
  /** 内置示例数据，通过注入解耦 / Bundled example; injected to decouple the import. */
  fixture?: ProjectState;
  /** 自定义存储后端；缺省使用 localStorage / Custom storage backend, defaults to localStorage. */
  storage?: ProjectStorage;
  /** localStorage 键 / Storage key used by the default backend. */
  storageKey?: string;
};

/**
 * Owns persisted research state and undo/redo while visual components stay stateless.
 * 统一管理持久化研究状态与撤销重做，让视觉组件保持无状态。
 * 本 hook 仅做编排：提交/撤销/补丁/同步逻辑分别位于 commit-logic / patch-apply / sync-logic。
 */
export function useWorkspaceProject(options: WorkspaceProjectOptions = {}) {
  const {
    fixture = zenWorkspaceFixture,
    storage: injectedStorage,
    storageKey = PROJECT_STORAGE_KEY,
  } = options;
  const fixtureRef = useRef(fixture);
  const storageBackend = useMemo(
    () => injectedStorage ?? createLocalStorageProjectStorage(storageKey),
    [injectedStorage, storageKey],
  );

  const [project, setProject] = useState<ProjectState>(() => cloneProject(fixture));
  const [selectedNodeId, setSelectedNodeId] = useState("variable-canopy");
  const [selectedEdgeId, setSelectedEdgeId] = useState("");
  const [past, setPast] = useState<WorkspaceHistory[]>([]);
  const [future, setFuture] = useState<WorkspaceHistory[]>([]);
  const hydrated = useRef(false);
  /** 标记自本次 hydration 启动以来用户是否已编辑项目，防止陈旧持久化数据覆盖新状态。 */
  const hasMutatedSinceHydration = useRef(false);

  // Latest-state refs keep callbacks stable while reading current snapshots.
  // 最新状态引用让回调保持稳定，同时读取当前快照。
  const projectRef = useRef(project);
  const pastRef = useRef(past);
  const futureRef = useRef(future);
  useEffect(() => {
    projectRef.current = project;
  });
  useEffect(() => {
    pastRef.current = past;
  });
  useEffect(() => {
    futureRef.current = future;
  });

  useEffect(() => {
    hasMutatedSinceHydration.current = false;
    const frame = window.requestAnimationFrame(() => {
      const restored = hydrateFromStorage(storageBackend, fixtureRef.current);
      if (restored && !hasMutatedSinceHydration.current) {
        setProject(restored);
      } else if (!restored) {
        // 无数据或数据损坏时清理；原实现仅在解析失败时移除该键。
        // Clears missing or corrupt payloads; removeItem of an absent key is a no-op.
        storageBackend.clear();
      }
      hydrated.current = true;
    });
    return () => window.cancelAnimationFrame(frame);
  }, [storageBackend]);

  useEffect(() => {
    if (!hydrated.current) return;
    storageBackend.save(project);
  }, [project, storageBackend]);

  const selectedNode = useMemo(
    () => project.nodes.find((node) => node.id === selectedNodeId) ?? null,
    [project.nodes, selectedNodeId],
  );
  const selectedEdge = useMemo(
    () => project.edges.find((edge) => edge.id === selectedEdgeId) ?? null,
    [project.edges, selectedEdgeId],
  );

  const selectNode = useCallback((nodeId: string) => {
    setSelectedNodeId(nodeId);
    setSelectedEdgeId("");
  }, []);

  const selectEdge = useCallback((edgeId: string) => {
    setSelectedEdgeId(edgeId);
    setSelectedNodeId("");
  }, []);

  /** 统一提交管道：克隆 → 变换 → 盖章 → 入历史栈，任何变更都必须走这里。 */
  const commit = useCallback((label: string, transform: (draft: ProjectState) => void) => {
    const current = projectRef.current;
    const before = cloneProject(current);
    const draft = cloneProject(current);
    transform(draft);
    stampDraftRevision(draft, new Date().toISOString());
    hasMutatedSinceHydration.current = true;
    setPast(pushHistoryEntry(pastRef.current, { project: before, label }));
    setFuture([]);
    setProject(draft);
  }, []);

  const updateNode = useCallback(
    (nodeId: string, update: InspectorUpdate) => {
      commit("Update node", (draft) =>
        updateNodeInDraft(draft, nodeId, update, new Date().toISOString()),
      );
    },
    [commit],
  );

  const updateEdge = useCallback(
    (edgeId: string, update: EdgeInspectorUpdate) => {
      commit("Update relation", (draft) => updateEdgeInDraft(draft, edgeId, update));
    },
    [commit],
  );

  const moveNode = useCallback(
    (nodeId: string, x: number, y: number) => {
      commit("Move node", (draft) => moveNodeInDraft(draft, nodeId, x, y));
    },
    [commit],
  );

  const createNode = useCallback(
    (draftNode: NodeDraft, x: number, y: number) => {
      const id = makeId("node");
      commit("Create node", (draft) =>
        createNodeInDraft(draft, id, draftNode, x, y, new Date().toISOString()),
      );
      setSelectedNodeId(id);
      setSelectedEdgeId("");
      return id;
    },
    [commit],
  );

  const createEdge = useCallback(
    (source: string, target: string, type: ResearchEdgeType = "causes") => {
      if (!source || !target || source === target) return;
      if (
        projectRef.current.edges.some(
          (edge) => edge.source === source && edge.target === target,
        )
      ) {
        return;
      }
      const edgeId = makeId("edge");
      commit("Create relation", (draft) => createEdgeInDraft(draft, edgeId, source, target, type));
      setSelectedEdgeId(edgeId);
      setSelectedNodeId("");
      return edgeId;
    },
    [commit],
  );

  const removeNode = useCallback(
    (nodeId: string) => {
      commit("Delete node", (draft) => removeNodeInDraft(draft, nodeId));
      setSelectedNodeId("");
      setSelectedEdgeId("");
    },
    [commit],
  );

  const duplicateNode = useCallback(
    (nodeId: string) => {
      const nextId = makeId("node");
      commit("Duplicate node", (draft) =>
        duplicateNodeInDraft(draft, nodeId, nextId, new Date().toISOString()),
      );
      setSelectedNodeId(nextId);
      setSelectedEdgeId("");
    },
    [commit],
  );

  const removeEdge = useCallback(
    (edgeId: string) => {
      commit("Delete relation", (draft) => removeEdgeInDraft(draft, edgeId));
      setSelectedEdgeId("");
    },
    [commit],
  );

  const reverseEdge = useCallback(
    (edgeId: string) => {
      commit("Reverse relation", (draft) => reverseEdgeInDraft(draft, edgeId));
    },
    [commit],
  );

  const applyLayout = useCallback(
    (mode: LayoutMode, filter: LinkLegendFilter | null = null) => {
      commit(`Apply ${mode} layout`, (draft) =>
        applyLayoutInDraft(draft, mode, selectedNodeId, filter),
      );
    },
    [commit, selectedNodeId],
  );

  const undo = useCallback(() => {
    const transition = undoHistory(pastRef.current, futureRef.current, projectRef.current);
    if (!transition) return;
    setPast(transition.past);
    setFuture(transition.future);
    setProject(transition.project);
  }, []);

  const redo = useCallback(() => {
    const transition = redoHistory(pastRef.current, futureRef.current, projectRef.current);
    if (!transition) return;
    setPast(transition.past);
    setFuture(transition.future);
    setProject(transition.project);
  }, []);

  const resetDemo = useCallback(() => {
    setProject(cloneProject(fixtureRef.current));
    // 保持与内置示例一致的初始选中节点 / Keeps the bundled example's initial selection.
    setSelectedNodeId("variable-canopy");
    setSelectedEdgeId("");
    setPast([]);
    setFuture([]);
  }, []);

  /** Replaces the aggregate after native import while preserving one undo checkpoint. */
  const replaceProject = useCallback((nextProject: ProjectState, label = "Import project") => {
    const current = projectRef.current;
    setPast(pushHistoryEntry(pastRef.current, { project: cloneProject(current), label }));
    setFuture([]);
    setProject(cloneProject(nextProject));
    setSelectedNodeId(nextProject.nodes[0]?.id ?? "");
    setSelectedEdgeId("");
  }, []);

  /** 插件 GraphPatch 必须经由 commit 管道应用，不绕过撤销栈。 */
  const applyGraphPatch = useCallback(
    (patch: PluginGraphPatch) => {
      if (patch.reviewRequired !== true) {
        throw new Error("GraphPatch must be review-gated (reviewRequired=true)");
      }
      const targetProjectId = patch.source.projectId;
      if (targetProjectId && targetProjectId !== projectRef.current.id) {
        throw new Error(
          `GraphPatch target project mismatch: expected ${projectRef.current.id}, got ${targetProjectId}`,
        );
      }
      commit(`Apply plugin patch: ${patch.title}`, (draft) => {
        applyGraphPatchToDraft(draft, patch, new Date().toISOString());
      });
    },
    [commit],
  );

  return {
    project,
    /** 指向当前 project 的 ref；用于在同步变更后读取最新状态（避免闭包过期）。 */
    projectRef,
    /** 撤销历史快照（past，按时间旧→新）；供 Canvas Diff 版本选择。 */
    history: past,
    selectedNode,
    selectedNodeId,
    selectedEdge,
    selectedEdgeId,
    canUndo: past.length > 0,
    canRedo: future.length > 0,
    selectNode,
    selectEdge,
    updateNode,
    updateEdge,
    moveNode,
    createNode,
    createEdge,
    removeNode,
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
}
