use crate::compiler::token::Span;
use crate::runtime::value::DebugValue;
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Table of active breakpoints, mapping source lines to probe location IDs.
#[derive(Debug, Default)]
pub struct BreakpointTable {
    line_to_probe: HashMap<u32, u32>,
    active_probes: HashSet<u32>,
}

impl BreakpointTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a breakpoint at the given source `line`, associated with a
    /// probe location `probe_id`.
    pub fn set_breakpoint(&mut self, line: u32, probe_id: u32) {
        self.line_to_probe.insert(line, probe_id);
        self.active_probes.insert(probe_id);
    }

    /// Remove the breakpoint at the given source `line`, if any.
    pub fn clear_breakpoint(&mut self, line: u32) {
        if let Some(probe_id) = self.line_to_probe.remove(&line) {
            // Only remove from active_probes if no other line maps to the same probe.
            let still_referenced = self.line_to_probe.values().any(|&p| p == probe_id);
            if !still_referenced {
                self.active_probes.remove(&probe_id);
            }
        }
    }

    /// Remove all breakpoints.
    pub fn clear_all(&mut self) {
        self.line_to_probe.clear();
        self.active_probes.clear();
    }

    /// Check whether a probe location has an active breakpoint.
    pub fn is_active(&self, probe_id: u32) -> bool {
        self.active_probes.contains(&probe_id)
    }

    /// Return all currently set (line, probe_id) pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&u32, &u32)> {
        self.line_to_probe.iter()
    }

    /// Number of active breakpoints.
    pub fn len(&self) -> usize {
        self.line_to_probe.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.line_to_probe.is_empty()
    }
}

/// Information about a paused execution point.
#[derive(Debug, Clone)]
pub struct BreakpointFrame {
    pub location: Span,
    pub fn_name: String,
    pub locals: HashMap<String, DebugValue>,
    pub callstack: Vec<SourceFrame>,
}

/// A single frame in a source-level stack trace.
#[derive(Debug, Clone)]
pub struct SourceFrame {
    pub fn_name: String,
    pub file: String,
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for SourceFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "  at {} ({}:{}:{})", self.fn_name, self.file, self.line, self.col)
    }
}

/// Action to take after hitting a breakpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DebugAction {
    Continue,
    StepOver,
    StepInto,
    StepOut,
    Stop,
    SetValue { name: String, value: String },
}

/// Internal stepping mode used by the VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepMode {
    /// Normal execution -- no stepping.
    None,
    /// Step into every call.
    StepInto,
    /// Step over -- pause when we return to the given call depth.
    StepOver(u32),
    /// Step out -- pause when we leave the given call depth.
    StepOut(u32),
}

impl Default for StepMode {
    fn default() -> Self {
        Self::None
    }
}

/// A script panic with a source-level stack trace.
#[derive(Debug, Clone)]
pub struct ScriptPanic {
    pub message: String,
    pub trace: Vec<SourceFrame>,
}

impl fmt::Display for ScriptPanic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "script panic: {}", self.message)?;
        for frame in &self.trace {
            writeln!(f, "{frame}")?;
        }
        Ok(())
    }
}

impl std::error::Error for ScriptPanic {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breakpoint_set_and_check() {
        let mut table = BreakpointTable::new();
        assert!(!table.is_active(5));
        table.set_breakpoint(10, 5);
        assert!(table.is_active(5));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn breakpoint_clear() {
        let mut table = BreakpointTable::new();
        table.set_breakpoint(10, 5);
        table.set_breakpoint(20, 8);
        assert!(table.is_active(5));
        assert!(table.is_active(8));

        table.clear_breakpoint(10);
        assert!(!table.is_active(5));
        assert!(table.is_active(8));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn breakpoint_clear_all() {
        let mut table = BreakpointTable::new();
        table.set_breakpoint(1, 1);
        table.set_breakpoint(2, 2);
        table.clear_all();
        assert!(table.is_empty());
        assert!(!table.is_active(1));
        assert!(!table.is_active(2));
    }

    #[test]
    fn script_panic_display() {
        let panic = ScriptPanic {
            message: "index out of bounds".into(),
            trace: vec![
                SourceFrame {
                    fn_name: "get_item".into(),
                    file: "main.ss".into(),
                    line: 42,
                    col: 5,
                },
                SourceFrame {
                    fn_name: "main".into(),
                    file: "main.ss".into(),
                    line: 10,
                    col: 1,
                },
            ],
        };
        let s = panic.to_string();
        assert!(s.contains("index out of bounds"));
        assert!(s.contains("get_item"));
        assert!(s.contains("main.ss:42:5"));
    }

    #[test]
    fn step_mode_default() {
        assert_eq!(StepMode::default(), StepMode::None);
    }
}
