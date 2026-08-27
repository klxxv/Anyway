//! Generic review-gated GraphPatch Host Bus handlers.

use std::sync::RwLock;

use serde::Deserialize;
use serde_json::Value;

use crate::kernel::blob::BlobStore;
use crate::kernel::graph_patches::graph_projects::GraphProjectRegistry;
use crate::kernel::graph_patches::GraphPatchProposalRegistry;
#[cfg(test)]
use crate::kernel::graph_patches::{GraphPatchCommitAdapter, MissingProjectCommitAdapter};
use crate::kernel_commands::{inline_or_blob_json_request, inline_request, HostCallRequest};

const MAX_PROJECT_SYNC_REQUEST_BYTES: usize = 2 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProposeRequest {
    project_id: String,
    base_revision: u64,
    patch: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GetRequest {
    plugin_id: String,
    plugin_version: String,
    session_id: String,
    project_id: String,
    base_revision: u64,
    proposal_id: String,
    expected_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ReviewRequest {
    plugin_id: String,
    plugin_version: String,
    session_id: String,
    project_id: String,
    base_revision: u64,
    proposal_id: String,
    expected_digest: String,
    accept: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CleanupSessionRequest {
    plugin_id: String,
    plugin_version: String,
    session_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectSyncRequest {
    project_id: String,
    revision: u64,
    project: Value,
    session_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectGetRequest {
    project_id: String,
    expected_revision: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectRemoveRequest {
    project_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ProjectCleanupSessionRequest {
    session_id: String,
}

pub(crate) fn dispatch_propose(
    request: &HostCallRequest,
    proposals: &RwLock<GraphPatchProposalRegistry>,
    plugin_id: &str,
    plugin_version: &str,
    session_id: &str,
) -> Result<Value, String> {
    let request = inline_request::<ProposeRequest>(request)
        .map_err(|error| format!("invalid graph.patch.propose request: {error}"))?;
    proposals
        .write()
        .map_err(|_| "GraphPatch proposal registry lock is poisoned".to_string())?
        .propose(
            plugin_id,
            plugin_version,
            session_id,
            &request.project_id,
            request.base_revision,
            request.patch,
        )
}

pub(crate) fn dispatch_get(
    request: &HostCallRequest,
    proposals: &RwLock<GraphPatchProposalRegistry>,
) -> Result<Value, String> {
    let request = inline_request::<GetRequest>(request)
        .map_err(|error| format!("invalid graph.patch.get request: {error}"))?;
    proposals
        .write()
        .map_err(|_| "GraphPatch proposal registry lock is poisoned".to_string())?
        .get(
            &request.plugin_id,
            &request.plugin_version,
            &request.session_id,
            &request.project_id,
            request.base_revision,
            &request.proposal_id,
            &request.expected_digest,
        )
}

#[cfg(test)]
pub(crate) fn dispatch_review(
    request: &HostCallRequest,
    proposals: &RwLock<GraphPatchProposalRegistry>,
) -> Result<Value, String> {
    let mut adapter = MissingProjectCommitAdapter;
    dispatch_review_with_commit_adapter(request, proposals, &mut adapter)
}

#[cfg(test)]
pub(crate) fn dispatch_review_with_commit_adapter<A: GraphPatchCommitAdapter>(
    request: &HostCallRequest,
    proposals: &RwLock<GraphPatchProposalRegistry>,
    adapter: &mut A,
) -> Result<Value, String> {
    let request = inline_request::<ReviewRequest>(request)
        .map_err(|error| format!("invalid graph.patch.review request: {error}"))?;
    proposals
        .write()
        .map_err(|_| "GraphPatch proposal registry lock is poisoned".to_string())?
        .review_with_commit_adapter(
            &request.plugin_id,
            &request.plugin_version,
            &request.session_id,
            &request.project_id,
            request.base_revision,
            &request.proposal_id,
            &request.expected_digest,
            request.accept,
            adapter,
        )
}

pub(crate) fn dispatch_review_with_project_registry(
    request: &HostCallRequest,
    proposals: &RwLock<GraphPatchProposalRegistry>,
    projects: &RwLock<GraphProjectRegistry>,
) -> Result<Value, String> {
    let request = inline_request::<ReviewRequest>(request)
        .map_err(|error| format!("invalid graph.patch.review request: {error}"))?;
    let mut proposals = proposals
        .write()
        .map_err(|_| "GraphPatch proposal registry lock is poisoned".to_string())?;
    let mut projects = projects
        .write()
        .map_err(|_| "GraphProject registry lock is poisoned".to_string())?;
    proposals.review_with_commit_adapter(
        &request.plugin_id,
        &request.plugin_version,
        &request.session_id,
        &request.project_id,
        request.base_revision,
        &request.proposal_id,
        &request.expected_digest,
        request.accept,
        &mut *projects,
    )
}

pub(crate) fn dispatch_cleanup_session(
    request: &HostCallRequest,
    proposals: &RwLock<GraphPatchProposalRegistry>,
) -> Result<Value, String> {
    let request = inline_request::<CleanupSessionRequest>(request)
        .map_err(|error| format!("invalid graph.patch.cleanup-session request: {error}"))?;
    let removed = proposals
        .write()
        .map_err(|_| "GraphPatch proposal registry lock is poisoned".to_string())?
        .cleanup_session(
            &request.plugin_id,
            &request.plugin_version,
            &request.session_id,
        );
    Ok(serde_json::json!({
        "removed": removed,
    }))
}

pub(crate) fn dispatch_project_sync(
    request: &HostCallRequest,
    projects: &RwLock<GraphProjectRegistry>,
    blobs: &RwLock<BlobStore>,
) -> Result<Value, String> {
    let request = inline_or_blob_json_request::<ProjectSyncRequest>(
        request,
        blobs,
        MAX_PROJECT_SYNC_REQUEST_BYTES,
    )
    .map_err(|error| format!("invalid graph.project.sync request: {error}"))?;
    projects
        .write()
        .map_err(|_| "GraphProject registry lock is poisoned".to_string())?
        .sync_with_session(
            &request.project_id,
            request.revision,
            request.project,
            request.session_id.as_deref(),
        )
}

/// Host-shell only: returns the full canonical project JSON for UI/session
/// sync. Do not expose this route to plugin principals.
pub(crate) fn dispatch_project_get(
    request: &HostCallRequest,
    projects: &RwLock<GraphProjectRegistry>,
) -> Result<Value, String> {
    let request = inline_request::<ProjectGetRequest>(request)
        .map_err(|error| format!("invalid graph.project.get request: {error}"))?;
    projects
        .read()
        .map_err(|_| "GraphProject registry lock is poisoned".to_string())?
        .get(&request.project_id, request.expected_revision)
}

pub(crate) fn dispatch_project_remove(
    request: &HostCallRequest,
    projects: &RwLock<GraphProjectRegistry>,
) -> Result<Value, String> {
    let request = inline_request::<ProjectRemoveRequest>(request)
        .map_err(|error| format!("invalid graph.project.remove request: {error}"))?;
    let removed = projects
        .write()
        .map_err(|_| "GraphProject registry lock is poisoned".to_string())?
        .remove(&request.project_id);
    Ok(serde_json::json!({ "removed": removed }))
}

pub(crate) fn dispatch_project_cleanup_session(
    request: &HostCallRequest,
    projects: &RwLock<GraphProjectRegistry>,
) -> Result<Value, String> {
    let request = inline_request::<ProjectCleanupSessionRequest>(request)
        .map_err(|error| format!("invalid graph.project.cleanup-session request: {error}"))?;
    let removed = projects
        .write()
        .map_err(|_| "GraphProject registry lock is poisoned".to_string())?
        .cleanup_session(&request.session_id);
    Ok(serde_json::json!({ "removed": removed }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::kernel::blob::BlobScope;
    use crate::kernel::policy::NATIVE_UI_PRINCIPAL_NAME;
    use crate::kernel_commands::{HOST_SDK_API_VERSION, MAX_INLINE_PAYLOAD_BYTES};

    fn request(operation: &str, value: Value) -> HostCallRequest {
        serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": format!("{operation}-request"),
            "operation": operation,
            "payload": { "kind": "inline", "value": value },
            "deadlineMs": 30_000
        }))
        .expect("valid Host Bus request")
    }

    fn patch() -> Value {
        json!({
            "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
            "source": {
                "pluginId": "myc.pdf-canvas-agent",
                "operation": "pdf-document-extraction",
                "externalId": "job-1",
                "projectId": "project-a"
            },
            "title": "Review extraction",
            "summary": "One source-grounded entity",
            "reviewRequired": true,
            "operations": [{
                "op": "add-node",
                "node": {
                    "id": "entity-1",
                    "type": "concept",
                    "title": "Entity",
                    "body": "Grounded summary",
                    "tags": [],
                    "data": {}
                }
            }]
        })
    }

    fn project(revision: u64) -> Value {
        json!({
            "schemaVersion": 2,
            "id": "project-a",
            "title": "Project A",
            "discipline": "Tests",
            "updatedAt": "2026-08-27T00:00:00.000Z",
            "revision": revision,
            "nodes": [],
            "edges": [],
            "evidence": [],
            "placements": [],
            "scenarios": [],
            "activity": []
        })
    }

    #[test]
    fn host_bus_propose_then_review_returns_only_the_kernel_validated_patch() {
        let proposals = RwLock::new(GraphPatchProposalRegistry::default());
        let receipt = dispatch_propose(
            &request(
                "graph.patch.propose",
                json!({
                    "projectId": "project-a",
                    "baseRevision": 7,
                    "patch": patch()
                }),
            ),
            &proposals,
            "myc.pdf-canvas-agent",
            "0.4.0",
            "surface-session",
        )
        .expect("Host Bus proposal");
        let proposal_id = receipt["proposalId"].as_str().expect("proposal id");
        let digest = receipt["digest"].as_str().expect("digest");
        assert_eq!(receipt["status"], "awaiting-review");
        assert!(receipt.get("patch").is_none());
        assert!(receipt.get("review").is_some());

        let get = request(
            "graph.patch.get",
            json!({
                "pluginId": "myc.pdf-canvas-agent",
                "pluginVersion": "0.4.0",
                "sessionId": "surface-session",
                "projectId": "project-a",
                "baseRevision": 7,
                "proposalId": proposal_id,
                "expectedDigest": digest
            }),
        );
        let fetched = dispatch_get(&get, &proposals).expect("Host get");
        assert_eq!(fetched["review"]["operationCount"], 1);

        let spoofed = request(
            "graph.patch.review",
            json!({
                "pluginId": "myc.pdf-canvas-agent",
                "pluginVersion": "0.4.0",
                "sessionId": "other-session",
                "projectId": "project-a",
                "baseRevision": 7,
                "proposalId": proposal_id,
                "expectedDigest": digest,
                "accept": true
            }),
        );
        assert!(dispatch_review(&spoofed, &proposals).is_err());

        let accept_without_adapter = request(
            "graph.patch.review",
            json!({
                "pluginId": "myc.pdf-canvas-agent",
                "pluginVersion": "0.4.0",
                "sessionId": "surface-session",
                "projectId": "project-a",
                "baseRevision": 7,
                "proposalId": proposal_id,
                "expectedDigest": digest,
                "accept": true
            }),
        );
        assert!(dispatch_review(&accept_without_adapter, &proposals)
            .expect_err("missing Rust project adapter blocks accept")
            .contains("integration blocker"));

        let reject = request(
            "graph.patch.review",
            json!({
                "pluginId": "myc.pdf-canvas-agent",
                "pluginVersion": "0.4.0",
                "sessionId": "surface-session",
                "projectId": "project-a",
                "baseRevision": 7,
                "proposalId": proposal_id,
                "expectedDigest": digest,
                "accept": false
            }),
        );
        let rejected = dispatch_review(&reject, &proposals).expect("Host review");
        assert_eq!(rejected["status"], "rejected");
        assert!(rejected.get("patch").is_none());
        assert!(dispatch_review(&reject, &proposals).is_err());
    }

    #[test]
    fn host_bus_project_sync_get_and_real_review_commit_are_atomic() {
        let proposals = RwLock::new(GraphPatchProposalRegistry::default());
        let projects = RwLock::new(GraphProjectRegistry::default());
        let blobs = RwLock::new(
            crate::kernel::blob::BlobStore::new(crate::kernel::blob::BlobQuota::default())
                .expect("blob store"),
        );
        let synced = dispatch_project_sync(
            &request(
                "graph.project.sync",
                json!({
                    "projectId": "project-a",
                    "revision": 7,
                    "sessionId": "surface-session",
                    "project": project(7)
                }),
            ),
            &projects,
            &blobs,
        )
        .expect("project sync");
        assert_eq!(synced["revision"], 7);

        let receipt = dispatch_propose(
            &request(
                "graph.patch.propose",
                json!({
                    "projectId": "project-a",
                    "baseRevision": 7,
                    "patch": patch()
                }),
            ),
            &proposals,
            "myc.pdf-canvas-agent",
            "0.4.0",
            "surface-session",
        )
        .expect("Host Bus proposal");
        let proposal_id = receipt["proposalId"].as_str().expect("proposal id");
        let digest = receipt["digest"].as_str().expect("digest");
        let review = request(
            "graph.patch.review",
            json!({
                "pluginId": "myc.pdf-canvas-agent",
                "pluginVersion": "0.4.0",
                "sessionId": "surface-session",
                "projectId": "project-a",
                "baseRevision": 7,
                "proposalId": proposal_id,
                "expectedDigest": digest,
                "accept": true
            }),
        );
        let accepted =
            dispatch_review_with_project_registry(&review, &proposals, &projects).expect("accept");
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(accepted["newRevision"], 8);
        assert!(accepted.get("patch").is_none());
        assert!(dispatch_review_with_project_registry(&review, &proposals, &projects).is_err());

        let fetched = dispatch_project_get(
            &request(
                "graph.project.get",
                json!({
                    "projectId": "project-a",
                    "expectedRevision": 8
                }),
            ),
            &projects,
        )
        .expect("host-only get");
        assert_eq!(fetched["project"]["revision"], 8);
    }

    #[test]
    fn host_bus_project_sync_accepts_bounded_blob_json_above_inline_limit() {
        let projects = RwLock::new(GraphProjectRegistry::default());
        let blobs = RwLock::new(
            crate::kernel::blob::BlobStore::new(crate::kernel::blob::BlobQuota::default())
                .expect("blob store"),
        );
        let nodes = (0..900)
            .map(|index| {
                json!({
                    "id": format!("node-{index}"),
                    "type": "concept",
                    "title": format!("Node {index}"),
                    "body": "A bounded body that makes this realistic project exceed the inline Host SDK limit.",
                    "tags": [],
                    "data": {}
                })
            })
            .collect::<Vec<_>>();
        let payload = json!({
            "projectId": "project-large",
            "revision": 4,
            "project": {
                "schemaVersion": 2,
                "id": "project-large",
                "title": "Large project",
                "discipline": "Tests",
                "updatedAt": "2026-08-27T00:00:00.000Z",
                "revision": 4,
                "nodes": nodes,
                "edges": [],
                "evidence": [],
                "placements": [],
                "scenarios": [],
                "activity": []
            }
        });
        let bytes = serde_json::to_vec(&payload).expect("JSON payload");
        assert!(bytes.len() > MAX_INLINE_PAYLOAD_BYTES);

        let reference = {
            let mut store = blobs.write().expect("blob store");
            let upload = store
                .begin_upload(
                    NATIVE_UI_PRINCIPAL_NAME,
                    BlobScope::private(NATIVE_UI_PRINCIPAL_NAME).expect("private scope"),
                    "application/json",
                    bytes.len() as u64,
                    1,
                    60_000,
                )
                .expect("begin upload");
            store
                .upload_chunk(upload, NATIVE_UI_PRINCIPAL_NAME, &bytes, 1)
                .expect("upload JSON");
            store
                .commit_upload(upload, NATIVE_UI_PRINCIPAL_NAME, 1)
                .expect("commit JSON")
        };
        let request: HostCallRequest = serde_json::from_value(json!({
            "apiVersion": HOST_SDK_API_VERSION,
            "requestId": "graph-project-blob-sync",
            "operation": "graph.project.sync",
            "payload": {
                "kind": "blob",
                "ref": {
                    "algorithm": "sha256",
                    "digest": reference.digest().to_hex(),
                    "size": reference.size(),
                    "mediaType": reference.media_type(),
                    "scope": reference.scope().to_wire(),
                    "owner": NATIVE_UI_PRINCIPAL_NAME,
                    "retentionClass": "request"
                }
            },
            "deadlineMs": 30_000
        }))
        .expect("blob Host request");

        let synced = dispatch_project_sync(&request, &projects, &blobs).expect("blob sync");
        assert_eq!(synced["projectId"], "project-large");
        assert_eq!(synced["revision"], 4);
        assert_eq!(
            projects
                .read()
                .expect("projects")
                .get("project-large", Some(4))
                .expect("synced project")["project"]["nodes"]
                .as_array()
                .expect("nodes")
                .len(),
            900
        );
    }
}
