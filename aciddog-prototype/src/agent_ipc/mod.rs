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
    /// Spawn an ephemeral worker from a declared worker template.
    /// Enforces the caller's `can_create_workers` allowlist. Reply
    /// message carries the new worker's display id and session UUID.
    WorkerCreate {
        template: String,
        task: String,
    },
    /// Deliver a final report to the parent and hibernate the calling
    /// worker. Only valid when the caller is itself a worker.
    WorkerComplete {
        body: String,
    },
    /// Add a new task to the shared team board.
    TaskAdd {
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        /// Ids of existing tasks that must be `Done` before this one
        /// can be claimed via `task take`.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        blocked_by: Vec<Uuid>,
    },
    /// List tasks, optionally filtered.
    TaskList {
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(default)]
        mine: bool,
        #[serde(default)]
        unassigned: bool,
    },
    /// Update fields on an existing task. Fields set to `None` are
    /// left unchanged. `unassign` clears the assignee (mutually
    /// exclusive with `assignee`). `add_blocked_by` / `remove_blocked_by`
    /// edit the dependency list.
    TaskUpdate {
        id: Uuid,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        status: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        assignee: Option<String>,
        #[serde(default)]
        unassign: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        add_blocked_by: Vec<Uuid>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        remove_blocked_by: Vec<Uuid>,
    },
    /// Remove a task by id.
    TaskRemove {
        id: Uuid,
    },
    /// Atomically claim the oldest unassigned task for the caller.
    TaskTake,
    /// Append a standalone note to a task.
    TaskNote {
        id: Uuid,
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
    fn worker_create_serializes_with_template_and_task() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::WorkerCreate {
                template: "researcher".into(),
                task: "find FOO".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"worker_create\""), "got: {s}");
        assert!(s.contains("\"template\":\"researcher\""), "got: {s}");
        assert!(s.contains("\"task\":\"find FOO\""), "got: {s}");
    }

    #[test]
    fn worker_complete_serializes_with_body() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::WorkerComplete {
                body: "all done".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"worker_complete\""), "got: {s}");
        assert!(s.contains("\"body\":\"all done\""), "got: {s}");
    }

    #[test]
    fn worker_create_roundtrips() {
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::WorkerCreate {
                template: "r".into(),
                task: "t".into(),
            },
        };
        let back: Request = serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
        match back.cmd {
            RequestCmd::WorkerCreate { template, task } => {
                assert_eq!(template, "r");
                assert_eq!(task, "t");
            }
            other => panic!("expected WorkerCreate, got {other:?}"),
        }
    }

    #[test]
    fn task_add_roundtrips() {
        let dep = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::new_v4(),
            cmd: RequestCmd::TaskAdd {
                title: "fix bug".into(),
                description: Some("details".into()),
                assignee: Some("tester".into()),
                blocked_by: vec![dep],
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_add\""), "got: {s}");
        assert!(s.contains("\"title\":\"fix bug\""), "got: {s}");
        assert!(s.contains("\"blocked_by\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskAdd {
                title,
                description,
                assignee,
                blocked_by,
            } => {
                assert_eq!(title, "fix bug");
                assert_eq!(description.as_deref(), Some("details"));
                assert_eq!(assignee.as_deref(), Some("tester"));
                assert_eq!(blocked_by, vec![dep]);
            }
            other => panic!("expected TaskAdd, got {other:?}"),
        }
    }

    #[test]
    fn task_add_omits_blocked_by_when_empty() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskAdd {
                title: "t".into(),
                description: None,
                assignee: None,
                blocked_by: vec![],
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(
            !s.contains("blocked_by"),
            "empty list should be omitted: {s}"
        );
    }

    #[test]
    fn task_list_roundtrips() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskList {
                assignee: Some("coder".into()),
                status: Some("in_progress".into()),
                mine: true,
                unassigned: false,
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_list\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back.cmd,
            RequestCmd::TaskList {
                mine: true,
                unassigned: false,
                ..
            }
        ));
    }

    #[test]
    fn task_update_roundtrips() {
        let id = Uuid::new_v4();
        let dep = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskUpdate {
                id,
                title: None,
                description: None,
                status: Some("done".into()),
                assignee: Some("coder".into()),
                unassign: false,
                note: Some("ready".into()),
                add_blocked_by: vec![dep],
                remove_blocked_by: vec![],
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_update\""), "got: {s}");
        assert!(s.contains("\"add_blocked_by\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskUpdate {
                id: parsed,
                status,
                assignee,
                note,
                unassign,
                add_blocked_by,
                remove_blocked_by,
                ..
            } => {
                assert_eq!(parsed, id);
                assert_eq!(status.as_deref(), Some("done"));
                assert_eq!(assignee.as_deref(), Some("coder"));
                assert_eq!(note.as_deref(), Some("ready"));
                assert!(!unassign);
                assert_eq!(add_blocked_by, vec![dep]);
                assert!(remove_blocked_by.is_empty());
            }
            other => panic!("expected TaskUpdate, got {other:?}"),
        }
    }

    #[test]
    fn task_remove_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskRemove { id },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_remove\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(
            back.cmd,
            RequestCmd::TaskRemove { id: parsed } if parsed == id
        ));
    }

    #[test]
    fn task_take_roundtrips() {
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskTake,
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_take\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        assert!(matches!(back.cmd, RequestCmd::TaskTake));
    }

    #[test]
    fn task_note_roundtrips() {
        let id = Uuid::new_v4();
        let req = Request {
            agent_id: Uuid::nil(),
            cmd: RequestCmd::TaskNote {
                id,
                body: "see commit abc123".into(),
            },
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"cmd\":\"task_note\""), "got: {s}");
        let back: Request = serde_json::from_str(&s).unwrap();
        match back.cmd {
            RequestCmd::TaskNote { id: parsed, body } => {
                assert_eq!(parsed, id);
                assert_eq!(body, "see commit abc123");
            }
            other => panic!("expected TaskNote, got {other:?}"),
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
