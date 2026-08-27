import assert from "node:assert/strict";
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

test("packager accepts the real development tree and excludes frontend toolchain sources", () => {
  const root = mkdtempSync(join(tmpdir(), "anyway-anpdfsolver-dev-tree-"));
  const source = resolve("my-plugins/anPdfsolver");
  const archive = join(root, "myc.pdf-canvas-agent@0.5.0.myc");
  try {
    const built = spawnSync(
      process.execPath,
      [resolve("node_modules/vite/bin/vite.js"), "build", "--config", "frontend/vite.config.ts"],
      { cwd: source, encoding: "utf8" },
    );
    assert.equal(built.status, 0, built.stderr || built.stdout);

    const packed = spawnSync(process.execPath, [resolve("scripts/pack-plugin.mjs"), source, archive], {
      cwd: process.cwd(),
      encoding: "utf8",
    });
    assert.equal(packed.status, 0, packed.stderr || packed.stdout);

    const names = new Set(inspectArchive(archive).names);
    assert.ok(names.has("dist/frontend.mjs"));
    assert.ok(names.has("src/anpdfsolver/worker.py"));
    assert.equal(names.has("package.json"), false);
    assert.equal(names.has("package-lock.json"), false);
    assert.ok([...names].every((name) => !name.startsWith("frontend/")));
    assert.ok([...names].every((name) => !name.includes("node_modules/")));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("packaged anPdfsolver contains trusted frontend, Python worker, and vendored codec", () => {
  const root = mkdtempSync(join(tmpdir(), "anyway-anpdfsolver-package-"));
  const packageSource = join(root, "source");
  const archive = join(root, "myc.pdf-canvas-agent@0.5.0.myc");
  try {
    createBuiltPackageSource(packageSource);
    const packed = spawnSync(process.execPath, [resolve("scripts/pack-plugin.mjs"), packageSource, archive], {
      cwd: process.cwd(),
      encoding: "utf8",
    });
    assert.equal(packed.status, 0, packed.stderr || packed.stdout);

    const inspected = inspectArchive(archive);
    const names = new Set(inspected.names);
    for (const required of [
      "plugin.json",
      "agent-manifest.json",
      "agent.yml",
      "dist/frontend.mjs",
      "src/anpdfsolver/worker.py",
      "src/anpdfsolver/kimi_client.py",
      "src/anpdfsolver/pdf_reader.py",
      "src/anpdfsolver/typed_frames.py",
      "src/research_canvas.py",
      "locales/en.json",
      "locales/zh-CN.json",
      "schemas/graph-patch.schema.json",
      "schemas/pass-a-structure.schema.json",
      "schemas/pass-b-v4.schema.json",
      "schemas/pass-e-v4.schema.json",
      "prompts/typed-ndjson-v1.yaml",
    ]) {
      assert.ok(names.has(required), `${required} must be packaged`);
      if (required !== "plugin.json") {
        assert.ok(required in inspected.manifest.payloads, `${required} must be payload-hashed`);
      }
    }

    assert.ok(inspected.names.every((name) => !name.startsWith("/") && !name.split("/").includes("..")));
    assert.ok(inspected.names.every((name) => !name.includes("__pycache__") && !name.endsWith(".pyc")));
    assert.ok(inspected.names.every((name) => !name.endsWith(".uiir.json")));
    assert.ok(inspected.names.every((name) => !name.startsWith("ui/")));
    assert.ok(inspected.names.every((name) => !name.startsWith("frontend/")));
    assert.ok(inspected.names.every((name) => !/PdfUploadDialog|AgentReviewPanel|pdf-agent-host-slots/u.test(name)));

    assert.equal(inspected.manifest.name, "myc.pdf-canvas-agent");
    assert.equal(inspected.manifest.version, "0.5.0");
    assert.deepEqual(inspected.manifest.frontend, {
      mode: "trusted-module",
      entry: "dist/frontend.mjs",
      framework: "vue3",
      apiVersion: "1",
    });
    assert.deepEqual(inspected.manifest.contributes.ui.map((entry: { slotId: string; export: string }) => [entry.slotId, entry.export]), [
      ["workspace.toolbar.actions", "AnPdfsolverToolbarButton"],
      ["workspace.dialogs", "AnPdfsolverDialog"],
    ]);
    assert.equal("uiIr" in inspected.manifest.contributes, false);
    assert.deepEqual(inspected.manifest.workers, [{
      id: "python-analyzer",
      language: "python",
      entrypoint: "src/anpdfsolver/worker.py",
      transport: "stdio-framed-json-v1",
      operations: ["ping", "health", "anpdfsolver.analyze"],
      hostOperations: ["blob.read"],
      providerEgress: [{
        providerId: "kimi",
        connectionId: "kimi",
        domains: ["api.moonshot.cn", "api.moonshot.ai"],
        purpose: "PDF upload and typed NDJSON response streaming",
        secretEnv: "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY",
      }],
    }]);
    assert.deepEqual(inspected.manifest.network, {
      mode: "direct",
      declaredDomains: ["api.moonshot.cn", "api.moonshot.ai"],
    });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("packaged anPdfsolver worker starts without repository Python imports", () => {
  const root = mkdtempSync(join(tmpdir(), "anyway-packaged-worker-"));
  const packageSource = join(root, "source");
  const archive = join(root, "myc.pdf-canvas-agent@0.5.0.myc");
  try {
    createBuiltPackageSource(packageSource);
    const packed = spawnSync(process.execPath, [resolve("scripts/pack-plugin.mjs"), packageSource, archive], {
      cwd: process.cwd(),
      encoding: "utf8",
    });
    assert.equal(packed.status, 0, packed.stderr || packed.stdout);

    const python = process.env.PYTHON ?? "python";
    const script = String.raw`
import json, os, struct, subprocess, sys, zipfile

archive, root, sdk_source = sys.argv[1], sys.argv[2], sys.argv[3]
unpacked = os.path.join(root, "unpacked")
os.mkdir(unpacked)
with zipfile.ZipFile(archive) as package:
    names = package.namelist()
    assert "dist/frontend.mjs" in names
    assert "src/anpdfsolver/worker.py" in names
    assert "src/research_canvas.py" in names
    assert package.read("src/research_canvas.py") == open(sdk_source, "rb").read()
    assert all(not name.endswith(".uiir.json") for name in names)
    assert all(not name.startswith("ui/") and not name.startswith("frontend/") for name in names)
    package.extractall(unpacked)

def send(stream, value):
    payload = json.dumps(value, separators=(",", ":")).encode("utf-8")
    stream.write(struct.pack(">I", len(payload)) + payload)
    stream.flush()

def receive(stream):
    header = stream.read(4)
    assert len(header) == 4
    size = struct.unpack(">I", header)[0]
    return json.loads(stream.read(size))

environment = {
    "PYTHONUNBUFFERED": "1",
    "PYTHONIOENCODING": "utf-8",
    "ANYWAY_WORKER_TRANSPORT": "stdio-framed-json-v1",
}
entrypoint = os.path.join(unpacked, "src", "anpdfsolver", "worker.py")
process = subprocess.Popen(
    [sys.executable, entrypoint],
    cwd=unpacked,
    env=environment,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
)
try:
    send(process.stdin, {
        "type": "hello",
        "apiVersion": "researchcanvas.dev/worker-rpc/v1",
        "pluginId": "myc.pdf-canvas-agent",
        "pluginVersion": "0.5.0",
        "workerId": "packaged.worker",
        "allowedOperations": ["ping", "health"],
    })
    ack = receive(process.stdout)
    assert ack["type"] == "helloAck"
    assert ack["apiVersion"] == "researchcanvas.dev/worker-rpc/v1"
    assert ack["workerId"] == "packaged.worker"
    assert ack["operations"] == ["health", "ping"]
    send(process.stdin, {
        "type": "request",
        "apiVersion": "researchcanvas.dev/worker-rpc/v1",
        "requestId": "packaged-ping",
        "operation": "ping",
        "payload": {"value": "packaged"},
        "deadlineMs": 1000,
    })
    response = receive(process.stdout)
    assert response["ok"] is True
    assert response["result"]["pong"] == "packaged"
    send(process.stdin, {
        "type": "request",
        "apiVersion": "researchcanvas.dev/worker-rpc/v1",
        "requestId": "packaged-health",
        "operation": "health",
        "payload": {},
        "deadlineMs": 1000,
    })
    health = receive(process.stdout)
    assert health["ok"] is True
    assert health["result"]["healthy"] is True
    assert health["result"]["worker"] == "anPdfsolver"
    assert "anpdfsolver.analyze" in health["result"]["operations"]
    send(process.stdin, {"type": "shutdown", "apiVersion": "researchcanvas.dev/worker-rpc/v1"})
    shutdown = receive(process.stdout)
    assert shutdown["ok"] is True
    assert shutdown["result"]["stopped"] is True
finally:
    process.stdin.close()
    process.wait(timeout=5)
`;
    const environment = { ...process.env };
    delete environment.PYTHONPATH;
    delete environment.PYTHONHOME;
    const runtime = spawnSync(python, ["-c", script, archive, root, resolve("plugins/sdk/python/research_canvas.py")], {
      cwd: root,
      env: environment,
      encoding: "utf8",
      timeout: 15000,
    });
    assert.equal(runtime.status, 0, runtime.stderr || runtime.stdout);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function createBuiltPackageSource(destination: string): void {
  const source = resolve("my-plugins/anPdfsolver");
  mkdirSync(destination, { recursive: true });
  for (const file of [
    "plugin.json",
    "agent-manifest.json",
    "agent.yml",
    "pyproject.toml",
    "README.md",
  ]) {
    cpSync(join(source, file), join(destination, file));
  }
  for (const directory of ["dist", "locales", "prompts", "schemas", "src"]) {
    cpSync(join(source, directory), join(destination, directory), { recursive: true });
  }
  assert.ok(existsSync(join(destination, "dist", "frontend.mjs")));
  assert.equal(existsSync(join(destination, "frontend")), false);
  assert.equal(existsSync(join(destination, "ui")), false);
}

function inspectArchive(archive: string): {
  names: string[];
  manifest: {
    name: string;
    version: string;
    frontend: unknown;
    contributes: { ui: Array<{ slotId: string; export: string }>; uiIr?: unknown };
    workers: unknown;
    network: unknown;
    payloads: Record<string, string>;
  };
} {
  const python = process.env.PYTHON ?? "python";
  const script = "import json, sys, zipfile; z=zipfile.ZipFile(sys.argv[1]); print(json.dumps({'names': z.namelist(), 'manifest': json.loads(z.read('plugin.json'))}))";
  const result = spawnSync(python, ["-c", script, archive], { encoding: "utf8" });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout);
}
