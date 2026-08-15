/**
 * AnCordis <-> Anyway Kernel Host Bus protocol sketch.
 *
 * This file intentionally contains only serializable TypeScript types. It is
 * not imported by the current application SDK and does not add a runtime
 * dependency. The Kernel remains the authority that validates every message.
 */

export const ANCORDIS_API_VERSION = "anyway.dev/ancordis/v1alpha1" as const;
export const HOST_BUS_API_VERSION = "anyway.dev/host-bus/v1alpha1" as const;

export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };

export type PrincipalKind =
  | "user"
  | "kernel"
  | "system-extension"
  | "plugin"
  | "worker"
  | "event-source";

export interface PrincipalRef {
  id: string;
  kind: PrincipalKind;
  /** Stable package identity; never inferred from the immediate caller. */
  packageId?: string;
  packageVersion?: string;
}

/**
 * The Kernel creates this context. AnCordis may append to delegationChain but
 * must never replace originalPrincipal or remove a capability grant.
 */
export interface SecurityContext {
  originalPrincipal: PrincipalRef;
  immediateCaller: PrincipalRef;
  delegationChain: PrincipalRef[];
  capabilityGrants: CapabilityGrantRef[];
  noCapabilityEscalation: true;
}

export interface CapabilityGrantRef {
  id: string;
  capability: string;
  resourceScope?: string;
  expiresAt: string;
  issuer: PrincipalRef;
}

export interface SchemaRef {
  kind: "json-schema";
  uri: string;
  sha256: string;
}

/** Large values travel through BlobStore; RPC carries only this reference. */
export interface BlobRef {
  blobId: string;
  sha256: string;
  sizeBytes: number;
  mediaType: string;
  createdAt: string;
  expiresAt?: string;
  access: "read" | "write-once";
}

export type ExecutionMode = "trusted-in-process" | "isolated-process";

export interface ServiceMethodDescriptor {
  name: string;
  input: SchemaRef;
  output: SchemaRef;
  streaming?: "none" | "server" | "bidirectional";
  idempotent?: boolean;
  maxPayloadBytes?: number;
}

/** Service descriptors contain schemas and policy, never function values. */
export interface ServiceDescriptor {
  serviceId: string;
  version: string;
  displayName: string;
  providerPrincipal: PrincipalRef;
  methods: ServiceMethodDescriptor[];
  events?: EventTypeDescriptor[];
  requiredCapabilities: string[];
  executionMode: ExecutionMode;
}

export interface EventTypeDescriptor {
  topic: string;
  payload: SchemaRef;
  replayable?: boolean;
}

export interface RegistrationLease {
  leaseId: string;
  ownerPrincipal: PrincipalRef;
  resourceType: "service" | "event-subscription" | "worker";
  resourceId: string;
  issuedAt: string;
  expiresAt: string;
  renewAfterMs: number;
  generation: number;
  revocable: true;
}

export interface ServiceRegisterRequest {
  apiVersion: typeof HOST_BUS_API_VERSION;
  service: ServiceDescriptor;
  requestedLeaseMs: number;
  security: SecurityContext;
}

export interface ServiceRegisterResponse {
  serviceId: string;
  lease: RegistrationLease;
  acceptedCapabilities: string[];
}

export interface LeaseRenewRequest {
  apiVersion: typeof HOST_BUS_API_VERSION;
  leaseId: string;
  requestedLeaseMs: number;
  security: SecurityContext;
}

export interface LeaseRenewResponse {
  lease: RegistrationLease;
}

export interface EventSubscriptionRequest {
  apiVersion: typeof HOST_BUS_API_VERSION;
  topic: string;
  payload: SchemaRef;
  filter?: JsonValue;
  delivery: "serial" | "parallel" | "latest";
  maxInFlight: number;
  ack: "none" | "required";
  requestedLeaseMs: number;
  security: SecurityContext;
}

export interface EventSubscriptionResponse {
  subscriptionId: string;
  lease: RegistrationLease;
}

export interface EventEnvelope {
  apiVersion: typeof HOST_BUS_API_VERSION;
  eventId: string;
  topic: string;
  sequence?: number;
  emittedAt: string;
  security: SecurityContext;
  payload?: JsonValue;
  blobs?: BlobRef[];
}

export interface WorkerResourceLimits {
  maxCpuMs: number;
  maxMemoryBytes: number;
  maxInFlightRpc: number;
  maxOutputBytes: number;
}

export interface WorkerSpawnRequest {
  apiVersion: typeof HOST_BUS_API_VERSION;
  workerId: string;
  packageId: string;
  packageVersion: string;
  language: "typescript" | "javascript" | "rust" | "go" | "python" | "cpp" | "other";
  mode: "isolated-process";
  artifact: BlobRef;
  entrypoint: string;
  requestedCapabilities: string[];
  resourceLimits: WorkerResourceLimits;
  requestedLeaseMs: number;
  security: SecurityContext;
}

export interface WorkerSpawnResponse {
  workerId: string;
  workerPrincipal: PrincipalRef;
  lease: RegistrationLease;
  channelId: string;
  acceptedCapabilities: string[];
}

export interface WorkerStopRequest {
  apiVersion: typeof HOST_BUS_API_VERSION;
  workerId: string;
  reason: "shutdown" | "lease-expired" | "cancelled" | "faulted" | "revoked";
  graceMs: number;
  security: SecurityContext;
}

export interface WorkerStopResponse {
  workerId: string;
  stopped: boolean;
  forced: boolean;
}

export interface ServiceCallRequest {
  apiVersion: typeof HOST_BUS_API_VERSION;
  serviceId: string;
  serviceVersion: string;
  method: string;
  args?: JsonValue;
  blobs?: BlobRef[];
  security: SecurityContext;
}

export interface ServiceCallResponse {
  requestId: string;
  result?: JsonValue;
  blobs?: BlobRef[];
  error?: RpcError;
}

export interface RpcError {
  code:
    | "invalid-request"
    | "unauthenticated"
    | "forbidden"
    | "lease-expired"
    | "not-found"
    | "schema-mismatch"
    | "quota-exceeded"
    | "worker-unavailable"
    | "cancelled"
    | "internal";
  message: string;
  retryable: boolean;
}

export type HostBusRequest =
  | ServiceRegisterRequest
  | LeaseRenewRequest
  | EventSubscriptionRequest
  | WorkerSpawnRequest
  | WorkerStopRequest
  | ServiceCallRequest;

export interface HostBusResponseEnvelope<T = unknown> {
  apiVersion: typeof HOST_BUS_API_VERSION;
  requestId: string;
  ok: boolean;
  response?: T;
  error?: RpcError;
}

export interface VueIrBinding {
  kind: "binding";
  path: string;
  mode: "read" | "write";
}

export interface VueIrEventAction {
  event: string;
  method: string;
  payload?: JsonValue;
}

/**
 * Host-owned Vue IR. It deliberately has no HTML string, component function,
 * JavaScript expression, slot callback, or renderer reference.
 */
export interface VueIrNode {
  type: string;
  key?: string;
  props?: Record<string, JsonValue | BlobRef | VueIrBinding>;
  children?: VueIrNode[];
  events?: VueIrEventAction[];
}

export interface VueIrContribution {
  contributionId: string;
  pluginId: string;
  slotId: string;
  node: VueIrNode;
  allowedEvents: string[];
  security: SecurityContext;
}
