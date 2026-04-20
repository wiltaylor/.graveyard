//! State and pure logic for the in-TUI Tasks screen.
//!
//! The screen presents tasks grouped by status (Open → Assigned →
//! In-Progress → Done → Cancelled), a modal add/edit form, a detail
//! modal, and one-line prompts (note body, delete-confirm) that
//! piggy-back on the existing bottom input bar. `TasksScreen` owns
//! all UI-local state; the `TaskStore` stays the single source of
//! truth for persisted task data.

use uuid::Uuid;

use crate::tasks::{Task, TaskError, TaskPatch, TaskStatus, TaskStore};

/// Author string used for TUI-originated mutations. Distinct from
/// agent display names so transcripts can tell which writes came from
/// the human operator.
pub const OPERATOR: &str = "tui";

/// Order of status groups in the rendered list. Driving everything
/// off this single array keeps rendering, tests, and status cycling
/// in sync.
pub const STATUS_ORDER: [TaskStatus; 5] = [
    TaskStatus::Open,
    TaskStatus::Assigned,
    TaskStatus::InProgress,
    TaskStatus::Done,
    TaskStatus::Cancelled,
];

/// A single rendered row. Group headers and tasks share one flat
/// vector so the selection cursor moves through the same space the
/// eye scans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRow {
    Header(TaskStatus),
    Task(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFormKind {
    Add,
    Edit(Uuid),
}

/// Fields in the modal form, laid out in tab order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Title,
    Description,
    Assignee,
    Status,
    BlockedByCsv,
    RemoveBlockedByCsv,
}

impl FormField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Description => "Description",
            Self::Assignee => "Assignee",
            Self::Status => "Status",
            Self::BlockedByCsv => "Add blocked-by (CSV uuids)",
            Self::RemoveBlockedByCsv => "Remove blocked-by (CSV uuids)",
        }
    }
}

/// A popup overlaying the form that lets the user pick from a list
/// of options. Open via Enter/click on a dropdown field; navigate
/// with ↑/↓/j/k; confirm with Enter; dismiss with Esc.
#[derive(Debug, Clone)]
pub struct DropdownPicker {
    /// Which form field this picker is editing.
    pub field: FormField,
    /// Options presented to the user, in display order.
    pub options: Vec<String>,
    /// Index of the currently highlighted option.
    pub selected: usize,
}

#[derive(Debug, Clone)]
pub struct TaskForm {
    pub kind: TaskFormKind,
    pub title: String,
    pub description: String,
    pub assignee: String,
    pub status: String,
    pub blocked_by_csv: String,
    pub remove_blocked_by_csv: String,
    pub focused_field: FormField,
    pub error: Option<String>,
    /// Options offered by the Assignee dropdown. The empty string
    /// (rendered as "(unassigned)") must be present; callers inject
    /// the rest based on the team's chat display names at open
    /// time via [`set_assignee_options`].
    pub assignee_options: Vec<String>,
    /// When `Some`, an option-picker popup is overlaid on the form
    /// for the focused dropdown field. All form keys except picker
    /// navigation are suspended while it's open.
    pub picker: Option<DropdownPicker>,
}

impl TaskForm {
    pub fn new_add() -> Self {
        Self {
            kind: TaskFormKind::Add,
            title: String::new(),
            description: String::new(),
            assignee: String::new(),
            status: String::new(),
            blocked_by_csv: String::new(),
            remove_blocked_by_csv: String::new(),
            focused_field: FormField::Title,
            error: None,
            assignee_options: vec![String::new()],
            picker: None,
        }
    }

    pub fn new_edit(task: &Task) -> Self {
        Self {
            kind: TaskFormKind::Edit(task.id),
            title: task.title.clone(),
            description: task.description.clone().unwrap_or_default(),
            assignee: task.assignee.clone().unwrap_or_default(),
            status: task.status.as_str().to_string(),
            blocked_by_csv: String::new(),
            remove_blocked_by_csv: String::new(),
            focused_field: FormField::Title,
            error: None,
            assignee_options: vec![String::new()],
            picker: None,
        }
    }

