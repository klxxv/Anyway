import assert from "node:assert/strict";
import test from "node:test";

import {
  HostPayloadRequiresBlobError,
  HostSdk,
  HOST_SDK_API_VERSION,
  assertHostBlobRef,
  type HostCallRequest,
  type HostSdkTransport,
} from "../app/platform/host-sdk";

class RecordingTransport implements HostSdkTransport {
  request: HostCallRequest | undefined;

  async invoke<T>(request: HostCallRequest) {
    this.request = request;
    return {
      apiVersion: HOST_SDK_API_VERSION,
      requestId: request.requestId,
      result: { accepted: true } as T,
    };
  }
}

test("HostSdk emits a principal-free, versioned envelope", async () => {
  const transport = new RecordingTransport();
  const sdk = new HostSdk(transport);

  assert.deepEqual(await sdk.call("graph.compile", { revision: 7 }), { accepted: true });
  assert.equal(transport.request?.apiVersion, HOST_SDK_API_VERSION);
  assert.equal(transport.request?.operation, "graph.compile");
  assert.equal("principal" in (transport.request ?? {}), false);
});
test("plugin.settings.read emits a valid operation and identity payload", async () => {
  const transport = new RecordingTransport();
  const sdk = new HostSdk(transport);

  const payload = { pluginId: "myc.onedarkpro", pluginVersion: "1.3.0" };
  assert.deepEqual(await sdk.call("plugin.settings.read", payload), { accepted: true });

  const request = transport.request;
  assert.ok(request, "Host SDK emitted a request");
  assert.equal(request.operation, "plugin.settings.read");
  assert.match(request.operation, /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/);
  assert.equal(request.payload.kind, "inline");
  if (request.payload.kind !== "inline") {
    assert.fail("settings read must use an inline payload");
    return;
  }
  assert.deepEqual(Object.keys(request.payload.value as Record<string, unknown>).sort(), [
    "pluginId",
    "pluginVersion",
  ]);
  assert.deepEqual(request.payload.value, payload);
});
test("workspace operations emit valid names and capability-free inline payloads", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "workspace.folder.list",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        root: "/workspace",
        path: "src",
      },
      expectedKeys: ["path", "pluginId", "pluginVersion", "root"],
    },
    {
      operation: "workspace.git.read",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        path: "/workspace",
      },
      expectedKeys: ["path", "pluginId", "pluginVersion"],
    },
    {
      operation: "workspace.github.read",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
      },
      expectedKeys: ["pluginId", "pluginVersion"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("agent and icon-theme operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "plugin.icon-theme.read",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        assetPath: "icons/theme.json",
      },
      expectedKeys: ["assetPath", "pluginId", "pluginVersion"],
    },
    {
      operation: "agent.job.status",
      payload: { jobId: "job-1" },
      expectedKeys: ["jobId"],
    },
    {
      operation: "agent.job.list",
      payload: {},
      expectedKeys: [],
    },
    {
      operation: "agent.batch.status",
      payload: { batchId: "batch-1" },
      expectedKeys: ["batchId"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("agent job review and cancel operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "agent.job.review",
      payload: { jobId: "job-1", accept: true },
      expectedKeys: ["accept", "jobId"],
    },
    {
      operation: "agent.job.cancel",
      payload: { jobId: "job-1" },
      expectedKeys: ["jobId"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("agent job start and batch start operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "agent.job.start",
      payload: { pdfPath: "/tmp/example.pdf" },
      expectedKeys: ["pdfPath"],
    },
    {
      operation: "agent.batch.start",
      payload: { paths: ["/tmp/a.pdf", "/tmp/b.pdf"] },
      expectedKeys: ["paths"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("plugin settings write and reset operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "plugin.settings.write",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        values: { theme: "dark", retries: 3 },
      },
      expectedKeys: ["pluginId", "pluginVersion", "values"],
    },
    {
      operation: "plugin.settings.reset",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
      },
      expectedKeys: ["pluginId", "pluginVersion"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("plugin install/uninstall and vsix import operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "plugin.install",
      payload: { path: "/tmp/example.myc" },
      expectedKeys: ["path"],
    },
    {
      operation: "plugin.uninstall",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
      },
      expectedKeys: ["pluginId", "pluginVersion"],
    },
    {
      operation: "plugin.vsix.import",
      payload: { path: "/tmp/theme.vsix" },
      expectedKeys: ["path"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("project persistence operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "project.save",
      payload: {
        path: "/tmp/project.mycproj",
        project: { schemaVersion: 2, title: "PINN architecture" },
      },
      expectedKeys: ["path", "project"],
    },
    {
      operation: "project.import",
      payload: { path: "/tmp/project.mycproj" },
      expectedKeys: ["path"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("workspace write operations emit valid names and capability-free inline payloads", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "workspace.folder.scan",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        path: "/workspace",
      },
      expectedKeys: ["path", "pluginId", "pluginVersion"],
    },
    {
      operation: "workspace.git.init",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        path: "/workspace",
      },
      expectedKeys: ["path", "pluginId", "pluginVersion"],
    },
    {
      operation: "workspace.github.ssh.generate",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        comment: "research@canvas",
      },
      expectedKeys: ["comment", "pluginId", "pluginVersion"],
    },
    {
      operation: "workspace.git.autosave",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        repoPath: "/workspace",
        projectPath: ".research-canvas/pinn.mycproj",
        project: { schemaVersion: 2, title: "PINN architecture" },
        message: "Research Canvas autosave",
      },
      expectedKeys: [
        "message",
        "pluginId",
        "pluginVersion",
        "project",
        "projectPath",
        "repoPath",
      ],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("workspace github login and ssh upload operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "workspace.github.login",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
      },
      expectedKeys: ["pluginId", "pluginVersion"],
    },
    {
      operation: "workspace.github.ssh.upload",
      payload: {
        pluginId: "myc.onedarkpro",
        pluginVersion: "1.3.0",
        path: "/home/user/.ssh/id_ed25519.pub",
      },
      expectedKeys: ["path", "pluginId", "pluginVersion"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("plugin.connection.test emits a valid name and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const payload = {
    pluginId: "myc.onedarkpro",
    pluginVersion: "1.3.0",
    connectionId: "openai",
    actionId: "test-pdf-extraction",
    values: { baseUrl: "https://api.example.com" },
    secrets: { apiKey: { action: "set", value: "sk-secret" } },
  };

  const transport = new RecordingTransport();
  const sdk = new HostSdk(transport);

  assert.deepEqual(await sdk.call("plugin.connection.test", payload), { accepted: true });

  const request = transport.request;
  assert.ok(request, "Host SDK emitted a request");
  assert.equal(request.operation, "plugin.connection.test");
  assert.match(request.operation, operationNames);
  assert.equal(request.payload.kind, "inline");
  if (request.payload.kind !== "inline") {
    assert.fail("plugin.connection.test must use an inline payload");
    return;
  }
  const value = request.payload.value as Record<string, unknown>;
  assert.deepEqual(Object.keys(value).sort(), [
    "actionId",
    "connectionId",
    "pluginId",
    "pluginVersion",
    "secrets",
    "values",
  ]);
  assert.equal("capability" in value, false, "the legacy capability field must not be forwarded");
  assert.deepEqual(request.payload.value, payload);
});
test("graph and plugin analysis operations emit valid names and exact payload keys", async () => {
  const operationNames = /^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/;
  const cases: Array<{
    operation: string;
    payload: Record<string, unknown>;
    expectedKeys: string[];
  }> = [
    {
      operation: "graph.compile",
      payload: {
        project: { schemaVersion: 3, title: "PINN architecture", nodes: [], edges: [] },
      },
      expectedKeys: ["project"],
    },
    {
      operation: "graph.diff",
      payload: {
        v1: { schemaVersion: 3, nodes: [], edges: [] },
        v2: { schemaVersion: 3, nodes: [], edges: [] },
      },
      expectedKeys: ["v1", "v2"],
    },
    {
      operation: "plugin.analysis.run",
      payload: {
        pluginId: "myc.example",
        pluginVersion: "1.0.0",
        capability: "analysis.run",
        input: { apiVersion: "researchcanvas.dev/plugin-call/v1alpha1" },
      },
      expectedKeys: ["capability", "input", "pluginId", "pluginVersion"],
    },
  ];

  for (const testCase of cases) {
    const transport = new RecordingTransport();
    const sdk = new HostSdk(transport);

    assert.deepEqual(await sdk.call(testCase.operation, testCase.payload), { accepted: true });

    const request = transport.request;
    assert.ok(request, "Host SDK emitted a request");
    assert.equal(request.operation, testCase.operation);
    assert.match(request.operation, operationNames);
    assert.equal(request.payload.kind, "inline");
    if (request.payload.kind !== "inline") {
      assert.fail(`${testCase.operation} must use an inline payload`);
      return;
    }
    const value = request.payload.value as Record<string, unknown>;
    assert.deepEqual(Object.keys(value).sort(), testCase.expectedKeys);
    assert.deepEqual(request.payload.value, testCase.payload);
  }
});
test("HostSdk requires BlobRef for oversized values", async () => {
  const sdk = new HostSdk(new RecordingTransport(), 32);
  await assert.rejects(
    sdk.call("blob.consume", { text: "x".repeat(64) }),
    HostPayloadRequiresBlobError,
  );
});

test("HostSdk rejects unbounded deadlines and invalid operation names", async () => {
  const sdk = new HostSdk(new RecordingTransport());
  await assert.rejects(sdk.call("../unsafe", {}), TypeError);
  await assert.rejects(sdk.call("graph.compile", {}, { deadlineMs: 0 }), RangeError);
});

test("BlobRef validation separates content identity from a bounded scope", () => {
  assert.doesNotThrow(() => assertHostBlobRef({
    algorithm: "sha256",
    digest: "a".repeat(64),
    size: 4096,
    mediaType: "application/pdf",
    scope: "workspace:example",
    owner: "plugin:example",
    retentionClass: "request",
  }));
  assert.throws(() => assertHostBlobRef({
    algorithm: "sha256",
    digest: "not-a-digest",
    size: 1,
    mediaType: "text/plain",
    scope: "workspace:example",
    owner: "plugin:example",
    retentionClass: "request",
  }));
});
