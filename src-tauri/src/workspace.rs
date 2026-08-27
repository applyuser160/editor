use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

const MAX_RECENT_WORKSPACES: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceInfo {
    pub root: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WorkspaceStore {
    recent: VecDeque<PathBuf>,
}

#[derive(Clone)]
pub struct WorkspaceState {
    root: Arc<Mutex<PathBuf>>,
    recent: Arc<Mutex<VecDeque<PathBuf>>>,
}

impl WorkspaceState {
    pub fn new() -> Self {
        let initial_root = std::env::current_dir()
            .ok()
            .and_then(|path| path.canonicalize().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let store = load_store();

        Self {
            root: Arc::new(Mutex::new(initial_root)),
            recent: Arc::new(Mutex::new(store.recent)),
        }
    }

    pub fn root(&self) -> PathBuf {
        self.root.lock().unwrap().clone()
    }

    pub fn info(&self) -> WorkspaceInfo {
        workspace_info(self.root())
    }

    pub fn set_root(&self, raw_path: &str) -> Result<WorkspaceInfo, String> {
        let path = Path::new(raw_path);
        if !path.is_dir() {
            return Err("Workspace path must be an existing directory".to_string());
        }
        let canonical_root = path
            .canonicalize()
            .map_err(|error| format!("Failed to resolve workspace path: {}", error))?;

        *self.root.lock().unwrap() = canonical_root.clone();
        let mut recent = self.recent.lock().unwrap();
        recent.retain(|entry| entry != &canonical_root);
        recent.push_front(canonical_root.clone());
        recent.truncate(MAX_RECENT_WORKSPACES);
        persist_store(&WorkspaceStore {
            recent: recent.clone(),
        })?;

        Ok(workspace_info(canonical_root))
    }

    pub fn recent(&self) -> Vec<WorkspaceInfo> {
        self.recent
            .lock()
            .unwrap()
            .iter()
            .filter(|path| path.is_dir())
            .cloned()
            .map(workspace_info)
            .collect()
    }

    pub fn remove_recent(&self, raw_path: &str) -> Result<(), String> {
        let canonical_path = Path::new(raw_path)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(raw_path));
        let mut recent = self.recent.lock().unwrap();
        recent.retain(|entry| entry != &canonical_path);
        persist_store(&WorkspaceStore {
            recent: recent.clone(),
        })
    }

    pub fn resolve_path(&self, raw_path: &str) -> Result<PathBuf, String> {
        let root = self.root();
        let requested = Path::new(raw_path);
        if !requested.is_absolute()
            && requested
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err("Paths outside the workspace are not allowed".to_string());
        }

        let candidate = if requested.is_absolute() {
            requested.to_path_buf()
        } else {
            root.join(requested)
        };

        let existing_ancestor = nearest_existing_ancestor(&candidate)?;
        let canonical_ancestor = existing_ancestor
            .canonicalize()
            .map_err(|error| format!("Failed to resolve path: {}", error))?;
        if !canonical_ancestor.starts_with(&root) {
            return Err("Paths outside the workspace are not allowed".to_string());
        }

        Ok(candidate)
    }
}

fn workspace_info(path: PathBuf) -> WorkspaceInfo {
    WorkspaceInfo {
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("workspace")
            .to_string(),
        root: path.to_string_lossy().replace('\\', "/"),
    }
}

fn nearest_existing_ancestor(path: &Path) -> Result<&Path, String> {
    let mut current = path;
    while !current.exists() {
        current = current
            .parent()
            .ok_or_else(|| "Path does not have an existing ancestor".to_string())?;
    }
    Ok(current)
}

fn store_path() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("oxide-editor");
    path.push("workspaces.json");
    path
}

fn load_store() -> WorkspaceStore {
    let path = store_path();
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn persist_store(store: &WorkspaceStore) -> Result<(), String> {
    let path = store_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let content = serde_json::to_string_pretty(store).map_err(|error| error.to_string())?;
    std::fs::write(path, content).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_rejects_parent_traversal() {
        let state = WorkspaceState::new();
        assert!(state.resolve_path("../outside.txt").is_err());
    }

    #[test]
    fn workspace_info_uses_folder_name() {
        let info = workspace_info(PathBuf::from("/tmp/oxide-workspace"));
        assert_eq!(info.name, "oxide-workspace");
    }
}
