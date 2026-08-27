use crate::workspace::WorkspaceState;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use tauri::{AppHandle, Emitter, Manager};
use zip::ZipArchive;

const MAX_VSIX_BYTES: usize = 50 * 1024 * 1024;
const MAX_VSIX_ENTRIES: usize = 10_000;
const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_EXTENSION_READ_BYTES: u64 = 10 * 1024 * 1024;

/// A command statically contributed by a VSIX package.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionCommand {
    pub command: String,
    pub title: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub enablement: Option<String>,
}

/// Permissions are deliberately Oxide-specific. VS Code manifests do not provide a
/// reliable cross-extension permission model, so the first implementation grants
/// only read access to the trusted workspace after the user explicitly enables an
/// extension. Write, network, and process access remain unavailable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExtensionPermissions {
    #[serde(default = "default_workspace_read_permission")]
    pub workspace_read: bool,
}

const fn default_workspace_read_permission() -> bool {
    true
}

impl Default for ExtensionPermissions {
    fn default() -> Self {
        Self {
            workspace_read: default_workspace_read_permission(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub browser: Option<String>,
    #[serde(default)]
    pub activation_events: Vec<String>,
    #[serde(default)]
    pub extension_kind: Vec<String>,
    #[serde(default)]
    pub engines_vscode: Option<String>,
    #[serde(default)]
    pub contributes_languages: Vec<String>,
    #[serde(default)]
    pub contributes_themes: Vec<String>,
    #[serde(default)]
    pub contributes_commands: Vec<ExtensionCommand>,
    #[serde(default)]
    pub permissions: ExtensionPermissions,
    pub enabled: bool,
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
    browser: Option<String>,
    #[serde(default)]
    activation_events: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_extension_kind")]
    extension_kind: Vec<String>,
    #[serde(default)]
    engines: VsixEngines,
    #[serde(default)]
    contributes: VsixContributes,
}

#[derive(Debug, Default, Deserialize)]
struct VsixEngines {
    #[serde(default)]
    vscode: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VsixContributes {
    #[serde(default)]
    languages: Vec<VsixLanguageContribution>,
    #[serde(default)]
    themes: Vec<VsixThemeContribution>,
    #[serde(default)]
    commands: Vec<VsixCommandContribution>,
}

#[derive(Debug, Deserialize)]
struct VsixLanguageContribution {
    id: String,
}

#[derive(Debug, Deserialize)]
struct VsixThemeContribution {
    label: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VsixCommandContribution {
    command: String,
    title: String,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    enablement: Option<String>,
}

fn deserialize_extension_kind<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ExtensionKind {
        One(String),
        Many(Vec<String>),
    }

    match Option::<ExtensionKind>::deserialize(deserializer)? {
        Some(ExtensionKind::One(value)) => Ok(vec![value]),
        Some(ExtensionKind::Many(values)) => Ok(values),
        None => Ok(Vec::new()),
    }
}

/// The native state owns the child process while a separate writer lock provides
/// serialized line-delimited JSON-RPC messages to the Node extension host.
#[derive(Default, Clone)]
pub struct ExtensionHostState {
    pub extensions: Arc<Mutex<Vec<ExtensionManifest>>>,
    child_process: Arc<Mutex<Option<Child>>>,
    child_stdin: Arc<Mutex<Option<ChildStdin>>>,
}

impl ExtensionHostState {
    pub fn new() -> Self {
        let mut extensions = builtin_extensions();
        extensions.extend(load_installed_extensions());

        Self {
            extensions: Arc::new(Mutex::new(extensions)),
            child_process: Arc::new(Mutex::new(None)),
            child_stdin: Arc::new(Mutex::new(None)),
        }
    }

    pub fn list_extensions(&self) -> Vec<ExtensionManifest> {
        self.extensions.lock().unwrap().clone()
    }

    pub fn list_contributed_commands(&self) -> Vec<ExtensionCommand> {
        self.extensions
            .lock()
            .unwrap()
            .iter()
            .filter(|extension| extension.enabled)
            .flat_map(|extension| extension.contributes_commands.clone())
            .collect()
    }

    pub fn install_vsix(
        &self,
        expected_id: &str,
        bytes: &[u8],
    ) -> Result<ExtensionManifest, String> {
        let mut manifest = parse_vsix_manifest(bytes)?;
        if manifest.id != expected_id {
            return Err(
                "VSIX manifest identifier does not match the selected extension".to_string(),
            );
        }

        // Downloading a package is not consent to execute arbitrary code. The user
        // must use the existing enable toggle after reviewing the extension.
        manifest.enabled = false;
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
        drop(extensions);
        let _ = fs::remove_file(extension_archive_path(id));
        let _ = fs::remove_dir_all(extension_extract_path(id));
        let _ = self.send_to_host(&json!({ "type": "reload" }));
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
        drop(extensions);

        // Reloading avoids retaining code or registrations from an extension that
        // was just disabled and discovers extensions enabled after host startup.
        let _ = self.send_to_host(&json!({ "type": "reload" }));
        Ok(updated)
    }

    pub fn start_sidecar(
        &self,
        app: AppHandle,
        workspace: WorkspaceState,
    ) -> Result<String, String> {
        workspace.require_trusted()?;
        if self.is_running() {
            self.send_to_host(&json!({ "type": "reload" }))?;
            return Ok("Extension host is already running and has been reloaded.".to_string());
        }

        let node_binary = std::env::var("OXIDE_NODE_BINARY").unwrap_or_else(|_| "node".to_string());
        let version = Command::new(&node_binary)
            .arg("--version")
            .output()
            .map_err(|error| format!("Node.js is required for extensions: {}", error))?;
        if !version.status.success() {
            return Err("Node.js could not be started for the extension host".to_string());
        }

        let host_script = extension_host_script_path(&app)?;
        let extensions_root = extensions_root();
        let workspace_root = workspace.root();
        let mut child = Command::new(&node_binary)
            .arg("--experimental-permission")
            // Node v22 accepts one filesystem path per permission flag. The
            // host script itself also needs an explicit read grant.
            .arg(format!(
                "--allow-fs-read={}",
                extensions_root.to_string_lossy()
            ))
            .arg(format!(
                "--allow-fs-read={}",
                workspace_root.to_string_lossy()
            ))
            .arg(format!("--allow-fs-read={}", host_script.to_string_lossy()))
            // Do not expose credentials or application-specific environment values
            // to third-party extension code. PATH is retained only to preserve Node's
            // normal child-independent runtime behavior.
            .env_clear()
            .env("PATH", std::env::var("PATH").unwrap_or_default())
            .arg(host_script)
            .arg("--extensions-root")
            .arg(extensions_root)
            .arg("--workspace")
            .arg(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Failed to start extension host: {}", error))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Extension host stdin is unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Extension host stdout is unavailable".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Extension host stderr is unavailable".to_string())?;

        *self.child_stdin.lock().unwrap() = Some(stdin);
        *self.child_process.lock().unwrap() = Some(child);

        let reader_state = self.clone();
        let reader_workspace = workspace.clone();
        let reader_app = app.clone();
        thread::spawn(move || {
            read_extension_host_stdout(stdout, reader_state, reader_workspace, reader_app)
        });

        let stderr_app = app.clone();
        thread::spawn(move || read_extension_host_stderr(stderr, stderr_app));

        Ok(format!(
            "Extension host started with Node.js {}. Enabled extensions run with read-only access to the trusted workspace.",
            String::from_utf8_lossy(&version.stdout).trim()
        ))
    }

    pub fn execute_command(&self, command: &str, args: Value) -> Result<(), String> {
        let command_is_enabled = self.extensions.lock().unwrap().iter().any(|extension| {
            extension.enabled
                && extension
                    .contributes_commands
                    .iter()
                    .any(|contribution| contribution.command == command)
        });
        if !command_is_enabled {
            return Err(format!(
                "Extension command '{}' is unavailable or its extension is disabled",
                command
            ));
        }
        self.send_to_host(&json!({
            "type": "execute-command",
            "command": command,
            "args": args,
        }))
    }

    pub fn notify_activation_event(&self, event: &str) -> Result<(), String> {
        self.send_to_host(&json!({ "type": "activate-event", "event": event }))
    }

    pub fn request_language_provider(
        &self,
        provider_id: &str,
        kind: &str,
        request_id: &str,
        document: Value,
        position: Value,
    ) -> Result<(), String> {
        if provider_id.is_empty() || provider_id.len() > 512 {
            return Err("Extension language provider identifier is invalid".to_string());
        }
        if !matches!(kind, "completion" | "hover") {
            return Err("Unsupported extension language provider kind".to_string());
        }
        let source = document
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                "Extension language request does not include document text".to_string()
            })?;
        if source.len() > MAX_EXTENSION_READ_BYTES as usize {
            return Err(format!(
                "Extension language requests are limited to {} MiB",
                MAX_EXTENSION_READ_BYTES / 1024 / 1024
            ));
        }
        self.send_to_host(&json!({
            "type": "language-provider-request",
            "providerId": provider_id,
            "kind": kind,
            "requestId": request_id,
            "document": document,
            "position": position,
        }))
    }

    pub fn stop_sidecar(&self) -> Result<(), String> {
        let _ = self.send_to_host(&json!({ "type": "shutdown" }));
        // Always take the writer lock before the child-process lock. The stdout
        // reader uses the same order when the host exits, which prevents a
        // stop/reap race from deadlocking the UI command handler.
        *self.child_stdin.lock().unwrap() = None;
        let mut child = self.child_process.lock().unwrap();
        if let Some(process) = child.as_mut() {
            let _ = process.kill();
            let _ = process.wait();
        }
        *child = None;
        Ok(())
    }

    fn is_running(&self) -> bool {
        let running = {
            let mut child = self.child_process.lock().unwrap();
            let running = match child.as_mut() {
                Some(process) => matches!(process.try_wait(), Ok(None)),
                None => false,
            };
            if !running {
                *child = None;
            }
            running
        };
        if !running {
            *self.child_stdin.lock().unwrap() = None;
        }
        running
    }

    fn send_to_host(&self, message: &Value) -> Result<(), String> {
        let mut stdin = self.child_stdin.lock().unwrap();
        let writer = stdin
            .as_mut()
            .ok_or_else(|| "Extension host is not running".to_string())?;
        let encoded = serde_json::to_string(message).map_err(|error| error.to_string())?;
        writer
            .write_all(encoded.as_bytes())
            .and_then(|_| writer.write_all(b"\n"))
            .and_then(|_| writer.flush())
            .map_err(|error| format!("Failed to send a message to extension host: {}", error))
    }

    fn can_read_workspace(&self, extension_id: &str) -> bool {
        self.extensions.lock().unwrap().iter().any(|extension| {
            extension.id == extension_id
                && extension.enabled
                && extension.permissions.workspace_read
        })
    }

    fn handle_host_request(
        &self,
        workspace: &WorkspaceState,
        app: &AppHandle,
        message: &Value,
    ) -> Result<Value, String> {
        let method = message
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| "Extension host request does not include a method".to_string())?;
        let params = message.get("params").cloned().unwrap_or(Value::Null);
        let extension_id = params
            .get("extensionId")
            .and_then(Value::as_str)
            .ok_or_else(|| "Extension host request does not include an extensionId".to_string())?;

        match method {
            "window.showInformationMessage"
            | "window.showWarningMessage"
            | "window.showErrorMessage" => {
                app.emit(
                    "extension-window-message",
                    json!({
                        "extensionId": extension_id,
                        "severity": method,
                        "message": params.get("message").and_then(Value::as_str).unwrap_or(""),
                    }),
                )
                .map_err(|error| error.to_string())?;
                Ok(Value::Null)
            }
            "workspace.fs.readFile" => {
                if !self.can_read_workspace(extension_id) {
                    return Err(format!(
                        "Extension '{}' does not have workspace read permission",
                        extension_id
                    ));
                }
                let uri = params
                    .get("uri")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "workspace.fs.readFile requires a file URI".to_string())?;
                let raw_path = file_uri_to_path(uri)?;
                let path = workspace.resolve_path(&raw_path)?;
                let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
                if metadata.len() > MAX_EXTENSION_READ_BYTES {
                    return Err(format!(
                        "Extension file reads are limited to {} MiB",
                        MAX_EXTENSION_READ_BYTES / 1024 / 1024
                    ));
                }
                let bytes = fs::read(&path).map_err(|error| error.to_string())?;
                Ok(json!({ "base64": STANDARD.encode(bytes) }))
            }
            "workspace.getConfiguration" => Ok(json!({})),
            _ => Err(format!("Unsupported extension host request: {}", method)),
        }
    }
}

fn read_extension_host_stdout(
    stdout: ChildStdout,
    state: ExtensionHostState,
    workspace: WorkspaceState,
    app: AppHandle,
) {
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let message: Value = match serde_json::from_str(&line) {
            Ok(message) => message,
            Err(error) => {
                let _ = app.emit(
                    "extension-host-event",
                    json!({ "type": "host.protocol-error", "message": error.to_string() }),
                );
                continue;
            }
        };

        if message.get("type").and_then(Value::as_str) == Some("request") {
            let id = message.get("id").cloned().unwrap_or(Value::Null);
            let response = match state.handle_host_request(&workspace, &app, &message) {
                Ok(result) => json!({ "type": "response", "id": id, "result": result }),
                Err(error) => json!({ "type": "response", "id": id, "error": error }),
            };
            let _ = state.send_to_host(&response);
        } else {
            let _ = app.emit("extension-host-event", message);
        }
    }

