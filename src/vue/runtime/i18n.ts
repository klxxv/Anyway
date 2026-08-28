import {
  inject,
  onBeforeUnmount,
  onMounted,
  provide,
  reactive,
  type InjectionKey,
} from "vue";
import type { Locale, MessageKey } from "../../../app/i18n/catalog";
import { useRuntimeI18nStore } from "../stores/runtime-i18n";
import { usePluginHost as getPluginHost } from "./plugin-host";

export const localeStorageKey = "research-canvas.locale.v1";
export const LOCALE_STORAGE_KEY = localeStorageKey;

export type AvailableLocale = {
  locale: Locale;
  name: string;
  source: "builtin" | "plugin";
};

export type I18nValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (
    key: MessageKey,
    parameters?: Readonly<Record<string, string | number>>,
  ) => string;
  availableLocales: Array<AvailableLocale>;
};

export const i18nKey: InjectionKey<I18nValue> = Symbol("research-canvas.i18n");

type RuntimeI18nStore = ReturnType<typeof useRuntimeI18nStore>;

/** Creates the existing injection value from the Pinia i18n store. */
function createI18nValue(store: RuntimeI18nStore): I18nValue {
  return reactive<I18nValue>({
    get locale() {
      return store.resolvedLocale;
    },
    setLocale: store.setLocale,
    t: store.t,
    get availableLocales() {
      return store.availableLocales as Array<AvailableLocale>;
    },
  });
}

/**
 * Compatibility bridge for the existing provider API. The provider still
 * owns injection and DOM lifecycle, while the Pinia store owns UI language.
 */
export function provideI18n(): I18nValue {
  // Preserve the existing provider nesting contract: i18n requires the plugin
  // host context because plugin locale bundles are part of its catalog.
  getPluginHost();
  const store = useRuntimeI18nStore();
  const value = createI18nValue(store);
  provide(i18nKey, value);
  onMounted(() => store.start());
  onBeforeUnmount(() => store.stop());
  return value;
}

export function useI18n(): I18nValue {
  const value = inject(i18nKey, null);
  if (!value) throw new Error("useI18n must be used inside I18nProvider");
  return value;
}
