import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  localeCatalog,
  normalizeLocale,
  translate,
} from "../app/i18n/catalog";
import {
  MYC_API_VERSION,
  isMycFileName,
  normalizeInstalledEdgeStyle,
  normalizeInstalledTheme,
  type PluginContextMenuContribution,
  type InstalledMycPlugin,
} from "../app/plugins/contracts";
import { contextMenuContributionsFromPlugins } from "../app/plugins/context-menu";
import { isProjectState } from "../app/lib/project-io";
import type { ProjectState } from "../app/lib/research-types";
import {
  localeBundlesFromPlugins,
  normalizePluginGraphPatch,
  projectToSvg,
  workspaceCommandsFromPlugins,
} from "../app/plugins/workspace";

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

test("installed EdgeStylePlugin metadata owns the registered style identity", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/researchcanvas.circuit-orthogonal@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "EdgeStylePlugin",
      metadata: {
        id: "researchcanvas.circuit-orthogonal",
        name: "Circuit Orthogonal",
        version: "1.0.0",
        publisher: "Research Canvas Community",
        developer: "Visual Systems",
        description: "Strict 90-degree semantic connectors",
      },
      spec: {
        engine: ">=0.1.0",
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
  assert.equal(edgeStyle?.id, "researchcanvas.circuit-orthogonal");
  assert.equal(edgeStyle?.name, "Circuit Orthogonal");
  assert.equal(edgeStyle?.version, "1.0.0");
  assert.equal(edgeStyle?.routing, "orthogonal");
  assert.equal(edgeStyle?.source, "myc");
});

test(".myc filenames are recognized case-insensitively", () => {
  assert.equal(isMycFileName("researchcanvas.onedarkpro@1.0.0.myc"), true);
  assert.equal(isMycFileName("THEME.MYC"), true);
  assert.equal(isMycFileName("theme.zip"), false);
});

test("installed ThemePlugin metadata owns the registered theme identity", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/researchcanvas.onedarkpro@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "ThemePlugin",
      metadata: {
        id: "researchcanvas.onedarkpro",
        name: "One Dark Pro",
        version: "1.0.0",
        publisher: "Research Canvas Community",
        developer: "Theme Lab",
        description: "Dark research theme",
      },
      spec: {
        engine: ">=0.1.0",
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
    },
  };

  const theme = normalizeInstalledTheme(plugin);
  assert.equal(theme?.id, "researchcanvas.onedarkpro");
  assert.equal(theme?.name, "One Dark Pro");
  assert.equal(theme?.version, "1.0.0");
  assert.equal(theme?.source, "myc");
});

test("runtime plugin metadata exposes only the verified wasm boundary", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/researchcanvas.runtime-smoke@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "AnalysisPlugin",
      metadata: {
        id: "researchcanvas.runtime-smoke",
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
});

test("plugin context menus require runtime, enablement, and an explicit capability", () => {
  const item: PluginContextMenuContribution = {
    id: "inspect-context",
    scope: "node",
    label: "Analyze node context",
    icon: "sparkles",
  };
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/researchcanvas.context@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "AnalysisPlugin",
      metadata: {
        id: "researchcanvas.context",
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
  assert.equal(contextMenuContributionsFromPlugins([plugin], new Set()).length, 0);
  const actions = contextMenuContributionsFromPlugins(
    [plugin],
    new Set(["researchcanvas.context@1.0.0"]),
  );
  assert.equal(actions.length, 1);
  assert.equal(actions[0]?.scope, "node");
  assert.equal(actions[0]?.pluginId, "researchcanvas.context");
});

test("community locale plugins extend the UI without changing the built-in language set", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/researchcanvas.i18n-ja@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "LocalePlugin",
      metadata: {
        id: "researchcanvas.i18n-ja",
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
  const enabled = new Set(["researchcanvas.i18n-ja@1.0.0"]);
  const bundles = localeBundlesFromPlugins([plugin], enabled);
  assert.equal(bundles.length, 1);
  assert.equal(normalizeLocale("ja-JP", ["en", "zh-CN", "ja-JP"]), "ja-JP");
  assert.equal(translate("ja-JP", "workspace.menu", { "ja-JP": bundles[0]!.messages }), "メニュー");
  assert.equal(translate("ja-JP", "workspace.undo", { "ja-JP": bundles[0]!.messages }), "Undo");
  assert.deepEqual(Object.keys(localeCatalog).sort(), ["en", "zh-CN"]);
});

test("workspace commands are enabled only with their matching declared capability", () => {
  const plugin: InstalledMycPlugin = {
    installPath: "plugins/installed/researchcanvas.export-suite@1.0.0",
    manifest: {
      apiVersion: MYC_API_VERSION,
      kind: "WorkspacePlugin",
      metadata: {
        id: "researchcanvas.export-suite",
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
  assert.equal(workspaceCommandsFromPlugins([plugin], new Set()).length, 0);
  const commands = workspaceCommandsFromPlugins(
    [plugin],
    new Set(["researchcanvas.export-suite@1.0.0"]),
  );
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
    "researchcanvas.export-suite",
    "researchcanvas.folder-workspaces",
    "researchcanvas.git-workspace",
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
});
