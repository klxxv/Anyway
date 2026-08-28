import io
import json
import os
from pathlib import Path
import sys
import time
import unittest


REPOSITORY = Path(__file__).resolve().parents[3]
for directory in (
    Path(__file__).resolve().parent,
    REPOSITORY / "my-plugins" / "anPdfsolver" / "src",
):
    if str(directory) not in sys.path:
        sys.path.insert(0, str(directory))

from anpdfsolver.kimi_client import HttpResponse, KimiClient, KimiClientError, KimiConfig, ProviderSecret, SECRET_ENV
from anpdfsolver.typed_frames import (
    MAX_FRAME_BYTES,
    TypedFrameError,
    TypedFrameParser,
    frames_to_graph_patch,
    layered_system_prompt,
    rendered_system_prompt,
)


def frame(value: dict[str, object]) -> str:
    return json.dumps(value, separators=(",", ":")) + "\n"


def valid_ndjson() -> str:
    return "".join([
        frame({"type": "progress", "seq": 1, "stage": "extract", "message": "Reading evidence", "percent": 25}),
        frame({"type": "evidence", "seq": 2, "id": "e1", "quote": "Grounded quote", "page": 1}),
        frame({"type": "entity", "seq": 3, "id": "n1", "entityType": "concept", "title": "Input", "evidenceIds": ["e1"]}),
        frame({"type": "entity", "seq": 4, "id": "n2", "entityType": "outcome", "title": "Output", "evidenceIds": ["e1"]}),
        frame({"type": "operator", "seq": 5, "id": "edge1", "operatorType": "influences", "sourceId": "n1", "targetId": "n2", "label": "influences", "evidenceIds": ["e1"]}),
        frame({"type": "end", "seq": 6, "status": "review-required", "summary": "Review two entities and one edge"}),
    ])


class ChunkBody:
    def __init__(self, chunks: list[bytes]):
        self.chunks = list(chunks)
        self.closed = False

    def read(self, size: int = -1) -> bytes:
        if not self.chunks:
            return b""
        return self.chunks.pop(0)

    def close(self):
        self.closed = True


class FakeTransport:
    def __init__(self, responses: list[HttpResponse]):
        self.responses = list(responses)
        self.requests: list[dict[str, object]] = []

    def request(self, method, url, headers, body, timeout):
        self.requests.append({"method": method, "url": url, "headers": dict(headers), "body": body, "timeout": timeout})
        if not self.responses:
            raise AssertionError("unexpected provider request")
        return self.responses.pop(0)


def config(
    api_format: str = "openai",
    pdf_transport: str = "local-text",
    public_progress: str = "disabled",
) -> KimiConfig:
    suffix = "v1" if api_format == "openai" else "anthropic"
    return KimiConfig(
        base_url=f"https://api.moonshot.cn/{suffix}",
        api_format=api_format,
        model="kimi-k2.6",
        pdf_transport=pdf_transport,
        thinking="enabled",
        allowed_domains=("api.moonshot.cn", "api.moonshot.ai"),
        public_progress=public_progress,
    )


