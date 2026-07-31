"use client";

import {
  IconArrowBackUp,
  IconArrowForwardUp,
  IconArrowUpRight,
  IconCheck,
  IconChevronDown,
  IconFlag3,
  IconMenu2,
  IconNote,
  IconSearch,
  IconSettings,
  IconSparkles,
} from "@tabler/icons-react";
import { useState } from "react";
import type { LayoutMode } from "../../../lib/research-types";
import { layoutOptions } from "../workspace-layout";

type WorkspaceTopbarProps = {
  canUndo: boolean;
  canRedo: boolean;
  onMenu: () => void;
  onAdd: () => void;
  onConnect: () => void;
  onNote: () => void;
  onFind: () => void;
  activeLayout: LayoutMode | null;
  onLayout: (mode: LayoutMode) => void;
  onUndo: () => void;
  onRedo: () => void;
  onExport: () => void;
};

const commandClass =
  "group inline-flex h-12 items-center gap-2 border-r border-ink/10 px-5 font-serif text-[15px] text-ink transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-blue";

/**
 * Reference-faithful command bar with low visual weight and complete keyboard focus.
 * 忠于参考图的低视觉重量命令栏，并保留完整键盘焦点。
 */
export function WorkspaceTopbar({
  canUndo,
  canRedo,
  onMenu,
  onAdd,
  onConnect,
  onNote,
  onFind,
  activeLayout,
  onLayout,
  onUndo,
  onRedo,
  onExport,
}: WorkspaceTopbarProps) {
  return (
    <header className="flex h-12 shrink-0 items-center justify-between border-b border-ink/15 bg-paper">
      <nav className="flex h-full items-stretch" aria-label="Workspace commands">
        <button className={commandClass} onClick={onMenu}>
          <IconMenu2 size={19} stroke={1.45} />
          Menu
        </button>
        <button className={commandClass} onClick={onAdd}>
          <IconSparkles size={19} stroke={1.35} />
          Add
        </button>
        <button className={commandClass} onClick={onConnect}>
          <IconArrowUpRight size={19} stroke={1.45} />
          Connect
        </button>
        <button className={commandClass} onClick={onNote}>
          <IconNote size={19} stroke={1.45} />
          Note
        </button>
        <button className={commandClass} onClick={onFind}>
          <IconSearch size={19} stroke={1.45} />
          Find
        </button>
        <LayoutMenu activeLayout={activeLayout} onLayout={onLayout} />
      </nav>

      <nav className="flex h-full items-stretch" aria-label="History and export">
        <button
          className={commandClass}
          onClick={onUndo}
          disabled={!canUndo}
          aria-label="Undo"
        >
          <IconArrowBackUp size={19} stroke={1.45} />
          Undo
        </button>
        <button
          className={commandClass}
          onClick={onRedo}
          disabled={!canRedo}
          aria-label="Redo"
        >
          <IconArrowForwardUp size={19} stroke={1.45} />
          Redo
        </button>
        <button className={`${commandClass} border-l border-r-0`} onClick={onExport}>
          <IconFlag3 size={19} stroke={1.4} />
          Export
        </button>
      </nav>
    </header>
  );
}

function LayoutMenu({
  activeLayout,
  onLayout,
}: {
  activeLayout: LayoutMode | null;
  onLayout: (mode: LayoutMode) => void;
}) {
  const [open, setOpen] = useState(false);
  return (
    <div
      className="relative"
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}
    >
      <button
        className={commandClass}
        onClick={() => setOpen((current) => !current)}
        aria-expanded={open}
        aria-haspopup="menu"
      >
        <IconSettings size={19} stroke={1.35} />
        Layout
        <IconChevronDown
          size={14}
          stroke={1.35}
          className={`transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>
      {open && (
        <div
          className="absolute left-2 top-[46px] z-[90] w-[270px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]"
          role="menu"
          aria-label="Layout mode"
        >
          <div className="px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            Arrange visible research
          </div>
          {layoutOptions.map((option) => {
            const selected = option.mode === activeLayout;
            return (
              <button
                key={option.mode}
                className={`flex w-full items-start gap-3 rounded-[4px] px-3 py-2 text-left transition ${
                  selected ? "bg-blue-soft text-blue" : "hover:bg-ink/5"
                }`}
                role="menuitemradio"
                aria-checked={selected}
                onClick={() => {
                  onLayout(option.mode);
                  setOpen(false);
                }}
              >
                <span className="mt-0.5 grid size-4 shrink-0 place-items-center">
                  {selected && <IconCheck size={14} stroke={1.6} />}
                </span>
                <span>
                  <span className="block font-serif text-[12px]">{option.label}</span>
                  <span className="mt-0.5 block font-serif text-[9px] leading-[1.3] text-ink/50">
                    {option.description}
                  </span>
                </span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
