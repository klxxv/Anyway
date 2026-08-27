#!/usr/bin/env node
/**
 * Materialize an explicit plugin runtime without executing source directories.
 *
 * Desktop dev defaults to official bundled packages only. Development sources
 * under my-plugins are staged only when their id is explicitly enabled in
 * config/plugin-loading.json or passed with --with-dev-plugin <pluginId>.
 */

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

const repository = process.cwd();
const args = process.argv.slice(2);
const mode = args.shift();

if (mode !== "dev" && mode !== "release") {
  console.error(
    "usage: node scripts/stage-plugin-runtime.mjs <dev|release> [--with-dev-plugin <pluginId>] [--clean-plugin <pluginId>@<version>]",
  );
  process.exit(2);
}

const cliDevPlugins = [];
let cleanPluginVersion = null;
for (let index = 0; index < args.length; index += 1) {
  const arg = args[index];
  if (arg === "--with-dev-plugin") {
    const value = args[index + 1];
    if (!value) throw new Error("--with-dev-plugin requires a plugin id");
    cliDevPlugins.push(value);
    index += 1;
    continue;
  }
  if (arg === "--clean-plugin") {
    const value = args[index + 1];
    if (!value) throw new Error("--clean-plugin requires <pluginId>@<version>");
    cleanPluginVersion = value;
    index += 1;
    continue;
  }
  throw new Error(`unknown argument: ${arg}`);
}

const configPath = path.join(repository, "config", "plugin-loading.json");
const config = JSON.parse(readFileSync(configPath, "utf8"));
const runtimeRoot = resolveRepoPath(
  config.runtimeRoots?.[mode === "dev" ? "dev" : "release"]
    ?? `.plugin-runtime/${mode === "dev" ? "dev" : "release-staging"}`,
  "runtime root",
);
const packagesRoot = path.join(runtimeRoot, "packages");
const installedRoot = path.join(runtimeRoot, "installed");
const quarantineRoot = path.join(runtimeRoot, "quarantine");

mkdirSync(packagesRoot, { recursive: true });
if (mode === "dev") {
  mkdirSync(installedRoot, { recursive: true });
  mkdirSync(quarantineRoot, { recursive: true });
}

if (cleanPluginVersion) {
  cleanStagedPluginVersion(cleanPluginVersion);
}

const modeConfig = mode === "dev" ? config.desktopDev : config.release;
const packageFiles = Array.isArray(modeConfig.packageFiles) ? modeConfig.packageFiles : [];
const freshBuilds = resolveFreshBuilds(mode);
const stagedPackages = [];

if (mode === "dev") {
  pruneDisabledInstalledPlugins(new Set([
    ...(modeConfig.allowedPluginIds ?? []),
    ...freshBuilds.map((build) => build.pluginId),
  ]));
}

if (mode === "dev") {
  const removedPath = path.join(runtimeRoot, "removed-plugins.json");
  const legacyRemovedPath = path.join(repository, "plugins", "removed-plugins.json");
  let removed = readJsonArrayIfPresent(removedPath);
  const legacyRemoved = readJsonArrayIfPresent(legacyRemovedPath);
  removed = [...new Set([...removed, ...legacyRemoved])];
  for (const build of freshBuilds) {
    const identity = pluginIdentity(readPluginManifest(build.source), build.source);
    removed = removed.filter((entry) => entry !== `${identity.id}@${identity.version}`);
  }
  writeFileSync(removedPath, `${JSON.stringify(removed, null, 2)}\n`, "utf8");
}

