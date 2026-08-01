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
  schemaVersion: 2,
  id: "project-urban-heat-islands",
  title: "城市树冠与热岛效应",
  discipline: "城市气候研究",
  updatedAt: fixtureTimestamp,
  revision: 1,
  nodes: [
    node(
      "question-tree",
      "question",
      "树冠覆盖如何影响城市地表温度？",
      "研究城市植被覆盖与夏季地表热环境之间的关系。",
      ["研究问题"],
      { shape: "circle" },
    ),
    node(
      "variable-temperature",
      "variable",
      "地表温度（°C）",
      "根据卫星热红外影像反演的白天地表温度。",
      ["因变量"],
      {
        valueType: "number",
        unit: "°C",
        instances: [
          { id: "temp-center-2023", label: "中心城区 · 2023 夏季均值", value: "34.2" },
          { id: "temp-riverside-2023", label: "滨水区 · 2023 夏季均值", value: "31.6" },
        ],
      },
    ),
    node(
      "variable-canopy",
      "variable",
      "树冠覆盖率（%）",
      "基于 10 米分辨率 NDVI 阈值估算。",
      ["自变量"],
      {
        valueType: "enum",
        enumValues: ["低", "中", "高"],
        instances: [
          { id: "canopy-north", label: "北部样区", value: "高" },
          { id: "canopy-center", label: "中心样区", value: "低" },
          { id: "canopy-south", label: "南部样区", value: "中" },
        ],
      },
    ),
    node(
      "paper-landsat",
      "evidence",
      "Landsat 8 地表温度（2020–2023）",
      "覆盖三个夏季观测窗口的栅格证据。",
      ["栅格数据"],
      { sourceKind: "raster" },
    ),
    node(
      "method-ndvi",
      "method",
      "基于 NDVI 的树冠分类",
      "根据归一化植被指数对树冠覆盖等级进行分类。",
      ["方法"],
    ),
    node(
      "result-canopy",
      "result",
      "高树冠覆盖区域的地表温度更低",
      "由两篇文献与遥感观测共同支持的阶段性结论。",
      ["结论摘要"],
    ),
    node(
      "variable-density",
      "variable",
      "建筑密度",
      "作为控制变量使用的建成区密度。",
      ["控制变量"],
      {
        valueType: "number",
        unit: "建筑覆盖率",
        shape: "circle",
        instances: [
          { id: "density-block-a", label: "街区 A", value: "0.72" },
          { id: "density-block-b", label: "街区 B", value: "0.48" },
        ],
      },
    ),
    node(
      "paper-zhang",
      "evidence",
      "张等（2022）",
      "报告树冠覆盖降温关系的同行评审论文。",
      ["论文"],
    ),
    node(
      "paper-nguyen",
      "evidence",
      "Nguyen 等（2021）",
      "在不同阈值条件下得到相冲突估计的论文。",
      ["论文", "存在争议"],
    ),
  ],
  edges: [
    edge("edge-canopy-temp", "variable-canopy", "variable-temperature", "causes", "影响"),
    edge("edge-question-canopy", "question-tree", "variable-canopy", "depends_on", "研究对象"),
    edge("edge-canopy-landsat", "variable-canopy", "paper-landsat", "measures", "测量依据"),
    edge("edge-canopy-ndvi", "variable-canopy", "method-ndvi", "derived_from", "派生自"),
    edge("edge-canopy-result", "variable-canopy", "result-canopy", "causes", "产生"),
    edge("edge-density-temp", "variable-density", "variable-temperature", "controls", "控制"),
    edge("edge-result-zhang", "result-canopy", "paper-zhang", "supports", "支持"),
    edge("edge-result-nguyen", "result-canopy", "paper-nguyen", "supports", "支持"),
    edge("edge-zhang-nguyen", "paper-zhang", "paper-nguyen", "contradicts", "结论冲突"),
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
      label: "打开城市树冠与热岛效应研究",
      origin: "human",
      createdAt: fixtureTimestamp,
    },
  ],
};
