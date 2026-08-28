<script setup lang="ts">
import { computed, ref } from "vue";
import type { MessageKey } from "../../../app/i18n/catalog";
import {
  customEdgeNote,
  editableEdgeTypes,
  edgeTypeMessageKeys,
} from "../../../app/features/research-workspace/workspace-edge-labels";
import type {
  ResearchEdge,
  ResearchNode,
} from "../../../app/lib/research-types";
import {
  nodeTypeMessageKeys,
  usePanelI18n,
  valueTypeOf,
  type InspectorPanelProps,
  type VariableValueType,
} from "./panel-types";

const props = defineProps<InspectorPanelProps>();
const { t } = usePanelI18n();
const actionsOpen = ref(false);
type VariableInstance = { id: string; label: string; value: string };

const activeNode = computed(() => props.node);
const activeEdge = computed(() => props.edge);
const valueType = (node: ResearchNode) => valueTypeOf(node);
const enumValues = (node: ResearchNode) =>
  Array.isArray(node.data.enumValues)
    ? node.data.enumValues.filter(
        (value): value is string => typeof value === "string",
      )
    : ["low", "medium", "high"];
const instances = (node: ResearchNode): VariableInstance[] => {
  if (Array.isArray(node.data.instances)) {
    return node.data.instances.flatMap((item) => {
      if (!item || typeof item !== "object") return [];
      const candidate = item as Partial<VariableInstance>;
      return typeof candidate.id === "string" &&
        typeof candidate.label === "string"
        ? [
            {
              id: candidate.id,
              label: candidate.label,
              value: String(candidate.value ?? ""),
            },
          ]
        : [];
    });
  }
  const type = valueType(node);
  const values = enumValues(node);
  return [
    {
      id: `${node.id}-instance-a`,
      label: `${node.title} · A`,
      value:
        type === "enum" ? (values[0] ?? "") : type === "bool" ? "true" : "",
    },
    {
      id: `${node.id}-instance-b`,
      label: `${node.title} · B`,
      value:
        type === "enum"
          ? (values[1] ?? values[0] ?? "")
          : type === "bool"
            ? "false"
            : "",
    },
  ];
};
const updateNodeData = (data: Record<string, unknown>) => {
  if (props.node)
    props.onUpdate(props.node.id, { data: { ...props.node.data, ...data } });
};
const updateEnumValue = (node: ResearchNode, index: number, value: string) => {
  const next = [...enumValues(node)];
  next[index] = value;
  updateNodeData({ enumValues: next });
};
const removeEnumValue = (node: ResearchNode, index: number) =>
  updateNodeData({
    enumValues: enumValues(node).filter((_, itemIndex) => itemIndex !== index),
  });
const updateInstance = (
  node: ResearchNode,
  index: number,
  update: Partial<VariableInstance>,
) => {
  const next = instances(node).map((item, itemIndex) =>
    itemIndex === index ? { ...item, ...update } : item,
  );
  updateNodeData({ instances: next });
};
const removeInstance = (node: ResearchNode, index: number) =>
  updateNodeData({
    instances: instances(node).filter((_, itemIndex) => itemIndex !== index),
  });
const addInstance = (node: ResearchNode) => {
  const type = valueType(node);
  updateNodeData({
    instances: [
      ...instances(node),
      {
        id: `instance-${Date.now()}`,
        label: t("inspector.instanceLabel"),
        value:
          type === "enum"
            ? (enumValues(node)[0] ?? "")
            : type === "bool"
              ? "true"
              : "",
      },
    ],
  });
};
const setTags = (node: ResearchNode, value: string) =>
  props.onUpdate(node.id, {
    tags: value
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean),
  });
const edgeSource = computed(() =>
  activeEdge.value
    ? props.nodes.find((node) => node.id === activeEdge.value?.source)
    : null,
);
const edgeTarget = computed(() =>
  activeEdge.value
    ? props.nodes.find((node) => node.id === activeEdge.value?.target)
    : null,
);
const edgeConditions = computed(
  () => activeEdge.value?.conditions.join(", ") ?? "",
);
const edgeVisibleNote = computed(() =>
  activeEdge.value ? customEdgeNote(activeEdge.value) : "",
);
const edgeLabel = (edge: ResearchEdge) => t(edgeTypeMessageKeys[edge.type]);
const inspectorLabel = (key: string) => t(key as MessageKey);
</script>

