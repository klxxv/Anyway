import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { normalizePluginSettingsSnapshot } from "../app/plugins/tauri-client";
import {
  connectionTestActions,
  defaultPluginSettingsDraft,
  normalizePluginSettingDefinitions,
  resolvePluginPrivateText,
  settingsWriteFromDraft,
  validatePluginSettingsDraft,
} from "../src/vue/components/panel-types";
import { normalizePluginSettings } from "../app/plugins/contracts";

test("shared settings contract canonicalizes legacy secret declarations", () => {
  const settings = normalizePluginSettings([
    { id: "api-key", label: "API key", type: "secret" },
  ]);
  assert.deepEqual(settings, [{
    id: "api-key",
    label: "API key",
    type: "text",
    secret: true,
    writeOnly: true,
  }]);
});

test("host normalizes declarative plugin settings including forward-compatible secrets", () => {
  const definitions = normalizePluginSettingDefinitions([
    { id: "depth", label: "Depth", type: "number", default: 3, min: 1, max: 8 },
    { id: "mode", label: "Mode", type: "select", options: [{ value: "fast", label: "Fast" }] },
    { id: "api-key", label: "API key", type: "text", secret: true },
    { id: "invalid", label: "Invalid", type: "unknown" },
  ]);

  assert.deepEqual(definitions.map((definition) => definition.id), ["depth", "mode", "api-key"]);
  const defaults = defaultPluginSettingsDraft(definitions);
  assert.equal(defaults.depth, 3);
  assert.equal(defaults.mode, "fast");
  assert.deepEqual(defaults["api-key"], { action: "clear", value: "" });
});

test("user-facing credential defaults prefer entering an API key in the app", () => {
  const definitions = normalizePluginSettingDefinitions([
    {
      id: "credential-source",
      label: "Credential source",
      type: "select",
      default: "environment",
      options: [
        { value: "environment", label: "Environment" },
        { value: "host-secret", label: "Host secret" },
      ],
    },
  ]);
  assert.equal(defaultPluginSettingsDraft(definitions)["credential-source"], "host-secret");
});

test("plugin-private i18n resolves before manifest fallback without using the host locale catalog", () => {
  const target = {
    i18n: {
      defaultLocale: "en",
      resources: {
        "zh-CN": { "settings.apiKey.label": "API Key（插件）" },
      },
    },
  };
  assert.equal(
    resolvePluginPrivateText(target, "zh-CN", "settings.apiKey.label", "API key"),
    "API Key（插件）",
  );
  assert.equal(
    resolvePluginPrivateText(target, "zh-CN", "settings.model.label", "Model"),
    "Model",
  );
});

test("connection test actions are read from manifest declarations without provider presets", () => {
  const legacy = {
    id: "custom-service",
    label: "Custom service",
    testAction: { id: "manifest-smoke", label: "Run manifest smoke test" },
  } as never;
  assert.deepEqual(connectionTestActions(legacy), [legacy.testAction]);
  const modern = { ...legacy, testAction: undefined, testActions: [
    { id: "manifest-connectivity", label: "Check connectivity" },
    { id: "manifest-runtime", label: "Check runtime" },
  ] } as never;
  assert.deepEqual(connectionTestActions(modern).map((action) => action.id), [
    "manifest-connectivity",
    "manifest-runtime",
  ]);
});

test("host validates bounds and serializes secrets without mixing them into ordinary values", () => {
  const definitions = normalizePluginSettingDefinitions([
    { id: "depth", label: "Depth", type: "number", min: 1, max: 8 },
    { id: "api-key", label: "API key", type: "text", secret: true, required: true },
  ]);
  const invalid = validatePluginSettingsDraft(definitions, {
    depth: 12,
    "api-key": { action: "set", value: "" },
  });
  assert.ok(invalid.depth);
  assert.ok(invalid["api-key"]);

  const requiredButMissing = validatePluginSettingsDraft(definitions, {
    depth: 4,
    "api-key": { action: "keep", value: "" },
  });
  assert.ok(requiredButMissing["api-key"]);

  const write = settingsWriteFromDraft(definitions, {
    depth: 4,
    "api-key": { action: "set", value: "secret-value" },
  });
  assert.deepEqual(write.values, { depth: 4 });
  assert.deepEqual(write.secrets, { "api-key": { action: "set", value: "secret-value" } });
});

