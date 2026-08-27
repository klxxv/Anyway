//! Canonical in-memory project registry for review-gated GraphPatch commits.
//!
//! This module is intentionally independent from `KernelState` wiring. The
//! integration layer can store one `GraphProjectRegistry` beside the proposal
//! registry and pass it as the commit adapter during `graph.patch.review`.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};
use sha2::{Digest as _, Sha256};

use super::{
    canonicalize_value, GraphPatchCommitAdapter, GraphPatchCommitReceipt, GraphPatchCommitRequest,
};

const MAX_PROJECT_BYTES: usize = 2 * 1024 * 1024;
const MAX_PROJECT_DEPTH: usize = 16;
const MAX_PROJECTS: usize = 64;
const MAX_NODES: usize = 10_000;
const MAX_EDGES: usize = 20_000;
const MAX_TEXT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
struct GraphProjectRecord {
    project_id: String,
    revision: u64,
    digest: String,
    project: Value,
    session_id: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct GraphProjectRegistry {
    projects: HashMap<String, GraphProjectRecord>,
}

impl GraphProjectRegistry {
    pub fn sync(
        &mut self,
        project_id: &str,
        revision: u64,
        project: Value,
    ) -> Result<Value, String> {
        self.sync_with_session(project_id, revision, project, None)
    }

    pub fn sync_with_session(
        &mut self,
        project_id: &str,
        revision: u64,
        project: Value,
        session_id: Option<&str>,
    ) -> Result<Value, String> {
        if !self.projects.contains_key(project_id) && self.projects.len() >= MAX_PROJECTS {
            return Err("GraphProject registry limit reached".to_string());
        }
        validate_project_id(project_id)?;
        if let Some(session_id) = session_id {
            validate_token(session_id, 160, "sessionId")?;
        }

        let canonical_project = canonicalize_value(project);
        validate_project_state(&canonical_project, project_id, revision)?;
        let digest = digest_json(&canonical_project)?;

        if let Some(existing) = self.projects.get(project_id) {
            if revision < existing.revision {
                return Err("GraphProject sync rejected an older revision".to_string());
            }
            if revision == existing.revision {
                if digest != existing.digest {
                    return Err(
                        "GraphProject sync rejected different content for the same revision"
                            .to_string(),
                    );
                }
                return Ok(project_receipt(existing));
            }
        }

        self.projects.insert(
            project_id.to_string(),
            GraphProjectRecord {
                project_id: project_id.to_string(),
                revision,
                digest: digest.clone(),
                project: canonical_project,
                session_id: session_id.map(str::to_string),
            },
        );
        Ok(json!({
            "projectId": project_id,
            "revision": revision,
            "digest": digest,
            "status": "synced",
        }))
    }

    /// Host-shell only: returns the full canonical project JSON for local UI
    /// sync. Do not expose this handler to plugin principals.
    pub fn get(&self, project_id: &str, expected_revision: Option<u64>) -> Result<Value, String> {
        validate_project_id(project_id)?;
        let project = self
            .projects
            .get(project_id)
            .ok_or_else(|| "GraphProject was not found".to_string())?;
        if expected_revision.is_some_and(|revision| revision != project.revision) {
            return Err("GraphProject expectedRevision does not match".to_string());
        }
        Ok(json!({
            "projectId": project.project_id,
            "revision": project.revision,
            "digest": project.digest,
            "project": project.project,
        }))
    }

    pub fn remove(&mut self, project_id: &str) -> bool {
        self.projects.remove(project_id).is_some()
    }