for (const build of freshBuilds) {
  validateFreshBuild(build);
  const sourceRoot = resolveRepoPath(build.source, `source for ${build.pluginId}`);
  const manifest = readPluginManifest(build.source);
  const identity = pluginIdentity(manifest, build.source);
  if (identity.id !== build.pluginId) {
    throw new Error(`fresh build id mismatch: expected ${build.pluginId}, got ${identity.id}`);
  }
  buildTrustedFrontend(sourceRoot, manifest, build.pluginId);
  const outputName = `${identity.id}@${identity.version}.myc`;
  const output = path.join(packagesRoot, outputName);
  const result = spawnSync(
    process.execPath,
    [path.join(repository, "scripts", "pack-plugin.mjs"), sourceRoot, output],
    { cwd: repository, encoding: "utf8" },
  );
  if (result.status !== 0) {
    throw new Error(`fresh build failed for ${build.pluginId}: ${result.stderr || result.stdout}`);
  }
  stagedPackages.push(outputName);
}

for (const relativeSource of packageFiles) {
  const basename = path.basename(relativeSource);
  if (freshBuilds.some((build) => basename.startsWith(`${build.pluginId}@`))) {
    continue;
  }
  const source = resolveRepoPath(relativeSource, "package file");
  const destination = path.join(packagesRoot, basename);
  copyFileSync(source, destination);
  stagedPackages.push(basename);
}

pruneUndeclaredPackages(new Set(stagedPackages));

writeFileSync(
  path.join(runtimeRoot, mode === "release" ? "release-manifest.json" : "dev-manifest.json"),
  `${JSON.stringify({
    apiVersion: config.apiVersion,
    mode,
    runtimeRoot: path.relative(repository, runtimeRoot).split(path.sep).join("/"),
    packageFiles: stagedPackages.map((name) => `packages/${name}`),
    freshBuilds: freshBuilds.map(({ pluginId, source }) => ({ pluginId, source })),
  }, null, 2)}\n`,
  "utf8",
);

console.log(`${mode} plugin runtime staged at ${runtimeRoot}`);

function resolveFreshBuilds(stageMode) {
  const configured = Array.isArray(modeConfig.freshBuilds) ? modeConfig.freshBuilds : [];
  if (stageMode !== "dev") return configured;

  const declared = new Map((config.developmentPlugins ?? []).map((entry) => [entry.pluginId, entry]));
  const enabledIds = new Set([
    ...(modeConfig.enabledDevelopmentPluginIds ?? []),
    ...cliDevPlugins,
  ]);
  for (const id of enabledIds) {
    if (!declared.has(id)) {
      throw new Error(`development plugin is not declared in config.developmentPlugins: ${id}`);
    }
  }
  return [
    ...configured,
    ...[...enabledIds].map((id) => declared.get(id)),
  ];
}

function validateFreshBuild(build) {
  if (!build || typeof build.source !== "string" || typeof build.pluginId !== "string") {
    throw new Error("freshBuilds entries require pluginId and source");
  }
  const sourceRoot = resolveRepoPath(build.source, `source for ${build.pluginId}`);
  const officialRoot = resolveRepoPath(config.sourceRoots?.official ?? "my-plugins", "official source root");
  if (!isInsideOrEqual(sourceRoot, officialRoot)) {
    throw new Error(`development source must be inside ${config.sourceRoots?.official ?? "my-plugins"}: ${build.source}`);
  }
}

function buildTrustedFrontend(sourceRoot, manifest, pluginId) {
  const frontend = manifest.frontend ?? manifest.spec?.frontend;
  if (!frontend || frontend.mode !== "trusted-module") return;
  const packageJson = path.join(sourceRoot, "package.json");
  if (!existsSync(packageJson)) {
    throw new Error(`trusted frontend ${pluginId} has no package.json build contract`);
  }
  const npmArgs = ["--prefix", sourceRoot, "run", "build:frontend"];
  const npmExecPath = process.env.npm_execpath;
  const result = npmExecPath && existsSync(npmExecPath)
    ? spawnSync(process.execPath, [npmExecPath, ...npmArgs], {
        cwd: repository,
        encoding: "utf8",
      })
    : spawnSync(process.platform === "win32" ? "npm.cmd" : "npm", npmArgs, {
        cwd: repository,
        encoding: "utf8",
        shell: process.platform === "win32",
      });
  if (result.status !== 0) {
    const detail = [result.error?.message, result.stderr, result.stdout]
      .filter((value) => typeof value === "string" && value.trim().length > 0)
      .join("\n")
      .trim();
    throw new Error(
      `frontend build failed for ${pluginId}: ${detail || `exit status ${String(result.status)}`}`,
    );
  }
  const output = path.resolve(sourceRoot, frontend.entry);
  assertInside(output, sourceRoot, `frontend output for ${pluginId}`);
  if (!existsSync(output)) {
    throw new Error(`frontend build did not create ${frontend.entry} for ${pluginId}`);
  }
}

