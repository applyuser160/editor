use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
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
    let full_path = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(&path);
    std::fs::read_to_string(&full_path).map_err(|e| format!("Failed to read {}: {}", path, e))
}

#[tauri::command]
pub async fn write_file_content(path: String, content: String) -> Result<(), String> {
    let full_path = std::env::current_dir()
        .map_err(|e| e.to_string())?
        .join(&path);
    if let Some(parent) = full_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&full_path, content).map_err(|e| format!("Failed to write {}: {}", path, e))
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
