<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { builtInPluginCatalog } from "../../../app/plugins/catalog";
import {
  latestCompatiblePlugins,
  pluginCompatibility,
  pluginKey,
  supersededCompatiblePlugins,
} from "../../../app/plugins/identity";
import { pluginReference, type InstalledMycPlugin } from "../../../app/plugins/contracts";
import type { PluginManifest } from "../../../app/lib/research-types";
import {
  listenForMycDrops,
  pickMycFiles,
  runAnalysisPlugin,
} from "../../../app/plugins/tauri-client";
import { usePluginHost } from "../runtime/plugin-host";
import {
  defaultPluginSettingsDraft,
  draftFromPluginSettings,
  normalizePluginSettingDefinitions,
  settingsWriteFromDraft,
  type PluginSettingsTarget,
} from "./panel-types";
import { usePanelI18n, type PluginStoreDialogProps } from "./panel-types";
import PluginSettingsDialog from "./PluginSettingsDialog.vue";
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
const settingsTarget = ref<PluginSettingsTarget | null>(null);
const settingsSnapshot = ref<Awaited<ReturnType<typeof pluginHost.loadPluginSettings>> | null>(null);
const settingsError = ref("");
const settingsSaving = ref(false);

const latestPlugins = computed(() => latestCompatiblePlugins(installed.value));
const latestById = computed(() => new Map<string, InstalledMycPlugin>(
  latestPlugins.value.map((plugin): [string, InstalledMycPlugin] => [plugin.manifest.metadata.id, plugin]),
));
const supersededInstalled = computed(() => supersededCompatiblePlugins(installed.value));
const supersededKeys = computed(() => new Set(supersededInstalled.value.map(pluginKey)));
const incompatibleCount = computed(() => installed.value.filter((plugin) => !pluginCompatibility(plugin).compatible).length);
const hostBusy = computed(() => busy.value || loading.value);
const matchesFilter = (plugin: InstalledMycPlugin): boolean => filter.value === "runtime"
  ? Boolean(plugin.runtime)
  : filter.value === "workspace"
    ? Boolean(plugin.workspace)
    : filter.value === "locales"
      ? Boolean(plugin.locales?.length)
      : true;
const visibleInstalled = computed(() => installed.value.filter(
  (plugin) => !supersededKeys.value.has(pluginKey(plugin)) && matchesFilter(plugin),
));
const visibleSuperseded = computed(() => supersededInstalled.value.filter(matchesFilter));
const currentVersionFor = (plugin: InstalledMycPlugin): string =>
  latestById.value.get(plugin.manifest.metadata.id)?.manifest.metadata.version
  ?? plugin.manifest.metadata.version;

const selectedSettingsKey = computed(() => {
  const target = settingsTarget.value;
  return target ? `${target.reference.id}@${target.reference.version}` : "";
});
const selectedSnapshot = computed(() => {
  const target = settingsTarget.value;
  if (!target) return null;
  return settingsSnapshot.value ?? {
    pluginId: target.reference.id,
    pluginVersion: target.reference.version,
    values: {},
    configuredSecrets: {},
  };
});
const selectedDraft = computed(() => {
  const target = settingsTarget.value;
  const snapshot = selectedSnapshot.value;
  if (!target || !snapshot) return {};
  return settingsSnapshot.value
    ? draftFromPluginSettings(target.definitions, snapshot)
    : defaultPluginSettingsDraft(target.definitions, snapshot.configuredSecrets);
});
const selectedSettingsLoading = computed(() => Boolean(
  selectedSettingsKey.value && pluginHost.pluginSettingsLoading.has(selectedSettingsKey.value),
));
const selectedSettingsError = computed(() => {
  if (settingsError.value) return settingsError.value;
  return selectedSettingsKey.value ? pluginHost.pluginSettingsErrors[selectedSettingsKey.value] ?? "" : "";
});

const stringMetadata = (metadata: Record<string, unknown>, keys: string[]): string | undefined => {
  for (const key of keys) {
    if (typeof metadata[key] === "string" && metadata[key]) return metadata[key] as string;
  }
  return undefined;
};

