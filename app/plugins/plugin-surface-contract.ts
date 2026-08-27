import type { UiIrActionDispatcher } from "./ui-ir";

export type PluginSurfaceState = Readonly<Record<string, unknown>>;

export type HostSurfaceFile = {
  readonly id: string;
  readonly label: string;
  readonly summary?: string;
  readonly error?: string;
  readonly progress?: number;
};
export type HostSurfaceJob = {
  readonly id: string;
  readonly label: string;
  readonly state: string;
  readonly progress?: number;
  readonly transfer?: { readonly done: number; readonly total: number };
  readonly error?: string;
};
export type HostSurfaceEvent = {
  readonly id?: string;
  readonly sequence?: number;
  readonly createdAt?: string | number;
  readonly phase?: string;
  readonly status?: string;
  readonly summary?: string;
  readonly evidenceCount?: number;
  readonly warningCount?: number;
};
export type HostSurfaceError = {
  readonly id: string;
  readonly code: string;
  readonly message: string;
  readonly stage?: string;
  readonly retryable?: boolean;
  readonly jobId?: string;
};
export type HostSurfaceReviewItem = {
  readonly id: string;
  readonly title: string;
  readonly summary?: string;
  readonly state?: "pending" | "accepted" | "rejected";
};
export type HostSurfaceModel = {
  readonly files: readonly HostSurfaceFile[];
  readonly jobs: readonly HostSurfaceJob[];
  readonly selectedJobId?: string | null;
  readonly publicEvents: readonly HostSurfaceEvent[];
  readonly errors: readonly HostSurfaceError[];
  readonly reviewItems: readonly HostSurfaceReviewItem[];
  readonly globalError?: string;
};
export type HostSurfaceAction =
  | { readonly type: "file.pick" }
  | { readonly type: "file.remove"; readonly fileId: string }
  | { readonly type: "job.select"; readonly jobId: string }
  | { readonly type: "job.open"; readonly jobId: string }
  | { readonly type: "job.cancel"; readonly jobId?: string }
  | { readonly type: "job.retry"; readonly jobId?: string }
  | { readonly type: "review.set-decision"; readonly itemId: string; readonly decision: "accepted" | "rejected" }
  | { readonly type: "review.apply"; readonly mode: "selected" | "all" | "none" };

/** Generic Host↔plugin surface state. Feature adapters may project their state into this shape. */
export type PluginSurfaceController = {
  readonly pluginId: { readonly value: string | null };
  readonly model: { readonly value: HostSurfaceModel };
  readonly state: { readonly value: PluginSurfaceState };
  readonly dispatchAction: UiIrActionDispatcher;
  readonly dispatchHostAction: (action: HostSurfaceAction) => void | Promise<void>;
  readonly dispose?: () => void;
};
