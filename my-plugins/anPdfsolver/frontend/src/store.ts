import { computed, reactive } from "vue";
import { buildRuntimeConfig } from "./runtime-config";
import {
  getPluginContext,
  trackWorker,
  type CanonicalGraphPatchReview,
  type HostContext,
  type PickedFile,
  type TrackedWorker
} from "./context";

type JobState = "queued" | "running" | "review" | "accepted" | "rejected" | "failed" | "cancelled";

export type PublicFrame = Record<string, unknown> & { type?: string; seq?: number; message?: string };

type AnalysisJob = {
  id: string;
  file: PickedFile;
  state: JobState;
  progress: number;
  stage: string;
  requestId?: string;
  error?: string;
  proposalId?: string;
  digest?: string;
  canonical?: CanonicalGraphPatchReview;
  frames: PublicFrame[];
  events: Array<{ stage: string; message: string; percent: number; createdAt?: number }>;
};

const state = reactive({
  panelOpen: false,
  analysisSessionId: crypto.randomUUID(),
  worker: null as TrackedWorker | null,
  files: [] as PickedFile[],
  jobs: [] as AnalysisJob[],
  selectedJobId: "" as string,
  busy: false,
  lastError: "" as string
});

export const selectedJob = computed(() => state.jobs.find((job) => job.id === state.selectedJobId) ?? state.jobs[0]);
export const queuedCount = computed(() => state.jobs.filter((job) => job.state === "queued").length);
export const runningCount = computed(() => state.jobs.filter((job) => job.state === "running").length);
export const completedCount = computed(() => state.jobs.filter((job) => ["review", "accepted"].includes(job.state)).length);
export const failedCount = computed(() => state.jobs.filter((job) => job.state === "failed").length);

export function useAnalysisStore() {
  return {
    state,
    selectedJob,
    queuedCount,
    runningCount,
    completedCount,
    failedCount,
    openPanel,
    closePanel,
    pickFiles,
    startBatch,
    retryFailed,
    cancelSelected,
    selectJob,
    acceptReview,
    rejectReview
  };
}

function openPanel(): void {
  state.panelOpen = true;
}

function closePanel(): void {
  state.panelOpen = false;
}

async function pickFiles(): Promise<void> {
  const context = getPluginContext();
  state.lastError = "";
  const picked = await context.files.pick({ accept: ["application/pdf"], multiple: true, retention: "session" });
  for (const file of picked) {
    const id = file.id || file.blobRef.digest.slice(0, 16);
    if (state.jobs.some((job) => job.id === id)) continue;
    state.files.push(file);
    state.jobs.push({
      id,
      file,
      state: "queued",
      progress: 0,
      stage: "queued",
      frames: [],
      events: []
    });
  }
  state.selectedJobId ||= state.jobs[0]?.id || "";
}

async function startBatch(hostContext: HostContext): Promise<void> {
  const targets = state.jobs.filter((job) => job.state === "queued");
  if (!targets.length) return;
  state.busy = true;
  state.lastError = "";
  try {
    for (const job of targets) {
      if (job.state !== "queued") continue;
      await runJob(job, hostContext);
    }
  } finally {
    state.busy = false;
  }
}

async function retryFailed(hostContext: HostContext): Promise<void> {
  for (const job of state.jobs.filter((item) => item.state === "failed")) {
    job.state = "queued";
    job.error = undefined;
    job.progress = 0;
    job.stage = "queued";
  }
  await startBatch(hostContext);
}

async function cancelSelected(): Promise<void> {
  const job = selectedJob.value;
  if (!job) return;
  job.state = "cancelled";
  job.stage = "cancelled";
  if (state.worker) {
    await getPluginContext().worker.cancel(state.worker.handle, job.requestId || job.id);
  }
}

function selectJob(id: string): void {
  state.selectedJobId = id;
}

async function acceptReview(hostContext: HostContext): Promise<void> {
  const job = selectedJob.value;
  if (!job?.proposalId || !job.digest) return;
  const context = getPluginContext();
  try {
    const binding = projectBinding(hostContext);
    await context.graphPatch.review({
      proposalId: job.proposalId,
      expectedDigest: job.digest,
      sessionId: state.analysisSessionId,
      projectId: binding.projectId,
      baseRevision: job.canonical?.baseRevision ?? binding.baseRevision,
      accept: true
    });
    job.state = "accepted";
  } catch (error) {
    state.lastError = error instanceof Error ? error.message : String(error);
  }
}

