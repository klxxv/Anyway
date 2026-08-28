<script setup lang="ts">
import { computed, ref, watch } from "vue";
import type { MessageKey } from "../../../app/i18n/catalog";
import type { Locale } from "../../../app/i18n/catalog";
import { NODE_TYPES } from "../../../app/lib/research-types";
import type {
  ProjectState,
  ResearchNodeType,
} from "../../../app/lib/research-types";
import {
  defaultWorkspacePreferences,
  type WorkspacePreferences,
} from "../../../app/features/research-workspace/workspace-preferences";
import { layoutOptions } from "../../../app/features/research-workspace/workspace-layout";
import {
  CONTEXT_MENU_ACTIONS,
  type ContextMenuActionId,
  type ContextMenuScope,
} from "../../../app/features/research-workspace/workspace-context-menu";
import {
  RADIAL_MENU_ACTIONS,
  RADIAL_MENU_POSITIONS,
  nodeTypeForRadialAction,
  type RadialMenuAction,
  type RadialMenuPosition,
} from "../../../app/features/research-workspace/workspace-radial-menu";
import {
  SHORTCUT_ACTIONS,
  defaultWorkspaceShortcuts,
  shortcutConflicts,
  shortcutFromKeyboardEvent,
  type ShortcutAction,
} from "../../../app/features/research-workspace/workspace-shortcuts";
import type { NodeDraft } from "../../../app/features/research-workspace/workspace-types";
import {
  nodeTypeMessageKeys,
  usePanelI18n,
  type ComposerState,
  type ProjectMenuProps,
  type SearchPaletteProps,
  type SettingsDialogProps,
} from "./panel-types";

type View = "composer" | "search" | "project-menu" | "settings";
type WorkspaceDialogsProps = {
  view: View;
  state?: ComposerState;
  project?: ProjectState;
  preferences?: WorkspacePreferences;
  onClose: () => void;
  onCreate?: (draft: NodeDraft, x: number, y: number) => void;
  onSelect?: (nodeId: string) => void;
  onReset?: () => void;
  onSettings?: () => void;
  onPlugins?: () => void;
  onSaveProject?: () => void;
  onImportProject?: () => void;
  onFolderWorkspace?: () => void;
  onGitWorkspace?: () => void;
  onSave?: (preferences: WorkspacePreferences) => void;
};

const props = defineProps<WorkspaceDialogsProps>();
const { locale, t, syncLocale } = usePanelI18n();
const nodeTypes = NODE_TYPES.filter((type): type is ResearchNodeType =>
  [
    "question",
    "concept",
    "variable",
    "method",
    "dataset",
    "evidence",
    "result",
    "note",
  ].includes(type),
);
const typeLabelKeys: Partial<Record<ResearchNodeType, MessageKey>> = {
  question: "node.question",
  concept: "node.group",
  variable: "node.variable",
  method: "node.method",
  dataset: "node.data",
  evidence: "node.evidence",
  result: "node.result",
  note: "node.note",
};
const valueFor = (draft: NodeDraft, key: string) =>
  String(draft.data[key] ?? "");

function defaultDataForType(
  type: ResearchNodeType,
  currentLocale: Locale = "en",
): Record<string, unknown> {
  switch (type) {
    case "question":
      return { questionKind: "causal", scope: "current study" };
    case "concept":
      return { groupRole: "theme", discipline: "" };
    case "variable":
      return {
        valueType: "enum",
        enumValues:
          currentLocale === "zh-CN"
            ? ["低", "中", "高"]
            : ["low", "medium", "high"],
        unit: "",
        observationRole: "measured",
        instances: [],
      };
    case "method":
      return {
        methodFamily: "observational",
        input: "",
        output: "",
        reproducible: true,
      };
    case "dataset":
      return { format: "table", source: "", coverage: "", resolution: "" };
    case "evidence":
      return {
        sourceKind: "article",
        citation: "",
        year: new Date().getFullYear(),
        confidence: "medium",
      };
    case "result":
      return {
        outcome: "supports",
        metric: "",
        confidence: 0.8,
        direction: "positive",
      };
    case "note":
      return {
        noteKind: "observation",
        author: currentLocale === "zh-CN" ? "本地研究者" : "local researcher",
      };
    default:
      return {};
  }
}
function makeDraft(
  type: ResearchNodeType,
  currentLocale: Locale = "en",
): NodeDraft {
  return {
    title: "",
    body: "",
    type,
    tags: [],
    data: defaultDataForType(type, currentLocale),
  };
}

