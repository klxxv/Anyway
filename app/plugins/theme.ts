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

/** Maps the portable theme token contract onto the host's semantic CSS layer. */
export function themeCssVariables(theme: ThemeManifest | null): ThemeVariables | undefined {
  if (!theme) return undefined;
  const toast = theme.components?.toast;
  const miniMap = theme.components?.miniMap;
  const radialMenu = theme.components?.radialMenu;
  return {
    "--color-paper": theme.colors.panel,
    "--color-canvas": theme.colors.canvas,
    "--color-ink": theme.colors.text,
    "--color-blue": theme.colors.accent,
    "--color-blue-soft": `color-mix(in srgb, ${theme.colors.accent} 16%, ${theme.colors.panel})`,
    "--color-olive": theme.colors.muted,
    "--toast-background": toast?.background ?? theme.colors.panel,
    "--toast-border": toast?.border ?? theme.colors.border,
    "--toast-text": toast?.text ?? theme.colors.text,
    "--toast-shadow": toast?.shadow ?? "0 5px 18px rgb(28 31 35 / 10%)",
    "--minimap-background": miniMap?.background ?? theme.colors.panel,
    "--minimap-border": miniMap?.border ?? theme.colors.border,
    "--minimap-mask": miniMap?.mask ?? `color-mix(in srgb, ${theme.colors.canvas} 72%, transparent)`,
    "--minimap-selected-node": miniMap?.selectedNode ?? theme.colors.accent,
    "--minimap-evidence-node": miniMap?.evidenceNode ?? theme.colors.muted,
    "--minimap-node": miniMap?.node ?? theme.colors.border,
    "--minimap-relation": miniMap?.relation ?? theme.colors.muted,
    "--radial-menu-background": radialMenu?.background ?? theme.colors.panel,
    "--radial-menu-border": radialMenu?.border ?? theme.colors.border,
    "--radial-menu-divider": radialMenu?.divider ?? theme.colors.border,
    "--radial-menu-text": radialMenu?.text ?? theme.colors.text,
    "--radial-menu-active": radialMenu?.active ?? theme.colors.accent,
    "--radial-menu-center-background": radialMenu?.centerBackground ?? theme.colors.panel,
    "--radial-menu-center-text": radialMenu?.centerText ?? theme.colors.accent,
    "--radial-menu-shadow": radialMenu?.shadow ?? "0 9px 34px rgb(28 31 35 / 8%)",
    "--radial-menu-active-shadow": radialMenu?.activeShadow ?? "rgb(40 87 219 / 20%)",
  };
}
