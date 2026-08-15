<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  clonePluginSettingsDraft,
  connectionTestActions,
  PLUGIN_TEST_ACTION_IDS,
  resolvePluginPrivateText,
  type HostPluginSettingDefinition,
  type PluginConnectionTestAction,
  type PluginSecretDraft,
  type PluginSettingsDialogProps,
  type PluginSettingsDraft,
  validatePluginSettingsDraft,
} from "./panel-types";
import { usePanelI18n } from "./panel-types";

const props = defineProps<PluginSettingsDialogProps>();
const { locale, t } = usePanelI18n();
const draft = ref<PluginSettingsDraft>(clonePluginSettingsDraft(props.draft));
const errors = ref<Record<string, string>>({});
const connectionTests = ref<Record<string, { status: "running" | "success" | "error"; message: string }>>({});
const editingSecrets = ref<Record<string, boolean>>({});
const advancedOpen = ref(credentialSourceOf(props.draft) === "environment");

watch(
  () => props.draft,
  (next) => {
    draft.value = clonePluginSettingsDraft(next);
    errors.value = {};
    editingSecrets.value = {};
    advancedOpen.value = credentialSourceOf(next) === "environment";
  },
  { deep: true },
);

const definitionsById = computed(() => new Map(props.target.definitions.map((definition) => [definition.id, definition])));
const hasDefinition = (id: string): boolean => definitionsById.value.has(id);
const valueFor = (id: string): unknown => draft.value[id];
const stringValueFor = (id: string): string => {
  const value = valueFor(id);
  return typeof value === "string" ? value : "";
};

function credentialSourceOf(sourceDraft: PluginSettingsDraft): string {
  const value = sourceDraft["credential-source"];
  return typeof value === "string" ? value : "host-secret";
}

const credentialSource = computed(() => credentialSourceOf(draft.value));
const isProviderSettings = computed(() => hasDefinition("api-url") && hasDefinition("api-format"));
const isPdfAgent = computed(() => props.target.reference.id.toLowerCase().includes("pdf") || hasDefinition("pdf-transport"));

const allProviderPresets = [
  { id: "deepseek", labelKey: "plugins.providerDeepSeek", fallback: "DeepSeek", url: "https://api.deepseek.com/anthropic", format: "anthropic", model: "deepseek-v4-flash", credentialEnvVar: "DEEPSEEK_API_KEY", pdfNative: false },
  { id: "kimi-cn", labelKey: "providers.kimiChina", fallback: "Kimi (China)", url: "https://api.moonshot.cn/v1", format: "openai", model: "kimi-k2.6", credentialEnvVar: "MOONSHOT_API_KEY", pdfNative: true },
  { id: "kimi-global", labelKey: "providers.kimiInternational", fallback: "Kimi (International)", url: "https://api.moonshot.ai/v1", format: "openai", model: "kimi-k2.6", credentialEnvVar: "MOONSHOT_API_KEY", pdfNative: true },
  { id: "anthropic", labelKey: "plugins.providerAnthropic", fallback: "Anthropic", url: "https://api.anthropic.com/v1/messages", format: "anthropic", model: "claude-3-5-sonnet-latest", credentialEnvVar: "ANTHROPIC_API_KEY", pdfNative: false },
  { id: "custom", labelKey: "providers.custom", fallback: "Custom service", url: "", format: "", model: "", credentialEnvVar: "", pdfNative: true },
] as const;

type ProviderPresetId = (typeof allProviderPresets)[number]["id"];
const providerPresets = computed(() => isPdfAgent.value
  ? allProviderPresets.filter((preset) => preset.pdfNative)
  : allProviderPresets);

const inferProvider = (): ProviderPresetId => {
  const url = stringValueFor("api-url").toLowerCase();
  if (url.includes("deepseek")) return "deepseek";
  if (url.includes("api.moonshot.cn")) return "kimi-cn";
  if (url.includes("api.moonshot.ai")) return "kimi-global";
  if (url.includes("anthropic")) return "anthropic";
  return "custom";
};
const provider = ref<ProviderPresetId>(inferProvider());

watch(
  () => props.draft,
  () => {
    provider.value = inferProvider();
  },
  { deep: true },
);