    pub fn cleanup_session(&mut self, session_id: &str) -> usize {
        let before = self.projects.len();
        self.projects.retain(|_, project| {
            project
                .session_id
                .as_deref()
                .is_none_or(|stored| stored != session_id)
        });
        before.saturating_sub(self.projects.len())
    }
}

impl GraphPatchCommitAdapter for GraphProjectRegistry {
    fn commit_graph_patch(
        &mut self,
        request: GraphPatchCommitRequest<'_>,
    ) -> Result<GraphPatchCommitReceipt, String> {
        validate_project_id(request.project_id)?;
        let record = self
            .projects
            .get(request.project_id)
            .ok_or_else(|| "GraphProject was not found for GraphPatch commit".to_string())?;
        if record.revision != request.base_revision {
            return Err(
                "GraphPatch baseRevision does not match the current project revision".to_string(),
            );
        }

        let mut next_project = record.project.clone();
        apply_patch_to_project(
            &mut next_project,
            request.canonical_patch,
            request.proposal_id,
            request.plugin_id,
        )?;
        let next_revision = request.base_revision.saturating_add(1);
        set_project_revision(&mut next_project, next_revision)?;
        validate_project_state(&next_project, request.project_id, next_revision)?;
        let next_digest = digest_json(&next_project)?;
        let commit_id = commit_id(request, &next_project)?;

        self.projects.insert(
            request.project_id.to_string(),
            GraphProjectRecord {
                project_id: request.project_id.to_string(),
                revision: next_revision,
                digest: next_digest,
                project: next_project,
                session_id: record.session_id.clone(),
            },
        );

        Ok(GraphPatchCommitReceipt {
            project_id: request.project_id.to_string(),
            base_revision: request.base_revision,
            new_revision: next_revision,
            commit_id: Some(commit_id),
        })
    }
}

fn apply_patch_to_project(
    project: &mut Value,
    patch: &Value,
    proposal_id: &str,
    plugin_id: &str,
) -> Result<(), String> {
    let operations = patch
        .get("operations")
        .and_then(Value::as_array)
        .ok_or("GraphPatch operations must be an array")?;
    for operation in operations {
        apply_operation(project, operation, patch, plugin_id)?;
        validate_graph_references(project)?;
    }
    let created_at = project_timestamp(project);
    if let Some(activity) = project.get_mut("activity").and_then(Value::as_array_mut) {
        activity.push(json!({
            "id": format!("activity-{proposal_id}"),
            "label": patch.get("title").and_then(Value::as_str).unwrap_or("Apply reviewed GraphPatch"),
            "origin": "import",
            "createdAt": created_at,
        }));
    }
    Ok(())
}

fn apply_operation(
    project: &mut Value,
    operation: &Value,
    patch: &Value,
    plugin_id: &str,
) -> Result<(), String> {
    let object = operation
        .as_object()
        .ok_or("GraphPatch operation must be an object")?;
    match object.get("op").and_then(Value::as_str) {
        Some("add-node") => add_node(
            project,
            object
                .get("node")
                .ok_or("add-node operation requires node")?
                .clone(),
            patch,
            plugin_id,
        ),
        Some("add-edge") => add_edge(
            project,
            object
                .get("edge")
                .ok_or("add-edge operation requires edge")?
                .clone(),
            plugin_id,
        ),
        Some("update-node") => update_item(
            project,
            "nodes",
            object.get("nodeId").and_then(Value::as_str),
            object.get("changes"),
            &["type", "title", "body", "tags", "data", "provenance"],
        ),
        Some("update-edge") => update_item(
            project,
            "edges",
            object.get("edgeId").and_then(Value::as_str),
            object.get("changes"),
            &[
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
        ),
        Some("graph.storage.put") => {
            Err("graph.storage.put is not allowed in GraphPatch".to_string())
        }
        _ => Err("GraphPatch operation type is not allowed".to_string()),
    }
}

fn add_node(
    project: &mut Value,
    node: Value,
    patch: &Value,
    plugin_id: &str,
) -> Result<(), String> {
    reject_dangerous_value(&node, 0)?;
    let node_id = item_id(&node, "node")?.to_string();
    if find_index(project, "nodes", &node_id)?.is_some() {
        return Err("GraphPatch add-node rejected a duplicate node id".to_string());
    }
    let node_type = node
        .get("type")
        .and_then(Value::as_str)
        .ok_or("GraphPatch node type is required")?;
    if !matches!(
        node_type,
        "question"
            | "concept"
            | "variable"
            | "hypothesis"
            | "method"
            | "evidence"
            | "paper"
            | "dataset"
            | "experiment"
            | "result"
            | "metric"
            | "formula"
            | "artifact"
            | "note"
    ) {
        return Err("GraphPatch node type is not supported by ProjectState".to_string());
    }
    let title = node
        .get("title")
        .and_then(Value::as_str)
        .ok_or("GraphPatch node title is required")?;
    let body = node
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or("Imported through a reviewed plugin GraphPatch.");
    let tags = node.get("tags").cloned().unwrap_or_else(|| json!([]));
    let data = node.get("data").cloned().unwrap_or_else(|| json!({}));
    let source_ref = patch
        .get("source")
        .and_then(|source| source.get("externalId"))
        .and_then(Value::as_str);
    let timestamp = project_timestamp(project);
    let index = project_array(project, "nodes")?.len();
    let canonical_node = json!({
        "id": node_id,
        "type": node_type,
        "title": title,
        "body": body,
        "tags": tags,
        "status": "draft",
        "evidenceIds": [],
        "data": data,
        "provenance": {
            "origin": "import",
            "actorId": plugin_id,
            "sourceRefs": source_ref.into_iter().collect::<Vec<_>>(),
        },
        "createdAt": timestamp,
        "updatedAt": timestamp,
    });
    project_array_mut(project, "nodes")?.push(canonicalize_value(canonical_node));
    if let Some(placements) = project.get_mut("placements").and_then(Value::as_array_mut) {
        placements.push(json!({
            "id": format!("placement-{node_id}"),
            "viewId": "view-main",
            "nodeId": node_id,
            "x": 120 + (index % 5) * 220,
            "y": 140 + (index / 5) * 160,
            "width": if node_type == "question" { 136 } else { 176 },
            "height": if node_type == "question" { 136 } else { 118 },
        }));
    }
    Ok(())
}

fn add_edge(project: &mut Value, edge: Value, plugin_id: &str) -> Result<(), String> {
    reject_dangerous_value(&edge, 0)?;
    let edge_id = item_id(&edge, "edge")?.to_string();
    if find_index(project, "edges", &edge_id)?.is_some() {
        return Err("GraphPatch add-edge rejected a duplicate edge id".to_string());
    }
    let source = edge
        .get("source")
        .and_then(Value::as_str)
        .ok_or("GraphPatch edge source is required")?;
    let target = edge
        .get("target")
        .and_then(Value::as_str)
        .ok_or("GraphPatch edge target is required")?;
    ensure_node_exists(project, source)?;
    ensure_node_exists(project, target)?;
    let edge_type = edge
        .get("type")
        .and_then(Value::as_str)
        .ok_or("GraphPatch edge type is required")?;
    if !matches!(edge_type, "T" | "K" | "I" | "M" | "Q") {
        return Err("GraphPatch edge type is not supported by ProjectState".to_string());
    }
    let canonical_edge = json!({
        "id": edge_id,
        "type": edge_type,
        "source": source,
        "target": target,
        "directed": true,
        "polarity": edge.get("polarity").cloned().unwrap_or_else(|| json!("positive")),
        "confidence": edge.get("confidence").cloned().unwrap_or(Value::Null),
        "conditions": [],
        "evidenceIds": [],
        "note": edge.get("note").cloned().unwrap_or(Value::Null),
        "experiment": edge.get("experiment").cloned().unwrap_or(Value::Null),
        "provenance": { "origin": "import", "actorId": plugin_id },
    });
    project_array_mut(project, "edges")?.push(canonicalize_value(canonical_edge));
    Ok(())
}

fn project_timestamp(project: &Value) -> String {
    project
        .get("updatedAt")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("1970-01-01T00:00:00.000Z")
        .to_string()
}

fn update_item(
    project: &mut Value,
    collection: &str,
    item_id: Option<&str>,
    changes: Option<&Value>,
    allowed_changes: &[&str],
) -> Result<(), String> {
    let item_id = item_id.ok_or("GraphPatch update operation requires an id")?;
    let changes = changes
        .and_then(Value::as_object)
        .ok_or("GraphPatch update changes must be an object")?;
    if changes.is_empty() {
        return Err("GraphPatch update changes must not be empty".to_string());
    }
    reject_unknown_keys(changes, allowed_changes, "GraphPatch update changes")?;
    reject_dangerous_value(&Value::Object(changes.clone()), 0)?;
    let index = find_index(project, collection, item_id)?
        .ok_or_else(|| format!("GraphPatch update rejected missing {collection} item"))?;
    let item = project_array_mut(project, collection)?
        .get_mut(index)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("GraphProject {collection} item must be an object"))?;
    for (key, value) in changes {
        item.insert(key.clone(), canonicalize_value(value.clone()));
    }
    if collection == "edges" {
        let edge = Value::Object(item.clone());
        let source = edge
            .get("source")
            .and_then(Value::as_str)
            .ok_or("GraphPatch edge source is required")?;
        let target = edge
            .get("target")
            .and_then(Value::as_str)
            .ok_or("GraphPatch edge target is required")?;
        ensure_node_exists(project, source)?;
        ensure_node_exists(project, target)?;
    }
    Ok(())
}

fn validate_project_state(project: &Value, project_id: &str, revision: u64) -> Result<(), String> {
    let encoded = serde_json::to_vec(project).map_err(|error| error.to_string())?;
    if encoded.len() > MAX_PROJECT_BYTES {
        return Err("GraphProject exceeds the bounded project size limit".to_string());
    }
    validate_project_id(project_id)?;
    reject_dangerous_value(project, 0)?;
    let object = project
        .as_object()
        .ok_or("GraphProject state must be an object")?;
    if object.get("schemaVersion").and_then(Value::as_u64) != Some(2) {
        return Err("GraphProject schemaVersion must be 2".to_string());
    }
    if object.get("id").and_then(Value::as_str) != Some(project_id) {
        return Err("GraphProject project.id does not match projectId".to_string());
    }
    if object.get("revision").and_then(Value::as_u64) != Some(revision) {
        return Err("GraphProject project.revision does not match revision".to_string());
    }
    let nodes = project_array(project, "nodes")?;
    let edges = project_array(project, "edges")?;
    for field in ["title", "discipline", "updatedAt"] {
        if object.get(field).and_then(Value::as_str).is_none() {
            return Err(format!("GraphProject {field} must be a string"));
        }
    }
    for field in ["evidence", "placements", "scenarios", "activity"] {
        if object.get(field).and_then(Value::as_array).is_none() {
            return Err(format!("GraphProject {field} must be an array"));
        }
    }
    if nodes.len() > MAX_NODES {
        return Err("GraphProject node limit exceeded".to_string());
    }
    if edges.len() > MAX_EDGES {
        return Err("GraphProject edge limit exceeded".to_string());
    }
    validate_unique_ids(nodes, "node")?;
    validate_unique_ids(edges, "edge")?;
    validate_project_nodes(nodes)?;
    validate_project_edges(edges)?;
    validate_graph_references(project)?;
    validate_placements(project)
}

fn validate_project_nodes(nodes: &[Value]) -> Result<(), String> {
    for node in nodes {
        let object = node
            .as_object()
            .ok_or("GraphProject node must be an object")?;
        let node_type = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or("GraphProject node type is required")?;
        if !matches!(
            node_type,
            "question"
                | "concept"
                | "variable"
                | "hypothesis"
                | "method"
                | "evidence"
                | "paper"
                | "dataset"
                | "experiment"
                | "result"
                | "metric"
                | "formula"
                | "artifact"
                | "note"
        ) {
            return Err("GraphProject node type is not supported".to_string());
        }
        for field in ["title", "body"] {
            if object.get(field).and_then(Value::as_str).is_none() {
                return Err(format!("GraphProject node {field} must be a string"));
            }
        }
        let tags = object
            .get("tags")
            .and_then(Value::as_array)
            .ok_or("GraphProject node tags must be an array")?;
        if tags.iter().any(|tag| !tag.is_string()) {
            return Err("GraphProject node tags must contain strings".to_string());
        }
        if !object.get("data").is_some_and(Value::is_object) {
            return Err("GraphProject node data must be an object".to_string());
        }
    }
    Ok(())
}

fn validate_project_edges(edges: &[Value]) -> Result<(), String> {
    for edge in edges {
        let edge_type = edge
            .get("type")
            .and_then(Value::as_str)
            .ok_or("GraphProject edge type is required")?;
        if !matches!(edge_type, "T" | "K" | "I" | "M" | "Q") {
            return Err("GraphProject edge type is not supported".to_string());
        }
    }
    Ok(())
}

fn validate_placements(project: &Value) -> Result<(), String> {
    let node_ids = project_array(project, "nodes")?
        .iter()
        .map(|node| item_id(node, "node").map(str::to_string))
        .collect::<Result<HashSet<_>, _>>()?;
    let placements = project_array(project, "placements")?;
    validate_unique_ids(placements, "placement")?;
    for placement in placements {
        let node_id = placement
            .get("nodeId")
            .and_then(Value::as_str)
            .ok_or("GraphProject placement nodeId is required")?;
        if !node_ids.contains(node_id) {
            return Err("GraphProject placement references a missing node".to_string());
        }
        for field in ["x", "y", "width", "height"] {
            let coordinate = placement
                .get(field)
                .and_then(Value::as_f64)
                .ok_or_else(|| format!("GraphProject placement {field} must be a number"))?;
            if !coordinate.is_finite() {
                return Err(format!("GraphProject placement {field} must be finite"));
            }
        }
    }
    Ok(())
}

fn validate_graph_references(project: &Value) -> Result<(), String> {
    let node_ids = project_array(project, "nodes")?
        .iter()
        .map(|node| item_id(node, "node").map(str::to_string))
        .collect::<Result<HashSet<_>, _>>()?;
    for edge in project_array(project, "edges")? {
        let source = edge
            .get("source")
            .and_then(Value::as_str)
            .ok_or("GraphProject edge source is required")?;
        let target = edge
            .get("target")
            .and_then(Value::as_str)
            .ok_or("GraphProject edge target is required")?;
        if !node_ids.contains(source) || !node_ids.contains(target) {
            return Err("GraphProject contains a dangling edge".to_string());
        }
    }
    Ok(())
}

fn validate_unique_ids(items: &[Value], label: &str) -> Result<(), String> {
    let mut ids = HashSet::new();
    for item in items {
        let id = item_id(item, label)?;
        if !ids.insert(id.to_string()) {
            return Err(format!("GraphProject contains duplicate {label} ids"));
        }
    }
    Ok(())
}

fn project_array<'a>(project: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    project
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("GraphProject {key} must be an array"))
}

