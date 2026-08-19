<script setup lang="ts">
import { computed } from "vue";
import type { MessageKey } from "../../../app/i18n/catalog";
import type { DiffState } from "../../../app/lib/graph/canvas-diff";
import type { CanvasDiffDocumentResult } from "../../../app/domain/canvas-diff";
import type { ResearchEdge, ResearchNodeType } from "../../../app/lib/research-types";
import {
  diffMeta,
  nodeTitleMap,
  usePanelI18n,
  versionById,
  type DiffPanelProps,
  type DiffVersion,
} from "./panel-types";

const props = defineProps<DiffPanelProps>();
const { t } = usePanelI18n();
const base = computed(() => versionById(props.versions, props.baseId));
const compare = computed(() => versionById(props.versions, props.compareId));
const batchDocuments = computed(() => props.batch?.documents ?? []);
const batchChanged = computed(() => props.batch?.summary.totalChanges ?? 0);

const batchDocumentLabel = (document: CanvasDiffDocumentResult) =>
  document.provenance.fileName || document.documentId;
const batchDocumentStateClass = (state: CanvasDiffDocumentResult["state"]) => {
  if (state === "added" || state === "removed" || state === "modified") {
    return `${diffMeta[state].bgClass} ${diffMeta[state].textClass}`;
  }
  return "bg-ink/5 text-ink/55";
};
const batchDocumentGlyph = (state: CanvasDiffDocumentResult["state"]) =>
  state === "added" ? "+" : state === "removed" ? "−" : state === "modified" ? "✎" : "·";

const baseStateOf = (id: string): DiffState | null => {
  if (!props.result) return null;
  if (props.result.removedNodes.includes(id) || props.result.removedEdges.includes(id)) return "removed";
  if (props.result.modifiedNodes.some((entity) => entity.entityId === id) || props.result.modifiedEdges.some((entity) => entity.entityId === id)) return "modified";
  return null;
};
const compareStateOf = (id: string): DiffState | null => {
  if (!props.result) return null;
  if (props.result.addedNodes.includes(id) || props.result.addedEdges.includes(id)) return "added";
  if (props.result.modifiedNodes.some((entity) => entity.entityId === id) || props.result.modifiedEdges.some((entity) => entity.entityId === id)) return "modified";
  return null;
};
const totals = computed(() => {
  if (!props.result) return null;
  const added = props.result.addedNodes.length + props.result.addedEdges.length + props.result.addedEvidence.length;
  const removed = props.result.removedNodes.length + props.result.removedEdges.length + props.result.removedEvidence.length;
  const modified = props.result.modifiedNodes.length + props.result.modifiedEdges.length + props.result.modifiedEvidence.length;
  return { added, removed, modified, changed: added + removed + modified };
});
const stateClass = (state: DiffState | null) => state ? `${diffMeta[state].bgClass} ${diffMeta[state].textClass}` : "text-ink/70 hover:bg-ink/5";
const stateGlyph = (state: DiffState | null) => state === "added" ? "+" : state === "removed" ? "−" : state === "modified" ? "✎" : "·";
const stateLabel = (state: DiffState) => t(diffMeta[state].labelKey);
const nodeKind = (type: ResearchNodeType) => {
  const keys: Partial<Record<ResearchNodeType, MessageKey>> = { question: "node.question", concept: "node.group", variable: "node.variable", method: "node.method", dataset: "node.data", evidence: "node.evidence", result: "node.result", note: "node.note" };
  return keys[type] ? t(keys[type]!) : type;
};
const edgeKind = (type: ResearchEdge["type"]) => {
  const keys: Partial<Record<ResearchEdge["type"], MessageKey>> = {
    T: "edgeType.transform",
    K: "edgeType.kernel",
    I: "edgeType.intervention",
    M: "edgeType.marginalize",
    Q: "edgeType.quotient",
  };
  return keys[type] ? t(keys[type]!) : type;
};
const versionRows = (version: DiffVersion, stateOf: (id: string) => DiffState | null) => {
  const titles = nodeTitleMap(version.project);
  return {
    nodes: version.project.nodes.map((node) => ({ id: node.id, label: node.title, kind: nodeKind(node.type), state: stateOf(node.id) })),
    edges: version.project.edges.map((edge) => ({ id: edge.id, label: `${titles.get(edge.source) ?? edge.source} → ${titles.get(edge.target) ?? edge.target}`, kind: edgeKind(edge.type), state: stateOf(edge.id) })),
  };
};
const baseRows = computed(() => base.value ? versionRows(base.value, baseStateOf) : { nodes: [], edges: [] });
const compareRows = computed(() => compare.value ? versionRows(compare.value, compareStateOf) : { nodes: [], edges: [] });
const overlayRows = computed(() => {
  if (!props.result || !compare.value) return [] as Array<{ id: string; label: string; kind: string; state: DiffState }>;
  const titles = nodeTitleMap(compare.value.project);
  const rows: Array<{ id: string; label: string; kind: string; state: DiffState }> = [];
  for (const id of props.result.addedNodes) {
    const node = compare.value.project.nodes.find((item) => item.id === id);
    if (node) rows.push({ id, label: node.title, kind: nodeKind(node.type), state: "added" });
  }
  for (const entity of props.result.modifiedNodes) {
    const node = compare.value.project.nodes.find((item) => item.id === entity.entityId);
    if (node) rows.push({ id: node.id, label: node.title, kind: nodeKind(node.type), state: "modified" });
  }
  for (const id of props.result.addedEdges) {
    const edge = compare.value.project.edges.find((item) => item.id === id);
    if (edge) rows.push({ id, label: `${titles.get(edge.source) ?? edge.source} → ${titles.get(edge.target) ?? edge.target}`, kind: edgeKind(edge.type), state: "added" });
  }
  for (const entity of props.result.modifiedEdges) {
    const edge = compare.value.project.edges.find((item) => item.id === entity.entityId);
    if (edge) rows.push({ id: edge.id, label: `${titles.get(edge.source) ?? edge.source} → ${titles.get(edge.target) ?? edge.target}`, kind: edgeKind(edge.type), state: "modified" });
  }
  return rows;
});
</script>

