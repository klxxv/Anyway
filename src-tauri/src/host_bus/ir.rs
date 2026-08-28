//! Host bus graph IR: `graph.ir.compile` + `graph.ir.query`.
//!
//! Runs the deterministic schema-v4 compiler (`ExtractionV3 → CanvasIRV3`)
//! and serves stateless projections over a compiled canvas. Determinism
//! (V3-20): identical extraction + ontology + compiler version yields
//! identical IR.

use anyway_schema_v4::compiler::Compiler;
use anyway_schema_v4::extract::ExtractionV3;
use anyway_schema_v4::ir::CanvasIRV3;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::kernel_commands::{inline_request, HostCallRequest};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrCompileRequest {
    pub extraction: ExtractionV3,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IrQueryRequest {
    /// One of `blocks | operators | chains | fibers | bundles | identifiability
    /// | consistency`.
    pub kind: String,
    pub canvas: CanvasIRV3,
}

/// `graph.ir.compile` — run the schema-v4 compiler. Compilation fails as a
/// whole when any structural error exists (no partial fallback).
pub fn dispatch_graph_ir_compile(request: &HostCallRequest) -> Result<Value, String> {
    let compile = inline_request::<IrCompileRequest>(request)
        .map_err(|error| format!("invalid graph.ir.compile request: {error}"))?;
    let compiler = Compiler::new();
    match compiler.compile(&compile.extraction) {
        Ok(canvas) => Ok(json!({ "canvas": canvas, "errors": [], "warnings": [] })),
        Err(report) => Ok(json!({
            "canvas": Value::Null,
            "errors": report.errors,
            "warnings": report.warnings,
        })),
    }
}

/// `graph.ir.query` — stateless projection over a compiled canvas.
pub fn dispatch_graph_ir_query(request: &HostCallRequest) -> Result<Value, String> {
    let query = inline_request::<IrQueryRequest>(request)
        .map_err(|error| format!("invalid graph.ir.query request: {error}"))?;
    let canvas = query.canvas;
    let value = match query.kind.as_str() {
        "blocks" => serde_json::to_value(&canvas.blocks),
        "operators" => serde_json::to_value(&canvas.operators),
        "chains" => serde_json::to_value(&canvas.chains),
        "fibers" => serde_json::to_value(&canvas.fibers),
        "bundles" => serde_json::to_value(&canvas.bundles),
        "identifiability" => serde_json::to_value(&canvas.identifiability),
        "consistency" => serde_json::to_value(&canvas.consistency_checks),
        other => {
            return Err(format!(
                "graph.ir.query kind must be blocks|operators|chains|fibers|bundles|identifiability|consistency, got {other}"
            ));
        }
    };
    value.map_err(|error| error.to_string())
}
