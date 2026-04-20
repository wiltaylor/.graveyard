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

use clap::Parser;

use crate::agent_ipc::{self, AgentSocketEvent, RequestCmd, Response};
use crate::ai::{AICore, AIEvent, ClaudeCodeCore};
use crate::cli::{AgentArgs, AgentCommand};
use crate::config::AciddogConfig;
use crate::tasks::{Task, TaskFilter, TaskPatch, TaskStatus, TaskStore};

use super::tasks_view::{TaskForm, TasksPrompt, TasksScreen, OPERATOR};

use super::blocks::{BlockAction, ChatBlock, ChatButton, ScrollAnchor};
use super::clipboard;

use super::ui;

const DEFAULT_ALLOWED_TOOLS: &[&str] = &["Bash", "Read", "Edit", "Grep", "Glob"];
const COPIED_TOAST_MS: u64 = 1500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Agents,
    Chat,
    /// The right-column "Views" list (above the agents list) that
    /// switches the main area between chat and alternate views.
    Views,
    /// The main area while `active_view == Tasks` — keyboard input
    /// goes to the tasks screen handler instead of the chat.
    Tasks,
}

/// Which content the main (left) area is currently showing. Chat
/// is the default per-agent view; Tasks is the in-TUI task board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveView {
    #[default]
    Chat,
    Tasks,
}

/// Entries in the right-column Views list. Currently a single
/// entry; adding more is just extending this array and wiring a new
/// activation arm in `handle_key_views`.
pub const VIEW_ENTRIES: &[&str] = &["Tasks"];

#[derive(Debug, Default, Clone)]
pub struct LayoutRects {
    pub agent_list_inner: Option<Rect>,
    pub chat_history_inner: Option<Rect>,
    pub chat_input_inner: Option<Rect>,
    /// Inner rect of the "views" panel on the right column. Populated
    /// by `render()` so mouse clicks on a view entry can hit-test
    /// against the same geometry the UI drew.
    pub views_list_inner: Option<Rect>,
    pub chat_buttons: Vec<ChatButton>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentStatus {
    #[default]
    Ready,
    Working,
    Error,
    /// A worker that has called `aciddog worker complete`. Its
    /// `ClaudeCodeCore` is gone, but the session UUID and chat history
    /// are retained so the parent can resume it later.
    Hibernated,
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
    /// For workers, index into `App::chats` of the chat that spawned
    /// this one. `None` for statically configured agents. Used to
    /// enforce "worker can only message its parent" routing.
    pub parent_idx: Option<usize>,
    /// For workers, the id of the `worker { ... }` template this
    /// instance was spawned from. Needed to look up
    /// `can_create_workers` for nested spawns and to rebuild the tool
    /// list / instructions on resurrection.
    pub worker_template: Option<String>,
    /// Display name for the agent list. Matches the config id for
    /// statically declared agents and `<template>#<uuid-prefix>` for
    /// worker instances. Also used as the `send --to` target.
    pub display_name: String,
    /// True after `worker complete`. Channels are dropped; the pane
    /// stays visible. Next inbound message triggers resurrection via
    /// `App::resurrect_worker`.
    pub hibernated: bool,
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
    /// Absolute path to the running aciddog binary. Used to detect
    /// `aciddog send …` bash tool invocations and rewrite them into a
    /// richer Mail block.
    pub aciddog_binary: String,
    /// Socket path injected into every spawned agent's environment.
    /// Set by `run`; required for spawning workers at runtime.
    pub socket_path: String,
    /// AI-event sender cloned for every `run_core` task. Kept on `App`
    /// so new worker spawns can route their stream into the same
    /// event loop that serves configured agents.
    pub ai_tx: Option<mpsc::Sender<(usize, AIEvent)>>,
    /// Shared team task board. `App::new` initialises this ephemeral
    /// (in-memory only); `tui::run` replaces it with a disk-backed
    /// store rooted at `.aciddog/tasks.json` in the cwd so tasks
    /// survive restarts.
    pub tasks: TaskStore,
    /// Which content is rendered in the main (left) area.
    pub active_view: ActiveView,
    /// Which row of the right-column Views list is highlighted.
    pub selected_view: usize,
    /// UI-local state for the Tasks screen: cursor, modals, prompts.
    pub tasks_screen: TasksScreen,
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
                    display_name: agent.id.clone(),
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
            aciddog_binary: String::new(),
            socket_path: String::new(),
            ai_tx: None,
            tasks: TaskStore::new(),
            active_view: ActiveView::default(),
            selected_view: 0,
            tasks_screen: TasksScreen::default(),
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

    /// Display order for the agent list: chat indices paired with
    /// their indentation depth. Hibernated chats are skipped. Workers
    /// appear immediately after the nearest *visible* ancestor; if
    /// that ancestor is hidden the worker floats up to the next one.
    pub fn visible_order(&self) -> Vec<(usize, usize)> {
        // Precompute nearest visible ancestor for every chat. `None`
        // means the chat is rendered at the top level.
        let mut visible_parent: Vec<Option<usize>> = vec![None; self.chats.len()];
        for (i, c) in self.chats.iter().enumerate() {
            let mut p = c.parent_idx;
            while let Some(pi) = p {
                match self.chats.get(pi) {
                    Some(parent) if !parent.hibernated => {
                        visible_parent[i] = Some(pi);
                        break;
                    }
                    Some(parent) => {
                        p = parent.parent_idx;
                    }
                    None => break,
                }
            }
        }

        let mut out: Vec<(usize, usize)> = Vec::with_capacity(self.chats.len());
        // Iterative DFS keyed by "what's the visible parent we're
        // currently listing children of?". Stable recursion is fine
        // here because `chats` is small (tens of entries at most).
        fn walk(
            chats: &[AgentChat],
            vp: &[Option<usize>],
            out: &mut Vec<(usize, usize)>,
            parent: Option<usize>,
            depth: usize,
        ) {
            for (i, c) in chats.iter().enumerate() {
                if c.hibernated {
                    continue;
                }
                if vp[i] == parent {
                    out.push((i, depth));
                    walk(chats, vp, out, Some(i), depth + 1);
                }
            }
        }
        walk(&self.chats, &visible_parent, &mut out, None, 0);
        out
    }

    fn select_next(&mut self) {
        let order = self.visible_order();
        if order.is_empty() {
            return;
        }
        let pos = order.iter().position(|(i, _)| *i == self.selected);
        let next = match pos {
            Some(p) => (p + 1) % order.len(),
            None => 0,
        };
        self.selected = order[next].0;
        self.focus = Focus::Chat;
    }

    fn select_prev(&mut self) {
        let order = self.visible_order();
        if order.is_empty() {
            return;
        }
        let pos = order.iter().position(|(i, _)| *i == self.selected);
        let prev = match pos {
            Some(0) => order.len() - 1,
            Some(p) => p - 1,
            None => 0,
        };
        self.selected = order[prev].0;
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
        let binary = self.aciddog_binary.clone();
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
                // Rewrite `aciddog send`/`worker create`/`worker complete`
                // invocations into dedicated blocks so the UI shows the
                // semantic action instead of the raw shell command.
                let block = if name == "Bash" {
                    let cmd = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
                    if let Some((to, body)) = try_parse_aciddog_send(cmd, &binary) {
                        ChatBlock::mail_sent(to, body)
                    } else if let Some((template, task)) =
                        try_parse_aciddog_worker_create(cmd, &binary)
                    {
                        ChatBlock::worker_spawn(template, task)
                    } else if let Some(body) = try_parse_aciddog_worker_complete(cmd, &binary) {
                        ChatBlock::worker_complete(body)
                    } else {
                        ChatBlock::from_tool_use(&name, &input)
                    }
                } else {
                    ChatBlock::from_tool_use(&name, &input)
                };
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
            RequestCmd::WorkerCreate { template, task } => {
                self.handle_worker_create(idx, template, task, reply);
            }
            RequestCmd::WorkerComplete { body } => {
                self.handle_worker_complete(idx, body, reply);
            }
            RequestCmd::TaskAdd {
                title,
                description,
                assignee,
                blocked_by,
            } => {
                self.handle_task_add(idx, title, description, assignee, blocked_by, reply);
            }
            RequestCmd::TaskList {
                assignee,
                status,
                mine,
                unassigned,
            } => {
                self.handle_task_list(idx, assignee, status, mine, unassigned, reply);
            }
            RequestCmd::TaskUpdate {
                id,
                title,
                description,
                status,
                assignee,
                unassign,
                note,
                add_blocked_by,
                remove_blocked_by,
            } => {
                self.handle_task_update(
                    idx,
                    id,
                    title,
                    description,
                    status,
                    assignee,
                    unassign,
                    note,
                    add_blocked_by,
                    remove_blocked_by,
                    reply,
                );
            }
            RequestCmd::TaskRemove { id } => {
                self.handle_task_remove(idx, id, reply);
            }
            RequestCmd::TaskTake => {
                self.handle_task_take(idx, reply);
            }
            RequestCmd::TaskNote { id, body } => {
                self.handle_task_note(idx, id, body, reply);
            }
            RequestCmd::TaskShow { id } => {
                self.handle_task_show(idx, id, reply);
            }
        }
    }

