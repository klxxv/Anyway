/**
 * Document-scoped Canvas Diff.
 *
 * This module is deliberately independent from the PDF agent runtime. It
 * accepts normalized document snapshots and returns a serializable contract
 * that can be consumed by Vue or an Agent review surface. Local graph ids are
 * never compared outside their document scope.
 */

import type {
  EvidenceRecord,
  ProjectState,
  Provenance,
  ResearchEdge,
  ResearchNode,
} from "../lib/research-types";
import { EDGE_TYPES, NODE_TYPES } from "../lib/research-types";
import { canonicalizeDiffValue } from "../lib/graph/canvas-diff";
import type { PluginGraphPatch } from "../plugins/contracts";

export const CANVAS_DIFF_API_VERSION =
  "researchcanvas.dev/canvas-diff/v1alpha1" as const;

export type CanvasDiffEntityKind = "node" | "edge" | "evidence";
export type CanvasDiffChangeState = "added" | "removed" | "modified";
export type CanvasDiffDocumentState = CanvasDiffChangeState | "unchanged";

/** Provenance carried with every document result and every document group. */
export interface CanvasDiffDocumentProvenance extends Provenance {
  documentId: string;
  fileName?: string;
  sourcePath?: string;
}

export interface CanvasDiffDocumentInput {
  /** Stable identity of the source paper/file, not the local project id. */
  documentId: string;
  project: ProjectState;
  provenance?: Partial<CanvasDiffDocumentProvenance>;
}

export interface CanvasDiffGroupInput {
  documents: readonly CanvasDiffDocumentInput[];
  groupId?: string;
  label?: string;
}

export type CanvasDiffDocumentCollection =
  | readonly CanvasDiffDocumentInput[]
  | CanvasDiffGroupInput;

export interface CanvasDiffBatchRequest {
  baseline: CanvasDiffDocumentCollection;
  comparison: CanvasDiffDocumentCollection;
}

export interface CanvasDiffGroupDescriptor {
  groupId: string;
  label: string;
  documentIds: string[];
}

/** A globally unique reference, even when two papers use the same local id. */
export interface CanvasDiffEntityRef {
  documentId: string;
  kind: CanvasDiffEntityKind;
  localId: string;
  entityKey: string;
}

export interface CanvasDiffModifiedEntity {
  ref: CanvasDiffEntityRef;
  oldBlockHash: string;
  newBlockHash: string;
  changedFields: string[];
}

export interface CanvasDiffEntityChanges {
  added: CanvasDiffEntityRef[];
  removed: CanvasDiffEntityRef[];
  modified: CanvasDiffModifiedEntity[];
}

export interface CanvasDiffCountSummary {
  added: number;
  removed: number;
  modified: number;
  changed: number;
}

export interface CanvasDiffDocumentSummary {
  nodes: CanvasDiffCountSummary;
  edges: CanvasDiffCountSummary;
  evidence: CanvasDiffCountSummary;
  changed: number;
}

export interface CanvasDiffDocumentResult {
  documentId: string;
  state: CanvasDiffDocumentState;
  /** Comparison provenance is primary; baseline provenance is retained when it differs. */
  provenance: CanvasDiffDocumentProvenance;
  baselineProvenance?: CanvasDiffDocumentProvenance;
  nodes: CanvasDiffEntityChanges;
  edges: CanvasDiffEntityChanges;
  evidence: CanvasDiffEntityChanges;
  changedBlockHashes: Record<string, [string, string]>;
  summary: CanvasDiffDocumentSummary;
}

export interface CanvasDiffBatchSummary {
  documents: CanvasDiffCountSummary;
  nodes: CanvasDiffCountSummary;
  edges: CanvasDiffCountSummary;
  evidence: CanvasDiffCountSummary;
  totalChanges: number;
}

/** Stable, JSON-serializable contract for Vue and Agent review. */
export interface CanvasDiffReviewContract {
  apiVersion: typeof CANVAS_DIFF_API_VERSION;
  baseline: CanvasDiffGroupDescriptor;
  comparison: CanvasDiffGroupDescriptor;
  documents: CanvasDiffDocumentResult[];
  summary: CanvasDiffBatchSummary;
}

export type CanvasDiffBatchResult = CanvasDiffReviewContract;

/** Current PDF-agent result envelope; the agent may carry a patch or a graph snapshot. */
export interface CanvasDiffAgentResultEnvelope {
  documentId: string;
  provenance: CanvasDiffDocumentProvenance;
  project: ProjectState | null;
  graphPatch: PluginGraphPatch | null;
}

