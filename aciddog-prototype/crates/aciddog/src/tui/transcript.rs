use super::blocks::{ChatBlock, MailDirection, SystemKind};

const HEAD_KEEP_CHARS: usize = 12 * 1024;
const TAIL_KEEP_CHARS: usize = 16 * 1024;

// Restart transcripts are Markdown intended for the replacement AI core.
// Headers are stable role cues:
// - `## User said:` and `## You replied:` for conversational turns.
// - `## Tool: <name> ...` for tool invocations and results.
// - `## Mail to/from <agent>:` and `## Channel message from <sender>:` for comms.
// - `## System (<kind>): ...` for orchestration events.
// `AgentInfo` is skipped because the replacement worker gets its own.

/// Build a Markdown transcript suitable for seeding a restart-core worker.
///
/// The session hash is optional because this is a pure function over chat
/// blocks. When supplied, the elision marker names `<hash>.jsonl`; otherwise it
/// says the prior session JSONL was retained on disk for forensics.
pub fn build_restart_transcript(
    blocks: &[ChatBlock],
    cap_chars: usize,
    session_hash: Option<&str>,
) -> String {
    let mut rendered = String::new();
    let mut spans = Vec::new();
    let mut pos = 0usize;
    for block in blocks {
        let Some(chunk) = render_block(block) else {
            continue;
        };
        let len = chunk.chars().count();
        rendered.push_str(&chunk);
        spans.push(pos..pos + len);
        pos += len;
    }

    if rendered.chars().count() <= cap_chars {
        return rendered;
    }

    elide_middle(&rendered, &spans, session_hash)
}

fn render_block(block: &ChatBlock) -> Option<String> {
    match block {
        ChatBlock::AgentInfo { .. } => None,
        ChatBlock::UserText { text } => Some(section("## User said:", text)),
        ChatBlock::AiMarkdown { raw, .. } => Some(section("## You replied:", raw)),
        ChatBlock::BashTool {
            command,
            stdout,
            is_error,
            pending,
            ..
        } => Some(tool_section(
            &format!(
                "## Tool: Bash $ {command}{}",
                status_suffix(*is_error, *pending)
            ),
            stdout,
        )),
        ChatBlock::EditTool {
            file_path,
            diff,
            is_error,
            pending,
            ..
        } => Some(tool_section(
            &format!(
                "## Tool: Edit {file_path}{}",
                status_suffix(*is_error, *pending)
            ),
            diff,
        )),
        ChatBlock::WriteTool {
            file_path,
            diff,
            is_error,
            pending,
            ..
        } => Some(tool_section(
            &format!(
                "## Tool: Write {file_path}{}",
                status_suffix(*is_error, *pending)
            ),
            diff,
        )),
        ChatBlock::ReadTool {
            file_path,
            range,
            is_error,
            pending,
        } => {
            let range = range
                .as_deref()
                .map(|r| format!(" {r}"))
                .unwrap_or_default();
            Some(section(
                &format!(
                    "## Tool: Read {file_path}{range}{}",
                    status_suffix(*is_error, *pending)
                ),
                "",
            ))
        }
        ChatBlock::GrepTool {
            pattern,
            path,
            matches,
            is_error,
            pending,
            ..
        } => {
            let path = path
                .as_deref()
                .map(|p| format!(" in {p}"))
                .unwrap_or_default();
            Some(tool_section(
                &format!(
                    "## Tool: Grep /{pattern}/{path}{}",
                    status_suffix(*is_error, *pending)
                ),
                matches,
            ))
        }
        ChatBlock::GlobTool {
            pattern,
            path,
            matches,
            is_error,
            pending,
            ..
        } => {
            let path = path
                .as_deref()
                .map(|p| format!(" in {p}"))
                .unwrap_or_default();
            Some(tool_section(
                &format!(
                    "## Tool: Glob {pattern}{path}{}",
                    status_suffix(*is_error, *pending)
                ),
                matches,
            ))
        }
        ChatBlock::GenericTool {
            name,
            input_pretty,
            result_pretty,
            is_error,
            pending,
            ..
        } => {
            let body = match (input_pretty.is_empty(), result_pretty.is_empty()) {
                (true, true) => String::new(),
                (false, true) => format!("Input:\n{input_pretty}"),
                (true, false) => result_pretty.clone(),
                (false, false) => format!("Input:\n{input_pretty}\n\nResult:\n{result_pretty}"),
            };
            Some(tool_section(
                &format!("## Tool: {name}{}", status_suffix(*is_error, *pending)),
                &body,
            ))
        }
        ChatBlock::System { text, kind } => Some(section(
            &format!("## System ({}):", system_kind(*kind)),
            text,
        )),
        ChatBlock::Mail {
            direction,
            counterpart,
            body,
            is_error,
            pending,
            ..
        } => {
            let header = match direction {
                MailDirection::Sent => format!(
                    "## Mail to {counterpart}:{}",
                    status_suffix(*is_error, *pending)
                ),
                MailDirection::Received => format!(
                    "## Mail from {counterpart}:{}",
                    status_suffix(*is_error, *pending)
                ),
            };
            Some(section(&header, body))
        }
        ChatBlock::WorkerSpawn {
            template,
            worker_id,
            task,
            is_error,
            pending,
            ..
        } => {
            let worker = worker_id
                .as_deref()
                .map(|id| format!(" as {id}"))
                .unwrap_or_default();
            Some(section(
                &format!(
                    "## Worker spawned: {template}{worker}{}",
                    status_suffix(*is_error, *pending)
                ),
                task,
            ))
        }
        ChatBlock::WorkerComplete {
            body,
            is_error,
            pending,
            ..
        } => Some(section(
            &format!("## Worker completed:{}", status_suffix(*is_error, *pending)),
            body,
        )),
        ChatBlock::ChannelMessage {
            sender,
            body,
            is_welcome,
        } => {
            if *is_welcome {
                Some(section("## Channel welcome:", body))
            } else {
                Some(section(&format!("## Channel message from {sender}:"), body))
            }
        }
        ChatBlock::RemotePatch {
            summary,
            diff,
            is_error,
            pending,
            ..
        } => Some(tool_section(
            &format!(
                "## Tool: Remote patch {summary}{}",
                status_suffix(*is_error, *pending)
            ),
            diff,
        )),
        ChatBlock::TaskOp {
            kind,
            subject,
            body,
            reply,
            is_error,
            pending,
            ..
        } => {
            let body = match (body.is_empty(), reply.is_empty()) {
                (true, true) => String::new(),
                (false, true) => body.clone(),
                (true, false) => reply.clone(),
                (false, false) => format!("{body}\n\n---\n{reply}"),
            };
            Some(tool_section(
                &format!(
                    "## Task op: {:?} {subject}{}",
                    kind,
                    status_suffix(*is_error, *pending)
                ),
                &body,
            ))
        }
        ChatBlock::AutomatonStdout { line } => Some(section("## Automaton stdout:", line)),
        ChatBlock::AutomatonStderr { line } => Some(section("## Automaton stderr:", line)),
    }
}