    *state.child_stdin.lock().unwrap() = None;
    *state.child_process.lock().unwrap() = None;
    let _ = app.emit("extension-host-event", json!({ "type": "host.stopped" }));
}

fn read_extension_host_stderr(stderr: std::process::ChildStderr, app: AppHandle) {
    for line in BufReader::new(stderr).lines().map_while(Result::ok) {
        if !line.trim().is_empty() {
            let _ = app.emit(
                "extension-host-event",
                json!({ "type": "host.stderr", "message": line }),
            );
        }
    }
}

fn extension_host_script_path(app: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("extension-host").join("host.mjs");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("extension-host")
        .join("host.mjs");
    if development.is_file() {
        Ok(development)
    } else {
        Err("The Oxide extension host script was not found in application resources".to_string())
    }
}

fn file_uri_to_path(uri: &str) -> Result<String, String> {
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| "Only file:// URIs are supported by extension workspace APIs".to_string())?;
    let decoded = percent_decode(path)?;
    #[cfg(windows)]
    let decoded = decoded.strip_prefix('/').unwrap_or(&decoded).to_string();
    Ok(decoded)
}

fn percent_decode(input: &str) -> Result<String, String> {
    let mut output = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("Malformed percent-encoded file URI".to_string());
            }
            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            output.push(high * 16 + low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).map_err(|_| "File URI is not valid UTF-8".to_string())
}

