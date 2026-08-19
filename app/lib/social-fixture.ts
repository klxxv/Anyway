/**
 * 社会科学验收 fixture：覆盖中介、控制、循环与场景覆盖等图行为。
 * Social-science acceptance fixture covering mediation, controls, cycles, and scenario overlays.
 */

import type {
  EvidenceRecord,
  ProjectState,
  ResearchEdge,
  ResearchNode,
} from "./research-types";

const now = "2026-07-27T00:00:00.000Z";
const human = { origin: "human" as const, actorId: "social-science-fixture" };

const definitions: Array<[string, ResearchNode["type"], string, string]> = [
  ["soc-q", "question", "How does algorithmic exposure shape polarization?", "Trace exposure, mediation, controls, and measurement choices."],
  ["soc-exposure", "variable", "Algorithmic exposure", "Ranked political content shown per session."],
  ["soc-homophily", "variable", "Information homophily", "Similarity of encountered political viewpoints."],
  ["soc-ideology", "variable", "Initial ideology", "Baseline ideological position."],
  ["soc-age", "variable", "Age", "Respondent age control."],
  ["soc-region", "variable", "Region", "Geographic context control."],
  ["soc-education", "variable", "Education", "Educational attainment control."],
  ["soc-income", "variable", "Income", "Household income control."],
  ["soc-trust", "variable", "Social trust", "Generalized trust at each panel wave."],
  ["soc-participation", "variable", "Civic participation", "Participation in civic and political organizations."],
  ["soc-diversity", "variable", "Network diversity", "Cross-group tie diversity."],
  ["soc-diet", "variable", "Media diet", "Relative mix of political information sources."],
  ["soc-polarization", "variable", "Political polarization", "Latent outcome construct."],
  ["soc-h1", "hypothesis", "Exposure increases homophily", "Ranked exposure increases viewpoint similarity."],
  ["soc-h2", "hypothesis", "Homophily mediates polarization", "Homophily carries part of the exposure effect."],
  ["soc-h3", "hypothesis", "Network diversity moderates the pathway", "Diverse networks weaken the homophily pathway."],
  ["soc-method-panel", "method", "Three-wave panel model", "Lagged panel model with reviewed controls."],
  ["soc-method-match", "method", "Matched exposure comparison", "Propensity-matched exposure groups."],
  ["soc-metric", "metric", "Polarization index", "Validated multi-item outcome score."],
  ["soc-result", "result", "Conditional mediation estimate", "Reviewed estimate with sensitivity analysis."],
];

const evidence: EvidenceRecord[] = Array.from({ length: 8 }, (_, index) => ({
  id: `soc-ev-${index + 1}`,
  sourceType: "paper",
  sourceId: `soc-paper-${index + 1}`,
  title: `Social evidence source ${index + 1}`,
  authors: `Research team ${index + 1}`,
  year: 2018 + index,
  doi: `10.0000/social.${index + 1}`,
  locator: {
    page: index + 2,
    section: index < 4 ? "Measurement" : "Results",
    quote: `Fixture excerpt ${index + 1} anchors one reviewed social-science relation.`,
    startOffset: index * 100,
    endOffset: index * 100 + 72,
  },
  status: index === 7 ? "disputed" : "confirmed",
  provenance: human,
}));

const nodes: ResearchNode[] = definitions.map(([id, type, title, body], index) => ({
  id,
  type,
  title,
  body,
  tags: [type, index < 13 ? "variable-map" : "research-logic"],
  status: "confirmed",
  evidenceIds: index < 8 ? [`soc-ev-${index + 1}`] : [],
  data: { analysisLevel: type === "variable" ? "individual" : "study" },
  provenance: human,
  createdAt: now,
  updatedAt: now,
}));

const relationDefinitions: Array<
  [string, string, string, ResearchEdge["type"], number]