fn project_array_mut<'a>(project: &'a mut Value, key: &str) -> Result<&'a mut Vec<Value>, String> {
    project
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("GraphProject {key} must be an array"))
}

fn find_index(project: &Value, collection: &str, id: &str) -> Result<Option<usize>, String> {
    Ok(project_array(project, collection)?
        .iter()
        .position(|item| item.get("id").and_then(Value::as_str) == Some(id)))
}

fn ensure_node_exists(project: &Value, id: &str) -> Result<(), String> {
    if find_index(project, "nodes", id)?.is_none() {
        return Err("GraphPatch edge references a missing node".to_string());
    }
    Ok(())
}

fn item_id<'a>(item: &'a Value, label: &str) -> Result<&'a str, String> {
    let id = item
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("GraphProject {label} id is required"))?;
    validate_token(id, 160, &format!("{label} id"))?;
    Ok(id)
}

fn set_project_revision(project: &mut Value, revision: u64) -> Result<(), String> {
    let object = project
        .as_object_mut()
        .ok_or("GraphProject state must be an object")?;
    object.insert("revision".to_string(), Value::from(revision));
    Ok(())
}

fn project_receipt(project: &GraphProjectRecord) -> Value {
    json!({
        "projectId": project.project_id,
        "revision": project.revision,
        "digest": project.digest,
        "status": "synced",
    })
}

