import type { ThemeManifest } from "../lib/research-types";

export const VSIX_IMPORT_API_VERSION = "researchcanvas.dev/vsix-import/v1alpha1";

export type VsixSafeAssetKind = "json" | "svg" | "png" | "font";

export interface VsixArchiveEntry {
  path: string;
  size: number;
  kind: VsixSafeAssetKind | "package" | "ignored";
}

export interface VsixThemeContribution {
  label: string;
  path: string;
  uiTheme?: string;
}

export interface VsixIconThemeContribution {
  id: string;
  label: string;
  path: string;
}

export interface VsixPackageContributes {
  themes?: VsixThemeContribution[];
  iconThemes?: VsixIconThemeContribution[];
  commands?: unknown[];
}

export interface VsixPackageJson {
  name: string;
  displayName?: string;
  publisher?: string;
  version: string;
  description?: string;
  main?: string;
  browser?: string;
  activationEvents?: unknown[];
  contributes?: VsixPackageContributes;
}

export interface VsixIconDefinition {
  iconPath?: string;
  fontCharacter?: string;
  fontId?: string;
}

export interface VsixIconFont {
  id: string;
  src: string[];
  weight?: string;
  style?: string;
}

/** Declarative icon data; it contains paths and metadata, never executable callbacks. */
export interface IconThemeManifest {
  schemaVersion: 1;
  id: string;
  name: string;
  publisher: string;
  version: string;
  description?: string;
  source: "vsix";
  fileExtensions: Record<string, string>;
  fileNames: Record<string, string>;
  folderNames: Record<string, string>;
  folderNamesExpanded: Record<string, string>;
  iconDefinitions: Record<string, VsixIconDefinition>;
  fonts: VsixIconFont[];
}

export interface VsixConvertedThemeResource {
  kind: "ThemePlugin";
  pluginId: string;
  version: string;
  manifestPath: "plugin.yml";
  entryPath: "theme.json";
  theme: ThemeManifest;
  copiedAssets: string[];
}

export interface VsixConvertedIconThemeResource {
  kind: "IconTheme";
  pluginId: string;
  version: string;
  manifestPath: "plugin.yml";
  entryPath: "icon-theme.json";
  iconTheme: IconThemeManifest;
  copiedAssets: string[];
}

export interface VsixImportReport {
  apiVersion: typeof VSIX_IMPORT_API_VERSION;
  source: string;
  package: Pick<VsixPackageJson, "name" | "publisher" | "version">;
  themes: VsixConvertedThemeResource[];
  iconThemes: VsixConvertedIconThemeResource[];
  ignoredExecutableAssets: string[];
  rejectedAssets: string[];
}

export interface NativeVsixImportReport {
  source: string;
  packageName: string;
  publisher: string;
  version: string;
  imported: Array<{
    id: string;
    name: string;
    version: string;
    kind: "ThemePlugin" | "IconThemePlugin" | string;
    assetCount: number;
  }>;
  ignoredCodeAssets: string[];
}

const SAFE_ASSET_EXTENSIONS = new Set([
  ".json",
  ".svg",
  ".png",
  ".woff",
  ".woff2",
  ".ttf",
  ".otf",
]);

export function isSafeVsixAssetPath(path: string): boolean {
  if (!path || path.startsWith("/") || path.includes("\\")) return false;
  const parts = path.split("/");
  if (parts.some((part) => !part || part === "." || part === "..")) return false;
  return SAFE_ASSET_EXTENSIONS.has(
    `.${parts.at(-1)?.split(".").at(-1)?.toLowerCase() ?? ""}`,
  );
}

export function assertDeclarativeVsixPackage(packageJson: VsixPackageJson) {
  if (packageJson.main || packageJson.browser) {
    throw new Error("VSIX declares main/browser code and is not a declarative theme package");
  }
  if (packageJson.activationEvents?.length) {
    throw new Error("VSIX declares activation events and cannot be imported as data");
  }
  if ((packageJson.contributes?.commands?.length ?? 0) > 0) {
    throw new Error("VSIX commands are not imported or executed");
  }
  const themes = packageJson.contributes?.themes ?? [];
  const iconThemes = packageJson.contributes?.iconThemes ?? [];
  if (!themes.length && !iconThemes.length) {
    throw new Error("VSIX does not contribute a theme or icon theme");
  }
  return { themes, iconThemes };
}
