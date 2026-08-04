"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { computeLayout } from "../../../lib/layout";
import type {
  LayoutMode,
  ProjectState,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../../lib/research-types";
import { EDGE_TYPES, NODE_TYPES } from "../../../lib/research-types";
import type { PluginGraphPatch } from "../../../plugins/contracts";
import {
  projectForLegendFilter,
  type LinkLegendFilter,
} from "../workspace-layout";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  NodeDraft,
  WorkspaceHistory,
} from "../workspace-types";
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
  const [selectedEdgeId, setSelectedEdgeId] = useState("");
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

  const updateEdge = useCallback(
    (edgeId: string, update: EdgeInspectorUpdate) => {
      commit("Update relation", (draft) => {
        const edge = draft.edges.find((item) => item.id === edgeId);
        if (!edge) return;
        Object.assign(edge, update);
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
      setSelectedEdgeId("");
      return id;
    },
    [commit],
  );

  const createEdge = useCallback(
    (source: string, target: string, type: ResearchEdgeType = "causes") => {
      if (!source || !target || source === target) return;
      if (project.edges.some((edge) => edge.source === source && edge.target === target)) return;
      const edgeId = makeId("edge");
      commit("Create relation", (draft) => {
        draft.edges.push({
          id: edgeId,
          source,
          target,
          type,
          directed: true,
          polarity: type === "contradicts" ? "negative" : "positive",
          confidence: 1,
          conditions: [],
          evidenceIds: [],
          provenance: { origin: "human", actorId: "local-researcher" },
        });
      });
      setSelectedEdgeId(edgeId);
      setSelectedNodeId("");
      return edgeId;
    },
    [commit, project.edges],
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
      setSelectedEdgeId("");
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
      setSelectedEdgeId("");
    },
    [commit],
  );

  const removeEdge = useCallback(
    (edgeId: string) => {
      commit("Delete relation", (draft) => {
        draft.edges = draft.edges.filter((edge) => edge.id !== edgeId);
      });
      setSelectedEdgeId("");
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
    setSelectedEdgeId("");
    setPast([]);
    setFuture([]);
  }, []);

  /** Replaces the aggregate after native import while preserving one undo checkpoint. */
  const replaceProject = useCallback((nextProject: ProjectState, label = "Import project") => {
    setProject((current) => {
      setPast((entries) => [
        ...entries.slice(-39),
        { project: cloneProject(current), label },
      ]);
      setFuture([]);
      return cloneProject(nextProject);
    });
    setSelectedNodeId(nextProject.nodes[0]?.id ?? "");
    setSelectedEdgeId("");
  }, []);

  /** Applies a previously reviewed portable GraphPatch; plugins never mutate this store directly. */
  const applyGraphPatch = useCallback(
    (patch: PluginGraphPatch) => {
      commit(`Apply plugin patch: ${patch.title}`, (draft) => {
        const now = new Date().toISOString();
        const phase = { "add-node": 0, "update-node": 1, "add-edge": 2, "update-edge": 3 } as const;
        const operations = [...patch.operations].sort(
          (left, right) => phase[left.op] - phase[right.op],
        );
        for (const operation of operations) {
          if (operation.op === "add-node") {
            if (draft.nodes.some((node) => node.id === operation.node.id)) continue;
            if (!NODE_TYPES.includes(operation.node.type as (typeof NODE_TYPES)[number])) continue;
            const index = draft.nodes.length;
            draft.nodes.push({
              id: operation.node.id,
              type: operation.node.type as ResearchNodeType,
              title: operation.node.title,
              body: operation.node.body ?? "Imported through a reviewed plugin GraphPatch.",
              tags: operation.node.tags ?? [],
              status: "draft",
              evidenceIds: [],
              data: operation.node.data ?? {},
              provenance: {
                origin: "import",
                actorId: patch.source.pluginId,
                sourceRefs: patch.source.externalId ? [patch.source.externalId] : [],
              },
              createdAt: now,
              updatedAt: now,
            });
            draft.placements.push({
              id: `placement-${operation.node.id}`,
              viewId: "view-main",
              nodeId: operation.node.id,
              x: 120 + (index % 5) * 220,
              y: 140 + Math.floor(index / 5) * 160,
              width: operation.node.type === "question" ? 136 : 176,
              height: operation.node.type === "question" ? 136 : 118,
            });
          } else if (operation.op === "add-edge") {
            if (
              draft.edges.some((edge) => edge.id === operation.edge.id) ||
              !EDGE_TYPES.includes(operation.edge.type as (typeof EDGE_TYPES)[number]) ||
              !draft.nodes.some((node) => node.id === operation.edge.source) ||
              !draft.nodes.some((node) => node.id === operation.edge.target)
            ) {
              continue;
            }
            draft.edges.push({
              id: operation.edge.id,
              type: operation.edge.type as ResearchEdgeType,
              source: operation.edge.source,
              target: operation.edge.target,
              directed: true,
              polarity: operation.edge.type === "contradicts" ? "negative" : "positive",
              conditions: [],
              evidenceIds: [],
              note: operation.edge.note,
              provenance: { origin: "import", actorId: patch.source.pluginId },
            });
          } else if (operation.op === "update-node") {
            const node = draft.nodes.find((item) => item.id === operation.nodeId);
            if (node) {
              if (typeof operation.changes.title === "string") node.title = operation.changes.title;
              if (typeof operation.changes.body === "string") node.body = operation.changes.body;
              if (
                Array.isArray(operation.changes.tags) &&
                operation.changes.tags.every((tag) => typeof tag === "string")
              ) {
                node.tags = operation.changes.tags;
              }
              if (
                operation.changes.data &&
                typeof operation.changes.data === "object" &&
                !Array.isArray(operation.changes.data)
              ) {
                node.data = { ...node.data, ...operation.changes.data };
              }
              node.updatedAt = now;
            }
          } else if (operation.op === "update-edge") {
            const edge = draft.edges.find((item) => item.id === operation.edgeId);
            if (edge) {
              if (typeof operation.changes.note === "string") edge.note = operation.changes.note;
              if (
                typeof operation.changes.type === "string" &&
                EDGE_TYPES.includes(operation.changes.type as ResearchEdgeType)
              ) {
                edge.type = operation.changes.type as ResearchEdgeType;
              }
              if (
                typeof operation.changes.confidence === "number" &&
                operation.changes.confidence >= 0 &&
                operation.changes.confidence <= 1
              ) {
                edge.confidence = operation.changes.confidence;
              }
            }
          }
        }
        draft.activity.push({
          id: makeId("activity"),
          label: `${patch.title} · ${patch.operations.length} proposed operations`,
          origin: "import",
          createdAt: now,
        });
      });
    },
    [commit],
  );

  return {
    project,
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
