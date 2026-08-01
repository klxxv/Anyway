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

export interface FolderProjectSummary {
  path: string;
  title: string;
  discipline: string;
  revision: number;
  updatedAt: string;
  nodeCount: number;
  edgeCount: number;
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
  branch: string;
  dirty: boolean;
  commits: GitCommitRecord[];
  graphPatch: unknown;
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

export async function saveProjectNative(project: ProjectState) {
  const { invoke, dialog } = await desktopModules();
  const path = await dialog.save({
    title: "Save Research Canvas project",
    defaultPath: `${projectFileStem(project)}.mycproj`,
    filters: [{ name: "Research Canvas project", extensions: [...projectFileExtensions] }],
  });
  if (!path) return null;
  return invoke<NativeProjectFileResult>("save_project_file", { path, project });
}

export async function importProjectNative() {
  const { invoke, dialog } = await desktopModules();
  const path = await dialog.open({
    title: "Import Research Canvas project",
    multiple: false,
    directory: false,
    filters: [{ name: "Research Canvas project", extensions: [...projectFileExtensions] }],
  });
  if (!path || Array.isArray(path)) return null;
  return importProjectAtPath(path, invoke);
}

export async function importProjectAtPath(
  path: string,
  suppliedInvoke?: <T>(command: string, args?: Record<string, unknown>) => Promise<T>,
) {
  const invoke = suppliedInvoke ?? (await desktopModules()).invoke;
  const result = await invoke<NativeProjectFileResult>("import_project_file", { path });
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
    pluginId: command.pluginId,
    pluginVersion: command.pluginVersion,
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
  const { invoke } = await desktopModules();
  const projects = await invoke<FolderProjectSummary[]>("scan_project_folder", {
    pluginId: command.pluginId,
    pluginVersion: command.pluginVersion,
    path,
  });
  return { path, projects };
}

export async function openGitWorkspace(command: EnabledWorkspaceCommand) {
  const path = await chooseDirectory(command.label);
  if (!path) return null;
  const { invoke } = await desktopModules();
  return invoke<GitWorkspaceSnapshot>("read_git_workspace", {
    pluginId: command.pluginId,
    pluginVersion: command.pluginVersion,
    path,
  });
}

export async function gitAutosaveProject(
  command: EnabledWorkspaceCommand,
  repoPath: string,
  project: ProjectState,
  message = "Research Canvas autosave",
) {
  const { invoke } = await desktopModules();
  return invoke<GitWorkspaceSnapshot>("git_autosave_project", {
    pluginId: command.pluginId,
    pluginVersion: command.pluginVersion,
    repoPath,
    projectPath: `.research-canvas/${projectFileStem(project)}.mycproj`,
    project,
    message,
  });
}
