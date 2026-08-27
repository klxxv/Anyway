import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

type Manifest = {
  capabilities?: string[];
  workers?: Array<Record<string, unknown>>;
  contributes?: {
    configuration?: {
      settings?: Array<Record<string, unknown>>;
      connections?: Array<Record<string, unknown>>;
    };
  };
};

function readText(path: string): string {
  return readFileSync(path, "utf8");
}

function readJson<T>(path: string): T {
  return JSON.parse(readText(path)) as T;
}

function functionBody(source: string, name: string): string {
  const start = source.indexOf(`function ${name}`);
  assert.notEqual(start, -1, `${name} should exist`);
  const tail = source.slice(start + 1);
  const nextFunction = /\n(?:async\s+)?function\s+\w+/u.exec(tail);
  const next = nextFunction ? start + 1 + nextFunction.index : -1;
  return source.slice(start, next === -1 ? undefined : next);
}

const manifest = readJson<Manifest>("my-plugins/anPdfsolver/plugin.json");
const contextSource = readText("my-plugins/anPdfsolver/frontend/src/context.ts");
const storeSource = readText("my-plugins/anPdfsolver/frontend/src/store.ts");
const dialogSource = readText("my-plugins/anPdfsolver/frontend/src/components/AnPdfsolverDialog.vue");
const runtimeConfigSource = readText("my-plugins/anPdfsolver/frontend/src/runtime-config.ts");
const indexSource = readText("my-plugins/anPdfsolver/frontend/src/index.ts");

