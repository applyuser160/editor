//! `editor-plugin`: WASM-based sandboxed plugin host.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub entrypoint: String,
}

pub struct PluginHost {
    pub loaded_plugins: Vec<PluginManifest>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            loaded_plugins: Vec::new(),
        }
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
