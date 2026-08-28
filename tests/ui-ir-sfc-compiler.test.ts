import assert from "node:assert/strict";
import test from "node:test";
import { compileUiIrSfc, compileUiIrSfcArtifact, UiIrSfcCompileError } from "../app/plugins/ui-ir-sfc-compiler";

const valid = [
  "<template>",
  "  <UiStack direction=\"column\" gap=\"2\">",
  "    <UiText :text=\"state.job.title\" text-fallback=\"Pending patch review\" />",
  "    <UiText tone=\"muted\">Review the proposed patch.</UiText>",
  "    <UiButton label=\"Accept patch\" variant=\"primary\" action-id=\"review.accept\" capability=\"agent.review.request\" parameter-source=\"sfc\" />",
  "    <UiButton label=\"Reject patch\" variant=\"danger\" action-id=\"review.reject\" capability=\"agent.review.request\" />",
  "  </UiStack>",
  "</template>",
].join("\n");

test("template-only SFC compiles to the expected UiIR golden", () => {
  const ir = compileUiIrSfc(valid, "AgentReview.vue");
  assert.deepEqual(ir, {
    apiVersion: "anyway.dev/ui-ir/v1",
    root: {
      type: "stack",
      direction: "column",
      gap: 2,
      children: [
        { type: "text", text: { type: "state-binding", path: "job.title", fallback: "Pending patch review" } },
        { type: "text", tone: "muted", text: "Review the proposed patch." },
        {
          type: "button",
          label: "Accept patch",
          action: { type: "action-binding", actionId: "review.accept", capability: "agent.review.request", parameters: { source: "sfc" } },
          variant: "primary",
        },
        {
          type: "button",
          label: "Reject patch",
          action: { type: "action-binding", actionId: "review.reject", capability: "agent.review.request" },
          variant: "danger",
        },
      ],
    },
  });
  assert.equal(compileUiIrSfcArtifact(valid), compileUiIrSfcArtifact(valid));
});

function rejects(source: string, code: string): void {
  assert.throws(
    () => compileUiIrSfc(source, "invalid.vue"),
    (error: unknown) => error instanceof UiIrSfcCompileError && error.code === code,
  );
}

test("SFC compiler rejects executable blocks and native/dynamic template features", () => {
  const cases: Array<[string, string]> = [
    ["<template><UiText>ok</UiText></template><script>export default {}</script>", "sfc-block-forbidden"],
    ["<template><UiText>ok</UiText></template><style>.x{}</style>", "sfc-block-forbidden"],
    ["<template><div>native</div></template>", "native-html-forbidden"],
    ["<template><component :is=\"state.component\" /></template>", "native-html-forbidden"],
    ["<template><UiText v-html=\"state.html\" /></template>", "directive-forbidden"],
    ["<template><UiStack v-for=\"item in state.items\"><UiText>item</UiText></UiStack></template>", "directive-forbidden"],
    ["<template><UiStack v-if=\"state.visible\"><UiText>item</UiText></UiStack></template>", "directive-forbidden"],
    ["<template><UiButton @click=\"doSomething\" label=\"Run\" action-id=\"run\" capability=\"run\" /></template>", "directive-forbidden"],
  ];
  for (const [source, code] of cases) rejects(source, code);
});

test("SFC compiler rejects arbitrary expressions, unsafe paths and pollution", () => {
  rejects("<template><UiText :text=\"state.job.title + secret\" /></template>", "expression-forbidden");
  rejects("<template><UiText :text=\"state.__proto__.polluted\" /></template>", "expression-forbidden");
  rejects("<template><UiButton label=\"Run\" action-id=\"run\" capability=\"run\" :action-id=\"state.action\" /></template>", "attribute-duplicate");
  rejects("<template><UiButton label=\"Run\" action-id=\"run\" capability=\"run\" v-bind=\"state.props\" /></template>", "directive-forbidden");
  rejects("<template><UiText :text=\"state.job.title\" :text=\"state.other\" /></template>", "sfc-invalid");
});

test("SFC compiler reuses UiIR limits for strings, depth and properties", () => {
  rejects("<template><UiText>" + "x".repeat(513) + "</UiText></template>", "string-limit-exceeded");
  let deep = "<UiStack>";
  for (let index = 0; index < 20; index += 1) deep += "<UiStack>";
  deep += "<UiText>deep</UiText>";
  for (let index = 0; index < 21; index += 1) deep += "</UiStack>";
  rejects("<template>" + deep + "</template>", "depth-exceeded");
  rejects("<template><UiStack bad-prop=\"x\"><UiText>bad</UiText></UiStack></template>", "prop-forbidden");
});
