use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub struct LspSession {
    pub stdin: Arc<Mutex<ChildStdin>>,
    pub next_request_id: AtomicU64,
    pub pending_requests: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Result<Value, String>>>>>,
}

#[derive(Default, Clone)]
pub struct LspState {
    pub sessions: Arc<Mutex<HashMap<String, LspSession>>>,
}

impl LspState {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start_server(&self, app_handle: AppHandle, lang: &str, workspace_root: &str) -> Result<String, String> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.contains_key(lang) {
            return Ok(format!("LSP server for '{}' is already running.", lang));
        }

        let cmd_name = match lang {
            "rust" => "rust-analyzer",
            "typescript" | "javascript" => "typescript-language-server",
            "python" => "pyright-langserver",
            "go" => "gopls",
            _ => return Err(format!("Unsupported LSP language: {}", lang)),
        };

        // Determine working directory & workspace root
        let cur_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let working_dir = if lang == "rust" && cur_dir.join("src-tauri").join("Cargo.toml").exists() {
            cur_dir.join("src-tauri")
        } else if !workspace_root.is_empty() && workspace_root != "." {
            std::path::PathBuf::from(workspace_root)
        } else {
            cur_dir.clone()
        };

        let mut child = match Command::new(cmd_name)
            .args(if lang == "typescript" || lang == "javascript" { vec!["--stdio"] } else { vec![] })
            .current_dir(&working_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                return Err(format!("Could not spawn '{}': {}. Please install it to enable full LSP intelligence.", cmd_name, e));
            }
        };

        let stdin = Arc::new(Mutex::new(child.stdin.take().ok_or("Failed to open stdin for LSP")?));
        let stdout = child.stdout.take().ok_or("Failed to open stdout for LSP")?;

        let pending_requests: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Result<Value, String>>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = pending_requests.clone();
        let app_clone = app_handle.clone();
        let lang_str = lang.to_string();
        let stdin_reader_clone = stdin.clone();

        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut header_line = String::new();
                if reader.read_line(&mut header_line).is_err() || header_line.is_empty() {
                    break;
                }

                if header_line.starts_with("Content-Length:") {
                    let parts: Vec<&str> = header_line.trim().split(':').collect();
                    if parts.len() == 2 {
                        if let Ok(content_length) = parts[1].trim().parse::<usize>() {
                            let mut empty_line = String::new();
                            let _ = reader.read_line(&mut empty_line);

                            let mut body_buf = vec![0u8; content_length];
                            if reader.read_exact(&mut body_buf).is_ok() {
                                if let Ok(json_msg) = serde_json::from_slice::<Value>(&body_buf) {
                                    handle_incoming_lsp_message(
                                        &lang_str,
                                        &app_clone,
                                        &pending_clone,
                                        &stdin_reader_clone,
                                        json_msg,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        });

        let session = LspSession {
            stdin: stdin.clone(),
            next_request_id: AtomicU64::new(1),
            pending_requests,
        };

        let root_uri = format!("file:///{}", working_dir.to_string_lossy().replace('\\', "/"));
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "workspaceFolders": [
                { "uri": root_uri, "name": "workspace" }
            ],
            "capabilities": {
                "workspace": {
                    "configuration": true,
                    "workspaceFolders": true
                },
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "completion": { "completionItem": { "snippetSupport": true } },
                    "definition": { "dynamicRegistration": true, "linkSupport": true },
                    "formatting": { "dynamicRegistration": true },
                    "publishDiagnostics": { "relatedInformation": true }
                }
            }
        });

        // 1. Send initialize request (id: 1)
        send_message_raw(&stdin, &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": init_params
        }));

        // 2. Send initialized notification
        send_message_raw(&stdin, &serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        }));

        sessions.insert(lang.to_string(), session);
        Ok(format!("LSP server '{}' initialized successfully.", cmd_name))
    }

    pub fn send_notification(&self, lang: &str, method: &str, params: Value) -> Result<(), String> {
        let sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get(lang) {
            send_message_raw(&session.stdin, &serde_json::json!({
                "jsonrpc": "2.0",
                "method": method,
                "params": params
            }));
            Ok(())
        } else {
            Err(format!("LSP session for '{}' not running", lang))
        }
    }

    pub async fn send_request(&self, lang: &str, method: &str, params: Value) -> Result<Value, String> {
        for attempt in 1..=5 {
            let rx = {
                let sessions = self.sessions.lock().unwrap();
                if let Some(session) = sessions.get(lang) {
                    let id = session.next_request_id.fetch_add(1, Ordering::SeqCst);
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    session.pending_requests.lock().unwrap().insert(id, tx);

                    let req_payload = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "method": method,
                        "params": params
                    });

                    send_message_raw(&session.stdin, &req_payload);
                    rx
                } else {
                    return Err(format!("LSP session for '{}' not running", lang));
                }
            };

            match tokio::time::timeout(std::time::Duration::from_secs(5), rx).await {
                Ok(Ok(Ok(val))) => return Ok(val),
                Ok(Ok(Err(err_msg))) => {
                    if err_msg.contains("content modified") && attempt < 5 {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    return Err(err_msg);
                }
                Ok(Err(_)) => {
                    if attempt < 5 {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    return Err("LSP channel closed".to_string());
                }
                Err(_) => {
                    if attempt < 5 {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        continue;
                    }
                    return Err("LSP request timed out".to_string());
                }
            }
        }

        Err("LSP request retry exhausted".to_string())
    }
}

fn send_message_raw(stdin: &Arc<Mutex<ChildStdin>>, payload: &Value) {
    if let Ok(mut handle) = stdin.lock() {
        let body = payload.to_string();
        let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
        let _ = handle.write_all(msg.as_bytes());
        let _ = handle.flush();
    }
}

fn handle_incoming_lsp_message(
    lang: &str,
    app: &AppHandle,
    pending: &Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Result<Value, String>>>>>,
    stdin: &Arc<Mutex<ChildStdin>>,
    msg: Value,
) {
    if let Some(id_val) = msg.get("id") {
        if let Some(id) = id_val.as_u64() {
            if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
                let res_val = if method == "workspace/configuration" {
                    serde_json::json!([{}])
                } else {
                    Value::Null
                };
                send_message_raw(stdin, &serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": res_val
                }));
                return;
            }

            if let Some(tx) = pending.lock().unwrap().remove(&id) {
                if let Some(err) = msg.get("error") {
                    let err_msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("LSP error");
                    let _ = tx.send(Err(err_msg.to_string()));
                } else {
                    let result = msg.get("result").cloned().unwrap_or(Value::Null);
                    let _ = tx.send(Ok(result));
                }
                return;
            }
        }
    }

    if let Some(method) = msg.get("method").and_then(|m| m.as_str()) {
        if method == "textDocument/publishDiagnostics" {
            if let Some(params) = msg.get("params") {
                let _ = app.emit("lsp-diagnostics", serde_json::json!({
                    "lang": lang,
                    "params": params
                }));
            }
        }
    }
}
