import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const read = (file: string) => readFileSync(file, "utf8");
const loading = JSON.parse(read("config/plugin-loading.json")) as {
  apiVersion: string;
  sourceRoots: { official: string; thirdParty: string };
  runtimeRoots: { dev: string; release: string };
  desktopDev: {
    defaultPolicy: string;
    packageFiles: string[];
    allowedPluginIds: string[];
    developmentPluginSources: Record<string, string>;
    officialSourceRoots?: Record<string, string>;
    enabledDevelopmentPluginIds: string[];
    freshBuilds: Array<{ pluginId: string; source: string }>;
  };
  developmentPlugins: Array<{ pluginId: string; source: string }>;
  release: {
    freshBuilds: Array<{ pluginId: string; source: string }>;
    packageFiles: string[];
  };
};

const officialBundledIds = [
  "myc.circuit-orthogonal",
  "myc.export-suite",
  "myc.folder-workspaces",
  "myc.git-workspace",
];

const developmentSources = {
  "myc.pdf-canvas-agent": "my-plugins/anPdfsolver",
  "anyway.system.ancordis": "my-plugins/ancordis",
  "anyway.anmarket": "my-plugins/anmarket",
} as const;

test("source roots are separate from generated runtime roots", () => {
  assert.equal(loading.sourceRoots.official, "my-plugins");
  assert.equal(loading.sourceRoots.thirdParty, "my-third-plugins");
  assert.equal(loading.runtimeRoots.dev, ".plugin-runtime/dev");
  assert.equal(loading.runtimeRoots.release, ".plugin-runtime/release-staging");

  assert.ok(existsSync("my-plugins/anPdfsolver/plugin.json"));
  assert.ok(existsSync("my-plugins/ancordis/manifest.json"));
  assert.ok(existsSync("my-plugins/anmarket/plugin.json"));

  assert.notEqual(loading.sourceRoots.official, loading.runtimeRoots.dev);
  assert.notEqual("my-plugins/anPdfsolver", loading.runtimeRoots.dev);
  assert.notEqual("my-plugins/anPdfsolver", loading.runtimeRoots.release);
  assert.doesNotMatch(loading.runtimeRoots.dev, /plugin-runtime\/desktop-dev/u);
});

test("desktop dev defaults to official bundled packages only", () => {
  assert.equal(loading.desktopDev.defaultPolicy, "official-bundled-only");
  assert.deepEqual(loading.desktopDev.allowedPluginIds, officialBundledIds);
  assert.deepEqual(loading.desktopDev.enabledDevelopmentPluginIds, []);
  assert.deepEqual(loading.desktopDev.freshBuilds, []);
  assert.equal(new Set(loading.desktopDev.allowedPluginIds).size, loading.desktopDev.allowedPluginIds.length);

  for (const id of [
    "myc.pdf-canvas-agent",
    "anyway.system.ancordis",
    "anyway.anmarket",
    "myc.i18n-ja",
    "myc.onedarkpro",
    "myc.runtime-smoke",
  ]) {
    assert.ok(!loading.desktopDev.allowedPluginIds.includes(id), `${id} must not be auto-enabled in desktop dev`);
  }

  assert.deepEqual(
    loading.desktopDev.packageFiles.map((file) => file.split("/").at(-1)),
    [
      "myc.circuit-orthogonal@1.1.0.myc",
      "myc.export-suite@1.0.0.myc",
      "myc.folder-workspaces@1.0.0.myc",
      "myc.git-workspace@1.2.0.myc",
    ],
  );
  assert.ok(!loading.desktopDev.packageFiles.some((file) => /pdf-canvas-agent|i18n-ja|onedarkpro/u.test(file)));
});

test("development plugin sources are explicit opt-in entries", () => {
  assert.deepEqual(loading.desktopDev.developmentPluginSources, developmentSources);
  assert.deepEqual(loading.desktopDev.officialSourceRoots, developmentSources);
  assert.deepEqual(loading.developmentPlugins, Object.entries(developmentSources).map(([pluginId, source]) => ({
    pluginId,
    source,
  })));

  const stagingScript = read("scripts/stage-plugin-runtime.mjs");
  assert.match(stagingScript, /--with-dev-plugin/u);
  assert.match(stagingScript, /\.plugin-runtime\/\$\{mode === "dev" \? "dev" : "release-staging"\}/u);
  assert.doesNotMatch(stagingScript, /plugin-runtime\/desktop-dev/u);
});