const targetFromInstalled = (plugin: InstalledMycPlugin): PluginSettingsTarget | null => {
  const metadata = plugin.manifest.metadata as unknown as Record<string, unknown>;
  const definitions = normalizePluginSettingDefinitions(plugin.manifest.spec.settings);
  if (!definitions.length) return null;
  return {
    source: "installed",
    reference: pluginReference(plugin),
    name: plugin.manifest.metadata.name,
    version: plugin.manifest.metadata.version,
    kind: plugin.manifest.kind,
    description: plugin.manifest.metadata.description,
    publisher: plugin.manifest.metadata.publisher,
    developer: plugin.manifest.metadata.developer,
    developerUuid: stringMetadata(metadata, ["developerUuid", "developerId", "uuid"]),
    signaturePresent: Boolean(plugin.manifest.signature),
    update: plugin.manifest.metadata.update,
    definitions,
    connections: plugin.manifest.spec.connections ?? [],
    native: true,
    uninstallable: true,
  };
};

const targetFromBuiltIn = (plugin: PluginManifest): PluginSettingsTarget | null => {
  const definitions = normalizePluginSettingDefinitions(plugin.settings);
  if (!definitions.length) return null;
  return {
    source: "builtin",
    reference: { id: plugin.id, version: plugin.version, name: plugin.name },
    name: plugin.name,
    version: plugin.version,
    kind: plugin.category,
    description: plugin.description,
    publisher: plugin.publisher,
    developer: plugin.developer,
    connections: [],
    definitions,
    update: plugin.update,
    native: false,
    uninstallable: false,
  };
};

const testConnection = async (
  connectionId: string,
  draft: Parameters<typeof settingsWriteFromDraft>[1],
) => {
  const target = settingsTarget.value;
  if (!target) throw new Error("PLUGIN_SETTINGS_TARGET_REQUIRED");
  return pluginHost.testPluginConnection(
    target.reference,
    connectionId,
    settingsWriteFromDraft(target.definitions, draft),
    target.native,
  );
};

const openSettings = async (target: PluginSettingsTarget) => {
  settingsTarget.value = target;
  settingsSnapshot.value = null;
  settingsError.value = "";
  try {
    settingsSnapshot.value = await pluginHost.loadPluginSettings(
      target.reference,
      target.definitions,
      target.native,
    );
  } catch {
    // The dialog stays open and exposes the host error; the user can cancel without losing state.
  }
};

const closeSettings = (): void => {
  if (settingsSaving.value) return;
  settingsTarget.value = null;
  settingsSnapshot.value = null;
  settingsError.value = "";
};

const saveSettings = async (draft: Parameters<typeof settingsWriteFromDraft>[1]) => {
  const target = settingsTarget.value;
  if (!target) return;
  settingsSaving.value = true;
  settingsError.value = "";
  try {
    settingsSnapshot.value = await pluginHost.savePluginSettings(
      target.reference,
      target.definitions,
      settingsWriteFromDraft(target.definitions, draft),
      target.native,
    );
    message.value = t("plugins.settingsSaved");
  } catch (cause) {
    settingsError.value = cause instanceof Error ? cause.message : String(cause);
    throw cause;
  } finally {
    settingsSaving.value = false;
  }
};

const resetSettings = async () => {
  const target = settingsTarget.value;
  if (!target) return;
  settingsSaving.value = true;
  settingsError.value = "";
  try {
    settingsSnapshot.value = await pluginHost.resetPluginSettings(target.reference, target.definitions, target.native);
    message.value = t("plugins.settingsReset");
  } catch (cause) {
    settingsError.value = cause instanceof Error ? cause.message : String(cause);
  } finally {
    settingsSaving.value = false;
  }
};

const uninstallPlugin = async (plugin: InstalledMycPlugin) => {
  const name = plugin.manifest.metadata.name;
  const label = `${name} (${plugin.manifest.metadata.version})`;
  if (typeof window !== "undefined" && !window.confirm(t("plugins.uninstallConfirm", { name: label }))) return;
  busy.value = true;
  try {
    await pluginHost.uninstall(plugin);
    if (settingsTarget.value?.reference.id === plugin.manifest.metadata.id && settingsTarget.value.reference.version === plugin.manifest.metadata.version) {
      settingsTarget.value = null;
      settingsSnapshot.value = null;
    }
    message.value = t("plugins.uninstalledToast", { name: label });
  } catch (cause) {
    if (settingsTarget.value?.reference.id === plugin.manifest.metadata.id && settingsTarget.value.reference.version === plugin.manifest.metadata.version) {
      settingsError.value = cause instanceof Error ? cause.message : String(cause);
    } else {
      message.value = cause instanceof Error ? cause.message : String(cause);
    }
  } finally {
    busy.value = false;
  }
};

