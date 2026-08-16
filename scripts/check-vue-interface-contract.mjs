import { existsSync, readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
export const REPOSITORY_ROOT = resolve(SCRIPT_DIR, "..");

/**
 * These are renderer-agnostic entry points. Keep this list explicit: a missing
 * entry is a migration break even when the TypeScript compiler still passes.
 */
export const REQUIRED_EXPORTS = Object.freeze({
  "app/lib/compiler-reference.ts": [
    "normalizeText",
    "normalizeKey",
    "canonicalNumber",
    "canonicalize",
  ],
  "app/lib/project-io.ts": [
    "projectFileExtensions",
    "isProjectState",
    "parseProjectText",
    "projectFileStem",
  ],
  "app/lib/project/state.ts": [
    "cloneProject",
    "CURRENT_SCHEMA_VERSION",
    "makeId",
    "migrateProject",
  ],
  "app/lib/project/scenario.ts": ["resolveEdges", "resolvedNodeIds"],
  "app/lib/project/queries.ts": ["evidenceBacklinks"],
  "app/lib/project/export.ts": ["exportJsonCanvas", "exportMarkdown", "exportCsv"],
  "app/lib/graph/traversal.ts": ["traverseGraph"],
  "app/lib/graph/cycles.ts": ["detectCycles"],
  "app/lib/graph/paths.ts": ["shortestPath", "allShortestPaths"],
  "app/lib/graph/reachability.ts": ["compareScenarioReachability"],
  "app/lib/graph/canvas-diff.ts": [
    "emptyDiffResult",
    "computeCanvasDiff",
    "canonicalizeDiffValue",
    "computeLocalDiff",
    "buildDiffOverlay",
  ],
  "app/lib/layout/compute.ts": ["computeLayout"],
  "app/lib/research-types.ts": ["NODE_TYPES", "EDGE_TYPES", "LAYOUT_MODES"],
  "app/platform/agent-client.ts": [
    "startPdfJob",
    "getPdfJobStatus",
    "reviewPdfPatch",
    "cancelPdfJob",
    "compileProject",
    "pickPdfFile",
    "listenForPdfDrops",
    "patchOperationsOf",
  ],
  "app/platform/native-project.ts": [
    "saveProjectNative",
    "importProjectNative",
    "importProjectAtPath",
    "exportProjectWithPlugin",
    "openFolderWorkspace",
    "openGitWorkspace",
    "initializeGitWorkspace",
    "readGitHubAccount",
    "loginGitHubAccount",
    "generateGitHubSshKey",
    "uploadGitHubSshKey",
    "gitAutosaveProject",
  ],
  "app/platform/trackpad.ts": ["listenForNativeTrackpadFrames"],
  "app/plugins/contracts.ts": [
    "MYC_API_VERSION",
    "PLUGIN_CALL_API_VERSION",
    "pluginReference",
    "isMycFileName",
    "normalizeInstalledTheme",
    "normalizeInstalledEdgeStyle",
  ],
  "app/plugins/agent-contracts.ts": [
    "AGENT_CAPABILITIES",
    "AGENT_PERMISSIONS",
    "AGENT_PLUGIN_CATEGORY",
    "PDF_CANVAS_AGENT_MANIFEST",
    "isJobTerminal",
    "isJobAwaitingReview",
  ],
  "app/plugins/provider-contracts.ts": ["PROVIDER_CAPABILITIES"],
  "app/plugins/catalog.ts": [
    "builtInPluginCatalog",
    "builtInThemeCatalog",
    "builtInEdgeStyleCatalog",
  ],
  "app/plugins/context-menu.ts": [
    "enabledPluginsStorageKey",
    "pluginsChangedEvent",
    "contextMenuContributionsFromPlugins",
    "readEnabledPluginKeys",
  ],
  "app/plugins/identity.ts": [
    "pluginKey",
    "comparePluginVersions",
    "pluginCompatibility",
    "latestCompatiblePlugins",
    "activePlugins",
    "enableLatestPluginKeys",
    "migrateEnabledPluginKeys",
    "updateEnabledPluginKeys",
  ],
  "app/plugins/workspace.ts": [
    "workspaceCommandsFromPlugins",
    "localeBundlesFromPlugins",
    "projectExportFileName",
    "projectToSvg",
    "renderProjectExport",
    "normalizePluginGraphPatch",
  ],
  "app/plugins/tauri-client.ts": [
    "listInstalledMycPlugins",
    "installMycPlugin",
    "uninstallMycPlugin",
    "runAnalysisPlugin",
    "listenForMycDrops",
    "pickMycFiles",
  ],
  "app/plugins/theme.ts": ["resolveTheme", "sanitizeCssColor", "themeCssVariables"],
  "app/plugins/edge-style.ts": ["resolveEdgeStyle"],
  "src/vue/runtime/plugin-host.ts": ["providePluginHost", "usePluginHost"],
});

export const REQUIRED_TAURI_COMMANDS = Object.freeze({
  "app/platform/native-project.ts": [
    "save_project_file",
    "import_project_file",
    "save_plugin_artifact",
    "scan_project_folder",
    "initialize_git_workspace",
    "login_github_account",
    "generate_github_ssh_key",
    "upload_github_ssh_key",
    "git_autosave_project",
  ],
  "app/platform/agent-client.ts": [
    "start_pdf_job",
    "get_job_status",
    "review_patch",
    "cancel_job",
    "compile_project",
  ],
  "app/plugins/tauri-client.ts": [
    "install_myc_plugin",
    "uninstall_myc_plugin",
    "execute_myc_plugin",
  ],
});

export const REQUIRED_STRING_CONSTANTS = Object.freeze([
  {
    file: "app/lib/project/state.ts",
    label: "CURRENT_SCHEMA_VERSION",
    pattern: /export\s+const\s+CURRENT_SCHEMA_VERSION\s*=\s*1\b/,
  },
  {
    file: "app/lib/project-io.ts",
    label: "projectFileExtensions",
    pattern: /export\s+const\s+projectFileExtensions\s*=\s*\[\s*["']mycproj["']\s*,\s*["']json["']\s*\]/,
  },
  {
    file: "app/plugins/contracts.ts",
    label: "MYC_API_VERSION",
    pattern: /export\s+const\s+MYC_API_VERSION\s*=\s*["']researchcanvas\.dev\/v1alpha1["']/,
  },
  {
    file: "app/plugins/contracts.ts",
    label: "PLUGIN_CALL_API_VERSION",
    pattern: /export\s+const\s+PLUGIN_CALL_API_VERSION\s*=\s*["']researchcanvas\.dev\/plugin-call\/v1alpha1["']/,
  },
  {
    file: "app/plugins/contracts.ts",
    label: "GraphPatch apiVersion",
    pattern: /apiVersion\s*:\s*["']researchcanvas\.dev\/graph-patch\/v1alpha1["']/,
  },
  {
    file: "app/plugins/agent-contracts.ts",
    label: "AGENT_PLUGIN_CATEGORY",
    pattern: /export\s+const\s+AGENT_PLUGIN_CATEGORY\s*=\s*["']agent["']/,
  },
  {
    file: "app/plugins/context-menu.ts",
    label: "enabledPluginsStorageKey",
    pattern: /export\s+const\s+enabledPluginsStorageKey\s*=\s*["']research-canvas\.enabled-plugins\.v1["']/,
  },
  {
    file: "app/plugins/context-menu.ts",
    label: "pluginsChangedEvent",
    pattern: /export\s+const\s+pluginsChangedEvent\s*=\s*["']research-canvas\.plugins-changed["']/,
  },
  {
    file: "app/features/research-workspace/hooks/sync-logic.ts",
    label: "PROJECT_STORAGE_KEY",
    pattern: /export\s+const\s+PROJECT_STORAGE_KEY\s*=\s*["']research-canvas\.zen-workspace\.v1["']/,
  },
  {
    file: "src/vue/runtime/i18n.ts",
    label: "localeStorageKey",
    pattern: /const\s+localeStorageKey\s*=\s*["']research-canvas\.locale\.v1["']/,
  },
  {
    file: "src/vue/ResearchWorkspaceApp.vue",
    label: "preferencesStorageKey",
    pattern: /const\s+preferencesStorageKey\s*=\s*["']research-canvas\.workspace-preferences\.v2["']/,
  },
]);

export const REQUIRED_ARRAY_CONSTANTS = Object.freeze([
  {
    file: "app/lib/research-types.ts",
    name: "NODE_TYPES",
    values: [
      "question",
      "concept",
      "variable",
      "hypothesis",
      "method",
      "evidence",
      "paper",
      "dataset",
      "experiment",
      "result",
      "metric",
      "formula",
      "artifact",
      "note",
    ],
  },
  {
    file: "app/lib/research-types.ts",
    name: "EDGE_TYPES",
    values: [
      "causes",
      "correlates",
      "supports",
      "contradicts",
      "depends_on",
      "derived_from",
      "part_of",
      "controls",
      "mediates",
      "moderates",
      "uses",
      "measures",
    ],
  },
  {
    file: "app/lib/research-types.ts",
    name: "LAYOUT_MODES",
    values: [
      "evidence-chain",
      "refutation-chain",
      "tree",
      "huffman",
      "table",
      "neural-network",
    ],
  },
  {
    file: "app/plugins/agent-contracts.ts",
    name: "AGENT_CAPABILITIES",
    values: ["agent.pdf.read", "agent.graph.patch.propose", "agent.review.request"],
  },
  {
    file: "app/plugins/provider-contracts.ts",
    name: "PROVIDER_CAPABILITIES",
    values: ["llm.chat", "llm.configure"],
  },
]);

/**
 * Pinia is part of the Vue renderer boundary. Keep this inventory explicit so
 * a future renderer cannot silently fall back to local composable state while
 * retaining the old public facade names.
 */
export const REQUIRED_PINIA_STORES = Object.freeze([
  {
    file: "src/vue/stores/project.ts",
    name: "useProjectStore",
    id: "project",
  },
  {
    file: "src/vue/stores/canvas-interaction.ts",
    name: "useCanvasInteractionStore",
    id: "canvas-interaction",
  },
  {
    file: "src/vue/stores/workspace-ui.ts",
    name: "useWorkspaceUiStore",
    id: "workspace-ui",
  },
  {
    file: "src/vue/stores/runtime-plugin-host.ts",
    name: "useRuntimePluginHostStore",
    id: "runtime-plugin-host",
  },
  {
    file: "src/vue/stores/runtime-i18n.ts",
    name: "useRuntimeI18nStore",
    id: "runtime-i18n",
  },
  {
    file: "src/vue/stores/runtime-auth.ts",
    name: "useRuntimeAuthStore",
    id: "runtime-auth",
  },
]);

export const REQUIRED_PINIA_FACADES = Object.freeze([
  {
    file: "src/vue/composables/use-workspace-project.ts",
    exportName: "useWorkspaceProject",
    storeName: "useProjectStore",
    preserveRefs: [
      "project",
      "projectRef",
      "history",
      "selectedNode",
      "selectedNodeId",
      "selectedEdge",
      "selectedEdgeId",
      "canUndo",
      "canRedo",
    ],
    requireStoreToRefs: true,
  },
]);

export const REQUIRED_PINIA_CONSUMERS = Object.freeze([
  {
    file: "src/vue/canvas/ResearchGraphCanvas.vue",
    storeName: "useCanvasInteractionStore",
    requireStoreToRefs: true,
  },
  {
    file: "src/vue/ResearchWorkspaceApp.vue",
    storeName: "useWorkspaceUiStore",
    requireStoreToRefs: true,
  },
  {
    file: "src/vue/runtime/plugin-host.ts",
    storeName: "useRuntimePluginHostStore",
    requireStoreToRefs: false,
  },
  {
    file: "src/vue/runtime/i18n.ts",
    storeName: "useRuntimeI18nStore",
    requireStoreToRefs: false,
  },
  {
    file: "src/vue/runtime/auth.ts",
    storeName: "useRuntimeAuthStore",
    requireStoreToRefs: false,
  },
]);

function readVueSource(root, relativePath, violations) {
  const absolutePath = resolve(root, relativePath);
  if (!existsSync(absolutePath)) {
    violations.push(`Missing Vue architecture file: ${relativePath}`);
    return null;
  }
  return readFileSync(absolutePath, "utf8");
}

function checkPiniaArchitecture(root, violations) {
  const rootSource = readVueSource(root, "src/vue/stores/pinia.ts", violations);
  const entrySource = readVueSource(root, "src/main.ts", violations);
  if (rootSource !== null) {
    if (!/from\s+["']pinia["']/.test(rootSource) || !/\bcreatePinia\s*\(\s*\)/.test(rootSource)) {
      violations.push("src/vue/stores/pinia.ts must create the single Pinia root");
    }
    if (!/export\s+const\s+pinia\b/.test(rootSource)) {
      violations.push("src/vue/stores/pinia.ts must export the application Pinia root");
    }
  }
  if (entrySource !== null) {
    if (!/import\s*\{[^}]*\bpinia\b[^}]*\}\s*from\s*["']\.\/vue\/stores\/pinia["']/.test(entrySource)) {
      violations.push("src/main.ts must import the shared Pinia root");
    }
    if (!/\.use\(\s*pinia\s*\)/.test(entrySource)) {
      violations.push("src/main.ts must register Pinia with the Vue app");
    }
  }

  for (const store of REQUIRED_PINIA_STORES) {
    const source = readVueSource(root, store.file, violations);
    if (source === null) continue;
    if (!/from\s+["']pinia["']/.test(source)) {
      violations.push(`${store.file} must import Pinia`);
    }
    if (!new RegExp(`export\\s+const\\s+${escapeRegExp(store.name)}\\s*=\\s*defineStore\\s*\\(\\s*["']${escapeRegExp(store.id)}["']\\s*,\\s*\\(\\s*\\)\\s*=>`).test(source)) {
      violations.push(`${store.file} must export setup-style store ${store.name} with id ${store.id}`);
    }
    if (/defineStore\s*\([^,]+,\s*\{/.test(source)) {
      violations.push(`${store.file} must not use an option-style Pinia store`);
    }
  }

  for (const facade of REQUIRED_PINIA_FACADES) {
    const source = readVueSource(root, facade.file, violations);
    if (source === null) continue;
    if (!new RegExp(`export\\s+function\\s+${escapeRegExp(facade.exportName)}\\b`).test(source)) {
      violations.push(`${facade.file} must preserve facade export ${facade.exportName}`);
    }
    if (!new RegExp(`\\b${escapeRegExp(facade.storeName)}\\s*\\(`).test(source)) {
      violations.push(`${facade.file} must delegate to ${facade.storeName}`);
    }
    if (facade.requireStoreToRefs && !/\bstoreToRefs\s*\(/.test(source)) {
      violations.push(`${facade.file} must use storeToRefs to preserve the ref-based facade`);
    }
    for (const field of facade.preserveRefs) {
      if (!new RegExp(`\\b${escapeRegExp(field)}\\s*:`).test(source)) {
        violations.push(`${facade.file} no longer exposes facade field ${field}`);
      }
    }
  }

  for (const consumer of REQUIRED_PINIA_CONSUMERS) {
    const source = readVueSource(root, consumer.file, violations);
    if (source === null) continue;
    if (!new RegExp(`\\b${escapeRegExp(consumer.storeName)}\\s*\\(`).test(source)) {
      violations.push(`${consumer.file} must consume ${consumer.storeName}`);
    }
    if (consumer.requireStoreToRefs && !/\bstoreToRefs\s*\(/.test(source)) {
      violations.push(`${consumer.file} must use storeToRefs at its Pinia consumption boundary`);
    }
  }
}

function checkVueLayerBoundaries(root, violations) {
  const vueRoot = resolve(root, "src/vue");
  if (!existsSync(vueRoot)) {
    violations.push("Missing Vue renderer root: src/vue/");
    return;
  }
  const vueFiles = [];
  const pending = [vueRoot];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const absolutePath = resolve(current, entry.name);
      if (entry.isDirectory()) pending.push(absolutePath);
      else if (/\.(?:vue|ts)$/.test(entry.name)) vueFiles.push(absolutePath);
    }
  }

  const forbiddenVuex = /(?:from\s*["']vuex["']|require\(\s*["']vuex["']\s*\)|\bcreateStore\s*\(|\buseStore\s*\()/;
  const forbiddenDirectRust = /(?:@tauri-apps\/api\/core|\binvoke\s*(?:<[^>]*>)?\s*\()/;
  const duplicateSchema = [
    /\b(?:interface|type)\s+(?:ProjectState|GraphPatch|PluginGraphPatch|GraphPatchOperation)\b/,
    /\b(?:const|let|var)\s+(?:CURRENT_SCHEMA_VERSION|NODE_TYPES|EDGE_TYPES|LAYOUT_MODES)\s*=/,
    /\bschemaVersion\s*:\s*\d+\b/,
    /\bapiVersion\s*:\s*["']researchcanvas\.dev\/graph-patch\//,
  ];

  for (const file of vueFiles) {
    const source = readFileSync(file, "utf8");
    const relativePath = file.slice(root.length + 1).replaceAll("\\", "/");
    if (forbiddenVuex.test(source)) {
      violations.push(`${relativePath}: Vue layer must use Pinia, not Vuex or a Vuex-style store API`);
    }
    if (forbiddenDirectRust.test(source)) {
      violations.push(`${relativePath}: Vue layer must call the platform facade, not Rust invoke directly`);
    }
    if (duplicateSchema.some((pattern) => pattern.test(source))) {
      violations.push(`${relativePath}: Vue layer duplicates a protected project/GraphPatch schema`);
    }
  }
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function readContractSource(root, relativePath, violations) {
  const absolutePath = resolve(root, relativePath);
  if (!existsSync(absolutePath)) {
    violations.push(`Missing protected source file: ${relativePath}`);
    return null;
  }
  return readFileSync(absolutePath, "utf8");
}

function hasNamedExport(source, name) {
  const escapedName = escapeRegExp(name);
  return [
    new RegExp(`\\bexport\\s+(?:async\\s+)?function\\s+${escapedName}\\b`),
    new RegExp(`\\bexport\\s+(?:const|let|var)\\s+${escapedName}\\b`),
    new RegExp(`\\bexport\\s+(?:type|interface|class)\\s+${escapedName}\\b`),
    new RegExp(`\\bexport\\s*\\{[^}]*\\b${escapedName}\\b[^}]*\\}`, "s"),
  ].some((pattern) => pattern.test(source));
}

function arrayConstantValues(source, name) {
  const declaration = source.match(
    new RegExp(`export\\s+const\\s+${escapeRegExp(name)}\\s*=\\s*\\[([\\s\\S]*?)\\]`),
  );
  if (!declaration) return null;
  return [...declaration[1].matchAll(/["']([^"']+)["']/g)].map((match) => match[1]);
}

function checkExports(root, violations) {
  for (const [relativePath, names] of Object.entries(REQUIRED_EXPORTS)) {
    const source = readContractSource(root, relativePath, violations);
    if (source === null) continue;
    for (const name of names) {
      if (!hasNamedExport(source, name)) {
        violations.push(`Missing export ${name} from ${relativePath}`);
      }
    }
  }
}

function checkTauriCommands(root, violations) {
  for (const [relativePath, commands] of Object.entries(REQUIRED_TAURI_COMMANDS)) {
    const source = readContractSource(root, relativePath, violations);
    if (source === null) continue;
    for (const command of commands) {
      const callPattern = new RegExp(
        `\\binvoke(?:<[^>]*>)?\\s*\\(\\s*["']${escapeRegExp(command)}["']`,
      );
      if (!callPattern.test(source)) {
        violations.push(`Missing Tauri command "${command}" in ${relativePath}`);
      }
    }
  }
}

function checkStringConstants(root, violations) {
  for (const contract of REQUIRED_STRING_CONSTANTS) {
    const source = readContractSource(root, contract.file, violations);
    if (source !== null && !contract.pattern.test(source)) {
      violations.push(`Changed or missing ${contract.label} in ${contract.file}`);
    }
  }
}

function checkArrayConstants(root, violations) {
  for (const contract of REQUIRED_ARRAY_CONSTANTS) {
    const source = readContractSource(root, contract.file, violations);
    if (source === null) continue;
    const actual = arrayConstantValues(source, contract.name);
    if (!actual) {
      violations.push(`Missing array constant ${contract.name} in ${contract.file}`);
      continue;
    }
    if (JSON.stringify(actual) !== JSON.stringify(contract.values)) {
      violations.push(
        `Changed ${contract.name} in ${contract.file}: expected ${JSON.stringify(contract.values)}, got ${JSON.stringify(actual)}`,
      );
    }
  }
}

export function collectContractViolations(root = REPOSITORY_ROOT) {
  const violations = [];
  checkExports(root, violations);
  checkTauriCommands(root, violations);
  checkStringConstants(root, violations);
  checkArrayConstants(root, violations);
  checkPiniaArchitecture(root, violations);
  checkVueLayerBoundaries(root, violations);
  return violations;
}

export function formatContractReport(violations) {
  if (violations.length === 0) return "Vue interface contract: PASS (all frozen interfaces are present).";
  return [
    `Vue interface contract: FAIL (${violations.length} violation${violations.length === 1 ? "" : "s"}).`,
    ...violations.map((violation) => `- ${violation}`),
  ].join("\n");
}

const invokedAsScript =
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href;

if (invokedAsScript) {
  const violations = collectContractViolations();
  console.log(formatContractReport(violations));
  process.exitCode = violations.length === 0 ? 0 : 1;
}
