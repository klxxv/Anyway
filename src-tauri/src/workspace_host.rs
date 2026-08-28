//! WorkspacePlugin 的能力中介文件、导出、文件夹与 Git 宿主动作。
//! Capability-mediated filesystem, export, folder, and Git host actions for WorkspacePlugin.

use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command,
};
use tauri::AppHandle;

use crate::graph_compiler;

const MAX_ARTIFACT_BYTES: usize = 32 * 1024 * 1024;
const MAX_GIT_OUTPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_FOLDER_PROJECTS: usize = 500;
const MAX_FOLDER_TREE_ENTRIES: usize = 1_000;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderProjectSummary {
    path: String,
    title: String,
    discipline: String,
    revision: u64,
    updated_at: String,
    node_count: usize,
    edge_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderTreeEntry {
    path: String,
    name: String,
    kind: String,
    size: u64,
    modified_at: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitRecord {
    id: String,
    short_id: String,
    parents: Vec<String>,
    author: String,
    timestamp: String,
    message: String,
    refs: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWorkspaceSnapshot {
    repo_path: String,
    is_repository: bool,
    branch: String,
    dirty: bool,
    commits: Vec<GitCommitRecord>,
    graph_patch: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitSshPublicKey {
    path: String,
    algorithm: String,
    fingerprint: String,
    public_key: String,
    managed_by_app: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubAccountStatus {
    cli_available: bool,
    authenticated: bool,
    host: String,
    login: Option<String>,
    git_protocol: Option<String>,
    ssh_keygen_available: bool,
    ssh_keys: Vec<GitSshPublicKey>,
}

fn bounded_command_output(
    program: &str,
    arguments: &[&str],
) -> Result<std::process::Output, String> {
    let output = Command::new(program)
        .args(arguments)
        .output()
        .map_err(|error| format!("Could not start {program}: {error}"))?;
    if output.stdout.len() + output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err(format!("{program} output exceeds 4 MB"));
    }
    Ok(output)
}

fn command_available(program: &str, version_argument: &str) -> bool {
    bounded_command_output(program, &[version_argument]).is_ok()
}

fn ssh_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .map(|path| path.join(".ssh"))
}

fn ssh_key_fingerprint(path: &Path) -> String {
    let Some(path_text) = path.to_str() else {
        return String::new();
    };
    bounded_command_output("ssh-keygen", &["-lf", path_text])
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|output| output.split_whitespace().nth(1).map(str::to_string))
        .unwrap_or_default()
}

fn list_ssh_public_keys() -> Vec<GitSshPublicKey> {
    let Some(directory) = ssh_directory() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    let mut keys = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let path = entry.path();
            if !file_type.is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("pub")
            {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if metadata.len() > 16 * 1024 {
                return None;
            }
            let public_key = fs::read_to_string(&path).ok()?.trim().to_string();
            let algorithm = public_key.split_whitespace().next()?.to_string();
            if !(algorithm.starts_with("ssh-") || algorithm.starts_with("sk-ssh-")) {
                return None;
            }
            let managed_by_app = path.file_name().and_then(|value| value.to_str())
                == Some("research_canvas_ed25519.pub");
            Some(GitSshPublicKey {
                path: path.to_string_lossy().into_owned(),
                algorithm,
                fingerprint: ssh_key_fingerprint(&path),
                public_key,
                managed_by_app,
            })
        })
        .take(32)
        .collect::<Vec<_>>();
    keys.sort_by(|left, right| left.path.cmp(&right.path));
    keys
}

fn github_account_status() -> GitHubAccountStatus {
    let cli_available = command_available("gh", "--version");
    let ssh_keygen_available = command_available("ssh-keygen", "-?");
    let mut status = GitHubAccountStatus {
        cli_available,
        authenticated: false,
        host: "github.com".to_string(),
        login: None,
        git_protocol: None,
        ssh_keygen_available,
        ssh_keys: list_ssh_public_keys(),
    };
    if !cli_available {
        return status;
    }
    let Ok(output) = bounded_command_output(
        "gh",
        &[
            "auth",
            "status",
            "--hostname",
            "github.com",
            "--active",
            "--json",
            "hosts",
        ],
    ) else {
        return status;
    };
    if !output.status.success() {
        return status;
    }
    let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) else {
        return status;
    };
    let account = value["hosts"]["github.com"]
        .as_array()
        .and_then(|accounts| accounts.iter().find(|account| account["active"] == true));
    if let Some(account) = account {
        status.authenticated = account["state"] == "success";
        status.login = account["login"].as_str().map(str::to_string);
        status.git_protocol = account["gitProtocol"].as_str().map(str::to_string);
    }
    status
}

