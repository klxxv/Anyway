//! Safe, declarative VSIX theme/icon-theme importer.
//!
//! This module never loads an extension runtime. It reads package.json and the
//! declared theme/icon-theme JSON plus referenced visual assets, then emits
//! ordinary declarative MYC packages for the existing installer.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;
use zip::{read::ZipFile, write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};

const MAX_VSIX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VSIX_UNPACKED_BYTES: u64 = 64 * 1024 * 1024;
const MAX_VSIX_ENTRIES: usize = 10_000;
const MAX_VSIX_MEMBER_BYTES: u64 = 16 * 1024 * 1024;
const MAX_COMPRESSION_RATIO: u64 = 200;
const MAX_JSON_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VsixImportedPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub kind: String,
    pub asset_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VsixImportReport {
    pub source: String,
    pub package_name: String,
    pub publisher: String,
    pub version: String,
    pub imported: Vec<VsixImportedPlugin>,
    pub ignored_code_assets: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct VsixPackageJson {
    name: String,
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    publisher: Option<String>,
    version: String,
    description: Option<String>,
    main: Option<String>,
    browser: Option<String>,
    #[serde(rename = "activationEvents", default)]
    activation_events: Vec<Value>,
    contributes: Option<VsixContributes>,
}

#[derive(Clone, Debug, Deserialize, Default)]
struct VsixContributes {
    #[serde(default)]
    themes: Vec<VsixThemeContribution>,
    #[serde(rename = "iconThemes", default)]
    icon_themes: Vec<VsixIconThemeContribution>,
    #[serde(default)]
    commands: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize)]
struct VsixThemeContribution {
    label: Option<String>,
    path: String,
    #[serde(rename = "uiTheme")]
    ui_theme: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct VsixIconThemeContribution {
    id: Option<String>,
    label: Option<String>,
    path: String,
}

#[derive(Clone, Debug)]
struct GeneratedPackage {
    kind: String,
    entry: String,
    manifest: String,
    entry_json: Value,
    assets: Vec<(String, Vec<u8>)>,
}

fn safe_archive_name(name: &str) -> Result<String, String> {
    if name.is_empty() || name.starts_with('/') || name.contains('\\') || name.contains(':') {
        return Err(format!("Unsafe VSIX archive path: {name}"));
    }
    if name
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(format!("Unsafe VSIX archive path: {name}"));
    }
    Ok(name.to_string())
}

fn has_native_extension(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("wasm" | "exe" | "dll" | "so" | "dylib" | "node" | "bin")
    )
}

fn has_code_extension(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("js" | "cjs" | "mjs" | "ts" | "tsx" | "jsx" | "bat" | "cmd" | "ps1" | "sh")
    )
}

fn is_zip_symlink(entry: &ZipFile<'_>) -> bool {
    entry
        .unix_mode()
        .is_some_and(|mode| (mode & 0o170000) == 0o120000)
}

fn validate_vsix_archive(archive: &mut ZipArchive<File>) -> Result<Vec<String>, String> {
    if archive.len() > MAX_VSIX_ENTRIES {
        return Err("VSIX contains too many archive entries".to_string());
    }
    let mut names = Vec::with_capacity(archive.len());
    let mut expanded = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let raw_name = entry.name();
        let name = safe_archive_name(raw_name.trim_end_matches('/'))?;
        if is_zip_symlink(&entry) {
            return Err(format!("VSIX contains a symbolic link: {name}"));
        }
        if entry.size() > MAX_VSIX_MEMBER_BYTES {
            return Err(format!("VSIX member exceeds 16 MB: {name}"));
        }
        if entry.compressed_size() > 0
            && entry.size() / entry.compressed_size() > MAX_COMPRESSION_RATIO
        {
            return Err(format!("VSIX member compression ratio is unsafe: {name}"));
        }
        if has_native_extension(&name) {
            return Err(format!("VSIX contains a native binary: {name}"));
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_VSIX_UNPACKED_BYTES {
            return Err("VSIX expanded payload exceeds 64 MB".to_string());
        }
        if !entry.is_dir() {
            names.push(name);
        }
    }
    Ok(names)
}