    fn handle_worker_create(
        &mut self,
        sender_idx: usize,
        template: String,
        task: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        // Resolve the sender's "template id" for the allowlist check.
        // Configured agents are identified by config id; workers are
        // identified by the template they were spawned from.
        let sender_template = match self.chats.get(sender_idx) {
            Some(c) if c.worker_template.is_some() => c.worker_template.clone().unwrap(),
            Some(_) => match self.config.agents.get(sender_idx) {
                Some(a) => a.id.clone(),
                None => {
                    let _ = reply.send(Response::err("unknown sender"));
                    return;
                }
            },
            None => {
                let _ = reply.send(Response::err("unknown sender"));
                return;
            }
        };

        let sender_display = match self.chats.get(sender_idx) {
            Some(c) => c.display_name.clone(),
            None => {
                let _ = reply.send(Response::err("unknown sender"));
                return;
            }
        };

        if !self.config.can_create(&sender_template, &template) {
            let _ = reply.send(Response::err(format!(
                "`{sender_template}` is not permitted to create workers of template `{template}`"
            )));
            return;
        }

        let worker = match self.config.worker(&template).cloned() {
            Some(w) => w,
            None => {
                let _ = reply.send(Response::err(format!("no worker template `{template}`")));
                return;
            }
        };

        let Some(ai_tx) = self.ai_tx.clone() else {
            let _ = reply.send(Response::err(
                "orchestrator has no ai_tx; cannot spawn workers at runtime",
            ));
            return;
        };

        let session_id = Uuid::new_v4();
        let display_name = format!("{}#{}", template, &session_id.to_string()[..8]);

        // Tool list: explicit template tools override the default.
        let tools: Vec<String> = worker.tools.clone().unwrap_or_else(|| {
            DEFAULT_ALLOWED_TOOLS
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        });

        let creatable: Vec<&str> = worker
            .can_create_workers
            .as_deref()
            .map(|list| list.iter().map(String::as_str).collect())
            .unwrap_or_default();

        let instructions = format!(
            "{}\n\n{}\n\n{}",
            agent_ipc::preamble::worker_preamble(&self.aciddog_binary, &sender_display, &task),
            agent_ipc::preamble::cli_preamble(
                &self.aciddog_binary,
                &[sender_display.as_str()],
                &creatable,
            ),
            worker.instructions,
        );

        let env = vec![
            (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
            (
                agent_ipc::ENV_AGENT_SOCKET.to_string(),
                self.socket_path.clone(),
            ),
        ];

        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);

        let info_block = ChatBlock::agent_info("claude-code", tools.clone(), instructions.clone());
        let mut chat = AgentChat {
            blocks: vec![info_block],
            sticky_bottom: true,
            user_tx: Some(user_tx.clone()),
            agent_tx: Some(agent_tx),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            parent_idx: Some(sender_idx),
            worker_template: Some(template.clone()),
            display_name: display_name.clone(),
            hibernated: false,
            ..Default::default()
        };
        // Surface the initial task as a user message on the worker's
        // pane so its chat reads naturally.
        chat.push_block(ChatBlock::user(task.clone()));

        let new_idx = self.chats.len();
        self.chats.push(chat);
        self.agent_ids.insert(session_id, display_name.clone());

        // Feed the task as the first prompt.
        if user_tx.send(task.clone()).is_err() {
            // Should not happen — we just created the channel.
            tracing::warn!(%display_name, "user_tx closed before initial task could be sent");
        }

        let core: Box<dyn AICore> =
            Box::new(ClaudeCodeCore::new(session_id, instructions, tools).with_env(env));
        tokio::spawn(run_core(new_idx, core, user_rx, agent_rx, ai_tx));

        // Surface the spawn on the parent pane so the user can see it.
        if let Some(parent) = self.chats.get_mut(sender_idx) {
            parent.push_block(ChatBlock::WorkerSpawn {
                template: template.clone(),
                worker_id: Some(display_name.clone()),
                task: task.clone(),
                is_error: false,
                collapsed: true,
                pending: false,
            });
        }

        // Reply with "<display_name> <uuid>" so the parent can parse
        // either token: display_name for subsequent `send --to`,
        // UUID if it wants to track the session explicitly.
        let _ = reply.send(Response::ok(format!("{display_name} {session_id}")));
    }

    fn handle_worker_complete(
        &mut self,
        sender_idx: usize,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let (parent_idx, sender_display) = match self.chats.get(sender_idx) {
            Some(c) if c.parent_idx.is_some() => (c.parent_idx.unwrap(), c.display_name.clone()),
            Some(_) => {
                let _ = reply.send(Response::err(
                    "`worker complete` may only be called from a running worker",
                ));
                return;
            }
            None => {
                let _ = reply.send(Response::err("unknown sender"));
                return;
            }
        };

        // Deliver the final report to the parent.
        let payload = format!("<Message from=\"{sender_display}\">{body}</Message>");
        let parent_tx = self.chats.get(parent_idx).and_then(|c| c.agent_tx.clone());
        if let Some(tx) = parent_tx.clone() {
            let body_for_task = payload.clone();
            tokio::spawn(async move {
                if tx.send(body_for_task).await.is_err() {
                    tracing::warn!("final report dropped: parent channel closed");
                }
            });
        }
        if let Some(parent_chat) = self.chats.get_mut(parent_idx) {
            parent_chat.push_block(ChatBlock::mail_received(
                sender_display.clone(),
                body.clone(),
            ));
        }

        // Record the final report on the worker's own pane, then
        // hibernate: drop both channel senders so `run_core` exits on
        // its next iteration and the claude child tears down.
        if let Some(chat) = self.chats.get_mut(sender_idx) {
            chat.push_block(ChatBlock::WorkerComplete {
                body: body.clone(),
                is_error: false,
                collapsed: true,
                pending: false,
            });
            chat.user_tx = None;
            chat.agent_tx = None;
            chat.status = AgentStatus::Hibernated;
            chat.hibernated = true;
        }

        // A hibernated worker is hidden from the agent list. If the
        // user was viewing its pane, snap selection to the parent so
        // the UI lands somewhere still visible.
        if self.selected == sender_idx {
            self.selected = parent_idx;
        }

        let _ = reply.send(Response::ok("hibernated"));
    }

    /// Restart a hibernated worker so a newly delivered message can be
    /// handled. Recreates the IPC channels, rebuilds the
    /// `ClaudeCodeCore` via `with_session_id` (which uses
    /// `claude --resume` on its first turn so the model resumes its
    /// prior history), and spawns a fresh `run_core`.
    fn resurrect_worker(&mut self, idx: usize) -> Result<(), String> {
        let Some(ai_tx) = self.ai_tx.clone() else {
            return Err("orchestrator has no ai_tx".into());
        };
        let chat = self
            .chats
            .get(idx)
            .ok_or_else(|| "unknown chat".to_string())?;
        if !chat.hibernated {
            return Ok(());
        }
        let template_id = chat
            .worker_template
            .clone()
            .ok_or_else(|| "chat is not a worker".to_string())?;
        let session_id = chat
            .session_id
            .ok_or_else(|| "worker has no session id".to_string())?;
        let worker = self
            .config
            .worker(&template_id)
            .cloned()
            .ok_or_else(|| format!("worker template `{template_id}` no longer exists"))?;

        let tools: Vec<String> = worker.tools.clone().unwrap_or_else(|| {
            DEFAULT_ALLOWED_TOOLS
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        });

        let env = vec![
            (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
            (
                agent_ipc::ENV_AGENT_SOCKET.to_string(),
                self.socket_path.clone(),
            ),
        ];

        // `with_session_id` sets `started = true`, so the first turn
        // after resurrection runs `claude --resume <uuid>` and does
        // not re-prepend the worker preamble — that content already
        // lives in the claude session's history.
        let core: Box<dyn AICore> = Box::new(
            ClaudeCodeCore::with_session_id(session_id, String::new(), tools).with_env(env),
        );

        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);

        if let Some(chat) = self.chats.get_mut(idx) {
            chat.user_tx = Some(user_tx);
            chat.agent_tx = Some(agent_tx);
            chat.status = AgentStatus::Working;
            chat.hibernated = false;
        }

        tokio::spawn(run_core(idx, core, user_rx, agent_rx, ai_tx));
        Ok(())
    }

