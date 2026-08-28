/**
 * AnMarket provider contracts.
 *
 * This file is intentionally dependency-free. It describes the data boundary
 * between the official AnCordis plugin and the Anyway Kernel. Provider code
 * must not use local paths or writable handles; large values travel as BlobRef.
 */

export const ANMARKET_API_VERSION = "anyway.dev/anmarket/v1alpha1" as const;

export type AnMarketApiVersion = typeof ANMARKET_API_VERSION;
export type HashAlgorithm = "sha256";
export type Digest = `${HashAlgorithm}:${string}`;
export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { readonly [key: string]: JsonValue };
export type JsonObject = { readonly [key: string]: JsonValue };

export interface BlobRef {
  readonly kind: "BlobRef";
  readonly store: "kernel";
  readonly digest: Digest;
  readonly size: number;
  readonly mediaType: string;
  readonly access: "read-only";
  readonly retention: "job" | "report" | "package";
}

export interface BlobRange {
  readonly offset: number;
  readonly length: number;
}

export interface BlobReadRequest {
  readonly ref: BlobRef;
  readonly range?: BlobRange;
  readonly maxBytes: number;
}

export interface BlobChunk {
  readonly ref: BlobRef;
  readonly range: BlobRange;
  readonly bytes: ReadonlyArray<number>;
  readonly eof: boolean;
}

export interface PluginIdentity {
  readonly id: string;
  readonly version: string;
  readonly publisher: string;
  readonly publisherKeyId: string;
  readonly principal: string;
}

export interface SubjectRef {
  readonly pluginId: string;
  readonly version: string;
  readonly subjectHash: Digest;
  readonly manifest: BlobRef;
  readonly artifact: BlobRef;
}

export interface RegistryQuery {
  readonly id?: string;
  readonly channel?: "stable" | "beta" | "nightly";
  readonly platform?: string;
  readonly compatibleApi?: string;
}

export interface RegistryCandidate {
  readonly pluginId: string;
  readonly version: string;
  readonly publisher: string;
  readonly subjectHash?: Digest;
  readonly source: string;
  readonly summary: JsonObject;
}

export interface RegistryArtifact {
  readonly candidate: RegistryCandidate;
  readonly subject: SubjectRef;
  readonly source: string;
}

export interface RegistryProvider {
  readonly identity: PluginIdentity;
  readonly kind: "registry-provider";
  search(request: RegistryQuery): Promise<ReadonlyArray<RegistryCandidate>>;
  fetch(candidate: RegistryCandidate): Promise<RegistryArtifact>;
}

export interface AnalyzerInput {
  readonly subject: SubjectRef;
  readonly requestedPermissions: PermissionSet;
  readonly previousPermissions?: PermissionSet;
}

export interface AnalyzerHostRpc {
  readonly blobRead: (request: BlobReadRequest) => Promise<BlobChunk>;
  readonly reportProgress: (progress: ScanProgress) => Promise<void>;
}

export interface AnalyzerProvider {
  readonly identity: PluginIdentity;
  readonly kind: "analyzer-provider";
  analyze(input: AnalyzerInput, host: AnalyzerHostRpc): Promise<ReadonlyArray<ScanFinding>>;
}

export interface ReputationQuery {
  readonly subject: SubjectRef;
  readonly publisherKeyId: string;
}

export interface ReputationAssessment {
  readonly provider: PluginIdentity;
  readonly status: "unknown" | "trusted" | "review" | "blocked";
  readonly score?: number;
  readonly reasons: ReadonlyArray<string>;
  readonly observedAt: string;
}

export interface BlocklistRequest {
  readonly channel: "stable" | "beta" | "nightly";
  readonly sinceVersion?: number;
}

export interface BlocklistEntry {
  readonly subjectHash?: Digest;
  readonly pluginId?: string;
  readonly publisherKeyId?: string;
  readonly reason: string;
  readonly expiresAt?: string;
}

export interface SignedBlocklistFeed {
  readonly feedId: string;
  readonly version: number;
  readonly issuedAt: string;
  readonly expiresAt: string;
  readonly signerKeyId: string;
  readonly payloadHash: Digest;
  readonly entries: ReadonlyArray<BlocklistEntry>;
  readonly signature: string;
}

export interface ReputationProvider {
  readonly identity: PluginIdentity;
  readonly kind: "reputation-provider";
  lookup(request: ReputationQuery): Promise<ReputationAssessment>;
  fetchBlocklist?(request: BlocklistRequest): Promise<SignedBlocklistFeed>;
}

export type PermissionName =
  | "registry.read"
  | "blob.read"
  | "scan.run"
  | "network.request"
  | "process.spawn"
  | "workspace.read"
  | "workspace.write";

export interface PermissionSet {
  readonly required: ReadonlyArray<PermissionName>;
  readonly optional: ReadonlyArray<PermissionName>;
}

