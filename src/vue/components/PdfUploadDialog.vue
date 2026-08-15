<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import {
  isJobTerminal,
  type AgentJobStatus,
  type PublicProgressEvent,
} from "../../../app/plugins/agent-contracts";
import {
  canvasDiffDocumentFromAgentResult,
  computeCanvasDiffBatch,
} from "../../../app/domain/canvas-diff";
import {
  cancelPdfJob,
  getPdfJobStatus,
  listImportJobs,
  listenForDocumentDrops,
  pickImportFiles,
  startDocumentBatch,
} from "../../../app/platform/agent-client";
import { deriveUploadProgress, usePanelI18n, type PdfUploadDialogProps } from "./panel-types";

type ImportListItem = {
  key: string;
  path: string;
  job: AgentJobStatus | null;
  error: string;
};

const props = defineProps<PdfUploadDialogProps>();
const { t } = usePanelI18n();
const items = ref<ImportListItem[]>([]);
const picking = ref(false);
const starting = ref(false);
const dragOver = ref(false);
const pollTimer = ref<number | null>(null);
const globalError = ref("");
const comparing = ref(false);

const pendingItems = computed(() => items.value.filter((item) => !item.job));
const submittedItems = computed(() => items.value.filter((item) => item.job));
const hasActiveJobs = computed(() =>
  submittedItems.value.some(
    (item) => item.job && item.job.state !== "awaiting_review" && !isJobTerminal(item.job.state),
  ),
);
const reviewableItems = computed(() =>
  submittedItems.value.filter((item) => item.job?.state === "awaiting_review" && item.job.result),
);

