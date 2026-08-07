"use client";

import {
  IconAdjustmentsHorizontal,
  IconAlertTriangle,
  IconArrowBackUp,
  IconArrowForwardUp,
  IconArrowUpRight,
  IconBinaryTree,
  IconChartHistogram,
  IconCheck,
  IconChevronDown,
  IconDatabase,
  IconFileText,
  IconFlag3,
  IconFlask2,
  IconGitBranch,
  IconGitCompare,
  IconHelp,
  IconHierarchy2,
  IconListTree,
  IconMenu2,
  IconNetwork,
  IconNote,
  IconRouteAltLeft,
  IconSearch,
  IconSparkles,
  IconTable,
  IconUsersGroup,
} from "@tabler/icons-react";
import { useCallback, useRef, useState } from "react";
import type { MessageKey } from "../../../i18n/catalog";
import { useI18n } from "../../../i18n/provider";
import type {
  LayoutMode,
  ResearchEdgeType,
  ResearchNodeType,
} from "../../../lib/research-types";
import { layoutOptions } from "../workspace-layout";
import type {
  CommandDensity,
  HoverDelay,
} from "../workspace-preferences";
import {
  shortcutToAria,
  type WorkspaceShortcuts,
} from "../workspace-shortcuts";

type WorkspaceTopbarProps = {
  canUndo: boolean;
  canRedo: boolean;
  connectMode: boolean;
  connectType: ResearchEdgeType;
  commandDensity: CommandDensity;
  hoverDelay: HoverDelay;
  shortcuts: WorkspaceShortcuts;
  onMenu: () => void;
  onAdd: () => void;
  onAddType: (type: ResearchNodeType) => void;
  onConnect: () => void;
  onConnectType: (type: ResearchEdgeType) => void;
  onNote: () => void;
  onFind: () => void;
  activeLayout: LayoutMode | null;
  onLayout: (mode: LayoutMode) => void;
  onCompare: () => void;
  onUndo: () => void;
  onRedo: () => void;
  onExport: () => void;
  exportFormats?: Array<"pdf" | "svg" | "png">;
  onExportFormat?: (format: "pdf" | "svg" | "png") => void;
};

type MenuOption<T extends string> = {
  value: T;
  labelKey: MessageKey;
  descriptionKey: MessageKey;
  icon: typeof IconSparkles;
};

const nodeOptions: Array<MenuOption<ResearchNodeType>> = [
  { value: "question", labelKey: "node.question", descriptionKey: "node.questionDesc", icon: IconHelp },
  { value: "concept", labelKey: "node.group", descriptionKey: "node.groupDesc", icon: IconUsersGroup },
  { value: "variable", labelKey: "node.variable", descriptionKey: "node.variableDesc", icon: IconChartHistogram },
  { value: "method", labelKey: "node.method", descriptionKey: "node.methodDesc", icon: IconFlask2 },
  { value: "dataset", labelKey: "node.data", descriptionKey: "node.dataDesc", icon: IconDatabase },
  { value: "evidence", labelKey: "node.evidence", descriptionKey: "node.evidenceDesc", icon: IconFileText },
  { value: "result", labelKey: "node.result", descriptionKey: "node.resultDesc", icon: IconCheck },
  { value: "note", labelKey: "node.note", descriptionKey: "node.noteDesc", icon: IconNote },
];

const connectionOptions: Array<MenuOption<ResearchEdgeType>> = [
  { value: "causes", labelKey: "relation.causal", descriptionKey: "relation.causalDesc", icon: IconArrowUpRight },
  { value: "controls", labelKey: "relation.control", descriptionKey: "relation.controlDesc", icon: IconAdjustmentsHorizontal },
  { value: "derived_from", labelKey: "relation.derived", descriptionKey: "relation.derivedDesc", icon: IconGitBranch },
  { value: "contradicts", labelKey: "relation.contradicts", descriptionKey: "relation.contradictsDesc", icon: IconAlertTriangle },
];

const layoutMessageKeys: Record<LayoutMode, [MessageKey, MessageKey]> = {
  "evidence-chain": ["layout.evidenceChain", "layout.evidenceChainDesc"],
  "refutation-chain": ["layout.refutationChain", "layout.refutationChainDesc"],
  tree: ["layout.tree", "layout.treeDesc"],
  huffman: ["layout.huffman", "layout.huffmanDesc"],
  table: ["layout.table", "layout.tableDesc"],
  "neural-network": ["layout.neural", "layout.neuralDesc"],
};

