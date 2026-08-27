//! Review-gated storage for plugin GraphPatch proposals.
//!
//! Plugins may create immutable, canonical proposals. A trusted Host UI can
//! accept or reject one proposal exactly once, but accepted proposals must be
//! committed by a Rust project adapter while the proposal registry is locked.
//! If no project adapter is wired, accept fails with an integration blocker
//! instead of returning a patch for Vue-side application.

use std::collections::{BTreeMap, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::{rngs::OsRng, RngCore as _};
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest as _, Sha256};

#[path = "graph_projects.rs"]
pub mod graph_projects;

const PATCH_API_VERSION: &str = "researchcanvas.dev/graph-patch/v1alpha1";
const MAX_PATCH_BYTES: usize = 48 * 1024;
const MAX_OPERATIONS: usize = 128;
const MAX_PROPOSALS: usize = 512;
const MAX_REVIEWED_PROPOSALS: usize = 128;
const MAX_DEPTH: usize = 10;
const MAX_TEXT_BYTES: usize = 10 * 1024;
const PROPOSAL_TTL_MS: u64 = 30 * 60 * 1000;
const REVIEWED_RECORD_TTL_MS: u64 = 10 * 60 * 1000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalStatus {
    AwaitingReview,
    Accepted,
    Rejected,
    Expired,
}

