//! 重新生成 myc.runtime-smoke 插件包夹具 / Regenerate the myc.runtime-smoke package fixture.
//!
//! 把 plugin.wat 编译为 plugin.wasm 写回源码目录,再按 build_myc_plugin.py
//! 的同一契约打包(全部允许文件 + payloads 哈希块),产出确定性的 .myc。
//! Compiles plugin.wat to plugin.wasm in the source directory, then packages
//! every allowed file with the same payloads contract as build_myc_plugin.py,
//! producing a deterministic .myc archive.

use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repository root")
        .to_path_buf();
    let source = repository.join("plugins/sources/myc.runtime-smoke");
    let packages = repository.join("plugins/packages");
    fs::create_dir_all(&packages)?;

    // wat → wasm 写回源码目录,使 python 与 cargo 两条打包路径产物一致。
    // Compile wat → wasm back into the source dir so the python and cargo
    // build paths produce identical packages.
    let wasm = wat::parse_file(source.join("plugin.wat"))?;
    fs::write(source.join("plugin.wasm"), &wasm)?;

    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for entry in fs::read_dir(&source)? {
        let path = entry?.path();
        if path.is_file() {
            let name = path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            entries.push((name, fs::read(&path)?));
        }
    }
    entries.sort();

    let manifest_text = String::from_utf8(
        entries
            .iter()
            .find(|(name, _)| name == "plugin.yml")
            .expect("plugin.yml present")
            .1
            .clone(),
    )?;
    let mut archived_manifest = manifest_text.trim_end().to_string();
    archived_manifest.push_str("\npayloads:\n");
    for (name, bytes) in &entries {
        if name == "plugin.yml" {
            continue;
        }
        archived_manifest.push_str(&format!("  {name}: {:x}\n", Sha256::digest(bytes)));
    }

    let output = packages.join("myc.runtime-smoke@1.1.0.myc");
    let file = File::create(&output)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, bytes) in &entries {
        archive.start_file(name, options)?;
        if name == "plugin.yml" {
            archive.write_all(archived_manifest.as_bytes())?;
        } else {
            archive.write_all(bytes)?;
        }
    }
    archive.finish()?;

    println!("{}", output.display());
    Ok(())
}
