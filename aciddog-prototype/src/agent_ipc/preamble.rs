//! System-generated `<tools>` preamble describing the aciddog command
//! surface. Two modes share the same structure (header → grouped
//! markdown tables → load-bearing prose notes):
//!
//! - [`cli_preamble`] — agents with a shell tool call the aciddog
//!   binary directly.
//! - [`tag_preamble`] — agents without a shell tool emit
//!   `<aciddog-call>` blocks the orchestrator intercepts.
//!
//! Disabled commands are **omitted** from the tables rather than listed
//! as disabled stubs; the contactable / creatable / memory_banks
//! allowlists drop whole groups when empty.
//!
//! Worker-only variants ([`worker_preamble`], [`tag_worker_preamble`])
//! inject the identity / task / hibernation contract; the
//! `worker complete` / `worker_complete` call is documented there (and
//! only there) so plain agents can't accidentally reach for it.
//!
//! Extending: add a row to the appropriate group inside `cli_preamble`
//! or `tag_preamble`, matching the existing param-column convention
//! (`--flag <val>` / `[--flag]` / `[--flag]…` for CLI; `"key":"<val>"`
//! / `["key":"<val>"]` for tag).

/// Describes one channel an agent belongs to, for preamble rendering.
/// Matches the minimum set of fields needed to tell the model what
/// rooms exist and who else is listening.
#[derive(Debug, Clone)]
pub struct ChannelInfo<'a> {
    pub id: &'a str,
    pub description: Option<&'a str>,
    /// Other members in the channel, excluding the caller. Used verbatim
    /// so the model can pick a specific person to address if needed.
    pub other_members: Vec<&'a str>,
}

/// Reserved recipient id used by the human operator's inbox. Listed
/// alongside `contactable` peers in the Messaging allowlist so every
/// agent always has a way to reach the user.
const USER_RECIPIENT_ID: &str = "user";

