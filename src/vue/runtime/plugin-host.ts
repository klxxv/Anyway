import {
  inject,
  onBeforeUnmount,
  onMounted,
  provide,
  reactive,
  type InjectionKey,
} from "vue";
import type { InstalledMycPlugin } from "../../../app/plugins/contracts";
import type {
  PluginSecretMutation,
  PluginConnectionTestResult,
  PluginSettingsSnapshot,
} from "../../../app/plugins/tauri-client";
import { useRuntimePluginHostStore } from "../stores/runtime-plugin-host";
import type { HostPluginSettingDefinition } from "../components/panel-types";

/**
 * Vue host state for plugins. The fields are
 * exposed as live properties so Vue consumers can use the same snapshot shape
 * without reaching into implementation refs.
 */
export type PluginHostValue = {
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
  pluginSettings: Record<string, PluginSettingsSnapshot>;
  pluginSettingsLoading: ReadonlySet<string>;
  pluginSettingsErrors: Record<string, string>;
  loadPluginSettings: (
    plugin: { id: string; version: string; name: string },
    definitions: readonly HostPluginSettingDefinition[],
    native?: boolean,
  ) => Promise<PluginSettingsSnapshot>;
  savePluginSettings: (
    plugin: { id: string; version: string; name: string },
    definitions: readonly HostPluginSettingDefinition[],
    write: { values: Record<string, unknown>; secrets: Record<string, PluginSecretMutation> },
    native?: boolean,
  ) => Promise<PluginSettingsSnapshot>;
  resetPluginSettings: (
    plugin: { id: string; version: string; name: string },
    definitions: readonly HostPluginSettingDefinition[],
    native?: boolean,
  ) => Promise<PluginSettingsSnapshot>;
  testPluginConnection: (
    plugin: { id: string; version: string; name: string },
    connectionId: string,
    actionId: string,
    write: { values: Record<string, unknown>; secrets: Record<string, PluginSecretMutation> },
    native?: boolean,
  ) => Promise<PluginConnectionTestResult>;
  uninstall: (plugin: InstalledMycPlugin) => Promise<void>;
};

export const pluginHostKey: InjectionKey<PluginHostValue> = Symbol("research-canvas.plugin-host");

type RuntimePluginHostStore = ReturnType<typeof useRuntimePluginHostStore>;

/** Creates the legacy context-shaped value from the Pinia source of truth. */
export function createPluginHostValue(store: RuntimePluginHostStore): PluginHostValue {
  return reactive<PluginHostValue>({
    get installedPlugins() {
      return store.installedPlugins;
    },
    get activePlugins() {
      return store.activePlugins;
    },
    get enabledPluginKeys() {
      return store.enabledPluginKeys;
    },
    get activePluginKeys() {
      return store.activePluginKeys;
    },
    get loading() {
      return store.loading;
    },
    get error() {
      return store.error;
    },
    refresh: store.refresh,
    install: store.install,
    setPluginEnabled: store.setPluginEnabled,
    enableAll: store.enableAll,
    removeIncompatible: store.removeIncompatible,
    get pluginSettings() {
      return store.pluginSettings;
    },
    get pluginSettingsLoading() {
      return store.pluginSettingsLoading;
    },
    get pluginSettingsErrors() {
      return store.pluginSettingsErrors;
    },
    loadPluginSettings: store.loadPluginSettings,
    savePluginSettings: store.savePluginSettings,
    resetPluginSettings: store.resetPluginSettings,
    testPluginConnection: store.testPluginConnection,
    uninstall: store.uninstall,
  });
}

/**
 * Compatibility bridge for the existing provider API. Plugin state and
 * lifecycle actions are owned by the setup-style Pinia store.
 */
export function providePluginHost(): PluginHostValue {
  const store = useRuntimePluginHostStore();
  const value = createPluginHostValue(store);
  provide(pluginHostKey, value);
  onMounted(() => store.start());
  onBeforeUnmount(() => store.stop());
  return value;
}

export function usePluginHost(): PluginHostValue {
  const value = inject(pluginHostKey, null);
  if (!value) throw new Error("usePluginHost must be used inside PluginHostProvider");
  return value;
}
