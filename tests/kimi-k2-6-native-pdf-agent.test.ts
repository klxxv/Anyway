import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const manifestPath = "plugins/sources/myc.pdf-canvas-agent/plugin.json";
const descriptorPath = "plugins/sources/myc.pdf-canvas-agent/agent-manifest.json";
const englishLocalePath = "plugins/sources/myc.pdf-canvas-agent/locales/en.json";
const chineseLocalePath = "plugins/sources/myc.pdf-canvas-agent/locales/zh-CN.json";

const read = (path: string) => readFileSync(path, "utf8");

type Setting = Record<string, unknown> & { id: string };
type Connection = Record<string, unknown> & { id: string };
type TestAction = Record<string, unknown> & { id: string };
type ManifestV2 = {
  contributes?: {
    configuration?: { settings?: Setting[]; connections?: Connection[] };
  };
};

const parsedManifest = JSON.parse(read(manifestPath)) as ManifestV2;
const declaredSettings = parsedManifest.contributes?.configuration?.settings ?? [];
const declaredConnections = parsedManifest.contributes?.configuration?.connections ?? [];

function settingBlock(id: string): Setting {
  const setting = declaredSettings.find((entry) => entry.id === id);
  assert.ok(setting, `manifest must declare setting ${id}`);
  return setting;
}

function connectionBlock(): Connection {
  assert.equal(declaredConnections.length, 1, "manifest must declare exactly one provider connection");
  return declaredConnections[0];
}

function actionBlock(connection: Connection, id: string): TestAction {
  const actions = (connection.testActions ?? []) as TestAction[];
  const action = actions.find((entry) => entry.id === id);
  assert.ok(action, `connection must declare action ${id}`);
  return action;
}

function collectFiles(root: string): string[] {
  if (!existsSync(root)) return [];
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name);
    return entry.isDirectory() ? collectFiles(path) : [path];
  });
}

test("Kimi K2.6 is the native PDF Agent model and requires a user-provided URL", () => {
  const apiUrl = settingBlock("api-url");
  assert.equal(apiUrl.required, true);
  assert.ok(!("default" in apiUrl), "the API URL must stay user-provided");
  assert.equal(apiUrl.placeholder, "https://api.moonshot.cn/v1");
  assert.equal(settingBlock("api-format").default, "openai");
  assert.equal(settingBlock("model").default, "kimi-k2.6");
  assert.equal(settingBlock("credential-env-var").default, "MOONSHOT_API_KEY");

  const connection = connectionBlock();
  assert.deepEqual(connection.apiKey, {
    source: "environment",
    name: "MOONSHOT_API_KEY",
    fallbackSettingId: "api-key",
  });
});

test("the plugin declares K2.6 request constraints and thinking translation", () => {
  const pluginPipeline = read("plugins/sources/myc.pdf-canvas-agent/agent.yml");
  const descriptor = JSON.parse(read(descriptorPath)) as {
    modelConfiguration?: { nativeProviderProfile?: { id?: string; declaration?: string } };
  };

  assert.equal(descriptor.modelConfiguration?.nativeProviderProfile?.id, "kimi-k2.6");
  assert.equal(
    descriptor.modelConfiguration?.nativeProviderProfile?.declaration,
    "agent.yml#nativeProvider",
  );
  assert.match(pluginPipeline, /^nativeProvider:\r?$/m);
  assert.match(pluginPipeline, /^  id: kimi-k2\.6$/m);
  assert.match(pluginPipeline, /^    formatSettingId: api-format$/m);
  assert.match(pluginPipeline, /^      openai:$/m);
  assert.match(pluginPipeline, /^        format: openai-compatible$/m);
  assert.match(pluginPipeline, /^        streamProtocol: openai-sse$/m);
  assert.match(pluginPipeline, /^      anthropic:$/m);
  assert.match(pluginPipeline, /^        format: anthropic-compatible$/m);
  assert.match(pluginPipeline, /^        streamProtocol: anthropic-sse$/m);
  assert.match(pluginPipeline, /^    baseUrlSource: plugin-setting$/m);
  assert.match(pluginPipeline, /^    baseUrlSettingId: api-url$/m);
  assert.match(pluginPipeline, /^          international: https:\/\/api\.moonshot\.ai\/v1$/m);
  assert.match(pluginPipeline, /^          international: https:\/\/api\.moonshot\.ai\/anthropic$/m);
  assert.match(pluginPipeline, /^    model: kimi-k2\.6$/m);
  assert.match(pluginPipeline, /^    apiFormat: openai$/m);
  assert.match(pluginPipeline, /^    thinking: enabled$/m);
  assert.match(pluginPipeline, /^      settingId: thinking$/m);
  assert.match(pluginPipeline, /^      requestPath: thinking\.type$/m);
  assert.match(pluginPipeline, /^        enabled: enabled$/m);
  assert.match(pluginPipeline, /^        disabled: disabled$/m);
  assert.match(pluginPipeline, /^      mode: omit$/m);
  assert.match(pluginPipeline, /^        - temperature$/m);
  assert.match(pluginPipeline, /^      stream: true$/m);
});

