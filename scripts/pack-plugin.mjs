#!/usr/bin/env node
/**
 * Deterministic Research Canvas .myc packager + Ed25519 signer.
 *
 * Usage:
 *   node scripts/pack-plugin.mjs <source-dir> <out.myc> [--key path/to/id_ed25519.pem]
 *
 * The signing key is an offline Ed25519 PKCS#8 PEM. It must never live in the
 * repository (the workspace .gitignore already excludes id_ed25519*). If no
 * key is provided the archive is built unsigned (payloads are still hashed,
 * matching the Rust verifier's "unsigned package" path).
 *
 * Signature coverage (mirrors `src-tauri/src/signing.rs::manifest_payload`):
 *   signature = Ed25519_sign( SHA-256( JSON(manifest without `signature`) ) )
 * The manifest carries `payloads` (sha256 of every payload byte), so one
 * signature transitively covers the whole package.
 */

import { createHash, createPrivateKey, sign } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const ALLOWED_ROOT_FILES = new Set([
  "plugin.json",
  "agent.yml",
  "theme.json",
  "edge-style.json",
  "workspace-plugin.json",
  "agent-manifest.json",
  "provider.json",
  "plugin.wasm",
  "plugin.wat",
  "pyproject.toml",
  "README.md",
  "LICENSE",
  "index.ts",
  "types.ts",
]);
const ALLOWED_DIRECTORIES = {
  artifacts: new Set([".json", ".jsonl", ".md", ".ndjson", ".txt", ".wasm"]),
  dist: new Set([".css", ".js", ".json", ".map", ".mjs", ".wasm"]),
  locales: new Set([".json"]),
  prompts: new Set([".yaml", ".yml"]),
  schemas: new Set([".json"]),
  src: new Set([".py"]),
  ui: new Set([".vue"]),
  workers: new Set([".js", ".json", ".mjs", ".py", ".toml", ".wasm"]),
};
const SOURCE_ONLY_ROOT_DIRECTORIES = new Set([".git", "frontend", "node_modules"]);
const SOURCE_ONLY_ROOT_FILES = new Set([
  ".gitignore",
  "package-lock.json",
  "package.json",
  "pnpm-lock.yaml",
  "yarn.lock",
]);

function walkFiles(dir, root = dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = path.join(dir, name);
    const stat = statSync(full);
    const isRootEntry = path.dirname(path.relative(root, full)) === ".";
    if (stat.isDirectory()) {
      if (
        name !== "__pycache__" &&
        name !== "node_modules" &&
        name !== ".git" &&
        !(isRootEntry && SOURCE_ONLY_ROOT_DIRECTORIES.has(name))
      ) {
        out.push(...walkFiles(full, root));
      }
    } else if (isRootEntry && SOURCE_ONLY_ROOT_FILES.has(name)) {
      continue;
    } else if (path.extname(name).toLowerCase() !== ".pyc") {
      out.push(full);
    }
  }
  return out;
}

// Minimal CRC-32 (IEEE) table — no external zip dependency.
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buffer) {
  let crc = 0xffffffff;
  for (const byte of buffer) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

function zipHeader(name, crc, size) {
  const nameBytes = Buffer.from(name, "utf8");
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0); // local file header signature
  local.writeUInt16LE(20, 4); // version needed
  local.writeUInt16LE(0x0800, 6); // UTF-8 flag
  local.writeUInt16LE(0, 8); // STORE method (deterministic)
  local.writeUInt16LE(0, 10); // mod time
  local.writeUInt16LE(0x21, 12); // mod date (1980-01-01)
  local.writeUInt32LE(crc, 14);
  local.writeUInt32LE(size, 18); // compressed size
  local.writeUInt32LE(size, 22); // uncompressed size
  local.writeUInt16LE(nameBytes.length, 26);
  local.writeUInt16LE(0, 28); // extra length
  return Buffer.concat([local, nameBytes]);
}

