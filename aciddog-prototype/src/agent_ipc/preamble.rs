/// Build the system-generated preamble that is prepended to each agent's
/// instructions. Describes the `aciddog` CLI surface so the agent knows
/// which subcommands it can invoke.
///
/// `binary` is the absolute path to the running aciddog binary, so the
/// agent can invoke it directly without relying on it being on `PATH`.
///
/// `contactable` is the list of other-agent config ids this agent is
/// permitted to message. If empty, the `send` subcommand is omitted.
///
/// Extending: add a new `push_section` call for each new subcommand just
/// before the `// add new subcommand sections above this line` marker.
pub fn cli_preamble(binary: &str, contactable: &[&str]) -> String {
    let mut out = String::new();
    push_line(&mut out, "## aciddog agent CLI");
    push_line(
        &mut out,
        "You have an `aciddog` command available that lets you interact with \
         the agent orchestrator running this session. Invoke it using the \
         absolute path below (it may not be on your `PATH`).",
    );
    push_line(&mut out, &format!("Binary path: `{binary}`"));
    push_line(&mut out, "### Subcommands");

    push_section(
        &mut out,
        binary,
        "test",
        "verify connectivity to the agent orchestrator.",
    );

    if contactable.is_empty() {
        push_line(
            &mut out,
            "- `send` — disabled: you may not message any other agent on this team.",
        );
    } else {
        push_section(
            &mut out,
            binary,
            "send --to <agent-id> --body \"<text>\"",
            "send a chat message to another agent on the team.",
        );
        push_line(
            &mut out,
            &format!("  You are allowed to message: {}", contactable.join(", ")),
        );
        push_line(
            &mut out,
            "  Messages you receive from other agents arrive wrapped as \
             `<Message from=\"sender-id\">body</Message>`. Plain text from the \
             user does NOT have this wrapper.",
        );
        push_line(
            &mut out,
            "  If you are busy when a message arrives it is queued and delivered \
             on your next turn. User messages always take priority over queued \
             agent messages.",
        );
    }
    // add new subcommand sections above this line

    out
}

/// Append a line followed by a blank line, so the final rendered prompt has
/// visible breathing room between entries.
fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push_str("\n\n");
}

fn push_section(out: &mut String, binary: &str, name: &str, description: &str) {
    push_line(out, &format!("- `{binary} {name}` — {description}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preamble_mentions_test_subcommand_with_binary_path() {
        let p = cli_preamble("/opt/aciddog/bin/aciddog", &["coder"]);
        assert!(
            p.contains("/opt/aciddog/bin/aciddog test"),
            "missing fully-qualified `test` entry: {p}"
        );
        assert!(
            p.to_lowercase().contains("connectivity"),
            "missing connectivity sentence: {p}"
        );
    }

    #[test]
    fn preamble_has_header() {
        let p = cli_preamble("/usr/local/bin/aciddog", &["coder"]);
        assert!(p.starts_with("## aciddog agent CLI"), "missing header: {p}");
    }

    #[test]
    fn preamble_surfaces_binary_path_explicitly() {
        let p = cli_preamble("/tmp/weird path/aciddog", &["coder"]);
        assert!(
            p.contains("Binary path: `/tmp/weird path/aciddog`"),
            "binary path callout missing: {p}"
        );
    }

    #[test]
    fn preamble_lists_only_contactable_ids() {
        let p = cli_preamble("/a", &["coder", "reviewer"]);
        assert!(
            p.contains("coder, reviewer"),
            "missing contactable ids: {p}"
        );
    }

    #[test]
    fn preamble_mentions_send_subcommand_and_wrapping_format() {
        let p = cli_preamble("/a", &["coder"]);
        assert!(p.contains("/a send --to"), "missing send example: {p}");
        assert!(
            p.contains("<Message from=\"sender-id\">body</Message>"),
            "missing wrapper doc: {p}"
        );
        assert!(
            p.contains("User messages always take priority"),
            "missing priority note: {p}"
        );
    }

    #[test]
    fn preamble_hides_send_when_allowlist_is_empty() {
        let p = cli_preamble("/a", &[]);
        assert!(!p.contains("/a send"), "send should be hidden: {p}");
        assert!(p.contains("disabled"), "should note send is disabled: {p}");
    }
}
