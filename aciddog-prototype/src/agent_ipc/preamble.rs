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
pub fn cli_preamble(
    binary: &str,
    contactable: &[&str],
    creatable: &[&str],
    memory_banks: &[&str],
) -> String {
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
        "task <add|list|show|update|remove|take|note|todo|comment>",
        "shared team task board. Use `task add --title \"<t>\" [--description ...] [--assignee <id>] \
         [--blocked-by <dep-uuid>]...` to create work (repeat `--blocked-by` for multiple deps), \
         `task list [--mine|--assignee <id>|--status <s>|--unassigned]` to see the table (the `deps` \
         column shows `blocked(n)` or `ready(n)` for tasks with dependencies), \
         `task show --id <uuid>` to print a task's full body — description, dependencies with their \
         current status, the complete note log, the TODOs board, and any comments, \
         `task update --id <uuid> [--title ...] [--description ...] [--status open|assigned|in_progress|done|cancelled] \
         [--assignee <id>|--unassign] [--note \"<context>\"] [--add-blocked-by <uuid>]... [--remove-blocked-by <uuid>]...` \
         to change it or hand it off, `task take` to atomically claim the oldest unblocked unassigned task \
         (it moves to `in_progress` assigned to you — tasks with any open dependency are skipped until \
         every dep reaches `done`), `task remove --id <uuid>` to drop it (also prunes the id from any \
         dependents' lists), and `task note --id <uuid> --body \"<text>\"` to append a log entry. \
         The board is team-wide — there is no allowlist. Always attach a `--note` when reassigning so \
         the next owner sees context (e.g. \"ready for test, commit abc123\"); run `task show` when you \
         pick up a handed-off task to read prior notes, TODOs, and comments.",
    );
    push_line(
        &mut out,
        "  **TODOs** (`task todo <add|list|update|remove>`) — break a task into sub-items and track \
         progress. `task todo add --task <uuid> --body \"<step>\"` appends a pending item; `task todo list \
         --task <uuid>` shows the board (glyphs: `□` pending · `◧` in-progress · `■` completed); \
         `task todo update --task <uuid> --id <todo-uuid> --status in_progress|completed|pending \
         [--body \"<revised>\"]` advances it; `task todo remove --task <uuid> --id <todo-uuid>` drops it. \
         TODOs are a flat checklist — no nesting or dependencies between items. Use them to keep \
         supervising agents in the loop on multi-step work.",
    );
    push_line(
        &mut out,
        "  **Comments** (`task comment <add|list|update|remove>`) — leave a message on a task for the \
         next agent. Distinct from notes (the system's event log): comments are intentional messages \
         authored by you for other agents. Use `task comment add --task <uuid> --body \"<text>\"` when \
         you're handing work off and the next owner needs specific context (e.g. \"tests pass, but \
         watch for flake X in CI\"). `update` / `remove` let you revise or withdraw your own comments.",
    );
    if memory_banks.is_empty() {
        push_line(
            &mut out,
            "- `memory search` / `memory store` / `memory link` / `memory links` / \
             `memory unlink` — disabled: you have no memory banks allowlisted on \
             your agent/worker config.",
        );
    } else {
        push_section(
            &mut out,
            binary,
            "memory search --bank <bank-id> [--query \"<text>\"] [--title-contains \"<frag>\"] \
             [--include-tag <tag>]... [--exclude-tag <tag>]... [--limit N]",
            "search a memory bank. With `--query` the search runs through the semantic \
             index (ranked by vector distance); without it, results come back newest-first. \
             `--title-contains` matches the title case-insensitively, `--include-tag` \
             (repeatable) requires the entry to carry every listed tag, `--exclude-tag` \
             filters out any entry with that tag. Default `--limit` is 5.",
        );
        push_section(
            &mut out,
            binary,
            "memory store --bank <bank-id> --body \"<text>\" [--title \"<short label>\"] \
             [--tag <tag>]...",
            "persist an entry. `--title` is a short label you can search with \
             `--title-contains`; `--tag` (repeatable) attaches labels you can later \
             intersect or exclude. Tags are lower-cased and de-duplicated on store. \
             The reply carries the new entry's uuid — keep it if you plan to link \
             from/to it. Entries are permanent.",
        );
        push_section(
            &mut out,
            binary,
            "memory link --bank <bank-id> --from <entry-uuid> --to <entry-uuid> [--kind <label>]",
            "create a directed link between two existing entries. Use `--kind` to \
             classify the relationship (e.g. `supersedes`, `see_also`, `contradicts`). \
             Both ids must come from prior `memory store` / `memory search` calls.",
        );
        push_section(
            &mut out,
            binary,
            "memory links --bank <bank-id> --id <entry-uuid> [--direction out|in|both] \
             [--include-tag <tag>]... [--exclude-tag <tag>]...",
            "list every link touching an entry. Default direction is `both`; use \
             `out` for entries this one points at and `in` for entries that point at it. \
             `--include-tag` / `--exclude-tag` filter peers by their tag set. \
             Each line carries the peer's full uuid, optional kind, title, tags, and body.",
        );
        push_section(
            &mut out,
            binary,
            "memory unlink --bank <bank-id> --from <entry-uuid> --to <entry-uuid>",
            "remove a directed link. Idempotent — removing a missing link just reports \
             that there was nothing to remove.",
        );
        push_line(
            &mut out,
            &format!(
                "  You may access these banks: {}. Each is scoped to a topic — \
                 see the `<memory>` block in your first-turn prompt for descriptions.",
                memory_banks.join(", ")
            ),
        );
    }
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

