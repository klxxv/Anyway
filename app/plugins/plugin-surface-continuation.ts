export const MAX_SURFACE_CONTINUATIONS = 64;
export const SURFACE_CONTINUATION_DEADLINE_MS = 120_000;

const SAFE_ACTION_ID = /^[A-Za-z0-9._-]{1,160}$/u;
const MAX_CURSOR_BYTES = 256;

export type PluginSurfaceIdentity = {
  readonly pluginId: string;
  readonly pluginVersion: string;
  readonly sessionId?: string;
  readonly surfaceIds: readonly string[];
};

export type SurfaceContinuation = {
  readonly actionId: string;
  readonly cursor: string;
};

type ContinuationDispatch = (
  identity: PluginSurfaceIdentity,
  continuation: SurfaceContinuation,
  context: { readonly remainingMs: number; readonly signal: AbortSignal },
) => Promise<unknown>;

type ContinuationDriverOptions = {
  readonly dispatch: ContinuationDispatch;
  readonly maxContinuations?: number;
  readonly deadlineMs?: number;
  readonly now?: () => number;
  readonly yieldTurn?: () => Promise<void>;
};

function ownKeysExactly(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  const keys = Object.keys(value).sort();
  return keys.length === allowed.length && keys.every((key, index) => key === [...allowed].sort()[index]);
}

/** Parse only a Rust-validated worker continuation envelope. */
export function parseSurfaceContinuation(result: unknown): SurfaceContinuation | null {
  if (!result || typeof result !== "object" || Array.isArray(result)) return null;
  const event = (result as { event?: unknown }).event;
  if (!event || typeof event !== "object" || Array.isArray(event)) return null;
  const eventObject = event as Record<string, unknown>;
  if (eventObject.type !== "surface.continue") return null;
  if (!ownKeysExactly(eventObject, ["payload", "type"])) throw new Error("SURFACE_CONTINUATION_INVALID");
  const payload = eventObject.payload;
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) throw new Error("SURFACE_CONTINUATION_INVALID");
  const value = payload as Record<string, unknown>;
  if (!ownKeysExactly(value, ["actionId", "cursor"])) throw new Error("SURFACE_CONTINUATION_INVALID");
  if (typeof value.actionId !== "string" || !SAFE_ACTION_ID.test(value.actionId)) throw new Error("SURFACE_CONTINUATION_INVALID");
  if (typeof value.cursor !== "string" || value.cursor.length === 0 || new TextEncoder().encode(value.cursor).byteLength > MAX_CURSOR_BYTES) {
    throw new Error("SURFACE_CONTINUATION_INVALID");
  }
  return { actionId: value.actionId, cursor: value.cursor };
}

export function createPluginSurfaceContinuationDriver(options: ContinuationDriverOptions) {
  const maxContinuations = options.maxContinuations ?? MAX_SURFACE_CONTINUATIONS;
  const deadlineMs = options.deadlineMs ?? SURFACE_CONTINUATION_DEADLINE_MS;
  const now = options.now ?? Date.now;
  const yieldTurn = options.yieldTurn ?? (() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
  let generation = 0;
  let activeAbort: AbortController | undefined;

  const cancel = () => {
    generation += 1;
    activeAbort?.abort();
    activeAbort = undefined;
  };

  const start = async (identity: PluginSurfaceIdentity, initialResult: unknown): Promise<number> => {
    cancel();
    const run = generation;
    const abort = new AbortController();
    activeAbort = abort;
    const fixedIdentity: PluginSurfaceIdentity = Object.freeze({
      pluginId: identity.pluginId,
      pluginVersion: identity.pluginVersion,
      sessionId: identity.sessionId,
      surfaceIds: Object.freeze([...identity.surfaceIds]),
    });
    const deadline = now() + deadlineMs;
    let result = initialResult;
    let count = 0;
    try {
      while (run === generation && !abort.signal.aborted) {
        const continuation = parseSurfaceContinuation(result);
        if (!continuation) return count;
        if (count >= maxContinuations) throw new Error("SURFACE_CONTINUATION_LIMIT");
        const remainingMs = deadline - now();
        if (remainingMs <= 0) throw new Error("SURFACE_CONTINUATION_DEADLINE");
        await yieldTurn();
        if (run !== generation || abort.signal.aborted) return count;
        result = await options.dispatch(fixedIdentity, continuation, { remainingMs, signal: abort.signal });
        count += 1;
      }
      return count;
    } finally {
      if (activeAbort === abort) activeAbort = undefined;
    }
  };

  return { start, cancel };
}