function zipCentral(name, crc, size, offset) {
  const nameBytes = Buffer.from(name, "utf8");
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0); // central directory signature
  central.writeUInt16LE(20, 4); // version made by
  central.writeUInt16LE(20, 6); // version needed
  central.writeUInt16LE(0x0800, 8); // UTF-8 flag
  central.writeUInt16LE(0, 10); // STORE
  central.writeUInt16LE(0, 12);
  central.writeUInt16LE(0x21, 14);
  central.writeUInt32LE(crc, 16);
  central.writeUInt32LE(size, 20);
  central.writeUInt32LE(size, 24);
  central.writeUInt16LE(nameBytes.length, 28);
  central.writeUInt32LE(offset, 42);
  return Buffer.concat([central, nameBytes]);
}

function zipEnd(entryCount, centralSize, centralOffset) {
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0); // end of central directory
  end.writeUInt16LE(entryCount, 8);
  end.writeUInt16LE(entryCount, 10);
  end.writeUInt32LE(centralSize, 12);
  end.writeUInt32LE(centralOffset, 16);
  return end;
}

/**
 * Canonical JSON: keys sorted, compact, UTF-8. Matches the Rust verifier,
 * which parses the raw manifest text into a `serde_json::Value` (BTreeMap =
 * sorted keys) and re-serializes with `serde_json::to_vec` (compact).
 */
