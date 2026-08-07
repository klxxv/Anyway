/**
 * PDF Agent UI 纯逻辑测试
 * =========================
 * 覆盖阶段 1/3 的组件纯函数（不渲染 React）：
 * 1. jobStageIndex —— 上传管线阶段顺序与终态处理
 * 2. deriveUploadProgress —— 进度条推导
 * 3. buildAcceptedPatch / countAccepted —— 逐项审阅决策与就地编辑
 * 4. operationSubject —— 操作主语摘要
 * 5. patchOperationsOf —— AgentJobStatus → GraphPatch 操作提取
 *
 * 组件本身只做编排，纯逻辑可被 tsx --test 直接测试（memory 约定）。
 */

import assert from "node:assert/strict";
import test from "node:test";

import type { AgentJobStatus } from "../app/plugins/agent-contracts";
import { AGENT_STAGE_LABELS } from "../app/plugins/agent-contracts";
import type { PluginGraphPatch } from "../app/plugins/contracts";
import { patchOperationsOf } from "../app/platform/agent-client";
import {
  PDF_PIPELINE_STAGES,
  deriveUploadProgress,
  jobStageIndex,
} from "../app/features/research-workspace/components/pdf-upload-dialog";
import {
  buildAcceptedPatch,
  countAccepted,
  operationSubject,
} from "../app/features/research-workspace/components/agent-review-panel";

function mockStatus(overrides: Partial<AgentJobStatus> = {}): AgentJobStatus {
  return {
    jobId: "a1b2c3d4e5f6a7b8",
    pdfPath: "/tmp/paper.pdf",
    fileHash: "sha256:abc",
    state: "extracting_text",
    progress: [2, 7],
    createdAt: 1750000000000,
    updatedAt: 1750000001000,
    error: null,
    result: null,
    ...overrides,
  };
}

const samplePatch: PluginGraphPatch = {
  apiVersion: "researchcanvas.dev/graph-patch/v1alpha1",
  source: { pluginId: "pdf-canvas-agent", operation: "pdf-document-extraction", externalId: "job-1" },
  title: "PDF structure extraction (1 sections, 2 paragraphs)",
  summary: "Extracted document structure from PDF.",
  reviewRequired: true,
  operations: [
    {
      op: "add-node",
      node: { id: "pdf-sec-s1", type: "note", title: "Introduction", tags: ["pdf-import"] },
    },
    {
      op: "add-node",
      node: {
        id: "pdf-p1",
        type: "evidence",
        title: "This paper presents a novel approach…",
        body: "Full paragraph text…",
        tags: ["pdf-import"],
      },
    },
    {
      op: "add-edge",
      edge: {
        id: "pdf-edge-para-p1",
        source: "pdf-p1",
        target: "pdf-sec-s1",
        type: "part_of",
        note: "paragraph belongs to section",
      },
    },
  ],
};

test("jobStageIndex 覆盖管线全部阶段并按序递增", () => {
  assert.equal(PDF_PIPELINE_STAGES.length, 8);
  PDF_PIPELINE_STAGES.forEach((stage, index) => {
    assert.equal(jobStageIndex(stage), index, `${stage} should be at index ${index}`);
  });
  // 终态不在管线内
  assert.equal(jobStageIndex("accepted"), -1);
  assert.equal(jobStageIndex("rejected"), -1);
  assert.equal(jobStageIndex("failed"), -1);
});

test("deriveUploadProgress 正确推导检查点与百分比", () => {
  const progress = deriveUploadProgress(mockStatus());
  assert.equal(progress.done, 2);
  assert.equal(progress.total, 7);
  assert.equal(progress.percent, 29);
  assert.match(progress.stageLabel, /extracting/i);

  // 完成后 100%
  const done = deriveUploadProgress(
    mockStatus({ state: "awaiting_review", progress: [7, 7] }),
  );
  assert.equal(done.percent, 100);

  // 空进度不除零
  const empty = deriveUploadProgress(mockStatus({ progress: [0, 0] }));
  assert.equal(empty.percent, 0);
});

test("buildAcceptedPatch 过滤拒绝项并保留默认接受", () => {
  // 默认全部接受
  const all = buildAcceptedPatch(samplePatch, {});
  assert.ok(all);
  assert.equal(all!.operations.length, 3);

  // 拒绝 index 1
  const partial = buildAcceptedPatch(samplePatch, { 1: { accept: false } });
  assert.ok(partial);
  assert.equal(partial!.operations.length, 2);
  assert.ok(partial!.operations.every((op) => op.op !== "add-node" || op.node.id !== "pdf-p1"));

  // 全部拒绝 → null
  const none = buildAcceptedPatch(samplePatch, {
    0: { accept: false },
    1: { accept: false },
    2: { accept: false },
  });
  assert.equal(none, null);
});

test("buildAcceptedPatch 应用就地编辑（标题/备注）", () => {
  const edited = buildAcceptedPatch(samplePatch, {
    0: { accept: true, edits: { title: "1 Introduction (revised)" } },
    2: { accept: true, edits: { note: "revised note" } },
  });
  assert.ok(edited);
  const first = edited!.operations[0];
  assert.equal(first.op, "add-node");
  if (first.op === "add-node") assert.equal(first.node.title, "1 Introduction (revised)");
  const edge = edited!.operations.find((op) => op.op === "add-edge");
  assert.ok(edge && edge.op === "add-edge");
  if (edge?.op === "add-edge") assert.equal(edge.edge.note, "revised note");
});

test("countAccepted 统计接受项（缺省视为接受）", () => {
  assert.equal(countAccepted({}, 3), 3);
  assert.equal(countAccepted({ 1: { accept: false } }, 3), 2);
  assert.equal(countAccepted({ 0: { accept: false }, 1: { accept: false }, 2: { accept: false } }, 3), 0);
});

test("operationSubject 返回各操作类型的主语", () => {
  assert.equal(operationSubject(samplePatch.operations[0]), "Introduction");
  assert.equal(
    operationSubject(samplePatch.operations[2]),
    "pdf-p1 → pdf-sec-s1",
  );
});

test("patchOperationsOf 从 AgentJobStatus.result 提取操作数组", () => {
  const status = mockStatus({ result: { ...samplePatch } });
  assert.equal(patchOperationsOf(status).length, 3);
  assert.equal(patchOperationsOf(mockStatus()).length, 0);
  assert.equal(patchOperationsOf(mockStatus({ result: { operations: "nope" } })).length, 0);
});

test("AgentJobStatus 序列化契约与 Rust PdfJobStatus 对齐（camelCase）", () => {
  // Rust 端 PdfJobStatus 序列化字段（camelCase）必须能被 AgentJobStatus 接收。
  const rustPayload = {
    jobId: "job-1",
    pdfPath: "/tmp/paper.pdf",
    fileHash: "sha256:abc",
    state: "awaiting_review",
    progress: [7, 7],
    createdAt: 1750000000000,
    updatedAt: 1750000001000,
    error: null,
    result: {
      apiVersion: "researchcanvas.dev/graph-patch/v1alpha1",
      reviewRequired: true,
      operations: [{ op: "add-node", node: { id: "n1", type: "note", title: "T" } }],
    },
  };
  const status = rustPayload as unknown as AgentJobStatus;
  assert.equal(status.state, "awaiting_review");
  assert.deepEqual(status.progress, [7, 7]);
  assert.equal(patchOperationsOf(status).length, 1);

  // 阶段名与 AgentJobStage 标签表一致
  assert.ok(AGENT_STAGE_LABELS.awaiting_review.includes(" / "));
});
