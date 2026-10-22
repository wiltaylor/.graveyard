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
    Review,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_submitter: Option<String>,
    /// Filesystem worktree path bound to this task by the aciddog
    /// side. Used to set the cwd of workers spawned against this task.
    /// Read-only on the wire; aciddog task-board CLI is the canonical
    /// editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
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
            "worktree":"/tmp/wt",
            "created_by":"planner",
            "created_at_secs":1,"updated_at_secs":2,
            "notes":[], "blocked_by":[], "todos":[], "comments":[]
        }"#;
        let t: Task = serde_json::from_str(wire).unwrap();
        assert_eq!(t.title, "t");
        assert_eq!(t.status, TaskStatus::InProgress);
        assert!(t.assignee.is_none());
        assert_eq!(t.worktree, Some("/tmp/wt".to_string()));
        let out = serde_json::to_value(&t).unwrap();
        assert_eq!(out["worktree"], "/tmp/wt");
    }

    #[test]
    fn task_round_trips_omits_worktree_when_absent() {
        let wire = r#"{
            "id":"00000000-0000-0000-0000-000000000001",
            "title":"t",
            "status":"in_progress",
            "created_by":"planner",
            "created_at_secs":1,"updated_at_secs":2,
            "notes":[], "blocked_by":[], "todos":[], "comments":[]
        }"#;
        let t: Task = serde_json::from_str(wire).unwrap();
        assert_eq!(t.worktree, None);
        let out = serde_json::to_string(&t).unwrap();
        assert!(!out.contains("\"worktree\""));
    }

    #[test]
    fn task_round_trips_review_status_and_submitter() {
        let absent_submitter = r#"{
            "id":"00000000-0000-0000-0000-000000000001",
            "title":"t",
            "status":"review",
            "created_by":"planner",
            "created_at_secs":1,"updated_at_secs":2,
            "notes":[], "blocked_by":[], "todos":[], "comments":[]
        }"#;
        let t: Task = serde_json::from_str(absent_submitter).unwrap();
        assert_eq!(t.status, TaskStatus::Review);
        assert_eq!(t.review_submitter, None);
        let out = serde_json::to_string(&t).unwrap();
        assert!(!out.contains("\"review_submitter\""));

        let present_submitter = r#"{
            "id":"00000000-0000-0000-0000-000000000001",
            "title":"t",
            "status":"review",
            "review_submitter":"builder",
            "created_by":"planner",
            "created_at_secs":1,"updated_at_secs":2,
            "notes":[], "blocked_by":[], "todos":[], "comments":[]
        }"#;
        let t: Task = serde_json::from_str(present_submitter).unwrap();
        assert_eq!(t.status, TaskStatus::Review);
        assert_eq!(t.review_submitter, Some("builder".to_string()));
        let out = serde_json::to_value(&t).unwrap();
        let expected: serde_json::Value = serde_json::from_str(present_submitter).unwrap();
        assert_eq!(out, expected);
    }

    #[test]
    fn task_round_trips_with_worktree() {
        let wire = r#"{
            "id":"00000000-0000-0000-0000-000000000001",
            "title":"t",
            "status":"in_progress",
            "worktree":"/tmp/wt",
            "created_by":"planner",
            "created_at_secs":1,"updated_at_secs":2,
            "notes":[], "blocked_by":[], "todos":[], "comments":[]
        }"#;
        let t: Task = serde_json::from_str(wire).unwrap();
        assert_eq!(t.worktree, Some("/tmp/wt".to_string()));
        let out = serde_json::to_value(&t).unwrap();
        let expected: serde_json::Value = serde_json::from_str(wire).unwrap();
        assert_eq!(out, expected);
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
