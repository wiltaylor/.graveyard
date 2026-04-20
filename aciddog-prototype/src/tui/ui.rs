use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use super::app::App;

const APP_NAME: &str = "aciddog";
const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn render(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_title(f, chunks[0], app);
    render_main(f, chunks[1], app);
    render_status(f, chunks[2]);
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

fn render_main(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    render_agent_list(f, cols[0], app);
    render_agent_details(f, cols[1], app);
}

fn render_agent_list(f: &mut Frame, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .config
        .agents
        .iter()
        .map(|a| ListItem::new(a.id.clone()))
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" agents "))
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
}

fn render_agent_details(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" details ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(agent) = app.config.agents.get(app.selected) else {
        let placeholder = Paragraph::new("no agents defined in aciddog.wcl")
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(placeholder, inner);
        return;
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("name: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                agent.id.clone(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("role: ", Style::default().fg(Color::DarkGray)),
            Span::raw(agent.role.clone()),
        ]),
        Line::from(vec![
            Span::styled("model: ", Style::default().fg(Color::DarkGray)),
            Span::raw(agent.model.clone()),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "instructions:",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(agent.instructions.clone()),
    ];

    let para = Paragraph::new(lines).wrap(Wrap { trim: false });
    f.render_widget(para, inner);
}

fn render_status(f: &mut Frame, area: Rect) {
    let hint = Line::from(vec![
        Span::styled(" j/k ", Style::default().fg(Color::Cyan)),
        Span::raw("navigate  "),
        Span::styled(" q ", Style::default().fg(Color::Cyan)),
        Span::raw("quit"),
    ]);
    f.render_widget(
        Paragraph::new(hint).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}
