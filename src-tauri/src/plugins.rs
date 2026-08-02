//! `.myc` 插件的桌面端安装与执行边界 / Desktop install and execution boundary for `.myc` plugins.
//!
//! 声明式视觉包只读取 JSON；分析包只执行经校验的 WebAssembly，并且默认没有主机能力。
//! Declarative visual packages only expose JSON; analysis packages execute verified WebAssembly
//! with no host capabilities by default. All archives are bounded and staged before visibility.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
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
const PLUGIN_CALL_API_VERSION: &str = "researchcanvas.dev/plugin-call/v1alpha1";
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
const REMOVED_PLUGINS_FILE: &str = "removed-plugins.json";

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
pub struct PluginContextMenuContribution {
    id: String,
    scope: String,
    label: String,
    icon: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLocaleContribution {
    locale: String,
    name: String,
    path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandContribution {
    id: String,
    label: String,
    description: String,
    category: String,
    capability: String,
    formats: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginContributions {
    context_menus: Option<Vec<PluginContextMenuContribution>>,
    locales: Option<Vec<PluginLocaleContribution>>,
    commands: Option<Vec<PluginCommandContribution>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginSpec {
    engine: String,
    entry: String,
    language: Option<String>,
    capabilities: Vec<String>,
    permissions: Vec<String>,
    contributes: Option<MycPluginContributions>,
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
    components: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMycPlugin {
    manifest: MycPluginManifest,
    install_path: String,
    theme: Option<ThemeManifest>,
    edge_style: Option<serde_json::Value>,
    runtime: Option<MycPluginRuntime>,
    locales: Option<Vec<InstalledPluginLocale>>,
    workspace: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginLocale {
    locale: String,
    name: String,
    messages: serde_json::Map<String, serde_json::Value>,
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

fn plugin_version_key(plugin_id: &str, plugin_version: &str) -> String {
    format!("{plugin_id}@{plugin_version}")
}

fn read_removed_plugins(base: &Path) -> Result<HashSet<String>, String> {
    let path = base.join(REMOVED_PLUGINS_FILE);
    if !path.is_file() {
        return Ok(HashSet::new());
    }
    let bytes = fs::read(&path).map_err(|error| error.to_string())?;
    let values: Vec<String> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid removal registry: {error}"))?;
    Ok(values.into_iter().collect())
}

fn write_removed_plugins(base: &Path, removed: &HashSet<String>) -> Result<(), String> {
    fs::create_dir_all(base).map_err(|error| error.to_string())?;
    let mut values = removed.iter().cloned().collect::<Vec<_>>();
    values.sort();
    let bytes = serde_json::to_vec_pretty(&values).map_err(|error| error.to_string())?;
    fs::write(base.join(REMOVED_PLUGINS_FILE), bytes).map_err(|error| error.to_string())
}

fn clear_removed_plugin(base: &Path, plugin_id: &str, plugin_version: &str) -> Result<(), String> {
    let mut removed = read_removed_plugins(base)?;
    if removed.remove(&plugin_version_key(plugin_id, plugin_version)) {
        write_removed_plugins(base, &removed)?;
    }
    Ok(())
}

fn uninstall_plugin_from(base: &Path, plugin_id: &str, plugin_version: &str) -> Result<(), String> {
    validate_slug(plugin_id, "plugin id")?;
    validate_slug(plugin_version, "plugin version")?;
    let key = plugin_version_key(plugin_id, plugin_version);
    let directory = base.join("installed").join(&key);
    let installed = read_installed_plugin(&directory)?;
    if installed.manifest.metadata.id != plugin_id
        || installed.manifest.metadata.version != plugin_version
    {
        return Err("Installed plugin identity does not match its directory".to_string());
    }
    fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
    let mut removed = read_removed_plugins(base)?;
    removed.insert(key);
    write_removed_plugins(base, &removed)
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
    if let Some(items) = manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.context_menus.as_ref())
    {
        if manifest.kind != "AnalysisPlugin"
            || !manifest
                .spec
                .capabilities
                .iter()
                .any(|capability| capability == "context-menu.contribute")
        {
            return Err(
                "Context menu contributions require an AnalysisPlugin with context-menu.contribute"
                    .to_string(),
            );
        }
        if items.len() > 24 {
            return Err("A plugin can contribute at most 24 context menu actions".to_string());
        }
        for item in items {
            validate_slug(&item.id, "context menu action id")?;
            if !matches!(item.scope.as_str(), "node" | "edge" | "canvas") {
                return Err(format!("Invalid context menu scope: {}", item.scope));
            }
            if item.label.trim().is_empty() || item.label.chars().count() > 64 {
                return Err("Context menu labels must contain 1 to 64 characters".to_string());
            }
            if item.icon.as_ref().is_some_and(|icon| {
                !matches!(
                    icon.as_str(),
                    "sparkles" | "search" | "wand" | "database" | "link"
                )
            }) {
                return Err("Unsupported context menu icon".to_string());
            }
        }
    }
    if let Some(locales) = manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.locales.as_ref())
    {
        if manifest.kind != "LocalePlugin"
            || !manifest
                .spec
                .capabilities
                .iter()
                .any(|capability| capability == "i18n.register")
        {
            return Err(
                "Locale contributions require a LocalePlugin with i18n.register".to_string(),
            );
        }
        if locales.len() > 16 {
            return Err("A plugin can contribute at most 16 locales".to_string());
        }
        for locale in locales {
            if locale.locale.len() > 35
                || locale.locale.is_empty()
                || !locale
                    .locale
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
            {
                return Err(format!("Invalid locale tag: {}", locale.locale));
            }
            if locale.name.trim().is_empty() || locale.name.chars().count() > 48 {
                return Err("Locale names must contain 1 to 48 characters".to_string());
            }
            let path = Path::new(&locale.path);
            if path.is_absolute()
                || path.components().count() != 2
                || path.parent() != Some(Path::new("locales"))
                || path.extension().and_then(|value| value.to_str()) != Some("json")
            {
                return Err("Locale bundles must use locales/<tag>.json".to_string());
            }
        }
    }
    if let Some(commands) = manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.commands.as_ref())
    {
        if manifest.kind != "WorkspacePlugin" {
            return Err("Workspace commands require WorkspacePlugin".to_string());
        }
        if commands.len() > 24 {
            return Err("A plugin can contribute at most 24 workspace commands".to_string());
        }
        for command in commands {
            validate_slug(&command.id, "workspace command id")?;
            if command.label.trim().is_empty()
                || command.label.chars().count() > 64
                || command.description.chars().count() > 180
            {
                return Err("Workspace command copy exceeds its bounded length".to_string());
            }
            if !matches!(
                command.category.as_str(),
                "export" | "folder" | "git" | "import"
            ) {
                return Err(format!(
                    "Unsupported workspace command category: {}",
                    command.category
                ));
            }
            if !manifest
                .spec
                .capabilities
                .iter()
                .any(|capability| capability == &command.capability)
            {
                return Err(format!(
                    "Workspace command {} requires undeclared capability {}",
                    command.id, command.capability
                ));
            }
            if command.formats.as_ref().is_some_and(|formats| {
                formats.is_empty()
                    || formats.len() > 3
                    || formats
                        .iter()
                        .any(|format| !matches!(format.as_str(), "pdf" | "svg" | "png"))
            }) {
                return Err("Export formats are limited to pdf, svg, and png".to_string());
            }
        }
    }
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
        "WorkspacePlugin" => {
            if manifest.spec.engine != "host-mediated" {
                return Err("WorkspacePlugin engine must be host-mediated".to_string());
            }
            if manifest.spec.entry != "workspace-plugin.json" {
                return Err("WorkspacePlugin entry must be workspace-plugin.json".to_string());
            }
            if manifest.spec.language.is_some() {
                return Err("WorkspacePlugin must not declare a guest language".to_string());
            }
            if manifest
                .spec
                .contributes
                .as_ref()
                .and_then(|contributions| contributions.commands.as_ref())
                .is_none_or(Vec::is_empty)
            {
                return Err("WorkspacePlugin must contribute at least one command".to_string());
            }
        }
        "LocalePlugin" => {
            if manifest.spec.engine != "declarative"
                || manifest.spec.language.is_some()
                || manifest
                    .spec
                    .contributes
                    .as_ref()
                    .and_then(|contributions| contributions.locales.as_ref())
                    .is_none_or(Vec::is_empty)
            {
                return Err(
                    "LocalePlugin requires declarative i18n locale contributions".to_string(),
                );
            }
            if !manifest.spec.entry.starts_with("locales/")
                || !manifest.spec.entry.ends_with(".json")
            {
                return Err("LocalePlugin entry must reference locales/<tag>.json".to_string());
            }
        }
        _ => {
            return Err(
                "Installer accepts ThemePlugin, EdgeStylePlugin, AnalysisPlugin, LocalePlugin, and WorkspacePlugin packages"
                    .to_string(),
            );
        }
    }
    if !manifest.spec.permissions.is_empty() {
        return Err(
            "Plugins declare capabilities; ambient permission requests are not accepted"
                .to_string(),
        );
    }
    Ok(())
}

fn read_locale_bundles(
    directory: &Path,
    manifest: &MycPluginManifest,
) -> Result<Option<Vec<InstalledPluginLocale>>, String> {
    let Some(contributions) = manifest.spec.contributes.as_ref() else {
        return Ok(None);
    };
    let Some(locales) = contributions.locales.as_ref() else {
        return Ok(None);
    };
    let mut bundles = Vec::with_capacity(locales.len());
    for contribution in locales {
        let path = directory.join(&contribution.path);
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read {}: {error}", path.display()))?;
        let value: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| error.to_string())?;
        let messages = value
            .as_object()
            .ok_or_else(|| format!("Locale bundle must be an object: {}", path.display()))?;
        if messages.len() > 2_000 {
            return Err("Locale bundle exceeds 2,000 messages".to_string());
        }
        if messages.iter().any(|(key, value)| {
            key.is_empty()
                || key.chars().count() > 128
                || value
                    .as_str()
                    .is_none_or(|message| message.chars().count() > 2_000)
        }) {
            return Err("Locale bundles accept bounded string-to-string messages only".to_string());
        }
        bundles.push(InstalledPluginLocale {
            locale: contribution.locale.clone(),
            name: contribution.name.clone(),
            messages: messages.clone(),
        });
    }
    Ok(Some(bundles))
}

fn read_installed_plugin(directory: &Path) -> Result<InstalledMycPlugin, String> {
    let manifest_path = directory.join("plugin.yml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Could not read {}: {error}", manifest_path.display()))?;
    let manifest: MycPluginManifest =
        serde_yaml::from_str(&manifest_text).map_err(|error| error.to_string())?;
    validate_manifest(&manifest)?;

    let entry_path = directory.join(&manifest.spec.entry);
    let (theme, edge_style, runtime, workspace) = match manifest.kind.as_str() {
        "ThemePlugin" => {
            let entry_text = fs::read_to_string(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            (
                Some(serde_json::from_str(&entry_text).map_err(|error| error.to_string())?),
                None,
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
                None,
            )
        }
        "WorkspacePlugin" => {
            let entry_text = fs::read_to_string(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            let descriptor: serde_json::Value =
                serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
            if descriptor
                .get("schemaVersion")
                .and_then(serde_json::Value::as_u64)
                != Some(1)
                || !matches!(
                    descriptor.get("mode").and_then(serde_json::Value::as_str),
                    Some("export" | "folder" | "git")
                )
            {
                return Err("Invalid workspace-plugin.json descriptor".to_string());
            }
            (None, None, None, Some(descriptor))
        }
        _ => (None, None, None, None),
    };
    let locales = read_locale_bundles(directory, &manifest)?;

    Ok(InstalledMycPlugin {
        manifest,
        install_path: directory.to_string_lossy().into_owned(),
        theme,
        edge_style,
        runtime,
        locales,
        workspace,
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
    let base = plugin_base(app)?;
    let removed = read_removed_plugins(&base)?;
    #[cfg(debug_assertions)]
    let package_roots = vec![base.join("packages")];
    #[cfg(not(debug_assertions))]
    let package_roots = vec![
        base.join("packages"),
        app.path()
            .resource_dir()
            .map_err(|error| error.to_string())?
            .join("plugins/packages"),
    ];
    for packages in package_roots {
        if !packages.is_dir() {
            continue;
        }
        for entry in fs::read_dir(packages).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if !path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("myc"))
            {
                continue;
            }
            let already_installed = path
                .file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|package| base.join("installed").join(package).is_dir());
            let explicitly_removed = path
                .file_stem()
                .and_then(|value| value.to_str())
                .is_some_and(|package| removed.contains(package));
            if !already_installed && !explicitly_removed {
                install_archive(app, &path)?;
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn install_myc_plugin(app: AppHandle, path: String) -> Result<InstalledMycPlugin, String> {
    let installed = install_archive(&app, Path::new(&path))?;
    let base = plugin_base(&app)?;
    clear_removed_plugin(
        &base,
        &installed.manifest.metadata.id,
        &installed.manifest.metadata.version,
    )?;
    Ok(installed)
}

#[tauri::command]
pub fn uninstall_myc_plugin(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
) -> Result<(), String> {
    let base = plugin_base(&app)?;
    uninstall_plugin_from(&base, &plugin_id, &plugin_version)
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

/** 原生动作前解析已安装包并验证一个命名能力 / Resolve an installed package and prove one capability. */
pub fn require_plugin_capability(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    capability: &str,
) -> Result<PathBuf, String> {
    require_plugin_capabilities(app, plugin_id, plugin_version, &[capability])
}

/** 解析一个 WorkspacePlugin 并验证全部宿主能力 / Resolve one WorkspacePlugin and prove all requested capabilities. */
pub fn require_plugin_capabilities(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    capabilities: &[&str],
) -> Result<PathBuf, String> {
    validate_slug(plugin_id, "plugin id")?;
    validate_slug(plugin_version, "plugin version")?;
    let directory = plugin_base(app)?
        .join("installed")
        .join(format!("{plugin_id}@{plugin_version}"));
    let installed = read_installed_plugin(&directory)?;
    if installed.manifest.kind != "WorkspacePlugin" {
        return Err("Native workspace actions require WorkspacePlugin".to_string());
    }
    for capability in capabilities {
        if !installed
            .manifest
            .spec
            .capabilities
            .iter()
            .any(|declared| declared == capability)
        {
            return Err(format!(
                "Plugin {plugin_id}@{plugin_version} does not declare {capability}"
            ));
        }
    }
    Ok(directory)
}

/** 再次确认导出格式属于已声明命令 / Revalidate that an export format belongs to a declared command. */
pub fn require_plugin_export_format(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    format: &str,
) -> Result<PathBuf, String> {
    let directory = require_plugin_capability(app, plugin_id, plugin_version, "project.export")?;
    let installed = read_installed_plugin(&directory)?;
    let declared = installed
        .manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.commands.as_ref())
        .is_some_and(|commands| {
            commands.iter().any(|command| {
                command.category == "export"
                    && command.capability == "project.export"
                    && command
                        .formats
                        .as_ref()
                        .is_some_and(|formats| formats.iter().any(|candidate| candidate == format))
            })
        });
    if !declared {
        return Err(format!(
            "Plugin {plugin_id}@{plugin_version} does not contribute {format} export"
        ));
    }
    Ok(directory)
}

fn validate_analysis_call(
    installed: &InstalledMycPlugin,
    input: &serde_json::Value,
) -> Result<(), String> {
    let object = input
        .as_object()
        .ok_or_else(|| "Plugin call must be a JSON object".to_string())?;
    if object.get("apiVersion").and_then(serde_json::Value::as_str) != Some(PLUGIN_CALL_API_VERSION)
    {
        return Err(format!(
            "Plugin call apiVersion must be {PLUGIN_CALL_API_VERSION}"
        ));
    }
    let operation = object
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Plugin call operation is required".to_string())?;
    validate_slug(operation, "plugin operation")?;
    if operation != "context-menu" {
        return Ok(());
    }
    if !installed
        .manifest
        .spec
        .capabilities
        .iter()
        .any(|capability| capability == "context-menu.contribute")
    {
        return Err("Context-menu calls require context-menu.contribute".to_string());
    }
    let context = object
        .get("context")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| "Context-menu call context is required".to_string())?;
    let action_id = context
        .get("actionId")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Context-menu actionId is required".to_string())?;
    let scope = context
        .get("scope")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "Context-menu scope is required".to_string())?;
    let declared = installed
        .manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.context_menus.as_ref())
        .is_some_and(|actions| {
            actions
                .iter()
                .any(|action| action.id == action_id && action.scope == scope)
        });
    if !declared {
        return Err(format!(
            "Plugin does not contribute context-menu action {action_id} for {scope}"
        ));
    }
    Ok(())
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
    if !installed
        .manifest
        .spec
        .capabilities
        .iter()
        .any(|capability| capability == "analysis.run")
    {
        return Err("AnalysisPlugin must declare analysis.run".to_string());
    }
    validate_analysis_call(&installed, &input)?;
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

    fn runtime_plugin_with_context_menu() -> InstalledMycPlugin {
        let mut manifest: MycPluginManifest =
            serde_yaml::from_str(&runtime_manifest("rust")).expect("parse runtime manifest");
        manifest.metadata.version = "1.1.0".to_string();
        manifest
            .spec
            .capabilities
            .push("context-menu.contribute".to_string());
        manifest.spec.contributes = Some(MycPluginContributions {
            context_menus: Some(vec![PluginContextMenuContribution {
                id: "inspect-context".to_string(),
                scope: "node".to_string(),
                label: "Analyze node context".to_string(),
                icon: Some("sparkles".to_string()),
            }]),
            locales: None,
            commands: None,
        });
        InstalledMycPlugin {
            manifest,
            install_path: "test".to_string(),
            theme: None,
            edge_style: None,
            runtime: Some(MycPluginRuntime {
                engine: "wasm32-myc".to_string(),
                language: "rust".to_string(),
                entry_sha256: "0".repeat(64),
            }),
            locales: None,
            workspace: None,
        }
    }

    #[test]
    fn validates_versioned_analysis_call_envelopes_and_declared_actions() {
        let installed = runtime_plugin_with_context_menu();
        validate_analysis_call(
            &installed,
            &json!({
                "apiVersion": PLUGIN_CALL_API_VERSION,
                "operation": "self-test",
                "payload": {}
            }),
        )
        .expect("versioned self-test is accepted");
        validate_analysis_call(
            &installed,
            &json!({
                "apiVersion": PLUGIN_CALL_API_VERSION,
                "operation": "context-menu",
                "context": {
                    "actionId": "inspect-context",
                    "scope": "node",
                    "targetId": "node-1",
                    "projectId": "project-1"
                }
            }),
        )
        .expect("declared action and scope are accepted");

        assert!(validate_analysis_call(
            &installed,
            &json!({"apiVersion": "legacy", "operation": "self-test"}),
        )
        .expect_err("legacy envelopes are rejected")
        .contains("apiVersion"));
        assert!(validate_analysis_call(
            &installed,
            &json!({
                "apiVersion": PLUGIN_CALL_API_VERSION,
                "operation": "context-menu",
                "context": {"actionId": "undeclared", "scope": "node"}
            }),
        )
        .expect_err("undeclared host actions are rejected")
        .contains("does not contribute"));
        assert!(validate_analysis_call(
            &installed,
            &json!({
                "apiVersion": PLUGIN_CALL_API_VERSION,
                "operation": "context-menu",
                "context": {"actionId": "inspect-context", "scope": "canvas"}
            }),
        )
        .expect_err("scope escalation is rejected")
        .contains("does not contribute"));
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

        uninstall_plugin_from(root.path(), "researchcanvas.runtime-smoke", "1.0.0")
            .expect("uninstall exact plugin version");
        assert!(!root
            .path()
            .join("installed/researchcanvas.runtime-smoke@1.0.0")
            .exists());
        assert!(read_removed_plugins(root.path())
            .expect("removal tombstones")
            .contains("researchcanvas.runtime-smoke@1.0.0"));
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

    #[test]
    fn context_menu_contributions_require_runtime_capability() {
        let mut manifest: MycPluginManifest =
            serde_yaml::from_str(&runtime_manifest("rust")).expect("parse manifest");
        manifest.spec.contributes = Some(MycPluginContributions {
            context_menus: Some(vec![PluginContextMenuContribution {
                id: "inspect-context".to_string(),
                scope: "node".to_string(),
                label: "Analyze node context".to_string(),
                icon: Some("sparkles".to_string()),
            }]),
            locales: None,
            commands: None,
        });
        assert!(validate_manifest(&manifest)
            .expect_err("missing contribution capability is rejected")
            .contains("context-menu.contribute"));
        manifest
            .spec
            .capabilities
            .push("context-menu.contribute".to_string());
        validate_manifest(&manifest).expect("bounded runtime contribution is accepted");
    }

    #[test]
    fn installs_host_mediated_workspace_and_declarative_locale_packages() {
        let root = tempdir().expect("temp root");
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let workspace_package = root.path().join("workspace.myc");
        let file = File::create(&workspace_package).expect("workspace archive");
        let mut archive = ZipWriter::new(file);
        archive.start_file("plugin.yml", options).expect("manifest");
        archive
            .write_all(
                br#"apiVersion: researchcanvas.dev/v1alpha1
kind: WorkspacePlugin
metadata:
  id: researchcanvas.test-export
  name: Test Export
  version: 1.0.0
  publisher: Research Canvas
  developer: Workspace Tests
  description: Test host mediated export capability.
spec:
  engine: host-mediated
  entry: workspace-plugin.json
  capabilities: [project.export]
  permissions: []
  contributes:
    commands:
      - id: export
        label: Export SVG
        description: Export the reviewed project.
        category: export
        capability: project.export
        formats: [svg]
"#,
            )
            .expect("manifest bytes");
        archive
            .start_file("workspace-plugin.json", options)
            .expect("workspace descriptor");
        archive
            .write_all(br#"{"schemaVersion":1,"mode":"export","testFixture":"pinn-architecture"}"#)
            .expect("workspace descriptor bytes");
        archive.finish().expect("workspace package");

        let installed =
            install_archive_into(root.path(), &workspace_package).expect("install workspace");
        assert_eq!(installed.manifest.kind, "WorkspacePlugin");
        assert_eq!(
            installed.workspace.expect("workspace descriptor")["mode"],
            "export"
        );
        assert!(installed.runtime.is_none());

        let locale_package = root.path().join("locale.myc");
        let file = File::create(&locale_package).expect("locale archive");
        let mut archive = ZipWriter::new(file);
        archive.start_file("plugin.yml", options).expect("manifest");
        archive
            .write_all(
                "apiVersion: researchcanvas.dev/v1alpha1\nkind: LocalePlugin\nmetadata:\n  id: researchcanvas.test-ja\n  name: Test Japanese\n  version: 1.0.0\n  publisher: Research Canvas\n  developer: Locale Tests\n  description: Test declarative community language.\nspec:\n  engine: declarative\n  entry: locales/ja-JP.json\n  capabilities: [i18n.register]\n  permissions: []\n  contributes:\n    locales:\n      - locale: ja-JP\n        name: 日本語\n        path: locales/ja-JP.json\n"
                    .as_bytes(),
            )
            .expect("locale manifest bytes");
        archive
            .start_file("locales/ja-JP.json", options)
            .expect("locale bundle");
        archive
            .write_all("{\"workspace.menu\":\"メニュー\"}".as_bytes())
            .expect("locale bytes");
        archive.finish().expect("locale package");

        let installed = install_archive_into(root.path(), &locale_package).expect("install locale");
        let locales = installed.locales.expect("installed locales");
        assert_eq!(locales[0].locale, "ja-JP");
        assert_eq!(locales[0].messages["workspace.menu"], "メニュー");
        assert!(installed.runtime.is_none());
    }
}
