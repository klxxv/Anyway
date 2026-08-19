//! Plugin manifest v2 — flat VSCode(`package.json`)+Cordis(services/events/lifecycle)
//! style. The kernel parses this JSON format and migrates it into the internal
//! [`crate::plugins::MycPluginManifest`] representation so the existing install /
//! discovery / validation pipeline keeps working unchanged.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::plugins::{
    MycPluginContributions, MycPluginManifest, MycPluginMetadata, MycPluginSpec,
    PluginCommandContribution, PluginConnectionDefinition, PluginContextMenuContribution,
    PluginLocaleContribution, PluginSettingDefinition,
};

/// The flat manifest accepted by the kernel (VSCode + Cordis style).
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestV2 {
    /// Unique plugin id (former `metadata.id`).
    pub name: String,
    /// Human-readable name (former `metadata.name`).
    #[serde(default)]
    pub display_name: Option<String>,
    pub version: String,
    pub publisher: String,
    #[serde(default)]
    pub developer: Option<String>,
    #[serde(default)]
    pub developer_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    /// Free-form categories; the first category is the executable kind.
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub engines: Option<serde_json::Value>,
    /// Entry payload (former `spec.entry`).
    #[serde(default)]
    pub main: Option<String>,
    /// Guest language for `AnalysisPlugin` payloads (former `spec.language`;
    /// informational — the VM validates the wasm bytes, not the source language).
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub activation_events: Vec<String>,
    #[serde(default)]
    pub contributes: Option<ContributesV2>,
    #[serde(default)]
    pub provides: Option<CordisProvides>,
    #[serde(default)]
    pub inject: Option<CordisInject>,
    #[serde(default)]
    pub lifecycle: Option<CordisLifecycle>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub payloads: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributesV2 {
    #[serde(default)]
    pub commands: Option<Vec<PluginCommandContribution>>,
    #[serde(default)]
    pub menus: Option<Vec<PluginContextMenuContribution>>,
    #[serde(default)]
    pub configuration: Option<ConfigurationV2>,
    #[serde(default)]
    pub views_containers: Option<serde_json::Value>,
    #[serde(default)]
    pub views: Option<serde_json::Value>,
    #[serde(default)]
    pub ui_ir: Option<serde_json::Value>,
    #[serde(default)]
    pub locales: Option<Vec<PluginLocaleContribution>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationV2 {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub settings: Option<Vec<PluginSettingDefinition>>,
    #[serde(default)]
    pub connections: Option<Vec<PluginConnectionDefinition>>,
}

/// Cordis-style services/events a plugin provides to the host.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CordisProvides {
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub events: Vec<String>,
}

/// Cordis-style services/events a plugin requires from the host.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CordisInject {
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub events: Vec<String>,
}

/// Cordis-style lifecycle entry points.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CordisLifecycle {
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub stop: Option<String>,
    #[serde(default)]
    pub reusable: bool,
}

fn infer_kind(categories: &[String], main: Option<&str>) -> String {
    if let Some(category) = categories.first() {
        if !category.trim().is_empty() {
            return category.clone();
        }
    }
    match main {
        Some(entry) if entry.ends_with(".wasm") => "AnalysisPlugin".to_string(),
        Some(entry) if entry == "theme.json" => "ThemePlugin".to_string(),
        Some(entry) if entry == "edge-style.json" => "EdgeStylePlugin".to_string(),
        Some(entry) if entry == "icon-theme.json" => "IconThemePlugin".to_string(),
        Some(entry) if entry == "agent-manifest.json" => "AgentPlugin".to_string(),
        Some(entry) if entry == "workspace-plugin.json" => "WorkspacePlugin".to_string(),
        _ => "ExtensionPlugin".to_string(),
    }
}

