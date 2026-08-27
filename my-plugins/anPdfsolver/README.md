# anPdfsolver

`anPdfsolver` is the source directory for the trusted PDF Canvas plugin. It is
not a Host-side PDF feature. The Host only enumerates plugin manifest
contributions, loads `dist/frontend.mjs`, provides `PluginContext`, and starts
the declared worker through the generic Rust Worker Manager.

## Runtime Flow

1. Host loads the trusted Vue module from `frontend.entry` and calls
   `activate(context)`.
2. Host enumerates `contributes.ui` and mounts exported Vue components into
   physical slots:
   - `workspace.toolbar.actions` -> `AnPdfsolverToolbarButton`
   - `workspace.dialogs` -> `AnPdfsolverDialog`
3. The plugin frontend calls `context.files.pick` and keeps one
   `analysisSessionId` for file selection, worker calls, proposals and review.
   Project binding comes only from contribution `props.hostContext.workspace`.
4. The plugin frontend opens `python-analyzer` with `context.worker.open`.
   Cross-layer operation names remain frozen as:
   `plugin.worker.open`, `plugin.worker.call`, `plugin.worker.cancel`,
   `plugin.worker.close`.
5. The Python worker handles the plugin operation `anpdfsolver.analyze`. It
   reads the Host-authorized `BlobRef` with `blob.read`, performs local PDF text
   parsing or uploads the PDF directly to Kimi Files, streams provider SSE, and
   parses small typed NDJSON frames.
6. The worker deterministically assembles a bounded GraphPatch draft and returns
   it to the plugin frontend. The worker does not call `graph.patch.propose`,
   `graph.patch.review`, `graph.storage.put`, or any provider proxy.
7. The plugin frontend calls `context.graphPatch.propose`, then
   `context.graphPatch.get`. The review UI displays only the Rust canonical
   projection and digest.
8. Accept/reject uses `context.graphPatch.review` with `proposalId`,
   `expectedDigest`, `projectId`, numeric `baseRevision`, `analysisSessionId`,
   and `accept`.

## Frontend Contract

The frontend source lives under `frontend/` and builds as an ESM Vue library:

```text
npm run build:frontend
```

Exports:

- `activate(context)`
- `deactivate()`
- `AnPdfsolverToolbarButton`
- `AnPdfsolverDialog`

Required `PluginContext` APIs:

- `context.files.pick({ accept, multiple, retention })`
- `context.worker.open(workerId, { sessionId, deadlineMs? })`
- `context.worker.call(handle, { sessionId, requestId, operation, payload?, deadlineMs? })`
- `context.worker.cancel(handle, requestId)`
- `context.worker.close(handle)`
- `context.graphPatch.propose({ sessionId, projectId, baseRevision, patch })`
- `context.graphPatch.get({ sessionId, projectId, baseRevision, proposalId, expectedDigest })`
- `context.graphPatch.review({ sessionId, projectId, baseRevision, proposalId, expectedDigest, accept })`
- `context.graphPatch.cleanupSession(sessionId)`
- Optional: `context.settings.read()` and `context.settings.write(values)` for plugin settings.

## Worker Contract

Worker id: `python-analyzer`

Transport: `stdio-framed-json-v1`

Business operations:

- `ping`
- `health`
- `anpdfsolver.analyze`

`anpdfsolver.analyze` input:

```json
{
  "analysisSessionId": "uuid",
  "requestId": "unique-worker-request-id",
  "jobId": "file-or-job-id",
  "projectId": "project-id",
  "baseRevision": 42,
  "file": {
    "label": "paper.pdf",
    "blobRef": {}
  },
  "runtimeConfig": {
    "providers": [
      {
        "id": "kimi",
        "baseUrl": "https://api.moonshot.cn/v1",
        "format": "openai",
        "model": "kimi-k2.6",
        "pdfTransport": "local-text",
        "thinking": "enabled",
        "publicProgress": "disabled",
        "allowedDomains": ["api.moonshot.cn", "api.moonshot.ai"],
        "secretEnv": "ANYWAY_PLUGIN_SECRET_PROVIDER_API_KEY"
      }
    ]
  }
}
```

Output contains sanitized progress, bounded public frames, and `draftPatch`.
The draft is not trusted for review display until Rust returns a canonical
projection from `graphPatch.get`.

## Typed NDJSON

The model must emit one small JSON frame per line. Allowed frame types are:
`progress`, `evidence`, `entity`, `relation`, `operator`, `warning`, `error`,
and `end`. `operator` is kept as a compatibility alias for `relation`.

The worker rejects aggregate document JSON, hidden reasoning, credentials,
headers, local paths, oversized frames, missing sequence numbers and streams
without an `end` frame.

## Removed Legacy Fields

This plugin no longer uses:

- `contributes.uiIr`
- `surface.state`, `surface.action`, `surface.host-action`
- `document.surface`
- `pdf.job.*`
- `graph.patch.review` injection
- Host-owned PDF upload/review components

`engines.worker` remains in `plugin.json` only as a compatibility alias for old
manifest readers. New runtime code must prefer `frontend`, `contributes.ui`,
`workers`, and `network`.

## Handoff Requirements

Host loader/Rust/GraphPatch shards must provide these exact integration points:

- Physical slot ids `workspace.toolbar.actions` and
  `workspace.dialogs`.
- Trusted module loading for `dist/frontend.mjs` with shared Vue 3 singleton.
- `activate/deactivate` lifecycle with component error isolation.
- Generic Worker Manager operations:
  `plugin.worker.open`, `plugin.worker.call`, `plugin.worker.cancel`,
  `plugin.worker.close`.
- BlobRef file picking that returns a strict `blobRef` envelope consumable by
  the Python SDK.
- GraphPatch API methods `propose`, `get`, and `review`, with canonical digest
  binding across plugin id/version, session id, project id and base revision.
