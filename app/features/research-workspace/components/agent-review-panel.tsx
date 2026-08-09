"use client";

import {
  IconAlertTriangle,
  IconCheck,
  IconGitBranch,
  IconPencil,
  IconSparkles,
  IconX,
} from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useI18n } from "../../../i18n/provider";
import type { AgentJobStage, AgentJobStatus } from "../../../plugins/agent-contracts";
import { isJobTerminal } from "../../../plugins/agent-contracts";
import type { GraphPatchOperation, PluginGraphPatch } from "../../../plugins/contracts";
import { normalizePluginGraphPatch } from "../../../plugins/workspace";
import type { PdfCompileResult } from "../../../platform/agent-client";
import { getPdfJobStatus, reviewPdfPatch } from "../../../platform/agent-client";

/** 逐项审阅决策 / Per-operation review decision. */
export type ReviewDecision = {
  accept: boolean;
  /** 接受时的就地编辑（add-node.title / add-edge.note） */
  edits?: Record<string, string>;
};

/** 操作主语摘要，用于列表展示 / Operation subject summary for display. */
export function operationSubject(operation: GraphPatchOperation): string {
  switch (operation.op) {
    case "add-node":
      return operation.node.title;
    case "add-edge":
      return `${operation.edge.source} → ${operation.edge.target}`;
    case "update-node":
      return operation.nodeId;
    case "update-edge":
      return operation.edgeId;
  }
}

/** 纯函数：按决策过滤操作并应用就地编辑，返回子集 patch；全部拒绝时返回 null。 */
export function buildAcceptedPatch(
  patch: PluginGraphPatch,
  decisions: Record<number, ReviewDecision>,
): PluginGraphPatch | null {
  const operations = patch.operations
    .map((operation, index) => {
      const decision = decisions[index];
      if (!decision || !decision.accept) return null;
      if (!decision.edits) return operation;
      if (operation.op === "add-node" && decision.edits.title !== undefined) {
        return { ...operation, node: { ...operation.node, title: decision.edits.title } };
      }
      if (operation.op === "add-edge" && decision.edits.note !== undefined) {
        return { ...operation, edge: { ...operation.edge, note: decision.edits.note } };
      }
      return operation;
    })
    .filter((operation): operation is GraphPatchOperation => operation !== null);
  if (operations.length === 0) return null;
  return { ...patch, operations };
}

/** 纯函数：统计被明确接受的操作数 / Count explicitly accepted operations. */
export function countAccepted(decisions: Record<number, ReviewDecision>, total: number): number {
  let count = 0;
  for (let index = 0; index < total; index += 1) {
    if (decisions[index]?.accept) count += 1;
  }
  return count;
}

/**
 * 阶段 3：Agent 提案审阅面板。
 * 只负责展示 GraphPatch 操作、逐项裁决与就地编辑；
 * 接受后发出 review_patch 命令并通过 onApply 交付过滤后的补丁，由宿主应用。
 */
