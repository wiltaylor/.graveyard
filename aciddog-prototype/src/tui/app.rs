use std::collections::HashMap;
use std::io;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
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
use uuid::Uuid;

use crate::agent_ipc::{self, AgentSocketEvent, RequestCmd, Response};
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
    /// Priority queue: user-typed prompts. Unbounded so a busy agent can
    /// never make the TUI drop keystrokes. Drained before `agent_tx`.
    pub user_tx: Option<mpsc::UnboundedSender<String>>,
    /// Inter-agent messages queued through the IPC socket. Bounded so a
    /// runaway sender can't exhaust memory; the socket handler awaits
    /// when full.
    pub agent_tx: Option<mpsc::Sender<String>>,
    pub status: AgentStatus,
    /// Session UUID shared with the AICore and the agent IPC layer. Set at
    /// spawn time; used to route inbound socket requests to the right chat.
    pub session_id: Option<Uuid>,
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
    /// Maps each agent's session UUID to its config id. Populated at spawn
    /// time so the IPC handler can resolve a sender's friendly name from
    /// the UUID it sees in `Request::agent_id`.
    pub agent_ids: HashMap<Uuid, String>,
}

impl App {
    pub fn new(config: AciddogConfig) -> Self {
        let chats: Vec<AgentChat> = config
            .agents
            .iter()
            .map(|agent| {
                let info = ChatBlock::agent_info(
                    "claude-code",
                    DEFAULT_ALLOWED_TOOLS
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect(),
                    agent.instructions.clone(),
                );
                AgentChat {
                    blocks: vec![info],
                    sticky_bottom: true,
                    ..Default::default()
                }
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
            agent_ids: HashMap::new(),
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
            match tx.send(content) {
                Ok(()) => {
                    chat.status = AgentStatus::Working;
                }
                Err(_closed) => {
                    chat.push_block(ChatBlock::system_error(
                        "agent offline, message dropped".to_string(),
                    ));
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

    pub fn handle_agent_socket_event(&mut self, event: AgentSocketEvent) {
        let AgentSocketEvent { request, reply } = event;
        let Some(idx) = self
            .chats
            .iter()
            .position(|c| c.session_id == Some(request.agent_id))
        else {
            let _ = reply.send(Response::err(format!(
                "no live agent with id {}",
                request.agent_id
            )));
            return;
        };

        match request.cmd {
            RequestCmd::Test => {
                let msg = "agent connectivity test passed";
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info(msg));
                }
                let _ = reply.send(Response::ok(msg));
            }
            RequestCmd::SendMessage { to, body } => {
                self.handle_send_message(idx, to, body, reply);
            }
        }
    }

    fn handle_send_message(
        &mut self,
        sender_idx: usize,
        to: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_id) = self.config.agents.get(sender_idx).map(|a| a.id.clone()) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };

        if to == sender_id {
            let _ = reply.send(Response::err("cannot send a message to yourself"));
            return;
        }

        let Some(recipient_idx) = self.config.agents.iter().position(|a| a.id == to) else {
            let _ = reply.send(Response::err(format!("no agent named {to}")));
            return;
        };

        if !self.config.can_send(&sender_id, &to) {
            let _ = reply.send(Response::err(format!(
                "agent `{sender_id}` is not permitted to message `{to}`"
            )));
            return;
        }

        let payload = format!("<Message from=\"{sender_id}\">{body}</Message>");

        let Some(agent_tx) = self
            .chats
            .get(recipient_idx)
            .and_then(|c| c.agent_tx.clone())
        else {
            let _ = reply.send(Response::err(format!("agent `{to}` is not online")));
            return;
        };

        // Echo into both chats so the user can see the traffic.
        if let Some(chat) = self.chats.get_mut(recipient_idx) {
            chat.push_block(ChatBlock::system_info(format!(
                "message from {sender_id}: {body}"
            )));
        }
        if let Some(chat) = self.chats.get_mut(sender_idx) {
            chat.push_block(ChatBlock::system_info(format!(
                "sent message to {to}: {body}"
            )));
        }

        // Enqueue on a background task so the IPC reply doesn't depend on
        // the recipient actually running. Reply "queued" immediately.
        tokio::spawn(async move {
            if agent_tx.send(payload).await.is_err() {
                tracing::warn!(recipient = %to, "agent message dropped: recipient channel closed");
            }
        });
        let _ = reply.send(Response::ok("queued"));
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

    // Bind the agent IPC socket for this team before spawning cores, so its
    // path can be injected into each claude child's environment.
    let team_id = app.config.team.id.clone();
    let mut socket = agent_ipc::server::bind(&team_id)
        .await
        .with_context(|| format!("binding agent IPC socket for team {team_id}"))?;
    info!(path = %socket.path.display(), "agent IPC socket bound");
    let socket_path_str = socket.path.display().to_string();

    // Resolve our own binary path so agents can invoke aciddog without it
    // being on PATH (cargo run, dev installs, nix store, etc).
    let aciddog_binary = std::env::current_exe()
        .context("resolving current aciddog binary path for agent preamble")?;
    let aciddog_binary_str = aciddog_binary.to_string_lossy().into_owned();
    info!(binary = %aciddog_binary_str, "agent CLI binary");

    // One core per agent, each with its own session, user channel, and
    // agent-to-agent channel. User channel is unbounded (the TUI key
    // handler must never block or drop); agent channel is bounded so
    // the socket side has backpressure.
    let (ai_tx, ai_rx) = mpsc::channel::<(usize, AIEvent)>(256);
    let all_ids: Vec<String> = app.config.agents.iter().map(|a| a.id.clone()).collect();
    for (idx, agent) in app.config.agents.iter().enumerate() {
        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);
        app.chats[idx].user_tx = Some(user_tx);
        app.chats[idx].agent_tx = Some(agent_tx);

        let tools: Vec<String> = DEFAULT_ALLOWED_TOOLS
            .iter()
            .map(|s| (*s).to_string())
            .collect();

        let session_id = Uuid::new_v4();
        app.chats[idx].session_id = Some(session_id);
        app.agent_ids.insert(session_id, agent.id.clone());

        // Compute the list of other agents this one is allowed to message.
        // `None` in config means unrestricted (all teammates except self);
        // `Some(list)` is used verbatim. An empty list disables `send`.
        let contactable: Vec<&str> = match &agent.can_message {
            Some(list) => list.iter().map(String::as_str).collect(),
            None => all_ids
                .iter()
                .filter(|id| id.as_str() != agent.id.as_str())
                .map(String::as_str)
                .collect(),
        };

        let instructions = format!(
            "{}\n\n{}",
            agent_ipc::preamble::cli_preamble(&aciddog_binary_str, &contactable),
            agent.instructions,
        );

        // Reflect the full prompt (preamble + instructions) in the top
        // AgentInfo block so the user can expand it and see exactly what
        // the LLM receives.
        if let Some(ChatBlock::AgentInfo { prompt, .. }) = app.chats[idx].blocks.first_mut() {
            *prompt = instructions.clone();
        }

        let env = vec![
            (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
            (
                agent_ipc::ENV_AGENT_SOCKET.to_string(),
                socket_path_str.clone(),
            ),
        ];

        let core: Box<dyn AICore> =
            Box::new(ClaudeCodeCore::new(session_id, instructions, tools).with_env(env));
        tokio::spawn(run_core(idx, core, user_rx, agent_rx, ai_tx.clone()));
    }
    drop(ai_tx);

    let result = event_loop(&mut terminal, &mut app, ai_rx, &mut socket.events).await;

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
    mut user_rx: mpsc::UnboundedReceiver<String>,
    mut agent_rx: mpsc::Receiver<String>,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
) {
    loop {
        // Biased select: user-typed prompts always preempt queued
        // inter-agent messages when both are pending.
        let prompt = tokio::select! {
            biased;
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            else => return,
        };

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
    socket_rx: &mut mpsc::Receiver<AgentSocketEvent>,
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
            Some(event) = socket_rx.recv() => {
                app.handle_agent_socket_event(event);
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
                    can_message: None,
                },
                Agent {
                    id: "b".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                },
                Agent {
                    id: "c".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                },
            ],
        }
    }

