"""Bounded, dependency-free PDF ingestion for the external worker.

This is intentionally a first migration slice, not a full PDF engine. It
validates the container, reads Host Blob chunks, extracts literal text from
plain and Flate streams, and fails explicitly when OCR or a richer parser is
required. No path, credential, network client, or Host implementation lives
here.
"""

from __future__ import annotations

import base64
import hashlib
import re
import zlib
from dataclasses import dataclass
from typing import Any, Mapping

from research_canvas import BlobRef, HostBusClient, RemoteWorkerError


BLOB_CHUNK_BYTES = 16 * 1024
MAX_BLOB_CHUNKS = 24
MAX_PDF_BYTES = BLOB_CHUNK_BYTES * MAX_BLOB_CHUNKS
MAX_TEXT_BYTES = 64 * 1024
MAX_STREAM_BYTES = 256 * 1024


class PdfReadError(RemoteWorkerError):
    def __init__(self, code: str, message: str, *, retryable: bool = False):
        super().__init__(code, message)
        self.retryable = retryable


@dataclass(frozen=True)
class ParsedPdf:
    page_count: int
    text: str
    text_bytes: int
    strategy: str


def read_host_blob(host: HostBusClient, blob: BlobRef) -> bytes:
    """Read and re-hash one bounded Host BlobRef through reverse RPC."""

    if blob.size > MAX_PDF_BYTES:
        raise PdfReadError(
            "PDF_TOO_LARGE",
            f"PDF exceeds the 384 KiB ({MAX_PDF_BYTES}-byte) bounded parser acceptance slice.",
        )
    chunks: list[bytes] = []
    offset = 0
    while offset < blob.size:
        response = host.call(
            "blob.read",
            {"ref": blob.to_mapping(), "offset": offset, "maxBytes": BLOB_CHUNK_BYTES},
        )
        if not isinstance(response, Mapping):
            raise PdfReadError("BLOB_READ_INVALID", "Host returned an invalid Blob read response.", retryable=True)
        if response.get("digest") != blob.digest or response.get("size") != blob.size:
            raise PdfReadError("BLOB_READ_MISMATCH", "Host Blob metadata did not match the selected reference.")
        if response.get("offset") != offset:
            raise PdfReadError("BLOB_READ_MISMATCH", "Host Blob chunk offset did not match the request.")
        encoded = response.get("contentBase64")
        if not isinstance(encoded, str):
            raise PdfReadError("BLOB_READ_INVALID", "Host Blob chunk did not contain bytes.", retryable=True)
        try:
            chunk = base64.b64decode(encoded, validate=True)
        except (ValueError, TypeError) as error:
            raise PdfReadError("BLOB_READ_INVALID", "Host Blob chunk was not valid base64.", retryable=True) from error
        next_offset = response.get("nextOffset")
        if not isinstance(next_offset, int) or next_offset != offset + len(chunk):
            raise PdfReadError("BLOB_READ_MISMATCH", "Host Blob chunk did not advance monotonically.")
        if not chunk and next_offset < blob.size:
            raise PdfReadError("BLOB_READ_TRUNCATED", "Host Blob ended before its declared size.", retryable=True)
        chunks.append(chunk)
        offset = next_offset
        if len(chunks) > MAX_BLOB_CHUNKS:
            raise PdfReadError("PDF_TOO_LARGE", "PDF requires more Blob chunks than this parser slice permits.")
    content = b"".join(chunks)
    if len(content) != blob.size or hashlib.sha256(content).hexdigest() != blob.digest:
        raise PdfReadError("BLOB_READ_MISMATCH", "Host Blob bytes failed size or digest verification.")
    return content


def parse_pdf(content: bytes) -> ParsedPdf:
    if not content.startswith(b"%PDF-"):
        raise PdfReadError("INVALID_PDF_MAGIC", "Selected file does not start with a PDF header.")
    if b"%%EOF" not in content[-4096:]:
        raise PdfReadError("TRUNCATED_PDF", "PDF end marker is missing; the file may be truncated.", retryable=True)
    page_count = len(re.findall(rb"/Type\s*/Page\b", content))
    if page_count == 0:
        raise PdfReadError("PDF_PAGE_TREE_MISSING", "PDF has no detectable page objects.")

    candidates = [content]
    for match in re.finditer(rb"stream\r?\n(.*?)\r?\nendstream", content, re.DOTALL):
        stream = match.group(1)
        try:
            inflater = zlib.decompressobj()
            decoded = inflater.decompress(stream, MAX_STREAM_BYTES + 1)
            if len(decoded) <= MAX_STREAM_BYTES:
                candidates.append(decoded)
        except zlib.error:
            continue

    pieces: list[str] = []
    total = 0
    for candidate in candidates:
        for token in re.finditer(rb"\((?:\\.|[^\\)]){1,4096}\)", candidate):
            decoded = _decode_pdf_literal(token.group(0)[1:-1])
            if not decoded:
                continue
            encoded_size = len(decoded.encode("utf-8"))
            if total + encoded_size > MAX_TEXT_BYTES:
                break
            pieces.append(decoded)
            total += encoded_size
        if total >= MAX_TEXT_BYTES:
            break
    text = " ".join(" ".join(pieces).split())
    if not text:
        raise PdfReadError(
            "OCR_REQUIRED",
            "PDF contains pages but no supported text layer; OCR is required and is not implemented in this Python slice.",
        )
    return ParsedPdf(
        page_count=page_count,
        text=text,
        text_bytes=len(text.encode("utf-8")),
        strategy="literal-text+flate",
    )


def read_and_parse_pdf(host: HostBusClient, blob: BlobRef) -> ParsedPdf:
    return parse_pdf(read_host_blob(host, blob))


def read_and_parse_pdf_bytes(host: HostBusClient, blob: BlobRef) -> tuple[bytes, ParsedPdf]:
    """Read a Blob once so direct provider upload never repeats Host calls."""
    content = read_host_blob(host, blob)
    return content, parse_pdf(content)


def _decode_pdf_literal(value: bytes) -> str:
    value = re.sub(rb"\\[nrtbf]", lambda match: {
        b"\\n": b"\n",
        b"\\r": b"\r",
        b"\\t": b"\t",
        b"\\b": b"\b",
        b"\\f": b"\f",
    }[match.group(0)], value)
    value = value.replace(b"\\(", b"(").replace(b"\\)", b")").replace(b"\\\\", b"\\")
    try:
        return value.decode("utf-8").strip()
    except UnicodeDecodeError:
        return value.decode("latin-1", errors="ignore").strip()
