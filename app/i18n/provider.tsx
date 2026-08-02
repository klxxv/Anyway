"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import {
  normalizeLocale,
  translate,
  type Locale,
  type MessageKey,
} from "./catalog";
import { localeBundlesFromPlugins } from "../plugins/workspace";
import { usePluginHost } from "../plugins/plugin-host";

const localeStorageKey = "research-canvas.locale.v1";

type I18nValue = {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: MessageKey, parameters?: Readonly<Record<string, string | number>>) => string;
  availableLocales: Array<{ locale: Locale; name: string; source: "builtin" | "plugin" }>;
};

const I18nContext = createContext<I18nValue | null>(null);

/**
 * Owns device-local UI language while leaving research content untouched.
 * 管理设备本地界面语言，不隐式翻译研究内容。
 */
export function I18nProvider({ children }: { children: ReactNode }) {
  const { activePlugins } = usePluginHost();
  const [locale, setLocaleState] = useState<Locale>("en");

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const saved = window.localStorage.getItem(localeStorageKey);
      setLocaleState(normalizeLocale(saved ?? window.navigator.language));
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);

  const { pluginCatalog, pluginLocaleNames } = useMemo(() => {
    const bundles = localeBundlesFromPlugins(activePlugins);
    const catalog: Record<string, Partial<Record<MessageKey, string>>> = {};
    const names: Record<string, string> = {};
    for (const bundle of bundles) {
      catalog[bundle.locale] = {
        ...(catalog[bundle.locale] ?? {}),
        ...(bundle.messages as Partial<Record<MessageKey, string>>),
      };
      names[bundle.locale] = bundle.name;
    }
    return { pluginCatalog: catalog, pluginLocaleNames: names };
  }, [activePlugins]);

  const resolvedLocale =
    locale === "en" || locale === "zh-CN" || pluginCatalog[locale] ? locale : "en";

  const setLocale = useCallback((nextLocale: Locale) => {
    setLocaleState(nextLocale);
    window.localStorage.setItem(localeStorageKey, nextLocale);
    document.documentElement.lang = nextLocale;
  }, []);

  useEffect(() => {
    document.documentElement.lang = resolvedLocale;
  }, [resolvedLocale]);

  const availableLocales = useMemo<I18nValue["availableLocales"]>(
    () => [
      { locale: "en", name: "English", source: "builtin" },
      { locale: "zh-CN", name: "简体中文", source: "builtin" },
      ...Object.entries(pluginLocaleNames)
        .filter(([candidate]) => candidate !== "en" && candidate !== "zh-CN")
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([candidate, name]) => ({
          locale: candidate,
          name,
          source: "plugin" as const,
        })),
    ],
    [pluginLocaleNames],
  );

  const value = useMemo<I18nValue>(
    () => ({
      locale: resolvedLocale,
      setLocale,
      availableLocales,
      t: (key, parameters) => translate(resolvedLocale, key, pluginCatalog, parameters),
    }),
    [availableLocales, pluginCatalog, resolvedLocale, setLocale],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n must be used inside I18nProvider");
  return value;
}
