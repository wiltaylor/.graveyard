//! Deserialize/serialize mirror of `aciddog::wire_repos`.
//!
//! Same rationale as [`super::wire_tasks`]: the manager crate can't
//! link `aciddog` directly because both crates link incompatible
//! `sqlite3` stacks. This module duplicates the wire shape; canonical
//! definition is in `crates/aciddog/src/wire_repos.rs`.

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct WireRepository {
    pub id: String,
    pub path: String,
    pub default_branch: Option<String>,
    pub worktrees: Vec<WireWorktree>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct WireWorktree {
    pub path: String,
    pub branch: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WireDiffLineKind {
    Header,
    Plus,
    Minus,
    Hunk,
    Context,
    Truncation,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct WireDiffLine {
    pub kind: WireDiffLineKind,
    pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum WireDiffOutcome {
    NoForwardChanges,
    Detached,
    Diff {
        lines: Vec<WireDiffLine>,
        truncated_at: Option<usize>,
    },
    Error {
        command: String,
        stderr: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_roundtrip(outcome: WireDiffOutcome, literal: &str) {
        let expected: serde_json::Value = serde_json::from_str(literal).unwrap();
        assert_eq!(serde_json::to_value(&outcome).unwrap(), expected);

        let back: WireDiffOutcome = serde_json::from_str(literal).unwrap();
        assert_eq!(back, outcome);
    }

    #[test]
    fn no_forward_changes_roundtrips_with_outcome_tag() {
        assert_roundtrip(
            WireDiffOutcome::NoForwardChanges,
            r#"{"outcome":"no_forward_changes"}"#,
        );
    }

    #[test]
    fn detached_roundtrips_with_outcome_tag() {
        assert_roundtrip(WireDiffOutcome::Detached, r#"{"outcome":"detached"}"#);
    }

    #[test]
    fn diff_without_truncation_roundtrips_with_snake_case_line_kinds() {
        assert_roundtrip(
            WireDiffOutcome::Diff {
                lines: vec![
                    WireDiffLine {
                        kind: WireDiffLineKind::Header,
                        text: "diff --git a/a b/a".into(),
                    },
                    WireDiffLine {
                        kind: WireDiffLineKind::Plus,
                        text: "+added".into(),
                    },
                ],
                truncated_at: None,
            },
            r#"{"outcome":"diff","lines":[{"kind":"header","text":"diff --git a/a b/a"},{"kind":"plus","text":"+added"}],"truncated_at":null}"#,
        );
    }

    #[test]
    fn diff_with_truncation_roundtrips_with_truncation_kind() {
        assert_roundtrip(
            WireDiffOutcome::Diff {
                lines: vec![WireDiffLine {
                    kind: WireDiffLineKind::Truncation,
                    text: "… diff exceeds 10 000 lines (got 10001); run `git -C /repo diff --no-color main...feature` for the full diff.".into(),
                }],
                truncated_at: Some(10_001),
            },
            r#"{"outcome":"diff","lines":[{"kind":"truncation","text":"… diff exceeds 10 000 lines (got 10001); run `git -C /repo diff --no-color main...feature` for the full diff."}],"truncated_at":10001}"#,
        );
    }

    #[test]
    fn error_roundtrips_with_command_and_stderr() {
        assert_roundtrip(
            WireDiffOutcome::Error {
                command: "git diff".into(),
                stderr: "fatal: bad revision".into(),
            },
            r#"{"outcome":"error","command":"git diff","stderr":"fatal: bad revision"}"#,
        );
    }
}
