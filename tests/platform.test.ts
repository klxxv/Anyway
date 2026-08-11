import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { createPinia, setActivePinia } from "pinia";
import { computed, nextTick } from "vue";
import {
  localeCatalog,
  normalizeLocale,
  translate,
} from "../app/i18n/catalog";
import { builtInPluginCatalog } from "../app/plugins/catalog";
import {
  MYC_API_VERSION,
  isMycFileName,
  normalizeInstalledEdgeStyle,
  normalizeInstalledTheme,
  type PluginContextMenuContribution,
  type InstalledMycPlugin,
} from "../app/plugins/contracts";
import { contextMenuContributionsFromPlugins } from "../app/plugins/context-menu";
import { resolveEdgeStyle } from "../app/plugins/edge-style";
import {
  activePlugins,
  enableLatestPluginKeys,
  migrateEnabledPluginKeys,
  pluginCompatibility,
  pluginKey,
  supersededCompatiblePlugins,
  updateEnabledPluginKeys,
} from "../app/plugins/identity";
import { resolveTheme, sanitizeCssColor, themeCssVariables } from "../app/plugins/theme";
import { isProjectState } from "../app/lib/project-io";
import type { ProjectState } from "../app/lib/research-types";
import {
  localeBundlesFromPlugins,
  normalizePluginGraphPatch,
  projectToSvg,
  workspaceCommandsFromPlugins,
} from "../app/plugins/workspace";
import { createPluginHostValue } from "../src/vue/runtime/plugin-host";
import { useRuntimePluginHostStore } from "../src/vue/stores/runtime-plugin-host";

test("locale normalization and Simplified Chinese catalog are deterministic", () => {
  assert.equal(normalizeLocale("zh-CN"), "zh-CN");
  assert.equal(normalizeLocale("zh-Hans"), "zh-CN");
  assert.equal(normalizeLocale("en-US"), "en");
  assert.equal(translate("zh-CN", "toolbar.filter"), "筛选");
  assert.equal(translate("en", "toolbar.filter"), "Filter");
  assert.equal(translate("zh-CN", "workspace.pluginStore"), "插件商店");
  assert.equal(translate("zh-CN", "layout.neural"), "神经网络");
  assert.equal(
    translate("zh-CN", "toast.linksFiltered", {}, { relation: "控制" }),
    "已筛选“控制”关系并重新布局。",
  );
  for (const [locale, messages] of Object.entries(localeCatalog)) {
    for (const [key, value] of Object.entries(messages)) {
      assert.ok(value.trim(), `${locale}:${key} must not be empty`);
    }
  }
});

test("live plugin host capability gates update when an installed AgentPlugin is enabled", async () => {
  setActivePinia(createPinia());
  const store = useRuntimePluginHostStore();
  const agent: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.pdf-agent@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "AgentPlugin",
      metadata: {
        id: "myc.pdf-agent",
        name: "PDF Agent",
        version: "1.0.0",
        publisher: "Research Canvas",
        developer: "Research Canvas",
        description: "Review-gated PDF graph agent",
      },
      spec: {
        engine: "host-mediated",
        entry: "agent.json",
        capabilities: ["agent.graph.patch.propose"],
        permissions: [],
      },
    },
    agent: {
      schemaVersion: 1,
      mode: "agent",
      reviewGated: true,
    },
  };

  store.installedPlugins = [agent];
  const pluginHost = createPluginHostValue(store);
  const hasPdfAgent = computed(() =>
    pluginHost.activePlugins.some(
      (plugin) =>
        plugin.manifest.kind === "AgentPlugin" &&
        plugin.manifest.spec.capabilities.includes("agent.graph.patch.propose"),
    ),
  );

  assert.equal(hasPdfAgent.value, false);
  store.setPluginEnabled(agent, true);
  await nextTick();
  assert.equal(hasPdfAgent.value, true);
  assert.deepEqual([...pluginHost.activePluginKeys], ["myc.pdf-agent@1.0.0"]);

  store.setPluginEnabled(agent, false);
  await nextTick();
  assert.equal(hasPdfAgent.value, false);
});

