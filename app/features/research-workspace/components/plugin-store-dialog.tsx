"use client";

import {
  IconBox,
  IconCheck,
  IconCode,
  IconDatabaseImport,
  IconFolder,
  IconPlayerPlay,
  IconPlugConnected,
  IconRefresh,
  IconShieldCheck,
  IconTrash,
  IconUpload,
  IconWorld,
  IconX,
} from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useI18n } from "../../../i18n/provider";
import { builtInPluginCatalog } from "../../../plugins/catalog";
import {
  latestCompatiblePlugins,
  pluginCompatibility,
  pluginKey,
} from "../../../plugins/identity";
import { usePluginHost } from "../../../plugins/plugin-host";
import { pluginReference } from "../../../plugins/contracts";
import {
  listenForMycDrops,
  pickMycFiles,
  runAnalysisPlugin,
} from "../../../plugins/tauri-client";

type StoreFilter = "all" | "installed" | "runtime" | "workspace" | "locales";

/**
 * Desktop-backed plugin store; package extraction and execution stay in Rust.
 * 桌面端插件商店；包解压和执行始终留在 Rust 边界内。
 */
export function PluginStoreDialog({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const {
    installedPlugins: installed,
    activePluginKeys,
    loading,
    error: hostError,
    refresh,
    install,
    setPluginEnabled,
    enableAll,
    removeIncompatible,
  } = usePluginHost();
  const [filter, setFilter] = useState<StoreFilter>("all");
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const [selfTestResults, setSelfTestResults] = useState<
    Record<string, { status: "running" | "success" | "error"; text: string }>
  >({});
  const latestKeys = useMemo(
    () => new Set(latestCompatiblePlugins(installed).map(pluginKey)),
    [installed],
  );
  const hostBusy = busy || loading;
  const incompatibleCount = useMemo(
    () => installed.filter((plugin) => !pluginCompatibility(plugin).compatible).length,
    [installed],
  );

  // ── 拖放状态 / Drag-drop state ──
  const [dragOver, setDragOver] = useState(false);
  const dragCounter = useRef(0);

  const installPaths = useCallback(
    async (paths: string[]) => {
      const mycPaths = paths.filter((p) => p.toLowerCase().endsWith(".myc"));
      if (mycPaths.length === 0) return;
      setBusy(true);
      setMessage("");
      let ok = 0;
      const errors: string[] = [];
      for (const path of mycPaths) {
        try {
          await install(path);
          ok++;
        } catch (error) {
          errors.push(error instanceof Error ? error.message : String(error));
        }
      }
      if (errors.length === 0) {
        setMessage(t("plugins.installedToast", { count: ok }));
      } else {
        setMessage(
          ok > 0
            ? `${t("plugins.installedToast", { count: ok })} · ${errors.join("; ")}`
            : errors.join("; "),
        );
      }
      setBusy(false);
    },
    [install, t],
  );

  // ── Tauri 原生拖放监听 (webview 级别) / Tauri native drop listener (webview-level) ──
  useEffect(() => {
    let cancelled = false;
    let stop: () => void = () => undefined;
    void listenForMycDrops(async (paths) => {
      if (paths.length === 0) return;
      await installPaths(paths);
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    });
    return () => {
      cancelled = true;
      stop();
    };
  }, [installPaths]);

  // ── 点击浏览按钮 / Browse button handler ──
  const handleBrowse = useCallback(async () => {
    const paths = await pickMycFiles();
    if (paths && paths.length > 0) {
      await installPaths(paths);
    }
  }, [installPaths]);

  // ── HTML5 拖放事件处理 / HTML5 drag-drop event handlers ──
  const handleDragEnter = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter.current++;
    setDragOver(true);
  }, []);

  const handleDragLeave = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    dragCounter.current--;
    if (dragCounter.current <= 0) {
      dragCounter.current = 0;
      setDragOver(false);
    }
  }, []);

  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
  }, []);

  const handleDrop = useCallback(
    async (e: React.DragEvent) => {
      e.preventDefault();
      e.stopPropagation();
      dragCounter.current = 0;
      setDragOver(false);

      // 尝试从 Tauri 原生事件中提取路径；回退到 HTML5 File API
      const files = e.dataTransfer?.files;
      if (!files || files.length === 0) return;

      const paths: string[] = [];
      for (let i = 0; i < files.length; i++) {
        const file = files[i];
        // Tauri v2 在 File 对象上暴露 path 属性
        const filePath = (file as unknown as { path?: string }).path;
        if (filePath) {
          paths.push(filePath);
        }
      }

      if (paths.length > 0) {
        await installPaths(paths);
      } else {
        setMessage(t("plugins.dropNoPaths"));
      }
    },
    [installPaths, t],
  );

  const visibleInstalled = useMemo(
    () =>
      filter === "runtime"
        ? installed.filter((plugin) => plugin.runtime)
        : filter === "workspace"
          ? installed.filter((plugin) => plugin.workspace)
          : filter === "locales"
            ? installed.filter((plugin) => plugin.locales?.length)
        : installed,
    [filter, installed],
  );

  return (
    <div className="fixed inset-0 z-[96] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <section
        className="flex h-[650px] w-[880px] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]"
        role="dialog"
        aria-modal="true"
        aria-labelledby="plugin-store-title"
      >
        <header className="flex shrink-0 items-start justify-between border-b border-ink/15 px-7 py-5">
          <div>
            <span className="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">
              {t("plugins.eyebrow")}
            </span>
            <h2 id="plugin-store-title" className="mt-1 font-serif text-[21px]">
              {t("plugins.title")}
            </h2>
            <p className="mt-1 font-serif text-[10px] text-ink/50">
              {t("plugins.subtitle")}
            </p>
          </div>
          <div className="flex items-center gap-1">
            {incompatibleCount > 0 && (
              <button
                className="button-secondary mr-2 px-3 text-alert"
                disabled={hostBusy}
                onClick={async () => {
                  if (!window.confirm(t("plugins.removeIncompatibleConfirm", { count: incompatibleCount }))) {
                    return;
                  }
                  setBusy(true);
                  try {
                    const removed = await removeIncompatible();
                    setMessage(t("plugins.removedIncompatibleToast", { count: removed }));
                  } catch (error) {
                    setMessage(error instanceof Error ? error.message : String(error));
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                <IconTrash size={14} stroke={1.4} />
                {t("plugins.removeIncompatible", { count: incompatibleCount })}
              </button>
            )}
            <button className="button-secondary mr-2 px-3" onClick={enableAll} disabled={hostBusy}>
              <IconCheck size={14} stroke={1.4} />
              {t("plugins.enableAll")}
            </button>
            <button className="icon-quiet" onClick={() => void refresh()} aria-label={t("plugins.refresh")}>
              <IconRefresh className={hostBusy ? "animate-spin" : ""} size={17} stroke={1.35} />
            </button>
            <button className="icon-quiet" onClick={onClose} aria-label={t("plugins.close")}>
              <IconX size={18} stroke={1.35} />
            </button>
          </div>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-[210px_minmax(0,1fr)]">
          <aside className="border-r border-ink/15 bg-canvas p-4">
            <nav className="space-y-1" aria-label="Plugin filters">
              {([
                ["all", t("plugins.all"), IconBox],
                ["installed", t("plugins.installed"), IconCheck],
                ["runtime", t("plugins.runtime"), IconCode],
                ["workspace", t("plugins.workspace"), IconFolder],
                ["locales", t("plugins.locales"), IconWorld],
              ] as const).map(([value, label, Icon]) => (
                <button
                  key={value}
                  className={`flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left font-serif text-[12px] transition ${
                    filter === value ? "bg-blue-soft text-blue" : "hover:bg-ink/5"
                  }`}
                  onClick={() => setFilter(value)}
                >
                  <Icon size={17} stroke={1.35} />
                  {label}
                </button>
              ))}
            </nav>
            <div className="mt-6 rounded-[5px] border border-blue/20 bg-blue-soft p-3">
              <div className="flex items-center gap-2 text-blue">
                <IconShieldCheck size={17} stroke={1.35} />
                <span className="font-serif text-[11px]">{t("plugins.runtime")}</span>
              </div>
              <p className="mt-2 font-serif text-[9px] leading-[1.45] text-ink/55">
                {t("plugins.runtimeHint")}
              </p>
            </div>
          </aside>

          <div className="min-h-0 overflow-y-auto p-6">
            <div
              className={`rounded-[6px] border-2 border-dashed px-5 py-5 transition-colors ${
                dragOver
                  ? "border-blue bg-blue/5"
                  : hostBusy
                    ? "border-ink/15 bg-canvas opacity-60"
                    : "border-ink/25 bg-canvas hover:border-ink/40"
              }`}
              onDragEnter={handleDragEnter}
              onDragLeave={handleDragLeave}
              onDragOver={handleDragOver}
              onDrop={handleDrop}
              role="button"
              tabIndex={0}
              aria-label={t("plugins.dropTitle")}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") handleBrowse();
              }}
            >
              <div className="flex items-center gap-3">
                <IconDatabaseImport
                  size={24}
                  stroke={1.25}
                  className={dragOver ? "text-blue" : "text-blue/70"}
                />
                <div className="flex-1">
                  <p className="font-serif text-[13px]">
                    {dragOver ? t("plugins.dropActive") : t("plugins.dropTitle")}
                  </p>
                  <p className="mt-0.5 font-serif text-[9px] text-ink/50">
                    {t("plugins.dropHint")}
                  </p>
                </div>
                <button
                  type="button"
                  className={`flex items-center gap-1.5 rounded-[5px] border px-3.5 py-2 font-serif text-[11px] transition ${
                    hostBusy
                      ? "cursor-not-allowed border-ink/15 text-ink/30"
                      : "border-blue/40 bg-blue-soft text-blue hover:bg-blue/15"
                  }`}
                  disabled={hostBusy}
                  onClick={(e) => {
                    e.stopPropagation();
                    void handleBrowse();
                  }}
                >
                  <IconUpload size={15} stroke={1.4} />
                  {t("plugins.browseFiles")}
                </button>
              </div>
            </div>

            {(message || hostError) && (
              <div className="mt-3 rounded-[4px] border border-blue/20 bg-blue-soft px-3 py-2 font-serif text-[10px] text-blue">
                {message || hostError}
              </div>
            )}

            {filter === "all" && (
              <section className="mt-6">
                <h3 className="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
                  {t("plugins.builtInCatalog")}
                </h3>
                <div className="mt-2 grid grid-cols-2 gap-3">
                  {builtInPluginCatalog.map((plugin) => (
                    <article key={plugin.id} className="rounded-[5px] border border-ink/15 p-3">
                      <div className="flex items-start gap-3">
                        <IconPlugConnected size={18} stroke={1.35} className="mt-0.5 text-ink/60" />
                        <div className="min-w-0 flex-1">
                          <p className="font-serif text-[12px]">{plugin.name}</p>
                          <p className="mt-1 line-clamp-2 font-serif text-[9px] leading-[1.4] text-ink/50">
                            {plugin.description}
                          </p>
                        </div>
                        <span className="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/40">
                          {plugin.status}
                        </span>
                      </div>
                    </article>
                  ))}
                </div>
              </section>
            )}

            <section className="mt-6">
              <h3 className="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
                {t("plugins.installed")} · .myc
              </h3>
              <div className="mt-2 space-y-2">
                {visibleInstalled.length === 0 && (
                  <p className="rounded-[5px] border border-ink/15 px-4 py-6 text-center font-serif text-[10px] text-ink/45">
                    {t("plugins.noInstalled")}
                  </p>
                )}
                {visibleInstalled.map((plugin) => {
                  const key = pluginKey(plugin);
                  const isEnabled = activePluginKeys.has(key);
                  const compatibility = pluginCompatibility(plugin);
                  const superseded = compatibility.compatible && !latestKeys.has(key);
                  return (
                    <article key={key} className="rounded-[5px] border border-ink/18 p-4">
                      <div className="flex items-start gap-3">
                        {plugin.runtime ? (
                          <IconCode size={19} stroke={1.35} className="mt-0.5 text-blue" />
                        ) : (
                          <IconBox size={19} stroke={1.35} className="mt-0.5 text-ink/60" />
                        )}
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <h4 className="font-serif text-[13px]">{plugin.manifest.metadata.name}</h4>
                            <span className="font-sans text-[7px] uppercase tracking-[0.12em] text-ink/40">
                              {plugin.manifest.kind} · {plugin.manifest.metadata.version}
                            </span>
                          </div>
                          <p className="mt-1 font-serif text-[9px] leading-[1.45] text-ink/50">
                            {plugin.manifest.metadata.description}
                          </p>
                          <p className="mt-2 font-sans text-[8px] text-ink/45">
                            {plugin.manifest.spec.capabilities.join(" · ")}
                            {plugin.runtime ? ` · ${plugin.runtime.language}/wasm` : ""}
                          </p>
                          {!compatibility.compatible && (
                            <p className="mt-1 font-serif text-[8px] text-alert">
                              {compatibility.issues.join(" · ")}
                            </p>
                          )}
                          {superseded && (
                            <p className="mt-1 font-serif text-[8px] text-ink/40">
                              {t("plugins.superseded")}
                            </p>
                          )}
                          {plugin.runtime && (
                            <p
                              className="mt-1 truncate font-mono text-[7px] text-ink/40"
                              title={`${t("plugins.sha256")}: ${plugin.runtime.entrySha256}`}
                            >
                              {t("plugins.sha256")} · {plugin.runtime.entrySha256}
                            </p>
                          )}
                          {selfTestResults[key] && (
                            <p
                              className={`mt-2 rounded-[4px] border px-2.5 py-2 font-serif text-[9px] ${
                                selfTestResults[key].status === "error"
                                  ? "border-alert/25 bg-alert/5 text-alert"
                                  : selfTestResults[key].status === "running"
                                    ? "border-blue/20 bg-blue-soft text-blue"
                                    : "border-ink/15 bg-canvas text-ink/65"
                              }`}
                              role="status"
                              aria-live="polite"
                            >
                              {selfTestResults[key].text}
                            </p>
                          )}
                          {plugin.workspace && (
                            <p className="mt-1 font-serif text-[8px] text-ink/45">
                              {(plugin.manifest.spec.contributes?.commands ?? [])
                                .map((command) => command.label)
                                .join(" · ")}
                            </p>
                          )}
                          {plugin.locales && plugin.locales.length > 0 && (
                            <p className="mt-1 font-serif text-[8px] text-ink/45">
                              {plugin.locales
                                .map((locale) => `${locale.name} (${locale.locale})`)
                                .join(" · ")}
                            </p>
                          )}
                        </div>
                        <div className="flex gap-2">
                          {plugin.runtime && (
                            <button
                              className="button-secondary px-3"
                              disabled={!isEnabled || hostBusy}
                              title={!isEnabled ? t("plugins.enableToTest") : undefined}
                              onClick={async () => {
                                setBusy(true);
                                setSelfTestResults((current) => ({
                                  ...current,
                                  [key]: { status: "running", text: t("plugins.selfTestRunning") },
                                }));
                                try {
                                  const result = await runAnalysisPlugin(
                                    pluginReference(plugin),
                                    { operation: "self-test" },
                                    "analysis.run",
                                  );
                                  setSelfTestResults((current) => ({
                                    ...current,
                                    [key]: {
                                      status: "success",
                                      text: `${t("plugins.selfTestPassed", {
                                        duration: result.durationMs,
                                        fuel: result.fuelConsumed,
                                      })} ${JSON.stringify(result.output)}`,
                                    },
                                  }));
                                } catch (error) {
                                  setSelfTestResults((current) => ({
                                    ...current,
                                    [key]: {
                                      status: "error",
                                      text: error instanceof Error ? error.message : String(error),
                                    },
                                  }));
                                } finally {
                                  setBusy(false);
                                }
                              }}
                            >
                              <IconPlayerPlay size={14} stroke={1.4} />
                              {t("plugins.selfTest")}
                            </button>
                          )}
                          <button
                            className={isEnabled ? "button-primary px-3" : "button-secondary px-3"}
                            disabled={!compatibility.compatible || hostBusy}
                            onClick={() => setPluginEnabled(plugin, !isEnabled)}
                          >
                            {isEnabled ? t("plugins.enabled") : t("plugins.enable")}
                          </button>
                        </div>
                      </div>
                    </article>
                  );
                })}
              </div>
            </section>
          </div>
        </div>
      </section>
    </div>
  );
}
