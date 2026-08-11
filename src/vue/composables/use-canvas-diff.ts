import {
  computed,
  onBeforeUnmount,
  ref,
  toValue,
  watch,
  type ComputedRef,
  type MaybeRefOrGetter,
  type Ref,
} from "vue";
import {
  buildDiffOverlay,
  computeCanvasDiff,
  type CanvasDiffResult,
  type DiffInput,
  type DiffOverlayState,
} from "../../../app/lib/graph/canvas-diff";
import type { ProjectState } from "../../../app/lib/research-types";

export type CanvasDiffComposableResult = {
  result: Ref<CanvasDiffResult | null>;
  overlay: ComputedRef<DiffOverlayState | null>;
  loading: Ref<boolean>;
  error: Ref<string | null>;
};

/**
 * Vue Composition API port of the canvas diff orchestration hook. Raw values,
 * refs, and getters are accepted so callers can preserve the old arguments
 * while still receiving reactive recomputation in Vue.
 */
export function useCanvasDiff(
  base: MaybeRefOrGetter<ProjectState | null>,
  compare: MaybeRefOrGetter<ProjectState | null>,
  enabled: MaybeRefOrGetter<boolean>,
): CanvasDiffComposableResult {
  const result = ref<CanvasDiffResult | null>(null);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const requestRef = ref(0);

  const stop = watch(
    () => [toValue(base), toValue(compare), toValue(enabled)] as const,
    ([nextBase, nextCompare, nextEnabled], _previous, onCleanup) => {
      const request = ++requestRef.value;
      let cancelled = false;
      onCleanup(() => {
        cancelled = true;
      });

      if (!nextEnabled || !nextBase || !nextCompare) {
        // Keep reset asynchronous so the rendered dialog can settle first.
        queueMicrotask(() => {
          if (cancelled || request !== requestRef.value) return;
          result.value = null;
          error.value = null;
          loading.value = false;
        });
        return;
      }

      queueMicrotask(() => {
        if (cancelled || request !== requestRef.value) return;
        loading.value = true;
        error.value = null;
      });

      computeCanvasDiff(nextBase as DiffInput, nextCompare as DiffInput)
        .then((nextResult) => {
          if (cancelled || request !== requestRef.value) return;
          result.value = nextResult;
          loading.value = false;
        })
        .catch((failure: unknown) => {
          if (cancelled || request !== requestRef.value) return;
          error.value = failure instanceof Error ? failure.message : String(failure);
          loading.value = false;
        });
    },
    { immediate: true },
  );

  onBeforeUnmount(() => {
    requestRef.value += 1;
    stop();
  });

  const overlay = computed<DiffOverlayState | null>(() => {
    const currentBase = toValue(base);
    const currentCompare = toValue(compare);
    if (!toValue(enabled) || !result.value || !currentBase || !currentCompare) {
      return null;
    }
    return buildDiffOverlay(result.value, currentBase, currentCompare);
  });

  return { result, overlay, loading, error };
}
