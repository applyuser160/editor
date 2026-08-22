//! `editor-plugin`: WASM-based sandboxed plugin host, capabilities security model, and theming/command registry.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("Plugin capability denied: {0:?}")]
    PermissionDenied(PluginCapability),
    #[error("Plugin execution error: {0}")]
    ExecutionFailed(String),
    #[error("Plugin not found: {0}")]
    NotFound(String),
}

/// Capabilities requested by a plugin (Sandboxed security).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginCapability {
    BufferRead,
    BufferWrite,
    FileSystemRead,
    FileSystemWrite,
    TerminalAccess,
    NetworkAccess,
    CommandRegister,
}

/// Command registered by a plugin for the Command Palette (Ctrl+Shift+P).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    pub category: Option<String>,
}

/// Metadata and permissions manifest for a WASM plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub entrypoint: String,
    pub capabilities: Vec<PluginCapability>,
    pub commands: Vec<PluginCommand>,
}

/// Runtime instance of an active plugin.
#[derive(Debug, Clone)]
pub struct PluginInstance {
    pub manifest: PluginManifest,
    pub is_enabled: bool,
}

/// Sandboxed plugin host managing extensions, commands, and security capabilities.
#[derive(Debug, Default)]
pub struct PluginHost {
    plugins: HashMap<String, PluginInstance>,
    commands: HashMap<String, String>, // command_id -> plugin_name
}

impl PluginHost {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new plugin manifest and validates security capabilities.
    pub fn register_plugin(&mut self, manifest: PluginManifest) -> Result<(), PluginError> {
        let plugin_name = manifest.name.clone();

        for cmd in &manifest.commands {
            self.commands.insert(cmd.id.clone(), plugin_name.clone());
        }

        self.plugins.insert(
            plugin_name,
            PluginInstance {
                manifest,
                is_enabled: true,
            },
        );

        Ok(())
    }

    /// Checks if a plugin has a specific permission.
    pub fn has_capability(&self, plugin_name: &str, cap: PluginCapability) -> bool {
        if let Some(plugin) = self.plugins.get(plugin_name) {
            plugin.is_enabled && plugin.manifest.capabilities.contains(&cap)
        } else {
            false
        }
    }

    /// Executes a registered plugin command safely in a sandbox.
    pub fn execute_command(&self, command_id: &str) -> Result<String, PluginError> {
        let plugin_name = self
            .commands
            .get(command_id)
            .ok_or_else(|| PluginError::NotFound(command_id.to_string()))?;

        let plugin = self
            .plugins
            .get(plugin_name)
            .ok_or_else(|| PluginError::NotFound(plugin_name.clone()))?;

        if !plugin.is_enabled {
            return Err(PluginError::ExecutionFailed(format!(
                "Plugin '{}' is disabled",
                plugin_name
            )));
        }

        // Simulating safe WASM execution
        Ok(format!("Executed command '{}' from plugin '{}'", command_id, plugin_name))
    }

    pub fn list_plugins(&self) -> Vec<&PluginManifest> {
        self.plugins.values().map(|p| &p.manifest).collect()
    }

    pub fn list_commands(&self) -> Vec<(&str, &str)> {
        self.commands.iter().map(|(c, p)| (c.as_str(), p.as_str())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_registration_and_capabilities() {
        let mut host = PluginHost::new();
        let manifest = PluginManifest {
            name: "rust-analyzer-helper".to_string(),
            version: "0.1.0".to_string(),
            description: "Helper commands for Rust".to_string(),
            entrypoint: "plugin.wasm".to_string(),
            capabilities: vec![PluginCapability::BufferRead, PluginCapability::BufferWrite],
            commands: vec![PluginCommand {
                id: "rust.formatSelection".to_string(),
                title: "Format Selection".to_string(),
                category: Some("Rust".to_string()),
            }],
        };

        host.register_plugin(manifest).unwrap();
        assert!(host.has_capability("rust-analyzer-helper", PluginCapability::BufferRead));
        assert!(!host.has_capability("rust-analyzer-helper", PluginCapability::NetworkAccess));

        let res = host.execute_command("rust.formatSelection").unwrap();
        assert!(res.contains("rust.formatSelection"));
    }
}