export interface PermissionDiff {
  readonly previous?: PermissionSet;
  readonly requested: PermissionSet;
  readonly added: ReadonlyArray<PermissionName>;
  readonly removed: ReadonlyArray<PermissionName>;
  readonly changed: ReadonlyArray<PermissionName>;
  readonly unchanged: ReadonlyArray<PermissionName>;
}

export type FindingSeverity = "info" | "low" | "medium" | "high" | "critical";

export interface FindingLocation {
  readonly path?: string;
  readonly offset?: number;
  readonly length?: number;
  readonly jsonPointer?: string;
}

export interface ScanFinding {
  readonly findingId: string;
  readonly severity: FindingSeverity;
  readonly category: "malware" | "vulnerability" | "integrity" | "license" | "policy" | "quality";
  readonly ruleId: string;
  readonly message: string;
  readonly confidence: number;
  readonly location?: FindingLocation;
  readonly evidenceHash?: Digest;
  readonly remediation?: string;
}

export interface AnalyzerBinding {
  readonly id: string;
  readonly version: string;
  readonly publisherKeyId: string;
  readonly invocationId: string;
}

export interface ScanProgress {
  readonly requestId: string;
  readonly analyzerId: string;
  readonly completed: number;
  readonly total?: number;
}

export interface ScanRequest {
  readonly requestId: string;
  readonly subject: SubjectRef;
  readonly analyzers: ReadonlyArray<PluginIdentity>;
  readonly timeoutMs: number;
  readonly maxParallelism: number;
  readonly failClosed: true;
}

export interface ScanReport {
  readonly reportVersion: 1;
  readonly requestId: string;
  readonly status: "complete" | "incomplete" | "quarantined";
  readonly subject: {
    readonly pluginId: string;
    readonly version: string;
    readonly subjectHash: Digest;
  };
  readonly analyzers: ReadonlyArray<AnalyzerBinding>;
  readonly policyVersion: string;
  readonly permissionDiff: PermissionDiff;
  readonly findings: ReadonlyArray<ScanFinding>;
  readonly reputation: ReadonlyArray<ReputationAssessment>;
  readonly blocklistVersions: ReadonlyArray<number>;
  readonly timedOutAnalyzers: ReadonlyArray<string>;
  readonly failedAnalyzers: ReadonlyArray<string>;
  readonly generatedAt: string;
  readonly reportHash: Digest;
}

export interface PolicyAdvice {
  readonly policyVersion: string;
  readonly recommendation: "allow" | "review" | "quarantine" | "deny";
  readonly reasons: ReadonlyArray<string>;
  readonly requiredApprovals: ReadonlyArray<string>;
}

export interface PolicyAdvisor {
  readonly identity: PluginIdentity;
  readonly kind: "policy-advisor";
  advise(input: {
    readonly report: ScanReport;
    readonly permissionDiff: PermissionDiff;
  }): Promise<PolicyAdvice>;
}

export interface InsideRpcEnvelope<TPayload extends JsonValue = JsonValue> {
  readonly protocol: "anyway.inside-rpc/v1alpha1";
  readonly requestId: string;
  readonly principal: string;
  readonly method: string;
  readonly deadlineAt: string;
  readonly payload: TPayload;
  readonly blobRefs: ReadonlyArray<BlobRef>;
}

export interface InsideRpcError {
  readonly code: "unauthorized" | "invalid" | "timeout" | "quota" | "quarantined" | "internal";
  readonly message: string;
  readonly retryable: boolean;
}

export interface InsideRpcResponse<TPayload extends JsonValue = JsonValue> {
  readonly requestId: string;
  readonly ok: boolean;
  readonly payload?: TPayload;
  readonly error?: InsideRpcError;
}

export type VueIrComponent =
  | "anmarket.registry-list"
  | "anmarket.install-card"
  | "anmarket.permission-diff"
  | "anmarket.scan-report"
  | "anmarket.finding-list"
  | "anmarket.update-history";

export interface VueIrBinding {
  readonly path: string;
  readonly fallback?: JsonValue;
}

export interface VueIrAction {
  readonly id: string;
  readonly command:
    | "anmarket.install.request"
    | "anmarket.scan.request"
    | "anmarket.update.rollback"
    | "anmarket.report.open";
  readonly args: JsonObject;
}

export interface VueIrNode {
  readonly type: "component" | "text" | "slot";
  readonly component?: VueIrComponent;
  readonly text?: string;
  readonly props?: JsonObject;
  readonly bindings?: ReadonlyArray<VueIrBinding>;
  readonly action?: VueIrAction;
  readonly children?: ReadonlyArray<VueIrNode>;
}

export interface VueIrDocument {
  readonly schema: "anyway.vue-ir/v1alpha1";
  readonly root: VueIrNode;
  readonly allowedSlots: ReadonlyArray<string>;
}
