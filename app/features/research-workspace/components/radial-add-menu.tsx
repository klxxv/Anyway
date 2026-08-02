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
import {
  forwardRef,
  memo,
  useImperativeHandle,
  useState,
} from "react";
import {
  type RadialMenuAction,
  type RadialMenuCache,
  type RadialMenuItem,
} from "../workspace-radial-menu";
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
  cache: RadialMenuCache;
  onChoose: (item: RadialMenuItem) => void;
  onClose: () => void;
};

export type RadialAddMenuHandle = {
  updateGesture: (sector: number | null, active: boolean) => void;
};

function actionLabelKey(
  action: RadialMenuAction,
  nodeType: ResearchNodeType | null,
): MessageKey {
  if (nodeType) return labelKeys[nodeType] ?? "node.note";
  return action === "canvas:fit" ? "contextMenu.fitView" : "contextMenu.applyLayout";
}

function selectionArcPath(index: number) {
  const point = (angle: number) => {
    const radians = ((angle - 90) * Math.PI) / 180;
    return {
      x: 138 + 136 * Math.cos(radians),
      y: 138 + 136 * Math.sin(radians),
    };
  };
  const start = point(index * 45 - 19.5);
  const end = point(index * 45 + 19.5);
  return `M ${start.x} ${start.y} A 136 136 0 0 1 ${end.x} ${end.y}`;
}

const selectionArcPaths = Array.from({ length: 8 }, (_, index) =>
  selectionArcPath(index),
);

/**
 * Blender-inspired pie menu. Buttons remain regular accessible controls.
 * 类 Blender 饼菜单；每一项仍是可访问的标准按钮。
 */
const RadialAddMenuComponent = forwardRef<RadialAddMenuHandle, RadialAddMenuProps>(
function RadialAddMenu({ menu, cache, onChoose, onClose }, ref) {
  const { t } = useI18n();
  const [activeSector, setActiveSector] = useState<number | null>(null);
  const [gestureActive, setGestureActive] = useState(Boolean(menu.gestureActive));
  useImperativeHandle(ref, () => ({
    updateGesture(sector, active) {
      setActiveSector((current) => (current === sector ? current : sector));
      setGestureActive((current) => (current === active ? current : active));
    },
  }), []);
  return (
    <div
      className={`zen-pie-menu${gestureActive ? " is-gesture-active" : ""}`}
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
      {activeSector !== null && (
        <svg className="zen-pie-selection-arc" viewBox="0 0 276 276" aria-hidden>
          <path d={selectionArcPaths[activeSector]} />
        </svg>
      )}
      {cache.items.map(({ item, nodeType, sectorIndex }) => {
        const Icon = nodeType
          ? icons[nodeType as keyof typeof icons] ?? IconNote
          : item.action === "canvas:fit"
            ? IconCheck
            : IconChartHistogram;
        return (
          <button
            key={item.id}
            className={`zen-pie-item zen-pie-item-${sectorIndex}${
              activeSector === sectorIndex ? " is-active" : ""
            }`}
            onClick={() => onChoose(item)}
            role="menuitem"
            aria-current={activeSector === sectorIndex ? "true" : undefined}
          >
            <Icon size={18} stroke={1.35} />
            <span>{t(actionLabelKey(item.action, nodeType))}</span>
          </button>
        );
      })}
      <button className="zen-pie-center" onClick={onClose} aria-label={t("gesture.close")}>
        <IconPlus size={24} stroke={1.35} />
        <span>{t("workspace.add")}</span>
      </button>
    </div>
  );
});

RadialAddMenuComponent.displayName = "RadialAddMenu";
export const RadialAddMenu = memo(RadialAddMenuComponent);