const NODE_CLAIM_FIELDS = ["id", "type", "title", "body", "tags", "data"] as const;
const EDGE_CLAIM_FIELDS = [
  "id",
  "type",
  "source",
  "target",
  "directed",
  "polarity",
  "confidence",
  "conditions",
  "note",
  "experiment",
] as const;
const EVIDENCE_CLAIM_FIELDS = [
  "id",
  "sourceType",
  "sourceId",
  "title",
  "authors",
  "year",
  "doi",
  "url",
] as const;

type Entity = ResearchNode | ResearchEdge | EvidenceRecord;

function compareBytes(left: string, right: string): number {
  const a = new TextEncoder().encode(left);
  const b = new TextEncoder().encode(right);
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  }
  return a.length - b.length;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function nonEmptyString(value: unknown): string | null {
  return typeof value === "string" && value.trim().length > 0 ? value.trim() : null;
}

function entityKey(documentId: string, kind: CanvasDiffEntityKind, localId: string): string {
  // JSON tuple encoding is unambiguous even if an id contains the separator.
  return JSON.stringify([documentId, kind, localId]);
}

export function createCanvasDiffEntityRef(
  documentId: string,
  kind: CanvasDiffEntityKind,
  localId: string,
): CanvasDiffEntityRef {
  return { documentId, kind, localId, entityKey: entityKey(documentId, kind, localId) };
}

function normalizeProvenance(
  documentId: string,
  provenance: Partial<CanvasDiffDocumentProvenance> | undefined,
  defaultOrigin: Provenance["origin"],
): CanvasDiffDocumentProvenance {
  const normalized: CanvasDiffDocumentProvenance = {
    documentId,
    origin: provenance?.origin ?? defaultOrigin,
  };
  if (provenance?.actorId) normalized.actorId = provenance.actorId;
  if (provenance?.modelId) normalized.modelId = provenance.modelId;
  if (provenance?.promptVersion) normalized.promptVersion = provenance.promptVersion;
  if (provenance?.reviewedBy) normalized.reviewedBy = provenance.reviewedBy;
  if (provenance?.reviewedAt) normalized.reviewedAt = provenance.reviewedAt;
  if (provenance?.sourceRefs) normalized.sourceRefs = [...provenance.sourceRefs].sort(compareBytes);
  if (provenance?.fileName) normalized.fileName = provenance.fileName;
  if (provenance?.sourcePath) normalized.sourcePath = provenance.sourcePath;
  return normalized;
}

/** Adapts the existing single-project canvas into a document-scoped input. */
export function adaptProjectToCanvasDiffDocument(
  project: ProjectState,
  metadata: {
    documentId: string;
    provenance?: Partial<CanvasDiffDocumentProvenance>;
  },
): CanvasDiffDocumentInput {
  const documentId = nonEmptyString(metadata.documentId);
  if (!documentId) throw new Error("Canvas Diff documentId is required");
  return {
    documentId,
    project,
    provenance: normalizeProvenance(documentId, metadata.provenance, "import"),
  };
}

function isProjectState(value: unknown): value is ProjectState {
  if (!isRecord(value)) return false;
  return (
    typeof value.id === "string" &&
    Array.isArray(value.nodes) &&
    Array.isArray(value.edges) &&
    Array.isArray(value.evidence) &&
    Array.isArray(value.placements)
  );
}

function isGraphPatch(value: unknown): value is PluginGraphPatch {
  if (!isRecord(value)) return false;
  return (
    value.apiVersion === "researchcanvas.dev/graph-patch/v1alpha1" &&
    value.reviewRequired === true &&
    isRecord(value.source) &&
    typeof value.title === "string" &&
    typeof value.summary === "string" &&
    Array.isArray(value.operations)
  );
}

/**
 * Adapts the current PDF-agent result shape without changing the agent host.
 *
 * Integration point: once the multi-file agent pipeline exists, pass each
 * per-file result and its stable job/document metadata through this adapter
 * before constructing a CanvasDiffDocumentInput. A GraphPatch alone is kept
 * for review but is not treated as a graph snapshot until the host supplies
 * the corresponding project.
 */
