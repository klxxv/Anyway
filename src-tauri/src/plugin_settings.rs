//! Host-owned plugin settings.
//!
//! Public setting overrides are persisted in a dedicated JSON file below the
//! Tauri app config directory. Secret settings intentionally never enter that
//! file: until a platform credential store is available, they are kept only
//! in this process and are exposed to the settings read API as configured
//! flags. Execution receives only non-secret effective values and configured
//! flags. A future host-internal model gateway may consume a credential, but
//! no plugin or guest execution envelope can read it.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager};

use crate::plugins::{
    MycPluginManifest, PluginApiKeySource, PluginSettingDefinition, PluginWorkerDescriptor,
};

const SETTINGS_SCHEMA_VERSION: u32 = 1;
const SETTINGS_DIRECTORY: &str = "plugin-settings";
const SETTINGS_FILE: &str = "settings.json";
const MAX_TEXT_LENGTH: usize = 8 * 1024;
const MAX_SECRET_LENGTH: usize = 8 * 1024;
const FLOAT_EPSILON: f64 = 1e-9;

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct PersistedPluginSettings {
    #[serde(default)]
    values: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct PersistedSettingsFile {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    plugins: BTreeMap<String, PersistedPluginSettings>,
}

fn default_schema_version() -> u32 {
    SETTINGS_SCHEMA_VERSION
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginSettingsSnapshot {
    pub plugin_id: String,
    pub plugin_version: String,
    pub definitions: Vec<PluginSettingDefinition>,
    pub effective_values: BTreeMap<String, Value>,
    pub overrides: BTreeMap<String, Value>,
    pub secret_configured: BTreeMap<String, bool>,
}

fn secret_values() -> &'static Mutex<HashMap<String, BTreeMap<String, String>>> {
    static VALUES: OnceLock<Mutex<HashMap<String, BTreeMap<String, String>>>> = OnceLock::new();
    VALUES.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn validate_definitions(settings: &[PluginSettingDefinition]) -> Result<(), String> {
    if settings.len() > 32 {
        return Err("A plugin can declare at most 32 settings".to_string());
    }

    let mut ids = HashSet::with_capacity(settings.len());
    for setting in settings {
        validate_slug(&setting.id, "plugin setting id")?;
        if !ids.insert(&setting.id) {
            return Err(format!("Duplicate plugin setting id: {}", setting.id));
        }
        if setting.label.trim().is_empty() || setting.label.chars().count() > 64 {
            return Err(format!(
                "Plugin setting labels must contain 1 to 64 characters: {}",
                setting.id
            ));
        }
        if setting
            .description
            .as_ref()
            .is_some_and(|description| description.chars().count() > 180)
        {
            return Err(format!(
                "Plugin setting descriptions must be at most 180 characters: {}",
                setting.id
            ));
        }
        if setting
            .placeholder
            .as_ref()
            .is_some_and(|placeholder| placeholder.chars().count() > 160)
        {
            return Err(format!(
                "Plugin setting placeholders must be at most 160 characters: {}",
                setting.id
            ));
        }
        if setting
            .group
            .as_ref()
            .is_some_and(|group| group.trim().is_empty() || group.chars().count() > 64)
        {
            return Err(format!(
                "Plugin setting groups must contain 1 to 64 characters: {}",
                setting.id
            ));
        }
        if setting.secret && setting.setting_type != "text" {
            return Err(format!(
                "Secret plugin settings must use type text: {}",
                setting.id
            ));
        }
        if setting.secret && setting.default.is_some() {
            return Err(format!(
                "Secret plugin settings must not declare a default: {}",
                setting.id
            ));
        }
        if setting.min.is_some_and(|value| !value.is_finite())
            || setting.max.is_some_and(|value| !value.is_finite())
            || setting
                .step
                .is_some_and(|value| !value.is_finite() || value <= 0.0)
            || setting
                .min
                .zip(setting.max)
                .is_some_and(|(min, max)| min > max)
        {
            return Err(format!(
                "Invalid numeric bounds for plugin setting: {}",
                setting.id
            ));
        }

        if setting.setting_type != "number"
            && (setting.min.is_some() || setting.max.is_some() || setting.step.is_some())
        {
            return Err(format!(
                "Only number settings may declare min, max, or step: {}",
                setting.id
            ));
        }

        match setting.setting_type.as_str() {
            "boolean" | "text" | "number" => {
                if setting
                    .options
                    .as_ref()
                    .is_some_and(|options| !options.is_empty())
                {
                    return Err(format!(
                        "Only select settings may declare options: {}",
                        setting.id
                    ));
                }
            }
            "select" => {
                let options = setting.options.as_ref().ok_or_else(|| {
                    format!("Select setting must declare options: {}", setting.id)
                })?;
                if options.is_empty() || options.len() > 32 {
                    return Err(format!(
                        "Select setting options must contain 1 to 32 entries: {}",
                        setting.id
                    ));
                }
                let mut values = HashSet::with_capacity(options.len());
                for option in options {
                    if option.value.trim().is_empty()
                        || option.value.chars().count() > 96
                        || option.label.trim().is_empty()
                        || option.label.chars().count() > 64
                        || !values.insert(&option.value)
                    {
                        return Err(format!("Invalid select option: {}", setting.id));
                    }
                }
            }
            _ => {
                return Err(format!(
                    "Unsupported plugin setting type: {}",
                    setting.setting_type
                ));
            }
        }

        if let Some(default) = setting.default.as_ref() {
            validate_value(setting, default)?;
        }
    }
    Ok(())
}

pub(crate) fn validate_connections(manifest: &MycPluginManifest) -> Result<(), String> {
    let connections = manifest.spec.connections.as_deref().unwrap_or_default();
    if connections.len() > 8 {
        return Err("A plugin can declare at most 8 connections".to_string());
    }
    let definitions = definition_map(manifest)?;
    let mut ids = HashSet::with_capacity(connections.len());
    for connection in connections {
        validate_slug(&connection.id, "plugin connection id")?;
        if !ids.insert(&connection.id) {
            return Err(format!("Duplicate plugin connection id: {}", connection.id));
        }
        if connection.label.trim().is_empty() || connection.label.chars().count() > 64 {
            return Err(format!(
                "Plugin connection labels must contain 1 to 64 characters: {}",
                connection.id
            ));
        }
        let url_setting = definitions
            .get(connection.url_setting_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Connection URL setting is not declared: {}",
                    connection.url_setting_id
                )
            })?;
        if url_setting.setting_type != "text" {
            return Err(format!(
                "Connection URL setting must be text: {}",
                connection.id
            ));
        }
        let format_setting = definitions
            .get(connection.format_setting_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Connection format setting is not declared: {}",
                    connection.format_setting_id
                )
            })?;
        if format_setting.setting_type != "select"
            || !format_setting.options.as_ref().is_some_and(|options| {
                options.iter().any(|option| option.value == "openai")
                    && options.iter().any(|option| option.value == "anthropic")
            })
        {
            return Err(format!(
                "Connection format setting must offer openai and anthropic: {}",
                connection.id
            ));
        }
        if let Some(model_setting_id) = connection.model_setting_id.as_deref() {
            let model_setting = definitions.get(model_setting_id).ok_or_else(|| {
                format!("Connection model setting is not declared: {model_setting_id}")
            })?;
            if model_setting.setting_type != "text" && model_setting.setting_type != "select" {
                return Err(format!(
                    "Connection model setting must be text or select: {model_setting_id}"
                ));
            }
        }
        if let Some(source_setting_id) = connection.credential_source_setting_id.as_deref() {
            let source_setting = definitions.get(source_setting_id).ok_or_else(|| {
                format!("Connection credential source setting is not declared: {source_setting_id}")
            })?;
            if source_setting.setting_type != "select"
                || !source_setting.options.as_ref().is_some_and(|options| {
                    options.iter().any(|option| option.value == "host-secret")
                        && options.iter().any(|option| option.value == "environment")
                })
            {
                return Err(format!("Connection credential source setting must offer host-secret and environment: {source_setting_id}"));
            }
        }
        if let Some(env_setting_id) = connection.credential_env_var_setting_id.as_deref() {
            let env_setting = definitions.get(env_setting_id).ok_or_else(|| {
                format!(
                    "Connection credential environment setting is not declared: {env_setting_id}"
                )
            })?;
            if env_setting.setting_type != "text" {
                return Err(format!(
                    "Connection credential environment setting must be text: {env_setting_id}"
                ));
            }
        }
        match &connection.api_key {
            crate::plugins::PluginApiKeySource::HostSecret { setting_id } => {
                let setting = definitions.get(setting_id.as_str()).ok_or_else(|| {
                    format!("Connection secret setting is not declared: {setting_id}")
                })?;
                if !setting.secret || setting.setting_type != "text" {
                    return Err(format!(
                        "Connection secret setting must be a secret text field: {setting_id}"
                    ));
                }
            }
            crate::plugins::PluginApiKeySource::Environment {
                name,
                fallback_setting_id,
            } => {
                if name.is_empty()
                    || name.len() > 128
                    || !name.chars().enumerate().all(|(index, character)| {
                        (index == 0 && (character.is_ascii_uppercase() || character == '_'))
                            || (index > 0
                                && (character.is_ascii_uppercase()
                                    || character.is_ascii_digit()
                                    || character == '_'))
                    })
                {
                    return Err(format!(
                        "Invalid connection environment variable name: {name}"
                    ));
                }
                if let Some(setting_id) = fallback_setting_id {
                    let setting = definitions.get(setting_id.as_str()).ok_or_else(|| {
                        format!("Connection fallback secret setting is not declared: {setting_id}")
                    })?;
                    if !setting.secret || setting.setting_type != "text" {
                        return Err(format!(
                            "Connection fallback secret must be a secret text field: {setting_id}"
                        ));
                    }
                }
            }
        }
        if let Some(action) = &connection.test_action {
            validate_slug(&action.id, "connection test action id")?;
            if action.label.trim().is_empty() || action.label.chars().count() > 64 {
                return Err(format!(
                    "Connection test action label is invalid: {}",
                    connection.id
                ));
            }
            if action
                .description
                .as_ref()
                .is_some_and(|description| description.chars().count() > 180)
            {
                return Err(format!(
                    "Connection test action description is too long: {}",
                    connection.id
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_value(
    setting: &PluginSettingDefinition,
    value: &Value,
) -> Result<(), String> {
    let valid_type = match setting.setting_type.as_str() {
        "boolean" => value.is_boolean(),
        "number" => value.as_f64().is_some_and(f64::is_finite),
        "text" => value
            .as_str()
            .is_some_and(|text| text.chars().count() <= MAX_TEXT_LENGTH),
        "select" => value.as_str().is_some_and(|selected| {
            setting
                .options
                .as_ref()
                .is_some_and(|options| options.iter().any(|option| option.value == selected))
        }),
        _ => false,
    };
    if !valid_type {
        return Err(format!("Invalid value for plugin setting: {}", setting.id));
    }

    if setting.setting_type == "number" {
        let number = value.as_f64().expect("number type was checked above");
        if setting
            .min
            .is_some_and(|minimum| number < minimum - FLOAT_EPSILON)
            || setting
                .max
                .is_some_and(|maximum| number > maximum + FLOAT_EPSILON)
        {
            return Err(format!(
                "Value is outside the range for plugin setting: {}",
                setting.id
            ));
        }
        if let Some(step) = setting.step {
            let base = setting.min.unwrap_or(0.0);
            let steps = (number - base) / step;
            if (steps - steps.round()).abs() > FLOAT_EPSILON * steps.abs().max(1.0) {
                return Err(format!(
                    "Value does not match step for plugin setting: {}",
                    setting.id
                ));
            }
        }
    }

    if setting.secret
        && value.as_str().is_some_and(|secret| {
            secret.trim().is_empty() || secret.chars().count() > MAX_SECRET_LENGTH
        })
    {
        return Err(format!(
            "Secret value cannot be empty or too long: {}",
            setting.id
        ));
    }
    Ok(())
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

fn settings_paths(app: &AppHandle) -> Result<(PathBuf, PathBuf), String> {
    let app_config = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("Cannot resolve app config directory: {error}"))?;
    let directory = app_config.join(SETTINGS_DIRECTORY);
    let file = directory.join(SETTINGS_FILE);
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Cannot create plugin settings directory: {error}"))?;

    let canonical_root = fs::canonicalize(&app_config).unwrap_or(app_config.clone());
    let canonical_directory = fs::canonicalize(&directory)
        .map_err(|error| format!("Cannot resolve plugin settings directory: {error}"))?;
    if !canonical_directory.starts_with(&canonical_root)
        || file.file_name().and_then(|name| name.to_str()) != Some(SETTINGS_FILE)
        || fs::symlink_metadata(&file)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    {
        return Err("Plugin settings path escaped the app config directory".to_string());
    }
    Ok((canonical_directory, file))
}

fn read_persisted_file(path: &Path) -> Result<PersistedSettingsFile, String> {
    if !path.is_file() {
        return Ok(PersistedSettingsFile {
            schema_version: SETTINGS_SCHEMA_VERSION,
            plugins: BTreeMap::new(),
        });
    }
    let bytes = fs::read(path).map_err(|error| format!("Cannot read plugin settings: {error}"))?;
    let document: PersistedSettingsFile = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Cannot parse plugin settings: {error}"))?;
    if document.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported plugin settings schema version: {}",
            document.schema_version
        ));
    }
    Ok(document)
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source_wide: Vec<u16> = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target_wide: Vec<u16> = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        MoveFileExW(
            PCWSTR(source_wide.as_ptr()),
            PCWSTR(target_wide.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(|error| format!("Cannot atomically replace plugin settings: {error}"))
    }
}

#[cfg(not(windows))]
fn replace_file_atomically(source: &Path, target: &Path) -> Result<(), String> {
    fs::rename(source, target)
        .map_err(|error| format!("Cannot atomically replace plugin settings: {error}"))
}

fn write_persisted_file(
    directory: &Path,
    path: &Path,
    document: &PersistedSettingsFile,
) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("Cannot serialize plugin settings: {error}"))?;
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let temporary = directory.join(format!(
        ".{SETTINGS_FILE}.{}.{}.tmp",
        std::process::id(),
        suffix
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("Cannot create temporary plugin settings: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("Cannot write temporary plugin settings: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Cannot sync temporary plugin settings: {error}"))?;
        drop(file);
        replace_file_atomically(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn definition_map<'a>(
    manifest: &'a MycPluginManifest,
) -> Result<BTreeMap<&'a str, &'a PluginSettingDefinition>, String> {
    let settings = manifest.spec.settings.as_deref().unwrap_or_default();
    validate_definitions(settings)?;
    Ok(settings
        .iter()
        .map(|setting| (setting.id.as_str(), setting))
        .collect())
}

fn normalized_public_values(
    manifest: &MycPluginManifest,
    values: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, Value>, String> {
    let definitions = definition_map(manifest)?;
    let mut normalized = BTreeMap::new();
    for (id, value) in values {
        let Some(setting) = definitions.get(id.as_str()) else {
            continue;
        };
        if setting.secret {
            continue;
        }
        if validate_value(setting, value).is_ok() {
            normalized.insert(id.clone(), value.clone());
        }
    }
    Ok(normalized)
}

fn configured_secrets(plugin_key: &str) -> Result<BTreeMap<String, String>, String> {
    secret_values()
        .lock()
        .map_err(|error| format!("Plugin secret settings lock poisoned: {error}"))
        .map(|values| values.get(plugin_key).cloned().unwrap_or_default())
}

/// Resolves one saved host-secret in memory. This function is crate-private
/// on purpose: callers receive the value only inside the native host and it
/// is never serialized into a snapshot, plugin envelope, UI response, or log.
pub(crate) fn resolve_host_secret(
    plugin_id: &str,
    plugin_version: &str,
    setting_id: &str,
) -> Result<Option<String>, String> {
    let plugin_key = format!("{plugin_id}@{plugin_version}");
    secret_values()
        .lock()
        .map_err(|error| format!("Plugin secret settings lock poisoned: {error}"))
        .map(|values| {
            values
                .get(&plugin_key)
                .and_then(|settings| settings.get(setting_id))
                .cloned()
        })
}

fn resolve_latest_host_secret(plugin_id: &str, setting_id: &str) -> Result<Option<String>, String> {
    secret_values()
        .lock()
        .map_err(|error| format!("Plugin secret settings lock poisoned: {error}"))
        .map(|values| {
            values
                .iter()
                .filter(|(plugin_key, settings)| {
                    plugin_key
                        .strip_prefix(&format!("{plugin_id}@"))
                        .is_some_and(|version| {
                            !version.is_empty() && settings.contains_key(setting_id)
                        })
                })
                .max_by(|(left, _), (right, _)| left.cmp(right))
                .and_then(|(_, settings)| settings.get(setting_id))
                .cloned()
        })
}

/// Host-only credential resolution used by native agent execution. The source
/// accepts host-secret, environment, or auto; an omitted plugin version
/// selects the newest in-memory version for legacy PDF callers.
pub(crate) fn resolve_connection_credentials(
    plugin_id: &str,
    plugin_version: Option<&str>,
    source: &str,
    env_var: &str,
    host_secret_setting_id: &str,
) -> Result<Option<String>, String> {
    let source = source.trim().to_ascii_lowercase();
    let host_secret = || {
        plugin_version
            .map(|version| resolve_host_secret(plugin_id, version, host_secret_setting_id))
            .unwrap_or_else(|| resolve_latest_host_secret(plugin_id, host_secret_setting_id))
    };
    let environment = || {
        if env_var.trim().is_empty() {
            return Ok(None);
        }
        Ok(std::env::var(env_var)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()))
    };
    match source.as_str() {
        "host-secret" | "host" | "secret" => host_secret(),
        "environment" | "env" | "env-var" => environment(),
        "auto" | "" => Ok(host_secret()?.or(environment()?)),
        _ => Err("Unsupported credential source".to_string()),
    }
}

fn build_snapshot(
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
    persisted: BTreeMap<String, Value>,
) -> Result<PluginSettingsSnapshot, String> {
    let definitions = manifest.spec.settings.clone().unwrap_or_default();
    validate_definitions(&definitions)?;
    let overrides = normalized_public_values(manifest, &persisted)?;
    let plugin_key = format!("{plugin_id}@{plugin_version}");
    let secrets = configured_secrets(&plugin_key)?;
    let mut effective_values = BTreeMap::new();
    let mut secret_configured = BTreeMap::new();

    for setting in &definitions {
        if setting.secret {
            secret_configured.insert(setting.id.clone(), secrets.contains_key(&setting.id));
        } else if let Some(value) = overrides
            .get(&setting.id)
            .cloned()
            .or_else(|| setting.default.clone())
        {
            effective_values.insert(setting.id.clone(), value);
        }
    }

    Ok(PluginSettingsSnapshot {
        plugin_id: plugin_id.to_string(),
        plugin_version: plugin_version.to_string(),
        definitions,
        effective_values,
        overrides,
        secret_configured,
    })
}

pub(crate) fn get_snapshot(
    app: &AppHandle,
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<PluginSettingsSnapshot, String> {
    let (_directory, path) = settings_paths(app)?;
    let document = read_persisted_file(&path)?;
    let plugin_key = format!("{plugin_id}@{plugin_version}");
    let persisted = document
        .plugins
        .get(&plugin_key)
        .map(|entry| entry.values.clone())
        .unwrap_or_default();
    build_snapshot(manifest, plugin_id, plugin_version, persisted)
}

pub(crate) fn set_values(
    app: &AppHandle,
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
    values: BTreeMap<String, Value>,
) -> Result<PluginSettingsSnapshot, String> {
    let definitions = definition_map(manifest)?;
    let plugin_key = format!("{plugin_id}@{plugin_version}");
    let mut secret_updates = BTreeMap::new();
    for (id, value) in &values {
        let setting = definitions
            .get(id.as_str())
            .ok_or_else(|| format!("Unknown plugin setting: {id}"))?;
        if setting.secret && value.is_null() {
            secret_updates.insert(id.clone(), None);
            continue;
        }
        validate_value(setting, value)?;
        if setting.secret {
            let secret = value
                .as_str()
                .expect("secret settings are validated as text")
                .trim()
                .to_string();
            secret_updates.insert(id.clone(), Some(secret));
        }
    }

    let (_directory, path) = settings_paths(app)?;
    let mut document = read_persisted_file(&path)?;
    let entry = document.plugins.entry(plugin_key.clone()).or_default();
    let mut public_values = normalized_public_values(manifest, &entry.values)?;
    let mut public_changed = false;
    for (id, value) in values {
        let setting = definitions
            .get(id.as_str())
            .expect("setting was validated above");
        if !setting.secret {
            if public_values.insert(id, value).is_none() {
                public_changed = true;
            } else {
                public_changed = true;
            }
        }
    }
    entry.values = public_values;
    if public_changed
        || document
            .plugins
            .get(&plugin_key)
            .is_some_and(|entry| entry.values.is_empty())
    {
        // Keep an empty entry only when the caller explicitly cleared all
        // public values; it is harmless and preserves exact version scope.
        document.schema_version = SETTINGS_SCHEMA_VERSION;
        write_persisted_file(&_directory, &path, &document)?;
    }

    if !secret_updates.is_empty() {
        let mut all_secrets = secret_values()
            .lock()
            .map_err(|error| format!("Plugin secret settings lock poisoned: {error}"))?;
        let entry = all_secrets.entry(plugin_key).or_default();
        for (id, value) in secret_updates {
            match value {
                Some(value) => {
                    entry.insert(id, value);
                }
                None => {
                    entry.remove(&id);
                }
            }
        }
        if entry.is_empty() {
            all_secrets.retain(|_, values| !values.is_empty());
        }
    }

    get_snapshot(app, manifest, plugin_id, plugin_version)
}

pub(crate) fn reset_values(
    app: &AppHandle,
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<PluginSettingsSnapshot, String> {
    validate_definitions(manifest.spec.settings.as_deref().unwrap_or_default())?;
    remove_plugin_settings(app, plugin_id, plugin_version)?;
    get_snapshot(app, manifest, plugin_id, plugin_version)
}

/** Remove all public and in-memory secret settings for an exact plugin version. */
pub(crate) fn remove_plugin_settings(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<(), String> {
    validate_slug(plugin_id, "plugin id")?;
    validate_slug(plugin_version, "plugin version")?;
    let plugin_key = format!("{plugin_id}@{plugin_version}");
    let (_directory, path) = settings_paths(app)?;
    let mut document = read_persisted_file(&path)?;
    if document.plugins.remove(&plugin_key).is_some() {
        document.schema_version = SETTINGS_SCHEMA_VERSION;
        write_persisted_file(&_directory, &path, &document)?;
    }
    let mut values = secret_values()
        .lock()
        .map_err(|error| format!("Plugin secret settings lock poisoned: {error}"))?;
    values.remove(&plugin_key);
    Ok(())
}

pub(crate) fn build_execution_settings(
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
    persisted: BTreeMap<String, Value>,
) -> Result<Value, String> {
    let snapshot = build_snapshot(manifest, plugin_id, plugin_version, persisted)?;
    let plugin_key = format!("{plugin_id}@{plugin_version}");
    let secrets = configured_secrets(&plugin_key)?;
    for setting in &snapshot.definitions {
        if setting.required {
            let configured = if setting.secret {
                secrets.contains_key(&setting.id)
            } else {
                snapshot.effective_values.contains_key(&setting.id)
            };
            if !configured {
                return Err(format!(
                    "Required plugin setting is not configured: {}",
                    setting.id
                ));
            }
        }
    }

    // Secret material deliberately stops at this host boundary. A future
    // ModelGateway may consume it inside Rust, but plugins and WASM guests
    // never receive a plaintext secret getter or an execution-envelope field.
    Ok(serde_json::json!({
        "effectiveValues": snapshot.effective_values,
        "secretConfigured": snapshot.secret_configured,
    }))
}

pub(crate) fn persisted_values_for_execution(
    app: &AppHandle,
    plugin_id: &str,
    plugin_version: &str,
) -> Result<BTreeMap<String, Value>, String> {
    let (_directory, path) = settings_paths(app)?;
    let document = read_persisted_file(&path)?;
    Ok(document
        .plugins
        .get(&format!("{plugin_id}@{plugin_version}"))
        .map(|entry| entry.values.clone())
        .unwrap_or_default())
}

pub(crate) struct WorkerProviderLaunchSettings {
    pub runtime_config: Value,
    pub secrets: Vec<(String, String)>,
    pub fingerprint: String,
}

/// Resolve direct-provider runtime metadata and the exact declared process
/// secret environment. Plaintext secrets never enter `runtime_config`, errors,
/// logs, command arguments, RPC payloads, or the returned fingerprint.
pub(crate) fn resolve_worker_provider_launch_settings(
    app: &AppHandle,
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
    worker: &PluginWorkerDescriptor,
) -> Result<WorkerProviderLaunchSettings, String> {
    let snapshot = get_snapshot(app, manifest, plugin_id, plugin_version)?;
    let connections = manifest.spec.connections.as_deref().unwrap_or_default();
    let mut providers = Vec::new();
    let mut secrets = Vec::new();
    let mut fingerprint_hasher = Sha256::new();
    for egress in &worker.provider_egress {
        let connection = connections
            .iter()
            .find(|candidate| candidate.id == egress.connection_id)
            .ok_or_else(|| {
                format!(
                    "Worker provider connection is not declared: {}",
                    egress.connection_id
                )
            })?;
        let value = |setting_id: &str| {
            snapshot
                .effective_values
                .get(setting_id)
                .and_then(Value::as_str)
        };
        let base_url = value(&connection.url_setting_id).ok_or_else(|| {
            format!(
                "Provider URL setting is not configured: {}",
                connection.url_setting_id
            )
        })?;
        let parsed_url =
            reqwest::Url::parse(base_url).map_err(|_| "Provider URL is invalid".to_string())?;
        let host = parsed_url
            .host_str()
            .ok_or("Provider URL has no host")?
            .to_ascii_lowercase();
        if parsed_url.scheme() != "https" || !egress.domains.iter().any(|domain| domain == &host) {
            return Err(
                "Provider URL must use HTTPS and an exact manifest-declared domain".to_string(),
            );
        }
        let format = value(&connection.format_setting_id).unwrap_or("openai");
        let model = connection
            .model_setting_id
            .as_deref()
            .and_then(value)
            .unwrap_or_default();
        let transport = value("pdf-transport").unwrap_or("local-text");
        let thinking = value("thinking").unwrap_or("disabled");
        let public_progress = value("public-progress").unwrap_or("disabled");
        let (default_source, default_env, host_secret_setting) = match &connection.api_key {
            PluginApiKeySource::HostSecret { setting_id } => {
                ("host-secret", "", Some(setting_id.as_str()))
            }
            PluginApiKeySource::Environment {
                name,
                fallback_setting_id,
            } => ("environment", name.as_str(), fallback_setting_id.as_deref()),
        };
        let source = connection
            .credential_source_setting_id
            .as_deref()
            .and_then(value)
            .unwrap_or(default_source);
        let env_name = connection
            .credential_env_var_setting_id
            .as_deref()
            .and_then(value)
            .unwrap_or(default_env);
        let secret = match host_secret_setting {
            Some(setting_id) => resolve_connection_credentials(
                plugin_id,
                Some(plugin_version),
                source,
                env_name,
                setting_id,
            )?,
            None if matches!(source, "environment" | "env" | "env-var") => std::env::var(env_name)
                .ok()
                .filter(|value| !value.trim().is_empty()),
            None => None,
        };
        if let Some(secret) = secret.as_ref() {
            if secret.len() > MAX_SECRET_LENGTH {
                return Err("Configured provider credential is too long".to_string());
            }
            secrets.push((egress.secret_env.clone(), secret.clone()));
        }
        let provider_runtime = serde_json::json!({
            "id": egress.provider_id,
            "connectionId": egress.connection_id,
            "baseUrl": base_url,
            "format": format,
            "model": model,
            "pdfTransport": transport,
            "thinking": thinking,
            "publicProgress": public_progress,
            "credentialSource": source,
            "allowedDomains": egress.domains,
            "purpose": egress.purpose,
            "secretEnv": egress.secret_env,
            "secretConfigured": secret.is_some(),
            "egressEnforcement": "declarative-policy-only"
        });
        let public_bytes =
            serde_json::to_vec(&provider_runtime).map_err(|error| error.to_string())?;
        fingerprint_hasher.update((public_bytes.len() as u64).to_be_bytes());
        fingerprint_hasher.update(&public_bytes);
        if let Some(secret) = secret {
            fingerprint_hasher.update((secret.len() as u64).to_be_bytes());
            fingerprint_hasher.update(secret.as_bytes());
        } else {
            fingerprint_hasher.update(0_u64.to_be_bytes());
        }
        providers.push(provider_runtime);
    }
    let fingerprint = format!("{:x}", fingerprint_hasher.finalize());
    Ok(WorkerProviderLaunchSettings {
        runtime_config: serde_json::json!({ "providers": providers }),
        secrets,
        fingerprint,
    })
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginConnectionTestResult {
    pub ok: bool,
    /// Stable machine-readable code for plugin-owned/localized UI copy.
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSecretMutationInput {
    pub action: String,
    pub value: Option<String>,
}

fn connection_endpoint(raw: &str, format: &str) -> Result<reqwest::Url, String> {
    let mut url =
        reqwest::Url::parse(raw.trim()).map_err(|_| "Connection URL is not valid".to_string())?;
    let is_local_http = url.scheme() == "http"
        && url
            .host_str()
            .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1"));
    if url.scheme() != "https" && !is_local_http {
        return Err("Connection URL must use https:// (or localhost for development)".to_string());
    }
    if url.username() != "" || url.password().is_some() || url.host_str().is_none() {
        return Err("Connection URL must not contain credentials".to_string());
    }
    let suffix = match format {
        "openai" => "/chat/completions",
        "anthropic" => "/v1/messages",
        _ => return Err("Connection format must be openai or anthropic".to_string()),
    };
    let path = url.path().trim_end_matches('/');
    if !path.ends_with(suffix) {
        let next_path = if path.is_empty() {
            suffix.to_string()
        } else {
            format!("{path}{suffix}")
        };
        url.set_path(&next_path);
    }
    Ok(url)
}

fn connection_value<'a>(
    values: &'a BTreeMap<String, Value>,
    setting_id: &str,
    label: &str,
) -> Result<&'a str, String> {
    values
        .get(setting_id)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Connection {label} is not configured"))
}

fn result_ok(code: &str, message: &str) -> PluginConnectionTestResult {
    PluginConnectionTestResult {
        ok: true,
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn result_error(code: &str, message: &str) -> PluginConnectionTestResult {
    PluginConnectionTestResult {
        ok: false,
        code: code.to_string(),
        message: message.to_string(),
    }
}

#[derive(Clone, Debug)]
struct ConnectionTestContext {
    api_url: String,
    api_format: String,
    model: String,
    transport: String,
    api_key: Option<String>,
}

fn connection_test_payload(format: &str, model: &str) -> Value {
    match format {
        "anthropic" => serde_json::json!({
            "model": model,
            "max_tokens": 1,
            "stream": false,
            "messages": [{"role": "user", "content": "Reply with OK."}]
        }),
        _ => serde_json::json!({
            "model": model,
            "max_tokens": 1,
            "stream": false,
            "messages": [{"role": "user", "content": "Reply with OK."}]
        }),
    }
}

fn draft_secret(
    input_secrets: &BTreeMap<String, PluginSecretMutationInput>,
    stored_secrets: &BTreeMap<String, String>,
    setting_id: &str,
) -> Result<Option<String>, String> {
    let value = match input_secrets
        .get(setting_id)
        .map(|mutation| mutation.action.as_str())
    {
        Some("set") => input_secrets
            .get(setting_id)
            .and_then(|mutation| mutation.value.clone()),
        Some("clear") => None,
        Some("keep") | None => stored_secrets.get(setting_id).cloned(),
        Some(_) => return Err("Invalid secret mutation".to_string()),
    };
    Ok(value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty()))
}

fn resolve_connection_context(
    app: &AppHandle,
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
    connection_id: &str,
    action_id: &str,
    input_values: BTreeMap<String, Value>,
    input_secrets: &BTreeMap<String, PluginSecretMutationInput>,
) -> Result<ConnectionTestContext, String> {
    validate_connections(manifest)?;
    let connection = manifest
        .spec
        .connections
        .as_deref()
        .unwrap_or_default()
        .iter()
        .find(|connection| connection.id == connection_id)
        .ok_or_else(|| format!("Unknown plugin connection: {connection_id}"))?;

    if action_id == "test-connection"
        && connection
            .test_action
            .as_ref()
            .is_none_or(|action| action.id != "test-connection")
    {
        return Err("This plugin connection does not declare a test action".to_string());
    }

    let definitions = definition_map(manifest)?;
    let snapshot = get_snapshot(app, manifest, plugin_id, plugin_version)?;
    let mut effective_values = snapshot.effective_values;
    for (id, value) in input_values {
        let setting = definitions
            .get(id.as_str())
            .ok_or_else(|| format!("Unknown plugin setting: {id}"))?;
        if setting.secret {
            return Err("Secret values must use the secret channel".to_string());
        }
        validate_value(setting, &value)?;
        effective_values.insert(id, value);
    }

    let url = connection_value(&effective_values, &connection.url_setting_id, "URL")?;
    let format = connection_value(&effective_values, &connection.format_setting_id, "format")?;
    if format != "openai" && format != "anthropic" {
        return Err("Connection format must be openai or anthropic".to_string());
    }
    let model = connection
        .model_setting_id
        .as_deref()
        .and_then(|setting_id| effective_values.get(setting_id))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("deepseek-v4-flash")
        .to_string();
    let transport = ["pdf-transport", "transport", "file-transport"]
        .iter()
        .find_map(|setting_id| effective_values.get(*setting_id).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("local-text")
        .to_string();

    let plugin_key = format!("{plugin_id}@{plugin_version}");
    let stored_secrets = configured_secrets(&plugin_key)?;
    let (static_source, static_env_var, static_host_secret_setting) = match &connection.api_key {
        crate::plugins::PluginApiKeySource::Environment {
            name,
            fallback_setting_id,
        } => (
            "environment",
            Some(name.as_str()),
            fallback_setting_id.as_deref(),
        ),
        crate::plugins::PluginApiKeySource::HostSecret { setting_id } => {
            ("host-secret", None, Some(setting_id.as_str()))
        }
    };
    let credential_source = connection
        .credential_source_setting_id
        .as_deref()
        .and_then(|setting_id| effective_values.get(setting_id))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(static_source);
    let credential_env_var = connection
        .credential_env_var_setting_id
        .as_deref()
        .and_then(|setting_id| effective_values.get(setting_id))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(static_env_var);

    let needs_credential = action_id == "test-connection" || transport == "kimi-file-extract";
    let api_key = if !needs_credential {
        None
    } else {
        let resolved = match credential_source.to_ascii_lowercase().as_str() {
            "host-secret" | "host" | "secret" => static_host_secret_setting
                .map(|setting_id| draft_secret(input_secrets, &stored_secrets, setting_id))
                .transpose()?
                .flatten(),
            "environment" | "env" | "env-var" => credential_env_var
                .and_then(|name| std::env::var(name).ok())
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            _ => return Err("Unsupported credential source".to_string()),
        };
        let value = resolved
            .ok_or_else(|| "No API credential is configured for this connection".to_string())?;
        if value.len() > MAX_SECRET_LENGTH {
            return Err("API credential is too long".to_string());
        }
        Some(value)
    };

    Ok(ConnectionTestContext {
        api_url: url.to_string(),
        api_format: format.to_string(),
        model,
        transport,
        api_key,
    })
}

fn built_in_test_pdf() -> Vec<u8> {
    let content = b"BT\n/F1 12 Tf\n72 72 Td\n(Research Canvas PDF test) Tj\nET\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!("<< /Length {} >>\nstream\n{}endstream", content.len(), String::from_utf8_lossy(content)),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut bytes = b"%PDF-1.4\n%ResearchCanvas\n".to_vec();
    let mut offsets = vec![0usize];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
    }
    let xref_offset = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n", offsets.len()).as_bytes());
    bytes.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            offsets.len(),
            xref_offset
        )
        .as_bytes(),
    );
    bytes
}

fn write_built_in_test_pdf() -> Result<PathBuf, String> {
    let bytes = built_in_test_pdf();
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "Could not create a temporary PDF".to_string())?
        .as_nanos();
    let base = std::env::temp_dir();
    for attempt in 0..3u8 {
        let path = base.join(format!(
            "research-canvas-plugin-test-{}-{stamp}-{attempt}.pdf",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&bytes)
                    .map_err(|_| "Could not create a temporary PDF".to_string())?;
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("Could not create a temporary PDF".to_string()),
        }
    }
    Err("Could not create a temporary PDF".to_string())
}

async fn test_pdf_extraction(
    context: &ConnectionTestContext,
) -> Result<PluginConnectionTestResult, String> {
    let path = write_built_in_test_pdf()?;
    let result = match context.transport.as_str() {
        "local-text" => match crate::pdf_pipeline::PdfPipeline::extract_text(&path) {
            Ok(extracted) if extracted.total_chars > 0 => Ok(result_ok(
                "pdf-local-extraction-succeeded",
                "Local PDF extraction succeeded.",
            )),
            Ok(_) => Err("Local PDF extraction returned no text".to_string()),
            Err(_) => Err("Local PDF extraction failed".to_string()),
        },
        "kimi-file-extract" => match context.api_key.as_ref() {
            None => Err("No API credential is configured for this connection".to_string()),
            Some(api_key) => {
                let api_format = crate::llm_client::ApiFormat::parse(Some(&context.api_format))
                    .map_err(|_| "Kimi parser backend is invalid".to_string())?;
                let config = crate::llm_client::PdfAgentLlmConfig {
                    api_url: context.api_url.clone(),
                    api_format,
                    api_key: api_key.clone(),
                    model: context.model.clone(),
                    thinking: false,
                    thinking_level: None,
                    provider: "moonshot".to_string(),
                    transport: crate::llm_client::PdfAgentTransport::KimiFileExtract,
                    timeout_secs: 30,
                };
                let runtime = crate::native_plugins::pdf_canvas_agent::normalize_config(config)
                    .map_err(|_| "Kimi PDF extraction could not be initialized".to_string())?;
                let crate::native_plugins::pdf_canvas_agent::Backend::KimiK26(config) =
                    runtime.backend
                else {
                    return Err("Kimi PDF extraction could not be initialized".to_string());
                };
                let client = crate::native_plugins::pdf_canvas_agent::KimiK26Client::new(config)
                    .map_err(|_| "Kimi PDF extraction could not be initialized".to_string())?;
                match client.extract_pdf_text(&path).await {
                    Ok(_) => Ok(result_ok(
                        "pdf-remote-extraction-succeeded",
                        "Kimi PDF extraction succeeded.",
                    )),
                    Err(_) => Err("Kimi PDF extraction failed".to_string()),
                }
            }
        },
        _ => Err("Unsupported PDF extraction mode".to_string()),
    };
    let _ = fs::remove_file(path);
    result
}

pub(crate) async fn test_connection(
    app: &AppHandle,
    manifest: &MycPluginManifest,
    plugin_id: &str,
    plugin_version: &str,
    connection_id: &str,
    action_id: Option<String>,
    input_values: BTreeMap<String, Value>,
    input_secrets: BTreeMap<String, PluginSecretMutationInput>,
) -> Result<PluginConnectionTestResult, String> {
    let action_id = action_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("test-connection");
    if !matches!(action_id, "test-connection" | "test-pdf-extraction") {
        return Ok(result_error(
            "unsupported-action",
            "This test action is not supported.",
        ));
    }
    let context = match resolve_connection_context(
        app,
        manifest,
        plugin_id,
        plugin_version,
        connection_id,
        action_id,
        input_values,
        &input_secrets,
    ) {
        Ok(context) => context,
        Err(error) => {
            let code = if error.contains("credential") {
                "credential-not-configured"
            } else {
                "invalid-settings"
            };
            return Ok(result_error(code, &error));
        }
    };

    if action_id == "test-pdf-extraction" {
        return Ok(match test_pdf_extraction(&context).await {
            Ok(result) => result,
            Err(error) if error.contains("credential") => {
                result_error("credential-not-configured", &error)
            }
            Err(error) if context.transport == "local-text" => {
                result_error("pdf-local-extraction-failed", &error)
            }
            Err(error) => result_error("pdf-remote-extraction-failed", &error),
        });
    }

    let api_key = match context.api_key.as_deref() {
        Some(value) => value,
        None => {
            return Ok(result_error(
                "credential-not-configured",
                "No API credential is configured for this connection.",
            ));
        }
    };
    let endpoint = match connection_endpoint(&context.api_url, &context.api_format) {
        Ok(endpoint) => endpoint,
        Err(error) => return Ok(result_error("invalid-url", &error)),
    };
    let payload = connection_test_payload(&context.api_format, &context.model);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return Ok(result_error(
                "client-init-failed",
                "Could not initialize the connection test.",
            ));
        }
    };
    let response = if context.api_format == "anthropic"
        && context.api_url.to_ascii_lowercase().contains("moonshot")
    {
        client
            .post(endpoint)
            .bearer_auth(api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await
    } else if context.api_format == "anthropic" {
        client
            .post(endpoint)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()
            .await
    } else {
        client
            .post(endpoint)
            .bearer_auth(api_key)
            .json(&payload)
            .send()
            .await
    };

    match response {
        Ok(response) if response.status().is_success() => Ok(PluginConnectionTestResult {
            ok: true,
            code: "connection-succeeded".to_string(),
            message: "Connection succeeded.".to_string(),
        }),
        Ok(response) if response.status().as_u16() == 401 || response.status().as_u16() == 403 => {
            Ok(PluginConnectionTestResult {
                ok: false,
                code: "credential-rejected".to_string(),
                message: "Provider rejected the API credential.".to_string(),
            })
        }
        Ok(response) if response.status().as_u16() == 429 => Ok(PluginConnectionTestResult {
            ok: false,
            code: "rate-limited".to_string(),
            message: "Provider reached, but rate limited the test request.".to_string(),
        }),
        Ok(response) => Ok(PluginConnectionTestResult {
            ok: false,
            code: "provider-error".to_string(),
            message: format!("Provider returned HTTP {}.", response.status().as_u16()),
        }),
        Err(_) => Ok(PluginConnectionTestResult {
            ok: false,
            code: "request-failed".to_string(),
            message: "Connection request could not be completed.".to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::{MycPluginSpec, PluginSettingOption};

    fn manifest(settings: Vec<PluginSettingDefinition>) -> MycPluginManifest {
        MycPluginManifest {
            api_version: "researchcanvas.dev/v1alpha1".to_string(),
            kind: "AnalysisPlugin".to_string(),
            metadata: crate::plugins::MycPluginMetadata {
                id: "test.settings".to_string(),
                name: "Settings".to_string(),
                version: "1.0.0".to_string(),
                publisher: "Test".to_string(),
                developer: "Test".to_string(),
                developer_uuid: None,
                description: "Settings test".to_string(),
                homepage: None,
                license: None,
                official: false,
                update: None,
            },
            spec: MycPluginSpec {
                engine: "wasm32-myc".to_string(),
                entry: "plugin.wasm".to_string(),
                language: Some("rust".to_string()),
                capabilities: vec!["analysis.run".to_string()],
                permissions: vec![],
                contributes: None,
                settings: Some(settings),
                connections: None,
            },
            frontend: None,
            worker: None,
            workers: None,
            network: None,
            provides: None,
            payloads: None,
            signature: None,
        }
    }

    fn setting(id: &str, setting_type: &str) -> PluginSettingDefinition {
        PluginSettingDefinition {
            id: id.to_string(),
            label: id.to_string(),
            description: None,
            setting_type: setting_type.to_string(),
            default: None,
            min: None,
            max: None,
            step: None,
            options: None,
            secret: false,
            required: false,
            placeholder: None,
            group: None,
        }
    }

    #[test]
    fn validates_secret_required_and_ranged_defaults() {
        let mut count = setting("max-nodes", "number");
        count.default = Some(serde_json::json!(20));
        count.min = Some(10.0);
        count.max = Some(100.0);
        count.step = Some(10.0);
        let mut api_key = setting("api-key", "text");
        api_key.secret = true;
        api_key.required = true;
        api_key.placeholder = Some("sk-...".to_string());
        api_key.group = Some("Credentials".to_string());
        validate_definitions(&[count, api_key]).expect("valid definitions");
    }

    #[test]
    fn rejects_unknown_keys_wrong_types_ranges_steps_and_invalid_selects() {
        let mut count = setting("max-nodes", "number");
        count.min = Some(10.0);
        count.max = Some(100.0);
        count.step = Some(10.0);
        let mut mode = setting("mode", "select");
        mode.options = Some(vec![PluginSettingOption {
            value: "safe".to_string(),
            label: "Safe".to_string(),
        }]);
        let definitions = vec![count, mode];
        let manifest = manifest(definitions.clone());
        let map = definition_map(&manifest).expect("definitions");
        assert!(map.get("missing").is_none());
        assert!(validate_value(map["max-nodes"], &serde_json::json!(11)).is_err());
        assert!(validate_value(map["max-nodes"], &serde_json::json!(101)).is_err());
        assert!(validate_value(map["max-nodes"], &serde_json::json!("10")).is_err());
        assert!(validate_value(map["mode"], &serde_json::json!("unsafe")).is_err());
    }

    #[test]
    fn execution_settings_keep_public_values_and_redact_secret_plaintext() {
        let mut secret = setting("api-key", "text");
        secret.secret = true;
        let manifest = manifest(vec![secret]);
        let key = "test.settings@1.0.0";
        if let Ok(mut values) = secret_values().lock() {
            values.insert(
                key.to_string(),
                BTreeMap::from([("api-key".to_string(), "secret-value".to_string())]),
            );
        }
        let execution =
            build_execution_settings(&manifest, "test.settings", "1.0.0", BTreeMap::new())
                .expect("execution settings");
        assert!(execution["effectiveValues"].get("api-key").is_none());
        assert_eq!(execution["secretConfigured"]["api-key"], true);
        assert!(execution.get("secrets").is_none());
        assert!(!serde_json::to_string(&execution)
            .expect("serialize execution settings")
            .contains("secret-value"));
        if let Ok(mut values) = secret_values().lock() {
            values.remove(key);
        }
    }

    #[test]
    fn host_secret_resolution_is_exact_and_never_part_of_public_snapshot() {
        let key = "test.settings@2.0.0";
        if let Ok(mut values) = secret_values().lock() {
            values.insert(
                key.to_string(),
                BTreeMap::from([("api-key".to_string(), "host-secret-value".to_string())]),
            );
        }
        assert_eq!(
            resolve_host_secret("test.settings", "2.0.0", "api-key").expect("host secret lookup"),
            Some("host-secret-value".to_string())
        );
        assert_eq!(
            resolve_host_secret("test.settings", "9.0.0", "api-key")
                .expect("missing host secret lookup"),
            None
        );
        let public = serde_json::to_string(
            &build_snapshot(
                &manifest(vec![setting("api-key", "text")]),
                "test.settings",
                "2.0.0",
                BTreeMap::new(),
            )
            .expect("snapshot"),
        )
        .expect("serialize snapshot");
        assert!(!public.contains("host-secret-value"));
        if let Ok(mut values) = secret_values().lock() {
            values.remove(key);
        }
    }

    #[test]
    fn connection_credentials_can_select_host_secret_without_serializing_it() {
        let key = "test.settings@3.0.0";
        if let Ok(mut values) = secret_values().lock() {
            values.insert(
                key.to_string(),
                BTreeMap::from([("api-key".to_string(), "selected-host-secret".to_string())]),
            );
        }
        let resolved = resolve_connection_credentials(
            "test.settings",
            Some("3.0.0"),
            "host-secret",
            "UNUSED_TEST_ENV",
            "api-key",
        )
        .expect("credential resolution");
        assert_eq!(resolved, Some("selected-host-secret".to_string()));
        if let Ok(mut values) = secret_values().lock() {
            values.remove(key);
        }
    }

    #[test]
    fn connection_test_payload_is_text_only_and_contains_no_file_upload_shape() {
        for format in ["openai", "anthropic"] {
            let payload = connection_test_payload(format, "test-model");
            let serialized = serde_json::to_string(&payload).expect("serialize test payload");
            assert!(payload.get("file").is_none());
            assert!(payload.get("files").is_none());
            assert!(!serialized.contains("multipart"));
            assert!(!serialized.contains("%PDF"));
            assert_eq!(payload["messages"][0]["content"], "Reply with OK.");
        }
    }

    #[test]
    fn built_in_pdf_is_non_empty_valid_and_extractable() {
        let bytes = built_in_test_pdf();
        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(b"%PDF-1.4"));

        let document = lopdf::Document::load_mem(&bytes).expect("valid built-in test PDF");
        let pages = document.get_pages();
        assert_eq!(pages.len(), 1);
        let page_number = pages.keys().next().expect("one test page");
        let text = document
            .extract_text(&[*page_number])
            .expect("extract test PDF text");
        assert!(text.contains("Research Canvas PDF test"));
    }

    #[test]
    fn environment_credentials_are_used_only_when_environment_is_selected() {
        let variable = "RESEARCH_CANVAS_PLUGIN_TEST_KEY";
        std::env::set_var(variable, "environment-secret");

        let environment = resolve_connection_credentials(
            "test.environment-only",
            None,
            "environment",
            variable,
            "api-key",
        )
        .expect("environment credential resolution");
        assert_eq!(environment.as_deref(), Some("environment-secret"));

        let host_only = resolve_connection_credentials(
            "test.environment-only",
            None,
            "host-secret",
            variable,
            "api-key",
        )
        .expect("host-only credential resolution");
        assert_eq!(host_only, None);

        std::env::remove_var(variable);
    }
}
