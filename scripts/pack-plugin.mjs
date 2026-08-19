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
  "README.md",
  "LICENSE",
]);
const ALLOWED_DIRECTORIES = {
  locales: new Set([".json"]),
  prompts: new Set([".yaml", ".yml"]),
  schemas: new Set([".json"]),
};

function walkFiles(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const full = path.join(dir, name);
    const stat = statSync(full);
    if (stat.isDirectory()) out.push(...walkFiles(full));
    else out.push(full);
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

function main() {
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
  const files = walkFiles(source).sort();
  for (const file of files) {
    const relative = path.relative(source, file).split(path.sep).join("/");
    const root = relative.split("/")[0];
    if (root in ALLOWED_DIRECTORIES) {
      if (relative.split("/").length !== 2 || !ALLOWED_DIRECTORIES[root].has(path.extname(relative).toLowerCase())) {
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

  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));
  if (manifest.payloads) {
    console.error("plugin.json must not declare payloads; they are build-generated");
    process.exit(1);
  }
  const payloads = {};
  for (const file of files) {
    const relative = path.relative(source, file).split(path.sep).join("/");
    if (relative === "plugin.json") continue;
    payloads[relative] = createHash("sha256").update(readFileSync(file)).digest("hex");
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
  for (const file of files) {
    const relative = path.relative(source, file).split(path.sep).join("/");
    const bytes = relative === "plugin.json" ? manifestBytes : readFileSync(file);
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
    zipEnd(files.length, central.length, offset),
  ]);

  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, archive);
  console.log(destination);
}

main();
