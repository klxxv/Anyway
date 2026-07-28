/**
 * React Flow rendering adapter for Research Canvas.
 * Research Canvas 的 React Flow 渲染适配层。
 *
 * This module converts derived UI data into node and edge visuals. It never
 * owns project state or calls graph algorithms, so the workspace coordinator
 * can evolve independently from rendering details.
 * 本模块把派生的 UI 数据转换成节点和连线视觉；它不持有项目状态也不调用图算法，
 * 因此工作区协调器可以独立于渲染细节演进。
 */

import {
  BaseEdge,
  EdgeLabelRenderer,
  Handle,
  NodeResizer,
  Position,
  getBezierPath,
  getSmoothStepPath,
  getStraightPath,
  type Edge,
  type EdgeProps,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import { Pin } from "lucide-react";
import type { CSSProperties } from "react";
import { builtInEdgeStyleCatalog } from "../plugins/catalog";
import type {
  BlockStyleId,
  EdgeStyleManifest,
  ResearchEdgeType,
  ResearchNode,
} from "../lib/research-types";

/**
 * Non-persisted node rendering data. / 非持久化的节点渲染数据。
 * The coordinator derives this object from project state and active UI tools.
 * 协调器从项目状态与当前 UI 工具派生此对象。
 */
export type CanvasNodeData = {
  record: ResearchNode;
  blockStyleId: BlockStyleId;
  disabled: boolean;
  depth?: number;
  traversed: boolean;
  chainState?: "effective" | "evidence" | "refutation";
  annotation?: string;
  influence?: number;
  collapsed?: boolean;
  pinned?: boolean;
  onResizeStart?: () => void;
  onResizeEnd?: () => void;
};

export type CanvasNode = Node<CanvasNodeData, "researchNode">;

/** Presentation-only edge data. / 仅用于展示的连线数据。 */
export type CanvasEdgeData = {
  type: ResearchEdgeType;
  edgeStyle: EdgeStyleManifest;
  confidence?: number;
  disabled: boolean;
  traversed: boolean;
  treeEdge: boolean;
  backEdge: boolean;
  chainState?: "effective" | "evidence" | "refutation";
  experimentLabel?: string;
  experimentDelta?: number;
};

export type CanvasEdge = Edge<CanvasEdgeData, "researchEdge">;

const nodeTypeLabels: Record<ResearchNode["type"], string> = {
  question: "Question",
  concept: "Concept",
  variable: "Variable",
  hypothesis: "Hypothesis",
  method: "Method",
  evidence: "Evidence",
  paper: "Paper",
  dataset: "Dataset",
  experiment: "Experiment",
  result: "Result",
  metric: "Metric",
  formula: "Formula",
  artifact: "Artifact",
  note: "Note",
};

const edgeTypeLabels: Record<ResearchEdgeType, string> = {
  causes: "causes",
  correlates: "correlates",
  supports: "supports",
  contradicts: "contradicts",
  depends_on: "depends on",
  derived_from: "derived from",
  part_of: "part of",
  controls: "controls",
  mediates: "mediates",
  moderates: "moderates",
  uses: "uses",
  measures: "measures",
};

/**
 * Renders a derived research node without modifying project data.
 * 渲染已派生的研究节点，不修改项目数据。
 */
function ResearchNodeCard({ data, selected }: NodeProps<CanvasNode>) {
  const {
    record,
    blockStyleId,
    disabled,
    depth,
    traversed,
    chainState,
    annotation,
    influence,
    collapsed,
    pinned,
    onResizeStart,
    onResizeEnd,
  } = data;
  return (
    <div
      className={[
        "research-node",
        `node-${record.type}`,
        `block-${blockStyleId}`,
        selected ? "is-selected" : "",
        disabled ? "is-disabled" : "",
        traversed ? "is-traversed" : "",
        chainState ? `is-chain-${chainState}` : "",
      ]
        .filter(Boolean)
        .join(" ")}
      data-testid={`node-${record.id}`}
      data-block-style={blockStyleId}
    >
      <NodeResizer
        isVisible={selected}
        minWidth={190}
        minHeight={94}
        maxWidth={520}
        maxHeight={360}
        onResizeStart={onResizeStart}
        onResizeEnd={onResizeEnd}
        lineClassName="research-node-resize-line"
        handleClassName="research-node-resize-handle"
      />
      <Handle type="target" position={Position.Left} className="research-handle" data-port="IN" />
      <div className="node-card-topline">
        <span className="node-kind">
          <i className="node-type-marker" aria-hidden="true" />
          <span className="node-type-label">{nodeTypeLabels[record.type]}</span>
        </span>
        <span className="node-view-state">
          {pinned && <Pin size={11} aria-label="Pinned node" />}
          {collapsed && <span title="Collapsed subtree">collapsed</span>}
          <span className={`status-dot status-${record.status}`} title={record.status} />
        </span>
      </div>
      <div className="node-title">{record.title}</div>
      <div className="node-summary">{record.body}</div>
      <div className="node-meta-row">
        <span>{record.evidenceIds.length} evidence</span>
        {traversed && typeof depth === "number" ? (
          <span className="depth-badge">depth {depth}</span>
        ) : (
          <span>{record.tags[0] ?? "untagged"}</span>
        )}
      </div>
      {(annotation || typeof influence === "number") && (
        <div className="node-analysis-row">
          {annotation && <span>{annotation}</span>}
          {typeof influence === "number" && (
            <span className={influence < 0 ? "negative" : "positive"}>
              BP {influence >= 0 ? "+" : ""}
              {(influence * 100).toFixed(0)}%
            </span>
          )}
        </div>
      )}
      {disabled && <div className="disabled-ribbon">disabled in scenario</div>}
      <Handle type="source" position={Position.Right} className="research-handle" data-port="OUT" />
    </div>
  );
}

type EdgePresentationState = {
  disabled?: boolean;
  traversed?: boolean;
  treeEdge?: boolean;
  backEdge?: boolean;
  chainState?: "effective" | "evidence" | "refutation";
  selected?: boolean;
};

/**
 * Resolves relation styling and temporary analysis states into one paint model.
 * 将关系样式与临时分析状态合成为单一绘制模型。
 */
function resolveEdgePresentation(
  edgeStyle: EdgeStyleManifest,
  edgeType: ResearchEdgeType,
  state: EdgePresentationState,
) {
  const relation = edgeStyle.relations?.[edgeType];
  let color = relation?.color ?? edgeStyle.stroke.color;
  let width = relation?.width ?? edgeStyle.stroke.width;
  let opacity = relation?.opacity ?? edgeStyle.stroke.opacity;
  let dash = relation?.dash ?? edgeStyle.stroke.dash;

  if (state.treeEdge) color = "#5b67c9";
  if (state.backEdge) {
    color = "#cf435b";
    dash = [6, 4];
  }
  if (state.traversed) {
    color = "var(--accent)";
    width = Math.max(width, 2.1);
  }
  if (state.chainState === "effective") {
    color = "#149b76";
    width = Math.max(width, 3.1);
    dash = undefined;
  }
  if (state.chainState === "evidence") {
    color = "#4e6fd2";
    width = Math.max(width, 3.1);
    dash = undefined;
  }
  if (state.chainState === "refutation") {
    color = "#d5485c";
    width = Math.max(width, 3.1);
    dash = [8, 5];
  }
  if (state.disabled) {
    color = "#9aa4b2";
    opacity = 0.38;
    dash = [5, 5];
  }
  if (state.selected) {
    width = Math.max(width, relation?.selectedWidth ?? edgeStyle.stroke.selectedWidth);
  }

  return { color, width, opacity, dash };
}

/**
 * Renders routed, semantic relation lines without reaching into application state.
 * 渲染带路由和语义的关系连线，不访问应用状态。
 */
function ResearchEdgeLine({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  markerEnd,
  data,
  selected,
}: EdgeProps<CanvasEdge>) {
  const route = data?.edgeStyle.routing ?? "bezier";
  const pathOptions = { sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition };
  const [path, labelX, labelY] =
    route === "orthogonal"
      ? getSmoothStepPath({ ...pathOptions, borderRadius: 0, offset: data?.edgeStyle.stroke.offset ?? 24 })
      : route === "smooth-step"
        ? getSmoothStepPath({
            ...pathOptions,
            borderRadius: data?.edgeStyle.stroke.cornerRadius ?? 8,
            offset: data?.edgeStyle.stroke.offset ?? 22,
          })
        : route === "straight"
          ? getStraightPath(pathOptions)
          : getBezierPath({ ...pathOptions, curvature: 0.28 });
  const presentation = resolveEdgePresentation(
    data?.edgeStyle ?? builtInEdgeStyleCatalog[0],
    data?.type ?? "depends_on",
    {
      disabled: data?.disabled,
      traversed: data?.traversed,
      treeEdge: data?.treeEdge,
      backEdge: data?.backEdge,
      chainState: data?.chainState,
      selected,
    },
  );
  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        interactionWidth={Math.max(18, presentation.width * 8)}
        style={{
          stroke: presentation.color,
          strokeWidth: presentation.width,
          strokeOpacity: presentation.opacity,
          strokeDasharray: presentation.dash?.join(" "),
        }}
        data-edge-routing={route}
        data-edge-style={data?.edgeStyle.id ?? builtInEdgeStyleCatalog[0].id}
        className={[
          "research-edge-path",
          selected ? "is-selected" : "",
          data?.disabled ? "is-disabled" : "",
          data?.traversed ? "is-traversed" : "",
          data?.treeEdge ? "is-tree-edge" : "",
          data?.backEdge ? "is-back-edge" : "",
          data?.chainState ? `is-chain-${data.chainState}` : "",
        ]
          .filter(Boolean)
          .join(" ")}
      />
      <EdgeLabelRenderer>
        <button
          className={[
            "edge-label",
            selected ? "is-selected" : "",
            data?.traversed ? "is-traversed" : "",
            data?.chainState ? `is-chain-${data.chainState}` : "",
          ]
            .filter(Boolean)
            .join(" ")}
          style={
            {
              transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
              "--edge-color": presentation.color,
            } as CSSProperties
          }
          tabIndex={-1}
        >
          {data?.experimentLabel ? (
            <>
              <span>{data.experimentLabel}</span>
              {typeof data.experimentDelta === "number" && (
                <small>Δ {(data.experimentDelta * 100).toFixed(2)} pp</small>
              )}
            </>
          ) : data ? (
            edgeTypeLabels[data.type]
          ) : (
            "related"
          )}
        </button>
      </EdgeLabelRenderer>
    </>
  );
}

/** Stable renderer maps passed to React Flow. / 传递给 React Flow 的稳定渲染器映射。 */
export const canvasNodeTypes = { researchNode: ResearchNodeCard };
export const canvasEdgeTypes = { researchEdge: ResearchEdgeLine };
