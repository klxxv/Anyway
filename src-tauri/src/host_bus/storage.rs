//! Host bus graph storage: `graph.storage.put` + `graph.storage.query`.
//!
//! Backend-agnostic storage over the schema-v4 `Storage` trait, backed by the
//! reference `InMemoryStorage`. The store is swapped by replacing the backend
//! (MySQL/SQLite/MongoDB/Milvus adapters implement the same trait); the RPC
//! surface never sees a concrete backend.

use std::sync::RwLock;

use anyway_schema_v4::ir::{Block, Bundle, CanvasIRV3, Chain, Fiber, Operator};
use anyway_schema_v4::state::CompilerState;
use anyway_schema_v4::storage::{InMemoryStorage, Storage};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::kernel_commands::{inline_request, HostCallRequest};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoragePutRequest {
    /// One of `block | operator | chain | fiber | bundle | state | canvas`.
    pub kind: String,
    pub object: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageQueryRequest {
    /// One of `neighbors | fiber | provenance`.
    pub query: String,
    #[serde(default)]
    pub state: Option<Value>,
    #[serde(default)]
    pub k: usize,
    #[serde(default)]
    pub fiber_id: Option<String>,
    #[serde(default)]
    pub evidence_id: Option<String>,
}

fn put_error(kind: &str, error: impl std::fmt::Display) -> String {
    format!("graph.storage.put ({kind}) failed: {error}")
}

/// `graph.storage.put` — persist one IR object (or a whole canvas).
pub fn dispatch_graph_storage_put(
    request: &HostCallRequest,
    storage: &RwLock<InMemoryStorage>,
) -> Result<Value, String> {
    let put = inline_request::<StoragePutRequest>(request)
        .map_err(|error| format!("invalid graph.storage.put request: {error}"))?;
    let mut guard = storage
        .write()
        .map_err(|_| "graph storage lock is poisoned".to_string())?;
    match put.kind.as_str() {
        "block" => {
            let block: Block =
                serde_json::from_value(put.object).map_err(|error| put_error("block", error))?;
            guard
                .put_block(&block)
                .map_err(|error| put_error("block", error))?;
        }
        "operator" => {
            let operator: Operator =
                serde_json::from_value(put.object).map_err(|error| put_error("operator", error))?;
            guard
                .put_operator(&operator)
                .map_err(|error| put_error("operator", error))?;
        }
        "chain" => {
            let chain: Chain =
                serde_json::from_value(put.object).map_err(|error| put_error("chain", error))?;
            guard
                .put_chain(&chain)
                .map_err(|error| put_error("chain", error))?;
        }
        "fiber" => {
            let fiber: Fiber =
                serde_json::from_value(put.object).map_err(|error| put_error("fiber", error))?;
            guard
                .put_fiber(&fiber)
                .map_err(|error| put_error("fiber", error))?;
        }
        "bundle" => {
            let bundle: Bundle =
                serde_json::from_value(put.object).map_err(|error| put_error("bundle", error))?;
            guard
                .put_bundle(&bundle)
                .map_err(|error| put_error("bundle", error))?;
        }
        "state" => {
            let state: CompilerState =
                serde_json::from_value(put.object).map_err(|error| put_error("state", error))?;
            guard
                .put_state(&state)
                .map_err(|error| put_error("state", error))?;
        }
        "canvas" => {
            let canvas: CanvasIRV3 =
                serde_json::from_value(put.object).map_err(|error| put_error("canvas", error))?;
            guard
                .put_canvas(&canvas)
                .map_err(|error| put_error("canvas", error))?;
        }
        other => {
            return Err(format!(
                "graph.storage.put kind must be block|operator|chain|fiber|bundle|state|canvas, got {other}"
            ));
        }
    }
    Ok(json!({ "ok": true, "kind": put.kind }))
}

/// `graph.storage.query` — `neighbors | fiber | provenance`.
pub fn dispatch_graph_storage_query(
    request: &HostCallRequest,
    storage: &RwLock<InMemoryStorage>,
) -> Result<Value, String> {
    let query = inline_request::<StorageQueryRequest>(request)
        .map_err(|error| format!("invalid graph.storage.query request: {error}"))?;
    let guard = storage
        .read()
        .map_err(|_| "graph storage lock is poisoned".to_string())?;
    match query.query.as_str() {
        "neighbors" => {
            let state: CompilerState = serde_json::from_value(
                query
                    .state
                    .ok_or_else(|| "neighbors query requires a state".to_string())?,
            )
            .map_err(|error| error.to_string())?;
            let neighbors = guard
                .query_neighbors(&state, query.k)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(neighbors).map_err(|error| error.to_string())
        }
        "fiber" => {
            let fiber_id = query.fiber_id.unwrap_or_default();
            let fiber = guard
                .query_fiber(&fiber_id)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(fiber).map_err(|error| error.to_string())
        }
        "provenance" => {
            let evidence_id = query.evidence_id.unwrap_or_default();
            let references = guard
                .query_provenance(&evidence_id)
                .map_err(|error| error.to_string())?;
            serde_json::to_value(references).map_err(|error| error.to_string())
        }
        other => Err(format!(
            "graph.storage.query must be neighbors|fiber|provenance, got {other}"
        )),
    }
}
