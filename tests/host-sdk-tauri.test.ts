import assert from "node:assert/strict";
import test from "node:test";

import {
  createDefaultTauriHostSdkTransport,
  createTauriHostSdkTransport,
  KERNEL_HOST_CALL_COMMAND,
  TAURI_HOST_UNAVAILABLE_MESSAGE,
  TauriHostUnavailableError,
  type TauriInvokeAdapter,
} from "../app/platform/host-sdk-tauri";
import {
  HOST_SDK_API_VERSION,
  HostSdk,
  type HostCallRequest,
  type HostCallResponse,
} from "../app/platform/host-sdk";

function request(): HostCallRequest {
  return {
    apiVersion: HOST_SDK_API_VERSION,
    requestId: "request-1",
    operation: "plugin.list",
    payload: { kind: "inline", value: {} },
    deadlineMs: 30_000,
  };
}

test("Tauri transport uses one command and a principal-free request envelope", async () => {
  let capturedCommand: string | undefined;
  let capturedArgs: { request: HostCallRequest } | undefined;
  const invokeAdapter: TauriInvokeAdapter = async (command, args) => {
    capturedCommand = command;
    capturedArgs = args;
    return {
      apiVersion: HOST_SDK_API_VERSION,
      requestId: args.request.requestId,
      result: { plugins: [] },
    } satisfies HostCallResponse<{ plugins: never[] }>;
  };

  const transport = createTauriHostSdkTransport(invokeAdapter);
  const response = await transport.invoke<{ plugins: never[] }>(request());

  assert.equal(capturedCommand, KERNEL_HOST_CALL_COMMAND);
  assert.deepEqual(capturedArgs, { request: request() });
  assert.equal("principal" in (capturedArgs?.request ?? {}), false);
  assert.deepEqual(response.result, { plugins: [] });
});

test("Tauri transport passes the kernel response through unchanged", async () => {
  const expected: HostCallResponse<{ accepted: boolean }> = {
    apiVersion: HOST_SDK_API_VERSION,
    requestId: "request-1",
    result: { accepted: true },
  };
  const transport = createTauriHostSdkTransport(async () => expected);

  assert.strictEqual(await transport.invoke(request()), expected);
});

test("HostSdk can use the injected Tauri transport", async () => {
  const transport = createTauriHostSdkTransport(async (_command, args) => ({
    apiVersion: HOST_SDK_API_VERSION,
    requestId: args.request.requestId,
    result: { accepted: true },
  }));

  assert.deepEqual(await new HostSdk(transport).call("plugin.list", {}), { accepted: true });
});

test("Tauri transport rejects before invoking when the signal is already aborted", async () => {
  let invoked = false;
  const transport = createTauriHostSdkTransport(async () => {
    invoked = true;
    return {
      apiVersion: HOST_SDK_API_VERSION,
      requestId: "request-1",
      result: null,
    };
  });
  const controller = new AbortController();
  controller.abort();

  await assert.rejects(
    transport.invoke(request(), controller.signal),
    (error: unknown) => error instanceof DOMException && error.name === "AbortError",
  );
  assert.equal(invoked, false);
});

test("Tauri transport locally aborts an in-flight call and consumes its late rejection", async () => {
  let rejectInvoke: ((error: Error) => void) | undefined;
  const invokeSettled = new Promise<HostCallResponse<unknown>>((_, reject) => {
    rejectInvoke = reject;
  });
  const transport = createTauriHostSdkTransport(async () => invokeSettled);
  const controller = new AbortController();
  const pending = transport.invoke(request(), controller.signal);

  controller.abort();
  await assert.rejects(
    pending,
    (error: unknown) => error instanceof DOMException && error.name === "AbortError",
  );
  rejectInvoke?.(new Error("late Tauri failure"));
  await new Promise((resolve) => setImmediate(resolve));
});

test("default Tauri transport gives a stable error outside Tauri", async () => {
  const transport = createDefaultTauriHostSdkTransport();

  await assert.rejects(
    transport.invoke(request()),
    (error: unknown) =>
      error instanceof TauriHostUnavailableError &&
      error.code === "TAURI_HOST_UNAVAILABLE" &&
      error.message === TAURI_HOST_UNAVAILABLE_MESSAGE,
  );
});
