import base64
import hashlib
import io
import json
from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch


REPOSITORY = Path(__file__).resolve().parents[3]
for directory in (
    Path(__file__).resolve().parent,
    REPOSITORY / "my-plugins" / "anPdfsolver" / "src",
):
    if str(directory) not in sys.path:
        sys.path.insert(0, str(directory))

from research_canvas import BlobRef, RemoteWorkerError, WORKER_RPC_API_VERSION, WorkerRuntime, encode_frame, read_frame
from anpdfsolver.pdf_reader import MAX_PDF_BYTES, PdfReadError, parse_pdf, read_host_blob
from anpdfsolver import worker as worker_module
from anpdfsolver.worker import OPERATIONS, analyze_pdf


def minimal_pdf(text: str | None = "Visible text") -> bytes:
    operation = f"BT ({text}) Tj ET" if text is not None else "q 1 0 0 1 0 0 cm Q"
    return (
        "%PDF-1.4\n"
        "1 0 obj << /Type /Page >>\n"
        "stream\n"
        f"{operation}\n"
        "endstream\n"
        "endobj\n"
        "%%EOF"
    ).encode("utf-8")


class FakeHost:
    def __init__(self, blobs: dict[str, bytes]):
        self.blobs = blobs
        self.events: list[dict[str, object]] = []
        self.calls = 0
        self.deadline = time.monotonic() + 60

    def call(self, operation: str, payload: dict[str, object]):
        self.calls += 1
        if operation != "blob.read":
            raise AssertionError(f"unexpected Host operation: {operation}")
        reference = payload["ref"]
        assert isinstance(reference, dict)
        content = self.blobs[reference["digest"]]
        offset = int(payload.get("offset", 0))
        limit = int(payload.get("maxBytes", 16 * 1024))
        chunk = content[offset : offset + limit]
        next_offset = offset + len(chunk)
        return {
            "digest": reference["digest"],
            "size": len(content),
            "mediaType": reference["mediaType"],
            "offset": offset,
            "nextOffset": next_offset,
            "eof": next_offset >= len(content),
            "contentBase64": base64.b64encode(chunk).decode("ascii"),
        }


RUNTIME_CONFIG = {
    "providers": [{
        "id": "kimi",
        "baseUrl": "https://api.moonshot.cn/v1",
        "format": "openai",
        "model": "kimi-k2.6",
        "pdfTransport": "local-text",
        "thinking": "disabled",
        "allowedDomains": ["api.moonshot.cn", "api.moonshot.ai"],
        "secretEnv": "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY",
    }],
}


class FakeKimiClient:
    def __init__(self, config):
        self.config = config

    def analyze(self, **kwargs):
        frames = (
            {"type": "evidence", "seq": 1, "id": "e1", "quote": "Public evidence", "page": 1},
            {"type": "entity", "seq": 2, "id": "n1", "entityType": "concept", "title": "Grounded concept", "evidenceIds": ["e1"]},
            {"type": "end", "seq": 3, "status": "review-required", "summary": "Review one grounded concept"},
        )
        for item in frames:
            callback = kwargs.get("on_frame")
            if callback:
                callback(item)
        return frames


class UnknownEvidenceKimiClient:
    def __init__(self, config):
        self.config = config

    def analyze(self, **kwargs):
        return (
            {"type": "entity", "seq": 1, "id": "n1", "entityType": "concept", "title": "Ungrounded", "evidenceIds": ["missing"]},
            {"type": "end", "seq": 2, "status": "review-required", "summary": "bad"},
        )


class CorruptChunkHost(FakeHost):
    def __init__(self, blobs: dict[str, bytes], corruption: str):
        super().__init__(blobs)
        self.corruption = corruption

    def call(self, operation: str, payload: dict[str, object]):
        response = super().call(operation, payload)
        if operation == "blob.read":
            if self.corruption == "offset":
                response["offset"] = int(response["offset"]) + 1
            elif self.corruption == "digest":
                response["digest"] = "0" * 64
        return response


def blob_ref(content: bytes, suffix: str) -> BlobRef:
    owner = f"plugin.test@1.0.0#{suffix}"
    return BlobRef(
        "sha256",
        hashlib.sha256(content).hexdigest(),
        len(content),
        "application/pdf",
        f"private:{owner}",
        owner,
        "session",
    )


