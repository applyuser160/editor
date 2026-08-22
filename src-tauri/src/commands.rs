use crate::pty_manager::PtyState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, State};

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

#[tauri::command]
pub async fn spawn_pty(
    app: AppHandle,
    state: State<'_, PtyState>,
    cols: u16,
    rows: u16,
) -> Result<u32, String> {
    state.spawn(app, cols, rows)
}

#[tauri::command]
pub async fn write_pty(
    state: State<'_, PtyState>,
    id: u32,
    data: String,
) -> Result<(), String> {
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
pub async fn list_workspace_files() -> Result<Vec<FileEntry>, String> {
    let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    scan_dir_recursive(&current_dir, &current_dir, 0, &mut entries, 3)?;
    Ok(entries)
}

fn scan_dir_recursive(
    root: &Path,
    current: &Path,
    depth: usize,
    entries: &mut Vec<FileEntry>,
    max_depth: usize,
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

        if file_name.starts_with('.') || file_name == "target" || file_name == "node_modules" || file_name == "dist" {
            continue;
        }

        let is_dir = path.is_dir();
        let relative_path = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();

        entries.push(FileEntry {
            name: file_name,
            path: relative_path,
            is_dir,
            depth,
        });

        if is_dir {
            scan_dir_recursive(root, &path, depth + 1, entries, max_depth)?;
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn read_file_content(path: String) -> Result<String, String> {
    let full_path = get_absolute_path(&path)?;
    std::fs::read_to_string(&full_path).map_err(|e| format!("Failed to read {}: {}", path, e))
}

#[tauri::command]
pub async fn write_file_content(path: String, content: String) -> Result<(), String> {
    let full_path = get_absolute_path(&path)?;
    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&full_path, content).map_err(|e| format!("Failed to write {}: {}", path, e))
}

#[tauri::command]
pub async fn create_file(path: String) -> Result<(), String> {
    let full_path = get_absolute_path(&path)?;
    if full_path.exists() {
        return Err(format!("File '{}' already exists", path));
    }
    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&full_path, "").map_err(|e| format!("Failed to create file {}: {}", path, e))
}

#[tauri::command]
pub async fn create_directory(path: String) -> Result<(), String> {
    let full_path = get_absolute_path(&path)?;
    std::fs::create_dir_all(&full_path).map_err(|e| format!("Failed to create dir {}: {}", path, e))
}

#[tauri::command]
pub async fn delete_file(path: String) -> Result<(), String> {
    let full_path = get_absolute_path(&path)?;
    if full_path.is_dir() {
        std::fs::remove_dir_all(&full_path).map_err(|e| format!("Failed to delete dir {}: {}", path, e))
    } else {
        std::fs::remove_file(&full_path).map_err(|e| format!("Failed to delete file {}: {}", path, e))
    }
}

#[tauri::command]
pub async fn rename_file(old_path: String, new_path: String) -> Result<(), String> {
    let src = get_absolute_path(&old_path)?;
    let dst = get_absolute_path(&new_path)?;
    if let Some(parent) = dst.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::rename(&src, &dst).map_err(|e| format!("Failed to rename {} to {}: {}", old_path, new_path, e))
}

#[tauri::command]
pub async fn get_file_stat(path: String) -> Result<FileStat, String> {
    let full_path = get_absolute_path(&path)?;
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
pub async fn search_in_workspace(query: String, case_sensitive: bool) -> Result<Vec<SearchMatch>, String> {
    let q = if case_sensitive { query } else { query.to_lowercase() };
    if q.trim().is_empty() {
        return Ok(Vec::new());
    }

    let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    let mut matches = Vec::new();
    search_recursive(&current_dir, &current_dir, &q, case_sensitive, &mut matches, 5)?;
    Ok(matches)
}

fn search_recursive(
    root: &Path,
    current: &Path,
    query: &str,
    case_sensitive: bool,
    matches: &mut Vec<SearchMatch>,
    max_depth: usize,
) -> Result<(), String> {
    if max_depth == 0 {
        return Ok(());
    }

    let read_dir = std::fs::read_dir(current).map_err(|e| e.to_string())?;
    for entry in read_dir.filter_map(|e| e.ok()) {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        if file_name.starts_with('.') || file_name == "target" || file_name == "node_modules" || file_name == "dist" {
            continue;
        }

        if path.is_dir() {
            search_recursive(root, &path, query, case_sensitive, matches, max_depth - 1)?;
        } else if let Ok(content) = std::fs::read_to_string(&path) {
            let relative_path = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
            for (idx, line) in content.lines().enumerate() {
                let target = if case_sensitive { line.to_string() } else { line.to_lowercase() };
                if target.contains(query) {
                    matches.push(SearchMatch {
                        file_path: relative_path.clone(),
                        line_number: idx + 1,
                        line_text: line.trim().to_string(),
                    });
                    if matches.len() >= 100 {
                        return Ok(());
                    }
                }
            }
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn git_get_status() -> Result<GitStatusResult, String> {
    let branch_out = Command::new("git")
        .args(["branch", "--show-current"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "main".to_string());

    let status_out = Command::new("git")
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
        branch: if branch_out.is_empty() { "main".to_string() } else { branch_out },
        changed_files,
    })
}

#[tauri::command]
pub async fn git_commit(message: String) -> Result<String, String> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err("Commit message cannot be empty".to_string());
    }

    let add_res = Command::new("git").args(["add", "."]).output();
    if let Err(e) = add_res {
        return Err(format!("git add failed: {}", e));
    }

    let commit_res = Command::new("git")
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

fn get_absolute_path(path_str: &str) -> Result<PathBuf, String> {
    let p = Path::new(path_str);
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        let cur = std::env::current_dir().map_err(|e| e.to_string())?;
        Ok(cur.join(p))
    }
}

#[tauri::command]
pub async fn execute_terminal_command(command: String) -> Result<String, String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    let output = if cfg!(target_os = "windows") {
        Command::new("powershell")
            .arg("-NoProfile")
            .arg("-Command")
            .arg(trimmed)
            .output()
    } else {
        Command::new("sh")
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
