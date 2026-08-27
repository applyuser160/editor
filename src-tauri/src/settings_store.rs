use keyring::v1::Entry;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};

const CONFIG_VERSION: u32 = 1;
const MAX_JSON_BYTES: u64 = 1024 * 1024;
const MAX_LANGUAGE_SCOPES: usize = 64;
const MAX_KEYBINDINGS: usize = 512;
const MAX_PROFILES: usize = 128;
const SERVICE_PREFIX: &str = "oxide-editor";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub user_settings: Value,
    pub workspace_settings: Value,
    pub language_settings: BTreeMap<String, Value>,
    pub keybindings: Value,
    pub profiles: Value,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            user_settings: Value::Object(Map::new()),
            workspace_settings: Value::Object(Map::new()),
            language_settings: BTreeMap::new(),
            keybindings: Value::Array(Vec::new()),
            profiles: Value::Array(Vec::new()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MigrationMarker {
    version: u32,
}

#[derive(Debug, Clone)]
struct SettingsPaths {
    user_settings: PathBuf,
    keybindings: PathBuf,
    profiles: PathBuf,
    languages: PathBuf,
    migration_marker: PathBuf,
    workspace_settings: PathBuf,
}

impl SettingsPaths {
    fn from_app(app: &AppHandle, workspace_root: &Path) -> Result<Self, String> {
        let base = app.path().app_config_dir().map_err(|error| {
            format!("Could not locate the application configuration directory: {error}")
        })?;
        Ok(Self::from_base(&base, workspace_root))
    }

    fn from_base(base: &Path, workspace_root: &Path) -> Self {
        Self {
            user_settings: base.join("settings.json"),
            keybindings: base.join("keybindings.json"),
            profiles: base.join("profiles.json"),
            languages: base.join("language-settings.json"),
            migration_marker: base.join("migrations").join("local-storage-v1.json"),
            workspace_settings: workspace_root.join(".oxide").join("settings.json"),
        }
    }

    #[cfg(test)]
    fn all_configuration_files(&self) -> [&Path; 5] {
        [
            &self.user_settings,
            &self.workspace_settings,
            &self.languages,
            &self.keybindings,
            &self.profiles,
        ]
    }
}

pub fn load(app: &AppHandle, workspace_root: &Path) -> Result<SettingsSnapshot, String> {
    let paths = SettingsPaths::from_app(app, workspace_root)?;
    load_from_paths(&paths)
}

pub fn save(
    app: &AppHandle,
    workspace_root: &Path,
    snapshot: SettingsSnapshot,
) -> Result<SettingsSnapshot, String> {
    validate_snapshot(&snapshot)?;
    let paths = SettingsPaths::from_app(app, workspace_root)?;
    save_to_paths(&paths, &snapshot)?;
    Ok(snapshot)
}

pub fn migrate_from_local_storage(
    app: &AppHandle,
    workspace_root: &Path,
    snapshot: SettingsSnapshot,
) -> Result<SettingsSnapshot, String> {
    validate_snapshot(&snapshot)?;
    let paths = SettingsPaths::from_app(app, workspace_root)?;

    if paths.migration_marker.exists() {
        return load_from_paths(&paths);
    }

    let existing = load_from_paths(&paths)?;
    let merged = SettingsSnapshot {
        user_settings: prefer_existing(existing.user_settings, snapshot.user_settings),
        workspace_settings: prefer_existing(
            existing.workspace_settings,
            snapshot.workspace_settings,
        ),
        language_settings: merge_language_settings(
            existing.language_settings,
            snapshot.language_settings,
        ),
        keybindings: prefer_existing_array(existing.keybindings, snapshot.keybindings),
        profiles: prefer_existing_array(existing.profiles, snapshot.profiles),
    };
    validate_snapshot(&merged)?;
    save_to_paths(&paths, &merged)?;
    write_json_atomically(
        &paths.migration_marker,
        &MigrationMarker {
            version: CONFIG_VERSION,
        },
    )?;
    Ok(merged)
}

pub fn store_credential(service: &str, account: &str, secret: &str) -> Result<(), String> {
    validate_credential_identifier("service", service)?;
    validate_credential_identifier("account", account)?;
    if secret.is_empty() {
        return Err("Credential value must not be empty".to_string());
    }
    if secret.len() > 16 * 1024 {
        return Err("Credential value exceeds the maximum size".to_string());
    }

    let entry = credential_entry(service, account)?;
    entry.set_password(secret).map_err(|_| {
        "Could not store the credential in the operating system credential store".to_string()
    })
}

pub fn has_credential(service: &str, account: &str) -> Result<bool, String> {
    validate_credential_identifier("service", service)?;
    validate_credential_identifier("account", account)?;
    let entry = credential_entry(service, account)?;
    match entry.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::v1::Error::NoEntry) => Ok(false),
        Err(_) => Err("Could not query the operating system credential store".to_string()),
    }
}