class PdfReaderTests(unittest.TestCase):
    def test_magic_page_and_literal_text_strategy(self):
        parsed = parse_pdf(minimal_pdf("A bounded PDF text layer"))
        self.assertEqual(parsed.page_count, 1)
        self.assertIn("bounded PDF text layer", parsed.text)
        self.assertEqual(parsed.strategy, "literal-text+flate")
        for content, code in [
            (b"not a pdf", "INVALID_PDF_MAGIC"),
            (b"%PDF-1.4\n/Type /Page", "TRUNCATED_PDF"),
            (minimal_pdf(None), "OCR_REQUIRED"),
        ]:
            with self.assertRaises(PdfReadError) as error:
                parse_pdf(content)
            self.assertEqual(error.exception.code, code)

    def test_blob_reader_uses_bounded_chunks_and_verifies_digest(self):
        content = minimal_pdf("x" * (20 * 1024))
        reference = blob_ref(content, "chunked")
        host = FakeHost({reference.digest: content})
        self.assertEqual(read_host_blob(host, reference), content)
        self.assertGreater(host.calls, 1)

        for corruption in ("offset", "digest"):
            with self.subTest(corruption=corruption):
                bad_host = CorruptChunkHost({reference.digest: content}, corruption)
                with self.assertRaises(PdfReadError) as error:
                    read_host_blob(bad_host, reference)
                self.assertEqual(error.exception.code, "BLOB_READ_MISMATCH")

    def test_blob_reader_rejects_the_acceptance_slice_limit_before_host_io(self):
        owner = "plugin.test@1.0.0#oversized"
        oversized = BlobRef("sha256", "a" * 64, MAX_PDF_BYTES + 1, "application/pdf", f"private:{owner}", owner, "session")
        host = FakeHost({})
        with self.assertRaises(PdfReadError) as error:
            read_host_blob(host, oversized)
        self.assertEqual(error.exception.code, "PDF_TOO_LARGE")
        self.assertIn("384", error.exception.message)
        self.assertEqual(host.calls, 0)

    def test_worker_analyze_reads_blob_and_returns_public_draft_patch(self):
        content = minimal_pdf("Worker analyze")
        reference = blob_ref(content, "analyze")
        host = FakeHost({reference.digest: content})
        payload = {
            "analysisSessionId": "analysis-1",
            "requestId": "request-1",
            "jobId": "job-1",
            "projectId": "project-1",
            "baseRevision": 7,
            "runtimeConfig": RUNTIME_CONFIG,
            "file": {"label": "paper.pdf", "blobRef": reference.to_mapping()},
        }
        with patch("anpdfsolver.worker.KimiClient", FakeKimiClient):
            result = analyze_pdf(payload, host)

        self.assertEqual(result["analysisSessionId"], "analysis-1")
        self.assertEqual(result["requestId"], "request-1")
        self.assertEqual(result["jobId"], "job-1")
        self.assertEqual(result["file"]["digest"], reference.digest)
        self.assertEqual(result["file"]["pageCount"], 1)
        self.assertEqual(result["frameCount"], 3)
        self.assertLessEqual(len(result["frames"]), 160)
        patch_value = result["draftPatch"]
        self.assertTrue(patch_value["reviewRequired"])
        self.assertEqual(len(patch_value["operations"]), 1)
        self.assertEqual(
            patch_value["source"],
            {
                "pluginId": "myc.pdf-canvas-agent",
                "operation": "pdf-document-extraction",
                "externalId": "request-1",
                "projectId": "project-1",
            },
        )
        for rust_envelope_field in ("pluginVersion", "sessionId", "baseRevision"):
            self.assertNotIn(rust_envelope_field, patch_value["source"])
        public_wire = json.dumps(result, sort_keys=True)
        self.assertNotIn("%PDF", public_wire)
        self.assertNotIn("Worker analyze", public_wire)
        self.assertNotIn("private:", public_wire)
        self.assertNotIn("path", public_wire.lower())
        self.assertLessEqual(host.calls, 24)

    def test_worker_analyze_maps_pdf_and_typed_frame_errors_to_remote_errors(self):
        unsupported = minimal_pdf(None)
        unsupported_ref = blob_ref(unsupported, "unsupported")
        payload = {
            "analysisSessionId": "analysis-err",
            "requestId": "request-err",
            "jobId": "job-err",
            "projectId": "project-1",
            "baseRevision": 7,
            "runtimeConfig": RUNTIME_CONFIG,
            "file": {"label": "scan.pdf", "blobRef": unsupported_ref.to_mapping()},
        }
        with self.assertRaises(RemoteWorkerError) as pdf_error:
            analyze_pdf(payload, FakeHost({unsupported_ref.digest: unsupported}))
        self.assertEqual(pdf_error.exception.code, "OCR_REQUIRED")

        content = minimal_pdf("bad evidence")
        reference = blob_ref(content, "bad-evidence")
        payload = dict(payload, file={"label": "bad.pdf", "blobRef": reference.to_mapping()})
        with patch("anpdfsolver.worker.KimiClient", UnknownEvidenceKimiClient):
            with self.assertRaises(RemoteWorkerError) as frame_error:
                analyze_pdf(payload, FakeHost({reference.digest: content}))
        self.assertEqual(frame_error.exception.code, "FRAME_REFERENCE_INVALID")

    def test_maximum_acceptance_pdf_keeps_reverse_calls_below_protocol_limit(self):
        prefix = minimal_pdf("Bounded call budget").removesuffix(b"%%EOF")
        content = prefix + (b" " * (MAX_PDF_BYTES - len(prefix) - len(b"%%EOF"))) + b"%%EOF"
        reference = blob_ref(content, "call-budget")
        host = FakeHost({reference.digest: content})
        payload = {
            "analysisSessionId": "analysis-budget",
            "requestId": "request-budget",
            "jobId": "job-budget",
            "projectId": "project-1",
            "baseRevision": 1,
            "runtimeConfig": RUNTIME_CONFIG,
            "file": {"label": "limit.pdf", "blobRef": reference.to_mapping()},
        }
        with patch("anpdfsolver.worker.KimiClient", FakeKimiClient):
            result = analyze_pdf(payload, host)
        self.assertEqual(result["draftPatch"]["operations"][0]["op"], "add-node")
        self.assertLessEqual(host.calls, 24)

    def test_worker_runtime_framed_hello_request_and_allowlist(self):
        reader = io.BytesIO()
        writer = io.BytesIO()
        reader.write(encode_frame({
            "type": "hello",
            "apiVersion": WORKER_RPC_API_VERSION,
            "pluginId": "myc.pdf-canvas-agent",
            "pluginVersion": "0.4.0",
            "workerId": "anpdfsolver",
            "allowedOperations": ["ping"],
        }))
        reader.write(encode_frame({
            "type": "request",
            "apiVersion": WORKER_RPC_API_VERSION,
            "requestId": "req-1",
            "operation": "ping",
            "payload": {"value": "ok"},
            "deadlineMs": 1000,
        }))
        reader.write(encode_frame({"type": "shutdown", "apiVersion": WORKER_RPC_API_VERSION}))
        reader.seek(0)
        WorkerRuntime(OPERATIONS).serve(reader, writer)
        writer.seek(0)
        ack = read_frame(writer)
        response = read_frame(writer)
        stopped = read_frame(writer)
        self.assertEqual(ack["type"], "helloAck")
        self.assertEqual(ack["operations"], ["ping"])
        self.assertEqual(response["result"], {"pong": "ok"})
        self.assertEqual(stopped["result"], {"stopped": True})

        denied_reader = io.BytesIO()
        denied_writer = io.BytesIO()
        denied_reader.write(encode_frame({
            "type": "hello",
            "apiVersion": WORKER_RPC_API_VERSION,
            "pluginId": "myc.pdf-canvas-agent",
            "pluginVersion": "0.4.0",
            "workerId": "anpdfsolver",
            "allowedOperations": ["ping"],
        }))
        denied_reader.write(encode_frame({
            "type": "request",
            "apiVersion": WORKER_RPC_API_VERSION,
            "requestId": "req-2",
            "operation": "anpdfsolver.analyze",
            "payload": {},
            "deadlineMs": 1000,
        }))
        denied_reader.write(encode_frame({"type": "shutdown", "apiVersion": WORKER_RPC_API_VERSION}))
        denied_reader.seek(0)
        WorkerRuntime(OPERATIONS).serve(denied_reader, denied_writer)
        denied_writer.seek(0)
        self.assertEqual(read_frame(denied_writer)["operations"], ["ping"])
        denied = read_frame(denied_writer)
        self.assertFalse(denied["ok"])
        self.assertEqual(denied["error"]["code"], "NOT_ALLOWED")

    def test_worker_static_contract_contains_new_operation_and_no_surface_runtime(self):
        source = Path(worker_module.__file__).read_text(encoding="utf-8")
        self.assertIn("anpdfsolver.analyze", OPERATIONS)
        self.assertIn("WorkerRuntime(OPERATIONS).serve()", source)
        self.assertNotIn("SurfaceRuntime", source)
        self.assertNotIn("surface.", source)


if __name__ == "__main__":
    unittest.main()
