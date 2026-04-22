//! State and pure logic for the in-TUI Memory view.
//!
//! Two stages:
//! - [`MemoryStage::Banks`]: list of team memory banks; Enter opens one.
//! - [`MemoryStage::Bank`]: a search + result list against a single
//!   bank, with modals for viewing, creating, editing, deleting, and
//!   linking notes.
//!
//! All mutations of the underlying [`MemoryStore`] are async (the embedder
//! is blocking, so ops are dispatched through `spawn_blocking` and replies
//! land via [`MemoryEvent`]). The screen state is correlation-aware: each
//! dispatched op carries an incrementing `op_token` and the app ignores
//! any reply whose token doesn't match the *current* pending op.

use uuid::Uuid;

use crate::memory::{Hit, Link};

use super::tasks_view::{
    cursor_visual_pos, offset_at_col_in_segment, segment_char_count, utf8_char_len,
    utf8_char_len_prev, wrap_description,
};

/// Default result cap when the user hasn't overridden it.
pub const DEFAULT_LIMIT: usize = 25;

/// Minimum visible rows inside the Body sub-block of the note form
/// when the terminal is cramped. Mirrors `DESCRIPTION_MIN_VISIBLE_LINES`.
pub const BODY_MIN_VISIBLE_LINES: u16 = 6;

#[derive(Debug, Default)]
pub struct MemoryScreen {
    pub stage: MemoryStage,
    /// Transient error/success toast shown beneath the main area.
    /// Cleared on next keypress.
    pub status_msg: Option<String>,
    /// Monotonic counter bumped on every new op dispatch. The
    /// corresponding `MemoryEvent` carries the same token; stale
    /// replies (arriving after the user moved on) are filtered out.
    pub op_counter: u64,
    /// Human-readable label of the in-flight op, rendered as a
    /// spinner-adjacent hint. `None` when idle.
    pub pending: Option<String>,
}

#[derive(Debug)]
pub enum MemoryStage {
    Banks { selected: usize },
    Bank(BankStage),
}

impl Default for MemoryStage {
    fn default() -> Self {
        MemoryStage::Banks { selected: 0 }
    }
}