def openai_sse(content: str, *, split: int | None = None) -> list[bytes]:
    midpoint = split or max(1, len(content) // 2)
    values = [
        {"choices": [{"delta": {"reasoning_content": "private reasoning must disappear"}}]},
        {"choices": [{"delta": {"content": content[:midpoint]}}]},
        {"choices": [{"delta": {"content": content[midpoint:]}}]},
    ]
    wire = b"".join(f"data: {json.dumps(value)}\n\n".encode() for value in values) + b"data: [DONE]\n\n"
    return [wire[:37], wire[37:101], wire[101:]]


def anthropic_sse(content: str) -> list[bytes]:
    midpoint = max(1, len(content) // 2)
    events = [
        ("content_block_delta", {"type": "content_block_delta", "delta": {"type": "thinking_delta", "thinking": "private"}}),
        ("content_block_delta", {"type": "content_block_delta", "delta": {"type": "text_delta", "text": content[:midpoint]}}),
        ("content_block_delta", {"type": "content_block_delta", "delta": {"type": "text_delta", "text": content[midpoint:]}}),
        ("message_stop", {"type": "message_stop"}),
    ]
    wire = b"".join(f"event: {name}\ndata: {json.dumps(value)}\n\n".encode() for name, value in events)
    return [wire[:19], wire[19:73], wire[73:]]


class TypedFrameTests(unittest.TestCase):
    def test_layered_prompt_and_partial_frames_produce_review_patch(self):
        self.assertEqual(
            [item["segment"] for item in layered_system_prompt()],
            ["role", "instruction", "constraints", "frame-schema", "graph-semantics", "termination"],
        )
        prompt = rendered_system_prompt()
        self.assertIn("NDJSON only", prompt)
        self.assertIn("No markdown fences and no aggregate JSON document", prompt)
        self.assertIn("Every entity and relation must cite existing evidenceIds", prompt)
        self.assertIn("Emit exactly one end frame last", prompt)
        for forbidden in ("reasoning_content", "Authorization", "api_key", "C:\\", "/Users/"):
            self.assertNotIn(forbidden, prompt)
        parser = TypedFrameParser()
        wire = valid_ndjson().encode()
        parsed = []
        for chunk in (wire[:7], wire[7:41], wire[41:173], wire[173:]):
            parsed.extend(parser.feed(chunk))
        self.assertEqual(len(parser.finish()), 6)
        self.assertEqual(len(parsed), 6)
        self.assertLess(max(len(line.encode("utf-8")) for line in valid_ndjson().splitlines()), MAX_FRAME_BYTES)
        self.assertEqual(parsed[3]["entityType"], "result")
        self.assertEqual(parsed[4]["operatorType"], "K")
        patch = frames_to_graph_patch(parser.frames, title="Extraction")
        wire_patch = patch.to_wire("myc.pdf-canvas-agent", "job-1")
        self.assertEqual(len(wire_patch["operations"]), 3)
        self.assertTrue(wire_patch["reviewRequired"])
        self.assertEqual(
            wire_patch["source"],
            {
                "pluginId": "myc.pdf-canvas-agent",
                "operation": "pdf-document-extraction",
                "externalId": "job-1",
            },
        )
        for rust_envelope_field in ("pluginVersion", "sessionId", "baseRevision"):
            self.assertNotIn(rust_envelope_field, wire_patch["source"])

    def test_unknown_evidence_refs_aggregate_json_and_huge_frames_are_rejected(self):
        parser = TypedFrameParser()
        parser.feed(
            "".join([
                frame({"type": "entity", "seq": 1, "id": "n1", "entityType": "claim", "title": "Unsupported", "evidenceIds": ["missing"]}),
                frame({"type": "end", "seq": 2, "status": "review-required", "summary": "bad"}),
            ]).encode()
        )
        with self.assertRaises(TypedFrameError) as unknown:
            frames_to_graph_patch(parser.frames, title="Bad")
        self.assertEqual(unknown.exception.code, "FRAME_REFERENCE_INVALID")

        aggregate = TypedFrameParser()
        with self.assertRaises(TypedFrameError) as aggregate_error:
            aggregate.feed(b'[]\n')
        self.assertEqual(aggregate_error.exception.code, "FRAME_SCHEMA_INVALID")

        huge = TypedFrameParser()
        with self.assertRaises(TypedFrameError) as huge_error:
            huge.feed(b'{"type":"progress","seq":1,"stage":"' + b"x" * MAX_FRAME_BYTES)
        self.assertEqual(huge_error.exception.code, "FRAME_TOO_LARGE")

    def test_draft_patch_is_deterministic_and_keeps_rust_envelope_fields_out_of_source(self):
        parser_a = TypedFrameParser()
        parser_b = TypedFrameParser()
        parser_a.feed(valid_ndjson().encode())
        parser_b.feed(valid_ndjson().encode())
        draft_a = frames_to_graph_patch(parser_a.finish(), title="Extraction")
        draft_b = frames_to_graph_patch(parser_b.finish(), title="Extraction")
        wire_a = draft_a.to_wire("myc.pdf-canvas-agent", "request-7", project_id="project-1")
        wire_b = draft_b.to_wire("myc.pdf-canvas-agent", "request-7", project_id="project-1")
        self.assertEqual(
            json.dumps(wire_a, sort_keys=True, separators=(",", ":")),
            json.dumps(wire_b, sort_keys=True, separators=(",", ":")),
        )
        self.assertEqual(wire_a["source"]["projectId"], "project-1")
        self.assertEqual(set(wire_a["source"]), {"pluginId", "operation", "externalId", "projectId"})

    def test_bad_duplicate_truncated_cancelled_and_secret_frames_fail(self):
        cases = [
            (frame({"type": "progress", "seq": 2, "stage": "x", "message": "x", "percent": 1}).encode(), "FRAME_SEQUENCE_INVALID"),
            (b"{bad}\n", "FRAME_JSON_INVALID"),
            (frame({"type": "error", "seq": 1, "code": "E", "message": "x", "retryable": False, "authorization": "hidden"}).encode(), "FRAME_SECRET_FIELD"),
        ]
        for wire, code in cases:
            parser = TypedFrameParser()
            with self.subTest(code=code), self.assertRaises(TypedFrameError) as caught:
                parser.feed(wire)
            self.assertEqual(caught.exception.code, code)
        parser = TypedFrameParser()
        parser.feed(b'{"type":"end"')
        with self.assertRaises(TypedFrameError) as truncated:
            parser.finish()
        self.assertEqual(truncated.exception.code, "FRAME_TRUNCATED")
        cancelled = TypedFrameParser()
        cancelled.cancel()
        with self.assertRaises(TypedFrameError) as stopped:
            cancelled.feed(b"{}\n")
        self.assertEqual(stopped.exception.code, "FRAME_CANCELLED")

    def test_parser_instances_isolate_retry_and_batch_sequences(self):
        first = TypedFrameParser()
        with self.assertRaises(TypedFrameError):
            first.feed(b"{bad}\n")
        for _ in range(2):
            parser = TypedFrameParser()
            parser.feed(frame({"type": "end", "seq": 1, "status": "incomplete", "summary": "isolated"}).encode())
            self.assertEqual(parser.finish()[0]["seq"], 1)


class KimiClientTests(unittest.TestCase):
    def setUp(self):
        self.old_secret = os.environ.get(SECRET_ENV)
        os.environ[SECRET_ENV] = "test-provider-secret-never-print"

    def tearDown(self):
        if self.old_secret is None:
            os.environ.pop(SECRET_ENV, None)
        else:
            os.environ[SECRET_ENV] = self.old_secret

    def test_openai_and_anthropic_fragmented_sse_ignore_private_reasoning(self):
        for api_format, chunks in (("openai", openai_sse(valid_ndjson())), ("anthropic", anthropic_sse(valid_ndjson()))):
            transport = FakeTransport([HttpResponse(200, ChunkBody(chunks))])
            client = KimiClient(config(api_format), transport, sleeper=lambda _: None)
            frames = client.stream_frames("document text", time.monotonic() + 5, lambda: False, None)
            self.assertEqual(frames[-1]["type"], "end")
            request = transport.requests[0]
            body = json.loads(request["body"])
            self.assertTrue(body["stream"])
            self.assertNotIn("temperature", body)
            serialized = json.dumps(frames)
            self.assertNotIn("private", serialized)
            self.assertEqual(request["headers"]["Authorization"], "Bearer test-provider-secret-never-print")
            if api_format == "anthropic":
                self.assertEqual(request["headers"]["anthropic-version"], "2023-06-01")

    def test_public_progress_setting_changes_only_the_visible_prompt_policy(self):
        disabled = json.loads(KimiClient(config())._chat_body("document"))
        enabled = json.loads(
            KimiClient(config(public_progress="enabled"))._chat_body("document")
        )
        disabled_prompt = disabled["messages"][0]["content"]
        enabled_prompt = enabled["messages"][0]["content"]
        self.assertIn("Do not emit progress frames", disabled_prompt)
        self.assertIn("Emit short progress frames", enabled_prompt)
        for prompt in (disabled_prompt, enabled_prompt):
            self.assertNotIn("reasoning_content", prompt)
            self.assertNotIn("Authorization", prompt)

    def test_files_upload_content_chat_and_cleanup_are_regional_and_bounded(self):
        transport = FakeTransport([
            HttpResponse(200, io.BytesIO(b'{"id":"file-1"}')),
            HttpResponse(200, io.BytesIO(b"remote extracted text")),
            HttpResponse(200, ChunkBody(openai_sse(valid_ndjson()))),
            HttpResponse(200, io.BytesIO(b'{"deleted":true}')),
        ])
        client = KimiClient(config(pdf_transport="kimi-file-extract"), transport, sleeper=lambda _: None)
        frames = client.analyze(
            pdf_bytes=b"%PDF-1.4\n1 0 obj << /Type /Page >>\n%%EOF",
            label="paper.pdf",
            local_text="",
            deadline=time.monotonic() + 5,
        )
        self.assertEqual(frames[-1]["type"], "end")
        self.assertEqual([(item["method"], item["url"]) for item in transport.requests], [
            ("POST", "https://api.moonshot.cn/v1/files"),
            ("GET", "https://api.moonshot.cn/v1/files/file-1/content"),
            ("POST", "https://api.moonshot.cn/v1/chat/completions"),
            ("DELETE", "https://api.moonshot.cn/v1/files/file-1"),
        ])
        self.assertIn(b'name="purpose"\r\n\r\nfile-extract', transport.requests[0]["body"])

    def test_retry_uses_fresh_parser_and_cancel_deadline_stop_requests(self):
        sleeps = []
        transport = FakeTransport([
            HttpResponse(503, io.BytesIO(b"untrusted provider body")),
            HttpResponse(200, ChunkBody(openai_sse(valid_ndjson()))),
        ])
        client = KimiClient(config(), transport, sleeper=sleeps.append)
        frames = client.stream_frames("text", time.monotonic() + 5, lambda: False, None)
        self.assertEqual(frames[0]["seq"], 1)
        self.assertEqual(sleeps, [0.25])
        with self.assertRaises(KimiClientError) as cancelled:
            client.stream_frames("text", time.monotonic() + 5, lambda: True, None)
        self.assertEqual(cancelled.exception.code, "PROVIDER_CANCELLED")
        with self.assertRaises(KimiClientError) as deadline:
            client.stream_frames("text", time.monotonic() - 1, lambda: False, None)
        self.assertEqual(deadline.exception.code, "PROVIDER_DEADLINE")

    def test_runtime_config_domain_and_secret_representation_are_safe(self):
        runtime = {"providers": [{
            "id": "kimi", "baseUrl": "https://api.moonshot.ai/anthropic", "format": "anthropic",
            "model": "kimi-k2.6", "pdfTransport": "local-text", "thinking": "disabled",
            "publicProgress": "enabled",
            "allowedDomains": ["api.moonshot.cn", "api.moonshot.ai"], "secretEnv": SECRET_ENV,
        }]}
        parsed = KimiConfig.from_runtime_config(runtime)
        self.assertEqual(parsed.api_format, "anthropic")
        self.assertEqual(parsed.public_progress, "enabled")
        with self.assertRaises(KimiClientError):
            parsed.validate_url("https://evil.example/v1")
        rendered = repr(ProviderSecret("test-provider-secret-never-print"))
        self.assertNotIn("test-provider", rendered)
        self.assertNotIn("test-provider-secret-never-print", repr(parsed))

        invalid = dict(runtime)
        invalid["providers"] = [dict(runtime["providers"][0], baseUrl="https://api.moonshot.ai/custom")]
        with self.assertRaises(KimiClientError) as invalid_url:
            KimiConfig.from_runtime_config(invalid)
        self.assertEqual(invalid_url.exception.code, "PROVIDER_URL_INVALID")

    def test_files_rejects_provider_path_injection_and_retry_discards_partial_frames(self):
        bad_id = FakeTransport([HttpResponse(200, io.BytesIO(b'{"id":"../escape"}'))])
        with self.assertRaises(KimiClientError) as invalid_id:
            KimiClient(config(pdf_transport="kimi-file-extract"), bad_id).upload_pdf(
                b"%PDF-1.4\n%%EOF", "paper.pdf", time.monotonic() + 5, lambda: False,
            )
        self.assertEqual(invalid_id.exception.code, "PROVIDER_FILE_RESPONSE_INVALID")

        partial = frame({"type": "progress", "seq": 1, "stage": "partial", "message": "discard", "percent": 1})
        first_stream = b"data: " + json.dumps({"choices": [{"delta": {"content": partial}}]}).encode() + b"\n\n"
        transport = FakeTransport([
            HttpResponse(200, ChunkBody([first_stream])),
            HttpResponse(200, ChunkBody(openai_sse(valid_ndjson()))),
        ])
        frames = KimiClient(config(), transport, sleeper=lambda _: None).stream_frames(
            "text", time.monotonic() + 5, lambda: False, None,
        )
        self.assertEqual(frames[0]["stage"], "extract")
        self.assertNotIn("discard", json.dumps(frames))


if __name__ == "__main__":
    unittest.main()
