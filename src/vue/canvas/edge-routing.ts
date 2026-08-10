import type { ProjectState, ResearchEdge } from "../../../app/lib/research-types";

type Side = "left" | "right" | "top" | "bottom";

export type EdgeRoute = {
  sourceHandle: string;
  targetHandle: string;
  labelOffsetX: number;
  labelOffsetY: number;
};

type Box = { x: number; y: number; width: number; height: number };

function center(box: Box) {
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

function opposite(side: Side): Side {
  if (side === "left") return "right";
  if (side === "right") return "left";
  if (side === "top") return "bottom";
  return "top";
}

function routedHandle(side: Side, secondaryDelta: number, secondarySize: number) {
  if (Math.abs(secondaryDelta) < secondarySize * 0.18) return side;
  if (side === "left" || side === "right") {
    return `${side}-${secondaryDelta < 0 ? "top" : "bottom"}`;
  }
  return `${side}-${secondaryDelta < 0 ? "left" : "right"}`;
}

function distributedHandle(side: Side, index: number, count: number) {
  if (count <= 1) return null;
  const slot = count === 2
    ? index === 0
      ? -1
      : 1
    : index === 0
      ? -1
      : index === count - 1
        ? 1
        : 0;
  if (slot === 0) return side;
  if (side === "left" || side === "right") {
    return `${side}-${slot < 0 ? "top" : "bottom"}`;
  }
  return `${side}-${slot < 0 ? "left" : "right"}`;
}

/**
 * Framework-neutral routing boundary. It reads persisted placements and emits
 * only handle ids/label offsets consumed by the Vue Flow adapter.
 */
export function computeEdgeRoutes(project: ProjectState): Record<string, EdgeRoute> {
  const boxes = new Map<string, Box>(
    project.placements.map((placement) => [
      placement.nodeId,
      {
        x: placement.x,
        y: placement.y,
        width: placement.width ?? 164,
        height: placement.height ?? 116,
      },
    ]),
  );
  const preliminary = new Map<
    string,
    {
      edge: ResearchEdge;
      sourceSide: Side;
      targetSide: Side;
      sourceHandle: string;
      targetHandle: string;
    }
  >();

  project.edges.forEach((edge) => {
    const sourceBox = boxes.get(edge.source);
    const targetBox = boxes.get(edge.target);
    if (!sourceBox || !targetBox) return;
    const source = center(sourceBox);
    const target = center(targetBox);
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const horizontal = Math.abs(dx) >= Math.abs(dy);
    const sourceSide: Side = horizontal
      ? dx >= 0
        ? "right"
        : "left"
      : dy >= 0
        ? "bottom"
        : "top";
    const targetSide = opposite(sourceSide);
    preliminary.set(edge.id, {
      edge,
      sourceSide,
      targetSide,
      sourceHandle: routedHandle(
        sourceSide,
        horizontal ? dy : dx,
        horizontal ? sourceBox.height : sourceBox.width,
      ),
      targetHandle: routedHandle(
        targetSide,
        horizontal ? -dy : -dx,
        horizontal ? targetBox.height : targetBox.width,
      ),
    });
  });

  const lanes = new Map<string, string[]>();
  preliminary.forEach((route, edgeId) => {
    const key = `${route.edge.source}:${route.sourceSide}`;
    lanes.set(key, [...(lanes.get(key) ?? []), edgeId]);
  });
  lanes.forEach((edgeIds) => {
    edgeIds.sort((leftId, rightId) => {
      const left = preliminary.get(leftId)!;
      const right = preliminary.get(rightId)!;
      const leftTarget = boxes.get(left.edge.target);
      const rightTarget = boxes.get(right.edge.target);
      if (!leftTarget || !rightTarget) return leftId.localeCompare(rightId);
      const leftCenter = center(leftTarget);
      const rightCenter = center(rightTarget);
      const verticalSide = left.sourceSide === "top" || left.sourceSide === "bottom";
      return (
        (verticalSide ? leftCenter.x - rightCenter.x : leftCenter.y - rightCenter.y) ||
        leftId.localeCompare(rightId)
      );
    });
  });

  return Object.fromEntries(
    [...preliminary.entries()].map(([edgeId, route]) => {
      const siblings = lanes.get(`${route.edge.source}:${route.sourceSide}`) ?? [edgeId];
      const lane = siblings.indexOf(edgeId) - (siblings.length - 1) / 2;
      const offset = lane * 14;
      const horizontal = route.sourceSide === "left" || route.sourceSide === "right";
      const distributedSource = distributedHandle(
        route.sourceSide,
        siblings.indexOf(edgeId),
        siblings.length,
      );
      return [
        edgeId,
        {
          sourceHandle: distributedSource ?? route.sourceHandle,
          targetHandle: route.targetHandle,
          labelOffsetX: horizontal ? 0 : offset,
          labelOffsetY: horizontal ? offset : 0,
        },
      ];
    }),
  );
}