export function AgentReviewPanel({
  jobId,
  compileResult,
  compileError,
  onClose,
  onApply,
  onReject,
  onRollback,
}: {
  jobId: string;
  compileResult?: PdfCompileResult | null;
  compileError?: string;
  onClose: () => void;
  onApply: (patch: PluginGraphPatch) => void;
  onReject: () => void;
  /** 后端 review 调用失败后回滚本地变更 / Roll back local mutation if backend review fails. */
  onRollback: () => void;
}) {
  const { t } = useI18n();
  const [status, setStatus] = useState<AgentJobStatus | null>(null);
  const [decisions, setDecisions] = useState<Record<number, ReviewDecision>>({});
  const [error, setError] = useState("");
  const [applying, setApplying] = useState(false);

  useEffect(() => {
    let mounted = true;
    void getPdfJobStatus(jobId)
      .then((snapshot) => {
        if (mounted) setStatus(snapshot);
      })
      .catch((loadError) => {
        if (mounted) setError(loadError instanceof Error ? loadError.message : String(loadError));
      });
    return () => {
      mounted = false;
    };
  }, [jobId]);

  const patch = useMemo(() => {
    if (!status?.result) return null;
    return normalizePluginGraphPatch(status.result);
  }, [status]);

  const counts = useMemo(() => {
    const byOp: Record<string, number> = {};
    for (const operation of patch?.operations ?? []) {
      byOp[operation.op] = (byOp[operation.op] ?? 0) + 1;
    }
    return byOp;
  }, [patch]);

  const setDecision = useCallback((index: number, decision: ReviewDecision) => {
    setDecisions((current) => ({ ...current, [index]: decision }));
  }, []);

  const setEdit = useCallback((index: number, field: string, value: string) => {
    setDecisions((current) => {
      const previous = current[index] ?? { accept: false };
      return { ...current, [index]: { ...previous, edits: { ...previous.edits, [field]: value } } };
    });
  }, []);

  const acceptAll = useCallback(() => {
    setDecisions(
      Object.fromEntries(
        (patch?.operations ?? []).map((_, index) => [
          index,
          { accept: true, edits: decisions[index]?.edits },
        ]),
      ),
    );
  }, [decisions, patch?.operations]);

  const applySelected = useCallback(async () => {
    if (!patch || applying) return;
    const accepted = buildAcceptedPatch(patch, decisions);
    setApplying(true);
    setError("");
    try {
      // 先应用本地变更（或关闭 UI），再通知后端；后端失败则回滚。
      // Apply locally first, then notify backend; rollback on backend failure.
      if (accepted) {
        onApply(accepted);
      } else {
        onReject();
      }
      await reviewPdfPatch(jobId, accepted !== null);
    } catch (applyError) {
      if (accepted) onRollback();
      setError(applyError instanceof Error ? applyError.message : String(applyError));
      setApplying(false);
    }
  }, [applying, decisions, jobId, onApply, onReject, onRollback, patch]);

  const rejectAll = useCallback(async () => {
    if (applying) return;
    setApplying(true);
    setError("");
    try {
      await reviewPdfPatch(jobId, false);
      onReject();
    } catch (rejectError) {
      setError(rejectError instanceof Error ? rejectError.message : String(rejectError));
      setApplying(false);
    }
  }, [applying, jobId, onReject]);

  const acceptedCount = patch ? countAccepted(decisions, patch.operations.length) : 0;
  const settled = status ? isJobTerminal(status.state) : false;

  return (
    <div className="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <section className="grid h-[min(680px,calc(100vh-32px))] w-[min(960px,calc(100vw-32px))] grid-cols-1 overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)] md:grid-cols-[minmax(0,1fr)_300px]">
        <div className="flex min-h-0 flex-col border-b border-ink/15 md:border-b-0 md:border-r">
          <header className="flex items-start justify-between border-b border-ink/15 px-7 py-5">
            <div>
              <span className="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">
                {t("agent.eyebrow")}
              </span>
              <h2 className="mt-1 flex items-center gap-2 font-serif text-[21px]">
                <IconSparkles size={19} stroke={1.35} />
                {t("agent.reviewTitle")}
              </h2>
              <p className="mt-1 max-w-[560px] font-serif text-[10px] leading-[1.5] text-ink/50">
                {t("agent.reviewSubtitle")}
              </p>
            </div>
            <button className="icon-quiet" onClick={onClose} aria-label={t("menu.close")}>
              <IconX size={18} stroke={1.35} />
            </button>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
            {!patch ? (
              <p className="grid min-h-[280px] place-items-center rounded-[6px] border border-dashed border-ink/20 bg-canvas px-5 text-center font-serif text-[11px] text-ink/45">
                {status ? t("workspace.noPatch") : `${t("agent.starting")} ${jobId.slice(0, 12)}…`}
              </p>
            ) : (
              <div className="space-y-2">
                {patch.operations.map((operation, index) => {
                  const decision = decisions[index] ?? { accept: false };
                  const editing = decision.accept;
                  const isNode = operation.op === "add-node";
                  const isEdge = operation.op === "add-edge";
                  const OpIcon = isNode ? IconCheck : isEdge ? IconGitBranch : IconPencil;
                  return (
                    <article
                      key={`${operation.op}-${index}`}
                      className={`rounded-[5px] border p-3 transition ${
                        editing ? "border-blue/35 bg-blue-soft/50" : "border-ink/15 bg-canvas opacity-60"
                      }`}
                    >
                      <div className="flex items-start gap-3">
                        <span
                          className={`mt-0.5 grid size-7 shrink-0 place-items-center rounded-full border ${
                            editing ? "border-blue/40 bg-blue-soft text-blue" : "border-ink/20 text-ink/45"
                          }`}
                        >
                          <OpIcon size={14} stroke={1.5} />
                        </span>
                        <div className="min-w-0 flex-1">
                          <div className="flex flex-wrap items-center gap-2">
                            <span className="rounded-[3px] bg-blue-soft px-1.5 py-0.5 font-sans text-[7px] uppercase tracking-[0.1em] text-blue">
                              {t(
                                operation.op === "add-node"
                                  ? "agent.op.addNode"
                                  : operation.op === "add-edge"
                                    ? "agent.op.addEdge"
                                    : operation.op === "update-node"
                                      ? "agent.op.updateNode"
                                      : "agent.op.updateEdge",
                              )}
                            </span>
                            <span className="rounded-[3px] bg-ink/8 px-1.5 py-0.5 font-sans text-[7px] uppercase tracking-[0.1em] text-ink/55">
                              {isNode
                                ? operation.node.type
                                : isEdge
                                  ? operation.edge.type
                                  : operation.op}
                            </span>
                          </div>
                          <p className="mt-1.5 truncate font-serif text-[12px]" title={operationSubject(operation)}>
                            {operationSubject(operation)}
                          </p>
                          {isNode && operation.node.body && (
                            <p className="mt-1 line-clamp-2 font-serif text-[9px] leading-[1.5] text-ink/50">
                              {operation.node.body}
                            </p>
                          )}
                          {editing && isNode && (
                            <label className="dialog-field mt-2">
                              {t("agent.op.title")}
                              <input
                                value={decision.edits?.title ?? operation.node.title}
                                onChange={(event) => setEdit(index, "title", event.target.value)}
                              />
                            </label>
                          )}
                          {editing && isEdge && (
                            <label className="dialog-field mt-2">
                              {t("agent.op.note")}
                              <input
                                value={decision.edits?.note ?? operation.edge.note ?? ""}
                                onChange={(event) => setEdit(index, "note", event.target.value)}
                              />
                            </label>
                          )}
                        </div>
                        <div className="flex shrink-0 items-center gap-1" role="group" aria-label={t("agent.decision.accept")}>
                          <button
                            className={`rounded-[4px] border px-2.5 py-1 font-sans text-[8px] transition ${
                              decision.accept
                                ? "border-blue bg-blue text-paper"
                                : "border-ink/20 text-ink/55 hover:border-blue hover:text-blue"
                            }`}
                            onClick={() => setDecision(index, { accept: true, edits: decision.edits })}
                          >
                            {t("agent.decision.accept")}
                          </button>
                          <button
                            className={`rounded-[4px] border px-2.5 py-1 font-sans text-[8px] transition ${
                              !decision.accept
                                ? "border-alert bg-alert text-paper"
                                : "border-ink/20 text-ink/55 hover:border-alert hover:text-alert"
                            }`}
                            onClick={() => setDecision(index, { accept: false })}
                          >
                            {t("agent.decision.reject")}
                          </button>
                        </div>
                      </div>
                    </article>
                  );
                })}
              </div>
            )}
          </div>
        </div>

        <aside className="flex min-h-0 flex-col overflow-y-auto bg-canvas p-5">
          <div className="flex items-center gap-2 text-blue">
            <IconSparkles size={17} stroke={1.35} />
            <h3 className="font-serif text-[14px]">{t("agent.reviewTitle")}</h3>
          </div>
          {status && (
            <p className="mt-2 font-serif text-[9px] leading-[1.5] text-ink/50">
              {status.pdfPath.split(/[\\/]/).pop()}
            </p>
          )}
          {patch && (
            <p className="mt-1 font-serif text-[10px] leading-[1.5] text-ink/60">
              {patch.operations.length} {t("agent.operations")} · {patch.summary}
            </p>
          )}
          {counts && (
            <div className="mt-3 flex flex-wrap gap-1">
              {Object.entries(counts).map(([op, count]) => (
                <span key={op} className="rounded-[3px] bg-ink/8 px-1.5 py-0.5 font-sans text-[7px] text-ink/60">
                  {op} × {count}
                </span>
              ))}
            </div>
          )}

          <div className="mt-5 space-y-2">
            <button
              className="button-primary w-full justify-center"
              disabled={!patch || applying || settled}
              onClick={() => void applySelected()}
            >
              <IconCheck size={15} stroke={1.35} />
              {t("agent.applySelected", { count: acceptedCount })}
            </button>
            <button
              className="button-secondary w-full justify-center"
              disabled={!patch || applying || settled}
              onClick={() => void rejectAll()}
            >
              <IconX size={15} stroke={1.35} />
              {t("agent.rejectAll")}
            </button>
            <button
              className="button-secondary w-full justify-center"
              disabled={!patch || applying}
              onClick={acceptAll}
            >
              {t("agent.acceptAll")}
            </button>
          </div>

          {error && (
            <p className="mt-3 flex items-start gap-2 rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-serif text-[9px] leading-[1.5] text-alert">
              <IconAlertTriangle size={13} stroke={1.5} className="mt-0.5 shrink-0" />
              <span className="min-w-0 break-words">{error}</span>
            </p>
          )}

          {compileError && (
            <p className="mt-3 flex items-start gap-2 rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-serif text-[9px] leading-[1.5] text-alert">
              <IconAlertTriangle size={13} stroke={1.5} className="mt-0.5 shrink-0" />
              <span className="min-w-0 break-words">{t("agent.compileFailed", { error: compileError })}</span>
            </p>
          )}

          {compileResult && (
            <div className="mt-5 border-t border-ink/15 pt-5">
              <h4 className="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
                {t("agent.compiling")}
              </h4>
              <dl className="mt-3 space-y-2">
                <div className="rounded-[4px] border border-ink/12 bg-paper px-3 py-2">
                  <dt className="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/45">
                    {t("agent.compileBlockHashes", { count: Object.keys(compileResult.compile.blockHashes).length })}
                  </dt>
                  <dd className="mt-1 truncate font-mono text-[8px] text-ink/70" title={compileResult.compile.contentRootHash}>
                    {t("agent.compileRootHash", { hash: compileResult.compile.contentRootHash.slice(0, 12) })}
                  </dd>
                </div>
                <div className="rounded-[4px] border border-ink/12 bg-paper px-3 py-2">
                  <dt className={`font-sans text-[7px] uppercase tracking-[0.12em] ${compileResult.compile.violations.length ? "text-alert" : "text-olive"}`}>
                    {compileResult.compile.violations.length
                      ? t("agent.compileInvariants", { count: compileResult.compile.violations.length })
                      : t("agent.compileInvariantsOk")}
                  </dt>
                  <dd className="mt-1 font-mono text-[8px] text-ink/60">
                    {compileResult.logicChain.summary}
                  </dd>
                </div>
                <div className="rounded-[4px] border border-ink/12 bg-paper px-3 py-2">
                  <dt className="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/45">
                    {t("agent.logicChain", { score: compileResult.logicChain.score.toFixed(2) })}
                  </dt>
                  <dd className="mt-1 font-mono text-[8px] text-ink/60">
                    {compileResult.logicChain.nodeIds.length} nodes · {compileResult.logicChain.edgeIds.length} edges
                  </dd>
                </div>
                <div className="rounded-[4px] border border-ink/12 bg-paper px-3 py-2">
                  <dt className={`font-sans text-[7px] uppercase tracking-[0.12em] ${compileResult.contradictions.cycles.length ? "text-alert" : "text-olive"}`}>
                    {compileResult.contradictions.cycles.length
                      ? t("agent.contradictions", { count: compileResult.contradictions.cycles.length })
                      : t("agent.contradictionsOk")}
                  </dt>
                  <dd className="mt-1 font-mono text-[8px] text-ink/60">
                    {t("agent.beliefs", { value: compileResult.beliefs.meanNetBelief.toFixed(2) })}
                  </dd>
                </div>
              </dl>
            </div>
          )}

          <p className="mt-auto pt-5 font-serif text-[8px] text-ink/35">
            {status ? t(`agent.stage.${status.state}` as `agent.stage.${AgentJobStage}`) : ""}
          </p>
        </aside>
      </section>
    </div>
  );
}
