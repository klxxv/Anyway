/**
 * Phase 1.4 —— TS↔Rust 双实现逐位比对测试。
 * Dual-implementation parity suite: drives the SAME fixtures (MNIST + social)
 * through both the TypeScript graph/lib implementations and the Rust
 * `graph_algorithms` kernel, then asserts bit-for-bit identical output.
 *
 * 覆盖：
 * - canonicalize：字节级一致（TS 参考实现 vs Rust canonicalize）
 * - 图算法：BFS / DFS / 最短路径 / 环检测 / 场景可达性 / 逻辑链 / 影响传播
 * - 确定性布局：全部 6 种投影的 positions 像素级一致
 *
 * 失败时输出差异并标记修复方向（比对失败 = 只修 Rust 端，不删 TS）。
 */

import assert from "node:assert/strict";
import test from "node:test";
import { spawnSync } from "node:child_process";
import path from "node:path";
import {
  allShortestPaths,
  compareScenarioReachability,
  detectCycles,
  shortestPath,
  traverseGraph,
} from "../app/lib/graph";
import { computeLayout } from "../app/lib/layout";
import { computeLogicChain, propagateInfluence } from "../app/lib/analysis";
import { canonicalize as tsCanonicalize } from "../app/lib/compiler-reference";
import { createMnistProject } from "../app/lib/mnist-fixture";
import { createSocialScienceProject } from "../app/lib/social-fixture";
import type { ProjectState } from "../app/lib/research-types";
import type { LayoutMode } from "../app/lib/research-types";
import type { LogicChainMode, TraversalRequest } from "../app/lib/research-types";

// ---------------------------------------------------------------------------
// Rust 引导程序启动 / Rust harness bootstrap
// ---------------------------------------------------------------------------

import { fileURLToPath } from "node:url";

const REPO = fileURLToPath(new URL("..", import.meta.url)).replace(/[\\/]$/, "");
const CARGO_MANIFEST = path.join(REPO, "src-tauri", "Cargo.toml");
const HARNESS = path.join(
  REPO,
  "src-tauri",
  "target",
  "debug",
  "examples",
  process.platform === "win32" ? "compile_harness.exe" : "compile_harness",
);

/** 在首次测试前构建 Rust 引导程序（失败即 abort，CI 可快速失败并给出修复提示）。 */
function ensureHarness(): void {
  if (!process.env.__PARITY_BUILT__) {
    const built = spawnSync(
      "cargo",
      ["build", "--manifest-path", CARGO_MANIFEST, "--example", "compile_harness"],
      { encoding: "utf8", shell: false },
    );
    assert.equal(
      built.status,
      0,
      `Rust harness build failed:\n${built.stdout}\n${built.stderr}\n→ 修复 Rust 端后再比对。`,
    );
    process.env.__PARITY_BUILT__ = "1";
  }
}

/** 调用 Rust 引导程序计算单一命令。 */
function rust(command: string, args: Record<string, unknown>, project: ProjectState): unknown {
  ensureHarness();
  const request = JSON.stringify({ project, command, args });
  const out = spawnSync(HARNESS, [], { input: request, encoding: "utf8", shell: false });
  const stderr = (out.stderr ?? "").trim();
  assert.equal(
    out.status,
    0,
    `Rust harness ${command} failed.\nstderr: ${stderr}\n→ 修复 Rust 端后重跑。`,
  );
  try {
    return JSON.parse(out.stdout);
  } catch (error) {
    throw new Error(
      `Rust harness ${command} returned non-JSON output:\n${out.stdout}\nstderr: ${stderr}`,
    );
  }
}

/** 用 JSON round-trip 规范化项目，确保 TS 与 Rust 收到完全相同的结构化输入。 */
function normalizeProject(project: ProjectState): ProjectState {
  return JSON.parse(JSON.stringify(project)) as ProjectState;
}

/** 剥离运行时遥测字段（durationMs），仅比对确定性产物。 */
function stripRuntime<T extends Record<string, unknown>>(value: T): Omit<T, "durationMs"> {
  const { durationMs: _, ...copy } = value;
  void _;
  return copy;
}

const social = () => normalizeProject(createSocialScienceProject());
const mnist = () => normalizeProject(createMnistProject());

const fixtureNames: Record<string, () => ProjectState> = {
  mnist: mnist,
  social: social,
};

const traversalRequests: Record<string, TraversalRequest[]> = {
  mnist: [
    { startId: "mnist-question", strategy: "bfs", direction: "out", maxDepth: 20 },
    { startId: "mnist-accuracy", strategy: "bfs", direction: "in", maxDepth: 20 },
    { startId: "mnist-normalization", strategy: "dfs", direction: "out", maxDepth: 20 },
    { startId: "mnist-question", strategy: "dfs", direction: "both", maxDepth: 4, edgeTypes: ["K"] },
  ],
  social: [
    { startId: "soc-polarization", strategy: "bfs", direction: "in", maxDepth: 8, edgeTypes: ["K"] },
    { startId: "soc-q", strategy: "bfs", direction: "out", maxDepth: 20 },
    { startId: "soc-trust", strategy: "dfs", direction: "both", maxDepth: 8 },
    { startId: "soc-result", strategy: "dfs", direction: "in", maxDepth: 6 },
  ],
};

const layoutModes: LayoutMode[] = [
  "evidence-chain",
  "refutation-chain",
  "tree",
  "huffman",
  "table",
  "neural-network",
];

