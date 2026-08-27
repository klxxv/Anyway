import { h, type VNodeChild } from "vue";
import type {
  HostSurfaceAction,
  HostSurfaceModel,
} from "../../../../app/plugins/plugin-surface-contract";

export type HostSurfaceProvider = {
  readonly getModel: () => HostSurfaceModel;
  readonly dispatch: (action: HostSurfaceAction) => void | Promise<void>;
};

function button(label: string, onClick: () => void, disabled = false, className = "button-secondary") {
  return h("button", { type: "button", class: className, disabled, onClick }, label);
}

function renderFilePicker(provider: HostSurfaceProvider): VNodeChild {
  const model = provider.getModel();
  return h("section", { class: "host-surface host-file-picker" }, [
    h("div", { class: "grid place-items-center rounded-[6px] border border-dashed border-ink/25 bg-canvas p-6 text-center" }, [
      h("p", { class: "font-serif text-[12px] text-ink/65" }, "Choose or drop files"),
      h("p", { class: "mt-1 font-serif text-[9px] text-ink/45" }, "The Host owns native paths and validation."),
      button("Add files", () => void provider.dispatch({ type: "file.pick" }), false, "button-primary mt-4"),
    ]),
    model.files.length === 0
      ? h("p", { class: "mt-3 font-serif text-[9px] text-ink/45" }, "No pending files.")
      : h("div", { class: "mt-3 space-y-2" }, model.files.map((file) => h("div", { key: file.id, class: "flex items-center gap-3 rounded-[5px] border border-ink/15 bg-canvas px-4 py-3" }, [
        h("div", { class: "min-w-0 flex-1" }, [
          h("p", { class: "truncate font-mono text-[9px] text-ink/70" }, file.label),
          file.summary ? h("p", { class: "mt-0.5 truncate font-serif text-[8px] text-ink/40" }, file.summary) : null,
          file.error ? h("p", { class: "mt-1 break-words font-serif text-[8px] text-alert" }, file.error) : null,
        ]),
        button("Remove", () => void provider.dispatch({ type: "file.remove", fileId: file.id }), false, "icon-quiet"),
      ]))),
    model.globalError ? h("p", { class: "mt-3 rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-mono text-[9px] text-alert" }, model.globalError) : null,
  ]);
}

function renderTransfer(provider: HostSurfaceProvider): VNodeChild {
  const jobs = provider.getModel().jobs;
  return h("section", { class: "host-surface host-transfer" }, [
    h("h3", { class: "font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45" }, "Transfer status"),
    jobs.length === 0 ? h("p", { class: "mt-2 font-serif text-[9px] text-ink/45" }, "No queued work.") : h("div", { class: "mt-2 space-y-2" }, jobs.map((job) => h("article", { key: job.id, class: "rounded-[5px] border border-ink/15 bg-canvas px-4 py-3" }, [
      h("div", { class: "flex items-center justify-between gap-3" }, [h("span", { class: "truncate font-mono text-[9px] text-ink/70" }, job.label), h("span", { class: "font-mono text-[8px] text-ink/45" }, job.state)]),
      job.progress !== undefined ? h("div", { class: "mt-2 h-1.5 overflow-hidden rounded-full bg-ink/10" }, [h("div", { class: "h-full rounded-full bg-blue", style: { width: `${Math.max(0, Math.min(100, job.progress))}%` } })]) : null,
      job.transfer ? h("p", { class: "mt-1 font-mono text-[8px] text-ink/45" }, `${job.transfer.done} / ${job.transfer.total}`) : null,
      job.error ? h("p", { class: "mt-1 break-words font-serif text-[8px] text-alert" }, job.error) : null,
      h("div", { class: "mt-2 flex gap-2" }, [
        ["awaiting review", "awaiting_review", "review"].includes(job.state) ? button("Open", () => void provider.dispatch({ type: "job.open", jobId: job.id }), false, "button-primary") : null,
        button("Retry", () => void provider.dispatch({ type: "job.retry", jobId: job.id }), false),
        button("Cancel", () => void provider.dispatch({ type: "job.cancel", jobId: job.id }), false),
      ]),
    ])))
  ]);
}

function renderJobs(provider: HostSurfaceProvider): VNodeChild {
  const model = provider.getModel();
  const counts = (state: string) => model.jobs.filter((job) => job.state === state).length;
  return h("section", { class: "host-surface host-job-list" }, [
    h("div", { class: "grid grid-cols-4 gap-2 font-mono text-[8px] text-ink/55" }, [h("span", `Queued ${counts("queued")}`), h("span", `Running ${counts("running")}`), h("span", `Completed ${counts("completed")}`), h("span", `Failed ${counts("failed")}`)]),
    model.jobs.length === 0 ? h("p", { class: "mt-2 font-serif text-[9px] text-ink/45" }, "No jobs.") : h("ul", { class: "mt-2 space-y-1" }, model.jobs.map((job) => h("li", { key: job.id }, [h("button", { type: "button", class: ["w-full rounded-[4px] border px-3 py-2 text-left font-mono text-[8px]", model.selectedJobId === job.id ? "border-blue bg-blue-soft" : "border-ink/12 bg-canvas"], onClick: () => void provider.dispatch({ type: "job.select", jobId: job.id }) }, `${job.label} · ${job.state}`)]))),
  ]);
}