function pruneDisabledInstalledPlugins(allowedPluginIds) {
  for (const entry of readdirSync(installedRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const separator = entry.name.lastIndexOf("@");
    if (separator <= 0) continue;
    const pluginId = entry.name.slice(0, separator);
    if (allowedPluginIds.has(pluginId)) continue;
    const target = path.join(installedRoot, entry.name);
    assertInside(target, installedRoot, "disabled installed plugin");
    rmSync(target, { recursive: true, force: true });
  }
}

function pruneUndeclaredPackages(allowedPackageNames) {
  for (const entry of readdirSync(packagesRoot, { withFileTypes: true })) {
    if (allowedPackageNames.has(entry.name) && entry.isFile()) continue;
    const target = path.join(packagesRoot, entry.name);
    assertInside(target, packagesRoot, "stale staged package");
    rmSync(target, { recursive: entry.isDirectory(), force: true });
  }
}

function readPluginManifest(relativeSource) {
  const sourceRoot = resolveRepoPath(relativeSource, "plugin source");
  const pluginJson = path.join(sourceRoot, "plugin.json");
  const manifestJson = path.join(sourceRoot, "manifest.json");
  if (existsSync(pluginJson)) return JSON.parse(readFileSync(pluginJson, "utf8"));
  if (existsSync(manifestJson)) return JSON.parse(readFileSync(manifestJson, "utf8"));
  throw new Error(`plugin source has no plugin.json or manifest.json: ${relativeSource}`);
}

function pluginIdentity(manifest, sourceLabel) {
  const id = manifest.name ?? manifest.metadata?.id;
  const version = manifest.version ?? manifest.metadata?.version;
  if (typeof id !== "string" || typeof version !== "string") {
    throw new Error(`plugin source must declare name/version or metadata.id/metadata.version: ${sourceLabel}`);
  }
  return { id, version };
}

function readJsonArrayIfPresent(file) {
  try {
    const parsed = JSON.parse(readFileSync(file, "utf8"));
    if (!Array.isArray(parsed)) throw new Error(`${file} must contain a JSON array`);
    return parsed;
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
}

function cleanStagedPluginVersion(pluginVersion) {
  if (!/^[A-Za-z0-9._:-]+@[A-Za-z0-9._+-]+$/u.test(pluginVersion)) {
    throw new Error("--clean-plugin must be an exact <pluginId>@<version> without path separators");
  }
  const targets = [
    path.join(packagesRoot, `${pluginVersion}.myc`),
    path.join(installedRoot, pluginVersion),
    path.join(quarantineRoot, pluginVersion),
  ];
  for (const target of targets) {
    assertInside(target, runtimeRoot, "clean target");
    rmSync(target, { recursive: true, force: true });
  }
}

function resolveRepoPath(relativePath, label) {
  if (typeof relativePath !== "string" || relativePath.length === 0) {
    throw new Error(`${label} must be a non-empty relative path`);
  }
  if (path.isAbsolute(relativePath) || relativePath.includes("\\")) {
    throw new Error(`${label} must be a repository-relative POSIX path`);
  }
  const resolved = path.resolve(repository, relativePath);
  assertInside(resolved, repository, label);
  return resolved;
}

function assertInside(candidate, root, label) {
  if (!isInsideOrEqual(candidate, root)) {
    throw new Error(`${label} escapes its allowed root: ${candidate}`);
  }
}

function isInsideOrEqual(candidate, root) {
  const relative = path.relative(path.resolve(root), path.resolve(candidate));
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
}
