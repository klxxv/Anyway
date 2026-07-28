/**
 * 将已提交的 MNIST 实验 artifact 映射成可审阅的研究图；不在前端训练模型。
 * Maps a checked-in MNIST experiment artifact to a reviewable research graph; the frontend never trains a model.
 */

import resultsJson from "../data/mnist-experiment-results.json";
import type {
  EvidenceRecord,
  GraphSuggestion,
  ProjectState,
  ResearchEdge,
  ResearchNode,
  ScenarioRecord,
} from "./research-types";

type MnistResult = {
  id: string;
  label: string;
  hypothesis: string;
  normalized: boolean;
  hiddenUnits: number;
  activation: string;
  accuracy: number;
  logLoss: number;
  iterations: number;
  durationSeconds: number;
  finalTrainingLoss: number;
  deltaAccuracy: number;
  evidenceOutcome: "baseline" | "supports" | "refutes";
};

type MnistArtifact = {
  task: string;
  dataset: {
    name: string;
    source: string;
    trainSamples: number;
    testSamples: number;
    inputShape: number[];
    classes: number;
  };
  environment: {
    runtime: string;
    device: string;
    randomState: number;
    gitCommit: string;
    repository: string;
  };
  results: MnistResult[];
};

const artifact = resultsJson as MnistArtifact;
const now = "2026-07-26T05:00:00.000Z";

const node = (
  id: string,
  type: ResearchNode["type"],
  title: string,
  body: string,
  tags: string[],
  data: Record<string, unknown> = {},
): ResearchNode => ({
  id,
  type,
  title,
  body,
  tags,
  status: "confirmed",
  evidenceIds: [],
  data,
  provenance: {
    origin: "import",
    actorId: "git-experiments@0.1.0",
    sourceRefs: [`${artifact.environment.repository}@${artifact.environment.gitCommit}`],
  },
  createdAt: now,
  updatedAt: now,
});

const nodes: ResearchNode[] = [
  node(
    "mnist-question",
    "question",
    "Which variables materially affect MNIST accuracy?",
    "Human-reviewed ablation map for a small CPU MLP trained on a stratified MNIST subset.",
    ["mnist", "ablation"],
    { role: "research-question" },
  ),
  node(
    "mnist-data",
    "variable",
    "MNIST pixels",
    `${artifact.dataset.trainSamples} train / ${artifact.dataset.testSamples} test samples; 28×28 grayscale input.`,
    ["input", "dataset"],
    { role: "input", shape: artifact.dataset.inputShape, classes: artifact.dataset.classes },
  ),
  node(
    "mnist-normalization",
    "variable",
    "Pixel normalization",
    "Scale uint8 pixels to [0, 1] before optimization.",
    ["preprocessing", "variable"],
    { role: "input-transform", baseline: true },
  ),
  node(
    "mnist-hidden",
    "variable",
    "Hidden width",
    "Capacity of the single hidden representation: baseline 64 units, ablation 16.",
    ["architecture", "variable"],
    { role: "parameter", baseline: 64, ablation: 16 },
  ),
  node(
    "mnist-activation",
    "variable",
    "Activation function",
    "Baseline ReLU compared with tanh while other controls remain fixed.",
    ["architecture", "variable"],
    { role: "parameter", baseline: "relu", ablation: "tanh" },
  ),
  node(
    "mnist-optimizer",
    "variable",
    "Adam optimization",
    "Fixed optimizer control: learning rate 0.001, batch size 128, random seed 42.",
    ["control", "optimizer"],
    { role: "control", learningRate: 0.001, batchSize: 128, seed: 42 },
  ),
  node(
    "mnist-representation",
    "variable",
    "Learned representation",
    "One-hidden-layer MLP representation produced from controlled inputs and variables.",
    ["latent", "model"],
    { role: "latent" },
  ),
  node(
    "mnist-accuracy",
    "metric",
    "Test accuracy",
    `Baseline accuracy ${(artifact.results[0].accuracy * 100).toFixed(2)}%. Higher is better.`,
    ["outcome", "metric"],
    { role: "output", baseline: artifact.results[0].accuracy },
  ),
  node(
    "mnist-conclusion",
    "result",
    "Normalization and width are effective variables",
    "Normalization has the strongest measured contribution; width is material; the claim that ReLU is uniquely required is refuted.",
    ["conclusion", "reviewed"],
    { role: "conclusion" },
  ),
];

