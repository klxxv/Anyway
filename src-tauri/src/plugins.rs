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

use crate::llm_plugin::{self, ProviderDescriptor};

const MYC_API_VERSION: &str = "researchcanvas.dev/v1alpha1";
const PLUGIN_CALL_API_VERSION: &str = "researchcanvas.dev/plugin-call/v1alpha1";
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
const REMOVED_PLUGINS_FILE: &str = "removed-plugins.json";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub developer: String,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginContextMenuContribution {
    pub id: String,
    pub scope: String,
    pub label: String,
    pub icon: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginLocaleContribution {
    pub locale: String,
    pub name: String,
    pub path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandContribution {
    pub id: String,
    pub label: String,
    pub description: String,
    pub category: String,
    pub capability: String,
    pub formats: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginContributions {
    pub context_menus: Option<Vec<PluginContextMenuContribution>>,
    pub locales: Option<Vec<PluginLocaleContribution>>,
    pub commands: Option<Vec<PluginCommandContribution>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginSpec {
    pub engine: String,
    pub entry: String,
    pub language: Option<String>,
    pub capabilities: Vec<String>,
    pub permissions: Vec<String>,
    pub contributes: Option<MycPluginContributions>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginManifest {
    pub api_version: String,
    pub kind: String,
    pub metadata: MycPluginMetadata,
    pub spec: MycPluginSpec,
    /// 包内每个载荷文件(plugin.yml 除外)的 sha256:相对路径 → 64 位小写十六进制。
    /// 签名覆盖清单 JSON,清单携带 payloads 后签名即覆盖全部载荷。
    /// sha256 of every payload file in the package (except plugin.yml itself):
    /// relative path → 64 lowercase hex chars. Since the signature covers the
    /// manifest JSON, a manifest carrying payloads extends the signature to
    /// every payload byte.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payloads: Option<std::collections::BTreeMap<String, String>>,
    /// 发布者对清单内容的 Ed25519 签名（base64 编码，覆盖不含本字段的 JSON 序列化的 SHA-256）。
    /// Ed25519 signature (base64) over SHA-256 of the JSON-serialized manifest without this field.
    #[serde(default)]
    pub signature: Option<String>,
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
    /// ThemePlugin 可内嵌边样式，统一颜色+连线外观。
    edge_style: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMycPlugin {
    pub(crate) manifest: MycPluginManifest,
    pub(crate) install_path: String,
    pub(crate) theme: Option<ThemeManifest>,
    pub(crate) edge_style: Option<serde_json::Value>,
    pub(crate) runtime: Option<MycPluginRuntime>,
    pub(crate) locales: Option<Vec<InstalledPluginLocale>>,
    pub(crate) workspace: Option<serde_json::Value>,
    pub provider: Option<ProviderDescriptor>,
    pub(crate) agent: Option<serde_json::Value>,
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
    // 容忍历史 `{}` 写入（空对象等价空集合），但拒绝其它畸形内容 / Tolerate a
    // legacy `{}` write (empty object == empty set); reject anything else malformed.
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("Invalid removal registry: {error}"))?;
    match value {
        serde_json::Value::Array(items) => {
            let mut removed = HashSet::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(entry) => {
                        removed.insert(entry.to_string());
                    }
                    None => return Err("Invalid removal registry: entries must be strings".to_string()),
                }
            }
            Ok(removed)
        }
        serde_json::Value::Object(map) if map.is_empty() => Ok(HashSet::new()),
        _ => Err("Invalid removal registry: expected a string array".to_string()),
    }
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
        if manifest.kind != "WorkspacePlugin" && manifest.kind != "ProviderPlugin" {
            return Err("Workspace commands require WorkspacePlugin or ProviderPlugin".to_string());
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
                "export" | "folder" | "git" | "import" | "llm-provider"
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
        "ProviderPlugin" => {
            if manifest.spec.engine != "host-mediated" {
                return Err("ProviderPlugin engine must be host-mediated".to_string());
            }
            if manifest.spec.entry != "provider.json" {
                return Err("ProviderPlugin entry must be provider.json".to_string());
            }
            if manifest.spec.language.is_some() {
                return Err("ProviderPlugin must not declare a guest language".to_string());
            }
            let has_llm = manifest
                .spec
                .capabilities
                .iter()
                .any(|c| c == "llm.chat" || c == "llm.configure");
            if !has_llm {
                return Err(
                    "ProviderPlugin must declare at least one of llm.chat or llm.configure"
                        .to_string(),
                );
            }
        }
        "AgentPlugin" => {
            if manifest.spec.engine != "host-mediated" {
                return Err("AgentPlugin engine must be host-mediated".to_string());
            }
            if manifest.spec.entry != "agent-manifest.json" {
                return Err("AgentPlugin entry must be agent-manifest.json".to_string());
            }
            if manifest.spec.language.is_some() {
                return Err("AgentPlugin must not declare a guest language".to_string());
            }
            const AGENT_CAPABILITIES: [&str; 3] = [
                "agent.pdf.read",
                "agent.graph.patch.propose",
                "agent.review.request",
            ];
            if manifest.spec.capabilities.is_empty()
                || !manifest
                    .spec
                    .capabilities
                    .iter()
                    .all(|capability| AGENT_CAPABILITIES.contains(&capability.as_str()))
            {
                return Err(
                    "AgentPlugin capabilities must be a non-empty subset of agent.pdf.read, agent.graph.patch.propose, agent.review.request"
                        .to_string(),
                );
            }
        }
        _ => {
            return Err(
                "Installer accepts ThemePlugin, EdgeStylePlugin, AnalysisPlugin, LocalePlugin, WorkspacePlugin, ProviderPlugin, and AgentPlugin packages"
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
    if let Some(payloads) = manifest.payloads.as_ref() {
        if payloads.is_empty() {
            return Err("Payloads map must not be empty".to_string());
        }
        for (path, digest) in payloads {
            let relative = Path::new(path);
            if path == "plugin.yml"
                || relative.is_absolute()
                || relative
                    .components()
                    .any(|component| !matches!(component, std::path::Component::Normal(_)))
                || path.contains('\\')
            {
                return Err(format!("Invalid payload path: {path}"));
            }
            if digest.len() != 64
                || !digest.chars().all(|character| character.is_ascii_hexdigit())
                || digest != &digest.to_lowercase()
            {
                return Err(format!("Invalid payload sha256 for {path}"));
            }
        }
    }
    if manifest.signature.is_some() && manifest.payloads.is_none() {
        return Err(
            "Signed packages must declare payloads so the signature covers every payload byte"
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
    let (theme, edge_style, runtime, workspace, provider, agent) = match manifest.kind.as_str() {
        "ThemePlugin" => {
            let entry_text = fs::read_to_string(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            let theme: ThemeManifest =
                serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
            let edge_style = theme.edge_style.clone();
            (
                Some(theme),
                edge_style,
                None,
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
                None,
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
            (None, None, None, Some(descriptor), None, None)
        }
        "ProviderPlugin" => {
            let entry_text = fs::read_to_string(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            let descriptor: ProviderDescriptor =
                serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
            llm_plugin::validate_provider_descriptor(&descriptor)?;
            (None, None, None, None, Some(descriptor), None)
        }
        "AgentPlugin" => {
            let entry_text = fs::read_to_string(&entry_path)
                .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
            let descriptor: serde_json::Value =
                serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
            if descriptor
                .get("schemaVersion")
                .and_then(serde_json::Value::as_u64)
                != Some(1)
                || descriptor.get("mode").and_then(serde_json::Value::as_str) != Some("agent")
                || descriptor
                    .get("reviewGated")
                    .and_then(serde_json::Value::as_bool)
                    != Some(true)
            {
                return Err(
                    "Invalid agent-manifest.json descriptor: requires schemaVersion 1, mode \"agent\", and reviewGated true"
                        .to_string(),
                );
            }
            (None, None, None, None, None, Some(descriptor))
        }
        _ => (None, None, None, None, None, None),
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
        provider,
        agent,
    })
}

/// 将清单序列化为 JSON 并移除 signature 字段，用于签名验证。
/// Serializes manifest to JSON with the signature field removed, for signature verification.
fn manifest_to_json_without_signature(manifest: &MycPluginManifest) -> Result<serde_json::Value, String> {
    let mut value = serde_json::to_value(manifest)
        .map_err(|error| format!("Manifest serialization failed: {error}"))?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("signature");
    }
    Ok(value)
}

/// 原子移动到 `installed` 前校验并暂存归档 / Validates and stages an archive before atomically renaming it into `installed`.
/// 递归收集目录下全部文件路径(载荷校验用,不引外部 walkdir 依赖)。
/// Recursively collects every file under a directory for payload verification.
fn walkdir_payloads(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let path = entry.map_err(|error| error.to_string())?.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

/// 从已打开的归档中读取 plugin.yml(含签名校验),供发现与安装共用。
/// Reads plugin.yml from an already-opened archive (with signature verification),
/// shared by discovery and install. This avoids a second `File::open` and the
/// time-of-check/time-of-use race it introduces.
fn read_archive_manifest_from_archive(
    base: &Path,
    archive: &mut ZipArchive<File>,
) -> Result<MycPluginManifest, String> {
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

    // --- Ed25519 签名验证 / Ed25519 signature verification ---
    if let Some(ref signature_b64) = manifest.signature {
        if signature_b64.trim().is_empty() {
            return Err("Plugin manifest contains an empty signature field".to_string());
        }
        let trusted_keys = crate::signing::load_all_trusted_keys(base)?;
        let manifest_without_sig = manifest_to_json_without_signature(&manifest)?;
        crate::signing::verify_manifest_signature(
            &manifest.metadata.publisher,
            &manifest_without_sig,
            signature_b64,
            &trusted_keys,
        )?;
    }
    // --- 签名验证结束 / End signature verification ---

    Ok(manifest)
}

/// 仅读取归档中的 plugin.yml(含签名校验)。
/// Reads only plugin.yml from an archive (with signature verification).
fn read_archive_manifest(base: &Path, archive_path: &Path) -> Result<MycPluginManifest, String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    read_archive_manifest_from_archive(base, &mut archive)
}

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

    // 只打开一次归档：先读清单并校验签名，再复用同一句柄解压。
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    let manifest = read_archive_manifest_from_archive(base, &mut archive)?;

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

    // 载荷完整性:声明了 payloads 的包,每个暂存文件必须与清单哈希一致,
    // 未列出的文件一律拒绝 — 签名因此覆盖包内每一字节。
    // Payload integrity: when the manifest declares payloads, every staged
    // file must hash to the declared value and unlisted files are rejected —
    // the manifest signature then covers every byte in the package.
    if let Some(payloads) = manifest.payloads.as_ref() {
        let verification = (|| -> Result<(), String> {
            let mut seen = std::collections::HashSet::new();
            for entry in walkdir_payloads(&staging)? {
                let relative = entry
                    .strip_prefix(&staging)
                    .map_err(|error| error.to_string())?
                    .iter()
                    .map(|part| part.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                if relative == "plugin.yml" {
                    continue;
                }
                let Some(expected) = payloads.get(&relative) else {
                    return Err(format!("Unlisted payload file in signed package: {relative}"));
                };
                let bytes = fs::read(&entry).map_err(|error| error.to_string())?;
                let actual = format!("{:x}", Sha256::digest(&bytes));
                if &actual != expected {
                    return Err(format!("Payload hash mismatch: {relative}"));
                }
                seen.insert(relative);
            }
            for listed in payloads.keys() {
                if !seen.contains(listed) {
                    return Err(format!("Declared payload missing from package: {listed}"));
                }
            }
            Ok(())
        })();
        if let Err(error) = verification {
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
    }

    let staged = read_installed_plugin(&staging)?;
    if staged.manifest.metadata.id != manifest.metadata.id
        || staged.manifest.metadata.version != manifest.metadata.version
    {
        let _ = fs::remove_dir_all(&staging);
        return Err("Manifest changed during extraction".to_string());
    }

    fs::rename(&staging, &destination).map_err(|error| error.to_string())?;
    // 只有真正完成安装时才清除墓碑；重复安装 no-op 必须保留墓碑。
    // Only clear the removal tombstone when a fresh install completes.
    let _ = clear_removed_plugin(base, &manifest.metadata.id, &manifest.metadata.version);
    read_installed_plugin(&destination)
}

fn install_archive(app: &AppHandle, archive_path: &Path) -> Result<InstalledMycPlugin, String> {
    let base = plugin_base(app)?;
    install_archive_into(&base, archive_path)
}

fn install_pending_from(base: &Path, packages: &Path, removed: &HashSet<String>) -> Result<(), String> {
    if !packages.is_dir() {
        return Ok(());
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
        // 安装目录与墓碑都以 manifest id@version 为准,与文件名无关;
        // 发现阶段必须按同一身份判断,否则改名包会重复安装、墓碑失效。
        // Installed dirs and tombstones key on manifest id@version, not the
        // filename; discovery must use the same identity or renamed packages
        // reinstall forever and tombstones silently stop working.
        let manifest = match read_archive_manifest(base, &path) {
            Ok(manifest) => manifest,
            Err(error) => {
                eprintln!(
                    "Skipping invalid plugin package {}: {error}",
                    path.display()
                );
                continue;
            }
        };
        let directory_name = format!("{}@{}", manifest.metadata.id, manifest.metadata.version);
        let already_installed = base.join("installed").join(&directory_name).is_dir();
        let explicitly_removed = removed.contains(&directory_name);
        if !already_installed && !explicitly_removed {
            // 一个坏包不能毒化整个插件发现:跳过并继续其他包。
            // A bad package must not poison discovery; skip it and continue.
            if let Err(error) = install_archive_into(base, &path) {
                eprintln!(
                    "Skipping invalid plugin package {}: {error}",
                    path.display()
                );
            }
        }
    }
    Ok(())
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
        install_pending_from(&base, &packages, &removed)?;
    }
    Ok(())
}

/// 将用户传入的插件路径解析为允许的 packages 目录下的真实路径。
/// Resolves a caller-supplied plugin path to a real path inside the configured
/// `packages` directory. Rejects paths that escape the directory.
fn resolve_package_path(base: &Path, path: &Path) -> Result<PathBuf, String> {
    let allowed = base.join("packages");
    let input = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve plugin path: {error}"))?;
    let normalized_allowed = allowed.canonicalize().unwrap_or(allowed);
    if !input.starts_with(&normalized_allowed) {
        return Err(
            "Plugin path must be inside the configured packages directory".to_string(),
        );
    }
    Ok(input)
}

#[tauri::command]
pub fn install_myc_plugin(app: AppHandle, path: String) -> Result<InstalledMycPlugin, String> {
    let base = plugin_base(&app)?;
    let input = resolve_package_path(&base, Path::new(&path))?;
    // install_archive_into 仅在真正完成安装时清除墓碑；重复安装 no-op 保留墓碑。
    install_archive(&app, &input)
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
  id: myc.runtime-smoke
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
            provider: None,
            agent: None,
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
                .join("installed/myc.runtime-smoke@1.0.0/plugin.wasm"),
            "myc.runtime-smoke",
            "1.0.0",
            &json!({"operation": "self-test"}),
        )
        .expect("execute installed package");
        assert_eq!(output.output, json!({"runtime": "ok"}));

        uninstall_plugin_from(root.path(), "myc.runtime-smoke", "1.0.0")
            .expect("uninstall exact plugin version");
        assert!(!root
            .path()
            .join("installed/myc.runtime-smoke@1.0.0")
            .exists());
        assert!(read_removed_plugins(root.path())
            .expect("removal tombstones")
            .contains("myc.runtime-smoke@1.0.0"));
    }

    #[test]
    fn repeat_install_no_op_preserves_removal_tombstone() {
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

        install_archive_into(root.path(), &package).expect("first install");
        uninstall_plugin_from(root.path(), "myc.runtime-smoke", "1.0.0")
            .expect("uninstall creates tombstone");
        assert!(read_removed_plugins(root.path())
            .expect("removal tombstones")
            .contains("myc.runtime-smoke@1.0.0"));

        // 重复安装已存在的包是 no-op，但不得清除墓碑。
        install_archive_into(root.path(), &package).expect("repeat install is no-op");
        assert!(read_removed_plugins(root.path())
            .expect("removal tombstones")
            .contains("myc.runtime-smoke@1.0.0"));
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
  id: myc.test-export
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
                "apiVersion: researchcanvas.dev/v1alpha1\nkind: LocalePlugin\nmetadata:\n  id: myc.test-ja\n  name: Test Japanese\n  version: 1.0.0\n  publisher: Research Canvas\n  developer: Locale Tests\n  description: Test declarative community language.\nspec:\n  engine: declarative\n  entry: locales/ja-JP.json\n  capabilities: [i18n.register]\n  permissions: []\n  contributes:\n    locales:\n      - locale: ja-JP\n        name: 日本語\n        path: locales/ja-JP.json\n"
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

    #[test]
    fn installs_host_mediated_agent_package() {
        let root = tempdir().expect("temp root");
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let agent_package = root.path().join("agent.myc");
        let file = File::create(&agent_package).expect("agent archive");
        let mut archive = ZipWriter::new(file);
        archive.start_file("plugin.yml", options).expect("manifest");
        archive
            .write_all(
                br#"apiVersion: researchcanvas.dev/v1alpha1
kind: AgentPlugin
metadata:
  id: myc.test-agent
  name: Test Agent
  version: 0.1.0
  publisher: Research Canvas
  developer: Agent Tests
  description: Test host-mediated review-gated agent.
spec:
  engine: host-mediated
  entry: agent-manifest.json
  capabilities: [agent.pdf.read, agent.graph.patch.propose, agent.review.request]
  permissions: []
"#,
            )
            .expect("manifest bytes");
        archive
            .start_file("agent-manifest.json", options)
            .expect("agent descriptor");
        archive
            .write_all(
                br#"{"schemaVersion":1,"mode":"agent","agentType":"pdf-canvas","reviewGated":true}"#,
            )
            .expect("agent descriptor bytes");
        archive.finish().expect("agent package");

        let installed = install_archive_into(root.path(), &agent_package).expect("install agent");
        assert_eq!(installed.manifest.kind, "AgentPlugin");
        assert_eq!(installed.agent.expect("agent descriptor")["mode"], "agent");
        assert!(installed.runtime.is_none());
        assert!(installed.workspace.is_none());

        // 非审阅门控的 agent 描述符必须被拒绝 / Non-review-gated descriptors are rejected.
        let rogue_package = root.path().join("rogue-agent.myc");
        let file = File::create(&rogue_package).expect("rogue archive");
        let mut archive = ZipWriter::new(file);
        archive.start_file("plugin.yml", options).expect("manifest");
        archive
            .write_all(
                br#"apiVersion: researchcanvas.dev/v1alpha1
kind: AgentPlugin
metadata:
  id: myc.rogue-agent
  name: Rogue Agent
  version: 0.1.0
  publisher: Research Canvas
  developer: Agent Tests
  description: Agent descriptor that is not review-gated.
spec:
  engine: host-mediated
  entry: agent-manifest.json
  capabilities: [agent.graph.patch.propose]
  permissions: []
"#,
            )
            .expect("manifest bytes");
        archive
            .start_file("agent-manifest.json", options)
            .expect("agent descriptor");
        archive
            .write_all(br#"{"schemaVersion":1,"mode":"agent","reviewGated":false}"#)
            .expect("agent descriptor bytes");
        archive.finish().expect("rogue package");

        let error = install_archive_into(root.path(), &rogue_package)
            .expect_err("non-review-gated agent must be rejected");
        assert!(error.contains("reviewGated"), "unexpected error: {error}");

        // 未知 agent 能力也必须被拒绝 / Unknown agent capabilities are rejected.
        let unknown_package = root.path().join("unknown-agent.myc");
        let file = File::create(&unknown_package).expect("unknown archive");
        let mut archive = ZipWriter::new(file);
        archive.start_file("plugin.yml", options).expect("manifest");
        archive
            .write_all(
                br#"apiVersion: researchcanvas.dev/v1alpha1
kind: AgentPlugin
metadata:
  id: myc.unknown-agent
  name: Unknown Capability Agent
  version: 0.1.0
  publisher: Research Canvas
  developer: Agent Tests
  description: Agent declaring an unknown capability.
spec:
  engine: host-mediated
  entry: agent-manifest.json
  capabilities: [agent.filesystem.write]
  permissions: []
"#,
            )
            .expect("manifest bytes");
        archive
            .start_file("agent-manifest.json", options)
            .expect("agent descriptor");
        archive
            .write_all(br#"{"schemaVersion":1,"mode":"agent","reviewGated":true}"#)
            .expect("agent descriptor bytes");
        archive.finish().expect("unknown package");

        let error = install_archive_into(root.path(), &unknown_package)
            .expect_err("unknown agent capability must be rejected");
        assert!(error.contains("capabilities"), "unexpected error: {error}");
    }

    #[test]
    fn pending_installs_skip_corrupt_packages_and_keep_discovering() {
        let root = tempdir().expect("temp root");
        let packages = root.path().join("packages");
        fs::create_dir_all(&packages).expect("packages dir");

        // 坏包在前(文件名排序靠前),好包在后:坏包不能阻断好包安装。
        // The corrupt package sorts first; it must not block the valid one.
        fs::write(packages.join("aaa.corrupt@1.0.0.myc"), b"not a zip")
            .expect("corrupt package");

        // 好包的文件名故意与 manifest id@version 不一致:
        // 发现、去重、墓碑都必须按 manifest 身份而不是文件名。
        // The valid package filename deliberately differs from its manifest
        // id@version: discovery, dedupe, and tombstones all key on identity.
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let valid_package = packages.join("zzz.renamed-file@9.9.9.myc");
        let file = File::create(&valid_package).expect("valid archive");
        let mut archive = ZipWriter::new(file);
        archive.start_file("plugin.yml", options).expect("manifest");
        archive
            .write_all(
                br#"apiVersion: researchcanvas.dev/v1alpha1
kind: ThemePlugin
metadata:
  id: myc.valid-theme
  name: Valid Theme
  version: 1.0.0
  publisher: Research Canvas
  developer: Tests
  description: A valid theme package.
spec:
  engine: declarative
  entry: theme.json
  capabilities: [theme.register]
  permissions: []
"#,
            )
            .expect("manifest bytes");
        archive
            .start_file("theme.json", options)
            .expect("theme payload");
        archive
            .write_all(
                br#"{"id":"myc.valid-theme","name":"Valid Theme","publisher":"Research Canvas","colors":{}}"#,
            )
            .expect("theme bytes");
        archive.finish().expect("valid package");

        install_pending_from(root.path(), &packages, &HashSet::new())
            .expect("pending installs complete despite the corrupt package");

        let installed_dir = root.path().join("installed").join("myc.valid-theme@1.0.0");
        assert!(
            installed_dir.is_dir(),
            "valid package installs under its manifest identity"
        );
        assert!(
            !root.path()
                .join("installed")
                .join("aaa.corrupt@1.0.0")
                .exists(),
            "corrupt package must not be installed"
        );

        // 再次运行不再重复安装(按 manifest 身份识别已装);
        // A second pass recognizes the install by identity, not filename.
        install_pending_from(root.path(), &packages, &HashSet::new())
            .expect("second pass is a no-op");

        // 墓碑按 manifest id@version 生效,即使文件名不同。
        // Tombstones key on manifest id@version even when filenames differ.
        let root2 = tempdir().expect("second temp root");
        let packages2 = root2.path().join("packages");
        fs::create_dir_all(&packages2).expect("second packages dir");
        fs::copy(&valid_package, packages2.join("zzz.renamed-file@9.9.9.myc"))
            .expect("copy valid package");
        let mut tombstoned = HashSet::new();
        tombstoned.insert("myc.valid-theme@1.0.0".to_string());
        install_pending_from(root2.path(), &packages2, &tombstoned)
            .expect("tombstoned pass completes");
        assert!(
            !root2.path()
                .join("installed")
                .join("myc.valid-theme@1.0.0")
                .exists(),
            "tombstoned package must not install regardless of filename"
        );
    }

    // ------------------------------------------------------------------
    // Ed25519 signature verification tests
    // ------------------------------------------------------------------

    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use ed25519_dalek::{Signature, Signer, SigningKey};

    /// A minimal valid theme.json that satisfies ThemeManifest deserialization.
    fn valid_theme_json() -> serde_json::Value {
        serde_json::json!({
            "id": "test.theme",
            "name": "Test Theme",
            "publisher": "test-publisher",
            "colors": {}
        })
    }

    fn theme_payload_hash() -> String {
        format!(
            "{:x}",
            Sha256::digest(valid_theme_json().to_string().as_bytes())
        )
    }

    fn signed_theme_manifest(publisher: &str, sign_fn: Option<&dyn Fn(&str) -> String>) -> String {
        let mut yaml = format!(
            r#"apiVersion: researchcanvas.dev/v1alpha1
kind: ThemePlugin
metadata:
  id: {publisher}.test-theme
  name: Signed Theme
  version: 1.0.0
  publisher: {publisher}
  developer: Test
  description: A signed theme plugin.
spec:
  engine: declarative
  entry: theme.json
  capabilities:
    - theme.register
  permissions: []
"#
        );
        if let Some(sign) = sign_fn {
            // Signed manifests must declare payloads; the hash matches valid_theme_json().
            let theme_hash = theme_payload_hash();
            yaml.push_str(&format!("payloads:\n  theme.json: {theme_hash}\n"));
            // Must match the exact JSON shape produced by serde_json::to_value(MycPluginManifest),
            // including Option fields that serialize as null.
            let manifest_value = serde_json::json!({
                "apiVersion": "researchcanvas.dev/v1alpha1",
                "kind": "ThemePlugin",
                "metadata": {
                    "id": format!("{publisher}.test-theme"),
                    "name": "Signed Theme",
                    "version": "1.0.0",
                    "publisher": publisher,
                    "developer": "Test",
                    "description": "A signed theme plugin.",
                    "homepage": null,
                    "license": null
                },
                "spec": {
                    "engine": "declarative",
                    "entry": "theme.json",
                    "language": null,
                    "capabilities": ["theme.register"],
                    "permissions": [],
                    "contributes": null
                },
                "payloads": {
                    "theme.json": theme_hash
                }
            });
            let payload = crate::signing::manifest_payload(&manifest_value).expect("manifest payload");
            let signature_b64 = sign(&BASE64.encode(&payload));
            yaml.push_str(&format!("signature: {signature_b64}\n"));
        }
        yaml
    }

    #[test]
    fn accepts_signed_plugin_with_trusted_key() {
        let root = tempdir().expect("temp root");

        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_b64 = BASE64.encode(verifying_key.as_bytes());
        let publisher = "trusted-publisher";

        let trusted_json =
            serde_json::json!({ publisher: pubkey_b64 }).to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json)
            .expect("write trusted keys");

        let package = root.path().join("signed-theme.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let sign_closure = |payload_b64: &str| -> String {
            let payload_bytes = BASE64.decode(payload_b64).expect("decode payload");
            let signature: Signature = signing_key.sign(&payload_bytes);
            BASE64.encode(signature.to_bytes())
        };

        let manifest_yaml = signed_theme_manifest(publisher, Some(&sign_closure));
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive
            .write_all(manifest_yaml.as_bytes())
            .expect("manifest bytes");
        let theme_json = valid_theme_json().to_string();
        archive
            .start_file("theme.json", options)
            .expect("theme entry");
        archive
            .write_all(theme_json.as_bytes())
            .expect("theme bytes");
        archive.finish().expect("finish archive");

        let installed =
            install_archive_into(root.path(), &package).expect("signed plugin should install");
        assert_eq!(installed.manifest.metadata.publisher, publisher);
        assert!(installed.manifest.signature.is_some());
        assert!(installed.theme.is_some());
    }

    #[test]
    fn rejects_signed_plugin_with_wrong_key() {
        let root = tempdir().expect("temp root");

        let signing_key_a = SigningKey::generate(&mut rand_core::OsRng);
        let verifying_key_b = SigningKey::generate(&mut rand_core::OsRng).verifying_key();
        let pubkey_b64_b = BASE64.encode(verifying_key_b.as_bytes());
        let publisher = "untrusted-publisher";

        let trusted_json =
            serde_json::json!({ publisher: pubkey_b64_b }).to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json)
            .expect("write trusted keys");

        let package = root.path().join("bad-sig.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let sign_closure_a = |payload_b64: &str| -> String {
            let payload_bytes = BASE64.decode(payload_b64).expect("decode payload");
            let signature: Signature = signing_key_a.sign(&payload_bytes);
            BASE64.encode(signature.to_bytes())
        };

        let manifest_yaml = signed_theme_manifest(publisher, Some(&sign_closure_a));
        let theme_json = valid_theme_json().to_string();
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive
            .write_all(manifest_yaml.as_bytes())
            .expect("manifest bytes");
        archive
            .start_file("theme.json", options)
            .expect("theme entry");
        archive
            .write_all(theme_json.as_bytes())
            .expect("theme bytes");
        archive.finish().expect("finish archive");

        let result = install_archive_into(root.path(), &package);
        assert!(result.is_err(), "signature mismatch must be rejected");
        assert!(
            result
                .unwrap_err()
                .contains("signature verification failed"),
            "error should mention signature verification"
        );
    }

    #[test]
    fn rejects_signed_plugin_without_trusted_key() {
        let root = tempdir().expect("temp root");

        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        let publisher = "unknown-publisher";

        let package = root.path().join("unknown-sig.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        let sign_closure = |payload_b64: &str| -> String {
            let payload_bytes = BASE64.decode(payload_b64).expect("decode payload");
            let signature: Signature = signing_key.sign(&payload_bytes);
            BASE64.encode(signature.to_bytes())
        };

        let manifest_yaml = signed_theme_manifest(publisher, Some(&sign_closure));
        let theme_json = valid_theme_json().to_string();
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive
            .write_all(manifest_yaml.as_bytes())
            .expect("manifest bytes");
        archive
            .start_file("theme.json", options)
            .expect("theme entry");
        archive
            .write_all(theme_json.as_bytes())
            .expect("theme bytes");
        archive.finish().expect("finish archive");

        let result = install_archive_into(root.path(), &package);
        assert!(result.is_err(), "unknown publisher must be rejected");
        assert!(
            result.unwrap_err().contains("No trusted public key found"),
            "error should mention missing trusted key"
        );
    }

    #[test]
    fn unsigned_plugin_still_installs() {
        let root = tempdir().expect("temp root");

        let manifest_yaml = signed_theme_manifest("unsigned-publisher", None::<&dyn Fn(&str) -> String>);
        let theme_json = valid_theme_json().to_string();

        let package = root.path().join("unsigned.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive
            .write_all(manifest_yaml.as_bytes())
            .expect("manifest bytes");
        archive
            .start_file("theme.json", options)
            .expect("theme entry");
        archive
            .write_all(theme_json.as_bytes())
            .expect("theme bytes");
        archive.finish().expect("finish archive");

        let installed =
            install_archive_into(root.path(), &package).expect("unsigned plugin should install");
        assert!(installed.manifest.signature.is_none());
        assert!(installed.theme.is_some());
    }

    #[test]
    fn tampered_manifest_with_valid_signature_rejected() {
        let root = tempdir().expect("temp root");

        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_b64 = BASE64.encode(verifying_key.as_bytes());
        let publisher = "honest-publisher";

        let trusted_json =
            serde_json::json!({ publisher: pubkey_b64 }).to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json)
            .expect("write trusted keys");

        let original_value = serde_json::json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": "honest-publisher.test-theme",
                "name": "Honest Theme",
                "version": "1.0.0",
                "publisher": publisher,
                "developer": "Honest Dev",
                "description": "Honest plugin.",
                "homepage": null,
                "license": null
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "language": null,
                "capabilities": ["theme.register"],
                "permissions": [],
                "contributes": null
            }
        });
        let payload = crate::signing::manifest_payload(&original_value).expect("manifest payload");
        let signature: Signature = signing_key.sign(&payload);
        let signature_b64 = BASE64.encode(signature.to_bytes());
        let theme_hash = theme_payload_hash();

        let tampered_yaml = format!(
            r#"apiVersion: researchcanvas.dev/v1alpha1
kind: ThemePlugin
metadata:
  id: {publisher}.evil-theme
  name: Evil Theme
  version: 9.9.9
  publisher: {publisher}
  developer: Evil Dev
  description: Tampered malicious plugin.
spec:
  engine: declarative
  entry: theme.json
  capabilities:
    - theme.register
  permissions: []
payloads:
  theme.json: {theme_hash}
signature: {signature_b64}
"#
        );

        let package = root.path().join("tampered.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive
            .write_all(tampered_yaml.as_bytes())
            .expect("tampered manifest bytes");
        let theme_json = valid_theme_json().to_string();
        archive
            .start_file("theme.json", options)
            .expect("theme entry");
        archive
            .write_all(theme_json.as_bytes())
            .expect("theme bytes");
        archive.finish().expect("finish archive");

        let result = install_archive_into(root.path(), &package);
        assert!(
            result.is_err(),
            "tampered manifest with valid signature must be rejected"
        );
        assert!(
            result
                .unwrap_err()
                .contains("signature verification failed"),
            "error should mention signature verification failure"
        );
    }

    fn theme_package_with_payloads(
        root: &Path,
        name: &str,
        payloads_yaml: &str,
        theme_bytes: &[u8],
        extra_file: Option<(&str, &[u8])>,
    ) -> PathBuf {
        let package = root.join(name);
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let manifest = format!(
            r#"apiVersion: researchcanvas.dev/v1alpha1
kind: ThemePlugin
metadata:
  id: myc.payload-theme
  name: Payload Theme
  version: 1.0.0
  publisher: Research Canvas
  developer: Tests
  description: Theme with declared payloads.
spec:
  engine: declarative
  entry: theme.json
  capabilities: [theme.register]
  permissions: []
{payloads_yaml}"#
        );
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive
            .write_all(manifest.as_bytes())
            .expect("manifest bytes");
        archive
            .start_file("theme.json", options)
            .expect("theme entry");
        archive.write_all(theme_bytes).expect("theme bytes");
        if let Some((path, bytes)) = extra_file {
            archive.start_file(path, options).expect("extra entry");
            archive.write_all(bytes).expect("extra bytes");
        }
        archive.finish().expect("finish archive");
        package
    }

    #[test]
    fn declared_payloads_are_hash_verified_at_install() {
        let root = tempdir().expect("temp root");
        let theme_json = valid_theme_json().to_string();
        let good = format!("payloads:\n  theme.json: {}\n", theme_payload_hash());
        let package = theme_package_with_payloads(
            root.path(),
            "good.myc",
            &good,
            theme_json.as_bytes(),
            None,
        );
        install_archive_into(root.path(), &package).expect("matching payloads install");

        // 载荷被替换 → 哈希不符 → 拒绝 / Swapped payload → hash mismatch → reject.
        let root2 = tempdir().expect("second root");
        let package = theme_package_with_payloads(
            root2.path(),
            "tampered.myc",
            &good,
            br#"{"id":"evil","name":"Evil","publisher":"x","colors":{}}"#,
            None,
        );
        let error = install_archive_into(root2.path(), &package)
            .expect_err("tampered payload must be rejected");
        assert!(error.contains("hash mismatch"), "unexpected error: {error}");

        // 未列出的额外文件 → 拒绝 / Unlisted extra file → reject.
        let root3 = tempdir().expect("third root");
        let package = theme_package_with_payloads(
            root3.path(),
            "extra.myc",
            &good,
            theme_json.as_bytes(),
            Some(("extra.txt", b"surprise")),
        );
        let error = install_archive_into(root3.path(), &package)
            .expect_err("unlisted payload must be rejected");
        assert!(error.contains("Unlisted payload"), "unexpected error: {error}");

        // 清单列出但包内缺失 → 拒绝 / Listed but missing → reject.
        let root4 = tempdir().expect("fourth root");
        let missing = format!(
            "payloads:\n  theme.json: {}\n  missing.txt: {}\n",
            theme_payload_hash(),
            "0".repeat(64)
        );
        let package = theme_package_with_payloads(
            root4.path(),
            "missing.myc",
            &missing,
            theme_json.as_bytes(),
            None,
        );
        let error = install_archive_into(root4.path(), &package)
            .expect_err("missing declared payload must be rejected");
        assert!(
            error.contains("missing from package"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn signed_manifest_without_payloads_is_rejected() {
        let root = tempdir().expect("temp root");
        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        let verifying_key = signing_key.verifying_key();
        let publisher = "payloadless-publisher";
        let trusted_json = serde_json::json!({ publisher: BASE64.encode(verifying_key.as_bytes()) })
            .to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json)
            .expect("write trusted keys");

        // 手工构造:有签名但无 payloads(攻击者换掉 wasm 后老方案仍过签)。
        // Hand-built: signature present, payloads absent — the old gap where a
        // swapped plugin.wasm still verified.
        let manifest_value = serde_json::json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": format!("{publisher}.test-theme"),
                "name": "Payloadless Theme",
                "version": "1.0.0",
                "publisher": publisher,
                "developer": "Test",
                "description": "Signed but payloadless.",
                "homepage": null,
                "license": null
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "language": null,
                "capabilities": ["theme.register"],
                "permissions": [],
                "contributes": null
            }
        });
        let payload = crate::signing::manifest_payload(&manifest_value).expect("manifest payload");
        let signature: Signature = signing_key.sign(&payload);
        let yaml = format!(
            r#"apiVersion: researchcanvas.dev/v1alpha1
kind: ThemePlugin
metadata:
  id: {publisher}.test-theme
  name: Payloadless Theme
  version: 1.0.0
  publisher: {publisher}
  developer: Test
  description: Signed but payloadless.
spec:
  engine: declarative
  entry: theme.json
  capabilities: [theme.register]
  permissions: []
signature: {}
"#,
            BASE64.encode(signature.to_bytes())
        );

        let package = root.path().join("payloadless.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.yml", options)
            .expect("manifest entry");
        archive.write_all(yaml.as_bytes()).expect("manifest bytes");
        archive
            .start_file("theme.json", options)
            .expect("theme entry");
        archive
            .write_all(valid_theme_json().to_string().as_bytes())
            .expect("theme bytes");
        archive.finish().expect("finish archive");

        let error = install_archive_into(root.path(), &package)
            .expect_err("signed package without payloads must be rejected");
        assert!(error.contains("payloads"), "unexpected error: {error}");
    }

    #[test]
    fn resolve_package_path_restricts_to_packages_directory() {
        let base = tempdir().expect("temp base").path().to_path_buf();
        let packages = base.join("packages");
        fs::create_dir_all(&packages).expect("create packages");
        let inside = packages.join("inside.myc");
        fs::write(&inside, b"dummy").expect("write inside");

        assert!(
            resolve_package_path(&base, &inside).is_ok(),
            "path inside packages must be allowed"
        );

        let outside = base.join("outside.myc");
        fs::write(&outside, b"dummy").expect("write outside");
        let result = resolve_package_path(&base, &outside);
        assert!(
            result.is_err(),
            "path outside packages must be rejected: {result:?}"
        );
        assert!(
            result.unwrap_err().contains("packages directory"),
            "error should mention packages directory"
        );

        let escaped = packages.join("..").join("escaped.myc");
        let result = resolve_package_path(&base, &escaped);
        assert!(
            result.is_err(),
            "path escaping packages via .. must be rejected: {result:?}"
        );
    }
}