    fn handle_send_message(
        &mut self,
        sender_idx: usize,
        to: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let sender_id = sender_chat.display_name.clone();
        let sender_parent_idx = sender_chat.parent_idx;

        if to == sender_id {
            let _ = reply.send(Response::err("cannot send a message to yourself"));
            return;
        }

        // Resolve recipient by display name across all chats, so workers
        // (whose names are `<template>#<uuid>`) are reachable too.
        let Some(recipient_idx) = self.chats.iter().position(|c| c.display_name == to) else {
            let _ = reply.send(Response::err(format!("no agent named {to}")));
            return;
        };

        // Worker routing rule: a worker may only message its parent.
        if let Some(parent_idx) = sender_parent_idx {
            if recipient_idx != parent_idx {
                let _ = reply.send(Response::err(
                    "worker can only message its parent".to_string(),
                ));
                return;
            }
        } else {
            // Statically configured agent: apply the regular
            // `can_message` allowlist — but only when the recipient is
            // itself a configured agent. Workers spawned by this agent
            // are not in the allowlist; reaching them by display id is
            // always permitted, since the agent spawned them.
            let recipient_is_worker = self
                .chats
                .get(recipient_idx)
                .map(|c| c.parent_idx.is_some())
                .unwrap_or(false);
            if !recipient_is_worker && !self.config.can_send(&sender_id, &to) {
                let _ = reply.send(Response::err(format!(
                    "agent `{sender_id}` is not permitted to message `{to}`"
                )));
                return;
            }
        }

        // Auto-resurrect a hibernated recipient before delivering.
        let recipient_hibernated = self
            .chats
            .get(recipient_idx)
            .map(|c| c.hibernated)
            .unwrap_or(false);
        if recipient_hibernated {
            if let Err(e) = self.resurrect_worker(recipient_idx) {
                let _ = reply.send(Response::err(format!("failed to resume worker: {e}")));
                return;
            }
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

        // Recipient sees a Mail block naming the sender and carrying the
        // body. The sender's chat already has a Mail block from the Bash
        // tool-call intercept in `handle_ai_event`, so no echo is needed
        // there.
        if let Some(chat) = self.chats.get_mut(recipient_idx) {
            chat.push_block(ChatBlock::mail_received(sender_id.clone(), body.clone()));
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

    fn handle_task_add(
        &mut self,
        sender_idx: usize,
        title: String,
        description: Option<String>,
        assignee: Option<String>,
        blocked_by: Vec<Uuid>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let who = sender_chat.display_name.clone();

        if let Some(ref name) = assignee {
            if !self.chats.iter().any(|c| &c.display_name == name) {
                let _ = reply.send(Response::err(format!("no agent or worker named `{name}`")));
                return;
            }
        }

        let (task_id, title_clone, assignee_clone) = match self.tasks.add(
            title,
            description,
            assignee.clone(),
            who.clone(),
            blocked_by,
        ) {
            Ok(t) => (t.id, t.title.clone(), t.assignee.clone()),
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };

        if let Some(new_assignee) = assignee_clone {
            // Don't notify yourself on self-assignment during `add`.
            if new_assignee != who {
                self.notify_assignee(
                    &new_assignee,
                    task_id,
                    &title_clone,
                    &format!("assigned to you by {who}"),
                );
            }
        }

        let _ = reply.send(Response::ok(format!("task created: {task_id}")));
    }

    fn handle_task_list(
        &mut self,
        sender_idx: usize,
        assignee: Option<String>,
        status: Option<String>,
        mine: bool,
        unassigned: bool,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let who = sender_chat.display_name.clone();

        let parsed_status = match status.as_deref().map(TaskStatus::parse).transpose() {
            Ok(v) => v,
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };

        let filter = TaskFilter {
            assignee: if mine { Some(who) } else { assignee },
            status: parsed_status,
            unassigned_only: unassigned,
        };

        let rows = self.tasks.list(&filter);
        if rows.is_empty() {
            let _ = reply.send(Response::ok("(no tasks match)"));
            return;
        }
        let mut out = String::new();
        out.push_str(
            "id                                     status        assignee            deps       title\n",
        );
        for t in rows {
            let deps_col = if t.blocked_by.is_empty() {
                "-".to_string()
            } else if self.tasks.is_blocked(t) {
                format!("blocked({})", t.blocked_by.len())
            } else {
                format!("ready({})", t.blocked_by.len())
            };
            out.push_str(&format!(
                "{}  {:<12}  {:<18}  {:<9}  {}\n",
                t.id,
                t.status.as_str(),
                t.assignee.as_deref().unwrap_or("-"),
                deps_col,
                t.title
            ));
        }
        let _ = reply.send(Response::ok(out.trim_end().to_string()));
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_task_update(
        &mut self,
        sender_idx: usize,
        id: Uuid,
        title: Option<String>,
        description: Option<String>,
        status: Option<String>,
        assignee: Option<String>,
        unassign: bool,
        note: Option<String>,
        add_blocked_by: Vec<Uuid>,
        remove_blocked_by: Vec<Uuid>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let who = sender_chat.display_name.clone();

        let parsed_status = match status.as_deref().map(TaskStatus::parse).transpose() {
            Ok(v) => v,
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };

        let assignee_patch: Option<Option<String>> = if unassign {
            Some(None)
        } else if let Some(name) = assignee {
            if !self.chats.iter().any(|c| c.display_name == name) {
                let _ = reply.send(Response::err(format!("no agent or worker named `{name}`")));
                return;
            }
            Some(Some(name))
        } else {
            None
        };

        let patch = TaskPatch {
            title,
            description,
            status: parsed_status,
            assignee: assignee_patch,
            note,
            add_blocked_by,
            remove_blocked_by,
        };

        let outcome = match self.tasks.update(id, patch, &who) {
            Ok(o) => o,
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };

        if let Some(new_assignee) = outcome.reassigned_to.as_deref() {
            if new_assignee != who {
                if let Some(t) = self.tasks.get(id) {
                    let title_clone = t.title.clone();
                    self.notify_assignee(
                        new_assignee,
                        id,
                        &title_clone,
                        &format!("assigned to you by {who}"),
                    );
                }
            }
        }

        let _ = reply.send(Response::ok("task updated"));
    }

    fn handle_task_remove(
        &mut self,
        sender_idx: usize,
        id: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        match self.tasks.remove(id) {
            Ok(_) => {
                let _ = reply.send(Response::ok("task removed"));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_task_take(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let who = sender_chat.display_name.clone();

        let (task_id, title_clone) = match self.tasks.take(&who) {
            Ok(t) => (t.id, t.title.clone()),
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };

        // Drop a system block into the taker's own pane so their chat
        // history records the claim (handy when reviewing transcripts).
        self.notify_assignee(&who, task_id, &title_clone, "taken by you");

        let _ = reply.send(Response::ok(format!("took task {task_id}: {title_clone}")));
    }

    fn handle_task_note(
        &mut self,
        sender_idx: usize,
        id: Uuid,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let who = sender_chat.display_name.clone();

        match self.tasks.add_note(id, who, body) {
            Ok(_) => {
                let _ = reply.send(Response::ok("note added"));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_task_show(
        &mut self,
        sender_idx: usize,
        id: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        let Some(t) = self.tasks.get(id) else {
            let _ = reply.send(Response::err(format!("task {id} not found")));
            return;
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = reply.send(Response::ok(format_task_detail(t, &self.tasks, now)));
    }

    /// Push a passive `system_info` block into the assignee's chat so
    /// they see the new work next time they look at the pane. Does
    /// NOT resurrect hibernated workers — tasks are pull-driven, so
    /// waking a sleeping worker every time someone assigns a task
    /// would undo the point of hibernation.
    fn notify_assignee(
        &mut self,
        assignee_display: &str,
        task_id: Uuid,
        title: &str,
        context: &str,
    ) {
        let short = task_id.to_string();
        let short = &short[..8.min(short.len())];
        let msg = format!("Task {short} {context}: {title}");
        if let Some(chat) = self
            .chats
            .iter_mut()
            .find(|c| c.display_name == assignee_display)
        {
            chat.push_block(ChatBlock::system_info(msg));
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

        // Tab cycles the three visible panels — UNLESS a Tasks-view
        // modal (form / detail / prompt) is open, in which case Tab
        // belongs to that modal's field navigation and must reach
        // `handle_key_tasks`.
        let tasks_modal_open = self.active_view == ActiveView::Tasks
            && self.focus == Focus::Tasks
            && (self.tasks_screen.form.is_some()
                || self.tasks_screen.detail.is_some()
                || self.tasks_screen.prompt.is_some());
        if matches!(code, KeyCode::Tab) && !tasks_modal_open {
            let main = match self.active_view {
                ActiveView::Chat => Focus::Chat,
                ActiveView::Tasks => Focus::Tasks,
            };
            self.focus = match self.focus {
                f if f == main => Focus::Agents,
                Focus::Agents => Focus::Views,
                Focus::Views => main,
                // If focus is on a main-area variant that doesn't
                // match the current active_view (e.g. after a view
                // switch), snap back to the right one.
                _ => main,
            };
            return;
        }

        match self.focus {
            Focus::Agents => self.handle_key_agents(code, mods),
            Focus::Chat => self.handle_key_chat(code, mods),
            Focus::Views => self.handle_key_views(code, mods),
            Focus::Tasks => self.handle_key_tasks(code, mods),
        }
    }

    fn handle_key_agents(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                info!("quit requested");
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.select_next();
                self.return_to_chat_if_viewing_tasks();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.select_prev();
                self.return_to_chat_if_viewing_tasks();
            }
            KeyCode::Enter => {
                // Explicit "activate this agent" — useful when the
                // user Tab'd to the agents list from the Tasks view
                // and wants to jump back without moving selection.
                self.return_to_chat_if_viewing_tasks();
                self.focus = Focus::Chat;
            }
            _ => {}
        }
    }

    /// If the user is in the Tasks view and interacts with the
    /// agents list, swap the main area back to chat. The operator
    /// is clearly asking for an agent, so stop showing the task
    /// board.
    fn return_to_chat_if_viewing_tasks(&mut self) {
        if self.active_view == ActiveView::Tasks {
            self.active_view = ActiveView::Chat;
            // Close any open Tasks modals so they don't get stranded
            // off-screen.
            self.tasks_screen.close_modal();
        }
    }

    fn handle_key_views(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') => {
                self.should_quit = true;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_view + 1 < VIEW_ENTRIES.len() {
                    self.selected_view += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_view > 0 {
                    self.selected_view -= 1;
                }
            }
            KeyCode::Enter => {
                // Only "Tasks" today. Hard-coded dispatch for now
                // since there's exactly one entry; extending means
                // matching on `VIEW_ENTRIES[self.selected_view]`.
                if VIEW_ENTRIES.get(self.selected_view).copied() == Some("Tasks") {
                    self.active_view = ActiveView::Tasks;
                    self.focus = Focus::Tasks;
                    // Snap the tasks-screen cursor to the first
                    // selectable row (in case the store state
                    // changed since it was last opened).
                    let rows = TasksScreen::rows(&self.tasks);
                    self.tasks_screen.reclamp(&rows);
                }
            }
            _ => {}
        }
    }

    fn handle_key_tasks(&mut self, code: KeyCode, _mods: KeyModifiers) {
        // Clear transient status messages the moment the user types.
        self.tasks_screen.status_msg = None;

        // Modal form takes all input.
        if self.tasks_screen.form.is_some() {
            self.handle_task_form_key(code);
            return;
        }
        // Detail modal: any key closes it.
        if self.tasks_screen.detail.is_some() {
            if matches!(code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                self.tasks_screen.close_modal();
            }
            return;
        }
        // One-line prompt (note / delete-confirm) takes all input.
        if self.tasks_screen.prompt.is_some() {
            self.handle_task_prompt_key(code);
            return;
        }

        let rows = TasksScreen::rows(&self.tasks);
        match code {
            KeyCode::Char('j') | KeyCode::Down => self.tasks_screen.move_down(&rows),
            KeyCode::Char('k') | KeyCode::Up => self.tasks_screen.move_up(&rows),
            KeyCode::Char('a') => {
                self.tasks_screen.open_add();
                self.inject_assignee_options();
            }
            KeyCode::Char('e') => {
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    if let Some(t) = self.tasks.get(id) {
                        self.tasks_screen.open_edit(t);
                        self.inject_assignee_options();
                    }
                }
            }
            KeyCode::Char('d') => {
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    self.tasks_screen.prompt = Some(TasksPrompt::ConfirmDelete {
                        task_id: id,
                        buffer: String::new(),
                    });
                }
            }
            KeyCode::Char('t') => {
                match self.tasks.take(OPERATOR) {
                    Ok(t) => {
                        self.tasks_screen.status_msg = Some(format!("took: {}", t.title));
                    }
                    Err(e) => {
                        self.tasks_screen.status_msg = Some(format!("error: {e}"));
                    }
                }
                let rows = TasksScreen::rows(&self.tasks);
                self.tasks_screen.reclamp(&rows);
            }
            KeyCode::Char('n') => {
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    self.tasks_screen.prompt = Some(TasksPrompt::Note {
                        task_id: id,
                        body: String::new(),
                    });
                }
            }
            KeyCode::Char('s') => {
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    if let Err(e) = self.tasks_screen.cycle_status(&mut self.tasks, id) {
                        self.tasks_screen.status_msg = Some(format!("error: {e}"));
                    }
                    let rows = TasksScreen::rows(&self.tasks);
                    self.tasks_screen.reclamp(&rows);
                }
            }
            KeyCode::Enter => {
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    self.tasks_screen.open_detail(id);
                }
            }
            KeyCode::Esc => {
                // Nothing open — treat Esc as "go back to chat".
                self.active_view = ActiveView::Chat;
                self.focus = Focus::Chat;
            }
            _ => {}
        }
    }

    fn handle_task_form_key(&mut self, code: KeyCode) {
        let Some(form) = self.tasks_screen.form.as_mut() else {
            return;
        };

        // Picker popup takes all keyboard input while open.
        if form.picker.is_some() {
            match code {
                KeyCode::Esc => form.close_picker(),
                KeyCode::Down | KeyCode::Char('j') => form.picker_move(1),
                KeyCode::Up | KeyCode::Char('k') => form.picker_move(-1),
                KeyCode::Enter => form.commit_picker(),
                _ => {}
            }
            return;
        }

        let on_dropdown = TaskForm::is_dropdown(form.focused_field);
        match code {
            KeyCode::Esc => self.tasks_screen.close_modal(),
            // Tab/Down → next field; Shift-Tab/Up → previous.
            KeyCode::Tab | KeyCode::Down => form.focus_next(),
            KeyCode::BackTab | KeyCode::Up => form.focus_prev(),
            // Enter on a dropdown opens the picker; on text fields
            // it submits the form (preserving the "hit enter to
            // save" muscle memory).
            KeyCode::Enter => {
                if on_dropdown {
                    form.open_picker();
                } else {
                    match self.tasks_screen.submit_form(&mut self.tasks) {
                        Ok(()) => {
                            let rows = TasksScreen::rows(&self.tasks);
                            self.tasks_screen.reclamp(&rows);
                        }
                        Err(_e) => { /* error is surfaced on form.error */ }
                    }
                }
            }
            // Character input is ignored on dropdown fields so
            // stray keystrokes can't corrupt the selection.
            KeyCode::Backspace if !on_dropdown => {
                form.field_mut(form.focused_field).pop();
            }
            KeyCode::Char(c) if !on_dropdown => {
                form.field_mut(form.focused_field).push(c);
            }
            _ => {}
        }
    }

    /// Populate the assignee-dropdown options on the currently-open
    /// form from the team's visible chats. Called immediately after
    /// `open_add` / `open_edit`.
    fn inject_assignee_options(&mut self) {
        let mut options: Vec<String> = vec![String::new()];
        for c in &self.chats {
            // Hibernated workers are hidden from the agent list, but
            // they can still be valid task assignees — include them
            // so a hand-off mid-hibernation stays possible.
            options.push(c.display_name.clone());
        }
        if let Some(form) = self.tasks_screen.form.as_mut() {
            form.set_assignee_options(options);
        }
    }

    fn handle_task_prompt_key(&mut self, code: KeyCode) {
        let prompt = self.tasks_screen.prompt.as_mut();
        let Some(prompt) = prompt else { return };
        match prompt {
            TasksPrompt::Note { task_id, body } => match code {
                KeyCode::Esc => self.tasks_screen.close_modal(),
                KeyCode::Enter => {
                    if !body.trim().is_empty() {
                        let id = *task_id;
                        let body = std::mem::take(body);
                        if let Err(e) = self.tasks.add_note(id, OPERATOR.to_string(), body) {
                            self.tasks_screen.status_msg = Some(format!("error: {e}"));
                        } else {
                            self.tasks_screen.status_msg = Some("note added".into());
                        }
                    }
                    self.tasks_screen.close_modal();
                }
                KeyCode::Backspace => {
                    body.pop();
                }
                KeyCode::Char(c) => body.push(c),
                _ => {}
            },
            TasksPrompt::ConfirmDelete { task_id, buffer } => match code {
                KeyCode::Esc => self.tasks_screen.close_modal(),
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let id = *task_id;
                    match self.tasks.remove(id) {
                        Ok(t) => {
                            self.tasks_screen.status_msg = Some(format!("removed: {}", t.title));
                            self.tasks_screen.close_modal();
                            let rows = TasksScreen::rows(&self.tasks);
                            self.tasks_screen.reclamp(&rows);
                        }
                        Err(e) => {
                            self.tasks_screen.status_msg = Some(format!("error: {e}"));
                            self.tasks_screen.close_modal();
                        }
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.tasks_screen.close_modal();
                }
                KeyCode::Char(c) => buffer.push(c),
                KeyCode::Backspace => {
                    buffer.pop();
                }
                _ => {}
            },
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

                // 2. Views list: clicking any entry both focuses
                //    the panel and activates that view (same as
                //    Enter on the keyboard).
                if let Some(rect) = self.layout.views_list_inner {
                    if rect_contains(rect, pos) {
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        if row_in_list < VIEW_ENTRIES.len() {
                            self.selected_view = row_in_list;
                            // Dispatch the selected view.
                            match VIEW_ENTRIES[row_in_list] {
                                "Tasks" => {
                                    self.active_view = ActiveView::Tasks;
                                    self.focus = Focus::Tasks;
                                    let rows = TasksScreen::rows(&self.tasks);
                                    self.tasks_screen.reclamp(&rows);
                                }
                                _ => {
                                    self.focus = Focus::Views;
                                }
                            }
                        } else {
                            self.focus = Focus::Views;
                        }
                        return;
                    }
                }

                if let Some(rect) = self.layout.agent_list_inner {
                    if rect_contains(rect, pos) {
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        let order = self.visible_order();
                        if let Some((chat_idx, _)) = order.get(row_in_list).copied() {
                            self.selected = chat_idx;
                            // Picking an agent always returns to the
                            // chat view — matches the j/k keyboard
                            // behaviour.
                            self.return_to_chat_if_viewing_tasks();
                            self.focus = Focus::Chat;
                        } else {
                            self.focus = Focus::Agents;
                        }
                        return;
                    }
                }
                if let Some(rect) = self.layout.chat_history_inner {
                    if rect_contains(rect, pos) {
                        // Main area: Focus depends on which view is
                        // drawn there.
                        self.focus = match self.active_view {
                            ActiveView::Chat => Focus::Chat,
                            ActiveView::Tasks => Focus::Tasks,
                        };
                        // In Tasks view, clicking a row selects it.
                        if self.active_view == ActiveView::Tasks {
                            let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                            let rows = TasksScreen::rows(&self.tasks);
                            if let Some(super::tasks_view::TaskRow::Task(_)) = rows.get(row_in_list)
                            {
                                self.tasks_screen.selected_row = row_in_list;
                            }
                        }
                        return;
                    }
                }
                if let Some(rect) = self.layout.chat_input_inner {
                    if rect_contains(rect, pos) {
                        self.focus = match self.active_view {
                            ActiveView::Chat => Focus::Chat,
                            ActiveView::Tasks => Focus::Tasks,
                        };
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

/// Render the gap between `then_secs` and `now_secs` as a compact
/// relative string: `12s ago`, `5m ago`, `3h ago`, `2d ago`, or
/// `just now` when the gap is zero. Both args are unix seconds.
pub(crate) fn format_age(now_secs: u64, then_secs: u64) -> String {
    let delta = now_secs.saturating_sub(then_secs);
    if delta == 0 {
        return "just now".into();
    }
    if delta < 60 {
        return format!("{delta}s ago");
    }
    if delta < 3600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < 86_400 {
        return format!("{}h ago", delta / 3600);
    }
    format!("{}d ago", delta / 86_400)
}

/// Build the multi-line detail rendering for a task — used by both
/// the `aciddog task show` IPC handler and the TUI's detail modal so
/// the two surfaces stay visually identical.
pub(crate) fn format_task_detail(t: &Task, store: &TaskStore, now: u64) -> String {
    let mut out = String::new();
    out.push_str(&format!("Task: {}\n", t.id));
    out.push_str(&format!("Title: {}\n", t.title));
    out.push_str(&format!("Status: {}\n", t.status.as_str()));
    out.push_str(&format!(
        "Assignee: {}\n",
        t.assignee.as_deref().unwrap_or("-")
    ));
    out.push_str(&format!("Created by: {}\n", t.created_by));
    out.push_str(&format!(
        "Created: {} (unix {})\n",
        format_age(now, t.created_at_secs),
        t.created_at_secs
    ));
    out.push_str(&format!(
        "Updated: {} (unix {})\n",
        format_age(now, t.updated_at_secs),
        t.updated_at_secs
    ));

    out.push_str("\nDescription:\n");
    match &t.description {
        Some(d) if !d.is_empty() => {
            for line in d.lines() {
                out.push_str("  ");
                out.push_str(line);
                out.push('\n');
            }
        }
        _ => out.push_str("  (none)\n"),
    }

    out.push_str("\nDependencies:\n");
    if t.blocked_by.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for dep in &t.blocked_by {
            match store.get(*dep) {
                Some(d) => out.push_str(&format!(
                    "  {}  {:<12}  {}\n",
                    d.id,
                    d.status.as_str(),
                    d.title
                )),
                None => out.push_str(&format!("  {dep}  (missing)\n")),
            }
        }
        if store.is_blocked(t) {
            out.push_str("  -> blocked: some deps not yet done\n");
        } else {
            out.push_str("  -> ready: all deps done\n");
        }
    }

    out.push_str("\nNotes:\n");
    if t.notes.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for n in &t.notes {
            out.push_str(&format!(
                "  [{} · {}] {}\n",
                format_age(now, n.timestamp_secs),
                n.author,
                n.body
            ));
        }
    }
    out.trim_end().to_string()
}

pub fn rect_contains(rect: Rect, (x, y): (u16, u16)) -> bool {
    x >= rect.x
        && x < rect.x.saturating_add(rect.width)
        && y >= rect.y
        && y < rect.y.saturating_add(rect.height)
}

/// If `cmd` is a shell invocation of `<binary> send --to <id> --body <text>`,
/// return the extracted `(to, body)`. Returns `None` otherwise — including
/// for invocations like `bash -c "..."` or composite pipelines that don't
/// start with the aciddog binary.
///
/// Parsing reuses `AgentArgs` (the same clap parser the agent-mode binary
/// uses) so flag semantics stay in one place.
fn try_parse_aciddog_send(cmd: &str, binary: &str) -> Option<(String, String)> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Send { to, body } => Some((to, body)),
        _ => None,
    }
}

fn try_parse_aciddog_worker_create(cmd: &str, binary: &str) -> Option<(String, String)> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Worker {
            cmd: crate::cli::WorkerCommand::Create { template, task },
        } => Some((template, task)),
        _ => None,
    }
}

fn try_parse_aciddog_worker_complete(cmd: &str, binary: &str) -> Option<String> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Worker {
            cmd: crate::cli::WorkerCommand::Complete { body },
        } => Some(body),
        _ => None,
    }
}

fn try_parse_aciddog(cmd: &str, binary: &str) -> Option<AgentCommand> {
    if binary.is_empty() {
        return None;
    }
    let tokens = shlex::split(cmd)?;
    let mut iter = tokens.into_iter();
    let first = iter.next()?;
    if first != binary {
        return None;
    }
    let argv: Vec<String> = std::iter::once("aciddog".to_string()).chain(iter).collect();
    AgentArgs::try_parse_from(argv).ok().map(|p| p.cmd)
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

    // Load the disk-backed task store from `.aciddog/tasks.json` in the
    // cwd so tasks survive restarts. On I/O/parse error, warn and fall
    // back to an ephemeral store so the TUI still boots.
    let tasks_path = std::path::PathBuf::from(".aciddog").join("tasks.json");
    match TaskStore::load_or_init(tasks_path.clone()) {
        Ok(store) => {
            info!(path = %tasks_path.display(), count = store.len(), "loaded task store");
            app.tasks = store;
        }
        Err(e) => {
            warn!(error = %e, path = %tasks_path.display(), "failed to load task store; using ephemeral store");
        }
    }

    // Bind the agent IPC socket for this team before spawning cores, so its
    // path can be injected into each claude child's environment.
    let team_id = app.config.team.id.clone();
    let mut socket = agent_ipc::server::bind(&team_id)
        .await
        .with_context(|| format!("binding agent IPC socket for team {team_id}"))?;
    info!(path = %socket.path.display(), "agent IPC socket bound");
    let socket_path_str = socket.path.display().to_string();
    app.socket_path = socket_path_str.clone();

    // Resolve our own binary path so agents can invoke aciddog without it
    // being on PATH (cargo run, dev installs, nix store, etc).
    let aciddog_binary = std::env::current_exe()
        .context("resolving current aciddog binary path for agent preamble")?;
    let aciddog_binary_str = aciddog_binary.to_string_lossy().into_owned();
    info!(binary = %aciddog_binary_str, "agent CLI binary");
    app.aciddog_binary = aciddog_binary_str.clone();

    // One core per agent, each with its own session, user channel, and
    // agent-to-agent channel. User channel is unbounded (the TUI key
    // handler must never block or drop); agent channel is bounded so
    // the socket side has backpressure.
    let (ai_tx, ai_rx) = mpsc::channel::<(usize, AIEvent)>(256);
    app.ai_tx = Some(ai_tx.clone());
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

        // Worker templates this agent can spawn. `None` in config means
        // the agent has no worker-create capability at all.
        let creatable: Vec<&str> = agent
            .can_create_workers
            .as_deref()
            .map(|list| list.iter().map(String::as_str).collect())
            .unwrap_or_default();

        let instructions = format!(
            "{}\n\n{}",
            agent_ipc::preamble::cli_preamble(&aciddog_binary_str, &contactable, &creatable),
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
    // `app.ai_tx` retains a sender clone so runtime worker spawns can
    // route their stream into the same loop. That clone is dropped
    // either when a new worker's `run_core` captures it, or when the
    // event loop exits and `App` is dropped.

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
                    can_create_workers: None,
                },
                Agent {
                    id: "b".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                },
                Agent {
                    id: "c".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                },
            ],
            workers: vec![],
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
    fn tab_cycles_focus_through_three_panels_in_chat_view() {
        let mut app = App::new(sample_config());
        assert_eq!(app.focus, Focus::Chat);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Agents);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Views);
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

    // --------- Bash → Mail rewrite ----------

    #[test]
    fn try_parse_aciddog_send_extracts_to_and_body() {
        let cmd = r#"/opt/aciddog send --to coder --body "hi there""#;
        let (to, body) = try_parse_aciddog_send(cmd, "/opt/aciddog").unwrap();
        assert_eq!(to, "coder");
        assert_eq!(body, "hi there");
    }

    #[test]
    fn try_parse_aciddog_send_rejects_non_aciddog_commands() {
        assert!(try_parse_aciddog_send("ls -la", "/opt/aciddog").is_none());
        assert!(try_parse_aciddog_send("/opt/aciddog test", "/opt/aciddog").is_none());
        // Shell pipelines / bash -c wrappers don't match; we intentionally
        // only intercept direct invocations.
        assert!(try_parse_aciddog_send(
            r#"bash -c "/opt/aciddog send --to x --body y""#,
            "/opt/aciddog"
        )
        .is_none());
    }

    #[test]
    fn try_parse_aciddog_send_requires_matching_binary_path() {
        let cmd = "/usr/local/bin/aciddog send --to coder --body hi";
        assert!(try_parse_aciddog_send(cmd, "/opt/aciddog").is_none());
    }

    #[test]
    fn try_parse_aciddog_send_is_disabled_when_binary_empty() {
        // Test harness (App::new) leaves aciddog_binary blank until run()
        // populates it — a blank binary must never match a real command.
        assert!(try_parse_aciddog_send("aciddog send --to coder --body hi", "").is_none());
    }

    #[test]
    fn bash_tool_use_for_aciddog_send_becomes_mail_sent() {
        let mut app = App::new(sample_config());
        app.aciddog_binary = "/opt/aciddog".into();
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({
                    "command": r#"/opt/aciddog send --to b --body "hi friend""#
                }),
                raw: serde_json::Value::Null,
            },
        );
        match &app.chats[0].blocks.last().unwrap() {
            ChatBlock::Mail {
                direction,
                counterpart,
                body,
                pending,
                ..
            } => {
                assert_eq!(*direction, super::super::blocks::MailDirection::Sent);
                assert_eq!(counterpart, "b");
                assert_eq!(body, "hi friend");
                assert!(*pending);
            }
            other => panic!("expected Mail, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_result_for_aciddog_send_clears_pending_on_mail() {
        let mut app = App::new(sample_config());
        app.aciddog_binary = "/opt/aciddog".into();
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({"command": "/opt/aciddog send --to b --body hi"}),
                raw: serde_json::Value::Null,
            },
        );
        app.handle_ai_event(
            0,
            AIEvent::ToolResult {
                tool_use_id: "t1".into(),
                content: serde_json::Value::String("queued".into()),
                is_error: false,
                raw: serde_json::Value::Null,
            },
        );
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::Mail { pending, .. } => assert!(!*pending),
            other => panic!("expected Mail, got {other:?}"),
        }
    }

    #[test]
    fn non_aciddog_bash_still_becomes_bash_tool_block() {
        let mut app = App::new(sample_config());
        app.aciddog_binary = "/opt/aciddog".into();
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({"command": "ls -la"}),
                raw: serde_json::Value::Null,
            },
        );
        assert!(matches!(
            app.chats[0].blocks.last().unwrap(),
            ChatBlock::BashTool { .. }
        ));
    }

    #[tokio::test]
    async fn send_message_pushes_mail_received_on_recipient() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "b".into(),
                    body: "hello b".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        // Recipient chat (agent b, index 1) should end with a Mail(Received).
        match app.chats[1].blocks.last().unwrap() {
            ChatBlock::Mail {
                direction,
                counterpart,
                body,
                pending,
                ..
            } => {
                assert_eq!(*direction, super::super::blocks::MailDirection::Received);
                assert_eq!(counterpart, "a");
                assert_eq!(body, "hello b");
                assert!(!*pending);
            }
            other => panic!("expected Mail(Received), got {other:?}"),
        }
    }

    #[tokio::test]
    async fn send_message_does_not_echo_into_sender_chat() {
        // The sender's chat shows a Mail(Sent) block from the Bash tool-use
        // intercept elsewhere; the socket handler must not also push a
        // system_info or second Mail block into the sender's chat.
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();
        let sender_blocks_before = app.chats[0].blocks.len();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "b".into(),
                    body: "hi".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        assert_eq!(
            app.chats[0].blocks.len(),
            sender_blocks_before,
            "socket handler must not push extra blocks into sender chat"
        );
    }

    // --------- Workers ----------

    use crate::config::Worker;

    fn worker_config() -> AciddogConfig {
        AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
            },
            agents: vec![
                Agent {
                    id: "coder".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: Some(vec!["researcher".into()]),
                },
                Agent {
                    id: "planner".into(),
                    role: "r".into(),
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                },
            ],
            workers: vec![
                Worker {
                    id: "researcher".into(),
                    role: "investigator".into(),
                    model: "m".into(),
                    instructions: "look stuff up".into(),
                    tools: None,
                    can_create_workers: None,
                },
                Worker {
                    id: "fetcher".into(),
                    role: "fetcher".into(),
                    model: "m".into(),
                    instructions: "fetch".into(),
                    tools: None,
                    can_create_workers: None,
                },
            ],
        }
    }

    /// Prime an App as if `run()` had already initialized channels /
    /// session ids / the AI event sender, so worker-creation tests can
    /// invoke `handle_agent_socket_event` without racing with the real
    /// runtime setup.
    fn prime_worker_app(cfg: AciddogConfig) -> (App, mpsc::Receiver<(usize, AIEvent)>) {
        let mut app = App::new(cfg);
        app.aciddog_binary = "/opt/aciddog".into();
        app.socket_path = "/tmp/fake.sock".into();
        let (ai_tx, ai_rx) = mpsc::channel::<(usize, AIEvent)>(32);
        app.ai_tx = Some(ai_tx);
        for (i, agent) in app.config.agents.clone().iter().enumerate() {
            let session_id = Uuid::new_v4();
            app.chats[i].session_id = Some(session_id);
            app.chats[i].display_name = agent.id.clone();
            app.agent_ids.insert(session_id, agent.id.clone());
            let (atx, _arx) = mpsc::channel::<String>(16);
            app.chats[i].agent_tx = Some(atx);
        }
        (app, ai_rx)
    }

    #[tokio::test]
    async fn worker_create_rejects_unknown_template() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        let sender_uuid = app.chats[0].session_id.unwrap();
        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::WorkerCreate {
                    template: "ghost".into(),
                    task: "anything".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        let err = resp.error.unwrap();
        assert!(
            err.contains("not permitted") || err.contains("no worker template"),
            "got: {err}"
        );
    }

    #[tokio::test]
    async fn worker_create_rejects_when_sender_lacks_permission() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        // planner is chat index 1 and has no can_create_workers.
        let planner_uuid = app.chats[1].session_id.unwrap();
        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: planner_uuid,
                cmd: RequestCmd::WorkerCreate {
                    template: "researcher".into(),
                    task: "t".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("not permitted"));
    }

    #[tokio::test]
    async fn worker_create_registers_new_chat_with_parent_idx_and_template() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        let coder_uuid = app.chats[0].session_id.unwrap();
        let chats_before = app.chats.len();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: coder_uuid,
                cmd: RequestCmd::WorkerCreate {
                    template: "researcher".into(),
                    task: "find FOO".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(resp.ok, "worker create failed: {resp:?}");
        assert_eq!(app.chats.len(), chats_before + 1);

        let worker = app.chats.last().unwrap();
        assert_eq!(worker.parent_idx, Some(0));
        assert_eq!(worker.worker_template.as_deref(), Some("researcher"));
        assert!(
            worker.display_name.starts_with("researcher#"),
            "unexpected display name: {}",
            worker.display_name
        );
        assert_eq!(worker.status, AgentStatus::Working);
        assert!(!worker.hibernated);
        // Parent should have a WorkerSpawn block surfacing the spawn.
        let parent_has_spawn_block = app.chats[0]
            .blocks
            .iter()
            .any(|b| matches!(b, ChatBlock::WorkerSpawn { .. }));
        assert!(parent_has_spawn_block, "missing WorkerSpawn on parent");
    }

    #[tokio::test]
    async fn worker_send_to_non_parent_is_rejected() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        // Manually register a fake worker under coder (index 0) so we
        // can drive its send path without running claude.
        let session_id = Uuid::new_v4();
        let worker_chat = AgentChat {
            display_name: "researcher#deadbeef".into(),
            parent_idx: Some(0),
            worker_template: Some("researcher".into()),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            agent_tx: Some(mpsc::channel::<String>(1).0),
            ..Default::default()
        };
        app.chats.push(worker_chat);
        app.agent_ids
            .insert(session_id, "researcher#deadbeef".into());

        // Worker tries to message `planner` — not its parent.
        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: session_id,
                cmd: RequestCmd::SendMessage {
                    to: "planner".into(),
                    body: "hi".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("can only message its parent"));
    }

    #[tokio::test]
    async fn worker_complete_delivers_body_to_parent_and_hibernates() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        // Replace the coder's agent_tx with one we control so we can
        // verify the delivered payload.
        let (parent_tx, mut parent_rx) = mpsc::channel::<String>(4);
        app.chats[0].agent_tx = Some(parent_tx);

        // Register a worker parented to coder.
        let session_id = Uuid::new_v4();
        let (user_tx, _user_rx) = mpsc::unbounded_channel::<String>();
        let (worker_agent_tx, _worker_agent_rx) = mpsc::channel::<String>(4);
        let worker_chat = AgentChat {
            display_name: "researcher#abcd1234".into(),
            parent_idx: Some(0),
            worker_template: Some("researcher".into()),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            user_tx: Some(user_tx),
            agent_tx: Some(worker_agent_tx),
            ..Default::default()
        };
        let worker_idx = app.chats.len();
        app.chats.push(worker_chat);
        app.agent_ids
            .insert(session_id, "researcher#abcd1234".into());

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: session_id,
                cmd: RequestCmd::WorkerComplete {
                    body: "findings".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        // Parent received the wrapped final report.
        let payload = parent_rx.recv().await.expect("parent got message");
        assert_eq!(
            payload,
            r#"<Message from="researcher#abcd1234">findings</Message>"#
        );

        // Worker hibernated: channels dropped, status flipped.
        assert!(app.chats[worker_idx].hibernated);
        assert!(app.chats[worker_idx].user_tx.is_none());
        assert!(app.chats[worker_idx].agent_tx.is_none());
        assert_eq!(app.chats[worker_idx].status, AgentStatus::Hibernated);

        // Parent has a Mail(Received) block; worker has a WorkerComplete.
        let parent_last = app.chats[0].blocks.last().unwrap();
        assert!(matches!(
            parent_last,
            ChatBlock::Mail {
                direction: crate::tui::blocks::MailDirection::Received,
                ..
            }
        ));
        let worker_last = app.chats[worker_idx].blocks.last().unwrap();
        assert!(matches!(worker_last, ChatBlock::WorkerComplete { .. }));
    }

    #[tokio::test]
    async fn worker_complete_rejects_when_sender_is_not_a_worker() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        let coder_uuid = app.chats[0].session_id.unwrap();
        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: coder_uuid,
                cmd: RequestCmd::WorkerComplete {
                    body: "nope".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("running worker"));
    }

    #[test]
    fn try_parse_aciddog_worker_create_extracts_template_and_task() {
        let cmd = r#"/opt/aciddog worker create --template researcher --task "find FOO""#;
        let (t, k) = try_parse_aciddog_worker_create(cmd, "/opt/aciddog").unwrap();
        assert_eq!(t, "researcher");
        assert_eq!(k, "find FOO");
    }

    #[test]
    fn try_parse_aciddog_worker_complete_extracts_body() {
        let cmd = r#"/opt/aciddog worker complete --body "all done""#;
        let body = try_parse_aciddog_worker_complete(cmd, "/opt/aciddog").unwrap();
        assert_eq!(body, "all done");
    }

    #[test]
    fn bash_tool_use_for_worker_create_becomes_spawn_block() {
        let mut app = App::new(worker_config());
        app.aciddog_binary = "/opt/aciddog".into();
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({
                    "command":
                        r#"/opt/aciddog worker create --template researcher --task "find FOO""#,
                }),
                raw: serde_json::Value::Null,
            },
        );
        let last = app.chats[0].blocks.last().unwrap();
        match last {
            ChatBlock::WorkerSpawn { template, task, .. } => {
                assert_eq!(template, "researcher");
                assert_eq!(task, "find FOO");
            }
            other => panic!("expected WorkerSpawn, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn worker_complete_hides_worker_from_visible_order_and_snaps_selection() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        // Register a worker parented to coder (idx 0).
        let session_id = Uuid::new_v4();
        let (worker_agent_tx, _worker_agent_rx) = mpsc::channel::<String>(1);
        let worker_chat = AgentChat {
            display_name: "researcher#11112222".into(),
            parent_idx: Some(0),
            worker_template: Some("researcher".into()),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            agent_tx: Some(worker_agent_tx),
            user_tx: Some(mpsc::unbounded_channel::<String>().0),
            ..Default::default()
        };
        let worker_idx = app.chats.len();
        app.chats.push(worker_chat);
        app.agent_ids
            .insert(session_id, "researcher#11112222".into());

        // Pre-check: worker is visible while running.
        let order_before = app.visible_order();
        assert!(order_before.iter().any(|(i, _)| *i == worker_idx));

        // User is viewing the worker's pane when it completes.
        app.selected = worker_idx;

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: session_id,
                cmd: RequestCmd::WorkerComplete {
                    body: "done".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        // Worker is gone from the visible order...
        let order_after = app.visible_order();
        assert!(
            !order_after.iter().any(|(i, _)| *i == worker_idx),
            "hibernated worker should disappear from agent list",
        );
        // ...but still present in `chats` so resurrection can find it.
        assert!(app.chats[worker_idx].hibernated);
        assert_eq!(
            app.chats[worker_idx].session_id,
            Some(session_id),
            "session id must survive hibernation for resurrection"
        );
        // Selection snapped to the parent.
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn visible_order_nests_workers_under_their_parent() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        // Coder is chat 0; planner is chat 1.
        // Register a worker under coder (0) and a sub-worker under it.
        let w1_id = Uuid::new_v4();
        app.chats.push(AgentChat {
            display_name: "researcher#aaa".into(),
            parent_idx: Some(0),
            worker_template: Some("researcher".into()),
            session_id: Some(w1_id),
            status: AgentStatus::Working,
            ..Default::default()
        });
        let w1_idx = app.chats.len() - 1;
        app.chats.push(AgentChat {
            display_name: "fetcher#bbb".into(),
            parent_idx: Some(w1_idx),
            worker_template: Some("fetcher".into()),
            session_id: Some(Uuid::new_v4()),
            status: AgentStatus::Working,
            ..Default::default()
        });
        let w2_idx = app.chats.len() - 1;
        // And a worker directly under planner (1).
        app.chats.push(AgentChat {
            display_name: "jokester#ccc".into(),
            parent_idx: Some(1),
            worker_template: Some("jokester".into()),
            session_id: Some(Uuid::new_v4()),
            status: AgentStatus::Working,
            ..Default::default()
        });
        let w3_idx = app.chats.len() - 1;

        let order: Vec<(usize, usize)> = app.visible_order();
        assert_eq!(
            order,
            vec![
                (0, 0),      // coder
                (w1_idx, 1), // researcher under coder
                (w2_idx, 2), // fetcher under researcher
                (1, 0),      // planner
                (w3_idx, 1), // jokester under planner
            ]
        );
    }

    #[test]
    fn bash_tool_use_for_worker_complete_becomes_complete_block() {
        let mut app = App::new(worker_config());
        app.aciddog_binary = "/opt/aciddog".into();
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({
                    "command": r#"/opt/aciddog worker complete --body "all done""#,
                }),
                raw: serde_json::Value::Null,
            },
        );
        let last = app.chats[0].blocks.last().unwrap();
        match last {
            ChatBlock::WorkerComplete { body, .. } => {
                assert_eq!(body, "all done");
            }
            other => panic!("expected WorkerComplete, got {other:?}"),
        }
    }

    // --------- Task board ----------

    /// Parse a `"task created: <uuid>"` reply into the Uuid, else fail the test.
    fn parse_task_created(msg: &str) -> Uuid {
        let id_str = msg
            .strip_prefix("task created: ")
            .expect("task created reply");
        Uuid::parse_str(id_str).expect("valid uuid in reply")
    }

    #[tokio::test]
    async fn task_add_creates_open_task_when_unassigned() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::TaskAdd {
                    title: "fix bug".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok, "error: {:?}", resp.error);
        let id = parse_task_created(resp.message.as_deref().unwrap());
        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.title, "fix bug");
        assert_eq!(t.status, crate::tasks::TaskStatus::Open);
        assert!(t.assignee.is_none());
    }

    #[tokio::test]
    async fn task_add_with_assignee_notifies_recipient_pane() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let b_blocks_before = app.chats[1].blocks.len();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::TaskAdd {
                    title: "look at logs".into(),
                    description: None,
                    assignee: Some("b".into()),
                    blocked_by: vec![],
                },
            },
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);
        let b_blocks_after = app.chats[1].blocks.len();
        assert_eq!(
            b_blocks_after,
            b_blocks_before + 1,
            "recipient should have a new system block"
        );
        match app.chats[1].blocks.last().unwrap() {
            ChatBlock::System { text, .. } => {
                assert!(text.contains("assigned to you by a"), "got: {text}");
                assert!(text.contains("look at logs"), "got: {text}");
            }
            other => panic!("expected System block, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn task_add_rejects_unknown_assignee() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::TaskAdd {
                    title: "t".into(),
                    description: None,
                    assignee: Some("ghost".into()),
                    blocked_by: vec![],
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("ghost"));
    }

    #[tokio::test]
    async fn task_take_claims_oldest_free_task_and_sets_in_progress() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();
        let b = app.chats[1].session_id.unwrap();

        // `a` creates an unassigned task.
        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "work".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        // `b` takes it.
        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: b,
                cmd: RequestCmd::TaskTake,
            },
            reply: tx2,
        });
        let resp = rx2.await.unwrap();
        assert!(resp.ok, "error: {:?}", resp.error);

        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.assignee.as_deref(), Some("b"));
        assert_eq!(t.status, crate::tasks::TaskStatus::InProgress);
        assert_eq!(t.notes.len(), 1);
        assert!(t.notes[0].body.contains("taken by b"));
    }

    #[tokio::test]
    async fn task_take_errors_when_no_free_tasks() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskTake,
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("no unassigned"));
    }

    #[tokio::test]
    async fn task_update_reassign_with_note_notifies_new_assignee() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();
        let b = app.chats[1].session_id.unwrap();

        // a adds and takes the task.
        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "ship feature".into(),
                    description: None,
                    assignee: Some("a".into()),
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());
        let a_blocks_before_handoff = app.chats[0].blocks.len();

        // a reassigns to b with a note (simulating handoff to tester).
        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskUpdate {
                    id,
                    title: None,
                    description: None,
                    status: None,
                    assignee: Some("b".into()),
                    unassign: false,
                    note: Some("ready for test, see PR #1".into()),
                    add_blocked_by: vec![],
                    remove_blocked_by: vec![],
                },
            },
            reply: tx2,
        });
        assert!(rx2.await.unwrap().ok);

        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.assignee.as_deref(), Some("b"));
        assert_eq!(t.notes.len(), 1);
        assert_eq!(t.notes[0].body, "ready for test, see PR #1");

        // b's pane got a system block.
        match app.chats[1].blocks.last().unwrap() {
            ChatBlock::System { text, .. } => {
                assert!(text.contains("assigned to you by a"), "got: {text}");
            }
            other => panic!("expected System block, got {other:?}"),
        }
        // a's pane is unchanged (self-reassignment suppressed, and b
        // didn't notify a).
        assert_eq!(app.chats[0].blocks.len(), a_blocks_before_handoff);

        // b hands it back with a bug report.
        let (tx3, rx3) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: b,
                cmd: RequestCmd::TaskUpdate {
                    id,
                    title: None,
                    description: None,
                    status: None,
                    assignee: Some("a".into()),
                    unassign: false,
                    note: Some("bug in edge case X".into()),
                    add_blocked_by: vec![],
                    remove_blocked_by: vec![],
                },
            },
            reply: tx3,
        });
        assert!(rx3.await.unwrap().ok);

        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.assignee.as_deref(), Some("a"));
        assert_eq!(t.notes.len(), 2);
        assert_eq!(t.notes[1].body, "bug in edge case X");
        // a's pane now has a handoff-back system block.
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::System { text, .. } => {
                assert!(text.contains("assigned to you by b"), "got: {text}");
            }
            other => panic!("expected System block, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn task_update_invalid_status_errors() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "t".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskUpdate {
                    id,
                    title: None,
                    description: None,
                    status: Some("nope".into()),
                    assignee: None,
                    unassign: false,
                    note: None,
                    add_blocked_by: vec![],
                    remove_blocked_by: vec![],
                },
            },
            reply: tx2,
        });
        let resp = rx2.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("invalid status"));
    }

    #[tokio::test]
    async fn task_list_mine_filters_to_calling_agent() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();
        let b = app.chats[1].session_id.unwrap();

        // a adds one task assigned to b, one to a.
        for assignee in ["b", "a"] {
            let (tx, rx) = oneshot::channel();
            app.handle_agent_socket_event(AgentSocketEvent {
                request: crate::agent_ipc::Request {
                    agent_id: a,
                    cmd: RequestCmd::TaskAdd {
                        title: format!("t for {assignee}"),
                        description: None,
                        assignee: Some(assignee.into()),
                        blocked_by: vec![],
                    },
                },
                reply: tx,
            });
            assert!(rx.await.unwrap().ok);
        }

        // b lists --mine → 1 task.
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: b,
                cmd: RequestCmd::TaskList {
                    assignee: None,
                    status: None,
                    mine: true,
                    unassigned: false,
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok);
        let msg = resp.message.unwrap();
        assert!(msg.contains("t for b"), "got: {msg}");
        assert!(!msg.contains("t for a"), "leaked other assignment: {msg}");
    }

    #[tokio::test]
    async fn task_remove_drops_the_task() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "t".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskRemove { id },
            },
            reply: tx2,
        });
        assert!(rx2.await.unwrap().ok);
        assert!(app.tasks.get(id).is_none());
    }

    #[tokio::test]
    async fn task_note_appends_to_log() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "t".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskNote {
                    id,
                    body: "investigating".into(),
                },
            },
            reply: tx2,
        });
        assert!(rx2.await.unwrap().ok);
        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.notes.len(), 1);
        assert_eq!(t.notes[0].body, "investigating");
        assert_eq!(t.notes[0].author, "a");
    }

    #[tokio::test]
    async fn task_show_renders_full_body_with_notes_and_deps() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        // Create a prereq, then a main task depending on it.
        let (tx0, rx0) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "prereq".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx0,
        });
        let prereq = parse_task_created(rx0.await.unwrap().message.as_deref().unwrap());

        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "ship feature".into(),
                    description: Some("Wire up the thing.\nAcceptance: tests pass.".into()),
                    assignee: Some("b".into()),
                    blocked_by: vec![prereq],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        // Attach a handoff note so the Notes block has content.
        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskNote {
                    id,
                    body: "see commit abc123".into(),
                },
            },
            reply: tx2,
        });
        assert!(rx2.await.unwrap().ok);

        // Now show it and check the rendering.
        let (tx3, rx3) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskShow { id },
            },
            reply: tx3,
        });
        let resp = rx3.await.unwrap();
        assert!(resp.ok, "error: {:?}", resp.error);
        let body = resp.message.unwrap();

        assert!(body.contains(&id.to_string()), "missing id: {body}");
        assert!(
            body.contains("Title: ship feature"),
            "missing title: {body}"
        );
        assert!(body.contains("Status: assigned"), "missing status: {body}");
        assert!(body.contains("Assignee: b"), "missing assignee: {body}");
        assert!(
            body.contains("Wire up the thing."),
            "missing description: {body}"
        );
        assert!(
            body.contains("Acceptance: tests pass."),
            "missing description line 2: {body}"
        );
        assert!(
            body.contains("Dependencies:"),
            "missing deps header: {body}"
        );
        assert!(body.contains(&prereq.to_string()), "missing dep id: {body}");
        assert!(body.contains("prereq"), "missing dep title: {body}");
        assert!(
            body.contains("blocked: some deps not yet done"),
            "missing blocked marker: {body}"
        );
        assert!(body.contains("Notes:"), "missing notes header: {body}");
        assert!(
            body.contains("see commit abc123"),
            "missing note body: {body}"
        );
    }

    #[tokio::test]
    async fn task_show_missing_id_errors() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskShow { id: Uuid::new_v4() },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn task_take_skips_blocked_tasks_and_claims_them_when_dep_done() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();
        let b = app.chats[1].session_id.unwrap();

        // a creates a prereq task.
        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "prereq".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let prereq_id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        // a creates a dependent task blocked by the prereq.
        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "dependent".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![prereq_id],
                },
            },
            reply: tx2,
        });
        let dep_id = parse_task_created(rx2.await.unwrap().message.as_deref().unwrap());

        // b takes → must claim the prereq, NOT the dependent.
        let (tx3, rx3) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: b,
                cmd: RequestCmd::TaskTake,
            },
            reply: tx3,
        });
        let resp = rx3.await.unwrap();
        assert!(resp.ok);
        assert!(resp.message.unwrap().contains("prereq"));
        assert_eq!(
            app.tasks.get(prereq_id).unwrap().assignee.as_deref(),
            Some("b")
        );
        assert!(app.tasks.get(dep_id).unwrap().assignee.is_none());

        // b marks the prereq done.
        let (tx4, rx4) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: b,
                cmd: RequestCmd::TaskUpdate {
                    id: prereq_id,
                    title: None,
                    description: None,
                    status: Some("done".into()),
                    assignee: None,
                    unassign: false,
                    note: None,
                    add_blocked_by: vec![],
                    remove_blocked_by: vec![],
                },
            },
            reply: tx4,
        });
        assert!(rx4.await.unwrap().ok);

        // b takes again → now the dependent is claimable.
        let (tx5, rx5) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: b,
                cmd: RequestCmd::TaskTake,
            },
            reply: tx5,
        });
        assert!(rx5.await.unwrap().ok);
        assert_eq!(
            app.tasks.get(dep_id).unwrap().assignee.as_deref(),
            Some("b")
        );
    }

    #[tokio::test]
    async fn task_add_rejects_unknown_blocked_by() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "t".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![Uuid::new_v4()],
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().to_lowercase().contains("dependency"));
    }

    #[tokio::test]
    async fn task_update_rejects_cycle() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        // Create x; create y blocked_by x.
        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "x".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let x = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "y".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![x],
                },
            },
            reply: tx2,
        });
        let y = parse_task_created(rx2.await.unwrap().message.as_deref().unwrap());

        // Now try to make x depend on y → cycle.
        let (tx3, rx3) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskUpdate {
                    id: x,
                    title: None,
                    description: None,
                    status: None,
                    assignee: None,
                    unassign: false,
                    note: None,
                    add_blocked_by: vec![y],
                    remove_blocked_by: vec![],
                },
            },
            reply: tx3,
        });
        let resp = rx3.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().to_lowercase().contains("cycle"));
    }

    #[tokio::test]
    async fn task_remove_prunes_dependents() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "prereq".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let prereq = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "dependent".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![prereq],
                },
            },
            reply: tx2,
        });
        let dep = parse_task_created(rx2.await.unwrap().message.as_deref().unwrap());

        // Remove the prereq; dependent's blocked_by should be empty now.
        let (tx3, rx3) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskRemove { id: prereq },
            },
            reply: tx3,
        });
        assert!(rx3.await.unwrap().ok);
        assert!(app.tasks.get(dep).unwrap().blocked_by.is_empty());
    }

    #[tokio::test]
    async fn task_store_persists_across_app_restart() {
        use crate::tasks::TaskStore;
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("tasks.json");

        let id;
        {
            let mut app = App::new(sample_config());
            app.tasks = TaskStore::load_or_init(path.clone()).unwrap();
            wire_up_agent_ids_and_channels(&mut app);
            let a = app.chats[0].session_id.unwrap();

            let (tx, rx) = oneshot::channel();
            app.handle_agent_socket_event(AgentSocketEvent {
                request: crate::agent_ipc::Request {
                    agent_id: a,
                    cmd: RequestCmd::TaskAdd {
                        title: "persisted".into(),
                        description: None,
                        assignee: Some("b".into()),
                        blocked_by: vec![],
                    },
                },
                reply: tx,
            });
            id = parse_task_created(rx.await.unwrap().message.as_deref().unwrap());
        } // drop app → file must already be on disk from persist()

        let mut app2 = App::new(sample_config());
        app2.tasks = TaskStore::load_or_init(path).unwrap();
        let t = app2.tasks.get(id).expect("task survived restart");
        assert_eq!(t.title, "persisted");
        assert_eq!(t.assignee.as_deref(), Some("b"));
    }

    // --------- Tasks screen (in-TUI) ----------

    use super::super::tasks_view::{FormField, TaskFormKind, TasksPrompt};

    fn send_keys(app: &mut App, keys: &[KeyCode]) {
        for k in keys {
            app.handle_key(*k, KeyModifiers::NONE);
        }
    }

    fn type_string(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    #[test]
    fn activating_tasks_view_from_views_list_switches_main_area() {
        let mut app = App::new(sample_config());
        // Tab twice → Views focus.
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Views);
        // Enter activates the selected view (only "Tasks" today).
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.active_view, ActiveView::Tasks);
        assert_eq!(app.focus, Focus::Tasks);
    }

    #[test]
    fn tab_cycles_three_panels_in_tasks_view() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Agents);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Views);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Tasks);
    }

    #[test]
    fn moving_in_agents_list_while_viewing_tasks_returns_to_chat() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Agents;
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.active_view, ActiveView::Chat);
    }

    #[test]
    fn pressing_a_opens_add_form_and_submitting_creates_task() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        // `a` opens the form.
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(app.tasks_screen.form.is_some());
        // Type a title, then Enter submits.
        type_string(&mut app, "from tui");
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.is_none(), "form closes on submit");
        assert_eq!(app.tasks.all().len(), 1);
        assert_eq!(app.tasks.all()[0].title, "from tui");
        assert_eq!(app.tasks.all()[0].created_by, "tui");
    }

    #[test]
    fn add_form_rejects_empty_title_and_surfaces_error() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Submit with no title.
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.is_some());
        assert!(app.tasks_screen.form.as_ref().unwrap().error.is_some());
        assert_eq!(app.tasks.all().len(), 0);
    }

    #[test]
    fn t_takes_an_open_task() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("work".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        // Selection should land on the first task row automatically;
        // render_tasks_screen would reclamp, but since tests don't
        // render, do it manually.
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('t'), KeyModifiers::NONE);
        let t = &app.tasks.all()[0];
        assert_eq!(t.status, crate::tasks::TaskStatus::InProgress);
        assert_eq!(t.assignee.as_deref(), Some("tui"));
    }

    #[test]
    fn s_cycles_status_on_selected_task() {
        let mut app = App::new(sample_config());
        let id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('s'), KeyModifiers::NONE);
        // After cycle from Open the task moves to Assigned group.
        // reclamp snaps selected_row to the first task; which group
        // it's in is irrelevant to this assertion.
        assert_eq!(
            app.tasks.get(id).unwrap().status,
            crate::tasks::TaskStatus::Assigned
        );
    }

    #[test]
    fn d_then_y_removes_the_selected_task() {
        let mut app = App::new(sample_config());
        let id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(matches!(
            app.tasks_screen.prompt,
            Some(TasksPrompt::ConfirmDelete { .. })
        ));
        app.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);
        assert!(app.tasks.get(id).is_none());
        assert!(app.tasks_screen.prompt.is_none());
    }

    #[test]
    fn d_then_n_cancels_delete() {
        let mut app = App::new(sample_config());
        let id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        assert!(app.tasks.get(id).is_some(), "task should still exist");
        assert!(app.tasks_screen.prompt.is_none());
    }

    #[test]
    fn n_prompts_for_note_body_and_submitting_appends_it() {
        let mut app = App::new(sample_config());
        let id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        type_string(&mut app, "progress");
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.notes.len(), 1);
        assert_eq!(t.notes[0].body, "progress");
        assert_eq!(t.notes[0].author, "tui");
    }

    #[test]
    fn enter_opens_detail_modal_and_esc_closes_it() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.detail.is_some());
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.tasks_screen.detail.is_none());
    }

    #[test]
    fn e_opens_edit_form_prefilled_from_task() {
        let mut app = App::new(sample_config());
        app.tasks
            .add(
                "original".into(),
                Some("original desc".into()),
                Some("a".into()),
                "seed".into(),
                Vec::new(),
            )
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert!(matches!(form.kind, TaskFormKind::Edit(_)));
        assert_eq!(form.title, "original");
        assert_eq!(form.description, "original desc");
        assert_eq!(form.assignee, "a");
    }

    #[test]
    fn esc_from_idle_tasks_view_returns_to_chat() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.active_view, ActiveView::Chat);
        assert_eq!(app.focus, Focus::Chat);
    }

    #[test]
    fn views_list_nav_up_down_adjusts_selection() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Views;
        // Only one entry; nav is no-op but must not panic.
        send_keys(&mut app, &[KeyCode::Down, KeyCode::Up]);
        assert_eq!(app.selected_view, 0);
    }

    #[test]
    fn clicking_tasks_entry_in_views_activates_the_tasks_view() {
        let mut app = App::new(sample_config());
        // Pretend the UI drew the views list at (30, 5) 20×3.
        app.layout.views_list_inner = Some(Rect::new(30, 5, 20, 3));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 31,
            row: 5, // the first entry row
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.active_view, ActiveView::Tasks);
        assert_eq!(app.focus, Focus::Tasks);
    }

    #[test]
    fn clicking_outside_views_entries_focuses_views_panel_only() {
        let mut app = App::new(sample_config());
        app.layout.views_list_inner = Some(Rect::new(30, 5, 20, 5));
        // Row 8 is inside the panel but beyond the single entry at row 5.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 31,
            row: 8,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.active_view, ActiveView::Chat);
        assert_eq!(app.focus, Focus::Views);
    }

    #[test]
    fn clicking_main_area_while_viewing_tasks_sets_tasks_focus_not_chat() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.layout.chat_history_inner = Some(Rect::new(0, 2, 60, 20));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.focus, Focus::Tasks);
    }

    #[test]
    fn tab_in_open_form_moves_to_next_field_not_focus_cycle() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Sanity: form is on Title to start.
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Title
        );
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        // Tab must have moved the form cursor, not cycled panel focus.
        assert_eq!(app.focus, Focus::Tasks);
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Description
        );
    }

    #[test]
    fn down_arrow_in_form_also_moves_to_next_field() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Description
        );
    }

    #[test]
    fn up_arrow_in_form_moves_to_prev_field() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Title
        );
    }

    #[test]
    fn typing_in_second_field_writes_to_second_field_only() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        type_string(&mut app, "mytitle");
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        type_string(&mut app, "mydesc");
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert_eq!(form.title, "mytitle");
        assert_eq!(form.description, "mydesc");
    }

    #[test]
    fn enter_on_dropdown_field_opens_picker_popup() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Tab to Assignee.
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Assignee
        );
        // Enter opens the picker; does NOT submit.
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.is_some());
        assert!(app.tasks_screen.form.as_ref().unwrap().picker.is_some());
    }

    #[test]
    fn picker_down_and_enter_updates_assignee() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE); // open picker
        let before = app.tasks_screen.form.as_ref().unwrap().assignee.clone();
        app.handle_key(KeyCode::Down, KeyModifiers::NONE); // next option
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE); // commit
        let after = app.tasks_screen.form.as_ref().unwrap().assignee.clone();
        assert_ne!(before, after);
        assert!(app.tasks_screen.form.as_ref().unwrap().picker.is_none());
    }

    #[test]
    fn picker_esc_cancels_without_changing_value() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let before = app.tasks_screen.form.as_ref().unwrap().assignee.clone();
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.as_ref().unwrap().picker.is_none());
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().assignee, before);
        assert!(app.tasks_screen.form.is_some());
    }

    #[test]
    fn typing_is_ignored_on_a_dropdown_field() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        let before = app.tasks_screen.form.as_ref().unwrap().assignee.clone();
        app.handle_key(KeyCode::Char('x'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);
        let after = app.tasks_screen.form.as_ref().unwrap().assignee.clone();
        assert_eq!(before, after);
    }

    #[test]
    fn edit_form_status_picker_updates_status_on_commit() {
        let mut app = App::new(sample_config());
        let id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        // Tab to Status (Title → Description → Assignee → Status).
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        // Open the picker, move down to "assigned", commit.
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().status, "assigned");
        // Back to Title, Enter to submit.
        app.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
        app.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
        app.handle_key(KeyCode::BackTab, KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(
            app.tasks.get(id).unwrap().status,
            crate::tasks::TaskStatus::Assigned
        );
    }

    #[test]
    fn clicking_agent_from_tasks_view_returns_to_chat() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.layout.agent_list_inner = Some(Rect::new(60, 5, 20, 10));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 61,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.active_view, ActiveView::Chat);
        assert_eq!(app.focus, Focus::Chat);
    }
}
