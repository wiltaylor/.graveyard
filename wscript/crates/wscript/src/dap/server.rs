//! Debug Adapter Protocol server implementation.
//!
//! Implements a basic DAP server that communicates over TCP.
//! The protocol uses JSON messages with Content-Length headers,
//! similar to LSP.

use crate::engine::Engine;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};

/// The Wscript DAP server.
pub struct WscriptDapServer {
    #[allow(dead_code)]
    engine: Engine,
    source_path: Option<PathBuf>,
    port: u16,
    breakpoints: HashMap<u32, bool>,
    seq: AtomicI64,
}

impl WscriptDapServer {
    pub fn new(engine: Engine, port: u16) -> Self {
        Self {
            engine,
            source_path: None,
            port,
            breakpoints: HashMap::new(),
            seq: AtomicI64::new(1),
        }
    }

    /// Set the source file to debug.
    pub fn set_source(&mut self, path: PathBuf) {
        self.source_path = Some(path);
    }

    #[cfg(feature = "dap")]
    pub async fn serve(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;
        log::info!("DAP server listening on 127.0.0.1:{}", self.port);

        let (mut stream, addr) = listener.accept().await?;
        log::info!("DAP client connected from {}", addr);

        let mut buf = vec![0u8; 8192];

        loop {
            let n = stream.read(&mut buf).await?;
            if n == 0 {
                log::info!("DAP client disconnected");
                break;
            }

            let data = String::from_utf8_lossy(&buf[..n]);

            // Parse Content-Length header and JSON body
            if let Some(json_str) = extract_dap_body(&data)
                && let Ok(msg) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let command = msg["command"].as_str().unwrap_or("");
                    let request_seq = msg["seq"].as_i64().unwrap_or(0);

                    log::debug!("DAP request: {} (seq={})", command, request_seq);

                    let response = match command {
                        "initialize" => self.handle_initialize(request_seq),
                        "launch" => self.handle_launch(request_seq, &msg),
                        "setBreakpoints" => self.handle_set_breakpoints(request_seq, &msg),
                        "configurationDone" => self.handle_configuration_done(request_seq),
                        "threads" => self.handle_threads(request_seq),
                        "stackTrace" => self.handle_stack_trace(request_seq),
                        "scopes" => self.handle_scopes(request_seq),
                        "variables" => self.handle_variables(request_seq),
                        "continue" => self.handle_continue(request_seq),
                        "next" => self.handle_next(request_seq),
                        "stepIn" => self.handle_step_in(request_seq),
                        "stepOut" => self.handle_step_out(request_seq),
                        "disconnect" => {
                            let resp = self.make_response(request_seq, command, serde_json::json!({}));
                            let encoded = encode_dap_message(&resp);
                            stream.write_all(encoded.as_bytes()).await?;
                            break;
                        }
                        _ => {
                            log::warn!("Unhandled DAP command: {}", command);
                            self.make_response(request_seq, command, serde_json::json!({}))
                        }
                    };

                    let encoded = encode_dap_message(&response);
                    stream.write_all(encoded.as_bytes()).await?;
                }
        }

        Ok(())
    }

    fn handle_initialize(&mut self, seq: i64) -> String {
        let body = serde_json::json!({
            "supportsConfigurationDoneRequest": true,
            "supportsFunctionBreakpoints": false,
            "supportsConditionalBreakpoints": false,
            "supportsEvaluateForHovers": false,
            "supportsSetVariable": false,
            "supportsStepInTargetsRequest": false,
        });
        // Send initialized event after response
        self.make_response(seq, "initialize", body)
    }

    fn handle_launch(&mut self, seq: i64, msg: &serde_json::Value) -> String {
        if let Some(program) = msg["arguments"]["program"].as_str() {
            self.source_path = Some(PathBuf::from(program));
            log::info!("Launch: {}", program);
        }
        self.make_response(seq, "launch", serde_json::json!({}))
    }

    fn handle_set_breakpoints(&mut self, seq: i64, msg: &serde_json::Value) -> String {
        let mut verified_breakpoints = Vec::new();
        if let Some(breakpoints) = msg["arguments"]["breakpoints"].as_array() {
            for bp in breakpoints {
                if let Some(line) = bp["line"].as_u64() {
                    self.breakpoints.insert(line as u32, true);
                    verified_breakpoints.push(serde_json::json!({
                        "verified": true,
                        "line": line,
                    }));
                }
            }
        }
        self.make_response(seq, "setBreakpoints", serde_json::json!({
            "breakpoints": verified_breakpoints,
        }))
    }

    fn handle_configuration_done(&mut self, seq: i64) -> String {
        self.make_response(seq, "configurationDone", serde_json::json!({}))
    }

    fn handle_threads(&self, seq: i64) -> String {
        self.make_response(seq, "threads", serde_json::json!({
            "threads": [{
                "id": 1,
                "name": "main"
            }]
        }))
    }

    fn handle_stack_trace(&self, seq: i64) -> String {
        let frames = vec![serde_json::json!({
            "id": 1,
            "name": "main",
            "line": 1,
            "column": 1,
            "source": {
                "name": self.source_path.as_ref().map(|p| p.file_name().unwrap_or_default().to_string_lossy().to_string()).unwrap_or_default(),
                "path": self.source_path.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default(),
            }
        })];
        self.make_response(seq, "stackTrace", serde_json::json!({
            "stackFrames": frames,
            "totalFrames": frames.len(),
        }))
    }

    fn handle_scopes(&self, seq: i64) -> String {
        self.make_response(seq, "scopes", serde_json::json!({
            "scopes": [{
                "name": "Locals",
                "variablesReference": 1,
                "expensive": false,
            }]
        }))
    }

    fn handle_variables(&self, seq: i64) -> String {
        // Return empty variables for now
        self.make_response(seq, "variables", serde_json::json!({
            "variables": []
        }))
    }

    fn handle_continue(&self, seq: i64) -> String {
        self.make_response(seq, "continue", serde_json::json!({
            "allThreadsContinued": true,
        }))
    }

    fn handle_next(&self, seq: i64) -> String {
        self.make_response(seq, "next", serde_json::json!({}))
    }

    fn handle_step_in(&self, seq: i64) -> String {
        self.make_response(seq, "stepIn", serde_json::json!({}))
    }

    fn handle_step_out(&self, seq: i64) -> String {
        self.make_response(seq, "stepOut", serde_json::json!({}))
    }

    fn make_response(&self, request_seq: i64, command: &str, body: serde_json::Value) -> String {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        serde_json::json!({
            "seq": seq,
            "type": "response",
            "request_seq": request_seq,
            "success": true,
            "command": command,
            "body": body,
        }).to_string()
    }
}

/// Extract the JSON body from a DAP message with Content-Length header.
fn extract_dap_body(data: &str) -> Option<&str> {
    if let Some(idx) = data.find("\r\n\r\n") {
        Some(&data[idx + 4..])
    } else if let Some(idx) = data.find("\n\n") {
        Some(&data[idx + 2..])
    } else {
        // Assume the whole thing is JSON
        Some(data)
    }
}

/// Encode a JSON response as a DAP message with Content-Length header.
fn encode_dap_message(json: &str) -> String {
    format!("Content-Length: {}\r\n\r\n{}", json.len(), json)
}
