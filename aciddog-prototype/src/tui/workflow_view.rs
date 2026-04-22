//! TUI-side state for the workflows feature: the per-instance view
//! `App` mirrors from `WorkflowEvent`s, the property-entry form, and
//! the supporting enums for step/instance status. The actual rendering
//! lives in `src/tui/ui.rs`.

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::config::{PropertyKind, StepKind, Workflow};
use crate::workflow::{InstanceStatus, PropertyValue, StepStatus};

/// Per-step progress snapshot. Maintained by the TUI from
/// `WorkflowEvent::StepStarted` / `StepFinished` / `Retrying` events.
#[derive(Debug, Clone)]
pub struct StepProgress {
    pub id: String,
    pub kind_label: &'static str,
    pub status: StepStatus,
    /// Number of retry jumps that landed on this step (only ever
    /// increments for steps reached via `Conditional.target` /
    /// `Check.target`).
    pub retry_count: u32,
    /// Last failure reason for this step, if any. Shown inline on the
    /// step row and in the output panel.
    pub reason: Option<String>,
}

impl StepProgress {
    pub fn new(id: String, kind: &StepKind) -> Self {
        Self {
            id,
            kind_label: match kind {
                StepKind::Worker { .. } => "worker",
                StepKind::Command { .. } => "command",
                StepKind::Conditional { .. } => "conditional",
                StepKind::Check { .. } => "check",
            },
            status: StepStatus::Pending,
            retry_count: 0,
            reason: None,
        }
    }
}

/// TUI-side mirror of a running (or failed) workflow instance. Updated
/// in reaction to `WorkflowEvent`s; successful instances are removed
/// on `Finished{ok:true}`.
#[derive(Debug, Clone)]
pub struct WorkflowInstanceView {
    pub id: u64,
    pub workflow_idx: usize,
    #[allow(dead_code)]
    pub workflow_id: String,
    pub status: InstanceStatus,
    pub steps: Vec<StepProgress>,
    pub current_step: usize,
    pub failure_reason: Option<String>,
    /// Tail of recent `StepOutput` lines. Capped to keep redraws cheap.
    pub tail_output: VecDeque<String>,
    /// Live snapshot of every variable's current value. Seeded from
    /// the `Started` event, updated on every `VarChanged`.
    pub vars: HashMap<String, PropertyValue>,
    /// Auto-removal deadline for successfully-completed instances. An
    /// instance that finishes in milliseconds would otherwise never be
    /// visible to the operator; we keep it in the sidebar (with a
    /// green glyph) until this `Instant` is reached, then drop it.
    /// `None` for running or failed instances.
    pub remove_at: Option<Instant>,
}

/// How long a successful instance lingers in the sidebar before being
/// auto-removed. Picked to be long enough that a human reads the row.
pub const SUCCESS_LINGER: std::time::Duration = std::time::Duration::from_secs(4);

/// Upper bound on retained output lines. Older lines are dropped.
pub const TAIL_OUTPUT_CAP: usize = 200;

impl WorkflowInstanceView {
    pub fn new(id: u64, workflow_idx: usize, workflow: &Workflow) -> Self {
        let steps = workflow
            .steps
            .iter()
            .map(|s| StepProgress::new(s.id.clone(), &s.kind))
            .collect();
        Self {
            id,
            workflow_idx,
            workflow_id: workflow.id.clone(),
            status: InstanceStatus::Running,
            steps,
            current_step: 0,
            failure_reason: None,
            tail_output: VecDeque::new(),
            vars: HashMap::new(),
            remove_at: None,
        }
    }

    pub fn push_output(&mut self, line: String) {
        if self.tail_output.len() >= TAIL_OUTPUT_CAP {
            self.tail_output.pop_front();
        }
        self.tail_output.push_back(line);
    }
}

/// Editable form field. Hidden properties are filtered out when the
/// form is constructed, so no `Hidden` variant exists here.
#[derive(Debug, Clone)]
pub enum FieldEditor {
    Text {
        name: String,
        value: String,
        cursor: usize,
    },
    Number {
        name: String,
        value: String,
        cursor: usize,
    },
    Select {
        name: String,
        options: Vec<String>,
        selected: usize,
    },
    MultiSelect {
        name: String,
        options: Vec<String>,
        chosen: Vec<bool>,
    },
}