    /// Replace the assignee dropdown options. The caller must ensure
    /// an entry representing "unassigned" (empty string) is present.
    /// If the current `assignee` value isn't in `options`, it's
    /// appended so the user doesn't silently lose the selection
    /// (e.g. a stale display name on an edit form).
    pub fn set_assignee_options(&mut self, mut options: Vec<String>) {
        if !options.iter().any(|o| o.is_empty()) {
            options.insert(0, String::new());
        }
        if !options.iter().any(|o| o == &self.assignee) {
            options.push(self.assignee.clone());
        }
        self.assignee_options = options;
    }

    pub fn is_dropdown(field: FormField) -> bool {
        matches!(field, FormField::Assignee | FormField::Status)
    }

    /// Open a picker popup for the currently focused dropdown
    /// field. No-op if the field isn't a dropdown. The initial
    /// highlighted row is whichever option matches the current
    /// value (falling back to 0 if there's no match).
    pub fn open_picker(&mut self) {
        let field = self.focused_field;
        if !Self::is_dropdown(field) {
            return;
        }
        let (options, current) = match field {
            FormField::Assignee => (self.assignee_options.clone(), self.assignee.as_str()),
            FormField::Status => (
                STATUS_ORDER
                    .iter()
                    .map(|s| s.as_str().to_string())
                    .collect(),
                self.status.as_str(),
            ),
            _ => return,
        };
        let selected = options.iter().position(|o| o == current).unwrap_or(0);
        self.picker = Some(DropdownPicker {
            field,
            options,
            selected,
        });
    }

    pub fn close_picker(&mut self) {
        self.picker = None;
    }

    pub fn picker_move(&mut self, delta: i32) {
        if let Some(p) = self.picker.as_mut() {
            if p.options.is_empty() {
                return;
            }
            let len = p.options.len() as i32;
            let next = (p.selected as i32 + delta).rem_euclid(len);
            p.selected = next as usize;
        }
    }

    /// Apply the picker's selection to the underlying form field
    /// and close the popup. No-op if the picker is closed.
    pub fn commit_picker(&mut self) {
        let Some(p) = self.picker.take() else { return };
        let Some(value) = p.options.get(p.selected).cloned() else {
            return;
        };
        match p.field {
            FormField::Assignee => self.assignee = value,
            FormField::Status => self.status = value,
            _ => {}
        }
    }

    /// All fields in tab order. For Add, the Status and
    /// Remove-blocked-by fields are skipped since they don't apply.
    pub fn tab_order(&self) -> &'static [FormField] {
        match self.kind {
            TaskFormKind::Add => &[
                FormField::Title,
                FormField::Description,
                FormField::Assignee,
                FormField::BlockedByCsv,
            ],
            TaskFormKind::Edit(_) => &[
                FormField::Title,
                FormField::Description,
                FormField::Assignee,
                FormField::Status,
                FormField::BlockedByCsv,
                FormField::RemoveBlockedByCsv,
            ],
        }
    }

    pub fn focus_next(&mut self) {
        let order = self.tab_order();
        if let Some(pos) = order.iter().position(|f| *f == self.focused_field) {
            self.focused_field = order[(pos + 1) % order.len()];
        }
    }

    pub fn focus_prev(&mut self) {
        let order = self.tab_order();
        if let Some(pos) = order.iter().position(|f| *f == self.focused_field) {
            self.focused_field = order[(pos + order.len() - 1) % order.len()];
        }
    }

    pub fn field_mut(&mut self, field: FormField) -> &mut String {
        match field {
            FormField::Title => &mut self.title,
            FormField::Description => &mut self.description,
            FormField::Assignee => &mut self.assignee,
            FormField::Status => &mut self.status,
            FormField::BlockedByCsv => &mut self.blocked_by_csv,
            FormField::RemoveBlockedByCsv => &mut self.remove_blocked_by_csv,
        }
    }

    pub fn field(&self, field: FormField) -> &str {
        match field {
            FormField::Title => &self.title,
            FormField::Description => &self.description,
            FormField::Assignee => &self.assignee,
            FormField::Status => &self.status,
            FormField::BlockedByCsv => &self.blocked_by_csv,
            FormField::RemoveBlockedByCsv => &self.remove_blocked_by_csv,
        }
    }
}

