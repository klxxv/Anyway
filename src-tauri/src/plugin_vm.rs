//! Capability-free WebAssembly VM for Rust/C++ `.myc` plugins.
//! Rust/C++ `.myc` 插件的零默认能力 WebAssembly 虚拟机。

use serde::Serialize;
use serde_json::Value;
use std::{fs, path::Path, time::Instant};
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const MAX_INPUT_BYTES: usize = 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
const MAX_MEMORY_BYTES: usize = 16 * 1024 * 1024;
const FUEL_BUDGET: u64 = 5_000_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginExecutionResult {
    pub plugin_id: String,
    pub plugin_version: String,
    pub output: Value,
    pub fuel_consumed: u64,
    pub duration_ms: u64,
}

struct VmState {
    limits: StoreLimits,
}

pub fn execute_plugin(
    entry: &Path,
    plugin_id: &str,
    plugin_version: &str,
    input: &Value,
) -> Result<PluginExecutionResult, String> {
    let wasm = fs::read(entry)
        .map_err(|error| format!("Could not read plugin entry {}: {error}", entry.display()))?;
    execute_wasm_bytes(&wasm, plugin_id, plugin_version, input)
}

fn execute_wasm_bytes(
    wasm: &[u8],
    plugin_id: &str,
    plugin_version: &str,
    input: &Value,
) -> Result<PluginExecutionResult, String> {
    let request = serde_json::to_vec(input).map_err(|error| error.to_string())?;
    if request.len() > MAX_INPUT_BYTES {
        return Err("Plugin request exceeds the 1 MB limit".to_string());
    }

    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, wasm).map_err(|error| error.to_string())?;
    if module.imports().next().is_some() {
        return Err("MYC runtime plugins cannot import host functions".to_string());
    }

    let limits = StoreLimitsBuilder::new()
        .memory_size(MAX_MEMORY_BYTES)
        .instances(1)
        .memories(1)
        .tables(1)
        .build();
    let mut store = Store::new(&engine, VmState { limits });
    store.limiter(|state| &mut state.limits);
    store
        .add_fuel(FUEL_BUDGET)
        .map_err(|error| error.to_string())?;

    let linker = Linker::new(&engine);
    let instance = linker
        .instantiate(&mut store, &module)
        .and_then(|pre| pre.start(&mut store))
        .map_err(|error| error.to_string())?;
    let memory = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| "Plugin must export memory".to_string())?;
    let allocate = instance
        .get_typed_func::<i32, i32>(&store, "myc_alloc")
        .map_err(|_| "Plugin must export myc_alloc(i32) -> i32".to_string())?;
    let run = instance
        .get_typed_func::<(i32, i32), i64>(&store, "myc_run")
        .map_err(|_| "Plugin must export myc_run(i32, i32) -> i64".to_string())?;

    let started = Instant::now();
    let request_len =
        i32::try_from(request.len()).map_err(|_| "Plugin request length overflow".to_string())?;
    let request_ptr = allocate
        .call(&mut store, request_len)
        .map_err(|error| format!("Plugin allocation failed: {error}"))?;
    if request_ptr < 0 {
        return Err("Plugin returned a negative request pointer".to_string());
    }
    memory
        .write(&mut store, request_ptr as usize, &request)
        .map_err(|error| format!("Plugin request write failed: {error}"))?;

    let packed = run
        .call(&mut store, (request_ptr, request_len))
        .map_err(|error| format!("Plugin execution trapped: {error}"))? as u64;
    let output_ptr = (packed >> 32) as usize;
    let output_len = (packed & 0xffff_ffff) as usize;
    if output_len > MAX_OUTPUT_BYTES {
        return Err("Plugin output exceeds the 1 MB limit".to_string());
    }
    let mut output_bytes = vec![0_u8; output_len];
    memory
        .read(&store, output_ptr, &mut output_bytes)
        .map_err(|error| format!("Plugin output read failed: {error}"))?;
    let output = serde_json::from_slice(&output_bytes)
        .map_err(|error| format!("Plugin returned invalid JSON: {error}"))?;

    Ok(PluginExecutionResult {
        plugin_id: plugin_id.to_string(),
        plugin_version: plugin_version.to_string(),
        output,
        fuel_consumed: store.fuel_consumed().unwrap_or(0),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn executes_a_capability_free_plugin() {
        let wasm = wat::parse_str(
            r#"(module
                (memory (export "memory") 1 2)
                (global $heap (mut i32) (i32.const 1024))
                (func (export "myc_alloc") (param $size i32) (result i32)
                  global.get $heap
                  global.get $heap
                  local.get $size
                  i32.add
                  global.set $heap)
                (data (i32.const 16) "{\22status\22:\22ok\22}")
                (func (export "myc_run") (param i32 i32) (result i64)
                  i64.const 68719476751))"#,
        )
        .expect("valid wat");

        let result = execute_wasm_bytes(&wasm, "test.plugin", "1.0.0", &json!({"x": 1}))
            .expect("plugin execution succeeds");
        assert_eq!(result.output, json!({"status": "ok"}));
        assert!(result.fuel_consumed > 0);
    }

    #[test]
    fn rejects_host_imports() {
        let wasm = wat::parse_str(
            r#"(module
                (import "host" "read_file" (func))
                (memory (export "memory") 1)
                (func (export "myc_alloc") (param i32) (result i32) i32.const 0)
                (func (export "myc_run") (param i32 i32) (result i64) i64.const 0))"#,
        )
        .expect("valid wat");
        let error = execute_wasm_bytes(&wasm, "test.plugin", "1.0.0", &json!({}))
            .expect_err("imports must be rejected");
        assert!(error.contains("cannot import host functions"));
    }

    #[test]
    fn fuel_stops_infinite_execution() {
        let wasm = wat::parse_str(
            r#"(module
                (memory (export "memory") 1)
                (func (export "myc_alloc") (param i32) (result i32) i32.const 0)
                (func (export "myc_run") (param i32 i32) (result i64)
                  (loop $forever br $forever)
                  i64.const 0))"#,
        )
        .expect("valid wat");
        let error = execute_wasm_bytes(&wasm, "test.plugin", "1.0.0", &json!({}))
            .expect_err("fuel exhaustion must trap");
        assert!(error.contains("trapped") || error.contains("fuel"));
    }
}