#[derive(Debug, Default)]
pub struct BankStage {
    pub bank_id: String,
    pub search: SearchInput,
    pub results: Vec<Hit>,
    pub selected_result: usize,
    /// Which pane owns the cursor/typing: the search bar or the
    /// results list. Tab toggles between them.
    pub focus: BankFocus,
    pub form: Option<NoteForm>,
    pub detail: Option<NoteDetail>,
    pub links_overlay: Option<LinksOverlay>,
    pub confirm_delete: Option<Uuid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BankFocus {
    #[default]
    Search,
    Results,
}

#[derive(Debug, Clone, Default)]
pub struct SearchInput {
    pub query: String,
    /// Byte offset of the edit cursor inside `query`.
    pub cursor: usize,
    pub limit: usize,
}

impl SearchInput {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor: 0,
            limit: DEFAULT_LIMIT,
        }
    }

    pub fn insert(&mut self, c: char) {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        self.query.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let n = utf8_char_len_prev(&self.query, self.cursor);
        if n == 0 {
            return;
        }
        let start = self.cursor - n;
        self.query.drain(start..self.cursor);
        self.cursor = start;
    }

    pub fn move_char(&mut self, delta: i32) {
        if delta > 0 {
            let n = utf8_char_len(&self.query, self.cursor);
            self.cursor = (self.cursor + n).min(self.query.len());
        } else if delta < 0 {
            let n = utf8_char_len_prev(&self.query, self.cursor);
            self.cursor = self.cursor.saturating_sub(n);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteFormKind {
    Add,
    Edit(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteField {
    Title,
    Body,
    Tags,
    Ok,
    Cancel,
}

impl NoteField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Title => "Title",
            Self::Body => "Body",
            Self::Tags => "Tags (comma separated)",
            Self::Ok => "OK",
            Self::Cancel => "Cancel",
        }
    }

    pub fn is_button(self) -> bool {
        matches!(self, Self::Ok | Self::Cancel)
    }
}

pub const NOTE_FORM_TAB_ORDER: [NoteField; 5] = [
    NoteField::Title,
    NoteField::Body,
    NoteField::Tags,
    NoteField::Ok,
    NoteField::Cancel,
];

#[derive(Debug, Clone)]
pub struct NoteForm {
    pub kind: NoteFormKind,
    pub title: String,
    pub title_cursor: usize,
    pub body: String,
    pub body_cursor: usize,
    pub body_scroll: u16,
    /// Wrap width the Body box was last rendered at. Key handlers
    /// use it to keep cursor-move math consistent with the rendered
    /// output.
    pub body_last_wrap_width: u16,
    pub body_last_visible_lines: u16,
    /// Comma-separated tags (same edit model as a single-line text
    /// field). Normalised into `Vec<String>` on submit.
    pub tags: String,
    pub tags_cursor: usize,
    pub focused_field: NoteField,
    pub error: Option<String>,
    #[doc(hidden)]
    pub _button_sink: String,
}

impl NoteForm {
    pub fn new_add() -> Self {
        Self {
            kind: NoteFormKind::Add,
            title: String::new(),
            title_cursor: 0,
            body: String::new(),
            body_cursor: 0,
            body_scroll: 0,
            body_last_wrap_width: 0,
            body_last_visible_lines: BODY_MIN_VISIBLE_LINES,
            tags: String::new(),
            tags_cursor: 0,
            focused_field: NoteField::Title,
            error: None,
            _button_sink: String::new(),
        }
    }

    pub fn new_edit(hit: &Hit) -> Self {
        let tags_string = hit.tags.join(", ");
        let tags_cursor = tags_string.len();
        let title_cursor = hit.title.len();
        let body_cursor = hit.body.len();
        Self {
            kind: NoteFormKind::Edit(hit.id),
            title: hit.title.clone(),
            title_cursor,
            body: hit.body.clone(),
            body_cursor,
            body_scroll: 0,
            body_last_wrap_width: 0,
            body_last_visible_lines: BODY_MIN_VISIBLE_LINES,
            tags: tags_string,
            tags_cursor,
            focused_field: NoteField::Title,
            error: None,
            _button_sink: String::new(),
        }
    }

    pub fn focus_next(&mut self) {
        if let Some(pos) = NOTE_FORM_TAB_ORDER
            .iter()
            .position(|f| *f == self.focused_field)
        {
            self.focused_field = NOTE_FORM_TAB_ORDER[(pos + 1) % NOTE_FORM_TAB_ORDER.len()];
        }
    }

    pub fn focus_prev(&mut self) {
        if let Some(pos) = NOTE_FORM_TAB_ORDER
            .iter()
            .position(|f| *f == self.focused_field)
        {
            self.focused_field = NOTE_FORM_TAB_ORDER
                [(pos + NOTE_FORM_TAB_ORDER.len() - 1) % NOTE_FORM_TAB_ORDER.len()];
        }
    }

    pub fn field_mut(&mut self, field: NoteField) -> &mut String {
        match field {
            NoteField::Title => &mut self.title,
            NoteField::Body => &mut self.body,
            NoteField::Tags => &mut self.tags,
            NoteField::Ok | NoteField::Cancel => &mut self._button_sink,
        }
    }

    pub fn cursor_mut(&mut self, field: NoteField) -> Option<&mut usize> {
        match field {
            NoteField::Title => Some(&mut self.title_cursor),
            NoteField::Body => Some(&mut self.body_cursor),
            NoteField::Tags => Some(&mut self.tags_cursor),
            NoteField::Ok | NoteField::Cancel => None,
        }
    }

    /// Insert `c` into the focused text field, advancing the field's
    /// cursor. No-op for button fields.
    pub fn insert_char(&mut self, c: char) {
        let field = self.focused_field;
        if field.is_button() {
            return;
        }
        let cursor_val = match field {
            NoteField::Title => self.title_cursor,
            NoteField::Body => self.body_cursor,
            NoteField::Tags => self.tags_cursor,
            _ => return,
        };
        let buf = self.field_mut(field);
        let mut tmp = [0u8; 4];
        let s = c.encode_utf8(&mut tmp);
        buf.insert_str(cursor_val, s);
        let new_cursor = cursor_val + s.len();
        if let Some(cur) = self.cursor_mut(field) {
            *cur = new_cursor;
        }
    }

    pub fn backspace(&mut self) {
        let field = self.focused_field;
        if field.is_button() {
            return;
        }
        let cursor_val = match field {
            NoteField::Title => self.title_cursor,
            NoteField::Body => self.body_cursor,
            NoteField::Tags => self.tags_cursor,
            _ => return,
        };
        if cursor_val == 0 {
            return;
        }
        // Borrow the field string long enough to compute the UTF-8 char
        // width at the cursor, then do the drain in a second step.
        let n = {
            let s: &str = match field {
                NoteField::Title => &self.title,
                NoteField::Body => &self.body,
                NoteField::Tags => &self.tags,
                _ => "",
            };
            utf8_char_len_prev(s, cursor_val)
        };
        if n == 0 {
            return;
        }
        let start = cursor_val - n;
        let buf = self.field_mut(field);
        buf.drain(start..cursor_val);
        if let Some(cur) = self.cursor_mut(field) {
            *cur = start;
        }
    }

    /// Move cursor left/right by one grapheme in the focused field.
    pub fn move_char(&mut self, delta: i32) {
        let field = self.focused_field;
        if field.is_button() {
            return;
        }
        let (len, cursor_val) = {
            let s: &str = match field {
                NoteField::Title => &self.title,
                NoteField::Body => &self.body,
                NoteField::Tags => &self.tags,
                _ => return,
            };
            let c = match field {
                NoteField::Title => self.title_cursor,
                NoteField::Body => self.body_cursor,
                NoteField::Tags => self.tags_cursor,
                _ => return,
            };
            (s.len(), c)
        };
        let next = if delta > 0 {
            let n = {
                let s: &str = match field {
                    NoteField::Title => &self.title,
                    NoteField::Body => &self.body,
                    NoteField::Tags => &self.tags,
                    _ => return,
                };
                utf8_char_len(s, cursor_val)
            };
            (cursor_val + n).min(len)
        } else if delta < 0 {
            let n = {
                let s: &str = match field {
                    NoteField::Title => &self.title,
                    NoteField::Body => &self.body,
                    NoteField::Tags => &self.tags,
                    _ => return,
                };
                utf8_char_len_prev(s, cursor_val)
            };
            cursor_val.saturating_sub(n)
        } else {
            cursor_val
        };
        if let Some(cur) = self.cursor_mut(field) {
            *cur = next;
        }
    }

    /// Move the body cursor up/down one visual line (wrap-aware when
    /// `body_last_wrap_width` > 0). Returns `false` when the move
    /// would leave the field — the caller may then focus_next / prev.
    pub fn body_move_line(&mut self, delta: i32) -> bool {
        let lines = wrap_description(&self.body, self.body_last_wrap_width);
        let (v_idx, col_in_seg) = cursor_visual_pos(&self.body, &lines, self.body_cursor);
        if delta < 0 {
            if v_idx == 0 {
                return false;
            }
            let target = &lines[v_idx - 1];
            let target_len = segment_char_count(&self.body, target);
            let col = col_in_seg.min(target_len);
            self.body_cursor = offset_at_col_in_segment(&self.body, target, col);
            true
        } else if delta > 0 {
            if v_idx + 1 >= lines.len() {
                return false;
            }
            let target = &lines[v_idx + 1];
            let target_len = segment_char_count(&self.body, target);
            let col = col_in_seg.min(target_len);
            self.body_cursor = offset_at_col_in_segment(&self.body, target, col);
            true
        } else {
            true
        }
    }

    pub fn body_ensure_visible(&mut self, visible_height: u16) {
        if visible_height == 0 {
            return;
        }
        let lines = wrap_description(&self.body, self.body_last_wrap_width);
        let (v_idx_usize, _) = cursor_visual_pos(&self.body, &lines, self.body_cursor);
        let v_idx = v_idx_usize as u16;
        if v_idx < self.body_scroll {
            self.body_scroll = v_idx;
        } else if v_idx >= self.body_scroll + visible_height {
            self.body_scroll = v_idx - visible_height + 1;
        }
    }

    /// Parse the tags field into a normalised `Vec<String>`. Tags are
    /// split on commas; empty tokens are dropped. Case/dedup is
    /// handled server-side by `normalise_tag`, so this just splits.
    pub fn parsed_tags(&self) -> Vec<String> {
        self.tags
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// Validate the form. Returns `Ok(())` and leaves `self.error`
    /// alone on success; on failure, sets the error and returns `Err`
    /// with the same message.
    pub fn validate(&mut self) -> Result<(), String> {
        let body = self.body.trim();
        if body.is_empty() {
            let msg = "body cannot be empty".to_string();
            self.error = Some(msg.clone());
            return Err(msg);
        }
        self.error = None;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct NoteDetail {
    pub id: Uuid,
    /// Loaded async: `None` while the spawned lookup is in flight.
    pub entry: Option<Hit>,
    pub out_links: Vec<Link>,
    pub in_links: Vec<Link>,
    pub scroll: u16,
}

impl NoteDetail {
    pub fn new(id: Uuid) -> Self {
        Self {
            id,
            entry: None,
            out_links: Vec::new(),
            in_links: Vec::new(),
            scroll: 0,
        }
    }
}

/// Overlay for editing the outgoing links on a single note. Shows
/// existing edges with kind + peer title; `+` opens the add-link
/// picker, `-` unlinks the cursor row.
#[derive(Debug, Clone)]
pub struct LinksOverlay {
    pub source_id: Uuid,
    pub existing: Vec<Link>,
    pub cursor: usize,
    pub picker: Option<LinkPicker>,
}

impl LinksOverlay {
    pub fn new(source_id: Uuid, existing: Vec<Link>) -> Self {
        Self {
            source_id,
            existing,
            cursor: 0,
            picker: None,
        }
    }

    pub fn move_cursor(&mut self, delta: i32) {
        if self.existing.is_empty() {
            self.cursor = 0;
            return;
        }
        let len = self.existing.len() as i32;
        self.cursor = ((self.cursor as i32 + delta).rem_euclid(len)) as usize;
    }

    pub fn selected(&self) -> Option<&Link> {
        self.existing.get(self.cursor)
    }
}

/// The add-link picker. The user types a search query (semantic over
/// the same bank), picks a target entry, then types an optional
/// `kind` label before committing.
#[derive(Debug, Clone, Default)]
pub struct LinkPicker {
    pub query: String,
    pub query_cursor: usize,
    pub results: Vec<Hit>,
    pub selected: usize,
    pub kind: String,
    pub kind_cursor: usize,
    pub focus: LinkPickerFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkPickerFocus {
    #[default]
    Query,
    Results,
    Kind,
}

/// Replies from spawn_blocking memory ops, delivered on a channel and
/// matched against `MemoryScreen.op_counter` to drop stale hits.
///
/// `token` and `to` on some variants are intentionally kept as part of
/// the payload for future correlation / diagnostics even though the
/// current reducer only gates on `bank_id`.
#[allow(dead_code)]
#[derive(Debug)]
pub enum MemoryEvent {
    SearchDone {
        token: u64,
        bank_id: String,
        result: Result<Vec<Hit>, String>,
    },
    EntryLoaded {
        token: u64,
        bank_id: String,
        id: Uuid,
        result: Result<Option<(Hit, Vec<Link>, Vec<Link>)>, String>,
    },
    StoreDone {
        token: u64,
        bank_id: String,
        result: Result<Uuid, String>,
    },
    UpdateDone {
        token: u64,
        bank_id: String,
        id: Uuid,
        result: Result<(), String>,
    },
    DeleteDone {
        token: u64,
        bank_id: String,
        id: Uuid,
        result: Result<bool, String>,
    },
    LinkDone {
        token: u64,
        bank_id: String,
        from: Uuid,
        to: Uuid,
        result: Result<(), String>,
    },
    UnlinkDone {
        token: u64,
        bank_id: String,
        from: Uuid,
        to: Uuid,
        result: Result<bool, String>,
    },
    LinkSearchDone {
        token: u64,
        bank_id: String,
        result: Result<Vec<Hit>, String>,
    },
}

impl MemoryScreen {
    /// Bump the token and label the in-flight op. Returns the new
    /// token so callers can stamp it on the dispatched task.
    pub fn begin_op(&mut self, label: impl Into<String>) -> u64 {
        self.op_counter = self.op_counter.wrapping_add(1);
        self.pending = Some(label.into());
        self.op_counter
    }

    pub fn clear_pending(&mut self) {
        self.pending = None;
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_msg = Some(msg.into());
    }

    pub fn banks_stage(&mut self) {
        self.stage = MemoryStage::Banks { selected: 0 };
    }

    pub fn enter_bank(&mut self, bank_id: String) {
        self.stage = MemoryStage::Bank(BankStage {
            bank_id,
            search: SearchInput::new(),
            results: Vec::new(),
            selected_result: 0,
            focus: BankFocus::Search,
            form: None,
            detail: None,
            links_overlay: None,
            confirm_delete: None,
        });
    }

    pub fn bank_mut(&mut self) -> Option<&mut BankStage> {
        match &mut self.stage {
            MemoryStage::Bank(b) => Some(b),
            _ => None,
        }
    }

    pub fn bank(&self) -> Option<&BankStage> {
        match &self.stage {
            MemoryStage::Bank(b) => Some(b),
            _ => None,
        }
    }

    /// Close any open overlay on the bank stage (form, detail, links,
    /// confirm). Leaves search + results alone.
    pub fn close_overlays(&mut self) {
        if let Some(b) = self.bank_mut() {
            b.form = None;
            b.detail = None;
            b.links_overlay = None;
            b.confirm_delete = None;
        }
    }

    /// Any modal-ish overlay that should capture Tab / Esc inside the
    /// memory view rather than returning to the view switcher.
    pub fn has_active_modal(&self) -> bool {
        self.bank()
            .map(|b| {
                b.form.is_some()
                    || b.detail.is_some()
                    || b.links_overlay.is_some()
                    || b.confirm_delete.is_some()
            })
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_hit(id: Uuid, title: &str, body: &str, tags: &[&str]) -> Hit {
        Hit {
            id,
            title: title.to_string(),
            body: body.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            created_at: chrono::Utc::now(),
            distance: f32::NAN,
        }
    }

    #[test]
    fn search_input_insert_and_backspace() {
        let mut s = SearchInput::new();
        s.insert('h');
        s.insert('i');
        assert_eq!(s.query, "hi");
        assert_eq!(s.cursor, 2);
        s.backspace();
        assert_eq!(s.query, "h");
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn note_form_new_edit_seeds_tags_from_hit() {
        let hit = sample_hit(Uuid::new_v4(), "t", "body", &["alpha", "beta"]);
        let form = NoteForm::new_edit(&hit);
        assert_eq!(form.tags, "alpha, beta");
        assert_eq!(form.body, "body");
        assert!(matches!(form.kind, NoteFormKind::Edit(_)));
    }

    #[test]
    fn note_form_parsed_tags_splits_and_trims() {
        let mut f = NoteForm::new_add();
        f.tags = "  foo , bar,,baz  ".into();
        assert_eq!(f.parsed_tags(), vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn note_form_validate_rejects_empty_body() {
        let mut f = NoteForm::new_add();
        f.body = "   \n  ".into();
        assert!(f.validate().is_err());
        assert!(f.error.is_some());
    }

    #[test]
    fn tab_order_cycles_forward_and_back() {
        let mut f = NoteForm::new_add();
        assert_eq!(f.focused_field, NoteField::Title);
        f.focus_next();
        assert_eq!(f.focused_field, NoteField::Body);
        f.focus_prev();
        assert_eq!(f.focused_field, NoteField::Title);
        f.focus_prev();
        assert_eq!(f.focused_field, NoteField::Cancel);
    }

    #[test]
    fn memory_screen_begin_op_bumps_token() {
        let mut s = MemoryScreen::default();
        let t0 = s.op_counter;
        let t1 = s.begin_op("searching");
        assert!(t1 > t0);
        assert_eq!(s.pending.as_deref(), Some("searching"));
    }

    #[test]
    fn memory_screen_close_overlays_clears_all() {
        let mut s = MemoryScreen::default();
        s.enter_bank("scratch".into());
        if let Some(b) = s.bank_mut() {
            b.form = Some(NoteForm::new_add());
            b.confirm_delete = Some(Uuid::new_v4());
        }
        assert!(s.has_active_modal());
        s.close_overlays();
        assert!(!s.has_active_modal());
    }
}
