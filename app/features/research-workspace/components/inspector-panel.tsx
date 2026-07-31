"use client";

import {
  IconDots,
  IconGripVertical,
  IconPin,
  IconPlus,
  IconTrash,
  IconX,
} from "@tabler/icons-react";
import { useState } from "react";
import type { ResearchNode } from "../../../lib/research-types";
import type { InspectorUpdate, VariableValueType } from "../workspace-types";

type InspectorPanelProps = {
  node: ResearchNode | null;
  onUpdate: (nodeId: string, update: InspectorUpdate) => void;
  onDelete: (nodeId: string) => void;
  onClose: () => void;
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
  const valueType = valueTypeOf(node);
  const [actionsOpen, setActionsOpen] = useState(false);
  const enumValues = Array.isArray(node.data.enumValues)
    ? node.data.enumValues.filter((value): value is string => typeof value === "string")
    : ["low", "medium", "high"];
  const updateData = (data: Record<string, unknown>) =>
    onUpdate(node.id, { data: { ...node.data, ...data } });

  return (
    <section className="border-b border-ink/20 px-4 pb-5 pt-4">
      <div className="mb-2 flex items-start gap-2">
        <input
          className="min-w-0 flex-1 border-0 bg-transparent p-0 font-serif text-[14px] leading-tight text-ink outline-none"
          value={node.title}
          onChange={(event) => onUpdate(node.id, { title: event.target.value })}
          aria-label="Node title"
        />
        <button className="icon-quiet" aria-label="Pin node">
          <IconPin size={17} stroke={1.35} />
        </button>
        <button className="icon-quiet" onClick={onClose} aria-label="Close inspector">
          <IconX size={18} stroke={1.35} />
        </button>
      </div>

      <div className="mb-4 flex items-center justify-between">
        <span className="rounded-[5px] border border-ink/25 bg-paper px-2 py-1 font-serif text-[10px] text-olive">
          {node.type} · {valueType}
        </span>
        <div className="relative">
          <button
            className="icon-quiet"
            aria-label="More node actions"
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
                Delete node
              </button>
            </div>
          )}
        </div>
      </div>

      {node.type === "variable" && (
        <>
          <label className="inspector-row">
            <span>Type</span>
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
              <span className="font-serif text-[12px] text-ink">Values</span>
              {valueType === "enum" && (
                <button
                  className="inline-flex items-center gap-1 font-serif text-[11px] text-ink/80 hover:text-blue"
                  onClick={() => updateData({ enumValues: [...enumValues, "new value"] })}
                >
                  <IconPlus size={15} stroke={1.35} />
                  Add value
                </button>
              )}
            </div>
            <div className="space-y-2">
              {(valueType === "enum"
                ? enumValues
                : valueType === "bool"
                  ? ["true", "false"]
                  : [typeof node.data.unit === "string" ? node.data.unit : valueType]
              ).map((value, index) => (
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
                      aria-label={`Remove ${value}`}
                    >
                      <IconX size={14} stroke={1.3} />
                    </button>
                  )}
                </div>
              ))}
            </div>
          </div>
        </>
      )}

      <label className="mt-4 block">
        <span className="mb-2 block font-serif text-[12px]">Notes</span>
        <textarea
          className="min-h-14 w-full resize-none border-0 bg-transparent p-0 font-serif text-[11px] leading-[1.4] text-ink outline-none"
          value={node.body}
          onChange={(event) => onUpdate(node.id, { body: event.target.value })}
        />
      </label>

    </section>
  );
}

function ObservedFactCard() {
  return (
    <section className="px-4 pb-5 pt-4">
      <div className="mb-2 flex items-start gap-2">
        <h3 className="flex-1 font-serif text-[16px] leading-tight">Observed Rain Yesterday</h3>
        <button className="icon-quiet" aria-label="Pin observed fact">
          <IconPin size={17} stroke={1.35} />
        </button>
        <button className="icon-quiet" aria-label="Hide observed fact">
          <IconX size={18} stroke={1.35} />
        </button>
      </div>
      <div className="mb-4 flex items-center justify-between">
        <span className="rounded-[5px] border border-ink/25 px-2 py-1 font-serif text-[10px] text-olive">
          bool · observed fact
        </span>
        <IconDots size={18} stroke={1.4} />
      </div>
      <div className="inspector-row">
        <span>Type</span>
        <span className="font-serif text-[11px]">bool⌄</span>
      </div>
      <div className="mt-4 space-y-2">
        {["true", "false"].map((value) => (
          <div
            key={value}
            className="flex h-9 items-center gap-2 rounded-[4px] border border-ink/25 px-2"
          >
            <IconGripVertical size={14} stroke={1.2} className="text-ink/45" />
            <span className="flex-1 font-serif text-[11px]">{value}</span>
            <IconX size={14} stroke={1.3} />
          </div>
        ))}
      </div>
      <div className="mt-4 border-t border-ink/15 pt-3">
        <p className="mb-2 font-serif text-[12px]">Notes</p>
        <p className="font-serif text-[11px] leading-[1.45]">
          Observed at 08:00 local time.
          <br />
          Source: Weather station log.
        </p>
      </div>
    </section>
  );
}

/**
 * Quiet properties inspector; one editable selection plus a pinned observed fact.
 * 克制的属性检查器：一个可编辑选中项与一个置顶观测事实。
 */
export function InspectorPanel({ node, onUpdate, onDelete, onClose }: InspectorPanelProps) {
  return (
    <aside className="h-full overflow-y-auto border-l border-ink/15 bg-canvas px-5 pb-4 pt-7 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
      <div className="overflow-hidden rounded-[7px] border border-ink/30 bg-paper shadow-[0_4px_12px_rgba(28,31,35,0.06)]">
        {node ? (
          <InspectorCard
            node={node}
            onUpdate={onUpdate}
            onDelete={onDelete}
            onClose={onClose}
          />
        ) : (
          <div className="px-5 py-12 text-center font-serif text-[12px] text-ink/55">
            Select a node to inspect its research properties.
          </div>
        )}
        <ObservedFactCard />
      </div>
    </aside>
  );
}
