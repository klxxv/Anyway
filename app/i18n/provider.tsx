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
import {
  pluginsChangedEvent,
  readEnabledPluginKeys,
} from "../plugins/context-menu";
import { listInstalledMycPlugins } from "../plugins/tauri-client";
import type { InstalledMycPlugin } from "../plugins/contracts";
import { localeBundlesFromPlugins } from "../plugins/workspace";

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
  const [locale, setLocaleState] = useState<Locale>("en");
  const [pluginCatalog, setPluginCatalog] = useState<
    Record<string, Partial<Record<MessageKey, string>>>
  >({});
  const [pluginLocaleNames, setPluginLocaleNames] = useState<Record<string, string>>({});

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      const saved = window.localStorage.getItem(localeStorageKey);
      setLocaleState(normalizeLocale(saved ?? window.navigator.language));
    });
    return () => window.cancelAnimationFrame(frame);
  }, []);

  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      let plugins: InstalledMycPlugin[];
      try {
        plugins = await listInstalledMycPlugins();
      } catch {
        plugins = [];
      }
      if (cancelled) return;
      const bundles = localeBundlesFromPlugins(plugins, readEnabledPluginKeys());
      const catalog: Record<string, Partial<Record<MessageKey, string>>> = {};
      const names: Record<string, string> = {};
      for (const bundle of bundles) {
        catalog[bundle.locale] = {
          ...(catalog[bundle.locale] ?? {}),
          ...(bundle.messages as Partial<Record<MessageKey, string>>),
        };
        names[bundle.locale] = bundle.name;
      }
      setPluginCatalog(catalog);
      setPluginLocaleNames(names);
      const saved = window.localStorage.getItem(localeStorageKey);
      if (saved && (saved === "en" || saved === "zh-CN" || catalog[saved])) {
        setLocaleState(saved);
      }
    };
    void refresh();
    window.addEventListener(pluginsChangedEvent, refresh);
    return () => {
      cancelled = true;
      window.removeEventListener(pluginsChangedEvent, refresh);
    };
  }, []);

  const setLocale = useCallback((nextLocale: Locale) => {
    setLocaleState(nextLocale);
    window.localStorage.setItem(localeStorageKey, nextLocale);
    document.documentElement.lang = nextLocale;
  }, []);

  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);

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
      locale,
      setLocale,
      availableLocales,
      t: (key, parameters) => translate(locale, key, pluginCatalog, parameters),
    }),
    [availableLocales, locale, pluginCatalog, setLocale],
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  const value = useContext(I18nContext);
  if (!value) throw new Error("useI18n must be used inside I18nProvider");
  return value;
}
