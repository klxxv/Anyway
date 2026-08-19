import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  normalizePluginSettings,
  pluginPrivateMessageNamespace,
  resolvePluginPrivateMessage,
} from "../app/plugins/contracts";

const read = (path: string) => readFileSync(path, "utf8");

test("Rust and C++ SDKs expose the same MYC guest ABI", () => {
  const rust = read("plugins/sdk/rust/src/lib.rs");
  const cpp = read("plugins/sdk/cpp/research_canvas_plugin.h");

  for (const symbol of ["myc_alloc", "myc_free", "myc_run"]) {
    assert.match(rust, new RegExp(`extern \\"C\\" fn ${symbol}`));
    assert.match(cpp, new RegExp(`${symbol}\\(`));
  }
  assert.match(cpp, /output_pointer << 32/);
  assert.match(rust, /pointer[\s\S]*<< 32/);
});

test("runtime documentation records enforceable sandbox limits", () => {
  const documentation = read("plugins/README.md");
  assert.match(documentation, /no host imports/i);
  assert.match(documentation, /16 MB memory ceiling/i);
  assert.match(documentation, /5,000,000 fuel units/i);
  assert.match(documentation, /1 MB\s+JSON input\/output limits/i);
});

test("community model adapters share a review-gated GraphPatch schema", () => {
  const schema = JSON.parse(read("plugins/sdk/graph-patch.schema.json")) as {
    properties: {
      apiVersion: { const: string };
      reviewRequired: { const: boolean };
      operations: { maxItems: number };
    };
  };
  const python = read("plugins/sdk/python/research_canvas.py");
  assert.equal(
    schema.properties.apiVersion.const,
    "researchcanvas.dev/graph-patch/v1alpha1",
  );
  assert.equal(schema.properties.reviewRequired.const, true);
  assert.equal(schema.properties.operations.maxItems, 2000);
  assert.match(python, /class NetworkBlockExtractor\(Protocol\)/);
  assert.match(python, /"reviewRequired": True/);
});