fn validate_ssh_comment(comment: &str) -> Result<&str, String> {
    let comment = comment.trim();
    if comment.is_empty() || comment.len() > 254 || comment.chars().any(char::is_control) {
        return Err("SSH key comment must be 1-254 printable characters".to_string());
    }
    Ok(comment)
}

fn generate_ssh_key_at(directory: &Path, comment: &str) -> Result<(), String> {
    let comment = validate_ssh_comment(comment)?;
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let private_key = directory.join("research_canvas_ed25519");
    let public_key = directory.join("research_canvas_ed25519.pub");
    if private_key.exists() || public_key.exists() {
        return Err("Research Canvas SSH key already exists".to_string());
    }
    let private_key_text = private_key
        .to_str()
        .ok_or_else(|| "SSH key path is not valid UTF-8".to_string())?;
    let output = bounded_command_output(
        "ssh-keygen",
        &[
            "-q",
            "-t",
            "ed25519",
            "-C",
            comment,
            "-f",
            private_key_text,
            "-N",
            "",
        ],
    )?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(())
}

fn generate_managed_ssh_key(comment: &str) -> Result<(), String> {
    let directory =
        ssh_directory().ok_or_else(|| "Could not resolve the user SSH folder".to_string())?;
    generate_ssh_key_at(&directory, comment)
}

fn validated_public_key_path(path: &str) -> Result<PathBuf, String> {
    let directory =
        ssh_directory().ok_or_else(|| "Could not resolve the user SSH folder".to_string())?;
    let directory = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let key = PathBuf::from(path)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !key.starts_with(&directory)
        || key.extension().and_then(|value| value.to_str()) != Some("pub")
    {
        return Err(
            "SSH public key must be a .pub file in the current user's .ssh folder".to_string(),
        );
    }
    Ok(key)
}

fn validate_artifact_extension(path: &Path, format: &str) -> Result<(), String> {
    if !matches!(format, "pdf" | "svg" | "png") {
        return Err(format!("Unsupported export format: {format}"));
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|extension| !extension.eq_ignore_ascii_case(format))
    {
        return Err(format!("Export path must end in .{format}"));
    }
    Ok(())
}

#[tauri::command]
pub fn save_plugin_artifact(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    format: String,
    path: String,
    data: Vec<u8>,
) -> Result<String, String> {
    crate::plugins::require_plugin_export_format(&app, &plugin_id, &plugin_version, &format)?;
    let path = PathBuf::from(path);
    validate_artifact_extension(&path, &format)?;
    if data.len() > MAX_ARTIFACT_BYTES {
        return Err("Export artifact exceeds 32 MB".to_string());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "Export path requires a parent folder".to_string())?;
    if !parent.is_dir() {
        return Err(format!(
            "Export folder does not exist: {}",
            parent.display()
        ));
    }
    fs::write(&path, data)
        .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}

fn visit_project_folder(root: &Path, depth: usize, found: &mut Vec<PathBuf>) -> Result<(), String> {
    if depth > 4 || found.len() >= MAX_FOLDER_PROJECTS {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(".git") {
                continue;
            }
            visit_project_folder(&path, depth + 1, found)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|extension| {
                extension.eq_ignore_ascii_case("mycproj") || extension.eq_ignore_ascii_case("json")
            })
        {
            found.push(path);
            if found.len() >= MAX_FOLDER_PROJECTS {
                break;
            }
        }
    }
    Ok(())
}

fn summarize_project_file(project_path: &Path) -> Option<FolderProjectSummary> {
    let bytes = fs::read(project_path).ok()?;
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return None;
    }
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    let object = value.as_object()?;
    let title = object.get("title").and_then(Value::as_str)?;
    let discipline = object.get("discipline").and_then(Value::as_str)?;
    let updated_at = object.get("updatedAt").and_then(Value::as_str)?;
    Some(FolderProjectSummary {
        path: project_path.to_string_lossy().into_owned(),
        title: title.to_string(),
        discipline: discipline.to_string(),
        revision: object.get("revision").and_then(Value::as_u64).unwrap_or(0),
        updated_at: updated_at.to_string(),
        node_count: object
            .get("nodes")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
        edge_count: object
            .get("edges")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    })
}

