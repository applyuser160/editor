//! `editor-workspace`: Workspace, file tree explorer, virtual scrolling, tabs, and window layout.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Represents an item in the file tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub children: Option<Vec<FileNode>>,
    pub is_ignored: bool,
}

impl FileNode {
    pub fn new(path: PathBuf, is_dir: bool) -> Self {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string_lossy().to_string());

        Self {
            name,
            path,
            is_dir,
            is_expanded: false,
            children: if is_dir { Some(Vec::new()) } else { None },
            is_ignored: false,
        }
    }
}

/// A flattened node for virtual scrolling in UI tree views.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatNode {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_expanded: bool,
}

/// File tree manager handling scanning, expanding/collapsing, and filtering.
#[derive(Debug, Clone, Default)]
pub struct FileTree {
    pub root: Option<FileNode>,
}

impl FileTree {
    pub fn new() -> Self {
        Self { root: None }
    }

    /// Scans a directory path recursively to build the file tree.
    pub fn scan(root_path: &Path) -> io::Result<Self> {
        let root_node = Self::scan_recursive(root_path)?;
        Ok(Self {
            root: Some(root_node),
        })
    }

    fn scan_recursive(path: &Path) -> io::Result<FileNode> {
        let is_dir = path.is_dir();
        let mut node = FileNode::new(path.to_path_buf(), is_dir);

        if is_dir {
            let mut children = Vec::new();
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    let child_path = entry.path();
                    let child_name = child_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                    
                    // Skip hidden .git directory by default
                    if child_name == ".git" {
                        continue;
                    }

                    if let Ok(child_node) = Self::scan_recursive(&child_path) {
                        children.push(child_node);
                    }
                }
            }

            // Sort: directories first, then alphabetically
            children.sort_by(|a, b| {
                match (a.is_dir, b.is_dir) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                }
            });

            node.children = Some(children);
        }

        Ok(node)
    }

    /// Toggles expanded/collapsed state for a directory node.
    pub fn toggle_expand(&mut self, target_path: &Path) -> bool {
        if let Some(root) = &mut self.root {
            Self::toggle_recursive(root, target_path)
        } else {
            false
        }
    }

    fn toggle_recursive(node: &mut FileNode, target_path: &Path) -> bool {
        if node.path == target_path {
            if node.is_dir {
                node.is_expanded = !node.is_expanded;
                return true;
            }
            return false;
        }

        if let Some(children) = &mut node.children {
            for child in children {
                if Self::toggle_recursive(child, target_path) {
                    return true;
                }
            }
        }
        false
    }

    /// Flattens only visible (expanded) nodes for 60fps virtual scroll rendering.
    pub fn flatten_visible(&self) -> Vec<FlatNode> {
        let mut flat = Vec::new();
        if let Some(root) = &self.root {
            Self::flatten_recursive(root, 0, &mut flat);
        }
        flat
    }

    fn flatten_recursive(node: &FileNode, depth: usize, out: &mut Vec<FlatNode>) {
        out.push(FlatNode {
            path: node.path.clone(),
            name: node.name.clone(),
            depth,
            is_dir: node.is_dir,
            is_expanded: node.is_expanded,
        });

        if node.is_dir && node.is_expanded {
            if let Some(children) = &node.children {
                for child in children {
                    Self::flatten_recursive(child, depth + 1, out);
                }
            }
        }
    }
}

/// A tab in the editor representing an open document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    pub id: Uuid,
    pub title: String,
    pub path: Option<PathBuf>,
    pub is_pinned: bool,
}

impl Tab {
    pub fn new(title: String, path: Option<PathBuf>) -> Self {
        Self {
            id: Uuid::new_v4(),
            title,
            path,
            is_pinned: false,
        }
    }
}

/// A group of tabs within a split pane.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TabGroup {
    pub id: Uuid,
    pub tabs: Vec<Tab>,
    pub active_tab_index: Option<usize>,
}

impl TabGroup {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            tabs: Vec::new(),
            active_tab_index: None,
        }
    }

    pub fn open_tab(&mut self, tab: Tab) {
        // Check if tab with same path already open
        if let Some(path) = &tab.path {
            if let Some(pos) = self.tabs.iter().position(|t| t.path.as_ref() == Some(path)) {
                self.active_tab_index = Some(pos);
                return;
            }
        }

        self.tabs.push(tab);
        self.active_tab_index = Some(self.tabs.len() - 1);
    }

    pub fn close_tab(&mut self, index: usize) -> Option<Tab> {
        if index >= self.tabs.len() {
            return None;
        }

        let closed = self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        } else if let Some(active) = self.active_tab_index {
            if active >= self.tabs.len() {
                self.active_tab_index = Some(self.tabs.len() - 1);
            }
        }
        Some(closed)
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_tab_index.and_then(|idx| self.tabs.get(idx))
    }
}

/// Workspace manager holding project file tree, active tab groups, and root folder.
#[derive(Debug, Clone, Default)]
pub struct Workspace {
    pub root_path: Option<PathBuf>,
    pub file_tree: FileTree,
    pub tab_groups: Vec<TabGroup>,
    pub active_group_index: usize,
}

impl Workspace {
    pub fn new(root_path: Option<PathBuf>) -> Self {
        let file_tree = if let Some(ref path) = root_path {
            FileTree::scan(path).unwrap_or_default()
        } else {
            FileTree::new()
        };

        Self {
            root_path,
            file_tree,
            tab_groups: vec![TabGroup::new()],
            active_group_index: 0,
        }
    }

    pub fn active_tab_group_mut(&mut self) -> &mut TabGroup {
        if self.tab_groups.is_empty() {
            self.tab_groups.push(TabGroup::new());
        }
        let idx = self.active_group_index.min(self.tab_groups.len() - 1);
        &mut self.tab_groups[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_group_management() {
        let mut group = TabGroup::new();
        assert!(group.active_tab().is_none());

        let tab1 = Tab::new("main.rs".to_string(), Some(PathBuf::from("src/main.rs")));
        let tab2 = Tab::new("lib.rs".to_string(), Some(PathBuf::from("src/lib.rs")));

        group.open_tab(tab1);
        assert_eq!(group.active_tab().unwrap().title, "main.rs");

        group.open_tab(tab2);
        assert_eq!(group.active_tab().unwrap().title, "lib.rs");
        assert_eq!(group.tabs.len(), 2);

        group.close_tab(1);
        assert_eq!(group.active_tab().unwrap().title, "main.rs");
        assert_eq!(group.tabs.len(), 1);
    }

    #[test]
    fn test_file_tree_flatten_and_toggle() {
        let mut root = FileNode::new(PathBuf::from("/project"), true);
        let child1 = FileNode::new(PathBuf::from("/project/src"), true);
        let child2 = FileNode::new(PathBuf::from("/project/README.md"), false);

        root.children = Some(vec![child1, child2]);
        let mut tree = FileTree { root: Some(root) };

        // Initially root is collapsed
        let visible = tree.flatten_visible();
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].name, "project");

        // Expand root
        tree.toggle_expand(&PathBuf::from("/project"));
        let visible = tree.flatten_visible();
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[1].name, "src");
        assert_eq!(visible[2].name, "README.md");
    }
}
