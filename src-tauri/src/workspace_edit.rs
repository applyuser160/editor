use crate::workspace::WorkspaceState;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEditPlan {
    pub operations: Vec<WorkspaceEditOperation>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WorkspaceEditOperation {
    Write {
        path: String,
        expected_content: Option<String>,
        content: String,
    },
    Create {
        path: String,
        content: String,
        #[serde(default)]
        overwrite: bool,
    },
    Delete {
        path: String,
        expected_content: Option<String>,
    },
    Rename {
        from: String,
        to: String,
        expected_content: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEditResult {
    pub changed_paths: Vec<String>,
}

#[derive(Debug, Clone)]
enum ResolvedOperation {
    Write {
        path: PathBuf,
        expected_content: Option<String>,
        content: String,
    },
    Create {
        path: PathBuf,
        content: String,
        overwrite: bool,
    },
    Delete {
        path: PathBuf,
        expected_content: Option<String>,
    },
    Rename {
        from: PathBuf,
        to: PathBuf,
        expected_content: Option<String>,
    },
}

#[derive(Debug)]
struct Backup {
    path: PathBuf,
    contents: Option<Vec<u8>>,
}

pub fn apply_plan(state: &WorkspaceState, plan: WorkspaceEditPlan) -> Result<WorkspaceEditResult, String> {
    if plan.operations.is_empty() {
        return Ok(WorkspaceEditResult { changed_paths: Vec::new() });
    }
    if plan.operations.len() > 512 {
        return Err("Workspace edit contains too many operations".to_string());
    }

    let operations = plan
        .operations
        .iter()
        .map(|operation| resolve_operation(state, operation))
        .collect::<Result<Vec<_>, _>>()?;
    validate_operations(&operations)?;
    let backups = create_backups(&operations)?;

    let result = apply_operations(&operations);
    if let Err(error) = result {
        if let Err(rollback_error) = restore_backups(&backups) {
            return Err(format!("Workspace edit failed: {error}. Rollback also failed: {rollback_error}"));
        }
        return Err(format!("Workspace edit failed and was rolled back: {error}"));
    }

    let changed_paths = operations
        .iter()
        .flat_map(operation_paths)
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();
    Ok(WorkspaceEditResult { changed_paths })
}

fn resolve_operation(state: &WorkspaceState, operation: &WorkspaceEditOperation) -> Result<ResolvedOperation, String> {
    match operation {
        WorkspaceEditOperation::Write { path, expected_content, content } => Ok(ResolvedOperation::Write {
            path: state.resolve_path(path)?,
            expected_content: expected_content.clone(),
            content: content.clone(),
        }),
        WorkspaceEditOperation::Create { path, content, overwrite } => Ok(ResolvedOperation::Create {
            path: state.resolve_path(path)?,
            content: content.clone(),
            overwrite: *overwrite,
        }),
        WorkspaceEditOperation::Delete { path, expected_content } => Ok(ResolvedOperation::Delete {
            path: state.resolve_path(path)?,
            expected_content: expected_content.clone(),
        }),
        WorkspaceEditOperation::Rename { from, to, expected_content } => Ok(ResolvedOperation::Rename {
            from: state.resolve_path(from)?,
            to: state.resolve_path(to)?,
            expected_content: expected_content.clone(),
        }),
    }
}

fn validate_operations(operations: &[ResolvedOperation]) -> Result<(), String> {
    let mut touched = HashSet::new();
    for operation in operations {
        match operation {
            ResolvedOperation::Write { path, expected_content, content } => {
                ensure_regular_file(path, "write")?;
                ensure_writable(path)?;
                ensure_expected_content(path, expected_content.as_deref())?;
                ensure_content_size(content)?;
                insert_unique_path(&mut touched, path)?;
            }
            ResolvedOperation::Create { path, content, overwrite } => {
                if path.exists() && !overwrite {
                    return Err(format!("Cannot create '{}': the file already exists", path.display()));
                }
                if path.exists() {
                    ensure_regular_file(path, "create")?;
                    ensure_writable(path)?;
                }
                ensure_parent_writable(path)?;
                ensure_content_size(content)?;
                insert_unique_path(&mut touched, path)?;
            }
            ResolvedOperation::Delete { path, expected_content } => {
                ensure_regular_file(path, "delete")?;
                ensure_writable(path)?;
                ensure_expected_content(path, expected_content.as_deref())?;
                insert_unique_path(&mut touched, path)?;
            }
            ResolvedOperation::Rename { from, to, expected_content } => {
                ensure_regular_file(from, "rename")?;
                ensure_writable(from)?;
                ensure_expected_content(from, expected_content.as_deref())?;
                if to.exists() {
                    return Err(format!("Cannot rename '{}': destination '{}' already exists", from.display(), to.display()));
                }
                ensure_parent_writable(to)?;
                insert_unique_path(&mut touched, from)?;
                insert_unique_path(&mut touched, to)?;
            }
        }
    }
    Ok(())
}

fn create_backups(operations: &[ResolvedOperation]) -> Result<Vec<Backup>, String> {
    let mut paths = Vec::new();
    for operation in operations {
        for path in operation_paths(operation) {
            if !paths.iter().any(|existing: &PathBuf| existing == path) {
                paths.push(path.clone());
            }
        }
    }
    paths
        .into_iter()
        .map(|path| {
            let contents = if path.exists() {
                Some(fs::read(&path).map_err(|_| format!("Could not create a backup of '{}'", path.display()))?)
            } else {
                None
            };
            Ok(Backup { path, contents })
        })
        .collect()
}

fn apply_operations(operations: &[ResolvedOperation]) -> Result<(), String> {
    for operation in operations {
        match operation {
            ResolvedOperation::Write { path, content, .. } | ResolvedOperation::Create { path, content, .. } => {
                write_atomically(path, content.as_bytes())?;
            }
            ResolvedOperation::Delete { path, .. } => {
                fs::remove_file(path).map_err(|error| format!("Could not delete '{}': {error}", path.display()))?;
            }
            ResolvedOperation::Rename { from, to, .. } => {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent).map_err(|error| format!("Could not create destination directory: {error}"))?;
                }
                fs::rename(from, to).map_err(|error| format!("Could not rename '{}': {error}", from.display()))?;
            }
        }
    }
    Ok(())
}

fn restore_backups(backups: &[Backup]) -> Result<(), String> {
    for backup in backups.iter().rev() {
        match &backup.contents {
            Some(contents) => write_atomically(&backup.path, contents)?,
            None => {
                if backup.path.exists() {
                    fs::remove_file(&backup.path)
                        .map_err(|error| format!("Could not remove rollback artifact '{}': {error}", backup.path.display()))?;
                }
            }
        }
    }
    Ok(())
}

fn operation_paths(operation: &ResolvedOperation) -> Vec<&PathBuf> {
    match operation {
        ResolvedOperation::Write { path, .. }
        | ResolvedOperation::Create { path, .. }
        | ResolvedOperation::Delete { path, .. } => vec![path],
        ResolvedOperation::Rename { from, to, .. } => vec![from, to],
    }
}

fn insert_unique_path(paths: &mut HashSet<PathBuf>, path: &Path) -> Result<(), String> {
    if !paths.insert(path.to_path_buf()) {
        return Err(format!("Workspace edit contains multiple operations for '{}'", path.display()));
    }
    Ok(())
}

fn ensure_regular_file(path: &Path, action: &str) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|_| format!("Cannot {action} '{}': file does not exist", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("Cannot {action} '{}': only regular files are supported", path.display()));
    }
    Ok(())
}

