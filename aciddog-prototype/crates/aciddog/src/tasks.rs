//! Shared task board for the aciddog team.
//!
//! Agents and workers coordinate multi-step work by putting entries on
//! the board: a coder adds a task, reassigns it to a tester on
//! completion, and the tester reassigns it back with a note if there
//! are bugs. Notes travel with the task, so handoff context is
//! preserved across reassignments.
//!
//! The board is team-wide: any agent can add/update/remove/take any
//! task. Task assignment does **not** send a chat message to the
//! assignee — the caller side of `App` writes a passive system block
//! into the assignee's pane so they can see the new work on next
//! glance, but the task board is otherwise pull-driven (via
//! `task list` / `task take`).
//!
//! Persistence: when constructed via [`TaskStore::load_or_init`] with
//! a path, every mutator persists atomically (write-tmp + rename).
//! [`TaskStore::new`] yields an ephemeral store used by unit tests.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::config::USER_INBOX_ID;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    /// Agent-proposed plan awaiting user approval. The task is
    /// assigned to the user and carries a `planned_assignee`; on
    /// approval the planned target becomes the assignee and the
    /// status flips to [`Approved`](Self::Approved).
    Planning,
    Open,
    Assigned,
    /// Plan has been approved by the user and routed to the
    /// previously-stored `planned_assignee`. Acts like `Assigned`
    /// but distinguishes "started life as a plan" from "directly
    /// handed off".
    Approved,
    InProgress,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub fn parse(s: &str) -> Result<Self, TaskError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "planning" | "plan" => Ok(Self::Planning),
            "open" => Ok(Self::Open),
            "assigned" => Ok(Self::Assigned),
            "approved" | "approve" => Ok(Self::Approved),
            "in_progress" | "in-progress" | "inprogress" => Ok(Self::InProgress),
            "done" => Ok(Self::Done),
            "cancelled" | "canceled" => Ok(Self::Cancelled),
            _ => Err(TaskError::InvalidStatus(s.to_string())),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Open => "open",
            Self::Assigned => "assigned",
            Self::Approved => "approved",
            Self::InProgress => "in_progress",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Note {
    pub author: String,
    pub timestamp_secs: u64,
    pub body: String,
}

/// Lifecycle of a single TODO inside a task. `Pending` is the default
/// on add; agents walk through `InProgress` and land on `Completed`.
/// There's no cancel state — remove the TODO if it no longer applies.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
}

impl TodoStatus {
    pub fn parse(s: &str) -> Result<Self, TaskError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "pending" | "todo" | "open" => Ok(Self::Pending),
            "in_progress" | "in-progress" | "inprogress" | "wip" => Ok(Self::InProgress),
            "completed" | "done" | "complete" => Ok(Self::Completed),
            _ => Err(TaskError::InvalidTodoStatus(s.to_string())),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
        }
    }

    /// Renderer-friendly glyph for CLI tables and TUI overlays.
    pub fn glyph(&self) -> char {
        match self {
            Self::Pending => '□',
            Self::InProgress => '◧',
            Self::Completed => '■',
        }
    }
}

/// A single sub-item on a task. Agents and workers use TODOs to
/// decompose a task; supervising agents watch the list to gauge
/// progress. TODOs are a flat checklist: no nesting, no dependencies
/// between them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Todo {
    pub id: Uuid,
    pub body: String,
    pub status: TodoStatus,
    pub created_by: String,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
}

/// An intentional message an agent leaves on a task for the next
/// agent to read. Distinct from `Note` (which is the system's
/// event log — auto-written on take / handoff).
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
    pub description: Option<String>,
    pub status: TaskStatus,
    pub assignee: Option<String>,
    /// Pre-stored handoff target for `Planning` tasks. Set when an
    /// agent proposes a plan to the user; cleared on approval (the
    /// value moves into `assignee`). `serde(default)` so older JSON
    /// files without this field still load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned_assignee: Option<String>,
    pub created_by: String,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
    pub notes: Vec<Note>,
    pub blocked_by: Vec<Uuid>,
    pub todos: Vec<Todo>,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task {0} not found")]
    NotFound(Uuid),

    #[error(
        "invalid status `{0}` \
         (expected planning|open|assigned|approved|in_progress|done|cancelled)"
    )]
    InvalidStatus(String),

    #[error("planning task requires assignee={0:?} and a planned_assignee")]
    PlanningRequiresUserAndTarget(&'static str),

    #[error("task {0} is not in planning status; only planning tasks can be approved")]
    NotInPlanning(Uuid),

    #[error("task {0} is in planning but has no planned_assignee")]
    PlanningWithoutTarget(Uuid),

    #[error("no unassigned tasks available")]
    NoFreeTasks,

    #[error("task title must not be empty")]
    EmptyTitle,

    #[error("task cannot depend on itself")]
    SelfDependency,

    #[error("dependency {0} does not exist")]
    UnknownDependency(Uuid),

    #[error("adding dependency {dep} to task {task} would create a cycle")]
    DependencyCycle { task: Uuid, dep: Uuid },

    #[error("todo {0} not found")]
    TodoNotFound(Uuid),

    #[error("todo body must not be empty")]
    EmptyTodoBody,

    #[error("invalid todo status `{0}` (expected pending|in_progress|completed)")]
    InvalidTodoStatus(String),

    #[error("comment {0} not found")]
    CommentNotFound(Uuid),

    #[error("comment body must not be empty")]
    EmptyCommentBody,

    #[error("task store I/O error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("task store parse error at {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Default, Clone)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<TaskStatus>,
    /// `Some(Some(name))` sets the assignee, `Some(None)` clears it,
    /// `None` leaves the field untouched. This three-state shape lets
    /// `update` distinguish "don't touch" from "clear" in one call.
    pub assignee: Option<Option<String>>,
    /// Same three-state shape as `assignee`, for the
    /// `planned_assignee` (the post-approval handoff target on a
    /// `Planning` task). Agents revise plans by patching this field
    /// in response to user comments.
    pub planned_assignee: Option<Option<String>>,
    pub note: Option<String>,
    /// Dependency ids to add to `blocked_by` on this update.
    pub add_blocked_by: Vec<Uuid>,
    /// Dependency ids to remove from `blocked_by` on this update.
    pub remove_blocked_by: Vec<Uuid>,
}

