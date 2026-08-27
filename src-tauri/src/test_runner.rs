use crate::task_runner::{run_task, TaskDefinition, TaskExecutionResult};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct TestSuite {
    pub id: String,
    pub label: String,
    pub command: String,
    pub args: Vec<String>,
}

pub fn discover_test_suites(root: &Path) -> Vec<TestSuite> {
    let mut suites = Vec::new();
    if root.join("Cargo.toml").is_file() {
        suites.push(suite("rust", "Rust: cargo test", "cargo", &["test"]));
    }
    if root.join("package.json").is_file() {
        suites.push(suite("node", "Node.js: npm test", "npm", &["test"]));
    }
    if root.join("pyproject.toml").is_file()
        || root.join("pytest.ini").is_file()
        || root.join("requirements.txt").is_file()
    {
        suites.push(suite(
            "python",
            "Python: pytest",
            "python",
            &["-m", "pytest"],
        ));
    }
    suites
}

pub fn run_test_suite(root: &Path, id: &str) -> Result<TaskExecutionResult, String> {
    let suite = discover_test_suites(root)
        .into_iter()
        .find(|suite| suite.id == id)
        .ok_or_else(|| format!("Test suite '{}' was not found", id))?;
    run_task(
        TaskDefinition {
            label: suite.label,
            command: suite.command,
            args: suite.args,
            is_background: false,
            cwd: None,
            env: BTreeMap::new(),
            depends_on: Vec::new(),
        },
        root,
    )
}

fn suite(id: &str, label: &str, command: &str, args: &[&str]) -> TestSuite {
    TestSuite {
        id: id.to_string(),
        label: label.to_string(),
        command: command.to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_rust_suite_in_the_current_repository() {
        let root = std::env::current_dir().unwrap();
        assert!(discover_test_suites(&root)
            .iter()
            .any(|suite| suite.id == "rust"));
    }
}
