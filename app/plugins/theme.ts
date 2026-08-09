import type { CSSProperties } from "react";
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

type ThemeVariables = CSSProperties & Record<`--${string}`, string>;

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
  const base = {
    panel: sanitizeCssColor(theme.colors.panel),
    canvas: sanitizeCssColor(theme.colors.canvas),
    text: sanitizeCssColor(theme.colors.text),
    muted: sanitizeCssColor(theme.colors.muted),
    accent: sanitizeCssColor(theme.colors.accent),
    border: sanitizeCssColor(theme.colors.border),
  };
  if (Object.values(base).some((value) => value === undefined)) {
    return undefined;
  }
  const toast = theme.components?.toast;
  const miniMap = theme.components?.miniMap;
  const radialMenu = theme.components?.radialMenu;
  const fallback = (value: string | undefined, defaultValue: string) =>
    value === undefined ? defaultValue : (sanitizeCssColor(value) ?? defaultValue);
  return {
    "--color-paper": base.panel,
    "--color-canvas": base.canvas,
    "--color-ink": base.text,
    "--color-blue": base.accent,
    "--color-blue-soft": `color-mix(in srgb, ${base.accent} 16%, ${base.panel})`,
    "--color-olive": base.muted,
    "--toast-background": fallback(toast?.background, base.panel),
    "--toast-border": fallback(toast?.border, base.border),
    "--toast-text": fallback(toast?.text, base.text),
    "--toast-shadow": fallback(toast?.shadow, "0 5px 18px rgb(28 31 35 / 10%)"),
    "--minimap-background": fallback(miniMap?.background, base.panel),
    "--minimap-border": fallback(miniMap?.border, base.border),
    "--minimap-mask": fallback(
      miniMap?.mask,
      `color-mix(in srgb, ${base.canvas} 72%, transparent)`,
    ),
    "--minimap-selected-node": fallback(miniMap?.selectedNode, base.accent),
    "--minimap-evidence-node": fallback(miniMap?.evidenceNode, base.muted),
    "--minimap-node": fallback(miniMap?.node, base.border),
    "--minimap-relation": fallback(miniMap?.relation, base.muted),
    "--radial-menu-background": fallback(radialMenu?.background, base.panel),
    "--radial-menu-border": fallback(radialMenu?.border, base.border),
    "--radial-menu-divider": fallback(radialMenu?.divider, base.border),
    "--radial-menu-text": fallback(radialMenu?.text, base.text),
    "--radial-menu-active": fallback(radialMenu?.active, base.accent),
    "--radial-menu-center-background": fallback(radialMenu?.centerBackground, base.panel),
    "--radial-menu-center-text": fallback(radialMenu?.centerText, base.accent),
    "--radial-menu-shadow": fallback(radialMenu?.shadow, "0 9px 34px rgb(28 31 35 / 8%)"),
    "--radial-menu-active-shadow": fallback(radialMenu?.activeShadow, "rgb(40 87 219 / 20%)"),
  };
}