const layoutIcons: Record<LayoutMode, typeof IconSparkles> = {
  "evidence-chain": IconListTree,
  "refutation-chain": IconRouteAltLeft,
  tree: IconBinaryTree,
  huffman: IconHierarchy2,
  table: IconTable,
  "neural-network": IconNetwork,
};

function useHoverDisclosure(delay: HoverDelay) {
  const [open, setOpen] = useState(false);
  const timer = useRef<number | null>(null);
  const clearTimer = useCallback(() => {
    if (timer.current) window.clearTimeout(timer.current);
    timer.current = null;
  }, []);
  const openSoon = useCallback(() => {
    clearTimer();
    timer.current = window.setTimeout(() => setOpen(true), delay);
  }, [clearTimer, delay]);
  const closeSoon = useCallback(() => {
    clearTimer();
    timer.current = window.setTimeout(() => setOpen(false), 120);
  }, [clearTimer]);
  return { open, setOpen, openSoon, closeSoon, clearTimer };
}

function commandClass(density: CommandDensity) {
  return `group relative inline-flex h-12 items-center gap-2 border-r border-ink/10 font-serif text-[15px] text-ink transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-offset-[-2px] focus-visible:outline-blue ${
    density === "compact" ? "px-3.5" : "px-5"
  }`;
}

function ShortcutTooltip({ binding }: { binding: string }) {
  if (!binding) return null;
  return (
    <span
      className="pointer-events-none absolute left-1/2 top-[52px] z-[100] -translate-x-1/2 whitespace-nowrap rounded-[4px] border border-ink/20 bg-ink px-2 py-1 font-sans text-[9px] font-medium tracking-wide text-paper opacity-0 shadow-sm transition-opacity delay-150 group-hover:opacity-100 group-focus-visible:opacity-100"
      role="tooltip"
    >
      {binding}
    </span>
  );
}

