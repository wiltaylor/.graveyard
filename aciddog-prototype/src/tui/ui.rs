use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use super::app::{AgentChat, AgentStatus, App, Focus};
use super::blocks::ScrollAnchor;

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

    let chat_area = body_cols[0];
    let agents_area = body_cols[1];

    render_title(f, title_area, app);
    let chat_inner = render_agent_chat(f, chat_area, app);
    let list_inner = render_agent_list(f, agents_area, app);
    let input_inner = render_chat_input(f, chat_input_area, app);
    render_status(f, status_area, app);

    app.layout.agent_list_inner = Some(list_inner);
    app.layout.chat_history_inner = Some(chat_inner);
    app.layout.chat_input_inner = Some(input_inner);
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

    let items: Vec<ListItem> = app
        .config
        .agents
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let status = app
                .chats
                .get(i)
                .map(|c| c.status)
                .unwrap_or(AgentStatus::Ready);
            let dot = match status {
                AgentStatus::Ready => "🟢",
                AgentStatus::Working => "🟡",
                AgentStatus::Error => "🔴",
            };
            let name_style = if i == app.selected {
                Style::default()
                    .bg(Color::Cyan)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(vec![
                Span::raw(format!("{dot} ")),
                Span::styled(a.id.clone(), name_style),
            ]))
        })
        .collect();

    let list = List::new(items).block(block);

    let mut state = ListState::default();
    if !app.config.agents.is_empty() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_agent_chat(f: &mut Frame, area: Rect, app: &mut App) -> Rect {
    let focused = app.focus == Focus::Chat;
    let selected_agent = app.selected;

    let title = match app.config.agents.get(selected_agent) {
        Some(a) => format!(" {} · {} · {} ", a.id, a.role, a.model),
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

fn render_status(f: &mut Frame, area: Rect, app: &App) {
    let focus_label = match app.focus {
        Focus::Agents => "agents",
        Focus::Chat => "chat",
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
            Span::raw(" chat  "),
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
