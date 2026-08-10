import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

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
  assert.match(rust, /pub trait SettingReader/);
  assert.match(cpp, /struct PluginSettingDefinition/);
  assert.match(cpp, /class SettingsReader/);
  assert.match(python, /class PluginSetting/);
  assert.match(python, /class SettingReader\(Protocol\)/);
  assert.match(python, /"writeOnly": self\.write_only or self\.is_secret/);
  assert.doesNotMatch(rust, /get_secret/i);
  assert.doesNotMatch(cpp, /getSecret\s*\(/);
  assert.doesNotMatch(python, /get_secret/);
});
