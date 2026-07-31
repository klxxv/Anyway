import type {
  ProjectState,
  ResearchEdge,
  ResearchNode,
  ResearchNodeType,
} from "../../lib/research-types";

const fixtureTimestamp = "2026-07-31T08:00:00.000Z";
const human = { origin: "human" as const, actorId: "local-researcher" };

function node(
  id: string,
  type: ResearchNodeType,
  title: string,
  body: string,
  tags: string[],
  data: Record<string, unknown> = {},
): ResearchNode {
  return {
    id,
    type,
    title,
    body,
    tags,
    status: "confirmed",
    evidenceIds: [],
    data,
    provenance: human,
    createdAt: fixtureTimestamp,
    updatedAt: fixtureTimestamp,
  };
}

function edge(
  id: string,
  source: string,
  target: string,
  type: ResearchEdge["type"],
  label: string,
): ResearchEdge {
  return {
    id,
    source,
    target,
    type,
    directed: true,
    polarity: type === "contradicts" ? "negative" : "positive",
    confidence: 0.9,
    conditions: [],
    evidenceIds: [],
    note: label,
    provenance: human,
  };
}

const placements = [
  ["question-tree", 334, 14, 136, 136],
  ["variable-temperature", 0, 218, 166, 126],
  ["variable-canopy", 334, 238, 164, 116],
  ["paper-landsat", 733, 112, 158, 110],
  ["method-ndvi", 562, 335, 104, 124],
  ["result-canopy", 320, 480, 170, 132],
  ["variable-density", -3, 459, 142, 142],
  ["paper-zhang", 219, 719, 148, 114],
  ["paper-nguyen", 480, 719, 148, 114],
] as const;

/**
 * Screenshot-stable climate fixture used by the desktop visual QA flow.
 * 用于桌面视觉验收的稳定气候研究示例。
 */
export const zenWorkspaceFixture: ProjectState = {
  schemaVersion: 1,
  id: "project-urban-heat-islands",
  title: "Urban Heat Islands",
  discipline: "Climate Study",
  updatedAt: fixtureTimestamp,
  revision: 1,
  nodes: [
    node(
      "question-tree",
      "question",
      "How does tree canopy cover affect surface temperature?",
      "Frame the relationship between urban vegetation and observed land-surface heat.",
      ["research question"],
      { shape: "circle" },
    ),
    node(
      "variable-temperature",
      "variable",
      "Land Surface Temperature (°C)",
      "Daytime land-surface temperature measured from satellite imagery.",
      ["dependent"],
      { valueType: "number", unit: "°C" },
    ),
    node(
      "variable-canopy",
      "variable",
      "Tree Canopy Cover (%)",
      "Estimated from NDVI thresholding at 10 m resolution.",
      ["independent"],
      { valueType: "enum", enumValues: ["low", "medium", "high"] },
    ),
    node(
      "paper-landsat",
      "evidence",
      "Landsat 8 LST (2020–2023)",
      "Raster evidence covering three summer observation windows.",
      ["raster"],
      { sourceKind: "raster" },
    ),
    node(
      "method-ndvi",
      "method",
      "NDVI-based Canopy Classification",
      "Classifies canopy coverage from normalized vegetation index values.",
      ["method"],
    ),
    node(
      "result-canopy",
      "result",
      "Higher canopy associated with lower LST",
      "A concise result summary supported by the two cited articles.",
      ["summary"],
    ),
    node(
      "variable-density",
      "variable",
      "Building Density",
      "Built-area density used as a control variable.",
      ["control"],
      { valueType: "number", shape: "circle" },
    ),
    node(
      "paper-zhang",
      "evidence",
      "Zhang et al. (2022)",
      "Peer-reviewed article reporting a canopy cooling association.",
      ["article"],
    ),
    node(
      "paper-nguyen",
      "evidence",
      "Nguyen et al. (2021)",
      "Article with a conflicting estimate under a different threshold.",
      ["article", "disputed"],
    ),
  ],
  edges: [
    edge("edge-canopy-temp", "variable-canopy", "variable-temperature", "causes", "influences"),
    edge("edge-question-canopy", "question-tree", "variable-canopy", "depends_on", "addresses"),
    edge("edge-canopy-landsat", "variable-canopy", "paper-landsat", "measures", "measured by"),
    edge("edge-canopy-ndvi", "variable-canopy", "method-ndvi", "derived_from", "derived from"),
    edge("edge-canopy-result", "variable-canopy", "result-canopy", "causes", "produces"),
    edge("edge-density-temp", "variable-density", "variable-temperature", "controls", "controlled by"),
    edge("edge-result-zhang", "result-canopy", "paper-zhang", "supports", "supported by"),
    edge("edge-result-nguyen", "result-canopy", "paper-nguyen", "supports", "supported by"),
    edge("edge-zhang-nguyen", "paper-zhang", "paper-nguyen", "contradicts", "contradicts"),
  ],
  evidence: [],
  placements: placements.map(([nodeId, x, y, width, height]) => ({
    id: `placement-${nodeId}`,
    viewId: "view-main",
    nodeId,
    x,
    y,
    width,
    height,
  })),
  scenarios: [],
  navigation: {
    recentNodeIds: ["variable-canopy", "result-canopy"],
    pinnedNodeIds: ["variable-canopy"],
  },
  activity: [
    {
      id: "activity-fixture",
      label: "Opened Urban Heat Islands study",
      origin: "human",
      createdAt: fixtureTimestamp,
    },
  ],
};