impl From<ManifestV2> for MycPluginManifest {
    fn from(value: ManifestV2) -> Self {
        let kind = infer_kind(&value.categories, value.main.as_deref());
        let (contributes, settings, connections) = match value.contributes {
            Some(c) => {
                let (settings, connections) = c
                    .configuration
                    .map(|config| (config.settings, config.connections))
                    .unwrap_or((None, None));
                (
                    Some(MycPluginContributions {
                        context_menus: c.menus,
                        locales: c.locales,
                        commands: c.commands,
                    }),
                    settings,
                    connections,
                )
            }
            None => (None, None, None),
        };

        MycPluginManifest {
            api_version: "researchcanvas.dev/v2".to_string(),
            kind,
            metadata: MycPluginMetadata {
                id: value.name,
                name: value.display_name.unwrap_or_default(),
                version: value.version,
                publisher: value.publisher,
                developer: value.developer.unwrap_or_default(),
                developer_uuid: value.developer_id,
                description: value.description.unwrap_or_default(),
                homepage: value.homepage,
                license: value.license,
                update: None,
            },
            spec: MycPluginSpec {
                engine: value
                    .engines
                    .and_then(|engines| {
                        engines
                            .get("engine")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "declarative".to_string()),
                entry: value.main.unwrap_or_default(),
                language: value.language,
                capabilities: value.capabilities,
                permissions: value.permissions,
                contributes,
                settings,
                connections,
            },
            payloads: value.payloads,
            signature: value.signature,
        }
    }
}

/// Parse a plugin manifest, accepting either the legacy nested v1 format or the
/// flat v2 format, and return the internal representation.
pub fn parse_plugin_manifest(text: &str) -> Result<MycPluginManifest, String> {
    let value: serde_json::Value =
        serde_json::from_str(text).map_err(|error| error.to_string())?;
    // v1 nested manifests carry a `metadata` object; v2 flat manifests carry `name`.
    if value.get("metadata").is_some() {
        serde_json::from_value::<MycPluginManifest>(value).map_err(|error| error.to_string())
    } else {
        let v2: ManifestV2 =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        Ok(MycPluginManifest::from(v2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn v2_flat_manifest_migrates_to_internal() {
        let text = json!({
            "name": "myc.folder-workspaces",
            "displayName": "Folder Workspaces",
            "version": "2.0.0",
            "publisher": "Research Canvas",
            "description": "Scans folders",
            "categories": ["WorkspacePlugin"],
            "main": "workspace-plugin.json",
            "capabilities": ["project.folder"],
            "contributes": {
                "commands": [{
                    "id": "open-folder-workspace",
                    "label": "Open folder",
                    "description": "Index projects",
                    "category": "folder",
                    "capability": "project.folder"
                }]
            }
        })
        .to_string();

        let manifest = parse_plugin_manifest(&text).expect("parses v2");
        assert_eq!(manifest.kind, "WorkspacePlugin");
        assert_eq!(manifest.metadata.id, "myc.folder-workspaces");
        assert_eq!(manifest.metadata.name, "Folder Workspaces");
        assert_eq!(manifest.spec.entry, "workspace-plugin.json");
        assert_eq!(manifest.spec.capabilities, vec!["project.folder"]);
        assert_eq!(
            manifest
                .spec
                .contributes
                .as_ref()
                .and_then(|c| c.commands.as_ref())
                .map(|commands| commands.len()),
            Some(1)
        );
    }

    #[test]
    fn v2_manifest_carries_guest_language() {
        let text = json!({
            "name": "myc.runtime-smoke",
            "version": "1.1.0",
            "publisher": "Research Canvas",
            "categories": ["AnalysisPlugin"],
            "main": "plugin.wasm",
            "engines": {"engine": "wasm32-myc"},
            "language": "rust",
            "capabilities": ["analysis.run"]
        })
        .to_string();

        let manifest = parse_plugin_manifest(&text).expect("parses v2");
        assert_eq!(manifest.spec.language.as_deref(), Some("rust"));
    }

    #[test]
    fn legacy_nested_manifest_still_parses() {
        let text = json!({
            "apiVersion": "researchcanvas.dev/v1alpha1",
            "kind": "ThemePlugin",
            "metadata": {
                "id": "myc.onedarkpro",
                "name": "One Dark Pro",
                "version": "1.3.0",
                "publisher": "Community",
                "developer": "Theme Lab",
                "description": "theme"
            },
            "spec": {
                "engine": "declarative",
                "entry": "theme.json",
                "capabilities": ["theme.register"],
                "permissions": []
            }
        })
        .to_string();

        let manifest = parse_plugin_manifest(&text).expect("parses v1");
        assert_eq!(manifest.kind, "ThemePlugin");
        assert_eq!(manifest.metadata.id, "myc.onedarkpro");
    }
}
