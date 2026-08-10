import { computed, shallowRef } from "vue";
import { defineStore } from "pinia";
import {
  pluginReference,
  type InstalledMycPlugin,
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
  uninstallMycPlugin,
} from "../../../app/plugins/tauri-client";

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
    refresh,
    install,
    setPluginEnabled,
    enableAll,
    removeIncompatible,
    start,
    stop,
  };
});
