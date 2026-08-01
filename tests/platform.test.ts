import assert from "node:assert/strict";
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

test("locale normalization and Simplified Chinese catalog are deterministic", () => {
  assert.equal(normalizeLocale("zh-CN"), "zh-CN");
  assert.equal(normalizeLocale("zh-Hans"), "zh-CN");
  assert.equal(normalizeLocale("en-US"), "en");
  assert.equal(translate("zh-CN", "toolbar.filter"), "筛选");
  assert.equal(translate("en", "toolbar.filter"), "Filter");
  assert.equal(translate("zh-CN", "workspace.pluginStore"), "插件商店");
  assert.equal(translate("zh-CN", "layout.neural"), "神经网络");
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