const basicIds = new Set(["api-key", "model", "thinking", "pdf-transport"]);
const advancedIds = new Set(["api-url", "api-format", "credential-source", "credential-env-var"]);
const basicDefinitions = computed(() => {
  if (!isProviderSettings.value) return props.target.definitions;
  const order = isPdfAgent.value
    ? ["api-key", "api-url", "model", "thinking", "pdf-transport"]
    : ["api-key", "model", "thinking", "pdf-transport"];
  return props.target.definitions
    .filter((definition) => basicIds.has(definition.id) || (isPdfAgent.value && definition.id === "api-url"))
    .sort((left, right) => order.indexOf(left.id) - order.indexOf(right.id));
});
const advancedDefinitions = computed(() => {
  if (!isProviderSettings.value) return [];
  return props.target.definitions.filter((definition) =>
    (advancedIds.has(definition.id) || definition.group === "connection" || definition.group === "advanced")
    && !basicIds.has(definition.id)
    && !(isPdfAgent.value && definition.id === "api-url"));
});

const updateAvailable = computed(() => {
  const latest = props.target.update?.latestVersion;
  return Boolean(latest && latest !== props.target.version);
});

const pluginText = (
  key: string | undefined,
  fallback: string,
  localI18n?: HostPluginSettingDefinition["i18n"],
): string => resolvePluginPrivateText(props.target, locale.value, key, fallback, localI18n);
const hostText = (key: string): string => t(key);

const labelFor = (definition: HostPluginSettingDefinition): string =>
  pluginText(definition.labelKey, definition.label, definition.i18n);
const descriptionFor = (definition: HostPluginSettingDefinition): string =>
  pluginText(definition.descriptionKey, definition.description ?? "", definition.i18n);
const placeholderFor = (definition: HostPluginSettingDefinition, fallback: string): string =>
  pluginText(definition.placeholderKey, definition.placeholder ?? fallback, definition.i18n);
const optionLabelFor = (definition: HostPluginSettingDefinition, option: { label: string; labelKey?: string }): string =>
  pluginText(option.labelKey, option.label, definition.i18n);

const setValue = (id: string, value: boolean | number | string): void => {
  draft.value = { ...draft.value, [id]: value };
};

const setTextValue = (id: string, event: Event): void => {
  setValue(id, (event.target as HTMLInputElement).value);
};

const setNumberValue = (id: string, event: Event): void => {
  const raw = (event.target as HTMLInputElement).value;
  setValue(id, raw === "" ? Number.NaN : Number(raw));
};

const setSelectValue = (id: string, event: Event): void => {
  setValue(id, (event.target as HTMLSelectElement).value);
};

const secretFor = (id: string): PluginSecretDraft => {
  const value = draft.value[id];
  return value && typeof value === "object" && "action" in value
    ? value as PluginSecretDraft
    : { action: props.configuredSecrets[id] ? "keep" : "clear", value: "" };
};

const secretConfigured = (id: string): boolean =>
  Boolean(props.configuredSecrets[id]) && secretFor(id).action !== "clear";
const secretEditing = (id: string): boolean => Boolean(editingSecrets.value[id]);

const editSecret = (id: string): void => {
  editingSecrets.value = { ...editingSecrets.value, [id]: true };
  draft.value = { ...draft.value, [id]: { action: "set", value: "" } };
};

const setSecretValue = (id: string, event: Event): void => {
  const value = (event.target as HTMLInputElement).value;
  draft.value = { ...draft.value, [id]: { action: "set", value } };
};

const deleteSecret = (id: string): void => {
  editingSecrets.value = { ...editingSecrets.value, [id]: false };
  draft.value = { ...draft.value, [id]: { action: "clear", value: "" } };
};

const applyProvider = (providerId: string): void => {
  const preset = providerPresets.value.find((candidate) => candidate.id === providerId);
  if (!preset) return;
  provider.value = preset.id;
  if (preset.url && hasDefinition("api-url")) setValue("api-url", preset.url);
  if (preset.format && hasDefinition("api-format")) setValue("api-format", preset.format);
  if (preset.model && hasDefinition("model")) setValue("model", preset.model);
  if (preset.credentialEnvVar && hasDefinition("credential-env-var")) {
    setValue("credential-env-var", preset.credentialEnvVar);
  }
  if (preset.id === "custom") advancedOpen.value = true;
};

const setCredentialSource = (event: Event): void => {
  setValue("credential-source", (event.target as HTMLSelectElement).value);
  advancedOpen.value = true;
};

const validate = (): boolean => {
  errors.value = validatePluginSettingsDraft(props.target.definitions, draft.value, props.configuredSecrets);
  return Object.keys(errors.value).length === 0;
};

