<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { builtInPluginCatalog } from "../../../app/plugins/catalog";
import {
  latestCompatiblePlugins,
  pluginCompatibility,
  pluginKey,
} from "../../../app/plugins/identity";
import { pluginReference } from "../../../app/plugins/contracts";
import {
  listenForMycDrops,
  pickMycFiles,
  runAnalysisPlugin,
} from "../../../app/plugins/tauri-client";
import type { InstalledMycPlugin } from "../../../app/plugins/contracts";
import { usePluginHost } from "../runtime/plugin-host";
import { usePanelI18n, type PluginStoreDialogProps } from "./panel-types";
import PluginStoreItem from "./PluginStoreItem.vue";

const props = defineProps<PluginStoreDialogProps>();
const { t } = usePanelI18n();
const pluginHost = usePluginHost();
type StoreFilter = "all" | "installed" | "runtime" | "workspace" | "locales";
const filter = ref<StoreFilter>("all");
const installed = computed(() => pluginHost.installedPlugins);
const activePluginKeys = computed(() => pluginHost.activePluginKeys);
const loading = computed(() => pluginHost.loading);
const hostError = computed(() => pluginHost.error);
const busy = ref(false);
const message = ref("");
const dragOver = ref(false);
const dragCounter = ref(0);
const selfTestResults = ref<Record<string, { status: "running" | "success" | "error"; text: string }>>({});

const latestKeys = computed(() => new Set(latestCompatiblePlugins(installed.value).map(pluginKey)));
const incompatibleCount = computed(() => installed.value.filter((plugin) => !pluginCompatibility(plugin).compatible).length);
const hostBusy = computed(() => busy.value || loading.value);
const visibleInstalled = computed(() => filter.value === "runtime"
  ? installed.value.filter((plugin) => plugin.runtime)
  : filter.value === "workspace"
    ? installed.value.filter((plugin) => plugin.workspace)
    : filter.value === "locales"
      ? installed.value.filter((plugin) => plugin.locales?.length)
      : installed.value);

const refresh = pluginHost.refresh;
const setPluginEnabled = pluginHost.setPluginEnabled;
const enableAll = pluginHost.enableAll;
const removeIncompatible = pluginHost.removeIncompatible;
const install = pluginHost.install;
const installPaths = async (paths: string[]) => {
  const mycPaths = paths.filter((path) => path.toLowerCase().endsWith(".myc"));
  if (!mycPaths.length) return;
  busy.value = true;
  message.value = "";
  let ok = 0;
  const errors: string[] = [];
  for (const path of mycPaths) {
    try { await install(path); ok += 1; } catch (cause) { errors.push(cause instanceof Error ? cause.message : String(cause)); }
  }
  message.value = errors.length === 0 ? t("plugins.installedToast", { count: ok }) : ok > 0 ? `${t("plugins.installedToast", { count: ok })} · ${errors.join("; ")}` : errors.join("; ");
  busy.value = false;
};
const handleBrowse = async () => {
  const paths = await pickMycFiles();
  if (paths?.length) await installPaths(paths);
};
const handleDrop = async (event: DragEvent) => {
  event.preventDefault();
  event.stopPropagation();
  dragCounter.value = 0;
  dragOver.value = false;
  const files = event.dataTransfer?.files;
  if (!files?.length) return;
  const paths: string[] = [];
  for (let index = 0; index < files.length; index += 1) {
    const path = (files[index] as File & { path?: string }).path;
    if (path) paths.push(path);
  }
  if (paths.length) await installPaths(paths);
  else message.value = t("plugins.dropNoPaths");
};
const runSelfTest = async (plugin: InstalledMycPlugin) => {
  const key = pluginKey(plugin);
  busy.value = true;
  selfTestResults.value = { ...selfTestResults.value, [key]: { status: "running", text: t("plugins.selfTestRunning") } };
  try {
    const result = await runAnalysisPlugin(pluginReference(plugin), { operation: "self-test" }, "analysis.run");
    selfTestResults.value = { ...selfTestResults.value, [key]: { status: "success", text: `${t("plugins.selfTestPassed", { duration: result.durationMs, fuel: result.fuelConsumed })} ${JSON.stringify(result.output)}` } };
  } catch (cause) {
    selfTestResults.value = { ...selfTestResults.value, [key]: { status: "error", text: cause instanceof Error ? cause.message : String(cause) } };
  } finally { busy.value = false; }
};

