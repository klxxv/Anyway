import { computed, ref } from "vue";
import type { UiIrActionDispatcher, UiIrActionRequest } from "../../../../app/plugins/ui-ir";
import type { HostSurfaceAction, HostSurfaceModel, PluginSurfaceController, PluginSurfaceState } from "../../../../app/plugins/plugin-surface-contract";
import { HostSdk } from "../../../../app/platform/host-sdk";
import { createDefaultTauriHostSdkTransport } from "../../../../app/platform/host-sdk-tauri";
import { emitGraphProjectCommitted, getGraphProject } from "../../../../app/platform/graph-project";
import { attachFilesToPluginSurface } from "../../../../app/platform/plugin-surface-file-capability";
import { listInstalledMycPlugins } from "../../../../app/plugins/tauri-client";
import { createPluginSurfaceContinuationDriver, type PluginSurfaceIdentity } from "../../../../app/plugins/plugin-surface-continuation";

export type PluginSurfaceSessionOptions = {
  readonly sessionId?: string;
  readonly surfaceIds: readonly string[];
  readonly pluginId?: string;
  readonly selector?: { readonly service?: string };
  readonly initialState?: PluginSurfaceState;
  readonly onCommit?: (payload: unknown) => void;
  readonly onReject?: () => void;
  readonly onRollback?: () => void;
};

const EMPTY_MODEL: HostSurfaceModel = Object.freeze({ files: [], jobs: [], publicEvents: [], errors: [], reviewItems: [] });
const sdk = new HostSdk(createDefaultTauriHostSdkTransport());

