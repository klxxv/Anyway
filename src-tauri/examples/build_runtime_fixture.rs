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
    let source = repository.join("plugins/sources/researchcanvas.runtime-smoke");
    let packages = repository.join("plugins/packages");
    fs::create_dir_all(&packages)?;

    let manifest = fs::read(source.join("plugin.yml"))?;
    let wasm = wat::parse_file(source.join("plugin.wat"))?;
    let output = packages.join("researchcanvas.runtime-smoke@1.1.0.myc");
    let file = File::create(&output)?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive.start_file("plugin.yml", options)?;
    archive.write_all(&manifest)?;
    archive.start_file("plugin.wasm", options)?;
    archive.write_all(&wasm)?;
    archive.finish()?;

    println!("{}", output.display());
    Ok(())
}
