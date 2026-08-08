/// Compile plugins/sources/myc.runtime-smoke/plugin.wat → plugin.wasm
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let wat_path = repo.join("plugins/sources/myc.runtime-smoke/plugin.wat");
    let wasm_path = repo.join("plugins/sources/myc.runtime-smoke/plugin.wasm");

    let wat = fs::read_to_string(&wat_path)
        .map_err(|e| format!("Failed to read {}: {e}", wat_path.display()))?;
    let wasm = wat::parse_str(&wat)
        .map_err(|e| format!("Failed to parse WAT: {e}"))?;
    let len = wasm.len();
    fs::write(&wasm_path, &wasm)
        .map_err(|e| format!("Failed to write {}: {e}", wasm_path.display()))?;

    println!("OK: {len} bytes → {}", wasm_path.display());
    Ok(())
}