const draft = ref<NodeDraft>(
  makeDraft(props.state?.type ?? "note", locale.value),
);
watch(
  () => props.state?.type,
  (type) => {
    if (type) draft.value = makeDraft(type, locale.value);
  },
  { immediate: true },
);
const updateData = (key: string, value: unknown) => {
  const next =
    key === "enumValues" && typeof value === "string"
      ? value
          .split(",")
          .map((item) => item.trim())
          .filter(Boolean)
      : value;
  draft.value = { ...draft.value, data: { ...draft.value.data, [key]: next } };
};
const createNode = () => {
  if (props.state && props.onCreate)
    props.onCreate(draft.value, props.state.x, props.state.y);
};
const selectNode = (nodeId: string) => props.onSelect?.(nodeId);

const query = ref("");
const searchResults = computed(() => {
  const project = props.project;
  if (!project) return [];
  const normalized = query.value.trim().toLowerCase();
  return project.nodes
    .filter(
      (node) =>
        !normalized ||
        `${node.title} ${node.body} ${node.tags.join(" ")}`
          .toLowerCase()
          .includes(normalized),
    )
    .slice(0, 7);
});
const closing = ref(false);
const requestClose = () => {
  if (closing.value) return;
  closing.value = true;
  window.setTimeout(props.onClose, 180);
};

type SettingsSection =
  | "interface"
  | "interaction"
  | "radial-menu"
  | "shortcuts"
  | "context-menus"
  | "canvas";
const section = ref<SettingsSection>("interface");
const settingsDraft = ref<WorkspacePreferences>(
  structuredClone(props.preferences ?? defaultWorkspacePreferences),
);
watch(
  () => props.preferences,
  (value) => {
    if (value) settingsDraft.value = structuredClone(value);
  },
);
const recordingShortcut = ref<ShortcutAction | null>(null);
const contextMenuScope = ref<ContextMenuScope>("node");
const conflicts = computed(() =>
  shortcutConflicts(settingsDraft.value.shortcuts),
);
const shortcutRows = computed(() =>
  SHORTCUT_ACTIONS.map((action) => ({
    action,
    label: t(`shortcut.${action}` as MessageKey),
  })),
);
const contextEntries = computed(() => {
  const enabled = settingsDraft.value.contextMenus[contextMenuScope.value];
  return [
    ...enabled,
    ...CONTEXT_MENU_ACTIONS[contextMenuScope.value]
      .map((item) => item.id)
      .filter((id) => !enabled.includes(id as ContextMenuActionId)),
  ]
    .map((id) => ({
      id: id as ContextMenuActionId,
      definition: CONTEXT_MENU_ACTIONS[contextMenuScope.value].find(
        (item) => item.id === id,
      ),
      enabled: enabled.includes(id as ContextMenuActionId),
      index: enabled.indexOf(id as ContextMenuActionId),
    }))
    .filter((entry) => entry.definition);
});
const toggleContextAction = (
  scope: ContextMenuScope,
  action: ContextMenuActionId,
) => {
  const current = settingsDraft.value.contextMenus[scope];
  settingsDraft.value = {
    ...settingsDraft.value,
    contextMenus: {
      ...settingsDraft.value.contextMenus,
      [scope]: current.includes(action)
        ? current.filter((item) => item !== action)
        : [...current, action],
    },
  };
};
const moveContextAction = (
  scope: ContextMenuScope,
  action: ContextMenuActionId,
  direction: -1 | 1,
) => {
  const actions = [...settingsDraft.value.contextMenus[scope]];
  const index = actions.indexOf(action);
  const nextIndex = index + direction;
  if (index < 0 || nextIndex < 0 || nextIndex >= actions.length) return;
  [actions[index], actions[nextIndex]] = [actions[nextIndex], actions[index]];
  settingsDraft.value = {
    ...settingsDraft.value,
    contextMenus: { ...settingsDraft.value.contextMenus, [scope]: actions },
  };
};
const recordShortcut = (action: ShortcutAction, event: KeyboardEvent) => {
  event.preventDefault();
  event.stopPropagation();
  if (event.key === "Escape") {
    recordingShortcut.value = null;
    return;
  }
  if (event.key === "Backspace" || event.key === "Delete") {
    settingsDraft.value = {
      ...settingsDraft.value,
      shortcuts: { ...settingsDraft.value.shortcuts, [action]: "" },
    };
    recordingShortcut.value = null;
    return;
  }
  const binding = shortcutFromKeyboardEvent(event);
  if (!binding) return;
  settingsDraft.value = {
    ...settingsDraft.value,
    shortcuts: { ...settingsDraft.value.shortcuts, [action]: binding },
  };
  recordingShortcut.value = null;
};
const setRadialItemCount = (count: number) => {
  const items = settingsDraft.value.radialMenu.items
    .slice(0, count)
    .map((item) => ({ ...item }));
  const occupied = new Set(items.map((item) => item.position));
  while (items.length < count) {
    const position = RADIAL_MENU_POSITIONS.find(
      (candidate) => !occupied.has(candidate),
    );
    if (!position) break;
    occupied.add(position);
    items.push({
      id: `radial-custom-${position}`,
      position,
      action: RADIAL_MENU_ACTIONS[items.length % RADIAL_MENU_ACTIONS.length],
    });
  }
  settingsDraft.value = { ...settingsDraft.value, radialMenu: { items } };
};
const updateRadialItem = (
  id: string,
  update: Partial<{ position: RadialMenuPosition; action: RadialMenuAction }>,
) => {
  settingsDraft.value = {
    ...settingsDraft.value,
    radialMenu: {
      items: settingsDraft.value.radialMenu.items.map((item) =>
        item.id === id ? { ...item, ...update } : item,
      ),
    },
  };
};
const radialActionLabel = (action: RadialMenuAction) => {
  const nodeType = nodeTypeForRadialAction(action);
  return nodeType
    ? t(typeLabelKeys[nodeType] ?? "node.note")
    : t(
        action === "canvas:fit"
          ? "contextMenu.fitView"
          : "contextMenu.applyLayout",
      );
};
const saveSettings = () => props.onSave?.(settingsDraft.value);
const restoreSettings = () => {
  settingsDraft.value = structuredClone(defaultWorkspacePreferences);
};
const setLocale = (next: string) => {
  if (typeof window !== "undefined")
    window.localStorage.setItem("research-canvas.locale.v1", next);
  syncLocale();
};
const inputTypeOptions = (type: ResearchNodeType) =>
  type === "question"
    ? [
        ["causal", t("option.causal")],
        ["descriptive", t("option.descriptive")],
        ["comparative", t("option.comparative")],
        ["exploratory", t("option.exploratory")],
      ]
    : type === "concept"
      ? [
          ["theme", t("option.theme")],
          ["population", t("option.population")],
          ["mechanism", t("option.mechanism")],
          ["context", t("option.context")],
        ]
      : type === "method"
        ? [
            ["observational", t("option.observational")],
            ["experimental", t("option.experimental")],
            ["classification", t("option.classification")],
            ["simulation", t("option.simulation")],
            ["statistical", t("option.statistical")],
          ]
        : type === "dataset"
          ? [
              ["table", t("option.table")],
              ["raster", t("option.raster")],
              ["vector", t("option.vector")],
              ["time-series", t("option.timeSeries")],
              ["text", t("option.textCorpus")],
            ]
          : [];