fn ensure_writable(path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|_| format!("Could not inspect '{}'", path.display()))?;
    if metadata.permissions().readonly() {
        return Err(format!("Cannot modify '{}': the file is read-only", path.display()));
    }
    Ok(())
}

fn ensure_parent_writable(path: &Path) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "Workspace edit path has no parent".to_string())?;
    let existing = nearest_existing_parent(parent)?;
    if fs::metadata(existing)
        .map_err(|_| "Could not inspect destination directory".to_string())?
        .permissions()
        .readonly()
    {
        return Err(format!("Cannot modify '{}': parent directory is read-only", path.display()));
    }
    Ok(())
}

fn nearest_existing_parent(path: &Path) -> Result<&Path, String> {
    let mut current = path;
    while !current.exists() {
        current = current.parent().ok_or_else(|| "Workspace edit path has no existing parent".to_string())?;
    }
    Ok(current)
}

fn ensure_expected_content(path: &Path, expected: Option<&str>) -> Result<(), String> {
    let Some(expected) = expected else { return Ok(()); };
    let actual = fs::read_to_string(path).map_err(|_| format!("Cannot read '{}' for conflict detection", path.display()))?;
    if actual != expected {
        return Err(format!("Cannot modify '{}': the file changed since the edit was previewed", path.display()));
    }
    Ok(())
}

