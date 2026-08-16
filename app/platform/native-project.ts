import type { ProjectState } from "../lib/research-types";
import {
  isProjectState,
  projectFileExtensions,
  projectFileStem,
  type NativeProjectFileResult,
} from "../lib/project-io";
import type { EnabledWorkspaceCommand } from "../plugins/workspace";
import {
  projectExportFileName,
  renderProjectExport,
} from "../plugins/workspace";
import { HostSdk } from "./host-sdk";
import { createDefaultTauriHostSdkTransport } from "./host-sdk-tauri";

export interface FolderProjectSummary {
  path: string;
  title: string;
  discipline: string;
  revision: number;
  updatedAt: string;
  nodeCount: number;
  edgeCount: number;
}

export interface FolderTreeEntry {
  path: string;
  name: string;
  kind: "directory" | "file";
  size: number;
  modifiedAt: number | null;
}

export interface GitCommitRecord {
  id: string;
  shortId: string;
  parents: string[];
  author: string;
  timestamp: string;
  message: string;
  refs: string[];
}

export interface GitWorkspaceSnapshot {
  repoPath: string;
  isRepository: boolean;
  branch: string;
  dirty: boolean;
  commits: GitCommitRecord[];
  graphPatch: unknown;
}

export interface GitSshPublicKey {
  path: string;
  algorithm: string;
  fingerprint: string;
  publicKey: string;
  managedByApp: boolean;
}

export interface GitHubAccountStatus {
  cliAvailable: boolean;
  authenticated: boolean;
  host: string;
  login: string | null;
  gitProtocol: string | null;
  sshKeygenAvailable: boolean;
  sshKeys: GitSshPublicKey[];
}

function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function desktopModules() {
  if (!hasTauriRuntime()) throw new Error("DESKTOP_REQUIRED");
  const [{ invoke }, dialog] = await Promise.all([
    import("@tauri-apps/api/core"),
    import("@tauri-apps/plugin-dialog"),
  ]);
  return { invoke, dialog };
}

let desktopHostSdk: HostSdk | undefined;

function getDesktopHostSdk(): HostSdk {
  desktopHostSdk ??= new HostSdk(createDefaultTauriHostSdkTransport());
  return desktopHostSdk;
}

export async function saveProjectNative(project: ProjectState) {
  const { dialog } = await desktopModules();
  const path = await dialog.save({
    title: "Save Research Canvas project",
    defaultPath: `${projectFileStem(project)}.mycproj`,
    filters: [{ name: "Research Canvas project", extensions: [...projectFileExtensions] }],
  });
  if (!path) return null;
  return getDesktopHostSdk().call<NativeProjectFileResult>("project.save", { path, project });
}

export async function importProjectNative() {
  const { dialog } = await desktopModules();
  const path = await dialog.open({
    title: "Import Research Canvas project",
    multiple: false,
    directory: false,
    filters: [{ name: "Research Canvas project", extensions: [...projectFileExtensions] }],
  });
  if (!path || Array.isArray(path)) return null;
  return importProjectAtPath(path);
}

export async function importProjectAtPath(path: string) {
  await desktopModules();
  const result = await getDesktopHostSdk().call<NativeProjectFileResult>("project.import", { path });
  if (!isProjectState(result.project)) throw new Error("PROJECT_FILE_INVALID");
  return { path: result.path, project: result.project };
}

export async function exportProjectWithPlugin(
  project: ProjectState,
  command: EnabledWorkspaceCommand,
  format: "pdf" | "svg" | "png",
) {
  if (!command.formats?.includes(format)) throw new Error("PLUGIN_FORMAT_NOT_DECLARED");
  const { invoke, dialog } = await desktopModules();
  const path = await dialog.save({
    title: command.label,
    defaultPath: projectExportFileName(project, format),
    filters: [{ name: `${format.toUpperCase()} export`, extensions: [format] }],
  });
  if (!path) return null;
  const data = await renderProjectExport(project, format);
  return invoke<string>("save_plugin_artifact", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
    capability: command.capability,
    format,
    path,
    data: Array.from(data),
  });
}

async function chooseDirectory(title: string) {
  const { dialog } = await desktopModules();
  const path = await dialog.open({ title, multiple: false, directory: true });
  return !path || Array.isArray(path) ? null : path;
}

export async function openFolderWorkspace(command: EnabledWorkspaceCommand) {
  const path = await chooseDirectory(command.label);
  if (!path) return null;
  await desktopModules();
  const projects = await getDesktopHostSdk().call<FolderProjectSummary[]>(
    "workspace.folder.scan",
    {
      pluginId: command.plugin.id,
      pluginVersion: command.plugin.version,
      path,
    },
  );
  return { path, projects };
}

export async function listFolderEntries(
  command: EnabledWorkspaceCommand,
  root: string,
  path = root,
) {
  await desktopModules();
  return getDesktopHostSdk().call<FolderTreeEntry[]>("workspace.folder.list", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
    root,
    path,
  });
}

export async function openGitWorkspace(command: EnabledWorkspaceCommand) {
  const path = await chooseDirectory(command.label);
  if (!path) return null;
  await desktopModules();
  return getDesktopHostSdk().call<GitWorkspaceSnapshot>("workspace.git.read", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
    path,
  });
}

export async function initializeGitWorkspace(
  command: EnabledWorkspaceCommand,
  path: string,
) {
  await desktopModules();
  return getDesktopHostSdk().call<GitWorkspaceSnapshot>("workspace.git.init", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
    path,
  });
}

export async function readGitHubAccount(command: EnabledWorkspaceCommand) {
  await desktopModules();
  return getDesktopHostSdk().call<GitHubAccountStatus>("workspace.github.read", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
  });
}

export async function loginGitHubAccount(command: EnabledWorkspaceCommand) {
  await desktopModules();
  return getDesktopHostSdk().call<GitHubAccountStatus>("workspace.github.login", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
  });
}

export async function generateGitHubSshKey(
  command: EnabledWorkspaceCommand,
  comment: string,
) {
  await desktopModules();
  return getDesktopHostSdk().call<GitHubAccountStatus>(
    "workspace.github.ssh.generate",
    {
      pluginId: command.plugin.id,
      pluginVersion: command.plugin.version,
      comment,
    },
  );
}

export async function uploadGitHubSshKey(
  command: EnabledWorkspaceCommand,
  path: string,
) {
  await desktopModules();
  return getDesktopHostSdk().call<GitHubAccountStatus>("workspace.github.ssh.upload", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
    path,
  });
}

export async function gitAutosaveProject(
  command: EnabledWorkspaceCommand,
  repoPath: string,
  project: ProjectState,
  message = "Research Canvas autosave",
) {
  await desktopModules();
  return getDesktopHostSdk().call<GitWorkspaceSnapshot>("workspace.git.autosave", {
    pluginId: command.plugin.id,
    pluginVersion: command.plugin.version,
    repoPath,
    projectPath: `.research-canvas/${projectFileStem(project)}.mycproj`,
    project,
    message,
  });
}
