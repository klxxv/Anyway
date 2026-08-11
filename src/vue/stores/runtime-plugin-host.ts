import { computed, shallowRef } from "vue";
import { defineStore } from "pinia";
import {
  pluginReference,
  type InstalledMycPlugin,
  type PluginReference,
} from "../../../app/plugins/contracts";
import {
  enabledPluginsStorageKey,
  pluginsChangedEvent,
  readEnabledPluginKeys,
} from "../../../app/plugins/context-menu";
import {
  activePlugins as resolveActivePlugins,
  enableLatestPluginKeys,
  migrateEnabledPluginKeys,
  pluginCompatibility,
  pluginKey,
  updateEnabledPluginKeys,
} from "../../../app/plugins/identity";
import {
  installMycPlugin,
  listInstalledMycPlugins,
  getPluginSettings,
  resetPluginSettings as resetPluginSettingsNative,
  savePluginSettings as savePluginSettingsNative,
  testPluginConnection as testPluginConnectionNative,
  uninstallMycPlugin,
  type PluginConnectionTestResult,
  type PluginSettingsSnapshot,
  type PluginSettingsWrite,
} from "../../../app/plugins/tauri-client";
import type { HostPluginSettingDefinition } from "../components/panel-types";

function writeEnabledPluginKeys(keys: ReadonlySet<string>): void {
  if (typeof window === "undefined") return;
  window.localStorage.setItem(enabledPluginsStorageKey, JSON.stringify([...keys].sort()));
  window.dispatchEvent(new CustomEvent(pluginsChangedEvent));
}

/**
 * Pinia setup store for plugin discovery, activation, version arbitration,
 * and installation. The public context-shaped bridge remains in runtime/plugin-host.ts.
 */
