"use client";

import {
  IconArrowsExchange,
  IconArrowsMaximize,
  IconArrowsMinimize,
  IconCopy,
  IconDatabase,
  IconFilter,
  IconFocus2,
  IconLayout,
  IconLink,
  IconNote,
  IconPlugConnected,
  IconSearch,
  IconSparkles,
  IconTrash,
  IconWand,
} from "@tabler/icons-react";
import { useEffect, useMemo, useRef } from "react";
import { useI18n } from "../../../i18n/provider";
import type { ResolvedPluginContextMenuAction } from "../../../plugins/context-menu";
import {
  CONTEXT_MENU_ACTIONS,
  type ContextMenuActionId,
  type WorkspaceContextMenuState,
} from "../workspace-context-menu";
import type { WorkspaceShortcuts } from "../workspace-shortcuts";

const builtInIcon = {
  inspect: IconFocus2,
  connect: IconPlugConnected,
  duplicate: IconCopy,
  delete: IconTrash,
  filter: IconFilter,
  reverse: IconArrowsExchange,
  add: IconSparkles,
  note: IconNote,
  layout: IconLayout,
  fit: IconArrowsMaximize,
  expand: IconArrowsMaximize,
  collapse: IconArrowsMinimize,
} as const;

const pluginIcon = {
  sparkles: IconSparkles,
  search: IconSearch,
  wand: IconWand,
  database: IconDatabase,
  link: IconLink,
} as const;

const actionShortcut: Partial<Record<ContextMenuActionId, keyof WorkspaceShortcuts>> = {
  "canvas.add": "add",
  "canvas.note": "note",
  "canvas.layout": "layout",
};

type WorkspaceContextMenuProps = {
  menu: WorkspaceContextMenuState;
  width: number;
  height: number;
  actionOrder: ContextMenuActionId[];
  shortcuts: WorkspaceShortcuts;
  pluginActions: ResolvedPluginContextMenuAction[];
  onBuiltInAction: (action: ContextMenuActionId, menu: WorkspaceContextMenuState) => void;
  onPluginAction: (
    action: ResolvedPluginContextMenuAction,
    menu: WorkspaceContextMenuState,
  ) => void;
  onClose: () => void;
};

/** 三种目标共享同一视觉壳，操作内容由语义作用域决定 / One visual shell, three semantic action scopes. */
export function WorkspaceContextMenu({
  menu,
  width,
  height,
  actionOrder,
  shortcuts,
  pluginActions,
  onBuiltInAction,
  onPluginAction,
  onClose,
}: WorkspaceContextMenuProps) {
  const { t } = useI18n();
  const ref = useRef<HTMLDivElement | null>(null);
  const actions = useMemo(() => {
    const definitions = [
      ...CONTEXT_MENU_ACTIONS.node,
      ...CONTEXT_MENU_ACTIONS.edge,
      ...CONTEXT_MENU_ACTIONS.canvas,
    ];
    return actionOrder.flatMap((id) => {
      const definition = definitions.find((item) => item.id === id);
      return definition ? [definition] : [];
    });
  }, [actionOrder]);
  const position = {
    left: Math.max(10, Math.min(menu.screenX, width - 246)),
    top: Math.max(10, Math.min(menu.screenY, height - Math.min(460, 82 + (actions.length + pluginActions.length) * 38))),
  };

  useEffect(() => {
    const closeOnPointer = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as globalThis.Node)) onClose();
    };
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
        return;
      }
      if (event.key !== "Delete") return;
      const deleteAction =
        menu.scope === "node"
          ? "node.delete"
          : menu.scope === "edge"
            ? "edge.delete"
            : null;
      if (deleteAction && actionOrder.includes(deleteAction)) {
        event.preventDefault();
        onBuiltInAction(deleteAction, menu);
      }
    };
    window.addEventListener("pointerdown", closeOnPointer, true);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("pointerdown", closeOnPointer, true);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [actionOrder, menu, onBuiltInAction, onClose]);

  return (
    <div
      ref={ref}
      className="absolute z-[70] w-[236px] overflow-hidden rounded-[6px] border border-ink/25 bg-paper shadow-[0_14px_38px_rgba(31,34,38,.16)]"
      style={position}
      role="menu"
      aria-label={`${t(`contextMenu.${menu.scope}`)} context menu`}
      onContextMenu={(event) => event.preventDefault()}
    >
      <header className="border-b border-ink/12 px-3.5 py-2.5">
        <p className="font-sans text-[7px] uppercase tracking-[0.16em] text-blue">
          {t(`contextMenu.${menu.scope}`)}
        </p>
        {menu.title && (
          <p className="mt-1 truncate font-serif text-[11px] text-ink/65">{menu.title}</p>
        )}
      </header>

      <div className="p-1.5">
        {actions.map((action) => {
          const Icon = builtInIcon[action.icon];
          const shortcutKey = actionShortcut[action.id];
          const danger = "danger" in action && action.danger;
          const shortcut = shortcutKey ? shortcuts[shortcutKey] : danger ? "Del" : "";
          return (
            <button
              key={action.id}
              className={`group flex min-h-9 w-full items-center gap-3 rounded-[4px] px-2.5 text-left font-serif text-[11px] transition focus-visible:outline-2 focus-visible:outline-blue ${
                danger
                  ? "text-alert hover:bg-alert/5"
                  : "text-ink/85 hover:bg-blue-soft hover:text-blue"
              }`}
              role="menuitem"
              onClick={() => onBuiltInAction(action.id, menu)}
            >
              <Icon size={16} stroke={1.35} />
              <span className="min-w-0 flex-1">{t(action.labelKey)}</span>
              {shortcut && (
                <kbd className="font-sans text-[8px] font-medium text-ink/35 group-hover:text-blue/60">
                  {shortcut}
                </kbd>
              )}
            </button>
          );
        })}
      </div>

      {pluginActions.length > 0 && (
        <div className="border-t border-ink/12 p-1.5">
          <p className="px-2.5 pb-1 pt-1 font-sans text-[7px] uppercase tracking-[0.15em] text-ink/40">
            {t("contextMenu.pluginGroup")}
          </p>
          {pluginActions.map((action) => {
            const Icon = pluginIcon[action.icon];
            return (
              <button
                key={action.id}
                className="group flex min-h-9 w-full items-center gap-3 rounded-[4px] px-2.5 text-left font-serif text-[11px] text-ink/85 transition hover:bg-blue-soft hover:text-blue focus-visible:outline-2 focus-visible:outline-blue"
                role="menuitem"
                title={`${action.plugin.name} · ${t("contextMenu.pluginRun")}`}
                onClick={() => onPluginAction(action, menu)}
              >
                <Icon size={16} stroke={1.35} />
                <span className="min-w-0 flex-1 truncate">{action.label}</span>
                <span className="size-1.5 rounded-full bg-blue" aria-hidden="true" />
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
