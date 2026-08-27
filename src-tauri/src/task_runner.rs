use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub is_background: bool,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default, alias = "dependsOn")]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProblem {
    pub message: String,
    pub severity: String,
    pub file_path: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResult {
    pub label: String,
    pub exit_code: Option<i32>,
    pub output: String,
    pub problems: Vec<TaskProblem>,
}

#[derive(Debug, Deserialize)]
struct TaskConfiguration {
    #[serde(default)]
    tasks: Vec<TaskDefinition>,
}

pub fn load_tasks(workspace_root: &Path) -> Result<Vec<TaskDefinition>, String> {
    let candidates = [
        workspace_root.join(".oxide/tasks.json"),
        workspace_root.join(".vscode/tasks.json"),
    ];
    for path in candidates {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path).map_err(|error| {
            format!(
                "Could not read task configuration {}: {}",
                path.display(),
                error
            )
        })?;
        let configuration: TaskConfiguration = serde_json::from_str(&content)
            .map_err(|error| format!("Invalid task configuration {}: {}", path.display(), error))?;
        validate_tasks(&configuration.tasks)?;
        return Ok(configuration.tasks);
    }
    Ok(Vec::new())
}

pub fn run_task(
    task: TaskDefinition,
    workspace_root: &Path,
) -> Result<TaskExecutionResult, String> {
    validate_task(&task)?;
    let cwd = resolve_task_cwd(&task, workspace_root)?;
    let output = Command::new(&task.command)
        .args(&task.args)
        .current_dir(cwd)
        .envs(&task.env)
        .output()
        .map_err(|error| format!("Could not start task '{}': {}", task.label, error))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!(
        "{}{}{}",
        stdout,
        if stdout.is_empty() || stderr.is_empty() {
            ""
        } else {
            "\n"
        },
        stderr
    );
    Ok(TaskExecutionResult {
        label: task.label,
        exit_code: output.status.code(),
        problems: extract_problems(&combined),
        output: combined,
    })
}

fn validate_tasks(tasks: &[TaskDefinition]) -> Result<(), String> {
    let mut labels = std::collections::HashSet::new();
    for task in tasks {
        validate_task(task)?;
        if !labels.insert(&task.label) {
            return Err(format!("Task labels must be unique: {}", task.label));
        }
    }
    for task in tasks {
        for dependency in &task.depends_on {
            if !labels.contains(dependency) {
                return Err(format!(
                    "Task '{}' depends on unknown task '{}'",
                    task.label, dependency
                ));
            }
        }
    }
    Ok(())
}

fn validate_task(task: &TaskDefinition) -> Result<(), String> {
    if task.label.trim().is_empty() {
        return Err("Task label is required".to_string());
    }
    if task.command.trim().is_empty() {
        return Err("Task command is required".to_string());
    }
    Ok(())
}

fn resolve_task_cwd(task: &TaskDefinition, workspace_root: &Path) -> Result<PathBuf, String> {
    let candidate = match &task.cwd {
        Some(cwd) => {
            let path = Path::new(cwd);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                workspace_root.join(path)
            }
        }
        None => workspace_root.to_path_buf(),
    };
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|error| format!("Could not resolve workspace root: {}", error))?;
    let canonical_cwd = candidate
        .canonicalize()
        .map_err(|error| format!("Could not resolve task working directory: {}", error))?;
    if !canonical_cwd.starts_with(&canonical_root) {
        return Err("Task working directory is outside the workspace".to_string());
    }
    Ok(canonical_cwd)
}

fn extract_problems(output: &str) -> Vec<TaskProblem> {
    let location_pattern = regex::Regex::new(
        r"^(?P<file>.+?):(?P<line>\d+):(?P<column>\d+):\s*(?P<severity>error|warning)[:\s]*(?P<message>.*)$",
    )
    .expect("task diagnostic pattern must be valid");

    output
        .lines()
        .filter_map(|line| {
            let text = line.trim();
            if let Some(captures) = location_pattern.captures(text) {
                return Some(TaskProblem {
                    message: captures
                        .name("message")
                        .map(|value| value.as_str().trim().to_string())
                        .unwrap_or_else(|| text.to_string()),
                    severity: captures["severity"].to_string(),
                    file_path: Some(captures["file"].to_string()),
                    line: captures["line"].parse().ok(),
                    column: captures["column"].parse().ok(),
                });
            }

            let severity = if text.starts_with("error") {
                "error"
            } else if text.starts_with("warning") {
                "warning"
            } else {
                return None;
            };
            Some(TaskProblem {
                message: text.to_string(),
                severity: severity.to_string(),
                file_path: None,
                line: None,
                column: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_commands() {
        let task = TaskDefinition {
            label: "bad".into(),
            command: "".into(),
            args: vec![],
            is_background: false,
            cwd: None,
            env: BTreeMap::new(),
            depends_on: vec![],
        };
        assert_eq!(
            validate_task(&task).unwrap_err(),
            "Task command is required"
        );
    }

    #[test]
    fn validates_unique_labels_and_dependencies() {
        let task = TaskDefinition {
            label: "test".into(),
            command: "cargo".into(),
            args: vec!["test".into()],
            is_background: false,
            cwd: None,
            env: BTreeMap::new(),
            depends_on: vec!["build".into()],
        };
        let build = TaskDefinition {
            label: "build".into(),
            command: "cargo".into(),
            args: vec!["build".into()],
            is_background: false,
            cwd: None,
            env: BTreeMap::new(),
            depends_on: vec![],
        };
        assert!(validate_tasks(&[task.clone(), build]).is_ok());
        assert!(validate_tasks(&[task.clone(), task]).is_err());
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn extracts_warning_and_error_lines_from_task_output() {
        let problems = extract_problems("warning: caution\nerror: failure");
        assert_eq!(
            problems,
            vec![
                TaskProblem {
                    message: "warning: caution".into(),
                    severity: "warning".into(),
                    file_path: None,
                    line: None,
                    column: None,
                },
                TaskProblem {
                    message: "error: failure".into(),
                    severity: "error".into(),
                    file_path: None,
                    line: None,
                    column: None,
                },
            ]
        );
    }

    #[test]
    fn extracts_file_line_and_column_from_compiler_diagnostics() {
        let problems = extract_problems("src/main.rs:12:8: error: expected expression");
        assert_eq!(
            problems,
            vec![TaskProblem {
                message: "expected expression".into(),
                severity: "error".into(),
                file_path: Some("src/main.rs".into()),
                line: Some(12),
                column: Some(8),
            }]
        );
    }
}