impl FieldEditor {
    pub fn name(&self) -> &str {
        match self {
            FieldEditor::Text { name, .. }
            | FieldEditor::Number { name, .. }
            | FieldEditor::Select { name, .. }
            | FieldEditor::MultiSelect { name, .. } => name,
        }
    }
}

/// Buttons at the bottom of the form. Only `Run` exists today — Esc
/// handles cancel so a dedicated Cancel button would be redundant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormButton {
    Run,
}

/// Which element on the form currently has the keyboard cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFocus {
    Field(usize),
    Button(FormButton),
}

/// Form-entry state for the workflow at `workflow_idx`. Constructed by
/// opening a workflow header in the sidebar; cleared when the instance
/// starts or the user presses Esc.
#[derive(Debug, Clone)]
pub struct WorkflowFormState {
    #[allow(dead_code)]
    pub workflow_idx: usize,
    pub fields: Vec<FieldEditor>,
    pub focus: FormFocus,
    /// Last validation error produced on submit. Cleared on next
    /// keypress so typos don't linger.
    pub error: Option<String>,
    /// Per-MultiSelect cursor position (which option the arrow keys
    /// will toggle on Space). Keyed by the field index.
    pub multi_cursors: HashMap<usize, usize>,
}

impl FieldEditor {
    /// Insert a single character at the cursor (text/number fields).
    /// No-op on Select / MultiSelect.
    pub fn insert_char(&mut self, c: char) {
        match self {
            FieldEditor::Text { value, cursor, .. } => {
                let byte_idx = char_idx_to_byte(value, *cursor);
                value.insert(byte_idx, c);
                *cursor += 1;
            }
            FieldEditor::Number { value, cursor, .. } => {
                // Only accept digits, sign, dot — keeps the form from
                // turning into free-form text.
                if c.is_ascii_digit() || c == '.' || c == '-' {
                    let byte_idx = char_idx_to_byte(value, *cursor);
                    value.insert(byte_idx, c);
                    *cursor += 1;
                }
            }
            _ => {}
        }
    }

    /// Delete the char left of the cursor (backspace).
    pub fn backspace(&mut self) {
        match self {
            FieldEditor::Text { value, cursor, .. } | FieldEditor::Number { value, cursor, .. } => {
                if *cursor > 0 {
                    let start = char_idx_to_byte(value, *cursor - 1);
                    let end = char_idx_to_byte(value, *cursor);
                    value.replace_range(start..end, "");
                    *cursor -= 1;
                }
            }
            _ => {}
        }
    }

    /// Move the cursor one char left (text/number) or cycle one step
    /// back (Select). No-op on MultiSelect (use Space to toggle).
    pub fn move_left(&mut self) {
        match self {
            FieldEditor::Text { cursor, .. } | FieldEditor::Number { cursor, .. } => {
                if *cursor > 0 {
                    *cursor -= 1;
                }
            }
            FieldEditor::Select {
                selected, options, ..
            } => {
                if !options.is_empty() && *selected > 0 {
                    *selected -= 1;
                }
            }
            _ => {}
        }
    }

    /// Move the cursor one char right (text/number) or cycle one step
    /// forward (Select).
    pub fn move_right(&mut self) {
        match self {
            FieldEditor::Text { value, cursor, .. } => {
                if *cursor < value.chars().count() {
                    *cursor += 1;
                }
            }
            FieldEditor::Number { value, cursor, .. } => {
                if *cursor < value.chars().count() {
                    *cursor += 1;
                }
            }
            FieldEditor::Select {
                selected, options, ..
            } => {
                if !options.is_empty() && *selected + 1 < options.len() {
                    *selected += 1;
                }
            }
            _ => {}
        }
    }
}

/// Convert a Unicode-char index to the corresponding byte offset.
/// `String::insert` takes byte offsets, and `usize::min(chars)` is
/// cheap enough to compute each edit.
fn char_idx_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

