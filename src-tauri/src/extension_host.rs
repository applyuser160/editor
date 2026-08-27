use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use zip::ZipArchive;

const MAX_VSIX_BYTES: usize = 50 * 1024 * 1024;
const MAX_VSIX_ENTRIES: usize = 10_000;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub main: Option<String>,
    pub activation_events: Vec<String>,
    pub contributes_languages: Vec<String>,
    pub contributes_themes: Vec<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtensionRuntimeStatus {
    pub running: bool,
    pub active_extensions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsixPackageManifest {
    name: String,
    publisher: String,
    version: String,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    activation_events: Vec<String>,
    #[serde(default)]
    contributes: VsixContributes,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsixContributes {
    #[serde(default)]
    languages: Vec<VsixLanguageContribution>,
    #[serde(default)]
    themes: Vec<VsixThemeContribution>,
}

#[derive(Debug, Deserialize)]
struct VsixLanguageContribution {
    id: String,
}

#[derive(Debug, Deserialize)]
struct VsixThemeContribution {
    label: Option<String>,
}

#[derive(Default, Clone)]
pub struct ExtensionHostState {
    pub extensions: Arc<Mutex<Vec<ExtensionManifest>>>,
    pub child_process: Arc<Mutex<Option<Child>>>,
}

impl ExtensionHostState {
    pub fn new() -> Self {
        let mut extensions = builtin_extensions();
        extensions.extend(load_installed_extensions());

        Self {
            extensions: Arc::new(Mutex::new(extensions)),
            child_process: Arc::new(Mutex::new(None)),
        }
    }

    pub fn list_extensions(&self) -> Vec<ExtensionManifest> {
        self.extensions.lock().unwrap().clone()
    }

    pub fn install_vsix(
        &self,
        expected_id: &str,
        bytes: &[u8],
    ) -> Result<ExtensionManifest, String> {
        let manifest = parse_vsix_manifest(bytes)?;
        if manifest.id != expected_id {
            return Err(
                "VSIX manifest identifier does not match the selected extension".to_string(),
            );
        }

        let archive_path = extension_archive_path(&manifest.id);
        let extract_dir = extension_extract_path(&manifest.id);
        if archive_path.exists() || extract_dir.exists() {
            return Err(format!("Extension '{}' is already installed", manifest.id));
        }

        let staging_dir = extract_dir.with_extension("staging");
        let archive_staging = archive_path.with_extension("vsix.staging");
        fs::create_dir_all(extensions_root()).map_err(|error| error.to_string())?;
        fs::write(&archive_staging, bytes).map_err(|error| error.to_string())?;

        if let Err(error) = extract_vsix(bytes, &staging_dir) {
            let _ = fs::remove_file(&archive_staging);
            let _ = fs::remove_dir_all(&staging_dir);
            return Err(error);
        }

        fs::rename(&archive_staging, &archive_path).map_err(|error| error.to_string())?;
        fs::rename(&staging_dir, &extract_dir).map_err(|error| error.to_string())?;

        let mut extensions = self.extensions.lock().unwrap();
        extensions.push(manifest.clone());
        if let Err(error) = persist_installed_extensions(&extensions) {
            extensions.retain(|extension| extension.id != manifest.id);
            let _ = fs::remove_file(&archive_path);
            let _ = fs::remove_dir_all(&extract_dir);
            return Err(error);
        }

        Ok(manifest)
    }

    pub fn uninstall(&self, id: &str) -> Result<(), String> {
        if is_builtin_extension(id) {
            return Err("Built-in extensions cannot be uninstalled".to_string());
        }

        let mut extensions = self.extensions.lock().unwrap();
        let original_len = extensions.len();
        extensions.retain(|extension| extension.id != id);
        if extensions.len() == original_len {
            return Err(format!("Extension '{}' not found", id));
        }
        persist_installed_extensions(&extensions)?;
        let _ = fs::remove_file(extension_archive_path(id));
        let _ = fs::remove_dir_all(extension_extract_path(id));
        Ok(())
    }

    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<ExtensionManifest, String> {
        if is_builtin_extension(id) && !enabled {
            return Err("Built-in extensions cannot be disabled".to_string());
        }

        let mut extensions = self.extensions.lock().unwrap();
        let extension = extensions
            .iter_mut()
            .find(|extension| extension.id == id)
            .ok_or_else(|| format!("Extension '{}' not found", id))?;
        extension.enabled = enabled;
        let updated = extension.clone();
        persist_installed_extensions(&extensions)?;
        Ok(updated)
    }

    pub fn start_sidecar(&self) -> Result<String, String> {
        let node_version = Command::new("node")
            .arg("--version")
            .output()
            .map_err(|_| "Node.js is required to run extensions but was not found".to_string())?;
        if !node_version.status.success() {
            return Err(
                "Node.js is required to run extensions but could not be started".to_string(),
            );
        }

        let mut child_process = self.child_process.lock().unwrap();
        if let Some(child) = child_process.as_mut() {
            if child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                return Ok("Extension runtime is already running".to_string());
            }
        }

        let active_extensions = self
            .extensions
            .lock()
            .unwrap()
            .iter()
            .filter(|extension| extension.enabled)
            .map(resolve_extension_entry)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if active_extensions.is_empty() {
            *child_process = None;
            return Ok("No enabled extensions provide a Node.js entry point".to_string());
        }

        let host_script = write_extension_host_script()?;
        let child = Command::new("node")
            .arg(host_script)
            .args(&active_extensions)
            .spawn()
            .map_err(|error| format!("Failed to start extension runtime: {}", error))?;
        *child_process = Some(child);
        let version = String::from_utf8_lossy(&node_version.stdout)
            .trim()
            .to_string();
        Ok(format!(
            "Extension runtime started with Node.js {} for {} extension(s)",
            version,
            active_extensions.len()
        ))
    }

    pub fn runtime_status(&self) -> Result<ExtensionRuntimeStatus, String> {
        let running = self
            .child_process
            .lock()
            .unwrap()
            .as_mut()
            .map(|child| child.try_wait().map(|status| status.is_none()))
            .transpose()
            .map_err(|error| error.to_string())?
            .unwrap_or(false);
        let active_extensions = if running {
            self.extensions
                .lock()
                .unwrap()
                .iter()
                .filter(|extension| extension.enabled && extension.main.is_some())
                .map(|extension| extension.id.clone())
                .collect()
        } else {
            Vec::new()
        };
        Ok(ExtensionRuntimeStatus {
            running,
            active_extensions,
        })
    }
}

fn resolve_extension_entry(extension: &ExtensionManifest) -> Result<Option<PathBuf>, String> {
    let Some(main) = &extension.main else {
        return Ok(None);
    };
    let main_path = Path::new(main);
    if main_path.is_absolute()
        || main_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!(
            "Extension '{}' has an unsafe main entry",
            extension.id
        ));
    }
    let root = extension_extract_path(&extension.id);
    let entry = root.join(main_path).canonicalize().map_err(|error| {
        format!(
            "Extension '{}' entry point could not be resolved: {}",
            extension.id, error
        )
    })?;
    if !entry.starts_with(&root) || !entry.is_file() {
        return Err(format!(
            "Extension '{}' has an invalid main entry",
            extension.id
        ));
    }
    Ok(Some(entry))
}

