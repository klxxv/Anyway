/**
 * Backend-agnostic storage contract (implementation-plan.md §3).
 *
 * Mirrors `crates/anyway-schema-v4/src/storage.rs` wire types. The Rust
 * `Storage` trait is authoritative; these are the cross-boundary types that
 * travel over the `graph.storage.*` data-host-bus operations.
 */

import type { Block, Bundle, CanvasIRV3, Chain, Fiber, Operator } from "./ir";
import type { CompilerState } from "./state";

export interface StorageError {
  code: string;
  message: string;
}

export type StorageOperation =
  | { op: "put_block"; block: Block }
  | { op: "put_operator"; operator: Operator }
  | { op: "put_chain"; chain: Chain }
  | { op: "put_fiber"; fiber: Fiber }
  | { op: "put_bundle"; bundle: Bundle }
  | { op: "put_state"; state: CompilerState }
  | { op: "put_canvas"; canvas: CanvasIRV3 }
  | { op: "query_neighbors"; state: CompilerState; k: number }
  | { op: "query_fiber"; fiber_id: string }
  | { op: "query_provenance"; evidence_id: string };
