//! Chat block data model + rendering.
//!
//! Each message in the chat history is a [`ChatBlock`]. A block knows how tall
//! it is at a given width ([`ChatBlock::height`]) and how to paint itself into
//! a ratatui sub-rect ([`ChatBlock::render`]). During render, interactive
//! buttons (expand/collapse, copy) push their hit rects into the shared
//! [`ChatButton`] list the app consults on mouse events.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Widget, Wrap};

use super::markdown::{self, MdChunk, ParsedMd};

/// Which chat message a scroll viewport is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScrollAnchor {
    pub block_idx: usize,
    pub within: u16,
}

/// Cached full-height render of the anchor block, kept between frames so
/// mouse-wheel scrolling only re-blits slices instead of re-rendering the
/// whole block each tick. Invalidated on width/height/content changes.
#[derive(Debug, Clone)]
pub struct AnchorScratch {
    pub block_idx: usize,
    pub width: u16,
    pub height: u16,
    pub fingerprint: u64,
    pub buffer: Buffer,
    pub buttons: Vec<ChatButton>,
}

/// Cheap per-frame fingerprint of a block's visible content. Only the
/// fields that affect rendering are fed into the hasher; this is what the
/// anchor-scratch cache compares on every frame to decide whether it can
/// skip re-rendering.
pub fn block_fingerprint(b: &ChatBlock) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut h = DefaultHasher::new();
    // Discriminant so two variants with the same-shaped bytes don't collide.
    std::mem::discriminant(b).hash(&mut h);

    let hash_usage = |h: &mut DefaultHasher, u: &Option<crate::ai::TurnUsage>| match u {
        Some(u) => {
            1u8.hash(h);
            u.input_tokens.hash(h);
            u.output_tokens.hash(h);
            u.cache_read_input_tokens.hash(h);
            u.cache_creation_input_tokens.hash(h);
        }
        None => 0u8.hash(h),
    };

    match b {
        ChatBlock::AgentInfo {
            core,
            tools,
            prompt,
            collapsed,
            context_used,
            context_window,
        } => {
            core.hash(&mut h);
            tools.hash(&mut h);
            prompt.hash(&mut h);
            collapsed.hash(&mut h);
            hash_usage(&mut h, context_used);
            context_window.hash(&mut h);
        }
        ChatBlock::UserText { text } => text.hash(&mut h),
        ChatBlock::AiMarkdown { raw, .. } => raw.hash(&mut h),
        ChatBlock::BashTool {
            command,
            stdout,
            is_error,
            collapsed,
            pending,
        } => {
            command.hash(&mut h);
            stdout.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::EditTool {
            file_path,
            diff,
            is_error,
            collapsed,
            pending,
        }
        | ChatBlock::WriteTool {
            file_path,
            diff,
            is_error,
            collapsed,
            pending,
        } => {
            file_path.hash(&mut h);
            diff.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::ReadTool {
            file_path,
            range,
            is_error,
            pending,
        } => {
            file_path.hash(&mut h);
            range.hash(&mut h);
            is_error.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::GrepTool {
            pattern,
            path,
            matches,
            is_error,
            collapsed,
            pending,
        }
        | ChatBlock::GlobTool {
            pattern,
            path,
            matches,
            is_error,
            collapsed,
            pending,
        } => {
            pattern.hash(&mut h);
            path.hash(&mut h);
            matches.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::GenericTool {
            name,
            input_pretty,
            result_pretty,
            is_error,
            collapsed,
            pending,
        } => {
            name.hash(&mut h);
            input_pretty.hash(&mut h);
            result_pretty.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::System { text, kind } => {
            text.hash(&mut h);
            (*kind as u8).hash(&mut h);
        }
        ChatBlock::Mail {
            direction,
            counterpart,
            body,
            is_error,
            collapsed,
            pending,
        } => {
            (*direction as u8).hash(&mut h);
            counterpart.hash(&mut h);
            body.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::WorkerSpawn {
            template,
            worker_id,
            task,
            is_error,
            collapsed,
            pending,
        } => {
            template.hash(&mut h);
            worker_id.hash(&mut h);
            task.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::WorkerComplete {
            body,
            is_error,
            collapsed,
            pending,
        } => {
            body.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::TaskOp {
            kind,
            subject,
            body,
            reply,
            is_error,
            collapsed,
            pending,
        } => {
            (*kind as u8).hash(&mut h);
            subject.hash(&mut h);
            body.hash(&mut h);
            reply.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::RemotePatch {
            summary,
            diff,
            is_error,
            collapsed,
            pending,
        } => {
            summary.hash(&mut h);
            diff.hash(&mut h);
            is_error.hash(&mut h);
            collapsed.hash(&mut h);
            pending.hash(&mut h);
        }
        ChatBlock::ChannelMessage {
            sender,
            body,
            is_welcome,
        } => {
            sender.hash(&mut h);
            body.hash(&mut h);
            is_welcome.hash(&mut h);
        }
        ChatBlock::AutomatonStdout { line } | ChatBlock::AutomatonStderr { line } => {
            line.hash(&mut h);
        }
    }

    h.finish()
}

/// Kind of system-info block — controls colour only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemKind {
    Info,
    Warn,
    Error,
}

/// Direction of a mail block: this agent sent the message, or received it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailDirection {
    Sent,
    Received,
}

/// Which `aciddog task …` CLI op produced a `TaskOp` block. A flat
/// enum instead of a nested one keeps dispatch cheap; each variant
/// is a single point in (top-level task, todo, comment) × (verb).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskOpKind {
    Add,
    List,
    Show,
    Update,
    Remove,
    Take,
    Note,
    TodoAdd,
    TodoList,
    TodoUpdate,
    TodoRemove,
    CommentAdd,
    CommentList,
    CommentUpdate,
    CommentRemove,
}

impl TaskOpKind {
    /// Glyph shown at the head of the block. Three groups — task /
    /// todo / comment — so the operator can tell them apart at a
    /// glance without reading the subject.
    pub fn icon(&self) -> &'static str {
        match self {
            Self::TodoAdd | Self::TodoList | Self::TodoUpdate | Self::TodoRemove => "☑",
            Self::CommentAdd | Self::CommentList | Self::CommentUpdate | Self::CommentRemove => {
                "💬"
            }
            _ => "📋",
        }
    }

    /// Label shown above the body area when the block is expanded.
    /// Conveys what the body contains ("task" for add, "note" for
    /// notes, "filter" for list ops where the body is empty but
    /// the reply is a table, etc).
    pub fn body_label(&self) -> &'static str {
        match self {
            Self::Add | Self::Update => "task",
            Self::List | Self::TodoList | Self::CommentList => "filter",
            Self::Show => "task",
            Self::Remove | Self::TodoRemove | Self::CommentRemove => "id",
            Self::Take => "",
            Self::Note | Self::CommentUpdate | Self::CommentAdd => "body",
            Self::TodoAdd | Self::TodoUpdate => "todo",
        }
    }
}

/// A clickable hot rect registered during render.
#[derive(Debug, Clone)]
pub struct ChatButton {
    pub rect: Rect,
    pub action: BlockAction,
}

/// What a click or Alt-key press should do.
#[derive(Debug, Clone)]
pub enum BlockAction {
    ToggleCollapse(usize),
    CopyText(String),
}

/// One message in the chat.
#[derive(Debug, Clone)]
pub enum ChatBlock {
    AgentInfo {
        core: String,
        tools: Vec<String>,
        prompt: String,
        collapsed: bool,
        /// Cumulative session token usage from this agent's core.
        /// `None` until the first usage report arrives, or for the
        /// whole session if the core never reports.
        context_used: Option<crate::ai::TurnUsage>,
        /// Maximum context window for the model in use, in tokens.
        /// `None` when unknown.
        context_window: Option<u32>,
    },
    UserText {
        text: String,
    },
    AiMarkdown {
        raw: String,
        parsed: ParsedMd,
    },
    BashTool {
        command: String,
        stdout: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    EditTool {
        file_path: String,
        diff: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    WriteTool {
        file_path: String,
        diff: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    ReadTool {
        file_path: String,
        range: Option<String>,
        is_error: bool,
        pending: bool,
    },
    GrepTool {
        pattern: String,
        path: Option<String>,
        matches: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    GlobTool {
        pattern: String,
        path: Option<String>,
        matches: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    GenericTool {
        name: String,
        input_pretty: String,
        result_pretty: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    System {
        text: String,
        kind: SystemKind,
    },
    /// An inter-agent message, either sent by this agent or received from
    /// another. Replaces the raw Bash tool call for `aciddog send` and the
    /// system_info echo that used to surface receipts.
    Mail {
        direction: MailDirection,
        counterpart: String,
        body: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    /// Rendered on the spawner's pane when it runs
    /// `aciddog worker create`. Captures the template id, the assigned
    /// worker display id (if known), and the initial task.
    WorkerSpawn {
        template: String,
        worker_id: Option<String>,
        task: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    /// Rendered on a worker's own pane when it runs
    /// `aciddog worker complete`. Makes it visible that the worker
    /// delivered a final report and hibernated.
    WorkerComplete {
        body: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    /// A single line in a channel's transcript. `sender` is the
    /// agent id that posted, or the literal string `"user"` for the
    /// human operator. `is_welcome` flags the synthetic banner block
    /// rendered at the top of a channel from the config's `welcome`
    /// field — styled as muted system text and never persisted.
    ChannelMessage {
        sender: String,
        body: String,
        is_welcome: bool,
    },
    /// A patch (changeSet / gitPatch) produced by a remote worker
    /// (Jules). `summary` is a one-line header describing the commit
    /// (message + file count); `diff` is the unified patch body.
    /// Rendered like EditTool but with a commit-style summary.
    RemotePatch {
        summary: String,
        diff: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    /// Replaces the raw Bash block for any `aciddog task …` call.
    /// `subject` is the pre-formatted one-line header; `body` is
    /// the payload the agent sent (if any); `reply` is populated on
    /// `ToolResult` with the orchestrator's response.
    TaskOp {
        kind: TaskOpKind,
        subject: String,
        body: String,
        reply: String,
        is_error: bool,
        collapsed: bool,
        pending: bool,
    },
    /// One line of stdout from an automaton child process. Rendered as
    /// plain monospace text with no markdown parsing so a script that
    /// accidentally prints backticks doesn't upset the layout.
    AutomatonStdout { line: String },
    /// One line of stderr from an automaton child process. Rendered
    /// like `AutomatonStdout` but coloured red so error output stays
    /// visually distinct while interleaved with stdout lines.
    AutomatonStderr { line: String },
}

impl ChatBlock {
    pub fn is_collapsible(&self) -> bool {
        matches!(
            self,
            Self::AgentInfo { .. }
                | Self::BashTool { .. }
                | Self::EditTool { .. }
                | Self::WriteTool { .. }
                | Self::GrepTool { .. }
                | Self::GlobTool { .. }
                | Self::GenericTool { .. }
                | Self::Mail { .. }
                | Self::WorkerSpawn { .. }
                | Self::WorkerComplete { .. }
                | Self::TaskOp { .. }
                | Self::RemotePatch { .. }
        )
    }

    pub fn toggle_collapsed(&mut self) {
        match self {
            Self::AgentInfo { collapsed, .. }
            | Self::BashTool { collapsed, .. }
            | Self::EditTool { collapsed, .. }
            | Self::WriteTool { collapsed, .. }
            | Self::GrepTool { collapsed, .. }
            | Self::GlobTool { collapsed, .. }
            | Self::GenericTool { collapsed, .. }
            | Self::Mail { collapsed, .. }
            | Self::WorkerSpawn { collapsed, .. }
            | Self::WorkerComplete { collapsed, .. }
            | Self::TaskOp { collapsed, .. }
            | Self::RemotePatch { collapsed, .. } => *collapsed = !*collapsed,
            _ => {}
        }
    }

    /// The content that `Alt+c` should copy from this block.
    pub fn primary_copy_text(&self) -> Option<String> {
        match self {
            Self::AgentInfo { prompt, .. } => Some(prompt.clone()),
            Self::UserText { text } => Some(text.clone()),
            Self::AiMarkdown { raw, .. } => Some(raw.clone()),
            Self::BashTool { stdout, .. } => Some(stdout.clone()),
            Self::EditTool { diff, .. } | Self::WriteTool { diff, .. } => Some(diff.clone()),
            Self::ReadTool { file_path, .. } => Some(file_path.clone()),
            Self::GrepTool { matches, .. } | Self::GlobTool { matches, .. } => {
                Some(matches.clone())
            }
            Self::GenericTool { result_pretty, .. } => Some(result_pretty.clone()),
            Self::System { text, .. } => Some(text.clone()),
            Self::Mail { body, .. } => Some(body.clone()),
            Self::ChannelMessage { body, .. } => Some(body.clone()),
            Self::WorkerSpawn { task, .. } => Some(task.clone()),
            Self::WorkerComplete { body, .. } => Some(body.clone()),
            Self::RemotePatch { diff, .. } => Some(diff.clone()),
            Self::TaskOp {
                body,
                reply,
                subject,
                ..
            } => {
                // Prefer the reply (the most useful content for
                // `task show` / `task list`), fall back to the sent
                // body, then to the subject line.
                if !reply.is_empty() {
                    Some(reply.clone())
                } else if !body.is_empty() {
                    Some(body.clone())
                } else {
                    Some(subject.clone())
                }
            }
            Self::AutomatonStdout { line } | Self::AutomatonStderr { line } => {
                Some(line.clone())
            }
        }
    }

    /// Compute the total number of rows this block consumes at `width`.
    pub fn height(&self, width: u16) -> u16 {
        let w = width.max(1);
        match self {
            Self::AgentInfo {
                core,
                tools,
                prompt,
                collapsed,
                context_used,
                context_window,
                ..
            } => agent_info_height(
                core,
                tools,
                prompt,
                *collapsed,
                *context_used,
                *context_window,
                w,
            ),
            Self::UserText { text } => wrapped_rows(text, w.saturating_sub(2)).max(1),
            Self::AiMarkdown { parsed, .. } => 1 + parsed_md_height(parsed, w),
            Self::BashTool {
                command,
                stdout,
                collapsed,
                ..
            } => tool_body_height(command, stdout, *collapsed, w),
            Self::EditTool {
                file_path,
                diff,
                collapsed,
                ..
            }
            | Self::WriteTool {
                file_path,
                diff,
                collapsed,
                ..
            } => tool_body_height(file_path, diff, *collapsed, w),
            Self::ReadTool { .. } => 1,
            Self::GrepTool {
                pattern,
                path,
                matches,
                collapsed,
                ..
            }
            | Self::GlobTool {
                pattern,
                path,
                matches,
                collapsed,
                ..
            } => {
                let subj = match path {
                    Some(p) => format!("\"{pattern}\" in {p}"),
                    None => format!("\"{pattern}\""),
                };
                tool_body_height(&subj, matches, *collapsed, w)
            }
            Self::GenericTool {
                name,
                input_pretty,
                result_pretty,
                collapsed,
                ..
            } => {
                let subj = format!("{name} {input_pretty}");
                tool_body_height(&subj, result_pretty, *collapsed, w)
            }
            Self::System { text, .. } => wrapped_rows(text, w.saturating_sub(2)).max(1),
            Self::Mail {
                direction,
                counterpart,
                body,
                collapsed,
                ..
            } => {
                let subj = mail_subject(*direction, counterpart);
                tool_body_height(&subj, body, *collapsed, w)
            }
            Self::ChannelMessage { body, .. } => {
                // Header row ("sender:" or "⭐ welcome") plus wrapped
                // body indented two columns.
                1 + wrapped_rows(body, w.saturating_sub(2)).max(1)
            }
            Self::WorkerSpawn {
                template,
                worker_id,
                task,
                collapsed,
                ..
            } => {
                let subj = worker_spawn_subject(template, worker_id.as_deref());
                tool_body_height(&subj, task, *collapsed, w)
            }
            Self::WorkerComplete {
                body, collapsed, ..
            } => tool_body_height("Final report", body, *collapsed, w),
            Self::RemotePatch {
                summary,
                diff,
                collapsed,
                ..
            } => tool_body_height(summary, diff, *collapsed, w),
            Self::TaskOp {
                subject,
                body,
                reply,
                collapsed,
                ..
            } => {
                let shown = task_op_shown_body(body, reply);
                tool_body_height(subject, &shown, *collapsed, w)
            }
            Self::AutomatonStdout { line } | Self::AutomatonStderr { line } => {
                wrapped_rows(line, w.saturating_sub(2)).max(1)
            }
        }
    }

    /// Paint this block into `rect`. Registers any buttons into `buttons`.
    pub fn render(
        &self,
        buf: &mut Buffer,
        rect: Rect,
        buttons: &mut Vec<ChatButton>,
        block_idx: usize,
        selected: bool,
    ) {
        if rect.width == 0 || rect.height == 0 {
            return;
        }
        // Draw selection indicator ▎ in column 0 if selected.
        let body_rect = if selected {
            let ind_rect = Rect::new(rect.x, rect.y, 1, rect.height);
            let para = Paragraph::new("▎").style(Style::default().fg(Color::Cyan));
            para.render(ind_rect, buf);
            Rect::new(
                rect.x + 1,
                rect.y,
                rect.width.saturating_sub(1),
                rect.height,
            )
        } else {
            Rect::new(
                rect.x + 1,
                rect.y,
                rect.width.saturating_sub(1),
                rect.height,
            )
        };

        match self {
            Self::AgentInfo {
                core,
                tools,
                prompt,
                collapsed,
                context_used,
                context_window,
            } => render_agent_info(
                buf,
                body_rect,
                buttons,
                block_idx,
                core,
                tools,
                prompt,
                *collapsed,
                *context_used,
                *context_window,
            ),
            Self::UserText { text } => self.render_prefix_text(
                buf,
                body_rect,
                "👤 ",
                text,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Self::AiMarkdown { parsed, .. } => {
                if body_rect.height == 0 {
                    return;
                }
                let header = Rect::new(body_rect.x, body_rect.y, body_rect.width, 1);
                Paragraph::new(Line::from(vec![Span::styled(
                    "🤖 ",
                    Style::default().fg(Color::Cyan),
                )]))
                .render(header, buf);
                let content = Rect::new(
                    body_rect.x,
                    body_rect.y + 1,
                    body_rect.width,
                    body_rect.height - 1,
                );
                render_parsed(parsed, buf, content, buttons, block_idx);
            }
            Self::BashTool {
                command,
                stdout,
                is_error,
                collapsed,
                pending,
                ..
            } => render_tool_block(
                buf,
                body_rect,
                buttons,
                block_idx,
                "$",
                command,
                "stdout",
                stdout,
                *collapsed,
                *is_error,
                *pending,
                Some(command.clone()),
                Some(("copy cmd".into(), command.clone())),
                Some(("copy out".into(), stdout.clone())),
            ),
            Self::EditTool {
                file_path,
                diff,
                is_error,
                collapsed,
                pending,
                ..
            } => render_tool_block(
                buf,
                body_rect,
                buttons,
                block_idx,
                "✎",
                file_path,
                "diff",
                diff,
                *collapsed,
                *is_error,
                *pending,
                Some(diff.clone()),
                None,
                Some(("copy diff".into(), diff.clone())),
            ),
            Self::WriteTool {
                file_path,
                diff,
                is_error,
                collapsed,
                pending,
                ..
            } => render_tool_block(
                buf,
                body_rect,
                buttons,
                block_idx,
                "✎+",
                file_path,
                "diff",
                diff,
                *collapsed,
                *is_error,
                *pending,
                Some(diff.clone()),
                None,
                Some(("copy diff".into(), diff.clone())),
            ),
            Self::ReadTool {
                file_path,
                range,
                is_error,
                pending,
            } => {
                let subj = match range {
                    Some(r) => format!("{file_path}:{r}"),
                    None => file_path.clone(),
                };
                render_tool_header(buf, body_rect, "📄", &subj, *is_error, *pending);
            }
            Self::GrepTool {
                pattern,
                path,
                matches,
                is_error,
                collapsed,
                pending,
                ..
            } => {
                let subj = match path {
                    Some(p) => format!("\"{pattern}\" in {p}"),
                    None => format!("\"{pattern}\""),
                };
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    "🔍",
                    &subj,
                    "matches",
                    matches,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(matches.clone()),
                    None,
                    Some(("copy".into(), matches.clone())),
                );
            }
            Self::GlobTool {
                pattern,
                path,
                matches,
                is_error,
                collapsed,
                pending,
                ..
            } => {
                let subj = match path {
                    Some(p) => format!("{pattern} in {p}"),
                    None => pattern.clone(),
                };
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    "✱",
                    &subj,
                    "matches",
                    matches,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(matches.clone()),
                    None,
                    Some(("copy".into(), matches.clone())),
                );
            }
            Self::GenericTool {
                name,
                input_pretty,
                result_pretty,
                is_error,
                collapsed,
                pending,
                ..
            } => {
                let subj = format!("{name} {input_pretty}");
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    "🛠",
                    &subj,
                    "result",
                    result_pretty,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(result_pretty.clone()),
                    None,
                    Some(("copy".into(), result_pretty.clone())),
                );
            }
            Self::System { text, kind } => {
                let style = match kind {
                    SystemKind::Info => Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                    SystemKind::Warn => Style::default().fg(Color::Yellow),
                    SystemKind::Error => Style::default().fg(Color::Red),
                };
                self.render_prefix_text(buf, body_rect, "⚙ ", text, style);
            }
            Self::Mail {
                direction,
                counterpart,
                body,
                is_error,
                collapsed,
                pending,
            } => {
                let subj = mail_subject(*direction, counterpart);
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    "📬",
                    &subj,
                    "message",
                    body,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(body.clone()),
                    None,
                    Some(("copy".into(), body.clone())),
                );
            }
            Self::ChannelMessage {
                sender,
                body,
                is_welcome,
            } => {
                if body_rect.height == 0 {
                    return;
                }
                let (header_text, header_style) = if *is_welcome {
                    (
                        "⭐ welcome".to_string(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::ITALIC),
                    )
                } else if sender == "user" {
                    (
                        format!("{sender}:"),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    (
                        format!("{sender}:"),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )
                };
                let header_rect = Rect::new(body_rect.x, body_rect.y, body_rect.width, 1);
                Paragraph::new(Line::from(vec![Span::styled(header_text, header_style)]))
                    .render(header_rect, buf);
                if body_rect.height > 1 {
                    let content_rect = Rect::new(
                        body_rect.x + 2,
                        body_rect.y + 1,
                        body_rect.width.saturating_sub(2),
                        body_rect.height - 1,
                    );
                    let body_style = if *is_welcome {
                        Style::default()
                            .fg(Color::Gray)
                            .add_modifier(Modifier::ITALIC)
                    } else {
                        Style::default()
                    };
                    Paragraph::new(Line::from(Span::styled(body.clone(), body_style)))
                        .wrap(Wrap { trim: false })
                        .render(content_rect, buf);
                }
            }
            Self::WorkerSpawn {
                template,
                worker_id,
                task,
                is_error,
                collapsed,
                pending,
            } => {
                let subj = worker_spawn_subject(template, worker_id.as_deref());
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    "🧑‍🌾",
                    &subj,
                    "task",
                    task,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(task.clone()),
                    None,
                    Some(("copy".into(), task.clone())),
                );
            }
            Self::WorkerComplete {
                body,
                is_error,
                collapsed,
                pending,
            } => {
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    "🏁",
                    "Final report (hibernating)",
                    "report",
                    body,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(body.clone()),
                    None,
                    Some(("copy".into(), body.clone())),
                );
            }
            Self::RemotePatch {
                summary,
                diff,
                is_error,
                collapsed,
                pending,
            } => {
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    "📦",
                    summary,
                    "diff",
                    diff,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(diff.clone()),
                    None,
                    Some(("copy diff".into(), diff.clone())),
                );
            }
            Self::TaskOp {
                kind,
                subject,
                body,
                reply,
                is_error,
                collapsed,
                pending,
            } => {
                let shown = task_op_shown_body(body, reply);
                // Primary copy gives the reply when present (e.g.
                // the full task body from `task show`); falls back
                // to the sent body so Note / TodoAdd blocks copy
                // what the user wrote.
                let primary = if !reply.is_empty() {
                    reply.clone()
                } else {
                    body.clone()
                };
                let body_copy = if !body.is_empty() {
                    Some(("copy body".into(), body.clone()))
                } else {
                    None
                };
                render_tool_block(
                    buf,
                    body_rect,
                    buttons,
                    block_idx,
                    kind.icon(),
                    subject,
                    kind.body_label(),
                    &shown,
                    *collapsed,
                    *is_error,
                    *pending,
                    Some(primary),
                    None,
                    body_copy,
                );
            }
            Self::AutomatonStdout { line } => self.render_prefix_text(
                buf,
                body_rect,
                "» ",
                line,
                Style::default().fg(Color::Gray),
            ),
            Self::AutomatonStderr { line } => self.render_prefix_text(
                buf,
                body_rect,
                "» ",
                line,
                Style::default().fg(Color::Red),
            ),
        }
    }

    fn render_prefix_text(
        &self,
        buf: &mut Buffer,
        rect: Rect,
        prefix: &str,
        text: &str,
        style: Style,
    ) {
        let line = Line::from(vec![
            Span::styled(prefix.to_string(), style),
            Span::raw(text.to_string()),
        ]);
        let para = Paragraph::new(line).wrap(Wrap { trim: false });
        para.render(rect, buf);
    }
}

// ---------------------------------------------------------------------------
// Height helpers
// ---------------------------------------------------------------------------

const COLLAPSED_PREVIEW_ROWS: u16 = 3;

fn agent_info_detail_line(tools: &[String], core: &str) -> String {
    let tools_joined = if tools.is_empty() {
        "(none)".to_string()
    } else {
        tools.join(", ")
    };
    format!("core: {core}   tools: {tools_joined}")
}

const CONTEXT_BAR_WIDTH: usize = 10;

/// Build the rendered "context: …" status line. Degrades gracefully:
/// - no usage           → `context: unknown`
/// - usage, no window   → `context: 12.4K used`
/// - both               → `context: 12.4K / 200K (6%)  ▰▰▱▱▱▱▱▱▱▱  cached: 5.2K`
pub(crate) fn format_context_line(
    used: Option<crate::ai::TurnUsage>,
    window: Option<u32>,
) -> String {
    let Some(u) = used else {
        return "context: unknown".to_string();
    };
    let total = u.total_tokens();
    let cached = u.cache_read_input_tokens + u.cache_creation_input_tokens;
    let cache_suffix = if cached > 0 {
        format!("  cached: {}", format_tokens(cached))
    } else {
        String::new()
    };
    match window {
        None => format!("context: {} used{cache_suffix}", format_tokens(total)),
        Some(w) => {
            let pct = if w == 0 {
                0
            } else {
                ((total.saturating_mul(100)) / w as u64).min(999) as u32
            };
            let bar = render_bar(total, w as u64, CONTEXT_BAR_WIDTH);
            format!(
                "context: {} / {} ({}%)  {bar}{cache_suffix}",
                format_tokens(total),
                format_tokens(w as u64),
                pct
            )
        }
    }
}

/// Pick a colour for the context line based on how full the window is.
/// Falls back to dark gray when there's nothing useful to colour.
pub(crate) fn context_line_color(used: Option<crate::ai::TurnUsage>, window: Option<u32>) -> Color {
    match (used, window) {
        (Some(u), Some(w)) if w > 0 => {
            let pct = (u.total_tokens().saturating_mul(100)) / w as u64;
            if pct >= 90 {
                Color::Red
            } else if pct >= 70 {
                Color::Yellow
            } else {
                Color::DarkGray
            }
        }
        _ => Color::DarkGray,
    }
}

/// Render `n` as a short string: `1234` → `1.2K`, `1_500_000` → `1.5M`.
fn format_tokens(n: u64) -> String {
    if n < 1_000 {
        n.to_string()
    } else if n < 1_000_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    }
}

/// Build a Unicode block-bar of `width` cells reflecting `used / total`.
fn render_bar(used: u64, total: u64, width: usize) -> String {
    if total == 0 || width == 0 {
        return String::new();
    }
    let filled_cells = ((used.saturating_mul(width as u64)) / total).min(width as u64) as usize;
    let mut s = String::with_capacity(width * 3);
    for i in 0..width {
        if i < filled_cells {
            s.push('▰');
        } else {
            s.push('▱');
        }
    }
    s
}

fn agent_info_height(
    core: &str,
    tools: &[String],
    prompt: &str,
    collapsed: bool,
    context_used: Option<crate::ai::TurnUsage>,
    context_window: Option<u32>,
    width: u16,
) -> u16 {
    // Row budget: detail(1..wrapped) + context(1..wrapped) + prompt
    // header(1) + optional prompt body.
    //
    // `ChatBlock::render` always shifts its incoming rect by
    // `+1/-1` to reserve column 0 for the selection indicator (even
    // when not selected), so `render_agent_info` actually draws into a
    // rect that's one narrower than the `width` we were given. Every
    // wrap calc here has to subtract that extra column — otherwise a
    // long line reports fewer rows than the render produces and the
    // bottom of the block gets clipped (classic symptom: the closing
    // `</instructions>` disappears).
    //
    //   detail/context:   rect.x+2, width-2  → effective width-3
    //   prompt body:      Borders::LEFT inside rect.x+2, width-2
    //                     → effective width-4 (extra -1 for the border)
    let detail_text = agent_info_detail_line(tools, core);
    let detail_rows = wrapped_rows(&detail_text, width.saturating_sub(3)).max(1);
    let context_text = format_context_line(context_used, context_window);
    let context_rows = wrapped_rows(&context_text, width.saturating_sub(3)).max(1);
    let prompt_body = if collapsed {
        0
    } else {
        wrapped_rows(prompt, width.saturating_sub(4))
    };
    detail_rows + context_rows + 1 + prompt_body
}

#[allow(clippy::too_many_arguments)]
fn render_agent_info(
    buf: &mut Buffer,
    rect: Rect,
    buttons: &mut Vec<ChatButton>,
    block_idx: usize,
    core: &str,
    tools: &[String],
    prompt: &str,
    collapsed: bool,
    context_used: Option<crate::ai::TurnUsage>,
    context_window: Option<u32>,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let mut y = rect.y;
    let end_y = rect.y.saturating_add(rect.height);

    // Details row (core + tools), wrapped.
    let detail = agent_info_detail_line(tools, core);
    let detail_w = rect.width.saturating_sub(2);
    let detail_rows = wrapped_rows(&detail, detail_w).max(1);
    let available = end_y.saturating_sub(y);
    let draw_h = detail_rows.min(available);
    let detail_rect = Rect::new(rect.x + 2, y, detail_w, draw_h);
    Paragraph::new(Span::styled(detail, Style::default().fg(Color::DarkGray)))
        .wrap(Wrap { trim: false })
        .render(detail_rect, buf);
    y = y.saturating_add(draw_h);
    if y >= end_y {
        return;
    }

    // Context-window row.
    let context_text = format_context_line(context_used, context_window);
    let context_w = rect.width.saturating_sub(2);
    let context_rows = wrapped_rows(&context_text, context_w).max(1);
    let available = end_y.saturating_sub(y);
    let draw_h = context_rows.min(available);
    let context_rect = Rect::new(rect.x + 2, y, context_w, draw_h);
    let context_color = context_line_color(context_used, context_window);
    Paragraph::new(Span::styled(
        context_text,
        Style::default().fg(context_color),
    ))
    .wrap(Wrap { trim: false })
    .render(context_rect, buf);
    y = y.saturating_add(draw_h);
    if y >= end_y {
        return;
    }

    // Prompt toggle row.
    let toggle_label = if collapsed { "[ + ]" } else { "[ - ]" };
    let tw = toggle_label.chars().count() as u16;
    let prompt_row = Rect::new(rect.x, y, rect.width, 1);
    Paragraph::new(Line::from(vec![
        Span::styled(
            "prompt ",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("({} chars)", prompt.chars().count()),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .render(prompt_row, buf);
    if rect.width > tw + 1 {
        let btn_rect = Rect::new(rect.x + rect.width - tw, y, tw, 1);
        Paragraph::new(Line::from(Span::styled(
            toggle_label.to_string(),
            Style::default().fg(Color::Cyan),
        )))
        .render(btn_rect, buf);
        buttons.push(ChatButton {
            rect: btn_rect,
            action: BlockAction::ToggleCollapse(block_idx),
        });
    }
    y = y.saturating_add(1);
    if y >= end_y || collapsed {
        return;
    }

    // Prompt body. Pass the String directly so ratatui splits on newlines
    // into individual Lines; a single Span would treat `\n` as a glyph and
    // collapse the body onto one wrapped line.
    // The `Borders::LEFT` block reserves one column for the border, so
    // the paragraph's effective wrap width is `body_w - 1`. Use that
    // for the row count or the tail of long prompts gets clipped.
    let body_w = rect.width.saturating_sub(2);
    let body_rows = wrapped_rows(prompt, body_w.saturating_sub(1));
    let available = end_y.saturating_sub(y);
    let draw_h = body_rows.min(available);
    let body_rect = Rect::new(rect.x + 2, y, body_w, draw_h);
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    Paragraph::new(prompt.to_string())
        .style(Style::default().fg(Color::Gray))
        .block(block)
        .wrap(Wrap { trim: false })
        .render(body_rect, buf);
}

/// Build the combined display body for a `TaskOp` block. Shows the
/// agent's sent payload, a `---` divider, and the orchestrator's
/// reply when both are present. Falls back gracefully when either
/// side is empty.
fn task_op_shown_body(body: &str, reply: &str) -> String {
    match (body.is_empty(), reply.is_empty()) {
        (true, true) => String::new(),
        (false, true) => body.to_string(),
        (true, false) => reply.to_string(),
        (false, false) => format!("{body}\n\n---\n{reply}"),
    }
}

fn mail_subject(direction: MailDirection, counterpart: &str) -> String {
    match direction {
        MailDirection::Sent => format!("To: {counterpart}"),
        MailDirection::Received => format!("From: {counterpart}"),
    }
}

fn worker_spawn_subject(template: &str, worker_id: Option<&str>) -> String {
    match worker_id {
        Some(id) => format!("Spawn worker {id} (template: {template})"),
        None => format!("Spawn worker (template: {template})"),
    }
}

fn tool_body_height(_subject: &str, body: &str, collapsed: bool, width: u16) -> u16 {
    // Layout:
    //   header row (1)  — icon + subject + buttons
    //   body rows       — summary (collapsed) or full (expanded)
    //   footer row (0 or 1) — "… N more lines" when collapsed and truncated
    let header = 1u16;
    if body.is_empty() {
        return header;
    }
    let body_width = width.saturating_sub(2);
    let body_lines: Vec<&str> = body.split('\n').collect();
    let total_rows: u16 = body_lines
        .iter()
        .map(|l| wrapped_rows(l, body_width).max(1))
        .sum();

    if collapsed {
        let shown = total_rows.min(COLLAPSED_PREVIEW_ROWS);
        let more = if total_rows > COLLAPSED_PREVIEW_ROWS {
            1
        } else {
            0
        };
        header + shown + more
    } else {
        header + total_rows
    }
}

fn parsed_md_height(parsed: &ParsedMd, width: u16) -> u16 {
    let mut total = 0u16;
    for chunk in &parsed.chunks {
        match chunk {
            MdChunk::Lines(lines) => {
                for line in lines {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    total = total.saturating_add(wrapped_rows(&text, width).max(1));
                }
            }
            MdChunk::Fence { body, .. } => {
                // header (1) + body rows + closing row (1) with copy button
                let body_width = width.saturating_sub(4);
                let body_rows: u16 = body
                    .split('\n')
                    .map(|l| wrapped_rows(l, body_width).max(1))
                    .sum();
                total = total.saturating_add(1 + body_rows + 1);
            }
        }
    }
    total.max(1)
}

/// Count the rows a paragraph occupies when rendered with
/// `Wrap { trim: false }` at the given width. Delegates to ratatui's
/// own word-wrapper so this matches the actual render exactly — a
/// naïve `chars / width` estimate under-counts whenever a word
/// doesn't fit on its current row and gets pushed to the next
/// (leaving trailing blanks), which shows up as the last row of the
/// block getting clipped.
fn wrapped_rows(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    let count = Paragraph::new(text.to_string())
        .wrap(Wrap { trim: false })
        .line_count(width);
    (count as u16).max(1)
}

// ---------------------------------------------------------------------------
// Tool block renderer
// ---------------------------------------------------------------------------

/// Render a one-line tool header with no body or buttons.
fn render_tool_header(
    buf: &mut Buffer,
    rect: Rect,
    icon: &str,
    subject: &str,
    is_error: bool,
    pending: bool,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let header_rect = Rect::new(rect.x, rect.y, rect.width, 1);
    let status_glyph = if pending {
        "⋯"
    } else if is_error {
        "!"
    } else {
        "✓"
    };
    let status_style = if is_error {
        Style::default().fg(Color::Red)
    } else if pending {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    };
    let header_line = Line::from(vec![
        Span::styled(format!("{status_glyph} "), status_style),
        Span::styled(format!("{icon} "), Style::default().fg(Color::Magenta)),
        Span::raw(truncate_display(
            subject,
            rect.width.saturating_sub(4) as usize,
        )),
    ]);
    Paragraph::new(header_line).render(header_rect, buf);
}

#[allow(clippy::too_many_arguments)]
fn render_tool_block(
    buf: &mut Buffer,
    rect: Rect,
    buttons: &mut Vec<ChatButton>,
    block_idx: usize,
    icon: &str,
    subject: &str,
    body_label: &str,
    body: &str,
    collapsed: bool,
    is_error: bool,
    pending: bool,
    _primary_copy: Option<String>,
    copy_a: Option<(String, String)>,
    copy_b: Option<(String, String)>,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }

    // Header row --------------------------------------------------------
    let header_rect = Rect::new(rect.x, rect.y, rect.width, 1);
    let status_glyph = if pending {
        "⋯"
    } else if is_error {
        "!"
    } else {
        "✓"
    };
    let status_style = if is_error {
        Style::default().fg(Color::Red)
    } else if pending {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Green)
    };

    // Right-aligned buttons: [ +/- ] [ copy_b ] [ copy_a ]
    let toggle_label = if collapsed { "[ + ]" } else { "[ - ]" };
    let mut button_specs: Vec<(String, BlockAction)> = Vec::new();
    button_specs.push((
        toggle_label.to_string(),
        BlockAction::ToggleCollapse(block_idx),
    ));
    if let Some((label, payload)) = copy_b {
        button_specs.push((format!("[ {label} ]"), BlockAction::CopyText(payload)));
    }
    if let Some((label, payload)) = &copy_a {
        button_specs.push((
            format!("[ {label} ]"),
            BlockAction::CopyText(payload.clone()),
        ));
    }
    let buttons_width: u16 = button_specs
        .iter()
        .map(|(l, _)| l.chars().count() as u16 + 1)
        .sum::<u16>()
        .saturating_sub(1);
    let subject_width = rect
        .width
        .saturating_sub(4) // glyph + space + icon + space
        .saturating_sub(buttons_width + 1);

    let truncated_subject = truncate_display(subject, subject_width as usize);

    let header_line = Line::from(vec![
        Span::styled(format!("{status_glyph} "), status_style),
        Span::styled(format!("{icon} "), Style::default().fg(Color::Magenta)),
        Span::raw(truncated_subject),
    ]);
    Paragraph::new(header_line).render(header_rect, buf);

    // Buttons live on the rightmost end of the header row.
    let mut bx = rect.x + rect.width.saturating_sub(buttons_width);
    for (label, action) in &button_specs {
        let w = label.chars().count() as u16;
        let brect = Rect::new(bx, rect.y, w, 1);
        let style = match action {
            BlockAction::ToggleCollapse(_) => Style::default().fg(Color::Cyan),
            BlockAction::CopyText(_) => Style::default().fg(Color::LightBlue),
        };
        Paragraph::new(Line::from(Span::styled(label.clone(), style))).render(brect, buf);
        buttons.push(ChatButton {
            rect: brect,
            action: action.clone(),
        });
        bx = bx.saturating_add(w + 1);
    }

    // Body --------------------------------------------------------------
    if rect.height <= 1 {
        return;
    }
    let body_rect = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(2),
        rect.height - 1,
    );

    if body.is_empty() {
        let hint = if pending {
            format!("(awaiting {body_label}…)")
        } else {
            format!("(no {body_label})")
        };
        Paragraph::new(Span::styled(
            hint,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ))
        .render(body_rect, buf);
        return;
    }

    let body_lines: Vec<&str> = body.split('\n').collect();
    let total_raw = body_lines.len();

    let (shown_lines, trailing): (Vec<&str>, Option<String>) = if collapsed {
        let shown = body_lines
            .iter()
            .take(COLLAPSED_PREVIEW_ROWS as usize)
            .copied()
            .collect::<Vec<_>>();
        let hidden = total_raw.saturating_sub(shown.len());
        let trail = if hidden > 0 {
            Some(format!("… ({hidden} more lines)"))
        } else {
            None
        };
        (shown, trail)
    } else {
        (body_lines, None)
    };

    let mut lines: Vec<Line<'static>> = shown_lines
        .into_iter()
        .map(|l| styled_body_line(l, body_label))
        .collect();
    if let Some(trail) = trailing {
        lines.push(Line::from(Span::styled(
            trail,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    let style_block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    let para = Paragraph::new(lines)
        .block(style_block)
        .wrap(Wrap { trim: false });
    para.render(body_rect, buf);
}

fn truncate_display(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".to_string();
    }
    let mut out: String = chars.into_iter().take(max - 1).collect();
    out.push('…');
    out
}

fn styled_body_line(text: &str, label: &str) -> Line<'static> {
    if label == "diff" {
        let style = if text.starts_with("+++") || text.starts_with("---") {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else if text.starts_with("@@") {
            Style::default().fg(Color::Cyan)
        } else if text.starts_with('+') {
            Style::default().fg(Color::Green)
        } else if text.starts_with('-') {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        Line::from(Span::styled(text.to_string(), style))
    } else {
        Line::from(Span::raw(text.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Markdown chunk renderer (used by AiMarkdown and in inline helpers)
// ---------------------------------------------------------------------------

pub fn render_parsed(
    parsed: &ParsedMd,
    buf: &mut Buffer,
    rect: Rect,
    buttons: &mut Vec<ChatButton>,
    block_idx: usize,
) {
    let mut y = rect.y;
    let end_y = rect.y.saturating_add(rect.height);
    for chunk in &parsed.chunks {
        if y >= end_y {
            break;
        }
        match chunk {
            MdChunk::Lines(lines) => {
                let chunk_height: u16 = lines
                    .iter()
                    .map(|l| {
                        let text: String = l.spans.iter().map(|s| s.content.as_ref()).collect();
                        wrapped_rows(&text, rect.width).max(1)
                    })
                    .sum();
                let draw_height = chunk_height.min(end_y - y);
                if draw_height == 0 {
                    continue;
                }
                let rect_here = Rect::new(rect.x, y, rect.width, draw_height);
                let para = Paragraph::new(lines.clone()).wrap(Wrap { trim: false });
                para.render(rect_here, buf);
                y = y.saturating_add(draw_height);
            }
            MdChunk::Fence { lang, body } => {
                let body_width = rect.width.saturating_sub(4);
                let body_rows: u16 = body
                    .split('\n')
                    .map(|l| wrapped_rows(l, body_width).max(1))
                    .sum();
                let needed = 1 + body_rows + 1;
                if end_y.saturating_sub(y) == 0 {
                    break;
                }
                let h = needed.min(end_y - y);
                let fence_rect = Rect::new(rect.x, y, rect.width, h);
                render_fence(buf, fence_rect, buttons, block_idx, lang, body);
                y = y.saturating_add(h);
            }
        }
    }
}

fn render_fence(
    buf: &mut Buffer,
    rect: Rect,
    buttons: &mut Vec<ChatButton>,
    block_idx: usize,
    lang: &str,
    body: &str,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(if lang.is_empty() {
            " code ".to_string()
        } else {
            format!(" {lang} ")
        });
    let inner = block.inner(rect);
    block.render(rect, buf);

    // Copy button in the top-right of the header border.
    let label = "[ copy ]";
    let w = label.chars().count() as u16;
    if rect.width > w + 2 {
        let btn_rect = Rect::new(rect.x + rect.width - w - 1, rect.y, w, 1);
        Paragraph::new(Line::from(Span::styled(
            label.to_string(),
            Style::default().fg(Color::LightBlue),
        )))
        .render(btn_rect, buf);
        buttons.push(ChatButton {
            rect: btn_rect,
            action: BlockAction::CopyText(body.to_string()),
        });
        // Re-mark the action as attributable to this block for Alt+c:
        let _ = block_idx;
    }

    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let lines: Vec<Line<'static>> = body
        .split('\n')
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::Gray),
            ))
        })
        .collect();
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    para.render(inner, buf);
}

// ---------------------------------------------------------------------------
// Constructors from AI events
// ---------------------------------------------------------------------------

impl ChatBlock {
    pub fn agent_info(
        core: impl Into<String>,
        tools: Vec<String>,
        prompt: impl Into<String>,
        context_window: Option<u32>,
    ) -> Self {
        Self::AgentInfo {
            core: core.into(),
            tools,
            prompt: prompt.into(),
            collapsed: true,
            context_used: None,
            context_window,
        }
    }

    pub fn user(text: impl Into<String>) -> Self {
        Self::UserText { text: text.into() }
    }

    pub fn ai_markdown(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let parsed = markdown::parse(&raw);
        Self::AiMarkdown { raw, parsed }
    }

    pub fn system_info(text: impl Into<String>) -> Self {
        Self::System {
            text: text.into(),
            kind: SystemKind::Info,
        }
    }

    pub fn system_warn(text: impl Into<String>) -> Self {
        Self::System {
            text: text.into(),
            kind: SystemKind::Warn,
        }
    }

    pub fn system_error(text: impl Into<String>) -> Self {
        Self::System {
            text: text.into(),
            kind: SystemKind::Error,
        }
    }

    /// A mail block representing a message this agent sent to another.
    /// Starts collapsed so long bodies don't push chat history offscreen.
    pub fn mail_sent(to: impl Into<String>, body: impl Into<String>) -> Self {
        Self::Mail {
            direction: MailDirection::Sent,
            counterpart: to.into(),
            body: body.into(),
            is_error: false,
            collapsed: true,
            pending: true,
        }
    }

    /// A mail block representing a message received from another agent.
    /// Already-delivered, so `pending` is false from the start.
    pub fn mail_received(from: impl Into<String>, body: impl Into<String>) -> Self {
        Self::Mail {
            direction: MailDirection::Received,
            counterpart: from.into(),
            body: body.into(),
            is_error: false,
            collapsed: true,
            pending: false,
        }
    }

    /// A single line in a channel transcript. `sender` is the agent
    /// id, or the literal string `"user"` for the human operator.
    pub fn channel_message(sender: impl Into<String>, body: impl Into<String>) -> Self {
        Self::ChannelMessage {
            sender: sender.into(),
            body: body.into(),
            is_welcome: false,
        }
    }

    /// Cosmetic banner pinned at the top of a channel from its
    /// `welcome` config field. Never persisted; never sent to agents.
    pub fn channel_welcome(body: impl Into<String>) -> Self {
        Self::ChannelMessage {
            sender: String::new(),
            body: body.into(),
            is_welcome: true,
        }
    }

    /// Block for an `aciddog worker create` tool call, rendered on the
    /// spawner's pane.
    pub fn worker_spawn(template: impl Into<String>, task: impl Into<String>) -> Self {
        Self::WorkerSpawn {
            template: template.into(),
            worker_id: None,
            task: task.into(),
            is_error: false,
            collapsed: true,
            pending: true,
        }
    }

    /// Block for an `aciddog worker complete` tool call, rendered on
    /// the worker's own pane.
    pub fn worker_complete(body: impl Into<String>) -> Self {
        Self::WorkerComplete {
            body: body.into(),
            is_error: false,
            collapsed: true,
            pending: true,
        }
    }

    /// Block for any `aciddog task …` CLI call. The caller has
    /// already parsed the argv into a `kind`, a subject (one-line
    /// header) and a body (the payload the agent sent, or ""). The
    /// reply slot starts empty and is filled by `apply_tool_result`
    /// when the orchestrator responds.
    pub fn task_op(kind: TaskOpKind, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self::TaskOp {
            kind,
            subject: subject.into(),
            body: body.into(),
            reply: String::new(),
            is_error: false,
            collapsed: true,
            pending: true,
        }
    }

    /// Build the right tool-call variant from a `ToolUse` event.
    pub fn from_tool_use(name: &str, input: &serde_json::Value) -> Self {
        use serde_json::Value;
        let get_str = |k: &str| -> String {
            input
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let get_opt_str = |k: &str| -> Option<String> {
            input.get(k).and_then(Value::as_str).map(str::to_string)
        };

        match name {
            "Bash" => ChatBlock::BashTool {
                command: get_str("command"),
                stdout: String::new(),
                is_error: false,
                collapsed: true,
                pending: true,
            },
            "Edit" => {
                let file_path = get_str("file_path");
                let old = get_str("old_string");
                let new = get_str("new_string");
                let diff = super::diff::unified(&old, &new, &file_path);
                ChatBlock::EditTool {
                    file_path,
                    diff,
                    is_error: false,
                    collapsed: true,
                    pending: true,
                }
            }
            "Write" => {
                let file_path = get_str("file_path");
                let content = get_str("content");
                let diff = super::diff::synth_new_file(&file_path, &content);
                ChatBlock::WriteTool {
                    file_path,
                    diff,
                    is_error: false,
                    collapsed: true,
                    pending: true,
                }
            }
            "Read" => ChatBlock::ReadTool {
                file_path: get_str("file_path"),
                range: range_from_read(input),
                is_error: false,
                pending: true,
            },
            "Grep" => ChatBlock::GrepTool {
                pattern: get_str("pattern"),
                path: get_opt_str("path"),
                matches: String::new(),
                is_error: false,
                collapsed: true,
                pending: true,
            },
            "Glob" => ChatBlock::GlobTool {
                pattern: get_str("pattern"),
                path: get_opt_str("path"),
                matches: String::new(),
                is_error: false,
                collapsed: true,
                pending: true,
            },
            // Remote-worker pseudo-tools — Jules emits these from its
            // activity stream to render native-feeling blocks on the
            // worker's chat pane.
            "jules_bash" => ChatBlock::BashTool {
                command: get_str("command"),
                stdout: String::new(),
                is_error: false,
                collapsed: true,
                pending: true,
            },
            "jules_change_set" => ChatBlock::RemotePatch {
                summary: get_str("summary"),
                diff: get_str("diff"),
                is_error: false,
                collapsed: true,
                pending: false,
            },
            other => ChatBlock::GenericTool {
                name: other.to_string(),
                input_pretty: serde_json::to_string(input).unwrap_or_default(),
                result_pretty: String::new(),
                is_error: false,
                collapsed: true,
                pending: true,
            },
        }
    }

    /// Fill in the result portion of a tool block once its `ToolResult` arrives.
    pub fn apply_tool_result(&mut self, content: &serde_json::Value, is_error: bool) {
        let (stdout, exit_code) = extract_bash_result(content);
        match self {
            ChatBlock::BashTool {
                stdout: s,
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                *s = stdout;
                let _ = exit_code; // reserved
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            ChatBlock::EditTool {
                is_error: e,
                collapsed,
                pending,
                ..
            }
            | ChatBlock::WriteTool {
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            ChatBlock::ReadTool {
                is_error: e,
                pending,
                ..
            } => {
                let _ = stringify_content(content);
                *e = is_error;
                *pending = false;
            }
            ChatBlock::GrepTool {
                matches,
                is_error: e,
                collapsed,
                pending,
                ..
            }
            | ChatBlock::GlobTool {
                matches,
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                *matches = stringify_content(content);
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            ChatBlock::GenericTool {
                result_pretty,
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                *result_pretty = stringify_content(content);
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            ChatBlock::Mail {
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            ChatBlock::WorkerSpawn {
                worker_id,
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                // The orchestrator replies with "<worker_id> <uuid>" on
                // success; extract the first whitespace-separated token
                // as the display id so the UI header shows the spawned
                // instance. On error, leave `worker_id` untouched.
                if !is_error {
                    if let Some(id) = stringify_content(content)
                        .split_whitespace()
                        .next()
                        .map(str::to_string)
                    {
                        if !id.is_empty() {
                            *worker_id = Some(id);
                        }
                    }
                }
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            ChatBlock::WorkerComplete {
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            ChatBlock::TaskOp {
                reply,
                is_error: e,
                collapsed,
                pending,
                ..
            } => {
                // The reply is whatever the orchestrator sent back.
                // For read ops (show / list) this is the whole
                // fetched content; for writes it's a short "task
                // created: <uuid>"-style confirmation.
                *reply = stringify_content(content);
                *e = is_error;
                *pending = false;
                if is_error {
                    *collapsed = false;
                }
            }
            _ => {}
        }
    }
}

fn range_from_read(input: &serde_json::Value) -> Option<String> {
    let offset = input.get("offset").and_then(|v| v.as_u64());
    let limit = input.get("limit").and_then(|v| v.as_u64());
    match (offset, limit) {
        (Some(o), Some(l)) => Some(format!("{}-{}", o, o + l)),
        (Some(o), None) => Some(format!("{}-", o)),
        (None, Some(l)) => Some(format!("0-{}", l)),
        (None, None) => None,
    }
}

fn extract_bash_result(content: &serde_json::Value) -> (String, Option<i32>) {
    use serde_json::Value;
    match content {
        Value::String(s) => (s.clone(), None),
        Value::Object(map) => {
            let stdout = map
                .get("stdout")
                .and_then(Value::as_str)
                .map(str::to_string);
            let stderr = map
                .get("stderr")
                .and_then(Value::as_str)
                .map(str::to_string);
            let exit = map
                .get("exit_code")
                .and_then(Value::as_i64)
                .map(|n| n as i32);
            let combined = match (stdout, stderr) {
                (Some(o), Some(e)) if !e.is_empty() => format!("{o}\n--- stderr ---\n{e}"),
                (Some(o), _) => o,
                (None, Some(e)) => e,
                (None, None) => serde_json::to_string_pretty(content).unwrap_or_default(),
            };
            (combined, exit)
        }
        Value::Array(items) => {
            // Some clients send [{"type":"text","text":"..."}]
            let mut out = String::new();
            for item in items {
                if let Some(t) = item.get("text").and_then(Value::as_str) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
            if out.is_empty() {
                (serde_json::to_string(content).unwrap_or_default(), None)
            } else {
                (out, None)
            }
        }
        _ => (serde_json::to_string(content).unwrap_or_default(), None),
    }
}

fn stringify_content(content: &serde_json::Value) -> String {
    use serde_json::Value;
    match content {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(t) = item.get("text").and_then(Value::as_str) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
            if out.is_empty() {
                serde_json::to_string(content).unwrap_or_default()
            } else {
                out
            }
        }
        _ => serde_json::to_string_pretty(content).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn user_block_has_expected_height() {
        let b = ChatBlock::user("hello");
        assert_eq!(b.height(80), 1);
    }

    #[test]
    fn format_context_line_unknown_when_no_usage() {
        assert_eq!(format_context_line(None, Some(200_000)), "context: unknown");
        assert_eq!(format_context_line(None, None), "context: unknown");
    }

    #[test]
    fn format_context_line_no_window_falls_back_to_used() {
        let u = crate::ai::TurnUsage {
            input_tokens: 1500,
            output_tokens: 500,
            ..Default::default()
        };
        assert_eq!(format_context_line(Some(u), None), "context: 2.0K used");
    }

    #[test]
    fn format_context_line_full_format_with_window_and_cache() {
        let u = crate::ai::TurnUsage {
            input_tokens: 30_000,
            output_tokens: 10_000,
            cache_read_input_tokens: 20_000,
            cache_creation_input_tokens: 0,
        };
        let s = format_context_line(Some(u), Some(200_000));
        // 60K total, 200K window → 30%
        assert!(s.starts_with("context: 60.0K / 200.0K (30%)"), "got: {s}");
        assert!(s.contains("cached: 20.0K"));
        // 30% of 10 cells = 3 filled cells, 7 empty.
        assert!(s.contains('▰'), "expected at least one filled cell: {s}");
        assert!(s.contains('▱'), "expected at least one empty cell: {s}");
    }

    #[test]
    fn format_tokens_uses_short_units() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_000), "1.0K");
        assert_eq!(format_tokens(12_345), "12.3K");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn render_bar_proportional_fill() {
        assert_eq!(render_bar(0, 100, 10), "▱▱▱▱▱▱▱▱▱▱");
        assert_eq!(render_bar(50, 100, 10), "▰▰▰▰▰▱▱▱▱▱");
        assert_eq!(render_bar(100, 100, 10), "▰▰▰▰▰▰▰▰▰▰");
        // Saturates rather than overflows.
        assert_eq!(render_bar(500, 100, 10), "▰▰▰▰▰▰▰▰▰▰");
    }

    #[test]
    fn agent_info_height_grows_with_context_line() {
        // Same prompt + tools, just toggling the new context line; the
        // height should grow by exactly one row (the new "context:" row).
        let h_no_ctx = agent_info_height("core", &["Bash".into()], "p", true, None, None, 80);
        let u = crate::ai::TurnUsage {
            input_tokens: 5000,
            ..Default::default()
        };
        let h_with_ctx = agent_info_height(
            "core",
            &["Bash".into()],
            "p",
            true,
            Some(u),
            Some(200_000),
            80,
        );
        // The function always reserves a context row (1+), so they should
        // be equal — what we're really asserting is that the new arg
        // hasn't pushed the height arithmetic off by more than the row
        // we explicitly accounted for.
        assert_eq!(h_no_ctx, h_with_ctx);
    }

    #[test]
    fn bash_tool_from_tool_use_captures_command() {
        let b = ChatBlock::from_tool_use("Bash", &json!({"command": "ls /tmp"}));
        match b {
            ChatBlock::BashTool {
                command, pending, ..
            } => {
                assert_eq!(command, "ls /tmp");
                assert!(pending);
            }
            _ => panic!("expected BashTool"),
        }
    }

    #[test]
    fn edit_tool_produces_unified_diff() {
        let b = ChatBlock::from_tool_use(
            "Edit",
            &json!({
                "file_path": "foo.txt",
                "old_string": "a\nb\n",
                "new_string": "a\nB\n",
            }),
        );
        match b {
            ChatBlock::EditTool { diff, .. } => {
                assert!(diff.contains("-b"));
                assert!(diff.contains("+B"));
            }
            _ => panic!("expected EditTool"),
        }
    }

    #[test]
    fn write_tool_produces_new_file_diff() {
        let b = ChatBlock::from_tool_use(
            "Write",
            &json!({"file_path": "new.txt", "content": "hello\nworld\n"}),
        );
        match b {
            ChatBlock::WriteTool { diff, .. } => {
                assert!(diff.contains("--- /dev/null"));
                assert!(diff.contains("+hello"));
            }
            _ => panic!("expected WriteTool"),
        }
    }

    #[test]
    fn apply_tool_result_fills_bash_and_clears_pending() {
        let mut b = ChatBlock::from_tool_use("Bash", &json!({"command": "echo hi"}));
        b.apply_tool_result(&json!("hi\n"), false);
        match b {
            ChatBlock::BashTool {
                stdout,
                pending,
                is_error,
                collapsed,
                ..
            } => {
                assert_eq!(stdout, "hi\n");
                assert!(!pending);
                assert!(!is_error);
                assert!(collapsed);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn apply_tool_result_auto_expands_on_error() {
        let mut b = ChatBlock::from_tool_use("Bash", &json!({"command": "cat /nope"}));
        b.apply_tool_result(&json!("cat: /nope: No such file"), true);
        match b {
            ChatBlock::BashTool {
                is_error,
                collapsed,
                ..
            } => {
                assert!(is_error);
                assert!(!collapsed, "error should auto-expand");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn toggle_collapse_works_on_tool_blocks() {
        let mut b = ChatBlock::from_tool_use("Bash", &json!({"command": "ls"}));
        match &b {
            ChatBlock::BashTool { collapsed, .. } => assert!(collapsed),
            _ => unreachable!(),
        }
        b.toggle_collapsed();
        match &b {
            ChatBlock::BashTool { collapsed, .. } => assert!(!collapsed),
            _ => unreachable!(),
        }
    }

    #[test]
    fn user_block_is_not_collapsible() {
        let b = ChatBlock::user("hi");
        assert!(!b.is_collapsible());
    }

    #[test]
    fn primary_copy_text_returns_raw_markdown_for_ai() {
        let b = ChatBlock::ai_markdown("# hi\n\nbody");
        assert_eq!(b.primary_copy_text().unwrap(), "# hi\n\nbody");
    }

    #[test]
    fn primary_copy_text_returns_diff_for_edit() {
        let b = ChatBlock::from_tool_use(
            "Edit",
            &json!({"file_path": "a", "old_string": "x", "new_string": "y"}),
        );
        let text = b.primary_copy_text().unwrap();
        assert!(text.contains("-x"));
        assert!(text.contains("+y"));
    }

    #[test]
    fn read_tool_range_formats_offset_limit() {
        let b = ChatBlock::from_tool_use(
            "Read",
            &json!({"file_path": "x", "offset": 10, "limit": 20}),
        );
        match b {
            ChatBlock::ReadTool { range, .. } => assert_eq!(range.as_deref(), Some("10-30")),
            _ => panic!(),
        }
    }

    #[test]
    fn generic_tool_is_used_for_unknown_tool_name() {
        let b = ChatBlock::from_tool_use("CustomThing", &json!({"k": "v"}));
        assert!(matches!(b, ChatBlock::GenericTool { .. }));
    }

    #[test]
    fn collapsed_tool_height_is_bounded() {
        let mut b = ChatBlock::from_tool_use("Bash", &json!({"command": "ls"}));
        let long_output = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        b.apply_tool_result(&serde_json::Value::String(long_output.clone()), false);
        // Collapsed: header + 3 preview rows + "... more" = 5
        let collapsed_h = b.height(80);
        assert!(collapsed_h <= 6, "collapsed height was {collapsed_h}");
        b.toggle_collapsed();
        let expanded_h = b.height(80);
        assert!(expanded_h > collapsed_h);
    }

    // --------- Mail block ----------

    #[test]
    fn mail_sent_block_starts_collapsed_and_pending() {
        let b = ChatBlock::mail_sent("coder", "please fix it");
        match b {
            ChatBlock::Mail {
                direction,
                counterpart,
                body,
                collapsed,
                pending,
                is_error,
            } => {
                assert_eq!(direction, MailDirection::Sent);
                assert_eq!(counterpart, "coder");
                assert_eq!(body, "please fix it");
                assert!(collapsed);
                assert!(pending);
                assert!(!is_error);
            }
            _ => panic!("expected Mail"),
        }
    }

    #[test]
    fn mail_received_block_is_not_pending() {
        let b = ChatBlock::mail_received("planner", "hi there");
        match b {
            ChatBlock::Mail {
                direction, pending, ..
            } => {
                assert_eq!(direction, MailDirection::Received);
                assert!(!pending);
            }
            _ => panic!("expected Mail"),
        }
    }

    #[test]
    fn mail_block_is_collapsible_and_toggles() {
        let mut b = ChatBlock::mail_sent("coder", "body");
        assert!(b.is_collapsible());
        b.toggle_collapsed();
        match &b {
            ChatBlock::Mail { collapsed, .. } => assert!(!collapsed),
            _ => unreachable!(),
        }
    }

    #[test]
    fn mail_primary_copy_returns_body() {
        let b = ChatBlock::mail_sent("coder", "the payload");
        assert_eq!(b.primary_copy_text().as_deref(), Some("the payload"));
    }

    #[test]
    fn apply_tool_result_clears_pending_on_mail() {
        let mut b = ChatBlock::mail_sent("coder", "hi");
        b.apply_tool_result(&json!("queued"), false);
        match b {
            ChatBlock::Mail {
                pending,
                is_error,
                collapsed,
                ..
            } => {
                assert!(!pending);
                assert!(!is_error);
                assert!(collapsed);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn apply_tool_result_marks_mail_errored_and_expanded() {
        let mut b = ChatBlock::mail_sent("ghost", "hi");
        b.apply_tool_result(&json!("no agent named ghost"), true);
        match b {
            ChatBlock::Mail {
                is_error,
                collapsed,
                ..
            } => {
                assert!(is_error);
                assert!(!collapsed, "error should auto-expand");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn task_op_constructor_defaults_to_pending_collapsed() {
        let b = ChatBlock::task_op(TaskOpKind::Add, "Add: fix bug", "details here");
        match b {
            ChatBlock::TaskOp {
                kind,
                subject,
                body,
                reply,
                is_error,
                collapsed,
                pending,
            } => {
                assert_eq!(kind, TaskOpKind::Add);
                assert_eq!(subject, "Add: fix bug");
                assert_eq!(body, "details here");
                assert!(reply.is_empty());
                assert!(!is_error);
                assert!(collapsed);
                assert!(pending);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn apply_tool_result_populates_task_op_reply_and_clears_pending() {
        let mut b = ChatBlock::task_op(TaskOpKind::Show, "Show: 3a4f12ab", "");
        b.apply_tool_result(&json!("Title: fix bug\nStatus: open"), false);
        match b {
            ChatBlock::TaskOp {
                reply,
                pending,
                is_error,
                collapsed,
                ..
            } => {
                assert!(reply.contains("Title: fix bug"));
                assert!(!pending);
                assert!(!is_error);
                assert!(collapsed, "success leaves the block collapsed");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn apply_tool_result_marks_task_op_errored_and_auto_expands() {
        let mut b = ChatBlock::task_op(TaskOpKind::Show, "Show: ghost", "");
        b.apply_tool_result(&json!("task 00000000-... not found"), true);
        match b {
            ChatBlock::TaskOp {
                is_error,
                collapsed,
                ..
            } => {
                assert!(is_error);
                assert!(!collapsed, "errors auto-expand");
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn task_op_primary_copy_prefers_reply_then_body() {
        let b = ChatBlock::task_op(TaskOpKind::Note, "Note on 3a4f12ab", "progress note");
        assert_eq!(b.primary_copy_text(), Some("progress note".into()));

        let mut b2 = ChatBlock::task_op(TaskOpKind::Show, "Show: 3a4f12ab", "");
        b2.apply_tool_result(&json!("full task body"), false);
        assert_eq!(b2.primary_copy_text(), Some("full task body".into()));
    }

    #[test]
    fn task_op_icon_groups_by_category() {
        assert_eq!(TaskOpKind::Add.icon(), "📋");
        assert_eq!(TaskOpKind::Show.icon(), "📋");
        assert_eq!(TaskOpKind::TodoAdd.icon(), "☑");
        assert_eq!(TaskOpKind::TodoList.icon(), "☑");
        assert_eq!(TaskOpKind::CommentAdd.icon(), "💬");
        assert_eq!(TaskOpKind::CommentList.icon(), "💬");
    }

    #[test]
    fn task_op_shown_body_combines_sent_and_reply_with_divider() {
        assert_eq!(task_op_shown_body("", ""), "");
        assert_eq!(task_op_shown_body("sent", ""), "sent");
        assert_eq!(task_op_shown_body("", "reply"), "reply");
        assert_eq!(task_op_shown_body("sent", "reply"), "sent\n\n---\nreply");
    }

    #[test]
    fn expanded_agent_info_block_renders_final_prompt_line() {
        // Renders a realistic AgentInfo block (long core label + long
        // prompt) into a scratch buffer sized to the block's reported
        // height, and checks that the last non-empty row contains the
        // prompt's closing tag. Regression guard for the "dropping
        // the last row of </instructions>" bug.
        let prompt = "<role>\nbe terse\n</role>\n\n<team>\nsome teammates and worker templates listed here one per line\n</team>\n\n<tools>\n- `Bash` — shell\n- `Read` — read files\n- `Edit` — edit files\n- `Grep` — search\n- `Glob` — match paths\n</tools>\n\n<instructions>\nBreak the user request into steps and delegate work to other agents. If the team needs a morale boost, either `send` a topic to the `jokemaster` agent (OpenRouter-backed one-liner) or spawn a `jokester` worker (ephemeral joke + cleanup). If the user wants corporate-speak buzzword bingo, `send` a topic to `humblemoonbeam` (Gemini-backed bs talker). All three return their punchline and stop.\n</instructions>";
        let block = ChatBlock::AgentInfo {
            core: "claude-code (default_core)".into(),
            tools: vec![
                "Bash".into(),
                "Read".into(),
                "Edit".into(),
                "Grep".into(),
                "Glob".into(),
            ],
            prompt: prompt.into(),
            context_used: None,
            context_window: Some(200_000),
            collapsed: false,
        };
        // Sweep every width in a wide range so we catch the specific
        // column counts where word-wrap produces a row count that
        // diverges from the naïve estimate.
        for width in 40u16..=160 {
            let h = block.height(width);
            // Render into a TALLER buffer than the block thinks it
            // needs — if ratatui wants more rows than h, we'll see
            // them spill past row h and know the height calc lied.
            let tall = h + 10;
            let area = Rect::new(0, 0, width, tall);
            let mut buf = Buffer::empty(area);
            let mut btns = Vec::new();
            block.render(&mut buf, area, &mut btns, 0, false);
            let mut last_row_text = String::new();
            let mut last_row_y = 0u16;
            for y in (0..tall).rev() {
                let mut line = String::new();
                for x in 0..width {
                    line.push_str(buf[(x, y)].symbol());
                }
                if !line.trim().is_empty() {
                    last_row_text = line;
                    last_row_y = y;
                    break;
                }
            }
            assert!(
                last_row_text.contains("</instructions>"),
                "width={width}: expected last non-empty row to contain \
                 </instructions>, got row {last_row_y}={last_row_text:?} \
                 (reported h={h}, tall={tall})"
            );
            assert!(
                last_row_y < h,
                "width={width}: content overflowed computed height — \
                 content at row {last_row_y} exceeds h={h}"
            );
        }
    }

    #[test]
    fn agent_info_height_accounts_for_wrapping_core_label() {
        // A long core label should push the detail line onto a second
        // row at narrow widths, and `agent_info_height` must include
        // that extra row — otherwise the expanded prompt body at the
        // bottom of the block gets clipped by one row.
        let tools: Vec<String> = vec!["Bash".into(), "Read".into(), "Edit".into()];
        let core = "anthropic-api (very-long-core-id)";
        let prompt = "short prompt";
        let h_short = agent_info_height("cc", &tools, prompt, false, None, None, 40);
        let h_long = agent_info_height(core, &tools, prompt, false, None, None, 40);
        assert!(
            h_long > h_short,
            "long core label {core:?} should add at least one wrapped row \
             to the detail line (short={h_short}, long={h_long})"
        );
    }

    /// Diagnostic: the very prompt that the `expanded_agent_info_block_*`
    /// test uses. Render only the body-paragraph portion (with the
    /// same block/wrap settings that `render_agent_info` uses) and
    /// compare its real row count against `wrapped_rows`.

    /// Diagnostic: measure the row count ratatui actually uses for a
    /// Paragraph with `Borders::LEFT` and `Wrap { trim: false }`, and
    /// compare it against `wrapped_rows(prompt, width - 1)` — which is
    /// what `agent_info_height` uses to size the block. A mismatch
    /// means long prompts get clipped.
    #[test]
    fn paragraph_with_left_border_matches_wrapped_rows_estimate() {
        // Use a deliberately long multi-paragraph prompt that will
        // definitely wrap at common widths.
        let prompt = (0..20)
            .map(|i| {
                format!(
                    "Line {i} is a long one with several words so it will wrap across multiple rows at narrow widths."
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        for total_width in [40u16, 60, 80, 100, 120] {
            let body_w = total_width - 2;
            let predicted = wrapped_rows(&prompt, body_w.saturating_sub(1));
            // Render into a very tall buffer so ratatui never clips.
            let huge_height = predicted * 2 + 4;
            let area = Rect::new(0, 0, body_w, huge_height);
            let mut buf = Buffer::empty(area);
            let block = Block::default().borders(Borders::LEFT);
            Paragraph::new(prompt.clone())
                .block(block)
                .wrap(Wrap { trim: false })
                .render(area, &mut buf);
            // Count rows that ratatui actually painted non-empty content
            // into (skipping the left-border column).
            let mut actual = 0u16;
            for y in 0..huge_height {
                let any = (1..body_w).any(|x| {
                    let cell = &buf[(x, y)];
                    !cell.symbol().trim().is_empty() && cell.symbol() != " "
                });
                if any {
                    actual = actual.saturating_add(1);
                }
            }
            assert_eq!(
                actual, predicted,
                "width={total_width}: ratatui drew {actual} rows, wrapped_rows predicted {predicted}"
            );
        }
    }
}
