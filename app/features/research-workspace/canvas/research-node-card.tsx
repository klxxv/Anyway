"use client";

import {
  IconChartHistogram,
  IconCheck,
  IconDatabase,
  IconFileText,
  IconFlask2,
  IconNote,
  IconQuestionMark,
  IconUsersGroup,
} from "@tabler/icons-react";
import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { CSSProperties } from "react";
import type { ResearchNodeType } from "../../../lib/research-types";
import type { WorkspaceNode } from "../workspace-types";

function NodeIcon({ type, selected }: { type: ResearchNodeType; selected: boolean }) {
  const props = {
    size: 20,
    stroke: 1.35,
    className: selected ? "mb-1 text-blue" : "mb-1 text-ink/75",
    "aria-hidden": true,
  } as const;
  switch (type) {
    case "question":
      return <IconQuestionMark {...props} />;
    case "concept":
      return <IconUsersGroup {...props} />;
    case "variable":
      return <IconChartHistogram {...props} />;
    case "method":
      return <IconFlask2 {...props} />;
    case "evidence":
    case "paper":
      return <IconFileText {...props} />;
    case "dataset":
      return <IconDatabase {...props} />;
    case "result":
      return <IconCheck {...props} />;
    default:
      return <IconNote {...props} />;
  }
}

/**
 * The graph node mirrors the reference's quiet serif cards and circular questions.
 * 图节点复刻参考图中的安静衬线卡片与圆形问题节点。
 */
export function ResearchNodeCard({ data, selected }: NodeProps<WorkspaceNode>) {
  const { record, shape } = data;
  const valueType =
    record.type === "variable" && typeof record.data.valueType === "string"
      ? record.data.valueType
      : record.tags[0] || "summary";
  const disputed = record.tags.includes("disputed");

  return (
    <article
      className={[
        "zen-node group relative flex h-full w-full flex-col items-center justify-center border bg-paper px-3 py-3 text-center text-ink transition",
        shape === "circle" ? "rounded-full" : "rounded-[3px]",
        selected
          ? "border-blue shadow-[0_0_0_1px_#2457d6]"
          : disputed
            ? "border-alert"
            : "border-ink/70 hover:border-ink",
      ].join(" ")}
      aria-label={`${record.type}: ${record.title}`}
    >
      <NodeIcon type={record.type} selected={selected} />
      <h3 className="max-w-[13rem] font-serif text-[14px] leading-[1.18]">{record.title}</h3>
      {record.type !== "question" && (
        <p className="mt-2 font-serif text-[10px] leading-none text-ink/60">
          {record.type} · {valueType}
        </p>
      )}
      {disputed && (
        <span className="absolute -bottom-3 -right-3 grid size-6 place-items-center rounded-full border border-alert bg-paper font-serif text-[14px] text-alert">
          !
        </span>
      )}

      {[
        ["left", Position.Left],
        ["right", Position.Right],
        ["top", Position.Top],
        ["bottom", Position.Bottom],
      ].map(([id, position]) => (
        <Handle
          key={id}
          className="zen-handle"
          id={id as string}
          type="source"
          position={position as Position}
        />
      ))}
      {[
        ["left-top", Position.Left, { top: "32%" }],
        ["left-bottom", Position.Left, { top: "68%" }],
        ["right-top", Position.Right, { top: "32%" }],
        ["right-bottom", Position.Right, { top: "68%" }],
        ["top-left", Position.Top, { left: "32%" }],
        ["top-right", Position.Top, { left: "68%" }],
        ["bottom-left", Position.Bottom, { left: "32%" }],
        ["bottom-right", Position.Bottom, { left: "68%" }],
      ].map(([id, position, style]) => (
        <Handle
          key={id as string}
          className="zen-route-handle"
          id={id as string}
          type="source"
          position={position as Position}
          style={style as CSSProperties}
          isConnectable={false}
        />
      ))}
    </article>
  );
}