/// Tag-based variant of [`cli_preamble`], used when the agent has no
/// shell tool available. Teaches the model to emit
/// `<aciddog-call>{...json...}</aciddog-call>` blocks which the core
/// intercepts and dispatches through the same handlers as CLI requests.
/// Responses arrive on the next turn wrapped in
/// `<aciddog-response index="N">{...}</aciddog-response>` blocks.
pub fn tag_preamble(
    contactable: &[&str],
    creatable: &[&str],
    memory_banks: &[&str],
) -> String {
    let mut out = String::new();
    push_line(&mut out, "## aciddog tag protocol");
    push_line(
        &mut out,
        "You do not have a shell tool, so you cannot invoke the aciddog CLI. \
         Instead, to interact with the agent orchestrator running this session, \
         emit one or more `<aciddog-call>` blocks inside your assistant message. \
         Each block's body is a single JSON object whose `cmd` field names the \
         command. After your turn ends, the orchestrator dispatches each call \
         and the next user turn will contain one `<aciddog-response index=\"N\">` \
         block per call, in the same order.",
    );
    push_line(
        &mut out,
        "**Format:** `<aciddog-call>{\"cmd\":\"<name>\", <fields...>}</aciddog-call>`. \
         One JSON object per tag. Do not wrap tags in triple-backtick code fences \
         — fenced content is ignored so you can quote examples safely.",
    );
    push_line(
        &mut out,
        "**Responses:** `<aciddog-response index=\"N\">{\"ok\":true,\"message\":\"...\"}</aciddog-response>` \
         on success, `{\"ok\":false,\"error\":\"...\"}` on failure. Malformed JSON \
         or unknown commands come back as `ok:false` errors — correct and retry \
         on the next turn rather than abandoning the task.",
    );
    push_line(
        &mut out,
        "**Chain limit:** the call → response → call loop stops after 8 consecutive \
         rounds per user turn. If you hit the cap, wait for the next user prompt \
         instead of emitting more calls.",
    );

    push_line(&mut out, "### Commands");

    push_tag_section(
        &mut out,
        "test",
        r#"{"cmd":"test"}"#,
        "verify connectivity to the agent orchestrator.",
    );

    if contactable.is_empty() {
        push_line(
            &mut out,
            "- `send_message` — disabled: you may not message any other agent on this team.",
        );
    } else {
        push_tag_section(
            &mut out,
            "send_message",
            r#"{"cmd":"send_message","to":"<agent-id>","body":"<text>"}"#,
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
            "- `worker_create` — disabled: you may not spawn workers.",
        );
    } else {
        push_tag_section(
            &mut out,
            "worker_create",
            r#"{"cmd":"worker_create","template":"<template-id>","task":"<initial task>"}"#,
            "spawn an ephemeral worker from a declared template. The response \
             message carries the new worker's display id and session UUID; use \
             the display id with `send_message.to` to message the worker.",
        );
        push_line(
            &mut out,
            &format!("  You are allowed to spawn: {}", creatable.join(", ")),
        );
    }

    push_task_board_tag_docs(&mut out);

    push_memory_tag_docs(&mut out, memory_banks);

    out
}

