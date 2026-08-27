use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub is_background: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskProblem {
    pub message: String,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskExecutionResult {
    pub label: String,
    pub exit_code: Option<i32>,
    pub output: String,
    pub problems: Vec<TaskProblem>,
}

pub fn run_task(task: TaskDefinition) -> Result<TaskExecutionResult, String> {
    if task.command.trim().is_empty() {
        return Err("Task command is required".to_string());
    }
    let output = Command::new(&task.command)
        .args(&task.args)
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
    let problems = combined
        .lines()
        .filter_map(|line| {
            let text = line.trim();
            if text.starts_with("error") || text.starts_with("warning") {
                Some(TaskProblem {
                    message: text.to_string(),
                    severity: if text.starts_with("error") {
                        "error".to_string()
                    } else {
                        "warning".to_string()
                    },
                })
            } else {
                None
            }
        })
        .collect();
    Ok(TaskExecutionResult {
        label: task.label,
        exit_code: output.status.code(),
        output: combined,
        problems,
    })
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
        };

        assert_eq!(run_task(task).unwrap_err(), "Task command is required");
    }

    #[cfg(target_family = "unix")]
    #[test]
    fn extracts_warning_and_error_lines_from_task_output() {
        let task = TaskDefinition {
            label: "diagnostics".into(),
            command: "sh".into(),
            args: vec![
                "-c".into(),
                "printf 'warning: caution\\n'; printf 'error: failure\\n' >&2".into(),
            ],
            is_background: false,
        };

        let result = run_task(task).unwrap();

        assert_eq!(result.exit_code, Some(0));
        assert_eq!(
            result.problems,
            vec![
                TaskProblem {
                    message: "warning: caution".into(),
                    severity: "warning".into(),
                },
                TaskProblem {
                    message: "error: failure".into(),
                    severity: "error".into(),
                },
            ]
        );
    }
}