test("plugin frontend context exposes only dedicated plugin APIs, not raw graph.project or HostSdk", () => {
  assert.match(contextSource, /files:\s*\{/u);
  assert.match(contextSource, /worker:\s*\{/u);
  assert.match(contextSource, /graphPatch:\s*\{/u);
  assert.match(contextSource, /settings\?:\s*\{/u);
  assert.doesNotMatch(contextSource, /graphProject|graph\.project|HostSdk|callWithBlob|host:\s*\{/u);
});

test("worker DTO keeps session identity single and request payload domain-specific", () => {
  const ensureWorker = functionBody(storeSource, "ensureWorker");
  const runJob = functionBody(storeSource, "runJob");
  assert.equal((ensureWorker.match(/sessionId:/gu) ?? []).length, 2);
  assert.match(ensureWorker, /worker\.open\(workerId,\s*\{\s*sessionId:\s*state\.analysisSessionId/u);
  assert.match(ensureWorker, /trackWorker\(\{\s*workerId,\s*handle,\s*sessionId:\s*state\.analysisSessionId/u);

  assert.match(runJob, /operation:\s*"anpdfsolver\.analyze"/u);
  assert.match(runJob, /analysisSessionId:\s*state\.analysisSessionId/u);
  assert.match(runJob, /requestId:\s*job\.requestId/u);
  assert.match(runJob, /jobId:\s*job\.id/u);
  assert.match(runJob, /projectId:\s*binding\.projectId/u);
  assert.match(runJob, /baseRevision:\s*binding\.baseRevision/u);
  const workerCallEnvelope = runJob.slice(
    runJob.indexOf("}>(worker.handle, {"),
    runJob.indexOf("payload:", runJob.indexOf("}>(worker.handle, {")),
  );
  assert.doesNotMatch(workerCallEnvelope, /sessionId:\s*state\.analysisSessionId/u);
});

test("GraphPatch binding is complete for proposal, get, accept and reject", () => {
  const runJob = functionBody(storeSource, "runJob");
  const acceptReview = functionBody(storeSource, "acceptReview");
  const rejectReview = functionBody(storeSource, "rejectReview");
  for (const body of [runJob, acceptReview, rejectReview]) {
    assert.match(body, /sessionId:\s*state\.analysisSessionId/u);
    assert.match(body, /projectId:\s*binding\.projectId/u);
    assert.match(body, /baseRevision:/u);
  }
  assert.match(runJob, /context\.graphPatch\.propose/u);
  assert.match(runJob, /context\.graphPatch\.get/u);
  assert.match(runJob, /expectedDigest:\s*proposal\.digest/u);
  assert.match(acceptReview, /accept:\s*true/u);
  assert.match(rejectReview, /accept:\s*false/u);
  assert.match(acceptReview, /expectedDigest:\s*job\.digest/u);
  assert.match(rejectReview, /expectedDigest:\s*job\.digest/u);
  assert.doesNotMatch(`${acceptReview}\n${rejectReview}`, /project:\s*|applyGraphPatch/u);
});

test("batch execution isolates failed jobs and continues through queued targets", () => {
  const startBatch = functionBody(storeSource, "startBatch");
  const runJob = functionBody(storeSource, "runJob");
  const retryFailed = functionBody(storeSource, "retryFailed");

  assert.match(startBatch, /for \(const job of targets\)/u);
  assert.match(startBatch, /await runJob\(job,\s*hostContext\)/u);
  assert.match(startBatch, /finally\s*\{\s*state\.busy = false/u);
  assert.match(runJob, /catch \(error\)/u);
  assert.match(runJob, /job\.state = "failed"/u);
  assert.match(runJob, /state\.lastError = job\.error/u);
  assert.match(retryFailed, /state\.jobs\.filter\(\(item\) => item\.state === "failed"\)/u);
  assert.match(retryFailed, /job\.state = "queued"/u);
});

test("typed response frames are bounded in UI and do not ask LLM for one giant JSON blob", () => {
  assert.match(storeSource, /export type PublicFrame = Record<string, unknown> & \{ type\?: string; seq\?: number; message\?: string \}/u);
  assert.match(storeSource, /frames\?: PublicFrame\[\]/u);
  assert.match(storeSource, /job\.frames = result\.frames \|\| \[\]/u);
  assert.match(dialogSource, /v-for="frame in selectedJob\?\.frames \|\| \[\]"/u);
  assert.match(dialogSource, /frame\.type \|\| "frame"/u);
  assert.match(dialogSource, /JSON\.stringify\(frame\)\.slice\(0,\s*32\)/u);
  assert.doesNotMatch(`${storeSource}\n${dialogSource}`, /one giant JSON|single huge JSON|JSON mode only/iu);
});

test("review controls gate accept/reject by canonical digest and review state", () => {
  assert.match(dialogSource, /:disabled="!selectedJob\.digest \|\| selectedJob\.state !== 'review'"/u);
  assert.match(dialogSource, /@click="acceptReview\(hostContext\)"/u);
  assert.match(dialogSource, /:disabled="selectedJob\.state !== 'review'"/u);
  assert.match(dialogSource, /@click="rejectReview\(hostContext\)"/u);

  const acceptReview = functionBody(storeSource, "acceptReview");
  const rejectReview = functionBody(storeSource, "rejectReview");
  assert.match(acceptReview, /if \(!job\?\.proposalId \|\| !job\.digest\) return/u);
  assert.match(rejectReview, /if \(!job\?\.proposalId\) return/u);
  assert.match(rejectReview, /if \(!job\.digest\) throw new Error\("Canonical digest is missing\."\)/u);
  assert.match(acceptReview, /job\.state = "accepted"/u);
  assert.match(rejectReview, /job\.state = "rejected"/u);
});

test("runtime config keeps direct Kimi networking plugin-owned and names secret env only", () => {
  assert.match(runtimeConfigSource, /id:\s*"kimi"/u);
  assert.match(runtimeConfigSource, /baseUrl:\s*await readSetting\("api-url"/u);
  assert.match(runtimeConfigSource, /pdfTransport:\s*await readSetting\("pdf-transport"/u);
  assert.match(runtimeConfigSource, /publicProgress:\s*await readSetting\("public-progress"/u);
  assert.match(runtimeConfigSource, /allowedDomains:\s*\["api\.moonshot\.cn",\s*"api\.moonshot\.ai"\]/u);
  assert.match(runtimeConfigSource, /secretEnv:\s*"ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY"/u);

  const connection = manifest.contributes?.configuration?.connections?.[0];
  assert.equal(connection?.id, "kimi");
  assert.deepEqual(connection?.apiKey, {
    source: "host-secret",
    settingId: "api-key",
    secretEnv: "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY",
  });
});

test("manifest capabilities match dedicated PluginContext operations", () => {
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
    assert.ok(manifest.capabilities?.includes(capability), `${capability} should be declared`);
  }
  assert.ok(!manifest.capabilities?.includes("graph.project.get"));
  assert.ok(!manifest.capabilities?.includes("graph.project.sync"));
});

test("frontend module activates plugin context and deactivates tracked workers", () => {
  assert.match(indexSource, /export \{ AnPdfsolverDialog,\s*AnPdfsolverToolbarButton \}/u);
  assert.match(indexSource, /export async function activate/u);
  assert.match(indexSource, /setPluginContext\(context\)/u);
  assert.match(indexSource, /export async function deactivate/u);
  assert.match(indexSource, /await closeTrackedWorkers\(\)/u);
  assert.match(indexSource, /clearPluginContext\(\)/u);
});