/// One-shot prompts that piggy-back on the chat input bar.
#[derive(Debug, Clone)]
pub enum TasksPrompt {
    /// Collecting a note body for `task_id`.
    Note { task_id: Uuid, body: String },
    /// Awaiting y/n on delete confirmation.
    ConfirmDelete { task_id: Uuid, buffer: String },
}

#[derive(Debug, Default)]
pub struct TasksScreen {
    pub selected_row: usize,
    pub form: Option<TaskForm>,
    pub detail: Option<Uuid>,
    pub prompt: Option<TasksPrompt>,
    /// Transient error/success message shown at the bottom of the
    /// task area. Cleared on next key press.
    pub status_msg: Option<String>,
}

impl TasksScreen {
    pub fn rows(store: &TaskStore) -> Vec<TaskRow> {
        let mut out: Vec<TaskRow> = Vec::new();
        for status in STATUS_ORDER {
            out.push(TaskRow::Header(status));
            for t in store.all() {
                if t.status == status {
                    out.push(TaskRow::Task(t.id));
                }
            }
        }
        out
    }

    pub fn selected_task_id(&self, rows: &[TaskRow]) -> Option<Uuid> {
        match rows.get(self.selected_row) {
            Some(TaskRow::Task(id)) => Some(*id),
            _ => None,
        }
    }

    /// Move selection up, skipping group-header rows. Stops at the
    /// first selectable row if none are found above.
    pub fn move_up(&mut self, rows: &[TaskRow]) {
        if rows.is_empty() {
            self.selected_row = 0;
            return;
        }
        let mut idx = self.selected_row;
        while idx > 0 {
            idx -= 1;
            if matches!(rows[idx], TaskRow::Task(_)) {
                self.selected_row = idx;
                return;
            }
        }
        // No task above — snap to the first selectable row anywhere.
        if let Some(first) = first_task_row(rows) {
            self.selected_row = first;
        }
    }

    pub fn move_down(&mut self, rows: &[TaskRow]) {
        if rows.is_empty() {
            self.selected_row = 0;
            return;
        }
        let mut idx = self.selected_row;
        while idx + 1 < rows.len() {
            idx += 1;
            if matches!(rows[idx], TaskRow::Task(_)) {
                self.selected_row = idx;
                return;
            }
        }
        // No task below — snap to the last selectable row.
        if let Some(last) = last_task_row(rows) {
            self.selected_row = last;
        }
    }

    /// Clamp `selected_row` to a valid task row after the store
    /// changes shape (add/remove). Call this after every mutation.
    pub fn reclamp(&mut self, rows: &[TaskRow]) {
        if rows.is_empty() {
            self.selected_row = 0;
            return;
        }
        if self.selected_row >= rows.len() || matches!(rows[self.selected_row], TaskRow::Header(_))
        {
            self.selected_row = first_task_row(rows).unwrap_or(0);
        }
    }

    pub fn open_add(&mut self) {
        self.form = Some(TaskForm::new_add());
    }

    pub fn open_edit(&mut self, task: &Task) {
        self.form = Some(TaskForm::new_edit(task));
    }

    pub fn open_detail(&mut self, id: Uuid) {
        self.detail = Some(id);
    }

    pub fn close_modal(&mut self) {
        self.form = None;
        self.detail = None;
        self.prompt = None;
    }

    /// Cycle a task's status to the next entry in `STATUS_ORDER`,
    /// wrapping at the end.
    pub fn cycle_status(&mut self, store: &mut TaskStore, task_id: Uuid) -> Result<(), TaskError> {
        let current = store
            .get(task_id)
            .map(|t| t.status)
            .ok_or(TaskError::NotFound(task_id))?;
        let next = next_status(current);
        store.update(
            task_id,
            TaskPatch {
                status: Some(next),
                ..Default::default()
            },
            OPERATOR,
        )?;
        Ok(())
    }

