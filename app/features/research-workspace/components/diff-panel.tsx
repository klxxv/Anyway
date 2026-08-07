"use client";

import {
  IconArrowRight,
  IconColumns3,
  IconMinus,
  IconPencil,
  IconPlus,
  IconStack2,
  IconX,
} from "@tabler/icons-react";
import { useMemo } from "react";
import type { MessageKey } from "../../../i18n/catalog";
import { useI18n } from "../../../i18n/provider";
import type { CanvasDiffResult, DiffState } from "../../../lib/graph/canvas-diff";
import type { ProjectState, ResearchEdge, ResearchNodeType } from "../../../lib/research-types";

/** 对比模式：并排两栏（side-by-side）或画布叠加（overlay）。 */
export type DiffMode = "side-by-side" | "overlay";

/** 可选对比版本：由 App 层从当前项目与撤销历史组装。 */
export type DiffVersion = {
  id: string;
  label: string;
  project: ProjectState;
};

export type DiffPanelProps = {
  versions: DiffVersion[];
  baseId: string;
  compareId: string;
  mode: DiffMode;
  result: CanvasDiffResult | null;
  loading: boolean;
  error: string | null;
  onBaseChange: (id: string) => void;
  onCompareChange: (id: string) => void;
  onModeChange: (mode: DiffMode) => void;
  onClose: () => void;
  /** 点击变更条目后定位到画布（App 层切换叠加模式并聚焦）。 */
  onFocus: (kind: "node" | "edge", entityId: string) => void;
};

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

const edgeTypeMessageKeys: Partial<Record<ResearchEdge["type"], MessageKey>> = {
  causes: "relation.causal",
  controls: "relation.control",
  derived_from: "relation.derived",
  contradicts: "relation.contradicts",
};

const diffMeta: Record<DiffState, { labelKey: MessageKey; icon: typeof IconPlus; textClass: string; bgClass: string }> = {
  added: {
    labelKey: "diff.added",
    icon: IconPlus,
    textClass: "text-diff-added",
    bgClass: "bg-diff-added-soft",
  },
  removed: {
    labelKey: "diff.removed",
    icon: IconMinus,
    textClass: "text-diff-removed",
    bgClass: "bg-diff-removed-soft",
  },
  modified: {
    labelKey: "diff.modified",
    icon: IconPencil,
    textClass: "text-diff-modified",
    bgClass: "bg-diff-modified-soft",
  },
};

function versionById(versions: DiffVersion[], id: string): DiffVersion | null {
  return versions.find((version) => version.id === id) ?? null;
}

function nodeTitleMap(project: ProjectState): Map<string, string> {
  return new Map(project.nodes.map((node) => [node.id, node.title]));
}

