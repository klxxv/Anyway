"""Plugin-owned PDF parsing and direct Kimi streaming worker.

Rust owns process identity, worker lifecycle, BlobRef authorization and the
GraphPatch kernel. This worker only performs plugin business operations: read
an authorized PDF BlobRef, call the configured provider directly, parse small
typed NDJSON frames, and return a bounded GraphPatch draft to the plugin
frontend. The frontend then submits the draft through ``context.graphPatch``.
"""

from __future__ import annotations

import os
from pathlib import Path
import sys
import time
from typing import Any, Mapping

PACKAGE_SRC = Path(__file__).resolve().parents[1]
if str(PACKAGE_SRC) not in sys.path:
    sys.path.insert(0, str(PACKAGE_SRC))

try:
    from research_canvas import BlobRef, HostBusClient, RemoteWorkerError, WorkerRuntime
except ModuleNotFoundError:
    # Development-only fallback. The packer vendors this module as
    # src/research_canvas.py, so installed .myc execution never reaches here.
    DEV_SDK_ROOT = Path(__file__).resolve().parents[4] / "plugins" / "sdk" / "python"
    if str(DEV_SDK_ROOT) not in sys.path:
        sys.path.insert(0, str(DEV_SDK_ROOT))
    from research_canvas import BlobRef, HostBusClient, RemoteWorkerError, WorkerRuntime

from anpdfsolver.kimi_client import KimiClient, KimiClientError, KimiConfig
from anpdfsolver.pdf_reader import PdfReadError, parse_pdf, read_host_blob
from anpdfsolver.typed_frames import TypedFrameError, frames_to_graph_patch


PLUGIN_ID = "myc.pdf-canvas-agent"
MAX_RETURNED_FRAMES = 160


def ping(payload: Mapping[str, Any]) -> Mapping[str, Any]:
    return {"pong": payload.get("value", "pong")}


def health(payload: Mapping[str, Any]) -> Mapping[str, Any]:
    delay_ms = int(payload.get("delayMs", 0))
    if delay_ms > 0:
        time.sleep(delay_ms / 1000)
    return {
        "healthy": True,
        "worker": "anPdfsolver",
        "processId": os.getpid(),
        "providerSecretPresent": bool(os.environ.get("ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY")),
        "operations": sorted(OPERATIONS),
    }


def analyze_pdf(payload: Mapping[str, Any], host: HostBusClient) -> Mapping[str, Any]:
    analysis_session_id = _bounded_text(payload.get("analysisSessionId"), "analysisSessionId", 160)
    request_id = _bounded_text(payload.get("requestId"), "requestId", 220)
    job_id = _bounded_text(payload.get("jobId"), "jobId", 160)
    project_id = _bounded_text(payload.get("projectId"), "projectId", 160)
    base_revision = _bounded_revision(payload.get("baseRevision"))
    file_info = payload.get("file")
    if not isinstance(file_info, Mapping):
        raise RemoteWorkerError("FILE_MISSING", "anpdfsolver.analyze requires a file object.")
    label = _bounded_text(file_info.get("label", "document.pdf"), "file.label", 256)
    blob = _blob_ref(file_info.get("blobRef"))
    runtime_config = payload.get("runtimeConfig")
    if not isinstance(runtime_config, Mapping):
        runtime_config = {}

    progress: list[Mapping[str, Any]] = []
    model_frames: list[Mapping[str, Any]] = []

    def record(stage: str, message: str, percent: int) -> None:
        progress.append({
            "stage": stage,
            "message": message[:512],
            "percent": max(0, min(100, percent)),
            "createdAt": int(time.time() * 1000),
        })

    record("file.reading", "Reading the Host-authorized PDF BlobRef.", 5)
    try:
        content = read_host_blob(host, blob)
        try:
            parsed = parse_pdf(content)
            local_text = parsed.text
            page_count = parsed.page_count
            text_bytes = parsed.text_bytes
            record("pdf.parsed", f"Parsed {page_count} page(s) locally.", 30)
        except PdfReadError as error:
            if error.code != "OCR_REQUIRED":
                raise
            config = KimiConfig.from_runtime_config(runtime_config)
            if config.pdf_transport != "kimi-file-extract":
                raise
            local_text = ""
            page_count = 0
            text_bytes = 0
            record("pdf.remote-extraction", "Uploading to Kimi Files for text extraction.", 30)

        config = KimiConfig.from_runtime_config(runtime_config)
        record("provider.streaming", "Streaming typed extraction frames from the configured provider.", 45)

        def on_frame(frame: Mapping[str, Any]) -> None:
            public_frame = _public_frame(frame)
            if len(model_frames) < MAX_RETURNED_FRAMES:
                model_frames.append(public_frame)
            if frame.get("type") in {"progress", "warning", "error"}:
                record(f"model.{frame.get('type')}", str(frame.get("message", frame.get("code", "model frame"))), 60)

        frames = KimiClient(config).analyze(
            pdf_bytes=content,
            label=label,
            local_text=local_text,
            deadline=host.deadline,
            on_frame=on_frame,
        )
        draft = frames_to_graph_patch(frames, title=f"Review extraction: {label}")
        draft_patch = draft.to_wire(
            PLUGIN_ID,
            request_id,
            project_id=project_id,
        )
        record("patch.ready", "Built a bounded GraphPatch draft for Rust canonical review.", 85)
    except PdfReadError as error:
        raise RemoteWorkerError(error.code, error.message) from error
    except (KimiClientError, TypedFrameError) as error:
        code = getattr(error, "code", "ANALYSIS_FAILED")
        message = getattr(error, "message", "The PDF analysis failed.")
        raise RemoteWorkerError(str(code), str(message)) from error

    return {
        "analysisSessionId": analysis_session_id,
        "requestId": request_id,
        "jobId": job_id,
        "file": {
            "label": label,
            "digest": blob.digest,
            "size": blob.size,
            "mediaType": blob.media_type,
            "pageCount": page_count,
            "textBytes": text_bytes,
        },
        "progress": progress[-64:],
        "frames": model_frames,
        "frameCount": len(frames),
        "draftPatch": draft_patch,
        "summary": draft.summary,
    }


def _blob_ref(value: Any) -> BlobRef:
    if not isinstance(value, Mapping):
        raise RemoteWorkerError("BLOB_REF_MISSING", "file.blobRef is required.")
    try:
        return BlobRef.from_mapping(value)
    except ValueError as error:
        raise RemoteWorkerError("BLOB_REF_INVALID", str(error)) from error


def _bounded_text(value: Any, field: str, limit: int) -> str:
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > limit:
        raise RemoteWorkerError("PAYLOAD_INVALID", f"{field} is invalid.")
    return value


def _bounded_revision(value: Any) -> int:
    if isinstance(value, bool) or not isinstance(value, int) or value < 0:
        raise RemoteWorkerError("PAYLOAD_INVALID", "baseRevision is invalid.")
    return value


def _public_frame(frame: Mapping[str, Any]) -> Mapping[str, Any]:
    allowed = {
        "type", "seq", "stage", "message", "percent", "id", "quote", "page",
        "section", "entityType", "title", "summary", "evidenceIds",
        "operatorType", "relationType", "sourceId", "targetId", "label",
        "code", "retryable", "status",
    }
    return {key: value for key, value in frame.items() if key in allowed}


OPERATIONS = {
    "ping": ping,
    "health": health,
    "anpdfsolver.analyze": analyze_pdf,
}


def main() -> None:
    WorkerRuntime(OPERATIONS).serve()


if __name__ == "__main__":
    main()