test("SDK setting declarations keep credentials host-owned and write-only", () => {
  const rust = read("plugins/sdk/rust/src/lib.rs");
  const cpp = read("plugins/sdk/cpp/research_canvas_plugin.h");
  const python = read("plugins/sdk/python/research_canvas.py");

  assert.match(rust, /pub struct PluginSettingDefinition/);
  assert.match(rust, /pub enum PluginApiKeySource/);
  assert.match(rust, /pub struct PluginConnectionDefinition/);
  assert.match(rust, /pub trait SettingReader/);
  assert.match(cpp, /struct PluginSettingDefinition/);
  assert.match(cpp, /struct PluginConnectionDefinition/);
  assert.match(cpp, /PluginApiKeySource/);
  assert.match(cpp, /class SettingsReader/);
  assert.match(python, /class PluginSetting/);
  assert.match(python, /class PluginConnection/);
  assert.match(python, /credential_env_var/);
  assert.match(python, /class SettingReader\(Protocol\)/);
  assert.match(python, /"writeOnly": self\.write_only or self\.is_secret/);
  assert.doesNotMatch(rust, /get_secret/i);
  assert.doesNotMatch(cpp, /getSecret\s*\(/);
  assert.doesNotMatch(python, /get_secret/);
});

test("plugin-private i18n is namespaced and independent from LocalePlugin", () => {
  const manifest = JSON.parse(read("plugins/sources/myc.pdf-canvas-agent/plugin.json")) as {
    capabilities: string[];
    privateI18n: { defaultLocale: string; locales: Record<string, string> };
  };
  const english = JSON.parse(read("plugins/sources/myc.pdf-canvas-agent/locales/en.json")) as Record<string, string>;
  const chinese = JSON.parse(read("plugins/sources/myc.pdf-canvas-agent/locales/zh-CN.json")) as Record<string, string>;
  assert.equal(manifest.privateI18n.defaultLocale, "en");
  assert.deepEqual(manifest.privateI18n.locales, {
    en: "locales/en.json",
    "zh-CN": "locales/zh-CN.json",
  });
  assert.ok(
    !manifest.capabilities.includes("i18n.register"),
    "plugin-private i18n must not declare the LocalePlugin capability",
  );
  assert.equal(pluginPrivateMessageNamespace("myc.pdf-canvas-agent"), "plugin:myc.pdf-canvas-agent");
  const installed = {
    namespace: "myc.pdf-canvas-agent",
    defaultLocale: "en",
    locales: { en: english, "zh-CN": chinese },
  };
  assert.equal(resolvePluginPrivateMessage(installed, "zh-CN", "settings.apiKey.label"), "API 密钥");
  assert.equal(resolvePluginPrivateMessage(installed, "fr", "settings.apiKey.label"), "API key");
  assert.equal(resolvePluginPrivateMessage(undefined, "zh-CN", "plugins.refresh"), undefined);
});

test("settings and options retain raw fallbacks with private message keys", () => {
  const normalized = normalizePluginSettings([
    {
      id: "model",
      label: "Model",
      labelKey: "settings.model.label",
      description: "Model used by the agent",
      descriptionKey: "settings.model.description",
      placeholder: "model-name",
      placeholderKey: "settings.model.placeholder",
      type: "select",
      options: [{
        value: "fast",
        label: "Fast",
        labelKey: "settings.thinking.options.low",
        descriptionKey: "settings.thinking.options.low.description",
      }],
    },
  ]);
  assert.equal(normalized[0].label, "Model");
  assert.equal(normalized[0].labelKey, "settings.model.label");
  assert.equal(normalized[0].descriptionKey, "settings.model.description");
  assert.equal(normalized[0].options?.[0].label, "Fast");
  assert.equal(normalized[0].options?.[0].labelKey, "settings.thinking.options.low");

  const manifest = JSON.parse(read("plugins/sources/myc.pdf-canvas-agent/plugin.json")) as {
    contributes?: {
      configuration?: {
        connections?: Array<{
          id: string;
          testActions?: Array<{
            id: string;
            kind: string;
            input: { type: string; fileUpload?: string; fixture?: string };
          }>;
          apiKey?: { source: string; name: string; fallbackSettingId: string };
        }>;
      };
    };
  };
  const provider = manifest.contributes?.configuration?.connections?.find(
    (connection) => connection.id === "provider",
  );
  assert.ok(provider, "manifest must declare the provider connection");
  const connectionTest = provider.testActions?.find(
    (action) => action.id === "test-connection",
  );
  assert.ok(connectionTest, "manifest must declare the test-connection action");
  assert.equal(connectionTest.input.type, "text");
  assert.equal(connectionTest.input.fileUpload, "never");
  const pdfTest = provider.testActions?.find(
    (action) => action.id === "test-pdf-extraction",
  );
  assert.ok(pdfTest, "manifest must declare the test-pdf-extraction action");
  assert.equal(pdfTest.input.type, "bundled-pdf");
  assert.equal(pdfTest.input.fixture, "host-minimal-pdf-v1");
  assert.equal(pdfTest.input.fileUpload, "may-upload");
  assert.ok(
    !("testAction" in provider),
    "the legacy singular action must not duplicate the two canonical tests",
  );
  assert.deepEqual(provider.apiKey, {
    source: "environment",
    name: "MOONSHOT_API_KEY",
    fallbackSettingId: "api-key",
  });
});

test("SDKs expose private i18n and the canonical testActions list", () => {
  const rust = read("plugins/sdk/rust/src/lib.rs");
  const cpp = read("plugins/sdk/cpp/research_canvas_plugin.h");
  const python = read("plugins/sdk/python/research_canvas.py");
  assert.match(rust, /struct PluginPrivateI18n/);
  assert.match(rust, /test_actions/);
  assert.match(cpp, /struct PluginPrivateI18n/);
  assert.match(cpp, /testActions/);
  assert.match(python, /class PluginPrivateI18n/);
  assert.match(python, /"testActions"/);
});
