//! Chat block data model + rendering.
//!
//! Each message in the chat history is a [`ChatBlock`]. A block knows how tall
//! it is at a given width ([`ChatBlock::height`]) and how to paint itself into
//! a ratatui sub-rect ([`ChatBlock::render`]). During render, interactive
//! buttons (expand/collapse, copy) push their hit rects into the shared
//! [`ChatButton`] list the app consults on mouse events.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use super::markdown::{self, MdChunk, ParsedMd};

/// Which chat message a scroll viewport is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScrollAnchor {
    pub block_idx: usize,
    pub within: u16,
}

/// Kind of system-info block — controls colour only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemKind {
    Info,
    Warn,
    Error,
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
        agent_id: String,
        role: String,
        model: String,
        core: String,
        tools: Vec<String>,
        prompt: String,
        collapsed: bool,
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
            | Self::GenericTool { collapsed, .. } => *collapsed = !*collapsed,
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
        }
    }

    /// Compute the total number of rows this block consumes at `width`.
    pub fn height(&self, width: u16) -> u16 {
        let w = width.max(1);
        match self {
            Self::AgentInfo {
                tools,
                prompt,
                collapsed,
                ..
            } => agent_info_height(tools, prompt, *collapsed, w),
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
        }
    }

    /// Paint this block into `rect`. Registers any buttons into `buttons`.
    pub fn render(
        &self,
        frame: &mut Frame,
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
            frame.render_widget(para, ind_rect);
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
                agent_id,
                role,
                model,
                core,
                tools,
                prompt,
                collapsed,
            } => render_agent_info(
                frame, body_rect, buttons, block_idx, agent_id, role, model, core, tools, prompt,
                *collapsed,
            ),
            Self::UserText { text } => self.render_prefix_text(
                frame,
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
                frame.render_widget(
                    Paragraph::new(Line::from(vec![Span::styled(
                        "🤖 ",
                        Style::default().fg(Color::Cyan),
                    )])),
                    header,
                );
                let content = Rect::new(
                    body_rect.x,
                    body_rect.y + 1,
                    body_rect.width,
                    body_rect.height - 1,
                );
                render_parsed(parsed, frame, content, buttons, block_idx);
            }
            Self::BashTool {
                command,
                stdout,
                is_error,
                collapsed,
                pending,
                ..
            } => render_tool_block(
                frame,
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
                frame,
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
                frame,
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
                render_tool_header(frame, body_rect, "📄", &subj, *is_error, *pending);
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
                    frame,
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
                    frame,
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
                    frame,
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
                self.render_prefix_text(frame, body_rect, "⚙ ", text, style);
            }
        }
    }

    fn render_prefix_text(
        &self,
        frame: &mut Frame,
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
        frame.render_widget(para, rect);
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

fn agent_info_height(tools: &[String], prompt: &str, collapsed: bool, width: u16) -> u16 {
    // Rows: header(1) + detail(1..) + prompt-header(1) + optional prompt body
    let detail_text = agent_info_detail_line(tools, "");
    let detail_rows = wrapped_rows(&detail_text, width.saturating_sub(2)).max(1);
    let prompt_body = if collapsed {
        0
    } else {
        wrapped_rows(prompt, width.saturating_sub(2))
    };
    1 + detail_rows + 1 + prompt_body
}

#[allow(clippy::too_many_arguments)]
fn render_agent_info(
    frame: &mut Frame,
    rect: Rect,
    buttons: &mut Vec<ChatButton>,
    block_idx: usize,
    agent_id: &str,
    role: &str,
    model: &str,
    core: &str,
    tools: &[String],
    prompt: &str,
    collapsed: bool,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    let mut y = rect.y;
    let end_y = rect.y.saturating_add(rect.height);

    // Header row.
    let header = Line::from(vec![
        Span::styled(
            "ℹ ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            agent_id.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" · "),
        Span::styled(role.to_string(), Style::default().fg(Color::Cyan)),
        Span::raw(" · "),
        Span::styled(model.to_string(), Style::default().fg(Color::Magenta)),
    ]);
    frame.render_widget(Paragraph::new(header), Rect::new(rect.x, y, rect.width, 1));
    y = y.saturating_add(1);
    if y >= end_y {
        return;
    }

    // Details row (core + tools), wrapped.
    let detail = agent_info_detail_line(tools, core);
    let detail_w = rect.width.saturating_sub(2);
    let detail_rows = wrapped_rows(&detail, detail_w).max(1);
    let available = end_y.saturating_sub(y);
    let draw_h = detail_rows.min(available);
    let detail_rect = Rect::new(rect.x + 2, y, detail_w, draw_h);
    frame.render_widget(
        Paragraph::new(Span::styled(detail, Style::default().fg(Color::DarkGray)))
            .wrap(Wrap { trim: false }),
        detail_rect,
    );
    y = y.saturating_add(draw_h);
    if y >= end_y {
        return;
    }

    // Prompt toggle row.
    let toggle_label = if collapsed { "[ + ]" } else { "[ - ]" };
    let tw = toggle_label.chars().count() as u16;
    let prompt_row = Rect::new(rect.x, y, rect.width, 1);
    frame.render_widget(
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
        ])),
        prompt_row,
    );
    if rect.width > tw + 1 {
        let btn_rect = Rect::new(rect.x + rect.width - tw, y, tw, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                toggle_label.to_string(),
                Style::default().fg(Color::Cyan),
            ))),
            btn_rect,
        );
        buttons.push(ChatButton {
            rect: btn_rect,
            action: BlockAction::ToggleCollapse(block_idx),
        });
    }
    y = y.saturating_add(1);
    if y >= end_y || collapsed {
        return;
    }

    // Prompt body.
    let body_w = rect.width.saturating_sub(2);
    let body_rows = wrapped_rows(prompt, body_w);
    let available = end_y.saturating_sub(y);
    let draw_h = body_rows.min(available);
    let body_rect = Rect::new(rect.x + 2, y, body_w, draw_h);
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        Paragraph::new(Span::styled(
            prompt.to_string(),
            Style::default().fg(Color::Gray),
        ))
        .block(block)
        .wrap(Wrap { trim: false }),
        body_rect,
    );
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

