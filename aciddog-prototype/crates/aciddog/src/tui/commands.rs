#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// A system message to append to the chat history.
    Message(String),
    /// Terminate the TUI.
    Quit,
    /// Begin a graceful shutdown: sleep all idle agents, let busy
    /// agents finish their current turn (then sleep them), stop the
    /// cron scheduler, and refuse further agent-originated mailbox /
    /// channel sends. When the last agent is asleep the TUI exits.
    Shutdown,
    /// Run the target agent's configured compaction strategy (falling
    /// back to `drop` when unconfigured).
    Compact,
    /// Unconditionally drop the target agent's session and re-seed
    /// with the first-turn preamble.
    Reset,
    /// Re-read the WCL config and reconcile the running fleet (agents,
    /// channels, workflows, automatons + webhooks). Per-agent
    /// `on_reload` controls whether changed agents are left alone,
    /// compacted, or reset.
    Reload,
    /// Cancel the target agent's currently running turn (if any),
    /// killing in-flight tool calls. Agent stays online.
    Stop,
    /// Stop any running turn, then refuse mailbox/channel deliveries
    /// until `/awake` is invoked.
    Sleep,
    /// Clear the asleep flag so the agent receives messages again.
    Awake,
    /// `/run <workflow_id>` — kick off a workflow instance using
    /// declared defaults (or empty where undeclared). Useful for
    /// smoke-testing before the full property-entry form lands.
    RunWorkflow { workflow_id: String },
    /// `/meeting <id>` — create an ephemeral channel at runtime.
    /// Valid from any pane; the view switches to the new channel.
    CreateMeeting { id: String },
    /// `/add <agent>` — add an agent to the channel in the current
    /// pane. Only valid from a channel pane.
    AddAgent { agent: String },
    /// `/kick <agent>` — remove an agent from the channel in the
    /// current pane. Only valid from a channel pane.
    KickAgent { agent: String },
    /// `/leave` — destroy the current pane's ephemeral channel. Only
    /// valid from a channel pane; rejected on WCL-declared channels.
    LeaveChannel,
    /// `/auth login chatgpt <name>` — run the OAuth flow for a named
    /// credential. Currently only `chatgpt` is supported.
    AuthLogin { provider: String, name: String },
    /// `/auth logout <name>` — remove a stored credential.
    AuthLogout { name: String },
    /// `/auth list` — print every stored credential (provider +
    /// account id + expiry, no tokens).
    AuthList,
    /// `/auth status [<name>]` — print the provider / account id /
    /// expiry for a single credential (or all if `name` is None).
    AuthStatus { name: Option<String> },
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
    let (name, args) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));
    let args = args.trim();

    let outcome = match name {
        "version" => CommandOutcome::Message(format!("aciddog v{}", env!("CARGO_PKG_VERSION"))),
        "exit" | "quit" => CommandOutcome::Quit,
        "shutdown" => CommandOutcome::Shutdown,
        "compact" => CommandOutcome::Compact,
        "reset" => CommandOutcome::Reset,
        "reload" => CommandOutcome::Reload,
        "stop" => CommandOutcome::Stop,
        "sleep" => CommandOutcome::Sleep,
        "awake" | "wake" => CommandOutcome::Awake,
        "run" | "workflow" => {
            if args.is_empty() {
                CommandOutcome::Message(
                    "usage: /run <workflow_id>  — kicks off a workflow with declared defaults"
                        .into(),
                )
            } else {
                // Only keep the first whitespace-delimited token; any
                // further args are ignored for now (the full
                // property-entry form lands with task #3).
                let id = args.split_whitespace().next().unwrap_or("").to_string();
                CommandOutcome::RunWorkflow { workflow_id: id }
            }
        }
        "meeting" => {
            if args.is_empty() {
                CommandOutcome::Message(
                    "usage: /meeting <id>  — creates an ephemeral meeting channel".into(),
                )
            } else {
                let id = args.split_whitespace().next().unwrap_or("").to_string();
                CommandOutcome::CreateMeeting { id }
            }
        }
        "add" => {
            if args.is_empty() {
                CommandOutcome::Message(
                    "usage: /add <agent>  — only valid from a channel pane".into(),
                )
            } else {
                let agent = args.split_whitespace().next().unwrap_or("").to_string();
                CommandOutcome::AddAgent { agent }
            }
        }
        "kick" => {
            if args.is_empty() {
                CommandOutcome::Message(
                    "usage: /kick <agent>  — only valid from a channel pane".into(),
                )
            } else {
                let agent = args.split_whitespace().next().unwrap_or("").to_string();
                CommandOutcome::KickAgent { agent }
            }
        }
        "leave" => CommandOutcome::LeaveChannel,
        "auth" => parse_auth(args),
        other => CommandOutcome::Unknown {
            name: other.to_string(),
            hint: "try /version, /compact, /reset, /reload, /stop, /sleep, /awake, /run <wf>, \
                   /meeting <id>, /add <agent>, /kick <agent>, /leave, /auth, \
                   /shutdown, or /exit"
                .to_string(),
        },
    };
    Some(outcome)
}