const save = async (): Promise<void> => {
  if (!validate()) return;
  await props.onSave(clonePluginSettingsDraft(draft.value));
};

const reset = async (): Promise<void> => {
  errors.value = {};
  await props.onReset();
};

type UiTestAction = PluginConnectionTestAction & { hostLabelKey?: string; hostDescriptionKey?: string };

const providerConnection = computed(() => props.target.connections[0]);
const testActions = computed<UiTestAction[]>(() => {
  const connection = providerConnection.value;
  if (!connection) return [];
  const declared = connectionTestActions(connection);
  if (!isPdfAgent.value) return declared;
  const declaredById = new Map(declared.map((action) => [action.id, action]));
  return [
    {
      ...(declaredById.get(PLUGIN_TEST_ACTION_IDS.connection) ?? { id: PLUGIN_TEST_ACTION_IDS.connection, label: "", description: "" }),
      hostLabelKey: "plugins.testAiConnection",
      hostDescriptionKey: "plugins.testAiConnectionHint",
    },
    {
      ...(declaredById.get(PLUGIN_TEST_ACTION_IDS.pdfExtraction) ?? { id: PLUGIN_TEST_ACTION_IDS.pdfExtraction, label: "", description: "" }),
      hostLabelKey: "plugins.testPdfExtraction",
      hostDescriptionKey: "plugins.testPdfExtractionHint",
    },
  ];
});

const testResultKey = (action: UiTestAction): string => `${providerConnection.value?.id ?? "provider"}:${action.id}`;
const testActionLabel = (action: UiTestAction): string =>
  pluginText(
    action.labelKey,
    action.hostLabelKey ? hostText(action.hostLabelKey) : action.label,
  );
const testActionDescription = (action: UiTestAction): string =>
  pluginText(
    action.descriptionKey,
    action.hostDescriptionKey ? hostText(action.hostDescriptionKey) : action.description ?? "",
  );
const testResultMessage = (result: { code?: string; message: string }): string =>
  pluginText(result.code ? `results.${result.code}` : undefined, result.message);

const testConnection = async (action: UiTestAction): Promise<void> => {
  const connection = providerConnection.value;
  if (!connection || !props.onTestConnection) return;
  if (!validate()) return;
  const key = testResultKey(action);
  connectionTests.value = {
    ...connectionTests.value,
    [key]: { status: "running", message: t("plugins.connectionTestingAction") },
  };
  try {
    const callback = props.onTestConnection;
    const snapshot = clonePluginSettingsDraft(draft.value);
    const result = callback.length >= 3
      ? await (callback as (connectionId: string, actionId: string, draft: PluginSettingsDraft) => Promise<{ ok: boolean; message: string }>)(connection.id, action.id, snapshot)
      : await (callback as (connectionId: string, draft: PluginSettingsDraft) => Promise<{ ok: boolean; message: string }>)(connection.id, snapshot);
    connectionTests.value = {
      ...connectionTests.value,
      [key]: { status: result.ok ? "success" : "error", message: testResultMessage(result) },
    };
  } catch (cause) {
    connectionTests.value = {
      ...connectionTests.value,
      [key]: { status: "error", message: cause instanceof Error ? cause.message : String(cause) },
    };
  }
};

const shouldShowAdvancedDefinition = (definition: HostPluginSettingDefinition): boolean => {
  if (definition.id === "credential-env-var") return credentialSource.value === "environment";
  if (definition.id === "api-format") return isPdfAgent.value || provider.value === "custom";
  if (definition.id === "api-url") return provider.value === "custom";
  return true;
};
</script>

