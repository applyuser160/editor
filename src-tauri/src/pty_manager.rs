use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
pub struct TerminalProfile {
    pub id: String,
    pub label: String,
    pub executable: String,
    pub args: Vec<String>,
}

pub struct PtySession {
    pub master: Box<dyn MasterPty + Send>,
    pub writer: Box<dyn Write + Send>,
    // Keeping the child alive is required: dropping it can terminate the shell immediately.
    pub child: Box<dyn Child + Send + Sync>,
}

#[derive(Default, Clone)]
pub struct PtyState {
    pub sessions: Arc<Mutex<HashMap<u32, PtySession>>>,
    pub next_id: Arc<Mutex<u32>>,
}

impl PtyState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
        }
    }

    pub fn profiles(&self) -> Vec<TerminalProfile> {
        terminal_profiles()
    }

    pub fn spawn(
        &self,
        app_handle: AppHandle,
        cols: u16,
        rows: u16,
        workspace_root: PathBuf,
        profile_id: Option<&str>,
    ) -> Result<u32, String> {
        let profiles = terminal_profiles();
        let profile = match profile_id {
            Some(id) => profiles
                .into_iter()
                .find(|profile| profile.id == id)
                .ok_or_else(|| format!("Terminal profile '{}' is unavailable", id))?,
            None => profiles
                .into_iter()
                .next()
                .ok_or_else(|| "No supported terminal shell was found".to_string())?,
        };

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to open PTY: {}", error))?;

        let mut command = CommandBuilder::new(&profile.executable);
        command.args(&profile.args);
        command.cwd(workspace_root);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Failed to start {}: {}", profile.label, error))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Failed to clone PTY reader: {}", error))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Failed to take PTY writer: {}", error))?;

        let id = {
            let mut next = self.next_id.lock().unwrap();
            let current = *next;
            *next += 1;
            current
        };

        self.sessions.lock().unwrap().insert(
            id,
            PtySession {
                master: pair.master,
                writer,
                child,
            },
        );

        let app_clone = app_handle.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let data = String::from_utf8_lossy(&buffer[..read]).to_string();
                let _ = app_clone.emit(&format!("pty-data-{}", id), data);
            }
        });

        Ok(id)
    }

    pub fn write(&self, id: u32, data: String) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get_mut(&id)
            .ok_or_else(|| format!("PTY session {} not found", id))?;
        session
            .writer
            .write_all(data.as_bytes())
            .map_err(|error| format!("Failed to write to PTY: {}", error))?;
        session
            .writer
            .flush()
            .map_err(|error| format!("Failed to flush PTY: {}", error))
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&id)
            .ok_or_else(|| format!("PTY session {} not found", id))?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to resize PTY: {}", error))
    }

    pub fn close(&self, id: u32) -> Result<(), String> {
        let mut session = self
            .sessions
            .lock()
            .unwrap()
            .remove(&id)
            .ok_or_else(|| format!("PTY session {} not found", id))?;
        session
            .child
            .kill()
            .map_err(|error| format!("Failed to stop PTY session {}: {}", id, error))
    }
}

fn terminal_profiles() -> Vec<TerminalProfile> {
    #[cfg(target_os = "windows")]
    {
        let mut profiles = Vec::new();
        add_profile(
            &mut profiles,
            "powershell",
            "PowerShell",
            "powershell.exe",
            &[],
        );
        add_profile(&mut profiles, "pwsh", "PowerShell 7", "pwsh.exe", &[]);
        add_profile(
            &mut profiles,
            "command-prompt",
            "Command Prompt",
            "cmd.exe",
            &[],
        );
        if let Some(git_bash) = git_bash_executable() {
            add_profile(
                &mut profiles,
                "git-bash",
                "Git Bash",
                &git_bash,
                &["--login", "-i"],
            );
        }
        add_profile(&mut profiles, "wsl", "Ubuntu (WSL)", "wsl.exe", &[]);
        return profiles;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let mut profiles = Vec::new();
        if let Ok(shell) = env::var("SHELL") {
            add_profile(&mut profiles, "default-shell", "Default shell", &shell, &[]);
        }
        add_profile(&mut profiles, "bash", "Bash", "bash", &["--login"]);
        add_profile(&mut profiles, "zsh", "Zsh", "zsh", &["--login"]);
        add_profile(&mut profiles, "sh", "POSIX shell", "sh", &[]);
        profiles
    }
}

#[cfg(target_os = "windows")]
fn git_bash_executable() -> Option<String> {
    let mut roots = vec![
        PathBuf::from(r"C:\Program Files"),
        PathBuf::from(r"C:\Program Files (x86)"),
    ];
    for variable in [
        "ProgramW6432",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "LocalAppData",
    ] {
        if let Some(path) = env::var_os(variable) {
            roots.push(PathBuf::from(path));
        }
    }

    roots
        .into_iter()
        .flat_map(|root| {
            [
                root.join("Git/bin/bash.exe"),
                root.join("Git/usr/bin/bash.exe"),
            ]
        })
        .find(|candidate| candidate.is_file())
        .map(|candidate| candidate.to_string_lossy().to_string())
}

fn add_profile(
    profiles: &mut Vec<TerminalProfile>,
    id: &str,
    label: &str,
    executable: &str,
    args: &[&str],
) {
    if !command_is_available(executable)
        || profiles
            .iter()
            .any(|existing| existing.executable == executable)
    {
        return;
    }
    profiles.push(TerminalProfile {
        id: id.to_string(),
        label: label.to_string(),
        executable: executable.to_string(),
        args: args.iter().map(|arg| (*arg).to_string()).collect(),
    });
}

fn command_is_available(executable: &str) -> bool {
    let executable_path = Path::new(executable);
    if executable_path.components().count() > 1 {
        return executable_path.is_file();
    }
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths).any(|directory| {
            let candidate = directory.join(executable);
            candidate.is_file()
                || cfg!(target_os = "windows")
                    && directory.join(format!("{}.exe", executable)).is_file()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_at_least_one_available_terminal_profile() {
        assert!(!terminal_profiles().is_empty());
    }

    #[test]
    fn does_not_return_duplicate_executables() {
        let profiles = terminal_profiles();
        let mut executables = std::collections::HashSet::new();
        assert!(profiles
            .iter()
            .all(|profile| executables.insert(&profile.executable)));
    }
}
