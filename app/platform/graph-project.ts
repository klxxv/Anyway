import type { ProjectState } from "../lib/research-types";
import { isProjectState } from "../lib/project-io";
import {
  DEFAULT_MAX_INLINE_PAYLOAD_BYTES,
  HostSdk,
  type HostBlobRef,
} from "./host-sdk";
import { createDefaultTauriHostSdkTransport } from "./host-sdk-tauri";

export const ANYWAY_GRAPH_PROJECT_COMMITTED_EVENT = "anyway:graph-project-committed" as const;
export const GRAPH_PROJECT_MAX_BYTES = 2 * 1024 * 1024;
export const GRAPH_PROJECT_UPLOAD_CHUNK_BYTES = 16 * 1024;

export interface GraphProjectSyncReceipt {
  readonly projectId: string;
  readonly revision: number;
  readonly bytes?: number;
  readonly syncedAt?: string;
}

export interface GraphProjectSnapshot {
  readonly projectId: string;
  readonly revision: number;
  readonly project: ProjectState;
}

export type GraphProjectCommittedEventDetail = GraphProjectSnapshot;

let graphProjectHostSdk: HostSdk | undefined;

function hostSdk(): HostSdk {
  graphProjectHostSdk ??= new HostSdk(createDefaultTauriHostSdkTransport());
  return graphProjectHostSdk;
}

function serializedJsonBytes(value: unknown): Uint8Array {
  const json = JSON.stringify(value);
  if (json === undefined) throw new Error("GRAPH_PROJECT_PAYLOAD_NOT_SERIALIZABLE");
  return new TextEncoder().encode(json);
}

/** Encode one upload slice as base64; btoa over a binary string is fine for 16KiB chunks. */
function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function validateSnapshot(value: unknown, expectedProjectId?: string, expectedRevision?: number): GraphProjectSnapshot {
  if (!isRecord(value)) throw new Error("GRAPH_PROJECT_SNAPSHOT_INVALID");
  const projectId = value.projectId;
  const revision = value.revision;
  const project = value.project;
  if (typeof projectId !== "string" || !Number.isInteger(revision) || !isProjectState(project)) {
    throw new Error("GRAPH_PROJECT_SNAPSHOT_INVALID");
  }
  if (project.id !== projectId || project.revision !== revision) {
    throw new Error("GRAPH_PROJECT_SNAPSHOT_MISMATCH");
  }
  if (expectedProjectId !== undefined && projectId !== expectedProjectId) {
    throw new Error("GRAPH_PROJECT_ID_MISMATCH");
  }
  if (expectedRevision !== undefined && revision !== expectedRevision) {
    throw new Error("GRAPH_PROJECT_REVISION_MISMATCH");
  }
  return { projectId, revision, project };
}

export async function syncGraphProject(project: ProjectState, signal?: AbortSignal): Promise<GraphProjectSyncReceipt> {
  const sdk = hostSdk();
  const payload = { projectId: project.id, revision: project.revision, project };
  const payloadBytes = serializedJsonBytes(payload);
  if (payloadBytes.byteLength > GRAPH_PROJECT_MAX_BYTES) {
    throw new Error(`GRAPH_PROJECT_TOO_LARGE:${payloadBytes.byteLength}:${GRAPH_PROJECT_MAX_BYTES}`);
  }
  const receipt = payloadBytes.byteLength <= DEFAULT_MAX_INLINE_PAYLOAD_BYTES
    ? await sdk.call<GraphProjectSyncReceipt>("graph.project.sync", payload, {}, signal)
    : await sdk.callWithBlob<GraphProjectSyncReceipt>(
      "graph.project.sync",
      await uploadGraphProjectPayload(sdk, payloadBytes, signal),
      {},
      signal,
    );
  if (!receipt || receipt.projectId !== project.id || receipt.revision !== project.revision) {
    throw new Error("GRAPH_PROJECT_SYNC_RECEIPT_INVALID");
  }
  return receipt;
}

async function uploadGraphProjectPayload(
  sdk: HostSdk,
  payloadBytes: Uint8Array,
  signal?: AbortSignal,
): Promise<HostBlobRef> {
  const { leaseId } = await sdk.call<{ leaseId: string | number }>("blob.upload.begin", {
    scope: "plugin",
    mediaType: "application/json",
    size: payloadBytes.byteLength,
  }, {}, signal);
  for (let offset = 0; offset < payloadBytes.byteLength; offset += GRAPH_PROJECT_UPLOAD_CHUNK_BYTES) {
    const chunk = payloadBytes.subarray(offset, offset + GRAPH_PROJECT_UPLOAD_CHUNK_BYTES);
    await sdk.call("blob.upload.chunk", {
      leaseId,
      contentBase64: bytesToBase64(chunk),
    }, {}, signal);
  }
  return sdk.call<HostBlobRef>("blob.upload.commit", { leaseId }, {}, signal);
}

export async function getGraphProject(
  projectId: string,
  expectedRevision?: number,
  signal?: AbortSignal,
): Promise<GraphProjectSnapshot> {
  const snapshot = await hostSdk().call<unknown>("graph.project.get", {
    projectId,
    expectedRevision,
  }, {}, signal);
  return validateSnapshot(snapshot, projectId, expectedRevision);
}

export function emitGraphProjectCommitted(snapshot: GraphProjectSnapshot): void {
  const detail = validateSnapshot(snapshot, snapshot.projectId, snapshot.revision);
  window.dispatchEvent(new CustomEvent<GraphProjectCommittedEventDetail>(
    ANYWAY_GRAPH_PROJECT_COMMITTED_EVENT,
    { detail },
  ));
}

export function parseGraphProjectCommittedEvent(event: Event): GraphProjectSnapshot | null {
  if (!(event instanceof CustomEvent)) return null;
  try {
    return validateSnapshot(event.detail);
  } catch {
    return null;
  }
}
