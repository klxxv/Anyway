# Python Worker RPC v1

Rust is the only Host and RPC/Host Bus permission kernel. Python is an
external plugin worker plus a dependency-free protocol adapter; there is no
Python Host. The Host launches one process per installed `pluginId@version`,
binds each surface request to a Host-created session principal, and never
accepts a worker-provided principal, lease, credential, or local path.

## Transport

One child-process stdio connection carries both directions. Each message is a
4-byte unsigned big-endian length followed by compact UTF-8 JSON. stdout is
protocol-only and stderr is diagnostic output.

- Rust → Python: `hello`, `request`, `cancel`, `shutdown`.
- Python → Rust: `helloAck`, `response`, bounded `event`, and `hostRequest`.
- Rust → Python reverse result: `hostResponse`.

Every request/response pair has a correlation ID. A `hostRequest` also carries
its parent worker request ID and its own nested ID. Rust continues servicing
these reverse requests while waiting for the worker response, so a worker can
call a permitted Host Bus operation without deadlocking the same connection.
The parent request owns one absolute monotonic deadline: events and nested Host
calls consume the remaining budget and never reset it. Cancellation gets a
short grace period before the Host kills the process.

The maximum frame is 1 MiB. Inline request, result, and event data is limited
to 64 KiB, error text to 8 KiB, events to 64 per request, and reverse Host calls
to 128 per request. The sole operation-scoped exception is a successful
`blob.read` Host response: it may contain at most 256 KiB of raw bytes encoded
inside a JSON result no larger than 384 KiB. Other Host results retain the
64 KiB inline limit.

The dependency-free local PDF parser remains a 384 KiB acceptance slice. The
`kimi-file-extract` transport bypasses that parser and may read at most 32 MiB
from the Host Blob store using 256 KiB chunks before uploading to Kimi Files.
At the maximum supported size this consumes exactly 128 reverse Host calls and
every base64-encoded response remains below the 1 MiB frame limit. Both paths
inherit the same absolute parent deadline and re-hash the complete Blob before
using it.

## BlobRef v1

Large values use the Host wire shape below. Extra fields inside `blobRef`, such
as `path` or `principal`, are rejected; a containing business object may carry
bounded metadata such as a file label.

```json
{
  "algorithm": "sha256",
  "digest": "64 lowercase hex characters",
  "size": 1024,
  "mediaType": "application/pdf",
  "scope": "private:plugin.example@1.0.0#session",
  "owner": "plugin.example@1.0.0#session",
  "retentionClass": "session"
}
```

`scope` is `shared`, `workspace:<id>`, or `private:<owner>`. For a private
reference, scope owner and `owner` must match. These fields describe the Host
object; they do not grant authority. Rust rebinds the caller from the installed
plugin/version/session and checks Blob Store scope again before reading.

The generic file capability keeps native picker paths inside the Host UI and
Rust command. Rust reads the selected file, creates a private Blob owned by the
current plugin session principal, and sends only `{label, BlobRef}` as the
worker's `file.selected` host action. Paths and file bytes are not copied into
surface model/events/audit.

## Permission and process boundaries

`manifest.worker.operations` allows Rust→worker operations.
`manifest.worker.hostOperations` allows worker→Rust operation names, and each
reverse operation must also map to a declared manifest capability. Rust creates
the short-lived policy grant and lease, performs Host Bus admission, invokes
the registered handler, audits the bound principal, finishes admission, and
revokes the lease. Python cannot submit or widen this authority.

The worker registry holds the global map only long enough to locate an entry.
Each plugin/version has an independent session mutex: calls to one worker are
ordered, while different plugins run concurrently. A failed, timed-out, or
protocol-corrupt process retires only its current generation before a later
request lazily starts a replacement.

The Rust launcher uses no shell and clears inherited environment variables.
For a manifest-declared direct provider it injects only that provider's
configured secret into the exact `pluginId@version` Worker. Non-secret URL,
format, model, and PDF transport settings are Host-constructed `runtimeConfig`
on every request and cannot be overridden by UI payload. A SHA-256
configuration fingerprint retires the old process after credential source,
value, URL, format, model, or reset changes.

This is credential isolation, not an OS network or filesystem sandbox.
`providerEgress` domains are validated declaration/policy metadata; OS-level
egress enforcement is not implemented. The anPdfsolver Worker connects
directly to its declared Kimi endpoint. It still has no direct Blob Store,
Graph Store, Tauri, principal, or lease access: those require admitted reverse
Host operations. Its only graph write-shaped operation is
`graph.patch.propose`, which stores an immutable pending proposal. Trusted Host
UI review is bound to plugin/version/session, is one-time, and only an accepted
Rust response exposes a patch for project application. Workers cannot call
`graph.storage.put`.

## Stable tests

From the repository root:

```text
python -m unittest discover -s plugins/sdk/python -p "test_*.py"
cargo test --manifest-path src-tauri/Cargo.toml --lib python_worker
cargo test --manifest-path src-tauri/Cargo.toml --lib plugin_surfaces::tests
npx tsx --test tests/python-worker-packaged.test.ts
```
