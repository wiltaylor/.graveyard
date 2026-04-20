pub mod client;
pub mod preamble;
pub mod server;

use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

/// Env var name holding the agent session UUID for child processes.
pub const ENV_AGENT_ID: &str = "ACIDDOG_AGENT_ID";

/// Env var name holding the full path to the agent IPC socket for child processes.
pub const ENV_AGENT_SOCKET: &str = "ACIDDOG_AGENT_SOCKET";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub agent_id: Uuid,
    #[serde(flatten)]
    pub cmd: RequestCmd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum RequestCmd {
    Test,
    /// Send a chat message to another agent on the team by its config id.
    /// The orchestrator wraps the body as `<Message from="sender">body</Message>`
    /// before delivering it, and enforces the sender's `can_message` allowlist.
    SendMessage {
        to: String,
        body: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: Some(message.into()),
            error: None,
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: None,
            error: Some(error.into()),
        }
    }
}

/// An inbound request routed from the socket server to the application,
/// together with a oneshot channel for the handler to deliver a reply.
#[derive(Debug)]
pub struct AgentSocketEvent {
    pub request: Request,
    pub reply: oneshot::Sender<Response>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_serializes_with_flat_cmd_tag() {
        let id = Uuid::nil();
        let req = Request {
            agent_id: id,
            cmd: RequestCmd::Test,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"test\""), "got: {s}");
        assert!(s.contains("\"agent_id\""), "got: {s}");
    }

    #[test]
    fn request_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: id,
            cmd: RequestCmd::Test,
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back.agent_id, id);
        assert!(matches!(back.cmd, RequestCmd::Test));
    }

    #[test]
    fn send_message_serializes_with_to_and_body() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::SendMessage {
                to: "coder".into(),
                body: "hi".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"send_message\""), "got: {s}");
        assert!(s.contains("\"to\":\"coder\""), "got: {s}");
        assert!(s.contains("\"body\":\"hi\""), "got: {s}");
    }

    #[test]
    fn send_message_roundtrips() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::SendMessage {
                to: "coder".into(),
                body: "hello world".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::SendMessage { to, body } => {
                assert_eq!(to, "coder");
                assert_eq!(body, "hello world");
            }
            other => panic!("expected SendMessage, got {other:?}"),
        }
    }

    #[test]
    fn response_helpers() {
        let ok = Response::ok("hello");
        assert!(ok.ok);
        assert_eq!(ok.message.as_deref(), Some("hello"));
        assert!(ok.error.is_none());

        let err = Response::err("nope");
        assert!(!err.ok);
        assert_eq!(err.error.as_deref(), Some("nope"));
        assert!(err.message.is_none());
    }
}