impl WorkflowFormState {
    /// Build a fresh form state, skipping `Hidden` properties and
    /// pre-populating fields from declared defaults.
    pub fn new(workflow_idx: usize, workflow: &Workflow) -> Self {
        let fields: Vec<FieldEditor> = workflow
            .properties
            .iter()
            .filter(|p| p.kind.is_user_visible())
            .map(|p| match &p.kind {
                PropertyKind::Text { default } => {
                    let value = default.clone().unwrap_or_default();
                    let cursor = value.chars().count();
                    FieldEditor::Text {
                        name: p.id.clone(),
                        value,
                        cursor,
                    }
                }
                PropertyKind::Number { default, .. } => {
                    let value = default
                        .map(|n| PropertyValue::Number(n).as_display())
                        .unwrap_or_default();
                    let cursor = value.chars().count();
                    FieldEditor::Number {
                        name: p.id.clone(),
                        value,
                        cursor,
                    }
                }
                PropertyKind::Select { options, default } => {
                    let selected = default
                        .as_ref()
                        .and_then(|d| options.iter().position(|o| o == d))
                        .unwrap_or(0);
                    FieldEditor::Select {
                        name: p.id.clone(),
                        options: options.clone(),
                        selected,
                    }
                }
                PropertyKind::MultiSelect { options, default } => {
                    let defaults = default.clone().unwrap_or_default();
                    let chosen = options.iter().map(|o| defaults.contains(o)).collect();
                    FieldEditor::MultiSelect {
                        name: p.id.clone(),
                        options: options.clone(),
                        chosen,
                    }
                }
                PropertyKind::Hidden { .. } => unreachable!("filtered above"),
            })
            .collect();
        let focus = if fields.is_empty() {
            FormFocus::Button(FormButton::Run)
        } else {
            FormFocus::Field(0)
        };
        Self {
            workflow_idx,
            fields,
            focus,
            error: None,
            multi_cursors: HashMap::new(),
        }
    }