fn hex_value(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("Malformed percent-encoded file URI".to_string()),
    }
}

fn builtin_extensions() -> Vec<ExtensionManifest> {
    vec![
        ExtensionManifest {
            id: "rust-lang.rust-analyzer".to_string(),
            name: "rust-analyzer".to_string(),
            version: "0.4.0".to_string(),
            description: "Rust language support and IntelliSense".to_string(),
            main: None,
            browser: None,
            activation_events: vec![],
            extension_kind: vec![],
            engines_vscode: None,
            contributes_languages: vec!["rust".to_string()],
            contributes_themes: vec![],
            contributes_commands: vec![],
            permissions: ExtensionPermissions::default(),
            enabled: true,
        },
        ExtensionManifest {
            id: "vscode.theme-defaults".to_string(),
            name: "Default Themes".to_string(),
            version: "1.0.0".to_string(),
            description: "Default Dark+, Light+, and High Contrast themes".to_string(),
            main: None,
            browser: None,
            activation_events: vec![],
            extension_kind: vec![],
            engines_vscode: None,
            contributes_languages: vec![],
            contributes_themes: vec![
                "vscode-dark-plus".to_string(),
                "vs".to_string(),
                "hc-black".to_string(),
            ],
            contributes_commands: vec![],
            permissions: ExtensionPermissions::default(),
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

    let contributes_commands = package
        .contributes
        .commands
        .into_iter()
        .map(|command| {
            validate_command_id(&command.command)?;
            if command.title.trim().is_empty() || command.title.len() > 255 {
                return Err("VSIX command title is invalid".to_string());
            }
            Ok(ExtensionCommand {
                command: command.command,
                title: command.title,
                category: command.category,
                enablement: command.enablement,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    Ok(ExtensionManifest {
        id: format!("{}.{}", package.publisher, package.name),
        name: package.display_name.unwrap_or(package.name),
        version: package.version,
        description: package.description.unwrap_or_default(),
        main: package.main,
        browser: package.browser,
        activation_events: package.activation_events,
        extension_kind: package.extension_kind,
        engines_vscode: package.engines.vscode,
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
        contributes_commands,
        permissions: ExtensionPermissions::default(),
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

fn validate_command_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 255
        || value.starts_with('.')
        || value.ends_with('.')
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err("VSIX manifest contains an invalid command identifier".to_string());
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
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    fn vsix_with_entries(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut archive = zip::ZipWriter::new(&mut output);
            let options = SimpleFileOptions::default();
            for (path, content) in entries {
                archive.start_file(path, options).unwrap();
                archive.write_all(content.as_bytes()).unwrap();
            }
            archive.finish().unwrap();
        }
        output.into_inner()
    }

    #[test]
    fn validates_manifest_identifiers() {
        assert!(validate_manifest_field("publisher-name", "publisher").is_ok());
        assert!(validate_manifest_field("bad/name", "publisher").is_err());
        assert!(validate_command_id("sample.hello-world").is_ok());
        assert!(validate_command_id("../unsafe").is_err());
    }

    #[test]
    fn rejects_oversized_or_empty_vsix() {
        assert!(parse_vsix_manifest(&[]).is_err());
        assert!(parse_vsix_manifest(&vec![0; MAX_VSIX_BYTES + 1]).is_err());
    }

    #[test]
    fn rejects_invalid_archives_and_missing_package_manifests() {
        assert!(parse_vsix_manifest(b"not an archive").is_err());
        let archive = vsix_with_entries(&[("README.md", "no extension manifest")]);
        assert!(parse_vsix_manifest(&archive).is_err());
    }

    #[test]
    fn reads_executable_manifest_metadata_and_commands() {
        let archive = vsix_with_entries(&[(
            "extension/package.json",
            r#"{
              "name":"hello","publisher":"oxide","version":"1.0.0",
              "main":"./out/extension.js","browser":"./dist/web.js",
              "activationEvents":["onCommand:oxide.hello"],
              "extensionKind":"workspace","engines":{"vscode":"^1.90.0"},
              "contributes":{"commands":[{"command":"oxide.hello","title":"Hello","category":"Oxide"}]}
            }"#,
        )]);
        let manifest = parse_vsix_manifest(&archive).unwrap();
        assert_eq!(manifest.id, "oxide.hello");
        assert_eq!(manifest.main.as_deref(), Some("./out/extension.js"));
        assert_eq!(manifest.browser.as_deref(), Some("./dist/web.js"));
        assert_eq!(manifest.extension_kind, vec!["workspace"]);
        assert_eq!(manifest.engines_vscode.as_deref(), Some("^1.90.0"));
        assert_eq!(manifest.contributes_commands.len(), 1);
        assert_eq!(manifest.contributes_commands[0].command, "oxide.hello");
    }

    #[test]
    fn decodes_file_uri_without_accepting_malformed_sequences() {
        assert_eq!(
            file_uri_to_path("file:///tmp/hello%20world.txt").unwrap(),
            "/tmp/hello world.txt"
        );
        assert!(file_uri_to_path("https://example.test/file").is_err());
        assert!(file_uri_to_path("file:///tmp/%GG").is_err());
    }

    #[test]
    fn rejects_archive_paths_that_escape_the_extension_directory() {
        let archive = vsix_with_entries(&[("../outside.txt", "unsafe")]);
        let destination =
            std::env::temp_dir().join(format!("oxide-editor-vsix-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&destination);

        let result = extract_vsix(&archive, &destination);

        assert!(result.is_err());
        assert!(!destination.join("outside.txt").exists());
        let _ = fs::remove_dir_all(destination);
    }
}
