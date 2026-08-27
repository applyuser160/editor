use crate::debug_config::{self, DebugConfiguration};
use crate::debug_session::{self, DebugSessionState, SourceBreakpoint};
use crate::extension_host::{ExtensionHostState, ExtensionManifest};
use crate::lsp_client::LspState;
use crate::pty_manager::PtyState;
use crate::settings_store::{self, SettingsSnapshot};
use crate::task_runner::{load_tasks, run_task, TaskDefinition, TaskExecutionResult};
use crate::test_runner::{discover_test_suites, run_test_suite, TestSuite};
use crate::workspace::{
    WorkspaceExcludes, WorkspaceFilter, WorkspaceFilterTarget, WorkspaceInfo, WorkspaceState,
    WorkspaceTrust,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, State};

const MAX_EXTENSION_DOWNLOAD_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileStat {
    pub size: u64,
    pub is_dir: bool,
    pub extension: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub file_path: String,
    pub line_number: usize,
    pub line_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitStatusResult {
    pub branch: String,
    pub changed_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenVsxExtension {
    pub namespace: String,
    pub name: String,
    pub version: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub download_count: Option<u64>,
    pub icon_url: Option<String>,
    pub download_url: Option<String>,
    pub url: Option<String>,
}

#[tauri::command]
pub async fn lsp_start_server(
    app: AppHandle,
    state: State<'_, LspState>,
    workspace: State<'_, WorkspaceState>,
    lang: String,
    _workspace_root: String,
) -> Result<String, String> {
    workspace.require_trusted()?;
    state.start_server(app, &lang, &workspace.root().to_string_lossy())
}

#[tauri::command]
pub async fn lsp_send_notification(
    state: State<'_, LspState>,
    lang: String,
    method: String,
    params: Value,
) -> Result<(), String> {
    state.send_notification(&lang, &method, params)
}

#[tauri::command]
pub async fn lsp_stop_all(state: State<'_, LspState>) -> Result<usize, String> {
    Ok(state.stop_all())
}

#[tauri::command]
pub async fn lsp_send_request(
    state: State<'_, LspState>,
    lang: String,
    method: String,
    params: Value,
) -> Result<Value, String> {
    state.send_request(&lang, &method, params).await
}

#[tauri::command]
pub async fn search_openvsx_extensions(query: String) -> Result<Vec<OpenVsxExtension>, String> {
    let q = query.trim();
    let url = if q.is_empty() {
        "https://open-vsx.org/api/-/search?size=15&sortBy=downloadCount&sortOrder=desc".to_string()
    } else {
        format!(
            "https://open-vsx.org/api/-/search?query={}&size=15",
            urlencoding_simple(q)
        )
    };

    let client = reqwest::Client::builder()
        .user_agent("Oxide-Editor/0.1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Network request failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Open VSX returned HTTP {}", resp.status()));
    }

    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;
    let mut results = Vec::new();

    if let Some(extensions) = json.get("extensions").and_then(|e| e.as_array()) {
        for ext in extensions {
            let namespace = ext
                .get("namespace")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = ext
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let version = ext
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("1.0.0")
                .to_string();
            let display_name = ext
                .get("displayName")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let description = ext
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let download_count = ext.get("downloadCount").and_then(|v| v.as_u64());

            let files = ext.get("files");
            let icon_url = files
                .and_then(|f| f.get("icon"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let download_url = files
                .and_then(|f| f.get("download"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let url = ext
                .get("url")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            results.push(OpenVsxExtension {
                namespace,
                name,
                version,
                display_name,
                description,
                download_count,
                icon_url,
                download_url,
                url,
            });
        }
    }

    Ok(results)
}

fn validate_openvsx_download_url(raw_url: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(raw_url)
        .map_err(|_| "Open VSX returned an invalid download URL".to_string())?;
    if url.scheme() != "https" {
        return Err("Extension downloads must use HTTPS".to_string());
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if host != "open-vsx.org" && !host.ends_with(".open-vsx.org") {
        return Err("Extension download host is not allowed".to_string());
    }
    Ok(())
}

fn urlencoding_simple(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            encoded.push(b as char);
        } else {
            encoded.push_str(&format!("%{:02X}", b));
        }
    }
    encoded
}

#[tauri::command]
pub async fn install_openvsx_extension(
    state: State<'_, ExtensionHostState>,
    namespace: String,
    name: String,
    _version: String,
    _description: String,
    download_url: Option<String>,
) -> Result<String, String> {
    let id = format!("{}.{}", namespace, name);
    {
        let exts = state.extensions.lock().unwrap();
        if exts.iter().any(|e| e.id == id) {
            return Ok(format!("Extension '{}' is already installed.", id));
        }
    }

    let url =
        download_url.ok_or_else(|| "Open VSX did not provide a VSIX download URL".to_string())?;
    validate_openvsx_download_url(&url)?;
    let client = reqwest::Client::builder()
        .user_agent("Oxide-Editor/0.1.0")
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|error| format!("Failed to download: {}", error))?;
    if !response.status().is_success() {
        return Err(format!(
            "Open VSX download returned HTTP {}",
            response.status()
        ));
    }
    if let Some(content_length) = response.content_length() {
        if content_length > MAX_EXTENSION_DOWNLOAD_BYTES {
            return Err(format!(
                "Extension exceeds the {} MiB download limit",
                MAX_EXTENSION_DOWNLOAD_BYTES / 1024 / 1024
            ));
        }
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read VSIX bytes: {}", error))?;
    let manifest = state.install_vsix(&id, &bytes)?;

    Ok(format!(
        "Extension '{}@{}' installed successfully",
        manifest.id, manifest.version
    ))
}

#[tauri::command]
pub async fn uninstall_extension(
    state: State<'_, ExtensionHostState>,
    id: String,
) -> Result<String, String> {
    state.uninstall(&id)?;
    Ok(format!("Extension '{}' uninstalled successfully.", id))
}

#[tauri::command]
pub async fn set_extension_enabled(
    state: State<'_, ExtensionHostState>,
    id: String,
    enabled: bool,
) -> Result<ExtensionManifest, String> {
    state.set_enabled(&id, enabled)
}

#[tauri::command]
pub async fn git_list_branches(state: State<'_, WorkspaceState>) -> Result<Vec<String>, String> {
    let output = Command::new("git")
        .current_dir(state.root())
        .args(["branch", "--list"])
        .output()
        .map_err(|e| format!("git branch failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let branches = stdout
        .lines()
        .map(|l| l.trim().trim_start_matches('*').trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();

    Ok(branches)
}

#[tauri::command]
pub async fn git_checkout_branch(
    state: State<'_, WorkspaceState>,
    branch: String,
) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(state.root())
        .args(["checkout", branch.trim()])
        .output()
        .map_err(|e| format!("git checkout failed: {}", e))?;

    if output.status.success() {
        Ok(format!("Switched to branch '{}'", branch.trim()))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
pub async fn git_create_branch(
    state: State<'_, WorkspaceState>,
    new_branch: String,
) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(state.root())
        .args(["checkout", "-b", new_branch.trim()])
        .output()
        .map_err(|e| format!("git checkout -b failed: {}", e))?;

    if output.status.success() {
        Ok(format!(
            "Created and switched to branch '{}'",
            new_branch.trim()
        ))
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

#[tauri::command]
pub async fn get_installed_extensions(
    state: State<'_, ExtensionHostState>,
) -> Result<Vec<ExtensionManifest>, String> {
    Ok(state.list_extensions())
}

#[tauri::command]
pub async fn start_extension_sidecar(
    state: State<'_, ExtensionHostState>,
    workspace: State<'_, WorkspaceState>,
) -> Result<String, String> {
    workspace.require_trusted()?;
    state.start_sidecar()
}

#[tauri::command]
pub async fn spawn_pty(
    app: AppHandle,
    state: State<'_, PtyState>,
    workspace: State<'_, WorkspaceState>,
    cols: u16,
    rows: u16,
) -> Result<u32, String> {
    workspace.require_trusted()?;
    state.spawn(app, cols, rows, workspace.root())
}

#[tauri::command]
pub async fn write_pty(state: State<'_, PtyState>, id: u32, data: String) -> Result<(), String> {
    state.write(id, data)
}

#[tauri::command]
pub async fn resize_pty(
    state: State<'_, PtyState>,
    id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.resize(id, cols, rows)
}

#[tauri::command]
pub async fn list_workspace_tasks(
    state: State<'_, WorkspaceState>,
) -> Result<Vec<TaskDefinition>, String> {
    load_tasks(&state.root())
}

#[tauri::command]
pub async fn run_workspace_task(
    state: State<'_, WorkspaceState>,
    label: String,
) -> Result<TaskExecutionResult, String> {
    state.require_trusted()?;
    let workspace_root = state.root();
    let task = load_tasks(&workspace_root)?
        .into_iter()
        .find(|task| task.label == label)
        .ok_or_else(|| format!("Task '{}' was not found", label))?;
    run_task(task, &workspace_root)
}

#[tauri::command]
pub async fn list_workspace_test_suites(
    state: State<'_, WorkspaceState>,
) -> Result<Vec<TestSuite>, String> {
    Ok(discover_test_suites(&state.root()))
}

#[tauri::command]
pub async fn run_workspace_test_suite(
    state: State<'_, WorkspaceState>,
    id: String,
) -> Result<TaskExecutionResult, String> {
    state.require_trusted()?;
    run_test_suite(&state.root(), &id)
}

#[tauri::command]
pub async fn list_workspace_files(
    state: State<'_, WorkspaceState>,
) -> Result<Vec<FileEntry>, String> {
    let workspace_root = state.root();
    let filter = state.filter();
    let mut entries = Vec::new();
    scan_dir_recursive(
        &workspace_root,
        &workspace_root,
        0,
        &mut entries,
        30,
        &filter,
    )?;
    Ok(entries)
}

fn scan_dir_recursive(
    root: &Path,
    current: &Path,
    depth: usize,
    entries: &mut Vec<FileEntry>,
    max_depth: usize,
    filter: &WorkspaceFilter,
) -> Result<(), String> {
    if depth > max_depth {
        return Ok(());
    }

    let read_dir = std::fs::read_dir(current).map_err(|e| e.to_string())?;
    let mut items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    items.sort_by_key(|e| (!e.path().is_dir(), e.file_name()));

    for entry in items {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;

        if file_type.is_symlink()
            || filter.should_exclude(root, &path, WorkspaceFilterTarget::Files)
        {
            continue;
        }

        let is_dir = file_type.is_dir();
        let relative_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();

        entries.push(FileEntry {
            name: file_name,
            path: relative_path,
            is_dir,
            depth,
        });

        if is_dir {
            scan_dir_recursive(root, &path, depth + 1, entries, max_depth, filter)?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn read_file_content(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<String, String> {
    let full_path = get_absolute_path(&state, &path)?;
    let bytes = std::fs::read(&full_path).map_err(|e| format!("Failed to read {}: {}", path, e))?;

    // バイナリファイル判定（先頭8KB内にNULLバイトがあるか、またはUTF-8として不正な場合）
    let sample = &bytes[..std::cmp::min(bytes.len(), 8192)];
    if sample.contains(&0) {
        return Err(format!(
            "BINARY_FILE: '{}' はバイナリファイルのためテキストエディタで開けません",
            path
        ));
    }

    String::from_utf8(bytes).map_err(|_| {
        format!(
            "BINARY_FILE: '{}' はテキストとしてデコードできません（非UTF-8またはバイナリ）",
            path
        )
    })
}

#[tauri::command]
pub async fn write_file_content(
    state: State<'_, WorkspaceState>,
    path: String,
    content: String,
) -> Result<(), String> {
    let full_path = get_absolute_path(&state, &path)?;
    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&full_path, content).map_err(|e| format!("Failed to write {}: {}", path, e))
}

#[tauri::command]
pub async fn create_file(state: State<'_, WorkspaceState>, path: String) -> Result<(), String> {
    let full_path = get_absolute_path(&state, &path)?;
    if full_path.exists() {
        return Err(format!("File '{}' already exists", path));
    }
    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&full_path, "").map_err(|e| format!("Failed to create file {}: {}", path, e))
}

#[tauri::command]
pub async fn create_directory(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<(), String> {
    let full_path = get_absolute_path(&state, &path)?;
    std::fs::create_dir_all(&full_path).map_err(|e| format!("Failed to create dir {}: {}", path, e))
}

#[tauri::command]
pub async fn delete_file(state: State<'_, WorkspaceState>, path: String) -> Result<(), String> {
    let full_path = get_absolute_path(&state, &path)?;
    if full_path.is_dir() {
        std::fs::remove_dir_all(&full_path)
            .map_err(|e| format!("Failed to delete dir {}: {}", path, e))
    } else {
        std::fs::remove_file(&full_path)
            .map_err(|e| format!("Failed to delete file {}: {}", path, e))
    }
}

#[tauri::command]
pub async fn rename_file(
    state: State<'_, WorkspaceState>,
    old_path: String,
    new_path: String,
) -> Result<(), String> {
    let src = get_absolute_path(&state, &old_path)?;
    let dst = get_absolute_path(&state, &new_path)?;
    if let Some(parent) = dst.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::rename(&src, &dst)
        .map_err(|e| format!("Failed to rename {} to {}: {}", old_path, new_path, e))
}

#[tauri::command]
pub async fn reveal_in_os_explorer(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<(), String> {
    let full_path = get_absolute_path(&state, &path)?;
    #[cfg(target_os = "windows")]
    {
        let path_str = full_path.to_string_lossy().to_string().replace('/', "\\");
        let _ = Command::new("explorer")
            .arg(format!("/select,{}", path_str))
            .spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg("-R").arg(full_path).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let parent = full_path.parent().unwrap_or(&full_path);
        let _ = Command::new("xdg-open").arg(parent).spawn();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_file_stat(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<FileStat, String> {
    let full_path = get_absolute_path(&state, &path)?;
    let metadata = std::fs::metadata(&full_path).map_err(|e| e.to_string())?;
    let extension = full_path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(FileStat {
        size: metadata.len(),
        is_dir: metadata.is_dir(),
        extension,
    })
}

#[tauri::command]
pub async fn search_in_workspace(
    state: State<'_, WorkspaceState>,
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    is_regex: bool,
) -> Result<Vec<SearchMatch>, String> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    let pattern_str = if is_regex {
        query.clone()
    } else {
        regex::escape(&query)
    };

    let final_pattern = if whole_word {
        format!(r"\b{}\b", pattern_str)
    } else {
        pattern_str
    };

    let re = regex::RegexBuilder::new(&final_pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let workspace_root = state.root();
    let filter = state.filter();
    let mut matches = Vec::new();
    search_recursive_regex(
        &workspace_root,
        &workspace_root,
        &re,
        &mut matches,
        20,
        &filter,
    )?;
    Ok(matches)
}

fn search_recursive_regex(
    root: &Path,
    current: &Path,
    re: &regex::Regex,
    matches: &mut Vec<SearchMatch>,
    max_depth: usize,
    filter: &WorkspaceFilter,
) -> Result<(), String> {
    if max_depth == 0 {
        return Ok(());
    }

    let read_dir = std::fs::read_dir(current).map_err(|e| e.to_string())?;
    for entry in read_dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;

        if file_type.is_symlink()
            || filter.should_exclude(root, &path, WorkspaceFilterTarget::Search)
        {
            continue;
        }

        if file_type.is_dir() {
            search_recursive_regex(root, &path, re, matches, max_depth - 1, filter)?;
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            let relative_path = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();
            for (idx, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    matches.push(SearchMatch {
                        file_path: relative_path.clone(),
                        line_number: idx + 1,
                        line_text: line.trim().to_string(),
                    });
                    if matches.len() >= 1000 {
                        return Ok(());
                    }
                }
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn replace_in_workspace(
    state: State<'_, WorkspaceState>,
    query: String,
    replace_text: String,
    case_sensitive: bool,
    whole_word: bool,
    is_regex: bool,
) -> Result<usize, String> {
    if query.trim().is_empty() {
        return Ok(0);
    }

    let pattern_str = if is_regex {
        query.clone()
    } else {
        regex::escape(&query)
    };

    let final_pattern = if whole_word {
        format!(r"\b{}\b", pattern_str)
    } else {
        pattern_str
    };

    let re = regex::RegexBuilder::new(&final_pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|e| format!("Invalid regex: {}", e))?;

    let workspace_root = state.root();
    let filter = state.filter();
    let mut total_replaced = 0;
    replace_recursive_regex(
        &workspace_root,
        &workspace_root,
        &re,
        &replace_text,
        &mut total_replaced,
        20,
        &filter,
    )?;
    Ok(total_replaced)
}

fn replace_recursive_regex(
    root: &Path,
    current: &Path,
    re: &regex::Regex,
    replace_text: &str,
    total_replaced: &mut usize,
    max_depth: usize,
    filter: &WorkspaceFilter,
) -> Result<(), String> {
    if max_depth == 0 {
        return Ok(());
    }

    let read_dir = std::fs::read_dir(current).map_err(|e| e.to_string())?;
    for entry in read_dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;

        if file_type.is_symlink()
            || filter.should_exclude(root, &path, WorkspaceFilterTarget::Search)
        {
            continue;
        }

        if file_type.is_dir() {
            replace_recursive_regex(
                root,
                &path,
                re,
                replace_text,
                total_replaced,
                max_depth - 1,
                filter,
            )?;
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            if re.is_match(&content) {
                let replaced = re.replace_all(&content, replace_text).to_string();
                let count = re.find_iter(&content).count();
                *total_replaced += count;
                let _ = std::fs::write(&path, replaced);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn git_get_status(state: State<'_, WorkspaceState>) -> Result<GitStatusResult, String> {
    let workspace_root = state.root();
    let branch_out = Command::new("git")
        .current_dir(&workspace_root)
        .args(["branch", "--show-current"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "main".to_string());

    let status_out = Command::new("git")
        .current_dir(&workspace_root)
        .args(["status", "--porcelain"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let changed_files = status_out
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();

    Ok(GitStatusResult {
        branch: if branch_out.is_empty() {
            "main".to_string()
        } else {
            branch_out
        },
        changed_files,
    })
}

#[tauri::command]
pub async fn git_commit(
    state: State<'_, WorkspaceState>,
    message: String,
) -> Result<String, String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err("Commit message cannot be empty".to_string());
    }

    let commit_res = Command::new("git")
        .current_dir(state.root())
        .args(["commit", "-m", trimmed])
        .output()
        .map_err(|e| format!("git commit failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&commit_res.stdout).to_string();
    let stderr = String::from_utf8_lossy(&commit_res.stderr).to_string();

    if commit_res.status.success() {
        Ok(stdout)
    } else {
        Err(format!("{}\n{}", stdout, stderr))
    }
}

#[tauri::command]
pub async fn git_push(state: State<'_, WorkspaceState>) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(state.root())
        .args(["push"])
        .output()
        .map_err(|e| format!("git push failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        Ok(format!("{}\n{}", stdout, stderr).trim().to_string())
    } else {
        Err(format!("{}\n{}", stdout, stderr))
    }
}

#[tauri::command]
pub async fn git_pull(state: State<'_, WorkspaceState>) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(state.root())
        .args(["pull"])
        .output()
        .map_err(|e| format!("git pull failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();

    if out.status.success() {
        Ok(format!("{}\n{}", stdout, stderr).trim().to_string())
    } else {
        Err(format!("{}\n{}", stdout, stderr))
    }
}

#[tauri::command]
pub async fn git_stage_file(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(state.root())
        .args(["add", &path])
        .output()
        .map_err(|e| format!("git add failed: {}", e))?;

    if out.status.success() {
        Ok("Staged".to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[tauri::command]
pub async fn git_unstage_file(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<String, String> {
    let out = Command::new("git")
        .current_dir(state.root())
        .args(["restore", "--staged", &path])
        .output()
        .map_err(|e| format!("git restore failed: {}", e))?;

    if out.status.success() {
        Ok("Unstaged".to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[tauri::command]
pub async fn get_workspace_path(state: State<'_, WorkspaceState>) -> Result<String, String> {
    Ok(state.info().root)
}

#[tauri::command]
pub async fn get_workspace_folders(
    state: State<'_, WorkspaceState>,
) -> Result<Vec<WorkspaceInfo>, String> {
    Ok(state.folders())
}

#[tauri::command]
pub async fn set_workspace_root(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<WorkspaceInfo, String> {
    state.set_root(&path)
}

#[tauri::command]
pub async fn add_workspace_folder(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<Vec<WorkspaceInfo>, String> {
    state.add_folder(&path)
}

#[tauri::command]
pub async fn remove_workspace_folder(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<Vec<WorkspaceInfo>, String> {
    state.remove_folder(&path)
}

#[tauri::command]
pub async fn select_workspace_folder(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<WorkspaceInfo, String> {
    state.select_folder(&path)
}

#[tauri::command]
pub async fn get_workspace_trust(
    state: State<'_, WorkspaceState>,
) -> Result<WorkspaceTrust, String> {
    Ok(state.trust())
}

#[tauri::command]
pub async fn set_workspace_trust(
    state: State<'_, WorkspaceState>,
    trusted: bool,
) -> Result<WorkspaceTrust, String> {
    state.set_trusted(trusted)
}

#[tauri::command]
pub async fn get_workspace_excludes(
    state: State<'_, WorkspaceState>,
) -> Result<WorkspaceExcludes, String> {
    Ok(state.excludes())
}

#[tauri::command]
pub async fn set_workspace_excludes(
    state: State<'_, WorkspaceState>,
    files: Vec<String>,
    search: Vec<String>,
) -> Result<WorkspaceExcludes, String> {
    state.set_excludes(files, search)
}

#[tauri::command]
pub async fn list_recent_workspaces(
    state: State<'_, WorkspaceState>,
) -> Result<Vec<WorkspaceInfo>, String> {
    Ok(state.recent())
}

#[tauri::command]
pub async fn remove_recent_workspace(
    state: State<'_, WorkspaceState>,
    path: String,
) -> Result<(), String> {
    state.remove_recent(&path)
}

fn get_absolute_path(state: &WorkspaceState, path_str: &str) -> Result<PathBuf, String> {
    state.resolve_path(path_str)
}

#[tauri::command]
pub async fn find_rust_stdlib_definition(symbol: String) -> Result<Option<SearchMatch>, String> {
    let output = Command::new("rustc").arg("--print").arg("sysroot").output();

    let sysroot = match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => return Ok(None),
    };

    let stdlib_path = Path::new(&sysroot)
        .join("lib")
        .join("rustlib")
        .join("src")
        .join("rust")
        .join("library");

    if !stdlib_path.exists() {
        return Ok(None);
    }

    let target_crates = ["alloc", "core", "std"];
    let patterns = [
        format!("pub struct {} ", symbol),
        format!("pub struct {}<", symbol),
        format!("pub struct {}{{", symbol),
        format!("pub struct {}\n", symbol),
        format!("pub struct {}\r", symbol),
        format!("pub enum {} ", symbol),
        format!("pub enum {}<", symbol),
        format!("pub enum {}{{", symbol),
        format!("pub trait {} ", symbol),
        format!("pub trait {}<", symbol),
        format!("pub trait {}{{", symbol),
        format!("pub type {} ", symbol),
        format!("pub type {}<", symbol),
        format!("pub type {}=", symbol),
        format!("pub fn {} ", symbol),
        format!("pub fn {}<", symbol),
        format!("pub fn {}(", symbol),
    ];

    for crate_name in &target_crates {
        let src_dir = stdlib_path.join(crate_name).join("src");
        if !src_dir.exists() {
            continue;
        }

        let direct_file = src_dir.join(format!("{}.rs", symbol.to_lowercase()));
        if direct_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&direct_file) {
                for (idx, line) in content.lines().enumerate() {
                    let trimmed = line.trim();
                    if patterns.iter().any(|pat| trimmed.starts_with(pat.trim())) {
                        return Ok(Some(SearchMatch {
                            file_path: direct_file.to_string_lossy().to_string(),
                            line_number: idx + 1,
                            line_text: line.to_string(),
                        }));
                    }
                }
            }
        }

        if let Ok(entries) = walk_rs_files(&src_dir) {
            for entry in entries {
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    if !content.contains(&symbol) {
                        continue;
                    }
                    for (idx, line) in content.lines().enumerate() {
                        let trimmed = line.trim();
                        if patterns.iter().any(|pat| trimmed.starts_with(pat.trim())) {
                            return Ok(Some(SearchMatch {
                                file_path: entry.to_string_lossy().to_string(),
                                line_number: idx + 1,
                                line_text: line.to_string(),
                            }));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

fn walk_rs_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_rs_files(&path)?);
            } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                files.push(path);
            }
        }
    }
    Ok(files)
}

#[tauri::command]
pub async fn execute_terminal_command(
    state: State<'_, WorkspaceState>,
    command: String,
) -> Result<String, String> {
    state.require_trusted()?;
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .current_dir(state.root())
            .arg("-NoProfile")
            .arg("-Command")
            .arg(trimmed)
            .output()
    } else {
        Command::new("sh")
            .current_dir(state.root())
            .arg("-c")
            .arg(trimmed)
            .output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            if !stderr.is_empty() {
                Ok(format!("{}\nエラー:\n{}", stdout, stderr))
            } else {
                Ok(stdout)
            }
        }
        Err(e) => Err(format!("Command execution failed: {}", e)),
    }
}

#[tauri::command]
pub fn load_editor_configuration(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
) -> Result<SettingsSnapshot, String> {
    settings_store::load(&app, &state.root())
}

#[tauri::command]
pub fn save_editor_configuration(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    snapshot: SettingsSnapshot,
) -> Result<SettingsSnapshot, String> {
    settings_store::save(&app, &state.root(), snapshot)
}

#[tauri::command]
pub fn migrate_editor_configuration(
    app: AppHandle,
    state: State<'_, WorkspaceState>,
    snapshot: SettingsSnapshot,
) -> Result<SettingsSnapshot, String> {
    settings_store::migrate_from_local_storage(&app, &state.root(), snapshot)
}

#[tauri::command]
pub fn store_credential(service: String, account: String, secret: String) -> Result<(), String> {
    settings_store::store_credential(&service, &account, &secret)
}

#[tauri::command]
pub fn has_credential(service: String, account: String) -> Result<bool, String> {
    settings_store::has_credential(&service, &account)
}

#[tauri::command]
pub fn delete_credential(service: String, account: String) -> Result<(), String> {
    settings_store::delete_credential(&service, &account)
}

#[tauri::command]
pub fn debug_list_configurations(
    state: State<'_, WorkspaceState>,
) -> Result<Vec<DebugConfiguration>, String> {
    debug_config::load_configurations(&state.root())
}

#[tauri::command]
pub fn debug_check_adapter(adapter_type: String) -> Result<debug_session::AdapterStatus, String> {
    debug_session::check_adapter(&adapter_type)
}

#[tauri::command]
pub fn debug_start_session(
    app: AppHandle,
    state: State<'_, DebugSessionState>,
    workspace: State<'_, WorkspaceState>,
    configuration_name: String,
    breakpoints: Vec<SourceBreakpoint>,
) -> Result<(), String> {
    let configurations = debug_config::load_configurations(&workspace.root())?;
    let configuration = configurations
        .into_iter()
        .find(|candidate| candidate.name == configuration_name)
        .ok_or_else(|| format!("Debug configuration not found: {configuration_name}"))?;
    debug_config::validate_configuration(&configuration, &workspace.root())?;
    state.start(app, configuration, breakpoints)
}

#[tauri::command]
pub fn debug_stop_session(state: State<'_, DebugSessionState>) -> Result<(), String> {
    state.stop()
}

#[tauri::command]
pub fn debug_set_breakpoints(
    state: State<'_, DebugSessionState>,
    source: String,
    lines: Vec<u32>,
) -> Result<Value, String> {
    state.require_session()?.set_breakpoints(&SourceBreakpoint { source, lines })
}

#[tauri::command]
pub fn debug_continue(
    state: State<'_, DebugSessionState>,
    thread_id: Option<i64>,
) -> Result<Value, String> {
    state.require_session()?.request("continue", json!({ "threadId": thread_id.unwrap_or(0) }))
}

#[tauri::command]
pub fn debug_next(
    state: State<'_, DebugSessionState>,
    thread_id: Option<i64>,
) -> Result<Value, String> {
    state.require_session()?.request("next", json!({ "threadId": thread_id.unwrap_or(0) }))
}

#[tauri::command]
pub fn debug_step_in(
    state: State<'_, DebugSessionState>,
    thread_id: Option<i64>,
) -> Result<Value, String> {
    state.require_session()?.request("stepIn", json!({ "threadId": thread_id.unwrap_or(0) }))
}

#[tauri::command]
pub fn debug_step_out(
    state: State<'_, DebugSessionState>,
    thread_id: Option<i64>,
) -> Result<Value, String> {
    state.require_session()?.request("stepOut", json!({ "threadId": thread_id.unwrap_or(0) }))
}

#[tauri::command]
pub fn debug_pause(
    state: State<'_, DebugSessionState>,
    thread_id: Option<i64>,
) -> Result<Value, String> {
    state.require_session()?.request("pause", json!({ "threadId": thread_id.unwrap_or(0) }))
}

#[tauri::command]
pub fn debug_threads(state: State<'_, DebugSessionState>) -> Result<Value, String> {
    state.require_session()?.request("threads", json!({}))
}

#[tauri::command]
pub fn debug_stack_trace(
    state: State<'_, DebugSessionState>,
    thread_id: i64,
) -> Result<Value, String> {
    state.require_session()?.request("stackTrace", json!({ "threadId": thread_id }))
}

#[tauri::command]
pub fn debug_scopes(
    state: State<'_, DebugSessionState>,
    frame_id: i64,
) -> Result<Value, String> {
    state.require_session()?.request("scopes", json!({ "frameId": frame_id }))
}

#[tauri::command]
pub fn debug_variables(
    state: State<'_, DebugSessionState>,
    variables_reference: i64,
) -> Result<Value, String> {
    state.require_session()?.request("variables", json!({ "variablesReference": variables_reference }))
}

#[tauri::command]
pub fn debug_evaluate(
    state: State<'_, DebugSessionState>,
    expression: String,
    frame_id: Option<i64>,
) -> Result<Value, String> {
    if expression.trim().is_empty() {
        return Err("Watch expression must not be empty".to_string());
    }
    state.require_session()?.request("evaluate", json!({
        "expression": expression,
        "frameId": frame_id,
        "context": "repl"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_find_string_definition() {
        let res = find_rust_stdlib_definition("String".to_string())
            .await
            .unwrap();
        assert!(
            res.is_some(),
            "Expected to find definition for String in stdlib"
        );
        let match_res = res.unwrap();
        assert!(
            match_res.file_path.ends_with("string.rs"),
            "Expected file to be string.rs, got: {}",
            match_res.file_path
        );
        assert!(
            match_res.line_number > 0,
            "Expected a positive source line number"
        );
        println!(
            "✔ Found String definition at: {}:{}",
            match_res.file_path, match_res.line_number
        );
    }

    #[tokio::test]
    async fn test_find_vec_definition() {
        let res = find_rust_stdlib_definition("Vec".to_string())
            .await
            .unwrap();
        assert!(
            res.is_some(),
            "Expected to find definition for Vec in stdlib"
        );
        let match_res = res.unwrap();
        println!(
            "✔ Found Vec definition at: {}:{}",
            match_res.file_path, match_res.line_number
        );
    }

    #[test]
    fn workspace_path_resolution_rejects_external_files() {
        let workspace = WorkspaceState::new();
        assert!(workspace.resolve_path("../outside.txt").is_err());
    }
}
