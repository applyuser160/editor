use crate::debug_config::{DebugConfiguration, DebugRequest};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

pub struct DebugSessionState {
    session: Mutex<Option<Arc<DebugSession>>>,
}

impl DebugSessionState {
    pub fn new() -> Self {
        Self {
            session: Mutex::new(None),
        }
    }

    pub fn start(
        &self,
        app: AppHandle,
        configuration: DebugConfiguration,
        breakpoints: Vec<SourceBreakpoint>,
    ) -> Result<(), String> {
        self.stop()?;
        let session = Arc::new(DebugSession::spawn(app, &configuration)?);

        let initialize = session.request(
            "initialize",
            json!({
                "clientID": "oxide-editor",
                "clientName": "Oxide Editor",
                "adapterID": configuration.adapter_type,
                "pathFormat": "path",
                "linesStartAt1": true,
                "columnsStartAt1": true,
                "supportsVariableType": true,
                "supportsRunInTerminalRequest": true,
            }),
        )?;
        session.emit("debug-event", json!({ "event": "capabilities", "body": initialize.get("body").cloned().unwrap_or(Value::Null) }));

        let launch_arguments = configuration_arguments(&configuration)?;
        let request = match configuration.request {
            DebugRequest::Launch => "launch",
            DebugRequest::Attach => "attach",
        };
        session.request(request, launch_arguments)?;

        for breakpoint in breakpoints {
            session.set_breakpoints(&breakpoint)?;
        }
        session.request("configurationDone", json!({}))?;

        *self.session.lock().map_err(|_| "Debug session state is unavailable".to_string())? = Some(session);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let session = self
            .session
            .lock()
            .map_err(|_| "Debug session state is unavailable".to_string())?
            .take();
        if let Some(session) = session {
            session.stop()?;
        }
        Ok(())
    }

    pub fn require_session(&self) -> Result<Arc<DebugSession>, String> {
        self.session
            .lock()
            .map_err(|_| "Debug session state is unavailable".to_string())?
            .clone()
            .ok_or_else(|| "No active debug session".to_string())
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBreakpoint {
    pub source: String,
    #[serde(default)]
    pub lines: Vec<u32>,
}

pub fn check_adapter(adapter_type: &str) -> Result<AdapterStatus, String> {
    match adapter_type {
        "lldb" => {
            let executable = ["lldb-dap", "lldb-vscode"]
                .iter()
                .find(|candidate| command_is_available(candidate))
                .map(|candidate| (*candidate).to_string());
            Ok(AdapterStatus {
                available: executable.is_some(),
                adapter_type: adapter_type.to_string(),
                executable,
                message: "LLDB DAP requires lldb-dap (or legacy lldb-vscode) on PATH".to_string(),
            })
        }
        "python" => {
            let python = ["python3", "python"]
                .iter()
                .find(|candidate| command_is_available(candidate))
                .map(|candidate| (*candidate).to_string());
            let available = python
                .as_deref()
                .map(|command| {
                    Command::new(command)
                        .args(["-c", "import debugpy"])
                        .status()
                        .map(|status| status.success())
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            Ok(AdapterStatus {
                available,
                adapter_type: adapter_type.to_string(),
                executable: if available { python } else { None },
                message: "Python debugging requires Python and the debugpy package (python -m pip install debugpy)".to_string(),
            })
        }
        _ => Err(format!("Unsupported debug adapter type '{adapter_type}'")),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdapterStatus {
    pub available: bool,
    pub adapter_type: String,
    pub executable: Option<String>,
    pub message: String,
}

pub struct DebugSession {
    app: AppHandle,
    stdin: Mutex<ChildStdin>,
    child: Mutex<Option<Child>>,
    next_sequence: Mutex<i64>,
    pending: Arc<Mutex<HashMap<i64, mpsc::Sender<Value>>>>,
}

impl DebugSession {
    fn spawn(app: AppHandle, configuration: &DebugConfiguration) -> Result<Self, String> {
        let (command, arguments) = adapter_command(&configuration.adapter_type)?;
        let mut child = Command::new(&command)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("Could not start the {} debug adapter: {error}", configuration.adapter_type))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "Debug adapter did not provide a standard-input channel".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Debug adapter did not provide a standard-output channel".to_string())?;
        let stderr = child.stderr.take();

        let session = Self {
            app: app.clone(),
            stdin: Mutex::new(stdin),
            child: Mutex::new(Some(child)),
            next_sequence: Mutex::new(1),
            pending: Arc::new(Mutex::new(HashMap::new())),
        };
        session.start_reader(stdout);
        if let Some(stderr) = stderr {
            session.start_stderr_reader(stderr);
        }
        Ok(session)
    }

    fn start_reader(&self, stdout: impl Read + Send + 'static) {
        let app = self.app.clone();
        let pending = self.pending.clone();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_dap_message(&mut reader) {
                    Ok(message) => match message.get("type").and_then(Value::as_str) {
                        Some("response") => {
                            if let Some(request_sequence) = message.get("request_seq").and_then(Value::as_i64) {
                                if let Ok(mut pending) = pending.lock() {
                                    if let Some(sender) = pending.remove(&request_sequence) {
                                        let _ = sender.send(message);
                                    }
                                }
                            }
                        }
                        Some("event") => {
                            let _ = app.emit("debug-event", message);
                        }
                        _ => {
                            let _ = app.emit("debug-event", json!({
                                "event": "protocolError",
                                "body": { "message": "Received an unsupported DAP message" }
                            }));
                        }
                    },
                    Err(error) => {
                        let _ = app.emit("debug-event", json!({
                            "event": "adapterClosed",
                            "body": { "message": error }
                        }));
                        break;
                    }
                }
            }
        });
    }

    fn start_stderr_reader(&self, stderr: impl Read + Send + 'static) {
        let app = self.app.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().map_while(Result::ok) {
                let _ = app.emit("debug-event", json!({
                    "event": "output",
                    "body": { "category": "stderr", "output": format!("{line}\n") }
                }));
            }
        });
    }