fn write_extension_host_script() -> Result<PathBuf, String> {
    let script_path = extensions_root().join("oxide-extension-host.cjs");
    fs::create_dir_all(extensions_root()).map_err(|error| error.to_string())?;
    fs::write(
        &script_path,
        r#"const entries = process.argv.slice(2);
for (const entry of entries) {
  try {
    const extension = require(entry);
    if (typeof extension.activate === "function") {
      Promise.resolve(extension.activate({ subscriptions: [], commands: { registerCommand() {} } }))
        .then(() => process.stdout.write(`activated:${entry}\\n`))
        .catch((error) => process.stderr.write(`activation-error:${entry}:${error.message}\\n`));
    }
  } catch (error) {
    process.stderr.write(`load-error:${entry}:${error.message}\\n`);
  }
}
setInterval(() => {}, 2 ** 31 - 1);
"#,
    )
    .map_err(|error| error.to_string())?;
    Ok(script_path)
}

fn builtin_extensions() -> Vec<ExtensionManifest> {
    vec![
        ExtensionManifest {
            id: "rust-lang.rust-analyzer".to_string(),
            name: "rust-analyzer".to_string(),
            version: "0.4.0".to_string(),
            description: "Rust language support and IntelliSense".to_string(),
            main: None,
            activation_events: vec![],
            contributes_languages: vec!["rust".to_string()],
            contributes_themes: vec![],
            enabled: true,
        },
        ExtensionManifest {
            id: "vscode.theme-defaults".to_string(),
            name: "Default Themes".to_string(),
            version: "1.0.0".to_string(),
            description: "Default Dark+, Light+, and High Contrast themes".to_string(),
            main: None,
            activation_events: vec![],
            contributes_languages: vec![],
            contributes_themes: vec![
                "vscode-dark-plus".to_string(),
                "vs".to_string(),
                "hc-black".to_string(),
            ],
            enabled: true,
        },
    ]
}

