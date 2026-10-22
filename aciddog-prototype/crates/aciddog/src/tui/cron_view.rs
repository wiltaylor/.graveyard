//! State and pure logic for the in-TUI Cron view.
//!
//! Lets the human operator inspect and manage the team's cron jobs
//! alongside the CLI (`aciddog cron add|list|...`). Reads / writes
//! [`crate::cron::CronStore`] directly — no IPC hop — and pokes
//! `cron_invalidate` after every mutation so the scheduler wakes up
//! and re-computes `next_due_at()`.

use uuid::Uuid;

use crate::cron::{CronJob, CronPayload, CronSchedule, CronStore, CronTz};

/// Clickable regions the Cron view publishes each frame. The mouse
/// handler picks the first rect whose bounds contain the click —
/// modal overlays are drawn last so their hotspots win over list
/// rows beneath them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronHotspot {
    /// Toolbar: "+ new" button.
    NewButton,
    /// Any part of a row (excluding explicit sub-hotspots below):
    /// move the cursor to this row.
    SelectRow(usize),
    /// The `●/○` enable indicator at the row's left edge.
    ToggleEnabled(usize),
    /// The row's inline `[edit]` button.
    EditRow(usize),
    /// The row's inline `[del]` button.
    DeleteRow(usize),
    /// A row in the add/edit modal — clicking focuses that field (and
    /// for toggleable fields, also cycles the value).
    FormField(CronFormField),
    FormSave,
    FormCancel,
    /// Confirm-delete modal buttons.
    ConfirmDeleteYes,
    ConfirmDeleteNo,
}

/// Top-level mode of the view. `List` is the default; the other
/// variants are modal overlays.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum CronMode {
    #[default]
    List,
    Form(CronForm),
    ConfirmDelete {
        job_id: Uuid,
        display: String,
    },
}

#[derive(Debug, Default)]
pub struct CronScreen {
    /// Cursor into the displayed job list.
    pub cursor: usize,
    pub mode: CronMode,
    /// Transient status toast rendered beneath the list. Cleared on
    /// the next keypress.
    pub status_msg: Option<String>,
}

impl CronScreen {
    pub fn reclamp(&mut self, len: usize) {
        if len == 0 {
            self.cursor = 0;
        } else if self.cursor >= len {
            self.cursor = len - 1;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronFormKind {
    Add,
    Edit(Uuid),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronPayloadKind {
    Message,
    SlashCommand,
}

impl CronPayloadKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::SlashCommand => "command",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Message => Self::SlashCommand,
            Self::SlashCommand => Self::Message,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronScheduleKind {
    Recurring,
    OneOff,
}

impl CronScheduleKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Recurring => "recurring",
            Self::OneOff => "one-off",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Recurring => Self::OneOff,
            Self::OneOff => Self::Recurring,
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::Recurring => "5-field cron (`min hour dom mon dow`, e.g. `*/5 * * * *`)",
            Self::OneOff => "ISO wall-clock `YYYY-MM-DDTHH:MM[:SS]`",
        }
    }
}

/// Tab-ordered list of focusable fields on the form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronFormField {
    Target,
    PayloadKind,
    PayloadBody,
    ScheduleKind,
    ScheduleText,
    Timezone,
    CatchupOnStartup,
    Enabled,
    SaveButton,
    CancelButton,
}

