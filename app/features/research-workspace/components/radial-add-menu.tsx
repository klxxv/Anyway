"use client";

import {
  IconChartHistogram,
  IconCheck,
  IconFileText,
  IconFlask2,
  IconHandFinger,
  IconHelp,
  IconNote,
  IconPlus,
  IconUsersGroup,
  IconDatabase,
} from "@tabler/icons-react";
import type { ResearchNodeType } from "../../../lib/research-types";
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
  return (
    <div
      className="zen-pie-menu"
      style={{ left: menu.screenX, top: menu.screenY }}
      role="menu"
      aria-label="Quick add node"
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
      {quickNodeTypes.map(({ type, label }, index) => {
        const Icon = icons[type as keyof typeof icons] ?? IconNote;
        return (
          <button
            key={type}
            className={`zen-pie-item zen-pie-item-${index}`}
            onClick={() => onChoose(type)}
            role="menuitem"
          >
            <Icon size={18} stroke={1.35} />
            <span>{label}</span>
          </button>
        );
      })}
      <button className="zen-pie-center" onClick={onClose} aria-label="Close quick add">
        <IconPlus size={24} stroke={1.35} />
        <span>Add</span>
      </button>
      <div className="zen-pie-gesture">
        <IconHandFinger size={48} stroke={1.15} />
        <p className="zen-pie-hint">hold two fingers · flick · release</p>
      </div>
    </div>
  );
}
