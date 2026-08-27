import type { InstalledMycPlugin } from "./contracts";
import type { HostSlotDescriptor, PluginUiContribution } from "./plugin-frontend-contract";
import { getPluginFrontendManifest } from "./plugin-frontend-loader";

/** Pure target selection used by the Vue loader and non-Vue tests. */
export type SurfaceContributionRef<T> = {
  readonly pluginId: string;
  readonly slotId: string;
  readonly contribution: T;
};

export function selectPluginSurfaceContributions<T>(
  contributions: readonly SurfaceContributionRef<T>[],
  pluginId: string,
  slotId: string,
): readonly T[] {
  return contributions
    .filter((candidate) => candidate.pluginId === pluginId && candidate.slotId === slotId)
    .map((candidate) => candidate.contribution);
}

export type PluginUiContributionRef = {
  readonly plugin: InstalledMycPlugin;
  readonly pluginId: string;
  readonly pluginVersion: string;
  readonly slotId: string;
  readonly contribution: PluginUiContribution;
};

export type PluginUiUserOrder = Readonly<Record<string, number>>;

function contributionOrderKey(pluginId: string, contributionId: string): string {
  return `${pluginId}:${contributionId}`;
}

function contributionWhenAllows(when: PluginUiContribution["when"]): boolean {
  if (when === undefined) return true;
  if (typeof when === "boolean") return when;
  const normalized = when.trim().toLowerCase();
  return normalized.length === 0
    || normalized === "true"
    || normalized === "enabled"
    || normalized === "workspace.active";
}

function hasTrustedModuleFrontend(plugin: InstalledMycPlugin): boolean {
  const frontend = getPluginFrontendManifest(plugin);
  return frontend?.mode === "trusted-module" && frontend.framework === "vue3" && frontend.apiVersion === "1";
}

export function pluginUiContributions(plugin: InstalledMycPlugin): readonly PluginUiContribution[] {
  if (!hasTrustedModuleFrontend(plugin)) return [];
  return plugin.manifest.spec.contributes?.ui ?? [];
}

export function selectPluginUiContributions(
  plugins: readonly InstalledMycPlugin[],
  slot: HostSlotDescriptor,
  userOrder: PluginUiUserOrder = {},
): readonly PluginUiContributionRef[] {
  if (!slot.accepts.includes("trusted-module")) return [];

  const selected = plugins.flatMap((plugin) => {
    const pluginId = plugin.manifest.metadata.id;
    const pluginVersion = plugin.manifest.metadata.version;
    return pluginUiContributions(plugin)
      .filter((contribution) => contribution.slotId === slot.id && contributionWhenAllows(contribution.when))
      .map((contribution) => ({ plugin, pluginId, pluginVersion, slotId: contribution.slotId, contribution }));
  });

  const ordered = selected.sort((a, b) => {
    const aUserOrder = userOrder[contributionOrderKey(a.pluginId, a.contribution.id)];
    const bUserOrder = userOrder[contributionOrderKey(b.pluginId, b.contribution.id)];
    const aOrder = aUserOrder ?? a.contribution.order ?? 0;
    const bOrder = bUserOrder ?? b.contribution.order ?? 0;
    return (
      aOrder - bOrder ||
      a.pluginId.localeCompare(b.pluginId) ||
      a.contribution.id.localeCompare(b.contribution.id)
    );
  });

  return slot.cardinality === "single" ? ordered.slice(0, 1) : ordered;
}