> = [
  ["soc-e1", "soc-q", "soc-exposure", "T", 0],
  ["soc-e2", "soc-exposure", "soc-homophily", "K", 1],
  ["soc-e3", "soc-homophily", "soc-polarization", "K", 2],
  ["soc-e4", "soc-exposure", "soc-polarization", "K", 3],
  ["soc-e5", "soc-diversity", "soc-homophily", "K", 4],
  ["soc-e6", "soc-diet", "soc-exposure", "K", 5],
  ["soc-e7", "soc-ideology", "soc-exposure", "K", 6],
  ["soc-e8", "soc-region", "soc-exposure", "K", 7],
  ["soc-e9", "soc-age", "soc-method-panel", "K", 0],
  ["soc-e10", "soc-education", "soc-method-panel", "K", 1],
  ["soc-e11", "soc-income", "soc-method-panel", "K", 2],
  ["soc-e12", "soc-h1", "soc-method-match", "T", 3],
  ["soc-e13", "soc-h2", "soc-method-panel", "T", 4],
  ["soc-e14", "soc-h3", "soc-method-panel", "T", 5],
  ["soc-e15", "soc-method-match", "soc-result", "T", 6],
  ["soc-e16", "soc-method-panel", "soc-result", "T", 7],
  ["soc-e17", "soc-result", "soc-metric", "K", 0],
  ["soc-e18", "soc-metric", "soc-polarization", "T", 1],
  ["soc-e19", "soc-trust", "soc-participation", "K", 2],
  ["soc-e20", "soc-participation", "soc-trust", "K", 3],
  ["soc-e21", "soc-diversity", "soc-trust", "K", 4],
  ["soc-e22", "soc-diet", "soc-homophily", "K", 5],
  ["soc-e23", "soc-ideology", "soc-polarization", "K", 6],
  ["soc-e24", "soc-homophily", "soc-result", "K", 7],
  ["soc-e25", "soc-diversity", "soc-result", "K", 0],
];

const edges: ResearchEdge[] = relationDefinitions.map(
  ([id, source, target, type, evidenceIndex]) => ({
    id,
    source,
    target,
    type,
    directed: true,
    polarity: "positive",
    confidence: 0.72 + (evidenceIndex % 4) * 0.06,
    conditions: ["three-wave panel", "reviewed measurement model"],
    evidenceIds: [`soc-ev-${evidenceIndex + 1}`],
    provenance: human,
  }),
);

export function createSocialScienceProject(): ProjectState {
  return {
    schemaVersion: 1,
    id: "social-science-acceptance",
    title: "Social science · polarization evidence map",
    discipline: "Computational social science",
    updatedAt: now,
    revision: 1,
    nodes,
    edges,
    evidence,
    placements: nodes.map((node, index) => ({
      id: `placement-${node.id}`,
      viewId: "view-main",
      nodeId: node.id,
      x: 90 + (index % 5) * 270,
      y: 90 + Math.floor(index / 5) * 155,
      width: 230,
      height: 116,
    })),
    scenarios: [
      {
        id: "soc-without-homophily",
        name: "Remove homophily mediator",
        disabledNodeIds: ["soc-homophily"],
        disabledEdgeIds: [],
        nodeOverrides: {},
        edgeOverrides: {},
        parameters: { estimator: "lagged-panel" },
        hypothesis: "The indirect exposure path disappears.",
        expectedEffect: "Reduced reachable support for polarization.",
        createdAt: now,
      },
      {
        id: "soc-alt-measure",
        name: "Alternative polarization measurement",
        disabledNodeIds: [],
        disabledEdgeIds: [],
        nodeOverrides: {
          "soc-polarization": {
            data: { operationalization: "cross-group network distance" },
          },
        },
        edgeOverrides: {},
        parameters: { measurement: "network-distance" },
        hypothesis: "The conclusion is stable to a second operationalization.",
        expectedEffect: "Same sign with wider uncertainty.",
        createdAt: now,
      },
    ],
    navigation: { recentNodeIds: ["soc-polarization"], pinnedNodeIds: ["soc-q"] },
    activity: [
      {
        id: "soc-activity",
        label: "Loaded 20-node social-science acceptance fixture",
        origin: "import",
        createdAt: now,
      },
    ],
  };
}