/// Worker-only tag preamble — injected before `tag_preamble` when a
/// worker spawns without a shell tool. Documents `worker_complete` so
/// the worker can report its final result.
pub fn tag_worker_preamble(parent_id: &str, task: &str) -> String {
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
        &format!(
            "- Progress updates: emit `<aciddog-call>{{\"cmd\":\"send_message\",\"to\":\"{parent_id}\",\"body\":\"<text>\"}}</aciddog-call>`."
        ),
    );
    push_line(
        &mut out,
        "- When finished, emit `<aciddog-call>{\"cmd\":\"worker_complete\",\"body\":\"<final report>\"}</aciddog-call>`. \
         This delivers your final report to your parent and hibernates you. After \
         that call, stop emitting further actions; your parent may resume the \
         conversation later if they need more from you.",
    );
    out
}

fn push_tag_section(out: &mut String, name: &str, example: &str, description: &str) {
    push_line(
        out,
        &format!("- `{name}` — {description}\n  Example: `<aciddog-call>{example}</aciddog-call>`"),
    );
}

fn push_task_board_tag_docs(out: &mut String) {
    push_line(
        out,
        "### Shared task board",
    );
    push_line(
        out,
        "The task board is team-wide (no allowlist). Use these commands to \
         coordinate work and track progress:",
    );
    push_line(
        out,
        r#"- `task_add` — `<aciddog-call>{"cmd":"task_add","title":"<t>","description":"<optional>","assignee":"<optional agent id>","blocked_by":["<dep-uuid>", ...]}</aciddog-call>`. Create work; repeat blocked_by entries for multiple deps."#,
    );
    push_line(
        out,
        r#"- `task_list` — `<aciddog-call>{"cmd":"task_list","mine":true}</aciddog-call>` or pass `"assignee":"<id>"`, `"status":"<s>"`, `"unassigned":true`. The `deps` column shows `blocked(n)` / `ready(n)`."#,
    );
    push_line(
        out,
        r#"- `task_show` — `<aciddog-call>{"cmd":"task_show","id":"<uuid>"}</aciddog-call>`. Full body: description, deps with status, note log, TODOs, comments."#,
    );
    push_line(
        out,
        r#"- `task_update` — `<aciddog-call>{"cmd":"task_update","id":"<uuid>","title":"...","description":"...","status":"open|assigned|in_progress|done|cancelled","assignee":"<id>","unassign":true,"note":"<context>","add_blocked_by":["<uuid>"],"remove_blocked_by":["<uuid>"]}</aciddog-call>`. Omit fields you don't want to change. Always include `note` when reassigning."#,
    );
    push_line(
        out,
        r#"- `task_take` — `<aciddog-call>{"cmd":"task_take"}</aciddog-call>`. Atomically claim the oldest unblocked unassigned task; it moves to `in_progress` assigned to you."#,
    );
    push_line(
        out,
        r#"- `task_remove` — `<aciddog-call>{"cmd":"task_remove","id":"<uuid>"}</aciddog-call>`. Also prunes the id from any dependents' lists."#,
    );
    push_line(
        out,
        r#"- `task_note` — `<aciddog-call>{"cmd":"task_note","id":"<uuid>","body":"<text>"}</aciddog-call>`. Append a log entry."#,
    );
    push_line(
        out,
        r#"**TODOs** break a task into sub-items (flat checklist, no nesting): `task_todo_add`, `task_todo_list`, `task_todo_update` (status: `pending|in_progress|completed`), `task_todo_remove`. Each takes a `task` uuid plus the per-command fields."#,
    );
    push_line(
        out,
        r#"**Comments** are intentional messages for the next agent (distinct from `note`, the system's event log): `task_comment_add`, `task_comment_list`, `task_comment_update`, `task_comment_remove`. Each takes a `task` uuid plus its fields."#,
    );
}