fn validate_folder_tree_path(root: &Path, requested: &Path) -> Result<(PathBuf, PathBuf), String> {
    let root = root
        .canonicalize()
        .map_err(|error| format!("Could not resolve folder root: {error}"))?;
    if !root.is_dir() {
        return Err("Project folder does not exist".to_string());
    }

    // Reject symlink components before canonicalization when the caller supplied
    // a path inside the selected root. This keeps a directory junction/symlink
    // from becoming an implicit escape hatch for the explorer.
    if requested.is_absolute() && requested.starts_with(&root) {
        let relative = requested
            .strip_prefix(&root)
            .map_err(|_| "Folder path is outside the selected root".to_string())?;
        let mut cursor = root.clone();
        for component in relative.components() {
            let Component::Normal(name) = component else {
                continue;
            };
            cursor.push(name);
            if fs::symlink_metadata(&cursor)
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err("Folder explorer does not follow symbolic links".to_string());
            }
        }
    }

    let requested = requested
        .canonicalize()
        .map_err(|error| format!("Could not resolve folder path: {error}"))?;
    if !requested.starts_with(&root) {
        return Err("Folder path must stay inside the selected root".to_string());
    }
    if !requested.is_dir() {
        return Err("Folder path must be a directory".to_string());
    }
    Ok((root, requested))
}

fn modified_timestamp(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[tauri::command]
pub fn list_folder_entries(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    root: String,
    path: String,
) -> Result<Vec<FolderTreeEntry>, String> {
    crate::plugins::require_plugin_capability(&app, &plugin_id, &plugin_version, "project.folder")?;
    let (_, directory) = validate_folder_tree_path(Path::new(&root), Path::new(&path))?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
        if entries.len() >= MAX_FOLDER_TREE_ENTRIES {
            break;
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        let entry_path = entry.path();
        let kind = if file_type.is_dir() {
            "directory"
        } else if file_type.is_file() {
            "file"
        } else {
            continue;
        };
        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        entries.push(FolderTreeEntry {
            path: entry_path.to_string_lossy().into_owned(),
            name: entry.file_name().to_string_lossy().into_owned(),
            kind: kind.to_string(),
            size: if file_type.is_file() {
                metadata.len()
            } else {
                0
            },
            modified_at: modified_timestamp(&entry_path),
        });
    }
    entries.sort_by(|left, right| {
        (left.kind != "directory")
            .cmp(&(right.kind != "directory"))
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(entries)
}

#[tauri::command]
pub fn scan_project_folder(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    path: String,
) -> Result<Vec<FolderProjectSummary>, String> {
    crate::plugins::require_plugin_capability(&app, &plugin_id, &plugin_version, "project.folder")?;
    let root = PathBuf::from(path);
    if !root.is_dir() {
        return Err("Project folder does not exist".to_string());
    }
    let mut paths = Vec::new();
    visit_project_folder(&root, 0, &mut paths)?;
    let mut projects = paths
        .iter()
        .filter_map(|path| summarize_project_file(path))
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(projects)
}

fn git_output(repo: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repo)
        .output()
        .map_err(|error| format!("Could not start git: {error}"))?;
    if output.stdout.len() + output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err("Git output exceeds 4 MB".to_string());
    }
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    String::from_utf8(output.stdout).map_err(|error| error.to_string())
}

fn resolve_repo(path: &Path) -> Result<PathBuf, String> {
    if !path.is_dir() {
        return Err("Git workspace folder does not exist".to_string());
    }
    let root = git_output(path, &["rev-parse", "--show-toplevel"])?;
    let root = PathBuf::from(root.trim());
    root.canonicalize().map_err(|error| error.to_string())
}

fn is_git_repository(path: &Path) -> Result<bool, String> {
    if !path.is_dir() {
        return Err("Git workspace folder does not exist".to_string());
    }
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .current_dir(path)
        .output()
        .map_err(|error| format!("Could not start git: {error}"))?;
    if output.stdout.len() + output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err("Git output exceeds 4 MB".to_string());
    }
    Ok(output.status.success() && String::from_utf8_lossy(&output.stdout).trim().eq("true"))
}

fn git_has_head(repo: &Path) -> Result<bool, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--verify", "HEAD"])
        .current_dir(repo)
        .output()
        .map_err(|error| format!("Could not start git: {error}"))?;
    if output.stdout.len() + output.stderr.len() > MAX_GIT_OUTPUT_BYTES {
        return Err("Git output exceeds 4 MB".to_string());
    }
    Ok(output.status.success())
}

fn non_repository_snapshot(path: &Path) -> Result<GitWorkspaceSnapshot, String> {
    if !path.is_dir() {
        return Err("Git workspace folder does not exist".to_string());
    }
    let root = path.canonicalize().map_err(|error| error.to_string())?;
    Ok(GitWorkspaceSnapshot {
        repo_path: root.to_string_lossy().into_owned(),
        is_repository: false,
        branch: String::new(),
        dirty: false,
        commits: Vec::new(),
        graph_patch: Value::Null,
    })
}