let disposeDropListener: (() => void) | undefined;
onMounted(async () => {
  await refresh();
  disposeDropListener = await listenForMycDrops((paths) => { void installPaths(paths); });
});
onUnmounted(() => disposeDropListener?.());
</script>

<template>
  <div class="fixed inset-0 z-[96] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
    <section class="flex h-[650px] w-[880px] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]" role="dialog" aria-modal="true" aria-labelledby="plugin-store-title">
      <header class="flex shrink-0 items-start justify-between border-b border-ink/15 px-7 py-5"><div><span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">{{ t('plugins.eyebrow') }}</span><h2 id="plugin-store-title" class="mt-1 font-serif text-[21px]">{{ t('plugins.title') }}</h2><p class="mt-1 font-serif text-[10px] text-ink/50">{{ t('plugins.subtitle') }}</p></div><div class="flex items-center gap-1"><button v-if="incompatibleCount" class="button-secondary mr-2 px-3 text-alert" :disabled="hostBusy" @click="void removeIncompatible().then((count) => { message = t('plugins.removedIncompatibleToast', { count }) }).catch((cause) => { message = cause instanceof Error ? cause.message : String(cause) })">⌫ {{ t('plugins.removeIncompatible', { count: incompatibleCount }) }}</button><button class="button-secondary mr-2 px-3" :disabled="hostBusy" @click="enableAll">✓ {{ t('plugins.enableAll') }}</button><button class="icon-quiet" :disabled="hostBusy" @click="void refresh()" :aria-label="t('plugins.refresh')">↻</button><button class="icon-quiet" @click="props.onClose" :aria-label="t('plugins.close')">×</button></div></header>
      <div class="grid min-h-0 flex-1 grid-cols-[210px_minmax(0,1fr)]">
        <aside class="border-r border-ink/15 bg-canvas p-4"><nav class="space-y-1" aria-label="Plugin filters"><button v-for="entry in [['all', t('plugins.all')], ['installed', t('plugins.installed')], ['runtime', t('plugins.runtime')], ['workspace', t('plugins.workspace')], ['locales', t('plugins.locales')]] as Array<[StoreFilter, string]>" :key="entry[0]" class="flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left font-serif text-[12px] transition" :class="filter === entry[0] ? 'bg-blue-soft text-blue' : 'hover:bg-ink/5'" @click="filter = entry[0]">▣ {{ entry[1] }}</button></nav><div class="mt-6 rounded-[5px] border border-blue/20 bg-blue-soft p-3"><div class="flex items-center gap-2 text-blue">▣ <span class="font-serif text-[11px]">{{ t('plugins.runtime') }}</span></div><p class="mt-2 font-serif text-[9px] leading-[1.45] text-ink/55">{{ t('plugins.runtimeHint') }}</p></div></aside>
        <div class="min-h-0 overflow-y-auto p-6">
          <div class="rounded-[6px] border-2 border-dashed px-5 py-5 transition-colors" :class="dragOver ? 'border-blue bg-blue/5' : hostBusy ? 'border-ink/15 bg-canvas opacity-60' : 'border-ink/25 bg-canvas hover:border-ink/40'" role="button" tabindex="0" :aria-label="t('plugins.dropTitle')" @dragenter.prevent="dragCounter++; dragOver = true" @dragleave.prevent="dragCounter--; if (dragCounter <= 0) { dragCounter = 0; dragOver = false }" @dragover.prevent @drop="void handleDrop($event)"><div class="flex items-center gap-3"><span class="text-2xl text-blue/70">⇧</span><div class="flex-1"><p class="font-serif text-[13px]">{{ dragOver ? t('plugins.dropActive') : t('plugins.dropTitle') }}</p><p class="mt-0.5 font-serif text-[9px] text-ink/50">{{ t('plugins.dropHint') }}</p></div><button type="button" class="flex items-center gap-1.5 rounded-[5px] border border-blue/40 bg-blue-soft px-3.5 py-2 font-serif text-[11px] text-blue transition" :disabled="hostBusy" @click.stop="void handleBrowse()">↑ {{ t('plugins.browseFiles') }}</button></div></div>
          <div v-if="message || hostError" class="mt-3 rounded-[4px] border border-blue/20 bg-blue-soft px-3 py-2 font-serif text-[10px] text-blue">{{ message || hostError }}</div>
          <section v-if="filter === 'all'" class="mt-6">
            <h3 class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">{{ t('plugins.builtInCatalog') }}</h3>
            <div class="mt-2 grid grid-cols-2 gap-3">
              <PluginStoreItem
                v-for="plugin in builtInPluginCatalog"
                :key="plugin.id"
                :name="plugin.name"
                :version="plugin.version"
                :kind="plugin.category"
                :description="plugin.description"
              >
                <template #icon><span class="mt-0.5 text-ink/60">◈</span></template>
                <template #status>
                  <span class="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/40">{{ plugin.status }}</span>
                </template>
              </PluginStoreItem>
            </div>
          </section>
          <section class="mt-6">
            <h3 class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">{{ t('plugins.installed') }} · .myc</h3>
            <div class="mt-2 space-y-2">
              <p v-if="visibleInstalled.length === 0" class="rounded-[5px] border border-ink/15 px-4 py-6 text-center font-serif text-[10px] text-ink/45">{{ t('plugins.noInstalled') }}</p>
              <PluginStoreItem
                v-for="plugin in visibleInstalled"
                :key="pluginKey(plugin)"
                :name="plugin.manifest.metadata.name"
                :version="plugin.manifest.metadata.version"
                :kind="plugin.manifest.kind"
                :description="plugin.manifest.metadata.description"
              >
                <template #icon><span class="mt-0.5 text-blue">{{ plugin.runtime ? '⌘' : '◈' }}</span></template>
                <p class="mt-2 font-sans text-[8px] text-ink/45">{{ plugin.manifest.spec.capabilities.join(' · ') }}{{ plugin.runtime ? ` · ${plugin.runtime.language}/wasm` : '' }}</p>
                <p v-if="!pluginCompatibility(plugin).compatible" class="mt-1 font-serif text-[8px] text-alert">{{ pluginCompatibility(plugin).issues.map((issue) => t(issue.key, issue.params ?? {})).join(' · ') }}</p>
                <p v-if="pluginCompatibility(plugin).compatible && !latestKeys.has(pluginKey(plugin))" class="mt-1 font-serif text-[8px] text-ink/40">{{ t('plugins.superseded') }}</p>
                <p v-if="plugin.runtime" class="mt-1 truncate font-mono text-[7px] text-ink/40">{{ t('plugins.sha256') }} · {{ plugin.runtime.entrySha256 }}</p>
                <p
                  v-if="selfTestResults[pluginKey(plugin)]"
                  class="mt-2 rounded-[4px] border px-2.5 py-2 font-serif text-[9px]"
                  :class="selfTestResults[pluginKey(plugin)].status === 'error' ? 'border-alert/25 bg-alert/5 text-alert' : selfTestResults[pluginKey(plugin)].status === 'running' ? 'border-blue/20 bg-blue-soft text-blue' : 'border-ink/15 bg-canvas text-ink/65'"
                  role="status"
                  aria-live="polite"
                >
                  {{ selfTestResults[pluginKey(plugin)].text }}
                </p>
                <template #actions>
                  <button v-if="plugin.runtime" class="button-secondary px-3" :disabled="!activePluginKeys.has(pluginKey(plugin)) || hostBusy" :title="!activePluginKeys.has(pluginKey(plugin)) ? t('plugins.enableToTest') : undefined" @click="void runSelfTest(plugin)">▶ {{ t('plugins.selfTest') }}</button>
                  <button class="px-3" :class="activePluginKeys.has(pluginKey(plugin)) ? 'button-primary' : 'button-secondary'" :disabled="!pluginCompatibility(plugin).compatible || hostBusy" @click="setPluginEnabled(plugin, !activePluginKeys.has(pluginKey(plugin)))">{{ activePluginKeys.has(pluginKey(plugin)) ? t('plugins.enabled') : t('plugins.enable') }}</button>
                </template>
              </PluginStoreItem>
            </div>
          </section>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
/* Shared plugin-store visual tokens remain in app/globals.css. */
</style>
