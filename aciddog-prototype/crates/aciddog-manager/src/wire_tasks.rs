//! Deserialize/serialize mirror of `aciddog::tasks::{Task, TaskStatus,
//! Todo, TodoStatus, Comment, Note}`.
//!
//! Same rationale as [`super::ipc`] and [`super::wire_chat`]: the
//! manager crate can't link `aciddog` directly because both crates
//! link `sqlite3` (cozo on one side, sqlx-sqlite on the other). This
//! module duplicates the wire shape; canonical definition is in
//! `crates/aciddog/src/tasks.rs`. Extractions to a shared
//! `aciddog-ipc-types` crate would let us drop the duplication.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Agent-proposed plan awaiting user approval. Carries a
    /// `planned_assignee` that flips into `assignee` on approval.
    Planning,
    Open,
    Assigned,
    /// Plan approved by the user; routed to the previously-stored
    /// `planned_assignee`. Same downstream behaviour as `Assigned`
    /// but distinguishes the provenance.
    Approved,
    InProgress,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub author: String,
    pub timestamp_secs: u64,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Todo {
    pub id: Uuid,
    pub body: String,
    pub status: TodoStatus,
    pub created_by: String,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Comment {
    pub id: Uuid,
    pub author: String,
    pub timestamp_secs: u64,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: TaskStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// Pre-stored handoff target for `planning` tasks. `None` for
    /// any other status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_assignee: Option<String>,
    pub created_by: String,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
    #[serde(default)]
    pub notes: Vec<Note>,
    #[serde(default)]
    pub blocked_by: Vec<Uuid>,
    #[serde(default)]
    pub todos: Vec<Todo>,
    #[serde(default)]
    pub comments: Vec<Comment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_round_trips() {
        let wire = r#"{
            "id":"00000000-0000-0000-0000-000000000001",
            "title":"t",
            "status":"in_progress",
            "created_by":"planner",
            "created_at_secs":1,"updated_at_secs":2,
            "notes":[], "blocked_by":[], "todos":[], "comments":[]
        }"#;
        let t: Task = serde_json::from_str(wire).unwrap();
        assert_eq!(t.title, "t");
        assert_eq!(t.status, TaskStatus::InProgress);
        assert!(t.assignee.is_none());
    }

    #[test]
    fn todo_status_deserialises_snake_case() {
        let wire = r#"{
            "id":"00000000-0000-0000-0000-000000000001",
            "body":"x","status":"in_progress",
            "created_by":"a","created_at_secs":0,"updated_at_secs":0
        }"#;
        let td: Todo = serde_json::from_str(wire).unwrap();
        assert_eq!(td.status, TodoStatus::InProgress);
    }
}