test("default staging removes packages left by a previous opt-in run", () => {
  const packagesRoot = resolve(".plugin-runtime/dev/packages");
  const stalePackage = resolve(packagesRoot, "myc.stale-opt-in@0.0.0.myc");
  const stagingScript = resolve("scripts/stage-plugin-runtime.mjs");
  const runDefaultStage = () => spawnSync(process.execPath, [stagingScript, "dev"], {
    cwd: process.cwd(),
    encoding: "utf8",
    timeout: 30_000,
  });

  const initial = runDefaultStage();
  assert.equal(initial.status, 0, initial.stderr || initial.stdout);
  try {
    writeFileSync(stalePackage, "stale opt-in package must be removed", "utf8");
    const staged = runDefaultStage();
    assert.equal(staged.status, 0, staged.stderr || staged.stdout);
    assert.deepEqual(readdirSync(packagesRoot).sort(), [
      "myc.circuit-orthogonal@1.1.0.myc",
      "myc.export-suite@1.0.0.myc",
      "myc.folder-workspaces@1.0.0.myc",
      "myc.git-workspace@1.2.0.myc",
    ]);
    assert.equal(existsSync(stalePackage), false);
  } finally {
    const restored = runDefaultStage();
    assert.equal(restored.status, 0, restored.stderr || restored.stdout);
  }
});

test("third-party packages are ignored and never automatic discovery roots", () => {
  assert.ok(existsSync("my-third-plugins/myc.i18n-ja@1.0.1.myc"));
  assert.ok(existsSync("my-third-plugins/myc.onedarkpro@1.3.0.myc"));
  assert.match(read(".gitignore"), /^\/my-third-plugins\/$/mu);

  const allConfiguredPackages = [
    ...loading.desktopDev.packageFiles,
    ...loading.release.packageFiles,
  ];
  assert.ok(!allConfiguredPackages.some((file) => /i18n-ja|onedarkpro/u.test(file)));
  assert.doesNotMatch(read("scripts/stage-plugin-runtime.mjs"), /my-third-plugins/u);
});

test("release resources come from explicit release-staging entries", () => {
  assert.deepEqual(loading.release.freshBuilds, [
    { pluginId: "myc.pdf-canvas-agent", source: "my-plugins/anPdfsolver" },
  ]);
  assert.ok(loading.release.packageFiles.includes("plugins/packages/myc.pdf-canvas-agent@0.5.0.myc"));
  assert.ok(!loading.release.packageFiles.some((file) => file.includes("@0.4.0")));

  const tauri = JSON.parse(read("src-tauri/tauri.conf.json")) as {
    bundle: { resources: Record<string, string> };
  };
  const resourceSources = Object.keys(tauri.bundle.resources);
  assert.equal(resourceSources.some((source) => source === "../plugins/packages/*.myc"), false);
  assert.ok(resourceSources.every((source) => source.startsWith("../.plugin-runtime/release-staging/packages/")));
  assert.deepEqual(
    resourceSources.map((source) => source.split("/").at(-1)).sort(),
    loading.release.packageFiles.map((file) => file.split("/").at(-1)).sort(),
  );
});

