import { computed, ref, watch } from "vue";
import { defineStore } from "pinia";
import {
  normalizeLocale,
  translate,
  type Locale,
  type MessageKey,
} from "../../../app/i18n/catalog";
import { localeBundlesFromPlugins } from "../../../app/plugins/workspace";
import { useRuntimePluginHostStore } from "./runtime-plugin-host";

export const runtimeLocaleStorageKey = "research-canvas.locale.v1";

export type RuntimeAvailableLocale = {
  locale: Locale;
  name: string;
  source: "builtin" | "plugin";
};

type PluginCatalog = Record<string, Partial<Record<MessageKey, string>>>;

const protectedMessageKeys = new Set<MessageKey>([
  "agent.eyebrow",
  "agent.reviewTitle",
  "agent.reviewSubtitle",
  "agent.acceptAll",
  "agent.rejectAll",
  "agent.applySelected",
  "agent.decision.accept",
  "agent.decision.reject",
  "agent.patchApplied",
  "agent.patchRejected",
  "agent.compileFailed",
  "workspace.reviewApplyPatch",
  "workspace.patchApplied",
  "workspace.noPatch",
  "menu.close",
]);

function persistLocale(nextLocale: Locale): void {
  if (typeof window !== "undefined") {
    window.localStorage.setItem(runtimeLocaleStorageKey, nextLocale);
  }
  if (typeof document !== "undefined") {
    document.documentElement.lang = nextLocale;
  }
}

/** Pinia setup store for device-local UI language and plugin locale bundles. */
export const useRuntimeI18nStore = defineStore("runtime-i18n", () => {
  const pluginHost = useRuntimePluginHostStore();
  const locale = ref<Locale>("en");

  const pluginData = computed(() => {
    const catalog: PluginCatalog = {};
    const names: Record<string, string> = {};
    for (const bundle of localeBundlesFromPlugins(pluginHost.activePlugins)) {
      const messages = {
        ...(bundle.messages as Partial<Record<MessageKey, string>>),
      };
      for (const key of Object.keys(messages)) {
        if (protectedMessageKeys.has(key as MessageKey)) {
          delete messages[key as MessageKey];
        }
      }
      catalog[bundle.locale] = {
        ...(catalog[bundle.locale] ?? {}),
        ...messages,
      };
      names[bundle.locale] = bundle.name;
    }
    return { catalog, names };
  });

  const resolvedLocale = computed<Locale>(() => {
    const current = locale.value;
    return current === "en" || current === "zh-CN" || pluginData.value.catalog[current]
      ? current
      : "en";
  });

  const availableLocales = computed<RuntimeAvailableLocale[]>(() => [
    { locale: "en", name: "English", source: "builtin" },
    { locale: "zh-CN", name: "简体中文", source: "builtin" },
    ...Object.entries(pluginData.value.names)
      .filter(([candidate]) => candidate !== "en" && candidate !== "zh-CN")
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([candidate, name]) => ({
        locale: candidate,
        name,
        source: "plugin" as const,
      })),
  ]);

  const setLocale = (nextLocale: Locale): void => {
    locale.value = nextLocale;
    persistLocale(nextLocale);
  };

  const t = (
    key: MessageKey,
    parameters?: Readonly<Record<string, string | number>>,
  ): string => translate(resolvedLocale.value, key, pluginData.value.catalog, parameters);

  let localeFrame: number | null = null;

  const start = (): void => {
    if (typeof window === "undefined" || localeFrame !== null) return;
    localeFrame = window.requestAnimationFrame(() => {
      localeFrame = null;
      const saved = window.localStorage.getItem(runtimeLocaleStorageKey);
      locale.value = normalizeLocale(saved ?? window.navigator.language);
      document.documentElement.lang = resolvedLocale.value;
    });
  };

  const stop = (): void => {
    if (typeof window === "undefined" || localeFrame === null) return;
    window.cancelAnimationFrame(localeFrame);
    localeFrame = null;
  };

  watch(resolvedLocale, (nextLocale) => {
    if (typeof document !== "undefined") {
      document.documentElement.lang = nextLocale;
    }
  });

  return {
    locale,
    resolvedLocale,
    availableLocales,
    setLocale,
    t,
    start,
    stop,
  };
});