<template>
  <div class="fixed top-0 bottom-0 z-40 flex flex-col bg-paper text-ink" :class="props.mode === 'overlay' ? 'right-0 w-[400px] border-l border-ink/15 shadow-[0_0_40px_rgba(30,32,35,.12)]' : 'inset-0'" role="dialog" :aria-label="t('diff.title')" :aria-modal="props.mode === 'side-by-side'">
    <header class="flex h-12 shrink-0 items-center gap-3 border-b border-ink/15 bg-paper px-4">
      <button type="button" class="grid size-8 place-items-center rounded-[4px] text-ink/60 transition hover:bg-blue-soft hover:text-blue" @click="props.onClose" :aria-label="t('diff.close')">×</button>
      <span class="font-serif text-[15px] font-semibold">{{ t('diff.title') }}</span>
      <div class="ml-2 flex items-center overflow-hidden rounded-[4px] border border-ink/20 bg-canvas p-0.5"><button type="button" class="flex items-center gap-1.5 rounded-[3px] px-2.5 py-1 font-sans text-[10px] transition" :class="props.mode === 'side-by-side' ? 'bg-paper text-blue shadow-sm' : 'text-ink/55 hover:text-ink'" :aria-pressed="props.mode === 'side-by-side'" @click="props.onModeChange('side-by-side')">▥ {{ t('diff.sideBySide') }}</button><button type="button" class="flex items-center gap-1.5 rounded-[3px] px-2.5 py-1 font-sans text-[10px] transition" :class="props.mode === 'overlay' ? 'bg-paper text-blue shadow-sm' : 'text-ink/55 hover:text-ink'" :aria-pressed="props.mode === 'overlay'" @click="props.onModeChange('overlay')">▤ {{ t('diff.overlay') }}</button></div>
      <template v-if="props.mode === 'side-by-side'"><label class="flex items-center gap-1.5"><span class="font-sans text-[9px] uppercase tracking-[0.12em] text-ink/45">{{ t('diff.base') }}</span><select class="max-w-[180px] rounded-[4px] border border-ink/20 bg-paper px-2 py-1 font-serif text-[11px] text-ink outline-none transition focus:border-blue" :value="props.baseId" @change="props.onBaseChange(($event.target as HTMLSelectElement).value)"><option v-for="version in props.versions" :key="version.id" :value="version.id">{{ version.label }}</option></select></label><span class="text-ink/40">→</span><label class="flex items-center gap-1.5"><span class="font-sans text-[9px] uppercase tracking-[0.12em] text-ink/45">{{ t('diff.comparison') }}</span><select class="max-w-[180px] rounded-[4px] border border-ink/20 bg-paper px-2 py-1 font-serif text-[11px] text-ink outline-none transition focus:border-blue" :value="props.compareId" @change="props.onCompareChange(($event.target as HTMLSelectElement).value)"><option v-for="version in props.versions" :key="version.id" :value="version.id">{{ version.label }}</option></select></label></template>
      <div v-if="totals && totals.changed > 0" class="ml-auto hidden items-center gap-2 md:flex"><span v-for="state in ['added','removed','modified'] as DiffState[]" :key="state" class="inline-flex items-center gap-1 rounded-full px-2 py-0.5 font-sans text-[9px] font-medium" :class="`${diffMeta[state].bgClass} ${diffMeta[state].textClass}`">{{ stateGlyph(state) }} {{ stateLabel(state) }} {{ totals[state] }}</span></div>
      <div v-if="props.batch && batchChanged > 0" class="ml-auto hidden items-center gap-2 md:flex"><span class="rounded-full bg-blue-soft px-2 py-0.5 font-sans text-[9px] text-blue">{{ batchDocuments.length }} documents · {{ batchChanged }} changes</span></div>
    </header>

    <div class="flex min-h-0 flex-1 flex-col">
      <p v-if="props.loading" class="m-auto font-serif text-[12px] text-ink/50">{{ t('diff.computing') }}</p>
      <p v-else-if="props.error" class="m-auto max-w-md text-center font-serif text-[12px] text-diff-removed">{{ props.error }}</p>
      <p v-else-if="!props.batch && (!props.result || !totals || totals.changed === 0)" class="m-auto font-serif text-[12px] text-ink/50">{{ t('diff.noChanges') }}</p>
      <div v-else-if="props.batch" class="min-h-0 flex-1 overflow-y-auto bg-canvas p-3">
        <div class="rounded-[6px] border border-ink/15 bg-paper p-3">
          <div class="flex items-center justify-between gap-3 border-b border-ink/10 pb-3">
            <div>
              <p class="font-sans text-[9px] uppercase tracking-[0.14em] text-ink/45">Document batch</p>
              <p class="mt-1 font-serif text-[13px] font-semibold">{{ props.batch.baseline.label }} → {{ props.batch.comparison.label }}</p>
            </div>
            <span class="font-sans text-[10px] text-ink/55">{{ batchChanged }} changes</span>
          </div>
          <div class="mt-3 grid grid-cols-2 gap-2 text-[10px] md:grid-cols-4">
            <div class="rounded-[4px] bg-diff-added-soft px-2 py-1.5 text-diff-added">+ {{ props.batch.summary.nodes.added }} nodes</div>
            <div class="rounded-[4px] bg-diff-removed-soft px-2 py-1.5 text-diff-removed">− {{ props.batch.summary.nodes.removed }} nodes</div>
            <div class="rounded-[4px] bg-diff-modified-soft px-2 py-1.5 text-diff-modified">✎ {{ props.batch.summary.nodes.modified }} nodes</div>
            <div class="rounded-[4px] bg-ink/5 px-2 py-1.5 text-ink/60">{{ props.batch.summary.documents.changed }} changed documents</div>
          </div>
        </div>
        <div class="mt-3 space-y-1.5">
          <div v-for="document in batchDocuments" :key="document.documentId" class="flex items-center gap-2 rounded-[5px] border border-ink/12 bg-paper px-3 py-2">
            <span class="grid size-5 shrink-0 place-items-center rounded-full font-sans text-[10px]" :class="batchDocumentStateClass(document.state)">{{ batchDocumentGlyph(document.state) }}</span>
            <span class="min-w-0 flex-1 truncate font-serif text-[11px]" :title="document.documentId">{{ batchDocumentLabel(document) }}</span>
            <span class="shrink-0 font-sans text-[9px] text-ink/45">{{ document.summary.changed }} changes</span>
            <span class="shrink-0 font-mono text-[8px] text-ink/35">{{ document.documentId }}</span>
          </div>
          <p v-if="batchDocuments.length === 0" class="rounded-[5px] border border-dashed border-ink/15 px-3 py-6 text-center font-serif text-[11px] text-ink/50">{{ t('diff.noChanges') }}</p>
        </div>
      </div>
      <div v-else-if="props.mode === 'side-by-side'" class="grid min-h-0 flex-1 grid-cols-2 gap-3 p-3">
        <div v-for="column in [{ version: base, rows: baseRows }, { version: compare, rows: compareRows }]" :key="column.version?.id" class="flex min-h-0 min-w-0 flex-col overflow-hidden rounded-[6px] border border-ink/15 bg-paper"><div class="flex items-center justify-between gap-2 border-b border-ink/10 px-3 py-2"><span class="truncate font-serif text-[12px] font-semibold">{{ column.version?.label }}</span><span class="shrink-0 font-sans text-[9px] uppercase tracking-[0.12em] text-ink/45">{{ t('diff.nodes') }} {{ column.version?.project.nodes.length }} · {{ t('diff.edges') }} {{ column.version?.project.edges.length }}</span></div><div class="min-h-0 flex-1 overflow-y-auto p-1.5"><button v-for="row in column.rows.nodes" :key="`node-${row.id}`" class="flex w-full items-center gap-2 rounded-[3px] px-2 py-1.5 text-left font-serif text-[10px]" :class="stateClass(row.state)" @click="props.onFocus('node', row.id)"><span class="w-4 font-sans">{{ stateGlyph(row.state) }}</span><span class="min-w-0 flex-1 truncate">{{ row.label }}</span><span class="font-sans text-[8px] text-ink/45">{{ row.kind }}</span></button><button v-for="row in column.rows.edges" :key="`edge-${row.id}`" class="flex w-full items-center gap-2 rounded-[3px] px-2 py-1.5 text-left font-serif text-[10px]" :class="stateClass(row.state)" @click="props.onFocus('edge', row.id)"><span class="w-4 font-sans">↗</span><span class="min-w-0 flex-1 truncate">{{ row.label }}</span><span class="font-sans text-[8px] text-ink/45">{{ row.kind }}</span></button></div></div>
      </div>
      <div v-else class="flex min-h-0 flex-1 flex-col border-l border-ink/15 bg-canvas"><div class="flex items-center justify-between border-b border-ink/10 px-3 py-2"><span class="font-serif text-[11px] text-olive">{{ compare?.label ?? '' }} · {{ t('diff.overlayHint') }}</span></div><div class="min-h-0 flex-1 overflow-y-auto p-2"><button v-for="row in overlayRows" :key="`${row.state}-${row.id}`" class="flex w-full items-center gap-2 rounded-[3px] px-2 py-2 text-left font-serif text-[10px]" :class="stateClass(row.state)" @click="props.onFocus(row.kind.includes('relation') ? 'edge' : 'node', row.id)"><span class="w-4 font-sans">{{ stateGlyph(row.state) }}</span><span class="min-w-0 flex-1 truncate">{{ row.label }}</span><span class="font-sans text-[8px] text-ink/45">{{ row.kind }}</span></button></div></div>
    </div>
  </div>
</template>

<style scoped>
/* Shared diff panel visual tokens remain in app/globals.css. */
</style>