/// Parse the tail of an `/auth …` command. Accepts
/// `login <provider> <name>`, `logout <name>`, `list`, or
/// `status [<name>]`. Everything else returns a usage-message
/// `CommandOutcome`.
fn parse_auth(args: &str) -> CommandOutcome {
    let mut it = args.split_whitespace();
    let Some(sub) = it.next() else {
        return CommandOutcome::Message(auth_usage().into());
    };
    match sub {
        "login" => {
            let provider = it.next().unwrap_or("");
            let name = it.next().unwrap_or("");
            if provider.is_empty() || name.is_empty() {
                return CommandOutcome::Message(
                    "usage: /auth login <provider> <name>  — provider: chatgpt".into(),
                );
            }
            CommandOutcome::AuthLogin {
                provider: provider.to_string(),
                name: name.to_string(),
            }
        }
        "logout" => {
            let name = it.next().unwrap_or("");
            if name.is_empty() {
                return CommandOutcome::Message(
                    "usage: /auth logout <name>  — removes the credential".into(),
                );
            }
            CommandOutcome::AuthLogout {
                name: name.to_string(),
            }
        }
        "list" => CommandOutcome::AuthList,
        "status" => CommandOutcome::AuthStatus {
            name: it.next().map(str::to_string),
        },
        other => CommandOutcome::Message(format!(
            "unknown /auth subcommand `{other}`. {}",
            auth_usage()
        )),
    }
}

fn auth_usage() -> &'static str {
    "usage: /auth <login|logout|list|status> …  (e.g. /auth login chatgpt personal)"
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
    fn shutdown_returns_shutdown_outcome() {
        assert_eq!(dispatch("/shutdown"), Some(CommandOutcome::Shutdown));
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
    fn reload_returns_reload_outcome() {
        assert_eq!(dispatch("/reload"), Some(CommandOutcome::Reload));
    }

    #[test]
    fn unknown_command_hint_mentions_reload() {
        let out = dispatch("/foobar").unwrap();
        match out {
            CommandOutcome::Unknown { hint, .. } => assert!(
                hint.contains("/reload"),
                "expected hint to mention /reload, got: {hint}"
            ),
            other => panic!("expected Unknown, got {other:?}"),
        }
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

    #[test]
    fn meeting_without_id_returns_usage_message() {
        let out = dispatch("/meeting").unwrap();
        match out {
            CommandOutcome::Message(msg) => assert!(msg.starts_with("usage: /meeting")),
            other => panic!("expected usage Message, got {other:?}"),
        }
    }

    #[test]
    fn meeting_with_id_returns_create_meeting() {
        assert_eq!(
            dispatch("/meeting standup"),
            Some(CommandOutcome::CreateMeeting {
                id: "standup".into()
            })
        );
    }

    #[test]
    fn meeting_keeps_first_token_only() {
        assert_eq!(
            dispatch("/meeting standup extra ignored"),
            Some(CommandOutcome::CreateMeeting {
                id: "standup".into()
            })
        );
    }

    #[test]
    fn add_without_agent_returns_usage_message() {
        let out = dispatch("/add").unwrap();
        match out {
            CommandOutcome::Message(msg) => assert!(msg.starts_with("usage: /add")),
            other => panic!("expected usage Message, got {other:?}"),
        }
    }

    #[test]
    fn add_with_agent_returns_add_agent() {
        assert_eq!(
            dispatch("/add planner"),
            Some(CommandOutcome::AddAgent {
                agent: "planner".into()
            })
        );
    }

    #[test]
    fn kick_without_agent_returns_usage_message() {
        let out = dispatch("/kick").unwrap();
        match out {
            CommandOutcome::Message(msg) => assert!(msg.starts_with("usage: /kick")),
            other => panic!("expected usage Message, got {other:?}"),
        }
    }

    #[test]
    fn kick_with_agent_returns_kick_agent() {
        assert_eq!(
            dispatch("/kick planner"),
            Some(CommandOutcome::KickAgent {
                agent: "planner".into()
            })
        );
    }

    #[test]
    fn leave_returns_leave_channel() {
        assert_eq!(dispatch("/leave"), Some(CommandOutcome::LeaveChannel));
    }

    #[test]
    fn leave_ignores_trailing_args() {
        assert_eq!(
            dispatch("/leave please"),
            Some(CommandOutcome::LeaveChannel)
        );
    }

    #[test]
    fn auth_login_parses_provider_and_name() {
        assert_eq!(
            dispatch("/auth login chatgpt personal"),
            Some(CommandOutcome::AuthLogin {
                provider: "chatgpt".into(),
                name: "personal".into(),
            })
        );
    }

    #[test]
    fn auth_login_missing_args_returns_usage() {
        let out = dispatch("/auth login").unwrap();
        match out {
            CommandOutcome::Message(msg) => assert!(msg.starts_with("usage: /auth login")),
            other => panic!("expected Message, got {other:?}"),
        }
    }

    #[test]
    fn auth_logout_requires_name() {
        assert_eq!(
            dispatch("/auth logout work"),
            Some(CommandOutcome::AuthLogout {
                name: "work".into()
            })
        );
        let out = dispatch("/auth logout").unwrap();
        assert!(matches!(out, CommandOutcome::Message(m) if m.starts_with("usage: /auth logout")));
    }

    #[test]
    fn auth_list_and_status() {
        assert_eq!(dispatch("/auth list"), Some(CommandOutcome::AuthList));
        assert_eq!(
            dispatch("/auth status"),
            Some(CommandOutcome::AuthStatus { name: None })
        );
        assert_eq!(
            dispatch("/auth status personal"),
            Some(CommandOutcome::AuthStatus {
                name: Some("personal".into())
            })
        );
    }

    #[test]
    fn auth_without_subcommand_returns_usage() {
        let out = dispatch("/auth").unwrap();
        assert!(matches!(out, CommandOutcome::Message(m) if m.starts_with("usage: /auth")));
    }

    #[test]
    fn auth_unknown_subcommand_returns_message() {
        let out = dispatch("/auth wat").unwrap();
        assert!(
            matches!(out, CommandOutcome::Message(m) if m.contains("unknown /auth subcommand"))
        );
    }
}
