"use client";

import {
  IconAlertTriangle,
  IconCheck,
  IconFileText,
  IconRefresh,
  IconUpload,
  IconX,
} from "@tabler/icons-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useI18n } from "../../../i18n/provider";
import type { AgentJobStage, AgentJobStatus } from "../../../plugins/agent-contracts";
import { AGENT_STAGE_LABELS, isJobTerminal } from "../../../plugins/agent-contracts";
import {
  getPdfJobStatus,
  listenForPdfDrops,
  pickPdfFile,
  startPdfJob,
} from "../../../platform/agent-client";

/** 上传管线阶段顺序（不含终态）/ Pipeline stages in execution order (terminal states excluded). */
export const PDF_PIPELINE_STAGES: readonly AgentJobStage[] = [
  "created",
  "validating_file",
  "extracting_text",
  "ocr_optional",
  "building_document_map",
  "extracting_semantics",
  "generating_patch",
  "awaiting_review",
];

/** 阶段在管线中的序号；终态返回 -1 / Index of a stage in the pipeline; -1 for terminal states. */
export function jobStageIndex(state: AgentJobStage): number {
  return PDF_PIPELINE_STAGES.indexOf(state);
}

/** 纯进度推导：done/total 检查点 + 百分比 + 当前阶段标签 / Pure progress derivation. */
export function deriveUploadProgress(status: AgentJobStatus): {
  done: number;
  total: number;
  percent: number;
  stageLabel: string;
} {
  const [done, total] = status.progress;
  const percent = total > 0 ? Math.min(100, Math.round((done / total) * 100)) : 0;
  return {
    done,
    total,
    percent,
    stageLabel: AGENT_STAGE_LABELS[status.state] ?? status.state,
  };
}

/**
 * 阶段 1：PDF 上传 + Job 进度。
 * 只负责选文件与展示状态；完成后通过 onReady 交出 jobId，不触碰图谱。
 */
