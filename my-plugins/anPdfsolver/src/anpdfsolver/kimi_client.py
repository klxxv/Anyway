"""Dependency-free direct Kimi client owned by the packaged plugin worker.

Rust injects one provider secret into the exact plugin process and constructs
the non-secret runtime configuration.  This module performs HTTPS, Kimi Files
and SSE parsing without forwarding credentials or raw provider diagnostics to
the plugin UI.
"""

from __future__ import annotations

from dataclasses import dataclass
import io
import json
import os
import re
import secrets
import ssl
import time
from typing import Any, BinaryIO, Callable, Iterable, Mapping, Protocol
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, urlopen

from anpdfsolver.typed_frames import TypedFrameError, TypedFrameParser, rendered_system_prompt

MAX_HTTP_BODY_BYTES = 2 * 1024 * 1024
MAX_SSE_BYTES = 8 * 1024 * 1024
MAX_SSE_EVENTS = 4_096
MAX_DOCUMENT_TEXT_BYTES = 1024 * 1024
MAX_UPLOAD_BYTES = 32 * 1024 * 1024
DEFAULT_TIMEOUT_SECONDS = 60.0
MAX_ATTEMPTS = 3
SECRET_ENV = "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY"


class KimiClientError(RuntimeError):
    def __init__(self, code: str, message: str, *, retryable: bool = False):
        super().__init__(message[:512])
        self.code = code
        self.message = message[:512]
        self.retryable = retryable


class ProviderSecret:
    """A deliberately non-printable provider secret."""

    __slots__ = ("__value",)

    def __init__(self, value: str):
        if not value:
            raise KimiClientError("PROVIDER_CREDENTIAL_MISSING", "The configured provider credential is missing.", retryable=True)
        self.__value = value

    def reveal(self) -> str:
        return self.__value

    def __repr__(self) -> str:
        return "ProviderSecret(configured=True)"

    __str__ = __repr__


@dataclass(frozen=True)
class KimiConfig:
    base_url: str
    api_format: str
    model: str
    pdf_transport: str
    thinking: str
    allowed_domains: tuple[str, ...]
    public_progress: str = "disabled"
    secret_env: str = SECRET_ENV

    @classmethod
    def from_runtime_config(cls, runtime_config: Mapping[str, Any]) -> "KimiConfig":
        providers = runtime_config.get("providers")
        if not isinstance(providers, list):
            raise KimiClientError("PROVIDER_CONFIG_MISSING", "The Host did not provide a provider configuration.", retryable=True)
        provider = next((item for item in providers if isinstance(item, Mapping) and item.get("id") == "kimi"), None)
        if provider is None:
            raise KimiClientError("PROVIDER_CONFIG_MISSING", "The Host did not provide the declared Kimi configuration.", retryable=True)
        allowed = provider.get("allowedDomains")
        if not isinstance(allowed, list) or not allowed or any(not isinstance(item, str) for item in allowed):
            raise KimiClientError("PROVIDER_CONFIG_INVALID", "The provider domain policy is invalid.")
        secret_env = provider.get("secretEnv")
        if secret_env != SECRET_ENV:
            raise KimiClientError("PROVIDER_CONFIG_INVALID", "The provider secret binding is invalid.")
        config = cls(
            base_url=_required_text(provider.get("baseUrl"), "baseUrl", 512),
            api_format=_choice(provider.get("format"), "format", {"openai", "anthropic"}),
            model=_required_text(provider.get("model"), "model", 160),
            pdf_transport=_choice(provider.get("pdfTransport"), "pdfTransport", {"local-text", "kimi-file-extract"}),
            thinking=_choice(provider.get("thinking", "enabled"), "thinking", {"enabled", "disabled"}),
            allowed_domains=tuple(allowed),
            public_progress=_choice(provider.get("publicProgress", "disabled"), "publicProgress", {"enabled", "disabled"}),
            secret_env=secret_env,
        )
        config.validate_base_url()
        return config

    def validate_url(self, value: str) -> None:
        parsed = urlparse(value)
        if parsed.scheme != "https" or not parsed.hostname or parsed.username or parsed.password or parsed.query or parsed.fragment:
            raise KimiClientError("PROVIDER_URL_INVALID", "The provider URL must be an HTTPS base URL without credentials or query data.")
        if parsed.hostname not in self.allowed_domains:
            raise KimiClientError("PROVIDER_URL_DENIED", "The provider URL is outside the declared domain policy.")

    def validate_base_url(self) -> None:
        self.validate_url(self.base_url)
        parsed = urlparse(self.base_url)
        expected = "/v1" if self.api_format == "openai" else "/anthropic"
        if parsed.path.rstrip("/") != expected:
            raise KimiClientError("PROVIDER_URL_INVALID", f"The {self.api_format} provider base URL must end with {expected}.")

    def secret(self) -> ProviderSecret:
        return ProviderSecret(os.environ.get(self.secret_env, ""))

    @property
    def files_base_url(self) -> str:
        parsed = urlparse(self.base_url)
        return f"https://{parsed.hostname}/v1"


