<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  clonePluginSettingsDraft,
  type PluginSecretDraft,
  type PluginSettingsDialogProps,
  type PluginSettingsDraft,
  validatePluginSettingsDraft,
} from "./panel-types";
import { usePanelI18n } from "./panel-types";

const props = defineProps<PluginSettingsDialogProps>();
const { t } = usePanelI18n();
const draft = ref<PluginSettingsDraft>(clonePluginSettingsDraft(props.draft));
const errors = ref<Record<string, string>>({});

watch(
  () => props.draft,
  (next) => {
    draft.value = clonePluginSettingsDraft(next);
    errors.value = {};
  },
  { deep: true },
);

const updateAvailable = computed(() => {
  const latest = props.target.update?.latestVersion;
  return Boolean(latest && latest !== props.target.version);
});

const valueFor = (id: string): unknown => draft.value[id];

const secretFor = (id: string): PluginSecretDraft => {
  const value = draft.value[id];
  return value && typeof value === "object" && "action" in value
    ? value as PluginSecretDraft
    : { action: "keep", value: "" };
};

const setValue = (id: string, value: boolean | number | string): void => {
  draft.value = { ...draft.value, [id]: value };
};

const setSecretAction = (id: string, action: PluginSecretDraft["action"]): void => {
  draft.value = { ...draft.value, [id]: { action, value: "" } };
};

const setSecretValue = (id: string, event: Event): void => {
  const value = (event.target as HTMLInputElement).value;
  draft.value = { ...draft.value, [id]: { action: "set", value } };
};

const setNumberValue = (id: string, event: Event): void => {
  const raw = (event.target as HTMLInputElement).value;
  setValue(id, raw === "" ? Number.NaN : Number(raw));
};

const setTextValue = (id: string, event: Event): void => {
  setValue(id, (event.target as HTMLInputElement).value);
};

const setSelectValue = (id: string, event: Event): void => {
  setValue(id, (event.target as HTMLSelectElement).value);
};

const validate = (): boolean => {
  errors.value = validatePluginSettingsDraft(props.target.definitions, draft.value, props.configuredSecrets);
  return Object.keys(errors.value).length === 0;
};

const save = async () => {
  if (!validate()) return;
  await props.onSave(clonePluginSettingsDraft(draft.value));
};

const reset = async () => {
  errors.value = {};
  await props.onReset();
};
</script>

