use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

pub struct LspSession {
    pub stdin: ChildStdin,
    pub next_request_id: AtomicU64,
    pub pending_requests: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
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

        let mut child = match Command::new(cmd_name)
            .args(if lang == "typescript" || lang == "javascript" { vec!["--stdio"] } else { vec![] })
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

        let stdin = child.stdin.take().ok_or("Failed to open stdin for LSP")?;
        let stdout = child.stdout.take().ok_or("Failed to open stdout for LSP")?;

        let pending_requests: Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let pending_clone = pending_requests.clone();
        let app_clone = app_handle.clone();
        let lang_str = lang.to_string();

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
                                    handle_incoming_lsp_message(&lang_str, &app_clone, &pending_clone, json_msg);
                                }
                            }
                        }
                    }
                }
            }
        });

        let mut session = LspSession {
            stdin,
            next_request_id: AtomicU64::new(1),
            pending_requests,
        };

        let root_uri = format!("file:///{}", workspace_root.replace('\\', "/"));
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "rootUri": root_uri,
            "capabilities": {
                "textDocument": {
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "completion": { "completionItem": { "snippetSupport": true } },
                    "definition": { "dynamicRegistration": true },
                    "formatting": { "dynamicRegistration": true },
                    "publishDiagnostics": { "relatedInformation": true }
                }
            }
        });

        send_notification_raw(&mut session.stdin, "initialize", &init_params);
        send_notification_raw(&mut session.stdin, "initialized", &serde_json::json!({}));

        sessions.insert(lang.to_string(), session);
        Ok(format!("LSP server '{}' initialized successfully.", cmd_name))
    }

    pub fn send_notification(&self, lang: &str, method: &str, params: Value) -> Result<(), String> {
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(session) = sessions.get_mut(lang) {
            send_notification_raw(&mut session.stdin, method, &params);
            Ok(())
        } else {
            Err(format!("LSP session for '{}' not running", lang))
        }
    }

    pub async fn send_request(&self, lang: &str, method: &str, params: Value) -> Result<Value, String> {
        let (_req_id, rx) = {
            let mut sessions = self.sessions.lock().unwrap();
            if let Some(session) = sessions.get_mut(lang) {
                let id = session.next_request_id.fetch_add(1, Ordering::SeqCst);
                let (tx, rx) = tokio::sync::oneshot::channel();
                session.pending_requests.lock().unwrap().insert(id, tx);

                let req_payload = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": method,
                    "params": params
                });

                let body = req_payload.to_string();
                let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
                let _ = session.stdin.write_all(msg.as_bytes());
                let _ = session.stdin.flush();
                (id, rx)
            } else {
                return Err(format!("LSP session for '{}' not running", lang));
            }
        };

        match tokio::time::timeout(std::time::Duration::from_secs(4), rx).await {
            Ok(Ok(val)) => Ok(val),
            _ => Err("LSP request timed out or cancelled".to_string()),
        }
    }
}

fn send_notification_raw(stdin: &mut ChildStdin, method: &str, params: &Value) {
    let payload = serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });
    let body = payload.to_string();
    let msg = format!("Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = stdin.write_all(msg.as_bytes());
    let _ = stdin.flush();
}

fn handle_incoming_lsp_message(
    lang: &str,
    app: &AppHandle,
    pending: &Arc<Mutex<HashMap<u64, tokio::sync::oneshot::Sender<Value>>>>,
    msg: Value,
) {
    if let Some(id_val) = msg.get("id") {
        if let Some(id) = id_val.as_u64() {
            if let Some(tx) = pending.lock().unwrap().remove(&id) {
                let result = msg.get("result").cloned().unwrap_or(Value::Null);
                let _ = tx.send(result);
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