fn wrapped_rows(text: &str, width: u16) -> u16 {
    if width == 0 {
        return 1;
    }
    let w = width as usize;
    let mut rows = 0u16;
    for line in text.split('\n') {
        let chars = line.chars().count();
        let r = if chars == 0 {
            1
        } else {
            chars.div_ceil(w) as u16
        };
        rows = rows.saturating_add(r);
    }
    rows.max(1)
}

// ---------------------------------------------------------------------------
// Tool block renderer
// ---------------------------------------------------------------------------

/// Render a one-line tool header with no body or buttons.
fn render_tool_header(
    frame: &mut Frame,
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
    frame.render_widget(Paragraph::new(header_line), header_rect);
}

#[allow(clippy::too_many_arguments)]
fn render_tool_block(
    frame: &mut Frame,
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
    frame.render_widget(Paragraph::new(header_line), header_rect);

    // Buttons live on the rightmost end of the header row.
    let mut bx = rect.x + rect.width.saturating_sub(buttons_width);
    for (label, action) in &button_specs {
        let w = label.chars().count() as u16;
        let brect = Rect::new(bx, rect.y, w, 1);
        let style = match action {
            BlockAction::ToggleCollapse(_) => Style::default().fg(Color::Cyan),
            BlockAction::CopyText(_) => Style::default().fg(Color::LightBlue),
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(label.clone(), style))),
            brect,
        );
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
        frame.render_widget(
            Paragraph::new(Span::styled(
                hint,
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )),
            body_rect,
        );
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
    frame.render_widget(para, body_rect);
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
    frame: &mut Frame,
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
                frame.render_widget(para, rect_here);
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
                render_fence(frame, fence_rect, buttons, block_idx, lang, body);
                y = y.saturating_add(h);
            }
        }
    }
}

fn render_fence(
    frame: &mut Frame,
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
    frame.render_widget(block, rect);

    // Copy button in the top-right of the header border.
    let label = "[ copy ]";
    let w = label.chars().count() as u16;
    if rect.width > w + 2 {
        let btn_rect = Rect::new(rect.x + rect.width - w - 1, rect.y, w, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                label.to_string(),
                Style::default().fg(Color::LightBlue),
            ))),
            btn_rect,
        );
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
    frame.render_widget(para, inner);
}

// ---------------------------------------------------------------------------
// Constructors from AI events
// ---------------------------------------------------------------------------

impl ChatBlock {
    pub fn agent_info(
        agent_id: impl Into<String>,
        role: impl Into<String>,
        model: impl Into<String>,
        core: impl Into<String>,
        tools: Vec<String>,
        prompt: impl Into<String>,
    ) -> Self {
        Self::AgentInfo {
            agent_id: agent_id.into(),
            role: role.into(),
            model: model.into(),
            core: core.into(),
            tools,
            prompt: prompt.into(),
            collapsed: true,
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
}