pub fn delete_credential(service: &str, account: &str) -> Result<(), String> {
    validate_credential_identifier("service", service)?;
    validate_credential_identifier("account", account)?;
    let entry = credential_entry(service, account)?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::v1::Error::NoEntry) => Ok(()),
        Err(_) => Err(
            "Could not delete the credential from the operating system credential store"
                .to_string(),
        ),
    }
}

fn credential_entry(service: &str, account: &str) -> Result<Entry, String> {
    Entry::new(&format!("{SERVICE_PREFIX}/{service}"), account)
        .map_err(|_| "The operating system credential store is unavailable".to_string())
}

fn load_from_paths(paths: &SettingsPaths) -> Result<SettingsSnapshot, String> {
    let snapshot = SettingsSnapshot {
        user_settings: read_json_or_default(&paths.user_settings, Value::Object(Map::new()))?,
        workspace_settings: read_json_or_default(
            &paths.workspace_settings,
            Value::Object(Map::new()),
        )?,
        language_settings: read_json_or_default(&paths.languages, BTreeMap::new())?,
        keybindings: read_json_or_default(&paths.keybindings, Value::Array(Vec::new()))?,
        profiles: read_json_or_default(&paths.profiles, Value::Array(Vec::new()))?,
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn save_to_paths(paths: &SettingsPaths, snapshot: &SettingsSnapshot) -> Result<(), String> {
    write_json_atomically(&paths.user_settings, &snapshot.user_settings)?;
    write_json_atomically(&paths.workspace_settings, &snapshot.workspace_settings)?;
    write_json_atomically(&paths.languages, &snapshot.language_settings)?;
    write_json_atomically(&paths.keybindings, &snapshot.keybindings)?;
    write_json_atomically(&paths.profiles, &snapshot.profiles)?;
    Ok(())
}

fn read_json_or_default<T>(path: &Path, default: T) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(default);
    }
    let metadata =
        fs::metadata(path).map_err(|_| "Could not inspect a configuration file".to_string())?;
    if metadata.len() > MAX_JSON_BYTES {
        return Err(format!(
            "Configuration file exceeds the {} MiB limit: {}",
            MAX_JSON_BYTES / 1024 / 1024,
            path.display()
        ));
    }
    let content = fs::read_to_string(path)
        .map_err(|_| format!("Could not read configuration file: {}", path.display()))?;
    serde_json::from_str(&content).map_err(|error| {
        format!(
            "Invalid JSON in configuration file {}: {error}",
            path.display()
        )
    })
}

fn write_json_atomically<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not serialize configuration data: {error}"))?;
    if content.len() as u64 > MAX_JSON_BYTES {
        return Err(format!(
            "Configuration data exceeds the {} MiB limit",
            MAX_JSON_BYTES / 1024 / 1024
        ));
    }

    let parent = path
        .parent()
        .ok_or_else(|| "Configuration file has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|_| "Could not create the configuration directory".to_string())?;

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("configuration.json");
    let temporary = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));

    let result = (|| -> Result<(), String> {
        let mut file = File::create(&temporary)
            .map_err(|_| "Could not create a temporary configuration file".to_string())?;
        file.write_all(&content)
            .map_err(|_| "Could not write a temporary configuration file".to_string())?;
        file.write_all(b"\n")
            .map_err(|_| "Could not finalize a temporary configuration file".to_string())?;
        file.sync_all()
            .map_err(|_| "Could not flush a temporary configuration file".to_string())?;
        drop(file);
        fs::rename(&temporary, path)
            .map_err(|_| "Could not atomically replace the configuration file".to_string())?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn validate_snapshot(snapshot: &SettingsSnapshot) -> Result<(), String> {
    validate_settings_object("User settings", &snapshot.user_settings)?;
    validate_settings_object("Workspace settings", &snapshot.workspace_settings)?;

    if snapshot.language_settings.len() > MAX_LANGUAGE_SCOPES {
        return Err("Too many language-specific setting scopes".to_string());
    }
    for (language, settings) in &snapshot.language_settings {
        validate_language_identifier(language)?;
        validate_settings_object("Language settings", settings)?;
    }

    validate_array("Keybindings", &snapshot.keybindings, MAX_KEYBINDINGS)?;
    validate_array("Profiles", &snapshot.profiles, MAX_PROFILES)?;
    reject_sensitive_fields(&snapshot.keybindings)?;
    reject_sensitive_fields(&snapshot.profiles)?;
    Ok(())
}

fn validate_settings_object(label: &str, value: &Value) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| format!("{label} must be a JSON object"))?;
    if object.len() > 128 {
        return Err(format!("{label} contains too many values"));
    }
    reject_sensitive_fields(value)
}