const uninstallSelected = async () => {
  const target = settingsTarget.value;
  if (!target || target.source !== "installed") return;
  const plugin = installed.value.find((candidate) => pluginKey(candidate) === `${target.reference.id}@${target.reference.version}`);
  if (plugin) await uninstallPlugin(plugin);
};

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
    try {
      await install(path);
      ok += 1;
    } catch (cause) {
      errors.push(cause instanceof Error ? cause.message : String(cause));
    }
  }
  message.value = errors.length === 0
    ? t("plugins.installedToast", { count: ok })
    : ok > 0
      ? `${t("plugins.installedToast", { count: ok })} · ${errors.join("; ")}`
      : errors.join("; ");
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
    selfTestResults.value = {
      ...selfTestResults.value,
      [key]: { status: "success", text: `${t("plugins.selfTestPassed", { duration: result.durationMs, fuel: result.fuelConsumed })} ${JSON.stringify(result.output)}` },
    };
  } catch (cause) {
    selfTestResults.value = { ...selfTestResults.value, [key]: { status: "error", text: cause instanceof Error ? cause.message : String(cause) } };
  } finally {
    busy.value = false;
  }
};

let disposeDropListener: (() => void) | undefined;
onMounted(async () => {
  await refresh();
  disposeDropListener = await listenForMycDrops((paths) => { void installPaths(paths); });
});
onUnmounted(() => disposeDropListener?.());
</script>