export function PdfUploadDialog({
  onClose,
  onReady,
}: {
  onClose: () => void;
  onReady: (jobId: string, status: AgentJobStatus) => void;
}) {
  const { t } = useI18n();
  const [pdfPath, setPdfPath] = useState<string | null>(null);
  const [status, setStatus] = useState<AgentJobStatus | null>(null);
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);
  const [dragOver, setDragOver] = useState(false);
  const pollTimer = useRef<number | null>(null);
  const droppedRef = useRef(false);

  const stopPolling = useCallback(() => {
    if (pollTimer.current !== null) {
      window.clearInterval(pollTimer.current);
      pollTimer.current = null;
    }
  }, []);

  useEffect(() => stopPolling, [stopPolling]);

  /** 轮询 job 直到进入审阅/终态；解耦：本组件只上报，不决策。 */
  const watchJob = useCallback(
    (jobId: string) => {
      stopPolling();
      pollTimer.current = window.setInterval(async () => {
        try {
          const next = await getPdfJobStatus(jobId);
          setStatus(next);
          if (next.state === "awaiting_review") {
            stopPolling();
            onReady(jobId, next);
          } else if (isJobTerminal(next.state)) {
            stopPolling();
            setError(next.error ?? `Unexpected terminal state: ${next.state}`);
          }
        } catch (pollError) {
          stopPolling();
          setError(pollError instanceof Error ? pollError.message : String(pollError));
        }
      }, 700);
    },
    [onReady, stopPolling],
  );

  const chooseFile = useCallback(async () => {
    try {
      const path = await pickPdfFile();
      if (path) {
        setPdfPath(path);
        setError("");
      }
    } catch (chooseError) {
      setError(
        chooseError instanceof Error && chooseError.message === "DESKTOP_REQUIRED"
          ? t("agent.desktopOnly")
          : chooseError instanceof Error
            ? chooseError.message
            : String(chooseError),
      );
    }
  }, [t]);

  const beginParse = useCallback(async () => {
    if (!pdfPath || starting) return;
    setStarting(true);
    setError("");
    droppedRef.current = false;
    try {
      const job = await startPdfJob(pdfPath);
      setStatus(job);
      if (job.state === "awaiting_review") {
        onReady(job.jobId, job);
      } else if (isJobTerminal(job.state)) {
        setError(job.error ?? `Unexpected terminal state: ${job.state}`);
      } else {
        watchJob(job.jobId);
      }
    } catch (startError) {
      setError(startError instanceof Error ? startError.message : String(startError));
      setStarting(false);
    }
  }, [onReady, pdfPath, starting, watchJob]);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    let mounted = true;
    void listenForPdfDrops((paths) => {
      if (!mounted) return;
      const first = paths[0];
      if (first) {
        droppedRef.current = true;
        setPdfPath(first);
        setError("");
      }
    }).then((cleanup) => {
      if (!mounted) cleanup();
      else dispose = cleanup;
    });
    return () => {
      mounted = false;
      dispose?.();
    };
  }, []);

  const progress = status ? deriveUploadProgress(status) : null;
  const finished = status?.state === "awaiting_review";

  return (
    <div className="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <section className="flex w-[min(560px,calc(100vw-32px))] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]">
        <header className="flex items-start justify-between border-b border-ink/15 px-7 py-5">
          <div>
            <span className="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">
              {t("agent.eyebrow")}
            </span>
            <h2 className="mt-1 font-serif text-[21px]">{t("agent.importPdf")}</h2>
            <p className="mt-1 max-w-[460px] font-serif text-[10px] leading-[1.5] text-ink/50">
              {t("agent.importPdfHint")}
            </p>
          </div>
          <button className="icon-quiet" onClick={onClose} aria-label={t("menu.close")}>
            <IconX size={18} stroke={1.35} />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto p-6">
          {!status ? (
            <div
              className={`grid place-items-center rounded-[6px] border border-dashed p-8 text-center transition ${
                dragOver ? "border-blue bg-blue-soft" : "border-ink/25 bg-canvas"
              }`}
              onDragEnter={(event) => {
                event.preventDefault();
                setDragOver(true);
              }}
              onDragOver={(event) => event.preventDefault()}
              onDragLeave={() => setDragOver(false)}
              onDrop={(event) => {
                event.preventDefault();
                setDragOver(false);
              }}
            >
              <div>
                <IconFileText className="mx-auto text-ink/35" size={30} stroke={1.2} />
                <h3 className="mt-4 font-serif text-[15px]">{t("agent.dropTitle")}</h3>
                <p className="mx-auto mt-2 max-w-[360px] font-serif text-[10px] leading-[1.55] text-ink/50">
                  {t("agent.dropHint")}
                </p>
                <button
                  className="button-primary mt-5 justify-center"
                  onClick={() => void chooseFile()}
                >
                  <IconUpload size={15} stroke={1.35} />
                  {t("agent.browse")}
                </button>
              </div>
            </div>
          ) : (
            <div className="space-y-5">
              <div className="flex items-center gap-3 rounded-[5px] border border-ink/15 bg-canvas px-4 py-3">
                <IconFileText size={18} stroke={1.3} className="shrink-0 text-blue" />
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-[9px] text-ink/70" title={status.pdfPath}>
                    {status.pdfPath}
                  </p>
                  <p className="mt-0.5 font-serif text-[9px] text-ink/45">
                    {status.jobId.slice(0, 12)} · {t("agent.fileSelected")}
                  </p>
                </div>
                {finished && <IconCheck size={18} stroke={1.6} className="text-olive" />}
              </div>

              <div>
                <div className="flex items-baseline justify-between">
                  <p className="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
                    {t("agent.stage")} · {AGENT_STAGE_LABELS[status.state]}
                  </p>
                  {progress && (
                    <p className="font-serif text-[9px] text-ink/50">
                      {t("agent.stageOf", { done: progress.done, total: progress.total })}
                    </p>
                  )}
                </div>
                <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-ink/10">
                  <div
                    className={`h-full rounded-full transition-all duration-500 ${error ? "bg-alert" : finished ? "bg-olive" : "bg-blue"}`}
                    style={{ width: `${progress?.percent ?? 0}%` }}
                  />
                </div>
              </div>

              <ol className="space-y-1">
                {PDF_PIPELINE_STAGES.map((stage) => {
                  const index = jobStageIndex(stage);
                  const current = jobStageIndex(status.state);
                  const done = current > index || finished;
                  const active = current === index && !finished && !error;
                  return (
                    <li
                      key={stage}
                      className={`flex items-center gap-2 rounded-[3px] px-2 py-1 font-serif text-[10px] ${
                        active
                          ? "bg-blue-soft text-blue"
                          : done
                            ? "text-olive"
                            : "text-ink/40"
                      }`}
                    >
                      <span
                        className={`grid size-4 shrink-0 place-items-center rounded-full border text-[8px] ${
                          done
                            ? "border-olive/40 bg-olive/10"
                            : active
                              ? "border-blue bg-blue-soft"
                              : "border-ink/20"
                        }`}
                      >
                        {done && !active ? <IconCheck size={9} stroke={2} /> : index + 1}
                      </span>
                      {AGENT_STAGE_LABELS[stage]}
                    </li>
                  );
                })}
              </ol>

              {error && (
                <p className="flex items-start gap-2 rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-serif text-[10px] leading-[1.5] text-alert">
                  <IconAlertTriangle size={14} stroke={1.5} className="mt-0.5 shrink-0" />
                  <span className="min-w-0 break-words">{error}</span>
                </p>
              )}
            </div>
          )}
        </div>

        <footer className="flex shrink-0 justify-end gap-2 border-t border-ink/15 px-6 py-4">
          {status ? (
            <>
              <button
                className="button-secondary"
                onClick={() => {
                  stopPolling();
                  setStatus(null);
                  setPdfPath(null);
                  setError("");
                  setStarting(false);
                }}
              >
                {t("agent.retry")}
              </button>
              <button className="button-primary" disabled={finished} onClick={onClose}>
                {finished ? t("agent.awaitingReview") : t("agent.cancel")}
              </button>
            </>
          ) : (
            <>
              <button className="button-secondary" onClick={onClose}>
                {t("agent.cancel")}
              </button>
              <button className="button-primary" disabled={!pdfPath || starting} onClick={() => void beginParse()}>
                <IconRefresh className={starting ? "animate-spin" : ""} size={15} stroke={1.35} />
                {starting ? t("agent.starting") : t("agent.start")}
              </button>
            </>
          )}
        </footer>
      </section>
    </div>
  );
}
