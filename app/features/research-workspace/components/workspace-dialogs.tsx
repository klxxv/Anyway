"use client";

import {
  IconArrowRight,
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

export function NodeComposer({
  state,
  onClose,
  onCreate,
}: {
  state: ComposerState;
  onClose: () => void;
  onCreate: (draft: NodeDraft, x: number, y: number) => void;
}) {
  const [draft, setDraft] = useState<NodeDraft>({
    title: "",
    body: "",
    type: state.type,
  });

  return (
    <div className="fixed inset-0 z-[80] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <form
        className="w-[390px] rounded-[7px] border border-ink/30 bg-paper p-5 shadow-[0_18px_60px_rgba(30,32,35,.15)]"
        onSubmit={(event) => {
          event.preventDefault();
          onCreate(draft, state.x, state.y);
        }}
      >
        <div className="mb-5 flex items-start justify-between">
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

        <label className="dialog-field">
          Type
          <select
            value={draft.type}
            onChange={(event) =>
              setDraft((current) => ({
                ...current,
                type: event.target.value as ResearchNodeType,
              }))
            }
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
        <label className="dialog-field">
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
        <label className="dialog-field">
          Note
          <textarea
            value={draft.body}
            onChange={(event) =>
              setDraft((current) => ({ ...current, body: event.target.value }))
            }
            placeholder="Add a precise definition or observation"
          />
        </label>

        <div className="mt-5 flex justify-end gap-2">
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