fn ensure_content_size(content: &str) -> Result<(), String> {
    if content.len() > 16 * 1024 * 1024 {
        return Err("Workspace edit content exceeds the 16 MiB limit".to_string());
    }
    Ok(())
}

fn write_atomically(path: &Path, contents: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "Workspace edit path has no parent".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("Could not create directory '{}': {error}", parent.display()))?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map(|time| time.as_nanos()).unwrap_or_default();
    let temporary = parent.join(format!(".{}.{}.tmp", path.file_name().and_then(|name| name.to_str()).unwrap_or("edit"), nonce));
    let result = (|| -> Result<(), String> {
        let mut file = File::create(&temporary).map_err(|_| "Could not create a temporary edit file".to_string())?;
        file.write_all(contents).map_err(|_| "Could not write a temporary edit file".to_string())?;
        file.sync_all().map_err(|_| "Could not flush a temporary edit file".to_string())?;
        drop(file);
        fs::rename(&temporary, path).map_err(|_| format!("Could not replace '{}' atomically", path.display()))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_operation_paths() {
        let path = PathBuf::from("/tmp/oxide-duplicate.txt");
        let operations = vec![
            ResolvedOperation::Write { path: path.clone(), expected_content: None, content: "one".to_string() },
            ResolvedOperation::Delete { path, expected_content: None },
        ];
        assert!(validate_operations(&operations).is_err());
    }

    #[test]
    fn rejects_content_larger_than_limit() {
        assert!(ensure_content_size(&"x".repeat(16 * 1024 * 1024 + 1)).is_err());
    }

    #[test]
    fn applies_a_symbol_rename_to_multiple_python_files_together() {
        let workspace = std::env::temp_dir().join(format!(
            "oxide-workspace-edit-{}",
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        ));
        fs::create_dir_all(&workspace).unwrap();
        let main = workspace.join("main.py");
        let helper = workspace.join("helper.py");
        let original_main = "from helper import old_name\nprint(old_name())\n";
        let original_helper = "def old_name():\n    return 42\n";
        fs::write(&main, original_main).unwrap();
        fs::write(&helper, original_helper).unwrap();

        let state = WorkspaceState::new();
        state.set_root(workspace.to_str().unwrap()).unwrap();
        let result = apply_plan(&state, WorkspaceEditPlan {
            operations: vec![
                WorkspaceEditOperation::Write {
                    path: "main.py".to_string(),
                    expected_content: Some(original_main.to_string()),
                    content: "from helper import new_name\nprint(new_name())\n".to_string(),
                },
                WorkspaceEditOperation::Write {
                    path: "helper.py".to_string(),
                    expected_content: Some(original_helper.to_string()),
                    content: "def new_name():\n    return 42\n".to_string(),
                },
            ],
        }).unwrap();

        assert_eq!(result.changed_paths.len(), 2);
        assert_eq!(fs::read_to_string(main).unwrap(), "from helper import new_name\nprint(new_name())\n");
        assert_eq!(fs::read_to_string(helper).unwrap(), "def new_name():\n    return 42\n");
        let _ = fs::remove_dir_all(workspace);
    }
}
