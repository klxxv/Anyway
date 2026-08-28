"""Dependency-free Python worker protocol SDK and manifest contract.

The worker transport uses framed UTF-8 JSON over stdin/stdout. stdout is
protocol-only; diagnostics belong on stderr. The Host remains authoritative for
plugin identity, capabilities, credentials, BlobRef access and Graph writes.
Python never acts as a Host; ``HostBusClient`` only sends correlated reverse
requests to the authoritative Rust Host over the worker's existing stdio link.
"""

from __future__ import annotations

import base64
import json
import inspect
import re
import select
import struct
import sys
import time
import uuid
from dataclasses import dataclass
from typing import Any, BinaryIO, Callable, Literal, Mapping, Protocol


GRAPH_PATCH_API_VERSION = "researchcanvas.dev/graph-patch/v1alpha1"
WORKER_RPC_API_VERSION = "researchcanvas.dev/worker-rpc/v1"
MAX_FRAME_BYTES = 1024 * 1024
MAX_INLINE_BYTES = 64 * 1024
MAX_BLOB_READ_CHUNK_BYTES = 256 * 1024
MAX_BLOB_READ_RESULT_BYTES = 384 * 1024
MAX_BLOB_READ_BASE64_BYTES = ((MAX_BLOB_READ_CHUNK_BYTES + 2) // 3) * 4
MAX_ERROR_MESSAGE_BYTES = 8 * 1024
MAX_EVENTS_PER_REQUEST = 64
MAX_HOST_CALLS_PER_REQUEST = 128
_BLOB_REF_KEYS = frozenset((
    "algorithm", "digest", "size", "mediaType", "scope", "owner", "retentionClass",
))
PluginSettingType = Literal["boolean", "number", "text", "select"]
PluginApiFormat = Literal["openai", "anthropic"]
PluginCredentialSource = Literal["host-secret", "environment"]
PluginConnectionTestActionKind = Literal["connection", "pdf-extraction"]


class WorkerRpcError(Exception):
    """Base error for the language-neutral worker transport."""


class FrameTooLargeError(WorkerRpcError):
    pass


class ProtocolError(WorkerRpcError):
    pass


class InlinePayloadTooLargeError(WorkerRpcError):
    pass


class WorkerTimeoutError(WorkerRpcError):
    pass


class RemoteWorkerError(WorkerRpcError):
    def __init__(self, code: str, message: str):
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message


@dataclass(frozen=True)
class BlobRef:
    """The Host BlobRef wire contract; never a local path or principal."""

    algorithm: str
    digest: str
    size: int
    media_type: str
    scope: str
    owner: str
    retention_class: Literal["request", "session", "plugin", "persistent"]

    def __post_init__(self) -> None:
        if self.algorithm != "sha256":
            raise ValueError("BlobRef.algorithm must be sha256")
        if isinstance(self.size, bool) or self.size < 0 or self.size > 1 << 40:
            raise ValueError("BlobRef.size is outside the bounded range")
        if not re.fullmatch(r"[0-9a-f]{64}", self.digest):
            raise ValueError("BlobRef.digest must be a lowercase 64-character hex digest")
        for name, value, limit in (
            ("mediaType", self.media_type, 256),
            ("scope", self.scope, 272),
            ("owner", self.owner, 256),
        ):
            if not isinstance(value, str) or not 0 < len(value) <= limit or any(ord(char) < 32 for char in value):
                raise ValueError(f"BlobRef.{name} is invalid")
        if self.scope != "shared":
            kind, separator, subject = self.scope.partition(":")
            if separator != ":" or kind not in {"private", "workspace"} or not subject or len(subject) > 256 or any(char.isspace() or ord(char) < 32 for char in subject):
                raise ValueError("BlobRef.scope is invalid")
        if self.scope.startswith("private:") and self.scope.removeprefix("private:") != self.owner:
            raise ValueError("BlobRef.scope is invalid")
        if self.retention_class not in {"request", "session", "plugin", "persistent"}:
            raise ValueError("BlobRef.retentionClass is invalid")

    def to_mapping(self) -> Mapping[str, Any]:
        return {
            "algorithm": self.algorithm,
            "digest": self.digest,
            "size": self.size,
            "mediaType": self.media_type,
            "scope": self.scope,
            "owner": self.owner,
            "retentionClass": self.retention_class,
        }

    @classmethod
    def from_mapping(cls, value: Mapping[str, Any]) -> "BlobRef":
        if not isinstance(value, Mapping) or set(value.keys()) != _BLOB_REF_KEYS:
            raise ValueError("BlobRef must contain exactly algorithm, digest, size, mediaType, scope, owner and retentionClass")
        for field in ("algorithm", "digest", "mediaType", "scope", "owner", "retentionClass"):
            if not isinstance(value[field], str):
                raise ValueError("BlobRef string fields have invalid types")
        if isinstance(value["size"], bool) or not isinstance(value["size"], int):
            raise ValueError("BlobRef.size must be an integer")
        try:
            return cls(
                algorithm=value["algorithm"],
                digest=value["digest"],
                size=value["size"],
                media_type=value["mediaType"],
                scope=value["scope"],
                owner=value["owner"],
                retention_class=value["retentionClass"],
            )
        except (KeyError, TypeError, ValueError) as error:
            raise ValueError(f"invalid BlobRef: {error}") from error


def _json_bytes(value: Any) -> bytes:
    def default(item: Any) -> Any:
        if isinstance(item, BlobRef):
            return item.to_mapping()
        raise TypeError(f"unsupported RPC value: {type(item).__name__}")

    return json.dumps(value, ensure_ascii=False, separators=(",", ":"), default=default).encode("utf-8")


def _contains_principal(value: Any) -> bool:
    if isinstance(value, Mapping):
        return "principal" in value or any(_contains_principal(child) for child in value.values())
    if isinstance(value, (list, tuple)):
        return any(_contains_principal(child) for child in value)
    return False


def _contains_authority_escape(value: Any) -> bool:
    if isinstance(value, Mapping):
        forbidden = {"principal", "lease", "leaseId", "capabilityLeaseIds"}
        return any(key in forbidden for key in value) or any(_contains_authority_escape(child) for child in value.values())
    if isinstance(value, (list, tuple)):
        return any(_contains_authority_escape(child) for child in value)
    return False


def _bounded_error_message(value: Any) -> str:
    encoded = str(value).encode("utf-8", errors="replace")
    if len(encoded) <= MAX_ERROR_MESSAGE_BYTES:
        return encoded.decode("utf-8")
    return encoded[:MAX_ERROR_MESSAGE_BYTES].decode("utf-8", errors="ignore")


def _validate_blob_refs(value: Any) -> None:
    if isinstance(value, Mapping):
        if "blobRef" in value:
            try:
                BlobRef.from_mapping(value["blobRef"])
            except ValueError as error:
                raise ProtocolError(str(error)) from error
        for key, child in value.items():
            if key == "blobRef":
                continue
            _validate_blob_refs(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _validate_blob_refs(child)


def _validate_inline_payload(value: Any) -> None:
    normalized = json.loads(_json_bytes(value).decode("utf-8"))
    if _contains_authority_escape(normalized):
        raise ProtocolError("principal and capability leases are Host-bound and forbidden in worker payloads")
    _validate_blob_refs(normalized)
    encoded = _json_bytes(normalized)
    if len(encoded) <= MAX_INLINE_BYTES:
        return
    if not isinstance(normalized, Mapping) or set(normalized.keys()) != {"blobRef"}:
        raise InlinePayloadTooLargeError(
            f"inline payload is {len(encoded)} bytes; use BlobRef"
        )
    try:
        BlobRef.from_mapping(normalized["blobRef"])
    except ValueError as error:
        raise ProtocolError(str(error)) from error


def _validate_host_result(operation: str, value: Any) -> None:
    if operation != "blob.read":
        _validate_inline_payload(value)
        return
    normalized = json.loads(_json_bytes(value).decode("utf-8"))
    if _contains_authority_escape(normalized):
        raise ProtocolError("principal and capability leases are Host-bound and forbidden in worker payloads")
    _validate_blob_refs(normalized)
    if not isinstance(normalized, Mapping):
        raise ProtocolError("blob.read result must be an object")
    content_base64 = normalized.get("contentBase64")
    if not isinstance(content_base64, str):
        raise ProtocolError("blob.read result must contain contentBase64")
    encoded = _json_bytes(normalized)
    try:
        decoded = base64.b64decode(content_base64, validate=True)
    except (ValueError, TypeError) as error:
        raise ProtocolError("blob.read contentBase64 is invalid") from error
    if (
        len(content_base64) > MAX_BLOB_READ_BASE64_BYTES
        or len(decoded) > MAX_BLOB_READ_CHUNK_BYTES
        or len(encoded) > MAX_BLOB_READ_RESULT_BYTES
    ):
        raise InlinePayloadTooLargeError(
            f"blob.read result is {len(encoded)} bytes; limit is {MAX_BLOB_READ_RESULT_BYTES}"
        )


def _validate_incoming_message(message: Mapping[str, Any]) -> None:
    if _contains_authority_escape(message):
        raise ProtocolError("principal and capability leases are Host-bound and forbidden in worker messages")
    message_type = message.get("type")
    if message_type == "response":
        if message.get("ok") is True:
            _validate_inline_payload(message.get("result"))
        error = message.get("error")
        if isinstance(error, Mapping) and "message" in error:
            if len(str(error["message"]).encode("utf-8")) > MAX_ERROR_MESSAGE_BYTES:
                raise ProtocolError("worker error message exceeds the bounded limit")
    elif message_type == "event":
        _validate_inline_payload(message.get("payload", message.get("event", {})))


def encode_frame(message: Mapping[str, Any]) -> bytes:
    payload = _json_bytes(message)
    if len(payload) > MAX_FRAME_BYTES:
        raise FrameTooLargeError(f"frame is {len(payload)} bytes; limit is {MAX_FRAME_BYTES}")
    return struct.pack(">I", len(payload)) + payload


def decode_frame(frame: bytes) -> Mapping[str, Any]:
    if len(frame) < 4:
        raise ProtocolError("incomplete frame header")
    size = struct.unpack(">I", frame[:4])[0]
    if size > MAX_FRAME_BYTES:
        raise FrameTooLargeError(f"frame is {size} bytes; limit is {MAX_FRAME_BYTES}")
    if len(frame) != size + 4:
        raise ProtocolError(f"frame length is {size}, received {len(frame) - 4}")
    try:
        value = json.loads(frame[4:].decode("utf-8"))
    except UnicodeDecodeError as error:
        raise ProtocolError("frame is not valid UTF-8") from error
    except json.JSONDecodeError as error:
        raise ProtocolError(f"frame is not valid JSON: {error}") from error
    if not isinstance(value, dict):
        raise ProtocolError("RPC frame must contain a JSON object")
    return value


class FrameDecoder:
    """Incremental decoder used by tests and transports with partial reads."""

    def __init__(self) -> None:
        self._buffer = bytearray()

    def feed(self, data: bytes) -> tuple[Mapping[str, Any], ...]:
        self._buffer.extend(data)
        if len(self._buffer) > MAX_FRAME_BYTES + 4:
            raise FrameTooLargeError("buffer exceeds the maximum frame size")
        messages: list[Mapping[str, Any]] = []
        while len(self._buffer) >= 4:
            size = struct.unpack(">I", self._buffer[:4])[0]
            if size > MAX_FRAME_BYTES:
                raise FrameTooLargeError(f"frame is {size} bytes; limit is {MAX_FRAME_BYTES}")
            if len(self._buffer) < size + 4:
                break
            messages.append(decode_frame(bytes(self._buffer[: size + 4])))
            del self._buffer[: size + 4]
        return tuple(messages)


def _read_exact(reader: BinaryIO, size: int) -> bytes:
    chunks: list[bytes] = []
    remaining = size
    while remaining:
        chunk = reader.read(remaining)
        if not chunk:
            raise EOFError("worker stream ended before a complete frame")
        chunks.append(chunk)
        remaining -= len(chunk)
    return b"".join(chunks)


def read_frame(reader: BinaryIO) -> Mapping[str, Any]:
    header = _read_exact(reader, 4)
    size = struct.unpack(">I", header)[0]
    if size > MAX_FRAME_BYTES:
        raise FrameTooLargeError(f"frame is {size} bytes; limit is {MAX_FRAME_BYTES}")
    return decode_frame(header + _read_exact(reader, size))


def write_frame(writer: BinaryIO, message: Mapping[str, Any]) -> None:
    writer.write(encode_frame(message))
    writer.flush()


class WorkerTransport(Protocol):
    def send(self, message: Mapping[str, Any]) -> None: ...

    def receive(self, timeout: float | None = None) -> Mapping[str, Any]: ...


class StreamTransport:
    """stdio transport; Host-side deadlines remain authoritative."""

    def __init__(self, reader: BinaryIO, writer: BinaryIO):
        self.reader = reader
        self.writer = writer

    def send(self, message: Mapping[str, Any]) -> None:
        write_frame(self.writer, message)

    def receive(self, timeout: float | None = None) -> Mapping[str, Any]:
        if timeout is not None and hasattr(self.reader, "fileno"):
            try:
                ready, _, _ = select.select([self.reader], [], [], timeout)
                if not ready:
                    raise TimeoutError("worker response deadline elapsed")
            except (OSError, ValueError):
                # Windows anonymous pipes do not support select; the Rust Host
                # enforces the process-level deadline in that environment.
                pass
        return read_frame(self.reader)


class WorkerClient:
    """Host-side test/compatibility client; production workers use WorkerRuntime."""

    def __init__(
        self,
        transport: WorkerTransport,
        *,
        plugin_id: str,
        plugin_version: str,
        worker_id: str,
        allowed_operations: tuple[str, ...],
        default_timeout: float = 5.0,
    ):
        self.transport = transport
        self.plugin_id = plugin_id
        self.plugin_version = plugin_version
        self.worker_id = worker_id
        self.allowed_operations = frozenset(allowed_operations)
        self.default_timeout = default_timeout
        self._sequence = 0
        self._handshaken = False
        self._events: list[Mapping[str, Any]] = []

    def handshake(self) -> Mapping[str, Any]:
        self.transport.send(
            {
                "type": "hello",
                "apiVersion": WORKER_RPC_API_VERSION,
                "pluginId": self.plugin_id,
                "pluginVersion": self.plugin_version,
                "workerId": self.worker_id,
                "allowedOperations": sorted(self.allowed_operations),
            }
        )
        handshake_deadline = time.monotonic() + self.default_timeout
        ack = self.transport.receive(max(0.0, handshake_deadline - time.monotonic()))
        if ack.get("type") != "helloAck" or ack.get("apiVersion") != WORKER_RPC_API_VERSION:
            raise ProtocolError("invalid worker helloAck")
        if ack.get("workerId") != self.worker_id:
            raise ProtocolError("worker identity mismatch")
        if "principal" in ack:
            raise ProtocolError("worker must not negotiate a principal")
        operations = ack.get("operations")
        if not isinstance(operations, list) or not set(operations).issubset(self.allowed_operations):
            raise ProtocolError("worker negotiated an operation outside the Host allowlist")
        self._handshaken = True
        return ack

    def request(self, operation: str, payload: Mapping[str, Any], timeout: float | None = None) -> Any:
        if not self._handshaken:
            raise ProtocolError("worker handshake is required")
        if operation not in self.allowed_operations:
            raise ProtocolError(f"operation is not allowed: {operation}")
        _validate_inline_payload(payload)
        self._sequence += 1
        request_id = f"{self.worker_id}-{self._sequence}-{uuid.uuid4().hex[:8]}"
        budget = self.default_timeout if timeout is None else timeout
        if budget <= 0:
            raise ProtocolError("request timeout must be positive")
        deadline = time.monotonic() + budget
        self.transport.send(
            {
                "type": "request",
                "apiVersion": WORKER_RPC_API_VERSION,
                "requestId": request_id,
                "operation": operation,
                "payload": payload,
                "deadlineMs": int(budget * 1000),
            }
        )
        event_count = 0
        while True:
            try:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError("worker response deadline elapsed")
                message = self.transport.receive(remaining)
            except TimeoutError as error:
                self.cancel(request_id)
                raise WorkerTimeoutError(str(error)) from error
            if message.get("type") == "event":
                event_count += 1
                if event_count > MAX_EVENTS_PER_REQUEST:
                    raise ProtocolError("worker event limit exceeded")
                _validate_incoming_message(message)
                self._events.append(message)
                continue
            if message.get("type") != "response":
                raise ProtocolError("expected worker response")
            if message.get("requestId") != request_id:
                raise ProtocolError("worker response correlation mismatch")
            if message.get("ok") is True:
                _validate_incoming_message(message)
                return message.get("result")
            error = message.get("error") or {}
            _validate_incoming_message(message)
            if len(str(error.get("message", "worker request failed")).encode("utf-8")) > MAX_ERROR_MESSAGE_BYTES:
                raise ProtocolError("worker error message exceeds the bounded limit")
            raise RemoteWorkerError(str(error.get("code", "REMOTE_ERROR")), _bounded_error_message(error.get("message", "worker request failed")))

    def cancel(self, request_id: str) -> None:
        self.transport.send(
            {
                "type": "cancel",
                "apiVersion": WORKER_RPC_API_VERSION,
                "requestId": request_id,
            }
        )

    def shutdown(self) -> None:
        self.transport.send({"type": "shutdown", "apiVersion": WORKER_RPC_API_VERSION})

    def events(self) -> tuple[Mapping[str, Any], ...]:
        return tuple(self._events)


class HostBusClient:
    """Thin reverse-RPC adapter bound to one Host-created worker request."""

    def __init__(self, reader: BinaryIO, writer: BinaryIO, parent_request_id: str, deadline: float):
        self._reader = reader
        self._writer = writer
        self._parent_request_id = parent_request_id
        self._deadline = deadline
        self._sequence = 0

    @property
    def deadline(self) -> float:
        """Absolute monotonic deadline inherited from the Rust parent call."""
        return self._deadline

    def remaining_seconds(self) -> float:
        """Remaining parent budget; direct provider clients must not reset it."""
        return max(0.0, self._deadline - time.monotonic())

    def call(self, operation: str, payload: Mapping[str, Any]) -> Any:
        if not re.fullmatch(r"[a-z][a-z0-9]*(?:[._/-][a-z0-9]+)*", operation) or len(operation) > 160:
            raise ProtocolError("invalid Host Bus operation")
        if _contains_authority_escape(payload):
            raise ProtocolError("worker cannot provide principal or capability leases")
        _validate_inline_payload(payload)
        self._sequence += 1
        if self._sequence > MAX_HOST_CALLS_PER_REQUEST:
            raise ProtocolError("worker Host Bus call limit exceeded")
        remaining = self._deadline - time.monotonic()
        if remaining <= 0:
            raise WorkerTimeoutError("worker Host Bus deadline elapsed")
        host_request_id = f"{self._parent_request_id}:host:{self._sequence}"
        write_frame(self._writer, {
            "type": "hostRequest",
            "apiVersion": WORKER_RPC_API_VERSION,
            "parentRequestId": self._parent_request_id,
            "hostRequestId": host_request_id,
            "operation": operation,
            "payload": dict(payload),
            "deadlineMs": max(1, int(remaining * 1000)),
        })
        response = read_frame(self._reader)
        if response.get("type") == "cancel" and response.get("requestId") == self._parent_request_id:
            raise WorkerTimeoutError("Host cancelled the worker request")
        if response.get("type") != "hostResponse" or response.get("apiVersion") != WORKER_RPC_API_VERSION:
            raise ProtocolError("expected versioned hostResponse")
        if response.get("parentRequestId") != self._parent_request_id or response.get("hostRequestId") != host_request_id:
            raise ProtocolError("Host Bus response correlation mismatch")
        if response.get("ok") is True:
            _validate_host_result(operation, response.get("result"))
            return response.get("result")
        error = response.get("error") or {}
        raise RemoteWorkerError(str(error.get("code", "HOST_ERROR")), _bounded_error_message(error.get("message", "Host Bus call failed")))


class WorkerServer:
    """Production worker runtime: Rust owns Host authority and process identity."""

    def __init__(self, operations: Mapping[str, Callable[..., Any]]):
        self.operations = dict(operations)
        self.allowed_operations: frozenset[str] = frozenset()

    def serve(self, reader: BinaryIO | None = None, writer: BinaryIO | None = None) -> None:
        reader = reader or sys.stdin.buffer
        writer = writer or sys.stdout.buffer
        hello = read_frame(reader)
        if hello.get("type") != "hello" or hello.get("apiVersion") != WORKER_RPC_API_VERSION:
            write_frame(writer, {"type": "error", "code": "PROTOCOL_VERSION", "message": _bounded_error_message("invalid hello")})
            return
        if _contains_principal(hello):
            raise ProtocolError("principal is Host-bound and forbidden in handshake")
        requested = hello.get("allowedOperations")
        if not isinstance(requested, list) or any(not isinstance(item, str) for item in requested):
            raise ProtocolError("hello.allowedOperations must be a string list")
        self.allowed_operations = frozenset(set(requested) & self.operations.keys())
        write_frame(
            writer,
            {
                "type": "helloAck",
                "apiVersion": WORKER_RPC_API_VERSION,
                "workerId": hello.get("workerId"),
                "operations": sorted(self.allowed_operations),
            },
        )
        while True:
            message = read_frame(reader)
            message_type = message.get("type")
            if message_type == "shutdown":
                self._send_response(writer, "shutdown", {"stopped": True})
                return
            if message_type == "cancel":
                self._send_response(writer, message.get("requestId"), {"cancelled": True})
                continue
            if message_type != "request":
                raise ProtocolError(f"unknown worker message type: {message_type}")
            request_id = message.get("requestId")
            operation = message.get("operation")
            payload = message.get("payload")
            try:
                if not isinstance(request_id, str) or not isinstance(operation, str) or not isinstance(payload, dict):
                    raise ProtocolError("request fields are invalid")
                _validate_inline_payload(payload)
                if operation not in self.allowed_operations:
                    raise RemoteWorkerError("NOT_ALLOWED", f"operation is not allowed: {operation}")
                deadline_ms = message.get("deadlineMs")
                if isinstance(deadline_ms, bool) or not isinstance(deadline_ms, int) or deadline_ms <= 0:
                    raise ProtocolError("request.deadlineMs must be a positive integer")
                host = HostBusClient(reader, writer, request_id, time.monotonic() + deadline_ms / 1000)
                handler = self.operations[operation]
                if len(inspect.signature(handler).parameters) >= 2:
                    result = handler(payload, host)
                else:
                    result = handler(payload)
                self._send_response(writer, request_id, result)
            except RemoteWorkerError as error:
                self._send_error(writer, request_id, error.code, error.message)
            except Exception as error:
                self._send_error(writer, request_id, "WORKER_ERROR", str(error))

    def emit_event(self, writer: BinaryIO, request_id: str, payload: Any) -> None:
        _validate_inline_payload(payload)
        write_frame(writer, {"type": "event", "apiVersion": WORKER_RPC_API_VERSION, "requestId": request_id, "payload": payload})

    def _send_response(self, writer: BinaryIO, request_id: Any, result: Any) -> None:
        _validate_inline_payload(result)
        write_frame(writer, {"type": "response", "apiVersion": WORKER_RPC_API_VERSION, "requestId": request_id, "ok": True, "result": result})

    def _send_error(self, writer: BinaryIO, request_id: Any, code: str, message: Any) -> None:
        write_frame(writer, {"type": "response", "apiVersion": WORKER_RPC_API_VERSION, "requestId": request_id, "ok": False, "error": {"code": code, "message": _bounded_error_message(message)}})


WorkerRuntime = WorkerServer


@dataclass(frozen=True)
class PluginUpdateInfo:
    """Optional SemVer update metadata rendered by the host plugin settings view."""

    latest_version: str | None = None
    url: str | None = None
    release_notes: str | None = None


@dataclass(frozen=True)
class PluginSetting:
    """Declarative setting metadata; the host owns persistence and rendering."""

    setting_id: str
    label: str
    setting_type: PluginSettingType
    label_key: str | None = None
    secret: bool = False
    required: bool = False
    description: str = ""
    description_key: str | None = None
    placeholder: str = ""
    placeholder_key: str | None = None
    group: str = ""
    write_only: bool = False
    default: bool | float | int | str | None = None
    minimum: float | None = None
    maximum: float | None = None
    step: float | None = None
    options: tuple[Mapping[str, str], ...] = ()

    @property
    def is_secret(self) -> bool:
        """Secret settings are host-owned and must never be read by a plugin."""

        return self.secret

    def to_mapping(self) -> Mapping[str, Any]:
        """Return the host manifest shape without exposing any secret value."""

        value: dict[str, Any] = {
            "id": self.setting_id,
            "label": self.label,
            "labelKey": self.label_key,
            "type": self.setting_type,
            "secret": self.secret,
            "required": self.required,
            "description": self.description,
            "descriptionKey": self.description_key,
            "placeholder": self.placeholder,
            "placeholderKey": self.placeholder_key,
            "group": self.group,
            "writeOnly": self.write_only or self.is_secret,
            "options": [dict(option) for option in self.options],
        }
        if self.default is not None and not self.is_secret:
            value["default"] = self.default
        if self.minimum is not None:
            value["min"] = self.minimum
        if self.maximum is not None:
            value["max"] = self.maximum
        if self.step is not None:
            value["step"] = self.step
        return value


@dataclass(frozen=True)
class PluginConnectionTestAction:
    """Declarative action metadata; execution remains host-owned."""

    action_id: str
    label: str
    label_key: str | None = None
    description: str = ""
    description_key: str | None = None
    placeholder: str = ""
    placeholder_key: str | None = None
    kind: PluginConnectionTestActionKind | None = None
    input_type: Literal["text", "bundled-pdf"] | None = None
    fixture: str | None = None
    file_upload: Literal["never", "may-upload"] | None = None

    def to_mapping(self) -> Mapping[str, Any]:
        result: dict[str, Any] = {
            "id": self.action_id,
            "label": self.label,
            "labelKey": self.label_key,
            "description": self.description,
            "descriptionKey": self.description_key,
            "placeholder": self.placeholder,
            "placeholderKey": self.placeholder_key,
        }
        if self.kind:
            result["kind"] = self.kind
        if self.input_type == "text":
            result["input"] = {"type": "text", "fileUpload": "never"}
        elif self.input_type == "bundled-pdf":
            result["input"] = {
                "type": "bundled-pdf",
                "fixture": self.fixture or "host-minimal-pdf-v1",
                "fileUpload": self.file_upload or "may-upload",
            }
        return result


@dataclass(frozen=True)
class PluginConnection:
    """Host-mediated API connection with a declarative, non-secret test action."""

    connection_id: str
    label: str
    url_setting_id: str
    format_setting_id: str
    label_key: str | None = None
    description: str = ""
    description_key: str | None = None
    placeholder: str = ""
    placeholder_key: str | None = None
    model_setting_id: str | None = None
    credential_source_setting_id: str | None = None
    credential_env_var_setting_id: str | None = None
    credential_source: PluginCredentialSource = "environment"
    credential_env_var: str | None = None
    host_secret_setting_id: str | None = None
    test_action_id: str = "test-connection"
    test_action_label: str = "Test connection"
    test_actions: tuple[PluginConnectionTestAction, ...] = ()

    def to_mapping(self) -> Mapping[str, Any]:
        api_key: dict[str, Any]
        if self.credential_source == "host-secret":
            api_key = {
                "source": "host-secret",
                "settingId": self.host_secret_setting_id,
            }
        else:
            api_key = {
                "source": "environment",
                "name": self.credential_env_var,
                "fallbackSettingId": self.host_secret_setting_id,
            }
        result: dict[str, Any] = {
            "id": self.connection_id,
            "label": self.label,
            "labelKey": self.label_key,
            "description": self.description,
            "descriptionKey": self.description_key,
            "placeholder": self.placeholder,
            "placeholderKey": self.placeholder_key,
            "urlSettingId": self.url_setting_id,
            "formatSettingId": self.format_setting_id,
            "apiKey": api_key,
            "testAction": {"id": self.test_action_id, "label": self.test_action_label},
        }
        if self.test_actions:
            result["testActions"] = [action.to_mapping() for action in self.test_actions]
        if self.model_setting_id:
            result["modelSettingId"] = self.model_setting_id
        if self.credential_source_setting_id:
            result["credentialSourceSettingId"] = self.credential_source_setting_id
        if self.credential_env_var_setting_id:
            result["credentialEnvVarSettingId"] = self.credential_env_var_setting_id
        return result


@dataclass(frozen=True)
class PluginManifest:
    plugin_id: str
    name: str
    version: str
    capabilities: tuple[str, ...]
    developer: str = ""
    developer_id: str | None = None
    permissions: tuple[str, ...] = ()
    update: PluginUpdateInfo | None = None
    settings: tuple[PluginSetting, ...] = ()
    connections: tuple[PluginConnection, ...] = ()
    private_i18n: "PluginPrivateI18n | None" = None


@dataclass(frozen=True)
class PluginPrivateLocale:
    locale: str
    path: str


@dataclass(frozen=True)
class PluginPrivateI18n:
    """Plugin-owned locale files; never a global LocalePlugin contribution."""

    default_locale: str
    locales: tuple[PluginPrivateLocale, ...]

    def to_mapping(self) -> Mapping[str, Any]:
        return {
            "defaultLocale": self.default_locale,
            "locales": {locale.locale: locale.path for locale in self.locales},
        }


class SettingReader(Protocol):
    """Read only host-validated non-secret settings.

    Secret settings never traverse this interface. A manifest-declared
    provider credential may instead be injected by Rust into the exact Worker
    process environment after inherited variables are cleared.
    """

    def get_boolean(self, setting_id: str) -> bool | None: ...

    def get_number(self, setting_id: str) -> float | None: ...

    def get_text(self, setting_id: str) -> str | None: ...

    def has(self, setting_id: str) -> bool: ...


@dataclass(frozen=True)
class GraphNodeProposal:
    """Reviewable semantic node / 可审阅的语义节点提案。"""

    node_id: str
    node_type: str
    title: str
    body: str = ""
    tags: tuple[str, ...] = ()
    data: Mapping[str, Any] | None = None

    def to_operation(self) -> Mapping[str, Any]:
        return {
            "op": "add-node",
            "node": {
                "id": self.node_id,
                "type": self.node_type,
                "title": self.title,
                "body": self.body,
                "tags": list(self.tags),
                "data": dict(self.data or {}),
            },
        }


@dataclass(frozen=True)
class GraphEdgeProposal:
    """Reviewable semantic relation / 可审阅的语义关系提案。"""

    edge_id: str
    source: str
    target: str
    edge_type: str
    note: str = ""

    def to_operation(self) -> Mapping[str, Any]:
        return {
            "op": "add-edge",
            "edge": {
                "id": self.edge_id,
                "source": self.source,
                "target": self.target,
                "type": self.edge_type,
                "note": self.note,
            },
        }


@dataclass(frozen=True)
class GraphPatch:
    """Portable host-reviewed patch / 由宿主审阅后应用的可移植图谱补丁。"""

    plugin_id: str
    operation: str
    title: str
    summary: str
    operations: tuple[Mapping[str, Any], ...]
    external_id: str | None = None

    def to_mapping(self) -> Mapping[str, Any]:
        source: dict[str, str] = {
            "pluginId": self.plugin_id,
            "operation": self.operation,
        }
        if self.external_id:
            source["externalId"] = self.external_id
        return {
            "apiVersion": GRAPH_PATCH_API_VERSION,
            "source": source,
            "title": self.title,
            "summary": self.summary,
            "reviewRequired": True,
            "operations": list(self.operations),
        }


class NetworkBlockExtractor(Protocol):
    """Torch/ONNX adapters implement this without importing application stores."""

    def extract(self, model: Any, *, external_id: str | None = None) -> GraphPatch: ...