#[derive(Clone, Debug)]
struct ProposalRecord {
    plugin_id: String,
    plugin_version: String,
    session_id: String,
    project_id: String,
    base_revision: u64,
    digest: String,
    canonical_patch: Value,
    review_projection: Value,
    status: ProposalStatus,
    created_at_ms: u64,
    expires_at_ms: u64,
    reviewed_at_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub struct GraphPatchCommitReceipt {
    pub project_id: String,
    pub base_revision: u64,
    pub new_revision: u64,
    pub commit_id: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct GraphPatchCommitRequest<'a> {
    pub proposal_id: &'a str,
    pub plugin_id: &'a str,
    pub plugin_version: &'a str,
    pub session_id: &'a str,
    pub project_id: &'a str,
    pub base_revision: u64,
    pub digest: &'a str,
    pub canonical_patch: &'a Value,
}

pub trait GraphPatchCommitAdapter {
    fn commit_graph_patch(
        &mut self,
        request: GraphPatchCommitRequest<'_>,
    ) -> Result<GraphPatchCommitReceipt, String>;
}

#[derive(Debug, Default)]
pub struct MissingProjectCommitAdapter;

impl GraphPatchCommitAdapter for MissingProjectCommitAdapter {
    fn commit_graph_patch(
        &mut self,
        _request: GraphPatchCommitRequest<'_>,
    ) -> Result<GraphPatchCommitReceipt, String> {
        Err(
            "GraphPatch commit integration blocker: no Rust project commit adapter is wired; accepting this proposal would fall back to unsafe Vue-side graph apply"
                .to_string(),
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct GraphPatchProposalRegistry {
    proposals: HashMap<String, ProposalRecord>,
}

impl GraphPatchProposalRegistry {
    pub fn propose(
        &mut self,
        plugin_id: &str,
        plugin_version: &str,
        session_id: &str,
        project_id: &str,
        base_revision: u64,
        patch: Value,
    ) -> Result<Value, String> {
        let now_ms = now_ms();
        self.cleanup_expired_and_reviewed(now_ms);
        self.ensure_capacity()?;

        bounded_token(Some(&Value::String(plugin_id.to_string())), 160, "pluginId")?;
        bounded_text(
            Some(&Value::String(plugin_version.to_string())),
            80,
            false,
            "pluginVersion",
        )?;
        bounded_token(
            Some(&Value::String(session_id.to_string())),
            160,
            "sessionId",
        )?;
        bounded_text(
            Some(&Value::String(project_id.to_string())),
            160,
            false,
            "projectId",
        )?;

        let canonical_patch = canonicalize_value(patch);
        validate_graph_patch(&canonical_patch, plugin_id)?;
        validate_project_binding(&canonical_patch, project_id)?;
        let encoded = serde_json::to_vec(&canonical_patch).map_err(|error| error.to_string())?;
        let digest = format!("{:x}", Sha256::digest(&encoded));
        let proposal_id = random_proposal_id();
        let expires_at_ms = now_ms.saturating_add(PROPOSAL_TTL_MS);
        let review_projection = build_review_projection(
            &proposal_id,
            &canonical_patch,
            plugin_id,
            plugin_version,
            session_id,
            project_id,
            base_revision,
            &digest,
            now_ms,
            expires_at_ms,
        )?;

        self.proposals.insert(
            proposal_id.clone(),
            ProposalRecord {
                plugin_id: plugin_id.to_string(),
                plugin_version: plugin_version.to_string(),
                session_id: session_id.to_string(),
                project_id: project_id.to_string(),
                base_revision,
                digest: digest.clone(),
                canonical_patch,
                review_projection: review_projection.clone(),
                status: ProposalStatus::AwaitingReview,
                created_at_ms: now_ms,
                expires_at_ms,
                reviewed_at_ms: None,
            },
        );
        Ok(json!({
            "proposalId": proposal_id,
            "digest": digest,
            "projectId": project_id,
            "baseRevision": base_revision,
            "status": "awaiting-review",
            "reviewRequired": true,
            "expiresAtMs": expires_at_ms,
            "review": review_projection,
        }))
    }

    pub fn get(
        &mut self,
        plugin_id: &str,
        plugin_version: &str,
        session_id: &str,
        project_id: &str,
        base_revision: u64,
        proposal_id: &str,
        expected_digest: &str,
    ) -> Result<Value, String> {
        let now_ms = now_ms();
        self.cleanup_expired_and_reviewed(now_ms);
        let proposal = self
            .proposals
            .get_mut(proposal_id)
            .ok_or_else(|| "GraphPatch proposal was not found".to_string())?;
        ensure_review_binding(
            proposal,
            plugin_id,
            plugin_version,
            session_id,
            project_id,
            base_revision,
            expected_digest,
            now_ms,
        )?;
        Ok(json!({
            "proposalId": proposal_id,
            "digest": proposal.digest,
            "projectId": proposal.project_id,
            "baseRevision": proposal.base_revision,
            "status": status_wire(&proposal.status),
            "createdAtMs": proposal.created_at_ms,
            "expiresAtMs": proposal.expires_at_ms,
            "review": proposal.review_projection,
        }))
    }

    pub fn review_with_commit_adapter<A: GraphPatchCommitAdapter>(
        &mut self,
        plugin_id: &str,
        plugin_version: &str,
        session_id: &str,
        project_id: &str,
        base_revision: u64,
        proposal_id: &str,
        expected_digest: &str,
        accept: bool,
        adapter: &mut A,
    ) -> Result<Value, String> {
        let now_ms = now_ms();
        self.cleanup_expired_and_reviewed(now_ms);
        let proposal = self
            .proposals
            .get_mut(proposal_id)
            .ok_or_else(|| "GraphPatch proposal was not found".to_string())?;
        ensure_review_binding(
            proposal,
            plugin_id,
            plugin_version,
            session_id,
            project_id,
            base_revision,
            expected_digest,
            now_ms,
        )?;
        if !accept {
            proposal.status = ProposalStatus::Rejected;
            proposal.reviewed_at_ms = Some(now_ms);
            return Ok(json!({
                "proposalId": proposal_id,
                "digest": proposal.digest,
                "projectId": proposal.project_id,
                "baseRevision": proposal.base_revision,
                "status": "rejected",
                "reviewedAtMs": now_ms,
            }));
        }

        let commit = adapter.commit_graph_patch(GraphPatchCommitRequest {
            proposal_id,
            plugin_id: &proposal.plugin_id,
            plugin_version: &proposal.plugin_version,
            session_id: &proposal.session_id,
            project_id: &proposal.project_id,
            base_revision: proposal.base_revision,
            digest: &proposal.digest,
            canonical_patch: &proposal.canonical_patch,
        })?;
        if commit.project_id != proposal.project_id
            || commit.base_revision != proposal.base_revision
        {
            return Err(
                "GraphPatch commit adapter returned a receipt for the wrong project revision"
                    .to_string(),
            );
        }
        proposal.status = ProposalStatus::Accepted;
        proposal.reviewed_at_ms = Some(now_ms);
        Ok(json!({
            "proposalId": proposal_id,
            "digest": proposal.digest,
            "projectId": proposal.project_id,
            "baseRevision": proposal.base_revision,
            "newRevision": commit.new_revision,
            "commitId": commit.commit_id,
            "status": "accepted",
            "reviewedAtMs": now_ms,
        }))
    }

    pub fn cleanup_session(
        &mut self,
        plugin_id: &str,
        plugin_version: &str,
        session_id: &str,
    ) -> usize {
        let before = self.proposals.len();
        self.proposals.retain(|_, proposal| {
            !(proposal.plugin_id == plugin_id
                && proposal.plugin_version == plugin_version
                && proposal.session_id == session_id)
        });
        before.saturating_sub(self.proposals.len())
    }

    fn cleanup_expired_and_reviewed(&mut self, now_ms: u64) {
        for proposal in self.proposals.values_mut() {
            if proposal.status == ProposalStatus::AwaitingReview && now_ms >= proposal.expires_at_ms
            {
                proposal.status = ProposalStatus::Expired;
                proposal.reviewed_at_ms = Some(now_ms);
            }
        }
        self.proposals.retain(|_, proposal| {
            proposal.reviewed_at_ms.is_none_or(|reviewed_at| {
                now_ms.saturating_sub(reviewed_at) <= REVIEWED_RECORD_TTL_MS
            })
        });
        while self.reviewed_count() > MAX_REVIEWED_PROPOSALS {
            let Some(id) = self.oldest_reviewed_id() else {
                break;
            };
            self.proposals.remove(&id);
        }
    }

    fn ensure_capacity(&mut self) -> Result<(), String> {
        while self.proposals.len() >= MAX_PROPOSALS {
            let Some(id) = self.oldest_reviewed_id() else {
                return Err("GraphPatch proposal registry limit reached".to_string());
            };
            self.proposals.remove(&id);
        }
        Ok(())
    }

    fn reviewed_count(&self) -> usize {
        self.proposals
            .values()
            .filter(|proposal| proposal.status != ProposalStatus::AwaitingReview)
            .count()
    }

    fn oldest_reviewed_id(&self) -> Option<String> {
        self.proposals
            .iter()
            .filter(|(_, proposal)| proposal.status != ProposalStatus::AwaitingReview)
            .min_by_key(|(_, proposal)| proposal.reviewed_at_ms.unwrap_or(proposal.created_at_ms))
            .map(|(id, _)| id.clone())
    }

    #[cfg(test)]
    fn status(&self, proposal_id: &str) -> Option<ProposalStatus> {
        self.proposals
            .get(proposal_id)
            .map(|proposal| proposal.status.clone())
    }
}

fn ensure_review_binding(
    proposal: &mut ProposalRecord,
    plugin_id: &str,
    plugin_version: &str,
    session_id: &str,
    project_id: &str,
    base_revision: u64,
    expected_digest: &str,
    now_ms: u64,
) -> Result<(), String> {
    if proposal.plugin_id != plugin_id
        || proposal.plugin_version != plugin_version
        || proposal.session_id != session_id
        || proposal.project_id != project_id
    {
        return Err(
            "GraphPatch proposal identity does not match this plugin session and project"
                .to_string(),
        );
    }
    if proposal.base_revision != base_revision {
        return Err(
            "GraphPatch proposal baseRevision does not match the current review request"
                .to_string(),
        );
    }
    if proposal.digest != expected_digest {
        return Err("GraphPatch proposal digest does not match expectedDigest".to_string());
    }
    if proposal.status != ProposalStatus::AwaitingReview {
        return Err("GraphPatch proposal has already been reviewed".to_string());
    }
    if now_ms >= proposal.expires_at_ms {
        proposal.status = ProposalStatus::Expired;
        proposal.reviewed_at_ms = Some(now_ms);
        return Err("GraphPatch proposal has expired".to_string());
    }
    Ok(())
}

fn status_wire(status: &ProposalStatus) -> &'static str {
    match status {
        ProposalStatus::AwaitingReview => "awaiting-review",
        ProposalStatus::Accepted => "accepted",
        ProposalStatus::Rejected => "rejected",
        ProposalStatus::Expired => "expired",
    }
}

fn random_proposal_id() -> String {
    let mut bytes = [0_u8; 24];
    OsRng.fill_bytes(&mut bytes);
    let mut value = String::from("proposal-");
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing to String cannot fail");
    }
    value
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize_value).collect()),
        Value::Object(object) => {
            let sorted = object
                .into_iter()
                .map(|(key, value)| (key, canonicalize_value(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect::<Map<_, _>>())
        }
        other => other,
    }
}

pub fn validate_graph_patch(patch: &Value, expected_plugin_id: &str) -> Result<(), String> {
    let encoded = serde_json::to_vec(patch).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_PATCH_BYTES {
        return Err("GraphPatch exceeds the bounded inline proposal limit".to_string());
    }
    validate_value(patch, 0)?;
    let object = exact_object(
        patch,
        &[
            "apiVersion",
            "source",
            "title",
            "summary",
            "reviewRequired",
            "operations",
        ],
        "GraphPatch",
    )?;
    if object.get("apiVersion").and_then(Value::as_str) != Some(PATCH_API_VERSION)
        || object.get("reviewRequired").and_then(Value::as_bool) != Some(true)
    {
        return Err("GraphPatch must use v1alpha1 and require review".to_string());
    }
    bounded_text(object.get("title"), 500, false, "GraphPatch title")?;
    bounded_text(object.get("summary"), 2_000, true, "GraphPatch summary")?;
    let source = exact_object(
        object
            .get("source")
            .ok_or("GraphPatch source is required")?,
        &["pluginId", "operation", "externalId", "projectId"],
        "GraphPatch source",
    )?;
    if source.get("pluginId").and_then(Value::as_str) != Some(expected_plugin_id) {
        return Err(
            "GraphPatch source pluginId does not match the authenticated worker".to_string(),
        );
    }
    bounded_token(source.get("operation"), 160, "GraphPatch source operation")?;
    for optional in ["externalId", "projectId"] {
        if let Some(value) = source.get(optional) {
            bounded_text(Some(value), 160, false, "GraphPatch source identifier")?;
        }
    }
    let operations = object
        .get("operations")
        .and_then(Value::as_array)
        .ok_or("GraphPatch operations must be an array")?;
    if operations.is_empty() || operations.len() > MAX_OPERATIONS {
        return Err("GraphPatch must contain 1 to 128 operations".to_string());
    }
    for operation in operations {
        validate_operation(operation)?;
    }
    Ok(())
}

fn validate_project_binding(patch: &Value, expected_project_id: &str) -> Result<(), String> {
    let source_project_id = patch
        .get("source")
        .and_then(|source| source.get("projectId"))
        .and_then(Value::as_str);
    if source_project_id.is_some_and(|project_id| project_id != expected_project_id) {
        return Err(
            "GraphPatch source projectId does not match the proposal projectId".to_string(),
        );
    }
    Ok(())
}

fn build_review_projection(
    proposal_id: &str,
    patch: &Value,
    plugin_id: &str,
    plugin_version: &str,
    session_id: &str,
    project_id: &str,
    base_revision: u64,
    digest: &str,
    created_at_ms: u64,
    expires_at_ms: u64,
) -> Result<Value, String> {
    let object = patch
        .as_object()
        .ok_or("GraphPatch must be an object before review projection")?;
    let operations = object
        .get("operations")
        .and_then(Value::as_array)
        .ok_or("GraphPatch operations must be an array before review projection")?;
    Ok(json!({
        "proposalId": proposal_id,
        "digest": digest,
        "pluginId": plugin_id,
        "pluginVersion": plugin_version,
        "sessionId": session_id,
        "projectId": project_id,
        "baseRevision": base_revision,
        "status": "awaiting-review",
        "title": object.get("title").cloned().unwrap_or(Value::Null),
        "summary": object.get("summary").cloned().unwrap_or(Value::Null),
        "operationCount": operations.len(),
        "operations": operations
            .iter()
            .enumerate()
            .map(|(index, operation)| operation_projection(index, operation))
            .collect::<Vec<_>>(),
        "createdAtMs": created_at_ms,
        "expiresAtMs": expires_at_ms,
    }))
}

fn operation_projection(index: usize, operation: &Value) -> Value {
    let object = operation.as_object();
    let op = object
        .and_then(|object| object.get("op"))
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let target = match op {
        "add-node" => object
            .and_then(|object| object.get("node"))
            .map(node_projection)
            .unwrap_or(Value::Null),
        "add-edge" => object
            .and_then(|object| object.get("edge"))
            .map(edge_projection)
            .unwrap_or(Value::Null),
        "update-node" => json!({
            "id": object.and_then(|object| object.get("nodeId")).cloned().unwrap_or(Value::Null),
        }),
        "update-edge" => json!({
            "id": object.and_then(|object| object.get("edgeId")).cloned().unwrap_or(Value::Null),
        }),
        _ => Value::Null,
    };
    json!({
        "index": index,
        "op": op,
        "target": target,
    })
}

fn node_projection(node: &Value) -> Value {
    json!({
        "id": node.get("id").cloned().unwrap_or(Value::Null),
        "type": node.get("type").cloned().unwrap_or(Value::Null),
        "title": node.get("title").cloned().unwrap_or(Value::Null),
    })
}

fn edge_projection(edge: &Value) -> Value {
    json!({
        "id": edge.get("id").cloned().unwrap_or(Value::Null),
        "source": edge.get("source").cloned().unwrap_or(Value::Null),
        "target": edge.get("target").cloned().unwrap_or(Value::Null),
        "type": edge.get("type").cloned().unwrap_or(Value::Null),
    })
}

fn validate_operation(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or("GraphPatch operation must be an object")?;
    match object.get("op").and_then(Value::as_str) {
        Some("add-node") => {
            exact_keys(object, &["op", "node"], "add-node")?;
            validate_node(object.get("node").ok_or("add-node node is required")?)
        }
        Some("add-edge") => {
            exact_keys(object, &["op", "edge"], "add-edge")?;
            validate_edge(object.get("edge").ok_or("add-edge edge is required")?)
        }
        Some("update-node") => {
            exact_keys(object, &["op", "nodeId", "changes"], "update-node")?;
            bounded_token(object.get("nodeId"), 160, "update-node nodeId")?;
            require_object(object.get("changes"), "update-node changes")
        }
        Some("update-edge") => {
            exact_keys(object, &["op", "edgeId", "changes"], "update-edge")?;
            bounded_token(object.get("edgeId"), 160, "update-edge edgeId")?;
            require_object(object.get("changes"), "update-edge changes")
        }
        Some("graph.storage.put") => {
            Err("graph.storage.put is not allowed in GraphPatch".to_string())
        }
        _ => Err("GraphPatch operation type is not allowed".to_string()),
    }
}

fn validate_node(value: &Value) -> Result<(), String> {
    let object = exact_object(
        value,
        &["id", "type", "title", "body", "tags", "data", "provenance"],
        "graph node",
    )?;
    bounded_token(object.get("id"), 160, "node id")?;
    bounded_token(object.get("type"), 40, "node type")?;
    bounded_text(object.get("title"), 500, false, "node title")?;
    if let Some(body) = object.get("body") {
        bounded_text(Some(body), 10_000, true, "node body")?;
    }
    if let Some(tags) = object.get("tags") {
        let tags = tags.as_array().ok_or("node tags must be an array")?;
        if tags.len() > 64 {
            return Err("node tag limit exceeded".to_string());
        }
        for tag in tags {
            bounded_text(Some(tag), 80, false, "node tag")?;
        }
    }
    require_object(object.get("data"), "node data")
}

fn validate_edge(value: &Value) -> Result<(), String> {
    let object = exact_object(
        value,
        &[
            "id",
            "source",
            "target",
            "type",
            "note",
            "data",
            "polarity",
            "confidence",
            "experiment",
            "provenance",
        ],
        "graph edge",
    )?;
    for field in ["id", "source", "target"] {
        bounded_token(object.get(field), 160, "edge identifier")?;
    }
    bounded_token(object.get("type"), 40, "edge type")?;
    if let Some(note) = object.get("note") {
        if !note.is_null() {
            bounded_text(Some(note), 2_000, true, "edge note")?;
        }
    }
    require_object(object.get("data"), "edge data")
}

fn validate_value(value: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("GraphPatch nesting limit exceeded".to_string());
    }
    match value {
        Value::String(text) if text.len() > MAX_TEXT_BYTES => {
            Err("GraphPatch string limit exceeded".to_string())
        }
        Value::Array(items) => {
            for item in items {
                validate_value(item, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            for (key, item) in object {
                let normalized = key
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                if matches!(
                    normalized.as_str(),
                    "accountid"
                        | "apikey"
                        | "authorization"
                        | "chainofthought"
                        | "credential"
                        | "headers"
                        | "identity"
                        | "localpath"
                        | "owner"
                        | "path"
                        | "principal"
                        | "reasoning"
                        | "reasoningcontent"
                        | "secret"
                        | "sessionid"
                        | "token"
                        | "userid"
                ) {
                    return Err(
                        "GraphPatch contains a forbidden identity, path or secret field"
                            .to_string(),
                    );
                }
                validate_value(item, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn exact_object<'a>(
    value: &'a Value,
    allowed: &[&str],
    label: &str,
) -> Result<&'a Map<String, Value>, String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{label} must be an object"))?;
    exact_keys(object, allowed, label)?;
    Ok(object)
}

fn exact_keys(object: &Map<String, Value>, allowed: &[&str], label: &str) -> Result<(), String> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{label} contains unknown fields"));
    }
    Ok(())
}

fn bounded_text(
    value: Option<&Value>,
    limit: usize,
    allow_empty: bool,
    label: &str,
) -> Result<(), String> {
    let text = value
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{label} must be text"))?;
    if (!allow_empty && text.is_empty()) || text.len() > limit || text.chars().any(char::is_control)
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn bounded_token(value: Option<&Value>, limit: usize, label: &str) -> Result<(), String> {
    bounded_text(value, limit, false, label)?;
    let text = value.and_then(Value::as_str).expect("validated text");
    if text.chars().any(char::is_whitespace) {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn require_object(value: Option<&Value>, label: &str) -> Result<(), String> {
    if value.is_none_or(|value| !value.is_object()) {
        return Err(format!("{label} must be an object"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::graph_projects::GraphProjectRegistry;
    use super::*;

    fn patch(plugin_id: &str) -> Value {
        json!({
            "apiVersion": PATCH_API_VERSION,
            "source": {"pluginId": plugin_id, "operation": "extract", "externalId": "job-1", "projectId": "project-a"},
            "title": "Review extraction",
            "summary": "One grounded entity",
            "reviewRequired": true,
            "operations": [{"op":"add-node","node":{"id":"n1","type":"concept","title":"N1","body":"grounded","tags":[],"data":{}}}]
        })
    }

    fn project_with_node(revision: u64) -> Value {
        json!({
            "schemaVersion": 2,
            "id": "project-a",
            "title": "Project A",
            "discipline": "Tests",
            "updatedAt": "2026-08-27T00:00:00.000Z",
            "revision": revision,
            "nodes": [
                {
                    "id": "n1",
                    "type": "concept",
                    "title": "N1",
                    "body": "Grounded",
                    "tags": [],
                    "status": "confirmed",
                    "evidenceIds": [],
                    "data": {},
                    "provenance": {"origin": "human"},
                    "createdAt": "2026-08-27T00:00:00.000Z",
                    "updatedAt": "2026-08-27T00:00:00.000Z"
                }
            ],
            "edges": [],
            "evidence": [],
            "placements": [{
                "id": "placement-n1",
                "viewId": "view-main",
                "nodeId": "n1",
                "x": 0,
                "y": 0,
                "width": 176,
                "height": 118
            }],
            "scenarios": [],
            "activity": []
        })
    }

    #[derive(Default)]
    struct AcceptingAdapter;

    impl GraphPatchCommitAdapter for AcceptingAdapter {
        fn commit_graph_patch(
            &mut self,
            request: GraphPatchCommitRequest<'_>,
        ) -> Result<GraphPatchCommitReceipt, String> {
            Ok(GraphPatchCommitReceipt {
                project_id: request.project_id.to_string(),
                base_revision: request.base_revision,
                new_revision: request.base_revision + 1,
                commit_id: Some("commit-1".to_string()),
            })
        }
    }

    #[test]
    fn proposal_is_bound_reviewed_once_and_accept_uses_commit_adapter() {
        let mut registry = GraphPatchProposalRegistry::default();
        let receipt = registry
            .propose(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                patch("plugin.a"),
            )
            .expect("proposal");
        let id = receipt["proposalId"].as_str().expect("id");
        let digest = receipt["digest"].as_str().expect("digest");
        assert_eq!(receipt["status"], "awaiting-review");
        assert!(receipt.get("patch").is_none());
        assert!(receipt.get("review").is_some());
        assert!(registry
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-b",
                "project-a",
                7,
                id,
                digest,
                true,
                &mut AcceptingAdapter,
            )
            .is_err());
        let accepted = registry
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                id,
                digest,
                true,
                &mut AcceptingAdapter,
            )
            .expect("review");
        assert_eq!(accepted["status"], "accepted");
        assert_eq!(accepted["newRevision"], 8);
        assert!(accepted.get("patch").is_none());
        assert_eq!(registry.status(id), Some(ProposalStatus::Accepted));
        assert!(registry
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                id,
                digest,
                false,
                &mut AcceptingAdapter,
            )
            .is_err());

        let rejected_receipt = registry
            .propose(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                patch("plugin.a"),
            )
            .expect("proposal");
        let rejected_id = rejected_receipt["proposalId"].as_str().unwrap();
        let rejected_digest = rejected_receipt["digest"].as_str().unwrap();
        let rejected = registry
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                rejected_id,
                rejected_digest,
                false,
                &mut AcceptingAdapter,
            )
            .expect("reject");
        assert_eq!(rejected["status"], "rejected");
        assert!(rejected.get("patch").is_none());
    }

    #[test]
    fn missing_project_adapter_blocks_accept_without_consuming_review() {
        let mut registry = GraphPatchProposalRegistry::default();
        let receipt = registry
            .propose(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                patch("plugin.a"),
            )
            .expect("proposal");
        let id = receipt["proposalId"].as_str().expect("id");
        let digest = receipt["digest"].as_str().expect("digest");
        assert!(registry
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                id,
                digest,
                true,
                &mut MissingProjectCommitAdapter,
            )
            .expect_err("missing adapter must block")
            .contains("integration blocker"));
        assert_eq!(registry.status(id), Some(ProposalStatus::AwaitingReview));
    }

    #[test]
    fn real_commit_failure_does_not_consume_proposal_and_replay_is_rejected_after_review() {
        let mut proposals = GraphPatchProposalRegistry::default();
        let mut projects = GraphProjectRegistry::default();
        projects
            .sync("project-a", 7, project_with_node(7))
            .expect("project sync");
        let receipt = proposals
            .propose(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                patch("plugin.a"),
            )
            .expect("proposal");
        let id = receipt["proposalId"].as_str().expect("id");
        let digest = receipt["digest"].as_str().expect("digest");
        assert!(proposals
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                id,
                digest,
                true,
                &mut projects,
            )
            .expect_err("duplicate node commit fails")
            .contains("duplicate"));
        assert_eq!(proposals.status(id), Some(ProposalStatus::AwaitingReview));
        let rejected = proposals
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                id,
                digest,
                false,
                &mut projects,
            )
            .expect("reject after failed accept");
        assert_eq!(rejected["status"], "rejected");
        assert!(proposals
            .review_with_commit_adapter(
                "plugin.a",
                "1.0.0",
                "session-a",
                "project-a",
                7,
                id,
                digest,
                false,
                &mut projects,
            )
            .expect_err("replay rejected")
            .contains("already"));
    }

    #[test]
    fn validation_rejects_identity_spoof_storage_escalation_and_secret_fields() {
        assert!(validate_graph_patch(&patch("plugin.other"), "plugin.a").is_err());
        let mut direct_storage = patch("plugin.a");
        direct_storage["operations"][0]["op"] = json!("graph.storage.put");
        assert!(validate_graph_patch(&direct_storage, "plugin.a").is_err());
        let mut secret = patch("plugin.a");
        secret["operations"][0]["node"]["data"] = json!({"authorization":"Bearer hidden"});
        assert!(validate_graph_patch(&secret, "plugin.a").is_err());
        let mut path = patch("plugin.a");
        path["operations"][0]["node"]["data"] = json!({"localPath":"C:/secret.pdf"});
        assert!(validate_graph_patch(&path, "plugin.a").is_err());
    }
}