test("host reads the native effectiveValues/secretConfigured snapshot without exposing secrets", () => {
  const definitions = normalizePluginSettingDefinitions([
    { id: "model", label: "Model", type: "text", default: "luna" },
    { id: "api-key", label: "API key", type: "text", secret: true },
  ]);
  const snapshot = normalizePluginSettingsSnapshot(
    {
      pluginId: "pdf-agent",
      pluginVersion: "0.2.0",
      definitions,
      effectiveValues: { model: "luna", "api-key": null },
      overrides: { model: "luna" },
      secretConfigured: { "api-key": true },
    },
    { id: "pdf-agent", version: "0.2.0", name: "PDF Agent" },
    definitions,
  );
  assert.deepEqual(snapshot.values, { model: "luna" });
  assert.equal(snapshot.configuredSecrets["api-key"], true);
  assert.equal(Object.prototype.hasOwnProperty.call(snapshot.values, "api-key"), false);
});

test("anPdfsolver manifest keeps plugin-owned settings, connection actions, and private locales parseable", () => {
  const manifest = JSON.parse(
    readFileSync("my-plugins/anPdfsolver/plugin.json", "utf8"),
  ) as {
    contributes?: {
      configuration?: {
        settings?: Array<{
          id: string;
          type: string;
          secret?: boolean;
          required?: boolean;
          placeholder?: string;
          default?: string;
          options?: Array<{ value: string }>;
        }>;
        connections?: Array<{
          id: string;
          urlSettingId?: string;
          formatSettingId?: string;
          modelSettingId?: string;
          testActions?: Array<{ id: string }>;
          apiKey: { source: string; settingId?: string; secretEnv?: string; name?: string; fallbackSettingId?: string };
        }>;
      };
    };
    i18n?: {
      defaultLocale?: string;
      resources?: Record<string, Record<string, string>>;
    };
  };
  const privateZh = JSON.parse(
    readFileSync("my-plugins/anPdfsolver/locales/zh-CN.json", "utf8"),
  ) as Record<string, string>;
  const settings = manifest.contributes?.configuration?.settings ?? [];
  const byId = (id: string) => {
    const setting = settings.find((entry) => entry.id === id);
    assert.ok(setting, `manifest must declare setting ${id}`);
    return setting;
  };
  assert.equal(byId("api-key").type, "text");
  assert.equal(byId("api-key").secret, true);
  assert.equal(normalizePluginSettingDefinitions(settings).find((setting) => setting.id === "api-key")?.type, "secret");
  assert.equal(byId("api-url").required, true);
  assert.equal(byId("api-url").placeholder, "https://api.moonshot.cn/v1");
  assert.equal(byId("api-format").default, "openai");
  assert.deepEqual(
    (byId("pdf-transport").options ?? []).map((option) => option.value),
    ["local-text", "kimi-file-extract"],
  );
  assert.equal(byId("model").default, "kimi-k2.6");
  assert.equal(byId("thinking").default, "enabled");
  assert.deepEqual(
    (byId("thinking").options ?? []).map((option) => option.value),
    ["enabled", "disabled"],
  );

  const provider = (manifest.contributes?.configuration?.connections ?? []).find(
    (connection) => connection.id === "kimi",
  );
  assert.ok(provider, "manifest must declare the plugin-owned Kimi connection");
  assert.equal(provider.urlSettingId, "api-url");
  assert.equal(provider.formatSettingId, "api-format");
  assert.equal(provider.modelSettingId, "model");
  assert.deepEqual((provider.testActions ?? []).map((action) => action.id), []);
  assert.deepEqual(provider.apiKey, {
    source: "host-secret",
    settingId: "api-key",
    secretEnv: "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY",
  });
  assert.equal(privateZh["results.connection-succeeded"], "AI 连接成功。");
  assert.equal(privateZh["settings.credentialSource.options.hostSecret"], "在此应用中填写 API 密钥");
  assert.match(privateZh["settings.pdfTransport.options.kimiFiles"], /Kimi|文件解析|上传/);
});