fn read_member(
    archive: &mut ZipArchive<File>,
    name: &str,
    max_bytes: u64,
) -> Result<Vec<u8>, String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| format!("VSIX member is missing: {name}"))?;
    if entry.size() > max_bytes {
        return Err(format!("VSIX member exceeds its limit: {name}"));
    }
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() as u64 > max_bytes {
        return Err(format!("VSIX member exceeds its limit: {name}"));
    }
    Ok(bytes)
}

fn read_json_member(archive: &mut ZipArchive<File>, name: &str) -> Result<Value, String> {
    let bytes = read_member(archive, name, MAX_JSON_BYTES)?;
    serde_json::from_slice(&bytes).map_err(|error| format!("Invalid JSON in {name}: {error}"))
}

fn package_json_path(names: &[String]) -> Result<String, String> {
    names
        .iter()
        .find(|name| name.as_str() == "extension/package.json")
        .or_else(|| names.iter().find(|name| name.as_str() == "package.json"))
        .or_else(|| {
            names
                .iter()
                .filter(|name| name.ends_with("/package.json"))
                .min_by_key(|name| name.len())
        })
        .cloned()
        .ok_or_else(|| "VSIX package.json is missing".to_string())
}

fn resolve_member_path(root: &str, requested: &str) -> Result<String, String> {
    if requested.is_empty() || requested.starts_with('/') || requested.contains('\\') {
        return Err(format!("Unsafe VSIX contribution path: {requested}"));
    }
    if requested
        .split('/')
        .any(|part| part.is_empty() || part == "..")
    {
        return Err(format!("Unsafe VSIX contribution path: {requested}"));
    }
    let requested = requested
        .split('/')
        .filter(|part| *part != ".")
        .collect::<Vec<_>>()
        .join("/");
    if requested.is_empty() {
        return Err("VSIX contribution path is empty".to_string());
    }
    let resolved = if root.is_empty() {
        requested.to_string()
    } else {
        format!("{root}/{requested}")
    };
    safe_archive_name(&resolved)
}

fn safe_slug(value: &str, fallback: &str) -> String {
    let mut result = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            result.push(character.to_ascii_lowercase());
        } else if !result.ends_with('-') {
            result.push('-');
        }
    }
    let result = result.trim_matches('-').to_string();
    if result.is_empty() {
        fallback.to_string()
    } else {
        result.chars().take(24).collect()
    }
}

fn safe_version(value: &str) -> String {
    safe_slug(value, "0-0-0").chars().take(48).collect()
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"imported\"".to_string())
}

fn plugin_manifest(
    kind: &str,
    id: &str,
    name: &str,
    version: &str,
    publisher: &str,
    entry: &str,
    capability: &str,
    description: &str,
) -> String {
    format!(
        "apiVersion: researchcanvas.dev/v1alpha1\nkind: {kind}\nmetadata:\n  id: {}\n  name: {}\n  version: {}\n  publisher: {}\n  developer: {}\n  description: {}\nspec:\n  engine: declarative\n  entry: {entry}\n  capabilities:\n    - {capability}\n  permissions: []\n",
        yaml_string(id),
        yaml_string(name),
        yaml_string(version),
        yaml_string(publisher),
        yaml_string(publisher),
        yaml_string(description),
    )
}

