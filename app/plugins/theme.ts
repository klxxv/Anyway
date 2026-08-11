import type { ThemeManifest } from "../lib/research-types";
import type { InstalledMycPlugin } from "./contracts";
import { normalizeInstalledTheme } from "./contracts";

export function resolveTheme(
  activePlugins: readonly InstalledMycPlugin[],
): ThemeManifest | null {
  for (let index = activePlugins.length - 1; index >= 0; index -= 1) {
    const plugin = activePlugins[index];
    if (!plugin.manifest.spec.capabilities.includes("theme.register")) continue;
    const theme = normalizeInstalledTheme(plugin);
    if (theme) return theme;
  }
  return null;
}

/** Portable inline-style map; it deliberately has no renderer dependency. */
type ThemeVariables = Record<`--${string}`, string>;

/**
 * 拒绝可终止 CSS 值或引入额外规则/函数的注入尝试。
 * Rejects values that could break out of a CSS value or introduce extra rules.
 */
export function sanitizeCssColor(value: string): string | undefined {
  const trimmed = value.trim();
  if (trimmed.length === 0 || trimmed.length > 64) return undefined;
  // 禁止可跳出值上下文的字符 / Forbid characters that can terminate a value context.
  if (/[;{}!\r\n]/.test(trimmed)) return undefined;
  // 禁止自定义属性、url()、expression()、javascript: 与 @import。
  if (/--|url\(|expression\(|javascript:|@import/i.test(trimmed)) return undefined;
  return trimmed;
}

/** Maps the portable theme token contract onto the host's semantic CSS layer. */
export function themeCssVariables(theme: ThemeManifest | null): ThemeVariables | undefined {
  if (!theme) return undefined;
  const panel = sanitizeCssColor(theme.colors.panel);
  const canvas = sanitizeCssColor(theme.colors.canvas);
  const text = sanitizeCssColor(theme.colors.text);
  const muted = sanitizeCssColor(theme.colors.muted);
  const accent = sanitizeCssColor(theme.colors.accent);
  const border = sanitizeCssColor(theme.colors.border);
  if (
    panel === undefined ||
    canvas === undefined ||
    text === undefined ||
    muted === undefined ||
    accent === undefined ||
    border === undefined
  ) {
    return undefined;
  }
  const toast = theme.components?.toast;
  const miniMap = theme.components?.miniMap;
  const radialMenu = theme.components?.radialMenu;
  const fallback = (value: string | undefined, defaultValue: string) =>
    value === undefined ? defaultValue : (sanitizeCssColor(value) ?? defaultValue);
  return {
    "--color-paper": panel,
    "--color-canvas": canvas,
    "--color-ink": text,
    "--color-blue": accent,
    "--color-blue-soft": `color-mix(in srgb, ${accent} 16%, ${panel})`,
    "--color-olive": muted,
    "--toast-background": fallback(toast?.background, panel),
    "--toast-border": fallback(toast?.border, border),
    "--toast-text": fallback(toast?.text, text),
    "--toast-shadow": fallback(toast?.shadow, "0 5px 18px rgb(28 31 35 / 10%)"),
    "--minimap-background": fallback(miniMap?.background, panel),
    "--minimap-border": fallback(miniMap?.border, border),
    "--minimap-mask": fallback(
      miniMap?.mask,
      `color-mix(in srgb, ${canvas} 72%, transparent)`,
    ),
    "--minimap-selected-node": fallback(miniMap?.selectedNode, accent),
    "--minimap-evidence-node": fallback(miniMap?.evidenceNode, muted),
    "--minimap-node": fallback(miniMap?.node, border),
    "--minimap-relation": fallback(miniMap?.relation, muted),
    "--radial-menu-background": fallback(radialMenu?.background, panel),
    "--radial-menu-border": fallback(radialMenu?.border, border),
    "--radial-menu-divider": fallback(radialMenu?.divider, border),
    "--radial-menu-text": fallback(radialMenu?.text, text),
    "--radial-menu-active": fallback(radialMenu?.active, accent),
    "--radial-menu-center-background": fallback(radialMenu?.centerBackground, panel),
    "--radial-menu-center-text": fallback(radialMenu?.centerText, accent),
    "--radial-menu-shadow": fallback(radialMenu?.shadow, "0 9px 34px rgb(28 31 35 / 8%)"),
    "--radial-menu-active-shadow": fallback(radialMenu?.activeShadow, "rgb(40 87 219 / 20%)"),
  };
}
