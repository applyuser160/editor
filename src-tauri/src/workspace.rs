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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceTrust {
    pub root: String,
    pub trusted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceExcludes {
    pub files: Vec<String>,
    pub search: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum WorkspaceFilterTarget {
    Files,
    Search,
}

#[derive(Debug, Clone)]
pub struct WorkspaceFilter {
    file_patterns: Vec<String>,
    search_patterns: Vec<String>,
    gitignore_patterns: Vec<String>,
}

impl WorkspaceFilter {
    pub fn should_exclude(&self, root: &Path, path: &Path, target: WorkspaceFilterTarget) -> bool {
        let relative = path.strip_prefix(root).unwrap_or(path);
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative.is_empty() {
            return false;
        }

        if relative.split('/').any(|segment| {
            segment.starts_with('.')
                || matches!(segment, "target" | "node_modules" | "dist" | ".git")
        }) {
            return true;
        }

        let mut excluded = matches_any_pattern(&relative, &self.file_patterns);
        if matches!(target, WorkspaceFilterTarget::Search) {
            excluded |= matches_any_pattern(&relative, &self.search_patterns);
        }

        apply_gitignore_patterns(&relative, &self.gitignore_patterns, excluded)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WorkspaceStore {
    #[serde(default)]
    recent: VecDeque<PathBuf>,
    #[serde(default)]
    folders: Vec<PathBuf>,
    #[serde(default)]
    active_folder: usize,
    #[serde(default)]
    trusted: Vec<PathBuf>,
    #[serde(default)]
    files_exclude: Vec<String>,
    #[serde(default)]
    search_exclude: Vec<String>,
}

#[derive(Clone)]
pub struct WorkspaceState {
    store: Arc<Mutex<WorkspaceStore>>,
}

fn requested_path(raw_path: &str) -> PathBuf {
    #[cfg(windows)]
    {
        let normalized = raw_path.replace('/', "\\");
        if let Some(path) = normalized.strip_prefix("\\\\?\\UNC\\") {
            return PathBuf::from(format!("\\\\{}", path));
        }
        if let Some(path) = normalized.strip_prefix("\\\\?\\") {
            return PathBuf::from(path);
        }
        PathBuf::from(normalized)
    }

    #[cfg(not(windows))]
    {
        PathBuf::from(raw_path)
    }
}

fn is_path_within_workspace(root: &Path, candidate: &Path) -> bool {
    #[cfg(windows)]
    {
        fn comparison_key(path: &Path) -> String {
            let normalized = path.to_string_lossy().replace('/', "\\");
            let normalized = if let Some(path) = normalized.strip_prefix("\\\\?\\UNC\\") {
                format!("\\\\{}", path)
            } else if let Some(path) = normalized.strip_prefix("\\\\?\\") {
                path.to_owned()
            } else {
                normalized
            };
            normalized.trim_end_matches('\\').to_ascii_lowercase()
        }

        let root = comparison_key(root);
        let candidate = comparison_key(candidate);
        candidate == root
            || candidate
                .strip_prefix(&root)
                .is_some_and(|remainder| remainder.starts_with('\\'))
    }

    #[cfg(not(windows))]
    {
        candidate.starts_with(root)
    }
}

impl Default for WorkspaceState {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceState {
    pub fn new() -> Self {
        let initial_root = std::env::current_dir()
            .ok()
            .and_then(|path| path.canonicalize().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let store = load_store();
        Self::from_store(initial_root, store)
    }

    fn from_store(initial_root: PathBuf, mut store: WorkspaceStore) -> Self {
        store.folders = std::mem::take(&mut store.folders)
            .into_iter()
            .filter_map(|path| path.canonicalize().ok())
            .filter(|path| path.is_dir())
            .collect();
        if store.folders.is_empty() {
            store.folders.push(initial_root);
        }
        store.active_folder = store.active_folder.min(store.folders.len() - 1);
        store.trusted.retain(|path| path.is_dir());

        Self {
            store: Arc::new(Mutex::new(store)),
        }
    }

    pub fn root(&self) -> PathBuf {
        let store = self.store.lock().unwrap();
        store.folders[store.active_folder].clone()
    }

    pub fn info(&self) -> WorkspaceInfo {
        workspace_info(self.root())
    }

    pub fn folders(&self) -> Vec<WorkspaceInfo> {
        self.store
            .lock()
            .unwrap()
            .folders
            .iter()
            .cloned()
            .map(workspace_info)
            .collect()
    }

    pub fn set_root(&self, raw_path: &str) -> Result<WorkspaceInfo, String> {
        let canonical_root = canonical_workspace_path(raw_path)?;
        let mut store = self.store.lock().unwrap();
        store.folders = vec![canonical_root.clone()];
        store.active_folder = 0;
        remember_recent(&mut store.recent, canonical_root.clone());
        persist_store(&store)?;
        Ok(workspace_info(canonical_root))
    }

    pub fn add_folder(&self, raw_path: &str) -> Result<Vec<WorkspaceInfo>, String> {
        let folder = canonical_workspace_path(raw_path)?;
        let mut store = self.store.lock().unwrap();
        if !store.folders.contains(&folder) {
            store.folders.push(folder.clone());
        }
        remember_recent(&mut store.recent, folder);
        persist_store(&store)?;
        Ok(store.folders.iter().cloned().map(workspace_info).collect())
    }

    pub fn remove_folder(&self, raw_path: &str) -> Result<Vec<WorkspaceInfo>, String> {
        let folder = canonical_workspace_path(raw_path)?;
        let mut store = self.store.lock().unwrap();
        if store.folders.len() == 1 {
            return Err("A workspace must contain at least one folder".to_string());
        }
        let index = store
            .folders
            .iter()
            .position(|candidate| candidate == &folder)
            .ok_or_else(|| "Workspace folder was not found".to_string())?;
        store.folders.remove(index);
        if store.active_folder >= store.folders.len() {
            store.active_folder = store.folders.len() - 1;
        } else if index < store.active_folder {
            store.active_folder -= 1;
        }
        persist_store(&store)?;
        Ok(store.folders.iter().cloned().map(workspace_info).collect())
    }

    pub fn select_folder(&self, raw_path: &str) -> Result<WorkspaceInfo, String> {
        let folder = canonical_workspace_path(raw_path)?;
        let mut store = self.store.lock().unwrap();
        let index = store
            .folders
            .iter()
            .position(|candidate| candidate == &folder)
            .ok_or_else(|| "Workspace folder was not found".to_string())?;
        store.active_folder = index;
        remember_recent(&mut store.recent, folder.clone());
        persist_store(&store)?;
        Ok(workspace_info(folder))
    }

    pub fn recent(&self) -> Vec<WorkspaceInfo> {
        self.store
            .lock()
            .unwrap()
            .recent
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
        let mut store = self.store.lock().unwrap();
        store.recent.retain(|entry| entry != &canonical_path);
        persist_store(&store)
    }

    pub fn trust(&self) -> WorkspaceTrust {
        let root = self.root();
        let trusted = self.store.lock().unwrap().trusted.contains(&root);
        WorkspaceTrust {
            root: root.to_string_lossy().replace('\\', "/"),
            trusted,
        }
    }

    pub fn set_trusted(&self, trusted: bool) -> Result<WorkspaceTrust, String> {
        let root = self.root();
        let mut store = self.store.lock().unwrap();
        if trusted {
            if !store.trusted.contains(&root) {
                store.trusted.push(root.clone());
            }
        } else {
            store.trusted.retain(|entry| entry != &root);
        }
        persist_store(&store)?;
        Ok(WorkspaceTrust {
            root: root.to_string_lossy().replace('\\', "/"),
            trusted,
        })
    }

    pub fn require_trusted(&self) -> Result<(), String> {
        if self.trust().trusted {
            Ok(())
        } else {
            Err("This workspace is untrusted. Trust the folder before running terminals, tasks, language servers, or extensions.".to_string())
        }
    }

    pub fn excludes(&self) -> WorkspaceExcludes {
        let store = self.store.lock().unwrap();
        WorkspaceExcludes {
            files: store.files_exclude.clone(),
            search: store.search_exclude.clone(),
        }
    }

    pub fn set_excludes(
        &self,
        files: Vec<String>,
        search: Vec<String>,
    ) -> Result<WorkspaceExcludes, String> {
        let mut store = self.store.lock().unwrap();
        store.files_exclude = normalize_patterns(files);
        store.search_exclude = normalize_patterns(search);
        persist_store(&store)?;
        Ok(WorkspaceExcludes {
            files: store.files_exclude.clone(),
            search: store.search_exclude.clone(),
        })
    }

    pub fn filter(&self) -> WorkspaceFilter {
        let root = self.root();
        let excludes = self.excludes();
        WorkspaceFilter {
            file_patterns: excludes.files,
            search_patterns: excludes.search,
            gitignore_patterns: read_gitignore_patterns(&root),
        }
    }

    pub fn resolve_path(&self, raw_path: &str) -> Result<PathBuf, String> {
        let root = self.root();
        let canonical_root = root
            .canonicalize()
            .map_err(|error| format!("Failed to resolve workspace path: {}", error))?;
        let requested = requested_path(raw_path);
        if !requested.is_absolute()
            && requested
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err("Paths outside the workspace are not allowed".to_string());
        }

        let candidate = if requested.is_absolute() {
            requested
        } else {
            canonical_root.join(requested)
        };

        let existing_ancestor = nearest_existing_ancestor(&candidate)?;
        let canonical_ancestor = existing_ancestor
            .canonicalize()
            .map_err(|error| format!("Failed to resolve path: {}", error))?;
        if !is_path_within_workspace(&canonical_root, &canonical_ancestor) {
            return Err("Paths outside the workspace are not allowed".to_string());
        }

        Ok(candidate)
    }
}

fn canonical_workspace_path(raw_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw_path);
    if !path.is_dir() {
        return Err("Workspace path must be an existing directory".to_string());
    }
    path.canonicalize()
        .map_err(|error| format!("Failed to resolve workspace path: {}", error))
}

fn remember_recent(recent: &mut VecDeque<PathBuf>, path: PathBuf) {
    recent.retain(|entry| entry != &path);
    recent.push_front(path);
    recent.truncate(MAX_RECENT_WORKSPACES);
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

fn normalize_patterns(patterns: Vec<String>) -> Vec<String> {
    patterns
        .into_iter()
        .map(|pattern| pattern.trim().replace('\\', "/"))
        .filter(|pattern| !pattern.is_empty())
        .collect()
}

fn read_gitignore_patterns(root: &Path) -> Vec<String> {
    std::fs::read_to_string(root.join(".gitignore"))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

fn apply_gitignore_patterns(relative: &str, patterns: &[String], mut excluded: bool) -> bool {
    for raw_pattern in patterns {
        let (include, pattern) = match raw_pattern.strip_prefix('!') {
            Some(pattern) => (true, pattern),
            None => (false, raw_pattern.as_str()),
        };
        if matches_pattern(relative, pattern) {
            excluded = !include;
        }
    }
    excluded
}

fn matches_any_pattern(relative: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .filter(|pattern| !pattern.starts_with('!'))
        .any(|pattern| matches_pattern(relative, pattern))
}

fn matches_pattern(relative: &str, raw_pattern: &str) -> bool {
    let pattern = raw_pattern
        .trim()
        .trim_start_matches('/')
        .trim_end_matches('/');
    if pattern.is_empty() {
        return false;
    }
    if !pattern.contains('/') {
        return relative
            .split('/')
            .any(|component| wildcard_matches(component, pattern));
    }
    if relative == pattern || relative.starts_with(&format!("{}/", pattern)) {
        return true;
    }
    wildcard_matches(relative, pattern)
}

fn wildcard_matches(value: &str, pattern: &str) -> bool {
    let value: Vec<char> = value.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    let mut previous = vec![false; pattern.len() + 1];
    previous[0] = true;

    for (index, character) in pattern.iter().enumerate() {
        if *character == '*' {
            previous[index + 1] = previous[index];
        }
    }

    for value_character in value {
        let mut current = vec![false; pattern.len() + 1];
        for (index, pattern_character) in pattern.iter().enumerate() {
            current[index + 1] = match pattern_character {
                '*' => current[index] || previous[index + 1],
                '?' => previous[index],
                character => previous[index] && *character == value_character,
            };
        }
        previous = current;
    }

    previous[pattern.len()]
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

    fn state_with_folders(folders: Vec<PathBuf>, active_folder: usize) -> WorkspaceState {
        WorkspaceState::from_store(
            folders[0].clone(),
            WorkspaceStore {
                folders,
                active_folder,
                ..WorkspaceStore::default()
            },
        )
    }

    #[test]
    fn resolve_path_rejects_parent_traversal() {
        let state = WorkspaceState::new();
        assert!(state.resolve_path("../outside.txt").is_err());
    }

    #[test]
    fn resolve_path_rejects_absolute_paths_outside_the_workspace() {
        let state = WorkspaceState::new();
        assert!(state.resolve_path("/tmp/oxide-editor-outside.txt").is_err());
    }

    #[test]
    fn resolve_path_allows_a_new_file_below_the_workspace_root() {
        let state = WorkspaceState::new();
        let resolved = state.resolve_path("nested/new-file.txt").unwrap();
        assert_eq!(resolved, state.root().join("nested/new-file.txt"));
    }

    #[test]
    fn resolve_path_allows_an_existing_absolute_path_within_the_workspace() {
        let workspace = std::env::temp_dir().join(format!(
            "oxide-workspace-path-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        let file = workspace.join("search-result.rs");
        std::fs::write(&file, "fn main() {}\n").unwrap();
        let state = state_with_folders(vec![workspace.clone()], 0);

        assert_eq!(
            state.resolve_path(file.to_str().unwrap()).unwrap(),
            file.canonicalize().unwrap()
        );

        let _ = std::fs::remove_dir_all(workspace);
    }

    #[cfg(windows)]
    #[test]
    fn resolve_path_allows_an_extended_windows_path_within_the_workspace() {
        let workspace = std::env::temp_dir().join(format!(
            "oxide-workspace-path-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&workspace).unwrap();
        let file = workspace.join("search-result.rs");
        std::fs::write(&file, "fn main() {}\n").unwrap();
        let state = state_with_folders(vec![workspace.clone()], 0);
        let extended_path = format!("\\\\?\\{}", file.display());

        assert!(state.resolve_path(&extended_path).is_ok());

        let _ = std::fs::remove_dir_all(workspace);
    }

    #[test]
    fn workspace_info_uses_folder_name() {
        let info = workspace_info(PathBuf::from("/tmp/oxide-workspace"));
        assert_eq!(info.name, "oxide-workspace");
    }

    #[test]
    fn tracks_multiple_roots_and_the_active_root() {
        let root = std::env::current_dir().unwrap().canonicalize().unwrap();
        let temporary = std::env::temp_dir().canonicalize().unwrap();
        let state = state_with_folders(vec![root.clone(), temporary.clone()], 1);

        assert_eq!(state.folders().len(), 2);
        assert_eq!(state.root(), temporary);
        let selected = state.select_folder(root.to_str().unwrap()).unwrap();
        assert_eq!(selected.root, root.to_string_lossy().replace('\\', "/"));
        assert_eq!(state.root(), root);
    }

    #[test]
    fn trust_is_disabled_until_a_workspace_is_explicitly_trusted() {
        let root = std::env::current_dir().unwrap().canonicalize().unwrap();
        let state = state_with_folders(vec![root], 0);

        assert!(!state.trust().trusted);
        assert!(state.require_trusted().is_err());
        assert!(state.set_trusted(true).unwrap().trusted);
        assert!(state.require_trusted().is_ok());
        assert!(!state.set_trusted(false).unwrap().trusted);
    }

    #[test]
    fn filters_files_search_rules_and_gitignore_patterns() {
        let filter = WorkspaceFilter {
            file_patterns: vec!["generated".to_string()],
            search_patterns: vec!["*.snapshot".to_string()],
            gitignore_patterns: vec!["*.log".to_string(), "!keep.log".to_string()],
        };
        let root = Path::new("/workspace");

        assert!(filter.should_exclude(
            root,
            Path::new("/workspace/generated/file.rs"),
            WorkspaceFilterTarget::Files
        ));
        assert!(filter.should_exclude(
            root,
            Path::new("/workspace/state.snapshot"),
            WorkspaceFilterTarget::Search
        ));
        assert!(!filter.should_exclude(
            root,
            Path::new("/workspace/state.snapshot"),
            WorkspaceFilterTarget::Files
        ));
        assert!(filter.should_exclude(
            root,
            Path::new("/workspace/debug.log"),
            WorkspaceFilterTarget::Files
        ));
        assert!(!filter.should_exclude(
            root,
            Path::new("/workspace/keep.log"),
            WorkspaceFilterTarget::Files
        ));
    }
}
