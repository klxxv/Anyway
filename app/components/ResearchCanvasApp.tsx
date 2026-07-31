"use client";

import { ResearchWorkspaceApp } from "../features/research-workspace/ResearchWorkspaceApp";
import { I18nProvider } from "../i18n/provider";

/**
 * Stable public entrypoint; the actual workspace is decomposed under features.
 * 稳定的公共入口；实际工作区已经拆分到 features 目录。
 */
export function ResearchCanvasApp() {
  return (
    <I18nProvider>
      <ResearchWorkspaceApp />
    </I18nProvider>
  );
}