function inDesktop(): boolean {
  return Boolean((globalThis as typeof globalThis & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);
}

function normalizeModel(value: unknown): HostSurfaceModel {
  if (!value || typeof value !== "object" || Array.isArray(value)) return EMPTY_MODEL;
  const candidate = value as Partial<HostSurfaceModel>;
  return {
    files: Array.isArray(candidate.files) ? candidate.files : [],
    jobs: Array.isArray(candidate.jobs) ? candidate.jobs : [],
    selectedJobId: typeof candidate.selectedJobId === "string" ? candidate.selectedJobId : null,
    publicEvents: Array.isArray(candidate.publicEvents) ? candidate.publicEvents.slice(-200) : [],
    errors: Array.isArray(candidate.errors) ? candidate.errors.slice(-50) : [],
    reviewItems: Array.isArray(candidate.reviewItems) ? candidate.reviewItems : [],
    globalError: typeof candidate.globalError === "string" ? candidate.globalError : undefined,
  };
}

export function createPluginSurfaceSessionController(options: PluginSurfaceSessionOptions): PluginSurfaceController {
  const model = ref<HostSurfaceModel>(EMPTY_MODEL);
  const state = ref<PluginSurfaceState>(options.initialState ?? {});
  const pluginId = ref<string | null>(options.pluginId ?? null);
  const pluginVersion = ref<string | null>(null);
  let disposed = false;
  let refreshTimer: ReturnType<typeof setInterval> | undefined;

  const refresh = async () => {
    if (disposed || !inDesktop() || !pluginId.value || !pluginVersion.value) return;
    try {
      const result = await sdk.call<{ model?: unknown; state?: unknown }>("plugin.surface.state", {
        sessionId: options.sessionId,
        pluginId: pluginId.value,
        pluginVersion: pluginVersion.value,
        surfaceIds: options.surfaceIds,
      });
      if (disposed) return;
      model.value = normalizeModel(result?.model ?? result);
      if (result?.state && typeof result.state === "object" && !Array.isArray(result.state)) state.value = result.state as PluginSurfaceState;
    } catch (cause) {
      if (!disposed) model.value = { ...model.value, globalError: cause instanceof Error ? cause.message : String(cause) };
    }
  };
  const handleResult = async (result: unknown) => {
    if (!result || typeof result !== "object") return;
    const candidate = result as { event?: { type?: unknown; payload?: unknown } };
    if (candidate.event?.type === "surface.commit" || candidate.event?.type === "surface.reject") {
      const payload = candidate.event.payload;
      if (!payload || typeof payload !== "object" || Array.isArray(payload)
        || typeof (payload as { proposalId?: unknown }).proposalId !== "string"
        || typeof (payload as { projectId?: unknown }).projectId !== "string"
        || typeof (payload as { baseRevision?: unknown }).baseRevision !== "number"
        || !Number.isSafeInteger((payload as { baseRevision: number }).baseRevision)
        || typeof (payload as { expectedDigest?: unknown }).expectedDigest !== "string"
        || !pluginId.value || !pluginVersion.value) {
        throw new Error("Worker review event did not reference a Host proposal");
      }
      const accept = candidate.event.type === "surface.commit";
      const binding = payload as {
        proposalId: string;
        projectId: string;
        baseRevision: number;
        expectedDigest: string;
      };
      const reviewed = await sdk.call<{ status?: unknown; newRevision?: unknown }>("graph.patch.review", {
        pluginId: pluginId.value,
        pluginVersion: pluginVersion.value,
        sessionId: options.sessionId ?? "default",
        proposalId: binding.proposalId,
        projectId: binding.projectId,
        baseRevision: binding.baseRevision,
        expectedDigest: binding.expectedDigest,
        accept,
      });
      if (accept) {
        if (
          reviewed?.status !== "accepted"
          || typeof reviewed.newRevision !== "number"
          || !Number.isSafeInteger(reviewed.newRevision)
        ) {
          throw new Error("Host review gate did not commit an accepted project revision");
        }
        emitGraphProjectCommitted(
          await getGraphProject(binding.projectId, reviewed.newRevision),
        );
        options.onCommit?.(reviewed);
      } else {
        if (reviewed?.status !== "rejected") throw new Error("Host review gate did not reject the proposal");
        options.onReject?.();
      }
    }
    else if (candidate.event?.type === "surface.rollback") options.onRollback?.();
  };
  const currentIdentity = (): PluginSurfaceIdentity | null => pluginId.value && pluginVersion.value ? {
    pluginId: pluginId.value,
    pluginVersion: pluginVersion.value,
    sessionId: options.sessionId,
    surfaceIds: [...options.surfaceIds],
  } : null;
  const continuations = createPluginSurfaceContinuationDriver({
    dispatch: async (identity, continuation, { remainingMs, signal }) => {
      if (disposed || signal.aborted
        || pluginId.value !== identity.pluginId
        || pluginVersion.value !== identity.pluginVersion
        || options.sessionId !== identity.sessionId) return undefined;
      const result = await sdk.call("plugin.surface.action", {
        pluginId: identity.pluginId,
        pluginVersion: identity.pluginVersion,
        sessionId: identity.sessionId,
        surfaceIds: identity.surfaceIds,
        payload: continuation,
      }, { deadlineMs: Math.max(1, Math.min(remainingMs, 90_000)) }, signal);
      if (!signal.aborted) {
        await handleResult(result);
        await refresh();
      }
      return result;
    },
  });
  const send = async (kind: "action" | "host-action", payload: unknown) => {
    if (!inDesktop() || disposed || !pluginId.value || !pluginVersion.value) return;
    continuations.cancel();
    if (kind === "host-action" && (payload as { type?: unknown })?.type === "file.pick") {
      const results = await attachFilesToPluginSurface(sdk, {
        pluginId: pluginId.value,
        pluginVersion: pluginVersion.value,
        sessionId: options.sessionId,
        surfaceIds: options.surfaceIds,
      });
      for (const result of results) await handleResult(result);
      await refresh();
      return;
    }
    const result = await sdk.call("plugin.surface." + kind, { sessionId: options.sessionId, pluginId: pluginId.value, pluginVersion: pluginVersion.value, surfaceIds: options.surfaceIds, payload }, kind === "action" ? { deadlineMs: 120_000 } : undefined);
    await handleResult(result);
    await refresh();
    if (kind === "action") {
      const identity = currentIdentity();
      if (identity) void continuations.start(identity, result).catch((cause) => {
        if (!disposed) model.value = { ...model.value, globalError: cause instanceof Error ? cause.message : String(cause) };
      });
    }
  };
  const dispatchAction: UiIrActionDispatcher = (request: UiIrActionRequest) => send("action", request);
  const dispatchHostAction = (action: HostSurfaceAction) => send("host-action", action);
  const resolve = async () => {
    if (inDesktop()) {
      try {
        const plugins = await listInstalledMycPlugins();
        const selected = plugins.find((plugin) => {
          if (pluginId.value) return plugin.manifest.metadata.id === pluginId.value;
          const services = (plugin.manifest as { provides?: { services?: unknown } }).provides?.services;
          return options.selector?.service && Array.isArray(services) && services.includes(options.selector.service);
        });
        pluginId.value = selected?.manifest.metadata.id ?? null;
        pluginVersion.value = selected?.manifest.metadata.version ?? null;
      } catch {
        pluginId.value = null;
        pluginVersion.value = null;
      }
    }
    await refresh();
  };
  void resolve();
  refreshTimer = setInterval(() => void refresh(), 700);
  return {
    pluginId: computed(() => pluginId.value),
    model: computed(() => model.value),
    state: computed(() => state.value),
    dispatchAction,
    dispatchHostAction,
    dispose: () => { disposed = true; continuations.cancel(); if (refreshTimer !== undefined) clearInterval(refreshTimer); },
  };
}
