/// Build the system-generated preamble that is prepended to each
/// actor's instructions. Describes the `aciddog` CLI surface so the
/// actor knows which subcommands it can invoke.
///
/// `binary` is the absolute path to the running aciddog binary, so the
/// actor can invoke it directly without relying on it being on `PATH`.
///
/// `contactable` is the list of other-actor ids this actor is permitted
/// to message. If empty, the `send` subcommand is omitted.
///
/// `creatable` is the list of worker template ids this actor is
/// permitted to spawn. If empty, the `worker create` subcommand is
/// omitted.
///
/// The `worker complete` subcommand is **not** documented here — it is
/// only available inside a running worker and is injected by
/// `worker_preamble` so plain agents cannot accidentally call it.
///
/// Extending: add a new `push_section` call for each new subcommand
/// just before the `// add new subcommand sections above this line`
/// marker.
pub fn cli_preamble(binary: &str, contactable: &[&str], creatable: &[&str]) -> String {
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

    if creatable.is_empty() {
        push_line(
            &mut out,
            "- `worker create` — disabled: you may not spawn workers.",
        );
    } else {
        push_section(
            &mut out,
            binary,
            "worker create --template <template-id> --task \"<initial task>\"",
            "spawn an ephemeral worker from a declared template to handle a sub-task. \
             The response carries the new worker's display id and session UUID; use \
             the display id with `send --to` to message the worker, and retain the \
             UUID so you can resume the worker later if it hibernates.",
        );
        push_line(
            &mut out,
            &format!("  You are allowed to spawn: {}", creatable.join(", ")),
        );
    }
    push_section(
        &mut out,
        binary,
        "task <add|list|show|update|remove|take|note>",
        "shared team task board. Use `task add --title \"<t>\" [--description ...] [--assignee <id>] \
         [--blocked-by <dep-uuid>]...` to create work (repeat `--blocked-by` for multiple deps), \
         `task list [--mine|--assignee <id>|--status <s>|--unassigned]` to see the table (the `deps` \
         column shows `blocked(n)` or `ready(n)` for tasks with dependencies), \
         `task show --id <uuid>` to print a task's full body — description, dependencies with their \
         current status, and the complete note log with authors and ages, \
         `task update --id <uuid> [--title ...] [--description ...] [--status open|assigned|in_progress|done|cancelled] \
         [--assignee <id>|--unassign] [--note \"<context>\"] [--add-blocked-by <uuid>]... [--remove-blocked-by <uuid>]...` \
         to change it or hand it off, `task take` to atomically claim the oldest unblocked unassigned task \
         (it moves to `in_progress` assigned to you — tasks with any open dependency are skipped until \
         every dep reaches `done`), `task remove --id <uuid>` to drop it (also prunes the id from any \
         dependents' lists), and `task note --id <uuid> --body \"<text>\"` to append a log entry. \
         The board is team-wide — there is no allowlist. Always attach a `--note` when reassigning so \
         the next owner sees context (e.g. \"ready for test, commit abc123\"); run `task show` when you \
         pick up a handed-off task to read prior notes.",
    );
    // add new subcommand sections above this line

    out
}

/// Worker-only preamble chunk. Prepended before `cli_preamble` when
/// spawning a worker, so the first thing the model sees is its
/// identity, task, and termination contract.
pub fn worker_preamble(binary: &str, parent_id: &str, task: &str) -> String {
    let mut out = String::new();
    push_line(&mut out, "## Worker assignment");
    push_line(
        &mut out,
        &format!(
            "You are an ephemeral worker spawned by `{parent_id}`. Your only job \
             is to complete the task below and report back. You may only message \
             `{parent_id}`; you cannot message anyone else on the team."
        ),
    );
    push_line(&mut out, "### Your task");
    push_line(&mut out, task);
    push_line(&mut out, "### Reporting back");
    push_line(
        &mut out,
        &format!("- For progress updates, use `{binary} send --to {parent_id} --body \"<text>\"`.",),
    );
    push_line(
        &mut out,
        &format!(
            "- When you are finished, call `{binary} worker complete --body \"<final report>\"`. \
             This delivers your final report to `{parent_id}` and hibernates you. After that \
             call, stop taking further actions; your parent may resume the conversation later \
             if they need more from you.",
        ),
    );
    out
}