    fn last_block(chat: &AgentChat) -> &ChatBlock {
        chat.blocks.last().expect("no blocks")
    }

    /// Build an app and clear the per-agent seeded AgentInfo blocks so tests
    /// that assert on absolute block indices/length don't have to account for
    /// them.
    fn fresh_app() -> App {
        let mut app = App::new(sample_config());
        for c in &mut app.chats {
            c.blocks.clear();
            c.tool_index.clear();
        }
        app
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
        let mut app = fresh_app();
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
        let mut app = fresh_app();
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
        let mut app = fresh_app();
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
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
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
        let (tx_a, mut rx_a) = mpsc::unbounded_channel::<String>();
        let (tx_b, mut rx_b) = mpsc::unbounded_channel::<String>();
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
        let (tx, rx) = mpsc::unbounded_channel::<String>();
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
        let (tx, _rx) = mpsc::unbounded_channel::<String>();
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
        let (tx, rx) = mpsc::unbounded_channel::<String>();
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
        let mut app = fresh_app();
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
        let mut app = fresh_app();
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
        let mut app = fresh_app();
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
        let mut app = fresh_app();
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

    #[test]
    fn each_chat_is_seeded_with_an_agent_info_block() {
        let app = App::new(sample_config());
        for (i, chat) in app.chats.iter().enumerate() {
            assert_eq!(chat.blocks.len(), 1, "chat {i} missing agent info block");
            match &chat.blocks[0] {
                ChatBlock::AgentInfo {
                    core,
                    tools,
                    prompt,
                    collapsed,
                } => {
                    assert_eq!(core, "claude-code");
                    assert!(tools.contains(&"Bash".to_string()));
                    assert_eq!(prompt, &app.config.agents[i].instructions);
                    assert!(*collapsed, "prompt starts collapsed");
                }
                other => panic!("expected AgentInfo, got {other:?}"),
            }
        }
    }

    // --------- Mailbox / inter-agent messaging ----------

    use tokio::sync::oneshot;

    fn wire_up_agent_ids_and_channels(app: &mut App) -> Vec<mpsc::Receiver<String>> {
        let mut agent_rxs = Vec::new();
        for (i, agent) in app.config.agents.clone().iter().enumerate() {
            let session_id = Uuid::new_v4();
            app.chats[i].session_id = Some(session_id);
            app.agent_ids.insert(session_id, agent.id.clone());
            let (agent_tx, agent_rx) = mpsc::channel::<String>(16);
            app.chats[i].agent_tx = Some(agent_tx);
            agent_rxs.push(agent_rx);
        }
        agent_rxs
    }

    #[tokio::test]
    async fn send_message_wraps_body_with_sender_config_id() {
        let mut app = App::new(sample_config());
        let mut agent_rxs = wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "b".into(),
                    body: "hello".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        // Recipient is chat index 1.
        let payload = agent_rxs[1].recv().await.expect("delivered");
        assert_eq!(payload, r#"<Message from="a">hello</Message>"#);
    }

    #[tokio::test]
    async fn send_message_rejects_self() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "a".into(),
                    body: "hi".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("yourself"));
    }

