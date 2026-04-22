#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// A system message to append to the chat history.
    Message(String),
    /// Terminate the TUI.
    Quit,
    /// Run the target agent's configured compaction strategy (falling
    /// back to `drop` when unconfigured).
    Compact,
    /// Unconditionally drop the target agent's session and re-seed
    /// with the first-turn preamble.
    Reset,
    /// Cancel the target agent's currently running turn (if any),
    /// killing in-flight tool calls. Agent stays online.
    Stop,
    /// Stop any running turn, then refuse mailbox/channel deliveries
    /// until `/awake` is invoked.
    Sleep,
    /// Clear the asleep flag so the agent receives messages again.
    Awake,
    /// Input didn't match any known command.
    Unknown { name: String, hint: String },
}

/// Parse an input line that begins with `/` and dispatch to a command.
///
/// The leading `/` is expected but tolerated if missing. Returns `None` if
/// the input is empty after trimming (so callers can decide whether to echo
/// the bare slash as a message or silently drop it).
pub fn dispatch(input: &str) -> Option<CommandOutcome> {
    let trimmed = input.trim().trim_start_matches('/').trim();
    if trimmed.is_empty() {
        return None;
    }
    let (name, _args) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));

    let outcome = match name {
        "version" => CommandOutcome::Message(format!("aciddog v{}", env!("CARGO_PKG_VERSION"))),
        "exit" | "quit" => CommandOutcome::Quit,
        "compact" => CommandOutcome::Compact,
        "reset" => CommandOutcome::Reset,
        "stop" => CommandOutcome::Stop,
        "sleep" => CommandOutcome::Sleep,
        "awake" | "wake" => CommandOutcome::Awake,
        other => CommandOutcome::Unknown {
            name: other.to_string(),
            hint: "try /version, /compact, /reset, /stop, /sleep, /awake or /exit".to_string(),
        },
    };
    Some(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_version_string() {
        let out = dispatch("/version").unwrap();
        match out {
            CommandOutcome::Message(msg) => {
                assert!(msg.contains(env!("CARGO_PKG_VERSION")));
                assert!(msg.contains("aciddog"));
            }
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn exit_and_quit_both_quit() {
        assert_eq!(dispatch("/exit"), Some(CommandOutcome::Quit));
        assert_eq!(dispatch("/quit"), Some(CommandOutcome::Quit));
    }

    #[test]
    fn unknown_command_returns_unknown() {
        let out = dispatch("/foobar").unwrap();
        match out {
            CommandOutcome::Unknown { name, .. } => assert_eq!(name, "foobar"),
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn empty_slash_returns_none() {
        assert_eq!(dispatch("/"), None);
        assert_eq!(dispatch("/   "), None);
    }

    #[test]
    fn ignores_args_for_now() {
        // `/version extra words` still matches `version`.
        let out = dispatch("/version something").unwrap();
        assert!(matches!(out, CommandOutcome::Message(_)));
    }

    #[test]
    fn compact_returns_compact_outcome() {
        assert_eq!(dispatch("/compact"), Some(CommandOutcome::Compact));
    }

    #[test]
    fn reset_returns_reset_outcome() {
        assert_eq!(dispatch("/reset"), Some(CommandOutcome::Reset));
    }

    #[test]
    fn stop_returns_stop_outcome() {
        assert_eq!(dispatch("/stop"), Some(CommandOutcome::Stop));
    }

    #[test]
    fn sleep_returns_sleep_outcome() {
        assert_eq!(dispatch("/sleep"), Some(CommandOutcome::Sleep));
    }

    #[test]
    fn awake_and_wake_both_return_awake() {
        assert_eq!(dispatch("/awake"), Some(CommandOutcome::Awake));
        assert_eq!(dispatch("/wake"), Some(CommandOutcome::Awake));
    }
}
