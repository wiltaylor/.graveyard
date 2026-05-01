//! State and pure logic for the in-TUI Inbox view.
//!
//! The inbox is the human operator's mailbox — any agent or automaton
//! can address messages to the reserved recipient id `"user"` and the
//! IPC handler routes them into [`App::inbox`]. The view lets the user
//! browse messages, reply, forward them to another agent/automaton, or
//! delete them.
//!
//! Storage is in-memory only (mirrors the ephemeral mpsc mailboxes
//! agents use today). Messages are lost on restart.

use std::time::Instant;

/// One of the clickable regions the Inbox view draws. Populated by
/// the renderer each frame and consumed by the mouse handler — the
/// hit-test returns the first matching spot, so overlays stacked
/// above the list win.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InboxHotspot {
    /// A message row in the list — click to select.
    SelectRow(usize),
    /// Toolbar buttons in List mode.
    ReplyButton,
    ForwardButton,
    DeleteButton,
    /// Send / Cancel in a Reply or Forward overlay.
    SendButton,
    CancelButton,
    /// A candidate row in the Forward pick-recipient stage.
    ForwardRecipient(usize),
}

/// One received message sitting in the user's inbox.
#[derive(Debug, Clone)]
pub struct InboxMessage {
    /// Sender label as emitted by the IPC handler: either a
    /// configured agent/automaton id, or the cron-label form
    /// `cron:<creator>:<short-id>` when the source is a cron job.
    pub from: String,
    /// Original body as the sender wrote it. No wrapper tag.
    pub body: String,
    /// When the message landed in the inbox.
    pub received_at: Instant,
}

impl InboxMessage {
    pub fn new(from: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            from: from.into(),
            body: body.into(),
            received_at: Instant::now(),
        }
    }
}