<template>
  <div class="fixed inset-0 z-[108] grid place-items-center bg-ink/15 px-4 backdrop-blur-[3px]" @wheel.stop @touchmove.stop>
    <section
      class="settings-dialog flex max-h-[min(820px,calc(100vh-32px))] w-[min(760px,100%)] flex-col overflow-hidden rounded-[8px] border border-ink/30 bg-paper shadow-[0_22px_80px_rgba(30,32,35,.2)]"
      role="dialog"
      aria-modal="true"
      aria-labelledby="plugin-settings-title"
    >
      <header class="flex shrink-0 items-start justify-between border-b border-ink/15 px-6 py-5">
        <div class="min-w-0">
          <span class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">{{ t('plugins.settingsEyebrow') }}</span>
          <h2 id="plugin-settings-title" class="mt-1 truncate font-serif text-[20px]">{{ pluginText('plugin.name', target.name) }}</h2>
          <p class="mt-1 font-serif text-[10px] text-ink/50">{{ t('plugins.settingsSubtitle') }}</p>
        </div>
        <button type="button" class="icon-quiet ml-4" :aria-label="t('plugins.closeSettings')" @click="props.onClose">×</button>
      </header>

      <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <section class="rounded-[6px] border border-ink/15 bg-canvas p-4" aria-labelledby="plugin-settings-title-inner">
          <div class="flex items-end justify-between gap-4">
            <div>
              <h3 id="plugin-settings-title-inner" class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/50">{{ t('plugins.userSettings') }}</h3>
              <p class="mt-1 font-serif text-[10px] text-ink/50">{{ t('plugins.settingsHostControlled') }}</p>
            </div>
            <span v-if="loading" class="font-sans text-[8px] uppercase tracking-[0.12em] text-blue">{{ t('plugins.settingsLoading') }}</span>
          </div>
          <div v-if="error" class="mt-3 rounded-[5px] border border-alert/25 bg-alert/5 px-3 py-2 font-serif text-[10px] text-alert" role="alert">{{ error }}</div>
        </section>

        <div v-if="!target.definitions.length" class="mt-4 rounded-[5px] border border-ink/15 px-4 py-5 font-serif text-[10px] text-ink/50">{{ t('plugins.noSettings') }}</div>

        <template v-else>
          <section v-if="isProviderSettings && !isPdfAgent" class="mt-4 space-y-3" aria-labelledby="plugin-connection-settings-title">
            <h3 id="plugin-connection-settings-title" class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/50">{{ t('plugins.provider') }}</h3>
            <div class="rounded-[6px] border border-ink/15 p-4">
              <label for="plugin-provider" class="font-serif text-[12px] text-ink/85">{{ pluginText(providerConnection?.labelKey, providerConnection?.label ?? t('plugins.provider')) }}</label>
              <select id="plugin-provider" class="mt-3 w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="provider" @change="applyProvider(($event.target as HTMLSelectElement).value)">
                <option v-for="preset in providerPresets" :key="preset.id" :value="preset.id">{{ pluginText(preset.labelKey, preset.labelKey.startsWith('plugins.') ? hostText(preset.labelKey) : preset.fallback) }}</option>
              </select>
            </div>
          </section>

          <section class="mt-4 space-y-3" aria-labelledby="plugin-basic-settings-title">
            <h3 id="plugin-basic-settings-title" class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/50">{{ isProviderSettings ? t('plugins.apiKey') : t('plugins.userSettings') }}</h3>
            <div v-for="definition in basicDefinitions" :key="definition.id" v-show="definition.id !== 'api-key' || credentialSource !== 'environment'" class="rounded-[6px] border border-ink/15 p-4">
              <template v-if="definition.id === 'api-key' && definition.type === 'secret'">
                <div class="flex items-start justify-between gap-4">
                  <div class="min-w-0">
                    <label :for="`plugin-setting-${definition.id}`" class="font-serif text-[12px] text-ink/85">{{ labelFor(definition) }}</label>
                    <p class="mt-1 font-serif text-[9px] leading-[1.45] text-ink/50">{{ descriptionFor(definition) || t('plugins.apiKeySessionNote') }}</p>
                  </div>
                  <span v-if="secretConfigured(definition.id) && !secretEditing(definition.id)" class="rounded-full bg-emerald-500/10 px-2 py-1 font-sans text-[8px] text-emerald-700">{{ t('plugins.apiKeyConfigured') }}</span>
                </div>
                <div class="mt-3 flex flex-wrap items-center gap-2">
                  <input
                    v-if="!secretConfigured(definition.id) || secretEditing(definition.id)"
                    :id="`plugin-setting-${definition.id}`"
                    type="password"
                    autocomplete="new-password"
                    class="min-w-[220px] flex-1 rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-mono text-[11px] outline-none focus:border-blue/60"
                    :value="secretFor(definition.id).value"
                    :placeholder="placeholderFor(definition, t('plugins.apiKeyPlaceholder'))"
                    @input="setSecretValue(definition.id, $event)"
                  >
                  <button v-if="secretConfigured(definition.id) && !secretEditing(definition.id)" type="button" class="button-secondary px-3" @click="editSecret(definition.id)">{{ t('plugins.changeApiKey') }}</button>
                  <button v-if="secretConfigured(definition.id)" type="button" class="button-secondary px-3 text-alert" @click="deleteSecret(definition.id)">{{ t('plugins.deleteApiKey') }}</button>
                </div>
              </template>

              <template v-else>
                <div class="flex items-start justify-between gap-4">
                  <div class="min-w-0">
                    <label :for="`plugin-setting-${definition.id}`" class="font-serif text-[12px] text-ink/85">{{ labelFor(definition) }}</label>
                    <p v-if="descriptionFor(definition)" class="mt-1 font-serif text-[9px] leading-[1.45] text-ink/50">{{ descriptionFor(definition) }}</p>
                  </div>
                </div>
                <div class="mt-3">
                  <select v-if="definition.id === 'pdf-transport'" :id="`plugin-setting-${definition.id}`" class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="stringValueFor(definition.id)" @change="setSelectValue(definition.id, $event)">
                    <option v-for="option in definition.options" :key="option.value" :value="option.value">{{ optionLabelFor(definition, option) }}</option>
                  </select>
                  <select v-else-if="definition.type === 'select'" :id="`plugin-setting-${definition.id}`" class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="stringValueFor(definition.id)" @change="setSelectValue(definition.id, $event)">
                    <option v-for="option in definition.options" :key="option.value" :value="option.value">{{ optionLabelFor(definition, option) }}</option>
                  </select>
                  <input v-else-if="definition.type === 'number'" :id="`plugin-setting-${definition.id}`" type="number" class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-mono text-[11px] outline-none focus:border-blue/60" :value="typeof valueFor(definition.id) === 'number' && Number.isFinite(valueFor(definition.id)) ? valueFor(definition.id) : ''" :min="definition.min" :max="definition.max" :step="definition.step" @input="setNumberValue(definition.id, $event)">
                  <input v-else-if="definition.type === 'boolean'" :id="`plugin-setting-${definition.id}`" type="checkbox" class="h-4 w-4 accent-blue" :checked="valueFor(definition.id) === true" @change="setValue(definition.id, ($event.target as HTMLInputElement).checked)">
                  <input v-else :id="`plugin-setting-${definition.id}`" type="text" class="w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="stringValueFor(definition.id)" :placeholder="placeholderFor(definition, '')" @input="setTextValue(definition.id, $event)">
                  <p v-if="definition.id === 'pdf-transport'" class="mt-2 font-serif text-[9px] leading-[1.45] text-ink/50">{{ descriptionFor(definition) }}</p>
                  <p v-if="errors[definition.id]" class="mt-2 font-serif text-[9px] text-alert" role="alert">{{ errors[definition.id] }}</p>
                </div>
              </template>
            </div>
          </section>

          <details v-if="isProviderSettings" class="mt-4 rounded-[6px] border border-ink/15" :open="advancedOpen" @toggle="advancedOpen = ($event.target as HTMLDetailsElement).open">
            <summary class="cursor-pointer list-none px-4 py-3 font-serif text-[12px] text-ink/80">{{ t('plugins.advancedSettings') }}</summary>
            <div class="space-y-3 border-t border-ink/10 p-4">
              <div v-for="definition in advancedDefinitions" :key="definition.id" v-show="shouldShowAdvancedDefinition(definition)" class="rounded-[5px] border border-ink/10 p-3">
                <label :for="`plugin-setting-${definition.id}`" class="font-serif text-[11px] text-ink/85">{{ labelFor(definition) }}</label>
                <p v-if="descriptionFor(definition)" class="mt-1 font-serif text-[9px] text-ink/50">{{ descriptionFor(definition) }}</p>
                <select v-if="definition.id === 'credential-source' || definition.id === 'api-format' || definition.type === 'select'" :id="`plugin-setting-${definition.id}`" class="mt-2 w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="stringValueFor(definition.id)" @change="definition.id === 'credential-source' ? setCredentialSource($event) : setSelectValue(definition.id, $event)">
                  <option v-for="option in definition.options" :key="option.value" :value="option.value">{{ optionLabelFor(definition, option) }}</option>
                </select>
                <input v-else :id="`plugin-setting-${definition.id}`" type="text" class="mt-2 w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="stringValueFor(definition.id)" :placeholder="placeholderFor(definition, '')" @input="setTextValue(definition.id, $event)">
                <p v-if="definition.id === 'credential-env-var'" class="mt-2 rounded-[4px] border border-ink/10 bg-canvas px-3 py-2 font-serif text-[9px] text-ink/55" role="status">{{ t('plugins.environmentWillCheck') }}</p>
                <p v-if="errors[definition.id]" class="mt-2 font-serif text-[9px] text-alert" role="alert">{{ errors[definition.id] }}</p>
              </div>
            </div>
          </details>

          <section v-if="!isProviderSettings && advancedDefinitions.length === 0" class="mt-4 space-y-3">
            <div v-for="definition in target.definitions.filter((candidate) => !basicIds.has(candidate.id))" :key="definition.id" class="rounded-[6px] border border-ink/15 p-4">
              <label :for="`plugin-setting-${definition.id}`" class="font-serif text-[12px] text-ink/85">{{ labelFor(definition) }}</label>
              <p v-if="descriptionFor(definition)" class="mt-1 font-serif text-[9px] text-ink/50">{{ descriptionFor(definition) }}</p>
              <input v-if="definition.type === 'text'" :id="`plugin-setting-${definition.id}`" type="text" class="mt-3 w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="stringValueFor(definition.id)" :placeholder="placeholderFor(definition, '')" @input="setTextValue(definition.id, $event)">
              <select v-else-if="definition.type === 'select'" :id="`plugin-setting-${definition.id}`" class="mt-3 w-full rounded-[4px] border border-ink/20 bg-paper px-3 py-2 font-serif text-[11px] outline-none focus:border-blue/60" :value="stringValueFor(definition.id)" @change="setSelectValue(definition.id, $event)"><option v-for="option in definition.options" :key="option.value" :value="option.value">{{ optionLabelFor(definition, option) }}</option></select>
            </div>
          </section>

          <section v-if="testActions.length && props.onTestConnection" class="mt-5 rounded-[6px] border border-blue/20 bg-blue-soft/30 p-4" aria-labelledby="plugin-connection-actions-title">
            <h3 id="plugin-connection-actions-title" class="font-sans text-[8px] uppercase tracking-[0.16em] text-blue">{{ t('plugins.testActions') }}</h3>
            <div class="mt-3 grid gap-3 sm:grid-cols-2">
              <div v-for="action in testActions" :key="action.id" class="rounded-[5px] border border-blue/15 bg-paper px-3 py-3">
                <p class="font-serif text-[11px] text-ink/80">{{ testActionLabel(action) }}</p>
                <p class="mt-1 min-h-[28px] font-serif text-[9px] leading-[1.4] text-ink/50">{{ testActionDescription(action) }}</p>
                <p v-if="action.id === PLUGIN_TEST_ACTION_IDS.pdfExtraction && stringValueFor('pdf-transport') === 'kimi-file-extract'" class="mt-2 font-serif text-[9px] text-amber-700">{{ t('plugins.testPdfRemoteWarning') }}</p>
                <p v-if="connectionTests[testResultKey(action)]" class="mt-2 font-serif text-[9px]" :class="connectionTests[testResultKey(action)].status === 'error' ? 'text-alert' : connectionTests[testResultKey(action)].status === 'running' ? 'text-blue' : 'text-emerald-700'" role="status" aria-live="polite">{{ connectionTests[testResultKey(action)].message }}</p>
                <button type="button" class="button-secondary mt-3 w-full px-3" :disabled="saving || loading || connectionTests[testResultKey(action)]?.status === 'running'" @click="void testConnection(action)">{{ connectionTests[testResultKey(action)]?.status === 'running' ? t('plugins.connectionTestingAction') : testActionLabel(action) }}</button>
              </div>
            </div>
          </section>
        </template>

        <section class="mt-5 rounded-[6px] border border-ink/15 bg-canvas p-4" aria-labelledby="plugin-host-actions-title">
          <h3 id="plugin-host-actions-title" class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/50">{{ t('plugins.hostActions') }}</h3>
          <div class="mt-3 flex flex-wrap items-center gap-2">
            <button type="button" class="button-secondary px-3" disabled :title="t('plugins.updateUnavailableHint')">{{ t('plugins.update') }}</button>
            <button v-if="target.uninstallable && props.onUninstall" type="button" class="button-secondary px-3 text-alert" :disabled="saving || loading" @click="void props.onUninstall?.()">{{ t('plugins.uninstall') }}</button>
            <span class="font-serif text-[9px] text-ink/45">{{ updateAvailable ? t('plugins.updateAvailable', { version: target.update?.latestVersion ?? '' }) : t('plugins.updateUnavailableHint') }}</span>
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

.settings-dialog summary::-webkit-details-marker {
  display: none;
}
</style>