test("installed EdgeStylePlugin metadata owns the registered style identity", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.circuit-orthogonal@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "EdgeStylePlugin",
      metadata: {
        id: "myc.circuit-orthogonal",
        name: "Circuit Orthogonal",
        version: "1.0.0",
        publisher: "Research Canvas Community",
        developer: "Visual Systems",
        description: "Strict 90-degree semantic connectors",
      },
      spec: {
        engine: "declarative",
        entry: "edge-style.json",
        capabilities: ["edge.style.register"],
        permissions: [],
      },
    },
    edgeStyle: {
      id: "untrusted-id",
      name: "Untrusted name",
      publisher: "Untrusted publisher",
      routing: "orthogonal",
      stroke: {
        color: "#61afef",
        width: 1.8,
        selectedWidth: 3,
        opacity: 0.94,
        cornerRadius: 0,
      },
      relations: {
        supports: { color: "#56b6c2" },
        contradicts: { color: "#e06c75", dash: [8, 4] },
      },
      marker: { type: "closed-arrow", size: 16 },
    },
  };

  const edgeStyle = normalizeInstalledEdgeStyle(plugin);
  assert.equal(edgeStyle?.id, "myc.circuit-orthogonal");
  assert.equal(edgeStyle?.name, "Circuit Orthogonal");
  assert.equal(edgeStyle?.version, "1.0.0");
  assert.equal(edgeStyle?.routing, "orthogonal");
  assert.equal(edgeStyle?.source, "myc");
  assert.equal(pluginCompatibility(plugin).compatible, true);
  assert.equal(
    resolveEdgeStyle([plugin]).id,
    "myc.circuit-orthogonal",
  );
  assert.equal(resolveEdgeStyle([]).id, "research-orthogonal");
  assert.equal(resolveEdgeStyle([]).stroke.cornerRadius, 12);
});

test(".myc filenames are recognized case-insensitively", () => {
  assert.equal(isMycFileName("myc.onedarkpro@1.0.0.myc"), true);
  assert.equal(isMycFileName("THEME.MYC"), true);
  assert.equal(isMycFileName("theme.zip"), false);
});

test("installed ThemePlugin metadata owns the registered theme identity", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.onedarkpro@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "ThemePlugin",
      metadata: {
        id: "myc.onedarkpro",
        name: "One Dark Pro",
        version: "1.0.0",
        publisher: "Research Canvas Community",
        developer: "Theme Lab",
        description: "Dark research theme",
      },
      spec: {
        engine: "declarative",
        entry: "theme.json",
        capabilities: ["theme.register"],
        permissions: [],
      },
    },
    theme: {
      id: "untrusted-id",
      name: "Untrusted name",
      publisher: "Untrusted publisher",
      colors: {
        app: "#1e222a",
        panel: "#282c34",
        canvas: "#21252b",
        text: "#abb2bf",
        muted: "#7f8797",
        accent: "#61afef",
        border: "#3e4451",
      },
      components: {
        toast: {
          background: "#20242c",
          text: "#f0f2f5",
        },
        miniMap: {
          background: "#20242c",
          relation: "#596170",
          showRelations: true,
        },
        radialMenu: {
          background: "#282c34",
          divider: "#4b5263",
          active: "#61afef",
          centerBackground: "#21252b",
        },
      },
    },
  };

  const theme = normalizeInstalledTheme(plugin);
  assert.equal(theme?.id, "myc.onedarkpro");
  assert.equal(theme?.name, "One Dark Pro");
  assert.equal(theme?.version, "1.0.0");
  assert.equal(theme?.source, "myc");
  assert.equal(pluginCompatibility(plugin).compatible, true);
  assert.equal(resolveTheme([plugin])?.id, "myc.onedarkpro");
  assert.equal(themeCssVariables(theme)?.["--color-blue"], "#61afef");
  assert.equal(themeCssVariables(theme)?.["--toast-background"], "#20242c");
  assert.equal(themeCssVariables(theme)?.["--minimap-relation"], "#596170");
  assert.equal(themeCssVariables(theme)?.["--radial-menu-background"], "#282c34");
  assert.equal(themeCssVariables(theme)?.["--radial-menu-divider"], "#4b5263");
  assert.equal(themeCssVariables(theme)?.["--radial-menu-active"], "#61afef");
  assert.equal(themeCssVariables(theme)?.["--radial-menu-center-background"], "#21252b");
  assert.equal(theme?.components?.miniMap?.showRelations, true);

  const maliciousTheme = {
    ...theme,
    colors: {
      app: "#1e222a",
      panel: "#282c34; --evil: url(https://example.com/leak)",
      canvas: "#21252b",
      text: "#abb2bf",
      muted: "#7f8797",
      accent: "#61afef",
      border: "#3e4451",
    },
  };
  assert.equal(themeCssVariables(maliciousTheme), undefined);
  assert.equal(sanitizeCssColor("red"), "red");
  assert.equal(sanitizeCssColor("  #fff  "), "#fff");
  assert.equal(sanitizeCssColor("rgb(0,0,0)"), "rgb(0,0,0)");
  assert.equal(sanitizeCssColor("rgb(0, 0, 0); background: red"), undefined);
  assert.equal(sanitizeCssColor("var(--x)"), undefined);
  assert.equal(sanitizeCssColor("url(https://x)"), undefined);

  const older: InstalledMycPlugin = {
    ...plugin,
    manifest: {
      ...plugin.manifest,
      metadata: { ...plugin.manifest.metadata, version: "0.9.0" },
    },
  };
  const allKeys = enableLatestPluginKeys([older, plugin]);
  assert.deepEqual([...allKeys], [pluginKey(plugin)]);
  assert.deepEqual(supersededCompatiblePlugins([older, plugin]), [older]);
  assert.deepEqual(activePlugins([older, plugin], new Set([pluginKey(older), pluginKey(plugin)])), [plugin]);
  assert.deepEqual(
    [...updateEnabledPluginKeys([older, plugin], new Set([pluginKey(older)]), plugin, true)],
    [pluginKey(plugin)],
  );
  assert.deepEqual(
    [...migrateEnabledPluginKeys([older, plugin], new Set([pluginKey(older)]))],
    [pluginKey(plugin)],
  );
});

