use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DebugConfiguration {
    pub name: String,
    #[serde(rename = "type")]
    pub adapter_type: String,
    pub request: DebugRequest,
    #[serde(default)]
    pub program: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DebugRequest {
    Launch,
    Attach,
}

#[derive(Debug, Deserialize)]
struct LaunchFile {
    #[serde(default)]
    configurations: Vec<DebugConfiguration>,
}

pub fn load_configurations(workspace_root: &Path) -> Result<Vec<DebugConfiguration>, String> {
    let candidates = [
        workspace_root.join(".vscode/launch.json"),
        workspace_root.join(".oxide/launch.json"),
    ];

    for path in candidates {
        if !path.exists() {
            continue;
        }
        let content = std::fs::read_to_string(&path)
            .map_err(|error| format!("Could not read {}: {}", path.display(), error))?;
        let file: LaunchFile = serde_json::from_str(&content).map_err(|error| {
            format!(
                "Invalid debug configuration in {}: {}",
                path.display(),
                error
            )
        })?;
        for config in &file.configurations {
            validate_configuration(config, workspace_root)?;
        }
        return Ok(file.configurations);
    }

    Ok(Vec::new())
}

pub fn validate_configuration(
    configuration: &DebugConfiguration,
    workspace_root: &Path,
) -> Result<(), String> {
    if configuration.name.trim().is_empty() {
        return Err("Debug configuration name is required".to_string());
    }
    if configuration.adapter_type != "lldb" && configuration.adapter_type != "python" {
        return Err(format!(
            "Unsupported debug adapter type '{}'. Supported types: lldb, python",
            configuration.adapter_type
        ));
    }
    if configuration.request == DebugRequest::Launch
        && configuration.program.as_deref().unwrap_or("").is_empty()
    {
        return Err("A launch configuration requires a program path".to_string());
    }
    if let Some(program) = &configuration.program {
        let path = resolve_workspace_path(workspace_root, program)?;
        if !path.is_file() {
            return Err(format!(
                "Debug program is not a readable file: {}",
                path.display()
            ));
        }
    }
    if let Some(cwd) = &configuration.cwd {
        let path = resolve_workspace_path(workspace_root, cwd)?;
        if !path.is_dir() {
            return Err(format!(
                "Debug working directory does not exist: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn resolve_workspace_path(workspace_root: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw_path);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        workspace_root.join(path)
    };
    let parent = candidate.parent().unwrap_or(workspace_root);
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| format!("Could not resolve debug path: {}", error))?;
    let canonical_root = workspace_root
        .canonicalize()
        .map_err(|error| format!("Could not resolve workspace root: {}", error))?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err("Debug configuration path is outside the workspace".to_string());
    }
    Ok(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configuration(request: DebugRequest) -> DebugConfiguration {
        DebugConfiguration {
            name: "Test configuration".to_string(),
            adapter_type: "python".to_string(),
            request,
            program: None,
            cwd: None,
            args: Vec::new(),
            env: BTreeMap::new(),
        }
    }

    #[test]
    fn rejects_unsupported_adapter() {
        let root = std::env::current_dir().unwrap();
        let mut config = configuration(DebugRequest::Attach);
        config.adapter_type = "node".to_string();
        assert!(validate_configuration(&config, &root).is_err());
    }

    #[test]
    fn rejects_launch_configurations_without_a_program() {
        let root = std::env::current_dir().unwrap();
        let error =
            validate_configuration(&configuration(DebugRequest::Launch), &root).unwrap_err();
        assert_eq!(error, "A launch configuration requires a program path");
    }

    #[test]
    fn rejects_debug_paths_outside_the_workspace() {
        let root = std::env::current_dir().unwrap();
        let mut config = configuration(DebugRequest::Attach);
        config.cwd = Some("/tmp".to_string());

        let error = validate_configuration(&config, &root).unwrap_err();

        assert_eq!(error, "Debug configuration path is outside the workspace");
    }
}