def _required_text(value: Any, field: str, limit: int) -> str:
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > limit:
        raise KimiClientError("PROVIDER_CONFIG_INVALID", f"The provider {field} setting is invalid.")
    return value


def _choice(value: Any, field: str, allowed: set[str]) -> str:
    if value not in allowed:
        raise KimiClientError("PROVIDER_CONFIG_INVALID", f"The provider {field} setting is invalid.")
    return str(value)


@dataclass
class HttpResponse:
    status: int
    body: BinaryIO

    def close(self) -> None:
        close = getattr(self.body, "close", None)
        if callable(close):
            close()


class HttpTransport(Protocol):
    def request(self, method: str, url: str, headers: Mapping[str, str], body: bytes | None, timeout: float) -> HttpResponse: ...


class UrllibTransport:
    def __init__(self) -> None:
        self._ssl_context = ssl.create_default_context()

    def request(self, method: str, url: str, headers: Mapping[str, str], body: bytes | None, timeout: float) -> HttpResponse:
        request = Request(url, data=body, headers=dict(headers), method=method)
        try:
            response = urlopen(request, timeout=timeout, context=self._ssl_context)
            return HttpResponse(status=int(response.status), body=response)
        except HTTPError as error:
            return HttpResponse(status=int(error.code), body=error)
        except (URLError, TimeoutError, OSError) as error:
            raise KimiClientError("PROVIDER_UNREACHABLE", "The provider request could not be completed.", retryable=True) from error


class SseDecoder:
    def __init__(self) -> None:
        self._buffer = bytearray()
        self._event = "message"
        self._data: list[str] = []
        self._bytes = 0
        self._events = 0

    def feed(self, chunk: bytes) -> list[tuple[str, str]]:
        self._bytes += len(chunk)
        if self._bytes > MAX_SSE_BYTES:
            raise KimiClientError("PROVIDER_STREAM_TOO_LARGE", "The provider stream exceeded its byte limit.")
        self._buffer.extend(chunk)
        output: list[tuple[str, str]] = []
        while b"\n" in self._buffer:
            raw, _, remainder = self._buffer.partition(b"\n")
            self._buffer = bytearray(remainder)
            line = raw.rstrip(b"\r")
            try:
                text = line.decode("utf-8")
            except UnicodeDecodeError as error:
                raise KimiClientError("PROVIDER_STREAM_UTF8", "The provider stream was not valid UTF-8.") from error
            if not text:
                if self._data:
                    self._events += 1
                    if self._events > MAX_SSE_EVENTS:
                        raise KimiClientError("PROVIDER_EVENT_LIMIT", "The provider stream exceeded its event limit.")
                    output.append((self._event, "\n".join(self._data)))
                self._event = "message"
                self._data = []
            elif text.startswith("event:"):
                self._event = text[6:].strip()[:80]
            elif text.startswith("data:"):
                self._data.append(text[5:].lstrip())
        return output

    def finish(self) -> list[tuple[str, str]]:
        if self._buffer.strip() or self._data:
            raise KimiClientError("PROVIDER_STREAM_TRUNCATED", "The provider SSE stream ended mid-event.", retryable=True)
        return []