/// CLI-mode preamble: the agent has a shell tool and invokes the
/// aciddog binary directly. `include_user_recipient` is true for
/// configured agents/automatons (who may always mail `user`) and
/// false for workers (scoped to their parent).
pub fn cli_preamble(
    binary: &str,
    contactable: &[&str],
    creatable: &[&str],
    memory_banks: &[&str],
    skills: &[&str],
    channels: &[ChannelInfo<'_>],
    include_user_recipient: bool,
) -> String {
    let mut out = String::new();
    push_line(&mut out, "## aciddog tools");
    push_line(
        &mut out,
        &format!(
            "Invoke via the absolute binary path `{binary}` (it may not be on \
             your `PATH`). Output comes back on stdout. Call `{binary} test` \
             to verify connectivity before relying on the rest."
        ),
    );

    push_group(
        &mut out,
        "Meta",
        &[Row {
            cmd: "info",
            params: "—",
            desc: "Orchestrator local time, UTC time, and IANA timezone. \
                   Call before scheduling a cron job — never shell out to `date`.",
        }],
        None,
    );

    let messaging_recipients: Vec<&str> = if include_user_recipient {
        std::iter::once(USER_RECIPIENT_ID)
            .chain(contactable.iter().copied())
            .collect()
    } else {
        contactable.to_vec()
    };
    if !messaging_recipients.is_empty() {
        let user_note = if include_user_recipient {
            " `user` is the human operator's inbox — send when you need their \
              input and continue other work; they may take a long time to reply, \
              so do not block or poll."
        } else {
            ""
        };
        push_group(
            &mut out,
            "Messaging",
            &[Row {
                cmd: "send",
                params: "`--to <agent-id>` `--body \"<text>\"`",
                desc: "Chat another agent on the team.",
            }],
            Some(&format!(
                "You may message: {}. Inbound messages arrive wrapped as \
                 `<Message from=\"sender-id\">body</Message>`; plain user text \
                 has no wrapper. Messages that land while you are busy are \
                 queued and delivered on your next turn — user messages \
                 always take priority over queued agent messages.{user_note}",
                messaging_recipients.join(", ")
            )),
        );
    }

    {
        let send_row = Row {
            cmd: "channel send",
            params: "`--id <channel-id>` `--body \"<text>\"`",
            desc: "Broadcast to every other member of a channel you belong to. \
                   This is the ONLY way to reply to a `<ChannelMessage>` you \
                   receive — direct `send --to` puts the text in a single \
                   agent's mailbox and other channel members never see it.",
        };
        let management_rows = [
            Row {
                cmd: "channel create",
                params: "`--id <channel-id>`",
                desc: "Create an ephemeral meeting channel. You are auto-joined. \
                       Dies when `channel end`-ed or on aciddog exit.",
            },
            Row {
                cmd: "channel join",
                params: "`--id <channel-id>`",
                desc: "Add yourself to a channel.",
            },
            Row {
                cmd: "channel add",
                params: "`--id <channel-id>` `--member <agent-id>`",
                desc: "Add another agent to a channel you are a member of.",
            },
            Row {
                cmd: "channel kick",
                params: "`--id <channel-id>` `--member <agent-id>`",
                desc: "Remove a member from a channel you are a member of.",
            },
            Row {
                cmd: "channel end",
                params: "`--id <channel-id>`",
                desc: "Destroy an ephemeral meeting channel you are in. \
                       Rejects WCL-declared channels.",
            },
        ];
        if channels.is_empty() {
            // Include `channel send` here too — another agent can add
            // you to a channel at runtime, and when that happens the
            // system-notice tells you you're in the channel but the
            // preamble is already fixed. Documenting the syntax up
            // front means the tool is always there when you need it.
            let rows: Vec<Row> = std::iter::once(send_row.clone())
                .chain(management_rows.iter().cloned())
                .collect();
            push_group(
                &mut out,
                "Channels",
                &rows,
                Some(
                    "You are not currently in any channel. You can still spin up \
                     an ephemeral meeting with `channel create`, or another agent \
                     may add you to one at runtime — if a `<ChannelMessage …>` \
                     payload ever arrives, reply with `channel send --id <id> …` \
                     (NOT `send --to …`). Runtime membership changes \
                     (create/join/add/kick/end) do NOT persist across restarts.",
                ),
            );
        } else {
            let roster = channels
                .iter()
                .map(|c| {
                    if c.other_members.is_empty() {
                        format!("#{} (you are the only member)", c.id)
                    } else {
                        format!("#{} (with {})", c.id, c.other_members.join(", "))
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            let rows: Vec<Row> = std::iter::once(send_row.clone())
                .chain(management_rows.iter().cloned())
                .collect();
            push_group(
                &mut out,
                "Channels",
                &rows,
                Some(&format!(
                    "You are in: {roster}. Channel messages arrive wrapped as \
                     `<ChannelMessage channel=\"<id>\" from=\"<sender>\">body</ChannelMessage>` \
                     and are delivered at LOWER priority than direct `<Message>` \
                     mailbox traffic. To target a subset of the channel, prefix \
                     the body with one or more `@name` tokens (e.g. \
                     `@planner @coder …`) — only those members receive the \
                     message; unmentioned members stay silent. With no leading \
                     `@` the message broadcasts to everyone. Only post when you \
                     have something substantive to add — do NOT reply just to \
                     acknowledge, do NOT echo each other in a loop. If a post \
                     isn't for you, stay silent. Runtime membership changes \
                     (create/join/add/kick/end) do NOT persist across restarts."
                )),
            );
        }
    }

    if !creatable.is_empty() {
        push_group(
            &mut out,
            "Workers",
            &[
                Row {
                    cmd: "worker create",
                    params: "`--template <id>` `--task \"<initial task>\"` [`--properties '<json>'`]",
                    desc: "Spawn an ephemeral worker. Response carries the new \
                           worker's display id (use with `send --to`) and session \
                           UUID (retain to resume if it hibernates). \
                           `--properties` is a JSON object required by \
                           remote-worker templates (e.g. Jules needs \
                           `{\"repo\":\"owner/repo\",\"branch\":\"main\"}`); \
                           ignored by local templates.",
                },
                Row {
                    cmd: "worker archive",
                    params: "`--id <worker-display-id>`",
                    desc: "Close out a worker you spawned. Hibernates it \
                           locally and tears down any remote backend session \
                           (Jules). Only the parent can archive.",
                },
            ],
            Some(&format!(
                "You may spawn: {}. Remote-worker templates (those backed by \
                 the `jules` core) behave differently from local workers: \
                 they stay alive after each `COMPLETED`/`FAILED` mail so you \
                 can send follow-up messages to request more changes on the \
                 same session (even after a PR is opened). Reply to worker \
                 mail with body exactly `approve` to accept a pending plan; \
                 any other body is forwarded as feedback or the next task. \
                 Call `worker archive --id <worker>` when you're finished \
                 with the remote work — that closes the remote session. \
                 Remote-worker `properties` may include `\"autoCreatePr\": \
                 true` (Jules auto-opens the PR when changes are ready, no \
                 mid-task confirmation) or `\"autoCreatePr\": false` (Jules \
                 pauses and asks before opening each PR — you must answer \
                 via `send` because there is no dedicated \"create PR\" \
                 API). Default comes from the core config.",
                creatable.join(", ")
            )),
        );
    }

    push_group(
        &mut out,
        "Task board",
        &[
            Row {
                cmd: "task add",
                params: "`--title \"<t>\"` [`--description \"…\"`] [`--assignee <id>`] [`--blocked-by <uuid>`]…",
                desc: "Create work. Repeat `--blocked-by` for multiple deps.",
            },
            Row {
                cmd: "task list",
                params: "[`--mine` \\| `--assignee <id>` \\| `--status <s>` \\| `--unassigned`]",
                desc: "Show the table. `deps` column reads `blocked(n)` / `ready(n)`.",
            },
            Row {
                cmd: "task show",
                params: "`--id <uuid>`",
                desc: "Full body: description, deps + their status, note log, TODOs, comments.",
            },
            Row {
                cmd: "task update",
                params: "`--id <uuid>` [`--title` / `--description` / `--status open\\|assigned\\|in_progress\\|done\\|cancelled` / `--assignee <id>` \\| `--unassign` / `--note \"…\"` / `--add-blocked-by <uuid>`… / `--remove-blocked-by <uuid>`…]",
                desc: "Change fields. Always attach `--note` when reassigning so the next owner sees context.",
            },
            Row {
                cmd: "task take",
                params: "—",
                desc: "Atomically claim the oldest unblocked unassigned task (moves to `in_progress`, assigned to you).",
            },
            Row {
                cmd: "task remove",
                params: "`--id <uuid>`",
                desc: "Delete; also prunes the id from any dependents' blocked-by lists.",
            },
            Row {
                cmd: "task note",
                params: "`--id <uuid>` `--body \"<text>\"`",
                desc: "Append a log entry (system event log).",
            },
            Row {
                cmd: "task todo <add\\|list\\|update\\|remove>",
                params: "`--task <uuid>` + per-action fields (`update --status pending\\|in_progress\\|completed`)",
                desc: "Flat sub-checklist per task (glyphs: `□` · `◧` · `■`).",
            },
            Row {
                cmd: "task comment <add\\|list\\|update\\|remove>",
                params: "`--task <uuid>` + per-action fields",
                desc: "Intentional messages to the next owner — distinct from `note` (the system's event log).",
            },
        ],
        Some("Board is team-wide; no allowlist. Run `task show` when picking up a handed-off task to read prior notes, TODOs, and comments."),
    );

    if !memory_banks.is_empty() {
        push_group(
            &mut out,
            "Memory banks",
            &[
                Row {
                    cmd: "memory search",
                    params: "`--bank <id>` [`--query \"<text>\"`] [`--title-contains \"<frag>\"`] [`--include-tag <tag>`]… [`--exclude-tag <tag>`]… [`--limit N`]",
                    desc: "With `--query`: semantic search (vector distance). Without: newest-first tag/title filter. Default `--limit` is 5.",
                },
                Row {
                    cmd: "memory store",
                    params: "`--bank <id>` `--body \"<text>\"` [`--title \"<label>\"`] [`--tag <tag>`]…",
                    desc: "Persist an entry (permanent). Reply carries the new entry's uuid. Tags lower-cased and de-duplicated on store.",
                },
                Row {
                    cmd: "memory link",
                    params: "`--bank <id>` `--from <uuid>` `--to <uuid>` [`--kind <label>`]",
                    desc: "Directed link between two stored entries. `--kind` classifies (e.g. `supersedes`, `see_also`, `contradicts`).",
                },
                Row {
                    cmd: "memory links",
                    params: "`--bank <id>` `--id <uuid>` [`--direction out\\|in\\|both`] [`--include-tag <tag>`]… [`--exclude-tag <tag>`]…",
                    desc: "List every link touching an entry (default `both`). Peer uuid, kind, title, tags, body per line.",
                },
                Row {
                    cmd: "memory unlink",
                    params: "`--bank <id>` `--from <uuid>` `--to <uuid>`",
                    desc: "Remove a directed link. Idempotent.",
                },
            ],
            Some(&format!(
                "You may access these banks: {}. Each is topic-scoped — see the `<memory>` block for descriptions.",
                memory_banks.join(", ")
            )),
        );
    }

    if !skills.is_empty() {
        push_group(
            &mut out,
            "Skills",
            &[
                Row {
                    cmd: "skill list",
                    params: "—",
                    desc: "List every skill assigned to you as \
                           `- <id> — <description>` lines. Call this \
                           first to discover what is available; skill \
                           ids are not pre-announced in the prompt.",
                },
                Row {
                    cmd: "skill search",
                    params: "`--query \"<text>\"`",
                    desc: "Case-insensitive substring match over \
                           assigned skill ids and descriptions. Output \
                           matches the shape of `skill list`.",
                },
                Row {
                    cmd: "skill run",
                    params: "`--name <id>`",
                    desc: "Print the instructions you must follow to \
                           run the named skill. Output is its \
                           `SKILL.md` prefixed with \
                           `{baseDir} = <abs-path>` on its own line \
                           followed by a blank line.",
                },
            ],
            Some(
                "Call `skill list` or `skill search` to discover what \
                 is assigned before running. Treat `{baseDir}` as a \
                 literal placeholder — wherever the loaded `SKILL.md` \
                 references `{baseDir}`, substitute the absolute path \
                 printed in the header.",
            ),
        );
    }

    push_group(
        &mut out,
        "Cron",
        &[
            Row {
                cmd: "cron add",
                params: "`--target <agent-id>` `--kind message\\|command` `--payload \"<text-or-/cmd>\"` `--schedule \"<expr>\"` [`--tz utc\\|local`] [`--oneoff`] [`--catchup-on-startup`]",
                desc: "Recurring: 5-field POSIX cron `min hour dom mon dow`. One-off: pass `--oneoff` + ISO `YYYY-MM-DDTHH:MM[:SS]`.",
            },
            Row {
                cmd: "cron list",
                params: "[`--target <id>`] [`--include-disabled`] [`--mine`]",
                desc: "List jobs; filters optional.",
            },
            Row {
                cmd: "cron show",
                params: "`--id <uuid>`",
                desc: "Full body: target, payload, schedule, tz, catchup, last/next run, creator.",
            },
            Row {
                cmd: "cron update",
                params: "`--id <uuid>` [any of `cron add`'s flags]",
                desc: "Change fields. Updating schedule / tz / oneoff recomputes `next_run` from now.",
            },
            Row {
                cmd: "cron remove",
                params: "`--id <uuid>`",
                desc: "Delete a job.",
            },
        ],
        Some(
            "`--tz` defaults to `local` (the box running aciddog — call `info` \
             to resolve). `/exit` and `/quit` are blocked as slash payloads. \
             Target must be a configured agent (workers cannot be cron \
             targets). Cross-agent targeting is gated by the same allowlist \
             as `send`. Workers cannot use `cron` at all. `--catchup-on-startup` \
             (one-off only) re-fires a missed instant on next start — without \
             it, missed one-offs are pruned during reconciliation.",
        ),
    );

    out
}

/// Tag-mode preamble: the agent has no shell tool and emits
/// `<aciddog-call>` blocks the core intercepts. `include_user_recipient`
/// is true for configured agents/automatons (who may always message
/// `user`) and false for workers (scoped to their parent).
pub fn tag_preamble(
    contactable: &[&str],
    creatable: &[&str],
    memory_banks: &[&str],
    skills: &[&str],
    channels: &[ChannelInfo<'_>],
    include_user_recipient: bool,
) -> String {
    let mut out = String::new();
    push_line(&mut out, "## aciddog tools");
    push_line(
        &mut out,
        "Call these tools by emitting \
         `<aciddog-call>{\"cmd\":\"<name>\", <fields>…}</aciddog-call>` blocks \
         inside your assistant message. One JSON object per tag; don't wrap \
         tags in triple backticks (fenced content is ignored so you can quote \
         examples safely). After your turn ends, each call returns on the \
         next turn as \
         `<aciddog-response index=\"N\">{\"ok\":true,\"message\":\"…\"}</aciddog-response>` \
         on success or `{\"ok\":false,\"error\":\"…\"}` on failure, in the \
         order you issued them. Malformed JSON or unknown commands come back \
         as errors — correct and retry rather than abandoning the task. The \
         call → response loop stops after 8 consecutive rounds per user turn; \
         if you hit the cap, wait for the next user prompt.",
    );
    push_line(
        &mut out,
        "Run `test` (`{\"cmd\":\"test\"}`) to verify connectivity before \
         relying on the rest.",
    );

    push_group(
        &mut out,
        "Meta",
        &[Row {
            cmd: "info",
            params: "—",
            desc: "Orchestrator local time, UTC time, and IANA timezone. Call before scheduling a cron job.",
        }],
        None,
    );

    let messaging_recipients: Vec<&str> = if include_user_recipient {
        std::iter::once(USER_RECIPIENT_ID)
            .chain(contactable.iter().copied())
            .collect()
    } else {
        contactable.to_vec()
    };
    if !messaging_recipients.is_empty() {
        let user_note = if include_user_recipient {
            " `user` is the human operator's inbox — send when you need their \
              input and continue other work; they may take a long time to reply, \
              so do not block or poll."
        } else {
            ""
        };
        push_group(
            &mut out,
            "Messaging",
            &[Row {
                cmd: "send_message",
                params: "`\"to\":\"<agent-id>\"`, `\"body\":\"<text>\"`",
                desc: "Chat another agent on the team.",
            }],
            Some(&format!(
                "You may message: {}. Inbound messages arrive wrapped as \
                 `<Message from=\"sender-id\">body</Message>`; plain user \
                 text has no wrapper. Messages that land while you are busy \
                 are queued and delivered on your next turn — user messages \
                 always take priority over queued agent messages.{user_note}",
                messaging_recipients.join(", ")
            )),
        );
    }

    {
        let send_row = Row {
            cmd: "channel_send",
            params: "`\"id\":\"<channel-id>\"`, `\"body\":\"<text>\"`",
            desc: "Broadcast to every other member of a channel you belong to. \
                   This is the ONLY way to reply to a `<ChannelMessage>` you \
                   receive — `send_message` puts the text in a single agent's \
                   mailbox and other channel members never see it.",
        };
        let management_rows = [
            Row {
                cmd: "channel_create",
                params: "`\"id\":\"<channel-id>\"`",
                desc: "Create an ephemeral meeting channel. You are auto-joined. Dies when `channel_end`-ed or on aciddog exit.",
            },
            Row {
                cmd: "channel_join",
                params: "`\"id\":\"<channel-id>\"`",
                desc: "Add yourself to a channel.",
            },
            Row {
                cmd: "channel_add",
                params: "`\"id\":\"<channel-id>\"`, `\"member\":\"<agent-id>\"`",
                desc: "Add another agent to a channel you are a member of.",
            },
            Row {
                cmd: "channel_kick",
                params: "`\"id\":\"<channel-id>\"`, `\"member\":\"<agent-id>\"`",
                desc: "Remove a member from a channel you are a member of.",
            },
            Row {
                cmd: "channel_end",
                params: "`\"id\":\"<channel-id>\"`",
                desc: "Destroy an ephemeral meeting channel you are in. Rejects WCL-declared channels.",
            },
        ];
        if channels.is_empty() {
            let rows: Vec<Row> = std::iter::once(send_row.clone())
                .chain(management_rows.iter().cloned())
                .collect();
            push_group(
                &mut out,
                "Channels",
                &rows,
                Some(
                    "You are not currently in any channel. You can still spin up \
                     an ephemeral meeting with `channel_create`, or another agent \
                     may add you to one at runtime — if a `<ChannelMessage …>` \
                     payload ever arrives, reply with `channel_send` (NOT \
                     `send_message`). Runtime membership changes \
                     (create/join/add/kick/end) do NOT persist across restarts.",
                ),
            );
        } else {
            let roster = channels
                .iter()
                .map(|c| {
                    if c.other_members.is_empty() {
                        format!("#{} (you are the only member)", c.id)
                    } else {
                        format!("#{} (with {})", c.id, c.other_members.join(", "))
                    }
                })
                .collect::<Vec<_>>()
                .join("; ");
            let rows: Vec<Row> = std::iter::once(send_row.clone())
                .chain(management_rows.iter().cloned())
                .collect();
            push_group(
                &mut out,
                "Channels",
                &rows,
                Some(&format!(
                    "You are in: {roster}. Channel messages arrive wrapped as \
                     `<ChannelMessage channel=\"<id>\" from=\"<sender>\">body</ChannelMessage>` \
                     and are delivered at LOWER priority than direct `<Message>` \
                     mailbox traffic. To target a subset of the channel, prefix \
                     the body with one or more `@name` tokens (e.g. \
                     `@planner @coder …`) — only those members receive the \
                     message; unmentioned members stay silent. With no leading \
                     `@` the message broadcasts to everyone. Only post when you \
                     have something substantive to add — do NOT reply just to \
                     acknowledge, do NOT echo each other in a loop. If a post \
                     isn't for you, stay silent. Runtime membership changes \
                     (create/join/add/kick/end) do NOT persist across restarts."
                )),
            );
        }
    }

    if !creatable.is_empty() {
        push_group(
            &mut out,
            "Workers",
            &[
                Row {
                    cmd: "worker_create",
                    params: "`\"template\":\"<id>\"`, `\"task\":\"<initial task>\"`, [`\"properties\":{…}`]",
                    desc: "Spawn an ephemeral worker. Response carries display id \
                           (use in `send_message.to`) and session UUID. \
                           `properties` is required for remote-worker templates \
                           (e.g. Jules needs `{\"repo\":\"owner/repo\",\"branch\":\"main\"}`); \
                           ignored by local templates.",
                },
                Row {
                    cmd: "worker_archive",
                    params: "`\"worker\":\"<worker-display-id>\"`",
                    desc: "Close out a worker you spawned. Hibernates it locally \
                           and tears down any remote backend session (Jules). \
                           Only the parent can archive.",
                },
            ],
            Some(&format!(
                "You may spawn: {}. Remote workers (those backed by the \
                 `jules` core) behave differently from local workers: they \
                 stay alive after each `COMPLETED`/`FAILED` mail so you can \
                 send follow-up messages to request more changes on the \
                 same session (even after a PR is opened). Reply via \
                 `send_message` — body exactly `approve` accepts a pending \
                 plan, any other body is forwarded as feedback or the next \
                 task. Call `worker_archive` when you're finished — that \
                 closes the remote session. Remote-worker `properties` may \
                 include `\"autoCreatePr\": true` (Jules auto-opens the PR \
                 when changes are ready, no mid-task confirmation) or \
                 `\"autoCreatePr\": false` (Jules pauses and asks before \
                 opening each PR — you must answer via `send_message` \
                 because there is no dedicated \"create PR\" API). Default \
                 comes from the core config.",
                creatable.join(", ")
            )),
        );
    }

    push_group(
        &mut out,
        "Task board",
        &[
            Row {
                cmd: "task_add",
                params: "`\"title\":\"<t>\"`, [`\"description\":\"…\"`], [`\"assignee\":\"<id>\"`], [`\"blocked_by\":[\"<uuid>\"…]`]",
                desc: "Create work.",
            },
            Row {
                cmd: "task_list",
                params: "[`\"mine\":true`] \\| [`\"assignee\":\"<id>\"`] \\| [`\"status\":\"<s>\"`] \\| [`\"unassigned\":true`]",
                desc: "Show the table. `deps` column reads `blocked(n)` / `ready(n)`.",
            },
            Row {
                cmd: "task_show",
                params: "`\"id\":\"<uuid>\"`",
                desc: "Full body: description, deps + status, note log, TODOs, comments.",
            },
            Row {
                cmd: "task_update",
                params: "`\"id\":\"<uuid>\"` + any of: `\"title\"`, `\"description\"`, `\"status\":\"open\\|assigned\\|in_progress\\|done\\|cancelled\"`, `\"assignee\"` \\| `\"unassign\":true`, `\"note\"`, `\"add_blocked_by\":[…]`, `\"remove_blocked_by\":[…]`",
                desc: "Omit fields you don't want to change. Always include `\"note\"` when reassigning.",
            },
            Row {
                cmd: "task_take",
                params: "—",
                desc: "Atomically claim the oldest unblocked unassigned task (moves to `in_progress`, assigned to you).",
            },
            Row {
                cmd: "task_remove",
                params: "`\"id\":\"<uuid>\"`",
                desc: "Delete; prunes the id from any dependents' blocked-by lists.",
            },
            Row {
                cmd: "task_note",
                params: "`\"id\":\"<uuid>\"`, `\"body\":\"<text>\"`",
                desc: "Append a log entry (system event log).",
            },
            Row {
                cmd: "task_todo_{add,list,update,remove}",
                params: "`\"task\":\"<uuid>\"` + per-action fields (update: `\"status\":\"pending\\|in_progress\\|completed\"`)",
                desc: "Flat sub-checklist per task (no nesting).",
            },
            Row {
                cmd: "task_comment_{add,list,update,remove}",
                params: "`\"task\":\"<uuid>\"` + per-action fields",
                desc: "Intentional messages to the next owner — distinct from `task_note` (the system's event log).",
            },
        ],
        Some("Board is team-wide; no allowlist. Run `task_show` when picking up a handed-off task to read prior notes, TODOs, and comments."),
    );

    if !memory_banks.is_empty() {
        push_group(
            &mut out,
            "Memory banks",
            &[
                Row {
                    cmd: "memory_search",
                    params: "`\"bank\":\"<id>\"`, [`\"query\":\"<text>\"`], [`\"title_contains\":\"<frag>\"`], [`\"include_tags\":[\"<tag>\"…]`], [`\"exclude_tags\":[\"<tag>\"…]`], [`\"limit\":N`]",
                    desc: "With `\"query\"`: semantic search. Without: newest-first tag/title filter. Default `\"limit\"` is 5.",
                },
                Row {
                    cmd: "memory_store",
                    params: "`\"bank\":\"<id>\"`, `\"body\":\"<text>\"`, [`\"title\":\"<label>\"`], [`\"tags\":[\"<tag>\"…]`]",
                    desc: "Persist an entry (permanent). Reply carries the new uuid. Tags lower-cased and de-duplicated.",
                },
                Row {
                    cmd: "memory_update",
                    params: "`\"bank\":\"<id>\"`, `\"id\":\"<uuid>\"`, `\"body\":\"<text>\"`, [`\"title\":\"<label>\"`], [`\"tags\":[\"<tag>\"…]`]",
                    desc: "Rewrite an existing entry. Re-embeds the body; replaces the tag set entirely.",
                },
                Row {
                    cmd: "memory_remove",
                    params: "`\"bank\":\"<id>\"`, `\"id\":\"<uuid>\"`",
                    desc: "Delete an entry. Cascades to its tags and every link that references it.",
                },
                Row {
                    cmd: "memory_show",
                    params: "`\"bank\":\"<id>\"`, `\"id\":\"<uuid>\"`",
                    desc: "Fetch a single entry's title, tags, created_at, and full body.",
                },
                Row {
                    cmd: "memory_link",
                    params: "`\"bank\":\"<id>\"`, `\"from\":\"<uuid>\"`, `\"to\":\"<uuid>\"`, [`\"kind\":\"<label>\"`]",
                    desc: "Directed link between two stored entries. `\"kind\"` classifies (e.g. `supersedes`, `see_also`, `contradicts`).",
                },
                Row {
                    cmd: "memory_links",
                    params: "`\"bank\":\"<id>\"`, `\"id\":\"<uuid>\"`, [`\"direction\":\"out\\|in\\|both\"`], [`\"include_tags\":[…]`], [`\"exclude_tags\":[…]`]",
                    desc: "List every link touching an entry (default `\"both\"`).",
                },
                Row {
                    cmd: "memory_unlink",
                    params: "`\"bank\":\"<id>\"`, `\"from\":\"<uuid>\"`, `\"to\":\"<uuid>\"`",
                    desc: "Remove a directed link. Idempotent.",
                },
            ],
            Some(&format!(
                "You may access these banks: {}. Each is topic-scoped — see the `<memory>` block for descriptions.",
                memory_banks.join(", ")
            )),
        );
    }

    if !skills.is_empty() {
        push_group(
            &mut out,
            "Skills",
            &[
                Row {
                    cmd: "skill_list",
                    params: "—",
                    desc: "Reply lists every skill assigned to you as \
                           `- <id> — <description>` lines. Call this \
                           first to discover what is available; skill \
                           ids are not pre-announced in the prompt.",
                },
                Row {
                    cmd: "skill_search",
                    params: "`\"query\":\"<text>\"`",
                    desc: "Case-insensitive substring match over \
                           assigned skill ids and descriptions. Output \
                           matches the shape of `skill_list`.",
                },
                Row {
                    cmd: "skill_run",
                    params: "`\"name\":\"<id>\"`",
                    desc: "Reply carries the instructions you must \
                           follow to run the named skill — its \
                           `SKILL.md` prefixed with \
                           `{baseDir} = <abs-path>` on its own line \
                           followed by a blank line.",
                },
            ],
            Some(
                "Call `skill_list` or `skill_search` to discover what \
                 is assigned before running. Treat `{baseDir}` as a \
                 literal placeholder — wherever the loaded `SKILL.md` \
                 references `{baseDir}`, substitute the absolute path \
                 printed in the header.",
            ),
        );
    }

    push_group(
        &mut out,
        "Cron",
        &[
            Row {
                cmd: "cron_add",
                params: "`\"target\":\"<agent-id>\"`, `\"kind\":\"message\\|command\"`, `\"payload\":\"<text-or-/cmd>\"`, `\"schedule\":\"<expr>\"`, [`\"tz\":\"utc\\|local\"`], [`\"oneoff\":true`], [`\"catchup_on_startup\":true`], [`\"disabled\":true`]",
                desc: "Recurring: 5-field POSIX cron `min hour dom mon dow`. One-off: set `\"oneoff\":true` and pass `\"YYYY-MM-DDTHH:MM[:SS]\"`.",
            },
            Row {
                cmd: "cron_list",
                params: "[`\"target\":\"<id>\"`], [`\"include_disabled\":true`], [`\"mine\":true`]",
                desc: "List jobs; filters optional.",
            },
            Row {
                cmd: "cron_show",
                params: "`\"id\":\"<uuid>\"`",
                desc: "Full body: target, payload, schedule, tz, catchup, last/next run, creator.",
            },
            Row {
                cmd: "cron_update",
                params: "`\"id\":\"<uuid>\"` + any `cron_add` field (+ `\"enabled\":false` to disable)",
                desc: "Change fields. Updating schedule / tz / oneoff recomputes `next_run` from now.",
            },
            Row {
                cmd: "cron_remove",
                params: "`\"id\":\"<uuid>\"`",
                desc: "Delete a job.",
            },
        ],
        Some(
            "`\"tz\"` defaults to `\"local\"` (the box running aciddog — call \
             `info` to resolve). `/exit` and `/quit` are blocked as slash \
             payloads. Target must be a configured agent (workers cannot be \
             cron targets). Cross-agent targeting is gated by the same \
             allowlist as `send_message`. Workers cannot use `cron` at all. \
             `\"catchup_on_startup\":true` (one-off only) re-fires a missed \
             instant on next start — without it, missed one-offs are pruned \
             during reconciliation.",
        ),
    );

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
        &format!("- For progress updates, use `{binary} send --to {parent_id} --body \"<text>\"`."),
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

/// A single command row in a grouped table.
#[derive(Clone, Copy)]
struct Row<'a> {
    cmd: &'a str,
    params: &'a str,
    desc: &'a str,
}

/// Emit a `### Title`, a markdown pipe table of `rows`, and an optional
/// trailing prose paragraph. Followed by a blank line so the next
/// group/section has breathing room.
fn push_group(out: &mut String, title: &str, rows: &[Row], note: Option<&str>) {
    out.push_str("### ");
    out.push_str(title);
    out.push_str("\n\n");
    out.push_str("| Command | Params | Description |\n");
    out.push_str("|---|---|---|\n");
    for r in rows {
        out.push_str("| ");
        out.push_str(r.cmd);
        out.push_str(" | ");
        out.push_str(r.params);
        out.push_str(" | ");
        out.push_str(r.desc);
        out.push_str(" |\n");
    }
    out.push('\n');
    if let Some(n) = note {
        out.push_str(n);
        out.push_str("\n\n");
    }
}

/// Append a line followed by a blank line, so the final rendered prompt
/// has visible breathing room between entries.
fn push_line(out: &mut String, line: &str) {
    out.push_str(line);
    out.push_str("\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── cli_preamble ────────────────────────────────────────────────

    #[test]
    fn cli_preamble_has_header_and_binary_path() {
        let p = cli_preamble("/opt/aciddog/bin/aciddog", &["coder"], &[], &[], &[], &[], true);
        assert!(p.starts_with("## aciddog tools"), "header: {p}");
        assert!(
            p.contains("`/opt/aciddog/bin/aciddog`"),
            "binary path callout missing: {p}"
        );
        assert!(
            p.contains("/opt/aciddog/bin/aciddog test"),
            "connectivity-test callout missing: {p}"
        );
    }

    #[test]
    fn cli_preamble_preserves_weird_binary_path() {
        let p = cli_preamble("/tmp/weird path/aciddog", &["coder"], &[], &[], &[], &[], true);
        assert!(
            p.contains("/tmp/weird path/aciddog"),
            "binary path verbatim: {p}"
        );
    }

    #[test]
    fn cli_preamble_lists_send_row_and_contactable_ids() {
        let p = cli_preamble("/a", &["coder", "reviewer"], &[], &[], &[], &[], true);
        assert!(p.contains("### Messaging"), "messaging group missing: {p}");
        assert!(p.contains("| send |"), "send row missing: {p}");
        assert!(
            p.contains("coder, reviewer"),
            "contactable allowlist missing: {p}"
        );
        assert!(
            p.contains("<Message from=\"sender-id\">body</Message>"),
            "wrapper doc missing: {p}"
        );
        assert!(
            p.contains("user messages always take priority"),
            "priority note missing: {p}"
        );
    }

    #[test]
    fn cli_preamble_messaging_group_lists_user_even_with_empty_allowlist() {
        let p = cli_preamble("/a", &[], &[], &[], &[], &[], true);
        assert!(
            p.contains("### Messaging"),
            "messaging group must always render for configured agents: {p}"
        );
        assert!(p.contains("| send |"), "send row missing: {p}");
        assert!(
            p.contains("You may message: user"),
            "user recipient missing from allowlist: {p}"
        );
    }

    #[test]
    fn cli_preamble_omits_messaging_group_for_worker_when_allowlist_empty() {
        let p = cli_preamble("/a", &[], &[], &[], &[], &[], false);
        assert!(
            !p.contains("### Messaging"),
            "worker with empty allowlist must not get Messaging group: {p}"
        );
        assert!(!p.contains("| send |"), "send row should be omitted: {p}");
    }

    #[test]
    fn cli_preamble_worker_does_not_list_user() {
        let p = cli_preamble("/a", &["parent"], &[], &[], &[], &[], false);
        assert!(p.contains("### Messaging"), "{p}");
        assert!(
            !p.contains("You may message: user"),
            "worker must not list user as recipient: {p}"
        );
        assert!(
            p.contains("You may message: parent"),
            "worker should list parent only: {p}"
        );
    }

    #[test]
    fn cli_preamble_lists_worker_create_when_creatable() {
        let p = cli_preamble("/a", &["coder"], &["researcher", "tester"], &[], &[], &[], true);
        assert!(p.contains("### Workers"), "workers group missing: {p}");
        assert!(
            p.contains("| worker create |"),
            "worker create row missing: {p}"
        );
        assert!(
            p.contains("researcher, tester"),
            "creatable allowlist missing: {p}"
        );
    }

    #[test]
    fn cli_preamble_omits_workers_group_when_creatable_empty() {
        let p = cli_preamble("/a", &["coder"], &[], &[], &[], &[], true);
        assert!(
            !p.contains("### Workers"),
            "workers group should be omitted: {p}"
        );
        assert!(
            !p.contains("worker create"),
            "worker create should be omitted: {p}"
        );
    }

    #[test]
    fn cli_preamble_never_documents_worker_complete_for_plain_agents() {
        let p = cli_preamble("/a", &["coder"], &["researcher"], &[], &[], &[], true);
        assert!(
            !p.contains("worker complete"),
            "plain agents must not see worker complete: {p}"
        );
    }

    #[test]
    fn cli_preamble_has_task_board_unconditionally() {
        let p = cli_preamble("/a", &[], &[], &[], &[], &[], true);
        assert!(
            p.contains("### Task board"),
            "task board group missing: {p}"
        );
        assert!(p.contains("| task add |"), "task add row missing: {p}");
        assert!(p.contains("| task take |"), "task take row missing: {p}");
        assert!(
            p.contains("team-wide; no allowlist"),
            "unconditional note missing: {p}"
        );
    }

    #[test]
    fn cli_preamble_has_info_and_cron_groups_unconditionally() {
        let p = cli_preamble("/a", &[], &[], &[], &[], &[], true);
        assert!(p.contains("### Meta"), "meta group missing: {p}");
        assert!(p.contains("| info |"), "info row missing: {p}");
        assert!(p.contains("### Cron"), "cron group missing: {p}");
        assert!(p.contains("| cron add |"), "cron add row missing: {p}");
    }

    #[test]
    fn cli_preamble_cron_prose_mentions_tz_blocklist_and_workers() {
        let p = cli_preamble("/a", &[], &[], &[], &[], &[], true);
        let lower = p.to_lowercase();
        assert!(
            p.contains("`--tz` defaults to `local`"),
            "tz default note missing: {p}"
        );
        assert!(
            p.contains("`/exit` and `/quit` are blocked"),
            "slash blocklist note missing: {p}"
        );
        assert!(
            lower.contains("workers cannot be cron targets"),
            "worker-target restriction note missing: {p}"
        );
        assert!(
            lower.contains("workers cannot use `cron`"),
            "worker-cron restriction note missing: {p}"
        );
    }

    #[test]
    fn cli_preamble_memory_group_omitted_when_banks_empty() {
        let p = cli_preamble("/a", &["coder"], &[], &[], &[], &[], true);
        assert!(
            !p.contains("### Memory banks"),
            "memory group should be omitted: {p}"
        );
        assert!(
            !p.contains("memory search"),
            "memory commands should be omitted: {p}"
        );
    }

    #[test]
    fn cli_preamble_memory_group_lists_commands_and_banks_when_present() {
        let p = cli_preamble("/a", &["coder"], &[], &["scratch", "incidents"], &[], &[], true);
        assert!(p.contains("### Memory banks"), "memory group missing: {p}");
        for cmd in [
            "| memory search |",
            "| memory store |",
            "| memory link |",
            "| memory links |",
            "| memory unlink |",
        ] {
            assert!(p.contains(cmd), "missing row {cmd} in: {p}");
        }
        assert!(
            p.contains("scratch, incidents"),
            "bank allowlist missing: {p}"
        );
    }

    #[test]
    fn cli_preamble_skills_group_omitted_when_skills_empty() {
        let p = cli_preamble("/a", &["coder"], &[], &[], &[], &[], true);
        assert!(
            !p.contains("### Skills"),
            "skills group should be omitted: {p}"
        );
        assert!(
            !p.contains("| skill list |"),
            "skill list should be omitted: {p}"
        );
        assert!(
            !p.contains("| skill search |"),
            "skill search should be omitted: {p}"
        );
        assert!(
            !p.contains("| skill run |"),
            "skill run should be omitted: {p}"
        );
    }

    #[test]
    fn cli_preamble_skills_group_lists_list_search_run_rows_when_present() {
        let p = cli_preamble(
            "/a",
            &["coder"],
            &[],
            &[],
            &["code-review", "research"],
            &[],
            true,
        );
        assert!(p.contains("### Skills"), "skills group missing: {p}");
        for row in ["| skill list |", "| skill search |", "| skill run |"] {
            assert!(p.contains(row), "missing row {row} in: {p}");
        }
        assert!(p.contains("`--name <id>`"), "run params missing: {p}");
        assert!(
            p.contains("`--query \"<text>\"`"),
            "search params missing: {p}"
        );
        assert!(
            p.contains("{baseDir}"),
            "baseDir placeholder mention missing: {p}"
        );
        assert!(
            p.contains("instructions you must follow"),
            "skill_run desc should emphasize it prints instructions: {p}"
        );
    }

    #[test]
    fn cli_preamble_skills_group_does_not_leak_skill_ids_into_prompt() {
        let p = cli_preamble(
            "/a",
            &["coder"],
            &[],
            &[],
            &["code-review", "research"],
            &[],
            true,
        );
        // The list of concrete ids must NOT appear in the prompt; the
        // model is expected to discover them at runtime via
        // `skill list` / `skill search`.
        assert!(!p.contains("code-review"), "skill id leaked: {p}");
        assert!(!p.contains("research"), "skill id leaked: {p}");
    }

    // ── worker_preamble (CLI) ───────────────────────────────────────

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
    fn worker_preamble_mentions_hibernation_and_resume() {
        let p = worker_preamble("/a", "coder", "task");
        let lower = p.to_lowercase();
        assert!(lower.contains("hibernate"), "hibernate wording: {p}");
        assert!(lower.contains("resume"), "resume wording: {p}");
    }

    // ── tag_preamble ────────────────────────────────────────────────

    #[test]
    fn tag_preamble_has_header_and_no_shell_tool_framing() {
        let p = tag_preamble(&["coder"], &[], &[], &[], &[], true);
        assert!(p.starts_with("## aciddog tools"), "header: {p}");
        assert!(
            !p.contains("shell tool"),
            "must not frame as absence-of-shell-tool: {p}"
        );
        assert!(p.contains("<aciddog-call>"), "call tag missing: {p}");
        assert!(p.contains("<aciddog-response"), "response tag missing: {p}");
    }

    #[test]
    fn tag_preamble_mentions_chain_cap() {
        let p = tag_preamble(&["coder"], &[], &[], &[], &[], true);
        assert!(
            p.contains("8 consecutive"),
            "must tell the model about the 8-round cap: {p}"
        );
    }

    #[test]
    fn tag_preamble_lists_send_message_row_and_contactable_ids() {
        let p = tag_preamble(&["coder", "reviewer"], &[], &[], &[], &[], true);
        assert!(
            p.contains("| send_message |"),
            "send_message row missing: {p}"
        );
        assert!(
            p.contains("coder, reviewer"),
            "contactable allowlist missing: {p}"
        );
    }

    #[test]
    fn tag_preamble_messaging_group_lists_user_even_with_empty_allowlist() {
        let p = tag_preamble(&[], &[], &[], &[], &[], true);
        assert!(
            p.contains("### Messaging"),
            "messaging group must always render for configured agents: {p}"
        );
        assert!(
            p.contains("| send_message |"),
            "send_message row missing: {p}"
        );
        assert!(
            p.contains("You may message: user"),
            "user recipient missing from allowlist: {p}"
        );
    }

    #[test]
    fn tag_preamble_omits_messaging_for_worker_when_allowlist_empty() {
        let p = tag_preamble(&[], &[], &[], &[], &[], false);
        assert!(
            !p.contains("### Messaging"),
            "worker with empty allowlist must not get Messaging group: {p}"
        );
        assert!(
            !p.contains("| send_message |"),
            "send_message row should be omitted: {p}"
        );
    }

    #[test]
    fn tag_preamble_worker_does_not_list_user() {
        let p = tag_preamble(&["parent"], &[], &[], &[], &[], false);
        assert!(p.contains("### Messaging"), "{p}");
        assert!(
            !p.contains("You may message: user"),
            "worker must not list user as recipient: {p}"
        );
        assert!(
            p.contains("You may message: parent"),
            "worker should list parent only: {p}"
        );
    }

    #[test]
    fn tag_preamble_lists_worker_create_when_creatable() {
        let p = tag_preamble(&["coder"], &["researcher", "tester"], &[], &[], &[], true);
        assert!(
            p.contains("| worker_create |"),
            "worker_create row missing: {p}"
        );
        assert!(
            p.contains("researcher, tester"),
            "creatable allowlist missing: {p}"
        );
    }

    #[test]
    fn tag_preamble_omits_worker_create_when_empty() {
        let p = tag_preamble(&["coder"], &[], &[], &[], &[], true);
        assert!(
            !p.contains("worker_create"),
            "worker_create should be omitted: {p}"
        );
    }

    #[test]
    fn tag_preamble_never_mentions_worker_complete_for_plain_agents() {
        let p = tag_preamble(&["coder"], &["researcher"], &[], &[], &[], true);
        assert!(
            !p.contains("worker_complete"),
            "plain agents must not see worker_complete: {p}"
        );
    }

    #[test]
    fn tag_preamble_has_task_board_unconditionally() {
        let p = tag_preamble(&[], &[], &[], &[], &[], true);
        assert!(p.contains("| task_add |"), "task_add row missing: {p}");
        assert!(p.contains("| task_take |"), "task_take row missing: {p}");
    }

    #[test]
    fn tag_preamble_has_info_and_cron_groups_unconditionally() {
        let p = tag_preamble(&[], &[], &[], &[], &[], true);
        assert!(p.contains("| info |"), "info row missing: {p}");
        assert!(p.contains("| cron_add |"), "cron_add row missing: {p}");
        assert!(p.contains("| cron_list |"), "cron_list row missing: {p}");
        assert!(p.contains("| cron_show |"), "cron_show row missing: {p}");
        assert!(
            p.contains("| cron_update |"),
            "cron_update row missing: {p}"
        );
        assert!(
            p.contains("| cron_remove |"),
            "cron_remove row missing: {p}"
        );
    }

    #[test]
    fn tag_preamble_cron_prose_mentions_tz_and_blocklist() {
        let p = tag_preamble(&[], &[], &[], &[], &[], true);
        assert!(
            p.contains("`/exit` and `/quit` are blocked"),
            "slash blocklist note missing: {p}"
        );
        assert!(
            p.contains("`\"tz\"` defaults to `\"local\"`"),
            "tz default note missing: {p}"
        );
    }

    #[test]
    fn tag_preamble_memory_group_omitted_when_banks_empty() {
        let p = tag_preamble(&["coder"], &[], &[], &[], &[], true);
        assert!(
            !p.contains("memory_search"),
            "memory commands should be omitted: {p}"
        );
    }

    #[test]
    fn tag_preamble_memory_group_lists_commands_when_banks_present() {
        let p = tag_preamble(&["coder"], &[], &["scratch"], &[], &[], true);
        for cmd in [
            "| memory_search |",
            "| memory_store |",
            "| memory_update |",
            "| memory_remove |",
            "| memory_show |",
            "| memory_link |",
            "| memory_links |",
            "| memory_unlink |",
        ] {
            assert!(p.contains(cmd), "missing row {cmd} in: {p}");
        }
        assert!(p.contains("scratch"), "bank allowlist missing: {p}");
    }

    #[test]
    fn tag_preamble_skills_group_omitted_when_skills_empty() {
        let p = tag_preamble(&["coder"], &[], &[], &[], &[], true);
        assert!(
            !p.contains("### Skills"),
            "skills group should be omitted: {p}"
        );
        for row in ["| skill_list |", "| skill_search |", "| skill_run |"] {
            assert!(!p.contains(row), "{row} should be omitted: {p}");
        }
    }

    #[test]
    fn tag_preamble_skills_group_lists_list_search_run_rows_when_present() {
        let p = tag_preamble(&["coder"], &[], &[], &["code-review", "research"], &[], true);
        assert!(p.contains("### Skills"), "skills group missing: {p}");
        for row in ["| skill_list |", "| skill_search |", "| skill_run |"] {
            assert!(p.contains(row), "missing row {row} in: {p}");
        }
        assert!(p.contains("\"name\":\"<id>\""), "run params missing: {p}");
        assert!(
            p.contains("\"query\":\"<text>\""),
            "search params missing: {p}"
        );
        assert!(
            p.contains("{baseDir}"),
            "baseDir placeholder mention missing: {p}"
        );
        assert!(
            p.contains("instructions you must follow"),
            "skill_run desc should emphasize it prints instructions: {p}"
        );
    }

    #[test]
    fn tag_preamble_skills_group_does_not_leak_skill_ids_into_prompt() {
        let p = tag_preamble(&["coder"], &[], &[], &["code-review", "research"], &[], true);
        assert!(!p.contains("code-review"), "skill id leaked: {p}");
        assert!(!p.contains("research"), "skill id leaked: {p}");
    }

    // ── tag_worker_preamble ─────────────────────────────────────────

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