fn section(header: &str, body: &str) -> String {
    if body.is_empty() {
        format!("{header}\n\n")
    } else {
        format!("{header}\n{body}\n\n")
    }
}

fn tool_section(header: &str, body: &str) -> String {
    section(header, body)
}

fn status_suffix(is_error: bool, pending: bool) -> &'static str {
    match (is_error, pending) {
        (true, true) => " [error, pending]",
        (true, false) => " [error]",
        (false, true) => " [pending]",
        (false, false) => "",
    }
}

fn system_kind(kind: SystemKind) -> &'static str {
    match kind {
        SystemKind::Info => "info",
        SystemKind::Warn => "warn",
        SystemKind::Error => "error",
    }
}

fn elide_middle(
    rendered: &str,
    spans: &[std::ops::Range<usize>],
    session_hash: Option<&str>,
) -> String {
    let total = rendered.chars().count();
    let head_keep = HEAD_KEEP_CHARS.min(total);
    let tail_keep = TAIL_KEEP_CHARS.min(total.saturating_sub(head_keep));
    let tail_start = total - tail_keep;

    let elided_blocks = spans
        .iter()
        .filter(|span| span.start >= head_keep && span.end <= tail_start)
        .count();
    let marker = elision_marker(elided_blocks, session_hash);

    format!(
        "{}{}{}",
        take_chars(rendered, head_keep),
        marker,
        skip_chars(rendered, tail_start)
    )
}

fn elision_marker(elided_blocks: usize, session_hash: Option<&str>) -> String {
    let location = match session_hash {
        Some(hash) if !hash.is_empty() => {
            format!("see prior session JSONL {hash}.jsonl for full record")
        }
        _ => "prior session JSONL retained for forensics".to_string(),
    };
    format!("\n... [{elided_blocks} blocks elided in the middle - {location}] ...\n")
}

fn take_chars(s: &str, count: usize) -> &str {
    if count == 0 {
        return "";
    }
    match s.char_indices().nth(count) {
        Some((idx, _)) => &s[..idx],
        None => s,
    }
}

