import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  collectContractViolations,
  formatContractReport,
} from "../scripts/check-vue-interface-contract.mjs";

const repositoryRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..");

test("framework-agnostic interface contract has no missing or changed entries", () => {
  const violations = collectContractViolations(repositoryRoot);
  assert.deepEqual(violations, [], formatContractReport(violations));
});

test("Pinia renderer architecture is part of the interface contract", () => {
  const violations = collectContractViolations(repositoryRoot);
  assert.deepEqual(
    violations,
    [],
    `${formatContractReport(violations)}\nPinia registration, setup stores, compatibility facades, and renderer boundaries are frozen contract inputs.`,
  );
});

test("standalone interface checker returns a useful process result", () => {
  const result = spawnSync(
    process.execPath,
    [resolve(repositoryRoot, "scripts/check-vue-interface-contract.mjs")],
    { cwd: repositoryRoot, encoding: "utf8" },
  );
  const output = `${result.stdout}${result.stderr}`.trim();
  assert.equal(result.error, undefined, output || "interface checker could not start");
  assert.equal(result.status, 0, output || "interface checker exited without a report");
  assert.match(output, /Vue interface contract: PASS/);
});