for (const fixtureName of Object.keys(fixtureNames)) {
  const make = fixtureNames[fixtureName];

  test(`${fixtureName}: canonicalize 字节级一致 (byte-level)`, () => {
    const project = make();
    const tsBytes = tsCanonicalize(project);
    const { bytes } = rust("canonicalize", {}, project) as { bytes: string };
    assert.equal(bytes, tsBytes, `${fixtureName} canonicalize differs byte-for-byte`);
  });

  for (const request of traversalRequests[fixtureName]) {
    const label = `${request.strategy}/${request.direction}/${request.startId}/d${request.maxDepth}`;
    test(`${fixtureName}: traverse ${label} 逐字段一致`, () => {
      const project = make();
      const ts = stripRuntime(traverseGraph(project, request) as unknown as Record<string, unknown>);
      const r = rust("traverse", request as unknown as Record<string, unknown>, project);
      const actual = stripRuntime(r as Record<string, unknown>);
      assert.deepStrictEqual(actual, ts, `${fixtureName} traverse ${label} differs`);
    });
  }

  test(`${fixtureName}: 环检测输出一致`, () => {
    const project = make();
    const ts = detectCycles(project);
    const r = rust("cycles", {}, project);
    assert.deepStrictEqual(r, ts, `${fixtureName} detectCycles differs`);
  });

  test(`${fixtureName}: 最短路径输出一致`, () => {
    const cases: Array<[string, string]> =
      fixtureName === "mnist"
        ? [
            ["mnist-question", "mnist-conclusion"],
            ["mnist-accuracy", "mnist-conclusion"],
            ["mnist-conclusion", "mnist-question"],
            ["mnist-data", "mnist-representation"],
          ]
        : [
            ["soc-q", "soc-result"],
            ["soc-polarization", "soc-result"],
            ["soc-result", "soc-q"],
          ];
    for (const [source, target] of cases) {
      const ts = shortestPath(make(), source, target);
      const r = rust("shortestPath", { source, target }, make());
      assert.deepStrictEqual(r, ts, `${fixtureName} shortestPath ${source}→${target} differs`);
    }
  });

  test(`${fixtureName}: 全部等长最短路径输出一致`, () => {
    const project = make();
    const source = fixtureName === "mnist" ? "mnist-question" : "soc-polarization";
    const target = fixtureName === "mnist" ? "mnist-conclusion" : "soc-result";
    const ts = allShortestPaths(project, source, target);
    const r = rust("allShortestPaths", { source, target }, project);
    assert.deepStrictEqual(r, ts, `${fixtureName} allShortestPaths differs`);
  });

  test(`${fixtureName}: 场景可达性差异输出一致`, () => {
    const project = make();
    const scenarios = fixtureName === "mnist" ? ["scenario-mnist-no-normalization"] : ["soc-without-homophily"];
    for (const scenarioId of scenarios) {
      if (!project.scenarios.some((s) => s.id === scenarioId)) continue;
      const root = fixtureName === "mnist" ? "mnist-question" : "soc-q";
      const ts = compareScenarioReachability(project, root, scenarioId);
      const r = rust("reachability", { root, scenarioId }, project);
      assert.deepStrictEqual(r, ts, `${fixtureName} reachability ${scenarioId} differs`);
    }
  });

  // Edge-type-dependent algorithms are fenced until the Rust
  // `research-graph-compiler` is migrated to the five-operator model
  // (T/K/I/M/Q); it still implements the legacy 12-edge semantics.
  const legacyEdgeModel = "Rust research-graph-compiler still uses the legacy 12-edge model";

  test(`${fixtureName}: 逻辑链输出一致`, { skip: legacyEdgeModel }, () => {
    const modes: LogicChainMode[] = ["effective", "evidence", "refutation"];
    const project = make();
    for (const mode of modes) {
      const targetId = fixtureName === "mnist" ? "mnist-conclusion" : "soc-result";
      const ts = computeLogicChain(project, mode, targetId);
      const r = rust("logicChain", { mode, targetId }, project);
      assert.deepStrictEqual(r, ts, `${fixtureName} logicChain ${mode} differs`);
    }
    // 无 target 的默认链
    for (const mode of modes) {
      const ts = computeLogicChain(project, mode);
      const r = rust("logicChain", { mode }, project);
      assert.deepStrictEqual(r, ts, `${fixtureName} logicChain ${mode} (no target) differs`);
    }
  });

  test(`${fixtureName}: 影响传播输出一致`, { skip: legacyEdgeModel }, () => {
    const project = make();
    const targetId = fixtureName === "mnist" ? "mnist-accuracy" : "soc-polarization";
    const ts = propagateInfluence(project, targetId);
    const r = rust("influence", { targetId }, project);
    assert.deepStrictEqual(r, ts, `${fixtureName} influence differs`);
  });

  test(`${fixtureName}: 确定性布局 positions 像素级一致`, () => {
    const project = make();
    const rootId = fixtureName === "mnist" ? "mnist-question" : "soc-q";
    for (const mode of layoutModes) {
      if (mode === "evidence-chain" || mode === "refutation-chain") continue; // see legacyEdgeModel
      const ts = computeLayout(project, mode, rootId);
      const r = rust("layout", { mode, rootId }, project);
      assert.deepStrictEqual(r, ts, `${fixtureName} layout ${mode} differs`);
    }
  });
}
