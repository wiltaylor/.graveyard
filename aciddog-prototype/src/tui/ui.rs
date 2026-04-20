use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{
    format_task_detail, ActiveView, AgentChat, AgentStatus, App, Focus, VIEW_ENTRIES,
};
use super::blocks::ScrollAnchor;
use super::tasks_view::{FormField, TaskForm, TaskFormKind, TaskRow, TasksPrompt, TasksScreen};

const APP_NAME: &str = "aciddog";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn render(f: &mut Frame, app: &mut App) {
    // Fresh frame: reset interactive regions.
    app.layout.chat_buttons.clear();

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Min(10),   // chat + agents
            Constraint::Length(3), // input
            Constraint::Length(1), // status
        ])
        .split(f.area());

    let title_area = root[0];
    let body_area = root[1];
    let chat_input_area = root[2];
    let status_area = root[3];

    let body_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(75), Constraint::Percentage(25)])
        .split(body_area);

    let main_area = body_cols[0];
    let right_col = body_cols[1];

    // Right column: Views list on top (small), agents list below.
    // Views height = border (2) + one line per entry. Extending
    // VIEW_ENTRIES just grows this proportionally.
    let views_height = (VIEW_ENTRIES.len() as u16).saturating_add(2).max(3);
    let right_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(views_height), Constraint::Min(5)])
        .split(right_col);
    let views_area = right_rows[0];
    let agents_area = right_rows[1];

    render_title(f, title_area, app);

    let main_inner = match app.active_view {
        ActiveView::Chat => render_agent_chat(f, main_area, app),
        ActiveView::Tasks => render_tasks_screen(f, main_area, app),
    };
    let views_inner = render_views_list(f, views_area, app);
    let list_inner = render_agent_list(f, agents_area, app);
    let input_inner = render_chat_input(f, chat_input_area, app);
    render_status(f, status_area, app);

    // Modal overlays (only meaningful in Tasks view).
    if app.active_view == ActiveView::Tasks {
        if app.tasks_screen.form.is_some() {
            render_task_form(f, main_area, app);
        } else if let Some(id) = app.tasks_screen.detail {
            render_task_detail(f, main_area, app, id);
        }
    }

    app.layout.agent_list_inner = Some(list_inner);
    app.layout.chat_history_inner = Some(main_inner);
    app.layout.chat_input_inner = Some(input_inner);
    app.layout.views_list_inner = Some(views_inner);
}