fn string_from_object(
    object: Option<&Map<String, Value>>,
    keys: &[&str],
    fallback: &str,
) -> String {
    keys.iter()
        .find_map(|key| {
            object
                .and_then(|value| value.get(*key))
                .and_then(Value::as_str)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn copy_asset(
    archive: &mut ZipArchive<File>,
    source: &str,
    root: &str,
    assets: &mut Vec<(String, Vec<u8>)>,
    asset_map: &mut HashMap<String, String>,
) -> Result<String, String> {
    let resolved = resolve_member_path(root, source)?;
    let extension = Path::new(&resolved)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    if !matches!(
        extension.as_str(),
        "json" | "svg" | "png" | "woff" | "woff2" | "ttf" | "otf"
    ) {
        return Err(format!(
            "VSIX visual asset has an unsupported type: {resolved}"
        ));
    }
    if let Some(target) = asset_map.get(&resolved) {
        return Ok(target.clone());
    }
    let source_name = Path::new(&resolved)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("asset");
    let target = format!(
        "assets/{}-{}",
        assets.len(),
        safe_slug(source_name, "asset")
    );
    let bytes = read_member(archive, &resolved, MAX_VSIX_MEMBER_BYTES)?;
    assets.push((target.clone(), bytes));
    asset_map.insert(resolved, target.clone());
    Ok(target)
}

fn theme_payload(
    package: &VsixPackageJson,
    contribution: &VsixThemeContribution,
    theme: &Value,
    id: &str,
    label: &str,
    publisher: &str,
    version: &str,
) -> Value {
    let colors = theme.get("colors").and_then(Value::as_object);
    json!({
        "id": id,
        "name": label,
        "publisher": publisher,
        "version": version,
        "description": package.description.clone().unwrap_or_else(|| "Imported declarative VS Code theme".to_string()),
        "developer": publisher,
        "source": "vsix",
        "colors": {
            "app": string_from_object(colors, &["activityBar.background", "titleBar.activeBackground"], "#eef1f5"),
            "panel": string_from_object(colors, &["sideBar.background", "panel.background"], "#ffffff"),
            "canvas": string_from_object(colors, &["editor.background", "editorGroup.emptyBackground"], "#f8f9fb"),
            "text": string_from_object(colors, &["foreground", "editor.foreground", "sideBar.foreground"], "#172033"),
            "muted": string_from_object(colors, &["descriptionForeground", "disabledForeground"], "#697386"),
            "accent": string_from_object(colors, &["focusBorder", "button.background", "textLink.foreground"], "#6750d8"),
            "border": string_from_object(colors, &["editorGroup.border", "panel.border", "contrastBorder"], "#dfe3e9")
        },
        "vscode": {
            "uiTheme": contribution.ui_theme,
            "tokenColors": theme.get("tokenColors").cloned().unwrap_or(Value::Array(Vec::new())),
            "semanticTokenColors": theme.get("semanticTokenColors").cloned().unwrap_or_else(|| json!({}))
        }
    })
}

fn icon_payload(
    package: &VsixPackageJson,
    icon_theme: &Value,
    id: &str,
    label: &str,
    publisher: &str,
    version: &str,
    archive: &mut ZipArchive<File>,
    icon_root: &str,
) -> Result<(Value, Vec<(String, Vec<u8>)>), String> {
    let mut assets = Vec::new();
    let mut asset_map = HashMap::new();
    let mut definitions = Map::new();
    if let Some(raw_definitions) = icon_theme.get("iconDefinitions").and_then(Value::as_object) {
        for (definition_id, raw_definition) in raw_definitions {
            let Some(raw_definition) = raw_definition.as_object() else {
                continue;
            };
            let mut definition = Map::new();
            if let Some(icon_path) = raw_definition.get("iconPath").and_then(Value::as_str) {
                let target =
                    copy_asset(archive, icon_path, icon_root, &mut assets, &mut asset_map)?;
                definition.insert("iconPath".to_string(), Value::String(target));
            }
            for key in ["fontCharacter", "fontId"] {
                if let Some(value) = raw_definition.get(key).and_then(Value::as_str) {
                    definition.insert(key.to_string(), Value::String(value.to_string()));
                }
            }
            definitions.insert(definition_id.clone(), Value::Object(definition));
        }
    }
    let mut fonts = Vec::new();
    if let Some(raw_fonts) = icon_theme.get("fonts").and_then(Value::as_array) {
        for raw_font in raw_fonts {
            let Some(raw_font) = raw_font.as_object() else {
                continue;
            };
            let Some(font_id) = raw_font.get("id").and_then(Value::as_str) else {
                continue;
            };
            let mut font = Map::new();
            font.insert("id".to_string(), Value::String(font_id.to_string()));
            let mut sources = Vec::new();
            if let Some(raw_sources) = raw_font.get("src").and_then(Value::as_array) {
                for raw_source in raw_sources {
                    let source_path = raw_source
                        .as_object()
                        .and_then(|object| object.get("path"))
                        .and_then(Value::as_str)
                        .or_else(|| raw_source.as_str());
                    if let Some(source_path) = source_path {
                        sources.push(Value::String(copy_asset(
                            archive,
                            source_path,
                            icon_root,
                            &mut assets,
                            &mut asset_map,
                        )?));
                    }
                }
            }
            font.insert("src".to_string(), Value::Array(sources));
            for key in ["weight", "style"] {
                if let Some(value) = raw_font.get(key).and_then(Value::as_str) {
                    font.insert(key.to_string(), Value::String(value.to_string()));
                }
            }
            fonts.push(Value::Object(font));
        }
    }
    let string_map = |key: &str| -> Value {
        let mut map = Map::new();
        if let Some(values) = icon_theme.get(key).and_then(Value::as_object) {
            for (name, value) in values {
                if let Some(value) = value.as_str() {
                    map.insert(name.clone(), Value::String(value.to_string()));
                }
            }
        }
        Value::Object(map)
    };
    Ok((
        json!({
            "schemaVersion": 1,
            "id": id,
            "name": label,
            "publisher": publisher,
            "version": version,
            "description": package.description.clone().unwrap_or_else(|| "Imported declarative VS Code icon theme".to_string()),
            "source": "vsix",
            "fileExtensions": string_map("fileExtensions"),
            "fileNames": string_map("fileNames"),
            "folderNames": string_map("folderNames"),
            "folderNamesExpanded": string_map("folderNamesExpanded"),
            "iconDefinitions": Value::Object(definitions),
            "fonts": Value::Array(fonts)
        }),
        assets,
    ))
}

fn write_generated_package(path: &Path, package: &GeneratedPackage) -> Result<(), String> {
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer
            .start_file("plugin.yml", options)
            .map_err(|error| error.to_string())?;
        writer
            .write_all(package.manifest.as_bytes())
            .map_err(|error| error.to_string())?;
        writer
            .start_file(&package.entry, options)
            .map_err(|error| error.to_string())?;
        let entry =
            serde_json::to_vec_pretty(&package.entry_json).map_err(|error| error.to_string())?;
        writer
            .write_all(&entry)
            .map_err(|error| error.to_string())?;
        for (path, bytes) in &package.assets {
            writer
                .start_file(path, options)
                .map_err(|error| error.to_string())?;
            writer.write_all(bytes).map_err(|error| error.to_string())?;
        }
        writer.finish().map_err(|error| error.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

fn generated_id(publisher: &str, name: &str, suffix: &str, index: usize) -> String {
    format!(
        "vsix.{}.{}.{}.{}",
        safe_slug(publisher, "publisher"),
        safe_slug(name, "extension"),
        safe_slug(suffix, "resource"),
        index
    )
}

fn generate_packages(
    archive: &mut ZipArchive<File>,
    names: &[String],
    package: &VsixPackageJson,
    package_json_path: &str,
) -> Result<(Vec<GeneratedPackage>, Vec<String>), String> {
    if package.main.is_some() || package.browser.is_some() {
        return Err("VSIX declares main/browser code and cannot be imported".to_string());
    }
    if !package.activation_events.is_empty() {
        return Err("VSIX declares activation events and cannot be imported".to_string());
    }
    let contributes = package
        .contributes
        .as_ref()
        .ok_or_else(|| "VSIX does not declare theme contributions".to_string())?;
    if !contributes.commands.is_empty() {
        return Err("VSIX commands are not imported or executed".to_string());
    }
    if contributes.themes.is_empty() && contributes.icon_themes.is_empty() {
        return Err("VSIX has no theme or icon-theme contribution".to_string());
    }
    let root = Path::new(package_json_path)
        .parent()
        .and_then(|path| path.to_str())
        .unwrap_or("");
    let publisher = package.publisher.as_deref().unwrap_or("vscode");
    let version = safe_version(&package.version);
    let mut generated = Vec::new();
    for (index, contribution) in contributes.themes.iter().enumerate() {
        let theme_path = resolve_member_path(root, &contribution.path)?;
        if !names.iter().any(|name| name == &theme_path)
            || Path::new(&theme_path)
                .extension()
                .and_then(|value| value.to_str())
                != Some("json")
        {
            return Err(format!(
                "Theme contribution must reference an existing JSON file: {theme_path}"
            ));
        }
        let theme = read_json_member(archive, &theme_path)?;
        let label = contribution
            .label
            .clone()
            .or_else(|| package.display_name.clone())
            .unwrap_or_else(|| package.name.clone());
        let id = generated_id(publisher, &package.name, &label, index);
        generated.push(GeneratedPackage {
            kind: "ThemePlugin".to_string(),
            entry: "theme.json".to_string(),
            manifest: plugin_manifest(
                "ThemePlugin",
                &id,
                &label,
                &version,
                publisher,
                "theme.json",
                "theme.register",
                package
                    .description
                    .as_deref()
                    .unwrap_or("Imported declarative VS Code theme"),
            ),
            entry_json: theme_payload(
                package,
                contribution,
                &theme,
                &id,
                &label,
                publisher,
                &version,
            ),
            assets: Vec::new(),
        });
    }
    for (index, contribution) in contributes.icon_themes.iter().enumerate() {
        let icon_path = resolve_member_path(root, &contribution.path)?;
        if !names.iter().any(|name| name == &icon_path)
            || Path::new(&icon_path)
                .extension()
                .and_then(|value| value.to_str())
                != Some("json")
        {
            return Err(format!(
                "Icon theme contribution must reference an existing JSON file: {icon_path}"
            ));
        }
        let icon_theme = read_json_member(archive, &icon_path)?;
        let icon_root = Path::new(&icon_path)
            .parent()
            .and_then(|path| path.to_str())
            .unwrap_or("");
        let label = contribution
            .label
            .clone()
            .or_else(|| package.display_name.clone())
            .unwrap_or_else(|| package.name.clone());
        let suffix = contribution.id.as_deref().unwrap_or(&label);
        let id = generated_id(publisher, &package.name, suffix, index);
        let (entry_json, assets) = icon_payload(
            package,
            &icon_theme,
            &id,
            &label,
            publisher,
            &version,
            archive,
            icon_root,
        )?;
        generated.push(GeneratedPackage {
            kind: "IconThemePlugin".to_string(),
            entry: "icon-theme.json".to_string(),
            manifest: plugin_manifest(
                "IconThemePlugin",
                &id,
                &label,
                &version,
                publisher,
                "icon-theme.json",
                "icon-theme.register",
                package
                    .description
                    .as_deref()
                    .unwrap_or("Imported declarative VS Code icon theme"),
            ),
            entry_json,
            assets,
        });
    }
    let ignored = names
        .iter()
        .filter(|name| has_code_extension(name))
        .cloned()
        .collect();
    Ok((generated, ignored))
}

#[tauri::command]
pub fn import_vscode_vsix(app: AppHandle, path: String) -> Result<VsixImportReport, String> {
    let source_path = PathBuf::from(&path);
    let source_metadata = fs::symlink_metadata(&source_path).map_err(|error| error.to_string())?;
    if source_metadata.file_type().is_symlink() || !source_metadata.is_file() {
        return Err("VSIX must be a regular file, not a symbolic link".to_string());
    }
    if !source_path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("vsix"))
    {
        return Err("VSIX import requires a .vsix file".to_string());
    }
    if source_metadata.len() > MAX_VSIX_ARCHIVE_BYTES {
        return Err("VSIX exceeds the 64 MB archive limit".to_string());
    }
    let source_path = source_path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let file = File::open(&source_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let names = validate_vsix_archive(&mut archive)?;
    let package_json_path = package_json_path(&names)?;
    let package_json = read_json_member(&mut archive, &package_json_path)?;
    let package: VsixPackageJson = serde_json::from_value(package_json)
        .map_err(|error| format!("Invalid VSIX package.json: {error}"))?;
    let (packages, ignored_code_assets) =
        generate_packages(&mut archive, &names, &package, &package_json_path)?;
    let base = crate::plugins::plugin_base(&app)?;
    let package_dir = base.join("packages");
    fs::create_dir_all(&package_dir).map_err(|error| error.to_string())?;
    let mut imported = Vec::new();
    for (index, generated) in packages.iter().enumerate() {
        let import_nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let package_path = package_dir.join(format!(
            ".vsix-import-{}-{import_nonce}-{index}.myc",
            std::process::id()
        ));
        write_generated_package(&package_path, generated)?;
        let install_result = crate::plugins::install_myc_plugin(
            app.clone(),
            package_path.to_string_lossy().into_owned(),
        );
        // The generated archive only stages this import into the existing
        // installer. It must not remain in packages/ and be discovered again
        // on the next startup.
        let _ = fs::remove_file(&package_path);
        let installed = install_result?;
        imported.push(VsixImportedPlugin {
            id: installed.manifest.metadata.id.clone(),
            name: installed.manifest.metadata.name.clone(),
            version: installed.manifest.metadata.version.clone(),
            kind: generated.kind.clone(),
            asset_count: generated.assets.len(),
        });
    }
    Ok(VsixImportReport {
        source: source_path.to_string_lossy().into_owned(),
        package_name: package.name,
        publisher: package.publisher.unwrap_or_else(|| "vscode".to_string()),
        version: package.version,
        imported,
        ignored_code_assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_sample_vsix(path: &Path) {
        let file = File::create(path).expect("sample VSIX");
        let mut writer = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        writer
            .start_file("extension/package.json", options)
            .unwrap();
        writer
            .write_all(
                br#"{
                  "name": "sample-theme",
                  "displayName": "Sample Theme",
                  "publisher": "Research Canvas",
                  "version": "1.2.3",
                  "contributes": {
                    "themes": [{"label":"Sample Dark","path":"themes/dark.json"}],
                    "iconThemes": [{"id":"sample-icons","label":"Sample Icons","path":"icons/theme.json"}]
                  }
                }"#,
            )
            .unwrap();
        writer
            .start_file("extension/themes/dark.json", options)
            .unwrap();
        writer
            .write_all(br##"{"colors":{"editor.background":"#101010"}}"##)
            .unwrap();
        writer
            .start_file("extension/icons/theme.json", options)
            .unwrap();
        writer
            .write_all(
                br##"{
                  "fileExtensions":{"rs":"rust"},
                  "folderNames":{"src":"folder-src"},
                  "iconDefinitions":{"rust":{"iconPath":"./rust.svg"}}
                }"##,
            )
            .unwrap();
        writer
            .start_file("extension/icons/rust.svg", options)
            .unwrap();
        writer
            .write_all(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>")
            .unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn native_importer_converts_theme_and_icon_contributions_without_runtime_code() {
        let root = tempdir().unwrap();
        let vsix = root.path().join("sample.vsix");
        write_sample_vsix(&vsix);
        let mut archive = ZipArchive::new(File::open(&vsix).unwrap()).unwrap();
        let names = validate_vsix_archive(&mut archive).unwrap();
        let package_path = package_json_path(&names).unwrap();
        let package: VsixPackageJson =
            serde_json::from_value(read_json_member(&mut archive, &package_path).unwrap()).unwrap();
        let (packages, ignored) =
            generate_packages(&mut archive, &names, &package, &package_path).unwrap();
        assert_eq!(packages.len(), 2);
        assert_eq!(packages[0].kind, "ThemePlugin");
        assert_eq!(packages[1].kind, "IconThemePlugin");
        assert_eq!(packages[1].assets.len(), 1);
        assert!(ignored.is_empty());
        assert_eq!(packages[1].entry_json["fileExtensions"]["rs"], "rust");
    }

    #[test]
    fn native_importer_rejects_traversal_and_native_binary_members() {
        assert!(safe_archive_name("extension/../package.json").is_err());
        assert!(has_native_extension("extension/native.dll"));
        assert!(!has_native_extension("extension/theme.json"));
    }
}
