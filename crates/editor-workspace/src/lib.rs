//! `editor-workspace`: Workspace, file tree explorer, tabs, and window state.

use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Option<Vec<FileNode>>,
}

pub struct Workspace {
    pub root_path: Option<PathBuf>,
    pub file_tree: Option<FileNode>,
}

impl Workspace {
    pub fn new(root_path: Option<PathBuf>) -> Self {
        Self {
            root_path,
            file_tree: None,
        }
    }
}