/// Append a line followed by a blank line, so the final rendered prompt
/// has visible breathing room between entries.
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
        let p = cli_preamble("/opt/aciddog/bin/aciddog", &["coder"], &[]);
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
        let p = cli_preamble("/usr/local/bin/aciddog", &["coder"], &[]);
        assert!(p.starts_with("## aciddog agent CLI"), "missing header: {p}");
    }

    #[test]
    fn preamble_surfaces_binary_path_explicitly() {
        let p = cli_preamble("/tmp/weird path/aciddog", &["coder"], &[]);
        assert!(
            p.contains("Binary path: `/tmp/weird path/aciddog`"),
            "binary path callout missing: {p}"
        );
    }

    #[test]
    fn preamble_lists_only_contactable_ids() {
        let p = cli_preamble("/a", &["coder", "reviewer"], &[]);
        assert!(
            p.contains("coder, reviewer"),
            "missing contactable ids: {p}"
        );
    }

    #[test]
    fn preamble_mentions_send_subcommand_and_wrapping_format() {
        let p = cli_preamble("/a", &["coder"], &[]);
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
        let p = cli_preamble("/a", &[], &[]);
        assert!(!p.contains("/a send"), "send should be hidden: {p}");
        assert!(
            p.contains("- `send` — disabled"),
            "should note send is disabled: {p}"
        );
    }

    #[test]
    fn preamble_mentions_worker_create_when_creatable() {
        let p = cli_preamble("/a", &["coder"], &["researcher", "tester"]);
        assert!(
            p.contains("/a worker create --template"),
            "missing worker create example: {p}"
        );
        assert!(
            p.contains("researcher, tester"),
            "missing creatable ids: {p}"
        );
    }

    #[test]
    fn preamble_hides_worker_create_when_empty() {
        let p = cli_preamble("/a", &["coder"], &[]);
        assert!(
            !p.contains("/a worker create"),
            "worker create should be hidden: {p}"
        );
        assert!(
            p.contains("- `worker create` — disabled"),
            "should note worker create is disabled: {p}"
        );
    }

    #[test]
    fn preamble_never_documents_worker_complete() {
        let p = cli_preamble("/a", &["coder"], &["researcher"]);
        assert!(
            !p.contains("worker complete"),
            "plain agents must not see worker complete: {p}"
        );
    }

    #[test]
    fn worker_preamble_names_parent_task_and_complete() {
        let p = worker_preamble("/a", "coder", "find FOO");
        assert!(p.contains("spawned by `coder`"), "missing parent id: {p}");
        assert!(p.contains("find FOO"), "missing task: {p}");
        assert!(
            p.contains("/a worker complete --body"),
            "missing complete example: {p}"
        );
        assert!(
            p.contains("/a send --to coder --body"),
            "missing progress send example: {p}"
        );
    }

    #[test]
    fn preamble_mentions_task_subcommand() {
        let p = cli_preamble("/a", &[], &[]);
        assert!(
            p.contains("/a task <add|list|show|update|remove|take|note>"),
            "missing task subcommand entry: {p}"
        );
        assert!(
            p.to_lowercase().contains("shared team task board"),
            "missing task board description: {p}"
        );
        assert!(p.contains("task show --id"), "missing show docs: {p}");
    }

    #[test]
    fn preamble_task_subcommand_present_even_without_allowlists() {
        // The task board has no allowlist gate, so it must appear even
        // when `contactable` and `creatable` are both empty.
        let p = cli_preamble("/a", &[], &[]);
        assert!(p.contains("/a task"), "task should be unconditional: {p}");
    }

    #[test]
    fn worker_preamble_mentions_hibernation_and_resume() {
        let p = worker_preamble("/a", "coder", "task");
        assert!(
            p.to_lowercase().contains("hibernate"),
            "missing hibernate wording: {p}"
        );
        assert!(
            p.to_lowercase().contains("resume") || p.to_lowercase().contains("resurr"),
            "missing resume wording: {p}"
        );
    }
}
