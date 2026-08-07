"use client";

import {
  BaseEdge,
  EdgeLabelRenderer,
  getBezierPath,
  getSmoothStepPath,
  getStraightPath,
  type EdgeProps,
} from "@xyflow/react";
import { memo } from "react";
import type { WorkspaceEdge } from "../workspace-types";

/**
 * Semantic line treatment: solid causal, dashed control, dotted derived, red contradiction.
 * 语义线型：因果实线、控制虚线、派生点线、矛盾红线。
 */
export const ResearchEdgeLine = memo(function ResearchEdgeLine({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  sourcePosition,
  targetPosition,
  selected,
  data,
}: EdgeProps<WorkspaceEdge>) {
  const type = data?.record.type ?? "causes";
  const edgeStyle = data?.edgeStyle;
  const routing = edgeStyle?.routing ?? "orthogonal";
  const dragPreview = data?.dragPreview ?? false;
  const highlighted = data?.highlighted === true;
  const pathOptions = {
    sourceX,
    sourceY,
    targetX,
    targetY,
    sourcePosition,
    targetPosition,
  };
  const [path, labelX, labelY] =
    dragPreview || routing === "straight"
      ? getStraightPath(pathOptions)
      : routing === "bezier"
        ? getBezierPath(pathOptions)
        : getSmoothStepPath({
            ...pathOptions,
            borderRadius: edgeStyle?.stroke.cornerRadius ?? 12,
            offset: edgeStyle?.stroke.offset ?? 24,
          });
  const contradictory = type === "contradicts";
  const relationStyle = edgeStyle?.relations?.[type];
  const diffState = data?.diffState;
  const dash =
    diffState === "removed"
      ? "6 5"
      : relationStyle?.dash?.join(" ") ??
        (type === "controls" ? "7 6" : type === "derived_from" ? "2 4" : contradictory ? "7 4" : undefined);
  const strokeColor =
    diffState === "added"
      ? "var(--color-diff-added)"
      : diffState === "removed"
        ? "var(--color-diff-removed)"
        : diffState === "modified"
          ? "var(--color-diff-modified)"
          : selected
            ? "#2457d6"
            : relationStyle?.color ?? (contradictory ? "#c85c55" : edgeStyle?.stroke.color ?? "#656b72");
  const renderedLabelX = labelX + (data?.labelOffsetX ?? 0);
  const renderedLabelY = labelY + (data?.labelOffsetY ?? 0);

  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        style={{
          stroke: highlighted
            ? "var(--color-blue)"
            : strokeColor,
          strokeWidth: highlighted
            ? (edgeStyle?.stroke.selectedWidth ?? 2.6) + 0.6
            : selected
              ? edgeStyle?.stroke.selectedWidth ?? 2.6
              : relationStyle?.width ?? edgeStyle?.stroke.width ?? 1.05,
          opacity: highlighted
            ? 1
            : diffState === "removed"
              ? 0.55
              : relationStyle?.opacity ?? edgeStyle?.stroke.opacity ?? 1,
          strokeDasharray: dash,
        }}
      />
      {!dragPreview && (
        <EdgeLabelRenderer>
          <span
            className={[
              "pointer-events-none absolute -translate-x-1/2 -translate-y-1/2 bg-canvas px-1.5 font-serif text-[10px] italic",
              contradictory ? "text-alert" : selected ? "text-blue" : "text-ink/75",
            ].join(" ")}
            style={{ transform: `translate(-50%, -50%) translate(${renderedLabelX}px, ${renderedLabelY}px)` }}
          >
            {data?.label}
          </span>
        </EdgeLabelRenderer>
      )}
    </>
  );
});

ResearchEdgeLine.displayName = "ResearchEdgeLine";