fn initialize_git_repository(path: &Path) -> Result<PathBuf, String> {
    if !path.is_dir() {
        return Err("Git workspace folder does not exist".to_string());
    }
    if is_git_repository(path)? {
        return resolve_repo(path);
    }
    if git_output(path, &["init", "--initial-branch=main"]).is_err() {
        git_output(path, &["init"])?;
    }
    resolve_repo(path)
}

fn parse_git_log(output: &str) -> Vec<GitCommitRecord> {
    output
        .split('\u{1e}')
        .filter_map(|record| {
            let fields = record.trim().splitn(7, '\u{1f}').collect::<Vec<_>>();
            if fields.len() != 7 || fields[0].is_empty() {
                return None;
            }
            Some(GitCommitRecord {
                id: fields[0].to_string(),
                short_id: fields[1].to_string(),
                parents: fields[2]
                    .split_whitespace()
                    .map(ToOwned::to_owned)
                    .collect(),
                author: fields[3].to_string(),
                timestamp: fields[4].to_string(),
                refs: fields[5]
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect(),
                message: fields[6].trim().to_string(),
            })
        })
        .collect()
}

/// 在仓库内定位项目文件（.research-canvas/ 优先，其次根目录的 .mycproj），
/// 返回仓库相对路径。JSON 仅在 .research-canvas/ 下接受，避免误抓 package.json 等。
fn find_project_file(repo: &Path) -> Option<PathBuf> {
    let mut searches: Vec<PathBuf> = vec![repo.join(".research-canvas")];
    if let Ok(entries) = fs::read_dir(repo) {
        let mut roots: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path.extension().and_then(|value| value.to_str()) == Some("mycproj")
            })
            .collect();
        roots.sort();
        searches.extend(roots);
    }
    for dir in searches {
        if !dir.is_dir() {
            continue;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut candidates: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                if !path.is_file() {
                    return false;
                }
                let ext = path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("");
                matches!(ext, "mycproj" | "json")
            })
            .collect();
        candidates.sort();
        if let Some(file) = candidates.into_iter().next() {
            return file.strip_prefix(repo).ok().map(PathBuf::from);
        }
    }
    None
}

/// `git show <rev>:<relative>` 读取文件在某次提交的版本；不存在则返回 None。
fn git_show_file(repo: &Path, rev: &str, relative: &Path) -> Option<String> {
    // git 接受正斜杠路径，与平台无关。
    let spec = format!("{rev}:{}", relative.to_string_lossy().replace('\\', "/"));
    let output = git_output(repo, &["show", &spec]).ok()?;
    if output.trim().is_empty() {
        return None;
    }
    Some(output)
}

fn empty_project() -> Value {
    json!({
        "schemaVersion": 2, "id": "", "title": "", "discipline": "",
        "nodes": [], "edges": [], "evidence": [],
        "placements": [], "scenarios": [], "activity": []
    })
}

/// 合并多 commit 的 diff 操作：add 按实体 id 去重（保留首个），update 保留末个。
fn dedup_operations(operations: Vec<Value>) -> Vec<Value> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut result: Vec<Value> = Vec::new();
    let mut last_update: HashMap<String, Value> = HashMap::new();
    let mut update_order: Vec<String> = Vec::new();
    for operation in operations {
        let op_type = operation.get("op").and_then(Value::as_str).unwrap_or("");
        if op_type == "add-node" || op_type == "add-edge" {
            let pointer = if op_type == "add-node" {
                "node.id"
            } else {
                "edge.id"
            };
            let id = operation
                .pointer(pointer)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            if seen.insert(id) {
                result.push(operation);
            }
        } else {
            let key = if op_type == "update-node" {
                "nodeId"
            } else {
                "edgeId"
            };
            let id = operation
                .get(key)
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_default();
            if !last_update.contains_key(&id) {
                update_order.push(id.clone());
            }
            last_update.insert(id, operation);
        }
    }
    result.extend(
        update_order
            .into_iter()
            .filter_map(|id| last_update.remove(&id)),
    );
    result
}

