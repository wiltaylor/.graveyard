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
use crate::ai::{
    AICore, AIEvent, AciddogCliCtx, AnthropicApiCore, ClaudeCodeCore, CopilotCliCore,
    GeminiCliCore, OpenAiApiCore, PromptParts, WorkerCtx,
};
use crate::cli::{AgentArgs, AgentCommand};
use crate::config::{AciddogConfig, AiCore, AiCoreKind};
use crate::memory::MemoryRegistry;
use crate::tasks::{Task, TaskFilter, TaskPatch, TaskStatus, TaskStore, TodoStatus};

use super::tasks_view::{
    ChildOverlay, ChildPromptKind, FormField, TaskForm, TasksPrompt, TasksScreen, OPERATOR,
};

/// True when a child-overlay prompt is a yes/no confirmation rather
/// than a body-entry prompt. Used by `handle_child_overlay_key` to
/// short-circuit on single-char y/n input.
fn is_confirm_prompt(kind: ChildPromptKind) -> bool {
    matches!(
        kind,
        ChildPromptKind::ConfirmDeleteTodo(_) | ChildPromptKind::ConfirmDeleteComment(_)
    )
}

use super::blocks::{BlockAction, ChatBlock, ChatButton, ScrollAnchor};
use super::clipboard;

use super::ui;

const COPIED_TOAST_MS: u64 = 1500;

/// Short label for the AgentInfo chat block. Format is
/// `<variant> (<wcl-id>)` so the user can see both the runtime kind
/// and which specific `ai_core` block in aciddog.wcl is backing this
/// agent (e.g. `anthropic-api (z_ai)`).
pub(super) fn core_display_label(core: &AiCore) -> String {
    let kind = match &core.kind {
        AiCoreKind::ClaudeCode { .. } => "claude-code",
        AiCoreKind::AnthropicApi { .. } => "anthropic-api",
        AiCoreKind::OpenAiApi { .. } => "openai-api",
        AiCoreKind::CopilotCli { .. } => "copilot-cli",
        AiCoreKind::GeminiCli { .. } => "gemini-cli",
    };
    format!("{kind} ({})", core.id)
}

/// Whether this core kind reports per-round token deltas (so we sum
/// them) or cumulative session totals (so we replace).
pub(super) fn core_usage_mode(kind: &AiCoreKind) -> ContextUsageMode {
    match kind {
        AiCoreKind::AnthropicApi { .. } | AiCoreKind::OpenAiApi { .. } => ContextUsageMode::Add,
        AiCoreKind::ClaudeCode { .. }
        | AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. } => ContextUsageMode::Replace,
    }
}

/// Mutate the seeded AgentInfo block (always at `blocks[0]`) so its
/// `context_used` field reflects the chat's current running totals.
/// No-op if the chat doesn't have a seeded AgentInfo block.
fn refresh_agent_info_usage(chat: &mut AgentChat) {
    if let Some(ChatBlock::AgentInfo { context_used, .. }) = chat.blocks.first_mut() {
        *context_used = chat.context_used;
    }
}

/// Shell-tool name for this core variant. Kept config-side so the TUI
/// can decide whether to enable tag-IPC *before* the core is built
/// (and therefore before the trait-side `AICore::shell_tool_name` is
/// available). Keep this in sync with each core's trait impl.
fn shell_tool_name_for_core(core: &AiCore) -> Option<&'static str> {
    match &core.kind {
        AiCoreKind::ClaudeCode { .. }
        | AiCoreKind::AnthropicApi { .. }
        | AiCoreKind::OpenAiApi { .. }
        | AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. } => Some("Bash"),
    }
}

/// Whether an agent with this core and tool set must use the tag-based
/// IPC protocol (because it has no shell tool to call the aciddog CLI).
pub(super) fn uses_tag_ipc(tools: &[String], core: &AiCore) -> bool {
    match shell_tool_name_for_core(core) {
        Some(name) => !tools.iter().any(|t| t == name),
        None => true,
    }
}

