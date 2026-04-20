use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

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
use tracing::{debug, info, warn};

use crate::ai::{AICore, AIEvent, ClaudeCodeCore};
use crate::config::AciddogConfig;

use super::blocks::{BlockAction, ChatBlock, ChatButton, ScrollAnchor};
use super::clipboard;

use super::ui;

const DEFAULT_ALLOWED_TOOLS: &[&str] = &["Bash", "Read", "Edit", "Grep", "Glob"];
const COPIED_TOAST_MS: u64 = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Agents,
    Chat,
}

#[derive(Debug, Default, Clone)]
pub struct LayoutRects {
    pub agent_list_inner: Option<Rect>,
    pub chat_history_inner: Option<Rect>,
    pub chat_input_inner: Option<Rect>,
    pub chat_buttons: Vec<ChatButton>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentStatus {
    #[default]
    Ready,
    Working,
    Error,
}

#[derive(Debug, Default)]
pub struct AgentChat {
    pub blocks: Vec<ChatBlock>,
    pub tool_index: HashMap<String, usize>,
    pub input: String,
    pub anchor: ScrollAnchor,
    pub sticky_bottom: bool,
    pub selected_block: Option<usize>,
    pub user_tx: Option<mpsc::Sender<String>>,
    pub status: AgentStatus,
}

impl AgentChat {
    fn push_block(&mut self, block: ChatBlock) -> usize {
        let idx = self.blocks.len();
        self.blocks.push(block);
        self.sticky_bottom = true;
        idx
    }
}

pub struct App {
    pub config: AciddogConfig,
    pub selected: usize,
    pub should_quit: bool,
    pub focus: Focus,
    pub chats: Vec<AgentChat>,
    pub layout: LayoutRects,
    pub copied_at: Option<Instant>,
}

impl App {
    pub fn new(config: AciddogConfig) -> Self {
        let chats = (0..config.agents.len())
            .map(|_| AgentChat {
                sticky_bottom: true,
                ..Default::default()
            })
            .collect();
        Self {
            config,
            selected: 0,
            should_quit: false,
            focus: Focus::Chat,
            chats,
            layout: LayoutRects::default(),
            copied_at: None,
        }
    }

    pub fn current_chat(&self) -> Option<&AgentChat> {
        self.chats.get(self.selected)
    }

    pub fn current_chat_mut(&mut self) -> Option<&mut AgentChat> {
        self.chats.get_mut(self.selected)
    }

    pub fn copied_recently(&self) -> bool {
        self.copied_at
            .map(|t| t.elapsed() < Duration::from_millis(COPIED_TOAST_MS))
            .unwrap_or(false)
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
            chat.push_block(ChatBlock::user(content.clone()));
            chat.input.clear();
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
            match tx.try_send(content) {
                Ok(()) => {
                    chat.status = AgentStatus::Working;
                }
                Err(err) => {
                    let msg = match err {
                        mpsc::error::TrySendError::Full(_) => {
                            "agent busy, message dropped".to_string()
                        }
                        mpsc::error::TrySendError::Closed(_) => {
                            "agent offline, message dropped".to_string()
                        }
                    };
                    chat.push_block(ChatBlock::system_error(msg));
                    chat.status = AgentStatus::Error;
                }
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
                chat.push_block(ChatBlock::ai_markdown(text));
            }
            AIEvent::ToolUse {
                id, name, input, ..
            } => {
                let block = ChatBlock::from_tool_use(&name, &input);
                let pos = chat.push_block(block);
                if !id.is_empty() {
                    chat.tool_index.insert(id, pos);
                }
            }
            AIEvent::ToolResult {
                tool_use_id,
                content,
                is_error,
                ..
            } => {
                if let Some(&pos) = chat.tool_index.get(&tool_use_id) {
                    if let Some(block) = chat.blocks.get_mut(pos) {
                        block.apply_tool_result(&content, is_error);
                    }
                } else {
                    // Unknown id — fallback to a generic result block.
                    warn!(%tool_use_id, "orphan tool_result; appending generic block");
                    let input_pretty = serde_json::to_string(&content).unwrap_or_default();
                    chat.push_block(ChatBlock::GenericTool {
                        name: "(orphan result)".into(),
                        input_pretty,
                        result_pretty: String::new(),
                        is_error,
                        collapsed: !is_error,
                        pending: false,
                    });
                }
            }
            AIEvent::TurnResult { total_cost_usd, .. } => {
                if let Some(cost) = total_cost_usd {
                    info!(cost_usd = cost, agent_idx = idx, "turn complete");
                }
                if chat.status == AgentStatus::Working {
                    chat.status = AgentStatus::Ready;
                }
            }
            AIEvent::Error { message, .. } => {
                chat.push_block(ChatBlock::system_error(message));
                chat.status = AgentStatus::Error;
            }
            AIEvent::Other(v) => {
                debug!(event = ?v, agent_idx = idx, "unhandled ai event");
            }
        }
    }

