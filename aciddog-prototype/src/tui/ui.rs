use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{App, Focus, MessageRole};

const APP_NAME: &str = "aciddog";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn render(f: &mut Frame, app: &mut App) {
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
        .map(|a| ListItem::new(a.id.clone()))
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut state = ListState::default();
    if !app.config.agents.is_empty() {
        state.select(Some(app.selected));
    }
    f.render_stateful_widget(list, area, &mut state);
    inner
}

fn render_agent_chat(f: &mut Frame, area: Rect, app: &App) -> Rect {
    let focused = app.focus == Focus::Chat;
    let agent = app.config.agents.get(app.selected);

    let title = match agent {
        Some(a) => format!(" {} · {} · {} ", a.id, a.role, a.model),
        None => " chat ".to_string(),
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(focus_border_style(focused));
    let inner = block.inner(area);

    let Some(chat) = app.current_chat() else {
        let para = Paragraph::new("no agents defined in aciddog.wcl")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(para, area);
        return inner;
    };

    let mut lines: Vec<Line> = Vec::new();
    if chat.messages.is_empty() {
        if let Some(a) = agent {
            lines.push(Line::from(Span::styled(
                "instructions",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                a.instructions.clone(),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "type a message and press Enter to begin this agent's session",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    } else {
        for msg in &chat.messages {
            let (prefix, style) = match msg.role {
                MessageRole::User => (
                    "👤 ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                MessageRole::Ai => ("🤖 ", Style::default().fg(Color::Cyan)),
                MessageRole::Tool => ("🛠️  ", Style::default().fg(Color::Magenta)),
                MessageRole::System => (
                    "⚙️  ",
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::ITALIC),
                ),
            };
            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::raw(msg.content.clone()),
            ]));
            lines.push(Line::from(""));
        }
    }

    let total_lines = lines.len() as u16;
    let viewport = inner.height;
    let max_scroll = total_lines.saturating_sub(viewport);
    let effective_scroll = chat.scroll.min(max_scroll);

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((effective_scroll, 0));
    f.render_widget(para, area);
    inner
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
    let hint_spans = match app.focus {
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
            Span::styled("wheel", Style::default().fg(Color::Cyan)),
            Span::raw(" scroll  "),
            Span::styled("tab/esc", Style::default().fg(Color::Cyan)),
            Span::raw(" agents  "),
            Span::styled("ctrl-c", Style::default().fg(Color::Cyan)),
            Span::raw(" quit"),
        ],
    };
    f.render_widget(
        Paragraph::new(Line::from(hint_spans)).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}
