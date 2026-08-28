import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";
import {
  collectContractViolations,
  formatContractReport,
  REQUIRED_PINIA_CONSUMERS,
  REQUIRED_PINIA_FACADES,
  REQUIRED_PINIA_STORES,
} from "../scripts/check-vue-interface-contract.mjs";

const repositoryRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..");

test("Pinia architecture inventory has unique store ids and explicit facades", () => {
  const storeIds = REQUIRED_PINIA_STORES.map((store) => store.id);
  assert.equal(new Set(storeIds).size, storeIds.length, "Pinia store ids must be unique");
  assert.ok(storeIds.includes("project"), "the project store is a protected boundary");
  assert.ok(storeIds.includes("canvas-interaction"), "the canvas interaction store is a protected boundary");
  assert.ok(storeIds.includes("workspace-ui"), "the workspace UI store is a protected boundary");
  assert.ok(storeIds.includes("runtime-plugin-host"), "the plugin host store is a protected boundary");
  assert.ok(storeIds.includes("runtime-i18n"), "the i18n store is a protected boundary");
  assert.ok(storeIds.includes("runtime-auth"), "the auth store is a protected boundary");
  assert.ok(
    REQUIRED_PINIA_FACADES.some((facade) => facade.exportName === "useWorkspaceProject"),
    "the historical workspace composable must remain a compatibility facade",
  );
  assert.ok(
    REQUIRED_PINIA_CONSUMERS.some((consumer) => consumer.storeName === "useCanvasInteractionStore"),
    "the canvas Pinia consumption boundary must remain explicit",
  );
  assert.ok(
    REQUIRED_PINIA_CONSUMERS.some((consumer) => consumer.storeName === "useWorkspaceUiStore"),
    "the workspace UI Pinia consumption boundary must remain explicit",
  );
  assert.ok(
    REQUIRED_PINIA_CONSUMERS.some((consumer) => consumer.storeName === "useRuntimePluginHostStore"),
    "the plugin host compatibility bridge must remain explicit",
  );
  assert.ok(
    REQUIRED_PINIA_CONSUMERS.some((consumer) => consumer.storeName === "useRuntimeI18nStore"),
    "the i18n compatibility bridge must remain explicit",
  );
  assert.ok(
    REQUIRED_PINIA_CONSUMERS.some((consumer) => consumer.storeName === "useRuntimeAuthStore"),
    "the auth compatibility bridge must remain explicit",
  );
});

test("Pinia setup stores and compatibility facades satisfy the frozen renderer contract", () => {
  const violations = collectContractViolations(repositoryRoot);
  assert.deepEqual(violations, [], formatContractReport(violations));

  for (const store of REQUIRED_PINIA_STORES) {
    const source = readFileSync(resolve(repositoryRoot, store.file), "utf8");
    assert.match(source, /defineStore\s*\(\s*["'][^"']+["']\s*,\s*\(\s*\)\s*=>/);
    assert.doesNotMatch(source, /defineStore\s*\([^,]+,\s*\{/);
  }
});