    fn apply_command(&mut self, outcome: super::commands::CommandOutcome) {
        use super::commands::CommandOutcome;
        let selected = self.selected;
        match outcome {
            CommandOutcome::Message(text) => {
                if let Some(chat) = self.chats.get_mut(selected) {
                    chat.push_block(ChatBlock::system_info(text));
                }
            }
            CommandOutcome::Quit => {
                info!("quit requested via /exit");
                self.should_quit = true;
            }
            CommandOutcome::Unknown { name, hint } => {
                if let Some(chat) = self.chats.get_mut(selected) {
                    chat.push_block(ChatBlock::system_warn(format!(
                        "unknown command: /{name} — {hint}"
                    )));
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

    fn handle_key_chat(&mut self, code: KeyCode, mods: KeyModifiers) {
        // Alt-modified keys address blocks, not the input buffer.
        if mods.contains(KeyModifiers::ALT) {
            match code {
                KeyCode::Up => {
                    self.move_selection(-1);
                    return;
                }
                KeyCode::Down => {
                    self.move_selection(1);
                    return;
                }
                KeyCode::Enter => {
                    self.toggle_selected_collapse();
                    return;
                }
                KeyCode::Char('c') => {
                    self.copy_selected_primary();
                    return;
                }
                _ => {}
            }
        }

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

    fn move_selection(&mut self, delta: i32) {
        let Some(chat) = self.current_chat_mut() else {
            return;
        };
        if chat.blocks.is_empty() {
            return;
        }
        let last = chat.blocks.len() - 1;
        let cur = chat.selected_block.unwrap_or(last);
        let next = (cur as i32 + delta).clamp(0, last as i32) as usize;
        chat.selected_block = Some(next);
        chat.sticky_bottom = false;
    }

    fn toggle_selected_collapse(&mut self) {
        let Some(chat) = self.current_chat_mut() else {
            return;
        };
        let Some(i) = chat
            .selected_block
            .or_else(|| chat.blocks.len().checked_sub(1))
        else {
            return;
        };
        if let Some(b) = chat.blocks.get_mut(i) {
            if b.is_collapsible() {
                b.toggle_collapsed();
            }
        }
    }

    fn copy_selected_primary(&mut self) {
        let chat = match self.current_chat() {
            Some(c) => c,
            None => return,
        };
        let idx = chat
            .selected_block
            .or_else(|| chat.blocks.len().checked_sub(1));
        let Some(i) = idx else { return };
        let Some(payload) = chat.blocks.get(i).and_then(|b| b.primary_copy_text()) else {
            return;
        };
        if clipboard::osc52_copy(&payload).is_ok() {
            self.copied_at = Some(Instant::now());
        }
    }

    pub fn handle_mouse(&mut self, ev: MouseEvent) {
        let pos = (ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // 1. Chat buttons take priority.
                if let Some(btn) = self
                    .layout
                    .chat_buttons
                    .iter()
                    .find(|b| rect_contains(b.rect, pos))
                    .cloned()
                {
                    self.dispatch_button(btn.action);
                    return;
                }

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
                    self.scroll_up(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.mouse_in_chat_scroll_zone(pos) {
                    self.scroll_down(3);
                }
            }
            _ => {}
        }
    }

    fn dispatch_button(&mut self, action: BlockAction) {
        match action {
            BlockAction::ToggleCollapse(idx) => {
                if let Some(chat) = self.current_chat_mut() {
                    if let Some(b) = chat.blocks.get_mut(idx) {
                        if b.is_collapsible() {
                            b.toggle_collapsed();
                        }
                    }
                }
            }
            BlockAction::CopyText(payload) => {
                if clipboard::osc52_copy(&payload).is_ok() {
                    self.copied_at = Some(Instant::now());
                }
            }
        }
    }

    fn scroll_up(&mut self, amount: u16) {
        let Some(chat) = self.current_chat_mut() else {
            return;
        };
        chat.sticky_bottom = false;
        let mut left = amount;
        while left > 0 {
            if chat.anchor.within > 0 {
                chat.anchor.within -= 1;
                left -= 1;
            } else if chat.anchor.block_idx > 0 {
                chat.anchor.block_idx -= 1;
                chat.anchor.within = 0;
            } else {
                break;
            }
        }
    }

    fn scroll_down(&mut self, amount: u16) {
        let Some(chat) = self.current_chat_mut() else {
            return;
        };
        chat.sticky_bottom = false;
        let max_block = chat.blocks.len().saturating_sub(1);
        for _ in 0..amount {
            if chat.anchor.block_idx < max_block {
                chat.anchor.within += 1;
                // Height not known here; the renderer will clamp visually.
                // If the within grows too large it'll auto-advance next time
                // the renderer reconciles. Cap at something sane to avoid drift.
                if chat.anchor.within > 64 {
                    chat.anchor.block_idx += 1;
                    chat.anchor.within = 0;
                }
            } else {
                break;
            }
        }
    }

    fn mouse_in_chat_scroll_zone(&self, pos: (u16, u16)) -> bool {
        self.layout
            .chat_history_inner
            .map(|r| rect_contains(r, pos))
            .unwrap_or(false)
    }
}

pub fn rect_contains(rect: Rect, (x, y): (u16, u16)) -> bool {
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
    use serde_json::json;

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

    fn last_block(chat: &AgentChat) -> &ChatBlock {
        chat.blocks.last().expect("no blocks")
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
        let last = last_block(app.current_chat().unwrap());
        match last {
            ChatBlock::UserText { text } => assert_eq!(text, "hello"),
            other => panic!("expected UserText, got {other:?}"),
        }
    }

    #[test]
    fn slash_version_emits_system_block() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "/version".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        let last = last_block(app.current_chat().unwrap());
        match last {
            ChatBlock::System { text, .. } => {
                assert!(text.contains(env!("CARGO_PKG_VERSION")))
            }
            other => panic!("expected System, got {other:?}"),
        }
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

        let last = last_block(app.current_chat().unwrap());
        match last {
            ChatBlock::System { text, .. } => {
                assert!(text.contains("unknown command"));
                assert!(text.contains("frobnicate"));
            }
            other => panic!("expected System, got {other:?}"),
        }
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
        let user_msgs: Vec<&str> = app
            .current_chat()
            .unwrap()
            .blocks
            .iter()
            .filter_map(|b| match b {
                ChatBlock::UserText { text } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(user_msgs, vec!["hello world"]);
    }

    #[test]
    fn empty_input_does_not_create_block() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        let initial = app.current_chat().unwrap().blocks.len();
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().blocks.len(), initial);
    }

    #[test]
    fn click_in_agent_list_selects_agent_and_focuses_chat() {
        let mut app = App::new(sample_config());
        app.layout.agent_list_inner = Some(Rect::new(10, 5, 20, 10));

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 15,
            row: 7,
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
    fn scroll_wheel_in_chat_history_clears_sticky_bottom() {
        let mut app = App::new(sample_config());
        app.layout.chat_history_inner = Some(Rect::new(0, 20, 80, 10));
        let chat = app.current_chat_mut().unwrap();
        chat.blocks.push(ChatBlock::user("first"));
        chat.blocks.push(ChatBlock::user("second"));
        chat.anchor = ScrollAnchor {
            block_idx: 1,
            within: 0,
        };
        chat.sticky_bottom = true;

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        assert!(!app.current_chat().unwrap().sticky_bottom);
        // Anchor moved toward the start.
        let a = app.current_chat().unwrap().anchor;
        assert!(a.block_idx == 0 || a.within == 0);
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
        assert!(app.current_chat().unwrap().sticky_bottom);
    }

    #[test]
    fn assistant_text_event_pushes_ai_markdown_block() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::AssistantText {
                text: "hi **there**".into(),
                raw: serde_json::Value::Null,
            },
        );
        match last_block(&app.chats[0]) {
            ChatBlock::AiMarkdown { raw, .. } => assert_eq!(raw, "hi **there**"),
            other => panic!("expected AiMarkdown, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_and_result_stitch_into_one_bash_block() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({"command": "ls"}),
                raw: serde_json::Value::Null,
            },
        );
        assert_eq!(app.chats[0].blocks.len(), 1);

        app.handle_ai_event(
            0,
            AIEvent::ToolResult {
                tool_use_id: "t1".into(),
                content: serde_json::Value::String("Cargo.toml\nsrc\n".into()),
                is_error: false,
                raw: serde_json::Value::Null,
            },
        );
        // Still a single block — the result was stitched onto the existing one.
        assert_eq!(app.chats[0].blocks.len(), 1);
        match &app.chats[0].blocks[0] {
            ChatBlock::BashTool {
                command,
                stdout,
                pending,
                is_error,
                ..
            } => {
                assert_eq!(command, "ls");
                assert!(stdout.contains("Cargo.toml"));
                assert!(!pending);
                assert!(!is_error);
            }
            other => panic!("expected BashTool, got {other:?}"),
        }
    }

    #[test]
    fn error_tool_result_auto_expands_block() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({"command": "cat /nope"}),
                raw: serde_json::Value::Null,
            },
        );
        app.handle_ai_event(
            0,
            AIEvent::ToolResult {
                tool_use_id: "t1".into(),
                content: serde_json::Value::String("no such file".into()),
                is_error: true,
                raw: serde_json::Value::Null,
            },
        );
        match &app.chats[0].blocks[0] {
            ChatBlock::BashTool {
                is_error,
                collapsed,
                ..
            } => {
                assert!(is_error);
                assert!(!collapsed, "error should auto-expand");
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn orphan_tool_result_becomes_generic_block() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::ToolResult {
                tool_use_id: "unknown".into(),
                content: serde_json::Value::String("content".into()),
                is_error: false,
                raw: serde_json::Value::Null,
            },
        );
        assert!(matches!(
            last_block(&app.chats[0]),
            ChatBlock::GenericTool { .. }
        ));
    }

    #[test]
    fn error_event_pushes_system_error_block() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            0,
            AIEvent::Error {
                message: "API down".into(),
                raw: None,
            },
        );
        match last_block(&app.chats[0]) {
            ChatBlock::System { text, kind } => {
                assert_eq!(text, "API down");
                assert!(matches!(kind, super::super::blocks::SystemKind::Error));
            }
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn ai_events_route_to_the_correct_agent() {
        let mut app = App::new(sample_config());
        app.handle_ai_event(
            1,
            AIEvent::AssistantText {
                text: "for b only".into(),
                raw: serde_json::Value::Null,
            },
        );
        assert!(app.chats[0].blocks.is_empty());
        assert_eq!(app.chats[1].blocks.len(), 1);
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

        for c in "for a".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

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
    async fn closed_channel_surfaces_system_error_block() {
        let mut app = App::new(sample_config());
        let (tx, rx) = mpsc::channel::<String>(1);
        drop(rx);
        app.chats[0].user_tx = Some(tx);
        app.focus = Focus::Chat;
        for c in "hi".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);

        let saw_offline = app.chats[0].blocks.iter().any(|b| match b {
            ChatBlock::System { text, .. } => text.contains("offline"),
            _ => false,
        });
        assert!(saw_offline);
    }

    #[test]
    fn initial_agent_status_is_ready() {
        let app = App::new(sample_config());
        assert_eq!(app.chats[0].status, AgentStatus::Ready);
    }

    #[tokio::test]
    async fn submit_marks_agent_working() {
        let mut app = App::new(sample_config());
        let (tx, _rx) = mpsc::channel::<String>(8);
        app.chats[0].user_tx = Some(tx);
        app.focus = Focus::Chat;
        for c in "hi".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.chats[0].status, AgentStatus::Working);
    }

