"""Bounded incremental protocol for model-produced PDF extraction frames.

The model emits one small JSON object per line.  This module never accepts a
single document-sized JSON value and deliberately excludes private reasoning,
credentials, request headers, and local paths from the wire contract.
"""

from __future__ import annotations

from dataclasses import dataclass
import json
from typing import Any, Iterable, Mapping

MAX_FRAME_BYTES = 8 * 1024
MAX_STREAM_BYTES = 2 * 1024 * 1024
MAX_FRAMES = 512
MAX_TEXT_BYTES = 2 * 1024
MAX_IDENTIFIER_BYTES = 160
MAX_PATCH_BYTES = 48 * 1024
MAX_PATCH_OPERATIONS = 128
FRAME_TYPES = frozenset({"progress", "evidence", "entity", "operator", "relation", "warning", "error", "end"})
ENTITY_TYPES = frozenset({"question", "concept", "variable", "method", "dataset", "evidence", "result", "note"})
RELATION_TYPES = frozenset({"T", "K", "I", "M", "Q"})
ENTITY_TYPE_ALIASES = {
    "claim": "concept",
    "finding": "result",
    "outcome": "result",
    "measure": "variable",
    "procedure": "method",
    "data": "dataset",
}
RELATION_TYPE_ALIASES = {
    "transform": "T",
    "transforms": "T",
    "derives": "T",
    "dependency": "K",
    "depends": "K",
    "influences": "K",
    "causal": "K",
    "intervention": "I",
    "intervenes": "I",
    "marginalization": "M",
    "omits": "M",
    "equivalence": "Q",
    "quotient": "Q",
}
FORBIDDEN_KEYS = frozenset({
    "apikey", "api_key", "authorization", "chainofthought", "chain_of_thought",
    "credential", "headers", "path", "principal", "reasoning", "reasoning_content",
    "secret", "token",
})

SYSTEM_PROMPT_SEGMENTS = (
    (
        "role",
        "You extract source-grounded research entities, evidence and relationships from one research document.",
    ),
    (
        "instruction",
        "Read only the supplied document text. Emit evidence, entity and relation records, then one end record. Emit operator only as a compatibility alias. Emit progress records only when the progress policy requests them.",
    ),
    (
        "constraints",
        "Never reveal hidden reasoning, credentials, headers or file paths. Do not infer unsupported claims. Keep every line below 8192 UTF-8 bytes.",
    ),
    (
        "frame-schema",
        "Output NDJSON only. Each line has type and strictly increasing seq starting at 1. Allowed types: progress, evidence, entity, relation, operator, warning, error, end. Entity entityType is exactly one of question, concept, variable, method, dataset, evidence, result, note. Relation relationType/operatorType is exactly one of T, K, I, M, Q. No markdown fences and no aggregate JSON document.",
    ),
    (
        "graph-semantics",
        "Use T for transformation or derivation, K for kernel/dependency, I for intervention, M for marginalization/omission, and Q for quotient/equivalence. Every entity and relation must cite existing evidenceIds. Relation sourceId and targetId must name entity frames from this stream. Use stable ASCII identifiers without whitespace.",
    ),
    (
        "termination",
        "Emit exactly one end frame last with status review-required when extraction is usable, or incomplete when evidence is insufficient. Do not emit any frame after end. Keep summaries concise and source-grounded.",
    ),
)


class TypedFrameError(ValueError):
    def __init__(self, code: str, message: str, *, retryable: bool = False):
        super().__init__(message)
        self.code = code
        self.message = message[:512]
        self.retryable = retryable


def layered_system_prompt() -> tuple[Mapping[str, str], ...]:
    """Return stable, independently inspectable prompt segments."""
    return tuple({"segment": name, "content": text} for name, text in SYSTEM_PROMPT_SEGMENTS)


def rendered_system_prompt() -> str:
    return "\n\n".join(f"[{name}]\n{text}" for name, text in SYSTEM_PROMPT_SEGMENTS)


def _utf8_len(value: str) -> int:
    return len(value.encode("utf-8"))


def _text(value: Any, field: str, limit: int = MAX_TEXT_BYTES, *, required: bool = True) -> str | None:
    if value is None and not required:
        return None
    if not isinstance(value, str) or (required and not value) or _utf8_len(value) > limit:
        raise TypedFrameError("FRAME_SCHEMA_INVALID", f"frame.{field} is invalid")
    return value