fn parse_vsix_manifest(bytes: &[u8]) -> Result<ExtensionManifest, String> {
    if bytes.is_empty() || bytes.len() > MAX_VSIX_BYTES {
        return Err("VSIX archive exceeds the 50 MiB size limit".to_string());
    }

    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Invalid VSIX archive: {}", error))?;
    if archive.len() > MAX_VSIX_ENTRIES {
        return Err("VSIX archive contains too many entries".to_string());
    }

    let package_index = archive
        .file_names()
        .position(|name| name == "extension/package.json" || name == "package.json")
        .ok_or_else(|| "VSIX archive does not contain extension/package.json".to_string())?;
    let mut package = archive
        .by_index(package_index)
        .map_err(|error| format!("Failed to read VSIX package manifest: {}", error))?;
    if package.size() > MAX_MANIFEST_BYTES as u64 {
        return Err("VSIX package manifest is too large".to_string());
    }

    let mut content = String::new();
    package
        .read_to_string(&mut content)
        .map_err(|error| format!("Failed to read VSIX package manifest: {}", error))?;
    let package: VsixPackageManifest = serde_json::from_str(&content)
        .map_err(|error| format!("Invalid VSIX package manifest: {}", error))?;

    validate_manifest_field(&package.publisher, "publisher")?;
    validate_manifest_field(&package.name, "name")?;
    validate_manifest_field(&package.version, "version")?;

    Ok(ExtensionManifest {
        id: format!("{}.{}", package.publisher, package.name),
        name: package.display_name.unwrap_or(package.name),
        version: package.version,
        description: package.description.unwrap_or_default(),
        main: package.main,
        activation_events: package.activation_events,
        contributes_languages: package
            .contributes
            .languages
            .into_iter()
            .map(|language| language.id)
            .collect(),
        contributes_themes: package
            .contributes
            .themes
            .into_iter()
            .filter_map(|theme| theme.label)
            .collect(),
        enabled: true,
    })
}

fn extract_vsix(bytes: &[u8], destination: &PathBuf) -> Result<(), String> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| format!("Invalid VSIX archive: {}", error))?;
    if archive.len() > MAX_VSIX_ENTRIES {
        return Err("VSIX archive contains too many entries".to_string());
    }
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let Some(relative_path) = entry.enclosed_name().map(|path| path.to_owned()) else {
            return Err("VSIX archive contains an unsafe path".to_string());
        };
        let output_path = destination.join(relative_path);
        if entry.is_dir() {
            fs::create_dir_all(&output_path).map_err(|error| error.to_string())?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut output = File::create(&output_path).map_err(|error| error.to_string())?;
        std::io::copy(&mut entry, &mut output).map_err(|error| error.to_string())?;
        output.flush().map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn validate_manifest_field(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 255
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err(format!("VSIX manifest contains an invalid {}", field));
    }
    Ok(())
}

fn extensions_root() -> PathBuf {
    let mut path = dirs::data_local_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("oxide-editor");
    path.push("extensions");
    path
}

fn extension_archive_path(id: &str) -> PathBuf {
    extensions_root().join(format!("{}.vsix", id))
}

fn extension_extract_path(id: &str) -> PathBuf {
    extensions_root().join(id)
}

fn installed_extensions_store_path() -> PathBuf {
    extensions_root().join("installed.json")
}

fn load_installed_extensions() -> Vec<ExtensionManifest> {
    fs::read_to_string(installed_extensions_store_path())
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn persist_installed_extensions(extensions: &[ExtensionManifest]) -> Result<(), String> {
    let installed: Vec<_> = extensions
        .iter()
        .filter(|extension| !is_builtin_extension(&extension.id))
        .cloned()
        .collect();
    fs::create_dir_all(extensions_root()).map_err(|error| error.to_string())?;
    let content = serde_json::to_string_pretty(&installed).map_err(|error| error.to_string())?;
    fs::write(installed_extensions_store_path(), content).map_err(|error| error.to_string())
}

fn is_builtin_extension(id: &str) -> bool {
    matches!(id, "rust-lang.rust-analyzer" | "vscode.theme-defaults")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_manifest_identifiers() {
        assert!(validate_manifest_field("publisher-name", "publisher").is_ok());
        assert!(validate_manifest_field("bad/name", "publisher").is_err());
    }

    #[test]
    fn rejects_oversized_or_empty_vsix() {
        assert!(parse_vsix_manifest(&[]).is_err());
        assert!(parse_vsix_manifest(&vec![0; MAX_VSIX_BYTES + 1]).is_err());
    }

    #[test]
    fn rejects_parent_traversal_in_extension_entry_point() {
        let mut extension = builtin_extensions().remove(0);
        extension.main = Some("../outside.cjs".to_string());
        assert!(resolve_extension_entry(&extension).is_err());
    }
}
