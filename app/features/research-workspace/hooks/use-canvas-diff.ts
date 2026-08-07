"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import {
  buildDiffOverlay,
  computeCanvasDiff,
  type CanvasDiffResult,
  type DiffInput,
  type DiffOverlayState,
} from "../../../lib/graph/canvas-diff";
import type { ProjectState } from "../../../lib/research-types";

/**
 * 画布 diff 计算的编排 hook：仅做异步编排，算法在 lib/graph/canvas-diff。
 * Desktop 走 Rust `compute_diff` command；Web 走本地 fallback。
 */
export function useCanvasDiff(
  base: ProjectState | null,
  compare: ProjectState | null,
  enabled: boolean,
) {
  const [result, setResult] = useState<CanvasDiffResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestRef = useRef(0);

  useEffect(() => {
    if (!enabled || !base || !compare) {
      // 异步边界：让重置离开 effect 的同步执行域，避免级联渲染。
      queueMicrotask(() => {
        setResult(null);
        setError(null);
        setLoading(false);
      });
      return;
    }
    const request = ++requestRef.current;
    let cancelled = false;
    queueMicrotask(() => {
      if (cancelled || request !== requestRef.current) return;
      setLoading(true);
      setError(null);
    });
    computeCanvasDiff(base as DiffInput, compare as DiffInput)
      .then((next) => {
        if (cancelled || request !== requestRef.current) return;
        setResult(next);
        setLoading(false);
      })
      .catch((failure: unknown) => {
        if (cancelled || request !== requestRef.current) return;
        setError(failure instanceof Error ? failure.message : String(failure));
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [base, compare, enabled]);

  /** 叠加模式画布状态：由 diff 结果与 base/compare 两版本构建。 */
  const overlay: DiffOverlayState | null = useMemo(() => {
    if (!enabled || !result || !base || !compare) return null;
    return buildDiffOverlay(result, base, compare);
  }, [enabled, result, base, compare]);

  return { result, overlay, loading, error };
}
