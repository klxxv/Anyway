"use client";

import {
  IconArrowBackUp,
  IconArrowForwardUp,
  IconArrowUpRight,
  IconFlag3,
  IconMenu2,
  IconNote,
  IconSearch,
  IconSettings,
  IconSparkles,
} from "@tabler/icons-react";

type WorkspaceTopbarProps = {
  canUndo: boolean;
  canRedo: boolean;
  onMenu: () => void;
  onAdd: () => void;
  onConnect: () => void;
  onNote: () => void;
  onFind: () => void;
  onLayout: () => void;
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
        <button className={commandClass} onClick={onLayout}>
          <IconSettings size={19} stroke={1.35} />
          Layout
        </button>
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