    /// Parse the current form's fields and apply them to `store`. On
    /// success the modal is closed and `Ok(())` is returned. On
    /// failure, `self.form.error` is set with a human-readable
    /// message and `Err` is returned so the caller can surface it.
    pub fn submit_form(&mut self, store: &mut TaskStore) -> Result<(), TaskError> {
        // Take() the form so we can return it on error and short-
        // circuit borrow lifetimes during mutation.
        let Some(mut form) = self.form.take() else {
            return Ok(());
        };
        let result = apply_form(&form, store);
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                form.error = Some(e.to_string());
                self.form = Some(form);
                Err(e)
            }
        }
    }
}

fn apply_form(form: &TaskForm, store: &mut TaskStore) -> Result<(), TaskError> {
    match form.kind {
        TaskFormKind::Add => {
            let title = form.title.trim().to_string();
            if title.is_empty() {
                return Err(TaskError::EmptyTitle);
            }
            let description = non_empty_owned(&form.description);
            let assignee = non_empty_owned(&form.assignee);
            let blocked_by = parse_csv_uuids(&form.blocked_by_csv)?;
            store.add(
                title,
                description,
                assignee,
                OPERATOR.to_string(),
                blocked_by,
            )?;
            Ok(())
        }
        TaskFormKind::Edit(id) => {
            let title_patch = {
                let t = form.title.trim();
                if t.is_empty() {
                    return Err(TaskError::EmptyTitle);
                }
                // Include only when changed — keeps updated_at_secs
                // untouched if nothing substantive shifted.
                match store.get(id) {
                    Some(existing) if existing.title == t => None,
                    _ => Some(t.to_string()),
                }
            };
            let desc_patch = match store.get(id) {
                Some(existing) => {
                    let new_val = form.description.clone();
                    let current = existing.description.clone().unwrap_or_default();
                    if current == new_val {
                        None
                    } else {
                        // Empty string clears the description.
                        Some(new_val)
                    }
                }
                None => None,
            };
            let assignee_patch: Option<Option<String>> = match store.get(id) {
                Some(existing) => {
                    let new_val = non_empty_owned(&form.assignee);
                    if new_val == existing.assignee {
                        None
                    } else {
                        Some(new_val)
                    }
                }
                None => None,
            };
            let status_patch = if form.status.trim().is_empty() {
                None
            } else {
                Some(TaskStatus::parse(&form.status)?)
            };
            let add_deps = parse_csv_uuids(&form.blocked_by_csv)?;
            let remove_deps = parse_csv_uuids(&form.remove_blocked_by_csv)?;

            store.update(
                id,
                TaskPatch {
                    title: title_patch,
                    description: desc_patch,
                    status: status_patch,
                    assignee: assignee_patch,
                    note: None,
                    add_blocked_by: add_deps,
                    remove_blocked_by: remove_deps,
                },
                OPERATOR,
            )?;
            Ok(())
        }
    }
}

