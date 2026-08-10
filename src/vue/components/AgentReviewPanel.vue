<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { MessageKey } from "../../../app/i18n/catalog";
import { isJobTerminal, type AgentJobStage } from "../../../app/plugins/agent-contracts";
import { normalizePluginGraphPatch } from "../../../app/plugins/workspace";
import { getPdfJobStatus, reviewPdfPatch } from "../../../app/platform/agent-client";
import {
  buildAcceptedPatch,
  countAccepted,
  operationSubject,
  usePanelI18n,
  type AgentReviewPanelProps,
  type ReviewDecision,
} from "./panel-types";

const props = defineProps<AgentReviewPanelProps>();
const { t } = usePanelI18n();
const status = ref<Awaited<ReturnType<typeof getPdfJobStatus>> | null>(null);
const decisions = ref<Record<number, ReviewDecision>>({});
const error = ref("");
const applying = ref(false);

const patch = computed(() => {
  if (!status.value?.result) return null;
  return normalizePluginGraphPatch(status.value.result);
});
const acceptedCount = computed(() =>
  patch.value ? countAccepted(decisions.value, patch.value.operations.length) : 0,
);
const settled = computed(() => (status.value ? isJobTerminal(status.value.state) : false));
const counts = computed(() => {
  const result: Record<string, number> = {};
  for (const operation of patch.value?.operations ?? []) {
    result[operation.op] = (result[operation.op] ?? 0) + 1;
  }
  return result;
});

const stageLabel = (stage: AgentJobStage) => t(`agent.stage.${stage}` as MessageKey);
const opLabel = (op: string) => {
  const key: Record<string, MessageKey> = {
    "add-node": "agent.op.addNode",
    "add-edge": "agent.op.addEdge",
    "update-node": "agent.op.updateNode",
    "update-edge": "agent.op.updateEdge",
  };
  return t(key[op] ?? "agent.op.updateNode");
};
const decisionFor = (index: number): ReviewDecision => decisions.value[index] ?? { accept: false };
const setDecision = (index: number, decision: ReviewDecision) => {
  decisions.value = { ...decisions.value, [index]: decision };
};
const setEdit = (index: number, field: string, value: string) => {
  const previous = decisionFor(index);
  setDecision(index, {
    ...previous,
    edits: { ...previous.edits, [field]: value },
  });
};
const acceptAll = () => {
  decisions.value = Object.fromEntries(
    (patch.value?.operations ?? []).map((_, index) => [
      index,
      { accept: true, edits: decisions.value[index]?.edits },
    ]),
  );
};

let requestSerial = 0;
watch(
  () => props.jobId,
  async (jobId) => {
    const serial = ++requestSerial;
    status.value = null;
    decisions.value = {};
    error.value = "";
    try {
      const snapshot = await getPdfJobStatus(jobId);
      if (serial === requestSerial) status.value = snapshot;
    } catch (cause) {
      if (serial === requestSerial) error.value = cause instanceof Error ? cause.message : String(cause);
    }
  },
  { immediate: true },
);

const applySelected = async () => {
  if (!patch.value || applying.value) return;
  const accepted = buildAcceptedPatch(patch.value, decisions.value);
  applying.value = true;
  error.value = "";
  try {
    if (accepted) props.onApply(accepted);
    else props.onReject();
    await reviewPdfPatch(props.jobId, accepted !== null);
  } catch (cause) {
    if (accepted) props.onRollback();
    error.value = cause instanceof Error ? cause.message : String(cause);
    applying.value = false;
  }
};

const rejectAll = async () => {
  if (applying.value) return;
  applying.value = true;
  error.value = "";
  try {
    await reviewPdfPatch(props.jobId, false);
    props.onReject();
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause);
    applying.value = false;
  }
};
</script>