const evidence: EvidenceRecord[] = artifact.results.map((result) => ({
  id: `evidence-${result.id}`,
  sourceType: "experiment",
  sourceId: result.id,
  title: `${result.label}: ${(result.accuracy * 100).toFixed(2)}% accuracy`,
  url: artifact.dataset.source,
  locator: {
    section: "Bundled MNIST run artifact",
    quote: `accuracy=${result.accuracy}; log_loss=${result.logLoss}; delta=${result.deltaAccuracy}`,
  },
  status: "verified",
  provenance: {
    origin: "python",
    actorId: artifact.environment.runtime,
    sourceRefs: [`commit:${artifact.environment.gitCommit}`, `seed:${artifact.environment.randomState}`],
  },
}));

for (const item of evidence) {
  const metricNode = nodes.find((candidate) => candidate.id === "mnist-accuracy");
  metricNode?.evidenceIds.push(item.id);
}

const edge = (
  id: string,
  source: string,
  target: string,
  type: ResearchEdge["type"],
  result: MnistResult,
  note: string,
): ResearchEdge => ({
  id,
  source,
  target,
  type,
  directed: true,
  polarity: result.evidenceOutcome === "refutes" ? "negative" : "positive",
  confidence: 0.94,
  conditions: [
    "same 6,000/1,500 stratified split",
    "same Adam optimizer",
    "same random seed 42",
    "same 12-iteration budget",
  ],
  evidenceIds: [`evidence-${result.id}`],
  note,
  experiment: {
    id: result.id,
    label: result.label,
    metric: "test_accuracy",
    baseline: artifact.results[0].accuracy,
    value: result.accuracy,
    delta: result.deltaAccuracy,
    outcome: result.evidenceOutcome,
    status: "completed",
    commit: artifact.environment.gitCommit,
    durationSeconds: result.durationSeconds,
  },
  provenance: {
    origin: "python",
    actorId: artifact.environment.runtime,
    sourceRefs: [`commit:${artifact.environment.gitCommit}`],
  },
});

const baseline = artifact.results[0];
const noNorm = artifact.results[1];
const narrow = artifact.results[2];
const tanh = artifact.results[3];

const edges: ResearchEdge[] = [
  edge(
    "mnist-exp-baseline-input",
    "mnist-data",
    "mnist-normalization",
    "supports",
    baseline,
    "Baseline data flow is fixed for all comparisons.",
  ),
  edge(
    "mnist-exp-no-normalization",
    "mnist-normalization",
    "mnist-representation",
    "supports",
    noNorm,
    "Removing normalization reduced accuracy by 5.53 percentage points.",
  ),
  edge(
    "mnist-exp-width",
    "mnist-hidden",
    "mnist-representation",
    "supports",
    narrow,
    "Reducing hidden width to 16 reduced accuracy by 2.20 percentage points.",
  ),
  edge(
    "mnist-exp-tanh",
    "mnist-activation",
    "mnist-representation",
    "contradicts",
    tanh,
    "Only a 1.13-point drop: this refutes the stronger claim that ReLU is uniquely necessary.",
  ),
  edge(
    "mnist-exp-control",
    "mnist-optimizer",
    "mnist-representation",
    "controls",
    baseline,
    "Optimization and seed are held constant across variants.",
  ),
  edge(
    "mnist-exp-readout",
    "mnist-representation",
    "mnist-accuracy",
    "measures",
    baseline,
    "The shared test set measures the learned representation.",
  ),
  edge(
    "mnist-exp-conclusion",
    "mnist-accuracy",
    "mnist-conclusion",
    "supports",
    noNorm,
    "Completed ablations support the reviewed conclusion.",
  ),
  edge(
    "mnist-question-link",
    "mnist-question",
    "mnist-data",
    "derived_from",
    baseline,
    "The study design operationalizes the research question.",
  ),
];

const scenarios: ScenarioRecord[] = [
  noNorm,
  narrow,
  tanh,
].map((result) => ({
  id: `scenario-${result.id}`,
  name: result.label,
  disabledNodeIds:
    result.id === "mnist-no-normalization"
      ? ["mnist-normalization"]
      : result.id === "mnist-bottleneck-16"
        ? ["mnist-hidden"]
        : ["mnist-activation"],
  disabledEdgeIds: [],
  nodeOverrides: {},
  edgeOverrides: {},
  parameters: {
    normalized: result.normalized,
    hiddenUnits: result.hiddenUnits,
    activation: result.activation,
    commit: artifact.environment.gitCommit,
  },
  hypothesis: result.hypothesis,
  expectedEffect: `Observed Δ accuracy ${(result.deltaAccuracy * 100).toFixed(2)} percentage points.`,
  createdAt: now,
}));

