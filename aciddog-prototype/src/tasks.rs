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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Assigned,
    InProgress,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub fn parse(s: &str) -> Result<Self, TaskError> {
        match s.trim().to_ascii_lowercase().as_str() {
            "open" => Ok(Self::Open),
            "assigned" => Ok(Self::Assigned),
            "in_progress" | "in-progress" | "inprogress" => Ok(Self::InProgress),
            "done" => Ok(Self::Done),
            "cancelled" | "canceled" => Ok(Self::Cancelled),
            _ => Err(TaskError::InvalidStatus(s.to_string())),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Assigned => "assigned",
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub status: TaskStatus,
    pub assignee: Option<String>,
    pub created_by: String,
    pub created_at_secs: u64,
    pub updated_at_secs: u64,
    pub notes: Vec<Note>,
    /// Ids of tasks that must reach `Done` before this task is
    /// eligible for `take`. `#[serde(default)]` keeps older
    /// `tasks.json` files (written before dependencies existed)
    /// loadable without migration.
    #[serde(default)]
    pub blocked_by: Vec<Uuid>,
}

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("task {0} not found")]
    NotFound(Uuid),

    #[error("invalid status `{0}` (expected open|assigned|in_progress|done|cancelled)")]
    InvalidStatus(String),

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
            created_by,
            created_at_secs: now,
            updated_at_secs: now,
            notes: Vec::new(),
            blocked_by: deps,
        };
        self.tasks.push(task);
        self.persist()?;
        // Safe: we just pushed.
        Ok(self.tasks.last().unwrap())
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
    fn loads_old_tasks_json_without_blocked_by_field() {
        // Older format written before dependencies existed — no
        // `blocked_by` key. `#[serde(default)]` should default it.
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
        let s = TaskStore::load_or_init(path).unwrap();
        assert_eq!(s.all().len(), 1);
        assert!(s.all()[0].blocked_by.is_empty());
    }
}
