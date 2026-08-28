import type { App, Component } from "vue";
import type * as VueRuntime from "vue";

export type PluginFrontendMode = "trusted-module" | "declarative-ui";
export type PluginFrontendFramework = "vue3";
export type PluginFrontendApiVersion = "1";
export type PluginWorkerTransport = "stdio-framed-json-v1";
export type PluginWorkerRpcOperation =
  | "plugin.worker.open"
  | "plugin.worker.call"
  | "plugin.worker.cancel"
  | "plugin.worker.close";

export type HostSlotCardinality = "single" | "multiple";

export interface HostSlotDescriptor {
  readonly id: string;
  readonly label: string;
  readonly region: "workspace" | "toolbar" | "dialog" | "panel" | "status" | "compat";
  readonly cardinality: HostSlotCardinality;
  readonly order: number;
  readonly accepts: readonly PluginFrontendMode[];
}

export type SlotCatalog = readonly HostSlotDescriptor[];

export interface MountedSlotInstance {
  readonly instanceId: string;
  readonly slotId: string;
  readonly owner: "host";
  readonly mountedAt: number;
  readonly context?: Readonly<Record<string, unknown>>;
}

export interface PluginFrontendManifest {
  readonly mode: "trusted-module";
  readonly entry: string;
  readonly framework: "vue3";
  readonly apiVersion: "1";
}

export interface PluginUiContribution {
  readonly id: string;
  readonly slotId: string;
  readonly export: string;
  readonly order?: number;
  readonly when?: string | boolean;
}

export interface PluginManifestWorker {
  readonly id: string;
  readonly language: string;
  readonly entrypoint: string;
  readonly transport: PluginWorkerTransport;
}

export interface PluginNetworkDeclaration {
  readonly mode: "direct" | "none";
}

export interface PluginFrontendPluginIdentity {
  readonly id: string;
  readonly version: string;
  readonly name: string;
  readonly installPath: string;
}

export interface PluginWorkerHandle {
  readonly pluginId: string;
  readonly pluginVersion: string;
  readonly workerId: string;
  readonly sessionId: string;
  readonly fingerprint?: string;
  readonly transport?: PluginWorkerTransport;
  readonly language?: string;
}

export interface PluginWorkerOpenOptions {
  readonly sessionId: string;
  readonly deadlineMs?: number;
}

export interface PluginWorkerCallOptions {
  readonly requestId: string;
  readonly operation: string;
  readonly payload?: unknown;
  readonly deadlineMs?: number;
}

export interface PluginGraphPatchBinding {
  readonly sessionId: string;
  readonly projectId: string;
  readonly baseRevision: number;
}

export interface PluginGraphPatchProposalRequest extends PluginGraphPatchBinding {
  readonly patch: Readonly<Record<string, unknown>>;
}

export interface PluginGraphPatchReviewRequest extends PluginGraphPatchBinding {
  readonly proposalId: string;
  readonly expectedDigest: string;
}

export interface PluginFilePickOptions {
  readonly accept?: readonly string[];
  readonly multiple?: boolean;
  readonly retention?: "session" | "plugin";
}

export interface PluginPickedFile {
  readonly label: string;
  readonly blobRef: Readonly<Record<string, unknown>>;
}

export interface PluginContext {
  readonly apiVersion: "anyway.dev/plugin-frontend/v1";
  readonly plugin: PluginFrontendPluginIdentity;
  readonly vue: typeof VueRuntime;
  readonly worker: {
    readonly open: (workerId: string, options: PluginWorkerOpenOptions, signal?: AbortSignal) => Promise<PluginWorkerHandle>;
    readonly call: <T = unknown>(handle: PluginWorkerHandle, options: PluginWorkerCallOptions, signal?: AbortSignal) => Promise<T>;
    readonly cancel: (handle: PluginWorkerHandle, requestId: string, signal?: AbortSignal) => Promise<boolean>;
    readonly close: (handle: PluginWorkerHandle, signal?: AbortSignal) => Promise<void>;
  };
  readonly graphPatch: {
    readonly propose: <T = unknown>(request: PluginGraphPatchProposalRequest, signal?: AbortSignal) => Promise<T>;
    readonly get: <T = unknown>(request: PluginGraphPatchReviewRequest, signal?: AbortSignal) => Promise<T>;
    readonly review: <T = unknown>(request: PluginGraphPatchReviewRequest & { readonly accept: boolean }, signal?: AbortSignal) => Promise<T>;
    readonly cleanupSession: (sessionId: string, signal?: AbortSignal) => Promise<number>;
  };
  readonly files: {
    readonly pick: (options?: PluginFilePickOptions, signal?: AbortSignal) => Promise<readonly PluginPickedFile[]>;
  };
  readonly settings: {
    readonly read: <T = Record<string, unknown>>(signal?: AbortSignal) => Promise<T>;
    readonly write: <T = Record<string, unknown>>(values: Record<string, unknown>, signal?: AbortSignal) => Promise<T>;
  };
  readonly slots: {
    readonly catalog: () => SlotCatalog;
    readonly mounted: (slotId?: string) => readonly MountedSlotInstance[];
  };
}

export interface PluginFrontendComponentProps {
  readonly plugin: PluginFrontendPluginIdentity;
  readonly contribution: PluginUiContribution;
  readonly context: PluginContext;
  readonly slotId: string;
  readonly instance?: MountedSlotInstance;
  readonly hostContext?: Readonly<Record<string, unknown>>;
}

export interface PluginFrontendActivationContext {
  readonly app?: App;
  readonly plugin: PluginFrontendPluginIdentity;
  readonly context: PluginContext;
}

export interface PluginFrontendModule {
  readonly default?: Component;
  readonly activate?: (context: PluginFrontendActivationContext) => void | (() => void) | Promise<void | (() => void)>;
  readonly deactivate?: (context: PluginFrontendActivationContext) => void | Promise<void>;
  readonly [exportName: string]: unknown;
}

export const PLUGIN_FRONTEND_CONTEXT_API_VERSION = "anyway.dev/plugin-frontend/v1" as const;