test("anPdfsolver manifest uses trusted frontend and direct-network worker contract", () => {
  const manifest = JSON.parse(read("my-plugins/anPdfsolver/plugin.json")) as {
    name: string;
    version: string;
    frontend: { mode: string; entry: string; framework: string; apiVersion: string };
    contributes: {
      ui?: Array<{ id: string; slotId: string; export: string; order?: number; when?: string }>;
      uiIr?: unknown;
    };
    workers: Array<{
      id: string;
      language: string;
      entrypoint: string;
      transport: string;
      operations: string[];
      hostOperations: string[];
      providerEgress?: Array<{ providerId: string; connectionId: string; domains: string[]; secretEnv: string }>;
    }>;
    network: { mode: string; declaredDomains: string[] };
    capabilities: string[];
    engines?: { worker?: { entrypoint?: string; transport?: string; operations?: string[]; hostOperations?: string[] } };
  };
  assert.equal(manifest.name, "myc.pdf-canvas-agent");
  assert.equal(manifest.version, "0.5.0");
  assert.deepEqual(manifest.frontend, {
    mode: "trusted-module",
    entry: "dist/frontend.mjs",
    framework: "vue3",
    apiVersion: "1",
  });
  assert.deepEqual(manifest.contributes.ui, [
    {
      id: "anpdfsolver.toolbar",
      slotId: "workspace.toolbar.actions",
      export: "AnPdfsolverToolbarButton",
      order: 40,
      when: "workspace.active",
    },
    {
      id: "anpdfsolver.dialog",
      slotId: "workspace.dialogs",
      export: "AnPdfsolverDialog",
      order: 40,
      when: "workspace.active",
    },
  ]);
  assert.equal("uiIr" in manifest.contributes, false);

  assert.deepEqual(manifest.workers, [{
    id: "python-analyzer",
    language: "python",
    entrypoint: "src/anpdfsolver/worker.py",
    transport: "stdio-framed-json-v1",
    operations: ["ping", "health", "anpdfsolver.analyze"],
    hostOperations: ["blob.read"],
    providerEgress: [{
      providerId: "kimi",
      connectionId: "kimi",
      domains: ["api.moonshot.cn", "api.moonshot.ai"],
      purpose: "PDF upload and typed NDJSON response streaming",
      secretEnv: "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY",
    }],
  }]);
  assert.deepEqual(manifest.network, {
    mode: "direct",
    declaredDomains: ["api.moonshot.cn", "api.moonshot.ai"],
  });

  const capabilities = new Set(manifest.capabilities);
  for (const capability of [
    "plugin.files.pick",
    "plugin.worker.open",
    "plugin.worker.call",
    "plugin.worker.cancel",
    "plugin.worker.close",
    "graph.patch.propose",
    "graph.patch.get",
    "graph.patch.review",
  ]) {
    assert.ok(capabilities.has(capability), `${capability} must be declared`);
  }
  assert.ok(!capabilities.has("graph.storage.write"));
  assert.ok(!manifest.workers[0].hostOperations.includes("graph.storage.put"));

  assert.equal(manifest.engines?.worker?.entrypoint, "src/anpdfsolver/worker.py");
  assert.equal(manifest.engines?.worker?.transport, "stdio-framed-json-v1");
  assert.deepEqual(manifest.engines?.worker?.operations, manifest.workers[0].operations);
  assert.deepEqual(manifest.engines?.worker?.hostOperations, manifest.workers[0].hostOperations);
});

test("agent manifest aligns with plugin-owned UI and Rust canonical graph boundary", () => {
  const agent = JSON.parse(read("my-plugins/anPdfsolver/agent-manifest.json")) as {
    pluginVersion: string;
    frontend: {
      entry: string;
      uiContributions: Array<{ id: string; slotId: string; export: string }>;
    };
    worker: {
      id: string;
      language: string;
      entrypoint: string;
      transport: string;
      openedBy: string;
      operations: string[];
      hostOperations: string[];
      forbiddenHostOperations: string[];
    };
    pipeline: { transport: string; directStorageWrite: boolean; stages: string[] };
    securityBoundary: { directProviderEgress: boolean; noGraphStoreWrite: boolean; pythonSdkRole: string };
  };

  assert.equal(agent.pluginVersion, "0.5.0");
  assert.equal(agent.frontend.entry, "dist/frontend.mjs");
  assert.deepEqual(agent.frontend.uiContributions.map((entry) => entry.slotId), [
    "workspace.toolbar.actions",
    "workspace.dialogs",
  ]);
  assert.deepEqual(agent.worker, {
    id: "python-analyzer",
    language: "python",
    entrypoint: "src/anpdfsolver/worker.py",
    transport: "stdio-framed-json-v1",
    openedBy: "plugin.frontend",
    operations: ["ping", "health", "anpdfsolver.analyze"],
    hostOperations: ["blob.read"],
    forbiddenHostOperations: ["graph.storage.put", "graph.patch.propose", "graph.patch.review"],
    credentials: "host-secret-injected-to-exact-worker",
  });
  assert.equal(agent.pipeline.transport, "plugin-direct-provider");
  assert.equal(agent.pipeline.directStorageWrite, false);
  assert.ok(agent.pipeline.stages.includes("graphPatch.propose"));
  assert.ok(agent.pipeline.stages.includes("graphPatch.review"));
  assert.equal(agent.securityBoundary.directProviderEgress, true);
  assert.equal(agent.securityBoundary.noGraphStoreWrite, true);
  assert.equal(agent.securityBoundary.pythonSdkRole, "framed-rpc-codec-only");
});
