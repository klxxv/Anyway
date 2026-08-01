//! 原生项目文件持久化 / Native project-file persistence.
//! 文件对话框由 Tauri 插件负责；本模块校验大小、扩展名、JSON 结构和用户明确选择的路径。

use serde::Serialize;
use serde_json::Value;
use std::{
    fs,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_PROJECT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileResult {
    path: String,
    bytes: usize,
    saved_at: Option<String>,
    project: Option<Value>,
}

fn validate_project_path(path: &Path) -> Result<(), String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !matches!(extension.to_ascii_lowercase().as_str(), "mycproj" | "json") {
        return Err("Project files must use .mycproj or .json".to_string());
    }
    if path.file_name().is_none() {
        return Err("Project path must include a file name".to_string());
    }
    Ok(())
}

pub(crate) fn validate_project(value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| "Project root must be a JSON object".to_string())?;
    for key in [
        "schemaVersion",
        "id",
        "title",
        "discipline",
        "revision",
        "nodes",
        "edges",
        "evidence",
        "placements",
        "scenarios",
        "activity",
    ] {
        if !object.contains_key(key) {
            return Err(format!("Project is missing required field: {key}"));
        }
    }
    for key in [
        "nodes",
        "edges",
        "evidence",
        "placements",
        "scenarios",
        "activity",
    ] {
        if !object.get(key).is_some_and(Value::is_array) {
            return Err(format!("Project field must be an array: {key}"));
        }
    }
    Ok(())
}

fn unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > MAX_PROJECT_BYTES {
        return Err("File exceeds the 32 MB project limit".to_string());
    }
    if let Some(parent) = path.parent() {
        if !parent.is_dir() {
            return Err(format!(
                "Parent folder does not exist: {}",
                parent.display()
            ));
        }
    }
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|error| format!("Could not create {}: {error}", path.display()))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("Could not save {}: {error}", path.display()))
}

#[tauri::command]
pub fn save_project_file(path: String, project: Value) -> Result<ProjectFileResult, String> {
    let path = PathBuf::from(path);
    validate_project_path(&path)?;
    validate_project(&project)?;
    let bytes = serde_json::to_vec_pretty(&project).map_err(|error| error.to_string())?;
    write_bytes(&path, &bytes)?;
    Ok(ProjectFileResult {
        path: path.to_string_lossy().into_owned(),
        bytes: bytes.len(),
        saved_at: Some(unix_millis().to_string()),
        project: None,
    })
}

#[tauri::command]
pub fn import_project_file(path: String) -> Result<ProjectFileResult, String> {
    let path = PathBuf::from(path);
    validate_project_path(&path)?;
    let bytes =
        fs::read(&path).map_err(|error| format!("Could not read {}: {error}", path.display()))?;
    if bytes.len() > MAX_PROJECT_BYTES {
        return Err("Project file exceeds the 32 MB limit".to_string());
    }
    let project: Value = serde_json::from_slice(&bytes)
        .map_err(|error| format!("Project file is not valid JSON: {error}"))?;
    validate_project(&project)?;
    Ok(ProjectFileResult {
        path: path.to_string_lossy().into_owned(),
        bytes: bytes.len(),
        saved_at: None,
        project: Some(project),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::tempdir;

    fn project() -> Value {
        json!({
            "schemaVersion": 2,
            "id": "pinn-test",
            "title": "PINN architecture",
            "discipline": "Physics-informed neural networks",
            "updatedAt": "2026-08-01T00:00:00Z",
            "revision": 3,
            "nodes": [], "edges": [], "evidence": [], "placements": [],
            "scenarios": [], "activity": []
        })
    }

    #[test]
    fn native_project_round_trip_preserves_json() {
        let root = tempdir().expect("temporary project root");
        let path = root.path().join("pinn.mycproj");
        save_project_file(path.to_string_lossy().into_owned(), project()).expect("save project");
        let loaded =
            import_project_file(path.to_string_lossy().into_owned()).expect("load project");
        assert_eq!(loaded.project, Some(project()));
    }

    #[test]
    fn project_io_rejects_unrelated_extensions_and_invalid_shapes() {
        let root = tempdir().expect("temporary project root");
        let bad_extension = root.path().join("pinn.txt");
        assert!(
            save_project_file(bad_extension.to_string_lossy().into_owned(), project()).is_err()
        );
        let bad_shape = root.path().join("pinn.mycproj");
        assert!(save_project_file(
            bad_shape.to_string_lossy().into_owned(),
            json!({"schemaVersion": 2})
        )
        .is_err());
    }
}
