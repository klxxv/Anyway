import {
  type HostCallRequest,
  type HostCallResponse,
  type HostSdkTransport,
} from "./host-sdk";

export const KERNEL_HOST_CALL_COMMAND = "kernel_host_call" as const;
export const KERNEL_HOST_CANCEL_COMMAND = "kernel_host_cancel" as const;
export const TAURI_HOST_UNAVAILABLE_MESSAGE = "Anyway Host SDK requires a Tauri runtime";

export type TauriInvokeAdapter = (
  command: typeof KERNEL_HOST_CALL_COMMAND,
  args: { request: HostCallRequest },
) => Promise<unknown>;

export type TauriCancelAdapter = (requestId: string) => Promise<unknown>;

export class TauriHostUnavailableError extends Error {
  readonly code = "TAURI_HOST_UNAVAILABLE";

  constructor() {
    super(TAURI_HOST_UNAVAILABLE_MESSAGE);
    this.name = "TauriHostUnavailableError";
  }
}

function abortError(): Error {
  if (typeof DOMException === "function") {
    return new DOMException("The operation was aborted", "AbortError");
  }
  const error = new Error("The operation was aborted");
  error.name = "AbortError";
  return error;
}

function isTauriRuntime(): boolean {
  const candidate = globalThis as typeof globalThis & {
    __TAURI_INTERNALS__?: unknown;
  };
  return candidate.__TAURI_INTERNALS__ !== undefined;
}

function invokeWithAbort<T>(
  invokeAdapter: TauriInvokeAdapter,
  cancelAdapter: TauriCancelAdapter | undefined,
  request: HostCallRequest,
  signal?: AbortSignal,
): Promise<HostCallResponse<T>> {
  if (!signal) {
    return Promise.resolve()
      .then(() => invokeAdapter(KERNEL_HOST_CALL_COMMAND, { request }))
      .then((response) => response as HostCallResponse<T>);
  }
  if (signal.aborted) return Promise.reject(abortError());

  let removeAbortListener: () => void = () => undefined;
  const abortPromise = new Promise<never>((_, reject) => {
    const onAbort = () => {
      void cancelAdapter?.(request.requestId).catch(() => undefined);
      reject(abortError());
    };
    signal.addEventListener("abort", onAbort, { once: true });
    removeAbortListener = () => signal.removeEventListener("abort", onAbort);
  });
  const invokePromise = Promise.resolve()
    .then(() => invokeAdapter(KERNEL_HOST_CALL_COMMAND, { request }))
    .then((response) => response as HostCallResponse<T>);

  // The underlying Tauri promise cannot be cancelled. Consume its eventual
  // rejection even when the local caller has already observed AbortError.
  void invokePromise.catch(() => undefined);
  return Promise.race([invokePromise, abortPromise]).finally(removeAbortListener);
}

export function createTauriHostSdkTransport(invokeAdapter: TauriInvokeAdapter, cancelAdapter?: TauriCancelAdapter): HostSdkTransport {
  return {
    invoke<T>(request: HostCallRequest, signal?: AbortSignal) {
      return invokeWithAbort<T>(invokeAdapter, cancelAdapter, request, signal);
    },
  };
}

async function defaultInvokeAdapter<T>(
  command: typeof KERNEL_HOST_CALL_COMMAND,
  args: { request: HostCallRequest },
): Promise<T> {
  if (!isTauriRuntime()) throw new TauriHostUnavailableError();

  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

async function defaultCancelAdapter(requestId: string): Promise<unknown> {
  if (!isTauriRuntime()) throw new TauriHostUnavailableError();
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke(KERNEL_HOST_CANCEL_COMMAND, { requestId });
}

export function createDefaultTauriHostSdkTransport(): HostSdkTransport {
  return createTauriHostSdkTransport(defaultInvokeAdapter, defaultCancelAdapter);
}