test("runtime plugin metadata exposes only the verified wasm boundary", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.runtime-smoke@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "AnalysisPlugin",
      metadata: {
        id: "myc.runtime-smoke",
        name: "Runtime Smoke",
        version: "1.0.0",
        publisher: "Research Canvas",
        developer: "Runtime Team",
        description: "VM test package",
      },
      spec: {
        engine: "wasm32-myc",
        entry: "plugin.wasm",
        language: "cpp",
        capabilities: ["analysis.run"],
        permissions: [],
      },
    },
    runtime: {
      engine: "wasm32-myc",
      language: "cpp",
      entrySha256: "a".repeat(64),
    },
  };

  assert.equal(plugin.runtime?.language, "cpp");
  assert.equal(plugin.runtime?.entrySha256.length, 64);
  assert.deepEqual(plugin.manifest.spec.permissions, []);
  assert.equal(pluginCompatibility(plugin).compatible, true);
});

test("active plugin context menus require runtime and an explicit capability", () => {
  const item: PluginContextMenuContribution = {
    id: "inspect-context",
    scope: "node",
    label: "Analyze node context",
    icon: "sparkles",
  };
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.context@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "AnalysisPlugin",
      metadata: {
        id: "myc.context",
        name: "Context analyst",
        version: "1.0.0",
        publisher: "Research Canvas",
        developer: "Runtime Team",
        description: "Context menu smoke plugin",
      },
      spec: {
        engine: "wasm32-myc",
        entry: "plugin.wasm",
        language: "rust",
        capabilities: ["analysis.run", "context-menu.contribute"],
        permissions: [],
        contributes: { contextMenus: [item] },
      },
    },
    runtime: {
      engine: "wasm32-myc",
      language: "rust",
      entrySha256: "b".repeat(64),
    },
  };
  assert.equal(contextMenuContributionsFromPlugins([]).length, 0);
  const actions = contextMenuContributionsFromPlugins([plugin]);
  assert.equal(actions.length, 1);
  assert.equal(actions[0]?.contributionId, "inspect-context");
  assert.equal(actions[0]?.scope, "node");
  assert.equal(actions[0]?.plugin.id, "myc.context");
});

test("community locale plugins extend the UI without changing the built-in language set", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.i18n-ja@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "LocalePlugin",
      metadata: {
        id: "myc.i18n-ja",
        name: "Japanese UI",
        version: "1.0.0",
        publisher: "Research Canvas Community",
        developer: "Localization Example Team",
        description: "Community locale",
      },
      spec: {
        engine: "declarative",
        entry: "locales/ja-JP.json",
        capabilities: ["i18n.register"],
        permissions: [],
        contributes: {
          locales: [{ locale: "ja-JP", name: "日本語", path: "locales/ja-JP.json" }],
        },
      },
    },
    locales: [{ locale: "ja-JP", name: "日本語", messages: { "workspace.menu": "メニュー" } }],
  };
  const bundles = localeBundlesFromPlugins([plugin]);
  assert.equal(bundles.length, 1);
  assert.equal(normalizeLocale("ja-JP", ["en", "zh-CN", "ja-JP"]), "ja-JP");
  assert.equal(translate("ja-JP", "workspace.menu", { "ja-JP": bundles[0]!.messages }), "メニュー");
  assert.equal(translate("ja-JP", "workspace.undo", { "ja-JP": bundles[0]!.messages }), "Undo");
  assert.deepEqual(Object.keys(localeCatalog).sort(), ["en", "zh-CN"]);
});