    #[tokio::test]
    async fn send_message_rejects_unknown_recipient() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "ghost".into(),
                    body: "hi".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("no agent named ghost"));
    }

    #[tokio::test]
    async fn send_message_rejects_when_allowlist_disallows_recipient() {
        let mut cfg = sample_config();
        // Restrict agent `a` to message only `b`.
        cfg.agents[0].can_message = Some(vec!["b".to_string()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "c".into(),
                    body: "hi".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error.as_ref().unwrap().contains("not permitted"),
            "got: {resp:?}"
        );
    }

    #[tokio::test]
    async fn submit_input_is_not_wrapped() {
        // User-typed input must arrive plain — only agent-to-agent traffic
        // is wrapped in <Message> tags.
        let mut app = App::new(sample_config());
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        app.chats[0].user_tx = Some(tx);
        app.focus = Focus::Chat;
        for c in "plain body".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let received = rx.try_recv().unwrap();
        assert_eq!(received, "plain body");
        assert!(!received.starts_with("<Message"));
    }

    #[tokio::test]
    async fn user_messages_preempt_queued_agent_messages() {
        // Biased select in run_core must drain all pending user messages
        // before any queued agent message is picked.
        let (user_tx, mut user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, mut agent_rx) = mpsc::channel::<String>(16);

        agent_tx.send("agent-first".into()).await.unwrap();
        user_tx.send("user-first".into()).unwrap();
        user_tx.send("user-second".into()).unwrap();

        // Simulate run_core's drain loop.
        let first = tokio::select! {
            biased;
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            else => unreachable!(),
        };
        let second = tokio::select! {
            biased;
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            else => unreachable!(),
        };
        let third = tokio::select! {
            biased;
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            else => unreachable!(),
        };
        assert_eq!(first, "user-first");
        assert_eq!(second, "user-second");
        assert_eq!(third, "agent-first");
    }

    #[tokio::test]
    async fn submit_input_does_not_drop_under_flood() {
        // Unbounded user channel: 10k pushes all arrive, none dropped.
        let mut app = App::new(sample_config());
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        app.chats[0].user_tx = Some(tx);
        app.focus = Focus::Chat;
        for i in 0..10_000 {
            let msg = format!("m{i}");
            for c in msg.chars() {
                app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
            }
            app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        }
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 10_000);
    }
}
