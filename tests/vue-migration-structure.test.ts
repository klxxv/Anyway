import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { basename, join, relative, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

const repositoryRoot = resolve(fileURLToPath(new URL(".", import.meta.url)), "..");
const sourceRoot = resolve(repositoryRoot, "src");

function walkFiles(root) {
  if (!existsSync(root)) return [];
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const absolutePath = join(root, entry.name);
    if (entry.isDirectory()) files.push(...walkFiles(absolutePath));
    else files.push(absolutePath);
  }
  return files;
}

const sourceFiles = () => walkFiles(sourceRoot);
const sourceRelativePath = (file) => relative(repositoryRoot, file).replaceAll("\\", "/");

function findByCandidates(candidates) {
  return candidates.find((candidate) => existsSync(resolve(repositoryRoot, candidate)));
}

function findVueFile(names, requiredPathPart) {
  const normalizedNames = new Set(names.map((name) => name.toLowerCase()));
  return sourceFiles().find((file) => {
    const relativePath = sourceRelativePath(file).toLowerCase();
    return (
      normalizedNames.has(basename(file).toLowerCase()) &&
      (!requiredPathPart || relativePath.includes(requiredPathPart.toLowerCase()))
    );
  });
}

test("Vue/Vite entry and runtime adapters exist", () => {
  const missing = [];
  if (!findByCandidates(["src/main.ts"])) missing.push("src/main.ts");
  if (!findByCandidates(["src/App.vue"])) missing.push("src/App.vue");
  if (!findByCandidates(["src/vue/runtime", "src/runtime"])) {
    missing.push("src/vue/runtime/ (or src/runtime/)");
  }
  for (const [label, candidates] of [
    ["i18n runtime adapter", ["src/vue/runtime/i18n.ts", "src/runtime/i18n.ts"]],
    [
      "plugin host runtime adapter",
      ["src/vue/runtime/plugin-host.ts", "src/runtime/plugin-host.ts"],
    ],
    [
      "Tauri platform runtime adapter",
      ["src/vue/runtime/tauri-client.ts", "src/runtime/tauri-client.ts"],
    ],
  ]) {
    if (!findByCandidates(candidates)) missing.push(`${label}: ${candidates.join(" or ")}`);
  }
  assert.deepEqual(missing, [], `Missing Vue runtime structure:\n${missing.map((item) => `- ${item}`).join("\n")}`);
});

test("workspace composables, canvas, workspace shell, and panels exist", () => {
  const missing = [];
  for (const [label, names, pathPart] of [
    ["workspace project composable", ["use-workspace-project.ts", "useWorkspaceProject.ts"], "composable"],
    ["canvas diff composable", ["use-canvas-diff.ts", "useCanvasDiff.ts"], "composable"],
    ["trackpad composable", ["use-trackpad-pinch.ts", "useTrackpadPinch.ts"], "composable"],
    ["research graph canvas", ["research-graph-canvas.vue", "ResearchGraphCanvas.vue"], "canvas"],
    ["research workspace app", ["ResearchWorkspaceApp.vue", "research-workspace-app.vue"], ""],
    ["workspace topbar", ["workspace-topbar.vue", "WorkspaceTopbar.vue"], ""],
    ["inspector panel", ["inspector-panel.vue", "InspectorPanel.vue"], ""],
    ["workspace dialogs", ["workspace-dialogs.vue", "WorkspaceDialogs.vue"], ""],
    ["workspace context menu", ["workspace-context-menu.vue", "WorkspaceContextMenu.vue"], ""],
    ["agent review panel", ["agent-review-panel.vue", "AgentReviewPanel.vue"], ""],
    ["diff panel", ["diff-panel.vue", "DiffPanel.vue"], ""],
    ["plugin store dialog", ["plugin-store-dialog.vue", "PluginStoreDialog.vue"], ""],
  ]) {
    if (!findVueFile(names, pathPart)) missing.push(`${label}: ${names.join(" or ")}`);
  }
  assert.deepEqual(
    missing,
    [],
    `Missing Vue migration structure:\n${missing.map((item) => `- ${item}`).join("\n")}`,
  );
});