fn digest_json(value: &Value) -> Result<String, String> {
    let encoded = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(&encoded)))
}

fn commit_id(request: GraphPatchCommitRequest<'_>, project: &Value) -> Result<String, String> {
    let commit_material = json!({
        "proposalId": request.proposal_id,
        "pluginId": request.plugin_id,
        "pluginVersion": request.plugin_version,
        "sessionId": request.session_id,
        "projectId": request.project_id,
        "baseRevision": request.base_revision,
        "digest": request.digest,
        "project": project,
    });
    let digest = digest_json(&commit_material)?;
    Ok(format!("graphpatch-{}", &digest[..24]))
}

fn validate_project_id(value: &str) -> Result<(), String> {
    validate_token(value, 160, "projectId")
}

fn validate_token(value: &str, limit: usize, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > limit
        || value.chars().any(char::is_control)
        || value.chars().any(char::is_whitespace)
    {
        return Err(format!("{label} is invalid"));
    }
    Ok(())
}

fn reject_unknown_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<(), String> {
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("{label} contains unknown fields"));
    }
    Ok(())
}

fn reject_dangerous_value(value: &Value, depth: usize) -> Result<(), String> {
    if depth > MAX_PROJECT_DEPTH {
        return Err("GraphProject nesting limit exceeded".to_string());
    }
    match value {
        Value::String(text) if text.len() > MAX_TEXT_BYTES => {
            Err("GraphProject string limit exceeded".to_string())
        }
        Value::Array(items) => {
            for item in items {
                reject_dangerous_value(item, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(object) => {
            for (key, value) in object {
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
                        "GraphProject contains a forbidden identity, path or secret field"
                            .to_string(),
                    );
                }
                reject_dangerous_value(value, depth + 1)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(revision: u64) -> Value {
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
                    "title": "One",
                    "body": "One body",
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

    fn patch(op: Value) -> Value {
        json!({
            "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
            "source": {"pluginId": "plugin.a", "operation": "extract", "projectId": "project-a"},
            "title": "Review extraction",
            "summary": "One grounded entity",
            "reviewRequired": true,
            "operations": [op]
        })
    }

    fn request<'a>(base_revision: u64, patch: &'a Value) -> GraphPatchCommitRequest<'a> {
        GraphPatchCommitRequest {
            proposal_id: "proposal-a",
            plugin_id: "plugin.a",
            plugin_version: "1.0.0",
            session_id: "session-a",
            project_id: "project-a",
            base_revision,
            digest: "digest-a",
            canonical_patch: patch,
        }
    }

    #[test]
    fn sync_accepts_same_revision_only_with_same_content_and_newer_revision() {
        let mut registry = GraphProjectRegistry::default();
        registry
            .sync("project-a", 1, project(1))
            .expect("initial sync");
        registry
            .sync("project-a", 1, project(1))
            .expect("same content same revision");
        let different_same_revision = json!({
            "schemaVersion": 2,
            "id": "project-a",
            "title": "Different",
            "discipline": "Tests",
            "updatedAt": "2026-08-27T00:00:00.000Z",
            "revision": 1,
            "nodes": [],
            "edges": [],
            "evidence": [],
            "placements": [],
            "scenarios": [],
            "activity": []
        });
        assert!(registry
            .sync("project-a", 1, different_same_revision)
            .expect_err("different content must be rejected")
            .contains("same revision"));
        registry
            .sync("project-a", 2, project(2))
            .expect("newer sync accepted");
        assert!(registry
            .sync("project-a", 1, project(1))
            .expect_err("older sync rejected")
            .contains("older revision"));
    }

    #[test]
    fn commit_applies_patch_atomically_and_increments_revision() {
        let mut registry = GraphProjectRegistry::default();
        registry
            .sync("project-a", 1, project(1))
            .expect("initial sync");
        let graph_patch = patch(json!({
            "op": "add-node",
            "node": {"id": "n2", "type": "concept", "title": "Two", "data": {}}
        }));
        let receipt = registry
            .commit_graph_patch(request(1, &graph_patch))
            .expect("commit");
        assert_eq!(receipt.new_revision, 2);
        assert!(receipt.commit_id.unwrap().starts_with("graphpatch-"));
        let project = registry.get("project-a", Some(2)).expect("project");
        assert_eq!(project["project"]["revision"], 2);
        assert_eq!(
            project["project"]["nodes"].as_array().expect("nodes").len(),
            2
        );
    }

    #[test]
    fn commit_rejects_revision_conflict_duplicate_and_dangling_edge() {
        let mut registry = GraphProjectRegistry::default();
        registry
            .sync("project-a", 1, project(1))
            .expect("initial sync");
        let add_node = patch(json!({
            "op": "add-node",
            "node": {"id": "n2", "type": "concept", "title": "Two", "data": {}}
        }));
        assert!(registry
            .commit_graph_patch(request(0, &add_node))
            .expect_err("revision conflict")
            .contains("baseRevision"));

        let duplicate = patch(json!({
            "op": "add-node",
            "node": {"id": "n1", "type": "concept", "title": "Duplicate", "data": {}}
        }));
        assert!(registry
            .commit_graph_patch(request(1, &duplicate))
            .expect_err("duplicate node")
            .contains("duplicate"));

        let dangling = patch(json!({
            "op": "add-edge",
            "edge": {"id": "e1", "source": "n1", "target": "missing", "type": "relates", "data": {}}
        }));
        assert!(registry
            .commit_graph_patch(request(1, &dangling))
            .expect_err("dangling edge")
            .contains("missing node"));
        assert_eq!(
            registry
                .get("project-a", Some(1))
                .expect("unchanged project")["project"]["revision"],
            1
        );
    }
}