export function adaptPdfAgentResultForDiff(
  value: unknown,
  metadata: {
    documentId: string;
    provenance?: Partial<CanvasDiffDocumentProvenance>;
  },
): CanvasDiffAgentResultEnvelope | null {
  const documentId = nonEmptyString(metadata.documentId);
  if (!documentId) return null;
  const candidate = isRecord(value) ? value : {};
  const projectCandidate =
    candidate.project ??
    (isRecord(candidate.document) ? candidate.document.project : undefined) ??
    (isRecord(candidate.graph) ? candidate.graph.project : undefined);
  const patchCandidate =
    candidate.graphPatch ?? candidate.patch ?? (isGraphPatch(value) ? value : undefined);
  const project = isProjectState(projectCandidate) ? projectCandidate : null;
  const graphPatch = isGraphPatch(patchCandidate) ? patchCandidate : null;
  if (!project && !graphPatch) return null;
  return {
    documentId,
    provenance: normalizeProvenance(documentId, metadata.provenance, "ai"),
    project,
    graphPatch,
  };
}

export function canvasDiffDocumentFromAgentResult(
  value: unknown,
  metadata: {
    documentId: string;
    provenance?: Partial<CanvasDiffDocumentProvenance>;
  },
): CanvasDiffDocumentInput | null {
  const adapted = adaptPdfAgentResultForDiff(value, metadata);
  if (!adapted) return null;
  const project = adapted.project ?? (adapted.graphPatch
    ? materializeReviewPatchForDiff(adapted.graphPatch, adapted.documentId)
    : null);
  if (!project) return null;
  return {
    documentId: adapted.documentId,
    project,
    provenance: adapted.provenance,
  };
}

/** Build an isolated snapshot for comparison without applying the patch. */
function materializeReviewPatchForDiff(
  patch: PluginGraphPatch,
  documentId: string,
): ProjectState {
  const timestamp = "1970-01-01T00:00:00.000Z";
  const project: ProjectState = {
    schemaVersion: 2,
    id: `agent-diff-${documentId}`,
    title: patch.title,
    discipline: "Imported document",
    updatedAt: timestamp,
    revision: 0,
    nodes: [],
    edges: [],
    evidence: [],
    placements: [],
    scenarios: [],
    activity: [],
  };
  const phase = (operation: PluginGraphPatch["operations"][number]) =>
    operation.op === "add-node" ? 0 : operation.op === "update-node" ? 1 : operation.op === "add-edge" ? 2 : 3;
  for (const operation of [...patch.operations].sort((left, right) => phase(left) - phase(right))) {
    if (operation.op === "add-node") {
      if (project.nodes.some((node) => node.id === operation.node.id) ||
          !NODE_TYPES.includes(operation.node.type as (typeof NODE_TYPES)[number])) continue;
      const index = project.nodes.length;
      project.nodes.push({
        id: operation.node.id,
        type: operation.node.type as (typeof NODE_TYPES)[number],
        title: operation.node.title,
        body: operation.node.body ?? "",
        tags: operation.node.tags ?? [],
        status: "draft",
        evidenceIds: [],
        data: operation.node.data ?? {},
        provenance: { origin: "ai", actorId: patch.source.pluginId },
        createdAt: timestamp,
        updatedAt: timestamp,
      });
      project.placements.push({
        id: `placement-${operation.node.id}`,
        viewId: "view-main",
        nodeId: operation.node.id,
        x: (index % 6) * 220,
        y: Math.floor(index / 6) * 150,
        width: 176,
        height: 118,
      });
    } else if (operation.op === "update-node") {
      const node = project.nodes.find((candidate) => candidate.id === operation.nodeId);
      if (!node) continue;
      if (typeof operation.changes.title === "string") node.title = operation.changes.title;
      if (typeof operation.changes.body === "string") node.body = operation.changes.body;
      if (Array.isArray(operation.changes.tags) && operation.changes.tags.every((tag) => typeof tag === "string")) node.tags = operation.changes.tags;
      if (isRecord(operation.changes.data)) node.data = { ...node.data, ...operation.changes.data };
    } else if (operation.op === "add-edge") {
      if (project.edges.some((edge) => edge.id === operation.edge.id) ||
          !EDGE_TYPES.includes(operation.edge.type as (typeof EDGE_TYPES)[number]) ||
          !project.nodes.some((node) => node.id === operation.edge.source) ||
          !project.nodes.some((node) => node.id === operation.edge.target)) continue;
      project.edges.push({
        id: operation.edge.id,
        type: operation.edge.type as (typeof EDGE_TYPES)[number],
        source: operation.edge.source,
        target: operation.edge.target,
        directed: true,
        polarity: operation.edge.type === "contradicts" ? "negative" : "positive",
        conditions: [],
        evidenceIds: [],
        note: operation.edge.note,
        provenance: { origin: "ai", actorId: patch.source.pluginId },
      });
    } else {
      const edge = project.edges.find((candidate) => candidate.id === operation.edgeId);
      if (!edge) continue;
      if (typeof operation.changes.note === "string") edge.note = operation.changes.note;
      if (typeof operation.changes.type === "string" && EDGE_TYPES.includes(operation.changes.type as (typeof EDGE_TYPES)[number])) edge.type = operation.changes.type as (typeof EDGE_TYPES)[number];
      if (typeof operation.changes.confidence === "number" && operation.changes.confidence >= 0 && operation.changes.confidence <= 1) edge.confidence = operation.changes.confidence;
    }
  }
  return project;
}

