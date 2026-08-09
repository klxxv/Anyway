"use client";

import {
  IconBrandGithub,
  IconCheck,
  IconClock,
  IconCopy,
  IconFileImport,
  IconGitBranch,
  IconGitCommit,
  IconKey,
  IconRefresh,
  IconUpload,
  IconX,
} from "@tabler/icons-react";
import { useCallback, useMemo, useState } from "react";
import { useI18n } from "../../../i18n/provider";
import type {
  FolderProjectSummary,
  GitHubAccountStatus,
  GitWorkspaceSnapshot,
} from "../../../platform/native-project";
import type { GraphPatchOperation, PluginGraphPatch } from "../../../plugins/contracts";

export function FolderWorkspaceDialog({
  root,
  projects,
  onClose,
  onOpen,
}: {
  root: string;
  projects: FolderProjectSummary[];
  onClose: () => void;
  onOpen: (path: string) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <section className="flex h-[min(620px,calc(100vh-32px))] w-[min(760px,calc(100vw-32px))] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]">
        <header className="flex items-start justify-between border-b border-ink/15 px-7 py-5">
          <div>
            <span className="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">
              {t("workspace.folderMode")}
            </span>
            <h2 className="mt-1 font-serif text-[21px]">{t("workspace.folderProjects")}</h2>
            <p className="mt-1 max-w-[620px] truncate font-mono text-[9px] text-ink/45" title={root}>
              {root}
            </p>
          </div>
          <button className="icon-quiet" onClick={onClose} aria-label={t("menu.close")}>
            <IconX size={18} stroke={1.35} />
          </button>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto p-6">
          {projects.length === 0 ? (
            <p className="rounded-[5px] border border-dashed border-ink/20 px-5 py-12 text-center font-serif text-[12px] text-ink/45">
              {t("workspace.noFolderProjects")}
            </p>
          ) : (
            <div className="space-y-2">
              {projects.map((project) => (
                <article key={project.path} className="flex items-center gap-4 rounded-[5px] border border-ink/15 p-4">
                  <IconFileImport size={20} stroke={1.3} className="text-blue" />
                  <div className="min-w-0 flex-1">
                    <h3 className="font-serif text-[14px]">{project.title}</h3>
                    <p className="mt-1 font-serif text-[9px] text-ink/50">
                      {project.discipline} · {project.nodeCount} {t("workspace.nodes")} · {project.edgeCount} {t("workspace.relations")}
                    </p>
                    <p className="mt-1 truncate font-mono text-[8px] text-ink/35" title={project.path}>
                      {project.path}
                    </p>
                  </div>
                  <button className="button-secondary" onClick={() => onOpen(project.path)}>
                    {t("workspace.openProject")}
                  </button>
                </article>
              ))}
            </div>
          )}
        </div>
      </section>
    </div>
  );
}

