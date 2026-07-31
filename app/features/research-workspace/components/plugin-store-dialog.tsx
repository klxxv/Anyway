"use client";

import {
  IconBox,
  IconCheck,
  IconCode,
  IconDatabaseImport,
  IconPlayerPlay,
  IconPlugConnected,
  IconRefresh,
  IconShieldCheck,
  IconX,
} from "@tabler/icons-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useI18n } from "../../../i18n/provider";
import { builtInPluginCatalog } from "../../../plugins/catalog";
import type { InstalledMycPlugin } from "../../../plugins/contracts";
import {
  executeMycPlugin,
  installMycPlugin,
  listenForMycDrops,
  listInstalledMycPlugins,
} from "../../../plugins/tauri-client";

const enabledStorageKey = "research-canvas.enabled-plugins.v1";
type StoreFilter = "all" | "installed" | "runtime";

function pluginKey(plugin: InstalledMycPlugin) {
  return `${plugin.manifest.metadata.id}@${plugin.manifest.metadata.version}`;
}

/**
 * Desktop-backed plugin store; package extraction and execution stay in Rust.
 * 桌面端插件商店；包解压和执行始终留在 Rust 边界内。
 */
export function PluginStoreDialog({ onClose }: { onClose: () => void }) {
  const { t } = useI18n();
  const [filter, setFilter] = useState<StoreFilter>("all");
  const [installed, setInstalled] = useState<InstalledMycPlugin[]>([]);
  const [enabled, setEnabled] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");

  const refresh = useCallback(async () => {
    setBusy(true);
    setMessage("");
    try {
      setInstalled(await listInstalledMycPlugins());
    } catch (error) {
      setMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    const frame = window.requestAnimationFrame(() => {
      try {
        const values = JSON.parse(
          window.localStorage.getItem(enabledStorageKey) ?? "[]",
        ) as string[];
        setEnabled(new Set(values));
      } catch {
        window.localStorage.removeItem(enabledStorageKey);
      }
      void refresh();
    });
    let cancelled = false;
    let stop: () => void = () => undefined;
    void listenForMycDrops(async (paths) => {
      if (paths.length === 0) return;
      setBusy(true);
      try {
        for (const path of paths) await installMycPlugin(path);
        setMessage(t("plugins.installedToast"));
        await refresh();
      } catch (error) {
        setMessage(error instanceof Error ? error.message : String(error));
      } finally {
        setBusy(false);
      }
    }).then((unlisten) => {
      if (cancelled) unlisten();
      else stop = unlisten;
    });
    return () => {
      cancelled = true;
      window.cancelAnimationFrame(frame);
      stop();
    };
  }, [refresh, t]);

  const visibleInstalled = useMemo(
    () =>
      filter === "runtime"
        ? installed.filter((plugin) => plugin.runtime)
        : installed,
    [filter, installed],
  );

  const toggleEnabled = (key: string) => {
    setEnabled((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      window.localStorage.setItem(enabledStorageKey, JSON.stringify([...next]));
      return next;
    });
  };

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
            <button className="icon-quiet" onClick={() => void refresh()} aria-label={t("plugins.refresh")}>
              <IconRefresh className={busy ? "animate-spin" : ""} size={17} stroke={1.35} />
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
            <div className="rounded-[6px] border border-dashed border-ink/25 bg-canvas px-5 py-4">
              <div className="flex items-center gap-3">
                <IconDatabaseImport size={22} stroke={1.25} className="text-blue" />
                <div>
                  <p className="font-serif text-[13px]">{t("plugins.dropTitle")}</p>
                  <p className="mt-0.5 font-serif text-[9px] text-ink/50">
                    {t("plugins.dropHint")}
                  </p>
                </div>
              </div>
            </div>

            {message && (
              <div className="mt-3 rounded-[4px] border border-blue/20 bg-blue-soft px-3 py-2 font-serif text-[10px] text-blue">
                {message}
              </div>
            )}

            {filter !== "installed" && filter !== "runtime" && (
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
                  const isEnabled = enabled.has(key);
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
                          {plugin.runtime && (
                            <p
                              className="mt-1 truncate font-mono text-[7px] text-ink/40"
                              title={`${t("plugins.sha256")}: ${plugin.runtime.entrySha256}`}
                            >
                              {t("plugins.sha256")} · {plugin.runtime.entrySha256}
                            </p>
                          )}
                        </div>
                        <div className="flex gap-2">
                          {plugin.runtime && (
                            <button
                              className="button-secondary px-3"
                              disabled={!isEnabled || busy}
                              title={!isEnabled ? t("plugins.enableToTest") : undefined}
                              onClick={async () => {
                                setBusy(true);
                                try {
                                  const result = await executeMycPlugin(
                                    plugin.manifest.metadata.id,
                                    plugin.manifest.metadata.version,
                                    { operation: "self-test" },
                                  );
                                  setMessage(JSON.stringify(result.output));
                                } catch (error) {
                                  setMessage(error instanceof Error ? error.message : String(error));
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
                            onClick={() => toggleEnabled(key)}
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