async function rejectReview(hostContext: HostContext): Promise<void> {
  const job = selectedJob.value;
  if (!job?.proposalId) return;
  if (!job.digest) throw new Error("Canonical digest is missing.");
  try {
    const binding = projectBinding(hostContext);
    await getPluginContext().graphPatch.review({
      proposalId: job.proposalId,
      expectedDigest: job.digest,
      sessionId: state.analysisSessionId,
      projectId: binding.projectId,
      baseRevision: job.canonical?.baseRevision ?? binding.baseRevision,
      accept: false
    });
    job.state = "rejected";
  } catch (error) {
    state.lastError = error instanceof Error ? error.message : String(error);
  }
}

async function ensureWorker(): Promise<TrackedWorker> {
  if (state.worker) return state.worker;
  const workerId = "python-analyzer";
  const handle = await getPluginContext().worker.open(workerId, {
    sessionId: state.analysisSessionId,
    deadlineMs: 120_000
  });
  state.worker = trackWorker({ workerId, handle, sessionId: state.analysisSessionId });
  return state.worker;
}

async function runJob(job: AnalysisJob, hostContext: HostContext): Promise<void> {
  const context = getPluginContext();
  state.selectedJobId = job.id;
  job.state = "running";
  job.progress = 5;
  job.stage = "starting";
  job.requestId = makeRequestId(job.id);
  job.frames = [];
  job.events = [];
  try {
    const worker = await ensureWorker();
    const binding = projectBinding(hostContext);
    const result = await context.worker.call<{
      analysisSessionId: string;
      requestId: string;
      jobId: string;
      progress?: AnalysisJob["events"];
      frames?: PublicFrame[];
      draftPatch: Record<string, unknown>;
      summary?: string;
    }>(worker.handle, {
      requestId: job.requestId,
      operation: "anpdfsolver.analyze",
      deadlineMs: 120_000,
      payload: {
        analysisSessionId: state.analysisSessionId,
        requestId: job.requestId,
        jobId: job.id,
        projectId: binding.projectId,
        baseRevision: binding.baseRevision,
        file: {
          label: job.file.label,
          blobRef: job.file.blobRef
        },
        runtimeConfig: await buildRuntimeConfig()
      }
    });
    if (result.analysisSessionId !== state.analysisSessionId || result.jobId !== job.id) {
      throw new Error("Worker returned a mismatched analysis session.");
    }
    if (result.requestId !== job.requestId) {
      throw new Error("Worker returned a mismatched request id.");
    }
    job.events = result.progress || [];
    job.frames = result.frames || [];
    job.progress = 90;
    job.stage = "graph.patch.propose";
    const proposal = await context.graphPatch.propose({
      sessionId: state.analysisSessionId,
      projectId: binding.projectId,
      baseRevision: binding.baseRevision,
      patch: result.draftPatch
    });
    if (!proposal.digest) {
      throw new Error("Rust GraphPatch proposal did not return a canonical digest.");
    }
    job.proposalId = proposal.proposalId;
    job.digest = proposal.digest;
    job.stage = "graph.patch.get";
    const canonical = await context.graphPatch.get({
      sessionId: state.analysisSessionId,
      projectId: binding.projectId,
      baseRevision: binding.baseRevision,
      proposalId: proposal.proposalId,
      expectedDigest: proposal.digest
    });
    job.canonical = canonical.review
      ? {
          ...canonical,
          title: canonical.review.title,
          summary: canonical.review.summary,
          operations: canonical.review.operations
        }
      : canonical;
    job.digest = canonical.digest || proposal.digest;
    job.progress = 100;
    job.state = "review";
    job.stage = "review";
  } catch (error) {
    job.state = "failed";
    job.progress = 0;
    job.stage = "failed";
    job.error = error instanceof Error ? error.message : String(error);
    state.lastError = job.error;
  }
}

function projectBinding(hostContext: HostContext): { projectId: string; baseRevision: number } {
  const projectId = hostContext.workspace.projectId;
  const baseRevision = hostContext.workspace.baseRevision;
  if (!projectId || typeof baseRevision !== "number" || !Number.isFinite(baseRevision)) {
    throw new Error("Host project binding is required before proposing a GraphPatch.");
  }
  return { projectId, baseRevision };
}

function makeRequestId(jobId: string): string {
  return `anpdfsolver-${jobId}-${crypto.randomUUID()}`;
}
