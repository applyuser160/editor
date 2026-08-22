//! `editor-lsp`: Asynchronous Language Server Protocol (LSP) client and diagnostics engine.

use editor_core::Position;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

/// Severity levels for LSP diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// A diagnostic item (error, warning) reported by the language server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub start: Position,
    pub end: Position,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
}

/// Code completion item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    pub kind_name: Option<String>,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    pub insert_text: Option<String>,
}

/// Location in a file for definition jumps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub start: Position,
    pub end: Position,
}

/// Raw JSON-RPC 2.0 Message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcMessage {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

impl JsonRpcMessage {
    pub fn request(id: i64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: Some(method.to_string()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    pub fn notification(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: Some(method.to_string()),
            params: Some(params),
            result: None,
            error: None,
        }
    }

    /// Formats the message with HTTP-style LSP header.
    pub fn to_lsp_payload(&self) -> serde_json::Result<Vec<u8>> {
        let json_body = serde_json::to_string(self)?;
        let header = format!("Content-Length: {}\r\n\r\n", json_body.len());
        let mut payload = header.into_bytes();
        payload.extend_from_slice(json_body.as_bytes());
        Ok(payload)
    }

    /// Parses an incoming stream buffer, extracting payload and returning consumed bytes.
    pub fn parse_lsp_stream(buffer: &[u8]) -> Option<(JsonRpcMessage, usize)> {
        let text = std::str::from_utf8(buffer).ok()?;
        let header_end = text.find("\r\n\r\n")?;
        let headers = &text[..header_end];

        let mut content_length: Option<usize> = None;
        for line in headers.lines() {
            if let Some(len_str) = line.strip_prefix("Content-Length:") {
                content_length = len_str.trim().parse().ok();
            }
        }

        let length = content_length?;
        let body_start = header_end + 4;
        let total_size = body_start + length;

        if buffer.len() < total_size {
            return None; // Incomplete message
        }

        let body_bytes = &buffer[body_start..total_size];
        let message: JsonRpcMessage = serde_json::from_slice(body_bytes).ok()?;
        Some((message, total_size))
    }
}

/// Store for managing file diagnostics published by LSP.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticsStore {
    diagnostics_by_uri: Arc<Mutex<HashMap<String, Vec<Diagnostic>>>>,
}

impl DiagnosticsStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn update_diagnostics(&self, uri: &str, diagnostics: Vec<Diagnostic>) {
        let mut map = self.diagnostics_by_uri.lock().await;
        map.insert(uri.to_string(), diagnostics);
    }

    pub async fn get_diagnostics(&self, uri: &str) -> Vec<Diagnostic> {
        let map = self.diagnostics_by_uri.lock().await;
        map.get(uri).cloned().unwrap_or_default()
    }
}

/// LSP Client session managing connection and asynchronous RPCs.
pub struct LspClient {
    next_id: AtomicI64,
    pending_requests: Arc<Mutex<HashMap<i64, oneshot::Sender<Result<Value, String>>>>>,
    diagnostics_store: DiagnosticsStore,
    outgoing_tx: Option<mpsc::Sender<JsonRpcMessage>>,
}

impl LspClient {
    pub fn new() -> Self {
        Self {
            next_id: AtomicI64::new(1),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            diagnostics_store: DiagnosticsStore::new(),
            outgoing_tx: None,
        }
    }

    pub fn set_outgoing_tx(&mut self, tx: mpsc::Sender<JsonRpcMessage>) {
        self.outgoing_tx = Some(tx);
    }

    pub async fn send_message(&self, msg: JsonRpcMessage) -> Result<(), String> {
        if let Some(tx) = &self.outgoing_tx {
            tx.send(msg).await.map_err(|e| e.to_string())
        } else {
            Err("Outgoing transport channel not configured".to_string())
        }
    }

    /// Generates next request ID.
    pub fn next_request_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    /// Handles an incoming JSON-RPC message.
    pub async fn handle_incoming_message(&self, msg: JsonRpcMessage) {
        // Check if it's a response to a pending request
        if let Some(id) = msg.id {
            let mut pending = self.pending_requests.lock().await;
            if let Some(sender) = pending.remove(&id) {
                if let Some(result) = msg.result {
                    let _ = sender.send(Ok(result));
                } else if let Some(error) = msg.error {
                    let _ = sender.send(Err(error.to_string()));
                }
            }
        } else if let Some(method) = &msg.method {
            // Check for notifications
            if method == "textDocument/publishDiagnostics" {
                if let Some(params) = msg.params {
                    if let Some(uri) = params.get("uri").and_then(|u| u.as_str()) {
                        if let Some(diag_array) = params.get("diagnostics").and_then(|d| d.as_array()) {
                            let mut parsed_diags = Vec::new();
                            for item in diag_array {
                                let message = item.get("message").and_then(|m| m.as_str()).unwrap_or_default().to_string();
                                let severity_raw = item.get("severity").and_then(|s| s.as_u64()).unwrap_or(1);
                                let severity = match severity_raw {
                                    1 => DiagnosticSeverity::Error,
                                    2 => DiagnosticSeverity::Warning,
                                    3 => DiagnosticSeverity::Information,
                                    _ => DiagnosticSeverity::Hint,
                                };

                                let range = item.get("range");
                                let start_line = range.and_then(|r| r.get("start")).and_then(|s| s.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                                let start_col = range.and_then(|r| r.get("start")).and_then(|s| s.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize;
                                let end_line = range.and_then(|r| r.get("end")).and_then(|e| e.get("line")).and_then(|l| l.as_u64()).unwrap_or(0) as usize;
                                let end_col = range.and_then(|r| r.get("end")).and_then(|e| e.get("character")).and_then(|c| c.as_u64()).unwrap_or(0) as usize;

                                parsed_diags.push(Diagnostic {
                                    start: Position::new(start_line, start_col),
                                    end: Position::new(end_line, end_col),
                                    severity,
                                    code: item.get("code").map(|c| c.to_string()),
                                    source: item.get("source").and_then(|s| s.as_str()).map(String::from),
                                    message,
                                });
                            }
                            self.diagnostics_store.update_diagnostics(uri, parsed_diags).await;
                        }
                    }
                }
            }
        }
    }
}

impl Default for LspClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lsp_stream_parser() {
        let msg = JsonRpcMessage::request(1, "initialize", serde_json::json!({"processId": 1234}));
        let payload = msg.to_lsp_payload().unwrap();

        let parsed = JsonRpcMessage::parse_lsp_stream(&payload);
        assert!(parsed.is_some());
        let (parsed_msg, consumed) = parsed.unwrap();
        assert_eq!(consumed, payload.len());
        assert_eq!(parsed_msg.id, Some(1));
        assert_eq!(parsed_msg.method.as_deref(), Some("initialize"));
    }

    #[tokio::test]
    async fn test_diagnostics_store() {
        let store = DiagnosticsStore::new();
        let diags = vec![Diagnostic {
            start: Position::new(10, 4),
            end: Position::new(10, 12),
            severity: DiagnosticSeverity::Error,
            code: Some("E0308".to_string()),
            source: Some("rustc".to_string()),
            message: "mismatched types".to_string(),
        }];

        store.update_diagnostics("file:///src/main.rs", diags.clone()).await;
        let retrieved = store.get_diagnostics("file:///src/main.rs").await;
        assert_eq!(retrieved.len(), 1);
        assert_eq!(retrieved[0].message, "mismatched types");
        assert_eq!(retrieved[0].severity, DiagnosticSeverity::Error);
    }
}
