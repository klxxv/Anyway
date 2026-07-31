export const SHORTCUT_ACTIONS = [
  "menu",
  "add",
  "connect",
  "note",
  "find",
  "layout",
  "undo",
  "redo",
  "export",
  "settings",
] as const;

export type ShortcutAction = (typeof SHORTCUT_ACTIONS)[number];
export type WorkspaceShortcuts = Record<ShortcutAction, string>;

export const defaultWorkspaceShortcuts: WorkspaceShortcuts = {
  menu: "Ctrl+M",
  add: "A",
  connect: "C",
  note: "N",
  find: "Ctrl+F",
  layout: "L",
  undo: "Ctrl+Z",
  redo: "Ctrl+Shift+Z",
  export: "Ctrl+E",
  settings: "Ctrl+,",
};

const modifierKeys = new Set(["Alt", "Control", "Meta", "Shift"]);

/**
 * 将键盘事件规范为稳定、可持久化的组合键字符串。
 * Normalizes a keyboard event into a stable, persistable shortcut string.
 */
export function shortcutFromKeyboardEvent(
  event: Pick<KeyboardEvent, "altKey" | "ctrlKey" | "key" | "metaKey" | "shiftKey">,
): string | null {
  if (modifierKeys.has(event.key)) return null;
  const key =
    event.key === " "
      ? "Space"
      : event.key === "Esc"
        ? "Escape"
        : event.key.length === 1
          ? event.key.toUpperCase()
          : event.key;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Meta");
  parts.push(key);
  return parts.join("+");
}

/** 返回所有冲突动作；空绑定不参与冲突检测 / Returns actions sharing a non-empty binding. */
export function shortcutConflicts(shortcuts: WorkspaceShortcuts): Set<ShortcutAction> {
  const owners = new Map<string, ShortcutAction[]>();
  SHORTCUT_ACTIONS.forEach((action) => {
    const binding = shortcuts[action];
    if (!binding) return;
    owners.set(binding, [...(owners.get(binding) ?? []), action]);
  });
  return new Set(
    [...owners.values()].filter((actions) => actions.length > 1).flat(),
  );
}

/** 将显示格式转换为 aria-keyshortcuts 语法 / Converts display syntax to aria-keyshortcuts syntax. */
export function shortcutToAria(binding: string): string | undefined {
  if (!binding) return undefined;
  return binding.replace("Ctrl", "Control");
}

export function isEditableShortcutTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}