fn focus_border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_title(f: &mut Frame, area: Rect, app: &App) {
    let title = Line::from(vec![
        Span::styled(
            format!(" {APP_NAME} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  v{VERSION}  ")),
        Span::styled(
            format!("team: {}", app.config.team.id),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled(
            format!("({})", app.config.team.description),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ),
    ]);
    f.render_widget(Paragraph::new(title), area);
}

fn render_agent_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Agents;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" agents ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);

    let order = app.visible_order();
    let items: Vec<ListItem> = order
        .iter()
        .map(|&(i, depth)| {
            let c = &app.chats[i];
            let dot = match c.status {
                AgentStatus::Ready => "🟢",
                AgentStatus::Working => "🟡",
                AgentStatus::Error => "🔴",
                AgentStatus::Hibernated => "💤",
            };
            let name_style = if i == app.selected {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut spans: Vec<Span> = Vec::with_capacity(4);
            if depth > 0 {
                // Indent once per ancestor, with a single "↳ " on the
                // last level so the reader can see which row is the
                // child entry.
                if depth > 1 {
                    spans.push(Span::raw("  ".repeat(depth - 1)));
                }
                spans.push(Span::styled("↳ ", Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::raw(format!("{dot} ")));
            spans.push(Span::styled(c.display_name.clone(), name_style));
            if c.parent_idx.is_some() {
                spans.push(Span::styled(
                    " [worker]",
                    Style::default().fg(Color::DarkGray),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items).block(block);

    let mut state = ListState::default();
    if let Some(sel_row) = order.iter().position(|(i, _)| *i == app.selected) {
        state.select(Some(sel_row));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_agent_chat(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Chat;
    let selected_agent = app.selected;

    let title = match app.chats.get(selected_agent) {
        Some(chat) => {
            // Show role/model for configured agents; for workers,
            // fall back to "worker · <template>" since the chat
            // itself carries the display name and template, not the
            // role/model pair from an `agent` config block.
            if let Some(template) = &chat.worker_template {
                if let Some(w) = app.config.worker(template) {
                    format!(
                        " {} · worker · {} · {} ",
                        chat.display_name, w.role, w.model
                    )
                } else {
                    format!(" {} · worker ", chat.display_name)
                }
            } else if let Some(a) = app.config.agents.get(selected_agent) {
                format!(" {} · {} · {} ", a.id, a.role, a.model)
            } else {
                format!(" {} ", chat.display_name)
            }
        }
        None => " chat ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Split borrows so we can mutate both the current chat and the button list.
    let App {
        chats,
        layout,
        config,
        ..
    } = app;
    let Some(chat) = chats.get_mut(selected_agent) else {
        let para = Paragraph::new("no agents defined in aciddog.wcl")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(para, inner);
        return inner;
    };

    if chat.blocks.is_empty() {
        return inner;
    }
    let _ = config;

    // Compute heights at the current viewport width.
    let heights: Vec<u16> = chat
        .blocks
        .iter()
        .map(|b| b.height(inner.width).max(1))
        .collect();

    reconcile_anchor(chat, &heights, inner.height);

    let start_idx = chat.anchor.block_idx;
    let mut y = inner.y;
    let end_y = inner.y.saturating_add(inner.height);

    for (i, &h) in heights.iter().enumerate().skip(start_idx) {
        if y >= end_y {
            break;
        }
        let visible_h = h.min(end_y - y);
        if visible_h == 0 {
            continue;
        }
        let rect = Rect::new(inner.x, y, inner.width, visible_h);
        let selected = chat.selected_block == Some(i);
        chat.blocks[i].render(f, rect, &mut layout.chat_buttons, i, selected);
        y = y.saturating_add(visible_h);
    }
    inner
}

/// Make sure the scroll anchor points somewhere sensible given the block
/// list and viewport height. Handles sticky-bottom and clamps the anchor
/// inside valid bounds.
fn reconcile_anchor(chat: &mut AgentChat, heights: &[u16], viewport_h: u16) {
    if chat.blocks.is_empty() || viewport_h == 0 {
        chat.anchor = ScrollAnchor::default();
        return;
    }

    let last = chat.blocks.len() - 1;

    // Clamp anchor to valid block index.
    if chat.anchor.block_idx > last {
        chat.anchor.block_idx = last;
        chat.anchor.within = 0;
    }

    // Roll forward if `within` has overshot the current block's height.
    loop {
        let h = heights[chat.anchor.block_idx];
        if chat.anchor.within < h {
            break;
        }
        if chat.anchor.block_idx == last {
            chat.anchor.within = h.saturating_sub(1);
            break;
        }
        chat.anchor.within -= h;
        chat.anchor.block_idx += 1;
    }

    if chat.sticky_bottom {
        // Pick the anchor so the last block ends flush with the viewport's bottom.
        let mut remaining = viewport_h;
        let mut idx = last;
        loop {
            let h = heights[idx];
            if h >= remaining {
                chat.anchor.block_idx = idx;
                chat.anchor.within = h - remaining;
                return;
            }
            remaining -= h;
            if idx == 0 {
                chat.anchor.block_idx = 0;
                chat.anchor.within = 0;
                return;
            }
            idx -= 1;
        }
    }
}

fn render_chat_input(f: &mut Frame, area: Rect, app: &App) -> Rect {
    // In Tasks view the bottom box does double duty: it shows
    // operation hints when the screen is idle, field-nav hints when
    // a form is open, and the current input buffer when a
    // single-line prompt (note body / delete confirmation) is
    // active.
    if app.active_view == ActiveView::Tasks {
        return render_tasks_input(f, area, app);
    }

    let focused = app.focus == Focus::Chat;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" input ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);

    let input_text = app
        .current_chat()
        .map(|c| c.input.clone())
        .unwrap_or_default();

    let display: Line = if focused {
        Line::from(vec![
            Span::raw(input_text),
            Span::styled(
                "▏",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        let placeholder = if input_text.is_empty() {
            Span::styled(
                "click here or press Tab to chat",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )
        } else {
            Span::raw(input_text)
        };
        Line::from(placeholder)
    };

    let para = Paragraph::new(display).block(block);
    f.render_widget(para, area);
    inner
}

fn render_tasks_input(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Tasks;
    let (title, line) = if let Some(prompt) = &app.tasks_screen.prompt {
        match prompt {
            TasksPrompt::Note { body, .. } => (
                " note body ".to_string(),
                Line::from(vec![
                    Span::raw(body.clone()),
                    Span::styled(
                        "▏",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            ),
            TasksPrompt::ConfirmDelete { .. } => (
                " delete? ".to_string(),
                Line::from(Span::styled(
                    "press y to delete, n or esc to cancel",
                    Style::default().fg(Color::Yellow),
                )),
            ),
        }
    } else if let Some(form) = app.tasks_screen.form.as_ref() {
        let hint = if form.picker.is_some() {
            "↑/↓ nav · enter pick · esc cancel"
        } else {
            "tab/↓ next · shift-tab/↑ prev · enter submit (or open dropdown) · esc cancel"
        };
        (
            " form ".to_string(),
            Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))),
        )
    } else if let Some(msg) = &app.tasks_screen.status_msg {
        (
            " status ".to_string(),
            Line::from(Span::styled(
                msg.clone(),
                Style::default().fg(Color::Yellow),
            )),
        )
    } else {
        (
            " tasks ".to_string(),
            Line::from(Span::styled(
                "a add · e edit · d del · t take · n note · s status · ⏎ detail · esc back",
                Style::default().fg(Color::DarkGray),
            )),
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(Paragraph::new(line).block(block), area);
    inner
}

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let focus_label = match app.focus {
        Focus::Agents => "agents",
        Focus::Chat => "chat",
        Focus::Views => "views",
        Focus::Tasks => "tasks",
    };
    let mut hint_spans: Vec<Span> = match app.focus {
        Focus::Agents => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(" navigate  "),
            Span::styled("tab", Style::default().fg(Color::Cyan)),
            Span::raw(" next panel  "),
            Span::styled("q", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ],
        Focus::Chat => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("enter", Style::default().fg(Color::Cyan)),
            Span::raw(" send  "),
            Span::styled("alt+↑↓", Style::default().fg(Color::Cyan)),
            Span::raw(" select  "),
            Span::styled("alt+⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" toggle  "),
            Span::styled("alt+c", Style::default().fg(Color::Cyan)),
            Span::raw(" copy  "),
            Span::styled("ctrl-c", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ],
        Focus::Views => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("j/k", Style::default().fg(Color::Cyan)),
            Span::raw(" nav  "),
            Span::styled("⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" activate  "),
            Span::styled("tab", Style::default().fg(Color::Cyan)),
            Span::raw(" next panel"),
        ],
        Focus::Tasks => vec![
            Span::styled(" focus: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                focus_label,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("a", Style::default().fg(Color::Cyan)),
            Span::raw(" add  "),
            Span::styled("e", Style::default().fg(Color::Cyan)),
            Span::raw(" edit  "),
            Span::styled("d", Style::default().fg(Color::Cyan)),
            Span::raw(" del  "),
            Span::styled("t", Style::default().fg(Color::Cyan)),
            Span::raw(" take  "),
            Span::styled("n", Style::default().fg(Color::Cyan)),
            Span::raw(" note  "),
            Span::styled("s", Style::default().fg(Color::Cyan)),
            Span::raw(" status  "),
            Span::styled("⏎", Style::default().fg(Color::Cyan)),
            Span::raw(" detail  "),
            Span::styled("esc", Style::default().fg(Color::Cyan)),
            Span::raw(" back"),
        ],
    };

    if app.copied_recently() {
        hint_spans.push(Span::raw("   "));
        hint_spans.push(Span::styled(
            "✓ copied",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }

    f.render_widget(
        Paragraph::new(Line::from(hint_spans)).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}

fn render_views_list(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Views;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" views ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    let items: Vec<ListItem> = VIEW_ENTRIES
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let active = matches!((*name, app.active_view), ("Tasks", ActiveView::Tasks));
            let style = if i == app.selected_view && focused {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else if active {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled((*name).to_string(), style)))
        })
        .collect();
    let list = List::new(items).block(block);
    let mut state = ListState::default();
    state.select(Some(app.selected_view));
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_tasks_screen(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Tasks;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" tasks ")
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = TasksScreen::rows(&app.tasks);
    // Guard against a stale selected_row after store changes.
    app.tasks_screen.reclamp(&rows);

    if rows.iter().all(|r| matches!(r, TaskRow::Header(_))) && app.tasks.is_empty() {
        let p = Paragraph::new("No tasks yet. Press `a` to add one.")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(p, inner);
        return inner;
    }

    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| match row {
            TaskRow::Header(status) => {
                let label = format!(" ── {} ──", status.as_str());
                ListItem::new(Line::from(Span::styled(
                    label,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )))
            }
            TaskRow::Task(id) => {
                let selected = i == app.tasks_screen.selected_row && focused;
                render_task_row(app, *id, selected)
            }
        })
        .collect();

    let list = List::new(items);
    let mut state = ListState::default();
    state.select(Some(app.tasks_screen.selected_row));
    f.render_stateful_widget(list, inner, &mut state);

    inner
}

fn render_task_row(app: &App, id: uuid::Uuid, selected: bool) -> ListItem<'static> {
    let Some(t) = app.tasks.get(id) else {
        return ListItem::new(Line::from(Span::raw("(missing)")));
    };
    let short = t.id.to_string();
    let short = short.get(..8).unwrap_or(&short).to_string();
    let blocked = app.tasks.is_blocked(t);
    let deps_label = if t.blocked_by.is_empty() {
        String::new()
    } else if blocked {
        format!(" [blocked({})] ", t.blocked_by.len())
    } else {
        format!(" [ready({})] ", t.blocked_by.len())
    };
    let assignee_label = format!(" {} ", t.assignee.as_deref().unwrap_or("-"));
    let mut base_style = if blocked {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default()
    };
    if selected {
        base_style = base_style
            .bg(Color::Cyan)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD);
    }
    let line = Line::from(vec![
        Span::styled(format!("  {short}"), base_style),
        Span::styled(
            format!(" ·{}·", assignee_label),
            if selected {
                base_style
            } else {
                Style::default().fg(Color::Yellow)
            },
        ),
        Span::styled(deps_label, base_style),
        Span::styled(t.title.clone(), base_style),
    ]);
    ListItem::new(line)
}

/// Centered rect helper: size = `(w, h)` clamped to `area`.
fn centered_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    Rect::new(x, y, w, h)
}

fn render_task_form(f: &mut Frame, area: Rect, app: &App) {
    let Some(form) = app.tasks_screen.form.as_ref() else {
        return;
    };
    let title = match form.kind {
        TaskFormKind::Add => " new task ",
        TaskFormKind::Edit(_) => " edit task ",
    };
    let order = form.tab_order();
    // Allocate vertical space: 1 line per field label + 1 line for
    // the value, plus top/bottom border, plus an error/footer line.
    let h: u16 = (order.len() as u16 * 2 + 4).min(area.height.saturating_sub(2));
    let rect = centered_rect(area, 64.min(area.width), h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let mut lines: Vec<Line> = Vec::with_capacity(order.len() * 2 + 2);
    for field in order {
        let is_focused = *field == form.focused_field;
        let is_dropdown = TaskForm::is_dropdown(*field);
        let label_style = if is_focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        lines.push(Line::from(Span::styled(
            format!("{}:", field.label()),
            label_style,
        )));
        let raw_value = form.field(*field).to_string();
        let display_value = if is_dropdown {
            // Show empty assignee as a readable placeholder so the
            // unassigned option doesn't look like an unset field.
            if raw_value.is_empty() && *field == FormField::Assignee {
                "(unassigned)".to_string()
            } else if raw_value.is_empty() {
                "-".to_string()
            } else {
                raw_value.clone()
            }
        } else {
            raw_value.clone()
        };
        let value_line = if is_dropdown {
            // Dropdown: boxed value with a ▾ hint; focused = bold
            // cyan; a small "enter to pick" nudge when focused.
            let value_style = if is_focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let mut spans = vec![
                Span::raw("  [ "),
                Span::styled(display_value, value_style),
                Span::raw(" ▾ ]"),
            ];
            if is_focused {
                spans.push(Span::styled(
                    "   enter to pick",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ));
            }
            Line::from(spans)
        } else if is_focused {
            Line::from(vec![
                Span::raw(format!("  {display_value}")),
                Span::styled(
                    "▏",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        } else if display_value.is_empty() {
            Line::from(Span::styled(
                "  (empty)",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            ))
        } else {
            Line::from(Span::raw(format!("  {display_value}")))
        };
        lines.push(value_line);
    }
    if let Some(err) = &form.error {
        lines.push(Line::from(Span::styled(
            format!("error: {err}"),
            Style::default().fg(Color::Red),
        )));
    }
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, inner);

    // Picker popup overlays the form when open.
    if form.picker.is_some() {
        render_dropdown_picker(f, area, form);
    }
}

fn render_dropdown_picker(f: &mut Frame, area: Rect, form: &super::tasks_view::TaskForm) {
    let Some(picker) = form.picker.as_ref() else {
        return;
    };
    // Size: wide enough for the longest option + padding, tall
    // enough for all options + header + border (clamped to area).
    let longest = picker
        .options
        .iter()
        .map(|s| {
            if s.is_empty() && picker.field == FormField::Assignee {
                "(unassigned)".len()
            } else {
                s.len()
            }
        })
        .max()
        .unwrap_or(0);
    let w = (longest as u16 + 8).clamp(24, area.width);
    let h = (picker.options.len() as u16 + 4).clamp(6, area.height);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);

    let title = match picker.field {
        FormField::Assignee => " pick assignee ",
        FormField::Status => " pick status ",
        _ => " pick ",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let items: Vec<ListItem> = picker
        .options
        .iter()
        .map(|o| {
            let label = if o.is_empty() && picker.field == FormField::Assignee {
                "(unassigned)".to_string()
            } else if o.is_empty() {
                "-".to_string()
            } else {
                o.clone()
            };
            ListItem::new(Line::from(Span::raw(label)))
        })
        .collect();
    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::Cyan)
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );
    let mut state = ListState::default();
    state.select(Some(picker.selected));
    f.render_stateful_widget(list, inner, &mut state);
}

fn render_task_detail(f: &mut Frame, area: Rect, app: &App, id: uuid::Uuid) {
    let Some(t) = app.tasks.get(id) else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let body = format_task_detail(t, &app.tasks, now);

    let w = 80.min(area.width);
    let h = area.height.saturating_sub(2).max(10);
    let rect = centered_rect(area, w, h);
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" task detail (press any key to close) ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let lines: Vec<Line> = body
        .lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect();
    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}
