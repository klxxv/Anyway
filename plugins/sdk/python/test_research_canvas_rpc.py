import io
import json
from pathlib import Path
import sys
import unittest

SDK_DIRECTORY = Path(__file__).resolve().parent
if str(SDK_DIRECTORY) not in sys.path:
    sys.path.insert(0, str(SDK_DIRECTORY))

from research_canvas import (
    BlobRef,
    FrameDecoder,
    FrameTooLargeError,
    InlinePayloadTooLargeError,
    ProtocolError,
    RemoteWorkerError,
    WorkerClient,
    WorkerServer,
    HostBusClient,
    MAX_ERROR_MESSAGE_BYTES,
    MAX_EVENTS_PER_REQUEST,
    WorkerTimeoutError,
    decode_frame,
    encode_frame,
)


class ChunkedReader(io.BytesIO):
    def read(self, size=-1):
        return super().read(min(size, 2) if size >= 0 else size)


class FakeTransport:
    def __init__(self, responses):
        self.responses = list(responses)
        self.sent = []

    def send(self, message):
        self.sent.append(message)

    def receive(self, timeout=None):
        if not self.responses:
            raise TimeoutError("fake timeout")
        response = self.responses.pop(0)
        if isinstance(response, Exception):
            raise response
        return response


class DynamicErrorTransport(FakeTransport):
    def __init__(self):
        super().__init__([ack()])

    def receive(self, timeout=None):
        if self.responses:
            return self.responses.pop(0)
        return {
            "type": "response",
            "requestId": self.sent[-1]["requestId"],
            "ok": False,
            "error": {"code": "DENIED", "message": "no"},
        }


class EventFloodTransport(FakeTransport):
    def __init__(self):
        super().__init__([ack()])
        self.receive_timeouts = []

    def receive(self, timeout=None):
        self.receive_timeouts.append(timeout)
        if self.responses:
            return self.responses.pop(0)
        event_count = len(self.receive_timeouts) - 2
        if event_count <= MAX_EVENTS_PER_REQUEST:
            return {"type": "event", "requestId": "pending", "payload": {"n": event_count}}
        return {"type": "response", "requestId": "never", "ok": True, "result": {}}


class CorrelatedResponseTransport(FakeTransport):
    def __init__(self, response):
        super().__init__([ack()])
        self.response = response

    def receive(self, timeout=None):
        if self.responses:
            return self.responses.pop(0)
        message = dict(self.response)
        message["requestId"] = self.sent[-1]["requestId"]
        return message


def ack():
    return {
        "type": "helloAck",
        "apiVersion": "researchcanvas.dev/worker-rpc/v1",
        "workerId": "worker.test",
        "operations": ["ping"],
    }