function canonicalJson(value) {
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  if (typeof value === "object") {
    const keys = Object.keys(value).sort();
    return `{${keys
      .map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function canonicalManifestJson(manifest) {
  const { signature: _signature, ...withoutSignature } = manifest;
  return canonicalJson(withoutSignature);
}

function validateUiPath(value, extension, label) {
  if (typeof value !== "string" || value.startsWith("/") || value.includes("\\")) {
    throw new Error(`${label} must be a relative path`);
  }
  const parts = value.split("/");
  if (parts.length !== 2 || parts[0] !== "ui" || parts[1].startsWith(".") || !value.endsWith(extension)) {
    throw new Error(`${label} must be ui/<name>${extension}`);
  }
  if (parts.some((part) => part === "" || part === "." || part === "..")) {
    throw new Error(`${label} contains an unsafe path component`);
  }
}

function validateRelativePackagePath(value, label, extensions = null) {
  if (typeof value !== "string" || value.length === 0 || value.startsWith("/") || value.includes("\\")) {
    throw new Error(`${label} must be a non-empty relative POSIX path`);
  }
  const parts = value.split("/");
  if (parts.some((part) => part === "" || part === "." || part === "..")) {
    throw new Error(`${label} contains an unsafe path component`);
  }
  if (extensions && !extensions.has(path.extname(value).toLowerCase())) {
    throw new Error(`${label} has unsupported extension: ${value}`);
  }
  return parts.join("/");
}

function requirePackagedEntry(entries, relativePath, label) {
  if (!entries.some((entry) => entry.relative === relativePath)) {
    throw new Error(`${label} is not packaged or does not exist: ${relativePath}`);
  }
}

function validateFrontendContract(manifest, entries) {
  const frontend = manifest.frontend;
  if (frontend === undefined) return;
  if (!frontend || typeof frontend !== "object" || Array.isArray(frontend)) {
    throw new Error("frontend must be an object");
  }
  if (frontend.mode !== "trusted-module") {
    throw new Error('frontend.mode must be "trusted-module"');
  }
  const entry = validateRelativePackagePath(frontend.entry, "frontend.entry", new Set([".mjs", ".js"]));
  requirePackagedEntry(entries, entry, "frontend.entry");
  if (frontend.framework !== undefined && frontend.framework !== "vue3") {
    throw new Error('frontend.framework must be "vue3" when present');
  }
  if (frontend.apiVersion !== undefined && typeof frontend.apiVersion !== "string") {
    throw new Error("frontend.apiVersion must be a string");
  }
}

function validateUiContributions(manifest) {
  const contributions = manifest.contributes?.ui;
  if (contributions === undefined) return;
  if (!Array.isArray(contributions)) throw new Error("contributes.ui must be an array");
  if (contributions.length > 64) throw new Error("a plugin may contribute at most 64 trusted UI entries");
  for (const contribution of contributions) {
    if (!contribution || typeof contribution !== "object" || Array.isArray(contribution)) {
      throw new Error("contributes.ui entries must be objects");
    }
    for (const field of ["id", "slotId", "export"]) {
      if (typeof contribution[field] !== "string" || contribution[field].length === 0) {
        throw new Error(`contributes.ui.${field} must be a non-empty string`);
      }
    }
    if (contribution.order !== undefined && typeof contribution.order !== "number") {
      throw new Error("contributes.ui.order must be a number when present");
    }
    if (contribution.when !== undefined && typeof contribution.when !== "string") {
      throw new Error("contributes.ui.when must be a string when present");
    }
  }
}

function workerEntries(manifest) {
  const entries = [];
  if (Array.isArray(manifest.workers)) {
    entries.push(...manifest.workers.map((worker, index) => ({ worker, label: `workers[${index}]` })));
  }
  if (manifest.engines?.worker?.entrypoint) {
    entries.push({ worker: manifest.engines.worker, label: "engines.worker" });
  }
  return entries;
}

function validateWorkers(manifest, entries) {
  for (const { worker, label } of workerEntries(manifest)) {
    if (!worker || typeof worker !== "object" || Array.isArray(worker)) {
      throw new Error(`${label} must be an object`);
    }
    if ("id" in worker && (typeof worker.id !== "string" || worker.id.length === 0)) {
      throw new Error(`${label}.id must be a non-empty string when present`);
    }
    if (typeof worker.language !== "string" || worker.language.length === 0) {
      throw new Error(`${label}.language must be a non-empty string`);
    }
    if (worker.transport !== "stdio-framed-json-v1") {
      throw new Error(`${label}.transport must be "stdio-framed-json-v1"`);
    }
    const entrypoint = validateRelativePackagePath(
      worker.entrypoint,
      `${label}.entrypoint`,
      new Set([".js", ".mjs", ".py", ".wasm"]),
    );
    requirePackagedEntry(entries, entrypoint, `${label}.entrypoint`);
  }
}

function artifactPathsFrom(value, label) {
  if (value === undefined) return [];
  if (typeof value === "string") return [{ path: value, label }];
  if (Array.isArray(value)) {
    return value.flatMap((entry, index) => artifactPathsFrom(entry, `${label}[${index}]`));
  }
  if (value && typeof value === "object") {
    if (typeof value.path === "string") return [{ path: value.path, label: `${label}.path` }];
    return Object.entries(value).flatMap(([key, nested]) => artifactPathsFrom(nested, `${label}.${key}`));
  }
  throw new Error(`${label} must contain relative artifact paths`);
}

function validateArtifacts(manifest, entries) {
  for (const artifact of artifactPathsFrom(manifest.artifacts, "artifacts")) {
    const relative = validateRelativePackagePath(artifact.path, artifact.label);
    requirePackagedEntry(entries, relative, artifact.label);
  }
}

function validateManifestReferences(manifest, entries) {
  validateFrontendContract(manifest, entries);
  validateUiContributions(manifest);
  validateWorkers(manifest, entries);
  validateArtifacts(manifest, entries);
}

function hasPythonWorker(manifest) {
  return workerEntries(manifest).some(({ worker }) => worker.language === "python");
}

async function addUiArtifacts(manifest, source, entries) {
  const contributions = manifest.contributes?.uiIr;
  if (contributions === undefined) return;
  if (!Array.isArray(contributions)) throw new Error("contributes.uiIr must be a source/artifact reference array");
  if (contributions.length > 16) throw new Error("a plugin may contribute at most 16 UI IR surfaces");
  const { register } = await import("tsx/esm/api");
  register();
  const { compileUiIrSfcArtifact } = await import("../app/plugins/ui-ir-sfc-compiler.ts");
  const byPath = new Map(entries.map((entry) => [entry.relative, entry]));
  const generated = new Set();
  for (const contribution of contributions) {
    if (!contribution || typeof contribution !== "object" || "ir" in contribution) {
      throw new Error("plugin.json UI contributions may not contain inline ir JSON");
    }
    validateUiPath(contribution.source, ".vue", "ui source");
    validateUiPath(contribution.artifact, ".uiir.json", "ui artifact");
    if (typeof contribution.slotId !== "string" || !/^[A-Za-z][A-Za-z0-9._:-]{0,127}$/u.test(contribution.slotId)) {
      throw new Error("ui slotId is invalid");
    }
    const sourceEntry = byPath.get(contribution.source);
    if (!sourceEntry) throw new Error(`UI source is not packaged: ${contribution.source}`);
    if (byPath.has(contribution.artifact) || generated.has(contribution.artifact)) {
      throw new Error(`UI artifact collides with a source entry: ${contribution.artifact}`);
    }
    const artifact = compileUiIrSfcArtifact(sourceEntry.bytes.toString("utf8"), contribution.source);
    const entry = { relative: contribution.artifact, bytes: Buffer.from(artifact, "utf8") };
    entries.push(entry);
    generated.add(entry.relative);
  }
}

async function main() {
  const args = process.argv.slice(2);
  const source = path.resolve(args[0]);
  const destination = path.resolve(args[1]);
  const keyIndex = args.indexOf("--key");
  const keyPath = keyIndex >= 0 ? args[keyIndex + 1] : null;
  if (!source || !destination) {
    console.error("usage: node scripts/pack-plugin.mjs <source-dir> <out.myc> [--key id_ed25519.pem]");
    process.exit(2);
  }

  const manifestPath = path.join(source, "plugin.json");
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  const sourceFiles = walkFiles(source).sort();
  for (const file of sourceFiles) {
    const relative = path.relative(source, file).split(path.sep).join("/");
    const root = relative.split("/")[0];
    if (root in ALLOWED_DIRECTORIES) {
      const segments = relative.split("/");
      const validDepth = root === "src" ? segments.length >= 3 : segments.length === 2;
      if (!validDepth || !ALLOWED_DIRECTORIES[root].has(path.extname(relative).toLowerCase())) {
        console.error(`unsupported package entry: ${relative}`);
        process.exit(1);
      }
      continue;
    }
    if (!ALLOWED_ROOT_FILES.has(root)) {
      console.error(`unsupported package entry: ${relative}`);
      process.exit(1);
    }
  }

  if (manifest.payloads) {
    console.error("plugin.json must not declare payloads; they are build-generated");
    process.exit(1);
  }
  const entries = sourceFiles.map((file) => ({
    relative: path.relative(source, file).split(path.sep).join("/"),
    bytes: readFileSync(file),
  }));
  validateManifestReferences(manifest, entries);
  await addUiArtifacts(manifest, source, entries);
  if (hasPythonWorker(manifest)) {
    const relative = "src/research_canvas.py";
    if (entries.some((entry) => entry.relative === relative)) {
      console.error(`${relative} is reserved for the generated Python SDK vendor`);
      process.exit(1);
    }
    const repositoryRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
    const sdkPath = path.join(repositoryRoot, "plugins", "sdk", "python", "research_canvas.py");
    entries.push({ relative, bytes: readFileSync(sdkPath) });
  }
  entries.sort((left, right) => left.relative.localeCompare(right.relative));

  const payloads = {};
  for (const entry of entries) {
    if (entry.relative === "plugin.json") continue;
    payloads[entry.relative] = createHash("sha256").update(entry.bytes).digest("hex");
  }
  const signed = { ...manifest, payloads };

  if (keyPath) {
    const pem = readFileSync(path.resolve(keyPath), "utf8");
    const privateKey = createPrivateKey({ key: pem, format: "pem", type: "pkcs8" });
    const digest = createHash("sha256").update(canonicalManifestJson(signed)).digest();
    signed.signature = sign(null, digest, privateKey).toString("base64");
  }

  const manifestBytes = Buffer.from(JSON.stringify(signed, null, 2) + "\n", "utf8");

  // Deterministic STORE archive.
  const parts = [];
  const centralParts = [];
  let offset = 0;
  for (const entry of entries) {
    const relative = entry.relative;
    const bytes = relative === "plugin.json" ? manifestBytes : entry.bytes;
    const crc = crc32(bytes);
    parts.push(zipHeader(relative, crc, bytes.length));
    centralParts.push(zipCentral(relative, crc, bytes.length, offset));
    parts.push(bytes);
    offset += 30 + Buffer.byteLength(relative, "utf8") + bytes.length;
  }
  const central = Buffer.concat(centralParts);
  const archive = Buffer.concat([
    ...parts,
    central,
    zipEnd(entries.length, central.length, offset),
  ]);

  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, archive);
  console.log(destination);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
