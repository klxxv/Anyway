import type {
  EdgeStyleManifest,
  PluginManifest,
  ThemeManifest,
} from "../lib/research-types";
import {
  AGENT_CAPABILITIES,
  AGENT_PERMISSIONS,
} from "./agent-contracts";

export const builtInPluginCatalog: PluginManifest[] = [
  {
    id: "git-experiments",
    name: "Git Experiments",
    version: "0.1.0",
    category: "connector",
    description:
      "Loads a versioned repository snapshot and binds commits to experiment evidence.",
    status: "installed",
    permissions: ["Read bundled repository metadata", "Read experiment artifacts"],
    capabilities: ["git.commit.read", "experiment.import", "provenance.bind"],
    publisher: "Research Canvas",
  },
  {
    id: "python-connector",
    name: "Python Connector",
    version: "0.1.0-reserved",
    category: "connector",
    description: "Reserved adapter for reviewed run manifests and structured RunResult artifacts.",
    status: "reserved",
    permissions: ["Local process access (future opt-in)"],
    capabilities: ["run.manifest", "run.result"],
    publisher: "Research Canvas",
  },
  {
    id: "pdf-canvas-agent",
    name: "PDF Canvas Agent",
    version: "0.1.0",
    category: "agent",
    description:
      "从 PDF 论文中提取 DocumentMap → 语义结构 → 审阅门控的 GraphPatch。Agent 不持有 API Key、文件句柄或网络权限；宿主管理一切，Agent 输出仅可进入 reviewRequired GraphPatch。",
    status: "installed",
    permissions: [...AGENT_PERMISSIONS],
    capabilities: [...AGENT_CAPABILITIES],
    publisher: "Research Canvas",
  },
  {
    id: "graph-audit",
    name: "Graph Audit",
    version: "0.1.0",
    category: "analysis",
    description: "Checks cycles, unsupported claims, broken evidence paths, and alternate routes.",
    status: "available",
    permissions: ["Read current project"],
    capabilities: ["graph.validate", "chain.score"],
    publisher: "Research Canvas Labs",
  },
];

export const builtInThemeCatalog: ThemeManifest[] = [
  {
    id: "research-light",
    name: "Research Light",
    publisher: "Research Canvas",
    source: "builtin",
    colors: {
      app: "#eef1f5",
      panel: "#ffffff",
      canvas: "#f8f9fb",
      text: "#172033",
      muted: "#697386",
      accent: "#6750d8",
      border: "#dfe3e9",
    },
  },
  {
    id: "midnight-lab",
    name: "Midnight Lab",
    publisher: "Community Themes",
    source: "builtin",
    colors: {
      app: "#11151d",
      panel: "#171d28",
      canvas: "#0f141c",
      text: "#e7ebf3",
      muted: "#99a4b7",
      accent: "#8b7cff",
      border: "#2c3442",
    },
  },
  {
    id: "paper-sepia",
    name: "Paper Sepia",
    publisher: "Community Themes",
    source: "builtin",
    colors: {
      app: "#eee9df",
      panel: "#faf6ed",
      canvas: "#f6f0e4",
      text: "#312d27",
      muted: "#766f62",
      accent: "#9b5f32",
      border: "#d8cebd",
    },
  },
];

export const builtInEdgeStyleCatalog: EdgeStyleManifest[] = [
  {
    id: "research-bezier",
    name: "Research Bezier",
    publisher: "Research Canvas",
    source: "builtin",
    description: "Quiet curved connectors for freeform human-led canvases.",
    routing: "bezier",
    stroke: {
      color: "#98a2b3",
      width: 1.5,
      selectedWidth: 2.6,
      opacity: 0.88,
    },
    relations: {
      supports: { color: "#25836f", width: 1.8 },
      contradicts: { color: "#c14457", width: 1.8, dash: [7, 4] },
      controls: { color: "#7d8796", dash: [3, 4] },
      measures: { color: "#6d5bc1" },
    },
    marker: { type: "closed-arrow", size: 15 },
  },
  {
    id: "research-orthogonal",
    name: "Orthogonal Grid",
    publisher: "Research Canvas",
    source: "builtin",
    description: "Strict 90° routing for dependency trees and experiment maps.",
    routing: "orthogonal",
    stroke: {
      color: "#7f8a9b",
      width: 1.6,
      selectedWidth: 2.8,
      opacity: 0.92,
      cornerRadius: 12,
      offset: 24,
    },
    relations: {
      supports: { color: "#16866e", width: 1.9 },
      contradicts: { color: "#cf4056", width: 2, dash: [8, 4] },
      depends_on: { color: "#5271c7" },
      controls: { color: "#8993a3", dash: [3, 4] },
      measures: { color: "#755fc7" },
    },
    marker: { type: "closed-arrow", size: 15 },
  },
  {
    id: "research-straight",
    name: "Direct Signal",
    publisher: "Research Canvas",
    source: "builtin",
    description: "Straight, low-noise connectors for sparse influence graphs.",
    routing: "straight",
    stroke: {
      color: "#8b95a5",
      width: 1.45,
      selectedWidth: 2.5,
      opacity: 0.86,
    },
    relations: {
      supports: { color: "#248672" },
      contradicts: { color: "#c34b5e", dash: [6, 4] },
    },
    marker: { type: "arrow", size: 14 },
  },
];