class WorkerRpcTests(unittest.TestCase):
    def test_codec_handles_partial_reads_and_multiple_frames(self):
        decoder = FrameDecoder()
        data = encode_frame({"type": "event", "n": 1}) + encode_frame({"type": "event", "n": 2})
        messages = []
        for byte in data:
            messages.extend(decoder.feed(bytes([byte])))
        self.assertEqual([message["n"] for message in messages], [1, 2])

    def test_codec_rejects_oversize_invalid_utf8_and_json(self):
        with self.assertRaises(FrameTooLargeError):
            encode_frame({"x": "a" * (1024 * 1024)})
        with self.assertRaises(ProtocolError):
            decode_frame(b"\x00\x00\x00\x01\xff")
        with self.assertRaises(ProtocolError):
            decode_frame(b"\x00\x00\x00\x01{")

    def test_client_correlates_remote_errors_and_cancel(self):
        transport = FakeTransport(
            [
                ack(),
                {"type": "event", "event": "progress"},
                {"type": "response", "requestId": "wrong", "ok": True, "result": {}},
            ]
        )
        client = WorkerClient(
            transport,
            plugin_id="myc.pdf-canvas-agent",
            plugin_version="0.4.0",
            worker_id="worker.test",
            allowed_operations=("ping",),
        )
        client.handshake()
        with self.assertRaises(ProtocolError):
            client.request("ping", {"value": "ok"})
        self.assertEqual(transport.sent[0]["type"], "hello")
        self.assertNotIn("principal", transport.sent[0])

        remote = DynamicErrorTransport()
        remote_client = WorkerClient(remote, plugin_id="p", plugin_version="1", worker_id="worker.test", allowed_operations=("ping",))
        remote_client.handshake()
        with self.assertRaises(RemoteWorkerError) as error:
            remote_client.request("ping", {})
        self.assertEqual(error.exception.code, "DENIED")

    def test_client_timeout_inline_limit_and_blobref(self):
        transport = FakeTransport([ack(), TimeoutError("deadline")])
        client = WorkerClient(transport, plugin_id="p", plugin_version="1", worker_id="worker.test", allowed_operations=("ping",))
        client.handshake()
        with self.assertRaises(WorkerTimeoutError):
            client.request("ping", {"value": "ok"})
        self.assertEqual(transport.sent[-1]["type"], "cancel")

        large = FakeTransport([ack()])
        large_client = WorkerClient(large, plugin_id="p", plugin_version="1", worker_id="worker.test", allowed_operations=("ping",))
        large_client.handshake()
        with self.assertRaises(InlinePayloadTooLargeError):
            large_client.request("ping", {"text": "x" * (64 * 1024)})

        blob = BlobRef("sha256", "a" * 64, 1024, "application/pdf", "private:plugin.a", "plugin.a", "session")
        self.assertEqual(BlobRef.from_mapping(blob.to_mapping()), blob)
        with self.assertRaises(ValueError):
            BlobRef("sha256", "not-a-digest", 1, "application/pdf", "private:plugin.a", "plugin.a", "session")
        with self.assertRaises(ValueError):
            BlobRef.from_mapping({"algorithm": "sha256", "digest": "a" * 64, "size": 1, "mediaType": "application/pdf", "scope": "private:plugin.a", "owner": "plugin.a", "retentionClass": "session", "path": "secret"})
        with self.assertRaises(ValueError):
            BlobRef.from_mapping({"algorithm": "sha256", "digest": "a" * 64, "size": 1, "mediaType": "application/pdf", "scope": "private:plugin.a", "owner": "plugin.a", "retentionClass": "session", "principal": "spoof"})
        with self.assertRaises(ValueError):
            BlobRef("sha256", "a" * 64, 1, "application/pdf", "private:plugin.a", "plugin.b", "session")

    def test_host_blobref_cross_language_fixture(self):
        fixture = Path(__file__).resolve().parents[1] / "fixtures" / "host-blob-ref-v1.json"
        value = json.loads(fixture.read_text(encoding="utf-8"))
        self.assertEqual(BlobRef.from_mapping(value).to_mapping(), value)

    def test_worker_host_bus_client_correlates_and_rejects_authority_fields(self):
        response = encode_frame({
            "type": "hostResponse",
            "apiVersion": "researchcanvas.dev/worker-rpc/v1",
            "parentRequestId": "request-1",
            "hostRequestId": "request-1:host:1",
            "ok": True,
            "result": {"delivered": 1},
        })
        output = io.BytesIO()
        host = HostBusClient(io.BytesIO(response), output, "request-1", __import__("time").monotonic() + 1)
        self.assertEqual(host.call("event.publish", {"topic": "test", "payload": {}}), {"delivered": 1})
        request = decode_frame(output.getvalue())
        self.assertEqual(request["type"], "hostRequest")
        self.assertEqual(request["hostRequestId"], "request-1:host:1")
        with self.assertRaises(ProtocolError):
            host.call("event.publish", {"principal": "spoof"})
        with self.assertRaises(ProtocolError):
            host.call("event.publish", {"capabilityLeaseIds": ["forged"]})

    def test_worker_server_bounds_results_events_and_errors(self):
        server = WorkerServer({"ping": lambda payload: {"ok": True}})
        output = io.BytesIO()
        with self.assertRaises(InlinePayloadTooLargeError):
            server._send_response(output, "request", {"text": "x" * (64 * 1024 + 1)})
        with self.assertRaises(InlinePayloadTooLargeError):
            server.emit_event(output, "request", {"text": "x" * (64 * 1024 + 1)})
        with self.assertRaises(ProtocolError):
            server.emit_event(output, "request", {"blobRef": {"algorithm": "sha256", "digest": "a" * 64, "size": 1, "mediaType": "application/pdf", "scope": "s", "owner": "plugin.a", "retentionClass": "session", "path": "x"}})
        server._send_error(output, "request", "WORKER_ERROR", "x" * (MAX_ERROR_MESSAGE_BYTES * 2))
        output.seek(0)
        error_frame = decode_frame(output.read())
        self.assertLessEqual(len(error_frame["error"]["message"].encode("utf-8")), MAX_ERROR_MESSAGE_BYTES)

    def test_client_event_flood_is_bounded_and_deadline_is_not_reset(self):
        transport = EventFloodTransport()
        client = WorkerClient(transport, plugin_id="p", plugin_version="1", worker_id="worker.test", allowed_operations=("ping",), default_timeout=1.0)
        client.handshake()
        with self.assertRaises(ProtocolError):
            client.request("ping", {}, timeout=0.05)
        self.assertGreaterEqual(len(transport.receive_timeouts), MAX_EVENTS_PER_REQUEST)
        self.assertLessEqual(transport.receive_timeouts[-1], transport.receive_timeouts[1])

    def test_client_rejects_oversized_result_and_error(self):
        transport = CorrelatedResponseTransport({"type": "response", "ok": True, "result": {"text": "x" * (64 * 1024 + 1)}})
        client = WorkerClient(transport, plugin_id="p", plugin_version="1", worker_id="worker.test", allowed_operations=("ping",))
        client.handshake()
        with self.assertRaises(InlinePayloadTooLargeError):
            client.request("ping", {})

        error_transport = CorrelatedResponseTransport({"type": "response", "ok": False, "error": {"code": "E", "message": "x" * (MAX_ERROR_MESSAGE_BYTES + 1)}})
        error_client = WorkerClient(error_transport, plugin_id="p", plugin_version="1", worker_id="worker.test", allowed_operations=("ping",))
        error_client.handshake()
        with self.assertRaises(ProtocolError):
            error_client.request("ping", {})


if __name__ == "__main__":
    unittest.main()