function renderJobSelection(provider: HostSurfaceProvider): VNodeChild {
  const model = provider.getModel();
  const job = model.jobs.find((candidate) => candidate.id === model.selectedJobId);
  return h("section", { class: "host-surface host-job-selection" }, [
    h("h3", { class: "font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45" }, "Selected work"),
    job ? h("div", { class: "mt-2 rounded-[5px] border border-ink/15 bg-canvas px-3 py-2" }, [h("p", { class: "font-mono text-[9px] text-ink/65" }, job.label), h("p", { class: "mt-1 font-serif text-[9px] text-ink/50" }, job.error ?? job.state), h("div", { class: "mt-2 flex gap-2" }, [button("Cancel", () => void provider.dispatch({ type: "job.cancel", jobId: job.id })), button("Retry", () => void provider.dispatch({ type: "job.retry", jobId: job.id }))])]) : h("p", { class: "mt-2 font-serif text-[9px] text-ink/45" }, "Select a job."),
  ]);
}

function renderEvents(provider: HostSurfaceProvider): VNodeChild {
  const model = provider.getModel();
  const jobs = model.jobs;
  const selected = model.selectedJobId;
  const job = jobs.find((candidate) => candidate.id === selected) ?? jobs[0];
  const all = model.publicEvents;
  const start = Math.max(0, all.length - 50);
  const events = all.slice(start);
  return h("section", { class: "host-surface host-public-events" }, [
    h("div", { class: "flex items-center justify-between" }, [h("h3", { class: "font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45" }, "Public events"), h("span", { class: "font-mono text-[8px] text-ink/45" }, job?.state ?? "disconnected")]),
    events.length === 0 ? h("p", { class: "mt-2 font-serif text-[9px] text-ink/45" }, job ? "Waiting for public events." : "No public event stream.") : h("ol", { class: "mt-2 max-h-48 space-y-1 overflow-y-auto", "aria-live": "polite" }, events.map((event, visibleIndex) => {
      const originalIndex = start + visibleIndex;
      const stablePart = event.id ?? (event.sequence !== undefined ? String(event.sequence) : `${event.createdAt ?? "unknown"}:${originalIndex}`);
      return h("li", { key: `${job?.id ?? "stream"}:${stablePart}`, class: "rounded-[4px] border border-olive/20 bg-olive/5 px-2 py-1.5" }, [
        h("div", { class: "flex justify-between gap-2 font-mono text-[8px] text-olive" }, [h("span", `#${originalIndex + 1} · ${event.phase ?? "event"} · ${event.status ?? "published"}`), h("time", event.createdAt ? new Date(event.createdAt).toLocaleTimeString() : "--:--")]),
        event.summary ? h("p", { class: "mt-1 font-serif text-[9px] text-ink/60" }, event.summary) : null,
        h("p", { class: "mt-1 font-mono text-[8px] text-ink/40" }, `evidence ${event.evidenceCount ?? 0} · warnings ${event.warningCount ?? 0}`),
      ]);
    })),
  ]);
}

function renderErrors(provider: HostSurfaceProvider): VNodeChild {
  const model = provider.getModel();
  return h("section", { class: "host-surface host-error-diagnostics" }, [
    h("h3", { class: "font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45" }, "Diagnostics"),
    model.globalError ? h("p", { class: "mt-2 break-words font-mono text-[8px] text-alert" }, model.globalError) : null,
    model.errors.length === 0 ? h("p", { class: "mt-2 font-serif text-[9px] text-ink/45" }, "No active errors.") : h("ul", { class: "mt-2 space-y-1" }, model.errors.map((error) => h("li", { key: error.id, class: "rounded-[4px] border border-alert/25 bg-alert/5 px-3 py-2" }, [h("p", { class: "font-mono text-[8px] text-alert" }, `${error.code} · ${error.stage ?? "unknown"}`), h("p", { class: "mt-1 break-words font-serif text-[9px] text-ink/60" }, error.message), h("div", { class: "mt-2 flex gap-2" }, [error.retryable ? button("Retry", () => void provider.dispatch({ type: "job.retry", jobId: error.jobId })) : null, button("Cancel", () => void provider.dispatch({ type: "job.cancel", jobId: error.jobId }))])]))),
  ]);
}

function renderReview(provider: HostSurfaceProvider): VNodeChild {
  const items = provider.getModel().reviewItems;
  return h("section", { class: "host-surface host-review-collection space-y-2" }, items.length === 0 ? [h("p", { class: "font-serif text-[10px] text-ink/45" }, "No review items.")] : items.map((item) => h("article", { key: item.id, class: "rounded-[5px] border border-ink/15 bg-canvas p-3" }, [
    h("div", { class: "flex items-start justify-between gap-3" }, [h("div", { class: "min-w-0" }, [h("p", { class: "font-serif text-[12px]" }, item.title), item.summary ? h("p", { class: "mt-1 font-serif text-[9px] text-ink/55" }, item.summary) : null]), h("span", { class: "font-mono text-[8px] text-ink/45" }, item.state ?? "pending")]),
    h("div", { class: "mt-2 flex gap-2" }, [button("Accept", () => void provider.dispatch({ type: "review.set-decision", itemId: item.id, decision: "accepted" })), button("Reject", () => void provider.dispatch({ type: "review.set-decision", itemId: item.id, decision: "rejected" }))]),
  ])));
}

export function createHostSlotRenderers(provider: HostSurfaceProvider): Readonly<Record<string, () => VNodeChild>> {
  return {
    "host.file-picker": () => renderFilePicker(provider),
    "host.blob-transfer": () => renderTransfer(provider),
    "host.job-list": () => renderJobs(provider),
    "host.job-selection": () => renderJobSelection(provider),
    "host.public-event-stream": () => renderEvents(provider),
    "host.error-diagnostics": () => renderErrors(provider),
    "host.review-collection": () => renderReview(provider),
  };
}