test("PluginSettingsDialog is generic and does not contain PDF or provider-specific branches", () => {
  const manifest = JSON.parse(
    readFileSync("my-plugins/anPdfsolver/plugin.json", "utf8"),
  ) as {
    contributes?: {
      configuration?: {
        settings?: Array<{ id: string; default?: string; group?: string }>;
      };
    };
  };
  const dialog = readFileSync("src/vue/components/PluginSettingsDialog.vue", "utf8");

  const publicProgress = (manifest.contributes?.configuration?.settings ?? []).find(
    (setting) => setting.id === "public-progress",
  );
  assert.ok(publicProgress, "manifest must declare the public-progress setting");
  assert.equal(publicProgress.default, "disabled");
  assert.equal(publicProgress.group, "advanced");

  assert.match(dialog, /definition\.group === "advanced"/);
  assert.match(dialog, /definition\.type === 'secret'/);
  assert.match(dialog, /definition\.type === 'select'/);
  assert.match(dialog, /connectionTestActions\(connection\)/);
  assert.doesNotMatch(dialog, /Pdf|PDF|Kimi|Moonshot|provider presets|PLUGIN_TEST_ACTION_IDS/);
});

test("plugin store promotes the latest compatible version and keeps older packages removable", () => {
  const dialog = readFileSync("src/vue/components/PluginStoreDialog.vue", "utf8");
  assert.match(dialog, /supersededCompatiblePlugins/);
  assert.match(dialog, /visibleSuperseded/);
  assert.match(dialog, /plugins\.versionMismatchTitle/);
  assert.match(dialog, /plugins\.uninstallVersion/);
});

test("installed plugins expose an explicit native settings action", () => {
  const dialog = readFileSync("src/vue/components/PluginStoreDialog.vue", "utf8");
  const catalog = readFileSync("app/plugins/catalog.ts", "utf8");
  assert.match(dialog, /targetFromInstalled\(plugin\).*openSettings\(targetFromInstalled\(plugin\)!\)/s);
  assert.doesNotMatch(catalog, /id:\s*"pdf-canvas-agent"/);
});

test("host settings render and invoke manifest-declared connection test actions generically", () => {
  const dialog = readFileSync("src/vue/components/PluginSettingsDialog.vue", "utf8");
  const store = readFileSync("src/vue/components/PluginStoreDialog.vue", "utf8");
  const client = readFileSync("app/plugins/tauri-client.ts", "utf8");
  assert.match(dialog, /connectionTestActions/);
  assert.match(dialog, /onTestConnection/);
  assert.match(dialog, /connectionTesting/);
  assert.match(dialog, /v-for="action in testActions"/);
  assert.match(dialog, /callback\.length >= 3/);
  assert.match(dialog, /connection\.id,\s*action\.id,\s*snapshot/);
  assert.match(dialog, /pluginText\(result\.code \? `results\.\$\{result\.code\}`/);
  assert.match(dialog, /labelFor\(definition\)/);
  assert.match(dialog, /optionLabelFor\(definition, option\)/);
  assert.doesNotMatch(dialog, /secretKeep|secretReplace|secretClear/);
  assert.doesNotMatch(dialog, /test-pdf-extraction|No PDF is uploaded|plugins\.testAiConnectionHint|Kimi|Moonshot/);
  assert.match(store, /pluginHost\.testPluginConnection/);
  assert.match(client, /plugin\.connection\.test/);
  assert.doesNotMatch(client, /console\.(log|debug|info).*secret/i);
});

test("plugin store modal blocks native trackpad frames before the canvas changes viewport", () => {
  const workspace = readFileSync("src/vue/ResearchWorkspaceApp.vue", "utf8");
  const canvas = readFileSync("src/vue/canvas/ResearchGraphCanvas.vue", "utf8");
  const dialog = readFileSync("src/vue/components/PluginStoreDialog.vue", "utf8");
  assert.match(workspace, /const canvasInputBlocked = computed\(\(\) => settingsOpen\.value \|\| pluginStoreOpen\.value\)/);
  assert.match(workspace, /:canvas-input-blocked="canvasInputBlocked"/);
  assert.match(canvas, /function handleTrackpadFrame\(frame: CanvasTrackpadFrame\) \{\s*if \(props\.canvasInputBlocked\) return;/);
  assert.match(dialog, /@wheel\.stop/);
});