test("active workspace commands require their matching declared capability", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.export-suite@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "WorkspacePlugin",
      metadata: {
        id: "myc.export-suite",
        name: "Export Suite",
        version: "1.0.0",
        publisher: "Research Canvas",
        developer: "Workspace Platform Team",
        description: "Bounded project export",
      },
      spec: {
        engine: "host-mediated",
        entry: "workspace-plugin.json",
        capabilities: ["project.export"],
        permissions: [],
        contributes: {
          commands: [
            {
              id: "export-artifacts",
              label: "Export PDF / SVG / PNG",
              description: "Export",
              category: "export",
              capability: "project.export",
              formats: ["pdf", "svg", "png"],
            },
            {
              id: "undeclared-command",
              label: "Unsafe command",
              description: "Must not surface",
              category: "git",
              capability: "git.autosave",
            },
          ],
        },
      },
    },
    workspace: { schemaVersion: 1, mode: "export", testFixture: "pinn-architecture" },
  };
  assert.equal(workspaceCommandsFromPlugins([]).length, 0);
  const commands = workspaceCommandsFromPlugins([plugin]);
  assert.deepEqual(commands.map((command) => command.id), ["export-artifacts"]);
});

test("the three workspace plugins share a complete PINN architecture fixture", () => {
  const project = JSON.parse(
    readFileSync("tests/fixtures/pinn-architecture.mycproj", "utf8"),
  ) as ProjectState;
  assert.equal(isProjectState(project), true);
  const nodes = new Map(project.nodes.map((node) => [node.id, node]));
  assert.deepEqual(nodes.get("hidden-dim")?.data.enumValues, [32, 64, 128]);
  assert.deepEqual(nodes.get("hidden-layers")?.data.enumValues, [8, 10, 12]);
  assert.deepEqual(nodes.get("fourier-embedding")?.data.values, [true, false]);
  assert.deepEqual(nodes.get("residual")?.data.residualLinks, [[1, 3], [3, 5], [5, 7], [7, 9]]);
  assert.deepEqual(nodes.get("hard-constraint")?.data.enumValues, ["none", "cos-sin"]);
  for (const nodeId of ["pde-loss", "separate-loss", "auto-weighted-loss"]) {
    assert.deepEqual(nodes.get(nodeId)?.data.values, [true, false]);
  }
  const ablation = nodes.get("fourier-off-ablation")?.data;
  assert.equal(ablation?.fourierEmbedding, false);
  assert.equal(ablation?.hiddenDim, 64);
  assert.equal(ablation?.hiddenLayers, 10);
  assert.equal(ablation?.gitTag, "research/pinn-fourier-off");
  assert.equal(nodes.get("pinn-backbone")?.data.framework, "torch");

  for (const pluginId of [
    "myc.export-suite",
    "myc.folder-workspaces",
    "myc.git-workspace",
  ]) {
    const descriptor = JSON.parse(
      readFileSync(`plugins/sources/${pluginId}/workspace-plugin.json`, "utf8"),
    ) as { testFixture?: string };
    assert.equal(descriptor.testFixture, "pinn-architecture", pluginId);
  }
  const svg = projectToSvg(project);
  assert.match(svg, /PINN MLP backbone/);
  assert.match(svg, /Hidden dimension/);
});