/// 基线（commit → evidence 节点 + parent 边）之上，用编译器解析项目文件
/// git diff（§6 版控差异 → GraphPatch），取代旧的 commit message 约定指令。
fn graph_patch_from_commits(plugin_id: &str, repo: &Path, commits: &[GitCommitRecord]) -> Value {
    let mut operations = Vec::new();
    let short_ids = commits
        .iter()
        .map(|commit| (commit.id.as_str(), commit.short_id.as_str()))
        .collect::<HashMap<_, _>>();
    for commit in commits {
        let commit_node_id = format!("git-{}", commit.short_id);
        operations.push(json!({
            "op": "add-node",
            "node": {
                "id": commit_node_id,
                "type": "evidence",
                "title": commit.message.lines().next().unwrap_or(&commit.short_id),
                "body": format!("{} · {}", commit.author, commit.timestamp),
                "tags": commit.refs,
                "data": { "gitCommit": commit.id, "gitRefs": commit.refs }
            }
        }));
        for parent in &commit.parents {
            let parent_short_id = short_ids
                .get(parent.as_str())
                .copied()
                .unwrap_or_else(|| &parent[..parent.len().min(8)]);
            operations.push(json!({
                "op": "add-edge",
                "edge": {
                    "id": format!("git-parent-{}-{}", commit.short_id, parent_short_id),
                    "source": format!("git-{}", parent_short_id),
                    "target": format!("git-{}", commit.short_id),
                    "type": "derived_from",
                    "note": "git parent"
                }
            }));
        }
    }

    // 主路径：解析项目文件 git diff，聚合为去重后的 GraphPatch 操作。
    let mut diff_operations = Vec::new();
    let mut parsed = 0usize;
    if let Some(relative) = find_project_file(repo) {
        for commit in commits {
            let Some(new_text) = git_show_file(repo, &commit.id, &relative) else {
                continue;
            };
            let Ok(new_value) = serde_json::from_str::<Value>(&new_text) else {
                continue;
            };
            let old_value = commit
                .parents
                .first()
                .and_then(|parent| git_show_file(repo, parent, &relative))
                .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                .unwrap_or_else(empty_project);
            let patch = graph_compiler::graph_patch_from_diff(
                &old_value,
                &new_value,
                plugin_id,
                "git-history-import",
                "Git history research graph",
            );
            if let Some(ops) = patch.get("operations").and_then(Value::as_array) {
                diff_operations.extend(ops.iter().cloned());
                parsed += 1;
            }
        }
    }
    let diff_count = diff_operations.len();
    operations.extend(dedup_operations(diff_operations));

    let summary = if parsed > 0 {
        format!(
            "{} commits; {} structural changes from {} parsed project diffs",
            commits.len(),
            diff_count,
            parsed
        )
    } else {
        format!("{} commits and their git history evidence", commits.len())
    };
    json!({
        "apiVersion": "researchcanvas.dev/graph-patch/v1alpha1",
        "source": { "pluginId": plugin_id, "operation": "git-history-import" },
        "title": "Git history research graph",
        "summary": summary,
        "reviewRequired": true,
        "operations": operations
    })
}

fn git_snapshot(plugin_id: &str, repo: &Path) -> Result<GitWorkspaceSnapshot, String> {
    let root = resolve_repo(repo)?;
    let branch = git_output(&root, &["branch", "--show-current"])?;
    let status = git_output(&root, &["status", "--porcelain"])?;
    let log = if git_has_head(&root)? {
        git_output(
            &root,
            &[
                "log",
                "--all",
                "-n",
                "200",
                "--date=iso-strict",
                "--pretty=format:%H%x1f%h%x1f%P%x1f%an%x1f%aI%x1f%D%x1f%B%x1e",
            ],
        )?
    } else {
        String::new()
    };
    let commits = parse_git_log(&log);
    let graph_patch = if commits.is_empty() {
        Value::Null
    } else {
        graph_patch_from_commits(plugin_id, &root, &commits)
    };
    Ok(GitWorkspaceSnapshot {
        repo_path: root.to_string_lossy().into_owned(),
        is_repository: true,
        branch: branch.trim().to_string(),
        dirty: !status.trim().is_empty(),
        commits,
        graph_patch,
    })
}

#[tauri::command]
pub fn read_git_workspace(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    path: String,
) -> Result<GitWorkspaceSnapshot, String> {
    crate::plugins::require_plugin_capabilities(
        &app,
        &plugin_id,
        &plugin_version,
        &["git.repository.read", "graph.patch.propose"],
    )?;
    let path = Path::new(&path);
    if is_git_repository(path)? {
        git_snapshot(&plugin_id, path)
    } else {
        non_repository_snapshot(path)
    }
}

