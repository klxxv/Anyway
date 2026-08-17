//! Backend-agnostic storage interface (implementation-plan.md §3).
//!
//! The graph engine never talks to a specific database. It talks to the
//! [`Storage`] trait, which a backend adapter implements for MySQL, SQLite,
//! MongoDB, or Milvus. Storage is exposed on the data host bus as a provider:
//! every read/write maps to a `graph.storage.*` host operation (see
//! [`StorageOperation`]) rather than a direct SQL/driver call.
//!
//! [`InMemoryStorage`] is the reference backend, used by tests and the MVP.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ir::{Block, Bundle, CanvasIRV3, Chain, Fiber, Operator};
use crate::matcher::rank_neighbors;
use crate::state::CompilerState;

/// A storage failure with a stable error code.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct StorageError {
    pub code: String,
    pub message: String,
}

impl StorageError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new("STORE-NOT-FOUND", message)
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for StorageError {}

/// The backend-agnostic storage contract (implementation-plan.md §3.1).
///
/// Every method returns a [`StorageError`] rather than a database-specific
/// error, so the IR compiler and extractor never see a concrete backend.
pub trait Storage {
    fn put_block(&mut self, block: &Block) -> Result<(), StorageError>;
    fn put_operator(&mut self, operator: &Operator) -> Result<(), StorageError>;
    fn put_chain(&mut self, chain: &Chain) -> Result<(), StorageError>;
    fn put_fiber(&mut self, fiber: &Fiber) -> Result<(), StorageError>;
    fn put_bundle(&mut self, bundle: &Bundle) -> Result<(), StorageError>;
    fn put_state(&mut self, state: &CompilerState) -> Result<(), StorageError>;

    /// Insert a whole compiled canvas.
    fn put_canvas(&mut self, ir: &CanvasIRV3) -> Result<(), StorageError> {
        for block in &ir.blocks {
            self.put_block(block)?;
        }
        for operator in &ir.operators {
            self.put_operator(operator)?;
        }
        for chain in &ir.chains {
            self.put_chain(chain)?;
        }
        for fiber in &ir.fibers {
            self.put_fiber(fiber)?;
        }
        for bundle in &ir.bundles {
            self.put_bundle(bundle)?;
        }
        for (evidence_id, refs) in &ir.provenance_index {
            for reference in refs {
                self.insert_provenance(evidence_id, reference)?;
            }
        }
        Ok(())
    }

    /// Rank the `k` nearest historical states by sparse distance.
    fn query_neighbors(
        &self,
        query: &CompilerState,
        k: usize,
    ) -> Result<Vec<CompilerState>, StorageError>;

    /// Look up a fiber by id.
    fn query_fiber(&self, fiber_id: &str) -> Result<Option<Fiber>, StorageError>;

    /// Reverse provenance lookup: evidence id → derived graph objects.
    fn query_provenance(&self, evidence_id: &str) -> Result<Vec<String>, StorageError>;

    /// Record one evidence → object provenance edge (used by `put_canvas`).
    fn insert_provenance(&mut self, evidence_id: &str, reference: &str)
        -> Result<(), StorageError>;
}

/// Reference in-memory backend.
#[derive(Clone, Debug, Default)]
pub struct InMemoryStorage {
    blocks: BTreeMap<String, Block>,
    operators: BTreeMap<String, Operator>,
    chains: BTreeMap<String, Chain>,
    fibers: BTreeMap<String, Fiber>,
    bundles: BTreeMap<String, Bundle>,
    states: BTreeMap<String, CompilerState>,
    provenance: HashMap<String, Vec<String>>,
}

impl Storage for InMemoryStorage {
    fn put_block(&mut self, block: &Block) -> Result<(), StorageError> {
        self.blocks.insert(block.id.clone(), block.clone());
        Ok(())
    }

    fn put_operator(&mut self, operator: &Operator) -> Result<(), StorageError> {
        self.operators
            .insert(operator.id.clone(), operator.clone());
        Ok(())
    }

    fn put_chain(&mut self, chain: &Chain) -> Result<(), StorageError> {
        self.chains.insert(chain.id.clone(), chain.clone());
        Ok(())
    }

    fn put_fiber(&mut self, fiber: &Fiber) -> Result<(), StorageError> {
        self.fibers.insert(fiber.id.clone(), fiber.clone());
        Ok(())
    }

    fn put_bundle(&mut self, bundle: &Bundle) -> Result<(), StorageError> {
        self.bundles.insert(bundle.id.clone(), bundle.clone());
        Ok(())
    }

    fn put_state(&mut self, state: &CompilerState) -> Result<(), StorageError> {
        self.states.insert(state.id.clone(), state.clone());
        Ok(())
    }

    fn query_neighbors(
        &self,
        query: &CompilerState,
        k: usize,
    ) -> Result<Vec<CompilerState>, StorageError> {
        let candidates: Vec<CompilerState> = self.states.values().cloned().collect();
        let ranked = rank_neighbors(query, &candidates, &HashMap::new());
        Ok(ranked.into_iter().take(k).map(|result| {
            self.states
                .get(&result.state_id)
                .cloned()
                .expect("ranked state exists in storage")
        }).collect())
    }