export const useRuntimePluginHostStore = defineStore("runtime-plugin-host", () => {
  const installedPlugins = shallowRef<InstalledMycPlugin[]>([]);
  const enabledPluginKeys = shallowRef<Set<string>>(new Set());
  const loading = shallowRef(true);
  const error = shallowRef("");
  const pluginSettings = shallowRef<Record<string, PluginSettingsSnapshot>>({});
  const pluginSettingsLoading = shallowRef<Set<string>>(new Set());
  const pluginSettingsErrors = shallowRef<Record<string, string>>({});

  const activePlugins = computed(() =>
    resolveActivePlugins(installedPlugins.value, enabledPluginKeys.value),
  );
  const activePluginKeys = computed(
    () => new Set(activePlugins.value.map((plugin) => pluginKey(plugin))),
  );

  const commitEnabled = (next: Set<string>): void => {
    enabledPluginKeys.value = next;
    writeEnabledPluginKeys(next);
  };

  const refresh = async (): Promise<void> => {
    loading.value = true;
    error.value = "";
    try {
      const plugins = await listInstalledMycPlugins();
      const storedKeys = new Set(readEnabledPluginKeys());
      const migratedKeys = migrateEnabledPluginKeys(plugins, storedKeys);
      installedPlugins.value = plugins;
      enabledPluginKeys.value = migratedKeys;
      if (
        migratedKeys.size !== storedKeys.size ||
        [...migratedKeys].some((key) => !storedKeys.has(key))
      ) {
        writeEnabledPluginKeys(migratedKeys);
      }
    } catch (cause) {
      installedPlugins.value = [];
      error.value = cause instanceof Error ? cause.message : String(cause);
    } finally {
      loading.value = false;
    }
  };

  const setPluginEnabled = (
    plugin: InstalledMycPlugin,
    enabled: boolean,
  ): void => {
    commitEnabled(
      updateEnabledPluginKeys(
        installedPlugins.value,
        enabledPluginKeys.value,
        plugin,
        enabled,
      ),
    );
  };

  const enableAll = (): void => {
    commitEnabled(enableLatestPluginKeys(installedPlugins.value));
  };

  const settingsKey = (plugin: PluginReference): string => `${plugin.id}@${plugin.version}`;

  const setSettingsLoading = (plugin: PluginReference, isLoading: boolean): void => {
    const next = new Set(pluginSettingsLoading.value);
    if (isLoading) next.add(settingsKey(plugin));
    else next.delete(settingsKey(plugin));
    pluginSettingsLoading.value = next;
  };

  const loadPluginSettings = async (
    plugin: PluginReference,
    definitions: readonly HostPluginSettingDefinition[],
    native = true,
  ): Promise<PluginSettingsSnapshot> => {
    const key = settingsKey(plugin);
    setSettingsLoading(plugin, true);
    pluginSettingsErrors.value = { ...pluginSettingsErrors.value, [key]: "" };
    try {
      const snapshot = await getPluginSettings(plugin, definitions, { native });
      pluginSettings.value = { ...pluginSettings.value, [key]: snapshot };
      return snapshot;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      pluginSettingsErrors.value = { ...pluginSettingsErrors.value, [key]: message };
      throw cause;
    } finally {
      setSettingsLoading(plugin, false);
    }
  };

  const savePluginSettings = async (
    plugin: PluginReference,
    definitions: readonly HostPluginSettingDefinition[],
    write: PluginSettingsWrite,
    native = true,
  ): Promise<PluginSettingsSnapshot> => {
    const key = settingsKey(plugin);
    setSettingsLoading(plugin, true);
    pluginSettingsErrors.value = { ...pluginSettingsErrors.value, [key]: "" };
    try {
      const snapshot = await savePluginSettingsNative(plugin, definitions, write, { native });
      pluginSettings.value = { ...pluginSettings.value, [key]: snapshot };
      return snapshot;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      pluginSettingsErrors.value = { ...pluginSettingsErrors.value, [key]: message };
      throw cause;
    } finally {
      setSettingsLoading(plugin, false);
    }
  };

  const resetPluginSettings = async (
    plugin: PluginReference,
    definitions: readonly HostPluginSettingDefinition[],
    native = true,
  ): Promise<PluginSettingsSnapshot> => {
    const key = settingsKey(plugin);
    setSettingsLoading(plugin, true);
    pluginSettingsErrors.value = { ...pluginSettingsErrors.value, [key]: "" };
    try {
      const snapshot = await resetPluginSettingsNative(plugin, definitions, { native });
      pluginSettings.value = { ...pluginSettings.value, [key]: snapshot };
      return snapshot;
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      pluginSettingsErrors.value = { ...pluginSettingsErrors.value, [key]: message };
      throw cause;
    } finally {
      setSettingsLoading(plugin, false);
    }
  };

  const testPluginConnection = async (
    plugin: PluginReference,
    connectionId: string,
    write: PluginSettingsWrite,
    native = true,
  ): Promise<PluginConnectionTestResult> => {
    if (!native) throw new Error("MYC_DESKTOP_REQUIRED");
    try {
      return await testPluginConnectionNative(plugin, connectionId, write, { native });
    } catch (cause) {
      const key = settingsKey(plugin);
      const message = cause instanceof Error ? cause.message : String(cause);
      pluginSettingsErrors.value = { ...pluginSettingsErrors.value, [key]: message };
      throw cause;
    }
  };

  const install = async (path: string): Promise<InstalledMycPlugin> => {
    const plugin = await installMycPlugin(path);
    installedPlugins.value = await listInstalledMycPlugins();
    return plugin;
  };

  const removeIncompatible = async (): Promise<number> => {
    const incompatible = installedPlugins.value.filter(
      (plugin) => !pluginCompatibility(plugin).compatible,
    );
    for (const plugin of incompatible) {
      await uninstallMycPlugin(pluginReference(plugin));
    }
    await refresh();
    return incompatible.length;
  };

  const uninstall = async (plugin: InstalledMycPlugin): Promise<void> => {
    await uninstallMycPlugin(pluginReference(plugin));
    const key = pluginKey(plugin);
    const nextSettings = { ...pluginSettings.value };
    delete nextSettings[key];
    pluginSettings.value = nextSettings;
    await refresh();
  };

  let refreshFrame: number | null = null;

  const start = (): void => {
    if (typeof window === "undefined" || refreshFrame !== null) return;
    refreshFrame = window.requestAnimationFrame(() => {
      refreshFrame = null;
      void refresh();
    });
  };

  const stop = (): void => {
    if (typeof window === "undefined" || refreshFrame === null) return;
    window.cancelAnimationFrame(refreshFrame);
    refreshFrame = null;
  };

  return {
    installedPlugins,
    activePlugins,
    enabledPluginKeys,
    activePluginKeys,
    loading,
    error,
    pluginSettings,
    pluginSettingsLoading,
    pluginSettingsErrors,
    refresh,
    install,
    setPluginEnabled,
    enableAll,
    removeIncompatible,
    loadPluginSettings,
    savePluginSettings,
    resetPluginSettings,
    testPluginConnection,
    uninstall,
    start,
    stop,
  };
});