#[tauri::command]
pub fn initialize_git_workspace(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    path: String,
) -> Result<GitWorkspaceSnapshot, String> {
    crate::plugins::require_plugin_capabilities(
        &app,
        &plugin_id,
        &plugin_version,
        &[
            "git.repository.init",
            "git.repository.read",
            "graph.patch.propose",
        ],
    )?;
    let root = initialize_git_repository(Path::new(&path))?;
    git_snapshot(&plugin_id, &root)
}

#[tauri::command]
pub fn read_github_account(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
) -> Result<GitHubAccountStatus, String> {
    crate::plugins::require_plugin_capability(
        &app,
        &plugin_id,
        &plugin_version,
        "git.account.read",
    )?;
    Ok(github_account_status())
}

#[tauri::command]
pub async fn login_github_account(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
) -> Result<GitHubAccountStatus, String> {
    crate::plugins::require_plugin_capabilities(
        &app,
        &plugin_id,
        &plugin_version,
        &["git.account.login", "git.account.read"],
    )?;
    tauri::async_runtime::spawn_blocking(|| {
        let output = bounded_command_output(
            "gh",
            &[
                "auth",
                "login",
                "--hostname",
                "github.com",
                "--web",
                "--git-protocol",
                "ssh",
                "--skip-ssh-key",
            ],
        )?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        let setup =
            bounded_command_output("gh", &["auth", "setup-git", "--hostname", "github.com"])?;
        if !setup.status.success() {
            return Err(String::from_utf8_lossy(&setup.stderr).trim().to_string());
        }
        Ok(github_account_status())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn generate_github_ssh_key(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    comment: String,
) -> Result<GitHubAccountStatus, String> {
    crate::plugins::require_plugin_capabilities(
        &app,
        &plugin_id,
        &plugin_version,
        &["git.ssh.generate", "git.account.read"],
    )?;
    generate_managed_ssh_key(&comment)?;
    Ok(github_account_status())
}

#[tauri::command]
pub async fn upload_github_ssh_key(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    path: String,
) -> Result<GitHubAccountStatus, String> {
    crate::plugins::require_plugin_capabilities(
        &app,
        &plugin_id,
        &plugin_version,
        &["git.ssh.upload", "git.account.read"],
    )?;
    let key = validated_public_key_path(&path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let key_text = key
            .to_str()
            .ok_or_else(|| "SSH public key path is not valid UTF-8".to_string())?;
        let machine = env::var("COMPUTERNAME").unwrap_or_else(|_| "device".to_string());
        let title = format!("Research Canvas on {machine}");
        let output = bounded_command_output(
            "gh",
            &[
                "ssh-key",
                "add",
                key_text,
                "--title",
                &title,
                "--type",
                "authentication",
            ],
        )?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        Ok(github_account_status())
    })
    .await
    .map_err(|error| error.to_string())?
}

fn safe_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    let components = path.components().collect::<Vec<_>>();
    if path.is_absolute()
        || components.len() != 2
        || components[0] != Component::Normal(std::ffi::OsStr::new(".research-canvas"))
        || !matches!(components[1], Component::Normal(_))
    {
        return Err("Git project path must stay inside the selected repository".to_string());
    }
    if path.extension().and_then(|value| value.to_str()) != Some("mycproj") {
        return Err("Git autosave target must use .mycproj".to_string());
    }
    Ok(path)
}

#[tauri::command]
pub fn git_autosave_project(
    app: AppHandle,
    plugin_id: String,
    plugin_version: String,
    repo_path: String,
    project_path: String,
    project: Value,
    message: String,
) -> Result<GitWorkspaceSnapshot, String> {
    crate::plugins::require_plugin_capabilities(
        &app,
        &plugin_id,
        &plugin_version,
        &["git.autosave", "git.repository.read", "graph.patch.propose"],
    )?;
    let root = resolve_repo(Path::new(&repo_path))?;
    let relative = safe_relative_path(&project_path)?;
    let destination = root.join(&relative);
    crate::projects::validate_project(&project)?;
    let bytes = serde_json::to_vec_pretty(&project).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_ARTIFACT_BYTES {
        return Err("Git autosave project exceeds 32 MB".to_string());
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(&destination, bytes).map_err(|error| error.to_string())?;
    let relative_text = relative.to_string_lossy();
    git_output(&root, &["add", "--", relative_text.as_ref()])?;
    let staged = git_output(
        &root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "--",
            relative_text.as_ref(),
        ],
    )?;
    if !staged.trim().is_empty() {
        let commit_message = if message.trim().is_empty() {
            "Research Canvas autosave"
        } else {
            message.trim()
        };
        if commit_message.chars().count() > 240 {
            return Err("Git autosave message exceeds 240 characters".to_string());
        }
        git_output(
            &root,
            &[
                "-c",
                "user.name=Research Canvas",
                "-c",
                "user.email=research-canvas@localhost",
                "commit",
                "-m",
                commit_message,
                "--",
                relative_text.as_ref(),
            ],
        )?;
    }
    git_snapshot(&plugin_id, &root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn git(repo: &Path, arguments: &[&str]) {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(repo)
            .output()
            .expect("git is installed for repository integration tests");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn git_history_links_commits_as_evidence_without_project_files() {
        let root = tempdir().expect("temporary git repository");
        git(root.path(), &["init"]);
        git(
            root.path(),
            &["config", "user.name", "Research Canvas Test"],
        );
        git(
            root.path(),
            &["config", "user.email", "canvas@example.invalid"],
        );
        fs::write(root.path().join("README.md"), "PINN architecture\n").expect("fixture file");
        git(root.path(), &["add", "README.md"]);
        git(
            root.path(),
            &["commit", "-m", "PINN Fourier embedding ablation"],
        );
        git(root.path(), &["tag", "research/pinn-fourier-off"]);
        let snapshot =
            git_snapshot("myc.git-workspace", root.path()).expect("read git research snapshot");
        assert!(snapshot.is_repository);
        assert_eq!(snapshot.commits.len(), 1);
        assert!(snapshot.commits[0]
            .refs
            .iter()
            .any(|reference| reference.contains("research/pinn-fourier-off")));
        let operations = snapshot.graph_patch["operations"]
            .as_array()
            .expect("graph patch operations");
        // 无项目文件：仅生成 commit → evidence 节点，不再解析 commit message 约定指令。
        assert!(operations.iter().any(|operation| {
            operation["op"] == "add-node"
                && operation["node"]["type"] == "evidence"
                && operation["node"]["id"]
                    .as_str()
                    .unwrap()
                    .starts_with("git-")
        }));
        assert!(!operations
            .iter()
            .any(|operation| { operation["node"]["type"] == "experiment" }));
    }

    #[test]
    fn git_history_parses_project_file_diffs_through_the_compiler() {
        let root = tempdir().expect("temporary git repository");
        git(root.path(), &["init"]);
        git(
            root.path(),
            &["config", "user.name", "Research Canvas Test"],
        );
        git(
            root.path(),
            &["config", "user.email", "canvas@example.invalid"],
        );
        fs::create_dir_all(root.path().join(".research-canvas")).expect("autosave directory");
        let project = json!({
            "schemaVersion": 2, "id": "pinn", "title": "PINN", "discipline": "Physics",
            "nodes": [
                {"id": "n1", "type": "question", "title": "主问题", "body": "如何建模?", "tags": [], "data": {},
                 "evidenceIds": [], "status": "confirmed", "provenance": {}},
                {"id": "n2", "type": "concept", "title": "先验约束", "body": "物理守恒", "tags": [], "data": {},
                 "evidenceIds": [], "status": "confirmed", "provenance": {}}
            ],
            "edges": [
                {"id": "x1", "type": "supports", "source": "n1", "target": "n2", "directed": true,
                 "polarity": "positive", "confidence": 0.9, "conditions": [], "evidenceIds": [], "provenance": {}}
            ],
            "evidence": [], "placements": [], "scenarios": [], "activity": []
        });
        let path = root.path().join(".research-canvas/pinn.mycproj");
        fs::write(&path, serde_json::to_string_pretty(&project).unwrap()).expect("write project");
        git(root.path(), &["add", ".research-canvas/pinn.mycproj"]);
        git(root.path(), &["commit", "-m", "feat: initial PINN project"]);

        let mut edited = project.clone();
        edited["nodes"].as_array_mut().unwrap().push(json!({
            "id": "n3", "type": "concept", "title": "新增", "body": "b", "tags": [], "data": {},
            "evidenceIds": [], "status": "draft", "provenance": {}
        }));
        edited["nodes"][0]["body"] = json!("改过的正文");
        fs::write(&path, serde_json::to_string_pretty(&edited).unwrap()).expect("rewrite project");
        git(root.path(), &["add", ".research-canvas/pinn.mycproj"]);
        git(
            root.path(),
            &["commit", "-m", "refactor: extend the PINN graph"],
        );

        let snapshot =
            git_snapshot("myc.git-workspace", root.path()).expect("read git research snapshot");
        let operations = snapshot.graph_patch["operations"]
            .as_array()
            .expect("graph patch operations");
        assert!(snapshot.graph_patch["summary"]
            .as_str()
            .unwrap()
            .contains("structural changes"));
        // 第二次提交引入 n3 → add-node；第一次提交 n1 的 body 变化 → update-node。
        assert!(operations
            .iter()
            .any(|operation| { operation["op"] == "add-node" && operation["node"]["id"] == "n3" }));
        assert!(operations.iter().any(|operation| {
            operation["op"] == "update-node"
                && operation["nodeId"] == "n1"
                && operation["changes"]["body"] == "改过的正文"
        }));
        // 基线 commit → evidence 节点仍然存在。
        assert!(operations.iter().any(|operation| operation["node"]["id"]
            .as_str()
            .unwrap()
            .starts_with("git-")));
    }

    #[test]
    fn non_repository_can_be_opened_and_initialized_without_an_error_overlay() {
        let root = tempdir().expect("temporary workspace folder");
        let placeholder = non_repository_snapshot(root.path()).expect("non-repository snapshot");
        assert!(!placeholder.is_repository);
        assert!(placeholder.commits.is_empty());
        assert!(placeholder.graph_patch.is_null());

        let repository = initialize_git_repository(root.path()).expect("initialize repository");
        let snapshot =
            git_snapshot("myc.git-workspace", &repository).expect("empty repository snapshot");
        assert!(snapshot.is_repository);
        assert!(snapshot.commits.is_empty());
        assert!(snapshot.graph_patch.is_null());
    }

    #[test]
    fn git_autosave_path_is_limited_to_one_research_canvas_snapshot() {
        assert_eq!(
            safe_relative_path(".research-canvas/pinn.mycproj")
                .expect("valid bounded autosave path"),
            PathBuf::from(".research-canvas/pinn.mycproj")
        );
        for path in [
            "pinn.mycproj",
            ".research-canvas/nested/pinn.mycproj",
            ".research-canvas/../pinn.mycproj",
            "../.research-canvas/pinn.mycproj",
            ".research-canvas/pinn.json",
        ] {
            assert!(safe_relative_path(path).is_err(), "must reject {path}");
        }
    }

    #[test]
    fn github_account_status_is_token_free_and_ssh_comments_are_bounded() {
        let status = github_account_status();
        assert_eq!(status.host, "github.com");
        let serialized = serde_json::to_value(status).expect("serialize bounded account status");
        assert!(serialized.get("token").is_none());
        assert!(serialized.get("scopes").is_none());
        assert_eq!(
            validate_ssh_comment("researcher@github.com").unwrap(),
            "researcher@github.com"
        );
        assert!(validate_ssh_comment("line one\nline two").is_err());
        assert!(validate_ssh_comment("").is_err());
    }

    #[test]
    fn managed_ed25519_generation_is_bounded_and_never_overwrites() {
        if !command_available("ssh-keygen", "-?") {
            return;
        }
        let root = tempdir().expect("temporary SSH folder");
        generate_ssh_key_at(root.path(), "canvas@example.invalid")
            .expect("generate isolated Ed25519 fixture");
        let private_key = root.path().join("research_canvas_ed25519");
        let public_key = root.path().join("research_canvas_ed25519.pub");
        assert!(private_key.is_file());
        assert!(public_key.is_file());
        assert!(fs::read_to_string(public_key)
            .expect("public key text")
            .starts_with("ssh-ed25519 "));
        assert!(generate_ssh_key_at(root.path(), "canvas@example.invalid").is_err());
    }

    #[test]
    fn folder_index_reads_the_shared_pinn_fixture() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/pinn-architecture.mycproj");
        let summary = summarize_project_file(&fixture).expect("PINN project summary");
        assert_eq!(summary.title, "PINN 网络架构与消融设计");
        assert_eq!(summary.node_count, 12);
        assert_eq!(summary.edge_count, 11);
        assert_eq!(summary.revision, 7);
    }

    #[test]
    fn folder_tree_path_stays_inside_the_selected_root() {
        let root = tempdir().expect("temporary folder root");
        let nested = root.path().join("nested");
        fs::create_dir_all(&nested).expect("nested folder");
        let (canonical_root, canonical_nested) =
            validate_folder_tree_path(root.path(), &nested).expect("nested path is allowed");
        assert_eq!(canonical_root, root.path().canonicalize().unwrap());
        assert_eq!(canonical_nested, nested.canonicalize().unwrap());

        let outside = root.path().join("..").join("outside");
        assert!(validate_folder_tree_path(root.path(), &outside).is_err());
        assert!(validate_folder_tree_path(root.path(), &root.path().join("missing")).is_err());
    }
}