class KimiClient:
    def __init__(
        self,
        config: KimiConfig,
        transport: HttpTransport | None = None,
        *,
        clock: Callable[[], float] = time.monotonic,
        sleeper: Callable[[float], None] = time.sleep,
    ) -> None:
        self.config = config
        self.config.validate_base_url()
        self.transport = transport or UrllibTransport()
        self.clock = clock
        self.sleeper = sleeper

    def analyze(
        self,
        *,
        pdf_bytes: bytes,
        label: str,
        local_text: str,
        deadline: float,
        cancelled: Callable[[], bool] = lambda: False,
        on_frame: Callable[[Mapping[str, Any]], None] | None = None,
    ) -> tuple[Mapping[str, Any], ...]:
        self._check_budget(deadline, cancelled)
        document_text = local_text
        file_id: str | None = None
        if self.config.pdf_transport == "kimi-file-extract":
            file_id = self.upload_pdf(pdf_bytes, label, deadline, cancelled)
            try:
                document_text = self.read_file_content(file_id, deadline, cancelled)
            except Exception:
                self.delete_file(file_id, deadline, cancelled, best_effort=True)
                raise
        try:
            return self.stream_frames(document_text, deadline, cancelled, on_frame)
        finally:
            if file_id is not None:
                self.delete_file(file_id, deadline, cancelled, best_effort=True)

    def upload_pdf(self, content: bytes, label: str, deadline: float, cancelled: Callable[[], bool]) -> str:
        if not content.startswith(b"%PDF-") or len(content) > MAX_UPLOAD_BYTES:
            raise KimiClientError("PDF_UPLOAD_INVALID", "The PDF is invalid or exceeds the provider upload limit.")
        safe_label = re.sub(r"[^A-Za-z0-9._ -]", "_", label)[:160].strip(" .")
        if not safe_label:
            safe_label = "document.pdf"
        boundary = "anyway-" + secrets.token_hex(12)
        body = (
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"purpose\"\r\n\r\nfile-extract\r\n"
            f"--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{safe_label[:160]}\"\r\n"
            "Content-Type: application/pdf\r\n\r\n"
        ).encode("utf-8") + content + f"\r\n--{boundary}--\r\n".encode("ascii")
        result = self._json_request(
            "POST", f"{self.config.files_base_url}/files",
            {"Content-Type": f"multipart/form-data; boundary={boundary}"}, body, deadline, cancelled,
        )
        file_id = result.get("id") if isinstance(result, Mapping) else None
        if not isinstance(file_id, str) or not re.fullmatch(r"[A-Za-z0-9._-]{1,256}", file_id):
            raise KimiClientError("PROVIDER_FILE_RESPONSE_INVALID", "Kimi Files did not return a valid file identifier.")
        return file_id

    def read_file_content(self, file_id: str, deadline: float, cancelled: Callable[[], bool]) -> str:
        response = self._request("GET", f"{self.config.files_base_url}/files/{file_id}/content", {}, None, deadline, cancelled)
        try:
            content = _read_bounded(response.body, MAX_DOCUMENT_TEXT_BYTES)
        finally:
            response.close()
        if response.status < 200 or response.status >= 300:
            raise _status_error(response.status)
        try:
            return content.decode("utf-8")
        except UnicodeDecodeError as error:
            raise KimiClientError("PROVIDER_FILE_CONTENT_INVALID", "Kimi Files returned invalid extracted text.") from error

    def delete_file(self, file_id: str, deadline: float, cancelled: Callable[[], bool], *, best_effort: bool) -> None:
        try:
            response = self._request("DELETE", f"{self.config.files_base_url}/files/{file_id}", {}, None, deadline, cancelled)
            try:
                _read_bounded(response.body, 64 * 1024)
            finally:
                response.close()
            if not 200 <= response.status < 300 and not best_effort:
                raise _status_error(response.status)
        except KimiClientError:
            if not best_effort:
                raise

    def stream_frames(
        self,
        document_text: str,
        deadline: float,
        cancelled: Callable[[], bool],
        on_frame: Callable[[Mapping[str, Any]], None] | None,
    ) -> tuple[Mapping[str, Any], ...]:
        encoded = document_text.encode("utf-8")
        if not encoded or len(encoded) > MAX_DOCUMENT_TEXT_BYTES:
            raise KimiClientError("DOCUMENT_TEXT_INVALID", "The extracted document text is empty or exceeds the model input limit.")
        last_error: KimiClientError | TypedFrameError | None = None
        for attempt in range(MAX_ATTEMPTS):
            self._check_budget(deadline, cancelled)
            parser = TypedFrameParser()
            decoder = SseDecoder()
            body = self._chat_body(document_text)
            response: HttpResponse | None = None
            try:
                response = self._request("POST", self._chat_url(), {"Content-Type": "application/json"}, body, deadline, cancelled)
                if not 200 <= response.status < 300:
                    raise _status_error(response.status)
                completed = False
                for chunk in _iter_chunks(response.body):
                    self._check_budget(deadline, cancelled)
                    for event_name, data in decoder.feed(chunk):
                        visible, done = self._visible_delta(event_name, data)
                        if visible:
                            for frame in parser.feed(visible.encode("utf-8")):
                                if on_frame is not None:
                                    on_frame(frame)
                        completed = completed or done
                decoder.finish()
                if not completed:
                    raise KimiClientError("PROVIDER_STREAM_TRUNCATED", "The provider stream ended before its completion marker.", retryable=True)
                return parser.finish()
            except (KimiClientError, TypedFrameError) as error:
                retryable = bool(getattr(error, "retryable", False))
                last_error = error
                if not retryable or attempt + 1 >= MAX_ATTEMPTS:
                    raise
                self._backoff(attempt, deadline, cancelled)
            finally:
                if response is not None:
                    response.close()
        assert last_error is not None
        raise last_error

    def _chat_body(self, document_text: str) -> bytes:
        user = "Document text follows. Produce bounded NDJSON frames only.\n\n" + document_text
        progress_policy = (
            "Emit short progress frames containing only user-visible status summaries."
            if self.config.public_progress == "enabled"
            else "Do not emit progress frames. Emit evidence, entity, relation, warning, error and end frames only. Use operator only as a compatibility alias for relation."
        )
        system_prompt = rendered_system_prompt() + "\n\n[progress-policy]\n" + progress_policy
        if self.config.api_format == "openai":
            value: Mapping[str, Any] = {
                "model": self.config.model,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": user},
                ],
                "stream": True,
                "thinking": {"type": self.config.thinking},
            }
        else:
            value = {
                "model": self.config.model,
                "system": system_prompt,
                "messages": [{"role": "user", "content": user}],
                "max_tokens": 32_768,
                "stream": True,
                "thinking": {"type": self.config.thinking, "budget_tokens": 16_000} if self.config.thinking == "enabled" else {"type": "disabled"},
            }
        return json.dumps(value, separators=(",", ":"), ensure_ascii=False).encode("utf-8")

    def _chat_url(self) -> str:
        suffix = "/chat/completions" if self.config.api_format == "openai" else "/v1/messages"
        return self.config.base_url.rstrip("/") + suffix

    def _visible_delta(self, event_name: str, data: str) -> tuple[str, bool]:
        if self.config.api_format == "openai":
            if data == "[DONE]":
                return "", True
            value = _json_event(data)
            delta = (((value.get("choices") or [{}])[0] or {}).get("delta") or {}) if isinstance(value, Mapping) else {}
            # Kimi may co-locate private ``reasoning_content`` with ordinary
            # deltas. It is intentionally neither stored nor forwarded.
            if isinstance(delta, Mapping) and "reasoning_content" in delta:
                pass
            content = delta.get("content") if isinstance(delta, Mapping) else None
            return (content if isinstance(content, str) else ""), False
        value = _json_event(data)
        event_type = value.get("type") if isinstance(value, Mapping) else event_name
        if event_type == "message_stop" or event_name == "message_stop":
            return "", True
        if event_type != "content_block_delta":
            return "", False
        delta = value.get("delta") if isinstance(value, Mapping) else None
        if not isinstance(delta, Mapping):
            return "", False
        if delta.get("type") == "thinking_delta":
            return "", False
        if delta.get("type") != "text_delta":
            return "", False
        text = delta.get("text")
        return (text if isinstance(text, str) else ""), False

    def _json_request(self, method: str, url: str, headers: Mapping[str, str], body: bytes | None, deadline: float, cancelled: Callable[[], bool]) -> Mapping[str, Any]:
        response = self._request(method, url, headers, body, deadline, cancelled)
        try:
            content = _read_bounded(response.body, MAX_HTTP_BODY_BYTES)
        finally:
            response.close()
        if not 200 <= response.status < 300:
            raise _status_error(response.status)
        try:
            value = json.loads(content.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise KimiClientError("PROVIDER_RESPONSE_INVALID", "The provider returned an invalid JSON response.") from error
        if not isinstance(value, Mapping):
            raise KimiClientError("PROVIDER_RESPONSE_INVALID", "The provider returned an invalid JSON response.")
        return value

    def _request(self, method: str, url: str, headers: Mapping[str, str], body: bytes | None, deadline: float, cancelled: Callable[[], bool]) -> HttpResponse:
        self.config.validate_url(url)
        self._check_budget(deadline, cancelled)
        secret = self.config.secret()
        request_headers = {"Accept": "application/json, text/event-stream", "Authorization": "Bearer " + secret.reveal(), **headers}
        if self.config.api_format == "anthropic" and url == self._chat_url():
            request_headers["anthropic-version"] = "2023-06-01"
        timeout = min(DEFAULT_TIMEOUT_SECONDS, max(0.05, deadline - self.clock()))
        return self.transport.request(method, url, request_headers, body, timeout)

    def _check_budget(self, deadline: float, cancelled: Callable[[], bool]) -> None:
        if cancelled():
            raise KimiClientError("PROVIDER_CANCELLED", "The provider request was cancelled.", retryable=True)
        if self.clock() >= deadline:
            raise KimiClientError("PROVIDER_DEADLINE", "The provider request deadline elapsed.", retryable=True)

    def _backoff(self, attempt: int, deadline: float, cancelled: Callable[[], bool]) -> None:
        delay = min(2.0, 0.25 * (2 ** attempt))
        self._check_budget(deadline, cancelled)
        if self.clock() + delay >= deadline:
            raise KimiClientError("PROVIDER_DEADLINE", "The provider request deadline elapsed.", retryable=True)
        self.sleeper(delay)


def _json_event(data: str) -> Mapping[str, Any]:
    if len(data.encode("utf-8")) > 64 * 1024:
        raise KimiClientError("PROVIDER_EVENT_TOO_LARGE", "The provider emitted an oversized SSE event.")
    try:
        value = json.loads(data)
    except json.JSONDecodeError as error:
        raise KimiClientError("PROVIDER_EVENT_INVALID", "The provider emitted invalid SSE JSON.", retryable=True) from error
    if not isinstance(value, Mapping):
        raise KimiClientError("PROVIDER_EVENT_INVALID", "The provider emitted invalid SSE JSON.")
    return value


def _iter_chunks(body: BinaryIO, size: int = 4_096) -> Iterable[bytes]:
    while True:
        chunk = body.read(size)
        if not chunk:
            return
        yield chunk


def _read_bounded(body: BinaryIO, limit: int) -> bytes:
    result = body.read(limit + 1)
    if len(result) > limit:
        raise KimiClientError("PROVIDER_RESPONSE_TOO_LARGE", "The provider response exceeded its byte limit.")
    return result


def _status_error(status: int) -> KimiClientError:
    retryable = status in {408, 425, 429} or 500 <= status <= 599
    if status in {401, 403}:
        return KimiClientError("PROVIDER_CREDENTIAL_REJECTED", "The provider rejected the configured credential.")
    if status == 429:
        return KimiClientError("PROVIDER_RATE_LIMITED", "The provider rate-limited the request.", retryable=True)
    return KimiClientError("PROVIDER_HTTP_ERROR", f"The provider request failed with HTTP status {status}.", retryable=retryable)
