//! `.myc` 插件的桌面端安装与执行边界 / Desktop install and execution boundary for `.myc` plugins.
//!
//! 声明式视觉包只读取 JSON；分析包只执行经校验的 WebAssembly，并且默认没有主机能力。
//! Declarative visual packages only expose JSON; analysis packages execute verified WebAssembly
//! with no host capabilities by default. All archives are bounded and staged before visibility.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use tauri::AppHandle;
#[cfg(not(debug_assertions))]
use tauri::Manager;
use zip::ZipArchive;

use crate::llm_plugin::{self, ProviderDescriptor};

const MYC_API_VERSION: &str = "researchcanvas.dev/v1alpha1";
/// v2 flat manifests (plugin_manifest_v2) migrate into the internal v1 shape
/// with this synthesized api version; validation accepts both.
const MYC_API_VERSION_V2: &str = "researchcanvas.dev/v2";
const PLUGIN_CALL_API_VERSION: &str = "researchcanvas.dev/plugin-call/v1alpha1";
const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_UNPACKED_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;
const MAX_ICON_THEME_ASSET_BYTES: u64 = 4 * 1024 * 1024;
const MAX_UI_IR_SOURCE_BYTES: u64 = 512 * 1024;
const MAX_UI_IR_ARTIFACT_BYTES: u64 = 256 * 1024;
const REMOVED_PLUGINS_FILE: &str = "removed-plugins.json";
const PLUGIN_LOADING_CONFIG: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../config/plugin-loading.json"
));

fn is_false(value: &bool) -> bool {
    !*value
}

fn manifest_cache() -> &'static Mutex<HashMap<PathBuf, InstalledMycPlugin>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, InstalledMycPlugin>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn invalidate_manifest_cache(directory: &Path) {
    if let Ok(mut cache) = manifest_cache().lock() {
        cache.remove(directory);
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub developer: String,
    /// Optional stable developer identity. Older manifests only have the
    /// human-readable `developer` field and remain valid.
    #[serde(
        default,
        alias = "developerId",
        alias = "developerUUID",
        skip_serializing_if = "Option::is_none"
    )]
    pub developer_uuid: Option<String>,
    pub description: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    /// 官方维护标记;仅在 publisher == "ResearchCanvas" 时被校验接受。
    /// Official-maintenance marker; validation honors it only for the
    /// built-in ResearchCanvas publisher identity.
    #[serde(default, skip_serializing_if = "is_false")]
    pub official: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<PluginUpdateInfo>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginUpdateInfo {
    pub latest_version: Option<String>,
    pub url: Option<String>,
    pub release_notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSettingDefinition {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub setting_type: String,
    pub default: Option<serde_json::Value>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub options: Option<Vec<PluginSettingOption>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub secret: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "source")]
