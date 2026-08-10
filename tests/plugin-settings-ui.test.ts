import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { normalizePluginSettingsSnapshot } from "../app/plugins/tauri-client";
import {
  defaultPluginSettingsDraft,
  normalizePluginSettingDefinitions,
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

test("PDF Agent declares host-managed credential, model, and thinking settings", () => {
  const manifest = readFileSync("plugins/sources/myc.pdf-canvas-agent/plugin.yml", "utf8");
  assert.match(manifest, /id:\s+api-key[\s\S]*?type:\s+text[\s\S]*?secret:\s+true/);
  assert.match(manifest, /id:\s+model[\s\S]*?default:\s+luna/);
  assert.match(manifest, /id:\s+thinking[\s\S]*?default:\s+extra_high/);

  const descriptor = JSON.parse(
    readFileSync("plugins/sources/myc.pdf-canvas-agent/agent-manifest.json", "utf8"),
  ) as { modelConfiguration?: { ownership?: string; agentReceivesPlaintextSecrets?: boolean } };
  assert.equal(descriptor.modelConfiguration?.ownership, "host-managed");
  assert.equal(descriptor.modelConfiguration?.agentReceivesPlaintextSecrets, false);
});

test("plugin store promotes the latest compatible version and keeps older packages removable", () => {
  const dialog = readFileSync("src/vue/components/PluginStoreDialog.vue", "utf8");
  assert.match(dialog, /supersededCompatiblePlugins/);
  assert.match(dialog, /visibleSuperseded/);
  assert.match(dialog, /plugins\.versionMismatchTitle/);
  assert.match(dialog, /plugins\.uninstallVersion/);
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