function normalizeGroup(
  input: CanvasDiffDocumentCollection,
  fallbackGroupId: string,
  fallbackLabel: string,
): { descriptor: CanvasDiffGroupDescriptor; documents: Map<string, CanvasDiffDocumentInput> } {
  const group = (Array.isArray(input) ? { documents: input } : input) as CanvasDiffGroupInput;
  const groupId = nonEmptyString(group.groupId) ?? fallbackGroupId;
  const label = nonEmptyString(group.label) ?? fallbackLabel;
  const documents = new Map<string, CanvasDiffDocumentInput>();
  for (const item of group.documents) {
    const documentId = nonEmptyString(item.documentId);
    if (!documentId) throw new Error("Canvas Diff documentId is required");
    if (documents.has(documentId)) {
      throw new Error(`Duplicate Canvas Diff documentId: ${documentId}`);
    }
    documents.set(documentId, {
      documentId,
      project: item.project,
      provenance: normalizeProvenance(documentId, item.provenance, "import"),
    });
  }
  const documentIds = [...documents.keys()].sort(compareBytes);
  return { descriptor: { groupId, label, documentIds }, documents };
}

function fieldsFor(kind: CanvasDiffEntityKind): readonly string[] {
  if (kind === "node") return NODE_CLAIM_FIELDS;
  if (kind === "edge") return EDGE_CLAIM_FIELDS;
  return EVIDENCE_CLAIM_FIELDS;
}

function entityClaim(kind: CanvasDiffEntityKind, entity: Entity): Record<string, unknown> {
  const claim: Record<string, unknown> = {};
  for (const field of fieldsFor(kind)) {
    const value = (entity as unknown as Record<string, unknown>)[field];
    if (value !== undefined) claim[field] = value;
  }
  return claim;
}

async function sha256BlockHash(value: string): Promise<string> {
  const subtle = globalThis.crypto?.subtle;
  if (!subtle) {
    // Browser and current Node runtimes expose Web Crypto. The fallback keeps
    // the contract usable in restricted test sandboxes; it is not security.
    let hash = 2_166_136_261;
    for (const byte of new TextEncoder().encode(value)) {
      hash ^= byte;
      hash = Math.imul(hash, 16_777_619);
    }
    return (hash >>> 0).toString(16).padStart(12, "0");
  }
  const digest = await subtle.digest("SHA-256", new TextEncoder().encode(value));
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("")
    .slice(0, 12);
}

function claimKey(kind: CanvasDiffEntityKind, entity: Entity): string {
  return `${kind}:${canonicalizeDiffValue(entityClaim(kind, entity))}`;
}

async function hashEntity(
  kind: CanvasDiffEntityKind,
  entity: Entity,
  cache: Map<string, Promise<string>>,
): Promise<string> {
  const key = claimKey(kind, entity);
  const cached = cache.get(key);
  if (cached) return cached;
  const pending = sha256BlockHash(key);
  cache.set(key, pending);
  return pending;
}

function indexEntities(
  documentId: string,
  kind: CanvasDiffEntityKind,
  entities: readonly Entity[],
): Map<string, Entity> {
  const indexed = new Map<string, Entity>();
  for (const entity of entities) {
    const localId = nonEmptyString(entity.id);
    if (!localId) throw new Error(`Canvas Diff ${kind} id is required`);
    if (indexed.has(localId)) {
      throw new Error(`Duplicate Canvas Diff ${kind} id in ${documentId}: ${localId}`);
    }
    indexed.set(localId, entity);
  }
  return indexed;
}

function entityList(
  project: ProjectState | undefined,
  kind: CanvasDiffEntityKind,
): readonly Entity[] {
  if (!project) return [];
  if (kind === "node") return project.nodes;
  if (kind === "edge") return project.edges;
  return project.evidence;
}

