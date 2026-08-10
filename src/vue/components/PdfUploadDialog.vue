<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { isJobTerminal } from "../../../app/plugins/agent-contracts";
import type { AgentJobStatus } from "../../../app/plugins/agent-contracts";
import {
  cancelPdfJob,
  getPdfJobStatus,
  listenForPdfDrops,
  pickPdfFile,
  startPdfJob,
} from "../../../app/platform/agent-client";
import {
  PDF_PIPELINE_STAGES,
  deriveUploadProgress,
  jobStageIndex,
  usePanelI18n,
  type PdfUploadDialogProps,
} from "./panel-types";

const props = defineProps<PdfUploadDialogProps>();
const { t } = usePanelI18n();
const pdfPath = ref<string | null>(null);
const status = ref<AgentJobStatus | null>(null);
const error = ref("");
const starting = ref(false);
const dragOver = ref(false);
const pollTimer = ref<number | null>(null);
const jobId = ref<string | null>(null);
const dropped = ref(false);

const progress = computed(() => (status.value ? deriveUploadProgress(status.value) : null));
const finished = computed(() => status.value?.state === "awaiting_review");

const stopPolling = () => {
  if (pollTimer.value !== null) {
    window.clearInterval(pollTimer.value);
    pollTimer.value = null;
  }
};

const watchJob = (nextJobId: string) => {
  stopPolling();
  jobId.value = nextJobId;
  pollTimer.value = window.setInterval(async () => {
    try {
      const next = await getPdfJobStatus(nextJobId);
      status.value = next;
      if (next.state === "awaiting_review") {
        stopPolling();
        props.onReady(nextJobId, next);
      } else if (isJobTerminal(next.state)) {
        stopPolling();
        jobId.value = null;
        error.value = next.error ?? `Unexpected terminal state: ${next.state}`;
      }
    } catch (cause) {
      stopPolling();
      jobId.value = null;
      error.value = cause instanceof Error ? cause.message : String(cause);
    }
  }, 700);
};

const chooseFile = async () => {
  try {
    const path = await pickPdfFile();
    if (path) {
      pdfPath.value = path;
      error.value = "";
    }
  } catch (cause) {
    error.value = cause instanceof Error && cause.message === "DESKTOP_REQUIRED"
      ? t("agent.desktopOnly")
      : cause instanceof Error ? cause.message : String(cause);
  }
};

const beginParse = async () => {
  if (!pdfPath.value || starting.value) return;
  starting.value = true;
  error.value = "";
  dropped.value = false;
  try {
    const job = await startPdfJob(pdfPath.value);
    status.value = job;
    jobId.value = job.jobId;
    if (job.state === "awaiting_review") props.onReady(job.jobId, job);
    else if (isJobTerminal(job.state)) {
      jobId.value = null;
      error.value = job.error ?? `Unexpected terminal state: ${job.state}`;
    } else watchJob(job.jobId);
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    starting.value = false;
  }
};

const reset = () => {
  stopPolling();
  status.value = null;
  pdfPath.value = null;
  jobId.value = null;
  error.value = "";
  starting.value = false;
};

let disposeDropListener: (() => void) | undefined;
onMounted(async () => {
  const cleanup = await listenForPdfDrops((paths) => {
    const first = paths[0];
    if (first) {
      dropped.value = true;
      pdfPath.value = first;
      error.value = "";
    }
  });
  disposeDropListener = cleanup;
});

onUnmounted(() => {
  disposeDropListener?.();
  stopPolling();
  if (jobId.value && status.value && !isJobTerminal(status.value.state)) {
    void cancelPdfJob(jobId.value);
  }
});
</script>

