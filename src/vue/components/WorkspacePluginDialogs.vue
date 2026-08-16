<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { pluginReference } from "../../../app/plugins/contracts";
import type {
  GraphPatchOperation,
  PluginGraphPatch,
} from "../../../app/plugins/contracts";
import {
  operationSubject,
  usePanelI18n,
  type FolderWorkspaceDialogProps,
  type GitWorkspaceDialogProps,
} from "./panel-types";
import FolderExplorerTree from "./FolderExplorerTree.vue";
import {
  listFolderEntries,
  type FolderTreeEntry,
} from "../../../app/platform/native-project";
import { readIconThemeAsset } from "../../../app/plugins/tauri-client";
import { usePluginHost } from "../runtime/plugin-host";
import type { IconThemeManifest } from "../../../app/plugins/vsix-contracts";

const { t } = usePanelI18n();
const pluginHost = usePluginHost();
const folderProps = defineProps<
  FolderWorkspaceDialogProps & Partial<GitWorkspaceDialogProps>
>();
const emit = defineEmits<{
  (event: "close"): void;
  (event: "open", path: string): void;
  (event: "toggle-auto-save", enabled: boolean): void;
  (event: "initialize"): void;
  (event: "refresh-account"): void;
  (event: "login"): void;
  (event: "generate-ssh-key"): void;
  (event: "upload-ssh-key", path: string): void;
  (event: "save-now"): void;
  (event: "apply-patch", patch: PluginGraphPatch | null): void;
}>();

const accepted = ref<Record<number, boolean>>({});
/** Git review preserves its legacy default: every operation starts accepted and
 * clicking an item toggles an explicit rejection. */
const buildAcceptedPatch = (patch: PluginGraphPatch | null) => {
  if (!patch) return null;
  const operations = patch.operations
    .map((operation, index) =>
      accepted.value[index] === false ? null : operation,
    )
    .filter(
      (operation): operation is GraphPatchOperation => operation !== null,
    );
  return operations.length ? { ...patch, operations } : null;
};
const acceptedCount = computed(
  () =>
    folderProps.patch?.operations.filter(
      (_, index) => accepted.value[index] !== false,
    ).length ?? 0,
);
const rejectedCount = computed(
  () =>
    folderProps.patch?.operations.filter(
      (_, index) => accepted.value[index] === false,
    ).length ?? 0,
);
const isGit = computed(() => Boolean(folderProps.snapshot));
const activeIconThemePlugin = computed(() =>
  pluginHost.activePlugins.find((plugin) => plugin.manifest.kind === "IconThemePlugin"),
);
const activeIconTheme = computed<IconThemeManifest | undefined>(() =>
  activeIconThemePlugin.value?.iconTheme,
);
const resolveIconThemeAsset = (assetPath: string): Promise<string | null> => {
  const plugin = activeIconThemePlugin.value;
  return plugin
    ? readIconThemeAsset(pluginReference(plugin), assetPath)
    : Promise.resolve(null);
};
const childrenByPath = ref<Record<string, FolderTreeEntry[]>>({});
const expandedPaths = ref(new Set<string>());
const loadingPaths = ref(new Set<string>());
const folderError = ref("");

async function loadFolderEntries(path: string) {
  if (!folderProps.command) return;
  loadingPaths.value = new Set([...loadingPaths.value, path]);
  folderError.value = "";
  try {
    const entries = await listFolderEntries(folderProps.command, folderProps.root, path);
    childrenByPath.value = { ...childrenByPath.value, [path]: entries };
  } catch (error) {
    folderError.value = error instanceof Error ? error.message : String(error);
  } finally {
    const next = new Set(loadingPaths.value);
    next.delete(path);
    loadingPaths.value = next;
  }
}

async function toggleFolder(path: string) {
  const next = new Set(expandedPaths.value);
  if (next.has(path)) {
    next.delete(path);
    expandedPaths.value = next;
    return;
  }
  next.add(path);
  expandedPaths.value = next;
  if (!childrenByPath.value[path]) await loadFolderEntries(path);
}