function StatBadge({ state, count }: { state: DiffState; count: number }) {
  const { t } = useI18n();
  if (count === 0) return null;
  const meta = diffMeta[state];
  const Icon = meta.icon;
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full px-2 py-0.5 font-sans text-[9px] font-medium ${meta.bgClass} ${meta.textClass}`}
    >
      <Icon size={10} stroke={1.6} />
      {t(meta.labelKey)} {count}
    </span>
  );
}

/** 并排模式下单个版本的栏：节点/边列表，变更条目按 diff 状态着色。 */
function VersionColumn({
  version,
  stateOf,
  onFocus,
}: {
  version: DiffVersion;
  stateOf: (id: string) => DiffState | null;
  onFocus: DiffPanelProps["onFocus"];
}) {
  const { t } = useI18n();
  const titles = useMemo(() => nodeTitleMap(version.project), [version.project]);
  const nodes = version.project.nodes;
  const edges = version.project.edges;
  const rowClass = (state: DiffState | null) =>
    state
      ? `${diffMeta[state].bgClass} ${diffMeta[state].textClass}`
      : "text-ink/70 hover:bg-ink/5";
  return (
    <div className="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-[6px] border border-ink/15 bg-paper">
      <div className="flex items-center justify-between gap-2 border-b border-ink/10 px-3 py-2">
        <span className="truncate font-serif text-[12px] font-semibold">{version.label}</span>
        <span className="shrink-0 font-sans text-[9px] uppercase tracking-[0.12em] text-ink/45">
          {t("diff.nodes")} {nodes.length} · {t("diff.edges")} {edges.length} ·{" "}
          {t("diff.evidence")} {version.project.evidence.length}
        </span>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-1.5">
        {nodes.length === 0 && edges.length === 0 ? (
          <p className="px-2 py-4 text-center font-serif text-[11px] text-ink/40">
            {t("diff.noChanges")}
          </p>
        ) : null}
        {nodes.map((node) => {
          const state = stateOf(node.id);
          return (
            <button
              key={node.id}
              type="button"
              onClick={() => onFocus("node", node.id)}
              className={`flex w-full items-center gap-2 rounded-[4px] px-2 py-1.5 text-left transition ${rowClass(state)}`}
            >
              <span className="w-16 shrink-0 truncate font-sans text-[8px] uppercase tracking-[0.1em] text-ink/45">
                {t(nodeTypeMessageKeys[node.type] ?? "node.note")}
              </span>
              <span className="min-w-0 flex-1 truncate font-serif text-[11px]">{node.title}</span>
              {state ? (
                <span className="shrink-0 font-sans text-[9px] font-medium">
                  {t(diffMeta[state].labelKey)}
                </span>
              ) : null}
            </button>
          );
        })}
        {edges.map((edge) => {
          const state = stateOf(edge.id);
          return (
            <button
              key={edge.id}
              type="button"
              onClick={() => onFocus("edge", edge.id)}
              className={`flex w-full items-center gap-2 rounded-[4px] px-2 py-1.5 text-left transition ${rowClass(state)}`}
            >
              <span className="w-16 shrink-0 truncate font-sans text-[8px] uppercase tracking-[0.1em] text-ink/45">
                {t(edgeTypeMessageKeys[edge.type] ?? "relation.causal")}
              </span>
              <IconArrowRight size={10} stroke={1.5} className="shrink-0 text-ink/35" />
              <span className="min-w-0 flex-1 truncate font-serif text-[10px]">
                {titles.get(edge.source) ?? edge.source} → {titles.get(edge.target) ?? edge.target}
              </span>
              {state ? (
                <span className="shrink-0 font-sans text-[9px] font-medium">
                  {t(diffMeta[state].labelKey)}
                </span>
              ) : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}

/** 叠加模式下的变更列表：按类别分组，点击条目定位画布。 */
function ChangeList({
  result,
  onFocus,
}: {
  result: CanvasDiffResult;
  onFocus: DiffPanelProps["onFocus"];
}) {
  const { t } = useI18n();
  const groups: Array<{
    titleKey: MessageKey;
    items: Array<{ id: string; state: DiffState; kind: "node" | "edge" }>;
  }> = useMemo(() => {
    const toItems = (ids: string[], state: DiffState, kind: "node" | "edge") =>
      ids.map((id) => ({ id, state, kind }));
    return [
      { titleKey: "diff.nodes" as MessageKey, items: [
        ...toItems(result.addedNodes, "added", "node"),
        ...toItems(result.removedNodes, "removed", "node"),
        ...result.modifiedNodes.map((entity) => ({ id: entity.entityId, state: "modified" as DiffState, kind: "node" as const })),
      ]},
      { titleKey: "diff.edges" as MessageKey, items: [
        ...toItems(result.addedEdges, "added", "edge"),
        ...toItems(result.removedEdges, "removed", "edge"),
        ...result.modifiedEdges.map((entity) => ({ id: entity.entityId, state: "modified" as DiffState, kind: "edge" as const })),
      ]},
    ];
  }, [result]);
  const rowClass = (state: DiffState) => `${diffMeta[state].textClass} hover:bg-ink/5`;
  return (
    <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-1.5">
      {groups.map((group) => {
        const count = group.items.length;
        return (
          <div key={group.titleKey} className="mb-2">
            <div className="flex items-center gap-2 px-2 py-1">
              <span className="font-sans text-[9px] uppercase tracking-[0.14em] text-ink/45">
                {t(group.titleKey)}
              </span>
              <span className="rounded-full bg-ink/8 px-1.5 font-sans text-[9px] text-ink/55">
                {count}
              </span>
            </div>
            {count === 0 ? (
              <p className="px-2 pb-2 font-serif text-[10px] text-ink/40">—</p>
            ) : (
              group.items.map((item) => (
                <button
                  key={`${item.kind}-${item.id}`}
                  type="button"
                  onClick={() => onFocus(item.kind, item.id)}
                  className={`flex w-full items-center gap-2 rounded-[4px] px-2 py-1.5 text-left font-serif text-[11px] transition ${rowClass(item.state)}`}
                >
                  <span className={`grid size-4 shrink-0 place-items-center rounded-full font-sans text-[9px] font-bold ${diffMeta[item.state].bgClass}`}>
                    {item.state === "added" ? "+" : item.state === "removed" ? "−" : "~"}
                  </span>
                  <span className="min-w-0 flex-1 truncate">{item.id}</span>
                  <span className="shrink-0 font-sans text-[9px]">{t(diffMeta[item.state].labelKey)}</span>
                </button>
              ))
            )}
          </div>
        );
      })}
    </div>
  );
}

/**
 * Canvas Diff 面板：并排对比与变更高亮。
 * 纯展示组件，diff 计算由 App 层完成；不耦合画布或工作区状态。
 */
export function DiffPanel({
  versions,
  baseId,
  compareId,
  mode,
  result,
  loading,
  error,
  onBaseChange,
  onCompareChange,
  onModeChange,
  onClose,
  onFocus,
}: DiffPanelProps) {
  const { t } = useI18n();
  const base = versionById(versions, baseId);
  const compare = versionById(versions, compareId);

  const baseStateOf = (id: string): DiffState | null => {
    if (!result) return null;
    if (result.removedNodes.includes(id)) return "removed";
    if (result.modifiedNodes.some((entity) => entity.entityId === id)) return "modified";
    if (result.removedEdges.includes(id)) return "removed";
    if (result.modifiedEdges.some((entity) => entity.entityId === id)) return "modified";
    return null;
  };
  const compareStateOf = (id: string): DiffState | null => {
    if (!result) return null;
    if (result.addedNodes.includes(id)) return "added";
    if (result.modifiedNodes.some((entity) => entity.entityId === id)) return "modified";
    if (result.addedEdges.includes(id)) return "added";
    if (result.modifiedEdges.some((entity) => entity.entityId === id)) return "modified";
    return null;
  };

  const totals = useMemo(() => {
    if (!result) return null;
    const added =
      result.addedNodes.length + result.addedEdges.length + result.addedEvidence.length;
    const removed =
      result.removedNodes.length + result.removedEdges.length + result.removedEvidence.length;
    const modified =
      result.modifiedNodes.length + result.modifiedEdges.length + result.modifiedEvidence.length;
    return { added, removed, modified, changed: added + removed + modified };
  }, [result]);

  return (
    <div
      className={`fixed top-0 bottom-0 z-40 flex flex-col bg-paper text-ink ${
        mode === "overlay"
          ? "right-0 w-[400px] border-l border-ink/15 shadow-[0_0_40px_rgba(30,32,35,.12)]"
          : "inset-0"
      }`}
      role="dialog"
      aria-label={t("diff.title")}
      aria-modal={mode === "side-by-side"}
    >
      <header className="flex h-12 shrink-0 items-center gap-3 border-b border-ink/15 bg-paper px-4">
        <button
          type="button"
          onClick={onClose}
          className="grid size-8 place-items-center rounded-[4px] text-ink/60 transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-blue"
          aria-label={t("diff.close")}
        >
          <IconX size={17} stroke={1.4} />
        </button>
        <span className="font-serif text-[15px] font-semibold">{t("diff.title")}</span>

        <div className="ml-2 flex items-center overflow-hidden rounded-[4px] border border-ink/20 bg-canvas p-0.5">
          <button
            type="button"
            onClick={() => onModeChange("side-by-side")}
            className={`flex items-center gap-1.5 rounded-[3px] px-2.5 py-1 font-sans text-[10px] transition ${
              mode === "side-by-side" ? "bg-paper text-blue shadow-sm" : "text-ink/55 hover:text-ink"
            }`}
            aria-pressed={mode === "side-by-side"}
          >
            <IconColumns3 size={13} stroke={1.5} />
            {t("diff.sideBySide")}
          </button>
          <button
            type="button"
            onClick={() => onModeChange("overlay")}
            className={`flex items-center gap-1.5 rounded-[3px] px-2.5 py-1 font-sans text-[10px] transition ${
              mode === "overlay" ? "bg-paper text-blue shadow-sm" : "text-ink/55 hover:text-ink"
            }`}
            aria-pressed={mode === "overlay"}
          >
            <IconStack2 size={13} stroke={1.5} />
            {t("diff.overlay")}
          </button>
        </div>

        {mode === "side-by-side" && (
          <>
            <VersionSelect
              label={t("diff.base")}
              value={baseId}
              versions={versions}
              onChange={onBaseChange}
            />
            <IconArrowRight size={13} stroke={1.4} className="text-ink/40" />
            <VersionSelect
              label={t("diff.comparison")}
              value={compareId}
              versions={versions}
              onChange={onCompareChange}
            />
          </>
        )}

        <div className="ml-auto hidden items-center gap-2 md:flex">
          {totals && totals.changed > 0 && (
            <>
              <StatBadge state="added" count={totals.added} />
              <StatBadge state="removed" count={totals.removed} />
              <StatBadge state="modified" count={totals.modified} />
            </>
          )}
        </div>
      </header>

      <div className="flex min-h-0 flex-1 flex-col">
        {loading ? (
          <p className="m-auto font-serif text-[12px] text-ink/50">{t("diff.computing")}</p>
        ) : error ? (
          <p className="m-auto max-w-md text-center font-serif text-[12px] text-diff-removed">
            {error}
          </p>
        ) : !result || totals?.changed === 0 ? (
          <p className="m-auto font-serif text-[12px] text-ink/50">{t("diff.noChanges")}</p>
        ) : mode === "side-by-side" ? (
          <div className="grid min-h-0 flex-1 grid-cols-2 gap-3 p-3">
            {base && <VersionColumn version={base} stateOf={baseStateOf} onFocus={onFocus} />}
            {compare && (
              <VersionColumn version={compare} stateOf={compareStateOf} onFocus={onFocus} />
            )}
          </div>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col border-l border-ink/15 bg-canvas">
            <div className="flex items-center justify-between border-b border-ink/10 px-3 py-2">
              <span className="font-serif text-[11px] text-olive">
                {compare?.label ?? ""} · {t("diff.overlayHint")}
              </span>
            </div>
            {result && <ChangeList result={result} onFocus={onFocus} />}
          </div>
        )}
      </div>
    </div>
  );
}

function VersionSelect({
  label,
  value,
  versions,
  onChange,
}: {
  label: string;
  value: string;
  versions: DiffVersion[];
  onChange: (id: string) => void;
}) {
  return (
    <label className="flex items-center gap-1.5">
      <span className="font-sans text-[9px] uppercase tracking-[0.12em] text-ink/45">{label}</span>
      <select
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="max-w-[180px] rounded-[4px] border border-ink/20 bg-paper px-2 py-1 font-serif text-[11px] text-ink outline-none transition focus:border-blue"
      >
        {versions.map((version) => (
          <option key={version.id} value={version.id}>
            {version.label}
          </option>
        ))}
      </select>
    </label>
  );
}