def _identifier(value: Any, field: str) -> str:
    result = _text(value, field, MAX_IDENTIFIER_BYTES)
    assert result is not None
    if any(character.isspace() or ord(character) < 0x20 for character in result):
        raise TypedFrameError("FRAME_SCHEMA_INVALID", f"frame.{field} is invalid")
    return result


def _identifiers(value: Any, field: str) -> list[str]:
    if value is None:
        return []
    if not isinstance(value, list) or len(value) > 64:
        raise TypedFrameError("FRAME_SCHEMA_INVALID", f"frame.{field} is invalid")
    return [_identifier(item, field) for item in value]


def _contains_forbidden_key(value: Any) -> bool:
    if isinstance(value, Mapping):
        return any(str(key).replace("-", "_").lower() in FORBIDDEN_KEYS or _contains_forbidden_key(item) for key, item in value.items())
    if isinstance(value, list):
        return any(_contains_forbidden_key(item) for item in value)
    return False


_ALLOWED_FIELDS = {
    "progress": frozenset({"type", "seq", "stage", "message", "percent"}),
    "evidence": frozenset({"type", "seq", "id", "quote", "page", "section"}),
    "entity": frozenset({"type", "seq", "id", "entityType", "title", "summary", "evidenceIds"}),
    "operator": frozenset({"type", "seq", "id", "operatorType", "sourceId", "targetId", "label", "evidenceIds"}),
    "relation": frozenset({"type", "seq", "id", "relationType", "operatorType", "sourceId", "targetId", "label", "evidenceIds"}),
    "warning": frozenset({"type", "seq", "code", "message", "retryable"}),
    "error": frozenset({"type", "seq", "code", "message", "retryable"}),
    "end": frozenset({"type", "seq", "status", "summary"}),
}


def validate_frame(value: Any, expected_seq: int) -> dict[str, Any]:
    if not isinstance(value, Mapping):
        raise TypedFrameError("FRAME_SCHEMA_INVALID", "typed frame must be an object")
    if _contains_forbidden_key(value):
        raise TypedFrameError("FRAME_SECRET_FIELD", "typed frame contains a forbidden field")
    frame_type = value.get("type")
    if frame_type not in FRAME_TYPES:
        raise TypedFrameError("FRAME_TYPE_INVALID", "typed frame type is not allowed")
    if set(value) - _ALLOWED_FIELDS[frame_type]:
        raise TypedFrameError("FRAME_SCHEMA_INVALID", "typed frame contains unknown fields")
    sequence = value.get("seq")
    if isinstance(sequence, bool) or not isinstance(sequence, int) or sequence != expected_seq:
        raise TypedFrameError("FRAME_SEQUENCE_INVALID", "typed frame sequence is duplicate, missing or out of order")

    frame = dict(value)
    if frame_type == "progress":
        frame["stage"] = _identifier(value.get("stage"), "stage")
        frame["message"] = _text(value.get("message"), "message")
        percent = value.get("percent")
        if isinstance(percent, bool) or not isinstance(percent, (int, float)) or not 0 <= percent <= 100:
            raise TypedFrameError("FRAME_SCHEMA_INVALID", "frame.percent is invalid")
    elif frame_type == "evidence":
        frame["id"] = _identifier(value.get("id"), "id")
        frame["quote"] = _text(value.get("quote"), "quote")
        page = value.get("page")
        if page is not None and (isinstance(page, bool) or not isinstance(page, int) or page < 1 or page > 100_000):
            raise TypedFrameError("FRAME_SCHEMA_INVALID", "frame.page is invalid")
        if "section" in value:
            frame["section"] = _text(value.get("section"), "section", 512, required=False)
    elif frame_type == "entity":
        frame["id"] = _identifier(value.get("id"), "id")
        frame["entityType"] = _identifier(value.get("entityType"), "entityType")
        frame["entityType"] = ENTITY_TYPE_ALIASES.get(frame["entityType"].lower(), frame["entityType"])
        if frame["entityType"] not in ENTITY_TYPES:
            raise TypedFrameError("FRAME_SCHEMA_INVALID", "frame.entityType is not a supported graph node type")
        frame["title"] = _text(value.get("title"), "title", 500)
        if "summary" in value:
            frame["summary"] = _text(value.get("summary"), "summary", 2_000, required=False)
        frame["evidenceIds"] = _identifiers(value.get("evidenceIds"), "evidenceIds")
    elif frame_type == "operator":
        for field in ("id", "operatorType", "sourceId", "targetId"):
            frame[field] = _identifier(value.get(field), field)
        frame["operatorType"] = RELATION_TYPE_ALIASES.get(frame["operatorType"].lower(), frame["operatorType"])
        frame["label"] = _text(value.get("label"), "label", 500)
        if frame["operatorType"] not in RELATION_TYPES:
            raise TypedFrameError("FRAME_SCHEMA_INVALID", "frame.operatorType is not a supported graph edge type")
        frame["evidenceIds"] = _identifiers(value.get("evidenceIds"), "evidenceIds")
    elif frame_type == "relation":
        for field in ("id", "sourceId", "targetId"):
            frame[field] = _identifier(value.get(field), field)
        relation_type = value.get("relationType", value.get("operatorType"))
        frame["relationType"] = _identifier(relation_type, "relationType")
        frame["relationType"] = RELATION_TYPE_ALIASES.get(frame["relationType"].lower(), frame["relationType"])
        frame["operatorType"] = frame["relationType"]
        if frame["relationType"] not in RELATION_TYPES:
            raise TypedFrameError("FRAME_SCHEMA_INVALID", "frame.relationType is not a supported graph edge type")
        frame["label"] = _text(value.get("label"), "label", 500)
        frame["evidenceIds"] = _identifiers(value.get("evidenceIds"), "evidenceIds")
    elif frame_type in {"warning", "error"}:
        frame["code"] = _identifier(value.get("code"), "code")
        frame["message"] = _text(value.get("message"), "message", 512)
        if not isinstance(value.get("retryable"), bool):
            raise TypedFrameError("FRAME_SCHEMA_INVALID", "frame.retryable is invalid")
    else:
        if value.get("status") not in {"review-required", "incomplete"}:
            raise TypedFrameError("FRAME_SCHEMA_INVALID", "frame.status is invalid")
        frame["summary"] = _text(value.get("summary"), "summary", 2_000)
    return frame


