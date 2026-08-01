"use client";

import {
  IconChartHistogram,
  IconCheck,
  IconDatabase,
  IconFileText,
  IconFlask2,
  IconHelp,
  IconNote,
  IconPlus,
  IconUsersGroup,
} from "@tabler/icons-react";
import type { ResearchNodeType } from "../../../lib/research-types";
import type { MessageKey } from "../../../i18n/catalog";
import { useI18n } from "../../../i18n/provider";
import { quickNodeTypes } from "../hooks/use-workspace-project";
import type { PieMenuState } from "../workspace-types";

const icons = {
  question: IconHelp,
  concept: IconUsersGroup,
  variable: IconChartHistogram,
  method: IconFlask2,
  dataset: IconDatabase,
  evidence: IconFileText,
  result: IconCheck,
  note: IconNote,
} as const;

const labelKeys: Partial<Record<ResearchNodeType, MessageKey>> = {
  question: "node.question",
  concept: "node.group",
  variable: "node.variable",
  method: "node.method",
  dataset: "node.data",
  evidence: "node.evidence",
  result: "node.result",
  note: "node.note",
};

type RadialAddMenuProps = {
  menu: PieMenuState;
  onChoose: (type: ResearchNodeType) => void;
  onClose: () => void;
};

/**
 * Blender-inspired pie menu. Buttons remain regular accessible controls.
 * 类 Blender 饼菜单；每一项仍是可访问的标准按钮。
 */
export function RadialAddMenu({ menu, onChoose, onClose }: RadialAddMenuProps) {
  const { t } = useI18n();
  return (
    <div
      className="zen-pie-menu"
      style={{ left: menu.screenX, top: menu.screenY }}
      role="menu"
      aria-label={t("gesture.quickAdd")}
    >
      <div className="zen-pie-spokes" aria-hidden>
        {Array.from({ length: 8 }, (_, index) => (
          <span
            key={index}
            className="zen-pie-spoke"
            style={{ transform: `rotate(${index * 45 + 22.5}deg)` }}
          />
        ))}
      </div>
      {quickNodeTypes.map(({ type }, index) => {
        const Icon = icons[type as keyof typeof icons] ?? IconNote;
        const selected = menu.selectedType === type;
        return (
          <button
            key={type}
            className={`zen-pie-item zen-pie-item-${index} ${selected ? "is-active" : ""}`}
            onClick={() => onChoose(type)}
            role="menuitem"
            aria-current={selected ? "true" : undefined}
          >
            <Icon size={18} stroke={1.35} />
            <span>{t(labelKeys[type] ?? "node.note")}</span>
          </button>
        );
      })}
      <button className="zen-pie-center" onClick={onClose} aria-label={t("gesture.close")}>
        <IconPlus size={24} stroke={1.35} />
        <span aria-live="polite">
          {menu.gestureActive
            ? menu.selectedType
              ? t(labelKeys[menu.selectedType] ?? "node.note")
              : t("gesture.moveToChoose")
            : t("workspace.add")}
        </span>
      </button>
    </div>
  );
}
