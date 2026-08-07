import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import { readdirSync, existsSync } from "node:fs";
import test from "node:test";

const templateRoot = new URL("../", import.meta.url);

test("Next.js build produces the research canvas application shell", async () => {
  // Next.js outputs to .next/ (not dist/ — that was the vinext/Cloudflare path).
  const nextDir = new URL("../.next/", import.meta.url);
  await assert.doesNotReject(access(nextDir), ".next/ directory must exist after build");

  // Static pages are pre-rendered into the server app bundle.
  const pageJs = new URL("../.next/server/app/page.js", import.meta.url);
  const pageContent = await readFile(pageJs, "utf8");
  assert.match(pageContent, /城市树冠与热岛效应/);
  assert.notMatch(pageContent, /codex-preview|Your site is taking shape|Building your site/i);

  // Verify key build-manifest files exist.
  const requiredFiles = [
    ".next/BUILD_ID",
    ".next/build-manifest.json",
    ".next/app-build-manifest.json",
  ];
  for (const f of requiredFiles) {
    const p = new URL(`../${f}`, import.meta.url);
    await assert.doesNotReject(access(p), `${f} must exist`);
  }
});

test("removes disposable starter assets and preserves product metadata", async () => {
  const [page, layout, packageJson] = await Promise.all([
    readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
  ]);

  assert.match(page, /ResearchCanvasApp/);
  assert.match(page, /城市树冠与热岛效应/);
  assert.match(layout, /title:\s*\{/);
  assert.match(layout, /Research Canvas/);
  assert.match(packageJson, /"@xyflow\/react"/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  assert.doesNotMatch(page + layout, /codex-preview|_sites-preview|SkeletonPreview/);

  await assert.rejects(access(new URL("../app/_sites-preview", templateRoot)));
});
