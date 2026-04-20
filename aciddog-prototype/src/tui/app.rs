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
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::ai::{AICore, AIEvent, ClaudeCodeCore};
use crate::config::AciddogConfig;

use super::ui;

const DEFAULT_ALLOWED_TOOLS: &[&str] = &["Bash", "Read", "Edit", "Grep", "Glob"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Agents,
    Chat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Ai,
    Tool,
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

#[derive(Debug, Default)]
pub struct AgentChat {
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub scroll: u16,
    pub user_tx: Option<mpsc::Sender<String>>,
}

pub struct App {
    pub config: AciddogConfig,
    pub selected: usize,
    pub should_quit: bool,
    pub focus: Focus,
    pub chats: Vec<AgentChat>,
    pub layout: LayoutRects,
}

impl App {
    pub fn new(config: AciddogConfig) -> Self {
        let chats = (0..config.agents.len())
            .map(|_| AgentChat::default())
            .collect();
        Self {
            config,
            selected: 0,
            should_quit: false,
            focus: Focus::Chat,
            chats,
            layout: LayoutRects::default(),
        }
    }

    pub fn current_chat(&self) -> Option<&AgentChat> {
        self.chats.get(self.selected)
    }

    pub fn current_chat_mut(&mut self) -> Option<&mut AgentChat> {
        self.chats.get_mut(self.selected)
    }

    fn select_next(&mut self) {
        if self.config.agents.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.config.agents.len();
        self.focus = Focus::Chat;
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
        self.focus = Focus::Chat;
    }

    fn submit_input(&mut self) {
        let selected = self.selected;
        let content = {
            let Some(chat) = self.chats.get_mut(selected) else {
                return;
            };
            let trimmed = chat.input.trim();
            if trimmed.is_empty() {
                return;
            }
            let content = trimmed.to_string();
            chat.messages.push(ChatMessage {
                role: MessageRole::User,
                content: content.clone(),
            });
            chat.input.clear();
            chat.scroll = u16::MAX;
            content
        };

        if content.starts_with('/') {
            if let Some(outcome) = super::commands::dispatch(&content) {
                self.apply_command(outcome);
            }
            return;
        }

        let Some(chat) = self.chats.get_mut(selected) else {
            return;
        };
        if let Some(tx) = &chat.user_tx {
            if let Err(err) = tx.try_send(content) {
                let msg = match err {
                    mpsc::error::TrySendError::Full(_) => {
                        "agent busy, message dropped".to_string()
                    }
                    mpsc::error::TrySendError::Closed(_) => {
                        "agent offline, message dropped".to_string()
                    }
                };
                chat.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: msg,
                });
                chat.scroll = u16::MAX;
            }
        }
    }

    pub fn handle_ai_event(&mut self, idx: usize, ev: AIEvent) {
        let Some(chat) = self.chats.get_mut(idx) else {
            debug!(agent_idx = idx, "dropping AI event for unknown agent");
            return;
        };
        match ev {
            AIEvent::SystemInit { session_id, .. } => {
                info!(%session_id, agent_idx = idx, "claude session ready");
            }
            AIEvent::AssistantText { text, .. } => {
                chat.messages.push(ChatMessage {
                    role: MessageRole::Ai,
                    content: text,
                });
            }
            AIEvent::ToolUse { name, input, .. } => {
                let input_str = serde_json::to_string(&input).unwrap_or_default();
                let truncated: String = input_str.chars().take(160).collect();
                chat.messages.push(ChatMessage {
                    role: MessageRole::Tool,
                    content: format!("{name}: {truncated}"),
                });
            }
            AIEvent::ToolResult {
                content, is_error, ..
            } => {
                let content_str = match content {
                    serde_json::Value::String(s) => s,
                    other => serde_json::to_string(&other).unwrap_or_default(),
                };
                let truncated: String = content_str.chars().take(240).collect();
                let prefix = if is_error { "(error) " } else { "" };
                chat.messages.push(ChatMessage {
                    role: MessageRole::Tool,
                    content: format!("{prefix}↳ {truncated}"),
                });
            }
            AIEvent::TurnResult { total_cost_usd, .. } => {
                if let Some(cost) = total_cost_usd {
                    info!(cost_usd = cost, agent_idx = idx, "turn complete");
                }
            }
            AIEvent::Error { message, .. } => {
                chat.messages.push(ChatMessage {
                    role: MessageRole::System,
                    content: message,
                });
            }
            AIEvent::Other(v) => {
                debug!(event = ?v, agent_idx = idx, "unhandled ai event");
            }
        }
        chat.scroll = u16::MAX;
    }

    fn apply_command(&mut self, outcome: super::commands::CommandOutcome) {
        use super::commands::CommandOutcome;
        let selected = self.selected;
        match outcome {
            CommandOutcome::Message(text) => {
                if let Some(chat) = self.chats.get_mut(selected) {
                    chat.messages.push(ChatMessage {
                        role: MessageRole::System,
                        content: text,
                    });
                    chat.scroll = u16::MAX;
                }
            }
            CommandOutcome::Quit => {
                info!("quit requested via /exit");
                self.should_quit = true;
            }
            CommandOutcome::Unknown { name, hint } => {
                if let Some(chat) = self.chats.get_mut(selected) {
                    chat.messages.push(ChatMessage {
                        role: MessageRole::System,
                        content: format!("unknown command: /{name} — {hint}"),
                    });
                    chat.scroll = u16::MAX;
                }
            }
        }
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
                if let Some(chat) = self.current_chat_mut() {
                    chat.input.pop();
                }
            }
            KeyCode::Char(c) => {
                if let Some(chat) = self.current_chat_mut() {
                    chat.input.push(c);
                }
            }
            _ => {}
        }
    }

    pub fn handle_mouse(&mut self, ev: MouseEvent) {
        let pos = (ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(rect) = self.layout.agent_list_inner {
                    if rect_contains(rect, pos) {
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        if row_in_list < self.config.agents.len() {
                            self.selected = row_in_list;
                            self.focus = Focus::Chat;
                        } else {
                            self.focus = Focus::Agents;
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
                    if let Some(chat) = self.current_chat_mut() {
                        chat.scroll = chat.scroll.saturating_sub(1);
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                if self.mouse_in_chat_scroll_zone(pos) {
                    if let Some(chat) = self.current_chat_mut() {
                        chat.scroll = chat.scroll.saturating_add(1);
                    }
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

    // One core per agent, each with its own session and user channel.
    let (ai_tx, ai_rx) = mpsc::channel::<(usize, AIEvent)>(256);
    for (idx, agent) in app.config.agents.iter().enumerate() {
        let (user_tx, user_rx) = mpsc::channel::<String>(8);
        app.chats[idx].user_tx = Some(user_tx);

        let tools: Vec<String> = DEFAULT_ALLOWED_TOOLS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let core: Box<dyn AICore> =
            Box::new(ClaudeCodeCore::new(agent.instructions.clone(), tools));
        tokio::spawn(run_core(idx, core, user_rx, ai_tx.clone()));
    }
    drop(ai_tx);

    let result = event_loop(&mut terminal, &mut app, ai_rx).await;

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

async fn run_core(
    idx: usize,
    mut core: Box<dyn AICore>,
    mut user_rx: mpsc::Receiver<String>,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
) {
    while let Some(prompt) = user_rx.recv().await {
        match core.turn(&prompt).await {
            Ok(mut stream) => {
                while let Some(ev) = stream.next().await {
                    if ai_tx.send((idx, ev)).await.is_err() {
                        return;
                    }
                }
            }
            Err(e) => {
                let _ = ai_tx
                    .send((
                        idx,
                        AIEvent::Error {
                            message: format!("turn failed: {e}"),
                            raw: None,
                        },
                    ))
                    .await;
            }
        }
    }
}

async fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut ai_rx: mpsc::Receiver<(usize, AIEvent)>,
) -> Result<()> {
    let mut events = EventStream::new();
    terminal.draw(|f| ui::render(f, app))?;

    loop {
        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                        debug!(?key, "key press");
                        app.handle_key(key.code, key.modifiers);
                    }
                    Some(Ok(Event::Mouse(mouse))) => {
                        debug!(?mouse, "mouse");
                        app.handle_mouse(mouse);
                    }
                    Some(Ok(Event::Resize(_, _))) => {
                        debug!("resize");
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e.into()),
                    None => break,
                }
            }
            Some((idx, ev)) = ai_rx.recv() => {
                app.handle_ai_event(idx, ev);
            }
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
    fn app_starts_with_chat_focused() {
        let app = App::new(sample_config());
        assert_eq!(app.focus, Focus::Chat);
    }

    #[test]
    fn tab_toggles_focus() {
        let mut app = App::new(sample_config());
        assert_eq!(app.focus, Focus::Chat);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Agents);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Chat);
    }

    #[test]
    fn q_quits_when_agents_focused_but_types_when_chat_focused() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert_eq!(app.current_chat().unwrap().input, "q");

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
        assert_eq!(app.current_chat().unwrap().input, "hello");
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input, "");
        assert!(app
            .current_chat()
            .unwrap()
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

        let last = app.current_chat().unwrap().messages.last().unwrap();
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

        let last = app.current_chat().unwrap().messages.last().unwrap();
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
        let user_msgs: Vec<_> = app
            .current_chat()
            .unwrap()
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
        let initial_count = app.current_chat().unwrap().messages.len();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().messages.len(), initial_count);
    }

    #[test]
    fn click_in_agent_list_selects_agent_and_focuses_chat() {
        let mut app = App::new(sample_config());
        app.layout.agent_list_inner = Some(Rect::new(10, 5, 20, 10));

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 15,
            row: 7, // 7 - 5 = index 2
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.selected, 2);
        assert_eq!(app.focus, Focus::Chat);
    }

    #[test]
    fn j_k_navigation_focuses_chat_after_switching() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Agents;
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.selected, 1);
        assert_eq!(app.focus, Focus::Chat);
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
        assert_eq!(app.current_chat().unwrap().scroll, 0);

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.current_chat().unwrap().scroll, 1);

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.current_chat().unwrap().scroll, 0);

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.current_chat().unwrap().scroll, 0);
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
        assert_eq!(app.current_chat().unwrap().scroll, 0);
    }

    #[test]
    fn assistant_text_event_pushes_ai_message() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::AssistantText {
                text: "hi there".into(),
                raw: serde_json::Value::Null,
            },
        );
        let last = app.chats[0].messages.last().unwrap();
        assert_eq!(last.role, MessageRole::Ai);
        assert_eq!(last.content, "hi there");
    }

    #[test]
    fn tool_use_event_pushes_tool_message() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: serde_json::json!({"command": "ls"}),
                raw: serde_json::Value::Null,
            },
        );
        let last = app.chats[0].messages.last().unwrap();
        assert_eq!(last.role, MessageRole::Tool);
        assert!(last.content.starts_with("Bash: "));
        assert!(last.content.contains("ls"));
    }

    #[test]
    fn error_event_pushes_system_message() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::Error {
                message: "API down".into(),
                raw: None,
            },
        );
        let last = app.chats[0].messages.last().unwrap();
        assert_eq!(last.role, MessageRole::System);
        assert_eq!(last.content, "API down");
    }

    #[test]
    fn ai_events_route_to_the_correct_agent() {
        let mut app = App::new(sample_config());
        // Event targeted at agent index 1 should not land on agent 0.
        app.handle_ai_event(
            1,
            AIEvent::AssistantText {
                text: "for b only".into(),
                raw: serde_json::Value::Null,
            },
        );
        assert!(app.chats[0].messages.is_empty());
        assert_eq!(app.chats[1].messages.len(), 1);
        assert_eq!(app.chats[1].messages[0].content, "for b only");
    }

    #[tokio::test]
    async fn non_slash_input_dispatches_to_current_agent_channel() {
        let mut app = App::new(sample_config());
        let (tx, mut rx) = mpsc::channel::<String>(8);
        app.chats[0].user_tx = Some(tx);
        app.focus = Focus::Chat;
        for c in "hi there".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        let received = rx.try_recv().expect("prompt should be sent");
        assert_eq!(received, "hi there");
    }

    #[tokio::test]
    async fn messages_routed_to_selected_agent_only() {
        let mut app = App::new(sample_config());
        let (tx_a, mut rx_a) = mpsc::channel::<String>(8);
        let (tx_b, mut rx_b) = mpsc::channel::<String>(8);
        app.chats[0].user_tx = Some(tx_a);
        app.chats[1].user_tx = Some(tx_b);
        app.focus = Focus::Chat;

        // Send on agent 0
        for c in "for a".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        // Switch to agent 1 and send
        app.selected = 1;
        for c in "for b".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(rx_a.try_recv().unwrap(), "for a");
        assert_eq!(rx_b.try_recv().unwrap(), "for b");
        assert!(rx_a.try_recv().is_err());
        assert!(rx_b.try_recv().is_err());
    }

    #[tokio::test]
    async fn closed_channel_surfaces_system_message() {
        let mut app = App::new(sample_config());
        let (tx, rx) = mpsc::channel::<String>(1);
        drop(rx);
        app.chats[0].user_tx = Some(tx);
        app.focus = Focus::Chat;
        for c in "hi".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        assert!(app.chats[0]
            .messages
            .iter()
            .any(|m| m.role == MessageRole::System && m.content.contains("offline")));
    }

    #[test]
    fn per_agent_input_buffers_are_independent() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "typing on a".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.selected = 1;
        for c in "typing on b".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(app.chats[0].input, "typing on a");
        assert_eq!(app.chats[1].input, "typing on b");
    }
}
