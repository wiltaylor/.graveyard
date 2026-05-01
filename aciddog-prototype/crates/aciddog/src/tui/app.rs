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
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};
use uuid::Uuid;

use clap::Parser;

use crate::agent_ipc::{self, AgentSocketEvent, HistorySelector, RequestCmd, Response};
use crate::ai::{
    AICore, AIEvent, AciddogCliCtx, AnthropicApiCore, ClaudeCodeCore, CopilotCliCore,
    GeminiCliCore, JulesCore, JulesProperties, OpenAiApiCore, OpenAiChatgptCore, PromptParts,
    WorkerCtx,
};
use crate::cli::{AgentArgs, AgentCommand};
use crate::config::{AciddogConfig, AiCore, AiCoreKind};
use crate::memory::MemoryRegistry;
use crate::tasks::{Task, TaskFilter, TaskPatch, TaskStatus, TaskStore, TodoStatus};
use crate::usage;

use super::cores_view::{CoresScreen, UsageEvent};
use super::tasks_view::{
    ChildOverlay, ChildPromptKind, FormField, TaskForm, TaskFormKind, TasksPrompt, TasksScreen,
    OPERATOR,
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

/// Capacity of the per-`App` chat-block broadcast channel. Larger than
/// the webservice's mail-only `InboundEvent` backlog (256) because
/// block events include every tool call and assistant-text chunk —
/// well over an order of magnitude chattier per turn. A slow
/// subscriber that falls behind this buffer receives
/// `BroadcastStreamRecvError::Lagged` and the SSE handler forwards it
/// as a named `lagged` event so the client can resync by re-fetching
/// history.
const CHAT_BLOCK_BROADCAST_CAPACITY: usize = 1024;

/// Short label for the AgentInfo chat block. Format is
/// `<variant> (<wcl-id>)` so the user can see both the runtime kind
/// and which specific `ai_core` block in aciddog.wcl is backing this
/// agent (e.g. `anthropic-api (z_ai)`).
pub(super) fn core_display_label(core: &AiCore) -> String {
    let kind = match &core.kind {
        AiCoreKind::ClaudeCode { .. } => "claude-code",
        AiCoreKind::AnthropicApi { .. } => "anthropic-api",
        AiCoreKind::OpenAiApi { .. } => "openai-api",
        AiCoreKind::OpenAiChatgpt { .. } => "openai-chatgpt",
        AiCoreKind::CopilotCli { .. } => "copilot-cli",
        AiCoreKind::GeminiCli { .. } => "gemini-cli",
        AiCoreKind::Jules { .. } => "jules",
    };
    format!("{kind} ({})", core.id)
}

/// Whether this core kind reports per-round token deltas (so we sum
/// them) or cumulative session totals (so we replace).
pub(super) fn core_usage_mode(kind: &AiCoreKind) -> ContextUsageMode {
    match kind {
        AiCoreKind::AnthropicApi { .. }
        | AiCoreKind::OpenAiApi { .. }
        | AiCoreKind::OpenAiChatgpt { .. } => ContextUsageMode::Add,
        AiCoreKind::ClaudeCode { .. }
        | AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. }
        // Jules doesn't report token usage — `Replace` keeps any
        // stray event from accumulating forever (none are expected).
        | AiCoreKind::Jules { .. } => ContextUsageMode::Replace,
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
        | AiCoreKind::OpenAiChatgpt { .. }
        | AiCoreKind::CopilotCli { .. }
        | AiCoreKind::GeminiCli { .. } => Some("Bash"),
        // Jules runs in Google's remote sandbox; no local shell is
        // reachable. Tag IPC is the only agent-level surface.
        AiCoreKind::Jules { .. } => None,
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

/// Handle `/auth login <provider> <name>` from inside the TUI.
/// The OAuth flow opens a browser and blocks on a callback for up to
/// five minutes, so we kick it off on a tokio task and return a
/// one-line status message for the chat. Users poll with
/// `/auth status <name>` to see whether the flow completed.
fn handle_auth_login(provider: &str, name: &str) -> String {
    if provider != "chatgpt" {
        return format!("unknown provider `{provider}` (try `chatgpt`)");
    }
    let name = name.to_string();
    let log_name = name.clone();
    tokio::spawn(async move {
        match crate::openai_auth::login_chatgpt(&log_name).await {
            Ok(t) => tracing::info!(
                credential = %log_name,
                account_id = %t.account_id,
                "openai_auth: TUI login succeeded"
            ),
            Err(e) => tracing::warn!(
                credential = %log_name,
                error = %e,
                "openai_auth: TUI login failed"
            ),
        }
    });
    format!(
        "starting OpenAI ChatGPT login for credential `{name}`. A browser tab will open; \
         if it doesn't, watch aciddog.log for the URL. When done, run `/auth status {name}`."
    )
}

fn handle_auth_logout(name: &str) -> String {
    match crate::openai_auth::remove(name) {
        Ok(path) => format!("removed credential `{name}` ({})", path.display()),
        Err(e) => format!("logout failed: {e}"),
    }
}

fn handle_auth_list() -> String {
    match crate::openai_auth::list() {
        Ok(entries) if entries.is_empty() => "(no stored credentials)".to_string(),
        Ok(entries) => {
            let mut out = String::from("stored credentials:\n");
            for e in entries {
                out.push_str(&format!(
                    "  {name:<18}  provider={prov}  account={acc}  expires={exp}\n",
                    name = e.name,
                    prov = e.provider,
                    acc = e.account_id,
                    exp = e.expires_at
                ));
            }
            out
        }
        Err(e) => format!("list failed: {e}"),
    }
}

fn handle_auth_status(name: Option<&str>) -> String {
    match name {
        Some(n) => match crate::openai_auth::get_chatgpt(n) {
            Ok(t) => format!(
                "{n}: provider=openai_chatgpt account={} expires={}",
                t.account_id, t.expires_at
            ),
            Err(e) => format!("{e}"),
        },
        None => handle_auth_list(),
    }
}

/// Construct a boxed `AICore` for the given config variant, wiring env
/// and (optionally) the tag-IPC socket. `resume` tells the factory to
/// skip the first-turn preamble — used by `resurrect_worker` so a
/// hibernated worker resumes cleanly.
///
/// Returns `Err(String)` only for remote-worker cores that fail
/// validation of their per-spawn properties — in that case the worker
/// spawn is rejected with a clear error to the caller. Local cores
/// never fail here.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_core(
    core_cfg: &AiCore,
    session_id: Uuid,
    parts: PromptParts,
    tools: Vec<String>,
    env: Vec<(String, String)>,
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    resume: bool,
    agent_model: Option<&str>,
    properties: Option<&serde_json::Value>,
) -> Result<Box<dyn AICore>, String> {
    build_core_with_extras(
        core_cfg,
        session_id,
        parts,
        tools,
        env,
        socket_tx,
        resume,
        Vec::new(),
        agent_model,
        properties,
    )
}

/// Same as `build_core` but allows injecting additional tools into the
/// core's `ToolRegistry`. Used by the workflow engine to hand a
/// worker-step core the per-instance `check_successful` /
/// `workflow_get` / `workflow_set` tools without adding them to
/// `BUILT_IN_TOOL_NAMES`. Only the direct-API cores (AnthropicApi,
/// OpenAiApi) honour `extra_tools`; CLI-harness cores (ClaudeCode,
/// CopilotCli, GeminiCli) don't own their own tool registry (they
/// delegate to the external binary), so extras are silently dropped
/// for those variants — workflows targeting them fall back to plain
/// worker execution.
#[allow(clippy::too_many_arguments)]
pub(super) fn build_core_with_extras(
    core_cfg: &AiCore,
    session_id: Uuid,
    parts: PromptParts,
    tools: Vec<String>,
    env: Vec<(String, String)>,
    socket_tx: Option<mpsc::Sender<AgentSocketEvent>>,
    resume: bool,
    extra_tools: Vec<std::sync::Arc<dyn crate::ai::tools::Tool>>,
    agent_model: Option<&str>,
    properties: Option<&serde_json::Value>,
) -> Result<Box<dyn AICore>, String> {
    match &core_cfg.kind {
        AiCoreKind::ClaudeCode {
            system_prompt,
            model,
            ..
        } => {
            let mut cc = if resume {
                ClaudeCodeCore::with_session_id(session_id, tools, system_prompt.clone())
            } else {
                ClaudeCodeCore::new(session_id, parts, tools, system_prompt.clone())
            }
            .with_env(env);
            if let Some(m) = agent_model.map(str::to_string).or_else(|| model.clone()) {
                cc = cc.with_model(m);
            }
            if let Some(tx) = socket_tx {
                cc = cc.with_socket_tx(tx);
            }
            Ok(Box::new(cc))
        }
        AiCoreKind::AnthropicApi {
            api_url,
            api_key_env,
            model,
            system_prompt,
            ..
        } => {
            let resolved_model = agent_model
                .map(str::to_string)
                .unwrap_or_else(|| model.clone());
            let mut c = AnthropicApiCore::new(
                session_id,
                api_url.clone(),
                api_key_env.clone(),
                resolved_model,
                parts,
                tools,
                system_prompt.clone(),
            )
            .with_env(env)
            .with_extra_tools(extra_tools);
            if resume {
                c = c.mark_started();
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Ok(Box::new(c))
        }
        AiCoreKind::OpenAiApi {
            api_url,
            api_key_env,
            model,
            system_prompt,
            ..
        } => {
            let resolved_model = agent_model
                .map(str::to_string)
                .unwrap_or_else(|| model.clone());
            let mut c = OpenAiApiCore::new(
                session_id,
                api_url.clone(),
                api_key_env.clone(),
                resolved_model,
                parts,
                tools,
                system_prompt.clone(),
            )
            .with_env(env)
            .with_extra_tools(extra_tools);
            if resume {
                c = c.mark_started();
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Ok(Box::new(c))
        }
        AiCoreKind::OpenAiChatgpt {
            credential,
            model,
            system_prompt,
            reasoning_effort,
            ..
        } => {
            let effort = reasoning_effort.as_deref().unwrap_or("medium");
            let resolved_model = agent_model
                .map(str::to_string)
                .unwrap_or_else(|| model.clone());
            let mut c = OpenAiChatgptCore::new(
                session_id,
                credential.clone(),
                resolved_model,
                effort,
                parts,
                tools,
                system_prompt.clone(),
            )
            .with_env(env)
            .with_extra_tools(extra_tools);
            if resume {
                c = c.mark_started();
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Ok(Box::new(c))
        }
        AiCoreKind::CopilotCli { model, .. } => {
            let mut c = if resume {
                CopilotCliCore::with_session_id(session_id, tools)
            } else {
                CopilotCliCore::new(session_id, parts, tools)
            }
            .with_env(env);
            if let Some(m) = agent_model.map(str::to_string).or_else(|| model.clone()) {
                c = c.with_model(m);
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Ok(Box::new(c))
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
            if let Some(m) = agent_model.map(str::to_string).or_else(|| model.clone()) {
                c = c.with_model(m);
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            Ok(Box::new(c))
        }
        AiCoreKind::Jules {
            api_key_env,
            default_branch,
            poll_interval_seconds,
            require_plan_approval,
            automation_mode,
        } => {
            let parent_id = parts
                .aciddog_cli
                .as_ref()
                .and_then(|c| c.worker.as_ref())
                .map(|w| w.parent_id.clone());
            let task = parts
                .aciddog_cli
                .as_ref()
                .and_then(|c| c.worker.as_ref())
                .map(|w| w.task.clone())
                .unwrap_or_default();
            let properties_value = properties.ok_or_else(|| {
                "Jules workers require `properties.repo` (\"owner/repo\") at spawn time".to_string()
            })?;
            let props = JulesProperties::parse(properties_value, default_branch.as_deref())?;
            // `properties.autoCreatePr` (if provided) wins over the
            // core-level `automationMode`; otherwise fall through to
            // the configured default. Lets a caller force PR
            // auto-creation on a per-spawn basis without changing the
            // core config.
            let effective_automation_mode = match props.auto_create_pr {
                Some(true) => Some("AUTO_CREATE_PR".to_string()),
                Some(false) => Some("AUTOMATION_MODE_UNSPECIFIED".to_string()),
                None => automation_mode.clone(),
            };
            // `extra_tools` and `env` do not apply to Jules — it runs
            // remotely and has no local tool registry / shell env.
            let _ = extra_tools;
            let _ = env;
            let mut c = JulesCore::new(
                session_id,
                api_key_env.clone(),
                props,
                *poll_interval_seconds,
                *require_plan_approval,
                effective_automation_mode,
                parent_id,
                &task,
            );
            if resume {
                c = c.mark_started();
            }
            if let Some(tx) = socket_tx {
                c = c.with_socket_tx(tx);
            }
            // `agent_model` overrides are not forwarded; Jules picks
            // its own model on the backend.
            let _ = agent_model;
            Ok(Box::new(c))
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
    include_user: bool,
) -> String {
    let mut out = String::new();
    out.push_str(&format!("Team: {}\n", cfg.team.description));

    // Split the contactable allowlist into agents (AI peers) vs.
    // automatons (script-backed fleet members) so the model knows
    // when it's messaging something that isn't an LLM. Unknown ids
    // fall back into the agent bucket with a generic label — the
    // parser's validation shouldn't let that happen, but keep the
    // fallback so config drift doesn't silently swallow an entry.
    let mut agent_entries: Vec<(&str, &str)> = Vec::new();
    let mut automaton_entries: Vec<(&str, &str)> = Vec::new();
    for id in contactable {
        if let Some(a) = cfg.agents.iter().find(|a| a.id == *id) {
            agent_entries.push((id, a.role.as_str()));
        } else if let Some(a) = cfg.automatons.iter().find(|a| a.id == *id) {
            automaton_entries.push((id, a.role.as_str()));
        } else {
            agent_entries.push((id, "agent"));
        }
    }

    // The reserved `user` recipient is unconditional for configured
    // agents/automatons: the human operator's inbox is always
    // reachable. Workers are scoped to their parent only, so they
    // pass `include_user = false`.
    if include_user {
        out.push_str("\nPeople you can message:\n");
        out.push_str(
            "- user — the human operator (may take a long time to reply; send and move on)\n",
        );
    }

    if !agent_entries.is_empty() {
        out.push_str("\nAgents you can message:\n");
        for (id, role) in &agent_entries {
            out.push_str(&format!("- {id} — {role}\n"));
        }
    }

    if !automaton_entries.is_empty() {
        out.push_str(
            "\nAutomatons you can message (script-backed fleet members, not AI agents — \
             each message spawns a one-shot child process):\n",
        );
        for (id, role) in &automaton_entries {
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

/// Format a single `memory show` reply. Same column set as the hit
/// formatter, minus rank / distance (irrelevant for a point lookup)
/// and including the full uuid so agents can feed it into follow-up
/// `memory link` / `memory update` calls.
fn format_memory_entry(hit: &crate::memory::Hit) -> String {
    let ts = hit.created_at.format("%Y-%m-%d %H:%M");
    let title = if hit.title.is_empty() {
        String::new()
    } else {
        format!(" · title={}", hit.title)
    };
    let tags = if hit.tags.is_empty() {
        String::new()
    } else {
        format!(" · tags=[{}]", hit.tags.join(","))
    };
    format!(
        "[{id} · {ts}{title}{tags}] {body}",
        id = hit.id,
        body = hit.body
    )
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
        RequestCmd::WorkerCreate {
            template,
            task,
            properties: _,
        } => ChatBlock::worker_spawn(template, task),
        RequestCmd::WorkerComplete { body } => ChatBlock::worker_complete(body),
        RequestCmd::WorkerArchive { worker } => {
            ChatBlock::from_tool_use(&format!("aciddog worker archive --id {worker}"), input)
        }
        RequestCmd::RemoteSessionBound { name } => {
            // Internal registration — shouldn't surface on any pane,
            // but if it does leak into a tag transcript, render it as
            // a generic tool call so nothing panics.
            ChatBlock::from_tool_use(&format!("remote_session_bound {name}"), input)
        }
        RequestCmd::WorkflowRun {
            workflow,
            properties,
        } => {
            let summary = if properties.is_empty() {
                format!("aciddog workflow run --id {workflow}")
            } else {
                let pairs: Vec<String> =
                    properties.iter().map(|(k, v)| format!("{k}={v}")).collect();
                format!("aciddog workflow run --id {workflow} {}", pairs.join(" "))
            };
            ChatBlock::from_tool_use(&summary, input)
        }
        RequestCmd::WorkflowTemplatesJson => {
            ChatBlock::from_tool_use("aciddog workflow templates --json", input)
        }
        RequestCmd::WorkflowInstancesJson => {
            ChatBlock::from_tool_use("aciddog workflow instances --json", input)
        }
        RequestCmd::WorkflowDismiss { instance_id } => {
            ChatBlock::from_tool_use(&format!("aciddog workflow dismiss {instance_id}"), input)
        }
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
        RequestCmd::TaskListJson {
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
        RequestCmd::MemorySearchJson { .. } => {
            ChatBlock::from_tool_use("aciddog memory search --json", input)
        }
        RequestCmd::MemoryStore { .. } => ChatBlock::from_tool_use("aciddog memory store", input),
        RequestCmd::MemoryStoreJson { .. } => {
            ChatBlock::from_tool_use("aciddog memory store --json", input)
        }
        RequestCmd::MemoryUpdate { .. } => ChatBlock::from_tool_use("aciddog memory update", input),
        RequestCmd::MemoryRemove { .. } => ChatBlock::from_tool_use("aciddog memory remove", input),
        RequestCmd::MemoryShow { .. } => ChatBlock::from_tool_use("aciddog memory show", input),
        RequestCmd::MemoryLink { .. } => ChatBlock::from_tool_use("aciddog memory link", input),
        RequestCmd::MemoryUnlink { .. } => ChatBlock::from_tool_use("aciddog memory unlink", input),
        RequestCmd::MemoryLinks { .. } => ChatBlock::from_tool_use("aciddog memory links", input),
        RequestCmd::MemoryLinksJson { .. } => {
            ChatBlock::from_tool_use("aciddog memory links --json", input)
        }
        RequestCmd::MemoryBanks => ChatBlock::from_tool_use("aciddog memory banks", input),
        RequestCmd::MemoryGraph { bank } => {
            ChatBlock::from_tool_use(&format!("aciddog memory graph --bank {bank}"), input)
        }
        RequestCmd::Info => ChatBlock::from_tool_use("aciddog info", input),
        RequestCmd::CronAdd { .. } => ChatBlock::from_tool_use("aciddog cron add", input),
        RequestCmd::CronList { .. } => ChatBlock::from_tool_use("aciddog cron list", input),
        RequestCmd::CronListJson { .. } => ChatBlock::from_tool_use("aciddog cron list", input),
        RequestCmd::CronShow { .. } => ChatBlock::from_tool_use("aciddog cron show", input),
        RequestCmd::CronUpdate { .. } => ChatBlock::from_tool_use("aciddog cron update", input),
        RequestCmd::CronRemove { .. } => ChatBlock::from_tool_use("aciddog cron remove", input),
        RequestCmd::SkillList => ChatBlock::from_tool_use("aciddog skill list", input),
        RequestCmd::SkillSearch { .. } => ChatBlock::from_tool_use("aciddog skill search", input),
        RequestCmd::SkillRun { .. } => ChatBlock::from_tool_use("aciddog skill run", input),
        RequestCmd::ChannelSend { .. } => ChatBlock::from_tool_use("aciddog channel send", input),
        RequestCmd::ChannelCreate { .. } => {
            ChatBlock::from_tool_use("aciddog channel create", input)
        }
        RequestCmd::ChannelJoin { .. } => ChatBlock::from_tool_use("aciddog channel join", input),
        RequestCmd::ChannelAdd { .. } => ChatBlock::from_tool_use("aciddog channel add", input),
        RequestCmd::ChannelKick { .. } => ChatBlock::from_tool_use("aciddog channel kick", input),
        RequestCmd::ChannelEnd { .. } => ChatBlock::from_tool_use("aciddog channel end", input),
        RequestCmd::AgentList => ChatBlock::from_tool_use("aciddog agent list", input),
        RequestCmd::AgentStatus { agent } => {
            ChatBlock::from_tool_use(&format!("aciddog agent status --id {agent}"), input)
        }
        RequestCmd::AgentHistory { agent, .. } => {
            ChatBlock::from_tool_use(&format!("aciddog agent history --id {agent}"), input)
        }
        RequestCmd::AgentHistoryJson { agent, .. } => {
            ChatBlock::from_tool_use(&format!("aciddog agent history --id {agent} --json"), input)
        }
        RequestCmd::AgentStop { agent, .. } => {
            ChatBlock::from_tool_use(&format!("aciddog agent stop --id {agent}"), input)
        }
        RequestCmd::AgentSleep { agent } => {
            ChatBlock::from_tool_use(&format!("aciddog agent sleep --id {agent}"), input)
        }
        RequestCmd::AgentWake { agent } => {
            ChatBlock::from_tool_use(&format!("aciddog agent wake --id {agent}"), input)
        }
        RequestCmd::AgentSendAsUser { to, body } => {
            ChatBlock::mail_sent(format!("{to} (as user)"), body)
        }
        RequestCmd::TeamInfo => ChatBlock::from_tool_use("aciddog team info", input),
        RequestCmd::ChannelList => ChatBlock::from_tool_use("aciddog channel list", input),
        RequestCmd::ChannelHistory { channel } => {
            ChatBlock::from_tool_use(&format!("aciddog channel history --id {channel}"), input)
        }
        RequestCmd::ChannelSendAsUser { channel, body } => {
            ChatBlock::channel_message("user".to_string(), format!("→ #{channel}: {body}"))
        }
        RequestCmd::InboxList => ChatBlock::from_tool_use("aciddog inbox list", input),
        RequestCmd::InboxDelete { index } => {
            ChatBlock::from_tool_use(&format!("aciddog inbox delete --index {index}"), input)
        }
        RequestCmd::WorkerList { .. } => ChatBlock::from_tool_use("aciddog worker list", input),
        RequestCmd::WorkerForceArchive { worker } => ChatBlock::from_tool_use(
            &format!("aciddog worker force-archive --id {worker}"),
            input,
        ),
        RequestCmd::ServerShutdown => ChatBlock::from_tool_use("aciddog server shutdown", input),
        RequestCmd::ServerExit => ChatBlock::from_tool_use("aciddog server exit", input),
        RequestCmd::ServerReload => ChatBlock::from_tool_use("aciddog server reload", input),
        RequestCmd::TeamRead => ChatBlock::from_tool_use("aciddog team read", input),
        RequestCmd::TeamWrite { .. } => ChatBlock::from_tool_use("aciddog team write", input),
        RequestCmd::SkillsList => ChatBlock::from_tool_use("aciddog skills list", input),
        RequestCmd::SkillRead { name } => {
            ChatBlock::from_tool_use(&format!("aciddog skills read --id {name}"), input)
        }
        RequestCmd::SkillWrite { name, .. } => {
            ChatBlock::from_tool_use(&format!("aciddog skills write --id {name}"), input)
        }
        RequestCmd::SkillRemove { name } => {
            ChatBlock::from_tool_use(&format!("aciddog skills remove --id {name}"), input)
        }
    }
}

/// Stable tag for a `ChatBlock` variant, used by the supervisor
/// `AgentHistory { select: kind }` selector. Values match the labels
/// the tag-IPC preamble documents for the `kind` filter.
fn history_block_kind(b: &ChatBlock) -> &'static str {
    match b {
        ChatBlock::AgentInfo { .. } => "agent_info",
        ChatBlock::UserText { .. } => "user",
        ChatBlock::AiMarkdown { .. } => "ai",
        ChatBlock::Mail {
            direction: super::blocks::MailDirection::Sent,
            ..
        } => "mail_sent",
        ChatBlock::Mail {
            direction: super::blocks::MailDirection::Received,
            ..
        } => "mail_received",
        ChatBlock::ChannelMessage { .. } => "channel",
        ChatBlock::WorkerSpawn { .. } => "worker_spawn",
        ChatBlock::WorkerComplete { .. } => "worker_complete",
        ChatBlock::System { kind, .. } => match kind {
            super::blocks::SystemKind::Info => "system_info",
            super::blocks::SystemKind::Warn => "system_warn",
            super::blocks::SystemKind::Error => "system_error",
        },
        ChatBlock::BashTool { .. }
        | ChatBlock::EditTool { .. }
        | ChatBlock::WriteTool { .. }
        | ChatBlock::ReadTool { .. }
        | ChatBlock::GrepTool { .. }
        | ChatBlock::GlobTool { .. }
        | ChatBlock::GenericTool { .. }
        | ChatBlock::TaskOp { .. }
        | ChatBlock::RemotePatch { .. }
        | ChatBlock::AutomatonStdout { .. }
        | ChatBlock::AutomatonStderr { .. } => "tool_use",
    }
}

/// One-line rendering of a chat block for the `AgentHistory` reply.
/// Uses `<kind>: <text>` so each selector result is scannable.
fn history_block_line(b: &ChatBlock) -> String {
    let kind = history_block_kind(b);
    let text = match b {
        ChatBlock::UserText { text } => text.clone(),
        ChatBlock::AiMarkdown { raw, .. } => raw.clone(),
        ChatBlock::Mail {
            counterpart, body, ..
        } => format!("{counterpart}: {body}"),
        ChatBlock::ChannelMessage { sender, body, .. } => format!("{sender}: {body}"),
        ChatBlock::WorkerSpawn { template, task, .. } => format!("{template}: {task}"),
        ChatBlock::WorkerComplete { body, .. } => body.clone(),
        ChatBlock::System { text, .. } => text.clone(),
        ChatBlock::TaskOp { subject, .. } => subject.clone(),
        ChatBlock::AutomatonStdout { line } | ChatBlock::AutomatonStderr { line } => line.clone(),
        ChatBlock::BashTool { command, .. } => command.clone(),
        ChatBlock::EditTool { file_path, .. }
        | ChatBlock::WriteTool { file_path, .. }
        | ChatBlock::ReadTool { file_path, .. } => file_path.clone(),
        ChatBlock::GrepTool { pattern, .. } | ChatBlock::GlobTool { pattern, .. } => {
            pattern.clone()
        }
        ChatBlock::GenericTool { name, .. } => name.clone(),
        ChatBlock::RemotePatch { summary, .. } => summary.clone(),
        ChatBlock::AgentInfo { core, .. } => format!("core={core}"),
    };
    let text_clean: String = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    format!("{kind}: {text_clean}")
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
    let target_marker = if target_known {
        ""
    } else {
        " [target unknown]"
    };
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
        if j.catchup_on_startup {
            "true"
        } else {
            "false"
        }
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

/// Origin of a channel post. `User` is the human operator typing in
/// the TUI; `Agent(display_name)` is an agent-initiated broadcast via
/// `aciddog channel send`. The variant controls how the `from=` attr
/// is populated on the delivered `<ChannelMessage>` and which ACL
/// checks run (the user never has to be a declared member).
#[derive(Debug, Clone)]
pub enum ChannelSender {
    User,
    Agent(String),
}

impl ChannelSender {
    /// Rendered in the `from="..."` attribute on delivered messages
    /// and as the `sender:` prefix in the TUI channel view.
    pub fn label(&self) -> &str {
        match self {
            ChannelSender::User => "user",
            ChannelSender::Agent(name) => name.as_str(),
        }
    }
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
    /// The right-column "Channels" list (below the agents list).
    /// Up/Down moves the selection, Enter sets the main area to
    /// `ActiveView::Channel(idx)`.
    Channels,
    /// The right-column "Workflows" list (between agents and channels).
    /// Up/Down moves the selection, Enter on a header opens the
    /// property-entry form; Enter on an instance opens its progress
    /// view.
    Workflows,
    /// The main area while `active_view == Memory` — keyboard input
    /// goes to the memory-view handler.
    Memory,
    /// The main area while a workflow-related view is active
    /// (`WorkflowForm` or `WorkflowInstance`). Keyboard input is
    /// routed to the workflow-view handler.
    Workflow,
    /// The main area while `active_view == Cores` — keyboard input
    /// goes to the cores-view handler.
    Cores,
    /// The main area while `active_view == Inbox` — keyboard input
    /// goes to the inbox-view handler (list navigation, reply/forward
    /// modals, delete).
    Inbox,
    /// The main area while `active_view == Cron` — keyboard input
    /// goes to the cron-view handler (list navigation, add/edit
    /// modal, confirm-delete, toggle-enabled).
    Cron,
}

/// Which scrollable zone inside the Memory view the wheel cursor is
/// hovering over. Used by the mouse-wheel handler so scrolling moves
/// the right thing (result selection vs. bank selection vs. detail
/// body scroll) instead of the whole main area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryScrollZone {
    Banks,
    Results,
    Detail,
}

/// Which content the main (left) area is currently showing. Chat
/// is the default per-agent view; Tasks is the in-TUI task board;
/// Channel routes the main area (and the chat input) to one of
/// `App::channels` indexed by the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveView {
    #[default]
    Chat,
    Tasks,
    Memory,
    /// Read-only listing of configured `ai_core` blocks with per-core
    /// usage percentages where a `UsageProvider` is resolved.
    Cores,
    /// User's personal inbox — messages sent to the reserved recipient
    /// id `"user"` land here. Counter rendered next to the entry in
    /// the Views list.
    Inbox,
    /// Cron jobs management — schedule, edit, and remove recurring or
    /// one-off payloads delivered to team members.
    Cron,
    Channel(usize),
    /// Property-entry form for the workflow at
    /// `config.workflows[workflow_idx]`.
    WorkflowForm {
        workflow_idx: usize,
    },
    /// Live progress view for the running or failed instance with id
    /// `instance_id` (belonging to the workflow at `workflow_idx`).
    WorkflowInstance {
        workflow_idx: usize,
        instance_id: u64,
    },
}

/// Entries in the right-column Views list. Extend this array and add
/// a matching activation arm in `handle_key_views` to wire a new
/// view up. Runtime filtering (e.g. hiding `Memory` when no banks are
/// declared) happens in [`App::view_entries`]; prefer that helper over
/// this const at every call site that has an `App` in scope.
pub const VIEW_ENTRIES: &[&str] = &["Inbox", "Tasks", "Memory", "Cores", "Cron"];

#[derive(Debug, Default, Clone)]
pub struct LayoutRects {
    pub agent_list_inner: Option<Rect>,
    pub chat_history_inner: Option<Rect>,
    pub chat_input_inner: Option<Rect>,
    /// Inner rect of the "views" panel on the right column. Populated
    /// by `render()` so mouse clicks on a view entry can hit-test
    /// against the same geometry the UI drew.
    pub views_list_inner: Option<Rect>,
    /// Inner rect of the "channels" panel on the right column.
    /// `None` if no channels are declared in the config.
    pub channels_list_inner: Option<Rect>,
    /// Inner rect of the "workflows" panel on the right column.
    /// `None` if no workflows are declared in the config.
    pub workflows_list_inner: Option<Rect>,
    /// Inner rect of the Cores view's top (list) panel. Populated each
    /// frame by `render_cores_screen` so click-to-select maps the
    /// click row back onto `CoresScreen::cursor`.
    pub cores_list_inner: Option<Rect>,
    /// Clickable Run/Cancel buttons on the workflow form view. Each
    /// entry is `(which_button, rect)`. Cleared every frame by
    /// `render_workflow_form` and populated when the form is drawn.
    pub workflow_form_buttons: Vec<(super::workflow_view::FormButton, Rect)>,
    /// Per-field hit-rects on the workflow form. Entry `(field_idx, rect)`.
    /// Cleared + repopulated each frame by `render_workflow_form`.
    pub workflow_form_fields: Vec<(usize, Rect)>,
    /// Per-checkbox hit-rects for MultiSelect fields on the workflow
    /// form. Entry `((field_idx, option_idx), rect)` — one rect per
    /// option row so clicks map to exactly one checkbox.
    pub workflow_form_multi_rects: Vec<((usize, usize), Rect)>,
    /// When the workflow Select picker is open, each visible option
    /// row gets a clickable rect here.
    pub workflow_form_picker_rows: Vec<Rect>,
    /// Outer rect of the open workflow Select picker. `Some` only while
    /// the picker is open. Clicks inside but not on a row are swallowed;
    /// clicks outside close the picker without committing.
    pub workflow_form_picker_outer: Option<Rect>,
    pub chat_buttons: Vec<ChatButton>,
    /// When a task form is open, every labelled field and button is
    /// given a clickable rect keyed by `FormField`. Populated each
    /// frame by `render_task_form` and cleared on every fresh
    /// frame. Clicking a field focuses it (and opens the picker on
    /// dropdowns); clicking a button activates it.
    pub task_form_fields: Vec<(FormField, Rect)>,
    /// Per-row hit rects for the expanded TODOs section in the task
    /// edit form. `Uuid` is the todo id; clicking the row toggles its
    /// status between Pending and Completed.
    pub task_form_todo_rows: Vec<(uuid::Uuid, Rect)>,
    /// Per-row hit rects for the expanded Comments section. Clicking
    /// the row sets `comments_cursor` to that index.
    pub task_form_comment_rows: Vec<(uuid::Uuid, Rect)>,
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
    /// Clickable rects for the Memory view's bank list (when the
    /// view is in the Banks stage). Populated each frame in `ui.rs`.
    pub memory_bank_rows: Vec<Rect>,
    /// Clickable rects for search-result rows in the Memory view.
    pub memory_result_rows: Vec<Rect>,
    /// Rect of the search-input bar on the Memory view. Click = focus
    /// search mode.
    pub memory_search_rect: Option<Rect>,
    /// Fields on the currently-open note form (if any). Click =
    /// focus that field.
    pub memory_form_fields: Vec<(super::memory_view::NoteField, Rect)>,
    /// Rows on the links overlay, when open. Click = move cursor.
    pub memory_link_rows: Vec<Rect>,
    /// Outer rect of whatever memory modal is active (form, detail,
    /// links overlay, confirm-delete). Used to distinguish outside-
    /// click (close) from inside-click (swallow).
    pub memory_modal_outer: Option<Rect>,
    /// Clickable hot-rects drawn by the Inbox view each frame.
    /// Painted top-to-bottom; the mouse handler picks the first hit
    /// so overlays stacked above the list take precedence.
    pub inbox_hotspots: Vec<(Rect, super::inbox_view::InboxHotspot)>,
    /// Inner rect of the Inbox view's list pane. Used by the wheel
    /// handler to scope scroll events to the message list.
    pub inbox_list_inner: Option<Rect>,
    /// Inner rect of the Inbox view's detail pane. Wheel events inside
    /// this rect scroll the message body; clicks switch the inbox sub-
    /// pane focus to `Detail`.
    pub inbox_detail_inner: Option<Rect>,
    /// Clickable hot-rects drawn by the Cron view each frame.
    /// Populated by `render_cron_screen` + `render_cron_modals`, and
    /// cleared at the start of every frame. Modal overlays are
    /// pushed last, so their hotspots take precedence in the linear
    /// "first-hit wins" scan.
    pub cron_hotspots: Vec<(Rect, super::cron_view::CronHotspot)>,
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
    /// User invoked `/sleep`. Mailbox/channel deliveries are refused
    /// and locally-typed plain text is rejected until `/awake`.
    Asleep,
}

/// Per-channel TUI state. Rendered in the main area when
/// `active_view == Channel(idx)`. The `blocks` vector mirrors the
/// disk-backed JSONL transcript (minus the cosmetic welcome banner,
/// which is always index 0 when present). Channels are agent-only —
/// the only non-agent participant is the human operator, who is
/// represented in the rendered transcript as the literal sender
/// "user" but has no entry in any `members` list.
#[derive(Debug, Default)]
pub struct ChannelState {
    pub id: String,
    /// Unused by the TUI today; preserved so future preamble updates
    /// can inject a per-channel description line under the banner.
    #[allow(dead_code)]
    pub description: Option<String>,
    /// Unused by the TUI today; the welcome block is materialized
    /// into `blocks[0]` at init, so rendering doesn't need to read
    /// this field again. Kept for /reset (clearing all non-welcome
    /// blocks re-reads whether this was ever set).
    #[allow(dead_code)]
    pub welcome: Option<String>,
    /// Agent ids that are members of this channel (verbatim from the
    /// WCL config). Preserved for the `<channels>` prompt section and
    /// for broadcast dispatch.
    #[allow(dead_code)]
    pub members: Vec<String>,
    /// Rendered transcript. The welcome banner (if any) is always at
    /// index 0 and is NOT persisted to disk. Everything else is a
    /// `ChatBlock::ChannelMessage` whose text came from (a) loading
    /// the JSONL tail at startup or (b) a live broadcast call.
    pub blocks: Vec<ChatBlock>,
    pub input: String,
    pub input_cursor: usize,
    /// Reserved for future wrap-aware channel input navigation;
    /// v1 renders the buffer on a single row with char-level cursor.
    #[allow(dead_code)]
    pub anchor: ScrollAnchor,
    pub sticky_bottom: bool,
    /// Reserved for future per-block selection (copy/collapse);
    /// channel messages are not collapsible today.
    #[allow(dead_code)]
    pub selected_block: Option<usize>,
    #[allow(dead_code)]
    pub anchor_scratch: Option<super::blocks::AnchorScratch>,
    pub input_scroll: u16,
    /// Cached wrap width from the last render. Key handlers read this
    /// so cursor-move/scroll math stays consistent with what's on
    /// screen. Zero until the first frame.
    pub input_wrap_width: u16,
    /// Cached visible row count from the last render. Used for scroll
    /// clamping. Zero until the first frame.
    pub input_visible_rows: u16,
    /// Append-only writer for this channel's JSONL file. `None` only
    /// in tests — the real TUI always supplies one via `tui::run`.
    pub log: Option<super::channel_log::ChannelLog>,
    /// `true` when this channel was created at runtime (via `/meeting`
    /// or `channel create`). Only ephemeral channels can be torn down
    /// by `/leave` / `channel end`. Not persisted — `App::new`
    /// rebuilds `app.channels` from `config.channels` as parsed from
    /// the WCL file, so any runtime-added entries in
    /// `self.config.channels` vanish on restart.
    pub is_ephemeral: bool,
}

/// Flavour of the chat entry shown in the agent list. Controls both
/// the row icon (circle family vs square family) and the runtime
/// backend (`run_core` for AI-driven kinds, `run_automaton_core` for
/// script-driven automatons). The discriminant is set once at spawn
/// and never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatKind {
    /// Statically declared AI agent from `agent { ... }` blocks.
    #[default]
    Agent,
    /// Ephemeral AI worker spawned from a `worker { ... }` template.
    Worker,
    /// Script-backed pseudo-agent from `automaton { ... }` blocks. No
    /// AI core; events spawn a child process whose stdout/stderr land
    /// in this chat pane.
    Automaton,
}

/// One row in the rendered agent-list sidebar. Group headers separate
/// `Chat` rows that share a `group` value; they're inserted by
/// `App::agent_list_visual_rows` so the renderer and the click
/// handler agree on row positions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentListVisualRow {
    /// Italic non-selectable section header. The label is the bare
    /// group name (or `"(ungrouped)"` for the catch-all bucket); the
    /// renderer adds visual chrome like `── {label} ──`.
    GroupHeader { label: String },
    /// Selectable chat row at the given indentation depth.
    Chat { idx: usize, depth: usize },
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
    /// Channel broadcasts. Bounded like `agent_tx`. Drained strictly
    /// after both `user_rx` and `agent_rx` by the biased select in
    /// `run_core`, so channel chatter can never starve direct tasking.
    /// `None` for workers (channels are agent-only).
    pub channel_tx: Option<mpsc::Sender<String>>,
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
    /// Flavour of the chat — drives the list icon and which runtime
    /// backend task is responsible for the chat's input queues. See
    /// [`ChatKind`].
    pub kind: ChatKind,
    /// For automatons only: the last child process's exit code.
    /// `None` until the first child exits or when a child is in
    /// flight. Used in tests; not read by the UI.
    #[allow(dead_code)]
    pub automaton_last_exit: Option<i32>,
    /// Id of the `ai_core` backing this chat (post-resolution of any
    /// agent/worker override). `None` until the agent is wired up; set
    /// once at spawn and preserved across worker hibernation/resurrection.
    /// Used to route `AIEvent::RateLimit` lines from the stream into
    /// the Cores view's usage map.
    pub ai_core_id: Option<String>,
    /// True after `worker complete`. Channels are dropped; the pane
    /// stays visible. Next inbound message triggers resurrection via
    /// `App::resurrect_worker`.
    pub hibernated: bool,
    /// Set on remote-worker chats when the upstream session has
    /// reached a terminal state (`FAILED`, or was archived via
    /// `worker archive`). Unlike local hibernated workers, these
    /// cannot be resumed — the backend session is gone — so
    /// `handle_send_message` skips auto-resurrection and returns a
    /// clear error instead of rebuilding a useless core.
    ///
    /// `COMPLETED` does NOT flip this: Jules treats completion as
    /// "idle, awaiting further input" — the parent can keep sending
    /// messages to request more changes, and only an explicit
    /// `worker archive` tears the remote session down.
    pub remote_terminal: bool,
    /// For remote-worker chats: the backend's own session identifier
    /// (e.g. `sessions/abc123` for Jules). Populated by the core via
    /// `RequestCmd::RemoteSessionBound` once the remote session is
    /// live, so `handle_worker_archive` can issue a real DELETE
    /// against the backend instead of relying on its inactivity TTL.
    /// `None` until that first registration arrives.
    pub remote_session_name: Option<String>,
    /// True between `/sleep` and `/awake`. While set, `handle_send_message`
    /// returns an "agent is asleep" error to the sender, channel
    /// broadcasts skip the agent, hibernated workers are NOT auto-
    /// resurrected, and locally-typed plain-text messages are
    /// rejected with a banner. Slash commands still run.
    pub asleep: bool,
    /// One-shot signal to the agent's `run_core` task that the
    /// current turn (if any) should be cancelled. Bounded with
    /// capacity 1 so `try_send` returns `Ok` for the first signal
    /// and silently drops follow-up signals while one is pending.
    /// `None` for hibernated workers (no `run_core` task is alive).
    pub cancel_tx: Option<mpsc::Sender<()>>,
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
    /// Broadcast sink for per-block wire events. When `Some`, every
    /// call to [`AgentChat::push_block`] also emits a
    /// [`crate::wire_chat::ChatBlockEvent`] tagged with this chat's
    /// `display_name`. Populated on chats by `App::install_chat_broadcaster`
    /// (called from `tui::run` for live sessions); left `None` in unit
    /// tests and for chats built before the broadcaster is installed —
    /// a `None` sink is a no-op, not an error.
    pub chat_block_tx: Option<broadcast::Sender<crate::wire_chat::ChatBlockEvent>>,
}

/// How an agent's running token total should incorporate each new
/// TurnResult's `usage`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContextUsageMode {
    /// Each new TurnResult carries the full cumulative session totals;
    /// overwrite. Used by Claude Code CLI and Gemini CLI.
    ///
    /// Conservative default: most callers will overwrite this at
    /// spawn anyway. `Replace` means a stray usage event won't
    /// accumulate forever if the spawn path forgets to set the mode.
    #[default]
    Replace,
    /// Each new TurnResult carries the delta for that round; add to
    /// the running total. Used by Anthropic API and OpenAI API cores.
    Add,
}

impl AgentChat {
    pub(super) fn push_block(&mut self, block: ChatBlock) -> usize {
        let idx = self.blocks.len();
        if let Some(tx) = &self.chat_block_tx {
            // Skip the projection cost entirely when nobody is subscribed —
            // the most common case for non-webservice sessions.
            if tx.receiver_count() > 0 {
                let ev = crate::wire_chat::ChatBlockEvent {
                    agent: self.display_name.clone(),
                    block: crate::wire_chat::WireChatBlock::from(&block),
                    ts: chrono::Utc::now().to_rfc3339(),
                    seq: idx as u64,
                };
                // `Err` only when there are zero receivers — already gated
                // above — or if the channel was closed. Drop silently.
                let _ = tx.send(ev);
            }
        }
        self.blocks.push(block);
        self.sticky_bottom = true;
        idx
    }

    /// Re-broadcast the block at `pos` to every SSE subscriber, reusing
    /// the original `seq` (the block's index). Subscribers replace the
    /// block at that index, which is how in-place mutations like a tool
    /// result resolving the `pending` flag reach connected clients.
    pub(super) fn emit_block_at(&self, pos: usize) {
        let Some(tx) = &self.chat_block_tx else {
            return;
        };
        if tx.receiver_count() == 0 {
            return;
        }
        let Some(block) = self.blocks.get(pos) else {
            return;
        };
        let ev = crate::wire_chat::ChatBlockEvent {
            agent: self.display_name.clone(),
            block: crate::wire_chat::WireChatBlock::from(block),
            ts: chrono::Utc::now().to_rfc3339(),
            seq: pos as u64,
        };
        let _ = tx.send(ev);
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

/// Abstracts over the two compose buffers we maintain (the per-agent
/// chat input and the per-channel input) so all the wrap-aware
/// helpers below — insert, backspace, caret navigation, scroll
/// clamping — can share the exact same logic. Both renderers cache
/// the wrap width and visible-row count they used on the last frame;
/// the key handlers read those back so their math matches what's on
/// screen.
pub(crate) trait TextInput {
    fn buf(&self) -> &str;
    fn buf_mut(&mut self) -> &mut String;
    fn cursor(&self) -> usize;
    fn set_cursor(&mut self, c: usize);
    fn scroll(&self) -> u16;
    fn set_scroll(&mut self, s: u16);
    fn wrap_width_cached(&self) -> u16;
    fn visible_rows_cached(&self) -> u16;
}

impl TextInput for AgentChat {
    fn buf(&self) -> &str {
        &self.input
    }
    fn buf_mut(&mut self) -> &mut String {
        &mut self.input
    }
    fn cursor(&self) -> usize {
        self.input_cursor
    }
    fn set_cursor(&mut self, c: usize) {
        self.input_cursor = c;
    }
    fn scroll(&self) -> u16 {
        self.input_scroll
    }
    fn set_scroll(&mut self, s: u16) {
        self.input_scroll = s;
    }
    fn wrap_width_cached(&self) -> u16 {
        self.input_wrap_width
    }
    fn visible_rows_cached(&self) -> u16 {
        self.input_visible_rows
    }
}

impl TextInput for ChannelState {
    fn buf(&self) -> &str {
        &self.input
    }
    fn buf_mut(&mut self) -> &mut String {
        &mut self.input
    }
    fn cursor(&self) -> usize {
        self.input_cursor
    }
    fn set_cursor(&mut self, c: usize) {
        self.input_cursor = c;
    }
    fn scroll(&self) -> u16 {
        self.input_scroll
    }
    fn set_scroll(&mut self, s: u16) {
        self.input_scroll = s;
    }
    fn wrap_width_cached(&self) -> u16 {
        self.input_wrap_width
    }
    fn visible_rows_cached(&self) -> u16 {
        self.input_visible_rows
    }
}

/// Wrap the input buffer and return one `(start, end)` byte span per
/// visual row. Accepts any [`TextInput`] so the renderer can use it
/// without caring whether it's an agent chat or a channel compose
/// buffer.
pub(crate) fn input_visual_rows(input: &dyn TextInput, width: u16) -> Vec<(usize, usize)> {
    wrap_rows(input.buf(), width)
}

/// Byte offset at the start of the caret's current visual row plus
/// the caret's column (in chars) within that row. Mirrors
/// [`AgentChat::input_cursor_visual`] but works on any [`TextInput`].
pub(crate) fn input_cursor_visual(input: &dyn TextInput, width: u16) -> (usize, usize, usize) {
    let rows = wrap_rows(input.buf(), width);
    if rows.is_empty() {
        return (0, 0, 0);
    }
    let cursor = input.cursor().min(input.buf().len());
    let mut row_idx = 0;
    for (i, &(start, end)) in rows.iter().enumerate() {
        if cursor >= start && cursor <= end {
            row_idx = i;
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
    let col = input.buf()[row_start..cursor].chars().count();
    (row_idx, row_start, col)
}

/// Effective wrap width to use in key-handling. Falls back to a sane
/// default when the input hasn't been rendered yet (e.g. during tests).
fn effective_input_wrap_width(input: &dyn TextInput) -> u16 {
    if input.wrap_width_cached() > 0 {
        input.wrap_width_cached()
    } else {
        u16::MAX
    }
}

/// Visible input rows. Mirrors [`effective_input_wrap_width`]: if
/// unset, fall back to the 5-row cap so scroll math works in tests.
fn effective_input_visible_rows(input: &dyn TextInput) -> u16 {
    if input.visible_rows_cached() > 0 {
        input.visible_rows_cached()
    } else {
        5
    }
}

fn input_insert(input: &mut dyn TextInput, ch: char) {
    let idx = input.cursor().min(input.buf().len());
    input.buf_mut().insert(idx, ch);
    input.set_cursor(idx + ch.len_utf8());
    clamp_input_scroll(input);
}

fn input_backspace(input: &mut dyn TextInput) {
    let cursor = input.cursor();
    if cursor == 0 {
        return;
    }
    let mut prev = cursor - 1;
    while prev > 0 && !input.buf().is_char_boundary(prev) {
        prev -= 1;
    }
    input.buf_mut().replace_range(prev..cursor, "");
    input.set_cursor(prev);
    clamp_input_scroll(input);
}

fn input_move_char(input: &mut dyn TextInput, delta: i32) {
    let cursor = input.cursor();
    if delta < 0 {
        if cursor == 0 {
            return;
        }
        let mut prev = cursor - 1;
        while prev > 0 && !input.buf().is_char_boundary(prev) {
            prev -= 1;
        }
        input.set_cursor(prev);
    } else if delta > 0 {
        let len = input.buf().len();
        if cursor >= len {
            return;
        }
        let mut next = cursor + 1;
        while next < len && !input.buf().is_char_boundary(next) {
            next += 1;
        }
        input.set_cursor(next);
    }
    clamp_input_scroll(input);
}

fn input_move_line(input: &mut dyn TextInput, delta: i32) {
    let width = effective_input_wrap_width(input);
    let rows = wrap_rows(input.buf(), width);
    let (row_idx, _row_start, col) = input_cursor_visual(input, width);
    let target_row = (row_idx as i32 + delta).clamp(0, rows.len() as i32 - 1) as usize;
    if target_row == row_idx {
        return;
    }
    let (t_start, t_end) = rows[target_row];
    let new_cursor = advance_chars(input.buf(), t_start, col, t_end);
    input.set_cursor(new_cursor);
    clamp_input_scroll(input);
}

fn input_move_to_row_start(input: &mut dyn TextInput) {
    let width = effective_input_wrap_width(input);
    let (_row_idx, row_start, _col) = input_cursor_visual(input, width);
    input.set_cursor(row_start);
    clamp_input_scroll(input);
}

fn input_move_to_row_end(input: &mut dyn TextInput) {
    let width = effective_input_wrap_width(input);
    let rows = wrap_rows(input.buf(), width);
    let (row_idx, _row_start, _col) = input_cursor_visual(input, width);
    if let Some(&(_, end)) = rows.get(row_idx) {
        input.set_cursor(end);
    }
    clamp_input_scroll(input);
}

/// Keep `scroll` such that the cursor's visual row is visible.
fn clamp_input_scroll(input: &mut dyn TextInput) {
    let width = effective_input_wrap_width(input);
    let visible = effective_input_visible_rows(input).max(1);
    let rows = wrap_rows(input.buf(), width);
    let total = rows.len() as u16;
    let (row_idx, _, _) = input_cursor_visual(input, width);
    let row_idx = row_idx as u16;
    let mut scroll = input.scroll();
    if row_idx < scroll {
        scroll = row_idx;
    } else if row_idx >= scroll + visible {
        scroll = row_idx + 1 - visible;
    }
    let max_scroll = total.saturating_sub(visible);
    if scroll > max_scroll {
        scroll = max_scroll;
    }
    input.set_scroll(scroll);
}

pub struct App {
    pub config: AciddogConfig,
    /// Absolute path the live `config` was loaded from. Set by
    /// [`super::boot::boot_orchestrator`] (or, in unit tests, left at
    /// the default empty `PathBuf`). Read by `/reload` to re-parse the
    /// same source after the user edits it.
    pub config_path: std::path::PathBuf,
    pub selected: usize,
    pub should_quit: bool,
    /// Graceful-shutdown latch. Set by `/shutdown`; once true the app
    /// sleeps idle agents immediately, lets busy agents finish their
    /// current turn (then sleeps them too), rejects new agent-originated
    /// mailbox / channel sends with "aciddog is shutting down", and
    /// stops firing cron triggers. The event loop sets `should_quit`
    /// as soon as every agent is non-Working.
    pub shutting_down: bool,
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
    /// Declared channels, one `ChannelState` per `channel` block.
    /// Order matches `config.channels` (declaration order). Empty if
    /// no channel blocks were declared.
    pub channels: Vec<ChannelState>,
    /// Which row of the right-column Channels list is highlighted.
    /// Bounded to `0..channels.len()` (saturating).
    pub selected_channel: usize,
    /// UI-local state for the Tasks screen: cursor, modals, prompts.
    pub tasks_screen: TasksScreen,
    /// UI-local state for the Memory view: bank list, search, modals.
    pub memory_screen: super::memory_view::MemoryScreen,
    /// Channel for replies from background memory ops (search, store,
    /// update, delete, link). Populated by `tui::run`; `None` only in
    /// unit tests that construct `App::new` directly and don't drive
    /// the memory view.
    pub memory_event_tx:
        Option<tokio::sync::mpsc::UnboundedSender<super::memory_view::MemoryEvent>>,
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
    /// Running and failed workflow instances as seen by the TUI.
    /// Successful instances are removed on arrival of their `Finished`
    /// event; failed ones stay until the user dismisses them.
    pub workflow_instances: Vec<super::workflow_view::WorkflowInstanceView>,
    /// Cursor into `config.workflows` for the workflows sidebar.
    pub selected_workflow: usize,
    /// Active property-entry form state, if the user is on a workflow
    /// form view. `None` everywhere else.
    pub workflow_form: Option<super::workflow_view::WorkflowFormState>,
    /// Monotonic id generator for new instances. Handed out to the
    /// backend at spawn time and echoed back on every `WorkflowEvent`.
    pub next_instance_id: u64,
    /// Owner of per-instance handles + the shared variable maps. Lives
    /// on `App` so the IPC WorkflowRun handler can invoke it directly.
    /// `None` means workflows are disabled for this session (early
    /// bring-up or unit tests).
    pub workflow_engine: Option<crate::workflow::WorkflowEngine>,
    /// State for the Cores view (right-column "Cores" entry). Rebuilt
    /// from `config.ai_cores` each time the view is opened.
    pub cores_screen: CoresScreen,
    /// Sender for replies from background usage-fetch tasks. Populated
    /// by `tui::run`; `None` in unit tests that don't open the Cores
    /// view.
    pub usage_event_tx: Option<tokio::sync::mpsc::UnboundedSender<UsageEvent>>,
    /// Messages addressed to the human operator (`to = "user"`). Held
    /// in memory only — matches the ephemeral agent mailbox model and
    /// does not persist across restarts.
    pub inbox: Vec<super::inbox_view::InboxMessage>,
    /// UI-local state for the Inbox view (cursor, reply/forward forms).
    pub inbox_screen: super::inbox_view::InboxScreen,
    /// UI-local state for the Cron view (cursor, edit/add form, delete
    /// confirmation prompt, transient status toasts).
    pub cron_screen: super::cron_view::CronScreen,
    /// Broadcast sender for per-block wire events, cloned into every
    /// [`AgentChat`] so every `push_block` fans out to any subscribed
    /// receiver (currently just the webservice's scoped SSE handler).
    /// Kept at the `App` level so worker-spawn paths can attach this
    /// sink to the new chat at construction time.
    pub chat_block_tx: broadcast::Sender<crate::wire_chat::ChatBlockEvent>,
    /// Broadcast sender for channel wire events. Fed from
    /// [`App::broadcast_to_channel`] every time a message is
    /// appended to any channel's transcript — regardless of whether
    /// the webservice automaton is a member of that channel — so the
    /// supervisor-scoped `/v1/channel/events` stream can follow any
    /// channel.
    pub channel_message_tx: broadcast::Sender<crate::wire_channel::ChannelMessageEvent>,
    /// Broadcast sender for workflow events. Populated at the end of
    /// [`App::on_workflow_event`], after the TUI mirror is updated.
    /// The supervisor-scoped `/v1/workflow/events` handler subscribes
    /// here and optionally filters by `instance_id`.
    pub workflow_event_tx: broadcast::Sender<crate::wire_workflow::WireWorkflowEvent>,
    /// Broadcast sender for cron mutations. Published from the cron
    /// mutation handlers (add/update/remove) and from `tick_cron` on
    /// each fire. The supervisor-scoped `/v1/cron/events` handler
    /// subscribes here.
    pub cron_event_tx: broadcast::Sender<crate::cron::CronEvent>,
}

/// Time window for detecting a double-click. Matches the typical
/// OS default of 500ms.
pub const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(500);

/// Produce the `AgentChat` slot for a configured agent — seeded with
/// the AgentInfo block, model/window metadata, and display name. Used
/// by [`App::new`] at boot and by reload's "agent added" path.
pub(super) fn make_agent_chat(config: &AciddogConfig, agent: &crate::config::Agent) -> AgentChat {
    let core = config.effective_ai_core(agent.ai_core.as_deref());
    let tools = config.effective_tools(agent.tools.as_deref(), core);
    let label = core
        .map(core_display_label)
        .unwrap_or_else(|| "claude-code".to_string());
    let model = config.effective_model(agent.model.as_deref(), core);
    let context_window = model.and_then(crate::ai::model_context_window);
    let context_usage_mode = core
        .map(|c| core_usage_mode(&c.kind))
        .unwrap_or(ContextUsageMode::Replace);
    let info = ChatBlock::agent_info(label, tools, agent.instructions.clone(), context_window);
    AgentChat {
        blocks: vec![info],
        sticky_bottom: true,
        display_name: agent.id.clone(),
        context_window,
        context_usage_mode,
        kind: ChatKind::Agent,
        ..Default::default()
    }
}

/// Produce the `AgentChat` slot for a configured automaton. Same role
/// as [`make_agent_chat`] but emits the synthetic "automaton (...)"
/// label and uses the role description for the body since automatons
/// have no AI core / instructions.
pub(super) fn make_automaton_chat(automaton: &crate::config::Automaton) -> AgentChat {
    let label = match &automaton.backend {
        crate::config::AutomatonBackend::Script { command, args, .. } => {
            let argv = std::iter::once(command.as_str())
                .chain(args.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            format!("automaton ({argv})")
        }
        crate::config::AutomatonBackend::Webservice { bind, .. } => match bind {
            crate::config::WebserviceBind::Https { address, .. } => {
                format!("automaton (webservice https://{address})")
            }
            crate::config::WebserviceBind::UnixSocket { path } => {
                format!("automaton (webservice unix:{})", path.display())
            }
        },
        crate::config::AutomatonBackend::Remote {
            transport,
            remote_target,
            ..
        } => match transport {
            crate::config::RemoteTransport::Https { url, .. } => {
                format!("automaton (remote {url} → {remote_target})")
            }
            crate::config::RemoteTransport::UnixSocket { path } => {
                format!(
                    "automaton (remote unix:{} → {remote_target})",
                    path.display()
                )
            }
        },
    };
    let info = ChatBlock::agent_info(
        label,
        Vec::new(),
        automaton
            .role_description
            .clone()
            .unwrap_or_else(|| automaton.role.clone()),
        None,
    );
    AgentChat {
        blocks: vec![info],
        sticky_bottom: true,
        display_name: automaton.id.clone(),
        kind: ChatKind::Automaton,
        ..Default::default()
    }
}

/// Result of resolving a task `assignee` string against the live config.
/// `Chat` is a configured agent / automaton / spawned worker (matches a
/// chat `display_name`); `Group` is a declared group label; `User` is
/// the reserved `user` recipient routed to the operator's inbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssigneeKind {
    Chat,
    Group,
    User,
}

impl App {
    pub fn new(config: AciddogConfig) -> Self {
        let channels: Vec<ChannelState> = config
            .channels
            .iter()
            .map(|c| {
                // Seed the display buffer with the welcome banner so
                // it sits above any live messages. Real disk history
                // is loaded later by `tui::run`, which inserts those
                // blocks after the welcome (index 1+).
                let mut blocks: Vec<ChatBlock> = Vec::new();
                if let Some(w) = c.welcome.as_ref().filter(|s| !s.trim().is_empty()) {
                    blocks.push(ChatBlock::channel_welcome(w.clone()));
                }
                ChannelState {
                    id: c.id.clone(),
                    description: c.description.clone(),
                    welcome: c.welcome.clone(),
                    members: c.members.clone(),
                    blocks,
                    sticky_bottom: true,
                    ..Default::default()
                }
            })
            .collect();

        let mut chats: Vec<AgentChat> = config
            .agents
            .iter()
            .map(|agent| make_agent_chat(&config, agent))
            .collect();

        // Automatons are peer fleet members in the agent list. They
        // don't carry an AI core, so the seeded `AgentInfo` block uses
        // a synthetic label ("automaton: <command>") and the
        // `instructions` slot carries the role description for
        // visibility in the expanded panel.
        for automaton in &config.automatons {
            chats.push(make_automaton_chat(automaton));
        }
        // Create a single block broadcaster and hand each chat a clone
        // of the sender. Sender clones are cheap (internally an Arc),
        // `receiver_count()` starts at 0 so the `push_block` fast-path
        // skips projection work until someone subscribes.
        let (chat_block_tx, _keepalive_rx) =
            broadcast::channel::<crate::wire_chat::ChatBlockEvent>(CHAT_BLOCK_BROADCAST_CAPACITY);
        // NOTE: `_keepalive_rx` is dropped immediately — that's fine.
        // `Sender::subscribe()` creates fresh receivers on demand from
        // the SSE handler; the sender stays alive as long as `App` does.
        for chat in &mut chats {
            chat.chat_block_tx = Some(chat_block_tx.clone());
        }
        // Parallel broadcaster for channel messages — fed from
        // `broadcast_to_channel` so the supervisor SSE stream can
        // observe any channel regardless of local membership.
        let (channel_message_tx, _channel_keepalive_rx) = broadcast::channel::<
            crate::wire_channel::ChannelMessageEvent,
        >(CHAT_BLOCK_BROADCAST_CAPACITY);
        // Parallel broadcaster for workflow events. The supervisor
        // `/v1/workflow/events` stream subscribes here. See
        // `on_workflow_event` for the fan-out site.
        let (workflow_event_tx, _workflow_keepalive_rx) = broadcast::channel::<
            crate::wire_workflow::WireWorkflowEvent,
        >(CHAT_BLOCK_BROADCAST_CAPACITY);
        // Parallel broadcaster for cron events. The supervisor
        // `/v1/cron/events` stream subscribes here.
        let (cron_event_tx, _cron_keepalive_rx) =
            broadcast::channel::<crate::cron::CronEvent>(CHAT_BLOCK_BROADCAST_CAPACITY);
        Self {
            config,
            config_path: std::path::PathBuf::new(),
            selected: 0,
            should_quit: false,
            shutting_down: false,
            focus: Focus::Chat,
            chats,
            chat_block_tx,
            channel_message_tx,
            workflow_event_tx,
            cron_event_tx,
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
            channels,
            selected_channel: 0,
            tasks_screen: TasksScreen::default(),
            memory_screen: super::memory_view::MemoryScreen::default(),
            memory_event_tx: None,
            last_click: None,
            frame_tick: 0,
            compaction_tx: None,
            workflow_instances: Vec::new(),
            selected_workflow: 0,
            workflow_form: None,
            next_instance_id: 1,
            workflow_engine: None,
            cores_screen: CoresScreen::default(),
            usage_event_tx: None,
            inbox: Vec::new(),
            inbox_screen: super::inbox_view::InboxScreen::default(),
            cron_screen: super::cron_view::CronScreen::default(),
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

    /// Visible rows in the agent-list sidebar — either a group
    /// section header (non-selectable, inserted at group transitions)
    /// or a chat row (selectable). The renderer walks this to draw
    /// the list and the click handler walks it to map a clicked row
    /// back to a chat index, so the two stay in sync as group headers
    /// shift the visual position of chat rows.
    pub fn agent_list_visual_rows(&self) -> Vec<AgentListVisualRow> {
        let order = self.visible_order();
        let mut out: Vec<AgentListVisualRow> = Vec::with_capacity(order.len() + 4);
        let mut current_group: Option<Option<String>> = None;
        for (i, depth) in order {
            if depth == 0 {
                let g = self.chat_group(i).map(|s| s.to_string());
                let needs_header = match &current_group {
                    None => true,
                    Some(prev) => prev != &g,
                };
                if needs_header {
                    let label = g.clone().unwrap_or_else(|| "(ungrouped)".to_string());
                    out.push(AgentListVisualRow::GroupHeader { label });
                    current_group = Some(g);
                }
            }
            out.push(AgentListVisualRow::Chat { idx: i, depth });
        }
        out
    }

    /// Display order for the agent list: chat indices paired with
    /// their indentation depth. Hibernated chats are skipped. Workers
    /// appear immediately after the nearest *visible* ancestor; if
    /// that ancestor is hidden the worker floats up to the next one.
    ///
    /// Top-level chats are bucketed by their `group` field in
    /// first-declaration order (named groups first, ungrouped last).
    /// Within each bucket leader-flagged chats pin first, then the
    /// rest in declaration order. Group section headers are NOT
    /// emitted by this function — `agent_list_visual_rows` wraps
    /// this output with the headers the renderer + click handler
    /// share. See `App::chat_group`.
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

        // Precompute leader-flag for every chat. Only top-level Agents
        // and Automatons whose config block has `leader = true` count.
        // Workers (and chats that have ended up nested via
        // visible_parent) never lift to the leaders bucket.
        let leader: Vec<bool> = (0..self.chats.len()).map(|i| self.is_leader(i)).collect();

        // Effective group per top-level chat. Workers (and other
        // nested chats) don't need their own bucket since they're
        // emitted recursively under their visible parent — `None` for
        // them, and we skip them in the bucket walk.
        let group_of: Vec<Option<String>> = (0..self.chats.len())
            .map(|i| {
                if visible_parent[i].is_some() {
                    None
                } else {
                    self.chat_group(i).map(|s| s.to_string())
                }
            })
            .collect();

        // Build the ordered list of bucket labels by walking top-level
        // visible chats once in declaration order. `None` is appended
        // at the end if it's not already present, so ungrouped rows
        // always render below named groups.
        let mut buckets: Vec<Option<String>> = Vec::new();
        for (i, c) in self.chats.iter().enumerate() {
            if c.hibernated || visible_parent[i].is_some() {
                continue;
            }
            let g = group_of[i].clone();
            if !buckets.iter().any(|b| b == &g) {
                buckets.push(g);
            }
        }
        if !buckets.iter().any(|b| b.is_none()) {
            buckets.push(None);
        }

        let mut out: Vec<(usize, usize)> = Vec::with_capacity(self.chats.len());
        // Iterative DFS keyed by "what's the visible parent we're
        // currently listing children of?". Stable recursion is fine
        // here because `chats` is small (tens of entries at most).
        //
        // At the top level we walk one bucket at a time. Within each
        // bucket: pass 1 picks leader-flagged chats (declaration
        // order), pass 2 picks the rest (declaration order). We do NOT
        // reorder `self.chats` itself — `parent_idx` is a usize index
        // into that vec and reshuffling it would corrupt worker links.
        // Nested levels (workers under their parent) keep the original
        // single-pass declaration order regardless of bucketing.
        fn walk_children(
            chats: &[AgentChat],
            vp: &[Option<usize>],
            out: &mut Vec<(usize, usize)>,
            parent: usize,
            depth: usize,
        ) {
            for (i, c) in chats.iter().enumerate() {
                if c.hibernated {
                    continue;
                }
                if vp[i] == Some(parent) {
                    out.push((i, depth));
                    walk_children(chats, vp, out, i, depth + 1);
                }
            }
        }

        for bucket in &buckets {
            // Pass 1: leader-flagged top-level chats in this bucket.
            for (i, c) in self.chats.iter().enumerate() {
                if c.hibernated || visible_parent[i].is_some() {
                    continue;
                }
                if &group_of[i] == bucket && leader[i] {
                    out.push((i, 0));
                    walk_children(&self.chats, &visible_parent, &mut out, i, 1);
                }
            }
            // Pass 2: everyone else in this bucket.
            for (i, c) in self.chats.iter().enumerate() {
                if c.hibernated || visible_parent[i].is_some() {
                    continue;
                }
                if &group_of[i] == bucket && !leader[i] {
                    out.push((i, 0));
                    walk_children(&self.chats, &visible_parent, &mut out, i, 1);
                }
            }
        }
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

        // Plain-text path: refuse to deliver if the agent is asleep.
        // The user's typed message has already been echoed as a user
        // block above; follow it with a banner explaining why nothing
        // happened so the conversation stays self-explanatory.
        if self.chats.get(idx).map(|c| c.asleep).unwrap_or(false) {
            if let Some(chat) = self.chats.get_mut(idx) {
                chat.push_block(ChatBlock::system_warn(
                    "agent is asleep — type /awake to resume before sending messages",
                ));
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
        let pool_len = self.config.thinking_messages.len();
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
                        ChatBlock::from_tool_use("aciddog skill list", &serde_json::json!({}))
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
                    chat.emit_block_at(pos);
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
            AIEvent::Stopped => {
                chat.push_block(ChatBlock::system_info("[stopped]"));
                if chat.status == AgentStatus::Working {
                    chat.status = if chat.asleep {
                        AgentStatus::Asleep
                    } else {
                        AgentStatus::Ready
                    };
                    chat.end_working();
                }
            }
            AIEvent::RateLimit {
                kind,
                resets_at_unix,
                ..
            } => {
                // The claude CLI emits `rate_limit_event` on every turn;
                // route the window-reset info to the Cores view keyed by
                // the core id we stashed at spawn. Clone the id so the
                // `chat` borrow drops before we touch `self.cores_screen`.
                let core_id = chat.ai_core_id.clone();
                if let Some(id) = core_id {
                    self.cores_screen
                        .merge_rate_limit(&id, kind, resets_at_unix);
                }
            }
            AIEvent::Other(v) => {
                debug!(event = ?v, agent_idx = idx, "unhandled ai event");
            }
            AIEvent::AutomatonStarted => {
                chat.begin_working(pool_len);
            }
            AIEvent::AutomatonStdout { line } => {
                chat.push_block(ChatBlock::AutomatonStdout { line });
            }
            AIEvent::AutomatonStderr { line } => {
                chat.push_block(ChatBlock::AutomatonStderr { line });
            }
            AIEvent::AutomatonExit {
                code,
                duration_ms,
                timed_out,
            } => {
                chat.automaton_last_exit = code;
                let (status, note): (AgentStatus, ChatBlock) = if timed_out {
                    (
                        AgentStatus::Error,
                        ChatBlock::system_error(format!(
                            "[timed out after {duration_ms}ms; child killed]"
                        )),
                    )
                } else {
                    match code {
                        Some(0) => (
                            AgentStatus::Ready,
                            ChatBlock::system_info(format!("[exit 0 in {duration_ms}ms]")),
                        ),
                        Some(n) => (
                            AgentStatus::Error,
                            ChatBlock::system_error(format!("[exit {n} in {duration_ms}ms]")),
                        ),
                        None => (
                            AgentStatus::Error,
                            ChatBlock::system_error(format!(
                                "[killed by signal after {duration_ms}ms]"
                            )),
                        ),
                    }
                };
                chat.push_block(note);
                chat.status = status;
                chat.end_working();
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
            RequestCmd::WorkerCreate {
                template,
                task,
                properties,
            } => {
                self.handle_worker_create(idx, template, task, properties, reply);
            }
            RequestCmd::WorkerComplete { body } => {
                self.handle_worker_complete(idx, body, reply);
            }
            RequestCmd::WorkerArchive { worker } => {
                self.handle_worker_archive(idx, worker, reply);
            }
            RequestCmd::RemoteSessionBound { name } => {
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.remote_session_name = Some(name);
                }
                let _ = reply.send(Response::ok("bound"));
            }
            RequestCmd::WorkflowRun {
                workflow,
                properties,
            } => {
                self.handle_workflow_run(idx, workflow, properties, reply);
            }
            RequestCmd::WorkflowTemplatesJson => {
                self.handle_workflow_templates_json(idx, reply);
            }
            RequestCmd::WorkflowInstancesJson => {
                self.handle_workflow_instances_json(idx, reply);
            }
            RequestCmd::WorkflowDismiss { instance_id } => {
                self.handle_workflow_dismiss(idx, instance_id, reply);
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
            RequestCmd::TaskListJson {
                assignee,
                status,
                mine,
                unassigned,
            } => {
                self.handle_admin_task_list_json(idx, assignee, status, mine, unassigned, reply);
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
            RequestCmd::MemoryUpdate {
                bank,
                id,
                body,
                title,
                tags,
            } => {
                self.handle_memory_update(idx, bank, id, body, title, tags, reply);
            }
            RequestCmd::MemoryRemove { bank, id } => {
                self.handle_memory_remove(idx, bank, id, reply);
            }
            RequestCmd::MemoryShow { bank, id } => {
                self.handle_memory_show(idx, bank, id, reply);
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
            RequestCmd::MemoryBanks => {
                self.handle_admin_memory_banks(idx, reply);
            }
            RequestCmd::MemorySearchJson {
                bank,
                query,
                title_contains,
                include_tags,
                exclude_tags,
                limit,
            } => {
                self.handle_admin_memory_search_json(
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
            RequestCmd::MemoryStoreJson {
                bank,
                body,
                title,
                tags,
            } => {
                self.handle_admin_memory_store_json(idx, bank, body, title, tags, reply);
            }
            RequestCmd::MemoryLinksJson {
                bank,
                id,
                direction,
                include_tags,
                exclude_tags,
            } => {
                self.handle_admin_memory_links_json(
                    idx,
                    bank,
                    id,
                    direction,
                    include_tags,
                    exclude_tags,
                    reply,
                );
            }
            RequestCmd::MemoryGraph { bank } => {
                self.handle_admin_memory_graph(idx, bank, reply);
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
            RequestCmd::CronListJson {
                target,
                include_disabled,
                mine,
            } => {
                self.handle_cron_list_json(idx, target, include_disabled, mine, reply);
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
            RequestCmd::ChannelSend { id, body } => {
                self.handle_channel_send_from_agent(idx, id, body, reply);
            }
            RequestCmd::ChannelCreate { id } => {
                self.handle_channel_create_from_agent(idx, id, reply);
            }
            RequestCmd::ChannelJoin { id } => {
                self.handle_channel_join_from_agent(idx, id, reply);
            }
            RequestCmd::ChannelAdd { id, member } => {
                self.handle_channel_add_from_agent(idx, id, member, reply);
            }
            RequestCmd::ChannelKick { id, member } => {
                self.handle_channel_kick_from_agent(idx, id, member, reply);
            }
            RequestCmd::ChannelEnd { id } => {
                self.handle_channel_end_from_agent(idx, id, reply);
            }
            RequestCmd::AgentList => {
                self.handle_admin_agent_list(idx, reply);
            }
            RequestCmd::AgentStatus { agent } => {
                self.handle_admin_agent_status(idx, agent, reply);
            }
            RequestCmd::AgentHistory { agent, selector } => {
                self.handle_admin_agent_history(idx, agent, selector, reply);
            }
            RequestCmd::AgentHistoryJson { agent, selector } => {
                self.handle_admin_agent_history_json(idx, agent, selector, reply);
            }
            RequestCmd::AgentStop { agent, reason } => {
                self.handle_admin_agent_stop(idx, agent, reason, reply);
            }
            RequestCmd::AgentSleep { agent } => {
                self.handle_admin_agent_sleep(idx, agent, reply);
            }
            RequestCmd::AgentWake { agent } => {
                self.handle_admin_agent_wake(idx, agent, reply);
            }
            RequestCmd::AgentSendAsUser { to, body } => {
                self.handle_admin_agent_send_as_user(idx, to, body, reply);
            }
            RequestCmd::TeamInfo => {
                self.handle_admin_team_info(idx, reply);
            }
            RequestCmd::ChannelList => {
                self.handle_admin_channel_list(idx, reply);
            }
            RequestCmd::ChannelHistory { channel } => {
                self.handle_admin_channel_history(idx, channel, reply);
            }
            RequestCmd::ChannelSendAsUser { channel, body } => {
                self.handle_admin_channel_send_as_user(idx, channel, body, reply);
            }
            RequestCmd::InboxList => {
                self.handle_admin_inbox_list(idx, reply);
            }
            RequestCmd::InboxDelete { index } => {
                self.handle_admin_inbox_delete(idx, index, reply);
            }
            RequestCmd::WorkerList { parent } => {
                self.handle_admin_worker_list(idx, parent, reply);
            }
            RequestCmd::WorkerForceArchive { worker } => {
                self.handle_admin_worker_force_archive(idx, worker, reply);
            }
            RequestCmd::ServerShutdown => {
                self.handle_admin_server_shutdown(idx, reply);
            }
            RequestCmd::ServerExit => {
                self.handle_admin_server_exit(idx, reply);
            }
            RequestCmd::ServerReload => {
                self.handle_admin_server_reload(idx, reply);
            }
            RequestCmd::TeamRead => {
                self.handle_admin_team_read(idx, reply);
            }
            RequestCmd::TeamWrite { wcl } => {
                self.handle_admin_team_write(idx, wcl, reply);
            }
            RequestCmd::SkillsList => {
                self.handle_admin_skills_list(idx, reply);
            }
            RequestCmd::SkillRead { name } => {
                self.handle_admin_skill_read(idx, name, reply);
            }
            RequestCmd::SkillWrite { name, files } => {
                self.handle_admin_skill_write(idx, name, files, reply);
            }
            RequestCmd::SkillRemove { name } => {
                self.handle_admin_skill_remove(idx, name, reply);
            }
        }
    }

    /// Handle a `WorkflowRun` IPC request: resolve the sender's
    /// template id, check its `can_run_workflows` allowlist, coerce
    /// properties, and hand off to the engine. Reply carries the new
    /// instance id or a human-readable error.
    fn handle_workflow_run(
        &mut self,
        sender_idx: usize,
        workflow_id: String,
        properties: std::collections::BTreeMap<String, serde_json::Value>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        // Resolve the sender's "template id" for the allowlist check.
        // Workers are identified by their spawn template; agents and
        // automatons by their configured id (which the chat records as
        // `display_name`). Supervisor automatons bypass the allowlist
        // entirely — this is how the manager portal drives workflow
        // runs through a `supervisor = true` gateway automaton without
        // needing a `can_run_workflows` entry there.
        let is_supervisor = self.is_supervisor(sender_idx);
        let sender_template = match self.chats.get(sender_idx) {
            Some(c) if c.worker_template.is_some() => c.worker_template.clone().unwrap(),
            Some(c) => c.display_name.clone(),
            None => {
                let _ = reply.send(Response::err("unknown sender"));
                return;
            }
        };
        if !is_supervisor && !self.config.can_run_workflow(&sender_template, &workflow_id) {
            let _ = reply.send(Response::err(format!(
                "`{sender_template}` is not permitted to run workflow `{workflow_id}` (check can_run_workflows)"
            )));
            return;
        }
        let Some(workflow) = self
            .config
            .workflows
            .iter()
            .find(|w| w.id == workflow_id)
            .cloned()
        else {
            let _ = reply.send(Response::err(format!(
                "no workflow `{workflow_id}` declared"
            )));
            return;
        };
        let workflow_idx = self
            .config
            .workflows
            .iter()
            .position(|w| w.id == workflow_id)
            .expect("found above");

        // Coerce every supplied property against its declared kind.
        // Unsupplied properties fall through to declared defaults via
        // `seed_vars`; required-with-no-value is caught below.
        let mut user_inputs: std::collections::HashMap<String, crate::workflow::PropertyValue> =
            std::collections::HashMap::new();
        for (name, raw) in properties {
            let Some(prop) = workflow.properties.iter().find(|p| p.id == name) else {
                let _ = reply.send(Response::err(format!(
                    "workflow `{workflow_id}` has no property `{name}`"
                )));
                return;
            };
            match crate::ai::tools::workflow_set::WorkflowSetTool::coerce(&prop.kind, &raw) {
                Ok(v) => {
                    user_inputs.insert(name, v);
                }
                Err(e) => {
                    let _ = reply.send(Response::err(format!("`{name}`: {e}")));
                    return;
                }
            }
        }

        // Enforce required-with-no-value: required property ids that
        // aren't in user_inputs AND don't have a declared default.
        for prop in &workflow.properties {
            if !prop.required {
                continue;
            }
            if user_inputs.contains_key(&prop.id) {
                continue;
            }
            let has_default = match &prop.kind {
                crate::config::PropertyKind::Text { default }
                | crate::config::PropertyKind::Hidden { default } => default.is_some(),
                crate::config::PropertyKind::Number { default, .. } => default.is_some(),
                crate::config::PropertyKind::Select { default, .. } => default.is_some(),
                crate::config::PropertyKind::MultiSelect { default, .. } => default.is_some(),
            };
            if !has_default {
                let _ = reply.send(Response::err(format!(
                    "required property `{}` not supplied and has no default",
                    prop.id
                )));
                return;
            }
        }

        let merged = crate::workflow::WorkflowEngine::seed_vars(&workflow, user_inputs);
        let engine = match self.workflow_engine.as_mut() {
            Some(e) => e,
            None => {
                let _ = reply.send(Response::err("workflow engine not initialized"));
                return;
            }
        };
        let instance_id = self.next_instance_id;
        self.next_instance_id += 1;
        engine.start_instance(instance_id, &workflow, merged);
        self.workflow_instances
            .push(super::workflow_view::WorkflowInstanceView::new(
                instance_id,
                workflow_idx,
                &workflow,
            ));
        let _ = reply.send(Response::ok(format!(
            "started workflow `{workflow_id}` as instance #{instance_id}"
        )));
    }

    /// Supervisor: serialize `self.config.workflows` into a wire-friendly
    /// template list. Used by the manager portal to render the property
    /// form.
    fn handle_workflow_templates_json(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let templates: Vec<crate::wire_workflow::WireWorkflowTemplate> = self
            .config
            .workflows
            .iter()
            .map(crate::wire_workflow::WireWorkflowTemplate::from_config)
            .collect();
        match serde_json::to_value(&templates) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize workflow templates: {e}")));
            }
        }
    }

    /// Supervisor: serialize the TUI's live `workflow_instances` mirror.
    /// Matches the sidebar/progress surface operators see in the TUI.
    fn handle_workflow_instances_json(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let instances: Vec<crate::wire_workflow::WireWorkflowInstance> = self
            .workflow_instances
            .iter()
            .map(crate::wire_workflow::WireWorkflowInstance::from_view)
            .collect();
        match serde_json::to_value(&instances) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize workflow instances: {e}")));
            }
        }
    }

    /// Supervisor: drop a failed instance. Parity with the TUI `d`
    /// keybind — running instances cannot be dismissed.
    fn handle_workflow_dismiss(
        &mut self,
        sender_idx: usize,
        instance_id: u64,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        match self.dismiss_workflow_instance(instance_id) {
            Ok(()) => {
                let _ = reply.send(Response::ok(format!(
                    "dismissed workflow instance #{instance_id}"
                )));
            }
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
            }
        }
    }

    fn handle_worker_create(
        &mut self,
        sender_idx: usize,
        template: String,
        task: String,
        properties: Option<serde_json::Value>,
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
        // Workers may only message their parent; never the user.
        let team_block = build_team_block(&self.config, &contactable_for_worker, &creatable, false);

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
                // Workers never participate in channels.
                channels: Vec::new(),
                worker: Some(WorkerCtx {
                    parent_id: sender_display.clone(),
                    task: task.clone(),
                }),
                // Workers never supervise, even if their parent does.
                supervisor: false,
                sysadmin: false,
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
        let effective_model = self
            .config
            .effective_model(worker.model.as_deref(), Some(core));
        let context_window = effective_model.and_then(crate::ai::model_context_window);
        let context_usage_mode = core_usage_mode(&core.kind);
        let built_core: Box<dyn AICore> = match build_core(
            core,
            session_id,
            parts,
            tools.clone(),
            env,
            socket_tx_for_core,
            false,
            worker.model.as_deref(),
            properties.as_ref(),
        ) {
            Ok(c) => c,
            Err(e) => {
                let _ = reply.send(Response::err(format!(
                    "cannot spawn worker `{template}`: {e}"
                )));
                return;
            }
        };
        let assembled_prompt = built_core.first_turn_prompt();
        let info_block =
            ChatBlock::agent_info(core_label, tools.clone(), assembled_prompt, context_window);
        let pool_len = self.config.thinking_messages.len();
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);
        let mut chat = AgentChat {
            blocks: vec![info_block],
            sticky_bottom: true,
            user_tx: Some(user_tx.clone()),
            agent_tx: Some(agent_tx),
            cancel_tx: Some(cancel_tx),
            session_id: Some(session_id),
            status: AgentStatus::Working,
            thinking_message_idx: Some(pseudo_random(pool_len)),
            parent_idx: Some(sender_idx),
            worker_template: Some(template.clone()),
            display_name: display_name.clone(),
            ai_core_id: Some(core.id.clone()),
            hibernated: false,
            context_window,
            context_usage_mode,
            kind: ChatKind::Worker,
            chat_block_tx: Some(self.chat_block_tx.clone()),
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

        // Workers never participate in channels. Give `run_core` a
        // pre-closed `channel_rx` so its biased select treats the
        // channel branch as permanently disabled.
        let (_wkr_ch_tx, wkr_ch_rx) = mpsc::channel::<String>(1);
        drop(_wkr_ch_tx);
        tokio::spawn(run_core(
            new_idx, built_core, user_rx, agent_rx, wkr_ch_rx, cancel_rx, ai_tx,
        ));

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
        //
        // Remote-worker cores (Jules) use this same path on COMPLETED /
        // FAILED, but the backend session is already gone — flip
        // `remote_terminal` so `handle_send_message` refuses to
        // resurrect us. Detect by the chat's ai_core kind rather than
        // threading a flag through every caller.
        let is_remote = {
            let chat = self.chats.get(sender_idx);
            let core_id = chat.and_then(|c| c.ai_core_id.clone());
            core_id
                .and_then(|id| self.config.ai_cores.iter().find(|c| c.id == id).cloned())
                .map(|c| matches!(c.kind, AiCoreKind::Jules { .. }))
                .unwrap_or(false)
        };
        if let Some(chat) = self.chats.get_mut(sender_idx) {
            chat.push_block(ChatBlock::WorkerComplete {
                body: body.clone(),
                is_error: false,
                collapsed: true,
                pending: false,
            });
            chat.user_tx = None;
            chat.agent_tx = None;
            chat.cancel_tx = None;
            chat.status = AgentStatus::Hibernated;
            chat.hibernated = true;
            if is_remote {
                chat.remote_terminal = true;
            }
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

    /// Parent-initiated close of a worker. Tears down the backend
    /// session (for remote-worker cores) and hibernates the local
    /// pane. Validated to be called by the worker's parent — any
    /// other caller gets a clear permission error.
    fn handle_worker_archive(
        &mut self,
        sender_idx: usize,
        worker_display: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        self.archive_worker(sender_idx, worker_display, false, reply);
    }

    /// Shared archive body used by both `handle_worker_archive`
    /// (parent-gated) and the supervisor-only `WorkerForceArchive`.
    /// `bypass_parent_check` skips the "only the parent may archive"
    /// rule; everything else is identical.
    fn archive_worker(
        &mut self,
        sender_idx: usize,
        worker_display: String,
        bypass_parent_check: bool,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(target_idx) = self
            .chats
            .iter()
            .position(|c| c.display_name == worker_display)
        else {
            let _ = reply.send(Response::err(format!("no worker named `{worker_display}`")));
            return;
        };

        let target_parent = self.chats.get(target_idx).and_then(|c| c.parent_idx);
        if target_parent.is_none() {
            let _ = reply.send(Response::err(format!("`{worker_display}` is not a worker")));
            return;
        }
        // Only the worker's own parent may archive it (bypassed for
        // supervisor force-archive). Prevents a sibling agent (or the
        // worker itself) from tearing down someone else's worker.
        if !bypass_parent_check && target_parent != Some(sender_idx) {
            let _ = reply.send(Response::err(
                "only the parent that spawned this worker may archive it",
            ));
            return;
        }

        // Snapshot Jules-specific info we need for the remote DELETE
        // before we flip state or hand over the chat. We need both
        // the api-key env var name (to resolve the key fresh — agents
        // may rotate it while aciddog is running) and the backend
        // session identifier the core registered via
        // `RemoteSessionBound` once Jules accepted its create call.
        let jules_delete: Option<(String, String)> = {
            let chat = self.chats.get(target_idx);
            let core_id = chat.and_then(|c| c.ai_core_id.clone());
            let session_name = chat.and_then(|c| c.remote_session_name.clone());
            let kind = core_id.as_ref().and_then(|id| {
                self.config
                    .ai_cores
                    .iter()
                    .find(|c| &c.id == id)
                    .map(|c| c.kind.clone())
            });
            match (kind, session_name) {
                (Some(AiCoreKind::Jules { api_key_env, .. }), Some(name)) => {
                    Some((api_key_env, name))
                }
                // A Jules chat whose session never got registered
                // (create failed, or archive arrived before the
                // create response) has nothing to delete — skip the
                // remote call. Non-Jules cores also skip.
                _ => None,
            }
        };

        // Hibernate locally: drop channels, mark terminal.
        if let Some(chat) = self.chats.get_mut(target_idx) {
            chat.user_tx = None;
            chat.agent_tx = None;
            chat.cancel_tx = None;
            chat.status = AgentStatus::Hibernated;
            chat.hibernated = true;
            chat.remote_terminal = true;
            chat.push_block(ChatBlock::WorkerComplete {
                body: "archived by parent".to_string(),
                is_error: false,
                collapsed: true,
                pending: false,
            });
            chat.end_working();
        }

        // Snap selection off the now-hibernated worker. Prefer the
        // target's parent; fall back to the caller's pane when
        // force-archiving an orphaned worker.
        if self.selected == target_idx {
            self.selected = target_parent.unwrap_or(sender_idx);
        }

        // Best-effort DELETE on the remote side. Spawn so the reply
        // returns quickly — parent shouldn't be blocked on network
        // teardown, and a failure here can't un-archive the local
        // pane anyway.
        if let Some((api_key_env, session_name)) = jules_delete {
            tokio::spawn(async move {
                let Ok(api_key) = std::env::var(&api_key_env) else {
                    tracing::warn!(
                        "worker_archive: {api_key_env} not set; skipping remote DELETE for {session_name}"
                    );
                    return;
                };
                if api_key.is_empty() {
                    return;
                }
                let http = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(15))
                    .build()
                    .unwrap_or_else(|_| reqwest::Client::new());
                if let Err(e) =
                    crate::ai::jules::delete_session(&http, &api_key, &session_name).await
                {
                    tracing::warn!(
                        session = %session_name,
                        error = %e,
                        "worker_archive: remote Jules DELETE failed; session will time out via TTL"
                    );
                }
            });
        }

        let _ = reply.send(Response::ok("archived"));
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
        // Remote-worker cores cannot resume a backend session, and
        // `handle_send_message` already guards against auto-resurrection
        // when `remote_terminal` is set. Hitting this path for a Jules
        // core would be a bug — surface it as a clear error rather
        // than silently creating a stranded core.
        let core: Box<dyn AICore> = build_core(
            ai_core,
            session_id,
            PromptParts::default(),
            tools,
            env,
            socket_tx_for_core,
            true,
            worker.model.as_deref(),
            None,
        )
        .map_err(|e| format!("build_core failed during resurrection: {e}"))?;

        let (user_tx, user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, agent_rx) = mpsc::channel::<String>(256);
        let (cancel_tx, cancel_rx) = mpsc::channel::<()>(1);

        let pool_len = self.config.thinking_messages.len();
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.user_tx = Some(user_tx);
            chat.agent_tx = Some(agent_tx);
            chat.cancel_tx = Some(cancel_tx);
            chat.begin_working(pool_len);
            chat.hibernated = false;
        }

        // Workers never participate in channels; give `run_core` a
        // pre-closed channel_rx (see the spawn path above).
        let (_wkr_ch_tx, wkr_ch_rx) = mpsc::channel::<String>(1);
        drop(_wkr_ch_tx);
        tokio::spawn(run_core(
            idx, core, user_rx, agent_rx, wkr_ch_rx, cancel_rx, ai_tx,
        ));
        Ok(())
    }

    fn handle_send_message(
        &mut self,
        sender_idx: usize,
        to: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        self.handle_send_message_with_origin(MessageOrigin::Agent { sender_idx }, to, body, reply);
    }

    /// IPC entry for `aciddog channel send`. Resolves the sender's
    /// config id from `agent_ids`, validates channel membership, and
    /// defers to the shared broadcaster. See `broadcast_to_channel`.
    fn handle_channel_send_from_agent(
        &mut self,
        sender_idx: usize,
        channel_id: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            let _ = reply.send(Response::err("unknown sender"));
            return;
        };
        // Workers are never channel members — fail fast for them.
        if sender_chat.parent_idx.is_some() {
            let _ = reply.send(Response::err(
                "workers cannot post to channels — only declared agents are members",
            ));
            return;
        }
        let sender_name = sender_chat.display_name.clone();
        self.broadcast_to_channel(ChannelSender::Agent(sender_name), channel_id, body, reply);
    }

    /// Resolve the sender's agent id for a channel-management IPC call.
    /// Rejects workers (workers are never channel members). Returns
    /// the agent's display name on success; sends the error response
    /// and returns `None` on failure.
    fn resolve_channel_caller(
        &self,
        sender_idx: usize,
        reply: &mut Option<tokio::sync::oneshot::Sender<Response>>,
    ) -> Option<String> {
        let Some(sender_chat) = self.chats.get(sender_idx) else {
            if let Some(r) = reply.take() {
                let _ = r.send(Response::err("unknown sender"));
            }
            return None;
        };
        if sender_chat.parent_idx.is_some() {
            if let Some(r) = reply.take() {
                let _ = r.send(Response::err(
                    "workers cannot manage channels — only declared agents can",
                ));
            }
            return None;
        }
        Some(sender_chat.display_name.clone())
    }

    /// IPC entry for `aciddog channel create`. Creates an ephemeral
    /// channel and auto-joins the caller so they can `channel send`
    /// to their own meeting immediately.
    fn handle_channel_create_from_agent(
        &mut self,
        sender_idx: usize,
        channel_id: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let mut reply = Some(reply);
        let Some(caller) = self.resolve_channel_caller(sender_idx, &mut reply) else {
            return;
        };
        let reply = reply.expect("caller resolution didn't consume reply");
        match self.create_meeting(channel_id.clone()) {
            Ok(_new_idx) => {
                // Auto-join the creator. If this fails for any reason
                // (shouldn't — we just created the channel) we still
                // report the create as successful.
                if let Err(e) = self.channel_join_by_id(&channel_id, &caller) {
                    tracing::warn!(
                        channel = %channel_id,
                        agent = %caller,
                        error = %e,
                        "auto-join after channel create failed"
                    );
                }
                let _ = reply.send(Response::ok(format!(
                    "channel `{channel_id}` created (ephemeral)"
                )));
            }
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
            }
        }
    }

    /// IPC entry for `aciddog channel join`. Adds the calling agent
    /// to an existing channel. Works for both ephemeral and
    /// WCL-declared channels (runtime-only — not persisted).
    fn handle_channel_join_from_agent(
        &mut self,
        sender_idx: usize,
        channel_id: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let mut reply = Some(reply);
        let Some(caller) = self.resolve_channel_caller(sender_idx, &mut reply) else {
            return;
        };
        let reply = reply.expect("caller resolution didn't consume reply");
        match self.channel_join_by_id(&channel_id, &caller) {
            Ok(()) => {
                let _ = reply.send(Response::ok(format!("joined channel `{channel_id}`")));
            }
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
            }
        }
    }

    /// IPC entry for `aciddog channel add`. Caller must already be a
    /// member of `channel_id`.
    fn handle_channel_add_from_agent(
        &mut self,
        sender_idx: usize,
        channel_id: String,
        member: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let mut reply = Some(reply);
        let Some(caller) = self.resolve_channel_caller(sender_idx, &mut reply) else {
            return;
        };
        let reply = reply.expect("caller resolution didn't consume reply");
        let Some(channel) = self.config.channel(&channel_id) else {
            let _ = reply.send(Response::err(format!("unknown channel `{channel_id}`")));
            return;
        };
        if !channel.members.iter().any(|m| m == &caller) {
            let _ = reply.send(Response::err(format!(
                "you are not a member of channel `{channel_id}`"
            )));
            return;
        }
        match self.channel_add_member_by_id(&channel_id, &member) {
            Ok(()) => {
                let _ = reply.send(Response::ok(format!(
                    "added `{member}` to channel `{channel_id}`"
                )));
            }
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
            }
        }
    }

    /// IPC entry for `aciddog channel kick`. Caller must already be a
    /// member of `channel_id`. Self-kick is allowed (acts as "leave
    /// without ending").
    fn handle_channel_kick_from_agent(
        &mut self,
        sender_idx: usize,
        channel_id: String,
        member: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let mut reply = Some(reply);
        let Some(caller) = self.resolve_channel_caller(sender_idx, &mut reply) else {
            return;
        };
        let reply = reply.expect("caller resolution didn't consume reply");
        let Some(channel) = self.config.channel(&channel_id) else {
            let _ = reply.send(Response::err(format!("unknown channel `{channel_id}`")));
            return;
        };
        if !channel.members.iter().any(|m| m == &caller) {
            let _ = reply.send(Response::err(format!(
                "you are not a member of channel `{channel_id}`"
            )));
            return;
        }
        match self.channel_kick_member_by_id(&channel_id, &member) {
            Ok(()) => {
                let _ = reply.send(Response::ok(format!(
                    "kicked `{member}` from channel `{channel_id}`"
                )));
            }
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
            }
        }
    }

    /// IPC entry for `aciddog channel end`. Caller must be a member.
    /// Rejects WCL-declared channels.
    fn handle_channel_end_from_agent(
        &mut self,
        sender_idx: usize,
        channel_id: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let mut reply = Some(reply);
        let Some(caller) = self.resolve_channel_caller(sender_idx, &mut reply) else {
            return;
        };
        let reply = reply.expect("caller resolution didn't consume reply");
        let Some(channel) = self.config.channel(&channel_id) else {
            let _ = reply.send(Response::err(format!("unknown channel `{channel_id}`")));
            return;
        };
        if !channel.members.iter().any(|m| m == &caller) {
            let _ = reply.send(Response::err(format!(
                "you are not a member of channel `{channel_id}`"
            )));
            return;
        }
        match self.channel_leave_by_id(&channel_id) {
            Ok(()) => {
                let _ = reply.send(Response::ok(format!("ended channel `{channel_id}`")));
            }
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
            }
        }
    }

    /// Core broadcast path: validate sender membership (for agents),
    /// build the `<ChannelMessage>` payload, append a ChannelMessage
    /// block to the channel's display log (and disk log), and enqueue
    /// a non-blocking send on every other member's `channel_tx`.
    /// Fully implemented after `ChannelState` exists; see task list.
    pub(crate) fn broadcast_to_channel(
        &mut self,
        sender: ChannelSender,
        channel_id: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if self.shutting_down && matches!(sender, ChannelSender::Agent(_)) {
            let _ = reply.send(Response::err("aciddog is shutting down"));
            return;
        }
        // Resolve channel + validate sender.
        let Some(channel) = self.config.channel(&channel_id).cloned() else {
            let _ = reply.send(Response::err(format!("unknown channel `{channel_id}`")));
            return;
        };
        if let ChannelSender::Agent(ref name) = sender {
            if !channel.members.iter().any(|m| m == name) {
                let _ = reply.send(Response::err(format!(
                    "agent `{name}` is not a member of channel `{channel_id}`"
                )));
                return;
            }
        }

        // Leading `@name` tokens narrow the recipient list. Unknown
        // names are rejected so typos surface fast instead of being
        // silently delivered to nobody.
        let mentions = parse_leading_mentions(&body);
        if !mentions.is_empty() {
            for m in &mentions {
                if !channel.members.iter().any(|member| member == m) {
                    let _ = reply.send(Response::err(format!(
                        "`@{m}` is not a member of channel `{channel_id}`"
                    )));
                    return;
                }
            }
        }

        let from_label = sender.label().to_string();
        // Transcript + disk store just the sender/body pair. The
        // per-recipient payload below adds a reminder so models don't
        // fall back to replying in assistant text (which would never
        // reach the channel).
        let payload = format!(
            "<ChannelMessage channel=\"{channel_id}\" from=\"{from_label}\">{body}</ChannelMessage>\n\
             <system-reminder>This is a channel message. To reply you MUST use the `channel send` tool with `id=\"{channel_id}\"` — text in your assistant response is NOT delivered to the channel and every other member will miss it. If you have nothing substantive to add, stay silent this turn.</system-reminder>"
        );

        // Find channel state + append display block + disk line.
        let mut emitted_seq: Option<u64> = None;
        if let Some(ch_state) = self.channels.iter_mut().find(|c| c.id == channel_id) {
            ch_state
                .blocks
                .push(ChatBlock::channel_message(from_label.clone(), body.clone()));
            ch_state.sticky_bottom = true;
            if let Some(log) = ch_state.log.as_mut() {
                if let Err(e) = log.append(&from_label, &body) {
                    tracing::warn!(channel = %channel_id, error = %e, "failed to append channel log");
                }
            }
            // Seq counts visible messages excluding the welcome
            // banner (if any) so web clients can detect gaps on
            // reconnect.
            let seq = ch_state
                .blocks
                .iter()
                .filter(|b| {
                    matches!(
                        b,
                        ChatBlock::ChannelMessage {
                            is_welcome: false,
                            ..
                        }
                    )
                })
                .count()
                .saturating_sub(1) as u64;
            emitted_seq = Some(seq);
        }

        // Fan out to the supervisor SSE broadcaster. `receiver_count`
        // is typically 0 when no portal is attached, so the send is
        // effectively free — but we still skip the struct build in
        // that case.
        if self.channel_message_tx.receiver_count() > 0 {
            let ev = crate::wire_channel::ChannelMessageEvent {
                channel: channel_id.clone(),
                sender: from_label.clone(),
                body: body.clone(),
                ts: chrono::Utc::now().to_rfc3339(),
                seq: emitted_seq.unwrap_or(0),
            };
            let _ = self.channel_message_tx.send(ev);
        }

        // Enqueue payload on every other member's channel_tx. When
        // the message starts with `@mentions` those become the sole
        // recipients (intersected with channel membership, which was
        // validated above); otherwise it broadcasts to everyone.
        let pool_len = self.config.thinking_messages.len();
        let sender_label = match &sender {
            ChannelSender::Agent(name) => Some(name.clone()),
            ChannelSender::User => None,
        };
        for member in &channel.members {
            if sender_label.as_deref() == Some(member.as_str()) {
                continue;
            }
            if !mentions.is_empty() && !mentions.iter().any(|m| m == member) {
                continue;
            }
            let Some(recipient_idx) = self
                .chats
                .iter()
                .position(|c| c.display_name == *member && c.parent_idx.is_none())
            else {
                continue;
            };
            // Asleep members silently skip the broadcast — the
            // sender's response stays `ok` and other members still
            // receive the message normally.
            if self
                .chats
                .get(recipient_idx)
                .map(|c| c.asleep)
                .unwrap_or(false)
            {
                continue;
            }
            let Some(tx) = self
                .chats
                .get(recipient_idx)
                .and_then(|c| c.channel_tx.clone())
            else {
                continue;
            };
            if let Some(chat) = self.chats.get_mut(recipient_idx) {
                if chat.status != AgentStatus::Error {
                    chat.begin_working(pool_len);
                }
            }
            let payload = payload.clone();
            let member_name = member.clone();
            tokio::spawn(async move {
                if tx.send(payload).await.is_err() {
                    tracing::warn!(
                        recipient = %member_name,
                        "channel message dropped: recipient channel closed"
                    );
                }
            });
        }

        let _ = reply.send(Response::ok("broadcast"));
    }

    /// Validate a runtime-supplied channel id. Stricter than the WCL
    /// parser: first char alpha/underscore, remainder alnum/`_`/`-`,
    /// max 64 chars. The tight rules keep the id safe to use as a
    /// filename for `ChannelLog::open`.
    fn validate_meeting_id(id: &str) -> Result<(), String> {
        if id.is_empty() {
            return Err("channel id cannot be empty".into());
        }
        if id.len() > 64 {
            return Err("channel id must be ≤ 64 characters".into());
        }
        let mut chars = id.chars();
        let first = chars.next().expect("non-empty checked above");
        if !(first.is_ascii_alphabetic() || first == '_') {
            return Err("channel id must start with an ASCII letter or underscore".into());
        }
        for ch in chars {
            if !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
                return Err(format!(
                    "channel id contains invalid character `{ch}` (allowed: letters, digits, `_`, `-`)"
                ));
            }
        }
        Ok(())
    }

    /// Create an ephemeral runtime channel. Succeeds if the id is
    /// valid and doesn't collide with an existing channel (WCL or
    /// runtime). Mutates both `self.config.channels` (so the existing
    /// `broadcast_to_channel` path works unchanged) and
    /// `self.channels` (runtime state). Returns the new index into
    /// `self.channels`.
    pub(crate) fn create_meeting(&mut self, id: String) -> Result<usize, String> {
        Self::validate_meeting_id(&id)?;
        if self.config.channel(&id).is_some() {
            return Err(format!("channel `{id}` already exists"));
        }
        self.config.channels.push(crate::config::Channel {
            id: id.clone(),
            description: None,
            welcome: None,
            members: Vec::new(),
        });
        let mut state = ChannelState {
            id: id.clone(),
            is_ephemeral: true,
            sticky_bottom: true,
            ..Default::default()
        };
        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        match super::channel_log::ChannelLog::open(&cwd, &id) {
            Ok(log) => state.log = Some(log),
            Err(e) => {
                warn!(channel = %id, error = %e, "failed to open ephemeral channel log");
            }
        }
        state.blocks.push(ChatBlock::system_info(format!(
            "meeting channel `{id}` created (ephemeral — dies when ended or on exit)"
        )));
        self.channels.push(state);
        Ok(self.channels.len() - 1)
    }

    /// Deliver a one-shot `<system-notice>` to a newly-added channel
    /// member via their `channel_tx`, and flip their UI status to
    /// working. Ignores send failures (queue full, closed, agent not
    /// yet online) — the agent will still discover the channel on
    /// the next inbound broadcast.
    fn notify_agent_added_to_channel(&mut self, agent_name: &str, channel_id: &str) {
        let Some(recipient_idx) = self
            .chats
            .iter()
            .position(|c| c.display_name == *agent_name && c.parent_idx.is_none())
        else {
            return;
        };
        // Automatons receive real channel broadcasts via their own
        // `channel_tx`; they don't benefit from an LLM-style onboarding
        // notice (it'd just spawn a child process with prose about
        // "the `channel send` tool"), so skip the notice for them.
        if self
            .chats
            .get(recipient_idx)
            .map(|c| c.kind == ChatKind::Automaton)
            .unwrap_or(false)
        {
            return;
        }
        if self
            .chats
            .get(recipient_idx)
            .map(|c| c.asleep)
            .unwrap_or(false)
        {
            return;
        }
        let Some(tx) = self
            .chats
            .get(recipient_idx)
            .and_then(|c| c.channel_tx.clone())
        else {
            return;
        };
        let pool_len = self.config.thinking_messages.len();
        if let Some(chat) = self.chats.get_mut(recipient_idx) {
            if chat.status != AgentStatus::Error {
                chat.begin_working(pool_len);
            }
        }
        let payload = format!(
            "<system-notice>You have been added to channel `{channel_id}`. \
             Incoming posts to this channel will arrive wrapped as \
             `<ChannelMessage channel=\"{channel_id}\" from=\"<sender>\">body</ChannelMessage>` \
             — reply with `aciddog channel send --id {channel_id} --body \"<text>\"` \
             (NOT `aciddog send --to …`, which would mail a single agent and \
             skip the rest of the channel). Prefix the body with one or more \
             `@name` tokens to narrow delivery to specific members; with no \
             leading `@` the message broadcasts to everyone in the channel. \
             Stay silent unless you have something substantive to contribute.</system-notice>"
        );
        if let Err(e) = tx.try_send(payload) {
            tracing::warn!(
                recipient = %agent_name,
                channel = %channel_id,
                error = ?e,
                "failed to deliver channel-add notice"
            );
        }
    }

    /// Add `agent` to channel `channel_id`. Validates agent exists
    /// (declared `agent` block, not a worker template) and isn't
    /// already a member. Mutates both `config.channels[c].members`
    /// and `self.channels[idx].members`, delivers a one-shot
    /// `<system-notice>` to the added agent, and pushes a system-info
    /// block to the channel pane.
    pub(crate) fn channel_add_member_by_id(
        &mut self,
        channel_id: &str,
        agent: &str,
    ) -> Result<(), String> {
        if !self.config.is_addressable(agent) {
            return Err(format!(
                "`{agent}` is not a declared agent or automaton (workers cannot be channel members)"
            ));
        }
        let Some(cfg_idx) = self.config.channels.iter().position(|c| c.id == channel_id) else {
            return Err(format!("unknown channel `{channel_id}`"));
        };
        if self.config.channels[cfg_idx]
            .members
            .iter()
            .any(|m| m == agent)
        {
            return Err(format!(
                "`{agent}` is already a member of channel `{channel_id}`"
            ));
        }
        self.config.channels[cfg_idx]
            .members
            .push(agent.to_string());
        if let Some(state_idx) = self.channels.iter().position(|c| c.id == channel_id) {
            self.channels[state_idx].members.push(agent.to_string());
            self.channels[state_idx]
                .blocks
                .push(ChatBlock::system_info(format!("added `{agent}`")));
            self.channels[state_idx].sticky_bottom = true;
        }
        self.notify_agent_added_to_channel(agent, channel_id);
        Ok(())
    }

    /// Remove `agent` from channel `channel_id`. Mutates both
    /// membership mirrors and pushes a system-info block to the
    /// channel pane. The removed agent gets no IPC notice — their
    /// next `channel send` attempt will be rejected at the existing
    /// membership check, which is a sufficient discovery path.
    pub(crate) fn channel_kick_member_by_id(
        &mut self,
        channel_id: &str,
        agent: &str,
    ) -> Result<(), String> {
        let Some(cfg_idx) = self.config.channels.iter().position(|c| c.id == channel_id) else {
            return Err(format!("unknown channel `{channel_id}`"));
        };
        let Some(pos) = self.config.channels[cfg_idx]
            .members
            .iter()
            .position(|m| m == agent)
        else {
            return Err(format!(
                "`{agent}` is not a member of channel `{channel_id}`"
            ));
        };
        self.config.channels[cfg_idx].members.remove(pos);
        if let Some(state_idx) = self.channels.iter().position(|c| c.id == channel_id) {
            if let Some(mirror_pos) = self.channels[state_idx]
                .members
                .iter()
                .position(|m| m == agent)
            {
                self.channels[state_idx].members.remove(mirror_pos);
            }
            self.channels[state_idx]
                .blocks
                .push(ChatBlock::system_info(format!("kicked `{agent}`")));
            self.channels[state_idx].sticky_bottom = true;
        }
        Ok(())
    }

    /// Add the calling agent to channel `channel_id` (the agent-side
    /// "join" path). Works for ephemeral AND WCL-declared channels —
    /// runtime-only membership that restarts will restore to WCL
    /// state. No inbound system-notice (the agent just asked).
    pub(crate) fn channel_join_by_id(
        &mut self,
        channel_id: &str,
        agent: &str,
    ) -> Result<(), String> {
        if !self.config.is_addressable(agent) {
            return Err(format!(
                "`{agent}` is not a declared agent or automaton (workers cannot be channel members)"
            ));
        }
        let Some(cfg_idx) = self.config.channels.iter().position(|c| c.id == channel_id) else {
            return Err(format!("unknown channel `{channel_id}`"));
        };
        if self.config.channels[cfg_idx]
            .members
            .iter()
            .any(|m| m == agent)
        {
            return Err(format!(
                "`{agent}` is already a member of channel `{channel_id}`"
            ));
        }
        self.config.channels[cfg_idx]
            .members
            .push(agent.to_string());
        if let Some(state_idx) = self.channels.iter().position(|c| c.id == channel_id) {
            self.channels[state_idx].members.push(agent.to_string());
            self.channels[state_idx]
                .blocks
                .push(ChatBlock::system_info(format!("`{agent}` joined")));
            self.channels[state_idx].sticky_bottom = true;
        }
        Ok(())
    }

    /// Destroy an ephemeral channel. Rejects WCL-declared channels.
    /// Removes from both `config.channels` and `self.channels`,
    /// clamps `selected_channel`, and switches `active_view` to a
    /// safe fallback when the ended channel was being viewed.
    pub(crate) fn channel_leave_by_id(&mut self, channel_id: &str) -> Result<(), String> {
        let Some(state_idx) = self.channels.iter().position(|c| c.id == channel_id) else {
            return Err(format!("unknown channel `{channel_id}`"));
        };
        if !self.channels[state_idx].is_ephemeral {
            return Err(format!(
                "channel `{channel_id}` is declared in the WCL config and cannot be ended"
            ));
        }
        // Drop the log handle before removal so the file closes.
        self.channels[state_idx].log = None;
        self.channels.remove(state_idx);
        if let Some(cfg_idx) = self.config.channels.iter().position(|c| c.id == channel_id) {
            self.config.channels.remove(cfg_idx);
        }
        // Clamp selection + fix active view.
        if self.channels.is_empty() {
            self.selected_channel = 0;
            if matches!(self.active_view, ActiveView::Channel(_)) {
                self.active_view = ActiveView::Chat;
            }
        } else {
            if self.selected_channel >= self.channels.len() {
                self.selected_channel = self.channels.len() - 1;
            }
            if matches!(self.active_view, ActiveView::Channel(_)) {
                self.active_view = ActiveView::Channel(self.selected_channel);
            }
        }
        Ok(())
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
        if self.shutting_down {
            let _ = reply.send(Response::err("aciddog is shutting down"));
            return;
        }
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
            MessageOrigin::Cron { job_id, created_by } => {
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

        // Reserved `user` recipient: route to the operator's inbox
        // instead of resolving a chat. ACLs are short-circuited in
        // `can_send`, but we still gate by sender identity: workers
        // may only message their parent, so they can't mail the user.
        if to == crate::config::USER_INBOX_ID {
            if sender_parent_idx.is_some() {
                let _ = reply.send(Response::err(
                    "worker can only message its parent".to_string(),
                ));
                return;
            }
            self.push_user_inbox(from_label.clone(), body);
            let _ = reply.send(Response::ok("queued"));
            return;
        }

        let Some(recipient_idx) = self.chats.iter().position(|c| c.display_name == to) else {
            let _ = reply.send(Response::err(format!("no agent named {to}")));
            return;
        };

        match &origin {
            MessageOrigin::Agent { sender_idx } => {
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
                    // recipient is a worker this agent spawned, or the
                    // sender is a supervisor (admin ACL bypass).
                    let recipient_is_worker = self
                        .chats
                        .get(recipient_idx)
                        .map(|c| c.parent_idx.is_some())
                        .unwrap_or(false);
                    let sender_is_supervisor = self.is_supervisor(*sender_idx);
                    if !recipient_is_worker
                        && !sender_is_supervisor
                        && !self.config.can_send(&sender_id, &to)
                    {
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
                // config changes take effect. Supervisors bypass.
                let creator_is_supervisor = self
                    .config
                    .agents
                    .iter()
                    .find(|a| a.id == sender_id)
                    .map(|a| a.supervisor)
                    .unwrap_or(false);
                if !creator_is_supervisor && !self.config.can_send(&sender_id, &to) {
                    let _ = reply.send(Response::err(format!(
                        "cron creator `{sender_id}` is not permitted to message `{to}`"
                    )));
                    return;
                }
            }
        }

        // Asleep gate must precede the auto-resurrect path: an asleep
        // hibernated worker should NOT be resurrected just because
        // somebody messaged it. The sender gets a clear "asleep"
        // error and the recipient stays paused until /awake.
        if self
            .chats
            .get(recipient_idx)
            .map(|c| c.asleep)
            .unwrap_or(false)
        {
            let _ = reply.send(Response::err(format!("agent `{to}` is asleep")));
            return;
        }

        // Remote-worker sessions (Jules) cannot be resumed once the
        // backend session has terminated or been archived — the
        // session id in our core is already DELETEd upstream. Reject
        // with a clear error before we attempt resurrection.
        let recipient_remote_terminal = self
            .chats
            .get(recipient_idx)
            .map(|c| c.remote_terminal)
            .unwrap_or(false);
        if recipient_remote_terminal {
            let _ = reply.send(Response::err(format!(
                "remote worker `{to}` has ended; spawn a new worker instead"
            )));
            return;
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

        let kind = if let Some(ref name) = assignee {
            match self.classify_assignee(name) {
                Some(k) => Some(k),
                None => {
                    let _ = reply.send(Response::err(format!(
                        "no agent / worker / group / user named `{name}`"
                    )));
                    return;
                }
            }
        } else {
            None
        };

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

        if let (Some(new_assignee), Some(kind)) = (assignee_clone, kind) {
            self.dispatch_assignment_notice(kind, &new_assignee, task_id, &title_clone, &who);
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

    /// Supervisor: structured variant of `handle_task_list`. Same
    /// filter semantics, but returns a `Vec<Task>` via
    /// `Response::data` so the manager portal's Kanban view can work
    /// with nested todos / comments / notes / blocked_by without
    /// parsing plaintext.
    fn handle_admin_task_list_json(
        &mut self,
        sender_idx: usize,
        assignee: Option<String>,
        status: Option<String>,
        mine: bool,
        unassigned: bool,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let who = match self.chats.get(sender_idx) {
            Some(c) => c.display_name.clone(),
            None => {
                let _ = reply.send(Response::err("unknown sender"));
                return;
            }
        };
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
        let rows: Vec<&Task> = self.tasks.list(&filter);
        match serde_json::to_value(&rows) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize tasks: {e}")));
            }
        }
    }

    /// Supervisor: list every declared channel with member ids and
    /// in-memory message count. Channels defined in the WCL config
    /// appear even if no live `ChannelState` exists yet (for runtime-
    /// created channels the state is the authority).
    fn handle_admin_channel_list(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        // Walk declared channels first (preserving config order); then
        // append any ephemeral runtime channels that aren't in the
        // config yet.
        let mut infos: Vec<crate::wire_channel::WireChannelInfo> = Vec::new();
        for cfg in &self.config.channels {
            let state = self.channels.iter().find(|c| c.id == cfg.id);
            let message_count = state
                .map(|s| {
                    s.blocks
                        .iter()
                        .filter(|b| {
                            matches!(
                                b,
                                ChatBlock::ChannelMessage {
                                    is_welcome: false,
                                    ..
                                }
                            )
                        })
                        .count()
                })
                .unwrap_or(0);
            infos.push(crate::wire_channel::WireChannelInfo {
                id: cfg.id.clone(),
                description: cfg.description.clone(),
                welcome: cfg.welcome.clone(),
                members: cfg.members.clone(),
                message_count,
            });
        }
        for state in &self.channels {
            if self.config.channels.iter().any(|c| c.id == state.id) {
                continue;
            }
            let message_count = state
                .blocks
                .iter()
                .filter(|b| {
                    matches!(
                        b,
                        ChatBlock::ChannelMessage {
                            is_welcome: false,
                            ..
                        }
                    )
                })
                .count();
            infos.push(crate::wire_channel::WireChannelInfo {
                id: state.id.clone(),
                description: None,
                welcome: None,
                members: state.members.clone(),
                message_count,
            });
        }
        match serde_json::to_value(&infos) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize channel list: {e}")));
            }
        }
    }

    /// Supervisor: transcript of a single channel in oldest-first
    /// order. Welcome banner (if any) appears first with `is_welcome =
    /// true`.
    fn handle_admin_channel_history(
        &mut self,
        sender_idx: usize,
        channel: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(state) = self.channels.iter().find(|c| c.id == channel) else {
            let _ = reply.send(Response::err(format!("unknown channel `{channel}`")));
            return;
        };
        let mut out: Vec<crate::wire_channel::WireChannelMessage> = Vec::new();
        for block in &state.blocks {
            if let ChatBlock::ChannelMessage {
                sender,
                body,
                is_welcome,
            } = block
            {
                out.push(crate::wire_channel::WireChannelMessage {
                    sender: sender.clone(),
                    body: body.clone(),
                    is_welcome: *is_welcome,
                });
            }
        }
        match serde_json::to_value(&out) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize channel history: {e}")));
            }
        }
    }

    /// Supervisor: post into `channel` as if the human operator typed
    /// it into the TUI's channel input box. Delegates to the same
    /// `broadcast_to_channel` path with `ChannelSender::User` so
    /// membership validation, `@mention` routing, disk logging, and
    /// per-recipient delivery all behave identically to the TUI.
    fn handle_admin_channel_send_as_user(
        &mut self,
        sender_idx: usize,
        channel: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let body = body.trim().to_string();
        if body.is_empty() {
            let _ = reply.send(Response::err("body is empty"));
            return;
        }
        self.broadcast_to_channel(ChannelSender::User, channel, body, reply);
    }

    /// Supervisor: snapshot the running team + AI cores config.
    /// Returns a [`crate::wire_team::WireTeamInfo`] in `Response::data`.
    fn handle_admin_team_info(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let info = crate::wire_team::WireTeamInfo::project(&self.config);
        match serde_json::to_value(&info) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize team info: {e}")));
            }
        }
    }

    /// Supervisor: snapshot the human-operator inbox. Returns a
    /// `Vec<WireInboxMessage>` in `Response::data`.
    fn handle_admin_inbox_list(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let now = std::time::Instant::now();
        let wire: Vec<crate::wire_inbox::WireInboxMessage> = self
            .inbox
            .iter()
            .map(|m| crate::wire_inbox::WireInboxMessage::project(m, now))
            .collect();
        match serde_json::to_value(&wire) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize inbox: {e}")));
            }
        }
    }

    /// Supervisor: delete one inbox entry by list index. Mirrors the
    /// TUI's `d` key — purely local, no wire event is emitted.
    fn handle_admin_inbox_delete(
        &mut self,
        sender_idx: usize,
        index: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        if index >= self.inbox.len() {
            let _ = reply.send(Response::err(format!(
                "inbox index {index} out of range (len={})",
                self.inbox.len()
            )));
            return;
        }
        self.inbox.remove(index);
        self.inbox_screen.reclamp(self.inbox.len());
        let _ = reply.send(Response::ok("deleted"));
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
            if self.classify_assignee(&name).is_none() {
                let _ = reply.send(Response::err(format!(
                    "no agent / worker / group / user named `{name}`"
                )));
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

        if let Some(new_assignee) = outcome.reassigned_to.clone() {
            if let Some(kind) = self.classify_assignee(&new_assignee) {
                let title_clone = self.tasks.get(id).map(|t| t.title.clone());
                if let Some(title_clone) = title_clone {
                    self.dispatch_assignment_notice(
                        kind,
                        &new_assignee,
                        id,
                        &title_clone,
                        &who,
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

        // Supervisors bypass their own can_message ACL at cron-create
        // time, matching the bypass on direct `send`.
        let sender_is_supervisor = self.is_supervisor(sender_idx);
        if !sender_is_supervisor && !self.config.can_send(&sender_id, &target) {
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
                let wire = crate::cron::WireCronJob::from(j);
                self.cron_invalidate.notify_one();
                let _ = self
                    .cron_event_tx
                    .send(crate::cron::CronEvent::Created { job: wire });
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

    fn handle_cron_list_json(
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
        let jobs: Vec<crate::cron::WireCronJob> = self
            .cron
            .list(&filter)
            .into_iter()
            .map(crate::cron::WireCronJob::from)
            .collect();
        match serde_json::to_value(&jobs) {
            Ok(value) => {
                let _ = reply.send(Response::ok_data(value));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("failed to serialize cron list: {e}")));
            }
        }
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
            let sender_is_supervisor = self.is_supervisor(sender_idx);
            if !sender_is_supervisor && !self.config.can_send(&sender_id, t) {
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
            Ok(updated) => {
                let wire = crate::cron::WireCronJob::from(updated);
                self.cron_invalidate.notify_one();
                let _ = self
                    .cron_event_tx
                    .send(crate::cron::CronEvent::Updated { job: wire });
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
                let _ = self
                    .cron_event_tx
                    .send(crate::cron::CronEvent::Removed { id });
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
        // Freeze cron as soon as shutdown begins. Any tick firing
        // mid-drain would spawn new message/slash work, which
        // defeats the point of waiting for agents to go idle.
        if self.shutting_down {
            return;
        }
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
                        if self.cron.remove(job.id).is_ok() {
                            let _ = self
                                .cron_event_tx
                                .send(crate::cron::CronEvent::Removed { id: job.id });
                        }
                    }
                    crate::cron::CronSchedule::Recurring { .. } => {
                        if self.cron.disable(job.id).is_ok() {
                            if let Some(j) = self.cron.get(job.id) {
                                let _ = self.cron_event_tx.send(crate::cron::CronEvent::Updated {
                                    job: crate::cron::WireCronJob::from(j),
                                });
                            }
                        }
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

            match self.cron.mark_fired(job.id, now) {
                Ok(post_fire) => {
                    let _ = self.cron_event_tx.send(crate::cron::CronEvent::Fired {
                        id: job.id,
                        at: now,
                    });
                    // Recurring: the surviving job carries a fresh
                    // `next_run` and `last_run`. One-off: already gone.
                    match post_fire {
                        Some(updated) => {
                            let wire = crate::cron::WireCronJob::from(&updated);
                            let _ = self
                                .cron_event_tx
                                .send(crate::cron::CronEvent::Updated { job: wire });
                        }
                        None => {
                            let _ = self
                                .cron_event_tx
                                .send(crate::cron::CronEvent::Removed { id: job.id });
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(job_id = %job.id, error = %e, "cron mark_fired failed");
                }
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
    /// checks. Configured agents and automatons use their declared id
    /// (same as their chat `display_name`); workers use the spawn
    /// template id (the `memory_banks` list lives on the template,
    /// not the running instance).
    fn sender_template_id(&self, sender_idx: usize) -> Option<String> {
        let chat = self.chats.get(sender_idx)?;
        if let Some(tpl) = &chat.worker_template {
            return Some(tpl.clone());
        }
        Some(chat.display_name.clone())
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

    /// Supervisor: list declared memory banks with entry counts.
    /// Returns `Vec<WireBankInfo>` in `Response::data`. Entry counts
    /// come from a cheap `search(None, large_limit)` per bank; for
    /// the demo scale this is well under 10ms.
    fn handle_admin_memory_banks(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(registry) = self.memory.clone() else {
            let _ = reply.send(Response::err(
                "memory registry is not initialised on this orchestrator",
            ));
            return;
        };
        let banks = self.config.memory_banks.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let mut out: Vec<crate::wire_memory::WireBankInfo> = Vec::new();
                for bank in &banks {
                    let opts = crate::memory::SearchOptions {
                        query: None,
                        title_contains: None,
                        tags: crate::memory::TagFilter::default(),
                        // Cozo's `:limit` can't accept usize::MAX-sized
                        // integers — 10k is well beyond any demo/teamscale
                        // bank and safely fits an i64.
                        limit: 10_000,
                    };
                    let count = registry
                        .search(bank, opts)
                        .map(|hits| hits.len())
                        .unwrap_or(0);
                    out.push(crate::wire_memory::WireBankInfo {
                        id: bank.id.clone(),
                        description: bank.description.clone(),
                        embed_model: bank.embed_model.clone(),
                        entry_count: count,
                    });
                }
                out
            })
            .await;
            match result {
                Ok(out) => match serde_json::to_value(&out) {
                    Ok(v) => {
                        let _ = reply.send(Response::ok_data(v));
                    }
                    Err(e) => {
                        let _ = reply.send(Response::err(format!("serialize memory banks: {e}")));
                    }
                },
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory banks task: {join_err}")));
                }
            }
        });
    }

    /// Supervisor structured variant of `handle_memory_search`.
    /// Returns `Vec<WireHit>` via `Response::data`.
    #[allow(clippy::too_many_arguments)]
    fn handle_admin_memory_search_json(
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
            limit: limit.unwrap_or(50).max(1),
        };
        tokio::spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || registry.search(&bank_decl, opts)).await;
            match result {
                Ok(Ok(hits)) => {
                    let wire: Vec<crate::wire_memory::WireHit> =
                        hits.iter().map(Into::into).collect();
                    match serde_json::to_value(&wire) {
                        Ok(v) => {
                            let _ = reply.send(Response::ok_data(v));
                        }
                        Err(e) => {
                            let _ = reply.send(Response::err(format!("serialize hits: {e}")));
                        }
                    }
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

    /// Structured `MemoryStore` — returns `{"id": <uuid>}` in data.
    fn handle_admin_memory_store_json(
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
                    let _ = reply.send(Response::ok_data(serde_json::json!({ "id": id })));
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

    /// Structured `MemoryLinks` — returns `Vec<WireLink>` in data.
    fn handle_admin_memory_links_json(
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
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                registry.links(&bank_decl, id, dir, &tag_filter)
            })
            .await;
            match result {
                Ok(Ok(links)) => {
                    let wire: Vec<crate::wire_memory::WireLink> =
                        links.iter().map(Into::into).collect();
                    match serde_json::to_value(&wire) {
                        Ok(v) => {
                            let _ = reply.send(Response::ok_data(v));
                        }
                        Err(e) => {
                            let _ = reply.send(Response::err(format!("serialize links: {e}")));
                        }
                    }
                }
                Ok(Err(e)) => {
                    let _ = reply.send(Response::err(e.to_string()));
                }
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory links task: {join_err}")));
                }
            }
        });
    }

    /// Supervisor full-bank graph: nodes + edges. Does one scan for
    /// nodes (relational list) and walks outgoing links per node —
    /// O(N) queries per call. Acceptable for demo-scale banks; a
    /// proper bulk query can come later.
    fn handle_admin_memory_graph(
        &mut self,
        sender_idx: usize,
        bank: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        let (registry, bank_decl) = match self.resolve_memory(sender_idx, &bank) {
            Ok(pair) => pair,
            Err(msg) => {
                let _ = reply.send(Response::err(msg));
                return;
            }
        };
        let bank_id = bank.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let opts = crate::memory::SearchOptions {
                    query: None,
                    title_contains: None,
                    tags: crate::memory::TagFilter::default(),
                    // Cozo's `:limit` can't accept usize::MAX-sized
                    // integers — 10k is well beyond any demo/teamscale
                    // bank and safely fits an i64.
                    limit: 10_000,
                };
                let hits = registry.search(&bank_decl, opts)?;
                let mut nodes: Vec<crate::wire_memory::WireGraphNode> =
                    Vec::with_capacity(hits.len());
                let mut edges: Vec<crate::wire_memory::WireGraphEdge> = Vec::new();
                let empty_filter = crate::memory::TagFilter::default();
                for hit in &hits {
                    nodes.push(crate::wire_memory::WireGraphNode {
                        id: hit.id,
                        title: hit.title.clone(),
                        tags: hit.tags.clone(),
                        created_at: hit.created_at,
                    });
                    // Outgoing-only to avoid double-counting edges.
                    let outgoing = registry.links(
                        &bank_decl,
                        hit.id,
                        crate::memory::LinkDirection::Out,
                        &empty_filter,
                    )?;
                    for link in outgoing {
                        edges.push(crate::wire_memory::WireGraphEdge {
                            from: hit.id,
                            to: link.peer_id,
                            kind: link.kind,
                            created_at: link.created_at,
                        });
                    }
                }
                Ok::<_, crate::memory::MemoryError>(crate::wire_memory::WireGraph {
                    bank: bank_id,
                    nodes,
                    edges,
                })
            })
            .await;
            match result {
                Ok(Ok(graph)) => match serde_json::to_value(&graph) {
                    Ok(v) => {
                        let _ = reply.send(Response::ok_data(v));
                    }
                    Err(e) => {
                        let _ = reply.send(Response::err(format!("serialize graph: {e}")));
                    }
                },
                Ok(Err(e)) => {
                    let _ = reply.send(Response::err(e.to_string()));
                }
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory graph task: {join_err}")));
                }
            }
        });
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
        // Supervisors (agents or webservice automatons flagged
        // `supervisor = true`) bypass the per-agent `memory_banks`
        // allowlist. Typical use: the manager web portal's gateway
        // automaton reads/writes every bank on behalf of the human
        // operator.
        if !self.is_supervisor(sender_idx) && !self.config.can_access_bank(&template_id, bank) {
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

    fn handle_memory_update(
        &mut self,
        sender_idx: usize,
        bank: String,
        id: Uuid,
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
                registry.update_entry(&bank_decl, id, &body, title.as_deref(), &tags)
            })
            .await;
            match result {
                Ok(Ok(())) => {
                    let _ = reply.send(Response::ok(format!("updated {}", short_uuid(id))));
                }
                Ok(Err(e)) => {
                    let _ = reply.send(Response::err(e.to_string()));
                }
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory update task: {join_err}")));
                }
            }
        });
    }

    fn handle_memory_remove(
        &mut self,
        sender_idx: usize,
        bank: String,
        id: Uuid,
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
            let result =
                tokio::task::spawn_blocking(move || registry.delete_entry(&bank_decl, id)).await;
            match result {
                Ok(Ok(true)) => {
                    let _ = reply.send(Response::ok(format!("removed {}", short_uuid(id))));
                }
                Ok(Ok(false)) => {
                    let _ = reply.send(Response::ok(format!(
                        "no entry {} (nothing to remove)",
                        short_uuid(id)
                    )));
                }
                Ok(Err(e)) => {
                    let _ = reply.send(Response::err(e.to_string()));
                }
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory remove task: {join_err}")));
                }
            }
        });
    }

    fn handle_memory_show(
        &mut self,
        sender_idx: usize,
        bank: String,
        id: Uuid,
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
            let result =
                tokio::task::spawn_blocking(move || registry.get_entry(&bank_decl, id)).await;
            match result {
                Ok(Ok(Some(hit))) => {
                    let _ = reply.send(Response::ok(format_memory_entry(&hit)));
                }
                Ok(Ok(None)) => {
                    let _ = reply.send(Response::err(format!("no entry {id}")));
                }
                Ok(Err(e)) => {
                    let _ = reply.send(Response::err(e.to_string()));
                }
                Err(join_err) => {
                    let _ = reply.send(Response::err(format!("memory show task: {join_err}")));
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

    /// Resolve `name` against the live config. Order: chat display_name
    /// (agents / automatons / spawned workers) → declared group label →
    /// reserved `user` id. `None` ⇒ unknown assignee.
    pub(crate) fn classify_assignee(&self, name: &str) -> Option<AssigneeKind> {
        if name == crate::config::USER_INBOX_ID {
            return Some(AssigneeKind::User);
        }
        if self.chats.iter().any(|c| c.display_name == name) {
            return Some(AssigneeKind::Chat);
        }
        if self.config.is_group(name) {
            return Some(AssigneeKind::Group);
        }
        None
    }

    /// Append a message to the operator's inbox and reclamp the cursor.
    /// Single source of truth for inbox writes — used by the
    /// `to == USER_INBOX_ID` send-message branch and by user-assigned
    /// task notifications.
    pub(crate) fn push_user_inbox(&mut self, from: String, body: String) {
        self.inbox
            .push(super::inbox_view::InboxMessage::new(from, body));
        self.inbox_screen.reclamp(self.inbox.len());
    }

    /// Dispatch the assignment notification for a task that just landed
    /// on `assignee` (already validated via `classify_assignee`). Chat
    /// assignment writes a `system_info` block into the assignee's pane
    /// (skipped for self-assignment); group assignment notifies the
    /// group's leader; user assignment pushes onto the operator inbox.
    fn dispatch_assignment_notice(
        &mut self,
        kind: AssigneeKind,
        assignee: &str,
        task_id: Uuid,
        title: &str,
        who: &str,
    ) {
        match kind {
            AssigneeKind::Chat => {
                if assignee != who {
                    self.notify_assignee(
                        assignee,
                        task_id,
                        title,
                        &format!("assigned to you by {who}"),
                    );
                }
            }
            AssigneeKind::Group => {
                let leader = self.config.group_leader(assignee).map(|s| s.to_string());
                if let Some(leader) = leader {
                    if leader != who {
                        self.notify_assignee(
                            &leader,
                            task_id,
                            title,
                            &format!("assigned to group {assignee} by {who}"),
                        );
                    }
                } else {
                    tracing::warn!(
                        group = %assignee,
                        %task_id,
                        "task assigned to group with no leader; no notification dispatched"
                    );
                }
            }
            AssigneeKind::User => {
                let short = task_id.to_string();
                let short = short.get(..8).unwrap_or(&short);
                let body = format!("Task {short} assigned to you by {who}: {title}");
                self.push_user_inbox(who.to_string(), body);
            }
        }
    }

    /// Flip the `shutting_down` latch, sleep every currently-idle
    /// agent, and surface a status line in the announcing chat.
    /// In-flight (`Working`) agents are left alone — they'll be slept
    /// by `check_shutdown_progress` as their turns complete.
    /// `announce_idx` is the chat that should show the "shutdown
    /// initiated" system line; `None` suppresses the message (used by
    /// channel-context dispatch where there's no obvious chat to
    /// annotate). Calling this while already shutting down is a
    /// no-op beyond an "already shutting down" note.
    pub(crate) fn begin_shutdown(&mut self, announce_idx: Option<usize>) {
        if self.shutting_down {
            if let Some(idx) = announce_idx {
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info("aciddog is already shutting down"));
                }
            }
            return;
        }
        info!("shutdown requested via /shutdown");
        self.shutting_down = true;
        for chat in self.chats.iter_mut() {
            if !chat.asleep && chat.status != AgentStatus::Working {
                chat.asleep = true;
                if chat.status != AgentStatus::Hibernated {
                    chat.status = AgentStatus::Asleep;
                }
            }
        }
        let busy = self
            .chats
            .iter()
            .filter(|c| c.status == AgentStatus::Working)
            .count();
        let msg = if busy == 0 {
            "shutting down…".to_string()
        } else {
            format!("shutting down — waiting for {busy} agent(s) to finish their turn")
        };
        if let Some(idx) = announce_idx {
            if let Some(chat) = self.chats.get_mut(idx) {
                chat.push_block(ChatBlock::system_info(msg));
            }
        }
    }

    /// Drive the graceful-shutdown state machine forward. Called once
    /// per `event_loop` iteration. For every agent that is no longer
    /// `Working` but not yet marked asleep, flip it to `Asleep`.
    /// Once no agent is `Working`, raise `should_quit` so the loop
    /// breaks on the next check.
    pub(crate) fn check_shutdown_progress(&mut self) {
        if !self.shutting_down {
            return;
        }
        for chat in self.chats.iter_mut() {
            if !chat.asleep && chat.status != AgentStatus::Working {
                chat.asleep = true;
                if chat.status != AgentStatus::Hibernated {
                    chat.status = AgentStatus::Asleep;
                }
            }
        }
        let any_working = self.chats.iter().any(|c| c.status == AgentStatus::Working);
        if !any_working {
            self.should_quit = true;
        }
    }

    /// Signal the target agent's `run_core` to abort the in-flight
    /// turn (if any). The cancel channel has capacity 1, so a stale
    /// signal queued before the next turn starts is consumed as a
    /// no-op. Pushes `reason` as a system info block so the user sees
    /// what happened. Hibernated workers have no `cancel_tx`; this is
    /// a no-op for them aside from the message.
    fn stop_turn(&mut self, idx: usize, reason: &str) {
        let cancel = self
            .chats
            .get(idx)
            .and_then(|c| c.cancel_tx.as_ref().cloned());
        if let Some(tx) = cancel {
            let _ = tx.try_send(());
        }
        if let Some(chat) = self.chats.get_mut(idx) {
            chat.push_block(ChatBlock::system_info(reason.to_string()));
        }
    }

    // ── supervisor / sysadmin admin surface ─────────────────────────

    /// Is the caller at `idx` an agent flagged `supervisor = true`?
    /// Workers, automatons, and plain agents return false.
    pub(crate) fn is_supervisor(&self, idx: usize) -> bool {
        let Some(chat) = self.chats.get(idx) else {
            return false;
        };
        if chat.parent_idx.is_some() {
            return false;
        }
        match chat.kind {
            ChatKind::Agent => self
                .config
                .agents
                .iter()
                .find(|a| a.id == chat.display_name)
                .map(|a| a.supervisor)
                .unwrap_or(false),
            ChatKind::Automaton => self
                .config
                .automatons
                .iter()
                .find(|a| a.id == chat.display_name)
                .map(|a| a.supervisor)
                .unwrap_or(false),
            ChatKind::Worker => false,
        }
    }

    /// The `group` field on the chat at `idx`. Returns the configured
    /// label for top-level Agents and Automatons; returns the visible
    /// parent's group for workers (so the renderer can label sections
    /// even though workers always render nested under their parent).
    /// `None` ⇒ the chat falls under the `(ungrouped)` section header.
    pub(crate) fn chat_group(&self, idx: usize) -> Option<&str> {
        let chat = self.chats.get(idx)?;
        match chat.kind {
            ChatKind::Agent => self
                .config
                .agents
                .iter()
                .find(|a| a.id == chat.display_name)
                .and_then(|a| a.group.as_deref()),
            ChatKind::Automaton => self
                .config
                .automatons
                .iter()
                .find(|a| a.id == chat.display_name)
                .and_then(|a| a.group.as_deref()),
            ChatKind::Worker => {
                // Workers don't get their own bucket — they nest under
                // their parent. Walk up to the nearest non-worker
                // ancestor and use its group, so renderers calling
                // `chat_group` on a worker get a sensible label rather
                // than the worker template's own (editor-only) `group`.
                let mut p = chat.parent_idx;
                while let Some(pi) = p {
                    let parent = self.chats.get(pi)?;
                    if parent.kind != ChatKind::Worker {
                        return self.chat_group(pi);
                    }
                    p = parent.parent_idx;
                }
                None
            }
        }
    }

    /// Is the caller at `idx` a top-level Agent or Automaton flagged
    /// `leader = true`? Workers, nested chats (parent_idx set), and
    /// agents/automatons whose config block has `leader = false`
    /// return false.
    pub(crate) fn is_leader(&self, idx: usize) -> bool {
        let Some(chat) = self.chats.get(idx) else {
            return false;
        };
        if chat.parent_idx.is_some() {
            return false;
        }
        match chat.kind {
            ChatKind::Agent => self
                .config
                .agents
                .iter()
                .find(|a| a.id == chat.display_name)
                .map(|a| a.leader)
                .unwrap_or(false),
            ChatKind::Automaton => self
                .config
                .automatons
                .iter()
                .find(|a| a.id == chat.display_name)
                .map(|a| a.leader)
                .unwrap_or(false),
            ChatKind::Worker => false,
        }
    }

    /// Is the caller at `idx` flagged `sysadmin = true`? Both
    /// configured agents and configured automatons may be sysadmins
    /// (mirroring the supervisor rule); workers cannot be.
    pub(crate) fn is_sysadmin(&self, idx: usize) -> bool {
        let Some(chat) = self.chats.get(idx) else {
            return false;
        };
        if chat.parent_idx.is_some() {
            return false;
        }
        match chat.kind {
            ChatKind::Agent => self
                .config
                .agents
                .iter()
                .find(|a| a.id == chat.display_name)
                .map(|a| a.sysadmin)
                .unwrap_or(false),
            ChatKind::Automaton => self
                .config
                .automatons
                .iter()
                .find(|a| a.id == chat.display_name)
                .map(|a| a.sysadmin)
                .unwrap_or(false),
            ChatKind::Worker => false,
        }
    }

    fn status_label(chat: &AgentChat) -> &'static str {
        if chat.asleep {
            return "asleep";
        }
        match chat.status {
            AgentStatus::Ready => "ready",
            AgentStatus::Working => "working",
            AgentStatus::Error => "error",
            AgentStatus::Hibernated => "hibernated",
            AgentStatus::Asleep => "asleep",
        }
    }

    fn kind_label(chat: &AgentChat) -> &'static str {
        match chat.kind {
            ChatKind::Agent => "agent",
            ChatKind::Worker => "worker",
            ChatKind::Automaton => "automaton",
        }
    }

    fn handle_admin_agent_list(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        // Sort iteration so leader-flagged top-level Agent / Automaton
        // chats come first (preserving declaration order within ties).
        // The kind column already encodes Agent vs Automaton, so the
        // crown-rendering surfaces (TUI + manager) get a single
        // `leader` bool per row regardless of kind.
        let mut indices: Vec<usize> = (0..self.chats.len()).collect();
        indices.sort_by_key(|&i| if self.is_leader(i) { 0u8 } else { 1u8 });

        let mut out = String::new();
        // Wire-format break: header gains a trailing `group` column on
        // top of the prior `leader` column. The manager parser is
        // tolerant — 5/6-column legacy headers parse with the missing
        // columns defaulted (leader=false, group=None).
        out.push_str("id\tkind\tstatus\tparent\tai_core\tleader\tgroup\n");
        for &i in &indices {
            let chat = &self.chats[i];
            let parent = chat
                .parent_idx
                .and_then(|p| self.chats.get(p))
                .map(|c| c.display_name.as_str())
                .unwrap_or("-");
            let core = chat.ai_core_id.as_deref().unwrap_or("-");
            let leader = self.is_leader(i);
            let group = self.chat_group(i).unwrap_or("-");
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                chat.display_name,
                Self::kind_label(chat),
                Self::status_label(chat),
                parent,
                core,
                leader,
                group,
            ));
        }
        let _ = reply.send(Response::ok(out.trim_end().to_string()));
    }

    fn handle_admin_agent_status(
        &mut self,
        sender_idx: usize,
        agent: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(chat) = self.chats.iter().find(|c| c.display_name == agent) else {
            let _ = reply.send(Response::err(format!("no agent named `{agent}`")));
            return;
        };
        let parent = chat
            .parent_idx
            .and_then(|p| self.chats.get(p))
            .map(|c| c.display_name.as_str())
            .unwrap_or("-");
        let core = chat.ai_core_id.as_deref().unwrap_or("-");
        let msg = format!(
            "id={}\nkind={}\nstatus={}\nhibernated={}\nasleep={}\nparent={}\nai_core={}\nblocks={}",
            chat.display_name,
            Self::kind_label(chat),
            Self::status_label(chat),
            chat.hibernated,
            chat.asleep,
            parent,
            core,
            chat.blocks.len(),
        );
        let _ = reply.send(Response::ok(msg));
    }

    fn handle_admin_agent_history(
        &mut self,
        sender_idx: usize,
        agent: String,
        selector: HistorySelector,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(chat) = self.chats.iter().find(|c| c.display_name == agent) else {
            let _ = reply.send(Response::err(format!("no agent named `{agent}`")));
            return;
        };
        let lines: Vec<String> = chat.blocks.iter().map(history_block_line).collect();
        let slice: Vec<&str> = match selector {
            HistorySelector::All => lines.iter().map(String::as_str).collect(),
            HistorySelector::Last { n } => {
                let start = lines.len().saturating_sub(n);
                lines[start..].iter().map(String::as_str).collect()
            }
            HistorySelector::LastMessage => chat
                .blocks
                .iter()
                .zip(lines.iter())
                .rev()
                .find(|(b, _)| matches!(b, ChatBlock::AiMarkdown { .. }))
                .map(|(_, l)| vec![l.as_str()])
                .unwrap_or_default(),
            HistorySelector::Kind { kind } => {
                let want = kind.to_ascii_lowercase();
                let keep: Vec<_> = chat
                    .blocks
                    .iter()
                    .zip(lines.iter())
                    .filter(|(b, _)| history_block_kind(b) == want)
                    .map(|(_, l)| l.as_str())
                    .collect();
                if keep.is_empty()
                    && !matches!(
                        want.as_str(),
                        "user"
                            | "ai"
                            | "mail_sent"
                            | "mail_received"
                            | "channel"
                            | "tool_use"
                            | "system_info"
                            | "system_warn"
                            | "system_error"
                            | "worker_spawn"
                            | "worker_complete"
                            | "agent_info"
                    )
                {
                    let _ = reply.send(Response::err(format!("unknown history kind `{kind}`")));
                    return;
                }
                keep
            }
        };
        let msg = if slice.is_empty() {
            "(no matching blocks)".to_string()
        } else {
            slice.join("\n")
        };
        let _ = reply.send(Response::ok(msg));
    }

    /// Structured variant of `handle_admin_agent_history`. Returns the
    /// selector-filtered blocks as `Vec<WireChatBlock>` in
    /// `Response::data`. Mirrors the supervisor gate and the selector
    /// semantics exactly so callers get the same answer as the
    /// plaintext form — just in a machine-friendly shape.
    fn handle_admin_agent_history_json(
        &mut self,
        sender_idx: usize,
        agent: String,
        selector: HistorySelector,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(chat) = self.chats.iter().find(|c| c.display_name == agent) else {
            let _ = reply.send(Response::err(format!("no agent named `{agent}`")));
            return;
        };
        let blocks: Vec<&crate::tui::ChatBlock> = match &selector {
            HistorySelector::All => chat.blocks.iter().collect(),
            HistorySelector::Last { n } => {
                let start = chat.blocks.len().saturating_sub(*n);
                chat.blocks[start..].iter().collect()
            }
            HistorySelector::LastMessage => chat
                .blocks
                .iter()
                .rev()
                .find(|b| matches!(b, crate::tui::ChatBlock::AiMarkdown { .. }))
                .into_iter()
                .collect(),
            HistorySelector::Kind { kind } => {
                let want = kind.to_ascii_lowercase();
                let known = matches!(
                    want.as_str(),
                    "user"
                        | "ai"
                        | "mail_sent"
                        | "mail_received"
                        | "channel"
                        | "tool_use"
                        | "system_info"
                        | "system_warn"
                        | "system_error"
                        | "worker_spawn"
                        | "worker_complete"
                        | "agent_info"
                );
                if !known {
                    let _ = reply.send(Response::err(format!("unknown history kind `{kind}`")));
                    return;
                }
                chat.blocks
                    .iter()
                    .filter(|b| history_block_kind(b) == want)
                    .collect()
            }
        };
        let wire: Vec<crate::wire_chat::WireChatBlock> = blocks
            .iter()
            .map(|b| crate::wire_chat::WireChatBlock::from(*b))
            .collect();
        match serde_json::to_value(&wire) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("serialize wire blocks: {e}")));
            }
        }
    }

    fn handle_admin_agent_stop(
        &mut self,
        sender_idx: usize,
        agent: String,
        reason: Option<String>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(target_idx) = self.chats.iter().position(|c| c.display_name == agent) else {
            let _ = reply.send(Response::err(format!("no agent named `{agent}`")));
            return;
        };
        let reason_line = match reason {
            Some(r) if !r.trim().is_empty() => format!("[stopped by supervisor: {}]", r.trim()),
            _ => "[stopped by supervisor]".to_string(),
        };
        self.stop_turn(target_idx, &reason_line);
        let _ = reply.send(Response::ok("stopped"));
    }

    fn handle_admin_agent_sleep(
        &mut self,
        sender_idx: usize,
        agent: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(target_idx) = self.chats.iter().position(|c| c.display_name == agent) else {
            let _ = reply.send(Response::err(format!("no agent named `{agent}`")));
            return;
        };
        self.stop_turn(target_idx, "[slept by supervisor]");
        if let Some(chat) = self.chats.get_mut(target_idx) {
            if !chat.asleep {
                chat.asleep = true;
                if chat.status != AgentStatus::Hibernated {
                    chat.status = AgentStatus::Asleep;
                }
            }
        }
        let _ = reply.send(Response::ok("asleep"));
    }

    fn handle_admin_agent_wake(
        &mut self,
        sender_idx: usize,
        agent: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let Some(target_idx) = self.chats.iter().position(|c| c.display_name == agent) else {
            let _ = reply.send(Response::err(format!("no agent named `{agent}`")));
            return;
        };
        if let Some(chat) = self.chats.get_mut(target_idx) {
            if chat.asleep {
                chat.asleep = false;
                if chat.status == AgentStatus::Asleep {
                    chat.status = AgentStatus::Ready;
                }
                chat.push_block(ChatBlock::system_info("woken by supervisor"));
            }
        }
        let _ = reply.send(Response::ok("awake"));
    }

    fn handle_admin_agent_send_as_user(
        &mut self,
        sender_idx: usize,
        to: String,
        body: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let body = body.trim().to_string();
        if body.is_empty() {
            let _ = reply.send(Response::err("body is empty"));
            return;
        }
        let Some(target_idx) = self.chats.iter().position(|c| c.display_name == to) else {
            let _ = reply.send(Response::err(format!("no agent named `{to}`")));
            return;
        };
        // Offline short-circuit: without a user_tx the local-input path
        // silently drops the message, which would leave the caller with
        // an ok reply and a dangling user block. Error out first so the
        // web portal can show an actionable message.
        if self.chats[target_idx].user_tx.is_none() {
            let _ = reply.send(Response::err(format!("agent `{to}` is not online")));
            return;
        }
        // Delegate to the same path the TUI uses when a human types
        // into the pane. It echoes a UserText block, routes slash
        // commands, flips status to Working, and surfaces asleep /
        // closed-channel errors as system blocks in the chat —
        // anything watching the scoped SSE stream sees the full
        // trajectory just like the TUI operator would.
        self.handle_local_input(target_idx, body);
        let _ = reply.send(Response::ok("sent"));
    }

    fn handle_admin_worker_list(
        &mut self,
        sender_idx: usize,
        parent_filter: Option<String>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        let mut out = String::new();
        out.push_str("id\tparent\ttemplate\tstatus\n");
        let mut count = 0usize;
        for chat in &self.chats {
            let Some(parent_idx) = chat.parent_idx else {
                continue;
            };
            let parent_name = self
                .chats
                .get(parent_idx)
                .map(|c| c.display_name.as_str())
                .unwrap_or("-");
            if let Some(want) = &parent_filter {
                if parent_name != want {
                    continue;
                }
            }
            let template = chat.worker_template.as_deref().unwrap_or("-");
            out.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                chat.display_name,
                parent_name,
                template,
                Self::status_label(chat),
            ));
            count += 1;
        }
        if count == 0 {
            let _ = reply.send(Response::ok("(no workers)".to_string()));
            return;
        }
        let _ = reply.send(Response::ok(out.trim_end().to_string()));
    }

    fn handle_admin_worker_force_archive(
        &mut self,
        sender_idx: usize,
        worker: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_supervisor(sender_idx) {
            let _ = reply.send(Response::err("not a supervisor"));
            return;
        }
        self.archive_worker(sender_idx, worker, true, reply);
    }

    fn handle_admin_server_shutdown(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        self.begin_shutdown(None);
        let _ = reply.send(Response::ok("shutting down"));
    }

    fn handle_admin_server_exit(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        info!("server exit requested by supervisor");
        self.should_quit = true;
        let _ = reply.send(Response::ok("exiting"));
    }

    fn handle_admin_server_reload(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        info!("server reload requested by supervisor");
        match self.reload() {
            Ok(summary) => {
                let _ = reply.send(Response::ok(summary));
            }
            Err(e) => {
                warn!(error = %e, "reload failed");
                let _ = reply.send(Response::err(e));
            }
        }
    }

    fn handle_admin_team_read(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        if self.config_path.as_os_str().is_empty() {
            let _ = reply.send(Response::err(
                "daemon was started without a config path; cannot read WCL",
            ));
            return;
        }
        let wcl = match std::fs::read_to_string(&self.config_path) {
            Ok(s) => s,
            Err(e) => {
                let _ = reply.send(Response::err(format!(
                    "reading config `{}`: {e}",
                    self.config_path.display()
                )));
                return;
            }
        };
        let payload = serde_json::json!({
            "wcl": wcl,
            "path": self.config_path.display().to_string(),
        });
        let _ = reply.send(Response::ok_data(payload));
    }

    fn handle_admin_team_write(
        &mut self,
        sender_idx: usize,
        wcl: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        if self.config_path.as_os_str().is_empty() {
            let _ = reply.send(Response::err(
                "daemon was started without a config path; cannot write WCL",
            ));
            return;
        }
        info!("team write requested by supervisor");
        // Validate before touching disk so a bad payload can never
        // replace a good config file.
        if let Err(e) = crate::config::parse_config(&wcl, &self.config_path) {
            let _ = reply.send(Response::err(format!("invalid WCL: {e}")));
            return;
        }
        // Atomic write: write to <path>.tmp then rename(2). On Unix
        // this guarantees the config file is either fully old or fully
        // new — no half-written state visible to the next reload.
        let target = self.config_path.clone();
        let tmp = match target.extension() {
            Some(ext) => {
                let mut s = ext.to_os_string();
                s.push(".tmp");
                target.with_extension(s)
            }
            None => target.with_extension("tmp"),
        };
        if let Err(e) = std::fs::write(&tmp, &wcl) {
            let _ = reply.send(Response::err(format!(
                "writing temp config `{}`: {e}",
                tmp.display()
            )));
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &target) {
            // Best-effort cleanup; ignore failure.
            let _ = std::fs::remove_file(&tmp);
            let _ = reply.send(Response::err(format!(
                "renaming temp config into place at `{}`: {e}",
                target.display()
            )));
            return;
        }
        match self.reload() {
            Ok(summary) => {
                let _ = reply.send(Response::ok(summary));
            }
            Err(e) => {
                warn!(error = %e, "reload after team write failed");
                let _ = reply.send(Response::err(e));
            }
        }
    }

    fn handle_admin_skills_list(
        &mut self,
        sender_idx: usize,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        let root = crate::skills::skills_root();
        let summaries = match crate::skills::read_skills_dir(&root) {
            Ok(s) => s,
            Err(e) => {
                let _ = reply.send(Response::err(format!("listing skills: {e}")));
                return;
            }
        };
        match serde_json::to_value(&summaries) {
            Ok(v) => {
                let _ = reply.send(Response::ok_data(v));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("encoding skills: {e}")));
            }
        }
    }

    fn handle_admin_skill_read(
        &mut self,
        sender_idx: usize,
        name: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        if !crate::skills::is_valid_skill_name(&name) {
            let _ = reply.send(Response::err(format!("invalid skill name `{name}`")));
            return;
        }
        let root = crate::skills::skills_root();
        let dir = root.join(&name);
        if !dir.is_dir() {
            let _ = reply.send(Response::err(format!("skill `{name}` not found")));
            return;
        }
        let files = match crate::skills::read_skill_files(&dir) {
            Ok(f) => f,
            Err(e) => {
                let _ = reply.send(Response::err(format!("reading `{name}`: {e}")));
                return;
            }
        };
        let payload = serde_json::json!({
            "name": name,
            "files": files,
        });
        let _ = reply.send(Response::ok_data(payload));
    }

    fn handle_admin_skill_write(
        &mut self,
        sender_idx: usize,
        name: String,
        files: Vec<agent_ipc::SkillFile>,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        if !crate::skills::is_valid_skill_name(&name) {
            let _ = reply.send(Response::err(format!("invalid skill name `{name}`")));
            return;
        }
        if files.is_empty() {
            let _ = reply.send(Response::err("no files supplied"));
            return;
        }
        if !files.iter().any(|f| f.rel_path == "SKILL.md") {
            let _ = reply.send(Response::err("missing SKILL.md"));
            return;
        }
        for f in &files {
            if let Err(e) = crate::skills::validate_skill_rel_path(&f.rel_path) {
                let _ = reply.send(Response::err(format!("invalid path `{}`: {e}", f.rel_path)));
                return;
            }
        }
        match crate::skills::write_skill_atomic(&name, &files) {
            Ok(()) => {
                info!(skill = %name, files = files.len(), "skill installed via supervisor RPC");
                let _ = reply.send(Response::ok(format!("wrote `{name}`")));
            }
            Err(e) => {
                warn!(skill = %name, error = %e, "skill write failed");
                let _ = reply.send(Response::err(format!("writing `{name}`: {e}")));
            }
        }
    }

    fn handle_admin_skill_remove(
        &mut self,
        sender_idx: usize,
        name: String,
        reply: tokio::sync::oneshot::Sender<Response>,
    ) {
        if !self.is_sysadmin(sender_idx) {
            let _ = reply.send(Response::err("not a sysadmin"));
            return;
        }
        if !crate::skills::is_valid_skill_name(&name) {
            let _ = reply.send(Response::err(format!("invalid skill name `{name}`")));
            return;
        }
        let dir = crate::skills::skills_root().join(&name);
        if !dir.exists() {
            let _ = reply.send(Response::ok(format!("`{name}` already absent")));
            return;
        }
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {
                info!(skill = %name, "skill removed via supervisor RPC");
                let _ = reply.send(Response::ok(format!("removed `{name}`")));
            }
            Err(e) => {
                let _ = reply.send(Response::err(format!("removing `{name}`: {e}")));
            }
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
            CommandOutcome::Shutdown => {
                self.begin_shutdown(Some(idx));
            }
            CommandOutcome::Compact => {
                self.begin_compaction(idx, false);
            }
            CommandOutcome::Reset => {
                self.begin_compaction(idx, true);
            }
            CommandOutcome::Reload => {
                let result = self.reload();
                if let Some(chat) = self.chats.get_mut(idx) {
                    let block = match result {
                        Ok(summary) => ChatBlock::system_info(summary),
                        Err(e) => ChatBlock::system_error(format!("reload failed: {e}")),
                    };
                    chat.push_block(block);
                }
            }
            CommandOutcome::Stop => {
                self.stop_turn(idx, "[stopped by /stop]");
            }
            CommandOutcome::Sleep => {
                self.stop_turn(idx, "[stopped by /sleep]");
                if let Some(chat) = self.chats.get_mut(idx) {
                    if !chat.asleep {
                        chat.asleep = true;
                        // Don't clobber Hibernated — a hibernated worker
                        // stays Hibernated visually; the asleep flag
                        // itself prevents auto-resurrection.
                        if chat.status != AgentStatus::Hibernated {
                            chat.status = AgentStatus::Asleep;
                        }
                        chat.push_block(ChatBlock::system_info(
                            "agent is now asleep — type /awake to resume",
                        ));
                    } else {
                        chat.push_block(ChatBlock::system_info("agent is already asleep"));
                    }
                }
            }
            CommandOutcome::Awake => {
                if let Some(chat) = self.chats.get_mut(idx) {
                    if chat.asleep {
                        chat.asleep = false;
                        if chat.status == AgentStatus::Asleep {
                            chat.status = AgentStatus::Ready;
                        }
                        chat.push_block(ChatBlock::system_info("agent awake"));
                    } else {
                        chat.push_block(ChatBlock::system_info("agent is not asleep"));
                    }
                }
            }
            CommandOutcome::RunWorkflow { workflow_id } => {
                let result = self.start_workflow_with_defaults(&workflow_id);
                if let Some(chat) = self.chats.get_mut(idx) {
                    let msg = match result {
                        Ok(id) => ChatBlock::system_info(format!(
                            "workflow `{workflow_id}` started as instance #{id}"
                        )),
                        Err(m) => ChatBlock::system_warn(m),
                    };
                    chat.push_block(msg);
                }
            }
            CommandOutcome::CreateMeeting { id } => {
                let result = self.create_meeting(id.clone());
                match result {
                    Ok(new_idx) => {
                        self.selected_channel = new_idx;
                        self.active_view = ActiveView::Channel(new_idx);
                    }
                    Err(msg) => {
                        if let Some(chat) = self.chats.get_mut(idx) {
                            chat.push_block(ChatBlock::system_warn(msg));
                        }
                    }
                }
            }
            CommandOutcome::AddAgent { .. }
            | CommandOutcome::KickAgent { .. }
            | CommandOutcome::LeaveChannel => {
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info(
                        "/add, /kick, /leave target channels — run them from a channel pane",
                    ));
                }
            }
            CommandOutcome::AuthLogin { provider, name } => {
                let msg = handle_auth_login(&provider, &name);
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info(msg));
                }
            }
            CommandOutcome::AuthLogout { name } => {
                let msg = handle_auth_logout(&name);
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info(msg));
                }
            }
            CommandOutcome::AuthList => {
                let msg = handle_auth_list();
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info(msg));
                }
            }
            CommandOutcome::AuthStatus { name } => {
                let msg = handle_auth_status(name.as_deref());
                if let Some(chat) = self.chats.get_mut(idx) {
                    chat.push_block(ChatBlock::system_info(msg));
                }
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

    /// Kick off a workflow instance using declared defaults for every
    /// property. Surfaced via the `/run <wf>` command and the TUI
    /// form's Run button. Returns the assigned instance id on success.
    pub(crate) fn start_workflow_with_defaults(
        &mut self,
        workflow_id: &str,
    ) -> Result<u64, String> {
        let workflow_idx = match self
            .config
            .workflows
            .iter()
            .position(|w| w.id == workflow_id)
        {
            Some(i) => i,
            None => return Err(format!("no workflow `{workflow_id}` declared")),
        };
        let engine = self
            .workflow_engine
            .as_mut()
            .ok_or_else(|| "workflow engine not initialized in this session".to_string())?;

        let workflow = self.config.workflows[workflow_idx].clone();
        let instance_id = self.next_instance_id;
        self.next_instance_id += 1;

        // Seed every property from its declared default (user-visible
        // + hidden). Required properties without a default are absent;
        // step templates referencing them will fail the render, which
        // is the right outcome for a "hit Enter on an incomplete form"
        // smoke test.
        let initial_vars =
            crate::workflow::WorkflowEngine::seed_vars(&workflow, std::collections::HashMap::new());
        engine.start_instance(instance_id, &workflow, initial_vars);

        // TUI mirror: the real `Started` event will populate vars, but
        // pushing the view eagerly means the sidebar shows the new
        // instance without waiting for the event pump to tick.
        self.workflow_instances
            .push(super::workflow_view::WorkflowInstanceView::new(
                instance_id,
                workflow_idx,
                &workflow,
            ));
        Ok(instance_id)
    }

    /// Consume one `WorkflowEvent` from the backend and update the
    /// TUI's instance mirror. Successful instances are removed from
    /// the sidebar; failed ones linger with a red icon until the user
    /// dismisses them.
    pub(crate) fn on_workflow_event(&mut self, ev: crate::workflow::WorkflowEvent) {
        use crate::workflow::{InstanceStatus, StepStatus, WorkflowEvent};
        // Wire-level projection constructed before the consuming match
        // so the SSE fan-out at the bottom sees the event in its entirety.
        // `send` fails with SendError only when there are zero
        // subscribers — that's the steady state when no browser is
        // watching, and we simply drop the event.
        let wire = crate::wire_workflow::WireWorkflowEvent::from_event(&ev);
        match ev {
            WorkflowEvent::Started {
                instance_id, vars, ..
            } => {
                if let Some(view) = self
                    .workflow_instances
                    .iter_mut()
                    .find(|i| i.id == instance_id)
                {
                    view.vars.extend(vars);
                }
            }
            WorkflowEvent::StepStarted {
                instance_id,
                step_idx,
            } => {
                if let Some(view) = self
                    .workflow_instances
                    .iter_mut()
                    .find(|i| i.id == instance_id)
                {
                    view.current_step = step_idx;
                    if let Some(step) = view.steps.get_mut(step_idx) {
                        step.status = StepStatus::Running;
                    }
                }
            }
            WorkflowEvent::StepOutput { instance_id, line } => {
                if let Some(view) = self
                    .workflow_instances
                    .iter_mut()
                    .find(|i| i.id == instance_id)
                {
                    view.push_output(line);
                }
            }
            WorkflowEvent::VarChanged {
                instance_id,
                name,
                value,
            } => {
                if let Some(view) = self
                    .workflow_instances
                    .iter_mut()
                    .find(|i| i.id == instance_id)
                {
                    view.vars.insert(name, value);
                }
            }
            WorkflowEvent::StepFinished {
                instance_id,
                step_idx,
                outcome,
            } => {
                if let Some(view) = self
                    .workflow_instances
                    .iter_mut()
                    .find(|i| i.id == instance_id)
                {
                    if let Some(step) = view.steps.get_mut(step_idx) {
                        step.status = match &outcome {
                            crate::workflow::StepOutcome::Ok => StepStatus::Succeeded,
                            crate::workflow::StepOutcome::Failed { reason } => {
                                step.reason = Some(reason.clone());
                                StepStatus::Failed
                            }
                            crate::workflow::StepOutcome::Retry { reason } => {
                                step.reason = Some(reason.clone());
                                StepStatus::Retrying
                            }
                        };
                    }
                }
            }
            WorkflowEvent::Retrying {
                instance_id,
                target_step_idx,
                attempt,
            } => {
                if let Some(view) = self
                    .workflow_instances
                    .iter_mut()
                    .find(|i| i.id == instance_id)
                {
                    if let Some(step) = view.steps.get_mut(target_step_idx) {
                        step.retry_count = attempt;
                        step.status = StepStatus::Pending;
                    }
                    view.current_step = target_step_idx;
                }
            }
            WorkflowEvent::Finished {
                instance_id,
                ok,
                reason,
            } => {
                if ok {
                    // Successful instances linger in the sidebar for
                    // a few seconds with a green glyph so the operator
                    // actually sees the completion; they're then
                    // auto-removed by `prune_completed_workflows`.
                    if let Some(view) = self
                        .workflow_instances
                        .iter_mut()
                        .find(|i| i.id == instance_id)
                    {
                        view.status = InstanceStatus::Succeeded;
                        view.remove_at =
                            Some(std::time::Instant::now() + super::workflow_view::SUCCESS_LINGER);
                    }
                } else if let Some(view) = self
                    .workflow_instances
                    .iter_mut()
                    .find(|i| i.id == instance_id)
                {
                    view.status = InstanceStatus::Failed;
                    view.failure_reason = reason;
                }
            }
        }
        let _ = self.workflow_event_tx.send(wire);
    }

    /// Drop any workflow instance whose `remove_at` deadline has
    /// passed. Called from the event loop's spinner tick so expired
    /// successes leave the sidebar without needing a dedicated timer.
    /// Running and failed instances ignore this (they have no
    /// deadline); failed instances require explicit `d` dismissal.
    pub(crate) fn prune_completed_workflows(&mut self) -> bool {
        let now = std::time::Instant::now();
        let mut to_remove: Vec<u64> = Vec::new();
        for inst in &self.workflow_instances {
            if let Some(t) = inst.remove_at {
                if now >= t {
                    to_remove.push(inst.id);
                }
            }
        }
        if to_remove.is_empty() {
            return false;
        }
        // Track whether the currently-viewed instance was pruned so
        // we can snap the main view back to Chat.
        let mut snap_back = false;
        for id in &to_remove {
            if matches!(
                self.active_view,
                ActiveView::WorkflowInstance { instance_id, .. } if instance_id == *id
            ) {
                snap_back = true;
            }
            if let Some(engine) = self.workflow_engine.as_mut() {
                engine.forget(*id);
            }
        }
        self.workflow_instances
            .retain(|i| !to_remove.contains(&i.id));
        if snap_back {
            self.active_view = ActiveView::Chat;
            self.focus = Focus::Chat;
        }
        true
    }

    pub fn handle_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        // Tab cycles the visible right-column panels plus the main
        // area — UNLESS a Tasks-view or Memory-view modal is open, in
        // which case Tab belongs to that modal's field navigation and
        // must reach the view's own key handler.
        let tasks_modal_open = self.active_view == ActiveView::Tasks
            && self.focus == Focus::Tasks
            && (self.tasks_screen.form.is_some() || self.tasks_screen.prompt.is_some());
        let memory_modal_open = self.active_view == ActiveView::Memory
            && self.focus == Focus::Memory
            && self.memory_screen.has_active_modal();
        if matches!(code, KeyCode::Tab) && !tasks_modal_open && !memory_modal_open {
            // Inbox sub-pane Tab: when focused on the inbox list,
            // first Tab moves focus into the detail pane (still on
            // Focus::Inbox). The next Tab advances the outer cycle
            // and resets the sub-pane back to List.
            if self.focus == Focus::Inbox && self.active_view == ActiveView::Inbox {
                use super::inbox_view::InboxPane;
                match self.inbox_screen.pane {
                    InboxPane::List if !self.inbox.is_empty() => {
                        self.inbox_screen.pane = InboxPane::Detail;
                        return;
                    }
                    InboxPane::Detail => {
                        self.inbox_screen.pane = InboxPane::List;
                        // Fall through to the outer cycle below so
                        // Detail → outer-Tab leaves the inbox view
                        // entirely and lands on Agents.
                    }
                    _ => {}
                }
            }
            let main = match self.active_view {
                ActiveView::Chat | ActiveView::Channel(_) => Focus::Chat,
                ActiveView::Tasks => Focus::Tasks,
                ActiveView::Memory => Focus::Memory,
                ActiveView::Cores => Focus::Cores,
                ActiveView::Inbox => Focus::Inbox,
                ActiveView::Cron => Focus::Cron,
                ActiveView::WorkflowForm { .. } | ActiveView::WorkflowInstance { .. } => {
                    Focus::Workflow
                }
            };
            let has_channels = !self.channels.is_empty();
            let has_workflows = !self.config.workflows.is_empty();
            self.focus = match self.focus {
                f if f == main => Focus::Agents,
                Focus::Agents => Focus::Views,
                Focus::Views => {
                    if has_workflows {
                        Focus::Workflows
                    } else if has_channels {
                        Focus::Channels
                    } else {
                        main
                    }
                }
                Focus::Workflows => {
                    if has_channels {
                        Focus::Channels
                    } else {
                        main
                    }
                }
                Focus::Channels => main,
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
            Focus::Channels => self.handle_key_channels(code, mods),
            Focus::Memory => self.handle_key_memory(code, mods),
            Focus::Workflows => self.handle_key_workflows(code, mods),
            Focus::Workflow => self.handle_key_workflow_view(code, mods),
            Focus::Cores => self.handle_key_cores(code, mods),
            Focus::Inbox => self.handle_key_inbox(code, mods),
            Focus::Cron => self.handle_key_cron(code, mods),
        }
    }

    fn handle_key_agents(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
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

    /// If the user is in any non-chat main view (Tasks or a Channel)
    /// and interacts with the agents list, swap the main area back
    /// to the agent chat. The operator is clearly asking for an
    /// agent, so stop showing the alternate view.
    fn return_to_chat_if_viewing_tasks(&mut self) {
        match self.active_view {
            ActiveView::Tasks => {
                self.active_view = ActiveView::Chat;
                // Close any open Tasks modals so they don't get
                // stranded off-screen.
                self.tasks_screen.close_modal();
            }
            ActiveView::Memory => {
                self.active_view = ActiveView::Chat;
                self.memory_screen.close_overlays();
            }
            ActiveView::Cores => {
                self.active_view = ActiveView::Chat;
            }
            ActiveView::Inbox => {
                self.active_view = ActiveView::Chat;
                self.inbox_screen.mode = super::inbox_view::InboxMode::List;
            }
            ActiveView::Cron => {
                self.active_view = ActiveView::Chat;
                self.cron_screen.mode = super::cron_view::CronMode::List;
            }
            ActiveView::Channel(_) => {
                self.active_view = ActiveView::Chat;
            }
            ActiveView::WorkflowForm { .. } | ActiveView::WorkflowInstance { .. } => {
                self.active_view = ActiveView::Chat;
                self.workflow_form = None;
            }
            ActiveView::Chat => {}
        }
    }

    /// Handle a keypress while the Workflows sidebar list has focus.
    /// j/k moves the row cursor across the flattened Header+Instance
    /// list (so instances under the expanded workflow are reachable),
    /// Enter activates the current row, `d` dismisses a failed
    /// instance, Esc/q returns focus to Agents.
    fn handle_key_workflows(&mut self, code: KeyCode, _mods: KeyModifiers) {
        if self.config.workflows.is_empty() {
            self.focus = Focus::Agents;
            return;
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.focus = Focus::Agents;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_workflow + 1 < self.config.workflows.len() {
                    self.selected_workflow += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_workflow > 0 {
                    self.selected_workflow -= 1;
                }
            }
            KeyCode::Enter => {
                let idx = self.selected_workflow.min(self.config.workflows.len() - 1);
                self.open_workflow_form(idx);
            }
            _ => {}
        }
    }

    /// Open the property-entry form for the workflow at `idx`. The
    /// form is seeded from declared defaults; hidden properties are
    /// filtered out by `WorkflowFormState::new`.
    pub(crate) fn open_workflow_form(&mut self, idx: usize) {
        let Some(wf) = self.config.workflows.get(idx) else {
            return;
        };
        self.workflow_form = Some(super::workflow_view::WorkflowFormState::new(idx, wf));
        self.active_view = ActiveView::WorkflowForm { workflow_idx: idx };
        self.focus = Focus::Workflow;
    }

    /// Open the live progress view for an existing instance.
    pub(crate) fn open_workflow_instance(&mut self, workflow_idx: usize, instance_id: u64) {
        self.active_view = ActiveView::WorkflowInstance {
            workflow_idx,
            instance_id,
        };
        self.focus = Focus::Workflow;
    }

    /// Handler for keys while a workflow form or progress view is
    /// active. Dispatches to specialised handlers for the two views.
    fn handle_key_workflow_view(&mut self, code: KeyCode, mods: KeyModifiers) {
        match self.active_view {
            ActiveView::WorkflowForm { workflow_idx } => {
                self.handle_key_workflow_form(workflow_idx, code, mods)
            }
            ActiveView::WorkflowInstance {
                workflow_idx,
                instance_id,
            } => self.handle_key_workflow_instance(workflow_idx, instance_id, code, mods),
            _ => {
                // Defensive: if focus drifted without a matching view,
                // return to chat.
                self.active_view = ActiveView::Chat;
                self.focus = Focus::Chat;
            }
        }
    }

    fn handle_key_workflow_form(&mut self, workflow_idx: usize, code: KeyCode, mods: KeyModifiers) {
        use super::workflow_view::{FieldEditor, FormButton, FormFocus};
        // Esc: close the picker if open, otherwise close the form.
        if matches!(code, KeyCode::Esc) {
            if let Some(form) = self.workflow_form.as_mut() {
                if form.picker.is_some() {
                    form.close_picker();
                    return;
                }
            }
            self.active_view = ActiveView::Chat;
            self.focus = Focus::Chat;
            self.workflow_form = None;
            return;
        }
        let Some(form) = self.workflow_form.as_mut() else {
            // Lost form state — rebuild or bail.
            return;
        };
        // Any keypress clears a lingering validation error.
        form.error = None;
        // Picker open: navigation + commit route through the picker so
        // arrow keys don't drift form focus.
        if form.picker.is_some() {
            match code {
                KeyCode::Up => form.picker_move(-1),
                KeyCode::Down => form.picker_move(1),
                KeyCode::Enter => form.commit_picker(),
                _ => {}
            }
            return;
        }
        match code {
            KeyCode::Tab => {
                if mods.contains(KeyModifiers::SHIFT) {
                    form.focus_prev();
                } else {
                    form.focus_next();
                }
            }
            KeyCode::BackTab => form.focus_prev(),
            KeyCode::Enter => {
                // On a focused Select, Enter opens the picker so the
                // keyboard has parity with mouse click. Otherwise,
                // Enter advances focus (or submits on the last
                // field / Run button).
                if let FormFocus::Field(i) = form.focus {
                    if let Some(FieldEditor::Select { .. }) = form.fields.get(i) {
                        form.open_picker(i);
                        return;
                    }
                }
                match form.focus {
                    FormFocus::Field(i) if i + 1 < form.fields.len() => form.focus_next(),
                    FormFocus::Field(_) | FormFocus::Button(FormButton::Run) => {
                        self.submit_workflow_form(workflow_idx);
                    }
                }
            }
            KeyCode::Left => {
                if let Some(field) = form.focused_field_mut() {
                    field.move_left();
                }
            }
            KeyCode::Right => {
                if let Some(field) = form.focused_field_mut() {
                    field.move_right();
                }
            }
            KeyCode::Up | KeyCode::Down => {
                // MultiSelect cursor navigation.
                if let FormFocus::Field(i) = form.focus {
                    if let Some(FieldEditor::MultiSelect { options, .. }) = form.fields.get(i) {
                        let len = options.len();
                        if len == 0 {
                            return;
                        }
                        let cur = form.multi_cursor(i);
                        let next = if matches!(code, KeyCode::Up) {
                            cur.saturating_sub(1)
                        } else {
                            (cur + 1).min(len - 1)
                        };
                        form.set_multi_cursor(i, next);
                    }
                }
            }
            KeyCode::Char(' ') => {
                // Space toggles the cursor option on a MultiSelect;
                // typed as a literal space in Text fields.
                if let FormFocus::Field(i) = form.focus {
                    if let Some(FieldEditor::MultiSelect { .. }) = form.fields.get(i) {
                        let cur = form.multi_cursor(i);
                        if let Some(FieldEditor::MultiSelect { .. }) = form.fields.get_mut(i) {
                            if let Some(FieldEditor::MultiSelect { chosen, .. }) =
                                form.fields.get_mut(i)
                            {
                                if let Some(b) = chosen.get_mut(cur) {
                                    *b = !*b;
                                }
                            }
                        }
                        return;
                    }
                }
                if let Some(field) = form.focused_field_mut() {
                    field.insert_char(' ');
                }
            }
            KeyCode::Backspace => {
                if let Some(field) = form.focused_field_mut() {
                    field.backspace();
                }
            }
            KeyCode::Char(c) => {
                if let Some(field) = form.focused_field_mut() {
                    field.insert_char(c);
                }
            }
            _ => {}
        }
    }

    /// Build the property map from the form and hand it to the engine.
    /// Populates `form.error` on validation failure so the UI can
    /// surface it without losing focus.
    fn submit_workflow_form(&mut self, workflow_idx: usize) {
        let workflow = match self.config.workflows.get(workflow_idx).cloned() {
            Some(w) => w,
            None => return,
        };
        let Some(form) = self.workflow_form.as_mut() else {
            return;
        };
        let inputs = match form.submit_values(&workflow) {
            Ok(m) => m,
            Err(msg) => {
                form.error = Some(msg);
                return;
            }
        };
        // Merge with declared defaults (for hidden properties).
        let merged = crate::workflow::WorkflowEngine::seed_vars(&workflow, inputs);

        let engine = match self.workflow_engine.as_mut() {
            Some(e) => e,
            None => {
                form.error = Some("workflow engine not initialized".into());
                return;
            }
        };
        let instance_id = self.next_instance_id;
        self.next_instance_id += 1;
        engine.start_instance(instance_id, &workflow, merged);

        self.workflow_instances
            .push(super::workflow_view::WorkflowInstanceView::new(
                instance_id,
                workflow_idx,
                &workflow,
            ));

        // Transition to the progress view.
        self.active_view = ActiveView::WorkflowInstance {
            workflow_idx,
            instance_id,
        };
        self.workflow_form = None;
    }

    fn handle_key_workflow_instance(
        &mut self,
        _workflow_idx: usize,
        instance_id: u64,
        code: KeyCode,
        _mods: KeyModifiers,
    ) {
        match code {
            KeyCode::Esc => {
                self.active_view = ActiveView::Chat;
                self.focus = Focus::Chat;
            }
            KeyCode::Char('d') => {
                // Only dismiss failed instances; running ones ignore
                // the key so the user can't accidentally lose state.
                if self.dismiss_workflow_instance(instance_id).is_ok() {
                    self.active_view = ActiveView::Chat;
                    self.focus = Focus::Chat;
                }
            }
            _ => {}
        }
    }

    /// Drop a failed workflow instance from the TUI mirror + engine.
    /// Shared by the `'d'` keybind and the `WorkflowDismiss` IPC.
    /// Returns a stable string error on the refusal paths so IPC
    /// callers can surface a useful message.
    pub(crate) fn dismiss_workflow_instance(
        &mut self,
        instance_id: u64,
    ) -> Result<(), &'static str> {
        let Some(pos) = self
            .workflow_instances
            .iter()
            .position(|i| i.id == instance_id)
        else {
            return Err("no such workflow instance");
        };
        if !matches!(
            self.workflow_instances[pos].status,
            crate::workflow::InstanceStatus::Failed
        ) {
            return Err("only failed workflow instances can be dismissed");
        }
        self.workflow_instances.remove(pos);
        if let Some(engine) = self.workflow_engine.as_mut() {
            engine.forget(instance_id);
        }
        Ok(())
    }

    /// Views-list entries for this fleet. Filters out items whose
    /// backing surface is empty — e.g. `Memory` is hidden when no
    /// memory banks are declared, so server/client demo configs
    /// (which have no banks) don't offer a dead click target.
    pub fn view_entries(&self) -> Vec<&'static str> {
        VIEW_ENTRIES
            .iter()
            .copied()
            .filter(|name| match *name {
                "Memory" => !self.config.memory_banks.is_empty(),
                _ => true,
            })
            .collect()
    }

    fn handle_key_views(&mut self, code: KeyCode, _mods: KeyModifiers) {
        let entries = self.view_entries();
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_view + 1 < entries.len() {
                    self.selected_view += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_view > 0 {
                    self.selected_view -= 1;
                }
            }
            KeyCode::Enter => match entries.get(self.selected_view).copied() {
                Some("Tasks") => {
                    self.active_view = ActiveView::Tasks;
                    self.focus = Focus::Tasks;
                    let rows = TasksScreen::rows(&self.tasks);
                    self.tasks_screen.reclamp(&rows);
                }
                Some("Memory") => {
                    self.open_memory_view();
                }
                Some("Cores") => {
                    self.open_cores_view();
                }
                Some("Inbox") => {
                    self.open_inbox_view();
                }
                Some("Cron") => {
                    self.open_cron_view();
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub(super) fn open_inbox_view(&mut self) {
        self.active_view = ActiveView::Inbox;
        self.focus = Focus::Inbox;
        // Always land on the list pane when entering the view.
        self.inbox_screen.pane = super::inbox_view::InboxPane::List;
        self.inbox_screen.reclamp(self.inbox.len());
    }

    pub(super) fn open_cron_view(&mut self) {
        self.active_view = ActiveView::Cron;
        self.focus = Focus::Cron;
        self.cron_screen.mode = super::cron_view::CronMode::List;
        self.cron_screen.status_msg = None;
        let len = super::cron_view::listed_jobs(&self.cron).len();
        self.cron_screen.reclamp(len);
    }

    fn handle_key_cron(&mut self, code: KeyCode, mods: KeyModifiers) {
        use super::cron_view::{
            CronForm, CronFormField, CronFormKind, CronMode, CronPayloadKind, CronScheduleKind,
        };

        // Snapshot jobs once — repeated mutable borrows below would
        // make the match arms hard to read.
        let jobs = super::cron_view::listed_jobs(&self.cron);

        // Clear stale toast on any keypress once we've read it.
        let had_status = self.cron_screen.status_msg.is_some();
        let clear_status = |this: &mut Self| {
            if had_status {
                this.cron_screen.status_msg = None;
            }
        };

        // --- ConfirmDelete overlay ---------------------------------
        if let CronMode::ConfirmDelete { job_id, .. } = self.cron_screen.mode.clone() {
            match code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    match self.cron.remove(job_id) {
                        Ok(_) => {
                            self.cron_invalidate.notify_one();
                            let _ = self
                                .cron_event_tx
                                .send(crate::cron::CronEvent::Removed { id: job_id });
                            self.cron_screen.status_msg =
                                Some(format!("removed cron job {job_id}"));
                        }
                        Err(e) => {
                            self.cron_screen.status_msg = Some(format!("remove failed: {e}"));
                        }
                    }
                    self.cron_screen.mode = CronMode::List;
                    let len = super::cron_view::listed_jobs(&self.cron).len();
                    self.cron_screen.reclamp(len);
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.cron_screen.mode = CronMode::List;
                }
                _ => {}
            }
            return;
        }

        // --- Form overlay -----------------------------------------
        if let CronMode::Form(mut form) = self.cron_screen.mode.clone() {
            clear_status(self);
            form.error = None;

            match code {
                KeyCode::Esc => {
                    self.cron_screen.mode = CronMode::List;
                    return;
                }
                KeyCode::Tab => {
                    form.cycle_focus(!mods.contains(KeyModifiers::SHIFT));
                    self.cron_screen.mode = CronMode::Form(form);
                    return;
                }
                KeyCode::BackTab => {
                    form.cycle_focus(false);
                    self.cron_screen.mode = CronMode::Form(form);
                    return;
                }
                KeyCode::Char(' ') if !form.focused.is_text() => {
                    match form.focused {
                        CronFormField::PayloadKind => {
                            form.payload_kind = form.payload_kind.toggle();
                        }
                        CronFormField::ScheduleKind => {
                            form.schedule_kind = form.schedule_kind.toggle();
                            // Catchup only makes sense for one-off jobs —
                            // clear it whenever we flip back to recurring
                            // so a stale checkbox doesn't confuse the
                            // next commit-time validation.
                            if form.schedule_kind == CronScheduleKind::Recurring {
                                form.catchup_on_startup = false;
                            }
                        }
                        CronFormField::Timezone => {
                            form.timezone = match form.timezone {
                                crate::cron::CronTz::Local => crate::cron::CronTz::Utc,
                                crate::cron::CronTz::Utc => crate::cron::CronTz::Local,
                            };
                        }
                        CronFormField::CatchupOnStartup => {
                            if form.schedule_kind == CronScheduleKind::OneOff {
                                form.catchup_on_startup = !form.catchup_on_startup;
                            } else {
                                form.error =
                                    Some("catchup_on_startup only applies to one-off jobs".into());
                            }
                        }
                        CronFormField::Enabled => {
                            form.enabled = !form.enabled;
                        }
                        _ => {}
                    }
                    self.cron_screen.mode = CronMode::Form(form);
                    return;
                }
                KeyCode::Enter => {
                    match form.focused {
                        CronFormField::SaveButton => match self.commit_cron_form(&form) {
                            Ok(msg) => {
                                self.cron_screen.status_msg = Some(msg);
                                self.cron_screen.mode = CronMode::List;
                                let len = super::cron_view::listed_jobs(&self.cron).len();
                                self.cron_screen.reclamp(len);
                            }
                            Err(e) => {
                                form.error = Some(e);
                                self.cron_screen.mode = CronMode::Form(form);
                            }
                        },
                        CronFormField::CancelButton => {
                            self.cron_screen.mode = CronMode::List;
                        }
                        _ => {
                            // Enter on a plain text field advances to
                            // the next field — same convention as the
                            // Tasks form so a fast typist doesn't need
                            // Tab.
                            form.cycle_focus(true);
                            self.cron_screen.mode = CronMode::Form(form);
                        }
                    }
                    return;
                }
                KeyCode::Backspace if form.focused.is_text() => {
                    let buf = match form.focused {
                        CronFormField::Target => &mut form.target,
                        CronFormField::PayloadBody => &mut form.payload_body,
                        CronFormField::ScheduleText => &mut form.schedule_text,
                        _ => unreachable!(),
                    };
                    buf.pop();
                    self.cron_screen.mode = CronMode::Form(form);
                    return;
                }
                KeyCode::Char(c) if form.focused.is_text() => {
                    let buf = match form.focused {
                        CronFormField::Target => &mut form.target,
                        CronFormField::PayloadBody => &mut form.payload_body,
                        CronFormField::ScheduleText => &mut form.schedule_text,
                        _ => unreachable!(),
                    };
                    buf.push(c);
                    self.cron_screen.mode = CronMode::Form(form);
                    return;
                }
                _ => {
                    self.cron_screen.mode = CronMode::Form(form);
                    return;
                }
            }
        }

        // --- List mode --------------------------------------------
        clear_status(self);
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.active_view = ActiveView::Chat;
                self.focus = Focus::Chat;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.cron_screen.cursor + 1 < jobs.len() {
                    self.cron_screen.cursor += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cron_screen.cursor > 0 {
                    self.cron_screen.cursor -= 1;
                }
            }
            KeyCode::Char('a') => {
                self.cron_screen.mode = CronMode::Form(super::cron_view::CronForm::new_add());
            }
            KeyCode::Char('e') => {
                if let Some(j) = jobs.get(self.cron_screen.cursor) {
                    self.cron_screen.mode = CronMode::Form(super::cron_view::CronForm::new_edit(j));
                }
            }
            KeyCode::Char('d') => {
                if let Some(j) = jobs.get(self.cron_screen.cursor) {
                    let display = format!("{} → {}", &j.id.to_string()[..8], j.target_agent);
                    self.cron_screen.mode = CronMode::ConfirmDelete {
                        job_id: j.id,
                        display,
                    };
                }
            }
            KeyCode::Char('x') => {
                if let Some(j) = jobs.get(self.cron_screen.cursor) {
                    let id = j.id;
                    let patch = crate::cron::CronPatch {
                        enabled: Some(!j.enabled),
                        ..Default::default()
                    };
                    match self.cron.update(id, patch) {
                        Ok(updated) => {
                            let wire = crate::cron::WireCronJob::from(updated);
                            self.cron_invalidate.notify_one();
                            let _ = self
                                .cron_event_tx
                                .send(crate::cron::CronEvent::Updated { job: wire });
                            self.cron_screen.status_msg = Some(format!("toggled cron job {id}"));
                        }
                        Err(e) => {
                            self.cron_screen.status_msg = Some(format!("toggle failed: {e}"));
                        }
                    }
                }
            }
            _ => {}
        }
        // Keep `CronFormKind`, `CronForm`, `CronPayloadKind`,
        // `CronFormField` imported even when list mode doesn't touch
        // them — easier for future edits than scoping them per-branch.
        let _ = CronFormKind::Add;
        let _ = CronForm::new_add;
        let _ = CronPayloadKind::Message;
        let _ = CronFormField::Target;
    }

    /// Parse + apply a [`super::cron_view::CronForm`] against the
    /// backing store. Returns a human-readable confirmation on success
    /// or a human-readable error string on failure (the caller puts
    /// the error back into the form's red banner).
    fn commit_cron_form(&mut self, form: &super::cron_view::CronForm) -> Result<String, String> {
        use super::cron_view::{CronFormKind, CronScheduleKind};

        let target = form.target.trim().to_string();
        if target.is_empty() {
            return Err("target is empty".into());
        }
        if !self.is_configured_agent(&target) {
            return Err(format!("target `{target}` is not a configured agent"));
        }

        let payload_kind = match form.payload_kind {
            super::cron_view::CronPayloadKind::Message => "message",
            super::cron_view::CronPayloadKind::SlashCommand => "command",
        };
        let payload = parse_cron_payload(payload_kind, form.payload_body.clone())?;

        let oneoff = form.schedule_kind == CronScheduleKind::OneOff;
        let schedule =
            parse_cron_schedule(&form.schedule_text, oneoff).map_err(|e| e.to_string())?;

        if form.catchup_on_startup && !oneoff {
            return Err("catchup_on_startup only applies to one-off jobs".into());
        }

        match form.kind {
            CronFormKind::Add => {
                match self.cron.add(
                    target,
                    payload,
                    schedule,
                    form.timezone,
                    form.catchup_on_startup,
                    form.enabled,
                    "user".to_string(),
                ) {
                    Ok(j) => {
                        let wire = crate::cron::WireCronJob::from(j);
                        let id = wire.id;
                        self.cron_invalidate.notify_one();
                        let _ = self
                            .cron_event_tx
                            .send(crate::cron::CronEvent::Created { job: wire });
                        Ok(format!("created cron job {id}"))
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
            CronFormKind::Edit(id) => {
                let patch = crate::cron::CronPatch {
                    target_agent: Some(target),
                    payload: Some(payload),
                    schedule: Some(schedule),
                    timezone: Some(form.timezone),
                    catchup_on_startup: Some(form.catchup_on_startup),
                    enabled: Some(form.enabled),
                };
                match self.cron.update(id, patch) {
                    Ok(updated) => {
                        let wire = crate::cron::WireCronJob::from(updated);
                        self.cron_invalidate.notify_one();
                        let _ = self
                            .cron_event_tx
                            .send(crate::cron::CronEvent::Updated { job: wire });
                        Ok(format!("updated cron job {id}"))
                    }
                    Err(e) => Err(e.to_string()),
                }
            }
        }
    }

    fn open_cores_view(&mut self) {
        self.active_view = ActiveView::Cores;
        self.focus = Focus::Cores;
        self.cores_screen.rebuild(&self.config.ai_cores);
        // Keep stream-derived reset info across open; it can't be
        // re-fetched on demand the way HTTP-backed entries can.
        self.cores_screen.usage.retain(|_, s| {
            s.claude_five_hour_resets_at.is_some() || s.claude_weekly_resets_at.is_some()
        });
        self.spawn_cores_usage_fetch();
    }

    fn handle_key_cores(&mut self, code: KeyCode, _mods: KeyModifiers) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.active_view = ActiveView::Chat;
                self.focus = Focus::Agents;
            }
            KeyCode::Char('j') | KeyCode::Down => self.cores_screen.move_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.cores_screen.move_cursor(-1),
            KeyCode::Char('r') => {
                // Preserve stream-derived entries across refresh —
                // only the HTTP providers can actually re-fetch.
                self.cores_screen.usage.retain(|_, s| {
                    s.claude_five_hour_resets_at.is_some() || s.claude_weekly_resets_at.is_some()
                });
                self.spawn_cores_usage_fetch();
            }
            _ => {}
        }
    }

    fn handle_key_inbox(&mut self, code: KeyCode, mods: KeyModifiers) {
        use super::inbox_view::{ForwardStage, InboxMode};
        self.inbox_screen.status_msg = None;

        // Forward overlay: two-stage (pick recipient → edit body).
        if let InboxMode::Forward(fwd) = &mut self.inbox_screen.mode {
            match fwd.stage {
                ForwardStage::PickRecipient => match code {
                    KeyCode::Esc => {
                        self.inbox_screen.mode = InboxMode::List;
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        if !fwd.candidates.is_empty() && fwd.selected + 1 < fwd.candidates.len() {
                            fwd.selected += 1;
                        }
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        if fwd.selected > 0 {
                            fwd.selected -= 1;
                        }
                    }
                    KeyCode::Enter => {
                        if fwd.candidates.is_empty() {
                            self.inbox_screen.status_msg =
                                Some("no agents to forward to".to_string());
                            return;
                        }
                        // Snapshot the quoted original now so it stays
                        // stable while the user composes their note.
                        let original = self.inbox.get(self.inbox_screen.cursor);
                        if let Some(msg) = original {
                            fwd.quoted = format!("Forwarded from {}:\n\n{}", msg.from, msg.body);
                        }
                        fwd.stage = ForwardStage::EditBody;
                    }
                    _ => {}
                },
                ForwardStage::EditBody => match code {
                    KeyCode::Esc => {
                        self.inbox_screen.mode = InboxMode::List;
                    }
                    KeyCode::Enter if mods.contains(KeyModifiers::CONTROL) => {
                        self.submit_inbox_send();
                    }
                    // Typing edits the user's note; the quoted original
                    // below is read-only context.
                    KeyCode::Char(c) => fwd.note.push(c),
                    KeyCode::Enter => fwd.note.push('\n'),
                    KeyCode::Backspace => {
                        fwd.note.pop();
                    }
                    _ => {}
                },
            }
            return;
        }

        // Reply overlay.
        if let InboxMode::Reply { body, .. } = &mut self.inbox_screen.mode {
            match code {
                KeyCode::Esc => {
                    self.inbox_screen.mode = InboxMode::List;
                }
                KeyCode::Enter if mods.contains(KeyModifiers::CONTROL) => {
                    self.submit_inbox_send();
                }
                KeyCode::Char(c) => body.push(c),
                KeyCode::Enter => body.push('\n'),
                KeyCode::Backspace => {
                    body.pop();
                }
                _ => {}
            }
            return;
        }

        // List mode (default).
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                // Esc from the detail pane returns to the list pane
                // (so it doesn't kick the user all the way out of the
                // inbox view in one keystroke).
                if self.inbox_screen.pane == super::inbox_view::InboxPane::Detail {
                    self.inbox_screen.pane = super::inbox_view::InboxPane::List;
                    return;
                }
                self.active_view = ActiveView::Chat;
                self.focus = Focus::Agents;
                self.inbox_screen.pane = super::inbox_view::InboxPane::List;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.inbox_screen.pane == super::inbox_view::InboxPane::Detail {
                    self.inbox_screen.detail_scroll =
                        self.inbox_screen.detail_scroll.saturating_add(1);
                } else if !self.inbox.is_empty()
                    && self.inbox_screen.cursor + 1 < self.inbox.len()
                {
                    self.inbox_screen
                        .select(self.inbox_screen.cursor + 1, self.inbox.len());
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.inbox_screen.pane == super::inbox_view::InboxPane::Detail {
                    self.inbox_screen.detail_scroll =
                        self.inbox_screen.detail_scroll.saturating_sub(1);
                } else if self.inbox_screen.cursor > 0 {
                    self.inbox_screen
                        .select(self.inbox_screen.cursor - 1, self.inbox.len());
                }
            }
            KeyCode::PageDown => {
                self.inbox_screen.detail_scroll =
                    self.inbox_screen.detail_scroll.saturating_add(5);
            }
            KeyCode::PageUp => {
                self.inbox_screen.detail_scroll =
                    self.inbox_screen.detail_scroll.saturating_sub(5);
            }
            KeyCode::Char('r') => {
                let Some(msg) = self.inbox.get(self.inbox_screen.cursor) else {
                    return;
                };
                self.inbox_screen.mode = InboxMode::Reply {
                    to: msg.from.clone(),
                    body: String::new(),
                };
            }
            KeyCode::Char('f') => {
                if self.inbox.get(self.inbox_screen.cursor).is_none() {
                    return;
                }
                let candidates = super::inbox_view::forward_candidates(&self.config);
                if candidates.is_empty() {
                    self.inbox_screen.status_msg = Some("no agents to forward to".to_string());
                    return;
                }
                self.inbox_screen.mode = InboxMode::Forward(super::inbox_view::ForwardState {
                    candidates,
                    selected: 0,
                    stage: ForwardStage::PickRecipient,
                    note: String::new(),
                    quoted: String::new(),
                });
            }
            KeyCode::Char('d') => {
                if self.inbox_screen.cursor < self.inbox.len() {
                    self.inbox.remove(self.inbox_screen.cursor);
                    // Content at the cursor changed (or vanished),
                    // so the existing scroll offset no longer matches
                    // the message that's now displayed.
                    self.inbox_screen.detail_scroll = 0;
                    self.inbox_screen.reclamp(self.inbox.len());
                }
            }
            _ => {}
        }
    }

    /// Deliver a user-originated message to a configured agent/automaton.
    /// Used by the Inbox view's reply/forward actions — the operator
    /// picks a recipient and the text is pushed into that chat's
    /// `user_tx` channel, mirroring the path a typed chat-prompt takes.
    /// Returns `true` when the send succeeded so callers can drop the
    /// underlying inbox message.
    fn send_user_message(&mut self, to: String, body: String) -> bool {
        let body = body.trim().to_string();
        if body.is_empty() {
            self.inbox_screen.status_msg = Some("body is empty".to_string());
            return false;
        }
        let Some(idx) = self
            .chats
            .iter()
            .position(|c| c.display_name == to && c.parent_idx.is_none())
        else {
            self.inbox_screen.status_msg = Some(format!("no agent named `{to}`"));
            return false;
        };
        let Some(tx) = self.chats[idx].user_tx.clone() else {
            self.inbox_screen.status_msg = Some(format!("agent `{to}` is not online"));
            return false;
        };
        if tx.send(body).is_err() {
            self.inbox_screen.status_msg = Some(format!("agent `{to}` is not receiving"));
            return false;
        }
        self.inbox_screen.status_msg = Some(format!("sent to {to}"));
        true
    }

    /// Commit the currently-open reply or forward. On success the
    /// underlying inbox message is removed (the user has handled it)
    /// and the overlay closes. On failure the overlay stays open so
    /// the user can fix the problem the status toast reports.
    fn submit_inbox_send(&mut self) {
        use super::inbox_view::{ForwardStage, InboxMode};
        let (to, body) = match &mut self.inbox_screen.mode {
            InboxMode::Reply { to, body } => (to.clone(), std::mem::take(body)),
            InboxMode::Forward(fwd) if matches!(fwd.stage, ForwardStage::EditBody) => {
                let Some(to) = fwd.candidates.get(fwd.selected).cloned() else {
                    self.inbox_screen.status_msg = Some("no recipient selected".to_string());
                    return;
                };
                (to, fwd.compose_body())
            }
            _ => return,
        };
        let sent = self.send_user_message(to, body);
        if sent {
            if self.inbox_screen.cursor < self.inbox.len() {
                self.inbox.remove(self.inbox_screen.cursor);
                self.inbox_screen.detail_scroll = 0;
                self.inbox_screen.reclamp(self.inbox.len());
            }
            self.inbox_screen.mode = InboxMode::List;
        }
    }

    /// Kick off one `tokio::spawn` per core that has a resolved usage
    /// provider. Each task delivers its `UsageEvent` back through
    /// `usage_event_tx`; the event loop forwards them to
    /// `on_usage_event`. Safe to call without a sender — unit tests
    /// that haven't plumbed one in simply do nothing.
    fn spawn_cores_usage_fetch(&mut self) {
        let Some(tx) = self.usage_event_tx.clone() else {
            return;
        };
        self.cores_screen.generation = self.cores_screen.generation.wrapping_add(1);
        self.cores_screen.pending = 0;
        let generation = self.cores_screen.generation;
        let client = reqwest::Client::new();
        for core in &self.config.ai_cores {
            if core.effective_usage_provider().is_none() {
                continue;
            }
            let core_id = core.id.clone();
            let core_snapshot = core.clone();
            let client = client.clone();
            let tx = tx.clone();
            self.cores_screen.pending += 1;
            tokio::spawn(async move {
                let snapshot = usage::fetch_usage(&core_snapshot, client)
                    .await
                    .unwrap_or_default();
                let _ = tx.send(UsageEvent {
                    core_id,
                    snapshot,
                    generation,
                });
            });
        }
    }

    pub(crate) fn on_usage_event(&mut self, ev: UsageEvent) {
        if ev.generation != self.cores_screen.generation {
            return;
        }
        // Defensive merge: an HTTP reply shouldn't wipe any
        // stream-derived reset fields on the same entry. In practice
        // the keys don't overlap (claudeCode has no HTTP path), but
        // encoding the invariant keeps future mixed providers safe.
        let existing = self.cores_screen.usage.remove(&ev.core_id);
        let merged = match existing {
            Some(prev) => crate::usage::UsageSnapshot {
                claude_five_hour_resets_at: prev.claude_five_hour_resets_at,
                claude_weekly_resets_at: prev.claude_weekly_resets_at,
                ..ev.snapshot
            },
            None => ev.snapshot,
        };
        self.cores_screen.usage.insert(ev.core_id, merged);
        if self.cores_screen.pending > 0 {
            self.cores_screen.pending -= 1;
        }
    }

    fn handle_key_channels(&mut self, code: KeyCode, _mods: KeyModifiers) {
        if self.channels.is_empty() {
            return;
        }
        match code {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.focus = Focus::Agents;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.selected_channel + 1 < self.channels.len() {
                    self.selected_channel += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_channel > 0 {
                    self.selected_channel -= 1;
                }
            }
            KeyCode::Enter => {
                self.active_view = ActiveView::Channel(self.selected_channel);
                self.focus = Focus::Chat;
            }
            _ => {}
        }
    }

    /// Enter the Memory view. Shows an error toast if there is no
    /// memory registry on this App (headless tests) or no banks
    /// configured.
    pub(crate) fn open_memory_view(&mut self) {
        if self.memory.is_none() {
            self.memory_screen
                .set_status("memory registry not initialised");
            return;
        }
        if self.config.memory_banks.is_empty() {
            self.memory_screen
                .set_status("no memory banks declared in config");
            return;
        }
        self.active_view = ActiveView::Memory;
        self.focus = Focus::Memory;
        self.memory_screen.banks_stage();
    }

    fn handle_key_memory(&mut self, code: KeyCode, mods: KeyModifiers) {
        // Clear transient toast on any keypress.
        self.memory_screen.status_msg = None;

        // Confirm-delete prompt intercepts input first.
        if let Some(bank) = self.memory_screen.bank() {
            if let Some(id) = bank.confirm_delete {
                match code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.dispatch_memory_delete(id);
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.confirm_delete = None;
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.confirm_delete = None;
                        }
                    }
                    _ => {}
                }
                return;
            }
        }

        // Form takes all input while open.
        if self
            .memory_screen
            .bank()
            .map(|b| b.form.is_some())
            .unwrap_or(false)
        {
            self.handle_memory_form_key(code, mods);
            return;
        }

        // Links overlay / picker.
        if self
            .memory_screen
            .bank()
            .map(|b| b.links_overlay.is_some())
            .unwrap_or(false)
        {
            self.handle_memory_links_key(code, mods);
            return;
        }

        // Detail overlay.
        if self
            .memory_screen
            .bank()
            .map(|b| b.detail.is_some())
            .unwrap_or(false)
        {
            self.handle_memory_detail_key(code);
            return;
        }

        match &self.memory_screen.stage {
            super::memory_view::MemoryStage::Banks { .. } => {
                self.handle_memory_banks_key(code);
            }
            super::memory_view::MemoryStage::Bank(_) => {
                self.handle_memory_bank_key(code, mods);
            }
        }
    }

    fn handle_memory_banks_key(&mut self, code: KeyCode) {
        let bank_count = self.config.memory_banks.len();
        let selected = match &self.memory_screen.stage {
            super::memory_view::MemoryStage::Banks { selected } => *selected,
            _ => return,
        };
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.focus = Focus::Views;
                self.active_view = ActiveView::Chat;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if selected + 1 < bank_count {
                    self.memory_screen.stage = super::memory_view::MemoryStage::Banks {
                        selected: selected + 1,
                    };
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if selected > 0 {
                    self.memory_screen.stage = super::memory_view::MemoryStage::Banks {
                        selected: selected - 1,
                    };
                }
            }
            KeyCode::Enter => {
                if let Some(bank) = self.config.memory_banks.get(selected) {
                    let id = bank.id.clone();
                    self.memory_screen.enter_bank(id);
                    self.dispatch_memory_search();
                }
            }
            _ => {}
        }
    }

    fn handle_memory_bank_key(&mut self, code: KeyCode, _mods: KeyModifiers) {
        // Search-input focus: most keys go to the text field.
        let focus = self
            .memory_screen
            .bank()
            .map(|b| b.focus)
            .unwrap_or_default();
        match (focus, code) {
            (_, KeyCode::Esc) => {
                // Back to bank list.
                self.memory_screen.banks_stage();
                return;
            }
            (super::memory_view::BankFocus::Search, KeyCode::Enter) => {
                // Submit search. Also switch focus to results so j/k
                // navigate the list immediately.
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.focus = super::memory_view::BankFocus::Results;
                }
                self.dispatch_memory_search();
                return;
            }
            (super::memory_view::BankFocus::Search, KeyCode::Down) => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.focus = super::memory_view::BankFocus::Results;
                }
                return;
            }
            (super::memory_view::BankFocus::Search, KeyCode::Backspace) => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.search.backspace();
                }
                return;
            }
            (super::memory_view::BankFocus::Search, KeyCode::Left) => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.search.move_char(-1);
                }
                return;
            }
            (super::memory_view::BankFocus::Search, KeyCode::Right) => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.search.move_char(1);
                }
                return;
            }
            (super::memory_view::BankFocus::Search, KeyCode::Char(c)) => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.search.insert(c);
                }
                return;
            }
            _ => {}
        }

        // Results focus.
        match code {
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    if !b.results.is_empty() && b.selected_result + 1 < b.results.len() {
                        b.selected_result += 1;
                    }
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    if b.selected_result > 0 {
                        b.selected_result -= 1;
                    } else {
                        b.focus = super::memory_view::BankFocus::Search;
                    }
                }
            }
            KeyCode::Char('/') => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.focus = super::memory_view::BankFocus::Search;
                }
            }
            KeyCode::Char('a') => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.form = Some(super::memory_view::NoteForm::new_add());
                }
            }
            KeyCode::Char('e') | KeyCode::Enter => {
                if let Some(hit) = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.results.get(b.selected_result).cloned())
                {
                    if matches!(code, KeyCode::Char('e')) {
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.form = Some(super::memory_view::NoteForm::new_edit(&hit));
                        }
                    } else {
                        self.open_memory_detail(hit.id);
                    }
                }
            }
            KeyCode::Char('d') => {
                if let Some(hit) = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.results.get(b.selected_result).cloned())
                {
                    if let Some(b) = self.memory_screen.bank_mut() {
                        b.confirm_delete = Some(hit.id);
                    }
                }
            }
            KeyCode::Char('l') => {
                if let Some(hit) = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.results.get(b.selected_result).cloned())
                {
                    self.open_memory_links(hit.id);
                }
            }
            KeyCode::Char('r') => {
                self.dispatch_memory_search();
            }
            _ => {}
        }
    }

    fn handle_memory_form_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        use super::memory_view::NoteField;
        let focused = match self.memory_screen.bank().and_then(|b| b.form.as_ref()) {
            Some(f) => f.focused_field,
            None => return,
        };
        match code {
            KeyCode::Esc => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.form = None;
                }
            }
            KeyCode::Tab => {
                if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                    if mods.contains(KeyModifiers::SHIFT) {
                        f.focus_prev();
                    } else {
                        f.focus_next();
                    }
                }
            }
            KeyCode::BackTab => {
                if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                    f.focus_prev();
                }
            }
            KeyCode::Enter => {
                match focused {
                    NoteField::Cancel => {
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.form = None;
                        }
                    }
                    NoteField::Ok => {
                        self.submit_memory_form();
                    }
                    NoteField::Body => {
                        // Newline in body.
                        if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut())
                        {
                            f.insert_char('\n');
                        }
                    }
                    NoteField::Title | NoteField::Tags => {
                        // Enter on single-line fields submits the form.
                        self.submit_memory_form();
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                    f.backspace();
                }
            }
            KeyCode::Left => {
                if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                    f.move_char(-1);
                }
            }
            KeyCode::Right => {
                if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                    f.move_char(1);
                }
            }
            KeyCode::Up => {
                if focused == NoteField::Body {
                    if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                        if !f.body_move_line(-1) {
                            f.focus_prev();
                        }
                    }
                }
            }
            KeyCode::Down => {
                if focused == NoteField::Body {
                    if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                        if !f.body_move_line(1) {
                            f.focus_next();
                        }
                    }
                }
            }
            KeyCode::Char(c) => {
                if !focused.is_button() {
                    if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                        f.insert_char(c);
                    }
                }
            }
            _ => {}
        }
    }

    fn handle_memory_detail_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.detail = None;
                }
            }
            KeyCode::Char('e') => {
                let hit = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.detail.as_ref())
                    .and_then(|d| d.entry.clone());
                if let Some(hit) = hit {
                    if let Some(b) = self.memory_screen.bank_mut() {
                        b.detail = None;
                        b.form = Some(super::memory_view::NoteForm::new_edit(&hit));
                    }
                }
            }
            KeyCode::Char('d') => {
                let id = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.detail.as_ref())
                    .map(|d| d.id);
                if let Some(id) = id {
                    if let Some(b) = self.memory_screen.bank_mut() {
                        b.detail = None;
                        b.confirm_delete = Some(id);
                    }
                }
            }
            KeyCode::Char('l') => {
                let id = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.detail.as_ref())
                    .map(|d| d.id);
                if let Some(id) = id {
                    if let Some(b) = self.memory_screen.bank_mut() {
                        b.detail = None;
                    }
                    self.open_memory_links(id);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(d) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.detail.as_mut())
                {
                    d.scroll = d.scroll.saturating_add(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(d) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.detail.as_mut())
                {
                    d.scroll = d.scroll.saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    fn handle_memory_links_key(&mut self, code: KeyCode, _mods: KeyModifiers) {
        // Picker mode: typing/query.
        let picker_open = self
            .memory_screen
            .bank()
            .and_then(|b| b.links_overlay.as_ref())
            .map(|o| o.picker.is_some())
            .unwrap_or(false);
        if picker_open {
            self.handle_memory_link_picker_key(code);
            return;
        }

        match code {
            KeyCode::Esc | KeyCode::Char('q') => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.links_overlay = None;
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(o) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.links_overlay.as_mut())
                {
                    o.move_cursor(1);
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(o) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.links_overlay.as_mut())
                {
                    o.move_cursor(-1);
                }
            }
            KeyCode::Char('a') | KeyCode::Char('+') => {
                if let Some(o) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.links_overlay.as_mut())
                {
                    o.picker = Some(super::memory_view::LinkPicker::default());
                }
                self.dispatch_memory_link_search();
            }
            KeyCode::Char('d') | KeyCode::Char('-') => {
                let pair = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.links_overlay.as_ref())
                    .and_then(|o| o.selected().map(|l| (o.source_id, l.peer_id)));
                if let Some((from, to)) = pair {
                    self.dispatch_memory_unlink(from, to);
                }
            }
            _ => {}
        }
    }

    fn handle_memory_link_picker_key(&mut self, code: KeyCode) {
        use super::memory_view::LinkPickerFocus;
        let focus = self
            .memory_screen
            .bank()
            .and_then(|b| b.links_overlay.as_ref())
            .and_then(|o| o.picker.as_ref())
            .map(|p| p.focus)
            .unwrap_or_default();
        match code {
            KeyCode::Esc => {
                if let Some(o) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.links_overlay.as_mut())
                {
                    o.picker = None;
                }
            }
            KeyCode::Tab => {
                if let Some(p) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.links_overlay.as_mut())
                    .and_then(|o| o.picker.as_mut())
                {
                    p.focus = match p.focus {
                        LinkPickerFocus::Query => LinkPickerFocus::Results,
                        LinkPickerFocus::Results => LinkPickerFocus::Kind,
                        LinkPickerFocus::Kind => LinkPickerFocus::Query,
                    };
                }
            }
            KeyCode::Enter => match focus {
                LinkPickerFocus::Query => {
                    self.dispatch_memory_link_search();
                    if let Some(p) = self
                        .memory_screen
                        .bank_mut()
                        .and_then(|b| b.links_overlay.as_mut())
                        .and_then(|o| o.picker.as_mut())
                    {
                        p.focus = LinkPickerFocus::Results;
                    }
                }
                LinkPickerFocus::Results | LinkPickerFocus::Kind => {
                    self.commit_memory_link();
                }
            },
            KeyCode::Down => {
                if focus == LinkPickerFocus::Results {
                    if let Some(p) = self
                        .memory_screen
                        .bank_mut()
                        .and_then(|b| b.links_overlay.as_mut())
                        .and_then(|o| o.picker.as_mut())
                    {
                        if !p.results.is_empty() && p.selected + 1 < p.results.len() {
                            p.selected += 1;
                        }
                    }
                }
            }
            KeyCode::Up => {
                if focus == LinkPickerFocus::Results {
                    if let Some(p) = self
                        .memory_screen
                        .bank_mut()
                        .and_then(|b| b.links_overlay.as_mut())
                        .and_then(|o| o.picker.as_mut())
                    {
                        if p.selected > 0 {
                            p.selected -= 1;
                        }
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(p) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.links_overlay.as_mut())
                    .and_then(|o| o.picker.as_mut())
                {
                    match focus {
                        LinkPickerFocus::Query => {
                            if p.query_cursor > 0 {
                                let n =
                                    super::tasks_view::utf8_char_len_prev(&p.query, p.query_cursor);
                                let start = p.query_cursor - n;
                                p.query.drain(start..p.query_cursor);
                                p.query_cursor = start;
                            }
                        }
                        LinkPickerFocus::Kind => {
                            if p.kind_cursor > 0 {
                                let n =
                                    super::tasks_view::utf8_char_len_prev(&p.kind, p.kind_cursor);
                                let start = p.kind_cursor - n;
                                p.kind.drain(start..p.kind_cursor);
                                p.kind_cursor = start;
                            }
                        }
                        LinkPickerFocus::Results => {}
                    }
                }
            }
            KeyCode::Char(c) => {
                if let Some(p) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.links_overlay.as_mut())
                    .and_then(|o| o.picker.as_mut())
                {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    match focus {
                        LinkPickerFocus::Query => {
                            p.query.insert_str(p.query_cursor, s);
                            p.query_cursor += s.len();
                        }
                        LinkPickerFocus::Kind => {
                            p.kind.insert_str(p.kind_cursor, s);
                            p.kind_cursor += s.len();
                        }
                        LinkPickerFocus::Results => {}
                    }
                }
            }
            _ => {}
        }
    }

    // ---- Async dispatch helpers for the Memory view ----

    fn current_memory_bank_decl(&self) -> Option<crate::config::MemoryBank> {
        let bank_id = self.memory_screen.bank()?.bank_id.clone();
        self.config.memory_bank(&bank_id).cloned()
    }

    fn dispatch_memory_search(&mut self) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            self.memory_screen.set_status("no memory registry");
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let (query, limit, bank_id) = match self.memory_screen.bank() {
            Some(b) => (
                b.search.query.trim().to_string(),
                b.search.limit.max(1),
                b.bank_id.clone(),
            ),
            None => return,
        };
        let token = self.memory_screen.begin_op("searching");
        tokio::spawn(async move {
            let opts = crate::memory::SearchOptions {
                query: if query.is_empty() { None } else { Some(query) },
                title_contains: None,
                tags: crate::memory::TagFilter::default(),
                limit,
            };
            let result = tokio::task::spawn_blocking(move || registry.search(&bank, opts)).await;
            let normalised = match result {
                Ok(Ok(hits)) => Ok(hits),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory search task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::SearchDone {
                token,
                bank_id,
                result: normalised,
            });
        });
    }

    fn open_memory_detail(&mut self, id: Uuid) {
        if let Some(b) = self.memory_screen.bank_mut() {
            b.detail = Some(super::memory_view::NoteDetail::new(id));
        }
        self.dispatch_memory_entry_load(id);
    }

    fn dispatch_memory_entry_load(&mut self, id: Uuid) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let token = self.memory_screen.begin_op("loading entry");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let entry = registry.get_entry(&bank, id)?;
                let Some(entry) = entry else {
                    return Ok::<_, crate::memory::MemoryError>(None);
                };
                let empty = crate::memory::TagFilter::default();
                let out = registry.links(&bank, id, crate::memory::LinkDirection::Out, &empty)?;
                let inc = registry.links(&bank, id, crate::memory::LinkDirection::In, &empty)?;
                Ok(Some((entry, out, inc)))
            })
            .await;
            let normalised = match result {
                Ok(Ok(payload)) => Ok(payload),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory load task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::EntryLoaded {
                token,
                bank_id,
                id,
                result: normalised,
            });
        });
    }

    fn open_memory_links(&mut self, id: Uuid) {
        if let Some(b) = self.memory_screen.bank_mut() {
            b.links_overlay = Some(super::memory_view::LinksOverlay::new(id, Vec::new()));
        }
        self.dispatch_memory_link_refresh(id);
    }

    fn dispatch_memory_link_refresh(&mut self, id: Uuid) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let token = self.memory_screen.begin_op("loading links");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                let entry = registry.get_entry(&bank, id)?;
                let Some(entry) = entry else {
                    return Ok::<_, crate::memory::MemoryError>(None);
                };
                let empty = crate::memory::TagFilter::default();
                let out = registry.links(&bank, id, crate::memory::LinkDirection::Out, &empty)?;
                let inc = registry.links(&bank, id, crate::memory::LinkDirection::In, &empty)?;
                Ok(Some((entry, out, inc)))
            })
            .await;
            let normalised = match result {
                Ok(Ok(payload)) => Ok(payload),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory links task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::EntryLoaded {
                token,
                bank_id,
                id,
                result: normalised,
            });
        });
    }

    fn submit_memory_form(&mut self) {
        let validated = self
            .memory_screen
            .bank_mut()
            .and_then(|b| b.form.as_mut())
            .map(|f| f.validate().is_ok())
            .unwrap_or(false);
        if !validated {
            return;
        }
        let (kind, title, body, tags) =
            match self.memory_screen.bank().and_then(|b| b.form.as_ref()) {
                Some(f) => (
                    f.kind,
                    if f.title.trim().is_empty() {
                        None
                    } else {
                        Some(f.title.trim().to_string())
                    },
                    f.body.clone(),
                    f.parsed_tags(),
                ),
                None => return,
            };
        match kind {
            super::memory_view::NoteFormKind::Add => self.dispatch_memory_store(body, title, tags),
            super::memory_view::NoteFormKind::Edit(id) => {
                self.dispatch_memory_update(id, body, title, tags)
            }
        }
    }

    fn dispatch_memory_store(&mut self, body: String, title: Option<String>, tags: Vec<String>) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let token = self.memory_screen.begin_op("saving note");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                registry.store(&bank, &body, title.as_deref(), &tags)
            })
            .await;
            let normalised = match result {
                Ok(Ok(id)) => Ok(id),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory store task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::StoreDone {
                token,
                bank_id,
                result: normalised,
            });
        });
    }

    fn dispatch_memory_update(
        &mut self,
        id: Uuid,
        body: String,
        title: Option<String>,
        tags: Vec<String>,
    ) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let token = self.memory_screen.begin_op("updating note");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                registry.update_entry(&bank, id, &body, title.as_deref(), &tags)
            })
            .await;
            let normalised = match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory update task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::UpdateDone {
                token,
                bank_id,
                id,
                result: normalised,
            });
        });
    }

    fn dispatch_memory_delete(&mut self, id: Uuid) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let token = self.memory_screen.begin_op("removing note");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || registry.delete_entry(&bank, id)).await;
            let normalised = match result {
                Ok(Ok(removed)) => Ok(removed),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory delete task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::DeleteDone {
                token,
                bank_id,
                id,
                result: normalised,
            });
        });
    }

    fn dispatch_memory_link_search(&mut self) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let Some(p) = self
            .memory_screen
            .bank()
            .and_then(|b| b.links_overlay.as_ref())
            .and_then(|o| o.picker.as_ref())
        else {
            return;
        };
        let query = p.query.trim().to_string();
        let token = self.memory_screen.begin_op("searching targets");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let opts = crate::memory::SearchOptions {
                query: if query.is_empty() { None } else { Some(query) },
                title_contains: None,
                tags: crate::memory::TagFilter::default(),
                limit: 20,
            };
            let result = tokio::task::spawn_blocking(move || registry.search(&bank, opts)).await;
            let normalised = match result {
                Ok(Ok(hits)) => Ok(hits),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory link search task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::LinkSearchDone {
                token,
                bank_id,
                result: normalised,
            });
        });
    }

    fn commit_memory_link(&mut self) {
        let (from, to, kind) = match self
            .memory_screen
            .bank()
            .and_then(|b| b.links_overlay.as_ref())
            .and_then(|o| {
                let p = o.picker.as_ref()?;
                let target = p.results.get(p.selected)?;
                Some((
                    o.source_id,
                    target.id,
                    if p.kind.trim().is_empty() {
                        None
                    } else {
                        Some(p.kind.trim().to_string())
                    },
                ))
            }) {
            Some(triple) => triple,
            None => return,
        };
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let token = self.memory_screen.begin_op("linking");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                registry.link(&bank, from, to, kind.as_deref())
            })
            .await;
            let normalised = match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory link task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::LinkDone {
                token,
                bank_id,
                from,
                to,
                result: normalised,
            });
        });
    }

    fn dispatch_memory_unlink(&mut self, from: Uuid, to: Uuid) {
        let (Some(registry), Some(bank)) = (self.memory.clone(), self.current_memory_bank_decl())
        else {
            return;
        };
        let Some(tx) = self.memory_event_tx.clone() else {
            return;
        };
        let token = self.memory_screen.begin_op("unlinking");
        let bank_id = bank.id.clone();
        tokio::spawn(async move {
            let result =
                tokio::task::spawn_blocking(move || registry.unlink(&bank, from, to)).await;
            let normalised = match result {
                Ok(Ok(removed)) => Ok(removed),
                Ok(Err(e)) => Err(e.to_string()),
                Err(e) => Err(format!("memory unlink task: {e}")),
            };
            let _ = tx.send(super::memory_view::MemoryEvent::UnlinkDone {
                token,
                bank_id,
                from,
                to,
                result: normalised,
            });
        });
    }

    /// Reducer for `MemoryEvent`s delivered on the background channel.
    /// Called from the event-loop select arm. Drops any event whose
    /// bank or token doesn't match the current state so stale replies
    /// (after the user moved on) are ignored.
    pub(crate) fn on_memory_event(&mut self, event: super::memory_view::MemoryEvent) {
        use super::memory_view::MemoryEvent;
        // We don't gate strictly on `token == current_token` because
        // some ops (search + link-refresh) intentionally run in
        // sequence with the reply. Instead, reject only when the bank
        // has changed. `clear_pending` runs on every terminal event
        // so the spinner disappears.
        let current_bank = self.memory_screen.bank().map(|b| b.bank_id.clone());
        match event {
            MemoryEvent::SearchDone {
                bank_id, result, ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(hits) => {
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.results = hits;
                            if b.selected_result >= b.results.len() {
                                b.selected_result = 0;
                            }
                        }
                    }
                    Err(e) => {
                        self.memory_screen.set_status(format!("search failed: {e}"));
                    }
                }
            }
            MemoryEvent::EntryLoaded {
                bank_id,
                id,
                result,
                ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(Some((hit, out, inc))) => {
                        if let Some(b) = self.memory_screen.bank_mut() {
                            if let Some(detail) = b.detail.as_mut() {
                                if detail.id == id {
                                    detail.entry = Some(hit.clone());
                                    detail.out_links = out.clone();
                                    detail.in_links = inc.clone();
                                }
                            }
                            if let Some(overlay) = b.links_overlay.as_mut() {
                                if overlay.source_id == id {
                                    overlay.existing = out;
                                    if overlay.cursor >= overlay.existing.len() {
                                        overlay.cursor = 0;
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        self.memory_screen.set_status("entry not found");
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.detail = None;
                            b.links_overlay = None;
                        }
                    }
                    Err(e) => {
                        self.memory_screen.set_status(format!("load failed: {e}"));
                    }
                }
            }
            MemoryEvent::StoreDone {
                bank_id, result, ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(id) => {
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.form = None;
                        }
                        self.memory_screen
                            .set_status(format!("saved {}", short_uuid(id)));
                        self.dispatch_memory_search();
                    }
                    Err(e) => {
                        if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut())
                        {
                            f.error = Some(e.clone());
                        }
                        self.memory_screen.set_status(format!("save failed: {e}"));
                    }
                }
            }
            MemoryEvent::UpdateDone {
                bank_id,
                id,
                result,
                ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(()) => {
                        if let Some(b) = self.memory_screen.bank_mut() {
                            b.form = None;
                        }
                        self.memory_screen
                            .set_status(format!("updated {}", short_uuid(id)));
                        self.dispatch_memory_search();
                    }
                    Err(e) => {
                        if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut())
                        {
                            f.error = Some(e.clone());
                        }
                        self.memory_screen.set_status(format!("update failed: {e}"));
                    }
                }
            }
            MemoryEvent::DeleteDone {
                bank_id,
                id,
                result,
                ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(true) => {
                        self.memory_screen
                            .set_status(format!("removed {}", short_uuid(id)));
                        self.dispatch_memory_search();
                    }
                    Ok(false) => {
                        self.memory_screen
                            .set_status(format!("{} already gone", short_uuid(id)));
                    }
                    Err(e) => {
                        self.memory_screen.set_status(format!("delete failed: {e}"));
                    }
                }
            }
            MemoryEvent::LinkDone {
                bank_id,
                from,
                result,
                ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(()) => {
                        if let Some(o) = self
                            .memory_screen
                            .bank_mut()
                            .and_then(|b| b.links_overlay.as_mut())
                        {
                            o.picker = None;
                        }
                        self.memory_screen.set_status("linked");
                        self.dispatch_memory_link_refresh(from);
                    }
                    Err(e) => {
                        self.memory_screen.set_status(format!("link failed: {e}"));
                    }
                }
            }
            MemoryEvent::UnlinkDone {
                bank_id,
                from,
                result,
                ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(true) => {
                        self.memory_screen.set_status("unlinked");
                        self.dispatch_memory_link_refresh(from);
                    }
                    Ok(false) => {
                        self.memory_screen.set_status("no link to remove");
                    }
                    Err(e) => {
                        self.memory_screen.set_status(format!("unlink failed: {e}"));
                    }
                }
            }
            MemoryEvent::LinkSearchDone {
                bank_id, result, ..
            } => {
                if current_bank.as_deref() != Some(&bank_id) {
                    return;
                }
                self.memory_screen.clear_pending();
                match result {
                    Ok(hits) => {
                        if let Some(p) = self
                            .memory_screen
                            .bank_mut()
                            .and_then(|b| b.links_overlay.as_mut())
                            .and_then(|o| o.picker.as_mut())
                        {
                            p.results = hits;
                            if p.selected >= p.results.len() {
                                p.selected = 0;
                            }
                        }
                    }
                    Err(e) => {
                        self.memory_screen
                            .set_status(format!("link search failed: {e}"));
                    }
                }
            }
        }
    }

    fn handle_key_tasks(&mut self, code: KeyCode, _mods: KeyModifiers) {
        // Clear transient status messages the moment the user types.
        self.tasks_screen.status_msg = None;

        // Prompt takes priority over the form so the comment-add /
        // edit / delete prompts can overlay an open edit form via the
        // chat input bar.
        if self.tasks_screen.prompt.is_some() {
            self.handle_task_prompt_key(code);
            return;
        }
        // Modal form takes all input.
        if self.tasks_screen.form.is_some() {
            self.handle_task_form_key(code);
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
                // Enter on a row opens the edit form (the only task-
                // detail surface). The form lays out title, description,
                // assignee, status, blocked-by plus collapsible read-
                // only TODOs and Comments sections.
                if let Some(id) = self.tasks_screen.selected_task_id(&rows) {
                    if let Some(t) = self.tasks.get(id) {
                        self.tasks_screen.open_edit(t);
                        self.inject_assignee_options();
                    }
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
        let on_todos = focused == FormField::Todos;
        let on_comments = focused == FormField::Comments;
        let on_section = on_todos || on_comments;

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
            // Tab/Down → next control; Shift-Tab/Up → previous. The
            // Todos / Comments sections claim j/k for inner scroll
            // when expanded, but always yield Tab and Up/Down to the
            // form's own focus walk so the user can leave the
            // section without first collapsing it.
            KeyCode::Tab | KeyCode::Down => form.focus_next(),
            KeyCode::BackTab | KeyCode::Up => form.focus_prev(),
            KeyCode::Enter => match focused {
                FormField::Ok => self.submit_task_form(),
                FormField::Cancel => self.tasks_screen.close_modal(),
                FormField::Todos => form.toggle_todos_collapsed(),
                FormField::Comments => form.toggle_comments_collapsed(),
                _ if on_dropdown => form.open_picker(),
                _ if on_blocked_by => self.open_blocked_by_picker_for_form(),
                _ => self.submit_task_form(),
            },
            // Cursor navigation inside an expanded section. The
            // renderer auto-tracks scroll to keep the cursor in view;
            // we don't have the live task here so we drop the cursor
            // walk down to the helpers below which consult `self.tasks`.
            KeyCode::Char('j') if on_section => {
                self.move_form_section_cursor(1);
            }
            KeyCode::Char('k') if on_section => {
                self.move_form_section_cursor(-1);
            }
            // Space on Todos toggles the cursor-selected todo between
            // Pending and Completed. Mirrors a checkbox click in the
            // mouse handler.
            KeyCode::Char(' ') if on_todos && !form.todos_collapsed => {
                self.toggle_form_selected_todo();
            }
            // `a` on the expanded TODOs section opens an inline
            // add-todo prompt at the chat input bar; the form stays
            // visible behind it.
            KeyCode::Char('a') if on_todos && !form.todos_collapsed => {
                self.open_form_add_todo_prompt();
            }
            // `d` opens the y/n confirmation prompt for the cursor-
            // selected todo. Confirming removes it from the store;
            // declining or Esc returns control to the form intact.
            KeyCode::Char('d') if on_todos && !form.todos_collapsed => {
                self.open_form_delete_todo_prompt();
            }
            // Comment management while focused + expanded: a/e/d
            // pipe into TasksPrompt so the user can compose without
            // leaving the form.
            KeyCode::Char('a') if on_comments && !form.comments_collapsed => {
                self.open_form_add_comment_prompt();
            }
            KeyCode::Char('e') if on_comments && !form.comments_collapsed => {
                self.open_form_edit_comment_prompt();
            }
            KeyCode::Char('d') if on_comments && !form.comments_collapsed => {
                self.open_form_delete_comment_prompt();
            }
            KeyCode::Backspace if !on_dropdown && !on_button && !on_blocked_by && !on_section => {
                form.field_mut(focused).pop();
            }
            KeyCode::Char(c) if !on_dropdown && !on_button && !on_blocked_by && !on_section => {
                form.field_mut(focused).push(c);
            }
            _ => {}
        }
    }

    /// Step the cursor inside the currently-focused expanded section
    /// by `delta` rows. Reads the live task to compute the upper
    /// bound. No-op if no form is open or the section is collapsed.
    fn move_form_section_cursor(&mut self, delta: i32) {
        let Some(form) = self.tasks_screen.form.as_ref() else {
            return;
        };
        let TaskFormKind::Edit(task_id) = form.kind else {
            return;
        };
        let focused = form.focused_field;
        let Some(task) = self.tasks.get(task_id) else {
            return;
        };
        let len = match focused {
            FormField::Todos if !form.todos_collapsed => task.todos.len(),
            FormField::Comments if !form.comments_collapsed => task.comments.len(),
            _ => return,
        };
        let Some(form) = self.tasks_screen.form.as_mut() else {
            return;
        };
        match focused {
            FormField::Todos => form.move_todos_cursor(delta, len),
            FormField::Comments => form.move_comments_cursor(delta, len),
            _ => {}
        }
    }

    /// Toggle the cursor-selected TODO between Pending and Completed.
    /// Stays a no-op if the section is collapsed, the form is closed,
    /// or the cursor is past the end.
    fn toggle_form_selected_todo(&mut self) {
        let Some(form) = self.tasks_screen.form.as_ref() else {
            return;
        };
        let TaskFormKind::Edit(task_id) = form.kind else {
            return;
        };
        if form.todos_collapsed {
            return;
        }
        let cursor = form.todos_cursor;
        let Some(task) = self.tasks.get(task_id) else {
            return;
        };
        let Some(td) = task.todos.get(cursor) else {
            return;
        };
        let next_status = match td.status {
            crate::tasks::TodoStatus::Completed => crate::tasks::TodoStatus::Pending,
            _ => crate::tasks::TodoStatus::Completed,
        };
        let todo_id = td.id;
        let _ = self.tasks.update_todo(task_id, todo_id, None, Some(next_status));
    }

    fn open_form_add_todo_prompt(&mut self) {
        let Some(form) = self.tasks_screen.form.as_ref() else {
            return;
        };
        let TaskFormKind::Edit(task_id) = form.kind else {
            return;
        };
        self.tasks_screen.prompt = Some(TasksPrompt::AddTodo {
            task_id,
            body: String::new(),
        });
    }

    fn open_form_delete_todo_prompt(&mut self) {
        let Some(form) = self.tasks_screen.form.as_ref() else {
            return;
        };
        let TaskFormKind::Edit(task_id) = form.kind else {
            return;
        };
        let cursor = form.todos_cursor;
        let Some(task) = self.tasks.get(task_id) else {
            return;
        };
        let Some(td) = task.todos.get(cursor) else {
            return;
        };
        let todo_id = td.id;
        self.tasks_screen.prompt = Some(TasksPrompt::ConfirmDeleteTodo {
            task_id,
            todo_id,
            buffer: String::new(),
        });
    }

    fn open_form_add_comment_prompt(&mut self) {
        let Some(form) = self.tasks_screen.form.as_ref() else {
            return;
        };
        let TaskFormKind::Edit(task_id) = form.kind else {
            return;
        };
        self.tasks_screen.prompt = Some(TasksPrompt::AddComment {
            task_id,
            body: String::new(),
        });
    }

    fn open_form_edit_comment_prompt(&mut self) {
        let Some(form) = self.tasks_screen.form.as_ref() else {
            return;
        };
        let TaskFormKind::Edit(task_id) = form.kind else {
            return;
        };
        let cursor = form.comments_cursor;
        let Some(task) = self.tasks.get(task_id) else {
            return;
        };
        let Some(c) = task.comments.get(cursor) else {
            return;
        };
        let comment_id = c.id;
        let body = c.body.clone();
        self.tasks_screen.prompt = Some(TasksPrompt::EditComment {
            task_id,
            comment_id,
            body,
        });
    }

    fn open_form_delete_comment_prompt(&mut self) {
        let Some(form) = self.tasks_screen.form.as_ref() else {
            return;
        };
        let TaskFormKind::Edit(task_id) = form.kind else {
            return;
        };
        let cursor = form.comments_cursor;
        let Some(task) = self.tasks.get(task_id) else {
            return;
        };
        let Some(c) = task.comments.get(cursor) else {
            return;
        };
        let comment_id = c.id;
        self.tasks_screen.prompt = Some(TasksPrompt::ConfirmDeleteComment {
            task_id,
            comment_id,
            buffer: String::new(),
        });
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
    /// form from the team's visible chats, declared groups, and the
    /// reserved `user` recipient. Called immediately after `open_add`
    /// / `open_edit`. Stored values stay raw; kind-aware glyphs are
    /// applied at render time via
    /// [`super::tasks_view::decorate_assignee_label`].
    fn inject_assignee_options(&mut self) {
        let mut options: Vec<String> = vec![String::new()];
        for c in &self.chats {
            // Hibernated workers are hidden from the agent list, but
            // they can still be valid task assignees — include them
            // so a hand-off mid-hibernation stays possible.
            options.push(c.display_name.clone());
        }
        for g in self.config.groups() {
            options.push(g.to_string());
        }
        options.push(crate::config::USER_INBOX_ID.to_string());
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
            TasksPrompt::AddTodo { task_id, body } => match code {
                // Cancel: drop the prompt only — keep the form open.
                KeyCode::Esc => {
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Enter => {
                    if !body.trim().is_empty() {
                        let id = *task_id;
                        let body = std::mem::take(body);
                        match self.tasks.add_todo(id, body, OPERATOR.to_string()) {
                            Ok(td) => {
                                let new_id = td.id;
                                self.tasks_screen.status_msg = Some("todo added".into());
                                self.tasks_screen.prompt = None;
                                // Bump the form's cursor onto the
                                // freshly-added todo so Space toggles
                                // it without an extra j-walk.
                                if let (Some(form), Some(t)) = (
                                    self.tasks_screen.form.as_mut(),
                                    self.tasks.get(id),
                                ) {
                                    if let Some(idx) =
                                        t.todos.iter().position(|td| td.id == new_id)
                                    {
                                        form.todos_cursor = idx;
                                    }
                                }
                            }
                            Err(e) => {
                                self.tasks_screen.status_msg = Some(format!("error: {e}"));
                                self.tasks_screen.prompt = None;
                            }
                        }
                    } else {
                        self.tasks_screen.prompt = None;
                    }
                }
                KeyCode::Backspace => {
                    body.pop();
                }
                KeyCode::Char(c) => body.push(c),
                _ => {}
            },
            TasksPrompt::ConfirmDeleteTodo {
                task_id,
                todo_id,
                buffer,
            } => match code {
                KeyCode::Esc => {
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let task_id = *task_id;
                    let todo_id = *todo_id;
                    match self.tasks.remove_todo(task_id, todo_id) {
                        Ok(_) => {
                            self.tasks_screen.status_msg = Some("todo removed".into());
                        }
                        Err(e) => {
                            self.tasks_screen.status_msg = Some(format!("error: {e}"));
                        }
                    }
                    // Snap the cursor back into range so a follow-up
                    // Space doesn't index past the new end.
                    if let (Some(form), Some(t)) =
                        (self.tasks_screen.form.as_mut(), self.tasks.get(task_id))
                    {
                        let len = t.todos.len();
                        if len == 0 {
                            form.todos_cursor = 0;
                        } else if form.todos_cursor >= len {
                            form.todos_cursor = len - 1;
                        }
                    }
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Char(c) => buffer.push(c),
                KeyCode::Backspace => {
                    buffer.pop();
                }
                _ => {}
            },
            TasksPrompt::AddComment { task_id, body } => match code {
                // Cancel: drop the prompt only — keep the form open.
                KeyCode::Esc => {
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Enter => {
                    if !body.trim().is_empty() {
                        let id = *task_id;
                        let body = std::mem::take(body);
                        match self.tasks.add_comment(id, body, OPERATOR.to_string()) {
                            Ok(c) => {
                                let new_id = c.id;
                                self.tasks_screen.status_msg = Some("comment added".into());
                                // Drop the prompt and bump the cursor
                                // to the freshly-added row.
                                self.tasks_screen.prompt = None;
                                if let (Some(form), Some(t)) = (
                                    self.tasks_screen.form.as_mut(),
                                    self.tasks.get(id),
                                ) {
                                    if let Some(idx) =
                                        t.comments.iter().position(|c| c.id == new_id)
                                    {
                                        form.comments_cursor = idx;
                                    }
                                }
                            }
                            Err(e) => {
                                self.tasks_screen.status_msg = Some(format!("error: {e}"));
                                self.tasks_screen.prompt = None;
                            }
                        }
                    } else {
                        // Empty body: just dismiss the prompt.
                        self.tasks_screen.prompt = None;
                    }
                }
                KeyCode::Backspace => {
                    body.pop();
                }
                KeyCode::Char(c) => body.push(c),
                _ => {}
            },
            TasksPrompt::EditComment {
                task_id,
                comment_id,
                body,
            } => match code {
                KeyCode::Esc => {
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Enter => {
                    if !body.trim().is_empty() {
                        let task_id = *task_id;
                        let comment_id = *comment_id;
                        let body = std::mem::take(body);
                        match self.tasks.update_comment(task_id, comment_id, body) {
                            Ok(_) => {
                                self.tasks_screen.status_msg = Some("comment updated".into());
                            }
                            Err(e) => {
                                self.tasks_screen.status_msg = Some(format!("error: {e}"));
                            }
                        }
                    }
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Backspace => {
                    body.pop();
                }
                KeyCode::Char(c) => body.push(c),
                _ => {}
            },
            TasksPrompt::ConfirmDeleteComment {
                task_id,
                comment_id,
                buffer,
            } => match code {
                KeyCode::Esc => {
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    let task_id = *task_id;
                    let comment_id = *comment_id;
                    match self.tasks.remove_comment(task_id, comment_id) {
                        Ok(_) => {
                            self.tasks_screen.status_msg = Some("comment removed".into());
                        }
                        Err(e) => {
                            self.tasks_screen.status_msg = Some(format!("error: {e}"));
                        }
                    }
                    // Snap the cursor back into range.
                    if let (Some(form), Some(t)) = (
                        self.tasks_screen.form.as_mut(),
                        self.tasks.get(task_id),
                    ) {
                        let len = t.comments.len();
                        if len == 0 {
                            form.comments_cursor = 0;
                        } else if form.comments_cursor >= len {
                            form.comments_cursor = len - 1;
                        }
                    }
                    self.tasks_screen.prompt = None;
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    self.tasks_screen.prompt = None;
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
        // Route to the channel-input path if the main area is
        // showing a channel rather than an agent chat.
        if let ActiveView::Channel(idx) = self.active_view {
            self.handle_key_channel_chat(idx, code, mods);
            return;
        }

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

    /// Channel-input key handler. Mirrors `handle_key_chat` for the
    /// per-channel compose buffer: wrap-aware up/down, row-start/end,
    /// Shift+Enter newline. No Alt+block selection since channel
    /// transcripts don't have collapsible blocks yet.
    fn handle_key_channel_chat(&mut self, idx: usize, code: KeyCode, mods: KeyModifiers) {
        let is_newline = (matches!(code, KeyCode::Enter) && mods.contains(KeyModifiers::SHIFT))
            || (matches!(code, KeyCode::Char('j')) && mods.contains(KeyModifiers::CONTROL));
        if is_newline {
            if let Some(ch) = self.channels.get_mut(idx) {
                input_insert(ch, '\n');
            }
            return;
        }
        match code {
            KeyCode::Esc => self.focus = Focus::Agents,
            KeyCode::Enter => self.submit_channel_input(idx),
            KeyCode::Backspace => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_backspace(ch);
                }
            }
            KeyCode::Left => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_move_char(ch, -1);
                }
            }
            KeyCode::Right => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_move_char(ch, 1);
                }
            }
            KeyCode::Up => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_move_line(ch, -1);
                }
            }
            KeyCode::Down => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_move_line(ch, 1);
                }
            }
            KeyCode::Home => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_move_to_row_start(ch);
                }
            }
            KeyCode::End => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_move_to_row_end(ch);
                }
            }
            KeyCode::Char(c) => {
                if let Some(ch) = self.channels.get_mut(idx) {
                    input_insert(ch, c);
                }
            }
            _ => {}
        }
    }

    /// Consume the channel's compose buffer and broadcast it (or
    /// dispatch a slash command). Mirrors `submit_input` for agents.
    fn submit_channel_input(&mut self, idx: usize) {
        let content = {
            let Some(ch) = self.channels.get_mut(idx) else {
                return;
            };
            let trimmed = ch.input.trim();
            if trimmed.is_empty() {
                return;
            }
            let content = trimmed.to_string();
            ch.input.clear();
            ch.input_cursor = 0;
            ch.input_scroll = 0;
            content
        };
        self.handle_local_channel_input(idx, content);
    }

    /// Handle user-typed input on a channel: dispatch slash commands
    /// (`/reset` clears the visible blocks; other commands emit a
    /// helpful system note), or broadcast the message to all other
    /// members.
    pub(crate) fn handle_local_channel_input(&mut self, idx: usize, content: String) {
        let trimmed = content.trim();
        if trimmed.is_empty() {
            return;
        }
        let content = trimmed.to_string();

        if content.starts_with('/') {
            if let Some(outcome) = super::commands::dispatch(&content) {
                self.apply_channel_command_for(idx, outcome);
            }
            return;
        }

        // Copy the channel id out because the broadcast path needs
        // `&mut self` and we can't hold a reference into `channels`.
        let Some(channel_id) = self.channels.get(idx).map(|c| c.id.clone()) else {
            return;
        };
        let (reply_tx, _reply_rx) = tokio::sync::oneshot::channel();
        self.broadcast_to_channel(ChannelSender::User, channel_id, content, reply_tx);
    }

    /// Apply a parsed slash command in the channel's context.
    /// `/reset` wipes the in-memory display log (preserving the
    /// welcome banner if any) without touching the disk JSONL.
    /// `/compact` is a no-op with a friendly system note — there is
    /// no agent session here to compact. Unknown commands get the
    /// same hint string as the agent path.
    fn apply_channel_command_for(&mut self, idx: usize, outcome: super::commands::CommandOutcome) {
        use super::commands::CommandOutcome;
        // Handle the cases that need `&mut self` (not just a single
        // channel) before taking the per-channel borrow.
        if matches!(outcome, CommandOutcome::Shutdown) {
            // A channel pane has no owning chat to annotate, so skip
            // the inline "shutting down" note; the user sees the app
            // exit instead.
            self.begin_shutdown(None);
            return;
        }
        let Some(ch) = self.channels.get_mut(idx) else {
            return;
        };
        match outcome {
            CommandOutcome::Quit => self.should_quit = true,
            CommandOutcome::Shutdown => unreachable!("handled above"),
            CommandOutcome::Message(text) => {
                ch.blocks.push(ChatBlock::system_info(text));
            }
            CommandOutcome::Reset => {
                // Keep only the welcome banner (which always lives
                // at index 0 if present). Disk log is NOT truncated
                // — the operator is clearing their own screen, not
                // forgetting history.
                let keep_welcome = matches!(
                    ch.blocks.first(),
                    Some(ChatBlock::ChannelMessage {
                        is_welcome: true,
                        ..
                    })
                );
                if keep_welcome {
                    ch.blocks.truncate(1);
                } else {
                    ch.blocks.clear();
                }
                ch.blocks.push(ChatBlock::system_info(
                    "channel cleared (disk log preserved)",
                ));
                ch.selected_block = None;
                ch.sticky_bottom = true;
            }
            CommandOutcome::Compact => {
                ch.blocks.push(ChatBlock::system_info(
                    "/compact is a no-op in channels — there is no agent session here to compact",
                ));
            }
            CommandOutcome::Reload => {
                let result = self.reload();
                if let Some(ch) = self.channels.get_mut(idx) {
                    let block = match result {
                        Ok(summary) => ChatBlock::system_info(summary),
                        Err(e) => ChatBlock::system_error(format!("reload failed: {e}")),
                    };
                    ch.blocks.push(block);
                }
            }
            CommandOutcome::Stop | CommandOutcome::Sleep | CommandOutcome::Awake => {
                // These commands target a single agent's run_core
                // loop. A channel pane has no agent of its own, so
                // there's nothing to stop or sleep here.
                ch.blocks.push(ChatBlock::system_info(
                    "/stop, /sleep, /awake target agents — run them from an agent's pane",
                ));
            }
            CommandOutcome::RunWorkflow { workflow_id } => {
                let result = self.start_workflow_with_defaults(&workflow_id);
                let ch = self.channels.get_mut(idx).expect("checked above");
                match result {
                    Ok(id) => ch.blocks.push(ChatBlock::system_info(format!(
                        "workflow `{workflow_id}` started as instance #{id}"
                    ))),
                    Err(msg) => ch.blocks.push(ChatBlock::system_warn(msg)),
                }
            }
            CommandOutcome::CreateMeeting { id } => {
                let result = self.create_meeting(id.clone());
                match result {
                    Ok(new_idx) => {
                        self.selected_channel = new_idx;
                        self.active_view = ActiveView::Channel(new_idx);
                    }
                    Err(msg) => {
                        if let Some(ch) = self.channels.get_mut(idx) {
                            ch.blocks.push(ChatBlock::system_warn(msg));
                        }
                    }
                }
            }
            CommandOutcome::AddAgent { agent } => {
                let channel_id = ch.id.clone();
                if let Err(msg) = self.channel_add_member_by_id(&channel_id, &agent) {
                    if let Some(ch) = self.channels.get_mut(idx) {
                        ch.blocks.push(ChatBlock::system_warn(msg));
                    }
                }
            }
            CommandOutcome::KickAgent { agent } => {
                let channel_id = ch.id.clone();
                if let Err(msg) = self.channel_kick_member_by_id(&channel_id, &agent) {
                    if let Some(ch) = self.channels.get_mut(idx) {
                        ch.blocks.push(ChatBlock::system_warn(msg));
                    }
                }
            }
            CommandOutcome::LeaveChannel => {
                let channel_id = ch.id.clone();
                if let Err(msg) = self.channel_leave_by_id(&channel_id) {
                    if let Some(ch) = self.channels.get_mut(idx) {
                        ch.blocks.push(ChatBlock::system_warn(msg));
                    }
                }
            }
            CommandOutcome::AuthLogin { provider, name } => {
                let msg = handle_auth_login(&provider, &name);
                ch.blocks.push(ChatBlock::system_info(msg));
            }
            CommandOutcome::AuthLogout { name } => {
                let msg = handle_auth_logout(&name);
                ch.blocks.push(ChatBlock::system_info(msg));
            }
            CommandOutcome::AuthList => {
                let msg = handle_auth_list();
                ch.blocks.push(ChatBlock::system_info(msg));
            }
            CommandOutcome::AuthStatus { name } => {
                let msg = handle_auth_status(name.as_deref());
                ch.blocks.push(ChatBlock::system_info(msg));
            }
            CommandOutcome::Unknown { name, hint } => {
                ch.blocks.push(ChatBlock::system_warn(format!(
                    "unknown command: /{name} — {hint}"
                )));
            }
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

                // 0c. Memory-view modals (form / detail / links /
                //     confirm-delete) capture all clicks. Clicks
                //     outside the modal dismiss it, clicks inside
                //     route to the specific handler.
                if self.active_view == ActiveView::Memory && self.memory_screen.has_active_modal() {
                    self.handle_mouse_memory_modal(pos);
                    return;
                }

                // 0d. Inbox hotspots (toolbar buttons, row selection,
                //     reply/forward modal buttons and recipient rows).
                //     Consulted before the generic sidebar/main hit
                //     tests so clicks on our own buttons win. When the
                //     click lands inside the inbox view but not on a
                //     hotspot it falls through to focus-grab below.
                if self.active_view == ActiveView::Inbox && self.handle_mouse_inbox(pos) {
                    return;
                }

                // 0e. Cron hotspots (toolbar new button, row select +
                //     inline buttons, add/edit modal fields and
                //     buttons, confirm-delete buttons). When a modal
                //     is open, clicks outside it are swallowed to
                //     prevent accidental dismissal via sidebar click.
                if self.active_view == ActiveView::Cron && self.handle_mouse_cron(pos) {
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

                // 1b. Workflow form: Run button, picker, MultiSelect
                //     checkbox, or whole-field click. Checked before
                //     sidebar hit-tests so form clicks win over any
                //     overlapping region. Clicks outside the form's
                //     rects fall through so the user can click the
                //     sidebar to switch views/workflows without having
                //     to Esc first — the form is not modal.
                if matches!(self.active_view, ActiveView::WorkflowForm { .. })
                    && self.handle_mouse_workflow_form(pos)
                {
                    return;
                }

                // 2. Views list: clicking any entry both focuses
                //    the panel and activates that view (same as
                //    Enter on the keyboard).
                if let Some(rect) = self.layout.views_list_inner {
                    if rect_contains(rect, pos) {
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        let entries = self.view_entries();
                        if row_in_list < entries.len() {
                            self.selected_view = row_in_list;
                            // Dispatch the selected view.
                            match entries[row_in_list] {
                                "Tasks" => {
                                    self.active_view = ActiveView::Tasks;
                                    self.focus = Focus::Tasks;
                                    let rows = TasksScreen::rows(&self.tasks);
                                    self.tasks_screen.reclamp(&rows);
                                }
                                "Memory" => {
                                    self.open_memory_view();
                                }
                                "Cores" => {
                                    self.open_cores_view();
                                }
                                "Inbox" => {
                                    self.open_inbox_view();
                                }
                                "Cron" => {
                                    self.open_cron_view();
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

                // 2b. Channels list: clicking a channel header opens
                //     that channel (same as Enter). When a channel is
                //     active its members are nested beneath it;
                //     clicking a member row switches to the agent's
                //     chat and highlights it in the Agents list.
                //     Clicks on empty rows just focus the panel.
                if let Some(rect) = self.layout.channels_list_inner {
                    if rect_contains(rect, pos) {
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        match super::ui::channel_rows(self).get(row_in_list) {
                            Some(super::ui::ChannelRow::Header { channel_idx }) => {
                                self.selected_channel = *channel_idx;
                                self.active_view = ActiveView::Channel(*channel_idx);
                                self.focus = Focus::Chat;
                            }
                            Some(super::ui::ChannelRow::Member {
                                agent_chat_idx: Some(idx),
                                ..
                            }) => {
                                self.selected = *idx;
                                self.active_view = ActiveView::Chat;
                                self.focus = Focus::Agents;
                            }
                            Some(super::ui::ChannelRow::Member {
                                agent_chat_idx: None,
                                ..
                            })
                            | None => {
                                self.focus = Focus::Channels;
                            }
                        }
                        return;
                    }
                }

                if let Some(rect) = self.layout.workflows_list_inner {
                    if rect_contains(rect, pos) {
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        match super::ui::workflow_rows(self).get(row_in_list) {
                            Some(super::ui::WorkflowRow::Header { workflow_idx }) => {
                                let idx = *workflow_idx;
                                self.selected_workflow = idx;
                                self.open_workflow_form(idx);
                            }
                            Some(super::ui::WorkflowRow::Instance { instance_id, .. }) => {
                                let id = *instance_id;
                                if let Some(inst) =
                                    self.workflow_instances.iter().find(|i| i.id == id)
                                {
                                    let wf_idx = inst.workflow_idx;
                                    self.open_workflow_instance(wf_idx, id);
                                }
                            }
                            None => {
                                self.focus = Focus::Workflows;
                            }
                        }
                        return;
                    }
                }

                if let Some(rect) = self.layout.agent_list_inner {
                    if rect_contains(rect, pos) {
                        let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                        // Walk the same row layout the renderer uses
                        // so group-header rows are treated as inert
                        // dividers rather than misaligning the click
                        // by one chat per visible header.
                        let rows = self.agent_list_visual_rows();
                        match rows.get(row_in_list) {
                            Some(AgentListVisualRow::Chat { idx, .. }) => {
                                self.selected = *idx;
                                // Picking an agent always returns to
                                // the chat view — matches the j/k
                                // keyboard behaviour.
                                self.return_to_chat_if_viewing_tasks();
                                self.focus = Focus::Chat;
                            }
                            _ => {
                                // Header row or out-of-range click —
                                // grab focus so j/k navigation works
                                // but don't change the selection.
                                self.focus = Focus::Agents;
                            }
                        }
                        return;
                    }
                }
                if let Some(rect) = self.layout.chat_history_inner {
                    if rect_contains(rect, pos) {
                        // Main area: Focus depends on which view is
                        // drawn there.
                        self.focus = match self.active_view {
                            ActiveView::Chat | ActiveView::Channel(_) => Focus::Chat,
                            ActiveView::Tasks => Focus::Tasks,
                            ActiveView::Memory => Focus::Memory,
                            ActiveView::Cores => Focus::Cores,
                            ActiveView::Inbox => Focus::Inbox,
                            ActiveView::Cron => Focus::Cron,
                            ActiveView::WorkflowForm { .. }
                            | ActiveView::WorkflowInstance { .. } => Focus::Workflow,
                        };

                        // Memory view: route row / search-box clicks to
                        // bank-list or bank-stage handlers. Returns
                        // early on any hit; falls through to plain
                        // focus-grab on an empty-area click.
                        if self.active_view == ActiveView::Memory
                            && self.handle_mouse_memory_main(pos, ev.column, ev.row)
                        {
                            return;
                        }

                        // Cores view: clicking a row selects it. Row 0
                        // is the column header and is non-selectable.
                        if self.active_view == ActiveView::Cores {
                            let row_in_list = ev.row.saturating_sub(rect.y) as usize;
                            if row_in_list >= 1 {
                                let target = row_in_list - 1;
                                if target < self.cores_screen.rows.len() {
                                    self.cores_screen.cursor = target;
                                }
                            }
                        }

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
                            ActiveView::Chat | ActiveView::Channel(_) => Focus::Chat,
                            ActiveView::Tasks => Focus::Tasks,
                            ActiveView::Memory => Focus::Memory,
                            ActiveView::Cores => Focus::Cores,
                            ActiveView::Inbox => Focus::Inbox,
                            ActiveView::Cron => Focus::Cron,
                            ActiveView::WorkflowForm { .. }
                            | ActiveView::WorkflowInstance { .. } => Focus::Workflow,
                        };
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                if self.active_view == ActiveView::Inbox
                    && self.mouse_in_inbox_detail_zone(pos)
                {
                    self.inbox_screen.detail_scroll =
                        self.inbox_screen.detail_scroll.saturating_sub(3);
                    return;
                }
                if self.active_view == ActiveView::Memory {
                    if let Some(zone) = self.memory_scroll_hit(pos) {
                        self.memory_scroll(zone, -1);
                        return;
                    }
                }
                if self.active_view == ActiveView::Inbox {
                    // Detail pane: scroll the message body up.
                    if let Some(rect) = self.layout.inbox_detail_inner {
                        if rect_contains(rect, pos) {
                            self.inbox_screen.detail_scroll =
                                self.inbox_screen.detail_scroll.saturating_sub(3);
                            return;
                        }
                    }
                    // List pane: move the selection up.
                    if let Some(rect) = self.layout.inbox_list_inner {
                        if rect_contains(rect, pos) {
                            if self.inbox_screen.cursor > 0 {
                                self.inbox_screen
                                    .set_cursor(self.inbox_screen.cursor - 1);
                            }
                            return;
                        }
                    }
                }
                if self.mouse_in_chat_scroll_zone(pos) {
                    self.scroll_up(3);
                }
            }
            MouseEventKind::ScrollDown => {
                if self.active_view == ActiveView::Inbox
                    && self.mouse_in_inbox_detail_zone(pos)
                {
                    let max = self.inbox_detail_max_scroll();
                    self.inbox_screen.detail_scroll = self
                        .inbox_screen
                        .detail_scroll
                        .saturating_add(3)
                        .min(max);
                    return;
                }
                if self.active_view == ActiveView::Memory {
                    if let Some(zone) = self.memory_scroll_hit(pos) {
                        self.memory_scroll(zone, 1);
                        return;
                    }
                }
                if self.active_view == ActiveView::Inbox {
                    // Detail pane: scroll the message body down.
                    if let Some(rect) = self.layout.inbox_detail_inner {
                        if rect_contains(rect, pos) {
                            self.inbox_screen.detail_scroll =
                                self.inbox_screen.detail_scroll.saturating_add(3);
                            return;
                        }
                    }
                    // List pane: advance the selection.
                    if let Some(rect) = self.layout.inbox_list_inner {
                        if rect_contains(rect, pos) {
                            if !self.inbox.is_empty()
                                && self.inbox_screen.cursor + 1 < self.inbox.len()
                            {
                                self.inbox_screen
                                    .set_cursor(self.inbox_screen.cursor + 1);
                            }
                            return;
                        }
                    }
                }
                if self.mouse_in_chat_scroll_zone(pos) {
                    self.scroll_down(3);
                }
            }
            _ => {}
        }
    }

    /// Returns the scrollable "zone" the cursor is in, if any. Used by
    /// the wheel handlers to scope scrolling to a sensible target
    /// (detail body, result list, bank list) instead of the whole
    /// main area.
    fn memory_scroll_hit(&self, pos: (u16, u16)) -> Option<MemoryScrollZone> {
        if let Some(rect) = self.layout.memory_modal_outer {
            if self
                .memory_screen
                .bank()
                .map(|b| b.detail.is_some())
                .unwrap_or(false)
                && rect_contains(rect, pos)
            {
                return Some(MemoryScrollZone::Detail);
            }
        }
        if matches!(
            self.memory_screen.stage,
            super::memory_view::MemoryStage::Banks { .. }
        ) {
            for rect in &self.layout.memory_bank_rows {
                if rect_contains(*rect, pos) {
                    return Some(MemoryScrollZone::Banks);
                }
            }
        } else {
            for rect in &self.layout.memory_result_rows {
                if rect_contains(*rect, pos) {
                    return Some(MemoryScrollZone::Results);
                }
            }
        }
        None
    }

    fn memory_scroll(&mut self, zone: MemoryScrollZone, delta: i32) {
        match zone {
            MemoryScrollZone::Banks => {
                if let super::memory_view::MemoryStage::Banks { selected } =
                    &mut self.memory_screen.stage
                {
                    let bank_count = self.config.memory_banks.len();
                    if bank_count == 0 {
                        return;
                    }
                    let next = (*selected as i32 + delta)
                        .clamp(0, bank_count.saturating_sub(1) as i32)
                        as usize;
                    *selected = next;
                }
            }
            MemoryScrollZone::Results => {
                if let Some(b) = self.memory_screen.bank_mut() {
                    if b.results.is_empty() {
                        return;
                    }
                    let next = (b.selected_result as i32 + delta)
                        .clamp(0, b.results.len().saturating_sub(1) as i32)
                        as usize;
                    b.selected_result = next;
                }
            }
            MemoryScrollZone::Detail => {
                if let Some(d) = self
                    .memory_screen
                    .bank_mut()
                    .and_then(|b| b.detail.as_mut())
                {
                    if delta > 0 {
                        d.scroll = d.scroll.saturating_add(delta as u16);
                    } else {
                        d.scroll = d.scroll.saturating_sub((-delta) as u16);
                    }
                }
            }
        }
    }

    /// Main-area mouse handler for the Memory view when no modal is
    /// open. Returns `true` when the click was consumed so the caller
    /// can skip its generic "focus the main area" fallback.
    /// Resolve a click against the inbox's registered hotspots.
    /// Returns `true` if the click hit one of our interactive regions
    /// (so the top-level mouse handler should stop) — otherwise
    /// returns `false` and the caller falls through to the generic
    /// focus-grab path.
    fn handle_mouse_inbox(&mut self, pos: (u16, u16)) -> bool {
        use super::inbox_view::{ForwardStage, InboxHotspot, InboxMode};
        let hit = self
            .layout
            .inbox_hotspots
            .iter()
            .find(|(rect, _)| rect_contains(*rect, pos))
            .map(|(_, h)| h.clone());
        let Some(hotspot) = hit else {
            // No hotspot under the cursor — but a click inside the
            // detail pane should still grab focus and switch the pane
            // to Detail so the wheel and j/k start scrolling the body.
            if let Some(rect) = self.layout.inbox_detail_inner {
                if rect_contains(rect, pos) {
                    self.focus = Focus::Inbox;
                    self.inbox_screen.pane = super::inbox_view::InboxPane::Detail;
                    return true;
                }
            }
            return false;
        };
        // A click anywhere on a hotspot also grabs focus, matching the
        // behaviour of the keyboard-activated inbox view.
        self.focus = Focus::Inbox;
        match hotspot {
            InboxHotspot::SelectRow(i) => {
                self.inbox_screen.select(i, self.inbox.len());
                // Clicking a list row implies the user is working in
                // the list pane, so any prior detail-pane focus is
                // dropped.
                self.inbox_screen.pane = super::inbox_view::InboxPane::List;
                // Drop any lingering status toast so it doesn't
                // confuse the next interaction.
                self.inbox_screen.status_msg = None;
            }
            InboxHotspot::ReplyButton => {
                if let Some(msg) = self.inbox.get(self.inbox_screen.cursor) {
                    self.inbox_screen.mode = InboxMode::Reply {
                        to: msg.from.clone(),
                        body: String::new(),
                    };
                }
            }
            InboxHotspot::ForwardButton => {
                if self.inbox.get(self.inbox_screen.cursor).is_none() {
                    return true;
                }
                let candidates = super::inbox_view::forward_candidates(&self.config);
                if candidates.is_empty() {
                    self.inbox_screen.status_msg = Some("no agents to forward to".to_string());
                    return true;
                }
                self.inbox_screen.mode = InboxMode::Forward(super::inbox_view::ForwardState {
                    candidates,
                    selected: 0,
                    stage: ForwardStage::PickRecipient,
                    note: String::new(),
                    quoted: String::new(),
                });
            }
            InboxHotspot::DeleteButton => {
                if self.inbox_screen.cursor < self.inbox.len() {
                    self.inbox.remove(self.inbox_screen.cursor);
                    self.inbox_screen.detail_scroll = 0;
                    self.inbox_screen.reclamp(self.inbox.len());
                }
            }
            InboxHotspot::SendButton => self.submit_inbox_send(),
            InboxHotspot::CancelButton => {
                self.inbox_screen.mode = InboxMode::List;
            }
            InboxHotspot::ForwardRecipient(i) => {
                if let InboxMode::Forward(fwd) = &mut self.inbox_screen.mode {
                    if i < fwd.candidates.len() {
                        fwd.selected = i;
                        // Advance to EditBody immediately — one click
                        // picks the target and drops the cursor into
                        // the note editor with the quoted original
                        // shown read-only below.
                        if let Some(msg) = self.inbox.get(self.inbox_screen.cursor) {
                            fwd.quoted = format!("Forwarded from {}:\n\n{}", msg.from, msg.body);
                        }
                        fwd.stage = ForwardStage::EditBody;
                    }
                }
            }
        }
        true
    }

    /// Mouse entry point for the Cron view. Returns `true` when a
    /// hotspot consumed the click (including modal-swallow clicks),
    /// `false` to fall through to the generic sidebar / focus-grab
    /// handlers.
    ///
    /// Hotspots are painted top-to-bottom so modal overlays (drawn
    /// last) take precedence over list rows beneath them.
    fn handle_mouse_cron(&mut self, pos: (u16, u16)) -> bool {
        use super::cron_view::{CronForm, CronFormField, CronHotspot, CronMode, CronScheduleKind};

        let modal_open = !matches!(self.cron_screen.mode, CronMode::List);

        let hit = self
            .layout
            .cron_hotspots
            .iter()
            .find(|(rect, _)| rect_contains(*rect, pos))
            .map(|(_, h)| h.clone());

        let Some(hotspot) = hit else {
            // No hotspot under the cursor. With a modal open we
            // swallow the click to prevent unexpected view switches
            // from a stray sidebar click.
            return modal_open;
        };

        // Any cron hotspot grabs focus, mirroring the keyboard path.
        self.focus = Focus::Cron;
        self.cron_screen.status_msg = None;

        let jobs = super::cron_view::listed_jobs(&self.cron);

        match hotspot {
            CronHotspot::NewButton => {
                self.cron_screen.mode = CronMode::Form(CronForm::new_add());
            }
            CronHotspot::SelectRow(i) => {
                if i < jobs.len() {
                    self.cron_screen.cursor = i;
                }
            }
            CronHotspot::ToggleEnabled(i) => {
                if let Some(j) = jobs.get(i) {
                    let id = j.id;
                    let patch = crate::cron::CronPatch {
                        enabled: Some(!j.enabled),
                        ..Default::default()
                    };
                    match self.cron.update(id, patch) {
                        Ok(updated) => {
                            let wire = crate::cron::WireCronJob::from(updated);
                            self.cron_invalidate.notify_one();
                            let _ = self
                                .cron_event_tx
                                .send(crate::cron::CronEvent::Updated { job: wire });
                            self.cron_screen.cursor = i;
                        }
                        Err(e) => {
                            self.cron_screen.status_msg = Some(format!("toggle failed: {e}"));
                        }
                    }
                }
            }
            CronHotspot::EditRow(i) => {
                if let Some(j) = jobs.get(i) {
                    self.cron_screen.cursor = i;
                    self.cron_screen.mode = CronMode::Form(CronForm::new_edit(j));
                }
            }
            CronHotspot::DeleteRow(i) => {
                if let Some(j) = jobs.get(i) {
                    self.cron_screen.cursor = i;
                    let display = format!("{} → {}", &j.id.to_string()[..8], j.target_agent);
                    self.cron_screen.mode = CronMode::ConfirmDelete {
                        job_id: j.id,
                        display,
                    };
                }
            }
            CronHotspot::ConfirmDeleteYes => {
                if let CronMode::ConfirmDelete { job_id, .. } = self.cron_screen.mode.clone() {
                    match self.cron.remove(job_id) {
                        Ok(_) => {
                            self.cron_invalidate.notify_one();
                            let _ = self
                                .cron_event_tx
                                .send(crate::cron::CronEvent::Removed { id: job_id });
                            self.cron_screen.status_msg =
                                Some(format!("removed cron job {job_id}"));
                        }
                        Err(e) => {
                            self.cron_screen.status_msg = Some(format!("remove failed: {e}"));
                        }
                    }
                }
                self.cron_screen.mode = CronMode::List;
                let len = super::cron_view::listed_jobs(&self.cron).len();
                self.cron_screen.reclamp(len);
            }
            CronHotspot::ConfirmDeleteNo => {
                self.cron_screen.mode = CronMode::List;
            }
            CronHotspot::FormSave => {
                if let CronMode::Form(form) = self.cron_screen.mode.clone() {
                    match self.commit_cron_form(&form) {
                        Ok(msg) => {
                            self.cron_screen.status_msg = Some(msg);
                            self.cron_screen.mode = CronMode::List;
                            let len = super::cron_view::listed_jobs(&self.cron).len();
                            self.cron_screen.reclamp(len);
                        }
                        Err(e) => {
                            if let CronMode::Form(ref mut f) = self.cron_screen.mode {
                                f.error = Some(e);
                            }
                        }
                    }
                }
            }
            CronHotspot::FormCancel => {
                self.cron_screen.mode = CronMode::List;
            }
            CronHotspot::FormField(field) => {
                if let CronMode::Form(ref mut form) = self.cron_screen.mode {
                    form.focused = field;
                    form.error = None;
                    // Clicking a toggleable field also cycles it —
                    // matches the web portal's one-click UX for
                    // checkboxes / segmented selects.
                    match field {
                        CronFormField::PayloadKind => {
                            form.payload_kind = form.payload_kind.toggle();
                        }
                        CronFormField::ScheduleKind => {
                            form.schedule_kind = form.schedule_kind.toggle();
                            if form.schedule_kind == CronScheduleKind::Recurring {
                                form.catchup_on_startup = false;
                            }
                        }
                        CronFormField::Timezone => {
                            form.timezone = match form.timezone {
                                crate::cron::CronTz::Local => crate::cron::CronTz::Utc,
                                crate::cron::CronTz::Utc => crate::cron::CronTz::Local,
                            };
                        }
                        CronFormField::CatchupOnStartup => {
                            if form.schedule_kind == CronScheduleKind::OneOff {
                                form.catchup_on_startup = !form.catchup_on_startup;
                            }
                        }
                        CronFormField::Enabled => {
                            form.enabled = !form.enabled;
                        }
                        // Text fields just take focus; typing still
                        // goes through the keyboard handler.
                        _ => {}
                    }
                }
            }
        }

        true
    }

    fn handle_mouse_memory_main(&mut self, pos: (u16, u16), col: u16, row: u16) -> bool {
        // Banks stage: click a bank row → select + open.
        if matches!(
            self.memory_screen.stage,
            super::memory_view::MemoryStage::Banks { .. }
        ) {
            let rects = self.layout.memory_bank_rows.clone();
            for (i, rect) in rects.iter().enumerate() {
                if rect_contains(*rect, pos) {
                    if let Some(bank) = self.config.memory_banks.get(i) {
                        let id = bank.id.clone();
                        self.memory_screen.stage =
                            super::memory_view::MemoryStage::Banks { selected: i };
                        self.memory_screen.enter_bank(id);
                        self.dispatch_memory_search();
                    }
                    return true;
                }
            }
            return false;
        }

        // Bank stage. Search rect → focus search (cursor moved to end).
        if let Some(rect) = self.layout.memory_search_rect {
            if rect_contains(rect, pos) {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.focus = super::memory_view::BankFocus::Search;
                }
                return true;
            }
        }

        // Result row click: select it (+ double-click opens detail).
        let result_rects = self.layout.memory_result_rows.clone();
        for (i, rect) in result_rects.iter().enumerate() {
            if rect_contains(*rect, pos) {
                let hit = self
                    .memory_screen
                    .bank()
                    .and_then(|b| b.results.get(i).cloned());
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.focus = super::memory_view::BankFocus::Results;
                    b.selected_result = i;
                }
                let is_double = match self.last_click {
                    Some((c, r, t)) => c == col && r == row && t.elapsed() < DOUBLE_CLICK_WINDOW,
                    None => false,
                };
                self.last_click = Some((col, row, Instant::now()));
                if is_double {
                    if let Some(hit) = hit {
                        self.open_memory_detail(hit.id);
                    }
                    self.last_click = None;
                }
                return true;
            }
        }

        false
    }

    /// Capture clicks while any Memory-view modal is open. Clicks
    /// outside the modal's outer rect dismiss the modal (operator-
    /// friendly default, unlike the Tasks modal which swallows);
    /// clicks inside route to field / row / picker hit-tests.
    fn handle_mouse_memory_modal(&mut self, pos: (u16, u16)) {
        // Confirm-delete: only y/n via keyboard; any click dismisses it.
        let confirm_open = self
            .memory_screen
            .bank()
            .map(|b| b.confirm_delete.is_some())
            .unwrap_or(false);
        if confirm_open {
            if let Some(b) = self.memory_screen.bank_mut() {
                b.confirm_delete = None;
            }
            return;
        }

        let outer = self.layout.memory_modal_outer;

        // Link picker takes priority over the links overlay when open.
        let picker_open = self
            .memory_screen
            .bank()
            .and_then(|b| b.links_overlay.as_ref())
            .map(|o| o.picker.is_some())
            .unwrap_or(false);
        if picker_open {
            // Clicks outside dismiss the picker.
            if let Some(rect) = outer {
                if !rect_contains(rect, pos) {
                    if let Some(o) = self
                        .memory_screen
                        .bank_mut()
                        .and_then(|b| b.links_overlay.as_mut())
                    {
                        o.picker = None;
                    }
                    return;
                }
            }
            // (Row-level picker hit-tests are not stored; picker is
            // driven by Tab/Enter/arrow keys. Clicks inside the outer
            // rect are swallowed so they don't leak to the main area.)
            return;
        }

        // Form open: hit-test field rects; outside the modal dismisses.
        let form_open = self
            .memory_screen
            .bank()
            .map(|b| b.form.is_some())
            .unwrap_or(false);
        if form_open {
            if let Some(rect) = outer {
                if !rect_contains(rect, pos) {
                    if let Some(b) = self.memory_screen.bank_mut() {
                        b.form = None;
                    }
                    return;
                }
            }
            let fields = self.layout.memory_form_fields.clone();
            for (field, rect) in &fields {
                if rect_contains(*rect, pos) {
                    // Focus the field; buttons also trigger their
                    // action immediately.
                    if let Some(f) = self.memory_screen.bank_mut().and_then(|b| b.form.as_mut()) {
                        f.focused_field = *field;
                    }
                    match field {
                        super::memory_view::NoteField::Ok => self.submit_memory_form(),
                        super::memory_view::NoteField::Cancel => {
                            if let Some(b) = self.memory_screen.bank_mut() {
                                b.form = None;
                            }
                        }
                        _ => {}
                    }
                    return;
                }
            }
            // Click inside the modal but not on a labelled rect —
            // swallow so it doesn't reach the background.
            return;
        }

        // Links overlay (no picker): hit-test link rows.
        let links_open = self
            .memory_screen
            .bank()
            .map(|b| b.links_overlay.is_some())
            .unwrap_or(false);
        if links_open {
            if let Some(rect) = outer {
                if !rect_contains(rect, pos) {
                    if let Some(b) = self.memory_screen.bank_mut() {
                        b.links_overlay = None;
                    }
                    return;
                }
            }
            let link_rects = self.layout.memory_link_rows.clone();
            for (i, rect) in link_rects.iter().enumerate() {
                if rect_contains(*rect, pos) {
                    if let Some(o) = self
                        .memory_screen
                        .bank_mut()
                        .and_then(|b| b.links_overlay.as_mut())
                    {
                        if i < o.existing.len() {
                            o.cursor = i;
                        }
                    }
                    return;
                }
            }
            return;
        }

        // Detail overlay: outside = close; inside = swallow.
        if let Some(rect) = outer {
            if !rect_contains(rect, pos) {
                if let Some(b) = self.memory_screen.bank_mut() {
                    b.detail = None;
                }
            }
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

        // Click on a TODO row: first click selects (focus + move
        // cursor); a second click on the already-selected row is what
        // actually toggles the done state. Two-step pattern keeps a
        // mis-aimed click from accidentally checking off the wrong
        // todo. Beats the field hit-test because the row rects sit
        // inside the section's outer rect.
        if let Some((todo_id, row_idx)) = self
            .layout
            .task_form_todo_rows
            .iter()
            .enumerate()
            .find(|(_, (_, r))| rect_contains(*r, pos))
            .map(|(idx, (id, _))| (*id, idx))
        {
            let mut should_toggle = false;
            if let Some(form) = self.tasks_screen.form.as_mut() {
                let new_cursor = (form.todos_scroll as usize) + row_idx;
                should_toggle =
                    form.focused_field == FormField::Todos && form.todos_cursor == new_cursor;
                form.focused_field = FormField::Todos;
                form.todos_cursor = new_cursor;
            }
            if should_toggle {
                let task_id = match self.tasks_screen.form.as_ref().map(|f| f.kind) {
                    Some(TaskFormKind::Edit(id)) => Some(id),
                    _ => None,
                };
                if let Some(task_id) = task_id {
                    let next = self
                        .tasks
                        .get(task_id)
                        .and_then(|t| t.todos.iter().find(|td| td.id == todo_id))
                        .map(|td| match td.status {
                            crate::tasks::TodoStatus::Completed => crate::tasks::TodoStatus::Pending,
                            _ => crate::tasks::TodoStatus::Completed,
                        });
                    if let Some(next) = next {
                        let _ = self.tasks.update_todo(task_id, todo_id, None, Some(next));
                    }
                }
            }
            return;
        }

        // Click on a comment row sets the cursor.
        if let Some((_, row_idx)) = self
            .layout
            .task_form_comment_rows
            .iter()
            .enumerate()
            .find(|(_, (_, r))| rect_contains(*r, pos))
            .map(|(idx, (id, _))| (*id, idx))
        {
            if let Some(form) = self.tasks_screen.form.as_mut() {
                form.focused_field = FormField::Comments;
                form.comments_cursor = (form.comments_scroll as usize) + row_idx;
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
                // Clicking the Todos / Comments label toggles the
                // collapse state — same as Enter on a focused
                // section. Click on a row inside the expanded box is
                // handled above and short-circuits before this point.
                else if field == FormField::Todos {
                    if let Some(form) = self.tasks_screen.form.as_mut() {
                        form.toggle_todos_collapsed();
                    }
                } else if field == FormField::Comments {
                    if let Some(form) = self.tasks_screen.form.as_mut() {
                        form.toggle_comments_collapsed();
                    }
                }
            }
        }
    }

    /// Handle a left click while the workflow property form is
    /// active. Returns `true` when the click landed on an interactive
    /// region (button, field, checkbox, picker) so the outer handler
    /// can short-circuit. Clicks outside any of these regions return
    /// `false` so the caller falls through to sidebar hit-tests.
    fn handle_mouse_workflow_form(&mut self, pos: (u16, u16)) -> bool {
        use super::workflow_view::{FieldEditor, FormButton, FormFocus};

        // 0. Picker open? Click a row → commit; click inside the
        //    picker frame but not on a row → swallow; click outside
        //    the outer rect → close without committing.
        if self
            .workflow_form
            .as_ref()
            .and_then(|f| f.picker.as_ref())
            .is_some()
        {
            if let Some(idx) = self
                .layout
                .workflow_form_picker_rows
                .iter()
                .position(|r| rect_contains(*r, pos))
            {
                if let Some(form) = self.workflow_form.as_mut() {
                    if let Some(p) = form.picker.as_mut() {
                        p.selected = idx;
                    }
                    form.commit_picker();
                }
                return true;
            }
            if let Some(outer) = self.layout.workflow_form_picker_outer {
                if rect_contains(outer, pos) {
                    return true;
                }
            }
            if let Some(form) = self.workflow_form.as_mut() {
                form.close_picker();
            }
            return true;
        }

        // 1. Run button wins over any overlapping field rect.
        if self
            .layout
            .workflow_form_buttons
            .iter()
            .any(|(b, r)| *b == FormButton::Run && rect_contains(*r, pos))
        {
            if let ActiveView::WorkflowForm { workflow_idx } = self.active_view {
                self.submit_workflow_form(workflow_idx);
            }
            return true;
        }

        // 2. MultiSelect checkbox rects win over whole-field rects
        //    because they're physically inside them. Toggle the
        //    checkbox, focus the field, and move the MultiSelect
        //    cursor so subsequent Space still targets the clicked row.
        if let Some(((field_idx, opt_idx), _)) = self
            .layout
            .workflow_form_multi_rects
            .iter()
            .find(|(_, r)| rect_contains(*r, pos))
            .copied()
        {
            let Some(form) = self.workflow_form.as_mut() else {
                return true;
            };
            form.error = None;
            form.focus = FormFocus::Field(field_idx);
            form.set_multi_cursor(field_idx, opt_idx);
            if let Some(FieldEditor::MultiSelect { chosen, .. }) = form.fields.get_mut(field_idx) {
                if let Some(b) = chosen.get_mut(opt_idx) {
                    *b = !*b;
                }
            }
            return true;
        }

        // 3. Whole-field rect: focus the field, and open the picker
        //    if the clicked field is a Select.
        if let Some((field_idx, _)) = self
            .layout
            .workflow_form_fields
            .iter()
            .find(|(_, r)| rect_contains(*r, pos))
            .copied()
        {
            let Some(form) = self.workflow_form.as_mut() else {
                return true;
            };
            form.error = None;
            form.focus = FormFocus::Field(field_idx);
            if let Some(FieldEditor::Select { .. }) = form.fields.get(field_idx) {
                form.open_picker(field_idx);
            }
            return true;
        }

        false
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

    /// Hit-test for the inbox detail Paragraph (the message body
    /// pane). Drives the wheel-handler arm so wheel events scope to
    /// the body rather than the row list. Returns false when the
    /// rect is unset (e.g. inbox is empty, Reply/Forward modal is
    /// up, or the user has navigated away mid-frame).
    fn mouse_in_inbox_detail_zone(&self, pos: (u16, u16)) -> bool {
        self.layout
            .inbox_detail_inner
            .map(|r| rect_contains(r, pos))
            .unwrap_or(false)
    }

    /// Maximum scroll offset for the inbox detail Paragraph: how
    /// many rows the user can scroll past the top before the body
    /// runs out. Returns 0 when no message is selected, the layout
    /// rect is unset, or the body fits inside the inner rect (i.e.
    /// no scrolling needed).
    fn inbox_detail_max_scroll(&self) -> u16 {
        let inner_height = self
            .layout
            .inbox_detail_inner
            .map(|r| r.height)
            .unwrap_or(0);
        let Some(msg) = self.inbox.get(self.inbox_screen.cursor) else {
            return 0;
        };
        // Mirrors the line-count built in `render_inbox_screen`:
        // 2 header rows (From: line + blank) + body lines.
        let body_lines = msg.body.lines().count();
        let body_height = u16::try_from(body_lines.saturating_add(2)).unwrap_or(u16::MAX);
        body_height.saturating_sub(inner_height)
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
/// Pull leading `@name` tokens off the front of a channel message
/// body. Returns the list of target names (in encounter order,
/// without the leading `@`) and the full original body unchanged —
/// mentions are NOT stripped from the delivered message, they just
/// narrow the recipient list.
///
/// A token is only a mention when it's at the start of a whitespace-
/// separated run at the beginning of the body. `@` inside running
/// prose (e.g. "ping @alice later") is ignored. Returns an empty
/// Vec when the body has no leading `@`.
fn parse_leading_mentions(body: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    let mut rest = body.trim_start();
    while let Some(stripped) = rest.strip_prefix('@') {
        let end = stripped
            .find(|c: char| c.is_whitespace())
            .unwrap_or(stripped.len());
        let name = &stripped[..end];
        // A bare `@` with nothing after it is not a mention.
        if name.is_empty() {
            break;
        }
        mentions.push(name.to_string());
        rest = stripped[end..].trim_start();
    }
    mentions
}

fn try_parse_aciddog_send(cmd: &str, binary: &str) -> Option<(String, String)> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Send { to, body } => Some((to, body)),
        _ => None,
    }
}

fn try_parse_aciddog_worker_create(cmd: &str, binary: &str) -> Option<(String, String)> {
    match try_parse_aciddog(cmd, binary)? {
        AgentCommand::Worker {
            cmd:
                crate::cli::WorkerCommand::Create {
                    template,
                    task,
                    // `--properties` is captured at IPC construction time;
                    // the bash-parsed shortcut ignores it.
                    properties: _,
                },
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
pub async fn run(config: AciddogConfig, config_path: std::path::PathBuf) -> Result<()> {
    info!(team = %config.team.id, agents = config.agents.len(), "starting TUI");

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut handles = super::boot::boot_orchestrator(config, config_path).await?;

    let result = event_loop(
        &mut terminal,
        &mut handles.app,
        handles.ai_rx,
        &mut handles.socket.events,
        &mut handles.compaction_rx,
        handles.memory_event_rx,
        &mut handles.workflow_event_rx,
        &mut handles.usage_event_rx,
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
    mut channel_rx: mpsc::Receiver<String>,
    mut cancel_rx: mpsc::Receiver<()>,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
) {
    loop {
        // Biased select. Strict priority ordering:
        //   cancel_rx > user_rx > agent_rx > channel_rx
        // The cancel branch sits first so any `/stop` signal queued
        // between turns is consumed as a no-op rather than being
        // delivered to the next turn. Among prompts:
        // a user-typed prompt always wins over a mailbox message,
        // and direct mailbox traffic always wins over channel
        // chatter. Channel messages drain only when the other two
        // queues are empty, making channel volume incapable of
        // starving the agent from real work.
        let prompt = tokio::select! {
            biased;
            _ = cancel_rx.recv() => continue,
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            Some(m) = channel_rx.recv() => m,
            else => return,
        };

        // Race the turn setup against `/stop`. If the user cancels
        // before the core hands back its event stream, drop the
        // pending `turn()` future (Rust async cancellation via Drop)
        // and return to the outer select.
        let turn_result = tokio::select! {
            biased;
            _ = cancel_rx.recv() => {
                let _ = ai_tx.send((idx, AIEvent::Stopped)).await;
                continue;
            }
            res = core.turn(&prompt) => res,
        };

        match turn_result {
            Ok(mut stream) => {
                let mut cancelled = false;
                loop {
                    tokio::select! {
                        biased;
                        _ = cancel_rx.recv() => {
                            // Mark cancelled and break; the stream is
                            // dropped at the end of this `match` arm,
                            // which (via `kill_on_drop` on any tool
                            // child) terminates in-flight tool calls.
                            cancelled = true;
                            break;
                        }
                        ev = stream.next() => match ev {
                            Some(ev) => {
                                if ai_tx.send((idx, ev)).await.is_err() {
                                    return;
                                }
                            }
                            None => break,
                        }
                    }
                }
                if cancelled {
                    let _ = ai_tx.send((idx, AIEvent::Stopped)).await;
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

/// Kind of message pulled off an automaton's priority queues. Used to
/// stamp the JSON envelope the child receives on stdin — `from` and
/// `channel` are extracted from the wrapper tags the main event loop
/// already applies when enqueueing agent/channel messages.
#[derive(Debug, Clone, Copy)]
enum AutomatonEventKind {
    UserMessage,
    AgentMessage,
    ChannelMessage,
}

impl AutomatonEventKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::UserMessage => "user_message",
            Self::AgentMessage => "agent_message",
            Self::ChannelMessage => "channel_message",
        }
    }
}

/// Parse the `from="..."` attribute out of a `<Message …>` or
/// `<ChannelMessage …>` wrapper. Returns `None` if the attribute is
/// absent — callers fall back to a default (empty string) so malformed
/// input is still delivered instead of being dropped.
fn extract_attr(wrapper: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let start = wrapper.find(&needle)? + needle.len();
    let rest = &wrapper[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// Unwrap a `<Message from="x">body</Message>` or
/// `<ChannelMessage channel="c" from="x">body</ChannelMessage>` into
/// the inner body. Returns the original string on any structural
/// mismatch so the child still sees *something* rather than an empty
/// body.
fn strip_outer_tag<'a>(wrapper: &'a str, tag: &str) -> &'a str {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    if let Some(open_end) = wrapper.find('>').map(|i| i + 1) {
        if wrapper.starts_with(&open) && wrapper.ends_with(&close) {
            let body_end = wrapper.len() - close.len();
            if body_end > open_end {
                return &wrapper[open_end..body_end];
            }
        }
    }
    wrapper
}

/// Per-automaton event loop. Mirrors `run_core`'s priority select
/// (user > agent > channel) but spawns a fresh child process for each
/// dequeued message, writes a JSON envelope to stdin, pipes
/// stdout/stderr lines back via `AIEvent::AutomatonStdout`/`Stderr`,
/// and enforces **one-child-at-a-time** by awaiting the child's exit
/// before pulling the next message. This is what makes the chat's
/// status stay `Working` for the full run and causes incoming
/// messages to queue up in their `mpsc` channels rather than race.
pub(super) async fn run_automaton_core(
    idx: usize,
    automaton: crate::config::Automaton,
    session_id: Uuid,
    socket_path: String,
    mut user_rx: mpsc::UnboundedReceiver<String>,
    mut agent_rx: mpsc::Receiver<String>,
    mut channel_rx: mpsc::Receiver<String>,
    mut cancel_rx: mpsc::Receiver<()>,
    ai_tx: mpsc::Sender<(usize, AIEvent)>,
) {
    use std::process::Stdio;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    // Caller is expected to only dispatch Script-backed automatons
    // here — webservice-backed automatons go to `run_webservice_core`.
    // Destructure once so the existing body reads like the old struct
    // shape.
    let (command, args, cwd, env, timeout_seconds) = match automaton.backend.clone() {
        crate::config::AutomatonBackend::Script {
            command,
            args,
            cwd,
            env,
            timeout_seconds,
        } => (command, args, cwd, env, timeout_seconds),
        crate::config::AutomatonBackend::Webservice { .. }
        | crate::config::AutomatonBackend::Remote { .. } => return,
    };

    loop {
        // Priority: user > agent > channel. Cancel sits first so any
        // queued `/stop` between children is drained as a no-op.
        let (raw, kind) = tokio::select! {
            biased;
            _ = cancel_rx.recv() => continue,
            Some(m) = user_rx.recv() => (m, AutomatonEventKind::UserMessage),
            Some(m) = agent_rx.recv() => (m, AutomatonEventKind::AgentMessage),
            Some(m) = channel_rx.recv() => (m, AutomatonEventKind::ChannelMessage),
            else => return,
        };

        // Unwrap whichever tag the orchestrator applied. User messages
        // arrive raw; agent/channel wrappers are stripped back to the
        // inner body for the JSON envelope so scripts never have to
        // parse XML-ish markup.
        let (from, channel, body): (String, Option<String>, String) = match kind {
            AutomatonEventKind::UserMessage => ("user".to_string(), None, raw.clone()),
            AutomatonEventKind::AgentMessage => {
                let from = extract_attr(&raw, "from").unwrap_or_default();
                let body = strip_outer_tag(&raw, "Message").to_string();
                (from, None, body)
            }
            AutomatonEventKind::ChannelMessage => {
                let from = extract_attr(&raw, "from").unwrap_or_default();
                let channel = extract_attr(&raw, "channel");
                let body = strip_outer_tag(&raw, "ChannelMessage").to_string();
                (from, channel, body)
            }
        };

        let envelope = serde_json::json!({
            "kind": kind.as_str(),
            "from": from,
            "channel": channel,
            "body": body,
            "ts": chrono::Utc::now().to_rfc3339(),
        });
        let envelope_text = envelope.to_string();

        // Let the TUI flip the status to `Working` *before* the child
        // spawns. Brief window between now and first stdout would
        // otherwise show `Ready` even though work is imminent.
        if ai_tx.send((idx, AIEvent::AutomatonStarted)).await.is_err() {
            return;
        }

        let start = std::time::Instant::now();
        let mut cmd = tokio::process::Command::new(&command);
        cmd.args(&args);
        if let Some(cwd) = cwd.as_ref() {
            cmd.current_dir(cwd);
        }
        for (k, v) in &env {
            cmd.env(k, v);
        }
        cmd.env(agent_ipc::ENV_AGENT_ID, session_id.to_string());
        cmd.env(agent_ipc::ENV_AGENT_SOCKET, &socket_path);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = ai_tx
                    .send((
                        idx,
                        AIEvent::AutomatonStderr {
                            line: format!("spawn failed: {e}"),
                        },
                    ))
                    .await;
                let _ = ai_tx
                    .send((
                        idx,
                        AIEvent::AutomatonExit {
                            code: None,
                            duration_ms: start.elapsed().as_millis(),
                            timed_out: false,
                        },
                    ))
                    .await;
                continue;
            }
        };

        // Deliver the envelope on stdin and close it so scripts see EOF.
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = stdin.write_all(envelope_text.as_bytes()).await {
                warn!(%e, agent_idx = idx, "failed to write envelope to automaton stdin");
            }
            let _ = stdin.write_all(b"\n").await;
            drop(stdin);
        }

        // Pipe stdout/stderr back as line-oriented events. Run both
        // pumps concurrently so a chatty stream can't starve the
        // other. Each pump finishes when the child closes its end.
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let ai_tx_out = ai_tx.clone();
        let ai_tx_err = ai_tx.clone();
        let out_task = tokio::spawn(async move {
            if let Some(out) = stdout {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if ai_tx_out
                        .send((idx, AIEvent::AutomatonStdout { line }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });
        let err_task = tokio::spawn(async move {
            if let Some(err) = stderr {
                let mut lines = BufReader::new(err).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if ai_tx_err
                        .send((idx, AIEvent::AutomatonStderr { line }))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            }
        });

        // Wait on the child, respecting cancel and timeout. `biased`
        // so cancel wins over a just-completed child when both fire
        // in the same tick. All three branches' futures are dropped
        // the moment one completes, which releases `&mut child` and
        // lets the follow-up kill/await path borrow it again.
        enum Outcome {
            Cancelled,
            TimedOut,
            Exited(Option<i32>),
        }
        let timeout_dur = timeout_seconds.map(|s| Duration::from_secs(s as u64));
        let outcome = tokio::select! {
            biased;
            _ = cancel_rx.recv() => Outcome::Cancelled,
            _ = async { tokio::time::sleep(timeout_dur.unwrap_or_default()).await }, if timeout_dur.is_some() => Outcome::TimedOut,
            exit = child.wait() => match exit {
                Ok(status) => Outcome::Exited(status.code()),
                Err(e) => {
                    warn!(%e, agent_idx = idx, "automaton child wait failed");
                    Outcome::Exited(None)
                }
            }
        };
        let (code, timed_out) = match outcome {
            Outcome::Cancelled => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                (None, false)
            }
            Outcome::TimedOut => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                (None, true)
            }
            Outcome::Exited(code) => (code, false),
        };

        // Wait for pipe pumps to drain any buffered output so lines
        // don't arrive after the `AutomatonExit` event that should
        // close the block.
        let _ = out_task.await;
        let _ = err_task.await;

        let _ = ai_tx
            .send((
                idx,
                AIEvent::AutomatonExit {
                    code,
                    duration_ms: start.elapsed().as_millis(),
                    timed_out,
                },
            ))
            .await;
    }
}

async fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut ai_rx: mpsc::Receiver<(usize, AIEvent)>,
    socket_rx: &mut mpsc::Receiver<AgentSocketEvent>,
    compaction_rx: &mut mpsc::Receiver<super::compaction::CompactionEvent>,
    mut memory_event_rx: tokio::sync::mpsc::UnboundedReceiver<super::memory_view::MemoryEvent>,
    workflow_event_rx: &mut tokio::sync::mpsc::UnboundedReceiver<crate::workflow::WorkflowEvent>,
    usage_event_rx: &mut tokio::sync::mpsc::UnboundedReceiver<UsageEvent>,
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
            Some(mem_ev) = memory_event_rx.recv() => {
                app.on_memory_event(mem_ev);
            }
            Some(wf_ev) = workflow_event_rx.recv() => {
                app.on_workflow_event(wf_ev);
            }
            Some(usage_ev) = usage_event_rx.recv() => {
                app.on_usage_event(usage_ev);
            }
            _ = spinner_tick.tick() => {
                // Workflow sidebar hygiene: drop any successful
                // instance whose linger timer has elapsed.
                let pruned = app.prune_completed_workflows();
                // Only redraw when something on-screen is actually
                // animating this tick. `frame_tick` drives the
                // thinking-row spinner, which renders *only* for the
                // selected agent in the Chat view. Any other Working
                // chat — e.g. a Jules worker polling every 10s while
                // the user is viewing a different pane or the Cores
                // view — has nothing animated per-frame, so forcing
                // a full 10 Hz redraw for it was pure churn. On a
                // long transcript that churn dominates the UI
                // budget and makes the whole TUI feel sluggish.
                let selected_is_working = matches!(app.active_view, ActiveView::Chat)
                    && app
                        .chats
                        .get(app.selected)
                        .map(|c| c.status == AgentStatus::Working)
                        .unwrap_or(false);
                let workflows_animating = app
                    .workflow_instances
                    .iter()
                    .any(|i| i.remove_at.is_some());
                if selected_is_working {
                    app.frame_tick = app.frame_tick.wrapping_add(1);
                } else if !pruned && !workflows_animating {
                    needs_redraw = false;
                }
                // else: a workflow is running out its linger timer —
                // redraw once per tick so the countdown advances.
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

        // Progress the graceful-shutdown state machine after every
        // event: sleep any agent that just returned to idle, and
        // raise `should_quit` when none are Working.
        app.check_shutdown_progress();

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
    use crate::config::{AciddogConfig, Agent, Automaton, ReloadAction, Team};
    use serde_json::json;

    #[test]
    fn team_block_splits_agents_and_automatons_into_labelled_sections() {
        let cfg = AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "the team".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
            },
            agents: vec![Agent {
                id: "planner".into(),
                role: "coordinates work".into(),
                role_description: None,
                model: None,
                instructions: "i".into(),
                can_message: None,
                can_create_workers: None,
                can_run_workflows: None,
                tools: None,
                ai_core: None,
                memory_banks: None,
                skills: None,
                compaction: None,
                supervisor: false,
                sysadmin: false,
                leader: false,
                group: None,
                on_reload: ReloadAction::Ignore,
            }],
            workers: vec![],
            automatons: vec![Automaton {
                id: "quokka".into(),
                role: "echo bot".into(),
                role_description: None,
                can_message: None,
                backend: crate::config::AutomatonBackend::Script {
                    command: "python3".into(),
                    args: vec![],
                    cwd: None,
                    env: vec![],
                    timeout_seconds: None,
                },
                supervisor: false,
                sysadmin: false,
                leader: false,
                group: None,
            }],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["t".into()],
        };
        let block = build_team_block(&cfg, &["planner", "quokka"], &[], true);

        assert!(block.contains("Agents you can message:"), "{block}");
        assert!(
            block.contains("- planner — coordinates work"),
            "agent entry missing: {block}"
        );
        assert!(
            block.contains("Automatons you can message"),
            "automaton section heading missing: {block}"
        );
        assert!(
            block.contains("- quokka — echo bot"),
            "automaton entry missing: {block}"
        );
        // The agent's own section must not shadow the automaton under
        // the same heading — the two entries live in different blocks.
        let agents_idx = block.find("Agents you can message:").unwrap();
        let autos_idx = block.find("Automatons you can message").unwrap();
        assert!(agents_idx < autos_idx, "section order wrong: {block}");
        assert!(
            !block[agents_idx..autos_idx].contains("quokka"),
            "quokka leaked into agent section: {block}"
        );
    }

    #[test]
    fn team_block_omits_automaton_heading_when_none_are_contactable() {
        let cfg = AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
            },
            agents: vec![Agent {
                id: "a".into(),
                role: "r".into(),
                role_description: None,
                model: None,
                instructions: "i".into(),
                can_message: None,
                can_create_workers: None,
                can_run_workflows: None,
                tools: None,
                ai_core: None,
                memory_banks: None,
                skills: None,
                compaction: None,
                supervisor: false,
                sysadmin: false,
                leader: false,
                group: None,
                on_reload: ReloadAction::Ignore,
            }],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["t".into()],
        };
        let block = build_team_block(&cfg, &["a"], &[], true);
        assert!(!block.contains("Automatons you can message"), "{block}");
    }

    #[test]
    fn team_block_lists_user_when_include_user_true() {
        let cfg = AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
            },
            agents: vec![],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["t".into()],
        };
        let block = build_team_block(&cfg, &[], &[], true);
        assert!(
            block.contains("People you can message:"),
            "people heading missing: {block}"
        );
        assert!(
            block.contains("- user — the human operator"),
            "user entry missing: {block}"
        );
    }

    #[test]
    fn team_block_omits_user_for_workers() {
        let cfg = AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: None,
            },
            agents: vec![Agent {
                id: "parent".into(),
                role: "r".into(),
                role_description: None,
                model: None,
                instructions: "i".into(),
                can_message: None,
                can_create_workers: None,
                can_run_workflows: None,
                tools: None,
                ai_core: None,
                memory_banks: None,
                skills: None,
                compaction: None,
                supervisor: false,
                sysadmin: false,
                leader: false,
                group: None,
                on_reload: ReloadAction::Ignore,
            }],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["t".into()],
        };
        let block = build_team_block(&cfg, &["parent"], &[], false);
        assert!(
            !block.contains("People you can message:"),
            "worker team block must not list user: {block}"
        );
        assert!(
            !block.contains("- user —"),
            "worker team block must not list user: {block}"
        );
    }

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
                    model: Some("m".into()),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    can_run_workflows: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                    supervisor: false,
                    sysadmin: false,
                    leader: false,
                    group: None,
                    on_reload: ReloadAction::Ignore,
                },
                Agent {
                    id: "b".into(),
                    role: "r".into(),
                    role_description: None,
                    model: Some("m".into()),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    can_run_workflows: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                    supervisor: false,
                    sysadmin: false,
                    leader: false,
                    group: None,
                    on_reload: ReloadAction::Ignore,
                },
                Agent {
                    id: "c".into(),
                    role: "r".into(),
                    role_description: None,
                    model: Some("m".into()),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    can_run_workflows: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                    supervisor: false,
                    sysadmin: false,
                    leader: false,
                    group: None,
                    on_reload: ReloadAction::Ignore,
                },
            ],
            workers: vec![],
            automatons: vec![],
            ai_cores: vec![crate::config::AiCore {
                id: "default".into(),
                kind: crate::config::AiCoreKind::ClaudeCode {
                    system_prompt: None,
                    model: None,
                    default_tools: None,
                },
                usage_provider: None,
            }],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
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
    fn q_types_in_chat_and_is_ignored_in_agents_and_views() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(!app.should_quit);
        assert_eq!(app.current_chat().unwrap().input, "q");

        app.focus = Focus::Agents;
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(!app.should_quit);
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(!app.should_quit);

        app.focus = Focus::Views;
        app.handle_key(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(!app.should_quit);
    }

    #[test]
    fn ctrl_c_no_longer_quits() {
        let mut app = App::new(sample_config());
        app.focus = Focus::Chat;
        app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!app.should_quit);
        app.focus = Focus::Agents;
        app.handle_key(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(!app.should_quit);
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

        // Visual row 0 is the `(ungrouped)` header (sample_config has
        // no `group` set on its agents). Visual rows 1/2/3 are the
        // three agents — clicking row 5 (rect.y=5) + offset 2 = ev.row
        // 7 lands on the second chat (idx 1). Pre-grouping this test
        // asserted idx 2, but the renderer always inserted that
        // header; the click handler just wasn't accounting for it
        // (the user-reported "wrong agent" bug).
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 15,
            row: 7,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.selected, 1);
        assert_eq!(app.focus, Focus::Chat);
    }

    #[test]
    fn click_past_group_header_selects_correct_agent() {
        // Two agents in one named group + one in another → three chat
        // rows but two header rows interleaved, so the agent-list
        // layout is:
        //   row 0: ── alpha ──
        //   row 1: agent_a       (clickable)
        //   row 2: agent_b       (clickable)
        //   row 3: ── beta ──
        //   row 4: agent_c       (clickable)
        // Clicking row 4 must land on agent_c (idx 2), not on the
        // second header (no-op) or on agent_b shifted by one.
        let mut a = leader_test_agent("agent_a");
        let mut b = leader_test_agent("agent_b");
        let mut c = leader_test_agent("agent_c");
        a.group = Some("alpha".into());
        b.group = Some("alpha".into());
        c.group = Some("beta".into());
        let cfg = leader_test_config(vec![a, b, c], vec![]);
        let mut app = App::new(cfg);
        app.layout.agent_list_inner = Some(Rect::new(0, 0, 30, 10));

        // Click the row showing agent_c — visual row 4, ev.row 4.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 4,
            modifiers: KeyModifiers::NONE,
        });
        let c_idx = app
            .chats
            .iter()
            .position(|c| c.display_name == "agent_c")
            .expect("agent_c chat exists");
        assert_eq!(app.selected, c_idx, "click on row 4 must select agent_c");

        // Clicking the second group header (visual row 3) must NOT
        // change the selection — headers are inert dividers.
        app.selected = c_idx;
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.selected, c_idx, "header click is a no-op");
        assert_eq!(app.focus, Focus::Agents);
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
    fn scroll_wheel_in_inbox_moves_cursor() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Inbox;
        app.layout.inbox_list_inner = Some(Rect::new(0, 0, 80, 10));
        app.inbox.push(InboxMessage::new("a", "one"));
        app.inbox.push(InboxMessage::new("b", "two"));
        app.inbox.push(InboxMessage::new("c", "three"));
        app.inbox_screen.cursor = 0;

        // Wheel down inside the rect advances the cursor one row per tick.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.inbox_screen.cursor, 1);
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.inbox_screen.cursor, 2);
        // Clamps at the last item.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.inbox_screen.cursor, 2);

        // Wheel up walks the cursor back and clamps at zero.
        for _ in 0..5 {
            app.handle_mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 5,
                row: 5,
                modifiers: KeyModifiers::NONE,
            });
        }
        assert_eq!(app.inbox_screen.cursor, 0);
    }

    #[test]
    fn scroll_wheel_outside_inbox_list_is_ignored() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Inbox;
        app.layout.inbox_list_inner = Some(Rect::new(0, 0, 80, 10));
        app.inbox.push(InboxMessage::new("a", "one"));
        app.inbox.push(InboxMessage::new("b", "two"));
        app.inbox_screen.cursor = 0;

        // Position outside both rects — cursor and detail_scroll
        // must stay put.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 30,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.inbox_screen.cursor, 0);
        assert_eq!(app.inbox_screen.detail_scroll, 0);
    }

    #[test]
    fn scroll_wheel_in_inbox_detail_scrolls_body_not_cursor() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Inbox;
        app.layout.inbox_list_inner = Some(Rect::new(0, 0, 80, 10));
        app.layout.inbox_detail_inner = Some(Rect::new(0, 20, 80, 8));
        app.inbox.push(InboxMessage::new(
            "a",
            // Body taller than the pane so scrolling has somewhere to go.
            (0..50)
                .map(|i| format!("line {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
        app.inbox_screen.cursor = 0;

        // Wheel down inside the detail rect scrolls the body, three rows
        // per tick — cursor must stay put.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 22,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.inbox_screen.cursor, 0);
        assert_eq!(app.inbox_screen.detail_scroll, 3);

        // Wheel up rolls it back, clamps at zero.
        for _ in 0..5 {
            app.handle_mouse(MouseEvent {
                kind: MouseEventKind::ScrollUp,
                column: 5,
                row: 22,
                modifiers: KeyModifiers::NONE,
            });
        }
        assert_eq!(app.inbox_screen.detail_scroll, 0);
    }

    #[test]
    fn moving_inbox_cursor_resets_detail_scroll() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Inbox;
        app.layout.inbox_list_inner = Some(Rect::new(0, 0, 80, 10));
        app.inbox.push(InboxMessage::new("a", "one"));
        app.inbox.push(InboxMessage::new("b", "two"));
        app.inbox_screen.cursor = 0;
        app.inbox_screen.detail_scroll = 17;

        // Wheel-down in the list moves the selection — detail_scroll
        // resets to 0 so the next message starts at the top.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.inbox_screen.cursor, 1);
        assert_eq!(app.inbox_screen.detail_scroll, 0);
    }

    #[test]
    fn tab_in_inbox_cycles_list_to_detail_then_outer() {
        use super::super::inbox_view::{InboxMessage, InboxPane};
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Inbox;
        app.focus = Focus::Inbox;
        app.inbox.push(InboxMessage::new("a", "one"));
        assert_eq!(app.inbox_screen.pane, InboxPane::List);

        // First Tab: stay on Focus::Inbox, switch to Detail pane.
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Inbox);
        assert_eq!(app.inbox_screen.pane, InboxPane::Detail);

        // Second Tab: leave the inbox view; pane resets to List for
        // when the user comes back.
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE);
        assert_eq!(app.focus, Focus::Agents);
        assert_eq!(app.inbox_screen.pane, InboxPane::List);
    }

    #[test]
    fn keyboard_jk_in_detail_pane_scrolls_body() {
        use super::super::inbox_view::{InboxMessage, InboxPane};
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Inbox;
        app.focus = Focus::Inbox;
        app.inbox.push(InboxMessage::new(
            "a",
            (0..50)
                .map(|i| format!("line {i}"))
                .collect::<Vec<_>>()
                .join("\n"),
        ));
        app.inbox_screen.pane = InboxPane::Detail;

        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.inbox_screen.detail_scroll, 1);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.inbox_screen.detail_scroll, 3);
        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.inbox_screen.detail_scroll, 2);
        // Cursor must not have moved while the detail pane was focused.
        assert_eq!(app.inbox_screen.cursor, 0);
    }

    #[test]
    fn esc_from_detail_pane_returns_to_list_pane() {
        use super::super::inbox_view::{InboxMessage, InboxPane};
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Inbox;
        app.focus = Focus::Inbox;
        app.inbox.push(InboxMessage::new("a", "x"));
        app.inbox_screen.pane = InboxPane::Detail;

        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        // Still in the inbox view, just back on the list pane.
        assert_eq!(app.active_view, ActiveView::Inbox);
        assert_eq!(app.focus, Focus::Inbox);
        assert_eq!(app.inbox_screen.pane, InboxPane::List);

        // Second Esc actually leaves the inbox view.
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.active_view, ActiveView::Chat);
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
        chat.blocks.push(ChatBlock::user("line\n".repeat(200)));
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
        chat.blocks.push(ChatBlock::user("line\n".repeat(200)));
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

    /// Wheel-down inside the inbox detail-pane zone advances
    /// `detail_scroll` by 3 rows per tick (mirrors chat-history feel).
    #[test]
    fn inbox_wheel_scrolls_detail_pane() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.layout.inbox_detail_inner = Some(Rect::new(0, 20, 80, 8));
        app.active_view = ActiveView::Inbox;
        app.inbox
            .push(InboxMessage::new("a", "line\n".repeat(50)));
        app.inbox_screen.cursor = 0;
        app.inbox_screen.detail_scroll = 0;

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(app.inbox_screen.detail_scroll, 3);
    }

    /// Wheel-down past the body height clamps to
    /// `inbox_detail_max_scroll()` so the user can't overshoot.
    #[test]
    fn inbox_wheel_clamps_at_max() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.layout.inbox_detail_inner = Some(Rect::new(0, 20, 80, 8));
        app.active_view = ActiveView::Inbox;
        app.inbox
            .push(InboxMessage::new("a", "line\n".repeat(50)));
        app.inbox_screen.cursor = 0;
        // Pre-stuff above any reasonable max; the .min() clamp must
        // pin detail_scroll to max_scroll on the next ScrollDown tick.
        app.inbox_screen.detail_scroll = 100;
        let max = app.inbox_detail_max_scroll();

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(app.inbox_screen.detail_scroll, max);
        // Sanity: 50 body lines + 2 header rows − 8 inner rows = 44.
        assert_eq!(max, 44);
    }

    /// Wheel-up at offset 0 stays at 0 (saturating_sub is the floor).
    #[test]
    fn inbox_wheel_clamps_at_zero() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.layout.inbox_detail_inner = Some(Rect::new(0, 20, 80, 8));
        app.active_view = ActiveView::Inbox;
        app.inbox
            .push(InboxMessage::new("a", "line\n".repeat(50)));
        app.inbox_screen.cursor = 0;
        app.inbox_screen.detail_scroll = 0;

        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 5,
            row: 25,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(app.inbox_screen.detail_scroll, 0);
    }

    /// Wheel events outside the detail-pane rect (e.g. above the
    /// inbox-tab area or while a Reply/Forward modal is up — modals
    /// leave `inbox_detail_inner` at None per the per-frame reset)
    /// must be ignored.
    #[test]
    fn inbox_wheel_outside_detail_zone_is_ignored() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.layout.inbox_detail_inner = Some(Rect::new(0, 20, 80, 8));
        app.active_view = ActiveView::Inbox;
        app.inbox
            .push(InboxMessage::new("a", "line\n".repeat(50)));
        app.inbox_screen.cursor = 0;
        app.inbox_screen.detail_scroll = 5;

        // row 5 is above the detail rect at y=20.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 5,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });

        assert_eq!(app.inbox_screen.detail_scroll, 5);
    }

    /// Moving the cursor to a different message resets
    /// `detail_scroll` to 0 so the new message renders from its
    /// top. Locks the R2 reset semantics across all cursor-move
    /// sites (j/k, mouse SelectRow, programmatic select).
    #[test]
    fn inbox_detail_scroll_resets_on_message_change() {
        use super::super::inbox_view::InboxMessage;
        let mut app = App::new(sample_config());
        app.inbox.push(InboxMessage::new("a", "first body"));
        app.inbox.push(InboxMessage::new("b", "second body"));
        app.inbox_screen.cursor = 0;
        app.inbox_screen.detail_scroll = 10;

        app.inbox_screen.select(1, app.inbox.len());

        assert_eq!(app.inbox_screen.cursor, 1);
        assert_eq!(app.inbox_screen.detail_scroll, 0);
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
    async fn agent_messages_preempt_queued_channel_messages() {
        // Extended biased select in run_core must drain user_rx first,
        // then agent_rx, and only then channel_rx. A queued channel
        // message must never jump ahead of a pending mailbox message.
        let (user_tx, mut user_rx) = mpsc::unbounded_channel::<String>();
        let (agent_tx, mut agent_rx) = mpsc::channel::<String>(16);
        let (channel_tx, mut channel_rx) = mpsc::channel::<String>(16);

        channel_tx.send("channel-first".into()).await.unwrap();
        agent_tx.send("agent-first".into()).await.unwrap();
        user_tx.send("user-first".into()).unwrap();

        let first = tokio::select! {
            biased;
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            Some(m) = channel_rx.recv() => m,
            else => unreachable!(),
        };
        let second = tokio::select! {
            biased;
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            Some(m) = channel_rx.recv() => m,
            else => unreachable!(),
        };
        let third = tokio::select! {
            biased;
            Some(m) = user_rx.recv() => m,
            Some(m) = agent_rx.recv() => m,
            Some(m) = channel_rx.recv() => m,
            else => unreachable!(),
        };
        assert_eq!(first, "user-first");
        assert_eq!(second, "agent-first");
        assert_eq!(third, "channel-first");
    }

    // --------- channel broadcast narrowing ----------

    /// Build a sample config with a channel containing all three
    /// agents, wire per-agent `channel_tx`/`_rx` pairs, and seed
    /// `App::channels` by hand. Returns the bounded receivers in
    /// agent-index order so tests can `try_recv()` without blocking.
    fn sample_config_with_channel() -> AciddogConfig {
        let mut cfg = sample_config();
        cfg.channels = vec![crate::config::Channel {
            id: "room".into(),
            description: None,
            welcome: None,
            members: vec!["a".into(), "b".into(), "c".into()],
        }];
        cfg
    }

    fn wire_up_channels(app: &mut App) -> Vec<mpsc::Receiver<String>> {
        let mut rxs = Vec::new();
        for i in 0..app.chats.len() {
            let (tx, rx) = mpsc::channel::<String>(16);
            app.chats[i].channel_tx = Some(tx);
            rxs.push(rx);
        }
        // `App::new` populates `channels` from `config.channels`, but
        // let's make sure the state is present in case the helper is
        // called on a freshly-mutated config.
        if app.channels.is_empty() {
            app.channels = app
                .config
                .channels
                .iter()
                .map(|c| ChannelState {
                    id: c.id.clone(),
                    members: c.members.clone(),
                    sticky_bottom: true,
                    ..Default::default()
                })
                .collect();
        }
        rxs
    }

    /// Assert that a delivered channel payload starts with the
    /// expected `<ChannelMessage …>body</ChannelMessage>` opening and
    /// includes the system reminder that points agents at the
    /// `channel send` tool. The exact reminder wording may evolve so
    /// we only check for its load-bearing fragments.
    fn assert_channel_payload(payload: &str, expected_head: &str, channel_id: &str) {
        assert!(
            payload.starts_with(expected_head),
            "payload should start with {expected_head:?}, got {payload:?}"
        );
        assert!(
            payload.contains("<system-reminder>"),
            "payload should carry a system-reminder: {payload:?}"
        );
        assert!(
            payload.contains("channel send"),
            "reminder should name the `channel send` tool: {payload:?}"
        );
        assert!(
            payload.contains(&format!("id=\"{channel_id}\"")),
            "reminder should include the channel id: {payload:?}"
        );
    }

    #[tokio::test]
    async fn broadcast_without_mentions_goes_to_every_other_member() {
        let mut app = App::new(sample_config_with_channel());
        let mut rxs = wire_up_channels(&mut app);

        let (reply_tx, reply_rx) = oneshot::channel();
        app.broadcast_to_channel(
            ChannelSender::Agent("a".into()),
            "room".into(),
            "hi team".into(),
            reply_tx,
        );
        assert!(reply_rx.await.unwrap().ok);

        // Sender `a` stays silent; `b` and `c` both receive.
        assert!(rxs[0].try_recv().is_err(), "sender should not self-echo");
        let b = rxs[1].recv().await.expect("b receives");
        let c = rxs[2].recv().await.expect("c receives");
        let head = r#"<ChannelMessage channel="room" from="a">hi team</ChannelMessage>"#;
        assert_channel_payload(&b, head, "room");
        assert_channel_payload(&c, head, "room");
    }

    #[tokio::test]
    async fn broadcast_with_single_mention_narrows_to_that_member() {
        let mut app = App::new(sample_config_with_channel());
        let mut rxs = wire_up_channels(&mut app);

        let (reply_tx, reply_rx) = oneshot::channel();
        app.broadcast_to_channel(
            ChannelSender::Agent("a".into()),
            "room".into(),
            "@b heads up".into(),
            reply_tx,
        );
        assert!(reply_rx.await.unwrap().ok);

        // Only `b` receives; `c` is skipped even though they're a
        // channel member.
        assert!(rxs[0].try_recv().is_err());
        let payload = rxs[1].recv().await.expect("b receives");
        assert_channel_payload(
            &payload,
            r#"<ChannelMessage channel="room" from="a">@b heads up</ChannelMessage>"#,
            "room",
        );
        assert!(rxs[2].try_recv().is_err(), "c should not receive");
    }

    #[tokio::test]
    async fn broadcast_with_multiple_mentions_narrows_to_listed_members() {
        let mut app = App::new(sample_config_with_channel());
        let mut rxs = wire_up_channels(&mut app);

        let (reply_tx, reply_rx) = oneshot::channel();
        app.broadcast_to_channel(
            ChannelSender::User,
            "room".into(),
            "@b @c ship it".into(),
            reply_tx,
        );
        assert!(reply_rx.await.unwrap().ok);

        // User is not a channel member, so every mentioned member
        // receives (self-echo rule only applies to agents).
        let b = rxs[1].recv().await.expect("b receives");
        let c = rxs[2].recv().await.expect("c receives");
        let head = r#"<ChannelMessage channel="room" from="user">@b @c ship it</ChannelMessage>"#;
        assert_channel_payload(&b, head, "room");
        assert_channel_payload(&c, head, "room");
        assert!(rxs[0].try_recv().is_err(), "a should not receive");
    }

    #[tokio::test]
    async fn broadcast_rejects_mention_of_non_member() {
        let mut app = App::new(sample_config_with_channel());
        let mut rxs = wire_up_channels(&mut app);

        let (reply_tx, reply_rx) = oneshot::channel();
        app.broadcast_to_channel(
            ChannelSender::User,
            "room".into(),
            "@ghost hello".into(),
            reply_tx,
        );
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(
            resp.error.as_deref().unwrap_or("").contains("@ghost"),
            "error should cite the bad mention: {resp:?}"
        );
        // No recipients should have received anything.
        for rx in rxs.iter_mut() {
            assert!(rx.try_recv().is_err());
        }
    }

    #[tokio::test]
    async fn broadcast_with_self_mention_skips_sender_echo() {
        let mut app = App::new(sample_config_with_channel());
        let mut rxs = wire_up_channels(&mut app);

        let (reply_tx, reply_rx) = oneshot::channel();
        app.broadcast_to_channel(
            ChannelSender::Agent("a".into()),
            "room".into(),
            "@a @b note to self".into(),
            reply_tx,
        );
        assert!(reply_rx.await.unwrap().ok);

        // `@a` in the recipient list is filtered out by the sender-
        // skip rule; only `b` receives.
        assert!(rxs[0].try_recv().is_err(), "sender should not self-echo");
        let b = rxs[1].recv().await.expect("b receives");
        assert_channel_payload(
            &b,
            r#"<ChannelMessage channel="room" from="a">@a @b note to self</ChannelMessage>"#,
            "room",
        );
        assert!(rxs[2].try_recv().is_err());
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

    // --------- @mention parser ----------

    #[test]
    fn parse_leading_mentions_empty_when_no_at() {
        assert!(parse_leading_mentions("hello world").is_empty());
        assert!(parse_leading_mentions("").is_empty());
        assert!(parse_leading_mentions("ping @alice later").is_empty());
    }

    #[test]
    fn parse_leading_mentions_extracts_single_name() {
        assert_eq!(parse_leading_mentions("@alice hi"), vec!["alice"]);
    }

    #[test]
    fn parse_leading_mentions_extracts_multiple_names() {
        assert_eq!(
            parse_leading_mentions("@alice @bob @carol hi team"),
            vec!["alice", "bob", "carol"]
        );
    }

    #[test]
    fn parse_leading_mentions_tolerates_extra_whitespace() {
        assert_eq!(
            parse_leading_mentions("  @alice   @bob   hi"),
            vec!["alice", "bob"]
        );
    }

    #[test]
    fn parse_leading_mentions_ignores_bare_at() {
        assert!(parse_leading_mentions("@ hi").is_empty());
        assert_eq!(parse_leading_mentions("@alice @ later"), vec!["alice"]);
    }

    #[test]
    fn parse_leading_mentions_stops_at_first_non_mention() {
        // Once a non-`@` token appears the scan ends, even if a
        // later `@name` shows up in the prose.
        assert_eq!(
            parse_leading_mentions("@alice here's the plan @bob"),
            vec!["alice"]
        );
    }

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
    async fn send_message_to_user_pushes_to_inbox() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();
        assert!(app.inbox.is_empty());

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "user".into(),
                    body: "hello operator".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        assert_eq!(app.inbox.len(), 1);
        assert_eq!(app.inbox[0].from, "a");
        assert_eq!(app.inbox[0].body, "hello operator");
    }

    #[tokio::test]
    async fn send_message_to_user_does_not_push_mail_into_any_chat() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender_uuid = app.chats[0].session_id.unwrap();
        let sizes_before: Vec<usize> = app.chats.iter().map(|c| c.blocks.len()).collect();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "user".into(),
                    body: "fyi".into(),
                },
            },
            reply: reply_tx,
        });
        assert!(reply_rx.await.unwrap().ok);

        let sizes_after: Vec<usize> = app.chats.iter().map(|c| c.blocks.len()).collect();
        assert_eq!(
            sizes_before, sizes_after,
            "user-routed mail must not append to any agent chat"
        );
    }

    #[tokio::test]
    async fn workers_cannot_send_to_user() {
        // Workers are scoped to their parent; the user inbox is not
        // a valid target for them. Sender-parent check rejects before
        // the inbox push.
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        // Turn chat index 1 (agent "b") into a pretend-worker by
        // pointing its parent_idx at chat 0. In a real run, worker
        // entries are separate AgentChats but the ACL check only
        // inspects parent_idx.
        app.chats[1].parent_idx = Some(0);
        let worker_uuid = app.chats[1].session_id.unwrap();

        let (reply_tx, reply_rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: worker_uuid,
                cmd: RequestCmd::SendMessage {
                    to: "user".into(),
                    body: "nope".into(),
                },
            },
            reply: reply_tx,
        });
        let resp = reply_rx.await.unwrap();
        assert!(!resp.ok);
        assert!(app.inbox.is_empty());
    }

    #[test]
    fn inbox_reply_deletes_source_message_on_successful_send() {
        use super::super::inbox_view::{InboxMessage, InboxMode};
        let mut app = App::new(sample_config());
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        // Agent "b" is index 1 in sample_config.
        app.chats[1].user_tx = Some(tx);
        app.inbox.push(InboxMessage::new("b", "how's it going"));
        app.open_inbox_view();
        app.inbox_screen.mode = InboxMode::Reply {
            to: "b".into(),
            body: "  all good  ".into(),
        };
        app.submit_inbox_send();
        assert!(app.inbox.is_empty(), "source message should be removed");
        assert!(matches!(app.inbox_screen.mode, InboxMode::List));
        assert_eq!(rx.try_recv().unwrap(), "all good");
    }

    #[test]
    fn inbox_reply_keeps_source_message_when_send_fails() {
        use super::super::inbox_view::{InboxMessage, InboxMode};
        let mut app = App::new(sample_config());
        // No user_tx wired — send will fail with "not online".
        app.inbox.push(InboxMessage::new("b", "hi"));
        app.open_inbox_view();
        app.inbox_screen.mode = InboxMode::Reply {
            to: "b".into(),
            body: "hello".into(),
        };
        app.submit_inbox_send();
        assert_eq!(app.inbox.len(), 1, "failed send must not delete the source");
        assert!(matches!(app.inbox_screen.mode, InboxMode::Reply { .. }));
        assert!(app.inbox_screen.status_msg.is_some());
    }

    #[test]
    fn inbox_mouse_click_selects_row() {
        use super::super::inbox_view::{InboxHotspot, InboxMessage};
        let mut app = App::new(sample_config());
        app.inbox.push(InboxMessage::new("b", "first"));
        app.inbox.push(InboxMessage::new("a", "second"));
        app.open_inbox_view();
        // Simulate the row hotspots the renderer would publish.
        app.layout
            .inbox_hotspots
            .push((Rect::new(0, 10, 80, 1), InboxHotspot::SelectRow(1)));
        assert!(app.handle_mouse_inbox((5, 10)));
        assert_eq!(app.inbox_screen.cursor, 1);
    }

    #[test]
    fn inbox_mouse_click_reply_button_opens_modal() {
        use super::super::inbox_view::{InboxHotspot, InboxMessage, InboxMode};
        let mut app = App::new(sample_config());
        app.inbox.push(InboxMessage::new("b", "hi"));
        app.open_inbox_view();
        app.layout
            .inbox_hotspots
            .push((Rect::new(0, 0, 10, 1), InboxHotspot::ReplyButton));
        assert!(app.handle_mouse_inbox((3, 0)));
        assert!(matches!(
            app.inbox_screen.mode,
            InboxMode::Reply { ref to, .. } if to == "b"
        ));
    }

    #[test]
    fn inbox_mouse_click_delete_button_removes_message() {
        use super::super::inbox_view::{InboxHotspot, InboxMessage};
        let mut app = App::new(sample_config());
        app.inbox.push(InboxMessage::new("b", "first"));
        app.inbox.push(InboxMessage::new("a", "second"));
        app.open_inbox_view();
        app.inbox_screen.cursor = 0;
        app.layout
            .inbox_hotspots
            .push((Rect::new(0, 0, 10, 1), InboxHotspot::DeleteButton));
        assert!(app.handle_mouse_inbox((3, 0)));
        assert_eq!(app.inbox.len(), 1);
        assert_eq!(app.inbox[0].from, "a");
    }

    #[test]
    fn inbox_mouse_click_cancel_closes_modal() {
        use super::super::inbox_view::{InboxHotspot, InboxMessage, InboxMode};
        let mut app = App::new(sample_config());
        app.inbox.push(InboxMessage::new("b", "hi"));
        app.open_inbox_view();
        app.inbox_screen.mode = InboxMode::Reply {
            to: "b".into(),
            body: "draft".into(),
        };
        app.layout
            .inbox_hotspots
            .push((Rect::new(0, 0, 10, 1), InboxHotspot::CancelButton));
        assert!(app.handle_mouse_inbox((3, 0)));
        assert!(matches!(app.inbox_screen.mode, InboxMode::List));
    }

    #[test]
    fn inbox_mouse_click_forward_recipient_advances_to_edit_body() {
        use super::super::inbox_view::{
            ForwardStage, ForwardState, InboxHotspot, InboxMessage, InboxMode,
        };
        let mut app = App::new(sample_config());
        app.inbox.push(InboxMessage::new("b", "original"));
        app.open_inbox_view();
        app.inbox_screen.mode = InboxMode::Forward(ForwardState {
            candidates: vec!["a".into(), "b".into()],
            selected: 0,
            stage: ForwardStage::PickRecipient,
            note: String::new(),
            quoted: String::new(),
        });
        app.layout
            .inbox_hotspots
            .push((Rect::new(0, 5, 20, 1), InboxHotspot::ForwardRecipient(1)));
        assert!(app.handle_mouse_inbox((1, 5)));
        match &app.inbox_screen.mode {
            InboxMode::Forward(fwd) => {
                assert_eq!(fwd.selected, 1);
                assert!(matches!(fwd.stage, ForwardStage::EditBody));
                assert!(
                    fwd.quoted.starts_with("Forwarded from b:"),
                    "quoted should be pre-filled with the original: {}",
                    fwd.quoted
                );
                assert!(fwd.note.is_empty(), "note should start blank");
            }
            other => panic!("expected Forward mode, got {other:?}"),
        }
    }

    #[test]
    fn forward_compose_body_combines_note_and_quoted() {
        use super::super::inbox_view::{ForwardStage, ForwardState};
        let fwd = ForwardState {
            candidates: vec!["a".into()],
            selected: 0,
            stage: ForwardStage::EditBody,
            note: "FYI — relevant to your investigation\n".into(),
            quoted: "Forwarded from b:\n\nthe body".into(),
        };
        let composed = fwd.compose_body();
        assert!(composed.starts_with("FYI — relevant to your investigation"));
        assert!(composed.contains("Forwarded from b:\n\nthe body"));
        // No triple newline between note and quoted block.
        assert!(!composed.contains("\n\n\n"));
    }

    #[test]
    fn forward_compose_body_emits_only_quoted_when_note_blank() {
        use super::super::inbox_view::{ForwardStage, ForwardState};
        let fwd = ForwardState {
            candidates: vec!["a".into()],
            selected: 0,
            stage: ForwardStage::EditBody,
            note: "   ".into(),
            quoted: "Forwarded from b:\n\nbody".into(),
        };
        assert_eq!(fwd.compose_body(), "Forwarded from b:\n\nbody");
    }

    #[test]
    fn forward_keypress_writes_to_note_not_quoted() {
        use super::super::inbox_view::{ForwardStage, ForwardState, InboxMessage, InboxMode};
        let mut app = App::new(sample_config());
        app.inbox.push(InboxMessage::new("b", "original"));
        app.open_inbox_view();
        app.inbox_screen.mode = InboxMode::Forward(ForwardState {
            candidates: vec!["a".into()],
            selected: 0,
            stage: ForwardStage::EditBody,
            note: String::new(),
            quoted: "Forwarded from b:\n\noriginal".into(),
        });
        app.focus = Focus::Inbox;
        for c in "hi".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        match &app.inbox_screen.mode {
            InboxMode::Forward(fwd) => {
                assert_eq!(fwd.note, "hi");
                assert_eq!(fwd.quoted, "Forwarded from b:\n\noriginal");
            }
            other => panic!("expected Forward mode, got {other:?}"),
        }
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
                    model: Some("m".into()),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: Some(vec!["researcher".into()]),
                    can_run_workflows: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                    supervisor: false,
                    sysadmin: false,
                    leader: false,
                    group: None,
                    on_reload: ReloadAction::Ignore,
                },
                Agent {
                    id: "planner".into(),
                    role: "r".into(),
                    role_description: None,
                    model: Some("m".into()),
                    instructions: "i".into(),
                    can_message: None,
                    can_create_workers: None,
                    can_run_workflows: None,
                    tools: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                    supervisor: false,
                    sysadmin: false,
                    leader: false,
                    group: None,
                    on_reload: ReloadAction::Ignore,
                },
            ],
            workers: vec![
                Worker {
                    id: "researcher".into(),
                    role: "investigator".into(),
                    role_description: None,
                    model: Some("m".into()),
                    instructions: "look stuff up".into(),
                    tools: None,
                    can_create_workers: None,
                    can_run_workflows: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                    group: None,
                },
                Worker {
                    id: "fetcher".into(),
                    role: "fetcher".into(),
                    role_description: None,
                    model: Some("m".into()),
                    instructions: "fetch".into(),
                    tools: None,
                    can_create_workers: None,
                    can_run_workflows: None,
                    ai_core: None,
                    memory_banks: None,
                    skills: None,
                    compaction: None,
                    group: None,
                },
            ],
            automatons: vec![],
            ai_cores: vec![crate::config::AiCore {
                id: "default".into(),
                kind: crate::config::AiCoreKind::ClaudeCode {
                    system_prompt: None,
                    model: None,
                    default_tools: None,
                },
                usage_provider: None,
            }],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
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
                    properties: None,
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
                    properties: None,
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
                    properties: None,
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
        assert!(try_parse_aciddog_skill_run("aciddog skill run --name foo", "").is_none());
    }

    #[test]
    fn is_aciddog_skill_list_matches_only_list_invocations() {
        assert!(is_aciddog_skill_list(
            "/opt/aciddog skill list",
            "/opt/aciddog"
        ));
        assert!(!is_aciddog_skill_list(
            "/opt/aciddog skill run --name a",
            "/opt/aciddog"
        ));
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
        assert!(
            try_parse_aciddog_skill_search("/opt/aciddog skill run --name a", "/opt/aciddog")
                .is_none()
        );
        assert!(
            try_parse_aciddog_skill_search("/opt/aciddog skill search --query foo", "").is_none()
        );
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

    /// Build a minimal Agent fixture for visible_order tests. Defaults
    /// `leader = false`; callers flip the flag in-place.
    fn leader_test_agent(id: &str) -> Agent {
        Agent {
            id: id.into(),
            role: "r".into(),
            role_description: None,
            model: Some("m".into()),
            instructions: "i".into(),
            can_message: None,
            can_create_workers: None,
            can_run_workflows: None,
            tools: None,
            ai_core: None,
            memory_banks: None,
            skills: None,
            compaction: None,
            supervisor: false,
            sysadmin: false,
            leader: false,
            group: None,
            on_reload: ReloadAction::Ignore,
        }
    }

    fn leader_test_config(agents: Vec<Agent>, automatons: Vec<Automaton>) -> AciddogConfig {
        AciddogConfig {
            team: Team {
                id: "t".into(),
                description: "d".into(),
                replace_thinking_messages: false,
                default_ai_core: Some("default".into()),
            },
            agents,
            workers: vec![],
            automatons,
            ai_cores: vec![crate::config::AiCore {
                id: "default".into(),
                kind: crate::config::AiCoreKind::ClaudeCode {
                    system_prompt: None,
                    model: None,
                    default_tools: None,
                },
                usage_provider: None,
            }],
            memory_banks: vec![],
            compactions: vec![],
            channels: vec![],
            workflows: vec![],
            webhooks: vec![],
            thinking_messages: vec!["thinking".into()],
        }
    }

    #[test]
    fn visible_order_pins_leaders_to_top() {
        // [coder, planner, tester] in declaration order; leader=true
        // on tester. visible_order should yield tester first, then
        // coder, then planner — confirming leader-first + declaration-
        // order tiebreak (Wil decision 3).
        let mut coder = leader_test_agent("coder");
        let mut planner = leader_test_agent("planner");
        let mut tester = leader_test_agent("tester");
        coder.leader = false;
        planner.leader = false;
        tester.leader = true;
        let cfg = leader_test_config(vec![coder, planner, tester], vec![]);
        let app = App::new(cfg);

        // Resolve display-name -> chat index (order of insertion is
        // declaration order, so this lets the assertion be written in
        // terms of names rather than fragile indices).
        let idx = |name: &str| -> usize {
            app.chats
                .iter()
                .position(|c| c.display_name == name)
                .unwrap_or_else(|| panic!("no chat named {name}"))
        };
        let order: Vec<usize> = app.visible_order().into_iter().map(|(i, _)| i).collect();
        assert_eq!(
            order,
            vec![idx("tester"), idx("coder"), idx("planner")],
            "leader (tester) must pin first, then non-leaders preserve declaration order"
        );
    }

    #[test]
    fn visible_order_pins_multiple_leaders_in_declaration_order() {
        // [coder, planner, tester] with leader on coder AND tester.
        // Expect [coder, tester, planner] — both leaders bucket-first
        // with declaration order preserved within the bucket.
        let mut coder = leader_test_agent("coder");
        let mut planner = leader_test_agent("planner");
        let mut tester = leader_test_agent("tester");
        coder.leader = true;
        planner.leader = false;
        tester.leader = true;
        let cfg = leader_test_config(vec![coder, planner, tester], vec![]);
        let app = App::new(cfg);

        let idx = |name: &str| -> usize {
            app.chats
                .iter()
                .position(|c| c.display_name == name)
                .unwrap_or_else(|| panic!("no chat named {name}"))
        };
        let order: Vec<usize> = app.visible_order().into_iter().map(|(i, _)| i).collect();
        assert_eq!(
            order,
            vec![idx("coder"), idx("tester"), idx("planner")],
            "tied leaders must preserve declaration order"
        );
    }

    #[test]
    fn visible_order_pins_automaton_leader() {
        // Scope-expansion case: automatons are leader-eligible. Build
        // one agent (non-leader) plus one automaton with leader=true
        // and assert the automaton pins to the top above the agent.
        let cfg = leader_test_config(
            vec![leader_test_agent("coder")],
            vec![Automaton {
                id: "portal".into(),
                role: "manager portal".into(),
                role_description: None,
                can_message: None,
                backend: crate::config::AutomatonBackend::Script {
                    command: "python3".into(),
                    args: vec![],
                    cwd: None,
                    env: vec![],
                    timeout_seconds: None,
                },
                supervisor: false,
                sysadmin: false,
                leader: true,
                group: None,
            }],
        );
        let app = App::new(cfg);

        let idx = |name: &str| -> usize {
            app.chats
                .iter()
                .position(|c| c.display_name == name)
                .unwrap_or_else(|| panic!("no chat named {name}"))
        };
        let order: Vec<usize> = app.visible_order().into_iter().map(|(i, _)| i).collect();
        assert_eq!(
            order,
            vec![idx("portal"), idx("coder")],
            "leader-flagged automaton must pin above non-leader agent"
        );
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

    /// Mutate `sample_config()` to put `b` in group "engineering" with
    /// `leader = true`, `c` in the same group as a non-leader, and
    /// leave `a` ungrouped. Used by group-assignment tests below.
    fn sample_config_with_engineering_group() -> AciddogConfig {
        let mut cfg = sample_config();
        cfg.agents[1].group = Some("engineering".into());
        cfg.agents[1].leader = true;
        cfg.agents[2].group = Some("engineering".into());
        cfg
    }

    #[tokio::test]
    async fn task_add_to_group_notifies_group_leader() {
        let mut app = App::new(sample_config_with_engineering_group());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let leader_blocks_before = app.chats[1].blocks.len();
        let other_blocks_before = app.chats[2].blocks.len();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::TaskAdd {
                    title: "wire up auth".into(),
                    description: None,
                    assignee: Some("engineering".into()),
                    blocked_by: vec![],
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok, "error: {:?}", resp.error);

        // Task is recorded with the group label as the literal assignee.
        let id = parse_task_created(resp.message.as_deref().unwrap());
        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.assignee.as_deref(), Some("engineering"));

        // Leader (chat[1] = "b") gets a system block; non-leader
        // member (chat[2] = "c") does NOT.
        assert_eq!(
            app.chats[1].blocks.len(),
            leader_blocks_before + 1,
            "leader should receive a notification"
        );
        assert_eq!(
            app.chats[2].blocks.len(),
            other_blocks_before,
            "non-leader group member must not receive a notification"
        );
        match app.chats[1].blocks.last().unwrap() {
            ChatBlock::System { text, .. } => {
                assert!(
                    text.contains("assigned to group engineering by a"),
                    "got: {text}"
                );
                assert!(text.contains("wire up auth"), "got: {text}");
            }
            other => panic!("expected System block, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn task_add_to_leaderless_group_succeeds_without_notification() {
        let mut cfg = sample_config();
        // engineering exists but has no leader.
        cfg.agents[1].group = Some("engineering".into());
        cfg.agents[2].group = Some("engineering".into());
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let b_before = app.chats[1].blocks.len();
        let c_before = app.chats[2].blocks.len();

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::TaskAdd {
                    title: "no leader".into(),
                    description: None,
                    assignee: Some("engineering".into()),
                    blocked_by: vec![],
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok, "task creation should succeed even without a leader");
        assert_eq!(app.chats[1].blocks.len(), b_before);
        assert_eq!(app.chats[2].blocks.len(), c_before);
    }

    #[tokio::test]
    async fn task_add_to_user_pushes_inbox_message() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        assert!(app.inbox.is_empty());

        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::TaskAdd {
                    title: "review PR".into(),
                    description: None,
                    assignee: Some(crate::config::USER_INBOX_ID.into()),
                    blocked_by: vec![],
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(resp.ok, "error: {:?}", resp.error);

        assert_eq!(app.inbox.len(), 1);
        let msg = &app.inbox[0];
        assert_eq!(msg.from, "a");
        assert!(msg.body.contains("assigned to you by a"), "got: {}", msg.body);
        assert!(msg.body.contains("review PR"), "got: {}", msg.body);
    }

    #[tokio::test]
    async fn task_update_reassign_to_group_notifies_new_leader() {
        let mut app = App::new(sample_config_with_engineering_group());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        // First create an unassigned task as `a`.
        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "initial".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());
        let leader_before = app.chats[1].blocks.len();

        // Now reassign to the engineering group.
        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskUpdate {
                    id,
                    title: None,
                    description: None,
                    status: None,
                    assignee: Some("engineering".into()),
                    unassign: false,
                    note: None,
                    add_blocked_by: vec![],
                    remove_blocked_by: vec![],
                },
            },
            reply: tx2,
        });
        let resp = rx2.await.unwrap();
        assert!(resp.ok, "error: {:?}", resp.error);

        let t = app.tasks.get(id).unwrap();
        assert_eq!(t.assignee.as_deref(), Some("engineering"));
        assert_eq!(
            app.chats[1].blocks.len(),
            leader_before + 1,
            "leader chat should grow by one system block"
        );
    }

    #[tokio::test]
    async fn task_update_reassign_to_user_pushes_inbox_message() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let a = app.chats[0].session_id.unwrap();

        // Create an unassigned task as `a`.
        let (tx1, rx1) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskAdd {
                    title: "for the human".into(),
                    description: None,
                    assignee: None,
                    blocked_by: vec![],
                },
            },
            reply: tx1,
        });
        let id = parse_task_created(rx1.await.unwrap().message.as_deref().unwrap());
        assert!(app.inbox.is_empty());

        // Reassign to user.
        let (tx2, rx2) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: a,
                cmd: RequestCmd::TaskUpdate {
                    id,
                    title: None,
                    description: None,
                    status: None,
                    assignee: Some(crate::config::USER_INBOX_ID.into()),
                    unassign: false,
                    note: None,
                    add_blocked_by: vec![],
                    remove_blocked_by: vec![],
                },
            },
            reply: tx2,
        });
        assert!(rx2.await.unwrap().ok);
        assert_eq!(app.inbox.len(), 1);
        assert!(app.inbox[0].body.contains("for the human"));
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
        assert!(
            !err.contains("not found"),
            "must not confuse not-found with not-allowed: {err}"
        );
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
                cmd: RequestCmd::SkillSearch {
                    query: "   ".into(),
                },
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
        assert!(
            !body.contains("- alpha —"),
            "alpha should not match: {body}"
        );

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
        assert!(
            body.ends_with(md),
            "body must contain verbatim SKILL.md contents: {body}"
        );
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
        // Navigate to the Tasks entry (Inbox sits at index 0, Tasks
        // at index 1 — `selected_view` is 0 at startup).
        let tasks_idx = VIEW_ENTRIES.iter().position(|n| *n == "Tasks").unwrap();
        for _ in 0..tasks_idx {
            app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        }
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
    fn enter_on_task_row_opens_edit_form() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(matches!(
            app.tasks_screen.form.as_ref().map(|f| f.kind),
            Some(TaskFormKind::Edit(_))
        ));
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.is_none());
    }

    #[test]
    fn edit_form_tab_order_includes_todos_and_comments() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        let order = app.tasks_screen.form.as_ref().unwrap().tab_order();
        assert!(order.contains(&FormField::Todos));
        assert!(order.contains(&FormField::Comments));
    }

    #[test]
    fn add_form_tab_order_excludes_todos_and_comments() {
        let mut app = App::new(sample_config());
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        let order = app.tasks_screen.form.as_ref().unwrap().tab_order();
        assert!(!order.contains(&FormField::Todos));
        assert!(!order.contains(&FormField::Comments));
    }

    #[test]
    fn edit_form_todos_and_comments_default_collapsed_with_zero_scroll() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert!(form.todos_collapsed);
        assert!(form.comments_collapsed);
        assert_eq!(form.todos_scroll, 0);
        assert_eq!(form.comments_scroll, 0);
    }

    #[test]
    fn enter_on_focused_todos_field_toggles_collapse() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        // Focus the Todos field directly (avoids Description swallowing keys).
        app.tasks_screen.form.as_mut().unwrap().focused_field = FormField::Todos;
        assert!(app.tasks_screen.form.as_ref().unwrap().todos_collapsed);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(!app.tasks_screen.form.as_ref().unwrap().todos_collapsed);
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.form.as_ref().unwrap().todos_collapsed);
    }

    #[test]
    fn enter_on_focused_comments_field_toggles_collapse_and_resets_scroll() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Comments;
            // Pretend the user had scrolled within an earlier expansion.
            form.comments_scroll = 7;
        }
        // Toggling resets scroll so a re-expansion lands at the top.
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert!(!form.comments_collapsed);
        assert_eq!(form.comments_scroll, 0);
    }

    #[test]
    fn j_k_moves_todo_cursor_only_when_expanded() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        // Three todos so cursor has somewhere to walk.
        for body in ["one", "two", "three"] {
            app.tasks
                .add_todo(task_id, body.into(), "seed".into())
                .unwrap();
        }
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        app.tasks_screen.form.as_mut().unwrap().focused_field = FormField::Todos;

        // Collapsed: j/k are absorbed (no cursor movement, no text
        // mutation either since Todos is gated as a "section").
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().todos_cursor, 0);

        // Expand, walk down twice, then up once.
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().todos_cursor, 2);
        app.handle_key(KeyCode::Char('k'), KeyModifiers::NONE);
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().todos_cursor, 1);
        // Past the end is clamped.
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().todos_cursor, 2);
    }

    #[test]
    fn space_toggles_cursor_selected_todo_done() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        for body in ["one", "two"] {
            app.tasks
                .add_todo(task_id, body.into(), "seed".into())
                .unwrap();
        }
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        // Focus + expand Todos, then point cursor at the second todo.
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Todos;
            form.todos_collapsed = false;
            form.todos_cursor = 1;
        }

        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(
            app.tasks.get(task_id).unwrap().todos[1].status,
            crate::tasks::TodoStatus::Completed
        );

        app.handle_key(KeyCode::Char(' '), KeyModifiers::NONE);
        assert_eq!(
            app.tasks.get(task_id).unwrap().todos[1].status,
            crate::tasks::TodoStatus::Pending
        );
    }

    #[test]
    fn a_in_expanded_todos_opens_add_todo_prompt_and_commits_to_store() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Todos;
            form.todos_collapsed = false;
        }

        // `a` opens the add-todo prompt overlaid on the form.
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(matches!(
            app.tasks_screen.prompt,
            Some(TasksPrompt::AddTodo { .. })
        ));
        assert!(app.tasks_screen.form.is_some(), "form must stay open");

        // Type body, commit, prompt closes, todo lands on the task,
        // and the form's cursor lands on the new todo.
        for c in "first todo".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.prompt.is_none());
        let todos = &app.tasks.get(task_id).unwrap().todos;
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].body, "first todo");
        assert_eq!(todos[0].status, crate::tasks::TodoStatus::Pending);
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().todos_cursor, 0);
    }

    #[test]
    fn d_then_y_in_expanded_todos_removes_selected_todo() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        let id_a = app
            .tasks
            .add_todo(task_id, "keep".into(), "seed".into())
            .unwrap()
            .id;
        let id_b = app
            .tasks
            .add_todo(task_id, "doomed".into(), "seed".into())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Todos;
            form.todos_collapsed = false;
            // Aim at the second todo (the doomed one).
            form.todos_cursor = 1;
        }
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(matches!(
            app.tasks_screen.prompt,
            Some(TasksPrompt::ConfirmDeleteTodo { .. })
        ));

        app.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);
        assert!(app.tasks_screen.prompt.is_none());
        let todos = &app.tasks.get(task_id).unwrap().todos;
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].id, id_a);
        assert_ne!(todos[0].id, id_b);
        // Cursor must clamp back into range after the delete.
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().todos_cursor, 0);
        // Form stays open.
        assert!(app.tasks_screen.form.is_some());
    }

    #[test]
    fn d_then_n_in_expanded_todos_keeps_todo() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.tasks
            .add_todo(task_id, "stay".into(), "seed".into())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Todos;
            form.todos_collapsed = false;
        }
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        app.handle_key(KeyCode::Char('n'), KeyModifiers::NONE);
        assert!(app.tasks_screen.prompt.is_none());
        assert_eq!(app.tasks.get(task_id).unwrap().todos.len(), 1);
    }

    #[test]
    fn esc_cancels_add_todo_prompt_without_creating_todo() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Todos;
            form.todos_collapsed = false;
        }
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        for c in "scratch".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Esc, KeyModifiers::NONE);
        assert!(app.tasks_screen.prompt.is_none());
        assert!(app.tasks.get(task_id).unwrap().todos.is_empty());
        assert!(app.tasks_screen.form.is_some());
    }

    #[test]
    fn a_in_expanded_comments_opens_add_comment_prompt() {
        let mut app = App::new(sample_config());
        app.tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Comments;
            form.comments_collapsed = false;
        }
        app.handle_key(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(matches!(
            app.tasks_screen.prompt,
            Some(TasksPrompt::AddComment { .. })
        ));

        // Type, commit; the comment should land on the live task and
        // the form should still be open.
        for c in "hello".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.prompt.is_none());
        assert!(app.tasks_screen.form.is_some());
        let comments = &app.tasks.all()[0].comments;
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].body, "hello");
    }

    #[test]
    fn e_in_expanded_comments_edits_selected_comment() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.tasks
            .add_comment(task_id, "seed".into(), "first".into())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Comments;
            form.comments_collapsed = false;
            form.comments_cursor = 0;
        }
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        assert!(matches!(
            app.tasks_screen.prompt,
            Some(TasksPrompt::EditComment { .. })
        ));

        // Replace the body wholesale.
        for _ in 0.."first".len() {
            app.handle_key(KeyCode::Backspace, KeyModifiers::NONE);
        }
        for c in "edited".chars() {
            app.handle_key(KeyCode::Char(c), KeyModifiers::NONE);
        }
        app.handle_key(KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.tasks_screen.prompt.is_none());
        assert_eq!(app.tasks.get(task_id).unwrap().comments[0].body, "edited");
    }

    #[test]
    fn d_then_y_in_expanded_comments_removes_selected_comment() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        app.tasks
            .add_comment(task_id, "seed".into(), "doomed".into())
            .unwrap();
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Comments;
            form.comments_collapsed = false;
            form.comments_cursor = 0;
        }
        app.handle_key(KeyCode::Char('d'), KeyModifiers::NONE);
        assert!(matches!(
            app.tasks_screen.prompt,
            Some(TasksPrompt::ConfirmDeleteComment { .. })
        ));
        app.handle_key(KeyCode::Char('y'), KeyModifiers::NONE);
        assert!(app.tasks_screen.prompt.is_none());
        assert!(app.tasks.get(task_id).unwrap().comments.is_empty());
        // Form must remain open after the comment delete.
        assert!(app.tasks_screen.form.is_some());
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
        // Pretend the UI drew the views list at (30, 5) 20×VIEW_ENTRIES+2.
        let h = VIEW_ENTRIES.len() as u16 + 2;
        app.layout.views_list_inner = Some(Rect::new(30, 5, 20, h));
        // Click the row for the "Tasks" entry (row_in_list computed
        // as `ev.row - rect.y`).
        let tasks_idx = VIEW_ENTRIES.iter().position(|n| *n == "Tasks").unwrap();
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 31,
            row: 5 + tasks_idx as u16,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.active_view, ActiveView::Tasks);
        assert_eq!(app.focus, Focus::Tasks);
    }

    #[test]
    fn clicking_outside_views_entries_focuses_views_panel_only() {
        let mut app = App::new(sample_config());
        // Give the panel room for one more line beyond the entries.
        let h = VIEW_ENTRIES.len() as u16 + 3;
        app.layout.views_list_inner = Some(Rect::new(30, 5, 20, h));
        // Row immediately after the last entry — inside the panel
        // but not on any view entry row.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 31,
            row: 5 + VIEW_ENTRIES.len() as u16,
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
    fn first_click_on_todo_row_selects_second_click_toggles() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        let todo_id = app
            .tasks
            .add_todo(task_id, "do thing".into(), "seed".into())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        // Simulate the renderer expanding the section + emitting a
        // hit rect for the row.
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            form.focused_field = FormField::Title;
            form.todos_collapsed = false;
            form.todos_cursor = 0;
            form.todos_scroll = 0;
        }
        let row_rect = Rect::new(10, 30, 40, 1);
        app.layout.task_form_todo_rows = vec![(todo_id, row_rect)];

        // First click: focus + select; status unchanged.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 12,
            row: 30,
            modifiers: KeyModifiers::NONE,
        });
        let form = app.tasks_screen.form.as_ref().unwrap();
        assert_eq!(form.focused_field, FormField::Todos);
        assert_eq!(form.todos_cursor, 0);
        assert_eq!(
            app.tasks.get(task_id).unwrap().todos[0].status,
            crate::tasks::TodoStatus::Pending,
            "first click must not toggle"
        );

        // Second click on the same row: now toggles.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 14,
            row: 30,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            app.tasks.get(task_id).unwrap().todos[0].status,
            crate::tasks::TodoStatus::Completed
        );

        // Third click toggles back.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 14,
            row: 30,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(
            app.tasks.get(task_id).unwrap().todos[0].status,
            crate::tasks::TodoStatus::Pending
        );
    }

    #[test]
    fn click_on_different_todo_row_only_moves_cursor() {
        let mut app = App::new(sample_config());
        let task_id = app
            .tasks
            .add("t".into(), None, None, "seed".into(), Vec::new())
            .unwrap()
            .id;
        let id_a = app
            .tasks
            .add_todo(task_id, "a".into(), "seed".into())
            .unwrap()
            .id;
        let id_b = app
            .tasks
            .add_todo(task_id, "b".into(), "seed".into())
            .unwrap()
            .id;
        app.active_view = ActiveView::Tasks;
        app.focus = Focus::Tasks;
        let rows = TasksScreen::rows(&app.tasks);
        app.tasks_screen.reclamp(&rows);
        app.handle_key(KeyCode::Char('e'), KeyModifiers::NONE);
        {
            let form = app.tasks_screen.form.as_mut().unwrap();
            // Pretend the user already selected todo `a`.
            form.focused_field = FormField::Todos;
            form.todos_collapsed = false;
            form.todos_cursor = 0;
        }
        app.layout.task_form_todo_rows = vec![
            (id_a, Rect::new(10, 30, 40, 1)),
            (id_b, Rect::new(10, 31, 40, 1)),
        ];

        // Click the OTHER row: only moves cursor, no toggle.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 14,
            row: 31,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.tasks_screen.form.as_ref().unwrap().todos_cursor, 1);
        assert_eq!(
            app.tasks.get(task_id).unwrap().todos[0].status,
            crate::tasks::TodoStatus::Pending
        );
        assert_eq!(
            app.tasks.get(task_id).unwrap().todos[1].status,
            crate::tasks::TodoStatus::Pending
        );
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
        // Row 5 is the `(ungrouped)` header — inert. The first
        // clickable agent row sits one line below at row 6.
        app.handle_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 61,
            row: 6,
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
        let past = (chrono::Utc::now() - chrono::Duration::seconds(1)).naive_utc();
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

        let past = (chrono::Utc::now() - chrono::Duration::seconds(1)).naive_utc();
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
            blocks
                .iter()
                .any(|b| matches!(b, ChatBlock::UserText { .. })),
            "expected a user block on target chat, got: {blocks:?}"
        );
        assert!(
            blocks.iter().any(|b| matches!(b, ChatBlock::System { .. })),
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
            request: cron_add_request(
                sender,
                "coder#deadbeef",
                "message",
                "ping",
                "0 9 * * *",
                false,
            ),
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
        let past = (chrono::Utc::now() - chrono::Duration::seconds(1)).naive_utc();
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
                    schedule: Some(crate::cron::CronSchedule::OneOff { run_at_local: past }),
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

    #[test]
    fn sleep_command_marks_chat_asleep_and_signals_cancel() {
        use super::super::commands::CommandOutcome;
        let mut app = fresh_app();
        // Wire a cancel channel so we can assert the signal was sent.
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);
        app.chats[0].cancel_tx = Some(cancel_tx);
        assert!(!app.chats[0].asleep);
        app.apply_command_for(0, CommandOutcome::Sleep);
        assert!(app.chats[0].asleep, "sleep should set asleep=true");
        assert_eq!(app.chats[0].status, AgentStatus::Asleep);
        // try_recv: the cancel signal must be queued.
        assert!(cancel_rx.try_recv().is_ok(), "stop signal not delivered");
    }

    #[test]
    fn awake_command_clears_asleep() {
        use super::super::commands::CommandOutcome;
        let mut app = fresh_app();
        app.chats[0].asleep = true;
        app.chats[0].status = AgentStatus::Asleep;
        app.apply_command_for(0, CommandOutcome::Awake);
        assert!(!app.chats[0].asleep);
        assert_eq!(app.chats[0].status, AgentStatus::Ready);
    }

    #[test]
    fn awake_on_non_asleep_chat_only_emits_notice() {
        use super::super::commands::CommandOutcome;
        let mut app = fresh_app();
        let blocks_before = app.chats[0].blocks.len();
        app.apply_command_for(0, CommandOutcome::Awake);
        assert!(!app.chats[0].asleep);
        // One info block appended; status untouched.
        assert_eq!(app.chats[0].blocks.len(), blocks_before + 1);
    }

    #[test]
    fn shutdown_with_all_idle_sleeps_all_and_triggers_quit() {
        use super::super::commands::CommandOutcome;
        let mut app = fresh_app();
        for c in &app.chats {
            assert_ne!(c.status, AgentStatus::Working);
        }
        app.apply_command_for(0, CommandOutcome::Shutdown);
        assert!(app.shutting_down, "shutdown latch should be set");
        for (i, c) in app.chats.iter().enumerate() {
            assert!(c.asleep, "chat {i} should be asleep");
            assert_eq!(c.status, AgentStatus::Asleep, "chat {i} status");
        }
        // Event loop runs this once per iteration — first tick with
        // no busy agents must raise should_quit.
        app.check_shutdown_progress();
        assert!(app.should_quit, "should_quit after all agents idle");
    }

    #[test]
    fn shutdown_waits_for_busy_agent_then_sleeps_and_quits() {
        use super::super::commands::CommandOutcome;
        let mut app = fresh_app();
        // Pretend chat 1 is mid-turn.
        app.chats[1].status = AgentStatus::Working;
        app.apply_command_for(0, CommandOutcome::Shutdown);
        assert!(app.shutting_down);
        // Idle peers are already asleep; the busy one isn't.
        assert!(app.chats[0].asleep);
        assert!(!app.chats[1].asleep);
        assert_eq!(app.chats[1].status, AgentStatus::Working);
        // Progress tick while the turn is still running: no quit yet.
        app.check_shutdown_progress();
        assert!(!app.should_quit);
        // Turn completes — run_core's TurnResult path transitions
        // Working -> Ready before the next event-loop iteration.
        app.chats[1].status = AgentStatus::Ready;
        app.check_shutdown_progress();
        assert!(app.chats[1].asleep);
        assert_eq!(app.chats[1].status, AgentStatus::Asleep);
        assert!(app.should_quit);
    }

    #[test]
    fn shutdown_called_twice_is_a_noop_on_second_call() {
        use super::super::commands::CommandOutcome;
        let mut app = fresh_app();
        app.apply_command_for(0, CommandOutcome::Shutdown);
        assert!(app.shutting_down);
        let blocks_after_first = app.chats[0].blocks.len();
        app.apply_command_for(0, CommandOutcome::Shutdown);
        // Second call only appends the "already shutting down" note.
        assert_eq!(app.chats[0].blocks.len(), blocks_after_first + 1);
    }

    #[tokio::test]
    async fn send_message_during_shutdown_returns_shutting_down_error() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        app.shutting_down = true;
        let sender = app.chats[0].session_id.unwrap();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::SendMessage {
                    to: "b".into(),
                    body: "hi".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok);
        assert_eq!(resp.error.as_deref(), Some("aciddog is shutting down"));
    }

    #[test]
    fn tick_cron_is_noop_while_shutting_down() {
        let mut app = fresh_app();
        // Install a due one-off so tick_cron would otherwise fire and
        // remove it. Scheduled 1s in the past so it's due right now.
        let target = app.chats[0].display_name.clone();
        let past = chrono::Utc::now() - chrono::Duration::seconds(1);
        app.cron
            .add(
                target,
                crate::cron::CronPayload::Message { body: "hi".into() },
                crate::cron::CronSchedule::OneOff {
                    run_at_local: past.naive_utc(),
                },
                crate::cron::CronTz::Utc,
                false,
                true,
                "tester".into(),
            )
            .expect("cron add");
        let len_before = app.cron.len();
        app.shutting_down = true;
        app.tick_cron(chrono::Utc::now());
        // One-offs fire-and-remove on tick; the early return leaves
        // the job untouched.
        assert_eq!(app.cron.len(), len_before);
    }

    #[tokio::test]
    async fn send_message_to_asleep_agent_returns_error() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        // Mark recipient (chat 1, "b") asleep.
        app.chats[1].asleep = true;
        let sender = app.chats[0].session_id.unwrap();
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender,
                cmd: RequestCmd::SendMessage {
                    to: "b".into(),
                    body: "ping".into(),
                },
            },
            reply: tx,
        });
        let resp = rx.await.unwrap();
        assert!(!resp.ok, "asleep recipient must reject send");
        assert!(
            resp.error.as_deref().unwrap_or("").contains("asleep"),
            "error should mention asleep, got {:?}",
            resp.error
        );
    }

    #[tokio::test]
    async fn channel_broadcast_silently_skips_asleep_member() {
        let mut app = App::new(sample_config_with_channel());
        let mut rxs = wire_up_channels(&mut app);
        // Mark "b" (chat 1) asleep.
        app.chats[1].asleep = true;

        let (reply_tx, reply_rx) = oneshot::channel();
        app.broadcast_to_channel(
            ChannelSender::Agent("a".into()),
            "room".into(),
            "hi team".into(),
            reply_tx,
        );
        assert!(
            reply_rx.await.unwrap().ok,
            "broadcast should succeed even when a member is asleep"
        );

        // "c" (chat 2) is awake and must receive. Use async recv so
        // the spawned send task gets a chance to run.
        let _ = rxs[2]
            .recv()
            .await
            .expect("awake member receives broadcast");
        // "b" (chat 1) is asleep — never enqueued, so the channel is
        // empty even after letting the runtime tick.
        assert!(
            rxs[1].try_recv().is_err(),
            "asleep agent should not receive channel broadcast"
        );
    }

    #[tokio::test]
    async fn local_text_input_to_asleep_agent_is_rejected() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let (utx, mut urx) = mpsc::unbounded_channel::<String>();
        app.chats[0].user_tx = Some(utx);
        app.chats[0].asleep = true;
        app.handle_local_input(0, "hello".into());
        // No prompt should reach the agent's user channel.
        assert!(
            urx.try_recv().is_err(),
            "asleep agent must not receive user prompt"
        );
        // Last block should be the system warn explaining why.
        match last_block(&app.chats[0]) {
            ChatBlock::System { text, kind } => {
                assert!(text.contains("asleep"));
                assert!(matches!(kind, super::super::blocks::SystemKind::Warn));
            }
            other => panic!("expected System Warn block, got {other:?}"),
        }
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

    // ── supervisor / sysadmin admin surface ─────────────────────────

    fn admin_send(app: &mut App, sender_uuid: Uuid, cmd: RequestCmd) -> Response {
        let (tx, rx) = oneshot::channel();
        app.handle_agent_socket_event(AgentSocketEvent {
            request: crate::agent_ipc::Request {
                agent_id: sender_uuid,
                cmd,
            },
            reply: tx,
        });
        futures::executor::block_on(rx).expect("reply dropped")
    }

    #[tokio::test]
    async fn agent_list_rejects_non_supervisor() {
        let mut app = App::new(sample_config());
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(&mut app, sender, RequestCmd::AgentList);
        assert!(!resp.ok);
        assert_eq!(resp.error.unwrap(), "not a supervisor");
    }

    #[tokio::test]
    async fn agent_list_returns_table_for_supervisor() {
        let mut cfg = sample_config();
        cfg.agents[0].supervisor = true;
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(&mut app, sender, RequestCmd::AgentList);
        assert!(resp.ok, "err={:?}", resp.error);
        let msg = resp.message.unwrap();
        assert!(msg.contains("id\tkind\tstatus"), "header missing: {msg}");
        assert!(msg.contains("\na\t"), "agent a row missing: {msg}");
        assert!(msg.contains("\nb\t"), "agent b row missing: {msg}");
        assert!(msg.contains("\nc\t"), "agent c row missing: {msg}");
    }

    #[tokio::test]
    async fn agent_stop_aborts_target_turn() {
        let mut cfg = sample_config();
        cfg.agents[0].supervisor = true;
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        // Wire a cancel_tx on the target so stop_turn has something
        // to fire.
        let (cancel_tx, mut cancel_rx) = mpsc::channel::<()>(1);
        app.chats[1].cancel_tx = Some(cancel_tx);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(
            &mut app,
            sender,
            RequestCmd::AgentStop {
                agent: "b".into(),
                reason: Some("boom".into()),
            },
        );
        assert!(resp.ok, "err={:?}", resp.error);
        assert!(cancel_rx.try_recv().is_ok(), "cancel signal not sent");
        let saw_reason = app.chats[1].blocks.iter().any(|b| match b {
            ChatBlock::System { text, .. } => text.contains("boom"),
            _ => false,
        });
        assert!(saw_reason, "reason block missing from target pane");
    }

    #[tokio::test]
    async fn agent_send_as_user_delivers_unwrapped_to_user_tx() {
        let mut cfg = sample_config();
        cfg.agents[0].supervisor = true;
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        // Replace target's user_tx with one the test can observe.
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        app.chats[1].user_tx = Some(tx);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(
            &mut app,
            sender,
            RequestCmd::AgentSendAsUser {
                to: "b".into(),
                body: "hello from human".into(),
            },
        );
        assert!(resp.ok, "err={:?}", resp.error);
        let delivered = rx.try_recv().expect("body delivered");
        assert_eq!(delivered, "hello from human");
        assert!(!delivered.contains("<Message"), "body must not be wrapped");
    }

    #[tokio::test]
    async fn agent_history_selectors_return_expected_subsets() {
        let mut cfg = sample_config();
        cfg.agents[0].supervisor = true;
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        // Seed target pane with a mix of blocks.
        app.chats[1].blocks.clear();
        app.chats[1].blocks.push(ChatBlock::user("first prompt"));
        app.chats[1]
            .blocks
            .push(ChatBlock::ai_markdown("first reply"));
        app.chats[1].blocks.push(ChatBlock::user("second prompt"));
        app.chats[1]
            .blocks
            .push(ChatBlock::ai_markdown("second reply"));

        let sender = app.chats[0].session_id.unwrap();

        let all = admin_send(
            &mut app,
            sender,
            RequestCmd::AgentHistory {
                agent: "b".into(),
                selector: HistorySelector::All,
            },
        );
        let all_msg = all.message.unwrap();
        assert_eq!(all_msg.lines().count(), 4, "{all_msg}");

        let last = admin_send(
            &mut app,
            sender,
            RequestCmd::AgentHistory {
                agent: "b".into(),
                selector: HistorySelector::Last { n: 2 },
            },
        );
        let last_msg = last.message.unwrap();
        assert_eq!(last_msg.lines().count(), 2);
        assert!(last_msg.contains("second reply"), "{last_msg}");

        let last_msg_only = admin_send(
            &mut app,
            sender,
            RequestCmd::AgentHistory {
                agent: "b".into(),
                selector: HistorySelector::LastMessage,
            },
        );
        let lm = last_msg_only.message.unwrap();
        assert_eq!(lm.lines().count(), 1);
        assert!(lm.starts_with("ai:"), "{lm}");
        assert!(lm.contains("second reply"), "{lm}");

        let user_kind = admin_send(
            &mut app,
            sender,
            RequestCmd::AgentHistory {
                agent: "b".into(),
                selector: HistorySelector::Kind {
                    kind: "user".into(),
                },
            },
        );
        let uk = user_kind.message.unwrap();
        assert_eq!(uk.lines().count(), 2);
        assert!(uk.contains("first prompt"), "{uk}");
        assert!(uk.contains("second prompt"), "{uk}");

        let bogus = admin_send(
            &mut app,
            sender,
            RequestCmd::AgentHistory {
                agent: "b".into(),
                selector: HistorySelector::Kind {
                    kind: "nonsense".into(),
                },
            },
        );
        assert!(!bogus.ok);
    }

    #[tokio::test]
    async fn supervisor_bypasses_can_message_allowlist() {
        let mut cfg = sample_config();
        cfg.agents[0].supervisor = true;
        // Empty allowlist would normally block all outbound sends.
        cfg.agents[0].can_message = Some(vec![]);
        let mut app = App::new(cfg);
        let rxs = wire_up_agent_ids_and_channels(&mut app);
        drop(rxs);
        let mut new_rxs = Vec::new();
        // Rewire b's agent_tx so we can observe delivery.
        let (tx, rx) = mpsc::channel::<String>(16);
        app.chats[1].agent_tx = Some(tx);
        new_rxs.push(rx);

        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(
            &mut app,
            sender,
            RequestCmd::SendMessage {
                to: "b".into(),
                body: "hey".into(),
            },
        );
        assert!(resp.ok, "err={:?}", resp.error);
        let delivered = new_rxs[0].recv().await.expect("delivered");
        assert_eq!(delivered, r#"<Message from="a">hey</Message>"#);
    }

    #[tokio::test]
    async fn non_supervisor_still_honours_can_message_allowlist() {
        let mut cfg = sample_config();
        cfg.agents[0].can_message = Some(vec![]);
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(
            &mut app,
            sender,
            RequestCmd::SendMessage {
                to: "b".into(),
                body: "hey".into(),
            },
        );
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("not permitted"));
    }

    #[tokio::test]
    async fn server_shutdown_requires_sysadmin() {
        let mut cfg = sample_config();
        cfg.agents[0].supervisor = true; // supervisor only, not sysadmin
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(&mut app, sender, RequestCmd::ServerShutdown);
        assert!(!resp.ok);
        assert_eq!(resp.error.unwrap(), "not a sysadmin");
        assert!(!app.shutting_down);
    }

    #[tokio::test]
    async fn sysadmin_shutdown_begins_graceful_shutdown() {
        let mut cfg = sample_config();
        cfg.agents[0].sysadmin = true;
        cfg.agents[0].supervisor = true; // sysadmin implies supervisor
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(&mut app, sender, RequestCmd::ServerShutdown);
        assert!(resp.ok, "err={:?}", resp.error);
        assert!(app.shutting_down);
    }

    #[tokio::test]
    async fn sysadmin_exit_sets_should_quit() {
        let mut cfg = sample_config();
        cfg.agents[0].sysadmin = true;
        cfg.agents[0].supervisor = true;
        let mut app = App::new(cfg);
        wire_up_agent_ids_and_channels(&mut app);
        let sender = app.chats[0].session_id.unwrap();
        let resp = admin_send(&mut app, sender, RequestCmd::ServerExit);
        assert!(resp.ok, "err={:?}", resp.error);
        assert!(app.should_quit);
    }
}