test("Vite is configured for Vue and the package exposes the Vue toolchain", () => {
  const packageJson = JSON.parse(readFileSync(resolve(repositoryRoot, "package.json"), "utf8"));
  const viteConfig = readFileSync(resolve(repositoryRoot, "vite.config.ts"), "utf8");
  const dependencies = { ...packageJson.dependencies, ...packageJson.devDependencies };
  const missingPackages = ["vue", "pinia", "@vue-flow/core", "@vitejs/plugin-vue", "vue-tsc"].filter(
    (name) => !dependencies[name],
  );
  assert.deepEqual(missingPackages, [], `Missing Vue migration packages: ${missingPackages.join(", ")}`);
  assert.match(viteConfig, /@vitejs\/plugin-vue/, "vite.config.ts must load @vitejs/plugin-vue");
  assert.match(viteConfig, /plugins\s*:\s*\[[\s\S]*vue\(\)/, "vite.config.ts must register vue()");
  for (const dependency of [
    "vue",
    "pinia",
    "@vue-flow/core",
    "@vue-flow/background",
    "@vue-flow/controls",
    "@vue-flow/minimap",
    "@tabler/icons-vue",
    "jspdf",
  ]) {
    assert.match(
      viteConfig,
      new RegExp(`include:[\\s\\S]*["']${dependency.replace(/[.*+?^${}()|[\\]\\]/g, "\\\\$&")}\"?`),
      `vite.config.ts must pre-bundle ${dependency}`,
    );
  }
  assert.match(viteConfig, /warmup\s*:\s*\{[\s\S]*clientFiles\s*:/, "vite.config.ts must warm the renderer entry");
  assert.match(viteConfig, /src\/vue\/ResearchWorkspaceApp\.vue/, "Vite warmup must include the workspace shell");
});

test("workspace shell lazy-loads conditional heavy panels", () => {
  const workspaceShell = readFileSync(
    resolve(repositoryRoot, "src/vue/ResearchWorkspaceApp.vue"),
    "utf8",
  );

  for (const [name, path] of [
    ["AgentReviewPanel", "./components/AgentReviewPanel.vue"],
    ["DiffPanel", "./components/DiffPanel.vue"],
    ["PdfUploadDialog", "./components/PdfUploadDialog.vue"],
    ["PluginStoreDialog", "./components/PluginStoreDialog.vue"],
    ["WorkspaceDialogs", "./components/WorkspaceDialogs.vue"],
    ["WorkspacePluginDialogs", "./components/WorkspacePluginDialogs.vue"],
  ]) {
    assert.match(
      workspaceShell,
      new RegExp(`const\\s+${name}\\s*=\\s*defineAsyncComponent\\(\\(\\)\\s*=>\\s*import\\(\\s*["']${path.replace(/[.*+?^${}()|[\\]\\]/g, "\\\\$&")}["']\\s*\\)\\s*\\)`),
      `${name} must be loaded with defineAsyncComponent`,
    );
    assert.doesNotMatch(
      workspaceShell,
      new RegExp(`import\\s+${name}\\s+from\\s+["']${path.replace(/[.*+?^${}()|[\\]\\]/g, "\\\\$&")}`),
      `${name} must not be synchronously imported by the workspace shell`,
    );
  }

  for (const [name, path] of [
    ["ResearchGraphCanvas", "./canvas/ResearchGraphCanvas.vue"],
    ["InspectorPanel", "./components/InspectorPanel.vue"],
    ["WorkspaceTopbar", "./components/WorkspaceTopbar.vue"],
  ]) {
    assert.match(
      workspaceShell,
      new RegExp(`import\\s+${name}\\s+from\\s+["']${path.replace(/[.*+?^${}()|[\\]\\]/g, "\\\\$&")}`),
      `${name} must remain synchronous for the initial workspace shell`,
    );
  }
});

test("Pinia is registered once and all renderer stores use setup-style definitions", () => {
  const entry = readFileSync(resolve(repositoryRoot, "src/main.ts"), "utf8");
  const piniaRoot = readFileSync(resolve(repositoryRoot, "src/vue/stores/pinia.ts"), "utf8");
  const storeRoot = resolve(repositoryRoot, "src/vue/stores");
  const storeFiles = walkFiles(storeRoot).filter(
    (file) => file.endsWith(".ts") && basename(file) !== "pinia.ts",
  );

  assert.match(entry, /import\s*\{[^}]*\bpinia\b[^}]*\}\s*from\s*["']\.\/vue\/stores\/pinia["']/, "src/main.ts must import the shared Pinia root");
  assert.equal((entry.match(/\.use\(\s*pinia\s*\)/g) ?? []).length, 1, "Pinia must be registered exactly once");
  assert.match(piniaRoot, /createPinia\s*\(\s*\)/, "the shared root must call createPinia()");
  assert.match(piniaRoot, /export\s+const\s+pinia\b/, "the shared root must export pinia");
  assert.ok(storeFiles.length >= 2, "the renderer must have more than one Pinia setup store");

  const violations = storeFiles.flatMap((file) => {
    const source = readFileSync(file, "utf8");
    const relativePath = sourceRelativePath(file);
    const problems = [];
    if (!/from\s+["']pinia["']/.test(source)) problems.push("does not import Pinia");
    if (!/defineStore\s*\(\s*["'][^"']+["']\s*,\s*\(\s*\)\s*=>/.test(source)) {
      problems.push("is not a setup-style defineStore");
    }
    if (/defineStore\s*\([^,]+,\s*\{/.test(source)) problems.push("uses an option-style store");
    return problems.map((problem) => `${relativePath}: ${problem}`);
  });
  assert.deepEqual(violations, [], `Pinia store violations:\n${violations.map((item) => `- ${item}`).join("\n")}`);
});

test("Pinia compatibility facades preserve the project refs and consumption boundaries", () => {
  const facade = readFileSync(resolve(repositoryRoot, "src/vue/composables/use-workspace-project.ts"), "utf8");
  const canvas = readFileSync(resolve(repositoryRoot, "src/vue/canvas/ResearchGraphCanvas.vue"), "utf8");
  const facadeFields = [
    "project",
    "projectRef",
    "history",
    "selectedNode",
    "selectedNodeId",
    "selectedEdge",
    "selectedEdgeId",
    "canUndo",
    "canRedo",
  ];

  assert.match(facade, /export\s+function\s+useWorkspaceProject\b/);
  assert.match(facade, /\buseProjectStore\s*\(/, "the legacy composable must delegate to Pinia");
  assert.match(facade, /\bstoreToRefs\s*\(/, "the legacy composable must preserve ref semantics with storeToRefs");
  assert.match(canvas, /\buseCanvasInteractionStore\s*\(/, "the canvas must consume its Pinia store");
  assert.match(canvas, /\bstoreToRefs\s*\(/, "the canvas must destructure reactive store state with storeToRefs");
  for (const field of facadeFields) {
    assert.match(facade, new RegExp(`\\b${field}\\s*:`), `facade field ${field} must remain public`);
  }
});

test("plugin store reuses the Vue item component and the Pinia host bridge", () => {
  const dialog = readFileSync(
    resolve(repositoryRoot, "src/vue/components/PluginStoreDialog.vue"),
    "utf8",
  );

  assert.match(dialog, /import\s+PluginStoreItem\s+from\s+["']\.\/PluginStoreItem\.vue["']/);
  assert.match(dialog, /import\s*\{\s*usePluginHost\s*\}\s*from\s*["']\.\.\/runtime\/plugin-host["']/);
  assert.match(dialog, /\busePluginHost\s*\(\s*\)/, "the dialog must share the runtime Pinia plugin host");
  assert.ok(
    (dialog.match(/<PluginStoreItem\b/g) ?? []).length >= 2,
    "built-in and installed plugin entries must reuse PluginStoreItem",
  );
  assert.doesNotMatch(dialog, /<article\s+v-for=["']plugin\s+in/);
  assert.doesNotMatch(
    dialog,
    /\b(?:listInstalledMycPlugins|installMycPlugin|enabledPluginsStorageKey)\b/,
    "the dialog must not maintain a second plugin-host state source",
  );
});

test("Vue canvas composes native touchpad frames and dispatches radial flick selections", () => {
  const canvas = readFileSync(
    resolve(repositoryRoot, "src/vue/canvas/ResearchGraphCanvas.vue"),
    "utf8",
  );

  assert.match(canvas, /lowPassCompleteTrackpadFrame\(/);
  assert.match(canvas, /viewportForCompleteTrackpadFrame\(\s*originViewport/);
  assert.match(canvas, /viewportForCoalescedWheelFrame\(/);
  assert.match(canvas, /radialSelectionForNormalizedDisplacement\(/);
  assert.match(canvas, /<RadialAddMenu\b/);
  assert.match(canvas, /function handleChromiumWheel\(/);
  assert.doesNotMatch(canvas, /function handleWheel\(/);
});

test("Vue components use separated SFC script setup, template, and style blocks", () => {
  const violations = sourceFiles()
    .filter((file) => file.endsWith(".vue"))
    .flatMap((file) => {
      const source = readFileSync(file, "utf8");
      const relativePath = sourceRelativePath(file);
      const problems = [];
      if (!/<script\s+setup\s+lang=["']ts["']\s*>/.test(source)) {
        problems.push("missing <script setup lang=\"ts\">");
      }
      if (!/<template(?:\s[^>]*)?>/.test(source)) problems.push("missing <template>");
      if (!/<style(?:\s[^>]*)?>/.test(source)) problems.push("missing <style>");
      if (/\brender\s*\(|\bjsx\b|<[A-Za-z][^>]*>\s*=>/.test(source)) {
        problems.push("contains render/JSX implementation");
      }
      return problems.map((problem) => `${relativePath}: ${problem}`);
    });

  assert.deepEqual(
    violations,
    [],
    `Vue SFC structure violations:\n${violations.map((item) => `- ${item}`).join("\n")}`,
  );
});

test("Vue source does not embed Rust or duplicate project/GraphPatch schemas", () => {
  const violations = [];
  const forbiddenRustImport = /(?:src-tauri|#\s*\[\s*tauri::|\b(?:use|extern\s+crate)\s+(?:tauri|serde)\b|\bpub\s+fn\b)/;
  const forbiddenSchema = [
    /\b(?:interface|type)\s+(?:ProjectState|GraphPatch|PluginGraphPatch|GraphPatchOperation)\b/,
    /\b(?:const|let|var)\s+(?:CURRENT_SCHEMA_VERSION|NODE_TYPES|EDGE_TYPES|LAYOUT_MODES)\s*=/,
    /\bschemaVersion\s*:\s*\d+\b/,
    /\bapiVersion\s*:\s*["']researchcanvas\.dev\/graph-patch\//,
    /researchcanvas\.dev\/graph-patch\/v1alpha1/,
  ];
  const forbiddenVuex = /(?:from\s*["']vuex["']|require\(\s*["']vuex["']\s*\)|\bcreateStore\s*\(|\buseStore\s*\()/;
  const forbiddenDirectRust = /(?:@tauri-apps\/api\/core|\binvoke\s*(?:<[^>]*>)?\s*\()/;

  for (const file of sourceFiles()) {
    const relativePath = sourceRelativePath(file);
    const source = readFileSync(file, "utf8");
    if (file.endsWith(".rs")) violations.push(`${relativePath}: Rust source must stay under src-tauri/`);
    if (forbiddenRustImport.test(source)) {
      violations.push(`${relativePath}: Vue layer contains a Rust implementation/import marker`);
    }
    const isVueLayer = /src\/vue\//i.test(relativePath);
    const isUiLayer = isVueLayer || /src\/(?:components|composables|features)\//i.test(relativePath);
    if (isUiLayer && /(?:@tauri-apps\/api\/core|\binvoke\s*\()/.test(source)) {
      violations.push(`${relativePath}: UI/composable code must call the platform adapter, not Rust invoke directly`);
    }
    if (isVueLayer && forbiddenVuex.test(source)) {
      violations.push(`${relativePath}: Vue layer must use Pinia, not Vuex or a Vuex-style store API`);
    }
    if (isVueLayer && forbiddenDirectRust.test(source)) {
      violations.push(`${relativePath}: Vue layer must call the platform facade, not Rust invoke directly`);
    }
    for (const schemaPattern of forbiddenSchema) {
      if (isVueLayer && schemaPattern.test(source)) {
        violations.push(`${relativePath}: Vue layer duplicates or defines a persisted project/GraphPatch schema`);
        break;
      }
    }
  }

  assert.deepEqual(
    violations,
    [],
    `Vue layering violations:\n${violations.map((item) => `- ${item}`).join("\n")}`,
  );
});
