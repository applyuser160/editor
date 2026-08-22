//! `editor-git`: Source control integration (Git status, diff parser, commit, branch management).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("Git command failed: {0}")]
    CommandFailed(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("UTF-8 decoding error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Status of a file in the Git working tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GitFileStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Conflicted,
}

/// A tracked change in the Git repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitChange {
    pub path: PathBuf,
    pub status: GitFileStatus,
    pub staged: bool,
}

/// Line change type within a diff view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffLineType {
    Context,
    Addition,
    Deletion,
}

/// A single line in a diff hunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub old_line_num: Option<usize>,
    pub new_line_num: Option<usize>,
    pub content: String,
}

/// A diff hunk containing changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub header: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub lines: Vec<DiffLine>,
}

/// Fast unified diff parser for rendering side-by-side or inline Git diffs.
pub struct DiffParser;

impl DiffParser {
    pub fn parse_unified_diff(raw_diff: &str) -> Vec<DiffHunk> {
        let mut hunks = Vec::new();
        let mut current_hunk: Option<DiffHunk> = None;
        let mut old_num = 0;
        let mut new_num = 0;

        for line in raw_diff.lines() {
            if line.starts_with("@@") {
                if let Some(hunk) = current_hunk.take() {
                    hunks.push(hunk);
                }

                // Parse @@ -old_start,old_lines +new_start,new_lines @@ header
                let parts: Vec<&str> = line.split("@@").collect();
                let header = line.to_string();
                let mut old_start = 1;
                let mut old_lines = 1;
                let mut new_start = 1;
                let mut new_lines = 1;

                if parts.len() >= 2 {
                    let range_str = parts[1].trim();
                    for token in range_str.split_whitespace() {
                        if let Some(old_part) = token.strip_prefix('-') {
                            let nums: Vec<&str> = old_part.split(',').collect();
                            old_start = nums[0].parse().unwrap_or(1);
                            old_lines = nums.get(1).and_then(|n| n.parse().ok()).unwrap_or(1);
                        } else if let Some(new_part) = token.strip_prefix('+') {
                            let nums: Vec<&str> = new_part.split(',').collect();
                            new_start = nums[0].parse().unwrap_or(1);
                            new_lines = nums.get(1).and_then(|n| n.parse().ok()).unwrap_or(1);
                        }
                    }
                }

                old_num = old_start;
                new_num = new_start;

                current_hunk = Some(DiffHunk {
                    header,
                    old_start,
                    old_lines,
                    new_start,
                    new_lines,
                    lines: Vec::new(),
                });
            } else if let Some(hunk) = &mut current_hunk {
                if let Some(rest) = line.strip_prefix('+') {
                    hunk.lines.push(DiffLine {
                        line_type: DiffLineType::Addition,
                        old_line_num: None,
                        new_line_num: Some(new_num),
                        content: rest.to_string(),
                    });
                    new_num += 1;
                } else if let Some(rest) = line.strip_prefix('-') {
                    hunk.lines.push(DiffLine {
                        line_type: DiffLineType::Deletion,
                        old_line_num: Some(old_num),
                        new_line_num: None,
                        content: rest.to_string(),
                    });
                    old_num += 1;
                } else if let Some(rest) = line.strip_prefix(' ') {
                    hunk.lines.push(DiffLine {
                        line_type: DiffLineType::Context,
                        old_line_num: Some(old_num),
                        new_line_num: Some(new_num),
                        content: rest.to_string(),
                    });
                    old_num += 1;
                    new_num += 1;
                }
            }
        }

        if let Some(hunk) = current_hunk {
            hunks.push(hunk);
        }

        hunks
    }
}

/// Git repository manager that performs operations via CLI for optimal stability and performance.
pub struct GitManager {
    repo_path: PathBuf,
}

impl GitManager {
    pub fn new(repo_path: PathBuf) -> Self {
        Self { repo_path }
    }

    /// Gets changed files (staged, unstaged, untracked).
    pub fn get_status(&self) -> Result<Vec<GitChange>, GitError> {
        let output = Command::new("git")
            .arg("status")
            .arg("--porcelain=v1")
            .current_dir(&self.repo_path)
            .output()?;

        if !output.status.success() {
            return Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        let stdout = String::from_utf8(output.stdout)?;
        let mut changes = Vec::new();

        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }
            let index_status = line.as_bytes()[0] as char;
            let worktree_status = line.as_bytes()[1] as char;
            let file_path = PathBuf::from(line[3..].trim());

            if index_status != ' ' && index_status != '?' {
                // Staged change
                let status = match index_status {
                    'M' => GitFileStatus::Modified,
                    'A' => GitFileStatus::Added,
                    'D' => GitFileStatus::Deleted,
                    'R' => GitFileStatus::Renamed,
                    _ => GitFileStatus::Modified,
                };
                changes.push(GitChange {
                    path: file_path.clone(),
                    status,
                    staged: true,
                });
            }

            if worktree_status != ' ' {
                // Unstaged change
                let status = match worktree_status {
                    '?' => GitFileStatus::Untracked,
                    'M' => GitFileStatus::Modified,
                    'D' => GitFileStatus::Deleted,
                    _ => GitFileStatus::Modified,
                };
                changes.push(GitChange {
                    path: file_path,
                    status,
                    staged: false,
                });
            }
        }

        Ok(changes)
    }

    /// Stages a file or directory.
    pub fn stage_file(&self, path: &Path) -> Result<(), GitError> {
        let output = Command::new("git")
            .arg("add")
            .arg(path)
            .current_dir(&self.repo_path)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Unstages a file.
    pub fn unstage_file(&self, path: &Path) -> Result<(), GitError> {
        let output = Command::new("git")
            .arg("restore")
            .arg("--staged")
            .arg(path)
            .current_dir(&self.repo_path)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Creates a commit with the specified message.
    pub fn commit(&self, message: &str) -> Result<String, GitError> {
        let output = Command::new("git")
            .arg("commit")
            .arg("-m")
            .arg(message)
            .current_dir(&self.repo_path)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?)
        } else {
            Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }

    /// Retrieves the current branch name.
    pub fn current_branch(&self) -> Result<String, GitError> {
        let output = Command::new("git")
            .arg("branch")
            .arg("--show-current")
            .current_dir(&self.repo_path)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8(output.stdout)?.trim().to_string())
        } else {
            Err(GitError::CommandFailed(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_parser() {
        let raw_diff = r#"@@ -10,3 +10,4 @@
 fn test() {
-    let a = 1;
+    let a = 2;
+    let b = 3;
 }
"#;
        let hunks = DiffParser::parse_unified_diff(raw_diff);
        assert_eq!(hunks.len(), 1);
        let hunk = &hunks[0];
        assert_eq!(hunk.old_start, 10);
        assert_eq!(hunk.new_start, 10);
        assert_eq!(hunk.lines.len(), 5);

        assert_eq!(hunk.lines[1].line_type, DiffLineType::Deletion);
        assert_eq!(hunk.lines[1].content, "    let a = 1;");
        assert_eq!(hunk.lines[2].line_type, DiffLineType::Addition);
        assert_eq!(hunk.lines[2].content, "    let a = 2;");
    }
}