function countSummary(changes: CanvasDiffEntityChanges): CanvasDiffCountSummary {
  const added = changes.added.length;
  const removed = changes.removed.length;
  const modified = changes.modified.length;
  return { added, removed, modified, changed: added + removed + modified };
}

async function diffEntityZone(
  documentId: string,
  kind: CanvasDiffEntityKind,
  baseline: ProjectState | undefined,
  comparison: ProjectState | undefined,
  hashCache: Map<string, Promise<string>>,
): Promise<{ changes: CanvasDiffEntityChanges; hashes: Record<string, [string, string]> }> {
  const baselineIndex = indexEntities(documentId, kind, entityList(baseline, kind));
  const comparisonIndex = indexEntities(documentId, kind, entityList(comparison, kind));
  const ids = [...new Set([...baselineIndex.keys(), ...comparisonIndex.keys()])].sort(compareBytes);
  const baselineHashes = new Map<string, string>();
  const comparisonHashes = new Map<string, string>();

  // Limit concurrent Web Crypto calls so many-document imports do not create
  // one promise per entity for the entire batch at once.
  for (let offset = 0; offset < ids.length; offset += 128) {
    const chunk = ids.slice(offset, offset + 128);
    await Promise.all(
      chunk.map(async (id) => {
        const before = baselineIndex.get(id);
        const after = comparisonIndex.get(id);
        if (before) baselineHashes.set(id, await hashEntity(kind, before, hashCache));
        if (after) comparisonHashes.set(id, await hashEntity(kind, after, hashCache));
      }),
    );
  }

  const added: CanvasDiffEntityRef[] = [];
  const removed: CanvasDiffEntityRef[] = [];
  const modified: CanvasDiffModifiedEntity[] = [];
  const hashes: Record<string, [string, string]> = {};
  for (const id of ids) {
    const ref = createCanvasDiffEntityRef(documentId, kind, id);
    const oldHash = baselineHashes.get(id) ?? "";
    const newHash = comparisonHashes.get(id) ?? "";
    if (!baselineIndex.has(id)) {
      added.push(ref);
      hashes[ref.entityKey] = ["", newHash];
    } else if (!comparisonIndex.has(id)) {
      removed.push(ref);
      hashes[ref.entityKey] = [oldHash, ""];
    } else if (oldHash !== newHash) {
      modified.push({
        ref,
        oldBlockHash: oldHash,
        newBlockHash: newHash,
        changedFields: changedClaimFields(
          kind,
          baselineIndex.get(id)!,
          comparisonIndex.get(id)!,
        ),
      });
      hashes[ref.entityKey] = [oldHash, newHash];
    }
  }
  return { changes: { added, removed, modified }, hashes };
}

function changedClaimFields(kind: CanvasDiffEntityKind, before: Entity, after: Entity): string[] {
  const oldClaim = entityClaim(kind, before);
  const newClaim = entityClaim(kind, after);
  return [...new Set([...Object.keys(oldClaim), ...Object.keys(newClaim)])]
    .filter((field) => {
      const oldValue = oldClaim[field];
      const newValue = newClaim[field];
      return canonicalizeDiffValue(oldValue) !== canonicalizeDiffValue(newValue);
    })
    .sort(compareBytes);
}

function changedDocumentSummary(
  nodes: CanvasDiffEntityChanges,
  edges: CanvasDiffEntityChanges,
  evidence: CanvasDiffEntityChanges,
): CanvasDiffDocumentSummary {
  const nodeSummary = countSummary(nodes);
  const edgeSummary = countSummary(edges);
  const evidenceSummary = countSummary(evidence);
  return {
    nodes: nodeSummary,
    edges: edgeSummary,
    evidence: evidenceSummary,
    changed: nodeSummary.changed + edgeSummary.changed + evidenceSummary.changed,
  };
}

