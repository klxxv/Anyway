import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { normalizePluginSettingsSnapshot } from "../app/plugins/tauri-client";
import {
  connectionTestActions,
  defaultPluginSettingsDraft,
  normalizePluginSettingDefinitions,
  PLUGIN_TEST_ACTION_IDS,
  resolvePluginPrivateText,
  settingsWriteFromDraft,
  validatePluginSettingsDraft,
} from "../src/vue/components/panel-types";
import { normalizePluginSettings } from "../app/plugins/contracts";
import {
  isPublicProgressEnabled,
  PUBLIC_PROGRESS_SETTING_ID,
} from "../app/plugins/agent-contracts";

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

test("connection test actions accept the new array and legacy singular declaration", () => {
  const legacy = {
    id: "provider",
    label: "Provider",
    urlSettingId: "api-url",
    formatSettingId: "api-format",
    apiKey: { source: "environment", name: "API_KEY" },
    testAction: { id: PLUGIN_TEST_ACTION_IDS.connection, label: "Test connection" },
  } as never;
  assert.deepEqual(connectionTestActions(legacy), [legacy.testAction]);
  const modern = { ...legacy, testAction: undefined, testActions: [
    { id: PLUGIN_TEST_ACTION_IDS.connection, label: "Test AI connection" },
    { id: PLUGIN_TEST_ACTION_IDS.pdfExtraction, label: "Test PDF parsing" },
  ] } as never;
  assert.deepEqual(connectionTestActions(modern).map((action) => action.id), [
    PLUGIN_TEST_ACTION_IDS.connection,
    PLUGIN_TEST_ACTION_IDS.pdfExtraction,
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

test("PDF Agent declares host-managed connection, credential, model, and thinking settings", () => {
  const manifest = readFileSync("plugins/sources/myc.pdf-canvas-agent/plugin.yml", "utf8");
  assert.match(manifest, /id:\s+api-key[\s\S]*?type:\s+text[\s\S]*?secret:\s+true/);
  assert.match(manifest, /id:\s+api-url[\s\S]*?required:\s+true[\s\S]*?placeholder:\s+https:\/\/api\.moonshot\.cn\/v1/);
  assert.match(manifest, /id:\s+api-format[\s\S]*?default:\s+openai/);
  assert.match(manifest, /id:\s+credential-source[\s\S]*?default:\s+host-secret/);
  assert.match(manifest, /id:\s+credential-env-var[\s\S]*?default:\s+MOONSHOT_API_KEY/);
  assert.match(manifest, /id:\s+pdf-transport[\s\S]*?default:\s+local-text[\s\S]*?value:\s+local-text[\s\S]*?value:\s+kimi-file-extract/);
  assert.match(manifest, /id:\s+model[\s\S]*?default:\s+kimi-k2\.6/);
  assert.match(manifest, /id:\s+thinking[\s\S]*?default:\s+enabled[\s\S]*?value:\s+disabled/);
  assert.match(manifest, /connections:[\s\S]*?credentialSourceSettingId:\s+credential-source[\s\S]*?testActions:[\s\S]*?id:\s+test-connection[\s\S]*?id:\s+test-pdf-extraction/);
  assert.match(manifest, /source:\s+environment[\s\S]*?name:\s+MOONSHOT_API_KEY[\s\S]*?fallbackSettingId:\s+api-key/);

  const descriptor = JSON.parse(
    readFileSync("plugins/sources/myc.pdf-canvas-agent/agent-manifest.json", "utf8"),
  ) as { modelConfiguration?: { ownership?: string; agentReceivesPlaintextSecrets?: boolean } };
  assert.equal(descriptor.modelConfiguration?.ownership, "host-managed");
  assert.equal(descriptor.modelConfiguration?.agentReceivesPlaintextSecrets, false);
});

test("public progress is an advanced host setting and is recognized by runtime contracts", () => {
  const manifest = readFileSync("plugins/sources/myc.pdf-canvas-agent/plugin.yml", "utf8");
  const dialog = readFileSync("src/vue/components/PluginSettingsDialog.vue", "utf8");
  const runtime = readFileSync("src-tauri/src/agent_commands.rs", "utf8");
  const pipeline = readFileSync("plugins/sources/myc.pdf-canvas-agent/agent.yml", "utf8");

  assert.equal(PUBLIC_PROGRESS_SETTING_ID, "public-progress");
  assert.equal(isPublicProgressEnabled("enabled"), true);
  assert.equal(isPublicProgressEnabled("disabled"), false);
  assert.match(manifest, /id:\s+public-progress[\s\S]*?default:\s+disabled[\s\S]*?group:\s+advanced/);
  assert.match(dialog, /definition\.group === "advanced"/);
  assert.match(runtime, /\["public-progress", "publicProgress", "public_progress"\]/);
  assert.match(pipeline, /publicProgress:[\s\S]*?settingId:\s+public-progress[\s\S]*?source:\s+assistant-content/);
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

test("host settings render and invoke declarative connection test actions", () => {
  const dialog = readFileSync("src/vue/components/PluginSettingsDialog.vue", "utf8");
  const store = readFileSync("src/vue/components/PluginStoreDialog.vue", "utf8");
  const client = readFileSync("app/plugins/tauri-client.ts", "utf8");
  const privateZh = JSON.parse(
    readFileSync("plugins/sources/myc.pdf-canvas-agent/locales/zh-CN.json", "utf8"),
  ) as Record<string, string>;
  assert.match(dialog, /connectionTestActions/);
  assert.match(dialog, /onTestConnection/);
  assert.match(dialog, /connectionTesting/);
  assert.match(dialog, /PLUGIN_TEST_ACTION_IDS\.pdfExtraction/);
  assert.match(dialog, /Test AI connection|plugins\.testAiConnection/);
  assert.match(dialog, /No PDF is uploaded|plugins\.testAiConnectionHint/);
  assert.match(dialog, /pluginText\(result\.code \? `results\.\$\{result\.code\}`/);
  assert.match(dialog, /labelFor\(definition\)/);
  assert.equal(privateZh["results.connection-succeeded"], "AI 连接成功。");
  assert.equal(privateZh["settings.credentialSource.options.hostSecret"], "在此应用中填写 API 密钥");
  assert.doesNotMatch(dialog, /secretKeep|secretReplace|secretClear/);
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