function HoverCommandMenu<T extends string>({
  label,
  icon: TriggerIcon,
  options,
  selected,
  active,
  density,
  hoverDelay,
  shortcut,
  onPrimary,
  onChoose,
}: {
  label: string;
  icon: typeof IconSparkles;
  options: Array<MenuOption<T>>;
  selected?: T | null;
  active?: boolean;
  density: CommandDensity;
  hoverDelay: HoverDelay;
  shortcut: string;
  onPrimary: () => void;
  onChoose: (value: T) => void;
}) {
  const { t } = useI18n();
  const disclosure = useHoverDisclosure(hoverDelay);
  return (
    <div
      className="relative"
      onMouseEnter={disclosure.openSoon}
      onMouseLeave={disclosure.closeSoon}
      onFocusCapture={() => {
        disclosure.clearTimer();
        disclosure.setOpen(true);
      }}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) disclosure.setOpen(false);
      }}
    >
      <button
        className={`${commandClass(density)} ${active ? "bg-blue-soft text-blue" : ""}`}
        onClick={() => {
          onPrimary();
          disclosure.setOpen(false);
        }}
        aria-expanded={disclosure.open}
        aria-haspopup="menu"
        aria-keyshortcuts={shortcutToAria(shortcut)}
      >
        <TriggerIcon size={19} stroke={1.4} />
        {label}
        <IconChevronDown
          size={13}
          stroke={1.35}
          className={`ml-0.5 transition-transform ${disclosure.open ? "rotate-180" : ""}`}
        />
      </button>
      {disclosure.open && (
        <div
          className="absolute left-2 top-[46px] z-[90] w-[286px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]"
          role="menu"
          aria-label={`${label} options`}
        >
          <div className="flex items-center justify-between px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            <span>
              {label === t("workspace.add")
                ? t("workspace.createObject")
                : t("workspace.chooseRelation")}
            </span>
            {shortcut && <kbd className="shortcut-key">{shortcut}</kbd>}
          </div>
          {options.map((option) => {
            const OptionIcon = option.icon;
            const isSelected = selected === option.value;
            return (
              <button
                key={option.value}
                className={`flex w-full items-start gap-3 rounded-[4px] px-3 py-2 text-left transition ${
                  isSelected ? "bg-blue-soft text-blue" : "hover:bg-ink/5"
                }`}
                role="menuitemradio"
                aria-checked={isSelected}
                onClick={() => {
                  onChoose(option.value);
                  disclosure.setOpen(false);
                }}
              >
                <OptionIcon className="mt-0.5 shrink-0" size={17} stroke={1.35} />
                <span className="min-w-0">
                  <span className="block font-serif text-[12px]">{t(option.labelKey)}</span>
                  <span className="mt-0.5 block font-serif text-[9px] leading-[1.35] text-ink/50">
                    {t(option.descriptionKey)}
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

/**
 * Low-weight command bar with hover-disclosed semantic actions.
 * 低视觉重量命令栏；通过悬停展开语义动作。
 */
export function WorkspaceTopbar({
  canUndo,
  canRedo,
  connectMode,
  connectType,
  commandDensity,
  hoverDelay,
  shortcuts,
  onMenu,
  onAdd,
  onAddType,
  onConnect,
  onConnectType,
  onNote,
  onFind,
  activeLayout,
  onLayout,
  onCompare,
  onUndo,
  onRedo,
  onExport,
  exportFormats = [],
  onExportFormat,
}: WorkspaceTopbarProps) {
  const { t } = useI18n();
  return (
    <header className="flex h-12 shrink-0 items-center justify-between border-b border-ink/15 bg-paper">
      <nav className="flex h-full items-stretch" aria-label="Workspace commands">
        <button
          className={commandClass(commandDensity)}
          onClick={onMenu}
          aria-keyshortcuts={shortcutToAria(shortcuts.menu)}
        >
          <IconMenu2 size={19} stroke={1.45} />
          {t("workspace.menu")}
          <ShortcutTooltip binding={shortcuts.menu} />
        </button>
        <HoverCommandMenu
          label={t("workspace.add")}
          icon={IconSparkles}
          options={nodeOptions}
          density={commandDensity}
          hoverDelay={hoverDelay}
          shortcut={shortcuts.add}
          onPrimary={onAdd}
          onChoose={onAddType}
        />
        <HoverCommandMenu
          label={t("workspace.connect")}
          icon={IconArrowUpRight}
          options={connectionOptions}
          selected={connectType}
          active={connectMode}
          density={commandDensity}
          hoverDelay={hoverDelay}
          shortcut={shortcuts.connect}
          onPrimary={onConnect}
          onChoose={onConnectType}
        />
        <button
          className={commandClass(commandDensity)}
          onClick={onNote}
          aria-keyshortcuts={shortcutToAria(shortcuts.note)}
        >
          <IconNote size={19} stroke={1.45} />
          {t("workspace.note")}
          <ShortcutTooltip binding={shortcuts.note} />
        </button>
        <button
          className={commandClass(commandDensity)}
          onClick={onFind}
          aria-keyshortcuts={shortcutToAria(shortcuts.find)}
        >
          <IconSearch size={19} stroke={1.45} />
          {t("workspace.find")}
          <ShortcutTooltip binding={shortcuts.find} />
        </button>
        <LayoutMenu
          activeLayout={activeLayout}
          density={commandDensity}
          hoverDelay={hoverDelay}
          shortcut={shortcuts.layout}
          onLayout={onLayout}
        />
        <button
          className={commandClass(commandDensity)}
          onClick={onCompare}
          aria-label={t("diff.compare")}
        >
          <IconGitCompare size={19} stroke={1.4} />
          {t("diff.compare")}
        </button>
      </nav>

      <nav className="flex h-full items-stretch" aria-label="History and export">
        <button
          className={commandClass(commandDensity)}
          onClick={onUndo}
          disabled={!canUndo}
          aria-label="Undo"
          aria-keyshortcuts={shortcutToAria(shortcuts.undo)}
        >
          <IconArrowBackUp size={19} stroke={1.45} />
          {t("workspace.undo")}
          <ShortcutTooltip binding={shortcuts.undo} />
        </button>
        <button
          className={commandClass(commandDensity)}
          onClick={onRedo}
          disabled={!canRedo}
          aria-label="Redo"
          aria-keyshortcuts={shortcutToAria(shortcuts.redo)}
        >
          <IconArrowForwardUp size={19} stroke={1.45} />
          {t("workspace.redo")}
          <ShortcutTooltip binding={shortcuts.redo} />
        </button>
        <ExportMenu
          density={commandDensity}
          hoverDelay={hoverDelay}
          shortcut={shortcuts.export}
          formats={exportFormats}
          onPrimary={onExport}
          onFormat={onExportFormat}
        />
      </nav>
    </header>
  );
}

function ExportMenu({
  density,
  hoverDelay,
  shortcut,
  formats,
  onPrimary,
  onFormat,
}: {
  density: CommandDensity;
  hoverDelay: HoverDelay;
  shortcut: string;
  formats: Array<"pdf" | "svg" | "png">;
  onPrimary: () => void;
  onFormat?: (format: "pdf" | "svg" | "png") => void;
}) {
  const { t } = useI18n();
  const disclosure = useHoverDisclosure(hoverDelay);
  const hasPluginFormats = formats.length > 0 && Boolean(onFormat);
  return (
    <div
      className="relative"
      onMouseEnter={hasPluginFormats ? disclosure.openSoon : undefined}
      onMouseLeave={hasPluginFormats ? disclosure.closeSoon : undefined}
      onFocusCapture={() => {
        if (!hasPluginFormats) return;
        disclosure.clearTimer();
        disclosure.setOpen(true);
      }}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) disclosure.setOpen(false);
      }}
    >
      <button
        className={`${commandClass(density)} border-l border-r-0`}
        onClick={() => {
          onPrimary();
          disclosure.setOpen(false);
        }}
        aria-keyshortcuts={shortcutToAria(shortcut)}
        aria-haspopup={hasPluginFormats ? "menu" : undefined}
        aria-expanded={hasPluginFormats ? disclosure.open : undefined}
      >
        <IconFlag3 size={19} stroke={1.4} />
        {t("workspace.export")}
        {hasPluginFormats && <IconChevronDown size={13} stroke={1.35} />}
        <ShortcutTooltip binding={shortcut} />
      </button>
      {hasPluginFormats && disclosure.open && (
        <div
          className="absolute right-2 top-[46px] z-[90] w-[220px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]"
          role="menu"
          aria-label={t("workspace.export")}
        >
          <div className="flex items-center justify-between px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            <span>{t("workspace.export")}</span>
            {shortcut && <kbd className="shortcut-key">{shortcut}</kbd>}
          </div>
          {formats.map((format) => (
            <button
              key={format}
              className="flex w-full items-center gap-3 rounded-[4px] px-3 py-2 text-left transition hover:bg-blue-soft hover:text-blue"
              role="menuitem"
              onClick={() => {
                onFormat?.(format);
                disclosure.setOpen(false);
              }}
            >
              <IconFileText size={17} stroke={1.35} />
              <span className="font-serif text-[12px]">{format.toUpperCase()}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

function LayoutMenu({
  activeLayout,
  density,
  hoverDelay,
  shortcut,
  onLayout,
}: {
  activeLayout: LayoutMode | null;
  density: CommandDensity;
  hoverDelay: HoverDelay;
  shortcut: string;
  onLayout: (mode: LayoutMode) => void;
}) {
  const { t } = useI18n();
  const disclosure = useHoverDisclosure(hoverDelay);
  return (
    <div
      className="relative"
      onMouseEnter={disclosure.openSoon}
      onMouseLeave={disclosure.closeSoon}
      onFocusCapture={() => {
        disclosure.clearTimer();
        disclosure.setOpen(true);
      }}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) disclosure.setOpen(false);
      }}
    >
      <button
        className={commandClass(density)}
        onClick={() => disclosure.setOpen((current) => !current)}
        aria-expanded={disclosure.open}
        aria-haspopup="menu"
        aria-keyshortcuts={shortcutToAria(shortcut)}
      >
        <IconHierarchy2 size={19} stroke={1.35} />
        {t("workspace.layout")}
        <IconChevronDown
          size={14}
          stroke={1.35}
          className={`transition-transform ${disclosure.open ? "rotate-180" : ""}`}
        />
      </button>
      {disclosure.open && (
        <div
          className="absolute left-2 top-[46px] z-[90] w-[286px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper p-1.5 shadow-[0_14px_40px_rgba(30,32,35,.14)]"
          role="menu"
          aria-label="Layout mode"
        >
          <div className="flex items-center justify-between px-3 pb-2 pt-1 font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
            <span>{t("workspace.arrangeResearch")}</span>
            {shortcut && <kbd className="shortcut-key">{shortcut}</kbd>}
          </div>
          {layoutOptions.map((option) => {
            const selected = option.mode === activeLayout;
            const LayoutIcon = layoutIcons[option.mode];
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
                  disclosure.setOpen(false);
                }}
              >
                <LayoutIcon className="mt-0.5 shrink-0" size={17} stroke={1.35} />
                <span className="min-w-0 flex-1">
                  <span className="block font-serif text-[12px]">
                    {t(layoutMessageKeys[option.mode][0])}
                  </span>
                  <span className="mt-0.5 block font-serif text-[9px] leading-[1.35] text-ink/50">
                    {t(layoutMessageKeys[option.mode][1])}
                  </span>
                </span>
                {selected && <IconCheck className="mt-0.5" size={14} stroke={1.6} />}
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
