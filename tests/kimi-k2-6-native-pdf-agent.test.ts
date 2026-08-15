import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const manifestPath = "plugins/sources/myc.pdf-canvas-agent/plugin.yml";
const descriptorPath = "plugins/sources/myc.pdf-canvas-agent/agent-manifest.json";
const englishLocalePath = "plugins/sources/myc.pdf-canvas-agent/locales/en.json";
const chineseLocalePath = "plugins/sources/myc.pdf-canvas-agent/locales/zh-CN.json";

const read = (path: string) => readFileSync(path, "utf8");

function settingBlock(manifest: string, id: string): string {
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const start = new RegExp(`^    - id: ${escapedId}\\r?\\n`, "m").exec(manifest);
  assert.ok(start, `manifest must declare setting ${id}`);
  const bodyStart = start.index + start[0].length;
  const remainder = manifest.slice(bodyStart);
  const boundaries = [remainder.search(/\r?\n    - id: /), remainder.search(/\r?\n  connections:/)]
    .filter((index) => index >= 0);
  const bodyEnd = boundaries.length > 0 ? Math.min(...boundaries) : remainder.length;
  return manifest.slice(start.index, bodyStart + bodyEnd);
}

function connectionBlock(manifest: string): string {
  const match = manifest.match(/^  connections:\r?\n([\s\S]*)$/m);
  assert.ok(match, "manifest must declare a provider connection");
  return match[1];
}

function actionBlock(connection: string, id: string): string {
  const escapedId = id.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const start = new RegExp(`^        - id: ${escapedId}\\r?\\n`, "m").exec(connection);
  assert.ok(start, `connection must declare action ${id}`);
  const bodyStart = start.index + start[0].length;
  const remainder = connection.slice(bodyStart);
  const boundaries = [
    remainder.search(/\r?\n        - id: /),
    remainder.search(/\r?\n      testAction:/),
  ].filter((index) => index >= 0);
  const bodyEnd = boundaries.length > 0 ? Math.min(...boundaries) : remainder.length;
  return connection.slice(start.index, bodyStart + bodyEnd);
}

function collectFiles(root: string): string[] {
  if (!existsSync(root)) return [];
  return readdirSync(root, { withFileTypes: true }).flatMap((entry) => {
    const path = join(root, entry.name);
    return entry.isDirectory() ? collectFiles(path) : [path];
  });
}

test("Kimi K2.6 is the native PDF Agent model and requires a user-provided URL", () => {
  const manifest = read(manifestPath);

  const apiUrl = settingBlock(manifest, "api-url");
  assert.match(apiUrl, /^      required: true$/m);
  assert.doesNotMatch(apiUrl, /^      default:/m);
  assert.match(apiUrl, /^      placeholder: https:\/\/api\.moonshot\.cn\/v1$/m);
  assert.match(settingBlock(manifest, "api-format"), /^      default: openai$/m);
  assert.match(settingBlock(manifest, "model"), /^      default: kimi-k2\.6$/m);
  assert.match(
    settingBlock(manifest, "credential-env-var"),
    /^      default: MOONSHOT_API_KEY$/m,
  );

  const connection = connectionBlock(manifest);
  assert.match(
    connection,
    /apiKey:[\s\S]*?source:\s+environment[\s\S]*?name:\s+MOONSHOT_API_KEY/,
  );
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
  const manifest = read(manifestPath);
  const connection = connectionBlock(manifest);
  const actionIds = [...connection.matchAll(/^        - id: (test-[^\s]+)/gm)].map(
    ([, id]) => id,
  );

  assert.deepEqual(actionIds, ["test-connection", "test-pdf-extraction"]);
  assert.doesNotMatch(
    connection,
    /^      testAction:/m,
    "the legacy singular action must not duplicate the two canonical tests",
  );

  const connectionTest = actionBlock(connection, "test-connection");
  assert.match(connectionTest, /kind:\s+connection/);
  assert.match(connectionTest, /type:\s+text/);
  assert.match(connectionTest, /fileUpload:\s+never/);
  assert.doesNotMatch(connectionTest, /bundled-pdf|multipart/i);
  assert.doesNotMatch(connectionTest, /fileUpload:\s+(?:may-upload|always)/i);

  const pdfTest = actionBlock(connection, "test-pdf-extraction");
  assert.match(pdfTest, /kind:\s+pdf-extraction/);
  assert.match(pdfTest, /type:\s+bundled-pdf/);
  assert.match(pdfTest, /fixture:\s+host-minimal-pdf-v1/);
  assert.match(pdfTest, /fileUpload:\s+may-upload/);

  const transport = settingBlock(manifest, "pdf-transport");
  assert.match(transport, /value:\s+local-text/);
  assert.match(transport, /value:\s+kimi-file-extract/);

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
