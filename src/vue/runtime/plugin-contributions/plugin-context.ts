import * as VueRuntime from "vue";
import { HostSdk } from "../../../../app/platform/host-sdk";
import { createDefaultTauriHostSdkTransport } from "../../../../app/platform/host-sdk-tauri";
import {
  emitGraphProjectCommitted,
  getGraphProject,
} from "../../../../app/platform/graph-project";
import { hostSlotRegistry } from "../../../../app/plugins/host-slot-registry";
import {
  PLUGIN_FRONTEND_CONTEXT_API_VERSION,
  type MountedSlotInstance,
  type PluginContext,
  type PluginFrontendPluginIdentity,
  type PluginGraphPatchProposalRequest,
  type PluginGraphPatchReviewRequest,
  type PluginFilePickOptions,
  type PluginPickedFile,
  type PluginWorkerCallOptions,
  type PluginWorkerHandle,
  type PluginWorkerOpenOptions,
} from "../../../../app/plugins/plugin-frontend-contract";
import type { InstalledMycPlugin } from "../../../../app/plugins/contracts";
import type { HostSlotRegistry } from "../../../../app/plugins/host-slot-registry";

let singletonHostSdk: HostSdk | undefined;

function hostSdk(): HostSdk {
  singletonHostSdk ??= new HostSdk(createDefaultTauriHostSdkTransport());
  return singletonHostSdk;
}

function pluginIdentity(plugin: InstalledMycPlugin): PluginFrontendPluginIdentity {
  return {
    id: plugin.manifest.metadata.id,
    version: plugin.manifest.metadata.version,
    name: plugin.manifest.metadata.name,
    installPath: plugin.installPath,
  };
}

function withPluginIdentity(plugin: PluginFrontendPluginIdentity, payload: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    pluginId: plugin.id,
    pluginVersion: plugin.version,
    ...payload,
  };
}

type GraphPatchReviewReceipt = {
  readonly status?: unknown;
  readonly newRevision?: unknown;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function acceptedRevision(receipt: unknown): number | null {
  if (!isRecord(receipt)) return null;
  const candidate = receipt as GraphPatchReviewReceipt;
  const revision = candidate?.newRevision;
  if (
    candidate.status !== "accepted" ||
    typeof revision !== "number" ||
    !Number.isSafeInteger(revision) ||
    revision < 0
  ) {
    return null;
  }
  return revision;
}

export function createPluginFrontendContext(
  plugin: InstalledMycPlugin,
  registry: HostSlotRegistry = hostSlotRegistry,
): PluginContext {
  const sdk = hostSdk();
  const identity = pluginIdentity(plugin);
  const call = sdk.call.bind(sdk);

  return Object.freeze({
    apiVersion: PLUGIN_FRONTEND_CONTEXT_API_VERSION,
    plugin: identity,
    vue: VueRuntime,
    worker: Object.freeze({
      open: (workerId: string, options: PluginWorkerOpenOptions, signal?: AbortSignal) =>
        call<PluginWorkerHandle>("plugin.worker.open", withPluginIdentity(identity, {
          workerId,
          sessionId: options.sessionId,
          deadlineMs: options.deadlineMs,
        }), { deadlineMs: options.deadlineMs }, signal),
      call: async <T = unknown>(handle: PluginWorkerHandle, options: PluginWorkerCallOptions, signal?: AbortSignal) => {
        const response = await call<{ result: T }>("plugin.worker.call", {
          sessionId: handle.sessionId,
          requestId: options.requestId,
          operation: options.operation,
          payload: options.payload,
          deadlineMs: options.deadlineMs,
        }, { deadlineMs: options.deadlineMs }, signal);
        return response.result;
      },
      cancel: async (handle: PluginWorkerHandle, requestId: string, signal?: AbortSignal) => {
        const response = await call<{ cancelled: boolean }>("plugin.worker.cancel", {
          sessionId: handle.sessionId,
          requestId,
        }, {}, signal);
        return response.cancelled;
      },
      close: (handle: PluginWorkerHandle, signal?: AbortSignal) =>
        call<void>("plugin.worker.close", { sessionId: handle.sessionId }, {}, signal),
    }),
    graphPatch: Object.freeze({
      propose: <T = unknown>(request: PluginGraphPatchProposalRequest, signal?: AbortSignal) =>
        call<T>("graph.patch.propose", withPluginIdentity(identity, { ...request }), {}, signal),
      get: <T = unknown>(request: PluginGraphPatchReviewRequest, signal?: AbortSignal) =>
        call<T>("graph.patch.get", withPluginIdentity(identity, { ...request }), {}, signal),
      review: async <T = unknown>(request: PluginGraphPatchReviewRequest & { readonly accept: boolean }, signal?: AbortSignal) => {
        const receipt = await call<T>("graph.patch.review", withPluginIdentity(identity, { ...request }), {}, signal);
        const newRevision = acceptedRevision(receipt);
        if (newRevision !== null) {
          const snapshot = await getGraphProject(request.projectId, newRevision, signal);
          emitGraphProjectCommitted(snapshot);
        }
        return receipt;
      },
      cleanupSession: async (sessionId: string, signal?: AbortSignal) => {
        const response = await call<{ removed: number }>("graph.patch.cleanup-session", withPluginIdentity(identity, {
          sessionId,
        }), {}, signal);
        return response.removed;
      },
    }),
    files: Object.freeze({
      pick: (options: PluginFilePickOptions = {}, signal?: AbortSignal) =>
        call<readonly PluginPickedFile[]>("plugin.files.pick", withPluginIdentity(identity, { options }), {}, signal),
    }),
    settings: Object.freeze({
      read: async <T = Record<string, unknown>>(signal?: AbortSignal) => {
        const response = await call<{ values: T }>("plugin.settings.read-trusted", withPluginIdentity(identity), {}, signal);
        return response.values;
      },
      write: <T = Record<string, unknown>>(values: Record<string, unknown>, signal?: AbortSignal) =>
        call<T>("plugin.settings.write", withPluginIdentity(identity, { values }), {}, signal),
    }),
    slots: Object.freeze({
      catalog: () => registry.catalog,
      mounted: (slotId?: string): readonly MountedSlotInstance[] => registry.mounted(slotId),
    }),
  });
}