<template>
  <div class="fixed inset-0 z-[108] grid place-items-center bg-ink/15 px-4 backdrop-blur-[3px]">
    <section
      class="settings-dialog flex max-h-[min(760px,calc(100vh-32px))] w-[min(720px,100%)] flex-col overflow-hidden rounded-[8px] border border-ink/30 bg-paper shadow-[0_22px_80px_rgba(30,32,35,.2)]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="plugin-settings-title"
    >
      <header class="flex shrink-0 items-start justify-between border-b border-ink/15 px-6 py-5">
        <div class="min-w-0">
          <span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">{{ t('plugins.settingsEyebrow') }}</span>
          <h2 id="plugin-settings-title" class="mt-1 truncate font-serif text-[20px]">{{ target.name }}</h2>
          <p class="mt-1 font-serif text-[10px] text-ink/50">{{ t('plugins.settingsSubtitle') }}</p>
        </div>
        <button type="button" class="icon-quiet ml-4" :aria-label="t('plugins.closeSettings')" @click="props.onClose">×</button>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <section class="rounded-[6px] border border-ink/15 bg-canvas p-4" aria-labelledby="plugin-metadata-title">
          <div class="flex items-center justify-between gap-4">
            <h3 id="plugin-metadata-title" class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/50">{{ t('plugins.hostMetadata') }}</h3>
            <span class="font-sans text-[8px] uppercase tracking-[0.12em] text-ink/40">{{ target.source === 'installed' ? t('plugins.installed') : t('plugins.builtIn') }}</span>
          </div>
          <dl class="mt-3 grid grid-cols-[minmax(100px,0.7fr)_minmax(0,1.3fr)] gap-x-5 gap-y-2 font-serif text-[10px]">
            <dt class="text-ink/45">{{ t('plugins.pluginId') }}</dt><dd class="break-all font-mono text-[9px] text-ink/75">{{ target.reference.id }}</dd>
            <dt class="text-ink/45">{{ t('plugins.version') }}</dt><dd class="text-ink/75">{{ target.version }}</dd>
            <dt class="text-ink/45">{{ t('plugins.developer') }}</dt><dd class="text-ink/75">{{ target.developer || target.publisher || t('plugins.unknownDeveloper') }}</dd>
            <template v-if="target.developerUuid">
              <dt class="text-ink/45">{{ t('plugins.developerUuid') }}</dt><dd class="break-all font-mono text-[9px] text-ink/75">{{ target.developerUuid }}</dd>
            </template>
            <dt class="text-ink/45">{{ t('plugins.updateStatus') }}</dt>
            <dd class="text-ink/75">
              <span v-if="updateAvailable">{{ t('plugins.updateAvailable', { version: target.update?.latestVersion ?? '' }) }}</span>
              <span v-else>{{ t('plugins.upToDate') }}</span>
            </dd>
          </dl>
        </section>

        <section class="mt-5" aria-labelledby="plugin-settings-title-inner">
          <div class="flex items-end justify-between gap-4">
            <div>
              <h3 id="plugin-settings-title-inner" class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/50">{{ t('plugins.userSettings') }}</h3>
              <p class="mt-1 font-serif text-[10px] text-ink/50">{{ t('plugins.settingsHostControlled') }}</p>
            </div>
            <span v-if="loading" class="font-sans text-[8px] uppercase tracking-[0.12em] text-blue">{{ t('plugins.settingsLoading') }}</span>
          </div>

          <div v-if="error" class="mt-3 rounded-[5px] border border-alert/25 bg-alert/5 px-3 py-2 font-serif text-[10px] text-alert" role="alert">{{ error }}</div>
          <div v-if="!target.definitions.length" class="mt-3 rounded-[5px] border border-ink/15 px-4 py-5 font-serif text-[10px] text-ink/50">{{ t('plugins.noSettings') }}</div>
          <div v-else class="mt-3 space-y-3">
            <div v-for="definition in target.definitions" :key="definition.id" class="rounded-[6px] border border-ink/15 p-4">
              <div class="flex items-start justify-between gap-4">
                <div class="min-w-0">
                  <label :for="`plugin-setting-${definition.id}`" class="font-serif text-[12px] text-ink/85">{{ definition.label }}</label>
                  <p v-if="definition.description" class="mt-1 font-serif text-[9px] leading-[1.45] text-ink/50">{{ definition.description }}</p>
                </div>
                <span class="shrink-0 font-sans text-[7px] uppercase tracking-[0.12em] text-ink/35">{{ definition.type }}</span>
              </div>

              <div class="mt-3">
                <input
                  v-if="definition.type === 'boolean'"
                  :id="`plugin-setting-${definition.id}`"
                  type="checkbox"
                  class="h-4 w-4 accent-blue"
                  :checked="valueFor(definition.id) === true"
                  @change="setValue(definition.id, ($event.target as HTMLInputElement).checked)"
                >
                <input
                  v-else-if="definition.type === 'number'"
                  :id="`plugin-setting-${definition.id}`"
                  type="number"
                  class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-mono text-[11px] outline-none focus:border-blue/60"
                  :value="typeof valueFor(definition.id) === 'number' && Number.isFinite(valueFor(definition.id)) ? valueFor(definition.id) : ''"
                  :min="definition.min"
                  :max="definition.max"
                  :step="definition.step"
                  @input="setNumberValue(definition.id, $event)"
                >
                <input
                  v-else-if="definition.type === 'text'"
                  :id="`plugin-setting-${definition.id}`"
                  type="text"
                  class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60"
                  :value="typeof valueFor(definition.id) === 'string' ? valueFor(definition.id) : ''"
                  :placeholder="definition.placeholder"
                  @input="setTextValue(definition.id, $event)"
                >
                <select
                  v-else-if="definition.type === 'select'"
                  :id="`plugin-setting-${definition.id}`"
                  class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60"
                  :value="typeof valueFor(definition.id) === 'string' ? valueFor(definition.id) : ''"
                  @change="setSelectValue(definition.id, $event)"
                >
                  <option v-for="option in definition.options" :key="option.value" :value="option.value">{{ option.label }}</option>
                </select>
                <div v-else class="space-y-2">
                  <div class="flex flex-wrap items-center gap-2">
                    <span class="rounded-full bg-ink/5 px-2 py-1 font-sans text-[8px] text-ink/55">
                      {{ configuredSecrets[definition.id] ? t('plugins.secretConfigured') : t('plugins.secretNotConfigured') }}
                    </span>
                    <span class="font-serif text-[9px] text-ink/45">{{ t('plugins.secretNeverShown') }}</span>
                  </div>
                  <select
                    :id="`plugin-setting-${definition.id}`"
                    class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60"
                    :value="secretFor(definition.id).action"
                    @change="setSecretAction(definition.id, ($event.target as HTMLSelectElement).value as PluginSecretDraft['action'])"
                  >
                    <option value="keep">{{ t('plugins.secretKeep') }}</option>
                    <option value="set">{{ t('plugins.secretReplace') }}</option>
                    <option value="clear">{{ t('plugins.secretClear') }}</option>
                  </select>
                  <input
                    v-if="secretFor(definition.id).action === 'set'"
                    type="password"
                    autocomplete="new-password"
                    class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-mono text-[11px] outline-none focus:border-blue/60"
                    :value="secretFor(definition.id).value"
                    :placeholder="definition.placeholder || t('plugins.secretPlaceholder')"
                    @input="setSecretValue(definition.id, $event)"
                  >
                </div>
                <p v-if="errors[definition.id]" class="mt-2 font-serif text-[9px] text-alert" role="alert">{{ errors[definition.id] }}</p>
              </div>
            </div>
          </div>
        </section>

        <section class="mt-5 rounded-[6px] border border-ink/15 bg-canvas p-4" aria-labelledby="plugin-actions-title">
          <h3 id="plugin-actions-title" class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/50">{{ t('plugins.hostActions') }}</h3>
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <button type="button" class="button-secondary px-3" disabled :title="t('plugins.updateUnavailableHint')">{{ t('plugins.update') }}</button>
            <button v-if="target.uninstallable && props.onUninstall" type="button" class="button-secondary px-3 text-alert" :disabled="saving || loading" @click="void props.onUninstall?.()">{{ t('plugins.uninstall') }}</button>
            <span class="font-serif text-[9px] text-ink/45">{{ t('plugins.updateUnavailableHint') }}</span>
          </div>
        </section>
      </div>

      <footer class="flex shrink-0 items-center justify-between gap-3 border-t border-ink/15 px-6 py-4">
        <button type="button" class="button-secondary px-3" :disabled="saving || loading" @click="void reset()">{{ t('plugins.resetDefaults') }}</button>
        <div class="flex items-center gap-2">
          <button type="button" class="button-secondary px-3" :disabled="saving" @click="props.onClose">{{ t('settings.cancel') }}</button>
          <button type="button" class="button-primary px-4" :disabled="saving || loading" @click="void save()">{{ saving ? t('plugins.settingsSaving') : t('settings.save') }}</button>
        </div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
.settings-dialog :focus-visible {
  outline: 2px solid rgb(72 102 218 / 45%);
  outline-offset: 2px;
}
</style>