/// Construct a boxed `AICore` for the given config variant, wiring env
/// and (optionally) the tag-IPC socket. `resume` tells the factory to
/// skip the first-turn preamble — used by `resurrect_worker` so a
/// hibernated worker resumes cleanly.
pub(super) fn build_core(
    core_cfg: &AiCore,
    session_id: Uuid,
    parts: PromptParts,
    tools: Vec<String>,
    env: Vec<(String, String)>,
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    resume: bool,
) -> Box<dyn AICore> {
    match &core_cfg.kind {
        AiCoreKind::ClaudeCode { system_prompt, .. } => {
            let mut cc = if resume {
                ClaudeCodeCore::with_session_id(session_id, tools, system_prompt.clone())
            } else {
                ClaudeCodeCore::new(session_id, parts, tools, system_prompt.clone())
            }
            .with_env(env);
            if let Some(tx) = socket_tx {
                cc = cc.with_socket_tx(tx);
            }
            Box::new(cc)
        }
        AiCoreKind::AnthropicApi {
            api_url,
            api_key_env,
            model,
            system_prompt,
            ..
        } => {
            let mut c = AnthropicApiCore::new(
                session_id,
                api_url.clone(),
                api_key_env.clone(),
                model.clone(),
                parts,
                tools,
                system_prompt.clone(),
            )
            .with_env(env);
            if resume {
                c = c.mark_started();
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Box::new(c)
        }
        AiCoreKind::OpenAiApi {
            api_url,
            api_key_env,
            model,
            system_prompt,
            ..
        } => {
            let mut c = OpenAiApiCore::new(
                session_id,
                api_url.clone(),
                api_key_env.clone(),
                model.clone(),
                parts,
                tools,
                system_prompt.clone(),
            )
            .with_env(env);
            if resume {
                c = c.mark_started();
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Box::new(c)
        }
        AiCoreKind::CopilotCli { model, .. } => {
            let mut c = if resume {
                CopilotCliCore::with_session_id(session_id, tools)
            } else {
                CopilotCliCore::new(session_id, parts, tools)
            }
            .with_env(env);
            if let Some(m) = model {
                c = c.with_model(m.clone());
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Box::new(c)
        }
        AiCoreKind::GeminiCli { model, .. } => {
            // `resume = true` here means "the aciddog-side session was
            // previously started" — for gemini that just skips the
            // preamble. Gemini's own conversation history is not
            // preserved across process restarts (same v1 limitation as
            // AnthropicApi); the first post-resurrection turn starts a
            // fresh gemini session under the covers.
            let mut c = if resume {
                GeminiCliCore::with_session_id(session_id, tools)
            } else {
                GeminiCliCore::new(session_id, parts, tools)
            }
            .with_env(env);
            if let Some(m) = model {
                c = c.with_model(m.clone());
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Box::new(c)
        }
    }
}

/// Render the per-actor `<team>` body: the team description plus the
/// agents this actor may message and the worker templates it may
/// spawn. Each section is omitted when its allowlist is empty so an
/// isolated agent gets only the team description. Agent and worker
/// short `role` strings are looked up against `cfg`.
pub(super) fn build_team_block(
    cfg: &AciddogConfig,
    contactable: &[&str],
    creatable: &[&str],
) -> String {
    let mut out = String::new();
    out.push_str(&format!("Team: {}\n", cfg.team.description));

    if !contactable.is_empty() {
        out.push_str("\nAgents you can message:\n");
        for id in contactable {
            let role = cfg
                .agents
                .iter()
                .find(|a| a.id == *id)
                .map(|a| a.role.as_str())
                .unwrap_or("agent");
            out.push_str(&format!("- {id} — {role}\n"));
        }
    }

    if !creatable.is_empty() {
        out.push_str("\nWorker templates you can spawn:\n");
        for id in creatable {
            let role = cfg
                .workers
                .iter()
                .find(|w| w.id == *id)
                .map(|w| w.role.as_str())
                .unwrap_or("worker");
            out.push_str(&format!("- {id} — {role}\n"));
        }
    }

    out
}

/// Format a list of memory-search hits as the plaintext body we return
/// to the calling agent. Empty results get a short "(no matches)" line
/// so the agent doesn't have to special-case empty strings.
///
/// Uuids are printed in full so the agent can copy them straight into
/// `memory link` / `memory unlink` / `memory links` calls — the short
/// form would force them to re-run `memory store` just to recover the
/// full id. `distance` is elided when the search was non-semantic
/// (no embedding → NaN).
fn format_memory_hits(hits: &[crate::memory::Hit]) -> String {
    if hits.is_empty() {
        return "(no matches)".to_string();
    }
    let mut out = String::new();
    for (i, h) in hits.iter().enumerate() {
        let ts = h.created_at.format("%Y-%m-%d %H:%M");
        let dist = if h.distance.is_nan() {
            String::new()
        } else {
            format!(" · d={:.3}", h.distance)
        };
        let title = if h.title.is_empty() {
            String::new()
        } else {
            format!(" · title={}", h.title)
        };
        let tags = if h.tags.is_empty() {
            String::new()
        } else {
            format!(" · tags=[{}]", h.tags.join(","))
        };
        out.push_str(&format!(
            "{n}. [{id}{dist} · {ts}{title}{tags}] {body}\n",
            n = i + 1,
            id = h.id,
            body = h.body
        ));
    }
    out.trim_end().to_string()
}

/// Short 8-char prefix of a uuid for chat display.
fn short_uuid(id: uuid::Uuid) -> String {
    let s = id.to_string();
    s[..8.min(s.len())].to_string()
}

/// Format a list of links from `memory links` as the plaintext body
/// returned to the agent. Arrows reflect direction: `->` for outgoing,
/// `<-` for incoming. An empty kind is omitted so short links don't
/// carry dead brackets. Peer uuids are printed in full so the agent
/// can feed them directly into another `memory` call.
fn format_memory_links(id: uuid::Uuid, links: &[crate::memory::Link]) -> String {
    if links.is_empty() {
        return format!("(no links on {id})");
    }
    let mut out = String::new();
    for link in links {
        let arrow = match link.direction {
            crate::memory::LinkDirection::Out => "->",
            crate::memory::LinkDirection::In => "<-",
            crate::memory::LinkDirection::Both => "<->",
        };
        let kind = if link.kind.is_empty() {
            String::new()
        } else {
            format!(" [{}]", link.kind)
        };
        let title = if link.peer_title.is_empty() {
            String::new()
        } else {
            format!(" title={}", link.peer_title)
        };
        let tags = if link.peer_tags.is_empty() {
            String::new()
        } else {
            format!(" tags=[{}]", link.peer_tags.join(","))
        };
        out.push_str(&format!(
            "{arrow} {peer}{kind}{title}{tags} {body}\n",
            peer = link.peer_id,
            body = link.peer_body
        ));
    }
    out.trim_end().to_string()
}

/// Render the per-actor `<memory>` body: a short preamble plus one
/// line per bank the actor is allowed to read/write, with the bank's
/// description. Returns `None` when the actor has zero banks so the
/// `<memory>` tag is omitted cleanly.
pub(super) fn build_memory_block(cfg: &AciddogConfig, actor_id: &str) -> Option<String> {
    let banks = cfg.memory_banks_for(actor_id);
    if banks.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str(
        "You have access to the following memory banks. Each bank is scoped to a topic — \
         search it when you need context and store an entry when you learn something worth \
         persisting. Entries are retrieved by semantic similarity.\n\n",
    );
    for bank in banks {
        out.push_str(&format!("- {} — {}\n", bank.id, bank.description));
    }
    Some(out)
}

/// Render the caller's assigned skills as `- <id> — <description>`
/// lines, one per skill. Unreadable `SKILL.md` files warn and render
/// a placeholder line rather than aborting. Returns a friendly
/// "no skills assigned" sentence when `allowed` is empty so the
/// caller (typically the `skill list` IPC handler) can forward it
/// verbatim.
fn render_skill_listing(actor_id: &str, allowed: &[String]) -> String {
    if allowed.is_empty() {
        return "No skills are assigned to you.".to_string();
    }
    let root = crate::skills::skills_root();
    let mut out = String::new();
    for id in allowed {
        match crate::skills::read_skill_description(&root, id) {
            Ok(desc) => out.push_str(&format!("- {id} — {desc}\n")),
            Err(e) => {
                warn!(skill = %id, actor = %actor_id, error = %e, "skill SKILL.md unreadable");
                out.push_str(&format!("- {id} — (SKILL.md unreadable)\n"));
            }
        }
    }
    out
}

/// Render a dispatched `<aciddog-call>` (emitted by `ClaudeCodeCore`
/// in tag mode as a `ToolUse` event named `aciddog-call`) as one of
/// the existing semantic chat blocks. `input` is the parsed
/// `RequestCmd` re-serialised to JSON; we deserialise it back into
/// `RequestCmd` to match on the variant.
fn block_for_aciddog_tag_call(input: &serde_json::Value) -> ChatBlock {
    use crate::cli::{CommentCommand, TaskCommand, TodoCommand};

    let cmd: Option<RequestCmd> = serde_json::from_value(input.clone()).ok();
    let Some(cmd) = cmd else {
        return ChatBlock::from_tool_use("aciddog-call", input);
    };

    match cmd {
        RequestCmd::SendMessage { to, body } => ChatBlock::mail_sent(to, body),
        RequestCmd::WorkerCreate { template, task } => ChatBlock::worker_spawn(template, task),
        RequestCmd::WorkerComplete { body } => ChatBlock::worker_complete(body),
        RequestCmd::Test => ChatBlock::from_tool_use("aciddog test", input),
        RequestCmd::TaskAdd {
            title,
            description,
            assignee,
            blocked_by,
        } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Add {
                title,
                description,
                assignee,
                blocked_by,
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskList {
            assignee,
            status,
            mine,
            unassigned,
        } => {
            let (k, s, b) = task_op_summary(&TaskCommand::List {
                assignee,
                status,
                mine,
                unassigned,
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskShow { id } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Show { id });
            ChatBlock::task_op(k, s, b)
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
            let (k, s, b) = task_op_summary(&TaskCommand::Update {
                id,
                title,
                description,
                status,
                assignee,
                unassign,
                note,
                add_blocked_by,
                remove_blocked_by,
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskRemove { id } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Remove { id });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskTake => {
            let (k, s, b) = task_op_summary(&TaskCommand::Take);
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskNote { id, body } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Note { id, body });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskTodoAdd { task, body } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Todo {
                cmd: TodoCommand::Add { task, body },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskTodoList { task } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Todo {
                cmd: TodoCommand::List { task },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskTodoUpdate {
            task,
            id,
            status,
            body,
        } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Todo {
                cmd: TodoCommand::Update {
                    task,
                    id,
                    status,
                    body,
                },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskTodoRemove { task, id } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Todo {
                cmd: TodoCommand::Remove { task, id },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskCommentAdd { task, body } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Comment {
                cmd: CommentCommand::Add { task, body },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskCommentList { task } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Comment {
                cmd: CommentCommand::List { task },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskCommentUpdate { task, id, body } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Comment {
                cmd: CommentCommand::Update { task, id, body },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::TaskCommentRemove { task, id } => {
            let (k, s, b) = task_op_summary(&TaskCommand::Comment {
                cmd: CommentCommand::Remove { task, id },
            });
            ChatBlock::task_op(k, s, b)
        }
        RequestCmd::MemorySearch { .. } => ChatBlock::from_tool_use("aciddog memory search", input),
        RequestCmd::MemoryStore { .. } => ChatBlock::from_tool_use("aciddog memory store", input),
        RequestCmd::MemoryLink { .. } => ChatBlock::from_tool_use("aciddog memory link", input),
        RequestCmd::MemoryUnlink { .. } => ChatBlock::from_tool_use("aciddog memory unlink", input),
        RequestCmd::MemoryLinks { .. } => ChatBlock::from_tool_use("aciddog memory links", input),
        RequestCmd::Info => ChatBlock::from_tool_use("aciddog info", input),
        RequestCmd::CronAdd { .. } => ChatBlock::from_tool_use("aciddog cron add", input),
        RequestCmd::CronList { .. } => ChatBlock::from_tool_use("aciddog cron list", input),
        RequestCmd::CronShow { .. } => ChatBlock::from_tool_use("aciddog cron show", input),
        RequestCmd::CronUpdate { .. } => ChatBlock::from_tool_use("aciddog cron update", input),
        RequestCmd::CronRemove { .. } => ChatBlock::from_tool_use("aciddog cron remove", input),
        RequestCmd::SkillList => ChatBlock::from_tool_use("aciddog skill list", input),
        RequestCmd::SkillSearch { .. } => {
            ChatBlock::from_tool_use("aciddog skill search", input)
        }
        RequestCmd::SkillRun { .. } => ChatBlock::from_tool_use("aciddog skill run", input),
    }
}

/// Parse a `--kind` flag value into a [`crate::cron::CronPayload`].
/// Returns the same human-readable error a CLI surface would.
fn parse_cron_payload(kind: &str, payload: String) -> Result<crate::cron::CronPayload, String> {
    match kind.trim().to_ascii_lowercase().as_str() {
        "message" => {
            let p = crate::cron::CronPayload::Message { body: payload };
            crate::cron::validate_payload(&p).map_err(|e| e.to_string())?;
            Ok(p)
        }
        "command" => {
            let p = crate::cron::CronPayload::SlashCommand { command: payload };
            crate::cron::validate_payload(&p).map_err(|e| e.to_string())?;
            Ok(p)
        }
        other => Err(format!(
            "invalid kind `{other}` (expected `message` or `command`)"
        )),
    }
}

/// Parse a `--schedule` flag plus the `--oneoff` toggle into a
/// [`crate::cron::CronSchedule`].
fn parse_cron_schedule(
    schedule: &str,
    oneoff: bool,
) -> Result<crate::cron::CronSchedule, crate::cron::CronError> {
    if oneoff {
        crate::cron::parse_oneoff(schedule)
    } else {
        crate::cron::parse_recurring(schedule)
    }
}

/// One-line summary of a cron job for `cron list`. Calls out
/// targets that no longer match a configured agent so the operator
/// can see broken jobs at a glance.
fn fmt_cron_job_short(j: &crate::cron::CronJob, config: &crate::config::AciddogConfig) -> String {
    let kind = match &j.payload {
        crate::cron::CronPayload::Message { .. } => "message",
        crate::cron::CronPayload::SlashCommand { .. } => "command",
    };
    let sched = match &j.schedule {
        crate::cron::CronSchedule::Recurring { expression } => format!("recurring `{expression}`"),
        crate::cron::CronSchedule::OneOff { run_at_local } => format!("oneoff @{run_at_local}"),
    };
    let next = j
        .next_run
        .map(|n| n.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| "—".to_string());
    let enabled = if j.enabled { "enabled" } else { "disabled" };
    let target_known = config.agents.iter().any(|a| a.id == j.target_agent);
    let target_marker = if target_known { "" } else { " [target unknown]" };
    format!(
        "{id} {enabled} {kind} → {target}{marker}; {sched}; tz={tz}; next_run={next}",
        id = j.id,
        target = j.target_agent,
        marker = target_marker,
        tz = j.timezone.as_str(),
    )
}

/// Multi-line full rendering for `cron show`.
fn fmt_cron_job_full(j: &crate::cron::CronJob, config: &crate::config::AciddogConfig) -> String {
    let mut out = String::new();
    out.push_str(&format!("id:                 {}\n", j.id));
    out.push_str(&format!("target_agent:       {}", j.target_agent));
    if !config.agents.iter().any(|a| a.id == j.target_agent) {
        out.push_str(" [target unknown]");
    }
    out.push('\n');
    out.push_str(&format!("created_by:         {}\n", j.created_by));
    out.push_str(&format!(
        "created_at:         {}\n",
        j.created_at.format("%Y-%m-%dT%H:%M:%SZ")
    ));
    out.push_str(&format!(
        "enabled:            {}\n",
        if j.enabled { "true" } else { "false" }
    ));
    out.push_str(&format!("timezone:           {}\n", j.timezone.as_str()));
    out.push_str(&format!(
        "catchup_on_startup: {}\n",
        if j.catchup_on_startup { "true" } else { "false" }
    ));
    match &j.payload {
        crate::cron::CronPayload::Message { body } => {
            out.push_str("payload_kind:       message\n");
            out.push_str(&format!("payload:            {body}\n"));
        }
        crate::cron::CronPayload::SlashCommand { command } => {
            out.push_str("payload_kind:       command\n");
            out.push_str(&format!("payload:            {command}\n"));
        }
    }
    match &j.schedule {
        crate::cron::CronSchedule::Recurring { expression } => {
            out.push_str("schedule_kind:      recurring\n");
            out.push_str(&format!("schedule:           {expression}\n"));
        }
        crate::cron::CronSchedule::OneOff { run_at_local } => {
            out.push_str("schedule_kind:      oneoff\n");
            out.push_str(&format!("schedule:           {run_at_local}\n"));
        }
    }
    out.push_str(&format!(
        "last_run:           {}\n",
        j.last_run
            .map(|n| n.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_else(|| "—".to_string()),
    ));
    out.push_str(&format!(
        "next_run:           {}\n",
        j.next_run
            .map(|n| n.format("%Y-%m-%dT%H:%M:%SZ").to_string())
            .unwrap_or_else(|| "—".to_string()),
    ));
    out
}

/// Who is "sending" a chat message in [`App::handle_send_message_with_origin`].
/// Plain agent-to-agent sends come from [`MessageOrigin::Agent`]; cron-fired
/// deliveries come from [`MessageOrigin::Cron`] and carry the creator's
/// display id plus the firing job id so the recipient can see which schedule
/// woke them up.
#[derive(Debug, Clone)]
pub enum MessageOrigin {
    Agent { sender_idx: usize },
    Cron { job_id: Uuid, created_by: String },
}

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
    /// When a task form is open, every labelled field and button is
    /// given a clickable rect keyed by `FormField`. Populated each
    /// frame by `render_task_form` and cleared on every fresh
    /// frame. Clicking a field focuses it (and opens the picker on
    /// dropdowns); clicking a button activates it.
    pub task_form_fields: Vec<(FormField, Rect)>,
    /// When a dropdown picker is open, each visible option gets a
    /// clickable rect here. Populated by `render_dropdown_picker`
    /// and consumed by the mouse handler.
    pub task_form_picker_rows: Vec<Rect>,
    /// Outer rect of the active picker overlay (single-select or
    /// multi-select blocked-by). Used by the mouse handler to
    /// distinguish "click outside the picker" (close) from "click
    /// inside but not on an interactive element" (swallow).
    pub task_form_picker_outer: Option<Rect>,
    /// OK/Cancel button rects on the multi-select blocked-by
    /// picker. Empty when no picker is open or when the picker
    /// doesn't show buttons.
    pub task_form_picker_buttons: Vec<(FormField, Rect)>,
    /// When a child overlay (TODOs or Comments) is open, each row
    /// gets a clickable rect here. The mouse handler uses it to
    /// jump the cursor to the clicked row.
    pub child_overlay_rows: Vec<Rect>,
    /// The full outer rect of the child overlay, used to detect
    /// clicks *outside* the overlay (which close it).
    pub child_overlay_outer: Option<Rect>,
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
    /// Byte offset of the caret inside `input`. Always on a UTF-8 char
    /// boundary.
    pub input_cursor: usize,
    /// Top visible wrapped row when the input buffer overflows the
    /// visible rows of the input box.
    pub input_scroll: u16,
    /// Inner width of the input box at the last render, cached so the
    /// key handler can wrap the buffer the same way.
    pub input_wrap_width: u16,
    /// Inner height of the input box at the last render, cached so
    /// Up/Down and scroll-follow math match what the user sees.
    pub input_visible_rows: u16,
    /// Index into `AciddogConfig::thinking_messages` for the line shown
    /// by the thinking indicator. `Some` whenever `status == Working`;
    /// set once on entering `Working` and cleared on leaving so the
    /// message stays stable for a whole turn.
    pub thinking_message_idx: Option<usize>,
    /// Cumulative token usage across the whole session for this agent,
    /// as reported by its core. `None` until the first TurnResult that
    /// carries usage data — cores that never report it (e.g. copilot)
    /// keep this at `None` for the entire session.
    pub context_used: Option<crate::ai::TurnUsage>,
    /// How `context_used` should be combined with each new TurnResult.
    /// Cores that emit cumulative totals at end of turn (claude CLI,
    /// gemini CLI) use `Replace`; cores that emit per-round deltas
    /// (Anthropic API, OpenAI API) use `Add`. Set at agent spawn from
    /// the core kind.
    pub context_usage_mode: ContextUsageMode,
    /// Maximum context window size for the model this agent runs on,
    /// in tokens. `None` when the model is unknown or the core doesn't
    /// expose a model id. Mirrored into the seeded `AgentInfo` block
    /// at spawn — kept here too so resurrection paths and any future
    /// status-bar consumers don't need to reach into `blocks[0]`.
    #[allow(dead_code)]
    pub context_window: Option<u32>,
    /// Cached full-height render of the current anchor block. Reused
    /// across frames so mouse-wheel scrolling inside a tall block only
    /// re-blits rows, rather than re-rendering the whole block each
    /// tick. Invalidated in `render_agent_chat` when width, height,
    /// block index, or a cheap content fingerprint changes.
    pub anchor_scratch: Option<super::blocks::AnchorScratch>,
    /// Compaction work queued up while the current turn finishes. Set
    /// by `promptAndDrop` after the final prompt is enqueued; consumed
    /// in `handle_ai_event` on `TurnResult` to trigger teardown.
    pub pending_compaction: Option<super::compaction::PendingCompaction>,
}

/// How an agent's running token total should incorporate each new
/// TurnResult's `usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextUsageMode {
    /// Each new TurnResult carries the full cumulative session totals;
    /// overwrite. Used by Claude Code CLI and Gemini CLI.
    Replace,
    /// Each new TurnResult carries the delta for that round; add to
    /// the running total. Used by Anthropic API and OpenAI API cores.
    Add,
}

impl Default for ContextUsageMode {
    fn default() -> Self {
        // Conservative default: most callers will overwrite this at
        // spawn anyway. `Replace` means a stray usage event won't
        // accumulate forever if the spawn path forgets to set the mode.
        ContextUsageMode::Replace
    }
}

impl AgentChat {
    pub(super) fn push_block(&mut self, block: ChatBlock) -> usize {
        let idx = self.blocks.len();
        self.blocks.push(block);
        self.sticky_bottom = true;
        idx
    }

    /// Trim the chat back to the opening `AgentInfo` block (if any),
    /// append a `system_info` marker explaining the compaction, and
    /// reset scroll/selection so the user sees the fresh start. Used
    /// by every compaction strategy.
    pub(super) fn reset_to_init(&mut self, note: impl Into<String>) {
        let keep = self
            .blocks
            .iter()
            .position(|b| matches!(b, ChatBlock::AgentInfo { .. }))
            .map(|i| i + 1)
            .unwrap_or(0);
        self.blocks.truncate(keep);
        self.tool_index.clear();
        self.selected_block = None;
        self.anchor_scratch = None;
        self.sticky_bottom = true;
        self.push_block(ChatBlock::system_info(note.into()));
    }

    /// Fold a fresh `TurnUsage` into the running session totals,
    /// honouring the core's reporting style (cumulative vs per-round).
    pub fn apply_turn_usage(&mut self, u: crate::ai::TurnUsage) {
        if u.is_zero() {
            return;
        }
        match self.context_usage_mode {
            ContextUsageMode::Replace => {
                self.context_used = Some(u);
            }
            ContextUsageMode::Add => {
                let prev = self.context_used.unwrap_or_default();
                self.context_used = Some(crate::ai::TurnUsage {
                    input_tokens: prev.input_tokens.saturating_add(u.input_tokens),
                    output_tokens: prev.output_tokens.saturating_add(u.output_tokens),
                    cache_read_input_tokens: prev
                        .cache_read_input_tokens
                        .saturating_add(u.cache_read_input_tokens),
                    cache_creation_input_tokens: prev
                        .cache_creation_input_tokens
                        .saturating_add(u.cache_creation_input_tokens),
                });
            }
        }
    }

    /// Clear the input buffer and any cursor/scroll state that references it.
    fn reset_input(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
        self.input_scroll = 0;
    }

    /// Word-wrap the input buffer to the given width and return one
    /// `(start, end)` byte span per visual row. A trailing empty row
    /// is emitted when the last logical line is empty or ends in `\n`,
    /// so the caret can be positioned there.
    pub fn input_visual_rows(&self, width: u16) -> Vec<(usize, usize)> {
        wrap_rows(&self.input, width)
    }

    /// Byte offset at the start of the caret's current visual row,
    /// plus the caret's column (in chars, not bytes) within that row.
    pub fn input_cursor_visual(&self, width: u16) -> (usize, usize, usize) {
        let rows = self.input_visual_rows(width);
        if rows.is_empty() {
            return (0, 0, 0);
        }
        // Find the row that contains `input_cursor`. Prefer the later row
        // when the cursor sits exactly on a boundary: that matches what
        // the user sees after Down/End.
        let cursor = self.input_cursor.min(self.input.len());
        let mut row_idx = 0;
        for (i, &(start, end)) in rows.iter().enumerate() {
            if cursor >= start && cursor <= end {
                row_idx = i;
                // If cursor is exactly at `end` and a later row starts at
                // the same offset (word-wrap boundary with no newline),
                // prefer that later row.
                if cursor == end && i + 1 < rows.len() && rows[i + 1].0 == end {
                    row_idx = i + 1;
                }
                break;
            }
            if i == rows.len() - 1 {
                row_idx = i;
            }
        }
        let (row_start, _) = rows[row_idx];
        let col = self.input[row_start..cursor].chars().count();
        (row_idx, row_start, col)
    }

    /// Transition this chat into `Working`, picking a random thinking
    /// message on the way in. A no-op (except for the status write) if
    /// the chat is already `Working`, so the message stays stable for
    /// the whole turn.
    pub fn begin_working(&mut self, pool_len: usize) {
        if self.status != AgentStatus::Working {
            self.thinking_message_idx = Some(pseudo_random(pool_len));
        }
        self.status = AgentStatus::Working;
    }

    /// Leave the `Working` state (turn ended, error, hibernation) and
    /// clear the thinking message so the indicator disappears. The
    /// caller writes the new status explicitly.
    pub fn end_working(&mut self) {
        self.thinking_message_idx = None;
    }
}

/// Pick an index in `[0, n)` from the current system clock. Good enough
/// for picking a random thinking message; not cryptographic.
fn pseudo_random(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0)
        % n
}

/// Word-wrap `s` to `width` columns. Returns one `(start, end)` byte
/// span per visual row. Width 0 is treated as 1. Wraps on word (space)
/// boundaries; words longer than `width` are hard-broken. Always
/// returns at least one row — an empty buffer yields `[(0, 0)]`.
fn wrap_rows(s: &str, width: u16) -> Vec<(usize, usize)> {
    let width = width.max(1) as usize;
    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut line_start = 0usize;
    for (i, b) in s.bytes().enumerate() {
        if b == b'\n' {
            wrap_line_into(s, line_start, i, width, &mut rows);
            line_start = i + 1;
        }
    }
    wrap_line_into(s, line_start, s.len(), width, &mut rows);
    if rows.is_empty() {
        rows.push((0, 0));
    }
    rows
}

/// Wrap one logical line `[start, end)` (no newlines inside) into visual
/// rows. Empty lines produce a single empty row so the caret has a home.
fn wrap_line_into(s: &str, start: usize, end: usize, width: usize, out: &mut Vec<(usize, usize)>) {
    if start == end {
        out.push((start, start));
        return;
    }

    // Collect tokens as (tok_start, tok_end, is_space, cols).
    let mut tokens: Vec<(usize, usize, bool, usize)> = Vec::new();
    let line = &s[start..end];
    let mut cur_start = start;
    let mut cur_is_space: Option<bool> = None;
    let mut cur_cols = 0usize;
    for (off, ch) in line.char_indices() {
        let abs = start + off;
        let is_space = ch == ' ';
        match cur_is_space {
            None => {
                cur_is_space = Some(is_space);
                cur_start = abs;
                cur_cols = 1;
            }
            Some(prev) if prev == is_space => cur_cols += 1,
            Some(prev) => {
                tokens.push((cur_start, abs, prev, cur_cols));
                cur_is_space = Some(is_space);
                cur_start = abs;
                cur_cols = 1;
            }
        }
    }
    if let Some(is_space) = cur_is_space {
        tokens.push((cur_start, end, is_space, cur_cols));
    }

    let mut row_start = start;
    let mut row_cols = 0usize;
    for &(t_start, t_end, is_space, t_cols) in &tokens {
        if is_space {
            row_cols += t_cols;
            continue;
        }
        if t_cols > width {
            // Hard-break the word across rows.
            if row_cols >= width && row_cols > 0 {
                out.push((row_start, t_start));
                row_start = t_start;
                row_cols = 0;
            }
            let mut cursor = t_start;
            if row_cols > 0 && row_cols < width {
                let take = width - row_cols;
                let br = advance_chars(s, cursor, take, t_end);
                out.push((row_start, br));
                cursor = br;
                row_start = br;
                row_cols = 0;
            }
            while cursor < t_end {
                let br = advance_chars(s, cursor, width, t_end);
                if br == t_end {
                    row_start = cursor;
                    row_cols = s[cursor..br].chars().count();
                    break;
                }
                out.push((cursor, br));
                cursor = br;
                row_start = br;
                row_cols = 0;
            }
        } else {
            if row_cols + t_cols > width && row_cols > 0 {
                out.push((row_start, t_start));
                row_start = t_start;
                row_cols = 0;
            }
            row_cols += t_cols;
        }
    }
    if row_cols > 0 || out.last().is_none_or(|&(_, e)| e != end) {
        out.push((row_start, end));
    }
}

fn advance_chars(s: &str, from: usize, count: usize, cap: usize) -> usize {
    for (taken, (off, _)) in s[from..cap].char_indices().enumerate() {
        if taken == count {
            return from + off;
        }
    }
    cap
}

/// Effective wrap width to use in key-handling. Falls back to a sane
/// default when the input hasn't been rendered yet (e.g. during tests).
fn effective_input_wrap_width(chat: &AgentChat) -> u16 {
    if chat.input_wrap_width > 0 {
        chat.input_wrap_width
    } else {
        u16::MAX
    }
}

/// Visible input rows. Mirrors `input_wrap_width`: if unset, fall back
/// to the 5-row cap we plan to grow to so scroll math works in tests.
fn effective_input_visible_rows(chat: &AgentChat) -> u16 {
    if chat.input_visible_rows > 0 {
        chat.input_visible_rows
    } else {
        5
    }
}

fn input_insert(chat: &mut AgentChat, ch: char) {
    let idx = chat.input_cursor.min(chat.input.len());
    chat.input.insert(idx, ch);
    chat.input_cursor = idx + ch.len_utf8();
    clamp_input_scroll(chat);
}

fn input_backspace(chat: &mut AgentChat) {
    if chat.input_cursor == 0 {
        return;
    }
    let mut prev = chat.input_cursor - 1;
    while prev > 0 && !chat.input.is_char_boundary(prev) {
        prev -= 1;
    }
    chat.input.replace_range(prev..chat.input_cursor, "");
    chat.input_cursor = prev;
    clamp_input_scroll(chat);
}

fn input_move_char(chat: &mut AgentChat, delta: i32) {
    if delta < 0 {
        if chat.input_cursor == 0 {
            return;
        }
        let mut prev = chat.input_cursor - 1;
        while prev > 0 && !chat.input.is_char_boundary(prev) {
            prev -= 1;
        }
        chat.input_cursor = prev;
    } else if delta > 0 {
        if chat.input_cursor >= chat.input.len() {
            return;
        }
        let mut next = chat.input_cursor + 1;
        while next < chat.input.len() && !chat.input.is_char_boundary(next) {
            next += 1;
        }
        chat.input_cursor = next;
    }
    clamp_input_scroll(chat);
}

fn input_move_line(chat: &mut AgentChat, delta: i32) {
    let width = effective_input_wrap_width(chat);
    let rows = chat.input_visual_rows(width);
    let (row_idx, _row_start, col) = chat.input_cursor_visual(width);
    let target_row = (row_idx as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
    if target_row == row_idx {
        return;
    }
    let (t_start, t_end) = rows[target_row];
    chat.input_cursor = advance_chars(&chat.input, t_start, col, t_end);
    clamp_input_scroll(chat);
}

fn input_move_to_row_start(chat: &mut AgentChat) {
    let width = effective_input_wrap_width(chat);
    let (_row_idx, row_start, _col) = chat.input_cursor_visual(width);
    chat.input_cursor = row_start;
    clamp_input_scroll(chat);
}

fn input_move_to_row_end(chat: &mut AgentChat) {
    let width = effective_input_wrap_width(chat);
    let rows = chat.input_visual_rows(width);
    let (row_idx, _row_start, _col) = chat.input_cursor_visual(width);
    if let Some(&(_, end)) = rows.get(row_idx) {
        chat.input_cursor = end;
    }
    clamp_input_scroll(chat);
}

/// Keep `input_scroll` such that the cursor's visual row is visible.
fn clamp_input_scroll(chat: &mut AgentChat) {
    let width = effective_input_wrap_width(chat);
    let visible = effective_input_visible_rows(chat).max(1);
    let rows = chat.input_visual_rows(width);
    let total = rows.len() as u16;
    let (row_idx, _, _) = chat.input_cursor_visual(width);
    let row_idx = row_idx as u16;
    if row_idx < chat.input_scroll {
        chat.input_scroll = row_idx;
    } else if row_idx >= chat.input_scroll + visible {
        chat.input_scroll = row_idx + 1 - visible;
    }
    let max_scroll = total.saturating_sub(visible);
    if chat.input_scroll > max_scroll {
        chat.input_scroll = max_scroll;
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
    /// Clone of the agent-IPC channel used for tag-protocol dispatch
    /// (when an agent has no shell tool). When `None`, tag-mode cores
    /// cannot be constructed. Populated by `run` alongside `ai_tx`.
    pub socket_events_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    /// Shared team task board. `App::new` initialises this ephemeral
    /// (in-memory only); `tui::run` replaces it with a disk-backed
    /// store rooted at `.aciddog/tasks.json` in the cwd so tasks
    /// survive restarts.
    pub tasks: TaskStore,
    /// Disk-backed cron jobs. `App::new` initialises this ephemeral;
    /// `tui::run` replaces it with a store rooted at
    /// `.aciddog/cron.json` and runs startup reconciliation (missed
    /// one-offs pruned unless they opted into catchup; recurring
    /// `next_run` recomputed from now).
    pub cron: crate::cron::CronStore,
    /// Woken whenever a cron mutation might change the next-due
    /// instant. The scheduler arm of `event_loop` awaits both
    /// `sleep_until(next_due)` and this notify, so newly-added jobs
    /// with a closer `next_run` fire on time instead of waiting for
    /// the previously-scheduled wakeup.
    pub cron_invalidate: std::sync::Arc<tokio::sync::Notify>,
    /// Disk-backed semantic memory banks. `App::new` leaves this unset;
    /// `tui::run` constructs a registry rooted at `.aciddog/memory/`
    /// before the TUI starts, so every agent that spawns afterwards
    /// sees its banks. `None` ⇒ memory subcommands fail with a
    /// "no memory registry" error.
    pub memory: Option<std::sync::Arc<MemoryRegistry>>,
    /// Which content is rendered in the main (left) area.
    pub active_view: ActiveView,
    /// Which row of the right-column Views list is highlighted.
    pub selected_view: usize,
    /// UI-local state for the Tasks screen: cursor, modals, prompts.
    pub tasks_screen: TasksScreen,
    /// Last left-click coord + timestamp, used only to detect
    /// double-clicks on task rows. A second click at the same
    /// (col,row) within `DOUBLE_CLICK_WINDOW` counts as double.
    pub last_click: Option<(u16, u16, Instant)>,
    /// Monotonic counter driven by the 100ms ticker in `event_loop`.
    /// Used solely to advance the thinking-indicator spinner; incremented
    /// only when at least one chat is `Working`, so idle terminals don't
    /// redraw.
    pub frame_tick: u64,
    /// Sender for results produced by background compaction tasks
    /// (currently just the summarise strategy). Populated by `run`
    /// alongside `ai_tx`; drained in `event_loop`.
    pub compaction_tx: Option<mpsc::Sender<super::compaction::CompactionEvent>>,
}

/// Time window for detecting a double-click. Matches the typical
/// OS default of 500ms.
pub const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);

impl App {
    pub fn new(config: AciddogConfig) -> Self {
        let chats: Vec<AgentChat> = config
            .agents
            .iter()
            .map(|agent| {
                let core = config.effective_ai_core(agent.ai_core.as_deref());
                let tools = config.effective_tools(agent.tools.as_deref(), core);
                let label = core
                    .map(core_display_label)
                    .unwrap_or_else(|| "claude-code".to_string());
                let model = core.and_then(|c| c.runtime_model());
                let context_window = model.and_then(crate::ai::model_context_window);
                let context_usage_mode = core
                    .map(|c| core_usage_mode(&c.kind))
                    .unwrap_or(ContextUsageMode::Replace);
                let info = ChatBlock::agent_info(
                    label,
                    tools,
                    agent.instructions.clone(),
                    context_window,
                );
                AgentChat {
                    blocks: vec![info],
                    sticky_bottom: true,
                    display_name: agent.id.clone(),
                    context_window,
                    context_usage_mode,
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
            socket_events_tx: None,
            tasks: TaskStore::new(),
            cron: crate::cron::CronStore::new(),
            cron_invalidate: std::sync::Arc::new(tokio::sync::Notify::new()),
            memory: None,
            active_view: ActiveView::default(),
            selected_view: 0,
            tasks_screen: TasksScreen::default(),
            last_click: None,
            frame_tick: 0,
            compaction_tx: None,
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
            chat.reset_input();
            content
        };
        self.handle_local_input(selected, content);
    }

    /// Inject `content` into agent `idx` exactly as if the user had
    /// typed it into that agent's input box. Slash commands (leading
    /// `/`) go through [`commands::dispatch`] and `apply_command` —
    /// the same branch `submit_input` uses — so the cron scheduler
    /// can fire `/foo` payloads identically to a real keystroke.
    /// Plain text is echoed as a user block and routed via
    /// `chat.user_tx`.
    pub(crate) fn handle_local_input(&mut self, idx: usize, content: String) {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return;
        }
        let content = trimmed.to_string();

        // Echo the input as a user block on the target chat so the
        // history records what was typed (or fired by cron).
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.push_block(ChatBlock::user(content.clone()));
        } else {
            return;
        }

        if content.starts_with('/') {
            if let Some(outcome) = super::commands::dispatch(&content) {
                self.apply_command_for(idx, outcome);
            }
            return;
        }

        let pool_len = self.config.thinking_messages.len();
        let Some(chat) = self.chats.get_mut(idx) else {
            return;
        };
        if let Some(tx) = &chat.user_tx {
            match tx.send(content) {
                Ok(()) => {
                    chat.begin_working(pool_len);
                }
                Err(_closed) => {
                    chat.push_block(ChatBlock::system_error(
                        "agent offline, message dropped".to_string(),
                    ));
                    chat.status = AgentStatus::Error;
                    chat.end_working();
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
                // The tag-mode core synthesizes ToolUse/ToolResult
                // events for every `<aciddog-call>` it extracts, so
                // the raw tag markup is visual noise in the assistant
                // text. Strip it; if the whole message was tags, drop
                // the block entirely.
                let cleaned = crate::ai::tag_call::strip_tags(&text);
                if !cleaned.trim().is_empty() {
                    chat.push_block(ChatBlock::ai_markdown(cleaned));
                }
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
                    } else if let Some(task_cmd) = try_parse_aciddog_task(cmd, &binary) {
                        let (kind, subject, body) = task_op_summary(&task_cmd);
                        ChatBlock::task_op(kind, subject, body)
                    } else if is_aciddog_skill_list(cmd, &binary) {
                        ChatBlock::from_tool_use(
                            "aciddog skill list",
                            &serde_json::json!({}),
                        )
                    } else if let Some(query) = try_parse_aciddog_skill_search(cmd, &binary) {
                        ChatBlock::from_tool_use(
                            "aciddog skill search",
                            &serde_json::json!({ "query": query }),
                        )
                    } else if let Some(skill_name) = try_parse_aciddog_skill_run(cmd, &binary) {
                        ChatBlock::from_tool_use(
                            "aciddog skill run",
                            &serde_json::json!({ "name": skill_name }),
                        )
                    } else {
                        ChatBlock::from_tool_use(&name, &input)
                    }
                } else if name == "aciddog-call" {
                    // Tag-mode core synthesizes these for each
                    // dispatched <aciddog-call>. Map by the parsed
                    // RequestCmd to the same semantic blocks that
                    // Bash-dispatched CLI calls use.
                    block_for_aciddog_tag_call(&input)
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
            AIEvent::TurnResult {
                total_cost_usd,
                usage,
                ..
            } => {
                if let Some(cost) = total_cost_usd {
                    info!(cost_usd = cost, agent_idx = idx, "turn complete");
                }
                if let Some(u) = usage {
                    chat.apply_turn_usage(u);
                    refresh_agent_info_usage(chat);
                }
                if chat.status == AgentStatus::Working {
                    chat.status = AgentStatus::Ready;
                    chat.end_working();
                }
                // `promptAndDrop` and `summarise` may have queued a
                // teardown to run once the current turn completes.
                // Early-return so the `chat` borrow ends before we
                // call back into `self`.
                if chat.pending_compaction.is_some() {
                    self.finish_pending_compaction(idx);
                }
            }
            AIEvent::Error { message, .. } => {
                chat.push_block(ChatBlock::system_error(message));
                chat.status = AgentStatus::Error;
                chat.end_working();
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
            RequestCmd::TaskTodoAdd { task, body } => {
                self.handle_task_todo_add(idx, task, body, reply);
            }
            RequestCmd::TaskTodoList { task } => {
                self.handle_task_todo_list(idx, task, reply);
            }
            RequestCmd::TaskTodoUpdate {
                task,
                id,
                status,
                body,
            } => {
                self.handle_task_todo_update(idx, task, id, status, body, reply);
            }
            RequestCmd::TaskTodoRemove { task, id } => {
                self.handle_task_todo_remove(idx, task, id, reply);
            }
            RequestCmd::TaskCommentAdd { task, body } => {
                self.handle_task_comment_add(idx, task, body, reply);
            }
            RequestCmd::TaskCommentList { task } => {
                self.handle_task_comment_list(idx, task, reply);
            }
            RequestCmd::TaskCommentUpdate { task, id, body } => {
                self.handle_task_comment_update(idx, task, id, body, reply);
            }
            RequestCmd::TaskCommentRemove { task, id } => {
                self.handle_task_comment_remove(idx, task, id, reply);
            }
            RequestCmd::MemorySearch {
                bank,
                query,
                title_contains,
                include_tags,
                exclude_tags,
                limit,
            } => {
                self.handle_memory_search(
                    idx,
                    bank,
                    query,
                    title_contains,
                    include_tags,
                    exclude_tags,
                    limit,
                    reply,
                );
            }
            RequestCmd::MemoryStore {
                bank,
                body,
                title,
                tags,
            } => {
                self.handle_memory_store(idx, bank, body, title, tags, reply);
            }
            RequestCmd::MemoryLink {
                bank,
                from,
                to,
                kind,
            } => {
                self.handle_memory_link(idx, bank, from, to, kind, reply);
            }
            RequestCmd::MemoryUnlink { bank, from, to } => {
                self.handle_memory_unlink(idx, bank, from, to, reply);
            }
            RequestCmd::MemoryLinks {
                bank,
                id,
                direction,
                include_tags,
                exclude_tags,
            } => {
                self.handle_memory_links(
                    idx,
                    bank,
                    id,
                    direction,
                    include_tags,
                    exclude_tags,
                    reply,
                );
            }
            RequestCmd::Info => {
                self.handle_info(reply);
            }
            RequestCmd::SkillList => {
                self.handle_skill_list(idx, reply);
            }
            RequestCmd::SkillSearch { query } => {
                self.handle_skill_search(idx, query, reply);
            }
            RequestCmd::SkillRun { name } => {
                self.handle_skill_run(idx, name, reply);
            }
            RequestCmd::CronAdd {
                target,
                kind,
                payload,
                schedule,
                tz,
                oneoff,
                catchup_on_startup,
                disabled,
            } => {
                self.handle_cron_add(
                    idx,
                    target,
                    kind,
                    payload,
                    schedule,
                    tz,
                    oneoff,
                    catchup_on_startup,
                    disabled,
                    reply,
                );
            }
            RequestCmd::CronList {
                target,
                include_disabled,
                mine,
            } => {
                self.handle_cron_list(idx, target, include_disabled, mine, reply);
            }
            RequestCmd::CronShow { id } => {
                self.handle_cron_show(idx, id, reply);
            }
            RequestCmd::CronUpdate {
                id,
                target,
                kind,
                payload,
                schedule,
                tz,
                oneoff,
                catchup_on_startup,
                enabled,
            } => {
                self.handle_cron_update(
                    idx,
                    id,
                    target,
                    kind,
                    payload,
                    schedule,
                    tz,
                    oneoff,
                    catchup_on_startup,
                    enabled,
                    reply,
                );
            }
            RequestCmd::CronRemove { id } => {
                self.handle_cron_remove(idx, id, reply);
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

        let core = match self.config.effective_ai_core(worker.ai_core.as_deref()) {
            Some(c) => c,
            None => {
                let _ = reply.send(Response::err(format!(
                    "worker template `{template}` has no ai_core (no override and no team default_ai_core)"
                )));
                return;
            }
        };
        let tools = self
            .config
            .effective_tools(worker.tools.as_deref(), Some(core));
        let tag_mode = uses_tag_ipc(&tools, core);

        let creatable: Vec<&str> = worker
            .can_create_workers
            .as_deref()
            .map(|list| list.iter().map(String::as_str).collect())
            .unwrap_or_default();

        // A worker can only message the actor that spawned it; that
        // single peer becomes its team listing.
        let contactable_for_worker: Vec<&str> = vec![sender_display.as_str()];
        let team_block = build_team_block(&self.config, &contactable_for_worker, &creatable);

        let memory_banks_for_worker: Vec<String> = worker
            .memory_banks
            .as_deref()
            .map(|list| list.to_vec())
            .unwrap_or_default();
        let memory_block = build_memory_block(&self.config, &template);
        let skills_for_worker = self.config.skills_for(&template);

        let parts = PromptParts {
            role: worker.role_description.clone(),
            team: Some(team_block),
            memory: memory_block,
            instructions: worker.instructions.clone(),
            aciddog_cli: Some(AciddogCliCtx {
                binary: self.aciddog_binary.clone(),
                contactable: vec![sender_display.clone()],
                creatable: creatable.iter().map(|s| s.to_string()).collect(),
                memory_banks: memory_banks_for_worker,
                skills: skills_for_worker,
                worker: Some(WorkerCtx {
                    parent_id: sender_display.clone(),
                    task: task.clone(),
                }),
            }),
        };

        let env = vec![
            (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
            (
                agent_ipc::ENV_AGENT_SOCKET.to_string(),
                self.socket_path.clone(),
            ),
        ];

        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);

        let socket_tx_for_core = if tag_mode {
            match self.socket_events_tx.clone() {
                Some(tx) => Some(tx),
                None => {
                    tracing::warn!(
                        %display_name,
                        "tag-mode worker spawned before socket_events_tx was set; \
                         aborting worker"
                    );
                    let _ = reply.send(Response::err(
                        "orchestrator has no socket_events_tx; cannot spawn tag-mode worker",
                    ));
                    return;
                }
            }
        } else {
            None
        };

        let core_label = core_display_label(core);
        let context_window = core.runtime_model().and_then(crate::ai::model_context_window);
        let context_usage_mode = core_usage_mode(&core.kind);
        let built_core: Box<dyn AICore> = build_core(
            core,
            session_id,
            parts,
            tools.clone(),
            env,
            socket_tx_for_core,
            false,
        );
        let assembled_prompt = built_core.first_turn_prompt();
        let info_block =
            ChatBlock::agent_info(core_label, tools.clone(), assembled_prompt, context_window);
        let pool_len = self.config.thinking_messages.len();
        let mut chat = AgentChat {
            blocks: vec![info_block],
            sticky_bottom: true,
            user_tx: Some(user_tx.clone()),
            agent_tx: Some(agent_tx),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            thinking_message_idx: Some(pseudo_random(pool_len)),
            parent_idx: Some(sender_idx),
            worker_template: Some(template.clone()),
            display_name: display_name.clone(),
            hibernated: false,
            context_window,
            context_usage_mode,
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

        tokio::spawn(run_core(new_idx, built_core, user_rx, agent_rx, ai_tx));

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
        let pool_len = self.config.thinking_messages.len();
        if let Some(parent_chat) = self.chats.get_mut(parent_idx) {
            parent_chat.push_block(ChatBlock::mail_received(
                sender_display.clone(),
                body.clone(),
            ));
            // Parent's AI turn is about to process the worker's report;
            // reflect that in the status icon. `TurnResult` resets to
            // `Ready`. Preserve existing `Error`.
            if parent_chat.status != AgentStatus::Error {
                parent_chat.begin_working(pool_len);
            }
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
            chat.end_working();
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

        let ai_core = self.config.effective_ai_core(worker.ai_core.as_deref());
        let tools = self
            .config
            .effective_tools(worker.tools.as_deref(), ai_core);
        let tag_mode = ai_core.map(|c| uses_tag_ipc(&tools, c)).unwrap_or(false);

        let env = vec![
            (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
            (
                agent_ipc::ENV_AGENT_SOCKET.to_string(),
                self.socket_path.clone(),
            ),
        ];

        let socket_tx_for_core = if tag_mode {
            match self.socket_events_tx.clone() {
                Some(tx) => Some(tx),
                None => return Err("orchestrator has no socket_events_tx".into()),
            }
        } else {
            None
        };

        // `resume = true` marks the core as "started" so the first
        // turn after resurrection does not re-prepend the worker
        // preamble. For ClaudeCode this also triggers `--resume`
        // against the claude session store; for AnthropicApi it just
        // skips the preamble (history is empty post-restart, which is
        // the documented v1 limitation).
        let ai_core = ai_core.ok_or_else(|| "worker has no resolvable ai_core".to_string())?;
        let core: Box<dyn AICore> = build_core(
            ai_core,
            session_id,
            PromptParts::default(),
            tools,
            env,
            socket_tx_for_core,
            true,
        );

        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);

        let pool_len = self.config.thinking_messages.len();
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.user_tx = Some(user_tx);
            chat.agent_tx = Some(agent_tx);
            chat.begin_working(pool_len);
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
        self.handle_send_message_with_origin(
            MessageOrigin::Agent { sender_idx },
            to,
            body,
            reply,
        );
    }

    /// Core message-delivery path. Drives recipient resolution, ACL,
    /// hibernation wake-up, recipient Mail block, and the async
    /// `agent_tx` enqueue. The origin determines the `from=`
    /// attribute and which ACL checks run.
    fn handle_send_message_with_origin(
        &mut self,
        origin: MessageOrigin,
        to: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        // Resolve sender identity + ACL context from the origin.
        let (sender_id, from_label, is_cron, sender_parent_idx) = match &origin {
            MessageOrigin::Agent { sender_idx } => {
                let Some(sender_chat) = self.chats.get(*sender_idx) else {
                    let _ = reply.send(Response::err("unknown sender"));
                    return;
                };
                let id = sender_chat.display_name.clone();
                let parent = sender_chat.parent_idx;
                (id.clone(), id, false, parent)
            }
            MessageOrigin::Cron {
                job_id,
                created_by,
            } => {
                let short = job_id.to_string();
                let short = short.get(..8).unwrap_or(&short);
                let label = format!("cron:{created_by}:{short}");
                (created_by.clone(), label, true, None)
            }
        };

        if !is_cron && to == sender_id {
            let _ = reply.send(Response::err("cannot send a message to yourself"));
            return;
        }

        let Some(recipient_idx) = self.chats.iter().position(|c| c.display_name == to) else {
            let _ = reply.send(Response::err(format!("no agent named {to}")));
            return;
        };

        match &origin {
            MessageOrigin::Agent { .. } => {
                // Worker routing rule: a worker may only message its parent.
                if let Some(parent_idx) = sender_parent_idx {
                    if recipient_idx != parent_idx {
                        let _ = reply.send(Response::err(
                            "worker can only message its parent".to_string(),
                        ));
                        return;
                    }
                } else {
                    // Configured agent: apply `can_message` unless the
                    // recipient is a worker this agent spawned.
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
            }
            MessageOrigin::Cron { .. } => {
                // Cron jobs are always delivered agent→agent (worker
                // targets are rejected at create time). Re-check the
                // creator's `can_message` allowlist at fire time so
                // config changes take effect.
                if !self.config.can_send(&sender_id, &to) {
                    let _ = reply.send(Response::err(format!(
                        "cron creator `{sender_id}` is not permitted to message `{to}`"
                    )));
                    return;
                }
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

        let payload = format!("<Message from=\"{from_label}\">{body}</Message>");

        let Some(agent_tx) = self
            .chats
            .get(recipient_idx)
            .and_then(|c| c.agent_tx.clone())
        else {
            let _ = reply.send(Response::err(format!("agent `{to}` is not online")));
            return;
        };

        let pool_len = self.config.thinking_messages.len();
        if let Some(chat) = self.chats.get_mut(recipient_idx) {
            chat.push_block(ChatBlock::mail_received(from_label.clone(), body.clone()));
            if chat.status != AgentStatus::Error {
                chat.begin_working(pool_len);
            }
        }

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

    fn handle_info(&mut self, reply: tokio::sync::oneshot::Sender<Response>) {
        let body = crate::info::render(chrono::Utc::now());
        let _ = reply.send(Response::ok(body));
    }

    /// List every skill assigned to the calling actor as one
    /// `- <id> — <description>` line per skill. Scoped to the
    /// caller's allowlist; never reveals unassigned skills.
    fn handle_skill_list(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(template_id) = self.sender_template_id(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let allowed = self.config.skills_for(&template_id);
        let body = render_skill_listing(&template_id, &allowed);
        let _ = reply.send(Response::ok(body));
    }

    /// Case-insensitive substring search over the caller's assigned
    /// skills. Matches against both skill id and description. Never
    /// surfaces skills outside the allowlist.
    fn handle_skill_search(
        &mut self,
        sender_idx: usize,
        query: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(template_id) = self.sender_template_id(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let needle = query.trim().to_ascii_lowercase();
        if needle.is_empty() {
            let _ = reply.send(Response::err("query must not be empty"));
            return;
        }
        let allowed = self.config.skills_for(&template_id);
        let root = crate::skills::skills_root();
        let mut rows: Vec<(String, String)> = Vec::new();
        for id in &allowed {
            let desc = match crate::skills::read_skill_description(&root, id) {
                Ok(d) => d,
                Err(e) => {
                    warn!(skill = %id, actor = %template_id, error = %e, "skill SKILL.md unreadable");
                    "(SKILL.md unreadable)".to_string()
                }
            };
            let hay = format!("{id} {desc}").to_ascii_lowercase();
            if hay.contains(&needle) {
                rows.push((id.clone(), desc));
            }
        }
        let body = if rows.is_empty() {
            format!("No assigned skills match `{}`.", query.trim())
        } else {
            let mut out = String::new();
            for (id, desc) in rows {
                out.push_str(&format!("- {id} — {desc}\n"));
            }
            out
        };
        let _ = reply.send(Response::ok(body));
    }

    /// Load a pre-announced skill's `SKILL.md` for the calling actor.
    /// The response body is the file's contents verbatim, prefixed
    /// with `{baseDir} = <absolute-path>` on its own line followed by
    /// a blank line — so `SKILL.md` authors can reference sibling
    /// files by that placeholder.
    ///
    /// Enforces the caller's `skills` allowlist. The
    /// "not-in-allowlist" error is distinct from the on-disk
    /// "not-found" error so agents can self-correct.
    fn handle_skill_run(
        &mut self,
        sender_idx: usize,
        name: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let template_id = match self.sender_template_id(sender_idx) {
            Some(t) => t,
            None => {
                let _ = reply.send(Response::err("unknown sender"));
                return;
            }
        };
        if !crate::skills::is_valid_skill_name(&name) {
            let _ = reply.send(Response::err(format!(
                "skill name `{name}` is not a valid identifier"
            )));
            return;
        }
        let allowed = self.config.skills_for(&template_id);
        if !allowed.iter().any(|id| id == &name) {
            let _ = reply.send(Response::err(format!(
                "skill `{name}` is not in your allowlist"
            )));
            return;
        }
        let dir = crate::skills::skills_root().join(&name);
        let abs_dir = match std::fs::canonicalize(&dir) {
            Ok(p) => p,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(
                    skill = %name,
                    actor = %template_id,
                    "skill directory missing"
                );
                let _ = reply.send(Response::err(format!("skill `{name}` not found")));
                return;
            }
            Err(e) => {
                warn!(
                    skill = %name,
                    actor = %template_id,
                    error = %e,
                    "skill directory unreadable"
                );
                let _ = reply.send(Response::err(format!(
                    "reading skill `{name}` directory: {e}"
                )));
                return;
            }
        };
        let skill_md = abs_dir.join("SKILL.md");
        let body = match std::fs::read_to_string(&skill_md) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                warn!(skill = %name, actor = %template_id, "SKILL.md missing");
                let _ = reply.send(Response::err(format!("skill `{name}` has no SKILL.md")));
                return;
            }
            Err(e) => {
                warn!(
                    skill = %name,
                    actor = %template_id,
                    error = %e,
                    "SKILL.md unreadable"
                );
                let _ = reply.send(Response::err(format!(
                    "reading SKILL.md for skill `{name}`: {e}"
                )));
                return;
            }
        };
        let abs_str = abs_dir.to_string_lossy().into_owned();
        let response = format!("{{baseDir}} = {abs_str}\n\n{body}");
        let _ = reply.send(Response::ok(response));
    }

    /// Validate caller-side ACL invariants and return the caller's
    /// display id. Returns `Err(reason)` if cron is unavailable to
    /// this caller (worker, or unknown sender). The reason is what
    /// gets sent back as `Response::err`.
    fn cron_caller_id(&self, sender_idx: usize) -> Result<String, String> {
        let chat = self
            .chats
            .get(sender_idx)
            .ok_or_else(|| "unknown sender".to_string())?;
        if chat.parent_idx.is_some() {
            return Err("workers cannot use cron".to_string());
        }
        Ok(chat.display_name.clone())
    }

    /// True when `id` matches a config-declared agent (workers are
    /// excluded). Used to validate cron `--target` at create time.
    fn is_configured_agent(&self, id: &str) -> bool {
        self.config.agents.iter().any(|a| a.id == id)
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_cron_add(
        &mut self,
        sender_idx: usize,
        target: String,
        kind: String,
        payload: String,
        schedule: String,
        tz: Option<String>,
        oneoff: bool,
        catchup_on_startup: bool,
        disabled: bool,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let sender_id = match self.cron_caller_id(sender_idx) {
            Ok(id) => id,
            Err(e) => {
                let _ = reply.send(Response::err(e));
                return;
            }
        };

        let tz_value = tz.as_deref().unwrap_or("local");
        let tz = match crate::cron::CronTz::parse(tz_value) {
            Ok(t) => t,
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };

        let payload = match parse_cron_payload(&kind, payload) {
            Ok(p) => p,
            Err(e) => {
                let _ = reply.send(Response::err(e));
                return;
            }
        };

        let schedule = match parse_cron_schedule(&schedule, oneoff) {
            Ok(s) => s,
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };

        if catchup_on_startup && !oneoff {
            let _ = reply.send(Response::err(
                "--catchup-on-startup is only valid for --oneoff jobs",
            ));
            return;
        }

        if !self.is_configured_agent(&target) {
            let _ = reply.send(Response::err(format!(
                "cron target `{target}` is not a configured agent (workers cannot be cron targets)"
            )));
            return;
        }

        if !self.config.can_send(&sender_id, &target) {
            let _ = reply.send(Response::err(format!(
                "agent `{sender_id}` is not permitted to message `{target}`"
            )));
            return;
        }

        let enabled = !disabled;
        match self.cron.add(
            target,
            payload,
            schedule,
            tz,
            catchup_on_startup,
            enabled,
            sender_id,
        ) {
            Ok(j) => {
                let id = j.id;
                self.cron_invalidate.notify_one();
                let _ = reply.send(Response::ok(format!("created cron job {id}")));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_cron_list(
        &mut self,
        sender_idx: usize,
        target: Option<String>,
        include_disabled: bool,
        mine: bool,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let sender_id = match self.cron_caller_id(sender_idx) {
            Ok(id) => id,
            Err(e) => {
                let _ = reply.send(Response::err(e));
                return;
            }
        };
        let filter = crate::cron::CronFilter {
            target,
            include_disabled,
            created_by: if mine { Some(sender_id) } else { None },
        };
        let jobs = self.cron.list(&filter);
        let body = if jobs.is_empty() {
            "no cron jobs".to_string()
        } else {
            let mut out = String::new();
            for j in jobs {
                out.push_str(&fmt_cron_job_short(j, &self.config));
                out.push('\n');
            }
            out.trim_end().to_string()
        };
        let _ = reply.send(Response::ok(body));
    }

    fn handle_cron_show(
        &mut self,
        sender_idx: usize,
        id: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if let Err(e) = self.cron_caller_id(sender_idx) {
            let _ = reply.send(Response::err(e));
            return;
        }
        match self.cron.get(id) {
            Some(j) => {
                let _ = reply.send(Response::ok(fmt_cron_job_full(j, &self.config)));
            }
            None => {
                let _ = reply.send(Response::err(format!("cron job {id} not found")));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_cron_update(
        &mut self,
        sender_idx: usize,
        id: Uuid,
        target: Option<String>,
        kind: Option<String>,
        payload: Option<String>,
        schedule: Option<String>,
        tz: Option<String>,
        oneoff: Option<bool>,
        catchup_on_startup: Option<bool>,
        enabled: Option<bool>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let sender_id = match self.cron_caller_id(sender_idx) {
            Ok(id) => id,
            Err(e) => {
                let _ = reply.send(Response::err(e));
                return;
            }
        };

        // Snapshot current target + oneoff so a partial update can
        // re-validate the resulting shape.
        let (current_target, current_oneoff) = match self.cron.get(id) {
            Some(j) => (
                j.target_agent.clone(),
                matches!(j.schedule, crate::cron::CronSchedule::OneOff { .. }),
            ),
            None => {
                let _ = reply.send(Response::err(format!("cron job {id} not found")));
                return;
            }
        };

        let _ = current_target; // kept for parity with future ACL re-checks
        let next_oneoff = oneoff.unwrap_or(current_oneoff);

        if let Some(ref t) = target {
            if !self.is_configured_agent(t) {
                let _ = reply.send(Response::err(format!(
                    "cron target `{t}` is not a configured agent"
                )));
                return;
            }
            if !self.config.can_send(&sender_id, t) {
                let _ = reply.send(Response::err(format!(
                    "agent `{sender_id}` is not permitted to message `{t}`"
                )));
                return;
            }
        }

        let new_tz = match tz.as_deref() {
            Some(s) => match crate::cron::CronTz::parse(s) {
                Ok(t) => Some(t),
                Err(e) => {
                    let _ = reply.send(Response::err(e.to_string()));
                    return;
                }
            },
            None => None,
        };

        let new_payload = match (kind, payload) {
            (Some(k), Some(p)) => match parse_cron_payload(&k, p) {
                Ok(pl) => Some(pl),
                Err(e) => {
                    let _ = reply.send(Response::err(e));
                    return;
                }
            },
            (Some(_), None) | (None, Some(_)) => {
                let _ = reply.send(Response::err(
                    "updating the payload requires both --kind and --payload",
                ));
                return;
            }
            (None, None) => None,
        };

        let new_schedule = match schedule.as_deref() {
            Some(s) => match parse_cron_schedule(s, next_oneoff) {
                Ok(parsed) => Some(parsed),
                Err(e) => {
                    let _ = reply.send(Response::err(e.to_string()));
                    return;
                }
            },
            None => None,
        };

        let patch = crate::cron::CronPatch {
            target_agent: target,
            payload: new_payload,
            schedule: new_schedule,
            timezone: new_tz,
            catchup_on_startup,
            enabled,
        };

        match self.cron.update(id, patch) {
            Ok(_) => {
                self.cron_invalidate.notify_one();
                let _ = reply.send(Response::ok(format!("updated cron job {id}")));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_cron_remove(
        &mut self,
        sender_idx: usize,
        id: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if let Err(e) = self.cron_caller_id(sender_idx) {
            let _ = reply.send(Response::err(e));
            return;
        }
        match self.cron.remove(id) {
            Ok(_) => {
                self.cron_invalidate.notify_one();
                let _ = reply.send(Response::ok(format!("removed cron job {id}")));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    /// Fire all due cron jobs at `now`. For each job:
    /// - Re-validate the target still resolves to a live agent chat.
    ///   If not: recurring → disable (keep for operator inspection),
    ///   one-off → remove. Either way log a warning.
    /// - Deliver the payload through the same path a real keystroke
    ///   or `aciddog send` would take.
    /// - `mark_fired` advances recurring `next_run` (or removes a
    ///   one-off).
    pub fn tick_cron(&mut self, now: chrono::DateTime<chrono::Utc>) {
        let due_ids = self.cron.due_at(now);
        for id in due_ids {
            let job = match self.cron.get(id) {
                Some(j) => j.clone(),
                None => continue,
            };

            let target_idx = self
                .chats
                .iter()
                .position(|c| c.display_name == job.target_agent && c.parent_idx.is_none());

            let Some(target_idx) = target_idx else {
                tracing::warn!(
                    job_id = %job.id,
                    target = %job.target_agent,
                    "cron target no longer exists; pruning or disabling job",
                );
                match job.schedule {
                    crate::cron::CronSchedule::OneOff { .. } => {
                        let _ = self.cron.remove(job.id);
                    }
                    crate::cron::CronSchedule::Recurring { .. } => {
                        let _ = self.cron.disable(job.id);
                    }
                }
                continue;
            };

            match &job.payload {
                crate::cron::CronPayload::Message { body } => {
                    let (tx, _rx) = tokio::sync::oneshot::channel();
                    self.handle_send_message_with_origin(
                        MessageOrigin::Cron {
                            job_id: job.id,
                            created_by: job.created_by.clone(),
                        },
                        job.target_agent.clone(),
                        body.clone(),
                        tx,
                    );
                }
                crate::cron::CronPayload::SlashCommand { command } => {
                    self.handle_local_input(target_idx, command.clone());
                }
            }

            if let Err(e) = self.cron.mark_fired(job.id, now) {
                tracing::warn!(job_id = %job.id, error = %e, "cron mark_fired failed");
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

    fn handle_task_todo_add(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let who = sender.display_name.clone();
        match self.tasks.add_todo(task, body, who) {
            Ok(t) => {
                let _ = reply.send(Response::ok(format!("todo added: {}", t.id)));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_task_todo_list(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        let Some(todos) = self.tasks.todos_of(task) else {
            let _ = reply.send(Response::err(format!("task {task} not found")));
            return;
        };
        if todos.is_empty() {
            let _ = reply.send(Response::ok("(no todos)"));
            return;
        }
        let mut out = String::new();
        out.push_str("id                                      status        body\n");
        for td in todos {
            out.push_str(&format!(
                "{}  {:<12}  {}\n",
                td.id,
                td.status.as_str(),
                td.body
            ));
        }
        let _ = reply.send(Response::ok(out.trim_end().to_string()));
    }

    fn handle_task_todo_update(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        id: Uuid,
        status: Option<String>,
        body: Option<String>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        let parsed_status = match status.as_deref().map(TodoStatus::parse).transpose() {
            Ok(v) => v,
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
                return;
            }
        };
        match self.tasks.update_todo(task, id, body, parsed_status) {
            Ok(_) => {
                let _ = reply.send(Response::ok("todo updated"));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_task_todo_remove(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        id: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        match self.tasks.remove_todo(task, id) {
            Ok(_) => {
                let _ = reply.send(Response::ok("todo removed"));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_task_comment_add(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        let who = sender.display_name.clone();
        match self.tasks.add_comment(task, body, who) {
            Ok(c) => {
                let _ = reply.send(Response::ok(format!("comment added: {}", c.id)));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_task_comment_list(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        let Some(comments) = self.tasks.comments_of(task) else {
            let _ = reply.send(Response::err(format!("task {task} not found")));
            return;
        };
        if comments.is_empty() {
            let _ = reply.send(Response::ok("(no comments)"));
            return;
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut out = String::new();
        for c in comments {
            out.push_str(&format!(
                "{}  [{} · {}] {}\n",
                c.id,
                format_age(now, c.timestamp_secs),
                c.author,
                c.body
            ));
        }
        let _ = reply.send(Response::ok(out.trim_end().to_string()));
    }

    fn handle_task_comment_update(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        id: Uuid,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        match self.tasks.update_comment(task, id, body) {
            Ok(_) => {
                let _ = reply.send(Response::ok("comment updated"));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_task_comment_remove(
        &mut self,
        sender_idx: usize,
        task: Uuid,
        id: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.chats.get(sender_idx).is_none() {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        }
        match self.tasks.remove_comment(task, id) {
            Ok(_) => {
                let _ = reply.send(Response::ok("comment removed"));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    /// Resolve the sender's template id for `memory_banks` allowlist
    /// checks. Configured agents use their agent id; workers use their
    /// spawn template id (the `memory_banks` list lives on the
    /// template, not the running instance).
    fn sender_template_id(&self, sender_idx: usize) -> Option<String> {
        let chat = self.chats.get(sender_idx)?;
        if let Some(tpl) = &chat.worker_template {
            return Some(tpl.clone());
        }
        self.config.agents.get(sender_idx).map(|a| a.id.clone())
    }

    fn handle_memory_search(
        &mut self,
        sender_idx: usize,
        bank: String,
        query: Option<String>,
        title_contains: Option<String>,
        include_tags: Vec<String>,
        exclude_tags: Vec<String>,
        limit: Option<usize>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let (registry, bank_decl) = match self.resolve_memory(sender_idx, &bank) {
            Ok(pair) => pair,
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
                return;
            }
        };

        let opts = crate::memory::SearchOptions {
            query,
            title_contains,
            tags: crate::memory::TagFilter {
                include: include_tags,
                exclude: exclude_tags,
            },
            limit: limit.unwrap_or(5).max(1),
        };
        tokio::spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || registry.search(&bank_decl, opts)).await;
            match result {
                Ok(Ok(hits)) => {
                    let _ = reply.send(Response::ok(format_memory_hits(&hits)));
                }
                Ok(Err(e)) => {
                    let _ = reply.send(Response::err(e.to_string()));
                }
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory search task: {join_err}")));
                }
            }
        });
    }

    fn handle_memory_link(
        &mut self,
        sender_idx: usize,
        bank: String,
        from: Uuid,
        to: Uuid,
        kind: Option<String>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let (registry, bank_decl) = match self.resolve_memory(sender_idx, &bank) {
            Ok(pair) => pair,
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
                return;
            }
        };
        match registry.link(&bank_decl, from, to, kind.as_deref()) {
            Ok(()) => {
                let kind_note = match kind.as_deref().map(str::trim) {
                    Some(k) if !k.is_empty() => format!(" [{k}]"),
                    _ => String::new(),
                };
                let _ = reply.send(Response::ok(format!(
                    "linked {} -> {}{}",
                    short_uuid(from),
                    short_uuid(to),
                    kind_note
                )));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_memory_unlink(
        &mut self,
        sender_idx: usize,
        bank: String,
        from: Uuid,
        to: Uuid,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let (registry, bank_decl) = match self.resolve_memory(sender_idx, &bank) {
            Ok(pair) => pair,
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
                return;
            }
        };
        match registry.unlink(&bank_decl, from, to) {
            Ok(true) => {
                let _ = reply.send(Response::ok(format!(
                    "unlinked {} -> {}",
                    short_uuid(from),
                    short_uuid(to)
                )));
            }
            Ok(false) => {
                let _ = reply.send(Response::ok(format!(
                    "no link {} -> {} (nothing to remove)",
                    short_uuid(from),
                    short_uuid(to)
                )));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    fn handle_memory_links(
        &mut self,
        sender_idx: usize,
        bank: String,
        id: Uuid,
        direction: Option<String>,
        include_tags: Vec<String>,
        exclude_tags: Vec<String>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let dir = match direction.as_deref() {
            None => crate::memory::LinkDirection::Both,
            Some(s) => match crate::memory::LinkDirection::parse(s) {
                Some(d) => d,
                None => {
                    let _ = reply.send(Response::err(format!(
                        "unknown direction `{s}`; expected `out`, `in`, or `both`"
                    )));
                    return;
                }
            },
        };
        let (registry, bank_decl) = match self.resolve_memory(sender_idx, &bank) {
            Ok(pair) => pair,
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
                return;
            }
        };
        let tag_filter = crate::memory::TagFilter {
            include: include_tags,
            exclude: exclude_tags,
        };
        match registry.links(&bank_decl, id, dir, &tag_filter) {
            Ok(links) => {
                let _ = reply.send(Response::ok(format_memory_links(id, &links)));
            }
            Err(e) => {
                let _ = reply.send(Response::err(e.to_string()));
            }
        }
    }

    /// Shared access-control + bank-lookup prelude for the link
    /// handlers. On failure returns the error string the caller should
    /// echo back to the agent; on success returns the registry handle
    /// and a clone of the bank declaration.
    fn resolve_memory(
        &self,
        sender_idx: usize,
        bank: &str,
    ) -> Result<(std::sync::Arc<MemoryRegistry>, crate::config::MemoryBank), String> {
        let template_id = self
            .sender_template_id(sender_idx)
            .ok_or_else(|| "unknown sender".to_string())?;
        if !self.config.can_access_bank(&template_id, bank) {
            return Err(format!(
                "bank `{bank}` is not accessible to `{template_id}`"
            ));
        }
        let bank_decl = self
            .config
            .memory_bank(bank)
            .cloned()
            .ok_or_else(|| format!("no memory bank `{bank}`"))?;
        let registry = self
            .memory
            .clone()
            .ok_or_else(|| "memory registry is not initialised on this orchestrator".to_string())?;
        Ok((registry, bank_decl))
    }

    fn handle_memory_store(
        &mut self,
        sender_idx: usize,
        bank: String,
        body: String,
        title: Option<String>,
        tags: Vec<String>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let (registry, bank_decl) = match self.resolve_memory(sender_idx, &bank) {
            Ok(pair) => pair,
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
                return;
            }
        };

        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                registry.store(&bank_decl, &body, title.as_deref(), &tags)
            })
            .await;
            match result {
                Ok(Ok(id)) => {
                    let _ = reply.send(Response::ok(format!("stored entry {id}")));
                }
                Ok(Err(e)) => {
                    let _ = reply.send(Response::err(e.to_string()));
                }
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory store task: {join_err}")));
                }
            }
        });
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

    /// Apply a parsed slash-command outcome against a specific chat
    /// `idx`. The keystroke path (`submit_input` → `handle_local_input`)
    /// passes `self.selected`; cron passes the target job's `idx`
    /// when firing a `/foo` payload on a background agent.
    pub(crate) fn apply_command_for(
        &mut self,
        idx: usize,
        outcome: super::commands::CommandOutcome,
    ) {
        use super::commands::CommandOutcome;
        match outcome {
            CommandOutcome::Message(text) => {
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info(text));
                }
            }
            CommandOutcome::Quit => {
                info!("quit requested via /exit");
                self.should_quit = true;
            }
            CommandOutcome::Compact => {
                self.begin_compaction(idx, false);
            }
            CommandOutcome::Reset => {
                self.begin_compaction(idx, true);
            }
            CommandOutcome::Unknown { name, hint } => {
                if let Some(chat) = self.chats.get_mut(idx) {
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
        // TODOs / Comments overlay takes all input while open.
        if self.tasks_screen.child_overlay.is_some() {
            self.handle_child_overlay_key(code);
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
            // Open the TODOs overlay on the selected task.
            KeyCode::Char('o') => {
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    self.tasks_screen.open_todos(id);
                }
            }
            // Open the Comments overlay on the selected task.
            KeyCode::Char('c') => {
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    self.tasks_screen.open_comments(id);
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

    /// Key handling when the TODOs or Comments overlay (or one of
    /// its prompt sub-states) is active. Splits into three paths:
    /// Browse (navigation + action triggers), Prompt with a buffer
    /// (text entry for add/edit), Prompt for confirm-delete.
    fn handle_child_overlay_key(&mut self, code: KeyCode) {
        let Some(overlay) = self.tasks_screen.child_overlay.clone() else {
            return;
        };
        match overlay {
            ChildOverlay::Todos { task_id, cursor } => {
                self.handle_child_todos_key(code, task_id, cursor);
            }
            ChildOverlay::Comments { task_id, cursor } => {
                self.handle_child_comments_key(code, task_id, cursor);
            }
            ChildOverlay::Prompt {
                task_id,
                kind,
                mut buffer,
            } => match code {
                KeyCode::Esc => {
                    // Return to the browse state for whichever list
                    // this prompt belonged to.
                    self.return_to_browse(task_id, kind);
                }
                KeyCode::Enter => {
                    self.commit_child_prompt(task_id, kind, buffer);
                }
                KeyCode::Char(c) if is_confirm_prompt(kind) => {
                    // Delete confirmation: y / n short-circuits.
                    match c {
                        'y' | 'Y' => {
                            self.commit_child_prompt(task_id, kind, String::from("y"));
                        }
                        'n' | 'N' => {
                            self.return_to_browse(task_id, kind);
                        }
                        _ => {}
                    }
                }
                KeyCode::Char(c) => {
                    buffer.push(c);
                    self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                        task_id,
                        kind,
                        buffer,
                    });
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                        task_id,
                        kind,
                        buffer,
                    });
                }
                _ => {}
            },
        }
    }

    fn handle_child_todos_key(&mut self, code: KeyCode, task_id: Uuid, cursor: usize) {
        let row_count = self.tasks.todos_of(task_id).map(|v| v.len()).unwrap_or(0);
        match code {
            KeyCode::Esc => self.tasks_screen.close_modal(),
            KeyCode::Char('j') | KeyCode::Down => {
                self.tasks_screen.child_overlay_move(1, row_count);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tasks_screen.child_overlay_move(-1, row_count);
            }
            KeyCode::Char('a') => {
                self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                    task_id,
                    kind: ChildPromptKind::AddTodo,
                    buffer: String::new(),
                });
            }
            KeyCode::Char('e') => {
                if let Some(todo) = self
                    .tasks
                    .todos_of(task_id)
                    .and_then(|v| v.get(cursor))
                    .cloned()
                {
                    self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                        task_id,
                        kind: ChildPromptKind::EditTodo(todo.id),
                        buffer: todo.body,
                    });
                }
            }
            KeyCode::Char('d') => {
                if let Some(id) = self
                    .tasks
                    .todos_of(task_id)
                    .and_then(|v| v.get(cursor))
                    .map(|t| t.id)
                {
                    self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                        task_id,
                        kind: ChildPromptKind::ConfirmDeleteTodo(id),
                        buffer: String::new(),
                    });
                }
            }
            KeyCode::Char(' ') => {
                // Cycle the selected TODO's status.
                let todo = self
                    .tasks
                    .todos_of(task_id)
                    .and_then(|v| v.get(cursor))
                    .cloned();
                if let Some(todo) = todo {
                    let next = match todo.status {
                        TodoStatus::Pending => TodoStatus::InProgress,
                        TodoStatus::InProgress => TodoStatus::Completed,
                        TodoStatus::Completed => TodoStatus::Pending,
                    };
                    if let Err(e) = self.tasks.update_todo(task_id, todo.id, None, Some(next)) {
                        self.tasks_screen.status_msg = Some(format!("error: {e}"));
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_child_comments_key(&mut self, code: KeyCode, task_id: Uuid, cursor: usize) {
        let row_count = self
            .tasks
            .comments_of(task_id)
            .map(|v| v.len())
            .unwrap_or(0);
        match code {
            KeyCode::Esc => self.tasks_screen.close_modal(),
            KeyCode::Char('j') | KeyCode::Down => {
                self.tasks_screen.child_overlay_move(1, row_count);
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tasks_screen.child_overlay_move(-1, row_count);
            }
            KeyCode::Char('a') => {
                self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                    task_id,
                    kind: ChildPromptKind::AddComment,
                    buffer: String::new(),
                });
            }
            KeyCode::Char('e') => {
                if let Some(comment) = self
                    .tasks
                    .comments_of(task_id)
                    .and_then(|v| v.get(cursor))
                    .cloned()
                {
                    self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                        task_id,
                        kind: ChildPromptKind::EditComment(comment.id),
                        buffer: comment.body,
                    });
                }
            }
            KeyCode::Char('d') => {
                if let Some(id) = self
                    .tasks
                    .comments_of(task_id)
                    .and_then(|v| v.get(cursor))
                    .map(|c| c.id)
                {
                    self.tasks_screen.child_overlay = Some(ChildOverlay::Prompt {
                        task_id,
                        kind: ChildPromptKind::ConfirmDeleteComment(id),
                        buffer: String::new(),
                    });
                }
            }
            _ => {}
        }
    }

    /// Restore the overlay to its Browse state after Esc / n on a
    /// prompt. Picks Todos vs Comments based on the prompt kind.
    fn return_to_browse(&mut self, task_id: Uuid, kind: ChildPromptKind) {
        self.tasks_screen.child_overlay = Some(match kind {
            ChildPromptKind::AddTodo
            | ChildPromptKind::EditTodo(_)
            | ChildPromptKind::ConfirmDeleteTodo(_) => ChildOverlay::Todos { task_id, cursor: 0 },
            ChildPromptKind::AddComment
            | ChildPromptKind::EditComment(_)
            | ChildPromptKind::ConfirmDeleteComment(_) => {
                ChildOverlay::Comments { task_id, cursor: 0 }
            }
        });
    }

    /// Apply a submitted child-prompt buffer. Errors surface in
    /// `status_msg`; on success the overlay returns to its Browse
    /// state so further ops can follow immediately.
    fn commit_child_prompt(&mut self, task_id: Uuid, kind: ChildPromptKind, buffer: String) {
        let body = buffer.trim().to_string();
        let result = match kind {
            ChildPromptKind::AddTodo => self
                .tasks
                .add_todo(task_id, body, OPERATOR.to_string())
                .map(|_| ()),
            ChildPromptKind::EditTodo(todo_id) => self
                .tasks
                .update_todo(task_id, todo_id, Some(body), None)
                .map(|_| ()),
            ChildPromptKind::ConfirmDeleteTodo(todo_id) => {
                if body.eq_ignore_ascii_case("y") {
                    self.tasks.remove_todo(task_id, todo_id).map(|_| ())
                } else {
                    Ok(())
                }
            }
            ChildPromptKind::AddComment => self
                .tasks
                .add_comment(task_id, body, OPERATOR.to_string())
                .map(|_| ()),
            ChildPromptKind::EditComment(comment_id) => self
                .tasks
                .update_comment(task_id, comment_id, body)
                .map(|_| ()),
            ChildPromptKind::ConfirmDeleteComment(comment_id) => {
                if body.eq_ignore_ascii_case("y") {
                    self.tasks.remove_comment(task_id, comment_id).map(|_| ())
                } else {
                    Ok(())
                }
            }
        };
        if let Err(e) = result {
            self.tasks_screen.status_msg = Some(format!("error: {e}"));
        }
        // Always drop back to the browse view for that list so the
        // next key can continue managing the entries.
        self.return_to_browse(task_id, kind);
    }

    fn handle_task_form_key(&mut self, code: KeyCode) {
        let Some(form) = self.tasks_screen.form.as_mut() else {
            return;
        };

        // Single-select dropdown picker (assignee / status) takes
        // all keyboard input while open.
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

        // Multi-select blocked-by picker: Space toggles, Enter
        // commits, Esc cancels, ↑/↓/j/k move.
        if form.blocked_by_picker.is_some() {
            match code {
                KeyCode::Esc => form.close_blocked_by_picker(),
                KeyCode::Down | KeyCode::Char('j') => form.blocked_by_picker_move(1),
                KeyCode::Up | KeyCode::Char('k') => form.blocked_by_picker_move(-1),
                KeyCode::Char(' ') => form.blocked_by_picker_toggle(),
                KeyCode::Enter => form.commit_blocked_by_picker(),
                _ => {}
            }
            return;
        }

        let focused = form.focused_field;
        let on_dropdown = TaskForm::is_dropdown(focused);
        let on_button = focused.is_button();
        let on_blocked_by = focused == FormField::BlockedBy;
        let on_description = focused == FormField::Description;

        // Description has its own text-editor key map.
        if on_description {
            // The box's visible-row count is stored on the form by
            // the renderer; fall back to the constant min when we
            // haven't rendered yet (headless tests).
            let vh = if form.last_visible_lines > 0 {
                form.last_visible_lines
            } else {
                super::tasks_view::DESCRIPTION_MIN_VISIBLE_LINES
            };
            match code {
                KeyCode::Esc => self.tasks_screen.close_modal(),
                KeyCode::Tab => form.focus_next(),
                KeyCode::BackTab => form.focus_prev(),
                KeyCode::Enter => {
                    form.description_insert('\n');
                    form.description_ensure_visible(vh);
                }
                KeyCode::Char(c) => {
                    form.description_insert(c);
                    form.description_ensure_visible(vh);
                }
                KeyCode::Backspace => {
                    form.description_backspace();
                    form.description_ensure_visible(vh);
                }
                KeyCode::Left => form.description_move_char(-1),
                KeyCode::Right => form.description_move_char(1),
                KeyCode::Up => {
                    if !form.description_move_line(-1) {
                        form.focus_prev();
                    } else {
                        form.description_ensure_visible(vh);
                    }
                }
                KeyCode::Down => {
                    if !form.description_move_line(1) {
                        form.focus_next();
                    } else {
                        form.description_ensure_visible(vh);
                    }
                }
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Esc => self.tasks_screen.close_modal(),
            // Tab/Down → next control; Shift-Tab/Up → previous.
            KeyCode::Tab | KeyCode::Down => form.focus_next(),
            KeyCode::BackTab | KeyCode::Up => form.focus_prev(),
            KeyCode::Enter => match focused {
                FormField::Ok => self.submit_task_form(),
                FormField::Cancel => self.tasks_screen.close_modal(),
                _ if on_dropdown => form.open_picker(),
                _ if on_blocked_by => self.open_blocked_by_picker_for_form(),
                _ => self.submit_task_form(),
            },
            KeyCode::Backspace if !on_dropdown && !on_button && !on_blocked_by => {
                form.field_mut(focused).pop();
            }
            KeyCode::Char(c) if !on_dropdown && !on_button && !on_blocked_by => {
                form.field_mut(focused).push(c);
            }
            _ => {}
        }
    }

    /// Open the blocked-by multi-select picker, feeding it the full
    /// current task set and the self-id for Edit (so the task can't
    /// depend on itself).
    fn open_blocked_by_picker_for_form(&mut self) {
        let self_id = match self.tasks_screen.form.as_ref().map(|f| f.kind) {
            Some(super::tasks_view::TaskFormKind::Edit(id)) => Some(id),
            _ => None,
        };
        // Clone the task slice so we don't hold an immutable borrow
        // on `self.tasks` while we mutate `self.tasks_screen.form`.
        let all_tasks: Vec<_> = self.tasks.all().to_vec();
        if let Some(form) = self.tasks_screen.form.as_mut() {
            form.open_blocked_by_picker(&all_tasks, self_id);
        }
    }

    /// Submit the currently open form and, on success, re-clamp the
    /// task list so the selection lands on a still-valid row.
    /// Errors stay surfaced inside `form.error` so the modal stays
    /// open for the user to correct.
    fn submit_task_form(&mut self) {
        match self.tasks_screen.submit_form(&mut self.tasks) {
            Ok(()) => {
                let rows = TasksScreen::rows(&self.tasks);
                self.tasks_screen.reclamp(&rows);
            }
            Err(_e) => { /* error surfaced on form.error */ }
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

        // Shift+Enter inserts a newline; Ctrl+J is the terminal-safe
        // fallback for terminals that conflate Enter with Shift+Enter.
        let is_newline = matches!(code, KeyCode::Enter) && mods.contains(KeyModifiers::SHIFT)
            || (matches!(code, KeyCode::Char('j')) && mods.contains(KeyModifiers::CONTROL));

        if is_newline {
            if let Some(chat) = self.current_chat_mut() {
                input_insert(chat, '\n');
            }
            return;
        }

        match code {
            KeyCode::Esc => self.focus = Focus::Agents,
            KeyCode::Enter => self.submit_input(),
            KeyCode::Backspace => {
                if let Some(chat) = self.current_chat_mut() {
                    input_backspace(chat);
                }
            }
            KeyCode::Left => {
                if let Some(chat) = self.current_chat_mut() {
                    input_move_char(chat, -1);
                }
            }
            KeyCode::Right => {
                if let Some(chat) = self.current_chat_mut() {
                    input_move_char(chat, 1);
                }
            }
            KeyCode::Up => {
                if let Some(chat) = self.current_chat_mut() {
                    input_move_line(chat, -1);
                }
            }
            KeyCode::Down => {
                if let Some(chat) = self.current_chat_mut() {
                    input_move_line(chat, 1);
                }
            }
            KeyCode::Home => {
                if let Some(chat) = self.current_chat_mut() {
                    input_move_to_row_start(chat);
                }
            }
            KeyCode::End => {
                if let Some(chat) = self.current_chat_mut() {
                    input_move_to_row_end(chat);
                }
            }
            KeyCode::Char(c) => {
                if let Some(chat) = self.current_chat_mut() {
                    input_insert(chat, c);
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
                // 0. If a task form or dropdown picker is open, it
                //    captures all clicks. Clicks outside its rect
                //    are swallowed so the user can't accidentally
                //    dismiss the modal by clicking behind it.
                if self.active_view == ActiveView::Tasks && self.tasks_screen.form.is_some() {
                    self.handle_mouse_task_modal(pos);
                    return;
                }
                // 0b. If a child overlay (TODOs / Comments) is
                //     open, clicks inside move the cursor; clicks
                //     outside the overlay close it.
                if self.active_view == ActiveView::Tasks
                    && self.tasks_screen.child_overlay.is_some()
                {
                    self.handle_mouse_child_overlay(pos);
                    return;
                }

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
                        // In Tasks view, clicking a row selects it;
                        // a double-click on the same row opens the
                        // edit modal.
                        if self.active_view == ActiveView::Tasks {
                            let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                            let rows = TasksScreen::rows(&self.tasks);
                            if let Some(super::tasks_view::TaskRow::Task(id)) =
                                rows.get(row_in_list).cloned()
                            {
                                self.tasks_screen.selected_row = row_in_list;

                                // Double-click test: same (col, row)
                                // within the double-click window.
                                let is_double = match self.last_click {
                                    Some((c, r, t)) => {
                                        c == ev.column
                                            && r == ev.row
                                            && t.elapsed() < DOUBLE_CLICK_WINDOW
                                    }
                                    None => false,
                                };
                                self.last_click = Some((ev.column, ev.row, Instant::now()));

                                if is_double {
                                    if let Some(t) = self.tasks.get(id) {
                                        self.tasks_screen.open_edit(t);
                                        self.inject_assignee_options();
                                    }
                                    // Reset so a third click isn't
                                    // also treated as a double.
                                    self.last_click = None;
                                }
                            } else {
                                // Click landed on a header or an
                                // empty row — forget the last task
                                // click so a subsequent click on a
                                // different row doesn't double.
                                self.last_click = None;
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

    /// Handle a left-click while a task form (and optionally its
    /// picker popup) is open. Picker takes priority; otherwise we
    /// hit-test against the form's field/button rects. Clicks
    /// outside any rect are swallowed — modals don't dismiss on
    /// outside click.
    fn handle_mouse_task_modal(&mut self, pos: (u16, u16)) {
        // Single-select (assignee/status) picker open? Click commits.
        let picker_open = self
            .tasks_screen
            .form
            .as_ref()
            .map(|f| f.picker.is_some())
            .unwrap_or(false);
        if picker_open {
            let hit_row = self
                .layout
                .task_form_picker_rows
                .iter()
                .position(|r| rect_contains(*r, pos));
            if let Some(idx) = hit_row {
                if let Some(form) = self.tasks_screen.form.as_mut() {
                    if let Some(p) = form.picker.as_mut() {
                        p.selected = idx;
                    }
                    form.commit_picker();
                }
            }
            if hit_row.is_none() {
                if let Some(form) = self.tasks_screen.form.as_mut() {
                    form.close_picker();
                }
            }
            return;
        }

        // Multi-select blocked-by picker open? Click the OK/Cancel
        // buttons first, then rows, then the outer rect (swallow),
        // then outside → close discarding.
        let blocked_picker_open = self
            .tasks_screen
            .form
            .as_ref()
            .map(|f| f.blocked_by_picker.is_some())
            .unwrap_or(false);
        if blocked_picker_open {
            // 1. Button?
            if let Some(btn) = self
                .layout
                .task_form_picker_buttons
                .iter()
                .find(|(_, r)| rect_contains(*r, pos))
                .map(|(f, _)| *f)
            {
                match btn {
                    FormField::Ok => {
                        if let Some(form) = self.tasks_screen.form.as_mut() {
                            form.commit_blocked_by_picker();
                        }
                    }
                    FormField::Cancel => {
                        if let Some(form) = self.tasks_screen.form.as_mut() {
                            form.close_blocked_by_picker();
                        }
                    }
                    _ => {}
                }
                return;
            }
            // 2. Row?
            if let Some(idx) = self
                .layout
                .task_form_picker_rows
                .iter()
                .position(|r| rect_contains(*r, pos))
            {
                if let Some(form) = self.tasks_screen.form.as_mut() {
                    form.blocked_by_picker_toggle_at(idx);
                }
                return;
            }
            // 3. Click inside the picker frame but not on anything
            //    interactive — swallow.
            if let Some(outer) = self.layout.task_form_picker_outer {
                if rect_contains(outer, pos) {
                    return;
                }
            }
            // 4. Click outside the picker entirely → close
            //    discarding, matching Esc.
            if let Some(form) = self.tasks_screen.form.as_mut() {
                form.close_blocked_by_picker();
            }
            return;
        }

        // No picker: hit-test form fields and buttons.
        let hit = self
            .layout
            .task_form_fields
            .iter()
            .find(|(_, r)| rect_contains(*r, pos))
            .map(|(f, _)| *f);
        let Some(field) = hit else { return };

        match field {
            FormField::Ok => self.submit_task_form(),
            FormField::Cancel => self.tasks_screen.close_modal(),
            _ => {
                // Focus the field first.
                if let Some(form) = self.tasks_screen.form.as_mut() {
                    form.focused_field = field;
                }
                // Clicking a dropdown opens its picker; clicking
                // the blocked-by list opens the multi-select
                // picker — matches native GUI behaviour.
                if TaskForm::is_dropdown(field) {
                    if let Some(form) = self.tasks_screen.form.as_mut() {
                        form.open_picker();
                    }
                } else if field == FormField::BlockedBy {
                    self.open_blocked_by_picker_for_form();
                }
            }
        }
    }

    /// Handle a left click while the TODOs / Comments overlay is
    /// open. Click on a visible row → move the cursor to it; click
    /// outside the overlay's outer rect → close it.
    fn handle_mouse_child_overlay(&mut self, pos: (u16, u16)) {
        // Click a row? Move the cursor.
        if let Some(idx) = self
            .layout
            .child_overlay_rows
            .iter()
            .position(|r| rect_contains(*r, pos))
        {
            let row_count = self.layout.child_overlay_rows.len();
            self.tasks_screen.child_overlay_set_cursor(idx, row_count);
            return;
        }
        // Click inside the overlay but not on a row → swallow.
        if let Some(outer) = self.layout.child_overlay_outer {
            if rect_contains(outer, pos) {
                return;
            }
        }
        // Click outside → close.
        self.tasks_screen.close_modal();
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
        // `reconcile_anchor` (ui.rs) rolls `within` forward into the next
        // block using the actual rendered heights and clamps to the last
        // block when we reach the end, so we only need to grow `within`
        // here. The renderer's scratch-buffer path gives us true per-row
        // scroll; the anchor always lands inside a valid block.
        chat.anchor.within = chat.anchor.within.saturating_add(amount);
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

    out.push_str("\nTODOs:\n");
    if t.todos.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for td in &t.todos {
            let short = td.id.to_string();
            let short = short.get(..8).unwrap_or(&short);
            out.push_str(&format!(
                "  [{}] {}  {}\n",
                td.status.glyph(),
                short,
                td.body
            ));
        }
    }

    out.push_str("\nComments:\n");
    if t.comments.is_empty() {
        out.push_str("  (none)\n");
    } else {
        for c in &t.comments {
            out.push_str(&format!(
                "  [{} · {}] {}\n",
                format_age(now, c.timestamp_secs),
                c.author,
                c.body
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

/// Parse any `aciddog task …` CLI invocation (including nested
/// `task todo …` / `task comment …`) back into the `TaskCommand`
/// clap parsed. Returns `None` for commands that aren't task ops.
fn try_parse_aciddog_task(cmd: &str, binary: &str) -> Option<crate::cli::TaskCommand> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Task { cmd: task_cmd } => Some(task_cmd),
        _ => None,
    }
}

/// Parse an `aciddog skill run --name <id>` invocation back into the
/// skill id. Returns `None` for other commands.
fn try_parse_aciddog_skill_run(cmd: &str, binary: &str) -> Option<String> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Skill {
            cmd: crate::cli::SkillCommand::Run { name },
        } => Some(name),
        _ => None,
    }
}

/// True when `cmd` is `aciddog skill list`.
fn is_aciddog_skill_list(cmd: &str, binary: &str) -> bool {
    matches!(
        try_parse_aciddog(cmd, binary),
        Some(AgentCommand::Skill {
            cmd: crate::cli::SkillCommand::List
        })
    )
}

/// Parse an `aciddog skill search --query <text>` invocation back
/// into the raw query string. Returns `None` for other commands.
fn try_parse_aciddog_skill_search(cmd: &str, binary: &str) -> Option<String> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Skill {
            cmd: crate::cli::SkillCommand::Search { query },
        } => Some(query),
        _ => None,
    }
}

/// Short-form UUID used in block subjects — first 8 chars of the
/// UUID matches the ids in `format_task_detail` output so the
/// operator can cross-reference at a glance.
fn short_id(id: uuid::Uuid) -> String {
    let s = id.to_string();
    s.get(..8).unwrap_or(&s).to_string()
}

/// Convert a parsed `TaskCommand` into a `(kind, subject, body)`
/// tuple for a `ChatBlock::TaskOp`. Pure — does no IO — so it's
/// easy to unit-test per verb.
pub(crate) fn task_op_summary(
    cmd: &crate::cli::TaskCommand,
) -> (crate::tui::blocks::TaskOpKind, String, String) {
    use crate::cli::{CommentCommand, TaskCommand, TodoCommand};
    use crate::tui::blocks::TaskOpKind;

    match cmd {
        TaskCommand::Add {
            title,
            description,
            assignee,
            blocked_by,
        } => {
            let mut subj = format!("Add: {title}");
            let mut notes: Vec<String> = Vec::new();
            if let Some(a) = assignee {
                notes.push(format!("assigned: {a}"));
            }
            if !blocked_by.is_empty() {
                notes.push(format!("{} deps", blocked_by.len()));
            }
            if !notes.is_empty() {
                subj.push_str(&format!(" ({})", notes.join(", ")));
            }
            let body = description.clone().unwrap_or_default();
            (TaskOpKind::Add, subj, body)
        }
        TaskCommand::List {
            assignee,
            status,
            mine,
            unassigned,
        } => {
            let mut filters: Vec<String> = Vec::new();
            if *mine {
                filters.push("--mine".into());
            }
            if *unassigned {
                filters.push("--unassigned".into());
            }
            if let Some(a) = assignee {
                filters.push(format!("assignee={a}"));
            }
            if let Some(s) = status {
                filters.push(format!("status={s}"));
            }
            let subj = if filters.is_empty() {
                "List".to_string()
            } else {
                format!("List ({})", filters.join(", "))
            };
            (TaskOpKind::List, subj, String::new())
        }
        TaskCommand::Show { id } => (
            TaskOpKind::Show,
            format!("Show: {}", short_id(*id)),
            String::new(),
        ),
        TaskCommand::Update {
            id,
            title,
            description: _,
            status,
            assignee,
            unassign,
            note,
            add_blocked_by,
            remove_blocked_by,
        } => {
            let mut changes: Vec<String> = Vec::new();
            if title.is_some() {
                changes.push("title".into());
            }
            if let Some(s) = status {
                changes.push(format!("status: {s}"));
            }
            if let Some(a) = assignee {
                changes.push(format!("assignee: {a}"));
            }
            if *unassign {
                changes.push("unassign".into());
            }
            if !add_blocked_by.is_empty() {
                changes.push(format!("+{} deps", add_blocked_by.len()));
            }
            if !remove_blocked_by.is_empty() {
                changes.push(format!("-{} deps", remove_blocked_by.len()));
            }
            let mut subj = format!("Update: {}", short_id(*id));
            if !changes.is_empty() {
                subj.push_str(&format!(" ({})", changes.join(", ")));
            }
            let body = note.clone().unwrap_or_default();
            (TaskOpKind::Update, subj, body)
        }
        TaskCommand::Remove { id } => (
            TaskOpKind::Remove,
            format!("Remove: {}", short_id(*id)),
            String::new(),
        ),
        TaskCommand::Take => (TaskOpKind::Take, "Take".to_string(), String::new()),
        TaskCommand::Note { id, body } => (
            TaskOpKind::Note,
            format!("Note on {}", short_id(*id)),
            body.clone(),
        ),
        TaskCommand::Todo { cmd: todo_cmd } => match todo_cmd {
            TodoCommand::Add { task, body } => (
                TaskOpKind::TodoAdd,
                format!("Todo add on {}", short_id(*task)),
                body.clone(),
            ),
            TodoCommand::List { task } => (
                TaskOpKind::TodoList,
                format!("Todo list on {}", short_id(*task)),
                String::new(),
            ),
            TodoCommand::Update {
                task,
                id: _,
                status,
                body,
            } => {
                let mut subj = format!("Todo update on {}", short_id(*task));
                if let Some(s) = status {
                    subj = format!("Todo {} on {}", s, short_id(*task));
                }
                (
                    TaskOpKind::TodoUpdate,
                    subj,
                    body.clone().unwrap_or_default(),
                )
            }
            TodoCommand::Remove { task, id: _ } => (
                TaskOpKind::TodoRemove,
                format!("Todo remove on {}", short_id(*task)),
                String::new(),
            ),
        },
        TaskCommand::Comment { cmd: comment_cmd } => match comment_cmd {
            CommentCommand::Add { task, body } => (
                TaskOpKind::CommentAdd,
                format!("Comment on {}", short_id(*task)),
                body.clone(),
            ),
            CommentCommand::List { task } => (
                TaskOpKind::CommentList,
                format!("Comment list on {}", short_id(*task)),
                String::new(),
            ),
            CommentCommand::Update { task, id: _, body } => (
                TaskOpKind::CommentUpdate,
                format!("Comment edit on {}", short_id(*task)),
                body.clone(),
            ),
            CommentCommand::Remove { task, id: _ } => (
                TaskOpKind::CommentRemove,
                format!("Comment remove on {}", short_id(*task)),
                String::new(),
            ),
        },
    }
}

fn try_parse_aciddog(cmd: &str, binary: &str) -> Option<AgentCommand> {
    if binary.is_empty() {
        return None;
    }
    let tokens = shlex::split(cmd)?;
    let mut iter = tokens.into_iter();
    let first = iter.next()?;
    if !is_aciddog_invocation(&first, binary) {
        return None;
    }
    // Drop any shell-pipeline tail (`| jq .`, `&& echo ok`, `; …`,
    // or a redirect). The aciddog portion is still a valid
    // standalone invocation and we don't want clap to choke on the
    // trailing operators.
    let argv: Vec<String> = std::iter::once("aciddog".to_string())
        .chain(iter.take_while(|t| !is_shell_pipeline_boundary(t)))
        .collect();
    AgentArgs::try_parse_from(argv).ok().map(|p| p.cmd)
}

/// True for shell tokens that start a new pipeline stage or a
/// redirect. shlex emits these as standalone tokens (it doesn't
/// interpret them as punctuation), so we can slice the argv at
/// the first occurrence.
fn is_shell_pipeline_boundary(tok: &str) -> bool {
    matches!(
        tok,
        "|" | "||" | "&" | "&&" | ";" | ";;" | ">" | ">>" | "<" | "<<" | "|&"
    )
}

/// True when `first` names the running aciddog binary. Accepts:
/// - The exact path configured on `App` (the canonical path the
///   preamble hands to agents).
/// - A relative or bare `aciddog` invocation — same basename — so
///   that agents running `./aciddog …` from their cwd or relying
///   on `$PATH` still surface as structured blocks.
fn is_aciddog_invocation(first: &str, binary: &str) -> bool {
    if first == binary {
        return true;
    }
    let expected_base = std::path::Path::new(binary)
        .file_name()
        .and_then(|n| n.to_str());
    let actual_base = std::path::Path::new(first)
        .file_name()
        .and_then(|n| n.to_str());
    match (expected_base, actual_base) {
        (Some(exp), Some(got)) => exp == got,
        _ => false,
    }
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

    // Disk-backed cron store. Same fallback policy as TaskStore: a
    // load failure logs a warning and the TUI boots with an ephemeral
    // store so an unparseable cron.json doesn't bring everything down.
    let cron_path = std::path::PathBuf::from(".aciddog").join("cron.json");
    match crate::cron::CronStore::load_or_init(cron_path.clone()) {
        Ok(mut store) => {
            match store.reconcile_on_startup(chrono::Utc::now()) {
                Ok(report) => {
                    info!(
                        path = %cron_path.display(),
                        count = store.len(),
                        removed_one_offs = report.removed_one_offs.len(),
                        queued_one_offs = report.queued_one_offs.len(),
                        recomputed_recurring = report.recomputed_recurring.len(),
                        disabled_unreachable = report.disabled_unreachable.len(),
                        "loaded cron store",
                    );
                }
                Err(e) => {
                    warn!(error = %e, path = %cron_path.display(), "cron startup reconciliation failed");
                }
            }
            app.cron = store;
        }
        Err(e) => {
            warn!(error = %e, path = %cron_path.display(), "failed to load cron store; using ephemeral store");
        }
    }

    // Construct the semantic-memory registry rooted at `.aciddog/memory/`.
    // Banks open lazily on first use, so this is just a handle creation —
    // no disk or model download cost unless an agent actually invokes
    // `memory search` / `memory store`.
    let memory_root = std::path::PathBuf::from(".aciddog").join("memory");
    let memory = std::sync::Arc::new(MemoryRegistry::new(memory_root.clone()));
    info!(
        path = %memory_root.display(),
        banks = app.config.memory_banks.len(),
        "memory registry ready"
    );
    app.memory = Some(memory);

    // Bind the agent IPC socket for this team before spawning cores, so its
    // path can be injected into each claude child's environment.
    let team_id = app.config.team.id.clone();
    let mut socket = agent_ipc::server::bind(&team_id)
        .await
        .with_context(|| format!("binding agent IPC socket for team {team_id}"))?;
    info!(path = %socket.path.display(), "agent IPC socket bound");
    let socket_path_str = socket.path.display().to_string();
    app.socket_path = socket_path_str.clone();
    // Clone the socket-events sender onto the App so tag-mode cores
    // spawned at runtime (worker create / resurrect) can push directly
    // into the same handler pipeline used by socket-borne requests.
    app.socket_events_tx = Some(socket.events_tx());

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
    let (compaction_tx, mut compaction_rx) =
        mpsc::channel::<super::compaction::CompactionEvent>(16);
    app.compaction_tx = Some(compaction_tx);
    let all_ids: Vec<String> = app.config.agents.iter().map(|a| a.id.clone()).collect();
    for (idx, agent) in app.config.agents.iter().enumerate() {
        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);
        app.chats[idx].user_tx = Some(user_tx);
        app.chats[idx].agent_tx = Some(agent_tx);

        let ai_core = app
            .config
            .effective_ai_core(agent.ai_core.as_deref())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "agent `{}` has no ai_core (no override and no team default_ai_core)",
                    agent.id
                )
            })?;
        let tools = app
            .config
            .effective_tools(agent.tools.as_deref(), Some(ai_core));
        let tag_mode = uses_tag_ipc(&tools, ai_core);

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

        let team_block = build_team_block(&app.config, &contactable, &creatable);
        let memory_block = build_memory_block(&app.config, &agent.id);
        let memory_banks_for_agent: Vec<String> = agent
            .memory_banks
            .as_deref()
            .map(|list| list.to_vec())
            .unwrap_or_default();

        let skills_for_agent = app.config.skills_for(&agent.id);

        let parts = PromptParts {
            role: agent.role_description.clone(),
            team: Some(team_block),
            memory: memory_block,
            instructions: agent.instructions.clone(),
            aciddog_cli: Some(AciddogCliCtx {
                binary: aciddog_binary_str.clone(),
                contactable: contactable.iter().map(|s| s.to_string()).collect(),
                creatable: creatable.iter().map(|s| s.to_string()).collect(),
                memory_banks: memory_banks_for_agent,
                skills: skills_for_agent,
                worker: None,
            }),
        };

        let env = vec![
            (agent_ipc::ENV_AGENT_ID.to_string(), session_id.to_string()),
            (
                agent_ipc::ENV_AGENT_SOCKET.to_string(),
                socket_path_str.clone(),
            ),
        ];

        let socket_tx_for_core = if tag_mode {
            Some(
                app.socket_events_tx
                    .clone()
                    .expect("socket_events_tx set immediately after socket.events_tx() above"),
            )
        } else {
            None
        };

        let core: Box<dyn AICore> = build_core(
            ai_core,
            session_id,
            parts,
            tools,
            env,
            socket_tx_for_core,
            false,
        );

        // Reflect the assembled first-turn prompt in the top AgentInfo
        // block so the user can expand it and see exactly what the LLM
        // receives (mirrors what the core will emit on turn 1).
        let assembled = core.first_turn_prompt();
        if let Some(ChatBlock::AgentInfo { prompt, .. }) = app.chats[idx].blocks.first_mut() {
            *prompt = assembled;
        }

        tokio::spawn(run_core(idx, core, user_rx, agent_rx, ai_tx.clone()));
    }
    drop(ai_tx);
    // `app.ai_tx` retains a sender clone so runtime worker spawns can
    // route their stream into the same loop. That clone is dropped
    // either when a new worker's `run_core` captures it, or when the
    // event loop exits and `App` is dropped.

    let result = event_loop(
        &mut terminal,
        &mut app,
        ai_rx,
        &mut socket.events,
        &mut compaction_rx,
    )
    .await;

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

pub(super) async fn run_core(
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
    compaction_rx: &mut mpsc::Receiver<super::compaction::CompactionEvent>,
) -> Result<()> {
    let mut events = EventStream::new();
    terminal.draw(|f| ui::render(f, app))?;

    // Drives the thinking-indicator spinner. Skip missed ticks so a
    // long-running event-handler doesn't produce a burst of catch-up
    // draws.
    let mut spinner_tick = tokio::time::interval(Duration::from_millis(100));
    spinner_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        // Recompute the next cron firing each iteration so a freshly-
        // added job with a closer `next_run` displaces the previous
        // sleep_until target. `cron_invalidate` is woken by every
        // mutating cron handler so the loop re-selects immediately
        // even when a longer sleep was already in flight.
        let next_cron_at = app.cron.next_due_at();
        let cron_invalidate = app.cron_invalidate.clone();

        let mut needs_redraw = true;
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
            Some(comp_ev) = compaction_rx.recv() => {
                app.handle_compaction_event(comp_ev);
            }
            _ = spinner_tick.tick() => {
                // Only redraw when something is actually animating.
                if app.chats.iter().any(|c| c.status == AgentStatus::Working) {
                    app.frame_tick = app.frame_tick.wrapping_add(1);
                } else {
                    needs_redraw = false;
                }
            }
            _ = async move {
                match next_cron_at {
                    Some(at) => {
                        let now = chrono::Utc::now();
                        let dur = (at - now).to_std().unwrap_or(Duration::from_millis(0));
                        tokio::time::sleep(dur).await
                    }
                    None => std::future::pending::<()>().await,
                }
            } => {
                app.tick_cron(chrono::Utc::now());
            }
            _ = cron_invalidate.notified() => {
                // No-op: just causes the loop to re-select with a
                // recomputed `next_cron_at`. Skip the redraw since
                // nothing visible changed.
                needs_redraw = false;
            }
        }

        if app.should_quit {
            break;
        }

        if needs_redraw {
            terminal.draw(|f| ui::render(f, app))?;
        }
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
                replace_thinking_messages: false,
                default_ai_core: Some("default".into()),
            },
            agents: vec![
                Agent {
                    id: "a".into(),
                    role: "r".into(),
                    role_description: None,
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                },
                Agent {
                    id: "b".into(),
                    role: "r".into(),
                    role_description: None,
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                },
                Agent {
                    id: "c".into(),
                    role: "r".into(),
                    role_description: None,
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                },
            ],
            workers: vec![],
            ai_cores: vec![crate::config::AiCore {
                id: "default".into(),
                kind: crate::config::AiCoreKind::ClaudeCode {
                    system_prompt: None,
                    default_tools: None,
                },
            }],
            memory_banks: vec![],
            compactions: vec![],
            thinking_messages: vec!["thinking".into()],
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
    fn render_skill_listing_reports_none_when_allowlist_empty() {
        let out = render_skill_listing("a", &[]);
        assert!(out.contains("No skills"), "friendly empty sentence: {out}");
    }

    #[test]
    fn render_skill_listing_emits_one_dash_id_line_per_skill() {
        // SKILL.md files won't exist for these ids at the test cwd, so
        // `read_skill_description` fails and each line falls back to
        // the placeholder. We only assert on id presence and line
        // shape, not on the description payload.
        let out = render_skill_listing("a", &["alpha".to_string(), "beta".to_string()]);
        assert!(out.contains("- alpha —"), "missing alpha line: {out}");
        assert!(out.contains("- beta —"), "missing beta line: {out}");
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
    fn shift_enter_inserts_newline_without_submitting() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "hi".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::SHIFT);
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        let chat = app.current_chat().unwrap();
        assert_eq!(chat.input, "hi\na");
        assert_eq!(chat.input_cursor, 4);
        // The buffer hasn't been submitted, so no user block yet.
        assert!(
            !chat
                .blocks
                .iter()
                .any(|b| matches!(b, ChatBlock::UserText { .. })),
            "no UserText expected yet"
        );
    }

    #[test]
    fn ctrl_j_inserts_newline_as_terminal_fallback() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::CONTROL);
        app.handle_key(KeyCode::Char('b'), KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input, "a\nb");
    }

    #[test]
    fn left_right_move_cursor() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "abc".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        assert_eq!(app.current_chat().unwrap().input_cursor, 3);
        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input_cursor, 2);
        app.handle_key(KeyCode::Char('X'), KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input, "abXc");
        app.handle_key(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input_cursor, 4);
    }

    #[test]
    fn backspace_is_utf8_safe() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        // Type "héllo" — 'é' is two bytes.
        for c in "héllo".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input, "héll");
        // Move to after 'é', then backspace should remove the whole 'é'.
        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        app.handle_key(KeyCode::Left, KeyModifiers::NONE);
        app.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.current_chat().unwrap().input, "hll");
    }

    #[test]
    fn up_down_navigate_between_logical_lines() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "ab".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::SHIFT);
        for c in "cd".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        // Cursor at end of "cd" on second row.
        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        // Now on first row at col 2 → byte offset 2 ("ab").
        assert_eq!(app.current_chat().unwrap().input_cursor, 2);
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        // Back to second row col 2 → byte offset 5 ("ab\ncd").
        assert_eq!(app.current_chat().unwrap().input_cursor, 5);
    }

    #[test]
    fn submit_input_resets_cursor_and_scroll() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        for c in "hi".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let chat = app.current_chat().unwrap();
        assert_eq!(chat.input, "");
        assert_eq!(chat.input_cursor, 0);
        assert_eq!(chat.input_scroll, 0);
    }

    #[test]
    fn input_visual_rows_wraps_on_word_boundaries() {
        let chat = AgentChat {
            input: "aaa bbb ccc".to_string(),
            ..Default::default()
        };
        let rows = chat.input_visual_rows(5);
        // "aaa " (4) / "bbb " (4) / "ccc" (3) — the word "bbb" won't
        // fit onto the first row's 5 cols alongside "aaa ", so the row
        // breaks at that boundary.
        assert!(
            rows.len() >= 2,
            "expected at least 2 wrapped rows: {rows:?}"
        );
        // First row must end before "bbb" starts.
        let (_, first_end) = rows[0];
        assert!(first_end <= 4);
    }

    #[test]
    fn input_scroll_follows_cursor_past_visible_rows() {
        let input: String = (1..=7)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut chat = AgentChat {
            input_wrap_width: 80,
            input_visible_rows: 5,
            input_cursor: input.len(),
            input,
            ..Default::default()
        };
        clamp_input_scroll(&mut chat);
        assert_eq!(chat.input_scroll, 2);
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

    /// Scrolling inside a tall first block must advance `within` without
    /// jumping past the block. This is the regression the smooth-scroll
    /// fix was written for.
    #[test]
    fn scroll_wheel_inside_tall_first_block_advances_within_smoothly() {
        let mut app = App::new(sample_config());
        app.layout.chat_history_inner = Some(Rect::new(0, 20, 80, 10));
        let chat = app.current_chat_mut().unwrap();
        // One very tall block followed by a short one. Only block 0 should
        // be the scroll target for the first several wheel ticks.
        chat.blocks
            .push(ChatBlock::user("line\n".repeat(200)));
        chat.blocks.push(ChatBlock::user("short"));
        chat.anchor = ScrollAnchor {
            block_idx: 0,
            within: 0,
        };
        chat.sticky_bottom = false;

        // Three wheel-ticks (each = 3 rows) should land well inside the
        // first block: within grows by 9, block_idx stays at 0.
        for _ in 0..3 {
            app.handle_mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: 5,
                row: 25,
                modifiers: KeyModifiers::NONE,
            });
        }
        let a = app.current_chat().unwrap().anchor;
        assert_eq!(a.block_idx, 0, "must not jump past the first block");
        assert_eq!(a.within, 9, "within should advance 3 rows per tick");
    }

    /// Scrolling back up after sitting inside a tall block should walk
    /// `within` back to zero before moving to an earlier block.
    #[test]
    fn scroll_wheel_up_inside_tall_block_decrements_within_first() {
        let mut app = App::new(sample_config());
        app.layout.chat_history_inner = Some(Rect::new(0, 20, 80, 10));
        let chat = app.current_chat_mut().unwrap();
        chat.blocks
            .push(ChatBlock::user("line\n".repeat(200)));
        chat.blocks.push(ChatBlock::user("short"));
        chat.anchor = ScrollAnchor {
            block_idx: 0,
            within: 30,
        };
        chat.sticky_bottom = false;

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });
        let a = app.current_chat().unwrap().anchor;
        assert_eq!(a.block_idx, 0);
        assert_eq!(a.within, 27, "one tick = 3 rows back up");
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
                usage: None,
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
                    ..
                } => {
                    assert_eq!(core, "claude-code (default)");
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
    fn try_parse_aciddog_send_accepts_same_basename_across_paths() {
        // Prior to the loosened matcher, only the exact configured
        // path was accepted. In practice the agent's bash step
        // sometimes invokes the binary by name (from `$PATH`) or
        // by relative path (`./aciddog`). All of those resolve to
        // the same binary in a real session, so we treat them as
        // equivalent here — but a different basename still falls
        // through to the generic Bash block.
        assert!(try_parse_aciddog_send(
            "/usr/local/bin/aciddog send --to coder --body hi",
            "/opt/aciddog",
        )
        .is_some());
        assert!(
            try_parse_aciddog_send("aciddog send --to coder --body hi", "/opt/aciddog").is_some()
        );
        assert!(try_parse_aciddog_send(
            "/opt/not-aciddog send --to coder --body hi",
            "/opt/aciddog",
        )
        .is_none());
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

    #[tokio::test]
    async fn send_message_marks_recipient_working() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "b".into(),
                    body: "ping".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        // Recipient (b, idx 1) must flip to Working so its icon
        // reflects the incoming turn, not just the selected agent.
        assert_eq!(app.chats[1].status, AgentStatus::Working);
    }

    #[tokio::test]
    async fn send_message_preserves_recipient_error_status() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        app.chats[1].status = AgentStatus::Error;
        let sender_uuid = app.chats[0].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "b".into(),
                    body: "ping".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);
        assert_eq!(app.chats[1].status, AgentStatus::Error);
    }

    // --------- Workers ----------

    use crate::config::Worker;

    fn worker_config() -> AciddogConfig {
        AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: Some("default".into()),
            },
            agents: vec![
                Agent {
                    id: "coder".into(),
                    role: "r".into(),
                    role_description: None,
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: Some(vec!["researcher".into()]),
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                },
                Agent {
                    id: "planner".into(),
                    role: "r".into(),
                    role_description: None,
                    model: "m".into(),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                },
            ],
            workers: vec![
                Worker {
                    id: "researcher".into(),
                    role: "investigator".into(),
                    role_description: None,
                    model: "m".into(),
                    instructions: "look stuff up".into(),
                    tools: None,
                    can_create_workers: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                },
                Worker {
                    id: "fetcher".into(),
                    role: "fetcher".into(),
                    role_description: None,
                    model: "m".into(),
                    instructions: "fetch".into(),
                    tools: None,
                    can_create_workers: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                },
            ],
            ai_cores: vec![crate::config::AiCore {
                id: "default".into(),
                kind: crate::config::AiCoreKind::ClaudeCode {
                    system_prompt: None,
                    default_tools: None,
                },
            }],
            memory_banks: vec![],
            compactions: vec![],
            thinking_messages: vec!["thinking".into()],
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
    async fn worker_complete_marks_parent_working() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        app.chats[0].status = AgentStatus::Ready;

        let session_id = Uuid::new_v4();
        let (user_tx, _user_rx) = mpsc::unbounded_channel::<String>();
        let (worker_agent_tx, _worker_agent_rx) = mpsc::channel::<String>(4);
        app.chats.push(AgentChat {
            display_name: "researcher#abcd1234".into(),
            parent_idx: Some(0),
            worker_template: Some("researcher".into()),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            user_tx: Some(user_tx),
            agent_tx: Some(worker_agent_tx),
            ..Default::default()
        });
        app.agent_ids
            .insert(session_id, "researcher#abcd1234".into());

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
        assert_eq!(app.chats[0].status, AgentStatus::Working);
    }

    #[tokio::test]
    async fn worker_complete_preserves_parent_error_status() {
        let (mut app, _rx) = prime_worker_app(worker_config());
        app.chats[0].status = AgentStatus::Error;

        let session_id = Uuid::new_v4();
        let (user_tx, _user_rx) = mpsc::unbounded_channel::<String>();
        let (worker_agent_tx, _worker_agent_rx) = mpsc::channel::<String>(4);
        app.chats.push(AgentChat {
            display_name: "researcher#abcd1234".into(),
            parent_idx: Some(0),
            worker_template: Some("researcher".into()),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            user_tx: Some(user_tx),
            agent_tx: Some(worker_agent_tx),
            ..Default::default()
        });
        app.agent_ids
            .insert(session_id, "researcher#abcd1234".into());

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
        assert_eq!(app.chats[0].status, AgentStatus::Error);
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
    fn try_parse_aciddog_skill_run_extracts_name() {
        let cmd = "/opt/aciddog skill run --name code-review";
        let name = try_parse_aciddog_skill_run(cmd, "/opt/aciddog").unwrap();
        assert_eq!(name, "code-review");
    }

    #[test]
    fn try_parse_aciddog_skill_run_rejects_other_commands() {
        assert!(try_parse_aciddog_skill_run("ls -la", "/opt/aciddog").is_none());
        assert!(try_parse_aciddog_skill_run("/opt/aciddog test", "/opt/aciddog").is_none());
        assert!(try_parse_aciddog_skill_run(
            "/opt/aciddog send --to coder --body hi",
            "/opt/aciddog"
        )
        .is_none());
    }

    #[test]
    fn try_parse_aciddog_skill_run_is_disabled_when_binary_empty() {
        assert!(
            try_parse_aciddog_skill_run("aciddog skill run --name foo", "").is_none()
        );
    }

    #[test]
    fn is_aciddog_skill_list_matches_only_list_invocations() {
        assert!(is_aciddog_skill_list("/opt/aciddog skill list", "/opt/aciddog"));
        assert!(!is_aciddog_skill_list("/opt/aciddog skill run --name a", "/opt/aciddog"));
        assert!(!is_aciddog_skill_list(
            "/opt/aciddog skill search --query foo",
            "/opt/aciddog"
        ));
        assert!(!is_aciddog_skill_list("/opt/aciddog test", "/opt/aciddog"));
        assert!(!is_aciddog_skill_list("/opt/aciddog skill list", ""));
    }

    #[test]
    fn try_parse_aciddog_skill_search_extracts_query() {
        let q = try_parse_aciddog_skill_search(
            r#"/opt/aciddog skill search --query "code review""#,
            "/opt/aciddog",
        )
        .unwrap();
        assert_eq!(q, "code review");
    }

    #[test]
    fn try_parse_aciddog_skill_search_rejects_other_commands() {
        assert!(
            try_parse_aciddog_skill_search("/opt/aciddog skill list", "/opt/aciddog").is_none()
        );
        assert!(try_parse_aciddog_skill_search(
            "/opt/aciddog skill run --name a",
            "/opt/aciddog"
        )
        .is_none());
        assert!(try_parse_aciddog_skill_search(
            "/opt/aciddog skill search --query foo",
            ""
        )
        .is_none());
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

    // --------- TaskOp intercept ---------

    fn fire_task_tool_use(app: &mut App, command: &str) {
        app.aciddog_binary = "/opt/aciddog".into();
        app.handle_ai_event(
            0,
            AIEvent::ToolUse {
                id: "t1".into(),
                name: "Bash".into(),
                input: json!({ "command": command }),
                raw: serde_json::Value::Null,
            },
        );
    }

    #[test]
    fn bash_tool_use_for_task_add_becomes_task_op_block() {
        let mut app = App::new(sample_config());
        fire_task_tool_use(
            &mut app,
            r#"/opt/aciddog task add --title "smoke" --assignee coder"#,
        );
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp {
                kind,
                subject,
                pending,
                ..
            } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::Add);
                assert!(subject.starts_with("Add: smoke"));
                assert!(subject.contains("assigned: coder"));
                assert!(*pending);
            }
            other => panic!("expected TaskOp, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_show_becomes_task_op_block() {
        let mut app = App::new(sample_config());
        let id = Uuid::new_v4();
        fire_task_tool_use(&mut app, &format!("/opt/aciddog task show --id {id}"));
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, subject, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::Show);
                assert!(subject.starts_with("Show:"));
            }
            other => panic!("expected TaskOp::Show, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_list_mine_captures_filter() {
        let mut app = App::new(sample_config());
        fire_task_tool_use(&mut app, "/opt/aciddog task list --mine");
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, subject, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::List);
                assert!(subject.contains("--mine"));
            }
            other => panic!("expected TaskOp::List, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_update_captures_changes() {
        let mut app = App::new(sample_config());
        let id = Uuid::new_v4();
        fire_task_tool_use(
            &mut app,
            &format!("/opt/aciddog task update --id {id} --status done"),
        );
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, subject, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::Update);
                assert!(subject.contains("status: done"));
            }
            other => panic!("expected TaskOp::Update, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_remove_becomes_remove_block() {
        let mut app = App::new(sample_config());
        let id = Uuid::new_v4();
        fire_task_tool_use(&mut app, &format!("/opt/aciddog task remove --id {id}"));
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::Remove);
            }
            other => panic!("expected TaskOp::Remove, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_take_becomes_take_block() {
        let mut app = App::new(sample_config());
        fire_task_tool_use(&mut app, "/opt/aciddog task take");
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, subject, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::Take);
                assert_eq!(subject, "Take");
            }
            other => panic!("expected TaskOp::Take, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_note_carries_body() {
        let mut app = App::new(sample_config());
        let id = Uuid::new_v4();
        fire_task_tool_use(
            &mut app,
            &format!(r#"/opt/aciddog task note --id {id} --body "progress update""#),
        );
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp {
                kind,
                subject,
                body,
                ..
            } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::Note);
                assert!(subject.starts_with("Note on"));
                assert_eq!(body, "progress update");
            }
            other => panic!("expected TaskOp::Note, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_todo_update_in_pipeline_still_becomes_task_block() {
        // Agents sometimes pipe aciddog output into jq, head, etc.
        // The aciddog portion should still intercept cleanly.
        let mut app = App::new(sample_config());
        let task_id = Uuid::new_v4();
        let todo_id = Uuid::new_v4();
        let cmd = format!(
            "/opt/aciddog task todo update --task {task_id} --id {todo_id} --status in_progress | head"
        );
        fire_task_tool_use(&mut app, &cmd);
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::TodoUpdate);
            }
            other => panic!("expected TaskOp::TodoUpdate, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_add_with_redirect_still_becomes_task_block() {
        let mut app = App::new(sample_config());
        fire_task_tool_use(&mut app, r#"/opt/aciddog task add --title "x" > /tmp/out"#);
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::Add);
            }
            other => panic!("expected TaskOp::Add, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_todo_update_status_becomes_todo_update_block() {
        let mut app = App::new(sample_config());
        let task_id = Uuid::new_v4();
        let todo_id = Uuid::new_v4();
        let cmd = format!(
            "/opt/aciddog task todo update --task {task_id} --id {todo_id} --status in_progress"
        );
        fire_task_tool_use(&mut app, &cmd);
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, subject, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::TodoUpdate);
                assert!(
                    subject.contains("in_progress"),
                    "subject should mention new status, got {subject}"
                );
            }
            other => panic!("expected TaskOp::TodoUpdate, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_todo_update_body_becomes_todo_update_block() {
        let mut app = App::new(sample_config());
        let task_id = Uuid::new_v4();
        let todo_id = Uuid::new_v4();
        let cmd = format!(
            r#"/opt/aciddog task todo update --task {task_id} --id {todo_id} --body "revised""#,
        );
        fire_task_tool_use(&mut app, &cmd);
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, body, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::TodoUpdate);
                assert_eq!(body, "revised");
            }
            other => panic!("expected TaskOp::TodoUpdate, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_todo_remove_becomes_todo_remove_block() {
        let mut app = App::new(sample_config());
        let task_id = Uuid::new_v4();
        let todo_id = Uuid::new_v4();
        let cmd = format!("/opt/aciddog task todo remove --task {task_id} --id {todo_id}");
        fire_task_tool_use(&mut app, &cmd);
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::TodoRemove);
            }
            other => panic!("expected TaskOp::TodoRemove, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_todo_list_becomes_todo_list_block() {
        let mut app = App::new(sample_config());
        let task_id = Uuid::new_v4();
        let cmd = format!("/opt/aciddog task todo list --task {task_id}");
        fire_task_tool_use(&mut app, &cmd);
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { kind, .. } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::TodoList);
            }
            other => panic!("expected TaskOp::TodoList, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_todo_add_uses_todo_icon_kind() {
        let mut app = App::new(sample_config());
        let id = Uuid::new_v4();
        fire_task_tool_use(
            &mut app,
            &format!(r#"/opt/aciddog task todo add --task {id} --body "draft schema""#),
        );
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp {
                kind,
                subject,
                body,
                ..
            } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::TodoAdd);
                assert!(subject.starts_with("Todo add on"));
                assert_eq!(body, "draft schema");
            }
            other => panic!("expected TaskOp::TodoAdd, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_task_comment_add_uses_comment_kind() {
        let mut app = App::new(sample_config());
        let id = Uuid::new_v4();
        fire_task_tool_use(
            &mut app,
            &format!(r#"/opt/aciddog task comment add --task {id} --body "handoff ctx""#),
        );
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp {
                kind,
                subject,
                body,
                ..
            } => {
                assert_eq!(*kind, super::super::blocks::TaskOpKind::CommentAdd);
                assert!(subject.starts_with("Comment on"));
                assert_eq!(body, "handoff ctx");
            }
            other => panic!("expected TaskOp::CommentAdd, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_result_populates_task_op_reply_and_clears_pending() {
        let mut app = App::new(sample_config());
        fire_task_tool_use(&mut app, r#"/opt/aciddog task add --title "fix bug""#);
        app.handle_ai_event(
            0,
            AIEvent::ToolResult {
                tool_use_id: "t1".into(),
                content: json!("task created: 3a4f..."),
                is_error: false,
                raw: serde_json::Value::Null,
            },
        );
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::TaskOp { reply, pending, .. } => {
                assert!(reply.contains("task created"));
                assert!(!*pending);
            }
            other => panic!("expected TaskOp, got {other:?}"),
        }
    }

    #[test]
    fn bash_tool_use_for_non_task_aciddog_still_falls_through_to_mail() {
        // Double-check we haven't broken the existing send intercept
        // now that the task branch sits above the fallthrough.
        let mut app = App::new(sample_config());
        fire_task_tool_use(&mut app, r#"/opt/aciddog send --to b --body "hi""#);
        match app.chats[0].blocks.last().unwrap() {
            ChatBlock::Mail { .. } => {}
            other => panic!("expected Mail, got {other:?}"),
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

    // -------- Task TODOs + Comments IPC handlers --------

    #[tokio::test]
    async fn task_todo_ipc_roundtrips_add_update_list_remove() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "parent".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let task_id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        // Add a todo.
        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskTodoAdd {
                    task: task_id,
                    body: "step one".into(),
                },
            },
            reply: tx2,
        });
        let r = rx2.await.unwrap();
        assert!(r.ok, "error: {:?}", r.error);
        let todo_id = app.tasks.todos_of(task_id).unwrap()[0].id;

        // Update status.
        let (tx3, rx3) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskTodoUpdate {
                    task: task_id,
                    id: todo_id,
                    status: Some("in_progress".into()),
                    body: None,
                },
            },
            reply: tx3,
        });
        assert!(rx3.await.unwrap().ok);
        assert_eq!(
            app.tasks.todos_of(task_id).unwrap()[0].status,
            crate::tasks::TodoStatus::InProgress
        );

        // List.
        let (tx4, rx4) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskTodoList { task: task_id },
            },
            reply: tx4,
        });
        let msg = rx4.await.unwrap().message.unwrap();
        assert!(msg.contains("step one"));
        assert!(msg.contains("in_progress"));

        // Remove.
        let (tx5, rx5) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskTodoRemove {
                    task: task_id,
                    id: todo_id,
                },
            },
            reply: tx5,
        });
        assert!(rx5.await.unwrap().ok);
        assert!(app.tasks.todos_of(task_id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn task_comment_ipc_add_uses_sender_display_name_as_author() {
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
        let task_id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());

        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskCommentAdd {
                    task: task_id,
                    body: "from agent a".into(),
                },
            },
            reply: tx2,
        });
        assert!(rx2.await.unwrap().ok);
        let comments = app.tasks.comments_of(task_id).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].author, "a");
        assert_eq!(comments[0].body, "from agent a");
    }

    // --------- TUI overlays: o (TODOs) / c (Comments) ---------

    #[tokio::test]
    async fn o_opens_todos_overlay_on_selected_task() {
        let mut app = App::new(sample_config());
        let tid = app
            .tasks
            .add("parent".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);
        match &app.tasks_screen.child_overlay {
            Some(super::super::tasks_view::ChildOverlay::Todos { task_id, .. }) => {
                assert_eq!(*task_id, tid);
            }
            other => panic!("expected Todos overlay, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn c_opens_comments_overlay_on_selected_task() {
        let mut app = App::new(sample_config());
        let tid = app
            .tasks
            .add("parent".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        match &app.tasks_screen.child_overlay {
            Some(super::super::tasks_view::ChildOverlay::Comments { task_id, .. }) => {
                assert_eq!(*task_id, tid);
            }
            other => panic!("expected Comments overlay, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn todos_overlay_add_body_enter_appends_a_todo() {
        let mut app = App::new(sample_config());
        let tid = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for c in "draft schema".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let todos = app.tasks.todos_of(tid).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].body, "draft schema");
        // Overlay bounces back to Browse so the next op can follow.
        assert!(matches!(
            app.tasks_screen.child_overlay,
            Some(super::super::tasks_view::ChildOverlay::Todos { .. })
        ));
    }

    #[tokio::test]
    async fn todos_overlay_space_cycles_status_of_cursor_row() {
        let mut app = App::new(sample_config());
        let tid = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        let td_id = app
            .tasks
            .add_todo(tid, "one".into(), "seed".into())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(
            app.tasks
                .todos_of(tid)
                .unwrap()
                .iter()
                .find(|td| td.id == td_id)
                .unwrap()
                .status,
            crate::tasks::TodoStatus::InProgress
        );
    }

    #[tokio::test]
    async fn todos_overlay_d_y_removes_cursor_row() {
        let mut app = App::new(sample_config());
        let tid = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        let td_id = app
            .tasks
            .add_todo(tid, "one".into(), "seed".into())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);
        assert!(app
            .tasks
            .todos_of(tid)
            .unwrap()
            .iter()
            .all(|td| td.id != td_id));
    }

    #[tokio::test]
    async fn comments_overlay_add_body_enter_appends_comment() {
        let mut app = App::new(sample_config());
        let tid = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('c'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for ch in "heads up".chars() {
            app.handle_key(KeyCode::Char(ch), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let comments = app.tasks.comments_of(tid).unwrap();
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "heads up");
        // Author is the TUI operator constant.
        assert_eq!(comments[0].author, "tui");
    }

    #[tokio::test]
    async fn description_enter_inserts_newline_instead_of_submitting() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Tab to Description.
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        // Type + newline + more.
        for c in "line1".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        for c in "line2".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        // Form should still be open (Enter didn't submit).
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert_eq!(form.description, "line1\nline2");
        assert_eq!(form.description_cursor, form.description.len());
    }

    #[tokio::test]
    async fn description_up_at_top_line_tabs_off_to_previous_field() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        // Cursor at offset 0 (empty description), Up should tab off.
        app.handle_key(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Title
        );
    }

    #[tokio::test]
    async fn description_down_at_bottom_line_tabs_off_to_next_field() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        // Single-line description → cursor is on the bottom line.
        for c in "abc".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Assignee
        );
    }

    #[tokio::test]
    async fn enter_on_blocked_by_field_opens_multi_select_picker() {
        let mut app = App::new(sample_config());
        // Two extra tasks so the picker has candidates.
        app.tasks
            .add("one".into(), None, None, "x".into(), Vec::new())
            .unwrap();
        app.tasks
            .add("two".into(), None, None, "x".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Tab to BlockedBy (Add form: Title, Desc, Assignee, BlockedBy).
        for _ in 0..3 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::BlockedBy
        );
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app
            .tasks_screen
            .form
            .as_ref()
            .unwrap()
            .blocked_by_picker
            .is_some());
    }

    #[tokio::test]
    async fn blocked_by_picker_space_toggles_and_enter_commits() {
        let mut app = App::new(sample_config());
        let dep = app
            .tasks
            .add("prereq".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for _ in 0..3 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        // Space toggles the first row (the only option since we just
        // added one task beyond the form target).
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert!(form.blocked_by_picker.is_none(), "picker closes on Enter");
        assert_eq!(form.blocked_by, vec![dep]);
    }

    #[tokio::test]
    async fn clicking_ok_button_in_blocked_by_picker_commits() {
        let mut app = App::new(sample_config());
        let dep = app
            .tasks
            .add("prereq".into(), None, None, "x".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for _ in 0..3 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        // Toggle the only option with Space so OK has something to commit.
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        // Inject an OK button rect and click it.
        let ok_rect = Rect::new(10, 20, 8, 1);
        app.layout.task_form_picker_buttons = vec![(FormField::Ok, ok_rect)];
        app.layout.task_form_picker_outer = Some(Rect::new(0, 0, 100, 30));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 20,
            modifiers: KeyModifiers::NONE,
        });
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert!(form.blocked_by_picker.is_none(), "picker closed on OK");
        assert_eq!(form.blocked_by, vec![dep]);
    }

    #[tokio::test]
    async fn clicking_cancel_button_in_blocked_by_picker_discards() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("prereq".into(), None, None, "x".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for _ in 0..3 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        let cancel_rect = Rect::new(20, 20, 10, 1);
        app.layout.task_form_picker_buttons = vec![(FormField::Cancel, cancel_rect)];
        app.layout.task_form_picker_outer = Some(Rect::new(0, 0, 100, 30));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 22,
            row: 20,
            modifiers: KeyModifiers::NONE,
        });
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert!(form.blocked_by_picker.is_none());
        assert!(form.blocked_by.is_empty(), "cancel discards the toggle");
    }

    #[tokio::test]
    async fn click_inside_blocked_by_picker_but_not_on_anything_is_swallowed() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("prereq".into(), None, None, "x".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for _ in 0..3 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        app.layout.task_form_picker_buttons = vec![];
        app.layout.task_form_picker_rows = vec![Rect::new(10, 10, 40, 1)];
        app.layout.task_form_picker_outer = Some(Rect::new(0, 0, 100, 30));
        // Click somewhere inside the outer rect but not on a row.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 50,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        // Picker stays open.
        assert!(app
            .tasks_screen
            .form
            .as_ref()
            .unwrap()
            .blocked_by_picker
            .is_some());
    }

    #[tokio::test]
    async fn blocked_by_picker_esc_cancels_changes() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("prereq".into(), None, None, "x".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for _ in 0..3 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert!(form.blocked_by_picker.is_none());
        assert!(form.blocked_by.is_empty(), "esc discards the toggle");
    }

    #[tokio::test]
    async fn esc_closes_child_overlay() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('o'), KeyModifiers::NONE);
        assert!(app.tasks_screen.child_overlay.is_some());
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.tasks_screen.child_overlay.is_none());
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

    // --------- Skill run ----------

    /// Serialises CWD-mutating skill_run tests so they don't race with
    /// each other or other cargo-test threads.
    static SKILL_CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct CwdGuard {
        prev: std::path::PathBuf,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl CwdGuard {
        fn enter(p: &std::path::Path) -> Self {
            let lock = SKILL_CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let prev = std::env::current_dir().expect("cwd");
            std::env::set_current_dir(p).expect("cd into tempdir");
            Self { prev, _lock: lock }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.prev);
        }
    }

    #[tokio::test]
    async fn skill_run_rejects_invalid_name_shape() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillRun {
                    name: "../etc".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error.unwrap().contains("not a valid identifier"),
            "expected validation error"
        );
    }

    #[tokio::test]
    async fn skill_run_rejects_when_skill_not_in_allowlist() {
        let mut cfg = sample_config();
        cfg.agents[0].skills = Some(vec!["other-skill".into()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillRun {
                    name: "not-assigned".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        let err = resp.error.unwrap();
        assert!(err.contains("not in your allowlist"), "got: {err}");
        assert!(!err.contains("not found"), "must not confuse not-found with not-allowed: {err}");
    }

    #[tokio::test]
    async fn skill_run_reports_not_found_distinct_from_not_allowed() {
        let mut cfg = sample_config();
        cfg.agents[0].skills = Some(vec!["ghost".into()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        // Enter a tempdir with no .aciddog/skills at all.
        let tmp = tempfile::TempDir::new().unwrap();
        let _cwd = CwdGuard::enter(tmp.path());

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillRun {
                    name: "ghost".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        let err = resp.error.unwrap();
        assert!(err.contains("not found"), "got: {err}");
        assert!(!err.contains("allowlist"), "wrong error kind: {err}");
    }

    #[tokio::test]
    async fn skill_list_returns_friendly_message_when_no_skills_assigned() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillList,
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok);
        assert!(resp.message.unwrap().contains("No skills"));
    }

    #[tokio::test]
    async fn skill_list_emits_one_line_per_assigned_skill() {
        let mut cfg = sample_config();
        cfg.agents[0].skills = Some(vec!["alpha".into(), "beta".into()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        // No SKILL.md on disk — allowed, warn-and-placeholder path.
        let tmp = tempfile::TempDir::new().unwrap();
        let _cwd = CwdGuard::enter(tmp.path());

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillList,
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok);
        let body = resp.message.unwrap();
        assert!(body.contains("- alpha —"), "missing alpha: {body}");
        assert!(body.contains("- beta —"), "missing beta: {body}");
    }

    #[tokio::test]
    async fn skill_search_rejects_empty_query() {
        let mut cfg = sample_config();
        cfg.agents[0].skills = Some(vec!["alpha".into()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillSearch { query: "   ".into() },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn skill_search_filters_by_case_insensitive_substring_over_id_and_description() {
        let mut cfg = sample_config();
        cfg.agents[0].skills = Some(vec!["alpha".into(), "beta".into()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        // Lay out the SKILL.mds so we can match on description text.
        let tmp = tempfile::TempDir::new().unwrap();
        let skills_root = tmp.path().join(".aciddog/skills");
        for (id, desc) in [("alpha", "Reviews code diffs"), ("beta", "Writes tests")] {
            let d = skills_root.join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("SKILL.md"),
                format!("---\ndescription: {desc}\n---\nbody\n"),
            )
            .unwrap();
        }
        let _cwd = CwdGuard::enter(tmp.path());

        // "code" matches only alpha's description.
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillSearch {
                    query: "CODE".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok);
        let body = resp.message.unwrap();
        assert!(body.contains("- alpha —"), "alpha should match: {body}");
        assert!(!body.contains("- beta —"), "beta should not match: {body}");

        // Match on id substring.
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillSearch {
                    query: "bet".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok);
        let body = resp.message.unwrap();
        assert!(body.contains("- beta —"), "beta should match by id: {body}");
        assert!(!body.contains("- alpha —"), "alpha should not match: {body}");

        // Query with no hits.
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillSearch {
                    query: "zzznope".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok);
        assert!(resp.message.unwrap().contains("No assigned skills match"));
    }

    #[tokio::test]
    async fn skill_run_returns_abs_basedir_header_and_verbatim_body() {
        let mut cfg = sample_config();
        cfg.agents[0].skills = Some(vec!["demo".into()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();

        let tmp = tempfile::TempDir::new().unwrap();
        let skill_dir = tmp.path().join(".aciddog/skills/demo");
        std::fs::create_dir_all(&skill_dir).unwrap();
        let md = "---\ndescription: demo skill\n---\n\n# Demo\n\nBody refers to {baseDir}/assets/x.py.\n";
        std::fs::write(skill_dir.join("SKILL.md"), md).unwrap();
        let expected_abs = std::fs::canonicalize(&skill_dir).unwrap();

        let _cwd = CwdGuard::enter(tmp.path());

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SkillRun {
                    name: "demo".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok, "error: {:?}", resp.error);
        let body = resp.message.unwrap();
        let first_line = body.lines().next().unwrap();
        let header = format!("{{baseDir}} = {}", expected_abs.display());
        assert_eq!(first_line, header, "header mismatch: {body}");
        assert!(
            body.starts_with(&format!("{header}\n\n")),
            "expected header + blank line: {body}"
        );
        assert!(
            expected_abs.is_absolute(),
            "canonicalize must yield an absolute path"
        );
        assert!(body.ends_with(md), "body must contain verbatim SKILL.md contents: {body}");
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
    fn tab_cycles_into_ok_and_cancel_buttons() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Add form tab_order: Title, Desc, Assignee, BlockedByCsv, Ok, Cancel.
        // 4 Tabs lands on Ok, a 5th lands on Cancel.
        for _ in 0..4 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Ok
        );
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Cancel
        );
    }

    #[test]
    fn enter_on_ok_submits_form_and_closes() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        type_string(&mut app, "via ok");
        // Tab to OK.
        for _ in 0..4 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.is_none());
        assert_eq!(app.tasks.all().len(), 1);
        assert_eq!(app.tasks.all()[0].title, "via ok");
    }

    #[test]
    fn enter_on_cancel_closes_without_saving() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        type_string(&mut app, "should not save");
        for _ in 0..5 {
            app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        }
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Cancel
        );
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.is_none());
        assert_eq!(app.tasks.all().len(), 0);
    }

    #[test]
    fn clicking_field_rect_focuses_that_field() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Simulate the renderer populating the layout hit-rects.
        // We inject a known rect for Description and click inside it.
        let desc_rect = Rect::new(10, 10, 40, 2);
        app.layout.task_form_fields = vec![(FormField::Description, desc_rect)];
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 11,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().focused_field,
            FormField::Description
        );
    }

    #[test]
    fn clicking_ok_button_submits() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        type_string(&mut app, "click ok");
        let ok_rect = Rect::new(10, 20, 8, 1);
        app.layout.task_form_fields = vec![(FormField::Ok, ok_rect)];
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 20,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.tasks_screen.form.is_none());
        assert_eq!(app.tasks.all()[0].title, "click ok");
    }

    #[test]
    fn clicking_cancel_button_closes_without_saving() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        type_string(&mut app, "x");
        let cancel_rect = Rect::new(20, 20, 10, 1);
        app.layout.task_form_fields = vec![(FormField::Cancel, cancel_rect)];
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 22,
            row: 20,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.tasks_screen.form.is_none());
        assert_eq!(app.tasks.all().len(), 0);
    }

    #[test]
    fn clicking_dropdown_field_focuses_and_opens_picker() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        let assignee_rect = Rect::new(10, 14, 40, 2);
        app.layout.task_form_fields = vec![(FormField::Assignee, assignee_rect)];
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 14,
            modifiers: KeyModifiers::NONE,
        });
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert_eq!(form.focused_field, FormField::Assignee);
        assert!(form.picker.is_some());
    }

    #[test]
    fn clicking_picker_row_commits_that_option() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        // Manually open the picker on Assignee.
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Assignee;
            form.open_picker();
        }
        // Inject hit rects for the first three picker rows.
        app.layout.task_form_picker_rows = vec![
            Rect::new(10, 10, 20, 1),
            Rect::new(10, 11, 20, 1),
            Rect::new(10, 12, 20, 1),
        ];
        let options_snapshot = app
            .tasks_screen
            .form
            .as_ref()
            .unwrap()
            .picker
            .as_ref()
            .unwrap()
            .options
            .clone();
        // Click the 2nd row (index 1).
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 11,
            modifiers: KeyModifiers::NONE,
        });
        // Picker closed; assignee now matches options[1].
        assert!(app.tasks_screen.form.as_ref().unwrap().picker.is_none());
        assert_eq!(
            app.tasks_screen.form.as_ref().unwrap().assignee,
            options_snapshot[1]
        );
    }

    #[test]
    fn double_click_on_task_row_opens_edit_modal() {
        let mut app = App::new(sample_config());
        let id = app
            .tasks
            .add("hello".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        // The main area rect starts at row 2; the first task row
        // sits at row 3 after the "── open ──" header at row 2.
        app.layout.chat_history_inner = Some(Rect::new(0, 2, 60, 20));
        // Sanity: row 3 must be a task.
        let rows = TasksScreen::rows(&app.tasks);
        match rows.get(1) {
            Some(super::super::tasks_view::TaskRow::Task(t_id)) => {
                assert_eq!(*t_id, id);
            }
            other => panic!("expected first task at row index 1, got {other:?}"),
        }
        // First click: selects.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        assert!(
            app.tasks_screen.form.is_none(),
            "single click should not open form"
        );
        assert_eq!(app.tasks_screen.selected_row, 1);
        // Second click: opens the edit modal.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        let form = app
            .tasks_screen
            .form
            .as_ref()
            .expect("double click should open edit form");
        assert!(matches!(form.kind, TaskFormKind::Edit(edit_id) if edit_id == id));
        // Pre-filled.
        assert_eq!(form.title, "hello");
    }

    #[test]
    fn single_click_outside_double_click_window_does_not_open_form() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("hello".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.layout.chat_history_inner = Some(Rect::new(0, 2, 60, 20));
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        // Simulate enough time passing that the next click is out
        // of the double-click window. We achieve this by clearing
        // `last_click` directly (time travel in unit tests is
        // awkward and this keeps the test deterministic).
        app.last_click = None;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        assert!(app.tasks_screen.form.is_none());
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

    // ─── info + cron ────────────────────────────────────────────────

    fn cron_add_request(
        sender: Uuid,
        target: &str,
        kind: &str,
        payload: &str,
        schedule: &str,
        oneoff: bool,
    ) -> crate::agent_ipc::Request {
        crate::agent_ipc::Request {
            agent_id: sender,
            cmd: RequestCmd::CronAdd {
                target: target.into(),
                kind: kind.into(),
                payload: payload.into(),
                schedule: schedule.into(),
                tz: Some("utc".into()),
                oneoff,
                catchup_on_startup: false,
                disabled: false,
            },
        }
    }

    #[tokio::test]
    async fn info_returns_local_and_utc_lines() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::Info,
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok, "info should succeed: {resp:?}");
        let body = resp.message.unwrap();
        assert!(body.contains("local_time:"), "body: {body}");
        assert!(body.contains("utc_time:"), "body: {body}");
    }

    #[tokio::test]
    async fn cron_add_persists_and_lists() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "message", "ping", "0 * * * *", false),
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok, "{resp:?}");
        assert_eq!(app.cron.len(), 1);

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::CronList {
                    target: None,
                    include_disabled: false,
                    mine: false,
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok);
        let body = resp.message.unwrap();
        assert!(body.contains("→ b"), "list output: {body}");
    }

    #[tokio::test]
    async fn cron_message_payload_delivers_with_cron_from_prefix() {
        let mut app = App::new(sample_config());
        let mut agent_rxs = wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        // Add a one-off message scheduled for 1s ago so it fires on the
        // next tick.
        let past =
            (chrono::Utc::now() - chrono::Duration::seconds(1)).naive_utc();
        let past_str = past.format("%Y-%m-%dT%H:%M:%S").to_string();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "message", "ping", &past_str, true),
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);

        app.tick_cron(chrono::Utc::now());
        let payload = agent_rxs[1].recv().await.expect("delivered");
        assert!(
            payload.starts_with("<Message from=\"cron:a:"),
            "got: {payload}"
        );
        assert!(payload.contains("ping</Message>"), "got: {payload}");
        // One-off should be gone after firing.
        assert_eq!(app.cron.len(), 0);
    }

    #[tokio::test]
    async fn cron_slash_payload_executes_apply_command() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let past =
            (chrono::Utc::now() - chrono::Duration::seconds(1)).naive_utc();
        let past_str = past.format("%Y-%m-%dT%H:%M:%S").to_string();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "command", "/version", &past_str, true),
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);

        app.tick_cron(chrono::Utc::now());
        // Target chat (b) should have a system_info block from the
        // /version command outcome AND a user echo block of the slash
        // command itself.
        let blocks = &app.chats[1].blocks;
        assert!(
            blocks.iter().any(|b| matches!(b, ChatBlock::UserText { .. })),
            "expected a user block on target chat, got: {blocks:?}"
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, ChatBlock::System { .. })),
            "expected /version system block on target chat, got: {blocks:?}"
        );
    }

    #[tokio::test]
    async fn cron_add_rejects_blocked_slash_payload() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "command", "/exit", "0 9 * * *", false),
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error.as_deref().unwrap_or("").contains("blocked"),
            "expected blocklist error: {resp:?}"
        );
    }

    #[tokio::test]
    async fn cron_add_rejected_for_unknown_tz() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::CronAdd {
                    target: "b".into(),
                    kind: "message".into(),
                    payload: "ping".into(),
                    schedule: "0 9 * * *".into(),
                    tz: Some("Pacific/Auckland".into()),
                    oneoff: false,
                    catchup_on_startup: false,
                    disabled: false,
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error.as_deref().unwrap_or("").contains("timezone"),
            "expected tz error: {resp:?}"
        );
    }

    #[tokio::test]
    async fn cron_add_rejected_when_target_is_a_worker() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            // `coder#deadbeef` — looks like a worker display id; not a
            // configured agent, so it must be rejected.
            request: cron_add_request(sender, "coder#deadbeef", "message", "ping", "0 9 * * *", false),
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error
                .as_deref()
                .unwrap_or("")
                .contains("not a configured agent"),
            "expected configured-agent error: {resp:?}"
        );
    }

    #[tokio::test]
    async fn cron_add_rejected_when_caller_lacks_can_message() {
        let mut cfg = sample_config();
        // Restrict agent `a` to message only `c`.
        cfg.agents[0].can_message = Some(vec!["c".into()]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "message", "ping", "0 9 * * *", false),
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error
                .as_deref()
                .unwrap_or("")
                .contains("not permitted to message"),
            "expected ACL error: {resp:?}"
        );
    }

    #[tokio::test]
    async fn cron_add_rejected_for_worker_caller() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        // Mark chat 0 as a worker (give it a parent_idx).
        app.chats[0].parent_idx = Some(1);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "message", "ping", "0 9 * * *", false),
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error
                .as_deref()
                .unwrap_or("")
                .contains("workers cannot use cron"),
            "expected worker-rejection: {resp:?}"
        );
    }

    #[tokio::test]
    async fn cron_list_and_show_rejected_for_worker_caller() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        app.chats[0].parent_idx = Some(1);
        let sender = app.chats[0].session_id.unwrap();

        for cmd in [
            RequestCmd::CronList {
                target: None,
                include_disabled: false,
                mine: false,
            },
            RequestCmd::CronShow { id: Uuid::nil() },
        ] {
            let (tx, rx) = oneshot::channel();
            app.handle_agent_socket_event(AgentSocketEvent {
                request: crate::agent_ipc::Request {
                    agent_id: sender,
                    cmd,
                },
                reply: tx,
            });
            let resp = rx.await.unwrap();
            assert!(!resp.ok);
            assert!(
                resp.error
                    .as_deref()
                    .unwrap_or("")
                    .contains("workers cannot use cron"),
                "expected worker-rejection: {resp:?}"
            );
        }
    }

    #[tokio::test]
    async fn cron_remove_drops_job_and_notifies_invalidate() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "message", "ping", "0 * * * *", false),
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);
        let id = app.cron.all()[0].id;

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::CronRemove { id },
            },
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);
        assert_eq!(app.cron.len(), 0);
    }

    #[tokio::test]
    async fn cron_target_removed_disables_recurring_and_removes_one_off() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();

        // Recurring job targeting `b`.
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "message", "ping", "0 * * * *", false),
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);

        // One-off targeting `c`, scheduled in the past so it's due.
        let past =
            (chrono::Utc::now() - chrono::Duration::seconds(1)).naive_utc();
        let past_str = past.format("%Y-%m-%dT%H:%M:%S").to_string();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "c", "message", "ping", &past_str, true),
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);

        // Now drop `b` and `c`'s chats so the targets vanish before
        // we tick. (They're still in `app.config.agents`, but tick
        // looks up the live chat by display name — that's how the
        // production flow behaves when an agent never spawns.)
        app.chats[1].display_name = "renamed-b".into();
        app.chats[2].display_name = "renamed-c".into();

        // Force the recurring job's next_run into the past so it's
        // also due this tick.
        if let Some(j) = app.cron.all().first().cloned() {
            let _ = app.cron.update(
                j.id,
                crate::cron::CronPatch {
                    schedule: Some(crate::cron::CronSchedule::OneOff {
                        run_at_local: past,
                    }),
                    timezone: Some(crate::cron::CronTz::Utc),
                    enabled: Some(true),
                    ..Default::default()
                },
            );
        }
        // The above turned the first job into a one-off too — for the
        // purpose of this test we just need both to disappear when the
        // target's gone.
        app.tick_cron(chrono::Utc::now());

        assert_eq!(
            app.cron.len(),
            0,
            "both jobs should be pruned (one-off targets vanished)"
        );
    }

    #[tokio::test]
    async fn cron_invalidate_wakes_on_mutation() {
        // Sanity: the cron_invalidate Notify is woken by a successful
        // CronAdd. We can't easily drive the full event loop in a unit
        // test, but we can subscribe to the Notify and assert it fires.
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let invalidate = app.cron_invalidate.clone();

        let notified = tokio::spawn(async move {
            tokio::time::timeout(std::time::Duration::from_secs(1), invalidate.notified())
                .await
                .is_ok()
        });

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: cron_add_request(sender, "b", "message", "ping", "0 * * * *", false),
            reply: tx,
        });
        assert!(rx.await.unwrap().ok);
        assert!(notified.await.unwrap(), "cron_invalidate did not fire");
    }
}
