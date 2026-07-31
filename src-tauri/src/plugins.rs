//! `.myc` 插件的桌面端安装与执行边界 / Desktop install and execution boundary for `.myc` plugins.
//!
//! 声明式视觉包只读取 JSON；分析包只执行经校验的 WebAssembly，并且默认没有主机能力。
//! Declarative visual packages only expose JSON; analysis packages execute verified WebAssembly
//! with no host capabilities by default. All archives are bounded and staged before visibility.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};
use tauri::AppHandle;
#[cfg(not(debug_assertions))]
use tauri::Manager;
use zip::ZipArchive;

const MYC_API_VERSION: &str = "researchcanvas.dev/v1alpha1";
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginMetadata {
    id: String,
    name: String,
    version: String,
    publisher: String,
    developer: String,
    description: String,
    homepage: Option<String>,
    license: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginSpec {
    engine: String,
    entry: String,
    language: Option<String>,
    capabilities: Vec<String>,
    permissions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginManifest {
    api_version: String,
    kind: String,
    metadata: MycPluginMetadata,
    spec: MycPluginSpec,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeManifest {
    id: String,
    name: String,
    publisher: String,
    version: Option<String>,
    description: Option<String>,
    developer: Option<String>,
    source: Option<String>,
    colors: serde_json::Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMycPlugin {
    manifest: MycPluginManifest,
    install_path: String,
    theme: Option<ThemeManifest>,
    edge_style: Option<serde_json::Value>,
    runtime: Option<MycPluginRuntime>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginRuntime {
    engine: String,
    language: String,
    entry_sha256: String,
}

fn plugin_base(_app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let manifest_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repository = manifest_directory
            .parent()
            .ok_or_else(|| "Could not resolve repository root".to_string())?;
        return Ok(repository.join("plugins"));
    }

    #[cfg(not(debug_assertions))]
    {
        _app.path()
            .app_data_dir()
            .map(|path| path.join("plugins"))
            .map_err(|error| error.to_string())
    }
}

fn validate_slug(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
    {
        return Err(format!("Invalid {label}: {value}"));
    }
    Ok(())
}

fn validate_manifest(manifest: &MycPluginManifest) -> Result<(), String> {
    if manifest.api_version != MYC_API_VERSION {
        return Err(format!(
            "Unsupported plugin API version: {}",
            manifest.api_version
        ));
    }
    validate_slug(&manifest.metadata.id, "plugin id")?;
    validate_slug(&manifest.metadata.version, "plugin version")?;
    match manifest.kind.as_str() {
        "ThemePlugin" => {
            if manifest.spec.entry != "theme.json" {
                return Err("ThemePlugin entry must be theme.json".to_string());
            }
            if !manifest
                .spec
                .capabilities
                .iter()
                .any(|capability| capability == "theme.register")
            {
                return Err("ThemePlugin must declare theme.register".to_string());
            }
        }
        "EdgeStylePlugin" => {
            if manifest.spec.entry != "edge-style.json" {
                return Err("EdgeStylePlugin entry must be edge-style.json".to_string());
            }
            if !manifest
                .spec
                .capabilities
                .iter()
                .any(|capability| capability == "edge.style.register")
            {
                return Err("EdgeStylePlugin must declare edge.style.register".to_string());
            }
        }
        "AnalysisPlugin" => {
            if manifest.spec.engine != "wasm32-myc" {
                return Err("AnalysisPlugin engine must be wasm32-myc".to_string());
            }
            if manifest.spec.entry != "plugin.wasm" {
                return Err("AnalysisPlugin entry must be plugin.wasm".to_string());
            }
            if !manifest
                .spec
                .capabilities
                .iter()
                .any(|capability| capability == "analysis.run")
            {
                return Err("AnalysisPlugin must declare analysis.run".to_string());
            }
            if !matches!(
                manifest.spec.language.as_deref(),
                Some("rust" | "cpp" | "other")
            ) {
                return Err("AnalysisPlugin language must be rust, cpp, or other".to_string());
            }
        }
        _ => {
            return Err(
                "Installer accepts ThemePlugin, EdgeStylePlugin, and AnalysisPlugin packages"
                    .to_string(),
            );
        }
    }
    if !manifest.spec.permissions.is_empty() {
        return Err("Declarative visual plugins cannot request permissions in the MVP".to_string());
    }
    Ok(())
}

fn read_installed_plugin(directory: &Path) -> Result<InstalledMycPlugin, String> {
    let manifest_path = directory.join("plugin.yml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Could not read {}: {error}", manifest_path.display()))?;
    let manifest: MycPluginManifest =
        serde_yaml::from_str(&manifest_text).map_err(|error| error.to_string())?;
    validate_manifest(&manifest)?;

    let entry_path = directory.join(&manifest.spec.entry);
    let (theme, edge_style, runtime) = match manifest.kind.as_str() {
        "ThemePlugin" => {
            let entry_text = fs::read_to_string(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            (
                Some(serde_json::from_str(&entry_text).map_err(|error| error.to_string())?),
                None,
                None,
            )
        }
        "EdgeStylePlugin" => {
            let entry_text = fs::read_to_string(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            (
                None,
                Some(serde_json::from_str(&entry_text).map_err(|error| error.to_string())?),
                None,
            )
        }
        "AnalysisPlugin" => {
            let bytes = fs::read(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            if !bytes.starts_with(b"\0asm") {
                return Err("AnalysisPlugin entry is not a WebAssembly module".to_string());
            }
            let digest = Sha256::digest(&bytes);
            (
                None,
                None,
                Some(MycPluginRuntime {
                    engine: "wasm32-myc".to_string(),
                    language: manifest
                        .spec
                        .language
                        .clone()
                        .unwrap_or_else(|| "other".to_string()),
                    entry_sha256: format!("{digest:x}"),
                }),
            )
        }
        _ => (None, None, None),
    };

    Ok(InstalledMycPlugin {
        manifest,
        install_path: directory.to_string_lossy().into_owned(),
        theme,
        edge_style,
        runtime,
    })
}

/// 原子移动到 `installed` 前校验并暂存归档 / Validates and stages an archive before atomically renaming it into `installed`.
fn install_archive_into(base: &Path, archive_path: &Path) -> Result<InstalledMycPlugin, String> {
    if archive_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| !value.eq_ignore_ascii_case("myc"))
        .unwrap_or(true)
    {
        return Err("Plugin package must use the .myc extension".to_string());
    }

    let archive_size = fs::metadata(archive_path)
        .map_err(|error| error.to_string())?
        .len();
    if archive_size > MAX_ARCHIVE_BYTES {
        return Err("Plugin package exceeds the 16 MB archive limit".to_string());
    }

    let manifest = {
        let file = File::open(archive_path).map_err(|error| error.to_string())?;
        let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
        if archive.len() > MAX_ENTRIES {
            return Err("Plugin package contains too many files".to_string());
        }
        let mut entry = archive
            .by_name("plugin.yml")
            .map_err(|_| "plugin.yml is required at the package root".to_string())?;
        let mut text = String::new();
        entry
            .read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
        let manifest: MycPluginManifest =
            serde_yaml::from_str(&text).map_err(|error| error.to_string())?;
        validate_manifest(&manifest)?;
        manifest
    };

    let installed_root = base.join("installed");
    fs::create_dir_all(&installed_root).map_err(|error| error.to_string())?;
    let directory_name = format!("{}@{}", manifest.metadata.id, manifest.metadata.version);
    let destination = installed_root.join(&directory_name);
    if destination.is_dir() {
        return read_installed_plugin(&destination);
    }

    let staging = installed_root.join(format!(".staging-{directory_name}"));
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;

    let extraction = (|| -> Result<(), String> {
        let file = File::open(archive_path).map_err(|error| error.to_string())?;
        let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
        let mut expanded = 0_u64;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
            expanded = expanded.saturating_add(entry.size());
            if expanded > MAX_UNPACKED_BYTES {
                return Err("Plugin package exceeds the 32 MB expanded limit".to_string());
            }
            let relative = entry
                .enclosed_name()
                .ok_or_else(|| format!("Unsafe archive path: {}", entry.name()))?;
            let output = staging.join(relative);
            if entry.is_dir() {
                fs::create_dir_all(&output).map_err(|error| error.to_string())?;
                continue;
            }
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut target = File::create(&output).map_err(|error| error.to_string())?;
            io::copy(&mut entry, &mut target).map_err(|error| error.to_string())?;
        }
        Ok(())
    })();

    if let Err(error) = extraction {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    let staged = read_installed_plugin(&staging)?;
    if staged.manifest.metadata.id != manifest.metadata.id
        || staged.manifest.metadata.version != manifest.metadata.version
    {
        let _ = fs::remove_dir_all(&staging);
        return Err("Manifest changed during extraction".to_string());
    }

    fs::rename(&staging, &destination).map_err(|error| error.to_string())?;
    read_installed_plugin(&destination)
}

fn install_archive(app: &AppHandle, archive_path: &Path) -> Result<InstalledMycPlugin, String> {
    let base = plugin_base(app)?;
    install_archive_into(&base, archive_path)
}

fn install_pending_packages(app: &AppHandle) -> Result<(), String> {
    let packages = plugin_base(app)?.join("packages");
    if !packages.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(packages).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("myc"))
        {
            install_archive(app, &path)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn install_myc_plugin(app: AppHandle, path: String) -> Result<InstalledMycPlugin, String> {
    install_archive(&app, Path::new(&path))
}

#[tauri::command]
pub fn list_installed_plugins(app: AppHandle) -> Result<Vec<InstalledMycPlugin>, String> {
    install_pending_packages(&app)?;
    let root = plugin_base(&app)?.join("installed");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let mut plugins = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir()
            && !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(".staging-"))
        {
            plugins.push(read_installed_plugin(&path)?);
        }
    }
    plugins.sort_by(|left, right| left.manifest.metadata.id.cmp(&right.manifest.metadata.id));
    Ok(plugins)
}

#[tauri::command]
pub fn execute_myc_plugin(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    input: serde_json::Value,
) -> Result<crate::plugin_vm::PluginExecutionResult, String> {
    validate_slug(&plugin_id, "plugin id")?;
    validate_slug(&plugin_version, "plugin version")?;
    let directory = plugin_base(&app)?
        .join("installed")
        .join(format!("{plugin_id}@{plugin_version}"));
    let installed = read_installed_plugin(&directory)?;
    if installed.manifest.kind != "AnalysisPlugin" || installed.runtime.is_none() {
        return Err("Only installed AnalysisPlugin packages can execute".to_string());
    }
    let entry = directory.join(&installed.manifest.spec.entry);
    crate::plugin_vm::execute_plugin(&entry, &plugin_id, &plugin_version, &input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    fn runtime_manifest(language: &str) -> String {
        format!(
            r#"apiVersion: researchcanvas.dev/v1alpha1
kind: AnalysisPlugin
metadata:
  id: researchcanvas.runtime-smoke
  name: Runtime Smoke
  version: 1.0.0
  publisher: Research Canvas
  developer: Runtime Team
  description: End-to-end VM smoke plugin.
spec:
  engine: wasm32-myc
  entry: plugin.wasm
  language: {language}
  capabilities:
    - analysis.run
  permissions: []
"#,
        )
    }

    fn smoke_wasm() -> Vec<u8> {
        wat::parse_str(
            r#"(module
                (memory (export "memory") 1 2)
                (global $heap (mut i32) (i32.const 1024))
                (func (export "myc_alloc") (param $size i32) (result i32)
                  global.get $heap
                  global.get $heap
                  local.get $size
                  i32.add
                  global.set $heap)
                (data (i32.const 16) "{\22runtime\22:\22ok\22}")
                (func (export "myc_run") (param i32 i32) (result i64)
                  i64.const 68719476752))"#,
        )
        .expect("valid smoke module")
    }

    #[test]
    fn installs_and_executes_a_runtime_myc_package() {
        let root = tempdir().expect("temp root");
        let package = root.path().join("runtime-smoke.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive
            .write_all(runtime_manifest("rust").as_bytes())
            .expect("manifest bytes");
        archive
            .start_file("plugin.wasm", options)
            .expect("wasm entry");
        archive.write_all(&smoke_wasm()).expect("wasm bytes");
        archive.finish().expect("finish archive");

        let installed = install_archive_into(root.path(), &package).expect("install package");
        let runtime = installed.runtime.expect("runtime metadata");
        assert_eq!(runtime.engine, "wasm32-myc");
        assert_eq!(runtime.language, "rust");
        assert_eq!(runtime.entry_sha256.len(), 64);

        let output = crate::plugin_vm::execute_plugin(
            &root
                .path()
                .join("installed/researchcanvas.runtime-smoke@1.0.0/plugin.wasm"),
            "researchcanvas.runtime-smoke",
            "1.0.0",
            &json!({"operation": "self-test"}),
        )
        .expect("execute installed package");
        assert_eq!(output.output, json!({"runtime": "ok"}));
    }

    #[test]
    fn rejects_runtime_manifest_with_unknown_language() {
        let cpp: MycPluginManifest =
            serde_yaml::from_str(&runtime_manifest("cpp")).expect("parse cpp manifest");
        validate_manifest(&cpp).expect("C++ wasm plugins use the same verified ABI");

        let manifest: MycPluginManifest =
            serde_yaml::from_str(&runtime_manifest("javascript")).expect("parse manifest");
        assert!(validate_manifest(&manifest)
            .expect_err("unknown language rejected")
            .contains("language"));
    }
}
