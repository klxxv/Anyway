"use client";

import {
  IconArrowRight,
  IconCheck,
  IconFileText,
  IconFolder,
  IconHistory,
  IconRefresh,
  IconSearch,
  IconX,
} from "@tabler/icons-react";
import { useMemo, useState } from "react";
import type { ProjectState, ResearchNodeType } from "../../../lib/research-types";
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
}: {
  project: ProjectState;
  onClose: () => void;
  onReset: () => void;
}) {
  return (
    <aside className="fixed bottom-0 left-0 top-12 z-[60] w-[290px] border-r border-ink/20 bg-paper shadow-[12px_0_40px_rgba(30,32,35,.08)]">
      <div className="flex h-14 items-center justify-between border-b border-ink/15 px-5">
        <h2 className="font-serif text-[18px]">Projects</h2>
        <button className="icon-quiet" onClick={onClose} aria-label="Close menu">
          <IconX size={18} stroke={1.35} />
        </button>
      </div>
      <div className="p-4">
        <p className="font-sans text-[9px] uppercase tracking-[0.16em] text-ink/45">
          Current study
        </p>
        <div className="mt-3 rounded-[5px] border border-blue/30 bg-blue-soft p-3">
          <div className="flex items-center gap-2">
            <IconFolder size={18} stroke={1.35} className="text-blue" />
            <span className="font-serif text-[14px]">{project.title}</span>
          </div>
          <p className="mt-1 pl-6 font-serif text-[10px] text-ink/55">
            {project.nodes.length} nodes · {project.edges.length} relations
          </p>
        </div>

        <div className="mt-6 space-y-1">
          {[
            [IconFileText, "Evidence library", `${project.evidence.length} sources`],
            [IconHistory, "Research history", `revision ${project.revision}`],
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
          Restore reference demo
        </button>
      </div>
    </aside>
  );
}