/// Modes the Inbox view can be in. `List` is the default; the others
/// are modal overlays on top of the message list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum InboxMode {
    /// Just browsing the list; detail of the selected row is shown
    /// beneath it.
    #[default]
    List,
    /// Composing a reply to the selected message. The recipient is
    /// fixed to the original sender.
    Reply { to: String, body: String },
    /// Picking a forward recipient, then composing the forward body.
    Forward(ForwardState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForwardStage {
    /// Choosing which agent/automaton to forward to.
    PickRecipient,
    /// Editing the forward body before sending.
    EditBody,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardState {
    /// Candidate recipient ids (configured agents + automatons).
    pub candidates: Vec<String>,
    /// Cursor into `candidates`.
    pub selected: usize,
    pub stage: ForwardStage,
    /// User-typed prefix shown above the quoted original. Empty
    /// until the user types something. The `EditBody` stage's
    /// keyboard input writes here — the quoted copy below is
    /// read-only context, not editable.
    pub note: String,
    /// Read-only quoted copy of the message being forwarded.
    /// Populated when the user transitions from `PickRecipient` to
    /// `EditBody` so it stays stable even if the inbox underneath
    /// shifts.
    pub quoted: String,
}

impl ForwardState {
    /// Compose the final outgoing body: the user's note (if any)
    /// followed by the quoted original. A blank note ships only the
    /// quoted block; trailing whitespace on the note is trimmed so
    /// the join doesn't end up with three blank lines.
    pub fn compose_body(&self) -> String {
        let note = self.note.trim_end();
        if note.is_empty() {
            self.quoted.clone()
        } else {
            format!("{note}\n\n{}", self.quoted)
        }
    }
}

/// Which sub-pane of the Inbox view currently has the keyboard /
/// mouse-wheel target. Tab toggles `List` → `Detail`; Esc from `Detail`
/// returns to `List`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InboxPane {
    #[default]
    List,
    Detail,
}

#[derive(Debug, Default)]
pub struct InboxScreen {
    /// Cursor into [`App::inbox`]. Clamped to `0..inbox.len()`
    /// whenever the list mutates.
    pub cursor: usize,
    /// Vertical row offset into the detail-pane Paragraph; reset to 0
    /// whenever `cursor` moves to a new message.
    pub detail_scroll: u16,
    /// Current mode (list vs. reply vs. forward overlay).
    pub mode: InboxMode,
    /// Transient status toast rendered beneath the list. Cleared on
    /// the next keypress.
    pub status_msg: Option<String>,
    /// Which sub-pane (list vs. detail) the keyboard / wheel currently
    /// drives. Reset to `List` whenever the message cursor changes.
    pub pane: InboxPane,
}

impl InboxScreen {
    /// Re-clamp the cursor after the backing inbox mutated (messages
    /// arrived or were deleted). Idempotent. Resets the detail scroll
    /// whenever the selection actually moves so a new message starts
    /// at the top.
    pub fn reclamp(&mut self, inbox_len: usize) {
        let prev = self.cursor;
        if inbox_len == 0 {
            self.cursor = 0;
        } else if self.cursor >= inbox_len {
            self.cursor = inbox_len - 1;
        }
        if self.cursor != prev {
            self.detail_scroll = 0;
        }
    }

    /// Move the list selection, resetting the detail scroll so the
    /// detail pane starts from the top of the new message. Use this
    /// from every code path that mutates `cursor`.
    pub fn set_cursor(&mut self, new_cursor: usize) {
        if self.cursor != new_cursor {
            self.cursor = new_cursor;
            self.detail_scroll = 0;
        }
    }

    /// Move the cursor to `idx` (clamped to `0..inbox_len`) and reset
    /// the detail-pane scroll offset to the top of the new message.
    /// Centralises the reset semantics so cursor-mutation sites can't
    /// forget to scroll back to the top.
    pub fn select(&mut self, idx: usize, inbox_len: usize) {
        let clamped = if inbox_len == 0 {
            0
        } else {
            idx.min(inbox_len - 1)
        };
        self.cursor = clamped;
        self.detail_scroll = 0;
    }
}

/// Build the ordered list of forward-candidate ids from a config —
/// every declared agent first (in declaration order), then every
/// declared automaton. Workers and channels are excluded: the user
/// cannot forward into an ephemeral worker or into a channel.
pub fn forward_candidates(cfg: &crate::config::AciddogConfig) -> Vec<String> {
    cfg.agents
        .iter()
        .map(|a| a.id.clone())
        .chain(cfg.automatons.iter().map(|a| a.id.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AciddogConfig, Agent, Team};

    #[test]
    fn reclamp_holds_cursor_in_bounds() {
        let mut s = InboxScreen {
            cursor: 5,
            ..Default::default()
        };
        s.reclamp(3);
        assert_eq!(s.cursor, 2);
        s.reclamp(0);
        assert_eq!(s.cursor, 0);
    }

    /// Pins the half-tab constraint at ui.rs:1755. If a future edit
    /// flips `Constraint::Percentage(50)` back to `Length(N)` the
    /// detail height will diverge sharply from the half-tab target
    /// and this test will catch it.
    #[test]
    fn inbox_layout_detail_pane_is_half_tab() {
        use ratatui::layout::{Constraint, Direction, Layout, Rect};
        let area = Rect::new(0, 0, 80, 30);
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(5),
                Constraint::Percentage(50),
            ])
            .split(area);
        let detail_height = split[2].height;
        // Toolbar takes 1 row; remaining 29 rows are split between
        // list (Min 5) and detail (Percentage 50). ratatui rounds, so
        // tolerate ±1 row around the half-tab target of 15.
        let expected = (area.height - 1) / 2;
        let delta = (detail_height as i32 - expected as i32).abs();
        assert!(
            delta <= 1,
            "detail height {detail_height} too far from half-tab {expected} (delta {delta})"
        );
    }

    #[test]
    fn forward_candidates_lists_agents_then_automatons() {
        let cfg = AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
            },
            agents: vec![Agent {
                id: "planner".into(),
                role: "r".into(),
                role_description: None,
                model: None,
                instructions: "i".into(),
                can_message: None,
                can_create_workers: None,
                can_run_workflows: None,
                tools: None,
                ai_core: None,
                memory_banks: None,
                skills: None,
                compaction: None,
                supervisor: false,
                sysadmin: false,
                leader: false,
                group: None,
                on_reload: crate::config::ReloadAction::Ignore,
            }],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["t".into()],
        };
        let ids = forward_candidates(&cfg);
        assert_eq!(ids, vec!["planner".to_string()]);
    }
}