class TypedFrameParser:
    """Incremental UTF-8 NDJSON parser with a bounded, terminal stream."""

    def __init__(self) -> None:
        self._buffer = bytearray()
        self._total = 0
        self._frames: list[dict[str, Any]] = []
        self._ended = False
        self._cancelled = False

    @property
    def frames(self) -> tuple[Mapping[str, Any], ...]:
        return tuple(self._frames)

    def cancel(self) -> None:
        self._cancelled = True

    def feed(self, chunk: bytes) -> list[dict[str, Any]]:
        if self._cancelled:
            raise TypedFrameError("FRAME_CANCELLED", "typed frame parsing was cancelled", retryable=True)
        if not isinstance(chunk, bytes):
            raise TypeError("typed frame chunk must be bytes")
        self._total += len(chunk)
        if self._total > MAX_STREAM_BYTES:
            raise TypedFrameError("FRAME_STREAM_TOO_LARGE", "typed frame stream exceeds its byte limit")
        self._buffer.extend(chunk)
        if len(self._buffer) > MAX_FRAME_BYTES and b"\n" not in self._buffer:
            raise TypedFrameError("FRAME_TOO_LARGE", "typed frame exceeds its line limit")
        parsed: list[dict[str, Any]] = []
        while b"\n" in self._buffer:
            raw, _, remainder = self._buffer.partition(b"\n")
            self._buffer = bytearray(remainder)
            if not raw.strip():
                continue
            parsed.append(self._parse_line(bytes(raw)))
        return parsed

    def _parse_line(self, raw: bytes) -> dict[str, Any]:
        if self._ended:
            raise TypedFrameError("FRAME_AFTER_END", "typed frame was emitted after end")
        if len(raw) > MAX_FRAME_BYTES:
            raise TypedFrameError("FRAME_TOO_LARGE", "typed frame exceeds its line limit")
        if len(self._frames) >= MAX_FRAMES:
            raise TypedFrameError("FRAME_COUNT_EXCEEDED", "typed frame count exceeds its limit")
        try:
            decoded = raw.decode("utf-8")
        except UnicodeDecodeError as error:
            raise TypedFrameError("FRAME_UTF8_INVALID", "typed frame is not valid UTF-8") from error
        try:
            value = json.loads(decoded)
        except json.JSONDecodeError as error:
            raise TypedFrameError("FRAME_JSON_INVALID", "typed frame is not valid JSON", retryable=True) from error
        frame = validate_frame(value, len(self._frames) + 1)
        self._frames.append(frame)
        if frame["type"] == "end":
            self._ended = True
        return frame

    def finish(self) -> tuple[Mapping[str, Any], ...]:
        if self._cancelled:
            raise TypedFrameError("FRAME_CANCELLED", "typed frame parsing was cancelled", retryable=True)
        if self._buffer.strip():
            raise TypedFrameError("FRAME_TRUNCATED", "typed frame stream ended with a partial line", retryable=True)
        if not self._ended:
            raise TypedFrameError("FRAME_END_MISSING", "typed frame stream ended before the end frame", retryable=True)
        return self.frames