<template>
  <div class="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
    <section class="flex w-[min(560px,calc(100vw-32px))] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]">
      <header class="flex items-start justify-between border-b border-ink/15 px-7 py-5">
        <div><span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">{{ t('agent.eyebrow') }}</span><h2 class="mt-1 font-serif text-[21px]">{{ t('agent.importPdf') }}</h2><p class="mt-1 max-w-[460px] font-serif text-[10px] leading-[1.5] text-ink/50">{{ t('agent.importPdfHint') }}</p></div>
        <button class="icon-quiet" @click="props.onClose" :aria-label="t('menu.close')">×</button>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto p-6">
        <div v-if="!status" class="grid place-items-center rounded-[6px] border border-dashed p-8 text-center transition" :class="dragOver ? 'border-blue bg-blue-soft' : 'border-ink/25 bg-canvas'" @dragenter.prevent="dragOver = true" @dragover.prevent @dragleave.prevent="dragOver = false" @drop.prevent="dragOver = false">
          <div><span class="mx-auto block text-3xl text-ink/35" aria-hidden="true">▧</span><h3 class="mt-4 font-serif text-[15px]">{{ t('agent.dropTitle') }}</h3><p class="mx-auto mt-2 max-w-[360px] font-serif text-[10px] leading-[1.55] text-ink/50">{{ t('agent.dropHint') }}</p><button class="button-primary mt-5 justify-center" @click="void chooseFile()">↑ {{ t('agent.browse') }}</button></div>
        </div>
        <div v-else class="space-y-5">
          <div class="flex items-center gap-3 rounded-[5px] border border-ink/15 bg-canvas px-4 py-3"><span class="shrink-0 text-blue" aria-hidden="true">▧</span><div class="min-w-0 flex-1"><p class="truncate font-mono text-[9px] text-ink/70" :title="status.pdfPath">{{ status.pdfPath }}</p><p class="mt-0.5 font-serif text-[9px] text-ink/45">{{ status.jobId.slice(0, 12) }} · {{ t('agent.fileSelected') }}</p></div><span v-if="finished" class="text-olive" aria-hidden="true">✓</span></div>
          <div><div class="flex items-baseline justify-between"><p class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">{{ t('agent.stage') }} · {{ t(`agent.stage.${status.state}` as any) }}</p><p v-if="progress" class="font-serif text-[9px] text-ink/50">{{ t('agent.stageOf', { done: progress.done, total: progress.total }) }}</p></div><div class="mt-2 h-1.5 overflow-hidden rounded-full bg-ink/10"><div class="h-full rounded-full transition-all duration-500" :class="error ? 'bg-alert' : finished ? 'bg-olive' : 'bg-blue'" :style="{ width: `${progress?.percent ?? 0}%` }" /></div></div>
          <ol class="space-y-1"><li v-for="(stage, index) in PDF_PIPELINE_STAGES" :key="stage" class="flex items-center gap-2 rounded-[3px] px-2 py-1 font-serif text-[10px]" :class="jobStageIndex(status.state) === index && !finished && !error ? 'bg-blue-soft text-blue' : jobStageIndex(status.state) > index || finished ? 'text-olive' : 'text-ink/40'"><span class="grid size-4 shrink-0 place-items-center rounded-full border text-[8px]">{{ jobStageIndex(status.state) > index || finished ? '✓' : index + 1 }}</span>{{ t(`agent.stage.${stage}` as any) }}</li></ol>
          <p v-if="error" class="flex items-start gap-2 rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-serif text-[10px] leading-[1.5] text-alert">⚠ <span class="min-w-0 break-words">{{ error }}</span></p>
        </div>
      </div>

      <footer class="flex shrink-0 justify-end gap-2 border-t border-ink/15 px-6 py-4">
        <template v-if="status"><button class="button-secondary" @click="reset">{{ t('agent.retry') }}</button><button class="button-primary" :disabled="finished" @click="props.onClose">{{ finished ? t('agent.awaitingReview') : t('agent.cancel') }}</button></template>
        <template v-else><button class="button-secondary" @click="props.onClose">{{ t('agent.cancel') }}</button><button class="button-primary" :disabled="!pdfPath || starting" @click="void beginParse()">↻ {{ starting ? t('agent.starting') : t('agent.start') }}</button></template>
      </footer>
    </section>
  </div>
</template>

<style scoped>
/* Shared dialog visual tokens remain in app/globals.css. */
</style>