fn skip_chars(s: &str, count: usize) -> &str {
    if count == 0 {
        return s;
    }
    match s.char_indices().nth(count) {
        Some((idx, _)) => &s[idx..],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::blocks::TaskOpKind;

    #[test]
    fn build_restart_transcript_tiny_renders_headers_and_bodies_in_order() {
        let blocks = vec![
            ChatBlock::agent_info("core", vec!["Bash".into()], "prompt", None, "hash"),
            ChatBlock::user("brief"),
            ChatBlock::ai_markdown("answer"),
            ChatBlock::BashTool {
                command: "cargo test".into(),
                stdout: "ok".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::EditTool {
                file_path: "src/lib.rs".into(),
                diff: "--- old\n+++ new".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::WriteTool {
                file_path: "src/new.rs".into(),
                diff: "new file".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::ReadTool {
                file_path: "README.md".into(),
                range: Some("1-3".into()),
                is_error: false,
                pending: false,
            },
            ChatBlock::GrepTool {
                pattern: "fn main".into(),
                path: Some("src".into()),
                matches: "src/main.rs:1".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::GlobTool {
                pattern: "*.rs".into(),
                path: Some("src".into()),
                matches: "src/lib.rs".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::GenericTool {
                name: "custom".into(),
                input_pretty: "{\"x\":1}".into(),
                result_pretty: "done".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::RemotePatch {
                summary: "change set".into(),
                diff: "diff --git".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::mail_sent("dev_lead", "progress"),
            ChatBlock::mail_received("dev_lead", "ack"),
            ChatBlock::channel_message("alice", "team note"),
            ChatBlock::WorkerSpawn {
                template: "general".into(),
                worker_id: Some("general#12345678".into()),
                task: "subtask".into(),
                is_error: false,
                collapsed: false,
                pending: false,
            },
            ChatBlock::worker_complete("finished"),
            ChatBlock::task_op(TaskOpKind::Add, "Add task", "body"),
            ChatBlock::system_warn("careful"),
            ChatBlock::system_error("failed"),
            ChatBlock::AutomatonStdout {
                line: "stdout".into(),
            },
            ChatBlock::AutomatonStderr {
                line: "stderr".into(),
            },
        ];

        let transcript = build_restart_transcript(&blocks, 64 * 1024, None);
        assert!(!transcript.contains("## AgentInfo"));
        assert_in_order(
            &transcript,
            &[
                "## User said:\nbrief",
                "## You replied:\nanswer",
                "## Tool: Bash $ cargo test\nok",
                "## Tool: Edit src/lib.rs\n--- old",
                "## Tool: Write src/new.rs\nnew file",
                "## Tool: Read README.md 1-3",
                "## Tool: Grep /fn main/ in src\nsrc/main.rs:1",
                "## Tool: Glob *.rs in src\nsrc/lib.rs",
                "## Tool: custom\nInput:\n{\"x\":1}\n\nResult:\ndone",
                "## Tool: Remote patch change set\ndiff --git",
                "## Mail to dev_lead: [pending]\nprogress",
                "## Mail from dev_lead:\nack",
                "## Channel message from alice:\nteam note",
                "## Worker spawned: general as general#12345678\nsubtask",
                "## Worker completed: [pending]\nfinished",
                "## Task op: Add Add task [pending]\nbody",
                "## System (warn):\ncareful",
                "## System (error):\nfailed",
                "## Automaton stdout:\nstdout",
                "## Automaton stderr:\nstderr",
            ],
        );
    }

    #[test]
    fn build_restart_transcript_over_cap_elides_middle_with_count() {
        let blocks: Vec<ChatBlock> = (0..220)
            .map(|i| ChatBlock::ai_markdown(format!("block-{i:03} {}", "x".repeat(300))))
            .collect();
        let full = build_restart_transcript(&blocks, usize::MAX, Some("abc123"));
        let transcript = build_restart_transcript(&blocks, 32 * 1024, Some("abc123"));

        let head = take_chars(&full, HEAD_KEEP_CHARS);
        let tail = skip_chars(&full, full.chars().count() - TAIL_KEEP_CHARS);
        assert!(transcript.starts_with(head));
        assert!(transcript.ends_with(tail));

        let expected = expected_elided_blocks(&blocks);
        let marker = format!(
            "... [{expected} blocks elided in the middle - see prior session JSONL abc123.jsonl for full record] ..."
        );
        assert!(transcript.contains(&marker), "{transcript}");
    }

    #[test]
    fn build_restart_transcript_under_cap_has_no_elision_marker() {
        let blocks: Vec<ChatBlock> = (0..10)
            .map(|i| ChatBlock::ai_markdown(format!("block-{i}")))
            .collect();
        let transcript = build_restart_transcript(&blocks, 32 * 1024, None);
        assert!(!transcript.contains("blocks elided in the middle"));
    }

    fn assert_in_order(haystack: &str, needles: &[&str]) {
        let mut cursor = 0usize;
        for needle in needles {
            let Some(relative) = haystack[cursor..].find(needle) else {
                panic!("missing `{needle}` after byte {cursor} in:\n{haystack}");
            };
            cursor += relative + needle.len();
        }
    }

    fn expected_elided_blocks(blocks: &[ChatBlock]) -> usize {
        let mut pos = 0usize;
        let mut spans = Vec::new();
        for block in blocks {
            let chunk = render_block(block).unwrap();
            let len = chunk.chars().count();
            spans.push(pos..pos + len);
            pos += len;
        }
        let tail_start = pos - TAIL_KEEP_CHARS;
        spans
            .iter()
            .filter(|span| span.start >= HEAD_KEEP_CHARS && span.end <= tail_start)
            .count()
    }
}