onMounted(() => {
  if (!isGit.value) void loadFolderEntries(folderProps.root);
});
const patchSubject = operationSubject;
const close = () => emit("close");
const copyPublicKey = (publicKey: string) => {
  if (typeof navigator !== "undefined") void navigator.clipboard?.writeText(publicKey);
};
</script>

<template>
  <div
    class="fixed inset-0 z-[97] grid place-items-center bg-ink/10 backdrop-blur-[2px]"
  >
    <section
      v-if="!isGit"
      class="flex h-[min(620px,calc(100vh-32px))] w-[min(760px,calc(100vw-32px))] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]"
    >
      <header
        class="flex items-start justify-between border-b border-ink/15 px-7 py-5"
      >
        <div>
          <span
            class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue"
            >{{ t("workspace.folderMode") }}</span
          >
          <h2 class="mt-1 font-serif text-[21px]">
            {{ t("workspace.folderProjects") }}
          </h2>
          <p
            class="mt-1 max-w-[620px] truncate font-mono text-[9px] text-ink/45"
            :title="folderProps.root"
          >
            {{ folderProps.root }}
          </p>
        </div>
        <button class="icon-quiet" @click="close" :aria-label="t('menu.close')">
          ×
        </button>
      </header>
      <div class="min-h-0 flex-1 overflow-y-auto p-6">
        <section class="mb-6 rounded-[6px] border border-ink/15 bg-canvas p-4">
          <div class="flex items-center justify-between border-b border-ink/10 pb-3">
            <div>
              <span class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45">{{ t("workspace.folderExplorer") }}</span>
              <h3 class="mt-1 font-serif text-[14px]">{{ folderProps.root.split(/[\\/]/).filter(Boolean).at(-1) || folderProps.root }}</h3>
            </div>
            <button class="button-secondary min-h-7 px-2 text-[8px]" @click="loadFolderEntries(folderProps.root)">
              {{ t("workspace.refreshFolder") }}
            </button>
          </div>
          <p v-if="folderError" class="mt-3 rounded-[4px] border border-red-500/20 bg-red-50 px-3 py-2 font-mono text-[9px] text-red-700">
            {{ folderError }}
          </p>
          <p v-else-if="loadingPaths.has(folderProps.root) && !childrenByPath[folderProps.root]" class="mt-4 font-serif text-[10px] text-ink/45">
            {{ t("workspace.loadingFolder") }}
          </p>
          <FolderExplorerTree
            v-else
            class="mt-3"
            :entries="childrenByPath[folderProps.root] ?? []"
            :children-by-path="childrenByPath"
            :expanded-paths="expandedPaths"
            :loading-paths="loadingPaths"
            :icon-theme="activeIconTheme"
            :resolve-icon-theme-asset="resolveIconThemeAsset"
            @toggle-folder="toggleFolder"
          />
          <p v-if="!loadingPaths.has(folderProps.root) && !(childrenByPath[folderProps.root]?.length) && !folderError" class="mt-4 font-serif text-[10px] text-ink/45">
            {{ t("workspace.emptyFolder") }}
          </p>
        </section>
        <p
          v-if="!folderProps.projects.length"
          class="rounded-[5px] border border-dashed border-ink/20 px-5 py-12 text-center font-serif text-[12px] text-ink/45"
        >
          {{ t("workspace.noFolderProjects") }}
        </p>
        <div v-else class="space-y-2">
          <article
            v-for="project in folderProps.projects"
            :key="project.path"
            class="flex items-center gap-4 rounded-[5px] border border-ink/15 p-4"
          >
            <span class="text-xl text-blue">▣</span>
            <div class="min-w-0 flex-1">
              <h3 class="font-serif text-[14px]">{{ project.title }}</h3>
              <p class="mt-1 font-serif text-[9px] text-ink/50">
                {{ project.discipline }} · {{ project.nodeCount }}
                {{ t("workspace.nodes") }} · {{ project.edgeCount }}
                {{ t("workspace.relations") }}
              </p>
              <p
                class="mt-1 truncate font-mono text-[8px] text-ink/35"
                :title="project.path"
              >
                {{ project.path }}
              </p>
            </div>
            <button
              class="button-secondary"
              @click="emit('open', project.path)"
            >
              {{ t("workspace.openProject") }}
            </button>
          </article>
        </div>
      </div>
    </section>

    <section
      v-else
      class="grid h-[min(680px,calc(100vh-32px))] w-[min(920px,calc(100vw-32px))] grid-cols-1 overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)] md:grid-cols-[minmax(0,1fr)_300px]"
    >
      <div
        class="flex min-h-0 flex-col border-b border-ink/15 md:border-b-0 md:border-r"
      >
        <header
          class="flex items-start justify-between border-b border-ink/15 px-7 py-5"
        >
          <div>
            <span
              class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue"
              >{{ t("workspace.gitWorkspace") }}</span
            >
            <h2 class="mt-1 flex items-center gap-2 font-serif text-[21px]">
              ⌘
              {{
                folderProps.snapshot?.branch ||
                (folderProps.snapshot?.isRepository
                  ? "HEAD"
                  : t("workspace.gitNotRepository"))
              }}
            </h2>
            <p
              class="mt-1 max-w-[520px] truncate font-mono text-[8px] text-ink/45"
              :title="folderProps.snapshot?.repoPath"
            >
              {{ folderProps.snapshot?.repoPath }}
            </p>
          </div>
          <button
            class="icon-quiet"
            @click="close"
            :aria-label="t('menu.close')"
          >
            ×
          </button>
        </header>
        <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
          <div class="mb-4 flex items-center justify-between">
            <p
              class="font-sans text-[8px] uppercase tracking-[0.16em] text-ink/45"
            >
              {{ t("workspace.gitTree") }} ·
              {{ folderProps.snapshot?.commits.length }}
            </p>
            <span
              v-if="folderProps.snapshot?.isRepository"
              class="font-serif text-[9px]"
              :class="
                folderProps.snapshot.dirty ? 'text-red-500' : 'text-olive'
              "
              >{{
                folderProps.snapshot.dirty
                  ? t("workspace.gitDirty")
                  : t("workspace.gitClean")
              }}</span
            >
          </div>
          <div
            v-if="!folderProps.snapshot?.isRepository"
            class="grid min-h-[260px] place-items-center rounded-[6px] border border-dashed border-ink/20 bg-canvas p-8 text-center"
          >
            <div>
              <span class="mx-auto block text-3xl text-ink/35">⌘</span>
              <h3 class="mt-4 font-serif text-[15px]">
                {{ t("workspace.gitNotRepository") }}
              </h3>
              <p
                class="mx-auto mt-2 max-w-[360px] font-serif text-[10px] leading-[1.55] text-ink/50"
              >
                {{ t("workspace.gitNotRepositoryHint") }}
              </p>
            </div>
          </div>
          <p
            v-else-if="!folderProps.snapshot?.commits.length"
            class="rounded-[6px] border border-dashed border-ink/20 bg-canvas px-4 py-10 text-center font-serif text-[10px] text-ink/45"
          >
            {{ t("workspace.gitNoCommits") }}
          </p>
          <div class="space-y-2">
            <article
              v-for="commit in folderProps.snapshot?.commits"
              :key="commit.id"
              class="relative ml-2 flex gap-3 border-l border-ink/20 py-2 pl-5 pr-3"
            >
              <span class="mt-0.5 shrink-0 text-blue">●</span>
              <div class="min-w-0 flex-1">
                <p class="truncate font-serif text-[12px]">
                  {{ commit.message.split("\n")[0] || commit.shortId }}
                </p>
                <p class="mt-1 font-mono text-[8px] text-ink/45">
                  {{ commit.shortId }} · {{ commit.author }} ·
                  {{ commit.timestamp.slice(0, 10) }}
                </p>
                <p
                  v-if="commit.parents.length"
                  class="mt-1 truncate font-mono text-[7px] text-ink/35"
                >
                  →
                  {{
                    commit.parents
                      .map((parent) => parent.slice(0, 8))
                      .join(" · ")
                  }}
                </p>
                <div
                  v-if="commit.refs.length"
                  class="mt-2 flex flex-wrap gap-1"
                >
                  <span
                    v-for="reference in commit.refs"
                    :key="reference"
                    class="rounded-[3px] bg-blue-soft px-1.5 py-0.5 font-sans text-[7px] text-blue"
                    >{{ reference }}</span
                  >
                </div>
              </div>
            </article>
          </div>
        </div>
      </div>
      <aside class="flex min-h-0 flex-col overflow-y-auto bg-canvas p-5">
        <div class="border-b border-ink/15 pb-5">
          <div class="flex items-center gap-2 text-blue">
            ⌘
            <h3 class="font-serif text-[14px]">
              {{ t("workspace.githubAccount") }}
            </h3>
            <button
              class="ml-auto grid size-7 place-items-center rounded-[4px] text-ink/45 transition hover:bg-blue-soft hover:text-blue"
              :disabled="folderProps.busy"
              @click="emit('refresh-account')"
              :aria-label="t('workspace.githubRefresh')"
            >
              ↻
            </button>
          </div>
          <p
            v-if="!folderProps.account?.cliAvailable"
            class="mt-2 font-serif text-[9px] leading-[1.5] text-ink/50"
          >
            {{ t("workspace.githubCliRequired") }}
          </p>
          <div
            v-else-if="folderProps.account.authenticated"
            class="mt-3 rounded-[5px] border border-ink/15 bg-paper p-3"
          >
            <p class="font-serif text-[11px] text-ink/85">
              @{{ folderProps.account.login }}
            </p>
            <p class="mt-1 font-mono text-[7px] text-ink/40">
              {{ folderProps.account.host }} ·
              {{ folderProps.account.gitProtocol ?? "git" }}
            </p>
          </div>
          <template v-else
            ><p class="mt-2 font-serif text-[9px] leading-[1.5] text-ink/50">
              {{ t("workspace.githubLoginHint") }}
            </p>
            <button
              class="button-primary mt-3 w-full justify-center"
              :disabled="folderProps.busy"
              @click="emit('login')"
            >
              ⌘ {{ t("workspace.githubLogin") }}
            </button></template
          >
          <div v-if="folderProps.account?.sshKeygenAvailable" class="mt-4">
            <p class="font-serif text-[11px]">
              🔑 {{ t("workspace.githubSshKeys") }}
            </p>
            <div class="mt-2 space-y-2">
              <article
                v-for="key in folderProps.account.sshKeys"
                :key="key.path"
                class="rounded-[4px] border border-ink/12 bg-paper p-2.5"
              >
                <p
                  class="truncate font-mono text-[7px] text-ink/55"
                  :title="key.path"
                >
                  {{ key.managedByApp ? "Research Canvas · " : ""
                  }}{{ key.algorithm }}
                </p>
                <p class="mt-1 truncate font-mono text-[7px] text-ink/35">
                  {{ key.fingerprint || key.path }}
                </p>
                <div class="mt-2 flex gap-1.5">
                  <button
                    class="button-secondary min-h-7 flex-1 justify-center px-2 text-[8px]"
                    @click="copyPublicKey(key.publicKey)"
                  >
                    ▣ {{ t("workspace.githubCopyKey") }}</button
                  ><button
                    v-if="folderProps.account.authenticated"
                    class="button-secondary min-h-7 flex-1 justify-center px-2 text-[8px]"
                    :disabled="folderProps.busy"
                    @click="emit('upload-ssh-key', key.path)"
                  >
                    ↑ {{ t("workspace.githubUploadKey") }}
                  </button>
                </div>
              </article>
              <button
                v-if="
                  !folderProps.account.sshKeys.some((key) => key.managedByApp)
                "
                class="button-secondary w-full justify-center"
                :disabled="folderProps.busy"
                @click="emit('generate-ssh-key')"
              >
                🔑 {{ t("workspace.githubGenerateKey") }}
              </button>
            </div>
          </div>
        </div>
        <div class="pt-5">
          <template v-if="folderProps.snapshot?.isRepository"
            ><div class="flex items-center gap-2 text-blue">
              ◷
              <h3 class="font-serif text-[14px]">
                {{ t("workspace.gitAutosave") }}
              </h3>
            </div>
            <p class="mt-2 font-serif text-[10px] leading-[1.5] text-ink/50">
              {{ t("workspace.gitAutosaveHint") }}
            </p>
            <label
              class="mt-4 flex items-center justify-between rounded-[5px] border border-ink/15 bg-paper p-3"
              ><span class="font-serif text-[11px]">5 min</span
              ><input
                type="checkbox"
                :checked="folderProps.autoSave"
                @change="
                  emit(
                    'toggle-auto-save',
                    ($event.target as HTMLInputElement).checked,
                  )
                " /></label
            ><button
              class="button-secondary mt-3 justify-center"
              :disabled="folderProps.busy"
              @click="emit('save-now')"
            >
              ↻ {{ t("workspace.gitSaveNow") }}
            </button>
            <div class="mt-6 border-t border-ink/15 pt-5">
              <div class="flex items-center gap-2">
                <span class="text-blue">✓</span>
                <h3 class="font-serif text-[14px]">GraphPatch</h3>
              </div>
              <p class="mt-2 font-serif text-[10px] leading-[1.5] text-ink/50">
                {{
                  folderProps.patch
                    ? `${acceptedCount}/${folderProps.patch.operations.length} ${t("workspace.patchOperations")} · ${folderProps.patch.summary}`
                    : t("workspace.noPatch")
                }}
              </p>
              <div v-if="folderProps.patch" class="mt-3 space-y-1">
                <button
                  v-for="(operation, index) in folderProps.patch.operations"
                  :key="`${operation.op}-${index}`"
                  type="button"
                  class="flex w-full items-center gap-2 truncate rounded-[3px] border px-2 py-1.5 text-left font-mono text-[7px] transition"
                  :class="
                    accepted[index] === false
                      ? 'border-ink/10 bg-canvas text-ink/30 line-through'
                      : 'border-ink/10 bg-paper text-ink/55'
                  "
                  :title="patchSubject(operation)"
                  @click="
                    accepted = {
                      ...accepted,
                      [index]: accepted[index] === false,
                    }
                  "
                >
                  <span
                    class="grid size-3.5 shrink-0 place-items-center rounded-[3px] border"
                    :class="
                      accepted[index] === false
                        ? 'border-ink/15'
                        : 'border-blue bg-blue-soft text-blue'
                    "
                    >{{ accepted[index] === false ? "" : "✓" }}</span
                  ><span class="min-w-0 flex-1 truncate"
                    >{{ operation.op }} · {{ patchSubject(operation) }}</span
                  >
                </button>
              </div>
              <button
                class="button-primary mt-4 w-full justify-center"
                :disabled="!folderProps.patch || acceptedCount === 0"
                @click="
                  emit(
                    'apply-patch',
                    buildAcceptedPatch(folderProps.patch ?? null),
                  )
                "
              >
                {{
                  rejectedCount > 0
                    ? `${t("workspace.reviewApplyPatch")} (${acceptedCount})`
                    : t("workspace.reviewApplyPatch")
                }}
              </button>
            </div></template
          ><template v-else
            ><div class="flex items-center gap-2 text-blue">
              ⌘
              <h3 class="font-serif text-[14px]">
                {{ t("workspace.gitInitialize") }}
              </h3>
            </div>
            <p class="mt-2 font-serif text-[10px] leading-[1.5] text-ink/50">
              {{ t("workspace.gitInitializeHint") }}
            </p>
            <button
              class="button-primary mt-4 w-full justify-center"
              :disabled="folderProps.busy"
              @click="emit('initialize')"
            >
              ↻ {{ t("workspace.gitInitialize") }}
            </button></template
          >
        </div>
      </aside>
    </section>
  </div>
</template>

<style scoped>
/* Layout and visual tokens are provided by the shared utility stylesheet. */
</style>
