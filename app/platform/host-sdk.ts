/** Versioned public envelope used by trusted Vue code and plugin SDK adapters. */
export const HOST_SDK_API_VERSION = "anyway.dev/host-rpc/v1alpha1" as const;

export const DEFAULT_MAX_INLINE_PAYLOAD_BYTES = 64 * 1024;
export const MAX_HOST_DEADLINE_MS = 5 * 60 * 1000;

export type BlobRetentionClass = "request" | "session" | "plugin" | "persistent";

/**
 * A content identity is not an authorization token. The kernel still resolves
 * every reference under the transport-bound principal and active lease.
 */
export interface HostBlobRef {
  algorithm: "sha256";
  digest: string;
  size: number;
  mediaType: string;
  scope: string;
  owner: string;
  retentionClass: BlobRetentionClass;
}
export type HostPayload =
  | { kind: "inline"; value: unknown }
  | { kind: "blob"; ref: HostBlobRef };

/**
 * Deliberately contains no caller principal. The native transport binds the
 * principal to its authenticated session; caller-provided identity is ignored.
 */
export interface HostCallRequest {
  apiVersion: typeof HOST_SDK_API_VERSION;
  requestId: string;
  operation: string;
  payload: HostPayload;
  deadlineMs: number;
  capabilityLeaseIds?: string[];
  traceParent?: string;
}

export interface HostCallResponse<T = unknown> {
  apiVersion: typeof HOST_SDK_API_VERSION;
  requestId: string;
  result?: T;
  error?: HostCallError;
}

export interface HostCallError {
  code: string;
  message: string;
  retryable: boolean;
  details?: unknown;
}

export interface HostCallOptions {
  deadlineMs?: number;
  capabilityLeaseIds?: readonly string[];
  traceParent?: string;
}

/** The concrete Tauri, worker, or test transport owns caller authentication. */
export interface HostSdkTransport {
  invoke<T>(request: HostCallRequest, signal?: AbortSignal): Promise<HostCallResponse<T>>;
}

export class HostPayloadRequiresBlobError extends Error {
  readonly byteLength: number;
  readonly inlineLimit: number;

  constructor(byteLength: number, inlineLimit: number) {
    super(`Host payload is ${byteLength} bytes; the inline limit is ${inlineLimit} bytes`);
    this.name = "HostPayloadRequiresBlobError";
    this.byteLength = byteLength;
    this.inlineLimit = inlineLimit;
  }
}

let fallbackRequestSequence = 0;

function requestId(): string {
  const randomUuid = globalThis.crypto?.randomUUID;
  if (typeof randomUuid === "function") return randomUuid.call(globalThis.crypto);
  fallbackRequestSequence += 1;
  return `local-${Date.now().toString(36)}-${fallbackRequestSequence.toString(36)}`;
}

function validateOperation(operation: string): void {
  if (!/^[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*$/.test(operation) || operation.length > 160) {
    throw new TypeError(`Invalid Host SDK operation: ${operation}`);
  }
}

function validateDeadline(deadlineMs: number): void {
  if (!Number.isInteger(deadlineMs) || deadlineMs <= 0 || deadlineMs > MAX_HOST_DEADLINE_MS) {
    throw new RangeError(`Host SDK deadline must be between 1 and ${MAX_HOST_DEADLINE_MS} ms`);
  }
}

function serializedByteLength(value: unknown): number {
  const json = JSON.stringify(value);
  if (json === undefined) throw new TypeError("Host SDK payload must be JSON serializable");
  return new TextEncoder().encode(json).byteLength;
}

export function assertHostBlobRef(value: unknown): asserts value is HostBlobRef {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new TypeError("BlobRef must be an object");
  }
  const candidate = value as Partial<HostBlobRef>;
  if (candidate.algorithm !== "sha256" || !/^[a-f0-9]{64}$/.test(candidate.digest ?? "")) {
    throw new TypeError("BlobRef must contain a lowercase SHA-256 digest");
  }
  if (!Number.isSafeInteger(candidate.size) || (candidate.size ?? -1) < 0) {
    throw new TypeError("BlobRef size must be a non-negative safe integer");
  }
  for (const [name, field] of [
    ["mediaType", candidate.mediaType],
    ["scope", candidate.scope],
    ["owner", candidate.owner],
  ] as const) {
    if (typeof field !== "string" || field.length === 0 || field.length > 256) {
      throw new TypeError(`BlobRef ${name} must be a bounded non-empty string`);
    }
  }
  if (!(["request", "session", "plugin", "persistent"] as const).includes(candidate.retentionClass as BlobRetentionClass)) {
    throw new TypeError("BlobRef retentionClass is not supported");
  }
}

export class HostSdk {
  readonly #transport: HostSdkTransport;
  readonly #maxInlinePayloadBytes: number;

  constructor(transport: HostSdkTransport, maxInlinePayloadBytes = DEFAULT_MAX_INLINE_PAYLOAD_BYTES) {
    if (!Number.isSafeInteger(maxInlinePayloadBytes) || maxInlinePayloadBytes <= 0) {
      throw new RangeError("maxInlinePayloadBytes must be a positive safe integer");
    }
    this.#transport = transport;
    this.#maxInlinePayloadBytes = maxInlinePayloadBytes;
  }

  async call<T>(
    operation: string,
    value: unknown,
    options: HostCallOptions = {},
    signal?: AbortSignal,
  ): Promise<T> {
    const bytes = serializedByteLength(value);
    if (bytes > this.#maxInlinePayloadBytes) {
      throw new HostPayloadRequiresBlobError(bytes, this.#maxInlinePayloadBytes);
    }
    return this.#invoke<T>(operation, { kind: "inline", value }, options, signal);
  }

  async callWithBlob<T>(
    operation: string,
    ref: HostBlobRef,
    options: HostCallOptions = {},
    signal?: AbortSignal,
  ): Promise<T> {
    assertHostBlobRef(ref);
    return this.#invoke<T>(operation, { kind: "blob", ref }, options, signal);
  }

  async #invoke<T>(
    operation: string,
    payload: HostPayload,
    options: HostCallOptions,
    signal?: AbortSignal,
  ): Promise<T> {
    validateOperation(operation);
    const deadlineMs = options.deadlineMs ?? 30_000;
    validateDeadline(deadlineMs);
    const id = requestId();
    const response = await this.#transport.invoke<T>({
      apiVersion: HOST_SDK_API_VERSION,
      requestId: id,
      operation,
      payload,
      deadlineMs,
      capabilityLeaseIds: options.capabilityLeaseIds ? [...options.capabilityLeaseIds] : undefined,
      traceParent: options.traceParent,
    }, signal);
    if (response.apiVersion !== HOST_SDK_API_VERSION || response.requestId !== id) {
      throw new Error("Host SDK received a mismatched response envelope");
    }
    if (response.error) {
      const error = new Error(response.error.message);
      error.name = response.error.code;
      throw error;
    }
    return response.result as T;
  }
}