export function createMnistProject(): ProjectState {
  return {
    schemaVersion: 1,
    id: "mnist-git-ablation",
    title: "MNIST · Git-backed ablation study",
    discipline: "Neural networks",
    updatedAt: now,
    revision: 1,
    nodes,
    edges,
    evidence,
    placements: nodes.map((item, index) => ({
      id: `placement-${item.id}`,
      viewId: "view-main",
      nodeId: item.id,
      x: 80 + (index % 4) * 300,
      y: 80 + Math.floor(index / 4) * 160,
      width: 242,
      height: 122,
    })),
    scenarios,
    activity: [
      {
        id: "mnist-activity-import",
        label: `Git Experiments loaded ${artifact.environment.gitCommit}`,
        origin: "import",
        createdAt: now,
      },
      {
        id: "mnist-activity-train",
        label: "4 MNIST variants trained and compared",
        origin: "python",
        createdAt: now,
      },
    ],
  };
}

export const mnistRunSummary = artifact;

export const mnistSuggestions: GraphSuggestion[] = [
  {
    id: "mnist-suggestion-calibration",
    kind: "node",
    operation: "add",
    title: "Add calibration error as a secondary metric",
    description:
      "Accuracy alone may hide confidence shifts between the normalized and raw-pixel variants.",
    confidence: 0.82,
    rationale: "A second metric would distinguish classification errors from confidence drift.",
    evidenceLabel: "MNIST run artifact · metric gap · candidate",
    status: "proposed",
    node: {
      type: "metric",
      title: "Expected calibration error",
      body: "Secondary metric proposed for each reviewed MNIST variant.",
      tags: ["mnist", "calibration", "candidate"],
      status: "draft",
      evidenceIds: [],
      data: { role: "secondary-output" },
      provenance: {
        origin: "ai",
        modelId: "mnist-review-helper",
        promptVersion: "ablation-review-v0.1",
        sourceRefs: [`commit:${artifact.environment.gitCommit}`],
      },
    },
  },
  {
    id: "mnist-suggestion-weight-decay",
    kind: "node",
    operation: "add",
    title: "Represent weight decay as a control",
    description:
      "The MLP uses alpha=1e-4 in every run, but this fixed optimization variable is only present in the artifact.",
    confidence: 0.94,
    rationale: "A fair ablation should make every held-constant optimization parameter reviewable.",
    evidenceLabel: `Git commit ${artifact.environment.gitCommit} · training script`,
    status: "proposed",
    node: {
      type: "variable",
      title: "Weight decay",
      body: "L2 penalty alpha=1e-4 held constant across all four MNIST runs.",
      tags: ["control", "optimization"],
      status: "draft",
      evidenceIds: [],
      data: { role: "control", value: 0.0001 },
      provenance: {
        origin: "ai",
        modelId: "mnist-review-helper",
        promptVersion: "ablation-review-v0.1",
        sourceRefs: [`commit:${artifact.environment.gitCommit}`],
      },
    },
  },
  {
    id: "mnist-suggestion-tanh-edge",
    kind: "edge",
    operation: "update",
    title: "Review the tanh refutation threshold",
    description:
      "The current 1.13-point change is treated as refuting unique necessity; the materiality threshold should remain explicit.",
    confidence: 0.88,
    rationale: "The edge exists, but its decision rule requires human confirmation.",
    evidenceLabel: "Replace ReLU with tanh · completed run",
    status: "proposed",
    edge: {
      source: "mnist-activation",
      target: "mnist-representation",
      type: "contradicts",
      directed: true,
      polarity: "negative",
      confidence: 0.88,
      conditions: ["materiality threshold = 1.5 percentage points"],
      evidenceIds: ["evidence-mnist-tanh"],
      note: "Candidate update: preserve the explicit refutation decision rule.",
      experiment: {
        id: tanh.id,
        label: tanh.label,
        metric: "test_accuracy",
        baseline: baseline.accuracy,
        value: tanh.accuracy,
        delta: tanh.deltaAccuracy,
        outcome: "refutes",
        status: "completed",
        commit: artifact.environment.gitCommit,
        durationSeconds: tanh.durationSeconds,
      },
      provenance: {
        origin: "ai",
        modelId: "mnist-review-helper",
        promptVersion: "ablation-review-v0.1",
        sourceRefs: [`commit:${artifact.environment.gitCommit}`],
      },
    },
  },
];