<template>
  <aside
    class="h-full w-[320px] shrink-0 overflow-y-auto border-l border-ink/15 bg-canvas px-5 pb-4 pt-7 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
  >
    <div
      class="overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_4px_12px_rgba(28,31,35,0.06)]"
    >
      <section v-if="activeNode" class="border-b border-ink/20 px-4 pb-5 pt-4">
        <div class="mb-2 flex items-start gap-2">
          <input
            class="min-w-0 flex-1 border-0 bg-transparent p-0 font-serif text-[14px] leading-tight text-ink outline-none"
            :value="activeNode.title"
            :aria-label="t('inspector.nodeTitle')"
            @input="
              props.onUpdate(activeNode.id, {
                title: ($event.target as HTMLInputElement).value,
              })
            "
          /><button class="icon-quiet" :aria-label="t('inspector.pinNode')">
            ⌖</button
          ><button
            class="icon-quiet"
            :aria-label="t('inspector.close')"
            @click="props.onClose"
          >
            ×
          </button>
        </div>
        <div class="mb-4 flex items-center justify-between">
          <span
            class="rounded-[5px] border border-ink/25 bg-paper px-2 py-1 font-serif text-[10px] text-olive"
            >{{ t(nodeTypeMessageKeys[activeNode.type] ?? "node.note") }} ·
            {{ valueType(activeNode) }}</span
          >
          <div class="relative">
            <button
              class="icon-quiet"
              :aria-label="t('inspector.moreActions')"
              @click="actionsOpen = !actionsOpen"
            >
              ⋯
            </button>
            <div
              v-if="actionsOpen"
              class="absolute right-0 top-8 z-20 w-32 rounded-[4px] border border-ink/20 bg-paper p-1 shadow-lg"
            >
              <button
                class="flex w-full items-center gap-2 rounded-[3px] px-2 py-2 font-serif text-[11px] text-alert hover:bg-blue-soft"
                @click="props.onDelete(activeNode.id)"
              >
                ⌫ {{ t("inspector.deleteNode") }}
              </button>
            </div>
          </div>
        </div>

        <label class="inspector-row"
          ><span>{{ t("inspector.status") }}</span
          ><select
            :value="activeNode.status"
            @change="
              props.onUpdate(activeNode.id, {
                status: ($event.target as HTMLSelectElement)
                  .value as ResearchNode['status'],
              })
            "
          >
            <option value="draft">draft</option>
            <option value="confirmed">confirmed</option>
            <option value="disputed">disputed</option>
            <option value="deprecated">deprecated</option>
          </select></label
        >
        <label class="inspector-row"
          ><span>{{ t("inspector.tags") }}</span
          ><input
            :value="activeNode.tags.join(', ')"
            @input="
              setTags(activeNode, ($event.target as HTMLInputElement).value)
            "
        /></label>

        <template v-if="activeNode.type === 'variable'">
          <label class="inspector-row"
            ><span>{{ t("inspector.type") }}</span
            ><select
              :value="valueType(activeNode)"
              @change="
                updateNodeData({
                  valueType: ($event.target as HTMLSelectElement)
                    .value as VariableValueType,
                })
              "
            >
              <option value="enum">enum</option>
              <option value="bool">bool</option>
              <option value="number">number</option>
              <option value="text">text</option>
            </select></label
          >
          <div class="mt-4">
            <div class="mb-2 flex items-center justify-between">
              <span class="font-serif text-[12px] text-ink">{{
                t("inspector.values")
              }}</span
              ><button
                v-if="valueType(activeNode) === 'enum'"
                class="inline-flex items-center gap-1 font-serif text-[11px] text-ink/80 hover:text-blue"
                @click="
                  updateNodeData({
                    enumValues: [
                      ...enumValues(activeNode),
                      t('inspector.newValue'),
                    ],
                  })
                "
              >
                ＋ {{ t("inspector.addValue") }}
              </button>
            </div>
            <div
              v-if="
                valueType(activeNode) === 'enum' ||
                valueType(activeNode) === 'bool'
              "
              class="space-y-2"
            >
              <div
                v-for="(value, index) in valueType(activeNode) === 'enum'
                  ? enumValues(activeNode)
                  : ['true', 'false']"
                :key="`${value}-${index}`"
                class="flex h-9 items-center gap-2 rounded-[4px] border border-ink/25 bg-paper px-2"
              >
                <span class="text-ink/45">⋮⋮</span
                ><input
                  class="min-w-0 flex-1 border-0 bg-transparent font-serif text-[11px] outline-none"
                  :value="value"
                  :readonly="valueType(activeNode) !== 'enum'"
                  @input="
                    updateEnumValue(
                      activeNode,
                      index,
                      ($event.target as HTMLInputElement).value,
                    )
                  "
                /><button
                  v-if="valueType(activeNode) === 'enum'"
                  class="icon-quiet"
                  :aria-label="`${t('inspector.removeValue')} ${value}`"
                  @click="removeEnumValue(activeNode, index)"
                >
                  ×
                </button>
              </div>
            </div>
            <label
              v-if="
                valueType(activeNode) === 'number' ||
                valueType(activeNode) === 'text'
              "
              class="inspector-row"
              ><span>{{ t("inspector.unit") }}</span
              ><input
                class="w-[142px] border-0 bg-transparent text-right font-serif text-[11px] outline-none focus:text-blue"
                :value="
                  typeof activeNode.data.unit === 'string'
                    ? activeNode.data.unit
                    : ''
                "
                @input="
                  updateNodeData({
                    unit: ($event.target as HTMLInputElement).value,
                  })
                "
            /></label>
          </div>
          <div class="mt-5 border-t border-ink/15 pt-4">
            <div class="flex items-start justify-between gap-3">
              <div>
                <p class="font-serif text-[12px]">
                  {{ t("inspector.instances") }}
                </p>
                <p class="mt-1 font-serif text-[9px] leading-[1.4] text-ink/45">
                  {{ t("inspector.instancesHint") }}
                </p>
              </div>
              <button
                class="inline-flex shrink-0 items-center gap-1 font-serif text-[10px] text-blue"
                @click="addInstance(activeNode)"
              >
                ＋ {{ t("inspector.addInstance") }}
              </button>
            </div>
            <div class="mt-3 space-y-2">
              <div
                v-for="(instance, index) in instances(activeNode)"
                :key="instance.id"
                class="rounded-[4px] border border-ink/20 bg-paper p-2"
              >
                <div class="flex items-center gap-2">
                  <input
                    class="min-w-0 flex-1 border-0 bg-transparent font-serif text-[10px] outline-none focus:text-blue"
                    :value="instance.label"
                    :aria-label="t('inspector.instanceLabel')"
                    @input="
                      updateInstance(activeNode, index, {
                        label: ($event.target as HTMLInputElement).value,
                      })
                    "
                  /><select
                    v-if="valueType(activeNode) === 'enum'"
                    class="max-w-20 border-0 bg-transparent font-serif text-[10px] outline-none"
                    :value="instance.value"
                    @change="
                      updateInstance(activeNode, index, {
                        value: ($event.target as HTMLSelectElement).value,
                      })
                    "
                  >
                    <option
                      v-for="value in enumValues(activeNode)"
                      :key="value"
                      :value="value"
                    >
                      {{ value }}
                    </option></select
                  ><select
                    v-else-if="valueType(activeNode) === 'bool'"
                    class="border-0 bg-transparent font-serif text-[10px] outline-none"
                    :value="instance.value"
                    @change="
                      updateInstance(activeNode, index, {
                        value: ($event.target as HTMLSelectElement).value,
                      })
                    "
                  >
                    <option value="true">true</option>
                    <option value="false">false</option></select
                  ><input
                    v-else
                    class="w-20 border-0 bg-transparent text-right font-serif text-[10px] outline-none focus:text-blue"
                    :value="instance.value"
                    :aria-label="t('inspector.instanceValue')"
                    @input="
                      updateInstance(activeNode, index, {
                        value: ($event.target as HTMLInputElement).value,
                      })
                    "
                  /><button
                    class="icon-quiet"
                    :aria-label="t('inspector.removeInstance')"
                    @click="removeInstance(activeNode, index)"
                  >
                    ×
                  </button>
                </div>
              </div>
            </div>
          </div>
        </template>
        <label class="mt-4 block"
          ><span class="mb-2 block font-serif text-[12px]">{{
            t("inspector.notes")
          }}</span
          ><textarea
            class="min-h-14 w-full resize-none border-0 bg-transparent p-0 font-serif text-[11px] leading-[1.4] text-ink outline-none"
            :value="activeNode.body"
            @input="
              props.onUpdate(activeNode.id, {
                body: ($event.target as HTMLTextAreaElement).value,
              })
            "
          />
        </label>
        <button
          class="mt-4 inline-flex w-full items-center justify-center gap-2 rounded-[4px] border border-alert/25 px-3 py-2 font-serif text-[10px] text-alert transition hover:border-alert/50 hover:bg-alert/5"
          @click="props.onDelete(activeNode.id)"
        >
          ⌫ {{ t("inspector.deleteNode") }}
        </button>
      </section>

      <section v-else-if="activeEdge" class="px-4 pb-5 pt-4">
        <div class="mb-3 flex items-start gap-2">
          <div class="min-w-0 flex-1">
            <p
              class="font-sans text-[8px] uppercase tracking-[0.16em] text-blue"
            >
              {{ t("inspector.relation") }}
            </p>
            <div
              class="mt-1 flex items-center gap-1.5 font-serif text-[13px] leading-tight"
            >
              <span class="truncate">{{
                edgeSource?.title ?? activeEdge.source
              }}</span
              ><span class="shrink-0 text-ink/45">→</span
              ><span class="truncate">{{
                edgeTarget?.title ?? activeEdge.target
              }}</span>
            </div>
          </div>
          <button
            class="icon-quiet"
            :aria-label="t('inspector.close')"
            @click="props.onClose"
          >
            ×
          </button>
        </div>
        <div class="mb-4 flex items-center gap-2">
          <span
            class="inline-flex items-center gap-1.5 rounded-[5px] border border-blue/25 bg-blue-soft px-2 py-1 font-serif text-[10px] text-blue"
            >⌁ {{ edgeLabel(activeEdge) }}</span
          ><span class="text-ink/35">{{
            activeEdge.directed ? "→" : "↔"
          }}</span>
        </div>
        <label class="inspector-row"
          ><span>{{ t("inspector.edgeType") }}</span
          ><select
            :value="activeEdge.type"
            @change="
              props.onUpdateEdge(activeEdge.id, {
                type: ($event.target as HTMLSelectElement)
                  .value as ResearchEdge['type'],
                ...(edgeVisibleNote ? {} : { note: undefined }),
              })
            "
          >
            <option v-for="type in editableEdgeTypes" :key="type" :value="type">
              {{ t(edgeTypeMessageKeys[type]) }}
            </option>
          </select></label
        >
        <label class="inspector-row"
          ><span>{{ t("inspector.source") }}</span
          ><select
            class="max-w-[178px] truncate"
            :value="activeEdge.source"
            @change="
              props.onUpdateEdge(activeEdge.id, {
                source: ($event.target as HTMLSelectElement).value,
              })
            "
          >
            <option
              v-for="node in props.nodes.filter(
                (item) => item.id !== activeEdge?.target,
              )"
              :key="node.id"
              :value="node.id"
            >
              {{ node.title }}
            </option>
          </select></label
        >
        <label class="inspector-row"
          ><span>{{ t("inspector.target") }}</span
          ><select
            class="max-w-[178px] truncate"
            :value="activeEdge.target"
            @change="
              props.onUpdateEdge(activeEdge.id, {
                target: ($event.target as HTMLSelectElement).value,
              })
            "
          >
            <option
              v-for="node in props.nodes.filter(
                (item) => item.id !== activeEdge?.source,
              )"
              :key="node.id"
              :value="node.id"
            >
              {{ node.title }}
            </option>
          </select></label
        >
        <label class="inspector-row"
          ><span>{{ t("inspector.direction") }}</span
          ><select
            :value="activeEdge.directed ? 'directed' : 'undirected'"
            @change="
              props.onUpdateEdge(activeEdge.id, {
                directed:
                  ($event.target as HTMLSelectElement).value === 'directed',
              })
            "
          >
            <option value="directed">{{ t("inspector.directed") }}</option>
            <option value="undirected">{{ t("inspector.undirected") }}</option>
          </select></label
        >
        <label class="inspector-row"
          ><span>{{ t("inspector.polarity") }}</span
          ><select
            :value="activeEdge.polarity"
            @change="
              props.onUpdateEdge(activeEdge.id, {
                polarity: ($event.target as HTMLSelectElement)
                  .value as ResearchEdge['polarity'],
              })
            "
          >
            <option
              v-for="polarity in ['positive', 'negative', 'mixed', 'unknown']"
              :key="polarity"
              :value="polarity"
            >
              {{ inspectorLabel(`inspector.${polarity}`) }}
            </option>
          </select></label
        >
        <label class="mt-4 block"
          ><span
            class="mb-2 flex items-center justify-between font-serif text-[12px]"
            >{{ t("inspector.confidence")
            }}<span class="font-sans text-[9px] text-blue"
              >{{ Math.round((activeEdge.confidence ?? 1) * 100) }}%</span
            ></span
          ><input
            class="w-full accent-blue"
            type="range"
            min="0"
            max="1"
            step="0.05"
            :value="activeEdge.confidence ?? 1"
            @input="
              props.onUpdateEdge(activeEdge.id, {
                confidence: Number(($event.target as HTMLInputElement).value),
              })
            "
        /></label>
        <label class="mt-4 block"
          ><span class="mb-2 block font-serif text-[12px]">{{
            t("inspector.edgeLabel")
          }}</span
          ><input
            class="h-9 w-full rounded-[4px] border border-ink/20 bg-paper px-2.5 font-serif text-[11px] outline-none focus:border-blue"
            :value="edgeVisibleNote"
            :placeholder="edgeLabel(activeEdge)"
            @input="
              props.onUpdateEdge(activeEdge.id, {
                note: ($event.target as HTMLInputElement).value,
              })
            "
        /></label>
        <label class="mt-4 block"
          ><span class="mb-2 block font-serif text-[12px]">{{
            t("inspector.conditions")
          }}</span
          ><textarea
            class="min-h-16 w-full resize-none rounded-[4px] border border-ink/20 bg-paper px-2.5 py-2 font-serif text-[11px] leading-[1.4] outline-none focus:border-blue"
            :value="edgeConditions"
            :placeholder="t('inspector.conditionsHint')"
            @input="
              props.onUpdateEdge(activeEdge.id, {
                conditions: ($event.target as HTMLTextAreaElement).value
                  .split(',')
                  .map((value) => value.trim())
                  .filter(Boolean),
              })
            "
          />
        </label>
        <div class="mt-5 grid grid-cols-2 gap-2 border-t border-ink/15 pt-4">
          <button
            class="button-secondary justify-center"
            @click="props.onReverseEdge(activeEdge.id)"
          >
            ↔ {{ t("inspector.reverseEdge") }}</button
          ><button
            class="button-secondary justify-center text-alert hover:border-alert/45 hover:text-alert"
            @click="props.onDeleteEdge(activeEdge.id)"
          >
            ⌫ {{ t("inspector.deleteEdge") }}
          </button>
        </div>
      </section>
      <div
        v-else
        class="px-5 py-12 text-center font-serif text-[12px] text-ink/55"
      >
        {{ t("inspector.selectObject") }}
      </div>
    </div>
  </aside>
</template>

<style scoped>
/* Layout and visual tokens are provided by the shared utility stylesheet. */
</style>