fn validate_array(label: &str, value: &Value, maximum: usize) -> Result<(), String> {
    let array = value
        .as_array()
        .ok_or_else(|| format!("{label} must be a JSON array"))?;
    if array.len() > maximum {
        return Err(format!("{label} exceeds the maximum entry count"));
    }
    Ok(())
}

fn reject_sensitive_fields(value: &Value) -> Result<(), String> {
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
                if [
                    "secret",
                    "token",
                    "password",
                    "credential",
                    "privatekey",
                    "accesskey",
                    "apikey",
                ]
                .iter()
                .any(|needle| normalized.contains(needle))
                {
                    return Err(format!("Sensitive field '{key}' must be stored in the operating system credential store"));
                }
                reject_sensitive_fields(nested)?;
            }
        }
        Value::Array(items) => {
            for item in items {
                reject_sensitive_fields(item)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn validate_language_identifier(language: &str) -> Result<(), String> {
    if language.is_empty()
        || language.len() > 64
        || !language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("Invalid language identifier".to_string());
    }
    Ok(())
}

fn validate_credential_identifier(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'@'))
    {
        return Err(format!("Invalid credential {label}"));
    }
    Ok(())
}

fn prefer_existing(existing: Value, legacy: Value) -> Value {
    if existing
        .as_object()
        .is_some_and(|object| !object.is_empty())
    {
        existing
    } else {
        legacy
    }
}

fn prefer_existing_array(existing: Value, legacy: Value) -> Value {
    if existing.as_array().is_some_and(|items| !items.is_empty()) {
        existing
    } else {
        legacy
    }
}

fn merge_language_settings(
    mut existing: BTreeMap<String, Value>,
    legacy: BTreeMap<String, Value>,
) -> BTreeMap<String, Value> {
    for (language, settings) in legacy {
        existing.entry(language).or_insert(settings);
    }
    existing
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("oxide-settings-store-{name}-{nonce}"))
    }

    #[test]
    fn atomically_persisted_snapshot_round_trips() {
        let root = temporary_directory("round-trip");
        let workspace = root.join("workspace");
        let paths = SettingsPaths::from_base(&root.join("app"), &workspace);
        let snapshot = SettingsSnapshot {
            user_settings: serde_json::json!({"fontSize": 16}),
            workspace_settings: serde_json::json!({"tabSize": 2}),
            language_settings: BTreeMap::from([(
                "rust".to_string(),
                serde_json::json!({"minimap": false}),
            )]),
            keybindings: serde_json::json!([{ "command": "save", "key": "Ctrl+S" }]),
            profiles: serde_json::json!([]),
        };

        save_to_paths(&paths, &snapshot).unwrap();
        let loaded = load_from_paths(&paths).unwrap();
        assert_eq!(loaded.user_settings, snapshot.user_settings);
        assert_eq!(loaded.workspace_settings, snapshot.workspace_settings);
        assert_eq!(loaded.language_settings, snapshot.language_settings);
        assert_eq!(loaded.keybindings, snapshot.keybindings);
        assert!(!paths
            .all_configuration_files()
            .iter()
            .any(|path| path.with_extension("json.tmp").exists()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_plaintext_secret_fields_from_all_configuration_scopes() {
        let snapshot = SettingsSnapshot {
            user_settings: serde_json::json!({"apiToken": "do-not-store-me"}),
            ..SettingsSnapshot::default()
        };
        let error = validate_snapshot(&snapshot).unwrap_err();
        assert!(error.contains("credential store"));
    }

    #[test]
    fn rejects_invalid_language_and_credential_identifiers() {
        assert!(validate_language_identifier("../../python").is_err());
        assert!(validate_credential_identifier("service", "git provider").is_err());
        assert!(validate_credential_identifier("account", "developer@example.com").is_ok());
    }
}
