"use client";

import {
  BaseEdge,
  EdgeLabelRenderer,
  getStraightPath,
  type EdgeProps,
} from "@xyflow/react";
import type { WorkspaceEdge } from "../workspace-types";

/**
 * Semantic line treatment: solid causal, dashed control, dotted derived, red contradiction.
 * 语义线型：因果实线、控制虚线、派生点线、矛盾红线。
 */
export function ResearchEdgeLine({
  id,
  sourceX,
  sourceY,
  targetX,
  targetY,
  selected,
  data,
}: EdgeProps<WorkspaceEdge>) {
  const [path, labelX, labelY] = getStraightPath({
    sourceX,
    sourceY,
    targetX,
    targetY,
  });
  const type = data?.record.type ?? "causes";
  const contradictory = type === "contradicts";
  const dash =
    type === "controls" ? "7 6" : type === "derived_from" ? "2 4" : contradictory ? "7 4" : undefined;

  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        style={{
          stroke: contradictory ? "#c85c55" : selected ? "#2457d6" : "#656b72",
          strokeWidth: selected ? 1.6 : 1.05,
          strokeDasharray: dash,
        }}
      />
      <EdgeLabelRenderer>
        <span
          className={[
            "pointer-events-none absolute -translate-x-1/2 -translate-y-1/2 bg-canvas px-1.5 font-serif text-[10px] italic",
            contradictory ? "text-alert" : selected ? "text-blue" : "text-ink/75",
          ].join(" ")}
          style={{ transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)` }}
        >
          {data?.label}
        </span>
      </EdgeLabelRenderer>
    </>
  );
}