#[derive(Debug, Default, Clone)]
pub struct TaskFilter {
    pub assignee: Option<String>,
    pub status: Option<TaskStatus>,
    pub unassigned_only: bool,
}

#[derive(Debug, Clone)]
pub struct TaskUpdateOutcome {
    pub task_id: Uuid,
    pub reassigned_to: Option<String>,
}

#[derive(Debug, Default)]
pub struct TaskStore {
    tasks: Vec<Task>,
    path: Option<PathBuf>,
}

impl TaskStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open the store backed by `path`. If the file exists it is
    /// parsed; if it is missing an empty store pointing at `path` is
    /// returned so the first mutator creates the file.
    pub fn load_or_init(path: PathBuf) -> Result<Self, TaskError> {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let tasks: Vec<Task> =
                    serde_json::from_slice(&bytes).map_err(|e| TaskError::Parse {
                        path: path.display().to_string(),
                        source: e,
                    })?;
                Ok(Self {
                    tasks,
                    path: Some(path),
                })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self {
                tasks: Vec::new(),
                path: Some(path),
            }),
            Err(e) => Err(TaskError::Io {
                path: path.display().to_string(),
                source: e,
            }),
        }
    }

    pub fn add(
        &mut self,
        title: String,
        description: Option<String>,
        assignee: Option<String>,
        created_by: String,
        blocked_by: Vec<Uuid>,
    ) -> Result<&Task, TaskError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(TaskError::EmptyTitle);
        }
        // Deduplicate and validate deps before anything else.
        let mut deps: Vec<Uuid> = Vec::new();
        for d in blocked_by {
            if !self.tasks.iter().any(|t| t.id == d) {
                return Err(TaskError::UnknownDependency(d));
            }
            if !deps.contains(&d) {
                deps.push(d);
            }
        }
        let now = now_secs();
        let status = if assignee.is_some() {
            TaskStatus::Assigned
        } else {
            TaskStatus::Open
        };
        let task = Task {
            id: Uuid::new_v4(),
            title,
            description,
            status,
            assignee,
            planned_assignee: None,
            created_by,
            created_at_secs: now,
            updated_at_secs: now,
            notes: Vec::new(),
            blocked_by: deps,
            todos: Vec::new(),
            comments: Vec::new(),
        };
        self.tasks.push(task);
        self.persist()?;
        // Safe: we just pushed.
        Ok(self.tasks.last().unwrap())
    }

    /// Create a `Planning` task assigned to the user inbox with a
    /// pre-stored handoff target. The user reviews the plan and either
    /// approves it (see [`approve`](Self::approve)) or comments to
    /// request a revision. Rejects empty titles and empty
    /// `planned_assignee` (the proposing agent must name a concrete
    /// chat / group up-front).
    pub fn add_planning(
        &mut self,
        title: String,
        description: Option<String>,
        planned_assignee: String,
        created_by: String,
        blocked_by: Vec<Uuid>,
    ) -> Result<&Task, TaskError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(TaskError::EmptyTitle);
        }
        let planned_assignee = planned_assignee.trim().to_string();
        if planned_assignee.is_empty() {
            return Err(TaskError::PlanningRequiresUserAndTarget(USER_INBOX_ID));
        }
        let mut deps: Vec<Uuid> = Vec::new();
        for d in blocked_by {
            if !self.tasks.iter().any(|t| t.id == d) {
                return Err(TaskError::UnknownDependency(d));
            }
            if !deps.contains(&d) {
                deps.push(d);
            }
        }
        let now = now_secs();
        let task = Task {
            id: Uuid::new_v4(),
            title,
            description,
            status: TaskStatus::Planning,
            assignee: Some(USER_INBOX_ID.to_string()),
            planned_assignee: Some(planned_assignee),
            created_by,
            created_at_secs: now,
            updated_at_secs: now,
            notes: Vec::new(),
            blocked_by: deps,
            todos: Vec::new(),
            comments: Vec::new(),
        };
        self.tasks.push(task);
        self.persist()?;
        Ok(self.tasks.last().unwrap())
    }

    /// Atomically reject a `Planning` task: append the rejection
    /// reason as a comment, reassign the task back to its
    /// `created_by` (the proposing agent), append a system note, and
    /// keep the status as `Planning` so the agent knows to revise
    /// and resubmit. Returns the creator name so the caller can
    /// dispatch the standard handoff notification.
    pub fn reject_planning(
        &mut self,
        id: Uuid,
        reason: String,
        rejecter: &str,
    ) -> Result<String, TaskError> {
        let reason = reason.trim().to_string();
        if reason.is_empty() {
            return Err(TaskError::EmptyCommentBody);
        }
        let now = now_secs();
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(TaskError::NotFound(id))?;
        if self.tasks[pos].status != TaskStatus::Planning {
            return Err(TaskError::NotInPlanning(id));
        }
        let creator = self.tasks[pos].created_by.clone();
        {
            let t = &mut self.tasks[pos];
            t.comments.push(Comment {
                id: Uuid::new_v4(),
                author: rejecter.to_string(),
                timestamp_secs: now,
                body: format!("Rejected: {reason}"),
            });
            t.assignee = Some(creator.clone());
            t.notes.push(Note {
                author: rejecter.to_string(),
                timestamp_secs: now,
                body: format!("rejected by {rejecter}; sent back to {creator} for revision"),
            });
            t.updated_at_secs = now;
        }
        self.persist()?;
        Ok(creator)
    }

    /// Atomically approve a `Planning` task: set status to `Approved`,
    /// move `planned_assignee` into `assignee`, append a system note
    /// recording the approver and the new routing. Returns the
    /// pre-approval `planned_assignee` so the caller can dispatch the
    /// handoff notification (the field is `None` after this call).
    pub fn approve(&mut self, id: Uuid, approver: &str) -> Result<String, TaskError> {
        let now = now_secs();
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(TaskError::NotFound(id))?;
        if self.tasks[pos].status != TaskStatus::Planning {
            return Err(TaskError::NotInPlanning(id));
        }
        let target = self.tasks[pos]
            .planned_assignee
            .clone()
            .ok_or(TaskError::PlanningWithoutTarget(id))?;
        {
            let t = &mut self.tasks[pos];
            t.status = TaskStatus::Approved;
            t.assignee = Some(target.clone());
            t.planned_assignee = None;
            t.updated_at_secs = now;
            t.notes.push(Note {
                author: approver.to_string(),
                timestamp_secs: now,
                body: format!("approved by {approver}; routed to {target}"),
            });
        }
        self.persist()?;
        Ok(target)
    }

    pub fn get(&self, id: Uuid) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// True when any task listed in `task.blocked_by` exists and is
    /// not yet `Done`. An unknown dep id counts as satisfied (a
    /// removed task cannot hold up the board); `remove()` prunes the
    /// id from everyone else's lists anyway so this is usually moot.
    pub fn is_blocked(&self, task: &Task) -> bool {
        task.blocked_by.iter().any(|dep| {
            self.tasks
                .iter()
                .any(|t| t.id == *dep && t.status != TaskStatus::Done)
        })
    }

    /// Claim the oldest unassigned `Open` task for `who`. Skips tasks
    /// whose `blocked_by` list contains any dep that isn't yet
    /// `Done`. Atomic at the process level — `App` serialises all
    /// IPC events on a single task.
    pub fn take(&mut self, who: &str) -> Result<&Task, TaskError> {
        let now = now_secs();
        let pos = {
            // Borrow immutably to find the position, then release the
            // borrow before mutating.
            let mut found = None;
            for (i, t) in self.tasks.iter().enumerate() {
                if t.assignee.is_none() && t.status == TaskStatus::Open && !self.is_blocked(t) {
                    found = Some(i);
                    break;
                }
            }
            found.ok_or(TaskError::NoFreeTasks)?
        };
        {
            let t = &mut self.tasks[pos];
            t.assignee = Some(who.to_string());
            t.status = TaskStatus::InProgress;
            t.updated_at_secs = now;
            t.notes.push(Note {
                author: who.to_string(),
                timestamp_secs: now,
                body: format!("taken by {who}"),
            });
        }
        self.persist()?;
        Ok(&self.tasks[pos])
    }

    pub fn update(
        &mut self,
        id: Uuid,
        patch: TaskPatch,
        author: &str,
    ) -> Result<TaskUpdateOutcome, TaskError> {
        if let Some(ref s) = patch.title {
            if s.trim().is_empty() {
                return Err(TaskError::EmptyTitle);
            }
        }
        // Validate every dependency add up-front: the dep must exist,
        // it must not be self, and adding the edge must not introduce
        // a cycle. Done before any mutation so a rejection leaves the
        // store untouched.
        for dep in &patch.add_blocked_by {
            if *dep == id {
                return Err(TaskError::SelfDependency);
            }
            if !self.tasks.iter().any(|t| t.id == *dep) {
                return Err(TaskError::UnknownDependency(*dep));
            }
            if self.reachable_via_blocked_by(*dep, id) {
                return Err(TaskError::DependencyCycle {
                    task: id,
                    dep: *dep,
                });
            }
        }

        let now = now_secs();
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(TaskError::NotFound(id))?;

        let mut reassigned_to: Option<String> = None;

        let t = &mut self.tasks[pos];
        if let Some(title) = patch.title {
            t.title = title.trim().to_string();
        }
        if let Some(desc) = patch.description {
            t.description = Some(desc);
        }
        if let Some(status) = patch.status {
            t.status = status;
        }
        if let Some(new_assignee) = patch.assignee {
            let changed = t.assignee != new_assignee;
            t.assignee = new_assignee.clone();
            if changed {
                if let Some(name) = &new_assignee {
                    reassigned_to = Some(name.clone());
                    // When handing off, don't clobber terminal or
                    // in-progress statuses; only auto-bump from `Open`
                    // to `Assigned`.
                    if t.status == TaskStatus::Open {
                        t.status = TaskStatus::Assigned;
                    }
                }
            }
        }
        if let Some(new_planned) = patch.planned_assignee {
            t.planned_assignee = new_planned;
        }
        for dep in patch.add_blocked_by {
            if !t.blocked_by.contains(&dep) {
                t.blocked_by.push(dep);
            }
        }
        for dep in patch.remove_blocked_by {
            t.blocked_by.retain(|d| *d != dep);
        }

        if let Some(body) = patch.note {
            t.notes.push(Note {
                author: author.to_string(),
                timestamp_secs: now,
                body,
            });
        } else if let Some(new_name) = &reassigned_to {
            // Synthesise a handoff note when none was supplied so the
            // history is never silent about a reassignment.
            t.notes.push(Note {
                author: author.to_string(),
                timestamp_secs: now,
                body: format!("reassigned to {new_name} by {author}"),
            });
        }
        t.updated_at_secs = now;

        self.persist()?;
        Ok(TaskUpdateOutcome {
            task_id: id,
            reassigned_to,
        })
    }

    pub fn remove(&mut self, id: Uuid) -> Result<Task, TaskError> {
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(TaskError::NotFound(id))?;
        let removed = self.tasks.remove(pos);
        // Prune the removed id from every surviving task's
        // `blocked_by` list so we don't strand dependents on a
        // non-existent task.
        for t in &mut self.tasks {
            t.blocked_by.retain(|d| *d != id);
        }
        self.persist()?;
        Ok(removed)
    }

    /// Walk the `blocked_by` graph starting at `from` and return true
    /// if `target` is reachable. Used before adding a new dep edge to
    /// detect cycles: if `target` can already reach `from`, adding
    /// `from → target` closes the loop.
    fn reachable_via_blocked_by(&self, from: Uuid, target: Uuid) -> bool {
        if from == target {
            return true;
        }
        let mut seen: Vec<Uuid> = vec![from];
        let mut stack: Vec<Uuid> = vec![from];
        while let Some(cur) = stack.pop() {
            if let Some(task) = self.tasks.iter().find(|t| t.id == cur) {
                for dep in &task.blocked_by {
                    if *dep == target {
                        return true;
                    }
                    if !seen.contains(dep) {
                        seen.push(*dep);
                        stack.push(*dep);
                    }
                }
            }
        }
        false
    }

    pub fn list(&self, filter: &TaskFilter) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|t| {
                if let Some(ref want) = filter.assignee {
                    if t.assignee.as_deref() != Some(want.as_str()) {
                        return false;
                    }
                }
                if let Some(status) = filter.status {
                    if t.status != status {
                        return false;
                    }
                }
                if filter.unassigned_only && t.assignee.is_some() {
                    return false;
                }
                true
            })
            .collect()
    }

    pub fn add_note(&mut self, id: Uuid, author: String, body: String) -> Result<&Task, TaskError> {
        let now = now_secs();
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == id)
            .ok_or(TaskError::NotFound(id))?;
        {
            let t = &mut self.tasks[pos];
            t.notes.push(Note {
                author,
                timestamp_secs: now,
                body,
            });
            t.updated_at_secs = now;
        }
        self.persist()?;
        Ok(&self.tasks[pos])
    }

    // ----- TODOs -----

    /// Append a `Pending` TODO to `task_id`. Rejects empty bodies.
    pub fn add_todo(
        &mut self,
        task_id: Uuid,
        body: String,
        author: String,
    ) -> Result<&Todo, TaskError> {
        let body = body.trim().to_string();
        if body.is_empty() {
            return Err(TaskError::EmptyTodoBody);
        }
        let now = now_secs();
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(TaskError::NotFound(task_id))?;
        let todo = Todo {
            id: Uuid::new_v4(),
            body,
            status: TodoStatus::Pending,
            created_by: author,
            created_at_secs: now,
            updated_at_secs: now,
        };
        {
            let t = &mut self.tasks[pos];
            t.todos.push(todo);
            t.updated_at_secs = now;
        }
        self.persist()?;
        Ok(self.tasks[pos].todos.last().unwrap())
    }

    pub fn update_todo(
        &mut self,
        task_id: Uuid,
        todo_id: Uuid,
        body: Option<String>,
        status: Option<TodoStatus>,
    ) -> Result<&Todo, TaskError> {
        if let Some(ref b) = body {
            if b.trim().is_empty() {
                return Err(TaskError::EmptyTodoBody);
            }
        }
        let now = now_secs();
        let task_pos = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(TaskError::NotFound(task_id))?;
        let todo_pos = self.tasks[task_pos]
            .todos
            .iter()
            .position(|td| td.id == todo_id)
            .ok_or(TaskError::TodoNotFound(todo_id))?;
        {
            let t = &mut self.tasks[task_pos];
            let td = &mut t.todos[todo_pos];
            if let Some(b) = body {
                td.body = b.trim().to_string();
            }
            if let Some(s) = status {
                td.status = s;
            }
            td.updated_at_secs = now;
            t.updated_at_secs = now;
        }
        self.persist()?;
        Ok(&self.tasks[task_pos].todos[todo_pos])
    }

    pub fn remove_todo(&mut self, task_id: Uuid, todo_id: Uuid) -> Result<Todo, TaskError> {
        let now = now_secs();
        let task_pos = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(TaskError::NotFound(task_id))?;
        let todo_pos = self.tasks[task_pos]
            .todos
            .iter()
            .position(|td| td.id == todo_id)
            .ok_or(TaskError::TodoNotFound(todo_id))?;
        let removed = self.tasks[task_pos].todos.remove(todo_pos);
        self.tasks[task_pos].updated_at_secs = now;
        self.persist()?;
        Ok(removed)
    }

    pub fn todos_of(&self, task_id: Uuid) -> Option<&[Todo]> {
        self.tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| &t.todos[..])
    }

    // ----- Comments -----

    pub fn add_comment(
        &mut self,
        task_id: Uuid,
        body: String,
        author: String,
    ) -> Result<&Comment, TaskError> {
        let body = body.trim().to_string();
        if body.is_empty() {
            return Err(TaskError::EmptyCommentBody);
        }
        let now = now_secs();
        let pos = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(TaskError::NotFound(task_id))?;
        let comment = Comment {
            id: Uuid::new_v4(),
            author,
            timestamp_secs: now,
            body,
        };
        {
            let t = &mut self.tasks[pos];
            t.comments.push(comment);
            t.updated_at_secs = now;
        }
        self.persist()?;
        Ok(self.tasks[pos].comments.last().unwrap())
    }

    pub fn update_comment(
        &mut self,
        task_id: Uuid,
        comment_id: Uuid,
        body: String,
    ) -> Result<&Comment, TaskError> {
        let body = body.trim().to_string();
        if body.is_empty() {
            return Err(TaskError::EmptyCommentBody);
        }
        let now = now_secs();
        let task_pos = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(TaskError::NotFound(task_id))?;
        let comment_pos = self.tasks[task_pos]
            .comments
            .iter()
            .position(|c| c.id == comment_id)
            .ok_or(TaskError::CommentNotFound(comment_id))?;
        {
            let t = &mut self.tasks[task_pos];
            let c = &mut t.comments[comment_pos];
            c.body = body;
            c.timestamp_secs = now;
            t.updated_at_secs = now;
        }
        self.persist()?;
        Ok(&self.tasks[task_pos].comments[comment_pos])
    }

    pub fn remove_comment(
        &mut self,
        task_id: Uuid,
        comment_id: Uuid,
    ) -> Result<Comment, TaskError> {
        let now = now_secs();
        let task_pos = self
            .tasks
            .iter()
            .position(|t| t.id == task_id)
            .ok_or(TaskError::NotFound(task_id))?;
        let comment_pos = self.tasks[task_pos]
            .comments
            .iter()
            .position(|c| c.id == comment_id)
            .ok_or(TaskError::CommentNotFound(comment_id))?;
        let removed = self.tasks[task_pos].comments.remove(comment_pos);
        self.tasks[task_pos].updated_at_secs = now;
        self.persist()?;
        Ok(removed)
    }

    pub fn comments_of(&self, task_id: Uuid) -> Option<&[Comment]> {
        self.tasks
            .iter()
            .find(|t| t.id == task_id)
            .map(|t| &t.comments[..])
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn all(&self) -> &[Task] {
        &self.tasks
    }

    /// Atomic write: serialize to `<path>.tmp` then rename over the
    /// real file. No-op when the store is ephemeral (`path.is_none()`).
    fn persist(&self) -> Result<(), TaskError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| TaskError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
        }
        let bytes = serde_json::to_vec_pretty(&self.tasks).map_err(|e| TaskError::Parse {
            path: path.display().to_string(),
            source: e,
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| TaskError::Io {
            path: tmp.display().to_string(),
            source: e,
        })?;
        std::fs::rename(&tmp, path).map_err(|e| TaskError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Ok(())
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn mk_store() -> TaskStore {
        TaskStore::new()
    }

    #[test]
    fn parse_status_accepts_common_spellings() {
        assert_eq!(TaskStatus::parse("open").unwrap(), TaskStatus::Open);
        assert_eq!(TaskStatus::parse("OPEN").unwrap(), TaskStatus::Open);
        assert_eq!(
            TaskStatus::parse("in_progress").unwrap(),
            TaskStatus::InProgress
        );
        assert_eq!(
            TaskStatus::parse("in-progress").unwrap(),
            TaskStatus::InProgress
        );
        assert_eq!(TaskStatus::parse("done").unwrap(), TaskStatus::Done);
        assert_eq!(
            TaskStatus::parse("cancelled").unwrap(),
            TaskStatus::Cancelled
        );
        assert_eq!(
            TaskStatus::parse("canceled").unwrap(),
            TaskStatus::Cancelled
        );
        assert!(TaskStatus::parse("nope").is_err());
    }

    #[test]
    fn add_assigns_fresh_uuid_and_timestamps() {
        let mut s = mk_store();
        let a = s
            .add("task a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .clone();
        let b = s
            .add("task b".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .clone();
        assert_ne!(a.id, b.id);
        assert_eq!(a.status, TaskStatus::Open);
        assert_eq!(a.created_at_secs, a.updated_at_secs);
        assert_eq!(a.notes.len(), 0);
    }

    #[test]
    fn add_with_assignee_sets_status_assigned() {
        let mut s = mk_store();
        let t = s
            .add(
                "t".into(),
                None,
                Some("bob".into()),
                "alice".into(),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(t.status, TaskStatus::Assigned);
        assert_eq!(t.assignee.as_deref(), Some("bob"));
    }

    #[test]
    fn add_rejects_empty_title() {
        let mut s = mk_store();
        assert!(matches!(
            s.add("   ".into(), None, None, "alice".into(), Vec::new()),
            Err(TaskError::EmptyTitle)
        ));
    }

    #[test]
    fn list_filters_by_assignee_status_unassigned() {
        let mut s = mk_store();
        s.add("t1".into(), None, None, "alice".into(), Vec::new())
            .unwrap();
        s.add(
            "t2".into(),
            None,
            Some("bob".into()),
            "alice".into(),
            Vec::new(),
        )
        .unwrap();
        s.add(
            "t3".into(),
            None,
            Some("bob".into()),
            "alice".into(),
            Vec::new(),
        )
        .unwrap();

        let all = s.list(&TaskFilter::default());
        assert_eq!(all.len(), 3);

        let bob_only = s.list(&TaskFilter {
            assignee: Some("bob".into()),
            ..Default::default()
        });
        assert_eq!(bob_only.len(), 2);

        let unassigned = s.list(&TaskFilter {
            unassigned_only: true,
            ..Default::default()
        });
        assert_eq!(unassigned.len(), 1);
        assert_eq!(unassigned[0].title, "t1");

        let assigned_status = s.list(&TaskFilter {
            status: Some(TaskStatus::Assigned),
            ..Default::default()
        });
        assert_eq!(assigned_status.len(), 2);
    }

    #[test]
    fn update_patches_selected_fields_and_bumps_updated_at() {
        let mut s = mk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        std::thread::sleep(std::time::Duration::from_millis(10));
        let before = s.get(id).unwrap().updated_at_secs;
        s.update(
            id,
            TaskPatch {
                title: Some("renamed".into()),
                description: Some("details".into()),
                status: Some(TaskStatus::InProgress),
                ..Default::default()
            },
            "alice",
        )
        .unwrap();
        let t = s.get(id).unwrap();
        assert_eq!(t.title, "renamed");
        assert_eq!(t.description.as_deref(), Some("details"));
        assert_eq!(t.status, TaskStatus::InProgress);
        assert!(t.updated_at_secs >= before);
    }

    #[test]
    fn update_reassigning_stamps_note_and_returns_outcome() {
        let mut s = mk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let outcome = s
            .update(
                id,
                TaskPatch {
                    assignee: Some(Some("bob".into())),
                    note: Some("please handle".into()),
                    ..Default::default()
                },
                "alice",
            )
            .unwrap();
        assert_eq!(outcome.reassigned_to.as_deref(), Some("bob"));
        let t = s.get(id).unwrap();
        assert_eq!(t.assignee.as_deref(), Some("bob"));
        assert_eq!(t.status, TaskStatus::Assigned);
        assert_eq!(t.notes.len(), 1);
        assert_eq!(t.notes[0].body, "please handle");
        assert_eq!(t.notes[0].author, "alice");
    }

    #[test]
    fn update_synthesises_handoff_note_when_none_supplied() {
        let mut s = mk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        s.update(
            id,
            TaskPatch {
                assignee: Some(Some("bob".into())),
                ..Default::default()
            },
            "alice",
        )
        .unwrap();
        let t = s.get(id).unwrap();
        assert_eq!(t.notes.len(), 1);
        assert!(t.notes[0].body.contains("reassigned to bob by alice"));
    }

    #[test]
    fn update_clears_assignee() {
        let mut s = mk_store();
        let id = s
            .add(
                "t".into(),
                None,
                Some("bob".into()),
                "alice".into(),
                Vec::new(),
            )
            .unwrap()
            .id;
        s.update(
            id,
            TaskPatch {
                assignee: Some(None),
                ..Default::default()
            },
            "alice",
        )
        .unwrap();
        assert!(s.get(id).unwrap().assignee.is_none());
    }

    #[test]
    fn update_unknown_id_errors() {
        let mut s = mk_store();
        let err = s
            .update(Uuid::new_v4(), TaskPatch::default(), "alice")
            .unwrap_err();
        assert!(matches!(err, TaskError::NotFound(_)));
    }

    #[test]
    fn remove_returns_removed_task() {
        let mut s = mk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let removed = s.remove(id).unwrap();
        assert_eq!(removed.id, id);
        assert!(s.get(id).is_none());
        assert!(matches!(s.remove(id), Err(TaskError::NotFound(_))));
    }

    #[test]
    fn take_pops_oldest_open_task_and_marks_it_in_progress() {
        let mut s = mk_store();
        let first = s
            .add("first".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let _second = s
            .add("second".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let taken = s.take("worker").unwrap().clone();
        assert_eq!(taken.id, first);
        assert_eq!(taken.status, TaskStatus::InProgress);
        assert_eq!(taken.assignee.as_deref(), Some("worker"));
        assert_eq!(taken.notes.len(), 1);
        assert!(taken.notes[0].body.contains("taken by worker"));
    }

    #[test]
    fn take_errors_when_none_free() {
        let mut s = mk_store();
        assert!(matches!(s.take("w"), Err(TaskError::NoFreeTasks)));
        // Assigned tasks are not "free".
        s.add(
            "t".into(),
            None,
            Some("x".into()),
            "alice".into(),
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(s.take("w"), Err(TaskError::NoFreeTasks)));
    }

    #[test]
    fn take_skips_already_assigned_tasks() {
        let mut s = mk_store();
        let _t1 = s
            .add(
                "t1".into(),
                None,
                Some("x".into()),
                "alice".into(),
                Vec::new(),
            )
            .unwrap()
            .id;
        let t2 = s
            .add("t2".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let taken = s.take("w").unwrap();
        assert_eq!(taken.id, t2);
    }

    #[test]
    fn add_note_appends_in_order() {
        let mut s = mk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        s.add_note(id, "alice".into(), "one".into()).unwrap();
        s.add_note(id, "bob".into(), "two".into()).unwrap();
        let t = s.get(id).unwrap();
        assert_eq!(t.notes.len(), 2);
        assert_eq!(t.notes[0].body, "one");
        assert_eq!(t.notes[1].body, "two");
        assert!(t.notes[1].timestamp_secs >= t.notes[0].timestamp_secs);
    }

    #[test]
    fn load_or_init_handles_missing_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        let s = TaskStore::load_or_init(path.clone()).unwrap();
        assert!(s.is_empty());
        assert!(!path.exists()); // empty store doesn't auto-create
    }

    #[test]
    fn persist_survives_reopen() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        let id;
        {
            let mut s = TaskStore::load_or_init(path.clone()).unwrap();
            id = s
                .add("keep me".into(), None, None, "alice".into(), Vec::new())
                .unwrap()
                .id;
            s.update(
                id,
                TaskPatch {
                    note: Some("progress".into()),
                    ..Default::default()
                },
                "alice",
            )
            .unwrap();
        }
        assert!(path.exists());
        let s2 = TaskStore::load_or_init(path).unwrap();
        let t = s2.get(id).expect("task persisted");
        assert_eq!(t.title, "keep me");
        assert_eq!(t.notes.len(), 1);
        assert_eq!(t.notes[0].body, "progress");
    }

    #[test]
    fn load_or_init_errors_on_malformed_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        std::fs::write(&path, b"not json at all").unwrap();
        assert!(matches!(
            TaskStore::load_or_init(path),
            Err(TaskError::Parse { .. })
        ));
    }

    // --------- Dependencies ----------

    #[test]
    fn add_with_unknown_dep_errors() {
        let mut s = mk_store();
        let err = s
            .add("t".into(), None, None, "alice".into(), vec![Uuid::new_v4()])
            .unwrap_err();
        assert!(matches!(err, TaskError::UnknownDependency(_)));
    }

    #[test]
    fn add_dedupes_blocked_by() {
        let mut s = mk_store();
        let a = s
            .add("a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let b = s
            .add("b".into(), None, None, "alice".into(), vec![a, a])
            .unwrap();
        assert_eq!(b.blocked_by, vec![a]);
    }

    #[test]
    fn take_skips_blocked_tasks() {
        let mut s = mk_store();
        let dep = s
            .add("prereq".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let dependent = s
            .add("follow-up".into(), None, None, "alice".into(), vec![dep])
            .unwrap()
            .id;

        // `take` must pick the prereq, not the blocked dependent.
        let taken = s.take("worker").unwrap().id;
        assert_eq!(taken, dep);

        // Dependent is still blocked because prereq is InProgress, not Done.
        assert!(matches!(s.take("worker2"), Err(TaskError::NoFreeTasks)));

        // Resolve the prereq.
        s.update(
            dep,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
            "worker",
        )
        .unwrap();

        // Now the dependent is claimable.
        let taken2 = s.take("worker2").unwrap().id;
        assert_eq!(taken2, dependent);
    }

    #[test]
    fn update_adds_and_removes_blocked_by() {
        let mut s = mk_store();
        let a = s
            .add("a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let b = s
            .add("b".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        s.update(
            b,
            TaskPatch {
                add_blocked_by: vec![a],
                ..Default::default()
            },
            "alice",
        )
        .unwrap();
        assert_eq!(s.get(b).unwrap().blocked_by, vec![a]);
        s.update(
            b,
            TaskPatch {
                remove_blocked_by: vec![a],
                ..Default::default()
            },
            "alice",
        )
        .unwrap();
        assert!(s.get(b).unwrap().blocked_by.is_empty());
    }

    #[test]
    fn update_rejects_self_dependency() {
        let mut s = mk_store();
        let a = s
            .add("a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let err = s
            .update(
                a,
                TaskPatch {
                    add_blocked_by: vec![a],
                    ..Default::default()
                },
                "alice",
            )
            .unwrap_err();
        assert!(matches!(err, TaskError::SelfDependency));
    }

    #[test]
    fn update_rejects_cycle() {
        let mut s = mk_store();
        let a = s
            .add("a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let b = s
            .add("b".into(), None, None, "alice".into(), vec![a])
            .unwrap()
            .id;
        // b depends on a; adding a depends-on-b would close the loop.
        let err = s
            .update(
                a,
                TaskPatch {
                    add_blocked_by: vec![b],
                    ..Default::default()
                },
                "alice",
            )
            .unwrap_err();
        assert!(matches!(err, TaskError::DependencyCycle { .. }));
    }

    #[test]
    fn update_rejects_unknown_dep() {
        let mut s = mk_store();
        let a = s
            .add("a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let err = s
            .update(
                a,
                TaskPatch {
                    add_blocked_by: vec![Uuid::new_v4()],
                    ..Default::default()
                },
                "alice",
            )
            .unwrap_err();
        assert!(matches!(err, TaskError::UnknownDependency(_)));
    }

    #[test]
    fn remove_prunes_blocked_by_in_dependents() {
        let mut s = mk_store();
        let a = s
            .add("a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let b = s
            .add("b".into(), None, None, "alice".into(), vec![a])
            .unwrap()
            .id;
        s.remove(a).unwrap();
        assert!(s.get(b).unwrap().blocked_by.is_empty());
    }

    #[test]
    fn is_blocked_matches_open_or_in_progress_dep() {
        let mut s = mk_store();
        let a = s
            .add("a".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let b_id = s
            .add("b".into(), None, None, "alice".into(), vec![a])
            .unwrap()
            .id;
        let b = s.get(b_id).unwrap().clone();
        assert!(s.is_blocked(&b));
        s.update(
            a,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
            "alice",
        )
        .unwrap();
        let b = s.get(b_id).unwrap().clone();
        assert!(!s.is_blocked(&b));
    }

    #[test]
    fn rejects_pre_release_json_missing_new_fields() {
        // Pre-release schema (no `todos` / `comments` / `blocked_by`
        // keys) must fail to load rather than silently defaulting —
        // there's no migration path at this stage.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        std::fs::write(
            &path,
            br#"[{
                "id": "00000000-0000-0000-0000-000000000001",
                "title": "legacy",
                "description": null,
                "status": "open",
                "assignee": null,
                "created_by": "alice",
                "created_at_secs": 1,
                "updated_at_secs": 1,
                "notes": []
            }]"#,
        )
        .unwrap();
        assert!(matches!(
            TaskStore::load_or_init(path),
            Err(TaskError::Parse { .. })
        ));
    }

    // ----- TODOs + Comments -----

    fn mk_disk_store() -> (TaskStore, TempDir) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        (TaskStore::load_or_init(path).unwrap(), tmp)
    }

    #[test]
    fn add_todo_appends_pending_and_bumps_task_timestamp() {
        let (mut s, _tmp) = mk_disk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let before = s.get(id).unwrap().updated_at_secs;
        std::thread::sleep(std::time::Duration::from_millis(5));
        let todo = s
            .add_todo(id, "draft schema".into(), "coder".into())
            .unwrap()
            .clone();
        assert_eq!(todo.status, TodoStatus::Pending);
        assert_eq!(todo.body, "draft schema");
        assert_eq!(todo.created_by, "coder");
        let t = s.get(id).unwrap();
        assert_eq!(t.todos.len(), 1);
        assert!(t.updated_at_secs >= before);
    }

    #[test]
    fn add_todo_rejects_empty_body() {
        let (mut s, _tmp) = mk_disk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        assert!(matches!(
            s.add_todo(id, "   ".into(), "coder".into()),
            Err(TaskError::EmptyTodoBody)
        ));
    }

    #[test]
    fn update_todo_changes_status_and_body() {
        let (mut s, _tmp) = mk_disk_store();
        let tid = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let todo_id = s.add_todo(tid, "one".into(), "coder".into()).unwrap().id;
        s.update_todo(tid, todo_id, None, Some(TodoStatus::InProgress))
            .unwrap();
        assert_eq!(s.todos_of(tid).unwrap()[0].status, TodoStatus::InProgress);
        s.update_todo(
            tid,
            todo_id,
            Some("revised".into()),
            Some(TodoStatus::Completed),
        )
        .unwrap();
        assert_eq!(s.todos_of(tid).unwrap()[0].body, "revised");
        assert_eq!(s.todos_of(tid).unwrap()[0].status, TodoStatus::Completed);
    }

    #[test]
    fn remove_todo_returns_and_drops() {
        let (mut s, _tmp) = mk_disk_store();
        let tid = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let todo_id = s.add_todo(tid, "one".into(), "coder".into()).unwrap().id;
        let removed = s.remove_todo(tid, todo_id).unwrap();
        assert_eq!(removed.id, todo_id);
        assert!(s.todos_of(tid).unwrap().is_empty());
        assert!(matches!(
            s.remove_todo(tid, todo_id),
            Err(TaskError::TodoNotFound(_))
        ));
    }

    #[test]
    fn todo_status_parse_accepts_common_spellings() {
        assert_eq!(TodoStatus::parse("pending").unwrap(), TodoStatus::Pending);
        assert_eq!(
            TodoStatus::parse("in_progress").unwrap(),
            TodoStatus::InProgress
        );
        assert_eq!(
            TodoStatus::parse("in-progress").unwrap(),
            TodoStatus::InProgress
        );
        assert_eq!(
            TodoStatus::parse("completed").unwrap(),
            TodoStatus::Completed
        );
        assert_eq!(TodoStatus::parse("done").unwrap(), TodoStatus::Completed);
        assert!(TodoStatus::parse("wat").is_err());
    }

    #[test]
    fn add_comment_appends_with_author_and_timestamp() {
        let (mut s, _tmp) = mk_disk_store();
        let tid = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let c = s
            .add_comment(tid, "heads up".into(), "coder".into())
            .unwrap()
            .clone();
        assert_eq!(c.author, "coder");
        assert_eq!(c.body, "heads up");
        assert!(c.timestamp_secs > 0);
        assert_eq!(s.comments_of(tid).unwrap().len(), 1);
    }

    #[test]
    fn update_comment_rejects_empty_body() {
        let (mut s, _tmp) = mk_disk_store();
        let tid = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let cid = s.add_comment(tid, "a".into(), "coder".into()).unwrap().id;
        assert!(matches!(
            s.update_comment(tid, cid, "   ".into()),
            Err(TaskError::EmptyCommentBody)
        ));
    }

    #[test]
    fn remove_comment_returns_and_drops() {
        let (mut s, _tmp) = mk_disk_store();
        let tid = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        let cid = s.add_comment(tid, "a".into(), "coder".into()).unwrap().id;
        let removed = s.remove_comment(tid, cid).unwrap();
        assert_eq!(removed.id, cid);
        assert!(s.comments_of(tid).unwrap().is_empty());
    }

    #[test]
    fn todos_comments_persist_across_reload() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        let tid;
        {
            let mut s = TaskStore::load_or_init(path.clone()).unwrap();
            tid = s
                .add("t".into(), None, None, "alice".into(), Vec::new())
                .unwrap()
                .id;
            s.add_todo(tid, "one".into(), "coder".into()).unwrap();
            s.add_comment(tid, "note to self".into(), "coder".into())
                .unwrap();
        }
        let s2 = TaskStore::load_or_init(path).unwrap();
        let t = s2.get(tid).unwrap();
        assert_eq!(t.todos.len(), 1);
        assert_eq!(t.comments.len(), 1);
    }

    // ----- Planning + Approval -----

    #[test]
    fn add_planning_creates_user_assigned_task_with_target() {
        let mut s = mk_store();
        let t = s
            .add_planning(
                "draft auth flow".into(),
                Some("OAuth handoff plan".into()),
                "builders".into(),
                "planner".into(),
                Vec::new(),
            )
            .unwrap()
            .clone();
        assert_eq!(t.status, TaskStatus::Planning);
        assert_eq!(t.assignee.as_deref(), Some(USER_INBOX_ID));
        assert_eq!(t.planned_assignee.as_deref(), Some("builders"));
        assert_eq!(t.created_by, "planner");
    }

    #[test]
    fn add_planning_rejects_empty_planned_assignee() {
        let mut s = mk_store();
        assert!(matches!(
            s.add_planning(
                "t".into(),
                None,
                "   ".into(),
                "planner".into(),
                Vec::new()
            ),
            Err(TaskError::PlanningRequiresUserAndTarget(_))
        ));
    }

    #[test]
    fn approve_routes_to_planned_assignee_and_sets_approved() {
        let mut s = mk_store();
        let id = s
            .add_planning(
                "t".into(),
                None,
                "builders".into(),
                "planner".into(),
                Vec::new(),
            )
            .unwrap()
            .id;
        let target = s.approve(id, USER_INBOX_ID).unwrap();
        assert_eq!(target, "builders");
        let t = s.get(id).unwrap();
        assert_eq!(t.status, TaskStatus::Approved);
        assert_eq!(t.assignee.as_deref(), Some("builders"));
        assert!(t.planned_assignee.is_none());
        assert_eq!(t.notes.len(), 1);
        assert!(t.notes[0].body.contains("approved by"));
        assert!(t.notes[0].body.contains("routed to builders"));
    }

    #[test]
    fn approve_rejects_non_planning_task() {
        let mut s = mk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        assert!(matches!(
            s.approve(id, USER_INBOX_ID),
            Err(TaskError::NotInPlanning(_))
        ));
    }

    #[test]
    fn reject_planning_bounces_to_creator_and_keeps_planning() {
        let mut s = mk_store();
        let id = s
            .add_planning(
                "t".into(),
                None,
                "builders".into(),
                "planner".into(),
                Vec::new(),
            )
            .unwrap()
            .id;
        let creator = s
            .reject_planning(id, "needs more detail".into(), USER_INBOX_ID)
            .unwrap();
        assert_eq!(creator, "planner");
        let t = s.get(id).unwrap();
        assert_eq!(t.status, TaskStatus::Planning);
        assert_eq!(t.assignee.as_deref(), Some("planner"));
        assert_eq!(t.comments.len(), 1);
        assert!(t.comments[0].body.starts_with("Rejected: "));
        assert!(t.comments[0].body.contains("needs more detail"));
        assert_eq!(t.notes.len(), 1);
        assert!(t.notes[0].body.contains("rejected by"));
        assert!(t.notes[0].body.contains("sent back to planner"));
        // planned_assignee is preserved so the agent can revise and
        // resubmit without losing the original target.
        assert_eq!(t.planned_assignee.as_deref(), Some("builders"));
    }

    #[test]
    fn reject_planning_rejects_non_planning_task() {
        let mut s = mk_store();
        let id = s
            .add("t".into(), None, None, "alice".into(), Vec::new())
            .unwrap()
            .id;
        assert!(matches!(
            s.reject_planning(id, "no".into(), USER_INBOX_ID),
            Err(TaskError::NotInPlanning(_))
        ));
    }

    #[test]
    fn reject_planning_rejects_empty_reason() {
        let mut s = mk_store();
        let id = s
            .add_planning(
                "t".into(),
                None,
                "builders".into(),
                "planner".into(),
                Vec::new(),
            )
            .unwrap()
            .id;
        assert!(matches!(
            s.reject_planning(id, "   ".into(), USER_INBOX_ID),
            Err(TaskError::EmptyCommentBody)
        ));
    }

    #[test]
    fn parse_status_accepts_planning_and_approved() {
        assert_eq!(TaskStatus::parse("planning").unwrap(), TaskStatus::Planning);
        assert_eq!(TaskStatus::parse("plan").unwrap(), TaskStatus::Planning);
        assert_eq!(TaskStatus::parse("approved").unwrap(), TaskStatus::Approved);
    }

    #[test]
    fn task_loads_legacy_json_without_planned_assignee() {
        // `planned_assignee` is `serde(default)` so older JSON that
        // pre-dates the planning workflow still loads. The other
        // post-pre-release fields (todos / comments / blocked_by) are
        // still required.
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        std::fs::write(
            &path,
            br#"[{
                "id": "00000000-0000-0000-0000-000000000001",
                "title": "legacy",
                "description": null,
                "status": "open",
                "assignee": null,
                "created_by": "alice",
                "created_at_secs": 1,
                "updated_at_secs": 1,
                "notes": [],
                "blocked_by": [],
                "todos": [],
                "comments": []
            }]"#,
        )
        .unwrap();
        let s = TaskStore::load_or_init(path).unwrap();
        let t = &s.all()[0];
        assert!(t.planned_assignee.is_none());
        assert_eq!(t.status, TaskStatus::Open);
    }

    #[test]
    fn planned_assignee_round_trips_across_reload() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");
        let tid;
        {
            let mut s = TaskStore::load_or_init(path.clone()).unwrap();
            tid = s
                .add_planning(
                    "t".into(),
                    None,
                    "builders".into(),
                    "planner".into(),
                    Vec::new(),
                )
                .unwrap()
                .id;
        }
        let s2 = TaskStore::load_or_init(path).unwrap();
        let t = s2.get(tid).unwrap();
        assert_eq!(t.status, TaskStatus::Planning);
        assert_eq!(t.planned_assignee.as_deref(), Some("builders"));
        assert_eq!(t.assignee.as_deref(), Some(USER_INBOX_ID));
    }
}
