import { shallowRef } from "vue";

export type BlobRef = {
  algorithm: "sha256";
  digest: string;
  size: number;
  mediaType: string;
  scope: string;
  owner: string;
  retentionClass: "request" | "session" | "plugin" | "persistent";
};

export type PickedFile = {
  id?: string;
  label: string;
  blobRef: BlobRef;
};

export type WorkerHandle = {
  pluginId: string;
  pluginVersion: string;
  workerId: string;
  sessionId: string;
  fingerprint?: string;
  transport?: "stdio-framed-json-v1";
  language?: string;
};

export type TrackedWorker = {
  handle: WorkerHandle;
  sessionId: string;
  workerId: string;
};

export type HostContext = {
  workspace: {
    projectId: string;
    baseRevision: number;
  };
};

export type RuntimeProvider = {
  id: "kimi";
  baseUrl: string;
  format: "openai" | "anthropic";
  model: string;
  pdfTransport: "local-text" | "kimi-file-extract";
  thinking: "enabled" | "disabled";
  publicProgress: "enabled" | "disabled";
  allowedDomains: string[];
  secretEnv: "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY";
};

export type PluginContext = {
  plugin: { id: string; version: string };
  files: {
    pick(options?: { accept?: string[]; multiple?: boolean; retention?: "session" | "plugin" }): Promise<PickedFile[]>;
  };
  worker: {
    open(workerId: string, options: { sessionId: string; deadlineMs?: number }): Promise<WorkerHandle>;
    call<T = unknown>(handle: WorkerHandle, request: {
      requestId: string;
      operation: string;
      payload?: Record<string, unknown>;
      deadlineMs?: number;
    }): Promise<T>;
    cancel(handle: WorkerHandle, requestId: string): Promise<boolean>;
    close(handle: WorkerHandle): Promise<void>;
  };
  graphPatch: {
    propose(request: {
      sessionId: string;
      projectId: string;
      baseRevision: number;
      patch: Record<string, unknown>;
    }): Promise<{ proposalId: string; digest: string }>;
    get(request: {
      sessionId: string;
      projectId: string;
      baseRevision: number;
      proposalId: string;
      expectedDigest: string;
    }): Promise<CanonicalGraphPatchReview>;
    review(request: {
      sessionId: string;
      projectId: string;
      baseRevision: number;
      proposalId: string;
      expectedDigest: string;
      accept: boolean;
    }): Promise<unknown>;
    cleanupSession(sessionId: string): Promise<number>;
  };
  settings?: {
    read(): Promise<Record<string, unknown>> | Record<string, unknown>;
    write(values: Record<string, unknown>): Promise<void> | void;
  };
  logger?: {
    info(message: string, fields?: Record<string, unknown>): void;
    warn(message: string, fields?: Record<string, unknown>): void;
    error(message: string, fields?: Record<string, unknown>): void;
  };
};

export type CanonicalGraphPatchReview = {
  proposalId: string;
  digest: string;
  projectId?: string;
  baseRevision?: number;
  title?: string;
  summary?: string;
  operations?: Array<Record<string, unknown>>;
  items?: Array<{ id: string; title: string; summary?: string; operation?: Record<string, unknown> }>;
  review?: {
    title?: string;
    summary?: string;
    operations?: Array<Record<string, unknown>>;
  };
};

const activeContext = shallowRef<PluginContext | null>(null);
const workerHandles = new Map<string, TrackedWorker>();

export function setPluginContext(context: PluginContext): void {
  activeContext.value = context;
}

export function getPluginContext(): PluginContext {
  if (!activeContext.value) {
    throw new Error("anPdfsolver plugin context has not been activated");
  }
  return activeContext.value;
}

export function trackWorker(worker: TrackedWorker): TrackedWorker {
  workerHandles.set(worker.sessionId, worker);
  return worker;
}

export async function closeTrackedWorkers(): Promise<void> {
  const context = activeContext.value;
  if (!context) return;
  const handles = [...workerHandles];
  workerHandles.clear();
  await Promise.allSettled([
    ...handles.map(([, worker]) => context.worker.close(worker.handle)),
    ...handles.map(([, worker]) => context.graphPatch.cleanupSession(worker.sessionId))
  ]);
}

export function clearPluginContext(): void {
  activeContext.value = null;
}

export async function readSetting<T>(id: string, fallback: T): Promise<T> {
  const context = getPluginContext();
  const settings = await context.settings?.read();
  const value = settings?.[id] as T | undefined;
  return value === undefined || value === null ? fallback : value;
}
