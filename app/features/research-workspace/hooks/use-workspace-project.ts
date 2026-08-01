"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { computeLayout } from "../../../lib/research-core";
import type {
  LayoutMode,
  ProjectState,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../../lib/research-types";
import {
  projectForLegendFilter,
  type LinkLegendFilter,
} from "../workspace-layout";
import type { InspectorUpdate, NodeDraft, WorkspaceHistory } from "../workspace-types";
import { zenWorkspaceFixture } from "../workspace-fixture";

const storageKey = "research-canvas.zen-workspace.v1";

function cloneProject(project: ProjectState): ProjectState {
  return JSON.parse(JSON.stringify(project)) as ProjectState;
}

function makeId(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

/**
 * Owns persisted research state and undo/redo while visual components stay stateless.
 * 统一管理持久化研究状态与撤销重做，让视觉组件保持无状态。
 */
export function useWorkspaceProject() {
  const [project, setProject] = useState<ProjectState>(() => cloneProject(zenWorkspaceFixture));
  const [selectedNodeId, setSelectedNodeId] = useState("variable-canopy");
  const [past, setPast] = useState<WorkspaceHistory[]>([]);
  const [future, setFuture] = useState<WorkspaceHistory[]>([]);
  const hydrated = useRef(false);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const saved = window.localStorage.getItem(storageKey);
      if (saved) {
        try {
          const parsed = JSON.parse(saved) as ProjectState;
          // 内置示例使用 schemaVersion 迁移，避免覆盖用户创建的其他项目。
          // Migrate only the bundled example; unrelated user projects remain untouched.
          setProject(
            parsed.id === zenWorkspaceFixture.id && parsed.schemaVersion < 2
              ? cloneProject(zenWorkspaceFixture)
              : parsed,
          );
        } catch {
          window.localStorage.removeItem(storageKey);
        }
      }
      hydrated.current = true;
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    if (!hydrated.current) return;
    window.localStorage.setItem(storageKey, JSON.stringify(project));
  }, [project]);

  const selectedNode = useMemo(
    () => project.nodes.find((node) => node.id === selectedNodeId) ?? null,
    [project.nodes, selectedNodeId],
  );

  const commit = useCallback((label: string, transform: (draft: ProjectState) => void) => {
    setProject((current) => {
      const before = cloneProject(current);
      const draft = cloneProject(current);
      transform(draft);
      draft.revision += 1;
      draft.updatedAt = new Date().toISOString();
      setPast((entries) => [...entries.slice(-39), { project: before, label }]);
      setFuture([]);
      return draft;
    });
  }, []);

  const updateNode = useCallback(
    (nodeId: string, update: InspectorUpdate) => {
      commit("Update node", (draft) => {
        const node = draft.nodes.find((item) => item.id === nodeId);
        if (!node) return;
        Object.assign(node, update, { updatedAt: new Date().toISOString() });
      });
    },
    [commit],
  );

  const moveNode = useCallback(
    (nodeId: string, x: number, y: number) => {
      commit("Move node", (draft) => {
        const placement = draft.placements.find((item) => item.nodeId === nodeId);
        if (!placement) return;
        placement.x = x;
        placement.y = y;
      });
    },
    [commit],
  );

  const createNode = useCallback(
    (draftNode: NodeDraft, x: number, y: number) => {
      const id = makeId("node");
      commit("Create node", (draft) => {
        const now = new Date().toISOString();
        draft.nodes.push({
          id,
          type: draftNode.type,
          title: draftNode.title.trim() || `Untitled ${draftNode.type}`,
          body: draftNode.body.trim() || "Add a concise research note.",
          tags: draftNode.tags,
          status: "draft",
          evidenceIds: [],
          data: draftNode.data,
          provenance: { origin: "human", actorId: "local-researcher" },
          createdAt: now,
          updatedAt: now,
        });
        draft.placements.push({
          id: `placement-${id}`,
          viewId: "view-main",
          nodeId: id,
          x,
          y,
          width: draftNode.type === "question" ? 136 : 164,
          height: draftNode.type === "question" ? 136 : 116,
        });
      });
      setSelectedNodeId(id);
      return id;
    },
    [commit],
  );

  const createEdge = useCallback(
    (source: string, target: string, type: ResearchEdgeType = "causes") => {
      if (!source || !target || source === target) return;
      commit("Create relation", (draft) => {
        if (draft.edges.some((edge) => edge.source === source && edge.target === target)) return;
        draft.edges.push({
          id: makeId("edge"),
          source,
          target,
          type,
          directed: true,
          polarity: type === "contradicts" ? "negative" : "positive",
          confidence: 1,
          conditions: [],
          evidenceIds: [],
          note: type.replaceAll("_", " "),
          provenance: { origin: "human", actorId: "local-researcher" },
        });
      });
    },
    [commit],
  );

  const removeNode = useCallback(
    (nodeId: string) => {
      commit("Delete node", (draft) => {
        draft.nodes = draft.nodes.filter((node) => node.id !== nodeId);
        draft.edges = draft.edges.filter(
          (edge) => edge.source !== nodeId && edge.target !== nodeId,
        );
        draft.placements = draft.placements.filter((placement) => placement.nodeId !== nodeId);
      });
      setSelectedNodeId("");
    },
    [commit],
  );

  const duplicateNode = useCallback(
    (nodeId: string) => {
      const nextId = makeId("node");
      commit("Duplicate node", (draft) => {
        const source = draft.nodes.find((node) => node.id === nodeId);
        const placement = draft.placements.find((item) => item.nodeId === nodeId);
        if (!source || !placement) return;
        const now = new Date().toISOString();
        draft.nodes.push({
          ...(JSON.parse(JSON.stringify(source)) as typeof source),
          id: nextId,
          title: `${source.title} copy`,
          provenance: { origin: "human", actorId: "local-researcher" },
          createdAt: now,
          updatedAt: now,
        });
        draft.placements.push({
          ...placement,
          id: `placement-${nextId}`,
          nodeId: nextId,
          x: placement.x + 28,
          y: placement.y + 28,
        });
      });
      setSelectedNodeId(nextId);
    },
    [commit],
  );

  const removeEdge = useCallback(
    (edgeId: string) => {
      commit("Delete relation", (draft) => {
        draft.edges = draft.edges.filter((edge) => edge.id !== edgeId);
      });
    },
    [commit],
  );

  const reverseEdge = useCallback(
    (edgeId: string) => {
      commit("Reverse relation", (draft) => {
        const edge = draft.edges.find((item) => item.id === edgeId);
        if (!edge) return;
        [edge.source, edge.target] = [edge.target, edge.source];
      });
    },
    [commit],
  );

  const applyLayout = useCallback(
    (mode: LayoutMode, filter: LinkLegendFilter | null = null) => {
      // Layout only the visible relation projection, then persist positions in one history step.
      // 仅对当前可见关系投影布局，并把坐标作为一次历史操作统一提交。
      commit(`Apply ${mode} layout`, (draft) => {
        const projected = projectForLegendFilter(draft, filter);
        const rootId = projected.nodes.some((node) => node.id === selectedNodeId)
          ? selectedNodeId
          : projected.nodes[0]?.id;
        const result = computeLayout(projected, mode, rootId);
        const positioned = new Set(Object.keys(result.positions));
        const fallbackNodes = projected.nodes.filter((node) => !positioned.has(node.id));
        const maxY = Math.max(
          80,
          ...Object.values(result.positions).map((position) => position.y),
        );
        fallbackNodes.forEach((node, index) => {
          result.positions[node.id] = {
            x: 80 + (index % 4) * 235,
            y: maxY + 210 + Math.floor(index / 4) * 170,
          };
        });
        draft.placements.forEach((placement) => {
          const position = result.positions[placement.nodeId];
          if (!position) return;
          placement.x = position.x;
          placement.y = position.y;
        });
      });
    },
    [commit, selectedNodeId],
  );

  const undo = useCallback(() => {
    setPast((entries) => {
      const previous = entries.at(-1);
      if (!previous) return entries;
      setProject((current) => {
        setFuture((futureEntries) => [
          { project: cloneProject(current), label: previous.label },
          ...futureEntries,
        ]);
        return cloneProject(previous.project);
      });
      return entries.slice(0, -1);
    });
  }, []);

  const redo = useCallback(() => {
    setFuture((entries) => {
      const next = entries[0];
      if (!next) return entries;
      setProject((current) => {
        setPast((pastEntries) => [
          ...pastEntries,
          { project: cloneProject(current), label: next.label },
        ]);
        return cloneProject(next.project);
      });
      return entries.slice(1);
    });
  }, []);

  const resetDemo = useCallback(() => {
    setProject(cloneProject(zenWorkspaceFixture));
    setSelectedNodeId("variable-canopy");
    setPast([]);
    setFuture([]);
  }, []);

  return {
    project,
    selectedNode,
    selectedNodeId,
    canUndo: past.length > 0,
    canRedo: future.length > 0,
    setSelectedNodeId,
    updateNode,
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
  };
}

export const quickNodeTypes: Array<{ type: ResearchNodeType; label: string }> = [
  { type: "question", label: "Question" },
  { type: "concept", label: "Group" },
  { type: "variable", label: "Variable" },
  { type: "method", label: "Method" },
  { type: "dataset", label: "Data" },
  { type: "evidence", label: "Evidence" },
  { type: "result", label: "Result" },
  { type: "note", label: "Note" },
];