    #[test]
    fn turn_result_returns_agent_to_ready() {
        let mut app = App::new(sample_config());
        app.chats[0].status = AgentStatus::Working;
        app.handle_ai_event(
            0,
            AIEvent::TurnResult {
                stop_reason: Some("end_turn".into()),
                total_cost_usd: Some(0.01),
                raw: serde_json::Value::Null,
            },
        );
        assert_eq!(app.chats[0].status, AgentStatus::Ready);
    }

    #[test]
    fn error_event_marks_agent_error() {
        let mut app = App::new(sample_config());
        app.chats[0].status = AgentStatus::Working;
        app.handle_ai_event(
            0,
            AIEvent::Error {
                message: "boom".into(),
                raw: None,
            },
        );
        assert_eq!(app.chats[0].status, AgentStatus::Error);
    }

    #[tokio::test]
    async fn closed_channel_sets_error_status() {
        let mut app = App::new(sample_config());
        let (tx, rx) = mpsc::channel::<String>(1);
        drop(rx);
        app.chats[0].user_tx = Some(tx);
        app.focus = Focus::Chat;
        for c in "hi".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.chats[0].status, AgentStatus::Error);
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

    // --------- New tests for rich blocks ----------

    #[test]
    fn alt_up_down_moves_selection() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.chats[0].blocks.push(ChatBlock::user("one"));
        app.chats[0].blocks.push(ChatBlock::user("two"));
        app.chats[0].blocks.push(ChatBlock::user("three"));