fn non_empty_owned(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

/// Parse a comma-separated list of UUIDs. Empty string → empty vec.
/// Whitespace is trimmed around each entry. Malformed entries
/// surface as `TaskError::UnknownDependency`-style failures — we
/// wrap them as `InvalidStatus`-less parse errors; the UI layer
/// just shows the message.
pub fn parse_csv_uuids(s: &str) -> Result<Vec<Uuid>, TaskError> {
    let mut out = Vec::new();
    for raw in s.split(',') {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        match Uuid::parse_str(t) {
            Ok(u) => out.push(u),
            Err(_) => {
                // Reuse UnknownDependency so the error surface stays
                // small; the message makes the actual cause clear.
                return Err(TaskError::UnknownDependency(Uuid::nil()));
            }
        }
    }
    Ok(out)
}

pub fn next_status(s: TaskStatus) -> TaskStatus {
    let pos = STATUS_ORDER.iter().position(|x| *x == s).unwrap_or(0);
    STATUS_ORDER[(pos + 1) % STATUS_ORDER.len()]
}

fn first_task_row(rows: &[TaskRow]) -> Option<usize> {
    rows.iter().position(|r| matches!(r, TaskRow::Task(_)))
}

fn last_task_row(rows: &[TaskRow]) -> Option<usize> {
    rows.iter().rposition(|r| matches!(r, TaskRow::Task(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_store() -> TaskStore {
        let mut s = TaskStore::new();
        s.add("open1".into(), None, None, "alice".into(), Vec::new())
            .unwrap();
        s.add(
            "assigned1".into(),
            None,
            Some("bob".into()),
            "alice".into(),
            Vec::new(),
        )
        .unwrap();
        s
    }

    #[test]
    fn rows_group_tasks_under_status_headers_in_order() {
        let s = seeded_store();
        let rows = TasksScreen::rows(&s);
        // 5 headers + 2 tasks.
        assert_eq!(rows.len(), 7);
        assert!(matches!(rows[0], TaskRow::Header(TaskStatus::Open)));
        assert!(matches!(rows[1], TaskRow::Task(_)));
        assert!(matches!(rows[2], TaskRow::Header(TaskStatus::Assigned)));
        assert!(matches!(rows[3], TaskRow::Task(_)));
        assert!(matches!(rows[4], TaskRow::Header(TaskStatus::InProgress)));
        assert!(matches!(rows[5], TaskRow::Header(TaskStatus::Done)));
        assert!(matches!(rows[6], TaskRow::Header(TaskStatus::Cancelled)));
    }

    #[test]
    fn move_down_skips_headers() {
        let s = seeded_store();
        let rows = TasksScreen::rows(&s);
        let mut screen = TasksScreen {
            selected_row: 1, // first task
            ..Default::default()
        };
        screen.move_down(&rows);
        assert_eq!(screen.selected_row, 3); // second task; skipped the header at 2
    }

    #[test]
    fn move_up_skips_headers() {
        let s = seeded_store();
        let rows = TasksScreen::rows(&s);
        let mut screen = TasksScreen {
            selected_row: 3,
            ..Default::default()
        };
        screen.move_up(&rows);
        assert_eq!(screen.selected_row, 1);
    }

    #[test]
    fn selected_task_id_returns_none_for_header_rows() {
        let s = seeded_store();
        let rows = TasksScreen::rows(&s);
        let screen = TasksScreen {
            selected_row: 0, // header
            ..Default::default()
        };
        assert!(screen.selected_task_id(&rows).is_none());
    }

    #[test]
    fn reclamp_snaps_away_from_header_after_delete() {
        let mut s = seeded_store();
        let rows_before = TasksScreen::rows(&s);
        // Selection on the second task (assigned1).
        let mut screen = TasksScreen {
            selected_row: 3,
            ..Default::default()
        };
        let assigned_id = match rows_before[3] {
            TaskRow::Task(id) => id,
            _ => panic!(),
        };
        s.remove(assigned_id).unwrap();
        let rows_after = TasksScreen::rows(&s);
        screen.reclamp(&rows_after);
        // Row 3 is now the "in_progress" header; reclamp should
        // snap to the first selectable task in the new rows.
        assert_eq!(screen.selected_row, 1);
    }

    #[test]
    fn parse_csv_uuids_handles_whitespace_and_empty() {
        assert!(parse_csv_uuids("").unwrap().is_empty());
        assert!(parse_csv_uuids("  ,  ,  ").unwrap().is_empty());
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let s = format!("  {a}  ,{b}");
        let got = parse_csv_uuids(&s).unwrap();
        assert_eq!(got, vec![a, b]);
    }

    #[test]
    fn parse_csv_uuids_rejects_malformed() {
        assert!(parse_csv_uuids("not-a-uuid").is_err());
    }

    #[test]
    fn next_status_cycles_through_all_and_wraps() {
        assert_eq!(next_status(TaskStatus::Open), TaskStatus::Assigned);
        assert_eq!(next_status(TaskStatus::Assigned), TaskStatus::InProgress);
        assert_eq!(next_status(TaskStatus::InProgress), TaskStatus::Done);
        assert_eq!(next_status(TaskStatus::Done), TaskStatus::Cancelled);
        assert_eq!(next_status(TaskStatus::Cancelled), TaskStatus::Open);
    }

    #[test]
    fn submit_add_form_creates_task() {
        let mut s = TaskStore::new();
        let mut screen = TasksScreen {
            form: Some(TaskForm {
                kind: TaskFormKind::Add,
                title: "from ui".into(),
                description: "".into(),
                assignee: "coder".into(),
                status: "".into(),
                blocked_by_csv: "".into(),
                remove_blocked_by_csv: "".into(),
                focused_field: FormField::Title,
                error: None,
                assignee_options: vec!["".into(), "coder".into()],
                picker: None,
            }),
            ..Default::default()
        };
        screen.submit_form(&mut s).unwrap();
        assert!(screen.form.is_none());
        assert_eq!(s.all().len(), 1);
        assert_eq!(s.all()[0].title, "from ui");
        assert_eq!(s.all()[0].assignee.as_deref(), Some("coder"));
        assert_eq!(s.all()[0].created_by, OPERATOR);
    }

    #[test]
    fn submit_add_form_rejects_empty_title_and_keeps_form_open() {
        let mut s = TaskStore::new();
        let mut screen = TasksScreen {
            form: Some(TaskForm::new_add()),
            ..Default::default()
        };
        // Blank title — should error.
        let err = screen.submit_form(&mut s).unwrap_err();
        assert!(matches!(err, TaskError::EmptyTitle));
        // Form stays open with error surfaced.
        assert!(screen.form.is_some());
        assert!(screen.form.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn submit_edit_form_updates_title_status_and_deps() {
        let mut s = TaskStore::new();
        let prereq = s
            .add("prereq".into(), None, None, "a".into(), Vec::new())
            .unwrap()
            .id;
        let main = s
            .add("main".into(), None, None, "a".into(), Vec::new())
            .unwrap()
            .id;
        let main_task = s.get(main).unwrap().clone();

        let mut screen = TasksScreen::default();
        let mut form = TaskForm::new_edit(&main_task);
        form.title = "main renamed".into();
        form.status = "in_progress".into();
        form.blocked_by_csv = prereq.to_string();
        screen.form = Some(form);

        screen.submit_form(&mut s).unwrap();
        let t = s.get(main).unwrap();
        assert_eq!(t.title, "main renamed");
        assert_eq!(t.status, TaskStatus::InProgress);
        assert_eq!(t.blocked_by, vec![prereq]);
    }

    #[test]
    fn cycle_status_walks_through_order() {
        let mut s = TaskStore::new();
        let id = s
            .add("t".into(), None, None, "a".into(), Vec::new())
            .unwrap()
            .id;
        let mut screen = TasksScreen::default();
        for expected in [
            TaskStatus::Assigned,
            TaskStatus::InProgress,
            TaskStatus::Done,
            TaskStatus::Cancelled,
            TaskStatus::Open,
        ] {
            screen.cycle_status(&mut s, id).unwrap();
            assert_eq!(s.get(id).unwrap().status, expected);
        }
    }

    #[test]
    fn form_tab_order_add_has_4_fields_edit_has_6() {
        let add = TaskForm::new_add();
        assert_eq!(add.tab_order().len(), 4);
        let mut s = TaskStore::new();
        let id = s
            .add("t".into(), None, None, "a".into(), Vec::new())
            .unwrap()
            .id;
        let edit = TaskForm::new_edit(s.get(id).unwrap());
        assert_eq!(edit.tab_order().len(), 6);
    }

    #[test]
    fn set_assignee_options_inserts_unassigned_and_preserves_current() {
        let mut form = TaskForm::new_add();
        form.assignee = "ghost".into();
        form.set_assignee_options(vec!["a".into(), "b".into()]);
        // "" prepended; "ghost" appended because it wasn't in the
        // supplied list.
        assert_eq!(form.assignee_options[0], "");
        assert!(form.assignee_options.contains(&"ghost".to_string()));
    }

    #[test]
    fn picker_opens_with_current_value_preselected() {
        let mut form = TaskForm::new_add();
        form.set_assignee_options(vec!["a".into(), "b".into(), "c".into()]);
        form.assignee = "b".into();
        form.focused_field = FormField::Assignee;
        form.open_picker();
        let p = form.picker.as_ref().unwrap();
        assert_eq!(p.field, FormField::Assignee);
        assert_eq!(p.selected, p.options.iter().position(|o| o == "b").unwrap());
    }

    #[test]
    fn picker_ignores_non_dropdown_fields() {
        let mut form = TaskForm::new_add();
        form.focused_field = FormField::Title;
        form.open_picker();
        assert!(form.picker.is_none());
    }

    #[test]
    fn picker_move_wraps_in_both_directions() {
        let mut form = TaskForm::new_add();
        form.set_assignee_options(vec!["a".into(), "b".into(), "c".into()]);
        form.focused_field = FormField::Assignee;
        form.open_picker();
        // Starts at index 0 ("") since default assignee is "".
        form.picker_move(1);
        assert_eq!(form.picker.as_ref().unwrap().selected, 1);
        form.picker_move(-1);
        assert_eq!(form.picker.as_ref().unwrap().selected, 0);
        form.picker_move(-1);
        // Wrapped to last option.
        assert_eq!(
            form.picker.as_ref().unwrap().selected,
            form.picker.as_ref().unwrap().options.len() - 1
        );
    }

    #[test]
    fn commit_picker_writes_selection_to_field_and_closes() {
        let mut form = TaskForm::new_add();
        form.set_assignee_options(vec!["a".into(), "b".into()]);
        form.focused_field = FormField::Assignee;
        // Options after set_assignee_options: ["", "a", "b"].
        // Current assignee is "" → picker opens with selected=0.
        form.open_picker();
        assert_eq!(form.picker.as_ref().unwrap().selected, 0);
        form.picker_move(1); // -> index 1 ("a")
        form.commit_picker();
        assert!(form.picker.is_none());
        assert_eq!(form.assignee, "a");
    }

    #[test]
    fn status_picker_uses_status_order() {
        let mut form = TaskForm::new_edit(&Task {
            id: Uuid::new_v4(),
            title: "t".into(),
            description: None,
            status: TaskStatus::Open,
            assignee: None,
            created_by: "a".into(),
            created_at_secs: 0,
            updated_at_secs: 0,
            notes: vec![],
            blocked_by: vec![],
        });
        form.focused_field = FormField::Status;
        form.open_picker();
        let p = form.picker.as_ref().unwrap();
        assert_eq!(p.options.len(), STATUS_ORDER.len());
        assert_eq!(p.options[0], "open");
        // Preselects the current status ("open" → index 0).
        assert_eq!(p.selected, 0);
    }

    #[test]
    fn is_dropdown_identifies_assignee_and_status() {
        assert!(TaskForm::is_dropdown(FormField::Assignee));
        assert!(TaskForm::is_dropdown(FormField::Status));
        assert!(!TaskForm::is_dropdown(FormField::Title));
        assert!(!TaskForm::is_dropdown(FormField::Description));
        assert!(!TaskForm::is_dropdown(FormField::BlockedByCsv));
    }

    #[test]
    fn focus_next_wraps_in_add_form() {
        let mut form = TaskForm::new_add();
        assert_eq!(form.focused_field, FormField::Title);
        form.focus_next();
        assert_eq!(form.focused_field, FormField::Description);
        form.focus_next();
        assert_eq!(form.focused_field, FormField::Assignee);
        form.focus_next();
        assert_eq!(form.focused_field, FormField::BlockedByCsv);
        form.focus_next();
        // Wraps back to Title.
        assert_eq!(form.focused_field, FormField::Title);
    }
}