test("the native settings contract exposes exactly two explicit connection tests", () => {
  const connection = connectionBlock();
  const actionIds = ((connection.testActions ?? []) as TestAction[]).map(
    (action) => action.id,
  );

  assert.deepEqual(actionIds, ["test-connection", "test-pdf-extraction"]);
  assert.ok(
    !("testAction" in connection),
    "the legacy singular action must not duplicate the two canonical tests",
  );

  const connectionTest = actionBlock(connection, "test-connection");
  assert.equal(connectionTest.kind, "connection");
  assert.equal((connectionTest.input as Record<string, unknown>).type, "text");
  assert.equal((connectionTest.input as Record<string, unknown>).fileUpload, "never");
  assert.doesNotMatch(JSON.stringify(connectionTest), /bundled-pdf|multipart/i);
  assert.ok(
    !["may-upload", "always"].includes(
      (connectionTest.input as Record<string, unknown>).fileUpload as string,
    ),
    "the plain connection test must never upload files",
  );

  const pdfTest = actionBlock(connection, "test-pdf-extraction");
  assert.equal(pdfTest.kind, "pdf-extraction");
  assert.equal((pdfTest.input as Record<string, unknown>).type, "bundled-pdf");
  assert.equal((pdfTest.input as Record<string, unknown>).fixture, "host-minimal-pdf-v1");
  assert.equal((pdfTest.input as Record<string, unknown>).fileUpload, "may-upload");

  const transport = settingBlock("pdf-transport");
  const transportOptions = ((transport.options ?? []) as { value?: string }[]).map(
    (option) => option.value,
  );
  assert.ok(transportOptions.includes("local-text"));
  assert.ok(transportOptions.includes("kimi-file-extract"));

  const pipeline = read("plugins/sources/myc.pdf-canvas-agent/agent.yml");
  assert.match(
    pipeline,
    /pdfInput:[\s\S]*?mode:\s+kimi-files[\s\S]*?purpose:\s+file-extract[\s\S]*?modelReceives:\s+extracted-text[\s\S]*?modelReceivesPdfBytes:\s+false/,
  );
});

test("private plugin i18n contains Kimi-specific English and Chinese copy", () => {
  const english = JSON.parse(read(englishLocalePath)) as Record<string, string>;
  const chinese = JSON.parse(read(chineseLocalePath)) as Record<string, string>;

  assert.match(english["settings.pdfTransport.options.kimiFiles"] ?? "", /Kimi Files/i);
  assert.match(english["settings.pdfTransport.description"] ?? "", /Moonshot|Kimi/i);
  assert.match(chinese["settings.pdfTransport.options.kimiFiles"] ?? "", /Kimi|Moonshot|上传/);
  assert.match(chinese["settings.pdfTransport.description"] ?? "", /Kimi|Moonshot|上传/);
});

test("K2.6 provider logic lives under native_plugins, while generic Kimi Files guards remain allowed", () => {
  const nativeRoot = "src-tauri/src/native_plugins";
  const nativeFiles = collectFiles(nativeRoot).filter((path) => /\.(rs|toml)$/.test(path));
  assert.ok(
    nativeFiles.length > 0,
    "provider-specific native Rust code must be placed under src-tauri/src/native_plugins",
  );

  const nativeSource = nativeFiles.map(read).join("\n");
  assert.match(nativeSource, /Kimi|Moonshot/i);
  assert.match(nativeSource, /kimi[-_. ]?k2\.6|K2\.6|temperature|thinking/i);

  const genericLlmClient = read("src-tauri/src/llm_client.rs");
  const forbiddenK26Markers = [
    /kimi[-_. ]?k2\.6/i,
    /KimiK26/i,
    /k2_6/i,
    /(?:if|else if|match|matches)[^\n]*(?:Kimi|Moonshot)[^\n]*(?:temperature|thinking)/i,
  ];
  for (const marker of forbiddenK26Markers) {
    assert.doesNotMatch(
      genericLlmClient,
      marker,
      "generic llm_client.rs may retain existing Kimi Files safeguards, but not new K2.6 provider branches",
    );
  }
});