        app.handle_key(KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(app.chats[0].selected_block, Some(1));
        app.handle_key(KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(app.chats[0].selected_block, Some(0));
        app.handle_key(KeyCode::Up, KeyModifiers::ALT);
        assert_eq!(app.chats[0].selected_block, Some(0)); // clamped
        app.handle_key(KeyCode::Down, KeyModifiers::ALT);
        assert_eq!(app.chats[0].selected_block, Some(1));
    }

    #[test]
    fn alt_enter_toggles_collapse_on_selected_block() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({"command": "ls"}),
                raw: serde_json::Value::Null,
            },
        );
        // Starts collapsed.
        match &app.chats[0].blocks[0] {
            ChatBlock::BashTool { collapsed, .. } => assert!(*collapsed),
            _ => unreachable!(),
        }
        app.chats[0].selected_block = Some(0);
        app.handle_key(KeyCode::Enter, KeyModifiers::ALT);
        match &app.chats[0].blocks[0] {
            ChatBlock::BashTool { collapsed, .. } => assert!(!*collapsed),
            _ => unreachable!(),
        }
    }

    #[test]
    fn alt_c_on_user_block_without_selection_copies_last_block() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.chats[0].blocks.push(ChatBlock::user("to copy"));
        // Nothing selected.
        assert!(app.chats[0].selected_block.is_none());
        // Should not panic; may or may not land in system clipboard
        // depending on terminal, but the flag must flip either way if osc52 write succeeds.
        app.handle_key(KeyCode::Char('c'), KeyModifiers::ALT);
        // We can't easily assert clipboard contents; just confirm no crash
        // and that plain 'c' without ALT still types.
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input, "c");
    }

    #[test]
    fn click_on_chat_button_toggles_collapse() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({"command": "ls"}),
                raw: serde_json::Value::Null,
            },
        );
        // Simulate a button registered by a prior render.
        app.layout.chat_buttons.push(ChatButton {
            rect: Rect::new(10, 5, 5, 1),
            action: BlockAction::ToggleCollapse(0),
        });

        // Pre-state.
        match &app.chats[0].blocks[0] {
            ChatBlock::BashTool { collapsed, .. } => assert!(*collapsed),
            _ => unreachable!(),
        }

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });

        match &app.chats[0].blocks[0] {
            ChatBlock::BashTool { collapsed, .. } => {
                assert!(!*collapsed, "click on toggle button should expand")
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn new_block_sets_sticky_bottom() {
        let mut app = App::new(sample_config());
        app.chats[0].sticky_bottom = false;
        app.handle_ai_event(
            0,
            AIEvent::AssistantText {
                text: "hi".into(),
                raw: serde_json::Value::Null,
            },
        );
        assert!(app.chats[0].sticky_bottom);
    }

    #[test]
    fn each_tool_kind_routes_to_expected_variant() {
        let mut app = App::new(sample_config());
        let cases: Vec<(&str, serde_json::Value)> = vec![
            (
                "Edit",
                json!({"file_path": "a", "old_string": "", "new_string": "hi"}),
            ),
            ("Write", json!({"file_path": "b", "content": "x"})),
            ("Read", json!({"file_path": "c"})),
            ("Grep", json!({"pattern": "foo"})),
            ("Glob", json!({"pattern": "*.rs"})),
        ];
        for (i, (name, input)) in cases.into_iter().enumerate() {
            app.handle_ai_event(
                0,
                AIEvent::ToolUse {
                    id: format!("t{i}"),
                    name: name.to_string(),
                    input,
                    raw: serde_json::Value::Null,
                },
            );
        }
        let kinds: Vec<&str> = app.chats[0]
            .blocks
            .iter()
            .map(|b| match b {
                ChatBlock::EditTool { .. } => "Edit",
                ChatBlock::WriteTool { .. } => "Write",
                ChatBlock::ReadTool { .. } => "Read",
                ChatBlock::GrepTool { .. } => "Grep",
                ChatBlock::GlobTool { .. } => "Glob",
                _ => "?",
            })
            .collect();
        assert_eq!(kinds, vec!["Edit", "Write", "Read", "Grep", "Glob"]);
    }
}
