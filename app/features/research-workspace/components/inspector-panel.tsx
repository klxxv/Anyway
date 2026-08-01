"use client";

import {
  IconArrowRight,
  IconArrowsExchange,
  IconDots,
  IconGripVertical,
  IconPin,
  IconPlus,
  IconRoute,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import { useState } from "react";
import type { MessageKey } from "../../../i18n/catalog";
import { useI18n } from "../../../i18n/provider";
import type { ResearchEdge, ResearchNode, ResearchNodeType } from "../../../lib/research-types";
import {
  customEdgeNote,
  edgeTypeMessageKeys,
  editableEdgeTypes,
} from "../workspace-edge-labels";
import type {
  EdgeInspectorUpdate,
  InspectorUpdate,
  VariableValueType,
} from "../workspace-types";

type InspectorPanelProps = {
  node: ResearchNode | null;
  edge: ResearchEdge | null;
  nodes: ResearchNode[];
  onUpdate: (nodeId: string, update: InspectorUpdate) => void;
  onUpdateEdge: (edgeId: string, update: EdgeInspectorUpdate) => void;
  onDelete: (nodeId: string) => void;
  onDeleteEdge: (edgeId: string) => void;
  onReverseEdge: (edgeId: string) => void;
  onClose: () => void;
};

type VariableInstance = {
  id: string;
  label: string;
  value: string;
};

const nodeTypeMessageKeys: Partial<Record<ResearchNodeType, MessageKey>> = {
  question: "node.question",
  concept: "node.group",
  variable: "node.variable",
  method: "node.method",
  dataset: "node.data",
  evidence: "node.evidence",
  result: "node.result",
  note: "node.note",
};

function valueTypeOf(node: ResearchNode): VariableValueType {
  const type = node.data.valueType;
  return type === "enum" || type === "bool" || type === "number" || type === "text"
    ? type
    : "text";
}

function InspectorCard({
  node,
  onUpdate,
  onDelete,
  onClose,
}: {
  node: ResearchNode;
  onUpdate: InspectorPanelProps["onUpdate"];
  onDelete: InspectorPanelProps["onDelete"];
  onClose: () => void;
}) {
  const { t } = useI18n();
  const valueType = valueTypeOf(node);
  const [actionsOpen, setActionsOpen] = useState(false);
  const enumValues = Array.isArray(node.data.enumValues)
    ? node.data.enumValues.filter((value): value is string => typeof value === "string")
    : ["low", "medium", "high"];
  const instances = Array.isArray(node.data.instances)
    ? node.data.instances.flatMap((item) => {
        if (!item || typeof item !== "object") return [];
        const candidate = item as Partial<VariableInstance>;
        return typeof candidate.id === "string" && typeof candidate.label === "string"
          ? [{ id: candidate.id, label: candidate.label, value: String(candidate.value ?? "") }]
          : [];
      })
    : [
        {
          id: `${node.id}-instance-a`,
          label: `${node.title} · A`,
          value:
            node.id === "variable-temperature"
              ? "34.2"
              : node.id === "variable-density"
                ? "0.72"
                : valueType === "enum"
                  ? enumValues[0] ?? ""
                  : valueType === "bool"
                    ? "true"
                    : "",
        },
        {
          id: `${node.id}-instance-b`,
          label: `${node.title} · B`,
          value:
            node.id === "variable-temperature"
              ? "31.6"
              : node.id === "variable-density"
                ? "0.48"
                : valueType === "enum"
                  ? enumValues[1] ?? enumValues[0] ?? ""
                  : valueType === "bool"
                    ? "false"
                    : "",
        },
      ];
  const updateData = (data: Record<string, unknown>) =>
    onUpdate(node.id, { data: { ...node.data, ...data } });

  return (
    <section className="border-b border-ink/20 px-4 pb-5 pt-4">
      <div className="mb-2 flex items-start gap-2">
        <input
          className="min-w-0 flex-1 border-0 bg-transparent p-0 font-serif text-[14px] leading-tight text-ink outline-none"
          value={node.title}
          onChange={(event) => onUpdate(node.id, { title: event.target.value })}
          aria-label={t("inspector.nodeTitle")}
        />
        <button className="icon-quiet" aria-label={t("inspector.pinNode")}>
          <IconPin size={17} stroke={1.35} />
        </button>
        <button className="icon-quiet" onClick={onClose} aria-label={t("inspector.close")}>
          <IconX size={18} stroke={1.35} />
        </button>
      </div>

      <div className="mb-4 flex items-center justify-between">
        <span className="rounded-[5px] border border-ink/25 bg-paper px-2 py-1 font-serif text-[10px] text-olive">
          {t(nodeTypeMessageKeys[node.type] ?? "node.note")} · {valueType}
        </span>
        <div className="relative">
          <button
            className="icon-quiet"
            aria-label={t("inspector.moreActions")}
            onClick={() => setActionsOpen((current) => !current)}
          >
            <IconDots size={18} stroke={1.4} />
          </button>
          {actionsOpen && (
            <div className="absolute right-0 top-8 z-20 w-32 rounded-[4px] border border-ink/20 bg-paper p-1 shadow-lg">
              <button
                className="flex w-full items-center gap-2 rounded-[3px] px-2 py-2 font-serif text-[11px] text-alert hover:bg-blue-soft"
                onClick={() => onDelete(node.id)}
              >
                <IconTrash size={14} stroke={1.3} />
                {t("inspector.deleteNode")}
              </button>
            </div>
          )}
        </div>
      </div>

      {node.type === "variable" && (
        <>
          <label className="inspector-row">
            <span>{t("inspector.type")}</span>
            <select
              value={valueType}
              onChange={(event) =>
                updateData({ valueType: event.target.value as VariableValueType })
              }
            >
              <option value="enum">enum</option>
              <option value="bool">bool</option>
              <option value="number">number</option>
              <option value="text">text</option>
            </select>
          </label>

          <div className="mt-4">
            <div className="mb-2 flex items-center justify-between">
              <span className="font-serif text-[12px] text-ink">{t("inspector.values")}</span>
              {valueType === "enum" && (
                <button
                  className="inline-flex items-center gap-1 font-serif text-[11px] text-ink/80 hover:text-blue"
                  onClick={() => updateData({ enumValues: [...enumValues, t("inspector.newValue")] })}
                >
                  <IconPlus size={15} stroke={1.35} />
                  {t("inspector.addValue")}
                </button>
              )}
            </div>
            {(valueType === "enum" || valueType === "bool") && (
            <div className="space-y-2">
              {(valueType === "enum" ? enumValues : ["true", "false"]).map((value, index) => (
                <div
                  key={`${value}-${index}`}
                  className="flex h-9 items-center gap-2 rounded-[4px] border border-ink/25 bg-paper px-2"
                >
                  <IconGripVertical size={14} stroke={1.2} className="text-ink/45" />
                  <input
                    className="min-w-0 flex-1 border-0 bg-transparent font-serif text-[11px] outline-none"
                    value={value}
                    readOnly={valueType !== "enum"}
                    onChange={(event) => {
                      const nextValues = [...enumValues];
                      nextValues[index] = event.target.value;
                      updateData({ enumValues: nextValues });
                    }}
                  />
                  {valueType === "enum" && (
                    <button
                      className="icon-quiet"
                      onClick={() =>
                        updateData({
                          enumValues: enumValues.filter((_, itemIndex) => itemIndex !== index),
                        })
                      }
                      aria-label={`${t("inspector.removeValue")} ${value}`}
                    >
                      <IconX size={14} stroke={1.3} />
                    </button>
                  )}
                </div>
              ))}
            </div>
            )}
            {(valueType === "number" || valueType === "text") && (
              <label className="inspector-row">
                <span>{t("inspector.unit")}</span>
                <input
                  className="w-[142px] border-0 bg-transparent text-right font-serif text-[11px] outline-none focus:text-blue"
                  value={typeof node.data.unit === "string" ? node.data.unit : ""}
                  placeholder={valueType === "number" ? "%, °C, kg" : "—"}
                  onChange={(event) => updateData({ unit: event.target.value })}
                />
              </label>
            )}
          </div>

          <div className="mt-5 border-t border-ink/15 pt-4">
            <div className="flex items-start justify-between gap-3">
              <div>
                <p className="font-serif text-[12px]">{t("inspector.instances")}</p>
                <p className="mt-1 font-serif text-[9px] leading-[1.4] text-ink/45">
                  {t("inspector.instancesHint")}
                </p>
              </div>
              <button
                className="inline-flex shrink-0 items-center gap-1 font-serif text-[10px] text-blue"
                onClick={() =>
                  updateData({
                    instances: [
                      ...instances,
                      {
                        id: `instance-${Date.now()}`,
                        label: t("inspector.instanceLabel"),
                        value: valueType === "enum" ? enumValues[0] ?? "" : valueType === "bool" ? "true" : "",
                      },
                    ],
                  })
                }
              >
                <IconPlus size={14} stroke={1.35} />
                {t("inspector.addInstance")}
              </button>
            </div>
            <div className="mt-3 space-y-2">
              {instances.map((instance, index) => (
                <div key={instance.id} className="rounded-[4px] border border-ink/20 bg-paper p-2">
                  <div className="flex items-center gap-2">
                    <input
                      className="min-w-0 flex-1 border-0 bg-transparent font-serif text-[10px] outline-none focus:text-blue"
                      value={instance.label}
                      aria-label={t("inspector.instanceLabel")}
                      onChange={(event) => {
                        const next = [...instances];
                        next[index] = { ...instance, label: event.target.value };
                        updateData({ instances: next });
                      }}
                    />
                    {valueType === "enum" ? (
                      <select
                        className="max-w-20 border-0 bg-transparent font-serif text-[10px] outline-none"
                        value={instance.value}
                        aria-label={t("inspector.instanceValue")}
                        onChange={(event) => {
                          const next = [...instances];
                          next[index] = { ...instance, value: event.target.value };
                          updateData({ instances: next });
                        }}
                      >
                        {enumValues.map((value) => <option key={value} value={value}>{value}</option>)}
                      </select>
                    ) : valueType === "bool" ? (
                      <select
                        className="border-0 bg-transparent font-serif text-[10px] outline-none"
                        value={instance.value}
                        aria-label={t("inspector.instanceValue")}
                        onChange={(event) => {
                          const next = [...instances];
                          next[index] = { ...instance, value: event.target.value };
                          updateData({ instances: next });
                        }}
                      >
                        <option value="true">true</option>
                        <option value="false">false</option>
                      </select>
                    ) : (
                      <input
                        className="w-20 border-0 bg-transparent text-right font-serif text-[10px] outline-none focus:text-blue"
                        value={instance.value}
                        aria-label={t("inspector.instanceValue")}
                        onChange={(event) => {
                          const next = [...instances];
                          next[index] = { ...instance, value: event.target.value };
                          updateData({ instances: next });
                        }}
                      />
                    )}
                    <button
                      className="icon-quiet"
                      aria-label={t("inspector.removeInstance")}
                      onClick={() => updateData({ instances: instances.filter((_, itemIndex) => itemIndex !== index) })}
                    >
                      <IconX size={14} stroke={1.3} />
                    </button>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </>
      )}

      <label className="mt-4 block">
        <span className="mb-2 block font-serif text-[12px]">{t("inspector.notes")}</span>
        <textarea
          className="min-h-14 w-full resize-none border-0 bg-transparent p-0 font-serif text-[11px] leading-[1.4] text-ink outline-none"
          value={node.body}
          onChange={(event) => onUpdate(node.id, { body: event.target.value })}
        />
      </label>

      <button
        className="mt-4 inline-flex w-full items-center justify-center gap-2 rounded-[4px] border border-alert/25 px-3 py-2 font-serif text-[10px] text-alert transition hover:border-alert/50 hover:bg-alert/5"
        onClick={() => onDelete(node.id)}
      >
        <IconTrash size={14} stroke={1.35} />
        {t("inspector.deleteNode")}
      </button>

    </section>
  );
}

function EdgeInspectorCard({
  edge,
  nodes,
  onUpdate,
  onDelete,
  onReverse,
  onClose,
}: {
  edge: ResearchEdge;
  nodes: ResearchNode[];
  onUpdate: (edgeId: string, update: EdgeInspectorUpdate) => void;
  onDelete: (edgeId: string) => void;
  onReverse: (edgeId: string) => void;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const source = nodes.find((node) => node.id === edge.source);
  const target = nodes.find((node) => node.id === edge.target);
  const conditions = edge.conditions.join(", ");
  const visibleNote = customEdgeNote(edge);

  return (
    <section className="px-4 pb-5 pt-4">
      <div className="mb-3 flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <p className="font-sans text-[8px] uppercase tracking-[0.16em] text-blue">
            {t("inspector.relation")}
          </p>
          <div className="mt-1 flex items-center gap-1.5 font-serif text-[13px] leading-tight">
            <span className="truncate">{source?.title ?? edge.source}</span>
            <IconArrowRight className="shrink-0 text-ink/45" size={14} stroke={1.35} />
            <span className="truncate">{target?.title ?? edge.target}</span>
          </div>
        </div>
        <button className="icon-quiet" onClick={onClose} aria-label={t("inspector.close")}>
          <IconX size={18} stroke={1.35} />
        </button>
      </div>

      <div className="mb-4 flex items-center gap-2">
        <span className="inline-flex items-center gap-1.5 rounded-[5px] border border-blue/25 bg-blue-soft px-2 py-1 font-serif text-[10px] text-blue">
          <IconRoute size={13} stroke={1.4} />
          {t(edgeTypeMessageKeys[edge.type])}
        </span>
        {edge.directed ? (
          <IconArrowRight size={14} stroke={1.35} className="text-ink/35" />
        ) : (
          <IconArrowsExchange size={14} stroke={1.35} className="text-ink/35" />
        )}
      </div>

      <label className="inspector-row">
        <span>{t("inspector.edgeType")}</span>
        <select
          value={edge.type}
          onChange={(event) =>
            onUpdate(edge.id, {
              type: event.target.value as ResearchEdge["type"],
              ...(visibleNote ? {} : { note: undefined }),
            })
          }
        >
          {editableEdgeTypes.map((type) => (
            <option key={type} value={type}>
              {t(edgeTypeMessageKeys[type])}
            </option>
          ))}
        </select>
      </label>

      <label className="inspector-row">
        <span>{t("inspector.source")}</span>
        <select
          className="max-w-[178px] truncate"
          value={edge.source}
          onChange={(event) => onUpdate(edge.id, { source: event.target.value })}
        >
          {nodes.filter((node) => node.id !== edge.target).map((node) => (
            <option key={node.id} value={node.id}>{node.title}</option>
          ))}
        </select>
      </label>

      <label className="inspector-row">
        <span>{t("inspector.target")}</span>
        <select
          className="max-w-[178px] truncate"
          value={edge.target}
          onChange={(event) => onUpdate(edge.id, { target: event.target.value })}
        >
          {nodes.filter((node) => node.id !== edge.source).map((node) => (
            <option key={node.id} value={node.id}>{node.title}</option>
          ))}
        </select>
      </label>

      <label className="inspector-row">
        <span>{t("inspector.direction")}</span>
        <select
          value={edge.directed ? "directed" : "undirected"}
          onChange={(event) => onUpdate(edge.id, { directed: event.target.value === "directed" })}
        >
          <option value="directed">{t("inspector.directed")}</option>
          <option value="undirected">{t("inspector.undirected")}</option>
        </select>
      </label>

      <label className="inspector-row">
        <span>{t("inspector.polarity")}</span>
        <select
          value={edge.polarity}
          onChange={(event) =>
            onUpdate(edge.id, { polarity: event.target.value as ResearchEdge["polarity"] })
          }
        >
          {(["positive", "negative", "mixed", "unknown"] as const).map((polarity) => (
            <option key={polarity} value={polarity}>
              {t(`inspector.${polarity}`)}
            </option>
          ))}
        </select>
      </label>

      <label className="mt-4 block">
        <span className="mb-2 flex items-center justify-between font-serif text-[12px]">
          {t("inspector.confidence")}
          <span className="font-sans text-[9px] text-blue">
            {Math.round((edge.confidence ?? 1) * 100)}%
          </span>
        </span>
        <input
          className="w-full accent-blue"
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={edge.confidence ?? 1}
          onChange={(event) => onUpdate(edge.id, { confidence: Number(event.target.value) })}
        />
      </label>

      <label className="mt-4 block">
        <span className="mb-2 block font-serif text-[12px]">{t("inspector.edgeLabel")}</span>
        <input
          className="h-9 w-full rounded-[4px] border border-ink/20 bg-paper px-2.5 font-serif text-[11px] outline-none focus:border-blue"
          value={visibleNote}
          placeholder={t(edgeTypeMessageKeys[edge.type])}
          onChange={(event) => onUpdate(edge.id, { note: event.target.value })}
        />
      </label>

      <label className="mt-4 block">
        <span className="mb-2 block font-serif text-[12px]">{t("inspector.conditions")}</span>
        <textarea
          className="min-h-16 w-full resize-none rounded-[4px] border border-ink/20 bg-paper px-2.5 py-2 font-serif text-[11px] leading-[1.4] outline-none focus:border-blue"
          value={conditions}
          placeholder={t("inspector.conditionsHint")}
          onChange={(event) =>
            onUpdate(edge.id, {
              conditions: event.target.value
                .split(",")
                .map((value) => value.trim())
                .filter(Boolean),
            })
          }
        />
      </label>

      <div className="mt-5 grid grid-cols-2 gap-2 border-t border-ink/15 pt-4">
        <button className="button-secondary justify-center" onClick={() => onReverse(edge.id)}>
          <IconArrowsExchange size={15} stroke={1.35} />
          {t("inspector.reverseEdge")}
        </button>
        <button
          className="button-secondary justify-center text-alert hover:border-alert/45 hover:text-alert"
          onClick={() => onDelete(edge.id)}
        >
          <IconTrash size={15} stroke={1.35} />
          {t("inspector.deleteEdge")}
        </button>
      </div>
    </section>
  );
}

/**
 * Quiet properties inspector; one editable selection plus a pinned observed fact.
 * 克制的属性检查器：一个可编辑选中项与一个置顶观测事实。
 */
export function InspectorPanel({
  node,
  edge,
  nodes,
  onUpdate,
  onUpdateEdge,
  onDelete,
  onDeleteEdge,
  onReverseEdge,
  onClose,
}: InspectorPanelProps) {
  const { t } = useI18n();
  return (
    <aside className="h-full w-[320px] shrink-0 overflow-y-auto border-l border-ink/15 bg-canvas px-5 pb-4 pt-7 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
      <div className="overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_4px_12px_rgba(28,31,35,0.06)]">
        {edge ? (
          <EdgeInspectorCard
            edge={edge}
            nodes={nodes}
            onUpdate={onUpdateEdge}
            onDelete={onDeleteEdge}
            onReverse={onReverseEdge}
            onClose={onClose}
          />
        ) : node ? (
          <InspectorCard
            node={node}
            onUpdate={onUpdate}
            onDelete={onDelete}
            onClose={onClose}
          />
        ) : (
          <div className="px-5 py-12 text-center font-serif text-[12px] text-ink/55">
            {t("inspector.selectObject")}
          </div>
        )}
      </div>
    </aside>
  );
}
