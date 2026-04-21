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

/// Fields (and buttons) in the modal form, laid out in tab order.
/// Buttons participate in Tab cycling and can also be clicked; they
/// carry no editable value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Title,
    Description,
    Assignee,
    Status,
    /// Task-dependency list. Enter opens a multi-select picker.
    BlockedBy,
    Ok,
    Cancel,
}

impl FormField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Description => "Description",
            Self::Assignee => "Assignee",
            Self::Status => "Status",
            Self::BlockedBy => "Blocked by",
            Self::Ok => "OK",
            Self::Cancel => "Cancel",
        }
    }

    pub fn is_button(self) -> bool {
        matches!(self, Self::Ok | Self::Cancel)
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

/// Multi-select popup for the `Blocked by` field. Each row shows a
/// task title + status + short id with a checkbox; Space or click
/// toggles a row; Enter commits the full set to the form.
#[derive(Debug, Clone)]
pub struct BlockedByPicker {
    pub options: Vec<BlockedByRow>,
    pub checked: Vec<bool>,
    pub cursor: usize,
    /// Index of the first visible option. The renderer clamps this
    /// each frame so the cursor stays inside the visible window.
    pub scroll: usize,
}

#[derive(Debug, Clone)]
pub struct BlockedByRow {
    pub id: Uuid,
    pub title: String,
    pub status: TaskStatus,
}

/// Default minimum number of text-lines visible inside the
/// Description sub-block when the modal is cramped. The actual
/// height expands to use whatever vertical space the full-screen
/// modal has left over after the other fields.
pub const DESCRIPTION_MIN_VISIBLE_LINES: u16 = 6;

/// One visual segment of the description after word-wrap. When the
/// wrap width is zero (caller doesn't want to wrap), one segment
/// per logical line is emitted and `logical_col_start` is always 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisualLine {
    /// Byte offset into the description where this segment begins.
    pub byte_start: usize,
    /// Byte offset where this segment ends (exclusive; does NOT
    /// include a trailing `\n`).
    pub byte_end: usize,
    /// Index of the logical line (as split by `\n`) that this
    /// segment belongs to.
    pub logical_line: u16,
    /// Column within the logical line where this visual segment
    /// begins. 0 for the first segment of each line.
    pub logical_col_start: u16,
}

#[derive(Debug, Clone)]
pub struct TaskForm {
    pub kind: TaskFormKind,
    pub title: String,
    pub description: String,
    /// Byte offset of the insertion cursor inside `description`.
    /// Kept in sync with edits so arrow-key movement works.
    pub description_cursor: usize,
    /// Scroll offset (in visual lines, post-wrap) hidden above the
    /// visible window. Zero means the top of the description.
    pub description_scroll: u16,
    /// Wrap width the Description box was last rendered at. Key
    /// handlers read this to keep cursor-move / scroll math
    /// consistent with what's actually on screen. Zero means "no
    /// wrap" / headless unit test.
    pub last_wrap_width: u16,
    /// Number of visible text rows inside the Description box as of
    /// the last render. Used by key handlers to keep the cursor
    /// onscreen after edits.
    pub last_visible_lines: u16,
    pub assignee: String,
    pub status: String,
    /// Current in-form blocked-by set. For Edit, seeded from the
    /// task's existing deps at open time (also snapshotted into
    /// `original_blocked_by` so `apply_form` can compute the diff).
    /// For Add, starts empty.
    pub blocked_by: Vec<Uuid>,
    /// Snapshot of the task's blocked-by at edit-open time. Empty
    /// for Add; used only for the diff on submit.
    pub original_blocked_by: Vec<Uuid>,
    pub focused_field: FormField,
    pub error: Option<String>,
    /// Options offered by the Assignee dropdown. The empty string
    /// (rendered as "(unassigned)") must be present; callers inject
    /// the rest based on the team's chat display names at open
    /// time via [`set_assignee_options`].
    pub assignee_options: Vec<String>,
    /// When `Some`, a single-select dropdown picker is overlaid on
    /// the form (assignee / status).
    pub picker: Option<DropdownPicker>,
    /// When `Some`, the multi-select blocked-by picker is overlaid
    /// on the form.
    pub blocked_by_picker: Option<BlockedByPicker>,
    /// Unused sink returned by `field_mut` for button variants so
    /// the signature can stay `&mut String` without panics. Key
    /// handlers gate character input on `FormField::is_button` so
    /// this string is never actually written to in the happy path.
    #[doc(hidden)]
    pub _button_sink: String,
}