<template>
  <div class="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
    <section class="grid h-[min(680px,calc(100vh-32px))] w-[min(960px,calc(100vw-32px))] grid-cols-1 overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)] md:grid-cols-[minmax(0,1fr)_300px]">
      <div class="flex min-h-0 flex-col border-b border-ink/15 md:border-b-0 md:border-r">
        <header class="flex items-start justify-between border-b border-ink/15 px-7 py-5">
          <div>
            <span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">{{ t('agent.eyebrow') }}</span>
            <h2 class="mt-1 flex items-center gap-2 font-serif text-[21px]"><span aria-hidden="true">✦</span>{{ t('agent.reviewTitle') }}</h2>
            <p class="mt-1 max-w-[560px] font-serif text-[10px] leading-[1.5] text-ink/50">{{ t('agent.reviewSubtitle') }}</p>
          </div>
          <button class="icon-quiet" @click="props.onClose" :aria-label="t('menu.close')">×</button>
        </header>

        <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          <p v-if="!patch" class="grid min-h-[280px] place-items-center rounded-[6px] border border-dashed border-ink/20 bg-canvas px-5 text-center font-serif text-[11px] text-ink/45">
            {{ status ? t('workspace.noPatch') : `${t('agent.starting')} ${props.jobId.slice(0, 12)}…` }}
          </p>
          <div v-else class="space-y-2">
            <article
              v-for="(operation, index) in patch.operations"
              :key="`${operation.op}-${index}`"
              class="rounded-[5px] border p-3 transition"
              :class="decisionFor(index).accept ? 'border-blue/35 bg-blue-soft/50' : 'border-ink/15 bg-canvas opacity-60'"
            >
              <div class="flex items-start gap-3">
                <span class="mt-0.5 grid size-7 shrink-0 place-items-center rounded-full border" :class="decisionFor(index).accept ? 'border-blue/40 bg-blue-soft text-blue' : 'border-ink/20 text-ink/45'">
                  {{ operation.op === 'add-node' ? '✓' : operation.op === 'add-edge' ? '↗' : '✎' }}
                </span>
                <div class="min-w-0 flex-1">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="rounded-[3px] bg-blue-soft px-1.5 py-0.5 font-sans text-[7px] uppercase tracking-[0.1em] text-blue">{{ opLabel(operation.op) }}</span>
                    <span class="rounded-[3px] bg-ink/8 px-1.5 py-0.5 font-sans text-[7px] uppercase tracking-[0.1em] text-ink/55">{{ operation.op === 'add-node' ? operation.node.type : operation.op === 'add-edge' ? operation.edge.type : operation.op }}</span>
                  </div>
                  <p class="mt-1.5 truncate font-serif text-[12px]" :title="operationSubject(operation)">{{ operationSubject(operation) }}</p>
                  <p v-if="operation.op === 'add-node' && operation.node.body" class="mt-1 line-clamp-2 font-serif text-[9px] leading-[1.5] text-ink/50">{{ operation.node.body }}</p>
                  <label v-if="decisionFor(index).accept && operation.op === 'add-node'" class="dialog-field mt-2">{{ t('agent.op.title') }}<input :value="decisionFor(index).edits?.title ?? operation.node.title" @input="setEdit(index, 'title', ($event.target as HTMLInputElement).value)" /></label>
                  <label v-if="decisionFor(index).accept && operation.op === 'add-edge'" class="dialog-field mt-2">{{ t('agent.op.note') }}<input :value="decisionFor(index).edits?.note ?? operation.edge.note ?? ''" @input="setEdit(index, 'note', ($event.target as HTMLInputElement).value)" /></label>
                </div>
                <div class="flex shrink-0 items-center gap-1" role="group" :aria-label="t('agent.decision.accept')">
                  <button class="rounded-[4px] border px-2.5 py-1 font-sans text-[8px] transition" :class="decisionFor(index).accept ? 'border-blue bg-blue text-paper' : 'border-ink/20 text-ink/55 hover:border-blue hover:text-blue'" @click="setDecision(index, { accept: true, edits: decisionFor(index).edits })">{{ t('agent.decision.accept') }}</button>
                  <button class="rounded-[4px] border px-2.5 py-1 font-sans text-[8px] transition" :class="!decisionFor(index).accept ? 'border-alert bg-alert text-paper' : 'border-ink/20 text-ink/55 hover:border-alert hover:text-alert'" @click="setDecision(index, { accept: false })">{{ t('agent.decision.reject') }}</button>
                </div>
              </div>
            </article>
          </div>
        </div>
      </div>

      <aside class="flex min-h-0 flex-col overflow-y-auto bg-canvas p-5">
        <div class="flex items-center gap-2 text-blue"><span aria-hidden="true">✦</span><h3 class="font-serif text-[14px]">{{ t('agent.reviewTitle') }}</h3></div>
        <p v-if="status" class="mt-2 font-serif text-[9px] leading-[1.5] text-ink/50">{{ status.pdfPath.split(/[\\/]/).pop() }}</p>
        <p v-if="patch" class="mt-1 font-serif text-[10px] leading-[1.5] text-ink/60">{{ patch.operations.length }} {{ t('agent.operations') }} · {{ patch.summary }}</p>
        <div v-if="patch" class="mt-3 flex flex-wrap gap-1"><span v-for="(count, op) in counts" :key="op" class="rounded-[3px] bg-ink/8 px-1.5 py-0.5 font-sans text-[7px] text-ink/60">{{ op }} · {{ count }}</span></div>
        <div class="mt-5 space-y-2">
          <button class="button-primary w-full justify-center" :disabled="!patch || applying || settled" @click="void applySelected()">✓ {{ t('agent.applySelected', { count: acceptedCount }) }}</button>
          <button class="button-secondary w-full justify-center" :disabled="!patch || applying || settled" @click="void rejectAll()">× {{ t('agent.rejectAll') }}</button>
          <button class="button-secondary w-full justify-center" :disabled="!patch || applying" @click="acceptAll">{{ t('agent.acceptAll') }}</button>
        </div>
        <p v-if="error" class="mt-3 flex items-start gap-2 rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-serif text-[9px] leading-[1.5] text-alert">⚠ <span class="min-w-0 break-words">{{ error }}</span></p>
        <p v-if="props.compileError" class="mt-3 flex items-start gap-2 rounded-[4px] border border-alert/40 bg-alert/5 px-3 py-2 font-serif text-[9px] leading-[1.5] text-alert">⚠ <span class="min-w-0 break-words">{{ t('agent.compileFailed', { error: props.compileError }) }}</span></p>
        <div v-if="props.compileResult" class="mt-5 border-t border-ink/15 pt-5">
          <h4 class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">{{ t('agent.compiling') }}</h4>
          <dl class="mt-3 space-y-2">
            <div class="rounded-[4px] border border-ink/12 bg-paper px-3 py-2"><dt class="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/45">{{ t('agent.compileBlockHashes', { count: Object.keys(props.compileResult.compile.blockHashes).length }) }}</dt><dd class="mt-1 truncate font-mono text-[8px] text-ink/70">{{ props.compileResult.compile.contentRootHash.slice(0, 12) }}</dd></div>
            <div class="rounded-[4px] border border-ink/12 bg-paper px-3 py-2"><dt class="font-sans text-[7px] uppercase tracking-[0.12em]" :class="props.compileResult.compile.violations.length ? 'text-alert' : 'text-olive'">{{ props.compileResult.compile.violations.length ? t('agent.compileInvariants', { count: props.compileResult.compile.violations.length }) : t('agent.compileInvariantsOk') }}</dt><dd class="mt-1 font-mono text-[8px] text-ink/60">{{ props.compileResult.logicChain.summary }}</dd></div>
            <div class="rounded-[4px] border border-ink/12 bg-paper px-3 py-2"><dt class="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/45">{{ t('agent.logicChain', { score: props.compileResult.logicChain.score.toFixed(2) }) }}</dt><dd class="mt-1 font-mono text-[8px] text-ink/60">{{ props.compileResult.logicChain.nodeIds.length }} nodes · {{ props.compileResult.logicChain.edgeIds.length }} edges</dd></div>
          </dl>
        </div>
        <p class="mt-auto pt-5 font-serif text-[8px] text-ink/35">{{ status ? stageLabel(status.state) : '' }}</p>
      </aside>
    </section>
  </div>
</template>

<style scoped>
/* Shared panel visual tokens remain in app/globals.css. */
</style>