export function GitWorkspaceDialog({
  snapshot,
  account,
  autoSave,
  busy,
  patch,
  onClose,
  onToggleAutoSave,
  onInitialize,
  onRefreshAccount,
  onLogin,
  onGenerateSshKey,
  onUploadSshKey,
  onSaveNow,
  onApplyPatch,
}: {
  snapshot: GitWorkspaceSnapshot;
  account: GitHubAccountStatus | null;
  autoSave: boolean;
  busy: boolean;
  patch: PluginGraphPatch | null;
  onClose: () => void;
  onToggleAutoSave: (enabled: boolean) => void;
  onInitialize: () => void;
  onRefreshAccount: () => void;
  onLogin: () => void;
  onGenerateSshKey: () => void;
  onUploadSshKey: (path: string) => void;
  onSaveNow: () => void;
  onApplyPatch: (acceptedPatch: PluginGraphPatch | null) => void;
}) {
  const { t } = useI18n();
  const [accepted, setAccepted] = useState<Record<number, boolean>>({});

  const buildAcceptedPatch = useCallback((patch: PluginGraphPatch | null) => {
    if (!patch) return null;
    const operations = patch.operations
      .map((operation, index) => (accepted[index] === false ? null : operation))
      .filter((operation): operation is GraphPatchOperation => operation !== null);
    if (operations.length === 0) return null;
    return { ...patch, operations };
  }, [accepted]);

  const acceptedCount = useMemo(() => {
    if (!patch) return 0;
    return patch.operations.filter((_, index) => accepted[index] !== false).length;
  }, [accepted, patch]);

  const rejectedCount = useMemo(() => {
    if (!patch) return 0;
    return patch.operations.filter((_, index) => accepted[index] === false).length;
  }, [accepted, patch]);

  return (
    <div className="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]">
      <section className="grid h-[min(680px,calc(100vh-32px))] w-[min(920px,calc(100vw-32px))] grid-cols-1 overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)] md:grid-cols-[minmax(0,1fr)_300px]">
        <div className="flex min-h-0 flex-col border-b border-ink/15 md:border-b-0 md:border-r">
          <header className="flex items-start justify-between border-b border-ink/15 px-7 py-5">
            <div>
              <span className="font-sans text-[8px] uppercase tracking-[0.18em] text-blue">
                {t("workspace.gitWorkspace")}
              </span>
              <h2 className="mt-1 flex items-center gap-2 font-serif text-[21px]">
                <IconGitBranch size={20} stroke={1.35} />
                {snapshot.branch || (snapshot.isRepository ? "HEAD" : t("workspace.gitNotRepository"))}
              </h2>
              <p className="mt-1 max-w-[520px] truncate font-mono text-[8px] text-ink/45" title={snapshot.repoPath}>
                {snapshot.repoPath}
              </p>
            </div>
            <button className="icon-quiet" onClick={onClose} aria-label={t("menu.close")}>
              <IconX size={18} stroke={1.35} />
            </button>
          </header>
          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
            <div className="mb-4 flex items-center justify-between">
              <p className="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">
                {t("workspace.gitTree")} · {snapshot.commits.length}
              </p>
              {snapshot.isRepository && (
                <span className={`font-serif text-[9px] ${snapshot.dirty ? "text-red-500" : "text-olive"}`}>
                  {snapshot.dirty ? t("workspace.gitDirty") : t("workspace.gitClean")}
                </span>
              )}
            </div>
            {!snapshot.isRepository && (
              <div className="grid min-h-[260px] place-items-center rounded-[6px] border border-dashed border-ink/20 bg-canvas p-8 text-center">
                <div>
                  <IconGitBranch className="mx-auto text-ink/35" size={30} stroke={1.2} />
                  <h3 className="mt-4 font-serif text-[15px]">{t("workspace.gitNotRepository")}</h3>
                  <p className="mx-auto mt-2 max-w-[360px] font-serif text-[10px] leading-[1.55] text-ink/50">
                    {t("workspace.gitNotRepositoryHint")}
                  </p>
                </div>
              </div>
            )}
            {snapshot.isRepository && snapshot.commits.length === 0 && (
              <p className="rounded-[6px] border border-dashed border-ink/20 bg-canvas px-4 py-10 text-center font-serif text-[10px] text-ink/45">
                {t("workspace.gitNoCommits")}
              </p>
            )}
            <div className="space-y-2">
              {snapshot.commits.map((commit) => (
                <article key={commit.id} className="relative ml-2 flex gap-3 border-l border-ink/20 py-2 pl-5 pr-3 before:absolute before:-left-[4px] before:top-4 before:size-[7px] before:rounded-full before:border before:border-blue before:bg-paper">
                  <IconGitCommit size={18} stroke={1.3} className="mt-0.5 shrink-0 text-blue" />
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-serif text-[12px]">
                      {commit.message.split("\n")[0] || commit.shortId}
                    </p>
                    <p className="mt-1 font-mono text-[8px] text-ink/45">
                      {commit.shortId} · {commit.author} · {commit.timestamp.slice(0, 10)}
                    </p>
                    {commit.parents.length > 0 && (
                      <p className="mt-1 truncate font-mono text-[7px] text-ink/35">
                        ← {commit.parents.map((parent) => parent.slice(0, 8)).join(" · ")}
                      </p>
                    )}
                    {commit.refs.length > 0 && (
                      <div className="mt-2 flex flex-wrap gap-1">
                        {commit.refs.map((reference) => (
                          <span key={reference} className="rounded-[3px] bg-blue-soft px-1.5 py-0.5 font-sans text-[7px] text-blue">
                            {reference}
                          </span>
                        ))}
                      </div>
                    )}
                  </div>
                </article>
              ))}
            </div>
          </div>
        </div>
        <aside className="flex min-h-0 flex-col overflow-y-auto bg-canvas p-5">
          <div className="border-b border-ink/15 pb-5">
            <div className="flex items-center gap-2 text-blue">
              <IconBrandGithub size={18} stroke={1.35} />
              <h3 className="font-serif text-[14px]">{t("workspace.githubAccount")}</h3>
              <button
                className="ml-auto grid size-7 place-items-center rounded-[4px] text-ink/45 transition hover:bg-blue-soft hover:text-blue"
                disabled={busy}
                onClick={onRefreshAccount}
                aria-label={t("workspace.githubRefresh")}
              >
                <IconRefresh className={busy ? "animate-spin" : ""} size={14} stroke={1.35} />
              </button>
            </div>
            {!account?.cliAvailable ? (
              <p className="mt-2 font-serif text-[9px] leading-[1.5] text-ink/50">
                {t("workspace.githubCliRequired")}
              </p>
            ) : account.authenticated ? (
              <div className="mt-3 rounded-[5px] border border-ink/15 bg-paper p-3">
                <p className="font-serif text-[11px] text-ink/85">@{account.login}</p>
                <p className="mt-1 font-mono text-[7px] text-ink/40">
                  {account.host} · {account.gitProtocol ?? "git"}
                </p>
              </div>
            ) : (
              <>
                <p className="mt-2 font-serif text-[9px] leading-[1.5] text-ink/50">
                  {t("workspace.githubLoginHint")}
                </p>
                <button className="button-primary mt-3 w-full justify-center" disabled={busy} onClick={onLogin}>
                  <IconBrandGithub size={15} stroke={1.35} />
                  {t("workspace.githubLogin")}
                </button>
              </>
            )}

            {account?.sshKeygenAvailable && (
              <div className="mt-4">
                <div className="flex items-center gap-2">
                  <IconKey size={15} stroke={1.35} className="text-blue" />
                  <p className="font-serif text-[11px]">{t("workspace.githubSshKeys")}</p>
                </div>
                <div className="mt-2 space-y-2">
                  {account.sshKeys.map((key) => (
                    <article key={key.path} className="rounded-[4px] border border-ink/12 bg-paper p-2.5">
                      <p className="truncate font-mono text-[7px] text-ink/55" title={key.path}>
                        {key.managedByApp ? "Research Canvas · " : ""}{key.algorithm}
                      </p>
                      <p className="mt-1 truncate font-mono text-[7px] text-ink/35" title={key.fingerprint}>
                        {key.fingerprint || key.path}
                      </p>
                      <div className="mt-2 flex gap-1.5">
                        <button
                          className="button-secondary min-h-7 flex-1 justify-center px-2 text-[8px]"
                          onClick={() => void navigator.clipboard.writeText(key.publicKey)}
                        >
                          <IconCopy size={12} stroke={1.35} />
                          {t("workspace.githubCopyKey")}
                        </button>
                        {account.authenticated && (
                          <button
                            className="button-secondary min-h-7 flex-1 justify-center px-2 text-[8px]"
                            disabled={busy}
                            onClick={() => onUploadSshKey(key.path)}
                          >
                            <IconUpload size={12} stroke={1.35} />
                            {t("workspace.githubUploadKey")}
                          </button>
                        )}
                      </div>
                    </article>
                  ))}
                  {!account.sshKeys.some((key) => key.managedByApp) && (
                    <button
                      className="button-secondary w-full justify-center"
                      disabled={busy}
                      onClick={onGenerateSshKey}
                    >
                      <IconKey size={14} stroke={1.35} />
                      {t("workspace.githubGenerateKey")}
                    </button>
                  )}
                </div>
              </div>
            )}
          </div>

          <div className="pt-5">
          {snapshot.isRepository ? (
            <>
          <div className="flex items-center gap-2 text-blue">
            <IconClock size={18} stroke={1.35} />
            <h3 className="font-serif text-[14px]">{t("workspace.gitAutosave")}</h3>
          </div>
          <p className="mt-2 font-serif text-[10px] leading-[1.5] text-ink/50">
            {t("workspace.gitAutosaveHint")}
          </p>
          <label className="mt-4 flex items-center justify-between rounded-[5px] border border-ink/15 bg-paper p-3">
            <span className="font-serif text-[11px]">5 min</span>
            <input type="checkbox" checked={autoSave} onChange={(event) => onToggleAutoSave(event.target.checked)} />
          </label>
          <button className="button-secondary mt-3 justify-center" disabled={busy} onClick={onSaveNow}>
            <IconRefresh className={busy ? "animate-spin" : ""} size={15} stroke={1.35} />
            {t("workspace.gitSaveNow")}
          </button>

          <div className="mt-6 border-t border-ink/15 pt-5">
            <div className="flex items-center gap-2">
              <IconCheck size={17} stroke={1.35} className="text-blue" />
              <h3 className="font-serif text-[14px]">GraphPatch</h3>
            </div>
            <p className="mt-2 font-serif text-[10px] leading-[1.5] text-ink/50">
              {patch
                ? `${acceptedCount}/${patch.operations.length} ${t("workspace.patchOperations")} · ${patch.summary}`
                : t("workspace.noPatch")}
            </p>
            {patch && (
              <div className="mt-3 space-y-1">
                {patch.operations.map((operation, index) => {
                  const rejected = accepted[index] === false;
                  const subject = operation.op === "add-node"
                    ? operation.node.title
                    : operation.op === "add-edge"
                      ? `${operation.edge.source} → ${operation.edge.target}`
                      : operation.op === "update-node"
                        ? operation.nodeId
                        : operation.edgeId;
                  return (
                    <button
                      key={`${operation.op}-${index}`}
                      type="button"
                      onClick={() => setAccepted((current) => ({ ...current, [index]: !rejected }))}
                      className={`flex w-full items-center gap-2 truncate rounded-[3px] border px-2 py-1.5 text-left font-mono text-[7px] transition ${
                        rejected
                          ? "border-ink/10 bg-canvas text-ink/30 line-through"
                          : "border-ink/10 bg-paper text-ink/55"
                      }`}
                      title={subject}
                    >
                      <span
                        className={`grid size-3.5 shrink-0 place-items-center rounded-[3px] border ${
                          rejected ? "border-ink/15" : "border-blue bg-blue-soft text-blue"
                        }`}
                      >
                        {!rejected && <IconCheck size={9} stroke={2} />}
                      </span>
                      <span className="min-w-0 flex-1 truncate">{operation.op} · {subject}</span>
                    </button>
                  );
                })}
              </div>
            )}
            <button
              className="button-primary mt-4 w-full justify-center"
              disabled={!patch || acceptedCount === 0}
              onClick={() => onApplyPatch(buildAcceptedPatch(patch))}
            >
              {rejectedCount > 0
                ? `${t("workspace.reviewApplyPatch")} (${acceptedCount})`
                : t("workspace.reviewApplyPatch")}
            </button>
          </div>
            </>
          ) : (
            <>
              <div className="flex items-center gap-2 text-blue">
                <IconGitBranch size={18} stroke={1.35} />
                <h3 className="font-serif text-[14px]">{t("workspace.gitInitialize")}</h3>
              </div>
              <p className="mt-2 font-serif text-[10px] leading-[1.5] text-ink/50">
                {t("workspace.gitInitializeHint")}
              </p>
              <button className="button-primary mt-4 w-full justify-center" disabled={busy} onClick={onInitialize}>
                <IconRefresh className={busy ? "animate-spin" : ""} size={15} stroke={1.35} />
                {t("workspace.gitInitialize")}
              </button>
            </>
          )}
          </div>
        </aside>
      </section>
    </div>
  );
}