fn push_memory_tag_docs(out: &mut String, memory_banks: &[&str]) {
    if memory_banks.is_empty() {
        push_line(
            out,
            "- `memory_search` / `memory_store` / `memory_link` / `memory_links` / \
             `memory_unlink` — disabled: you have no memory banks allowlisted on \
             your agent/worker config.",
        );
        return;
    }
    push_line(out, "### Semantic memory banks");
    push_line(
        out,
        "Each bank is a persistent, embedding-indexed store of free-form entries, \
         plus a directed link graph between them. Search when you need context; \
         store when you learn something worth persisting; link when two entries \
         belong together (e.g. a follow-up fact superseding an older one).",
    );
    push_tag_section(
        out,
        "memory_search",
        r#"{"cmd":"memory_search","bank":"<bank-id>","query":"<text>","title_contains":"<frag>","include_tags":["<tag>"],"exclude_tags":["<tag>"],"limit":5}"#,
        "search a bank. `query` triggers semantic search; omit it for a pure \
         tag/title filter (newest first). `title_contains` is a case-insensitive \
         substring. `include_tags` requires every listed tag on the entry; \
         `exclude_tags` disqualifies entries with any listed tag. Any field \
         may be omitted.",
    );
    push_tag_section(
        out,
        "memory_store",
        r#"{"cmd":"memory_store","bank":"<bank-id>","body":"<text>","title":"<short label>","tags":["<tag>","<tag>"]}"#,
        "persist an entry. `title` and `tags` are optional; tags are lower-cased \
         and de-duplicated on store. The success reply carries the new entry's \
         uuid — keep it if you plan to link from/to it.",
    );
    push_tag_section(
        out,
        "memory_link",
        r#"{"cmd":"memory_link","bank":"<bank-id>","from":"<uuid>","to":"<uuid>","kind":"supersedes"}"#,
        "create a directed link between two existing entries. `kind` is optional \
         free text classifying the relationship (e.g. `supersedes`, `see_also`, \
         `contradicts`).",
    );
    push_tag_section(
        out,
        "memory_links",
        r#"{"cmd":"memory_links","bank":"<bank-id>","id":"<uuid>","direction":"both","include_tags":["<tag>"],"exclude_tags":["<tag>"]}"#,
        "list every link touching an entry. `direction` is `out`, `in`, or `both` \
         (default). `include_tags` / `exclude_tags` filter peers by their tag set. \
         Response lists peer id, optional kind, title, tags, and body.",
    );
    push_tag_section(
        out,
        "memory_unlink",
        r#"{"cmd":"memory_unlink","bank":"<bank-id>","from":"<uuid>","to":"<uuid>"}"#,
        "remove a directed link. Idempotent.",
    );
    push_line(
        out,
        &format!(
            "You may access these banks: {}. Descriptions are in your `<memory>` block.",
            memory_banks.join(", ")
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preamble_mentions_test_subcommand_with_binary_path() {
        let p = cli_preamble("/opt/aciddog/bin/aciddog", &["coder"], &[], &[]);
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
        let p = cli_preamble("/usr/local/bin/aciddog", &["coder"], &[], &[]);
        assert!(p.starts_with("## aciddog agent CLI"), "missing header: {p}");
    }

    #[test]
    fn preamble_surfaces_binary_path_explicitly() {
        let p = cli_preamble("/tmp/weird path/aciddog", &["coder"], &[], &[]);
        assert!(
            p.contains("Binary path: `/tmp/weird path/aciddog`"),
            "binary path callout missing: {p}"
        );
    }

    #[test]
    fn preamble_lists_only_contactable_ids() {
        let p = cli_preamble("/a", &["coder", "reviewer"], &[], &[]);
        assert!(
            p.contains("coder, reviewer"),
            "missing contactable ids: {p}"
        );
    }

    #[test]
    fn preamble_mentions_send_subcommand_and_wrapping_format() {
        let p = cli_preamble("/a", &["coder"], &[], &[]);
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
        let p = cli_preamble("/a", &[], &[], &[]);
        assert!(!p.contains("/a send"), "send should be hidden: {p}");
        assert!(
            p.contains("- `send` — disabled"),
            "should note send is disabled: {p}"
        );
    }

    #[test]
    fn preamble_mentions_worker_create_when_creatable() {
        let p = cli_preamble("/a", &["coder"], &["researcher", "tester"], &[]);
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
        let p = cli_preamble("/a", &["coder"], &[], &[]);
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
        let p = cli_preamble("/a", &["coder"], &["researcher"], &[]);
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
        let p = cli_preamble("/a", &[], &[], &[]);
        assert!(
            p.contains("/a task <add|list|show|update|remove|take|note|todo|comment>"),
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
        let p = cli_preamble("/a", &[], &[], &[]);
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

    // ── tag_preamble ────────────────────────────────────────────────

    #[test]
    fn tag_preamble_has_header_and_format_note() {
        let p = tag_preamble(&["coder"], &[], &[]);
        assert!(p.starts_with("## aciddog tag protocol"), "header: {p}");
        assert!(
            p.contains("<aciddog-call>"),
            "must document call tag: {p}"
        );
        assert!(
            p.contains("<aciddog-response"),
            "must document response tag: {p}"
        );
    }

    #[test]
    fn tag_preamble_mentions_chain_cap() {
        let p = tag_preamble(&["coder"], &[], &[]);
        assert!(
            p.contains("8 consecutive"),
            "must tell the model about the 8-round cap: {p}"
        );
    }

    #[test]
    fn tag_preamble_lists_contactable_ids() {
        let p = tag_preamble(&["coder", "reviewer"], &[], &[]);
        assert!(
            p.contains("coder, reviewer"),
            "missing contactable ids: {p}"
        );
        assert!(
            p.contains("\"cmd\":\"send_message\""),
            "missing send_message example: {p}"
        );
    }

    #[test]
    fn tag_preamble_hides_send_when_allowlist_empty() {
        let p = tag_preamble(&[], &[], &[]);
        assert!(
            !p.contains("\"cmd\":\"send_message\""),
            "send_message example must be hidden: {p}"
        );
        assert!(
            p.contains("- `send_message` — disabled"),
            "should note disabled: {p}"
        );
    }

    #[test]
    fn tag_preamble_mentions_worker_create_when_allowed() {
        let p = tag_preamble(&["coder"], &["researcher", "tester"], &[]);
        assert!(
            p.contains("\"cmd\":\"worker_create\""),
            "missing worker_create example: {p}"
        );
        assert!(
            p.contains("researcher, tester"),
            "missing creatable ids: {p}"
        );
    }

    #[test]
    fn tag_preamble_hides_worker_create_when_empty() {
        let p = tag_preamble(&["coder"], &[], &[]);
        assert!(
            !p.contains("\"cmd\":\"worker_create\""),
            "worker_create must be hidden: {p}"
        );
        assert!(
            p.contains("- `worker_create` — disabled"),
            "should note disabled: {p}"
        );
    }

    #[test]
    fn tag_preamble_never_mentions_worker_complete_for_plain_agents() {
        let p = tag_preamble(&["coder"], &["researcher"], &[]);
        assert!(
            !p.contains("worker_complete"),
            "plain agents must not see worker_complete: {p}"
        );
    }

    #[test]
    fn tag_preamble_has_task_board_unconditionally() {
        let p = tag_preamble(&[], &[], &[]);
        assert!(
            p.contains("task_add"),
            "task board must be unconditional: {p}"
        );
        assert!(p.contains("task_take"), "missing task_take: {p}");
    }

    #[test]
    fn tag_worker_preamble_documents_worker_complete() {
        let p = tag_worker_preamble("coder", "find FOO");
        assert!(p.contains("spawned by `coder`"), "missing parent: {p}");
        assert!(p.contains("find FOO"), "missing task: {p}");
        assert!(
            p.contains("\"cmd\":\"worker_complete\""),
            "missing worker_complete example: {p}"
        );
        assert!(
            p.contains("\"cmd\":\"send_message\""),
            "missing progress send example: {p}"
        );
    }

    #[test]
    fn tag_worker_preamble_mentions_hibernation() {
        let p = tag_worker_preamble("coder", "task");
        assert!(
            p.to_lowercase().contains("hibernate"),
            "missing hibernate wording: {p}"
        );
    }
}