async function diffDocument(
  documentId: string,
  baseline: CanvasDiffDocumentInput | undefined,
  comparison: CanvasDiffDocumentInput | undefined,
  hashCache: Map<string, Promise<string>>,
): Promise<CanvasDiffDocumentResult> {
  const before = baseline?.project;
  const after = comparison?.project;
  const [nodes, edges, evidence] = await Promise.all([
    diffEntityZone(documentId, "node", before, after, hashCache),
    diffEntityZone(documentId, "edge", before, after, hashCache),
    diffEntityZone(documentId, "evidence", before, after, hashCache),
  ]);
  const summary = changedDocumentSummary(nodes.changes, edges.changes, evidence.changes);
  const state: CanvasDiffDocumentState =
    !baseline ? "added" : !comparison ? "removed" : summary.changed === 0 ? "unchanged" : "modified";
  const provenance = comparison?.provenance
    ? normalizeProvenance(documentId, comparison.provenance, "import")
    : normalizeProvenance(documentId, baseline?.provenance, "import");
  const baselineProvenance =
    baseline && comparison &&
    canonicalizeDiffValue(baseline.provenance ?? {}) !== canonicalizeDiffValue(comparison.provenance ?? {})
      ? normalizeProvenance(documentId, baseline.provenance, "import")
      : undefined;
  const changedBlockHashes = Object.fromEntries(
    Object.entries({ ...nodes.hashes, ...edges.hashes, ...evidence.hashes }).sort(([a], [b]) =>
      compareBytes(a, b),
    ),
  );
  return {
    documentId,
    state,
    provenance,
    ...(baselineProvenance ? { baselineProvenance } : {}),
    nodes: nodes.changes,
    edges: edges.changes,
    evidence: evidence.changes,
    changedBlockHashes,
    summary,
  };
}

function aggregateCount(
  results: readonly CanvasDiffDocumentResult[],
  kind: "nodes" | "edges" | "evidence",
): CanvasDiffCountSummary {
  const added = results.reduce((total, result) => total + result.summary[kind].added, 0);
  const removed = results.reduce((total, result) => total + result.summary[kind].removed, 0);
  const modified = results.reduce((total, result) => total + result.summary[kind].modified, 0);
  return { added, removed, modified, changed: added + removed + modified };
}

/**
 * Computes a stable document-level diff for one or many documents per side.
 * Matching is by documentId; local node/edge/evidence ids are only compared
 * inside that document's namespace.
 */
export async function computeCanvasDiffBatch(
  request: CanvasDiffBatchRequest,
): Promise<CanvasDiffBatchResult> {
  const baseline = normalizeGroup(request.baseline, "baseline", "Baseline");
  const comparison = normalizeGroup(request.comparison, "comparison", "Comparison");
  const documentIds = [...new Set([...baseline.documents.keys(), ...comparison.documents.keys()])].sort(
    compareBytes,
  );
  const hashCache = new Map<string, Promise<string>>();
  const documents: CanvasDiffDocumentResult[] = [];
  for (let offset = 0; offset < documentIds.length; offset += 8) {
    const chunk = documentIds.slice(offset, offset + 8);
    const results = await Promise.all(
      chunk.map((documentId) =>
        diffDocument(
          documentId,
          baseline.documents.get(documentId),
          comparison.documents.get(documentId),
          hashCache,
        ),
      ),
    );
    documents.push(...results);
  }
  const addedDocuments = documents.filter((item) => item.state === "added").length;
  const removedDocuments = documents.filter((item) => item.state === "removed").length;
  const modifiedDocuments = documents.filter((item) => item.state === "modified").length;
  const unchangedDocuments = documents.filter((item) => item.state === "unchanged").length;
  const documentCounts = {
    added: addedDocuments,
    removed: removedDocuments,
    modified: modifiedDocuments,
    changed: addedDocuments + removedDocuments + modifiedDocuments,
  };
  const nodes = aggregateCount(documents, "nodes");
  const edges = aggregateCount(documents, "edges");
  const evidence = aggregateCount(documents, "evidence");
  return {
    apiVersion: CANVAS_DIFF_API_VERSION,
    baseline: baseline.descriptor,
    comparison: comparison.descriptor,
    documents,
    summary: {
      documents: documentCounts,
      nodes,
      edges,
      evidence,
      totalChanges: nodes.changed + edges.changed + evidence.changed,
    },
  };
}

export function toCanvasDiffReviewContract(
  result: CanvasDiffBatchResult,
): CanvasDiffReviewContract {
  return result;
}

export function emptyCanvasDiffBatch(): CanvasDiffBatchResult {
  return {
    apiVersion: CANVAS_DIFF_API_VERSION,
    baseline: { groupId: "baseline", label: "Baseline", documentIds: [] },
    comparison: { groupId: "comparison", label: "Comparison", documentIds: [] },
    documents: [],
    summary: {
      documents: { added: 0, removed: 0, modified: 0, changed: 0 },
      nodes: { added: 0, removed: 0, modified: 0, changed: 0 },
      edges: { added: 0, removed: 0, modified: 0, changed: 0 },
      evidence: { added: 0, removed: 0, modified: 0, changed: 0 },
      totalChanges: 0,
    },
  };
}
