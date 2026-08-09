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
import { pluginReference, type InstalledMycPlugin } from "./contracts";
import {
  enabledPluginsStorageKey,
  pluginsChangedEvent,
  readEnabledPluginKeys,
} from "./context-menu";
import {
  activePlugins,
  enableLatestPluginKeys,
  migrateEnabledPluginKeys,
  pluginCompatibility,
  pluginKey,
  updateEnabledPluginKeys,
} from "./identity";
import {
  installMycPlugin,
  listInstalledMycPlugins,
  uninstallMycPlugin,
} from "./tauri-client";

type PluginHostValue = {
  installedPlugins: InstalledMycPlugin[];
  activePlugins: InstalledMycPlugin[];
  enabledPluginKeys: ReadonlySet<string>;
  activePluginKeys: ReadonlySet<string>;
  loading: boolean;
  error: string;
  refresh: () => Promise<void>;
  install: (path: string) => Promise<InstalledMycPlugin>;
  setPluginEnabled: (plugin: InstalledMycPlugin, enabled: boolean) => void;
  enableAll: () => void;
  removeIncompatible: () => Promise<number>;
};

const PluginHostContext = createContext<PluginHostValue | null>(null);

function writeEnabledPluginKeys(keys: ReadonlySet<string>) {
  window.localStorage.setItem(enabledPluginsStorageKey, JSON.stringify([...keys].sort()));
  // Compatibility event for extensions that observe the documented event.
  // The host itself does not subscribe, preventing synchronous render re-entry.
  window.dispatchEvent(new CustomEvent(pluginsChangedEvent));
}

/**
 * Single lifecycle owner for discovery, activation, version arbitration, and
 * installation. Every UI and contribution selector consumes this snapshot.
 */
export function PluginHostProvider({ children }: { children: ReactNode }) {
  const [installedPlugins, setInstalledPlugins] = useState<InstalledMycPlugin[]>([]);
  const [enabledPluginKeys, setEnabledPluginKeys] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const plugins = await listInstalledMycPlugins();
      const storedKeys = new Set(readEnabledPluginKeys());
      const migratedKeys = migrateEnabledPluginKeys(plugins, storedKeys);
      setInstalledPlugins(plugins);
      setEnabledPluginKeys(migratedKeys);
      if (
        migratedKeys.size !== storedKeys.size ||
        [...migratedKeys].some((key) => !storedKeys.has(key))
      ) {
        writeEnabledPluginKeys(migratedKeys);
      }
    } catch (cause) {
      setInstalledPlugins([]);
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      void refresh();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [refresh]);

  const selectedPlugins = useMemo(
    () => activePlugins(installedPlugins, enabledPluginKeys),
    [enabledPluginKeys, installedPlugins],
  );
  const activePluginKeys = useMemo(
    () => new Set(selectedPlugins.map(pluginKey)),
    [selectedPlugins],
  );

  const commitEnabled = useCallback((next: Set<string>) => {
    setEnabledPluginKeys(next);
    writeEnabledPluginKeys(next);
  }, []);

  const setPluginEnabled = useCallback(
    (plugin: InstalledMycPlugin, enabled: boolean) => {
      commitEnabled(
        updateEnabledPluginKeys(
          installedPlugins,
          enabledPluginKeys,
          plugin,
          enabled,
        ),
      );
    },
    [commitEnabled, enabledPluginKeys, installedPlugins],
  );

  const enableAll = useCallback(() => {
    commitEnabled(enableLatestPluginKeys(installedPlugins));
  }, [commitEnabled, installedPlugins]);

  const install = useCallback(
    async (path: string) => {
      const plugin = await installMycPlugin(path);
      // 刷新安装列表；安装后不再自动启用，需用户显式在插件商店开启。
      const plugins = await listInstalledMycPlugins();
      setInstalledPlugins(plugins);
      return plugin;
    },
    [],
  );

  const removeIncompatible = useCallback(async () => {
    const incompatible = installedPlugins.filter(
      (plugin) => !pluginCompatibility(plugin).compatible,
    );
    for (const plugin of incompatible) {
      await uninstallMycPlugin(pluginReference(plugin));
    }
    await refresh();
    return incompatible.length;
  }, [installedPlugins, refresh]);

  const value = useMemo<PluginHostValue>(
    () => ({
      installedPlugins,
      activePlugins: selectedPlugins,
      enabledPluginKeys,
      activePluginKeys,
      loading,
      error,
      refresh,
      install,
      setPluginEnabled,
      enableAll,
      removeIncompatible,
    }),
    [
      activePluginKeys,
      enableAll,
      enabledPluginKeys,
      error,
      install,
      installedPlugins,
      loading,
      refresh,
      removeIncompatible,
      selectedPlugins,
      setPluginEnabled,
    ],
  );

  return <PluginHostContext.Provider value={value}>{children}</PluginHostContext.Provider>;
}

export function usePluginHost(): PluginHostValue {
  const value = useContext(PluginHostContext);
  if (!value) throw new Error("usePluginHost must be used inside PluginHostProvider");
  return value;
}
