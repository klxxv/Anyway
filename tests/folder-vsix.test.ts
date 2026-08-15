import assert from "node:assert/strict";
import test from "node:test";
import {
  assertDeclarativeVsixPackage,
  isSafeVsixAssetPath,
} from "../app/plugins/vsix-contracts";

test("VSIX importer accepts only safe declarative asset paths", () => {
  assert.equal(isSafeVsixAssetPath("themes/dark.json"), true);
  assert.equal(isSafeVsixAssetPath("icons/file.svg"), true);
  assert.equal(isSafeVsixAssetPath("fonts/file.woff2"), true);
  assert.equal(isSafeVsixAssetPath("../outside.json"), false);
  assert.equal(isSafeVsixAssetPath("icons\\outside.svg"), false);
  assert.equal(isSafeVsixAssetPath("extension.js"), false);
});

test("VSIX importer rejects executable extension contributions", () => {
  assert.throws(() =>
    assertDeclarativeVsixPackage({
      name: "unsafe-theme",
      version: "1.0.0",
      main: "extension.js",
      contributes: { themes: [{ label: "Unsafe", path: "theme.json" }] },
    }),
  /main\/browser code/);
  assert.throws(() =>
    assertDeclarativeVsixPackage({
      name: "command-theme",
      version: "1.0.0",
      contributes: {
        themes: [{ label: "Command", path: "theme.json" }],
        commands: [{ command: "unsafe.run" }],
      },
    }),
  /commands/);
});

test("Folder Explorer is a lazy host-mediated tree, not a recursive frontend walk", async () => {
  const workspaceHost = await import("../app/platform/native-project");
  const source = await import("node:fs/promises").then((fs) => fs.readFile(
    "src/vue/components/FolderExplorerTree.vue",
    "utf8",
  ));
  assert.match(source, /childrenByPath/);
  assert.match(source, /toggle-folder/);
  assert.equal(typeof workspaceHost.listFolderEntries, "function");
});
