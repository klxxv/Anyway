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
