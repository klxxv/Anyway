"use client";

import {
  IconChartHistogram,
  IconCheck,
  IconChevronDown,
  IconChevronRight,
  IconDatabase,
  IconFileText,
  IconFlask2,
  IconNote,
  IconQuestionMark,
  IconUsersGroup,
} from "@tabler/icons-react";
import { Handle, Position, useUpdateNodeInternals, type NodeProps } from "@xyflow/react";
import { memo, useEffect, type CSSProperties } from "react";
import type { MessageKey } from "../../../i18n/catalog";
import { useI18n } from "../../../i18n/provider";
import type { ResearchNodeType } from "../../../lib/research-types";
import type { WorkspaceNode } from "../workspace-types";
import { variableBranchValues } from "./variable-branches";

const nodeTypeMessageKeys: Partial<Record<ResearchNodeType, MessageKey>> = {
  question: "node.question",
  concept: "node.group",
  variable: "node.variable",
  method: "node.method",
  dataset: "node.data",
  evidence: "node.evidence",
  result: "node.result",
  note: "node.note",
};

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
export const ResearchNodeCard = memo(function ResearchNodeCard({ data, selected }: NodeProps<WorkspaceNode>) {
  const { t } = useI18n();
  const updateNodeInternals = useUpdateNodeInternals();
  const { record, shape, expanded, onToggleExpanded, diffState } = data;
  const highlighted = data.highlighted === true;
  const valueType =
    record.type === "variable" && typeof record.data.valueType === "string"
      ? record.data.valueType
      : record.tags[0] || "summary";
  const disputed = record.tags.includes("disputed");
  const branchValues = variableBranchValues(record);
  const expandable = branchValues.length > 0;
  const binary = record.data.valueType === "bool";
  const ghost = diffState === "removed";

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => updateNodeInternals(record.id));
    return () => window.cancelAnimationFrame(frame);
  }, [expanded, record.id, updateNodeInternals]);

  const diffBorder = diffState
    ? diffState === "added"
      ? "border-diff-added bg-diff-added-soft"
      : diffState === "removed"
        ? "border-diff-removed border-dashed bg-diff-removed-soft"
        : "border-diff-modified bg-diff-modified-soft"
    : null;

  return (
    <article
      className={[
        "zen-node group relative flex h-full w-full flex-col items-center border bg-paper px-3 py-3 text-center text-ink transition",
        expanded ? "justify-start" : "justify-center",
        shape === "circle" ? "rounded-full" : "rounded-[3px]",
        diffBorder ??
          (selected
            ? "border-blue shadow-[0_0_0_1px_#2457d6]"
            : highlighted
              ? "chain-highlight-node"
              : disputed
                ? "border-alert"
                : "border-ink/70 hover:border-ink"),
        ghost ? "pointer-events-none opacity-60" : "",
      ].join(" ")}
      aria-label={`${t(nodeTypeMessageKeys[record.type] ?? "node.note")}: ${record.title}`}
    >
      <NodeIcon type={record.type} selected={selected} />
      <h3 className="max-w-[13rem] font-serif text-[14px] leading-[1.18]">{record.title}</h3>
      {record.type !== "question" && (
        <p className="mt-2 font-serif text-[10px] leading-none text-ink/60">
          {t(nodeTypeMessageKeys[record.type] ?? "node.note")} · {valueType}
        </p>
      )}
      {expandable && (
        <button
          type="button"
          className="nodrag nopan absolute right-1.5 top-1.5 grid size-6 place-items-center rounded-full text-ink/45 transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-blue"
          aria-label={t(expanded ? "node.collapseBranches" : "node.expandBranches")}
          aria-expanded={expanded}
          onClick={(event) => {
            event.stopPropagation();
            onToggleExpanded(record.id);
          }}
        >
          {expanded ? (
            <IconChevronDown size={14} stroke={1.6} />
          ) : (
            <IconChevronRight size={14} stroke={1.6} />
          )}
        </button>
      )}
      {expanded && (
        <div
          className={`nodrag nopan mt-3 w-full border-t border-ink/12 pt-3 ${
            binary ? "grid grid-cols-2 gap-2" : "space-y-1.5"
          }`}
          aria-label={t("inspector.values")}
        >
          {branchValues.map((value, index) => (
            <div
              key={`${value}-${index}`}
              className={
                binary
                  ? "relative border-t border-ink/30 pt-2 before:absolute before:-top-2 before:left-1/2 before:h-2 before:border-l before:border-ink/30"
                  : "relative flex items-center pl-4 before:absolute before:left-1 before:top-1/2 before:w-3 before:border-t before:border-ink/30 after:absolute after:bottom-1/2 after:left-1 after:top-[-7px] after:border-l after:border-ink/30"
              }
            >
              <span className="block w-full truncate rounded-[3px] border border-ink/15 bg-canvas px-2 py-1 font-sans text-[9px] text-ink/70">
                {value}
              </span>
            </div>
          ))}
        </div>
      )}
      {diffState && (
        <span
          className={`absolute -top-3 -right-3 grid size-6 place-items-center rounded-full border bg-paper font-serif text-[13px] font-bold shadow-sm ${
            diffState === "added"
              ? "border-diff-added text-diff-added"
              : diffState === "removed"
                ? "border-diff-removed text-diff-removed"
                : "border-diff-modified text-diff-modified"
          }`}
          aria-hidden
        >
          {diffState === "added" ? "+" : diffState === "removed" ? "−" : "~"}
        </span>
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
});

ResearchNodeCard.displayName = "ResearchNodeCard";