    pub fn request(&self, command: &str, arguments: Value) -> Result<Value, String> {
        let sequence = {
            let mut next = self
                .next_sequence
                .lock()
                .map_err(|_| "Debug adapter sequence state is unavailable".to_string())?;
            let current = *next;
            *next += 1;
            current
        };
        let (sender, receiver) = mpsc::channel();
        self.pending
            .lock()
            .map_err(|_| "Debug adapter request state is unavailable".to_string())?
            .insert(sequence, sender);

        let message = json!({
            "seq": sequence,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        if let Err(error) = write_dap_message(&mut *self.stdin.lock().map_err(|_| "Debug adapter input is unavailable".to_string())?, &message) {
            self.remove_pending(sequence);
            return Err(format!("Could not send '{command}' to the debug adapter: {error}"));
        }

        let response = receiver.recv_timeout(REQUEST_TIMEOUT).map_err(|_| {
            self.remove_pending(sequence);
            format!("The debug adapter did not respond to '{command}' within {} seconds", REQUEST_TIMEOUT.as_secs())
        })?;
        if response.get("success").and_then(Value::as_bool) == Some(false) {
            let message = response
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("The debug adapter rejected the request");
            return Err(format!("Debug adapter rejected '{command}': {message}"));
        }
        Ok(response)
    }

    pub fn set_breakpoints(&self, breakpoint: &SourceBreakpoint) -> Result<Value, String> {
        if breakpoint.source.trim().is_empty() {
            return Err("Breakpoint source path is required".to_string());
        }
        let source = std::fs::canonicalize(&breakpoint.source)
            .map_err(|_| format!("Breakpoint source file does not exist: {}", breakpoint.source))?;
        let requested: Vec<Value> = breakpoint
            .lines
            .iter()
            .copied()
            .filter(|line| *line > 0)
            .map(|line| json!({ "line": line }))
            .collect();
        self.request(
            "setBreakpoints",
            json!({
                "source": { "path": source.to_string_lossy() },
                "breakpoints": requested,
                "sourceModified": false,
            }),
        )
    }

    pub fn stop(&self) -> Result<(), String> {
        let _ = self.request("disconnect", json!({ "restart": false, "terminateDebuggee": true }));
        if let Some(mut child) = self
            .child
            .lock()
            .map_err(|_| "Debug adapter process state is unavailable".to_string())?
            .take()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.emit("debug-event", json!({ "event": "terminated", "body": {} }));
        Ok(())
    }

    pub fn emit(&self, event: &str, payload: Value) {
        let _ = self.app.emit(event, payload);
    }

    fn remove_pending(&self, sequence: i64) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.remove(&sequence);
        }
    }
}

