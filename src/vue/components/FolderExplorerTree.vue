<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { FolderTreeEntry } from "../../../app/platform/native-project";
import type {
  IconThemeManifest,
  VsixIconDefinition,
} from "../../../app/plugins/vsix-contracts";

defineOptions({ name: "FolderExplorerTree" });

const props = withDefaults(
  defineProps<{
    entries: FolderTreeEntry[];
    childrenByPath: Readonly<Record<string, FolderTreeEntry[]>>;
    expandedPaths: ReadonlySet<string>;
    loadingPaths: ReadonlySet<string>;
    iconTheme?: IconThemeManifest;
    resolveIconThemeAsset?: (assetPath: string) => Promise<string | null>;
    depth?: number;
  }>(),
  { depth: 0 },
);

const emit = defineEmits<{
  (event: "toggle-folder", path: string): void;
  (event: "select-file", entry: FolderTreeEntry): void;
}>();

const indent = () => ({ paddingLeft: `${props.depth * 16 + 4}px` });

function iconId(entry: FolderTreeEntry): string {
  const iconTheme = props.iconTheme;
  if (!iconTheme) return entry.kind === "directory" ? "folder" : "file";
  const normalizedName = entry.name.toLocaleLowerCase();
  if (entry.kind === "directory") {
    return (props.expandedPaths.has(entry.path)
      ? iconTheme.folderNamesExpanded[normalizedName] ?? iconTheme.folderNamesExpanded[entry.name]
      : undefined)
      ?? iconTheme.folderNames[normalizedName]
      ?? iconTheme.folderNames[entry.name]
      ?? "folder";
  }
  const extension = normalizedName.includes(".")
    ? normalizedName.slice(normalizedName.lastIndexOf(".") + 1)
    : "";
  return iconTheme.fileNames[normalizedName]
    ?? iconTheme.fileNames[entry.name]
    ?? iconTheme.fileExtensions[extension]
    ?? iconTheme.fileExtensions[`.${extension}`]
    ?? "file";
}

function iconDefinition(entry: FolderTreeEntry): VsixIconDefinition | undefined {
  const id = iconId(entry);
  return props.iconTheme?.iconDefinitions[id];
}

function iconTitle(entry: FolderTreeEntry): string {
  const id = iconId(entry);
  const definition = iconDefinition(entry);
  if (definition?.iconPath) return `${id} · ${definition.iconPath}`;
  if (definition?.fontCharacter) return `${id} · font glyph fallback`;
  return id;
}

function fallbackIcon(entry: FolderTreeEntry): string {
  return entry.kind === "directory" ? "▰" : "▱";
}

const iconAssetUrls = ref<Record<string, string>>({});
const requestedIconAssets = new Set<string>();
const iconAssetKey = computed(() => props.entries
  .map((entry) => `${entry.path}:${iconDefinition(entry)?.iconPath ?? ""}`)
  .filter((entry) => !entry.endsWith(":"))
  .join("\u0000"));

function iconAssetUrl(entry: FolderTreeEntry): string | undefined {
  const path = iconDefinition(entry)?.iconPath;
  return path ? iconAssetUrls.value[path] : undefined;
}

function handleIconAssetError(assetPath: string): void {
  const next = { ...iconAssetUrls.value };
  delete next[assetPath];
  iconAssetUrls.value = next;
}

async function loadIconAssets(): Promise<void> {
  const resolver = props.resolveIconThemeAsset;
  if (!resolver || !props.iconTheme) return;
  const paths = new Set(
    props.entries
      .map((entry) => iconDefinition(entry)?.iconPath)
      .filter((path): path is string => Boolean(path)),
  );
  await Promise.all(
    [...paths].map(async (path) => {
      if (requestedIconAssets.has(path)) return;
      requestedIconAssets.add(path);
      try {
        const url = await resolver(path);
        if (url) iconAssetUrls.value = { ...iconAssetUrls.value, [path]: url };
      } catch {
        // A missing or rejected asset falls back to the host glyph below.
      }
    }),
  );
}

watch(
  [
    iconAssetKey,
    () => props.iconTheme ? `${props.iconTheme.id}@${props.iconTheme.version}` : "",
    () => props.resolveIconThemeAsset,
  ],
  () => {
    iconAssetUrls.value = {};
    requestedIconAssets.clear();
    void loadIconAssets();
  },
  { immediate: true },
);
</script>

<template>
  <div class="space-y-0.5">
    <template v-for="entry in props.entries" :key="entry.path">
      <button
        v-if="entry.kind === 'directory'"
        type="button"
        class="flex min-h-8 w-full items-center gap-1.5 rounded-[4px] px-1 text-left font-mono text-[10px] text-ink/70 transition hover:bg-blue-soft hover:text-blue"
        :style="indent()"
        :data-icon-theme-id="iconId(entry)"
        :title="iconTitle(entry)"
        :aria-expanded="props.expandedPaths.has(entry.path)"
        @click="emit('toggle-folder', entry.path)"
      >
        <span class="grid size-4 shrink-0 place-items-center text-[10px] text-ink/45">
          {{ props.loadingPaths.has(entry.path) ? "…" : props.expandedPaths.has(entry.path) ? "▾" : "▸" }}
        </span>
        <img
          v-if="iconAssetUrl(entry)"
          class="size-4 shrink-0 object-contain"
          :src="iconAssetUrl(entry)"
          :alt="iconId(entry)"
          @error="handleIconAssetError(iconDefinition(entry)?.iconPath ?? '')"
        />
        <span v-else class="text-[12px] text-blue/75">{{ fallbackIcon(entry) }}</span>
        <span class="min-w-0 flex-1 truncate">{{ entry.name }}</span>
      </button>
      <button
        v-else
        type="button"
        class="flex min-h-8 w-full items-center gap-1.5 rounded-[4px] px-1 text-left font-mono text-[10px] text-ink/60 transition hover:bg-blue-soft hover:text-blue"
        :style="indent()"
        :data-icon-theme-id="iconId(entry)"
        :title="iconTitle(entry)"
        @click="emit('select-file', entry)"
      >
        <span class="size-4 shrink-0" aria-hidden="true" />
        <img
          v-if="iconAssetUrl(entry)"
          class="size-4 shrink-0 object-contain"
          :src="iconAssetUrl(entry)"
          :alt="iconId(entry)"
          @error="handleIconAssetError(iconDefinition(entry)?.iconPath ?? '')"
        />
        <span v-else class="text-[12px] text-ink/35">{{ fallbackIcon(entry) }}</span>
        <span class="min-w-0 flex-1 truncate">{{ entry.name }}</span>
        <span v-if="entry.size" class="shrink-0 text-[8px] text-ink/35">
          {{ entry.size > 1024 * 1024 ? `${(entry.size / 1024 / 1024).toFixed(1)} MB` : `${Math.max(1, Math.round(entry.size / 1024))} KB` }}
        </span>
      </button>
      <FolderExplorerTree
        v-if="entry.kind === 'directory' && props.expandedPaths.has(entry.path)"
        :entries="props.childrenByPath[entry.path] ?? []"
        :children-by-path="props.childrenByPath"
        :expanded-paths="props.expandedPaths"
        :loading-paths="props.loadingPaths"
        :icon-theme="props.iconTheme"
        :resolve-icon-theme-asset="props.resolveIconThemeAsset"
        :depth="props.depth + 1"
        @toggle-folder="emit('toggle-folder', $event)"
        @select-file="emit('select-file', $event)"
      />
    </template>
  </div>
</template>

<style scoped>
/* Explorer rows intentionally remain host-rendered and contain no plugin markup. */
</style>