@dataclass(frozen=True)
class GraphPatchDraft:
    title: str
    summary: str
    operations: tuple[Mapping[str, Any], ...]

    def to_wire(
        self,
        plugin_id: str,
        external_id: str,
        *,
        project_id: str | None = None,
    ) -> Mapping[str, Any]:
        source: dict[str, Any] = {
            "pluginId": plugin_id,
            "operation": "pdf-document-extraction",
            "externalId": external_id,
        }
        if project_id:
            source["projectId"] = project_id
        return {
            "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
            "source": source,
            "title": self.title,
            "summary": self.summary,
            "reviewRequired": True,
            "operations": [dict(operation) for operation in self.operations],
        }


def frames_to_graph_patch(frames: Iterable[Mapping[str, Any]], *, title: str) -> GraphPatchDraft:
    collected = list(frames)
    entities = [frame for frame in collected if frame.get("type") == "entity"]
    operators = [frame for frame in collected if frame.get("type") in {"operator", "relation"}]
    evidence = {str(frame["id"]): frame for frame in collected if frame.get("type") == "evidence"}
    end = next((frame for frame in reversed(collected) if frame.get("type") == "end"), None)
    if end is None:
        raise TypedFrameError("FRAME_END_MISSING", "cannot build a patch without an end frame")
    operations: list[Mapping[str, Any]] = []
    for entity in entities:
        missing_evidence = [item for item in entity.get("evidenceIds", []) if item not in evidence]
        if missing_evidence:
            raise TypedFrameError("FRAME_REFERENCE_INVALID", "entity references unknown evidence")
        evidence_items = [evidence[item] for item in entity.get("evidenceIds", []) if item in evidence]
        operations.append({
            "op": "add-node",
            "node": {
                "id": entity["id"],
                "type": entity["entityType"],
                "title": entity["title"],
                "body": entity.get("summary", ""),
                "tags": [],
                "data": {"evidence": evidence_items},
            },
        })
    entity_ids = {str(entity["id"]) for entity in entities}
    for operator in operators:
        if operator["sourceId"] not in entity_ids or operator["targetId"] not in entity_ids:
            raise TypedFrameError("FRAME_REFERENCE_INVALID", "operator references an unknown entity")
        if any(item not in evidence for item in operator.get("evidenceIds", [])):
            raise TypedFrameError("FRAME_REFERENCE_INVALID", "operator references unknown evidence")
        operations.append({
            "op": "add-edge",
            "edge": {
                "id": operator["id"],
                "source": operator["sourceId"],
                "target": operator["targetId"],
                "type": operator.get("operatorType") or operator.get("relationType"),
                "note": operator["label"],
                "data": {"evidenceIds": list(operator.get("evidenceIds", []))},
            },
        })
    if not operations:
        raise TypedFrameError("PATCH_EMPTY", "model output did not contain reviewable graph operations")
    if len(operations) > MAX_PATCH_OPERATIONS:
        raise TypedFrameError("PATCH_OPERATION_LIMIT", "model output exceeds the review patch operation limit")
    draft = GraphPatchDraft(title=title[:500], summary=str(end["summary"])[:2_000], operations=tuple(operations))
    probe = draft.to_wire(
        "plugin.probe",
        "probe",
        project_id="probe-project",
    )
    if len(json.dumps(probe, separators=(",", ":"), ensure_ascii=False).encode("utf-8")) > MAX_PATCH_BYTES:
        raise TypedFrameError("PATCH_INLINE_TOO_LARGE", "review patch exceeds the bounded inline proposal limit")
    return draft