<template>
  <div class="fixed inset-0 z-[96] grid place-items-center bg-ink/10 backdrop-blur-[2px]" @wheel.stop>
    <section class="flex h-[650px] w-[880px] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]" role="dialog" aria-modal="true" aria-labelledby="plugin-store-title">
      <header class="flex shrink-0 items-start justify-between border-b border-ink/15 px-7 py-5">
        <div>
          <span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">{{ t('plugins.eyebrow') }}</span>
          <h2 id="plugin-store-title" class="mt-1 font-serif text-[21px]">{{ t('plugins.title') }}</h2>
          <p class="mt-1 font-serif text-[10px] text-ink/50">{{ t('plugins.subtitle') }}</p>
        </div>
        <div class="flex items-center gap-1">
          <button v-if="incompatibleCount" class="button-secondary mr-2 px-3 text-alert" :disabled="hostBusy" @click="void removeIncompatible().then((count) => { message = t('plugins.removedIncompatibleToast', { count }) }).catch((cause) => { message = cause instanceof Error ? cause.message : String(cause) })">{{ t('plugins.removeIncompatible', { count: incompatibleCount }) }}</button>
          <button class="button-secondary mr-2 px-3" :disabled="hostBusy" @click="enableAll">{{ t('plugins.enableAll') }}</button>
          <button class="icon-quiet" :disabled="hostBusy" @click="void refresh()" :aria-label="t('plugins.refresh')">↻</button>
          <button class="icon-quiet" @click="props.onClose" :aria-label="t('plugins.close')">×</button>
        </div>
      </header>

      <div class="grid min-h-0 flex-1 grid-cols-[210px_minmax(0,1fr)]">
        <aside class="border-r border-ink/15 bg-canvas p-4">
          <nav class="space-y-1" aria-label="Plugin filters">
            <button v-for="entry in [['all', t('plugins.all')], ['installed', t('plugins.installed')], ['runtime', t('plugins.runtime')], ['workspace', t('plugins.workspace')], ['locales', t('plugins.locales')]] as Array<[StoreFilter, string]>" :key="entry[0]" class="flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left font-serif text-[12px] transition" :class="filter === entry[0] ? 'bg-blue-soft text-blue' : 'hover:bg-ink/5'" @click="filter = entry[0]">{{ entry[1] }}</button>
          </nav>
          <div class="mt-6 rounded-[5px] border border-blue/20 bg-blue-soft p-3">
            <div class="flex items-center gap-2 text-blue"><span class="font-serif text-[11px]">{{ t('plugins.runtime') }}</span></div>
            <p class="mt-2 font-serif text-[9px] leading-[1.45] text-ink/55">{{ t('plugins.runtimeHint') }}</p>
          </div>
        </aside>

        <div class="min-h-0 overflow-y-auto p-6">
          <div class="rounded-[6px] border-2 border-dashed px-5 py-5 transition-colors" :class="dragOver ? 'border-blue bg-blue/5' : hostBusy ? 'border-ink/15 bg-canvas opacity-60' : 'border-ink/25 bg-canvas hover:border-ink/40'" role="button" tabindex="0" :aria-label="t('plugins.dropTitle')" @dragenter.prevent="dragCounter++; dragOver = true" @dragleave.prevent="dragCounter--; if (dragCounter <= 0) { dragCounter = 0; dragOver = false }" @dragover.prevent @drop="void handleDrop($event)">
            <div class="flex items-center gap-3">
              <span class="text-2xl text-blue/70">⇩</span>
              <div class="flex-1"><p class="font-serif text-[13px]">{{ dragOver ? t('plugins.dropActive') : t('plugins.dropTitle') }}</p><p class="mt-0.5 font-serif text-[9px] text-ink/50">{{ t('plugins.dropHint') }}</p></div>
              <button type="button" class="flex items-center gap-1.5 rounded-[5px] border border-blue/40 bg-blue-soft px-3.5 py-2 font-serif text-[11px] text-blue transition" :disabled="hostBusy" @click.stop="void handleBrowse()">{{ t('plugins.browseFiles') }}</button>
            </div>
          </div>
          <div v-if="message || hostError" class="mt-3 rounded-[4px] border border-blue/20 bg-blue-soft px-3 py-2 font-serif text-[10px] text-blue">{{ message || hostError }}</div>
          <div v-if="supersededInstalled.length" class="mt-3 rounded-[5px] border border-alert/25 bg-alert/5 px-3 py-2" role="alert">
            <p class="font-sans text-[8px] uppercase tracking-[0.12em] text-alert">{{ t('plugins.versionMismatchTitle') }}</p>
            <p class="mt-1 font-serif text-[10px] leading-[1.4] text-ink/65">{{ t('plugins.versionMismatchHint', { count: supersededInstalled.length }) }}</p>
          </div>

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
                :on-open-settings="targetFromBuiltIn(plugin) ? () => void openSettings(targetFromBuiltIn(plugin)!) : undefined"
              >
                <template #icon><span class="mt-0.5 text-ink/60">◈</span></template>
                <template #status>
                  <span class="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/40">{{ plugin.settings?.length ? t('plugins.settingsAvailable') : plugin.status }}</span>
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
                :on-open-settings="targetFromInstalled(plugin) ? () => void openSettings(targetFromInstalled(plugin)!) : undefined"
              >
                <template #icon><span class="mt-0.5 text-blue">{{ plugin.runtime ? '◆' : '◈' }}</span></template>
                <p class="mt-2 font-sans text-[8px] text-ink/45">{{ plugin.manifest.spec.capabilities.join(' · ') }}{{ plugin.runtime ? ` · ${plugin.runtime.language}/wasm` : '' }}</p>
                <p v-if="!pluginCompatibility(plugin).compatible" class="mt-1 font-serif text-[8px] text-alert">{{ pluginCompatibility(plugin).issues.map((issue) => t(issue.key, issue.params ?? {})).join(' · ') }}</p>
                <p v-if="plugin.runtime" class="mt-1 truncate font-mono text-[7px] text-ink/40">{{ t('plugins.sha256') }} · {{ plugin.runtime.entrySha256 }}</p>
                <p v-if="plugin.manifest.spec.settings?.length" class="mt-1 font-serif text-[9px] text-blue/70">{{ t('plugins.settingsAvailable') }}</p>
                <p v-if="selfTestResults[pluginKey(plugin)]" class="mt-2 rounded-[4px] border px-2.5 py-2 font-serif text-[9px]" :class="selfTestResults[pluginKey(plugin)].status === 'error' ? 'border-alert/25 bg-alert/5 text-alert' : selfTestResults[pluginKey(plugin)].status === 'running' ? 'border-blue/20 bg-blue-soft text-blue' : 'border-ink/15 bg-canvas text-ink/65'" role="status" aria-live="polite">{{ selfTestResults[pluginKey(plugin)].text }}</p>
                <template #actions>
                  <button v-if="targetFromInstalled(plugin)" type="button" class="button-secondary px-3" :disabled="hostBusy" @click="void openSettings(targetFromInstalled(plugin)!)">{{ t('settings.title') }}</button>
                  <button v-if="plugin.runtime" class="button-secondary px-3" :disabled="!activePluginKeys.has(pluginKey(plugin)) || hostBusy" :title="!activePluginKeys.has(pluginKey(plugin)) ? t('plugins.enableToTest') : undefined" @click="void runSelfTest(plugin)">{{ t('plugins.selfTest') }}</button>
                  <button class="px-3" :class="activePluginKeys.has(pluginKey(plugin)) ? 'button-primary' : 'button-secondary'" :disabled="!pluginCompatibility(plugin).compatible || hostBusy" @click="setPluginEnabled(plugin, !activePluginKeys.has(pluginKey(plugin)))">{{ activePluginKeys.has(pluginKey(plugin)) ? t('plugins.enabled') : t('plugins.enable') }}</button>
                  <button type="button" class="button-secondary px-3 text-alert" :disabled="hostBusy" :aria-label="t('plugins.uninstallVersion', { version: plugin.manifest.metadata.version })" @click="void uninstallPlugin(plugin)">{{ t('plugins.uninstall') }}</button>
                </template>
              </PluginStoreItem>
            </div>

            <section v-if="visibleSuperseded.length" class="mt-5 rounded-[6px] border border-alert/25 bg-alert/5 p-4" aria-labelledby="plugin-version-mismatch-title">
              <div class="flex items-start justify-between gap-4">
                <div>
                  <h4 id="plugin-version-mismatch-title" class="font-sans text-[8px] uppercase tracking-[0.16em] text-alert">{{ t('plugins.versionMismatchTitle') }}</h4>
                  <p class="mt-1 font-serif text-[9px] leading-[1.45] text-ink/60">{{ t('plugins.versionMismatchActionHint') }}</p>
                </div>
              </div>
              <div class="mt-3 space-y-2">
                <PluginStoreItem
                  v-for="plugin in visibleSuperseded"
                  :key="`superseded-${pluginKey(plugin)}`"
                  :name="plugin.manifest.metadata.name"
                  :version="plugin.manifest.metadata.version"
                  :kind="plugin.manifest.kind"
                  :description="plugin.manifest.metadata.description"
                >
                  <template #icon><span class="mt-0.5 text-alert">◇</span></template>
                  <template #status>
                    <span class="font-sans text-[7px] uppercase tracking-[0.12em] text-alert">{{ t('plugins.versionMismatch') }}</span>
                  </template>
                  <p class="mt-2 font-serif text-[9px] text-ink/55">{{ t('plugins.versionMismatchDetail', { version: currentVersionFor(plugin) }) }}</p>
                  <template #actions>
                    <button type="button" class="button-secondary px-3 text-alert" :disabled="hostBusy" :aria-label="t('plugins.uninstallVersion', { version: plugin.manifest.metadata.version })" @click="void uninstallPlugin(plugin)">{{ t('plugins.uninstall') }}</button>
                  </template>
                </PluginStoreItem>
              </div>
            </section>
          </section>
        </div>
      </div>
    </section>

    <PluginSettingsDialog
      v-if="settingsTarget"
      :target="settingsTarget"
      :draft="selectedDraft"
      :configured-secrets="selectedSnapshot?.configuredSecrets ?? {}"
      :loading="selectedSettingsLoading"
      :saving="settingsSaving || busy"
      :error="selectedSettingsError"
      :on-close="closeSettings"
      :on-save="saveSettings"
      :on-reset="resetSettings"
      :on-test-connection="settingsTarget.native ? testConnection : undefined"
      :on-uninstall="settingsTarget.uninstallable ? uninstallSelected : undefined"
    />
  </div>
</template>

<style scoped>
/* Shared plugin-store visual tokens remain in app/globals.css. */
</style>
