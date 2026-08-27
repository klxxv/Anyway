import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { DEFAULT_HOST_SLOT_CATALOG } from "../app/plugins/host-slot-registry";
import { selectPluginUiContributions } from "../app/plugins/plugin-surface-selection";
import type { InstalledMycPlugin } from "../app/plugins/contracts";

type AnPdfsolverManifest = {
  name: string;
  version: string;
  displayName: string;
  frontend?: Record<string, unknown>;
  contributes?: {
    ui?: Array<Record<string, unknown>>;
    uiIr?: unknown;
  };
  workers?: Array<Record<string, unknown>>;
  network?: Record<string, unknown>;
};

function readText(path: string): string {
  return readFileSync(path, "utf8");
}

function readJson<T>(path: string): T {
  return JSON.parse(readText(path)) as T;
}

const manifestPath = "my-plugins/anPdfsolver/plugin.json";
const dialogPath = "my-plugins/anPdfsolver/frontend/src/components/AnPdfsolverDialog.vue";
const toolbarPath = "my-plugins/anPdfsolver/frontend/src/components/AnPdfsolverToolbarButton.vue";
const storePath = "my-plugins/anPdfsolver/frontend/src/store.ts";
const pluginContextPath = "src/vue/runtime/plugin-contributions/plugin-context.ts";
const workspaceAppPath = "src/vue/ResearchWorkspaceApp.vue";

function installedFromManifest(manifest: AnPdfsolverManifest): InstalledMycPlugin {
  return {
    installPath: "my-plugins/anPdfsolver",
    manifest: {
      metadata: {
        id: manifest.name,
        version: manifest.version,
        name: manifest.displayName,
      },
      frontend: manifest.frontend,
      spec: {
        contributes: {
          ui: manifest.contributes?.ui ?? [],
        },
      },
    },
  } as InstalledMycPlugin;
}

test("anPdfsolver manifest declares trusted Vue frontend, physical slots, worker and direct network", () => {
  const manifest = readJson<AnPdfsolverManifest>(manifestPath);
  assert.deepEqual(manifest.frontend, {
    mode: "trusted-module",
    entry: "dist/frontend.mjs",
    framework: "vue3",
    apiVersion: "1",
  });

  assert.equal(manifest.contributes?.ui?.length, 2);
  assert.deepEqual(
    manifest.contributes?.ui?.map((contribution) => ({
      id: contribution.id,
      slotId: contribution.slotId,
      export: contribution.export,
    })),
    [
      {
        id: "anpdfsolver.toolbar",
        slotId: "workspace.toolbar.actions",
        export: "AnPdfsolverToolbarButton",
      },
      {
        id: "anpdfsolver.dialog",
        slotId: "workspace.dialogs",
        export: "AnPdfsolverDialog",
      },
    ],
  );
  assert.equal(manifest.contributes?.uiIr, undefined);
  assert.doesNotMatch(JSON.stringify(manifest), /\.uiir\.json/u);

  const worker = manifest.workers?.[0];
  assert.equal(worker?.id, "python-analyzer");
  assert.equal(worker?.language, "python");
  assert.equal(worker?.transport, "stdio-framed-json-v1");
  assert.ok((worker?.operations as string[]).includes("anpdfsolver.analyze"));
  assert.ok((worker?.hostOperations as string[]).includes("blob.read"));
  assert.equal(manifest.network?.mode, "direct");
  assert.deepEqual(manifest.network?.declaredDomains, ["api.moonshot.cn", "api.moonshot.ai"]);
});

test("anPdfsolver trusted Vue UI owns upload, batch, stream, error and canonical review controls", () => {
  const dialog = readText(dialogPath);
  const toolbar = readText(toolbarPath);
  const store = readText(storePath);

  for (const expected of [
    "Choose PDF",
    "Start batch",
    "Retry failed",
    "Response API SSE",
    "No model frames yet.",
    "Error",
    "Canonical Review",
    "Accept",
    "Reject",
  ]) {
    assert.match(dialog, new RegExp(expected.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "u"));
  }
  assert.match(toolbar, /Analyze PDF/u);
  assert.match(toolbar, /openPanel/u);

  for (const method of [
    "pickFiles",
    "startBatch",
    "retryFailed",
    "cancelSelected",
    "acceptReview",
    "rejectReview",
    "runJob",
  ]) {
    assert.match(store, new RegExp(`function ${method}\\b`, "u"));
  }
  assert.match(store, /frames:\s*PublicFrame\[\]/u);
  assert.match(store, /job\.frames = result\.frames \|\| \[\]/u);
  assert.match(store, /context\.worker\.call/u);
  assert.match(store, /context\.graphPatch\.propose/u);
  assert.match(store, /context\.graphPatch\.get/u);
  assert.match(store, /context\.graphPatch\.review/u);
});

test("Host workspace mounts generic plugin contribution slots and contains no PDF-specific branches", () => {
  const source = readText(workspaceAppPath);
  const slots = source.match(/<PluginContributionSlot\b/gu) ?? [];
  assert.equal(slots.length, 3);
  assert.match(source, /slot-id="workspace\.toolbar\.actions"/u);
  assert.match(source, /slot-id="workspace\.status"/u);
  assert.match(source, /slot-id="workspace\.dialogs"/u);

  assert.doesNotMatch(source, /PdfUploadDialog|AgentReviewPanel|pdf-agent-host-slots/u);
  assert.doesNotMatch(source, /PDF_AGENT_PLUGIN_ID|hasPdfAgent|import-pdf|myc\.pdf-canvas-agent/u);
});

test("plugin-context review bridges accepted Rust receipts to canonical project events only", () => {
  const source = readText(pluginContextPath);
  assert.match(source, /review:\s*async\s*<T = unknown>/u);
  assert.match(source, /"graph\.patch\.review"/u);
  assert.match(source, /getGraphProject\(request\.projectId,\s*newRevision,\s*signal\)/u);
  assert.match(source, /emitGraphProjectCommitted\(snapshot\)/u);
  assert.match(source, /return receipt/u);
  assert.doesNotMatch(source, /reviewed\.patch|reviewedPatch|applyGraphPatch|workspace\.applyGraphPatch/u);
});

test("PluginContext runtime does not expose raw HostSdk or Blob calls to plugin modules", () => {
  const source = readText(pluginContextPath);
  assert.doesNotMatch(source, /host:\s*Object\.freeze/u);
  assert.doesNotMatch(source, /callWithBlob/u);
  assert.doesNotMatch(source, /readonly host:/u);
});

test("trusted UI contributions are selected by physical Host slot catalog", () => {
  const manifest = readJson<AnPdfsolverManifest>(manifestPath);
  const installed = installedFromManifest(manifest);
  const toolbarSlot = DEFAULT_HOST_SLOT_CATALOG.find((slot) => slot.id === "workspace.toolbar.actions");
  const dialogSlot = DEFAULT_HOST_SLOT_CATALOG.find((slot) => slot.id === "workspace.dialogs");
  assert.ok(toolbarSlot);
  assert.ok(dialogSlot);

  const toolbar = selectPluginUiContributions([installed], toolbarSlot);
  const dialog = selectPluginUiContributions([installed], dialogSlot);
  assert.equal(toolbar.length, 1);
  assert.equal(dialog.length, 1);
  assert.equal(toolbar[0]?.contribution.export, "AnPdfsolverToolbarButton");
  assert.equal(dialog[0]?.contribution.export, "AnPdfsolverDialog");
  assert.ok(toolbarSlot.accepts.includes("trusted-module"));
  assert.ok(dialogSlot.accepts.includes("trusted-module"));
});
