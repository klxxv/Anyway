"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ProjectState, ResearchEdgeType, ResearchNodeType } from "../../../lib/research-types";
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
          setProject(JSON.parse(saved) as ProjectState);
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
          tags: [],
          status: "draft",
          evidenceIds: [],
          data:
            draftNode.type === "variable"
              ? { valueType: "enum", enumValues: ["low", "medium", "high"] }
              : {},
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

  const autoLayout = useCallback(() => {
    commit("Auto layout", (draft) => {
      draft.placements.forEach((placement, index) => {
        const column = index % 4;
        const row = Math.floor(index / 4);
        placement.x = 80 + column * 235;
        placement.y = 90 + row * 190 + (column % 2) * 34;
      });
    });
  }, [commit]);

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
    autoLayout,
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
