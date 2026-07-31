"use client";

import {
  IconArrowRight,
  IconCheck,
  IconFileText,
  IconFolder,
  IconHistory,
  IconKeyboard,
  IconLayoutGrid,
  IconPalette,
  IconPointer,
  IconPlugConnected,
  IconRefresh,
  IconSearch,
  IconSettings,
  IconX,
} from "@tabler/icons-react";
import { useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { useI18n } from "../../../i18n/provider";
import type { ProjectState, ResearchNodeType } from "../../../lib/research-types";
import { layoutOptions } from "../workspace-layout";
import {
  defaultWorkspacePreferences,
  type WorkspacePreferences,
} from "../workspace-preferences";
import {
  defaultWorkspaceShortcuts,
  shortcutConflicts,
  shortcutFromKeyboardEvent,
  type ShortcutAction,
} from "../workspace-shortcuts";
import type { NodeDraft } from "../workspace-types";

export type ComposerState = {
  type: ResearchNodeType;
  x: number;
  y: number;
};

function defaultDataForType(type: ResearchNodeType): Record<string, unknown> {
  switch (type) {
    case "question":
      return { questionKind: "causal", scope: "current study" };
    case "concept":
      return { groupRole: "theme", discipline: "" };
    case "variable":
      return {
        valueType: "enum",
        enumValues: ["low", "medium", "high"],
        unit: "",
        observationRole: "measured",
      };
    case "method":
      return {
        methodFamily: "observational",
        input: "",
        output: "",
        reproducible: true,
      };
    case "dataset":
      return { format: "table", source: "", coverage: "", resolution: "" };
    case "evidence":
      return {
        sourceKind: "article",
        citation: "",
        year: new Date().getFullYear(),
        confidence: "medium",
      };
    case "result":
      return { outcome: "supports", metric: "", confidence: 0.8, direction: "positive" };
    case "note":
      return { noteKind: "observation", author: "local researcher" };
    default:
      return {};
  }
}

function makeDraft(type: ResearchNodeType): NodeDraft {
  return {
    title: "",
    body: "",
    type,
    tags: [],
    data: defaultDataForType(type),
  };
}

function NodeTypeFields({
  draft,
  onData,
}: {
  draft: NodeDraft;
  onData: (key: string, value: unknown) => void;
}) {
  // Keep domain-specific metadata close to its node type without multiplying dialogs.
  // 将领域字段收敛在节点类型内部，避免为每种节点复制整套弹窗。
  const value = (key: string) => String(draft.data[key] ?? "");
  const select = (
    label: string,
    key: string,
    options: Array<[string, string]>,
  ) => (
    <label className="dialog-field">
      {label}
      <select value={value(key)} onChange={(event) => onData(key, event.target.value)}>
        {options.map(([optionValue, optionLabel]) => (
          <option key={optionValue} value={optionValue}>
            {optionLabel}
          </option>
        ))}
      </select>
    </label>
  );
  const input = (label: string, key: string, placeholder: string, type = "text") => (
    <label className="dialog-field">
      {label}
      <input
        type={type}
        value={value(key)}
        onChange={(event) =>
          onData(key, type === "number" ? Number(event.target.value) : event.target.value)
        }
        placeholder={placeholder}
      />
    </label>
  );

  switch (draft.type) {
    case "question":
      return (
        <>
          {select("Question kind", "questionKind", [
            ["causal", "causal"],
            ["descriptive", "descriptive"],
            ["comparative", "comparative"],
            ["exploratory", "exploratory"],
          ])}
          {input("Research scope", "scope", "Population, place, or time range")}
        </>
      );
    case "concept":
      return (
        <>
          {select("Group role", "groupRole", [
            ["theme", "theme"],
            ["population", "population"],
            ["mechanism", "mechanism"],
            ["context", "context"],
          ])}
          {input("Discipline", "discipline", "e.g. urban climatology")}
        </>
      );
    case "variable": {
      const valueType = value("valueType");
      return (
        <>
          {select("Value schema", "valueType", [
            ["enum", "enum"],
            ["bool", "bool"],
            ["number", "number"],
            ["text", "text"],
          ])}
          {input("Unit", "unit", valueType === "bool" ? "not applicable" : "%, °C, score…")}
          {valueType === "enum" &&
            input("Enum values", "enumValues", "low, medium, high")}
          {valueType === "bool" &&
            select("Fact role", "observationRole", [
              ["measured", "measured fact"],
              ["observed", "observed fact"],
              ["assumed", "working assumption"],
            ])}
        </>
      );
    }
    case "method":
      return (
        <>
          {select("Method family", "methodFamily", [
            ["observational", "observational"],
            ["experimental", "experimental"],
            ["classification", "classification"],
            ["simulation", "simulation"],
            ["statistical", "statistical"],
          ])}
          {input("Primary input", "input", "Dataset or measured variable")}
          {input("Expected output", "output", "Classification, estimate, or model")}
          <label className="mb-4 flex h-10 items-center gap-3 rounded-[4px] border border-ink/20 px-3 font-serif text-[12px]">
            <input
              className="size-4 accent-blue"
              type="checkbox"
              checked={Boolean(draft.data.reproducible)}
              onChange={(event) => onData("reproducible", event.target.checked)}
            />
            Reproducible protocol
          </label>
        </>
      );
    case "dataset":
      return (
        <>
          {select("Data format", "format", [
            ["table", "table"],
            ["raster", "raster"],
            ["vector", "vector"],
            ["time-series", "time series"],
            ["text", "text corpus"],
          ])}
          {input("Source", "source", "Repository, sensor, or provider")}
          {input("Coverage", "coverage", "2020–2025 / study region")}
          {input("Resolution", "resolution", "10 m / daily / one record")}
        </>
      );
    case "evidence":
      return (
        <>
          {select("Source kind", "sourceKind", [
            ["article", "article"],
            ["measurement", "measurement"],
            ["report", "report"],
            ["raster", "raster"],
            ["code", "code artifact"],
          ])}
          {input("Citation", "citation", "Author, venue, or source")}
          {input("Publication year", "year", "2026", "number")}
          {select("Confidence", "confidence", [
            ["low", "low"],
            ["medium", "medium"],
            ["high", "high"],
          ])}
        </>
      );
    case "result":
      return (
        <>
          {select("Outcome", "outcome", [
            ["supports", "supports"],
            ["refutes", "refutes"],
            ["mixed", "mixed"],
            ["neutral", "neutral"],
          ])}
          {input("Metric", "metric", "Primary measured outcome")}
          {input("Confidence", "confidence", "0.80", "number")}
          {select("Direction", "direction", [
            ["positive", "positive"],
            ["negative", "negative"],
            ["mixed", "mixed"],
            ["unknown", "unknown"],
          ])}
        </>
      );
    case "note":
      return (
        <>
          {select("Note kind", "noteKind", [
            ["observation", "observation"],
            ["assumption", "assumption"],
            ["decision", "decision"],
            ["todo", "to do"],
          ])}
          {input("Author", "author", "Researcher or team")}
        </>
      );
    default:
      return null;
  }
}

export function NodeComposer({
  state,
  onClose,
  onCreate,
}: {
  state: ComposerState;
  onClose: () => void;
  onCreate: (draft: NodeDraft, x: number, y: number) => void;
}) {
  const [draft, setDraft] = useState<NodeDraft>(() => makeDraft(state.type));
  const updateData = (key: string, nextValue: unknown) => {
    setDraft((current) => {
      const value =
        key === "enumValues" && typeof nextValue === "string"
          ? nextValue
              .split(",")
              .map((item) => item.trim())
              .filter(Boolean)
          : nextValue;
      return { ...current, data: { ...current.data, [key]: value } };
    });
  };

  return (
    <div className="fixed inset-0 z-[80] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <form
        className="flex max-h-[86vh] w-[590px] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]"
        onSubmit={(event) => {
          event.preventDefault();
          onCreate(draft, state.x, state.y);
        }}
      >
        <div className="flex shrink-0 items-start justify-between border-b border-ink/15 px-7 pb-5 pt-6">
          <div>
            <span className="font-sans text-[9px] uppercase tracking-[0.18em] text-blue">
              New research node
            </span>
            <h2 className="mt-1 font-serif text-[20px]">Add to the canvas</h2>
          </div>
          <button className="icon-quiet" type="button" onClick={onClose} aria-label="Close">
            <IconX size={19} stroke={1.35} />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-7 py-5">
          <div className="grid grid-cols-2 gap-x-4">
            <label className="dialog-field col-span-2">
              Type
              <select
                value={draft.type}
                onChange={(event) => {
                  const type = event.target.value as ResearchNodeType;
                  setDraft((current) => ({
                    ...makeDraft(type),
                    title: current.title,
                    body: current.body,
                    tags: current.tags,
                  }));
                }}
              >
                {[
                  ["question", "question"],
                  ["concept", "group"],
                  ["variable", "variable"],
                  ["method", "method"],
                  ["dataset", "data"],
                  ["evidence", "evidence"],
                  ["result", "result"],
                  ["note", "note"],
                ].map(([type, label]) => (
                  <option key={type} value={type}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <label className="dialog-field col-span-2">
              Title
              <input
                autoFocus
                value={draft.title}
                onChange={(event) =>
                  setDraft((current) => ({ ...current, title: event.target.value }))
                }
                placeholder="Name the research object"
              />
            </label>

            <div className="col-span-2 mb-4 flex items-center gap-3">
              <span className="font-sans text-[9px] uppercase tracking-[0.16em] text-ink/45">
                Node profile
              </span>
              <span className="h-px flex-1 bg-ink/12" />
              <IconCheck size={15} stroke={1.35} className="text-blue" />
              <span className="font-serif text-[10px] text-ink/55">type-specific data</span>
            </div>
            <NodeTypeFields draft={draft} onData={updateData} />

            <label className="dialog-field col-span-2">
              Tags
              <input
                value={draft.tags.join(", ")}
                onChange={(event) =>
                  setDraft((current) => ({
                    ...current,
                    tags: event.target.value
                      .split(",")
                      .map((item) => item.trim())
                      .filter(Boolean),
                  }))
                }
                placeholder="independent, verified, urban"
              />
            </label>
            <label className="dialog-field col-span-2">
              Note
              <textarea
                value={draft.body}
                onChange={(event) =>
                  setDraft((current) => ({ ...current, body: event.target.value }))
                }
                placeholder="Add a precise definition or observation"
              />
            </label>
          </div>
        </div>

        <div className="flex shrink-0 justify-end gap-2 border-t border-ink/15 px-7 py-4">
          <button className="button-secondary" type="button" onClick={onClose}>
            Cancel
          </button>
          <button className="button-primary" type="submit" disabled={!draft.title.trim()}>
            Add node
            <IconArrowRight size={16} stroke={1.4} />
          </button>
        </div>
      </form>
    </div>
  );
}

export function SearchPalette({
  project,
  onClose,
  onSelect,
}: {
  project: ProjectState;
  onClose: () => void;
  onSelect: (nodeId: string) => void;
}) {
  const [query, setQuery] = useState("");
  const results = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    return project.nodes
      .filter((node) =>
        normalized
          ? `${node.title} ${node.body} ${node.tags.join(" ")}`
              .toLowerCase()
              .includes(normalized)
          : true,
      )
      .slice(0, 7);
  }, [project.nodes, query]);

  return (
    <div className="fixed inset-0 z-[70] flex justify-center bg-ink/8 pt-[12vh] backdrop-blur-[1px]">
      <section className="h-fit w-[520px] overflow-hidden rounded-[7px] border border-ink/25 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.14)]">
        <div className="flex h-13 items-center gap-3 border-b border-ink/15 px-4">
          <IconSearch size={19} stroke={1.35} />
          <input
            autoFocus
            className="h-full flex-1 border-0 bg-transparent font-serif text-[15px] outline-none"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Find a node, tag, or note…"
          />
          <button className="icon-quiet" onClick={onClose} aria-label="Close search">
            <IconX size={18} stroke={1.35} />
          </button>
        </div>
        <div className="p-2">
          {results.map((node) => (
            <button
              key={node.id}
              className="flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left hover:bg-blue-soft"
              onClick={() => onSelect(node.id)}
            >
              <span className="w-16 font-sans text-[8px] uppercase tracking-[0.12em] text-ink/45">
                {node.type}
              </span>
              <span className="min-w-0 flex-1 truncate font-serif text-[13px]">
                {node.title}
              </span>
              <IconArrowRight size={16} stroke={1.3} className="text-ink/45" />
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

export function ProjectMenu({
  project,
  onClose,
  onReset,
  onSettings,
  onPlugins,
}: {
  project: ProjectState;
  onClose: () => void;
  onReset: () => void;
  onSettings: () => void;
  onPlugins: () => void;
}) {
  const { t } = useI18n();
  return (
    <aside className="fixed bottom-0 left-0 top-12 z-[60] flex w-[290px] flex-col border-r border-ink/20 bg-paper shadow-[12px_0_40px_rgba(30,32,35,.08)]">
      <div className="flex h-14 items-center justify-between border-b border-ink/15 px-5">
        <h2 className="font-serif text-[18px]">{t("workspace.projects")}</h2>
        <button className="icon-quiet" onClick={onClose} aria-label="Close menu">
          <IconX size={18} stroke={1.35} />
        </button>
      </div>
      <div className="min-h-0 flex-1 p-4">
        <p className="font-sans text-[9px] uppercase tracking-[0.16em] text-ink/45">
          {t("workspace.currentStudy")}
        </p>
        <div className="mt-3 rounded-[5px] border border-blue/30 bg-blue-soft p-3">
          <div className="flex items-center gap-2">
            <IconFolder size={18} stroke={1.35} className="text-blue" />
            <span className="font-serif text-[14px]">{project.title}</span>
          </div>
          <p className="mt-1 pl-6 font-serif text-[10px] text-ink/55">
            {project.nodes.length} {t("workspace.nodes")} · {project.edges.length}{" "}
            {t("workspace.relations")}
          </p>
        </div>

        <div className="mt-6 space-y-1">
          {[
            [IconFileText, t("workspace.evidenceLibrary"), `${project.evidence.length} ${t("workspace.sources")}`],
            [IconHistory, t("workspace.researchHistory"), `${t("workspace.revision")} ${project.revision}`],
          ].map(([Icon, label, meta]) => (
            <button
              key={String(label)}
              className="flex w-full items-center gap-3 rounded-[4px] px-2 py-2.5 text-left hover:bg-blue-soft"
            >
              <Icon size={18} stroke={1.35} />
              <span className="flex-1 font-serif text-[13px]">{String(label)}</span>
              <span className="font-serif text-[9px] text-ink/45">{String(meta)}</span>
            </button>
          ))}
        </div>

        <button
          className="mt-8 inline-flex items-center gap-2 font-serif text-[11px] text-ink/60 hover:text-blue"
          onClick={onReset}
        >
          <IconRefresh size={16} stroke={1.35} />
          {t("workspace.restoreDemo")}
        </button>
      </div>
      <div className="border-t border-ink/15 p-3">
        <button
          className="flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left transition hover:bg-blue-soft hover:text-blue"
          onClick={onPlugins}
        >
          <IconPlugConnected size={18} stroke={1.35} />
          <span className="flex-1 font-serif text-[13px]">{t("workspace.pluginStore")}</span>
          <IconArrowRight size={15} stroke={1.3} className="text-ink/40" />
        </button>
        <button
          className="flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left transition hover:bg-blue-soft hover:text-blue"
          onClick={onSettings}
        >
          <IconSettings size={18} stroke={1.35} />
          <span className="flex-1 font-serif text-[13px]">{t("workspace.settings")}</span>
          <IconArrowRight size={15} stroke={1.3} className="text-ink/40" />
        </button>
      </div>
    </aside>
  );
}

type SettingsSection = "interface" | "interaction" | "shortcuts" | "canvas";

function PreferenceToggle({
  checked,
  label,
  description,
  onChange,
}: {
  checked: boolean;
  label: string;
  description: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex cursor-pointer items-center gap-4 border-b border-ink/12 py-4">
      <span className="min-w-0 flex-1">
        <span className="block font-serif text-[13px]">{label}</span>
        <span className="mt-1 block font-serif text-[10px] leading-[1.45] text-ink/50">
          {description}
        </span>
      </span>
      <input
        className="peer sr-only"
        type="checkbox"
        checked={checked}
        onChange={(event) => onChange(event.target.checked)}
      />
      <span className="relative h-5 w-9 shrink-0 rounded-full border border-ink/25 bg-ink/10 transition peer-checked:border-blue peer-checked:bg-blue peer-focus-visible:ring-2 peer-focus-visible:ring-blue/30">
        <span
          className={`absolute top-[2px] size-3.5 rounded-full bg-paper shadow-sm transition ${
            checked ? "left-[18px]" : "left-[2px]"
          }`}
        />
      </span>
    </label>
  );
}

/**
 * Focused settings surface for interface, hover, and canvas preferences.
 * 面向界面、悬停和画布偏好的专注设置界面。
 */
export function SettingsDialog({
  preferences,
  onClose,
  onSave,
}: {
  preferences: WorkspacePreferences;
  onClose: () => void;
  onSave: (preferences: WorkspacePreferences) => void;
}) {
  const { locale, setLocale, t } = useI18n();
  const [section, setSection] = useState<SettingsSection>("interface");
  const [draft, setDraft] = useState(preferences);
  const [recordingShortcut, setRecordingShortcut] = useState<ShortcutAction | null>(null);
  const conflicts = shortcutConflicts(draft.shortcuts);
  const sections: Array<{
    key: SettingsSection;
    label: string;
    description: string;
    icon: typeof IconSettings;
  }> = [
    { key: "interface", label: t("settings.interface"), description: t("settings.commandDensity"), icon: IconPalette },
    { key: "interaction", label: t("settings.interaction"), description: t("settings.hoverBehavior"), icon: IconPointer },
    { key: "shortcuts", label: t("settings.shortcuts"), description: t("settings.shortcutsHint"), icon: IconKeyboard },
    { key: "canvas", label: t("settings.canvas"), description: t("settings.graphDefaults"), icon: IconLayoutGrid },
  ];
  const shortcutRows: Array<{ action: ShortcutAction; label: string }> = [
    { action: "menu", label: t("shortcut.menu") },
    { action: "add", label: t("shortcut.add") },
    { action: "connect", label: t("shortcut.connect") },
    { action: "note", label: t("shortcut.note") },
    { action: "find", label: t("shortcut.find") },
    { action: "layout", label: t("shortcut.layout") },
    { action: "undo", label: t("shortcut.undo") },
    { action: "redo", label: t("shortcut.redo") },
    { action: "export", label: t("shortcut.export") },
    { action: "settings", label: t("shortcut.settings") },
  ];

  const recordShortcut = (
    action: ShortcutAction,
    event: ReactKeyboardEvent<HTMLButtonElement>,
  ) => {
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") {
      setRecordingShortcut(null);
      return;
    }
    if (event.key === "Backspace" || event.key === "Delete") {
      setDraft((current) => ({
        ...current,
        shortcuts: { ...current.shortcuts, [action]: "" },
      }));
      setRecordingShortcut(null);
      return;
    }
    const binding = shortcutFromKeyboardEvent(event.nativeEvent);
    if (!binding) return;
    setDraft((current) => ({
      ...current,
      shortcuts: { ...current.shortcuts, [action]: binding },
    }));
    setRecordingShortcut(null);
  };

  return (
    <div className="fixed inset-0 z-[95] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <section
        className="grid h-[580px] w-[760px] grid-cols-[190px_minmax(0,1fr)] overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
      >
        <aside className="border-r border-ink/15 bg-canvas p-3">
          <div className="px-3 pb-5 pt-3">
            <span className="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">
              Research Canvas
            </span>
            <h2 id="settings-title" className="mt-1 font-serif text-[20px]">
              {t("settings.title")}
            </h2>
          </div>
          <nav className="space-y-1" aria-label="Settings sections">
            {sections.map((item) => {
              const SectionIcon = item.icon;
              const selected = section === item.key;
              return (
                <button
                  key={item.key}
                  className={`flex w-full items-start gap-3 rounded-[4px] px-3 py-2.5 text-left transition ${
                    selected ? "bg-blue-soft text-blue" : "hover:bg-ink/5"
                  }`}
                  onClick={() => setSection(item.key)}
                >
                  <SectionIcon className="mt-0.5" size={17} stroke={1.35} />
                  <span>
                    <span className="block font-serif text-[12px]">{item.label}</span>
                    <span className="mt-0.5 block font-serif text-[9px] text-ink/45">
                      {item.description}
                    </span>
                  </span>
                </button>
              );
            })}
          </nav>
        </aside>

        <div className="flex min-h-0 min-w-0 flex-col">
          <div className="flex h-14 items-center justify-between border-b border-ink/15 px-6">
            <div>
              <p className="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
                Workspace preferences
              </p>
              <p className="mt-0.5 font-serif text-[13px] capitalize">{section}</p>
            </div>
            <button className="icon-quiet" onClick={onClose} aria-label="Close settings">
              <IconX size={18} stroke={1.35} />
            </button>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto px-7 py-6">
            {section === "interface" && (
              <div>
                <h3 className="font-serif text-[18px]">{t("settings.commandBar")}</h3>
                <p className="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
                  {t("settings.commandBarHint")}
                </p>
                <div className="mt-6 grid grid-cols-2 gap-3">
                  {(["comfortable", "compact"] as const).map((density) => {
                    const selected = draft.commandDensity === density;
                    return (
                      <button
                        key={density}
                        className={`rounded-[5px] border p-4 text-left transition ${
                          selected
                            ? "border-blue bg-blue-soft text-blue"
                            : "border-ink/20 hover:border-ink/40"
                        }`}
                        onClick={() => setDraft((current) => ({ ...current, commandDensity: density }))}
                      >
                        <span className="font-serif text-[13px] capitalize">
                          {density === "comfortable"
                            ? t("settings.comfortable")
                            : t("settings.compact")}
                        </span>
                        <span className="mt-1 block font-serif text-[10px] leading-[1.4] text-ink/50">
                          {density === "comfortable"
                            ? "More breathing room between commands."
                            : "Tighter controls for smaller displays."}
                        </span>
                      </button>
                    );
                  })}
                </div>
                <div className="mt-7">
                  <p className="font-sans text-[8px] uppercase tracking-[0.15em] text-ink/45">
                    {t("settings.language")}
                  </p>
                  <div className="mt-2 flex gap-2">
                    {(["en", "zh-CN"] as const).map((candidate) => (
                      <button
                        key={candidate}
                        className={locale === candidate ? "button-primary" : "button-secondary"}
                        onClick={() => setLocale(candidate)}
                      >
                        {candidate === "en" ? t("settings.english") : t("settings.chinese")}
                      </button>
                    ))}
                  </div>
                </div>
                <div className="mt-7 rounded-[5px] border border-ink/15 bg-canvas p-4">
                  <p className="font-sans text-[8px] uppercase tracking-[0.15em] text-ink/45">
                    Color system
                  </p>
                  <p className="mt-2 font-serif text-[12px]">White canvas · gray-black text · blue state</p>
                  <p className="mt-1 font-serif text-[10px] text-ink/50">
                    The focused visual language remains locked to the current product direction.
                  </p>
                </div>
              </div>
            )}

            {section === "interaction" && (
              <div>
                <h3 className="font-serif text-[18px]">{t("settings.hoverDisclosure")}</h3>
                <p className="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
                  {t("settings.hoverDisclosureHint")}
                </p>
                <label className="dialog-field mt-6">
                  {t("settings.openingDelay")}
                  <select
                    value={draft.hoverDelay}
                    onChange={(event) =>
                      setDraft((current) => ({
                        ...current,
                        hoverDelay: Number(event.target.value) as WorkspacePreferences["hoverDelay"],
                      }))
                    }
                  >
                    <option value={80}>Fast · 80 ms</option>
                    <option value={180}>Balanced · 180 ms</option>
                    <option value={320}>Deliberate · 320 ms</option>
                  </select>
                </label>
                <div className="mt-4 rounded-[5px] border border-blue/20 bg-blue-soft p-4">
                  <p className="font-serif text-[12px] text-blue">Click behavior remains direct</p>
                  <p className="mt-1 font-serif text-[10px] leading-[1.45] text-ink/55">
                    Clicking Add opens the radial chooser; clicking Connect toggles the selected relation mode.
                  </p>
                </div>
              </div>
            )}

            {section === "shortcuts" && (
              <div>
                <h3 className="font-serif text-[18px]">{t("settings.shortcutBindings")}</h3>
                <p className="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
                  {t("settings.shortcutBindingsHint")}
                </p>
                <div className="mt-4 divide-y divide-ink/10 border-y border-ink/15">
                  {shortcutRows.map(({ action, label }) => {
                    const conflict = conflicts.has(action);
                    const recording = recordingShortcut === action;
                    return (
                      <div key={action} className="flex min-h-9 items-center gap-3 py-1.5">
                        <span className="min-w-0 flex-1 font-serif text-[12px]">{label}</span>
                        <button
                          className={`min-w-[112px] rounded-[4px] border px-3 py-1.5 text-center font-sans text-[10px] font-semibold transition focus-visible:outline-2 focus-visible:outline-blue ${
                            conflict
                              ? "border-alert/65 bg-alert/5 text-alert"
                              : recording
                                ? "border-blue bg-blue-soft text-blue"
                                : "border-ink/20 bg-paper hover:border-blue/55 hover:text-blue"
                          }`}
                          onClick={() => setRecordingShortcut(action)}
                          onKeyDown={
                            recording ? (event) => recordShortcut(action, event) : undefined
                          }
                          aria-label={`${label}: ${draft.shortcuts[action] || t("settings.unassigned")}`}
                        >
                          {recording
                            ? t("settings.pressShortcut")
                            : draft.shortcuts[action] || t("settings.unassigned")}
                        </button>
                        <button
                          className="w-10 text-right font-serif text-[9px] text-ink/40 hover:text-blue"
                          onClick={() =>
                            setDraft((current) => ({
                              ...current,
                              shortcuts: {
                                ...current.shortcuts,
                                [action]: defaultWorkspaceShortcuts[action],
                              },
                            }))
                          }
                        >
                          {t("settings.resetBinding")}
                        </button>
                      </div>
                    );
                  })}
                </div>
                <p className={`mt-3 font-serif text-[10px] ${conflicts.size ? "text-alert" : "text-ink/45"}`}>
                  {conflicts.size
                    ? t("settings.shortcutConflict")
                    : t("settings.shortcutCaptureHint")}
                </p>
              </div>
            )}

            {section === "canvas" && (
              <div>
                <h3 className="font-serif text-[18px]">{t("settings.graphDefaults")}</h3>
                <p className="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
                  {t("settings.graphDefaultsHint")}
                </p>
                <label className="dialog-field mt-6">
                  {t("settings.defaultFilterLayout")}
                  <select
                    value={draft.defaultLayout}
                    onChange={(event) =>
                      setDraft((current) => ({
                        ...current,
                        defaultLayout: event.target.value as WorkspacePreferences["defaultLayout"],
                      }))
                    }
                  >
                    {layoutOptions.map((option) => (
                      <option key={option.mode} value={option.mode}>
                        {option.label}
                      </option>
                    ))}
                  </select>
                </label>
                <div className="mt-3">
                  <PreferenceToggle
                    checked={draft.showMiniMap}
                    label={t("settings.showMinimap")}
                    description={t("settings.showMinimapHint")}
                    onChange={(showMiniMap) =>
                      setDraft((current) => ({ ...current, showMiniMap }))
                    }
                  />
                  <PreferenceToggle
                    checked={draft.showLinkCounts}
                    label={t("settings.showLinkCounts")}
                    description={t("settings.showLinkCountsHint")}
                    onChange={(showLinkCounts) =>
                      setDraft((current) => ({ ...current, showLinkCounts }))
                    }
                  />
                </div>
              </div>
            )}
          </div>

          <footer className="flex h-16 items-center justify-between border-t border-ink/15 px-6">
            <button
              className="font-serif text-[11px] text-ink/55 hover:text-blue"
              onClick={() => setDraft(defaultWorkspacePreferences)}
            >
              {t("settings.restore")}
            </button>
            <div className="flex gap-2">
              <button className="button-secondary" onClick={onClose}>
                {t("settings.cancel")}
              </button>
              <button
                className="button-primary disabled:cursor-not-allowed disabled:opacity-40"
                onClick={() => onSave(draft)}
                disabled={conflicts.size > 0}
              >
                {t("settings.save")}
                <IconCheck size={15} stroke={1.45} />
              </button>
            </div>
          </footer>
        </div>
      </section>
    </div>
  );
}