    fn query_fiber(&self, fiber_id: &str) -> Result<Option<Fiber>, StorageError> {
        Ok(self.fibers.get(fiber_id).cloned())
    }

    fn query_provenance(&self, evidence_id: &str) -> Result<Vec<String>, StorageError> {
        Ok(self.provenance.get(evidence_id).cloned().unwrap_or_default())
    }

    fn insert_provenance(
        &mut self,
        evidence_id: &str,
        reference: &str,
    ) -> Result<(), StorageError> {
        self.provenance
            .entry(evidence_id.to_string())
            .or_default()
            .push(reference.to_string());
        Ok(())
    }
}

/// The `graph.storage.*` operation surface exposed on the data host bus
/// (implementation-plan.md §3.3). A host adapter serializes these operations
/// into Host SDK calls; the storage backend never owns transport.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum StorageOperation {
    PutBlock { block: Block },
    PutOperator { operator: Operator },
    PutChain { chain: Chain },
    PutFiber { fiber: Fiber },
    PutBundle { bundle: Bundle },
    PutState { state: CompilerState },
    PutCanvas { canvas: CanvasIRV3 },
    QueryNeighbors { state: CompilerState, k: usize },
    QueryFiber { fiber_id: String },
    QueryProvenance { evidence_id: String },
}

/// Execute one data-host-bus storage operation against a backend.
pub fn dispatch_storage<S: Storage>(
    storage: &mut S,
    operation: StorageOperation,
) -> Result<Value, StorageError> {
    match operation {
        StorageOperation::PutBlock { block } => {
            storage.put_block(&block)?;
            Ok(Value::Null)
        }
        StorageOperation::PutOperator { operator } => {
            storage.put_operator(&operator)?;
            Ok(Value::Null)
        }
        StorageOperation::PutChain { chain } => {
            storage.put_chain(&chain)?;
            Ok(Value::Null)
        }
        StorageOperation::PutFiber { fiber } => {
            storage.put_fiber(&fiber)?;
            Ok(Value::Null)
        }
        StorageOperation::PutBundle { bundle } => {
            storage.put_bundle(&bundle)?;
            Ok(Value::Null)
        }
        StorageOperation::PutState { state } => {
            storage.put_state(&state)?;
            Ok(Value::Null)
        }
        StorageOperation::PutCanvas { canvas } => {
            storage.put_canvas(&canvas)?;
            Ok(Value::Null)
        }
        StorageOperation::QueryNeighbors { state, k } => {
            let neighbors = storage.query_neighbors(&state, k)?;
            serde_json::to_value(neighbors).map_err(|e| StorageError::new("STORE-SERIALIZE", e.to_string()))
        }
        StorageOperation::QueryFiber { fiber_id } => {
            let fiber = storage.query_fiber(&fiber_id)?;
            serde_json::to_value(fiber).map_err(|e| StorageError::new("STORE-SERIALIZE", e.to_string()))
        }
        StorageOperation::QueryProvenance { evidence_id } => {
            let refs = storage.query_provenance(&evidence_id)?;
            serde_json::to_value(refs).map_err(|e| StorageError::new("STORE-SERIALIZE", e.to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::BlockType;
    use crate::OperatorKind;
    use serde_json::json;

    fn block(id: &str) -> Block {
        Block {
            id: id.to_string(),
            block_type: BlockType::Variable,
            concept_id: Some(id.to_string()),
            variable_ref: None,
            member_refs: vec![],
            context_ref: None,
            axiom_set_ref: None,
            semantic_hash: format!("sha256:{id}"),
            instance_hash: format!("sha256:{id}"),
        }
    }

    #[test]
    fn in_memory_round_trips_blocks_and_provenance() {
        let mut storage = InMemoryStorage::default();
        let ir = CanvasIRV3 {
            schema_version: crate::GRAPH_IR_SCHEMA_VERSION.to_string(),
            blocks: vec![block("block_001")],
            operators: vec![Operator {
                id: "op_001".to_string(),
                operator: OperatorKind::I,
                input_refs: vec!["block_001".to_string()],
                output_refs: vec!["block_002".to_string()],
                payload: json!({}),
                context_ref: None,
                axiom_set_ref: None,
                evidence_refs: vec!["ev_001".to_string()],
                semantic_hash: "sha256:op".to_string(),
                instance_hash: "sha256:op".to_string(),
            }],
            provenance_index: BTreeMap::from([("ev_001".to_string(), vec!["op_001".to_string()])]),
            ..CanvasIRV3::default()
        };

        storage.put_canvas(&ir).unwrap();
        assert_eq!(storage.blocks.len(), 1);
        assert_eq!(storage.query_provenance("ev_001").unwrap(), vec!["op_001"]);
        assert!(storage.query_provenance("missing").unwrap().is_empty());
    }

    #[test]
    fn dispatch_storage_routes_query_provenance() {
        let mut storage = InMemoryStorage::default();
        let op = StorageOperation::QueryProvenance {
            evidence_id: "ev_001".to_string(),
        };
        let result = dispatch_storage(&mut storage, op).unwrap();
        assert_eq!(result, json!([]));
    }
}