</script>

<template>
  <div
    v-if="props.view === 'composer'"
    class="fixed inset-0 z-[80] grid place-items-center bg-ink/10 backdrop-blur-[2px]"
  >
    <form
      class="flex max-h-[86vh] w-[590px] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]"
      @submit.prevent="createNode"
    >
      <div
        class="flex shrink-0 items-start justify-between border-b border-ink/15 px-7 pb-5 pt-6"
      >
        <div>
          <span
            class="font-sans text-[9px] uppercase tracking-[0.18em] text-blue"
            >{{ t("composer.eyebrow") }}</span
          >
          <h2 class="mt-1 font-serif text-[20px]">{{ t("composer.title") }}</h2>
        </div>
        <button
          type="button"
          class="icon-quiet"
          @click="props.onClose"
          :aria-label="t('composer.close')"
        >
          ×
        </button>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto px-7 py-5">
        <label class="dialog-field"
          ><span>{{ t("composer.type") }}</span
          ><select
            :value="draft.type"
            @change="
              draft = {
                ...makeDraft(
                  ($event.target as HTMLSelectElement)
                    .value as ResearchNodeType,
                  locale,
                ),
                title: draft.title,
                body: draft.body,
                tags: draft.tags,
              }
            "
          >
            <option v-for="type in nodeTypes" :key="type" :value="type">
              {{ t(typeLabelKeys[type] ?? "node.note") }}
            </option>
          </select></label
        ><label class="dialog-field"
          ><span>{{ t("composer.nodeTitle") }}</span
          ><input
            v-model="draft.title"
            autofocus
            :placeholder="t('composer.nodeTitleHint')"
        /></label>
        <div class="mb-4 flex items-center gap-3">
          <span
            class="font-sans text-[9px] uppercase tracking-[0.16em] text-ink/45"
            >{{ t("composer.profile") }}</span
          ><span class="h-px flex-1 bg-ink/12" /><span class="text-blue">✓</span
          ><span class="font-serif text-[10px] text-ink/55">{{
            t("composer.typeSpecific")
          }}</span>
        </div>
        <template v-if="inputTypeOptions(draft.type).length"
          ><label class="dialog-field"
            ><span>{{
              draft.type === "question"
                ? t("composer.questionKind")
                : draft.type === "concept"
                  ? t("composer.groupRole")
                  : draft.type === "method"
                    ? t("composer.methodFamily")
                    : t("composer.dataFormat")
            }}</span
            ><select
              :value="
                valueFor(
                  draft,
                  draft.type === 'question'
                    ? 'questionKind'
                    : draft.type === 'concept'
                      ? 'groupRole'
                      : draft.type === 'method'
                        ? 'methodFamily'
                        : 'format',
                )
              "
              @change="
                updateData(
                  draft.type === 'question'
                    ? 'questionKind'
                    : draft.type === 'concept'
                      ? 'groupRole'
                      : draft.type === 'method'
                        ? 'methodFamily'
                        : 'format',
                  ($event.target as HTMLSelectElement).value,
                )
              "
            >
              <option
                v-for="option in inputTypeOptions(draft.type)"
                :key="option[0]"
                :value="option[0]"
              >
                {{ option[1] }}
              </option>
            </select></label
          ></template
        ><label class="dialog-field"
          ><span>{{ t("composer.note") }}</span
          ><textarea
            v-model="draft.body"
            :placeholder="t('composer.noteHint')"
          /></label
        ><label class="dialog-field"
          ><span>{{ t("composer.tags") }}</span
          ><input
            :value="draft.tags.join(', ')"
            :placeholder="t('composer.tagsHint')"
            @input="
              draft.tags = ($event.target as HTMLInputElement).value
                .split(',')
                .map((item) => item.trim())
                .filter(Boolean)
            "
        /></label>
      </div>
      <footer
        class="flex shrink-0 justify-end gap-2 border-t border-ink/15 px-7 py-4"
      >
        <button type="button" class="button-secondary" @click="props.onClose">
          {{ t("composer.cancel") }}</button
        ><button type="submit" class="button-primary">
          {{ t("composer.create") }} →
        </button>
      </footer>
    </form>
  </div>

  <div
    v-else-if="props.view === 'search'"
    class="fixed inset-0 z-[70] flex justify-center bg-ink/8 pt-[12vh] backdrop-blur-[1px]"
  >
    <section
      class="h-fit w-[520px] overflow-hidden rounded-[7px] border border-ink/25 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.14)]"
    >
      <div class="flex h-13 items-center gap-3 border-b border-ink/15 px-4">
        <span class="text-lg">⌕</span
        ><input
          v-model="query"
          autofocus
          class="h-full flex-1 border-0 bg-transparent font-serif text-[15px] outline-none"
          :placeholder="t('search.placeholder')"
        /><button
          class="icon-quiet"
          @click="props.onClose"
          :aria-label="t('search.close')"
        >
          ×
        </button>
      </div>
      <div class="p-2">
        <p
          v-if="!searchResults.length"
          class="px-3 py-8 text-center font-serif text-[11px] text-ink/45"
        >
          {{ t("search.noResults") }}
        </p>
        <button
          v-for="node in searchResults"
          :key="node.id"
          class="flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left hover:bg-blue-soft"
          @click="selectNode(node.id)"
        >
          <span
            class="w-16 font-sans text-[8px] uppercase tracking-[0.12em] text-ink/45"
            >{{ t(typeLabelKeys[node.type] ?? "node.note") }}</span
          ><span class="min-w-0 flex-1 truncate font-serif text-[13px]">{{
            node.title
          }}</span
          ><span class="text-ink/45">→</span>
        </button>
      </div>
    </section>
  </div>

  <div
    v-else-if="props.view === 'project-menu'"
    class="project-menu-layer fixed inset-x-0 bottom-0 top-12 z-[60]"
    :class="closing ? 'is-closing' : ''"
    :aria-hidden="closing"
    @pointerdown.self="requestClose"
  >
    <aside
      class="project-menu-panel flex h-full w-[290px] flex-col border-r border-ink/20 bg-paper shadow-[12px_0_40px_rgba(30,32,35,.08)]"
    >
      <div
        class="flex h-14 items-center justify-between border-b border-ink/15 px-5"
      >
        <h2 class="font-serif text-[18px]">{{ t("workspace.projects") }}</h2>
        <button
          class="icon-quiet"
          @click="requestClose"
          :aria-label="t('menu.close')"
        >
          ×
        </button>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto p-4">
        <p class="font-sans text-[9px] uppercase tracking-[0.16em] text-ink/45">
          {{ t("workspace.currentStudy") }}
        </p>
        <div
          v-if="props.project"
          class="mt-3 rounded-[5px] border border-blue/30 bg-blue-soft p-3"
        >
          <div class="flex items-center gap-2">
            <span class="text-blue">▣</span
            ><span class="font-serif text-[14px]">{{
              props.project.title
            }}</span>
          </div>
          <p class="mt-1 pl-6 font-serif text-[10px] text-ink/55">
            {{ props.project.nodes.length }} {{ t("workspace.nodes") }} ·
            {{ props.project.edges.length }} {{ t("workspace.relations") }}
          </p>
        </div>
        <div class="mt-4 grid grid-cols-2 gap-2">
          <button
            class="button-secondary justify-center"
            @click="props.onSaveProject"
          >
            ▣ {{ t("workspace.saveProject") }}</button
          ><button
            class="button-secondary justify-center"
            @click="props.onImportProject"
          >
            ⇧ {{ t("workspace.importProject") }}
          </button>
        </div>
        <div
          v-if="props.onFolderWorkspace || props.onGitWorkspace"
          class="mt-4 space-y-1 border-t border-ink/12 pt-4"
        >
          <button
            v-if="props.onFolderWorkspace"
            class="flex w-full items-center gap-3 rounded-[4px] px-2 py-2.5 text-left hover:bg-blue-soft"
            @click="props.onFolderWorkspace"
          >
            ▣
            <span class="font-serif text-[13px]">{{
              t("workspace.folderMode")
            }}</span></button
          ><button
            v-if="props.onGitWorkspace"
            class="flex w-full items-center gap-3 rounded-[4px] px-2 py-2.5 text-left hover:bg-blue-soft"
            @click="props.onGitWorkspace"
          >
            ⌘
            <span class="font-serif text-[13px]">{{
              t("workspace.gitWorkspace")
            }}</span>
          </button>
        </div>
        <div v-if="props.project" class="mt-6 space-y-1">
          <button
            class="flex w-full items-center gap-3 rounded-[4px] px-2 py-2.5 text-left hover:bg-blue-soft"
          >
            ▤
            <span class="flex-1 font-serif text-[13px]">{{
              t("workspace.evidenceLibrary")
            }}</span
            ><span class="font-serif text-[9px] text-ink/45"
              >{{ props.project.evidence.length }}
              {{ t("workspace.sources") }}</span
            ></button
          ><button
            class="flex w-full items-center gap-3 rounded-[4px] px-2 py-2.5 text-left hover:bg-blue-soft"
          >
            ◷
            <span class="flex-1 font-serif text-[13px]">{{
              t("workspace.researchHistory")
            }}</span
            ><span class="font-serif text-[9px] text-ink/45"
              >{{ t("workspace.revision") }} {{ props.project.revision }}</span
            >
          </button>
        </div>
        <button
          class="mt-8 inline-flex items-center gap-2 font-serif text-[11px] text-ink/60 hover:text-blue"
          @click="props.onReset"
        >
          ↻ {{ t("workspace.restoreDemo") }}
        </button>
      </div>
      <div class="border-t border-ink/15 p-3">
        <button
          class="flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left transition hover:bg-blue-soft hover:text-blue"
          @click="props.onPlugins"
        >
          ◇
          <span class="flex-1 font-serif text-[13px]">{{
            t("workspace.pluginStore")
          }}</span
          >→</button
        ><button
          class="mt-1 flex w-full items-center gap-3 rounded-[4px] px-3 py-2.5 text-left transition hover:bg-blue-soft hover:text-blue"
          @click="props.onSettings"
        >
          ⚙
          <span class="flex-1 font-serif text-[13px]">{{
            t("workspace.settings")
          }}</span
          >→
        </button>
      </div>
    </aside>
  </div>

  <div
    v-else-if="props.view === 'settings'"
    class="fixed inset-0 z-[90] grid place-items-center bg-ink/10 backdrop-blur-[2px]"
  >
    <section
      class="flex h-[min(720px,calc(100vh-32px))] w-[min(900px,calc(100vw-32px))] flex-col overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_18px_60px_rgba(30,32,35,.15)]"
    >
      <header
        class="flex shrink-0 items-start justify-between border-b border-ink/15 px-7 py-5"
      >
        <div>
          <span
            class="font-sans text-[8px] uppercase tracking-[0.18em] text-blue"
            >{{ t("settings.eyebrow") }}</span
          >
          <h2 class="mt-1 font-serif text-[21px]">{{ t("settings.title") }}</h2>
          <p class="mt-1 font-serif text-[10px] text-ink/50">
            {{ t("settings.subtitle") }}
          </p>
        </div>
        <button
          class="icon-quiet"
          @click="props.onClose"
          :aria-label="t('menu.close')"
        >
          ×
        </button>
      </header>
      <div class="grid min-h-0 flex-1 grid-cols-[210px_minmax(0,1fr)]">
        <nav class="border-r border-ink/15 bg-canvas p-3">
          <button
            v-for="entry in [
              ['interface', t('settings.interface')],
              ['interaction', t('settings.interaction')],
              ['radial-menu', t('settings.radialMenu')],
              ['shortcuts', t('settings.shortcuts')],
              ['context-menus', t('settings.contextMenus')],
              ['canvas', t('settings.canvas')],
            ] as Array<[SettingsSection, string]>"
            :key="entry[0]"
            class="flex w-full items-center gap-3 rounded-[4px] px-3 py-3 text-left transition"
            :class="
              section === entry[0]
                ? 'bg-paper text-blue shadow-sm'
                : 'hover:bg-ink/5'
            "
            @click="section = entry[0]"
          >
            ⚙
            <span
              ><span class="block font-serif text-[12px]">{{
                entry[1]
              }}</span></span
            >
          </button>
        </nav>
        <div class="min-h-0 overflow-y-auto px-7 py-6">
          <div v-if="section === 'interface'">
            <h3 class="font-serif text-[18px]">
              {{ t("settings.interface") }}
            </h3>
            <p class="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
              {{ t("settings.commandDensity") }}
            </p>
            <label class="dialog-field mt-6"
              >{{ t("settings.language")
              }}<select
                :value="locale"
                @change="setLocale(($event.target as HTMLSelectElement).value)"
              >
                <option value="en">English</option>
                <option value="zh-CN">简体中文</option>
              </select></label
            ><label class="dialog-field"
              >{{ t("settings.commandDensity")
              }}<select v-model="settingsDraft.commandDensity">
                <option value="comfortable">
                  {{ t("settings.comfortable") }}
                </option>
                <option value="compact">{{ t("settings.compact") }}</option>
              </select></label
            >
          </div>
          <div v-else-if="section === 'interaction'">
            <h3 class="font-serif text-[18px]">
              {{ t("settings.interaction") }}
            </h3>
            <p class="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
              {{ t("settings.hoverBehavior") }}
            </p>
            <label class="dialog-field mt-6"
              >{{ t("settings.openingDelay")
              }}<select v-model.number="settingsDraft.hoverDelay">
                <option :value="80">{{ t("settings.fast") }} · 80 ms</option>
                <option :value="180">
                  {{ t("settings.balanced") }} · 180 ms
                </option>
                <option :value="320">
                  {{ t("settings.deliberate") }} · 320 ms
                </option>
              </select></label
            ><label
              class="mt-5 block font-sans text-[9px] uppercase tracking-[0.12em] text-ink/55"
              >{{ t("settings.trackpadSensitivity") }}
              <output class="font-medium text-blue"
                >{{
                  Math.round(settingsDraft.trackpadSensitivity * 100)
                }}%</output
              ><input
                v-model.number="settingsDraft.trackpadSensitivity"
                class="mt-2 w-full accent-blue"
                type="range"
                min="0.5"
                max="2"
                step="0.1" /></label
            ><label
              class="mt-5 block font-sans text-[9px] uppercase tracking-[0.12em] text-ink/55"
              >{{ t("settings.trackpadLowPass") }}
              <output class="font-medium text-blue"
                >{{
                  Math.round(settingsDraft.trackpadFilterStrength * 100)
                }}%</output
              ><input
                v-model.number="settingsDraft.trackpadFilterStrength"
                class="mt-2 w-full accent-blue"
                type="range"
                min="0"
                max="0.9"
                step="0.05"
            /></label>
          </div>
          <div v-else-if="section === 'radial-menu'">
            <h3 class="font-serif text-[18px]">
              {{ t("settings.radialMenuTitle") }}
            </h3>
            <p class="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
              {{ t("settings.radialMenuDescription") }}
            </p>
            <label class="dialog-field mt-5"
              >{{ t("settings.radialItemCount")
              }}<select
                :value="settingsDraft.radialMenu.items.length"
                @change="
                  setRadialItemCount(
                    Number(($event.target as HTMLSelectElement).value),
                  )
                "
              >
                <option v-for="count in 8" :key="count" :value="count">
                  {{ count }}
                </option>
              </select></label
            >
            <div class="mt-4 space-y-2">
              <div
                v-for="(item, index) in settingsDraft.radialMenu.items"
                :key="item.id"
                class="grid grid-cols-[26px_minmax(120px,.8fr)_minmax(150px,1.2fr)] items-end gap-2 rounded-[5px] border border-ink/15 bg-paper p-3"
              >
                <span
                  class="mb-2 grid size-6 place-items-center rounded-full bg-blue-soft font-sans text-[9px] font-bold text-blue"
                  >{{ index + 1 }}</span
                ><label class="dialog-field"
                  >{{ t("settings.radialPosition")
                  }}<select
                    :value="item.position"
                    @change="
                      updateRadialItem(item.id, {
                        position: ($event.target as HTMLSelectElement)
                          .value as RadialMenuPosition,
                      })
                    "
                  >
                    <option
                      v-for="position in RADIAL_MENU_POSITIONS"
                      :key="position"
                      :value="position"
                    >
                      {{ position }}
                    </option>
                  </select></label
                ><label class="dialog-field"
                  >{{ t("settings.radialAction")
                  }}<select
                    :value="item.action"
                    @change="
                      updateRadialItem(item.id, {
                        action: ($event.target as HTMLSelectElement)
                          .value as RadialMenuAction,
                      })
                    "
                  >
                    <option
                      v-for="action in RADIAL_MENU_ACTIONS"
                      :key="action"
                      :value="action"
                    >
                      {{ radialActionLabel(action) }}
                    </option>
                  </select></label
                >
              </div>
            </div>
          </div>
          <div v-else-if="section === 'shortcuts'">
            <h3 class="font-serif text-[18px]">
              {{ t("settings.shortcutBindings") }}
            </h3>
            <p class="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
              {{ t("settings.shortcutBindingsHint") }}
            </p>
            <div class="mt-4 divide-y divide-ink/10 border-y border-ink/15">
              <div
                v-for="row in shortcutRows"
                :key="row.action"
                class="flex min-h-9 items-center gap-3 py-1.5"
              >
                <span class="min-w-0 flex-1 font-serif text-[12px]">{{
                  row.label
                }}</span
                ><button
                  class="min-w-[112px] rounded-[4px] border px-3 py-1.5 text-center font-sans text-[10px] font-semibold transition"
                  :class="
                    conflicts.has(row.action)
                      ? 'border-alert/65 bg-alert/5 text-alert'
                      : recordingShortcut === row.action
                        ? 'border-blue bg-blue-soft text-blue'
                        : 'border-ink/20 bg-paper hover:border-blue/55 hover:text-blue'
                  "
                  @click="recordingShortcut = row.action"
                  @keydown="
                    recordingShortcut === row.action
                      ? recordShortcut(row.action, $event)
                      : undefined
                  "
                >
                  {{
                    recordingShortcut === row.action
                      ? t("settings.pressShortcut")
                      : settingsDraft.shortcuts[row.action] ||
                        t("settings.unassigned")
                  }}</button
                ><button
                  class="w-10 text-right font-serif text-[9px] text-ink/40 hover:text-blue"
                  @click="
                    settingsDraft.shortcuts[row.action] =
                      defaultWorkspaceShortcuts[row.action]
                  "
                >
                  {{ t("settings.resetBinding") }}
                </button>
              </div>
            </div>
            <p
              class="mt-3 font-serif text-[10px]"
              :class="conflicts.size ? 'text-alert' : 'text-ink/45'"
            >
              {{
                conflicts.size
                  ? t("settings.shortcutConflict")
                  : t("settings.shortcutCaptureHint")
              }}
            </p>
          </div>
          <div v-else-if="section === 'context-menus'">
            <h3 class="font-serif text-[18px]">
              {{ t("settings.contextMenuTitle") }}
            </h3>
            <p class="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
              {{ t("settings.contextMenuDescription") }}
            </p>
            <div
              class="mt-5 grid grid-cols-3 rounded-[5px] border border-ink/15 bg-canvas p-1"
            >
              <button
                v-for="scope in [
                  'node',
                  'edge',
                  'canvas',
                ] as ContextMenuScope[]"
                :key="scope"
                class="rounded-[4px] px-3 py-2 font-serif text-[11px] transition"
                :class="
                  contextMenuScope === scope
                    ? 'bg-paper text-blue shadow-sm'
                    : 'text-ink/55 hover:text-ink'
                "
                @click="contextMenuScope = scope"
              >
                {{
                  t(
                    scope === "node"
                      ? "settings.contextNode"
                      : scope === "edge"
                        ? "settings.contextEdge"
                        : "settings.contextCanvas",
                  )
                }}
              </button>
            </div>
            <div
              class="mt-4 overflow-hidden rounded-[5px] border border-ink/15"
            >
              <div
                v-for="entry in contextEntries"
                :key="entry.id"
                class="flex min-h-11 items-center gap-3 border-b border-ink/10 px-3 last:border-b-0"
                :class="entry.enabled ? 'bg-paper' : 'bg-canvas text-ink/45'"
              >
                <label
                  class="flex min-w-0 flex-1 cursor-pointer items-center gap-3"
                  ><input
                    class="size-3.5 accent-blue"
                    type="checkbox"
                    :checked="entry.enabled"
                    @change="toggleContextAction(contextMenuScope, entry.id)"
                  /><span class="font-serif text-[11px]">{{
                    t(entry.definition!.labelKey)
                  }}</span></label
                >
                <div class="flex items-center gap-0.5">
                  <button
                    class="icon-quiet size-7"
                    :disabled="!entry.enabled || entry.index === 0"
                    @click="moveContextAction(contextMenuScope, entry.id, -1)"
                  >
                    ↑</button
                  ><button
                    class="icon-quiet size-7"
                    :disabled="
                      !entry.enabled ||
                      entry.index ===
                        settingsDraft.contextMenus[contextMenuScope].length - 1
                    "
                    @click="moveContextAction(contextMenuScope, entry.id, 1)"
                  >
                    ↓
                  </button>
                </div>
              </div>
            </div>
            <label
              class="mt-4 flex cursor-pointer items-center gap-4 rounded-[5px] border border-blue/20 bg-blue-soft p-3"
              ><span class="min-w-0 flex-1 font-serif text-[12px]">{{
                t("settings.pluginMenuActions")
              }}</span
              ><input
                type="checkbox"
                v-model="settingsDraft.showPluginContextMenuActions"
            /></label>
          </div>
          <div v-else>
            <h3 class="font-serif text-[18px]">
              {{ t("settings.graphDefaults") }}
            </h3>
            <p class="mt-1 font-serif text-[11px] leading-[1.5] text-ink/50">
              {{ t("settings.graphDefaultsHint") }}
            </p>
            <label class="dialog-field mt-6"
              >{{ t("settings.defaultFilterLayout")
              }}<select v-model="settingsDraft.defaultLayout">
                <option
                  v-for="option in layoutOptions"
                  :key="option.mode"
                  :value="option.mode"
                >
                  {{ option.mode }}
                </option>
              </select></label
            ><label
              class="flex cursor-pointer items-center gap-4 border-b border-ink/12 py-4"
              ><span class="min-w-0 flex-1"
                ><span class="block font-serif text-[13px]">{{
                  t("settings.showMinimap")
                }}</span
                ><span class="mt-1 block font-serif text-[10px] text-ink/50">{{
                  t("settings.showMinimapHint")
                }}</span></span
              ><input
                type="checkbox"
                v-model="settingsDraft.showMiniMap" /></label
            ><label
              class="flex cursor-pointer items-center gap-4 border-b border-ink/12 py-4"
              ><span class="min-w-0 flex-1"
                ><span class="block font-serif text-[13px]">{{
                  t("settings.showLinkCounts")
                }}</span
                ><span class="mt-1 block font-serif text-[10px] text-ink/50">{{
                  t("settings.showLinkCountsHint")
                }}</span></span
              ><input type="checkbox" v-model="settingsDraft.showLinkCounts"
            /></label>
          </div>
        </div>
      </div>
      <footer
        class="flex h-16 items-center justify-between border-t border-ink/15 px-6"
      >
        <button
          class="font-serif text-[11px] text-ink/55 hover:text-blue"
          @click="restoreSettings"
        >
          {{ t("settings.restore") }}
        </button>
        <div class="flex gap-2">
          <button class="button-secondary" @click="props.onClose">
            {{ t("settings.cancel") }}</button
          ><button
            class="button-primary disabled:cursor-not-allowed disabled:opacity-40"
            :disabled="conflicts.size > 0"
            @click="saveSettings"
          >
            {{ t("settings.save") }} ✓
          </button>
        </div>
      </footer>
    </section>
  </div>
</template>

<style scoped>
/* Layout and visual tokens are provided by the shared utility stylesheet. */
</style>