test("GraphPatch proposals are review-gated and structurally validated", () => {
  const patch = normalizePluginGraphPatch({
    apiVersion: "researchcanvas.dev/graph-patch/v1alpha1",
    source: { pluginId: "torch.blocks", operation: "torch-module-sync" },
    title: "Torch module proposal",
    summary: "Map model.backbone blocks into reviewable nodes",
    reviewRequired: true,
    operations: [
      {
        op: "add-node",
        node: {
          id: "torch-block-residual-1",
          type: "method",
          title: "ResidualMLP block 1→3",
          data: { modulePath: "model.backbone.1", framework: "torch" },
        },
      },
    ],
  });
  assert.equal(patch?.operations.length, 1);
  assert.equal(
    normalizePluginGraphPatch({ ...patch, reviewRequired: false }),
    null,
  );
  assert.equal(
    normalizePluginGraphPatch({
      ...patch,
      operations: [{ op: "add-node", node: { id: "missing-fields" } }],
    }),
    null,
  );

  // 文本字段有界；超长 body/note 应被拒绝。
  assert.equal(
    normalizePluginGraphPatch({
      ...patch,
      operations: [
        {
          op: "add-node",
          node: {
            id: "long-body",
            type: "note",
            title: "Long body",
            body: "x".repeat(10_001),
          },
        },
      ],
    }),
    null,
  );
  assert.equal(
    normalizePluginGraphPatch({
      ...patch,
      operations: [
        {
          op: "add-edge",
          edge: {
            id: "long-note",
            source: "a",
            target: "b",
            type: "causes",
            note: "x".repeat(2_001),
          },
        },
      ],
    }),
    null,
  );
});

test("built-in plugin catalog only advertises implemented or clearly reserved packages", () => {
  const ids = builtInPluginCatalog.map((plugin) => plugin.id);
  assert.ok(!ids.includes("pdf-canvas-agent"));
  assert.ok(!ids.includes("zotero-source"));
  assert.ok(!ids.includes("mcp-bridge"));
  assert.ok(!ids.includes("agent-runtime"));
});

test("AgentPlugin and ProviderPlugin are activatable install kinds", () => {
  const agent: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.pdf-canvas-agent@0.1.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "AgentPlugin",
      metadata: {
        id: "myc.pdf-canvas-agent",
        name: "PDF Canvas Agent",
        version: "0.1.0",
        publisher: "Research Canvas",
        developer: "Agent Platform Team",
        description: "Review-gated PDF canvas agent.",
      },
      spec: {
        engine: "host-mediated",
        entry: "agent-manifest.json",
        capabilities: [
          "agent.pdf.read",
          "agent.graph.patch.propose",
          "agent.review.request",
        ],
        permissions: [],
      },
    },
    agent: { schemaVersion: 1, mode: "agent", reviewGated: true },
  };
  assert.equal(pluginCompatibility(agent).compatible, true);
  assert.equal(
    pluginCompatibility({
      ...agent,
      agent: { schemaVersion: 1, mode: "agent", reviewGated: false as never },
    }).compatible,
    false,
  );
  assert.equal(
    pluginCompatibility({ ...agent, agent: undefined }).compatible,
    false,
  );

  const provider: InstalledMycPlugin = {
    installPath: "plugins/installed/myc.test-provider@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "ProviderPlugin",
      metadata: {
        id: "myc.test-provider",
        name: "Test Provider",
        version: "1.0.0",
        publisher: "Research Canvas",
        developer: "Provider Tests",
        description: "Test LLM provider.",
      },
      spec: {
        engine: "host-mediated",
        entry: "provider.json",
        capabilities: ["llm.chat"],
        permissions: [],
      },
    },
    provider: {
      schemaVersion: 1,
      provider: {
        type: "openai-compatible",
        baseUrl: "https://api.example.com",
        chatCompletionsPath: "/v1/chat/completions",
        defaultRouting: {
          extraction: { model: "m", thinking: false, jsonOutput: true },
          synthesis: { model: "m", thinking: false, jsonOutput: true },
          recovery: { model: "m", thinking: false, jsonOutput: false },
        },
        requiresApiKey: true,
      },
    },
  };
  assert.equal(pluginCompatibility(provider).compatible, true);
  assert.equal(
    pluginCompatibility({ ...provider, provider: undefined }).compatible,
    false,
  );

  // The shipped pdf-canvas-agent source stays review-gated and host-mediated.
  const descriptor = JSON.parse(
    readFileSync("plugins/sources/myc.pdf-canvas-agent/agent-manifest.json", "utf8"),
  ) as { mode?: string; reviewGated?: boolean };
  assert.equal(descriptor.mode, "agent");
  assert.equal(descriptor.reviewGated, true);
  const manifest = readFileSync(
    "plugins/sources/myc.pdf-canvas-agent/plugin.yml",
    "utf8",
  );
  assert.match(manifest, /kind: AgentPlugin/);
  assert.match(manifest, /engine: host-mediated/);
});