    /// Advance the keyboard focus to the next form element. Order:
    /// field 0 → field 1 → … → Run → field 0 (wrap).
    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            FormFocus::Field(i) => {
                if i + 1 < self.fields.len() {
                    FormFocus::Field(i + 1)
                } else {
                    FormFocus::Button(FormButton::Run)
                }
            }
            FormFocus::Button(FormButton::Run) => {
                if self.fields.is_empty() {
                    FormFocus::Button(FormButton::Run)
                } else {
                    FormFocus::Field(0)
                }
            }
        };
    }

    /// Reverse of `focus_next`.
    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            FormFocus::Field(0) => FormFocus::Button(FormButton::Run),
            FormFocus::Field(i) => FormFocus::Field(i - 1),
            FormFocus::Button(FormButton::Run) => {
                if self.fields.is_empty() {
                    FormFocus::Button(FormButton::Run)
                } else {
                    FormFocus::Field(self.fields.len() - 1)
                }
            }
        };
    }

    /// Return a mutable borrow of the currently-focused field, if any.
    pub fn focused_field_mut(&mut self) -> Option<&mut FieldEditor> {
        match self.focus {
            FormFocus::Field(i) => self.fields.get_mut(i),
            _ => None,
        }
    }

    /// Pointer into the MultiSelect cursor companion state. Each
    /// MultiSelect field tracks which option the cursor is on in its
    /// own counter so arrow keys feel consistent. Stored alongside the
    /// form so it survives focus cycles without adding another struct.
    pub fn multi_cursor(&self, field_idx: usize) -> usize {
        self.multi_cursors.get(&field_idx).copied().unwrap_or(0)
    }

    pub fn set_multi_cursor(&mut self, field_idx: usize, cursor: usize) {
        self.multi_cursors.insert(field_idx, cursor);
    }

    /// Validate + gather user inputs into a PropertyValue map suitable
    /// for `WorkflowEngine::start_instance`. Returns the `Run` button's
    /// error message on failure; callers should display it on the form.
    pub fn submit_values(
        &self,
        workflow: &Workflow,
    ) -> Result<HashMap<String, PropertyValue>, String> {
        let mut out: HashMap<String, PropertyValue> = HashMap::new();
        for (field, prop) in self.fields.iter().zip(
            workflow
                .properties
                .iter()
                .filter(|p| p.kind.is_user_visible()),
        ) {
            match (field, &prop.kind) {
                (FieldEditor::Text { value, .. }, PropertyKind::Text { .. }) => {
                    if prop.required && value.is_empty() {
                        return Err(format!("`{}` is required", prop.id));
                    }
                    if !value.is_empty() {
                        out.insert(prop.id.clone(), PropertyValue::Text(value.clone()));
                    }
                }
                (FieldEditor::Number { value, .. }, PropertyKind::Number { min, max, .. }) => {
                    if value.is_empty() {
                        if prop.required {
                            return Err(format!("`{}` is required", prop.id));
                        }
                        continue;
                    }
                    let parsed: f64 = value
                        .parse()
                        .map_err(|_| format!("`{}` must be a number, got `{}`", prop.id, value))?;
                    if let Some(m) = min {
                        if parsed < *m {
                            return Err(format!("`{}` must be ≥ {m}", prop.id));
                        }
                    }
                    if let Some(m) = max {
                        if parsed > *m {
                            return Err(format!("`{}` must be ≤ {m}", prop.id));
                        }
                    }
                    out.insert(prop.id.clone(), PropertyValue::Number(parsed));
                }
                (
                    FieldEditor::Select {
                        selected, options, ..
                    },
                    PropertyKind::Select { .. },
                ) => {
                    if let Some(v) = options.get(*selected) {
                        out.insert(prop.id.clone(), PropertyValue::Selection(v.clone()));
                    } else if prop.required {
                        return Err(format!("`{}` is required", prop.id));
                    }
                }
                (
                    FieldEditor::MultiSelect {
                        chosen, options, ..
                    },
                    PropertyKind::MultiSelect { .. },
                ) => {
                    let picks: Vec<String> = options
                        .iter()
                        .zip(chosen.iter())
                        .filter_map(|(o, &b)| if b { Some(o.clone()) } else { None })
                        .collect();
                    if prop.required && picks.is_empty() {
                        return Err(format!("`{}` requires at least one selection", prop.id));
                    }
                    out.insert(prop.id.clone(), PropertyValue::Multi(picks));
                }
                _ => {
                    // Shape drift between field/prop (shouldn't happen
                    // with the current constructor, but bail safely).
                    return Err(format!(
                        "internal: field/prop kind mismatch for `{}`",
                        prop.id
                    ));
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{WorkflowProperty, WorkflowStep};

    fn wf_with_props(props: Vec<WorkflowProperty>) -> Workflow {
        Workflow {
            id: "w".into(),
            description: "d".into(),
            max_retries: 5,
            properties: props,
            steps: vec![WorkflowStep {
                id: "only".into(),
                kind: StepKind::Command {
                    command: "true".into(),
                    cwd: None,
                },
            }],
        }
    }

    #[test]
    fn form_skips_hidden_properties() {
        let wf = wf_with_props(vec![
            WorkflowProperty {
                id: "visible".into(),
                label: None,
                required: false,
                kind: PropertyKind::Text {
                    default: Some("seen".into()),
                },
            },
            WorkflowProperty {
                id: "secret".into(),
                label: None,
                required: false,
                kind: PropertyKind::Hidden {
                    default: Some("shh".into()),
                },
            },
        ]);
        let form = WorkflowFormState::new(0, &wf);
        assert_eq!(form.fields.len(), 1);
        assert_eq!(form.fields[0].name(), "visible");
    }

    #[test]
    fn form_with_no_visible_fields_focuses_run_button() {
        let wf = wf_with_props(vec![WorkflowProperty {
            id: "secret".into(),
            label: None,
            required: false,
            kind: PropertyKind::Hidden { default: None },
        }]);
        let form = WorkflowFormState::new(0, &wf);
        assert!(form.fields.is_empty());
        assert_eq!(form.focus, FormFocus::Button(FormButton::Run));
    }

    #[test]
    fn form_preselects_multi_select_defaults() {
        let wf = wf_with_props(vec![WorkflowProperty {
            id: "suites".into(),
            label: None,
            required: false,
            kind: PropertyKind::MultiSelect {
                options: vec!["unit".into(), "integ".into(), "e2e".into()],
                default: Some(vec!["unit".into(), "e2e".into()]),
            },
        }]);
        let form = WorkflowFormState::new(0, &wf);
        match &form.fields[0] {
            FieldEditor::MultiSelect { chosen, .. } => {
                assert_eq!(chosen, &vec![true, false, true]);
            }
            other => panic!("expected MultiSelect, got {other:?}"),
        }
    }
}