impl TaskForm {
    pub fn new_add() -> Self {
        Self {
            kind: TaskFormKind::Add,
            title: String::new(),
            description: String::new(),
            description_cursor: 0,
            description_scroll: 0,
            last_wrap_width: 0,
            last_visible_lines: DESCRIPTION_MIN_VISIBLE_LINES,
            assignee: String::new(),
            status: String::new(),
            blocked_by: Vec::new(),
            original_blocked_by: Vec::new(),
            focused_field: FormField::Title,
            error: None,
            assignee_options: vec![String::new()],
            picker: None,
            blocked_by_picker: None,
            _button_sink: String::new(),
        }
    }

    pub fn new_edit(task: &Task) -> Self {
        let desc = task.description.clone().unwrap_or_default();
        let cursor = desc.len();
        Self {
            kind: TaskFormKind::Edit(task.id),
            title: task.title.clone(),
            description: desc,
            description_cursor: cursor,
            description_scroll: 0,
            last_wrap_width: 0,
            last_visible_lines: DESCRIPTION_MIN_VISIBLE_LINES,
            assignee: task.assignee.clone().unwrap_or_default(),
            status: task.status.as_str().to_string(),
            blocked_by: task.blocked_by.clone(),
            original_blocked_by: task.blocked_by.clone(),
            focused_field: FormField::Title,
            error: None,
            assignee_options: vec![String::new()],
            picker: None,
            blocked_by_picker: None,
            _button_sink: String::new(),
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

    /// All fields in tab order. For Add, the Status field is
    /// skipped since new tasks derive their status from whether an
    /// assignee is set. Ok and Cancel buttons are always last so
    /// Tabbing lands you on them before wrapping to Title.
    pub fn tab_order(&self) -> &'static [FormField] {
        match self.kind {
            TaskFormKind::Add => &[
                FormField::Title,
                FormField::Description,
                FormField::Assignee,
                FormField::BlockedBy,
                FormField::Ok,
                FormField::Cancel,
            ],
            TaskFormKind::Edit(_) => &[
                FormField::Title,
                FormField::Description,
                FormField::Assignee,
                FormField::Status,
                FormField::BlockedBy,
                FormField::Ok,
                FormField::Cancel,
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

    /// Returns a mutable string ref for text-backed fields.
    /// Non-text fields (buttons, the list-picker `BlockedBy`) have
    /// no editable string value; callers must gate character input
    /// before writing. The sink is returned for those variants to
    /// keep the signature `&mut String` without panics.
    pub fn field_mut(&mut self, field: FormField) -> &mut String {
        match field {
            FormField::Title => &mut self.title,
            FormField::Description => &mut self.description,
            FormField::Assignee => &mut self.assignee,
            FormField::Status => &mut self.status,
            FormField::BlockedBy | FormField::Ok | FormField::Cancel => &mut self._button_sink,
        }
    }

    pub fn field(&self, field: FormField) -> &str {
        match field {
            FormField::Title => &self.title,
            FormField::Description => &self.description,
            FormField::Assignee => &self.assignee,
            FormField::Status => &self.status,
            FormField::BlockedBy | FormField::Ok | FormField::Cancel => "",
        }
    }

    /// Open the multi-select picker for Blocked-by. `all_tasks` is
    /// the full live task set; `self_id` (Some for Edit) is
    /// excluded. Eligibility: rows already in `self.blocked_by` are
    /// always included (so the user can uncheck them, even Done /
    /// Cancelled ones); everything else is only shown if it's in
    /// one of the still-active statuses.
    pub fn open_blocked_by_picker(&mut self, all_tasks: &[Task], self_id: Option<Uuid>) {
        let mut rows: Vec<BlockedByRow> = Vec::new();
        let mut checked: Vec<bool> = Vec::new();

        // Pass 1: entries already in blocked_by go first, checked.
        for id in &self.blocked_by {
            if Some(*id) == self_id {
                continue;
            }
            if let Some(t) = all_tasks.iter().find(|t| t.id == *id) {
                rows.push(BlockedByRow {
                    id: t.id,
                    title: t.title.clone(),
                    status: t.status,
                });
                checked.push(true);
            }
        }
        // Pass 2: active tasks not already in blocked_by, unchecked.
        for t in all_tasks {
            if Some(t.id) == self_id {
                continue;
            }
            if rows.iter().any(|r| r.id == t.id) {
                continue;
            }
            let eligible = matches!(
                t.status,
                TaskStatus::Open | TaskStatus::Assigned | TaskStatus::InProgress
            );
            if !eligible {
                continue;
            }
            rows.push(BlockedByRow {
                id: t.id,
                title: t.title.clone(),
                status: t.status,
            });
            checked.push(false);
        }

        self.blocked_by_picker = Some(BlockedByPicker {
            options: rows,
            checked,
            cursor: 0,
            scroll: 0,
        });
    }

    pub fn close_blocked_by_picker(&mut self) {
        self.blocked_by_picker = None;
    }

    pub fn blocked_by_picker_move(&mut self, delta: i32) {
        if let Some(p) = self.blocked_by_picker.as_mut() {
            if p.options.is_empty() {
                return;
            }
            let len = p.options.len() as i32;
            p.cursor = ((p.cursor as i32 + delta).rem_euclid(len)) as usize;
        }
    }

    pub fn blocked_by_picker_toggle(&mut self) {
        if let Some(p) = self.blocked_by_picker.as_mut() {
            if let Some(c) = p.checked.get_mut(p.cursor) {
                *c = !*c;
            }
        }
    }

    pub fn blocked_by_picker_toggle_at(&mut self, idx: usize) {
        if let Some(p) = self.blocked_by_picker.as_mut() {
            if let Some(c) = p.checked.get_mut(idx) {
                *c = !*c;
                p.cursor = idx;
            }
        }
    }

    /// Write the checked set of the blocked-by picker back into
    /// `self.blocked_by`, preserving option order. Closes the
    /// picker.
    pub fn commit_blocked_by_picker(&mut self) {
        let Some(p) = self.blocked_by_picker.take() else {
            return;
        };
        self.blocked_by = p
            .options
            .iter()
            .zip(p.checked.iter())
            .filter_map(|(row, on)| if *on { Some(row.id) } else { None })
            .collect();
    }

    // ---- Description text-editor helpers ----

    /// Insert a character at the cursor. Newlines are accepted so
    /// `\n` can be piped in from Enter.
    pub fn description_insert(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.description.insert_str(self.description_cursor, s);
        self.description_cursor += s.len();
    }

    pub fn description_backspace(&mut self) {
        if self.description_cursor == 0 {
            return;
        }
        let n = utf8_char_len_prev(&self.description, self.description_cursor);
        if n == 0 {
            return;
        }
        let start = self.description_cursor - n;
        self.description.drain(start..self.description_cursor);
        self.description_cursor = start;
    }

    pub fn description_move_char(&mut self, delta: i32) {
        if delta > 0 {
            let n = utf8_char_len(&self.description, self.description_cursor);
            self.description_cursor = (self.description_cursor + n).min(self.description.len());
        } else if delta < 0 {
            let n = utf8_char_len_prev(&self.description, self.description_cursor);
            self.description_cursor = self.description_cursor.saturating_sub(n);
        }
    }

    /// Move the cursor up/down one visual line, preserving column
    /// as best effort. Returns false when the move would leave the
    /// text (caller should tab to the next/prev field instead).
    /// Move the cursor up/down one *visual* line (wrap-aware when
    /// `self.last_wrap_width` is set). Returns false when the move
    /// would leave the text — the caller should tab to the
    /// next/prev field.
    pub fn description_move_line(&mut self, delta: i32) -> bool {
        let lines = wrap_description(&self.description, self.last_wrap_width);
        let (v_idx, col_in_seg) =
            cursor_visual_pos(&self.description, &lines, self.description_cursor);
        if delta < 0 {
            if v_idx == 0 {
                return false;
            }
            let target = &lines[v_idx - 1];
            let target_len = segment_char_count(&self.description, target);
            let col = col_in_seg.min(target_len);
            self.description_cursor = offset_at_col_in_segment(&self.description, target, col);
            true
        } else if delta > 0 {
            if v_idx + 1 >= lines.len() {
                return false;
            }
            let target = &lines[v_idx + 1];
            let target_len = segment_char_count(&self.description, target);
            let col = col_in_seg.min(target_len);
            self.description_cursor = offset_at_col_in_segment(&self.description, target, col);
            true
        } else {
            true
        }
    }

    /// Adjust `description_scroll` so the cursor's visual line is
    /// visible inside a window of `visible_height` rows. Uses
    /// `self.last_wrap_width` for wrap math; when zero, each
    /// logical line is one visual line.
    pub fn description_ensure_visible(&mut self, visible_height: u16) {
        if visible_height == 0 {
            return;
        }
        let lines = wrap_description(&self.description, self.last_wrap_width);
        let (v_idx_usize, _) =
            cursor_visual_pos(&self.description, &lines, self.description_cursor);
        let v_idx = v_idx_usize as u16;
        if v_idx < self.description_scroll {
            self.description_scroll = v_idx;
        } else if v_idx >= self.description_scroll + visible_height {
            self.description_scroll = v_idx - visible_height + 1;
        }
    }
}

/// Returns `(visual_line_index, col_in_segment)` for a byte offset
/// into `s`. When the cursor sits on a wrap boundary (at a
/// segment's `byte_end` AND the next segment starts there within
/// the same logical line), the cursor is reported on the *next*
/// segment so further typing continues on the wrapped row.
pub fn cursor_visual_pos(s: &str, lines: &[VisualLine], cursor: usize) -> (usize, u16) {
    if lines.is_empty() {
        return (0, 0);
    }
    for (i, v) in lines.iter().enumerate() {
        if cursor >= v.byte_start && cursor <= v.byte_end {
            if cursor == v.byte_end {
                if let Some(next) = lines.get(i + 1) {
                    if next.logical_line == v.logical_line && next.byte_start == cursor {
                        continue;
                    }
                }
            }
            let col = s[v.byte_start..cursor].chars().count() as u16;
            return (i, col);
        }
    }
    let last = lines.len() - 1;
    let seg = &lines[last];
    let count = s[seg.byte_start..seg.byte_end].chars().count() as u16;
    (last, count)
}

fn segment_char_count(s: &str, v: &VisualLine) -> u16 {
    s[v.byte_start..v.byte_end].chars().count() as u16
}

fn offset_at_col_in_segment(s: &str, segment: &VisualLine, col: u16) -> usize {
    let mut remaining = col as usize;
    let mut off = segment.byte_start;
    while off < segment.byte_end && remaining > 0 {
        let c_len = utf8_char_len(s, off);
        off += c_len;
        remaining -= 1;
    }
    off
}

/// One-shot prompts that piggy-back on the chat input bar.
#[derive(Debug, Clone)]
pub enum TasksPrompt {
    /// Collecting a note body for `task_id`.
    Note { task_id: Uuid, body: String },
    /// Awaiting y/n on delete confirmation.
    ConfirmDelete { task_id: Uuid, buffer: String },
}

/// The two sub-entity overlays (TODOs and Comments) that sit on top
/// of the Tasks view and let the operator manage a single task's
/// children. `Prompt` is a shared one-line text-input mode used for
/// add / edit / delete-confirmation flows.
#[derive(Debug, Clone)]
pub enum ChildOverlay {
    Todos {
        task_id: Uuid,
        cursor: usize,
    },
    Comments {
        task_id: Uuid,
        cursor: usize,
    },
    Prompt {
        task_id: Uuid,
        kind: ChildPromptKind,
        buffer: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ChildPromptKind {
    AddTodo,
    EditTodo(Uuid),
    ConfirmDeleteTodo(Uuid),
    AddComment,
    EditComment(Uuid),
    ConfirmDeleteComment(Uuid),
}

impl ChildOverlay {
    /// Which task this overlay belongs to, regardless of state.
    pub fn task_id(&self) -> Uuid {
        match self {
            Self::Todos { task_id, .. }
            | Self::Comments { task_id, .. }
            | Self::Prompt { task_id, .. } => *task_id,
        }
    }
}

#[derive(Debug, Default)]
pub struct TasksScreen {
    pub selected_row: usize,
    pub form: Option<TaskForm>,
    pub detail: Option<Uuid>,
    pub prompt: Option<TasksPrompt>,
    /// The TODOs or Comments overlay for a task, when open.
    /// Mutually exclusive with `form` / `detail` / `prompt` in
    /// practice; the key handler gates on this field's presence
    /// before routing any normal task-view keys.
    pub child_overlay: Option<ChildOverlay>,
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
        self.child_overlay = None;
    }

    // ---- Child overlays (TODOs + Comments) ----

    pub fn open_todos(&mut self, task_id: Uuid) {
        self.child_overlay = Some(ChildOverlay::Todos { task_id, cursor: 0 });
    }

    pub fn open_comments(&mut self, task_id: Uuid) {
        self.child_overlay = Some(ChildOverlay::Comments { task_id, cursor: 0 });
    }

    /// Move the cursor in an open Browse overlay, clamped to the
    /// count of rows available. No-op if the overlay is in a prompt
    /// state or if there are no rows.
    pub fn child_overlay_move(&mut self, delta: i32, row_count: usize) {
        if row_count == 0 {
            return;
        }
        let len = row_count as i32;
        match self.child_overlay.as_mut() {
            Some(ChildOverlay::Todos { cursor, .. })
            | Some(ChildOverlay::Comments { cursor, .. }) => {
                *cursor = ((*cursor as i32 + delta).rem_euclid(len)) as usize;
            }
            _ => {}
        }
    }

    /// Jump the Browse cursor to an absolute row (used by
    /// click-to-select). No-op if the index is out of range.
    pub fn child_overlay_set_cursor(&mut self, idx: usize, row_count: usize) {
        if idx >= row_count {
            return;
        }
        match self.child_overlay.as_mut() {
            Some(ChildOverlay::Todos { cursor, .. })
            | Some(ChildOverlay::Comments { cursor, .. }) => {
                *cursor = idx;
            }
            _ => {}
        }
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
            store.add(
                title,
                description,
                assignee,
                OPERATOR.to_string(),
                form.blocked_by.clone(),
            )?;
            Ok(())
        }
        TaskFormKind::Edit(id) => {
            let title_patch = {
                let t = form.title.trim();
                if t.is_empty() {
                    return Err(TaskError::EmptyTitle);
                }
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
            // Diff original vs current blocked-by.
            let add_deps: Vec<Uuid> = form
                .blocked_by
                .iter()
                .copied()
                .filter(|id| !form.original_blocked_by.contains(id))
                .collect();
            let remove_deps: Vec<Uuid> = form
                .original_blocked_by
                .iter()
                .copied()
                .filter(|id| !form.blocked_by.contains(id))
                .collect();

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

/// Byte length in UTF-8 of the char starting at `offset`, or 0 if
/// out of bounds.
fn utf8_char_len(s: &str, offset: usize) -> usize {
    if offset >= s.len() {
        return 0;
    }
    let bytes = s.as_bytes();
    let lead = bytes[offset];
    if lead < 0x80 {
        1
    } else if lead < 0xC0 {
        // Continuation byte; caller mispositioned. Treat as 1 so we
        // don't infinite-loop.
        1
    } else if lead < 0xE0 {
        2
    } else if lead < 0xF0 {
        3
    } else {
        4
    }
}

/// Byte length of the char that *ends* at `offset` (i.e. the char
/// immediately before cursor position `offset`). Returns 0 at the
/// start of the string.
fn utf8_char_len_prev(s: &str, offset: usize) -> usize {
    if offset == 0 || offset > s.len() {
        return 0;
    }
    let bytes = s.as_bytes();
    let mut i = offset - 1;
    while i > 0 && (bytes[i] & 0xC0) == 0x80 {
        i -= 1;
    }
    offset - i
}

/// Convert a byte offset into (line_index, column_as_char_count).
/// Kept test-only for now; the production cursor path uses
/// `cursor_visual_pos` which is wrap-aware.
#[cfg(test)]
pub fn line_col_from_offset(s: &str, offset: usize) -> (u16, u16) {
    let offset = offset.min(s.len());
    let mut line: u16 = 0;
    let mut col: u16 = 0;
    let mut i = 0;
    while i < offset {
        let c_len = utf8_char_len(s, i);
        let ch = s[i..i + c_len].chars().next().unwrap_or('\u{fffd}');
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
        i += c_len;
    }
    (line, col)
}

/// Inverse of `line_col_from_offset`, with column clamped to the
/// line's length. Test-only counterpart to `line_col_from_offset`.
#[cfg(test)]
pub fn offset_from_line_col(s: &str, line: u16, col: u16) -> usize {
    let mut cur_line: u16 = 0;
    let mut cur_col: u16 = 0;
    let mut i = 0;
    while i < s.len() {
        if cur_line == line && cur_col == col {
            return i;
        }
        let c_len = utf8_char_len(s, i);
        let ch = s[i..i + c_len].chars().next().unwrap_or('\u{fffd}');
        if ch == '\n' {
            if cur_line == line {
                return i;
            }
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += 1;
        }
        i += c_len;
    }
    // Reached end: return the end offset (valid cursor position).
    s.len()
}

/// Split `s` into visual segments that fit inside a rendering
/// window `width` characters wide. `\n` always starts a new logical
/// line (and a new visual segment). If `width == 0`, one segment
/// per logical line is emitted — no wrapping is applied.
///
/// Wrapping strategy:
/// - Break at the last whitespace (` `) before the width limit so
///   whole words stay together.
/// - If there's no whitespace in the current segment (word longer
///   than `width`), fall back to a character break so the rest of
///   the line still fits on screen.
/// - The space we break at is consumed into the preceding segment
///   (segment's byte range ends *after* the space). This keeps the
///   cursor byte offset continuously walkable across wrap
///   boundaries — there's no byte that belongs to no segment.
pub fn wrap_description(s: &str, width: u16) -> Vec<VisualLine> {
    let mut out: Vec<VisualLine> = Vec::new();

    // Build (start, end, logical_line) ranges — one per logical
    // line separated by `\n`.
    let mut logical_ranges: Vec<(usize, usize, u16)> = Vec::new();
    {
        let mut ls = 0;
        let mut ll: u16 = 0;
        for (i, b) in s.bytes().enumerate() {
            if b == b'\n' {
                logical_ranges.push((ls, i, ll));
                ls = i + 1;
                ll += 1;
            }
        }
        logical_ranges.push((ls, s.len(), ll));
    }

    for (start, end, logical_line) in logical_ranges {
        if start == end {
            out.push(VisualLine {
                byte_start: start,
                byte_end: end,
                logical_line,
                logical_col_start: 0,
            });
            continue;
        }
        if width == 0 {
            out.push(VisualLine {
                byte_start: start,
                byte_end: end,
                logical_line,
                logical_col_start: 0,
            });
            continue;
        }

        let w = width as usize;
        let mut cur = start;

        while cur < end {
            let seg_start = cur;
            let col_start = s[start..seg_start].chars().count() as u16;

            // Scan up to `w` chars, remembering the most recent
            // whitespace we pass so we can back up to it if we
            // need to word-break.
            let mut scan = cur;
            let mut chars = 0usize;
            let mut last_space_byte: Option<usize> = None;
            while scan < end && chars < w {
                let c = s[scan..].chars().next().unwrap();
                let c_len = c.len_utf8();
                if c == ' ' {
                    last_space_byte = Some(scan);
                }
                scan += c_len;
                chars += 1;
            }

            let seg_end: usize;
            if scan >= end {
                // Whole remainder of the logical line fits — emit it.
                seg_end = scan;
                cur = scan;
            } else {
                // We hit the width. Try to word-break.
                let next_char = s[scan..].chars().next().unwrap();
                if next_char == ' ' {
                    // Perfect break: the char past our window IS a
                    // space. Include the space in this segment and
                    // start the next one after it.
                    let sp_len = next_char.len_utf8();
                    seg_end = scan + sp_len;
                    cur = seg_end;
                } else if let Some(sp) = last_space_byte {
                    // Word-break at the last space in the segment.
                    // Segment ends AFTER the space so no byte is
                    // orphaned between segments.
                    seg_end = sp + ' '.len_utf8();
                    cur = seg_end;
                } else {
                    // No space found — the word is longer than the
                    // wrap width. Char-break.
                    seg_end = scan;
                    cur = scan;
                }
            }

            out.push(VisualLine {
                byte_start: seg_start,
                byte_end: seg_end,
                logical_line,
                logical_col_start: col_start,
            });
        }
    }

    out
}

#[cfg(test)]
pub fn line_count(s: &str) -> u16 {
    if s.is_empty() {
        return 1;
    }
    let mut n: u16 = 1;
    for b in s.bytes() {
        if b == b'\n' {
            n += 1;
        }
    }
    n
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
    fn description_line_col_and_back() {
        let s = "hello\nworld\nfoo";
        assert_eq!(line_col_from_offset(s, 0), (0, 0));
        assert_eq!(line_col_from_offset(s, 5), (0, 5));
        assert_eq!(line_col_from_offset(s, 6), (1, 0));
        assert_eq!(line_col_from_offset(s, 11), (1, 5));
        assert_eq!(line_col_from_offset(s, 12), (2, 0));
        // Round trip roughly: col clamps to line length at end.
        let off = offset_from_line_col(s, 1, 3);
        assert_eq!(&s[..off], "hello\nwor");
    }

    #[test]
    fn description_line_count_counts_lines_and_empty() {
        assert_eq!(line_count(""), 1);
        assert_eq!(line_count("a"), 1);
        assert_eq!(line_count("a\nb"), 2);
        assert_eq!(line_count("a\nb\n"), 3);
    }

    #[test]
    fn wrap_description_emits_one_segment_per_logical_line_when_width_is_zero() {
        let lines = wrap_description("alpha\nbeta", 0);
        assert_eq!(lines.len(), 2);
        assert_eq!(
            &"alpha\nbeta"[lines[0].byte_start..lines[0].byte_end],
            "alpha"
        );
        assert_eq!(
            &"alpha\nbeta"[lines[1].byte_start..lines[1].byte_end],
            "beta"
        );
    }

    #[test]
    fn wrap_description_char_breaks_words_longer_than_width() {
        // Single word with no spaces → falls back to char-break so
        // the text still fits on screen.
        let s = "abcdefghij";
        let lines = wrap_description(s, 4);
        let got: Vec<&str> = lines.iter().map(|v| &s[v.byte_start..v.byte_end]).collect();
        assert_eq!(got, vec!["abcd", "efgh", "ij"]);
        assert!(lines.iter().all(|v| v.logical_line == 0));
        assert_eq!(lines[1].logical_col_start, 4);
        assert_eq!(lines[2].logical_col_start, 8);
    }

    #[test]
    fn wrap_description_word_breaks_at_spaces() {
        let s = "hello world foo bar";
        let lines = wrap_description(s, 8);
        let got: Vec<&str> = lines.iter().map(|v| &s[v.byte_start..v.byte_end]).collect();
        assert_eq!(got, vec!["hello ", "world ", "foo bar"]);
    }

    #[test]
    fn wrap_description_mixes_word_and_char_break() {
        // "abc defghijk" — first word fits, long word after breaks
        // character-wise because there's no space inside it.
        let s = "abc defghijk";
        let lines = wrap_description(s, 5);
        let got: Vec<&str> = lines.iter().map(|v| &s[v.byte_start..v.byte_end]).collect();
        assert_eq!(got, vec!["abc ", "defgh", "ijk"]);
    }

    #[test]
    fn wrap_description_does_not_orphan_bytes_between_segments() {
        // Every byte in the source must belong to exactly one
        // segment — cursor_visual_pos relies on this.
        let s = "alpha beta gamma delta epsilon zeta";
        let lines = wrap_description(s, 10);
        let covered: usize = lines.iter().map(|v| v.byte_end - v.byte_start).sum();
        assert_eq!(covered, s.len());
        let mut prev_end = 0;
        for l in &lines {
            assert_eq!(
                l.byte_start, prev_end,
                "gap between segments at {}",
                l.byte_start
            );
            prev_end = l.byte_end;
        }
    }

    #[test]
    fn wrap_description_preserves_logical_line_boundaries() {
        let s = "hello world\nfoo bar";
        let lines = wrap_description(s, 100);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].logical_line, 0);
        assert_eq!(lines[1].logical_line, 1);
    }

    #[test]
    fn cursor_visual_pos_at_end_of_wrap_continues_on_next_segment() {
        let s = "abcdefgh";
        let lines = wrap_description(s, 4);
        // Cursor at byte offset 4 is the boundary — should report
        // the *next* segment so typing continues there.
        let (v_idx, col) = cursor_visual_pos(s, &lines, 4);
        assert_eq!(v_idx, 1);
        assert_eq!(col, 0);
    }

    #[test]
    fn description_insert_and_backspace_maintain_cursor() {
        let mut form = TaskForm::new_add();
        form.description_insert('a');
        form.description_insert('b');
        form.description_insert('\n');
        form.description_insert('c');
        assert_eq!(form.description, "ab\nc");
        assert_eq!(form.description_cursor, 4);
        form.description_backspace();
        assert_eq!(form.description, "ab\n");
        assert_eq!(form.description_cursor, 3);
        form.description_backspace();
        assert_eq!(form.description, "ab");
    }

    #[test]
    fn description_move_line_uses_wrap_width_when_set() {
        // A single logical line wider than the wrap width; Up/Down
        // between visual rows should work without leaving the field.
        let mut form = TaskForm::new_add();
        form.description = "abcdefghij".into();
        form.description_cursor = 0;
        form.last_wrap_width = 4;
        // Down: cursor moves to visual row 1 (offset 4).
        assert!(form.description_move_line(1));
        assert_eq!(form.description_cursor, 4);
        // Down again: visual row 2 (offset 8).
        assert!(form.description_move_line(1));
        assert_eq!(form.description_cursor, 8);
        // Down again: no more visual rows → false.
        assert!(!form.description_move_line(1));
    }

    #[test]
    fn description_move_line_returns_false_when_past_edges() {
        let mut form = TaskForm::new_add();
        form.description = "one\ntwo".into();
        form.description_cursor = 0;
        assert!(!form.description_move_line(-1), "above top must tab off");
        form.description_cursor = form.description.len();
        assert!(!form.description_move_line(1), "below bottom must tab off");
        // Middle row works.
        form.description_cursor = 0;
        assert!(form.description_move_line(1));
    }

    #[test]
    fn description_ensure_visible_adjusts_scroll() {
        let mut form = TaskForm::new_add();
        form.description = "a\nb\nc\nd\ne\nf\ng\nh\n".into();
        form.description_cursor = form.description.len();
        form.description_ensure_visible(3);
        // Cursor sits on the last (empty) line; scroll should push
        // it into view.
        let (cursor_line, _) = line_col_from_offset(&form.description, form.description_cursor);
        assert!(cursor_line >= form.description_scroll);
        assert!(cursor_line < form.description_scroll + 3);
    }

    #[test]
    fn blocked_by_picker_filters_to_active_tasks() {
        let mut s = TaskStore::new();
        let a = s
            .add("a".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;
        let b = s
            .add("b".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;
        // Mark `b` as Done — it should NOT appear as an add
        // candidate.
        s.update(
            b,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
            "x",
        )
        .unwrap();
        let c = s
            .add("c".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;

        let mut form = TaskForm::new_add();
        form.open_blocked_by_picker(s.all(), None);
        let ids: Vec<Uuid> = form
            .blocked_by_picker
            .as_ref()
            .unwrap()
            .options
            .iter()
            .map(|r| r.id)
            .collect();
        assert!(ids.contains(&a));
        assert!(!ids.contains(&b), "Done task must not be an add candidate");
        assert!(ids.contains(&c));
    }

    #[test]
    fn blocked_by_picker_keeps_already_blocking_done_tasks_so_user_can_remove() {
        let mut s = TaskStore::new();
        let a = s
            .add("a".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;
        s.update(
            a,
            TaskPatch {
                status: Some(TaskStatus::Done),
                ..Default::default()
            },
            "x",
        )
        .unwrap();
        let b = s
            .add("b".into(), None, Some("x".into()), "x".into(), vec![a])
            .unwrap()
            .id;
        let mut form = TaskForm::new_edit(s.get(b).unwrap());
        form.open_blocked_by_picker(s.all(), Some(b));
        let p = form.blocked_by_picker.as_ref().unwrap();
        // `a` (Done) must be present AND checked.
        let idx = p.options.iter().position(|r| r.id == a).unwrap();
        assert!(p.checked[idx]);
    }

    #[test]
    fn blocked_by_picker_excludes_self_task() {
        let mut s = TaskStore::new();
        let a = s
            .add("a".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;
        let mut form = TaskForm::new_edit(s.get(a).unwrap());
        form.open_blocked_by_picker(s.all(), Some(a));
        assert!(form
            .blocked_by_picker
            .as_ref()
            .unwrap()
            .options
            .iter()
            .all(|r| r.id != a));
    }

    #[test]
    fn blocked_by_picker_starts_with_scroll_at_zero() {
        let mut s = TaskStore::new();
        for i in 0..10 {
            s.add(format!("t{i}"), None, None, "x".into(), Vec::new())
                .unwrap();
        }
        let mut form = TaskForm::new_add();
        form.open_blocked_by_picker(s.all(), None);
        assert_eq!(form.blocked_by_picker.as_ref().unwrap().scroll, 0);
    }

    #[test]
    fn blocked_by_picker_toggle_and_commit_updates_form_list() {
        let mut s = TaskStore::new();
        let a = s
            .add("a".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;
        let mut form = TaskForm::new_add();
        form.open_blocked_by_picker(s.all(), None);
        // Move to `a` and check it.
        form.blocked_by_picker_toggle();
        form.commit_blocked_by_picker();
        assert_eq!(form.blocked_by, vec![a]);
        assert!(form.blocked_by_picker.is_none());
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
                description_cursor: 0,
                description_scroll: 0,
                last_wrap_width: 0,
                last_visible_lines: DESCRIPTION_MIN_VISIBLE_LINES,
                assignee: "coder".into(),
                status: "".into(),
                blocked_by: Vec::new(),
                original_blocked_by: Vec::new(),
                focused_field: FormField::Title,
                error: None,
                assignee_options: vec!["".into(), "coder".into()],
                picker: None,
                blocked_by_picker: None,
                _button_sink: String::new(),
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
        form.blocked_by = vec![prereq];
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
    fn form_tab_order_includes_ok_and_cancel_at_the_end() {
        let add = TaskForm::new_add();
        // Add: Title, Description, Assignee, BlockedByCsv, Ok, Cancel.
        assert_eq!(add.tab_order().len(), 6);
        assert_eq!(add.tab_order()[4], FormField::Ok);
        assert_eq!(add.tab_order()[5], FormField::Cancel);

        let mut s = TaskStore::new();
        let id = s
            .add("t".into(), None, None, "a".into(), Vec::new())
            .unwrap()
            .id;
        let edit = TaskForm::new_edit(s.get(id).unwrap());
        // Edit: Title, Description, Assignee, Status, BlockedBy, Ok, Cancel.
        assert_eq!(edit.tab_order().len(), 7);
        assert_eq!(edit.tab_order()[5], FormField::Ok);
        assert_eq!(edit.tab_order()[6], FormField::Cancel);
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
            todos: vec![],
            comments: vec![],
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
        assert!(!TaskForm::is_dropdown(FormField::BlockedBy));
    }

    #[test]
    fn focus_next_walks_all_controls_including_buttons() {
        let mut form = TaskForm::new_add();
        let sequence = [
            FormField::Title,
            FormField::Description,
            FormField::Assignee,
            FormField::BlockedBy,
            FormField::Ok,
            FormField::Cancel,
            FormField::Title, // wraps
        ];
        for expected in sequence {
            assert_eq!(form.focused_field, expected);
            form.focus_next();
        }
    }
}
