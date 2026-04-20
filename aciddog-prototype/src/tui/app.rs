use std::io;

use anyhow::Result;
use crossterm::event::{
    DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyCode, KeyEventKind,
    KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use futures::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tracing::{debug, info};

use crate::config::AciddogConfig;

use super::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Agents,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    System,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LayoutRects {
    pub agent_list_inner: Option<Rect>,
    pub chat_history_inner: Option<Rect>,
    pub chat_input_inner: Option<Rect>,
}

pub struct App {
    pub config: AciddogConfig,
    pub selected: usize,
    pub should_quit: bool,
    pub focus: Focus,
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub chat_scroll: u16,
    pub layout: LayoutRects,
}

impl App {
    pub fn new(config: AciddogConfig) -> Self {
        let mut app = Self {
            config,
            selected: 0,
            should_quit: false,
            focus: Focus::Agents,
            messages: Vec::new(),
            input: String::new(),
            chat_scroll: 0,
            layout: LayoutRects::default(),
        };
        app.messages.push(ChatMessage {
            role: MessageRole::System,
            content: "Chat is a UI stub — messages aren't sent anywhere yet. \
                      Tab toggles focus, mouse wheel scrolls history. \
                      Slash commands: /version, /exit."
                .to_string(),
        });
        app
    }

    fn select_next(&mut self) {
        if self.config.agents.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.config.agents.len();
    }

    fn select_prev(&mut self) {
        if self.config.agents.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.config.agents.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn submit_input(&mut self) {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            return;
        }
        let content = trimmed.to_string();
        self.messages.push(ChatMessage {
            role: MessageRole::User,
            content: content.clone(),
        });
        self.input.clear();
        self.chat_scroll = u16::MAX;

        if content.starts_with('/') {
            if let Some(outcome) = super::commands::dispatch(&content) {
                self.apply_command(outcome);
            }
        }
    }

    fn apply_command(&mut self, outcome: super::commands::CommandOutcome) {
        use super::commands::CommandOutcome;
        match outcome {
            CommandOutcome::Message(text) => {
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: text,
                });
            }
            CommandOutcome::Quit => {
                info!("quit requested via /exit");
                self.should_quit = true;
            }
            CommandOutcome::Unknown { name, hint } => {
                self.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: format!("unknown command: /{name} — {hint}"),
                });
            }
        }
        self.chat_scroll = u16::MAX;
    }

    pub fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        // Ctrl-C always quits, regardless of focus.
        if matches!(code, KeyCode::Char('c')) && mods.contains(KeyModifiers::CONTROL) {
            info!("ctrl-c received");
            self.should_quit = true;
            return;
        }

        if matches!(code, KeyCode::Tab) {
            self.focus = match self.focus {
                Focus::Agents => Focus::Chat,
                Focus::Chat => Focus::Agents,
            };
            return;
        }

        match self.focus {
            Focus::Agents => self.handle_key_agents(code, mods),
            Focus::Chat => self.handle_key_chat(code, mods),
        }
    }

    fn handle_key_agents(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                info!("quit requested");
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => self.select_next(),
            KeyCode::Char('k') | KeyCode::Up => self.select_prev(),
            _ => {}
        }
    }

    fn handle_key_chat(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Esc => self.focus = Focus::Agents,
            KeyCode::Enter => self.submit_input(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, ev: MouseEvent) {
        let pos = (ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(rect) = self.layout.agent_list_inner {
                    if rect_contains(rect, pos) {
                        self.focus = Focus::Agents;
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        if row_in_list < self.config.agents.len() {
                            self.selected = row_in_list;
                        }
                        return;
                    }
                }
                if let Some(rect) = self.layout.chat_history_inner {
                    if rect_contains(rect, pos) {
                        self.focus = Focus::Chat;
                        return;
                    }
                }
                if let Some(rect) = self.layout.chat_input_inner {
                    if rect_contains(rect, pos) {
                        self.focus = Focus::Chat;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if self.mouse_in_chat_scroll_zone(pos) {
                    self.chat_scroll = self.chat_scroll.saturating_sub(1);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.mouse_in_chat_scroll_zone(pos) {
                    self.chat_scroll = self.chat_scroll.saturating_add(1);
                }
            }
            _ => {}
        }
    }

    fn mouse_in_chat_scroll_zone(&self, pos: (u16, u16)) -> bool {
        self.layout
            .chat_history_inner
            .map(|r| rect_contains(r, pos))
            .unwrap_or(false)
    }
}

fn rect_contains(rect: Rect, (x, y): (u16, u16)) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

#[tracing::instrument(skip(config))]
pub async fn run(config: AciddogConfig) -> Result<()> {
    info!(team = %config.team.id, agents = config.agents.len(), "starting TUI");

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = App::new(config);

    let result = event_loop(&mut terminal, &mut app).await;

    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    disable_raw_mode()?;
    terminal.show_cursor()?;

    info!("TUI exited");
    result
}

async fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()> {
    let mut events = EventStream::new();
    terminal.draw(|f| ui::render(f, app))?;

    while let Some(event) = events.next().await {
        match event? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                debug!(?key, "key press");
                app.handle_key(key.code, key.modifiers);
            }
            Event::Mouse(mouse) => {
                debug!(?mouse, "mouse");
                app.handle_mouse(mouse);
            }
            Event::Resize(_, _) => {
                debug!("resize");
            }
            _ => {}
        }

        if app.should_quit {
            break;
        }

        terminal.draw(|f| ui::render(f, app))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AciddogConfig, Agent, Team};

    fn sample_config() -> AciddogConfig {
        AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
            },
            agents: vec![
                Agent {
                    id: "a".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                },
                Agent {
                    id: "b".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                },
                Agent {
                    id: "c".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                },
            ],
        }
    }

    #[test]
    fn tab_toggles_focus() {
        let mut app = App::new(sample_config());
        assert_eq!(app.focus, Focus::Agents);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Chat);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Agents);
    }

    #[test]
    fn q_quits_when_agents_focused_but_types_when_chat_focused() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert_eq!(app.input, "q");

        app.focus = Focus::Agents;
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn enter_submits_chat_message_and_clears_input() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "hello".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(app.input, "hello");
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.input, "");
        assert!(app
            .messages
            .iter()
            .any(|m| m.role == MessageRole::User && m.content == "hello"));
    }

    #[test]
    fn slash_version_emits_system_message() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "/version".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        let last = app.messages.last().unwrap();
        assert_eq!(last.role, MessageRole::System);
        assert!(last.content.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn slash_exit_sets_should_quit() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "/exit".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.should_quit);
    }

    #[test]
    fn unknown_slash_command_emits_hint() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "/frobnicate".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        let last = app.messages.last().unwrap();
        assert_eq!(last.role, MessageRole::System);
        assert!(last.content.contains("unknown command"));
        assert!(last.content.contains("frobnicate"));
    }

    #[test]
    fn non_slash_input_is_not_dispatched() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "hello world".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.should_quit);
        // Only the user message should have been added (plus the startup system message).
        let user_msgs: Vec<_> = app
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .collect();
        assert_eq!(user_msgs.len(), 1);
        assert_eq!(user_msgs[0].content, "hello world");
    }

    #[test]
    fn empty_input_does_not_create_message() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        let initial_count = app.messages.len();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.messages.len(), initial_count);
    }

    #[test]
    fn click_in_agent_list_selects_agent() {
        let mut app = App::new(sample_config());
        app.layout.agent_list_inner = Some(Rect::new(10, 5, 20, 10));

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 15,
            row: 7, // 7 - 5 = index 2
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.selected, 2);
        assert_eq!(app.focus, Focus::Agents);
    }

    #[test]
    fn click_outside_list_does_not_select() {
        let mut app = App::new(sample_config());
        app.selected = 1;
        app.layout.agent_list_inner = Some(Rect::new(10, 5, 20, 10));

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.selected, 1);
    }

    #[test]
    fn click_in_chat_history_focuses_chat() {
        let mut app = App::new(sample_config());
        app.layout.chat_history_inner = Some(Rect::new(0, 20, 80, 10));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.focus, Focus::Chat);
    }

    #[test]
    fn scroll_wheel_in_chat_history_changes_scroll() {
        let mut app = App::new(sample_config());
        app.layout.chat_history_inner = Some(Rect::new(0, 20, 80, 10));
        assert_eq!(app.chat_scroll, 0);

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.chat_scroll, 1);

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.chat_scroll, 0);

        // Saturating below zero.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.chat_scroll, 0);
    }

    #[test]
    fn scroll_wheel_outside_chat_is_ignored() {
        let mut app = App::new(sample_config());
        app.layout.chat_history_inner = Some(Rect::new(0, 20, 80, 10));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.chat_scroll, 0);
    }
}