function normalizedPath(path: string): string {
  return path.replaceAll("\\", "/").toLocaleLowerCase();
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

function appendPaths(paths: string[]) {
  const existing = new Set(items.value.map((item) => normalizedPath(item.path)));
  for (const path of paths) {
    if (!/\.(pdf|docx|md)$/i.test(path) || existing.has(normalizedPath(path))) continue;
    existing.add(normalizedPath(path));
    items.value.push({ key: crypto.randomUUID(), path, job: null, error: "" });
  }
  globalError.value = "";
}

async function chooseFiles() {
  picking.value = true;
  try {
    appendPaths(await pickImportFiles());
  } catch (cause) {
    globalError.value =
      cause instanceof Error && cause.message === "DESKTOP_REQUIRED"
        ? t("agent.desktopOnly")
        : cause instanceof Error
          ? cause.message
          : String(cause);
  } finally {
    picking.value = false;
  }
}

function mergeStatuses(statuses: AgentJobStatus[]) {
  for (const status of statuses) {
    const byJob = items.value.find((item) => item.job?.jobId === status.jobId);
    if (byJob) {
      byJob.job = status;
      byJob.error = status.error ?? "";
      continue;
    }
    items.value.push({
      key: status.jobId,
      path: status.filePath || status.pdfPath,
      job: status,
      error: status.error ?? "",
    });
  }
}

async function refreshActiveJobs() {
  const active = submittedItems.value.filter(
    (item) => item.job && item.job.state !== "awaiting_review" && !isJobTerminal(item.job.state),
  );
  if (active.length === 0) return;
  const results = await Promise.allSettled(
    active.map((item) => getPdfJobStatus(item.job!.jobId)),
  );
  results.forEach((result, index) => {
    const item = active[index];
    if (result.status === "fulfilled") {
      item.job = result.value;
      item.error = result.value.error ?? "";
    } else {
      item.error = result.reason instanceof Error ? result.reason.message : String(result.reason);
    }
  });
}

async function startPending() {
  const pending = pendingItems.value.slice();
  if (pending.length === 0 || starting.value) return;
  starting.value = true;
  globalError.value = "";
  try {
    const batch = await startDocumentBatch(pending.map((item) => item.path));
    pending.forEach((item, index) => {
      item.job = batch.jobs[index] ?? null;
      item.error = item.job?.error ?? "";
    });
  } catch (cause) {
    globalError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    starting.value = false;
  }
}

async function cancelItem(item: ImportListItem) {
  if (!item.job || isJobTerminal(item.job.state) || item.job.state === "awaiting_review") return;
  try {
    item.job = await cancelPdfJob(item.job.jobId);
    item.error = item.job.error ?? "";
  } catch (cause) {
    item.error = cause instanceof Error ? cause.message : String(cause);
  }
}

async function retryItem(item: ImportListItem) {
  item.error = "";
  try {
    const batch = await startDocumentBatch([item.path]);
    item.job = batch.jobs[0] ?? null;
  } catch (cause) {
    item.error = cause instanceof Error ? cause.message : String(cause);
  }
}

async function compareReviewableCanvases() {
  if (reviewableItems.value.length < 2 || comparing.value) return;
  comparing.value = true;
  globalError.value = "";
  try {
    const [baselineItem, ...comparisonItems] = reviewableItems.value;
    const baselineStatus = baselineItem.job!;
    const baselinePath = baselineStatus.filePath || baselineStatus.pdfPath;
    const baselineProject = canvasDiffDocumentFromAgentResult(baselineStatus.result, {
      documentId: baselineStatus.fileHash || baselineStatus.jobId,
      provenance: { origin: "ai", fileName: fileName(baselinePath), sourcePath: baselinePath },
    });
    if (!baselineProject) throw new Error(t("agent.diffUnavailable"));
    const baselineDocuments = [];
    const comparisonDocuments = [];
    for (const item of comparisonItems) {
      const status = item.job!;
      const path = status.filePath || status.pdfPath;
      const pairId = status.fileHash || status.jobId;
      const comparison = canvasDiffDocumentFromAgentResult(status.result, {
        documentId: pairId,
        provenance: { origin: "ai", fileName: fileName(path), sourcePath: path },
      });
      if (!comparison) continue;
      baselineDocuments.push({
        ...baselineProject,
        documentId: pairId,
        provenance: {
          ...baselineProject.provenance,
          documentId: pairId,
          fileName: fileName(baselinePath),
          sourcePath: baselinePath,
        },
      });
      comparisonDocuments.push(comparison);
    }
    if (!comparisonDocuments.length) throw new Error(t("agent.diffUnavailable"));
    const result = await computeCanvasDiffBatch({
      baseline: { groupId: baselineStatus.jobId, label: fileName(baselinePath), documents: baselineDocuments },
      comparison: { groupId: "import-batch", label: t("agent.diffComparisonGroup"), documents: comparisonDocuments },
    });
    props.onDiffReady?.(result);
  } catch (cause) {
    globalError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    comparing.value = false;
  }
}

function removePending(item: ImportListItem) {
  if (item.job) return;
  items.value = items.value.filter((candidate) => candidate.key !== item.key);
}

function progressFor(item: ImportListItem) {
  return item.job ? deriveUploadProgress(item.job) : { done: 0, total: 0, percent: 0 };
}

function formatActivityBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function uploadPercent(job: AgentJobStatus): number {
  return job.uploadTotalBytes
    ? Math.min(100, Math.round((job.uploadBytes / job.uploadTotalBytes) * 100))
    : 0;
}

function formatElapsed(milliseconds: number): string {
  return `${Math.max(0, Math.round(milliseconds / 1000))}s`;
}

function latestPublicProgress(job: AgentJobStatus): PublicProgressEvent | null {
  const events = job.publicProgress;
  return events && events.length > 0 ? events[events.length - 1] ?? null : null;
}

function latestRepairAudit(job: AgentJobStatus) {
  const records = job.repairAudit;
  return records && records.length > 0 ? records[records.length - 1] ?? null : null;
}

function repairEntryCount(job: AgentJobStatus): number {
  return (job.repairAudit ?? []).reduce((total, record) => total + record.entries.length, 0);
}

let disposeDropListener: (() => void) | undefined;
onMounted(async () => {
  try {
    mergeStatuses(await listImportJobs());
  } catch {
    // A fresh host has no import history; selection remains available.
  }
  disposeDropListener = await listenForDocumentDrops(appendPaths);
  pollTimer.value = window.setInterval(() => void refreshActiveJobs(), 700);
});

onUnmounted(() => {
  disposeDropListener?.();
  if (pollTimer.value !== null) window.clearInterval(pollTimer.value);
  // Jobs are host-owned and intentionally continue after the dialog closes.
});
</script>

<template>
  <div class="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
    <section class="flex max-h-[min(760px,calc(100vh-32px))] w-[min(680px,calc(100vw-32px))] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]">
      <header class="flex items-start justify-between border-b border-ink/15 px-7 py-5">
        <div>
          <span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">{{ t('agent.eyebrow') }}</span>
          <h2 class="mt-1 font-serif text-[21px]">{{ t('agent.importDocuments') }}</h2>
          <p class="mt-1 max-w-[520px] font-serif text-[10px] leading-[1.5] text-ink/50">{{ t('agent.importDocumentsHint') }}</p>
        </div>
        <button class="icon-quiet" :aria-label="t('menu.close')" @click="props.onClose">×</button>
      </header>

      <div class="min-h-0 flex-1 space-y-5 overflow-y-auto p-6">
        <div
          class="grid place-items-center rounded-[6px] border border-dashed p-6 text-center transition"
          :class="dragOver ? 'border-blue bg-blue-soft' : 'border-ink/25 bg-canvas'"
          @dragenter.prevent="dragOver = true"
          @dragover.prevent
          @dragleave.prevent="dragOver = false"
          @drop.prevent="dragOver = false"
        >
          <p class="font-serif text-[12px] text-ink/65">{{ t('agent.dropDocuments') }}</p>
          <p class="mt-1 font-serif text-[9px] text-ink/45">{{ t('agent.allowedDocuments') }}</p>
          <button class="button-primary mt-4" :disabled="picking" @click="void chooseFiles()">
            ＋ {{ picking ? t('agent.choosingFiles') : t('agent.addFiles') }}
          </button>
        </div>

        <section v-if="pendingItems.length" class="space-y-2">
          <div class="flex items-center justify-between">
            <h3 class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">{{ t('agent.pendingFiles') }}</h3>
            <span class="font-mono text-[9px] text-ink/40">{{ pendingItems.length }}</span>
          </div>
          <div v-for="item in pendingItems" :key="item.key" class="flex items-center gap-3 rounded-[5px] border border-ink/15 bg-canvas px-4 py-3">
            <span class="text-blue" aria-hidden="true">◇</span>
            <div class="min-w-0 flex-1">
              <p class="truncate font-mono text-[9px] text-ink/70" :title="item.path">{{ fileName(item.path) }}</p>
              <p class="mt-0.5 truncate font-serif text-[8px] text-ink/40">{{ item.path }}</p>
            </div>
            <button class="icon-quiet" :aria-label="t('agent.removeFile')" @click="removePending(item)">×</button>
          </div>
        </section>

        <section v-if="submittedItems.length" class="space-y-2">
          <div class="flex items-center justify-between">
            <h3 class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">{{ t('agent.importedFiles') }}</h3>
            <span class="font-mono text-[9px] text-ink/40">{{ submittedItems.length }}</span>
          </div>
          <article v-for="item in submittedItems" :key="item.key" class="rounded-[5px] border border-ink/15 bg-canvas px-4 py-3">
            <div class="flex items-start gap-3">
              <span class="mt-0.5 text-blue" aria-hidden="true">▧</span>
              <div class="min-w-0 flex-1">
                <div class="flex items-center justify-between gap-3">
                  <p class="truncate font-mono text-[9px] text-ink/70" :title="item.path">{{ fileName(item.path) }}</p>
                  <span class="shrink-0 font-sans text-[8px] uppercase tracking-[0.12em] text-ink/45">{{ t(`agent.stage.${item.job!.state}` as any) }}</span>
                </div>
                <div class="mt-2 h-1.5 overflow-hidden rounded-full bg-ink/10">
                  <div
                    class="h-full rounded-full transition-all duration-500"
                    :class="item.error ? 'bg-alert' : item.job!.state === 'awaiting_review' ? 'bg-olive' : 'bg-blue'"
                    :style="{ width: `${progressFor(item).percent}%` }"
                  />
                </div>
                <div v-if="item.job!.uploadTotalBytes" class="mt-2">
                  <div class="flex justify-between font-mono text-[8px] text-ink/45">
                    <span>{{ t('agent.uploadProgress') }}</span>
                    <span>{{ formatActivityBytes(item.job!.uploadBytes) }} / {{ formatActivityBytes(item.job!.uploadTotalBytes!) }}</span>
                  </div>
                  <div class="mt-1 h-1 overflow-hidden rounded-full bg-ink/10">
                    <div class="h-full rounded-full bg-blue transition-all duration-200" :style="{ width: `${uploadPercent(item.job!)}%` }" />
                  </div>
                </div>
                <div
                  v-if="item.job!.reasoningActivity?.currentPass"
                  class="mt-2 rounded-[4px] border border-blue/15 bg-blue-soft/40 px-2.5 py-2"
                >
                  <div class="flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[8px] text-ink/55">
                    <span>{{ item.job!.reasoningActivity.currentPass }}</span>
                    <span>{{ t('agent.reasoningChunks', { count: item.job!.reasoningActivity.chunkCount }) }}</span>
                    <span>{{ formatActivityBytes(item.job!.reasoningActivity.bytes) }}</span>
                    <span>{{ formatElapsed(item.job!.reasoningActivity.elapsedMs) }}</span>
                    <span v-if="item.job!.reasoningActivity.retryCount">{{ t('agent.reasoningRetries', { count: item.job!.reasoningActivity.retryCount }) }}</span>
                  </div>
                  <p v-if="item.job!.reasoningActivity.safeSummary" class="mt-1 font-serif text-[9px] text-ink/50">
                    {{ item.job!.reasoningActivity.safeSummary }}
                  </p>
                  <p class="mt-1 font-serif text-[8px] text-ink/35">{{ t('agent.reasoningPrivacy') }}</p>
                </div>
                <div
                  v-if="latestPublicProgress(item.job!)"
                  class="mt-2 rounded-[4px] border border-olive/25 bg-olive/5 px-2.5 py-2"
                  data-testid="public-progress"
                  aria-live="polite"
                >
                  <div class="font-mono text-[8px] uppercase tracking-[0.1em] text-olive">
                    {{ latestPublicProgress(item.job!)!.stage }}
                  </div>
                  <p class="mt-1 font-serif text-[9px] text-ink/60">
                    {{ latestPublicProgress(item.job!)!.summary }}
                  </p>
                  <div v-if="latestPublicProgress(item.job!)!.evidenceCount !== undefined || latestPublicProgress(item.job!)!.warningCount !== undefined" class="mt-1 flex gap-3 font-mono text-[8px] text-ink/40">
                    <span v-if="latestPublicProgress(item.job!)!.evidenceCount !== undefined">{{ t('agent.publicProgressEvidence', { count: latestPublicProgress(item.job!)!.evidenceCount ?? 0 }) }}</span>
                    <span v-if="latestPublicProgress(item.job!)!.warningCount !== undefined">{{ t('agent.publicProgressWarnings', { count: latestPublicProgress(item.job!)!.warningCount ?? 0 }) }}</span>
                  </div>
                </div>
                <details
                  v-if="latestRepairAudit(item.job!)"
                  class="mt-2 rounded-[4px] border border-ink/15 bg-paper/70 px-2.5 py-2"
                  data-testid="repair-audit"
                >
                  <summary class="cursor-pointer font-mono text-[8px] text-ink/55">
                    {{ t('agent.repairAuditSummary', { count: repairEntryCount(item.job!), status: latestRepairAudit(item.job!)!.status }) }}
                  </summary>
                  <ol class="mt-2 space-y-1">
                    <li v-for="(entry, index) in latestRepairAudit(item.job!)!.entries" :key="`${entry.code}:${entry.path}:${index}`" class="font-mono text-[8px] leading-[1.45] text-ink/50">
                      {{ entry.code }} · {{ entry.path }} · {{ entry.beforeSummary }} → {{ entry.afterSummary }}
                    </li>
                  </ol>
                  <p v-if="latestRepairAudit(item.job!)!.status === 'model-recovered'" class="mt-2 font-serif text-[8px] text-alert/80">
                    {{ t('agent.repairAuditModelRecovery') }}
                  </p>
                </details>
                <p v-if="item.error" class="mt-2 break-words font-serif text-[9px] text-alert">{{ item.error }}</p>
              </div>
              <button
                v-if="item.job!.state === 'awaiting_review'"
                class="button-primary shrink-0"
                @click="props.onReady(item.job!.jobId, item.job!)"
              >{{ t('agent.reviewFile') }}</button>
              <button
                v-else-if="item.job!.state === 'failed' || item.job!.state === 'cancelled'"
                class="button-secondary shrink-0"
                @click="void retryItem(item)"
              >{{ t('agent.retry') }}</button>
              <button
                v-else-if="!isJobTerminal(item.job!.state)"
                class="button-secondary shrink-0"
                @click="void cancelItem(item)"
              >{{ t('agent.cancel') }}</button>
            </div>
          </article>
        </section>

        <p v-if="globalError" class="rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-serif text-[10px] text-alert">{{ globalError }}</p>
        <button
          v-if="reviewableItems.length >= 2 && props.onDiffReady"
          type="button"
          class="button-secondary w-full"
          :disabled="comparing"
          @click="void compareReviewableCanvases()"
        >
          {{ comparing ? t('agent.comparingCanvases') : t('agent.compareCanvases', { count: reviewableItems.length }) }}
        </button>
        <p v-if="hasActiveJobs" class="font-serif text-[9px] text-ink/45">{{ t('agent.serialQueueHint') }}</p>
      </div>

      <footer class="flex shrink-0 items-center justify-between gap-2 border-t border-ink/15 px-6 py-4">
        <span class="font-serif text-[9px] text-ink/40">{{ t('agent.queueCount', { count: items.length }) }}</span>
        <div class="flex gap-2">
          <button class="button-secondary" @click="props.onClose">{{ t('menu.close') }}</button>
          <button class="button-primary" :disabled="pendingItems.length === 0 || starting" @click="void startPending()">
            {{ starting ? t('agent.starting') : t('agent.startQueued') }}
          </button>
        </div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
/* Shared dialog visual tokens remain in app/globals.css. */
</style>