fn adapter_command(adapter_type: &str) -> Result<(String, Vec<String>), String> {
    let status = check_adapter(adapter_type)?;
    if !status.available {
        return Err(status.message);
    }
    match adapter_type {
        "lldb" => Ok((status.executable.unwrap_or_else(|| "lldb-dap".to_string()), Vec::new())),
        "python" => Ok((
            status.executable.unwrap_or_else(|| "python3".to_string()),
            vec!["-m".to_string(), "debugpy.adapter".to_string()],
        )),
        _ => Err(format!("Unsupported debug adapter type '{adapter_type}'")),
    }
}

fn configuration_arguments(configuration: &DebugConfiguration) -> Result<Value, String> {
    let mut arguments = serde_json::Map::new();
    arguments.insert("name".to_string(), Value::String(configuration.name.clone()));
    arguments.insert("type".to_string(), Value::String(configuration.adapter_type.clone()));
    arguments.insert(
        "request".to_string(),
        Value::String(match configuration.request {
            DebugRequest::Launch => "launch".to_string(),
            DebugRequest::Attach => "attach".to_string(),
        }),
    );
    if let Some(program) = &configuration.program {
        arguments.insert("program".to_string(), Value::String(program.clone()));
    }
    if let Some(cwd) = &configuration.cwd {
        arguments.insert("cwd".to_string(), Value::String(cwd.clone()));
    }
    arguments.insert("args".to_string(), serde_json::to_value(&configuration.args).map_err(|error| error.to_string())?);
    arguments.insert("env".to_string(), serde_json::to_value(&configuration.env).map_err(|error| error.to_string())?);
    Ok(Value::Object(arguments))
}

fn command_is_available(command: &str) -> bool {
    let status = if cfg!(target_os = "windows") {
        Command::new("where").arg(command).status()
    } else {
        Command::new("sh")
            .args(["-c", &format!("command -v {} >/dev/null 2>&1", shell_escape(command))])
            .status()
    };
    status.map(|status| status.success()).unwrap_or(false)
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn write_dap_message(writer: &mut impl Write, message: &Value) -> Result<(), String> {
    let body = serde_json::to_vec(message).map_err(|error| error.to_string())?;
    writer
        .write_all(format!("Content-Length: {}\r\n\r\n", body.len()).as_bytes())
        .and_then(|_| writer.write_all(&body))
        .and_then(|_| writer.flush())
        .map_err(|error| error.to_string())
}

fn read_dap_message(reader: &mut impl BufRead) -> Result<Value, String> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("Debug adapter closed its output stream".to_string());
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| "Debug adapter sent an invalid Content-Length header".to_string())?,
            );
        }
    }
    let content_length = content_length.ok_or_else(|| "Debug adapter response omitted Content-Length".to_string())?;
    if content_length > 16 * 1024 * 1024 {
        return Err("Debug adapter response exceeds the 16 MiB protocol limit".to_string());
    }
    let mut payload = vec![0_u8; content_length];
    reader.read_exact(&mut payload).map_err(|error| error.to_string())?;
    serde_json::from_slice(&payload).map_err(|error| format!("Debug adapter sent invalid JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn dap_messages_round_trip_without_losing_unicode() {
        let original = json!({ "seq": 1, "type": "event", "event": "output", "body": { "output": "こんにちは\n" } });
        let mut wire = Vec::new();
        write_dap_message(&mut wire, &original).unwrap();
        let parsed = read_dap_message(&mut Cursor::new(wire)).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn unsupported_adapter_is_rejected() {
        assert!(check_adapter("node").is_err());
    }
}
