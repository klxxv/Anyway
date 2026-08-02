import type { ResearchNodeType } from "../../lib/research-types";

export const RADIAL_MENU_POSITIONS = [
  "north",
  "north-east",
  "east",
  "south-east",
  "south",
  "south-west",
  "west",
  "north-west",
] as const;

export type RadialMenuPosition = (typeof RADIAL_MENU_POSITIONS)[number];

export const RADIAL_MENU_POSITION_INDEX: Record<RadialMenuPosition, number> = {
  north: 0,
  "north-east": 1,
  east: 2,
  "south-east": 3,
  south: 4,
  "south-west": 5,
  west: 6,
  "north-west": 7,
};

export const RADIAL_NODE_ACTIONS = [
  "create:question",
  "create:concept",
  "create:variable",
  "create:method",
  "create:dataset",
  "create:evidence",
  "create:result",
  "create:note",
] as const;

export const RADIAL_CANVAS_ACTIONS = ["canvas:fit", "canvas:default-layout"] as const;
export const RADIAL_MENU_ACTIONS = [
  ...RADIAL_NODE_ACTIONS,
  ...RADIAL_CANVAS_ACTIONS,
] as const;

export type RadialMenuAction = (typeof RADIAL_MENU_ACTIONS)[number];

export type RadialMenuItem = {
  id: string;
  position: RadialMenuPosition;
  action: RadialMenuAction;
};

export type RadialMenuPreferences = {
  items: RadialMenuItem[];
};

export type CachedRadialMenuItem = {
  item: RadialMenuItem;
  sectorIndex: number;
  nodeType: ResearchNodeType | null;
};

export type RadialMenuCache = {
  items: CachedRadialMenuItem[];
  itemsBySector: Array<RadialMenuItem | null>;
};

export const defaultRadialMenuPreferences: RadialMenuPreferences = {
  items: RADIAL_MENU_POSITIONS.map((position, index) => ({
    id: `radial-${index + 1}`,
    position,
    action: RADIAL_NODE_ACTIONS[index],
  })),
};

const positionSet = new Set<string>(RADIAL_MENU_POSITIONS);
const actionSet = new Set<string>(RADIAL_MENU_ACTIONS);

/**
 * Restores at most eight unique directional slots and rejects unknown actions.
 * 恢复最多八个方向唯一的槽位，同时拒绝未知动作。
 */
export function normalizeRadialMenuPreferences(
  value: Partial<RadialMenuPreferences> | null | undefined,
): RadialMenuPreferences {
  if (!Array.isArray(value?.items)) {
    return { items: defaultRadialMenuPreferences.items.map((item) => ({ ...item })) };
  }

  const occupied = new Set<RadialMenuPosition>();
  const items = value.items
    .slice(0, RADIAL_MENU_POSITIONS.length)
    .flatMap((item, index): RadialMenuItem[] => {
      if (
        !item ||
        !positionSet.has(item.position) ||
        !actionSet.has(item.action) ||
        occupied.has(item.position)
      ) {
        return [];
      }
      occupied.add(item.position);
      return [{
        id: typeof item.id === "string" && item.id ? item.id : `radial-${index + 1}`,
        position: item.position,
        action: item.action,
      }];
    });

  return { items };
}

export function nodeTypeForRadialAction(
  action: RadialMenuAction,
): ResearchNodeType | null {
  if (!action.startsWith("create:")) return null;
  return action.slice("create:".length) as ResearchNodeType;
}

/**
 * Compiles persisted settings once into render metadata and O(1) gesture lookup.
 * 设置变化时一次性编译渲染元数据与 O(1) 手势查找表。
 */
export function compileRadialMenu(
  preferences: RadialMenuPreferences,
): RadialMenuCache {
  const itemsBySector = Array<RadialMenuItem | null>(RADIAL_MENU_POSITIONS.length).fill(null);
  const items = preferences.items.map((item) => {
    const sectorIndex = RADIAL_MENU_POSITION_INDEX[item.position];
    itemsBySector[sectorIndex] = item;
    return {
      item,
      sectorIndex,
      nodeType: nodeTypeForRadialAction(item.action),
    };
  });
  return { items, itemsBySector };
}

/**
 * Resolves normalized travel to a sector; the dead zone avoids accidental activation.
 * 将归一化位移解析为扇区；中心死区用于避免误触。
 */
export function radialSectorForNormalizedDisplacement(
  normalizedX: number,
  normalizedY: number,
): number | null {
  if (normalizedX * normalizedX + normalizedY * normalizedY < 0.018 ** 2) return null;

  const sector = Math.round(
    (Math.atan2(normalizedY, normalizedX) + Math.PI / 2) / (Math.PI / 4),
  );
  return ((sector % RADIAL_MENU_POSITIONS.length) + RADIAL_MENU_POSITIONS.length) %
    RADIAL_MENU_POSITIONS.length;
}

/**
 * Uses the compiled cache in the animation-frame hot path without scanning menu items.
 * 动画帧热路径直接查询预编译缓存，不再遍历菜单项。
 */
export function radialSelectionForNormalizedDisplacement(
  cache: RadialMenuCache,
  normalizedX: number,
  normalizedY: number,
): { sectorIndex: number; item: RadialMenuItem } | null {
  const sectorIndex = radialSectorForNormalizedDisplacement(normalizedX, normalizedY);
  if (sectorIndex === null) return null;
  const item = cache.itemsBySector[sectorIndex];
  return item ? { sectorIndex, item } : null;
}