impl CronFormField {
    pub const fn all() -> &'static [CronFormField] {
        &[
            CronFormField::Target,
            CronFormField::PayloadKind,
            CronFormField::PayloadBody,
            CronFormField::ScheduleKind,
            CronFormField::ScheduleText,
            CronFormField::Timezone,
            CronFormField::CatchupOnStartup,
            CronFormField::Enabled,
            CronFormField::SaveButton,
            CronFormField::CancelButton,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Target => "Target",
            Self::PayloadKind => "Payload kind",
            Self::PayloadBody => "Payload",
            Self::ScheduleKind => "Schedule kind",
            Self::ScheduleText => "Schedule",
            Self::Timezone => "Timezone",
            Self::CatchupOnStartup => "Catchup on startup",
            Self::Enabled => "Enabled",
            Self::SaveButton => "[ save ]",
            Self::CancelButton => "[ cancel ]",
        }
    }

    pub fn is_text(self) -> bool {
        matches!(self, Self::Target | Self::PayloadBody | Self::ScheduleText)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronForm {
    pub kind: CronFormKind,
    pub target: String,
    pub payload_kind: CronPayloadKind,
    pub payload_body: String,
    pub schedule_kind: CronScheduleKind,
    pub schedule_text: String,
    pub timezone: CronTz,
    pub catchup_on_startup: bool,
    pub enabled: bool,
    pub focused: CronFormField,
    pub error: Option<String>,
}

impl CronForm {
    pub fn new_add() -> Self {
        Self {
            kind: CronFormKind::Add,
            target: String::new(),
            payload_kind: CronPayloadKind::Message,
            payload_body: String::new(),
            schedule_kind: CronScheduleKind::Recurring,
            schedule_text: String::new(),
            timezone: CronTz::Local,
            catchup_on_startup: false,
            enabled: true,
            focused: CronFormField::Target,
            error: None,
        }
    }

    pub fn new_edit(job: &CronJob) -> Self {
        let (payload_kind, payload_body) = match &job.payload {
            CronPayload::Message { body } => (CronPayloadKind::Message, body.clone()),
            CronPayload::SlashCommand { command } => {
                (CronPayloadKind::SlashCommand, command.clone())
            }
        };
        let (schedule_kind, schedule_text) = match &job.schedule {
            CronSchedule::Recurring { expression } => {
                (CronScheduleKind::Recurring, expression.clone())
            }
            CronSchedule::OneOff { run_at_local } => (
                CronScheduleKind::OneOff,
                run_at_local.format("%Y-%m-%dT%H:%M:%S").to_string(),
            ),
        };
        Self {
            kind: CronFormKind::Edit(job.id),
            target: job.target_agent.clone(),
            payload_kind,
            payload_body,
            schedule_kind,
            schedule_text,
            timezone: job.timezone,
            catchup_on_startup: job.catchup_on_startup,
            enabled: job.enabled,
            focused: CronFormField::Target,
            error: None,
        }
    }

    /// Cycle forward / backward through [`CronFormField::all`].
    pub fn cycle_focus(&mut self, forward: bool) {
        let fields = CronFormField::all();
        let cur = fields.iter().position(|f| *f == self.focused).unwrap_or(0);
        let next = if forward {
            (cur + 1) % fields.len()
        } else {
            (cur + fields.len() - 1) % fields.len()
        };
        self.focused = fields[next];
    }
}

/// Short one-line summary for the list. Separate from
/// `fmt_cron_job_short` in `tui/app.rs` because that format targets
/// CLI output; this one is tuned for a row in the TUI table.
pub fn fmt_cron_row(j: &CronJob) -> String {
    let enabled = if j.enabled { "●" } else { "○" };
    let kind = match &j.payload {
        CronPayload::Message { .. } => "msg",
        CronPayload::SlashCommand { .. } => "cmd",
    };
    let sched = match &j.schedule {
        CronSchedule::Recurring { expression } => format!("every `{expression}`"),
        CronSchedule::OneOff { run_at_local } => format!("at {run_at_local}"),
    };
    let next = j
        .next_run
        .map(|n| n.format("%Y-%m-%d %H:%M:%SZ").to_string())
        .unwrap_or_else(|| "—".to_string());
    format!(
        "{enabled} {kind} → {target}   {sched}   next={next}   tz={tz}",
        target = j.target_agent,
        tz = j.timezone.as_str(),
    )
}

/// Snapshot of the cron store as an owned, ordered list of jobs.
pub fn listed_jobs(store: &CronStore) -> Vec<CronJob> {
    let filter = crate::cron::CronFilter {
        target: None,
        include_disabled: true,
        created_by: None,
    };
    store.list(&filter).into_iter().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reclamp_holds_cursor_in_bounds() {
        let mut s = CronScreen {
            cursor: 5,
            ..Default::default()
        };
        s.reclamp(3);
        assert_eq!(s.cursor, 2);
        s.reclamp(0);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn form_cycle_focus_is_circular() {
        let mut f = CronForm::new_add();
        assert_eq!(f.focused, CronFormField::Target);
        for _ in 0..CronFormField::all().len() {
            f.cycle_focus(true);
        }
        assert_eq!(f.focused, CronFormField::Target);
        f.cycle_focus(false);
        assert_eq!(f.focused, CronFormField::CancelButton);
    }
}