pub enum PluginApiKeySource {
    #[serde(rename = "host-secret")]
    HostSecret {
        #[serde(rename = "settingId")]
        setting_id: String,
    },
    #[serde(rename = "environment", alias = "Environment")]
    Environment {
        name: String,
        #[serde(
            rename = "fallbackSettingId",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        fallback_setting_id: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConnectionTestAction {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_key: Option<String>,
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<PluginConnectionTestActionInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PluginConnectionTestActionInput {
    #[serde(rename = "text")]
    Text {
        #[serde(rename = "fileUpload", alias = "file_upload")]
        file_upload: String,
    },
    #[serde(rename = "bundled-pdf")]
    BundledPdf {
        fixture: String,
        #[serde(rename = "fileUpload", alias = "file_upload")]
        file_upload: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConnectionDefinition {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder_key: Option<String>,
    pub url_setting_id: String,
    pub format_setting_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_setting_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_source_setting_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_env_var_setting_id: Option<String>,
    pub api_key: PluginApiKeySource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_actions: Option<Vec<PluginConnectionTestAction>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_action: Option<PluginConnectionTestAction>,
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
pub struct PluginUiContribution {
    pub id: String,
    pub slot_id: String,
    #[serde(rename = "export")]
    pub export_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginContributions {
    pub context_menus: Option<Vec<PluginContextMenuContribution>>,
    pub locales: Option<Vec<PluginLocaleContribution>>,
    pub commands: Option<Vec<PluginCommandContribution>>,
    /// Trusted dynamic frontend placements. These are physical Host slots,
    /// not nested plugin UI controls such as file pickers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<Vec<PluginUiContribution>>,
    /// 声明式 Vue UI IR 贡献(v2 平面清单的 contributes.uiIr 透传)。
    /// Declarative Vue UI IR contributions (v2 `contributes.uiIr` passthrough).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui_ir: Option<serde_json::Value>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Vec<PluginSettingDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connections: Option<Vec<PluginConnectionDefinition>>,
}

fn default_frontend_api_version() -> String {
    "1".to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginFrontendDescriptor {
    pub mode: String,
    pub entry: String,
    pub framework: String,
    #[serde(default = "default_frontend_api_version")]
    pub api_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginNetworkDescriptor {
    pub mode: String,
    #[serde(default)]
    pub declared_domains: Vec<String>,
}

fn default_worker_id() -> String {
    "default".to_string()
}

fn default_true() -> bool {
    true
}

/// Host-mediated external worker descriptor. This is an execution contract,
/// not a permission grant: the installed manifest remains the source of the
/// executable path and operation allowlist.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginWorkerDescriptor {
    #[serde(default = "default_worker_id")]
    pub id: String,
    pub language: String,
    pub entrypoint: String,
    pub transport: String,
    #[serde(default = "default_true")]
    pub host_mediated: bool,
    #[serde(default)]
    pub operations: Vec<String>,
    #[serde(default)]
    pub host_operations: Vec<String>,
    /// Direct provider network declarations. These are validated policy
    /// metadata and credential-routing inputs; OS egress enforcement is not
    /// implemented by the process launcher yet.
    #[serde(default)]
    pub provider_egress: Vec<PluginProviderEgress>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginProviderEgress {
    pub provider_id: String,
    pub connection_id: String,
    pub domains: Vec<String>,
    pub purpose: String,
    pub secret_env: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentManifestDescriptorV1 {
    schema_version: u64,
    mode: String,
    #[serde(default)]
    agent_type: Option<String>,
    review_gated: bool,
    #[serde(default)]
    maintainer: Option<String>,
    #[serde(default)]
    extraction_schema: Option<String>,
    #[serde(default)]
    compiled_schema: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    security_boundary: Option<serde_json::Value>,
    #[serde(default)]
    model_configuration: Option<serde_json::Value>,
    #[serde(default)]
    pipeline: Option<serde_json::Value>,
    #[serde(default)]
    worker: Option<PluginWorkerDescriptor>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentManifestDescriptorV2 {
    schema_version: u64,
    mode: String,
    plugin_id: String,
    plugin_version: String,
    #[serde(default)]
    agent_type: Option<String>,
    review_gated: bool,
    frontend: AgentFrontendContract,
    worker: AgentWorkerContract,
    #[serde(default)]
    frontend_context_required: Option<serde_json::Value>,
    #[serde(default)]
    model_configuration: Option<serde_json::Value>,
    #[serde(default)]
    pipeline: Option<serde_json::Value>,
    #[serde(default)]
    security_boundary: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentFrontendContract {
    mode: String,
    entry: String,
    framework: String,
    #[serde(default = "default_frontend_api_version")]
    api_version: String,
    exports: Vec<String>,
    ui_contributions: Vec<AgentUiContributionContract>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentUiContributionContract {
    id: String,
    slot_id: String,
    #[serde(rename = "export")]
    export_name: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AgentWorkerContract {
    id: String,
    language: String,
    entrypoint: String,
    transport: String,
    opened_by: String,
    operations: Vec<String>,
    host_operations: Vec<String>,
    forbidden_host_operations: Vec<String>,
    credentials: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPrivateI18nDefinition {
    pub default_locale: String,
    pub locales: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginManifest {
    pub api_version: String,
    pub kind: String,
    pub metadata: MycPluginMetadata,
    pub spec: MycPluginSpec,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontend: Option<PluginFrontendDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<PluginWorkerDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workers: Option<Vec<PluginWorkerDescriptor>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<PluginNetworkDescriptor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provides: Option<PluginProvides>,
    /// 包内每个载荷文件(plugin.json 除外)的 sha256:相对路径 → 64 位小写十六进制。
    /// 签名覆盖清单 JSON,清单携带 payloads 后签名即覆盖全部载荷。
    /// sha256 of every payload file in the package (except plugin.json itself):
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
pub struct PluginProvides {
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub entries: Vec<String>,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IconThemeManifest {
    schema_version: u64,
    id: String,
    name: String,
    publisher: String,
    version: String,
    description: Option<String>,
    source: String,
    file_extensions: BTreeMap<String, String>,
    file_names: BTreeMap<String, String>,
    folder_names: BTreeMap<String, String>,
    folder_names_expanded: BTreeMap<String, String>,
    icon_definitions: BTreeMap<String, serde_json::Value>,
    fonts: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledMycPlugin {
    pub(crate) manifest: MycPluginManifest,
    pub(crate) install_path: String,
    pub(crate) theme: Option<ThemeManifest>,
    pub(crate) icon_theme: Option<IconThemeManifest>,
    pub(crate) edge_style: Option<serde_json::Value>,
    pub(crate) runtime: Option<MycPluginRuntime>,
    pub(crate) frontend: Option<InstalledPluginFrontend>,
    pub(crate) locales: Option<Vec<InstalledPluginLocale>>,
    pub(crate) private_i18n: Option<InstalledPluginPrivateI18n>,
    pub(crate) workspace: Option<serde_json::Value>,
    pub provider: Option<ProviderDescriptor>,
    pub(crate) agent: Option<serde_json::Value>,
    pub(crate) ui_ir_contributions: Option<Vec<serde_json::Value>>,
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
pub struct InstalledPluginPrivateI18n {
    namespace: String,
    default_locale: String,
    locales: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MycPluginRuntime {
    engine: String,
    language: String,
    entry_sha256: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginFrontend {
    mode: String,
    framework: String,
    api_version: String,
    entry: String,
    installed_entry_path: String,
    entry_sha256: String,
}

pub(crate) fn plugin_base(_app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    {
        let manifest_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let repository = manifest_directory
            .parent()
            .ok_or_else(|| "Could not resolve repository root".to_string())?;
        return Ok(repository.join(".plugin-runtime/dev"));
    }

    #[cfg(not(debug_assertions))]
    {
        _app.path()
            .app_data_dir()
            .map(|path| path.join("plugins"))
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn resolve_plugin_frontend_source(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    requested_entry: &str,
    requested_framework: &str,
    requested_api_version: &str,
) -> Result<serde_json::Value, String> {
    let (_directory, installed) =
        read_installed_plugin_by_identity(app, plugin_id, plugin_version)?;
    let frontend = installed
        .frontend
        .as_ref()
        .ok_or_else(|| format!("Plugin {plugin_id}@{plugin_version} has no trusted frontend"))?;
    if frontend.mode != "trusted-module"
        || frontend.entry != requested_entry
        || frontend.framework != requested_framework
        || frontend.api_version != requested_api_version
    {
        return Err(
            "Requested frontend descriptor does not match the installed manifest".to_string(),
        );
    }
    let bytes = fs::read(&frontend.installed_entry_path)
        .map_err(|error| format!("Could not read installed frontend entry: {error}"))?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("Installed frontend module exceeds the 8 MB source limit".to_string());
    }
    let digest = format!("{:x}", Sha256::digest(&bytes));
    if digest != frontend.entry_sha256 {
        return Err(
            "Installed frontend module digest no longer matches installation metadata".to_string(),
        );
    }
    let source = String::from_utf8(bytes)
        .map_err(|_| "Installed frontend module must be UTF-8 JavaScript".to_string())?;
    Ok(serde_json::json!({
        "kind": "source",
        "source": source,
        "contentType": "text/javascript",
        "revision": frontend.entry_sha256,
    }))
}

pub(crate) fn trusted_plugin_frontend_settings(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<serde_json::Value, String> {
    let (_directory, installed) =
        read_installed_plugin_by_identity(app, plugin_id, plugin_version)?;
    if installed
        .frontend
        .as_ref()
        .is_none_or(|frontend| frontend.mode != "trusted-module")
    {
        return Err(
            "Only an installed trusted-module frontend may read plugin-owned settings".to_string(),
        );
    }
    let snapshot =
        crate::plugin_settings::get_snapshot(app, &installed.manifest, plugin_id, plugin_version)?;
    let mut values = snapshot.effective_values.clone();
    for definition in &snapshot.definitions {
        if !definition.secret {
            continue;
        }
        if let Some(secret) =
            crate::plugin_settings::resolve_host_secret(plugin_id, plugin_version, &definition.id)?
        {
            values.insert(definition.id.clone(), serde_json::Value::String(secret));
        }
    }
    Ok(serde_json::json!({
        "pluginId": plugin_id,
        "pluginVersion": plugin_version,
        "values": values,
    }))
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
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Invalid removal registry: {error}"))?;
    match value {
        serde_json::Value::Array(items) => {
            let mut removed = HashSet::with_capacity(items.len());
            for item in items {
                match item.as_str() {
                    Some(entry) => {
                        removed.insert(entry.to_string());
                    }
                    None => {
                        return Err("Invalid removal registry: entries must be strings".to_string())
                    }
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
    invalidate_manifest_cache(&directory);
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

fn validate_plugin_settings(manifest: &MycPluginManifest) -> Result<(), String> {
    crate::plugin_settings::validate_definitions(
        manifest.spec.settings.as_deref().unwrap_or_default(),
    )?;
    crate::plugin_settings::validate_connections(manifest)
}

fn validate_developer_uuid(value: &str) -> Result<(), String> {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || ![8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| ![8, 13, 18, 23].contains(&index) && !byte.is_ascii_hexdigit())
    {
        return Err("Developer UUID must be a canonical UUID".to_string());
    }
    Ok(())
}

fn validate_plugin_update(update: Option<&PluginUpdateInfo>) -> Result<(), String> {
    let Some(update) = update else {
        return Ok(());
    };
    if let Some(version) = update.latest_version.as_deref() {
        validate_slug(version, "latest plugin version")?;
    }
    if update
        .url
        .as_deref()
        .is_some_and(|url| !(url.starts_with("https://") || url.starts_with("http://")))
    {
        return Err("Plugin update URLs must use http:// or https://".to_string());
    }
    if update
        .release_notes
        .as_ref()
        .is_some_and(|notes| notes.chars().count() > 2000)
    {
        return Err("Plugin update release notes must be at most 2000 characters".to_string());
    }
    Ok(())
}

fn validate_relative_plugin_path(value: &str, label: &str) -> Result<(), String> {
    let path = Path::new(value);
    let unsafe_segment = value
        .split('/')
        .any(|segment| segment.is_empty() || matches!(segment, "." | ".."));
    if value.is_empty()
        || value.len() > 240
        || path.is_absolute()
        || value.contains('\\')
        || unsafe_segment
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!("{label} must be a relative path without traversal"));
    }
    Ok(())
}

fn validate_wire_identifier(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 160
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_uppercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'.' | b'_' | b'-' | b':')
        })
    {
        return Err(format!("Invalid {label}: {value}"));
    }
    Ok(())
}

fn validate_export_name(value: &str) -> Result<(), String> {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return Err("UI contribution export must not be empty".to_string());
    };
    if !(first == '_' || first == '$' || first.is_ascii_alphabetic()) {
        return Err(format!("Invalid UI contribution export: {value}"));
    }
    if value.len() > 96
        || chars.any(|character| {
            !(character == '_' || character == '$' || character.is_ascii_alphanumeric())
        })
    {
        return Err(format!("Invalid UI contribution export: {value}"));
    }
    Ok(())
}

fn validate_frontend_descriptor(frontend: &PluginFrontendDescriptor) -> Result<(), String> {
    if frontend.mode != "trusted-module" {
        return Err("Plugin frontend mode must be trusted-module".to_string());
    }
    if frontend.framework != "vue3" {
        return Err("Plugin frontend framework must be vue3".to_string());
    }
    if frontend.api_version != "1" {
        return Err("Plugin frontend apiVersion must be 1".to_string());
    }
    validate_relative_plugin_path(&frontend.entry, "Plugin frontend entry")?;
    if !frontend.entry.ends_with(".mjs") {
        return Err("Plugin trusted-module frontend entry must be an .mjs file".to_string());
    }
    Ok(())
}

fn validate_network_descriptor(network: &PluginNetworkDescriptor) -> Result<(), String> {
    if network.mode != "direct" {
        return Err("Plugin network mode must be direct".to_string());
    }
    if network.declared_domains.len() > 32 {
        return Err("Plugin network may declare at most 32 domains".to_string());
    }
    let mut domains = HashSet::new();
    for domain in &network.declared_domains {
        validate_declared_domain(domain, "Plugin network declared domain")?;
        if !domains.insert(domain) {
            return Err("Plugin network declared domains must be unique".to_string());
        }
    }
    Ok(())
}

fn validate_declared_domain(domain: &str, label: &str) -> Result<(), String> {
    if domain.is_empty()
        || domain.len() > 253
        || domain != domain.to_ascii_lowercase()
        || domain.starts_with('.')
        || domain.ends_with('.')
        || domain.contains(['/', '\\', ':'])
        || domain.split('.').count() < 2
        || !domain
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
        || domain.split('.').any(|part| {
            part.is_empty() || part.len() > 63 || part.starts_with('-') || part.ends_with('-')
        })
    {
        return Err(format!("{label} must be an exact lowercase DNS name"));
    }
    Ok(())
}

fn validate_ui_contributions(items: &[PluginUiContribution]) -> Result<(), String> {
    if items.is_empty() || items.len() > 64 {
        return Err("Plugin contributes.ui must contain 1 to 64 contributions".to_string());
    }
    let mut ids = HashSet::new();
    for item in items {
        validate_wire_identifier(&item.id, "UI contribution id")?;
        validate_wire_identifier(&item.slot_id, "UI slot id")?;
        validate_export_name(&item.export_name)?;
        if !ids.insert(item.id.as_str()) {
            return Err(format!("Duplicate UI contribution id: {}", item.id));
        }
        if item.when.as_ref().is_some_and(|when| {
            when.is_empty() || when.len() > 240 || when.chars().any(char::is_control)
        }) {
            return Err(format!("Invalid UI contribution when clause: {}", item.id));
        }
    }
    Ok(())
}

fn manifest_worker_descriptors(
    manifest: &MycPluginManifest,
) -> impl Iterator<Item = &PluginWorkerDescriptor> {
    manifest
        .worker
        .iter()
        .chain(manifest.workers.as_deref().unwrap_or_default().iter())
}

fn validate_locale_tag(locale: &str) -> Result<(), String> {
    if locale.is_empty()
        || locale.len() > 35
        || !locale
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(format!("Invalid locale tag: {locale}"));
    }
    Ok(())
}

fn validate_private_i18n(private_i18n: Option<&PluginPrivateI18nDefinition>) -> Result<(), String> {
    let Some(private_i18n) = private_i18n else {
        return Ok(());
    };
    if private_i18n.locales.is_empty() || private_i18n.locales.len() > 16 {
        return Err("Private i18n must declare 1 to 16 locales".to_string());
    }
    validate_locale_tag(&private_i18n.default_locale)?;
    if !private_i18n
        .locales
        .contains_key(&private_i18n.default_locale)
    {
        return Err("Private i18n defaultLocale must be declared in locales".to_string());
    }
    for (locale, path) in &private_i18n.locales {
        validate_locale_tag(locale)?;
        let expected = format!("locales/{locale}.json");
        if path != &expected {
            return Err(format!(
                "Private i18n bundle for {locale} must use {expected}"
            ));
        }
    }
    Ok(())
}

fn validate_connection_test_action(action: &PluginConnectionTestAction) -> Result<(), String> {
    validate_slug(&action.id, "connection test action id")?;
    if action.label.trim().is_empty() || action.label.chars().count() > 64 {
        return Err(format!(
            "Connection test action label is invalid: {}",
            action.id
        ));
    }
    if action
        .description
        .as_ref()
        .is_some_and(|description| description.chars().count() > 180)
    {
        return Err(format!(
            "Connection test action description is too long: {}",
            action.id
        ));
    }
    if let Some(kind) = action.kind.as_deref() {
        if !matches!(kind, "connection" | "pdf-extraction") {
            return Err(format!("Unsupported connection test action kind: {kind}"));
        }
    }
    if let Some(input) = action.input.as_ref() {
        match input {
            PluginConnectionTestActionInput::Text { file_upload } => {
                if file_upload != "never" {
                    return Err("Text connection tests must never upload files".to_string());
                }
            }
            PluginConnectionTestActionInput::BundledPdf {
                fixture,
                file_upload,
            } => {
                if fixture != "host-minimal-pdf-v1" || file_upload != "may-upload" {
                    return Err(
                        "Bundled PDF tests require host-minimal-pdf-v1 and may-upload".to_string(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn validate_connection_test_actions(connection: &PluginConnectionDefinition) -> Result<(), String> {
    if let Some(action) = connection.test_action.as_ref() {
        validate_connection_test_action(action)?;
    }
    if let Some(actions) = connection.test_actions.as_ref() {
        if actions.is_empty() || actions.len() > 8 {
            return Err(format!(
                "Connection {} must declare 1 to 8 test actions",
                connection.id
            ));
        }
        let mut ids = HashSet::new();
        for action in actions {
            validate_connection_test_action(action)?;
            if !ids.insert(action.id.as_str()) {
                return Err(format!("Duplicate connection test action: {}", action.id));
            }
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &MycPluginManifest) -> Result<(), String> {
    if manifest.api_version != MYC_API_VERSION && manifest.api_version != MYC_API_VERSION_V2 {
        return Err(format!(
            "Unsupported plugin API version: {}",
            manifest.api_version
        ));
    }
    validate_slug(&manifest.metadata.id, "plugin id")?;
    validate_slug(&manifest.metadata.version, "plugin version")?;
    if manifest.metadata.official && manifest.metadata.publisher != "ResearchCanvas" {
        return Err(
            "The official flag is reserved for the ResearchCanvas publisher identity".to_string(),
        );
    }
    if let Some(developer_uuid) = manifest.metadata.developer_uuid.as_deref() {
        validate_developer_uuid(developer_uuid)?;
    }
    validate_plugin_update(manifest.metadata.update.as_ref())?;
    validate_plugin_settings(manifest)?;
    if let Some(frontend) = manifest.frontend.as_ref() {
        validate_frontend_descriptor(frontend)?;
    }
    if let Some(network) = manifest.network.as_ref() {
        validate_network_descriptor(network)?;
    }
    if let Some(items) = manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.ui.as_ref())
    {
        validate_ui_contributions(items)?;
    }
    let mut worker_ids = HashSet::new();
    for worker in manifest_worker_descriptors(manifest) {
        validate_worker_descriptor(worker)?;
        if !worker_ids.insert(worker.id.as_str()) {
            return Err(format!("Duplicate worker id: {}", worker.id));
        }
    }
    if let Some(connections) = manifest.spec.connections.as_ref() {
        for connection in connections {
            validate_connection_test_actions(connection)?;
        }
    }
    let connections = manifest.spec.connections.as_deref().unwrap_or_default();
    for worker in manifest_worker_descriptors(manifest) {
        for egress in &worker.provider_egress {
            if !connections
                .iter()
                .any(|connection| connection.id == egress.connection_id)
            {
                return Err(format!(
                    "Worker provider egress references an unknown connection: {}",
                    egress.connection_id
                ));
            }
        }
    }
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
        "IconThemePlugin" => {
            if manifest.spec.engine != "declarative" {
                return Err("IconThemePlugin engine must be declarative".to_string());
            }
            if manifest.spec.entry != "icon-theme.json" {
                return Err("IconThemePlugin entry must be icon-theme.json".to_string());
            }
            if manifest.spec.language.is_some()
                || !manifest
                    .spec
                    .capabilities
                    .iter()
                    .any(|capability| capability == "icon-theme.register")
            {
                return Err(
                    "IconThemePlugin must declare icon-theme.register and no guest language"
                        .to_string(),
                );
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
        "ExtensionPlugin" => {
            if manifest.frontend.is_none() && manifest.workers.as_ref().is_none_or(Vec::is_empty) {
                return Err(
                    "ExtensionPlugin must declare a trusted frontend or at least one worker"
                        .to_string(),
                );
            }
            if manifest
                .spec
                .contributes
                .as_ref()
                .and_then(|contributions| contributions.ui.as_ref())
                .is_some()
                && manifest.frontend.is_none()
            {
                return Err("contributes.ui requires a trusted-module frontend".to_string());
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
            const AGENT_CAPABILITIES: [&str; 5] = [
                "agent.pdf.read",
                "agent.graph.patch.propose",
                "agent.review.request",
                "agent.job.cancel",
                "graph.patch.propose",
            ];
            // 官方 host-mediated agent 在清单中声明其 host-bus 数据契约;这些
            // 能力由宿主以 native principal 代持执行,绝不授予 agent 本体。
            // Official host-mediated agents declare their host-bus data
            // contract in the manifest; the host exercises these capabilities
            // as the native principal — they are never granted to the agent.
            const HOST_BUS_CONTRACT_CAPABILITIES: [&str; 13] = [
                "graph.ir",
                "graph.patch.get",
                "graph.patch.review",
                "graph.storage.read",
                "graph.storage.write",
                "host-bus.event",
                "audit.read",
                "blob.manage",
                "plugin.files.pick",
                "plugin.worker.open",
                "plugin.worker.call",
                "plugin.worker.cancel",
                "plugin.worker.close",
            ];
            let has_agent_capability = manifest
                .spec
                .capabilities
                .iter()
                .any(|capability| AGENT_CAPABILITIES.contains(&capability.as_str()));
            let all_declared = manifest.spec.capabilities.iter().all(|capability| {
                AGENT_CAPABILITIES.contains(&capability.as_str())
                    || HOST_BUS_CONTRACT_CAPABILITIES.contains(&capability.as_str())
            });
            if !has_agent_capability || !all_declared {
                return Err(
                    "AgentPlugin capabilities must include at least one agent capability (agent.pdf.read, agent.graph.patch.propose, graph.patch.propose, agent.review.request, or agent.job.cancel), plus optional host-bus contract capabilities"
                        .to_string(),
                );
            }
        }
        _ => {
            return Err(
                "Installer accepts ThemePlugin, IconThemePlugin, EdgeStylePlugin, AnalysisPlugin, LocalePlugin, WorkspacePlugin, ProviderPlugin, AgentPlugin, and ExtensionPlugin packages"
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
            if path == "plugin.json"
                || relative.is_absolute()
                || relative
                    .components()
                    .any(|component| !matches!(component, std::path::Component::Normal(_)))
                || path.contains('\\')
            {
                return Err(format!("Invalid payload path: {path}"));
            }
            if digest.len() != 64
                || !digest
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
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

fn validate_worker_descriptor(worker: &PluginWorkerDescriptor) -> Result<(), String> {
    validate_wire_identifier(&worker.id, "worker id")?;
    validate_wire_identifier(&worker.language, "worker language")?;
    if worker.transport != "stdio-framed-json-v1" {
        return Err(format!(
            "Unsupported worker transport: {}",
            worker.transport
        ));
    }
    if !worker.host_mediated {
        return Err("External workers must be hostMediated".to_string());
    }
    validate_relative_plugin_path(&worker.entrypoint, "Worker entrypoint")?;
    if worker.operations.is_empty() || worker.operations.len() > 128 {
        return Err("Worker must declare 1 to 128 operations".to_string());
    }
    if worker.host_operations.len() > 64 {
        return Err("Worker may declare at most 64 reverse Host Bus operations".to_string());
    }
    let mut unique = HashSet::new();
    for operation in &worker.operations {
        if !unique.insert(operation) {
            return Err(format!(
                "Invalid or duplicate worker operation: {operation}"
            ));
        }
        validate_worker_operation_name(operation, "worker operation")?;
    }
    let mut host_operations = HashSet::new();
    for operation in &worker.host_operations {
        if !host_operations.insert(operation) {
            return Err(format!(
                "Invalid or duplicate worker Host Bus operation: {operation}"
            ));
        }
        validate_worker_operation_name(operation, "worker Host Bus operation")?;
    }
    if worker.provider_egress.len() > 4 {
        return Err("worker can declare at most four direct provider egress profiles".to_string());
    }
    let mut providers = HashSet::new();
    for egress in &worker.provider_egress {
        validate_slug(&egress.provider_id, "worker provider id")?;
        validate_slug(&egress.connection_id, "worker provider connection id")?;
        if !providers.insert(&egress.provider_id) {
            return Err("worker provider egress ids must be unique".to_string());
        }
        if egress.domains.is_empty() || egress.domains.len() > 8 {
            return Err("worker provider egress must declare one to eight domains".to_string());
        }
        for domain in &egress.domains {
            validate_declared_domain(domain, "worker provider egress domain")?;
        }
        if egress.purpose.trim().is_empty() || egress.purpose.len() > 240 {
            return Err("worker provider egress purpose must be bounded".to_string());
        }
        if egress.secret_env.len() > 96
            || !egress.secret_env.starts_with("ANYWAY_PLUGIN_SECRET_")
            || !egress.secret_env.chars().all(|character| {
                character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
            })
        {
            return Err(
                "worker provider secret env must use the ANYWAY_PLUGIN_SECRET_ prefix".to_string(),
            );
        }
    }
    Ok(())
}

fn validate_worker_operation_name(operation: &str, label: &str) -> Result<(), String> {
    if operation.is_empty()
        || operation.len() > 160
        || !operation.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
    {
        return Err(format!("Invalid {label}: {operation}"));
    }
    Ok(())
}

fn validate_worker_entrypoint(
    directory: &Path,
    worker: &PluginWorkerDescriptor,
) -> Result<PathBuf, String> {
    let root = fs::canonicalize(directory)
        .map_err(|error| format!("Could not canonicalize plugin root: {error}"))?;
    let entry = directory.join(&worker.entrypoint);
    let canonical = fs::canonicalize(&entry).map_err(|error| {
        format!(
            "Could not resolve worker entrypoint {}: {error}",
            worker.entrypoint
        )
    })?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return Err("Worker entrypoint escapes the installed plugin root".to_string());
    }
    Ok(canonical)
}

fn validate_agent_manifest_descriptor(
    descriptor: &serde_json::Value,
    manifest: &mut MycPluginManifest,
    directory: &Path,
) -> Result<(), String> {
    match descriptor
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
    {
        Some(1) => validate_agent_manifest_descriptor_v1(descriptor, manifest, directory),
        Some(2) => validate_agent_manifest_descriptor_v2(descriptor, manifest, directory),
        _ => {
            Err("Invalid agent-manifest.json descriptor: requires schemaVersion 1 or 2".to_string())
        }
    }
}

fn validate_agent_manifest_descriptor_v1(
    descriptor: &serde_json::Value,
    manifest: &mut MycPluginManifest,
    directory: &Path,
) -> Result<(), String> {
    let descriptor: AgentManifestDescriptorV1 = serde_json::from_value(descriptor.clone())
        .map_err(|error| format!("Invalid agent-manifest.json descriptor: {error}"))?;
    if descriptor.schema_version != 1
        || descriptor.mode != "agent"
        || !descriptor.review_gated
        || descriptor
            .agent_type
            .as_deref()
            .is_some_and(|agent_type| agent_type.trim().is_empty() || agent_type.len() > 80)
    {
        return Err(
            "Invalid agent-manifest.json descriptor: requires schemaVersion 1, mode \"agent\", and reviewGated true"
                .to_string(),
        );
    }
    for (label, value) in [
        ("maintainer", descriptor.maintainer.as_deref()),
        ("extractionSchema", descriptor.extraction_schema.as_deref()),
        ("compiledSchema", descriptor.compiled_schema.as_deref()),
    ] {
        if value.is_some_and(|value| {
            value.trim().is_empty() || value.len() > 160 || value.chars().any(char::is_control)
        }) {
            return Err(format!(
                "Invalid agent-manifest.json v1 {label}: value must be bounded text"
            ));
        }
    }
    if descriptor.capabilities.len() > 128 {
        return Err(
            "Invalid agent-manifest.json v1 capabilities: at most 128 entries are allowed"
                .to_string(),
        );
    }
    let mut capabilities = HashSet::new();
    for capability in &descriptor.capabilities {
        if !capabilities.insert(capability.as_str())
            || !manifest
                .spec
                .capabilities
                .iter()
                .any(|declared| declared == capability)
        {
            return Err(format!(
                "Invalid agent-manifest.json v1 capability: {capability}"
            ));
        }
    }
    for (label, section) in [
        ("securityBoundary", descriptor.security_boundary.as_ref()),
        (
            "modelConfiguration",
            descriptor.model_configuration.as_ref(),
        ),
        ("pipeline", descriptor.pipeline.as_ref()),
    ] {
        if section.is_some_and(|section| !section.is_object()) {
            return Err(format!(
                "Invalid agent-manifest.json v1 {label}: expected an object"
            ));
        }
    }
    if manifest.worker.is_none() {
        manifest.worker = descriptor.worker;
    }
    if let Some(worker) = manifest.worker.as_ref() {
        validate_worker_descriptor(worker)?;
        validate_worker_entrypoint(directory, worker)?;
    }
    Ok(())
}

fn validate_agent_manifest_descriptor_v2(
    descriptor: &serde_json::Value,
    manifest: &MycPluginManifest,
    directory: &Path,
) -> Result<(), String> {
    let descriptor: AgentManifestDescriptorV2 = serde_json::from_value(descriptor.clone())
        .map_err(|error| format!("Invalid agent-manifest.json v2 descriptor: {error}"))?;
    if descriptor.schema_version != 2
        || descriptor.mode != "trusted-plugin"
        || !descriptor.review_gated
    {
        return Err(
            "Invalid agent-manifest.json v2 descriptor: requires schemaVersion 2, mode \"trusted-plugin\", and reviewGated true"
                .to_string(),
        );
    }
    if descriptor.plugin_id != manifest.metadata.id
        || descriptor.plugin_version != manifest.metadata.version
    {
        return Err(
            "agent-manifest.json v2 plugin identity must match the package manifest".to_string(),
        );
    }
    if descriptor
        .agent_type
        .as_deref()
        .is_some_and(|agent_type| agent_type.trim().is_empty() || agent_type.len() > 80)
    {
        return Err("agent-manifest.json v2 agentType must be bounded".to_string());
    }
    if descriptor.frontend_context_required.is_none()
        || descriptor.model_configuration.is_none()
        || descriptor.pipeline.is_none()
        || descriptor.security_boundary.is_none()
    {
        return Err(
            "agent-manifest.json v2 must declare frontend context, model, pipeline, and security boundary contracts"
                .to_string(),
        );
    }
    validate_agent_frontend_contract(&descriptor.frontend, manifest)?;
    validate_agent_worker_contract(&descriptor.worker, manifest, directory)?;
    Ok(())
}

fn validate_agent_frontend_contract(
    frontend: &AgentFrontendContract,
    manifest: &MycPluginManifest,
) -> Result<(), String> {
    if frontend.mode != "trusted-module" {
        return Err("agent-manifest.json v2 frontend mode must be trusted-module".to_string());
    }
    if frontend.framework != "vue3" {
        return Err("agent-manifest.json v2 frontend framework must be vue3".to_string());
    }
    validate_relative_plugin_path(&frontend.entry, "agent frontend entry")?;
    if !frontend.entry.ends_with(".mjs") {
        return Err("agent-manifest.json v2 frontend entry must be an .mjs file".to_string());
    }
    for export in &frontend.exports {
        validate_export_name(export)?;
    }
    if frontend.exports.is_empty() || frontend.exports.len() > 64 {
        return Err(
            "agent-manifest.json v2 frontend exports must contain 1 to 64 names".to_string(),
        );
    }
    let package_frontend = manifest.frontend.as_ref().ok_or_else(|| {
        "agent-manifest.json v2 requires a package frontend descriptor".to_string()
    })?;
    if package_frontend.mode != frontend.mode
        || package_frontend.entry != frontend.entry
        || package_frontend.framework != frontend.framework
        || package_frontend.api_version != frontend.api_version
    {
        return Err(
            "agent-manifest.json v2 frontend contract must match the package manifest frontend"
                .to_string(),
        );
    }
    let package_ui = manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.ui.as_ref())
        .ok_or_else(|| "agent-manifest.json v2 requires package contributes.ui".to_string())?;
    if frontend.ui_contributions.len() != package_ui.len() {
        return Err(
            "agent-manifest.json v2 uiContributions must match package contributes.ui".to_string(),
        );
    }
    for contribution in &frontend.ui_contributions {
        validate_wire_identifier(&contribution.id, "agent UI contribution id")?;
        validate_wire_identifier(&contribution.slot_id, "agent UI slot id")?;
        validate_export_name(&contribution.export_name)?;
        if !frontend
            .exports
            .iter()
            .any(|export| export == &contribution.export_name)
        {
            return Err(
                "agent-manifest.json v2 UI exports must be listed in frontend exports".to_string(),
            );
        }
        let Some(package_contribution) = package_ui.iter().find(|item| item.id == contribution.id)
        else {
            return Err(
                "agent-manifest.json v2 uiContributions must reference package contributes.ui ids"
                    .to_string(),
            );
        };
        if package_contribution.slot_id != contribution.slot_id
            || package_contribution.export_name != contribution.export_name
        {
            return Err(
                "agent-manifest.json v2 uiContributions must match package slot and export"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn validate_agent_worker_contract(
    worker: &AgentWorkerContract,
    manifest: &MycPluginManifest,
    directory: &Path,
) -> Result<(), String> {
    validate_wire_identifier(&worker.id, "agent worker id")?;
    validate_wire_identifier(&worker.language, "agent worker language")?;
    validate_relative_plugin_path(&worker.entrypoint, "agent worker entrypoint")?;
    if worker.opened_by != "plugin.frontend" {
        return Err("agent-manifest.json v2 worker must be openedBy plugin.frontend".to_string());
    }
    if worker.credentials != "host-secret-injected-to-exact-worker" {
        return Err(
            "agent-manifest.json v2 worker credentials contract is unsupported".to_string(),
        );
    }
    if worker.forbidden_host_operations.is_empty() || worker.forbidden_host_operations.len() > 32 {
        return Err(
            "agent-manifest.json v2 worker must declare bounded forbiddenHostOperations"
                .to_string(),
        );
    }
    for operation in &worker.forbidden_host_operations {
        validate_worker_operation_name(operation, "forbidden Host Bus operation")?;
    }
    let package_worker = manifest_worker_descriptors(manifest)
        .find(|descriptor| descriptor.id == worker.id)
        .ok_or_else(|| {
            "agent-manifest.json v2 worker id must match a package worker".to_string()
        })?;
    if package_worker.language != worker.language
        || package_worker.entrypoint != worker.entrypoint
        || package_worker.transport != worker.transport
        || package_worker.operations != worker.operations
        || package_worker.host_operations != worker.host_operations
    {
        return Err(
            "agent-manifest.json v2 worker contract must match the package worker descriptor"
                .to_string(),
        );
    }
    validate_worker_descriptor(package_worker)?;
    validate_worker_entrypoint(directory, package_worker)?;
    Ok(())
}

fn resolve_installed_relative_file(
    directory: &Path,
    relative_path: &str,
    label: &str,
) -> Result<PathBuf, String> {
    validate_relative_plugin_path(relative_path, label)?;
    let root = fs::canonicalize(directory)
        .map_err(|error| format!("Could not canonicalize plugin root: {error}"))?;
    let candidate = directory.join(relative_path);
    let canonical = fs::canonicalize(&candidate)
        .map_err(|error| format!("Could not resolve {label} {relative_path}: {error}"))?;
    if !canonical.starts_with(&root) || !canonical.is_file() {
        return Err(format!("{label} escapes the installed plugin root"));
    }
    Ok(canonical)
}

fn read_installed_frontend(
    directory: &Path,
    frontend: Option<&PluginFrontendDescriptor>,
) -> Result<Option<InstalledPluginFrontend>, String> {
    let Some(frontend) = frontend else {
        return Ok(None);
    };
    let entry_path = resolve_installed_relative_file(directory, &frontend.entry, "frontend entry")?;
    let bytes = fs::read(&entry_path)
        .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
    let digest = Sha256::digest(&bytes);
    Ok(Some(InstalledPluginFrontend {
        mode: frontend.mode.clone(),
        framework: frontend.framework.clone(),
        api_version: frontend.api_version.clone(),
        entry: frontend.entry.clone(),
        installed_entry_path: entry_path.to_string_lossy().into_owned(),
        entry_sha256: format!("{digest:x}"),
    }))
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

fn read_private_i18n_bundles(
    directory: &Path,
    manifest: &MycPluginManifest,
    private_i18n: Option<&PluginPrivateI18nDefinition>,
) -> Result<Option<InstalledPluginPrivateI18n>, String> {
    let Some(private_i18n) = private_i18n else {
        return Ok(None);
    };
    let mut locales = BTreeMap::new();
    for (locale, relative_path) in &private_i18n.locales {
        let path = directory.join(relative_path);
        let text = fs::read_to_string(&path)
            .map_err(|error| format!("Could not read private i18n {}: {error}", path.display()))?;
        let messages: BTreeMap<String, String> = serde_json::from_str(&text)
            .map_err(|error| format!("Invalid private i18n {}: {error}", path.display()))?;
        if messages.len() > 2_000 {
            return Err("Private i18n bundle exceeds 2,000 messages".to_string());
        }
        if messages.iter().any(|(key, message)| {
            key.is_empty() || key.chars().count() > 128 || message.chars().count() > 2_000
        }) {
            return Err("Private i18n accepts bounded string-to-string messages only".to_string());
        }
        locales.insert(locale.clone(), messages);
    }
    Ok(Some(InstalledPluginPrivateI18n {
        namespace: manifest.metadata.id.clone(),
        default_locale: private_i18n.default_locale.clone(),
        locales,
    }))
}

fn parse_private_i18n_manifest(
    manifest_text: &str,
) -> Result<Option<PluginPrivateI18nDefinition>, String> {
    let document: serde_json::Value =
        serde_json::from_str(manifest_text).map_err(|error| error.to_string())?;
    let Some(spec) = document.get("spec") else {
        return Ok(None);
    };
    let Some(private_i18n) = spec.get("privateI18n") else {
        return Ok(None);
    };
    serde_json::from_value(private_i18n.clone())
        .map(Some)
        .map_err(|error| format!("Invalid privateI18n declaration: {error}"))
}

fn validate_ui_ir_reference_path(value: &str, suffix: &str) -> Result<(), String> {
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().count() != 2
        || path.parent() != Some(Path::new("ui"))
        || !value.ends_with(suffix)
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| name.is_empty())
    {
        return Err(format!("UI IR files must use ui/<name>{suffix}"));
    }
    Ok(())
}

fn read_ui_ir_contributions(
    directory: &Path,
    manifest: &MycPluginManifest,
) -> Result<Option<Vec<serde_json::Value>>, String> {
    let Some(raw) = manifest
        .spec
        .contributes
        .as_ref()
        .and_then(|contributions| contributions.ui_ir.as_ref())
    else {
        return Ok(None);
    };
    // Older tracked packages may still carry the removed inline shape. Keep
    // installation compatibility, but never hydrate or expose that data.
    let Some(references) = raw.as_array() else {
        return Ok(None);
    };
    if references.iter().any(|reference| {
        reference
            .as_object()
            .is_some_and(|object| object.contains_key("ir"))
    }) {
        // Legacy inline IR is deliberately ignored rather than exposed.
        return Ok(None);
    }
    if references.len() > 16 {
        return Err("A plugin can contribute at most 16 UI IR surfaces".to_string());
    }
    let mut hydrated = Vec::with_capacity(references.len());
    for reference in references {
        let object = reference
            .as_object()
            .ok_or_else(|| "UI IR contribution must be an object".to_string())?;
        let allowed = ["slotId", "source", "artifact"];
        if object.keys().any(|key| !allowed.contains(&key.as_str()))
            || object.len() != allowed.len()
        {
            return Err(
                "UI IR contribution must contain only slotId, source, and artifact".to_string(),
            );
        }
        let slot_id = object
            .get("slotId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "UI IR slotId must be a string".to_string())?;
        validate_slug(slot_id, "UI IR slot id")?;
        let source = object
            .get("source")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "UI IR source must be a string".to_string())?;
        let artifact = object
            .get("artifact")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "UI IR artifact must be a string".to_string())?;
        validate_ui_ir_reference_path(source, ".vue")?;
        validate_ui_ir_reference_path(artifact, ".uiir.json")?;

        let source_path = directory.join(source);
        let artifact_path = directory.join(artifact);
        let source_metadata = fs::metadata(&source_path)
            .map_err(|error| format!("Could not read UI IR source {source}: {error}"))?;
        if !source_metadata.is_file() || source_metadata.len() > MAX_UI_IR_SOURCE_BYTES {
            return Err(format!(
                "UI IR source exceeds {} bytes",
                MAX_UI_IR_SOURCE_BYTES
            ));
        }
        let artifact_metadata = fs::metadata(&artifact_path)
            .map_err(|error| format!("Could not read UI IR artifact {artifact}: {error}"))?;
        if !artifact_metadata.is_file() || artifact_metadata.len() > MAX_UI_IR_ARTIFACT_BYTES {
            return Err(format!(
                "UI IR artifact exceeds {} bytes",
                MAX_UI_IR_ARTIFACT_BYTES
            ));
        }
        let artifact_value: serde_json::Value =
            serde_json::from_slice(&fs::read(&artifact_path).map_err(|error| error.to_string())?)
                .map_err(|error| format!("Invalid UI IR artifact {artifact}: {error}"))?;
        if artifact_value
            .get("apiVersion")
            .and_then(serde_json::Value::as_str)
            != Some("anyway.dev/ui-ir/v1")
            || !artifact_value
                .get("root")
                .is_some_and(serde_json::Value::is_object)
        {
            return Err(format!(
                "UI IR artifact {artifact} has an invalid document shape"
            ));
        }
        hydrated.push(serde_json::json!({ "slotId": slot_id, "ir": artifact_value }));
    }
    Ok(Some(hydrated))
}

fn read_installed_plugin(directory: &Path) -> Result<InstalledMycPlugin, String> {
    {
        if let Ok(cache) = manifest_cache().lock() {
            if let Some(installed) = cache.get(directory) {
                return Ok(installed.clone());
            }
        }
    }
    let manifest_path = directory.join("plugin.json");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("Could not read {}: {error}", manifest_path.display()))?;
    let private_i18n = parse_private_i18n_manifest(&manifest_text)?;
    let mut manifest: MycPluginManifest =
        crate::plugin_manifest_v2::parse_plugin_manifest(&manifest_text)?;
    validate_manifest(&manifest)?;
    validate_private_i18n(private_i18n.as_ref())?;
    let ui_ir_contributions = read_ui_ir_contributions(directory, &manifest)?;
    let frontend = read_installed_frontend(directory, manifest.frontend.as_ref())?;
    for worker in manifest_worker_descriptors(&manifest) {
        validate_worker_entrypoint(directory, worker)?;
    }

    let entry_path = directory.join(&manifest.spec.entry);
    let (theme, icon_theme, edge_style, runtime, workspace, provider, agent) =
        match manifest.kind.as_str() {
            "ThemePlugin" => {
                let entry_text = fs::read_to_string(&entry_path)
                    .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
                let theme: ThemeManifest =
                    serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
                let edge_style = theme.edge_style.clone();
                (Some(theme), None, edge_style, None, None, None, None)
            }
            "IconThemePlugin" => {
                let entry_text = fs::read_to_string(&entry_path)
                    .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
                let icon_theme: IconThemeManifest =
                    serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
                (None, Some(icon_theme), None, None, None, None, None)
            }
            "EdgeStylePlugin" => {
                let entry_text = fs::read_to_string(&entry_path)
                    .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
                (
                    None,
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
                (None, None, None, None, Some(descriptor), None, None)
            }
            "ProviderPlugin" => {
                let entry_text = fs::read_to_string(&entry_path)
                    .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
                let descriptor: ProviderDescriptor =
                    serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
                llm_plugin::validate_provider_descriptor(&descriptor)?;
                (None, None, None, None, None, Some(descriptor), None)
            }
            "AgentPlugin" => {
                let entry_text = fs::read_to_string(&entry_path)
                    .map_err(|error| format!("Could not read {}: {error}", entry_path.display()))?;
                let descriptor: serde_json::Value =
                    serde_json::from_str(&entry_text).map_err(|error| error.to_string())?;
                validate_agent_manifest_descriptor(&descriptor, &mut manifest, directory)?;
                (None, None, None, None, None, None, Some(descriptor))
            }
            "ExtensionPlugin" => (None, None, None, None, None, None, None),
            _ => (None, None, None, None, None, None, None),
        };
    let locales = read_locale_bundles(directory, &manifest)?;
    let private_i18n = read_private_i18n_bundles(directory, &manifest, private_i18n.as_ref())?;

    let installed = InstalledMycPlugin {
        manifest,
        install_path: directory.to_string_lossy().into_owned(),
        theme,
        icon_theme,
        edge_style,
        runtime,
        frontend,
        locales,
        private_i18n,
        workspace,
        provider,
        agent,
        ui_ir_contributions,
    };
    if let Ok(mut cache) = manifest_cache().lock() {
        cache.insert(directory.to_path_buf(), installed.clone());
    }
    Ok(installed)
}

fn validate_icon_theme_asset_path(asset_path: &str) -> Result<PathBuf, String> {
    if asset_path.is_empty()
        || asset_path.starts_with('/')
        || asset_path.contains('\\')
        || asset_path.contains(':')
        || !asset_path.starts_with("assets/")
    {
        return Err("Icon theme asset must be a relative path under assets/".to_string());
    }
    let relative = PathBuf::from(asset_path);
    if relative.components().any(|component| {
        matches!(
            component,
            std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::CurDir
                | std::path::Component::ParentDir
        )
    }) {
        return Err("Icon theme asset path contains an unsafe component".to_string());
    }
    let extension = relative
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    if !matches!(
        extension.as_deref(),
        Some("svg" | "png" | "woff" | "woff2" | "ttf" | "otf")
    ) {
        return Err("Icon theme assets must be SVG, PNG, or font files".to_string());
    }
    Ok(relative)
}

fn icon_theme_references_asset(icon_theme: &IconThemeManifest, asset_path: &str) -> bool {
    let definition_reference = icon_theme.icon_definitions.values().any(|definition| {
        definition
            .get("iconPath")
            .and_then(serde_json::Value::as_str)
            == Some(asset_path)
    });
    if definition_reference {
        return true;
    }
    icon_theme.fonts.iter().any(|font| {
        font.get("src")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|sources| {
                sources.iter().any(|source| {
                    source.as_str() == Some(asset_path)
                        || source.get("path").and_then(serde_json::Value::as_str)
                            == Some(asset_path)
                })
            })
    })
}

fn icon_theme_asset_mime(asset_path: &Path) -> &'static str {
    match asset_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        _ => "application/octet-stream",
    }
}

fn validate_icon_theme_image(asset_path: &Path, bytes: &[u8]) -> Result<(), String> {
    match asset_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => {
            if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err("Icon theme PNG has an invalid signature".to_string());
            }
        }
        Some("svg") => {
            let text = std::str::from_utf8(bytes)
                .map_err(|_| "Icon theme SVG must be valid UTF-8".to_string())?;
            let lower = text.to_ascii_lowercase();
            if !lower.contains("<svg") {
                return Err("Icon theme SVG has no svg root".to_string());
            }
            let forbidden = [
                "<script",
                "<foreignobject",
                "<!doctype",
                "<!entity",
                "javascript:",
                "href=\"http:",
                "href='http:",
                "href=\"https:",
                "href='https:",
                "href=\"//",
                "href='//",
                "url(http:",
                "url(https:",
                "url(//",
                "@import",
            ];
            if forbidden.iter().any(|needle| lower.contains(needle))
                || regex::Regex::new(r"(?i)\son[a-z]+\s*=")
                    .expect("static SVG event-handler regex")
                    .is_match(text)
            {
                return Err("Icon theme SVG contains active or external content".to_string());
            }
        }
        _ => {}
    }
    Ok(())
}

/// Returns a data URL only for an asset referenced by an installed declarative
/// IconThemePlugin. No caller can use this command to read arbitrary files.
#[tauri::command]
pub fn read_icon_theme_asset(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    asset_path: String,
) -> Result<String, String> {
    let directory = installed_plugin_directory(&app, &plugin_id, &plugin_version)?;
    let installed = read_installed_plugin(&directory)?;
    if installed.manifest.kind != "IconThemePlugin" {
        return Err("Plugin is not an IconThemePlugin".to_string());
    }
    let icon_theme = installed
        .icon_theme
        .as_ref()
        .ok_or_else(|| "IconThemePlugin has no icon theme descriptor".to_string())?;
    if !icon_theme_references_asset(icon_theme, &asset_path) {
        return Err("Asset is not referenced by the installed icon theme".to_string());
    }
    let relative = validate_icon_theme_asset_path(&asset_path)?;
    let canonical_directory = directory
        .canonicalize()
        .map_err(|error| format!("Could not resolve icon theme directory: {error}"))?;
    let candidate = directory.join(&relative);
    let metadata = fs::symlink_metadata(&candidate)
        .map_err(|error| format!("Could not read icon theme asset: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Icon theme asset must be a regular file".to_string());
    }
    if metadata.len() > MAX_ICON_THEME_ASSET_BYTES {
        return Err("Icon theme asset exceeds the 4 MB limit".to_string());
    }
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|error| format!("Could not resolve icon theme asset: {error}"))?;
    if !canonical_candidate.starts_with(&canonical_directory) {
        return Err("Icon theme asset escaped its installed plugin directory".to_string());
    }
    let bytes = fs::read(&canonical_candidate).map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_ICON_THEME_ASSET_BYTES {
        return Err("Icon theme asset exceeds the 4 MB limit".to_string());
    }
    validate_icon_theme_image(&relative, &bytes)?;
    Ok(format!(
        "data:{};base64,{}",
        icon_theme_asset_mime(&relative),
        BASE64.encode(bytes)
    ))
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

/// 从已打开的归档中读取 plugin.json(含签名校验),供发现与安装共用。
/// Reads plugin.json from an already-opened archive (with signature verification),
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
        .by_name("plugin.json")
        .map_err(|_| "plugin.json is required at the package root".to_string())?;
    let mut text = String::new();
    entry
        .read_to_string(&mut text)
        .map_err(|error| error.to_string())?;
    let manifest: MycPluginManifest = crate::plugin_manifest_v2::parse_plugin_manifest(&text)?;
    validate_manifest(&manifest)?;

    // --- Ed25519 签名验证 / Ed25519 signature verification ---
    if let Some(ref signature_b64) = manifest.signature {
        if signature_b64.trim().is_empty() {
            return Err("Plugin manifest contains an empty signature field".to_string());
        }
        let trusted_keys = crate::signing::load_all_trusted_keys(base)?;
        // The signature covers the canonical raw manifest JSON (sorted keys,
        // compact) with the `signature` field removed — the exact bytes the
        // packager signed, independent of the internal v1 struct migration.
        let mut raw_without_signature: serde_json::Value =
            serde_json::from_str(&text).map_err(|error| error.to_string())?;
        raw_without_signature
            .as_object_mut()
            .and_then(|object| object.remove("signature"));
        crate::signing::verify_manifest_signature(
            &manifest.metadata.publisher,
            &raw_without_signature,
            signature_b64,
            &trusted_keys,
        )?;
    }
    // --- 签名验证结束 / End signature verification ---

    Ok(manifest)
}

/// 仅读取归档中的 plugin.json(含签名校验)。
/// Reads only plugin.json from an archive (with signature verification).
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
                if relative == "plugin.json" {
                    continue;
                }
                let Some(expected) = payloads.get(&relative) else {
                    return Err(format!(
                        "Unlisted payload file in signed package: {relative}"
                    ));
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
    invalidate_manifest_cache(&destination);
    read_installed_plugin(&destination)
}

#[cfg(test)]
pub(crate) fn install_archive_into_for_test(
    base: &Path,
    archive_path: &Path,
) -> Result<InstalledMycPlugin, String> {
    install_archive_into(base, archive_path)
}

fn install_archive(app: &AppHandle, archive_path: &Path) -> Result<InstalledMycPlugin, String> {
    let base = plugin_base(app)?;
    install_archive_into(&base, archive_path)
}

#[cfg(any(test, not(debug_assertions)))]
fn install_pending_from(
    base: &Path,
    packages: &Path,
    removed: &HashSet<String>,
) -> Result<(), String> {
    install_pending_from_with_allowlist(base, packages, removed, None)
}

fn install_pending_from_with_allowlist(
    base: &Path,
    packages: &Path,
    removed: &HashSet<String>,
    allowed_plugin_ids: Option<&HashSet<String>>,
) -> Result<(), String> {
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
        if let Some(allowed) = allowed_plugin_ids {
            if !allowed.contains(&manifest.metadata.id) {
                continue;
            }
        }
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

fn desktop_dev_allowlist() -> Result<HashSet<String>, String> {
    let value: serde_json::Value = serde_json::from_str(PLUGIN_LOADING_CONFIG)
        .map_err(|error| format!("Invalid plugin-loading.json: {error}"))?;
    value
        .pointer("/desktopDev/allowedPluginIds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "plugin-loading.json is missing desktopDev.allowedPluginIds".to_string())?
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|id| !id.is_empty())
                .map(ToOwned::to_owned)
                .ok_or_else(|| "desktopDev.allowedPluginIds must contain strings".to_string())
        })
        .collect()
}

fn staged_desktop_dev_allowlist(base: &Path) -> Result<HashSet<String>, String> {
    let mut allowed = desktop_dev_allowlist()?;
    let manifest_path = base.join("dev-manifest.json");
    if !manifest_path.is_file() {
        return Ok(allowed);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("Invalid staged dev-manifest.json: {error}"))?;
    let fresh_builds = value
        .get("freshBuilds")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "staged dev-manifest.json is missing freshBuilds".to_string())?;
    for build in fresh_builds {
        let plugin_id = build
            .get("pluginId")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "staged dev-manifest freshBuilds require pluginId".to_string())?;
        allowed.insert(plugin_id.to_string());
    }
    Ok(allowed)
}

pub(crate) fn install_pending_packages(app: &AppHandle) -> Result<(), String> {
    let base = plugin_base(app)?;
    let removed = read_removed_plugins(&base)?;
    #[cfg(debug_assertions)]
    {
        let allowlist = staged_desktop_dev_allowlist(&base)?;
        install_pending_from_with_allowlist(
            &base,
            &base.join("packages"),
            &removed,
            Some(&allowlist),
        )?;
    }
    #[cfg(not(debug_assertions))]
    {
        install_pending_from(&base, &base.join("packages"), &removed)?;
    }
    Ok(())
}

fn installed_plugin_directory(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<PathBuf, String> {
    validate_slug(plugin_id, "plugin id")?;
    validate_slug(plugin_version, "plugin version")?;
    let directory = plugin_base(app)?
        .join("installed")
        .join(format!("{plugin_id}@{plugin_version}"));
    if !directory.is_dir() {
        return Err(format!(
            "Plugin {plugin_id}@{plugin_version} is not installed"
        ));
    }
    Ok(directory)
}

fn read_installed_plugin_by_identity(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<(PathBuf, InstalledMycPlugin), String> {
    let directory = installed_plugin_directory(app, plugin_id, plugin_version)?;
    let installed = read_installed_plugin(&directory)?;
    if installed.manifest.metadata.id != plugin_id
        || installed.manifest.metadata.version != plugin_version
    {
        return Err("Installed plugin identity does not match its directory".to_string());
    }
    Ok((directory, installed))
}

pub(crate) fn plugin_worker_launch_plan(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    worker_id: &str,
) -> Result<crate::host_bus::workers::PluginWorkerLaunchPlan, String> {
    let (directory, installed) = read_installed_plugin_by_identity(app, plugin_id, plugin_version)?;
    let worker = manifest_worker_descriptors(&installed.manifest)
        .find(|descriptor| descriptor.id == worker_id)
        .ok_or_else(|| {
            format!("Plugin {plugin_id}@{plugin_version} does not declare worker {worker_id}")
        })?;
    let entry = validate_worker_entrypoint(&directory, worker)?;
    let entry_bytes = fs::read(&entry).map_err(|error| {
        format!(
            "Could not read worker entrypoint {}: {error}",
            entry.display()
        )
    })?;
    let fingerprint = crate::host_bus::workers::PluginWorkerLaunchPlan::fingerprint_for(
        plugin_id,
        plugin_version,
        &worker.id,
        &worker.language,
        &worker.transport,
        &worker.entrypoint,
        &entry_bytes,
    );
    let command = if worker.language == "python" {
        crate::host_bus::workers::PluginWorkerCommand {
            executable: PathBuf::from("python"),
            args: vec![entry.into_os_string()],
            working_directory: Some(directory.clone()),
        }
    } else {
        crate::host_bus::workers::PluginWorkerCommand {
            executable: entry,
            args: Vec::new(),
            working_directory: Some(directory.clone()),
        }
    };
    let mut secret_environment = crate::host_bus::workers::SecretEnv::default();
    for egress in &worker.provider_egress {
        let connection = installed
            .manifest
            .spec
            .connections
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|connection| connection.id == egress.connection_id)
            .ok_or_else(|| {
                format!(
                    "Worker provider {} references unknown connection {}",
                    egress.provider_id, egress.connection_id
                )
            })?;
        let secret = match &connection.api_key {
            PluginApiKeySource::HostSecret { setting_id } => {
                crate::plugin_settings::resolve_host_secret(plugin_id, plugin_version, setting_id)?
            }
            PluginApiKeySource::Environment {
                name,
                fallback_setting_id,
            } => {
                let environment = std::env::var(name)
                    .ok()
                    .filter(|value| !value.trim().is_empty());
                match (environment, fallback_setting_id.as_deref()) {
                    (Some(value), _) => Some(value),
                    (None, Some(setting_id)) => crate::plugin_settings::resolve_host_secret(
                        plugin_id,
                        plugin_version,
                        setting_id,
                    )?,
                    (None, None) => None,
                }
            }
        };
        if let Some(secret) = secret {
            secret_environment.insert(egress.secret_env.clone(), secret);
        }
    }
    Ok(crate::host_bus::workers::PluginWorkerLaunchPlan {
        plugin_id: plugin_id.to_string(),
        plugin_version: plugin_version.to_string(),
        worker_id: worker.id.clone(),
        language: worker.language.clone(),
        transport: worker.transport.clone(),
        entrypoint: worker.entrypoint.clone(),
        command,
        allowed_operations: worker.operations.clone(),
        host_operations: worker.host_operations.clone(),
        fingerprint,
        environment: BTreeMap::new(),
        secret_environment,
    })
}

/// 将用户传入的插件路径解析为允许的 packages 目录下的真实路径。
/// Stages an external package into the configured packages directory.
///
/// The destination is created with `create_new`, so an existing package or
/// symlink is never overwritten. The source is opened only after canonical
/// resolution and is copied with the archive-size bound applied.
fn stage_external_myc_package(base: &Path, path: &Path) -> Result<PathBuf, String> {
    let source = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve plugin path: {error}"))?;
    let mut source_file = File::open(&source).map_err(|error| error.to_string())?;
    let source_metadata = source_file.metadata().map_err(|error| error.to_string())?;
    if !source_metadata.is_file() {
        return Err("Plugin package must be a regular file".to_string());
    }
    if !source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("myc"))
    {
        return Err("Plugin package must use the .myc extension".to_string());
    }
    if source_metadata.len() > MAX_ARCHIVE_BYTES {
        return Err("Plugin package exceeds the 16 MB archive limit".to_string());
    }

    let packages = base.join("packages");
    fs::create_dir_all(&packages).map_err(|error| error.to_string())?;
    let packages = packages
        .canonicalize()
        .map_err(|error| format!("Cannot resolve packages directory: {error}"))?;
    if !packages.is_dir() {
        return Err("Configured packages path is not a directory".to_string());
    }

    let source_name = source
        .file_name()
        .ok_or_else(|| "Plugin package path has no file name".to_string())?;
    for attempt in 0_u32.. {
        let destination = if attempt == 0 {
            packages.join(source_name)
        } else {
            packages.join(format!(".imported-{}-{attempt}.myc", std::process::id()))
        };
        let mut destination_file = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        };

        let copy_result = (|| -> Result<(), String> {
            let mut bounded_source = source_file.by_ref().take(MAX_ARCHIVE_BYTES + 1);
            let copied = io::copy(&mut bounded_source, &mut destination_file)
                .map_err(|error| error.to_string())?;
            if copied > MAX_ARCHIVE_BYTES {
                return Err("Plugin package exceeds the 16 MB archive limit".to_string());
            }
            destination_file
                .sync_all()
                .map_err(|error| error.to_string())?;
            Ok(())
        })();
        if let Err(error) = copy_result {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        return Ok(destination);
    }
    unreachable!("u32 staging attempts are exhausted")
}

/// Resolves a caller-supplied plugin path. Existing paths inside the
/// configured `packages` directory retain their current behavior; external
/// files are canonicalized, validated, and copied into that directory first.
fn resolve_package_path(base: &Path, path: &Path) -> Result<PathBuf, String> {
    let allowed = base.join("packages");
    let input = path
        .canonicalize()
        .map_err(|error| format!("Cannot resolve plugin path: {error}"))?;
    let normalized_allowed = allowed.canonicalize().unwrap_or(allowed);
    if input.starts_with(&normalized_allowed) {
        return Ok(input);
    }
    stage_external_myc_package(base, &input)
}

/// Computes the lowercase hex SHA-256 of a resolved plugin package archive.
///
/// The path is resolved exactly like `install_myc_plugin` (plugin base plus
/// `resolve_package_path`), so the digest that keys the kernel PackageGate
/// admission transaction covers the same staged bytes the real install reads.
/// This is the gate's lightweight, deterministic pre-check: the archive must
/// resolve and be readable, and it must not be empty; the full
/// manifest/signature/payload validation still happens in `install_archive`.
pub(crate) fn package_digest(app: &AppHandle, path: &str) -> Result<String, String> {
    let base = plugin_base(app)?;
    let input = resolve_package_path(&base, Path::new(path))?;
    let bytes = fs::read(&input).map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err("Plugin package is empty".to_string());
    }
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

#[tauri::command]
pub fn install_myc_plugin(app: AppHandle, path: String) -> Result<InstalledMycPlugin, String> {
    let base = plugin_base(&app)?;
    let input = resolve_package_path(&base, Path::new(&path))?;
    let installed = install_archive(&app, &input)?;
    // 显式安装视为用户重新启用该插件，清除移除墓碑。
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
    crate::plugin_settings::remove_plugin_settings(&app, &plugin_id, &plugin_version)?;
    uninstall_plugin_from(&base, &plugin_id, &plugin_version)
}

#[tauri::command]
pub fn list_installed_plugins(app: AppHandle) -> Result<Vec<InstalledMycPlugin>, String> {
    install_pending_packages(&app)?;
    let root = plugin_base(&app)?.join("installed");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    query_installed_plugins(&app)
}

/// Read-only catalog query used after startup package discovery has completed.
/// It never scans package candidates, creates directories, or activates code.
pub(crate) fn query_installed_plugins(app: &AppHandle) -> Result<Vec<InstalledMycPlugin>, String> {
    let base = plugin_base(app)?;
    query_installed_plugins_from(&base)
}

fn query_installed_plugins_from(base: &Path) -> Result<Vec<InstalledMycPlugin>, String> {
    let root = base.join("installed");
    if !root.is_dir() {
        return Ok(Vec::new());
    }
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
pub fn get_plugin_settings(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
) -> Result<crate::plugin_settings::PluginSettingsSnapshot, String> {
    let (_directory, installed) =
        read_installed_plugin_by_identity(&app, &plugin_id, &plugin_version)?;
    crate::plugin_settings::get_snapshot(&app, &installed.manifest, &plugin_id, &plugin_version)
}

fn select_connection_test_action(
    plugin_id: &str,
    connection: &PluginConnectionDefinition,
    action_id: Option<&str>,
) -> Result<Option<PluginConnectionTestAction>, String> {
    let requested = action_id.map(str::trim).filter(|value| !value.is_empty());
    if let Some(requested) = requested {
        if let Some(action) = connection
            .test_actions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|action| action.id == requested)
        {
            return Ok(Some(action.clone()));
        }
        if connection
            .test_action
            .as_ref()
            .is_some_and(|action| action.id == requested)
        {
            return Ok(connection.test_action.clone());
        }
        // The built-in PDF Agent is implemented natively. Development builds
        // may still have an older 0.3.0 package installed under the same
        // identity, from before the second action was added to testActions.
        // Keep that installed package usable while the source manifest remains
        // the canonical declaration for fresh installs.
        if plugin_id == "myc.pdf-canvas-agent" && requested == "test-pdf-extraction" {
            return Ok(Some(PluginConnectionTestAction {
                id: requested.to_string(),
                label: "Test PDF extraction".to_string(),
                label_key: Some("actions.testPdfExtraction.label".to_string()),
                description: Some(
                    "Use the built-in non-empty test PDF. It may be uploaded when remote extraction is selected."
                        .to_string(),
                ),
                description_key: Some("actions.testPdfExtraction.description".to_string()),
                placeholder: None,
                placeholder_key: None,
                kind: Some("pdf-extraction".to_string()),
                input: Some(PluginConnectionTestActionInput::BundledPdf {
                    fixture: "host-minimal-pdf-v1".to_string(),
                    file_upload: "may-upload".to_string(),
                }),
            }));
        }
        return Err(format!(
            "Plugin connection {} does not declare test action {}",
            connection.id, requested
        ));
    }
    Ok(connection.test_action.clone().or_else(|| {
        connection
            .test_actions
            .as_ref()
            .and_then(|actions| actions.first().cloned())
    }))
}

#[tauri::command]
pub fn set_plugin_settings(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    values: BTreeMap<String, serde_json::Value>,
) -> Result<crate::plugin_settings::PluginSettingsSnapshot, String> {
    let (_directory, installed) =
        read_installed_plugin_by_identity(&app, &plugin_id, &plugin_version)?;
    crate::plugin_settings::set_values(
        &app,
        &installed.manifest,
        &plugin_id,
        &plugin_version,
        values,
    )
}

#[tauri::command]
pub fn reset_plugin_settings(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
) -> Result<crate::plugin_settings::PluginSettingsSnapshot, String> {
    let (_directory, installed) =
        read_installed_plugin_by_identity(&app, &plugin_id, &plugin_version)?;
    crate::plugin_settings::reset_values(&app, &installed.manifest, &plugin_id, &plugin_version)
}

/** 原生动作前解析已安装包并验证一个命名能力 / Resolve an installed package and prove one capability. */
#[tauri::command]
pub async fn test_plugin_connection(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    connection_id: String,
    // Tauri exposes this snake_case Rust parameter as the actionId JSON key;
    // action_id remains the canonical Rust-side spelling for both callers.
    action_id: Option<String>,
    values: BTreeMap<String, serde_json::Value>,
    secrets: BTreeMap<String, crate::plugin_settings::PluginSecretMutationInput>,
) -> Result<crate::plugin_settings::PluginConnectionTestResult, String> {
    let (_directory, installed) =
        read_installed_plugin_by_identity(&app, &plugin_id, &plugin_version)?;
    let connection = installed
        .manifest
        .spec
        .connections
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|connection| connection.id == connection_id)
        .ok_or_else(|| format!("Unknown plugin connection: {connection_id}"))?;
    let selected_action =
        select_connection_test_action(&plugin_id, connection, action_id.as_deref())?;
    let mut manifest = installed.manifest.clone();
    if let Some(selected_action) = selected_action {
        if let Some(connection) =
            manifest.spec.connections.as_mut().and_then(|connections| {
                connections.iter_mut().find(|item| item.id == connection_id)
            })
        {
            // plugin_settings::test_connection currently consumes the legacy
            // single-action slot; mirror the selected action at this boundary
            // while keeping the canonical testActions declaration intact.
            connection.test_action = Some(selected_action);
        }
    }
    crate::plugin_settings::test_connection(
        &app,
        &manifest,
        &plugin_id,
        &plugin_version,
        &connection_id,
        action_id,
        values,
        secrets,
    )
    .await
}

pub fn require_plugin_capability(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    capability: &str,
) -> Result<PathBuf, String> {
    require_plugin_capabilities(app, plugin_id, plugin_version, &[capability])
}

/// Validate capabilities for any installed plugin kind. This is the gate for
/// generic Host APIs such as plugin.files.pick and graph.patch.propose; the
/// installer remains responsible for constraining which capabilities each
/// plugin kind may declare.
pub fn require_installed_plugin_capability(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    capability: &str,
) -> Result<PathBuf, String> {
    require_installed_plugin_capabilities_from(
        &plugin_base(app)?,
        plugin_id,
        plugin_version,
        &[capability],
    )
}

/** 解析一个 WorkspacePlugin 并验证全部宿主能力 / Resolve one WorkspacePlugin and prove all requested capabilities. */
pub fn require_plugin_capabilities(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
    capabilities: &[&str],
) -> Result<PathBuf, String> {
    require_workspace_plugin_capabilities_from(
        &plugin_base(app)?,
        plugin_id,
        plugin_version,
        capabilities,
    )
}

fn installed_plugin_for_capability_check(
    base: &Path,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<(PathBuf, InstalledMycPlugin), String> {
    validate_slug(plugin_id, "plugin id")?;
    validate_slug(plugin_version, "plugin version")?;
    let directory = base
        .join("installed")
        .join(format!("{plugin_id}@{plugin_version}"));
    let installed = read_installed_plugin(&directory)?;
    Ok((directory, installed))
}

fn validate_declared_plugin_capabilities(
    installed: &InstalledMycPlugin,
    plugin_id: &str,
    plugin_version: &str,
    capabilities: &[&str],
) -> Result<(), String> {
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
    Ok(())
}

fn require_installed_plugin_capabilities_from(
    base: &Path,
    plugin_id: &str,
    plugin_version: &str,
    capabilities: &[&str],
) -> Result<PathBuf, String> {
    let (directory, installed) =
        installed_plugin_for_capability_check(base, plugin_id, plugin_version)?;
    validate_declared_plugin_capabilities(&installed, plugin_id, plugin_version, capabilities)?;
    Ok(directory)
}

fn require_workspace_plugin_capabilities_from(
    base: &Path,
    plugin_id: &str,
    plugin_version: &str,
    capabilities: &[&str],
) -> Result<PathBuf, String> {
    let (directory, installed) =
        installed_plugin_for_capability_check(base, plugin_id, plugin_version)?;
    if installed.manifest.kind != "WorkspacePlugin" {
        return Err("Native workspace actions require WorkspacePlugin".to_string());
    }
    validate_declared_plugin_capabilities(&installed, plugin_id, plugin_version, capabilities)?;
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

pub(crate) fn inject_trusted_host_settings(
    input: &serde_json::Value,
    plugin_id: &str,
    plugin_version: &str,
    settings: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let object = input
        .as_object()
        .ok_or_else(|| "Plugin call must be a JSON object".to_string())?;
    let mut sanitized = object.clone();
    // The frontend may submit an arbitrary `host` object, but it is never
    // allowed to survive into the guest invocation.
    let trusted_settings = settings
        .as_object()
        .ok_or_else(|| "Trusted plugin settings must be a JSON object".to_string())?;
    // Keep the guest-facing schema deliberately allow-listed. In particular,
    // a future caller cannot accidentally add a plaintext `secrets` field to
    // an execution envelope.
    let guest_settings = serde_json::json!({
        "effectiveValues": trusted_settings
            .get("effectiveValues")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
        "secretConfigured": trusted_settings
            .get("secretConfigured")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({})),
    });
    sanitized.remove("host");
    sanitized.insert(
        "host".to_string(),
        serde_json::json!({
            "pluginId": plugin_id,
            "pluginVersion": plugin_version,
            "settings": guest_settings,
        }),
    );
    Ok(serde_json::Value::Object(sanitized))
}

#[tauri::command]
pub fn execute_myc_plugin(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    capability: Option<String>,
    input: serde_json::Value,
) -> Result<crate::plugin_vm::PluginExecutionResult, String> {
    let (directory, installed) =
        read_installed_plugin_by_identity(&app, &plugin_id, &plugin_version)?;
    if installed.manifest.kind != "AnalysisPlugin" || installed.runtime.is_none() {
        return Err("Only installed AnalysisPlugin packages can execute".to_string());
    }
    let requested_capability = capability.as_deref().unwrap_or("analysis.run");
    validate_slug(requested_capability, "plugin capability")?;
    if !installed
        .manifest
        .spec
        .capabilities
        .iter()
        .any(|declared| declared == "analysis.run")
    {
        return Err("AnalysisPlugin must declare analysis.run".to_string());
    }
    if !installed
        .manifest
        .spec
        .capabilities
        .iter()
        .any(|declared| declared == requested_capability)
    {
        return Err(format!(
            "Plugin {plugin_id}@{plugin_version} does not declare {requested_capability}"
        ));
    }
    validate_analysis_call(&installed, &input)?;
    let persisted =
        crate::plugin_settings::persisted_values_for_execution(&app, &plugin_id, &plugin_version)?;
    let settings = crate::plugin_settings::build_execution_settings(
        &installed.manifest,
        &plugin_id,
        &plugin_version,
        persisted,
    )?;
    let trusted_input =
        inject_trusted_host_settings(&input, &plugin_id, &plugin_version, settings)?;
    let entry = directory.join(&installed.manifest.spec.entry);
    crate::plugin_vm::execute_plugin(&entry, &plugin_id, &plugin_version, &trusted_input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

    #[test]
    fn read_only_catalog_query_does_not_create_an_installed_directory() {
        let base = tempdir().expect("temp plugin base");

        assert!(query_installed_plugins_from(base.path())
            .expect("empty catalog")
            .is_empty());
        assert!(!base.path().join("installed").exists());
    }

    fn runtime_manifest(language: &str) -> String {
        format!(
            r#"{{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"AnalysisPlugin","metadata":{{"id":"myc.runtime-smoke","name":"Runtime Smoke","version":"1.0.0","publisher":"Research Canvas","developer":"Runtime Team","description":"End-to-end VM smoke plugin."}},"spec":{{"engine":"wasm32-myc","entry":"plugin.wasm","language":"{language}","capabilities":["analysis.run"],"permissions":[]}}}}"#,
        )
    }

    fn python_worker_descriptor(entrypoint: &str) -> PluginWorkerDescriptor {
        PluginWorkerDescriptor {
            id: "default".to_string(),
            language: "python".to_string(),
            entrypoint: entrypoint.to_string(),
            transport: "stdio-framed-json-v1".to_string(),
            host_mediated: true,
            operations: vec![
                "surface.state".to_string(),
                "surface.action".to_string(),
                "surface.host-action".to_string(),
            ],
            host_operations: vec!["blob.read".to_string(), "event.publish".to_string()],
            provider_egress: Vec::new(),
        }
    }

    #[test]
    fn worker_descriptor_rejects_path_and_allowlist_escalation() {
        validate_worker_descriptor(&python_worker_descriptor("src/worker.py"))
            .expect("canonical worker descriptor");
        for entrypoint in [
            "../worker.py",
            "/tmp/worker.py",
            "src\\worker.py",
            "src/./worker.py",
        ] {
            assert!(
                validate_worker_descriptor(&python_worker_descriptor(entrypoint)).is_err(),
                "unsafe entrypoint must fail: {entrypoint}"
            );
        }
        let mut duplicate = python_worker_descriptor("src/worker.py");
        duplicate.operations.push("surface.state".to_string());
        assert!(validate_worker_descriptor(&duplicate).is_err());
        let mut invalid_host_operation = python_worker_descriptor("src/worker.py");
        invalid_host_operation
            .host_operations
            .push("Principal.Inject".to_string());
        assert!(validate_worker_descriptor(&invalid_host_operation).is_err());
    }

    #[test]
    fn installed_worker_entrypoint_must_resolve_inside_plugin_root() {
        let root = tempdir().expect("plugin root");
        fs::create_dir_all(root.path().join("src")).expect("worker directory");
        fs::write(root.path().join("src/worker.py"), b"print('ok')").expect("worker file");
        validate_worker_entrypoint(root.path(), &python_worker_descriptor("src/worker.py"))
            .expect("ordinary packaged worker entrypoint");

        let outside = tempdir().expect("outside root");
        let outside_worker = outside.path().join("worker.py");
        fs::write(&outside_worker, b"print('escape')").expect("outside worker");
        let link = root.path().join("src/linked.py");
        #[cfg(unix)]
        let linked = std::os::unix::fs::symlink(&outside_worker, &link).is_ok();
        #[cfg(windows)]
        let linked = std::os::windows::fs::symlink_file(&outside_worker, &link).is_ok();
        if linked {
            assert!(validate_worker_entrypoint(
                root.path(),
                &python_worker_descriptor("src/linked.py")
            )
            .expect_err("symlink escape must fail")
            .contains("escapes"));
        }
    }

    #[test]
    fn parses_sdk_environment_api_key_source() {
        for source in ["environment", "Environment"] {
            let json = format!("{{\"source\":\"{source}\",\"name\":\"DEEPSEEK_API_KEY\"}}");
            let parsed: PluginApiKeySource =
                serde_json::from_str(&json).expect("parse environment API key source");
            assert!(matches!(parsed, PluginApiKeySource::Environment { .. }));
        }
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
            serde_json::from_str(&runtime_manifest("rust")).expect("parse runtime manifest");
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
            ui: None,
            ui_ir: None,
        });
        InstalledMycPlugin {
            manifest,
            install_path: "test".to_string(),
            theme: None,
            icon_theme: None,
            edge_style: None,
            runtime: Some(MycPluginRuntime {
                engine: "wasm32-myc".to_string(),
                language: "rust".to_string(),
                entry_sha256: "0".repeat(64),
            }),
            frontend: None,
            locales: None,
            private_i18n: None,
            workspace: None,
            provider: None,
            agent: None,
            ui_ir_contributions: None,
        }
    }

    fn manifest_with_ui_reference(source: &str, artifact: &str) -> MycPluginManifest {
        let mut manifest: MycPluginManifest =
            serde_json::from_str(&runtime_manifest("rust")).expect("parse runtime manifest");
        manifest.spec.contributes = Some(MycPluginContributions {
            context_menus: None,
            locales: None,
            commands: None,
            ui: None,
            ui_ir: Some(json!([{
                "slotId": "agent.review",
                "source": source,
                "artifact": artifact,
            }])),
        });
        manifest
    }

    #[test]
    fn ui_ir_installation_rejects_missing_tampered_escaping_and_oversized_artifacts() {
        let root = tempdir().expect("ui ir root");
        fs::create_dir_all(root.path().join("ui")).expect("ui directory");
        fs::write(root.path().join("ui/Surface.vue"), "<template />").expect("source");

        let missing = manifest_with_ui_reference("ui/Surface.vue", "ui/Missing.uiir.json");
        assert!(read_ui_ir_contributions(root.path(), &missing)
            .expect_err("missing artifact must fail")
            .contains("Could not read UI IR artifact"));

        let escaping = manifest_with_ui_reference("ui/../outside.vue", "ui/Surface.uiir.json");
        assert!(read_ui_ir_contributions(root.path(), &escaping)
            .expect_err("escaping source must fail")
            .contains("ui/<name>.vue"));

        fs::write(
            root.path().join("ui/Surface.uiir.json"),
            r#"{"apiVersion":"anyway.dev/ui-ir/v1","root":[]}"#,
        )
        .expect("tampered artifact");
        let tampered = manifest_with_ui_reference("ui/Surface.vue", "ui/Surface.uiir.json");
        assert!(read_ui_ir_contributions(root.path(), &tampered)
            .expect_err("invalid document shape must fail")
            .contains("invalid document shape"));

        fs::write(
            root.path().join("ui/Surface.uiir.json"),
            vec![b' '; (MAX_UI_IR_ARTIFACT_BYTES + 1) as usize],
        )
        .expect("oversized artifact");
        assert!(read_ui_ir_contributions(root.path(), &tampered)
            .expect_err("oversized artifact must fail")
            .contains("exceeds"));
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
    fn trusted_host_settings_replace_frontend_host_fields() {
        let input = json!({
            "apiVersion": PLUGIN_CALL_API_VERSION,
            "operation": "self-test",
            "host": {
                "pluginId": "attacker.plugin",
                "settings": {"api-key": "attacker-secret"}
            },
            "payload": {"value": 1}
        });
        let trusted_settings = json!({
            "effectiveValues": {"model": "luna"},
            "secretConfigured": {"api-key": true},
            "secrets": {"api-key": "host-secret"}
        });
        let sanitized =
            inject_trusted_host_settings(&input, "myc.runtime-smoke", "1.0.0", trusted_settings)
                .expect("host settings are injected");

        assert_eq!(sanitized["host"]["pluginId"], "myc.runtime-smoke");
        assert_eq!(sanitized["host"]["pluginVersion"], "1.0.0");
        assert!(sanitized["host"]["settings"].get("secrets").is_none());
        assert!(!serde_json::to_string(&sanitized)
            .expect("serialize sanitized input")
            .contains("attacker-secret"));
        assert!(!serde_json::to_string(&sanitized)
            .expect("serialize sanitized input")
            .contains("host-secret"));
        assert_eq!(sanitized["payload"]["value"], 1);
    }

    #[test]
    fn validates_optional_developer_uuid_without_breaking_legacy_metadata() {
        let mut manifest: MycPluginManifest =
            serde_json::from_str(&runtime_manifest("rust")).expect("parse runtime manifest");
        validate_manifest(&manifest).expect("legacy developer field remains valid");
        manifest.metadata.developer_uuid = Some("550e8400-e29b-41d4-a716-446655440000".to_string());
        validate_manifest(&manifest).expect("canonical developer UUID is valid");
        manifest.metadata.developer_uuid = Some("not-a-uuid".to_string());
        assert!(validate_manifest(&manifest)
            .expect_err("invalid developer UUID is rejected")
            .contains("Developer UUID"));
    }

    #[test]
    fn installs_and_executes_a_runtime_myc_package() {
        let root = tempdir().expect("temp root");
        let package = root.path().join("runtime-smoke.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.json", options)
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
            .start_file("plugin.json", options)
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

        // 模拟已安装包同时存在墓碑的异常状态；重复安装 no-op 不得清除它。
        // Simulate a tombstone for an already-installed package; a no-op repeat
        // install must not clear it.
        let mut removed = HashSet::new();
        removed.insert("myc.runtime-smoke@1.0.0".to_string());
        write_removed_plugins(root.path(), &removed).expect("write tombstone");

        install_archive_into(root.path(), &package).expect("repeat install is no-op");
        assert!(read_removed_plugins(root.path())
            .expect("removal tombstones")
            .contains("myc.runtime-smoke@1.0.0"));
    }

    #[test]
    fn rejects_runtime_manifest_with_unknown_language() {
        let cpp: MycPluginManifest =
            serde_json::from_str(&runtime_manifest("cpp")).expect("parse cpp manifest");
        validate_manifest(&cpp).expect("C++ wasm plugins use the same verified ABI");

        let manifest: MycPluginManifest =
            serde_json::from_str(&runtime_manifest("javascript")).expect("parse manifest");
        assert!(validate_manifest(&manifest)
            .expect_err("unknown language rejected")
            .contains("language"));
    }

    #[test]
    fn context_menu_contributions_require_runtime_capability() {
        let mut manifest: MycPluginManifest =
            serde_json::from_str(&runtime_manifest("rust")).expect("parse manifest");
        manifest.spec.contributes = Some(MycPluginContributions {
            context_menus: Some(vec![PluginContextMenuContribution {
                id: "inspect-context".to_string(),
                scope: "node".to_string(),
                label: "Analyze node context".to_string(),
                icon: Some("sparkles".to_string()),
            }]),
            locales: None,
            commands: None,
            ui: None,
            ui_ir: None,
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
        archive
            .start_file("plugin.json", options)
            .expect("manifest");
        archive
            .write_all(
                br#"{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"WorkspacePlugin","metadata":{"id":"myc.test-export","name":"Test Export","version":"1.0.0","publisher":"Research Canvas","developer":"Workspace Tests","description":"Test host mediated export capability."},"spec":{"engine":"host-mediated","entry":"workspace-plugin.json","capabilities":["project.export"],"permissions":[],"contributes":{"commands":[{"id":"export","label":"Export SVG","description":"Export the reviewed project.","category":"export","capability":"project.export","formats":["svg"]}]}}}"#,
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
        archive
            .start_file("plugin.json", options)
            .expect("manifest");
        archive
            .write_all(
                "{\"apiVersion\":\"researchcanvas.dev/v1alpha1\",\"kind\":\"LocalePlugin\",\"metadata\":{\"id\":\"myc.test-ja\",\"name\":\"Test Japanese\",\"version\":\"1.0.0\",\"publisher\":\"Research Canvas\",\"developer\":\"Locale Tests\",\"description\":\"Test declarative community language.\"},\"spec\":{\"engine\":\"declarative\",\"entry\":\"locales/ja-JP.json\",\"capabilities\":[\"i18n.register\"],\"permissions\":[],\"contributes\":{\"locales\":[{\"locale\":\"ja-JP\",\"name\":\"日本語\",\"path\":\"locales/ja-JP.json\"}]}}}"
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
        archive
            .start_file("plugin.json", options)
            .expect("manifest");
        archive
            .write_all(
                br#"{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"AgentPlugin","metadata":{"id":"myc.test-agent","name":"Test Agent","version":"0.1.0","publisher":"Research Canvas","developer":"Agent Tests","description":"Test host-mediated review-gated agent."},"spec":{"engine":"host-mediated","entry":"agent-manifest.json","capabilities":["agent.pdf.read","agent.graph.patch.propose","graph.patch.propose","plugin.files.pick","agent.review.request"],"permissions":[]}}"#,
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

        require_installed_plugin_capabilities_from(
            root.path(),
            "myc.test-agent",
            "0.1.0",
            &["plugin.files.pick", "graph.patch.propose"],
        )
        .expect("generic Host capabilities accept an installed AgentPlugin");
        let missing = require_installed_plugin_capabilities_from(
            root.path(),
            "myc.test-agent",
            "0.1.0",
            &["project.folder"],
        )
        .expect_err("undeclared generic capability must be rejected");
        assert!(missing.contains("does not declare project.folder"));
        let workspace_only = require_workspace_plugin_capabilities_from(
            root.path(),
            "myc.test-agent",
            "0.1.0",
            &["plugin.files.pick"],
        )
        .expect_err("Workspace actions must still reject AgentPlugin");
        assert_eq!(
            workspace_only,
            "Native workspace actions require WorkspacePlugin"
        );

        // 非审阅门控的 agent 描述符必须被拒绝 / Non-review-gated descriptors are rejected.
        let rogue_package = root.path().join("rogue-agent.myc");
        let file = File::create(&rogue_package).expect("rogue archive");
        let mut archive = ZipWriter::new(file);
        archive
            .start_file("plugin.json", options)
            .expect("manifest");
        archive
            .write_all(
                br#"{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"AgentPlugin","metadata":{"id":"myc.rogue-agent","name":"Rogue Agent","version":"0.1.0","publisher":"Research Canvas","developer":"Agent Tests","description":"Agent descriptor that is not review-gated."},"spec":{"engine":"host-mediated","entry":"agent-manifest.json","capabilities":["agent.graph.patch.propose"],"permissions":[]}}"#,
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
        archive
            .start_file("plugin.json", options)
            .expect("manifest");
        archive
            .write_all(
                br#"{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"AgentPlugin","metadata":{"id":"myc.unknown-agent","name":"Unknown Capability Agent","version":"0.1.0","publisher":"Research Canvas","developer":"Agent Tests","description":"Agent declaring an unknown capability."},"spec":{"engine":"host-mediated","entry":"agent-manifest.json","capabilities":["agent.filesystem.write"],"permissions":[]}}"#,
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
        fs::write(packages.join("aaa.corrupt@1.0.0.myc"), b"not a zip").expect("corrupt package");

        // 好包的文件名故意与 manifest id@version 不一致:
        // 发现、去重、墓碑都必须按 manifest 身份而不是文件名。
        // The valid package filename deliberately differs from its manifest
        // id@version: discovery, dedupe, and tombstones all key on identity.
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let valid_package = packages.join("zzz.renamed-file@9.9.9.myc");
        let file = File::create(&valid_package).expect("valid archive");
        let mut archive = ZipWriter::new(file);
        archive
            .start_file("plugin.json", options)
            .expect("manifest");
        archive
            .write_all(
                br#"{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"ThemePlugin","metadata":{"id":"myc.valid-theme","name":"Valid Theme","version":"1.0.0","publisher":"Research Canvas","developer":"Tests","description":"A valid theme package."},"spec":{"engine":"declarative","entry":"theme.json","capabilities":["theme.register"],"permissions":[]}}"#,
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
            !root
                .path()
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
            !root2
                .path()
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
        let mut manifest_value = serde_json::json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": format!("{publisher}.test-theme"),
                "name": "Signed Theme",
                "version": "1.0.0",
                "publisher": publisher,
                "developer": "Test",
                "description": "A signed theme plugin.",
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "capabilities": ["theme.register"],
                "permissions": [],
            },
        });
        if let Some(sign) = sign_fn {
            // Signed manifests must declare payloads; the hash matches valid_theme_json().
            manifest_value["payloads"] = serde_json::json!({ "theme.json": theme_payload_hash() });
            // Signature covers the canonical (sorted-key, compact) JSON without
            // `signature` — the same bytes the verifier recomputes from the raw
            // archived manifest text.
            let payload =
                crate::signing::manifest_payload(&manifest_value).expect("manifest payload");
            let signature_b64 = sign(&BASE64.encode(&payload));
            manifest_value["signature"] = serde_json::Value::String(signature_b64);
        }
        serde_json::to_string(&manifest_value).expect("manifest json")
    }

    #[test]
    fn accepts_signed_plugin_with_trusted_key() {
        let root = tempdir().expect("temp root");

        let signing_key = SigningKey::generate(&mut rand_core::OsRng);
        let verifying_key = signing_key.verifying_key();
        let pubkey_b64 = BASE64.encode(verifying_key.as_bytes());
        let publisher = "trusted-publisher";

        let trusted_json = serde_json::json!({ publisher: pubkey_b64 }).to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json).expect("write trusted keys");

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
            .start_file("plugin.json", options)
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

        let trusted_json = serde_json::json!({ publisher: pubkey_b64_b }).to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json).expect("write trusted keys");

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
            .start_file("plugin.json", options)
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
            .start_file("plugin.json", options)
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

        let manifest_yaml =
            signed_theme_manifest("unsigned-publisher", None::<&dyn Fn(&str) -> String>);
        let theme_json = valid_theme_json().to_string();

        let package = root.path().join("unsigned.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.json", options)
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

        let trusted_json = serde_json::json!({ publisher: pubkey_b64 }).to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json).expect("write trusted keys");

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
            r#"{{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"ThemePlugin","metadata":{{"id":"{publisher}.evil-theme","name":"Evil Theme","version":"9.9.9","publisher":"{publisher}","developer":"Evil Dev","description":"Tampered malicious plugin."}},"spec":{{"engine":"declarative","entry":"theme.json","capabilities":["theme.register"],"permissions":[]}},"payloads":{{"theme.json":"{theme_hash}"}},"signature":"{signature_b64}"}}"#
        );

        let package = root.path().join("tampered.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.json", options)
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
            r#"{{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"ThemePlugin","metadata":{{"id":"myc.payload-theme","name":"Payload Theme","version":"1.0.0","publisher":"Research Canvas","developer":"Tests","description":"Theme with declared payloads."}},"spec":{{"engine":"declarative","entry":"theme.json","capabilities":["theme.register"],"permissions":[]}},"payloads":{payloads_yaml}}}"#
        );
        archive
            .start_file("plugin.json", options)
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
        let good = format!("{{\"theme.json\":\"{}\"}}", theme_payload_hash());
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
        assert!(
            error.contains("Unlisted payload"),
            "unexpected error: {error}"
        );

        // 清单列出但包内缺失 → 拒绝 / Listed but missing → reject.
        let root4 = tempdir().expect("fourth root");
        let missing = format!(
            "{{\"theme.json\":\"{}\",\"missing.txt\":\"{}\"}}",
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
        let trusted_json =
            serde_json::json!({ publisher: BASE64.encode(verifying_key.as_bytes()) }).to_string();
        fs::write(root.path().join("trusted-keys.json"), trusted_json).expect("write trusted keys");

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
            r#"{{"apiVersion":"researchcanvas.dev/v1alpha1","kind":"ThemePlugin","metadata":{{"id":"{publisher}.test-theme","name":"Payloadless Theme","version":"1.0.0","publisher":"{publisher}","developer":"Test","description":"Signed but payloadless."}},"spec":{{"engine":"declarative","entry":"theme.json","capabilities":["theme.register"],"permissions":[]}},"signature":"{}"}}"#,
            BASE64.encode(signature.to_bytes())
        );

        let package = root.path().join("payloadless.myc");
        let file = File::create(&package).expect("create archive");
        let mut archive = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        archive
            .start_file("plugin.json", options)
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
    fn external_myc_package_is_staged_and_installed() {
        let base = tempdir().expect("temp base");
        let external = tempdir().expect("external root");
        let theme_json = valid_theme_json().to_string();
        let package = theme_package_with_payloads(
            external.path(),
            "external.myc",
            &format!("{{\"theme.json\":\"{}\"}}", theme_payload_hash()),
            theme_json.as_bytes(),
            None,
        );

        let staged =
            resolve_package_path(base.path(), &package).expect("external package should be staged");
        let packages = base
            .path()
            .join("packages")
            .canonicalize()
            .expect("packages");
        assert!(
            staged.starts_with(&packages),
            "staged package must be inside packages"
        );
        assert_ne!(staged, package.canonicalize().expect("source path"));
        assert_eq!(
            fs::read(&staged).expect("staged bytes"),
            fs::read(&package).expect("source bytes")
        );

        let installed = install_archive_into(base.path(), &staged)
            .expect("staged external package should install");
        assert_eq!(installed.manifest.metadata.id, "myc.payload-theme");
        assert!(base
            .path()
            .join("installed/myc.payload-theme@1.0.0")
            .is_dir());
    }

    #[test]
    fn escaping_external_path_is_staged_without_trusting_or_overwriting_destination() {
        let base = tempdir().expect("temp base");
        let packages = base.path().join("packages");
        fs::create_dir_all(&packages).expect("create packages");
        let inside = packages.join("inside.myc");
        fs::write(&inside, b"dummy").expect("write inside");

        assert!(
            resolve_package_path(base.path(), &inside).is_ok(),
            "path inside packages must be allowed"
        );

        let outside = base.path().join("outside");
        fs::create_dir_all(&outside).expect("create outside");
        let escaped_source = outside.join("escaped.myc");
        fs::write(&escaped_source, b"external package").expect("write outside");
        let existing_destination = packages.join("escaped.myc");
        fs::write(&existing_destination, b"keep me").expect("write destination sentinel");

        let escaped = packages.join("..").join("outside").join("escaped.myc");
        let staged = resolve_package_path(base.path(), &escaped)
            .expect("escaping external path should be staged safely");
        assert!(staged.starts_with(&packages.canonicalize().expect("packages")));
        assert_ne!(staged, existing_destination);
        assert_eq!(
            fs::read(&existing_destination).expect("sentinel bytes"),
            b"keep me"
        );
        assert_eq!(
            fs::read(&staged).expect("staged bytes"),
            b"external package"
        );
    }

    #[test]
    fn official_flag_is_reserved_for_the_research_canvas_publisher() {
        let spoof = json!({
            "name": "evil.agent",
            "version": "1.0.0",
            "publisher": "Random",
            "official": true,
            "categories": ["AgentPlugin"],
            "main": "agent-manifest.json",
            "engines": {"engine": "host-mediated"},
            "capabilities": ["agent.pdf.read", "agent.graph.patch.propose", "agent.review.request"]
        })
        .to_string();
        let manifest = crate::plugin_manifest_v2::parse_plugin_manifest(&spoof).expect("parses v2");
        let error =
            validate_manifest(&manifest).expect_err("a spoofed official flag must be rejected");
        assert!(
            error.contains("ResearchCanvas"),
            "unexpected error: {error}"
        );

        let official = json!({
            "name": "myc.pdf-canvas-agent",
            "version": "0.4.0",
            "publisher": "ResearchCanvas",
            "official": true,
            "categories": ["AgentPlugin"],
            "main": "agent-manifest.json",
            "engines": {"engine": "host-mediated"},
            "capabilities": ["agent.pdf.read", "agent.graph.patch.propose", "agent.review.request"]
        })
        .to_string();
        let manifest =
            crate::plugin_manifest_v2::parse_plugin_manifest(&official).expect("parses v2");
        validate_manifest(&manifest).expect("the official publisher is accepted");
    }

    #[test]
    fn desktop_dev_allowlist_is_explicit_and_excludes_local_third_party_packages() {
        let allowlist = desktop_dev_allowlist().expect("tracked plugin-loading config");
        assert_eq!(allowlist.len(), 4);
        assert!(allowlist.contains("myc.circuit-orthogonal"));
        assert!(allowlist.contains("myc.export-suite"));
        assert!(allowlist.contains("myc.folder-workspaces"));
        assert!(allowlist.contains("myc.git-workspace"));
        assert!(!allowlist.contains("myc.pdf-canvas-agent"));
        assert!(!allowlist.contains("myc.i18n-ja"));
        assert!(!allowlist.contains("myc.onedarkpro"));
        assert!(!allowlist.contains("myc.runtime-smoke"));
    }

    /// 官方 release 包必须走完整的 v2 安装管线:
    /// JSON 清单解析 → 校验 → 解压 → payloads 哈希核验 → 身份比对。
    /// Every official release .myc package in the repository must pass the full v2
    /// install pipeline: JSON manifest parse → validation → extraction →
    /// payload hash verification → identity comparison.
    #[test]
    fn tracked_packages_pass_the_full_v2_install_pipeline() {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root")
            .join("plugins/packages");
        let mut packages: Vec<PathBuf> = fs::read_dir(&repository)
            .expect("packages directory")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("myc"))
            .collect();
        packages.sort();
        assert!(
            packages.len() >= 6,
            "expected the official packages, found {}",
            packages.len()
        );

        for package in packages {
            let base = tempdir().expect("temp base");
            let installed = install_archive_into(base.path(), &package)
                .unwrap_or_else(|error| panic!("{} failed to install: {error}", package.display()));
            assert_eq!(
                installed.manifest.metadata.id,
                package
                    .file_stem()
                    .expect("package stem")
                    .to_string_lossy()
                    .rsplit_once('@')
                    .map(|(id, _)| id)
                    .expect("id before @"),
                "installed id must match the package filename"
            );
            assert!(
                base.path()
                    .join(format!(
                        "installed/{}@{}",
                        installed.manifest.metadata.id, installed.manifest.metadata.version
                    ))
                    .is_dir(),
                "{} must be installed atomically",
                package.display()
            );
        }
    }
}
